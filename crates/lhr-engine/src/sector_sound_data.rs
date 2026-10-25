//! Original I3D_sector::SetSoundData/GetSoundData, PC RVAs0x4fdb0/0x50140.
//!
//! LS3DF SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Original PS2 symbols corroborate method/index-type names, not PC clamps or
//! layouts. Index member names remain descriptive numbers. This changes the
//! actual retained sector record and its0x800 flag; it calls no audio backend.
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::SectorKey;

pub const SOUND_INVALID_PARAMS: u32 = 0x8000_0007;
pub const SOUND_CHANGED: u32 = 0x800;

/// Native data is a raw DWORD except indices20/21, which dereference a borrowed
/// vector pointer. Typed words replace that pointer without inventing addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorSoundInput {
    Scalar(u32),
    Pan([u32; 3]),
}
#[derive(Debug, PartialEq, Eq)]
pub enum SectorSoundError {
    Owner(ModelError),
    UnsupportedIndex(i32),
    WrongInputKind(i32),
}
/// Native GetSoundData returns the address of the live pan, not its first word.
/// The borrow belongs to the same sector record and prevents owner mutation.
#[derive(Debug, PartialEq, Eq)]
pub enum SectorSoundValue<'a> {
    Scalar(u32),
    Pan(&'a [u32; 3]),
}

impl ModelArena {
    /// PC24/RN arithmetic; an enclosing Pc24PrecisionState records sticky
    /// precision. Other x87 status bits and unmasked exceptions are not modeled.
    /// The original signed upper guard permits negative indices to write before
    /// the record. Those unsafe writes are explicitly outside host admission.
    pub fn set_sector_sound_data(
        &mut self,
        sector: SectorKey,
        index: i32,
        data: SectorSoundInput,
    ) -> Result<u32, SectorSoundError> {
        if index >= 22 {
            return Ok(SOUND_INVALID_PARAMS);
        }
        if index < 0 {
            return Err(SectorSoundError::UnsupportedIndex(index));
        }
        self.sector(sector).map_err(SectorSoundError::Owner)?;
        let words = match (index, data) {
            (20 | 21, SectorSoundInput::Pan(words)) => normal_pan(words),
            (20 | 21, _) => return Err(SectorSoundError::WrongInputKind(index)),
            (_, SectorSoundInput::Scalar(word)) => [scalar(index, word), 0, 0],
            (_, _) => return Err(SectorSoundError::WrongInputKind(index)),
        };
        let owner = self
            .sectors
            .iter_mut()
            .find(|s| s.key == sector)
            .expect("validated sector owner");
        match index {
            20 => owner.sound_properties[20..23].copy_from_slice(&words),
            21 => owner.sound_properties[23..26].copy_from_slice(&words),
            _ => owner.sound_properties[index as usize] = words[0],
        }
        owner.sector_flags |= SOUND_CHANGED;
        Ok(0)
    }

    /// The original getter has no bounds check. Only existing owned words are
    /// exposed: sound record0..25, flags26 and separately owned words32/33.
    /// Unsupported adjacent memory is a host error, not a native return code.
    pub fn get_sector_sound_data(
        &self,
        sector: SectorKey,
        index: i32,
    ) -> Result<SectorSoundValue<'_>, SectorSoundError> {
        let owner = self.sector(sector).map_err(SectorSoundError::Owner)?;
        Ok(match index {
            20 => SectorSoundValue::Pan(owner.sound_properties[20..23].try_into().unwrap()),
            21 => SectorSoundValue::Pan(owner.sound_properties[23..26].try_into().unwrap()),
            0..=25 => SectorSoundValue::Scalar(owner.sound_properties[index as usize]),
            26 => SectorSoundValue::Scalar(owner.sector_flags),
            32..=33 => SectorSoundValue::Scalar(owner.sound_words_1e8_1ec[(index - 32) as usize]),
            _ => return Err(SectorSoundError::UnsupportedIndex(index)),
        })
    }
}

fn scalar(index: i32, word: u32) -> u32 {
    match index {
        2 => (word as i32).clamp(100, 20_000) as u32,
        5 => (word as i32).clamp(0, 300) as u32,
        7 => (word as i32).clamp(0, 100) as u32,
        12 => (word as i32).clamp(75, 250) as u32,
        14 => (word as i32).clamp(39, 4_000) as u32,
        9 => word,
        _ => {
            let value = f32::from_bits(word);
            // Bounds compare against the input. Unordered comparisons retain
            // its original raw bits, including signalling NaN and signed zero.
            if value < 0.0 {
                0
            } else if value > 1.0 {
                1f32.to_bits()
            } else {
                word
            }
        }
    }
}

