//! Owned model/frame materialization for the validated mesh, dummy and joint subset.
//! Singlemesh admission retains binding fields and requires a separate checked
//! pose snapshot and populated-definition bounds before rendering.
//! LS3DF RVAs 0x33680/0x33720/0x33500 define model ownership; 0x603e0
//! transfers loaded frames in file order, including a successfully loaded prefix
//! before an error. File lookup, animation and GPU resource creation are separate.
use crate::frame_transform::{FrameError, FrameKey, FrameKind, I3dFrameGraph, I3dFrameState};
use crate::joint_state::{I3dJointState, SinglemeshInvalidationState};
use crate::native_transform::{SMatrix, SQuat};
use crate::singlemesh_pose::{SinglemeshPose, SkinPoseError};
use lhr_assets::Model;
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelKey(FrameKey);
impl ModelKey {
    pub fn root(self) -> FrameKey {
        self.0
    }
}
#[derive(Clone, Debug)]
pub struct LoadedFrame {
    pub key: FrameKey,
    pub references: u32,
    pub owner: Option<ModelKey>,
    pub source_index: Option<usize>,
    pub name_bytes: Vec<u8>,
    pub properties: Vec<u8>,
    pub dummy_bounds: Option<[[f32; 3]; 2]>,
}
#[derive(Debug)]
pub struct LoadedModel {
    pub key: ModelKey,
    pub frames: Vec<FrameKey>,
    native_capacity: usize,
    growth: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelError {
    Frame(FrameError),
    UnknownModel(ModelKey),
    UnknownFrame(FrameKey),
    UnsupportedFrame {
        index: usize,
        kind: u8,
        visual: Option<u8>,
    },
    MissingDummyBounds(usize),
    MissingJointRecord(usize),
    MissingSkinRecord(usize),
    SkinPose(SkinPoseError),
    InvalidParent {
        index: usize,
        parent: usize,
    },
    ReferenceOverflow(FrameKey),
    DestructionRequired(FrameKey),
    FrameNotOwned {
        model: ModelKey,
        frame: FrameKey,
    },
    AllocationOverflow,
}
impl From<FrameError> for ModelError {
    fn from(e: FrameError) -> Self {
        Self::Frame(e)
    }
}
impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "model materialization: {self:?}")
    }
}
impl std::error::Error for ModelError {}

