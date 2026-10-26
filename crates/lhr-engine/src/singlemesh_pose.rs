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
    singlemorph_state::{SinglemorphError, SinglemorphState},
};
use lhr_assets::Model;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkinPoseError {
    Source(&'static str),
    Frame(FrameError),
    Deformation(DeformationUnavailable),
    Morph(SinglemorphError),
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
impl From<SinglemorphError> for SkinPoseError {
    fn from(value: SinglemorphError) -> Self {
        Self::Morph(value)
    }
}

/// Owned first-LOD source buffer retained across the original morph dirty gate.
/// Joint animation reskins this buffer; it must not replace a clean morph result
/// with the original, unmorphed source vertices.
#[derive(Debug)]
pub(crate) struct MorphPose {
    state: SinglemorphState,
    vertices: Vec<lhr_assets::Vertex>,
}
impl MorphPose {
    pub(crate) fn from_frame(frame: &lhr_assets::Frame) -> Result<Self, SkinPoseError> {
        let record = frame
            .morph
            .as_ref()
            .ok_or(SkinPoseError::Source("morph record"))?;
        if record
            .definition
            .as_ref()
            .is_some_and(|definition| definition.lods.len() > frame.lods.len())
        {
            return Err(SkinPoseError::Source("morph/geometry LOD count"));
        }
        let lod = frame
            .lods
            .first()
            .ok_or(SkinPoseError::Source("morph first LOD"))?;
        Ok(Self {
            state: SinglemorphState::from_record(record)?,
            vertices: lod.vertices.clone(),
        })
    }
    pub(crate) fn storage_bytes(&self) -> Option<usize> {
        self.vertices
            .capacity()
            .checked_mul(std::mem::size_of::<lhr_assets::Vertex>())?
            .checked_add(self.state.storage_bytes())
    }
}

#[derive(Debug)]
pub struct SinglemeshPose {
    vertices: Vec<Option<Vec<SkinVertex>>>,
    // Nonzero geometry LODs are evaluated only after the live render selector
    // chooses them. Each slot owns the result for its own skin definition.
    extra_lods: Vec<Vec<Option<Vec<SkinVertex>>>>,
}
impl SinglemeshPose {
    /// Only successful complete pose snapshots are published. Failed numerical
    /// preparation can retain earlier graph/cache writes; it is not a native
    /// UpdateLodVertices failure simulation.
    pub(crate) fn build(
        source: &Model,
        keys: &[FrameKey],
        graph: &mut I3dFrameGraph,
        morphs: &mut [Option<MorphPose>],
    ) -> Result<Self, SkinPoseError> {
        if source.frames.len() != keys.len() || source.frames.len() != morphs.len() {
            return Err(SkinPoseError::Source("frame identity count"));
        }
        let mut output = Self {
            vertices: (0..source.frames.len()).map(|_| None).collect(),
            extra_lods: source
                .frames
                .iter()
                .map(|frame| (1..frame.lods.len()).map(|_| None).collect())
                .collect(),
        };
        for (index, frame) in source.frames.iter().enumerate() {
            if !matches!((frame.kind, frame.visual), (1, Some(2 | 3))) {
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
            if graph.singlemesh_binding(keys[index])?.joints().is_empty() {
                if !graph.d_build_loaded_singlemesh(
                    keys[index],
                    bbox(definition.bounds),
                    &joint_boxes,
                )? {
                    return Err(SkinPoseError::Source(
                        "singlemesh has no mesh or descendant joints",
                    ));
                }
            } else {
                graph.update_singlemesh_loaded_bounds_cached(
                    keys[index],
                    bbox(definition.bounds),
                    &joint_boxes,
                )?;
            }
            // Original singlemorph UpdateLodVertices (0x57590) updates its
            // shared morph buffer before invoking inherited skin deformation.
            // Initial binding uses DBuild. Later host pose requests use the
            // original bounds virtual wrapper and its Joint-change cache gate.
            let source_vertices = if frame.visual == Some(3) {
                let cache = morphs[index]
                    .as_mut()
                    .ok_or(SkinPoseError::Source("owned morph buffer"))?;
                let record = frame
                    .morph
                    .as_ref()
                    .ok_or(SkinPoseError::Source("morph record"))?;
                cache
                    .state
                    .update_morph_lod(record, 0, &mut cache.vertices)?;
                &cache.vertices
            } else {
                if frame.morph.is_some() || morphs[index].is_some() {
                    return Err(SkinPoseError::Source("morph on non-singlemorph frame"));
                }
                &lod.vertices
            };
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
            let vertices: Vec<_> = source_vertices
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

    /// Evaluate a selected nonzero LOD from its own source vertices and skin
    /// definition. The first-LOD binding and bounds remain the live owner; no
    /// geometry, UV, or material is copied from another LOD. A morph definition
    /// for this higher LOD needs a separate native dirty-buffer reconstruction.
    pub(crate) fn prepare_selected_lod(
        &mut self,
        source: &Model,
        frame_index: usize,
        lod_index: usize,
        key: FrameKey,
        graph: &mut I3dFrameGraph,
    ) -> Result<(), SkinPoseError> {
        if lod_index == 0 {
            return self
                .vertices(frame_index)
                .map(|_| ())
                .ok_or(SkinPoseError::Source("first pose unavailable"));
        }
        let frame = source
            .frames
            .get(frame_index)
            .ok_or(SkinPoseError::Source("selected frame index"))?;
        if !matches!((frame.kind, frame.visual), (1, Some(2 | 3)))
            || frame.instance.is_some()
            || frame.render_flags & 0x40 != 0
        {
            return Err(SkinPoseError::Source("selected singlemesh owner"));
        }
        let slots = self
            .extra_lods
            .get_mut(frame_index)
            .ok_or(SkinPoseError::Source("selected pose frame"))?;
        let slot = slots
            .get_mut(lod_index - 1)
            .ok_or(SkinPoseError::Source("selected pose LOD"))?;
        if slot.is_some() {
            return Ok(());
        }
        let lod = frame
            .lods
            .get(lod_index)
            .ok_or(SkinPoseError::Source("selected geometry LOD"))?;
        let skin = frame
            .skin
            .as_ref()
            .ok_or(SkinPoseError::Source("selected skin record"))?;
        if skin.lods.len() != frame.lods.len() {
            return Err(SkinPoseError::Source("skin/geometry LOD count"));
        }
        let definition = skin
            .lods
            .get(lod_index)
            .and_then(|lod| lod.definition.as_ref())
            .ok_or(SkinPoseError::Source(
                "selected skin definition unavailable",
            ))?;
        if definition.bones.iter().any(|bone| {
            bone.weighted_vertex_count as usize != bone.weights.len()
                || (!bone.weights.is_empty()
                    && bone.blend_joint_index as usize > definition.bones.len())
        }) {
            return Err(SkinPoseError::Source("weight count or blend reference"));
        }
        if frame.visual == Some(3) {
            let record = frame
                .morph
                .as_ref()
                .ok_or(SkinPoseError::Source("selected morph record"))?;
            if record
                .definition
                .as_ref()
                .is_some_and(|definition| definition.lods.get(lod_index).is_some())
            {
                return Err(SkinPoseError::Source(
                    "selected morph LOD dirty buffer unavailable",
                ));
            }
        } else if frame.morph.is_some() {
            return Err(SkinPoseError::Source("morph on non-singlemorph frame"));
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
        let joints = graph
            .singlemesh_binding(key)?
            .joints()
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()
            .ok_or(SkinPoseError::Source("unwritten native joint slot"))?;
        if joints.len() != groups.len() {
            return Err(SkinPoseError::Source("bound joint/definition count"));
        }
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
        *slot = Some(deform_singlemesh_lod(
            &vertices,
            &groups,
            definition.root_vertex_count as usize,
            &matrices,
        )?);
        Ok(())
    }

    pub fn vertices(&self, frame_index: usize) -> Option<&[SkinVertex]> {
        self.vertices.get(frame_index).and_then(Option::as_deref)
    }

    pub(crate) fn selected_vertices(
        &self,
        frame_index: usize,
        lod_index: usize,
    ) -> Option<&[SkinVertex]> {
        if lod_index == 0 {
            return self.vertices(frame_index);
        }
        self.extra_lods
            .get(frame_index)?
            .get(lod_index - 1)?
            .as_deref()
    }

    pub fn storage_bytes(&self) -> Option<usize> {
        let first = self.vertices.iter().flatten().try_fold(
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
        )?;
        self.extra_lods.iter().try_fold(
            first.checked_add(
                self.extra_lods
                    .capacity()
                    .checked_mul(std::mem::size_of::<Vec<Option<Vec<SkinVertex>>>>())?,
            )?,
            |bytes, slots| {
                slots.iter().flatten().try_fold(
                    bytes.checked_add(
                        slots
                            .capacity()
                            .checked_mul(std::mem::size_of::<Option<Vec<SkinVertex>>>())?,
                    )?,
                    |bytes, vertices| {
                        bytes.checked_add(
                            vertices
                                .capacity()
                                .checked_mul(std::mem::size_of::<SkinVertex>())?,
                        )
                    },
                )
            },
        )
    }
}
