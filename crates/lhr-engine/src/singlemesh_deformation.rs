//! Checked CPU singlemesh deformation under the original PC24/RN policy.
//!
//! This is a caller-owned snapshot of the arithmetic in `UpdateLodVertices`
//! (RVA 0x54f60), not its buffer allocation, proxy dispatch or cache lifecycle.
//! Positions use row vectors; normals use column-normalized matrix bases.
//! No final normal normalization or weight clamping is performed.

use crate::native_matrix::{
    checked_f32_add as add, checked_f32_mul as mul, MatrixArithmeticUnavailable,
};
use crate::native_transform::SMatrix;

/// The six transformed fields; source texture coordinates are not read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// One PC skin group, in joint-index order. Its rigid vertices precede its
/// blended vertices. The record's one-based blend index becomes `Some(i - 1)`;
/// record index zero becomes `None`, selecting identity for the second matrix.
#[derive(Clone, Copy, Debug)]
pub struct SkinGroup<'a> {
    pub bind_matrix: SMatrix,
    pub rigid_count: usize,
    pub blend_joint: Option<usize>,
    pub weights: &'a [f32],
}

/// Host availability checks, not extra branches in the original routines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeformationUnavailable {
    /// A needed operand/intermediate is not finite normal f32 or signed zero.
    NumericDomain,
    /// The original leaf loops require strictly positive counts.
    EmptyInput,
    WeightCount,
    JointCount,
    BlendJointIndex,
    VertexCount,
}

impl From<MatrixArithmeticUnavailable> for DeformationUnavailable {
    fn from(_: MatrixArithmeticUnavailable) -> Self {
        Self::NumericDomain
    }
}

/// Original `S_matrix::SetRot(S_matrix const&)`, RVA 0x2e1f0.
/// Normalizes each basis column only when |squared length - 1| > 1e-8
/// (the f32 threshold is widened to binary64). Zero columns are unavailable here:
/// the original produces non-finite values, outside the verified Rust domain.
/// The seven non-basis input elements are ignored.
pub fn skin_normal_matrix(matrix: &SMatrix) -> Result<SMatrix, DeformationUnavailable> {
    let mut result = SMatrix::identity();
    for column in 0..3 {
        let [x, y, z] = [
            matrix.elements[column],
            matrix.elements[4 + column],
            matrix.elements[8 + column],
        ];
        let squared = add(add(mul(x, x)?, mul(y, y)?)?, mul(z, z)?)?;
        let difference = add(squared, -1.0)?;
        let values = if f64::from(difference.abs()) > f64::from(1e-8_f32) {
            let root = squared.sqrt();
            let reciprocal = 1.0 / root;
            if !root.is_normal() || !reciprocal.is_normal() {
                return Err(DeformationUnavailable::NumericDomain);
            }
            [
                mul(reciprocal, x)?,
                mul(reciprocal, y)?,
                mul(reciprocal, z)?,
            ]
        } else {
            [x, y, z]
        };
        for (row, value) in values.into_iter().enumerate() {
            result.elements[row * 4 + column] = value;
        }
    }
    Ok(result)
}

fn transform_normal(
    normal: [f32; 3],
    matrix: &SMatrix,
) -> Result<[f32; 3], DeformationUnavailable> {
    let mut output = [0.; 3];
    for (column, value) in output.iter_mut().enumerate() {
        *value = add(
            add(
                mul(normal[0], matrix.elements[column])?,
                mul(normal[1], matrix.elements[4 + column])?,
            )?,
            mul(normal[2], matrix.elements[8 + column])?,
        )?;
    }
    Ok(output)
}

fn transform_vertex(
    source: SkinVertex,
    position_matrix: &SMatrix,
    normal_matrix: &SMatrix,
) -> Result<SkinVertex, DeformationUnavailable> {
    Ok(SkinVertex {
        position: position_matrix.try_transform_point(source.position)?,
        normal: transform_normal(source.normal, normal_matrix)?,
    })
}

/// Descriptive mapping of the original CPU helper at RVA 0x68360. It reads
/// positions/normals from 32-byte source records and writes 24-byte records;
/// the owned representation omits the unread texture fields and raw strides.
pub fn transform_skin_vertices(
    source: &[SkinVertex],
    position_matrix: &SMatrix,
    normal_matrix: &SMatrix,
) -> Result<Vec<SkinVertex>, DeformationUnavailable> {
    if source.is_empty() {
        return Err(DeformationUnavailable::EmptyInput);
    }
    source
        .iter()
        .map(|vertex| transform_vertex(*vertex, position_matrix, normal_matrix))
        .collect()
}