/// Stable keys are never reused. The graph owns transform caches; this arena
/// owns reference counts, model membership and original common-frame metadata.
#[derive(Debug, Default)]
pub struct ModelArena {
    graph: I3dFrameGraph,
    frames: Vec<LoadedFrame>,
    models: Vec<LoadedModel>,
}
impl ModelArena {
    fn insert(
        &mut self,
        state: I3dFrameState,
        source_index: Option<usize>,
        name_bytes: Vec<u8>,
        properties: Vec<u8>,
        dummy_bounds: Option<[[f32; 3]; 2]>,
    ) -> Result<FrameKey, ModelError> {
        let key = if state.kind == FrameKind::Singlemesh {
            // PC constructor RVA0x54240 initializes this counter to -300.
            // This is only its binding/invalidation field projection; mesh
            // data remains owned by the parsed source, not a native pointer.
            self.graph.insert_singlemesh_binding(
                state,
                SinglemeshInvalidationState {
                    joints_changed: (-300_i32) as u32,
                },
                true,
            )?
        } else {
            self.graph.insert(state)?
        };
        self.frames.push(LoadedFrame {
            key,
            references: 1,
            owner: None,
            source_index,
            name_bytes,
            properties,
            dummy_bounds,
        });
        Ok(key)
    }
    pub fn create_model(&mut self) -> Result<ModelKey, ModelError> {
        let key =
            ModelKey(self.insert(I3dFrameState::model(), None, Vec::new(), Vec::new(), None)?);
        self.models.push(LoadedModel {
            key,
            frames: Vec::new(),
            native_capacity: 0,
            growth: 32,
        });
        Ok(key)
    }
    fn frame_index(&self, key: FrameKey) -> Result<usize, ModelError> {
        let i = key.0.checked_sub(1).ok_or(ModelError::UnknownFrame(key))? as usize;
        self.frames
            .get(i)
            .filter(|f| f.key == key)
            .map(|_| i)
            .ok_or(ModelError::UnknownFrame(key))
    }
    fn model_index(&self, key: ModelKey) -> Result<usize, ModelError> {
        self.models
            .iter()
            .position(|m| m.key == key)
            .ok_or(ModelError::UnknownModel(key))
    }
    pub fn frame(&self, key: FrameKey) -> Result<&LoadedFrame, ModelError> {
        Ok(&self.frames[self.frame_index(key)?])
    }
    pub fn model(&self, key: ModelKey) -> Result<&LoadedModel, ModelError> {
        Ok(&self.models[self.model_index(key)?])
    }
    pub fn state(&self, key: FrameKey) -> Result<&I3dFrameState, ModelError> {
        self.frame_index(key)?;
        Ok(self.graph.state(key)?)
    }
    pub fn joint_state(&self, key: FrameKey) -> Result<&I3dJointState, ModelError> {
        self.frame_index(key)?;
        Ok(self.graph.joint_state(key)?)
    }
    pub fn singlemesh_bound_volume(
        &self,
        key: FrameKey,
    ) -> Result<crate::singlemesh_bounds::SinglemeshBoundVolume, ModelError> {
        self.frame_index(key)?;
        Ok(self.graph.singlemesh_bound_volume(key)?)
    }
    pub fn retain(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let i = self.frame_index(key)?;
        self.frames[i].references = self.frames[i]
            .references
            .checked_add(1)
            .ok_or(ModelError::ReferenceOverflow(key))?;
        Ok(())
    }
    /// Zero-count object destruction needs its derived destructor and is an
    /// explicit dependency. On rejection, no reference count is changed.
    pub fn release(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let i = self.frame_index(key)?;
        if self.frames[i].references <= 1 {
            return Err(ModelError::DestructionRequired(key));
        }
        self.frames[i].references -= 1;
        Ok(())
    }
    pub fn add_frame(&mut self, model: ModelKey, key: FrameKey) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        let fi = self.frame_index(key)?;
        self.frames[fi]
            .references
            .checked_add(1)
            .ok_or(ModelError::ReferenceOverflow(key))?;
        let required = self.models[mi]
            .frames
            .len()
            .checked_add(1)
            .ok_or(ModelError::AllocationOverflow)?;
        if required > self.models[mi].native_capacity {
            let capacity = self.models[mi]
                .native_capacity
                .checked_add(self.models[mi].growth)
                .ok_or(ModelError::AllocationOverflow)?;
            if capacity < required {
                return Err(ModelError::AllocationOverflow);
            }
            let additional = required.saturating_sub(self.models[mi].frames.len());
            self.models[mi]
                .frames
                .try_reserve(additional)
                .map_err(|_| ModelError::AllocationOverflow)?;
        }
        // Original AddFrame increments first, so moving the sole model reference
        // through DeleteFromOwner cannot destroy the frame.
        self.retain(key)?;
        if let Some(previous) = self.frames[fi].owner {
            self.delete_frame(previous, key)?;
        }
        if self.models[mi].frames.len() >= self.models[mi].native_capacity {
            self.models[mi].native_capacity += self.models[mi].growth;
        }
        self.models[mi].frames.push(key);
        self.frames[fi].owner = Some(model);
        Ok(())
    }
    pub fn delete_frame(&mut self, model: ModelKey, key: FrameKey) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        let fi = self.frame_index(key)?;
        let position = self.models[mi]
            .frames
            .iter()
            .position(|&k| k == key)
            .ok_or(ModelError::FrameNotOwned { model, frame: key })?;
        if self.frames[fi].references <= 1 {
            return Err(ModelError::DestructionRequired(key));
        }
        self.models[mi].frames.swap_remove(position);
        self.frames[fi].owner = None;
        self.frames[fi].references -= 1;
        Ok(())
    }
    /// Close's ownership loop is represented only when every child has another
    /// live reference. Animation deletion, filename sharing and derived frame
    /// destruction require their concrete backends and are not fabricated here.
    pub fn close(&mut self, model: ModelKey) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        for &key in &self.models[mi].frames {
            if self.frame(key)?.references <= 1 {
                return Err(ModelError::DestructionRequired(key));
            }
        }
        for &key in &self.models[mi].frames {
            let fi = self.frame_index(key)?;
            self.frames[fi].owner = None;
            self.frames[fi].references -= 1;
        }
        self.models[mi].frames.clear();
        self.graph.discard_hr_bound_vol(model.root())?;
        Ok(())
    }
    pub fn world_matrix(&mut self, key: FrameKey) -> Result<SMatrix, ModelError> {
        self.frame_index(key)?;
        Ok(self.graph.world_matrix(key)?)
    }
    pub fn storage_bytes(&self) -> Result<usize, ModelError> {
        let mut size = self.graph.storage_bytes()?;
        let mut add = |n: usize| {
            size = size.checked_add(n).ok_or(ModelError::AllocationOverflow)?;
            Ok::<_, ModelError>(())
        };
        add(self
            .frames
            .capacity()
            .checked_mul(std::mem::size_of::<LoadedFrame>())
            .ok_or(ModelError::AllocationOverflow)?)?;
        add(self
            .models
            .capacity()
            .checked_mul(std::mem::size_of::<LoadedModel>())
            .ok_or(ModelError::AllocationOverflow)?)?;
        for f in &self.frames {
            add(f.name_bytes.capacity())?;
            add(f.properties.capacity())?;
        }
        for m in &self.models {
            add(m
                .frames
                .capacity()
                .checked_mul(std::mem::size_of::<FrameKey>())
                .ok_or(ModelError::AllocationOverflow)?)?;
        }
        Ok(size)
    }
}

