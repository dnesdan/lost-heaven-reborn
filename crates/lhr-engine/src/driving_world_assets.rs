//! Owned original-data static world for driving. This host composition shares
//! one checked source frame graph between KLZ binding and first-LOD rendering.
//! Missing source geometry remains an explicit collision candidate error.
use crate::{
    collision_links::{resolve_collision_links, CollisionLinkSlot, CollisionLinkState},
    collision_world_driving::{DrivingCollisionWorld, SceneCollisionGeometry},
    frame_fast_lookup::{FastFrameStorage, I3dFastFrameTree, OriginalFrameName},
    frame_transform::{FrameKey, FrameKind, I3dFrameGraph, I3dFrameState},
    native_transform::SQuat,
};
use lhr_assets::{Installation, Model};
use lhr_render::{Mesh, Scene, Texture, Vertex};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const MAX_SOURCE_BYTES: usize = 256 * 1024 * 1024;
const MAX_RENDER_BYTES: usize = 256 * 1024 * 1024;
const MAX_FRAMES: usize = 65536;
const MAX_DRAWS: usize = 16384;
static NEXT_WORLD_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// Host ownership domain for one durable physics/source graph. It is unrelated
/// to original frame addresses and survives moves of this owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrivingWorldIdentity(u64);
impl DrivingWorldIdentity {
    pub(crate) fn fresh() -> Result<Self, String> {
        NEXT_WORLD_ID
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .map(Self)
            .map_err(|_| "Driving world identity exhausted".into())
    }
}
/// Host symbolic identity range, distinct from canonical frame-memory regions.
/// Values in this range are resolved by the world owner, never dereferenced.
pub const DRIVING_WORLD_FRAME_BASE: u32 = 0xd000_0000;
pub const DRIVING_WORLD_FRAME_END: u32 = 0xd100_0000;

