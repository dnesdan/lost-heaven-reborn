//! Original exported LS3DF `S_matrix`/`S_quat` operations, using Game's
//! single-precision, round-to-nearest arithmetic policy (x87 control word 0x003f).
//! Row vectors multiply row-major 4×4 storage; translation occupies elements12–14.
//! These operations do not implement lazy frame caches.
//! Native comparisons cover ordinary finite inputs; extended-exponent x87 edge
//! cases (intermediate underflow/overflow), NaNs and other control words are not
//! established as equivalent to portable Rust arithmetic.
//! Callers requiring verified fidelity must check `is_verified_input` (and each
//! vector scalar), then recheck generated matrices before subsequent operations.
//! Legacy arithmetic methods do not reject values outside that domain. The
//! `try_make` entries enforce their documented bounded domains and rounding
//! enclosures, assuming the host default IEEE round-to-nearest arithmetic.

#[path = "native_trig.rs"]
pub(crate) mod native_trig;

/// Conservative per-call domain for equivalence to x87 PC24 arithmetic.
/// Nonzero magnitudes stay far enough from exponent limits for products and
/// cancellation residuals in these primitives to remain ordinary f32 values.
/// This is a Rust availability guard, not a condition in the original methods.
pub fn is_verified_scalar(value: f32) -> bool {
    value.is_finite() && (value == 0.0 || (1e-12..=1e12).contains(&value.abs()))
}

