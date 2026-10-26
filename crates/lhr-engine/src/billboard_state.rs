//! Bounded Billboard orientation and ProductBillMatrix arithmetic.
//! PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Static entry RVAs 0x5db0, 0x94d0, 0x6090; REA report SHA-256
//! ba8fa043c9444a1dd259c36cd66d405d2ed0d28e6a0e5dcc31e5f1ae2ada83a4.
//! Matrix operands are borrowed from the live Frame/Camera at render preparation.
//! This pure operation creates no Frame, resource, cache, or renderer owner.
use crate::native_matrix::{
    checked_f32_add as add, checked_f32_div as div, checked_f32_mul as mul, checked_f32_sub as sub,
    MatrixArithmeticUnavailable,
};
use crate::native_transform::SMatrix;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BillboardOrientation {
    pub rot_axis: u32,
    pub axis_mode: u8,
}
impl Default for BillboardOrientation {
    fn default() -> Self {
        Self {
            rot_axis: 2,
            axis_mode: 0,
        }
    }
}
impl BillboardOrientation {
    /// Native LoadImage reads a DWORD and the low byte of constructor-zero mode.
    pub fn from_source(rot_axis_word: u32, axis_mode_byte: u8) -> Self {
        let mut result = Self {
            rot_axis: rot_axis_word,
            axis_mode: axis_mode_byte,
        };
        if result.rot_axis > 2 {
            result.rot_axis = 0;
            result.axis_mode = 2;
        }
        if result.axis_mode > 3 {
            result.axis_mode = 1;
        }
        result
    }
}

/// Borrow only the live getters that the selected native mode actually uses.
/// The caller performs lazy Frame/Camera getters in native order before making
/// one variant; this pure operation never requests an unused inverse matrix.
pub enum BillboardMatrixInput<'a> {
    Facing {
        camera_world_position: [f32; 3],
        inverse_matrix: &'a SMatrix,
        render_matrix: &'a SMatrix,
    },
    CameraAligned {
        camera_view_inverse: &'a SMatrix,
        world_scale: [f32; 3],
        world_matrix: &'a SMatrix,
    },
    LocalAligned {
        local_matrix: &'a SMatrix,
        camera_view_inverse: &'a SMatrix,
        world_matrix: &'a SMatrix,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BillboardNumericUnavailable {
    Arithmetic,
    InvalidOrientation,
    InputModeMismatch,
    NonAffineMatrix,
}
impl From<MatrixArithmeticUnavailable> for BillboardNumericUnavailable {
    fn from(_: MatrixArithmeticUnavailable) -> Self {
        Self::Arithmetic
    }
}
type NumericResult<T> = Result<T, BillboardNumericUnavailable>;

