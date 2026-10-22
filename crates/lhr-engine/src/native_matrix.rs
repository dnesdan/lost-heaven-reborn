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

/// Availability guard for original scale extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScaleUnavailable;

/// Unsupported numeric or degenerate-direction path in the bounded SetDir3 port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectionUnavailable {
    NumericDomain,
    DegenerateUp,
}

fn squared(v: [f32; 3]) -> Result<f32, InverseUnavailable> {
    add(add(mul(v[2], v[2])?, mul(v[1], v[1])?)?, mul(v[0], v[0])?)
}
fn normalized(v: [f32; 3]) -> Result<[f32; 3], InverseUnavailable> {
    let length2 = squared(v)?;
    if (sub(length2, 1.0)? as f64).abs() < 1e-8_f32 as f64 {
        return Ok(v);
    }
    if length2 as f64 >= 1e-8_f32 as f64 {
        let length = checked(length2.sqrt(), (length2 as f64).sqrt())?;
        let reciprocal = checked(1.0 / length, 1.0 / length as f64)?;
        return Ok([
            mul(reciprocal, v[0])?,
            mul(reciprocal, v[1])?,
            mul(reciprocal, v[2])?,
        ]);
    }
    let mut out = v;
    let axis = if v[0] != 0.0 {
        0
    } else if v[2] != 0.0 {
        2
    } else {
        1
    };
    out[axis] = if v[axis] < 0.0 { -1.0 } else { 1.0 };
    Ok(out)
}

impl SMatrix {
    /// Original `S_matrix::SetDir3`, RVA0x2dcf0, for nondegenerate input
    /// directions under PC24/RN. Copies and normalizes Z before constructing Y
    /// and X. The small-residual fallback is explicitly unavailable. A
    /// failure may preserve basis writes preceding the unsupported dependency.
    /// Translation and the fourth column remain untouched.
    pub fn try_set_dir3(
        &mut self,
        direction: [f32; 3],
        up: [f32; 3],
    ) -> Result<(), DirectionUnavailable> {
        if direction
            .into_iter()
            .chain(up)
            .any(|x| !x.is_finite() || (x != 0.0 && !x.is_normal()))
        {
            return Err(DirectionUnavailable::NumericDomain);
        }
        self.elements[8..11].copy_from_slice(&direction);
        let z = normalized(direction).map_err(|_| DirectionUnavailable::NumericDomain)?;
        self.elements[8..11].copy_from_slice(&z);
        let residual = (|| {
            let dot = add(
                add(mul(z[2], up[2])?, mul(z[1], up[1])?)?,
                mul(up[0], z[0])?,
            )?;
            Ok::<_, InverseUnavailable>([
                sub(up[0], mul(dot, z[0])?)?,
                sub(up[1], mul(dot, z[1])?)?,
                sub(up[2], mul(dot, z[2])?)?,
            ])
        })()
        .map_err(|_| DirectionUnavailable::NumericDomain)?;
        if (squared(residual).map_err(|_| DirectionUnavailable::NumericDomain)? as f64)
            < 1e-8_f32 as f64
        {
            return Err(DirectionUnavailable::DegenerateUp);
        }
        self.elements[4..7].copy_from_slice(&residual);
        let y = normalized(residual).map_err(|_| DirectionUnavailable::NumericDomain)?;
        self.elements[4..7].copy_from_slice(&y);
        // The original cross product uses the unnormalized residual and the
        // original direction; it then normalizes the resulting X row.
        let x = (|| {
            Ok::<_, InverseUnavailable>([
                minor(residual[1], direction[2], residual[2], direction[1])?,
                minor(residual[2], direction[0], direction[2], residual[0])?,
                minor(residual[0], direction[1], residual[1], direction[0])?,
            ])
        })()
        .map_err(|_| DirectionUnavailable::NumericDomain)?;
        self.elements[..3].copy_from_slice(&x);
        let x = normalized(x).map_err(|_| DirectionUnavailable::NumericDomain)?;
        self.elements[..3].copy_from_slice(&x);
        Ok(())
    }

    /// Original `S_matrix::GetScale`, RVA0x2e3a0. Extracts nonnegative
    /// basis-row lengths with native zero and unit cutoffs under PC24/RN.
    /// The unused fourth column and translation are ignored. Every arithmetic
    /// intermediate must fit a finite normal f32 value or zero.
    pub fn try_get_scale(&self) -> Result<[f32; 3], ScaleUnavailable> {
        let mut out = [0.0; 3];
        // Native computes Z, then Y, then X, storing the result as XYZ.
        for row in [2, 1, 0] {
            let v = &self.elements[row * 4..row * 4 + 3];
            if v.iter()
                .any(|x| !x.is_finite() || (*x != 0.0 && !x.is_normal()))
            {
                return Err(ScaleUnavailable);
            }
            let length = (|| {
                let squared = add(add(mul(v[0], v[0])?, mul(v[1], v[1])?)?, mul(v[2], v[2])?)?;
                if (squared.to_bits() as i32) < 0x322bcc77 {
                    Ok(0.0)
                } else if (sub(squared, 1.0)? as f64).abs() < 1e-8_f32 as f64 {
                    Ok(1.0)
                } else {
                    checked(squared.sqrt(), (squared as f64).sqrt())
                }
            })()
            .map_err(|_| ScaleUnavailable)?;
            out[row] = length;
        }
        Ok(out)
    }

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
    fn scale_uses_row_lengths_and_ignores_other_fields() {
        let mut matrix = SMatrix {
            elements: [f32::NAN; 16],
        };
        for (row, values) in [[3., 4., 0.], [0., -2., 0.], [0., 0., 1e-6]]
            .into_iter()
            .enumerate()
        {
            matrix.elements[row * 4..row * 4 + 3].copy_from_slice(&values);
        }
        assert_eq!(matrix.try_get_scale(), Ok([5., 2., 0.]));
        matrix.elements[0] = f32::MAX;
        assert_eq!(matrix.try_get_scale(), Err(ScaleUnavailable));
    }
    #[test]
    fn direction_builds_basis_and_preserves_translation_and_partial_writes() {
        let mut matrix = SMatrix::identity();
        matrix.elements[12] = 7.;
        matrix.elements[3] = 19.;
        matrix.try_set_dir3([0., 0., 2.], [0., 4., 0.]).unwrap();
        assert_eq!(matrix.elements[0], 1.);
        assert_eq!(matrix.elements[5], 1.);
        assert_eq!(matrix.elements[10], 1.);
        assert_eq!(matrix.elements[12], 7.);
        assert_eq!(matrix.elements[3], 19.);
        assert_eq!(
            matrix.try_set_dir3([0., 2., 0.], [0., 3., 0.]),
            Err(DirectionUnavailable::DegenerateUp)
        );
        assert_eq!(&matrix.elements[8..11], &[0., 1., 0.]);
    }

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
