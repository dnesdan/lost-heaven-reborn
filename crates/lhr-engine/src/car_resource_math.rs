//! Checked PC24 arithmetic for Game car animation placement helpers. Included
//! below native_trig so the existing retained transcendental enclosures apply.
use super::{atan2_positive_interval, sin_cos_frame, Arithmetic, Interval, MakeUnavailable};

pub(crate) fn normalized_z(vector: [f32; 3]) -> Result<f32, MakeUnavailable> {
    let a = Arithmetic::new();
    let [x, y, z] = vector.map(|v| a.input(v));
    let squared = (y * y + z * z) + x * x;
    a.finish()?;
    let out = if f64::from((squared - a.input(1.)).abs().value) < f64::from(1e-8_f32) {
        z.value
    } else if squared.value >= 1e-8_f32 {
        let reciprocal = a.input(1.) / squared.sqrt();
        let _x = x * reciprocal;
        let _y = y * reciprocal;
        (z * reciprocal).value
    } else if x.value == 0. && z.value != 0. {
        if z.value < 0. {
            -1.
        } else {
            1.
        }
    } else {
        z.value
    };
    a.finish()?;
    Ok(out)
}

/// Game 0x408490: retain both FPATAN values until their PC24 subtraction.
pub(crate) fn animation_angle(first: [f32; 3], last: [f32; 3]) -> Result<f32, MakeUnavailable> {
    if first[0].is_finite()
        && first[2].is_finite()
        && first[0].to_bits() == last[0].to_bits()
        && first[2].to_bits() == last[2].to_bits()
    {
        // The two identical FPATAN instructions produce identical retained
        // values, so their subtraction is exact even near zero.
        // They still raise precision for a nonzero angle, including pi on
        // the negative X axis. The value shortcut must preserve that history.
        crate::native_precision::record_precision(first[2] != 0.0 || first[0].is_sign_negative());
        return Ok(0.);
    }
    fn angle(x: f32, y: f32) -> Result<Interval, MakeUnavailable> {
        let value = atan2_positive_interval(y.abs(), x)?;
        // This caller executes FPATAN even on the negative axis, whereas the
        // CRT acos endpoint represented by the shared enclosure uses FLDPI.
        crate::native_precision::record_precision(y == 0.0 && x < 0.0);
        Ok(if y.is_sign_negative() {
            value.neg()
        } else {
            value
        })
    }
    let value = angle(first[0], first[2])?
        .sub(angle(last[0], last[2])?)
        .rounded()?;
    let a = Arithmetic::new();
    let mut value = a.input(value);
    if value.value < f32::from_bits(0xc0490fdb) {
        value = value + a.input(f32::from_bits(0x40c90fdb));
    }
    if value.value > f32::from_bits(0x40490fdb) {
        value = value - a.input(f32::from_bits(0x40c90fdb));
    }
    a.finish()?;
    Ok(value.value)
}

/// Game 0x408630, and 0x4084d0's rotation prefix. FSIN/FCOS are spilled to
/// matrix f32 slots before RotateByMatrix evaluates the three row products.
pub(crate) fn rotate_y(vector: [f32; 3], angle: f32) -> Result<[f32; 3], MakeUnavailable> {
    let (sine, cosine) = if angle == 0. {
        (angle, 1.)
    } else {
        let (sine, cosine) = sin_cos_frame(angle)?;
        (sine.rounded()?, cosine.rounded()?)
    };
    let a = Arithmetic::new();
    let [x, y, z] = vector.map(|v| a.input(v));
    let s = a.input(sine);
    let c = a.input(cosine);
    let zero = a.input(0.);
    let out = [
        ((x * c + y * zero) + z * -s).value,
        ((x * zero + y * a.input(1.)) + z * zero).value,
        ((x * s + y * zero) + z * c).value,
    ];
    a.finish()?;
    Ok(out)
}

/// Game 0x4084d0's normalization after forcing the vertical component to +0.
pub(crate) enum HorizontalNormalization {
    Unchanged,
    Axis { axis: usize, value: f32 },
    Scaled([f32; 3]),
}
pub(crate) fn normalize_horizontal(
    vector: [f32; 3],
) -> Result<HorizontalNormalization, MakeUnavailable> {
    let a = Arithmetic::new();
    let x = a.input(vector[0]);
    let z = a.input(vector[2]);
    let squared = x * x + z * z;
    a.finish()?;
    let out = [x.value, 0., z.value];
    let change = if f64::from((squared - a.input(1.)).abs().value) >= f64::from(1e-8_f32) {
        if squared.value < 1e-8_f32 {
            let axis = if out[0] != 0. {
                0
            } else if out[2] != 0. {
                2
            } else {
                1
            };
            HorizontalNormalization::Axis {
                axis,
                value: if out[axis] < 0. { -1. } else { 1. },
            }
        } else {
            let reciprocal = a.input(1.) / squared.sqrt();
            HorizontalNormalization::Scaled([
                (reciprocal * x).value,
                (a.input(0.) * reciprocal).value,
                (reciprocal * z).value,
            ])
        }
    } else {
        HorizontalNormalization::Unchanged
    };
    a.finish()?;
    Ok(change)
}

pub(crate) fn add(left: f32, right: f32) -> Result<f32, MakeUnavailable> {
    let a = Arithmetic::new();
    let value = (a.input(left) + a.input(right)).value;
    a.finish()?;
    Ok(value)
}
pub(crate) fn subtract(left: f32, right: f32) -> Result<f32, MakeUnavailable> {
    let a = Arithmetic::new();
    let value = (a.input(left) - a.input(right)).value;
    a.finish()?;
    Ok(value)
}
