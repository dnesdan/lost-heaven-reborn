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

/// Original GetLightness(scene,position,NULL,0,NULL), spilled to binary32 by
/// C_Vehicle::Update. Reverse light order, luminance association and the two
/// separate sums follow the supplied PC instructions.
pub fn scene_lightness(arena: &ModelArena, scene: SceneKey, position: [f32; 3]) -> Out<f32> {
    let Some(sector) = arena.scene_sector_at_position(scene, position)? else {
        return Ok(0.);
    };
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
    fn empty_and_disabled_registered_lights_retain_original_zero_result() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(scene_lightness(&arena, scene, [0.; 3]).unwrap(), 0.);
        let light = add_light(&mut arena, scene, 4, [1.; 3]);
        arena.set_frame_on(light, 0).unwrap();
        assert_eq!(scene_lightness(&arena, scene, [0.; 3]).unwrap(), 0.);
        arena.set_frame_on(light, 1).unwrap();
        arena.graph.set_light_mode(light, 0).unwrap();
        assert_eq!(scene_lightness(&arena, scene, [0.; 3]).unwrap(), 0.);
    }
    #[test]
    fn ambient_and_directional_lights_use_real_registered_colors_and_clamp() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        add_light(&mut arena, scene, 4, [0., 1., 0.]);
        add_light(&mut arena, scene, 3, [1., 0., 0.]);
        assert_eq!(scene_lightness(&arena, scene, [0.; 3]).unwrap(), 0.90000004);
        add_light(&mut arena, scene, 4, [1.; 3]);
        assert_eq!(scene_lightness(&arena, scene, [0.; 3]).unwrap(), 1.);
    }
    #[test]
    fn point_light_range_and_spot_cone_reject_unlit_positions() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let light = add_light(&mut arena, scene, 1, [1.; 3]);
        arena.graph.set_light_range(light, 1., 3.).unwrap();
        assert_eq!(scene_lightness(&arena, scene, [0.; 3]).unwrap(), 1.);
        assert_eq!(scene_lightness(&arena, scene, [2., 0., 0.]).unwrap(), 0.5);
        assert_eq!(scene_lightness(&arena, scene, [3., 0., 0.]).unwrap(), 0.);
        arena.graph.set_light_type(light, 2).unwrap();
        assert_eq!(scene_lightness(&arena, scene, [-1., 1., -1.]).unwrap(), 0.);
    }
}
