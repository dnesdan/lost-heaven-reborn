//! Bounded original `C_Vehicle` input setters over caller-supplied raw storage.
//! This projection neither constructs a vehicle nor simulates its dynamics.
//! The caller must establish ordinary IEEE round-to-nearest host arithmetic;
//! these APIs neither inspect nor set its floating-point environment. Original
//! comparisons are bounded to masked x87 PC24/RN (control word 0x007f, equivalent
//! defined bits to requested 0x003f). Other profiles, status flags and traps are
//! outside this contract. Needed arithmetic operands and intermediates must be
//! finite normal values or signed zero; raw copies and ignored fields preserve
//! every bit, including NaN payloads. No original whole-runtime FP profile or
//! gameplay execution is established by these bounded setters.

/// Host availability failures, distinct from a native setter returning false.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleControlUnavailable {
    /// The next original read or write falls outside supplied storage.
    Storage(u32),
    /// A needed arithmetic operand or intermediate needs wider exponent support.
    NumericDomain,
}

/// Typed owner of an explicitly supplied vehicle storage projection. No native
/// constructor, default parameters, frame ownership or physics is implied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleControlState<T = Vec<u8>> {
    bytes: T,
}

type Result<T> = std::result::Result<T, VehicleControlUnavailable>;

fn operand(value: f32) -> Result<f32> {
    if value == 0.0 || value.is_normal() {
        Ok(value)
    } else {
        Err(VehicleControlUnavailable::NumericDomain)
    }
}

