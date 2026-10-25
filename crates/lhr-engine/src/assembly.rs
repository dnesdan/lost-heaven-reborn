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
    animation_sample: Option<(Arc<lhr_assets::animation::AnimationSet>, u32)>,
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
            animation_sample: None,
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
    /// Sample the next original model once; no controller or clock is inferred.
    pub fn sample_original_animation(
        &mut self,
        animation: Arc<lhr_assets::animation::AnimationSet>,
        milliseconds: u32,
    ) {
        self.original_model_loading = true;
        self.animation_sample = Some((animation, milliseconds));
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
        let lit_context =
            lhr_engine::loaded_visual_resources::MaterialLoadContext::ORIGINAL_DRIVER_INIT_DEFAULTS;
        let mut lit_materials = HashSet::new();
        let mut ordinary_materials = HashSet::new();
        for frame in &model.frames {
            if frame.instance.is_some_and(|index| {
                frame.visual == Some(1)
                    || model
                        .frames
                        .get(index)
                        .is_some_and(|original| original.visual == Some(1))
            }) {
                return Err("Static lit instance subtype ownership unavailable".into());
            }
            if frame.kind == 1 && frame.visual == Some(1) && frame.render_flags & 0x50 != 0 {
                return Err("Static lit texture-driven/world-space dispatch unavailable".into());
            }
            for group in frame.lods.iter().flat_map(|lod| &lod.groups) {
                if frame.kind != 1 || frame.visual != Some(1) {
                    ordinary_materials.insert(group.material);
                    continue;
                }
                let material = model
                    .materials
                    .get(group.material)
                    .ok_or("Static unconstructed lit requires assigned material")?;
                if material.animation.is_some() {
                    return Err("Static unconstructed lit animation unavailable".into());
                }
                if !material.texture.is_empty()
                    && (lit_context.loaded_material_flags(material) & 0xe0000000 != 0
                        || lit_context
                            .base_texture_flags(material)
                            .map_err(|e| format!("Static lit material: {e:?}"))?
                            & 0xe
                            != 0)
                {
                    return Err(
                        "Static unconstructed lit texture alpha/add/color-key unavailable".into(),
                    );
                }
                lit_materials.insert(group.material);
            }
        }
        if strict
            && model.materials.iter().enumerate().any(|(index, material)| {
                (!lit_materials.contains(&index) || ordinary_materials.contains(&index))
                    && super::material_effects_omitted(material)
            })
        {
            return Err("Selected model requires unsupported material effects".into());
        }
        if model.frames.iter().any(|frame| {
            matches!(frame.visual, Some(2 | 3)) || frame.skin.is_some() || frame.morph.is_some()
        }) && !self.original_model_loading
        {
            return Err(
                "Singlemesh or singlemorph requires owned binding and a checked CPU pose".into(),
            );
        }
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
            let mut loaded = lhr_engine::model_loading::MaterializedModel::from_parsed(
                source.clone(),
            )
            .map_err(|failure| format!("Original model materialization: {}", failure.error))?;
            if let Some((animation, milliseconds)) = self.animation_sample.take() {
                let binding = lhr_engine::skeletal_animation::SkeletalAnimation::new(
                    &source,
                    animation,
                    loaded.source_frame_keys(),
                )
                .map_err(|error| format!("Unsupported skeletal binding: {error:?}"))?;
                let bytes = binding
                    .storage_bytes()
                    .map_err(|error| format!("{error:?}"))?
                    .checked_add(
                        binding
                            .animation_storage_bytes()
                            .map_err(|error| format!("{error:?}"))?,
                    )
                    .and_then(|bytes| {
                        binding
                            .bindings()
                            .len()
                            .checked_mul(std::mem::size_of::<
                                lhr_engine::skeletal_animation::SampledFrame,
                            >())
                            .and_then(|samples| bytes.checked_add(samples))
                    })
                    .ok_or("Animation storage overflow")?;
                charge_scene(&mut self.bytes, bytes, MAX_SCENE_BYTES)?;
                loaded
                    .evaluate_skeletal_animation(&binding, milliseconds)
                    .map_err(|error| error.to_string())?;
                println!("Original skeletal pose sample: {} bound frames at {milliseconds} ms; position/rotation evaluators, Target updates and CPU skinning; no controller playback or gameplay", binding.bindings().len());
            }
            loaded
                .prepare_skin_pose()
                .map_err(|error| error.to_string())?;
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
        if let Some(loaded) = &materialized {
            println!(
                "Original parsed-model materialization: {} frames ({} visual, {} dummy, {} target, {} joint); owned hierarchy and original world/point arithmetic; fast-find, native vertex buffers and mission construction are not reconstructed",
                model.frames.len(),
                model.frames.iter().filter(|frame| frame.kind == 1).count(),
                model.frames.iter().filter(|frame| frame.kind == 6).count(),
                model.frames.iter().filter(|frame| frame.kind == 7).count(),
                model.frames.iter().filter(|frame| frame.kind == 10).count()
            );
            if loaded.skin_pose().is_some() {
                println!(
                    "Original singlemesh pose: first LOD, bound Joint hierarchy, eager bounds and CPU position/normal deformation; source UVs and topology retained; unlit inspection rendering, no character controller or gameplay"
                );
            }
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
            let pose = materialized
                .as_ref()
                .and_then(|loaded| loaded.skin_pose())
                .and_then(|pose| pose.vertices(frame_index));
            if matches!(geometry.visual, Some(2 | 3)) && pose.is_none() {
                return Err(
                    "Character pose unavailable; source vertices cannot substitute for deformation"
                        .into(),
                );
            }
            let lit = frame.kind == 1 && frame.visual == Some(1);
            for group in &lod.groups {
                let Some(material) = model.materials.get(group.material) else {
                    if !strict {
                        self.unassigned_groups += 1;
                        continue;
                    }
                    return Err("Invalid model material reference".into());
                };
                let (selected, unique_vertices) =
                    select_triangles(lod, group, transform, crop, pose)?;
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
                let texture = if lit && material.texture.is_empty() {
                    Some(Arc::new(Texture {
                        width: 1,
                        height: 1,
                        repeat: [false; 2],
                        rgba: vec![255; 4],
                    }))
                } else if material.texture.is_empty() {
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
                            Err(error) if !strict && self.allow_texture_fallback && !lit => {
                                eprintln!(
                                    "Static subset: {name} unavailable ({error}); source diffuse color fallback"
                                );
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
                                ));
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
                let (vertices, indices) =
                    make_geometry(lod, &selected, transform, crop.is_some(), pose)?;
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
                    material: if lit {
                        lhr_engine::presentation_material::unconstructed_lit_material_flags(
                            lit_context.loaded_material_flags(material),
                            !material.texture.is_empty(),
                            [0.; 3],
                            lhr_render::FrontFace::Clockwise,
                        )?
                    } else {
                        Default::default()
                    },
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
        println!(
            "Scene resources: {} draws, {} budgeted geometry/texture bytes, {} shared texture variants; {} unassigned material groups skipped, {} triangles outside spatial subset",
            self.meshes.len(),
            self.bytes,
            self.texture_cache.len(),
            self.unassigned_groups,
            self.cropped_triangles
        );
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
    pose: Option<&[lhr_engine::singlemesh_deformation::SkinVertex]>,
) -> Result<(Vec<u32>, usize), String> {
    let transform = transform.into();
    if pose.is_some_and(|pose| pose.len() != lod.vertices.len()) {
        return Err("Deformed/source vertex count mismatch".into());
    }
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
            let position = pose.map_or(vertex.position, |pose| pose[tri[i] as usize].position);
            p[i] = Vec3::from_array(transform.source_point(position)?);
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
    pose: Option<&[lhr_engine::singlemesh_deformation::SkinVertex]>,
) -> Result<(Vec<Vertex>, Vec<u32>), String> {
    let transform = transform.into();
    if pose.is_some_and(|pose| pose.len() != lod.vertices.len()) {
        return Err("Deformed/source vertex count mismatch".into());
    }
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
                position: transform
                    .render_point(pose.map_or(v.position, |pose| pose[index].position))?,
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
            skin: None,
            morph: None,
            target: None,
            occluder: None,
            sector: None,
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
    fn character_source(visual: u8) -> Arc<Model> {
        let mut mesh = frame(None, [0.; 3]);
        mesh.visual = Some(visual);
        mesh.lods.push(Lod {
            distance: 0.,
            vertices: [[100., 0., 0.], [101., 0., 0.], [100., 0., 1.]]
                .into_iter()
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 1., 0.],
                    uv: [0.25, 0.75],
                })
                .collect(),
            groups: vec![FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            }],
        });
        mesh.skin = Some(lhr_assets::SkinRecord {
            lods: vec![lhr_assets::SkinLodRecord {
                definition: Some(lhr_assets::SkinLodDefinition {
                    root_vertex_count: 0,
                    bounds: [-1., -1., -1., 1., 1., 1.],
                    bones: vec![lhr_assets::SkinBoneRecord {
                        matrix: lhr_engine::native_transform::SMatrix::identity().elements,
                        rigid_vertex_count: 3,
                        weighted_vertex_count: 0,
                        blend_joint_index: 0,
                        bounds: [-1., -1., -1., 1., 1., 1.],
                        weights: vec![],
                    }],
                }),
            }],
        });
        if visual == 3 {
            mesh.morph = Some(lhr_assets::MorphRecord {
                target_count: 2,
                definition: Some(lhr_assets::MorphDefinition {
                    channel_count: 1,
                    trailer_words: [0; 10],
                    lods: vec![lhr_assets::MorphLodRecord {
                        channels: vec![lhr_assets::MorphChannelRecord {
                            vertex_count: 3,
                            index_mode: Some(1),
                            vertex_indices: vec![2, 0, 1],
                            samples: [[0., 0., 1.], [0., 0., 0.], [1., 0., 0.]]
                                .into_iter()
                                .flat_map(|position| {
                                    [
                                        lhr_assets::MorphSampleRecord {
                                            position,
                                            normal: [0., 1., 0.],
                                        },
                                        lhr_assets::MorphSampleRecord {
                                            position: [9.; 3],
                                            normal: [1., 0., 0.],
                                        },
                                    ]
                                })
                                .collect(),
                        }],
                    }],
                }),
            });
        }
        let mut joint = frame(Some(0), [0.; 3]);
        joint.kind = 10;
        joint.visual = None;
        joint.joint = Some(lhr_assets::JointRecord {
            bone_matrix: lhr_engine::native_transform::SMatrix::identity().elements,
            index: 0,
        });
        Arc::new(Model {
            frames: vec![mesh, joint],
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
        })
    }
    #[test]
    fn character_assembly_requires_owned_pose_and_rejects_unsupported_morph_materials() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let directory = Directory(
            std::env::temp_dir().join(format!("lhr-character-assembly-{}", std::process::id())),
        );
        std::fs::create_dir(&directory.0).unwrap();
        let installation = Installation::open(&directory.0).unwrap();
        for visual in [2, 3] {
            let source = character_source(visual);
            let mut legacy = SceneBuilder::new(&installation, "maps");
            assert!(legacy
                .append_retained(
                    "synthetic",
                    source.clone(),
                    Mat4::IDENTITY,
                    None,
                    true,
                    None
                )
                .unwrap_err()
                .contains("checked CPU pose"));
            let mut builder = SceneBuilder::new(&installation, "maps");
            builder.use_original_model_loading();
            builder
                .append_retained(
                    "synthetic",
                    source.clone(),
                    Mat4::IDENTITY,
                    None,
                    true,
                    None,
                )
                .unwrap();
            let (scene, retained) = builder.finish().unwrap();
            assert_eq!(scene.meshes.len(), 1);
            assert_eq!(scene.meshes[0].indices, [0, 2, 1]);
            assert_eq!(scene.meshes[0].vertices[0].uv, [0.25, 0.75]);
            assert_eq!(
                scene.meshes[0].vertices[0].position,
                if visual == 3 { [0.; 3] } else { [100., 0., 0.] }
            );
            assert_eq!(
                scene.meshes[0].vertices[2].position,
                if visual == 3 {
                    [0., 0., -1.]
                } else {
                    [100., 0., -1.]
                }
            );
            assert_eq!(
                source.frames[0].lods[0].vertices[0].position,
                [100., 0., 0.]
            );
            assert!(Arc::ptr_eq(&retained.models[0].source, &source));
        }
        let mut invalid = (*character_source(3)).clone();
        let channel = &mut invalid.frames[0]
            .morph
            .as_mut()
            .unwrap()
            .definition
            .as_mut()
            .unwrap()
            .lods[0]
            .channels[0];
        channel.index_mode = Some(0);
        channel.vertex_indices.clear();
        let mut builder = SceneBuilder::new(&installation, "maps");
        builder.use_original_model_loading();
        assert!(builder
            .append_retained(
                "synthetic",
                Arc::new(invalid),
                Mat4::IDENTITY,
                None,
                true,
                None
            )
            .unwrap_err()
            .contains("MissingVertexIndices"));
        let mut invalid = (*character_source(3)).clone();
        invalid.materials[0].flags = 0x8000;
        let mut builder = SceneBuilder::new(&installation, "maps");
        builder.use_original_model_loading();
        assert!(builder
            .append_retained(
                "synthetic",
                Arc::new(invalid),
                Mat4::IDENTITY,
                None,
                true,
                None
            )
            .unwrap_err()
            .contains("unsupported material effects"));
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
            select_triangles(&lod, &group, Mat4::IDENTITY, Some(bounds), None).unwrap();
        assert_eq!(selected, vec![0, 1, 2]);
        assert_eq!(count, 3);
        let (vertices, indices) =
            make_geometry(&lod, &selected, Mat4::IDENTITY, true, None).unwrap();
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
        assert!(
            select_triangles(&lod, &group, Mat4::IDENTITY, Some(bounds), None)
                .unwrap()
                .0
                .is_empty()
        );
        let transform = Mat4::from_translation(Vec3::new(-9.0, 0.0, -10.0));
        assert_eq!(
            select_triangles(&lod, &group, transform, Some(bounds), None)
                .unwrap()
                .0
                .len(),
            3
        );
        let invalid = FaceGroup {
            material: 0,
            indices: vec![0, 1, 8],
        };
        assert!(select_triangles(&lod, &invalid, transform, Some(bounds), None).is_err());
        assert!(make_geometry(&lod, &[0, 1, 9], transform, true, None).is_err());
    }
    #[test]
    fn skin_draw_uses_deformed_positions_with_original_uvs_and_topology() {
        let lod = lod(&[[100., 0., 0.], [101., 0., 0.], [100., 0., 1.]]);
        let pose: Vec<_> = [[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]]
            .into_iter()
            .map(|position| lhr_engine::singlemesh_deformation::SkinVertex {
                position,
                normal: [0., 1., 0.],
            })
            .collect();
        let group = FaceGroup {
            material: 0,
            indices: vec![0, 1, 2],
        };
        let bounds = XzBounds::around([0.; 3], 2.).unwrap();
        let (selected, _) =
            select_triangles(&lod, &group, Mat4::IDENTITY, Some(bounds), Some(&pose)).unwrap();
        assert_eq!(selected, [0, 1, 2]);
        assert!(
            select_triangles(&lod, &group, Mat4::IDENTITY, Some(bounds), None)
                .unwrap()
                .0
                .is_empty()
        );
        let (vertices, indices) =
            make_geometry(&lod, &selected, Mat4::IDENTITY, false, Some(&pose)).unwrap();
        assert_eq!(indices, [0, 2, 1]);
        assert_eq!(vertices[2].position, [0., 0., -1.]);
        assert_eq!(vertices[2].uv, lod.vertices[2].uv);
        assert_eq!(lod.vertices[0].position, [100., 0., 0.]);
        assert!(make_geometry(&lod, &selected, Mat4::IDENTITY, false, Some(&pose[..2])).is_err());
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
            None,
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
    #[test]
    fn static_lit_constructor_policy_keeps_decoded_pixels_and_uses_cyan_for_empty_texture() {
        let path = std::env::temp_dir().join(format!(
            "lhr-static-lit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
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
        bmp[54..57].copy_from_slice(&[32, 64, 128]);
        std::fs::write(path.join("maps/lit.bmp"), bmp).unwrap();
        let installation = Installation::open(&path).unwrap();
        let mut visual = frame(None, [0.; 3]);
        visual.visual = Some(1);
        visual.render_flags = 0;
        visual.logic_flags = 0;
        let mut level = lod(&[[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]]);
        level.groups = vec![FaceGroup {
            material: 0,
            indices: vec![0, 1, 2],
        }];
        visual.lods.push(level);
        let mut source = Model {
            frames: vec![visual],
            materials: vec![lhr_assets::Material {
                flags: 0x40000,
                ambient: [0.; 3],
                diffuse: [1., 0., 0.],
                texture: "lit.bmp".into(),
                emission: [0.7; 3],
                opacity: 0.25,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
        };
        for textured in [true, false] {
            if !textured {
                source.materials[0].texture.clear();
                source.materials[0].flags = 0xe0000000;
            }
            let mut builder = SceneBuilder::new(&installation, "maps");
            builder
                .append_retained(
                    "synthetic",
                    Arc::new(source.clone()),
                    Mat4::IDENTITY,
                    None,
                    true,
                    None,
                )
                .unwrap();
            let (scene, _) = builder.finish().unwrap();
            let mesh = &scene.meshes[0];
            let lhr_render::RenderMaterial::Original(policy) = mesh.material else {
                panic!()
            };
            assert_eq!(policy.color, [0., 1., 1., 1.]);
            assert_eq!(policy.alpha, lhr_render::AlphaMode::Opaque);
            assert_eq!(policy.front_face, lhr_render::FrontFace::Clockwise);
            assert_eq!(mesh.indices, [0, 2, 1]);
            assert_eq!(
                mesh.texture.as_ref().unwrap().rgba,
                if textured {
                    vec![128, 64, 32, 255]
                } else {
                    vec![255; 4]
                }
            );
        }
        source.materials[0].texture = "missing.bmp".into();
        for flags in [0x20000000, 0x40000000, 0x80000000] {
            source.materials[0].flags = flags | 0x40000;
            let mut builder = SceneBuilder::new(&installation, "maps");
            assert!(builder
                .append_retained(
                    "synthetic",
                    Arc::new(source.clone()),
                    Mat4::IDENTITY,
                    None,
                    false,
                    None
                )
                .unwrap_err()
                .contains("lit texture alpha/add/color-key"));
            assert_eq!(builder.bytes, 0);
            assert!(builder.meshes.is_empty());
            assert!(builder.texture_cache.is_empty());
        }
        source.frames[0].instance = Some(0);
        let mut builder = SceneBuilder::new(&installation, "maps");
        assert!(builder
            .append_retained(
                "synthetic",
                Arc::new(source.clone()),
                Mat4::IDENTITY,
                None,
                false,
                None
            )
            .unwrap_err()
            .contains("lit instance"));
        source.frames[0].instance = None;
        source.frames[0].lods[0].groups[0].material = lhr_assets::UNASSIGNED_MATERIAL;
        assert!(builder
            .append_retained(
                "synthetic",
                Arc::new(source),
                Mat4::IDENTITY,
                None,
                false,
                None
            )
            .unwrap_err()
            .contains("assigned material"));
        assert_eq!(builder.bytes, 0);
        std::fs::remove_dir_all(path).unwrap();
    }
}
