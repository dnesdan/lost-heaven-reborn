//! Addressable Update leaves. External frame operations preserve their reached
//! order; particle emission stops explicitly at an unbound live template.
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::vehicle_init::VehicleInitBackend;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameEffect {
    SetOn {
        frame: u32,
        enabled: bool,
    },
    Update {
        frame: u32,
    },
    Scale {
        frame: u32,
        scale: [f32; 3],
    },
    SetDir {
        frame: u32,
        direction: [f32; 3],
    },
    MatrixDir3 {
        frame: u32,
        direction: [f32; 3],
        up: [f32; 3],
    },
    /// The original Object mesh texture translation, conditional on subtype.
    TextureScroll {
        frame: u32,
        offset: f32,
    },
}
#[path = "vehicle_skid_effects.rs"]
mod skid;
pub use skid::{vehicle_end_skid_addressable, vehicle_wheel_skid, VehicleSkidBackend};
#[derive(Debug)]
pub enum EffectError<E> {
    Host(E),
    NumericDomain,
    ParticleTemplate { entry: u32, template: u32 },
}
type Result<T, E> = std::result::Result<T, EffectError<E>>;

struct Memory<'a, B>(&'a mut B);
impl<B: VehicleInitBackend> Memory<'_, B> {
    fn byte(&mut self, a: u32) -> Result<u8, B::Error> {
        self.0.read_byte(a).map_err(EffectError::Host)
    }
    fn word(&mut self, a: u32) -> Result<u32, B::Error> {
        self.0.read_word(a).map_err(EffectError::Host)
    }
    fn half(&mut self, a: u32) -> Result<u16, B::Error> {
        Ok(u16::from_le_bytes([self.byte(a)?, self.byte(a + 1)?]))
    }
    fn float(&mut self, a: u32) -> Result<f32, B::Error> {
        Ok(f32::from_bits(self.word(a)?))
    }
    fn put(&mut self, a: u32, n: u32) -> Result<(), B::Error> {
        self.0.write_word(a, n).map_err(EffectError::Host)
    }
    fn putfloat(&mut self, a: u32, n: f32) -> Result<(), B::Error> {
        self.put(a, n.to_bits())
    }
    fn putbyte(&mut self, a: u32, n: u8) -> Result<(), B::Error> {
        self.0.write_byte(a, n).map_err(EffectError::Host)
    }
    fn count(&mut self, header: u32, stride: u32) -> Result<(u32, u32), B::Error> {
        let base = self.word(header)?;
        if base == 0 {
            return Ok((0, 0));
        }
        let end = self.word(header + 4)?;
        let bytes = end.wrapping_sub(base) as i32;
        if bytes < 0 || !(bytes as u32).is_multiple_of(stride) {
            return Err(EffectError::NumericDomain);
        }
        Ok((base, bytes as u32 / stride))
    }
}

