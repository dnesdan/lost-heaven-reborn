//! Bounded static scene assembly; source transforms precede the handedness conversion.
use super::{charge_scene, source_rotation};
use glam::{Mat4, Vec3};
use lhr_assets::{FaceGroup, Installation, Lod, Model};
use lhr_engine::runtime::{DrawBinding, GeometryTransform, RetainedScene};
use lhr_render::{Mesh, Scene, Texture, Vertex};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
pub const MAX_SCENE_BYTES: usize = 256 * 1024 * 1024;
const MAX_DRAWS: usize = 4096;
#[derive(Clone, Copy, Debug)]
pub struct XzBounds {
    pub min: [f32; 2],
    pub max: [f32; 2],
}
impl XzBounds {
    pub fn around(center: [f32; 3], radius: f32) -> Result<Self, String> {
        if center.iter().any(|v| !v.is_finite() || v.abs() > 1.0e8)
            || !radius.is_finite()
            || radius <= 0.0
            || radius > 10000.0
        {
            return Err("Neighborhood bounds exceed supported range".into());
        }
        Ok(Self {
            min: [center[0] - radius, center[2] - radius],
            max: [center[0] + radius, center[2] + radius],
        })
    }
    fn overlaps(self, positions: [Vec3; 3]) -> bool {
        let lo = positions
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |a, p| a.min(*p));
        let hi = positions
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |a, p| a.max(*p));
        hi.x >= self.min[0] && lo.x <= self.max[0] && hi.z >= self.min[1] && lo.z <= self.max[1]
    }
}
pub struct SceneBuilder<'a> {
    installation: &'a Installation,
    textures_directory: &'a str,
    meshes: Vec<Mesh>,
    retained: RetainedScene,
    retained_sources: HashSet<usize>,
    bytes: usize,
    texture_cache: HashMap<(String, [bool; 2]), Arc<Texture>>,
    allow_texture_fallback: bool,
    original_model_loading: bool,
    pub cropped_triangles: usize,
    pub unassigned_groups: usize,
}
impl<'a> SceneBuilder<'a> {
    pub fn new(installation: &'a Installation, textures_directory: &'a str) -> Self {
        Self {
            installation,
            textures_directory,
            meshes: Vec::new(),
            retained: RetainedScene::default(),
            retained_sources: HashSet::new(),
            bytes: 0,
            texture_cache: HashMap::new(),
            allow_texture_fallback: true,
            original_model_loading: false,
            cropped_triangles: 0,
            unassigned_groups: 0,
        }
    }
    pub fn deny_texture_fallback(&mut self) {
        self.allow_texture_fallback = false;
    }
    pub fn use_original_model_loading(&mut self) {
        self.original_model_loading = true;
    }
    pub fn append_retained(
        &mut self,
        source_name: &str,
        source: Arc<Model>,
        world: Mat4,
        crop: Option<XzBounds>,
        strict: bool,
        visual_scale: Option<[f32; 3]>,
    ) -> Result<(), String> {
        let model = &*source;
        let source_key = Arc::as_ptr(&source) as usize;
        if !self.retained_sources.contains(&source_key) {
            let source_bytes = super::neighborhood::model_storage_bytes(model)?;
            charge_scene(&mut self.bytes, source_bytes, MAX_SCENE_BYTES)?;
            self.retained_sources.insert(source_key);
        }
        let pose_bytes = model
            .frames
            .len()
            .checked_mul(std::mem::size_of::<lhr_engine::runtime::FramePose>())
            .ok_or("Retained pose size overflow")?;
        charge_scene(&mut self.bytes, pose_bytes, MAX_SCENE_BYTES)?;
        let mut bindings = Vec::new();
        if !world.is_finite() {
            return Err("Model placement transform overflow".into());
        }
        if crop.is_some_and(|b| {
            b.min.iter().chain(&b.max).any(|v| !v.is_finite())
                || b.min[0] > b.max[0]
                || b.min[1] > b.max[1]
        }) {
            return Err("Invalid spatial crop bounds".into());
        }
        let installation = self.installation;
        let mut materialized = if self.original_model_loading {
            if model
                .frames
                .iter()
                .any(|frame| frame.kind == 1 && frame.render_flags & 0x40 != 0)
            {
                return Err(
                    "Original world-space visual vertex dispatch is not reconstructed".into(),
                );
            }
            if world.to_cols_array().map(f32::to_bits)
                != Mat4::IDENTITY.to_cols_array().map(f32::to_bits)
                || visual_scale.is_some()
            {
                return Err(
                    "Original model materialization requires identity wrapper placement".into(),
                );
            }
            let loaded = lhr_engine::model_loading::MaterializedModel::from_parsed(source.clone())
                .map_err(|failure| format!("Original model materialization: {}", failure.error))?;
            charge_scene(
                &mut self.bytes,
                loaded.storage_bytes().map_err(|error| error.to_string())?,
                MAX_SCENE_BYTES,
            )?;
            Some(loaded)
        } else {
            None
        };
        let transforms: Vec<GeometryTransform> = if let Some(loaded) = &mut materialized {
            loaded
                .world_matrices()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(GeometryTransform::Original)
                .collect()
        } else {
            model_transforms(&model.frames, world, visual_scale)?
                .into_iter()
                .map(GeometryTransform::Inspection)
                .collect()
        };
        if transforms.len() != model.frames.len() {
            return Err("Materialized hierarchy/source frame mismatch".into());
        }
        if materialized.is_some() {
            println!("Original parsed-model materialization: {} frames ({} mesh, {} dummy); owned hierarchy and original world/point arithmetic; fast-find, derived mesh resources and mission construction are not reconstructed",
                model.frames.len(), model.frames.iter().filter(|frame| frame.kind == 1).count(),
                model.frames.iter().filter(|frame| frame.kind == 6).count());
        }
        for (frame_index, (frame, transform)) in model.frames.iter().zip(transforms).enumerate() {
            let geometry = if let Some(index) = frame.instance {
                model.frames.get(index).ok_or("Invalid mesh instance")?
            } else {
                frame
            };
            let Some(lod) = geometry.lods.first() else {
                continue;
            };
            for group in &lod.groups {
                let Some(material) = model.materials.get(group.material) else {
                    if !strict {
                        self.unassigned_groups += 1;
                        continue;
                    }
                    return Err("Invalid model material reference".into());
                };
                let (selected, unique_vertices) = select_triangles(lod, group, transform, crop)?;
                self.cropped_triangles += (group.indices.len() - selected.len()) / 3;
                if selected.is_empty() {
                    continue;
                }
                let vertex_count = if crop.is_some() {
                    unique_vertices
                } else {
                    lod.vertices.len()
                };
                let geometry_bytes = vertex_count
                    .checked_mul(std::mem::size_of::<Vertex>())
                    .and_then(|n| selected.len().checked_mul(4).and_then(|i| n.checked_add(i)))
                    .ok_or("Scene size overflow")?;
                if self.meshes.len() >= MAX_DRAWS {
                    return Err("Scene exceeds draw quota".into());
                }
                charge_scene(&mut self.bytes, geometry_bytes, MAX_SCENE_BYTES)?;
                let texture = if material.texture.is_empty() {
                    let mut rgba: Vec<u8> = material
                        .diffuse
                        .iter()
                        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                        .collect();
                    rgba.push(255);
                    Some(Arc::new(Texture {
                        width: 1,
                        height: 1,
                        repeat: [false, false],
                        rgba,
                    }))
                } else {
                    let name = format!("{}/{}", self.textures_directory, material.texture);
                    let repeat = [material.flags & 0x10000 == 0, material.flags & 0x20000 == 0];
                    let key = (name.to_ascii_lowercase(), repeat);
                    if let Some(texture) = self.texture_cache.get(&key) {
                        Some(texture.clone())
                    } else {
                        let bytes = installation
                            .read(&name)
                            .map_err(|_| "A model texture is unavailable")?;
                        let width = bytes
                            .get(18..22)
                            .and_then(|b| b.try_into().ok())
                            .map(i32::from_le_bytes)
                            .filter(|&n| n > 0)
                            .ok_or("Invalid BMP width")?
                            as usize;
                        let height = bytes
                            .get(22..26)
                            .and_then(|b| b.try_into().ok())
                            .map(i32::from_le_bytes)
                            .filter(|&n| n != 0 && n != i32::MIN)
                            .ok_or("Invalid BMP height")?
                            .unsigned_abs() as usize;
                        let texture_bytes = width
                            .checked_mul(height)
                            .and_then(|n| n.checked_mul(4))
                            .ok_or("Texture size overflow")?;
                        charge_scene(&mut self.bytes, texture_bytes, MAX_SCENE_BYTES)?;
                        let decoded = match lhr_assets::decode_bmp_original(&bytes) {
                            Ok(decoded) => decoded,
                            Err(error) if !strict && self.allow_texture_fallback => {
                                eprintln!("Static subset: {name} unavailable ({error}); source diffuse color fallback");
                                let mut rgba: Vec<u8> = material
                                    .diffuse
                                    .iter()
                                    .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                                    .collect();
                                rgba.push(255);
                                lhr_assets::Texture {
                                    width: 1,
                                    height: 1,
                                    rgba,
                                }
                            }
                            Err(error) => {
                                return Err(format!(
                                    "Model texture {name} is unsupported or malformed: {error}"
                                ))
                            }
                        };
                        let texture = Arc::new(Texture {
                            width: decoded.width,
                            height: decoded.height,
                            rgba: decoded.rgba,
                            repeat,
                        });
                        self.texture_cache.insert(key, texture.clone());
                        Some(texture)
                    }
                };
                let (vertices, indices) = make_geometry(lod, &selected, transform, crop.is_some())?;
                let source_vertices = source_vertex_indices(lod, &selected, crop.is_some())?;
                let binding_bytes = source_vertices
                    .len()
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(std::mem::size_of::<DrawBinding>()))
                    .ok_or("Retained binding size overflow")?;
                charge_scene(&mut self.bytes, binding_bytes, MAX_SCENE_BYTES)?;
                bindings.push(DrawBinding {
                    draw_index: self.meshes.len(),
                    frame_index,
                    geometry_frame: frame.instance.unwrap_or(frame_index),
                    source_vertices,
                });
                self.meshes.push(Mesh {
                    vertices,
                    indices,
                    texture,
                });
            }
        }
        if let Some(loaded) = materialized {
            self.retained
                .retain_materialized(source_name.to_owned(), loaded, bindings)?;
        } else {
            self.retained.retain(
                source_name.to_owned(),
                source,
                world,
                visual_scale,
                bindings,
            )?;
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<(Scene, RetainedScene), String> {
        if self.meshes.is_empty() {
            return Err("Selected scene contains no supported visible geometry".into());
        }
        println!("Scene resources: {} draws, {} budgeted geometry/texture bytes, {} shared texture variants; {} unassigned material groups skipped, {} triangles outside spatial subset",
            self.meshes.len(),self.bytes,self.texture_cache.len(),self.unassigned_groups,self.cropped_triangles);
        self.retained.set_assembly_budget(self.bytes)?;
        Ok((
            Scene {
                meshes: self.meshes,
            },
            self.retained,
        ))
    }
}
fn model_transforms(
    frames: &[lhr_assets::Frame],
    world: Mat4,
    visual_scale: Option<[f32; 3]>,
) -> Result<Vec<Mat4>, String> {
    if !world.is_finite()
        || visual_scale.is_some_and(|s| {
            s.iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 10000.0)
        })
    {
        return Err("Invalid model world transform or visual scale".into());
    }
    let mut transforms = Vec::with_capacity(frames.len());
    for frame in frames {
        let scale = if frame.kind == 1 {
            visual_scale.unwrap_or(frame.scale)
        } else {
            frame.scale
        };
        let local = Mat4::from_scale_rotation_translation(
            Vec3::from_array(scale),
            source_rotation(frame.rotation_wxyz)?,
            Vec3::from_array(frame.position),
        );
        let transform = if let Some(parent) = frame.parent {
            *transforms
                .get(parent)
                .ok_or("Model parent ordering is unsupported")?
                * local
        } else {
            world * local
        };
        if !transform.is_finite() {
            return Err("Model transform overflow".into());
        }
        transforms.push(transform);
    }
    Ok(transforms)
}
fn select_triangles(
    lod: &Lod,
    group: &FaceGroup,
    transform: impl Into<GeometryTransform>,
    crop: Option<XzBounds>,
) -> Result<(Vec<u32>, usize), String> {
    let transform = transform.into();
    if !group.indices.len().is_multiple_of(3) {
        return Err("Invalid triangle index count".into());
    }
    let mut selected = Vec::new();
    let mut used = vec![false; lod.vertices.len()];
    for tri in group.indices.as_chunks::<3>().0 {
        let mut p = [Vec3::ZERO; 3];
        for i in 0..3 {
            let vertex = lod
                .vertices
                .get(tri[i] as usize)
                .ok_or("Invalid triangle vertex index")?;
            p[i] = Vec3::from_array(transform.source_point(vertex.position)?);
            if !p[i].is_finite() {
                return Err("Model vertex transform overflow".into());
            }
        }
        if crop.is_none_or(|bounds| bounds.overlaps(p)) {
            selected.extend_from_slice(tri);
            for index in tri {
                used[*index as usize] = true;
            }
        }
    }
    Ok((selected, used.into_iter().filter(|v| *v).count()))
}
fn source_vertex_indices(lod: &Lod, selected: &[u32], compact: bool) -> Result<Vec<u32>, String> {
    let mut used = vec![!compact; lod.vertices.len()];
    if compact {
        for index in selected {
            *used
                .get_mut(*index as usize)
                .ok_or("Invalid retained source index")? = true;
        }
    }
    used.into_iter()
        .enumerate()
        .filter(|(_, used)| *used)
        .map(|(index, _)| u32::try_from(index).map_err(|_| "Retained source index overflow".into()))
        .collect()
}

