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