/// Descriptive mapping of the original CPU blend helper at RVA 0x68430.
/// `weight` multiplies the first transformed vertex, and `(1 - weight)` the
/// second. Each operation rounds separately; neither endpoint is skipped.
pub fn blend_skin_vertices(
    source: &[SkinVertex],
    first: (&SMatrix, &SMatrix),
    weights: &[f32],
    second: (&SMatrix, &SMatrix),
) -> Result<Vec<SkinVertex>, DeformationUnavailable> {
    if source.is_empty() {
        return Err(DeformationUnavailable::EmptyInput);
    }
    if source.len() != weights.len() {
        return Err(DeformationUnavailable::WeightCount);
    }
    source
        .iter()
        .zip(weights)
        .map(|(source, weight)| {
            let a = transform_vertex(*source, first.0, first.1)?;
            let b = transform_vertex(*source, second.0, second.1)?;
            let complement = add(1.0, -*weight)?;
            let blend = |a: [f32; 3], b: [f32; 3]| -> Result<_, DeformationUnavailable> {
                let mut out = [0.; 3];
                for i in 0..3 {
                    out[i] = add(mul(a[i], *weight)?, mul(b[i], complement)?)?;
                }
                Ok(out)
            };
            Ok(SkinVertex {
                position: blend(a.position, b.position)?,
                normal: blend(a.normal, b.normal)?,
            })
        })
        .collect()
}

