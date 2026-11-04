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

/// Original I3D_scene::m_UpdateSoundProperties, PC RVA 0x4aaa0 (contract:
/// Ghidra body 380 insn/93 BB: null-listener early return with flags
/// untouched; 1-blend via x87 FSUBR with the IsAbsMrgZero gate
/// (ABS < 0x322bcc77); FUN_1004ea50 blend from InitialSoundProp when prev is
/// null else from prev; EAX3.0 push throttled by the function-static armed
/// flag + last_time via timeGetTime, pushed iff done||now-last>100; __ftol
/// truncations with post-clamps; struct size 0x70 via the indirect
/// vtable+0x24 call; prev then current 0x800 clear with the 0xfffff7ff mask).
/// The indirect SetEAX3Property target and FUN_1004ea50 internals stay
/// backend contracts (partial relationship coverage disclosed: the indirect
/// setter stays a contract; FUN_1004ea50 is bound by SectorSoundBlender /
/// make_sector_sound_blend, PC RVA 0x4ea50).
/// mafia-re I3d_scene.cpp:721-855. Upstream working name, not an evidenced
/// original symbol.
pub const EAX30_FLAG: u32 = 0x20_0000;
pub const SOUND_CHANGED_MASK_CLEAR: u32 = 0xffff_f7ff;

/// Conversion outputs in native evaluation order (mafia-re I3d_scene.cpp
/// order cross-checked with the decompile __ftol/float sequence). The audio
/// backend owns the EAXLISTENERPROPERTIES packing; indices here are NOT
/// claimed to be EAX field offsets (explicit OPEN boundary).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorEaxProperties {
    pub words: [u32; 28],
}
/// timeGetTime analog (native WINMM.DLL import).
pub trait SoundClock {
    fn now_ms(&self) -> u32;
}
/// EAX listener presence (native this+0x284 null guard), EAX3.0 enablement
/// (native create_flags & 0x200000 via DAT_101c529c+0x30) and the indirect
/// vtable+0x24 SetEAX3Property call.
pub trait EaxListenerBackend {
    fn has_listener(&self) -> bool;
    fn eax30_enabled(&self) -> bool;
    fn set_eax_listener(&mut self, props: &SectorEaxProperties);
}
/// FUN_1004ea50 (PC RVA 0x4ea50): bound by make_sector_sound_blend below
/// (differential body); this trait stays the injection seam for the 0x4aaa0
/// caller and its tests.
pub trait SoundPropertyBlender {
    fn blend(&self, out: &mut [u32; 26], from: &[u32; 26], to: &[u32; 26], blend: f32);
}

/// Original S_sector_sound_properties::MakeBlend, PC RVA 0x4ea50
/// (differential: REA ghidra body 726B/236insn/3BB: t1 = 1-t via FSUB,
/// TEST AL all-gate, floats out = b*t + a*t1, int words 2/5/7/12/14 via
/// FILD/FMUL/FADD/__ftol in a*t1 + b*t order, pans at words 20-22/23-25 via
/// stack-temp vb = b*t, va = a*t1 sums, blend_time word 9 stored
/// unconditionally, RET 0x10; mafia-re I3d_sector.cpp:61-97). `from` is a,
/// `to` is b, matching the blender call order (prev-or-initial, current).
/// x87 double-rounding is approximated by the crate f32 pipeline (disclosed,
/// same standard as eax_properties_of); int words truncate toward zero like
/// __ftol (FILD is exact; the host `as f32` rounds only past 2^24, outside
/// the small time ranges in play).
pub fn make_sector_sound_blend(
    out: &mut [u32; 26],
    from: &[u32; 26],
    to: &[u32; 26],
    blend: f32,
    all: bool,
) {
    use crate::native_precision::{add, mul, sub};
    let t1 = sub(1.0, blend);
    if all {
        for index in [0usize, 1, 3, 4, 6, 8, 10, 11, 13, 15, 16, 17, 18, 19] {
            out[index] = add(
                mul(blend, f32::from_bits(to[index])),
                mul(t1, f32::from_bits(from[index])),
            )
            .to_bits();
        }
        for index in [2usize, 5, 7, 12, 14] {
            out[index] = trunc_toward_zero(add(
                mul(from[index] as f32, t1),
                mul(to[index] as f32, blend),
            )) as u32;
        }
        for base in [20usize, 23] {
            for lane in 0..3 {
                out[base + lane] = add(
                    mul(t1, f32::from_bits(from[base + lane])),
                    mul(blend, f32::from_bits(to[base + lane])),
                )
                .to_bits();
            }
        }
    }
    out[9] = trunc_toward_zero(add(mul(from[9] as f32, t1), mul(to[9] as f32, blend))) as u32;
}

