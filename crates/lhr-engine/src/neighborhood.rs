//! Deliberately bounded static inspection around an authored mission spawn.
use super::{
    assembly::{SceneBuilder, XzBounds},
    material_effects_omitted, source_rotation, LoadedScene,
};
use glam::{Mat4, Vec3};
use lhr_assets::{CacheObject, CityCache, Installation, Model, SceneObject};
use lhr_render::Camera;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
const RADIUS: f32 = 120.0;
const PLACEMENT_CAP: usize = 64;

pub fn load(installation: &Installation, textures: &str) -> Result<LoadedScene, String> {
    let read = |name: &str| {
        installation
            .read(name)
            .map_err(|e| format!("Neighborhood resource {name}: {e}"))
    };
    let metadata = lhr_assets::parse_scene2(&read("missions/mise01/scene2.bin")?)
        .map_err(|e| e.to_string())?;
    let player = actor_object(&metadata, "TommyTAXIDRIVER", 2)?;
    let center = player
        .world_position
        .ok_or("Player authored world position absent")?;
    let bounds = XzBounds::around(center, RADIUS)?;
    let taxi = actor_object(&metadata, "taxi", 4)?;
    let taxi_world = object_transform(&metadata, taxi)?;
    let taxi_path = model_path(taxi.model.as_deref().ok_or("Taxi source model absent")?)?;
    let mut source_budget = SourceBudget::default();
    let root_bytes = read("missions/mise01/scene.4ds")?;
    source_budget.input(root_bytes.len())?;
    let root = Arc::new(lhr_assets::parse_four_ds_scene(&root_bytes).map_err(|e| e.to_string())?);
    source_budget.model(&root)?;
    drop(root_bytes);
    let mut builder = SceneBuilder::new(installation, textures);
    builder.deny_texture_fallback();
    let mut effect_materials = root
        .materials
        .iter()
        .filter(|m| material_effects_omitted(m))
        .count();
    builder.append_retained(
        "missions/mise01/scene.4ds",
        root.clone(),
        Mat4::IDENTITY,
        Some(bounds),
        false,
        None,
    )?;
    drop(root);
    let cache = lhr_assets::parse_city_cache(&read("missions/mise01/cache.bin")?)
        .map_err(|e| e.to_string())?;
    let (selected, candidates, total) = select(&cache, center, RADIUS, PLACEMENT_CAP)?;
    let mut models = HashMap::<String, Arc<Model>>::new();
    let mut unsupported = BTreeMap::<String, String>::new();
    let mut loaded = 0usize;
    let mut secondary_overrides = 0usize;
    let mut billboards = 0usize;
    for object in &selected {
        if object.model.eq_ignore_ascii_case("0rek23.i3d") {
            return Err(
                "Native cache filename substitution is unsupported in this inspection subset"
                    .into(),
            );
        }
        let name = model_path(&object.model)?;
        if unsupported.contains_key(&name) {
            billboards += 1;
            continue;
        }
        let model = if let Some(model) = models.get(&name) {
            model.clone()
        } else {
            let bytes = read(&name)?;
            source_budget.input(bytes.len())?;
            match lhr_assets::parse_four_ds_scene(&bytes) {
                Ok(model) => {
                    source_budget.model(&model)?;
                    effect_materials += model
                        .materials
                        .iter()
                        .filter(|m| material_effects_omitted(m))
                        .count();
                    let model = Arc::new(model);
                    models.insert(name.clone(), model.clone());
                    model
                }
                Err(error) if error.is_unsupported_billboard() => {
                    // Only a complete structurally valid model containing visual type 4 is skipped.
                    // Malformed/truncated models and other unsupported visuals remain fatal.
                    unsupported.insert(name, error.to_string());
                    billboards += 1;
                    continue;
                }
                Err(error) => return Err(format!("Neighborhood model {name}: {error}")),
            }
        };
        let world = placement_transform(object.position, object.rotation_wxyz, object.scale)?;
        builder.append_retained(
            &name,
            model.clone(),
            world,
            None,
            false,
            Some(object.secondary_scale),
        )?;
        if object.secondary_scale != [1.0; 3] {
            secondary_overrides += 1;
        }
        loaded += 1;
    }
    let taxi_bytes = read(&taxi_path)?;
    source_budget.input(taxi_bytes.len())?;
    let taxi_model =
        Arc::new(lhr_assets::parse_four_ds_scene(&taxi_bytes).map_err(|e| e.to_string())?);
    source_budget.model(&taxi_model)?;
    drop(taxi_bytes);
    effect_materials += taxi_model
        .materials
        .iter()
        .filter(|m| material_effects_omitted(m))
        .count();
    builder.append_retained(&taxi_path, taxi_model, taxi_world, None, false, None)?;
    println!(
        "MISE01 static neighborhood: {total} cache placements, {candidates} within {RADIUS} source units, {} selected (cap {PLACEMENT_CAP}), {loaded} loaded; {billboards} unsupported billboard-visual placements skipped; secondary visual-scale override applied per placement ({secondary_overrides} nonunit); {} shared parsed cache models; one authored taxi placed across all model roots",
        selected.len(),
        models.len()
    );
    for (name, reason) in &unsupported {
        println!("Neighborhood unsupported model: {name}: {reason}");
    }
    println!(
        "Neighborhood metadata: {} objects, {} actors, {} parsed script programs; {effect_materials} source materials have effects not rendered; lighting, actor initialization, scripts, traffic, physics and gameplay are not executed",
        metadata.objects.len(),
        metadata.actors.len(),
        metadata.scripts.len()
    );
    println!(
        "Cache source word retained without visibility interpretation; native part visibility, pooling and PVS are not executed. The native filename substitution outside this selected subset is not implemented"
    );
    println!(
        "Neighborhood model budgets: {} encoded source bytes (limit {}), {} all-LOD parsed allocation bytes (limit {}) charged across root/cache/taxi",
        source_budget.encoded, MAX_MODEL_INPUT_BYTES, source_budget.decoded, MAX_PARSED_MODEL_BYTES
    );
    let (scene, mut runtime) = builder.finish()?;
    let camera = inspection_camera(taxi_world)?;
    println!(
        "Camera: rewrite inspection view of authored taxi/spawn; not an original gameplay camera"
    );
    runtime.retain_metadata(metadata)?;
    Ok(LoadedScene {
        scene,
        camera,
        runtime,
        animation: None,
    })
}
const MAX_MODEL_INPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_PARSED_MODEL_BYTES: usize = 64 * 1024 * 1024;
#[derive(Default)]
struct SourceBudget {
    encoded: usize,
    decoded: usize,
}
impl SourceBudget {
    fn input(&mut self, bytes: usize) -> Result<(), String> {
        super::charge_scene(&mut self.encoded, bytes, MAX_MODEL_INPUT_BYTES)
            .map_err(|_| "Neighborhood encoded model source quota exceeded".into())
    }
    fn model(&mut self, model: &Model) -> Result<(), String> {
        self.charge_model(model_storage_bytes(model)?)
    }
    fn charge_model(&mut self, bytes: usize) -> Result<(), String> {
        super::charge_scene(&mut self.decoded, bytes, MAX_PARSED_MODEL_BYTES)
            .map_err(|_| "Neighborhood all-LOD parsed model quota exceeded".into())
    }
}
pub(super) fn model_storage_bytes(model: &Model) -> Result<usize, String> {
    let mut total = std::mem::size_of::<Model>();
    let mut add = |count: usize, size: usize| -> Result<(), String> {
        let bytes = count
            .checked_mul(size)
            .ok_or("Parsed model size overflow")?;
        total = total
            .checked_add(bytes)
            .ok_or("Parsed model size overflow")?;
        Ok(())
    };
    add(
        model.materials.capacity(),
        std::mem::size_of::<lhr_assets::Material>(),
    )?;
    add(
        model.frames.capacity(),
        std::mem::size_of::<lhr_assets::Frame>(),
    )?;
    for material in &model.materials {
        add(material.texture.capacity(), 1)?;
        for name in [&material.alpha_texture, &material.environment_texture]
            .into_iter()
            .flatten()
        {
            add(name.capacity(), 1)?;
        }
    }
    for frame in &model.frames {
        add(frame.name.capacity(), 1)?;
        add(frame.name_bytes.capacity(), 1)?;
        add(frame.properties.capacity(), 1)?;
        add(
            frame.lods.capacity(),
            std::mem::size_of::<lhr_assets::Lod>(),
        )?;
        for lod in &frame.lods {
            add(
                lod.vertices.capacity(),
                std::mem::size_of::<lhr_assets::Vertex>(),
            )?;
            add(
                lod.groups.capacity(),
                std::mem::size_of::<lhr_assets::FaceGroup>(),
            )?;
            for group in &lod.groups {
                add(group.indices.capacity(), std::mem::size_of::<u32>())?;
            }
        }
        if let Some(skin) = &frame.skin {
            add(
                skin.lods.capacity(),
                std::mem::size_of::<lhr_assets::SkinLodRecord>(),
            )?;
            for lod in &skin.lods {
                let Some(definition) = &lod.definition else {
                    continue;
                };
                add(
                    definition.bones.capacity(),
                    std::mem::size_of::<lhr_assets::SkinBoneRecord>(),
                )?;
                for bone in &definition.bones {
                    add(bone.weights.capacity(), std::mem::size_of::<f32>())?;
                }
            }
        }
    }
    Ok(total)
}
fn normalized_placement_rotation(rotation: [f32; 4]) -> Result<glam::Quat, String> {
    let length_squared = rotation.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    if !length_squared.is_finite() || length_squared < 1.0e-20 {
        return Err("Invalid placement quaternion".into());
    }
    let length = length_squared.sqrt();
    source_rotation(rotation.map(|v| (f64::from(v) / length) as f32))
}
fn actor_object<'a>(
    scene: &'a lhr_assets::Scene,
    name: &str,
    kind: u32,
) -> Result<&'a SceneObject, String> {
    let actors: Vec<_> = scene
        .actors
        .iter()
        .filter(|a| a.name.eq_ignore_ascii_case(name) && a.kind == Some(kind))
        .collect();
    let objects: Vec<_> = scene
        .objects
        .iter()
        .filter(|o| o.name.eq_ignore_ascii_case(name))
        .collect();
    if actors.len() != 1 || objects.len() != 1 {
        return Err("Mission actor/object association is absent or ambiguous".into());
    }
    Ok(objects[0])
}
fn object_transform(scene: &lhr_assets::Scene, object: &SceneObject) -> Result<Mat4, String> {
    let position = object
        .world_position
        .ok_or("Authored object world position absent")?;
    if object.position != Some(position) {
        return Err("Object local/world position disagreement is unsupported".into());
    }
    if let Some(parent) = &object.parent {
        let parents: Vec<_> = scene
            .objects
            .iter()
            .filter(|o| o.name.eq_ignore_ascii_case(parent))
            .collect();
        if parents.len() != 1
            || parents[0].position.is_some()
            || parents[0].world_position.is_some()
            || parents[0].direction.is_some()
            || parents[0].rotation_wxyz.is_some()
            || parents[0].scale.is_some()
            || parents[0].uniform_scale.is_some()
            || parents[0].parent.is_some()
        {
            return Err("Transformed or ambiguous scene object parent is unsupported".into());
        }
    }
    if object.uniform_scale.is_some_and(|s| s != 1.0) {
        return Err("Additional scene object scale is unsupported".into());
    }
    compose_placement(
        position,
        source_rotation(object.rotation_wxyz.ok_or("Object rotation absent")?)?,
        object.scale.ok_or("Object scale absent")?,
    )
}
fn placement_transform(
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
) -> Result<Mat4, String> {
    compose_placement(position, normalized_placement_rotation(rotation)?, scale)
}
fn compose_placement(
    position: [f32; 3],
    rotation: glam::Quat,
    scale: [f32; 3],
) -> Result<Mat4, String> {
    if position.iter().any(|v| !v.is_finite() || v.abs() > 1.0e8)
        || scale
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0 || *v > 10000.0)
    {
        return Err("Placement position or scale exceeds supported range".into());
    }
    let result = Mat4::from_scale_rotation_translation(
        Vec3::from_array(scale),
        rotation,
        Vec3::from_array(position),
    );
    if !result.is_finite() {
        return Err("Placement transform overflow".into());
    }
    Ok(result)
}
fn model_path(name: &str) -> Result<String, String> {
    if name.len() < 5
        || name.len() > 255
        || !name.to_ascii_lowercase().ends_with(".i3d")
        || !name.bytes().all(|b| {
            (b.is_ascii_graphic()
                && !matches!(
                    b,
                    b'/' | b'\\' | b':' | b'"' | b'<' | b'>' | b'|' | b'*' | b'?'
                ))
                || b == b' '
        })
        || name[..name.len() - 4].contains('.')
    {
        return Err(format!(
            "Unsupported cache/actor model resource name {name:?}"
        ));
    }
    Ok(format!(
        "models/{}.4ds",
        name[..name.len() - 4].to_ascii_lowercase()
    ))
}
fn select(
    cache: &CityCache,
    center: [f32; 3],
    radius: f32,
    cap: usize,
) -> Result<(Vec<&CacheObject>, usize, usize), String> {
    XzBounds::around(center, radius)?;
    if cap == 0 || cap > 256 {
        return Err("Neighborhood placement cap outside supported range".into());
    }
    let mut candidates = Vec::new();
    let mut total = 0usize;
    for (part_index, part) in cache.parts.iter().enumerate() {
        for (object_index, object) in part.objects.iter().enumerate() {
            total += 1;
            if object
                .position
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1.0e8)
            {
                return Err("Cache placement position exceeds supported range".into());
            }
            let dx = f64::from(object.position[0]) - f64::from(center[0]);
            let dz = f64::from(object.position[2]) - f64::from(center[2]);
            let distance = dx * dx + dz * dz;
            if distance <= f64::from(radius).powi(2) {
                candidates.push((distance, part_index, object_index, object));
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let count = candidates.len();
    Ok((
        candidates
            .into_iter()
            .take(cap)
            .map(|(_, _, _, o)| o)
            .collect(),
        count,
        total,
    ))
}
fn inspection_camera(world: Mat4) -> Result<Camera, String> {
    let source_target = world.transform_point3(Vec3::Y);
    let source_eye = world.transform_point3(Vec3::new(3.0, 4.5, -9.0));
    let reflect = |p: Vec3| Vec3::new(p.x, p.y, -p.z);
    let camera = Camera {
        eye: reflect(source_eye),
        target: reflect(source_target),
        up: Vec3::Y,
        near: 0.05,
        far: 400.0,
    };
    camera.validate()?;
    Ok(camera)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn placement(name: &str, position: [f32; 3]) -> CacheObject {
        CacheObject {
            model: name.into(),
            position,
            rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
            scale: [1.0; 3],
            flags: u32::MAX,
            secondary_scale: [1.0; 3],
            trailing_bytes: 0,
        }
    }
    fn part(objects: Vec<CacheObject>) -> lhr_assets::CachePart {
        lhr_assets::CachePart {
            name: String::new(),
            sphere_position: [0.0; 2],
            sphere_radius: 10.0,
            bounds_and_edges: [[0.0; 2]; 8],
            objects,
        }
    }
    #[test]
    fn selection_caps_placements_with_stable_part_and_object_ties() {
        let cache = CityCache {
            parts: vec![
                part(vec![
                    placement("first.i3d", [1.0, 0.0, 0.0]),
                    placement("second.i3d", [-1.0, 0.0, 0.0]),
                ]),
                part(vec![
                    placement("third.i3d", [0.0, 0.0, 1.0]),
                    placement("boundary.i3d", [2.0, 0.0, 0.0]),
                    placement("outside.i3d", [3.0, 0.0, 0.0]),
                ]),
            ],
            unknown_chunks: 0,
        };
        let (selected, candidates, total) = select(&cache, [0.0; 3], 2.0, 2).unwrap();
        assert_eq!((candidates, total), (4, 5));
        assert_eq!(
            selected
                .iter()
                .map(|o| o.model.as_str())
                .collect::<Vec<_>>(),
            vec!["first.i3d", "second.i3d"]
        );
        assert_eq!(
            select(&cache, [0.0; 3], 2.0, 4).unwrap().0[3].model,
            "boundary.i3d"
        );
        assert!(select(&cache, [0.0; 3], 2.0, 0).is_err());
        let mut invalid = cache.clone();
        invalid.parts[0].objects[0].position[0] = f32::NAN;
        assert!(select(&invalid, [0.0; 3], 2.0, 2).is_err());
    }
    #[test]
    fn model_names_are_single_safe_basenames_and_case_normalized() {
        assert_eq!(model_path("@Fixture.I3D").unwrap(), "models/@fixture.4ds");
        for invalid in [
            "../fixture.i3d",
            "folder/fixture.i3d",
            "folder\\fixture.i3d",
            "c:fixture.i3d",
            "fixture.4ds",
            "fixture.other.i3d",
            ".i3d",
        ] {
            assert!(model_path(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn source_budgets_aggregate_across_models_and_reject_without_mutating_counter() {
        let mut budget = SourceBudget::default();
        budget.input(MAX_MODEL_INPUT_BYTES / 2).unwrap();
        budget.input(MAX_MODEL_INPUT_BYTES / 2).unwrap();
        assert!(budget.input(1).is_err());
        assert_eq!(budget.encoded, MAX_MODEL_INPUT_BYTES);
        budget.charge_model(MAX_PARSED_MODEL_BYTES / 2).unwrap();
        budget.charge_model(MAX_PARSED_MODEL_BYTES / 2).unwrap();
        assert!(budget.charge_model(1).is_err());
        assert_eq!(budget.decoded, MAX_PARSED_MODEL_BYTES);
    }
    #[test]
    fn parsed_model_quota_counts_unused_lods_and_allocated_capacity() {
        let mut model = Model {
            materials: vec![],
            frames: vec![lhr_assets::Frame {
                kind: 6,
                visual: None,
                render_flags: 0,
                logic_flags: 0,
                culling: 0,
                name: String::new(),
                name_bytes: Vec::new(),
                properties: Vec::new(),
                dummy_bounds: None,
                joint: None,
                skin: None,
                parent: None,
                position: [0.0; 3],
                scale: [1.0; 3],
                rotation_wxyz: [1.0, 0.0, 0.0, 0.0],
                instance: None,
                lods: vec![],
            }],
        };
        let before = model_storage_bytes(&model).unwrap();
        let vertices = Vec::<lhr_assets::Vertex>::with_capacity(1024);
        model.frames[0].lods.push(lhr_assets::Lod {
            distance: 1000.0,
            vertices,
            groups: vec![lhr_assets::FaceGroup {
                material: lhr_assets::UNASSIGNED_MATERIAL,
                indices: Vec::with_capacity(1024),
            }],
        });
        let after = model_storage_bytes(&model).unwrap();
        assert!(
            after - before
                >= 1024 * (std::mem::size_of::<lhr_assets::Vertex>() + std::mem::size_of::<u32>())
        );
        let mut budget = SourceBudget {
            decoded: MAX_PARSED_MODEL_BYTES - before,
            encoded: 0,
        };
        assert!(budget.model(&model).is_err());
        assert_eq!(budget.decoded, MAX_PARSED_MODEL_BYTES - before);
    }
    fn parent_scene() -> (lhr_assets::Scene, SceneObject) {
        let parent = SceneObject {
            name: "Sector".into(),
            ..SceneObject::default()
        };
        let child = SceneObject {
            name: "Fixture".into(),
            parent: Some("Sector".into()),
            position: Some([0.0; 3]),
            world_position: Some([0.0; 3]),
            rotation_wxyz: Some([1.0, 0.0, 0.0, 0.0]),
            scale: Some([1.0; 3]),
            ..SceneObject::default()
        };
        (
            lhr_assets::Scene {
                objects: vec![parent, child.clone()],
                ..lhr_assets::Scene::default()
            },
            child,
        )
    }
    #[test]
    fn transformed_or_ambiguous_parent_metadata_is_rejected() {
        let (mut scene, child) = parent_scene();
        assert!(object_transform(&scene, &child).is_ok());
        scene.objects[0].world_position = Some([10.0, 0.0, 0.0]);
        assert!(object_transform(&scene, &child).is_err());
        scene.objects[0].world_position = None;
        scene.objects[0].direction = Some([0.0, 0.0, 1.0]);
        assert!(object_transform(&scene, &child).is_err());
        scene.objects[0].direction = None;
        scene.objects.push(scene.objects[0].clone());
        assert!(object_transform(&scene, &child).is_err());
    }
    #[test]
    fn placement_rotation_normalization_and_camera_validate_finite_geometry() {
        let world = placement_transform([-10.0, 0.0, 3.0], [2.0, 0.0, 0.0, 0.0], [1.0; 3]).unwrap();
        assert_eq!(world.transform_vector3(Vec3::X), Vec3::X);
        let camera = inspection_camera(world).unwrap();
        assert!(camera.validate().is_ok());
        assert!(placement_transform([0.0; 3], [0.0; 4], [1.0; 3]).is_err());
        assert!(placement_transform([0.0; 3], [1.0, 0.0, 0.0, 0.0], [-1.0, 1.0, 1.0]).is_err());
    }
}
