//! Source-backed null-first-material subset of PrepareLights (LS3DF RVA65a70)
//! and its inline LightVertex loop, pinned mafia-re29b65b0 T_l.cpp. The current
//! sector and mesh/LOD/light records come from the same canonical arena.
//! No native runtime equivalence or independent function credit is claimed.
use crate::frame_transform::{FrameKey, FrameKind};
use crate::i3d_object::{ObjectCleanupPhase, ObjectVisualType};
use crate::model_loading::ModelArena;
use crate::native_matrix::{checked_f32_add, checked_f32_div, checked_f32_mul, checked_f32_sub};
use crate::scene_ownership::SectorKey;

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
struct Point {
    position: [f32; 3],
    color: [f32; 3],
    near: f32,
    far: f32,
    far_squared: f32,
    reciprocal_range: f32,
    energy: f32,
    #[cfg(test)]
    key: FrameKey,
}
struct Prepared {
    ambient: [f32; 3],
    points: Vec<Point>,
}

fn prepare(arena: &mut ModelArena, frame: FrameKey, sector: SectorKey) -> Out<Prepared> {
    let bounds = arena
        .visual_world_bounds(frame)
        .map_err(|e| format!("null material lighting bounds: {e:?}"))?;
    let world = arena
        .world_matrix(frame)
        .map_err(|e| format!("null material lighting matrix: {e:?}"))?;
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
        points: Vec::new(),
    };
    out.points
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
                let slot = if out.points.len() < 8 {
                    Some(out.points.len())
                } else {
                    out.points.iter().position(|point| point.energy < energy)
                };
                let Some(slot) = slot else { continue };
                let near = mul(scale, light.near_scaled)?;
                let far = mul(scale, light.far_scaled)?;
                let delta = sub(far, near)?;
                let reciprocal_range = if delta > 0. { div(1., delta)? } else { delta };
                let position = inverse
                    .try_transform_point(light.position)
                    .map_err(|_| numeric())?;
                let point = Point {
                    position,
                    color: [
                        mul(light.scaled_color[0], 1.)?,
                        mul(light.scaled_color[1], 1.)?,
                        mul(light.scaled_color[2], 1.)?,
                    ],
                    near,
                    far,
                    far_squared: mul(far, far)?,
                    reciprocal_range,
                    energy,
                    #[cfg(test)]
                    key,
                };
                if slot == out.points.len() {
                    out.points.push(point);
                } else {
                    out.points[slot] = point;
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
    for light in &prepared.points {
        let delta = difference(light.position, position)?;
        let cosine = dot(delta, normal)?;
        if cosine.to_bits() & 0x8000_0000 != 0 {
            continue;
        }
        let squared = dot(delta, delta)?;
        if squared > light.far_squared {
            continue;
        }
        let distance = root(squared)?;
        let mut contribution = cosine;
        if distance > light.near {
            contribution = mul(
                contribution,
                mul(sub(light.far, distance)?, light.reciprocal_range)?,
            )?;
        }
        contribution = div(contribution, distance)?;
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
/// actual selected local FVF0x112 positions/normals, not transformed normals.
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
    if arena.state(frame).map_err(|e| format!("{e:?}"))?.kind != FrameKind::Mesh {
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
    if object.visual_type() != ObjectVisualType::Plain || object.phase != ObjectCleanupPhase::Alive
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_loading::ModelKey;
    use crate::scene_ownership::SceneKey;
    use std::sync::Arc;
    static FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    const POSITIONS: [[f32; 3]; 3] = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
    const NORMALS: [[f32; 3]; 3] = [[0., 0., 1.]; 3];
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
            assert_eq!(selected.points.len(), 8);
            assert_eq!(selected.points[0].key, last);
            assert_eq!(selected.points[1].key, weakest);
            assert!(!selected.points.iter().any(|p| p.key == first));
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
    fn unsupported_contributor_is_explicit_but_filtered_modes_do_not_block() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, scene, sector, _model, frame) = world();
            let key = light(&mut arena, scene, sector, 3, [1.; 3], [0.; 3]);
            let result = null_material_vertex_lighting(
                &mut arena,
                frame,
                sector,
                &POSITIONS,
                &NORMALS,
                context(),
            );
            assert!(result
                .unwrap_err()
                .contains("contributing light type 3 unavailable"));
            arena.graph.set_light_mode(key, 2).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
            arena.graph.set_light_mode(key, 1).unwrap();
            arena.set_frame_on(key, 0).unwrap();
            assert_eq!(colors(&mut arena, frame, sector)[0], [0., 0., 0., 1.]);
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
                selected.points.iter().map(|p| p.key).collect::<Vec<_>>(),
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
            assert_eq!(selected.points[0].key, ninth);
            assert_eq!(selected.points[0].energy, 8_388_609.);
            assert!(!selected.points.iter().any(|point| point.key == lights[0]));
            assert_eq!(selected.points[1].key, lights[1]);
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
            assert_eq!(selected.points[0].key, ninth);
            assert_eq!(selected.points[0].energy, 2.);
        });
    }
}
