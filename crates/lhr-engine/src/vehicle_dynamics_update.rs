//! Addressable PC `C_Vehicle::Update`, canonical RVA 0xe0a70.
//!
//! Original state/control flow is retained over the caller's live memory.
//! Frame, audio/particle, collision-registration and wheel-pose procedures are
//! required typed calls. This orchestration does not fabricate their success.
//! Arithmetic admits finite normal binary32 or zero under masked PC24/RN;
//! errors retain all reached stores and external effects.

use crate::{native_transform::SQuat, vehicle_init::VehicleInitBackend};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VehicleUpdateUnavailable<E> {
    Effect(E),
    NumericDomain,
}
type Out<T, E> = Result<T, VehicleUpdateUnavailable<E>>;

/// External calls made by Update, in the original reached order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehicleUpdateEffect {
    ModelUpdate {
        frame: u32,
    },
    ScenePosition {
        frame: u32,
        position: u32,
    },
    Horn {
        enabled: u8,
    },
    Siren {
        enabled: u8,
    },
    SkidSound {
        seconds: f32,
    },
    EnvironmentSound,
    FlatTyreSound {
        audible: bool,
    },
    Doors {
        seconds: f32,
    },
    Windows {
        seconds: f32,
    },
    WheelSkid {
        index: u32,
        emit: bool,
    },
    DynamicUpdate {
        object: u32,
    },
    SteeringWheels {
        seconds: f32,
    },
    Lights {
        seconds: f32,
    },
    RotationWheels {
        seconds: f32,
    },
    SteeringVisual {
        frame: u32,
        direction: u32,
        angle: f32,
    },
    WheelParticles {
        milliseconds: u32,
    },
    ExhaustParticles {
        milliseconds: u32,
    },
    Axes,
    Strips {
        seconds: f32,
    },
    SetGear {
        direction: u32,
    },
}

pub trait VehicleUpdateBackend: VehicleInitBackend {
    fn vehicle_update_effect(
        &mut self,
        vehicle: u32,
        effect: VehicleUpdateEffect,
    ) -> Result<(), Self::Error>;
    /// Original scene virtual +0x68(position,0,0,0), spilled to vehicle+0xb0.
    fn vehicle_scene_sample(&mut self, position: u32) -> Result<f32, Self::Error>;
    /// Original UpdateMotorParticles, whose low-byte result controls sleep.
    fn vehicle_motor_particles(
        &mut self,
        vehicle: u32,
        milliseconds: u32,
    ) -> Result<bool, Self::Error>;
    /// Original CRT rand, PC 0x610791; preserve its global state and call order.
    fn vehicle_update_rand(&mut self) -> Result<i32, Self::Error>;
    /// Imported S_quat::Make with axis Y and angle zero in mode 2.
    fn vehicle_update_make_rotation(
        &mut self,
        axis: [f32; 3],
        angle: f32,
    ) -> Result<SQuat, Self::Error>;
    fn vehicle_update_normalize_rotation(&mut self, frame: u32) -> Result<(), Self::Error>;
}

/// Actual supplied-PC global addresses, supplied separately from vehicle data.
/// A runtime can map these fields to owned original material-table storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VehicleUpdateGlobals {
    pub materials_pointer: u32,
    pub material_index: u32,
}
impl VehicleUpdateGlobals {
    pub const SUPPLIED_PC: Self = Self {
        materials_pointer: 0x63d9b4,
        material_index: 0x63d8b0,
    };
}

