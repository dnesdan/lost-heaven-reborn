//! Original swept-sphere query state and triangle/edge workers.
//!
//! Coordinates and stored triangle planes remain in original Y-up space.
//! Arithmetic is guarded PC24/RN finite normal-or-zero arithmetic; unsupported
//! numeric behavior is a host availability error, never a fabricated miss.

use crate::{collision_line::Triangle, collision_prepare::CollisionPreparationError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereSweepRequest {
    pub origin: [f32; 3],
    pub delta: [f32; 3],
    pub radius: f32,
    pub selector: i32,
    pub reject_mask: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SphereSweepError {
    NumericDomain,
    InvalidRadius,
    InvalidGrid,
    UnsortedVertices,
    UnsortedCandidates,
    UnsupportedDynamicQuery,
    UnsupportedVolume(u8),
    Preparation(CollisionPreparationError),
}
type Out<T> = Result<T, SphereSweepError>;

/// Used fields of original tCSphereParam; identities replace record pointers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereParam {
    pub origin: [f32; 3],
    pub delta: [f32; 3],
    pub radius: f32,
    pub min_xz: [f32; 2],
    pub max_xz: [f32; 2],
    pub end: [f32; 3],
    pub direction: [f32; 3],
    pub reject_mask: u32,
    pub selector: i32,
    pub best_distance_squared: f32,
    pub side: [f32; 3],
    pub strip_min: f32,
    pub strip_max: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    pub primitive: Option<u32>,
}

pub(crate) fn number(x: f32) -> Out<f32> {
    if x == 0. || x.is_normal() {
        Ok(x)
    } else {
        Err(SphereSweepError::NumericDomain)
    }
}
fn arithmetic(x: f32, wide: f64) -> Out<f32> {
    if !x.is_finite()
        || wide.abs() > f32::MAX as f64
        || (wide != 0. && wide.abs() < f32::MIN_POSITIVE as f64)
    {
        Err(SphereSweepError::NumericDomain)
    } else {
        Ok(x)
    }
}
pub(crate) fn add(a: f32, b: f32) -> Out<f32> {
    let (a, b) = (number(a)?, number(b)?);
    let result = arithmetic(a + b, a as f64 + b as f64)?;
    crate::native_precision::record_add(a, b, result);
    Ok(result)
}
pub(crate) fn sub(a: f32, b: f32) -> Out<f32> {
    let (a, b) = (number(a)?, number(b)?);
    let result = arithmetic(a - b, a as f64 - b as f64)?;
    crate::native_precision::record_sub(a, b, result);
    Ok(result)
}
pub(crate) fn mul(a: f32, b: f32) -> Out<f32> {
    let (a, b) = (number(a)?, number(b)?);
    let result = arithmetic(a * b, a as f64 * b as f64)?;
    crate::native_precision::record_mul(a, b, result);
    Ok(result)
}
pub(crate) fn div(a: f32, b: f32) -> Out<f32> {
    let (a, b) = (number(a)?, number(b)?);
    let result = arithmetic(a / b, a as f64 / b as f64)?;
    crate::native_precision::record_div(a, b, result);
    Ok(result)
}
pub(crate) fn sqrt(x: f32) -> Out<f32> {
    let x = number(x)?;
    let result = number(x.sqrt())?;
    crate::native_precision::record_sqrt(x, result);
    Ok(result)
}
pub(crate) fn sum(v: [f32; 3], order: [usize; 3]) -> Out<f32> {
    add(add(v[order[0]], v[order[1]])?, v[order[2]])
}
pub(crate) fn dot(a: [f32; 3], b: [f32; 3], order: [usize; 3]) -> Out<f32> {
    sum(
        [mul(a[0], b[0])?, mul(a[1], b[1])?, mul(a[2], b[2])?],
        order,
    )
}
pub(crate) fn difference(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3]> {
    Ok([sub(a[0], b[0])?, sub(a[1], b[1])?, sub(a[2], b[2])?])
}
pub(crate) fn scaled(a: [f32; 3], s: f32) -> Out<[f32; 3]> {
    Ok([mul(a[0], s)?, mul(a[1], s)?, mul(a[2], s)?])
}
pub(crate) fn translated(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3]> {
    Ok([add(a[0], b[0])?, add(a[1], b[1])?, add(a[2], b[2])?])
}
pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3]> {
    Ok([
        sub(mul(a[1], b[2])?, mul(a[2], b[1])?)?,
        sub(mul(a[2], b[0])?, mul(a[0], b[2])?)?,
        sub(mul(a[0], b[1])?, mul(a[1], b[0])?)?,
    ])
}
pub(crate) fn normalized(mut v: [f32; 3], order: [usize; 3]) -> Out<[f32; 3]> {
    let length = dot(v, v, order)?;
    if f64::from(sub(length, 1.)?.abs()) >= f64::from(1e-8_f32) {
        if length >= 1e-8_f32 {
            let inverse = div(1., sqrt(length)?)?;
            v = scaled(v, inverse)?;
        } else {
            let axis = if v[0] != 0. {
                0
            } else if v[2] != 0. {
                2
            } else {
                1
            };
            v[axis] = if v[axis] < 0. { -1. } else { 1. };
        }
    }
    Ok(v)
}

