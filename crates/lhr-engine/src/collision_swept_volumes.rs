//! Transformed SphereInNonface branches, PC RVA0x1d15a0.
//! XTOBB uses the original local AABox; OBB is a two-sided local Y=0
//! rectangle. Neither shape is regenerated from rendered mesh faces.
use crate::{
    collision_swept::{
        add, cross, difference, div, dot, mul, normalized, scaled, sphere_in_edge, sub, translated,
        vertex_hit, SphereParam, SphereSweepError,
    },
    collision_swept_aabox::sphere_in_aa_box,
    collision_swept_world::sphere_box_bounds_miss,
    collision_volumes::{Volume, VolumeGeometry},
};
type Out<T> = Result<T, SphereSweepError>;

// Imported mutating S_vector::operator*=, LS3DF RVA0x2e920. Its original
// association differs from the nonmutating point-transform export.
fn transform(v: [f32; 3], m: &[f32; 16]) -> Out<[f32; 3]> {
    let [x, y, z] = v;
    Ok([
        add(
            add(add(mul(m[8], z)?, mul(m[4], y)?)?, mul(m[0], x)?)?,
            m[12],
        )?,
        add(
            add(add(mul(m[9], z)?, mul(m[1], x)?)?, mul(m[5], y)?)?,
            m[13],
        )?,
        add(
            add(add(mul(m[10], z)?, mul(m[2], x)?)?, mul(m[6], y)?)?,
            m[14],
        )?,
    ])
}
fn rotate(v: [f32; 3], m: &[f32; 16]) -> Out<[f32; 3]> {
    let mut out = [0.; 3];
    for (i, x) in out.iter_mut().enumerate() {
        *x = add(
            add(mul(v[0], m[i])?, mul(v[1], m[4 + i])?)?,
            mul(v[2], m[8 + i])?,
        )?;
    }
    Ok(out)
}

fn rectangle(min: [f32; 3], max: [f32; 3], q: &mut SphereParam) -> Out<bool> {
    if sphere_box_bounds_miss(min, max, q)? {
        return Ok(false);
    }
    let a = [min[0], 0., min[2]];
    let b = [max[0], 0., min[2]];
    let c = [max[0], 0., max[2]];
    let d = [min[0], 0., max[2]];
    let mut mask = 0;
    if f64::from(q.delta[1].abs()) >= f64::from(1e-8_f32) {
        let normal = [0., if q.direction[1] < 0. { -1. } else { 1. }, 0.];
        let signed = mul(q.origin[1], normal[1])?;
        if signed <= 0. && signed > -q.radius {
            let quad = if normal[1] < 0. {
                [a, d, c, b]
            } else {
                [a, b, c, d]
            };
            let line = cross(normal, q.origin)?;
            let mut inside = true;
            for i in 0..4 {
                let first = quad[i];
                let last = quad[(i + 1) % 4];
                let score = add(
                    dot(difference(first, last)?, line, [2, 1, 0])?,
                    dot(cross(first, last)?, normal, [2, 1, 0])?,
                )?;
                if score > 0. {
                    inside = false;
                    break;
                }
            }
            if inside {
                q.best_distance_squared = 0.;
                q.point = translated(scaled(normal, sub(-signed, q.radius)?)?, q.origin)?;
                q.normal = normal;
                return Ok(true);
            }
        }
        let shifted = translated(scaled(normal, q.radius)?, q.origin)?;
        let inverse = div(1., q.direction[1])?;
        let x = sub(shifted[0], mul(mul(inverse, shifted[1])?, q.direction[0])?)?;
        let z = sub(shifted[2], mul(mul(inverse, shifted[1])?, q.direction[2])?)?;
        if x < min[0] {
            mask |= 8;
        }
        if x > max[0] {
            mask |= 2;
        }
        if z < min[2] {
            mask |= 1;
        }
        if z > max[2] {
            mask |= 4;
        }
        if mask == 0 {
            let fraction = mul(
                div(-1., q.delta[1])?,
                add(mul(normal[1], q.radius)?, q.origin[1])?,
            )?;
            if fraction >= 0. {
                let point = translated(scaled(q.delta, fraction)?, q.origin)?;
                let distance = difference(point, q.origin)?;
                let squared = dot(distance, distance, [1, 2, 0])?;
                if squared < q.best_distance_squared {
                    q.best_distance_squared = squared;
                    q.point = point;
                    q.normal = normal;
                    return Ok(true);
                }
            }
        }
    } else if q.origin[1].abs() < q.radius {
        if q.delta[0] > 0. {
            mask |= 1;
        }
        if q.delta[2] < 0. {
            mask |= 2;
        }
        if q.delta[0] < 0. {
            mask |= 4;
        }
        if q.delta[2] > 0. {
            mask |= 8;
        }
    }
    let mut vertex = 0;
    for (bit, first, last, first_tag, last_tag) in [
        (1, a, b, 1, 2),
        (2, b, c, 2, 4),
        (4, c, d, 4, 8),
        (8, d, a, 8, 1),
    ] {
        if mask & bit != 0 && sphere_in_edge(first, last, q, &mut vertex, first_tag, last_tag)? {
            return Ok(true);
        }
    }
    let candidate = match vertex {
        1 => a,
        2 => b,
        4 => c,
        8 => d,
        _ => return Ok(false),
    };
    vertex_hit(candidate, q)
}

