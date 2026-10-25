//! Unlit first-LOD inspection of the same live arena initialized by C_car.
//! Geometry comes from owned CPU buffers, including object-private LOD data.
//! This bridge does not implement the original lighting/material renderer.
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
    pub visited_frames: usize,
    pub visible_meshes: usize,
    pub omitted_material_effects: usize,
    pub unsupported_alpha_masks: usize,
    pub budgeted_bytes: usize,
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
/// Snapshot the actual visible subtree, including sector-owned car effects.
/// Unlike car inspection, an empty visible tree is a valid presentation state.
/// The same bounded CPU geometry, first-LOD and material policies apply.
pub fn snapshot_arena_tree(
    arena: &mut ModelArena,
    root: FrameKey,
) -> Result<CarInspectionScene, String> {
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
            let object = arena
                .frame_object_resources(frame)
                .map_err(|e| e.to_string())?
                .ok_or("Car mesh has no owned CPU resources")?;
            let matrix = arena.world_matrix(frame).map_err(|e| e.to_string())?;
            frames.push((object, matrix));
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
        });
    }
    let resources = arena
        .loaded_visual_resources()
        .ok_or("Car CPU resource owner unavailable")?;
    let mut bytes = 0usize;
    let mut meshes = Vec::new();
    let mut textures = BTreeMap::<(u64, [u32; 3], u32), Arc<Texture>>::new();
    let mut omitted = 0usize;
    let mut unsupported_alpha_masks = 0usize;
    for (object, world) in &frames {
        let Some(view) = resources
            .inspection_mesh(*object, CAR_INSPECTION_MAX_DRAWS - meshes.len())
            .map_err(|e| format!("Live car CPU geometry: {e:?}"))?
        else {
            continue;
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
            let material = group.material;
            let alpha = InspectionAlpha::select(
                group.texture,
                group.texture_alpha,
                group.inspection_rgba8_color_key,
            )?;
            if alpha.omitted {
                unsupported_alpha_masks += 1;
            }
            if alpha.omitted
                || material.env_texture.is_some()
                || material.alpha != 1.
                || material.emissive != [0.; 3]
                || material.flags
                    & (0x80000 | 0x01000000 | 0x04000000 | 0x20000000 | 0x40000000 | 0x80000000)
                    != 0
            {
                omitted += 1;
            }
            if !material.diffuse.into_iter().all(f32::is_finite)
                || !view.uv_offset.into_iter().all(f32::is_finite)
            {
                return Err("Car material or UV offset is nonfinite".into());
            }
            let key = (
                group.texture_identity.map_or(0, |key| key.0),
                material.diffuse.map(f32::to_bits),
                material.flags & 0x30000,
            );
            let texture = if let Some(texture) = textures.get(&key) {
                texture.clone()
            } else {
                let (width, height, source) = match group.texture {
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
                    for (channel, value) in pixel.iter().take(3).enumerate() {
                        rgba.push(
                            (f32::from(*value) * material.diffuse[channel].clamp(0., 1.)).round()
                                as u8,
                        );
                    }
                    rgba.push(alpha.alpha(index, pixel));
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
            for vertex in view.vertex_bytes.as_chunks::<32>().0 {
                let point = [word(vertex, 0)?, word(vertex, 4)?, word(vertex, 8)?];
                let [x, y, z] = world
                    .try_transform_point(point)
                    .map_err(|_| "Car world transform outside verified arithmetic domain")?;
                let uv = [
                    word(vertex, 24)? + view.uv_offset[0],
                    word(vertex, 28)? + view.uv_offset[1],
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
            meshes.push(Mesh {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
