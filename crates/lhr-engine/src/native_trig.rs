//! Portable enclosures for the retained x87 transcendental values.
//! Every elementary f64 operation expands outward by one representable value.
//! Taylor tails and the Intel Pentium-or-later reduced-argument x87 error bound
//! are included before any single-precision rounding decision is admitted.
use super::MakeUnavailable;

#[derive(Clone, Copy, Debug)]
pub(super) struct Interval {
    lo: f64,
    hi: f64,
}
impl Interval {
    fn exact(x: f64) -> Self {
        Self { lo: x, hi: x }
    }
    fn bounds(lo: f64, hi: f64) -> Self {
        Self { lo, hi }
    }
    fn add(self, b: Self) -> Self {
        Self::bounds((self.lo + b.lo).next_down(), (self.hi + b.hi).next_up())
    }
    fn neg(self) -> Self {
        Self::bounds(-self.hi, -self.lo)
    }
    fn sub(self, b: Self) -> Self {
        self.add(b.neg())
    }
    fn mul(self, b: Self) -> Self {
        let p = [
            self.lo * b.lo,
            self.lo * b.hi,
            self.hi * b.lo,
            self.hi * b.hi,
        ];
        Self::bounds(
            p.into_iter().fold(f64::INFINITY, f64::min).next_down(),
            p.into_iter().fold(f64::NEG_INFINITY, f64::max).next_up(),
        )
    }
    fn div(self, b: Self) -> Self {
        debug_assert!(b.lo > 0.);
        self.mul(Self::bounds((1. / b.hi).next_down(), (1. / b.lo).next_up()))
    }
    fn expand(self, error: f64) -> Self {
        self.add(Self::bounds(-error, error))
    }
    pub(super) fn rounded(self) -> Result<f32, MakeUnavailable> {
        let a = self.lo as f32;
        let b = self.hi as f32;
        if a.to_bits() == b.to_bits() && a.is_finite() && (a == 0. || a.is_normal()) {
            Ok(a)
        } else {
            Err(MakeUnavailable::RoundingUncertain)
        }
    }
    pub(super) fn product(self, value: f32) -> Result<f32, MakeUnavailable> {
        if value == 0. {
            // Nonzero sine has the same sign as its argument on this domain.
            if self.lo > 0. {
                return Ok(value);
            }
            if self.hi < 0. {
                return Ok(-value);
            }
        }
        self.mul(Self::exact(value as f64)).rounded()
    }
    pub(super) fn squared(self) -> Result<f32, MakeUnavailable> {
        self.mul(self).rounded()
    }
}

/// FSIN/FCOS ignore PC24 and retain extended results. |x|<=1 is inside
/// Intel's reduced-argument domains for both instructions. The absolute x87
/// allowance 2^-60 exceeds one extended ulp throughout [-1,1].
pub(super) fn sin_cos(x: f32) -> Result<(Interval, Interval), MakeUnavailable> {
    if !x.is_finite() || x.abs() > 1. || x == 0. {
        return Err(MakeUnavailable::UnrecoveredAxisAngle);
    }
    let x = Interval::exact(x as f64);
    let square = x.mul(x);
    let mut sin = x;
    let mut sin_term = x;
    let mut cos = Interval::exact(1.);
    let mut cos_term = cos;
    for n in 1..=20 {
        sin_term = sin_term
            .mul(square)
            .div(Interval::exact(((2 * n) * (2 * n + 1)) as f64))
            .neg();
        cos_term = cos_term
            .mul(square)
            .div(Interval::exact(((2 * n - 1) * (2 * n)) as f64))
            .neg();
        sin = sin.add(sin_term);
        cos = cos.add(cos_term);
    }
    // First omitted terms <=1/42! and1/43!, both <2^-120.
    let error = (f64::from_bits(903 << 52) + f64::from_bits(963 << 52)).next_up();
    Ok((sin.expand(error), cos.expand(error)))
}

fn pi() -> Interval {
    Interval::bounds(
        std::f64::consts::PI.next_down(),
        std::f64::consts::PI.next_up(),
    )
}

