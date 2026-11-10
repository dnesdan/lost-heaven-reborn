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
use crate::native_precision::{add, div, mul, sqrt, sub};

#[path = "native_frame_direction.rs"]
mod frame_direction;
#[path = "native_retained_angle.rs"]
mod retained_angle;
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub(crate) use native_trig::vector_angle::try_ph_thing_friction_products;
pub use native_trig::vector_angle::try_vector_angle_to;
pub use native_trig::car_resource_math::animation_angle;
pub use native_trig::car_resource_math::rotate_y;
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub use native_trig::vector_angle::{
    try_acos_sin_cos_products, try_cosine_product, try_vector_angle_sine,
    try_vector_angle_sine_scaled, try_wheel_friction_limit,
};
pub use retained_angle::{try_retained_vector_angle, RetainedVectorAngle};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[path = "native_vector_x87.rs"]
mod vector_x87;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(crate) use vector_x87::{
    try_acos_sin_cos_products, try_cosine_product, try_ph_thing_friction_products,
    try_vector_angle_sine, try_vector_angle_sine_scaled, try_wheel_friction_limit,
};
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
    let squared = add(add(mul(x, x), mul(y, y)), mul(z, z));
    if (squared.to_bits() as i32) < 0x322b_cc77 {
        0.0
    } else if sub(squared, 1.0).abs() < 1e-8 {
        1.0
    } else {
        sqrt(squared)
    }
}

/// Checked counterpart of the existing Magnitude arithmetic, shared by
/// GetScale and the missing-row reconstruction. All intermediate results stay
/// in the same admitted PC24 exponent domain as the rest of Make.
fn matrix_row_length(row: [f32; 3], arithmetic: &native_trig::Arithmetic) -> f32 {
    let [x, y, z] = row.map(|value| arithmetic.input(value));
    let squared = (x * x + y * y) + z * z;
    if (squared.value.to_bits() as i32) < 0x322b_cc77 {
        0.
    } else if (squared - arithmetic.input(1.)).abs().value < 1e-8 {
        1.
    } else {
        squared.sqrt().value
    }
}
fn matrix_row_lengths(matrix: &SMatrix, arithmetic: &native_trig::Arithmetic) -> [f32; 3] {
    [0, 4, 8].map(|i| matrix_row_length(matrix.elements[i..i + 3].try_into().unwrap(), arithmetic))
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

/// Rounded FSIN/FCOS stores for the admitted reduced-angle domain. Reuses the
/// retained x87 enclosures and the active owner's sticky precision state.
pub(crate) fn try_rounded_sin_cos(angle: f32) -> Result<(f32, f32), MakeUnavailable> {
    let (sine, cosine) = native_trig::sin_cos(angle)?;
    Ok((sine.rounded()?, cosine.rounded()?))
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
        let x2 = add(x, x);
        let y2 = add(y, y);
        let z2 = add(z, z);
        let xx = mul(x2, x);
        let xy = mul(y2, x);
        let xz = mul(z2, x);
        let yy = mul(y2, y);
        let yz = mul(z2, y);
        let zz = mul(z2, z);
        let wx = mul(x2, w);
        let wy = mul(y2, w);
        let wz = mul(z2, w);
        let basis = [
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
                let first = mul(a[row * 4], b[column]);
                let second = mul(a[row * 4 + 1], b[4 + column]);
                let third = mul(a[row * 4 + 2], b[8 + column]);
                let initial = if row == 3 {
                    add(first, b[12 + column])
                } else {
                    first
                };
                result.elements[row * 4 + column] = add(add(initial, second), third);
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
            let first = mul(vector[0], self.elements[column]);
            let translated = add(first, self.elements[12 + column]);
            let second = mul(vector[1], self.elements[4 + column]);
            let third = mul(vector[2], self.elements[8 + column]);
            *output = add(add(translated, second), third);
        }
        out
    }
}
impl SQuat {
    pub fn is_verified_input(&self) -> bool {
        self.wxyz.iter().copied().all(is_verified_scalar)
    }

