//! Original PC S_matrix::SetDir (LS3DF RVA 0x2d9d0), without a roll angle.
//! Separate f32 operations follow the game's PC24/RN arithmetic. Inputs and
//! intermediates must be finite normal f32 values or zero; exponent-edge x87
//! behavior and nonfinite inputs remain unavailable.
use crate::{
    native_matrix::{checked_f32_add as add, checked_f32_mul as mul, MatrixArithmeticUnavailable},
    native_transform::SMatrix,
};

type Result<T> = std::result::Result<T, MatrixArithmeticUnavailable>;

fn checked(value: f32, exact: f64) -> Result<f32> {
    if !value.is_finite()
        || (exact != 0.0 && exact.abs() < f64::from(f32::MIN_POSITIVE))
        || exact.abs() > f64::from(f32::MAX)
    {
        Err(MatrixArithmeticUnavailable)
    } else {
        Ok(value)
    }
}
fn sub(a: f32, b: f32) -> Result<f32> {
    add(a, -b)
}
fn reciprocal_sqrt(value: f32) -> Result<f32> {
    let root = checked(value.sqrt(), f64::from(value).sqrt())?;
    checked(1.0 / root, 1.0 / f64::from(root))
}
fn normalized(mut value: [f32; 3]) -> Result<[f32; 3]> {
    let squared = add(
        add(mul(value[2], value[2])?, mul(value[1], value[1])?)?,
        mul(value[0], value[0])?,
    )?;
    if f64::from(sub(squared, 1.0)?).abs() < 1e-8 {
        return Ok(value);
    }
    if squared >= 1e-8_f32 {
        let reciprocal = reciprocal_sqrt(squared)?;
        for component in &mut value {
            *component = mul(reciprocal, *component)?;
        }
    } else {
        let axis = if value[0] != 0.0 {
            0
        } else if value[2] != 0.0 {
            2
        } else {
            1
        };
        value[axis] = if value[axis] < 0.0 { -1.0 } else { 1.0 };
    }
    Ok(value)
}

/// Build the original no-roll direction matrix, including its small-direction
/// axis fallback. The second row uses the original input vector rather than
/// the normalized third row; no extra orthonormalization is performed.
pub fn try_target_direction_matrix(direction: [f32; 3]) -> Result<SMatrix> {
    let [x, y, z] = direction;
    if direction.into_iter().any(|v| v != 0.0 && !v.is_normal()) {
        return Err(MatrixArithmeticUnavailable);
    }
    let mut matrix = SMatrix::identity();
    matrix.elements[8..11].copy_from_slice(&normalized(direction)?);

    let horizontal_squared = add(mul(z, z)?, mul(x, x)?)?;
    if horizontal_squared > 1e-8_f32 {
        let reciprocal = if f64::from(sub(horizontal_squared, 1.0)?).abs() < 1e-8 {
            1.0
        } else {
            reciprocal_sqrt(horizontal_squared)?
        };
        matrix.elements[0] = mul(reciprocal, z)?;
        matrix.elements[2] = -mul(reciprocal, x)?;
    }
    if f64::from(add(z.abs(), x.abs())?) > 1e-8 {
        let right = normalized([z, 0.0, -x])?;
        matrix.elements[..3].copy_from_slice(&right);
    }
    let [rx, ry, rz] = matrix.elements[..3].try_into().unwrap();
    matrix.elements[4] = -sub(mul(ry, z)?, mul(rz, y)?)?;
    matrix.elements[5] = -sub(mul(rz, x)?, mul(z, rx)?)?;
    matrix.elements[6] = -sub(mul(y, rx)?, mul(ry, x)?)?;
    Ok(matrix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_direction_magnitude_and_zero_fallback_are_preserved() {
        let matrix = try_target_direction_matrix([0., 0., 2.]).unwrap();
        assert_eq!(matrix.elements[0], 1.);
        assert_eq!(matrix.elements[5], 2.);
        assert_eq!(matrix.elements[10], 1.);
        let zero = try_target_direction_matrix([0.; 3]).unwrap();
        assert_eq!(zero.elements[9], 1.);
        assert_eq!(zero.elements[0], 1.);
        assert_eq!(zero.elements[4].to_bits(), (-0.0_f32).to_bits());
    }

    #[test]
    fn unsupported_exponent_edges_are_explicit() {
        for direction in [
            [f32::NAN, 0., 1.],
            [f32::INFINITY, 0., 0.],
            [f32::from_bits(1), 0., 1.],
            [1e-30, 0., 0.],
            [1e30, 0., 1.],
        ] {
            assert!(try_target_direction_matrix(direction).is_err());
        }
    }
}
