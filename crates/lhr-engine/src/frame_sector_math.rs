//! PC24 finite-normal-or-zero arithmetic for sector/portal derived geometry.
//! Static assembly contracts4f710/419e0 and raw GetNormalddd0; no native runtime
//! comparison is claimed. Extended-exponent/subnormal/nonfinite arithmetic is
//! an explicit availability boundary rather than a native rejection.
use super::SectorError;
fn number(v: f32) -> Result<f32, SectorError> {
    if v.is_finite() && !v.is_subnormal() {
        Ok(v)
    } else {
        Err(SectorError::NumericUnavailable)
    }
}
pub(crate) fn add(a: f32, b: f32) -> Result<f32, SectorError> {
    number(a)?;
    number(b)?;
    number(crate::native_precision::add(a, b))
}
pub(crate) fn sub(a: f32, b: f32) -> Result<f32, SectorError> {
    number(a)?;
    number(b)?;
    number(crate::native_precision::sub(a, b))
}
pub(crate) fn mul(a: f32, b: f32) -> Result<f32, SectorError> {
    number(a)?;
    number(b)?;
    let value = number(crate::native_precision::mul(a, b))?;
    if value == 0. && a != 0. && b != 0. {
        Err(SectorError::NumericUnavailable)
    } else {
        Ok(value)
    }
}
pub(crate) fn sqrt(v: f32) -> Result<f32, SectorError> {
    number(v)?;
    number(crate::native_precision::sqrt(v))
}
pub(crate) fn squared(v: [f32; 3]) -> Result<f32, SectorError> {
    add(add(mul(v[2], v[2])?, mul(v[1], v[1])?)?, mul(v[0], v[0])?)
}
pub(crate) fn transformed(
    m: &crate::native_transform::SMatrix,
    v: [f32; 3],
) -> Result<[f32; 3], SectorError> {
    let mut out = [0.; 3];
    for (axis, component) in out.iter_mut().enumerate() {
        *component = add(
            add(
                add(mul(v[0], m.elements[axis])?, m.elements[12 + axis])?,
                mul(v[1], m.elements[4 + axis])?,
            )?,
            mul(v[2], m.elements[8 + axis])?,
        )?;
    }
    Ok(out)
}
pub(crate) fn plane(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Result<[f32; 4], SectorError> {
    let mut ab = [0.; 3];
    let mut cb = [0.; 3];
    for i in [2, 1, 0] {
        cb[i] = sub(c[i], b[i])?;
        ab[i] = sub(a[i], b[i])?;
    }
    let mut normal = [
        sub(mul(ab[2], cb[1])?, mul(ab[1], cb[2])?)?,
        sub(mul(ab[0], cb[2])?, mul(cb[0], ab[2])?)?,
        sub(mul(cb[0], ab[1])?, mul(ab[0], cb[1])?)?,
    ];
    let square = squared(normal)?;
    let epsilon = f64::from_bits(0x3e45798ee0000000);
    if f64::from(sub(square, 1.)?.abs()) >= epsilon {
        if square >= f32::from_bits(0x322bcc77) {
            let length = sqrt(square)?;
            let inverse = number(crate::native_precision::div(1., length))?;
            for n in &mut normal {
                *n = mul(*n, inverse)?;
            }
        } else {
            let axis = if normal[0] != 0. {
                0
            } else if normal[2] != 0. {
                2
            } else {
                1
            };
            normal[axis] = if normal[axis] >= 0. { 1. } else { -1. };
        }
    }
    let d = -add(
        add(mul(a[2], normal[2])?, mul(a[1], normal[1])?)?,
        mul(a[0], normal[0])?,
    )?;
    Ok([normal[0], normal[1], normal[2], d])
}
pub(crate) fn magnitude(v: [f32; 3]) -> Result<f32, SectorError> {
    let square = squared(v)?;
    if (square.to_bits() as i32) < 0x322bcc77 {
        return Ok(0.);
    }
    if f64::from(sub(square, 1.)?.abs()) < f64::from_bits(0x3e45798ee0000000) {
        Ok(1.)
    } else {
        sqrt(square)
    }
}