pub fn sphere_in_transformed_nonface(volume: &Volume, q: &mut SphereParam) -> Out<bool> {
    let (min, max, matrix, inverse, box_shape) = match volume.geometry {
        VolumeGeometry::XtObb {
            extent_min,
            extent_max,
            matrix,
            inverse,
            ..
        } => (extent_min, extent_max, matrix, inverse, true),
        VolumeGeometry::Obb {
            extent_min,
            extent_max,
            matrix,
            inverse,
        } => (extent_min, extent_max, matrix, inverse, false),
        _ => {
            return Err(SphereSweepError::UnsupportedVolume(
                volume.geometry.classification(),
            ))
        }
    };
    let mut local = *q;
    local.origin = transform(q.origin, &inverse)?;
    local.end = transform(q.end, &inverse)?;
    local.delta = difference(local.end, local.origin)?;
    local.direction = normalized(local.delta, [2, 1, 0])?;
    let hit = if box_shape {
        sphere_in_aa_box(min, max, &mut local)?
    } else {
        rectangle(min, max, &mut local)?
    };
    if !hit {
        return Ok(false);
    }
    let point = transform(local.point, &matrix)?;
    let normal = rotate(local.normal, &matrix)?;
    q.point = point;
    q.normal = normal;
    q.best_distance_squared = local.best_distance_squared;
    q.primitive = Some(volume.identity);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{collision_swept::SphereSweepRequest, native_transform::SMatrix};
    #[test]
    fn translated_obb_remains_a_two_sided_rectangle() {
        let mut matrix = SMatrix::identity().elements;
        matrix[13] = 3.;
        let mut inverse = SMatrix::identity().elements;
        inverse[13] = -3.;
        let volume = Volume {
            properties: 0x83,
            identity: 17,
            geometry: VolumeGeometry::Obb {
                extent_min: [-2., 0., -2.],
                extent_max: [2., 0., 2.],
                matrix,
                inverse,
            },
        };
        for (origin, delta, height, normal) in [
            ([0., 5., 0.], [0., -4., 0.], 3.5, -1.),
            ([0., 1., 0.], [0., 4., 0.], 2.5, 1.),
        ] {
            let mut q = SphereParam::new(&SphereSweepRequest {
                origin,
                delta,
                radius: 0.5,
                selector: -1,
                reject_mask: 0,
            })
            .unwrap();
            assert!(sphere_in_transformed_nonface(&volume, &mut q).unwrap());
            assert_eq!(q.point[1], height);
            assert_eq!(q.normal[1], normal);
            assert_eq!(q.primitive, Some(17));
        }
    }
    #[test]
    fn xtobb_uses_local_extent_and_ignores_stored_world_bounds() {
        let matrix = SMatrix::identity().elements;
        let volume = Volume {
            properties: 0x80,
            identity: 18,
            geometry: VolumeGeometry::XtObb {
                min: [f32::NAN; 3],
                max: [f32::NAN; 3],
                extent_min: [-1.; 3],
                extent_max: [1.; 3],
                matrix,
                inverse: matrix,
            },
        };
        let mut q = SphereParam::new(&SphereSweepRequest {
            origin: [0., 3., 0.],
            delta: [0., -4., 0.],
            radius: 0.5,
            selector: -1,
            reject_mask: 0,
        })
        .unwrap();
        assert!(sphere_in_transformed_nonface(&volume, &mut q).unwrap());
        assert_eq!(q.point, [0., 1.5, 0.]);
    }
}
