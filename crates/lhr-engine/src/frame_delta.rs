//! Bounded pure-state Game delta modifier, independent of the Game frame loop.
//!
//! Original Game SHA `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`,
//! RVA `0x19b130`, evidence `PC-GAME-PHASE-STATIC-20261021`.
//! Explicit masked x87 precision/rounding is modeled with integers. Floating
//! exception/status flags, unmasked traps and invalid/aliasing memory are excluded.
//! `Default` describes host-adapter storage, not a recovered native constructor.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpeedRecord {
    pub raw_timestamp: u32,
    pub derived_base: u32,
    pub multiplier_bits: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameDeltaState {
    pub mode: u32,
    pub raw_accumulated: u32,
    pub derived_position: u32,
    pub stored_multiplier_bits: u32,
    pub current_index: usize,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModifierError {
    MissingDelta,
    InvalidRecordIndex,
    UnmaskedExceptions,
    ReservedPrecision,
}

#[derive(Clone, Copy)]
struct FloatPolicy {
    precision: u32,
    rounding: u16,
}

impl FloatPolicy {
    fn from_control_word(word: u16) -> Result<Self, ModifierError> {
        if word & 0x3f != 0x3f {
            return Err(ModifierError::UnmaskedExceptions);
        }
        let precision = match (word >> 8) & 3 {
            0 => 24,
            2 => 53,
            3 => 64,
            _ => return Err(ModifierError::ReservedPrecision),
        };
        Ok(Self {
            precision,
            rounding: (word >> 10) & 3,
        })
    }

    /// Return the low32 of truncation to signed64, including masked invalid=>0.
    fn multiply_low32(self, magnitude: u32, integer_negative: bool, bits: u32) -> u32 {
        let exponent_field = (bits >> 23) & 0xff;
        if exponent_field == 0xff {
            return 0; // NaN/infinity, including zero times infinity
        }
        let negative = integer_negative ^ (bits >> 31 != 0);
        let fraction = bits & 0x7fffff;
        let (significand, exponent) = if exponent_field == 0 {
            (fraction, -149)
        } else {
            (fraction | 0x800000, exponent_field as i32 - 150)
        };
        let mut product = u64::from(magnitude) * u64::from(significand);
        let width = 64 - product.leading_zeros();
        if width > self.precision {
            let discard = width - self.precision;
            let mask = (1u64 << discard) - 1;
            let remainder = product & mask;
            let quotient = product >> discard;
            let increment = match self.rounding {
                0 => {
                    let half = 1u64 << (discard - 1);
                    remainder > half || (remainder == half && quotient & 1 != 0)
                }
                1 => negative && remainder != 0,
                2 => !negative && remainder != 0,
                _ => false,
            };
            product = (quotient + u64::from(increment)) << discard;
        }
        let integer = if exponent >= 0 {
            let shift = exponent as u32;
            if product == 0 {
                0
            } else if (64 - product.leading_zeros()) + shift > 64 {
                return 0;
            } else {
                product << shift
            }
        } else {
            let shift = (-exponent) as u32;
            if shift >= 64 {
                0
            } else {
                product >> shift
            }
        };
        let limit = if negative {
            1u64 << 63
        } else {
            i64::MAX as u64
        };
        if integer > limit {
            return 0;
        }
        if negative {
            integer.wrapping_neg() as u32
        } else {
            integer as u32
        }
    }
}

impl FrameDeltaState {
    /// Apply the modifier within its valid typed memory and masked-FPU domain.
    /// Mode4 accepts no delta and performs no accesses or writes. For other modes,
    /// errors after accumulation retain that prior write; no rollback is implied.
    pub fn apply(
        &mut self,
        delta: Option<&mut u32>,
        records: &[SpeedRecord],
        control_word: u16,
    ) -> Result<(), ModifierError> {
        if self.mode == 4 {
            return Ok(());
        }
        let delta = delta.ok_or(ModifierError::MissingDelta)?;
        self.raw_accumulated = self.raw_accumulated.wrapping_add(*delta);
        if !self.enabled || records.is_empty() {
            self.derived_position = self.raw_accumulated;
            self.stored_multiplier_bits = 0x3f800000;
            return Ok(());
        }
        if self.current_index >= records.len() {
            return Err(ModifierError::InvalidRecordIndex);
        }
        let policy = FloatPolicy::from_control_word(control_word)?;
        while self.current_index < records.len() - 1
            && records[self.current_index + 1].raw_timestamp < self.raw_accumulated
        {
            self.current_index += 1;
        }
        let record = records[self.current_index];
        let offset = self.raw_accumulated.wrapping_sub(record.raw_timestamp);
        self.derived_position = policy
            .multiply_low32(offset, false, record.multiplier_bits)
            .wrapping_add(record.derived_base);
        let bits = record.multiplier_bits;
        self.stored_multiplier_bits = if bits & 0x7f800000 == 0x7f800000 && bits & 0x7fffff != 0 {
            bits | 0x400000
        } else {
            bits
        };
        let signed_delta = *delta as i32;
        *delta = policy.multiply_low32(signed_delta.unsigned_abs(), signed_delta < 0, bits);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(bits: u32) -> SpeedRecord {
        SpeedRecord {
            raw_timestamp: 0,
            derived_base: 0,
            multiplier_bits: bits,
        }
    }

    #[test]
    fn mode_four_never_requires_delta_or_valid_track_or_fpu() {
        let mut state = FrameDeltaState {
            mode: 4,
            enabled: true,
            current_index: usize::MAX,
            raw_accumulated: 17,
            stored_multiplier_bits: 0x7f800001,
            ..Default::default()
        };
        let before = state;
        assert_eq!(state.apply(None, &[], 0), Ok(()));
        assert_eq!(state, before);
    }

    #[test]
    fn disabled_or_empty_tracks_accumulate_without_scaling() {
        for (enabled, track) in [(false, vec![record(0x7fc00000)]), (true, vec![])] {
            let mut state = FrameDeltaState {
                enabled,
                current_index: 99,
                raw_accumulated: u32::MAX - 2,
                ..Default::default()
            };
            let mut delta = 5;
            state.apply(Some(&mut delta), &track, 0).unwrap();
            assert_eq!(state.raw_accumulated, 2);
            assert_eq!(state.derived_position, 2);
            assert_eq!(state.stored_multiplier_bits, 0x3f800000);
            assert_eq!(state.current_index, 99);
            assert_eq!(delta, 5);
        }
    }

    #[test]
    fn strict_timestamp_boundary_and_wrapping_time_never_rewind() {
        let track = [
            SpeedRecord {
                raw_timestamp: 0,
                derived_base: 100,
                ..record(0x3f800000)
            },
            SpeedRecord {
                raw_timestamp: 10,
                derived_base: 200,
                ..record(0x3f800000)
            },
            SpeedRecord {
                raw_timestamp: 20,
                derived_base: 300,
                ..record(0x3f800000)
            },
        ];
        let mut state = FrameDeltaState {
            enabled: true,
            ..Default::default()
        };
        for (input, index, position) in [(10, 0, 110), (1, 1, 201), (20, 2, 311)] {
            let mut delta = input;
            state.apply(Some(&mut delta), &track, 0x033f).unwrap();
            assert_eq!(state.current_index, index);
            assert_eq!(state.derived_position, position);
            assert_eq!(delta, input);
        }
        state.raw_accumulated = u32::MAX;
        let mut delta = 2;
        state.apply(Some(&mut delta), &track, 0x033f).unwrap();
        assert_eq!(state.current_index, 2);
        assert_eq!(state.raw_accumulated, 1);
        assert_eq!(state.derived_position, 281);
    }

    #[test]
    fn offset_is_unsigned_but_delta_is_signed_before_scaling() {
        let mut state = FrameDeltaState {
            enabled: true,
            ..Default::default()
        };
        let mut delta = u32::MAX;
        state
            .apply(Some(&mut delta), &[record(0x3f000000)], 0x033f)
            .unwrap();
        assert_eq!(state.derived_position, 0x7fffffff);
        assert_eq!(delta, 0); // signed -1 * .5 truncates toward zero
    }

    #[test]
    fn precision_and_signed_directed_rounding_precede_truncation() {
        for (word, positive, negative) in [
            (0x003f, 16777216, -16777216i32),
            (0x043f, 16777216, -16777218),
            (0x083f, 16777218, -16777216),
            (0x0c3f, 16777216, -16777216),
        ] {
            let policy = FloatPolicy::from_control_word(word).unwrap();
            assert_eq!(policy.multiply_low32(16777217, false, 0x3f800000), positive);
            assert_eq!(
                policy.multiply_low32(16777217, true, 0x3f800000),
                negative as u32
            );
        }
        let nearest = FloatPolicy::from_control_word(0x003f).unwrap();
        assert_eq!(
            nearest.multiply_low32(16777219, false, 0x3f800000),
            16777220
        );
        for precision in [0x0200, 0x0300] {
            for rounding in [0, 0x0400, 0x0800, 0x0c00] {
                let policy = FloatPolicy::from_control_word(0x003f | precision | rounding).unwrap();
                assert_eq!(policy.multiply_low32(16777217, false, 0x3f800000), 16777217);
            }
        }
    }

    #[test]
    fn nonfinite_and_out_of_signed64_range_convert_to_indefinite_low_zero() {
        let policy = FloatPolicy::from_control_word(0x033f).unwrap();
        for bits in [0x7f800000, 0xff800000, 0x7fc12345, 0x7f800001] {
            for magnitude in [0, 1, u32::MAX] {
                assert_eq!(policy.multiply_low32(magnitude, false, bits), 0);
            }
        }
        // This mathematical integer has nonzero low bits, but exceeds signed64.
        assert_eq!(policy.multiply_low32(u32::MAX, false, 0x4f000001), 0);
        assert_eq!(policy.multiply_low32(1, true, 0x5f000000), 0); // valid MIN64 low32
    }

    #[test]
    fn stored_nan_payload_is_quieted_and_signed_zero_bits_survive() {
        for (bits, stored) in [
            (0x7f800001, 0x7fc00001),
            (0xff812345, 0xffc12345),
            (0x7fc12345, 0x7fc12345),
            (0x80000000, 0x80000000),
            (1, 1),
        ] {
            let mut state = FrameDeltaState {
                enabled: true,
                ..Default::default()
            };
            let mut delta = 1;
            state
                .apply(Some(&mut delta), &[record(bits)], 0x033f)
                .unwrap();
            assert_eq!(state.stored_multiplier_bits, stored);
            assert_eq!(delta, 0);
            assert_eq!(state.derived_position, 0);
        }
    }

    #[test]
    fn safe_domain_errors_preserve_only_the_prior_accumulator_write() {
        let mut state = FrameDeltaState {
            enabled: true,
            current_index: 1,
            derived_position: 99,
            stored_multiplier_bits: 77,
            ..Default::default()
        };
        let mut delta = 5;
        assert_eq!(
            state.apply(Some(&mut delta), &[record(0x3f800000)], 0x033f),
            Err(ModifierError::InvalidRecordIndex)
        );
        assert_eq!(state.raw_accumulated, 5);
        assert_eq!(state.derived_position, 99);
        assert_eq!(state.stored_multiplier_bits, 77);
        assert_eq!(delta, 5);
        state.current_index = 0;
        assert_eq!(
            state.apply(Some(&mut delta), &[record(0x3f800000)], 0),
            Err(ModifierError::UnmaskedExceptions)
        );
        assert_eq!(state.raw_accumulated, 10);
        assert_eq!(
            state.apply(Some(&mut delta), &[record(0x3f800000)], 0x013f),
            Err(ModifierError::ReservedPrecision)
        );
        assert_eq!(state.raw_accumulated, 15);
        let before = state;
        assert_eq!(
            state.apply(None, &[], 0x033f),
            Err(ModifierError::MissingDelta)
        );
        assert_eq!(state, before);
    }
}