impl SphereParam {
    pub fn new(request: &SphereSweepRequest) -> Out<Self> {
        for value in request
            .origin
            .into_iter()
            .chain(request.delta)
            .chain([request.radius])
        {
            number(value)?;
        }
        if request.radius < 0. {
            return Err(SphereSweepError::InvalidRadius);
        }
        let end = translated(request.origin, request.delta)?;
        let direction = normalized(request.delta, [0, 2, 1])?;
        let mut min_xz = [0.; 2];
        let mut max_xz = [0.; 2];
        for (i, axis) in [0, 2].into_iter().enumerate() {
            min_xz[i] = sub(
                if end[axis] <= request.origin[axis] {
                    end[axis]
                } else {
                    request.origin[axis]
                },
                request.radius,
            )?;
            max_xz[i] = add(
                request.radius,
                if request.origin[axis] <= end[axis] {
                    end[axis]
                } else {
                    request.origin[axis]
                },
            )?;
        }
        let (side, strip_min, strip_max) = if request.reject_mask != u32::MAX {
            let mut side = [
                sub(direction[2], mul(direction[1], 0.)?)?,
                sub(mul(direction[0], 0.)?, mul(direction[2], 0.)?)?,
                sub(mul(direction[1], 0.)?, direction[0])?,
            ];
            if f64::from(dot(side, side, [0, 2, 1])?) < 0.001_f64 {
                side = [1., 0., 0.];
            }
            side = normalized(side, [0, 2, 1])?;
            let extent = scaled(side, request.radius)?;
            let plus = translated(extent, request.origin)?;
            let minus = difference(request.origin, extent)?;
            let first = -dot(side, plus, [0, 2, 1])?;
            let second = -dot(side, minus, [0, 2, 1])?;
            let (strip_min, strip_max) = if second < first {
                (second, first)
            } else {
                (first, second)
            };
            (side, strip_min, strip_max)
        } else {
            ([0.; 3], 0., 0.)
        };
        Ok(Self {
            origin: request.origin,
            delta: request.delta,
            radius: request.radius,
            min_xz,
            max_xz,
            end,
            direction,
            reject_mask: request.reject_mask | 0x20,
            selector: request.selector,
            best_distance_squared: dot(request.delta, request.delta, [2, 1, 0])?,
            side,
            strip_min,
            strip_max,
            point: [0.; 3],
            normal: [0.; 3],
            primitive: None,
        })
    }
}

