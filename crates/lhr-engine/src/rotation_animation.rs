//! Bounded rotation branch of original LS3DF evaluator RVA 0x1d320.
//!
//! LS3DF SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Descriptive sampling adapters below cover one enabled rotation contribution,
//! before boundary handling, with no pose override. They normalize the result as
//! the original destination application does. They do not apply frame flags,
//! blend controller slots, dispatch notes, or reconstruct the whole evaluator.
//! Arithmetic assumes Game's x87 PC24, round-to-nearest control word 0x003f.

use crate::native_transform::SQuat;
use lhr_assets::animation::RotationTrack;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RotationUnavailable {
    InvalidTrack,
    TimeOutsideVerifiedDomain,
    NumericDomain,
    /// The original retained FPATAN/FSIN spherical branch is not ported here.
    SphericalInterpolation,
}

// x87 PC24 has a wider exponent range than f32. Check EVERY intermediate,
// including ones which could subsequently return to the ordinary f32 range.
fn ordinary(value: f64) -> Result<f32, RotationUnavailable> {
    let rounded = value as f32;
    if !rounded.is_finite()
        || (rounded != 0. && !rounded.is_normal())
        || (rounded == 0. && value != 0.)
    {
        Err(RotationUnavailable::NumericDomain)
    } else {
        Ok(rounded)
    }
}
fn input(value: f32) -> Result<f32, RotationUnavailable> {
    ordinary(value as f64)
}
fn add(a: f32, b: f32) -> Result<f32, RotationUnavailable> {
    ordinary(a as f64 + b as f64)
}
fn sub(a: f32, b: f32) -> Result<f32, RotationUnavailable> {
    ordinary(a as f64 - b as f64)
}
fn mul(a: f32, b: f32) -> Result<f32, RotationUnavailable> {
    ordinary(a as f64 * b as f64)
}
fn div(a: f32, b: f32) -> Result<f32, RotationUnavailable> {
    ordinary(a as f64 / b as f64)
}

/// Original exported `S_quat::Slerp`, RVA 0x2fee0, restricted to its linear
/// branch and t in [0,1]. WXYZ inputs need not be unit quaternions. A negative
/// dot negates the second quaternion; no normalization occurs in this leaf.
/// The original unused boolean argument has no effect on this branch.
/// Wider angles return an availability error, never a substitute interpolation.
pub fn try_slerp_linear(
    first: SQuat,
    second: SQuat,
    fraction: f32,
) -> Result<SQuat, RotationUnavailable> {
    for value in first.wxyz.into_iter().chain(second.wxyz).chain([fraction]) {
        input(value)?;
    }
    if !(0. ..=1.).contains(&fraction) {
        return Err(RotationUnavailable::NumericDomain);
    }
    let a = first.wxyz;
    let mut b = second.wxyz;
    // Assembly starts with components2 and1, then0, then3.
    let dot = add(
        add(add(mul(b[2], a[2])?, mul(b[1], a[1])?)?, mul(b[0], a[0])?)?,
        mul(a[3], b[3])?,
    )?;
    if dot < 0. {
        b = b.map(|value| -value);
    }
    // FSUB is PC24; the comparison constant is the original DOUBLE 0.0001.
    if sub(1., dot.abs())? as f64 > 0.0001_f64 {
        return Err(RotationUnavailable::SphericalInterpolation);
    }
    let complement = sub(1., fraction)?;
    let mut result = [0.; 4];
    // Component3 and0 use the reverse addition order (signed-zero behavior).
    for component in [1, 2, 3, 0] {
        let left = mul(a[component], complement)?;
        let right = mul(b[component], fraction)?;
        result[component] = if component < 3 && component != 0 {
            add(right, left)?
        } else {
            add(left, right)?
        };
    }
    Ok(SQuat { wxyz: result })
}

/// Guarded adapter to the already reconstructed `S_quat::Normalize` RVA30140.
/// Degenerate quaternions retain the native X, Z, Y axis-selection behavior.
fn normalize(mut quaternion: SQuat) -> Result<SQuat, RotationUnavailable> {
    let [w, x, y, z] = quaternion.wxyz;
    for value in quaternion.wxyz {
        input(value)?;
    }
    let squared = add(add(add(mul(w, w)?, mul(x, x)?)?, mul(y, y)?)?, mul(z, z)?)?;
    if squared <= 1e-8 {
        quaternion.wxyz[0] = 0.;
        let axis = if x != 0. {
            1
        } else if z != 0. {
            3
        } else {
            2
        };
        quaternion.wxyz[axis] = if quaternion.wxyz[axis] < 0. { -1. } else { 1. };
    } else if sub(squared, 1.)?.abs() as f64 >= 1e-8_f32 as f64 {
        let length = ordinary((squared as f64).sqrt())?;
        let reciprocal = div(1., length)?;
        for axis in [1, 2, 3, 0] {
            quaternion.wxyz[axis] = mul(quaternion.wxyz[axis], reciprocal)?;
        }
    }
    Ok(quaternion)
}

/// Sample one actual 5DS rotation channel and normalize the destination value.
pub fn sample_rotation_channel(
    track: &RotationTrack,
    current_ms: u32,
) -> Result<SQuat, RotationUnavailable> {
    sample_rotation_keys(track.key_times(), track.key_values(), current_ms)
}

