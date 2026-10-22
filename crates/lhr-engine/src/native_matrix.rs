//! Bounded affine inverse from the original `S_matrix` entry point.
use crate::native_transform::SMatrix;

/// Portable arithmetic cannot reproduce x87 extended exponent intermediates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InverseUnavailable;

fn checked(value: f32, exact: f64) -> Result<f32, InverseUnavailable> {
    if !value.is_finite()
        || (exact != 0.0 && exact.abs() < f32::MIN_POSITIVE as f64)
        || exact.abs() > f32::MAX as f64
    {
        Err(InverseUnavailable)
    } else {
        Ok(value)
    }
}
fn mul(a: f32, b: f32) -> Result<f32, InverseUnavailable> {
    checked(a * b, a as f64 * b as f64)
}
fn add(a: f32, b: f32) -> Result<f32, InverseUnavailable> {
    checked(a + b, a as f64 + b as f64)
}
fn sub(a: f32, b: f32) -> Result<f32, InverseUnavailable> {
    checked(a - b, a as f64 - b as f64)
}
fn minor(a: f32, b: f32, c: f32, d: f32) -> Result<f32, InverseUnavailable> {
    sub(mul(a, b)?, mul(c, d)?)
}

impl SMatrix {
    /// Original `S_matrix::Inverse`, RVA0x2d0b0, under Game's PC24/RN policy.
    /// Returns the affine inverse and native success flag. A determinant with
    /// magnitude below the original widened f32 1e-8 cutoff returns the
    /// identity and false. The fourth input column is ignored by the original.
    /// Inputs and every arithmetic intermediate must remain finite normal f32
    /// values or zero. Unavailable calls do not yield a partially written matrix.
    /// Source and destination aliasing is deliberately absent from this API.
    pub fn inverse(&self) -> Result<(Self, bool), InverseUnavailable> {
        let a = self.elements;
        for i in [0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14] {
            if !a[i].is_finite() || (a[i] != 0.0 && !a[i].is_normal()) {
                return Err(InverseUnavailable);
            }
        }
        let c0 = minor(a[5], a[10], a[9], a[6])?;
        let c4 = minor(a[10], a[4], a[8], a[6])?;
        let c8 = minor(a[9], a[4], a[5], a[8])?;
        let det = add(sub(mul(a[0], c0)?, mul(c4, a[1])?)?, mul(c8, a[2])?)?;
        if (det as f64).abs() < 1e-8_f32 as f64 {
            return Ok((Self::identity(), false));
        }
        let r = checked(1.0 / det, 1.0 / det as f64)?;
        let mut o = [0.0; 16];
        o[0] = mul(r, c0)?;
        o[1] = -mul(minor(a[10], a[1], a[9], a[2])?, r)?;
        o[2] = mul(minor(a[6], a[1], a[2], a[5])?, r)?;
        o[4] = -mul(r, c4)?;
        o[5] = mul(minor(a[0], a[10], a[2], a[8])?, r)?;
        o[6] = -mul(minor(a[0], a[6], a[2], a[4])?, r)?;
        o[8] = mul(r, c8)?;
        o[9] = -mul(minor(a[9], a[0], a[8], a[1])?, r)?;
        o[10] = mul(minor(a[5], a[0], a[4], a[1])?, r)?;
        // Preserve the three distinct native accumulation orders.
        o[12] = -add(
            add(mul(o[0], a[12])?, mul(o[8], a[14])?)?,
            mul(o[4], a[13])?,
        )?;
        o[13] = -add(
            add(mul(o[5], a[13])?, mul(o[9], a[14])?)?,
            mul(a[12], o[1])?,
        )?;
        o[14] = -add(
            add(mul(o[6], a[13])?, mul(o[2], a[12])?)?,
            mul(a[14], o[10])?,
        )?;
        o[15] = 1.0;
        Ok((Self { elements: o }, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_inverse_preserves_native_layout_and_singular_fallback() {
        let mut m = SMatrix::identity();
        m.elements[0] = 2.0;
        m.elements[5] = 4.0;
        m.elements[10] = -8.0;
        m.elements[12..15].copy_from_slice(&[6.0, -8.0, 16.0]);
        let (inverse, success) = m.inverse().unwrap();
        assert!(success);
        assert_eq!(inverse.elements[0], 0.5);
        assert_eq!(inverse.elements[5], 0.25);
        assert_eq!(inverse.elements[10], -0.125);
        assert_eq!(&inverse.elements[12..15], &[-3.0, 2.0, 2.0]);
        m.elements[0] = 0.0;
        assert_eq!(m.inverse(), Ok((SMatrix::identity(), false)));
    }

    #[test]
    fn determinant_threshold_is_strict_and_uses_native_constant() {
        let mut m = SMatrix::identity();
        m.elements[0] = 1e-8_f32;
        assert!(m.inverse().unwrap().1);
        m.elements[0] = f32::from_bits(1e-8_f32.to_bits() - 1);
        assert_eq!(m.inverse(), Ok((SMatrix::identity(), false)));
    }

    #[test]
    fn ignored_column_and_unavailable_exponent_paths() {
        let mut m = SMatrix::identity();
        for i in [3, 7, 11, 15] {
            m.elements[i] = f32::NAN;
        }
        assert!(m.inverse().unwrap().1);
        m.elements[0] = f32::MAX;
        m.elements[5] = 2.0;
        assert_eq!(m.inverse(), Err(InverseUnavailable));
        m.elements[0] = f32::MIN_POSITIVE;
        m.elements[5] = 0.5;
        assert_eq!(m.inverse(), Err(InverseUnavailable));
    }
}
