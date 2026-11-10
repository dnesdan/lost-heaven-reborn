//! Live LENS scalar parameters. LS3DF SHA e211c850… constructor RVA09520;
//! Duplicate RVA096e0 copies five of these fields, leaving intensity intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LensflareParameterUnavailable {
    ScalarInput,
    ExistingFadeIn,
    ExistingFadeOut,
    IntervalConversion,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensflareParameters {
    pub size: f32,
    pub intensity: f32,
    pub test_interval: u32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub range: f32,
}

impl LensflareParameters {
    pub const fn constructed() -> Self {
        Self {
            size: 1.,
            intensity: 1.,
            test_interval: 150,
            fade_in: 15.,
            fade_out: 8.,
            range: 300.,
        }
    }

    /// Original096e0 copies raw words; destination intensity is not assigned.
    pub(crate) fn duplicate_from(&mut self, source: Self) {
        self.test_interval = source.test_interval;
        self.size = source.size;
        self.fade_in = source.fade_in;
        self.fade_out = source.fade_out;
        self.range = source.range;
    }

    /// Scene2's recognized scalar stores occur after their child CloseChunk.
    /// This availability domain covers ordinary finite FLD/FSTP values only.
    pub(crate) fn assign(
        &mut self,
        parameter: LensflareParameter,
        value: f32,
    ) -> Result<(), LensflareParameterUnavailable> {
        fn ordinary(value: f32) -> Result<f32, LensflareParameterUnavailable> {
            if value == 0. || value.is_normal() {
                Ok(value)
            } else {
                Err(LensflareParameterUnavailable::ScalarInput)
            }
        }
        match parameter {
            LensflareParameter::Size => self.size = ordinary(value)?,
            LensflareParameter::Intensity => self.intensity = ordinary(value)?,
            LensflareParameter::TestInterval => {
                let value = ordinary(value)?;
                // Game6109d0 __ftol: signed64 truncation, then EAX low32.
                if f64::from(value) < i64::MIN as f64
                    || f64::from(value) >= 9_223_372_036_854_775_808.
                {
                    return Err(LensflareParameterUnavailable::IntervalConversion);
                }
                crate::native_precision::record_precision(value != value.trunc());
                self.test_interval = value.trunc() as i64 as u32;
            }
            LensflareParameter::FadeIn => {
                self.fade_in = ordinary(value)?;
                // Native543fe9 reloads/stores the other live fade AFTER this store.
                self.fade_out = ordinary(self.fade_out)
                    .map_err(|_| LensflareParameterUnavailable::ExistingFadeOut)?;
            }
            LensflareParameter::FadeOut => {
                // Native544016 reloads/stores the other fade BEFORE the new store.
                self.fade_in = ordinary(self.fade_in)
                    .map_err(|_| LensflareParameterUnavailable::ExistingFadeIn)?;
                self.fade_out = ordinary(value)?;
            }
            LensflareParameter::Range => self.range = ordinary(value)?,
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LensflareParameter {
    Size,
    Intensity,
    TestInterval,
    FadeIn,
    FadeOut,
    Range,
}

/// 0x9630 forward-release cursor (mafia-re I3d_billboard.cpp:884-895).
/// Native walks glows[0..n) forward, null-guarded Release per material.
/// Host cursor makes the order observable across interrupted cleanups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LensflareReleaseCursor(pub usize);
impl LensflareReleaseCursor {
    pub fn next_release(&mut self, glow_count: usize) -> Option<usize> {
        if self.0 < glow_count {
            let i = self.0;
            self.0 += 1;
            Some(i)
        } else {
            None
        }
    }
}

/// 0xac90 glow admission (contract; I3d_billboard.cpp:1180-1206). Widths from
/// dtaRead: num u8(1), pos 4 floats, id i16(2). Native errors only on
/// `id > LoadMaterialu` and reads `LoadMaterialy[id-1]` unchecked otherwise
/// (id 0/negative is an OOB read with no error). The host refuses id <= 0 as
/// well — disclosed hardening, bit-exact on all native-reachable inputs.
/// The caller still pushes the glow slot on error, and the fixed bound is
/// installed whenever the file count was nonzero (even if every id was
/// invalid), not only when a glow was admitted. No in-tree caller wires this
/// residue yet (pure helper + re-export; contract tier holds regardless).
pub const LENSFLARE_FIXED_RADIUS: f32 = 0.01;
pub const LENSFLARE_FIXED_EXTENT: f32 = 0.05;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LensflareGlowError {
    FileCorrupted(i32),
}
pub fn lensflare_admit_glow(
    material_count: usize,
    material_id: i32,
) -> Result<Option<usize>, LensflareGlowError> {
    if material_id <= 0 || (material_id as usize) > material_count {
        return Err(LensflareGlowError::FileCorrupted(material_id));
    }
    Ok(Some((material_id as usize) - 1))
}

/// 0x9890 draw fade plan (contract; I3d_billboard.cpp:959-1175). Pure portion
/// only: dt==0 no-op (native `if (time-last_update)`), else a LINEAR
/// min/max approach (not exponential) with step `(dt*rate)*0.001` in host
/// f32 (native x87 order; disclosed). Rate select is the native
/// IsMrgZeroLess bit-test (`(bits as i32) < 0x322BCC77` → fade_out), so tiny
/// positive targets take the fade_out arm, unlike `<= 0.0`. Occlusion ray,
/// viewport projection, texture/draw stay backend contracts (no live device).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensflareFadePlan {
    pub visibility: f32,
    pub color: [f32; 3],
}
/// Marginal-zero bit threshold, same native helper as the projector/shadow
/// passes (IsMrgZeroLess).
const LENSFLARE_MRG_ZERO_BITMASK: i32 = 0x322b_cc77;
fn is_lensflare_mrg_zero_less(f: f32) -> bool {
    (f.to_bits() as i32) < LENSFLARE_MRG_ZERO_BITMASK
}
pub fn lensflare_fade_step(
    visibility: f32,
    target_visibility: f32,
    color: [f32; 3],
    target_color: [f32; 3],
    dt_ms: u32,
    fade_in: f32,
    fade_out: f32,
) -> Option<LensflareFadePlan> {
    if dt_ms == 0 {
        return None;
    }
    let rate = if is_lensflare_mrg_zero_less(target_visibility) {
        fade_out
    } else {
        fade_in
    };
    let step = (dt_ms as f32 * rate) * 0.001;
    let approach = |v: f32, t: f32| {
        if v < t {
            (v + step).min(t)
        } else if v > t {
            (v - step).max(t)
        } else {
            v
        }
    };
    Some(LensflareFadePlan {
        visibility: approach(visibility, target_visibility),
        color: [
            approach(color[0], target_color[0]),
            approach(color[1], target_color[1]),
            approach(color[2], target_color[2]),
        ],
    })
}
/// Working name for the game-only inlined `I3D_lensflare::SetSize`
/// (mafia-re ls3df/I3D/I3D.h:3019-3023 at pin 113b5b6, `#ifndef
/// LS3DF_EXPORTS`). The reference records no LS3DF FUNCTION address and no
/// Game.exe out-of-line copy for it; the game inlines the plain store.
/// Stores the size with no validation; the validated scene2 intake stays in
/// `LensflareParameters::assign`. Pure, no device.
pub fn lensflare_plain_set_size(params: &mut LensflareParameters, value: f32) {
    params.size = value;
}
/// Working name for the game-only inlined `I3D_lensflare::SetIntensity`
/// (mafia-re ls3df/I3D/I3D.h:3025-3028 at pin 113b5b6, `#ifndef
/// LS3DF_EXPORTS`). The reference records no LS3DF FUNCTION address and no
/// Game.exe out-of-line copy for it. Stores the intensity with no
/// validation; pure, no device.
pub fn lensflare_plain_set_intensity(params: &mut LensflareParameters, value: f32) {
    params.intensity = value;
}
/// Working name for the game-only inlined `I3D_lensflare::SetTestInterval`
/// (mafia-re ls3df/I3D/I3D.h:3029-3032 at pin 113b5b6, `#ifndef
/// LS3DF_EXPORTS`). The reference records no LS3DF FUNCTION address and no
/// Game.exe out-of-line copy for it. Stores the interval with no float
/// conversion; the validated `__ftol` path stays in `assign`. Pure.
pub fn lensflare_plain_set_test_interval(params: &mut LensflareParameters, value: u32) {
    params.test_interval = value;
}
/// Working name for the game-only inlined `I3D_lensflare::SetFade`
/// (mafia-re ls3df/I3D/I3D.h:3033-3039 at pin 113b5b6, `#ifndef
/// LS3DF_EXPORTS`). The reference records no LS3DF FUNCTION address and no
/// Game.exe out-of-line copy for it. Stores both fades in parameter order
/// with no reachability check; the validated load path that preserves the
/// reached store prefix stays in `assign`. Pure, no device.
pub fn lensflare_plain_set_fade(params: &mut LensflareParameters, fade_in: f32, fade_out: f32) {
    params.fade_in = fade_in;
    params.fade_out = fade_out;
}
/// Working name for the game-only inlined `I3D_lensflare::SetRange`
/// (mafia-re ls3df/I3D/I3D.h:3041-3044 at pin 113b5b6, `#ifndef
/// LS3DF_EXPORTS`). The reference records no LS3DF FUNCTION address and no
/// Game.exe out-of-line copy for it. Stores the range with no validation;
/// pure, no device.
pub fn lensflare_plain_set_range(params: &mut LensflareParameters, value: f32) {
    params.range = value;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn y5_lensflare_ctor_dtor_admit_draw_contracts() {
        let c = LensflareParameters::constructed();
        assert_eq!(
            (
                c.size,
                c.intensity,
                c.test_interval,
                c.fade_in,
                c.fade_out,
                c.range
            ),
            (1., 1., 150, 15., 8., 300.)
        );
        // 0x9630 forward order observable through the cursor.
        let mut cur = LensflareReleaseCursor::default();
        assert_eq!(
            (
                cur.next_release(3),
                cur.next_release(3),
                cur.next_release(3),
                cur.next_release(3)
            ),
            (Some(0), Some(1), Some(2), None)
        );
        // 0xac90: id widths + residue (invalid id errors; caller still pushes).
        assert_eq!(lensflare_admit_glow(4, 3), Ok(Some(2)));
        assert_eq!(
            lensflare_admit_glow(4, 5),
            Err(LensflareGlowError::FileCorrupted(5))
        );
        assert_eq!(
            lensflare_admit_glow(4, 0),
            Err(LensflareGlowError::FileCorrupted(0))
        );
        assert_eq!(
            (LENSFLARE_FIXED_RADIUS, LENSFLARE_FIXED_EXTENT),
            (0.01, 0.05)
        );
        // 0x9890 fade step: dt==0 no-op, else clamped approach.
        assert_eq!(
            lensflare_fade_step(0.5, 1.0, [0.5; 3], [1.0; 3], 0, 15.0, 8.0),
            None
        );
        let s = lensflare_fade_step(0.0, 1.0, [0.0; 3], [1.0; 3], 100, 15.0, 8.0).unwrap();
        assert_eq!(s.visibility, 1.0);
        // Marginal-zero select: tiny positive target takes the fade_out arm.
        let t = lensflare_fade_step(1.0, 1e-9, [0.0; 3], [0.0; 3], 100, 15.0, 8.0).unwrap();
        let expect = 1.0f32 - 100.0f32 * 8.0f32 * 0.001f32;
        assert!((t.visibility - expect).abs() < 1e-6);
        // 0x96e0 core: duplicate_from keeps destination intensity.
        let mut d = LensflareParameters::constructed();
        d.duplicate_from(LensflareParameters {
            size: 2.,
            intensity: 0.25,
            test_interval: 9,
            fade_in: 2.,
            fade_out: 3.,
            range: 4.,
        });
        assert_eq!((d.size, d.intensity, d.test_interval), (2., 1., 9));
    }
    #[test]
    fn constructor_and_duplicate_keep_destination_intensity() {
        let mut destination = LensflareParameters::constructed();
        assert_eq!(
            destination,
            LensflareParameters {
                size: 1.,
                intensity: 1.,
                test_interval: 150,
                fade_in: 15.,
                fade_out: 8.,
                range: 300.
            }
        );
        let source = LensflareParameters {
            size: -0.,
            intensity: 0.25,
            test_interval: u32::MAX,
            fade_in: 2.,
            fade_out: 3.,
            range: 4.,
        };
        destination.duplicate_from(source);
        assert_eq!(destination.size.to_bits(), (-0.0f32).to_bits());
        assert_eq!(destination.intensity, 1.);
        assert_eq!(destination.test_interval, u32::MAX);
        assert_eq!(
            (destination.fade_in, destination.fade_out, destination.range),
            (2., 3., 4.)
        );
    }
    #[test]
    fn interval_truncates_signed64_then_keeps_low32_and_precision() {
        let mut parameters = LensflareParameters::constructed();
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            parameters
                .assign(LensflareParameter::TestInterval, -1.75)
                .unwrap()
        });
        assert_eq!(parameters.test_interval, u32::MAX);
        assert!(precision.precision_raised());
        parameters
            .assign(LensflareParameter::TestInterval, 4_294_967_296.)
            .unwrap();
        assert_eq!(parameters.test_interval, 0);
        parameters
            .assign(
                LensflareParameter::TestInterval,
                -9_223_372_036_854_775_808.,
            )
            .unwrap();
        assert_eq!(parameters.test_interval, 0);
        for value in [9_223_372_036_854_775_808., f32::NAN, f32::INFINITY] {
            let before = parameters;
            assert!(parameters
                .assign(LensflareParameter::TestInterval, value)
                .is_err());
            assert_eq!(parameters, before);
        }
    }
    #[test]
    fn fade_pair_loads_keep_the_reached_store_prefix() {
        let mut parameters = LensflareParameters::constructed();
        parameters.fade_out = f32::INFINITY;
        assert!(parameters.assign(LensflareParameter::FadeIn, 7.).is_err());
        assert_eq!(parameters.fade_in, 7.);
        assert_eq!(parameters.fade_out, f32::INFINITY);
        parameters.fade_in = f32::INFINITY;
        parameters.fade_out = 8.;
        assert!(parameters.assign(LensflareParameter::FadeOut, 7.).is_err());
        assert_eq!(parameters.fade_out, 8.);
    }
    #[test]
    fn plain_game_setters_store_without_validation() {
        // Working names for the game-only inlined lensflare setters
        // (mafia-re ls3df/I3D/I3D.h:3017-3044): no LS3DF FUNCTION addresses
        // exist. Plain stores, unlike the validated scene2 `assign` path.
        let mut params = LensflareParameters::constructed();
        lensflare_plain_set_size(&mut params, 2.5);
        assert_eq!(params.size.to_bits(), 2.5f32.to_bits());
        lensflare_plain_set_size(&mut params, f32::NAN);
        assert!(params.size.is_nan());
        lensflare_plain_set_intensity(&mut params, 0.25);
        assert_eq!(params.intensity.to_bits(), 0.25f32.to_bits());
        lensflare_plain_set_test_interval(&mut params, 9);
        assert_eq!(params.test_interval, 9);
        lensflare_plain_set_test_interval(&mut params, u32::MAX);
        assert_eq!(params.test_interval, u32::MAX);
        lensflare_plain_set_fade(&mut params, 3.0, 4.0);
        assert_eq!((params.fade_in.to_bits(), params.fade_out.to_bits()), (3.0f32.to_bits(), 4.0f32.to_bits()));
        lensflare_plain_set_fade(&mut params, f32::INFINITY, f32::NEG_INFINITY);
        assert_eq!((params.fade_in, params.fade_out), (f32::INFINITY, f32::NEG_INFINITY));
        lensflare_plain_set_range(&mut params, 42.0);
        assert_eq!(params.range.to_bits(), 42.0f32.to_bits());
        // Untouched fields keep their values through each plain store.
        let mut fresh = LensflareParameters::constructed();
        lensflare_plain_set_size(&mut fresh, 7.0);
        assert_eq!(
            (
                fresh.intensity,
                fresh.test_interval,
                fresh.fade_in,
                fresh.fade_out,
                fresh.range
            ),
            (1.0, 150, 15.0, 8.0, 300.0)
        );
    }
}
