//! Ordered static triangle/volume composition of the original collision workers.
//!
//! Candidate order and triangle corners are caller-supplied original order.
//! This adapter neither sorts nor repairs geometry. Missing preparation remains
//! an explicit diagnostic; masks and the original triangle early break precede
//! later candidates. It does not construct a scene or dynamic collision world.

use crate::collision_line::{line_in_tree_level_triangles, LineQuery, QueryError, Triangle};
use crate::collision_prepare::{
    CollisionPreparationEntry, CollisionPreparationError, PreparedCollisionPrimitive,
};
use crate::collision_query::{
    test_line_v_grid, CollisionLineHit, CollisionLineOutputs, CollisionLineRequest,
    CollisionQueryError, CollisionTraversalState, StaticGridGeometry, StaticQueryGrid,
};
use crate::collision_volumes::{line_in_volume, Volume, VolumeError, VolumeGeometry};
use lhr_assets::klz::{PrimitiveData, PrimitiveKind};

#[derive(Clone, Copy, Debug)]
pub enum CollisionPrimitive {
    Triangle(Triangle),
    Volume(Volume),
    /// Retains an original record whose geometry/ownership is unavailable.
    Unavailable {
        properties: u32,
        identity: u32,
        error: CollisionPreparationError,
    },
}

impl CollisionPrimitive {
    pub fn properties(&self) -> u32 {
        match self {
            Self::Triangle(triangle) => triangle.properties,
            Self::Volume(volume) => volume.properties,
            Self::Unavailable { properties, .. } => *properties,
        }
    }
}

/// Preserve one preparation outcome and the original stored primitive fields.
/// The caller supplies a stable record tag, never a native pointer. A bound
/// volume's owner remains in the preparation audit; this static query only uses
/// its geometry. Failed owner bindings remain unavailable, not assumed hits or
/// misses. No new native loader effects are performed by this field projection.
pub fn primitive_from_prepared(
    entry: &CollisionPreparationEntry<'_>,
    identity: u32,
) -> Result<CollisionPrimitive, VolumeError> {
    Ok(match entry.outcome {
        Err(error) => CollisionPrimitive::Unavailable {
            properties: entry.source.properties().word(),
            identity,
            error,
        },
        Ok(PreparedCollisionPrimitive::Triangle(triangle)) => {
            if entry.source.kind() != PrimitiveKind::Triangle {
                return Err(VolumeError::UnsupportedClassification);
            }
            CollisionPrimitive::Triangle(triangle.line_triangle(identity))
        }
        Ok(PreparedCollisionPrimitive::Volume(_)) => {
            let geometry = match entry.source.data() {
                PrimitiveData::Aabb { min, max, .. } => VolumeGeometry::Aabb { min, max },
                PrimitiveData::XtObb {
                    min,
                    max,
                    extent_min,
                    extent_max,
                    matrix,
                    inverse,
                    ..
                } => VolumeGeometry::XtObb {
                    min,
                    max,
                    extent_min,
                    extent_max,
                    matrix,
                    inverse,
                },
                PrimitiveData::Sphere { center, radius, .. } => {
                    VolumeGeometry::Sphere { center, radius }
                }
                PrimitiveData::Obb {
                    extent_min,
                    extent_max,
                    matrix,
                    inverse,
                    ..
                } => VolumeGeometry::Obb {
                    extent_min,
                    extent_max,
                    matrix,
                    inverse,
                },
                PrimitiveData::Cylinder { center, radius, .. } => {
                    VolumeGeometry::Cylinder { center, radius }
                }
                PrimitiveData::Triangle { .. } => {
                    return Err(VolumeError::UnsupportedClassification);
                }
            };
            CollisionPrimitive::Volume(Volume {
                properties: entry.source.properties().word(),
                identity,
                geometry,
            })
        }
    })
}

