//! Source-backed null-first-material subset of PrepareLights (LS3DF RVA65a70)
//! and its inline LightVertex loop, pinned mafia-re29b65b0 T_l.cpp. The current
//! sector and mesh/LOD/light records come from the same canonical arena.
//! No native runtime equivalence or independent function credit is claimed.
use crate::frame_transform::{FrameKey, FrameKind};
use crate::i3d_object::{ObjectCleanupPhase, ObjectVisualType};
use crate::model_loading::ModelArena;
use crate::native_matrix::{checked_f32_add, checked_f32_div, checked_f32_mul, checked_f32_sub};
use crate::native_transform::SMatrix;
use crate::scene_ownership::SectorKey;
use crate::visual_resources::{MaterialKey, MeshLevelKey};

/// Actual caller-selected fade and an explicit constructor-only scalar/profile
/// admission. The current object owner has no mutable lightness/transparency
/// producer; accepting arbitrary replacements would fabricate those fields.
#[derive(Clone, Copy, Debug)]
pub struct NullMaterialLightingContext {
    pub fade: f32,
    pub lightness: f32,
    pub transparency: f32,
    pub modulate_2x: bool,
}
type Out<T> = Result<T, String>;
fn numeric() -> String {
    "null material lighting: PC24 numeric domain".into()
}
fn number(value: f32) -> Out<f32> {
    if value == 0. || value.is_normal() {
        Ok(value)
    } else {
        Err(numeric())
    }
}
fn add(a: f32, b: f32) -> Out<f32> {
    checked_f32_add(a, b).map_err(|_| numeric())
}
fn sub(a: f32, b: f32) -> Out<f32> {
    checked_f32_sub(a, b).map_err(|_| numeric())
}
fn mul(a: f32, b: f32) -> Out<f32> {
    checked_f32_mul(a, b).map_err(|_| numeric())
}
fn div(a: f32, b: f32) -> Out<f32> {
    checked_f32_div(a, b).map_err(|_| numeric())
}
fn root(value: f32) -> Out<f32> {
    let value = number(value)?;
    let result = number(value.sqrt())?;
    crate::native_precision::record_sqrt(value, result);
    Ok(result)
}
fn difference(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3]> {
    Ok([sub(a[0], b[0])?, sub(a[1], b[1])?, sub(a[2], b[2])?])
}
fn dot(a: [f32; 3], b: [f32; 3]) -> Out<f32> {
    add(add(mul(a[0], b[0])?, mul(a[1], b[1])?)?, mul(a[2], b[2])?)
}
/// PrepareLights' point-selection path has a distinct native association from
/// the inline LightVertex dot: PC65fd6..65fe4 accumulates Z, then Y, then X.
fn squared_prepare(value: [f32; 3]) -> Out<f32> {
    add(
        add(mul(value[2], value[2])?, mul(value[1], value[1])?)?,
        mul(value[0], value[0])?,
    )
}

#[derive(Clone, Copy, Debug)]
struct TemporaryLight {
    color: [f32; 3],
    energy: f32,
    kind: TemporaryLightKind,
    #[cfg(test)]
    key: FrameKey,
}
#[derive(Clone, Copy, Debug)]
enum TemporaryLightKind {
    Point {
        position: [f32; 3],
        near: f32,
        far: f32,
        far_squared: f32,
        reciprocal_range: f32,
    },
    Directional {
        direction: [f32; 3],
    },
}
struct Prepared {
    ambient: [f32; 3],
    lights: Vec<TemporaryLight>,
}
#[derive(Clone, Copy)]
struct BillboardMaterialTerms {
    material: MaterialKey,
    level: MeshLevelKey,
    temp_color: [f32; 3],
}

/// RotateByMatrix RVA2ea10 uses XYZ row products without translation. The
/// caller's FCHS negates the cached world direction before normalization.
fn rotated_negative_direction(direction: [f32; 3], inverse: &[f32; 16]) -> Out<[f32; 3]> {
    let mut local = [0.; 3];
    for axis in 0..3 {
        local[axis] = -add(
            add(
                mul(direction[0], inverse[axis])?,
                mul(direction[1], inverse[4 + axis])?,
            )?,
            mul(direction[2], inverse[8 + axis])?,
        )?;
    }
    // Inline Normalize PC66249..6625f associates X² + Y², then Z².
    let squared = dot(local, local)?;
    if f64::from(sub(squared, 1.)?.abs()) < f64::from(1e-8_f32) {
        return Ok(local);
    }
    if f64::from(squared) >= f64::from(1e-8_f32) {
        let reciprocal = div(1., root(squared)?)?;
        return Ok([
            mul(reciprocal, local[0])?,
            mul(reciprocal, local[1])?,
            mul(reciprocal, local[2])?,
        ]);
    }
    let axis = if local[0] != 0. {
        0
    } else if local[2] != 0. {
        2
    } else {
        1
    };
    local[axis] = if local[axis] < 0. { -1. } else { 1. };
    Ok(local)
}
fn selected_slot(lights: &[TemporaryLight], energy: f32) -> Option<usize> {
    if lights.len() < 8 {
        Some(lights.len())
    } else {
        lights.iter().position(|light| light.energy < energy)
    }
}

fn prepare(arena: &mut ModelArena, frame: FrameKey, sector: SectorKey) -> Out<Prepared> {
    prepare_with_billboard_matrix(arena, frame, sector, None, None)
}

