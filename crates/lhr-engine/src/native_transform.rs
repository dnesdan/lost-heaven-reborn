//! Original exported LS3DF `S_matrix`/`S_quat` operations, using Game's
//! single-precision, round-to-nearest arithmetic policy (x87 control word 0x003f).
//! Row vectors multiply row-major 4×4 storage; translation occupies elements12–14.
//! These operations do not implement lazy frame caches.
//! Native comparisons cover ordinary finite inputs; extended-exponent x87 edge
//! cases (intermediate underflow/overflow), NaNs and other control words are not
//! established as equivalent to portable Rust arithmetic.
//! Callers requiring verified fidelity must check `is_verified_input` (and each
//! vector scalar), then recheck generated matrices before subsequent operations.
//! The arithmetic methods themselves do not reject values outside that domain.

/// Conservative per-call domain for equivalence to x87 PC24 arithmetic.
/// Nonzero magnitudes stay far enough from exponent limits for products and
/// cancellation residuals in these primitives to remain ordinary f32 values.
/// This is a Rust availability guard, not a condition in the original methods.
pub fn is_verified_scalar(value: f32) -> bool {
    value.is_finite() && (value == 0.0 || (1e-12..=1e12).contains(&value.abs()))
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
