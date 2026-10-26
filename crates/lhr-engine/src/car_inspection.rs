//! Bounded presentation of the same live arena initialized by C_car. View-aware
//! callers select actual owned LODs; legacy wrappers explicitly select LOD0.
//! Geometry comes from owned CPU buffers, including object-private LOD data.
//! Saved vertex lighting and an explicit static lightmap context are supported.
//! Dynamic lighting, envmaps, fog and native visibility remain separate work.
use crate::{
    frame_transform::{FrameKey, FrameKind},
    model_loading::ModelArena,
    scene_ownership::{SceneKey, SectorKey},
};
use lhr_render::{FrontFace, Mesh, Scene, Texture, Vertex};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const CAR_INSPECTION_MAX_BYTES: usize = 256 * 1024 * 1024;
pub const CAR_INSPECTION_MAX_DRAWS: usize = 4096;

/// Explicit host diagnostic rendering context. This is the caller's current
/// sector, not a reconstructed portal/visual-queue selection. Every lightmapped
/// frame is checked against this same arena, live scene and actual ancestry.
#[derive(Clone, Copy, Debug)]
pub struct StaticLightmapContext {
    pub scene: SceneKey,
    pub sector: SectorKey,
}

pub struct CarInspectionScene {
    pub scene: Scene,
    pub visited_frames: usize,
    pub visible_meshes: usize,
    pub omitted_material_effects: usize,
    pub unsupported_alpha_masks: usize,
    pub budgeted_bytes: usize,
    pub lod_culled_meshes: usize,
}
fn charge(total: &mut usize, bytes: usize) -> Result<(), String> {
    *total = total
        .checked_add(bytes)
        .filter(|n| *n <= CAR_INSPECTION_MAX_BYTES)
        .ok_or("Car inspection exceeds the geometry/texture budget")?;
    Ok(())
}
fn word(bytes: &[u8], offset: usize) -> Result<f32, String> {
    let bytes = bytes
        .get(offset..offset.checked_add(4).ok_or("Vertex offset overflow")?)
        .ok_or("Incomplete CPU vertex")?;
    let value = f32::from_bits(u32::from_le_bytes(bytes.try_into().unwrap()));
    value
        .is_finite()
        .then_some(value)
        .ok_or("Nonfinite CPU vertex".into())
}
struct InspectionAlpha<'a> {
    mask: Option<&'a lhr_assets::Texture>,
    keyed: bool,
    omitted: bool,
}
impl<'a> InspectionAlpha<'a> {
    fn select(
        color: Option<&lhr_assets::Texture>,
        mask: Option<&'a lhr_assets::Texture>,
        keyed: bool,
    ) -> Result<Self, String> {
        let Some(mask) = mask else {
            return Ok(Self {
                mask: None,
                keyed,
                omitted: false,
            });
        };
        let color = color.ok_or("Car alpha image has no base image")?;
        let size = (color.width as usize)
            .checked_mul(color.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("Car alpha image extent overflow")?;
        if (mask.width, mask.height) != (color.width, color.height) || mask.rgba.len() != size {
            return Err("Car alpha image extent is unavailable".into());
        }
        let gray = mask
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[0] == pixel[1] && pixel[1] == pixel[2]);
        // Unknown mask-channel semantics are an omitted presentation effect,
        // independent of whether the reconstructed car initialized correctly.
        Ok(Self {
            mask: gray.then_some(mask),
            keyed: false,
            omitted: !gray,
        })
    }
    fn alpha(&self, index: usize, color: &[u8; 4]) -> u8 {
        self.mask.map_or_else(
            || if self.keyed { color[3] } else { 255 },
            |mask| mask.rgba[index * 4],
        )
    }
}
/// Captures live buffer data and current world transforms without constructing
/// a second model graph. The original CPU owner must stay alive until capture
/// or presentation completes; this result owns only renderer upload data.
pub fn snapshot_car_arena(
    arena: &mut ModelArena,
    root: FrameKey,
) -> Result<CarInspectionScene, String> {
    if arena.state(root).map_err(|e| e.to_string())?.kind != FrameKind::Model {
        return Err("Car inspection requires its initialized model root".into());
    }
    let snapshot = snapshot_arena_tree(arena, root)?;
    if snapshot.scene.meshes.is_empty() {
        return Err("Initialized car has no supported visible CPU geometry".into());
    }
    Ok(snapshot)
}
/// Bootstrap the host camera from the actual PC hierarchy bounds. This
/// performs the same Z reflection as geometry intake, including min/max swap.
pub fn camera_from_arena_bounds(
    arena: &mut ModelArena,
    root: FrameKey,
) -> Result<lhr_render::Camera, String> {
    let bound = arena.hr_bound_volume_w(root).map_err(|e| e.to_string())?;
    lhr_render::Camera::fit_bounds(
        [bound.bbox[0][0], bound.bbox[0][1], -bound.bbox[1][2]],
        [bound.bbox[1][0], bound.bbox[1][1], -bound.bbox[0][2]],
    )
}