/// Compose original ordered leaf dispatch. Only fraction, normal and primitive
/// may change. Errors leave the whole query unchanged, which is host policy.
/// Reached triangles retain the prior ascending-X admission requirement; an
/// original early break never visits subsequent volume or unavailable records.
pub fn line_in_tree_level(
    candidates: &[CollisionPrimitive],
    query: &mut LineQuery,
) -> Result<bool, CollisionQueryError> {
    let mut result = *query;
    let mut hit = false;
    let mut previous_min = None;
    for candidate in candidates {
        if candidate.properties() & result.reject_mask != 0 {
            continue;
        }
        match candidate {
            CollisionPrimitive::Triangle(triangle) => {
                let minimum = triangle.vertices[0][0];
                if previous_min.is_some_and(|x| x > minimum) {
                    return Err(CollisionQueryError::Triangle(
                        QueryError::UnsortedCandidates,
                    ));
                }
                previous_min = Some(minimum);
                // These guards precede the original plane/projection arithmetic.
                if triangle.vertices[2][0] < result.min_xz[0] {
                    continue;
                }
                if minimum > result.max_xz[0] {
                    break;
                }
                hit |= line_in_tree_level_triangles(std::slice::from_ref(triangle), &mut result)
                    .map_err(CollisionQueryError::Triangle)?;
            }
            CollisionPrimitive::Volume(volume) => {
                hit |= line_in_volume(volume, &mut result).map_err(CollisionQueryError::Volume)?;
            }
            CollisionPrimitive::Unavailable { error, .. } => {
                return Err(CollisionQueryError::Preparation(*error));
            }
        }
    }
    *query = result;
    Ok(hit)
}

#[derive(Clone, Copy, Debug)]
pub struct MixedCollisionQueryCell<'a> {
    pub max_y: f32,
    pub candidates: &'a [CollisionPrimitive],
    pub identity: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct MixedCollisionQueryGrid<'a> {
    pub origin_xz: [f32; 2],
    pub cell_size_xz: [f32; 2],
    pub width: u32,
    pub height: u32,
    pub cells: &'a [MixedCollisionQueryCell<'a>],
    pub dynamic_candidates_present: bool,
}

impl StaticQueryGrid for MixedCollisionQueryGrid<'_> {
    fn geometry(&self) -> StaticGridGeometry {
        StaticGridGeometry {
            origin_xz: self.origin_xz,
            cell_size_xz: self.cell_size_xz,
            width: self.width,
            height: self.height,
            dynamic_candidates_present: self.dynamic_candidates_present,
        }
    }
    fn cell_count(&self) -> usize {
        self.cells.len()
    }
    fn cell(&self, index: usize) -> (f32, u32) {
        (self.cells[index].max_y, self.cells[index].identity)
    }
    fn query_cell(&self, index: usize, query: &mut LineQuery) -> Result<(), CollisionQueryError> {
        line_in_tree_level(self.cells[index].candidates, query).map(|_| ())
    }
}

