//! LS3DF I3D_scene::GetLightness, PC RVA 0x49560, for the vehicle's flags=0
//! caller. Sector selection and registered light storage are the actual arena
//! owners. Occlusion queries used by flags=1 are a separate dependency.
use crate::{
    frame_transform::FrameError,
    model_loading::{ModelArena, ModelError},
    scene_ownership::SceneKey,
};
type Out<T> = Result<T, ModelError>;
fn numeric() -> ModelError {
    FrameError::NumericDomain("Scene GetLightness").into()
}
fn number(x: f32) -> Out<f32> {
    crate::vehicle_dynamics_prepare::number::<()>(x).map_err(|_| numeric())
}
fn add(a: f32, b: f32) -> Out<f32> {
    crate::vehicle_dynamics_prepare::add::<()>(a, b).map_err(|_| numeric())
}
fn sub(a: f32, b: f32) -> Out<f32> {
    crate::vehicle_dynamics_prepare::sub::<()>(a, b).map_err(|_| numeric())
}
fn mul(a: f32, b: f32) -> Out<f32> {
    crate::vehicle_dynamics_prepare::mul::<()>(a, b).map_err(|_| numeric())
}
fn div(a: f32, b: f32) -> Out<f32> {
    crate::vehicle_dynamics_prepare::div::<()>(a, b).map_err(|_| numeric())
}
fn root(x: f32) -> Out<f32> {
    let value = number(x)?.sqrt();
    crate::native_precision::record_sqrt(x, value);
    crate::vehicle_dynamics_prepare::result::<()>(value, f64::from(x).sqrt()).map_err(|_| numeric())
}
fn squared(v: [f32; 3]) -> Out<f32> {
    add(add(mul(v[2], v[2])?, mul(v[1], v[1])?)?, mul(v[0], v[0])?)
}
fn difference(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3]> {
    Ok([sub(a[0], b[0])?, sub(a[1], b[1])?, sub(a[2], b[2])?])
}
fn normalize(mut v: [f32; 3]) -> Out<[f32; 3]> {
    let square = squared(v)?;
    if f64::from(sub(square, 1.)?.abs()) >= 1e-8 {
        if square >= 1e-8_f32 {
            let inverse = div(1., root(square)?)?;
            for x in &mut v {
                *x = mul(*x, inverse)?;
            }
        } else {
            let axis = if v[0] != 0. {
                0
            } else if v[2] != 0. {
                2
            } else {
                1
            };
            v[axis] = if v[axis] < 0. { -1. } else { 1. };
        }
    }
    Ok(v)
}
fn length(v: [f32; 3]) -> Out<f32> {
    let square = squared(v)?;
    if (square.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if f64::from(sub(square, 1.)?.abs()) < 1e-8 {
        Ok(1.)
    } else {
        root(square)
    }
}

/// I3D_LIGHTNESS_TESTCOL 0x1 (mafia-re ls3df/I3D/I3D.h:1325-1326 at pin 113b5b6):
/// GetLightness only counts what reaches the point unoccluded.
pub const LIGHTNESS_TESTCOL: u32 = 0x1;
/// Working name for the I3D_LIGHTNESS_TESTCOL bit test (mafia-re ls3df/I3D/I3D.h:1326).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is only consumed inline by `I3D_scene::GetLightness` flags.
/// Reports occlusion testing; no state moves.
pub fn lightness_tests_collision(flags: u32) -> bool {
    flags & LIGHTNESS_TESTCOL != 0
}

/// Working name for the `I3D_scene::GetLightness` no-contribution type gate
/// (mafia-re ls3df/I3D/I3d_scene.cpp:344-348 at pin 113b5b6:
/// `I3DLIGHT_NULL/FOG/POINTAMBIENT/POINTFOG` fall into `continue`).
/// Type words come from `I3D_LIGHTTYPE` (mafia-re ls3df/I3D/I3D.h:1878-1886:
/// NULL=0, POINT=1, SPOT=2, DIRECTIONAL=3, AMBIENT=4, FOG=5, POINTAMBIENT=6,
/// POINTFOG=7). The reference records the GetLightness body at PC RVA 0x49560,
/// already occupied in `docs/rewrite-overlay.json`, so no RVA is claimed here.
/// Reports whether the kind contributes nothing; no state moves.
pub fn lightness_type_skipped(kind: u32) -> bool {
    matches!(kind, 0 | 5 | 6 | 7)
}

/// Working name for the `I3D_scene::GetLightness` ambient arm gate (mafia-re
/// ls3df/I3D/I3d_scene.cpp:322-324 at pin 113b5b6: `I3DLIGHT_AMBIENT` adds `f`
/// to the ambient sum and continues). Same occupied-body status as
/// `lightness_type_skipped`: no RVA is claimed here. Reports whether the kind
/// feeds the ambient sum instead of the direct sum; no state moves.
pub fn lightness_type_is_ambient(kind: u32) -> bool {
    kind == 4
}

/// Working name for the `I3D_scene::GetLightness` spot-cone arm gate (mafia-re
/// ls3df/I3D/I3d_scene.cpp:325-333 at pin 113b5b6: `I3DLIGHT_SPOT` normalizes
/// the offset, dots it with the light direction and tests the cosines before
/// falling through to the point arm). Same occupied-body status as
/// `lightness_type_skipped`: no RVA is claimed here. Reports whether the kind
/// runs the cone test; no state moves.
pub fn lightness_type_has_spot_cone(kind: u32) -> bool {
    kind == 2
}

/// Working name for the `I3D_scene::GetLightness` distance-falloff arm gate
/// (mafia-re ls3df/I3D/I3d_scene.cpp:335-341 at pin 113b5b6: the
/// `I3DLIGHT_SPOT` fallthrough and `I3DLIGHT_POINT` arm measure the distance
/// and scale `f` past the near range). Same occupied-body status as
/// `lightness_type_skipped`: no RVA is claimed here. Reports whether the kind
/// runs the range test; no state moves.
pub fn lightness_type_has_distance_falloff(kind: u32) -> bool {
    matches!(kind, 1 | 2)
}

/// Working name for the `I3D_scene::GetLightness` directional arm gate
/// (mafia-re ls3df/I3D/I3d_scene.cpp:342-343 at pin 113b5b6:
/// `I3DLIGHT_DIRECTIONAL` breaks with `f` verbatim: no cone, no range, no
/// accumulation choice). Same occupied-body status as
/// `lightness_type_skipped`: no RVA is claimed here. Reports whether the kind
/// keeps the luminance unchanged; no state moves.
pub fn lightness_type_is_directional(kind: u32) -> bool {
    kind == 3
}

/// Working name for the `I3D_scene::GetLightness` outer-cone rejection
/// (mafia-re ls3df/I3D/I3d_scene.cpp:329-330 at pin 113b5b6:
/// `if (a <= lt->outer_cos) continue`). Same occupied-body status as
/// `lightness_type_skipped`: no RVA is claimed here. Pure `f32 <=`
/// comparison, so NaN reports false on both sides exactly like the native
/// unordered branch; the normalize/dot that produced `cosine` stay
/// caller-owned.
pub fn lightness_spot_outside_cone(cosine: f32, outer_cos: f32) -> bool {
    cosine <= outer_cos
}

/// Working name for the `I3D_scene::GetLightness` inner-cone partial gate
/// (mafia-re ls3df/I3D/I3d_scene.cpp:331-332 at pin 113b5b6:
/// `if (a <= lt->inner_cos) f *= ...`, else `f` stays full). Same
/// occupied-body status and comparison exactness as
/// `lightness_spot_outside_cone`: no RVA is claimed here. Only meaningful for
/// a cosine that passed the outer gate; reports whether the partial cone
/// scale applies; no state moves.
pub fn lightness_spot_needs_cone_scale(cosine: f32, inner_cos: f32) -> bool {
    cosine <= inner_cos
}

/// Working name for the `I3D_scene::GetLightness` far-range rejection
/// (mafia-re ls3df/I3D/I3d_scene.cpp:337-338 at pin 113b5b6:
/// `if (dist >= lt->far_range) continue`). Same occupied-body status and
/// comparison exactness as `lightness_spot_outside_cone`: no RVA is claimed
/// here. The distance measure stays caller-owned; reports whether the light
/// contributes nothing; no state moves.
pub fn lightness_range_beyond_far(distance: f32, far_range: f32) -> bool {
    distance >= far_range
}

/// Working name for the `I3D_scene::GetLightness` near-range full gate
/// (mafia-re ls3df/I3D/I3d_scene.cpp:339-340 at pin 113b5b6:
/// `if (dist > lt->near_range) f *= ...`, else `f` stays full). Same
/// occupied-body status and comparison exactness as
/// `lightness_spot_outside_cone`: no RVA is claimed here. Only meaningful for
/// a distance inside the far range; reports whether the falloff scale
/// applies; no state moves.
pub fn lightness_range_needs_falloff(distance: f32, near_range: f32) -> bool {
    distance > near_range
}

/// Working name for the `I3D_scene::GetLightness` cone partial scale
/// (mafia-re ls3df/I3D/I3d_scene.cpp:332 at pin 113b5b6:
/// `f *= (a - lt->outer_cos) * lt->cone_scale`). Same occupied-body status as
/// `lightness_type_skipped`: no RVA is claimed here. The subtraction runs
/// before the scaling and the product scales `f` last, mirroring the native
/// association through the module PC24 wrappers; the gate checks stay with
/// the caller.
pub fn lightness_spot_cone_scaled(
    f: f32,
    cosine: f32,
    outer_cos: f32,
    cone_scale: f32,
) -> Out<f32> {
    mul(f, mul(sub(cosine, outer_cos)?, cone_scale)?)
}

/// Working name for the `I3D_scene::GetLightness` range falloff scale
/// (mafia-re ls3df/I3D/I3d_scene.cpp:340 at pin 113b5b6:
/// `f *= (lt->far_range - dist) / (lt->far_range - lt->near_range)`). Same
/// occupied-body status as `lightness_spot_cone_scaled`: no RVA is claimed
/// here. Both differences run before the division and the quotient scales `f`
/// last, mirroring the native association; the gate checks stay with the
/// caller.
pub fn lightness_range_attenuated(
    f: f32,
    far_range: f32,
    distance: f32,
    near_range: f32,
) -> Out<f32> {
    mul(f, div(sub(far_range, distance)?, sub(far_range, near_range)?)?)
}

/// Working name for the `I3D_scene::GetLightness` final sum (mafia-re
/// ls3df/I3D/I3d_scene.cpp:363 at pin 113b5b6:
/// `float ret = lightness + ambient`). Same occupied-body status as
/// `lightness_spot_cone_scaled`: no RVA is claimed here. Single PC24 addition
/// in native operand order; the clamp stays with the caller.
pub fn lightness_combine_direct_ambient(direct: f32, ambient: f32) -> Out<f32> {
    add(direct, ambient)
}

/// Working name for the `I3D_scene::GetLightness` result clamp (mafia-re
/// ls3df/I3D/I3d_scene.cpp:364-366 at pin 113b5b6: below zero returns zero,
/// below one returns the sum, else one). Same occupied-body status as
/// `lightness_type_skipped`: no RVA is claimed here. Pure comparisons, so NaN
/// fails both and returns 1.0 exactly like the native branches;
/// no state moves.
pub fn lightness_clamp_result(ret: f32) -> f32 {
    if ret < 0.0 {
        0.0
    } else if ret < 1.0 {
        ret
    } else {
        1.0
    }
}

/// Original GetLightness(scene,position,NULL,0,NULL), spilled to binary32 by
/// C_Vehicle::Update. Reverse light order, luminance association and the two
/// separate sums follow the supplied PC instructions.
pub fn scene_lightness(arena: &mut ModelArena, scene: SceneKey, position: [f32; 3]) -> Out<f32> {
    let sector = arena.scene_get_sector(scene, position, None)?;
    let mut direct = 0.;
    let mut ambient = 0.;
    for &key in arena.sector(sector)?.lights.iter().rev() {
        if arena.graph.state(key)?.flags & 1 == 0 {
            continue;
        }
        let light = arena.light(key)?;
        if light.mode & 0x40 == 0 {
            continue;
        }
        let color = light.scaled_color;
        let mut luminance = add(
            add(mul(color[2], 0.1)?, mul(color[1], 0.6)?)?,
            mul(color[0], 0.3)?,
        )?;
        match light.light_type {
            0 | 5 | 6 | 7 => continue,
            4 => {
                ambient = add(luminance, ambient)?;
                continue;
            }
            2 => {
                let direction = normalize(difference(position, light.position)?)?;
                let cosine = add(
                    add(
                        mul(direction[2], light.direction[2])?,
                        mul(direction[1], light.direction[1])?,
                    )?,
                    mul(direction[0], light.direction[0])?,
                )?;
                if cosine <= number(light.outer_cos)? {
                    continue;
                }
                if cosine <= number(light.inner_cos)? {
                    luminance = mul(
                        mul(sub(cosine, light.outer_cos)?, light.cone_reciprocal)?,
                        luminance,
                    )?;
                }
            }
            _ => {}
        }
        if matches!(light.light_type, 1 | 2) {
            let distance = length(difference(position, light.position)?)?;
            let [near, far] = light.range;
            if distance >= number(far)? {
                continue;
            }
            if distance > number(near)? {
                luminance = mul(div(sub(far, distance)?, sub(far, near)?)?, luminance)?;
            }
        }
        direct = add(luminance, direct)?;
    }
    let value = add(direct, ambient)?;
    Ok(value.clamp(0., 1.))
}

/// Shared synthetic loaded hull for the two real spatial consumer regressions.
#[cfg(test)]
pub(crate) fn loaded_sector_fixture(
    arena: &mut ModelArena,
    scene: SceneKey,
    translation: [f32; 3],
) -> crate::scene_ownership::SectorKey {
    let sector = arena.create_sector_frame().unwrap();
    let record = lhr_assets::SectorRecord {
        flags: 0x801,
        group: u32::MAX,
        vertices: vec![
            [-1., -1., -1.],
            [1., -1., -1.],
            [1., 1., -1.],
            [-1., 1., -1.],
            [-1., -1., 1.],
            [1., -1., 1.],
            [1., 1., 1.],
            [-1., 1., 1.],
        ],
        faces: vec![
            // Sector CheckPoint accepts the positive half-space: hull faces
            // therefore face inward, unlike an occluder's outward hull.
            [0, 1, 2],
            [0, 2, 3],
            [4, 6, 5],
            [4, 7, 6],
            [0, 7, 4],
            [0, 3, 7],
            [1, 6, 2],
            [1, 5, 6],
            [0, 5, 1],
            [0, 4, 5],
            [3, 6, 7],
            [3, 2, 6],
        ],
        bounds: [[-1.; 3], [1.; 3]],
        portals: vec![],
    };
    assert_eq!(arena.load_sector_record(sector, &record).unwrap(), None);
    arena
        .graph
        .assign_position(sector.frame(), translation)
        .unwrap();
    arena.sector_add_to_scene_ring(sector, Some(scene)).unwrap();
    arena.sector_make_ready(sector).unwrap();
    assert_eq!(arena.sector_planes(sector).unwrap().len(), 6);
    assert!(arena.sector_check_point(sector, translation).unwrap());
    assert!(!arena
        .sector_check_point(
            sector,
            [translation[0] + 2., translation[1], translation[2]]
        )
        .unwrap());
    sector
}

#[cfg(test)]
mod tests {
    use super::*;
    fn add_light(
        arena: &mut ModelArena,
        scene: SceneKey,
        kind: u32,
        color: [f32; 3],
    ) -> crate::frame_transform::FrameKey {
        let light = arena.create_light().unwrap();
        arena.graph.set_light_type(light, kind).unwrap();
        arena.graph.set_light_mode(light, 0x40).unwrap();
        arena.graph.set_light_diffuse(light, color).unwrap();
        arena.set_frame_on(light, 1).unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        arena.sector_add_light(primary, light).unwrap();
        light
    }
    #[test]
    fn lightness_flag_reports_only_occlusion_bit() {
        // Working name for I3D_LIGHTNESS_TESTCOL (I3D.h:1326): no LS3DF FUNCTION entry.
        assert_eq!(LIGHTNESS_TESTCOL, 0x1);
        assert!(lightness_tests_collision(LIGHTNESS_TESTCOL));
        assert!(lightness_tests_collision(u32::MAX));
        assert!(!lightness_tests_collision(0));
        assert!(!lightness_tests_collision(0x2));
        assert!(!lightness_tests_collision(u32::MAX & !LIGHTNESS_TESTCOL));
    }
    #[test]
    fn lightness_type_gates_cover_all_eight_light_kinds() {
        // Working names for the GetLightness switch arms
        // (I3d_scene.cpp:321-349 at pin 113b5b6): the body lives at occupied
        // PC RVA 0x49560, so no RVA is claimed here.
        for kind in [0u32, 5, 6, 7] {
            assert!(lightness_type_skipped(kind), "kind {kind}");
            assert!(!lightness_type_is_ambient(kind));
            assert!(!lightness_type_has_spot_cone(kind));
            assert!(!lightness_type_has_distance_falloff(kind));
            assert!(!lightness_type_is_directional(kind));
        }
        assert!(lightness_type_is_ambient(4));
        assert!(!lightness_type_skipped(4));
        assert!(lightness_type_has_spot_cone(2));
        assert!(lightness_type_has_distance_falloff(2));
        assert!(lightness_type_has_distance_falloff(1));
        assert!(!lightness_type_has_spot_cone(1));
        assert!(lightness_type_is_directional(3));
        assert!(!lightness_type_skipped(3));
        // Unknown kinds (shadow-only 8, unmapped words) take no arm.
        for kind in [8u32, 9, u32::MAX] {
            assert!(!lightness_type_skipped(kind));
            assert!(!lightness_type_is_ambient(kind));
            assert!(!lightness_type_has_spot_cone(kind));
            assert!(!lightness_type_has_distance_falloff(kind));
            assert!(!lightness_type_is_directional(kind));
        }
    }
    #[test]
    fn lightness_cone_gates_follow_native_unordered_comparisons() {
        // Working names for I3d_scene.cpp:329-332: pure `<=`, so NaN is
        // false on both sides exactly like the native branch.
        assert!(lightness_spot_outside_cone(0.5, 0.5));
        assert!(lightness_spot_outside_cone(0.4, 0.5));
        assert!(!lightness_spot_outside_cone(0.6, 0.5));
        assert!(!lightness_spot_outside_cone(f32::NAN, 0.5));
        assert!(!lightness_spot_outside_cone(0.5, f32::NAN));
        assert!(lightness_spot_needs_cone_scale(0.7, 0.7));
        assert!(lightness_spot_needs_cone_scale(0.6, 0.7));
        assert!(!lightness_spot_needs_cone_scale(0.8, 0.7));
        assert!(!lightness_spot_needs_cone_scale(f32::NAN, 0.7));
    }
    #[test]
    fn lightness_range_gates_follow_native_ordered_comparisons() {
        // Working names for I3d_scene.cpp:337-340: `>=` rejects at the far
        // edge, `>` keeps full strength at the near edge.
        assert!(lightness_range_beyond_far(10.0, 10.0));
        assert!(lightness_range_beyond_far(11.0, 10.0));
        assert!(!lightness_range_beyond_far(9.9, 10.0));
        assert!(!lightness_range_beyond_far(f32::NAN, 10.0));
        assert!(lightness_range_needs_falloff(2.1, 2.0));
        assert!(!lightness_range_needs_falloff(2.0, 2.0));
        assert!(!lightness_range_needs_falloff(1.9, 2.0));
        assert!(!lightness_range_needs_falloff(f32::NAN, 2.0));
    }
    #[test]
    fn lightness_cone_partial_scale_mirrors_native_association() {
        // Working name for I3d_scene.cpp:332 `f *= (a - outer) * scale`:
        // dyadic values keep the PC24 wrappers exact.
        assert_eq!(
            lightness_spot_cone_scaled(2.0, 0.75, 0.5, 4.0).unwrap(),
            2.0
        );
        assert_eq!(
            lightness_spot_cone_scaled(8.0, 1.0, 0.5, 0.5).unwrap(),
            2.0
        );
        assert_eq!(
            lightness_spot_cone_scaled(3.0, 0.5, 0.5, 7.0).unwrap(),
            0.0
        );
    }
    #[test]
    fn lightness_range_falloff_mirrors_native_quotient_order() {
        // Working name for I3d_scene.cpp:340
        // `f *= (far - dist) / (far - near)`: dyadic values stay exact.
        assert_eq!(
            lightness_range_attenuated(8.0, 10.0, 6.0, 2.0).unwrap(),
            4.0
        );
        assert_eq!(
            lightness_range_attenuated(5.0, 10.0, 2.0, 2.0).unwrap(),
            5.0
        );
        assert_eq!(
            lightness_range_attenuated(5.0, 10.0, 10.0, 2.0).unwrap(),
            0.0
        );
    }
    #[test]
    fn lightness_sum_and_clamp_match_native_tail() {
        // Working names for I3d_scene.cpp:363-366: the sum runs before the
        // two ordered clamp branches; NaN fails both and returns 1.0 natively.
        assert_eq!(
            lightness_combine_direct_ambient(0.25, 0.5).unwrap(),
            0.75
        );
        assert_eq!(lightness_clamp_result(-0.5), 0.0);
        assert_eq!(lightness_clamp_result(0.0), 0.0);
        assert_eq!(lightness_clamp_result(0.3), 0.3);
        assert_eq!(lightness_clamp_result(1.0), 1.0);
        assert_eq!(lightness_clamp_result(1.5), 1.0);
        // NaN fails both native comparisons, so it returns 1.0 like the original.
        assert_eq!(lightness_clamp_result(f32::NAN), 1.0);
    }
    #[test]
    fn empty_and_disabled_registered_lights_retain_original_zero_result() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(scene_lightness(&mut arena, scene, [0.; 3]).unwrap(), 0.);
        let light = add_light(&mut arena, scene, 4, [1.; 3]);
        arena.set_frame_on(light, 0).unwrap();
        assert_eq!(scene_lightness(&mut arena, scene, [0.; 3]).unwrap(), 0.);
        arena.set_frame_on(light, 1).unwrap();
        arena.graph.set_light_mode(light, 0).unwrap();
        assert_eq!(scene_lightness(&mut arena, scene, [0.; 3]).unwrap(), 0.);
    }
    #[test]
    fn ambient_and_directional_lights_use_real_registered_colors_and_clamp() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        add_light(&mut arena, scene, 4, [0., 1., 0.]);
        add_light(&mut arena, scene, 3, [1., 0., 0.]);
        assert_eq!(
            scene_lightness(&mut arena, scene, [0.; 3]).unwrap(),
            0.90000004
        );
        add_light(&mut arena, scene, 4, [1.; 3]);
        assert_eq!(scene_lightness(&mut arena, scene, [0.; 3]).unwrap(), 1.);
    }
    #[test]
    fn point_light_range_and_spot_cone_reject_unlit_positions() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let light = add_light(&mut arena, scene, 1, [1.; 3]);
        arena.graph.set_light_range(light, 1., 3.).unwrap();
        assert_eq!(scene_lightness(&mut arena, scene, [0.; 3]).unwrap(), 1.);
        assert_eq!(
            scene_lightness(&mut arena, scene, [2., 0., 0.]).unwrap(),
            0.5
        );
        assert_eq!(
            scene_lightness(&mut arena, scene, [3., 0., 0.]).unwrap(),
            0.
        );
        arena.graph.set_light_type(light, 2).unwrap();
        assert_eq!(
            scene_lightness(&mut arena, scene, [-1., 1., -1.]).unwrap(),
            0.
        );
    }
    #[test]
    fn loaded_hull_selection_samples_its_registered_lights_and_outside_uses_primary() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let a = loaded_sector_fixture(&mut arena, scene, [0.; 3]);
            let b = loaded_sector_fixture(&mut arena, scene, [10., 0., 0.]);
            let primary = arena.scene(scene).unwrap().sectors[0];
            for (target, color) in [
                (a, [1., 0., 0.]),
                (b, [0., 1., 0.]),
                (primary, [0., 0., 1.]),
            ] {
                let light = add_light(&mut arena, scene, 4, color);
                if target != primary {
                    arena.sector_delete_light(primary, light).unwrap();
                    arena.sector_add_light(target, light).unwrap();
                }
            }
            assert_eq!(scene_lightness(&mut arena, scene, [0.; 3]).unwrap(), 0.3);
            assert_eq!(
                scene_lightness(&mut arena, scene, [10., 0., 0.]).unwrap(),
                0.6
            );
            assert_eq!(
                scene_lightness(&mut arena, scene, [30., 0., 0.]).unwrap(),
                0.1
            );
        });
    }
}
