//! Original PC vehicle integration scheduling over the caller's live storage.
//!
//! The numeric profile is CW=007f (PC24, nearest), with finite normal or zero
//! arithmetic and host nearest rounding. Raw snapshot copies retain every bit.
//! `Engine` schedules the original six-argument `Move`; the backend must supply
//! that solver, external wheel storage and the original vector angle operation.
//! A completed scheduler is not a completed vehicle solver or playable driving.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleDynamicsUnavailable<E> {
    Storage(u32),
    NumericDomain,
    Effect(E),
}

/// External dependencies of C_Vehicle::Engine. Addresses refer to storage
/// disjoint from `state`; each Move call receives the same live vehicle bytes.
/// Effects must retain their own completed prefix when returning an error.
pub trait VehicleDynamicsBackend {
    type Error;

    fn read_word(&mut self, address: u32) -> std::result::Result<u32, Self::Error>;
    fn read_byte(&mut self, address: u32) -> std::result::Result<u8, Self::Error>;
    fn write_word(&mut self, address: u32, value: u32) -> std::result::Result<(), Self::Error>;
    /// Actual six-float PC Move entry, in stack argument order. There are no
    /// default force, collision, elapsed-time or solver implementations.
    fn move_step(
        &mut self,
        state: &mut [u8],
        arguments: [f32; 6],
    ) -> std::result::Result<(), Self::Error>;
    fn angle_to(
        &mut self,
        first: [f32; 3],
        second: [f32; 3],
    ) -> std::result::Result<f32, Self::Error>;
}