fn operand<E>(x: f32) -> Out<f32, E> {
    if x == 0. || x.is_normal() {
        Ok(x)
    } else {
        Err(VehicleUpdateUnavailable::NumericDomain)
    }
}
fn arithmetic<E>(x: f32, wide: f64) -> Out<f32, E> {
    if !x.is_finite()
        || wide.abs() > f32::MAX as f64
        || (wide != 0. && wide.abs() < f32::MIN_POSITIVE as f64)
    {
        Err(VehicleUpdateUnavailable::NumericDomain)
    } else {
        Ok(x)
    }
}
fn add<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a + b, a as f64 + b as f64)?;
    crate::native_precision::record_add(a, b, result);
    Ok(result)
}
fn sub<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a - b, a as f64 - b as f64)?;
    crate::native_precision::record_sub(a, b, result);
    Ok(result)
}
fn mul<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a * b, a as f64 * b as f64)?;
    crate::native_precision::record_mul(a, b, result);
    Ok(result)
}
fn div<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a / b, a as f64 / b as f64)?;
    crate::native_precision::record_div(a, b, result);
    Ok(result)
}
fn rw<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(VehicleUpdateUnavailable::Effect)
}
fn rb<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a).map_err(VehicleUpdateUnavailable::Effect)
}
fn ww<B: VehicleInitBackend>(b: &mut B, a: u32, w: u32) -> Out<(), B::Error> {
    b.write_word(a, w).map_err(VehicleUpdateUnavailable::Effect)
}
fn wb<B: VehicleInitBackend>(b: &mut B, a: u32, w: u8) -> Out<(), B::Error> {
    b.write_byte(a, w).map_err(VehicleUpdateUnavailable::Effect)
}
fn rf<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    operand(f32::from_bits(rw(b, a)?))
}
fn wf<B: VehicleInitBackend>(b: &mut B, a: u32, w: f32) -> Out<(), B::Error> {
    ww(b, a, w.to_bits())
}
fn rh<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u16, B::Error> {
    Ok(u16::from_le_bytes([rb(b, a)?, rb(b, a.wrapping_add(1))?]))
}
fn wh<B: VehicleInitBackend>(b: &mut B, a: u32, w: u16) -> Out<(), B::Error> {
    for (i, x) in w.to_le_bytes().into_iter().enumerate() {
        wb(b, a.wrapping_add(i as u32), x)?;
    }
    Ok(())
}
fn copy3<B: VehicleInitBackend>(b: &mut B, to: u32, from: u32) -> Out<(), B::Error> {
    for i in [0, 4, 8] {
        let w = rw(b, from.wrapping_add(i))?;
        ww(b, to.wrapping_add(i), w)?;
    }
    Ok(())
}
fn effect<B: VehicleUpdateBackend>(b: &mut B, v: u32, e: VehicleUpdateEffect) -> Out<(), B::Error> {
    b.vehicle_update_effect(v, e)
        .map_err(VehicleUpdateUnavailable::Effect)
}
fn wheel<B: VehicleInitBackend>(b: &mut B, v: u32, i: u32) -> Out<u32, B::Error> {
    let p = rw(b, v.wrapping_add(0xc38))?;
    rw(b, p.wrapping_add(i.wrapping_mul(4)))
}
fn count<B: VehicleInitBackend>(b: &mut B, v: u32, offset: u32, stride: i32) -> Out<u32, B::Error> {
    let p = rw(b, v.wrapping_add(offset))?;
    Ok(if p == 0 {
        0
    } else {
        (rw(b, v.wrapping_add(offset + 4))?.wrapping_sub(p) as i32 / stride) as u32
    })
}