/// Slice adapter for caller-owned records, in original WXYZ order. Strictly
/// increasing keys and nonnegative times through frame65535 are admitted.
/// Selection uses integer time/40; the interpolation fraction separately uses
/// the original f32 0.025 constant. Thus an exact key time can still execute
/// Slerp, and times before/after the track clamp without looping.
pub fn sample_rotation_keys(
    times: &[u16],
    values: &[[f32; 4]],
    current_ms: u32,
) -> Result<SQuat, RotationUnavailable> {
    if times.is_empty()
        || times.len() != values.len()
        || times.len() > u16::MAX as usize
        || times.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(RotationUnavailable::InvalidTrack);
    }
    if current_ms > u16::MAX as u32 * 40 + 39 {
        return Err(RotationUnavailable::TimeOutsideVerifiedDomain);
    }
    let frame = (current_ms / 40) as u16;
    let last = times.len() - 1;
    let sampled = if frame < times[0] {
        SQuat { wxyz: values[0] }
    } else if frame >= times[last] {
        SQuat { wxyz: values[last] }
    } else {
        let right = times.partition_point(|&time| time <= frame);
        let left = right - 1;
        let frame_time = mul(current_ms as f32, f32::from_bits(0x3ccc_cccd))?;
        let numerator = sub(frame_time, f32::from(times[left]))?;
        let fraction = div(numerator, f32::from(times[right] - times[left]))?;
        try_slerp_linear(
            SQuat { wxyz: values[left] },
            SQuat {
                wxyz: values[right],
            },
            fraction,
        )?
    };
    normalize(sampled)
}

#[cfg(test)]
mod tests {
    use super::*;
    type SlerpCase = ([u32; 4], [u32; 4], u32, [u32; 4]);

    #[test]
    fn synthetic_inputs_match_original_slerp_component_bits() {
        // Private original-PC oracle, CW003f; seeded synthetic inputs only.
        let cases: [SlerpCase; 3] = [
            (
                [0x3ee4b720, 0xbbf0f444, 0xbeb8fce9, 0x3f5185be],
                [0x3ee58180, 0xbbeed766, 0xbeb9e04f, 0x3f511c0a],
                0x3e800000,
                [0x3ee4e9b8, 0xbbf06d0c, 0xbeb935c3, 0x3f516b50],
            ),
            (
                [0xbe2a6fb6, 0xbf69c08e, 0xbe2fc436, 0xbea91c17],
                [0xbe28a284, 0xbf69dc9a, 0xbe2f7b04, 0xbea9078f],
                0x3f000000,
                [0xbe29891d, 0xbf69ce94, 0xbe2f9f9d, 0xbea911d3],
            ),
            (
                [0xbe9d0fdc, 0xbd31f97c, 0xbf2bf6f1, 0xbf2c426d],
                [0x3e9ca205, 0x3d393f3e, 0x3f2c1f8a, 0x3f2c2b35],
                0x3f7fbe77,
                [0xbe9ca221, 0xbd393d62, 0xbf2c1f80, 0xbf2c2b3b],
            ),
        ];
        for (a, b, fraction, expected) in cases {
            let actual = try_slerp_linear(
                SQuat {
                    wxyz: a.map(f32::from_bits),
                },
                SQuat {
                    wxyz: b.map(f32::from_bits),
                },
                f32::from_bits(fraction),
            )
            .unwrap();
            assert_eq!(actual.wxyz.map(f32::to_bits), expected);
        }
    }

    #[test]
    fn antipodal_linear_branch_is_not_normalized_by_the_leaf() {
        let a = SQuat {
            wxyz: [2., 0., -0., 0.],
        };
        let b = SQuat {
            wxyz: [-2., -0., 0., -0.],
        };
        assert_eq!(try_slerp_linear(a, b, 0.25).unwrap().wxyz[0], 2.);
        assert_eq!(
            sample_rotation_keys(&[0, 10], &[a.wxyz, b.wxyz], 100)
                .unwrap()
                .wxyz[0],
            1.
        );
    }

    #[test]
    fn endpoints_normalize_and_degenerate_axis_selection_is_native() {
        let keys = [[0., 0., 0., 0.], [2., 0., 0., 0.]];
        assert_eq!(
            sample_rotation_keys(&[2, 3], &keys, 0).unwrap().wxyz,
            [0., 0., 1., 0.]
        );
        assert_eq!(
            sample_rotation_keys(&[2, 3], &keys, 200).unwrap().wxyz,
            [1., 0., 0., 0.]
        );
    }

    #[test]
    fn unavailable_paths_and_extended_exponent_edges_fail_explicitly() {
        let a = SQuat {
            wxyz: [1., 0., 0., 0.],
        };
        let b = SQuat {
            wxyz: [0., 1., 0., 0.],
        };
        assert_eq!(
            try_slerp_linear(a, b, 0.5),
            Err(RotationUnavailable::SphericalInterpolation)
        );
        assert_eq!(
            sample_rotation_keys(&[1, 1], &[a.wxyz; 2], 0),
            Err(RotationUnavailable::InvalidTrack)
        );
        assert_eq!(
            sample_rotation_keys(&[0], &[a.wxyz], 2_621_440),
            Err(RotationUnavailable::TimeOutsideVerifiedDomain)
        );
        assert_eq!(
            mul(f32::MIN_POSITIVE, 0.5),
            Err(RotationUnavailable::NumericDomain)
        );
        assert_eq!(mul(f32::MAX, 2.), Err(RotationUnavailable::NumericDomain));
        assert!(try_slerp_linear(
            SQuat {
                wxyz: [f32::NAN; 4]
            },
            a,
            0.
        )
        .is_err());
    }
}
