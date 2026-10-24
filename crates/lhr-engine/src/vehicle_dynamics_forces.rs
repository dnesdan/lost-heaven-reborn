//! Aerodynamic and drivetrain force setup between wheel preparation and the
//! per-wheel force solver in the original PC Move (RVA e294a through e2fb0).
use crate::native_transform::SMatrix;
use crate::vehicle_dynamics_prepare::{
    add, div, float, mul, number, store, store_float, sub, word, Result,
    VehicleMovePrepareUnavailable, VehicleMovePrepared,
};
use crate::vehicle_init::VehicleInitBackend;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMoveForceSetup {
    pub linear_force: [f32; 3],
    pub torque: [f32; 3],
    pub limited_motor_force: [f32; 2],
    pub local_angular_velocity: [f32; 3],
    pub saved_traction_control: u8,
    pub saved_brake_bits: u32,
}
fn vector<B: VehicleInitBackend>(b: &mut B, address: u32) -> Result<[f32; 3], B::Error> {
    Ok([
        float(b, address)?,
        float(b, address.wrapping_add(4))?,
        float(b, address.wrapping_add(8))?,
    ])
}
fn matrix<B: VehicleInitBackend>(b: &mut B, address: u32) -> Result<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    for (i, value) in elements.iter_mut().enumerate() {
        *value = float(b, address.wrapping_add(i as u32 * 4))?;
    }
    Ok(SMatrix { elements })
}
fn cross<E>(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], E> {
    Ok([
        sub(mul(a[2], b[1])?, mul(a[1], b[2])?)?,
        sub(mul(a[0], b[2])?, mul(a[2], b[0])?)?,
        sub(mul(a[1], b[0])?, mul(a[0], b[1])?)?,
    ])
}

/// Arguments retain the original Move stack order: dt, three drag terms, two
/// motor-force channels. `prepared` must come from this same Move invocation.
pub fn vehicle_move_force_setup<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    arguments: [f32; 6],
    prepared: VehicleMovePrepared,
) -> Result<VehicleMoveForceSetup, B::Error> {
    let mut force = [0.0; 3];
    let mut torque = [0.0; 3];
    if b.read_byte(vehicle.wrapping_add(0xc68))
        .map_err(VehicleMovePrepareUnavailable::Effect)?
        & 8
        != 0
    {
        for (point_index, (point_offset, magnitude)) in
            [(0x3e0, arguments[2]), (0x3ec, arguments[3])]
                .into_iter()
                .enumerate()
        {
            let point = vector(b, vehicle.wrapping_add(point_offset))?;
            let transform = matrix(b, vehicle.wrapping_add(0xce8))?;
            let world = transform
                .try_transform_point(point)
                .map_err(|_| VehicleMovePrepareUnavailable::NumericDomain)?;
            let applied = [
                mul(prepared.down[0], magnitude)?,
                mul(prepared.down[1], magnitude)?,
                mul(prepared.down[2], magnitude)?,
            ];
            let center = vector(b, vehicle.wrapping_add(0x398))?;
            let arm = [
                sub(world[0], center[0])?,
                sub(world[1], center[1])?,
                sub(world[2], center[2])?,
            ];
            let moment = cross(arm, applied)?;
            for i in 0..3 {
                if point_index == 0 {
                    force[i] = applied[i];
                    torque[i] = moment[i];
                } else {
                    force[i] = add(force[i], applied[i])?;
                    torque[i] = add(torque[i], moment[i])?;
                }
            }
        }
    }
    let drag = -number(arguments[1])?;
    let velocity_direction = vector(b, vehicle.wrapping_add(0x358))?;
    for i in 0..3 {
        force[i] = add(mul(drag, velocity_direction[i])?, force[i])?;
    }
    let gear = word(b, vehicle.wrapping_add(0x55c))?;
    let ratio = float(
        b,
        vehicle
            .wrapping_add(0x570)
            .wrapping_add(gear.wrapping_mul(4)),
    )?;
    let clutch = float(b, vehicle.wrapping_add(0x5dc))?;
    let transmission = float(b, vehicle.wrapping_add(0x5d0))?;
    let scale = float(b, vehicle.wrapping_add(0x44))?;
    let maximum = mul(mul(mul(mul(ratio, clutch)?, transmission)?, scale)?, 0.5)?;
    let mode = word(b, vehicle.wrapping_add(0xb8))?;
    let mut limited = [arguments[4], arguments[5]];
    for value in &mut limited {
        number(*value)?;
        if maximum < value.abs() {
            *value = if *value < 0.0 { -maximum } else { maximum };
        }
        if mode == 0 || mode == 2 {
            if sub(prepared.wheel_speed, float(b, vehicle.wrapping_add(0x59c))?)?.abs() > 2.0 {
                *value = if prepared.wheel_speed <= number(float(b, vehicle.wrapping_add(0x59c))?)?
                {
                    -maximum
                } else {
                    maximum
                };
            }
            let angle = number(float(b, vehicle.wrapping_add(0xc64))?)?;
            if angle < f32::from_bits(0x3fc9_0fdb) && (gear as i32) < 0
                || angle > f32::from_bits(0x3fc9_0fdb) && (gear as i32) > 0
            {
                *value = maximum;
            }
        } else if prepared.wheel_speed < number(float(b, vehicle.wrapping_add(0x59c))?)? {
            *value =
                if number(float(b, vehicle.wrapping_add(0xc64))?)? >= f32::from_bits(0x3fc9_0fdb) {
                    maximum
                } else {
                    -maximum
                };
        }
        if mode == 0
            && b.read_byte(vehicle.wrapping_add(0x99))
                .map_err(VehicleMovePrepareUnavailable::Effect)?
                == 0
            && number(float(b, vehicle.wrapping_add(0x42c))?)? != 0.0
        {
            *value = div(*value, mul(float(b, vehicle.wrapping_add(0x42c))?, 0.5)?)?;
        }
    }
    let speed = number(float(b, vehicle.wrapping_add(0x59c))?)?;
    let saved_traction_control = b
        .read_byte(vehicle.wrapping_add(0x228))
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    let saved_brake_bits = word(b, vehicle.wrapping_add(0x5b8))?;
    store(b, vehicle.wrapping_add(0x554), 0)?;
    if speed > number(float(b, vehicle.wrapping_add(0x4a4))?)?
        && speed
            < add(
                float(b, vehicle.wrapping_add(0x4a4))?,
                f32::from_bits(0x4033_3333),
            )?
    {
        b.write_byte(vehicle.wrapping_add(0x228), 1)
            .map_err(VehicleMovePrepareUnavailable::Effect)?;
        store_float(b, vehicle.wrapping_add(0x5b8), 1.0)?;
    }
    let mut local_transform = matrix(b, vehicle.wrapping_add(0xd28))?;
    store(b, vehicle.wrapping_add(0x148), 0)?;
    store(b, vehicle.wrapping_add(0x144), 0)?;
    local_transform.elements[12..15].fill(0.0);
    let angular_velocity = vector(b, vehicle.wrapping_add(0x420))?;
    let local_angular_velocity = local_transform
        .try_transform_point(angular_velocity)
        .map_err(|_| VehicleMovePrepareUnavailable::NumericDomain)?;
    b.write_byte(vehicle.wrapping_add(0x394), 0)
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    Ok(VehicleMoveForceSetup {
        linear_force: force,
        torque,
        limited_motor_force: limited,
        local_angular_velocity,
        saved_traction_control,
        saved_brake_bits,
    })
}