/// Descriptive mapping of the internal vector-length helper, RVA0x2f9f0.
/// Returns zero below the native squared-length cutoff, and exactly one for
/// squared lengths within 1e-8 of one. Callers must guard each input scalar.
pub fn vector_length(vector: [f32; 3]) -> f32 {
    let [x, y, z] = vector;
    let squared = (x * x + y * y) + z * z;
    if (squared.to_bits() as i32) < 0x322b_cc77 {
        0.0
    } else if (squared - 1.0).abs() < 1e-8 {
        1.0
    } else {
        squared.sqrt()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SMatrix {
    pub elements: [f32; 16],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SQuat {
    /// Original WXYZ component order.
    pub wxyz: [f32; 4],
}
/// Availability failures for the bounded original Make entry points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MakeUnavailable {
    InputOutsideVerifiedDomain,
    UnrecoveredAxisAngle,
    UnrecoveredMatrix,
    /// Conservative transcendental bounds cross a single-precision boundary.
    RoundingUncertain,
}

impl SMatrix {
    pub fn is_verified_input(&self) -> bool {
        self.elements.iter().copied().all(is_verified_scalar)
    }

    /// Original `S_matrix::Identity`, RVA0x2cf20.
    pub const fn identity() -> Self {
        Self {
            elements: [
                1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
            ],
        }
    }
    /// Original `S_matrix::SetRot3(S_quat const&)`, RVA0x2e070.
    /// Replaces only the3×3 basis, preserving the other seven elements. It does
    /// not normalize; abs(w)>=1 writes an identity basis regardless of XYZ.
    pub fn set_rot3(&mut self, quaternion: SQuat) {
        let [w, x, y, z] = quaternion.wxyz;
        let indices = [0, 1, 2, 4, 5, 6, 8, 9, 10];
        if w.abs() >= 1.0 {
            for (index, value) in indices
                .into_iter()
                .zip([1., 0., 0., 0., 1., 0., 0., 0., 1.])
            {
                self.elements[index] = value;
            }
            return;
        }
        let x2 = x + x;
        let y2 = y + y;
        let z2 = z + z;
        let xx = x2 * x;
        let xy = y2 * x;
        let xz = z2 * x;
        let yy = y2 * y;
        let yz = z2 * y;
        let zz = z2 * z;
        let wx = x2 * w;
        let wy = y2 * w;
        let wz = z2 * w;
        let basis = [
            1.0 - (zz + yy),
            xy - wz,
            wy + xz,
            wz + xy,
            1.0 - (zz + xx),
            yz - wx,
            xz - wy,
            wx + yz,
            1.0 - (yy + xx),
        ];
        for (index, value) in indices.into_iter().zip(basis) {
            self.elements[index] = value;
        }
    }
    /// Original `S_matrix::SetRot(S_quat const&)`, RVA0x2e1c0.
    /// Also resets translation and the homogeneous column.
    pub fn set_rot(&mut self, quaternion: SQuat) {
        self.set_rot3(quaternion);
        for index in [3, 7, 11, 12, 13, 14] {
            self.elements[index] = 0.0;
        }
        self.elements[15] = 1.0;
    }
    /// Original affine `S_matrix::operator*`/`operator*=`, RVAs0x2d3a0/0x2d220.
    /// Ignores both input homogeneous columns and forces output [0,0,0,1].
    /// Separate multiply/add operations preserve the native association.
    pub fn mul_affine(&self, right: &Self) -> Self {
        let a = &self.elements;
        let b = &right.elements;
        let mut result = Self::identity();
        for row in 0..4 {
            for column in 0..3 {
                let first = a[row * 4] * b[column];
                let second = a[row * 4 + 1] * b[4 + column];
                let third = a[row * 4 + 2] * b[8 + column];
                let initial = if row == 3 {
                    first + b[12 + column]
                } else {
                    first
                };
                result.elements[row * 4 + column] = (initial + second) + third;
            }
        }
        result
    }
    /// Original mutating `S_matrix::operator*=`, RVA0x2d220.
    pub fn mul_assign_affine(&mut self, right: &Self) {
        *self = self.mul_affine(right);
    }

    /// Original `S_vector::operator*(S_matrix const&)`, RVA0x2e980.
    /// Translation is added after the FIRST product, as in matrix composition.
    pub fn transform_point(&self, vector: [f32; 3]) -> [f32; 3] {
        let mut out = [0.; 3];
        for (column, output) in out.iter_mut().enumerate() {
            let first = vector[0] * self.elements[column];
            let translated = first + self.elements[12 + column];
            let second = vector[1] * self.elements[4 + column];
            let third = vector[2] * self.elements[8 + column];
            *output = (translated + second) + third;
        }
        out
    }
}
impl SQuat {
    pub fn is_verified_input(&self) -> bool {
        self.wxyz.iter().copied().all(is_verified_scalar)
    }

    /// Original `S_quat::Make(S_vector const&, float)`, RVA0x2ee00.
    /// Nonzero angles are bounded to [-2,2] radians and use outward enclosures
    /// of retained x87 sin/cos. An ambiguous PC24 rounding returns an error.
    /// Signed zero angles retain the previously admitted full finite-axis domain.
    pub fn try_make_axis_angle(axis: [f32; 3], angle: f32) -> Result<Self, MakeUnavailable> {
        if !angle.is_finite() || !axis.into_iter().all(f32::is_finite) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        if angle == 0.0 {
            return Ok(Self {
                wxyz: [1., angle * axis[0], angle * axis[1], angle * axis[2]],
            });
        }
        if !axis.into_iter().all(is_verified_scalar) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        let half = angle * 0.5;
        if !half.is_normal() {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        let (sine, cosine) = native_trig::sin_cos(half)?;
        let w = cosine.rounded()?;
        let [x, y, z] = [
            sine.product(axis[0])?,
            sine.product(axis[1])?,
            sine.product(axis[2])?,
        ];
        let arithmetic = native_trig::Arithmetic::new();
        let f = |v| arithmetic.input(v);
        let [px, py, pz] = [x, y, z].map(f);
        let norm = ((px * px + py * py) + pz * pz) + f(cosine.squared()?);
        let mut out = Self { wxyz: [w, x, y, z] };
        if f64::from((norm - f(1.)).abs().value) >= 1e-8 {
            if norm.value < 1e-8 {
                arithmetic.finish()?;
                return Ok(Self {
                    wxyz: [1., 0., 0., 0.],
                });
            }
            let reciprocal = f(1.) / norm.sqrt();
            for i in [1, 2, 3, 0] {
                out.wxyz[i] = (f(out.wxyz[i]) * reciprocal).value;
            }
        }
        arithmetic.finish()?;
        Ok(out)
    }

    /// Original `S_quat::Make(S_matrix const&)`, RVA0x2eef0, in a guarded domain.
    /// Nondegenerate matrices follow the original two axis-angle factors and
    /// composition; each nonzero factor angle must be at most two radians.
    /// PC24 exponent edges and uncertain transcendental rounding return errors.
    /// Retains the admitted zero-length and exact diagonal identity branches.
    /// Matrix admission checks each PC24 intermediate, including row lengths;
    /// ordinary small basis components need not satisfy the legacy scalar guard.
    /// Missing-row reconstruction remains unavailable.
    pub fn try_make_from_matrix(matrix: &SMatrix) -> Result<Self, MakeUnavailable> {
        let arithmetic = native_trig::Arithmetic::new();
        let f = |value| arithmetic.input(value);
        for value in matrix.elements {
            f(value);
        }
        arithmetic.finish()?;
        let m = &matrix.elements;
        let lengths = [0, 4, 8].map(|i| {
            let [x, y, z] = [m[i], m[i + 1], m[i + 2]].map(f);
            let squared = (x * x + y * y) + z * z;
            if (squared.value.to_bits() as i32) < 0x322b_cc77 {
                0.
            } else if (squared - f(1.)).abs().value < 1e-8 {
                1.
            } else {
                squared.sqrt().value
            }
        });
        let empty_basis = ((f(lengths[2]) + f(lengths[1])) + f(lengths[0])).value < 1e-8;
        arithmetic.finish()?;
        let diagonal = [1, 2, 4, 6, 8, 9].into_iter().all(|i| m[i].to_bits() == 0);
        let power = |value: f32| {
            let bits = value.abs().to_bits();
            bits & 0x7f_ffff == 0 && (119..=135).contains(&((bits >> 23) & 0xff))
        };
        let scaled_identity =
            diagonal && m[0] > 0.0 && m[5] > 0.0 && [m[0], m[5], m[10]].into_iter().all(power);
        if empty_basis || scaled_identity {
            Ok(Self {
                wxyz: [1., 0., 0., 0.],
            })
        } else {
            Self::make_nonzero_matrix(matrix, lengths)
        }
    }

    fn make_nonzero_matrix(matrix: &SMatrix, lengths: [f32; 3]) -> Result<Self, MakeUnavailable> {
        use native_trig::{Arithmetic, Float};
        let arithmetic = Arithmetic::new();
        let f = |v| arithmetic.input(v);
        let [one, zero] = [f(1.), f(0.)];
        let [lx, ly, lz] = lengths.map(f);
        if ((lz * ly) * lx).value < 1e-8 {
            return Err(MakeUnavailable::UnrecoveredMatrix);
        }
        let m = matrix.elements.map(f);
        let ix = one / lx;
        let iy = one / ly;
        let v = [iy * m[4], iy * m[5], iy * m[6]];
        let sum = [ix * m[0] + v[0], v[1] + ix * m[1], v[2] + ix * m[2]];
        fn normalize<'a>(
            mut a: [Float<'a>; 3],
            squared: Float<'a>,
            one: Float<'a>,
        ) -> [Float<'a>; 3] {
            if f64::from((squared - one).abs().value) >= 1e-8 {
                if squared.value >= 1e-8 {
                    let reciprocal = one / squared.sqrt();
                    a = a.map(|v| v * reciprocal);
                } else {
                    let i = if a[0].value != 0. {
                        0
                    } else if a[2].value != 0. {
                        2
                    } else {
                        1
                    };
                    a[i] = if a[i].value < 0. { -one } else { one };
                }
            }
            a
        }
        let acos = |x: Float<'_>| -> Result<f32, MakeUnavailable> {
            arithmetic.finish()?;
            if x.value.abs() > 1. {
                return Err(MakeUnavailable::UnrecoveredMatrix);
            }
            let y = ((one + x) * (one - x)).sqrt();
            arithmetic.finish()?;
            native_trig::atan2_positive(y.value, x.value)
        };
        let first = if v[1].value.abs() < 1. {
            let axis = [v[2], zero, -v[0]];
            let axis = normalize(axis, axis[2] * axis[2] + axis[0] * axis[0], one);
            Self::try_make_axis_angle(axis.map(|v| -v.value), acos(v[1])?)?
        } else if v[1].value <= 0. {
            Self {
                wxyz: [0., 0., 0., 1.],
            }
        } else {
            Self {
                wxyz: [1., 0., 0., 0.],
            }
        };
        let [w, x, y, z] = first.wxyz.map(f);
        let a = if w.value.abs() < 1. {
            let x2 = x + x;
            let y2 = y + y;
            let z2 = z + z;
            [
                ((one - (y * y2 + z * z2)) + z2 * w) + y2 * x,
                ((y2 * x - z2 * w) + one) - (x * x2 + z * z2),
                ((y2 * w + z2 * x) + y * z2) - x2 * w,
            ]
        } else {
            [one, one, zero]
        };
        let d = [a[0] - v[0], a[1] - v[1], a[2] - v[2]];
        let from = [sum[0] - v[0], sum[1] - v[1], sum[2] - v[2]];
        let dot = (d[2] * from[2] + from[1] * d[1]) + from[0] * d[0];
        let second = if dot.value.abs() >= 1. {
            let triple = ((d[2] * v[1] - d[1] * v[2]) * sum[0]
                + (d[1] * v[0] - v[1] * d[0]) * sum[2])
                + (v[2] * d[0] - d[2] * v[0]) * sum[1];
            let axis = if triple.value <= 0. { v } else { v.map(|v| -v) };
            let angle = if dot.value <= 0. {
                f32::from_bits(0x40490fdb)
            } else {
                0.
            };
            arithmetic.finish()?;
            Self::try_make_axis_angle(axis.map(|v| v.value), angle)?
        } else {
            let axis = [
                d[1] * from[2] - from[1] * d[2],
                d[2] * from[0] - from[2] * d[0],
                from[1] * d[0] - d[1] * from[0],
            ];
            let squared = (axis[2] * axis[2] + axis[1] * axis[1]) + axis[0] * axis[0];
            let axis = normalize(axis, squared, one);
            Self::try_make_axis_angle(axis.map(|v| -v.value), acos(dot)?)?
        };
        arithmetic.finish()?;
        let [w, x, y, z] = first.wxyz.map(f);
        let [rw, rx, ry, rz] = second.wxyz.map(f);
        let dot = (z * rz + y * ry) + x * rx;
        let result = Self {
            wxyz: [
                (w * rw - dot).value,
                ((rx * w + x * rw) + (rz * y - ry * z)).value,
                ((ry * w + y * rw) + (rx * z - rz * x)).value,
                ((rz * w + z * rw) + (ry * x - rx * y)).value,
            ],
        };
        arithmetic.finish()?;
        if !result.is_verified_input() {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        Ok(result)
    }

    /// Descriptive mapping of the internal quaternion-composition helper,
    /// RVA0x2fa70. Returns the WXYZ product without normalization; separate
    /// products and sums retain the native association.
    pub fn compose(self, right: Self) -> Self {
        let [w, x, y, z] = self.wxyz;
        let [rw, rx, ry, rz] = right.wxyz;
        let dot = (z * rz + y * ry) + x * rx;
        Self {
            wxyz: [
                w * rw - dot,
                (rx * w + x * rw) + (rz * y - ry * z),
                (ry * w + y * rw) + (rx * z - rz * x),
                (rz * w + z * rw) + (ry * x - y * rx),
            ],
        }
    }

    /// Original `S_quat::Normalize`, RVA0x30140, under the verified input policy.
    /// Degenerate input selects X, then Z, then Y, preserving other components;
    /// an all-zero quaternion therefore becomes [0,0,1,0], not identity.
    pub fn normalize(&mut self) {
        let [w, x, y, z] = self.wxyz;
        let norm_squared = ((w * w + x * x) + y * y) + z * z;
        if norm_squared <= 1e-8 {
            self.wxyz[0] = 0.0;
            let axis = if x != 0.0 {
                1
            } else if z != 0.0 {
                3
            } else {
                2
            };
            self.wxyz[axis] = if self.wxyz[axis] < 0.0 { -1.0 } else { 1.0 };
        } else if (norm_squared - 1.0).abs() >= 1e-8 {
            let reciprocal = 1.0 / norm_squared.sqrt();
            for axis in [1, 2, 3, 0] {
                self.wxyz[axis] *= reciprocal;
            }
        }
    }

    /// Original `S_quat::RotationMatrix`, RVA0x2ec00; returns a complete matrix.
    pub fn rotation_matrix(self) -> SMatrix {
        let mut matrix = SMatrix::identity();
        matrix.set_rot3(self);
        matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn general_make_matches_original_nonzero_axis_and_matrix_paths() {
        assert_eq!(
            SQuat::try_make_axis_angle([2., 3., 4.], 1.)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            [0x3ea4c6c3, 0x3eb40919, 0x3f0706d2, 0x3f340919]
        );
        let matrix = SMatrix {
            elements: [
                1., 0., 0., 0., 0., 0., -1., 0., 0., 1., 0., 0., 10., 20., 30., 1.,
            ],
        };
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            [0x3f3504f4, 0x3f3504f4, 0, 0]
        );
        let matrix = SMatrix {
            elements: [
                0.5,
                0.,
                f32::from_bits(0x3f5db3d7),
                0.,
                0.,
                1.,
                0.,
                0.,
                -f32::from_bits(0x3f5db3d7),
                0.,
                0.5,
                0.,
                0.,
                0.,
                0.,
                1.,
            ],
        };
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            [0x3f5db3d7, 0, 0x3effffff, 0]
        );
    }

    #[test]
    fn general_make_fails_closed_for_unrecovered_or_uncertain_numeric_paths() {
        assert_eq!(
            SQuat::try_make_axis_angle([1., 0., 0.], 1e-20),
            Err(MakeUnavailable::RoundingUncertain)
        );
        assert_eq!(
            SQuat::try_make_axis_angle([1., 0., 0.], f32::from_bits(1)),
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        );
        let mut matrix = SMatrix::identity();
        matrix.elements[5] = -1.;
        matrix.elements[6] = -1.;
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix),
            Err(MakeUnavailable::UnrecoveredAxisAngle)
        );
        matrix = SMatrix::identity();
        matrix.elements[0] = 0.;
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix),
            Err(MakeUnavailable::UnrecoveredMatrix)
        );
    }

    #[test]
    fn matrix_make_checks_small_basis_intermediates_instead_of_blanket_magnitudes() {
        // Synthetic original-PC Make comparisons: all eight outputs are identity.
        for exponent in [40, 48, 54, 60] {
            for sign in [-1., 1.] {
                let tiny = sign * 2f32.powi(-exponent);
                let mut matrix = SMatrix::identity();
                matrix.elements[1] = tiny;
                matrix.elements[4] = -tiny;
                assert!(!matrix.is_verified_input());
                assert_eq!(
                    SQuat::try_make_from_matrix(&matrix)
                        .unwrap()
                        .wxyz
                        .map(f32::to_bits),
                    [1f32.to_bits(), 0, 0, 0]
                );
            }
        }
        // A tiny product may disappear into an otherwise ordinary row length;
        // the wider native exponent remains unsupported even after that sum.
        for tiny in [f32::from_bits(1), 2f32.powi(-64)] {
            let mut matrix = SMatrix::identity();
            matrix.elements[1] = tiny;
            assert_eq!(
                SQuat::try_make_from_matrix(&matrix),
                Err(MakeUnavailable::InputOutsideVerifiedDomain)
            );
        }
        let mut matrix = SMatrix::identity();
        matrix.elements[0] = f32::MAX;
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix),
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        );
    }

    #[test]
    fn bounded_make_preserves_signed_zero_and_rejects_unrecovered_angles() {
        for angle in [0., -0.] {
            let axis = [-2., 0., -0.];
            let result = SQuat::try_make_axis_angle(axis, angle).unwrap();
            assert_eq!(
                result.wxyz.map(f32::to_bits),
                [
                    1.0f32.to_bits(),
                    (angle * -2.).to_bits(),
                    (angle * 0.).to_bits(),
                    (angle * -0.).to_bits()
                ]
            );
        }
        assert!(SQuat::try_make_axis_angle([1., 0., 0.], 3.).is_err());
        assert!(SQuat::try_make_axis_angle([f32::INFINITY, 0., 0.], 0.).is_err());
        assert!(SQuat::try_make_axis_angle([1., 0., 0.], f32::NAN).is_err());
    }

    #[test]
    fn bounded_matrix_make_preserves_original_identity_paths() {
        let expected = [1.0f32.to_bits(), 0, 0, 0];
        let mut matrix = SMatrix::identity();
        matrix.elements[0] = 2.;
        matrix.elements[5] = 0.25;
        matrix.elements[10] = -8.;
        matrix.elements[12..15].copy_from_slice(&[10., 20., 30.]);
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
        matrix.elements[0] = 3.;
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
        matrix.elements[0] = -2.;
        assert!(SQuat::try_make_from_matrix(&matrix).is_err());
        matrix.elements = [1e-5; 16];
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
        matrix.elements[0] = 0.001;
        assert!(SQuat::try_make_from_matrix(&matrix).is_err());
        matrix.elements[0] = f32::NAN;
        assert!(SQuat::try_make_from_matrix(&matrix).is_err());
    }

    #[test]
    fn vector_length_cutoff_and_ordinary_values() {
        assert_eq!(vector_length([0.; 3]).to_bits(), 0);
        assert_eq!(vector_length([-0.; 3]).to_bits(), 0);
        assert_eq!(vector_length([0.00001, 0., 0.]), 0.);
        assert_eq!(vector_length([0.0001, 0., 0.]), 0.);
        assert_eq!(vector_length([0.00011, 0., 0.]), 0.00011);
        assert_eq!(vector_length([1., 0., 0.]), 1.);
        assert_eq!(vector_length([3., 4., 0.]), 5.);
    }

    #[test]
    fn quaternion_composition_preserves_order_and_does_not_normalize() {
        let a = SQuat {
            wxyz: [2., 3., 4., 5.],
        };
        let b = SQuat {
            wxyz: [7., 11., 13., 17.],
        };
        assert_eq!(a.compose(b).wxyz, [-156., 46., 58., 64.]);
        assert_eq!(b.compose(a).wxyz, [-156., 40., 50., 74.]);
        let identity = SQuat {
            wxyz: [1., 0., 0., 0.],
        };
        assert_eq!(a.compose(identity), a);
    }

    #[test]
    fn availability_guard_rejects_unverified_exponent_ranges() {
        for value in [0., -0., 1e-12, -1e12, 0.75] {
            assert!(is_verified_scalar(value));
        }
        for value in [f32::NAN, f32::INFINITY, f32::MAX, f32::MIN_POSITIVE, 1e-20] {
            assert!(!is_verified_scalar(value));
        }
        assert!(SMatrix::identity().is_verified_input());
        assert!(!SQuat {
            wxyz: [1., 1e-20, 0., 0.]
        }
        .is_verified_input());
    }

    #[test]
    fn zero_quaternion_normalizes_to_y_axis_with_signed_zeros_preserved() {
        let mut zero = SQuat { wxyz: [0.; 4] };
        zero.normalize();
        assert_eq!(zero.wxyz, [0., 0., 1., 0.]);
        let mut negative_zero = SQuat { wxyz: [-0.; 4] };
        negative_zero.normalize();
        assert_eq!(
            negative_zero.wxyz.map(f32::to_bits),
            [0, 0x80000000, 0x3f800000, 0x80000000]
        );
    }

    #[test]
    fn degenerate_normalization_selects_x_then_z_then_y() {
        let mut x = SQuat {
            wxyz: [1e-12, -1e-12, 2e-12, 3e-12],
        };
        x.normalize();
        assert_eq!(x.wxyz, [0., -1., 2e-12, 3e-12]);
        let mut z = SQuat {
            wxyz: [1e-12, 0., 2e-12, -3e-12],
        };
        z.normalize();
        assert_eq!(z.wxyz, [0., 0., 2e-12, -1.]);
        let mut y = SQuat {
            wxyz: [1e-12, 0., -2e-12, 0.],
        };
        y.normalize();
        assert_eq!(y.wxyz, [0., 0., -1., 0.]);
    }

    #[test]
    fn nonunit_normalization_matches_original_body_vector() {
        let mut quaternion = SQuat {
            wxyz: [2., 3., 4., 5.],
        };
        quaternion.normalize();
        assert_eq!(
            quaternion.wxyz.map(f32::to_bits),
            [1049319752, 1053885932, 1057708360, 1059991450]
        );
        let mut already_unit_by_rounded_norm = SQuat {
            wxyz: [1., 1e-12, 0., 0.],
        };
        let original = already_unit_by_rounded_norm;
        already_unit_by_rounded_norm.normalize();
        assert_eq!(already_unit_by_rounded_norm, original);
    }

    #[test]
    fn rotation_basis_preserves_other_seven_fields() {
        let mut matrix = SMatrix {
            elements: std::array::from_fn(|i| i as f32 + 10.),
        };
        let original = matrix;
        matrix.set_rot3(SQuat {
            wxyz: [1., 4., 5., 6.],
        });
        for index in [3, 7, 11, 12, 13, 14, 15] {
            assert_eq!(matrix.elements[index], original.elements[index]);
        }
        for index in [0, 5, 10] {
            assert_eq!(matrix.elements[index], 1.);
        }
        for index in [1, 2, 4, 6, 8, 9] {
            assert_eq!(matrix.elements[index], 0.);
        }
        matrix.set_rot(SQuat {
            wxyz: [-1., 4., 5., 6.],
        });
        assert_eq!(matrix, SMatrix::identity());
    }
    #[test]
    fn row_vector_rotation_and_translation() {
        let matrix = SQuat {
            wxyz: [0., 0., 0., 1.],
        }
        .rotation_matrix();
        assert_eq!(matrix.transform_point([1., 2., 3.]), [-1., -2., 3.]);
        let mut translation = SMatrix::identity();
        translation.elements[12..15].copy_from_slice(&[4., 5., 6.]);
        assert_eq!(
            matrix
                .mul_affine(&translation)
                .transform_point([1., 2., 3.]),
            [3., 3., 9.]
        );
    }
    #[test]
    fn affine_composition_ignores_homogeneous_inputs() {
        let mut left = SMatrix::identity();
        let mut right = left;
        for i in [3, 7, 11, 15] {
            left.elements[i] = 99.;
            right.elements[i] = -99.;
        }
        assert_eq!(left.mul_affine(&right), SMatrix::identity());
    }
    #[test]
    fn translation_is_added_before_remaining_products() {
        let mut matrix = SMatrix::identity();
        matrix.elements[0] = 100_000_000.;
        matrix.elements[4] = -100_000_000.;
        matrix.elements[8] = 1.;
        matrix.elements[12] = 1.;
        assert_eq!(matrix.transform_point([1., 1., 1.])[0], 1.);
        let mut left = SMatrix::identity();
        left.elements[12..15].copy_from_slice(&[1., 1., 1.]);
        assert_eq!(left.mul_affine(&matrix).elements[12], 1.);
    }
    #[test]
    fn synthetic_original_body_float32_vector() {
        let left = SMatrix {
            elements: [
                1132897885, 3290022936, 3276362493, 3295664613, 3296035574, 3246909545, 3291864004,
                3293763908, 3258347808, 3255140371, 1146150210, 1135910550, 3294676825, 1144396121,
                1124599056, 3290013247,
            ]
            .map(f32::from_bits),
        };
        let right = SMatrix {
            elements: [
                1118177642, 1133222786, 3294167647, 1142128313, 1134482369, 1121164527, 1147922827,
                3285913938, 3287969606, 1137962374, 3288374497, 3290412376, 1142779689, 1124124083,
                3290892496, 3294797754,
            ]
            .map(f32::from_bits),
        };
        let quat = SQuat {
            wxyz: [1037606066, 3204656677, 3218552140, 1052799482].map(f32::from_bits),
        };
        let mut basis = left;
        basis.set_rot3(quat);
        assert_eq!(
            basis.elements.map(f32::to_bits),
            [
                3231577412, 1070751938, 3208489308, 3295664613, 1072085888, 1044703132, 3214141411,
                3293763908, 3169981760, 3215959985, 3232086178, 1135910550, 3294676825, 1144396121,
                1124599056, 3290013247
            ]
        );
        assert_eq!(
            left.mul_affine(&right).elements.map(f32::to_bits),
            [
                3347905511, 3348293846, 3375201332, 0, 1216837150, 3373181030, 1234421973, 0,
                3369294180, 1218770650, 3368937752, 0, 1202604633, 3353539032, 1235914141,
                1065353216
            ]
        );
        assert_eq!(
            right
                .transform_point([1139435458, 1144110426, 3279650317].map(f32::from_bits))
                .map(f32::to_bits),
            [1220482865, 1203938210, 1220524765]
        );
    }
}