/// Camera-driven ordinary live LOD snapshot. A model whose meshes are all
/// source-distance-culled is a valid empty draw set, distinct from no CPU mesh.
pub fn snapshot_car_arena_with_view(
    arena: &mut ModelArena,
    root: FrameKey,
    view: crate::inspection_lod::InspectionLodView,
) -> Result<CarInspectionScene, String> {
    if arena.state(root).map_err(|e| e.to_string())?.kind != FrameKind::Model {
        return Err("Car inspection requires its initialized model root".into());
    }
    let snapshot = snapshot_arena_tree_with_view(arena, root, view)?;
    if snapshot.scene.meshes.is_empty() && snapshot.lod_culled_meshes == 0 {
        return Err("Initialized car has no admitted drawable CPU geometry".into());
    }
    Ok(snapshot)
}
pub fn snapshot_arena_tree_with_view(
    arena: &mut ModelArena,
    root: FrameKey,
    view: crate::inspection_lod::InspectionLodView,
) -> Result<CarInspectionScene, String> {
    snapshot_arena_tree_inner(arena, root, Some(view), None)
}

/// HWTL/single-pass saved-lightmap inspection for an explicitly supplied
/// current sector. Only its empty-light, zero-emission domain is implemented;
/// this does not select visible sectors or substitute for the native renderer.
pub fn snapshot_arena_tree_with_view_and_lighting(
    arena: &mut ModelArena,
    root: FrameKey,
    view: crate::inspection_lod::InspectionLodView,
    lighting: StaticLightmapContext,
) -> Result<CarInspectionScene, String> {
    snapshot_arena_tree_inner(arena, root, Some(view), Some(lighting))
}

