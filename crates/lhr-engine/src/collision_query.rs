//! Bounded static-grid projection of PC `g_collision::_TestLineV` (RVA `0x1c6b10`).
//!
//! Despite its name, the PC body accepts a general segment displacement. Cells
//! are visited in row-major Z/X order, preserving each candidate list verbatim.
//! The triangle leaf's ascending-X and classification prerequisites still apply;
//! loading/preparing collision data does not establish those prerequisites.
//!
//! Arithmetic has the same CW `0x007f`, precision-24, round-to-nearest and
//! zero-or-`2^-40..=2^40` operand/intermediate contract as `collision_line`.
//! The host floating-point environment is neither inspected nor changed.
//! Conversion to grid coordinates additionally requires signed-32-bit results.
//! Unsupported dynamic dispatch and eligible volume records fail explicitly.
//! Host validation errors leave both outputs and traversal state unchanged.

use crate::collision_line::{line_in_tree_level_triangles, LineQuery, QueryError, Triangle};

/// A cell's original ordered candidate references and its stored upper Y bound.
#[derive(Clone, Copy, Debug)]
pub struct CollisionQueryCell<'a> {
    pub max_y: f32,
    pub triangles: &'a [Triangle],
    /// Stable host tag replacing the original cell pointer.
    pub identity: u32,
}

/// Fields used by the original grid walk; cells are indexed `z * width + x`.
#[derive(Clone, Copy, Debug)]
pub struct CollisionQueryGrid<'a> {
    pub origin_xz: [f32; 2],
    pub cell_size_xz: [f32; 2],
    pub width: u32,
    pub height: u32,
    pub cells: &'a [CollisionQueryCell<'a>],
    /// Corresponds to the non-null dynamic-query dependency at owner `+0x28c`.
    pub dynamic_candidates_present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionLineRequest {
    pub origin: [f32; 3],
    pub delta: [f32; 3],
    /// `-1` bypasses the original dynamic candidate worker.
    pub selector: i32,
    pub reject_mask: u32,
}

/// `None` corresponds to a null optional output pointer. Misses preserve values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionLineOutputs {
    pub point: Option<[f32; 3]>,
    pub normal: Option<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollisionCellVisit {
    pub identity: u32,
    pub x: u32,
    pub z: u32,
}

/// The original records the last visited cell even if its Y bound culls it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CollisionTraversalState {
    pub last_cell: Option<CollisionCellVisit>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionLineHit {
    /// The original return value is a record pointer, represented by this tag.
    pub primitive: u32,
    pub fraction: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionQueryError {
    InvalidGrid,
    NumericDomain,
    UnsupportedDynamicQuery,
    Triangle(QueryError),
    Volume(crate::collision_volumes::VolumeError),
    Preparation(crate::collision_prepare::CollisionPreparationError),
}

/// Shared host view of the original grid fields. This is not a native interface.
#[derive(Clone, Copy)]
pub(crate) struct StaticGridGeometry {
    pub origin_xz: [f32; 2],
    pub cell_size_xz: [f32; 2],
    pub width: u32,
    pub height: u32,
    pub dynamic_candidates_present: bool,
}

pub(crate) trait StaticQueryGrid {
    fn geometry(&self) -> StaticGridGeometry;
    fn cell_count(&self) -> usize;
    fn cell(&self, index: usize) -> (f32, u32);
    fn query_cell(&self, index: usize, query: &mut LineQuery) -> Result<(), CollisionQueryError>;
}

impl StaticQueryGrid for CollisionQueryGrid<'_> {
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
        line_in_tree_level_triangles(self.cells[index].triangles, query)
            .map(|_| ())
            .map_err(CollisionQueryError::Triangle)
    }
}