fn arithmetic(value: f32, widened: f64) -> Result<f32> {
    if !value.is_finite()
        || (widened != 0.0 && widened.abs() < f32::MIN_POSITIVE as f64)
        || widened.abs() > f32::MAX as f64
    {
        Err(VehicleControlUnavailable::NumericDomain)
    } else {
        Ok(value)
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

// `RangeInclusive::contains` would reject unordered values, unlike the native
// x87 branches. Comparisons deliberately retain that distinction.
#[allow(clippy::manual_range_contains)]
fn outside_range(value: f32, lower: f32, upper: f32) -> bool {
    value > upper || value < lower
}

impl VehicleControlState {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl<'a> VehicleControlState<&'a mut [u8]> {
    /// Borrows the existing vehicle bytes. Setters retain their ordered writes
    /// directly in that owner, including writes preceding an unavailable read.
    pub fn from_bytes_mut(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> VehicleControlState<T> {
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    fn byte(&self, offset: u32) -> Result<u8> {
        self.bytes
            .as_ref()
            .get(offset as usize)
            .copied()
            .ok_or(VehicleControlUnavailable::Storage(offset))
    }

    fn word(&self, offset: u32) -> Result<u32> {
        let start = offset as usize;
        let end = start
            .checked_add(4)
            .ok_or(VehicleControlUnavailable::Storage(offset))?;
        let bytes = self
            .bytes
            .as_ref()
            .get(start..end)
            .ok_or(VehicleControlUnavailable::Storage(offset))?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn float(&self, offset: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(offset)?))
    }

    fn store(&mut self, offset: u32, value: u32) -> Result<()> {
        let start = offset as usize;
        let end = start
            .checked_add(4)
            .ok_or(VehicleControlUnavailable::Storage(offset))?;
        self.bytes
            .as_mut()
            .get_mut(start..end)
            .ok_or(VehicleControlUnavailable::Storage(offset))?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn store_float(&mut self, offset: u32, value: f32) -> Result<()> {
        self.store(offset, value.to_bits())
    }

    // Native brake-ramp sequence; its first store survives later unavailability.
    fn brake_value(&mut self, input: f32) -> Result<()> {
        if self.byte(0x4aa)? != 0 {
            self.store_float(0x5b8, input)?;
        } else {
            let sum = add(self.float(0x4c8)?, self.float(0x3cc)?)?;
            self.store_float(0x3cc, sum)?;
            let maximum = self.float(0x3c8)?;
            if sum > maximum {
                self.store_float(0x3cc, maximum)?;
            }
            let ratio = div(self.float(0x3cc)?, self.float(0x3c8)?)?;
            self.store_float(0x5b8, mul(ratio, input)?)?;
        }
        Ok(())
    }

    fn clear_motion_flag(&mut self) -> Result<()> {
        let flags = self.word(0x1a4)?;
        self.store(0x1a4, flags & 0xfeff_ffff)
    }

    /// Original SetPower. Native range failure leaves storage intact; unknown
    /// vehicle modes return true without examining the input. NaN comparisons
    /// are unordered and pass the original range branches, not clamped.
    pub fn set_power(&mut self, input: f32, first_channel: bool) -> Result<bool> {
        let mode = self.word(0xb8)?;
        if mode == 1 {
            if outside_range(input, -1.0, 1.0) {
                return Ok(false);
            }
            let enabled = self.byte(0xc2c)? != 0;
            self.store_float(
                if first_channel { 0x3a4 } else { 0x3a8 },
                if enabled { input } else { 0.0 },
            )?;
            self.store(0x5b8, 0)?;
        } else if mode == 0 || mode == 2 {
            if outside_range(input, 0.0, 1.0) {
                return Ok(false);
            }
            let reverse_enabled = self.byte(0x4cc)? != 0;
            self.store(0x3a8, 0)?;
            let reversing = reverse_enabled && (self.word(0x55c)? as i32) < 0;
            if reversing {
                self.brake_value(input)?;
                self.clear_motion_flag()?;
            } else {
                let enabled = self.byte(0xc2c)? != 0;
                self.store(0x3a4, 0)?;
                if enabled {
                    self.store_float(0x3a4, input)?;
                }
            }
        }
        Ok(true)
    }

    /// Original SetBrake, including reversal into the power channel and the
    /// original ordered partial writes. Mode one always returns false.
    pub fn set_brake(&mut self, input: f32) -> Result<bool> {
        if outside_range(input, 0.0, 1.0) {
            return Ok(false);
        }
        if self.word(0xb8)? == 1 {
            return Ok(false);
        }
        if self.byte(0x4cc)? == 0 {
            self.brake_value(input)?;
            if input > 0.0 {
                self.store(0x3a8, 0)?;
                self.store(0x3a4, 0)?;
            }
        } else if (self.word(0x55c)? as i32) >= 0 {
            self.brake_value(input)?;
            if input > 0.0 {
                self.store(0x3a8, 0)?;
                self.store(0x3a4, 0)?;
            }
            self.clear_motion_flag()?;
        } else {
            // Native reads enable before these two stores.
            let enabled = self.byte(0xc2c)? != 0;
            self.store(0x3a4, 0)?;
            self.store(0x3a8, 0)?;
            if enabled {
                self.store_float(0x3a4, input)?;
            }
        }
        Ok(true)
    }

    /// Original SetClutch. The raw branch copies the argument bits exactly.
    pub fn set_clutch(&mut self, input: f32) -> Result<bool> {
        if outside_range(input, 0.0, 1.0) {
            return Ok(false);
        }
        let value = if self.byte(0x4ab)? != 0 {
            let remaining = sub(1.0, input)?;
            let curved = mul(remaining, self.float(0x19c)?)?;
            mul(add(curved, input)?, input)?
        } else {
            input
        };
        self.store_float(0x5e0, value)?;
        Ok(true)
    }

    /// Original SetSteer. Negative and zero inputs use the native negative
    /// branch, including its sign change before the maximum-steer product.
    pub fn set_steer(&mut self, input: f32) -> Result<bool> {
        if outside_range(input, -1.0, 1.0) {
            return Ok(false);
        }
        let shaped = if self.byte(0x4a8)? != 0 {
            if input > 0.0 {
                let remaining = sub(1.0, input)?;
                let curved = mul(remaining, self.float(0x198)?)?;
                mul(add(curved, input)?, input)?
            } else {
                let shifted = add(input, 1.0)?;
                let curved = mul(shifted, self.float(0x198)?)?;
                -mul(sub(input, curved)?, input)?
            }
        } else {
            input
        };
        let value = mul(shaped, self.float(0x610)?)?;
        self.store_float(0x624, value)?;
        Ok(true)
    }

    /// Original PC SetHandbrake. The pressed branch retains the ramp write
    /// before clamping and scaling; release clears force before ramp storage.
    /// The PC entry accepts one boolean, unlike the PS2 two-boolean symbol.
    pub fn set_handbrake(&mut self, pressed: bool) -> Result<bool> {
        if pressed {
            let sum = add(self.float(0x4c8)?, self.float(0x3b8)?)?;
            self.store_float(0x3b8, sum)?;
            let maximum = self.float(0x3b4)?;
            if sum > maximum {
                self.store_float(0x3b8, maximum)?;
            }
            let ratio = div(self.float(0x3b8)?, self.float(0x3b4)?)?;
            let force = mul(ratio, self.float(0x3b0)?)?;
            self.store_float(0x434, force)?;
        } else {
            self.store(0x434, 0)?;
            self.store(0x3b8, 0)?;
        }
        Ok(true)
    }

    /// Original SetGear. Success can leave the selected gear unchanged. Table
    /// accesses preserve native wrapping index arithmetic and require supplied
    /// storage; no guessed maximum gear count or ratios are introduced.
    pub fn set_gear(&mut self, gear: i32) -> Result<bool> {
        if gear < -1 || gear > self.word(0x568)? as i32 {
            return Ok(false);
        }
        if self.word(0x560)? as i32 == gear {
            return Ok(true);
        }
        if self.word(0xb8)? == 1 && gear < 0 {
            return Ok(false);
        }
        let restricted = self.word(0xc68)? & 0x4000 != 0;
        if restricted && self.float(0x1ec)?.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return Ok(true);
        }
        if gear != -1 {
            let previous = self.word(0x55c)?;
            let requested_offset = 0x570u32.wrapping_add((gear as u32).wrapping_mul(4));
            let previous_offset = 0x570u32.wrapping_add(previous.wrapping_mul(4));
            let ratio = div(self.float(requested_offset)?, self.float(previous_offset)?)?;
            let rotation = mul(ratio, self.float(0x548)?)?;
            if rotation > self.float(0x1f9c)? {
                return Ok(false);
            }
        }
        let flags = self.word(0x5e8)?;
        self.store(0x494, 0)?;
        if flags & 0x20 != 0 {
            self.store(0x5e8, (flags & !1) | 2)?;
        }
        self.store(0x560, gear as u32)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> VehicleControlState {
        VehicleControlState::from_bytes(vec![0xa5; 0x2000])
    }

    #[test]
    fn invalid_input_does_not_read_or_change_storage() {
        let mut state = VehicleControlState::from_bytes(vec![]);
        assert_eq!(state.set_steer(1.01), Ok(false));
        assert_eq!(state.set_brake(-0.01), Ok(false));
        assert_eq!(state.set_clutch(f32::INFINITY), Ok(false));
        assert!(state.as_bytes().is_empty());
    }

    #[test]
    fn power_reads_routing_before_resetting_channels() {
        let mut state = VehicleControlState::from_bytes(vec![0xa5; 0x4cc]);
        state.store(0xb8, 0).unwrap();
        let expected = state.clone();
        assert_eq!(
            state.set_power(0.5, false),
            Err(VehicleControlUnavailable::Storage(0x4cc))
        );
        assert_eq!(state, expected);

        let mut state = VehicleControlState::from_bytes(vec![0xa5; 0x4cd]);
        state.store(0xb8, 0).unwrap();
        state.bytes[0x4cc] = 0;
        let mut expected = state.clone();
        expected.store(0x3a8, 0).unwrap();
        assert_eq!(
            state.set_power(0.5, false),
            Err(VehicleControlUnavailable::Storage(0xc2c))
        );
        assert_eq!(state, expected);

        let mut state = VehicleControlState::from_bytes(vec![0xa5; 0x560]);
        state.store(0xb8, 2).unwrap();
        state.bytes[0x4cc] = 1;
        state.store(0x55c, 0).unwrap();
        let mut expected = state.clone();
        expected.store(0x3a8, 0).unwrap();
        assert_eq!(
            state.set_power(0.5, false),
            Err(VehicleControlUnavailable::Storage(0xc2c))
        );
        assert_eq!(state, expected);
    }

    #[test]
    fn raw_clutch_preserves_nan_payload_and_unrelated_storage() {
        let mut state = state();
        state.bytes[0x4ab] = 0;
        let mut expected = state.as_bytes().to_vec();
        let payload = 0x7f81_2345u32;
        expected[0x5e0..0x5e4].copy_from_slice(&payload.to_le_bytes());
        assert_eq!(state.set_clutch(f32::from_bits(payload)), Ok(true));
        assert_eq!(state.as_bytes(), expected);
    }

    #[test]
    fn brake_ramp_retains_first_store_when_later_arithmetic_is_unavailable() {
        let mut state = state();
        state.store(0xb8, 0).unwrap();
        state.bytes[0x4cc] = 0;
        state.bytes[0x4aa] = 0;
        state.store_float(0x4c8, 0.25).unwrap();
        state.store_float(0x3cc, 0.25).unwrap();
        state.store_float(0x3c8, 0.0).unwrap();
        let mut expected = state.as_bytes().to_vec();
        expected[0x3cc..0x3d0].copy_from_slice(&0.0f32.to_le_bytes());
        assert_eq!(
            state.set_brake(0.5),
            Err(VehicleControlUnavailable::NumericDomain)
        );
        assert_eq!(state.as_bytes(), expected);
    }

    #[test]
    fn gear_success_can_be_an_exact_noop() {
        let mut state = state();
        state.store(0x568, 5).unwrap();
        state.store(0x560, 2).unwrap();
        let expected = state.clone();
        assert_eq!(state.set_gear(2), Ok(true));
        assert_eq!(state, expected);
    }

    #[test]
    fn reverse_gear_bypasses_unneeded_ratio_arithmetic() {
        let mut state = state();
        state.store(0x568, 5).unwrap();
        state.store(0x560, 2).unwrap();
        state.store(0xb8, 0).unwrap();
        state.store(0xc68, 0).unwrap();
        state.store(0x5e8, 0xa5a5_a521).unwrap();
        assert_eq!(state.set_gear(-1), Ok(true));
        assert_eq!(state.word(0x560), Ok(u32::MAX));
        assert_eq!(state.word(0x494), Ok(0));
        assert_eq!(state.word(0x5e8), Ok(0xa5a5_a522));
    }

    #[test]
    fn handbrake_ramps_clamps_and_releases_only_original_fields() {
        let mut state = state();
        state.store_float(0x4c8, 0.25).unwrap();
        state.store_float(0x3b8, 0.5).unwrap();
        state.store_float(0x3b4, 1.0).unwrap();
        state.store_float(0x3b0, 20.0).unwrap();
        let mut expected = state.as_bytes().to_vec();
        expected[0x3b8..0x3bc].copy_from_slice(&0.75f32.to_le_bytes());
        expected[0x434..0x438].copy_from_slice(&15.0f32.to_le_bytes());
        assert_eq!(state.set_handbrake(true), Ok(true));
        assert_eq!(state.as_bytes(), expected);
        assert_eq!(state.set_handbrake(true), Ok(true));
        assert_eq!(state.float(0x3b8), Ok(1.0));
        assert_eq!(state.float(0x434), Ok(20.0));
        assert_eq!(state.set_handbrake(true), Ok(true));
        assert_eq!(state.float(0x3b8), Ok(1.0));
        expected[0x3b8..0x3bc].fill(0);
        expected[0x434..0x438].fill(0);
        assert_eq!(state.set_handbrake(false), Ok(true));
        assert_eq!(state.as_bytes(), expected);
    }

    #[test]
    fn handbrake_retains_ramp_prefix_on_numeric_and_storage_frontiers() {
        let mut state = state();
        state.store_float(0x4c8, 0.25).unwrap();
        state.store_float(0x3b8, 0.25).unwrap();
        state.store_float(0x3b4, 0.0).unwrap();
        let mut expected = state.as_bytes().to_vec();
        expected[0x3b8..0x3bc].fill(0);
        assert_eq!(
            state.set_handbrake(true),
            Err(VehicleControlUnavailable::NumericDomain)
        );
        assert_eq!(state.as_bytes(), expected);

        let mut short = VehicleControlState::from_bytes(vec![0xa5; 0x4cb]);
        let expected = short.clone();
        assert_eq!(
            short.set_handbrake(true),
            Err(VehicleControlUnavailable::Storage(0x4c8))
        );
        assert_eq!(short, expected);
        let mut short = VehicleControlState::from_bytes(vec![0xa5; 0x437]);
        let expected = short.clone();
        assert_eq!(
            short.set_handbrake(false),
            Err(VehicleControlUnavailable::Storage(0x434))
        );
        assert_eq!(short, expected);
    }

    #[test]
    fn borrowed_controls_match_owned_setters_in_the_actual_caller_storage() {
        let mut owned = state();
        owned.store(0xb8, 0).unwrap();
        owned.bytes[0x4cc] = 0;
        owned.bytes[0x4aa] = 1;
        owned.bytes[0x4ab] = 0;
        owned.bytes[0x4a8] = 0;
        owned.bytes[0xc2c] = 1;
        owned.store_float(0x610, 0.75).unwrap();
        owned.store_float(0x4c8, 0.25).unwrap();
        owned.store_float(0x3b8, 0.25).unwrap();
        owned.store_float(0x3b4, 1.0).unwrap();
        owned.store_float(0x3b0, 20.0).unwrap();
        owned.store(0x568, 5).unwrap();
        owned.store(0x560, 2).unwrap();
        owned.store(0xc68, 0).unwrap();
        owned.store(0x5e8, 0).unwrap();
        let mut caller_bytes = owned.as_bytes().to_vec();
        {
            let mut borrowed = VehicleControlState::from_bytes_mut(&mut caller_bytes);
            assert_eq!(owned.set_power(0.5, true), borrowed.set_power(0.5, true));
            assert_eq!(owned.set_brake(0.25), borrowed.set_brake(0.25));
            assert_eq!(owned.set_clutch(0.75), borrowed.set_clutch(0.75));
            assert_eq!(owned.set_steer(-0.5), borrowed.set_steer(-0.5));
            assert_eq!(owned.set_handbrake(true), borrowed.set_handbrake(true));
            assert_eq!(owned.set_gear(-1), borrowed.set_gear(-1));
            assert_eq!(owned.as_bytes(), borrowed.as_bytes());
        }
        assert_eq!(owned.as_bytes(), caller_bytes);
    }

    #[test]
    fn borrowed_storage_retains_partial_writes_when_a_later_access_fails() {
        let mut owned = VehicleControlState::from_bytes(vec![0xa5; 0x4cd]);
        owned.store(0xb8, 0).unwrap();
        owned.bytes[0x4cc] = 0;
        let mut caller_bytes = owned.as_bytes().to_vec();
        let expected = Err(VehicleControlUnavailable::Storage(0xc2c));
        assert_eq!(owned.set_power(0.5, true), expected);
        {
            let mut borrowed = VehicleControlState::from_bytes_mut(&mut caller_bytes);
            assert_eq!(borrowed.set_power(0.5, true), expected);
        }
        assert_eq!(owned.as_bytes(), caller_bytes);
        assert_eq!(&caller_bytes[0x3a8..0x3ac], &[0; 4]);
        assert_eq!(&caller_bytes[0x3a4..0x3a8], &[0xa5; 4]);
    }
}
