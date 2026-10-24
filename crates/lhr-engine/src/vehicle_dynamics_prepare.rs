//! Wheel friction preparation in the original six-float C_Vehicle::Move.
//! This is an internal phase (PC RVA e2090 through e294a), not a complete Move.
//! It operates on the existing addressable vehicle and wheel records.

use crate::vehicle_init::VehicleInitBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleMovePrepareUnavailable<E> {
    Effect(E),
    NumericDomain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMovePrepared {
    pub down: [f32; 3],
    pub wheel_speed: f32,
    /// Reverse wheel traversal alternates channels even for inactive wheels.
    pub driven_normal_force: [f32; 2],
}

pub(crate) type Result<T, E> = std::result::Result<T, VehicleMovePrepareUnavailable<E>>;
pub(crate) fn number<E>(v: f32) -> Result<f32, E> {
    if v.is_normal() || v == 0.0 {
        Ok(v)
    } else {
        Err(VehicleMovePrepareUnavailable::NumericDomain)
    }
}
pub(crate) fn result<E>(v: f32, wide: f64) -> Result<f32, E> {
    if v.is_normal() || v == 0.0 && wide == 0.0 {
        Ok(v)
    } else {
        Err(VehicleMovePrepareUnavailable::NumericDomain)
    }
}
pub(crate) fn mul<E>(a: f32, b: f32) -> Result<f32, E> {
    number(a)?;
    number(b)?;
    let value = result(a * b, f64::from(a) * f64::from(b))?;
    crate::native_precision::record_mul(a, b, value);
    Ok(value)
}
pub(crate) fn add<E>(a: f32, b: f32) -> Result<f32, E> {
    number(a)?;
    number(b)?;
    let value = result(a + b, f64::from(a) + f64::from(b))?;
    crate::native_precision::record_add(a, b, value);
    Ok(value)
}
pub(crate) fn sub<E>(a: f32, b: f32) -> Result<f32, E> {
    number(a)?;
    number(b)?;
    let value = result(a - b, f64::from(a) - f64::from(b))?;
    crate::native_precision::record_sub(a, b, value);
    Ok(value)
}
pub(crate) fn div<E>(a: f32, b: f32) -> Result<f32, E> {
    number(a)?;
    number(b)?;
    let value = result(a / b, f64::from(a) / f64::from(b))?;
    crate::native_precision::record_div(a, b, value);
    Ok(value)
}
pub(crate) fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Result<u32, B::Error> {
    b.read_word(a)
        .map_err(VehicleMovePrepareUnavailable::Effect)
}
pub(crate) fn float<B: VehicleInitBackend>(b: &mut B, a: u32) -> Result<f32, B::Error> {
    Ok(f32::from_bits(word(b, a)?))
}
pub(crate) fn store<B: VehicleInitBackend>(b: &mut B, a: u32, v: u32) -> Result<(), B::Error> {
    b.write_word(a, v)
        .map_err(VehicleMovePrepareUnavailable::Effect)
}
pub(crate) fn store_float<B: VehicleInitBackend>(
    b: &mut B,
    a: u32,
    v: f32,
) -> Result<(), B::Error> {
    store(b, a, v.to_bits())
}

// The table root is the original PC global, not a configurable substitute for
// game surface data. Both pointer loads are repeated at their original uses.
fn material_record<B: VehicleInitBackend>(
    b: &mut B,
    material: u32,
    index: u32,
) -> Result<u32, B::Error> {
    let root = word(b, 0x63d9c8)?;
    let owner = word(
        b,
        root.wrapping_add(material.wrapping_mul(0x28))
            .wrapping_add(0x24),
    )?;
    let records = word(b, owner.wrapping_add(4))?;
    Ok(records.wrapping_add(index.wrapping_mul(0x5c)))
}

/// Finite normal-or-zero CW007f/PC24 nearest arithmetic, as in Engine. Raw
/// record flags and copies are not admitted as floating-point operands.
pub fn vehicle_move_prepare<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    timestep: f32,
) -> Result<VehicleMovePrepared, B::Error> {
    let vx = vehicle.wrapping_add(0xc54);
    let down_x = -number(float(b, vx)?)?;
    let down_y = -number(float(b, vx.wrapping_add(4))?)?;
    let down_z = -number(float(b, vx.wrapping_add(8))?)?;
    let low = b
        .read_byte(vehicle.wrapping_add(0x66))
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    let high = b
        .read_byte(vehicle.wrapping_add(0x67))
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    let step = u16::from_le_bytes([low, high])
        .wrapping_add(1)
        .to_le_bytes();
    b.write_byte(vehicle.wrapping_add(0x66), step[0])
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    b.write_byte(vehicle.wrapping_add(0x67), step[1])
        .map_err(VehicleMovePrepareUnavailable::Effect)?;
    let gear = word(b, vehicle.wrapping_add(0x55c))?;
    let rotation = float(b, vehicle.wrapping_add(0x548))?;
    let ratio = float(
        b,
        vehicle
            .wrapping_add(0x570)
            .wrapping_add(gear.wrapping_mul(4)),
    )?;
    let ratio_rotation = div(rotation, ratio)?;
    let mut remaining = word(b, vehicle.wrapping_add(0x4c4))?;
    let wheel_speed = mul(ratio_rotation, float(b, vehicle.wrapping_add(0x5ac))?)?;
    let mut totals = [0.0; 2];
    let mut channel = 0;
    while remaining != 0 {
        let pointers = word(b, vehicle.wrapping_add(0xc38))?;
        remaining = remaining.wrapping_sub(1);
        let wheel = word(b, pointers.wrapping_add(remaining.wrapping_mul(4)))?;
        let flags = word(b, wheel.wrapping_add(0x120))?;
        if flags & 8 == 0 || flags & 0x4000_0000 != 0 {
            for offset in [0xa0, 0xa4, 0x34, 0x38, 0x3c, 0x108] {
                store(b, wheel.wrapping_add(offset), 0)?;
            }
        } else {
            let material = word(b, wheel.wrapping_add(0xac))?;
            let negative = (flags as i32) < 0;
            let slipping = (flags as u8 as i8) < 0;
            if slipping {
                let speed = float(b, vehicle.wrapping_add(0x59c))?;
                let scale = float(b, wheel.wrapping_add(0xc4))?;
                let target = if wheel_speed <= add(speed, scale)? {
                    let delta = sub(wheel_speed, float(b, vehicle.wrapping_add(0x59c))?)?;
                    let amount = div(delta, float(b, wheel.wrapping_add(0xc4))?)?;
                    if amount < 0.0 {
                        0.0
                    } else {
                        amount
                    }
                } else {
                    1.0
                };
                if number(float(b, wheel.wrapping_add(0xc8))?)? < target {
                    let increase = mul(timestep, float(b, wheel.wrapping_add(0xcc))?)?;
                    let current = float(b, wheel.wrapping_add(0xc8))?;
                    store_float(b, wheel.wrapping_add(0xc8), add(increase, current)?)?;
                }
                if target < number(float(b, wheel.wrapping_add(0xc8))?)? {
                    store_float(b, wheel.wrapping_add(0xc8), target)?;
                }
            }
            let index = if material == 0 {
                0
            } else {
                word(b, wheel.wrapping_add(0xb0))?
            };
            let offsets = if material == 0 {
                if negative {
                    [0xf4, 0xf8, 0xfc, 0x100]
                } else {
                    [0xd0, 0xd4, 0xd8, 0xdc]
                }
            } else if negative {
                [0x48, 0x44, 0x54, 0x50]
            } else {
                [0x1c, 0x18, 0x28, 0x24]
            };
            let source = if material == 0 {
                wheel
            } else {
                material_record(b, material, index)?
            };
            let first = if slipping {
                let fall = mul(
                    float(b, source.wrapping_add(offsets[0]))?,
                    float(b, wheel.wrapping_add(0xc8))?,
                )?;
                sub(float(b, source.wrapping_add(offsets[1]))?, fall)?
            } else {
                float(b, source.wrapping_add(offsets[1]))?
            };
            let first = mul(first, float(b, wheel.wrapping_add(0x90))?)?;
            store_float(b, wheel.wrapping_add(0xa0), first)?;
            let source = if material == 0 {
                wheel
            } else {
                material_record(b, material, index)?
            };
            let second = if slipping {
                let fall = mul(
                    float(b, source.wrapping_add(offsets[2]))?,
                    float(b, wheel.wrapping_add(0xc8))?,
                )?;
                sub(float(b, source.wrapping_add(offsets[3]))?, fall)?
            } else {
                float(b, source.wrapping_add(offsets[3]))?
            };
            let second = mul(second, float(b, wheel.wrapping_add(0x90))?)?;
            store_float(b, wheel.wrapping_add(0xa4), second)?;
            let reached_flags = if material == 0 {
                flags
            } else {
                word(b, wheel.wrapping_add(0x120))?
            };
            if reached_flags & 0x120 != 0 {
                let (source, offset) = if material == 0 {
                    (wheel, if negative { 0xec } else { 0xc0 })
                } else {
                    (
                        material_record(b, material, index)?,
                        if negative { 0x38 } else { 0xc },
                    )
                };
                let a = mul(
                    float(b, source.wrapping_add(offset))?,
                    float(b, wheel.wrapping_add(0xa0))?,
                )?;
                store_float(b, wheel.wrapping_add(0xa0), a)?;
                let source = if material == 0 {
                    wheel
                } else {
                    material_record(b, material, index)?
                };
                let c = mul(
                    float(b, source.wrapping_add(offset))?,
                    float(b, wheel.wrapping_add(0xa4))?,
                )?;
                store_float(b, wheel.wrapping_add(0xa4), c)?;
            }
            let a = add(
                float(b, wheel.wrapping_add(0xa0))?,
                float(b, vehicle.wrapping_add(0x40))?,
            )?;
            store_float(b, wheel.wrapping_add(0xa0), a)?;
            let c = add(
                float(b, wheel.wrapping_add(0xa4))?,
                float(b, vehicle.wrapping_add(0x40))?,
            )?;
            store_float(b, wheel.wrapping_add(0xa4), c)?;
            let mut contact = [0.0; 3];
            for (i, item) in contact.iter_mut().enumerate() {
                *item = mul(
                    float(b, wheel.wrapping_add(0x40 + i as u32 * 4))?,
                    float(b, wheel.wrapping_add(0x90))?,
                )?;
            }
            for (i, item) in contact.into_iter().enumerate() {
                store_float(b, wheel.wrapping_add(0x34 + i as u32 * 4), item)?;
            }
            // The native stack adds the Z and Y products before the X product.
            let z = mul(
                float(b, wheel.wrapping_add(0x48))?,
                float(b, vehicle.wrapping_add(0xc5c))?,
            )?;
            let y = mul(
                float(b, wheel.wrapping_add(0x44))?,
                float(b, vehicle.wrapping_add(0xc58))?,
            )?;
            let zy = add(z, y)?;
            let x = mul(
                float(b, wheel.wrapping_add(0x40))?,
                float(b, vehicle.wrapping_add(0xc54))?,
            )?;
            let denominator = add(zy, x)?;
            let denominator = if denominator < 0.5 { 0.5 } else { denominator };
            let reciprocal = div(1.0, denominator)?;
            for offset in [0x34, 0x38, 0x3c] {
                let v = mul(reciprocal, float(b, wheel.wrapping_add(offset))?)?;
                store_float(b, wheel.wrapping_add(offset), v)?;
            }
            // Clamp both friction coefficients with native strict-min policy;
            // equality selects the table value and retains its sign-zero bits.
            let limit = if material == 0 {
                wheel.wrapping_add(if negative { 0xe4 } else { 0xb8 })
            } else {
                material_record(b, material, index)?.wrapping_add(if negative { 0x30 } else { 4 })
            };
            let a = float(b, wheel.wrapping_add(0xa0))?;
            let selected = if number(a)? < number(float(b, limit)?)? {
                a.to_bits()
            } else {
                word(b, limit)?
            };
            store(b, wheel.wrapping_add(0xa0), selected)?;
            let limit = if material == 0 {
                wheel.wrapping_add(if negative { 0xe4 } else { 0xb8 })
            } else {
                material_record(b, material, index)?.wrapping_add(if negative { 0x30 } else { 4 })
            };
            let c = float(b, wheel.wrapping_add(0xa4))?;
            let selected = if number(c)? < number(float(b, limit)?)? {
                c.to_bits()
            } else {
                word(b, limit)?
            };
            store(b, wheel.wrapping_add(0xa4), selected)?;
            if reached_flags & 0x202 != 0 {
                totals[channel] = add(totals[channel], float(b, wheel.wrapping_add(0xa0))?)?;
            }
        }
        channel ^= 1;
    }
    Ok(VehicleMovePrepared {
        down: [down_x, down_y, down_z],
        wheel_speed,
        driven_normal_force: totals,
    })
}
