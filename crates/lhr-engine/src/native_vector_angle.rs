//! Exported LS3DF S_vector::AngleTo, PC RVA 0x2e7a0, under PC24/RN.
use super::{atan2_positive_interval, Arithmetic, Float, Interval, MakeUnavailable};

fn length<'a>(a: &'a Arithmetic, v: [Float<'a>; 3]) -> Float<'a> {
    let one = a.input(1.);
    let squared = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if squared.value < 1e-8_f32 {
        a.input(0.)
    } else if f64::from((squared - one).abs().value) < f64::from(1e-8_f32) {
        one
    } else {
        squared.sqrt()
    }
}

/// Retains the original length cutoffs, X/Z/Y dot accumulation and clamping.
/// Arithmetic must remain finite normal binary32 or zero. The final CRT acos
/// uses its original PC24 square root and an enclosure of retained FPATAN;
/// an ambiguous binary32 rounding returns an explicit availability error.
fn angle_interval(first: [f32; 3], second: [f32; 3]) -> Result<Interval, MakeUnavailable> {
    let a = Arithmetic::new();
    let first = first.map(|v| a.input(v));
    let second = second.map(|v| a.input(v));
    let one = a.input(1.);
    let first_length = length(&a, first);
    let product = length(&a, second) * first_length;
    a.finish()?;
    if product.value < 1e-8_f32 {
        return Ok(Interval::exact(0.));
    }
    let dot = ((first[0] * second[0] + first[2] * second[2]) + first[1] * second[1]) / product;
    a.finish()?;
    let cosine = a.input(dot.value.clamp(-1., 1.));
    let sine = ((one + cosine) * (one - cosine)).sqrt();
    a.finish()?;
    atan2_positive_interval(sine.value, cosine.value)
}

pub fn try_vector_angle_to(first: [f32; 3], second: [f32; 3]) -> Result<f32, MakeUnavailable> {
    angle_interval(first, second)?.rounded()
}

#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub(crate) struct PortableRetainedVectorAngle {
    angle: Interval,
}
#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub(crate) fn try_portable_retained_vector_angle(
    first: [f32; 3],
    second: [f32; 3],
) -> Result<PortableRetainedVectorAngle, MakeUnavailable> {
    Ok(PortableRetainedVectorAngle {
        angle: angle_interval(first, second)?,
    })
}
#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
impl PortableRetainedVectorAngle {
    pub(crate) fn compare(&self, operand: f32) -> Result<std::cmp::Ordering, MakeUnavailable> {
        if operand != 0. && !operand.is_normal() {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        let operand = f64::from(operand);
        if self.angle.hi < operand {
            Ok(std::cmp::Ordering::Less)
        } else if self.angle.lo > operand {
            Ok(std::cmp::Ordering::Greater)
        } else if self.angle.lo == operand && self.angle.hi == operand {
            Ok(std::cmp::Ordering::Equal)
        } else {
            Err(MakeUnavailable::RoundingUncertain)
        }
    }
    pub(crate) fn spill(&self) -> Result<f32, MakeUnavailable> {
        self.angle.rounded()
    }
    pub(crate) fn divide_numerator(self, numerator: f32) -> Result<f32, MakeUnavailable> {
        let a = Arithmetic::new();
        a.input(numerator);
        a.finish()?;
        if self.angle.lo <= 0. {
            return Err(MakeUnavailable::RoundingUncertain);
        }
        if numerator == 0. {
            return Ok(numerator);
        }
        Interval::exact(f64::from(numerator))
            .div(self.angle)
            .rounded()
    }
}

// The intervals retain the x87 result through FSIN/FCOS. Twenty-eight terms
// bound the Taylor remainder below 2^-80 throughout [-8,8]; 2^-58 also
// encloses reduced-argument x87 error. Ambiguous float rounding stays explicit.
#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
fn sine_cosine(x: Interval) -> Result<(Interval, Interval), MakeUnavailable> {
    if x.lo < -8. || x.hi > 8. {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    if x.lo == 0. && x.hi == 0. {
        return Ok((x, Interval::exact(1.)));
    }
    crate::native_precision::record_precision(true);
    let squared = x.mul(x);
    let mut sine = x;
    let mut sine_term = x;
    let mut cosine = Interval::exact(1.);
    let mut cosine_term = cosine;
    for n in 1..=28 {
        sine_term = sine_term
            .mul(squared)
            .div(Interval::exact(((2 * n) * (2 * n + 1)) as f64))
            .neg();
        sine = sine.add(sine_term);
        cosine_term = cosine_term
            .mul(squared)
            .div(Interval::exact(((2 * n - 1) * (2 * n)) as f64))
            .neg();
        cosine = cosine.add(cosine_term);
    }
    let error = f64::from_bits(965 << 52);
    Ok((sine.expand(error), cosine.expand(error)))
}

/// AngleTo immediately followed by FSIN and a binary32 spill in Move.
#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub fn try_vector_angle_sine(first: [f32; 3], second: [f32; 3]) -> Result<f32, MakeUnavailable> {
    sine_cosine(angle_interval(first, second)?)?.0.rounded()
}

#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub fn try_vector_angle_sine_scaled(
    first: [f32; 3],
    second: [f32; 3],
    factors: [f32; 3],
) -> Result<[f32; 3], MakeUnavailable> {
    let a = Arithmetic::new();
    for factor in factors {
        a.input(factor);
    }
    a.finish()?;
    let sine = sine_cosine(angle_interval(first, second)?)?.0;
    Ok([
        sine.product(factors[0])?,
        sine.product(factors[1])?,
        sine.product(factors[2])?,
    ])
}

