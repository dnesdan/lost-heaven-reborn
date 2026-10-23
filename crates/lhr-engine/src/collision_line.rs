//! Triangle-only projection of PC `g_collision::LineInTreeLevel` (RVA `0x1c66a0`).
//!
//! This is a dependency of collision queries, not a collision world or `TestLineV`.
//! Candidates and each triangle's vertices must retain the original ascending-X
//! order. Planes and classification bytes come from the original collision data.
//! Arithmetic models x87 precision 24, round-to-nearest (control word `0x007f`),
//! provided the host uses ordinary IEEE round-to-nearest arithmetic. This helper
//! does not inspect or change the host floating-point environment. Floating-point
//! status flags, traps and other rounding/precision profiles are outside its contract.
//! Every used arithmetic value must be zero or have magnitude in `2^-40..=2^40`;
//! this bounded domain avoids x87/SSE exponent-range differences. Other x87
//! precision modes, degenerate divisions and nonfinite inputs are unsupported.
//!
//! The original worker writes the stored plane normal, not an outward normalized
//! normal. Its caller performs that conversion. Errors below are host policy:
//! they leave the caller's query unchanged; the original has no such error API.

/// A bound 32-byte collision triangle, with pointers replaced by world vertices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    /// Type/classification, sort, flags and material in increasing byte order.
    pub properties: u32,
    pub vertices: [[f32; 3]; 3],
    pub plane: [f32; 4],
    /// Caller-owned stable record tag; it is never treated as a pointer.
    pub identity: u32,
}

/// The used fields of original `tCLineParam`. Bounds are supplied by the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineQuery {
    pub origin: [f32; 3],
    /// A displacement, with point(t) = origin + delta*t.
    pub delta: [f32; 3],
    pub min_xz: [f32; 2],
    pub max_xz: [f32; 2],
    pub reject_mask: u32,
    /// Only a strictly smaller t replaces the current result.
    pub best_fraction: f32,
    pub normal: [f32; 3],
    pub primitive: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryError {
    /// An eligible record would call the unimplemented volume worker.
    UnsupportedVolume,
    UnsupportedClassification,
    /// The input or an arithmetic operation is outside the admitted domain.
    NumericDomain,
    UnsortedVertices,
    UnsortedCandidates,
    InvalidBounds,
}

const MIN_MAGNITUDE: f32 = 1.0 / ((1_u64 << 40) as f32);
const MAX_MAGNITUDE: f32 = (1_u64 << 40) as f32;

fn number(x: f32) -> Result<f32, QueryError> {
    if x.is_finite() && (x == 0.0 || (MIN_MAGNITUDE..=MAX_MAGNITUDE).contains(&x.abs())) {
        Ok(x)
    } else {
        Err(QueryError::NumericDomain)
    }
}
fn add(a: f32, b: f32) -> Result<f32, QueryError> {
    number(a + b)
}
fn sub(a: f32, b: f32) -> Result<f32, QueryError> {
    number(a - b)
}
fn mul(a: f32, b: f32) -> Result<f32, QueryError> {
    number(a * b)
}
fn div(a: f32, b: f32) -> Result<f32, QueryError> {
    if b == 0.0 {
        return Err(QueryError::NumericDomain);
    }
    number(a / b)
}

