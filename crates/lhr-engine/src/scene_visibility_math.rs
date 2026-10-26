//! Exact PC24 rounding for the source visibility routines' binary64 memory
//! constants. Rounding a decimal constant to f32 before an x87 operation changes
//! boundary decisions. Integer significands avoid that and binary64 double rounding.
use super::{number, Result, SceneVisibilityError};

fn dyadic32(v: f32) -> Result<(bool, u128, i32)> {
    number(v)?;
    let b = v.to_bits();
    if v == 0. {
        return Ok((b >> 31 != 0, 0, 0));
    }
    Ok((
        b >> 31 != 0,
        u128::from((b & 0x7fffff) | 0x800000),
        ((b >> 23) & 255) as i32 - 150,
    ))
}
fn dyadic64(v: f64) -> Result<(bool, u128, i32)> {
    let b = v.to_bits();
    let e = ((b >> 52) & 2047) as i32;
    if e == 0 || e == 2047 {
        return Err(SceneVisibilityError::Unsupported(
            "visibility constant exponent",
        ));
    }
    Ok((
        b >> 63 != 0,
        u128::from((b & 0xfffffffffffff) | 0x10000000000000),
        e - 1075,
    ))
}
fn rounded(sign: bool, mut n: u128, mut exponent: i32) -> Result<f32> {
    if n == 0 {
        return Ok(f32::from_bits(u32::from(sign) << 31));
    }
    let bits = 128 - n.leading_zeros();
    if bits > 24 {
        let shift = bits - 24;
        let remainder = n & ((1u128 << shift) - 1);
        n >>= shift;
        exponent += shift as i32;
        crate::native_precision::record_precision(remainder != 0);
        let half = 1u128 << (shift - 1);
        if remainder > half || (remainder == half && n & 1 != 0) {
            n += 1;
        }
    } else {
        let shift = 24 - bits;
        n <<= shift;
        exponent -= shift as i32;
    }
    if n == 1 << 24 {
        n >>= 1;
        exponent += 1;
    }
    let unbiased = exponent + 23;
    if !(-126..=127).contains(&unbiased) {
        return Err(SceneVisibilityError::Unsupported(
            "visibility extended exponent",
        ));
    }
    number(f32::from_bits(
        (u32::from(sign) << 31) | (((unbiased + 127) as u32) << 23) | (n as u32 & 0x7fffff),
    ))
}
pub(crate) fn mul_constant(a: f32, b: f64) -> Result<f32> {
    let (sa, na, ea) = dyadic32(a)?;
    let (sb, nb, eb) = dyadic64(b)?;
    rounded(sa ^ sb, na * nb, ea + eb)
}
pub(crate) fn add_constant(a: f32, b: f64) -> Result<f32> {
    let (sa, na, ea) = dyadic32(a)?;
    let (sb, nb, eb) = dyadic64(b)?;
    if na == 0 {
        return rounded(sb, nb, eb);
    }
    let exponent = ea.min(eb);
    let ashift = (ea - exponent) as u32;
    let bshift = (eb - exponent) as u32;
    if ashift + 128 - na.leading_zeros() > 126 || bshift + 128 - nb.leading_zeros() > 126 {
        return Err(SceneVisibilityError::Unsupported(
            "visibility mixed addition exponent span",
        ));
    }
    let a = na << ashift;
    let b = nb << bshift;
    if sa == sb {
        rounded(sa, a + b, exponent)
    } else if a >= b {
        rounded(if a == b { false } else { sa }, a - b, exponent)
    } else {
        rounded(sb, b - a, exponent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_constants_round_after_operation_and_preserve_integer_ties() {
        assert_eq!(mul_constant(5., 0.1).unwrap(), 0.5);
        assert_eq!(add_constant(-0.1f32, 0.1).unwrap().to_bits(), 0xb0cccccd);
        assert_ne!(add_constant(-0.1f32, 0.1).unwrap(), -0.1f32 + 0.1f32);
        assert!(add_constant(f32::MAX, 0.1).is_err());
    }
}