/// C_Vehicle::DoWindows, PC RVA0xd7ad0.
pub fn vehicle_windows<B, F>(b: &mut B, v: u32, seconds: f32, mut frame: F) -> Result<(), B::Error>
where
    B: VehicleInitBackend,
    F: FnMut(&mut B, FrameEffect) -> std::result::Result<(), B::Error>,
{
    let mut m = Memory(b);
    let (_, count) = m.count(v + 0xca4, 0x24)?;
    for i in (0..count).rev() {
        let entries = m.word(v + 0xca4)?;
        let window = m.word(entries + i * 0x24 + 0x20)?;
        if window == 0 || m.byte(window + 0x56)? == 0 {
            continue;
        }
        let glass = m.word(v + 0x218)?;
        let index = u32::from(m.half(window + 0x54)?);
        if m.byte(glass + index * 0x34 + 0xe)? & 1 != 0 {
            continue;
        }
        let step = mul(seconds, m.float(window + 0x50)?);
        if m.byte(window + 0x57)? == 0 {
            let current = add(m.float(window + 0x48)?, step);
            m.putfloat(window + 0x48, current)?;
            let object = m.word(window + 0x34)?;
            frame(
                m.0,
                FrameEffect::SetOn {
                    frame: object,
                    enabled: true,
                },
            )
            .map_err(EffectError::Host)?;
            let delta = sub(m.float(window + 0x48)?, m.float(window + 0x4c)?);
            let shift = mul(sub(1.0, add(delta, delta)), 0.1);
            let origin = m.float(window + 0x38)?;
            let x = if origin > 0.0 {
                add(shift, origin)
            } else {
                sub(origin, shift)
            };
            m.putfloat(window + 0x44, x)?;
            if m.float(window + 0x3c)? < m.float(window + 0x48)? {
                let original = m.word(window + 0x38)?;
                m.put(window + 0x44, original)?;
                let end = m.word(window + 0x3c)?;
                m.put(window + 0x48, end)?;
                m.putbyte(window + 0x56, 0)?;
            }
        } else {
            let current = sub(m.float(window + 0x48)?, step);
            m.putfloat(window + 0x48, current)?;
            let end = m.float(window + 0x4c)?;
            if end <= current {
                let delta = sub(current, end);
                let shift = mul(sub(1.0, add(delta, delta)), 0.1);
                let origin = m.float(window + 0x38)?;
                m.putfloat(
                    window + 0x44,
                    if origin > 0.0 {
                        add(shift, origin)
                    } else {
                        sub(origin, shift)
                    },
                )?;
            } else {
                let end = m.word(window + 0x4c)?;
                m.put(window + 0x48, end)?;
                m.putbyte(window + 0x56, 0)?;
                let object = m.word(window + 0x34)?;
                frame(
                    m.0,
                    FrameEffect::SetOn {
                        frame: object,
                        enabled: false,
                    },
                )
                .map_err(EffectError::Host)?;
            }
        }
        let object = m.word(window + 0x34)?;
        let flags = m.word(object + 0xac)?;
        if flags & 0x80 == 0 {
            for (destination, source) in [(0x80, 0x44), (0x84, 0x48), (0x88, 0x40)] {
                let value = m.word(window + source)?;
                m.put(object + destination, value)?;
            }
            m.put(object + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
        }
        let object = m.word(window + 0x34)?;
        frame(m.0, FrameEffect::Update { frame: object }).map_err(EffectError::Host)?;
    }
    Ok(())
}

/// Original strip geometry publication and authored texture phase, RVA0xdc170.
pub fn vehicle_strips<B, F>(b: &mut B, v: u32, seconds: f32, mut frame: F) -> Result<(), B::Error>
where
    B: VehicleInitBackend,
    F: FnMut(&mut B, FrameEffect) -> std::result::Result<(), B::Error>,
{
    let mut m = Memory(b);
    for header in [0xdc, 0xec] {
        let (_, count) = m.count(v + header, 12)?;
        for i in (0..count).rev() {
            let base = m.word(v + header)?;
            let entry = base + i * 12;
            let flags = m.half(entry + 10)?;
            if flags & 1 == 0 {
                continue;
            }
            let object = m.word(entry)?;
            let wheel_index = u32::from(m.half(entry + 8)?);
            let wheels = m.word(v + 0xc38)?;
            let first = m.word(wheels + wheel_index * 4)?;
            let endpoint = |m: &mut Memory<'_, B>, wheel: u32| -> Result<[f32; 3], B::Error> {
                Ok([
                    0.0,
                    add(
                        -add(m.float(wheel + 0x10c)?, m.float(wheel + 0x94)?),
                        m.float(wheel + 0x14)?,
                    ),
                    m.float(wheel + 0x18)?,
                ])
            };
            let (a, b) = if flags & 6 == 0 {
                let second = m.word(wheels + wheel_index * 4 + 8)?;
                (endpoint(&mut m, first)?, endpoint(&mut m, second)?)
            } else {
                (
                    [0.0, m.float(object + 0x84)?, m.float(object + 0x88)?],
                    endpoint(&mut m, first)?,
                )
            };
            let difference = [sub(a[0], b[0]), sub(a[1], b[1]), sub(a[2], b[2])];
            let length = crate::native_transform::vector_length(difference);
            let original = m.float(entry + 4)?;
            frame(
                m.0,
                FrameEffect::Scale {
                    frame: object,
                    scale: [1.0, 1.0, mul(div(1.0, original), length)],
                },
            )
            .map_err(EffectError::Host)?;
            if flags & 6 == 0 {
                let flags = m.word(object + 0xac)?;
                if flags & 0x80 == 0 {
                    m.putfloat(object + 0x84, a[1])?;
                    m.putfloat(object + 0x88, a[2])?;
                    m.put(object + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
                }
            }
            let direction = if flags & 4 != 0 {
                difference.map(|value| -value)
            } else {
                difference
            };
            let direction = normalize(direction);
            frame(
                m.0,
                FrameEffect::SetDir {
                    frame: object,
                    direction,
                },
            )
            .map_err(EffectError::Host)?;
            let base = m.word(v + header)?;
            let object = m.word(base + i * 12)?;
            frame(m.0, FrameEffect::Update { frame: object }).map_err(EffectError::Host)?;
        }
    }
    for (header, phase) in [(0xdc, 0x1c), (0xec, 0x20)] {
        let (base, count) = m.count(v + header, 12)?;
        if count == 0 {
            continue;
        }
        let idle = m.float(v + 0x5ec)?;
        let rpm = m.float(v + 0x548)?;
        let maximum = m.float(v + 0x1f9c)?;
        let mut next = add(
            mul(
                mul(div(sub(rpm, idle), sub(maximum, idle)), m.float(v + 0x18)?),
                seconds,
            ),
            m.float(v + phase)?,
        );
        m.putfloat(v + phase, next)?;
        if next > 1.0 {
            next = sub(next, next.floor());
            m.putfloat(v + phase, next)?;
        } else if next < -1.0 {
            next = sub(next, next.ceil());
            m.putfloat(v + phase, next)?;
        }
        for i in 0..count {
            let object = m.word(base + i * 12)?;
            frame(
                m.0,
                FrameEffect::TextureScroll {
                    frame: object,
                    offset: next,
                },
            )
            .map_err(EffectError::Host)?;
        }
    }
    Ok(())
}
fn normalize(mut value: [f32; 3]) -> [f32; 3] {
    let square = add(
        add(mul(value[0], value[0]), mul(value[1], value[1])),
        mul(value[2], value[2]),
    );
    if sub(square, 1.0).abs() >= 1e-8 {
        if square >= 1e-8 {
            let inverse = div(1.0, sqrt(square));
            value = value.map(|v| mul(v, inverse));
        } else {
            let i = if value[0] != 0.0 {
                0
            } else if value[2] != 0.0 {
                2
            } else {
                1
            };
            value[i] = if value[i] >= 0.0 { 1.0 } else { -1.0 };
        }
    }
    value
}

/// Motor particle guards and counter stores, PC RVA0xdd7a0. An admitted live
/// template requires the real particle backend; its absence is never success.
pub fn vehicle_motor_particles<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    milliseconds: u32,
) -> Result<bool, B::Error> {
    let mut m = Memory(b);
    if m.word(v + 0xc68)? & 0x200 == 0 {
        return Ok(true);
    }
    if m.float(v + 0x1b4)? >= m.float(v + 0x1c8)? || m.float(v + 0x59c)? >= m.float(v + 0x1d0)? {
        return Ok(true);
    }
    let mut index = m.word(v + 0x1d4)?;
    let (base, count) = m.count(0x63d8a4, 0x1c)?;
    if m.float(v + 0x1b4)? < m.float(v + 0x1cc)? {
        let extra = m.word(v + 0x1d8)?;
        if extra != 0 && extra < count {
            index = extra;
        }
    }
    if index as i32 > 0 && base != 0 && index < count {
        let template = m.word(base + index * 0x1c)?;
        if template != 0 && m.byte(v + 0x2e0)? != 0 {
            let elapsed = m.word(v + 0x1dc)?.wrapping_add(milliseconds);
            m.put(v + 0x1dc, elapsed)?;
            return Err(EffectError::ParticleTemplate {
                entry: 0x4dd7a0,
                template,
            });
        }
        return Ok(false);
    }
    Ok(true)
}