/// Original static grid traversal with mixed candidates. Dynamic dispatch
/// remains unavailable. The old triangle-only API retains its earlier scope.
pub fn test_line_v_mixed(
    grid: Option<&MixedCollisionQueryGrid<'_>>,
    request: &CollisionLineRequest,
    outputs: &mut CollisionLineOutputs,
    traversal: &mut CollisionTraversalState,
) -> Result<Option<CollisionLineHit>, CollisionQueryError> {
    test_line_v_grid(
        grid.map(|g| g as &dyn StaticQueryGrid),
        request,
        outputs,
        traversal,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_world::CollisionBindingError;

    fn triangle(identity: u32) -> CollisionPrimitive {
        CollisionPrimitive::Triangle(Triangle {
            properties: 3,
            vertices: [[0., 0., 0.], [0., 0., 2.], [2., 0., 0.]],
            plane: [0., -1., 0., 0.],
            identity,
        })
    }
    fn sphere(identity: u32) -> CollisionPrimitive {
        CollisionPrimitive::Volume(Volume {
            properties: 0x82,
            identity,
            geometry: VolumeGeometry::Sphere {
                center: [0.5, -1., 0.5],
                radius: 1.,
            },
        })
    }
    fn unavailable(properties: u32) -> CollisionPrimitive {
        CollisionPrimitive::Unavailable {
            properties,
            identity: 99,
            error: CollisionPreparationError::Binding(CollisionBindingError::MissingFrame {
                link_index: 4,
            }),
        }
    }
    fn request() -> CollisionLineRequest {
        CollisionLineRequest {
            origin: [0.5, 1., 0.5],
            delta: [0., -2., 0.],
            selector: -1,
            reject_mask: 0,
        }
    }
    fn grid<'a>(cells: &'a [MixedCollisionQueryCell<'a>]) -> MixedCollisionQueryGrid<'a> {
        MixedCollisionQueryGrid {
            origin_xz: [0.; 2],
            cell_size_xz: [4.; 2],
            width: cells.len() as u32,
            height: 1,
            cells,
            dynamic_candidates_present: false,
        }
    }
    #[test]
    fn equal_triangle_volume_hits_preserve_candidate_order_and_outputs() {
        for candidates in [[triangle(1), sphere(2)], [sphere(2), triangle(1)]] {
            let cells = [MixedCollisionQueryCell {
                max_y: 1.,
                candidates: &candidates,
                identity: 12,
            }];
            let mut outputs = CollisionLineOutputs {
                point: Some([9.; 3]),
                normal: Some([9.; 3]),
            };
            let mut traversal = CollisionTraversalState::default();
            let hit = test_line_v_mixed(
                Some(&grid(&cells)),
                &request(),
                &mut outputs,
                &mut traversal,
            )
            .unwrap()
            .unwrap();
            let identity = match candidates[0] {
                CollisionPrimitive::Triangle(t) => t.identity,
                CollisionPrimitive::Volume(v) => v.identity,
                _ => unreachable!(),
            };
            assert_eq!(hit.primitive, identity);
            assert_eq!(hit.fraction, 0.5);
            assert_eq!(outputs.point, Some([0.5, 0., 0.5]));
            assert_eq!(outputs.normal.unwrap()[1], 1.);
            assert_eq!(traversal.last_cell.unwrap().identity, 12);
        }
    }
    #[test]
    fn unresolved_eligible_record_rolls_back_earlier_hit_but_masked_one_is_skipped() {
        for properties in [0, 0x20] {
            let candidates = [triangle(1), unavailable(properties)];
            let cells = [MixedCollisionQueryCell {
                max_y: 1.,
                candidates: &candidates,
                identity: 12,
            }];
            let mut outputs = CollisionLineOutputs {
                point: Some([9.; 3]),
                normal: None,
            };
            let original = outputs;
            let mut traversal = CollisionTraversalState::default();
            let result = test_line_v_mixed(
                Some(&grid(&cells)),
                &request(),
                &mut outputs,
                &mut traversal,
            );
            if properties == 0 {
                assert!(matches!(result, Err(CollisionQueryError::Preparation(_))));
                assert_eq!(outputs, original);
                assert_eq!(traversal, CollisionTraversalState::default());
            } else {
                assert_eq!(result.unwrap().unwrap().primitive, 1);
            }
        }
    }
    #[test]
    fn original_early_break_and_cell_y_cull_do_not_visit_unavailable_candidates() {
        let CollisionPrimitive::Triangle(mut beyond) = triangle(1) else {
            unreachable!()
        };
        for vertex in &mut beyond.vertices {
            vertex[0] += 10.;
        }
        let candidates = [CollisionPrimitive::Triangle(beyond), unavailable(0)];
        for max_y in [1., -5.] {
            let cells = [MixedCollisionQueryCell {
                max_y,
                candidates: &candidates,
                identity: 12,
            }];
            let mut outputs = CollisionLineOutputs::default();
            let mut traversal = CollisionTraversalState::default();
            assert_eq!(
                test_line_v_mixed(
                    Some(&grid(&cells)),
                    &request(),
                    &mut outputs,
                    &mut traversal
                )
                .unwrap(),
                None
            );
            assert_eq!(traversal.last_cell.unwrap().identity, 12);
        }
    }
}