fn sqrt(value: f32) -> NumericResult<f32> {
    if value < 0. || !value.is_finite() {
        return Err(BillboardNumericUnavailable::Arithmetic);
    }
    let result = value.sqrt();
    if result != 0. && !result.is_normal() {
        return Err(BillboardNumericUnavailable::Arithmetic);
    }
    crate::native_precision::record_sqrt(value, result);
    Ok(result)
}
fn sign_unit(value: f32) -> f32 {
    if value < 0. {
        -1.
    } else {
        1.
    }
}
fn unit_horizontal(x: f32, z: f32) -> NumericResult<[f32; 3]> {
    let squared = add(mul(z, z)?, mul(x, x)?)?;
    if sub(squared, 1.)?.abs() < 1e-8 {
        return Ok([x, 0., z]);
    }
    if squared >= 1e-8 {
        let inverse_length = div(1., sqrt(squared)?)?;
        return Ok([mul(x, inverse_length)?, 0., mul(z, inverse_length)?]);
    }
    // PC mode-0 and mode-1 horizontal paths have the zero Y component folded
    // away: an exact zero horizontal vector remains zero.
    if x != 0. {
        Ok([sign_unit(x), 0., z])
    } else if z != 0. {
        Ok([x, 0., sign_unit(z)])
    } else {
        Ok([x, 0., z])
    }
}
fn unit_elevation(horizontal: f32, y: f32) -> NumericResult<[f32; 3]> {
    let squared = add(mul(horizontal, horizontal)?, mul(y, y)?)?;
    if sub(squared, 1.)?.abs() < 1e-8 {
        return Ok([horizontal, y, 0.]);
    }
    if squared >= 1e-8 {
        let inverse_length = div(1., sqrt(squared)?)?;
        return Ok([
            mul(horizontal, inverse_length)?,
            mul(y, inverse_length)?,
            0.,
        ]);
    }
    if horizontal != 0. {
        Ok([sign_unit(horizontal), y, 0.])
    } else {
        // Native Normalize's final Y branch selects +1 even for exact zero.
        Ok([horizontal, sign_unit(y), 0.])
    }
}
fn unit_yz(y: f32, z: f32) -> NumericResult<[f32; 3]> {
    let squared = add(mul(y, y)?, mul(z, z)?)?;
    if sub(squared, 1.)?.abs() < 1e-8 {
        return Ok([0., y, z]);
    }
    if squared >= 1e-8 {
        let inverse_length = div(1., sqrt(squared)?)?;
        return Ok([0., mul(y, inverse_length)?, mul(z, inverse_length)?]);
    }
    if z != 0. {
        Ok([0., y, sign_unit(z)])
    } else {
        Ok([0., sign_unit(y), z])
    }
}
fn unit_xy(x: f32, y: f32) -> NumericResult<[f32; 3]> {
    let squared = add(mul(x, x)?, mul(y, y)?)?;
    if sub(squared, 1.)?.abs() < 1e-8 {
        return Ok([x, y, 0.]);
    }
    if squared >= 1e-8 {
        let inverse_length = div(1., sqrt(squared)?)?;
        return Ok([mul(x, inverse_length)?, mul(y, inverse_length)?, 0.]);
    }
    if x != 0. {
        Ok([sign_unit(x), y, 0.])
    } else {
        Ok([x, sign_unit(y), 0.])
    }
}
fn row(matrix: &SMatrix, index: usize) -> [f32; 4] {
    matrix.elements[index * 4..index * 4 + 4]
        .try_into()
        .unwrap()
}
fn write_row(matrix: &mut SMatrix, index: usize, value: [f32; 4]) {
    matrix.elements[index * 4..index * 4 + 4].copy_from_slice(&value);
}
fn is_affine(matrix: &SMatrix) -> bool {
    matrix.elements[3] == 0.
        && matrix.elements[7] == 0.
        && matrix.elements[11] == 0.
        && matrix.elements[15] == 1.
}
fn row2(a: [f32; 4], x: f32, b: [f32; 4], y: f32, subtract: bool) -> NumericResult<[f32; 4]> {
    let mut result = [0.; 4];
    for c in 0..4 {
        let first = mul(x, a[c])?;
        let second = mul(y, b[c])?;
        result[c] = if subtract {
            sub(first, second)?
        } else {
            add(first, second)?
        };
    }
    Ok(result)
}
/// Bounded PC ProductBillMatrix. Arithmetic outside normal-f32-or-zero host
/// support refuses a result; no partial matrix is published on such failure.
/// Lazy native matrix getters and their reached effects belong to the caller.
pub fn product_bill_matrix(
    orientation: BillboardOrientation,
    input: &BillboardMatrixInput<'_>,
) -> NumericResult<SMatrix> {
    if orientation.rot_axis > 2 || orientation.axis_mode > 3 {
        return Err(BillboardNumericUnavailable::InvalidOrientation);
    }
    if !matches!(
        (orientation.axis_mode, input),
        (0 | 1, BillboardMatrixInput::Facing { .. })
            | (2, BillboardMatrixInput::CameraAligned { .. })
            | (3, BillboardMatrixInput::LocalAligned { .. })
    ) {
        return Err(BillboardNumericUnavailable::InputModeMismatch);
    }
    let mut result = match input {
        BillboardMatrixInput::Facing { render_matrix, .. } => **render_matrix,
        BillboardMatrixInput::CameraAligned {
            camera_view_inverse,
            ..
        } => **camera_view_inverse,
        BillboardMatrixInput::LocalAligned { local_matrix, .. } => **local_matrix,
    };
    match orientation.axis_mode {
        0 => {
            let BillboardMatrixInput::Facing {
                camera_world_position,
                inverse_matrix,
                render_matrix,
            } = input
            else {
                unreachable!()
            };
            let v = inverse_matrix.try_transform_point(*camera_world_position)?;
            // Axis 1 normalizes elevation before horizontal; 0/2 do the
            // reverse. Keep native arithmetic and precision-state order.
            let elevation = || -> NumericResult<[f32; 3]> {
                let length = sqrt(add(mul(v[2], v[2])?, mul(v[0], v[0])?)?)?;
                unit_elevation(length, v[1])
            };
            let (a, e) = if orientation.rot_axis == 1 {
                let e = elevation()?;
                (unit_horizontal(v[0], v[2])?, e)
            } else {
                let a = unit_horizontal(v[0], v[2])?;
                (a, elevation()?)
            };
            let r0 = row(render_matrix, 0);
            let r1 = row(render_matrix, 1);
            let r2 = row(render_matrix, 2);
            match orientation.rot_axis {
                0 => {
                    for c in 0..4 {
                        let axial = add(mul(a[0], r0[c])?, mul(a[2], r2[c])?)?;
                        result.elements[c] = add(mul(e[1], r1[c])?, mul(axial, e[0])?)?;
                    }
                    for c in 0..4 {
                        let axial = add(mul(a[0], r0[c])?, mul(a[2], r2[c])?)?;
                        result.elements[4 + c] = sub(mul(e[0], r1[c])?, mul(axial, e[1])?)?;
                    }
                    write_row(&mut result, 2, row2(r2, a[0], r0, a[2], true)?);
                }
                1 => {
                    for c in 0..4 {
                        let axial = add(mul(a[0], r0[c])?, mul(a[2], r2[c])?)?;
                        result.elements[c] = sub(mul(axial, e[1])?, mul(e[0], r1[c])?)?;
                    }
                    for c in 0..4 {
                        let axial = add(mul(a[0], r0[c])?, mul(a[2], r2[c])?)?;
                        result.elements[4 + c] = add(mul(axial, e[0])?, mul(e[1], r1[c])?)?;
                    }
                    write_row(&mut result, 2, row2(r2, a[0], r0, a[2], true)?);
                }
                2 => {
                    write_row(&mut result, 0, row2(r2, a[0], r0, a[2], true)?);
                    for c in 0..4 {
                        let axial = add(mul(a[0], r0[c])?, mul(a[2], r2[c])?)?;
                        result.elements[4 + c] = sub(mul(e[0], r1[c])?, mul(axial, e[1])?)?;
                    }
                    for c in 0..4 {
                        let axial = add(mul(a[0], r0[c])?, mul(a[2], r2[c])?)?;
                        result.elements[8 + c] = sub(-mul(e[1], r1[c])?, mul(axial, e[0])?)?;
                    }
                }
                _ => unreachable!(),
            }
        }
        1 => {
            let BillboardMatrixInput::Facing {
                camera_world_position,
                inverse_matrix,
                render_matrix,
            } = input
            else {
                unreachable!()
            };
            let v = inverse_matrix.try_transform_point(*camera_world_position)?;
            let r0 = row(render_matrix, 0);
            let r1 = row(render_matrix, 1);
            let r2 = row(render_matrix, 2);
            match orientation.rot_axis {
                0 => {
                    let d = unit_yz(v[1], v[2])?;
                    write_row(&mut result, 1, row2(r2, d[1], r1, d[2], true)?);
                    write_row(&mut result, 2, row2(r1, -d[1], r2, d[2], true)?);
                }
                1 => {
                    let a = unit_horizontal(v[0], v[2])?;
                    write_row(&mut result, 0, row2(r2, a[0], r0, a[2], true)?);
                    write_row(&mut result, 2, row2(r0, -a[0], r2, a[2], true)?);
                }
                2 => {
                    let d = unit_xy(v[0], v[1])?;
                    write_row(&mut result, 0, row2(r0, d[1], r1, d[0], true)?);
                    write_row(&mut result, 1, row2(r0, d[0], r1, d[1], false)?);
                }
                _ => unreachable!(),
            }
        }
        2 => {
            let BillboardMatrixInput::CameraAligned {
                camera_view_inverse: camera,
                world_scale,
                world_matrix,
            } = input
            else {
                unreachable!()
            };
            if !is_affine(camera) {
                return Err(BillboardNumericUnavailable::NonAffineMatrix);
            }
            let previous_row1 = row(camera, 1);
            for (index, (source, scale)) in [
                (row(camera, 0), world_scale[0]),
                (row(camera, 2), world_scale[1]),
                (previous_row1, world_scale[2]),
            ]
            .into_iter()
            .enumerate()
            {
                // Native S_vector row scaling writes XYZ only; preserve the
                // copied camera homogeneous column, including signed zero.
                for (c, &component) in source.iter().take(3).enumerate() {
                    result.elements[index * 4 + c] = mul(component, scale)?;
                }
            }
            result.elements[12..15].copy_from_slice(&world_matrix.elements[12..15]);
            result.elements[15] = camera.elements[15];
        }
        3 => {
            let BillboardMatrixInput::LocalAligned {
                local_matrix,
                camera_view_inverse,
                world_matrix,
            } = input
            else {
                unreachable!()
            };
            if [*local_matrix, *camera_view_inverse]
                .into_iter()
                .any(|m| !is_affine(m))
            {
                // The shared bounded SMatrix product is affine; do not claim
                // the native full product for a non-affine input.
                return Err(BillboardNumericUnavailable::NonAffineMatrix);
            }
            result = local_matrix.try_mul_affine(camera_view_inverse)?;
            result.elements[12..15].copy_from_slice(&world_matrix.elements[12..15]);
        }
        _ => unreachable!(),
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input<'a>(identity: &'a SMatrix, camera: [f32; 3]) -> BillboardMatrixInput<'a> {
        BillboardMatrixInput::Facing {
            camera_world_position: camera,
            inverse_matrix: identity,
            render_matrix: identity,
        }
    }
    #[test]
    fn constructor_and_load_corrections_follow_primary_pc() {
        assert_eq!(
            BillboardOrientation::default(),
            BillboardOrientation {
                rot_axis: 2,
                axis_mode: 0
            }
        );
        assert_eq!(
            BillboardOrientation::from_source(2, 0),
            BillboardOrientation::default()
        );
        assert_eq!(
            BillboardOrientation::from_source(3, 0),
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 2
            }
        );
        assert_eq!(
            BillboardOrientation::from_source(1, 4),
            BillboardOrientation {
                rot_axis: 1,
                axis_mode: 1
            }
        );
        assert_eq!(
            BillboardOrientation::from_source(4, 255),
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 2
            }
        );
    }
    #[test]
    fn actual_axis_two_mode_zero_reads_each_camera_pose() {
        let matrix = SMatrix::identity();
        let orientation = BillboardOrientation::default();
        let front = product_bill_matrix(orientation, &input(&matrix, [0., 0., 2.])).unwrap();
        let side = product_bill_matrix(orientation, &input(&matrix, [2., 0., 0.])).unwrap();
        assert_eq!(row(&front, 0)[..3], [-1., 0., 0.]);
        assert_eq!(row(&front, 1)[..3], [0., 1., 0.]);
        assert_eq!(row(&front, 2)[..3], [0., 0., -1.]);
        assert_eq!(row(&side, 0)[..3], [0., 0., 1.]);
        assert_ne!(front, side);
    }
    #[test]
    fn all_modes_axes_change_only_the_source_rows() {
        let matrix = SMatrix::identity();
        for mode in 0..=1 {
            for axis in 0..=2 {
                let produced = product_bill_matrix(
                    BillboardOrientation {
                        rot_axis: axis,
                        axis_mode: mode,
                    },
                    &input(&matrix, [2., 3., 4.]),
                )
                .unwrap();
                assert_eq!(&produced.elements[12..16], &matrix.elements[12..16]);
                assert!(produced.elements.iter().all(|x| x.is_finite()));
            }
        }
        let mut camera = matrix;
        camera.elements[0] = 2.;
        let mut world = matrix;
        world.elements[12] = 7.;
        let args = BillboardMatrixInput::CameraAligned {
            camera_view_inverse: &camera,
            world_scale: [3., 4., 5.],
            world_matrix: &world,
        };
        let mode2 = product_bill_matrix(
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 2,
            },
            &args,
        )
        .unwrap();
        assert_eq!(row(&mode2, 0)[0], 6.);
        assert_eq!(row(&mode2, 1)[2], 4.);
        assert_eq!(row(&mode2, 2)[1], 5.);
        assert_eq!(row(&mode2, 3)[0], 7.);
        let args = BillboardMatrixInput::LocalAligned {
            local_matrix: &matrix,
            camera_view_inverse: &camera,
            world_matrix: &world,
        };
        let mode3 = product_bill_matrix(
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 3,
            },
            &args,
        )
        .unwrap();
        assert_eq!(row(&mode3, 0)[0], 2.);
        assert_eq!(row(&mode3, 3)[0], 7.);
    }
    #[test]
    fn native_near_unit_and_zero_horizontal_branches_are_distinct() {
        assert_eq!(unit_horizontal(0., 0.).unwrap(), [0., 0., 0.]);
        assert_eq!(unit_elevation(0., 0.).unwrap(), [0., 1., 0.]);
        assert_eq!(unit_horizontal(-0.00001, 0.).unwrap()[0], -1.);
        assert_eq!(unit_horizontal(1., 0.).unwrap(), [1., 0., 0.]);
        let matrix = SMatrix::identity();
        assert!(product_bill_matrix(
            BillboardOrientation::default(),
            &input(&matrix, [0., 0., 0.])
        )
        .is_ok());
    }
    #[test]
    fn camera_modes_refuse_non_affine_input_before_a_false_matrix() {
        let matrix = SMatrix::identity();
        let mut projection = matrix;
        projection.elements[11] = 0.25;
        let aligned = BillboardMatrixInput::CameraAligned {
            camera_view_inverse: &projection,
            world_scale: [1.; 3],
            world_matrix: &matrix,
        };
        let local = BillboardMatrixInput::LocalAligned {
            local_matrix: &matrix,
            camera_view_inverse: &projection,
            world_matrix: &matrix,
        };
        assert_eq!(
            product_bill_matrix(
                BillboardOrientation {
                    rot_axis: 2,
                    axis_mode: 2
                },
                &aligned
            ),
            Err(BillboardNumericUnavailable::NonAffineMatrix)
        );
        assert_eq!(
            product_bill_matrix(
                BillboardOrientation {
                    rot_axis: 2,
                    axis_mode: 3
                },
                &local
            ),
            Err(BillboardNumericUnavailable::NonAffineMatrix)
        );
    }
    #[test]
    fn camera_aligned_mode_needs_no_inverse_or_render_matrix() {
        let camera = SMatrix::identity();
        let mut singular_frame = camera;
        singular_frame.elements[0] = 0.;
        singular_frame.elements[5] = 0.;
        singular_frame.elements[10] = 0.;
        singular_frame.elements[12] = 7.;
        let mode_two = BillboardMatrixInput::CameraAligned {
            camera_view_inverse: &camera,
            world_scale: [0., 0., 0.],
            world_matrix: &singular_frame,
        };
        let out = product_bill_matrix(
            BillboardOrientation {
                rot_axis: 2,
                axis_mode: 2,
            },
            &mode_two,
        )
        .unwrap();
        assert_eq!(out.elements[12], 7.);
        assert_eq!(out.elements[0], 0.);
        assert_eq!(
            product_bill_matrix(BillboardOrientation::default(), &mode_two),
            Err(BillboardNumericUnavailable::InputModeMismatch)
        );
    }
}