/// Production blender for update_sector_sound_properties (PC RVA 0x4aaa0
/// InitialSoundProp/prev arms): the all=true arm plus the unconditional
/// blend_time word, exactly the native call shape.
pub struct SectorSoundBlender;
impl SoundPropertyBlender for SectorSoundBlender {
    fn blend(&self, out: &mut [u32; 26], from: &[u32; 26], to: &[u32; 26], blend: f32) {
        make_sector_sound_blend(out, from, to, blend, true);
    }
}

fn trunc_toward_zero(value: f32) -> i32 {
    // Native __ftol truncation. Inputs are pre-clamped floats, so out-of-range
    // `as` saturation is unreachable.
    debug_assert!(value.is_finite());
    value as i32
}
fn clamp_i(value: i32, lo: i32, hi: i32) -> i32 {
    value.clamp(lo, hi)
}
fn clamp_f(value: f32, lo: f32, hi: f32) -> f32 {
    if value < lo {
        lo
    } else if value > hi {
        hi
    } else {
        value
    }
}

/// Native conversion/clamp table in evaluation order. All float ops run the
/// PC24/RN pipeline under an enclosing Pc24PrecisionState; doubles (e.g.
/// *19000.0) compute in f32 then record — disclosed approximation of x87
/// double temporaries, same standard as the committed weather code.
fn eax_properties_of(props: &[u32; 26]) -> SectorEaxProperties {
    use crate::native_precision::{add, mul, sub};
    let f = |index: usize| f32::from_bits(props[index]);
    let room_lf = clamp_i(
        trunc_toward_zero(add(mul(f(0), 10000.0), -10000.0)),
        -10000,
        0,
    );
    let room_hf = clamp_i(
        trunc_toward_zero(add(mul(f(1), 10000.0), -10000.0)),
        -10000,
        0,
    );
    let decay_time = clamp_f(mul(f(2), 0.001), 0.1, 20.0);
    let decay_hf = clamp_f(add(mul(f(3), 1.9), 0.1), 0.1, 2.0);
    let reflections = clamp_i(
        trunc_toward_zero(sub(mul(f(5), 11000.0), 10000.0)),
        -10000,
        1000,
    );
    let refl_delay = clamp_f(mul(f(6), 0.001), 0.0, 0.3);
    let reverb = clamp_i(
        trunc_toward_zero(sub(mul(f(8), 12000.0), 10000.0)),
        -10000,
        2000,
    );
    let reverb_delay = clamp_f(mul(f(9), 0.001), 0.0, 0.1);
    let env_diff = clamp_f(f(10), 0.0, 1.0);
    let decay_lf = clamp_f(add(mul(f(11), 1.9), 0.1), 0.1, 2.0);
    let room = clamp_i(
        trunc_toward_zero(sub(mul(f(12), 10000.0), 10000.0)),
        -10000,
        0,
    );
    let echo_time = clamp_f(mul(f(14), 0.001), 0.075, 0.25);
    let echo_depth = clamp_f(f(15), 0.0, 1.0);
    let mod_time = clamp_f(mul(f(16), 0.001), 0.04, 4.0);
    let mod_depth = clamp_f(f(17), 0.0, 1.0);
    let rolloff = clamp_f(mul(f(18), 10.0), 0.0, 10.0);
    let air_abs = clamp_f(sub(mul(f(19), 100.0), 100.0), -100.0, 0.0);
    let hf_ref = clamp_f(add(mul(f(20), 19000.0), 1000.0), 1000.0, 20000.0);
    let lf_ref = clamp_f(add(mul(f(21), 980.0), 20.0), 20.0, 1000.0);
    let mut words = [0u32; 28];
    words[0] = room_lf as u32;
    words[1] = room_hf as u32;
    words[2] = decay_time.to_bits();
    words[3] = decay_hf.to_bits();
    words[5] = reflections as u32;
    words[6] = refl_delay.to_bits();
    words[8] = reverb as u32;
    words[9] = reverb_delay.to_bits();
    words[10] = env_diff.to_bits();
    words[11] = decay_lf.to_bits();
    words[12] = room as u32;
    words[14] = echo_time.to_bits();
    words[15] = echo_depth.to_bits();
    words[16] = mod_time.to_bits();
    words[17] = mod_depth.to_bits();
    words[18] = rolloff.to_bits();
    words[19] = air_abs.to_bits();
    words[20] = hf_ref.to_bits();
    words[21] = lf_ref.to_bits();
    // Remaining property words pass through raw; their backend packing is
    // part of the same OPEN boundary as the index map above.
    for index in [4usize, 7, 13, 22, 23, 24, 25] {
        words[index] = props[index];
    }
    SectorEaxProperties { words }
}