fn prepare_with_billboard_matrix(
    arena: &mut ModelArena,
    frame: FrameKey,
    sector: SectorKey,
    draw_matrix: Option<SMatrix>,
    material: Option<BillboardMaterialTerms>,
) -> Out<Prepared> {
    let bounds = arena
        .visual_world_bounds(frame)
        .map_err(|e| format!("null material lighting bounds: {e:?}"))?;
    let world = match draw_matrix {
        Some(matrix) => matrix,
        None => arena
            .world_matrix(frame)
            .map_err(|e| format!("null material lighting matrix: {e:?}"))?,
    };
    let (inverse, success) = world.inverse().map_err(|_| numeric())?;
    if !success {
        return Err("null material lighting: singular object inverse".into());
    }
    // GetUScale2e580 uses each row's Magnitude, then (x+y+z)/3.
    let scales = inverse.try_get_scale().map_err(|_| numeric())?;
    let scale = mul(
        add(add(scales[0], scales[1])?, scales[2])?,
        f32::from_bits(0x3eaaaaab),
    )?;
    let light_keys = arena
        .sector(sector)
        .map_err(|e| format!("null material lighting sector: {e:?}"))?
        .lights
        .clone();
    let mut out = Prepared {
        ambient: [0.; 3],
        lights: Vec::new(),
    };
    out.lights
        .try_reserve_exact(8)
        .map_err(|_| "null material lighting allocation".to_string())?;
    for key in light_keys {
        let light = arena
            .light(key)
            .map_err(|e| format!("null material lighting light: {e:?}"))?
            .clone();
        if light.mode & 1 == 0 || arena.state(key).map_err(|e| format!("{e:?}"))?.flags & 1 == 0 {
            continue;
        }
        if number(light.intensity)?.to_bits() & 0x7fff_ffff < 0x322bcc77 {
            continue;
        }
        match light.light_type {
            0 | 5 | 6 | 7 => continue,
            4 => {
                for (sum, color) in out.ambient.iter_mut().zip(light.scaled_color) {
                    *sum = add(*sum, mul(color, 1.)?)?;
                }
            }
            1 => {
                // Native selection updates the light's world matrix for its
                // sphere test, then transforms its already-produced w_pos.
                let light_world = arena.world_matrix(key).map_err(|e| format!("{e:?}"))?;
                let light_position = light_world.elements[12..15].try_into().unwrap();
                let d = difference(bounds.center, light_position)?;
                let squared = squared_prepare(d)?;
                let radius = add(bounds.radius, light.far_scaled)?;
                if mul(radius, radius)? <= squared {
                    continue;
                }
                let energy = div(
                    // PC66008..6601d adds blue + green + red.
                    add(
                        add(light.scaled_color[2], light.scaled_color[1])?,
                        light.scaled_color[0],
                    )?,
                    root(squared)?,
                )?;
                let slot = selected_slot(&out.lights, energy);
                let Some(slot) = slot else { continue };
                let near = mul(scale, light.near_scaled)?;
                let far = mul(scale, light.far_scaled)?;
                let delta = sub(far, near)?;
                let reciprocal_range = if delta > 0. { div(1., delta)? } else { delta };
                let position = inverse
                    .try_transform_point(light.position)
                    .map_err(|_| numeric())?;
                let point = TemporaryLight {
                    color: [
                        match material {
                            Some(m) => mul(m.temp_color[0], light.scaled_color[0])?,
                            None => mul(light.scaled_color[0], 1.)?,
                        },
                        match material {
                            Some(m) => mul(m.temp_color[1], light.scaled_color[1])?,
                            None => mul(light.scaled_color[1], 1.)?,
                        },
                        match material {
                            Some(m) => mul(m.temp_color[2], light.scaled_color[2])?,
                            None => mul(light.scaled_color[2], 1.)?,
                        },
                    ],
                    energy,
                    kind: TemporaryLightKind::Point {
                        position,
                        near,
                        far,
                        far_squared: mul(far, far)?,
                        reciprocal_range,
                    },
                    #[cfg(test)]
                    key,
                };
                if slot == out.lights.len() {
                    out.lights.push(point);
                } else {
                    out.lights[slot] = point;
                }
            }
            3 => {
                // PC66177..66249 selects from the same eight slots as point
                // lights, then rotates the cached UpdateV world direction.
                let energy = add(
                    add(light.scaled_color[2], light.scaled_color[1])?,
                    light.scaled_color[0],
                )?;
                let Some(slot) = selected_slot(&out.lights, energy) else {
                    continue;
                };
                let direction = rotated_negative_direction(light.direction, &inverse.elements)?;
                let directional = TemporaryLight {
                    color: [
                        match material {
                            Some(m) => mul(m.temp_color[0], light.scaled_color[0])?,
                            None => mul(light.scaled_color[0], 1.)?,
                        },
                        match material {
                            Some(m) => mul(m.temp_color[1], light.scaled_color[1])?,
                            None => mul(light.scaled_color[1], 1.)?,
                        },
                        match material {
                            Some(m) => mul(m.temp_color[2], light.scaled_color[2])?,
                            None => mul(light.scaled_color[2], 1.)?,
                        },
                    ],
                    energy,
                    kind: TemporaryLightKind::Directional { direction },
                    #[cfg(test)]
                    key,
                };
                if slot == out.lights.len() {
                    out.lights.push(directional);
                } else {
                    out.lights[slot] = directional;
                }
            }
            _ => {
                return Err(format!(
                    "null material lighting: contributing light type {} unavailable",
                    light.light_type
                ))
            }
        }
    }
    if let Some(material) = material {
        // PrepareLights applies the first face group's ambient material after
        // the light walk. Keep the early material identity while reading its
        // mutable ambient/emissive and the level's aggregate flags only now.
        let owner = arena
            .loaded_visual_resources()
            .ok_or("null material lighting: missing visual owner")?;
        let resources = owner.objects().managed().resources();
        let level = resources
            .mesh_level(material.level)
            .map_err(|e| format!("{e:?}"))?;
        let current = resources
            .material(material.material)
            .map_err(|e| format!("{e:?}"))?;
        let use_material_color = level.mesh_flags & 0x10 == 0 || level.mesh_flags & 0x20 != 0;
        if use_material_color {
            for channel in 0..3 {
                out.ambient[channel] =
                    mul(out.ambient[channel], number(current.ambient[channel])?)?;
            }
        }
        for channel in 0..3 {
            out.ambient[channel] = add(out.ambient[channel], number(current.emissive[channel])?)?;
        }
    }
    Ok(out)
}