/// Snapshot of the non-proxy `UpdateLodVertices` arithmetic. Joint matrices
/// must already come from `GetJointMatrix`; each is premultiplied by its skin
/// bind matrix. Unmoved vertices form a suffix and are copied bit for bit.
/// Count/index validation and failure without publication are host policies;
/// native malformed layouts, missing joints and partial writes are not modeled.
pub fn deform_singlemesh_lod(
    source: &[SkinVertex],
    groups: &[SkinGroup<'_>],
    rigid_suffix_count: usize,
    joint_matrices: &[SMatrix],
) -> Result<Vec<SkinVertex>, DeformationUnavailable> {
    if groups.len() != joint_matrices.len() {
        return Err(DeformationUnavailable::JointCount);
    }
    let moved = groups.iter().try_fold(0usize, |total, group| {
        total
            .checked_add(group.rigid_count)?
            .checked_add(group.weights.len())
    });
    if moved.and_then(|count| count.checked_add(rigid_suffix_count)) != Some(source.len()) {
        return Err(DeformationUnavailable::VertexCount);
    }
    let mut output = source.to_vec();
    let mut cursor = 0;
    for (group, joint_matrix) in groups.iter().zip(joint_matrices) {
        if group.rigid_count == 0 && group.weights.is_empty() {
            continue;
        }
        let matrix = group.bind_matrix.try_mul_affine(joint_matrix)?;
        let normals = skin_normal_matrix(&matrix)?;
        let end = cursor + group.rigid_count;
        if end != cursor {
            let transformed = transform_skin_vertices(&source[cursor..end], &matrix, &normals)?;
            output[cursor..end].copy_from_slice(&transformed);
        }
        cursor = end;
        let end = cursor + group.weights.len();
        if end != cursor {
            let second = if let Some(index) = group.blend_joint {
                let other = groups
                    .get(index)
                    .ok_or(DeformationUnavailable::BlendJointIndex)?;
                other.bind_matrix.try_mul_affine(&joint_matrices[index])?
            } else {
                SMatrix::identity()
            };
            let second_normals = skin_normal_matrix(&second)?;
            let transformed = blend_skin_vertices(
                &source[cursor..end],
                (&matrix, &normals),
                group.weights,
                (&second, &second_normals),
            )?;
            output[cursor..end].copy_from_slice(&transformed);
        }
        cursor = end;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex() -> SkinVertex {
        SkinVertex {
            position: [1., 2., 3.],
            normal: [0., 2., 0.],
        }
    }

    #[test]
    fn transforms_positions_and_normals_without_normalizing_output() {
        let mut matrix = SMatrix::identity();
        matrix.elements[0] = 2.;
        matrix.elements[5] = 3.;
        matrix.elements[10] = 4.;
        matrix.elements[12..15].copy_from_slice(&[10., 20., 30.]);
        let normal = skin_normal_matrix(&matrix).unwrap();
        assert_eq!(normal, SMatrix::identity());
        assert_eq!(
            transform_skin_vertices(&[vertex()], &matrix, &normal).unwrap(),
            [SkinVertex {
                position: [12., 26., 42.],
                normal: [0., 2., 0.],
            }]
        );
    }

    #[test]
    fn normal_matrix_uses_columns_and_ignores_non_basis_fields() {
        let mut matrix = SMatrix {
            elements: [
                2.,
                0.,
                0.,
                f32::NAN,
                0.,
                3.,
                4.,
                f32::INFINITY,
                0.,
                4.,
                -3.,
                f32::NAN,
                f32::NAN,
                f32::NAN,
                f32::NAN,
                f32::NAN,
            ],
        };
        let result = skin_normal_matrix(&matrix).unwrap();
        assert_eq!(
            result.elements,
            [1., 0., 0., 0., 0., 0.6, 0.8, 0., 0., 0.8, -0.6, 0., 0., 0., 0., 1.]
        );
        matrix.elements[0] = 0.;
        assert_eq!(
            skin_normal_matrix(&matrix),
            Err(DeformationUnavailable::NumericDomain)
        );
    }

    #[test]
    fn blend_weights_first_endpoint_and_extrapolates() {
        let identity = SMatrix::identity();
        let mut first = identity;
        first.elements[12] = 10.;
        let source = [vertex(); 4];
        let output = blend_skin_vertices(
            &source,
            (&first, &identity),
            &[0., 1., -1., 2.],
            (&identity, &identity),
        )
        .unwrap();
        assert_eq!(
            output.iter().map(|v| v.position[0]).collect::<Vec<_>>(),
            [1., 11., -9., 21.]
        );
        assert!(output.iter().all(|v| v.normal == vertex().normal));
    }

    #[test]
    fn zero_weight_does_not_skip_unsupported_endpoint_arithmetic() {
        let identity = SMatrix::identity();
        let mut first = identity;
        first.elements[12] = f32::INFINITY;
        assert_eq!(
            blend_skin_vertices(
                &[vertex()],
                (&first, &identity),
                &[0.],
                (&identity, &identity)
            ),
            Err(DeformationUnavailable::NumericDomain)
        );
    }

    #[test]
    fn snapshot_preserves_group_order_secondary_bind_and_raw_suffix() {
        let mut bind = SMatrix::identity();
        bind.elements[12] = 2.;
        let groups = [
            SkinGroup {
                bind_matrix: bind,
                rigid_count: 1,
                blend_joint: Some(1),
                weights: &[0.25],
            },
            SkinGroup {
                bind_matrix: SMatrix::identity(),
                rigid_count: 0,
                blend_joint: None,
                weights: &[0.5],
            },
        ];
        let mut a = SMatrix::identity();
        a.elements[0] = 2.;
        let mut b = SMatrix::identity();
        b.elements[12] = 10.;
        let suffix = SkinVertex {
            position: [f32::from_bits(0x7fc01234), -0., f32::INFINITY],
            normal: [-0., f32::from_bits(1), 1.],
        };
        let result =
            deform_singlemesh_lod(&[vertex(), vertex(), vertex(), suffix], &groups, 1, &[a, b])
                .unwrap();
        assert_eq!(result[0].position, [6., 2., 3.]);
        assert_eq!(result[1].position, [9.75, 2., 3.]);
        assert_eq!(result[2].position, [6., 2., 3.]);
        assert_eq!(
            result[3].position.map(f32::to_bits),
            suffix.position.map(f32::to_bits)
        );
        assert_eq!(
            result[3].normal.map(f32::to_bits),
            suffix.normal.map(f32::to_bits)
        );
    }

    #[test]
    fn empty_groups_do_not_read_matrices_or_blend_indices() {
        let unused = SMatrix {
            elements: [f32::NAN; 16],
        };
        let group = SkinGroup {
            bind_matrix: unused,
            rigid_count: 0,
            blend_joint: Some(usize::MAX),
            weights: &[],
        };
        assert_eq!(
            deform_singlemesh_lod(&[], &[group], 0, &[unused]).unwrap(),
            []
        );
    }

    #[test]
    fn malformed_counts_and_indices_fail_closed() {
        let identity = SMatrix::identity();
        let mut group = SkinGroup {
            bind_matrix: identity,
            rigid_count: 0,
            blend_joint: Some(1),
            weights: &[0.5],
        };
        assert_eq!(
            deform_singlemesh_lod(&[vertex()], &[group], 0, &[identity]),
            Err(DeformationUnavailable::BlendJointIndex)
        );
        group.rigid_count = usize::MAX;
        assert_eq!(
            deform_singlemesh_lod(&[], &[group], 0, &[identity]),
            Err(DeformationUnavailable::VertexCount)
        );
        assert_eq!(
            deform_singlemesh_lod(&[], &[group], 0, &[]),
            Err(DeformationUnavailable::JointCount)
        );
        assert_eq!(
            transform_skin_vertices(&[], &identity, &identity),
            Err(DeformationUnavailable::EmptyInput)
        );
        assert_eq!(
            blend_skin_vertices(
                &[vertex()],
                (&identity, &identity),
                &[],
                (&identity, &identity)
            ),
            Err(DeformationUnavailable::WeightCount)
        );
    }

    #[test]
    fn arithmetic_guards_extended_exponents_and_retains_native_association() {
        let identity = SMatrix::identity();
        let mut matrix = identity;
        matrix.elements[0] = 100_000_000.;
        matrix.elements[4] = -100_000_000.;
        matrix.elements[8] = 1.;
        matrix.elements[12] = 1.;
        let source = SkinVertex {
            position: [1.; 3],
            ..vertex()
        };
        assert_eq!(
            transform_skin_vertices(&[source], &matrix, &identity).unwrap()[0].position[0],
            1.
        );
        matrix.elements[0] = f32::MIN_POSITIVE;
        let source = SkinVertex {
            position: [0.5, 0., 0.],
            ..vertex()
        };
        assert_eq!(
            transform_skin_vertices(&[source], &matrix, &identity),
            Err(DeformationUnavailable::NumericDomain)
        );
    }
}