fn bump<B: VehicleUpdateBackend>(
    b: &mut B,
    v: u32,
    w: u32,
    g: VehicleUpdateGlobals,
) -> Out<(), B::Error> {
    let span = add(rf(b, w.wrapping_add(0x1bc))?, rf(b, w.wrapping_add(0x1b8))?)?;
    let distance = rf(b, w.wrapping_add(0x1c8))?;
    let custom = rb(b, v.wrapping_add(0x13d))? != 0;
    if distance <= span {
        if rf(b, w.wrapping_add(0x1b8))? <= rf(b, w.wrapping_add(0x1c8))? {
            let value = rw(b, w.wrapping_add(0x1c4))?;
            ww(b, w.wrapping_add(0x1c0), value)?;
        }
        return Ok(());
    }
    ww(b, w.wrapping_add(0x1c0), 0)?;
    if span > 0. {
        let distance = rf(b, w.wrapping_add(0x1c8))?;
        let quotient = div(distance, span)?;
        let periods = operand(quotient.floor())?;
        crate::native_precision::record_precision(periods != quotient);
        let remainder = sub(distance, mul(periods, span)?)?;
        if rf(b, w.wrapping_add(0x1b8))? <= remainder {
            let value = rw(b, w.wrapping_add(0x1c4))?;
            ww(b, w.wrapping_add(0x1c0), value)?;
        }
    }
    ww(b, w.wrapping_add(0x1c8), 0)?;
    let (value, minimum) = if custom {
        let random = b
            .vehicle_update_rand()
            .map_err(VehicleUpdateUnavailable::Effect)?
            % 100;
        let scale = mul(random as f32, 0.01)?;
        let difference = sub(rf(b, w.wrapping_add(0x1a8))?, rf(b, w.wrapping_add(0x1a4))?)?;
        let value = add(mul(scale, difference)?, rf(b, w.wrapping_add(0x1a4))?)?;
        wf(b, w.wrapping_add(0x1b8), value)?;
        let random = b
            .vehicle_update_rand()
            .map_err(VehicleUpdateUnavailable::Effect)?
            % 100;
        let scale = mul(random as f32, 0.01)?;
        let difference = sub(rf(b, w.wrapping_add(0x1b0))?, rf(b, w.wrapping_add(0x1ac))?)?;
        let value = add(mul(scale, difference)?, rf(b, w.wrapping_add(0x1ac))?)?;
        wf(b, w.wrapping_add(0x1bc), value)?;
        let random = b
            .vehicle_update_rand()
            .map_err(VehicleUpdateUnavailable::Effect)?
            % 200
            - 100;
        let value = mul(mul(random as f32, 0.01)?, rf(b, w.wrapping_add(0x1b4))?)?;
        wf(b, w.wrapping_add(0x1c4), value)?;
        (value, mul(rf(b, w.wrapping_add(0x1b4))?, 0.5)?)
    } else {
        let material = rw(b, w.wrapping_add(0xb4))?;
        let table = rw(b, g.materials_pointer)?;
        let index = rb(b, g.material_index.wrapping_add(material))? as u32 * 48;
        let random = b
            .vehicle_update_rand()
            .map_err(VehicleUpdateUnavailable::Effect)?
            % 100;
        let next_material = rw(b, w.wrapping_add(0xb4))?;
        let scale = mul(random as f32, 0.01)?;
        let difference = sub(
            rf(b, table.wrapping_add(index + 8))?,
            rf(b, table.wrapping_add(index + 4))?,
        )?;
        let value = add(
            mul(scale, difference)?,
            rf(b, table.wrapping_add(index + 4))?,
        )?;
        wf(b, w.wrapping_add(0x1b8), value)?;
        let table = rw(b, g.materials_pointer)?;
        let index = rb(b, g.material_index.wrapping_add(next_material))? as u32 * 48;
        let random = b
            .vehicle_update_rand()
            .map_err(VehicleUpdateUnavailable::Effect)?
            % 100;
        let scale = mul(random as f32, 0.01)?;
        let difference = sub(
            rf(b, table.wrapping_add(index + 16))?,
            rf(b, table.wrapping_add(index + 12))?,
        )?;
        let value = add(
            mul(scale, difference)?,
            rf(b, table.wrapping_add(index + 12))?,
        )?;
        let next_material = rw(b, w.wrapping_add(0xb4))?;
        wf(b, w.wrapping_add(0x1bc), value)?;
        let table = rw(b, g.materials_pointer)?;
        let random = b
            .vehicle_update_rand()
            .map_err(VehicleUpdateUnavailable::Effect)?
            % 200
            - 100;
        let index = rb(b, g.material_index.wrapping_add(next_material))? as u32 * 48;
        let value = mul(
            mul(random as f32, 0.01)?,
            rf(b, table.wrapping_add(index + 20))?,
        )?;
        wf(b, w.wrapping_add(0x1c4), value)?;
        let index = rb(b, g.material_index.wrapping_add(next_material))? as u32 * 48;
        let table = rw(b, g.materials_pointer)?;
        (value, mul(rf(b, table.wrapping_add(index + 20))?, 0.5)?)
    };
    if minimum > value.abs() {
        wf(
            b,
            w.wrapping_add(0x1c4),
            if value > 0. { minimum } else { -minimum },
        )?;
    }
    Ok(())
}

// FILD loads the unsigned 32-bit value exactly, then FMUL rounds once to
// precision24. Round the exact integer product instead of first rounding u32.
fn milliseconds_to_seconds(milliseconds: u32) -> f32 {
    let product = u64::from(milliseconds) * u64::from(0.001_f32.to_bits() & 0x7fffff | 0x800000);
    let bits = 64 - product.leading_zeros();
    let shift = bits.saturating_sub(24);
    let mut rounded = product >> shift;
    if shift != 0 {
        let remainder = product & ((1_u64 << shift) - 1);
        crate::native_precision::record_precision(remainder != 0);
        let halfway = 1_u64 << (shift - 1);
        if remainder > halfway || remainder == halfway && rounded & 1 != 0 {
            rounded += 1;
        }
    }
    (rounded as f64 * 2_f64.powi(shift as i32 - 33)) as f32
}

