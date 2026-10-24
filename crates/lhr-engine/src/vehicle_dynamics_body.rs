//! Rigid-body integration and body-collision retry inside original PC Move.
//! The body-collision operation remains an explicit original dependency.
use crate::native_transform::{MakeUnavailable, SMatrix, SQuat};
use crate::vehicle_dynamics_forces::VehicleMoveForceSetup;
use crate::vehicle_dynamics_prepare::{
    add, div, float, mul, number, store, store_float, sub, word, Result,
    VehicleMovePrepareUnavailable,
};
use crate::vehicle_init::VehicleInitBackend;

/// Inputs from the original wheel force/torque phase, before RVA e41dc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMoveBodyForces {
    pub linear_force: [f32; 3],
    pub torque: [f32; 3],
    pub contact_normal_sum: [f32; 3],
    pub any_contact: bool,
    pub drive_force_sum: f32,
}

/// Descriptive fields for the seven physical arguments at PC RVA e8af0.
/// Their layouts and initial values come from the Move caller. Field names
/// do not claim recovered source-level names for the auxiliary outputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleBodyCollisionCall {
    pub collision_records: u32,
    pub correction_first: [f32; 3],
    pub correction_second: [f32; 3],
    pub auxiliary_scalar: f32,
    pub auxiliary_vector: [f32; 3],
    pub previous_up: [f32; 3],
    pub milliseconds: u32,
}
pub trait VehicleRigidBodyBackend: VehicleInitBackend {
    fn body_collision(
        &mut self,
        vehicle: u32,
        call: &mut VehicleBodyCollisionCall,
    ) -> std::result::Result<bool, Self::Error>;
}

