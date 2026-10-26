//! Original wheel steering pose update over borrowed vehicle storage. Quaternion
//! construction/normalization and external wheel/frame storage are explicit
//! dependencies; no host kinematics or substitute wheel transforms are used.
use crate::native_transform::SQuat;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleWheelPoseUnavailable<E> {
    Storage(u32),
    NumericDomain,
    Effect(E),
}

type Out<T, E> = Result<T, VehicleWheelPoseUnavailable<E>>;

pub trait VehicleWheelPoseBackend {
    type Error;
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error>;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error>;
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error>;
    fn make_axis_angle(&mut self, axis: [f32; 3], angle: f32) -> Result<SQuat, Self::Error>;
    /// Original quaternion Normalize on the already written frame+0x9c.
    fn normalize_frame_rotation(&mut self, frame: u32) -> Result<(), Self::Error>;
    /// Original S_matrix::SetRot3 at frame+0x50 from its normalized quaternion.
    fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error>;
}

fn number<E>(v: f32) -> Out<f32, E> {
    if v.is_normal() || v == 0.0 {
        Ok(v)
    } else {
        Err(VehicleWheelPoseUnavailable::NumericDomain)
    }
}
fn arithmetic<E>(v: f32, wide: f64) -> Out<f32, E> {
    if !v.is_finite()
        || wide.abs() > f32::MAX as f64
        || (wide != 0.0 && wide.abs() < f32::MIN_POSITIVE as f64)
    {
        Err(VehicleWheelPoseUnavailable::NumericDomain)
    } else {
        Ok(v)
    }
}
fn add<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    arithmetic(crate::native_precision::add(a, b), a as f64 + b as f64)
}
fn sub<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    arithmetic(crate::native_precision::sub(a, b), a as f64 - b as f64)
}
fn mul<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    arithmetic(crate::native_precision::mul(a, b), a as f64 * b as f64)
}
fn div<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    arithmetic(crate::native_precision::div(a, b), a as f64 / b as f64)
}

fn compose_rotation<E>(first: SQuat, second: SQuat) -> Out<SQuat, E> {
    let [w, x, y, z] = first.wxyz;
    let [rw, rx, ry, rz] = second.wxyz;
    let dot = add(add(mul(y, ry)?, mul(z, rz)?)?, mul(x, rx)?)?;
    Ok(SQuat {
        wxyz: [
            sub(mul(rw, w)?, dot)?,
            add(
                add(mul(rx, w)?, mul(x, rw)?)?,
                sub(mul(y, rz)?, mul(ry, z)?)?,
            )?,
            add(
                add(mul(ry, w)?, mul(y, rw)?)?,
                sub(mul(rx, z)?, mul(x, rz)?)?,
            )?,
            add(
                add(mul(rz, w)?, mul(z, rw)?)?,
                sub(mul(x, ry)?, mul(rx, y)?)?,
            )?,
        ],
    })
}
fn normalized_basis<E>(v: [f32; 3]) -> Out<[f32; 3], E> {
    let [x, y, z] = v;
    let squared = add(add(mul(y, y)?, mul(z, z)?)?, mul(x, x)?)?;
    if f64::from(sub(squared, 1.0)?.abs()) < f64::from(1e-8_f32) {
        return Ok(v);
    }
    if squared >= 1e-8_f32 {
        let root = arithmetic(
            crate::native_precision::sqrt(squared),
            f64::from(squared).sqrt(),
        )?;
        let reciprocal = div(1.0, root)?;
        return Ok([
            mul(x, reciprocal)?,
            mul(y, reciprocal)?,
            mul(z, reciprocal)?,
        ]);
    }
    let mut v = v;
    let index = if x != 0.0 {
        0
    } else if z != 0.0 {
        2
    } else {
        1
    };
    v[index] = if v[index] < 0.0 { -1.0 } else { 1.0 };
    Ok(v)
}

