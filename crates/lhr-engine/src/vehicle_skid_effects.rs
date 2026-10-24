//! C_Vehicle::Slaif (PC0xd34b0) and SlaifEnd (0xd4c90). The paired models,
//! wheel edge history and ring indices are the caller's actual live records.
use super::*;
pub trait VehicleSkidBackend: VehicleInitBackend {
    /// Original first word of the contacted primitive. A typed world owner
    /// resolves its stable collision identity without casting it to an address.
    fn skid_primitive_properties(&mut self, surface: u32) -> std::result::Result<u32, Self::Error> {
        self.read_word(surface)
    }
    fn skid_frame_effect(&mut self, effect: FrameEffect) -> std::result::Result<(), Self::Error>;
}
fn vector<B: VehicleInitBackend>(
    m: &mut Memory<'_, B>,
    address: u32,
) -> Result<[f32; 3], B::Error> {
    Ok([
        m.float(address)?,
        m.float(address + 4)?,
        m.float(address + 8)?,
    ])
}
fn put_vector<B: VehicleInitBackend>(
    m: &mut Memory<'_, B>,
    address: u32,
    value: [f32; 3],
) -> Result<(), B::Error> {
    for (i, value) in value.into_iter().enumerate() {
        m.putfloat(address + i as u32 * 4, value)?;
    }
    Ok(())
}
fn plus(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| add(a[i], b[i]))
}
fn minus(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| sub(a[i], b[i]))
}
fn times(a: [f32; 3], factor: f32) -> [f32; 3] {
    a.map(|v| mul(v, factor))
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        sub(mul(a[1], b[2]), mul(a[2], b[1])),
        sub(mul(a[2], b[0]), mul(a[0], b[2])),
        sub(mul(a[0], b[1]), mul(a[1], b[0])),
    ]
}
fn squared_order(value: [f32; 3], order: [usize; 3]) -> f32 {
    add(
        add(
            mul(value[order[0]], value[order[0]]),
            mul(value[order[1]], value[order[1]]),
        ),
        mul(value[order[2]], value[order[2]]),
    )
}
fn normalized_order(mut value: [f32; 3], order: [usize; 3]) -> [f32; 3] {
    let square = squared_order(value, order);
    if f64::from(sub(square, 1.0).abs()) < 1e-8 {
        return value;
    }
    if square >= 1e-8 {
        let inverse = div(1.0, sqrt(square));
        value = times(value, inverse);
    } else {
        let index = if value[0] != 0.0 {
            0
        } else if value[2] != 0.0 {
            2
        } else {
            1
        };
        value[index] = if value[index] >= 0.0 { 1.0 } else { -1.0 };
    }
    value
}
fn length(value: [f32; 3]) -> f32 {
    let square = squared_order(value, [0, 1, 2]);
    if (square.to_bits() as i32) < 0x322bcc77 {
        0.0
    } else if f64::from(sub(square, 1.0).abs()) < 1e-8 {
        1.0
    } else {
        sqrt(square)
    }
}
fn frame<B: VehicleSkidBackend>(
    m: &mut Memory<'_, B>,
    effect: FrameEffect,
) -> Result<(), B::Error> {
    m.0.skid_frame_effect(effect).map_err(EffectError::Host)
}
fn position<B: VehicleInitBackend>(
    m: &mut Memory<'_, B>,
    object: u32,
    value: [f32; 3],
) -> Result<(), B::Error> {
    if m.byte(object + 0xac)? & 0x80 == 0 {
        put_vector(m, object + 0x80, value)?;
        let flags = m.word(object + 0xac)?;
        m.put(object + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
    }
    Ok(())
}
fn matrix_direction<B: VehicleSkidBackend>(
    m: &mut Memory<'_, B>,
    object: u32,
    direction: [f32; 3],
    up: [f32; 3],
) -> Result<(), B::Error> {
    frame(
        m,
        FrameEffect::MatrixDir3 {
            frame: object,
            direction,
            up,
        },
    )?;
    let flags = m.word(object + 0xac)?;
    m.put(object + 0xac, (flags & 0xffff_fed3) | 0x4000_0010)
}
fn current_point<B: VehicleInitBackend>(
    m: &mut Memory<'_, B>,
    wheel: u32,
) -> Result<[f32; 3], B::Error> {
    let normal = vector(m, wheel + 0x40)?;
    let contact = vector(m, wheel + 0x28)?;
    Ok(plus(times(normal, 0.05), contact))
}
fn pair<B: VehicleInitBackend>(
    m: &mut Memory<'_, B>,
    wheel: u32,
    index: u32,
) -> Result<u32, B::Error> {
    Ok(m.word(wheel + 0x12c)? + index * 0x14)
}
fn pair_word<B: VehicleInitBackend>(
    m: &mut Memory<'_, B>,
    wheel: u32,
    index: u32,
    offset: u32,
) -> Result<u32, B::Error> {
    let entry = pair(m, wheel, index)?;
    m.word(entry + offset)
}
fn end<B: VehicleInitBackend>(m: &mut Memory<'_, B>, wheel: u32) -> Result<(), B::Error> {
    let (_, count) = m.count(wheel + 0x12c, 0x14)?;
    let index = m.word(wheel + 0x16c)?;
    if index < count {
        let entry = pair(m, wheel, index)?;
        if m.byte(entry + 0x10)? != 0 {
            m.putbyte(entry + 0x10, 0)?;
            let next = m.word(wheel + 0x16c)?.wrapping_add(1);
            m.put(wheel + 0x16c, next)?;
            let (_, count) = m.count(wheel + 0x12c, 0x14)?;
            if next >= count {
                m.put(wheel + 0x16c, 0)?;
            }
        }
    }
    Ok(())
}
pub fn vehicle_end_skid_addressable<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    index: u32,
) -> Result<(), B::Error> {
    let mut m = Memory(b);
    let wheels = m.word(v + 0xc38)?;
    let wheel = m.word(wheels + index * 4)?;
    end(&mut m, wheel)
}