// All finite f32 products and this short PC24 sum fit binary64's exponent.
// Round each arithmetic operation to24 significand bits before explicit f32
// spills. This preserves x87's wider exponent and subnormal double rounding.
fn pc24(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    let bits = value.to_bits();
    let magnitude = bits & 0x7fff_ffff_ffff_ffff;
    let discarded = magnitude & ((1u64 << 29) - 1);
    let mut retained = magnitude >> 29;
    if discarded > 1u64 << 28 || discarded == 1u64 << 28 && retained & 1 != 0 {
        retained += 1;
    }
    f64::from_bits((bits & (1u64 << 63)) | (retained << 29))
}
fn mul(a: f64, b: f64) -> f64 {
    let value = pc24(a * b);
    if a.is_finite() && b.is_finite() && value.is_finite() {
        crate::native_precision::record_wide_pc24_mul(a, b, value);
    }
    value
}
fn add(a: f64, b: f64) -> f64 {
    let value = pc24(a + b);
    if a.is_finite() && b.is_finite() && value.is_finite() {
        crate::native_precision::record_wide_pc24_add(a, b, value);
    }
    value
}
fn spill(value: f64) -> f32 {
    let narrowed = value as f32;
    if value.is_finite() {
        crate::native_precision::record_precision(f64::from(narrowed) != value);
    }
    narrowed
}
fn normal_pan(words: [u32; 3]) -> [u32; 3] {
    let [x, y, z] = words.map(|w| f64::from(f32::from_bits(w)));
    // The actual instructions multiplyZ, multiplyY, add, multiplyX, add.
    let zz = mul(z, z);
    let yy = mul(y, y);
    let zy = add(zz, yy);
    let xx = mul(x, x);
    let squared = spill(add(zy, xx));
    // NaN intermediates do not cause scaling; original component bits survive.
    // Independent finite products above can still have raised precision.
    if words.into_iter().any(|w| f32::from_bits(w).is_nan()) {
        return words;
    }
    if (squared.to_bits() as i32) < 0x322b_cc77 {
        return words;
    }
    let delta = pc24(f64::from(squared) - 1.0);
    if squared.is_finite() {
        crate::native_precision::record_wide_pc24_sub(f64::from(squared), 1.0, delta);
    }
    if delta.abs() < f64::from_bits(0x3e45_798e_e000_0000) {
        return words;
    }
    let magnitude = squared.sqrt();
    if squared.is_finite() {
        crate::native_precision::record_sqrt(squared, magnitude);
    }
    if magnitude <= 1.0 {
        return words;
    }
    let reciprocal = 1.0 / magnitude;
    if magnitude.is_finite() {
        crate::native_precision::record_div(1.0, magnitude, reciprocal);
    }
    words.map(|word| {
        let component = f32::from_bits(word);
        if component.is_infinite() && reciprocal == 0.0 {
            0xffc0_0000
        } else {
            spill(mul(f64::from(component), f64::from(reciprocal))).to_bits()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_precision::Pc24PrecisionState;
    fn owner() -> (ModelArena, SectorKey) {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let row = arena.sectors.iter_mut().find(|s| s.key == sector).unwrap();
        row.sector_flags = 0xa5000100;
        row.sound_words_1e8_1ec = [0x12345678, 0x87654321];
        (arena, sector)
    }
    #[test]
    fn scalar_clamps_preserve_raw_nan_zero_and_unrelated_owned_words() {
        for (index, input, expected) in [
            (0, (-0.0f32).to_bits(), (-0.0f32).to_bits()),
            (1, 0x7fa12345, 0x7fa12345),
            (3, 0xffc12345, 0xffc12345),
            (4, f32::NEG_INFINITY.to_bits(), 0),
            (6, f32::INFINITY.to_bits(), 1f32.to_bits()),
            (2, u32::MAX, 100),
            (2, 30000, 20000),
            (5, u32::MAX, 0),
            (5, 301, 300),
            (7, 101, 100),
            (12, 0, 75),
            (12, 251, 250),
            (14, 0, 39),
            (14, 4001, 4000),
            (9, 0xdeadbeef, 0xdeadbeef),
        ] {
            let (mut arena, sector) = owner();
            let before = arena.sector(sector).unwrap().sound_properties;
            assert_eq!(
                arena
                    .set_sector_sound_data(sector, index, SectorSoundInput::Scalar(input))
                    .unwrap(),
                0
            );
            let row = arena.sector(sector).unwrap();
            assert_eq!(row.sound_properties[index as usize], expected);
            assert_eq!(row.sector_flags, 0xa5000900);
            assert_eq!(row.sound_words_1e8_1ec, [0x12345678, 0x87654321]);
            for (word, before_word) in before.iter().enumerate() {
                if word != index as usize {
                    assert_eq!(row.sound_properties[word], *before_word);
                }
            }
        }
    }
    #[test]
    fn native_invalid_upper_index_and_unsafe_host_indices_are_distinct() {
        let (mut arena, sector) = owner();
        let before = arena.sector(sector).unwrap().clone();
        for index in [22, 100, i32::MAX] {
            assert_eq!(
                arena
                    .set_sector_sound_data(sector, index, SectorSoundInput::Pan([u32::MAX; 3]))
                    .unwrap(),
                SOUND_INVALID_PARAMS
            );
        }
        for index in [-1, i32::MIN] {
            assert_eq!(
                arena.set_sector_sound_data(sector, index, SectorSoundInput::Scalar(7)),
                Err(SectorSoundError::UnsupportedIndex(index))
            );
        }
        assert_eq!(
            arena.set_sector_sound_data(sector, 20, SectorSoundInput::Scalar(0)),
            Err(SectorSoundError::WrongInputKind(20))
        );
        assert_eq!(
            arena.set_sector_sound_data(sector, 9, SectorSoundInput::Pan([0; 3])),
            Err(SectorSoundError::WrongInputKind(9))
        );
        let after = arena.sector(sector).unwrap();
        assert_eq!(after.sound_properties, before.sound_properties);
        assert_eq!(after.sector_flags, before.sector_flags);
        assert_eq!(after.sound_words_1e8_1ec, before.sound_words_1e8_1ec);
        for index in [-1, 27, 31, 34, i32::MAX] {
            assert_eq!(
                arena.get_sector_sound_data(sector, index),
                Err(SectorSoundError::UnsupportedIndex(index))
            );
        }
    }
    #[test]
    fn pans_copy_or_scale_both_owned_vectors_without_touching_other_fields() {
        for index in [20, 21] {
            for input in [
                [-0.0, 0.0, -0.0],
                [0.25, 0.0, -0.5],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
            ] {
                let (mut arena, sector) = owner();
                let before = arena.sector(sector).unwrap().sound_properties;
                let words = input.map(f32::to_bits);
                let expected = if input[0] == 2.0 {
                    [1f32.to_bits(), 0, 0]
                } else {
                    words
                };
                Pc24PrecisionState::default()
                    .run(|| {
                        arena.set_sector_sound_data(sector, index, SectorSoundInput::Pan(words))
                    })
                    .unwrap();
                let start = if index == 20 { 20 } else { 23 };
                let row = arena.sector(sector).unwrap();
                assert_eq!(row.sound_properties[start..start + 3], expected);
                for (word, before_word) in before.iter().enumerate() {
                    if !(start..start + 3).contains(&word) {
                        assert_eq!(row.sound_properties[word], *before_word);
                    }
                }
            }
        }
    }
    #[test]
    fn pan_extended_exponent_spill_and_nonfinite_results_follow_native_profile() {
        let (mut arena, sector) = owner();
        let mut precision = Pc24PrecisionState::default();
        precision
            .run(|| {
                arena.set_sector_sound_data(
                    sector,
                    20,
                    SectorSoundInput::Pan([f32::MAX.to_bits(), 1f32.to_bits(), (-0f32).to_bits()]),
                )
            })
            .unwrap();
        assert_eq!(
            &arena.sector(sector).unwrap().sound_properties[20..23],
            &[0, 0, (-0f32).to_bits()]
        );
        assert!(precision.precision_raised());
        let mut exact = Pc24PrecisionState::default();
        exact
            .run(|| {
                arena.set_sector_sound_data(
                    sector,
                    21,
                    SectorSoundInput::Pan([f32::INFINITY.to_bits(), 0, 0]),
                )
            })
            .unwrap();
        assert_eq!(
            &arena.sector(sector).unwrap().sound_properties[23..26],
            &[0xffc00000, 0, 0]
        );
        assert!(!exact.precision_raised());
        let words = [0x7fa12345, (-0f32).to_bits(), 1f32.to_bits()];
        let mut unchanged = Pc24PrecisionState::default();
        unchanged
            .run(|| arena.set_sector_sound_data(sector, 20, SectorSoundInput::Pan(words)))
            .unwrap();
        assert_eq!(
            &arena.sector(sector).unwrap().sound_properties[20..23],
            &words
        );
        assert!(!unchanged.precision_raised());
    }
    #[test]
    fn getters_borrow_pan_identity_and_read_actual_backed_adjacent_words() {
        let (mut arena, sector) = owner();
        let words = [0, 1f32.to_bits(), 0];
        arena
            .set_sector_sound_data(sector, 20, SectorSoundInput::Pan(words))
            .unwrap();
        match arena.get_sector_sound_data(sector, 20).unwrap() {
            SectorSoundValue::Pan(pan) => {
                assert_eq!(*pan, words);
                assert_eq!(
                    pan.as_ptr(),
                    arena.sector(sector).unwrap().sound_properties[20..].as_ptr()
                );
            }
            _ => panic!("native pan address must remain a borrowed vector"),
        }
        for index in 22..=25 {
            assert_eq!(
                arena.get_sector_sound_data(sector, index).unwrap(),
                SectorSoundValue::Scalar(
                    arena.sector(sector).unwrap().sound_properties[index as usize]
                )
            );
        }
        assert_eq!(
            arena.get_sector_sound_data(sector, 26).unwrap(),
            SectorSoundValue::Scalar(0xa5000900)
        );
        assert_eq!(
            arena.get_sector_sound_data(sector, 32).unwrap(),
            SectorSoundValue::Scalar(0x12345678)
        );
        assert_eq!(
            arena.get_sector_sound_data(sector, 33).unwrap(),
            SectorSoundValue::Scalar(0x87654321)
        );
    }
}