/// Publishes to the frame resolved by the caller after its quaternion callbacks.
/// Normalization may change flags, so their read must follow that effect.
fn publish_frame_rotation<B: VehicleWheelPoseBackend>(
    backend: &mut B,
    frame: u32,
    rotation: SQuat,
) -> Out<(), B::Error> {
    use VehicleWheelPoseUnavailable::Effect;
    for (index, value) in rotation.wxyz.into_iter().enumerate() {
        backend
            .write_word(frame.wrapping_add(0x9c + index as u32 * 4), value.to_bits())
            .map_err(Effect)?;
    }
    backend.normalize_frame_rotation(frame).map_err(Effect)?;
    let flags = backend
        .read_word(frame.wrapping_add(0xac))
        .map_err(Effect)?;
    backend
        .write_word(
            frame.wrapping_add(0xac),
            (flags & 0xffff_fecb) | 0x4000_0008,
        )
        .map_err(Effect)
}

pub struct VehicleWheelPoseState<'a> {
    bytes: &'a mut [u8],
}
impl<'a> VehicleWheelPoseState<'a> {
    pub fn from_bytes(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }
    fn word<E>(&self, offset: u32) -> Out<u32, E> {
        let p = offset as usize;
        let v = self
            .bytes
            .get(p..p + 4)
            .ok_or(VehicleWheelPoseUnavailable::Storage(offset))?;
        Ok(u32::from_le_bytes(v.try_into().unwrap()))
    }
    fn byte<E>(&self, offset: u32) -> Out<u8, E> {
        self.bytes
            .get(offset as usize)
            .copied()
            .ok_or(VehicleWheelPoseUnavailable::Storage(offset))
    }
    fn float<E>(&self, offset: u32) -> Out<f32, E> {
        Ok(f32::from_bits(self.word(offset)?))
    }
    fn store<E>(&mut self, offset: u32, value: u32) -> Out<(), E> {
        let p = offset as usize;
        self.bytes
            .get_mut(p..p + 4)
            .ok_or(VehicleWheelPoseUnavailable::Storage(offset))?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    fn store_float<E>(&mut self, offset: u32, value: f32) -> Out<(), E> {
        self.store(offset, value.to_bits())
    }

    /// C_Vehicle::UpdateSteeringWheels, PC canonical RVA ddc60. Arithmetic is
    /// bounded to masked x87 PC24/RN normal-or-zero intermediates; completed
    /// storage and external effects remain visible on later unavailability.
    pub fn update_steering_wheels<B: VehicleWheelPoseBackend>(
        &mut self,
        dt: f32,
        backend: &mut B,
    ) -> Out<(), B::Error> {
        let wheel_count = self.update_steering_response(dt)?;
        self.publish_steering_wheels(wheel_count, backend)
    }

    /// Updates the live steering response before any wheel/frame dependency.
    /// The count is captured before the differential calculation, as in the
    /// original access order; later publication reads the other fields live.
    fn update_steering_response<E>(&mut self, dt: f32) -> Out<u32, E> {
        let mut blend = sub(1.0, div(self.float(0x59c)?, self.float(0x388)?)?)?;
        if blend < 0.0 {
            blend = 0.0;
        }
        let difference = sub(self.float(0x608)?, self.float(0x60c)?)?;
        // The shaping flag is read before the native maximum-steer store.
        let immediate = self.byte(0x4a8)? != 0;
        let max_steer = add(mul(difference, blend)?, self.float(0x60c)?)?;
        self.store_float(0x610, max_steer)?;
        if immediate {
            let requested = self.word(0x624)?;
            self.store(0x628, requested)?;
        } else {
            let product = mul(self.float(0x624)?, self.float(0x628)?)?;
            let accelerating = product >= 0.0
                && number(self.float(0x628)?)?.abs() <= number(self.float(0x624)?)?.abs();
            let (fast, slow) = if accelerating {
                (0x614, 0x61c)
            } else {
                (0x618, 0x620)
            };
            let rate = add(
                mul(sub(self.float(fast)?, self.float(slow)?)?, blend)?,
                self.float(slow)?,
            )?;
            let step = mul(rate, dt)?;
            let difference = sub(self.float(0x624)?, self.float(0x628)?)?.abs();
            if step >= difference {
                let requested = self.word(0x624)?;
                self.store(0x628, requested)?;
            } else if number(self.float(0x624)?)? > number(self.float(0x628)?)? {
                self.store_float(0x628, add(step, self.float(0x628)?)?)?;
            } else {
                self.store_float(0x628, sub(self.float(0x628)?, step)?)?;
            }
        }
        let ratio = div(number(self.float(0x628)?)?.abs(), self.float(0x608)?)?;
        let count = self.word(0x4c4)?;
        let differential = mul(ratio, self.float(0x62c)?)?;
        self.store_float(0x634, differential)?;
        Ok(count)
    }

    fn publish_steering_wheels<B: VehicleWheelPoseBackend>(
        &self,
        mut count: u32,
        backend: &mut B,
    ) -> Out<(), B::Error> {
        use VehicleWheelPoseUnavailable::Effect;
        while count != 0 {
            let order = self.word(0xc38)?;
            count = count.wrapping_sub(1);
            let wheel = backend
                .read_word(order.wrapping_add(count.wrapping_mul(4)))
                .map_err(Effect)?;
            if backend
                .read_byte(wheel.wrapping_add(0x120))
                .map_err(Effect)?
                & 1
                != 0
            {
                let wheel_side = number(f32::from_bits(
                    backend
                        .read_word(wheel.wrapping_add(0x18))
                        .map_err(Effect)?,
                ))?;
                let reversed = wheel_side >= number(self.float(0x40c)?)?;
                let requested = number(self.float(0x628)?)?;
                let scale = f32::from_bits(
                    backend
                        .read_word(wheel.wrapping_add(0x11c))
                        .map_err(Effect)?,
                );
                let angle = if requested <= 0.0 && count & 1 != 0 {
                    mul(add(self.float(0x634)?, requested)?, scale)?
                } else if requested > 0.0 && count & 1 == 0 {
                    mul(sub(requested, self.float(0x634)?)?, scale)?
                } else {
                    mul(scale, requested)?
                };
                let angle = if reversed { -angle } else { angle };
                let rotation = backend
                    .make_axis_angle([0.0, 1.0, 0.0], angle)
                    .map_err(Effect)?;
                let frame = backend.read_word(wheel.wrapping_add(8)).map_err(Effect)?;
                publish_frame_rotation(backend, frame, rotation)?;
            }
        }
        Ok(())
    }

    /// C_Vehicle::UpdateRotationWheels, PC canonical RVA de020. Wheel phase,
    /// original quaternion effects and final accumulator resets retain their
    /// native order. This publishes wheel poses, without advancing the body.
    pub fn update_rotation_wheels<B: VehicleWheelPoseBackend>(
        &mut self,
        dt: f32,
        backend: &mut B,
    ) -> Out<(), B::Error> {
        use VehicleWheelPoseUnavailable::Effect;
        let turn = f32::from_bits(0x40c9_0fda);
        let gear = self.word(0x55c)?;
        let mut count = self.word(0x4c4)?;
        let ratio_offset = 0x570u32.wrapping_add(gear.wrapping_mul(4));
        let engine_step = mul(
            mul(div(turn, self.float(ratio_offset)?)?, self.float(0x548)?)?,
            dt,
        )?;
        while count != 0 {
            let order = self.word(0xc38)?;
            count = count.wrapping_sub(1);
            let wheel = backend
                .read_word(order.wrapping_add(count.wrapping_mul(4)))
                .map_err(Effect)?;
            let flags = backend
                .read_word(wheel.wrapping_add(0x120))
                .map_err(Effect)?;
            if flags & 0x4000_0020 == 0 {
                let read = |b: &mut B, offset: u32| -> Out<f32, B::Error> {
                    Ok(f32::from_bits(
                        b.read_word(wheel.wrapping_add(offset)).map_err(Effect)?,
                    ))
                };
                let dot_local = |b: &mut B| -> Out<f32, B::Error> {
                    add(
                        add(
                            mul(read(b, 0x60)?, read(b, 0x6c)?)?,
                            mul(read(b, 0x5c)?, read(b, 0x68)?)?,
                        )?,
                        mul(read(b, 0x58)?, read(b, 0x64)?)?,
                    )
                };
                if flags & 0x202 == 0 {
                    let displacement = dot_local(backend)?;
                    let rotation = div(displacement, read(backend, 0x10c)?)?;
                    let angle = sub(read(backend, 0x114)?, rotation)?;
                    backend
                        .write_word(wheel.wrapping_add(0x114), angle.to_bits())
                        .map_err(Effect)?;
                } else if flags & 0x80 != 0 {
                    let gear = self.word(0x55c)? as i32;
                    if gear > 0 || gear == -1 {
                        let delta =
                            mul(div(self.float(0x5b0)?, read(backend, 0x10c)?)?, engine_step)?;
                        let angle = if gear == -1 {
                            add(delta, read(backend, 0x114)?)?
                        } else {
                            sub(read(backend, 0x114)?, delta)?
                        };
                        backend
                            .write_word(wheel.wrapping_add(0x114), angle.to_bits())
                            .map_err(Effect)?;
                    }
                } else {
                    let displacement = if flags & 8 != 0 {
                        dot_local(backend)?
                    } else {
                        add(
                            add(
                                mul(read(backend, 0x6c)?, self.float(0xc44)?)?,
                                mul(read(backend, 0x68)?, self.float(0xc40)?)?,
                            )?,
                            mul(read(backend, 0x64)?, self.float(0xc3c)?)?,
                        )?
                    };
                    let angle = sub(
                        read(backend, 0x114)?,
                        div(displacement, read(backend, 0x10c)?)?,
                    )?;
                    backend
                        .write_word(wheel.wrapping_add(0x114), angle.to_bits())
                        .map_err(Effect)?;
                }
                let angle = number(read(backend, 0x114)?)?;
                if angle > turn {
                    backend
                        .write_word(wheel.wrapping_add(0x114), sub(angle, turn)?.to_bits())
                        .map_err(Effect)?;
                } else if angle < 0.0 {
                    backend
                        .write_word(wheel.wrapping_add(0x114), add(angle, turn)?.to_bits())
                        .map_err(Effect)?;
                }
                let angle = read(backend, 0x114)?;
                let spin = backend
                    .make_axis_angle([1.0, 0.0, 0.0], angle)
                    .map_err(Effect)?;
                let frame = backend.read_word(wheel.wrapping_add(4)).map_err(Effect)?;
                let frame_flags = backend
                    .read_word(frame.wrapping_add(0xac))
                    .map_err(Effect)?;
                let prepared = frame_flags & 0x10 != 0;
                if !prepared {
                    backend
                        .write_word(frame.wrapping_add(0xac), frame_flags | 0x10)
                        .map_err(Effect)?;
                    backend.normalize_frame_rotation(frame).map_err(Effect)?;
                    backend.set_frame_rotation_matrix(frame).map_err(Effect)?;
                }
                let mut axis = [0.0; 3];
                for (index, value) in axis.iter_mut().enumerate() {
                    *value = f32::from_bits(
                        backend
                            .read_word(frame.wrapping_add(0x70 + index as u32 * 4))
                            .map_err(Effect)?,
                    );
                }
                if prepared {
                    axis = normalized_basis(axis)?;
                }
                let angle = read(backend, 0x188)?;
                let camber = backend.make_axis_angle(axis, angle).map_err(Effect)?;
                let rotation = compose_rotation(spin, camber)?;
                // Original reloads the wheel's frame after quaternion work.
                let frame = backend.read_word(wheel.wrapping_add(4)).map_err(Effect)?;
                publish_frame_rotation(backend, frame, rotation)?;
            }
            for offset in [0x64, 0x68, 0x6c] {
                backend
                    .write_word(wheel.wrapping_add(offset), 0)
                    .map_err(Effect)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        ReadWord(u32),
        ReadByte(u32),
        WriteWord(u32, u32),
        Make([u32; 3], u32),
        Normalize(u32),
        SetMatrix(u32),
    }

    #[derive(Default)]
    struct Backend {
        memory: BTreeMap<u32, u32>,
        angles: Vec<f32>,
        axes: Vec<[f32; 3]>,
        normalized: Vec<u32>,
        fail_make: bool,
        events: Vec<Event>,
        prefixes: Vec<BTreeMap<u32, u32>>,
        fail_event: Option<usize>,
        redirect_after_make: Option<(usize, u32, u32)>,
        flags_after_normalize: Option<u32>,
        fail_normalize: bool,
    }
    impl Backend {
        fn record(&mut self, event: Event) -> Result<(), &'static str> {
            self.prefixes.push(self.memory.clone());
            self.events.push(event);
            if self.fail_event == Some(self.events.len()) {
                Err("injected publication failure")
            } else {
                Ok(())
            }
        }
    }
    impl VehicleWheelPoseBackend for Backend {
        type Error = &'static str;
        fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
            self.record(Event::ReadWord(address))?;
            self.memory.get(&address).copied().ok_or("missing word")
        }
        fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
            self.record(Event::ReadByte(address))?;
            self.memory
                .get(&address)
                .copied()
                .map(|value| value as u8)
                .ok_or("missing word")
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            self.record(Event::WriteWord(address, value))?;
            self.memory.insert(address, value);
            Ok(())
        }
        fn make_axis_angle(&mut self, axis: [f32; 3], angle: f32) -> Result<SQuat, Self::Error> {
            self.record(Event::Make(axis.map(f32::to_bits), angle.to_bits()))?;
            self.axes.push(axis);
            self.angles.push(angle);
            if self.fail_make {
                return Err("make unavailable");
            }
            match self.redirect_after_make {
                Some((call, pointer, frame)) if self.angles.len() == call => {
                    self.memory.insert(pointer, frame);
                }
                _ => {}
            }
            // Deliberate spy return: this test verifies ordering and publication,
            // while original quaternion math belongs to the external dependency.
            Ok(SQuat {
                wxyz: [1.0, 0.0, angle, 0.0],
            })
        }
        fn normalize_frame_rotation(&mut self, frame: u32) -> Result<(), Self::Error> {
            self.record(Event::Normalize(frame))?;
            assert_eq!(self.memory[&(frame + 0x9c)], 1.0f32.to_bits());
            self.normalized.push(frame);
            if let Some(flags) = self.flags_after_normalize {
                self.memory.insert(frame + 0xac, flags);
            }
            if self.fail_normalize {
                Err("normalize unavailable")
            } else {
                Ok(())
            }
        }
        fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
            self.record(Event::SetMatrix(frame))?;
            Err("matrix unavailable")
        }
    }
    fn store(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn float(bytes: &[u8], offset: usize) -> f32 {
        f32::from_bits(u32::from_le_bytes(
            bytes[offset..offset + 4].try_into().unwrap(),
        ))
    }
    fn state(requested: f32) -> Vec<u8> {
        let mut bytes = vec![0; 0x2000];
        for (offset, value) in [
            (0x59c, 0.0),
            (0x388, 10.0),
            (0x608, 1.0),
            (0x60c, 0.5),
            (0x624, requested),
            (0x628, 0.0),
            (0x62c, 0.5),
            (0x40c, 0.0),
            (0x614, 1.0),
            (0x61c, 0.5),
            (0x618, 2.0),
            (0x620, 1.0),
        ] {
            store(&mut bytes, offset, value.to_bits());
        }
        bytes[0x4a8] = 1;
        bytes
    }
    fn wheels() -> Backend {
        let mut b = Backend::default();
        for (index, side) in [(0u32, -1.0f32), (1, 1.0)] {
            let wheel = 0x2000 + index * 0x200;
            let frame = 0x4000 + index * 0x200;
            for (address, value) in [
                (0x1000 + index * 4, wheel),
                (wheel + 8, frame),
                (wheel + 0x18, side.to_bits()),
                (wheel + 0x11c, 1.0f32.to_bits()),
                (wheel + 0x120, 1),
                (frame + 0xac, 0xffff_ffff),
            ] {
                b.memory.insert(address, value);
            }
        }
        b
    }
    #[test]
    fn steering_publishes_original_side_angles_in_reverse_wheel_order() {
        for requested in [-0.25f32, 0.25] {
            let mut bytes = state(requested);
            store(&mut bytes, 0x4c4, 2);
            store(&mut bytes, 0xc38, 0x1000);
            let mut b = wheels();
            VehicleWheelPoseState::from_bytes(&mut bytes)
                .update_steering_wheels(0.1, &mut b)
                .unwrap();
            let expected = if requested < 0.0 {
                [0.125, -0.25]
            } else {
                [-0.25, 0.125]
            };
            assert_eq!(b.angles, expected);
            assert!(b.axes.iter().all(|axis| *axis == [0.0, 1.0, 0.0]));
            assert_eq!(b.normalized, [0x4200, 0x4000]);
            assert_eq!(b.memory[&0x40ac], 0xffff_fecb | 0x4000_0008);
            assert_eq!(float(&bytes, 0x610), 1.0);
            assert_eq!(float(&bytes, 0x634), 0.125);
        }
    }
    #[test]
    fn steering_smoothing_uses_turn_and_return_rates_without_overshoot() {
        let mut bytes = state(0.5);
        bytes[0x4a8] = 0;
        let mut b = Backend::default();
        VehicleWheelPoseState::from_bytes(&mut bytes)
            .update_steering_wheels(0.25, &mut b)
            .unwrap();
        assert_eq!(float(&bytes, 0x628), 0.25);
        store(&mut bytes, 0x624, 0.0f32.to_bits());
        VehicleWheelPoseState::from_bytes(&mut bytes)
            .update_steering_wheels(0.05, &mut b)
            .unwrap();
        assert_eq!(float(&bytes, 0x628), 0.15);
        VehicleWheelPoseState::from_bytes(&mut bytes)
            .update_steering_wheels(1.0, &mut b)
            .unwrap();
        assert_eq!(float(&bytes, 0x628), 0.0);
        assert!(b.angles.is_empty());
    }
    #[test]
    fn missing_make_retains_vehicle_prefix_without_publishing_a_frame() {
        let mut bytes = state(0.25);
        store(&mut bytes, 0x4c4, 2);
        store(&mut bytes, 0xc38, 0x1000);
        let mut b = wheels();
        b.fail_make = true;
        assert_eq!(
            VehicleWheelPoseState::from_bytes(&mut bytes).update_steering_wheels(0.1, &mut b),
            Err(VehicleWheelPoseUnavailable::Effect("make unavailable"))
        );
        assert_eq!(float(&bytes, 0x628), 0.25);
        assert_eq!(float(&bytes, 0x634), 0.125);
        assert!(!b.memory.contains_key(&0x429c));
        assert!(b.normalized.is_empty());
    }
    #[test]
    fn invalid_numeric_tail_retains_the_original_immediate_raw_request_copy() {
        let payload = 0x7fc1_2345u32;
        let mut bytes = state(f32::from_bits(payload));
        let mut b = Backend::default();
        assert_eq!(
            VehicleWheelPoseState::from_bytes(&mut bytes).update_steering_wheels(0.1, &mut b),
            Err(VehicleWheelPoseUnavailable::NumericDomain)
        );
        assert_eq!(&bytes[0x628..0x62c], &payload.to_le_bytes());
        assert_eq!(float(&bytes, 0x610), 1.0);
        assert!(b.angles.is_empty());
    }

    #[test]
    fn steering_response_survives_unavailable_wheel_order_storage() {
        let mut bytes = state(0.25);
        store(&mut bytes, 0x4c4, 1);
        bytes.truncate(0x638);
        let mut expected = bytes.clone();
        store(&mut expected, 0x610, 1.0f32.to_bits());
        store(&mut expected, 0x628, 0.25f32.to_bits());
        store(&mut expected, 0x634, 0.125f32.to_bits());
        let mut backend = Backend::default();
        assert_eq!(
            VehicleWheelPoseState::from_bytes(&mut bytes).update_steering_wheels(0.1, &mut backend),
            Err(VehicleWheelPoseUnavailable::Storage(0xc38))
        );
        assert_eq!(bytes, expected);
        assert!(backend.events.is_empty());
    }

    fn rotation_state() -> Vec<u8> {
        let mut bytes = state(0.0);
        store(&mut bytes, 0x55c, 1);
        store(&mut bytes, 0x574, 1.0f32.to_bits());
        store(&mut bytes, 0x548, 0.125f32.to_bits());
        store(&mut bytes, 0x4c4, 1);
        store(&mut bytes, 0xc38, 0x1000);
        bytes
    }
    #[test]
    fn skipped_rotation_wheel_still_clears_its_accumulated_displacement() {
        let mut bytes = rotation_state();
        let mut b = Backend::default();
        for (address, value) in [
            (0x1000, 0x2000),
            (0x2120, 0x20),
            (0x2064, 1),
            (0x2068, 2),
            (0x206c, 3),
        ] {
            b.memory.insert(address, value);
        }
        VehicleWheelPoseState::from_bytes(&mut bytes)
            .update_rotation_wheels(0.1, &mut b)
            .unwrap();
        for address in [0x2064, 0x2068, 0x206c] {
            assert_eq!(b.memory[&address], 0);
        }
        assert!(b.angles.is_empty());
    }
    #[test]
    fn rotation_wraps_only_once_and_retains_phase_before_unavailable_make() {
        let mut bytes = rotation_state();
        let mut b = Backend {
            fail_make: true,
            ..Default::default()
        };
        for (address, value) in [
            (0x1000, 0x2000),
            (0x2120, 0),
            (0x2060, 0.0f32.to_bits()),
            (0x206c, 0.0f32.to_bits()),
            (0x205c, 0.0f32.to_bits()),
            (0x2068, 0.0f32.to_bits()),
            (0x2058, 1.0f32.to_bits()),
            (0x2064, 0.125f32.to_bits()),
            (0x210c, 1.0f32.to_bits()),
            (0x2114, 12.8f32.to_bits()),
        ] {
            b.memory.insert(address, value);
        }
        assert_eq!(
            VehicleWheelPoseState::from_bytes(&mut bytes).update_rotation_wheels(0.1, &mut b),
            Err(VehicleWheelPoseUnavailable::Effect("make unavailable"))
        );
        let expected = (12.8f32 - 0.125) - f32::from_bits(0x40c9_0fda);
        assert_eq!(b.angles, [expected]);
        assert!(expected > f32::from_bits(0x40c9_0fda));
        assert_eq!(b.axes, [[1.0, 0.0, 0.0]]);
        assert_eq!(b.memory[&0x2114], expected.to_bits());
        assert_eq!(b.memory[&0x2064], 0.125f32.to_bits());
    }

    #[derive(Clone, Copy)]
    enum Pose {
        Steering,
        Rotation,
    }
    impl Pose {
        fn update(self, bytes: &mut [u8], backend: &mut Backend) -> Out<(), &'static str> {
            let mut state = VehicleWheelPoseState::from_bytes(bytes);
            match self {
                Self::Steering => state.update_steering_wheels(0.1, backend),
                Self::Rotation => state.update_rotation_wheels(0.1, backend),
            }
        }
        fn frame_pointer(self) -> u32 {
            match self {
                Self::Steering => 0x2008,
                Self::Rotation => 0x2004,
            }
        }
        fn published_quaternion(self) -> [u32; 4] {
            match self {
                Self::Steering => [1.0f32.to_bits(), 0, 0.125f32.to_bits(), 0],
                Self::Rotation => [1.0f32.to_bits(), 0, 0, 0],
            }
        }
    }

    fn publication_fixture(pose: Pose) -> (Vec<u8>, Backend) {
        let mut bytes = match pose {
            Pose::Steering => state(0.25),
            Pose::Rotation => rotation_state(),
        };
        store(&mut bytes, 0x4c4, 1);
        store(&mut bytes, 0xc38, 0x1000);
        let mut backend = wheels();
        for (address, value) in [
            (0x2004, 0x4000),
            (0x2058, 0),
            (0x205c, 0),
            (0x2060, 0),
            (0x2064, 0.25f32.to_bits()),
            (0x2068, 0.5f32.to_bits()),
            (0x206c, 0.75f32.to_bits()),
            (0x210c, 1.0f32.to_bits()),
            (0x2114, 0),
            (0x2188, 0),
            (0x4070, 1.0f32.to_bits()),
            (0x4074, 0),
            (0x4078, 0),
            (0x409c, 0x1111_1111),
            (0x40a0, 0x2222_2222),
            (0x40a4, 0x3333_3333),
            (0x40a8, 0x4444_4444),
            (0x40ac, 0x10),
            (0x50ac, 0xffff_ffff),
        ] {
            backend.memory.insert(address, value);
        }
        backend.redirect_after_make = Some((
            match pose {
                Pose::Steering => 1,
                Pose::Rotation => 2,
            },
            pose.frame_pointer(),
            0x5000,
        ));
        backend.flags_after_normalize = Some(0x8000_0134);
        (bytes, backend)
    }

    fn final_frame_lookup(events: &[Event]) -> usize {
        events
            .iter()
            .rposition(|e| matches!(e, Event::Make(..)))
            .unwrap()
            + 1
    }

    #[test]
    fn both_poses_resolve_frames_after_make_and_flags_after_normalize() {
        for pose in [Pose::Steering, Pose::Rotation] {
            let (mut bytes, mut backend) = publication_fixture(pose);
            let mut expected = backend.memory.clone();
            expected.insert(pose.frame_pointer(), 0x5000);
            let quaternion = pose.published_quaternion();
            for (index, value) in quaternion.into_iter().enumerate() {
                expected.insert(0x509c + index as u32 * 4, value);
            }
            expected.insert(0x50ac, 0xc000_0008);
            let mut tail = vec![Event::ReadWord(pose.frame_pointer())];
            tail.extend(
                quaternion
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| Event::WriteWord(0x509c + index as u32 * 4, value)),
            );
            tail.extend([
                Event::Normalize(0x5000),
                Event::ReadWord(0x50ac),
                Event::WriteWord(0x50ac, 0xc000_0008),
            ]);
            if matches!(pose, Pose::Rotation) {
                for address in [0x2064, 0x2068, 0x206c] {
                    expected.insert(address, 0);
                    tail.push(Event::WriteWord(address, 0));
                }
            }
            assert_eq!(pose.update(&mut bytes, &mut backend), Ok(()));
            assert_eq!(backend.memory, expected);
            assert_eq!(backend.events[final_frame_lookup(&backend.events)..], tail);
        }
    }

    #[test]
    fn publication_failure_keeps_completed_words_and_stops_before_later_effects() {
        for pose in [Pose::Steering, Pose::Rotation] {
            let (mut completed_bytes, mut completed) = publication_fixture(pose);
            pose.update(&mut completed_bytes, &mut completed).unwrap();
            let frame_lookup = final_frame_lookup(&completed.events);
            // Four quaternion writes, normalize, flag read and flag write.
            for event in frame_lookup + 1..=frame_lookup + 7 {
                let (mut bytes, mut backend) = publication_fixture(pose);
                backend.fail_event = Some(event + 1);
                assert_eq!(
                    pose.update(&mut bytes, &mut backend),
                    Err(VehicleWheelPoseUnavailable::Effect(
                        "injected publication failure"
                    ))
                );
                assert_eq!(backend.events, completed.events[..=event]);
                assert_eq!(backend.memory, completed.prefixes[event]);
                assert_eq!(bytes, completed_bytes);
            }
        }
    }

    #[test]
    fn normalize_error_keeps_written_quaternion_and_callback_flag_mutation() {
        for pose in [Pose::Steering, Pose::Rotation] {
            let (mut bytes, mut backend) = publication_fixture(pose);
            let mut expected = backend.memory.clone();
            expected.insert(pose.frame_pointer(), 0x5000);
            for (index, value) in pose.published_quaternion().into_iter().enumerate() {
                expected.insert(0x509c + index as u32 * 4, value);
            }
            expected.insert(0x50ac, 0x8000_0134);
            backend.fail_normalize = true;
            assert_eq!(
                pose.update(&mut bytes, &mut backend),
                Err(VehicleWheelPoseUnavailable::Effect("normalize unavailable"))
            );
            assert_eq!(backend.events.last(), Some(&Event::Normalize(0x5000)));
            assert_eq!(backend.memory, expected);
        }
    }
}