impl ModelArena {
    /// m_UpdateSoundProperties over (sector, prev-or-None, blend). Order:
    /// listener-null early return (flags untouched) -> blend select (direct
    /// copy when 1-blend is merge-zero, InitialSoundProp arm when prev is
    /// null, prev arm otherwise) -> throttled EAX push (first call arms,
    /// push iff done||>100ms) -> clear prev then current 0x800.
    /// `initial_sound` is the caller's InitialSoundProp global mirror
    /// (scene_resources initial_sound_properties).
    #[allow(clippy::too_many_arguments)]
    pub fn update_sector_sound_properties(
        &mut self,
        sector: SectorKey,
        prev: Option<SectorKey>,
        blend: f32,
        clock: &impl SoundClock,
        blender: &impl SoundPropertyBlender,
        backend: &mut impl EaxListenerBackend,
        initial_sound: &[u32; 26],
    ) -> Result<(), ModelError> {
        if !backend.has_listener() {
            return Ok(());
        }
        self.sector(sector)?;
        if let Some(previous) = prev {
            self.sector(previous)?;
        }
        let current = self.sector(sector)?.sound_properties;
        let one_minus = crate::native_precision::sub(1.0, blend);
        let done = one_minus.abs() < f32::from_bits(0x322bcc77);
        let mut props = [0u32; 26];
        if done {
            props = current;
        } else if let Some(previous) = prev {
            let from = self.sector(previous)?.sound_properties;
            blender.blend(&mut props, &from, &current, blend);
        } else {
            blender.blend(&mut props, initial_sound, &current, blend);
        }
        if backend.eax30_enabled() {
            let now = clock.now_ms();
            if !self.eax_armed {
                self.eax_armed = true;
                self.eax_last_ms = now;
            }
            if done || now.wrapping_sub(self.eax_last_ms) > 100 {
                backend.set_eax_listener(&eax_properties_of(&props));
                self.eax_last_ms = now;
            }
        }
        if let Some(previous) = prev {
            let index = self
                .sectors
                .iter()
                .position(|entry| entry.key == previous)
                .expect("validated sector owner");
            self.sectors[index].sector_flags &= SOUND_CHANGED_MASK_CLEAR;
        }
        let index = self
            .sectors
            .iter()
            .position(|entry| entry.key == sector)
            .expect("validated sector owner");
        self.sectors[index].sector_flags &= SOUND_CHANGED_MASK_CLEAR;
        Ok(())
    }
}

#[cfg(test)]
mod sound_update_tests {
    use super::*;
    use crate::native_precision::Pc24PrecisionState;

