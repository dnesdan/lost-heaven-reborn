//! Bounded original `C_Vehicle` drivetrain arithmetic over supplied raw storage.
//! No constructor, parameter defaults, contact solver or frame publication is
//! provided. The caller supplies the shared rotation-direction byte separately.
//! Arithmetic requires IEEE nearest host rounding and matches masked x87 PC24/RN
//! (CW 0x007f) only. Needed floats must be finite normal values or signed zero;
//! exponent underflow/overflow, FP status/traps and other profiles are unsupported.
//! Failures preserve writes already reached in the original instruction order.

/// Host boundaries are distinct from a native return value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleDrivetrainUnavailable {
    Storage(u32),
    NumericDomain,
    /// The original gear sound routine must run before the remaining stores.
    ShiftCallback,
}

/// Failure in arithmetic/storage or in the supplied original sound effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleMotorRotUnavailable<E> {
    Drivetrain(VehicleDrivetrainUnavailable),
    Effect(E),
}

impl<E> From<VehicleDrivetrainUnavailable> for VehicleMotorRotUnavailable<E> {
    fn from(value: VehicleDrivetrainUnavailable) -> Self {
        Self::Drivetrain(value)
    }
}

type Result<T> = std::result::Result<T, VehicleDrivetrainUnavailable>;

/// Descriptive phases selected from the live flags, not additional stored state
/// or recovered C++ type names. Disengaging wins when both transition bits are set.
enum ClutchPhase {
    Disengaging,
    Engaging,
    FollowingTarget,
    LimitedByTarget,
}

impl ClutchPhase {
    fn from_flags(flags: u32) -> Self {
        if flags & 2 != 0 {
            Self::Disengaging
        } else if flags & 1 != 0 {
            Self::Engaging
        } else if flags & 0x24 == 0 {
            Self::FollowingTarget
        } else {
            Self::LimitedByTarget
        }
    }
}

fn operand(x: f32) -> Result<f32> {
    if x == 0.0 || x.is_normal() {
        Ok(x)
    } else {
        Err(VehicleDrivetrainUnavailable::NumericDomain)
    }
}

fn arithmetic(x: f32, wide: f64) -> Result<f32> {
    if !x.is_finite()
        || (wide != 0.0 && wide.abs() < f32::MIN_POSITIVE as f64)
        || wide.abs() > f32::MAX as f64
    {
        Err(VehicleDrivetrainUnavailable::NumericDomain)
    } else {
        Ok(x)
    }
}
fn add(a: f32, b: f32) -> Result<f32> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a + b, a as f64 + b as f64)?;
    crate::native_precision::record_add(a, b, result);
    Ok(result)
}
fn sub(a: f32, b: f32) -> Result<f32> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a - b, a as f64 - b as f64)?;
    crate::native_precision::record_sub(a, b, result);
    Ok(result)
}
fn mul(a: f32, b: f32) -> Result<f32> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a * b, a as f64 * b as f64)?;
    crate::native_precision::record_mul(a, b, result);
    Ok(result)
}
fn div(a: f32, b: f32) -> Result<f32> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a / b, a as f64 / b as f64)?;
    crate::native_precision::record_div(a, b, result);
    Ok(result)
}

/// Owned or borrowed storage projection; construction never initializes a car.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleDrivetrainState<T = Vec<u8>> {
    bytes: T,
}