/// Snapshot the actual visible subtree, including sector-owned car effects.
/// Unlike car inspection, an empty visible tree is a valid presentation state.
/// This legacy diagnostic wrapper explicitly chooses LOD0. Main presentation
/// uses the view-aware entry point for selected buffers and source fade.
pub fn snapshot_arena_tree(
    arena: &mut ModelArena,
    root: FrameKey,
) -> Result<CarInspectionScene, String> {
    snapshot_arena_tree_inner(arena, root, None, None)
}
fn snapshot_arena_tree_inner(
    arena: &mut ModelArena,
    root: FrameKey,
    view: Option<crate::inspection_lod::InspectionLodView>,
    lighting: Option<StaticLightmapContext>,
) -> Result<CarInspectionScene, String> {
    let mut lod_culled_meshes = 0;
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    let mut frames = Vec::new();
    while let Some(frame) = pending.pop() {
        if !seen.insert(frame.0) || seen.len() > 65536 {
            return Err("Car inspection hierarchy is cyclic or exceeds its frame budget".into());
        }
        let state = *arena.state(frame).map_err(|e| e.to_string())?;
        if state.flags & 1 == 0 {
            continue;
        }
        let mut child = arena.links(frame).map_err(|e| e.to_string())?.child;
        let mut siblings = BTreeSet::new();
        while let Some(key) = child {
            if !siblings.insert(key.0) || siblings.len() > 65536 {
                return Err("Car inspection child links are cyclic".into());
            }
            child = arena.links(key).map_err(|e| e.to_string())?.next;
            pending.push(key);
        }
        if state.kind == FrameKind::Mesh {
            if view.is_some()
                && (state.flags & 0x400 != 0 || state.visual_flags & (0x10 | 0x40 | 0x10000) != 0)
            {
                return Err(
                    "Camera LOD callback/texture-driven/world-space/proxy domain unavailable"
                        .into(),
                );
            }
            let object = arena
                .frame_object_resources(frame)
                .map_err(|e| e.to_string())?
                .ok_or("Car mesh has no owned CPU resources")?;
            let matrix = arena.world_matrix(frame).map_err(|e| e.to_string())?;
            let bound = arena.hr_bound_volume_w(frame).map_err(|e| e.to_string())?;
            let center = [bound.center[0], bound.center[1], -bound.center[2]];
            let (lod_index, fade) = if let Some(view) = view {
                let own = arena
                    .visual_world_bounds(frame)
                    .map_err(|e| e.to_string())?;
                let distance_squared = view.distance_squared(own.center)?;
                let world_scale = arena.state(frame).map_err(|e| e.to_string())?.world_scale;
                let selected = arena
                    .loaded_visual_resources_mut()
                    .ok_or("LOD CPU resource owner absent")?
                    .select_render_lod(
                        object,
                        distance_squared,
                        view.rs_lod_quality,
                        world_scale,
                        state.flags,
                        state.visual_flags,
                    )
                    .map_err(|e| format!("Live object LOD: {e:?}"))?;
                match selected {
                    crate::loaded_visual_resources::RenderLodSelection::Selected {
                        index,
                        fade,
                    } => (index, fade),
                    crate::loaded_visual_resources::RenderLodSelection::Culled { .. } => {
                        lod_culled_meshes += 1;
                        continue;
                    }
                    crate::loaded_visual_resources::RenderLodSelection::Missing { .. } => continue,
                }
            } else {
                (0, 1.)
            };
            frames.push((frame, object, matrix, center, lod_index, fade));
        } else if matches!(state.kind, FrameKind::Singlemesh) {
            return Err("Car inspection cannot substitute undeformed singlemesh vertices".into());
        }
    }
    if frames.is_empty() {
        return Ok(CarInspectionScene {
            scene: Scene { meshes: Vec::new() },
            visited_frames: seen.len(),
            visible_meshes: 0,
            omitted_material_effects: 0,
            unsupported_alpha_masks: 0,
            budgeted_bytes: 0,
            lod_culled_meshes,
        });
    }
    let resources = arena
        .loaded_visual_resources()
        .ok_or("Car CPU resource owner unavailable")?;
    let mut bytes = 0usize;
    let mut meshes = Vec::new();
    let mut textures = BTreeMap::<(u64, u32), Arc<Texture>>::new();
    let mut atlases = BTreeMap::new();
    let mut omitted = 0usize;
    let mut unsupported_alpha_masks = 0usize;
    for (frame, object, world, center, lod_index, fade) in &frames {
        let Some(view) = resources
            .inspection_mesh_at_lod(*object, *lod_index, CAR_INSPECTION_MAX_DRAWS - meshes.len())
            .map_err(|e| format!("Live car CPU geometry: {e:?}"))?
        else {
            continue;
        };
        let lightmap_texture = if let Some(lightmap) = &view.lightmap {
            validate_static_lightmap_context(
                arena,
                *frame,
                lighting
                    .ok_or("Saved texture lightmaps require an explicit current-sector context")?,
            )?;
            let first = view
                .groups
                .first()
                .ok_or("Saved lightmap has no first material")?
                .material;
            // PrepareLights starts with zero ambient, then uses the FIRST face
            // group's material. Even with no sector lights it adds emissive.
            if first.emissive != [0.; 3] || !first.ambient.into_iter().all(f32::is_finite) {
                return Err(
                    "Saved lightmap dynamic lighting/emission is outside the static profile".into(),
                );
            }
            if view.uv_offset != [0.; 2] || view.vertex_stride != 32 {
                return Err(
                    "Saved lightmap private layout or animated UV offset is unavailable".into(),
                );
            }
            if let Some(texture) = atlases.get(&lightmap.atlas) {
                Some(Arc::clone(texture))
            } else {
                let source = lightmap.texture;
                let size = (source.width as usize)
                    .checked_mul(source.height as usize)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or("Saved lightmap texture size overflow")?;
                if source.width == 0 || source.height == 0 || source.rgba.len() != size {
                    return Err("Saved lightmap CPU atlas is incomplete".into());
                }
                charge(&mut bytes, size)?;
                let mut rgba = Vec::new();
                rgba.try_reserve_exact(size)
                    .map_err(|_| "Saved lightmap snapshot allocation unavailable")?;
                rgba.extend_from_slice(&source.rgba);
                let texture = Arc::new(Texture {
                    width: source.width,
                    height: source.height,
                    rgba,
                    // Source stage 0 keeps the driver's default WRAP state.
                    repeat: [true; 2],
                });
                atlases.insert(lightmap.atlas, Arc::clone(&texture));
                Some(texture)
            }
        } else {
            None
        };
        for group in view.groups {
            if meshes.len() >= CAR_INSPECTION_MAX_DRAWS {
                return Err("Car inspection exceeds its draw budget".into());
            }
            let vertex_count = view.vertex_count as usize;
            let index_count = group.index_bytes.len() / 2;
            charge(
                &mut bytes,
                vertex_count
                    .checked_mul(std::mem::size_of::<Vertex>())
                    .and_then(|n| index_count.checked_mul(4).and_then(|i| n.checked_add(i)))
                    .ok_or("Car geometry size overflow")?,
            )?;
            let lit = view.visual_type == crate::i3d_object::ObjectVisualType::Lit;
            let material = group.material;
            let has_texture = material.flags & 0x40000 != 0 && group.texture.is_some();
            let effective_texture = has_texture.then_some(group.texture).flatten();
            let alpha = InspectionAlpha::select(
                effective_texture,
                has_texture.then_some(group.texture_alpha).flatten(),
                has_texture && group.inspection_rgba8_color_key,
            )?;
            let policy = if view.lightmap.is_some() {
                crate::presentation_material::saved_texture_lit_material(
                    material,
                    has_texture,
                    *center,
                    FrontFace::CounterClockwise,
                )?
            } else if view.saved_vertex_colors.is_some() {
                crate::presentation_material::saved_vertex_lit_material(
                    material,
                    has_texture,
                    *center,
                    FrontFace::CounterClockwise,
                )?
            } else if lit {
                crate::presentation_material::unconstructed_lit_material(
                    material,
                    has_texture,
                    *center,
                    FrontFace::CounterClockwise,
                )?
            } else {
                crate::presentation_material::live_material_with_fade(
                    material,
                    has_texture,
                    *center,
                    FrontFace::CounterClockwise,
                    *fade,
                )?
            };
            if alpha.omitted {
                unsupported_alpha_masks += 1;
            }
            if !lit
                && (alpha.omitted
                    || material.env_texture.is_some()
                    || material.emissive != [0.; 3]
                    || material.flags & (0x80000 | 0x01000000 | 0x04000000) != 0)
            {
                omitted += 1;
            }
            if !material.diffuse.into_iter().all(f32::is_finite)
                || !view.uv_offset.into_iter().all(f32::is_finite)
            {
                return Err("Car material or UV offset is nonfinite".into());
            }
            let key = (
                if has_texture {
                    group.texture_identity.map_or(0, |key| key.0)
                } else {
                    0
                },
                (material.flags & 0x30000) | if lit { 0x80000000 } else { 0 },
            );
            let texture = if let Some(texture) = textures.get(&key) {
                texture.clone()
            } else {
                let (width, height, source) = match effective_texture {
                    Some(texture) => (texture.width, texture.height, texture.rgba.as_slice()),
                    None => (1, 1, &[255, 255, 255, 255][..]),
                };
                let size = (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or("Car texture size overflow")?;
                if source.len() != size || width == 0 || height == 0 {
                    return Err("Car CPU texture is incomplete".into());
                }
                charge(&mut bytes, size)?;
                let mut rgba = Vec::new();
                rgba.try_reserve_exact(size)
                    .map_err(|_| "Car texture allocation unavailable")?;
                for (index, pixel) in source.as_chunks::<4>().0.iter().enumerate() {
                    rgba.extend_from_slice(&pixel[..3]);
                    rgba.push(if lit {
                        pixel[3]
                    } else {
                        alpha.alpha(index, pixel)
                    });
                }
                let texture = Arc::new(Texture {
                    width,
                    height,
                    rgba,
                    repeat: [material.flags & 0x10000 == 0, material.flags & 0x20000 == 0],
                });
                textures.insert(key, texture.clone());
                texture
            };
            let mut vertices = Vec::new();
            vertices
                .try_reserve_exact(vertex_count)
                .map_err(|_| "Car vertex allocation unavailable")?;
            if !matches!(view.vertex_stride, 24 | 32)
                || view.vertex_bytes.len() != vertex_count * view.vertex_stride
                || view
                    .saved_vertex_colors
                    .is_some_and(|colors| colors.len() != vertex_count)
            {
                return Err("Car vertex lighting layout is inconsistent".into());
            }
            for (index, vertex) in view
                .vertex_bytes
                .chunks_exact(view.vertex_stride)
                .enumerate()
            {
                let point = [word(vertex, 0)?, word(vertex, 4)?, word(vertex, 8)?];
                let [x, y, z] = world
                    .try_transform_point(point)
                    .map_err(|_| "Car world transform outside verified arithmetic domain")?;
                let uv = [
                    word(vertex, view.uv_byte_offset)? + view.uv_offset[0],
                    word(vertex, view.uv_byte_offset + 4)? + view.uv_offset[1],
                ];
                if !uv.into_iter().all(f32::is_finite) {
                    return Err("Car UV transform overflow".into());
                }
                let lightmap_uv = if let Some(lightmap) = &view.lightmap {
                    [
                        word(vertex, lightmap.uv_byte_offset)?,
                        word(vertex, lightmap.uv_byte_offset + 4)?,
                    ]
                } else {
                    [0.; 2]
                };
                vertices.push(Vertex {
                    lightmap_uv,
                    color: if view.lightmap.is_some() {
                        [0.; 3]
                    } else {
                        view.saved_vertex_colors.map_or([1.; 3], |colors| {
                            let argb = colors[index];
                            [
                                ((argb >> 16) & 255) as f32 / 255.,
                                ((argb >> 8) & 255) as f32 / 255.,
                                (argb & 255) as f32 / 255.,
                            ]
                        })
                    },
                    position: [x, y, -z],
                    uv,
                });
            }
            let mut indices = Vec::new();
            indices
                .try_reserve_exact(index_count)
                .map_err(|_| "Car index allocation unavailable")?;
            for index in group.index_bytes.as_chunks::<2>().0 {
                let index = u16::from_le_bytes(*index) as u32;
                if index >= view.vertex_count {
                    return Err("Car index exceeds live vertex storage".into());
                }
                indices.push(index);
            }
            meshes.push(Mesh {
                lightmap: lightmap_texture.clone(),
                material: policy,
                vertices,
                indices,
                texture: Some(texture),
            });
        }
    }
    Ok(CarInspectionScene {
        scene: Scene { meshes },
        visited_frames: seen.len(),
        visible_meshes: frames.len(),
        omitted_material_effects: omitted,
        unsupported_alpha_masks,
        budgeted_bytes: bytes,
        lod_culled_meshes,
    })
}

fn validate_static_lightmap_context(
    arena: &ModelArena,
    frame: FrameKey,
    context: StaticLightmapContext,
) -> Result<(), String> {
    let closest = arena
        .graph
        .closest_sector(Some(frame))
        .map_err(|e| format!("Lightmap sector ancestry: {e:?}"))?;
    if closest != Some(context.sector.frame()) {
        return Err("Lightmap context does not match the frame's actual closest sector".into());
    }
    let sector = arena.sector(context.sector).map_err(|e| e.to_string())?;
    if sector.scene != Some(context.scene) {
        return Err("Lightmap sector belongs to a different scene".into());
    }
    arena
        .check_owner_available(context.scene.into())
        .map_err(|e| e.to_string())?;
    let scene = arena.scene(context.scene).map_err(|e| e.to_string())?;
    if scene.sectors[3] == context.sector {
        return Err("Near-sector lightmaps require the actual current camera sector".into());
    }
    if !sector.lights.is_empty() {
        return Err("Saved lightmap dynamic sector lights are outside the static profile".into());
    }
    // The supplied context must not hide a cross-scene or cyclic hierarchy.
    let mut at = arena
        .links(context.sector.frame())
        .map_err(|e| e.to_string())?
        .parent;
    let mut seen = BTreeSet::new();
    while let Some(ancestor) = at {
        if !seen.insert(ancestor.0) || seen.len() > 65536 {
            return Err("Lightmap sector ancestry is cyclic or exceeds its budget".into());
        }
        if arena.state(ancestor).map_err(|e| e.to_string())?.kind == FrameKind::Sector
            && arena
                .sector(SectorKey(ancestor))
                .map_err(|e| e.to_string())?
                .scene
                != Some(context.scene)
        {
            return Err("Lightmap nested sector scene association is inconsistent".into());
        }
        at = arena.links(ancestor).map_err(|e| e.to_string())?.parent;
    }
    Ok(())
}

#[cfg(test)]
#[path = "lightmap_presentation_tests.rs"]
mod lightmap_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_arena_materials_share_pixels_and_preserve_diffuse_opacity_and_world_center() {
        use crate::model_loading::tests::frame;
        use lhr_render::{AlphaMode, MaterialQueue, RenderMaterial};
        let path = std::env::temp_dir().join(format!(
            "lhr-material-bridge-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("maps")).unwrap();
        let mut bmp = vec![0u8; 58];
        bmp[..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&1i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&1i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp[54..57].fill(255);
        std::fs::write(path.join("maps/white.bmp"), bmp).unwrap();
        let material = |flags, opacity| lhr_assets::Material {
            flags,
            ambient: [0.; 3],
            diffuse: [0.25, 0.5, 1.],
            texture: "white.bmp".into(),
            emission: [0.; 3],
            opacity,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        };
        let mut visual = frame(1, None, [10., 20., 30.]);
        visual.render_flags = 0;
        visual.logic_flags = 0;
        visual.lods.push(lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., -1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![
                lhr_assets::FaceGroup {
                    material: 0,
                    indices: vec![0, 1, 2],
                },
                lhr_assets::FaceGroup {
                    material: 1,
                    indices: vec![0, 1, 2],
                },
            ],
        });
        let source = Arc::new(lhr_assets::Model {
            // Stage 1's KEEP_PIXELS storage flag remains compatible with the
            // live material path and must not count as an omitted draw effect.
            materials: vec![material(0x48000, 1.), material(0x08048000, 0.5)],
            frames: vec![visual],
        });
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let model = arena.create_model().unwrap();
        arena.scene_add_frame(scene, model.root()).unwrap();
        arena.load_parsed_into_model(model, source).unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        let camera = camera_from_arena_bounds(&mut arena, model.root()).unwrap();
        assert_eq!(camera.target.to_array(), [10., 20., -30.]);
        assert!(camera.eye.z > -30.);
        let snapshot = snapshot_car_arena(&mut arena, model.root()).unwrap();
        assert_eq!(snapshot.scene.meshes.len(), 2);
        let [a, b] = &snapshot.scene.meshes[..] else {
            panic!()
        };
        assert!(Arc::ptr_eq(
            a.texture.as_ref().unwrap(),
            b.texture.as_ref().unwrap()
        ));
        assert_eq!(a.texture.as_ref().unwrap().rgba, [255; 4]); // diffuse never baked into texture storage
        let RenderMaterial::Original(a) = a.material else {
            panic!()
        };
        let RenderMaterial::Original(b) = b.material else {
            panic!()
        };
        assert_eq!(a.color, [1.; 4]);
        assert_eq!(b.color, [0.25, 0.5, 1., 128. / 255.]);
        assert_eq!(b.sort_center, [10., 20., -30.]);
        assert_eq!(
            (b.alpha, b.queue),
            (AlphaMode::Blend, MaterialQueue::Transparent)
        );
        assert_eq!(snapshot.omitted_material_effects, 0);
        arena.close(model).unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn empty_visible_trees_are_valid_while_car_inspection_keeps_its_model_gate() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let snapshot = snapshot_arena_tree(&mut arena, model.root()).unwrap();
        assert_eq!(snapshot.visited_frames, 1);
        assert!(snapshot.scene.meshes.is_empty());
        assert!(snapshot_car_arena(&mut arena, model.root()).is_err());
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0].frame();
        assert!(snapshot_arena_tree(&mut arena, primary)
            .unwrap()
            .scene
            .meshes
            .is_empty());
        assert!(snapshot_car_arena(&mut arena, primary).is_err());
    }
    #[test]
    fn non_gray_optional_mask_is_opaque_and_reported_without_guessing_a_channel() {
        let color = lhr_assets::Texture {
            width: 2,
            height: 1,
            rgba: vec![40, 80, 120, 0, 100, 150, 200, 0],
        };
        let mask = lhr_assets::Texture {
            width: 2,
            height: 1,
            rgba: vec![0, 255, 0, 255, 255, 0, 255, 0],
        };
        let policy = InspectionAlpha::select(Some(&color), Some(&mask), false).unwrap();
        assert!(policy.omitted);
        assert_eq!(
            color
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .map(|(i, pixel)| policy.alpha(i, pixel))
                .collect::<Vec<_>>(),
            [255, 255]
        );
        let gray = lhr_assets::Texture {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 255, 255, 255, 255, 0],
        };
        let policy = InspectionAlpha::select(Some(&color), Some(&gray), false).unwrap();
        assert!(!policy.omitted);
        assert_eq!(
            [
                policy.alpha(0, &[40, 80, 120, 255]),
                policy.alpha(1, &[100, 150, 200, 255])
            ],
            [0, 255]
        );
    }
}