fn vector<B: VehicleInitBackend>(b: &mut B, a: u32) -> Result<[f32; 3], B::Error> {
    Ok([
        float(b, a)?,
        float(b, a.wrapping_add(4))?,
        float(b, a.wrapping_add(8))?,
    ])
}
fn put_vector<B: VehicleInitBackend>(b: &mut B, a: u32, v: [f32; 3]) -> Result<(), B::Error> {
    for (i, x) in v.into_iter().enumerate() {
        store_float(b, a.wrapping_add(i as u32 * 4), x)?;
    }
    Ok(())
}
fn matrix<B: VehicleInitBackend>(b: &mut B, a: u32) -> Result<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    for (i, x) in elements.iter_mut().enumerate() {
        *x = float(b, a.wrapping_add(i as u32 * 4))?;
    }
    Ok(SMatrix { elements })
}
fn copy<B: VehicleInitBackend>(b: &mut B, to: u32, from: u32, n: u32) -> Result<(), B::Error> {
    for i in 0..n {
        let v = word(b, from.wrapping_add(i * 4))?;
        store(b, to.wrapping_add(i * 4), v)?;
    }
    Ok(())
}
fn transform<E>(m: SMatrix, v: [f32; 3]) -> Result<[f32; 3], E> {
    m.try_transform_point(v)
        .map_err(|_| VehicleMovePrepareUnavailable::NumericDomain)
}
// Original in-place S_vector::operator*= (LS3DF RVA 2e920) associates
// products differently from the returned-value operation at RVA 2e980.
pub(crate) fn transform_assign<E>(m: SMatrix, v: [f32; 3]) -> Result<[f32; 3], E> {
    let e = m.elements;
    Ok([
        add(
            add(add(mul(e[8], v[2])?, mul(e[4], v[1])?)?, mul(e[0], v[0])?)?,
            e[12],
        )?,
        add(
            add(add(mul(e[9], v[2])?, mul(e[1], v[0])?)?, mul(e[5], v[1])?)?,
            e[13],
        )?,
        add(
            add(add(mul(e[10], v[2])?, mul(e[2], v[0])?)?, mul(e[6], v[1])?)?,
            e[14],
        )?,
    ])
}
fn squared<E>(v: [f32; 3]) -> Result<f32, E> {
    add(add(mul(v[2], v[2])?, mul(v[1], v[1])?)?, mul(v[0], v[0])?)
}
fn squared_xyz<E>(v: [f32; 3]) -> Result<f32, E> {
    add(add(mul(v[0], v[0])?, mul(v[1], v[1])?)?, mul(v[2], v[2])?)
}
fn sqrt<E>(v: f32) -> Result<f32, E> {
    number(v)?;
    let value = number(v.sqrt())?;
    crate::native_precision::record_sqrt(v, value);
    Ok(value)
}
fn length<E>(v: [f32; 3]) -> Result<f32, E> {
    let s = squared(v)?;
    if (s.to_bits() as i32) < 0x322bcc77 {
        Ok(0.0)
    } else if f64::from(sub(s, 1.0)?.abs()) < 1e-8 {
        Ok(1.0)
    } else {
        sqrt(s)
    }
}
fn length_xyz<E>(v: [f32; 3]) -> Result<f32, E> {
    let s = squared_xyz(v)?;
    if (s.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if f64::from(sub(s, 1.)?.abs()) < 1e-8 {
        Ok(1.)
    } else {
        sqrt(s)
    }
}
fn normalize_velocity<B: VehicleInitBackend>(b: &mut B, a: u32) -> Result<(), B::Error> {
    let mut v = vector(b, a)?;
    let s = squared_xyz(v)?;
    if f64::from(sub(s, 1.)?.abs()) >= 1e-8 {
        if s >= f32::from_bits(0x322bcc77) {
            let r = div(1., sqrt(s)?)?;
            for x in &mut v {
                *x = mul(r, *x)?;
            }
        } else {
            let i = if v[0] != 0. {
                0
            } else if v[2] != 0. {
                2
            } else {
                1
            };
            v[i] = if v[i] < 0. { -1. } else { 1. };
        }
    }
    put_vector(b, a, v)
}
fn normalized<E>(mut v: [f32; 3]) -> Result<[f32; 3], E> {
    let s = squared(v)?;
    if f64::from(sub(s, 1.0)?.abs()) >= 1e-8 {
        if s >= f32::from_bits(0x322bcc77) {
            let r = div(1.0, sqrt(s)?)?;
            for x in &mut v {
                *x = mul(r, *x)?;
            }
        } else {
            let i = if v[0] != 0.0 {
                0
            } else if v[2] != 0.0 {
                2
            } else {
                1
            };
            v[i] = if v[i] < 0.0 { -1.0 } else { 1.0 };
        }
    }
    Ok(v)
}
fn normalize_memory<B: VehicleInitBackend>(b: &mut B, a: u32) -> Result<(), B::Error> {
    normalize_velocity(b, a)
}
fn neg_cross<E>(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], E> {
    Ok([
        sub(mul(a[2], b[1])?, mul(a[1], b[2])?)?,
        sub(mul(a[0], b[2])?, mul(a[2], b[0])?)?,
        sub(mul(a[1], b[0])?, mul(a[0], b[1])?)?,
    ])
}

#[derive(Clone, Copy)]
struct Pose {
    position: [f32; 3],
    direction: [f32; 3],
    up: [f32; 3],
    side: [f32; 3],
}
fn pose<B: VehicleInitBackend>(b: &mut B, v: u32) -> Result<Pose, B::Error> {
    Ok(Pose {
        position: vector(b, v.wrapping_add(0x320))?,
        direction: vector(b, v.wrapping_add(0xc3c))?,
        up: vector(b, v.wrapping_add(0xc54))?,
        side: vector(b, v.wrapping_add(0xc48))?,
    })
}
fn restore<B: VehicleInitBackend>(b: &mut B, v: u32, p: Pose) -> Result<(), B::Error> {
    for (a, values) in [
        (0x320, p.position),
        (0xc3c, p.direction),
        (0xc54, p.up),
        (0xc48, p.side),
    ] {
        put_vector(b, v.wrapping_add(a), values)?;
    }
    Ok(())
}
fn publish<B: VehicleInitBackend>(b: &mut B, v: u32) -> Result<(), B::Error> {
    let frame = word(b, v.wrapping_add(0x34c))?;
    if b.read_byte(frame.wrapping_add(0xac))
        .map_err(VehicleMovePrepareUnavailable::Effect)?
        & 0x80
        == 0
    {
        let p = vector(b, v.wrapping_add(0x320))?;
        put_vector(b, frame.wrapping_add(0x80), p)?;
        let flags = word(b, frame.wrapping_add(0xac))?;
        store(b, frame.wrapping_add(0xac), flags & 0xfffffedf | 0x40000000)?;
    }
    let frame = word(b, v.wrapping_add(0x34c))?;
    let direction = vector(b, v.wrapping_add(0xc3c))?;
    let up = vector(b, v.wrapping_add(0xc54))?;
    let mut transform = matrix(b, frame.wrapping_add(0x50))?;
    let outcome = transform.try_set_dir3(direction, up);
    for index in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        store_float(
            b,
            frame.wrapping_add(0x50 + index * 4),
            transform.elements[index as usize],
        )?;
    }
    outcome.map_err(|_| VehicleMovePrepareUnavailable::NumericDomain)?;
    let flags = word(b, frame.wrapping_add(0xac))?;
    store(b, frame.wrapping_add(0xac), flags & 0xfffffed3 | 0x40000010)
}
fn quat<E>(axis: [f32; 3], angle: f32) -> Result<SQuat, E> {
    SQuat::try_make_axis_angle(axis, angle)
        .map_err(|_: MakeUnavailable| VehicleMovePrepareUnavailable::NumericDomain)
}
fn compose<E>(first: SQuat, second: SQuat) -> Result<SQuat, E> {
    let [w, x, y, z] = first.wxyz;
    let [rw, rx, ry, rz] = second.wxyz;
    let dot = add(add(mul(x, rx)?, mul(y, ry)?)?, mul(z, rz)?)?;
    Ok(SQuat {
        wxyz: [
            sub(mul(rw, w)?, dot)?,
            add(
                add(mul(x, rw)?, mul(rx, w)?)?,
                sub(mul(y, rz)?, mul(z, ry)?)?,
            )?,
            add(
                add(mul(y, rw)?, mul(ry, w)?)?,
                sub(mul(z, rx)?, mul(x, rz)?)?,
            )?,
            add(
                add(mul(z, rw)?, mul(rz, w)?)?,
                sub(mul(x, ry)?, mul(y, rx)?)?,
            )?,
        ],
    })
}
fn advance_pose<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    dt: f32,
    local_transform: SMatrix,
) -> Result<(), B::Error> {
    let angular = vector(b, v.wrapping_add(0x420))?;
    let delta = [
        mul(dt, angular[0])?,
        mul(dt, angular[1])?,
        mul(dt, angular[2])?,
    ];
    let delta = transform(local_transform, delta)?;
    let side = vector(b, v.wrapping_add(0xc48))?;
    let up = vector(b, v.wrapping_add(0xc54))?;
    let direction = vector(b, v.wrapping_add(0xc3c))?;
    let first = quat(side, delta[0])?;
    let second = quat(up, delta[1])?;
    let combined = compose(first, second)?;
    let rotation = compose(combined, quat(direction, delta[2])?)?;
    for value in rotation.wxyz {
        number(value)?;
    }
    let mut rotation_matrix = SMatrix::identity();
    rotation_matrix
        .try_set_rot3(rotation)
        .map_err(|_| VehicleMovePrepareUnavailable::NumericDomain)?;
    let dir = transform_assign(rotation_matrix, direction)?;
    put_vector(b, v.wrapping_add(0xc3c), dir)?;
    let up = transform_assign(rotation_matrix, up)?;
    put_vector(b, v.wrapping_add(0xc54), up)?;
    let center = vector(b, v.wrapping_add(0x398))?;
    let mut to_center = SMatrix::identity();
    to_center.elements[12..15].copy_from_slice(&center);
    let mut from_center = SMatrix::identity();
    from_center.elements[12..15].copy_from_slice(&center.map(|x| -x));
    // The original builds translation(-center)*rotation*translation(center).
    let rotate_about_center = from_center
        .try_mul_affine(&rotation_matrix)
        .and_then(|m| m.try_mul_affine(&to_center))
        .map_err(|_| VehicleMovePrepareUnavailable::NumericDomain)?;
    let dir = vector(b, v.wrapping_add(0xc3c))?;
    let up = vector(b, v.wrapping_add(0xc54))?;
    let side = neg_cross(dir, up)?;
    put_vector(b, v.wrapping_add(0xc48), side)?;
    let up = neg_cross(side, dir)?;
    put_vector(b, v.wrapping_add(0xc54), up)?;
    for offset in [0xc48, 0xc54, 0xc3c] {
        normalize_memory(b, v.wrapping_add(offset))?;
    }
    let p = transform_assign(rotate_about_center, vector(b, v.wrapping_add(0x320))?)?;
    put_vector(b, v.wrapping_add(0x320), p)?;
    let velocity = vector(b, v.wrapping_add(0x1f90))?;
    for (i, x) in velocity.into_iter().enumerate() {
        let a = v.wrapping_add(0x320 + i as u32 * 4);
        let new = add(mul(dt, x)?, float(b, a)?)?;
        store_float(b, a, new)?;
    }
    Ok(())
}

