//! Original static SphereInTreeLevel / _TestSphereB composition.
//!
//! KLZ candidate order and source planes are preserved. Missing geometry,
//! unsupported reached narrow phases and dynamic dispatch remain typed errors.

use crate::{
    collision_dispatch::CollisionPrimitive,
    collision_query::{CollisionCellVisit, CollisionLineOutputs},
    collision_swept::{
        add, difference, div, dot, mul, normalized, number, scaled, sphere_in_face, sqrt, sub,
        translated, SphereParam, SphereSweepError, SphereSweepRequest,
    },
    collision_volumes::{Volume, VolumeGeometry},
    collision_world_driving::DrivingCollisionWorld,
};
type Out<T> = Result<T, SphereSweepError>;

#[derive(Clone, Copy, Debug)]
pub struct SphereGridGeometry {
    pub origin_xz: [f32; 2],
    pub cell_size_xz: [f32; 2],
    pub width: u32,
    pub height: u32,
    pub dynamic_candidates_present: bool,
}
pub trait SphereStaticGrid {
    fn sphere_geometry(&self) -> SphereGridGeometry;
    fn sphere_cell_count(&self) -> usize;
    fn sphere_cell(&self, index: usize) -> (f32, u32, &[CollisionPrimitive]);
}
impl SphereStaticGrid for DrivingCollisionWorld {
    fn sphere_geometry(&self) -> SphereGridGeometry {
        let g = self.query_geometry();
        SphereGridGeometry {
            origin_xz: g.origin_xz,
            cell_size_xz: g.cell_size_xz,
            width: g.width,
            height: g.height,
            dynamic_candidates_present: g.dynamic_candidates_present,
        }
    }
    fn sphere_cell_count(&self) -> usize {
        let g = self.query_geometry();
        g.width as usize * g.height as usize
    }
    fn sphere_cell(&self, index: usize) -> (f32, u32, &[CollisionPrimitive]) {
        self.query_cell_candidates(index)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SphereTraversalState {
    pub last_cell: Option<CollisionCellVisit>,
    /// Original global at PC0x648240; misses preserve its previous value.
    pub last_distance_squared: Option<f32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereSweepHit {
    pub primitive: u32,
    pub distance_squared: f32,
}

/// The exact first AABox rejection, before any face/edge/vertex narrow phase.
pub fn sphere_box_bounds_miss(min: [f32; 3], max: [f32; 3], q: &SphereParam) -> Out<bool> {
    let end = translated(q.origin, q.delta)?;
    let expanded_min = difference(min, [q.radius; 3])?;
    let expanded_max = translated([q.radius; 3], max)?;
    for axis in [0, 2, 1] {
        let (lo, hi) = if q.origin[axis] < end[axis] {
            (q.origin[axis], end[axis])
        } else {
            (end[axis], q.origin[axis])
        };
        if hi < expanded_min[axis] || expanded_max[axis] < lo {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Original nonface shape dispatch, including local XTOBB and OBB queries.
pub fn sphere_in_nonface(volume: &Volume, q: &mut SphereParam) -> Out<bool> {
    if volume.properties as u8 != volume.geometry.classification() {
        return Err(SphereSweepError::UnsupportedVolume(volume.properties as u8));
    }
    match volume.geometry {
        VolumeGeometry::Sphere { center, radius } => {
            let relative = difference(center, q.origin)?;
            let projected = dot(relative, q.direction, [2, 1, 0])?;
            if projected <= 0. {
                return Ok(false);
            }
            let combined = add(radius, q.radius)?;
            let discriminant = sub(
                mul(combined, combined)?,
                sub(
                    dot(relative, relative, [2, 1, 0])?,
                    mul(projected, projected)?,
                )?,
            )?;
            if discriminant < 0. {
                return Ok(false);
            }
            let distance = sub(projected, sqrt(discriminant)?)?;
            if distance < -q.radius {
                return Ok(false);
            }
            let squared = mul(distance, distance)?;
            if distance >= 0. && squared >= q.best_distance_squared {
                return Ok(false);
            }
            q.point = translated(scaled(q.direction, distance)?, q.origin)?;
            q.normal = difference(center, q.point)?;
            q.best_distance_squared = squared;
            q.primitive = Some(volume.identity);
            Ok(true)
        }
        VolumeGeometry::Cylinder { center, radius } => {
            let relative = [sub(center[0], q.origin[0])?, sub(center[1], q.origin[2])?];
            let [dx, _, dz] = q.direction;
            let squared = add(mul(dx, dx)?, mul(dz, dz)?)?;
            let length = if (squared.to_bits() as i32) < 0x322bcc77 {
                0.
            } else if f64::from(sub(squared, 1.)?.abs()) < f64::from(1e-8_f32) {
                1.
            } else {
                sqrt(squared)?
            };
            if f64::from(length.abs()) < f64::from(1e-8_f32) {
                return Ok(false);
            }
            let inverse = div(1., length)?;
            let projected = add(
                add(
                    mul(mul(inverse, 0.)?, 0.)?,
                    mul(mul(dx, inverse)?, relative[0])?,
                )?,
                mul(mul(relative[1], dz)?, inverse)?,
            )?;
            if projected <= 0. {
                return Ok(false);
            }
            let combined = add(radius, q.radius)?;
            let discriminant = sub(
                mul(combined, combined)?,
                sub(
                    add(
                        mul(relative[0], relative[0])?,
                        mul(relative[1], relative[1])?,
                    )?,
                    mul(projected, projected)?,
                )?,
            )?;
            if discriminant <= 0. {
                return Ok(false);
            }
            let distance = mul(sub(projected, sqrt(discriminant)?)?, inverse)?;
            if distance < -q.radius {
                return Ok(false);
            }
            let squared = mul(distance, distance)?;
            if distance >= 0. && squared >= q.best_distance_squared {
                return Ok(false);
            }
            q.point = translated(scaled(q.direction, distance)?, q.origin)?;
            q.normal = [sub(center[0], q.point[0])?, 0., sub(center[1], q.point[2])?];
            q.best_distance_squared = squared;
            q.primitive = Some(volume.identity);
            Ok(true)
        }
        VolumeGeometry::Aabb { min, max } => {
            let hit = crate::collision_swept_aabox::sphere_in_aa_box(min, max, q)?;
            if hit {
                q.primitive = Some(volume.identity);
            }
            Ok(hit)
        }
        VolumeGeometry::XtObb { .. } | VolumeGeometry::Obb { .. } => {
            crate::collision_swept_volumes::sphere_in_transformed_nonface(volume, q)
        }
    }
}

pub fn sphere_in_tree_level(candidates: &[CollisionPrimitive], q: &mut SphereParam) -> Out<bool> {
    let mut hit = false;
    let mut previous = None;
    for candidate in candidates {
        if candidate.properties() & q.reject_mask != 0 {
            continue;
        }
        match candidate {
            CollisionPrimitive::Unavailable { error, .. } => {
                return Err(SphereSweepError::Preparation(*error));
            }
            CollisionPrimitive::Volume(volume) => {
                hit |= sphere_in_nonface(volume, q)?;
            }
            CollisionPrimitive::Triangle(triangle) => {
                if triangle.properties & 0xa0 != 0 {
                    return Err(SphereSweepError::UnsupportedVolume(
                        triangle.properties as u8,
                    ));
                }
                let [a, b, c] = triangle.vertices;
                if previous.is_some_and(|x| x > a[0]) {
                    return Err(SphereSweepError::UnsortedCandidates);
                }
                previous = Some(a[0]);
                if c[0] < q.min_xz[0] {
                    continue;
                }
                if a[0] > q.max_xz[0] {
                    break;
                }
                if a[0] > b[0] || b[0] > c[0] {
                    return Err(SphereSweepError::UnsortedVertices);
                }
                let mut lo = a[2];
                let mut hi = c[2];
                if lo > hi {
                    std::mem::swap(&mut lo, &mut hi);
                }
                if b[2] < lo {
                    lo = b[2];
                } else if b[2] > hi {
                    hi = b[2];
                }
                if lo <= q.max_xz[1] && hi >= q.min_xz[1] {
                    hit |= sphere_in_face(triangle, q)?;
                }
            }
        }
    }
    Ok(hit)
}

fn integer(value: f32) -> Out<i32> {
    if (-2147483648.0..2147483648.0).contains(&value) {
        let result = value as i32;
        // Actual __ftol at PC0x6109d0 changes RC, performs FISTP qword,
        // then restores CW without clearing sticky precision status.
        crate::native_precision::record_precision(result as f32 != value);
        Ok(result)
    } else {
        Err(SphereSweepError::NumericDomain)
    }
}

/// Original static wrapper. A null world is a miss before reading request
/// fields. Host errors leave outputs and traversal unchanged; misses preserve
/// optional output values, while successful cell visits remain observable.
pub fn test_sphere_b(
    grid: Option<&dyn SphereStaticGrid>,
    request: &SphereSweepRequest,
    outputs: &mut CollisionLineOutputs,
    traversal: &mut SphereTraversalState,
) -> Out<Option<SphereSweepHit>> {
    let Some(grid) = grid else { return Ok(None) };
    let g = grid.sphere_geometry();
    if g.width > i32::MAX as u32
        || g.height > i32::MAX as u32
        || (g.width as usize).checked_mul(g.height as usize) != Some(grid.sphere_cell_count())
        || g.cell_size_xz.iter().any(|x| !x.is_finite() || *x <= 0.)
    {
        return Err(SphereSweepError::InvalidGrid);
    }
    let query = SphereParam::new(request)?;
    if request.selector != -1 && g.dynamic_candidates_present {
        return Err(SphereSweepError::UnsupportedDynamicQuery);
    }
    test_sphere_b_continue(grid, request, outputs, traversal, query, false)
}
/// Continue the same original query after canonical dynamic traversal. The
/// dynamic return flag stays distinct from its retained nearest descriptor.
pub(crate) fn test_sphere_b_continue(
    grid: &dyn SphereStaticGrid,
    request: &SphereSweepRequest,
    outputs: &mut CollisionLineOutputs,
    traversal: &mut SphereTraversalState,
    mut query: SphereParam,
    mut hit: bool,
) -> Out<Option<SphereSweepHit>> {
    let g = grid.sphere_geometry();
    let mut next = *traversal;
    if request.reject_mask != u32::MAX {
        let min_y = sub(
            if query.end[1] <= query.origin[1] {
                query.end[1]
            } else {
                query.origin[1]
            },
            query.radius,
        )?;
        let mut start = [0; 2];
        let mut stop = [0; 2];
        for axis in [0, 1] {
            let inverse = div(1., g.cell_size_xz[axis])?;
            start[axis] =
                integer(mul(sub(query.min_xz[axis], g.origin_xz[axis])?, inverse)?)?.max(0);
            stop[axis] = integer(add(
                mul(sub(query.max_xz[axis], g.origin_xz[axis])?, inverse)?,
                1.,
            )?)?
            .min([g.width, g.height][axis] as i32);
        }
        for z in start[1]..stop[1] {
            for x in start[0]..stop[0] {
                let index = z as usize * g.width as usize + x as usize;
                let (max_y, identity, candidates) = grid.sphere_cell(index);
                if min_y <= number(max_y)? {
                    hit |= sphere_in_tree_level(candidates, &mut query)?;
                }
                next.last_cell = Some(CollisionCellVisit {
                    identity,
                    x: x as u32,
                    z: z as u32,
                });
            }
        }
    }
    let result = if hit {
        next.last_distance_squared = Some(query.best_distance_squared);
        let normal = normalized(query.normal, [0, 1, 2])?;
        let mut values = *outputs;
        if values.point.is_some() {
            values.point = Some(query.point);
        }
        if values.normal.is_some() {
            values.normal = Some(normal.map(|x| -x));
        }
        *outputs = values;
        Some(SphereSweepHit {
            primitive: query.primitive.expect("native hit carries a primitive"),
            distance_squared: query.best_distance_squared,
        })
    } else {
        None
    };
    *traversal = next;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_line::Triangle;
    struct Grid {
        candidates: Vec<CollisionPrimitive>,
        dynamic: bool,
    }
    impl SphereStaticGrid for Grid {
        fn sphere_geometry(&self) -> SphereGridGeometry {
            SphereGridGeometry {
                origin_xz: [-10.; 2],
                cell_size_xz: [20.; 2],
                width: 1,
                height: 1,
                dynamic_candidates_present: self.dynamic,
            }
        }
        fn sphere_cell_count(&self) -> usize {
            1
        }
        fn sphere_cell(&self, _: usize) -> (f32, u32, &[CollisionPrimitive]) {
            (10., 99, &self.candidates)
        }
    }
    fn request() -> SphereSweepRequest {
        SphereSweepRequest {
            origin: [-0.5, 2., -0.5],
            delta: [0., -4., 0.],
            radius: 0.5,
            selector: -1,
            reject_mask: 0,
        }
    }
    fn outputs() -> CollisionLineOutputs {
        CollisionLineOutputs {
            point: Some([17., 18., 19.]),
            normal: Some([21., 22., 23.]),
        }
    }
    fn triangle(identity: u32) -> CollisionPrimitive {
        CollisionPrimitive::Triangle(Triangle {
            properties: 3,
            vertices: [[-2., 0., -2.], [-2., 0., 2.], [2., 0., -2.]],
            plane: [0., -1., 0., 0.],
            identity,
        })
    }
    #[test]
    fn static_grid_returns_original_record_center_normal_and_squared_distance() {
        let grid = Grid {
            candidates: vec![triangle(7), triangle(8)],
            dynamic: false,
        };
        let mut out = outputs();
        let mut state = SphereTraversalState::default();
        let hit = test_sphere_b(Some(&grid), &request(), &mut out, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(
            hit,
            SphereSweepHit {
                primitive: 7,
                distance_squared: 2.25
            }
        );
        assert_eq!(out.point, Some([-0.5, 0.5, -0.5]));
        assert_eq!(out.normal, Some([-0., 1., -0.]));
        assert_eq!(state.last_distance_squared, Some(2.25));
        assert_eq!(
            state.last_cell,
            Some(CollisionCellVisit {
                identity: 99,
                x: 0,
                z: 0
            })
        );
    }
    #[test]
    fn null_world_and_static_mask_bypass_preserve_outputs_and_last_distance() {
        let grid = Grid {
            candidates: vec![triangle(7)],
            dynamic: false,
        };
        let mut out = outputs();
        let initial = out;
        let mut state = SphereTraversalState {
            last_distance_squared: Some(9.),
            ..Default::default()
        };
        let mut q = request();
        q.origin = [f32::NAN; 3];
        assert_eq!(test_sphere_b(None, &q, &mut out, &mut state), Ok(None));
        q = request();
        q.reject_mask = u32::MAX;
        assert_eq!(
            test_sphere_b(Some(&grid), &q, &mut out, &mut state),
            Ok(None)
        );
        assert_eq!(out, initial);
        assert_eq!(state.last_distance_squared, Some(9.));
        assert_eq!(state.last_cell, None);
    }
    #[test]
    fn fractional_grid_conversion_retains_precision_after_exact_request_arithmetic() {
        struct UnitGrid;
        impl SphereStaticGrid for UnitGrid {
            fn sphere_geometry(&self) -> SphereGridGeometry {
                SphereGridGeometry {
                    origin_xz: [0.; 2],
                    cell_size_xz: [1.; 2],
                    width: 1,
                    height: 1,
                    dynamic_candidates_present: false,
                }
            }
            fn sphere_cell_count(&self) -> usize {
                1
            }
            fn sphere_cell(&self, _: usize) -> (f32, u32, &[CollisionPrimitive]) {
                (-100., 1, &[])
            }
        }
        for coordinate in [0., 0.25] {
            let mut precision = crate::native_precision::Pc24PrecisionState::default();
            let q = SphereSweepRequest {
                origin: [coordinate, 2., coordinate],
                delta: [0., -4., 0.],
                radius: 0.,
                selector: -1,
                reject_mask: 0,
            };
            precision.run(|| {
                assert_eq!(
                    test_sphere_b(
                        Some(&UnitGrid),
                        &q,
                        &mut outputs(),
                        &mut SphereTraversalState::default(),
                    ),
                    Ok(None)
                );
            });
            assert_eq!(precision.precision_raised(), coordinate != 0.);
        }
    }
    #[test]
    fn dynamic_and_reached_preparation_gaps_fail_transactionally() {
        let mut grid = Grid {
            candidates: vec![triangle(7)],
            dynamic: true,
        };
        let mut q = request();
        q.selector = 4;
        let mut out = outputs();
        let initial = out;
        let mut state = SphereTraversalState::default();
        assert_eq!(
            test_sphere_b(Some(&grid), &q, &mut out, &mut state),
            Err(SphereSweepError::UnsupportedDynamicQuery)
        );
        grid.dynamic = false;
        grid.candidates.push(CollisionPrimitive::Unavailable {
            properties: 3,
            identity: 8,
            error: crate::collision_prepare::CollisionPreparationError::NumericDomain,
        });
        q.selector = -1;
        assert_eq!(
            test_sphere_b(Some(&grid), &q, &mut out, &mut state),
            Err(SphereSweepError::Preparation(
                crate::collision_prepare::CollisionPreparationError::NumericDomain
            ))
        );
        assert_eq!(out, initial);
        assert_eq!(state, SphereTraversalState::default());
    }
    #[test]
    fn static_sphere_and_infinite_cylinder_use_sphere_sweeps() {
        for (geometry, origin, delta, expected) in [
            (
                VolumeGeometry::Sphere {
                    center: [0.; 3],
                    radius: 1.,
                },
                [0., 3., 0.],
                [0., -4., 0.],
                [0., 1.5, 0.],
            ),
            (
                VolumeGeometry::Cylinder {
                    center: [0.; 2],
                    radius: 1.,
                },
                [3., 0., 0.],
                [-4., 0., 0.],
                [1.5, 0., 0.],
            ),
        ] {
            let grid = Grid {
                candidates: vec![CollisionPrimitive::Volume(Volume {
                    properties: geometry.classification() as u32,
                    identity: 12,
                    geometry,
                })],
                dynamic: false,
            };
            let mut q = request();
            q.origin = origin;
            q.delta = delta;
            let mut out = outputs();
            let hit = test_sphere_b(
                Some(&grid),
                &q,
                &mut out,
                &mut SphereTraversalState::default(),
            )
            .unwrap()
            .unwrap();
            assert_eq!(hit.primitive, 12);
            assert_eq!(hit.distance_squared, 2.25);
            assert_eq!(out.point, Some(expected));
        }
    }
}
