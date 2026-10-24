//! Original C_Break_plain_glass::CutMesh arithmetic (PC 0x562780).
//! The returned temporary vectors belong to the caller's mesh construction.
use crate::{native_transform::SMatrix, ph_thing_response::PhThingNumber as R};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlassGeometry {
    pub vertices: Vec<GlassVertex>,
    pub faces: Vec<[u16; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GlassSubdivisionError<E> {
    Random(E),
    EmptySourceFaces,
    InvalidVertex(u16),
    VertexIndexOverflow,
    NumericDomain(&'static str),
}
impl<E> From<&'static str> for GlassSubdivisionError<E> {
    fn from(value: &'static str) -> Self {
        Self::NumericDomain(value)
    }
}
fn n(value: f32) -> Result<R, &'static str> {
    R::load(value)
}
const EPSILON: f32 = f32::from_bits(0x322b_cc77);

fn squared(v: [R; 3], order: [usize; 3]) -> Result<R, &'static str> {
    let [a, b, c] = order;
    v[a].mul(v[a])?.add(v[b].mul(v[b])?)?.add(v[c].mul(v[c])?)
}

fn edge_length(a: [f32; 3], b: [f32; 3], order: [usize; 3]) -> Result<R, &'static str> {
    let delta = [
        n(a[0])?.sub(n(b[0])?)?,
        n(a[1])?.sub(n(b[1])?)?,
        n(a[2])?.sub(n(b[2])?)?,
    ];
    let square = squared(delta, order)?.store()?;
    if (square.to_bits() as i32) < EPSILON.to_bits() as i32 {
        n(0.)
    } else if n(square)?.sub(n(1.)?)?.value().abs() < f64::from(EPSILON) {
        n(1.)
    } else {
        n(square)?.sqrt()
    }
}

pub(crate) fn glass_transform_position(
    position: [f32; 3],
    matrix: &SMatrix,
) -> Result<[f32; 3], &'static str> {
    let mut out = [0.; 3];
    for (column, value) in out.iter_mut().enumerate() {
        let [a, b, c] = if column == 0 { [2, 1, 0] } else { [2, 0, 1] };
        *value = n(matrix.elements[a * 4 + column])?
            .mul(n(position[a])?)?
            .add(n(matrix.elements[b * 4 + column])?.mul(n(position[b])?)?)?
            .add(n(matrix.elements[c * 4 + column])?.mul(n(position[c])?)?)?
            .add(n(matrix.elements[12 + column])?)?
            .store()?;
    }
    Ok(out)
}

pub(crate) fn glass_rotate_normal(
    normal: [f32; 3],
    matrix: &SMatrix,
) -> Result<[f32; 3], &'static str> {
    let mut out = [0.; 3];
    for (column, value) in out.iter_mut().enumerate() {
        *value = n(normal[0])?
            .mul(n(matrix.elements[column])?)?
            .add(n(normal[1])?.mul(n(matrix.elements[4 + column])?)?)?
            .add(n(normal[2])?.mul(n(matrix.elements[8 + column])?)?)?
            .store()?;
    }
    Ok(out)
}

fn split_vertex(a: GlassVertex, b: GlassVertex, fraction: R) -> Result<GlassVertex, &'static str> {
    // The original spills the X product and Z difference separately. Y's
    // difference and product retain the x87 exponent until the final store.
    let dx = n(b.position[0])?
        .sub(n(a.position[0])?)?
        .mul(fraction)?
        .store()?;
    let dz = n(b.position[2])?.sub(n(a.position[2])?)?.store()?;
    let position = [
        n(dx)?.add(n(a.position[0])?)?.store()?,
        n(b.position[1])?
            .sub(n(a.position[1])?)?
            .mul(fraction)?
            .add(n(a.position[1])?)?
            .store()?,
        n(dz)?.mul(fraction)?.add(n(a.position[2])?)?.store()?,
    ];
    let mut normal = [0.; 3];
    for (i, value) in normal.iter_mut().enumerate() {
        *value = n(a.normal[i])?.add(n(b.normal[i])?)?.store()?;
    }
    let square = squared([n(normal[0])?, n(normal[1])?, n(normal[2])?], [0, 2, 1])?;
    let spilled_square = square.store()?;
    if square.sub(n(1.)?)?.value().abs() >= f64::from(EPSILON) {
        if spilled_square >= EPSILON {
            let inverse = n(1.)?.div(n(spilled_square)?.sqrt()?)?;
            for value in &mut normal {
                *value = n(*value)?.mul(inverse)?.store()?;
            }
        } else {
            let axis = if normal[0] != 0. {
                0
            } else if normal[2] != 0. {
                2
            } else {
                1
            };
            normal[axis] = if normal[axis] < 0. { -1. } else { 1. };
        }
    }
    let mut uv = [0.; 2];
    for (i, value) in uv.iter_mut().enumerate() {
        *value = n(b.uv[i])?
            .sub(n(a.uv[i])?)?
            .mul(fraction)?
            .add(n(a.uv[i])?)?
            .store()?;
    }
    Ok(GlassVertex {
        position,
        normal,
        uv,
    })
}