fn channel(value: f32) -> Out<u8> {
    let value = mul(value, 128.)?.round_ties_even();
    if value < i32::MIN as f32 || value >= 2147483648. {
        return Err(numeric());
    }
    let integer = value as i32;
    // LightVertex tests all bits above the low byte: negative colors also
    // become255. This deliberately follows the source rather than clamp(0,1).
    Ok(if integer & !255 != 0 {
        255
    } else {
        integer as u8
    })
}

fn vertex(prepared: &Prepared, position: [f32; 3], normal: [f32; 3]) -> Out<[u8; 3]> {
    let mut color = prepared.ambient;
    for light in &prepared.lights {
        let contribution = match light.kind {
            TemporaryLightKind::Directional { direction } => {
                // Inline LightVertex PC66880..668a6 tests the sign bit;
                // PC66a30 applies the common marginal-zero threshold below.
                let cosine = dot(direction, normal)?;
                if cosine.to_bits() & 0x8000_0000 != 0 {
                    continue;
                }
                cosine
            }
            TemporaryLightKind::Point {
                position: light_position,
                near,
                far,
                far_squared,
                reciprocal_range,
            } => {
                let delta = difference(light_position, position)?;
                let cosine = dot(delta, normal)?;
                if cosine.to_bits() & 0x8000_0000 != 0 {
                    continue;
                }
                let squared = dot(delta, delta)?;
                if squared > far_squared {
                    continue;
                }
                let distance = root(squared)?;
                let mut contribution = cosine;
                if distance > near {
                    contribution = mul(contribution, mul(sub(far, distance)?, reciprocal_range)?)?;
                }
                contribution = div(contribution, distance)?;
                contribution
            }
        };
        if (contribution.to_bits() as i32) < 0x322bcc77 {
            continue;
        }
        for (sum, light_color) in color.iter_mut().zip(light.color) {
            *sum = add(*sum, mul(light_color, contribution)?)?;
        }
    }
    Ok([channel(color[0])?, channel(color[1])?, channel(color[2])?])
}

/// Fresh evaluation of the admitted PrepareLights write branch. Inputs are the
/// actual selected local positions/normals, using the owned pose for deformed visuals.
/// Output is the source packed128-profile diffuse channels normalized for GPU
/// upload; MODULATE2X remains an explicit downstream pipeline operation.
pub fn null_material_vertex_lighting(
    arena: &mut ModelArena,
    frame: FrameKey,
    sector: SectorKey,
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    context: NullMaterialLightingContext,
) -> Out<Vec<[f32; 4]>> {
    if context.lightness != 1. || context.transparency != 1. || !context.modulate_2x {
        return Err("null material lighting: unsupported scalar or vertex-light profile".into());
    }
    if !(0. ..=1.).contains(&number(context.fade)?) || positions.len() != normals.len() {
        return Err("null material lighting: fade/vertex extent".into());
    }
    let frame_kind = arena.state(frame).map_err(|e| format!("{e:?}"))?.kind;
    if !matches!(frame_kind, FrameKind::Mesh | FrameKind::Singlemesh) {
        return Err("null material lighting: derived visual unavailable".into());
    }
    let object = arena
        .frame_object_resources(frame)
        .map_err(|e| format!("{e:?}"))?
        .ok_or("null material lighting: missing object owner")?;
    let owner = arena
        .loaded_visual_resources()
        .ok_or("null material lighting: missing visual owner")?;
    let object = owner
        .objects()
        .object(object)
        .map_err(|e| format!("{e:?}"))?;
    let admitted = matches!(
        (frame_kind, object.visual_type()),
        (FrameKind::Mesh, ObjectVisualType::Plain)
            | (
                FrameKind::Singlemesh,
                ObjectVisualType::Singlemesh | ObjectVisualType::Singlemorph
            )
    );
    if !admitted || object.phase != ObjectCleanupPhase::Alive {
        return Err("null material lighting: object class/lifecycle".into());
    }
    let resources = owner.objects().managed().resources();
    let mesh = resources
        .mesh_object(object.mesh.ok_or("null material lighting: missing mesh")?)
        .map_err(|e| format!("{e:?}"))?;
    let level = *mesh
        .levels
        .get(object.cur_lod as usize)
        .ok_or("null material lighting: LOD extent")?;
    let level = resources
        .mesh_level(level.ok_or("null material lighting: missing LOD")?)
        .map_err(|e| format!("{e:?}"))?;
    let first = resources
        .face_group(
            level
                .fgroups
                .ok_or("null material lighting: no face groups")?,
        )
        .map_err(|e| format!("{e:?}"))?;
    if first.material.is_some() || level.num_vertices as usize != positions.len() {
        return Err("null material lighting: first material/actual vertex extent".into());
    }
    let average = arena
        .state(frame)
        .map_err(|e| format!("{e:?}"))?
        .visual_flags
        & 0x80
        != 0;
    let prepared = prepare(arena, frame, sector)?;
    let alpha = mul(context.fade, 255.)?.round_ties_even() as u8;
    let mut packed = Vec::new();
    packed
        .try_reserve_exact(positions.len())
        .map_err(|_| "null material lighting allocation".to_string())?;
    for (&position, &normal) in positions.iter().zip(normals) {
        packed.push(vertex(&prepared, position, normal)?);
    }
    if average {
        if packed.is_empty() {
            return Err("null material lighting: empty average".into());
        }
        let mut sums = [0u32; 3];
        for color in &packed {
            for (sum, value) in sums.iter_mut().zip(color) {
                *sum = sum.wrapping_add(u32::from(*value));
            }
        }
        let count = u32::try_from(packed.len())
            .map_err(|_| "null material lighting: vertex quota".to_string())?;
        let mean = sums.map(|sum| (sum / count).min(255) as u8);
        packed.fill(mean);
    }
    Ok(packed
        .into_iter()
        .map(|c| {
            [
                f32::from(c[0]) / 255.,
                f32::from(c[1]) / 255.,
                f32::from(c[2]) / 255.,
                f32::from(alpha) / 255.,
            ]
        })
        .collect())
}