/// SphereInEdge. Vertex tags retain the native end-rejection communication
/// with SphereInFace; a rejected edge can nominate one endpoint for testing.
pub fn sphere_in_edge(
    a: [f32; 3],
    b: [f32; 3],
    q: &mut SphereParam,
    vertex: &mut u32,
    first: u32,
    last: u32,
) -> Out<bool> {
    let relative = difference(a, q.origin)?;
    let edge = difference(b, a)?;
    let perpendicular = cross(edge, q.direction)?;
    let coefficient = dot(perpendicular, perpendicular, [2, 1, 0])?;
    if f64::from(coefficient.abs()) <= f64::from(1e-8_f32) {
        return Ok(false);
    }
    let triple = dot(edge, cross(q.direction, relative)?, [2, 1, 0])?;
    let edge_squared = dot(edge, edge, [2, 1, 0])?;
    let discriminant = mul(
        sub(
            mul(mul(coefficient, q.radius)?, q.radius)?,
            mul(triple, triple)?,
        )?,
        edge_squared,
    )?;
    if discriminant < 0. {
        return Ok(false);
    }
    let projected = dot(perpendicular, cross(edge, relative)?, [2, 1, 0])?;
    let root = sqrt(discriminant)?;
    if projected < -root {
        return Ok(false);
    }
    let distance = div(sub(projected, root)?, coefficient)?;
    if distance < -q.radius {
        return Ok(false);
    }
    let relative_projection = dot(edge, relative, [2, 1, 0])?;
    let along = div(
        sub(
            mul(dot(edge, q.direction, [2, 1, 0])?, distance)?,
            relative_projection,
        )?,
        edge_squared,
    )?;
    if along < 0. {
        *vertex = first;
        return Ok(false);
    }
    if along > 1. {
        *vertex = last;
        return Ok(false);
    }
    let closest = translated(scaled(edge, along)?, a)?;
    let center = translated(scaled(q.direction, distance)?, q.origin)?;
    let normal = difference(closest, center)?;
    if distance >= 0. {
        let squared = mul(distance, distance)?;
        if squared >= q.best_distance_squared {
            return Ok(false);
        }
        q.best_distance_squared = squared;
        q.point = center;
        q.normal = normal;
        return Ok(true);
    }
    if relative_projection > 0. {
        *vertex = first;
        return Ok(false);
    }
    if dot(difference(b, q.origin)?, edge, [2, 1, 0])? < 0. {
        *vertex = last;
        return Ok(false);
    }
    if -dot(normal, q.direction, [2, 1, 0])? <= distance {
        q.best_distance_squared = -1.;
        q.normal = normal;
        q.point = center;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn vertex_hit(vertex: [f32; 3], q: &mut SphereParam) -> Out<bool> {
    let relative = difference(vertex, q.origin)?;
    let projected = dot(relative, q.direction, [2, 1, 0])?;
    let discriminant = sub(
        mul(q.radius, q.radius)?,
        sub(
            dot(relative, relative, [2, 1, 0])?,
            mul(projected, projected)?,
        )?,
    )?;
    if discriminant < 0. {
        return Ok(false);
    }
    let distance = sub(projected, sqrt(discriminant)?)?;
    let squared = mul(distance, distance)?;
    if squared >= q.best_distance_squared {
        return Ok(false);
    }
    q.point = translated(scaled(q.direction, distance)?, q.origin)?;
    q.normal = difference(vertex, q.point)?;
    q.best_distance_squared = squared;
    Ok(true)
}

/// SphereInFace, including initial overlap, edge and nominated-vertex paths.
/// The stored plane participates directly; no normal or corner repair occurs.
pub fn sphere_in_face(t: &Triangle, q: &mut SphereParam) -> Out<bool> {
    let [a, b, c] = t.vertices;
    let normal = [t.plane[0], t.plane[1], t.plane[2]];
    if dot(q.direction, normal, [2, 1, 0])? < 0. {
        return Ok(false);
    }
    let offset_plane = -dot(difference(a, scaled(normal, q.radius)?)?, normal, [2, 1, 0])?;
    let signed = add(dot(q.origin, normal, [1, 2, 0])?, t.plane[3])?;
    if signed > 0. {
        return Ok(false);
    }
    let reversed = t.properties & 0x100 != 0;
    if signed > -q.radius {
        let pluecker = cross(normal, q.origin)?;
        let mut inside = true;
        for (first, last) in [(a, b), (b, c), (c, a)] {
            let edge = difference(first, last)?;
            let product = cross(first, last)?;
            let score = add(
                dot(edge, pluecker, [0, 2, 1])?,
                dot(product, normal, [2, 1, 0])?,
            )?;
            if if reversed { score < 0. } else { score > 0. } {
                inside = false;
                break;
            }
        }
        if inside {
            let push = sub(-signed, q.radius)?;
            q.point = translated(scaled(normal, push)?, q.origin)?;
            q.normal = normal;
            q.best_distance_squared = 0.;
            q.primitive = Some(t.identity);
            return Ok(true);
        }
    }
    let shifted = translated(scaled(normal, q.radius)?, q.origin)?;
    let pluecker = cross(q.delta, shifted)?;
    let mut mask = if reversed { 7 } else { 0 };
    for (bit, (first, last)) in [1, 2, 4].into_iter().zip([(a, b), (b, c), (c, a)]) {
        let product = cross(first, last)?;
        let edge = difference(first, last)?;
        let score = add(
            dot(pluecker, edge, [2, 1, 0])?,
            dot(product, q.delta, [2, 1, 0])?,
        )?;
        if score > 0. {
            mask ^= bit;
        }
    }
    let denominator = dot(normal, q.delta, [2, 1, 0])?;
    if f64::from(denominator.abs()) < f64::from(1e-8_f32) {
        return Ok(false);
    }
    let fraction = -div(
        add(dot(normal, q.origin, [2, 1, 0])?, offset_plane)?,
        denominator,
    )?;
    if mask == 0 {
        if fraction < 0. {
            return Ok(false);
        }
        let point = translated(scaled(q.delta, fraction)?, q.origin)?;
        let distance = difference(point, q.origin)?;
        // PC0x5ccba8..5ccbc2 sums the constructed X/Y/Z components in
        // that order; the imported general vector dot uses Z/Y/X instead.
        let squared = dot(distance, distance, [0, 1, 2])?;
        if squared >= q.best_distance_squared {
            return Ok(false);
        }
        q.point = point;
        q.normal = normal;
        q.best_distance_squared = squared;
        q.primitive = Some(t.identity);
        return Ok(true);
    }
    let mut vertex = 0;
    for (bit, first, last, first_tag, last_tag) in
        [(4, c, a, 1, 2), (2, b, c, 4, 1), (1, a, b, 2, 4)]
    {
        if mask & bit != 0 && sphere_in_edge(first, last, q, &mut vertex, first_tag, last_tag)? {
            q.primitive = Some(t.identity);
            return Ok(true);
        }
    }
    let candidate = match vertex {
        1 => c,
        2 => a,
        4 => b,
        _ => return Ok(false),
    };
    if vertex_hit(candidate, q)? {
        q.primitive = Some(t.identity);
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn triangle() -> Triangle {
        Triangle {
            properties: 3,
            vertices: [[-2., 0., -2.], [-2., 0., 2.], [2., 0., -2.]],
            plane: [0., -1., 0., 0.],
            identity: 7,
        }
    }
    fn query(origin: [f32; 3], delta: [f32; 3], radius: f32) -> SphereParam {
        SphereParam::new(&SphereSweepRequest {
            origin,
            delta,
            radius,
            selector: -1,
            reject_mask: 0,
        })
        .unwrap()
    }
    #[test]
    fn query_preparation_preserves_exact_precision_and_retains_inexact_normalization() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            let q = query([0.; 3], [0., -4., 0.], 0.5);
            assert_eq!(q.direction, [0., -1., 0.]);
        });
        assert!(!precision.precision_raised());
        precision.run(|| {
            let q = query([0.; 3], [1., -1., 0.], 0.5);
            assert!(q.direction[0] > 0.7 && q.direction[0] < 0.71);
        });
        assert!(precision.precision_raised());
        precision.run(|| {
            query([0.; 3], [0., -4., 0.], 0.5);
        });
        assert!(precision.precision_raised());
    }
    #[test]
    fn finite_radius_contacts_face_before_the_center_crosses_its_plane() {
        let mut q = query([-0.5, 2., -0.5], [0., -4., 0.], 0.5);
        assert!(sphere_in_face(&triangle(), &mut q).unwrap());
        assert_eq!(q.point, [-0.5, 0.5, -0.5]);
        assert_eq!(q.normal, [0., -1., 0.]);
        assert_eq!(q.best_distance_squared, 2.25);
        assert_eq!(q.primitive, Some(7));
    }
    #[test]
    fn sloped_face_distance_retains_native_xyz_squared_sum() {
        let mut q = query(
            [-1.2315513, 3.6753778, 0.19731414],
            [1.3333334, -3.7333336, 0.5333333],
            1.,
        );
        q.direction = [0.33333334, -0.9333334, 0.13333333];
        q.best_distance_squared = 16.;
        let face = Triangle {
            properties: 3,
            vertices: [[-2., 1.2, -1.6], [-2., -1.2, 1.6], [2., 1.2, -1.6]],
            plane: [0., -0.8, -0.6, 0.],
            identity: 7,
        };
        assert!(sphere_in_face(&face, &mut q).unwrap());
        assert_eq!(q.best_distance_squared.to_bits(), 0x4118_9351);
        assert_eq!(q.normal, [0., -0.8, -0.6]);
    }
    #[test]
    fn edge_contact_occurs_when_the_center_line_misses_the_triangle() {
        let mut q = query([-2.4, 2., 0.], [0., -4., 0.], 0.5);
        assert!(sphere_in_face(&triangle(), &mut q).unwrap());
        assert!(q.point[1] > 0.29 && q.point[1] < 0.31);
        assert_eq!(q.point[0], -2.4);
        assert!(q.normal[0] > 0.39 && q.normal[0] < 0.41);
        assert_eq!(q.primitive, Some(7));
    }
    #[test]
    fn negative_edge_contact_keeps_resolved_center_and_native_negative_best() {
        let mut q = query([-3., 0., 0.], [0., -4., 0.], 2.);
        assert!(sphere_in_face(&triangle(), &mut q).unwrap());
        assert_eq!(q.best_distance_squared, -1.);
        assert_eq!(q.point[0], -3.);
        assert!(q.point[1] > 1.73 && q.point[1] < 1.74);
        assert_eq!(q.normal[0], 1.);
    }
    #[test]
    fn endpoint_and_backface_misses_preserve_prior_contact_fields() {
        for (origin, delta) in [
            ([-0.5, 2., -0.5], [0., -1.5, 0.]),
            ([-0.5, 2., -0.5], [0., 4., 0.]),
        ] {
            let mut q = query(origin, delta, 0.5);
            q.point = [17., 18., 19.];
            q.normal = [21., 22., 23.];
            let old = q;
            assert!(!sphere_in_face(&triangle(), &mut q).unwrap());
            assert_eq!(q, old);
        }
    }
    #[test]
    fn initial_face_overlap_projects_the_sphere_outward_without_a_ray_hit() {
        let mut q = query([-0.5, 0.25, -0.5], [0., -4., 0.], 0.5);
        assert!(sphere_in_face(&triangle(), &mut q).unwrap());
        assert_eq!(q.point, [-0.5, 0.5, -0.5]);
        assert_eq!(q.best_distance_squared, 0.);
    }
}
