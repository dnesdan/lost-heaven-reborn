//! Owned model/frame materialization for the validated mesh, dummy and joint subset.
//! Singlemesh admission retains binding fields and requires a separate checked
//! pose snapshot and populated-definition bounds before rendering.
//! LS3DF RVAs 0x33680/0x33720/0x33500 define model ownership; 0x603e0
//! transfers loaded frames in file order, including a successfully loaded prefix
//! before an error. Persistent parsed Close/reopen keeps root identity and uses
//! nonreused tombstones for destroyed model/dummy/joint/Target objects. Visual
//! resource destruction remains explicit. File lookup, native filename sharing,
//! animation ownership and GPU resource creation are separate.
use crate::frame_transform::{FrameError, FrameKey, FrameKind, I3dFrameGraph, I3dFrameState};
use crate::joint_state::{I3dJointState, SinglemeshInvalidationState};
use crate::native_transform::{SMatrix, SQuat};
use crate::singlemesh_pose::{MorphPose, SinglemeshPose, SkinPoseError};
use crate::target_state::{I3dTargetState, TargetError};
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
    /// Immutable host backing, independent of model membership. This is not a
    /// native mesh/LOD resource allocation or filename reference count.
    pub source: Option<Arc<Model>>,
    alive: bool,
}
#[derive(Debug)]
pub struct LoadedModel {
    pub key: ModelKey,
    pub frames: Vec<FrameKey>,
    native_capacity: usize,
    growth: usize,
    lifecycle_epoch: u64,
    close_cursor: usize,
    closing: bool,
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
    MissingTargetRecord(usize),
    Target(TargetError),
    SkinPose(SkinPoseError),
    Animation(crate::skeletal_animation::SkeletalAnimationError),
    AnimationIdentity,
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
    IncompleteModel,
    CloseIncomplete(ModelKey),
    OwnershipCycle,
    NestedDestructionUnavailable(FrameKey),
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
    destruction_depth: usize,
    lookup_owner: crate::frame_lookup::FrameLookupOwner,
}
impl ModelArena {
    pub(crate) fn lookup_reference(
        &self,
        key: FrameKey,
    ) -> crate::script_scheduler::FrameReferenceId {
        self.lookup_owner.reference(key.0)
    }
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
            source: None,
            alive: true,
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
            lifecycle_epoch: 0,
            close_cursor: 0,
            closing: false,
        });
        Ok(key)
    }
    fn frame_index(&self, key: FrameKey) -> Result<usize, ModelError> {
        let i = key.0.checked_sub(1).ok_or(ModelError::UnknownFrame(key))? as usize;
        self.frames
            .get(i)
            .filter(|f| f.key == key && f.alive)
            .map(|_| i)
            .ok_or(ModelError::UnknownFrame(key))
    }
    fn model_index(&self, key: ModelKey) -> Result<usize, ModelError> {
        self.frame_index(key.root())
            .map_err(|_| ModelError::UnknownModel(key))?;
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
    pub fn links(&self, key: FrameKey) -> Result<crate::frame_transform::FrameLinks, ModelError> {
        self.frame_index(key)?;
        Ok(self.graph.links(key)?)
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
            .filter(|&count| count <= i32::MAX as u32)
            .ok_or(ModelError::ReferenceOverflow(key))?;
        Ok(())
    }
    /// Release the owned reference. Unsupported derived effects are rejected
    /// before decrementing the last reference; completed earlier Close releases
    /// remain observable. Dead stable keys never identify a later allocation.
    pub fn release(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let i = self.frame_index(key)?;
        if self.frames[i].references > 1 {
            self.frames[i].references -= 1;
            return Ok(());
        }
        self.graph.check_destruction(key)?;
        let owner_position = if let Some(owner) = self.frames[i].owner {
            let mi = self.model_index(owner)?;
            if self.models[mi].closing {
                return Err(ModelError::CloseIncomplete(owner));
            }
            let position = self.models[mi]
                .frames
                .iter()
                .position(|&k| k == key)
                .ok_or(ModelError::FrameNotOwned {
                    model: owner,
                    frame: key,
                })?;
            Some((mi, position))
        } else {
            None
        };
        if self.graph.state(key)?.kind == FrameKind::Model {
            // Host guard for independently constructed ownership graphs. This
            // is an explicit availability bound, not a recovered native limit.
            if self.destruction_depth >= 64 {
                return Err(ModelError::NestedDestructionUnavailable(key));
            }
            self.destruction_depth += 1;
            let result = self.close(ModelKey(key));
            self.destruction_depth -= 1;
            result?;
            // Model's derived destructor frees its frame-array allocation
            // after Close and before entering the common frame destructor.
            let mi = self.model_index(ModelKey(key))?;
            self.models[mi].frames = Vec::new();
            self.models[mi].native_capacity = 0;
        }
        self.graph.destroy_derived(key)?;
        // Derived teardown precedes base owner removal. The zero-count base
        // destructor removes membership without an additional Release.
        if let Some((mi, position)) = owner_position {
            self.models[mi].frames.swap_remove(position);
            self.frames[i].owner = None;
        }
        self.graph.destroy_base(key)?;
        self.frames[i].references = 0;
        self.frames[i].alive = false;
        self.frames[i].properties = Vec::new();
        self.frames[i].name_bytes = Vec::new();
        self.frames[i].dummy_bounds = None;
        self.frames[i].source = None;
        Ok(())
    }
    pub fn add_frame(&mut self, model: ModelKey, key: FrameKey) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        let fi = self.frame_index(key)?;
        if self.models[mi].closing {
            return Err(ModelError::CloseIncomplete(model));
        }
        let mut ancestor = Some(model);
        while let Some(current) = ancestor {
            if current.root() == key {
                return Err(ModelError::OwnershipCycle);
            }
            ancestor = self.frame(current.root())?.owner;
        }
        if let Some(previous) = self.frames[fi].owner {
            if self.models[self.model_index(previous)?].closing {
                return Err(ModelError::CloseIncomplete(previous));
            }
        }
        self.frames[fi]
            .references
            .checked_add(1)
            .filter(|&count| count <= i32::MAX as u32)
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
        if self.models[mi].closing {
            return Err(ModelError::CloseIncomplete(model));
        }
        let position = self.models[mi]
            .frames
            .iter()
            .position(|&k| k == key)
            .ok_or(ModelError::FrameNotOwned { model, frame: key })?;
        if self.frames[fi].references <= 1 {
            self.graph.check_destruction(key)?;
        }
        self.models[mi].frames.swap_remove(position);
        self.frames[fi].owner = None;
        self.release(key)
    }
    /// Forward ownership release for the represented destructor subset. Native
    /// list storage is retained. An unavailable later destructor retains its
    /// successful prefix; a host cursor prevents releasing that prefix twice.
    /// A host lock and restored failed-slot owner keep unavailable nested
    /// destruction consistent; these are not native exception semantics.
    /// External references survive with their hierarchy links and source backing.
    pub fn close(&mut self, model: ModelKey) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        self.models[mi].lifecycle_epoch = self.models[mi]
            .lifecycle_epoch
            .checked_add(1)
            .ok_or(ModelError::AllocationOverflow)?;
        self.models[mi].closing = true;
        while self.models[mi].close_cursor < self.models[mi].frames.len() {
            let key = self.models[mi].frames[self.models[mi].close_cursor];
            let fi = self.frame_index(key)?;
            if self.frames[fi].references <= 1 {
                self.graph.check_destruction(key)?;
            }
            self.frames[fi].owner = None;
            if let Err(error) = self.release(key) {
                // The unsupported current slot still belongs to this pending
                // host Close, including a nested model with a completed prefix.
                self.frames[fi].owner = Some(model);
                return Err(error);
            }
            self.models[mi].close_cursor += 1;
        }
        self.models[mi].frames.clear();
        self.models[mi].close_cursor = 0;
        self.models[mi].closing = false;
        self.graph.discard_hr_bound_vol(model.root())?;
        Ok(())
    }
    pub fn lifecycle_epoch(&self, model: ModelKey) -> Result<u64, ModelError> {
        Ok(self.models[self.model_index(model)?].lifecycle_epoch)
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
    identity: Arc<()>,
    source: Arc<Model>,
    arena: ModelArena,
    root: ModelKey,
    source_frame_keys: Vec<FrameKey>,
    skin_pose: Option<SinglemeshPose>,
    morph_poses: Vec<Option<MorphPose>>,
    complete: bool,
    content_epoch: u64,
    transferred_frames: usize,
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
                        identity: Arc::new(()),
                        source,
                        arena,
                        root: ModelKey(FrameKey(0)),
                        source_frame_keys: Vec::new(),
                        skin_pose: None,
                        morph_poses: Vec::new(),
                        complete: false,
                        content_epoch: 0,
                        transferred_frames: 0,
                    }),
                });
            }
        };
        let mut result = Self {
            identity: Arc::new(()),
            source,
            arena,
            root,
            source_frame_keys: Vec::new(),
            skin_pose: None,
            morph_poses: Vec::new(),
            complete: false,
            content_epoch: 0,
            transferred_frames: 0,
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
        if let Err(error) = result.resolve_targets() {
            let error = result.transfer_loaded_frames().err().unwrap_or(error);
            return Err(ModelLoadFailure {
                error,
                partial: Box::new(result),
            });
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
            (1, Some(2 | 3)) => {
                let mut state = I3dFrameState::mesh();
                state.kind = FrameKind::Singlemesh;
                state
            }
            (6, None) => I3dFrameState::dummy(),
            (7, None) => I3dFrameState::target(),
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
        if source.kind == 7 && source.target.is_none() {
            return Err(ModelError::MissingTargetRecord(index));
        }
        if source.kind != 7 && source.target.is_some() {
            return Err(ModelError::UnsupportedFrame {
                index,
                kind: source.kind,
                visual: source.visual,
            });
        }
        if source.kind == 10 && source.joint.is_none() {
            return Err(ModelError::MissingJointRecord(index));
        }
        if matches!(source.visual, Some(2 | 3))
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
        let morph = if source.visual == Some(3) {
            Some(MorphPose::from_frame(source).map_err(ModelError::SkinPose)?)
        } else {
            if source.morph.is_some() {
                return Err(ModelError::SkinPose(SkinPoseError::Source(
                    "morph on non-singlemorph frame",
                )));
            }
            None
        };
        let key = self.arena.insert(
            state,
            Some(index),
            source.name_bytes.clone(),
            source.properties.clone(),
            source.dummy_bounds,
        )?;
        let fi = self.arena.frame_index(key)?;
        self.arena.frames[fi].source = Some(Arc::clone(&self.source));
        self.morph_poses.push(morph);
        self.source_frame_keys.push(key);
        if source.kind == 7 {
            self.arena
                .graph
                .register_target(key, I3dTargetState::default())?;
        }
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
        Ok(())
    }
    fn resolve_targets(&mut self) -> Result<(), ModelError> {
        for (index, source) in self.source.frames.iter().enumerate() {
            if let Some(record) = &source.target {
                let state = I3dTargetState::from_parsed_fields(
                    record.flags,
                    &record.frame_indices,
                    &self.source_frame_keys,
                )
                .map_err(ModelError::Target)?;
                self.arena
                    .graph
                    .apply_target_load_image_state(self.source_frame_keys[index], state)?;
            }
        }
        Ok(())
    }
    fn transfer_loaded_frames(&mut self) -> Result<(), ModelError> {
        while self.transferred_frames < self.source_frame_keys.len() {
            let key = self.source_frame_keys[self.transferred_frames];
            if self.arena.graph.links(key)?.parent.is_none() {
                self.arena.graph.link_to(key, Some(self.root.root()))?;
            }
            self.arena.add_frame(self.root, key)?;
            self.arena.release(key)?;
            self.transferred_frames += 1;
        }
        Ok(())
    }
    pub fn is_complete(&self) -> bool {
        self.complete
            && self
                .lifecycle_epoch()
                .is_ok_and(|epoch| epoch == self.content_epoch)
            && self
                .arena
                .model(self.root)
                .is_ok_and(|model| model.frames.len() == self.source_frame_keys.len())
            && self.source_frame_keys.iter().all(|&key| {
                self.arena
                    .frame(key)
                    .is_ok_and(|frame| frame.owner == Some(self.root))
            })
    }
    /// Content epoch also changes when callers close the low-level arena.
    pub fn lifecycle_epoch(&self) -> Result<u64, ModelError> {
        self.arena.lifecycle_epoch(self.root)
    }
    /// Close this parsed host materialization. External playback must be dropped
    /// first by its owner; this adapter does not own the original model controller.
    /// On failure the previous renderable pose and binding identity are invalid.
    pub fn close(&mut self) -> Result<(), ModelError> {
        self.complete = false;
        self.identity = Arc::new(());
        self.skin_pose = None;
        // Failed intake may still own creator references not yet transferred.
        // Keep their keys until adoption and Close both succeed.
        self.transfer_loaded_frames()?;
        self.arena.close(self.root)?;
        self.morph_poses.clear();
        self.source_frame_keys.clear();
        self.transferred_frames = 0;
        Ok(())
    }
    /// Close before loading a new parsed source into the same root identity.
    /// This is not native file Open: filename sharing, I/O and fast-find naming
    /// remain separate. Failed intake retains its successfully owned prefix.
    pub fn reopen_from_parsed(&mut self, source: Arc<Model>) -> Result<(), ModelError> {
        self.close()?;
        self.source = source;
        for index in 0..self.source.frames.len() {
            if let Err(error) = self.load_frame(index) {
                return Err(self.transfer_loaded_frames().err().unwrap_or(error));
            }
        }
        if let Err(error) = self.resolve_targets() {
            return Err(self.transfer_loaded_frames().err().unwrap_or(error));
        }
        self.transfer_loaded_frames()?;
        self.arena.graph.mark_loaded_root(self.root.root())?;
        self.content_epoch = self.lifecycle_epoch()?;
        self.complete = true;
        Ok(())
    }
    pub fn source(&self) -> &Arc<Model> {
        &self.source
    }
    pub(crate) fn identity(&self) -> &Arc<()> {
        &self.identity
    }
    pub(crate) fn mark_animated(&mut self, frame: FrameKey) -> Result<(), ModelError> {
        self.arena.graph.mark_animated(frame)?;
        Ok(())
    }
    pub(crate) fn apply_animation_sample(
        &mut self,
        sample: crate::skeletal_animation::SampledFrame,
    ) -> Result<(), ModelError> {
        self.skin_pose = None;
        if let Some(position) = sample.position {
            self.arena.graph.assign_position(sample.frame, position)?;
        }
        if let Some(rotation) = sample.rotation {
            self.arena
                .graph
                .apply_evaluated_rotation(sample.frame, rotation)?;
        }
        Ok(())
    }
    pub(crate) fn update_animation_model(&mut self) -> Result<(), ModelError> {
        self.skin_pose = None;
        self.arena.graph.update(self.root.root())?;
        Ok(())
    }
    pub fn root(&self) -> FrameKey {
        self.root.root()
    }
    pub fn model_key(&self) -> ModelKey {
        self.root
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
        if !self.is_complete() {
            return Err(ModelError::IncompleteModel);
        }
        self.source_frame_keys
            .iter()
            .map(|&key| self.arena.world_matrix(key))
            .collect()
    }
    /// Sample one explicitly bound position/rotation animation at an admitted
    /// time, apply its evaluator writes, then execute the model hierarchy update.
    /// This host inspection adapter does not create original controller/resource
    /// ownership, advance IPol time, blend slots, or dispatch boundary events.
    /// Sampling is preflighted before any writes. After writes begin, unavailable
    /// graph work preserves its prefix and discards the old renderable pose.
    pub fn evaluate_skeletal_animation(
        &mut self,
        animation: &crate::skeletal_animation::SkeletalAnimation,
        current_ms: u32,
    ) -> Result<(), ModelError> {
        if !self.is_complete()
            || !Arc::ptr_eq(&self.source, animation.model())
            || animation.bindings().iter().any(|binding| {
                self.source_frame_keys.get(binding.frame_index) != Some(&binding.frame)
            })
        {
            return Err(ModelError::AnimationIdentity);
        }
        let samples = animation
            .sample(current_ms)
            .map_err(ModelError::Animation)?;
        self.skin_pose = None;
        for sample in samples {
            if let Some(position) = sample.position {
                self.arena.graph.assign_position(sample.frame, position)?;
            }
            if let Some(rotation) = sample.rotation {
                self.arena
                    .graph
                    .apply_evaluated_rotation(sample.frame, rotation)?;
            }
        }
        self.arena.graph.update(self.root.root())?;
        Ok(())
    }
    /// Build an immutable first-LOD CPU skin pose from the unchanged source and
    /// the owned Joint hierarchy, including the populated-definition bounds
    /// calculation before deformation. This does not bind an animation or
    /// implement native vertex-buffer management and rendering.
    pub fn prepare_skin_pose(&mut self) -> Result<(), ModelError> {
        if !self.is_complete() {
            return Err(ModelError::SkinPose(SkinPoseError::Source(
                "incomplete model",
            )));
        }
        self.skin_pose = None;
        if self
            .source
            .frames
            .iter()
            .any(|frame| matches!(frame.visual, Some(2 | 3)))
        {
            self.skin_pose = Some(
                SinglemeshPose::build(
                    &self.source,
                    &self.source_frame_keys,
                    &mut self.arena.graph,
                    &mut self.morph_poses,
                )
                .map_err(ModelError::SkinPose)?,
            );
        }
        Ok(())
    }
    pub fn skin_pose(&self) -> Option<&SinglemeshPose> {
        self.is_complete()
            .then_some(self.skin_pose.as_ref())
            .flatten()
    }
    /// Additional owned storage; shared source Arc<Model> payloads are charged
    /// by the caller, including any old backing retained by surviving frames.
    /// Arc handles are already included in the frame allocation size.
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
        let mut bytes = bytes
            .checked_add(
                self.morph_poses
                    .capacity()
                    .checked_mul(std::mem::size_of::<Option<MorphPose>>())
                    .ok_or(ModelError::AllocationOverflow)?,
            )
            .ok_or(ModelError::AllocationOverflow)?;
        for morph in self.morph_poses.iter().flatten() {
            bytes = bytes
                .checked_add(
                    morph
                        .storage_bytes()
                        .ok_or(ModelError::AllocationOverflow)?,
                )
                .ok_or(ModelError::AllocationOverflow)?;
        }
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
            morph: None,
            target: None,
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

    fn synthetic_joint_animation() -> Arc<lhr_assets::animation::AnimationSet> {
        let mut payload = vec![1, 0, 10, 0];
        payload.extend(12u32.to_le_bytes());
        payload.extend(17u32.to_le_bytes());
        payload.extend(b"bone\0");
        payload.extend(6u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        for values in [
            vec![1., 0., 0., 0., 1., 0.001, 0., 0.],
            vec![4., 0., 0., 8., 2., 0.],
        ] {
            payload.extend(2u16.to_le_bytes());
            payload.extend(0u16.to_le_bytes());
            payload.extend(10u16.to_le_bytes());
            payload.extend(0u16.to_le_bytes());
            for value in values {
                payload.extend(f32::to_le_bytes(value));
            }
        }
        let mut bytes = b"5DS\0".to_vec();
        bytes.extend(20u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        Arc::new(lhr_assets::animation::parse_5ds(&bytes).unwrap())
    }

    #[test]
    fn skeletal_sample_updates_owned_joint_and_invalidates_old_skin_pose() {
        let mut source = skinned_source();
        let joint = &mut Arc::make_mut(&mut source).frames[1];
        joint.name = "bone".into();
        joint.name_bytes = b"bone".to_vec();
        let mut loaded = MaterializedModel::from_parsed(source.clone()).unwrap();
        loaded.prepare_skin_pose().unwrap();
        let old_position = loaded.skin_pose().unwrap().vertices(0).unwrap()[0].position;
        let animation = crate::skeletal_animation::SkeletalAnimation::new(
            &source,
            synthetic_joint_animation(),
            loaded.source_frame_keys(),
        )
        .unwrap();
        let sampled = animation.sample(200).unwrap();
        loaded.evaluate_skeletal_animation(&animation, 200).unwrap();
        assert!(loaded.skin_pose().is_none());
        let joint = loaded.arena.state(loaded.source_frame_keys()[1]).unwrap();
        assert_eq!(joint.local.elements[12..15], sampled[0].position.unwrap());
        assert_eq!(
            joint.rotation.wxyz.map(f32::to_bits),
            sampled[0].rotation.unwrap().wxyz.map(f32::to_bits)
        );
        loaded.prepare_skin_pose().unwrap();
        assert_ne!(
            loaded.skin_pose().unwrap().vertices(0).unwrap()[0].position,
            old_position
        );
        let before = *loaded.arena.state(loaded.source_frame_keys()[1]).unwrap();
        assert!(loaded.evaluate_skeletal_animation(&animation, 401).is_err());
        assert_eq!(
            *loaded.arena.state(loaded.source_frame_keys()[1]).unwrap(),
            before
        );
        assert!(loaded.skin_pose().is_some());
        let other_source = Arc::new(source.as_ref().clone());
        let other = crate::skeletal_animation::SkeletalAnimation::new(
            &other_source,
            synthetic_joint_animation(),
            loaded.source_frame_keys(),
        )
        .unwrap();
        assert_eq!(
            loaded.evaluate_skeletal_animation(&other, 200),
            Err(ModelError::AnimationIdentity)
        );
    }

    #[test]
    fn parsed_target_references_keep_membership_and_reject_missing_payload() {
        let mut target = frame(7, None, [1., 0., 0.]);
        target.target = Some(lhr_assets::TargetRecord {
            flags: 0,
            frame_indices: vec![0, 1, 1],
        });
        let source = Arc::new(Model {
            materials: vec![],
            frames: vec![frame(6, None, [0.; 3]), target],
        });
        let loaded = MaterializedModel::from_parsed(source).unwrap();
        let keys = loaded.source_frame_keys();
        assert_eq!(
            loaded.arena.graph.target_state(keys[1]).unwrap().frames(),
            &[keys[0], keys[0]]
        );
        assert_ne!(loaded.arena.state(keys[0]).unwrap().flags & 0x4000, 0);
        let missing = Arc::new(Model {
            materials: vec![],
            frames: vec![frame(7, None, [0.; 3])],
        });
        assert_eq!(
            MaterializedModel::from_parsed(missing).unwrap_err().error,
            ModelError::MissingTargetRecord(0)
        );
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
    fn transfers_last_owner_then_destroys_supported_unreferenced_frame() {
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
        loaded.arena.close(next).unwrap();
        assert!(
            matches!(loaded.arena.frame(key), Err(ModelError::UnknownFrame(dead)) if dead == key)
        );
        assert!(
            matches!(loaded.arena.state(key), Err(ModelError::UnknownFrame(dead)) if dead == key)
        );
    }
    #[test]
    fn persistent_root_reopens_with_new_keys_and_preserves_external_source_backing() {
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(6, None, [3., 0., 0.])],
        });
        let old_source = Arc::downgrade(&source);
        let mut loaded = MaterializedModel::from_parsed(source).unwrap();
        let root = loaded.root();
        let old_identity = Arc::clone(loaded.identity());
        let old = loaded.source_frame_keys()[0];
        loaded.arena.retain(old).unwrap();
        loaded.close().unwrap();
        assert!(!loaded.is_complete());
        assert!(!Arc::ptr_eq(&old_identity, loaded.identity()));
        assert_eq!(loaded.arena.frame(old).unwrap().references, 1);
        assert_eq!(loaded.arena.frame(old).unwrap().owner, None);
        assert_eq!(loaded.arena.links(old).unwrap().parent, Some(root));
        let replacement = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(6, None, [7., 0., 0.])],
        });
        loaded.reopen_from_parsed(replacement).unwrap();
        assert!(loaded.is_complete());
        assert_eq!(loaded.root(), root);
        assert!(loaded.source_frame_keys()[0].0 > old.0);
        assert_eq!(loaded.arena.links(root).unwrap().child_count, 2);
        assert!(old_source.upgrade().is_some());
        loaded.arena.release(old).unwrap();
        assert!(old_source.upgrade().is_none());
        assert!(matches!(
            loaded.arena.retain(old),
            Err(ModelError::UnknownFrame(_))
        ));
        loaded.close().unwrap();
        loaded.close().unwrap();
        assert_eq!(loaded.arena.links(root).unwrap().child_count, 0);
    }

    #[test]
    fn failed_close_retains_prefix_and_can_resume_without_double_release() {
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(6, None, [0.; 3]), frame(1, None, [0.; 3])],
        });
        let mut loaded = MaterializedModel::from_parsed(source).unwrap();
        let keys = loaded.source_frame_keys().to_vec();
        assert_eq!(
            loaded.close(),
            Err(ModelError::Frame(FrameError::UnsupportedDestruction(
                keys[1]
            )))
        );
        assert!(!loaded.is_complete());
        assert!(matches!(
            loaded.arena.frame(keys[0]),
            Err(ModelError::UnknownFrame(_))
        ));
        assert_eq!(loaded.arena.frame(keys[1]).unwrap().references, 1);
        assert_eq!(
            loaded.arena.frame(keys[1]).unwrap().owner,
            Some(loaded.root)
        );
        assert_eq!(
            loaded.close(),
            Err(ModelError::Frame(FrameError::UnsupportedDestruction(
                keys[1]
            )))
        );
        let other = loaded.arena.create_model().unwrap();
        assert_eq!(
            loaded.arena.add_frame(other, keys[1]),
            Err(ModelError::CloseIncomplete(loaded.root))
        );
        loaded.arena.retain(keys[1]).unwrap();
        loaded.close().unwrap();
        assert_eq!(loaded.arena.frame(keys[1]).unwrap().references, 1);
        assert_eq!(loaded.arena.frame(keys[1]).unwrap().owner, None);
        assert!(loaded.source_frame_keys().is_empty());
    }

    #[test]
    fn low_level_close_invalidates_content_epoch_and_failed_target_intake_is_owned() {
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame(6, None, [0.; 3])],
        });
        let mut loaded = MaterializedModel::from_parsed(source).unwrap();
        let epoch = loaded.lifecycle_epoch().unwrap();
        loaded.arena.close(loaded.root).unwrap();
        assert!(!loaded.is_complete());
        assert!(loaded.lifecycle_epoch().unwrap() > epoch);
        let mut target = frame(7, None, [0.; 3]);
        target.target = Some(lhr_assets::TargetRecord {
            flags: 0,
            frame_indices: vec![2],
        });
        let failure = MaterializedModel::from_parsed(Arc::new(Model {
            materials: Vec::new(),
            frames: vec![target],
        }))
        .unwrap_err();
        let mut partial = *failure.partial;
        let key = partial.source_frame_keys()[0];
        assert_eq!(partial.arena.frame(key).unwrap().owner, Some(partial.root));
        // Constructor registration remains empty after failed reference intake.
        // Its successfully constructed record is owned and can be closed.
        partial.close().unwrap();
        assert!(partial.source_frame_keys().is_empty());
        assert!(matches!(
            partial.arena.frame(key),
            Err(ModelError::UnknownFrame(_))
        ));
    }

    #[test]
    fn destroyed_parent_detaches_surviving_child_without_releasing_or_invalidating_cache() {
        let mut loaded = MaterializedModel::from_parsed(Arc::new(Model {
            materials: Vec::new(),
            frames: vec![
                frame(6, None, [10., 0., 0.]),
                frame(6, Some(0), [2., 0., 0.]),
            ],
        }))
        .unwrap();
        let keys = loaded.source_frame_keys().to_vec();
        assert_eq!(
            loaded.arena.world_matrix(keys[1]).unwrap().elements[12],
            12.
        );
        loaded.arena.retain(keys[1]).unwrap();
        loaded.close().unwrap();
        assert_eq!(loaded.arena.links(keys[1]).unwrap().parent, None);
        assert_eq!(loaded.arena.frame(keys[1]).unwrap().references, 1);
        assert_eq!(
            loaded.arena.world_matrix(keys[1]).unwrap().elements[12],
            12.
        );
        loaded.arena.release(keys[1]).unwrap();
        assert!(matches!(
            loaded.arena.frame(keys[1]),
            Err(ModelError::UnknownFrame(_))
        ));
    }

    #[test]
    fn retired_later_frame_invalidates_skin_and_rejects_legacy_sampling_before_writes() {
        let mut source = skinned_source();
        let joint = &mut Arc::make_mut(&mut source).frames[1];
        joint.name_bytes = b"bone".to_vec();
        let mut loaded = MaterializedModel::from_parsed(source.clone()).unwrap();
        loaded.prepare_skin_pose().unwrap();
        let keys = loaded.source_frame_keys().to_vec();
        let animation = crate::skeletal_animation::SkeletalAnimation::new(
            &source,
            synthetic_joint_animation(),
            &keys,
        )
        .unwrap();
        let before = *loaded.arena.state(keys[1]).unwrap();
        loaded.arena.release(keys[2]).unwrap();
        assert!(!loaded.is_complete());
        assert!(loaded.skin_pose().is_none());
        assert_eq!(
            loaded.evaluate_skeletal_animation(&animation, 200),
            Err(ModelError::AnimationIdentity)
        );
        assert_eq!(*loaded.arena.state(keys[1]).unwrap(), before);
        assert_eq!(loaded.world_matrices(), Err(ModelError::IncompleteModel));
    }

    #[test]
    fn nested_unavailable_close_locks_even_an_empty_completed_prefix() {
        let mut arena = ModelArena::default();
        let outer = arena.create_model().unwrap();
        let inner = arena.create_model().unwrap();
        let other = arena.create_model().unwrap();
        let mesh = arena
            .insert(I3dFrameState::mesh(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        arena.add_frame(inner, mesh).unwrap();
        arena.release(mesh).unwrap();
        arena.add_frame(outer, inner.root()).unwrap();
        arena.release(inner.root()).unwrap();
        assert_eq!(
            arena.close(outer),
            Err(ModelError::Frame(FrameError::UnsupportedDestruction(mesh)))
        );
        assert_eq!(arena.frame(inner.root()).unwrap().owner, Some(outer));
        assert_eq!(
            arena.add_frame(other, inner.root()),
            Err(ModelError::CloseIncomplete(outer))
        );
        assert_eq!(
            arena.delete_frame(outer, inner.root()),
            Err(ModelError::CloseIncomplete(outer))
        );
        arena.retain(mesh).unwrap();
        arena.close(outer).unwrap();
        assert!(matches!(
            arena.frame(inner.root()),
            Err(ModelError::UnknownFrame(_))
        ));
        assert_eq!(arena.frame(mesh).unwrap().references, 1);
        assert_eq!(arena.frame(mesh).unwrap().owner, None);
    }

    #[test]
    fn deep_model_ownership_is_guarded_and_failed_close_can_resume() {
        let mut arena = ModelArena::default();
        let root = arena.create_model().unwrap();
        let mut last = root;
        for _ in 0..80 {
            let child = arena.create_model().unwrap();
            arena.add_frame(last, child.root()).unwrap();
            arena.release(child.root()).unwrap();
            last = child;
        }
        let blocked = match arena.close(root) {
            Err(ModelError::NestedDestructionUnavailable(key)) => key,
            other => panic!("unexpected nested close result: {other:?}"),
        };
        arena.retain(blocked).unwrap();
        arena.close(root).unwrap();
        assert!(arena.frame(blocked).is_ok());
        arena.release(blocked).unwrap();
        assert!(matches!(
            arena.frame(last.root()),
            Err(ModelError::UnknownFrame(_))
        ));
    }

    #[test]
    fn reference_growth_stays_within_the_native_signed_positive_domain() {
        let mut arena = ModelArena::default();
        let root = arena.create_model().unwrap();
        let index = arena.frame_index(root.root()).unwrap();
        arena.frames[index].references = i32::MAX as u32;
        assert_eq!(
            arena.retain(root.root()),
            Err(ModelError::ReferenceOverflow(root.root()))
        );
        assert_eq!(
            arena.frame(root.root()).unwrap().references,
            i32::MAX as u32
        );
        arena.release(root.root()).unwrap();
        assert_eq!(
            arena.frame(root.root()).unwrap().references,
            i32::MAX as u32 - 1
        );
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

    #[test]
    fn lookup_borrows_owned_link_order_and_rejects_deleted_or_foreign_frames() {
        use crate::frame_lookup::{
            self, FrameLookupBackend, FrameLookupError, ModelFrameLookup, SCRIPT_FIND_FLAGS,
        };
        let source = Arc::new(Model {
            materials: vec![],
            frames: vec![frame(6, None, [0.; 3]), frame(6, None, [0.; 3])],
        });
        let mut loaded = MaterializedModel::from_parsed(source.clone()).unwrap();
        let keys = loaded.source_frame_keys().to_vec();
        let root = loaded.root();
        let (root_ref, child_ref) = {
            let lookup = ModelFrameLookup::borrow(loaded.arena(), true);
            let root_ref = lookup.reference(root).unwrap();
            let first = lookup.reference(keys[0]).unwrap();
            let last = lookup.reference(keys[1]).unwrap();
            // LinkTo prepends: actual child order differs from source file order.
            assert_eq!(lookup.frame(root_ref).unwrap().child, Some(last));
            assert_eq!(
                frame_lookup::get_child_frame(&lookup, root_ref, b"FIXTURE", SCRIPT_FIND_FLAGS, 3),
                Ok(Some(last))
            );
            assert_eq!(
                frame_lookup::get_child_frame(&lookup, first, b"fixture", SCRIPT_FIND_FLAGS, 1),
                Ok(Some(first))
            );
            assert_eq!(
                frame_lookup::get_child_frame(&lookup, root_ref, b"", SCRIPT_FIND_FLAGS, 3),
                Err(FrameLookupError::UnsupportedName)
            );
            assert_eq!(loaded.arena().frame(keys[1]).unwrap().references, 1);
            (root_ref, last)
        };
        let other = MaterializedModel::from_parsed(source).unwrap();
        let foreign = ModelFrameLookup::borrow(other.arena(), true);
        assert_eq!(
            foreign.frame(child_ref).unwrap_err(),
            FrameLookupError::StaleFrame(child_ref)
        );
        loaded.close().unwrap();
        let lookup = ModelFrameLookup::borrow(loaded.arena(), true);
        assert_eq!(
            lookup.frame(child_ref).unwrap_err(),
            FrameLookupError::StaleFrame(child_ref)
        );
        assert_eq!(
            frame_lookup::get_child_frame(&lookup, root_ref, b"fixture", SCRIPT_FIND_FLAGS, 3),
            Ok(None)
        );
        assert_eq!(
            lookup.reference(keys[1]),
            Err(FrameLookupError::StaleFrame(child_ref))
        );
        assert_eq!(loaded.arena().frame(root).unwrap().references, 1);
    }
}