/// Select normal-facing faces, duplicate their vertices, transform positions,
/// and split long edges in the original reverse traversal/RNG order. The
/// original computes normal rotations into discarded stack storage.
/// The RNG callback supplies the original signed `rand()` result and owns its
/// canonical seed; this worker never creates a second random stream.
pub fn subdivide_glass<E>(
    source: &GlassGeometry,
    world: &SMatrix,
    mut rand: impl FnMut() -> Result<i32, E>,
) -> Result<GlassGeometry, GlassSubdivisionError<E>> {
    let first = source
        .faces
        .first()
        .ok_or(GlassSubdivisionError::EmptySourceFaces)?;
    let vertex = |index: u16| {
        source
            .vertices
            .get(usize::from(index))
            .copied()
            .ok_or(GlassSubdivisionError::InvalidVertex(index))
    };
    let reference = vertex(first[0])?.normal;
    let mut out = GlassGeometry::default();
    for face in &source.faces {
        let normal = vertex(face[0])?.normal;
        let dot = n(reference[2])?
            .mul(n(normal[2])?)?
            .add(n(reference[1])?.mul(n(normal[1])?)?)?
            .add(n(reference[0])?.mul(n(normal[0])?)?)?;
        if dot.value() > 0. {
            let index = u16::try_from(out.vertices.len())
                .map_err(|_| GlassSubdivisionError::VertexIndexOverflow)?;
            let second = index
                .checked_add(1)
                .ok_or(GlassSubdivisionError::VertexIndexOverflow)?;
            let third = index
                .checked_add(2)
                .ok_or(GlassSubdivisionError::VertexIndexOverflow)?;
            out.faces.push([index, second, third]);
            for &index in face {
                out.vertices.push(vertex(index)?);
            }
        }
    }
    for vertex in out.vertices.iter_mut().rev() {
        vertex.position = glass_transform_position(vertex.position, world)?;
        // RotateByMatrix writes the original stack temporary, not the vertex.
        // Execute its arithmetic (including precision effects), then discard it.
        let _ = glass_rotate_normal(vertex.normal, world)?;
    }
    loop {
        let mut changed = false;
        let count = out.faces.len();
        for i in (0..count).rev() {
            if out.faces.len() > 200 {
                return Ok(out);
            }
            let [ia, ib, ic] = out.faces[i];
            let [a, b, c] = [
                out.vertices[usize::from(ia)],
                out.vertices[usize::from(ib)],
                out.vertices[usize::from(ic)],
            ];
            let ab = n(edge_length(a.position, b.position, [2, 1, 0])?.store()?)?;
            let bc = n(edge_length(b.position, c.position, [2, 1, 0])?.store()?)?;
            let ca = edge_length(c.position, a.position, [0, 2, 1])?;
            if bc.add(ca)?.add(ab)?.value() <= f64::from(f32::from_bits(0x3f33_3333)) {
                continue;
            }
            let edge = if ab.value() > bc.value() {
                if ab.value() > ca.value() {
                    0
                } else {
                    2
                }
            } else if bc.value() > ca.value() {
                1
            } else {
                2
            };
            let value = rand().map_err(GlassSubdivisionError::Random)? % 100;
            let fraction = n(value as f32)?
                .mul(n(f32::from_bits(0x3c23_d70a))?)?
                .mul(n(0.5)?)?
                .add(n(0.25)?)?;
            let new_index = u16::try_from(out.vertices.len())
                .map_err(|_| GlassSubdivisionError::VertexIndexOverflow)?;
            let (split, appended, replaced) = match edge {
                0 => (
                    split_vertex(a, b, fraction)?,
                    [ia, new_index, ic],
                    [new_index, ib, ic],
                ),
                1 => (
                    split_vertex(b, c, fraction)?,
                    [ia, ib, new_index],
                    [new_index, ic, ia],
                ),
                _ => (
                    split_vertex(c, a, fraction)?,
                    [ia, ib, new_index],
                    [ib, ic, new_index],
                ),
            };
            out.faces.push(appended);
            out.faces[i] = replaced;
            out.vertices.push(split);
            changed = true;
        }
        if out.faces.len() > 200 || !changed {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> GlassVertex {
        GlassVertex {
            position,
            normal,
            uv,
        }
    }
    fn triangle(positions: [[f32; 3]; 3]) -> GlassGeometry {
        GlassGeometry {
            vertices: positions
                .map(|p| vertex(p, [0., 0., 1.], [0., 0.]))
                .to_vec(),
            faces: vec![[0, 1, 2]],
        }
    }

    #[test]
    fn facing_filter_duplicates_selected_faces_and_preserves_source_normals() {
        let mut source = triangle([[0., 0., 0.], [0.02, 0., 0.], [0., 0.02, 0.]]);
        source
            .vertices
            .push(vertex([0., 0., 0.], [0., 0., -1.], [0., 0.]));
        source.faces.extend([[3, 1, 2], [0, 2, 1]]);
        let mut world = SMatrix::identity();
        world.elements[10] = 2.;
        world.elements[12] = 10.;
        let out =
            subdivide_glass::<()>(&source, &world, || panic!("short faces do not consume RNG"))
                .unwrap();
        assert_eq!(out.faces, [[0, 1, 2], [3, 4, 5]]);
        assert_eq!(out.vertices.len(), 6);
        assert_eq!(out.vertices[0].position, [10., 0., 0.]);
        assert_eq!(out.vertices[0].normal, [0., 0., 1.]);
        assert_eq!(out.vertices[4].position, [10., 0.02, 0.]);
    }

    #[test]
    fn single_bc_split_preserves_faces_and_interpolates_uv() {
        let mut source = triangle([[0., 0., 0.], [0.3, 0., 0.], [0., 0.1, 0.]]);
        source.vertices[1].uv = [0., 1.];
        source.vertices[2].uv = [1., 3.];
        let mut calls = 0;
        let out = subdivide_glass::<()>(&source, &SMatrix::identity(), || {
            calls += 1;
            Ok(0)
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(out.faces, [[3, 2, 0], [0, 1, 3]]);
        assert_eq!(out.vertices[3].position, [0.22500001, 0.025, 0.]);
        assert_eq!(out.vertices[3].normal, [0., 0., 1.]);
        assert_eq!(out.vertices[3].uv, [0.25, 1.5]);
    }

    #[test]
    fn antiparallel_normal_sum_uses_original_positive_y_fallback() {
        let a = vertex([0., 0., 0.], [1., 0., 0.], [0., 0.]);
        let b = vertex([1., 2., 3.], [-1., 0., 0.], [1., 1.]);
        let split = split_vertex(a, b, n(0.5).unwrap()).unwrap();
        assert_eq!(split.position, [0.5, 1., 1.5]);
        assert_eq!(split.normal, [0., 1., 0.]);
    }

    #[test]
    fn stop_after_crossing_face_limit_and_consume_one_random_per_split() {
        let source = triangle([[0., 0., 0.], [100., 0., 0.], [0., 100., 0.]]);
        let mut calls = 0;
        let out = subdivide_glass::<()>(&source, &SMatrix::identity(), || {
            calls += 1;
            Ok(50)
        })
        .unwrap();
        assert_eq!(out.faces.len(), 201);
        assert_eq!(out.vertices.len(), 203);
        assert_eq!(calls, 200);
        assert!(out
            .faces
            .iter()
            .flatten()
            .all(|&i| usize::from(i) < out.vertices.len()));
    }

    #[test]
    fn zero_reference_normal_selects_nothing_without_random_draws() {
        let mut source = triangle([[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]);
        source.vertices[0].normal = [0.; 3];
        let out = subdivide_glass::<()>(&source, &SMatrix::identity(), || {
            panic!("no selected faces")
        })
        .unwrap();
        assert!(out.vertices.is_empty());
        assert!(out.faces.is_empty());
    }

    #[test]
    fn invalid_source_and_random_failure_remain_explicit() {
        assert_eq!(
            subdivide_glass::<()>(&GlassGeometry::default(), &SMatrix::identity(), || Ok(0)),
            Err(GlassSubdivisionError::EmptySourceFaces)
        );
        let source = triangle([[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]);
        assert_eq!(
            subdivide_glass(&source, &SMatrix::identity(), || Err("rng")),
            Err(GlassSubdivisionError::Random("rng"))
        );
    }
}