impl VehicleDrivetrainState {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
impl<'a> VehicleDrivetrainState<&'a mut [u8]> {
    /// Ordered drivetrain stores act directly on the canonical vehicle owner.
    pub fn from_bytes_mut(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }
}
impl<T: AsRef<[u8]> + AsMut<[u8]>> VehicleDrivetrainState<T> {
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }
    pub(crate) fn as_bytes_mut(&mut self) -> &mut [u8] {
        self.bytes.as_mut()
    }
    fn byte(&self, offset: u32) -> Result<u8> {
        self.bytes
            .as_ref()
            .get(offset as usize)
            .copied()
            .ok_or(VehicleDrivetrainUnavailable::Storage(offset))
    }
    fn word(&self, offset: u32) -> Result<u32> {
        let start = offset as usize;
        let end = start
            .checked_add(4)
            .ok_or(VehicleDrivetrainUnavailable::Storage(offset))?;
        let bytes = self
            .bytes
            .as_ref()
            .get(start..end)
            .ok_or(VehicleDrivetrainUnavailable::Storage(offset))?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }
    fn float(&self, offset: u32) -> Result<f32> {
        operand(f32::from_bits(self.word(offset)?))
    }
    fn store(&mut self, offset: u32, value: u32) -> Result<()> {
        let start = offset as usize;
        let end = start
            .checked_add(4)
            .ok_or(VehicleDrivetrainUnavailable::Storage(offset))?;
        self.bytes
            .as_mut()
            .get_mut(start..end)
            .ok_or(VehicleDrivetrainUnavailable::Storage(offset))?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    fn store_float(&mut self, offset: u32, value: f32) -> Result<()> {
        self.store(offset, value.to_bits())
    }
    fn interpolate(&self, x: f32, lo: u32, hi: u32, from: u32, to: u32) -> Result<f32> {
        // Endpoint comparison precedes fraction arithmetic in the original.
        let (a, b) = (self.float(from)?, self.float(to)?);
        let fraction = div(
            sub(x, self.float(lo)?)?,
            sub(self.float(hi)?, self.float(lo)?)?,
        )?;
        if b < a {
            sub(a, mul(sub(a, b)?, fraction)?)
        } else {
            add(mul(sub(b, a)?, fraction)?, a)
        }
    }
    /// Original SetCutTime: signed milliseconds in 5..=50, multiplied by
    /// the original binary32 0.001 constant. Rejection makes no storage access.
    pub fn set_cut_time(&mut self, milliseconds: i32) -> Result<bool> {
        if !(5..=50).contains(&milliseconds) {
            return Ok(false);
        }
        self.store_float(0xc24, mul(milliseconds as f32, f32::from_bits(0x3a83126f))?)?;
        Ok(true)
    }

    /// Original ComputeMoment. Knot comparisons follow the original branches; no
    /// sorting, clamping or repair of the supplied curve is performed.
    pub fn compute_moment(&self, rotation: f32, power: f32) -> Result<f32> {
        let rotation = operand(rotation)?;
        let minimum = self.float(0x5ec)?;
        let x = if rotation < minimum {
            minimum
        } else {
            rotation
        };
        let first = self.float(0x484)?;
        let curve = if x <= first {
            self.interpolate(x, 0x5ec, 0x484, 0x474, 0x478)?
        } else if x > first && x <= self.float(0x488)? {
            self.float(0x478)?
        } else if x > self.float(0x488)? && x <= self.float(0x48c)? {
            self.interpolate(x, 0x488, 0x48c, 0x478, 0x47c)?
        } else {
            self.interpolate(x, 0x48c, 0x1f9c, 0x47c, 0x480)?
        };
        let drag = mul(div(rotation, self.float(0x1f9c)?)?, self.float(0x5bc)?)?;
        sub(mul(add(drag, curve)?, power)?, drag)
    }
    /// Original MotorForce, including torque-ramp and motion-flag stores.
    pub fn motor_force(&mut self, dt: f32, input: f32) -> Result<f32> {
        let mut input = operand(input)?;
        let minimum = self.float(0x24)?;
        let gear = self.word(0x55c)?;
        if input < minimum {
            input = minimum;
        }
        let flags = self.word(0x1a4)?;
        let enabled = self.byte(0xc2c)? != 0;
        self.store(0x1a4, flags & !4)?;
        if !enabled {
            let (z, y, x) = (
                self.float(0x1f98)?,
                self.float(0x1f94)?,
                self.float(0x1f90)?,
            );
            let squared = add(add(mul(x, x)?, mul(y, y)?)?, mul(z, z)?)?;
            let small = if (squared.to_bits() as i32) < 0x322bcc77 {
                true
            } else {
                let delta = sub(squared, 1.0)?.abs();
                if (delta as f64) < 1e-8 {
                    false
                } else {
                    let root = operand(squared.sqrt())?;
                    crate::native_precision::record_sqrt(squared, root);
                    root < 0.5
                }
            };
            if small && self.word(0x55c)? != 0 {
                self.store(0x1a4, self.word(0x1a4)? | 4)?;
            }
            return Ok(0.0);
        }
        let power = mul(input, self.float(0x1b0)?)?;
        let moment = self.compute_moment(self.float(0x548)?, power)?;
        // Both comparisons use the same field: the interpolation block in the
        // native body is unreachable for the admitted ordered domain.
        let mut target = if moment > self.float(0x63c)? {
            1.0
        } else {
            0.0
        };
        let rise = mul(dt, self.float(0x644)?)?;
        let previous = self.float(0x4a0)?;
        let upper = add(rise, previous)?;
        if target > upper {
            target = upper;
        } else {
            let fall = mul(dt, self.float(0x648)?)?;
            let lower = sub(self.float(0x4a0)?, fall)?;
            if target < lower {
                target = lower;
            }
        }
        self.store_float(0x4a0, target)?;
        if self.float(0x4a0)? > 1.0 {
            self.store_float(0x4a0, 1.0)?;
        } else if self.float(0x4a0)? < 0.0 {
            self.store(0x4a0, 0)?;
        }
        let ratio_offset = 0x56cu32.wrapping_add(gear.wrapping_add(1).wrapping_mul(4));
        let mut force = mul(moment, self.float(ratio_offset)?)?;
        if self.float(0x59c)? > self.float(0x4a4)? {
            force = -1.0;
        }
        if self.float(0x548)? > self.float(0x1f9c)? {
            force = -1.0;
        } else if force > 0.0 {
            return Ok(force);
        }
        if self.float(0x5dc)? > f32::from_bits(0x3f666666)
            && self.word(0x55c)? != 0
            && self.float(0x59c)? < 0.5
        {
            self.store(0x1a4, self.word(0x1a4)? | 4)?;
        }
        Ok(force)
    }
    /// Original MotorRot. `direction` represents the original shared byte,
    /// retained unchanged when no synchronization branch writes it. The native
    /// PC entry consumes two floats; PS2's three-float symbol is platform-specific.
    pub fn motor_rot(&mut self, dt: f32, force: f32, direction: &mut u8) -> Result<()> {
        self.motor_rot_with_shift_effect(dt, force, direction, |_, _| {
            Err(VehicleDrivetrainUnavailable::ShiftCallback)
        })
        .map_err(|error| match error {
            VehicleMotorRotUnavailable::Drivetrain(error)
            | VehicleMotorRotUnavailable::Effect(error) => error,
        })
    }

    /// Runs the external effect at the original clutch transition. The effect
    /// observes the reached writes and may mutate storage and the shared byte;
    /// the pending gear is read only after it returns successfully.
    pub fn motor_rot_with_shift_effect<E>(
        &mut self,
        dt: f32,
        force: f32,
        direction: &mut u8,
        mut effect: impl FnMut(&mut Self, &mut u8) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), VehicleMotorRotUnavailable<E>> {
        let limit = mul(self.float(0x5d0)?, self.float(0x44)?)?;
        let gear = self.word(0x55c)?;
        let limit = mul(limit, self.float(0x5dc)?)?;
        let ratio_offset = 0x570u32.wrapping_add(gear.wrapping_mul(4));
        let inverse = div(1.0, self.float(ratio_offset)?)?;
        let mut coupling = mul(inverse, self.float(0x554)?)?;
        let target = div(
            mul(self.float(ratio_offset)?, self.float(0x59c)?)?,
            self.float(0x5ac)?,
        )?;
        let mut delta = mul(inverse, force)?;
        if coupling > limit {
            coupling = limit;
        }
        let small = delta.abs() < coupling;
        let mode = self.word(0xb8)?;
        let mut syncing = false;
        let mut oppose = false;
        let mut compare = mode == 1;
        if mode == 0 || mode == 2 {
            let angle = self.float(0xc64)?;
            oppose = angle > f32::from_bits(0x3fc90fdb) && (gear as i32) > 0;
            if !oppose && angle < f32::from_bits(0x3fc90fdb) && (gear as i32) < 0 {
                oppose = self.float(0x59c)? > f32::from_bits(0x38d1b717);
            }
            compare = !oppose && gear != 0;
        }
        if oppose {
            delta = sub(delta, coupling)?;
        } else if compare {
            if self.float(0x548)? > target {
                delta = sub(delta, coupling)?;
                *direction = 1;
                syncing = true;
            }
            if self.float(0x548)? < target {
                delta = add(delta, coupling)?;
                *direction = 0;
                syncing = true;
            }
        }
        let delta = mul(delta, dt)?;
        let denominator = mul(self.float(0x41c)?, f32::from_bits(0x40c90fda))?;
        let updated = add(div(delta, denominator)?, self.float(0x548)?)?;
        self.store_float(0x548, updated)?;
        if syncing && (self.byte(0x5e8)? & 4 == 0 || small) {
            let bound = if *direction != 0 {
                sub(target, 0.1)?
            } else {
                add(target, 0.1)?
            };
            if (*direction != 0 && updated < bound) || (*direction == 0 && updated > bound) {
                self.store_float(0x548, bound)?;
            }
        }
        if self.float(0x548)? > self.float(0x1f9c)? {
            self.store(0x548, self.word(0x1f9c)?)?;
        }
        if self.float(0x548)? < 0.0 {
            self.store(0x548, 0)?;
        }

        self.advance_clutch_shift(dt, direction, &mut effect)
    }

    /// Advance the clutch on the same owner after publishing the motor rotation.
    /// A completed release publishes its state before the external shift effect;
    /// the requested gear is deliberately reloaded after that effect returns.
    fn advance_clutch_shift<E>(
        &mut self,
        dt: f32,
        direction: &mut u8,
        effect: &mut impl FnMut(&mut Self, &mut u8) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), VehicleMotorRotUnavailable<E>> {
        let flags = self.word(0x5e8)?;
        match ClutchPhase::from_flags(flags) {
            ClutchPhase::Disengaging => {
                let decrement = mul(dt, self.float(0x5d8)?)?;
                let next = sub(self.float(0x5dc)?, decrement)?;
                self.store_float(0x5dc, next)?;
                if next <= 0.0 {
                    let callback = self.byte(0x432)? != 0;
                    self.store(0x5dc, 0)?;
                    self.store(0x5e8, (flags & !2) | 1)?;
                    if callback {
                        effect(self, direction).map_err(VehicleMotorRotUnavailable::Effect)?;
                    }
                    self.store(0x55c, self.word(0x560)?)?;
                }
            }
            ClutchPhase::Engaging => {
                let next = add(mul(dt, self.float(0x5d4)?)?, self.float(0x5dc)?)?;
                let maximum = self.float(0x5e4)?;
                self.store_float(0x5dc, next)?;
                if next > maximum {
                    self.store(0x5dc, self.word(0x5e4)?)?;
                    self.store(0x5e8, flags & !3)?;
                } else if next > 1.0 {
                    self.store_float(0x5dc, 1.0)?;
                    self.store(0x5e8, flags & !3)?;
                }
            }
            ClutchPhase::FollowingTarget => {
                // Raw copying does not admit either field as an arithmetic operand.
                self.store(0x5dc, self.word(0x5e4)?)?;
            }
            ClutchPhase::LimitedByTarget => {
                if self.float(0x5e4)? < self.float(0x5dc)? {
                    self.store(0x5dc, self.word(0x5e4)?)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> VehicleDrivetrainState {
        let mut s = VehicleDrivetrainState::from_bytes(vec![0; 0x2000]);
        for (offset, value) in [
            (0x44, 1.0),
            (0x41c, 2.0),
            (0x474, 20.0),
            (0x478, 40.0),
            (0x47c, 30.0),
            (0x480, 5.0),
            (0x484, 100.0),
            (0x488, 200.0),
            (0x48c, 400.0),
            (0x4a0, 0.3),
            (0x4a4, 100.0),
            (0x548, 300.0),
            (0x554, 10.0),
            (0x570, 1.0),
            (0x574, 2.0),
            (0x578, 3.0),
            (0x59c, 1.0),
            (0x5ac, 2.0),
            (0x5bc, 6.0),
            (0x5d0, 4.0),
            (0x5d4, 2.0),
            (0x5d8, 2.0),
            (0x5dc, 0.6),
            (0x5e4, 0.8),
            (0x5ec, 50.0),
            (0x63c, 15.0),
            (0x644, 1.0),
            (0x648, 0.7),
            (0x1b0, 1.0),
            (0x1f90, 1.0),
            (0x1f9c, 600.0),
        ] {
            s.store_float(offset, value).unwrap();
        }
        s.store(0xb8, 1).unwrap();
        s.store(0x55c, 1).unwrap();
        s.store(0x560, 2).unwrap();
        s.bytes[0xc2c] = 1;
        s
    }

    #[test]
    fn borrowed_workers_keep_ordered_stores_in_the_original_owner() {
        let mut owned = state();
        let mut bytes = owned.as_bytes().to_vec();
        let mut direction_owned = 1;
        let mut direction_borrowed = direction_owned;
        {
            let mut borrowed = VehicleDrivetrainState::from_bytes_mut(&mut bytes);
            assert_eq!(owned.set_cut_time(16), borrowed.set_cut_time(16));
            assert_eq!(
                owned.motor_force(0.016, 0.7),
                borrowed.motor_force(0.016, 0.7)
            );
            assert_eq!(
                owned.motor_rot(0.016, 3., &mut direction_owned),
                borrowed.motor_rot(0.016, 3., &mut direction_borrowed)
            );
        }
        assert_eq!(owned.as_bytes(), bytes);
        assert_eq!(direction_owned, direction_borrowed);
        // A short live projection retains the force prefix on the same failed
        // downstream access; no temporary owner's successful copyback is needed.
        let mut short = bytes[..0x600].to_vec();
        let mut detached = VehicleDrivetrainState::from_bytes(short.clone());
        let result = VehicleDrivetrainState::from_bytes_mut(&mut short).motor_force(0.016, 0.8);
        assert_eq!(result, detached.motor_force(0.016, 0.8));
        assert!(result.is_err());
        assert_eq!(short, detached.as_bytes());
    }

    #[test]
    fn curve_preserves_knot_and_neighbor_behavior() {
        let mut s = state();
        s.store_float(0x5bc, 0.0).unwrap();
        assert_eq!(s.compute_moment(100.0, 1.0), Ok(40.0));
        assert_eq!(
            s.compute_moment(f32::from_bits(100.0f32.to_bits() + 1), 1.0),
            Ok(40.0)
        );
        assert_eq!(s.compute_moment(200.0, 1.0), Ok(40.0));
        assert_eq!(s.compute_moment(400.0, 1.0), Ok(30.0));
        assert_eq!(s.compute_moment(600.0, 1.0), Ok(5.0));
    }

    #[test]
    fn drag_uses_unclamped_rotation_and_curve_supports_both_slopes() {
        let mut s = state();
        assert_eq!(s.compute_moment(-100.0, 0.0), Ok(1.0));
        s.store_float(0x5bc, 0.0).unwrap();
        assert_eq!(s.compute_moment(75.0, 1.0), Ok(30.0));
        s.store_float(0x474, 60.0).unwrap();
        assert_eq!(s.compute_moment(75.0, 1.0), Ok(50.0));
    }

    #[test]
    fn rejected_cut_time_does_not_touch_unavailable_storage() {
        let mut s = VehicleDrivetrainState::from_bytes(vec![]);
        for value in [i32::MIN, -1, 0, 4, 51, i32::MAX] {
            assert_eq!(s.set_cut_time(value), Ok(false));
        }
        assert_eq!(
            s.set_cut_time(5),
            Err(VehicleDrivetrainUnavailable::Storage(0xc24))
        );
    }

    #[test]
    fn cut_time_multiplies_original_binary32_constant() {
        let mut s = state();
        assert_eq!(s.set_cut_time(42), Ok(true));
        assert_eq!(
            s.word(0xc24).unwrap(),
            (42.0f32 * f32::from_bits(0x3a83126f)).to_bits()
        );
        // Multiplication has a different last bit from division for this input.
        assert_ne!(s.word(0xc24).unwrap(), (42.0f32 / 1000.0).to_bits());
    }

    #[test]
    fn stopped_disabled_engine_sets_only_original_motion_flag() {
        let mut s = state();
        s.bytes[0xc2c] = 0;
        s.store_float(0x1f90, 0.0).unwrap();
        s.store(0x1a4, 0x80000010).unwrap();
        let mut expected = s.clone();
        expected.store(0x1a4, 0x80000014).unwrap();
        assert_eq!(s.motor_force(f32::NAN, 0.0), Ok(0.0));
        assert_eq!(s, expected);
    }

    #[test]
    fn numeric_failure_retains_reached_motion_flag_store() {
        let mut s = state();
        s.store(0x1a4, 7).unwrap();
        s.store_float(0x1b0, f32::MIN_POSITIVE).unwrap();
        assert_eq!(
            s.motor_force(0.1, 0.5),
            Err(VehicleDrivetrainUnavailable::NumericDomain)
        );
        assert_eq!(s.word(0x1a4).unwrap(), 3);
        assert_eq!(s.float(0x4a0).unwrap(), 0.3);
    }

    #[test]
    fn shift_callback_boundary_preserves_prefix_and_pending_gear() {
        let mut s = state();
        s.store(0x5e8, 2).unwrap();
        s.bytes[0x432] = 1;
        let mut direction = 9;
        assert_eq!(
            s.motor_rot(1.0, 0.0, &mut direction),
            Err(VehicleDrivetrainUnavailable::ShiftCallback)
        );
        assert_eq!(direction, 1);
        assert_eq!(s.float(0x5dc).unwrap(), 0.0);
        assert_eq!(s.word(0x5e8).unwrap(), 1);
        assert_eq!(s.word(0x55c).unwrap(), 1);
        assert!(s.float(0x548).unwrap() < 300.0);
    }

    #[test]
    fn completed_release_keeps_callback_mutations_and_reloads_the_requested_gear() {
        for fail in [false, true] {
            let mut s = state();
            // An unrecognized mode leaves RPM synchronization and direction alone.
            s.store(0xb8, 9).unwrap();
            s.store(0x5e8, 0x8000_0003).unwrap();
            s.store_float(0x5dc, 0.5).unwrap();
            s.bytes[0x432] = 1;
            let mut bytes = s.into_bytes();
            let mut expected = bytes.clone();
            expected[0x5dc..0x5e0].fill(0);
            expected[0x5e8..0x5ec].copy_from_slice(&0xa5a5_5a5au32.to_le_bytes());
            expected[0x560..0x564].copy_from_slice(&4u32.to_le_bytes());
            expected[0x20..0x24].copy_from_slice(&0x1234_5678u32.to_le_bytes());
            if !fail {
                expected[0x55c..0x560].copy_from_slice(&4u32.to_le_bytes());
            }
            let mut direction = 9;
            let mut calls = 0;
            let result = VehicleDrivetrainState::from_bytes_mut(&mut bytes)
                .motor_rot_with_shift_effect(0.25, 0.0, &mut direction, |owner, direction| {
                    calls += 1;
                    assert_eq!(owner.word(0x5dc), Ok(0));
                    assert_eq!(owner.word(0x5e8), Ok(0x8000_0001));
                    assert_eq!(owner.word(0x55c), Ok(1));
                    assert_eq!(owner.word(0x560), Ok(2));
                    assert_eq!(*direction, 9);
                    owner.store(0x560, 4).unwrap();
                    owner.store(0x5e8, 0xa5a5_5a5a).unwrap();
                    owner.store(0x20, 0x1234_5678).unwrap();
                    *direction = 7;
                    if fail {
                        Err("shift failed")
                    } else {
                        Ok(())
                    }
                });
            assert_eq!(
                result,
                if fail {
                    Err(VehicleMotorRotUnavailable::Effect("shift failed"))
                } else {
                    Ok(())
                }
            );
            assert_eq!(calls, 1);
            assert_eq!(direction, 7);
            assert_eq!(bytes, expected);
        }
    }

    #[test]
    fn engaging_clutch_preserves_unknown_flags_and_strict_completion_thresholds() {
        for (target, dt, clutch, flags) in [
            (0.75, 0.25, 0.75f32, 0x8000_0004u32),
            (1.0, 0.25, 1.0, 0x8000_0005),
            (2.0, 0.5, 1.0, 0x8000_0004),
            (2.0, 0.25, 1.0, 0x8000_0005),
        ] {
            let mut s = state();
            s.store(0xb8, 9).unwrap();
            s.store(0x5e8, 0x8000_0005).unwrap();
            s.store_float(0x5dc, 0.5).unwrap();
            s.store_float(0x5e4, target).unwrap();
            let mut expected = s.as_bytes().to_vec();
            expected[0x5dc..0x5e0].copy_from_slice(&clutch.to_le_bytes());
            expected[0x5e8..0x5ec].copy_from_slice(&flags.to_le_bytes());
            let mut direction = 9;
            assert_eq!(s.motor_rot(dt, 0.0, &mut direction), Ok(()));
            assert_eq!(s.as_bytes(), expected);
            assert_eq!(direction, 9);
        }
    }

    #[test]
    fn engaging_clutch_validates_target_before_publishing_progress() {
        let mut s = state();
        s.store(0xb8, 9).unwrap();
        s.store(0x5e8, 0x8000_0001).unwrap();
        s.store_float(0x5dc, 0.5).unwrap();
        s.store(0x5e4, 0x7fc1_2345).unwrap();
        let expected = s.as_bytes().to_vec();
        let mut direction = 9;
        assert_eq!(
            s.motor_rot(0.25, 0.0, &mut direction),
            Err(VehicleDrivetrainUnavailable::NumericDomain)
        );
        assert_eq!(s.as_bytes(), expected);
        assert_eq!(direction, 9);
    }

    #[test]
    fn zero_clutch_completes_pending_gear_without_callback() {
        let mut s = state();
        s.store(0x5e8, 2).unwrap();
        s.store_float(0x5dc, 0.5).unwrap();
        let mut direction = 9;
        s.motor_rot(0.25, 0.0, &mut direction).unwrap();
        assert_eq!(s.word(0x55c).unwrap(), 2);
        assert_eq!(s.word(0x5e8).unwrap(), 1);
        assert_eq!(s.float(0x5dc).unwrap(), 0.0);
    }

    #[test]
    fn ignored_float_bits_are_copied_without_numeric_admission() {
        let mut s = state();
        s.store(0xb8, 9).unwrap();
        s.store(0x5e4, 0x7fc12345).unwrap();
        let mut direction = 9;
        s.motor_rot(0.0, 0.0, &mut direction).unwrap();
        assert_eq!(s.word(0x5dc).unwrap(), 0x7fc12345);
        assert_eq!(direction, 9);
    }

    #[test]
    fn curve_denominator_and_nonfinite_operands_are_explicit_failures() {
        let mut s = state();
        let before = s.clone();
        assert_eq!(
            s.compute_moment(f32::NAN, 1.0),
            Err(VehicleDrivetrainUnavailable::NumericDomain)
        );
        assert_eq!(s, before);
        s.store_float(0x1f9c, 400.0).unwrap();
        assert_eq!(
            s.compute_moment(401.0, 1.0),
            Err(VehicleDrivetrainUnavailable::NumericDomain)
        );
    }
}