/// Original Move phase RVA e41dc through e6103. Shared force state comes from
/// the same invocation's wheel solver; no synthetic contact force is supplied.
pub fn vehicle_move_integrate_body<B: VehicleRigidBodyBackend>(
    b: &mut B,
    vehicle: u32,
    timestep: f32,
    setup: VehicleMoveForceSetup,
    mut forces: VehicleMoveBodyForces,
) -> Result<(), B::Error> {
    b.write_byte(vehicle.wrapping_add(0x228), setup.saved_traction_control)
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    store(b, vehicle.wrapping_add(0x5b8), setup.saved_brake_bits)?;
    for i in 0..3 {
        forces.linear_force[i] = add(
            forces.linear_force[i],
            float(b, vehicle.wrapping_add(0xcd0 + i as u32 * 4))?,
        )?;
        forces.torque[i] = add(
            forces.torque[i],
            float(b, vehicle.wrapping_add(0xcdc + i as u32 * 4))?,
        )?;
    }
    if number(float(b, vehicle.wrapping_add(0x59c))?)? < 3.0 && forces.any_contact {
        let mut inverse = matrix(b, vehicle.wrapping_add(0xd28))?;
        inverse.elements[12..15].fill(0.0);
        let mut count = word(b, vehicle.wrapping_add(0x4c4))?;
        let mut normal = [0.0; 3];
        let mut normal_friction = 0.0;
        let mut tangent_friction = 0.0;
        if count != 0 {
            let mut end = word(b, vehicle.wrapping_add(0xc38))?.wrapping_add(count.wrapping_mul(4));
            loop {
                end = end.wrapping_sub(4);
                let wheel = word(b, end)?;
                let flags = word(b, wheel.wrapping_add(0x120))?;
                if flags & 8 != 0 && flags & 0x40000000 == 0 {
                    for (i, x) in normal.iter_mut().enumerate() {
                        *x = add(*x, float(b, wheel.wrapping_add(0x34 + i as u32 * 4))?)?;
                    }
                    normal_friction = add(normal_friction, float(b, wheel.wrapping_add(0xa0))?)?;
                    tangent_friction = add(tangent_friction, float(b, wheel.wrapping_add(0xa4))?)?;
                }
                count = count.wrapping_sub(1);
                if count == 0 {
                    break;
                }
            }
        }
        normal_friction = sub(normal_friction, forces.drive_force_sum)?;
        let local_normal = transform(inverse, normal)?;
        let external = vector(b, vehicle.wrapping_add(0x410))?;
        let mut external = transform(inverse, external)?;
        external[0] = add(external[0], local_normal[0])?;
        external[2] = add(external[2], local_normal[2])?;
        let mut counter = [0.0; 3];
        if tangent_friction < external[0].abs() {
            external[0] = if external[0] <= 0.0 {
                -tangent_friction
            } else {
                tangent_friction
            };
        }
        counter[0] = -mul(
            sub(
                1.0,
                mul(
                    float(b, vehicle.wrapping_add(0x59c))?,
                    f32::from_bits(0x3eaaaaab),
                )?,
            )?,
            external[0],
        )?;
        if word(b, vehicle.wrapping_add(0x55c))? != 0
            && number(float(b, vehicle.wrapping_add(0x59c))?)? < 2.0
            && number(float(b, vehicle.wrapping_add(0x3a4))?)? < f32::from_bits(0x3e99999a)
            && number(float(b, vehicle.wrapping_add(0x3a8))?)? < f32::from_bits(0x3e99999a)
        {
            let mut count = word(b, vehicle.wrapping_add(0x4c4))?;
            let mut blocked = false;
            if count != 0 {
                let mut end =
                    word(b, vehicle.wrapping_add(0xc38))?.wrapping_add(count.wrapping_mul(4));
                loop {
                    end = end.wrapping_sub(4);
                    let w = word(b, end)?;
                    count = count.wrapping_sub(1);
                    let flags = word(b, w.wrapping_add(0x120))?;
                    if flags & 0x202 != 0 && flags & 8 != 0 && flags & 0x10 != 0 {
                        blocked = true;
                        break;
                    }
                    if count == 0 {
                        break;
                    }
                }
            }
            if !blocked {
                if normal_friction < external[2].abs() {
                    external[2] = if external[2] <= 0.0 {
                        -normal_friction
                    } else {
                        normal_friction
                    };
                }
                counter[2] = -mul(
                    sub(1.0, mul(float(b, vehicle.wrapping_add(0x59c))?, 0.5)?)?,
                    external[2],
                )?;
            }
        }
        let mut world = matrix(b, vehicle.wrapping_add(0xce8))?;
        world.elements[12..15].fill(0.0);
        let counter = transform_assign(world, counter)?;
        for (i, x) in counter.into_iter().enumerate() {
            forces.linear_force[i] = add(x, forces.linear_force[i])?;
        }
    }
    let dt_over_mass = div(timestep, float(b, vehicle.wrapping_add(0x5f4))?)?;
    for (i, force) in forces.linear_force.into_iter().enumerate() {
        let additional = float(b, vehicle.wrapping_add(0x410 + i as u32 * 4))?;
        let increment = mul(dt_over_mass, add(force, additional)?)?;
        let a = vehicle.wrapping_add(0x1f90 + i as u32 * 4);
        let next = add(increment, float(b, a)?)?;
        store_float(b, a, next)?;
    }
    copy(
        b,
        vehicle.wrapping_add(0x358),
        vehicle.wrapping_add(0x1f90),
        3,
    )?;
    normalize_velocity(b, vehicle.wrapping_add(0x358))?;
    if b.read_byte(vehicle.wrapping_add(0x1a4))
        .map_err(VehicleMovePrepareUnavailable::Effect)?
        & 4
        != 0
        && b.read_byte(vehicle.wrapping_add(0x394))
            .map_err(VehicleMovePrepareUnavailable::Effect)?
            != 0
        && b.read_byte(vehicle.wrapping_add(0xc60))
            .map_err(VehicleMovePrepareUnavailable::Effect)?
            == 0
        && add(setup.limited_motor_force[1], setup.limited_motor_force[0])? < 0.0
    {
        let normals = normalized(forces.contact_normal_sum)?;
        let up = vector(b, vehicle.wrapping_add(0xc54))?;
        let dot = add(
            add(mul(normals[2], up[2])?, mul(normals[1], up[1])?)?,
            mul(normals[0], up[0])?,
        )?;
        if dot > f32::from_bits(0x3f7ae148) {
            for offset in [0x1f90, 0x1f94, 0x1f98, 0x358, 0x35c, 0x360] {
                store(b, vehicle.wrapping_add(offset), 0)?;
            }
        }
    }
    let local_torque = transform_assign(matrix(b, vehicle.wrapping_add(0xda8))?, forces.torque)?;
    let mut angular_acceleration = [0.; 3];
    for i in 0..3 {
        angular_acceleration[i] = mul(
            local_torque[i],
            float(b, vehicle.wrapping_add(0x438 + i as u32 * 4))?,
        )?;
    }
    let angular_acceleration = transform_assign(
        matrix(b, vehicle.wrapping_add(0xd68))?,
        angular_acceleration,
    )?;
    for (i, acceleration) in angular_acceleration.into_iter().enumerate() {
        let a = vehicle.wrapping_add(0x420 + i as u32 * 4);
        let next = add(mul(acceleration, timestep)?, float(b, a)?)?;
        store_float(b, a, next)?;
    }
    let velocity = vector(b, vehicle.wrapping_add(0x1f90))?;
    let displacement = [
        mul(timestep, velocity[0])?,
        mul(timestep, velocity[1])?,
        mul(timestep, velocity[2])?,
    ];
    let distance = length(displacement)?;
    let distance = add(distance, float(b, vehicle.wrapping_add(0x5b4))?)?;
    store_float(b, vehicle.wrapping_add(0x5b4), distance)?;
    let mut previous = pose(b, vehicle)?;
    let mut local_transform = matrix(b, vehicle.wrapping_add(0xd28))?;
    local_transform.elements[12..15].fill(0.0);
    advance_pose(b, vehicle, timestep, local_transform)?;
    publish(b, vehicle)?;
    let flags = word(b, vehicle.wrapping_add(0xc68))?;
    let collided_before = b
        .read_byte(vehicle.wrapping_add(0x48))
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    b.write_byte(vehicle.wrapping_add(0x48), 0)
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    if flags & 0x10 == 0 {
        return Ok(());
    }
    if flags & 0x400 != 0 {
        copy(
            b,
            vehicle.wrapping_add(0xde8),
            vehicle.wrapping_add(0xce8),
            16,
        )?;
        copy(
            b,
            vehicle.wrapping_add(0xe28),
            vehicle.wrapping_add(0xd28),
            16,
        )?;
    } else {
        let steps = word(b, vehicle.wrapping_add(0x64))?;
        if steps as u16 != (steps >> 16) as u16 {
            return Ok(());
        }
        previous = Pose {
            position: vector(b, vehicle.wrapping_add(0x68))?,
            direction: vector(b, vehicle.wrapping_add(0x74))?,
            up: vector(b, vehicle.wrapping_add(0x80))?,
            side: vector(b, vehicle.wrapping_add(0x8c))?,
        };
    }
    let speed = length_xyz(vector(b, vehicle.wrapping_add(0x1f90))?)?;
    let angular_speed = length_xyz(vector(b, vehicle.wrapping_add(0x420))?)?;
    let mut call = VehicleBodyCollisionCall {
        collision_records: vehicle.wrapping_add(0x4ac),
        correction_first: [0.; 3],
        correction_second: [0.; 3],
        auxiliary_scalar: 0.,
        auxiliary_vector: [0.; 3],
        previous_up: previous.up,
        milliseconds: 0,
    };
    if speed > f32::from_bits(0x3dcccccd)
        || angular_speed > f32::from_bits(0x3d4ccccd)
        || b.read_byte(vehicle.wrapping_add(0xc60))
            .map_err(VehicleMovePrepareUnavailable::Effect)?
            != 0
        || collided_before != 0
    {
        let ms: f32 = mul(timestep, 1000.0)?;
        if f64::from(ms) < i32::MIN as f64 || f64::from(ms) >= 2_147_483_648.0 {
            return Err(VehicleMovePrepareUnavailable::NumericDomain);
        }
        crate::native_precision::record_precision(ms != ms.trunc());
        call.milliseconds = ms.trunc() as i32 as u32;
        let collided = b
            .body_collision(vehicle, &mut call)
            .map_err(VehicleMovePrepareUnavailable::Effect)?;
        b.write_byte(vehicle.wrapping_add(0x48), u8::from(collided))
            .map_err(VehicleMovePrepareUnavailable::Effect)?;
    }
    if b.read_byte(vehicle.wrapping_add(0x48))
        .map_err(VehicleMovePrepareUnavailable::Effect)?
        == 0
    {
        return Ok(());
    }
    previous.up = call.previous_up;
    restore(b, vehicle, previous)?;
    publish(b, vehicle)?;
    let mut retry_transform = matrix(b, vehicle.wrapping_add(0xe28))?;
    retry_transform.elements[12..15].fill(0.0);
    advance_pose(b, vehicle, timestep, retry_transform)?;
    if collided_before != 0 {
        let first = squared(call.correction_first)?;
        if first > f32::from_bits(0x3b23d70a) {
            for (i, x) in call.correction_first.into_iter().enumerate() {
                let a = vehicle.wrapping_add(0x320 + i as u32 * 4);
                let next = add(x, float(b, a)?)?;
                store_float(b, a, next)?;
            }
        }
        let second = add(
            add(
                mul(call.correction_second[0], call.correction_second[0])?,
                mul(call.correction_second[2], call.correction_second[2])?,
            )?,
            mul(call.correction_second[1], call.correction_second[1])?,
        )?;
        if second > f32::from_bits(0x3b23d70a) && first <= f32::from_bits(0x3b23d70a) {
            for (i, x) in call.correction_second.into_iter().enumerate() {
                let a = vehicle.wrapping_add(0x320 + i as u32 * 4);
                let next = add(x, float(b, a)?)?;
                store_float(b, a, next)?;
            }
        }
    }
    publish(b, vehicle)?;
    if length_xyz(vector(b, vehicle.wrapping_add(0x1f90))?)? <= f32::from_bits(0x3e4ccccd)
        && length_xyz(vector(b, vehicle.wrapping_add(0x420))?)? <= f32::from_bits(0x3da3d70a)
    {
        for offset in [0x1f90, 0x1f94, 0x1f98, 0x420, 0x424, 0x428] {
            store(b, vehicle.wrapping_add(offset), 0)?;
        }
        let flags = word(b, vehicle.wrapping_add(0x1a4))?;
        store(b, vehicle.wrapping_add(0x1a4), flags | 2)?;
    }
    Ok(())
}
