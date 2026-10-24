//! Bounded original `S_matrix` arithmetic under Game's PC24/RN policy.
use crate::native_transform::{SMatrix, SQuat};

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
    let result = checked(a * b, a as f64 * b as f64)?;
    crate::native_precision::record_mul(a, b, result);
    Ok(result)
}
fn add(a: f32, b: f32) -> Result<f32, InverseUnavailable> {
    let result = checked(a + b, a as f64 + b as f64)?;
    crate::native_precision::record_add(a, b, result);
    Ok(result)
}
fn sub(a: f32, b: f32) -> Result<f32, InverseUnavailable> {
    let result = checked(a - b, a as f64 - b as f64)?;
    crate::native_precision::record_sub(a, b, result);
    Ok(result)
}
fn minor(a: f32, b: f32, c: f32, d: f32) -> Result<f32, InverseUnavailable> {
    sub(mul(a, b)?, mul(c, d)?)
}

/// A needed operand or arithmetic intermediate is outside the finite normal
/// f32-or-zero domain. Ignored fields are not constrained by this guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixArithmeticUnavailable;

fn normal_operand(value: f32) -> Result<f32, MatrixArithmeticUnavailable> {
    if value == 0.0 || value.is_normal() {
        Ok(value)
    } else {
        Err(MatrixArithmeticUnavailable)
    }
}
// Arithmetic values are computed by separate f32 operations, with no FMA.
// The widened computation guards x87's wider exponent, not extra precision.
// f32 products fit f64 exactly; near-zero cancellation of f32 sums also fits.
// Inputs and every accepted intermediate are normal-or-zero, so PC24 rounding
// agrees with IEEE f32 without using unverified subnormal/overflow arithmetic.
pub(crate) fn checked_f32_mul(a: f32, b: f32) -> Result<f32, MatrixArithmeticUnavailable> {
    let (a, b) = (normal_operand(a)?, normal_operand(b)?);
    mul(a, b).map_err(|_| MatrixArithmeticUnavailable)
}
pub(crate) fn checked_f32_add(a: f32, b: f32) -> Result<f32, MatrixArithmeticUnavailable> {
    let (a, b) = (normal_operand(a)?, normal_operand(b)?);
    add(a, b).map_err(|_| MatrixArithmeticUnavailable)
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
        crate::native_precision::record_sqrt(length2, length);
        let reciprocal = checked(1.0 / length, 1.0 / length as f64)?;
        crate::native_precision::record_div(1.0, length, reciprocal);
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
    /// Checked original SetRot3, RVA0x2e070, using only needed operands.
    /// abs(W)>=1 writes an identity basis without reading XYZ. Existing matrix
    /// fields are never read by the native routine; seven non-basis fields are
    /// preserved even when outside the arithmetic domain.
    /// This wrapper preflights all arithmetic and leaves the destination intact
    /// on failure. That is a host availability policy, not a claim about native
    /// partial basis stores after arithmetic outside the supported domain.
    pub fn try_set_rot3(&mut self, quaternion: SQuat) -> Result<(), MatrixArithmeticUnavailable> {
        let [w, x, y, z] = quaternion.wxyz;
        normal_operand(w)?;
        let basis = if w.abs() >= 1.0 {
            [1., 0., 0., 0., 1., 0., 0., 0., 1.]
        } else {
            if ![x, y, z].into_iter().all(f32::is_finite) {
                return Err(MatrixArithmeticUnavailable);
            }
            // PC24 uses a 24-bit significand with the x87 wider exponent.
            // z2 and seven products are explicitly spilled to f32 stack slots;
            // tiny spilled products may become subnormal/zero, as on AXIS data.
            fn pc24(value: f64) -> f64 {
                let bits = value.to_bits();
                let magnitude = bits & 0x7fff_ffff_ffff_ffff;
                let discarded = magnitude & ((1_u64 << 29) - 1);
                let mut retained = magnitude >> 29;
                let halfway = 1_u64 << 28;
                if discarded > halfway || (discarded == halfway && retained & 1 != 0) {
                    retained += 1;
                }
                f64::from_bits((bits & (1_u64 << 63)) | (retained << 29))
            }
            fn add(a: f64, b: f64) -> f64 {
                let result = pc24(a + b);
                crate::native_precision::record_wide_pc24_add(a, b, result);
                result
            }
            fn sub(a: f64, b: f64) -> f64 {
                let result = pc24(a - b);
                crate::native_precision::record_wide_pc24_sub(a, b, result);
                result
            }
            fn mul(a: f64, b: f64) -> f64 {
                let result = pc24(a * b);
                crate::native_precision::record_wide_pc24_mul(a, b, result);
                result
            }
            fn spill(value: f64) -> Result<f64, MatrixArithmeticUnavailable> {
                let narrowed = value as f32;
                crate::native_precision::record_precision(f64::from(narrowed) != value);
                if narrowed.is_finite() {
                    Ok(narrowed as f64)
                } else {
                    Err(MatrixArithmeticUnavailable)
                }
            }
            let [w, x, y, z] = [w, x, y, z].map(f64::from);
            let x2 = add(x, x);
            let y2 = add(y, y);
            let z2 = spill(add(z, z))?;
            let xx = spill(mul(x2, x))?;
            let xy = spill(mul(y2, x))?;
            let xz = spill(mul(z2, x))?;
            let yy = spill(mul(y2, y))?;
            let yz = spill(mul(z2, y))?;
            let zz = spill(mul(z2, z))?;
            let wx = spill(mul(x2, w))?;
            let wy = mul(y2, w);
            let wz = mul(z2, w);
            let values = [
                sub(1.0, add(zz, yy)),
                sub(xy, wz),
                add(wy, xz),
                add(wz, xy),
                sub(1.0, add(zz, xx)),
                sub(yz, wx),
                sub(xz, wy),
                add(wx, yz),
                sub(1.0, add(yy, xx)),
            ];
            let mut basis = [0.; 9];
            for (target, value) in basis.iter_mut().zip(values) {
                *target = spill(value)? as f32;
            }
            basis
        };
        for (index, value) in [0, 1, 2, 4, 5, 6, 8, 9, 10].into_iter().zip(basis) {
            self.elements[index] = value;
        }
        Ok(())
    }
    /// Checked original affine product, RVA0x2d3a0, with distinct output storage.
    /// Both homogeneous columns are ignored; output is [0,0,0,1]. Needed
    /// operands and intermediates must remain finite normal f32 values or zero.
    pub fn try_mul_affine(&self, right: &Self) -> Result<Self, MatrixArithmeticUnavailable> {
        let a = &self.elements;
        let b = &right.elements;
        let mut result = Self::identity();
        for row in 0..4 {
            for column in 0..3 {
                let first = checked_f32_mul(a[row * 4], b[column])?;
                let second = checked_f32_mul(a[row * 4 + 1], b[4 + column])?;
                let third = checked_f32_mul(a[row * 4 + 2], b[8 + column])?;
                let initial = if row == 3 {
                    checked_f32_add(first, b[12 + column])?
                } else {
                    first
                };
                result.elements[row * 4 + column] =
                    checked_f32_add(checked_f32_add(initial, second)?, third)?;
            }
        }
        Ok(result)
    }
    /// Checked original point transform, RVA0x2e980. Adds translation after
    /// the first product, then Y and Z products; ignores the homogeneous column.
    pub fn try_transform_point(
        &self,
        vector: [f32; 3],
    ) -> Result<[f32; 3], MatrixArithmeticUnavailable> {
        let mut out = [0.; 3];
        for (column, value) in out.iter_mut().enumerate() {
            let first = checked_f32_mul(vector[0], self.elements[column])?;
            let translated = checked_f32_add(first, self.elements[12 + column])?;
            let second = checked_f32_mul(vector[1], self.elements[4 + column])?;
            let third = checked_f32_mul(vector[2], self.elements[8 + column])?;
            *value = checked_f32_add(checked_f32_add(translated, second)?, third)?;
        }
        Ok(out)
    }

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
            let length = (|| -> Result<f32, InverseUnavailable> {
                let squared = add(add(mul(v[0], v[0])?, mul(v[1], v[1])?)?, mul(v[2], v[2])?)?;
                if (squared.to_bits() as i32) < 0x322bcc77 {
                    Ok(0.0)
                } else if (sub(squared, 1.0)? as f64).abs() < 1e-8_f32 as f64 {
                    Ok(1.0)
                } else {
                    let length = checked(squared.sqrt(), (squared as f64).sqrt())?;
                    crate::native_precision::record_sqrt(squared, length);
                    Ok(length)
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
        crate::native_precision::record_div(1.0, det, r);
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
    fn checked_rotation_records_wide_pc24_rounding_and_explicit_spills() {
        for (x, expected_precision) in [
            (0.5_f32, false),
            (0.2_f32, true),
            (2_f32.powi(-30), true),
            (f32::from_bits(1), true),
        ] {
            let mut matrix = SMatrix::identity();
            let mut state = crate::native_precision::Pc24PrecisionState::default();
            state
                .run(|| {
                    matrix.try_set_rot3(SQuat {
                        wxyz: [0.5, x, 0., 0.],
                    })
                })
                .unwrap();
            assert_eq!(state.precision_raised(), expected_precision, "x={x:?}");
            if x == f32::from_bits(1) {
                assert_eq!(matrix.elements[6].to_bits(), 0x8000_0001);
                assert_eq!(matrix.elements[9].to_bits(), 0x0000_0001);
            }
        }
    }

    #[test]
    fn normalization_records_inexact_reciprocal_with_exact_sqrt_and_products() {
        let mut exact = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            exact.run(|| normalized([0., 0., 2.])).unwrap(),
            [0., 0., 1.]
        );
        assert!(!exact.precision_raised());
        let mut inexact = crate::native_precision::Pc24PrecisionState::default();
        let result = inexact.run(|| normalized([1., 2., 2.])).unwrap();
        assert_eq!(result, [1. / 3., 2. / 3., 2. / 3.]);
        assert!(inexact.precision_raised());
    }

    #[test]
    fn checked_rotation_uses_needed_operands_and_preflights_rejected_arithmetic() {
        let mut matrix = SMatrix {
            elements: [f32::from_bits(0x7fc01234); 16],
        };
        matrix
            .try_set_rot3(SQuat {
                wxyz: [1., f32::NAN, f32::INFINITY, f32::from_bits(1)],
            })
            .unwrap();
        assert_eq!(matrix.elements[0], 1.);
        assert_eq!(matrix.elements[5], 1.);
        assert_eq!(matrix.elements[10], 1.);
        assert_eq!(matrix.elements[12].to_bits(), 0x7fc01234);
        let before = matrix.elements.map(f32::to_bits);
        assert_eq!(
            matrix.try_set_rot3(SQuat {
                wxyz: [0.5, f32::MAX, 0., 0.]
            }),
            Err(MatrixArithmeticUnavailable)
        );
        assert_eq!(matrix.elements.map(f32::to_bits), before);
    }
    #[test]
    fn checked_rotation_preserves_pc24_explicit_tiny_product_spills() {
        let mut matrix = SMatrix::identity();
        // Synthetic powers of two: xx spills to the minimum subnormal;
        // xz is exactly halfway below it and rounds to even zero.
        let tiny = f32::from_bits((127 - 75) << 23);
        matrix
            .try_set_rot3(SQuat {
                wxyz: [0.5, tiny, 0.75, tiny * 0.5],
            })
            .unwrap();
        let expected = [
            -0.125,
            tiny,
            0.75,
            tiny * 2.,
            1.,
            -tiny * 0.25,
            -0.75,
            tiny * 1.75,
            -0.125,
        ];
        let indices = [0, 1, 2, 4, 5, 6, 8, 9, 10];
        assert_eq!(
            indices.map(|index| matrix.elements[index].to_bits()),
            expected.map(f32::to_bits)
        );
        assert_eq!(
            [3, 7, 11, 12, 13, 14, 15].map(|i| matrix.elements[i]),
            [0., 0., 0., 0., 0., 0., 1.]
        );
    }
    #[test]
    fn checked_affine_and_point_preserve_native_association_and_ignore_columns() {
        let mut left = SMatrix::identity();
        let mut right = SMatrix::identity();
        for i in [3, 7, 11, 15] {
            left.elements[i] = f32::NAN;
            right.elements[i] = f32::INFINITY;
        }
        right.elements[4] = 1.;
        right.elements[8] = 1.;
        right.elements[12] = -100_000_000.;
        let point = [100_000_000., 1., 1.];
        assert_eq!(right.try_transform_point(point).unwrap()[0], 2.);
        left.elements[12..15].copy_from_slice(&point);
        assert_eq!(left.try_mul_affine(&right).unwrap().elements[12], 2.);
        right.elements[0] = f32::MIN_POSITIVE;
        right.elements[4] = 0.;
        right.elements[8] = 0.;
        right.elements[12] = 0.;
        assert_eq!(
            right.try_transform_point([0.5, 0., 0.]),
            Err(MatrixArithmeticUnavailable)
        );
        right.elements[0] = f32::MAX;
        assert_eq!(
            right.try_transform_point([2., 0., 0.]),
            Err(MatrixArithmeticUnavailable)
        );
    }

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