fn make_geometry(
    lod: &Lod,
    selected: &[u32],
    transform: impl Into<GeometryTransform>,
    compact: bool,
) -> Result<(Vec<Vertex>, Vec<u32>), String> {
    let transform = transform.into();
    let mut mapping = vec![u32::MAX; lod.vertices.len()];
    if compact {
        for i in selected {
            let slot = mapping
                .get_mut(*i as usize)
                .ok_or("Invalid compact vertex index")?;
            *slot = 0;
        }
    }
    let mut vertices = Vec::new();
    for (index, v) in lod.vertices.iter().enumerate() {
        if !compact || mapping[index] != u32::MAX {
            mapping[index] = u32::try_from(vertices.len()).map_err(|_| "Vertex count overflow")?;
            vertices.push(Vertex {
                position: transform.render_point(v.position)?,
                uv: v.uv,
            });
        }
    }
    let mut indices = Vec::with_capacity(selected.len());
    for tri in selected.as_chunks::<3>().0 {
        for i in [tri[0], tri[2], tri[1]] {
            let mapped = *mapping
                .get(i as usize)
                .ok_or("Invalid compact triangle index")?;
            if mapped == u32::MAX {
                return Err("Unmapped compact vertex".into());
            }
            indices.push(mapped);
        }
    }
    Ok((vertices, indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lod(points: &[[f32; 3]]) -> Lod {
        Lod {
            distance: 0.0,
            vertices: points
                .iter()
                .map(|p| lhr_assets::Vertex {
                    position: *p,
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.25, 0.75],
                })
                .collect(),
            groups: vec![],
        }
    }
    fn frame(parent: Option<usize>, position: [f32; 3]) -> lhr_assets::Frame {
        lhr_assets::Frame {
            name: String::new(),
            name_bytes: Vec::new(),
            properties: Vec::new(),
            dummy_bounds: None,
            joint: None,
            parent,
            position,
            scale: [1.0; 3],
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            instance: None,
            lods: vec![],
            kind: 1,
            visual: Some(0),
            render_flags: 0,
            logic_flags: 0,
            culling: 0,
        }
    }
    #[test]
    fn crossing_triangle_is_retained_when_all_vertices_are_outside_crop() {
        let lod = lod(&[
            [-5.0, 0.0, -5.0],
            [5.0, 0.0, -5.0],
            [0.0, 0.0, 5.0],
            [20.0, 0.0, 20.0],
            [21.0, 0.0, 20.0],
            [20.0, 0.0, 21.0],
        ]);
        let group = FaceGroup {
            material: 0,
            indices: vec![0, 1, 2, 3, 4, 5],
        };
        let bounds = XzBounds::around([0.0; 3], 1.0).unwrap();
        let (selected, count) =
            select_triangles(&lod, &group, Mat4::IDENTITY, Some(bounds)).unwrap();
        assert_eq!(selected, vec![0, 1, 2]);
        assert_eq!(count, 3);
        let (vertices, indices) = make_geometry(&lod, &selected, Mat4::IDENTITY, true).unwrap();
        assert_eq!(vertices.len(), 3);
        assert_eq!(indices, vec![0, 2, 1]);
        assert_eq!(vertices[2].position, [0.0, 0.0, -5.0]);
        assert_eq!(vertices[0].uv, [0.25, 0.75]);
    }
    #[test]
    fn crop_uses_world_transform_and_inclusive_negative_coordinate_boundaries() {
        let lod = lod(&[[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]]);
        let group = FaceGroup {
            material: 0,
            indices: vec![0, 1, 2],
        };
        let bounds = XzBounds::around([-10.0, 0.0, -10.0], 1.0).unwrap();
        assert!(select_triangles(&lod, &group, Mat4::IDENTITY, Some(bounds))
            .unwrap()
            .0
            .is_empty());
        let transform = Mat4::from_translation(Vec3::new(-9.0, 0.0, -10.0));
        assert_eq!(
            select_triangles(&lod, &group, transform, Some(bounds))
                .unwrap()
                .0
                .len(),
            3
        );
        let invalid = FaceGroup {
            material: 0,
            indices: vec![0, 1, 8],
        };
        assert!(select_triangles(&lod, &invalid, transform, Some(bounds)).is_err());
        assert!(make_geometry(&lod, &[0, 1, 9], transform, true).is_err());
    }
    #[test]
    fn placement_wraps_every_independent_root_and_preserves_nested_hierarchy() {
        let frames = vec![
            frame(None, [1.0, 0.0, 0.0]),
            frame(None, [0.0, 2.0, 0.0]),
            frame(Some(1), [0.0, 0.0, 3.0]),
        ];
        let world = Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            Vec3::new(10.0, 20.0, 30.0),
        );
        let transforms = model_transforms(&frames, world, None).unwrap();
        for (i, position) in [[1.0, 0.0, 0.0], [0.0, 2.0, 0.0], [0.0, 2.0, 3.0]]
            .into_iter()
            .enumerate()
        {
            assert!(
                transforms[i]
                    .transform_point3(Vec3::ZERO)
                    .distance(world.transform_point3(Vec3::from_array(position)))
                    < 0.00001
            );
        }
        let mut invalid = frames.clone();
        invalid[0].parent = Some(2);
        assert!(model_transforms(&invalid, world, None).is_err());
        assert!(model_transforms(&frames, Mat4::from_scale(Vec3::splat(f32::NAN)), None).is_err());
    }
    #[test]
    fn per_placement_visual_scale_does_not_mutate_shared_model_or_dummy_scale() {
        let mut visual = frame(None, [0.0; 3]);
        visual.scale = [2.0; 3];
        let mut dummy = frame(None, [0.0; 3]);
        dummy.kind = 6;
        dummy.scale = [3.0; 3];
        let mut nested = frame(Some(1), [0.0; 3]);
        nested.scale = [2.0; 3];
        let frames = vec![visual, dummy, nested];
        let first = model_transforms(&frames, Mat4::IDENTITY, Some([4.0; 3])).unwrap();
        let second = model_transforms(&frames, Mat4::IDENTITY, Some([1.0; 3])).unwrap();
        assert_eq!(first[0].transform_vector3(Vec3::X), Vec3::X * 4.0);
        assert_eq!(first[1].transform_vector3(Vec3::X), Vec3::X * 3.0);
        assert_eq!(first[2].transform_vector3(Vec3::X), Vec3::X * 12.0);
        assert_eq!(second[0].transform_vector3(Vec3::X), Vec3::X);
        assert_eq!(second[2].transform_vector3(Vec3::X), Vec3::X * 3.0);
        assert_eq!(frames[0].scale, [2.0; 3]);
        assert_eq!(frames[1].scale, [3.0; 3]);
    }
    #[test]
    fn retained_binding_rebuild_matches_assembly_hierarchy_crop_and_scale() {
        let geometry = lod(&[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [9., 9., 9.]]);
        let mut parent = frame(None, [2., 3., 4.]);
        parent.kind = 6;
        parent.visual = None;
        parent.scale = [3.; 3];
        let mut visual = frame(Some(0), [1., 2., 3.]);
        visual.lods = vec![geometry];
        let q = std::f32::consts::FRAC_1_SQRT_2;
        visual.rotation_wxyz = [q, 0., 0., q];
        let source = Arc::new(Model {
            materials: vec![],
            frames: vec![parent, visual],
        });
        let world = Mat4::from_translation(Vec3::new(5., 6., 7.));
        let selected = [2, 0, 1];
        let mapping = source_vertex_indices(&source.frames[1].lods[0], &selected, true).unwrap();
        assert_eq!(mapping, vec![0, 1, 2]);
        let original_transforms =
            model_transforms(&source.frames, world, Some([2., 1., 1.])).unwrap();
        let (original, _) = make_geometry(
            &source.frames[1].lods[0],
            &selected,
            original_transforms[1],
            true,
        )
        .unwrap();
        let mut graph = RetainedScene::default();
        graph
            .retain(
                "synthetic".into(),
                source,
                world,
                Some([2., 1., 1.]),
                vec![DrawBinding {
                    draw_index: 0,
                    frame_index: 1,
                    geometry_frame: 1,
                    source_vertices: mapping,
                }],
            )
            .unwrap();
        let model = &graph.models[0];
        let rebuilt = model
            .vertices(&model.draws[0], &model.transforms().unwrap())
            .unwrap();
        for (a, b) in original.iter().zip(rebuilt) {
            assert_eq!(a.position.map(f32::to_bits), b.position.map(f32::to_bits));
            assert_eq!(a.uv.map(f32::to_bits), b.uv.map(f32::to_bits));
        }
        assert!(source_vertex_indices(&model.source.frames[1].lods[0], &[99], true).is_err());
    }
}
