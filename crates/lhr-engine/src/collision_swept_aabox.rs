//! SphereInAABox, PC RVA0x1cd040: the original selected quadrilateral faces,
//! twelve edge masks/order and nominated vertex. Radius expansion is used only
//! for the initial rejection; it is not a line-versus-inflated-box narrow phase.
use crate::{
    collision_swept::{
        add, cross, difference, div, dot, mul, scaled, sphere_in_edge, sub, translated, vertex_hit,
        SphereParam, SphereSweepError,
    },
    collision_swept_world::sphere_box_bounds_miss,
};
type Out<T> = Result<T, SphereSweepError>;

fn score(a: [f32; 3], b: [f32; 3], line: [f32; 3], direction: [f32; 3]) -> Out<f32> {
    add(
        dot(difference(a, b)?, line, [2, 1, 0])?,
        dot(cross(a, b)?, direction, [2, 1, 0])?,
    )
}

fn face(
    vertices: [[f32; 3]; 4],
    axis: usize,
    negative: bool,
    q: &mut SphereParam,
    shift: u32,
    masks: &mut u32,
) -> Out<bool> {
    let mut normal = [0.; 3];
    normal[axis] = if negative { -1. } else { 1. };
    let signed = dot(difference(q.origin, vertices[0])?, normal, [2, 1, 0])?;
    if signed <= 0. && signed > -q.radius {
        let line = cross(normal, q.origin)?;
        let mut inside = true;
        for i in 0..4 {
            if score(vertices[i], vertices[(i + 1) % 4], line, normal)? > 0. {
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
    let line = cross(q.delta, shifted)?;
    let mut face_mask = 0;
    for i in 0..4 {
        if score(vertices[i], vertices[(i + 1) % 4], line, q.delta)? > 0. {
            face_mask |= 1_u32 << (shift + i as u32);
        }
    }
    *masks |= face_mask;
    if face_mask != 0 || f64::from(q.delta[axis].abs()) < f64::from(1e-8_f32) {
        return Ok(false);
    }
    let fraction = if negative {
        mul(
            div(1., q.delta[axis])?,
            sub(add(vertices[0][axis], q.radius)?, q.origin[axis])?,
        )?
    } else {
        mul(
            div(-1., q.delta[axis])?,
            add(sub(q.radius, vertices[0][axis])?, q.origin[axis])?,
        )?
    };
    if fraction < 0. {
        return Ok(false);
    }
    let point = translated(scaled(q.delta, fraction)?, q.origin)?;
    let distance = difference(point, q.origin)?;
    let squared = dot(distance, distance, [2, 1, 0])?;
    if squared >= q.best_distance_squared {
        return Ok(false);
    }
    q.best_distance_squared = squared;
    q.point = point;
    q.normal = normal;
    Ok(true)
}

/// The helper does not write the record identity; its Nonface caller does.
pub fn sphere_in_aa_box(min: [f32; 3], max: [f32; 3], q: &mut SphereParam) -> Out<bool> {
    if sphere_box_bounds_miss(min, max, q)? {
        return Ok(false);
    }
    // Native endpoint tags, not binary corner indices.
    let v1 = min;
    let v2 = [min[0], min[1], max[2]];
    let v4 = [min[0], max[1], min[2]];
    let v8 = [max[0], min[1], min[2]];
    let v10 = [max[0], max[1], min[2]];
    let v20 = [max[0], min[1], max[2]];
    let v40 = [min[0], max[1], max[2]];
    let v80 = max;
    let mut mask = 0;
    if q.direction[0] < 0. {
        if face([v8, v10, v80, v20], 0, true, q, 4, &mut mask)? {
            return Ok(true);
        }
    } else if face([v1, v2, v40, v4], 0, false, q, 0, &mut mask)? {
        return Ok(true);
    }
    if q.direction[1] < 0. {
        if mask & 0x24 != 0 && face([v4, v40, v80, v10], 1, true, q, 8, &mut mask)? {
            return Ok(true);
        }
    } else if mask & 0x81 != 0 && face([v1, v8, v20, v2], 1, false, q, 12, &mut mask)? {
        return Ok(true);
    }
    if q.direction[2] < 0. {
        if mask & 0x4242 != 0 && face([v2, v20, v80, v40], 2, true, q, 20, &mut mask)? {
            return Ok(true);
        }
    } else if mask & 0x1818 != 0 && face([v1, v4, v10, v8], 2, false, q, 16, &mut mask)? {
        return Ok(true);
    }
    let mut vertex = 0;
    for (bits, a, b, first, last) in [
        (0x8001, v1, v2, 1, 2),
        (0x10008, v1, v4, 1, 4),
        (0x81000, v1, v8, 1, 8),
        (0x40010, v8, v10, 8, 0x10),
        (0x2080, v8, v20, 8, 0x20),
        (0x104000, v2, v20, 2, 0x20),
        (0x800002, v2, v40, 2, 0x40),
        (0x200040, v20, v80, 0x20, 0x80),
        (0x20800, v4, v10, 4, 0x10),
        (0x420, v10, v80, 0x10, 0x80),
        (0x400200, v80, v40, 0x80, 0x40),
        (0x104, v40, v4, 0x40, 4),
    ] {
        if mask & bits != 0 && sphere_in_edge(a, b, q, &mut vertex, first, last)? {
            return Ok(true);
        }
    }
    let candidate = match vertex {
        1 => v1,
        2 => v2,
        4 => v4,
        8 => v8,
        0x10 => v10,
        0x20 => v20,
        0x40 => v40,
        0x80 => v80,
        _ => return Ok(false),
    };
    if dot(difference(candidate, q.origin)?, q.direction, [2, 1, 0])? < 0. {
        return Ok(false);
    }
    vertex_hit(candidate, q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_swept::SphereSweepRequest;
    fn query(origin: [f32; 3], delta: [f32; 3]) -> SphereParam {
        SphereParam::new(&SphereSweepRequest {
            origin,
            delta,
            radius: 0.5,
            selector: -1,
            reject_mask: 0,
        })
        .unwrap()
    }
    #[test]
    fn six_box_faces_use_sphere_radius_and_native_normal() {
        for axis in [0, 1, 2] {
            for sign in [-1., 1.] {
                let mut origin = [0.; 3];
                origin[axis] = 3. * sign;
                let mut delta = [0.; 3];
                delta[axis] = -4. * sign;
                let mut q = query(origin, delta);
                assert!(sphere_in_aa_box([-1.; 3], [1.; 3], &mut q).unwrap());
                assert_eq!(q.point[axis], 1.5 * sign);
                assert_eq!(q.normal[axis], -sign);
                assert_eq!(q.best_distance_squared, 2.25);
                assert_eq!(q.primitive, None);
            }
        }
    }
    #[test]
    fn disjoint_and_fully_interior_sweeps_are_real_misses() {
        for origin in [[10., 0., 0.], [0., 0., 0.]] {
            let mut q = query(origin, [0., 0.25, 0.]);
            let old = q;
            assert!(!sphere_in_aa_box([-2.; 3], [2.; 3], &mut q).unwrap());
            assert_eq!(q, old);
        }
    }
}