/// Billboard m_AddFGroups calls ProductBillMatrix before PrepareLights and
/// passes the inverse of that per-pass matrix. Its first selected face-group
/// material sets the diffuse/ambient/emissive and alpha terms for the entire
/// selected vertex buffer, including later groups with different materials.
/// The returned packed128 RGB already includes those terms. A presentation
/// consumer must use neutral material RGB with these vertex colors; its
/// material opacity remains the source of output alpha in the current shader.
pub fn billboard_vertex_lighting(
    arena: &mut ModelArena,
    frame: FrameKey,
    sector: SectorKey,
    draw_matrix: SMatrix,
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    fade: f32,
) -> Out<Vec<[f32; 4]>> {
    fn billboard_error(error: String) -> String {
        if error.starts_with("null material lighting") {
            error.replacen("null material lighting", "billboard lighting", 1)
        } else {
            format!("billboard lighting: {error}")
        }
    }
    let result = (|| -> Out<Vec<[f32; 4]>> {
        if !(0. ..=1.).contains(&number(fade)?) || positions.len() != normals.len() {
            return Err("fade/vertex extent".into());
        }
        if arena.state(frame).map_err(|e| format!("{e:?}"))?.kind != FrameKind::Mesh {
            return Err("derived frame class unavailable".into());
        }
        let object = arena
            .frame_object_resources(frame)
            .map_err(|e| format!("{e:?}"))?
            .ok_or("missing object owner")?;
        let owner = arena
            .loaded_visual_resources()
            .ok_or("missing visual owner")?;
        let object = owner
            .objects()
            .object(object)
            .map_err(|e| format!("{e:?}"))?;
        if object.visual_type() != ObjectVisualType::Billboard
            || object.phase != ObjectCleanupPhase::Alive
        {
            return Err("object class/lifecycle".into());
        }
        let resources = owner.objects().managed().resources();
        let mesh = resources
            .mesh_object(object.mesh.ok_or("missing mesh")?)
            .map_err(|e| format!("{e:?}"))?;
        let level = *mesh
            .levels
            .get(object.cur_lod as usize)
            .ok_or("LOD extent")?;
        let level_key = level.ok_or("missing LOD")?;
        let level = resources
            .mesh_level(level_key)
            .map_err(|e| format!("{e:?}"))?;
        if level.num_vertices as usize != positions.len() {
            return Err("selected vertex extent".into());
        }
        let first = resources
            .face_group(level.fgroups.ok_or("no face groups")?)
            .map_err(|e| format!("{e:?}"))?;
        let fade_times_transparency = mul(fade, 1.)?;
        let (terms, alpha) = if let Some(key) = first.material {
            let material = resources.material(key).map_err(|e| format!("{e:?}"))?;
            let use_material_color = level.mesh_flags & 0x10 == 0 || level.mesh_flags & 0x20 != 0;
            if !(0. ..=1.).contains(&number(material.alpha)?) {
                return Err("material alpha domain".into());
            }
            // PC65a70 computes the packed alpha before multiplying TempColor
            // by the selected first material's conditional diffuse RGB.
            let alpha =
                mul(mul(fade_times_transparency, material.alpha)?, 255.)?.round_ties_even() as u8;
            let temp_color = if use_material_color {
                let mut color = [1.; 3];
                for channel in [1, 2, 0] {
                    color[channel] = mul(1., number(material.diffuse[channel])?)?;
                }
                color
            } else {
                [1.; 3]
            };
            (
                Some(BillboardMaterialTerms {
                    material: key,
                    level: level_key,
                    temp_color,
                }),
                alpha,
            )
        } else {
            (
                None,
                mul(fade_times_transparency, 255.)?.round_ties_even() as u8,
            )
        };
        let average = arena
            .state(frame)
            .map_err(|e| format!("{e:?}"))?
            .visual_flags
            & 0x80
            != 0;
        // The canonical frame world matrix is deliberately not read here.
        // Native passes the inverse of the just-produced g_matrix to lights.
        let prepared =
            prepare_with_billboard_matrix(arena, frame, sector, Some(draw_matrix), terms)?;
        let mut packed = Vec::new();
        packed
            .try_reserve_exact(positions.len())
            .map_err(|_| "allocation".to_string())?;
        for (&position, &normal) in positions.iter().zip(normals) {
            packed.push(vertex(&prepared, position, normal)?);
        }
        if average {
            if packed.is_empty() {
                return Err("empty average".into());
            }
            let mut sums = [0u32; 3];
            for color in &packed {
                for (sum, value) in sums.iter_mut().zip(color) {
                    *sum = sum.wrapping_add(u32::from(*value));
                }
            }
            let count = u32::try_from(packed.len()).map_err(|_| "vertex quota".to_string())?;
            packed.fill(sums.map(|sum| (sum / count).min(255) as u8));
        }
        Ok(packed
            .into_iter()
            .map(|c| {
                [
                    f32::from(c[0]) / 255.,
                    f32::from(c[1]) / 255.,
                    f32::from(c[2]) / 255.,
                    f32::from(alpha) / 255.,
                ]
            })
            .collect())
    })();
    result.map_err(billboard_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_loading::ModelKey;
    use crate::scene_ownership::SceneKey;
    use crate::scene_resources::SceneScreen;
    use std::convert::Infallible;
    use std::sync::Arc;
    static FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    const POSITIONS: [[f32; 3]; 3] = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
    const NORMALS: [[f32; 3]; 3] = [[0., 0., 1.]; 3];
    struct CameraScreen;
    impl SceneScreen for CameraScreen {
        type Error = Infallible;
        fn screen_sx(&mut self) -> Result<i32, Self::Error> {
            Ok(64)
        }
        fn screen_sy(&mut self) -> Result<i32, Self::Error> {
            Ok(64)
        }
    }
    fn context() -> NullMaterialLightingContext {
        NullMaterialLightingContext {
            fade: 1.,
            lightness: 1.,
            transparency: 1.,
            modulate_2x: true,
        }
    }
    fn world() -> (ModelArena, SceneKey, SectorKey, ModelKey, FrameKey) {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let model = arena.create_model().unwrap();
        arena
            .link_frame(model.root(), Some(sector.frame()), 0)
            .unwrap();
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.render_flags = 0;
        frame.logic_flags = 0x2a;
        frame.lods = vec![lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: POSITIONS
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
        let content = arena
            .load_parsed_into_model(
                model,
                Arc::new(lhr_assets::Model {
                    materials: vec![],
                    frames: vec![frame],
                }),
            )
            .unwrap();
        let frame = content.frames()[0];
        let dir = std::env::temp_dir().join(format!(
            "lhr-null-lighting-{}-{}",
            std::process::id(),
            FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        std::fs::remove_dir(&dir).unwrap();
        (arena, scene, sector, model, frame)
    }
    fn light(
        arena: &mut ModelArena,
        scene: SceneKey,
        sector: SectorKey,
        kind: u32,
        color: [f32; 3],
        position: [f32; 3],
    ) -> FrameKey {
        let key = arena.create_light().unwrap();
        arena.graph.set_light_type(key, kind).unwrap();
        arena.graph.set_light_mode(key, 1).unwrap();
        arena.graph.set_light_diffuse(key, color).unwrap();
        arena.graph.set_light_range(key, 2., 4.).unwrap();
        arena.graph.assign_position(key, position).unwrap();
        arena.link_frame(key, Some(sector.frame()), 0).unwrap();
        arena.update_frame(key).unwrap();
        arena.sector_add_light(sector, key).unwrap();
        arena.scene_add_frame(scene, key).unwrap();
        arena.release(key).unwrap();
        key
    }

    fn billboard_world(
        material: Option<lhr_assets::Material>,
    ) -> (
        ModelArena,
        SceneKey,
        SectorKey,
        ModelKey,
        FrameKey,
        crate::frame_camera::CameraKey,
    ) {
        let mut arena = ModelArena::default();
        let needs_texture = material
            .as_ref()
            .is_some_and(|material| !material.texture.is_empty());
        let second_material_index = usize::from(material.is_some());
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let model = arena.create_model().unwrap();
        arena
            .link_frame(model.root(), Some(sector.frame()), 0)
            .unwrap();
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.visual = Some(4);
        frame.render_flags = 0;
        frame.logic_flags = 0;
        frame.lods.push(lhr_assets::Lod {
            distance: 16.,
            uv_only: None,
            vertices: POSITIONS
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., 1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![
                lhr_assets::FaceGroup {
                    material: if material.is_some() {
                        0
                    } else {
                        lhr_assets::UNASSIGNED_MATERIAL
                    },
                    indices: vec![0, 1, 2],
                },
                lhr_assets::FaceGroup {
                    material: second_material_index as _,
                    indices: vec![0, 1, 2],
                },
            ],
        });
        let later = lhr_assets::Material {
            flags: 0,
            ambient: [0.; 3],
            diffuse: [0.; 3],
            texture: String::new(),
            emission: [1., 1., 1.],
            opacity: 1.,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        };
        let mut materials: Vec<_> = material.into_iter().collect();
        materials.push(later);
        let source = Arc::new(lhr_assets::Model {
            materials,
            frames: vec![frame],
        });
        let suffix = [lhr_assets::BillboardSource {
            frame_index: 0,
            rot_axis_word: 2,
            axis_mode_byte: 0,
        }];
        let content = arena
            .load_parsed_billboards_into_model(model, source, &suffix)
            .unwrap();
        let frame = content.frames()[0];
        let dir = std::env::temp_dir().join(format!(
            "lhr-billboard-light-{}-{}",
            std::process::id(),
            FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        if needs_texture {
            std::fs::create_dir_all(dir.join("maps")).unwrap();
            let mut tga = vec![0; 18];
            tga[2] = 2;
            tga[12] = 1;
            tga[14] = 1;
            tga[16] = 24;
            tga[17] = 0x20;
            tga.extend_from_slice(&[3, 5, 7]);
            std::fs::write(dir.join("maps/bb.tga"), tga).unwrap();
        }
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let camera = arena.create_camera(&mut CameraScreen).unwrap();
        (arena, scene, sector, model, frame, camera)
    }

    fn billboard_material() -> lhr_assets::Material {
        lhr_assets::Material {
            flags: 0,
            ambient: [0.5, 0.25, 0.5],
            diffuse: [0.5, 0.25, 1.],
            texture: String::new(),
            emission: [0.125, 0., 0.],
            opacity: 0.5,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        }
    }

    #[test]
    fn billboard_first_assigned_material_applies_ambient_emissive_and_packed_alpha() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame, camera) =
                billboard_world(Some(billboard_material()));
            light(&mut arena, scene, sector, 4, [1.; 3], [0.; 3]);
            arena.camera_set_position(camera, [10., 0., 0.]).unwrap();
            let matrix = arena.billboard_render_matrix(frame, camera).unwrap();
            let result = billboard_vertex_lighting(
                &mut arena, frame, sector, matrix, &POSITIONS, &NORMALS, 0.5,
            )
            .unwrap();
            assert_eq!(
                result,
                vec![[80. / 255., 32. / 255., 64. / 255., 64. / 255.]; 3]
            );
            assert_eq!(arena.world_matrix(frame).unwrap(), SMatrix::identity());
        });
    }

    #[test]
    fn billboard_live_mesh_texture_and_keep_diffuse_flags_control_first_material_terms() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            for (flags, expected) in [
                (0x40000, [128. / 255., 128. / 255., 128. / 255., 1.]),
                (
                    0x40000 | 0x8000000,
                    [64. / 255., 32. / 255., 64. / 255., 1.],
                ),
            ] {
                let mut material = billboard_material();
                material.flags = flags;
                material.texture = "bb.tga".into();
                material.emission = [0.; 3];
                material.opacity = 1.;
                let (mut arena, scene, sector, _model, frame, camera) =
                    billboard_world(Some(material));
                let object = arena.frame_object_resources(frame).unwrap().unwrap();
                let owner = arena.loaded_visual_resources().unwrap();
                let object = owner.objects().object(object).unwrap();
                let resources = owner.objects().managed().resources();
                let mesh = resources.mesh_object(object.mesh.unwrap()).unwrap();
                let level = resources.mesh_level(mesh.levels[0].unwrap()).unwrap();
                assert_eq!(
                    level.mesh_flags & 0x30,
                    if flags & 0x8000000 != 0 { 0x30 } else { 0x10 }
                );
                light(&mut arena, scene, sector, 4, [1.; 3], [0.; 3]);
                arena.camera_set_position(camera, [10., 0., 0.]).unwrap();
                let matrix = arena.billboard_render_matrix(frame, camera).unwrap();
                assert_eq!(
                    billboard_vertex_lighting(
                        &mut arena, frame, sector, matrix, &POSITIONS, &NORMALS, 1.
                    )
                    .unwrap()[0],
                    expected
                );
            }
        });
    }

    #[test]
    fn billboard_live_directional_uses_each_camera_draw_matrix_not_frame_world() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame, camera) = billboard_world(None);
            let light = light(&mut arena, scene, sector, 3, [1.; 3], [0.; 3]);
            arena
                .graph
                .assign_rotation(
                    light,
                    crate::native_transform::SQuat {
                        wxyz: [0.70710677, 0., -0.70710677, 0.],
                    },
                )
                .unwrap();
            arena.update_frame(light).unwrap();
            arena.camera_set_position(camera, [10., 0., 0.]).unwrap();
            let a_matrix = arena.billboard_render_matrix(frame, camera).unwrap();
            let a = billboard_vertex_lighting(
                &mut arena, frame, sector, a_matrix, &POSITIONS, &NORMALS, 1.,
            )
            .unwrap();
            arena.camera_set_position(camera, [0., 0., 10.]).unwrap();
            let b_matrix = arena.billboard_render_matrix(frame, camera).unwrap();
            let b = billboard_vertex_lighting(
                &mut arena, frame, sector, b_matrix, &POSITIONS, &NORMALS, 1.,
            )
            .unwrap();
            assert_ne!(a_matrix, b_matrix);
            assert_ne!(a, b);
            assert_eq!(arena.world_matrix(frame).unwrap(), SMatrix::identity());
        });
    }

    #[test]
    fn billboard_lighting_rejects_false_selected_extent_and_singular_draw_matrix() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, _scene, sector, _model, frame, camera) = billboard_world(None);
            arena.camera_set_position(camera, [10., 0., 0.]).unwrap();
            let matrix = arena.billboard_render_matrix(frame, camera).unwrap();
            assert_eq!(
                billboard_vertex_lighting(
                    &mut arena,
                    frame,
                    sector,
                    matrix,
                    &POSITIONS[..2],
                    &NORMALS[..2],
                    1.
                )
                .unwrap_err(),
                "billboard lighting: selected vertex extent"
            );
            assert_eq!(
                billboard_vertex_lighting(
                    &mut arena,
                    frame,
                    sector,
                    SMatrix { elements: [0.; 16] },
                    &POSITIONS,
                    &NORMALS,
                    1.
                )
                .unwrap_err(),
                "billboard lighting: singular object inverse"
            );
        });
    }
    #[test]
    fn owned_singlemesh_null_first_group_uses_live_deformed_vertex_extent() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut source = (*crate::model_loading::tests::skinned_source()).clone();
            source.frames[0].lods[0].vertices.push(lhr_assets::Vertex {
                position: [0., 0., 0.],
                normal: [0., 0., 1.],
                uv: [0., 1.],
            });
            source.frames[0].lods[0].groups.push(lhr_assets::FaceGroup {
                material: lhr_assets::UNASSIGNED_MATERIAL,
                indices: vec![0, 1, 2],
            });
            source.frames[0].skin.as_mut().unwrap().lods[0]
                .definition
                .as_mut()
                .unwrap()
                .root_vertex_count = 2;
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let sector = arena.scene(scene).unwrap().sectors[0];
            let model = arena.create_model().unwrap();
            arena
                .link_frame(model.root(), Some(sector.frame()), 0)
                .unwrap();
            let content = arena
                .load_parsed_into_model(model, Arc::new(source))
                .unwrap();
            let frame = content.frames()[0];
            let dir = std::env::temp_dir().join(format!(
                "lhr-null-singlemesh-{}-{}",
                std::process::id(),
                FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let installation = lhr_assets::Installation::open(&dir).unwrap();
            arena
                .load_plain_visual_resources(model, &installation)
                .unwrap();
            std::fs::remove_dir(&dir).unwrap();
            let pose = arena.owned_singlemesh_lod0_vertices(frame, 0).unwrap();
            let positions = pose
                .iter()
                .map(|vertex| vertex.position)
                .collect::<Vec<_>>();
            let normals = pose.iter().map(|vertex| vertex.normal).collect::<Vec<_>>();
            assert_eq!(positions.len(), 3);
            light(&mut arena, scene, sector, 4, [0.5, 0., 0.], [0.; 3]);
            let output = null_material_vertex_lighting(
                &mut arena,
                frame,
                sector,
                &positions,
                &normals,
                context(),
            )
            .unwrap();
            assert_eq!(output, vec![[64. / 255., 0., 0., 1.]; 3]);
            assert_eq!(
                null_material_vertex_lighting(
                    &mut arena,
                    frame,
                    sector,
                    &positions[..2],
                    &normals[..2],
                    context()
                )
                .unwrap_err(),
                "null material lighting: first material/actual vertex extent"
            );
        });
    }
    fn colors(arena: &mut ModelArena, frame: FrameKey, sector: SectorKey) -> Vec<[f32; 4]> {
        null_material_vertex_lighting(arena, frame, sector, &POSITIONS, &NORMALS, context())
            .unwrap()
    }
    #[test]
    fn zero_ambient_and_light_admission_use_live_registered_state() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            assert_eq!(colors(&mut arena, frame, sector), vec![[0., 0., 0., 1.]; 3]);
            let ambient = light(&mut arena, scene, sector, 4, [0.5, 1., 0.], [0.; 3]);
            assert_eq!(
                colors(&mut arena, frame, sector)[0],
                [64. / 255., 128. / 255., 0., 1.]
            );
            arena.set_frame_on(ambient, 0).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena.set_frame_on(ambient, 1).unwrap();
            arena.graph.set_light_mode(ambient, 2).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena.graph.set_light_mode(ambient, 1).unwrap();
            arena.graph.set_light_diffuse_power(ambient, 0.).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
        });
    }
    #[test]
    fn point_near_linear_far_outside_and_back_faces_follow_actual_light_caches() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let key = light(&mut arena, scene, sector, 1, [1.; 3], [0., 0., 2.]);
            assert_eq!(
                colors(&mut arena, frame, sector)[0],
                [128. / 255., 128. / 255., 128. / 255., 1.]
            );
            arena.graph.assign_position(key, [0., 0., 3.]).unwrap();
            arena.update_frame(key).unwrap();
            assert_eq!(
                colors(&mut arena, frame, sector)[0],
                [64. / 255., 64. / 255., 64. / 255., 1.]
            );
            arena.graph.assign_position(key, [0., 0., 4.]).unwrap();
            arena.update_frame(key).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena.graph.assign_position(key, [0., 0., 5.]).unwrap();
            arena.update_frame(key).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena.graph.assign_position(key, [0., 0., -2.]).unwrap();
            arena.update_frame(key).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
        });
    }
    #[test]
    fn eight_slots_replace_first_weaker_in_source_order_instead_of_weakest() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let first = light(&mut arena, scene, sector, 1, [0.2, 0., 0.], [0., 0., 2.]);
            let weakest = light(&mut arena, scene, sector, 1, [0., 0.1, 0.], [0., 0., 2.]);
            for _ in 0..6 {
                light(&mut arena, scene, sector, 1, [0., 0., 0.4], [0., 0., 2.]);
            }
            let last = light(&mut arena, scene, sector, 1, [0.3, 0., 0.], [0., 0., 2.]);
            let selected = prepare(&mut arena, frame, sector).unwrap();
            assert_eq!(selected.lights.len(), 8);
            assert_eq!(selected.lights[0].key, last);
            assert_eq!(selected.lights[1].key, weakest);
            assert!(!selected.lights.iter().any(|p| p.key == first));
            assert_eq!(
                colors(&mut arena, frame, sector)[0],
                [38. / 255., 13. / 255., 1., 1.]
            );
        });
    }
    #[test]
    fn inverse_object_space_preserves_point_illumination_and_rn_fade() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, model, frame) = world();
            light(&mut arena, scene, sector, 1, [1.; 3], [10., 0., 4.]);
            arena
                .graph
                .assign_position(model.root(), [10., 0., 0.])
                .unwrap();
            arena.graph.assign_scale(model.root(), [2.; 3]).unwrap();
            arena.update_frame(model.root()).unwrap();
            let mut ctx = context();
            ctx.fade = 0.5;
            let result =
                null_material_vertex_lighting(&mut arena, frame, sector, &POSITIONS, &NORMALS, ctx)
                    .unwrap();
            // Inverse-scale changes the point position/ranges together: the
            // vertex remains at the far boundary, and RN(127.5) gives128 alpha.
            assert_eq!(result[0], [0., 0., 0., 128. / 255.]);
        });
    }

    #[test]
    fn actual_directional_update_changes_front_and_back_lighting() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let key = light(&mut arena, scene, sector, 3, [1., 0.5, 0.], [0.; 3]);
            arena
                .graph
                .assign_rotation(
                    key,
                    crate::native_transform::SQuat {
                        wxyz: [0., 0., 1., 0.],
                    },
                )
                .unwrap();
            arena.update_frame(key).unwrap();
            assert_eq!(arena.light(key).unwrap().direction, [0., 0., -1.]);
            assert_eq!(
                colors(&mut arena, frame, sector)[0],
                [128. / 255., 64. / 255., 0., 1.]
            );
            arena.graph.set_light_mode(key, 2).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena.graph.set_light_mode(key, 1).unwrap();
            arena.set_frame_on(key, 0).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena.set_frame_on(key, 1).unwrap();
            arena
                .graph
                .assign_rotation(
                    key,
                    crate::native_transform::SQuat {
                        wxyz: [1., 0., 0., 0.],
                    },
                )
                .unwrap();
            arena.update_frame(key).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
        });
    }

    #[test]
    fn directional_uses_inverse_object_rotation_scale_and_local_normal() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, model, frame) = world();
            let key = light(&mut arena, scene, sector, 3, [1.; 3], [0.; 3]);
            arena
                .graph
                .assign_rotation(
                    key,
                    crate::native_transform::SQuat {
                        wxyz: [1., 0., 0., 0.],
                    },
                )
                .unwrap();
            arena.update_frame(key).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena
                .graph
                .assign_rotation(
                    model.root(),
                    crate::native_transform::SQuat {
                        wxyz: [0., 0., 1., 0.],
                    },
                )
                .unwrap();
            arena
                .graph
                .assign_scale(model.root(), [2., 1., 2.])
                .unwrap();
            arena.update_frame(model.root()).unwrap();
            assert_eq!(
                colors(&mut arena, frame, sector)[0],
                [128. / 255., 128. / 255., 128. / 255., 1.]
            );
            let back = [[0., 0., -1.]; 3];
            assert_eq!(
                null_material_vertex_lighting(
                    &mut arena,
                    frame,
                    sector,
                    &POSITIONS,
                    &back,
                    context()
                )
                .unwrap()[0],
                [0., 0., 0., 1.]
            );
        });
    }

    #[test]
    fn point_and_directional_share_eight_slots_and_first_weaker_replacement() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let first = light(&mut arena, scene, sector, 1, [0.2, 0., 0.], [0., 0., 2.]);
            for _ in 0..7 {
                light(&mut arena, scene, sector, 1, [0.1, 0., 0.], [0., 0., 2.]);
            }
            let directional = light(&mut arena, scene, sector, 3, [1.; 3], [0.; 3]);
            let selected = prepare(&mut arena, frame, sector).unwrap();
            assert_eq!(selected.lights.len(), 8);
            assert_eq!(selected.lights[0].key, directional);
            assert!(!selected.lights.iter().any(|light| light.key == first));
        });
    }

    #[test]
    fn directional_nonfinite_cache_stops_in_numeric_prefix_after_live_gate() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let key = light(&mut arena, scene, sector, 3, [1.; 3], [0.; 3]);
            arena.graph.light_state_mut(key).unwrap().direction[0] = f32::NAN;
            assert_eq!(
                null_material_vertex_lighting(
                    &mut arena,
                    frame,
                    sector,
                    &POSITIONS,
                    &NORMALS,
                    context()
                )
                .unwrap_err(),
                "null material lighting: PC24 numeric domain"
            );
            arena.graph.set_light_mode(key, 2).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
        });
    }

    #[test]
    fn directional_uses_shared_lightvertex_marginal_zero_threshold() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let key = light(&mut arena, scene, sector, 3, [1e10, 0., 0.], [0.; 3]);
            arena
                .graph
                .assign_rotation(
                    key,
                    crate::native_transform::SQuat {
                        wxyz: [0., 0., 1., 0.],
                    },
                )
                .unwrap();
            arena.update_frame(key).unwrap();
            let below = [[0., 0., f32::from_bits(0x322b_cc76)]; 3];
            let at = [[0., 0., f32::from_bits(0x322b_cc77)]; 3];
            assert_eq!(
                null_material_vertex_lighting(
                    &mut arena,
                    frame,
                    sector,
                    &POSITIONS,
                    &below,
                    context()
                )
                .unwrap()[0],
                [0., 0., 0., 1.]
            );
            assert_eq!(
                null_material_vertex_lighting(
                    &mut arena,
                    frame,
                    sector,
                    &POSITIONS,
                    &at,
                    context()
                )
                .unwrap()[0],
                [1., 0., 0., 1.]
            );
        });
    }

    #[test]
    fn equal_energy_ninth_light_keeps_the_first_eight_actual_owners() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let mut lights = Vec::new();
            for _ in 0..9 {
                lights.push(light(&mut arena, scene, sector, 1, [0.01; 3], [0., 0., 2.]));
            }
            let selected = prepare(&mut arena, frame, sector).unwrap();
            assert_eq!(
                selected.lights.iter().map(|p| p.key).collect::<Vec<_>>(),
                lights[..8]
            );
            assert_eq!(
                colors(&mut arena, frame, sector)[0],
                [10. / 255., 10. / 255., 10. / 255., 1.]
            );
        });
    }

    #[test]
    fn selection_squared_uses_zyx_while_light_vertex_dot_keeps_xyz() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let delta = [4096., 1., 1.];
            assert_eq!(squared_prepare(delta).unwrap(), 16_777_218.);
            assert_eq!(dot(delta, delta).unwrap(), 16_777_216.);
        });
    }

    #[test]
    fn bgr_energy_rounding_replaces_the_first_actual_owner_at_the_boundary() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            // The actual object sphere center is[.5,.5,0], so every point's
            // selection distance is exactly2. All eight energies are8388608.
            let mut lights = Vec::new();
            for _ in 0..8 {
                lights.push(light(
                    &mut arena,
                    scene,
                    sector,
                    1,
                    [16_777_216., 0., 0.],
                    [0.5, 0.5, 2.],
                ));
            }
            let ninth = light(
                &mut arena,
                scene,
                sector,
                1,
                [16_777_216., 1., 1.],
                [0.5, 0.5, 2.],
            );
            let selected = prepare(&mut arena, frame, sector).unwrap();
            // B+G+R produces16777218/2=8388609. R+G+B would round
            // both small additions away and wrongly retain the first owner.
            assert_eq!(selected.lights[0].key, ninth);
            assert_eq!(selected.lights[0].energy, 8_388_609.);
            assert!(!selected.lights.iter().any(|point| point.key == lights[0]));
            assert_eq!(selected.lights[1].key, lights[1]);
        });
    }

    #[test]
    fn bgr_cancellation_changes_selection_with_normal_finite_operands() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            for _ in 0..8 {
                light(&mut arena, scene, sector, 1, [0., 0., 3.5], [0.5, 0.5, 2.]);
            }
            let ninth = light(
                &mut arena,
                scene,
                sector,
                1,
                [33_554_432., -33_554_432., 3.],
                [0.5, 0.5, 2.],
            );
            let selected = prepare(&mut arena, frame, sector).unwrap();
            // Source B+G rounds to-33554428, then +R gives4: energy2.
            // R+G+B would instead give3: energy1.5, below the existing1.75.
            assert_eq!(selected.lights[0].key, ninth);
            assert_eq!(selected.lights[0].energy, 2.);
        });
    }
}