pub fn vehicle_exhaust_particles<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    milliseconds: u32,
) -> Result<(), B::Error> {
    let mut m = Memory(b);
    if m.word(v + 0xc68)? & 0x200 == 0
        || m.byte(v + 0xc2c)? == 0
        || m.float(v + 0x59c)? >= m.float(v + 0x468)?
    {
        return Ok(());
    }
    let index = m.word(v + 0x460)?;
    if index as i32 <= 0 {
        return Ok(());
    }
    let (base, count) = m.count(0x63d8a4, 0x1c)?;
    if index < count {
        let template = m.word(base + index * 0x1c)?;
        if template != 0 {
            let elapsed = m.word(v + 0x464)?.wrapping_add(milliseconds);
            m.put(v + 0x464, elapsed)?;
            return Err(EffectError::ParticleTemplate {
                entry: 0x4dee90,
                template,
            });
        }
    }
    Ok(())
}

pub fn vehicle_wheel_particles<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    milliseconds: u32,
) -> Result<(), B::Error> {
    let mut m = Memory(b);
    if m.word(v + 0xc68)? & 0x200 == 0 {
        return Ok(());
    }
    let count = m.word(v + 0x4c4)?;
    for i in (0..count).rev() {
        let wheels = m.word(v + 0xc38)?;
        let wheel = m.word(wheels + i * 4)?;
        let flags = m.word(wheel + 0x120)?;
        if flags & 0x4000_0000 != 0 || flags & 8 == 0 {
            continue;
        }
        let material = m.word(wheel + 0xb4)?;
        let map = u32::from(m.byte(0x63d8b0 + material)?);
        let materials = m.word(0x63d9b4)?;
        let index = m.word(materials + map * 0x30 + 0x24)?;
        if index != 0 && flags & 0x10000a0 != 0 {
            let (base, total) = m.count(0x63d8a4, 0x1c)?;
            if index < total {
                let template = m.word(base + index * 0x1c)?;
                if template != 0 {
                    let x = m.float(wheel + 0x4c)?;
                    let y = m.float(wheel + 0x50)?;
                    let z = m.float(wheel + 0x54)?;
                    if add(add(mul(x, x), mul(y, y)), mul(z, z)) > 1.0 || flags & 0x80 != 0 {
                        let elapsed = add_milliseconds(milliseconds, m.float(wheel + 0x1cc)?);
                        m.putfloat(wheel + 0x1cc, elapsed)?;
                        return Err(EffectError::ParticleTemplate {
                            entry: 0x4de500,
                            template,
                        });
                    }
                }
            }
        }
        let index = m.word(materials + map * 0x30 + 0x20)?;
        if index != 0 && m.float(v + 0x59c)? > 3.0 {
            let (base, total) = m.count(0x63d8a4, 0x1c)?;
            if index < total {
                let template = m.word(base + index * 0x1c)?;
                if template != 0 {
                    let elapsed = add_milliseconds(milliseconds, m.float(wheel + 0x1cc)?);
                    m.putfloat(wheel + 0x1cc, elapsed)?;
                    return Err(EffectError::ParticleTemplate {
                        entry: 0x4de500,
                        template,
                    });
                }
            }
        }
    }
    Ok(())
}