/// Original Update return value: 0 below the world floor, 2 when eligible for
/// sleep, otherwise 1. The floor is the original -85 guard, not a collider.
pub fn update_vehicle<B: VehicleUpdateBackend>(
    b: &mut B,
    v: u32,
    milliseconds: u32,
    g: VehicleUpdateGlobals,
) -> Out<u32, B::Error> {
    use VehicleUpdateEffect as Call;
    use VehicleUpdateUnavailable::Effect;
    let a = |offset: u32| v.wrapping_add(offset);
    if rb(b, a(0x1fa8))? != 0 {
        let value = rb(b, a(0x1fa9))?;
        wb(b, a(0xc2c), value)?;
    }
    let milliseconds = milliseconds.max(1);
    let seconds = milliseconds_to_seconds(milliseconds);
    if rf(b, a(0x1f94))? < -85. {
        return Ok(0);
    }
    let frame = rw(b, a(0x34c))?;
    effect(b, v, Call::ModelUpdate { frame })?;
    let frame = rw(b, a(0x34c))?;
    effect(
        b,
        v,
        Call::ScenePosition {
            frame,
            position: a(0x398),
        },
    )?;
    if rb(b, a(0x432))? != 0 {
        let enabled = rb(b, a(0x430))?;
        effect(b, v, Call::Horn { enabled })?;
        let enabled = rb(b, a(0x431))?;
        effect(b, v, Call::Siren { enabled })?;
        effect(b, v, Call::SkidSound { seconds })?;
        effect(b, v, Call::EnvironmentSound)?;
        let mut n = rw(b, a(0x4c4))?;
        let timer = rh(b, a(0x14))?.wrapping_add(milliseconds as u16);
        wh(b, a(0x14), timer)?;
        let order = rw(b, a(0xc38))?;
        while n != 0 {
            n = n.wrapping_sub(1);
            let w = rw(b, order.wrapping_add(n.wrapping_mul(4)))?;
            let flags = rw(b, w.wrapping_add(0x120))?;
            if flags & 0x80000008 == 0x80000008 && flags & 0x40000020 == 0 {
                wh(b, a(0x14), 0)?;
                break;
            }
        }
        let audible = rh(b, a(0x14))? < 1000;
        effect(b, v, Call::FlatTyreSound { audible })?;
        if !audible {
            wh(b, a(0x14), 1000)?;
        }
    }
    let timer = rw(b, a(0xb4))?.wrapping_add(milliseconds);
    ww(b, a(0xb4), timer)?;
    if timer > 300 && rb(b, a(0x2e0))? != 0 {
        ww(b, a(0xb4), 0)?;
        let sample = b.vehicle_scene_sample(a(0x398)).map_err(Effect)?;
        wf(b, a(0xb0), sample)?;
    }
    if rw(b, a(0x4c4))? as i32 > 0 {
        let w = wheel(b, v, 0)?;
        let z = mul(rf(b, w.wrapping_add(0x60))?, rf(b, a(0x360))?)?;
        let y = mul(rf(b, w.wrapping_add(0x5c))?, rf(b, a(0x35c))?)?;
        let x = mul(rf(b, a(0x358))?, rf(b, w.wrapping_add(0x58))?)?;
        let travel = add(
            mul(add(add(z, y)?, x)?, rf(b, a(0x5b4))?)?,
            rf(b, a(0x1fa4))?,
        )?;
        wf(b, a(0x1fa4), travel)?;
    }
    effect(b, v, Call::Doors { seconds })?;
    effect(b, v, Call::Windows { seconds })?;
    if rb(b, a(0xc68))? & 0x40 != 0 {
        let mut n = rw(b, a(0x4c4))?;
        while n != 0 {
            n = n.wrapping_sub(1);
            let w = wheel(b, v, n)?;
            let emit = rb(b, w.wrapping_add(0x120))? & 0x10 != 0;
            effect(b, v, Call::WheelSkid { index: n, emit })?;
        }
    }
    if rb(b, a(0x1a4))? & 1 != 0 {
        if rb(b, a(0x1fab))? != 0 {
            if rb(b, a(0x1faa))? == 0 {
                copy3(b, a(0x23c), a(0x320))?;
                effect(b, v, Call::DynamicUpdate { object: a(0x22c) })?;
            } else {
                let mut n = count(b, v, 0x1fe4, 72)?;
                while n != 0 {
                    n = n.wrapping_sub(1);
                    let base = rw(b, a(0x1fe4))?;
                    copy3(
                        b,
                        base.wrapping_add(n.wrapping_mul(72).wrapping_add(16)),
                        a(0x320),
                    )?;
                    let enabled = rw(b, a(0x1ffc))?;
                    if rb(b, enabled.wrapping_add(n.wrapping_mul(8).wrapping_add(4)))? != 0 {
                        let base = rw(b, a(0x1fe4))?;
                        effect(
                            b,
                            v,
                            Call::DynamicUpdate {
                                object: base.wrapping_add(n.wrapping_mul(72)),
                            },
                        )?;
                    }
                }
            }
        }
        if rb(b, a(0x1fac))? != 0 {
            copy3(b, a(0x284), a(0x320))?;
            effect(b, v, Call::DynamicUpdate { object: a(0x274) })?;
        }
        let mut n = rw(b, a(0x4c4))?;
        while n != 0 {
            n = n.wrapping_sub(1);
            let w = wheel(b, v, n)?;
            if rw(b, w.wrapping_add(0x120))? & 0x40000000 != 0 {
                let flags = rw(b, a(0xc68))?;
                ww(b, a(0xc68), flags | 0x10)?;
            } else if rf(b, a(0x2c))? != 0. {
                bump(b, v, w, g)?;
            }
        }
    }
    let mode = rw(b, a(0xb8))?;
    if mode == 0 {
        effect(b, v, Call::SteeringWheels { seconds })?;
    } else if mode == 2 {
        let mut n = rw(b, a(0x4c4))?;
        while n != 0 {
            n = n.wrapping_sub(1);
            let w = wheel(b, v, n)?;
            if rb(b, w.wrapping_add(0x120))? & 1 != 0 {
                let quat = b
                    .vehicle_update_make_rotation([0., 1., 0.], 0.)
                    .map_err(Effect)?;
                let w = wheel(b, v, n)?;
                let frame = rw(b, w.wrapping_add(8))?;
                for (i, value) in quat.wxyz.into_iter().enumerate() {
                    ww(b, frame.wrapping_add(0x9c + i as u32 * 4), value.to_bits())?;
                }
                b.vehicle_update_normalize_rotation(frame).map_err(Effect)?;
                let flags = rw(b, frame.wrapping_add(0xac))?;
                ww(
                    b,
                    frame.wrapping_add(0xac),
                    (flags & 0xfffffecb) | 0x40000008,
                )?;
            }
        }
    }
    effect(b, v, Call::Lights { seconds })?;
    if rb(b, a(0xc68))? & 4 != 0 {
        effect(b, v, Call::RotationWheels { seconds })?;
    }
    if rb(b, a(0x2e0))? != 0 {
        let frame = rw(b, a(0xc28))?;
        if frame != 0 && rb(b, a(0xc68))? & 2 != 0 {
            let angle = mul(
                div(rf(b, a(0x628))?, rf(b, a(0x610))?)?,
                f32::from_bits(0x3fc8_f5c3),
            )?;
            effect(
                b,
                v,
                Call::SteeringVisual {
                    frame,
                    direction: a(0x37c),
                    angle,
                },
            )?;
        }
        effect(b, v, Call::WheelParticles { milliseconds })?;
        effect(b, v, Call::ExhaustParticles { milliseconds })?;
        effect(b, v, Call::Axes)?;
        effect(b, v, Call::Strips { seconds })?;
    } else if rw(b, a(0xc68))? != rw(b, a(0xc6c))? {
        effect(b, v, Call::Axes)?;
    }
    let mut sleep = b.vehicle_motor_particles(v, milliseconds).map_err(Effect)?;
    for offset in [0xcd0, 0xcd4, 0xcd8, 0xcdc, 0xce0, 0xce4, 0x140] {
        ww(b, a(offset), 0)?;
    }
    wb(b, a(0x550), 0)?;
    if rb(b, a(0xf8))? & 2 != 0 {
        let mut n = count(b, v, 0x218, 52)?;
        while n != 0 {
            n = n.wrapping_sub(1);
            let base = rw(b, a(0x218))?;
            let address = base.wrapping_add(n.wrapping_mul(52).wrapping_add(14));
            let flags = rb(b, address)?;
            wb(b, address, flags & 0xfd)?;
        }
        let mut n = rw(b, a(0x4c4))?;
        while n != 0 {
            n = n.wrapping_sub(1);
            let w = wheel(b, v, n)?;
            if rw(b, w.wrapping_add(0x120))? & 0x10000000 != 0
                && rf(b, w.wrapping_add(0x18c))? < rf(b, w.wrapping_add(0x1a0))?
            {
                let blend = sub(
                    1.,
                    div(rf(b, w.wrapping_add(0x18c))?, rf(b, w.wrapping_add(0x1a0))?)?,
                )?;
                let value = mul(blend, rf(b, w.wrapping_add(0x184))?)?;
                wf(b, w.wrapping_add(0x188), value)?;
            }
            let w = wheel(b, v, n)?;
            let flags = rw(b, w.wrapping_add(0x120))?;
            ww(b, w.wrapping_add(0x120), flags & 0xefffffff)?;
        }
        let flags = rw(b, a(0xf8))?;
        ww(b, a(0xf8), flags & 0xfffffffd)?;
    }
    let flags = rw(b, a(0xf8))?;
    ww(b, a(0xf8), flags & 0xfffffff3)?;
    let mut n = count(b, v, 0xca4, 36)?;
    let base = rw(b, a(0xca4))?;
    while n != 0 {
        n = n.wrapping_sub(1);
        let owner = rw(b, base.wrapping_add(n.wrapping_mul(36).wrapping_add(32)))?;
        if owner != 0 && rb(b, owner.wrapping_add(0x30))? & 2 != 0 {
            sleep = false;
            break;
        }
    }
    let frame = rw(b, a(0x34c))?;
    let field = rw(b, frame.wrapping_add(0x120))?;
    ww(b, a(0xac), field)?;
    if sleep && rb(b, a(0x11c))? & 0x83 != 0 {
        sleep = false;
    }
    let flags = rw(b, a(0xc68))?;
    ww(b, a(0xc6c), flags)?;
    let flags = if rb(b, a(0x28))? == 0 {
        rw(b, a(0x1a4))? & 0xfdffffff
    } else {
        ww(b, a(0x1a8), 0)?;
        ww(b, a(4), 0)?;
        rw(b, a(0x1a4))? | 0x2000001
    };
    ww(b, a(0x1a4), flags)?;
    let calm = flags & 1 == 0 && sleep && rb(b, a(0x2c0))? == 0 && rb(b, a(0xc2c))? == 0;
    let restart = rb(b, a(0xc60))?;
    if calm && restart == 0 {
        return Ok(2);
    }
    let skip_gear = !calm && restart == 0 && (rf(b, a(0x5b8))? <= 0.9 || rf(b, a(0x540))? <= 0.);
    if !skip_gear
        && (rw(b, a(0xc68))? & 0x4000 == 0 || rf(b, a(0x1ec))? > 0.)
        && rb(b, a(0xc2c))? != 0
        && rb(b, a(0x4cc))? != 0
        && rw(b, a(0x55c))? != 0
        && (rb(b, a(0x394))? != 0 || rb(b, a(0x99))? != 0)
        && rw(b, a(0x560))? as i32 > 0
    {
        let speed = div(rf(b, a(0x59c))?, rf(b, a(0x5ac))?)?;
        let gear = rw(b, a(0x55c))?;
        let speed = mul(
            speed,
            rf(b, a(0x570u32.wrapping_add(gear.wrapping_mul(4))))?,
        )?;
        if rw(b, a(0x568))? as i32 >= 2 && rf(b, a(0x50c))? > speed {
            effect(b, v, Call::SetGear { direction: 1 })?;
        } else if rw(b, a(0x568))? as i32 > 2 && speed < rf(b, a(0x510))? {
            effect(b, v, Call::SetGear { direction: 2 })?;
        }
    }
    for offset in [0xc60, 0x28, 0x3c] {
        wb(b, a(offset), 0)?;
    }
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: u32 = 0x7d000000;
    #[derive(Clone)]
    struct Backend {
        bytes: Vec<u8>,
        calls: Vec<VehicleUpdateEffect>,
        fail: Option<usize>,
        sleep: bool,
        random: Vec<i32>,
    }
    impl Backend {
        fn new() -> Self {
            let mut value = Self {
                bytes: vec![0; 65536],
                calls: vec![],
                fail: None,
                sleep: true,
                random: vec![],
            };
            value.put(0x34c, BASE + 0x4000);
            value.put(0xb8, 1);
            value.put(0x4120, 0xabcdef01);
            value
        }
        fn put(&mut self, o: usize, w: u32) {
            self.bytes[o..o + 4].copy_from_slice(&w.to_le_bytes());
        }
        fn float(&mut self, o: usize, w: f32) {
            self.put(o, w.to_bits());
        }
        fn word(&self, o: usize) -> u32 {
            u32::from_le_bytes(self.bytes[o..o + 4].try_into().unwrap())
        }
        fn event(&mut self, e: VehicleUpdateEffect) -> Result<(), u32> {
            let n = self.calls.len();
            self.calls.push(e);
            if self.fail == Some(n) {
                Err(n as u32)
            } else {
                Ok(())
            }
        }
        fn wheel(&mut self) {
            self.put(0x4c4, 1);
            self.put(0xc38, BASE + 0x5000);
            self.put(0x5000, BASE + 0x6000);
            self.put(0x6008, BASE + 0x7000);
        }
    }
    impl VehicleInitBackend for Backend {
        type Error = u32;
        fn read_byte(&mut self, a: u32) -> Result<u8, u32> {
            self.bytes
                .get(a.wrapping_sub(BASE) as usize)
                .copied()
                .ok_or(a)
        }
        fn read_word(&mut self, a: u32) -> Result<u32, u32> {
            let o = a.wrapping_sub(BASE) as usize;
            self.bytes
                .get(o..o + 4)
                .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
                .ok_or(a)
        }
        fn write_byte(&mut self, a: u32, w: u8) -> Result<(), u32> {
            *self.bytes.get_mut(a.wrapping_sub(BASE) as usize).ok_or(a)? = w;
            Ok(())
        }
        fn write_word(&mut self, a: u32, w: u32) -> Result<(), u32> {
            let o = a.wrapping_sub(BASE) as usize;
            self.bytes
                .get_mut(o..o + 4)
                .ok_or(a)?
                .copy_from_slice(&w.to_le_bytes());
            Ok(())
        }
        fn free(&mut self, a: u32) -> Result<(), u32> {
            Err(a)
        }
        fn allocate(&mut self, a: u32) -> Result<u32, u32> {
            Err(a)
        }
        fn collect_frame(
            &mut self,
            _: &mut crate::vehicle_init::VehicleInitFrameVector,
            a: u32,
        ) -> Result<(), u32> {
            Err(a)
        }
        fn enumerate(
            &mut self,
            a: u32,
            _: crate::vehicle_init::VehicleInitQuery,
            _: &mut crate::vehicle_init::VehicleInitEnumeration<'_>,
        ) -> Result<(), crate::vehicle_init::VehicleInitUnavailable<u32>> {
            Err(crate::vehicle_init::VehicleInitUnavailable::Effect(a))
        }
    }
    impl VehicleUpdateBackend for Backend {
        fn vehicle_update_effect(&mut self, _: u32, e: VehicleUpdateEffect) -> Result<(), u32> {
            self.event(e)
        }
        fn vehicle_scene_sample(&mut self, _: u32) -> Result<f32, u32> {
            Ok(0.75)
        }
        fn vehicle_motor_particles(&mut self, _: u32, _: u32) -> Result<bool, u32> {
            if self.fail == Some(self.calls.len()) {
                Err(self.calls.len() as u32)
            } else {
                Ok(self.sleep)
            }
        }
        fn vehicle_update_rand(&mut self) -> Result<i32, u32> {
            if self.random.is_empty() {
                Err(0x610791)
            } else {
                Ok(self.random.remove(0))
            }
        }
        fn vehicle_update_make_rotation(
            &mut self,
            axis: [f32; 3],
            angle: f32,
        ) -> Result<SQuat, u32> {
            assert_eq!(axis, [0., 1., 0.]);
            assert_eq!(angle, 0.);
            Ok(SQuat {
                wxyz: [1., 0., 0., 0.],
            })
        }
        fn vehicle_update_normalize_rotation(&mut self, _: u32) -> Result<(), u32> {
            Ok(())
        }
    }
    fn run(b: &mut Backend, ms: u32) -> Out<u32, u32> {
        update_vehicle(b, BASE, ms, VehicleUpdateGlobals::SUPPLIED_PC)
    }

    #[test]
    fn update_sleep_and_active_returns_keep_original_tail_stores() {
        let mut b = Backend::new();
        b.put(0xcd0, 0xdeadbeef);
        b.put(0x140, 123);
        b.bytes[0x550] = 1;
        b.bytes[0x3c] = 9;
        assert_eq!(run(&mut b, 0), Ok(2));
        assert_eq!(b.word(0xac), 0xabcdef01);
        assert_eq!(b.word(0xcd0), 0);
        assert_eq!(b.word(0x140), 0);
        assert_eq!(b.bytes[0x550], 0);
        assert_eq!(b.bytes[0x3c], 9);
        assert_eq!(
            b.calls,
            vec![
                VehicleUpdateEffect::ModelUpdate {
                    frame: BASE + 0x4000
                },
                VehicleUpdateEffect::ScenePosition {
                    frame: BASE + 0x4000,
                    position: BASE + 0x398
                },
                VehicleUpdateEffect::Doors { seconds: 0.001 },
                VehicleUpdateEffect::Windows { seconds: 0.001 },
                VehicleUpdateEffect::Lights { seconds: 0.001 }
            ]
        );
        let mut b = Backend::new();
        b.bytes[0x28] = 1;
        b.bytes[0x3c] = 9;
        b.put(0x1a8, 99);
        b.put(4, 88);
        assert_eq!(run(&mut b, 16), Ok(1));
        assert_eq!(b.word(0x1a4), 0x2000001);
        assert_eq!(b.word(0x1a8), 0);
        assert_eq!(b.word(4), 0);
        assert_eq!(b.bytes[0x28], 0);
        assert_eq!(b.bytes[0x3c], 0);
    }

    #[test]
    fn update_below_world_preserves_only_the_reached_override() {
        let mut b = Backend::new();
        b.float(0x1f94, -86.);
        b.bytes[0x1fa8] = 1;
        b.bytes[0x1fa9] = 7;
        let mut expected = b.bytes.clone();
        expected[0xc2c] = 7;
        assert_eq!(run(&mut b, 0), Ok(0));
        assert_eq!(b.bytes, expected);
        assert!(b.calls.is_empty());
    }

    #[test]
    fn update_mode_two_publishes_wheel_quaternion_and_rotation_before_axes() {
        let mut b = Backend::new();
        b.wheel();
        b.put(0xb8, 2);
        b.bytes[0x6120] = 1;
        b.put(0x70ac, u32::MAX);
        b.put(0xc68, 4);
        assert_eq!(run(&mut b, 20), Ok(2));
        assert_eq!(b.word(0x709c), 1_f32.to_bits());
        assert_eq!(b.word(0x70a0), 0);
        assert_eq!(b.word(0x70ac), 0xfffffecb);
        assert_eq!(b.word(0xc6c), 4);
        let rotation = b
            .calls
            .iter()
            .position(|x| matches!(x, VehicleUpdateEffect::RotationWheels { .. }))
            .unwrap();
        let axes = b
            .calls
            .iter()
            .position(|x| matches!(x, VehicleUpdateEffect::Axes))
            .unwrap();
        assert!(rotation < axes);
    }

    #[test]
    fn update_custom_surface_bumps_keep_rng_and_minimum_amplitude_behavior() {
        let mut b = Backend::new();
        b.wheel();
        b.put(0x1a4, 1);
        b.float(0x2c, 1.);
        b.bytes[0x13d] = 1;
        for (o, f) in [
            (0x61b8, 1.),
            (0x61bc, 1.),
            (0x61c8, 5.),
            (0x61c4, 0.2),
            (0x61a4, 2.),
            (0x61a8, 6.),
            (0x61ac, 4.),
            (0x61b0, 8.),
            (0x61b4, 0.4),
        ] {
            b.float(o, f);
        }
        b.random = vec![75, 25, 100];
        assert_eq!(run(&mut b, 16), Ok(1));
        assert!(b.random.is_empty());
        assert_eq!(b.word(0x61c0), 0.2_f32.to_bits());
        assert_eq!(b.word(0x61c8), 0);
        assert_eq!(b.word(0x61b8), 5_f32.to_bits());
        assert_eq!(b.word(0x61bc), 5_f32.to_bits());
        assert_eq!(b.word(0x61c4), (-0.2_f32).to_bits());
    }

    #[test]
    fn update_external_failure_retains_the_exact_reached_effect_prefix() {
        let mut original = Backend::new();
        original.put(0xb4, 10);
        let mut complete = original.clone();
        assert_eq!(run(&mut complete, 16), Ok(2));
        for stopped in 0..=complete.calls.len() {
            let mut b = original.clone();
            b.fail = Some(stopped);
            assert_eq!(
                run(&mut b, 16),
                Err(VehicleUpdateUnavailable::Effect(stopped as u32))
            );
            let reached = (stopped + 1).min(complete.calls.len());
            assert_eq!(b.calls, &complete.calls[..reached]);
            assert_eq!(b.word(0xb4), if stopped >= 2 { 26 } else { 10 });
            assert_eq!(b.word(0xac), 0);
        }
    }

    #[test]
    fn elapsed_conversion_rounds_the_unsigned_integer_product_once() {
        for ms in [1, 16, 1000, 16777215, 16777216, 16777217, u32::MAX] {
            let value = milliseconds_to_seconds(ms);
            assert!(value.is_normal());
        }
        // This input distinguishes native FILD/FMUL from rounding u32 first.
        assert_ne!(
            milliseconds_to_seconds(16_777_217),
            (16_777_217_u32 as f32) * 0.001_f32
        );
    }

    #[test]
    fn below_world_exit_retains_only_actual_elapsed_product_precision() {
        for (milliseconds, expected_precision) in [(1, false), (16, false), (83, true)] {
            let mut backend = Backend::new();
            backend.float(0x1f94, -86.);
            let original = backend.bytes.clone();
            let mut precision = crate::native_precision::Pc24PrecisionState::default();
            precision.run(|| assert_eq!(run(&mut backend, milliseconds), Ok(0)));
            assert_eq!(precision.precision_raised(), expected_precision);
            assert_eq!(backend.bytes, original);
            assert!(backend.calls.is_empty());
        }
        let mut backend = Backend::new();
        backend.float(0x1f94, -86.);
        let mut precision = crate::native_precision::Pc24PrecisionState::new(true);
        precision.run(|| assert_eq!(run(&mut backend, 1), Ok(0)));
        assert!(precision.precision_raised());
    }
}
