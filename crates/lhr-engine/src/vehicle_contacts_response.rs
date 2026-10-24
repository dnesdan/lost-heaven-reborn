//! Frame/suspension/impulse continuation of DoWheelsCollision, PC 0x4e7c0d.
use super::*;
use crate::native_transform::SQuat;

fn normalize<E>(mut v: [f32; 3]) -> Result<[f32; 3], E> {
    let length2 = square_xyz(v)?;
    if (sub(length2, 1.)? as f64).abs() < 1e-8 {
        return Ok(v);
    }
    if length2 >= 1e-8_f32 {
        let length = arithmetic(length2.sqrt(), (length2 as f64).sqrt())?;
        crate::native_precision::record_sqrt(length2, length);
        return scale(v, div(1., length)?);
    }
    let axis = if v[0] != 0. {
        0
    } else if v[2] != 0. {
        2
    } else {
        1
    };
    v[axis] = if v[axis] < 0. { -1. } else { 1. };
    Ok(v)
}
fn length<E>(v: [f32; 3]) -> Result<f32, E> {
    let length2 = square_xyz(v)?;
    if (length2.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if (sub(length2, 1.)? as f64).abs() < 1e-8 {
        Ok(1.)
    } else {
        let value = arithmetic(length2.sqrt(), (length2 as f64).sqrt())?;
        crate::native_precision::record_sqrt(length2, value);
        Ok(value)
    }
}
fn cosine_product<E>(angle: f32, factor: f32) -> Result<f32, E> {
    crate::native_transform::try_cosine_product(angle, factor)
        .map_err(VehicleContactsUnavailable::Library)
}
fn rotate_direction<E>(axis: [f32; 3], direction: [f32; 3], angle: f32) -> Result<[f32; 3], E> {
    let quat =
        SQuat::try_make_axis_angle(axis, angle).map_err(VehicleContactsUnavailable::Library)?;
    let mut matrix = SMatrix::identity();
    matrix
        .try_set_rot3(quat)
        .map_err(|_| VehicleContactsUnavailable::NumericDomain)?;
    matrix
        .try_transform_point(direction)
        .map_err(|_| VehicleContactsUnavailable::NumericDomain)
}
fn inertia<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    v: u32,
    vector: [f32; 3],
) -> Result<[f32; 3], B::Error> {
    let local = transform(b, v.wrapping_add(0xda8), vector)?;
    let diagonal = rv(b, v.wrapping_add(0x438))?;
    let local = [
        mul(local[0], diagonal[0])?,
        mul(local[1], diagonal[1])?,
        mul(local[2], diagonal[2])?,
    ];
    transform(b, v.wrapping_add(0xd68), local)
}
fn effect<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    v: u32,
    e: VehicleContactEffect<'_>,
) -> Result<(), B::Error> {
    b.contact_effect(v, e)
        .map_err(VehicleContactsUnavailable::Effect)
}
fn new_bottom_impact<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    v: u32,
    w: u32,
    linear_delta: &mut [f32; 3],
    angular_delta: &mut [f32; 3],
) -> Result<bool, B::Error> {
    let r = vs(rv(b, w.wrapping_add(0x28))?, rv(b, v.wrapping_add(0x398))?)?;
    let velocity = va(
        rv(b, v.wrapping_add(0x1f90))?,
        cross(r, rv(b, v.wrapping_add(0x420))?)?,
    )?;
    let normal = rv(b, w.wrapping_add(0x40))?;
    let normal_speed = dot(velocity, normal)?;
    if normal_speed >= 0. {
        return Ok(false);
    }
    let speed = mul(normal_speed, -1.)?.abs();
    let response = inertia(b, v, cross(r, normal)?)?;
    let inverse_mass = div(1., rf(b, v.wrapping_add(0x5f4))?)?;
    let mut denominator = add(dot(cross(response, r)?, normal)?, inverse_mass)?;
    if (denominator.abs() as f64) < 1e-6 {
        denominator = 1e-6_f32;
    }
    let mut impulse = scale(normal, div(speed, denominator)?)?;
    *linear_delta = va(scale(impulse, inverse_mass)?, *linear_delta)?;
    let torque = inertia(b, v, cross(r, impulse)?)?;
    *angular_delta = vs(*angular_delta, torque)?;
    let magnitude = length(impulse)?;
    if rw(b, v.wrapping_add(0xc68))? & 0x8000 != 0 {
        effect(
            b,
            v,
            VehicleContactEffect::ImpactParticles {
                point: w.wrapping_add(0x1c),
                down: v.wrapping_add(0xc54),
                magnitude,
            },
        )?;
    }
    if rw(b, v.wrapping_add(0xc68))? & 0x4000 != 0 {
        effect(b, v, VehicleContactEffect::ImpactSound { magnitude })?;
        let callback = rw(b, v.wrapping_add(0x2f4))?;
        if callback != 0 {
            let context = rw(b, v.wrapping_add(0xc74))?;
            effect(
                b,
                v,
                VehicleContactEffect::ImpactCallback {
                    callback,
                    context,
                    impulse: &mut impulse,
                    point: w.wrapping_add(0x1c),
                    velocity,
                },
            )?;
        }
    }
    if rw(b, v.wrapping_add(0xc68))? & 0x20 != 0
        && b.read_byte(v.wrapping_add(0x432))
            .map_err(VehicleContactsUnavailable::Effect)?
            != 0
    {
        // PC 4e82e2..8307 reloads the same local impulse that both callback
        // arguments alias. The earlier velocity deltas remain accumulated
        // from its pre-callback value. Mass is also reloaded from live storage.
        let magnitude = length(impulse)?;
        let inverse_mass = div(1., rf(b, v.wrapping_add(0x5f4))?)?;
        if mul(inverse_mass, magnitude)? > 1.4_f32 {
            effect(b, v, VehicleContactEffect::ImpactDamage)?;
        }
    }
    Ok(true)
}
fn contact_velocity<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    v: u32,
    w: u32,
) -> Result<[f32; 3], B::Error> {
    let r = vs(rv(b, w.wrapping_add(0x1c))?, rv(b, v.wrapping_add(0x398))?)?;
    va(
        rv(b, v.wrapping_add(0x1f90))?,
        cross(r, rv(b, v.wrapping_add(0x420))?)?,
    )
}
fn wheel_directions<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    v: u32,
    w: u32,
) -> Result<(), B::Error> {
    let flags = rw(b, w.wrapping_add(0x120))?;
    let velocity = contact_velocity(b, v, w)?;
    let direction;
    if flags & 8 != 0 {
        let primitive = rw(b, w.wrapping_add(0x124))?;
        if primitive != 0
            && b.contact_primitive_properties(primitive)
                .map_err(VehicleContactsUnavailable::Effect)?
                & 0x10000
                != 0
        {
            b.write_byte(v.wrapping_add(0x28), 1)
                .map_err(VehicleContactsUnavailable::Effect)?;
        }
        let normal = rv(b, w.wrapping_add(0x40))?;
        let lateral = cross(velocity, normal)?;
        let tangent = cross(normal, lateral)?;
        wv(b, w.wrapping_add(0x4c), tangent)?;
        if rw(b, w.wrapping_add(0x120))? & 1 == 0 {
            direction = cross(rv(b, v.wrapping_add(0xc48))?, normal)?;
        } else {
            let frame = rw(b, w.wrapping_add(8))?;
            if b.read_byte(frame.wrapping_add(0xac))
                .map_err(VehicleContactsUnavailable::Effect)?
                & 0x20
                == 0
            {
                b.contact_update_frame_world(frame)
                    .map_err(VehicleContactsUnavailable::Effect)?;
            }
            let frame_x = normalize(rv(b, frame.wrapping_add(0x30))?)?;
            let down = rv(b, v.wrapping_add(0xc54))?;
            direction = cross(cross(down, frame_x)?, normal)?;
        }
    } else {
        let down = rv(b, v.wrapping_add(0xc54))?;
        let tangent = cross(down, cross(velocity, down)?)?;
        wv(b, w.wrapping_add(0x4c), tangent)?;
        if rw(b, w.wrapping_add(0x120))? & 1 != 0 {
            let frame = rw(b, w.wrapping_add(8))?;
            direction = b
                .contact_frame_direction(frame)
                .map_err(VehicleContactsUnavailable::Effect)?;
        } else {
            direction = rv(b, v.wrapping_add(0xc3c))?;
        }
    }
    wv(b, w.wrapping_add(0x58), direction)?;
    if rf(b, w.wrapping_add(0x188))? > 0. {
        let oscillation = rf(b, w.wrapping_add(0x114))?;
        let amplitude = rf(b, w.wrapping_add(0x188))?;
        let angle = cosine_product(oscillation, amplitude)?;
        let direction = rv(b, w.wrapping_add(0x58))?;
        let down = rv(b, v.wrapping_add(0xc54))?;
        let rotated = rotate_direction(down, direction, angle)?;
        wv(b, w.wrapping_add(0x58), rotated)?;
    }
    let direction = normalize(rv(b, w.wrapping_add(0x58))?)?;
    wv(b, w.wrapping_add(0x58), direction)?;
    let speed = dot(direction, rv(b, w.wrapping_add(0x4c))?)?;
    let flags = rw(b, w.wrapping_add(0x120))?;
    ww(
        b,
        w.wrapping_add(0x120),
        if speed >= 0. {
            flags | 0x10000
        } else {
            flags & !0x10000
        },
    )?;
    Ok(())
}
pub(super) fn respond<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    v: u32,
    timestep: f32,
    impacts: bool,
) -> Result<[f32; 3], B::Error> {
    let count = rw(b, v.wrapping_add(0x4c4))?;
    let mut output = [0.; 3];
    let mut linear_delta = [0.; 3];
    let mut angular_delta = [0.; 3];
    let mut impacts_count = 0_u32;
    for i in (0..count).rev() {
        let w = wheel_at(b, v, i)?;
        let flags = rw(b, w.wrapping_add(0x120))?;
        if flags & 0x40000000 == 0 {
            let position = rv(b, w.wrapping_add(0x1c))?;
            let local = transform(b, v.wrapping_add(0xd28), position)?;
            let rest_height = rf(b, w.wrapping_add(0x14))?;
            let height = if rest_height < local[1] {
                rest_height
            } else {
                local[1]
            };
            let frame = rw(b, w.wrapping_add(8))?;
            if (b
                .read_byte(frame.wrapping_add(0xac))
                .map_err(VehicleContactsUnavailable::Effect)? as i8)
                >= 0
            {
                let x = rw(b, w.wrapping_add(0x10))?;
                ww(b, frame.wrapping_add(0x80), x)?;
                wf(b, frame.wrapping_add(0x84), height)?;
                let z = rw(b, w.wrapping_add(0x18))?;
                ww(b, frame.wrapping_add(0x88), z)?;
                let frame_flags = rw(b, frame.wrapping_add(0xac))?;
                ww(
                    b,
                    frame.wrapping_add(0xac),
                    (frame_flags & 0xfffffedf) | 0x40000000,
                )?;
            }
            let mut compression = sub(rest_height, height)?;
            let rate = div(sub(rf(b, w.wrapping_add(0x94))?, compression)?, timestep)?;
            if compression > 0. || !impacts {
                let flags = rw(b, w.wrapping_add(0x120))?;
                ww(b, w.wrapping_add(0x120), flags & !0x40)?;
            } else {
                compression = 0.;
                let flags = rw(b, w.wrapping_add(0x120))?;
                if flags & 0x40 == 0 {
                    ww(b, w.wrapping_add(0x120), flags | 0x40)?;
                    if new_bottom_impact(b, v, w, &mut linear_delta, &mut angular_delta)? {
                        impacts_count = impacts_count.wrapping_add(1);
                    }
                }
            }
            let spring = mul(
                sub(rf(b, w.wrapping_add(0x8c))?, compression)?,
                rf(b, w.wrapping_add(0x88))?,
            )?;
            if rate < 0. {
                let damper = mul(rate, rf(b, w.wrapping_add(0x9c))?)?;
                wf(b, w.wrapping_add(0x90), add(damper, spring)?)?;
            } else {
                wf(b, w.wrapping_add(0x90), spring)?;
                if rw(b, w.wrapping_add(0x120))? & 0x40 == 0 {
                    let damper = mul(rate, rf(b, w.wrapping_add(0x98))?)?;
                    wf(b, w.wrapping_add(0x90), add(damper, spring)?)?;
                }
            }
            if rf(b, w.wrapping_add(0x90))? < 0. {
                let flags = rw(b, w.wrapping_add(0x120))?;
                ww(b, w.wrapping_add(0x120), flags & !8)?;
            }
            wf(b, w.wrapping_add(0x94), compression)?;
            wheel_directions(b, v, w)?;
            if rw(b, w.wrapping_add(0x120))? & 8 != 0 {
                output = va(output, rv(b, w.wrapping_add(0x58))?)?;
            }
        }
        let flags = rw(b, w.wrapping_add(0x120))?;
        ww(b, w.wrapping_add(0x120), flags & !0x20000000)?;
    }
    if impacts_count != 0 {
        for (j, x) in linear_delta.into_iter().enumerate() {
            let a = v.wrapping_add(0x1f90 + j as u32 * 4);
            let value = add(rf(b, a)?, x)?;
            wf(b, a, value)?;
        }
        for (j, x) in angular_delta.into_iter().enumerate() {
            let a = v.wrapping_add(0x420 + j as u32 * 4);
            let value = add(rf(b, a)?, x)?;
            wf(b, a, value)?;
        }
    }
    Ok(output)
}