#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub(crate) fn try_ph_thing_friction_products(
    first: [f32; 3],
    second: [f32; 3],
    magnitude: f32,
    friction: f32,
    timestep: f32,
    mass: f32,
    axis: [f32; 3],
) -> Result<[f32; 3], MakeUnavailable> {
    for value in [magnitude, friction, timestep, mass]
        .into_iter()
        .chain(axis)
    {
        if value != 0. && !value.is_normal() {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
    }
    let sine = sine_cosine(angle_interval(first, second)?)?.0;
    fn product(a: f64, b: f64) -> Result<f64, MakeUnavailable> {
        let value = a * b;
        let bits = value.to_bits();
        let discarded = bits & ((1_u64 << 29) - 1);
        let mut retained = bits >> 29;
        if discarded > 1_u64 << 28 || discarded == 1_u64 << 28 && retained & 1 != 0 {
            retained += 1;
        }
        let result = f64::from_bits(retained << 29);
        crate::native_precision::record_wide_pc24_mul(a, b, result);
        if result.is_finite() && (result == 0. || result.is_normal()) {
            Ok(result)
        } else {
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        }
    }
    let factor = product(f64::from(magnitude), f64::from(friction))?;
    let magnitude = if sine.lo == 0. && sine.hi == 0. {
        sine.lo * factor
    } else if factor == 0. {
        if sine.lo > 0. {
            factor
        } else if sine.hi < 0. {
            -factor
        } else {
            return Err(MakeUnavailable::RoundingUncertain);
        }
    } else {
        sine.mul(Interval::exact(factor)).rounded_pc24()?
    };
    let retained_limit = product(
        product(
            product(f64::from(timestep), f64::from(friction))?,
            f64::from(mass),
        )?,
        3.,
    )?;
    let limit = retained_limit as f32;
    crate::native_precision::record_precision(f64::from(limit) != retained_limit);
    if limit != 0. && !limit.is_normal() {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let clamped = if magnitude > f64::from(limit) {
        f64::from(limit)
    } else {
        magnitude
    };
    let mut result = [0.; 3];
    for (out, value) in result.iter_mut().zip(axis) {
        let retained = product(clamped, f64::from(value))?;
        *out = retained as f32;
        crate::native_precision::record_precision(f64::from(*out) != retained);
        if *out != 0. && !out.is_normal() {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
    }
    Ok(result)
}

/// Original Move friction ellipse arithmetic, retaining FPTAN until each
/// following PC24 arithmetic/spill point. Returns lateral, longitudinal, norm.
#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub fn try_wheel_friction_limit(
    first: [f32; 3],
    second: [f32; 3],
    max_long: f32,
    max_lat: f32,
) -> Result<[f32; 3], MakeUnavailable> {
    let angle = angle_interval(first, second)?;
    let argument = angle
        .sub(Interval::exact(f64::from(f32::from_bits(0x3fc90fdb))))
        .rounded()?;
    let (sine, cosine) = sine_cosine(Interval::exact(f64::from(argument)))?;
    if cosine.lo <= 0. && cosine.hi >= 0. {
        return Err(MakeUnavailable::RoundingUncertain);
    }
    let tangent = if cosine.hi < 0. {
        sine.neg().div(cosine.neg())
    } else {
        sine.div(cosine)
    };
    let a = Arithmetic::new();
    let longitudinal = a.input(max_long);
    let lateral = a.input(max_lat);
    let lat_squared = (lateral * lateral).value;
    a.finish()?;
    let first_product = tangent.product(lat_squared)?;
    let second_product = tangent.product(first_product)?;
    let denominator_squared = longitudinal * longitudinal + a.input(second_product);
    let lat = (((longitudinal / denominator_squared) * denominator_squared.sqrt()) * lateral).value;
    a.finish()?;
    let long = tangent.product(lat)?;
    let norm = (a.input(long) * a.input(long) + a.input(lat) * a.input(lat))
        .sqrt()
        .value;
    a.finish()?;
    Ok([lat, long, norm])
}

/// CRT acos at Game VA611350 followed by retained FSIN/FCOS products.
/// This is a Move expression helper, not a second reconstructed CRT entry.
#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub fn try_acos_sin_cos_products(
    argument: f32,
    sine_factor: f32,
    cosine_factor: f32,
) -> Result<[f32; 2], MakeUnavailable> {
    let a = Arithmetic::new();
    let x = a.input(argument);
    a.input(sine_factor);
    a.input(cosine_factor);
    a.finish()?;
    if x.value.abs() > 1. {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let one = a.input(1.);
    let root = ((one + x) * (one - x)).sqrt();
    a.finish()?;
    let angle = atan2_positive_interval(root.value, x.value)?;
    let (sine, cosine) = sine_cosine(angle)?;
    let product = |interval: Interval, factor| {
        if interval.lo == 0. && interval.hi == 0. {
            Ok((interval.lo as f32) * factor)
        } else {
            interval.product(factor)
        }
    };
    Ok([product(sine, sine_factor)?, product(cosine, cosine_factor)?])
}

/// Retained FCOS product over reduced wheel angles. Larger portable arguments
/// remain explicitly unavailable instead of using a platform libm result.
#[cfg(any(test, not(any(target_arch = "x86", target_arch = "x86_64"))))]
pub fn try_cosine_product(angle: f32, factor: f32) -> Result<f32, MakeUnavailable> {
    let a = Arithmetic::new();
    a.input(angle);
    a.input(factor);
    a.finish()?;
    sine_cosine(Interval::exact(f64::from(angle)))?
        .1
        .product(factor)
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;
    use crate::native_transform::vector_x87;
    #[test]
    fn physical_friction_portable_enclosures_match_retained_native_suffix() {
        let mut admitted = 0;
        for (first, second, magnitude, friction, timestep, mass, axis) in [
            (
                [1., -2., 0.],
                [0., 1., 0.],
                2.,
                0.5,
                0.125,
                2.,
                [-1., 0., 0.],
            ),
            ([0.; 3], [0., 1., 0.], 2., 0.5, 0.125, 2., [-1., 0., 0.]),
            ([1., 0., 0.], [0., 1., 0.], 1., 1., 1., 1., [-1., 0., 0.]),
            (
                [0.25, -1., 0.5],
                [0., 1., 0.],
                1.,
                0.5,
                1.,
                2.,
                [0.125, -0.5, 0.25],
            ),
            (
                [1., -2., 0.],
                [0., 1., 0.],
                2.,
                0.,
                0.125,
                2.,
                [-1., 0., 0.],
            ),
        ] {
            let mut portable_precision = crate::native_precision::Pc24PrecisionState::default();
            let portable = portable_precision.run(|| {
                try_ph_thing_friction_products(
                    first, second, magnitude, friction, timestep, mass, axis,
                )
            });
            if let Ok(portable) = portable {
                let mut native_precision = crate::native_precision::Pc24PrecisionState::default();
                let native = native_precision
                    .run(|| {
                        vector_x87::try_ph_thing_friction_products(
                            first, second, magnitude, friction, timestep, mass, axis,
                        )
                    })
                    .unwrap();
                assert_eq!(portable.map(f32::to_bits), native.map(f32::to_bits));
                assert_eq!(portable_precision, native_precision);
                admitted += 1;
            }
        }
        assert!(
            admitted >= 3,
            "only {admitted} portable physical suffix cases admitted"
        );
    }
    #[test]
    fn retained_comparison_rejects_overlapping_portable_bounds() {
        let operand = 0.78_f32;
        let value = f64::from(operand);
        let token = PortableRetainedVectorAngle {
            angle: Interval::bounds(value.next_down(), value.next_up()),
        };
        assert_eq!(
            token.compare(operand),
            Err(MakeUnavailable::RoundingUncertain)
        );
        let zero = PortableRetainedVectorAngle {
            angle: Interval::exact(0.),
        };
        assert_eq!(zero.compare(0.), Ok(std::cmp::Ordering::Equal));
    }
    #[test]
    fn admitted_portable_enclosures_match_the_x87_expressions() {
        let mut admitted = 0;
        for i in 1..40 {
            let x = i as f32 / 40.;
            let first = [0., 0., 1.];
            let second = [x, 0.25, 1.];
            let portable = try_portable_retained_vector_angle(first, second).unwrap();
            let native = vector_x87::try_retained_vector_angle(first, second).unwrap();
            assert_eq!(
                portable.spill().unwrap().to_bits(),
                native.spill().unwrap().to_bits()
            );
            for threshold in [
                f32::from_bits(0x3f47ae14),
                f32::from_bits(0x4016b852),
                f32::from_bits(0x402ccccd),
            ] {
                assert_eq!(
                    portable.compare(threshold).unwrap(),
                    native.compare(threshold).unwrap()
                );
            }
            if let Ok(value) = portable.divide_numerator(0.375) {
                assert_eq!(
                    value.to_bits(),
                    native.divide_numerator(0.375).unwrap().to_bits()
                );
                admitted += 1;
            }
            if let Ok(value) = try_vector_angle_sine(first, second) {
                assert_eq!(
                    value.to_bits(),
                    vector_x87::try_vector_angle_sine(first, second)
                        .unwrap()
                        .to_bits()
                );
                admitted += 1;
            }
            if let Ok(value) = try_vector_angle_sine_scaled(first, second, [0.25, 0.5, 0.75]) {
                assert_eq!(
                    value.map(f32::to_bits),
                    vector_x87::try_vector_angle_sine_scaled(first, second, [0.25, 0.5, 0.75])
                        .unwrap()
                        .map(f32::to_bits)
                );
                admitted += 1;
            }
            if let Ok(value) = try_wheel_friction_limit(first, second, 0.5, 0.75) {
                assert_eq!(
                    value.map(f32::to_bits),
                    vector_x87::try_wheel_friction_limit(first, second, 0.5, 0.75)
                        .unwrap()
                        .map(f32::to_bits)
                );
                admitted += 1;
            }
            if let Ok(value) = try_acos_sin_cos_products(x, 0.25, 0.75) {
                assert_eq!(
                    value.map(f32::to_bits),
                    vector_x87::try_acos_sin_cos_products(x, 0.25, 0.75)
                        .unwrap()
                        .map(f32::to_bits)
                );
                admitted += 1;
            }
            let angle = x * 8.;
            if let Ok(value) = try_cosine_product(angle, 0.75) {
                assert_eq!(
                    value.to_bits(),
                    vector_x87::try_cosine_product(angle, 0.75)
                        .unwrap()
                        .to_bits()
                );
                admitted += 1;
            }
        }
        assert!(
            admitted >= 120,
            "Portable enclosure unexpectedly rejected ordinary expressions: {admitted}"
        );
    }
}
