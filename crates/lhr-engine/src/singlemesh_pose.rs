//! Original-data first-LOD pose inspection. This joins owned frame binding and
//! checked CPU deformation and the populated-definition bounds path. Animation
//! and native rendering backends remain separate. Source data is never modified.
use crate::{
    frame_transform::{FrameError, FrameKey, I3dFrameGraph},
    native_transform::SMatrix,
    singlemesh_bounds::SinglemeshBoundingBox,
    singlemesh_deformation::{
        deform_singlemesh_lod, DeformationUnavailable, SkinGroup, SkinVertex,
    },
};
use lhr_assets::Model;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkinPoseError {
    Source(&'static str),
    Frame(FrameError),
    Deformation(DeformationUnavailable),
}
impl From<FrameError> for SkinPoseError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}
impl From<DeformationUnavailable> for SkinPoseError {
    fn from(value: DeformationUnavailable) -> Self {
        Self::Deformation(value)
    }
}

#[derive(Debug)]
pub struct SinglemeshPose {
    vertices: Vec<Option<Vec<SkinVertex>>>,
}
impl SinglemeshPose {
    /// Only successful complete pose snapshots are published. Failed numerical
    /// preparation can retain earlier graph/cache writes; it is not a native
    /// UpdateLodVertices failure simulation.
    pub(crate) fn build(
        source: &Model,
        keys: &[FrameKey],
        graph: &mut I3dFrameGraph,
    ) -> Result<Self, SkinPoseError> {
        if source.frames.len() != keys.len() {
            return Err(SkinPoseError::Source("frame identity count"));
        }
        let mut output = Self {
            vertices: (0..source.frames.len()).map(|_| None).collect(),
        };
        for (index, frame) in source.frames.iter().enumerate() {
            if (frame.kind, frame.visual) != (1, Some(2)) {
                if frame.skin.is_some() {
                    return Err(SkinPoseError::Source("skin on a non-singlemesh frame"));
                }
                continue;
            }
            if frame.instance.is_some() || frame.render_flags & 0x40 != 0 {
                return Err(SkinPoseError::Source("instance or world-space singlemesh"));
            }
            let skin = frame
                .skin
                .as_ref()
                .ok_or(SkinPoseError::Source("skin record"))?;
            if skin.lods.len() != frame.lods.len() {
                return Err(SkinPoseError::Source("skin/geometry LOD count"));
            }
            let lod = frame
                .lods
                .first()
                .ok_or(SkinPoseError::Source("first LOD"))?;
            let definition = skin
                .lods
                .first()
                .and_then(|lod| lod.definition.as_ref())
                .ok_or(SkinPoseError::Source("first skin definition unavailable"))?;
            if definition.bones.iter().any(|bone| {
                bone.weighted_vertex_count as usize != bone.weights.len()
                    || (!bone.weights.is_empty()
                        && bone.blend_joint_index as usize > definition.bones.len())
            }) {
                return Err(SkinPoseError::Source("weight count or blend reference"));
            }
            let groups: Vec<_> = definition
                .bones
                .iter()
                .map(|bone| SkinGroup {
                    bind_matrix: SMatrix {
                        elements: bone.matrix,
                    },
                    rigid_count: bone.rigid_vertex_count as usize,
                    blend_joint: bone
                        .blend_joint_index
                        .checked_sub(1)
                        .map(|index| index as usize),
                    weights: &bone.weights,
                })
                .collect();
            let bbox = |raw: [f32; 6]| SinglemeshBoundingBox {
                min: [raw[0], raw[1], raw[2]],
                max: [raw[3], raw[4], raw[5]],
            };
            let joint_boxes: Vec<_> = definition
                .bones
                .iter()
                .map(|bone| bbox(bone.bounds))
                .collect();
            if !graph.d_build_loaded_singlemesh(
                keys[index],
                bbox(definition.bounds),
                &joint_boxes,
            )? {
                return Err(SkinPoseError::Source(
                    "singlemesh has no mesh or descendant joints",
                ));
            }
            let joints: Vec<_> = graph
                .singlemesh_binding(keys[index])?
                .joints()
                .iter()
                .copied()
                .collect::<Option<Vec<_>>>()
                .ok_or(SkinPoseError::Source("unwritten native joint slot"))?;
            if joints.len() != groups.len() {
                return Err(SkinPoseError::Source("bound joint/definition count"));
            }
            // Preserve native first-use order: primary matrix, then its blend
            // target. A cached joint ancestor changes multiplication grouping.
            // Empty groups do not request their matrix. Unused palette slots
            // below are never consumed by the checked arithmetic routine.
            let mut matrices = vec![None; joints.len()];
            for (group_index, group) in groups.iter().enumerate() {
                if group.rigid_count != 0 || !group.weights.is_empty() {
                    matrices[group_index] = Some(graph.get_joint_matrix(joints[group_index])?);
                }
                if !group.weights.is_empty() {
                    if let Some(other) = group.blend_joint {
                        matrices[other] = Some(graph.get_joint_matrix(joints[other])?);
                    }
                }
            }
            let matrices: Vec<_> = matrices
                .into_iter()
                .map(|value| value.unwrap_or(SMatrix::identity()))
                .collect();
            let vertices: Vec<_> = lod
                .vertices
                .iter()
                .map(|vertex| SkinVertex {
                    position: vertex.position,
                    normal: vertex.normal,
                })
                .collect();
            output.vertices[index] = Some(deform_singlemesh_lod(
                &vertices,
                &groups,
                definition.root_vertex_count as usize,
                &matrices,
            )?);
        }
        Ok(output)
    }

    pub fn vertices(&self, frame_index: usize) -> Option<&[SkinVertex]> {
        self.vertices.get(frame_index).and_then(Option::as_deref)
    }

    pub fn storage_bytes(&self) -> Option<usize> {
        self.vertices.iter().flatten().try_fold(
            self.vertices
                .capacity()
                .checked_mul(std::mem::size_of::<Option<Vec<SkinVertex>>>())?,
            |bytes, vertices| {
                bytes.checked_add(
                    vertices
                        .capacity()
                        .checked_mul(std::mem::size_of::<SkinVertex>())?,
                )
            },
        )
    }
}
