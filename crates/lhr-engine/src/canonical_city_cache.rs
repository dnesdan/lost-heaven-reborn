//! Canonical ownership adapter for already-admitted host city-cache selections.
//! Actual models/resources live in the same ModelArena/Scene/MissionCaches as
//! the rest of the world. This is not the original Game city-streaming controller.
//! Activation SRT/visual-scale/order crosscheck: Game08398317... RVA25d0;
//! complete private primary contract is retained outside the repository.
use crate::driving_world_assets::{
    AdmittedCityCacheInstance, CachePlacementId, DrivingWorldAssets, DrivingWorldIdentity,
};
use crate::frame_transform::{FrameKey, FrameKind, I3dFrameGraph};
use crate::mission_resources::{
    CacheMemoryError, CpuMissionCacheMemory, MissionCaches, ModelCacheOpenError,
    ModelCacheOpenProgress,
};
use crate::model_loading::{ModelArena, ModelKey};
use crate::scene_ownership::{FrameOwner, SceneKey};
use crate::script_scheduler::FrameReferenceId;
use lhr_assets::Installation;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalCachePhase {
    Open,
    RootPose,
    VisualScale,
    Membership,
    Sector,
    Visibility,
    CreatorRelease,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalCacheFrameMap {
    pub world_frame: FrameKey,
    pub frame: FrameKey,
    /// None denotes the actual model root, not a native pointer formula.
    pub source_index: Option<usize>,
}
#[derive(Debug)]
pub struct CanonicalCacheInstance {
    pub id: CachePlacementId,
    pub resource: String,
    pub model: ModelKey,
    pub frame_map: Vec<CanonicalCacheFrameMap>,
}
#[derive(Debug)]
pub struct PendingCanonicalCacheInstance {
    pub instance: CanonicalCacheInstance,
    pub phase: CanonicalCachePhase,
}
#[derive(Clone, Debug)]
pub struct UnsupportedCanonicalCacheInstance {
    pub id: CachePlacementId,
    pub resource: String,
    pub detail: String,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CanonicalCityCacheSummary {
    pub selected: usize,
    pub adopted: usize,
    pub unsupported: usize,
    pub mapped_frames: usize,
}
#[derive(Debug, Default)]
pub struct CanonicalCityCache {
    instances: Vec<CanonicalCacheInstance>,
    pending: Vec<PendingCanonicalCacheInstance>,
    unsupported: Vec<UnsupportedCanonicalCacheInstance>,
    processed: BTreeSet<CachePlacementId>,
    interrupted: bool,
    closed: bool,
    binding: Option<Binding>,
    cache_open_failure: Option<ModelCacheOpenError<CacheMemoryError>>,
    cache_open_progress: Option<ModelCacheOpenProgress>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Binding {
    arena: FrameReferenceId,
    scene: SceneKey,
    scene_reference: FrameReferenceId,
    world: DrivingWorldIdentity,
}
struct Context<'a> {
    arena: &'a mut ModelArena,
    scene: SceneKey,
    caches: &'a mut MissionCaches,
    memory: &'a mut CpuMissionCacheMemory,
    installation: &'a Installation,
    world_graph: &'a I3dFrameGraph,
    world_identity: DrivingWorldIdentity,
}
type Out<T> = Result<T, String>;
fn error(e: impl std::fmt::Debug) -> String {
    format!("canonical city cache: {e:?}")
}
fn unsupported(instance: &AdmittedCityCacheInstance) -> Option<String> {
    if !instance.resource.is_ascii() {
        return Some("resource name requires the unimplemented original non-ASCII locale".into());
    }
    if instance.world_frames.len() != instance.source.frames.len() {
        return Some("source/world frame correspondence is incomplete".into());
    }
    for (index, frame) in instance.source.frames.iter().enumerate() {
        if !matches!((frame.kind, frame.visual), (1, Some(0)) | (6, None)) {
            return Some(format!("source frame {index} kind{} visual{:?} is outside the admitted static plain/dummy cache-copy subset",frame.kind,frame.visual));
        }
    }
    None
}
fn source_frames(arena: &ModelArena, model: ModelKey) -> Out<BTreeMap<usize, FrameKey>> {
    let mut out = BTreeMap::new();
    for &key in &arena.model(model).map_err(error)?.frames {
        let frame = arena.frame(key).map_err(error)?;
        if frame.owner != Some(FrameOwner::Model(model)) {
            return Err("canonical city cache: foreign frame membership".into());
        }
        let index = frame
            .source_index
            .ok_or("canonical city cache: missing source index")?;
        if out.insert(index, key).is_some() {
            return Err("canonical city cache: duplicate source index".into());
        }
    }
    Ok(out)
}
fn source_visible(graph: &I3dFrameGraph, root: FrameKey) -> Out<bool> {
    let mut current = Some(root);
    let mut count = 0;
    while let Some(key) = current {
        if count >= 65536 {
            return Err("canonical city cache: world hierarchy extent".into());
        }
        if graph.state(key).map_err(error)?.flags & 1 == 0 {
            return Ok(false);
        }
        current = graph.links(key).map_err(error)?.parent;
        count += 1;
    }
    Ok(true)
}
fn sync_instance(
    arena: &mut ModelArena,
    graph: &I3dFrameGraph,
    instance: &mut CanonicalCacheInstance,
) -> Out<()> {
    let current = source_frames(arena, instance.model)?;
    for mapping in &mut instance.frame_map {
        let (frame, on) = if let Some(index) = mapping.source_index {
            (
                *current
                    .get(&index)
                    .ok_or("canonical city cache: removed logical source frame")?,
                graph.state(mapping.world_frame).map_err(error)?.flags & 1 != 0,
            )
        } else {
            (
                instance.model.root(),
                source_visible(graph, mapping.world_frame)?,
            )
        };
        // Resolve current owned membership by source provenance on every sync.
        // A genuine scene2 visual replacement can therefore replace the actual
        // key without leaving this correspondence pointed at its old tombstone.
        mapping.frame = frame;
        arena.set_frame_on(frame, u32::from(on)).map_err(error)?;
    }
    Ok(())
}
impl CanonicalCityCache {
    pub fn placement_count(&self) -> usize {
        self.instances.len()
    }
    pub fn cache_open_failure(&self) -> Option<&ModelCacheOpenError<CacheMemoryError>> {
        self.cache_open_failure.as_ref()
    }
    pub fn cache_open_progress(&self) -> Option<&ModelCacheOpenProgress> {
        self.cache_open_progress.as_ref()
    }
    pub fn instances(&self) -> &[CanonicalCacheInstance] {
        &self.instances
    }
    pub fn pending(&self) -> &[PendingCanonicalCacheInstance] {
        &self.pending
    }
    pub fn unsupported(&self) -> &[UnsupportedCanonicalCacheInstance] {
        &self.unsupported
    }
    pub fn interrupted(&self) -> bool {
        self.interrupted
    }
    /// Adopt only completed old-selector records. Model loading is the genuine
    /// shared MissionCaches resource path, not a parallel parsed/render owner.
    pub fn adopt(
        &mut self,
        arena: &mut ModelArena,
        scene: SceneKey,
        caches: &mut MissionCaches,
        memory: &mut CpuMissionCacheMemory,
        installation: &Installation,
        world: &DrivingWorldAssets,
    ) -> Out<CanonicalCityCacheSummary> {
        let instances = world.admitted_cache_instances()?;
        self.adopt_instances(
            &mut Context {
                arena,
                scene,
                caches,
                memory,
                installation,
                world_graph: world.frame_graph(),
                world_identity: world.identity(),
            },
            instances,
        )
    }
    fn adopt_instances(
        &mut self,
        context: &mut Context<'_>,
        instances: &[AdmittedCityCacheInstance],
    ) -> Out<CanonicalCityCacheSummary> {
        ctx_binding(self, context)?;
        if self.closed {
            return Err("canonical city cache is closed".into());
        }
        if self.interrupted {
            return Err(
                "canonical city cache intake was interrupted; retained prefix cannot replay".into(),
            );
        }
        self.interrupted = true;
        let result = self.adopt_inner(context, instances);
        if result.is_ok() {
            self.interrupted = false;
        }
        result
    }
    fn adopt_inner(
        &mut self,
        ctx: &mut Context<'_>,
        instances: &[AdmittedCityCacheInstance],
    ) -> Out<CanonicalCityCacheSummary> {
        let mut summary = CanonicalCityCacheSummary::default();
        for source in instances {
            if self.processed.contains(&source.id) {
                continue;
            }
            summary.selected += 1;
            if let Some(detail) = unsupported(source) {
                self.unsupported.push(UnsupportedCanonicalCacheInstance {
                    id: source.id,
                    resource: source.resource.clone(),
                    detail,
                });
                self.processed.insert(source.id);
                summary.unsupported += 1;
                continue;
            }
            let model = ctx.arena.create_model().map_err(error)?;
            self.pending.push(PendingCanonicalCacheInstance {
                instance: CanonicalCacheInstance {
                    id: source.id,
                    resource: source.resource.clone(),
                    model,
                    frame_map: vec![CanonicalCacheFrameMap {
                        world_frame: source.world_root,
                        frame: model.root(),
                        source_index: None,
                    }],
                },
                phase: CanonicalCachePhase::Open,
            });
            let at = self.pending.len() - 1;
            let opened = ctx.caches.open_static_model(
                ctx.arena,
                ctx.installation,
                model,
                source.resource.as_bytes(),
                ctx.memory,
            );
            self.cache_open_progress = ctx
                .caches
                .model_open_progress()
                .filter(|progress| progress.target == model)
                .cloned();
            let status = match opened {
                Ok(status) => status,
                Err(failure) => {
                    let detail = error(&failure);
                    self.cache_open_failure = Some(failure);
                    return Err(detail);
                }
            };
            if status < 0 {
                return Err(format!(
                    "canonical city cache: model Open returned{status:#x} for {:?}",
                    source.id
                ));
            }
            let frames = source_frames(ctx.arena, model)?;
            if frames.len() != source.world_frames.len() {
                return Err("canonical city cache: loaded source frame count differs from admitted metadata".into());
            }
            for (index, &world_frame) in source.world_frames.iter().enumerate() {
                let frame = *frames
                    .get(&index)
                    .ok_or("canonical city cache: loaded source index missing")?;
                self.pending[at]
                    .instance
                    .frame_map
                    .push(CanonicalCacheFrameMap {
                        world_frame,
                        frame,
                        source_index: Some(index),
                    });
            }
            self.pending[at].phase = CanonicalCachePhase::RootPose;
            ctx.arena
                .graph
                .assign_position(model.root(), source.position)
                .map_err(error)?;
            // Already normalized by the admitted cache source adapter: retain
            // these exact quaternion bits instead of normalizing a second time.
            {
                let state = ctx
                    .arena
                    .graph
                    .projected_state_mut(model.root())
                    .map_err(error)?;
                state.rotation = source.rotation;
                state.flags = (state.flags & 0xfffffecb) | 0x40000008;
            }
            ctx.arena
                .graph
                .assign_scale(model.root(), source.root_scale)
                .map_err(error)?;
            ctx.arena.update_frame(model.root()).map_err(error)?;
            self.pending[at].phase = CanonicalCachePhase::VisualScale;
            for &frame in frames.values() {
                if ctx.arena.state(frame).map_err(error)?.kind == FrameKind::Mesh {
                    ctx.arena
                        .graph
                        .assign_scale(frame, source.secondary_scale)
                        .map_err(error)?;
                }
                ctx.arena.update_frame(frame).map_err(error)?;
            }
            self.pending[at].phase = CanonicalCachePhase::Membership;
            ctx.arena
                .scene_add_frame(ctx.scene, model.root())
                .map_err(error)?;
        }
        // Native activation spatially publishes queued roots in reverse order.
        // Calling SetFrameSectorPos from no parent performs the actual sector
        // selection/link; an eager Primary link would change sibling order.
        for index in (0..self.pending.len()).rev() {
            let pending = &mut self.pending[index];
            let root = pending.instance.model.root();
            pending.phase = CanonicalCachePhase::Sector;
            let world = ctx.arena.world_matrix(root).map_err(error)?;
            let position = world.elements[12..15].try_into().unwrap();
            let _native_result = ctx
                .arena
                .set_frame_sector_pos(ctx.scene, root, position)
                .map_err(error)?;
            ctx.arena.update_frame(root).map_err(error)?;
            pending.phase = CanonicalCachePhase::Visibility;
            sync_instance(ctx.arena, ctx.world_graph, &mut pending.instance)?;
            pending.phase = CanonicalCachePhase::CreatorRelease;
            ctx.arena.release(root).map_err(error)?;
            let completed = self.pending.remove(index).instance;
            self.processed.insert(completed.id);
            summary.adopted += 1;
            summary.mapped_frames += completed.frame_map.len();
            self.instances.push(completed);
        }
        Ok(summary)
    }
    pub fn sync_visibility(
        &mut self,
        arena: &mut ModelArena,
        world: &DrivingWorldAssets,
    ) -> Out<()> {
        self.check_world(arena, world.identity())?;
        self.sync_from_graph(arena, world.frame_graph())
    }
    fn sync_from_graph(&mut self, arena: &mut ModelArena, graph: &I3dFrameGraph) -> Out<()> {
        if self.closed || self.interrupted {
            return Err(
                "canonical city cache visibility is unavailable after close/interruption".into(),
            );
        }
        for instance in &mut self.instances {
            sync_instance(arena, graph, instance)?;
        }
        Ok(())
    }
    /// Drop outstanding factory references before the containing SceneClose.
    /// Completed roots are scene-owned and are released by that actual owner;
    /// MissionCaches teardown/reclamation must follow SceneClose as usual.
    pub fn close(&mut self, arena: &mut ModelArena) -> Out<()> {
        if let Some(binding) = self.binding {
            if arena.lookup_reference(FrameKey(0)) != binding.arena {
                return Err("canonical city cache: foreign arena during close".into());
            }
        }
        while let Some(pending) = self.pending.last() {
            arena
                .release(pending.instance.model.root())
                .map_err(error)?;
            self.pending.pop();
        }
        self.closed = true;
        Ok(())
    }
    fn check_world(&self, arena: &ModelArena, world: DrivingWorldIdentity) -> Out<()> {
        let binding = self.binding.ok_or("canonical city cache is not bound")?;
        if arena.lookup_reference(FrameKey(0)) != binding.arena
            || world != binding.world
            || arena.lookup_reference(binding.scene.root()) != binding.scene_reference
        {
            return Err("canonical city cache: foreign arena/scene/world domain".into());
        }
        arena.scene(binding.scene).map_err(error)?;
        Ok(())
    }
}
fn ctx_binding(owner: &mut CanonicalCityCache, ctx: &Context<'_>) -> Out<()> {
    ctx.arena.scene(ctx.scene).map_err(error)?;
    let binding = Binding {
        arena: ctx.arena.lookup_reference(FrameKey(0)),
        scene: ctx.scene,
        scene_reference: ctx.arena.lookup_reference(ctx.scene.root()),
        world: ctx.world_identity,
    };
    if owner.binding.is_some_and(|previous| previous != binding) {
        return Err("canonical city cache: foreign arena/scene/world domain".into());
    }
    owner.binding = Some(binding);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_transform::I3dFrameState;
    use crate::mission_resources::MissionCacheKind;
    use crate::native_transform::SQuat;
    use std::sync::Arc;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    struct Fixture {
        arena: ModelArena,
        scene: SceneKey,
        caches: MissionCaches,
        memory: CpuMissionCacheMemory,
        directory: std::path::PathBuf,
        installation: Installation,
        graph: I3dFrameGraph,
        domain: DrivingWorldIdentity,
        placements: Vec<AdmittedCityCacheInstance>,
        source_model: ModelKey,
    }
    fn source() -> Arc<lhr_assets::Model> {
        let mut dummy = crate::model_loading::tests::frame(6, None, [1., 0., 0.]);
        dummy.scale = [3.; 3];
        let mut mesh = crate::model_loading::tests::frame(1, Some(0), [2., 0., 0.]);
        mesh.scale = [9.; 3];
        mesh.render_flags = 0;
        mesh.logic_flags = 0x2a;
        mesh.lods = vec![lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., 1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: lhr_assets::UNASSIGNED_MATERIAL,
                indices: vec![0, 1, 2],
            }],
        }];
        Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![dummy, mesh],
        })
    }
    fn write_dummy(directory: &std::path::Path) {
        std::fs::create_dir_all(directory.join("models")).unwrap();
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.push(6);
        bytes.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0x11, 4]);
        bytes.extend(b"part");
        bytes.push(0);
        bytes.extend([0; 24]);
        bytes.push(0);
        std::fs::write(directory.join("models/dummy.4ds"), bytes).unwrap();
    }
    impl Fixture {
        fn new() -> Self {
            Self::with_source(source())
        }
        fn with_source(source: Arc<lhr_assets::Model>) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "lhr-canonical-cache-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            write_dummy(&directory);
            let installation = Installation::open(&directory).unwrap();
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let source_model = arena.create_model().unwrap();
            arena
                .load_parsed_into_model(source_model, source.clone())
                .unwrap();
            arena
                .load_plain_visual_resources(source_model, &installation)
                .unwrap();
            let mut caches = MissionCaches::empty().unwrap();
            let mut memory = CpuMissionCacheMemory::default();
            caches
                .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
                .unwrap();
            caches
                .retain_model(source_model, b"fixture.i3d", &mut arena, &mut memory)
                .unwrap();
            // One real miss materializes the native item-array projection. The
            // mesh source remains pinned until that insertion has completed.
            let target = arena.create_model().unwrap();
            assert_eq!(
                caches
                    .open_static_model(&mut arena, &installation, target, b"dummy.i3d", &mut memory)
                    .unwrap(),
                0
            );
            arena.release(target.root()).unwrap();
            arena.release(source_model.root()).unwrap();
            let mut graph = I3dFrameGraph::default();
            let mut state = I3dFrameState::model();
            state.kind = FrameKind::Sector;
            let primary = graph.insert(state).unwrap();
            let mut placements = Vec::new();
            for object in 0..2 {
                let position = [10. + 10. * object as f32, 0., 0.];
                let root_scale = [2.; 3];
                let secondary_scale = if object == 0 { [2., 1., 1.] } else { [1.; 3] };
                let rotation = SQuat {
                    wxyz: [1., 0., 0., 0.],
                };
                let mut state = I3dFrameState::source_mesh(position, root_scale, rotation);
                state.kind = FrameKind::Model;
                let world_root = graph.insert(state).unwrap();
                graph.link_to(world_root, Some(primary)).unwrap();
                let mut world_frames = Vec::new();
                for frame in &source.frames {
                    let mut state = I3dFrameState::source_mesh(
                        frame.position,
                        if frame.kind == 1 {
                            secondary_scale
                        } else {
                            frame.scale
                        },
                        SQuat {
                            wxyz: frame.rotation_wxyz,
                        },
                    );
                    state.kind = if frame.kind == 1 {
                        FrameKind::Mesh
                    } else {
                        FrameKind::Dummy
                    };
                    let key = graph.insert(state).unwrap();
                    let parent = frame
                        .parent
                        .map(|index| world_frames[index])
                        .unwrap_or(world_root);
                    graph.link_to(key, Some(parent)).unwrap();
                    world_frames.push(key);
                }
                placements.push(AdmittedCityCacheInstance {
                    id: CachePlacementId { part: 0, object },
                    resource: "fixture.i3d".into(),
                    source: source.clone(),
                    position,
                    rotation,
                    root_scale,
                    secondary_scale,
                    opaque_flags: 0xdead_beef,
                    world_root,
                    world_frames,
                });
            }
            Self {
                arena,
                scene,
                caches,
                memory,
                directory,
                installation,
                graph,
                domain: DrivingWorldIdentity::fresh().unwrap(),
                placements,
                source_model,
            }
        }
        fn adopt(&mut self, owner: &mut CanonicalCityCache) -> Out<CanonicalCityCacheSummary> {
            owner.adopt_instances(
                &mut Context {
                    arena: &mut self.arena,
                    scene: self.scene,
                    caches: &mut self.caches,
                    memory: &mut self.memory,
                    installation: &self.installation,
                    world_graph: &self.graph,
                    world_identity: self.domain,
                },
                &self.placements,
            )
        }
        fn finish(&mut self, owner: &mut CanonicalCityCache) {
            let models = owner
                .instances()
                .iter()
                .map(|instance| instance.model)
                .collect::<Vec<_>>();
            owner.close(&mut self.arena).unwrap();
            for model in models {
                self.arena
                    .scene_delete_frame(self.scene, model.root())
                    .unwrap();
            }
            self.caches
                .destruct(MissionCacheKind::Model, &mut self.arena, &mut self.memory)
                .unwrap();
            self.arena.reclaim_unused_visual_buffers().unwrap();
            assert_eq!(self.memory.live_allocations(), 0);
            assert_eq!(self.memory.live_bytes(), 0);
            assert!(self.arena.scene(self.scene).unwrap().frames().is_empty());
            assert!(self.arena.frame(self.source_model.root()).is_err());
            let backend = self.arena.loaded_visual_resources().unwrap().backend();
            assert_eq!(backend.live_graphics(), 0);
            assert_eq!(backend.live_textures(), 0);
            assert_eq!(backend.live_allocations(), 0);
            std::fs::remove_dir_all(&self.directory).unwrap();
        }
    }
    fn mapped(instance: &CanonicalCacheInstance, index: usize) -> FrameKey {
        instance
            .frame_map
            .iter()
            .find(|mapping| mapping.source_index == Some(index))
            .unwrap()
            .frame
    }
    #[test]
    fn actual_instances_share_resources_replace_visual_scale_and_keep_dummy_pose_until_close() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut fixture = Fixture::new();
            let mut owner = CanonicalCityCache::default();
            let summary = fixture.adopt(&mut owner).unwrap();
            assert_eq!(summary.adopted, 2);
            assert_eq!(summary.mapped_frames, 6);
            assert!(owner.pending().is_empty());
            assert_eq!(owner.placement_count(), 2);
            let mut mesh_keys = Vec::new();
            let mut object_keys = Vec::new();
            for instance in owner.instances() {
                let source = &fixture.placements[instance.id.object];
                let dummy = mapped(instance, 0);
                let mesh = mapped(instance, 1);
                assert_eq!(
                    fixture.arena.frame(instance.model.root()).unwrap().owner,
                    Some(FrameOwner::Scene(fixture.scene))
                );
                assert_eq!(
                    fixture
                        .arena
                        .frame(instance.model.root())
                        .unwrap()
                        .references,
                    1
                );
                assert_eq!(fixture.arena.state(dummy).unwrap().scale, [3.; 3]);
                assert_eq!(
                    fixture.arena.state(mesh).unwrap().scale,
                    source.secondary_scale
                );
                let actual = fixture
                    .arena
                    .world_matrix(mesh)
                    .unwrap()
                    .try_transform_point([1., 0., 0.])
                    .unwrap();
                let old = fixture
                    .graph
                    .world_matrix(source.world_frames[1])
                    .unwrap()
                    .try_transform_point([1., 0., 0.])
                    .unwrap();
                assert_eq!(actual, old);
                assert_eq!(actual[0], if instance.id.object == 0 { 36. } else { 40. });
                let object = fixture.arena.frame_object_resources(mesh).unwrap().unwrap();
                object_keys.push(object);
                mesh_keys.push(
                    fixture
                        .arena
                        .loaded_visual_resources()
                        .unwrap()
                        .object_mesh(object)
                        .unwrap()
                        .unwrap(),
                );
            }
            assert_ne!(object_keys[0], object_keys[1]);
            assert_eq!(mesh_keys[0], mesh_keys[1]);
            let cached = source_frames(&fixture.arena, fixture.source_model).unwrap();
            assert_eq!(fixture.arena.state(cached[&1]).unwrap().scale, [9.; 3]);
            let primary = fixture.arena.scene(fixture.scene).unwrap().sectors[0].frame();
            let first = owner
                .instances()
                .iter()
                .find(|instance| instance.id.object == 0)
                .unwrap()
                .model
                .root();
            assert_eq!(fixture.arena.links(primary).unwrap().child, Some(first));
            let before = fixture.arena.live_frame_count();
            assert_eq!(fixture.adopt(&mut owner).unwrap().adopted, 0);
            assert_eq!(fixture.arena.live_frame_count(), before);
            fixture.finish(&mut owner);
            for key in object_keys {
                assert!(fixture
                    .arena
                    .loaded_visual_resources()
                    .unwrap()
                    .objects()
                    .object(key)
                    .is_err());
            }
            assert!(fixture
                .arena
                .loaded_visual_resources()
                .unwrap()
                .objects()
                .managed()
                .resources()
                .mesh_object(mesh_keys[0])
                .is_err());
        });
    }
    #[test]
    fn visibility_tracks_real_parents_and_refreshes_a_genuinely_replaced_visual_key() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut fixture = Fixture::new();
            let mut owner = CanonicalCityCache::default();
            fixture.adopt(&mut owner).unwrap();
            let index = owner
                .instances
                .iter()
                .position(|instance| instance.id.object == 0)
                .unwrap();
            let model = owner.instances[index].model;
            let old = mapped(&owner.instances[index], 1);
            let parent = fixture.arena.links(old).unwrap().parent;
            let new = fixture
                .arena
                .create_owned_object_visual(crate::visual_replacement::LIT_OBJECT_VISUAL)
                .unwrap_or_else(|e| panic!("{:?}", e.error))
                .unwrap();
            fixture
                .arena
                .duplicate_owned_object_visual(new, old)
                .unwrap();
            fixture.arena.link_frame(new, parent, 0).unwrap();
            fixture.arena.link_frame(old, None, 0).unwrap();
            fixture.arena.delete_frame(model, old).unwrap();
            fixture.arena.add_frame(model, new).unwrap();
            fixture.arena.release(new).unwrap();
            let world_mesh = fixture.placements[0].world_frames[1];
            fixture.graph.set_on(world_mesh, 0).unwrap();
            owner
                .sync_from_graph(&mut fixture.arena, &fixture.graph)
                .unwrap();
            assert_eq!(mapped(&owner.instances[index], 1), new);
            assert!(fixture.arena.frame(old).is_err());
            assert_eq!(fixture.arena.state(new).unwrap().flags & 1, 0);
            fixture.graph.set_on(world_mesh, 1).unwrap();
            let world_dummy = fixture.placements[0].world_frames[0];
            fixture.graph.set_on(world_dummy, 0).unwrap();
            owner
                .sync_from_graph(&mut fixture.arena, &fixture.graph)
                .unwrap();
            assert_eq!(fixture.arena.state(new).unwrap().flags & 1, 1);
            assert_eq!(
                fixture
                    .arena
                    .state(mapped(&owner.instances[index], 0))
                    .unwrap()
                    .flags
                    & 1,
                0
            );
            fixture
                .graph
                .set_on(fixture.placements[0].world_root, 0)
                .unwrap();
            owner
                .sync_from_graph(&mut fixture.arena, &fixture.graph)
                .unwrap();
            assert_eq!(fixture.arena.state(model.root()).unwrap().flags & 1, 0);
            fixture.finish(&mut owner);
        });
    }
    #[test]
    fn foreign_actual_world_scene_and_arena_are_rejected_before_effects() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut fixture = Fixture::new();
            let mut owner = CanonicalCityCache::default();
            fixture.adopt(&mut owner).unwrap();
            let frames = fixture.arena.live_frame_count();
            let bytes = fixture.memory.live_bytes();
            let domain = fixture.domain;
            fixture.domain = DrivingWorldIdentity::fresh().unwrap();
            assert!(fixture.adopt(&mut owner).unwrap_err().contains("foreign"));
            fixture.domain = domain;
            assert_eq!(fixture.arena.live_frame_count(), frames);
            assert_eq!(fixture.memory.live_bytes(), bytes);
            assert!(!owner.interrupted());
            let original = fixture.scene;
            fixture.scene = fixture.arena.construct_scene_owner_prefix().unwrap();
            let frames = fixture.arena.live_frame_count();
            assert!(fixture.adopt(&mut owner).unwrap_err().contains("foreign"));
            assert_eq!(fixture.arena.live_frame_count(), frames);
            fixture.scene = original;
            let mut foreign = ModelArena::default();
            let scene = foreign.construct_scene_owner_prefix().unwrap();
            let frames = foreign.live_frame_count();
            assert!(owner
                .adopt_instances(
                    &mut Context {
                        arena: &mut foreign,
                        scene,
                        caches: &mut fixture.caches,
                        memory: &mut fixture.memory,
                        installation: &fixture.installation,
                        world_graph: &fixture.graph,
                        world_identity: fixture.domain
                    },
                    &fixture.placements
                )
                .unwrap_err()
                .contains("foreign"));
            assert_eq!(foreign.live_frame_count(), frames);
            assert!(owner.close(&mut foreign).is_err());
            assert!(!owner.interrupted());
            fixture.finish(&mut owner);
        });
    }
    #[test]
    fn different_actual_worlds_with_identical_table_ids_cannot_reuse_a_mapping() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut first = crate::driving_world_assets::tests::cache_world(0);
            let mut second = crate::driving_world_assets::tests::cache_world(0);
            let installation = Installation::open(env!("CARGO_MANIFEST_DIR")).unwrap();
            first
                .load_cache_render_near(&installation, [0.; 3], 10.)
                .unwrap();
            second
                .load_cache_render_near(&installation, [0.; 3], 10.)
                .unwrap();
            let a = &first.admitted_cache_instances().unwrap()[0];
            let b = &second.admitted_cache_instances().unwrap()[0];
            assert_eq!(a.id, b.id);
            assert_eq!(a.world_frames, b.world_frames);
            assert_ne!(first.identity(), second.identity());
            let mut fixture = Fixture::with_source(a.source.clone());
            let mut owner = CanonicalCityCache::default();
            owner
                .adopt(
                    &mut fixture.arena,
                    fixture.scene,
                    &mut fixture.caches,
                    &mut fixture.memory,
                    &fixture.installation,
                    &first,
                )
                .unwrap();
            let frames = fixture.arena.live_frame_count();
            let bytes = fixture.memory.live_bytes();
            assert!(owner
                .adopt(
                    &mut fixture.arena,
                    fixture.scene,
                    &mut fixture.caches,
                    &mut fixture.memory,
                    &fixture.installation,
                    &second
                )
                .unwrap_err()
                .contains("foreign"));
            assert!(owner
                .sync_visibility(&mut fixture.arena, &second)
                .unwrap_err()
                .contains("foreign"));
            assert_eq!(fixture.arena.live_frame_count(), frames);
            assert_eq!(fixture.memory.live_bytes(), bytes);
            assert!(!owner.interrupted());
            fixture.finish(&mut owner);
        });
    }
    #[test]
    fn entered_cache_copy_failure_keeps_real_pending_owner_and_forbids_replay() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut fixture = Fixture::new();
            let mut owner = CanonicalCityCache::default();
            let cached = source_frames(&fixture.arena, fixture.source_model).unwrap();
            fixture
                .arena
                .graph
                .assign_scale(cached[&0], [4.; 3])
                .unwrap();
            assert!(fixture.adopt(&mut owner).is_err());
            assert!(owner.interrupted());
            assert_eq!(owner.pending().len(), 1);
            let pending = owner.pending()[0].instance.model;
            assert_eq!(owner.pending()[0].phase, CanonicalCachePhase::Open);
            assert_eq!(fixture.arena.frame(pending.root()).unwrap().references, 1);
            assert!(owner.cache_open_failure().is_some());
            let progress = owner.cache_open_progress().unwrap();
            assert_eq!(progress.target, pending);
            assert_eq!(progress.cached_source, Some(fixture.source_model));
            let frames = fixture.arena.live_frame_count();
            let bytes = fixture.memory.live_bytes();
            assert!(fixture
                .adopt(&mut owner)
                .unwrap_err()
                .contains("cannot replay"));
            assert_eq!(fixture.arena.live_frame_count(), frames);
            assert_eq!(fixture.memory.live_bytes(), bytes);
            owner.close(&mut fixture.arena).unwrap();
            assert!(fixture.arena.frame(pending.root()).is_err());
            assert!(fixture.caches.is_interrupted(MissionCacheKind::Model));
            assert_eq!(
                fixture
                    .arena
                    .frame(fixture.source_model.root())
                    .unwrap()
                    .references,
                1
            );
            // The entered cache's interrupted source/name ownership is retained
            // by that real owner. No fake recovery/destructor success is claimed.
            assert!(fixture
                .caches
                .destruct(
                    MissionCacheKind::Model,
                    &mut fixture.arena,
                    &mut fixture.memory
                )
                .is_err());
            std::fs::remove_dir_all(&fixture.directory).unwrap();
        });
    }
    #[test]
    fn unsupported_preselection_is_recorded_without_creating_an_actual_model() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut fixture = Fixture::new();
            let mut owner = CanonicalCityCache::default();
            let mut unsupported = (*fixture.placements[0].source).clone();
            unsupported.frames[1].visual = Some(2);
            fixture.placements.truncate(1);
            fixture.placements[0].source = Arc::new(unsupported);
            let frames = fixture.arena.live_frame_count();
            let summary = fixture.adopt(&mut owner).unwrap();
            assert_eq!(summary.unsupported, 1);
            assert_eq!(summary.adopted, 0);
            assert_eq!(owner.unsupported().len(), 1);
            assert_eq!(fixture.arena.live_frame_count(), frames);
            assert!(owner.pending().is_empty());
            fixture.finish(&mut owner);
        });
    }
}
