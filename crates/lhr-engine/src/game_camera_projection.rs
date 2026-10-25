//! Arithmetic of descriptive Game helper RVA0x204890, PC SHA256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Its caller supplies the actual global mode and C_screen+46f4 prestate.
//! This module neither guesses GameInit state nor owns another camera domain.

#[derive(Clone, Copy, Debug)]
pub struct GameCameraProjection {
    pub global_mode: u32,
    pub screen_flag_46f4: bool,
}
impl GameCameraProjection {
    pub(crate) fn fov(self, input: f32) -> Result<f32, &'static str> {
        if self.screen_flag_46f4 || !matches!(self.global_mode, 1 | 2) {
            return Ok(input);
        }
        if !input.is_finite() {
            return Err("nonfinite Game604890 retained FOV arithmetic unavailable");
        }
        let factor = if self.global_mode == 1 {
            f32::from_bits(0x3fcccccd)
        } else {
            2.
        };
        let exact = f64::from(input) * f64::from(factor);
        // PC24 retains the wider x87 exponent for the comparison following
        // FSTS. The spilled binary32 is used only when the cap is not selected.
        let retained = pc24(exact);
        crate::native_precision::record_wide_pc24_mul(
            f64::from(input),
            f64::from(factor),
            retained,
        );
        let spill = retained as f32;
        crate::native_precision::record_precision(f64::from(spill) != retained);
        let cap = f32::from_bits(0x40450a6b);
        Ok(if retained > f64::from(cap) {
            cap
        } else {
            spill
        })
    }
    pub(crate) fn width_single(width: i32) -> f32 {
        let single = width as f32;
        crate::native_precision::record_precision(f64::from(single) != f64::from(width));
        single
    }
    pub(crate) fn aspect(self, width_single: f32, height: i32) -> f32 {
        // Width was already FST32; height is still an exact signed FILD32.
        // Width remains an integer in [-2^31,2^31], including rounded INT_MAX.
        let numerator = width_single as i64;
        let ratio = crate::native_precision::pc24_integer_ratio(numerator, height);
        if self.screen_flag_46f4 && ratio.is_finite() {
            match self.global_mode {
                1 => crate::native_precision::mul(ratio, 0.5),
                2 => crate::native_precision::mul(ratio, f32::from_bits(0x3eaaaaab)),
                _ => ratio,
            }
        } else {
            ratio
        }
    }
}
fn pc24(value: f64) -> f64 {
    if value == 0. || !value.is_finite() {
        return value;
    }
    let bits = value.to_bits();
    let remainder = bits & ((1 << 29) - 1);
    let mut retained = bits >> 29;
    if remainder > 1 << 28 || remainder == 1 << 28 && retained & 1 != 0 {
        retained += 1;
    }
    f64::from_bits(retained << 29)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_helper_twelve_native_projection_cases() {
        // Actual unchanged604890 plus actual camera setters, O0/O3 agreement.
        let expected = [
            (0, false, 800, 600, 0x3fa66666, 0x3faaaaab),
            (0, false, 16777217, 16777217, 0x3fa66666, 0x3f7fffff),
            (0, true, 800, 600, 0x3fa66666, 0x3faaaaab),
            (0, true, 16777217, 16777217, 0x3fa66666, 0x3f7fffff),
            (1, false, 800, 600, 0x40051eb8, 0x3faaaaab),
            (1, false, 16777217, 16777217, 0x40051eb8, 0x3f7fffff),
            (1, true, 800, 600, 0x3fa66666, 0x3f2aaaab),
            (1, true, 16777217, 16777217, 0x3fa66666, 0x3effffff),
            (2, false, 800, 600, 0x40266666, 0x3faaaaab),
            (2, false, 16777217, 16777217, 0x40266666, 0x3f7fffff),
            (2, true, 800, 600, 0x3fa66666, 0x3ee38e3a),
            (2, true, 16777217, 16777217, 0x3fa66666, 0x3eaaaaaa),
        ];
        for (global_mode, screen_flag_46f4, width, height, fov, aspect) in expected {
            let projection = GameCameraProjection {
                global_mode,
                screen_flag_46f4,
            };
            assert_eq!(
                projection
                    .fov(f32::from_bits(0x3fa66666))
                    .unwrap()
                    .to_bits(),
                fov
            );
            assert_eq!(
                projection
                    .aspect(GameCameraProjection::width_single(width), height)
                    .to_bits(),
                aspect
            );
        }
    }
    #[test]
    fn exact_integer_height_is_not_prematurely_rounded() {
        let projection = GameCameraProjection {
            global_mode: 0,
            screen_flag_46f4: false,
        };
        // The exact signed integer denominator lies just across a binary32
        // quotient tie from its prematurely converted single representation.
        let width = GameCameraProjection::width_single(16_777_217);
        assert_eq!(width.to_bits(), 16_777_216f32.to_bits());
        assert_eq!(projection.aspect(width, 16_777_217).to_bits(), 0x3f7fffff);
        assert_ne!(
            projection.aspect(width, 16_777_217),
            width / (16_777_217i32 as f32)
        );
    }
    #[test]
    fn mode_flag_and_cap_are_explicit_caller_state() {
        for (mode, expected) in [(0, 1.), (1, 1.6), (2, 2.), (9, 1.)] {
            assert_eq!(
                GameCameraProjection {
                    global_mode: mode,
                    screen_flag_46f4: false
                }
                .fov(1.)
                .unwrap(),
                expected
            );
        }
        let one = GameCameraProjection {
            global_mode: 1,
            screen_flag_46f4: false,
        };
        assert_eq!(one.fov(3.).unwrap().to_bits(), 0x40450a6b);
        let split = GameCameraProjection {
            global_mode: 1,
            screen_flag_46f4: true,
        };
        assert_eq!(split.fov(3.).unwrap(), 3.);
        assert_eq!(split.aspect(800., 600), (800f32 / 600.) * 0.5);
    }
}