    /// Original `S_quat::Make(S_vector const&, float)`, RVA0x2ee00.
    /// Nonzero angles are bounded to [-2,2] radians. On x86, the CPU retains
    /// original x87 sine/cosine values under an isolated PC24/RN environment;
    /// other architectures use outward enclosures and reject ambiguous rounding.
    /// Signed zero angles retain the admitted full finite-axis domain.
    pub fn try_make_axis_angle(axis: [f32; 3], angle: f32) -> Result<Self, MakeUnavailable> {
        Self::make_axis_angle_domain(axis, angle, false)
    }
    /// The same original Make entry used by frame SetDir. On x86 the raw angle
    /// may have magnitude below 2^64: its half stays inside the original x87
    /// FSIN/FCOS argument range. Portable enclosures retain their [-8,8] domain
    /// and reject uncertain rounding. No angle reduction is performed here.
    /// This is an expanded host availability domain,
    /// not a separate original function or an approximation of vehicle motion.
    pub fn try_make_axis_angle_reduced(
        axis: [f32; 3],
        angle: f32,
    ) -> Result<Self, MakeUnavailable> {
        Self::make_axis_angle_domain(axis, angle, true)
    }
    fn make_axis_angle_domain(
        axis: [f32; 3],
        angle: f32,
        frame_domain: bool,
    ) -> Result<Self, MakeUnavailable> {
        if !angle.is_finite() || !axis.into_iter().all(f32::is_finite) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        if angle == 0.0 {
            return Ok(Self {
                wxyz: [1., angle * axis[0], angle * axis[1], angle * axis[2]],
            });
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        if !axis.into_iter().all(is_verified_scalar) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        let half = angle * 0.5;
        if !half.is_normal() {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        if !frame_domain && half.abs() > 1. {
            return Err(MakeUnavailable::UnrecoveredAxisAngle);
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        if frame_domain && half.abs() > 4. {
            return Err(MakeUnavailable::UnrecoveredAxisAngle);
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            vector_x87::try_make_axis_angle(axis, angle).map(|wxyz| Self { wxyz })
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let (sine, cosine) = if frame_domain {
                native_trig::sin_cos_frame(half)?
            } else {
                native_trig::sin_cos(half)?
            };
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
    }

    /// Original `S_quat::Make(S_matrix const&)`, RVA0x2eef0, in a guarded domain.
    /// Nondegenerate matrices follow the original two axis-angle factors and
    /// composition. Internal factors use retained x87 operations on x86 and
    /// checked portable enclosures elsewhere, through the original pi branch.
    /// PC24 exponent edges and uncertain transcendental rounding return errors.
    /// Retains the admitted zero-length and exact diagonal identity branches.
    /// Matrix admission checks each PC24 intermediate, including row lengths;
    /// ordinary small basis components need not satisfy the legacy scalar guard.
    /// Missing rows follow the pinned source's cross-product/fallback and
    /// second-scale check before re-entering the existing nondegenerate path.
    /// This branch is source-grounded; native runtime parity is not established.
    pub fn try_make_from_matrix(matrix: &SMatrix) -> Result<Self, MakeUnavailable> {
        let arithmetic = native_trig::Arithmetic::new();
        let f = |value| arithmetic.input(value);
        for value in matrix.elements {
            f(value);
        }
        arithmetic.finish()?;
        let m = &matrix.elements;
        let lengths = matrix_row_lengths(matrix, &arithmetic);
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
        let missing_axis = ((lz * ly) * lx).value < 1e-8;
        arithmetic.finish()?;
        if missing_axis {
            return Self::make_missing_axis_matrix(matrix, lengths);
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
            Self::make_axis_angle_domain(axis.map(|v| -v.value), acos(v[1])?, true)?
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
            Self::make_axis_angle_domain(axis.map(|v| v.value), angle, true)?
        } else {
            let axis = [
                d[1] * from[2] - from[1] * d[2],
                d[2] * from[0] - from[2] * d[0],
                from[1] * d[0] - d[1] * from[0],
            ];
            let squared = (axis[2] * axis[2] + axis[1] * axis[1]) + axis[0] * axis[0];
            let axis = normalize(axis, squared, one);
            Self::make_axis_angle_domain(axis.map(|v| -v.value), acos(dot)?, true)?
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
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        let result_valid = result
            .wxyz
            .into_iter()
            .all(|value| value == 0. || value.is_normal());
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        let result_valid = result.is_verified_input();
        if !result_valid {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        Ok(result)
    }

    /// Source-only missing-axis branch from I3d_math.cpp at pinned mafia-re
    /// 29b65b0. Cross products ALWAYS read the original matrix, including when
    /// multiple rows are missing. In particular the Y row is X cross Z; changing
    /// this to the more usual Z cross X changes the original behavior.
    fn make_missing_axis_matrix(
        matrix: &SMatrix,
        lengths: [f32; 3],
    ) -> Result<Self, MakeUnavailable> {
        let arithmetic = native_trig::Arithmetic::new();
        let f = |v| arithmetic.input(v);
        let original = [0, 4, 8].map(|i| {
            [
                matrix.elements[i],
                matrix.elements[i + 1],
                matrix.elements[i + 2],
            ]
            .map(f)
        });
        let mut rebuilt = *matrix;
        for (row, (left, right)) in [(1, 2), (0, 2), (0, 1)].into_iter().enumerate() {
            if lengths[row] >= 1e-8 {
                continue;
            }
            let [a, b] = [original[left], original[right]];
            let cross = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
            .map(|v| v.value);
            arithmetic.finish()?;
            let length = matrix_row_length(cross, &arithmetic);
            arithmetic.finish()?;
            let value = if length < 1e-8 {
                let mut unit = [0.; 3];
                unit[row] = 1.;
                unit
            } else {
                cross
            };
            rebuilt.elements[row * 4..row * 4 + 3].copy_from_slice(&value);
        }
        let [lx, ly, lz] = matrix_row_lengths(&rebuilt, &arithmetic).map(f);
        let still_degenerate = ((lz * ly) * lx).value < 1e-8;
        arithmetic.finish()?;
        if still_degenerate {
            return Ok(Self {
                wxyz: [1., 0., 0., 0.],
            });
        }
        // The identical checked scale-product condition above guarantees this
        // re-entry takes the nondegenerate path instead of recurring again.
        Self::try_make_from_matrix(&rebuilt)
    }

    /// Descriptive mapping of the internal quaternion-composition helper,
    /// RVA0x2fa70. Returns the WXYZ product without normalization; separate
    /// products and sums retain the native association.
    pub fn compose(self, right: Self) -> Self {
        let [w, x, y, z] = self.wxyz;
        let [rw, rx, ry, rz] = right.wxyz;
        let dot = add(add(mul(z, rz), mul(y, ry)), mul(x, rx));
        Self {
            wxyz: [
                sub(mul(w, rw), dot),
                add(add(mul(rx, w), mul(x, rw)), sub(mul(rz, y), mul(ry, z))),
                add(add(mul(ry, w), mul(y, rw)), sub(mul(rx, z), mul(rz, x))),
                add(add(mul(rz, w), mul(z, rw)), sub(mul(ry, x), mul(y, rx))),
            ],
        }
    }

    /// Checked original Normalize, RVA0x30140. Unlike the generic transform
    /// domain, this admits tiny normal components when every reached PC24
    /// arithmetic result remains normal or zero. Extended-exponent paths stay
    /// unavailable; no component is clamped or replaced to admit an input.
    pub fn try_normalize(&mut self) -> Result<(), MakeUnavailable> {
        let arithmetic = native_trig::Arithmetic::new();
        let [w, x, y, z] = self.wxyz.map(|v| arithmetic.input(v));
        let squared = ((w * w + x * x) + y * y) + z * z;
        arithmetic.finish()?;
        let mut result = *self;
        if squared.value <= 1e-8_f32 {
            result.wxyz[0] = 0.;
            let axis = if x.value != 0. {
                1
            } else if z.value != 0. {
                3
            } else {
                2
            };
            result.wxyz[axis] = if result.wxyz[axis] < 0. { -1. } else { 1. };
        } else {
            let difference = (squared - arithmetic.input(1.)).abs();
            arithmetic.finish()?;
            if f64::from(difference.value) >= f64::from(1e-8_f32) {
                let reciprocal = arithmetic.input(1.) / squared.sqrt();
                for axis in [1, 2, 3, 0] {
                    result.wxyz[axis] = (arithmetic.input(self.wxyz[axis]) * reciprocal).value;
                }
                arithmetic.finish()?;
            }
        }
        *self = result;
        Ok(())
    }

    /// Original `S_quat::Normalize`, RVA0x30140, under the verified input policy.
    /// Degenerate input selects X, then Z, then Y, preserving other components;
    /// an all-zero quaternion therefore becomes [0,0,1,0], not identity.
    pub fn normalize(&mut self) {
        let [w, x, y, z] = self.wxyz;
        let norm_squared = add(add(add(mul(w, w), mul(x, x)), mul(y, y)), mul(z, z));
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
        } else if sub(norm_squared, 1.0).abs() >= 1e-8 {
            let reciprocal = div(1.0, sqrt(norm_squared));
            for axis in [1, 2, 3, 0] {
                self.wxyz[axis] = mul(self.wxyz[axis], reciprocal);
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

// ---------------------------------------------------------------------------
// Wave-3 math ports (LS3DF RVAs 0x2cda0-0x30440; Ghidra bodies formed for all;
// tiers per item below). Precision: checked-f32 domain (finite normal-or-zero
// operands and intermediates; subnormal/Inf/NaN or widened-exponent results
// are explicit Err, never substituted). Separate f32 mul/add (native_precision
//::{mul,add,sub,div,sqrt}), no FMA. Transcendentals (acos/cos-half) use host
// libm on guarded domains — disclosed per item, tier capped accordingly.

/// LS3DF 0x2cda0 RandFloat mapping. The CRT `rand()` call itself is a backend
/// contract; this pure step is `sample * (1.0f/RAND_MAX)` in f32 — reciprocal
/// then multiply, no wider intermediate (a binary64-mediated product differs
/// up to 1 ulp). Out-of-range samples are refused (native `rand()` never
/// exceeds RAND_MAX=0x7fff on the CRT path — disclosed domain guard).
pub fn rand_float_sample(rand_result: u32) -> Result<f32, MakeUnavailable> {
    if rand_result > 0x7fff {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let v = mul(rand_result as f32, div(1.0, 32767.0));
    if !v.is_finite() || (v != 0.0 && !v.is_normal()) {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    Ok(v)
}

impl SMatrix {
    fn basis_rotation(cos: f32, sin: f32, a: usize, b: usize, c: usize, d: usize) -> Self {
        let mut m = Self::identity();
        m.elements[a] = cos;
        m.elements[b] = sin;
        m.elements[c] = -sin;
        m.elements[d] = cos;
        m
    }
    /// LS3DF 0x2cdc0 RotateXMatrix. cos/sin via try_rounded_sin_cos (x87
    /// enclosure on x86_64, admitted reduced-angle domain |angle|<=1 —
    /// outside refuses, disclosed). Zero angle is exact without the
    /// enclosure (sin keeps the input's own zero sign, cos is 1).
    pub fn rotation_x(angle: f32) -> Result<Self, MakeUnavailable> {
        let (s, c) = Self::axis_angle_sin_cos(angle)?;
        Ok(Self::basis_rotation(c, s, 5, 6, 9, 10))
    }
    /// LS3DF 0x2ce10 RotateYMatrix. Same domain policy as rotation_x.
    pub fn rotation_y(angle: f32) -> Result<Self, MakeUnavailable> {
        let (s, c) = Self::axis_angle_sin_cos(angle)?;
        let mut m = Self::identity();
        m.elements[0] = c;
        m.elements[2] = s;
        m.elements[8] = -s;
        m.elements[10] = c;
        Ok(m)
    }
    /// LS3DF 0x2ce60 RotateZMatrix. Same domain policy as rotation_x.
    pub fn rotation_z(angle: f32) -> Result<Self, MakeUnavailable> {
        let (s, c) = Self::axis_angle_sin_cos(angle)?;
        Ok(Self::basis_rotation(c, s, 0, 1, 4, 5))
    }
    fn axis_angle_sin_cos(angle: f32) -> Result<(f32, f32), MakeUnavailable> {
        if angle == 0.0 {
            return Ok((angle, 1.0));
        }
        try_rounded_sin_cos(angle)
    }
    /// LS3DF 0x2cf40 rotation-only product. Row3 is rotated (not translated);
    /// col3 forced [0,0,0,1]. Association per cell: (a0*b0 + a1*b4) + a2*b8
    /// via separate f32 mul/add (native FMUL/FADDP chain), each step rounded
    /// in f32 where native rounds once at the store (disclosed x87 gap,
    /// contract-capped).
    pub fn rotate_by_matrix(&self, mat: &Self) -> Result<Self, MakeUnavailable> {
        for v in self.elements.iter().chain(mat.elements.iter()) {
            if !is_verified_scalar(*v) {
                return Err(MakeUnavailable::InputOutsideVerifiedDomain);
            }
        }
        let (ae, be) = (&self.elements, &mat.elements);
        let mut o = Self::identity();
        for row in 0..4 {
            for col in 0..3 {
                o.elements[row * 4 + col] = add(
                    add(mul(ae[row * 4], be[col]), mul(ae[row * 4 + 1], be[4 + col])),
                    mul(ae[row * 4 + 2], be[8 + col]),
                );
            }
        }
        o.elements[3] = 0.0;
        o.elements[7] = 0.0;
        o.elements[11] = 0.0;
        o.elements[15] = 1.0;
        Ok(o)
    }
    /// LS3DF 0x2d520 full 4x4 product (homogeneous column included).
    /// Native cascade is (t0+t1)+(t2+t3) in 80-bit with a single final
    /// rounding (fld/fmul x4, faddp, fxch st(2), faddp, faddp, fstp); the
    /// port rounds each f32 step (disclosed x87 gap, contract-capped).
    pub fn mul_full(&self, m2: &Self) -> Result<Self, MakeUnavailable> {
        for v in self.elements.iter().chain(m2.elements.iter()) {
            if !is_verified_scalar(*v) {
                return Err(MakeUnavailable::InputOutsideVerifiedDomain);
            }
        }
        let mut o = Self::identity();
        for row in 0..4 {
            for col in 0..4 {
                let t0 = mul(self.elements[row * 4], m2.elements[col]);
                let t1 = mul(self.elements[row * 4 + 1], m2.elements[4 + col]);
                let t2 = mul(self.elements[row * 4 + 2], m2.elements[8 + col]);
                let t3 = mul(self.elements[row * 4 + 3], m2.elements[12 + col]);
                o.elements[row * 4 + col] = add(add(t0, t1), add(t2, t3));
            }
        }
        Ok(o)
    }
    /// LS3DF 0x2d780 mutating full 4x4 helper (Make4X4 working name).
    pub fn make_full(&mut self, m1: &Self, m2: &Self) -> Result<(), MakeUnavailable> {
        *self = m1.mul_full(m2)?;
        Ok(())
    }
    /// LS3DF 0x2e030 SetDir(dir,up): SetDir3 then col3/row3 fixup. Partial
    /// native writes on DegenerateUp are preserved (basis prefix stays),
    /// matching native order. SetDir3's own domain error maps here.
    pub fn set_dir(&mut self, dir: [f32; 3], up: [f32; 3]) -> Result<(), MakeUnavailable> {
        self.try_set_dir3(dir, up)
            .map_err(|_| MakeUnavailable::InputOutsideVerifiedDomain)?;
        for i in [3, 7, 11] {
            self.elements[i] = 0.0;
        }
        self.elements[12] = 0.0;
        self.elements[13] = 0.0;
        self.elements[14] = 0.0;
        self.elements[15] = 1.0;
        Ok(())
    }
    /// LS3DF 0x2d9d0 single-arg SetDir (helper, no function credit): the
    /// rotation turning the z axis into `dir`. Exact port of the three limbs
    /// (mirror of frame_mirror::set_dir_rows for the matrix type): Identity;
    /// row2 = SetNormalized(dir) with the |sq-1| fast path and the
    /// dominant-axis fallback; the sq = dir.z^2+dir.x^2 fast arms writing
    /// row0 lanes 0/2; row0 = SetNormalized(dir.z,0,-dir.x) when
    /// |dir.z|+|dir.x| > MRG_ZERO (1e-8); row1 = -(row0 x dir). Host f32 ops
    /// in source order, no fused ops. Err on refused input.
    pub fn set_dir_single(dir: [f32; 3]) -> Result<SMatrix, MakeUnavailable> {
        const MRG_ZERO: f32 = 1e-8;
        for v in dir {
            if !is_verified_scalar(v) {
                return Err(MakeUnavailable::InputOutsideVerifiedDomain);
            }
        }
        fn normalized(v: [f32; 3]) -> Option<[f32; 3]> {
            const MRG_ZERO: f32 = 1e-8;
            let sq = add(add(mul(v[0], v[0]), mul(v[1], v[1])), mul(v[2], v[2]));
            if !sq.is_finite() {
                return None;
            }
            if (sq - 1.0).abs() < MRG_ZERO {
                return Some(v);
            }
            if sq < MRG_ZERO {
                if v[0] != 0.0 {
                    return Some([if v[0] < 0.0 { -1.0 } else { 1.0 }, v[1], v[2]]);
                } else if v[2] != 0.0 {
                    return Some([v[0], v[1], if v[2] < 0.0 { -1.0 } else { 1.0 }]);
                } else {
                    return Some([v[0], if v[1] < 0.0 { -1.0 } else { 1.0 }, v[2]]);
                }
            }
            let inv = div(1.0, sqrt(sq));
            if !inv.is_finite() {
                return None;
            }
            Some([mul(v[0], inv), mul(v[1], inv), mul(v[2], inv)])
        }
        let row2 = normalized(dir).ok_or(MakeUnavailable::InputOutsideVerifiedDomain)?;
        let mut row0 = [1.0, 0.0, 0.0];
        let sq = add(mul(dir[2], dir[2]), mul(dir[0], dir[0]));
        if sq > MRG_ZERO {
            if (sq - 1.0).abs() < MRG_ZERO {
                row0[0] = dir[2];
                row0[2] = -dir[0];
            } else {
                let f = div(1.0, sqrt(sq));
                if !f.is_finite() {
                    return Err(MakeUnavailable::InputOutsideVerifiedDomain);
                }
                row0[0] = mul(dir[2], f);
                row0[2] = -mul(dir[0], f);
            }
        }
        if dir[2].abs() + dir[0].abs() > MRG_ZERO {
            row0 = normalized([dir[2], 0.0, -dir[0]])
                .ok_or(MakeUnavailable::InputOutsideVerifiedDomain)?;
        }
        let cross = [
            sub(mul(row0[1], dir[2]), mul(row0[2], dir[1])),
            sub(mul(row0[2], dir[0]), mul(row0[0], dir[2])),
            sub(mul(row0[0], dir[1]), mul(row0[1], dir[0])),
        ];
        let row1 = [-cross[0], -cross[1], -cross[2]];
        let mut m = SMatrix::identity();
        m.elements[0] = row0[0];
        m.elements[1] = row0[1];
        m.elements[2] = row0[2];
        m.elements[4] = row1[0];
        m.elements[5] = row1[1];
        m.elements[6] = row1[2];
        m.elements[8] = row2[0];
        m.elements[9] = row2[1];
        m.elements[10] = row2[2];
        Ok(m)
    }
    /// LS3DF 0x2e510 GetScale2: squares of row lengths, rows Z,Y,X computed
    /// then stored in XYZ order.
    pub fn try_get_scale2(&self) -> Result<[f32; 3], MakeUnavailable> {
        let mut o = [0.0; 3];
        for row in [2usize, 1, 0] {
            let v = [
                self.elements[row * 4],
                self.elements[row * 4 + 1],
                self.elements[row * 4 + 2],
            ];
            for x in v {
                if !is_verified_scalar(x) {
                    return Err(MakeUnavailable::InputOutsideVerifiedDomain);
                }
            }
            o[row] = add(add(mul(v[0], v[0]), mul(v[1], v[1])), mul(v[2], v[2]));
        }
        Ok(o)
    }
    /// LS3DF 0x2e580 GetUScale: mean of the three row lengths (native
    /// Magnitude each, then *(1/3)).
    pub fn try_get_uscale(&self) -> Result<f32, MakeUnavailable> {
        for v in self.elements {
            if !is_verified_scalar(v) {
                return Err(MakeUnavailable::InputOutsideVerifiedDomain);
            }
        }
        let lens = [
            vector_length([self.elements[0], self.elements[1], self.elements[2]]),
            vector_length([self.elements[4], self.elements[5], self.elements[6]]),
            vector_length([self.elements[8], self.elements[9], self.elements[10]]),
        ];
        Ok(mul(add(add(lens[0], lens[1]), lens[2]), 1.0 / 3.0))
    }
    /// LS3DF 0x2eab0 asm rotate then per-column renormalization
    /// (|col|² vs 1 gate, divide on miss).
    pub fn rotate_by_norm_matrix(
        &self,
        v: [f32; 3],
        mat: &Self,
    ) -> Result<[f32; 3], MakeUnavailable> {
        for x in v {
            if !is_verified_scalar(x) {
                return Err(MakeUnavailable::InputOutsideVerifiedDomain);
            }
        }
        let mut r = mat.rotate_rows_only(v)?;
        for (col, slot) in r.iter_mut().enumerate() {
            let c = [
                mat.elements[col],
                mat.elements[4 + col],
                mat.elements[8 + col],
            ];
            let sq = add(add(mul(c[0], c[0]), mul(c[1], c[1])), mul(c[2], c[2]));
            if (sq - 1.0).abs() > 1e-8 {
                *slot = div(*slot, sqrt(sq));
            }
        }
        Ok(r)
    }
    fn rotate_rows_only(&self, v: [f32; 3]) -> Result<[f32; 3], MakeUnavailable> {
        let mut o = [0.0; 3];
        for (col, slot) in o.iter_mut().enumerate() {
            *slot = add(
                add(
                    mul(v[0], self.elements[col]),
                    mul(v[1], self.elements[4 + col]),
                ),
                mul(v[2], self.elements[8 + col]),
            );
        }
        Ok(o)
    }
}

/// LS3DF 0x2ceb0 2D angle. Delegates to the 3D AngleTo lane; cutoff-route
/// parity with the native 2D single-sqrt path is unpinned (contract tier).
pub fn try_vector2_angle_to(a: [f32; 2], b: [f32; 2]) -> Result<f32, MakeUnavailable> {
    try_vector_angle_to([a[0], a[1], 0.0], [b[0], b[1], 0.0])
}

/// LS3DF 0x2e710 cos(half-angle): acos path, then sqrt((c+1)/2). The acos
/// itself is host libm on the guarded [-1,1] domain — disclosed
/// transcendental rounding (contract-capped, never bit-exactness).
pub fn try_vector_cos_half_angle(a: [f32; 3], b: [f32; 3]) -> Result<f32, MakeUnavailable> {
    for x in a.into_iter().chain(b) {
        if !is_verified_scalar(x) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
    }
    let dot = add(add(mul(a[0], b[0]), mul(a[1], b[1])), mul(a[2], b[2]));
    let sq_a = add(add(mul(a[0], a[0]), mul(a[1], a[1])), mul(a[2], a[2]));
    let sq_b = add(add(mul(b[0], b[0]), mul(b[1], b[1])), mul(b[2], b[2]));
    // Native `Dot/sqrt(Square*v.Square())`: one product, one root — kept
    // verbatim (two sqrts + multiply would differ in rounding and overflow
    // domain). Disclosed hardening: native yields NaN through 0/0 and
    // out-of-range c; the port refuses both (contract tier).
    let l = sqrt(mul(sq_a, sq_b));
    // Negated form keeps the x87 NaN flow (NaN fails `>`); partial_cmp would
    // need the same negation to stay faithful, so the direct form stays.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(l > 0.0) {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let c = div(dot, l);
    if !(-1.0..=1.0).contains(&c) {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    Ok(sqrt(div(add(c, 1.0), 2.0)))
}

impl SQuat {
    /// LS3DF 0x2ed60 third row of RotationMatrix; |s|>=1 -> (0,0,1).
    pub fn direction(self) -> Result<[f32; 3], MakeUnavailable> {
        let [w, x, y, z] = self.wxyz;
        for v in [w, x, y, z] {
            if !is_verified_scalar(v) {
                return Err(MakeUnavailable::InputOutsideVerifiedDomain);
            }
        }
        if w.abs() >= 1.0 {
            return Ok([0.0, 0.0, 1.0]);
        }
        let x2 = add(x, x);
        let y2 = add(y, y);
        Ok([
            sub(mul(x2, z), mul(y2, w)),
            add(mul(y2, z), mul(x2, w)),
            sub(1.0, add(mul(y2, y), mul(x2, x))),
        ])
    }
    /// LS3DF 0x2fb50 SetDir(dir) + roll-gated RotateZ premultiply +
    /// Make(matrix): single-arg SetDir first (NOT the two-arg Y-up SetDir3 —
    /// they differ, e.g. dir=[0,1,0] keeps X=[1,0,0], Y=[0,0,-1] here), then
    /// `m = RotateZ(roll) * m` iff |roll| > MRG_ZERO (affine operator*).
    pub fn try_make_dir_roll(dir: [f32; 3], roll: f32) -> Result<Self, MakeUnavailable> {
        let mut m = SMatrix::set_dir_single(dir)?;
        if roll.abs() > 1e-8 {
            let rz = SMatrix::rotation_z(roll)?;
            m = rz.mul_affine(&m);
        }
        Self::try_make_from_matrix(&m)
    }
    /// LS3DF 0x2fd70 axis-angle inverse. Normalizes axis first; `>=1→0`,
    /// `<=-1→2π`, else `2*acos`. acos is host libm on the guarded domain —
    /// disclosed (contract-capped).
    pub fn inverse_axis_angle(self) -> Result<([f32; 3], f32), MakeUnavailable> {
        let [s, x, y, z] = [self.wxyz[0], self.wxyz[1], self.wxyz[2], self.wxyz[3]];
        for v in [s, x, y, z] {
            if !v.is_finite() {
                return Err(MakeUnavailable::InputOutsideVerifiedDomain);
            }
        }
        let len_sq = add(add(mul(x, x), mul(y, y)), mul(z, z));
        let len = sqrt(len_sq);
        // Same negated-form requirement as try_vector_cos_half_angle above.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(len > 0.0) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
        let axis = [div(x, len), div(y, len), div(z, len)];
        let angle = if s >= 1.0 {
            0.0
        } else if s <= -1.0 {
            std::f32::consts::PI * 2.0
        } else {
            s.acos() * 2.0
        };
        Ok((axis, angle))
    }
    /// LS3DF 0x30040 Make(matrix) then self*q. Distinct from compose():
    /// validates m first (composition order identical).
    pub fn rotate_by_matrix(self, m: &SMatrix) -> Result<Self, MakeUnavailable> {
        let q = Self::try_make_from_matrix(m)?;
        Ok(self.compose(q))
    }
}

/// LS3DF 0x30290 Graphics-Gems plane-plane line. Returns (point, unit dir).
/// Branch order z-dominant, then y, then x; else Err (parallel).
pub fn try_plane_intersection_line(
    n1: [f32; 3],
    d1: f32,
    n2: [f32; 3],
    d2: f32,
) -> Result<([f32; 3], [f32; 3]), MakeUnavailable> {
    for v in n1.into_iter().chain(n2).chain([d1, d2]) {
        if !is_verified_scalar(v) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
    }
    let dir = [
        sub(mul(n1[1], n2[2]), mul(n1[2], n2[1])),
        sub(mul(n1[2], n2[0]), mul(n1[0], n2[2])),
        sub(mul(n1[0], n2[1]), mul(n1[1], n2[0])),
    ];
    let sq = [
        mul(dir[0], dir[0]),
        mul(dir[1], dir[1]),
        mul(dir[2], dir[2]),
    ];
    let mut p = [0.0; 3];
    let inv: f32;
    if sq[2] > sq[1] && sq[2] > sq[0] && sq[2] > 1e-8 {
        inv = div(1.0, dir[2]);
        p[0] = sub(mul(n1[1], d2), mul(n2[1], d1));
        p[1] = sub(mul(n2[0], d1), mul(n1[0], d2));
        p[2] = 0.0;
    } else if sq[1] > sq[0] && sq[1] > 1e-8 {
        inv = div(1.0, dir[1]);
        p[0] = sub(mul(n2[2], d1), mul(n1[2], d2));
        p[1] = 0.0;
        p[2] = sub(mul(n1[0], d2), mul(n2[0], d1));
    } else if sq[0] > 1e-8 {
        inv = div(1.0, dir[0]);
        p[0] = 0.0;
        p[1] = sub(mul(n1[2], d2), mul(n2[2], d1));
        p[2] = sub(mul(n2[1], d1), mul(n1[1], d2));
    } else {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    p[0] = mul(p[0], inv);
    p[1] = mul(p[1], inv);
    p[2] = mul(p[2], inv);
    let l = sqrt(add(
        add(mul(dir[0], dir[0]), mul(dir[1], dir[1])),
        mul(dir[2], dir[2]),
    ));
    Ok((p, [div(dir[0], l), div(dir[1], l), div(dir[2], l)]))
}

/// LS3DF 0x30440 closest parameters of p1+u1*d1, p2+u2*d2. denom guard first.
pub fn try_line_line_intersect(
    p1: [f32; 3],
    d1: [f32; 3],
    p2: [f32; 3],
    d2: [f32; 3],
) -> Result<(f32, f32), MakeUnavailable> {
    let dot =
        |a: [f32; 3], b: [f32; 3]| add(add(mul(a[0], b[0]), mul(a[1], b[1])), mul(a[2], b[2]));
    for v in p1.into_iter().chain(d1).chain(p2).chain(d2) {
        if !is_verified_scalar(v) {
            return Err(MakeUnavailable::InputOutsideVerifiedDomain);
        }
    }
    let sub_v = [sub(p1[0], p2[0]), sub(p1[1], p2[1]), sub(p1[2], p2[2])];
    let d22 = dot(d2, d2);
    let d12 = dot(d1, d2);
    let denom = sub(mul(dot(d1, d1), d22), mul(d12, d12));
    if denom.abs() < 1e-8 {
        return Err(MakeUnavailable::InputOutsideVerifiedDomain);
    }
    let d23 = dot(sub_v, d2);
    let d13 = dot(sub_v, d1);
    let u1 = div(sub(mul(d12, d23), mul(d13, d22)), denom);
    Ok((u1, div(add(mul(u1, d12), d23), d22)))
}

/// LS3DF RVA 0x56e60, VA 0x10056e60. `S_vector::operator*=(const float &)`
/// out-of-line copy working name (mafia-re ab67b13
/// `ls3df/I3D/I3d_math.h:223-231`; the copy the linker kept lives in the
/// single-mesh file for `I3D_joint::GetJointMatrix`; `S_vector` itself is
/// PS2-evidenced but this operator spelling is a mafia-re recovery, NOT a PDB
/// original symbol). Exact f32 lane multiply-assign (`x *= f; y *= f; z *=
/// f`), no domain guard: NaN, infinities and subnormals flow through exactly
/// as the native x87 `FMUL` stores do. The host takes the factor by value;
/// the original takes `const float &`. Tier differential (REA formed body 1BB/25B/8insn + executed pinning tests).
pub fn scale_vector_assign(vector: &mut [f32; 3], factor: f32) {
    vector[0] *= factor;
    vector[1] *= factor;
    vector[2] *= factor;
}

/// LS3DF RVA 0xdd60, VA 0x1000dd60. `S_vector(float, float, float)` ctor
/// (mafia-re ab67b13 `ls3df/I3D/I3d_math.h:64`; the out-of-line copy the
/// linker kept lives in camera code; `S_vector` itself is PS2-evidenced).
/// Pure lane copy, no arithmetic. Tier differential (REA formed body 1BB/25B/8insn + executed pinning tests).
pub fn vector_from_components(x: f32, y: f32, z: f32) -> [f32; 3] {
    [x, y, z]
}

/// LS3DF RVA 0xdd80, VA 0x1000dd80. `S_vector::operator-` (mafia-re
/// ab67b13 `ls3df/I3D/I3d_math.h:201`; out-of-line copy in camera code).
/// Exact f32 lane subtraction, no domain guard: NaN, infinities and
/// subnormals flow through as the native x87 `FSUB` stores do. Tier
/// differential (REA formed body 1BB/35B/12insn + executed pinning tests).
pub fn vector_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// LS3DF RVA 0xddb0, VA 0x1000ddb0. `S_vector::Dot` (mafia-re ab67b13
/// `ls3df/I3D/I3d_math.h:69`; PS2 `Dot__8S_vectorCFRC8S_vector`; the
/// out-of-line copy the linker kept lives in camera code). Exact f32
/// `(x*v.x + y*v.y) + z*v.z` left-associated multiplies then adds, no
/// domain guard and no fused operations. Extended-exponent x87
/// intermediates, NaN ordering and non-default control words are not
/// established as equivalent to portable Rust arithmetic. Tier differential
/// (REA formed body 1BB/27B/10insn + executed pinning tests).
pub fn vector_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

/// LS3DF RVA 0xddd0, VA 0x1000ddd0. `S_vector::GetNormal` (mafia-re
/// ab67b13 `ls3df/I3D/I3d_math.h:276`; PS2
/// `GetNormal__8S_vectorFRC8S_vectorRC8S_vectorRC8S_vector`). Unnormalized
/// triangle normal `(w1 - v1) x (u1 - v1)` via two `vector_sub` differences
/// then the component cross, all exact f32 with the native association and
/// no domain guard. Tier differential (REA formed body 1BB/113B/41insn +
/// executed pinning tests).
pub fn vector_get_normal(u1: [f32; 3], v1: [f32; 3], w1: [f32; 3]) -> [f32; 3] {
    let u = vector_sub(w1, v1);
    let v = vector_sub(u1, v1);
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

/// LS3DF RVA 0x112a0, VA 0x100112a0. `S_vector::operator*(const float &)`
/// (mafia-re ab67b13 `ls3df/I3D/I3d_math.h:176`; PS2
/// `__ml__8S_vectorCFRCf`; the factor is by reference, so the native reads
/// it again for each lane; the host takes it by value, which is
/// indistinguishable without aliasing mutation). Exact f32 lane multiply,
/// no domain guard. Tier differential (REA formed body 1BB/33B/12insn +
/// executed pinning tests).
pub fn vector_scale(vector: [f32; 3], factor: f32) -> [f32; 3] {
    [vector[0] * factor, vector[1] * factor, vector[2] * factor]
}

/// LS3DF RVA 0x112d0, VA 0x100112d0. `S_vector::operator+` (mafia-re
/// ab67b13 `ls3df/I3D/I3d_math.h:184`; PS2 `__pl__8S_vectorCFRC8S_vector`).
/// Exact f32 lane addition, no domain guard. Tier differential (REA formed
/// body 1BB/35B/12insn + executed pinning tests).
pub fn vector_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// LS3DF RVA 0x12500, VA 0x10012500. `S_vector::Square` (mafia-re ab67b13
/// `ls3df/I3D/I3d_math.h:123`; out-of-line copy in collision code). Exact
/// f32 `(x*x + y*y) + z*z` written out (not via `Dot`, matching the
/// native inline-budget spelling), no domain guard. Tier differential (REA
/// formed body 1BB/31B/15insn + executed pinning tests).
pub fn vector_square(vector: [f32; 3]) -> f32 {
    (vector[0] * vector[0] + vector[1] * vector[1]) + vector[2] * vector[2]
}

/// LS3DF RVA 0x14970, VA 0x10014970. `S_vector::SetNormalized` (mafia-re
/// ab67b13 `ls3df/I3D/I3d_math.h:80`; working name, mafia-re guess, NOT an
/// original symbol; out-of-line copy in collision code). Copies `v`, then
/// the `|sq - 1| < MRG_ZERO` (1e-8) fast path, the `sq < MRG_ZERO`
/// axis-fallback (`x`, then `z`, then `y`, preserving the other lanes),
/// else the `1/sqrt(sq)` scale. Direct `<` keeps the native NaN flow
/// (NaN takes the scale path). The square sum is written out, not via
/// `Dot`. Host `sqrt` is the f64-mediated `sqrt` narrowed to f32 (matching
/// the native `(float)sqrt` promotion) followed by an f32 divide; x87
/// `FSQRT` bit parity and extended-exponent intermediates are not
/// established (disclosed). Tier differential (REA formed body 15BB/302B/96insn
/// + executed pinning tests).
pub fn vector_set_normalized(v: [f32; 3]) -> [f32; 3] {
    // Local copy of MRG_ZERO (ls3df.h:48, 1e-8f) to avoid cross-module
    // coupling with render_clip; same value, same branches.
    const MRG_ZERO: f32 = 1e-8;
    let sq = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if (sq - 1.0).abs() < MRG_ZERO {
        return v;
    }
    if sq < MRG_ZERO {
        let mut out = v;
        if v[0] != 0.0 {
            out[0] = if v[0] < 0.0 { -1.0 } else { 1.0 };
        } else if v[2] != 0.0 {
            out[2] = if v[2] < 0.0 { -1.0 } else { 1.0 };
        } else {
            out[1] = if v[1] < 0.0 { -1.0 } else { 1.0 };
        }
        return out;
    }
    let root = f64::from(sq).sqrt() as f32;
    let reciprocal = 1.0 / root;
    [v[0] * reciprocal, v[1] * reciprocal, v[2] * reciprocal]
}

/// LS3DF RVA 0x268b0, VA 0x100268b0. `I3DFloatAsInt` out-of-line copy
/// (mafia-re `ls3df/I3D/I3D.h:243-249` via
/// `ls3df/I3D/visuals/I3d_lit_object.cpp:10`; working name (Insanity3D's
/// spelling for the float-bits helper, NOT an evidenced original symbol);
/// the copy the linker kept lives in the lit-object
/// file for `Construct`). Copies the float to a local then reinterprets
/// the bits as `int`, with no arithmetic. Tier differential (REA formed body
/// 1BB/9B/5insn + executed pinning tests).
pub fn float_bits_copy(value: f32) -> i32 {
    let local = value;
    local.to_bits() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mise01_near_rotation_boundary_admits_under_owner() {
        // Genuine MISE01 scene matrix (animated joint with f32 composition
        // noise). Portable factor enclosures require an owner at this rounding
        // boundary; the retained x87 factors are also available ownerless.
        // Check each backend's admission and scoped precision state without
        // assigning original-runtime parity to a captured quaternion value.
        use crate::native_precision::{current_precision, Pc24PrecisionState};

        let matrix = SMatrix {
            elements: [
                0.99999976,
                6.424726e-11,
                -0.0006357619,
                0.0,
                4.210135e-5,
                0.9978049,
                0.06622197,
                0.0,
                0.0006343665,
                -0.06622198,
                0.99780476,
                0.0,
                -2.5054965e-9,
                0.10806094,
                0.01910636,
                1.0,
            ],
        };
        assert_eq!(current_precision(), None);
        let ownerless = SQuat::try_make_from_matrix(&matrix);
        assert_eq!(current_precision(), None);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        let ownerless = ownerless.expect("retained x87 factors admit this matrix without an owner");
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        assert_eq!(ownerless, Err(MakeUnavailable::RoundingUncertain));

        for initial_precision in [false, true] {
            let mut owner = Pc24PrecisionState::new(initial_precision);
            let quaternion = owner
                .run(|| {
                    assert_eq!(current_precision(), Some(initial_precision));
                    let result = SQuat::try_make_from_matrix(&matrix);
                    assert_eq!(current_precision(), Some(true));
                    result
                })
                .expect("boundary straddle admits under owner");
            assert!(owner.precision_raised());
            assert_eq!(current_precision(), None);
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            assert_eq!(
                quaternion.wxyz.map(f32::to_bits),
                ownerless.wxyz.map(f32::to_bits)
            );
            let norm = quaternion
                .wxyz
                .iter()
                .map(|value| f64::from(*value) * f64::from(*value))
                .sum::<f64>()
                .sqrt();
            assert!((norm - 1.).abs() < 1e-6, "norm={norm}");
            assert!(quaternion
                .wxyz
                .into_iter()
                .all(|value| value == 0. || value.is_normal()));
        }
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn frame_roll_keeps_raw_angles_through_the_x87_argument_boundary() {
        for angle in [8.000001, -13.9, 1000., -65536., f32::from_bits(0x5f7f_ffff)] {
            let value = SQuat::try_make_axis_angle_reduced([0., 0., 1.], angle).unwrap();
            assert!(value.wxyz.into_iter().all(f32::is_finite));
            let frame = SQuat::try_make_frame_direction([0., 0., 1.], angle).unwrap();
            assert_eq!(frame.wxyz, value.wxyz);
        }
        assert_eq!(
            SQuat::try_make_axis_angle([0., 0., 1.], 10.),
            Err(MakeUnavailable::UnrecoveredAxisAngle)
        );
        for angle in [f32::from_bits(0x5f80_0000), -f32::from_bits(0x5f80_0000)] {
            assert_eq!(
                SQuat::try_make_axis_angle_reduced([0., 0., 1.], angle),
                Err(MakeUnavailable::InputOutsideVerifiedDomain)
            );
        }
    }

    #[test]
    fn tiny_axis_angles_keep_the_retained_sine_products_nonzero() {
        // Synthetic dyadic values avoid borrowing any authored game fixture.
        // All squared components remain in the normal binary32 domain.
        for exponent in [-32, -44, -55, -59] {
            for sign in [1., -1.] {
                let angle = sign * 2f32.powi(exponent);
                let expected = [1., angle * 0.125, angle * 0., -angle * 0.5];
                for reduced in [false, true] {
                    let mut precision = crate::native_precision::Pc24PrecisionState::default();
                    let result = precision
                        .run(|| {
                            if reduced {
                                SQuat::try_make_axis_angle_reduced([0.25, 0., -1.], angle)
                            } else {
                                SQuat::try_make_axis_angle([0.25, 0., -1.], angle)
                            }
                        })
                        .unwrap();
                    assert_eq!(result.wxyz.map(f32::to_bits), expected.map(f32::to_bits));
                    assert_ne!(result.wxyz[1], 0.);
                    assert_ne!(result.wxyz[3], 0.);
                    assert!(precision.precision_raised());
                }
            }
        }
    }

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
    fn missing_matrix_rows_follow_source_cross_order_for_each_axis() {
        // A rotated basis with exactly representable row lengths. These
        // controls spell out the source cross products independently.
        let basis = [[0., -1., 0.], [1., 0., 0.], [0., 0., 1.]];
        let expected_rows = [[0., -1., 0.], [-1., 0., 0.], [0., 0., 1.]];
        for (row, expected_row) in expected_rows.into_iter().enumerate() {
            let mut matrix = SMatrix::identity();
            for (i, values) in basis.into_iter().enumerate() {
                matrix.elements[i * 4..i * 4 + 3].copy_from_slice(&values);
            }
            matrix.elements[12..15].copy_from_slice(&[10., 20., 30.]);
            matrix.elements[row * 4..row * 4 + 3].fill(0.);
            let mut reconstructed = matrix;
            reconstructed.elements[row * 4..row * 4 + 3].copy_from_slice(&expected_row);
            let original = matrix;
            let got = SQuat::try_make_from_matrix(&matrix).unwrap();
            let expected = SQuat::try_make_from_matrix(&reconstructed).unwrap();
            assert_eq!(
                got.wxyz.map(f32::to_bits),
                expected.wxyz.map(f32::to_bits),
                "axis {row}"
            );
            assert_eq!(matrix, original); // reconstruction copies; it does not repair the caller's matrix
        }
    }

    #[test]
    fn multiple_missing_rows_read_original_rows_and_use_unit_fallbacks() {
        let expected = [1f32.to_bits(), 0, 0, 0];
        let mut matrix = SMatrix::identity();
        matrix.elements[..3].fill(0.);
        matrix.elements[4..7].fill(0.);
        matrix.elements[8..11].copy_from_slice(&[1., 1., 1.]);
        // X and Y both cross original zero rows and fall back to their unit
        // axes. Reading rebuilt X while reconstructing Y would change this.
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
        matrix.elements[4..7].copy_from_slice(&[0., 1., 0.]);
        matrix.elements[8..11].copy_from_slice(&[0., 2., 0.]);
        // Two present but parallel rows also select the X fallback.
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
    }

    #[test]
    fn small_scale_product_rechecks_scale_and_preserves_numeric_admission() {
        let expected = [1f32.to_bits(), 0, 0, 0];
        let mut matrix = SMatrix::identity();
        matrix.elements[0] = 0.001;
        matrix.elements[5] = 0.;
        matrix.elements[6] = -0.001;
        matrix.elements[9] = 0.001;
        matrix.elements[10] = 0.;
        // Every row length exceeds MRG_ZERO, so none is replaced, but their
        // product stays below it: source's second scale test returns identity.
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
        matrix = SMatrix::identity();
        matrix.elements[0] = 0.00001; // Magnitude's squared-length cutoff gives zero
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
        matrix.elements[..3].fill(0.);
        matrix.elements[4..7].copy_from_slice(&[1e10, 0., 0.]);
        matrix.elements[8..11].copy_from_slice(&[0., 1e10, 0.]);
        // Reconstructed row's square overflows the admitted f32 exponent
        // domain. Do not turn that unavailable arithmetic into a unit fallback.
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix),
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        );
    }

    #[test]
    fn general_make_fails_closed_for_unrecovered_or_uncertain_numeric_paths() {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        assert_eq!(
            SQuat::try_make_axis_angle([1., 0., 0.], 1e-20)
                .unwrap()
                .wxyz,
            [1., 5e-21, 0., 0.]
        );
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        assert_eq!(
            SQuat::try_make_axis_angle([1., 0., 0.], 1e-20),
            // The portable profile keeps its explicit binary32 exponent edge.
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        );
        assert_eq!(
            SQuat::try_make_axis_angle([1., 0., 0.], f32::from_bits(1)),
            Err(MakeUnavailable::InputOutsideVerifiedDomain)
        );
        let mut matrix = SMatrix::identity();
        matrix.elements[5] = -1.;
        matrix.elements[6] = -1.;
        // Original PC24 Make executes its factor beyond the old two-radian
        // host boundary. Keep the independently captured output as regression.
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            [1053028119, 1064076128, 0, 0]
        );
        matrix = SMatrix::identity();
        matrix.elements[0] = 0.;
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix).unwrap().wxyz,
            [1., 0., 0., 0.]
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
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            [3007036718, 0, 1065353216, 0]
        );
        matrix.elements = [1e-5; 16];
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix)
                .unwrap()
                .wxyz
                .map(f32::to_bits),
            expected
        );
        matrix.elements[0] = 0.001;
        // Both short rows use their axis fallbacks; the surviving X row and
        // all nonbasis elements are retained for the recursive Make call.
        let mut rebuilt = matrix;
        rebuilt.elements[4..7].copy_from_slice(&[0., 1., 0.]);
        rebuilt.elements[8..11].copy_from_slice(&[0., 0., 1.]);
        assert_eq!(
            SQuat::try_make_from_matrix(&matrix),
            SQuat::try_make_from_matrix(&rebuilt)
        );
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
    fn checked_normalize_admits_tiny_axes_and_preserves_native_branches() {
        // Constructed [-3/5, 2^-50, 4/5, -2^-52], with independent tiny axes.
        let components = [
            -0.6,
            f32::from_bits(0x26800000),
            0.8,
            -f32::from_bits(0x25800000),
        ];
        let bits = components.map(f32::to_bits);
        let mut rotation = SQuat { wxyz: components };
        assert!(!rotation.is_verified_input());
        rotation.try_normalize().unwrap();
        assert_eq!(rotation.wxyz.map(f32::to_bits), bits);
        let mut scaled = SQuat {
            wxyz: [2., 1e-16, 0., -0.],
        };
        scaled.try_normalize().unwrap();
        assert_eq!(
            scaled.wxyz.map(f32::to_bits),
            [1., 5e-17, 0., -0.].map(f32::to_bits)
        );
        let mut tiny = SQuat {
            wxyz: [1e-16, -1e-16, 2e-16, 3e-16],
        };
        tiny.try_normalize().unwrap();
        assert_eq!(tiny.wxyz, [0., -1., 2e-16, 3e-16]);
        let mut zero = SQuat { wxyz: [-0.; 4] };
        zero.try_normalize().unwrap();
        assert_eq!(
            zero.wxyz.map(f32::to_bits),
            [0, 0x80000000, 0x3f800000, 0x80000000]
        );
    }

    #[test]
    fn checked_normalize_retains_unavailable_input_without_partial_output() {
        for value in [1e-20, f32::MIN_POSITIVE, f32::MAX, f32::INFINITY, f32::NAN] {
            let mut rotation = SQuat {
                wxyz: [1., value, 0., 0.],
            };
            let bits = rotation.wxyz.map(f32::to_bits);
            assert!(rotation.try_normalize().is_err());
            assert_eq!(rotation.wxyz.map(f32::to_bits), bits);
        }
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

    #[test]
    fn pin_rand_float_maps_crt_range() {
        // RVA 0x2cda0: sample * (1.0f/32767.0f) in f32, bit-pinned:
        // step = 0x38000100; 16383*step = 0x3EFFFE00 (0.49998474);
        // 32767*step is exactly 1.0. Out-of-range refused.
        assert_eq!(rand_float_sample(0).unwrap().to_bits(), 0x0);
        assert_eq!(rand_float_sample(1).unwrap().to_bits(), 0x3800_0100);
        assert_eq!(rand_float_sample(16383).unwrap().to_bits(), 0x3EFF_FE00);
        assert_eq!(rand_float_sample(32767).unwrap().to_bits(), 0x3F80_0000);
        assert!(rand_float_sample(0x8000).is_err());
    }
    #[test]
    fn pin_rotate_xyz_identity_and_nan_guard() {
        // RVAs 0x2cdc0/0x2ce10/0x2ce60: basis placement per axis.
        let x = SMatrix::rotation_x(0.0).unwrap();
        assert_eq!(
            [x.elements[5], x.elements[6], x.elements[9], x.elements[10]],
            [1.0, 0.0, 0.0, 1.0]
        );
        let y = SMatrix::rotation_y(0.0).unwrap();
        assert_eq!(
            [y.elements[0], y.elements[2], y.elements[8], y.elements[10]],
            [1.0, 0.0, 0.0, 1.0]
        );
        let z = SMatrix::rotation_z(0.0).unwrap();
        assert_eq!(
            [z.elements[0], z.elements[1], z.elements[4], z.elements[5]],
            [1.0, 0.0, 0.0, 1.0]
        );
        assert!(SMatrix::rotation_x(f32::NAN).is_err());
    }
    #[test]
    fn pin_full_products_match_affine_on_affine_inputs() {
        // RVAs 0x2d520/0x2d780: full product agrees with the affine fast
        // path on affine inputs; mutating helper matches.
        let a = SMatrix::rotation_z(0.5).unwrap();
        let b = SMatrix::rotation_x(0.25).unwrap();
        assert_eq!(
            a.mul_full(&b).unwrap().elements.map(f32::to_bits),
            a.mul_affine(&b).elements.map(f32::to_bits)
        );
        let mut m = SMatrix::identity();
        m.make_full(&a, &b).unwrap();
        assert_eq!(m.elements, a.mul_full(&b).unwrap().elements);
    }
    #[test]
    fn pin_rotate_by_matrix_col3_and_dir_fixup() {
        // RVAs 0x2cf40/0x2e030: col3 forced, SetDir tail words.
        let a = SMatrix::rotation_z(0.5).unwrap();
        let r = a.rotate_by_matrix(&SMatrix::identity()).unwrap();
        assert_eq!(&r.elements[12..16], &[0.0, 0.0, 0.0, 1.0]);
        let mut m = SMatrix::identity();
        m.set_dir([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]).unwrap();
        assert_eq!(&m.elements[12..16], &[0.0, 0.0, 0.0, 1.0]);
    }
    #[test]
    fn pin_scale_family_order_and_zero() {
        // RVAs 0x2e510/0x2e580/0x2e710: rows Z,Y,X then XYZ store; mean of
        // lengths; cos-half endpoints.
        let mut m = SMatrix::identity();
        m.elements[0..3].copy_from_slice(&[3.0, 4.0, 0.0]);
        assert_eq!(m.try_get_scale2().unwrap(), [25.0, 1.0, 1.0]);
        assert!((m.try_get_uscale().unwrap() - 7.0 / 3.0).abs() < 1e-6);
        assert_eq!(
            try_vector_cos_half_angle([1.0, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap(),
            1.0
        );
        assert_eq!(
            try_vector_cos_half_angle([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]).unwrap(),
            0.0
        );
        assert_eq!(
            try_vector2_angle_to([1.0, 0.0], [0.0, 1.0]).unwrap(),
            try_vector2_angle_to([1.0, 0.0], [1.0, 0.0]).unwrap() + std::f32::consts::FRAC_PI_2
        );
    }
    #[test]
    fn pin_vector2_angle_to_absolute_zero_half_pi_and_pi() {
        // LS3DF RVA 0x2ceb0 S_vector2::AngleTo (unsigned acos, MRG_ZERO->0),
        // NOT Game AngleTo2 (mafia-re C_actor.cpp:1748-1760, signed atan2
        // difference wrapped to [-pi,pi], ported separately as
        // car_resource_math::animation_angle for Game 0x408490).
        assert_eq!(try_vector2_angle_to([1.0, 0.0], [1.0, 0.0]).unwrap(), 0.0);
        assert_eq!(
            try_vector2_angle_to([1.0, 0.0], [0.0, 1.0]).unwrap(),
            std::f32::consts::FRAC_PI_2
        );
        assert_eq!(
            try_vector2_angle_to([1.0, 0.0], [-1.0, 0.0]).unwrap(),
            std::f32::consts::PI
        );
        assert_eq!(try_vector2_angle_to([0.0, 0.0], [1.0, 0.0]).unwrap(), 0.0);
        // Unsigned: the signed Game AngleTo2 would return -FRAC_PI_2 here.
        assert_eq!(
            try_vector2_angle_to([1.0, 0.0], [0.0, -1.0]).unwrap(),
            std::f32::consts::FRAC_PI_2
        );
    }
    #[test]
    fn pin_rotate_by_norm_matrix_renormalizes_on_miss() {
        // LS3DF RVA 0x2eab0: asm rotate then per-column renormalization
        // (|col|^2 vs 1 gate, divide on miss). Ghidra body 122 insns, 10 bbs.
        let m = SMatrix::identity();
        assert_eq!(m.rotate_by_norm_matrix([1.0, 2.0, 3.0], &m).unwrap(), [1.0, 2.0, 3.0]);
        let mut scaled = SMatrix::identity();
        scaled.elements[0] = 2.0;
        assert_eq!(scaled.rotate_by_norm_matrix([1.0, 0.0, 0.0], &scaled).unwrap(), [1.0, 0.0, 0.0]);
    }
    #[test]
    fn pin_quat_dir_roll_inverse() {
        // RVAs 0x2ed60/0x2fb50/0x2fd70/0x30040.
        assert_eq!(
            SQuat {
                wxyz: [1.0, 0.0, 0.0, 0.0]
            }
            .direction()
            .unwrap(),
            [0.0, 0.0, 1.0]
        );
        let q = SQuat::try_make_dir_roll([0.0, 0.0, 1.0], 0.0).unwrap();
        assert!((q.wxyz[0] - 1.0).abs() < 1e-6);
        // RVA 0x2d9d0 single-arg basis: dir=[0,1,0] keeps X=[1,0,0] and
        // takes Y=[0,0,-1] (the two-arg Y-up SetDir3 gives Y=[0,0,1] instead).
        let s = SMatrix::set_dir_single([0.0, 1.0, 0.0]).unwrap();
        assert_eq!(
            [
                [s.elements[0], s.elements[1], s.elements[2]],
                [s.elements[4], s.elements[5], s.elements[6]],
                [s.elements[8], s.elements[9], s.elements[10]]
            ],
            [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]]
        );
        let (axis, ang) = SQuat {
            wxyz: [1.0, 0.5, 0.0, 0.0],
        }
        .inverse_axis_angle()
        .unwrap();
        assert_eq!(ang, 0.0);
        assert_eq!(axis, [1.0, 0.0, 0.0]);
        let m = SMatrix::identity();
        assert_eq!(
            SQuat {
                wxyz: [1.0, 0.0, 0.0, 0.0]
            }
            .rotate_by_matrix(&m)
            .unwrap()
            .wxyz,
            [1.0, 0.0, 0.0, 0.0]
        );
    }
    #[test]
    fn pin_plane_and_line_exact_branches() {
        // RVAs 0x30290/0x30440: z-dominant first; denom guard.
        let (p, d) =
            try_plane_intersection_line([0.0, 0.0, 1.0], 0.0, [1.0, 0.0, 0.0], 0.0).unwrap();
        assert_eq!(p, [0.0, 0.0, 0.0]);
        assert_eq!(d, [0.0, 1.0, 0.0]);
        assert!(try_plane_intersection_line([0.0, 0.0, 1.0], 0.0, [0.0, 0.0, 1.0], 0.0).is_err());
        let (u1, u2) = try_line_line_intersect(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        )
        .unwrap();
        assert_eq!((u1, u2), (0.0, 0.0));
        assert!(try_line_line_intersect(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0]
        )
        .is_err());
    }

    #[test]
    fn scale_vector_assign_multiplies_each_lane_exactly() {
        // LS3DF RVA 0x56e60 out-of-line copy: hand-derived dyadic vectors,
        // no game fixtures. Exact f32 stores, bit-pinned.
        let mut vector = [1.5, -2.0, 0.0];
        scale_vector_assign(&mut vector, 2.0);
        assert_eq!(
            vector.map(f32::to_bits),
            [3.0f32.to_bits(), (-4.0f32).to_bits(), 0.0f32.to_bits()]
        );
        // Signed-zero factor keeps lane signs, as IEEE multiply does.
        let mut vector = [-1.0, 1.0, 3.25];
        scale_vector_assign(&mut vector, 0.0);
        assert_eq!(
            vector.map(f32::to_bits),
            [(-0.0f32).to_bits(), 0.0f32.to_bits(), 0.0f32.to_bits()]
        );
        // Non-finite lanes flow through without a domain guard.
        let mut vector = [f32::INFINITY, f32::NAN, 1.0];
        scale_vector_assign(&mut vector, 2.0);
        assert_eq!(vector[0], f32::INFINITY);
        assert!(vector[1].is_nan());
        assert_eq!(vector[2].to_bits(), 2.0f32.to_bits());
    }

    #[test]
    fn early_vector_ctor_sub_add_scale_match_exact_lanes() {
        assert_eq!(
            vector_from_components(1.5, -2.0, 0.25).map(f32::to_bits),
            [1.5f32.to_bits(), (-2.0f32).to_bits(), 0.25f32.to_bits()]
        );
        assert_eq!(
            vector_sub([1.5, -2.0, 4.0], [0.5, 1.0, -1.0]),
            [1.0, -3.0, 5.0]
        );
        assert_eq!(
            vector_add([1.5, -2.0, 0.25], [0.5, 3.0, 0.75]),
            [2.0, 1.0, 1.0]
        );
        assert_eq!(
            vector_scale([1.5, -2.0, 0.5], 2.0).map(f32::to_bits),
            [3.0f32.to_bits(), (-4.0f32).to_bits(), 1.0f32.to_bits()]
        );
        assert_eq!(vector_square([1.0, 2.0, 2.0]), 9.0);
        assert_eq!(vector_dot([1.0, 2.0, 3.0], [4.0, -1.0, 0.5]), 3.5);
        assert_eq!(
            vector_get_normal([0.0, 1.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            [-1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn early_vector_set_normalized_keeps_native_branches() {
        assert_eq!(vector_set_normalized([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
        assert_eq!(vector_set_normalized([2.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
        assert_eq!(vector_set_normalized([0.0, -5.0, 0.0]), [0.0, -1.0, 0.0]);
        assert_eq!(vector_set_normalized([0.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
        assert_eq!(vector_set_normalized([0.0, 0.0, -2.0]), [0.0, 0.0, -1.0]);
        assert_eq!(float_bits_copy(1.0), 0x3f80_0000);
        assert_eq!(float_bits_copy(-0.0), i32::MIN);
    }
}