/// Project the original ordered triangle path, preserving no-hit output fields.
///
/// Masked records are skipped before their type or geometry is inspected. The
/// supported input excludes eligible volume/classification records and unsorted
/// geometry. This validation is host policy and runs before any query mutation.
/// On success, only `best_fraction`, `normal` and `primitive` can change.
/// A best fraction of 1 excludes a hit exactly at the segment's far endpoint.
pub fn line_in_tree_level_triangles(
    candidates: &[Triangle],
    query: &mut LineQuery,
) -> Result<bool, QueryError> {
    for value in query
        .origin
        .into_iter()
        .chain(query.delta)
        .chain(query.min_xz)
        .chain(query.max_xz)
        .chain([query.best_fraction])
    {
        number(value)?;
    }
    if query.min_xz[0] > query.max_xz[0] || query.min_xz[1] > query.max_xz[1] {
        return Err(QueryError::InvalidBounds);
    }
    let mut previous_min = None;
    for triangle in candidates {
        if triangle.properties & query.reject_mask != 0 {
            continue;
        }
        if triangle.properties & 0xa0 != 0 {
            return Err(QueryError::UnsupportedVolume);
        }
        if !(1..=7).contains(&(triangle.properties as u8)) {
            return Err(QueryError::UnsupportedClassification);
        }
        for value in triangle
            .vertices
            .into_iter()
            .flatten()
            .chain(triangle.plane)
        {
            number(value)?;
        }
        let [a, b, c] = triangle.vertices;
        if a[0] > b[0] || b[0] > c[0] {
            return Err(QueryError::UnsortedVertices);
        }
        if previous_min.is_some_and(|x| x > a[0]) {
            return Err(QueryError::UnsortedCandidates);
        }
        previous_min = Some(a[0]);
    }
    let mut result = *query;
    let mut hit = false;
    for triangle in candidates {
        if triangle.properties & result.reject_mask != 0 {
            continue;
        }
        let [a, b, c] = triangle.vertices;
        if c[0] < result.min_xz[0] {
            continue;
        }
        if a[0] > result.max_xz[0] {
            break;
        }
        let min_z = a[2].min(b[2]).min(c[2]);
        let max_z = a[2].max(b[2]).max(c[2]);
        if min_z > result.max_xz[1] || max_z < result.min_xz[1] {
            continue;
        }
        let [nx, ny, nz, distance] = triangle.plane;
        // Preserve the observed x87 instruction order; do not fuse operations.
        let denominator = add(
            add(mul(result.delta[2], nz)?, mul(result.delta[1], ny)?)?,
            mul(result.delta[0], nx)?,
        )?;
        if denominator < 1.0e-8_f32 {
            continue;
        }
        let numerator = add(
            add(
                add(mul(ny, result.origin[1])?, mul(nz, result.origin[2])?)?,
                mul(nx, result.origin[0])?,
            )?,
            distance,
        )?;
        let fraction = -div(numerator, denominator)?;
        if fraction < 0.0 || fraction >= result.best_fraction {
            continue;
        }
        let x = sub(
            add(mul(fraction, result.delta[0])?, result.origin[0])?,
            a[0],
        )?;
        let y = sub(
            add(mul(fraction, result.delta[1])?, result.origin[1])?,
            a[1],
        )?;
        let z = sub(
            add(mul(fraction, result.delta[2])?, result.origin[2])?,
            a[2],
        )?;
        let bx = sub(b[0], a[0])?;
        let cx = sub(c[0], a[0])?;
        let bz = sub(b[2], a[2])?;
        let cz = sub(c[2], a[2])?;
        let by = sub(b[1], a[1])?;
        let cy = sub(c[1], a[1])?;
        let class = triangle.properties as u8;
        let u = match class {
            1 => div(y, cy)?,
            2 => div(
                sub(mul(by, z)?, mul(bz, y)?)?,
                sub(mul(by, cz)?, mul(cy, bz)?)?,
            )?,
            3 | 6 => div(x, cx)?,
            4 => div(z, cz)?,
            5 => div(
                sub(mul(bx, z)?, mul(bz, x)?)?,
                sub(mul(cz, bx)?, mul(bz, cx)?)?,
            )?,
            7 => div(
                sub(mul(bx, y)?, mul(by, x)?)?,
                sub(mul(cy, bx)?, mul(by, cx)?)?,
            )?,
            _ => unreachable!("validated classification"),
        };
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let v = match class {
            1 | 3 => div(sub(z, mul(cz, u)?)?, bz)?,
            2 | 6 => div(sub(y, mul(cy, u)?)?, by)?,
            4 | 5 | 7 => div(sub(x, mul(cx, u)?)?, bx)?,
            _ => unreachable!("validated classification"),
        };
        if v >= 0.0 && add(u, v)? <= 1.0 {
            result.best_fraction = fraction;
            result.normal = [nx, ny, nz];
            result.primitive = Some(triangle.identity);
            hit = true;
        }
    }
    *query = result;
    Ok(hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(class: u8) -> (Triangle, LineQuery) {
        let (b, c, normal, origin, delta) = match class {
            1 => (
                [0., 0., 2.],
                [0., 2., 0.],
                [1., 0., 0.],
                [-1., 0.5, 0.5],
                [2., 0., 0.],
            ),
            2 => (
                [0., 1., 2.],
                [0., 2., 0.],
                [1., 0., 0.],
                [-1., 0.75, 0.5],
                [2., 0., 0.],
            ),
            3 => (
                [0., 0., 2.],
                [2., 0., 0.],
                [0., -1., 0.],
                [0.5, 1., 0.5],
                [0., -2., 0.],
            ),
            4 => (
                [1., 0., 0.],
                [2., 0., 2.],
                [0., -1., 0.],
                [0.75, 1., 0.5],
                [0., -2., 0.],
            ),
            5 => (
                [1., 0., 2.],
                [2., 0., 0.],
                [0., -1., 0.],
                [0.75, 1., 0.5],
                [0., -2., 0.],
            ),
            6 => (
                [0., 2., 0.],
                [2., 0., 0.],
                [0., 0., 1.],
                [0.5, 0.5, -1.],
                [0., 0., 2.],
            ),
            7 => (
                [1., 2., 0.],
                [2., 0., 0.],
                [0., 0., 1.],
                [0.75, 0.5, -1.],
                [0., 0., 2.],
            ),
            _ => unreachable!(),
        };
        (
            Triangle {
                properties: u32::from(class),
                vertices: [[0.; 3], b, c],
                plane: [normal[0], normal[1], normal[2], 0.],
                identity: 41,
            },
            LineQuery {
                origin,
                delta,
                min_xz: [-4.; 2],
                max_xz: [4.; 2],
                reject_mask: 0,
                best_fraction: 1.,
                normal: [7., 8., 9.],
                primitive: Some(100),
            },
        )
    }

    #[test]
    fn seven_projection_classes_write_raw_normal_and_keep_equal_first_hit() {
        for class in 1..=7 {
            let (triangle, mut query) = fixture(class);
            let mut second = triangle;
            second.identity = 42;
            assert_eq!(
                line_in_tree_level_triangles(&[triangle, second], &mut query),
                Ok(true)
            );
            assert_eq!(query.best_fraction.to_bits(), 0.5_f32.to_bits());
            assert_eq!(query.normal, triangle.plane[..3]);
            assert_eq!(query.primitive, Some(41));
            let retained = query;
            assert_eq!(
                line_in_tree_level_triangles(&[second], &mut query),
                Ok(false)
            );
            assert_eq!(query, retained);
        }
    }

    #[test]
    fn far_endpoint_and_reverse_side_preserve_output_sentinels() {
        let (triangle, mut query) = fixture(3);
        query.delta[1] = -1.;
        let sentinel = query;
        assert_eq!(
            line_in_tree_level_triangles(&[triangle], &mut query),
            Ok(false)
        );
        assert_eq!(query, sentinel);
        query.delta[1] = 2.;
        let sentinel = query;
        assert_eq!(
            line_in_tree_level_triangles(&[triangle], &mut query),
            Ok(false)
        );
        assert_eq!(query, sentinel);
    }

    #[test]
    fn masked_unsupported_records_are_skipped_and_errors_are_transactional() {
        let (triangle, mut query) = fixture(3);
        let mut volume = triangle;
        volume.properties = 0x81 | 0x0200_0000;
        query.reject_mask = 0x0200_0000;
        assert_eq!(
            line_in_tree_level_triangles(&[volume, triangle], &mut query),
            Ok(true)
        );
        let (_, mut query) = fixture(3);
        let sentinel = query;
        assert_eq!(
            line_in_tree_level_triangles(&[triangle, volume], &mut query),
            Err(QueryError::UnsupportedVolume)
        );
        assert_eq!(query, sentinel);
        let mut invalid = triangle;
        invalid.plane[0] = f32::INFINITY;
        assert_eq!(
            line_in_tree_level_triangles(&[invalid], &mut query),
            Err(QueryError::NumericDomain)
        );
        assert_eq!(query, sentinel);
    }
}