fn number(value: f32) -> Result<f32, CollisionQueryError> {
    if value.is_finite()
        && (value == 0.0
            || ((1.0 / (1_u64 << 40) as f32)..=(1_u64 << 40) as f32).contains(&value.abs()))
    {
        Ok(value)
    } else {
        Err(CollisionQueryError::NumericDomain)
    }
}
fn add(a: f32, b: f32) -> Result<f32, CollisionQueryError> {
    number(a + b)
}
fn sub(a: f32, b: f32) -> Result<f32, CollisionQueryError> {
    number(a - b)
}
fn mul(a: f32, b: f32) -> Result<f32, CollisionQueryError> {
    number(a * b)
}
fn reciprocal(value: f32) -> Result<f32, CollisionQueryError> {
    number(1.0 / value)
}

fn grid_integer(value: f32) -> Result<i32, CollisionQueryError> {
    // The original conversion helper temporarily selects truncation. The
    // admitted range avoids its wider-integer/low-word overflow behavior.
    if (-2147483648.0..2147483648.0).contains(&value) {
        Ok(value as i32)
    } else {
        Err(CollisionQueryError::NumericDomain)
    }
}

fn outward_normal(stored: [f32; 3]) -> Result<[f32; 3], CollisionQueryError> {
    let mut normal = stored.map(|value| -value);
    let [x, y, z] = normal;
    // This caller uses X,Y,Z summation, unlike the leaf's dot product.
    let length_squared = add(add(mul(x, x)?, mul(y, y)?)?, mul(z, z)?)?;
    if f64::from(sub(length_squared, 1.0)?.abs()) < f64::from(1.0e-8_f32) {
        return Ok(normal);
    }
    if length_squared < 1.0e-8_f32 {
        // Native degeneracy fallback changes only one component, X then Z
        // then Y. In particular an all-zero plane becomes [ -0, +1, -0 ].
        let axis = if x != 0.0 {
            0
        } else if z != 0.0 {
            2
        } else {
            1
        };
        normal[axis] = if normal[axis] < 0.0 { -1.0 } else { 1.0 };
    } else {
        let scale = reciprocal(number(length_squared.sqrt())?)?;
        for value in &mut normal {
            *value = mul(*value, scale)?;
        }
    }
    Ok(normal)
}

/// Execute the original static grid walk and its optional output conversion.
///
/// A missing grid returns a miss before reading the request. The original
/// always adds `0x20` to the reject mask and starts at strict fraction `1`.
/// Equal-distance hits retain the first candidate in traversal order. Dynamic
/// dispatch is required only when both its dependency is present and selector
/// differs from `-1`; that path is explicitly outside this implementation.
pub fn test_line_v(
    grid: Option<&CollisionQueryGrid<'_>>,
    request: &CollisionLineRequest,
    outputs: &mut CollisionLineOutputs,
    traversal: &mut CollisionTraversalState,
) -> Result<Option<CollisionLineHit>, CollisionQueryError> {
    test_line_v_grid(
        grid.map(|grid| grid as &dyn StaticQueryGrid),
        request,
        outputs,
        traversal,
    )
}