/// Engine dependencies over the same addressable owner as Tick and Move.
/// This API does not copy or detach vehicle bytes to cross a solver boundary.
pub trait VehicleEngineBackend: crate::vehicle_init::VehicleInitBackend {
    /// Original AngleTo keeps its unspilled transcendental result in ST0.
    /// A token can own the retained x87 value or a portable rounding enclosure.
    type Angle;
    fn engine_move(
        &mut self,
        vehicle: u32,
        arguments: [f32; 6],
    ) -> std::result::Result<(), Self::Error>;
    fn engine_angle_to(
        &mut self,
        first: [f32; 3],
        second: [f32; 3],
    ) -> std::result::Result<Self::Angle, Self::Error>;
    fn engine_angle_spill(&mut self, angle: &Self::Angle) -> std::result::Result<f32, Self::Error>;
    fn engine_angle_ratio(
        &mut self,
        angle: Self::Angle,
        numerator: f32,
    ) -> std::result::Result<f32, Self::Error>;
}
struct AddressableEngine<'a, B> {
    backend: &'a mut B,
    vehicle: u32,
}
impl<B: VehicleEngineBackend> EngineRuntime for AddressableEngine<'_, B> {
    type Error = B::Error;
    type Angle = B::Angle;
    fn word(&mut self, o: u32) -> Result<u32, B::Error> {
        self.backend
            .read_word(self.vehicle.wrapping_add(o))
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn half(&mut self, o: u32) -> Result<u16, B::Error> {
        let low = self
            .backend
            .read_byte(self.vehicle.wrapping_add(o))
            .map_err(VehicleDynamicsUnavailable::Effect)?;
        let high = self
            .backend
            .read_byte(self.vehicle.wrapping_add(o + 1))
            .map_err(VehicleDynamicsUnavailable::Effect)?;
        Ok(u16::from_le_bytes([low, high]))
    }
    fn store(&mut self, o: u32, v: u32) -> Result<(), B::Error> {
        self.backend
            .write_word(self.vehicle.wrapping_add(o), v)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn store_half(&mut self, o: u32, v: u16) -> Result<(), B::Error> {
        for (i, x) in v.to_le_bytes().into_iter().enumerate() {
            self.backend
                .write_byte(self.vehicle.wrapping_add(o + i as u32), x)
                .map_err(VehicleDynamicsUnavailable::Effect)?;
        }
        Ok(())
    }
    fn external_word(&mut self, a: u32) -> Result<u32, B::Error> {
        self.backend
            .read_word(a)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn external_byte(&mut self, a: u32) -> Result<u8, B::Error> {
        self.backend
            .read_byte(a)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn external_store(&mut self, a: u32, v: u32) -> Result<(), B::Error> {
        self.backend
            .write_word(a, v)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn move_step(&mut self, a: [f32; 6]) -> Result<(), B::Error> {
        self.backend
            .engine_move(self.vehicle, a)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn angle_to(&mut self, a: [f32; 3], b: [f32; 3]) -> Result<Self::Angle, B::Error> {
        self.backend
            .engine_angle_to(a, b)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn angle_spill(&mut self, a: &Self::Angle) -> Result<f32, B::Error> {
        self.backend
            .engine_angle_spill(a)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn angle_ratio(&mut self, a: Self::Angle, n: f32) -> Result<f32, B::Error> {
        self.backend
            .engine_angle_ratio(a, n)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
}

/// Original Engine scheduling directly on the caller's live address map.
pub fn vehicle_engine<B: VehicleEngineBackend>(
    backend: &mut B,
    vehicle: u32,
    dt: f32,
    first_force: f32,
    second_force: f32,
) -> Result<bool, B::Error> {
    run_engine(
        &mut AddressableEngine { backend, vehicle },
        dt,
        first_force,
        second_force,
    )
}

/// A borrow of the existing vehicle owner, never a second physics object.
pub struct VehicleDynamicsState<'a> {
    bytes: &'a mut [u8],
}

type Result<T, E> = std::result::Result<T, VehicleDynamicsUnavailable<E>>;

fn operand<E>(value: f32) -> Result<f32, E> {
    if value.is_normal() || value == 0.0 {
        Ok(value)
    } else {
        Err(VehicleDynamicsUnavailable::NumericDomain)
    }
}

fn arithmetic<E>(value: f32, wide: f64) -> Result<f32, E> {
    // A result rounded all the way to zero from a nonzero value is outside the
    // recovered finite-normal domain, just as in the drivetrain helpers.
    if value.is_normal() || value == 0.0 && wide == 0.0 {
        Ok(value)
    } else {
        Err(VehicleDynamicsUnavailable::NumericDomain)
    }
}
fn mul<E>(a: f32, b: f32) -> Result<f32, E> {
    operand(a)?;
    operand(b)?;
    let value = arithmetic(a * b, f64::from(a) * f64::from(b))?;
    crate::native_precision::record_mul(a, b, value);
    Ok(value)
}
fn add<E>(a: f32, b: f32) -> Result<f32, E> {
    operand(a)?;
    operand(b)?;
    let value = arithmetic(a + b, f64::from(a) + f64::from(b))?;
    crate::native_precision::record_add(a, b, value);
    Ok(value)
}
fn sub<E>(a: f32, b: f32) -> Result<f32, E> {
    operand(a)?;
    operand(b)?;
    let value = arithmetic(a - b, f64::from(a) - f64::from(b))?;
    crate::native_precision::record_sub(a, b, value);
    Ok(value)
}
fn div<E>(a: f32, b: f32) -> Result<f32, E> {
    operand(a)?;
    operand(b)?;
    let value = arithmetic(a / b, f64::from(a) / f64::from(b))?;
    crate::native_precision::record_div(a, b, value);
    Ok(value)
}

impl<'a> VehicleDynamicsState<'a> {
    pub fn from_bytes(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }

    fn word<E>(&self, offset: u32) -> Result<u32, E> {
        let start = offset as usize;
        let bytes = self
            .bytes
            .get(start..start + 4)
            .ok_or(VehicleDynamicsUnavailable::Storage(offset))?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }
    fn half<E>(&self, offset: u32) -> Result<u16, E> {
        let start = offset as usize;
        let bytes = self
            .bytes
            .get(start..start + 2)
            .ok_or(VehicleDynamicsUnavailable::Storage(offset))?;
        Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
    }
    fn store<E>(&mut self, offset: u32, value: u32) -> Result<(), E> {
        let start = offset as usize;
        self.bytes
            .get_mut(start..start + 4)
            .ok_or(VehicleDynamicsUnavailable::Storage(offset))?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    fn store_half<E>(&mut self, offset: u32, value: u16) -> Result<(), E> {
        let start = offset as usize;
        self.bytes
            .get_mut(start..start + 2)
            .ok_or(VehicleDynamicsUnavailable::Storage(offset))?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    /// C_Vehicle::Engine, PC canonical RVA e1ce0. Only the defined boolean
    /// return is projected. Move/AngleTo completion is the backend's boundary.
    /// This borrowed facade admits a controlled binary32 AngleTo dependency;
    /// use the addressable `vehicle_engine` API with a retained-angle backend
    /// when composing the actual unspilled LS3DF result.
    pub fn engine<B: VehicleDynamicsBackend>(
        &mut self,
        dt: f32,
        argument2: f32,
        argument3: f32,
        backend: &mut B,
    ) -> Result<bool, B::Error> {
        run_engine(
            &mut BorrowedEngine {
                state: self,
                backend,
            },
            dt,
            argument2,
            argument3,
        )
    }
}

trait EngineRuntime {
    type Error;
    type Angle;
    fn word(&mut self, offset: u32) -> Result<u32, Self::Error>;
    fn half(&mut self, offset: u32) -> Result<u16, Self::Error>;
    fn store(&mut self, offset: u32, value: u32) -> Result<(), Self::Error>;
    fn store_half(&mut self, offset: u32, value: u16) -> Result<(), Self::Error>;
    fn external_word(&mut self, address: u32) -> Result<u32, Self::Error>;
    fn external_byte(&mut self, address: u32) -> Result<u8, Self::Error>;
    fn external_store(&mut self, address: u32, value: u32) -> Result<(), Self::Error>;
    fn move_step(&mut self, arguments: [f32; 6]) -> Result<(), Self::Error>;
    fn angle_to(&mut self, first: [f32; 3], second: [f32; 3]) -> Result<Self::Angle, Self::Error>;
    fn angle_spill(&mut self, angle: &Self::Angle) -> Result<f32, Self::Error>;
    fn angle_ratio(&mut self, angle: Self::Angle, numerator: f32) -> Result<f32, Self::Error>;
    fn float(&mut self, offset: u32) -> Result<f32, Self::Error> {
        Ok(f32::from_bits(self.word(offset)?))
    }
    fn copy_words(&mut self, destination: u32, source: u32, count: u32) -> Result<(), Self::Error> {
        for i in 0..count {
            let value = self.word(source + i * 4)?;
            self.store(destination + i * 4, value)?;
        }
        Ok(())
    }
    fn copy_vector_pair(&mut self, destination: u32, source: u32) -> Result<(), Self::Error> {
        let first = self.word(source)?;
        self.store(destination, first)?;
        let second = self.word(source + 4)?;
        let third = self.word(source + 8)?;
        self.store(destination + 4, second)?;
        self.store(destination + 8, third)
    }
}
struct BorrowedEngine<'a, 'b, B> {
    state: &'a mut VehicleDynamicsState<'b>,
    backend: &'a mut B,
}
impl<B: VehicleDynamicsBackend> EngineRuntime for BorrowedEngine<'_, '_, B> {
    type Error = B::Error;
    // The legacy borrowed interface is the explicitly binary32-angle profile.
    type Angle = f32;
    fn word(&mut self, o: u32) -> Result<u32, B::Error> {
        self.state.word(o)
    }
    fn half(&mut self, o: u32) -> Result<u16, B::Error> {
        self.state.half(o)
    }
    fn store(&mut self, o: u32, v: u32) -> Result<(), B::Error> {
        self.state.store(o, v)
    }
    fn store_half(&mut self, o: u32, v: u16) -> Result<(), B::Error> {
        self.state.store_half(o, v)
    }
    fn external_word(&mut self, a: u32) -> Result<u32, B::Error> {
        self.backend
            .read_word(a)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn external_byte(&mut self, a: u32) -> Result<u8, B::Error> {
        self.backend
            .read_byte(a)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn external_store(&mut self, a: u32, v: u32) -> Result<(), B::Error> {
        self.backend
            .write_word(a, v)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn move_step(&mut self, a: [f32; 6]) -> Result<(), B::Error> {
        self.backend
            .move_step(self.state.bytes, a)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn angle_to(&mut self, a: [f32; 3], b: [f32; 3]) -> Result<f32, B::Error> {
        self.backend
            .angle_to(a, b)
            .map_err(VehicleDynamicsUnavailable::Effect)
    }
    fn angle_spill(&mut self, a: &f32) -> Result<f32, B::Error> {
        Ok(*a)
    }
    fn angle_ratio(&mut self, a: f32, n: f32) -> Result<f32, B::Error> {
        div(n, a)
    }
}
fn run_engine<R: EngineRuntime>(
    r: &mut R,
    dt: f32,
    argument2: f32,
    argument3: f32,
) -> Result<bool, R::Error> {
    let speed = operand(r.float(0x59c)?)?;
    let drag = if speed > 0.0 {
        let squared = mul(speed, speed)?;
        let a = mul(
            mul(mul(r.float(0x53c)?, r.float(0x538)?)?, squared)?,
            f32::from_bits(0x3f26_6666),
        )?;
        let b = mul(
            mul(mul(r.float(0x3d4)?, r.float(0x3d0)?)?, squared)?,
            f32::from_bits(0x3f26_6666),
        )?;
        let c = mul(
            mul(mul(r.float(0x3dc)?, r.float(0x3d8)?)?, squared)?,
            f32::from_bits(0x3f26_6666),
        )?;
        [a, b, c]
    } else {
        [0.0; 3]
    };
    if operand(r.float(0x5b8)?)? == 0.0 {
        r.store(0x3cc, 0)?;
    }
    let old_x = r.float(0xc3c)?;
    r.store(0x5b4, 0)?;
    let old = [old_x, r.float(0xc40)?, r.float(0xc44)?];
    r.copy_words(0xde8, 0xce8, 16)?;
    // The native comparison precedes the second matrix and vector copies.
    let multiple = operand(dt)? > operand(r.float(0xc24)?)?;
    r.copy_words(0xe28, 0xd28, 16)?;
    // Native source second/third words are loaded before either is stored.
    r.copy_vector_pair(0x68, 0x320)?;
    r.copy_vector_pair(0x74, 0xc3c)?;
    r.copy_words(0x80, 0xc54, 3)?;
    r.copy_vector_pair(0x8c, 0xc48)?;
    if multiple {
        let quotient: f32 = div(dt, r.float(0xc24)?)?;
        // __ftol returns a signed 64-bit integer. Only its low 16 bits
        // survive INC/MOV AX; retain that wrap instead of clamping steps.
        if f64::from(quotient) < i64::MIN as f64
            || f64::from(quotient) >= 9_223_372_036_854_775_808.0
        {
            return Err(VehicleDynamicsUnavailable::NumericDomain);
        }
        crate::native_precision::record_precision(quotient != quotient.trunc());
        let count = (quotient.trunc() as i64 as u16).wrapping_add(1);
        r.store_half(0x66, 0)?;
        r.store_half(0x64, count)?;
        let step = div(dt, f32::from(count))?;
        let mut index = 0u32;
        if count != 0 {
            loop {
                r.move_step([step, drag[0], drag[1], drag[2], argument2, argument3])?;
                index += 1;
                // The solver can mutate the step-count header.
                if index >= u32::from(r.half(0x64)?) {
                    break;
                }
            }
        }
    } else {
        r.store_half(0x64, 1)?;
        r.store_half(0x66, 0)?;
        r.move_step([dt, drag[0], drag[1], drag[2], argument2, argument3])?;
    }
    let mut remaining = r.word(0x4c4)?;
    while remaining != 0 {
        let pointers = r.word(0xc38)?;
        remaining = remaining.wrapping_sub(1);
        let wheel = r.external_word(pointers.wrapping_add(remaining.wrapping_mul(4)))?;
        let value = r.external_word(wheel.wrapping_add(0x1c8))?;
        let adjusted = add(f32::from_bits(value), r.float(0x5b4)?)?;
        r.external_store(wheel.wrapping_add(0x1c8), adjusted.to_bits())?;
    }
    let up = [r.float(0xc54)?, r.float(0xc58)?, r.float(0xc5c)?];
    let cross_z = sub(mul(old[2], up[1])?, mul(old[1], up[2])?)?;
    let cross_x = sub(mul(old[0], up[2])?, mul(old[2], up[0])?)?;
    let cross_y = sub(mul(old[1], up[0])?, mul(old[0], up[1])?)?;
    let projected = [
        sub(mul(cross_x, up[2])?, mul(cross_y, up[1])?)?,
        sub(mul(cross_y, up[0])?, mul(cross_z, up[2])?)?,
        sub(mul(cross_z, up[1])?, mul(cross_x, up[0])?)?,
    ];
    let current = [r.float(0xc3c)?, r.float(0xc40)?, r.float(0xc44)?];
    let retained_angle = r.angle_to(projected, current)?;
    let angle = r.angle_spill(&retained_angle)?;
    operand(angle)?;
    r.store(0x350, angle.to_bits())?;
    if angle > 0.0 {
        let distance = r.float(0x5b4)?;
        let curvature = operand(r.angle_ratio(retained_angle, distance)?)?;
        r.store(0x350, curvature.to_bits())?;
    }
    let curvature = operand(r.float(0x350)?)?;
    if curvature > 0.0 && curvature < operand(r.float(0x354)?)? {
        let mut remaining = r.word(0x4c4)?;
        let mut eligible = true;
        if remaining != 0 {
            let mut end = r.word(0xc38)?.wrapping_add(remaining.wrapping_mul(4));
            loop {
                let wheel = r.external_word(end.wrapping_sub(4))?;
                end = end.wrapping_sub(4);
                remaining = remaining.wrapping_sub(1);
                if r.external_byte(wheel.wrapping_add(0x120))? & 0x90 != 0 {
                    eligible = false;
                    break;
                }
                if remaining == 0 {
                    break;
                }
            }
        }
        if eligible {
            let bits = r.word(0x350)?;
            r.store(0x354, bits)?;
        }
    }
    if operand(r.float(0x628)?)? != 0.0
        && operand(r.float(0x59c)?)? < 2.0
        && f64::from(operand(r.float(0x350)?)?.abs()) < 0.001
    {
        let bits = r.word(0x354)?;
        r.store(0x350, bits)?;
    }
    if operand(r.float(0x628)?)? < 0.0 {
        let bits = r.word(0x350)? ^ 0x8000_0000;
        r.store(0x350, bits)?;
    }
    // EDI, ESI and EBP have been popped at the epilogue; [ESP+30h]
    // therefore addresses the caller's original first argument, dt.
    r.store(0x4c8, dt.to_bits())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Backend {
        steps: Vec<[u32; 6]>,
        external: BTreeMap<u32, u32>,
        writes: Vec<(u32, u32)>,
        fail_move: bool,
        shorten: bool,
        angle: f32,
    }
    impl VehicleDynamicsBackend for Backend {
        type Error = u32;
        fn read_word(&mut self, address: u32) -> std::result::Result<u32, u32> {
            self.external.get(&address).copied().ok_or(address)
        }
        fn read_byte(&mut self, address: u32) -> std::result::Result<u8, u32> {
            Ok(self.read_word(address)? as u8)
        }
        fn write_word(&mut self, address: u32, value: u32) -> std::result::Result<(), u32> {
            self.writes.push((address, value));
            self.external.insert(address, value);
            Ok(())
        }
        fn move_step(&mut self, bytes: &mut [u8], args: [f32; 6]) -> std::result::Result<(), u32> {
            self.steps.push(args.map(f32::to_bits));
            if self.fail_move {
                return Err(7);
            }
            if self.shorten {
                bytes[0x64..0x66].copy_from_slice(&1u16.to_le_bytes());
            }
            bytes[0x5b4..0x5b8].copy_from_slice(&0.25f32.to_le_bytes());
            Ok(())
        }
        fn angle_to(
            &mut self,
            _first: [f32; 3],
            _second: [f32; 3],
        ) -> std::result::Result<f32, u32> {
            Ok(self.angle)
        }
    }
    fn word(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }
    fn put(bytes: &mut [u8], offset: usize, bits: u32) {
        bytes[offset..offset + 4].copy_from_slice(&bits.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0xe68];
        for (offset, value) in [
            (0x59c, 2.0f32),
            (0x53c, 1.0),
            (0x538, 1.0),
            (0x3d4, 2.0),
            (0x3d0, 1.0),
            (0x3dc, 3.0),
            (0x3d8, 1.0),
            (0xc24, 0.01),
            (0xc3c, 1.0),
            (0xc58, 1.0),
            (0x354, 5.0),
        ] {
            put(&mut bytes, offset, value.to_bits());
        }
        for i in 0..32 {
            put(&mut bytes, 0xce8 + i * 4, 0x7fc0_0000 + i as u32);
        }
        bytes
    }
    #[test]
    fn scheduling_uses_six_arguments_and_preserves_raw_snapshots() {
        let mut bytes = fixture();
        let before = bytes.clone();
        let mut backend = Backend::default();
        assert_eq!(
            VehicleDynamicsState::from_bytes(&mut bytes).engine(0.02, 0.3, -0.7, &mut backend),
            Ok(true)
        );
        assert_eq!(backend.steps.len(), 3);
        assert_eq!(
            backend.steps[0],
            [
                (0.02f32 / 3.0).to_bits(),
                2.6f32.to_bits(),
                5.2f32.to_bits(),
                7.7999997f32.to_bits(),
                0.3f32.to_bits(),
                (-0.7f32).to_bits()
            ]
        );
        assert_eq!(&bytes[0xde8..0xe28], &before[0xce8..0xd28]);
        assert_eq!(&bytes[0xe28..0xe68], &before[0xd28..0xd68]);
        assert_eq!(word(&bytes, 0x4c8), 0.02f32.to_bits());
    }
    #[test]
    fn unavailable_solver_retains_snapshot_and_header_prefix() {
        let mut bytes = fixture();
        put(&mut bytes, 0x350, 0x1234_5678);
        put(&mut bytes, 0x4c8, 0x1234_5678);
        let mut backend = Backend {
            fail_move: true,
            ..Backend::default()
        };
        assert_eq!(
            VehicleDynamicsState::from_bytes(&mut bytes).engine(0.005, 0.0, 0.0, &mut backend),
            Err(VehicleDynamicsUnavailable::Effect(7))
        );
        assert_eq!(&bytes[0x64..0x68], &1u32.to_le_bytes());
        assert_eq!(word(&bytes, 0x5b4), 0);
        assert_eq!(word(&bytes, 0x350), 0x1234_5678);
        assert_eq!(word(&bytes, 0x4c8), 0x1234_5678);
    }
    #[test]
    fn solver_changes_to_step_count_are_observed() {
        let mut bytes = fixture();
        let mut backend = Backend {
            shorten: true,
            ..Backend::default()
        };
        VehicleDynamicsState::from_bytes(&mut bytes)
            .engine(0.04, 0.0, 0.0, &mut backend)
            .unwrap();
        assert_eq!(backend.steps.len(), 1);
        assert_eq!(backend.steps[0][0], (0.04f32 / 5.0).to_bits());
    }
    #[test]
    fn reverse_wheel_updates_complete_before_curvature_stores() {
        let mut bytes = fixture();
        put(&mut bytes, 0x4c4, 2);
        put(&mut bytes, 0xc38, 0x8000);
        let mut backend = Backend {
            angle: 0.5,
            ..Backend::default()
        };
        for (address, value) in [
            (0x8000, 0x9000),
            (0x8004, 0xa000),
            (0x91c8, 1.0f32.to_bits()),
            (0xa1c8, 2.0f32.to_bits()),
            (0x9120, 0),
            (0xa120, 0x80),
        ] {
            backend.external.insert(address, value);
        }
        VehicleDynamicsState::from_bytes(&mut bytes)
            .engine(0.005, 0.0, 0.0, &mut backend)
            .unwrap();
        assert_eq!(
            backend.writes,
            [(0xa1c8, 2.25f32.to_bits()), (0x91c8, 1.25f32.to_bits())]
        );
        assert_eq!(word(&bytes, 0x350), 0.5f32.to_bits());
        assert_eq!(word(&bytes, 0x354), 5.0f32.to_bits());
    }
    #[test]
    fn numeric_failure_preserves_reached_zero_brake_store() {
        let mut bytes = fixture();
        bytes.truncate(0xc3c);
        put(&mut bytes, 0x3cc, 9);
        let mut backend = Backend::default();
        assert_eq!(
            VehicleDynamicsState::from_bytes(&mut bytes).engine(0.005, 0.0, 0.0, &mut backend),
            Err(VehicleDynamicsUnavailable::Storage(0xc3c))
        );
        assert_eq!(word(&bytes, 0x3cc), 0);
        assert!(backend.steps.is_empty());
    }
    struct ReadFault<R> {
        inner: R,
        offset: u32,
    }
    impl<R: EngineRuntime> EngineRuntime for ReadFault<R> {
        type Error = R::Error;
        type Angle = R::Angle;
        fn word(&mut self, o: u32) -> Result<u32, Self::Error> {
            if o == self.offset {
                Err(VehicleDynamicsUnavailable::Storage(o))
            } else {
                self.inner.word(o)
            }
        }
        fn half(&mut self, o: u32) -> Result<u16, Self::Error> {
            self.inner.half(o)
        }
        fn store(&mut self, o: u32, v: u32) -> Result<(), Self::Error> {
            self.inner.store(o, v)
        }
        fn store_half(&mut self, o: u32, v: u16) -> Result<(), Self::Error> {
            self.inner.store_half(o, v)
        }
        fn external_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            self.inner.external_word(a)
        }
        fn external_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.inner.external_byte(a)
        }
        fn external_store(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.inner.external_store(a, v)
        }
        fn move_step(&mut self, a: [f32; 6]) -> Result<(), Self::Error> {
            self.inner.move_step(a)
        }
        fn angle_to(&mut self, a: [f32; 3], b: [f32; 3]) -> Result<Self::Angle, Self::Error> {
            self.inner.angle_to(a, b)
        }
        fn angle_spill(&mut self, a: &Self::Angle) -> Result<f32, Self::Error> {
            self.inner.angle_spill(a)
        }
        fn angle_ratio(&mut self, a: Self::Angle, n: f32) -> Result<f32, Self::Error> {
            self.inner.angle_ratio(a, n)
        }
    }
    #[test]
    fn source_vector_pair_is_loaded_before_either_later_store() {
        let mut bytes = fixture();
        put(&mut bytes, 0x320, 1.0f32.to_bits());
        put(&mut bytes, 0x324, 2.0f32.to_bits());
        put(&mut bytes, 0x328, 3.0f32.to_bits());
        put(&mut bytes, 0x6c, 0x11223344);
        put(&mut bytes, 0x70, 0x55667788);
        let mut state = VehicleDynamicsState::from_bytes(&mut bytes);
        let mut backend = Backend::default();
        let inner = BorrowedEngine {
            state: &mut state,
            backend: &mut backend,
        };
        let mut fault = ReadFault {
            inner,
            offset: 0x328,
        };
        assert_eq!(
            run_engine(&mut fault, 0.005, 0., 0.),
            Err(VehicleDynamicsUnavailable::Storage(0x328))
        );
        assert_eq!(word(&bytes, 0x68), 1.0f32.to_bits());
        assert_eq!(word(&bytes, 0x6c), 0x11223344);
        assert_eq!(word(&bytes, 0x70), 0x55667788);
        assert!(backend.steps.is_empty());
    }
}
