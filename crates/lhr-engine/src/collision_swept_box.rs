//! Original g_collision::_TestDBox and its BoxInDynamic/BoxInTreeLevel/
//! BoxInNonFace dependencies. The original predicates, including their
//! different tangent rules and squared extent fields, are retained literally.
use crate::{
    collision_dispatch::CollisionPrimitive,
    collision_prepare::CollisionPreparationError,
    collision_swept::{add, difference, div, dot, mul, scaled, sub, translated, SphereSweepError},
    collision_volumes::{Volume, VolumeGeometry},
    native_transform::SMatrix,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSweepError {
    NumericDomain,
    InvalidGrid,
    UnsortedVertices,
    UnsortedCandidates,
    GeometryMismatch,
    UnsupportedDynamicQuery,
    Preparation(CollisionPreparationError),
}
impl From<SphereSweepError> for BoxSweepError {
    fn from(_: SphereSweepError) -> Self {
        Self::NumericDomain
    }
}
type Out<T> = Result<T, BoxSweepError>;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxSweepRequest {
    pub bounds: [[f32; 3]; 2],
    pub first_matrix: SMatrix,
    pub second_matrix: SMatrix,
    /// The three vector arguments in original call order. Only vector 2
    /// contributes to the returned predicate; the first two feed discarded
    /// assertion/debug arithmetic. Floating-point status flags are not modeled.
    pub vectors: [[f32; 3]; 3],
    pub dynamic_mask: u32,
    /// UINT_MAX suppresses the static pass. Other values do not filter leaves.
    pub static_mask: u32,
}
/// Used tCBoxParam fields. Names describe the actual stored arithmetic, rather
/// than reinterpreting the original squared values as geometric radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxParam {
    pub world_min: [f32; 3],
    pub world_max: [f32; 3],
    pub center: [f32; 3],
    pub extended_center: [f32; 3],
    pub half_diagonal_squared: f32,
    pub extended_diagonal_squared: f32,
    pub dynamic_mask: u32,
    pub static_mask: u32,
}
fn number(value: f32) -> Out<f32> {
    if value == 0. || value.is_normal() {
        Ok(value)
    } else {
        Err(BoxSweepError::NumericDomain)
    }
}
// Imported mutating vector transform, LS3DF RVA0x2e920. Its order differs
// from the nonmutating S_matrix::try_transform_point used for the center.
fn transform_mutating(point: [f32; 3], m: &SMatrix) -> Out<[f32; 3]> {
    let a = &m.elements;
    let [x, y, z] = point;
    Ok([
        add(
            add(add(mul(a[8], z)?, mul(a[4], y)?)?, mul(a[0], x)?)?,
            a[12],
        )?,
        add(
            add(add(mul(a[9], z)?, mul(a[1], x)?)?, mul(a[5], y)?)?,
            a[13],
        )?,
        add(
            add(add(mul(a[10], z)?, mul(a[2], x)?)?, mul(a[6], y)?)?,
            a[14],
        )?,
    ])
}
impl BoxParam {
    pub fn new(request: &BoxSweepRequest) -> Out<Self> {
        let corner = |x: usize, y: usize, z: usize| {
            [
                request.bounds[x][0],
                request.bounds[y][1],
                request.bounds[z][2],
            ]
        };
        let mut first = true;
        let mut world_min = [0.; 3];
        let mut world_max = [0.; 3];
        let mut last = [0.; 3];
        for matrix in [&request.first_matrix, &request.second_matrix] {
            for x in [1, 0] {
                for y in [1, 0] {
                    for z in [0, 1] {
                        let point = transform_mutating(corner(x, y, z), matrix)?;
                        if first {
                            world_min = point;
                            world_max = point;
                            first = false;
                        } else {
                            for axis in 0..3 {
                                if point[axis] < world_min[axis] {
                                    world_min[axis] = point[axis];
                                }
                                if world_max[axis] < point[axis] {
                                    world_max[axis] = point[axis];
                                }
                            }
                        }
                        last = point;
                    }
                }
            }
        }
        let mut local = [0.; 3];
        for (axis, value) in local.iter_mut().enumerate() {
            *value = mul(add(request.bounds[1][axis], request.bounds[0][axis])?, 0.5)?;
        }
        let center = request
            .second_matrix
            .try_transform_point(local)
            .map_err(|_| BoxSweepError::NumericDomain)?;
        let diagonal = difference(center, last)?;
        let half_diagonal_squared = dot(diagonal, diagonal, [0, 2, 1])?;
        let extended_center = translated(center, scaled(request.vectors[2], 20.)?)?;
        let extended_diagonal = difference(extended_center, last)?;
        let extended_diagonal_squared = dot(extended_diagonal, extended_diagonal, [2, 1, 0])?;
        Ok(Self {
            world_min,
            world_max,
            center,
            extended_center,
            half_diagonal_squared,
            extended_diagonal_squared,
            dynamic_mask: request.dynamic_mask,
            static_mask: request.static_mask,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxGridGeometry {
    pub origin_xz: [f32; 2],
    pub cell_size_xz: [f32; 2],
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Copy)]
pub struct BoxCellView<'a> {
    pub max_y: f32,
    pub candidates: &'a [CollisionPrimitive],
}
/// Host view only; it exposes original cell membership/order without allocating
/// per-query collections or constructing a second world/frame graph.
pub trait BoxStaticGrid {
    fn geometry(&self) -> BoxGridGeometry;
    fn cell_count(&self) -> usize;
    fn cell(&self, index: usize) -> BoxCellView<'_>;
}
impl BoxStaticGrid for crate::collision_world_driving::DrivingCollisionWorld {
    fn geometry(&self) -> BoxGridGeometry {
        let g = self.query_geometry();
        BoxGridGeometry {
            origin_xz: g.origin_xz,
            cell_size_xz: g.cell_size_xz,
            width: g.width,
            height: g.height,
        }
    }
    fn cell_count(&self) -> usize {
        let g = self.query_geometry();
        g.width as usize * g.height as usize
    }
    fn cell(&self, index: usize) -> BoxCellView<'_> {
        let (max_y, _, candidates) = self.query_cell_candidates(index);
        BoxCellView { max_y, candidates }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxDynamicGeometry {
    SphereBound {
        center: [f32; 3],
        radius: f32,
    },
    Aabb {
        min: [f32; 3],
        max: [f32; 3],
    },
    /// Original classifications other than 0x40/0x41/0x42/0x44 are ignored.
    Ignored,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxDynamicCandidate {
    pub identity: u32,
    pub properties: u32,
    pub reject_flags: u32,
    pub geometry: BoxDynamicGeometry,
}
#[derive(Clone, Copy)]
pub struct BoxDynamicGrid<'a> {
    pub geometry: BoxGridGeometry,
    /// A null cell is distinct from a nonnull cell with an empty candidate list.
    pub cells: &'a [Option<&'a [BoxDynamicCandidate]>],
    pub excluded: &'a [u32],
}
#[derive(Clone, Copy)]
pub enum BoxDynamicQuery<'a> {
    Absent,
    Grid(&'a BoxDynamicGrid<'a>),
    Unavailable,
}
fn integer(value: f32) -> Out<i32> {
    number(value)?;
    if (value as f64) < i32::MIN as f64 || (value as f64) >= 2147483648. {
        return Err(BoxSweepError::NumericDomain);
    }
    let result = value.trunc() as i32;
    crate::native_precision::record_precision(result as f32 != value);
    Ok(result)
}
fn grid_valid(g: BoxGridGeometry, cells: usize) -> Out<()> {
    if g.width > i32::MAX as u32
        || g.height > i32::MAX as u32
        || g.width as usize * (g.height as usize) != cells
    {
        return Err(BoxSweepError::InvalidGrid);
    }
    for size in g.cell_size_xz {
        if number(size)? <= 0. {
            return Err(BoxSweepError::InvalidGrid);
        }
    }
    for origin in g.origin_xz {
        number(origin)?;
    }
    Ok(())
}
fn cell_range(g: BoxGridGeometry, param: &BoxParam, dynamic: bool) -> Out<([i32; 2], [i32; 2])> {
    let mut begin = [0; 2];
    let mut end = [0; 2];
    for (i, axis) in [0, 2].into_iter().enumerate() {
        let inverse = div(1., g.cell_size_xz[i])?;
        let lo = mul(sub(param.world_min[axis], g.origin_xz[i])?, inverse)?;
        let hi = mul(sub(param.world_max[axis], g.origin_xz[i])?, inverse)?;
        begin[i] = integer(if dynamic { sub(lo, 1.)? } else { lo })?.max(0);
        let high = integer(add(hi, if dynamic { 2. } else { 1. })?)?;
        let limit = if i == 0 { g.width } else { g.height };
        end[i] = if dynamic {
            if high as u32 > limit {
                limit as i32
            } else {
                high
            }
        } else {
            high.min(limit as i32)
        };
    }
    Ok((begin, end))
}
/// Original BoxInDynamic, PC RVA0x1c4230. Sphere-bound tangency is inclusive,
/// unlike BoxInNonFace's strict sphere-bound predicate.
pub fn box_in_dynamic(grid: &BoxDynamicGrid<'_>, param: &BoxParam) -> Out<bool> {
    let g = grid.geometry;
    grid_valid(g, grid.cells.len())?;
    let (begin, end) = cell_range(g, param, true)?;
    for z in begin[1]..end[1] {
        for x in begin[0]..end[0] {
            let Some(candidates) = grid.cells[z as usize * g.width as usize + x as usize] else {
                continue;
            };
            for candidate in candidates {
                if candidate.reject_flags & param.dynamic_mask != 0
                    || grid.excluded.contains(&candidate.identity)
                {
                    continue;
                }
                match (candidate.properties as u8, candidate.geometry) {
                    (0x40 | 0x42 | 0x44, BoxDynamicGeometry::SphereBound { center, radius }) => {
                        let delta = difference(param.center, center)?;
                        let distance = dot(delta, delta, [1, 2, 0])?;
                        let sum = add(radius, param.half_diagonal_squared)?;
                        if distance <= mul(sum, sum)? {
                            return Ok(true);
                        }
                    }
                    (0x41, BoxDynamicGeometry::Aabb { min, max }) => {
                        let hit = number(min[0])? <= number(param.world_max[0])?
                            && number(min[1])? <= number(param.world_max[1])?
                            && number(min[2])? <= number(param.world_max[2])?
                            && number(param.world_min[0])? <= number(max[0])?
                            && number(param.world_min[1])? <= number(max[1])?
                            && number(param.world_min[2])? <= number(max[2])?;
                        if hit {
                            return Ok(true);
                        }
                    }
                    (0x40 | 0x41 | 0x42 | 0x44, _) => return Err(BoxSweepError::GeometryMismatch),
                    _ => {}
                }
            }
        }
    }
    Ok(false)
}
/// Original BoxInNonFace, PC RVA0x1c44b0. These predicates intentionally retain
/// the original bound tests, rather than substituting a line cast or SAT test.
pub fn box_in_non_face(volume: &Volume, param: &BoxParam) -> Out<bool> {
    if !(0x80..=0x84).contains(&(volume.properties as u8)) {
        return Ok(false);
    }
    if volume.properties as u8 != volume.geometry.classification() {
        return Err(BoxSweepError::GeometryMismatch);
    }
    match volume.geometry {
        VolumeGeometry::XtObb { min, max, .. } | VolumeGeometry::Aabb { min, max } => {
            for axis in [0, 2, 1] {
                if !(number(min[axis])? < number(param.world_max[axis])?
                    && number(max[axis])? > number(param.world_min[axis])?)
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        VolumeGeometry::Sphere { center, radius } => {
            let delta = difference(center, param.center)?;
            let distance = dot(delta, delta, [1, 2, 0])?;
            let sum = add(param.half_diagonal_squared, radius)?;
            Ok(distance < mul(sum, sum)?)
        }
        VolumeGeometry::Cylinder { center, radius } => {
            let delta = [
                sub(center[0], param.center[0])?,
                sub(param.center[1], param.center[1])?,
                sub(center[1], param.center[2])?,
            ];
            let distance = dot(delta, delta, [1, 2, 0])?;
            let sum = add(param.half_diagonal_squared, radius)?;
            Ok(distance < mul(sum, sum)?)
        }
        VolumeGeometry::Obb {
            extent_min,
            extent_max,
            inverse,
            ..
        } => {
            let center = SMatrix { elements: inverse }
                .try_transform_point(param.center)
                .map_err(|_| BoxSweepError::NumericDomain)?;
            let extent = number(param.half_diagonal_squared)?;
            Ok(add(center[0], extent)? > number(extent_min[0])?
                && sub(center[0], extent)? < number(extent_max[0])?
                && add(center[2], extent)? > number(extent_min[2])?
                && sub(center[2], extent)? < number(extent_max[2])?
                && add(center[1], extent)? > 0.
                && sub(center[1], extent)? < 0.)
        }
    }
}
/// Original BoxInTreeLevel, PC RVA0x1c46c0. Candidate and vertex order is the
/// original ascending-X order, including the early whole-leaf miss.
pub fn box_in_tree_level(candidates: &[CollisionPrimitive], param: &BoxParam) -> Out<bool> {
    let mut last_min = None;
    for candidate in candidates {
        let properties = candidate.properties();
        if properties & 0x20 != 0 {
            continue;
        }
        if properties & 0x80 != 0 {
            if !(0x80..=0x84).contains(&(properties as u8)) {
                continue;
            }
            match candidate {
                CollisionPrimitive::Volume(volume) => {
                    if box_in_non_face(volume, param)? {
                        return Ok(true);
                    }
                }
                CollisionPrimitive::Unavailable { error, .. } => {
                    return Err(BoxSweepError::Preparation(*error));
                }
                _ => return Err(BoxSweepError::GeometryMismatch),
            }
            continue;
        }
        let triangle = match candidate {
            CollisionPrimitive::Triangle(t) => t,
            CollisionPrimitive::Unavailable { error, .. } => {
                return Err(BoxSweepError::Preparation(*error));
            }
            _ => return Err(BoxSweepError::GeometryMismatch),
        };
        let v = &triangle.vertices;
        let min_x = number(v[0][0])?;
        let mid_x = number(v[1][0])?;
        let max_x = number(v[2][0])?;
        if min_x > mid_x || mid_x > max_x {
            return Err(BoxSweepError::UnsortedVertices);
        }
        if last_min.is_some_and(|last| last > min_x) {
            return Err(BoxSweepError::UnsortedCandidates);
        }
        last_min = Some(min_x);
        if param.world_min[0] > max_x {
            continue;
        }
        if param.world_max[0] < min_x {
            return Ok(false);
        }
        let mut overlap = true;
        for axis in [1, 2] {
            let values = [
                number(v[0][axis])?,
                number(v[1][axis])?,
                number(v[2][axis])?,
            ];
            let lo = values[0].min(values[1]).min(values[2]);
            let hi = values[0].max(values[1]).max(values[2]);
            if lo > param.world_max[axis] || param.world_min[axis] > hi {
                overlap = false;
                break;
            }
        }
        if !overlap {
            continue;
        }
        let delta = difference(param.extended_center, v[0])?;
        let distance = dot(
            delta,
            [triangle.plane[0], triangle.plane[1], triangle.plane[2]],
            [1, 2, 0],
        )?
        .abs();
        if mul(distance, distance)? <= param.extended_diagonal_squared {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Original _TestDBox, PC RVA0x1c48b0. A null static owner returns before
/// dynamic dispatch. Eligible unavailable owners return an error, never a miss.
pub fn test_d_box<G: BoxStaticGrid + ?Sized>(
    grid: Option<&G>,
    dynamic: BoxDynamicQuery<'_>,
    request: &BoxSweepRequest,
) -> Out<bool> {
    let Some(grid) = grid else { return Ok(false) };
    let param = BoxParam::new(request)?;
    if param.dynamic_mask != u32::MAX {
        match dynamic {
            BoxDynamicQuery::Absent => {}
            BoxDynamicQuery::Unavailable => return Err(BoxSweepError::UnsupportedDynamicQuery),
            BoxDynamicQuery::Grid(grid) => {
                if box_in_dynamic(grid, &param)? {
                    return Ok(true);
                }
            }
        }
    }
    if param.static_mask == u32::MAX {
        return Ok(false);
    }
    let geometry = grid.geometry();
    grid_valid(geometry, grid.cell_count())?;
    let (begin, end) = cell_range(geometry, &param, false)?;
    let mut result = false;
    for z in begin[1]..end[1] {
        for x in begin[0]..end[0] {
            let cell = grid.cell(z as usize * geometry.width as usize + x as usize);
            if param.world_min[1] < number(cell.max_y)? {
                result |= box_in_tree_level(cell.candidates, &param)?;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> BoxSweepRequest {
        BoxSweepRequest {
            bounds: [[-1.; 3], [1.; 3]],
            first_matrix: SMatrix::identity(),
            second_matrix: SMatrix::identity(),
            vectors: [[0.; 3]; 3],
            dynamic_mask: u32::MAX,
            static_mask: 0,
        }
    }
    struct Grid {
        max_y: f32,
        candidates: Vec<CollisionPrimitive>,
    }
    impl BoxStaticGrid for Grid {
        fn geometry(&self) -> BoxGridGeometry {
            BoxGridGeometry {
                origin_xz: [-10.; 2],
                cell_size_xz: [20.; 2],
                width: 1,
                height: 1,
            }
        }
        fn cell_count(&self) -> usize {
            1
        }
        fn cell(&self, _: usize) -> BoxCellView<'_> {
            BoxCellView {
                max_y: self.max_y,
                candidates: &self.candidates,
            }
        }
    }
    #[test]
    fn null_static_owner_precedes_dynamic_and_unused_invalid_bbox() {
        let mut input = request();
        input.bounds[0][0] = f32::NAN;
        assert_eq!(
            test_d_box::<Grid>(None, BoxDynamicQuery::Unavailable, &input),
            Ok(false)
        );
    }
    #[test]
    fn cell_y_gate_does_not_enter_an_ineligible_bad_leaf() {
        let triangle = crate::collision_line::Triangle {
            properties: 0,
            vertices: [[2., 0., 0.], [1., 0., 1.], [0., 0., 2.]],
            plane: [0., 1., 0., 0.],
            identity: 1,
        };
        let mut grid = Grid {
            max_y: -1.,
            candidates: vec![CollisionPrimitive::Triangle(triangle)],
        };
        assert_eq!(
            test_d_box(Some(&grid), BoxDynamicQuery::Absent, &request()),
            Ok(false)
        );
        grid.max_y = -0.5;
        assert_eq!(
            test_d_box(Some(&grid), BoxDynamicQuery::Absent, &request()),
            Err(BoxSweepError::UnsortedVertices)
        );
    }
    #[test]
    fn original_squared_fields_and_dynamic_static_tangencies_stay_distinct() {
        let mut input = request();
        input.vectors[2] = [0., 0.05, 0.];
        let mut param = BoxParam::new(&input).unwrap();
        assert_eq!(param.half_diagonal_squared, 3.);
        assert_eq!(param.extended_center, [0., 1., 0.]);
        assert_eq!(param.extended_diagonal_squared, 6.);
        param.half_diagonal_squared = 1.;
        let volume = Volume {
            properties: 0x82,
            identity: 1,
            geometry: VolumeGeometry::Sphere {
                center: [2., 0., 0.],
                radius: 1.,
            },
        };
        assert_eq!(box_in_non_face(&volume, &param), Ok(false));
        let candidates = [BoxDynamicCandidate {
            identity: 1,
            properties: 0x42,
            reject_flags: 0,
            geometry: BoxDynamicGeometry::SphereBound {
                center: [2., 0., 0.],
                radius: 1.,
            },
        }];
        let cells = [Some(candidates.as_slice())];
        let grid = BoxDynamicGrid {
            geometry: BoxGridGeometry {
                origin_xz: [-10.; 2],
                cell_size_xz: [20.; 2],
                width: 1,
                height: 1,
            },
            cells: &cells,
            excluded: &[],
        };
        assert_eq!(box_in_dynamic(&grid, &param), Ok(true));
    }
}