pub(crate) fn test_line_v_grid(
    grid: Option<&dyn StaticQueryGrid>,
    request: &CollisionLineRequest,
    outputs: &mut CollisionLineOutputs,
    traversal: &mut CollisionTraversalState,
) -> Result<Option<CollisionLineHit>, CollisionQueryError> {
    let Some(grid) = grid else {
        return Ok(None);
    };
    let geometry = grid.geometry();
    for value in request
        .origin
        .into_iter()
        .chain(request.delta)
        .chain(geometry.origin_xz)
        .chain(geometry.cell_size_xz)
    {
        number(value)?;
    }
    if geometry.width > i32::MAX as u32
        || geometry.height > i32::MAX as u32
        || geometry.cell_size_xz.iter().any(|&size| size <= 0.0)
        || (geometry.width as usize).checked_mul(geometry.height as usize)
            != Some(grid.cell_count())
    {
        return Err(CollisionQueryError::InvalidGrid);
    }
    let mut end = [0.0; 3];
    for (axis, value) in end.iter_mut().enumerate() {
        *value = add(request.origin[axis], request.delta[axis])?;
    }
    let min_xz = [
        if request.origin[0] < end[0] {
            request.origin[0]
        } else {
            end[0]
        },
        if request.origin[2] < end[2] {
            request.origin[2]
        } else {
            end[2]
        },
    ];
    let max_xz = [
        if request.origin[0] > end[0] {
            request.origin[0]
        } else {
            end[0]
        },
        if request.origin[2] > end[2] {
            request.origin[2]
        } else {
            end[2]
        },
    ];
    let min_y = if request.origin[1] < end[1] {
        request.origin[1]
    } else {
        end[1]
    };
    // This value is initialized even when no normal output was requested.
    let [dx, dy, dz] = request.delta;
    number(add(add(mul(dz, dz)?, mul(dy, dy)?)?, mul(dx, dx)?)?)?;
    if request.selector != -1 && geometry.dynamic_candidates_present {
        return Err(CollisionQueryError::UnsupportedDynamicQuery);
    }
    let mut start = [0; 2];
    let mut stop = [0; 2];
    for axis in 0..2 {
        let inverse = reciprocal(geometry.cell_size_xz[axis])?;
        start[axis] =
            grid_integer(mul(sub(min_xz[axis], geometry.origin_xz[axis])?, inverse)?)?.max(0);
        stop[axis] = grid_integer(add(
            mul(sub(max_xz[axis], geometry.origin_xz[axis])?, inverse)?,
            1.0,
        )?)?
        .min([geometry.width, geometry.height][axis] as i32);
    }
    let mut query = LineQuery {
        origin: request.origin,
        delta: request.delta,
        min_xz,
        max_xz,
        reject_mask: request.reject_mask | 0x20,
        best_fraction: 1.0,
        normal: [0.0; 3],
        primitive: None,
    };
    let mut next_traversal = *traversal;
    for z in start[1]..stop[1] {
        for x in start[0]..stop[0] {
            let index = z as usize * geometry.width as usize + x as usize;
            let (max_y, identity) = grid.cell(index);
            if number(max_y)? >= min_y {
                grid.query_cell(index, &mut query)?;
            }
            next_traversal.last_cell = Some(CollisionCellVisit {
                identity,
                x: x as u32,
                z: z as u32,
            });
        }
    }
    let mut next_outputs = *outputs;
    let result = if let Some(primitive) = query.primitive {
        if next_outputs.point.is_some() {
            let mut point = [0.0; 3];
            for (axis, value) in point.iter_mut().enumerate() {
                *value = add(
                    mul(query.best_fraction, request.delta[axis])?,
                    request.origin[axis],
                )?;
            }
            next_outputs.point = Some(point);
        }
        if next_outputs.normal.is_some() {
            next_outputs.normal = Some(outward_normal(query.normal)?);
        }
        Some(CollisionLineHit {
            primitive,
            fraction: query.best_fraction,
        })
    } else {
        None
    };
    *outputs = next_outputs;
    *traversal = next_traversal;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn triangle(identity: u32) -> Triangle {
        Triangle {
            properties: 3,
            vertices: [[0., 0., 0.], [0., 0., 2.], [2., 0., 0.]],
            plane: [0., -2., 0., 0.],
            identity,
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
    fn grid<'a>(cells: &'a [CollisionQueryCell<'a>]) -> CollisionQueryGrid<'a> {
        CollisionQueryGrid {
            origin_xz: [0., 0.],
            cell_size_xz: [4., 4.],
            width: cells.len() as u32,
            height: 1,
            cells,
            dynamic_candidates_present: false,
        }
    }
    #[test]
    fn hit_uses_stored_plane_and_optional_normal_conversion() {
        let candidates = [triangle(7), triangle(8)];
        let cells = [CollisionQueryCell {
            max_y: 1.,
            triangles: &candidates,
            identity: 42,
        }];
        let mut out = CollisionLineOutputs {
            point: Some([9.; 3]),
            normal: Some([9.; 3]),
        };
        let mut state = CollisionTraversalState::default();
        let hit = test_line_v(Some(&grid(&cells)), &request(), &mut out, &mut state)
            .unwrap()
            .unwrap();
        assert_eq!(
            hit,
            CollisionLineHit {
                primitive: 7,
                fraction: 0.5
            }
        );
        assert_eq!(out.point, Some([0.5, 0., 0.5]));
        assert_eq!(out.normal, Some([-0., 1., -0.]));
        assert_eq!(
            state.last_cell,
            Some(CollisionCellVisit {
                identity: 42,
                x: 0,
                z: 0
            })
        );
    }
    #[test]
    fn forced_disabled_mask_and_height_cull_preserve_outputs() {
        let mut disabled = triangle(7);
        disabled.properties = 0x20;
        let records = [disabled];
        let cells = [
            CollisionQueryCell {
                max_y: 1.,
                triangles: &records,
                identity: 1,
            },
            CollisionQueryCell {
                max_y: -4.,
                triangles: &[Triangle {
                    properties: 0x80,
                    ..triangle(9)
                }],
                identity: 2,
            },
        ];
        let mut req = request();
        req.delta[0] = 4.;
        let mut out = CollisionLineOutputs {
            point: Some([9.; 3]),
            normal: Some([8.; 3]),
        };
        let before = out;
        let mut state = CollisionTraversalState::default();
        assert_eq!(
            test_line_v(Some(&grid(&cells)), &req, &mut out, &mut state),
            Ok(None)
        );
        assert_eq!(out, before);
        assert_eq!(state.last_cell.unwrap().identity, 2);
    }
    #[test]
    fn missing_grid_short_circuits_invalid_request() {
        let mut req = request();
        req.origin[0] = f32::NAN;
        assert_eq!(
            test_line_v(
                None,
                &req,
                &mut CollisionLineOutputs::default(),
                &mut CollisionTraversalState::default()
            ),
            Ok(None)
        );
    }
    #[test]
    fn unresolved_dependencies_are_transactional_errors() {
        let records = [Triangle {
            properties: 0x80,
            ..triangle(9)
        }];
        let cells = [CollisionQueryCell {
            max_y: 1.,
            triangles: &records,
            identity: 2,
        }];
        let mut out = CollisionLineOutputs {
            point: Some([9.; 3]),
            normal: Some([8.; 3]),
        };
        let before = out;
        let mut state = CollisionTraversalState::default();
        assert_eq!(
            test_line_v(Some(&grid(&cells)), &request(), &mut out, &mut state),
            Err(CollisionQueryError::Triangle(QueryError::UnsupportedVolume))
        );
        assert_eq!(out, before);
        assert_eq!(state, CollisionTraversalState::default());
        let mut dynamic = grid(&cells);
        dynamic.dynamic_candidates_present = true;
        let mut req = request();
        req.selector = 0;
        assert_eq!(
            test_line_v(Some(&dynamic), &req, &mut out, &mut state),
            Err(CollisionQueryError::UnsupportedDynamicQuery)
        );
    }
    #[test]
    fn normal_fallback_preserves_other_components_and_axis_order() {
        assert_eq!(outward_normal([0., 0., 0.]).unwrap(), [-0., 1., -0.]);
        assert_eq!(
            outward_normal([0.000001, 0.000002, 0.000003]).unwrap(),
            [-1., -0.000002, -0.000003]
        );
        assert_eq!(
            outward_normal([0., 0.000002, -0.000003]).unwrap(),
            [-0., -0.000002, 1.]
        );
    }
    #[test]
    fn conversion_truncates_toward_zero_before_clamping() {
        assert_eq!(grid_integer(-0.75), Ok(0));
        assert_eq!(grid_integer(-1.75), Ok(-1));
        assert_eq!(
            grid_integer(2147483648.),
            Err(CollisionQueryError::NumericDomain)
        );
    }
}