/// FPATAN on positive Y and arbitrary finite X; callers supply the original
/// PC24 square-root result. This encloses the extended atan2, not acos(x).
pub(super) fn atan2_positive(y: f32, x: f32) -> Result<f32, MakeUnavailable> {
    atan2_positive_interval(y, x)?.rounded()
}

fn atan2_positive_interval(y: f32, x: f32) -> Result<Interval, MakeUnavailable> {
    if !x.is_finite() || !y.is_finite() || y.is_sign_negative() {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    if y == 0. {
        return if x < 0. {
            Ok(pi())
        } else if x > 0. {
            Ok(Interval::exact(0.))
        } else {
            Err(MakeUnavailable::RoundingUncertain)
        };
    }
    let a = x.abs() as f64;
    let b = y as f64;
    if a == 0. {
        return Ok(pi().div(Interval::exact(2.)));
    }
    let swapped = b > a;
    let r = Interval::exact(a.min(b)).div(Interval::exact(a.max(b)));
    let shifted = r.lo > 0.5;
    let t = if shifted {
        r.sub(Interval::exact(1.)).div(r.add(Interval::exact(1.)))
    } else {
        r
    };
    let square = t.mul(t);
    let mut power = t;
    let mut sum = t;
    for n in 1..=64 {
        power = power.mul(square).neg();
        sum = sum.add(power.div(Interval::exact((2 * n + 1) as f64)));
    }
    // |t|<=1/2 (or1/3 after shifting), so tail <2^-129/131<2^-128.
    sum = sum.expand(f64::from_bits(895 << 52));
    if shifted {
        sum = sum.add(pi().div(Interval::exact(4.)));
    }
    if swapped {
        sum = pi().div(Interval::exact(2.)).sub(sum);
    }
    if x < 0. {
        sum = pi().sub(sum);
    }
    // FPATAN <1 extended ulp; every result is <=pi, ulp<=2^-62.
    Ok(sum.expand(f64::from_bits(963 << 52)))
}

/// Enclose FSIN within the Intel reduced-argument domain. The Slerp caller
/// restricts all arguments to [0,1.5], strictly below pi/2. The first omitted
/// Taylor term is below 2^-120 even at 1.5; 2^-60 covers x87 extended error.
fn slerp_sine(x: Interval) -> Interval {
    let square = x.mul(x);
    let mut sum = x;
    let mut term = x;
    for n in 1..=20 {
        term = term
            .mul(square)
            .div(Interval::exact(((2 * n) * (2 * n + 1)) as f64))
            .neg();
        sum = sum.add(term);
    }
    sum.expand((f64::from_bits(903 << 52) + f64::from_bits(963 << 52)).next_up())
}

/// Original RVA2fee0 spherical weights, including the retained extended angle
/// used by the denominator FSIN. A stored f32 angle feeds both numerator FSINs.
/// Every arithmetic instruction following a transcendental rounds to PC24.
/// A coefficient is admitted only if its entire enclosure rounds identically.
pub(crate) fn slerp_weights(dot: f32, fraction: f32) -> Result<(f32, f32), MakeUnavailable> {
    if !dot.is_finite() || !(0. ..1.).contains(&dot) || !(0. ..=1.).contains(&fraction) {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let arithmetic = Arithmetic::new();
    let one = arithmetic.input(1.);
    let cosine = arithmetic.input(dot);
    let y = ((one + cosine) * (one - cosine)).sqrt();
    arithmetic.finish()?;
    let angle = atan2_positive_interval(y.value, dot)?;
    let stored = angle.rounded()?;
    if angle.lo <= 0. || angle.hi > 1.5 || stored > 1.5 {
        return Err(MakeUnavailable::UnrecoveredAxisAngle);
    }
    let reciprocal = Interval::exact(1.).div(slerp_sine(angle)).rounded()?;
    let fraction_value = arithmetic.input(fraction);
    let left_argument = ((one - fraction_value) * arithmetic.input(stored)).value;
    let right_argument = (arithmetic.input(stored) * fraction_value).value;
    arithmetic.finish()?;
    let weight = |argument: f32| {
        if argument == 0. {
            // FSIN preserves signed zero; the reciprocal is positive.
            Ok(argument)
        } else {
            slerp_sine(Interval::exact(argument as f64)).product(reciprocal)
        }
    };
    Ok((weight(left_argument)?, weight(right_argument)?))
}

/// PC24 arithmetic with a sticky exponent-domain guard. x87 retains a wider
/// exponent than f32; rejecting each nonordinary exact result avoids silently
/// substituting SSE underflow/overflow for those extended-exponent paths.
pub(super) struct Arithmetic {
    valid: std::cell::Cell<bool>,
}
#[derive(Clone, Copy)]
pub(super) struct Float<'a> {
    pub(super) value: f32,
    arithmetic: &'a Arithmetic,
}
impl Arithmetic {
    pub(super) fn new() -> Self {
        Self {
            valid: std::cell::Cell::new(true),
        }
    }
    pub(super) fn input(&self, value: f32) -> Float<'_> {
        self.result(value as f64)
    }
    fn result(&self, value: f64) -> Float<'_> {
        let v = value as f32;
        if !v.is_finite() || (v != 0. && !v.is_normal()) || (v == 0. && value != 0.) {
            self.valid.set(false);
        }
        Float {
            value: v,
            arithmetic: self,
        }
    }
    pub(super) fn finish(&self) -> Result<(), MakeUnavailable> {
        if self.valid.get() {
            Ok(())
        } else {
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        }
    }
}
impl Float<'_> {
    pub(super) fn abs(self) -> Self {
        Self {
            value: self.value.abs(),
            ..self
        }
    }
    pub(super) fn sqrt(self) -> Self {
        self.arithmetic.result((self.value as f64).sqrt())
    }
}
macro_rules! operation {
 ($trait:ident,$name:ident,$operator:tt)=>{
  impl<'a> std::ops::$trait for Float<'a> {
   type Output=Self;
   fn $name(self,right:Self)->Self {self.arithmetic.result((self.value as f64) $operator (right.value as f64))}
  }
 }
}
operation!(Add,add,+);
operation!(Sub,sub,-);
operation!(Mul,mul,*);
operation!(Div,div,/);
impl std::ops::Neg for Float<'_> {
    type Output = Self;
    fn neg(self) -> Self {
        Self {
            value: -self.value,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounding_enclosures_reject_midpoints_signed_zeros_and_subnormals() {
        let a = 1.0f32;
        let b = a.next_up();
        let midpoint = (a as f64 + b as f64) * 0.5;
        assert!(Interval::bounds(midpoint.next_down(), midpoint.next_up())
            .rounded()
            .is_err());
        assert_eq!(Interval::bounds(1., 1f64.next_up()).rounded().unwrap(), 1.);
        assert!(Interval::bounds(-0., 0.).rounded().is_err());
        assert!(Interval::exact(f32::from_bits(1) as f64).rounded().is_err());
        assert!(atan2_positive(-0., 1.).is_err());
    }
    #[test]
    fn pc24_guard_is_sticky_across_extended_exponent_edges() {
        let arithmetic = Arithmetic::new();
        let a = arithmetic.input(f32::MIN_POSITIVE);
        let small = a * arithmetic.input(0.5);
        let _restored = small * arithmetic.input(2.);
        assert!(arithmetic.finish().is_err());
        let arithmetic = Arithmetic::new();
        let a = arithmetic.input(f32::MAX);
        let _overflow = a * arithmetic.input(2.);
        assert!(arithmetic.finish().is_err());
    }
    #[test]
    fn crt_pc24_arithmetic_differs_from_direct_acos() {
        let x = f32::from_bits(0x3f7670eb);
        let y = ((1. + x) * (1. - x)).sqrt();
        let actual = atan2_positive(y, x).unwrap();
        // This is the original PC24 sqrt/FPATAN path, not f64::acos(x).
        assert_eq!(actual.to_bits(), 1049385742);
        assert_eq!(((x as f64).acos() as f32).to_bits(), 1049385741);
        assert!((actual - (x as f64).acos() as f32).abs() < 1e-6);
    }
}