    struct FixedClock(u32);
    impl SoundClock for FixedClock {
        fn now_ms(&self) -> u32 {
            self.0
        }
    }
    struct Rec {
        pushes: u32,
        words: Vec<[u32; 28]>,
        eax30: bool,
        listener: bool,
    }
    impl EaxListenerBackend for Rec {
        fn has_listener(&self) -> bool {
            self.listener
        }
        fn eax30_enabled(&self) -> bool {
            self.eax30
        }
        fn set_eax_listener(&mut self, props: &SectorEaxProperties) {
            self.pushes += 1;
            self.words.push(props.words);
        }
    }

    fn two_sectors(arena: &mut ModelArena) -> (SectorKey, SectorKey) {
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        (sectors[0], sectors[1])
    }

    #[test]
    fn null_listener_returns_before_touching_flags() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let (sector, prev) = two_sectors(&mut arena);
            let si = arena.sector_index(sector).unwrap();
            arena.sectors[si].sector_flags |= 0x800;
            let mut backend = Rec {
                pushes: 0,
                words: Vec::new(),
                eax30: true,
                listener: false,
            };
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    1.0,
                    &FixedClock(0),
                    &SectorSoundBlender,
                    &mut backend,
                    &[0; 26],
                )
                .unwrap();
            assert_eq!(backend.pushes, 0);
            assert_ne!(arena.sector(sector).unwrap().sector_flags & 0x800, 0);
        });
    }

    #[test]
    fn throttle_arms_first_then_pushes_after_100ms_and_done_forces_push() {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let (sector, prev) = two_sectors(&mut arena);
            let mut backend = Rec {
                pushes: 0,
                words: Vec::new(),
                eax30: true,
                listener: true,
            };
            let initial = [0u32; 26];
            // In-progress blend at t=0: arms, no push.
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    0.5,
                    &FixedClock(0),
                    &SectorSoundBlender,
                    &mut backend,
                    &initial,
                )
                .unwrap();
            assert_eq!(backend.pushes, 0);
            // Same tick: still suppressed.
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    0.5,
                    &FixedClock(50),
                    &SectorSoundBlender,
                    &mut backend,
                    &initial,
                )
                .unwrap();
            assert_eq!(backend.pushes, 0);
            // Past 100ms: push.
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    0.5,
                    &FixedClock(101),
                    &SectorSoundBlender,
                    &mut backend,
                    &initial,
                )
                .unwrap();
            assert_eq!(backend.pushes, 1);
            // done forces a push even within the window.
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    1.0,
                    &FixedClock(110),
                    &SectorSoundBlender,
                    &mut backend,
                    &initial,
                )
                .unwrap();
            assert_eq!(backend.pushes, 2);
            // prev then current 0x800 cleared (both set first).
            let si = arena.sector_index(sector).unwrap();
            arena.sectors[si].sector_flags |= 0x800;
            let pi = arena.sector_index(prev).unwrap();
            arena.sectors[pi].sector_flags |= 0x800;
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    1.0,
                    &FixedClock(500),
                    &SectorSoundBlender,
                    &mut backend,
                    &initial,
                )
                .unwrap();
            assert_eq!(arena.sector(sector).unwrap().sector_flags & 0x800, 0);
            assert_eq!(arena.sector(prev).unwrap().sector_flags & 0x800, 0);
            // End-to-end conversion through the production blender: decay
            // word 1000.0 reaches the pushed properties as 1.0.
            let si = arena.sector_index(sector).unwrap();
            arena.sectors[si].sound_properties[2] = 1000f32.to_bits();
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    1.0,
                    &FixedClock(600),
                    &SectorSoundBlender,
                    &mut backend,
                    &initial,
                )
                .unwrap();
            assert_eq!(backend.pushes, 4);
            assert_eq!(backend.words.last().unwrap()[2], 1f32.to_bits());
            // EAX3.0 disabled: never pushes.
            backend.eax30 = false;
            arena
                .update_sector_sound_properties(
                    sector,
                    Some(prev),
                    1.0,
                    &FixedClock(10_000),
                    &SectorSoundBlender,
                    &mut backend,
                    &initial,
                )
                .unwrap();
            assert_eq!(backend.pushes, 4);
        });
    }

    #[test]
    fn conversion_clamp_spots_match_native_table() {
        Pc24PrecisionState::default().run(|| {
            // decay_time word 1000.0 -> 1.0; echo_time 0 -> 0.075 floor.
            let mut props = [0u32; 26];
            props[2] = 1000f32.to_bits();
            props[3] = 1f32.to_bits();
            props[14] = 0u32;
            props[18] = 0.5f32.to_bits();
            let out = eax_properties_of(&props);
            assert_eq!(out.words[2], 1f32.to_bits());
            assert_eq!(out.words[14], 0.075f32.to_bits());
            // room_lf 0.0 -> -10000; decay_hf 1.0 -> 2.0 ceiling.
            assert_eq!(out.words[0], (-10000i32) as u32);
            assert_eq!(out.words[3], 2f32.to_bits());
            assert_eq!(out.words[18], 5f32.to_bits());
        });
    }

    #[test]
    fn make_blend_endpoints_copy_midpoint_truncates_and_keeps_residue() {
        Pc24PrecisionState::default().run(|| {
            let mut from = [0u32; 26];
            let mut to = [0u32; 26];
            from[0] = 0.25f32.to_bits();
            to[0] = 0.75f32.to_bits();
            from[2] = 101;
            to[2] = 100;
            from[9] = 1000;
            to[9] = 2000;
            from[20..23].copy_from_slice(&[2f32.to_bits(), 0, 0]);
            let mut out = [0xdead_beefu32; 26];
            make_sector_sound_blend(&mut out, &from, &to, 0.0, true);
            assert_eq!(out, from);
            make_sector_sound_blend(&mut out, &from, &to, 1.0, true);
            assert_eq!(out, to);
            make_sector_sound_blend(&mut out, &from, &to, 0.5, true);
            assert_eq!(f32::from_bits(out[0]), 0.5f32);
            // (int)(101*0.5 + 100*0.5) = (int)100.5 = 100: x87 truncation arm.
            assert_eq!(out[2], 100);
            assert_eq!(out[9], 1500);
            assert_eq!(&out[20..23], &[1f32.to_bits(), 0, 0]);
            // Every word written: no residue of the sentinel fill.
            assert_ne!(out[25], 0xdead_beef);
        });
    }

    #[test]
    fn make_blend_time_only_arm_writes_word_9_alone() {
        Pc24PrecisionState::default().run(|| {
            let from = [100u32; 26];
            let to = [200u32; 26];
            let mut out = [0u32; 26];
            make_sector_sound_blend(&mut out, &from, &to, 0.5, false);
            // (int)(100*0.5 + 200*0.5) = 150; all other words untouched.
            assert_eq!(out[9], 150);
            for (index, word) in out.iter().enumerate() {
                if index != 9 {
                    assert_eq!(*word, 0);
                }
            }
        });
    }

    #[test]
    fn pan_paths_set_flag_and_skip_scalar_slot_with_residue() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let before = arena.sector(sector).unwrap().sound_properties;
        arena
            .set_sector_sound_data(sector, 20, SectorSoundInput::Pan([2f32.to_bits(), 0, 0]))
            .unwrap();
        arena
            .set_sector_sound_data(sector, 21, SectorSoundInput::Pan([0, 0, 0]))
            .unwrap();
        let row = arena.sector(sector).unwrap();
        assert_eq!(&row.sound_properties[20..23], &[1f32.to_bits(), 0, 0]);
        assert_eq!(&row.sound_properties[23..26], &[0, 0, 0]);
        assert_ne!(row.sector_flags & SOUND_CHANGED, 0);
        // Early pan return performs no scalar store: other words intact.
        for (word, before_word) in before.iter().enumerate() {
            if !(20..26).contains(&word) {
                assert_eq!(row.sound_properties[word], *before_word);
            }
        }
    }
}