pub fn vehicle_wheel_skid<B: VehicleSkidBackend>(
    b: &mut B,
    v: u32,
    index: u32,
) -> Result<(), B::Error> {
    let mut m = Memory(b);
    // The original tail recurses after reversal and every completed 0.3m
    // segment. Iteration keeps that same live reread boundary without growing
    // the Rust stack; the original five-metre discontinuity check is retained.
    loop {
        let wheels = m.word(v + 0xc38)?;
        let wheel = m.word(wheels + index * 4)?;
        if m.byte(wheel + 0x120)? & 8 == 0 {
            return end(&mut m, wheel);
        }
        let material = m.word(wheel + 0xb4)?;
        let material_index = u32::from(m.byte(0x63d8b0 + material)?);
        let materials = m.word(0x63d9b4)?;
        if m.byte(materials + material_index * 0x30 + 0x2d)? == 0 {
            return end(&mut m, wheel);
        }
        let surface = m.word(wheel + 0x124)?;
        if surface == 0
            || m.0
                .skid_primitive_properties(surface)
                .map_err(EffectError::Host)?
                & 0x10000
                != 0
        {
            return end(&mut m, wheel);
        }
        let (_, count) = m.count(wheel + 0x12c, 0x14)?;
        let active = m.word(wheel + 0x16c)?;
        if active >= count {
            return Ok(());
        }
        let previous = if active == 0 { count - 1 } else { active - 1 };
        let mut entry = pair(&mut m, wheel, active)?;
        if m.byte(entry + 0x10)? == 0 {
            let object = m.word(entry)?;
            frame(
                &mut m,
                FrameEffect::SetOn {
                    frame: object,
                    enabled: true,
                },
            )?;
        }
        entry = pair(&mut m, wheel, active)?;
        if m.byte(entry + 0x10)? == 0 {
            m.putbyte(entry + 0x10, 1)?;
            let start = current_point(&mut m, wheel)?;
            put_vector(&mut m, wheel + 0x160, start)?;
            let object = pair_word(&mut m, wheel, active, 0)?;
            position(&mut m, object, start)?;
            let child = pair_word(&mut m, wheel, active, 4)?;
            let width = m.float(wheel + 0x138)?;
            frame(
                &mut m,
                FrameEffect::Scale {
                    frame: child,
                    scale: [width, 1.0, 0.01],
                },
            )?;
            let object = pair_word(&mut m, wheel, active, 0)?;
            frame(&mut m, FrameEffect::Update { frame: object })?;
            let entry = pair(&mut m, wheel, active)?;
            m.putbyte(entry + 0x11, 1)?;
            let velocity = vector(&mut m, wheel + 0x4c)?;
            put_vector(&mut m, wheel + 0x154, velocity)?;
            let direction = normalized_order(velocity, [0, 1, 2]);
            put_vector(&mut m, wheel + 0x154, direction)?;
            return Ok(());
        }
        let mut start = vector(&mut m, wheel + 0x160)?;
        let contact = current_point(&mut m, wheel)?;
        let delta = minus(contact, start);
        let square = squared_order(delta, [0, 1, 2]);
        if (square.to_bits() as i32) < 0x322bcc77 {
            return Ok(());
        }
        let mut distance = if f64::from(sub(square, 1.0).abs()) < 1e-8 {
            1.0
        } else {
            let distance = sqrt(square);
            if distance < 0.05 {
                return Ok(());
            }
            if distance > 5.0 {
                return end(&mut m, wheel);
            }
            distance
        };
        let mut direction = normalized_order(delta, [2, 1, 0]);
        let velocity = vector(&mut m, wheel + 0x4c)?;
        let old_direction = vector(&mut m, wheel + 0x154)?;
        let motion = add(
            add(
                mul(velocity[1], old_direction[1]),
                mul(velocity[2], old_direction[2]),
            ),
            mul(velocity[0], old_direction[0]),
        );
        if motion < 0.0 {
            end(&mut m, wheel)?;
            continue;
        }
        let entry = pair(&mut m, wheel, active)?;
        if m.byte(entry + 0x11)? == 0 {
            let up = vector(&mut m, wheel + 0x40)?;
            let side = cross(direction, up);
            let left = vector(&mut m, wheel + 0x13c)?;
            let right = vector(&mut m, wheel + 0x148)?;
            let old_side_raw = minus(left, right);
            let old_width = length(old_side_raw);
            let old_side = normalized_order(old_side_raw, [1, 2, 0]);
            let turn = add(
                add(
                    mul(old_side[2], direction[2]),
                    mul(old_side[0], direction[0]),
                ),
                mul(old_side[1], direction[1]),
            );
            let width = m.float(wheel + 0x138)?;
            let half = mul(width, 0.5);
            let (origin, corner_direction, gap) = if turn >= 0.0 {
                start = minus(left, times(side, half));
                (
                    left,
                    plus(side, old_side).map(|v| -v),
                    minus(right, minus(left, times(side, width))),
                )
            } else {
                start = plus(right, times(side, half));
                (
                    right,
                    plus(side, old_side),
                    minus(left, plus(right, times(side, width))),
                )
            };
            let half_gap = mul(length(gap), 0.5);
            let depth = sqrt(sub(mul(old_width, old_width), mul(half_gap, half_gap)));
            let old_entry = pair(&mut m, wheel, previous)?;
            if m.byte(old_entry + 0x12)? == 0 {
                let object = m.word(old_entry + 8)?;
                frame(
                    &mut m,
                    FrameEffect::SetOn {
                        frame: object,
                        enabled: true,
                    },
                )?;
                let old_entry = pair(&mut m, wheel, previous)?;
                m.putbyte(old_entry + 0x12, 1)?;
            }
            let old_entry = pair(&mut m, wheel, previous)?;
            let object = m.word(old_entry + 8)?;
            position(&mut m, object, origin)?;
            matrix_direction(&mut m, object, corner_direction, up)?;
            let child = pair_word(&mut m, wheel, previous, 12)?;
            frame(
                &mut m,
                FrameEffect::Scale {
                    frame: child,
                    scale: [add(half_gap, half_gap), 1.0, depth],
                },
            )?;
            let object = pair_word(&mut m, wheel, previous, 8)?;
            frame(&mut m, FrameEffect::Update { frame: object })?;
            let delta = minus(current_point(&mut m, wheel)?, start);
            distance = length(delta);
            direction = normalized_order(delta, [2, 1, 0]);
        }
        let up = vector(&mut m, wheel + 0x40)?;
        if distance <= 0.3 {
            let object = pair_word(&mut m, wheel, active, 0)?;
            position(&mut m, object, start)?;
            matrix_direction(&mut m, object, direction, up)?;
            let width = m.float(wheel + 0x138)?;
            let child = pair_word(&mut m, wheel, active, 4)?;
            frame(
                &mut m,
                FrameEffect::Scale {
                    frame: child,
                    scale: [width, 1.0, distance],
                },
            )?;
            let object = pair_word(&mut m, wheel, active, 0)?;
            frame(&mut m, FrameEffect::Update { frame: object })?;
            return Ok(());
        }
        let width = m.float(wheel + 0x138)?;
        let side = times(
            normalized_order(cross(direction, up), [0, 1, 2]),
            mul(width, 0.5),
        );
        put_vector(&mut m, wheel + 0x13c, side)?;
        put_vector(&mut m, wheel + 0x148, side.map(|v| -v))?;
        let object = pair_word(&mut m, wheel, active, 0)?;
        position(&mut m, object, start)?;
        matrix_direction(&mut m, object, direction, up)?;
        let child = pair_word(&mut m, wheel, active, 4)?;
        frame(
            &mut m,
            FrameEffect::Scale {
                frame: child,
                scale: [width, 1.0, 0.3],
            },
        )?;
        let object = pair_word(&mut m, wheel, active, 0)?;
        frame(&mut m, FrameEffect::Update { frame: object })?;
        let entry = pair(&mut m, wheel, active)?;
        m.putbyte(entry + 0x10, 0)?;
        let entry = pair(&mut m, wheel, active)?;
        m.putbyte(entry + 0x11, 0)?;
        let (_, count) = m.count(wheel + 0x12c, 0x14)?;
        let next = if active.wrapping_add(1) < count {
            active + 1
        } else {
            0
        };
        m.put(wheel + 0x16c, next)?;
        start = plus(start, times(direction, 0.3));
        let left = plus(start, vector(&mut m, wheel + 0x13c)?);
        let right = plus(start, vector(&mut m, wheel + 0x148)?);
        put_vector(&mut m, wheel + 0x13c, left)?;
        put_vector(&mut m, wheel + 0x148, right)?;
        put_vector(&mut m, wheel + 0x160, start)?;
        let object = pair_word(&mut m, wheel, next, 0)?;
        position(&mut m, object, start)?;
        let child = pair_word(&mut m, wheel, next, 4)?;
        frame(
            &mut m,
            FrameEffect::Scale {
                frame: child,
                scale: [width, 1.0, 0.01],
            },
        )?;
        let object = pair_word(&mut m, wheel, next, 0)?;
        frame(&mut m, FrameEffect::Update { frame: object })?;
        let entry = pair(&mut m, wheel, next)?;
        if m.byte(entry + 0x10)? == 0 {
            let object = m.word(entry)?;
            frame(
                &mut m,
                FrameEffect::SetOn {
                    frame: object,
                    enabled: true,
                },
            )?;
        }
        let entry = pair(&mut m, wheel, next)?;
        m.putbyte(entry + 0x10, 1)?;
        let entry = pair(&mut m, wheel, next)?;
        m.putbyte(entry + 0x11, 0)?;
        let velocity = vector(&mut m, wheel + 0x4c)?;
        put_vector(&mut m, wheel + 0x154, velocity)?;
        put_vector(&mut m, wheel + 0x154, normalized_order(velocity, [0, 1, 2]))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        frame_transform::{FrameKey, I3dFrameState},
        model_loading::ModelArena,
        vehicle_frame_backend::{
            VehicleFrameBackend, VehicleFrameError, VehicleFrameMemory, VehicleMemoryError,
        },
        vehicle_init::{
            VehicleAxisUpdateBackend, VehicleInitEnumeration, VehicleInitFrameVector,
            VehicleInitQuery, VehicleInitUnavailable,
        },
    };
    struct Owner {
        arena: ModelArena,
        memory: VehicleFrameMemory,
        events: Vec<FrameEffect>,
    }
    type Error = VehicleFrameError<VehicleMemoryError>;
    impl Owner {
        fn frame<T>(
            &mut self,
            action: impl FnOnce(
                &mut VehicleFrameBackend<'_, VehicleFrameMemory>,
            ) -> std::result::Result<T, Error>,
        ) -> std::result::Result<T, Error> {
            action(&mut VehicleFrameBackend::new(
                &mut self.arena,
                &mut self.memory,
            ))
        }
        fn new() -> Self {
            let mut out = Self {
                arena: ModelArena::default(),
                memory: VehicleFrameMemory::default(),
                events: Vec::new(),
            };
            for (base, length) in [
                (0x1000, 0x3000),
                (0x4000, 0x200),
                (0x5000, 4),
                (0x6000, 40),
                (0x8000, 4),
                (0x9000, 0x30),
            ] {
                out.memory.map_bytes(base, &vec![0; length]).unwrap();
            }
            out.memory.map_bytes(0x63d8b0, &[0]).unwrap();
            out.memory
                .map_bytes(0x63d9b4, &0x9000u32.to_le_bytes())
                .unwrap();
            out.memory.write_byte(0x902d, 1).unwrap();
            for (a, v) in [
                (0x1000 + 0xc38, 0x5000),
                (0x5000, 0x4000),
                (0x4120, 8),
                (0x4124, 0x8000),
                (0x412c, 0x6000),
                (0x4130, 0x6028),
            ] {
                out.memory.write_word(a, v).unwrap();
            }
            for (a, v) in [(0x4044, 1.0f32), (0x4054, 10.0), (0x4138, 0.3)] {
                out.memory.write_word(a, v.to_bits()).unwrap();
            }
            for pair_index in 0..2 {
                for kind in 0..2 {
                    let model = out.arena.create_model().unwrap();
                    let child = out
                        .arena
                        .insert(I3dFrameState::dummy(), None, Vec::new(), Vec::new(), None)
                        .unwrap();
                    out.arena.graph.link_to(child, Some(model.root())).unwrap();
                    let model_address = out
                        .frame(|frames| frames.frame_address(model.root()))
                        .unwrap();
                    let child_address = out.frame(|frames| frames.frame_address(child)).unwrap();
                    let offset = 0x6000 + pair_index * 20 + kind * 8;
                    out.memory.write_word(offset, model_address).unwrap();
                    out.memory.write_word(offset + 4, child_address).unwrap();
                }
            }
            out
        }
        fn contact(&mut self, z: f32) {
            self.memory.write_word(0x4030, z.to_bits()).unwrap();
        }
    }
    impl VehicleInitBackend for Owner {
        type Error = Error;
        fn read_byte(&mut self, a: u32) -> std::result::Result<u8, Error> {
            self.frame(|b| b.read_byte(a))
        }
        fn read_word(&mut self, a: u32) -> std::result::Result<u32, Error> {
            self.frame(|b| b.read_word(a))
        }
        fn write_byte(&mut self, a: u32, v: u8) -> std::result::Result<(), Error> {
            self.frame(|b| b.write_byte(a, v))
        }
        fn write_word(&mut self, a: u32, v: u32) -> std::result::Result<(), Error> {
            self.frame(|b| b.write_word(a, v))
        }
        fn free(&mut self, a: u32) -> std::result::Result<(), Error> {
            self.frame(|b| b.free(a))
        }
        fn allocate(&mut self, n: u32) -> std::result::Result<u32, Error> {
            self.frame(|b| b.allocate(n))
        }
        fn collect_frame(
            &mut self,
            v: &mut VehicleInitFrameVector,
            f: u32,
        ) -> std::result::Result<(), Error> {
            self.frame(|b| b.collect_frame(v, f))
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> std::result::Result<(), VehicleInitUnavailable<Error>> {
            Err(VehicleInitUnavailable::Effect(
                VehicleFrameError::UnavailableFrameMethod("synthetic enumeration"),
            ))
        }
    }
    impl VehicleSkidBackend for Owner {
        fn skid_frame_effect(&mut self, e: FrameEffect) -> std::result::Result<(), Error> {
            self.events.push(e);
            self.frame(|frames| match e {
                FrameEffect::SetOn { frame, enabled } => {
                    frames.set_axis_frame_on(frame, u32::from(enabled))
                }
                FrameEffect::Update { frame } => {
                    let key = frames.frame_key(frame)?;
                    frames
                        .arena_mut()
                        .graph
                        .update(key)
                        .map_err(crate::model_loading::ModelError::from)
                        .map_err(Into::into)
                }
                FrameEffect::Scale { frame, scale } => {
                    let key = frames.frame_key(frame)?;
                    frames
                        .arena_mut()
                        .graph
                        .assign_scale(key, scale)
                        .map_err(crate::model_loading::ModelError::from)
                        .map_err(Into::into)
                }
                FrameEffect::MatrixDir3 {
                    frame,
                    direction,
                    up,
                } => frames.set_axis_matrix_direction(
                    frame,
                    direction.map(f32::to_bits),
                    up.map(f32::to_bits),
                ),
                _ => unreachable!(),
            })
        }
    }
    #[test]
    fn starts_extends_wraps_paired_models_and_ends_without_replacing_live_history() {
        let mut owner = Owner::new();
        vehicle_wheel_skid(&mut owner, 0x1000, 0).unwrap();
        assert_eq!(owner.read_byte(0x6010).unwrap(), 1);
        assert_eq!(owner.read_byte(0x6011).unwrap(), 1);
        assert_eq!(owner.read_word(0x4168).unwrap(), 0);
        owner.contact(0.2);
        owner.events.clear();
        vehicle_wheel_skid(&mut owner, 0x1000, 0).unwrap();
        assert!(owner
            .events
            .iter()
            .any(|e| matches!(e,FrameEffect::Scale{scale,..}if scale[2]==0.2)));
        owner.contact(1.0);
        owner.events.clear();
        vehicle_wheel_skid(&mut owner, 0x1000, 0).unwrap();
        assert!(owner
            .events
            .iter()
            .any(|e| matches!(e,FrameEffect::Scale{scale,..}if scale[2]==0.3)));
        assert!(owner
            .events
            .iter()
            .any(|e| matches!(e,FrameEffect::MatrixDir3{direction,..}if direction[0]!=0.0)));
        assert!(f32::from_bits(owner.read_word(0x4168).unwrap()) >= 0.89);
        assert!(owner.read_word(0x416c).unwrap() < 2);
        let active = owner.read_word(0x416c).unwrap();
        vehicle_end_skid_addressable(&mut owner, 0x1000, 0).unwrap();
        assert_eq!(owner.read_byte(0x6000 + active * 20 + 0x10).unwrap(), 0);
        assert!(owner.arena.frame(FrameKey(1)).is_ok());
    }
    #[test]
    fn reverse_and_discontinuity_finish_the_current_ring_segment() {
        let mut owner = Owner::new();
        vehicle_wheel_skid(&mut owner, 0x1000, 0).unwrap();
        owner.contact(0.2);
        vehicle_wheel_skid(&mut owner, 0x1000, 0).unwrap();
        owner.memory.write_word(0x4054, (-10f32).to_bits()).unwrap();
        owner.contact(0.1);
        vehicle_wheel_skid(&mut owner, 0x1000, 0).unwrap();
        assert_eq!(owner.read_word(0x416c).unwrap(), 1);
        assert_eq!(owner.read_byte(0x6024).unwrap(), 1);
        owner.contact(10.0);
        vehicle_wheel_skid(&mut owner, 0x1000, 0).unwrap();
        assert_eq!(owner.read_byte(0x6024).unwrap(), 0);
    }
}