/// A failure retains every frame already constructed and owned by the model.
#[derive(Debug)]
pub struct ModelLoadFailure {
    pub error: ModelError,
    pub partial: Box<MaterializedModel>,
}
impl fmt::Display for ModelLoadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for ModelLoadFailure {}
#[derive(Debug)]
pub struct MaterializedModel {
    source: Arc<Model>,
    arena: ModelArena,
    root: ModelKey,
    source_frame_keys: Vec<FrameKey>,
    skin_pose: Option<SinglemeshPose>,
    complete: bool,
}
impl MaterializedModel {
    /// Parsed, complete common records only; names/properties retain original
    /// bytes. Native file I/O, animation, derived mesh allocation and other
    /// frame variants are outside this materialization boundary.
    pub fn from_parsed(source: Arc<Model>) -> Result<Self, ModelLoadFailure> {
        let mut arena = ModelArena::default();
        // A fresh arena always has room for its first stable identity. Allocation
        // failure remains explicit rather than creating an empty successful model.
        let root = match arena.create_model() {
            Ok(root) => root,
            Err(error) => {
                return Err(ModelLoadFailure {
                    error,
                    partial: Box::new(Self {
                        source,
                        arena,
                        root: ModelKey(FrameKey(0)),
                        source_frame_keys: Vec::new(),
                        skin_pose: None,
                        complete: false,
                    }),
                });
            }
        };
        let mut result = Self {
            source,
            arena,
            root,
            source_frame_keys: Vec::new(),
            skin_pose: None,
            complete: false,
        };
        for index in 0..result.source.frames.len() {
            if let Err(error) = result.load_frame(index) {
                let error = result.transfer_loaded_frames().err().unwrap_or(error);
                return Err(ModelLoadFailure {
                    error,
                    partial: Box::new(result),
                });
            }
        }
        if let Err(error) = result.transfer_loaded_frames() {
            return Err(ModelLoadFailure {
                error,
                partial: Box::new(result),
            });
        }
        if let Err(error) = result.arena.graph.mark_loaded_root(root.root()) {
            return Err(ModelLoadFailure {
                error: error.into(),
                partial: Box::new(result),
            });
        }
        result.complete = true;
        Ok(result)
    }
    fn load_frame(&mut self, index: usize) -> Result<(), ModelError> {
        let source = &self.source.frames[index];
        let mut state = match (source.kind, source.visual) {
            (1, Some(0 | 1)) => I3dFrameState::mesh(),
            (1, Some(2)) => {
                let mut state = I3dFrameState::mesh();
                state.kind = FrameKind::Singlemesh;
                state
            }
            (6, None) => I3dFrameState::dummy(),
            (10, None) => I3dFrameState::joint(),
            _ => {
                return Err(ModelError::UnsupportedFrame {
                    index,
                    kind: source.kind,
                    visual: source.visual,
                });
            }
        };
        if source.kind == 6 && source.dummy_bounds.is_none() {
            return Err(ModelError::MissingDummyBounds(index));
        }
        if source.kind == 10 && source.joint.is_none() {
            return Err(ModelError::MissingJointRecord(index));
        }
        if source.visual == Some(2)
            && (source.skin.is_none() || source.lods.is_empty() || source.instance.is_some())
        {
            return Err(ModelError::MissingSkinRecord(index));
        }
        if source.kind == 1 {
            state.visual_flags = u32::from(u16::from_le_bytes([
                source.render_flags,
                source.logic_flags,
            ]));
        }
        let parent = match source.parent {
            Some(parent) => Some(
                *self
                    .source_frame_keys
                    .get(parent)
                    .filter(|_| parent < index)
                    .ok_or(ModelError::InvalidParent { index, parent })?,
            ),
            None => None,
        };
        let key = self.arena.insert(
            state,
            Some(index),
            source.name_bytes.clone(),
            source.properties.clone(),
            source.dummy_bounds,
        )?;
        // LoadImage links earlier parents before storing the common local SRT.
        if parent.is_some() {
            self.arena.graph.link_to(key, parent)?;
        }
        self.arena.graph.apply_common_load_image_fields(
            key,
            source.position,
            source.scale,
            SQuat {
                wxyz: source.rotation_wxyz,
            },
            source.culling,
        )?;
        if source.kind == 10 {
            let joint = source.joint.as_ref().unwrap();
            self.arena.graph.apply_joint_load_image_fields(
                key,
                SMatrix {
                    elements: joint.bone_matrix,
                },
                joint.index,
            )?;
        }
        self.source_frame_keys.push(key);
        Ok(())
    }
    fn transfer_loaded_frames(&mut self) -> Result<(), ModelError> {
        for &key in &self.source_frame_keys {
            if self.arena.graph.links(key)?.parent.is_none() {
                self.arena.graph.link_to(key, Some(self.root.root()))?;
            }
            self.arena.add_frame(self.root, key)?;
            self.arena.release(key)?;
        }
        Ok(())
    }
    pub fn is_complete(&self) -> bool {
        self.complete
    }
    pub fn source(&self) -> &Arc<Model> {
        &self.source
    }
    pub fn root(&self) -> FrameKey {
        self.root.root()
    }
    pub fn source_frame_keys(&self) -> &[FrameKey] {
        &self.source_frame_keys
    }
    pub fn arena(&self) -> &ModelArena {
        &self.arena
    }
    pub fn arena_mut(&mut self) -> &mut ModelArena {
        &mut self.arena
    }
    pub fn world_matrices(&mut self) -> Result<Vec<SMatrix>, ModelError> {
        self.source_frame_keys
            .iter()
            .map(|&key| self.arena.world_matrix(key))
            .collect()
    }
    /// Build an immutable first-LOD CPU skin pose from the unchanged source and
    /// the owned Joint hierarchy, including the populated-definition bounds
    /// calculation before deformation. This does not bind an animation or
    /// implement native vertex-buffer management and rendering.
    pub fn prepare_skin_pose(&mut self) -> Result<(), ModelError> {
        if !self.complete {
            return Err(ModelError::SkinPose(SkinPoseError::Source(
                "incomplete model",
            )));
        }
        self.skin_pose = None;
        if self
            .source
            .frames
            .iter()
            .any(|frame| frame.visual == Some(2))
        {
            self.skin_pose = Some(
                SinglemeshPose::build(&self.source, &self.source_frame_keys, &mut self.arena.graph)
                    .map_err(ModelError::SkinPose)?,
            );
        }
        Ok(())
    }
    pub fn skin_pose(&self) -> Option<&SinglemeshPose> {
        self.skin_pose.as_ref()
    }
    /// Additional owned storage; source Arc<Model> payload is charged by caller.
    pub fn storage_bytes(&self) -> Result<usize, ModelError> {
        let bytes = self
            .arena
            .storage_bytes()?
            .checked_add(
                self.source_frame_keys
                    .capacity()
                    .checked_mul(std::mem::size_of::<FrameKey>())
                    .ok_or(ModelError::AllocationOverflow)?,
            )
            .ok_or(ModelError::AllocationOverflow)?;
        bytes
            .checked_add(match &self.skin_pose {
                Some(pose) => pose.storage_bytes().ok_or(ModelError::AllocationOverflow)?,
                None => 0,
            })
            .ok_or(ModelError::AllocationOverflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(kind: u8, parent: Option<usize>, position: [f32; 3]) -> lhr_assets::Frame {
        lhr_assets::Frame {
            kind,
            visual: (kind == 1).then_some(0),
            render_flags: 0x40,
            logic_flags: 0x21,
            culling: 0x11,
            name: "fixture".into(),
            name_bytes: b"fixture".to_vec(),
            properties: vec![0xff, 0, 0x80],
            dummy_bounds: (kind == 6).then_some([[0.; 3], [1.; 3]]),
            joint: None,
            skin: None,
            parent,
            position,
            scale: [1.; 3],
            rotation_wxyz: [1., 0., 0., 0.],
            instance: None,
            lods: Vec::new(),
        }
    }
    #[test]
    fn materializes_owned_hierarchy_and_original_bytes() {
        let mut parent = frame(6, None, [10., 0., 0.]);
        parent.scale = [2.; 3];
        parent.dummy_bounds.as_mut().unwrap()[0][0] = f32::from_bits(0x7fc01234);
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![parent, frame(1, Some(0), [1., 2., 3.])],
        });
        let mut loaded = MaterializedModel::from_parsed(source.clone()).unwrap();
        assert!(Arc::ptr_eq(&source, loaded.source()));
        assert!(loaded.is_complete());
        let keys = loaded.source_frame_keys().to_vec();
        let matrices = loaded.world_matrices().unwrap();
        assert_eq!(&matrices[1].elements[12..15], &[12., 4., 6.]);
        assert_eq!(
            loaded.arena.graph.links(keys[0]).unwrap().parent,
            Some(loaded.root())
        );
        assert_eq!(
            loaded.arena.graph.links(keys[1]).unwrap().parent,
            Some(keys[0])
        );
        for &key in &keys {
            let f = loaded.arena.frame(key).unwrap();
            assert_eq!(f.references, 1);
            assert_eq!(f.owner, Some(loaded.root));
            assert_eq!(f.properties, [0xff, 0, 0x80]);
        }
        assert_eq!(
            loaded.arena.frame(keys[0]).unwrap().dummy_bounds.unwrap()[0][0].to_bits(),
            0x7fc01234
        );
        assert!(loaded.storage_bytes().unwrap() > 0);
    }
    #[test]
    fn materializes_joint_hierarchy_without_inventing_binding_or_cache_values() {
        let mut first = frame(10, None, [10., 0., 0.]);
        first.scale = [2.; 3];
        let mut bone_matrix = SMatrix::identity().elements;
        bone_matrix[2] = f32::from_bits(0x7fc01234);
        bone_matrix[12] = -0.;
        first.joint = Some(lhr_assets::JointRecord {
            bone_matrix,
            index: u32::MAX,
        });
        let mut second = frame(10, Some(0), [1., 2., 3.]);
        second.joint = first.joint.clone();
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![first, second, frame(1, Some(1), [1., 0., 0.])],
        });
        let mut loaded = MaterializedModel::from_parsed(source).unwrap();
        assert!(loaded.is_complete());
        let keys = loaded.source_frame_keys().to_vec();
        let matrices = loaded.world_matrices().unwrap();
        assert_eq!(&matrices[1].elements[12..15], &[12., 4., 6.]);
        assert_eq!(&matrices[2].elements[12..15], &[14., 4., 6.]);
        for &key in &keys[..2] {
            let joint = loaded.arena.joint_state(key).unwrap();
            assert_eq!(joint.bone_matrix.elements[2].to_bits(), 0x7fc01234);
            assert_eq!(joint.bone_matrix.elements[12].to_bits(), 0x80000000);
            assert_eq!(joint.matrix_184.elements[0].to_bits(), 0x3c23d70a);
            assert_eq!(joint.index, u32::MAX);
            assert_eq!(joint.flags, 1);
            assert_eq!(joint.joint_matrix, None);
            assert_eq!(joint.singlemesh, None);
            assert_eq!(loaded.arena.frame(key).unwrap().references, 1);
            assert_eq!(loaded.arena.frame(key).unwrap().owner, Some(loaded.root));
        }
    }

    #[test]
    fn missing_joint_payload_preserves_prefix_and_never_claims_complete() {
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(6, None, [0.; 3]), frame(10, Some(0), [0.; 3])],
        });
        let failure = MaterializedModel::from_parsed(source).unwrap_err();
        assert_eq!(failure.error, ModelError::MissingJointRecord(1));
        assert!(!failure.partial.is_complete());
        assert_eq!(failure.partial.source_frame_keys().len(), 1);
        let key = failure.partial.source_frame_keys()[0];
        assert_eq!(
            failure.partial.arena.frame(key).unwrap().owner,
            Some(failure.partial.root)
        );
        assert_eq!(failure.partial.arena.frame(key).unwrap().references, 1);
    }

    fn skinned_source() -> Arc<Model> {
        let mut mesh = frame(1, None, [0.; 3]);
        mesh.visual = Some(2);
        mesh.render_flags = 0;
        mesh.lods.push(lhr_assets::Lod {
            distance: 0.,
            vertices: vec![
                lhr_assets::Vertex {
                    position: [1., 2., 3.],
                    normal: [0., 1., 0.],
                    uv: [0.25, 0.75],
                },
                lhr_assets::Vertex {
                    position: [9.; 3],
                    normal: [-0., 0., 1.],
                    uv: [1.; 2],
                },
            ],
            groups: vec![],
        });
        let bone = lhr_assets::SkinBoneRecord {
            matrix: SMatrix::identity().elements,
            rigid_vertex_count: 0,
            weighted_vertex_count: 1,
            blend_joint_index: 2,
            bounds: [-1., -1., -1., 1., 1., 1.],
            weights: vec![0.25],
        };
        let mut unused = bone.clone();
        unused.weighted_vertex_count = 0;
        unused.weights.clear();
        mesh.skin = Some(lhr_assets::SkinRecord {
            lods: vec![lhr_assets::SkinLodRecord {
                definition: Some(lhr_assets::SkinLodDefinition {
                    root_vertex_count: 1,
                    bounds: [-1., -1., -1., 1., 1., 1.],
                    bones: vec![bone, unused],
                }),
            }],
        });
        let mut a = frame(10, Some(0), [4., 0., 0.]);
        a.joint = Some(lhr_assets::JointRecord {
            bone_matrix: SMatrix::identity().elements,
            index: 0,
        });
        let mut b = frame(10, Some(0), [0., 8., 0.]);
        b.joint = Some(lhr_assets::JointRecord {
            bone_matrix: SMatrix::identity().elements,
            index: 1,
        });
        Arc::new(Model {
            materials: vec![],
            frames: vec![mesh, a, b],
        })
    }

    #[test]
    fn skin_pose_binds_original_indices_blends_and_preserves_source_and_suffix() {
        let source = skinned_source();
        let mut loaded = MaterializedModel::from_parsed(source.clone()).unwrap();
        assert!(loaded.skin_pose().is_none());
        loaded.prepare_skin_pose().unwrap();
        let pose = loaded.skin_pose().unwrap().vertices(0).unwrap();
        assert_eq!(pose[0].position, [2., 8., 3.]);
        assert_eq!(pose[0].normal, [0., 1., 0.]);
        assert_eq!(pose[1].position, [9.; 3]);
        assert_eq!(pose[1].normal[0].to_bits(), (-0.0_f32).to_bits());
        assert_eq!(source.frames[0].lods[0].vertices[0].position, [1., 2., 3.]);
        assert_eq!(source.frames[0].lods[0].vertices[0].uv, [0.25, 0.75]);
        let target = loaded.source_frame_keys()[0];
        let bounds = loaded.arena.singlemesh_bound_volume(target).unwrap();
        assert_eq!(bounds.bbox.min, [-1.; 3]);
        assert_eq!(bounds.bbox.max, [5., 9., 1.]);
        assert_eq!(bounds.center, [2., 4., 0.]);
        assert_eq!(bounds.radius.to_bits(), 35_f32.sqrt().to_bits());
        assert_eq!(
            loaded
                .arena
                .joint_state(loaded.source_frame_keys()[1])
                .unwrap()
                .singlemesh,
            Some(target)
        );
        assert!(loaded.skin_pose().unwrap().vertices(1).is_none());
        loaded.prepare_skin_pose().unwrap();
        assert_eq!(
            loaded.skin_pose().unwrap().vertices(0).unwrap()[0].position,
            [2., 8., 3.]
        );
        assert_eq!(
            loaded.arena.singlemesh_bound_volume(target).unwrap(),
            bounds
        );
    }

    #[test]
    fn skin_pose_never_publishes_missing_joint_or_malformed_partition_fallback() {
        let mut source = skinned_source();
        Arc::make_mut(&mut source).frames[2]
            .joint
            .as_mut()
            .unwrap()
            .index = 2;
        let mut loaded = MaterializedModel::from_parsed(source).unwrap();
        assert!(loaded.prepare_skin_pose().is_err());
        assert!(loaded.skin_pose().is_none());
        let mut source = skinned_source();
        Arc::make_mut(&mut source).frames[0]
            .skin
            .as_mut()
            .unwrap()
            .lods[0]
            .definition
            .as_mut()
            .unwrap()
            .root_vertex_count = 0;
        let mut loaded = MaterializedModel::from_parsed(source).unwrap();
        assert!(loaded.prepare_skin_pose().is_err());
        assert!(loaded.skin_pose().is_none());
    }
    #[test]
    fn unsupported_frame_preserves_loaded_prefix_ownership() {
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(6, None, [0.; 3]), frame(5, None, [0.; 3])],
        });
        let failure = MaterializedModel::from_parsed(source).unwrap_err();
        assert_eq!(
            failure.error,
            ModelError::UnsupportedFrame {
                index: 1,
                kind: 5,
                visual: None
            }
        );
        let partial = failure.partial;
        assert!(!partial.is_complete());
        assert_eq!(partial.source_frame_keys().len(), 1);
        let key = partial.source_frame_keys()[0];
        assert_eq!(partial.arena.frame(key).unwrap().owner, Some(partial.root));
        assert_eq!(partial.arena.frame(key).unwrap().references, 1);
        assert_eq!(
            partial.arena.graph.links(key).unwrap().parent,
            Some(partial.root())
        );
        assert_eq!(partial.arena.state(partial.root()).unwrap().update_count, 0);
    }
    #[test]
    fn transfers_last_owner_without_destroying_then_requires_real_destructor() {
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(6, None, [0.; 3]), frame(1, None, [0.; 3])],
        });
        let mut loaded = MaterializedModel::from_parsed(source).unwrap();
        let key = loaded.source_frame_keys()[0];
        let next = loaded.arena.create_model().unwrap();
        loaded.arena.add_frame(next, key).unwrap();
        assert_eq!(
            loaded.arena.model(loaded.root).unwrap().frames,
            [loaded.source_frame_keys()[1]]
        );
        assert_eq!(loaded.arena.frame(key).unwrap().owner, Some(next));
        assert_eq!(loaded.arena.frame(key).unwrap().references, 1);
        assert_eq!(
            loaded.arena.close(next),
            Err(ModelError::DestructionRequired(key))
        );
        assert_eq!(loaded.arena.frame(key).unwrap().owner, Some(next));
        loaded.arena.retain(key).unwrap();
        loaded.arena.close(next).unwrap();
        assert_eq!(loaded.arena.frame(key).unwrap().owner, None);
        assert_eq!(loaded.arena.frame(key).unwrap().references, 1);
    }
    #[test]
    fn validates_parent_without_returning_empty_success() {
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(1, Some(1), [0.; 3])],
        });
        let failure = MaterializedModel::from_parsed(source).unwrap_err();
        assert_eq!(
            failure.error,
            ModelError::InvalidParent {
                index: 0,
                parent: 1
            }
        );
        assert!(failure.partial.source_frame_keys().is_empty());
        assert!(!failure.partial.is_complete());
    }
}
