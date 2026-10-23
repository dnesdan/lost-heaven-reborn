//! Frame SetDir quaternion construction at LS3DF RVA0x1ab50. The two
//! quaternion products retain the original cross/dot association and spills.
use super::{native_trig, MakeUnavailable, SQuat};
impl SQuat {
    /// Original frame SetDir's quaternion result, before its final stores.
    /// PC24/RN finite ordinary arithmetic is checked at each operation.
    pub fn try_make_frame_direction(
        direction: [f32; 3],
        roll: f32,
    ) -> Result<Self, MakeUnavailable> {
        let a = native_trig::Arithmetic::new();
        let f = |v| a.input(v);
        let [x, y, z] = direction.map(f);
        a.finish()?;
        let mut pitch = 0.;
        let mut yaw = 0.;
        let epsilon = f64::from(1e-5_f32);
        if f64::from(y.value.abs()) > epsilon {
            let squared = (z * z + y * y) + x * x;
            let length = if (squared.value.to_bits() as i32) < 0x322bcc77 {
                0.
            } else if f64::from((squared - f(1.)).abs().value) < f64::from(1e-8_f32) {
                1.
            } else {
                squared.sqrt().value
            };
            a.finish()?;
            let ratio = if length == 0. {
                y.value.signum() * f32::INFINITY
            } else {
                (y / f(length)).value
            };
            a.finish()?;
            pitch = if ratio.abs() < 1. {
                // CRT asin: FPATAN(x, sqrt((1+x)*(1-x))), preserving PC24.
                let q = f(ratio);
                let root = ((f(1.) + q) * (f(1.) - q)).sqrt().value;
                a.finish()?;
                native_trig::atan_signed(ratio, root)?
            } else if ratio > 0. {
                f32::from_bits(0x3fc90fdb)
            } else {
                f32::from_bits(0xbfc90fdb)
            };
        }
        if f64::from(x.value.abs()) > epsilon {
            if f64::from(z.value.abs()) < epsilon {
                if x.value > 0. {
                    yaw = f32::from_bits(0xbfc90fdb);
                } else if x.value < 0. {
                    yaw = f32::from_bits(0x3fc90fdb);
                }
            } else {
                let ratio = -(x / z);
                a.finish()?;
                yaw = native_trig::atan_signed(ratio.value, 1.)?;
                if z.value < 0. {
                    yaw = (f(yaw) + f(f32::from_bits(0x40490fdb))).value;
                }
            }
        } else if z.value < 0. {
            yaw = f32::from_bits(0x40490fdb);
        }
        a.finish()?;
        let pitch = Self::make_axis_angle_domain([1., 0., 0.], pitch, true)?;
        let yaw = Self::make_axis_angle_domain([0., 1., 0.], yaw, true)?;
        let roll = Self::make_axis_angle_domain([0., 0., 1.], roll, true)?;
        fn product(left: SQuat, right: SQuat) -> Result<SQuat, MakeUnavailable> {
            let a = native_trig::Arithmetic::new();
            let f = |v| a.input(v);
            let [w, x, y, z] = left.wxyz.map(f);
            let [rw, rx, ry, rz] = right.wxyz.map(f);
            let cross = [y * rz - z * ry, z * rx - x * rz, x * ry - y * rx];
            let scaled = [x * rw, y * rw, z * rw];
            let dot = (z * rz + y * ry) + x * rx;
            let value = SQuat {
                wxyz: [
                    (w * rw - dot).value,
                    ((rx * w + scaled[0]) + cross[0]).value,
                    ((ry * w + scaled[1]) + cross[1]).value,
                    ((rz * w + scaled[2]) + cross[2]).value,
                ],
            };
            a.finish()?;
            Ok(value)
        }
        product(product(roll, pitch)?, yaw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_direction_branches_and_quaternion_products() {
        for (direction, roll, expected) in [
            ([1., 0., 0.], 0., [0x3f3504f4, 0, 0xbf3504f4, 0]),
            ([0., 0., -1.], 0., [0xb33bbd2e, 0, 0x3f800000, 0]),
            (
                [1., 1., 1.],
                0.,
                [0x3f6166e4, 0x3e8f4841, 0xbebabaa5, 0xbded65d7],
            ),
            (
                [-2., 3., -4.],
                0.,
                [0x3e6112f4, 0x3d88ffda, 0x3f6e5b77, 0x3e9115b2],
            ),
            (
                [1., 0., 0.],
                f32::from_bits(0x40490fdb),
                [0xb304c063, 0x3f3504f4, 0x3304c063, 0x3f3504f4],
            ),
        ] {
            assert_eq!(
                SQuat::try_make_frame_direction(direction, roll)
                    .unwrap()
                    .wxyz
                    .map(f32::to_bits),
                expected
            );
        }
    }
    #[test]
    fn original_small_direction_cutoff_and_unavailable_domain() {
        for direction in [[0.; 3], [1e-5, 0., 0.], [1e-8, 1e-5, 1e-8]] {
            assert_eq!(
                SQuat::try_make_frame_direction(direction, 0.).unwrap().wxyz,
                [1., 0., 0., 0.]
            );
        }
        assert!(SQuat::try_make_frame_direction([f32::NAN, 0., 0.], 0.).is_err());
        assert!(SQuat::try_make_frame_direction([1., 0., 0.], f32::INFINITY).is_err());
    }
}