fn add_milliseconds(milliseconds: u32, previous: f32) -> f32 {
    let exact = f64::from(milliseconds) + f64::from(previous);
    let rounded = exact as f32;
    crate::native_precision::record_precision(f64::from(rounded) != exact);
    rounded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;
    fn memory() -> VehicleFrameMemory {
        let mut m = VehicleFrameMemory::default();
        for (base, length) in [
            (0x1000, 0x3000),
            (0x4000, 0x100),
            (0x5000, 0x100),
            (0x6000, 0x200),
            (0x7000, 0x100),
            (0x8000, 0x100),
            (0x9000, 0x200),
            (0xa000, 0x200),
        ] {
            m.map_bytes(base, &vec![0; length]).unwrap();
        }
        m
    }
    fn put(m: &mut VehicleFrameMemory, a: u32, v: f32) {
        m.write_word(a, v.to_bits()).unwrap();
    }
    #[test]
    fn windows_move_and_close_through_actual_frame_effect_order() {
        let mut m = memory();
        m.write_word(0x1000 + 0xca4, 0x4000).unwrap();
        m.write_word(0x1000 + 0xca8, 0x4024).unwrap();
        m.write_word(0x4020, 0x5000).unwrap();
        m.write_word(0x1000 + 0x218, 0x7000).unwrap();
        m.write_word(0x5034, 0x6000).unwrap();
        for (a, v) in [(0x5038, 1.0), (0x503c, 1.0), (0x5040, 3.0), (0x5050, 1.0)] {
            put(&mut m, a, v);
        }
        m.write_byte(0x5056, 1).unwrap();
        let mut effects = Vec::new();
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision
            .run(|| {
                vehicle_windows(&mut m, 0x1000, 0.25, |b, e| {
                    effects.push(e);
                    if let FrameEffect::SetOn { frame, enabled } = e {
                        let flags = b.read_word(frame + 0xac)?;
                        b.write_word(frame + 0xac, if enabled { flags | 1 } else { flags & !1 })?;
                    }
                    Ok(())
                })
            })
            .unwrap();
        assert_eq!(
            effects,
            [
                FrameEffect::SetOn {
                    frame: 0x6000,
                    enabled: true
                },
                FrameEffect::Update { frame: 0x6000 }
            ]
        );
        assert_eq!(f32::from_bits(m.read_word(0x6084).unwrap()), 0.25);
        assert!((f32::from_bits(m.read_word(0x6080).unwrap()) - 1.05).abs() < 1e-6);
        assert!(precision.precision_raised());
        m.write_byte(0x5057, 1).unwrap();
        effects.clear();
        vehicle_windows(&mut m, 0x1000, 0.5, |_, e| {
            effects.push(e);
            Ok(())
        })
        .unwrap();
        assert_eq!(m.read_byte(0x5056).unwrap(), 0);
        assert_eq!(m.read_word(0x6084).unwrap(), 0);
        assert_eq!(
            effects[0],
            FrameEffect::SetOn {
                frame: 0x6000,
                enabled: false
            }
        );
    }
    #[test]
    fn strip_pair_uses_real_wheel_endpoints_before_texture_phase() {
        let mut m = memory();
        m.write_word(0x1000 + 0xdc, 0x4000).unwrap();
        m.write_word(0x1000 + 0xe0, 0x400c).unwrap();
        m.write_word(0x4000, 0x6000).unwrap();
        put(&mut m, 0x4004, 2.0);
        m.write_byte(0x400a, 1).unwrap();
        m.write_word(0x1000 + 0xc38, 0x8000).unwrap();
        m.write_word(0x8000, 0x9000).unwrap();
        m.write_word(0x8008, 0xa000).unwrap();
        put(&mut m, 0x9014, 2.0);
        put(&mut m, 0x6080, 7.0);
        for (o, v) in [(0x548, 50.0), (0x1f9c, 100.0), (0x18, 1.0)] {
            put(&mut m, 0x1000 + o, v);
        }
        let mut effects = Vec::new();
        vehicle_strips(&mut m, 0x1000, 0.1, |_, e| {
            effects.push(e);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            effects[0],
            FrameEffect::Scale {
                frame: 0x6000,
                scale: [1.0, 1.0, 1.0]
            }
        );
        assert_eq!(
            effects[1],
            FrameEffect::SetDir {
                frame: 0x6000,
                direction: [0.0, 1.0, 0.0]
            }
        );
        assert_eq!(effects[2], FrameEffect::Update { frame: 0x6000 });
        assert_eq!(
            effects[3],
            FrameEffect::TextureScroll {
                frame: 0x6000,
                offset: 0.05
            }
        );
        assert_eq!(f32::from_bits(m.read_word(0x6080).unwrap()), 7.0);
        assert_eq!(f32::from_bits(m.read_word(0x6084).unwrap()), 2.0);
    }
    #[test]
    fn particle_empty_manager_and_live_template_failure_are_distinct() {
        let mut m = memory();
        m.map_bytes(0x63d8a4, &[0; 8]).unwrap();
        m.write_word(0x1000 + 0xc68, 0x200).unwrap();
        put(&mut m, 0x1000 + 0x1c8, 1.0);
        put(&mut m, 0x1000 + 0x1d0, 1.0);
        m.write_word(0x1000 + 0x1d4, 1).unwrap();
        m.write_byte(0x1000 + 0x2e0, 1).unwrap();
        assert!(vehicle_motor_particles(&mut m, 0x1000, 16).unwrap());
        assert_eq!(m.read_word(0x1000 + 0x1dc).unwrap(), 0);
        m.write_word(0x63d8a4, 0x7000).unwrap();
        m.write_word(0x63d8a8, 0x7038).unwrap();
        m.write_word(0x701c, 0x6000).unwrap();
        assert!(matches!(
            vehicle_motor_particles(&mut m, 0x1000, 16),
            Err(EffectError::ParticleTemplate {
                entry: 0x4dd7a0,
                template: 0x6000
            })
        ));
        assert_eq!(m.read_word(0x1000 + 0x1dc).unwrap(), 16);
    }
}
