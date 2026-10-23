//! Static primitive dispatch from PC RVA `0x1c60e0`, called by the ordered
//! collision leaf. These are the original five record classifications, including
//! the OBB record's two-sided local Y=0 rectangle and the infinite Y cylinder.
//!
//! Arithmetic admits the same CW `0x007f`, round-to-nearest precision-24 and
//! zero-or-`2^-40..=2^40` used operand/intermediate domain as `collision_line`.
//! The host must supply ordinary IEEE round-to-nearest arithmetic. Floating-point
//! status, traps, other precision profiles and extended exponent behavior are
//! outside this contract. Errors are transactional host policy. Unused stored
//! bounds/matrix columns are preserved and are not numerically validated.

use crate::collision_line::LineQuery;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume {
    pub properties: u32,
    pub identity: u32,
    pub geometry: VolumeGeometry,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VolumeGeometry {
    Aabb {
        min: [f32; 3],
        max: [f32; 3],
    },
    XtObb {
        /// Stored world bounds, unused by this original worker.
        min: [f32; 3],
        max: [f32; 3],
        extent_min: [f32; 3],
        extent_max: [f32; 3],
        matrix: [f32; 16],
        inverse: [f32; 16],
    },
    Sphere {
        center: [f32; 3],
        radius: f32,
    },
    Obb {
        /// Only X and Z bounds are used; the tested plane is local Y=0.
        extent_min: [f32; 3],
        extent_max: [f32; 3],
        matrix: [f32; 16],
        inverse: [f32; 16],
    },
    Cylinder {
        /// X,Z center. The original cylinder has no Y caps.
        center: [f32; 2],
        radius: f32,
    },
}

impl VolumeGeometry {
    pub const fn classification(self) -> u8 {
        match self {
            Self::XtObb { .. } => 0x80,
            Self::Aabb { .. } => 0x81,
            Self::Sphere { .. } => 0x82,
            Self::Obb { .. } => 0x83,
            Self::Cylinder { .. } => 0x84,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VolumeError {
    /// The property classification does not select the supplied geometry.
    UnsupportedClassification,
    NumericDomain,
}

const EPSILON: f32 = 1.0e-8;
fn number(value: f32) -> Result<f32, VolumeError> {
    if value.is_finite()
        && (value == 0.0
            || ((1.0 / (1_u64 << 40) as f32)..=(1_u64 << 40) as f32).contains(&value.abs()))
    {
        Ok(value)
    } else {
        Err(VolumeError::NumericDomain)
    }
}
fn add(a: f32, b: f32) -> Result<f32, VolumeError> {
    number(number(a)? + number(b)?)
}
fn sub(a: f32, b: f32) -> Result<f32, VolumeError> {
    number(number(a)? - number(b)?)
}
fn mul(a: f32, b: f32) -> Result<f32, VolumeError> {
    number(number(a)? * number(b)?)
}
fn div(a: f32, b: f32) -> Result<f32, VolumeError> {
    if b == 0.0 {
        return Err(VolumeError::NumericDomain);
    }
    number(number(a)? / number(b)?)
}
fn sqrt(value: f32) -> Result<f32, VolumeError> {
    number(number(value)?.sqrt())
}

/// The imported *mutating* vector transform (LS3DF RVA `0x2e920`). Its
/// instruction order differs from the nonmutating operator in native_transform.
fn transform(point: [f32; 3], matrix: &[f32; 16]) -> Result<[f32; 3], VolumeError> {
    let [x, y, z] = point;
    Ok([
        add(
            add(
                add(mul(matrix[8], z)?, mul(matrix[4], y)?)?,
                mul(matrix[0], x)?,
            )?,
            matrix[12],
        )?,
        add(
            add(
                add(mul(matrix[9], z)?, mul(matrix[1], x)?)?,
                mul(matrix[5], y)?,
            )?,
            matrix[13],
        )?,
        add(
            add(
                add(mul(matrix[10], z)?, mul(matrix[2], x)?)?,
                mul(matrix[6], y)?,
            )?,
            matrix[14],
        )?,
    ])
}
/// Imported `S_vector::RotateByMatrix`, LS3DF RVA `0x2ea10`.
fn rotate(vector: [f32; 3], matrix: &[f32; 16]) -> Result<[f32; 3], VolumeError> {
    let mut out = [0.0; 3];
    for axis in 0..3 {
        out[axis] = add(
            add(
                mul(vector[0], matrix[axis])?,
                mul(vector[1], matrix[4 + axis])?,
            )?,
            mul(vector[2], matrix[8 + axis])?,
        )?;
    }
    Ok(out)
}

/// PC RVA `0x1c5b20`: entry-face tests in Y,X,Z order. This deliberately
/// preserves the original early miss when an eligible axis gives t>1.
fn box_entry(
    min: [f32; 3],
    max: [f32; 3],
    origin: [f32; 3],
    delta: [f32; 3],
    best: f32,
) -> Result<Option<(f32, [f32; 3])>, VolumeError> {
    for axis in [1, 0, 2] {
        let direction = number(delta[axis])?;
        if direction.abs() <= EPSILON {
            continue;
        }
        let plane = if direction > 0.0 {
            min[axis]
        } else {
            max[axis]
        };
        let fraction = div(sub(plane, origin[axis])?, direction)?;
        if fraction > 1.0 {
            return Ok(None);
        }
        if fraction >= best || fraction <= 0.0 {
            continue;
        }
        let other = match axis {
            1 => [0, 2],
            0 => [1, 2],
            _ => [1, 0],
        };
        let mut inside = true;
        // Both point coordinates are evaluated before the bounds tests.
        let point = [
            add(mul(fraction, delta[other[0]])?, origin[other[0]])?,
            add(mul(fraction, delta[other[1]])?, origin[other[1]])?,
        ];
        for (index, other_axis) in other.into_iter().enumerate() {
            if point[index] < number(min[other_axis])? || point[index] > number(max[other_axis])? {
                inside = false;
                break;
            }
        }
        if inside {
            let mut normal = [0.0; 3];
            normal[axis] = if direction > 0.0 { 1.0 } else { -1.0 };
            return Ok(Some((fraction, normal)));
        }
    }
    Ok(None)
}

fn local_line(query: &LineQuery, inverse: &[f32; 16]) -> Result<([f32; 3], [f32; 3]), VolumeError> {
    let mut end = [0.0; 3];
    for (axis, value) in end.iter_mut().enumerate() {
        *value = add(query.origin[axis], query.delta[axis])?;
    }
    let origin = transform(query.origin, inverse)?;
    let end = transform(end, inverse)?;
    let mut delta = [0.0; 3];
    for axis in 0..3 {
        delta[axis] = sub(end[axis], origin[axis])?;
    }
    Ok((origin, delta))
}

/// Intersect one original volume, preserving raw inward normal and strict
/// nearest-hit ties. The mask is checked before type/geometry/query inspection.
/// AABB/XTOBB/OBB reject t=0; sphere/cylinder accept t=0. Starts inside a solid
/// do not select the exit root. Only fraction, normal and identity may change.
pub fn line_in_volume(volume: &Volume, query: &mut LineQuery) -> Result<bool, VolumeError> {
    if volume.properties & query.reject_mask != 0 {
        return Ok(false);
    }
    if volume.properties as u8 != volume.geometry.classification() {
        return Err(VolumeError::UnsupportedClassification);
    }
    for value in query
        .origin
        .into_iter()
        .chain(query.delta)
        .chain([query.best_fraction])
    {
        number(value)?;
    }
    let hit = match volume.geometry {
        VolumeGeometry::Aabb { min, max } => {
            box_entry(min, max, query.origin, query.delta, query.best_fraction)?
        }
        VolumeGeometry::XtObb {
            extent_min,
            extent_max,
            matrix,
            inverse,
            ..
        } => {
            let (origin, delta) = local_line(query, &inverse)?;
            match box_entry(extent_min, extent_max, origin, delta, query.best_fraction)? {
                Some((fraction, normal)) => Some((fraction, rotate(normal, &matrix)?)),
                None => None,
            }
        }
        VolumeGeometry::Obb {
            extent_min,
            extent_max,
            matrix,
            inverse,
        } => {
            let (origin, delta) = local_line(query, &inverse)?;
            if delta[1].abs() <= EPSILON {
                return Ok(false);
            }
            let fraction = -div(origin[1], delta[1])?;
            if fraction >= query.best_fraction || fraction <= 0.0 {
                return Ok(false);
            }
            let x = add(mul(delta[0], fraction)?, origin[0])?;
            let z = add(mul(delta[2], fraction)?, origin[2])?;
            if x < number(extent_min[0])?
                || x > number(extent_max[0])?
                || z < number(extent_min[2])?
                || z > number(extent_max[2])?
            {
                return Ok(false);
            }
            let normal = [0.0, if delta[1] > 0.0 { 1.0 } else { -1.0 }, 0.0];
            Some((fraction, rotate(normal, &matrix)?))
        }
        VolumeGeometry::Sphere { center, radius } => {
            let [dx, dy, dz] = query.delta;
            let length_squared = add(add(mul(dz, dz)?, mul(dy, dy)?)?, mul(dx, dx)?)?;
            if length_squared == 0.0 {
                return Ok(false);
            }
            let x = sub(center[0], query.origin[0])?;
            let y = sub(center[1], query.origin[1])?;
            let z = sub(center[2], query.origin[2])?;
            let projection = add(add(mul(z, dz)?, mul(y, dy)?)?, mul(x, dx)?)?;
            let distance_squared = add(add(mul(z, z)?, mul(y, y)?)?, mul(x, x)?)?;
            let discriminant = sub(
                mul(projection, projection)?,
                mul(sub(distance_squared, mul(radius, radius)?)?, length_squared)?,
            )?;
            if discriminant < 0.0 {
                return Ok(false);
            }
            let fraction = div(sub(projection, sqrt(discriminant)?)?, length_squared)?;
            if fraction < 0.0 || fraction >= query.best_fraction {
                return Ok(false);
            }
            let mut normal = [0.0; 3];
            for axis in 0..3 {
                normal[axis] = sub(
                    center[axis],
                    add(mul(fraction, query.delta[axis])?, query.origin[axis])?,
                )?;
            }
            Some((fraction, normal))
        }
        VolumeGeometry::Cylinder { center, radius } => {
            let x = sub(center[0], query.origin[0])?;
            let z = sub(center[1], query.origin[2])?;
            let [dx, _, dz] = query.delta;
            let length_squared = add(mul(dz, dz)?, mul(dx, dx)?)?;
            let length = if (length_squared.to_bits() as i32) < 0x322bcc77 {
                0.0
            } else if sub(length_squared, 1.0)?.abs() < EPSILON {
                1.0
            } else {
                sqrt(length_squared)?
            };
            if length.abs() < EPSILON {
                return Ok(false);
            }
            let inverse_length = div(1.0, length)?;
            let ux = mul(dx, inverse_length)?;
            let uz = mul(dz, inverse_length)?;
            let projection = add(
                add(mul(z, uz)?, mul(ux, x)?)?,
                mul(mul(inverse_length, 0.0)?, 0.0)?,
            )?;
            let discriminant = sub(
                mul(radius, radius)?,
                sub(add(mul(z, z)?, mul(x, x)?)?, mul(projection, projection)?)?,
            )?;
            if discriminant < 0.0 {
                return Ok(false);
            }
            let fraction = mul(sub(projection, sqrt(discriminant)?)?, inverse_length)?;
            if fraction < 0.0 || fraction >= query.best_fraction {
                return Ok(false);
            }
            // Y is evaluated and written before the original overwrites it +0.
            let _discarded_y = -add(mul(fraction, query.delta[1])?, query.origin[1])?;
            let normal = [
                sub(center[0], add(mul(fraction, dx)?, query.origin[0])?)?,
                0.0,
                sub(center[1], add(mul(fraction, dz)?, query.origin[2])?)?,
            ];
            Some((fraction, normal))
        }
    };
    if let Some((fraction, normal)) = hit {
        query.best_fraction = fraction;
        query.normal = normal;
        query.primitive = Some(volume.identity);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];

    fn query(origin: [f32; 3], delta: [f32; 3]) -> LineQuery {
        LineQuery {
            origin,
            delta,
            min_xz: [-100.; 2],
            max_xz: [100.; 2],
            reject_mask: 0,
            best_fraction: 1.,
            normal: [7., 8., 9.],
            primitive: Some(99),
        }
    }

    fn volume(geometry: VolumeGeometry) -> Volume {
        Volume {
            properties: u32::from(geometry.classification()),
            identity: 12,
            geometry,
        }
    }

    #[test]
    fn all_five_record_classes_retain_original_normal_convention() {
        let min = [-1.; 3];
        let max = [1.; 3];
        let cases = [
            (
                VolumeGeometry::Aabb { min, max },
                [0., 2., 0.],
                [0., -4., 0.],
                [0., -1., 0.],
            ),
            (
                VolumeGeometry::XtObb {
                    min: [f32::NAN; 3],
                    max: [f32::NAN; 3],
                    extent_min: min,
                    extent_max: max,
                    matrix: IDENTITY,
                    inverse: IDENTITY,
                },
                [0., 2., 0.],
                [0., -4., 0.],
                [0., -1., 0.],
            ),
            (
                VolumeGeometry::Sphere {
                    center: [0.; 3],
                    radius: 1.,
                },
                [-2., 0., 0.],
                [4., 0., 0.],
                [1., 0., 0.],
            ),
            (
                VolumeGeometry::Obb {
                    extent_min: [-1., f32::NAN, -1.],
                    extent_max: [1., f32::NAN, 1.],
                    matrix: IDENTITY,
                    inverse: IDENTITY,
                },
                [0., 1., 0.],
                [0., -4., 0.],
                [0., -1., 0.],
            ),
            (
                VolumeGeometry::Cylinder {
                    center: [0.; 2],
                    radius: 1.,
                },
                [-2., 60., 0.],
                [4., 4., 0.],
                [1., 0., 0.],
            ),
        ];
        for (geometry, origin, delta, normal) in cases {
            let mut state = query(origin, delta);
            assert_eq!(line_in_volume(&volume(geometry), &mut state), Ok(true));
            assert_eq!(state.best_fraction, 0.25);
            assert_eq!(state.normal, normal);
            assert_eq!(state.primitive, Some(12));
            let saved = state;
            assert_eq!(line_in_volume(&volume(geometry), &mut state), Ok(false));
            assert_eq!(state, saved);
        }
    }

    #[test]
    fn box_corner_priority_inside_and_surface_rules() {
        let box_volume = volume(VolumeGeometry::Aabb {
            min: [-1.; 3],
            max: [1.; 3],
        });
        let mut corner = query([-2.; 3], [4.; 3]);
        assert_eq!(line_in_volume(&box_volume, &mut corner), Ok(true));
        assert_eq!(corner.normal, [0., 1., 0.]);
        for (origin, delta) in [([0.; 3], [2.; 3]), ([-1., 0., 0.], [2., 0., 0.])] {
            let mut state = query(origin, delta);
            let saved = state;
            assert_eq!(line_in_volume(&box_volume, &mut state), Ok(false));
            assert_eq!(state, saved);
        }
        for geometry in [
            VolumeGeometry::Sphere {
                center: [0.; 3],
                radius: 1.,
            },
            VolumeGeometry::Cylinder {
                center: [0.; 2],
                radius: 1.,
            },
        ] {
            let mut state = query([-1., 0., 0.], [2., 0., 0.]);
            assert_eq!(line_in_volume(&volume(geometry), &mut state), Ok(true));
            assert_eq!(state.best_fraction, 0.);
        }
    }

    #[test]
    fn masks_precede_classification_and_errors_are_transactional() {
        let mut primitive = volume(VolumeGeometry::Sphere {
            center: [f32::NAN; 3],
            radius: 1.,
        });
        primitive.properties = 0x10080;
        let mut state = query([0.; 3], [1., 0., 0.]);
        state.reject_mask = 0x10000;
        let saved = state;
        assert_eq!(line_in_volume(&primitive, &mut state), Ok(false));
        assert_eq!(state, saved);
        state.reject_mask = 0;
        let saved = state;
        assert_eq!(
            line_in_volume(&primitive, &mut state),
            Err(VolumeError::UnsupportedClassification)
        );
        assert_eq!(state, saved);
        primitive.properties = 0x82;
        assert_eq!(
            line_in_volume(&primitive, &mut state),
            Err(VolumeError::NumericDomain)
        );
        assert_eq!(state, saved);
    }
}
