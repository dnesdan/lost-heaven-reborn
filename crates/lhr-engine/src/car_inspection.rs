//! Unlit inspection of the same live arena initialized by C_car. View-aware
//! callers select actual owned LODs; legacy wrappers explicitly select LOD0.
//! Geometry comes from owned CPU buffers, including object-private LOD data.
//! Live source material decisions are explicit; lighting/envmaps/fog and full
//! original renderer parity remain outside this unlit presentation bridge.
use crate::presentation_material::{source_ccw_cull_front_face, SourceIndexOrder};
use crate::{
    frame_transform::{FrameKey, FrameKind},
    model_loading::ModelArena,
};
use lhr_render::{Mesh, Scene, Texture, Vertex};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const CAR_INSPECTION_MAX_BYTES: usize = 256 * 1024 * 1024;
pub const CAR_INSPECTION_MAX_DRAWS: usize = 4096;

pub struct CarInspectionScene {
    pub scene: Scene,
    /// Canonical frame identity for each materialized GPU draw.
    pub draw_frames: Vec<FrameKey>,
    pub lighting: Vec<Option<lhr_render::MeshLighting>>,
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
    snapshot_arena_tree_inner(arena, &[root], true, Some(view), None)
}

/// Snapshot the actual visible subtree, including sector-owned car effects.
/// Unlike car inspection, an empty visible tree is a valid presentation state.
/// This legacy diagnostic wrapper explicitly chooses LOD0. Main presentation
/// uses the view-aware entry point for selected buffers and source fade.
pub fn snapshot_arena_tree(
    arena: &mut ModelArena,
    root: FrameKey,
) -> Result<CarInspectionScene, String> {
    snapshot_arena_tree_inner(arena, &[root], true, None, None)
}
/// Materialize only the visual frames selected by canonical Scene visibility.
/// Descendants are not implicitly included; each input has already passed the
/// source sector, frustum and occlusion decisions.
pub fn snapshot_selected_frames_with_view(
    arena: &mut ModelArena,
    frames: &[FrameKey],
    view: crate::inspection_lod::InspectionLodView,
) -> Result<CarInspectionScene, String> {
    snapshot_arena_tree_inner(arena, frames, false, Some(view), None)
}
/// Canonical Scene presentation borrows its active Camera owner for each
/// billboard matrix. Standalone inspection has no camera owner and stops when
/// it reaches a billboard instead of presenting it as an ordinary object.
pub fn snapshot_selected_frames_with_view_and_camera(
    arena: &mut ModelArena,
    frames: &[FrameKey],
    view: crate::inspection_lod::InspectionLodView,
    camera: crate::frame_camera::CameraKey,
) -> Result<CarInspectionScene, String> {
    snapshot_arena_tree_inner(arena, frames, false, Some(view), Some(camera))
}
fn snapshot_arena_tree_inner(
    arena: &mut ModelArena,
    roots: &[FrameKey],
    recursive: bool,
    view: Option<crate::inspection_lod::InspectionLodView>,
    camera: Option<crate::frame_camera::CameraKey>,
) -> Result<CarInspectionScene, String> {
    let mut lod_culled_meshes = 0;
    let mut pending: Vec<_> = roots.iter().copied().rev().collect();
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
        let mut child = if recursive {
            arena.links(frame).map_err(|e| e.to_string())?.child
        } else {
            None
        };
        let mut siblings = BTreeSet::new();
        while let Some(key) = child {
            if !siblings.insert(key.0) || siblings.len() > 65536 {
                return Err("Car inspection child links are cyclic".into());
            }
            child = arena.links(key).map_err(|e| e.to_string())?.next;
            pending.push(key);
        }
        if matches!(state.kind, FrameKind::Mesh | FrameKind::Singlemesh) {
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
            let visual_type = arena
                .loaded_visual_resources()
                .ok_or("Car CPU resource owner unavailable")?
                .objects()
                .object(object)
                .map_err(|e| format!("Live visual class frame {frame:?}: {e:?}"))?
                .visual_type();
            let billboard = visual_type == crate::i3d_object::ObjectVisualType::Billboard;
            // Ordinary presentation keeps its historical world read here.
            // Billboard bounds/LOD may return early; its render matrix is read
            // only at ProductBillMatrix after a drawable LOD is prepared.
            let ordinary_world = if billboard {
                None
            } else {
                Some(arena.world_matrix(frame).map_err(|e| e.to_string())?)
            };
            let bound = arena.hr_bound_volume_w(frame).map_err(|e| e.to_string())?;
            let center = [bound.center[0], bound.center[1], -bound.center[2]];
            let (lod_index, fade) = if let Some(view) = view {
                let own = arena
                    .visual_world_bounds(frame)
                    .map_err(|e| e.to_string())?;
                let distance_squared = view.distance_squared(own.center)?;
                let world_scale = arena.state(frame).map_err(|e| e.to_string())?.world_scale;
                let resources = arena
                    .loaded_visual_resources_mut()
                    .ok_or("LOD CPU resource owner absent")?;
                let selected = if billboard {
                    resources.select_billboard_render_lod(
                        object,
                        distance_squared,
                        view.rs_lod_quality,
                        world_scale,
                        state.flags,
                        state.visual_flags,
                    )
                } else {
                    resources.select_render_lod(
                        object,
                        distance_squared,
                        view.rs_lod_quality,
                        world_scale,
                        state.flags,
                        state.visual_flags,
                    )
                }
                .map_err(|e| format!("Live visual LOD frame {frame:?}: {e:?}"))?;
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
            if state.kind == FrameKind::Singlemesh {
                arena
                    .prepare_owned_selected_singlemesh_lod(frame, lod_index)
                    .map_err(|e| format!("Selected singlemesh pose frame {frame:?}: {e:?}"))?;
            }
            arena
                .prepare_owned_selected_morph_lod(frame, lod_index)
                .map_err(|e| format!("Selected morph geometry frame {frame:?}: {e:?}"))?;
            arena
                .loaded_visual_resources_mut()
                .ok_or("Saved lighting resource owner unavailable")?
                .prepare_saved_lit_storage(object, lod_index)
                .map_err(|e| {
                    format!(
                        "Selected saved lighting preparation frame {frame:?} LOD{lod_index}: {e:?}"
                    )
                })?;
            // Source UVs in this admitted static subset are already installed
            // in the selected LOD. ProductBillMatrix uses the current Camera
            // after LOD/storage preparation and before any lighting or vertex
            // transform; the canonical world matrix remains owned by the frame.
            let matrix = if billboard {
                let camera = camera.ok_or("Billboard requires active Scene camera")?;
                arena
                    .billboard_render_matrix(frame, camera)
                    .map_err(|e| format!("Billboard render matrix frame {frame:?}: {e:?}"))?
            } else {
                ordinary_world.ok_or("Ordinary visual world matrix absent")?
            };
            let (no_dynamic_lm, null_colors, billboard_colors) = if billboard {
                // The selected source vertex records already own static UVs.
                // Borrow only their local position/normal words, then release
                // that borrow before PrepareLights reads live Scene light owners.
                let (positions, normals) = {
                    let resources = arena
                        .loaded_visual_resources()
                        .ok_or("Billboard selected resource owner unavailable")?;
                    let view = resources
                        .inspection_mesh_at_lod(object, lod_index, CAR_INSPECTION_MAX_DRAWS)
                        .map_err(|e| format!("Billboard selected geometry: {e:?}"))?
                        .ok_or("Billboard selected LOD has no geometry")?;
                    let mut positions = Vec::new();
                    let mut normals = Vec::new();
                    for vertex in view.vertex_bytes.chunks_exact(view.vertex_stride) {
                        positions.push([word(vertex, 0)?, word(vertex, 4)?, word(vertex, 8)?]);
                        normals.push([word(vertex, 12)?, word(vertex, 16)?, word(vertex, 20)?]);
                    }
                    if positions.len() != view.vertex_count as usize {
                        return Err("Billboard selected vertex extent unavailable".into());
                    }
                    (positions, normals)
                };
                let sector = containing_lighting_sector(arena, frame)?
                    .ok_or("Billboard lighting sector unavailable")?;
                let colors = crate::null_material_lighting::billboard_vertex_lighting(
                    arena, frame, sector, matrix, &positions, &normals, fade,
                )
                .map_err(|e| format!("Billboard draw-matrix lighting: {e}"))?;
                (true, None, Some(colors))
            } else {
                (
                    no_dynamic_lm_lights(arena, frame)?,
                    prepare_null_colors(arena, frame, object, lod_index, fade)?,
                    None,
                )
            };
            frames.push((
                frame,
                object,
                matrix,
                center,
                lod_index,
                fade,
                no_dynamic_lm,
                null_colors,
                billboard_colors,
            ));
        }
    }
    if frames.is_empty() {
        return Ok(CarInspectionScene {
            scene: Scene { meshes: Vec::new() },
            draw_frames: Vec::new(),
            lighting: Vec::new(),
            visited_frames: seen.len(),
            visible_meshes: 0,
            omitted_material_effects: 0,
            unsupported_alpha_masks: 0,
            budgeted_bytes: 0,
            lod_culled_meshes,
        });
    }
    let mut bytes = 0usize;
    let mut meshes = Vec::new();
    let mut draw_frames = Vec::new();
    let mut lighting = Vec::new();
    let mut atlas_textures = BTreeMap::<(usize, u64), Arc<Texture>>::new();
    let mut textures = BTreeMap::<(u64, u32), Arc<Texture>>::new();
    let mut omitted = 0usize;
    let mut unsupported_alpha_masks = 0usize;
    for (
        frame,
        object,
        world,
        center,
        lod_index,
        fade,
        no_dynamic_lm,
        null_colors,
        billboard_colors,
    ) in &frames
    {
        let material_accesses = {
            let resources = arena
                .loaded_visual_resources()
                .ok_or("Car CPU resource owner unavailable")?;
            let Some(view) = resources
                .inspection_mesh_at_lod_for_group(
                    *object,
                    *lod_index,
                    CAR_INSPECTION_MAX_DRAWS - meshes.len(),
                    usize::MAX,
                )
                .map_err(|e| {
                    format!("Canonical CPU geometry frame {frame:?} LOD{lod_index}: {e:?}")
                })?
            else {
                continue;
            };
            view.groups
                .iter()
                .map(|group| {
                    group
                        .material_key
                        .filter(|_| group.material.is_some_and(|m| m.flags & 0x40000 != 0))
                })
                .collect::<Vec<_>>()
        };
        for (group_number, material_key) in material_accesses.into_iter().enumerate() {
            if let Some(key) = material_key {
                arena.access_material_texture(key).map_err(|e| {
                    format!("Live material GetTexture frame {frame:?} group {group_number}: {e:?}")
                })?;
            }
            let resources = arena
                .loaded_visual_resources()
                .ok_or("Car CPU resource owner unavailable")?;
            let view = resources
                .inspection_mesh_at_lod_for_group(
                    *object,
                    *lod_index,
                    CAR_INSPECTION_MAX_DRAWS,
                    group_number,
                )
                .map_err(|e| format!("Live material geometry frame {frame:?}: {e:?}"))?
                .ok_or("Live selected geometry disappeared")?;
            let billboard = view.visual_type == crate::i3d_object::ObjectVisualType::Billboard;
            let pose = if matches!(
                view.visual_type,
                crate::i3d_object::ObjectVisualType::Singlemesh
                    | crate::i3d_object::ObjectVisualType::Singlemorph
            ) {
                let pose = arena
                    .owned_singlemesh_selected_vertices(*frame, *lod_index)
                    .map_err(|e| format!("Selected singlemesh pose frame {frame:?}: {e:?}"))?;
                if pose.len() != view.vertex_count as usize {
                    return Err(format!(
                        "Selected singlemesh pose/geometry extent frame {frame:?}"
                    ));
                }
                Some(pose)
            } else {
                None
            };
            let morph_positions = if view.visual_type == crate::i3d_object::ObjectVisualType::Morph
            {
                let positions = resources
                    .morph_render_positions(*object, *lod_index)
                    .map_err(|e| {
                        format!("Selected morph render positions frame {frame:?}: {e:?}")
                    })?;
                if positions.len() != view.vertex_count as usize {
                    return Err("Selected morph render position extent unavailable".into());
                }
                Some(positions)
            } else {
                None
            };
            let saved = resources
                .saved_lit_lod(*object, *lod_index)
                .map_err(|e| format!("Saved lighting view: {e:?}"))?;
            let managed_colors = view
                .color_byte_offset
                .map(|offset| {
                    view.vertex_bytes
                        .chunks_exact(view.vertex_stride)
                        .map(|v| {
                            let raw = v
                                .get(offset..offset + 4)
                                .ok_or("Incomplete managed diffuse")?;
                            let c = u32::from_le_bytes(raw.try_into().unwrap());
                            Ok([
                                ((c >> 16) & 255) as f32 / 255.,
                                ((c >> 8) & 255) as f32 / 255.,
                                (c & 255) as f32 / 255.,
                                ((c >> 24) & 255) as f32 / 255.,
                            ])
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()?;
            let managed_uv2 = view
                .lightmap_uv_byte_offset
                .map(|offset| {
                    view.vertex_bytes
                        .chunks_exact(view.vertex_stride)
                        .map(|v| Ok([word(v, offset)?, word(v, offset + 4)?]))
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()?;
            let object_vertex_alpha = if !billboard
                && view.visual_type != crate::i3d_object::ObjectVisualType::LitUnconstructed
            {
                Some(crate::presentation_material::packed_object_vertex_alpha(
                    view.groups.first().and_then(|group| group.material),
                    *fade,
                )?)
            } else {
                None
            };
            let group = &view.groups[group_number];
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
            let lit = view.visual_type == crate::i3d_object::ObjectVisualType::LitUnconstructed;
            let material = group.material;
            let flags = material.map_or(0, |m| m.flags);
            let has_texture = flags & 0x40000 != 0 && group.texture.is_some();
            let effective_texture = has_texture.then_some(group.texture).flatten();
            let alpha = InspectionAlpha::select(
                effective_texture,
                has_texture.then_some(group.texture_alpha).flatten(),
                has_texture && group.inspection_rgba8_color_key,
            )?;
            let mut policy = if let Some(material) = material {
                if saved.is_some() {
                    constructed_lit_material(material, has_texture, *center)?
                } else if lit {
                    crate::presentation_material::unconstructed_lit_material(
                        material,
                        has_texture,
                        *center,
                        source_ccw_cull_front_face(SourceIndexOrder::Retained),
                    )?
                } else {
                    crate::presentation_material::live_material_with_fade(
                        material,
                        has_texture,
                        *center,
                        source_ccw_cull_front_face(SourceIndexOrder::Retained),
                        *fade,
                    )?
                }
            } else {
                let color = if saved.is_some() {
                    [1.; 4]
                } else if lit {
                    [0., 1., 1., 1.]
                } else if billboard {
                    [
                        1.,
                        1.,
                        1.,
                        billboard_colors
                            .as_ref()
                            .and_then(|colors| colors.first())
                            .ok_or("Billboard first vertex lighting unavailable")?[3],
                    ]
                } else {
                    [
                        1.,
                        1.,
                        1.,
                        null_colors
                            .as_ref()
                            .and_then(|c| c.first())
                            .ok_or("Null material lighting unavailable")?[3],
                    ]
                };
                lhr_render::RenderMaterial::OriginalNull {
                    color,
                    sort_center: *center,
                    visual_z_bias: false,
                }
            };
            // Render's per-group state uses the current live visual, even
            // when the face-group material is null.
            let visual_z_bias =
                arena.state(*frame).map_err(|e| e.to_string())?.visual_flags & 0x100 != 0;
            match &mut policy {
                lhr_render::RenderMaterial::Original(material) => {
                    material.visual_z_bias = visual_z_bias
                }
                lhr_render::RenderMaterial::OriginalNull {
                    visual_z_bias: value,
                    ..
                } => *value = visual_z_bias,
                lhr_render::RenderMaterial::Legacy => unreachable!(),
            }
            if let Some(alpha) = object_vertex_alpha {
                match &mut policy {
                    lhr_render::RenderMaterial::Original(material) => material.color[3] = alpha,
                    lhr_render::RenderMaterial::OriginalNull { color, .. } => color[3] = alpha,
                    lhr_render::RenderMaterial::Legacy => unreachable!(),
                }
            }
            if billboard {
                // VertexColors supplies native first-material RGB. The shader
                // reads alpha from the material policy, not the sidecar, so
                // carry the helper's packed TempAlpha byte into that policy.
                // Preserve the float-based queue, but apply Billboard's own
                // per-face-group alpha blend rule. PrepareLights uses the
                // first group's material for vertex colors, whereas this
                // decision reads the material of the current draw group.
                if let lhr_render::RenderMaterial::Original(render) = &mut policy {
                    render.color[..3].fill(1.);
                    render.color[3] = billboard_colors
                        .as_ref()
                        .and_then(|colors| colors.first())
                        .ok_or("Billboard first vertex lighting unavailable")?[3];
                    if let Some(source) = material {
                        if source.flags & 0x80000000 == 0
                            && matches!(
                                render.alpha,
                                lhr_render::AlphaMode::Blend | lhr_render::AlphaMode::Additive
                            )
                        {
                            let emissive = source.emissive.iter().any(|v| v.to_bits() != 0);
                            let diffuse = source.diffuse.iter().all(|v| v.to_bits() != 0);
                            render.alpha = if emissive && diffuse {
                                lhr_render::AlphaMode::Additive
                            } else {
                                lhr_render::AlphaMode::Blend
                            };
                        }
                    }
                }
            }
            if alpha.omitted {
                unsupported_alpha_masks += 1;
            }
            if !lit
                && (alpha.omitted
                    || material.is_some_and(|m| m.env_texture.is_some())
                    || (!billboard && material.is_some_and(|m| m.emissive != [0.; 3]))
                    || flags & (0x80000 | 0x01000000) != 0)
            {
                omitted += 1;
            }
            if material.is_some_and(|m| !m.diffuse.into_iter().all(f32::is_finite))
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
                (flags & 0x30000) | if lit { 0x80000000 } else { 0 },
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
                    repeat: [flags & 0x10000 == 0, flags & 0x20000 == 0],
                });
                textures.insert(key, texture.clone());
                texture
            };
            let mut vertices = Vec::new();
            vertices
                .try_reserve_exact(vertex_count)
                .map_err(|_| "Car vertex allocation unavailable")?;
            for (index, vertex) in view
                .vertex_bytes
                .chunks_exact(view.vertex_stride)
                .enumerate()
            {
                let point = if let Some(positions) = &morph_positions {
                    positions
                        .position(index)
                        .map_err(|e| format!("Morph draw position: {e:?}"))?
                } else if let Some(pose) = pose {
                    let point = pose[index].position;
                    if !point.into_iter().all(f32::is_finite) {
                        return Err("Nonfinite live singlemesh pose".into());
                    }
                    point
                } else {
                    [word(vertex, 0)?, word(vertex, 4)?, word(vertex, 8)?]
                };
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
                vertices.push(Vertex {
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
            let saved_lighting = if billboard {
                let colors = billboard_colors
                    .as_ref()
                    .ok_or("Billboard draw-matrix colors unavailable")?;
                if colors.len() != vertices.len() {
                    return Err("Billboard vertex lighting extent unavailable".into());
                }
                charge(
                    &mut bytes,
                    vertices
                        .len()
                        .checked_mul(24)
                        .ok_or("Billboard vertex lighting extent overflow")?,
                )?;
                Some(lhr_render::MeshLighting {
                    uv2: vec![[0.; 2]; vertices.len()],
                    colors: colors.clone(),
                    atlas: None,
                    mode: lhr_render::LightingMode::VertexColors { modulate_2x: true },
                })
            } else {
                match saved {
                    None if material.is_none() && !lit => {
                        let colors = null_colors
                            .as_ref()
                            .ok_or("Null material vertex colors unavailable")?;
                        if colors.len() != vertices.len() {
                            return Err("Null material vertex color extent unavailable".into());
                        }
                        charge(
                            &mut bytes,
                            vertices
                                .len()
                                .checked_mul(24)
                                .ok_or("Null vertex lighting extent overflow")?,
                        )?;
                        Some(lhr_render::MeshLighting {
                            uv2: vec![[0.; 2]; vertices.len()],
                            colors: colors.clone(),
                            atlas: None,
                            mode: lhr_render::LightingMode::VertexColors { modulate_2x: true },
                        })
                    }
                    None => None,
                    Some(crate::i3d_object::saved_lightmaps::SavedLitLod::Lightmap(lod)) => {
                        if !no_dynamic_lm || material.is_some_and(|m| m.emissive != [0.; 3]) {
                            return Err(
                                "Saved lightmap dynamic light/emissive evaluation is unavailable"
                                    .into(),
                            );
                        }
                        let faces = lod
                            .face_rectangles
                            .get(group_number)
                            .ok_or("Saved lightmap face-group ownership unavailable")?
                            .len();
                        if group.index_bytes.len()
                            != faces
                                .checked_mul(6)
                                .ok_or("Saved lightmap index extent overflow")?
                            || lod.vertices.len() != vertices.len()
                        {
                            return Err("Saved lightmap managed extent unavailable".into());
                        }
                        charge(
                            &mut bytes,
                            vertices
                                .len()
                                .checked_mul(24)
                                .ok_or("Saved lighting extent overflow")?,
                        )?;
                        let atlas = lod
                            .atlas
                            .read()
                            .map_err(|_| "Saved lightmap atlas lock poisoned")?;
                        let key = (Arc::as_ptr(&lod.atlas) as usize, atlas.revision);
                        let texture = if let Some(texture) = atlas_textures.get(&key) {
                            texture.clone()
                        } else {
                            charge(&mut bytes, atlas.rgba.len())?;
                            let mut rgba = Vec::new();
                            rgba.try_reserve_exact(atlas.rgba.len())
                                .map_err(|_| "Saved lightmap atlas allocation unavailable")?;
                            rgba.extend_from_slice(&atlas.rgba);
                            let texture = Arc::new(Texture {
                                width: atlas.width,
                                height: atlas.height,
                                rgba,
                                repeat: [false; 2],
                            });
                            atlas_textures.insert(key, texture.clone());
                            texture
                        };
                        Some(lhr_render::MeshLighting {
                            uv2: managed_uv2
                                .as_ref()
                                .ok_or("Managed atlas UV unavailable")?
                                .clone(),
                            colors: managed_colors
                                .as_ref()
                                .ok_or("Managed lightmap diffuse unavailable")?
                                .clone(),
                            atlas: Some(texture),
                            mode: lhr_render::LightingMode::LightMap {
                                base_texture: has_texture,
                                modulate_2x: true,
                            },
                        })
                    }
                    Some(crate::i3d_object::saved_lightmaps::SavedLitLod::VertexColors {
                        colors,
                        ..
                    }) => {
                        if colors.len() != vertices.len() {
                            return Err("Saved vertex color extent unavailable".into());
                        }
                        charge(
                            &mut bytes,
                            vertices
                                .len()
                                .checked_mul(24)
                                .ok_or("Saved vertex color sidecar extent overflow")?,
                        )?;
                        Some(lhr_render::MeshLighting {
                            uv2: vec![[0.; 2]; vertices.len()],
                            colors: managed_colors
                                .as_ref()
                                .ok_or("Managed saved vertex diffuse unavailable")?
                                .clone(),
                            atlas: None,
                            mode: lhr_render::LightingMode::VertexColors { modulate_2x: true },
                        })
                    }
                }
            };
            lighting.push(saved_lighting);
            draw_frames.push(*frame);
            meshes.push(Mesh {
                material: policy,
                vertices,
                indices,
                texture: Some(texture),
            });
        }
    }
    Ok(CarInspectionScene {
        scene: Scene { meshes },
        draw_frames,
        lighting,
        visited_frames: seen.len(),
        visible_meshes: frames.len(),
        omitted_material_effects: omitted,
        unsupported_alpha_masks,
        budgeted_bytes: bytes,
        lod_culled_meshes,
    })
}

fn prepare_null_colors(
    arena: &mut ModelArena,
    frame: FrameKey,
    object: crate::i3d_object::ObjectResourceKey,
    lod: u32,
    fade: f32,
) -> Result<Option<Vec<[f32; 4]>>, String> {
    let input = {
        let resources = arena
            .loaded_visual_resources()
            .ok_or("Null material resource owner absent")?;
        let Some(view) = resources
            .inspection_mesh_at_lod(object, lod, CAR_INSPECTION_MAX_DRAWS)
            .map_err(|e| format!("Null material frame {frame:?} geometry: {e:?}"))?
        else {
            return Ok(None);
        };
        let skinned = matches!(
            view.visual_type,
            crate::i3d_object::ObjectVisualType::Singlemesh
                | crate::i3d_object::ObjectVisualType::Singlemorph
        );
        if !(matches!(
            view.visual_type,
            crate::i3d_object::ObjectVisualType::Plain | crate::i3d_object::ObjectVisualType::Morph
        ) || skinned)
            || !view.groups.iter().any(|g| g.material.is_none())
        {
            return Ok(None);
        }
        if view.groups.first().is_none_or(|g| g.material.is_some()) {
            return Err(format!(
                "Null material frame {frame:?} needs first-material lighting evaluation"
            ));
        }
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        if skinned {
            let pose = arena
                .owned_singlemesh_selected_vertices(frame, lod)
                .map_err(|e| format!("Null material pose frame {frame:?}: {e:?}"))?;
            if pose.len() != view.vertex_count as usize {
                return Err(format!(
                    "Null material pose/geometry extent frame {frame:?}"
                ));
            }
            for vertex in pose {
                if !vertex
                    .position
                    .into_iter()
                    .chain(vertex.normal)
                    .all(f32::is_finite)
                {
                    return Err("Nonfinite null material singlemesh pose".into());
                }
                positions.push(vertex.position);
                normals.push(vertex.normal);
            }
        } else {
            for vertex in view.vertex_bytes.as_chunks::<32>().0 {
                positions.push([word(vertex, 0)?, word(vertex, 4)?, word(vertex, 8)?]);
                normals.push([word(vertex, 12)?, word(vertex, 16)?, word(vertex, 20)?]);
            }
        }
        (positions, normals)
    };
    let sector = containing_lighting_sector(arena, frame)?
        .ok_or("Null material requires its actual containing sector")?;
    crate::null_material_lighting::null_material_vertex_lighting(
        arena,
        frame,
        sector,
        &input.0,
        &input.1,
        crate::null_material_lighting::NullMaterialLightingContext {
            fade,
            lightness: 1.,
            transparency: 1.,
            modulate_2x: true,
        },
    )
    .map(Some)
    .map_err(|e| format!("Null material frame {frame:?}: {e}"))
}
fn containing_lighting_sector(
    arena: &ModelArena,
    frame: FrameKey,
) -> Result<Option<crate::scene_ownership::SectorKey>, String> {
    let mut at = Some(frame);
    let mut seen = BTreeSet::new();
    while let Some(key) = at {
        if !seen.insert(key.0) {
            return Err("Lighting parent hierarchy is cyclic".into());
        }
        if arena.state(key).map_err(|e| e.to_string())?.kind == FrameKind::Sector {
            let sector = crate::scene_ownership::SectorKey(key);
            if let Some(scene) = arena.sector(sector).map_err(|e| e.to_string())?.scene {
                let special = arena.scene(scene).map_err(|e| e.to_string())?.sectors;
                if sector == special[3] {
                    let camera = arena
                        .scene_active_camera(scene)
                        .map_err(|e| e.to_string())?
                        .ok_or("Lighting camera absent")?;
                    return Ok(Some(
                        arena
                            .camera(camera)
                            .map_err(|e| e.to_string())?
                            .sector
                            .ok_or("Lighting camera sector absent")?,
                    ));
                }
            }
            return Ok(Some(sector));
        }
        at = arena.links(key).map_err(|e| e.to_string())?.parent;
    }
    Ok(None)
}

/// Lit m_AddFGroups/m_RenderFGroup (LS3DF62030 caller,28d40 draw) use
/// material texture flags for queue/blend/cull but never ordinary diffuse or
/// opacity multiplication. Stage alpha SELECTARG1 takes the base texture.
fn constructed_lit_material(
    material: &crate::visual_resources::I3dMaterial,
    has_texture: bool,
    center: [f32; 3],
) -> Result<lhr_render::RenderMaterial, String> {
    use lhr_render::{AlphaMode, MaterialQueue, OriginalMaterial, RenderMaterial};
    if !center.into_iter().all(f32::is_finite) {
        return Err("Constructed lit sort center is nonfinite".into());
    }
    let flags = material.flags;
    let alpha = if flags & 0x80000000 != 0 {
        AlphaMode::Additive
    } else if flags & 0x40000000 != 0 {
        if material.emissive.iter().any(|v| v.to_bits() != 0) {
            AlphaMode::Additive
        } else {
            AlphaMode::Blend
        }
    } else if flags & 0x20000000 != 0 {
        AlphaMode::ColorKey
    } else {
        AlphaMode::Opaque
    };
    let queue = if !has_texture {
        MaterialQueue::Opaque
    } else if flags & 0xc0000000 != 0 {
        MaterialQueue::Transparent
    } else if flags & 0x20000000 != 0 {
        MaterialQueue::ColorKey
    } else {
        MaterialQueue::Opaque
    };
    Ok(RenderMaterial::Original(OriginalMaterial {
        color: [1.; 4],
        alpha,
        queue,
        has_texture,
        color_key_reference: 200,
        two_sided: flags & 0x10000000 != 0,
        front_face: source_ccw_cull_front_face(SourceIndexOrder::Retained),
        visual_z_bias: false,
        sort_center: center,
    }))
}

/// The admitted static branch is witnessed from the actual containing sector's
/// light list. Any enabled dynamic-LM light requires real PrepareLights support.
fn no_dynamic_lm_lights(arena: &ModelArena, frame: FrameKey) -> Result<bool, String> {
    let mut at = Some(frame);
    let mut seen = BTreeSet::new();
    while let Some(key) = at {
        if !seen.insert(key.0) {
            return Err("Lighting parent hierarchy is cyclic".into());
        }
        if arena.state(key).map_err(|e| e.to_string())?.kind == FrameKind::Sector {
            let mut sector = crate::scene_ownership::SectorKey(key);
            let s = arena.sector(sector).map_err(|e| e.to_string())?;
            if let Some(scene) = s.scene {
                if arena.scene(scene).map_err(|e| e.to_string())?.sectors[3] == sector {
                    let camera = arena
                        .scene_active_camera(scene)
                        .map_err(|e| e.to_string())?
                        .ok_or("Near lighting camera absent")?;
                    sector = arena
                        .camera(camera)
                        .map_err(|e| e.to_string())?
                        .sector
                        .ok_or("Near lighting camera sector absent")?;
                }
            }
            for &light in &arena.sector(sector).map_err(|e| e.to_string())?.lights {
                let state = arena.light(light).map_err(|e| e.to_string())?;
                if !state.intensity.is_finite() {
                    return Err("Sector dynamic light intensity is nonfinite".into());
                }
                if state.mode & 0x10 != 0
                    && arena.state(light).map_err(|e| e.to_string())?.flags & 1 != 0
                    && state.intensity.to_bits() & 0x7fff_ffff >= 0x322bcc77
                    && matches!(state.light_type, 1..=4)
                {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        at = arena.links(key).map_err(|e| e.to_string())?.parent;
    }
    // Explicit standalone model inspection has no sector lights.
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_singlemesh_draw_uses_owned_deformed_vertices_after_joint_update() {
        let path = std::env::temp_dir().join(format!(
            "lhr-inspection-skin-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut source = (*crate::model_loading::tests::skinned_source()).clone();
        let visual = &mut source.frames[0];
        visual.lods[0].vertices.push(lhr_assets::Vertex {
            position: [0., 0., 0.],
            normal: [0., 0., 1.],
            uv: [0., 1.],
        });
        visual.lods[0].groups.push(lhr_assets::FaceGroup {
            material: 0,
            indices: vec![0, 1, 2],
        });
        visual.skin.as_mut().unwrap().lods[0]
            .definition
            .as_mut()
            .unwrap()
            .root_vertex_count = 2;
        source.materials.push(lhr_assets::Material {
            flags: 0,
            ambient: [1.; 3],
            diffuse: [1.; 3],
            texture: String::new(),
            emission: [0.; 3],
            opacity: 1.,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        });
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let model = arena.create_model().unwrap();
        arena.scene_add_frame(scene, model.root()).unwrap();
        arena
            .load_parsed_into_model(model, Arc::new(source))
            .unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        let visual = arena
            .model(model)
            .unwrap()
            .frames
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(0))
            .unwrap();
        let joint = arena
            .model(model)
            .unwrap()
            .frames
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(1))
            .unwrap();
        arena
            .graph
            .assign_position(model.root(), [11., -3., 5.])
            .unwrap();
        arena.update_frame(model.root()).unwrap();
        let first = snapshot_arena_tree(&mut arena, model.root()).unwrap();
        assert_eq!(first.scene.meshes.len(), 1);
        let posed_position = arena.owned_singlemesh_lod0_vertices(visual, 0).unwrap()[0].position;
        let world = arena.world_matrix(visual).unwrap();
        let once = world.try_transform_point(posed_position).unwrap();
        let twice = world.try_transform_point(once).unwrap();
        assert_eq!(
            first.scene.meshes[0].vertices[0].position,
            [once[0], once[1], -once[2]]
        );
        assert_ne!(
            first.scene.meshes[0].vertices[0].position,
            [twice[0], twice[1], -twice[2]]
        );
        let old = first.scene.meshes[0].vertices[0].position;
        arena.graph.assign_position(joint, [6., 0., 0.]).unwrap();
        arena.update_frame(model.root()).unwrap();
        let second = snapshot_arena_tree(&mut arena, model.root()).unwrap();
        let posed_position = arena.owned_singlemesh_lod0_vertices(visual, 0).unwrap()[0].position;
        let once = arena
            .world_matrix(visual)
            .unwrap()
            .try_transform_point(posed_position)
            .unwrap();
        assert_eq!(
            second.scene.meshes[0].vertices[0].position,
            [once[0], once[1], -once[2]]
        );
        assert_ne!(old, second.scene.meshes[0].vertices[0].position);
        arena.close(model).unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn ordinary_and_skinned_lod_share_first_group_alpha_across_group_order_and_fade() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            use lhr_render::{AlphaMode, MaterialQueue, RenderMaterial};
            let path = std::env::temp_dir().join(format!(
                "lhr-first-group-alpha-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&path).unwrap();
            let installation = lhr_assets::Installation::open(&path).unwrap();
            for skinned in [false, true] {
                for order in [[0, 1], [1, 0]] {
                    let mut source = (*crate::model_loading::tests::skinned_source()).clone();
                    let visual = &mut source.frames[0];
                    visual.lods[0].distance = 400.;
                    visual.lods[0].vertices.push(lhr_assets::Vertex {
                        position: [0.; 3],
                        normal: [0., 0., 1.],
                        uv: [0.; 2],
                    });
                    visual.lods[0].groups = order
                        .map(|material| lhr_assets::FaceGroup {
                            material,
                            indices: vec![0, 1, 2],
                        })
                        .to_vec();
                    if skinned {
                        visual.skin.as_mut().unwrap().lods[0]
                            .definition
                            .as_mut()
                            .unwrap()
                            .root_vertex_count = 2;
                    } else {
                        visual.visual = Some(0);
                        visual.skin = None;
                        source.frames.truncate(1);
                    }
                    source.materials = [0.5, 1.]
                        .map(|opacity| lhr_assets::Material {
                            flags: 0,
                            ambient: [0.; 3],
                            diffuse: [1.; 3],
                            texture: String::new(),
                            emission: [0.; 3],
                            opacity,
                            alpha_texture: None,
                            reflection: None,
                            environment_texture: None,
                            animation: None,
                        })
                        .to_vec();
                    let mut arena = ModelArena::default();
                    let scene = arena.construct_scene_owner_prefix().unwrap();
                    let model = arena.create_model().unwrap();
                    arena.scene_add_frame(scene, model.root()).unwrap();
                    let loaded = arena
                        .load_parsed_into_model(model, Arc::new(source))
                        .unwrap();
                    let frame = loaded.frames()[0];
                    arena
                        .load_plain_visual_resources(model, &installation)
                        .unwrap();
                    arena.graph.projected_state_mut(frame).unwrap().visual_flags &= !0x100;
                    let unfaded = snapshot_arena_tree(&mut arena, model.root()).unwrap();
                    arena.graph.projected_state_mut(frame).unwrap().visual_flags |= 0x100;
                    let center = arena.visual_world_bounds(frame).unwrap().center;
                    // Three exact offsets produce dist²=300 and fade=.5 for
                    // this one-level LOD (limit=400, fade starts at 200).
                    let faded = snapshot_selected_frames_with_view(
                        &mut arena,
                        &[frame],
                        crate::inspection_lod::InspectionLodView {
                            camera_pc: center.map(|v| v + 10.),
                            rs_lod_quality: 1.,
                        },
                    )
                    .unwrap();
                    for (snapshot, fade) in [(&unfaded, 1.), (&faded, 0.5)] {
                        assert_eq!(snapshot.scene.meshes.len(), 2);
                        let first_alpha = if order[0] == 0 { 0.5 } else { 1. };
                        let packed = (first_alpha * fade * 255_f32).round_ties_even() / 255.;
                        for (draw, material_index) in snapshot.scene.meshes.iter().zip(order) {
                            let RenderMaterial::Original(material) = draw.material else {
                                panic!()
                            };
                            assert_eq!(material.visual_z_bias, fade < 1.);
                            assert_eq!(
                                material.color[3], packed,
                                "skinned={skinned}, order={order:?}, fade={fade}"
                            );
                            // Current-group float alpha still determines its
                            // draw blend and queue, before shared alpha packing.
                            let transparent = material_index == 0 || fade < 1.;
                            assert_eq!(
                                material.queue,
                                if transparent {
                                    MaterialQueue::Transparent
                                } else {
                                    MaterialQueue::Opaque
                                }
                            );
                            assert_eq!(
                                material.alpha,
                                if transparent {
                                    AlphaMode::Blend
                                } else {
                                    AlphaMode::Opaque
                                }
                            );
                        }
                    }
                    arena.close(model).unwrap();
                }
            }
            std::fs::remove_dir_all(path).unwrap();
        });
    }
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
        assert_eq!(b.color, [0.25, 0.5, 1., 1.]);
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
    fn canonical_snapshot_uses_owned_animated_frames_lazily_in_group_order() {
        canonical_animated_snapshot_fixture(false);
    }
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn gpu_canonical_animated_frames_change_rendered_pixels() {
        canonical_animated_snapshot_fixture(true);
    }
    fn canonical_animated_snapshot_fixture(check_gpu: bool) {
        use crate::model_loading::tests::frame;
        let path = std::env::temp_dir().join(format!(
            "lhr-material-playback-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("maps")).unwrap();
        for (index, rgb) in [[255, 0, 0], [0, 255, 0], [0, 0, 255]]
            .into_iter()
            .enumerate()
        {
            let mut bmp = vec![0u8; 58];
            bmp[..2].copy_from_slice(b"BM");
            bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
            bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
            bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
            bmp[18..22].copy_from_slice(&1i32.to_le_bytes());
            bmp[22..26].copy_from_slice(&1i32.to_le_bytes());
            bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
            bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
            bmp[54..57].copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
            std::fs::write(path.join(format!("maps/fx{index:02}.bmp")), bmp).unwrap();
        }
        let material = lhr_assets::Material {
            flags: 0x14048000,
            ambient: [0.; 3],
            diffuse: [1.; 3],
            texture: "fx00.bmp".into(),
            emission: [0.; 3],
            opacity: 1.,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: Some(lhr_assets::TextureAnimation {
                num_frames: 3,
                env_num_frames: 0,
                loop_frame: -1,
                frame_delay: 10,
                env_loop_frame: -1,
                env_frame_delay: 10,
            }),
        };
        let mut visual = frame(1, None, [0.; 3]);
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
            groups: [0, 1, 0]
                .map(|material| lhr_assets::FaceGroup {
                    material,
                    indices: vec![0, 1, 2],
                })
                .to_vec(),
        });
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut arena = ModelArena::default();
        arena.set_material_driver_time(100).unwrap();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let model = arena.create_model().unwrap();
        arena.scene_add_frame(scene, model.root()).unwrap();
        arena
            .load_parsed_into_model(
                model,
                Arc::new(lhr_assets::Model {
                    materials: vec![material.clone(), material],
                    frames: vec![visual],
                }),
            )
            .unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        arena.light_random_state().seed(1);
        let first = snapshot_car_arena(&mut arena, model.root()).unwrap();
        assert_eq!(first.scene.meshes.len(), 3);
        assert!(first
            .scene
            .meshes
            .iter()
            .all(|mesh| mesh.texture.as_ref().unwrap().rgba == [255, 0, 0, 255]));
        assert_eq!(arena.light_random_state().state(), 1);
        let camera = camera_from_arena_bounds(&mut arena, model.root()).unwrap();
        let mut renderer = check_gpu.then(|| {
            let instance = lhr_render::instance();
            let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
                .expect("animated material GPU fixture requires adapter");
            let renderer = pollster::block_on(lhr_render::Renderer::with_adapter(
                &adapter,
                &first.scene,
                wgpu::TextureFormat::Rgba8Unorm,
                false,
            ))
            .unwrap();
            let pixels = renderer.readback(64, 64, &camera).unwrap();
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[0] > 200 && p[1] < 40 && p[2] < 40),
                "first owned frame must render red pixels"
            );
            renderer
        });
        arena.set_material_driver_time(110).unwrap();
        let second = snapshot_car_arena(&mut arena, model.root()).unwrap();
        assert!(second
            .scene
            .meshes
            .iter()
            .all(|mesh| mesh.texture.as_ref().unwrap().rgba == [0, 0, 255, 255]));
        // Two distinct materials consume rand41/rand18467; the third group
        // shares the first material and cannot consume again at the same tick.
        assert_eq!(arena.light_random_state().state(), 3357800067);
        assert_eq!(second.omitted_material_effects, 0);
        if let Some(renderer) = renderer.as_mut() {
            renderer.replace_meshes(0..3, &second.scene.meshes).unwrap();
            let pixels = renderer.readback(64, 64, &camera).unwrap();
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[2] > 200 && p[0] < 40 && p[1] < 40),
                "new owned frame must render blue pixels after mesh replacement"
            );
            assert!(!pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] > 200 && p[1] < 40 && p[2] < 40));
        }
        let repeated = snapshot_car_arena(&mut arena, model.root()).unwrap();
        assert_eq!(arena.light_random_state().state(), 3357800067);
        assert_eq!(
            repeated.scene.meshes[0].texture.as_ref().unwrap().rgba,
            [0, 0, 255, 255]
        );
        arena.close(model).unwrap();
        arena
            .loaded_visual_resources_mut()
            .unwrap()
            .reclaim_unused_buffers()
            .unwrap();
        let owner = arena.loaded_visual_resources().unwrap();
        assert_eq!(owner.backend().live_textures(), 0);
        assert_eq!(owner.backend().live_graphics(), 0);
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

#[cfg(test)]
#[path = "scene_render_lighting_tests.rs"]
mod lighting_tests;