#[derive(Clone, Debug)]
pub struct DrivingWorldSpawn {
    pub position: [f32; 3],
    pub rotation_wxyz: [f32; 4],
    pub scale: [f32; 3],
    pub actor_name: String,
    pub model: String,
}
#[derive(Clone, Debug, Default)]
pub struct DrivingWorldAssetSummary {
    pub source_frames: usize,
    pub resolved_links: usize,
    pub total_links: usize,
    pub unavailable_primitives: usize,
    pub null_volume_owners: usize,
    /// Unsupported external source resources remain visible to callers. Their
    /// KLZ candidates are retained as unavailable whenever vertices are needed.
    pub unsupported_resources: Vec<(String, String)>,
}
struct GeometryOwner {
    source: Arc<Model>,
    keys: Vec<FrameKey>,
    local_vertices: Vec<Option<Vec<[f32; 3]>>>,
}
struct WorldModelSource {
    name: Option<Vec<u8>>,
    resource: String,
}
/// Borrowed original source fields for a frame in this world's own graph.
/// Frame keys remain separate from canonical car memory addresses.
pub struct DrivingWorldFrameSource<'a> {
    pub kind: FrameKind,
    pub parent: Option<FrameKey>,
    pub local_scale: [f32; 3],
    pub name_bytes: Option<&'a [u8]>,
    pub properties: Option<&'a [u8]>,
    pub model_resource: Option<&'a str>,
}
/// Durable host selection identity from the parsed cache part/object tables.
/// This is not a native frame pointer or a reconstructed streaming-controller ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CachePlacementId {
    pub part: usize,
    pub object: usize,
}
/// Already-admitted cache source and the exact old physics graph correspondence.
/// Canonical rendering must materialize its own actual arena/scene owners from
/// this record; none of the old render snapshot's meshes are canonical draws.
#[derive(Clone, Debug)]
pub struct AdmittedCityCacheInstance {
    pub id: CachePlacementId,
    pub resource: String,
    pub source: Arc<Model>,
    pub position: [f32; 3],
    pub rotation: SQuat,
    pub root_scale: [f32; 3],
    pub secondary_scale: [f32; 3],
    /// Retained uninterpreted cache flags; actual On synchronization reads the
    /// existing physics graph rather than assigning guessed flag meanings.
    pub opaque_flags: u32,
    pub world_root: FrameKey,
    /// File-order source frame keys, independent of model membership order.
    pub world_frames: Vec<FrameKey>,
}
/// Static frame/source ownership is distinct from the car's live ModelArena.
/// It is never seeded from a native process. Render snapshots can be moved out
/// while this owner and its independently owned collision candidates remain.
pub struct DrivingWorldAssets {
    identity: DrivingWorldIdentity,
    collision: DrivingCollisionWorld,
    render_scene: Scene,
    render_mesh_frames: Vec<FrameKey>,
    render_revision: u64,
    graph: I3dFrameGraph,
    sources: Vec<GeometryOwner>,
    model_roots: BTreeMap<u32, WorldModelSource>,
    metadata: lhr_assets::Scene,
    summary: DrivingWorldAssetSummary,
    dynamic_grid_source: crate::dynamic_collision_manager::DynamicCollisionGridSource,
    mission: String,
    primary: FrameKey,
    cache: Option<lhr_assets::CityCache>,
    cache_failed: bool,
    cache_loaded: BTreeSet<(usize, usize)>,
    cache_models: BTreeMap<String, Arc<Model>>,
    admitted_cache_instances: Vec<AdmittedCityCacheInstance>,
    cache_unsupported: BTreeMap<String, String>,
    source_bytes: usize,
    render_textures: BTreeMap<(String, [bool; 2]), Arc<Texture>>,
    render_bytes: usize,
}
fn charge(total: &mut usize, bytes: usize, limit: usize) -> Result<(), String> {
    *total = total
        .checked_add(bytes)
        .filter(|n| *n <= limit)
        .ok_or("Driving world storage quota exceeded")?;
    Ok(())
}
fn resource_model(name: &str) -> Result<String, String> {
    let lower = name.to_ascii_lowercase().replace('\\', "/");
    let basename = lower.strip_prefix("models/").unwrap_or(&lower);
    let stem = basename
        .strip_suffix(".i3d")
        .or_else(|| basename.strip_suffix(".4ds"))
        .ok_or("Original model resource extension is unavailable")?;
    if stem.is_empty()
        || stem.len() > 250
        || stem
            .bytes()
            .any(|b| b < 32 || matches!(b, b'/' | b'\\' | b':' | b'.'))
    {
        return Err("Invalid original model resource basename".into());
    }
    Ok(format!("models/{stem}.4ds"))
}
fn rewritten_cache_resource(name: &str) -> &str {
    // Exact existing original cache-loader rewrite, PC401a67..401a8e.
    if name.eq_ignore_ascii_case("0rek23.i3d") {
        "0rek17.i3d"
    } else {
        name
    }
}
/// Basename accepted by the same actual MissionCaches Models directory owner.
pub fn normalized_cache_model_resource(name: &str) -> Result<String, String> {
    let path = resource_model(rewritten_cache_resource(name))?;
    let stem = path
        .strip_prefix("models/")
        .unwrap()
        .strip_suffix(".4ds")
        .unwrap();
    Ok(format!("{stem}.i3d"))
}
fn frame_kind(kind: u8) -> Result<FrameKind, String> {
    Ok(match kind {
        1 => FrameKind::Mesh,
        2 => FrameKind::Light,
        5 => FrameKind::Sector,
        6 => FrameKind::Dummy,
        7 => FrameKind::Target,
        9 => FrameKind::Model,
        10 => FrameKind::Joint,
        _ => return Err(format!("Unsupported collision frame type {kind}")),
    })
}
fn import_frame(
    graph: &mut I3dFrameGraph,
    tree: &mut I3dFastFrameTree,
    state: I3dFrameState,
    name: Vec<u8>,
    original_name: OriginalFrameName,
    parent: Option<FrameKey>,
    kind: u32,
) -> Result<FrameKey, String> {
    let frame = graph
        .insert(state)
        .map_err(|e| format!("Driving frame construction: {e:?}"))?;
    if frame.0 as usize >= MAX_FRAMES {
        return Err("Driving world frame quota exceeded".into());
    }
    graph
        .link_to(frame, parent)
        .map_err(|e| format!("Driving frame hierarchy: {e:?}"))?;
    tree.import_frame(FastFrameStorage {
        frame,
        name: Some(name),
        original_name,
        frame_type: kind,
        parent,
        sector_scene: (kind == 5).then_some(1),
        flags: 0,
        left: None,
        right: None,
    })
    .map_err(|e| format!("Driving frame names: {e:?}"))?;
    tree.add_to_ff_tree(frame)
        .map_err(|e| format!("Driving frame registration: {e:?}"))?;
    Ok(frame)
}
fn append_source(
    graph: &mut I3dFrameGraph,
    tree: &mut I3dFastFrameTree,
    source: Arc<Model>,
    parent: FrameKey,
    prefix: Option<&[u8]>,
) -> Result<GeometryOwner, String> {
    let mut keys = Vec::new();
    let mut local_vertices = Vec::new();
    for frame in &source.frames {
        let mut state = I3dFrameState::source_mesh(
            frame.position,
            frame.scale,
            SQuat {
                wxyz: frame.rotation_wxyz,
            },
        );
        state.kind = frame_kind(frame.kind)?;
        let parent = if let Some(index) = frame.parent {
            *keys
                .get(index)
                .ok_or("Driving source parent order is unavailable")?
        } else {
            parent
        };
        let (name, original_name) = if let Some(prefix) = prefix {
            let mut name = prefix.to_vec();
            name.push(b'.');
            name.extend_from_slice(&frame.name_bytes);
            (name, OriginalFrameName::Offset(prefix.len() + 1))
        } else {
            (frame.name_bytes.clone(), OriginalFrameName::Offset(0))
        };
        let key = import_frame(
            graph,
            tree,
            state,
            name,
            original_name,
            Some(parent),
            u32::from(frame.kind),
        )?;
        keys.push(key);
        let geometry = if let Some(instance) = frame.instance {
            source
                .frames
                .get(instance)
                .ok_or("Driving instance source is unavailable")?
        } else {
            frame
        };
        // Skin/morph vertices are not static collision geometry.
        let vertices = if frame.kind == 1 && matches!(geometry.visual, Some(0)) {
            geometry
                .lods
                .first()
                .map(|lod| lod.vertices.iter().map(|v| v.position).collect())
        } else {
            None
        };
        local_vertices.push(vertices);
    }
    Ok(GeometryOwner {
        source,
        keys,
        local_vertices,
    })
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DrivingCacheRenderSummary {
    pub selected: usize,
    pub loaded: usize,
    pub unsupported: usize,
    pub added_meshes: usize,
}
fn mesh_near(mesh: &Mesh, position: [f32; 3], radius: f32) -> bool {
    let center = [position[0], -position[2]];
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for vertex in &mesh.vertices {
        for (axis, component) in [0, 2].into_iter().enumerate() {
            min[axis] = min[axis].min(vertex.position[component]);
            max[axis] = max[axis].max(vertex.position[component]);
        }
    }
    (0..2).all(|axis| max[axis] >= center[axis] - radius && min[axis] <= center[axis] + radius)
}
fn presentation_bounds(position: [f32; 3], radius: f32) -> Result<(), String> {
    if position.iter().any(|v| !v.is_finite())
        || !radius.is_finite()
        || radius <= 0.
        || radius > 10000.
    {
        Err("Invalid driving presentation bounds".into())
    } else {
        Ok(())
    }
}
pub fn normalized_cache_rotation(mut rotation: SQuat) -> Result<SQuat, String> {
    // Guard actual used operands/intermediates, rather than rejecting harmless
    // small original quaternion components at an arbitrary magnitude cutoff.
    use crate::collision_swept::{add, div, mul, number, sqrt, sub};
    let [w, x, y, z] = rotation.wxyz;
    for value in [w, x, y, z] {
        number(value).map_err(|e| format!("Cache quaternion operand: {e:?}"))?;
    }
    let checked = (|| {
        let norm = add(add(add(mul(w, w)?, mul(x, x)?)?, mul(y, y)?)?, mul(z, z)?)?;
        if norm > 1e-8 && sub(norm, 1.)?.abs() >= 1e-8 {
            let inverse = div(1., sqrt(norm)?)?;
            for value in [x, y, z, w] {
                mul(value, inverse)?;
            }
        }
        Ok::<(), crate::collision_swept::SphereSweepError>(())
    })();
    checked.map_err(|e| format!("Cache quaternion arithmetic: {e:?}"))?;
    rotation.normalize();
    Ok(rotation)
}
fn append_cache_source(
    graph: &mut I3dFrameGraph,
    source: Arc<Model>,
    parent: FrameKey,
    secondary: [f32; 3],
) -> Result<GeometryOwner, String> {
    let mut keys = Vec::new();
    let mut local_vertices = Vec::new();
    for frame in &source.frames {
        let scale = if frame.kind == 1 {
            secondary
        } else {
            frame.scale
        };
        let mut state = I3dFrameState::source_mesh(
            frame.position,
            scale,
            SQuat {
                wxyz: frame.rotation_wxyz,
            },
        );
        state.kind = frame_kind(frame.kind)?;
        let parent = if let Some(index) = frame.parent {
            *keys
                .get(index)
                .ok_or("Cache source parent order is unavailable")?
        } else {
            parent
        };
        let key = graph
            .insert(state)
            .map_err(|e| format!("Cache source frame: {e:?}"))?;
        if key.0 as usize >= MAX_FRAMES {
            return Err("Driving world frame quota exceeded".into());
        }
        graph
            .link_to(key, Some(parent))
            .map_err(|e| format!("Cache source hierarchy: {e:?}"))?;
        keys.push(key);
        let geometry = if let Some(index) = frame.instance {
            source
                .frames
                .get(index)
                .ok_or("Cache source instance is unavailable")?
        } else {
            frame
        };
        local_vertices.push(if frame.kind == 1 && matches!(geometry.visual, Some(0)) {
            geometry
                .lods
                .first()
                .map(|lod| lod.vertices.iter().map(|v| v.position).collect())
        } else {
            None
        });
    }
    Ok(GeometryOwner {
        source,
        keys,
        local_vertices,
    })
}
impl DrivingWorldAssets {
    pub fn load(installation: &Installation, mission: &str) -> Result<Self, String> {
        if mission.is_empty()
            || mission.len() > 64
            || !mission
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err("Invalid mission resource name".into());
        }
        let mut source_bytes = 0;
        let mut read = |path: &str| -> Result<Vec<u8>, String> {
            let data = installation
                .read(path)
                .map_err(|e| format!("Driving world {path}: {e}"))?;
            charge(&mut source_bytes, data.len(), MAX_SOURCE_BYTES)?;
            Ok(data)
        };
        let klz_bytes = read(&format!("missions/{mission}/tree.klz"))?;
        let klz = lhr_assets::klz::Klz::parse(&klz_bytes).map_err(|e| e.to_string())?;
        let dynamic_grid_source = crate::dynamic_collision_manager::DynamicCollisionGridSource {
            min_xz: [klz.grid.min_x, klz.grid.min_z],
            max_xz: [klz.grid.max_x, klz.grid.max_z],
            desired_cell_size: f32::from_le_bytes(klz.grid.raw[0x20..0x24].try_into().unwrap()),
        };
        let scene = Arc::new(
            lhr_assets::parse_four_ds_scene(&read(&format!("missions/{mission}/scene.4ds"))?)
                .map_err(|e| e.to_string())?,
        );
        let metadata = lhr_assets::parse_scene2(&read(&format!("missions/{mission}/scene2.bin"))?)
            .map_err(|e| e.to_string())?;
        let mut graph = I3dFrameGraph::default();
        let mut tree = I3dFastFrameTree::new(true);
        let mut primary = I3dFrameState::model();
        primary.kind = FrameKind::Sector;
        let primary = import_frame(
            &mut graph,
            &mut tree,
            primary,
            b"Primary sector".to_vec(),
            OriginalFrameName::Offset(0),
            None,
            5,
        )?;
        let mut sources = vec![append_source(&mut graph, &mut tree, scene, primary, None)?];
        let mut model_roots = BTreeMap::new();
        let mut summary = DrivingWorldAssetSummary::default();
        let link_names: BTreeSet<_> = klz
            .links
            .iter()
            .map(|l| l.name.to_ascii_lowercase())
            .collect();
        let dynamic: BTreeSet<_> = metadata
            .actors
            .iter()
            .filter(|a| matches!(a.kind, Some(2 | 4)))
            .map(|a| a.name.to_ascii_lowercase())
            .collect();
        let mut models = BTreeMap::<String, Arc<Model>>::new();
        // Expand named source models only when an original KLZ slot depends on
        // that namespace. Actors remain owned by the gameplay runtime.
        for object in &metadata.objects {
            let Some(model) = object.model.as_deref() else {
                continue;
            };
            if dynamic.contains(&object.name.to_ascii_lowercase()) {
                continue;
            }
            let prefix = format!("{}.", object.name.to_ascii_lowercase());
            if !link_names.contains(&object.name.to_ascii_lowercase())
                && !link_names.iter().any(|name| name.starts_with(&prefix))
            {
                continue;
            }
            // These placements are admitted only for an untransformed primary
            // parent. Other hierarchy dependencies remain explicit diagnostics.
            if object.parent.as_deref() != Some("Primary sector")
                || object.uniform_scale.is_some_and(|s| s != 1.)
                || object.position != object.world_position
            {
                summary.unsupported_resources.push((
                    object.name.clone(),
                    "External transformed parent is unavailable".into(),
                ));
                continue;
            }
            let Some(position) = object.position else {
                summary.unsupported_resources.push((
                    object.name.clone(),
                    "External position is unavailable".into(),
                ));
                continue;
            };
            let Some(rotation) = object.rotation_wxyz else {
                summary.unsupported_resources.push((
                    object.name.clone(),
                    "External rotation is unavailable".into(),
                ));
                continue;
            };
            let Some(scale) = object.scale else {
                summary
                    .unsupported_resources
                    .push((object.name.clone(), "External scale is unavailable".into()));
                continue;
            };
            let path = resource_model(model)?;
            let source = if let Some(source) = models.get(&path) {
                source.clone()
            } else {
                match lhr_assets::parse_four_ds_scene(&read(&path)?) {
                    Ok(source) => {
                        let source = Arc::new(source);
                        models.insert(path.clone(), source.clone());
                        source
                    }
                    Err(error) => {
                        summary
                            .unsupported_resources
                            .push((path, error.to_string()));
                        continue;
                    }
                }
            };
            let mut state = I3dFrameState::source_mesh(position, scale, SQuat { wxyz: rotation });
            state.kind = FrameKind::Model;
            let root = import_frame(
                &mut graph,
                &mut tree,
                state,
                object.name.as_bytes().to_vec(),
                OriginalFrameName::Null,
                Some(primary),
                9,
            )?;
            model_roots.insert(
                root.0,
                WorldModelSource {
                    name: Some(object.name.as_bytes().to_vec()),
                    resource: model.to_owned(),
                },
            );
            sources.push(append_source(
                &mut graph,
                &mut tree,
                source,
                root,
                Some(object.name.as_bytes()),
            )?);
        }
        let mut links: Vec<_> = klz
            .links
            .iter()
            .map(|link| CollisionLinkSlot::unresolved(link.kind, link.name.as_bytes()))
            .collect();
        resolve_collision_links(&mut CollisionLinkState {
            scene: Some(1),
            links: &mut links,
            frames: &mut tree,
        })
        .map_err(|e| format!("Driving collision links: {e:?}"))?;
        summary.resolved_links = links.iter().filter(|l| l.frame().is_some()).count();
        summary.total_links = links.len();
        summary.source_frames = sources.iter().map(|s| s.keys.len()).sum();
        let geometry: Vec<_> = sources
            .iter()
            .flat_map(|owner| {
                owner
                    .keys
                    .iter()
                    .zip(&owner.local_vertices)
                    .map(|(frame, vertices)| SceneCollisionGeometry {
                        frame: *frame,
                        lod0_local_vertices: vertices.as_deref(),
                    })
            })
            .collect();
        let collision = DrivingCollisionWorld::from_resolved_local_geometry(
            &klz, &links, &geometry, &mut graph, false,
        )
        .map_err(|e| format!("Driving collision world: {e:?}"))?;
        summary.unavailable_primitives = collision
            .primitives()
            .iter()
            .filter(|p| p.unavailable.is_some())
            .count();
        summary.null_volume_owners = collision
            .primitives()
            .iter()
            .filter(|p| p.owner_unavailable.is_some() && p.unavailable.is_none())
            .count();
        let mut render_textures = BTreeMap::new();
        let mut render_bytes = 0;
        let mut render_mesh_frames = Vec::new();
        let render_scene = render_sources(
            installation,
            &sources,
            &mut graph,
            &mut render_textures,
            &mut render_bytes,
            &mut render_mesh_frames,
        )?;
        Ok(Self {
            identity: DrivingWorldIdentity::fresh()?,
            collision,
            render_scene,
            render_mesh_frames,
            render_revision: 0,
            graph,
            sources,
            model_roots,
            metadata,
            summary,
            dynamic_grid_source,
            mission: mission.to_owned(),
            primary,
            cache: None,
            cache_failed: false,
            cache_loaded: BTreeSet::new(),
            cache_models: BTreeMap::new(),
            admitted_cache_instances: Vec::new(),
            cache_unsupported: BTreeMap::new(),
            source_bytes,
            render_textures,
            render_bytes,
        })
    }
    /// Bounded original cache placement intake for presentation. It preserves
    /// placement roots, source hierarchy, geometry sharing and original visual
    /// scale replacement. Native pooling, PVS and streaming are not simulated.
    pub fn load_cache_render_near(
        &mut self,
        installation: &Installation,
        position: [f32; 3],
        radius: f32,
    ) -> Result<DrivingCacheRenderSummary, String> {
        presentation_bounds(position, radius)?;
        if self.cache_failed {
            return Err("Driving cache intake was interrupted; retained source ownership requires reloading the world".into());
        }
        // Source/frame ownership and resource budgets advance before rendering
        // finishes. Preserve that reached prefix on failure and prevent a retry
        // from treating already marked placements as complete rendered draws.
        self.cache_failed = true;
        let result = self.load_cache_render_near_inner(installation, position, radius);
        if result.is_ok() {
            self.cache_failed = false;
        }
        result
    }
    fn load_cache_render_near_inner(
        &mut self,
        installation: &Installation,
        position: [f32; 3],
        radius: f32,
    ) -> Result<DrivingCacheRenderSummary, String> {
        if self.cache.is_none() {
            let bytes = installation
                .read(&format!("missions/{}/cache.bin", self.mission))
                .map_err(|e| format!("Driving cache source: {e}"))?;
            charge(&mut self.source_bytes, bytes.len(), MAX_SOURCE_BYTES)?;
            self.cache = Some(lhr_assets::parse_city_cache(&bytes).map_err(|e| e.to_string())?);
        }
        let mut selected = Vec::new();
        for (part, source) in self.cache.as_ref().unwrap().parts.iter().enumerate() {
            for (index, object) in source.objects.iter().enumerate() {
                let dx = f64::from(object.position[0]) - f64::from(position[0]);
                let dz = f64::from(object.position[2]) - f64::from(position[2]);
                if dx * dx + dz * dz <= f64::from(radius) * f64::from(radius)
                    && !self.cache_loaded.contains(&(part, index))
                {
                    selected.push((part, index, object.clone()));
                }
            }
        }
        let mut summary = DrivingCacheRenderSummary {
            selected: selected.len(),
            ..Default::default()
        };
        let begin = self.sources.len();
        for (part, index, object) in selected {
            // Exact original cache-loader filename rewrite, PC401a67..401a8e.
            let resource = rewritten_cache_resource(&object.model);
            let path = resource_model(resource)?;
            if self.cache_unsupported.contains_key(&path) {
                summary.unsupported += 1;
                self.cache_loaded.insert((part, index));
                continue;
            }
            let source = if let Some(source) = self.cache_models.get(&path) {
                source.clone()
            } else {
                let bytes = installation
                    .read(&path)
                    .map_err(|e| format!("Driving cache model {path}: {e}"))?;
                charge(&mut self.source_bytes, bytes.len(), MAX_SOURCE_BYTES)?;
                let source = match lhr_assets::parse_four_ds_scene(&bytes) {
                    Ok(source) => source,
                    Err(error) if error.is_unsupported_billboard() => {
                        self.summary
                            .unsupported_resources
                            .push((path.clone(), error.to_string()));
                        self.cache_unsupported.insert(path, error.to_string());
                        summary.unsupported += 1;
                        self.cache_loaded.insert((part, index));
                        continue;
                    }
                    Err(error) => return Err(format!("Driving cache model {path}: {error}")),
                };
                for frame in &source.frames {
                    frame_kind(frame.kind)?;
                }
                let source = Arc::new(source);
                self.cache_models.insert(path.clone(), source.clone());
                source
            };
            let rotation=normalized_cache_rotation(SQuat{wxyz:object.rotation_wxyz}).map_err(|error|format!("{error}; part={part} placement={index} model={resource:?} position={:?} quaternion={:?}",object.position,object.rotation_wxyz))?;
            let mut state = I3dFrameState::source_mesh(object.position, object.scale, rotation);
            state.kind = FrameKind::Model;
            let root = self
                .graph
                .insert(state)
                .map_err(|e| format!("Cache placement root: {e:?}"))?;
            self.graph
                .link_to(root, Some(self.primary))
                .map_err(|e| format!("Cache placement hierarchy: {e:?}"))?;
            self.model_roots.insert(
                root.0,
                WorldModelSource {
                    name: None,
                    resource: resource.to_owned(),
                },
            );
            let owner = append_cache_source(&mut self.graph, source, root, object.secondary_scale)?;
            self.admitted_cache_instances
                .push(AdmittedCityCacheInstance {
                    id: CachePlacementId {
                        part,
                        object: index,
                    },
                    resource: normalized_cache_model_resource(&object.model)?,
                    source: owner.source.clone(),
                    position: object.position,
                    rotation,
                    root_scale: object.scale,
                    secondary_scale: object.secondary_scale,
                    opaque_flags: object.flags,
                    world_root: root,
                    world_frames: owner.keys.clone(),
                });
            self.sources.push(owner);
            self.cache_loaded.insert((part, index));
            summary.loaded += 1;
        }
        let mut mesh_frames = Vec::new();
        let mut scene = render_sources(
            installation,
            &self.sources[begin..],
            &mut self.graph,
            &mut self.render_textures,
            &mut self.render_bytes,
            &mut mesh_frames,
        )?;
        if self
            .render_scene
            .meshes
            .len()
            .checked_add(scene.meshes.len())
            .is_none_or(|n| n > MAX_DRAWS)
        {
            return Err("Driving render draw quota exceeded".into());
        }
        summary.added_meshes = scene.meshes.len();
        self.render_scene.meshes.append(&mut scene.meshes);
        self.render_mesh_frames.append(&mut mesh_frames);
        if summary.added_meshes != 0 {
            self.render_revision = self.render_revision.wrapping_add(1);
        }
        self.summary.source_frames = self.sources.iter().map(|s| s.keys.len()).sum();
        Ok(summary)
    }
    /// Non-destructive renderer snapshot. Textures retain shared owned images;
    /// mesh buffers are copied only when the presentation caller refreshes it.
    pub fn render_scene_near(&self, position: [f32; 3], radius: f32) -> Result<Scene, String> {
        presentation_bounds(position, radius)?;
        let mut meshes = Vec::new();
        for (mesh, frame) in self
            .render_scene
            .meshes
            .iter()
            .zip(&self.render_mesh_frames)
        {
            if self.frame_is_visible(*frame)? && mesh_near(mesh, position, radius) {
                meshes.push(Mesh {
                    material: mesh.material,
                    vertices: mesh.vertices.clone(),
                    indices: mesh.indices.clone(),
                    texture: mesh.texture.clone(),
                });
            }
        }
        Ok(Scene { meshes })
    }
    /// Only a completed old selector invocation can supply canonical adoption
    /// metadata. A failed intake retains its prefix for diagnostics, not replay.
    pub fn admitted_cache_instances(&self) -> Result<&[AdmittedCityCacheInstance], String> {
        if self.cache_failed {
            Err("Driving cache intake is interrupted; canonical adoption is unavailable".into())
        } else {
            Ok(&self.admitted_cache_instances)
        }
    }
    pub fn collision(&self) -> &DrivingCollisionWorld {
        &self.collision
    }
    pub fn dynamic_grid_source(
        &self,
    ) -> crate::dynamic_collision_manager::DynamicCollisionGridSource {
        self.dynamic_grid_source
    }
    pub fn render_scene(&self) -> &Scene {
        &self.render_scene
    }
    pub fn take_render_scene(&mut self) -> Scene {
        self.render_mesh_frames.clear();
        std::mem::replace(&mut self.render_scene, Scene { meshes: Vec::new() })
    }
    /// Presentation-only culling against actual world-space mesh bounds. The
    /// complete original KLZ collision world and frame/source owner are kept.
    pub fn take_render_scene_near(
        &mut self,
        position: [f32; 3],
        radius: f32,
    ) -> Result<Scene, String> {
        if position.iter().any(|v| !v.is_finite())
            || !radius.is_finite()
            || radius <= 0.
            || radius > 10000.
        {
            return Err("Invalid driving presentation bounds".into());
        }
        let center = [position[0], -position[2]];
        let mut scene = self.take_render_scene();
        scene.meshes.retain(|mesh| {
            let mut min = [f32::INFINITY; 2];
            let mut max = [f32::NEG_INFINITY; 2];
            for vertex in &mesh.vertices {
                for (axis, component) in [0, 2].into_iter().enumerate() {
                    min[axis] = min[axis].min(vertex.position[component]);
                    max[axis] = max[axis].max(vertex.position[component]);
                }
            }
            (0..2).all(|axis| {
                max[axis] >= center[axis] - radius && min[axis] <= center[axis] + radius
            })
        });
        Ok(scene)
    }
    pub fn summary(&self) -> &DrivingWorldAssetSummary {
        &self.summary
    }
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
    pub fn frame_graph(&self) -> &I3dFrameGraph {
        &self.graph
    }
    pub fn identity(&self) -> DrivingWorldIdentity {
        self.identity
    }
    pub fn frame_world_matrix(
        &mut self,
        frame: FrameKey,
    ) -> Result<crate::native_transform::SMatrix, String> {
        self.graph
            .world_matrix(frame)
            .map_err(|e| format!("World frame transform: {e:?}"))
    }
    pub fn render_revision(&self) -> u64 {
        self.render_revision
    }
    pub fn crash_frame_identity(&self, frame: FrameKey) -> Result<u32, String> {
        self.graph
            .state(frame)
            .map_err(|e| format!("World frame identity: {e:?}"))?;
        DRIVING_WORLD_FRAME_BASE
            .checked_add(frame.0)
            .filter(|identity| *identity < DRIVING_WORLD_FRAME_END)
            .ok_or("World symbolic frame identity quota exceeded".into())
    }
    pub fn crash_frame_key(&self, identity: u32) -> Result<FrameKey, String> {
        if !(DRIVING_WORLD_FRAME_BASE..DRIVING_WORLD_FRAME_END).contains(&identity) {
            return Err("Identity is outside the world symbolic frame range".into());
        }
        let frame = FrameKey(identity - DRIVING_WORLD_FRAME_BASE);
        self.graph
            .state(frame)
            .map_err(|e| format!("World frame identity: {e:?}"))?;
        Ok(frame)
    }
    pub fn set_frame_on(&mut self, frame: FrameKey, on: u32) -> Result<(), String> {
        self.graph
            .set_on(frame, on)
            .map_err(|e| format!("World SetOn: {e:?}"))?;
        self.render_revision = self.render_revision.wrapping_add(1);
        Ok(())
    }
    fn frame_is_visible(&self, frame: FrameKey) -> Result<bool, String> {
        let mut current = Some(frame);
        let mut steps = 0;
        while let Some(at) = current {
            if steps >= MAX_FRAMES {
                return Err("World visibility traversal quota exceeded".into());
            }
            if self
                .graph
                .state(at)
                .map_err(|e| format!("World visibility: {e:?}"))?
                .flags
                & 1
                == 0
            {
                return Ok(false);
            }
            current = self
                .graph
                .links(at)
                .map_err(|e| format!("World visibility hierarchy: {e:?}"))?
                .parent;
            steps += 1;
        }
        Ok(true)
    }
    /// Source-backed ModelCollisionsOFF430bb0: own descriptors first, then
    /// descendants in reverse direct-child order. Reached stores persist.
    pub fn model_collisions_off(&mut self, frame: FrameKey) -> Result<Vec<u32>, String> {
        self.graph
            .state(frame)
            .map_err(|e| format!("World collision owner: {e:?}"))?;
        let mut stack = vec![frame];
        let mut identities = Vec::new();
        let mut visited = 0;
        while let Some(at) = stack.pop() {
            if visited >= MAX_FRAMES {
                return Err("World collision owner traversal quota exceeded".into());
            }
            identities.extend(self.collision.disable_frame_collisions(at));
            stack.extend(self.frame_children(at)?);
            visited += 1;
        }
        Ok(identities)
    }
    /// Original407cd0: static visual mode5c(0), followed by reverse-index
    /// child recursion. Nonvisual frames retain their visual fields.
    pub fn debustize(&mut self, frame: FrameKey) -> Result<(), String> {
        let mut stack = vec![frame];
        let mut visited = 0;
        while let Some(at) = stack.pop() {
            if visited >= MAX_FRAMES {
                return Err("World visual traversal quota exceeded".into());
            }
            if self
                .graph
                .state(at)
                .map_err(|e| format!("World visual: {e:?}"))?
                .kind
                == FrameKind::Mesh
            {
                self.graph
                    .projected_state_mut(at)
                    .map_err(|e| format!("World visual store: {e:?}"))?
                    .visual_flags &= !0x6000;
            }
            stack.extend(self.frame_children(at)?);
            visited += 1;
        }
        Ok(())
    }
    pub fn frame_source(&self, frame: FrameKey) -> Result<DrivingWorldFrameSource<'_>, String> {
        let state = *self
            .graph
            .state(frame)
            .map_err(|e| format!("World frame: {e:?}"))?;
        let kind = state.kind;
        let parent = self
            .graph
            .links(frame)
            .map_err(|e| format!("World frame hierarchy: {e:?}"))?
            .parent;
        if let Some(model) = self.model_roots.get(&frame.0) {
            return Ok(DrivingWorldFrameSource {
                kind,
                parent,
                local_scale: state.scale,
                name_bytes: model.name.as_deref(),
                properties: None,
                model_resource: Some(&model.resource),
            });
        }
        for owner in &self.sources {
            if let Some(index) = owner.keys.iter().position(|key| *key == frame) {
                let source = &owner.source.frames[index];
                return Ok(DrivingWorldFrameSource {
                    kind,
                    parent,
                    local_scale: state.scale,
                    name_bytes: Some(&source.name_bytes),
                    properties: (!source.properties.is_empty())
                        .then_some(source.properties.as_slice()),
                    model_resource: None,
                });
            }
        }
        if frame == self.primary {
            return Ok(DrivingWorldFrameSource {
                kind,
                parent,
                local_scale: state.scale,
                name_bytes: Some(b"Primary sector"),
                properties: None,
                model_resource: None,
            });
        }
        Err("World frame source ownership is unavailable".into())
    }
    /// Direct children in the actual graph's stored sibling order.
    pub fn frame_children(&self, frame: FrameKey) -> Result<Vec<FrameKey>, String> {
        let mut next = self
            .graph
            .links(frame)
            .map_err(|e| format!("World frame hierarchy: {e:?}"))?
            .child;
        let mut children = Vec::new();
        while let Some(child) = next {
            if children.len() >= MAX_FRAMES {
                return Err("World child traversal quota exceeded".into());
            }
            children.push(child);
            next = self
                .graph
                .links(child)
                .map_err(|e| format!("World child hierarchy: {e:?}"))?
                .next;
        }
        Ok(children)
    }
    pub fn authored_driving_spawn(&self, vehicle_model: &str) -> Result<DrivingWorldSpawn, String> {
        let requested = resource_model(vehicle_model)?;
        let mut result = None;
        for actor in self.metadata.actors.iter().filter(|a| a.kind == Some(4)) {
            for object in self
                .metadata
                .objects
                .iter()
                .filter(|o| o.name.eq_ignore_ascii_case(&actor.name))
            {
                let Some(model) = object.model.as_deref() else {
                    continue;
                };
                if resource_model(model)? != requested {
                    continue;
                }
                if result.is_some() {
                    return Err("Authored driving vehicle is ambiguous".into());
                }
                if object.position != object.world_position
                    || object.parent.as_deref() != Some("Primary sector")
                    || object.uniform_scale.is_some_and(|s| s != 1.)
                {
                    return Err("Authored vehicle parent transform is unavailable".into());
                }
                result = Some(DrivingWorldSpawn {
                    position: object
                        .world_position
                        .ok_or("Authored vehicle world position is absent")?,
                    rotation_wxyz: object
                        .rotation_wxyz
                        .ok_or("Authored vehicle rotation is absent")?,
                    scale: object.scale.ok_or("Authored vehicle scale is absent")?,
                    actor_name: actor.name.clone(),
                    model: model.to_owned(),
                });
            }
        }
        result.ok_or("No original vehicle actor matches the requested model".into())
    }
}
fn render_sources(
    installation: &Installation,
    sources: &[GeometryOwner],
    graph: &mut I3dFrameGraph,
    textures: &mut BTreeMap<(String, [bool; 2]), Arc<Texture>>,
    bytes: &mut usize,
    mesh_frames: &mut Vec<FrameKey>,
) -> Result<Scene, String> {
    let mut meshes = Vec::new();
    for owner in sources {
        for (index, frame) in owner.source.frames.iter().enumerate() {
            let Some(local) = owner.local_vertices[index].as_deref() else {
                continue;
            };
            let geometry = if let Some(instance) = frame.instance {
                &owner.source.frames[instance]
            } else {
                frame
            };
            let lod = geometry
                .lods
                .first()
                .ok_or("Driving render LOD is unavailable")?;
            let world = graph
                .world_matrix(owner.keys[index])
                .map_err(|e| format!("Driving render transform: {e:?}"))?;
            for group in &lod.groups {
                if group.indices.is_empty() {
                    continue;
                }
                let Some(material) = owner.source.materials.get(group.material) else {
                    continue;
                };
                if meshes.len() >= MAX_DRAWS {
                    return Err("Driving render draw quota exceeded".into());
                }
                charge(
                    bytes,
                    local.len() * std::mem::size_of::<Vertex>() + group.indices.len() * 4,
                    MAX_RENDER_BYTES,
                )?;
                let mut vertices = Vec::with_capacity(local.len());
                for (position, vertex) in local.iter().zip(&lod.vertices) {
                    let p = world
                        .try_transform_point(*position)
                        .map_err(|e| format!("Driving render vertex: {e:?}"))?;
                    vertices.push(Vertex {
                        position: [p[0], p[1], -p[2]],
                        uv: vertex.uv,
                    });
                }
                let repeat = [material.flags & 0x10000 == 0, material.flags & 0x20000 == 0];
                let texture = if material.texture.is_empty() {
                    let rgba = material
                        .diffuse
                        .map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
                    Arc::new(Texture {
                        width: 1,
                        height: 1,
                        repeat,
                        rgba: vec![rgba[0], rgba[1], rgba[2], 255],
                    })
                } else {
                    let path = format!("maps/{}", material.texture);
                    let key = (path.to_ascii_lowercase(), repeat);
                    if let Some(texture) = textures.get(&key) {
                        texture.clone()
                    } else {
                        let encoded = installation
                            .read(&path)
                            .map_err(|e| format!("Driving texture {path}: {e}"))?;
                        let image = if encoded.starts_with(b"BM") {
                            lhr_assets::decode_bmp_original(&encoded)
                        } else {
                            lhr_assets::decode_tga(&encoded)
                        }
                        .map_err(|e| format!("Driving texture {path}: {e}"))?;
                        charge(bytes, image.rgba.len(), MAX_RENDER_BYTES)?;
                        let texture = Arc::new(Texture {
                            width: image.width,
                            height: image.height,
                            rgba: image.rgba,
                            repeat,
                        });
                        textures.insert(key, texture.clone());
                        texture
                    }
                };
                let mut indices = group.indices.clone();
                for triangle in indices.as_chunks_mut::<3>().0 {
                    triangle.swap(1, 2);
                }
                meshes.push(Mesh {
                    material: Default::default(),
                    vertices,
                    indices,
                    texture: Some(texture),
                });
                mesh_frames.push(owner.keys[index]);
            }
        }
    }
    Ok(Scene { meshes })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn empty_collision_world() -> DrivingCollisionWorld {
        let mut bytes = vec![0; 36 + 104];
        bytes[..4].copy_from_slice(b"GifC");
        for (offset, value) in [(4, 5_u32), (8, 36), (12, 1), (24, 28), (28, 1)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[32] = b'f';
        for (index, value) in [0_f32, 0., 10., 10., 10., 10.].into_iter().enumerate() {
            bytes[36 + index * 4..40 + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        for offset in [24, 28] {
            bytes[36 + offset..40 + offset].copy_from_slice(&1_u32.to_le_bytes());
        }
        for value in [0_f32, 10., 0., 10.] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 8]); // Array and world-vertex separators.
        bytes.extend_from_slice(&[0; 16]); // Empty cell.
        let source = lhr_assets::klz::Klz::parse(&bytes).unwrap();
        let bound = crate::collision_world::bind_collision_world(&source, &[]).unwrap();
        let prepared = crate::collision_prepare::prepare_bound_collision_world(&bound).unwrap();
        DrivingCollisionWorld::from_prepared(&prepared, false).unwrap()
    }
    pub(crate) fn cache_world(render_bytes: usize) -> DrivingWorldAssets {
        let mut graph = I3dFrameGraph::default();
        let mut primary = I3dFrameState::model();
        primary.kind = FrameKind::Sector;
        let primary = graph.insert(primary).unwrap();
        let mut visual = frame(1, None, [1.; 3], [0.; 3]);
        // Match the admitted ordinary object constructor flags so this public
        // fixture can also enter the real pristine MissionCaches copy path.
        visual.logic_flags = 0x2a;
        visual.culling = 0x11;
        visual.properties = b";X100M100;".to_vec();
        visual.lods[0].groups.push(lhr_assets::FaceGroup {
            material: 0,
            indices: vec![0, 0, 0],
        });
        let source = Arc::new(Model {
            materials: vec![lhr_assets::Material {
                flags: 0,
                ambient: [0.; 3],
                diffuse: [1.; 3],
                texture: String::new(),
                emission: [0.; 3],
                opacity: 1.,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
            frames: vec![visual],
        });
        DrivingWorldAssets {
            identity: DrivingWorldIdentity::fresh().unwrap(),
            collision: empty_collision_world(),
            render_scene: Scene { meshes: Vec::new() },
            render_mesh_frames: Vec::new(),
            render_revision: 0,
            graph,
            sources: Vec::new(),
            model_roots: BTreeMap::new(),
            metadata: Default::default(),
            summary: Default::default(),
            dynamic_grid_source: crate::dynamic_collision_manager::DynamicCollisionGridSource {
                min_xz: [0.; 2],
                max_xz: [10.; 2],
                desired_cell_size: 10.,
            },
            mission: "synthetic".into(),
            primary,
            cache: Some(lhr_assets::CityCache {
                parts: vec![lhr_assets::CachePart {
                    name: "synthetic".into(),
                    sphere_position: [0.; 2],
                    sphere_radius: 10.,
                    bounds_and_edges: [[0.; 2]; 8],
                    objects: vec![lhr_assets::CacheObject {
                        model: "fixture.i3d".into(),
                        position: [0.; 3],
                        rotation_wxyz: [1., 0., 0., 0.],
                        scale: [1.; 3],
                        flags: 0,
                        secondary_scale: [1.; 3],
                        trailing_bytes: 0,
                    }],
                }],
                unknown_chunks: 0,
            }),
            cache_failed: false,
            cache_loaded: BTreeSet::new(),
            cache_models: BTreeMap::from([("models/fixture.4ds".into(), source)]),
            admitted_cache_instances: Vec::new(),
            cache_unsupported: BTreeMap::new(),
            source_bytes: 0,
            render_textures: BTreeMap::new(),
            render_bytes,
        }
    }
    #[test]
    fn failed_cache_render_retains_source_prefix_and_cannot_retry_as_success() {
        let mut world = cache_world(MAX_RENDER_BYTES);
        // The cached source and untextured material avoid installation reads.
        let installation = Installation::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let error = world
            .load_cache_render_near(&installation, [0.; 3], 10.)
            .unwrap_err();
        assert!(error.contains("storage quota"), "{error}");
        assert_eq!(world.sources.len(), 1);
        assert!(world.cache_loaded.contains(&(0, 0)));
        assert!(world.render_scene.meshes.is_empty());
        world.render_bytes = 0;
        let error = world
            .load_cache_render_near(&installation, [0.; 3], 10.)
            .unwrap_err();
        assert!(error.contains("interrupted"), "{error}");
        assert_eq!(world.sources.len(), 1);
        assert!(world.render_scene.meshes.is_empty());
    }
    #[test]
    fn source_frames_preserve_properties_and_symbolic_identity_and_live_visibility() {
        let mut world = cache_world(0);
        let installation = Installation::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert_eq!(
            world
                .load_cache_render_near(&installation, [0.; 3], 10.)
                .unwrap()
                .added_meshes,
            1
        );
        let model = FrameKey(*world.model_roots.keys().next().unwrap());
        let identity = world.crash_frame_identity(model).unwrap();
        assert_eq!(identity, DRIVING_WORLD_FRAME_BASE + model.0);
        assert_eq!(world.crash_frame_key(identity).unwrap(), model);
        assert!(world.crash_frame_key(DRIVING_WORLD_FRAME_BASE).is_err());
        assert!(world.crash_frame_key(model.0).is_err());
        let source = world.frame_source(model).unwrap();
        assert_eq!(source.kind, FrameKind::Model);
        assert_eq!(source.parent, Some(world.primary));
        assert_eq!(source.local_scale, [1.; 3]);
        assert_eq!(source.properties, None);
        assert_eq!(source.model_resource, Some("fixture.i3d"));
        let children = world.frame_children(model).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(
            world.frame_source(children[0]).unwrap().properties,
            Some(b";X100M100;".as_slice())
        );
        assert_eq!(
            world.frame_world_matrix(model).unwrap(),
            crate::native_transform::SMatrix::identity()
        );
        assert_eq!(
            world.render_scene_near([0.; 3], 10.).unwrap().meshes.len(),
            1
        );
        let revision = world.render_revision();
        world.set_frame_on(model, 0).unwrap();
        assert_ne!(world.render_revision(), revision);
        assert_eq!(
            world.render_scene_near([0.; 3], 10.).unwrap().meshes.len(),
            0
        );
        world.set_frame_on(model, 1).unwrap();
        assert_eq!(
            world.render_scene_near([0.; 3], 10.).unwrap().meshes.len(),
            1
        );
        assert!(world
            .load_cache_render_near(&installation, [0.; 3], 10.)
            .is_ok());
    }
    fn frame(
        kind: u8,
        parent: Option<usize>,
        scale: [f32; 3],
        position: [f32; 3],
    ) -> lhr_assets::Frame {
        lhr_assets::Frame {
            kind,
            visual: (kind == 1).then_some(0),
            render_flags: 0,
            logic_flags: 0,
            culling: 0,
            name: "synthetic".into(),
            name_bytes: b"synthetic".to_vec(),
            properties: Vec::new(),
            dummy_bounds: None,
            light: None,
            joint: None,
            target: None,
            occluder: None,
            sector: None,
            skin: None,
            morph: None,
            lensflare: None,
            parent,
            position,
            scale,
            rotation_wxyz: [1., 0., 0., 0.],
            instance: None,
            lods: if kind == 1 {
                vec![lhr_assets::Lod {
                    distance: 0.,
                    uv_only: None,
                    vertices: vec![lhr_assets::Vertex {
                        position: [1., 0., 0.],
                        normal: [0., 1., 0.],
                        uv: [0.; 2],
                    }],
                    groups: Vec::new(),
                }]
            } else {
                Vec::new()
            },
        }
    }
    #[test]
    fn cache_secondary_scale_replaces_visual_local_scale_under_original_dummy_hierarchy() {
        for (secondary, expected) in [([1.; 3], 30.), ([2., 1., 1.], 36.)] {
            let mut graph = I3dFrameGraph::default();
            let mut state = I3dFrameState::source_mesh(
                [10., 0., 0.],
                [2.; 3],
                SQuat {
                    wxyz: [1., 0., 0., 0.],
                },
            );
            state.kind = FrameKind::Model;
            let root = graph.insert(state).unwrap();
            let source = Arc::new(Model {
                materials: Vec::new(),
                frames: vec![
                    frame(6, None, [3.; 3], [1., 0., 0.]),
                    frame(1, Some(0), [9.; 3], [2., 0., 0.]),
                ],
            });
            let owner = append_cache_source(&mut graph, source, root, secondary).unwrap();
            assert_eq!(graph.state(owner.keys[0]).unwrap().scale, [3.; 3]);
            assert_eq!(graph.state(owner.keys[1]).unwrap().scale, secondary);
            let matrix = graph.world_matrix(owner.keys[1]).unwrap();
            assert_eq!(
                matrix.try_transform_point([1., 0., 0.]).unwrap(),
                [expected, 0., 0.]
            );
        }
    }
    #[test]
    fn cache_rotation_admits_small_normal_components_with_normal_intermediates() {
        let rotation = SQuat {
            wxyz: [0.8, 1e-15, 0.6, 0.],
        };
        assert!(!rotation.is_verified_input());
        let normalized = normalized_cache_rotation(rotation).unwrap();
        assert!(normalized.wxyz.iter().all(|x| *x == 0. || x.is_normal()));
    }
    #[test]
    fn original_model_filename_accepts_windows_and_posix_resource_separators() {
        for name in ["Models\\taxi00.i3d", "models/taxi00.4ds", "taxi00.i3d"] {
            assert_eq!(resource_model(name).unwrap(), "models/taxi00.4ds");
        }
        for name in [
            "../taxi00.i3d",
            "models\\..\\taxi00.i3d",
            "other/taxi00.i3d",
        ] {
            assert!(resource_model(name).is_err());
        }
    }
    #[test]
    fn admitted_cache_records_keep_table_identity_pose_and_actual_world_keys() {
        let mut world = cache_world(0);
        let mut other = cache_world(0);
        assert_ne!(world.identity(), other.identity());
        let installation = Installation::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        world
            .load_cache_render_near(&installation, [0.; 3], 10.)
            .unwrap();
        other
            .load_cache_render_near(&installation, [0.; 3], 10.)
            .unwrap();
        let placement = &world.admitted_cache_instances().unwrap()[0];
        assert_eq!(placement.id, CachePlacementId { part: 0, object: 0 });
        assert_eq!(placement.resource, "fixture.i3d");
        assert_eq!(placement.rotation.wxyz, [1., 0., 0., 0.]);
        assert_eq!(placement.world_frames.len(), placement.source.frames.len());
        let root = placement.world_root;
        let mesh = placement.world_frames[0];
        assert_eq!(world.graph.links(mesh).unwrap().parent, Some(root));
        world.set_frame_on(root, 0).unwrap();
        assert!(!world.frame_is_visible(mesh).unwrap());
        assert_eq!(
            normalized_cache_model_resource("0rek23.i3d").unwrap(),
            "0rek17.i3d"
        );
    }
}
