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

#[cfg(test)]
mod tests {
    use super::*;
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
}
