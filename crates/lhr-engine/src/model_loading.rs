//! Owned model/frame materialization for the validated mesh, dummy and joint subset.
//! Singlemesh admission retains binding fields and requires a separate checked
//! pose snapshot and populated-definition bounds before rendering.
//! LS3DF RVAs 0x33680/0x33720/0x33500 define model ownership; 0x603e0
//! transfers loaded frames in file order, including a successfully loaded prefix
//! before an error. Persistent parsed Close/reopen keeps root identity and uses
//! nonreused tombstones for destroyed model/dummy/joint/Target objects. Visual
//! resources can now own concrete CPU geometry/images through the original
//! plain object/resource teardown. Shared-arena static file intake composes
//! close-first loading, fresh/shared filename descriptors and model prefixes.
//! Loader callbacks/animation failure paths and GPU resource creation remain
//! separate; the actual Installation adapter is not a full native LoadImage.
#[path = "scene_world_intake.rs"]
mod scene_world_intake;
pub use scene_world_intake::{
    CpuSceneWorldError, CpuSceneWorldFailure, CpuSceneWorldPhase, CpuSceneWorldProgress,
};
#[path = "model_cache_duplicate.rs"]
mod cache_duplicate;
#[path = "model_crash_duplicate.rs"]
mod crash_duplicate;
#[path = "model_glass.rs"]
mod glass;
use crate::frame_transform::{FrameError, FrameKey, FrameKind, I3dFrameGraph, I3dFrameState};
use crate::joint_state::{I3dJointState, SinglemeshInvalidationState};
use crate::native_transform::{SMatrix, SQuat};
use crate::singlemesh_pose::{MorphPose, SinglemeshPose, SkinPoseError};
use crate::target_state::{I3dTargetState, TargetError};
use lhr_assets::Model;
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelKey(pub(crate) FrameKey);
impl ModelKey {
    pub fn root(self) -> FrameKey {
        self.0
    }
}
#[derive(Clone, Debug)]
pub struct LoadedFrame {
    pub key: FrameKey,
    pub references: u32,
    pub owner: Option<crate::scene_ownership::FrameOwner>,
    pub source_index: Option<usize>,
    pub name_bytes: Vec<u8>,
    /// Local original-name (+0x104) projection, distinct from the full current
    /// name (+0x100). Parsed materialization has no model-name prefix operation;
    /// a fresh unnamed model root retains the constructor's null original name.
    pub original_name_bytes: Option<Vec<u8>>,
    pub properties: Vec<u8>,
    pub dummy_bounds: Option<[[f32; 3]; 2]>,
    pub(crate) hr_bounds: crate::mesh_bounds::MeshBoundVolume,
    pub(crate) hr_world_bounds: crate::mesh_bounds::MeshBoundVolume,
    pub(crate) visual_bounds: Option<crate::mesh_bounds::MeshBoundVolume>,
    pub(crate) object_resources: Option<crate::i3d_object::ObjectResourceKey>,
    pub(crate) projector: Option<crate::frame_projector::I3dProjectorState>,
    pub(crate) hr_bounds_error: Option<ModelError>,
    pub(crate) hr_world_bounds_error: Option<ModelError>,
    /// Immutable host backing, independent of model membership. This is not a
    /// native mesh/LOD resource allocation or filename reference count.
    pub source: Option<Arc<Model>>,
    /// Common frame+8 userdata pointer projection. A present actor record is
    /// distinct from a null pointer, including when its first word is null.
    pub user_data: crate::actor_frames::FrameUserData,
    alive: bool,
    pub(crate) name_pointer: crate::frame_fast_lookup::OriginalFrameName,
    pub(crate) name_generation: u64,
    pub(crate) name_is_null: bool,
    pub(crate) ff_left: Option<FrameKey>,
    pub(crate) ff_right: Option<FrameKey>,
}
#[derive(Debug)]
pub struct LoadedModel {
    pub key: ModelKey,
    pub frames: Vec<FrameKey>,
    pub(crate) native_capacity: usize,
    pub(crate) growth: usize,
    lifecycle_epoch: u64,
    close_cursor: usize,
    pub(crate) closing: bool,
    filename: Option<ModelFileNameKey>,
    /// Actual fresh I3D_model+150 is null. Host inspection playback is separate.
    /// An active native set requires its producer/resource owner before admission.
    original_ipol_set: Option<crate::scene_tick::InterpolatorId>,
}
impl LoadedModel {
    pub fn capacity(&self) -> usize {
        self.native_capacity
    }
}
/// Stable identity of the allocation at the native filename pointer minus four.
/// Equal byte strings are distinct allocations; this is not a filename cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelFileNameKey {
    arena: crate::script_scheduler::FrameReferenceId,
    slot: usize,
}
impl ModelFileNameKey {
    pub(crate) fn projection_slot(self) -> usize {
        self.slot
    }
}
#[derive(Debug)]
pub struct ModelFileName {
    bytes: Vec<u8>,
    references: i32,
}
impl ModelFileName {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn references(&self) -> i32 {
        self.references
    }
}
/// Loaded source-index mapping in the shared arena. On failure, the successfully
/// constructed prefix remains inspectable, including any creator references
/// whose transfer was blocked. This host mapping is not a native resource.
#[derive(Debug)]
pub struct SharedModelContent {
    model: ModelKey,
    frames: Vec<FrameKey>,
    transferred_frames: usize,
    arena: crate::script_scheduler::FrameReferenceId,
    epoch: u64,
}
impl SharedModelContent {
    pub fn model(&self) -> ModelKey {
        self.model
    }
    pub fn frames(&self) -> &[FrameKey] {
        &self.frames
    }
    pub fn transferred_frames(&self) -> usize {
        self.transferred_frames
    }
}
#[derive(Debug)]
pub struct SharedModelLoadFailure {
    pub error: ModelError,
    pub partial: SharedModelContent,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelError {
    Frame(FrameError),
    Camera(crate::frame_camera::CameraError),
    Occluder(crate::frame_occluder::OccluderError),
    Portal(crate::frame_portal::PortalError),
    Sector(crate::frame_sector::SectorError),
    UnknownModel(ModelKey),
    UnsupportedActiveModelController(ModelKey),
    UnknownFrame(FrameKey),
    Scene(crate::scene_ownership::SceneOwnerError),
    FastFrame(crate::frame_fast_lookup::FastFrameError),
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
    ModelNotEmpty(ModelKey),
    ForeignArena,
    UnavailableOriginalName(FrameKey),
    VisualBoundsUnavailable(FrameKey),
    LoadedVisual(crate::loaded_visual_resources::LoadedVisualError),
    BoundsDepthUnavailable(FrameKey),
    StaleParsedContent(ModelKey),
    Asset(String),
    NativeModelOpenResult(u32),
    LightConstructorUnavailable {
        frame: FrameKey,
        cause: Box<ModelError>,
    },
    OriginalModelOpenUnavailable(&'static str),
    UnknownFilename(ModelFileNameKey),
    FilenameAlreadyOwned(ModelKey),
    FilenameReferenceOverflow(ModelFileNameKey),
    CloseIncomplete(ModelKey),
    OwnershipCycle,
    NestedDestructionUnavailable(FrameKey),
    UnknownActorDataRecord(crate::actor_frames::ActorDataRecordId),
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

/// Admitted original LoadHierarchy callback configuration. The cache thunk
/// remains present even when the user callback is null; event16 would require
/// that callback and is not admitted by this static-resource path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticModelLoadProfile {
    NoCallback,
    CacheThunkNoOuter,
}
impl StaticModelLoadProfile {
    pub fn flags(self) -> u32 {
        match self {
            Self::NoCallback => 0,
            Self::CacheThunkNoOuter => 0x10,
        }
    }
    pub fn has_callback_thunk(self) -> bool {
        self == Self::CacheThunkNoOuter
    }
}
/// Stable keys are never reused. The graph owns transform caches; this arena
/// owns reference counts, model/scene membership, names and fast-tree links.
#[derive(Debug, Default)]
pub struct ModelArena {
    pub(crate) graph: I3dFrameGraph,
    pub(crate) frames: Vec<LoadedFrame>,
    pub(crate) models: Vec<LoadedModel>,
    pub(crate) scenes: Vec<crate::scene_ownership::SceneFrameOwner>,
    pub(crate) sectors: Vec<crate::scene_ownership::SectorFrame>,
    pub(crate) portal_frames: Vec<crate::frame_portal::PortalFrame>,
    pub(crate) portal_version: u32,
    pub(crate) portal_allocation_calls: usize,
    pub(crate) portal_fail_on_call: Option<usize>,
    pub(crate) sector_storage: crate::frame_sector::SectorStorage,
    pub(crate) cameras: Vec<crate::frame_camera::CameraFrame>,
    pub(crate) occluders: Vec<crate::frame_occluder::OccluderFrame>,
    pub(crate) occluder_storage: crate::frame_occluder::OccluderStorage,
    pub(crate) sector_version: u32,
    destruction_depth: usize,
    pub(crate) light_random: crate::frame_light::LightCrtRandom,
    pub(crate) last_destroyed_frame: Option<FrameKey>,
    next_name_generation: u64,
    lookup_owner: crate::frame_lookup::FrameLookupOwner,
    pub(crate) fast_tree: crate::arena_frame_names::ArenaFastTree,
    actor_data_records: Vec<crate::actor_frames::ActorDataRecord>,
    filenames: Vec<Option<ModelFileName>>,
    static_load_profile: Option<StaticModelLoadProfile>,
    loaded_visual_resources: Option<crate::loaded_visual_resources::LoadedVisualResources>,
}
impl ModelArena {
    pub(crate) fn mark_loaded_model_root(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.graph.mark_loaded_root_fields(key)?;
        self.update_frame(key)
    }
    /// Actual live arena slots, excluding retained host tombstone metadata.
    pub fn live_frame_count(&self) -> usize {
        self.frames.iter().filter(|frame| frame.alive).count()
    }
    /// A real sound node in the shared hierarchy, with the base constructor's
    /// reference and transform ownership. The audio owner supplies its tail.
    pub fn create_sound_frame(&mut self) -> Result<FrameKey, ModelError> {
        self.insert(I3dFrameState::sound(), None, Vec::new(), Vec::new(), None)
    }
    /// Host frame publication boundary: unlock the actual retained manager
    /// buffers before CPU inspection/draw or explicit resource destruction.
    /// This is not a reconstruction of the original driver's whole frame loop.
    pub fn publish_runtime_buffers(&mut self) -> Result<(), ModelError> {
        if let Some(owner) = self.loaded_visual_resources.as_mut() {
            owner
                .game_unlock_frame_buffers()
                .map_err(ModelError::LoadedVisual)?;
        }
        Ok(())
    }
    pub(crate) fn lookup_reference(
        &self,
        key: FrameKey,
    ) -> crate::script_scheduler::FrameReferenceId {
        self.lookup_owner.reference(key.0)
    }
    pub(crate) fn fresh_name_generation(&mut self) -> Result<u64, ModelError> {
        self.next_name_generation = self
            .next_name_generation
            .checked_add(1)
            .ok_or(ModelError::AllocationOverflow)?;
        Ok(self.next_name_generation)
    }
    pub(crate) fn insert(
        &mut self,
        state: I3dFrameState,
        source_index: Option<usize>,
        name_bytes: Vec<u8>,
        properties: Vec<u8>,
        dummy_bounds: Option<[[f32; 3]; 2]>,
    ) -> Result<FrameKey, ModelError> {
        self.frames
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        let name_generation = self.fresh_name_generation()?;
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
        // Native LoadImage skips the name allocation/stores for byte length0.
        let original_name_bytes =
            (source_index.is_some() && !name_bytes.is_empty()).then(|| name_bytes.clone());
        let name_is_null = original_name_bytes.is_none();
        self.frames.push(LoadedFrame {
            key,
            references: 1,
            owner: None,
            source_index,
            name_bytes,
            original_name_bytes,
            properties,
            dummy_bounds,
            hr_bounds: crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR,
            hr_world_bounds: crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR,
            visual_bounds: None,
            object_resources: None,
            projector: None,
            hr_bounds_error: None,
            hr_world_bounds_error: None,
            source: None,
            user_data: crate::actor_frames::FrameUserData::Null,
            alive: true,
            name_is_null,
            name_generation,
            name_pointer: if name_is_null {
                crate::frame_fast_lookup::OriginalFrameName::Null
            } else {
                crate::frame_fast_lookup::OriginalFrameName::Offset(0)
            },
            ff_left: None,
            ff_right: None,
        });
        Ok(key)
    }
    fn load_parsed_frame(
        &mut self,
        source_model: &Arc<Model>,
        source_frame_keys: &mut Vec<FrameKey>,
        morph_poses: &mut Vec<Option<MorphPose>>,
        index: usize,
    ) -> Result<(), ModelError> {
        let source = &source_model.frames[index];
        let mut state = match (source.kind, source.visual) {
            (1, Some(0 | 1)) => I3dFrameState::mesh(),
            (1, Some(2 | 3)) => {
                let mut state = I3dFrameState::mesh();
                state.kind = FrameKind::Singlemesh;
                state
            }
            (5, None) => {
                let mut state = I3dFrameState::model();
                state.kind = FrameKind::Sector;
                state.flags |= 0x08080000;
                state
            }
            (6, None) => I3dFrameState::dummy(),
            (7, None) => I3dFrameState::target(),
            (10, None) => I3dFrameState::joint(),
            (12, None) => {
                let mut state = I3dFrameState::model();
                state.kind = FrameKind::Occluder;
                state.flags |= 0x08100000;
                state
            }
            _ => {
                return Err(ModelError::UnsupportedFrame {
                    index,
                    kind: source.kind,
                    visual: source.visual,
                });
            }
        };
        if source.kind == 5 && source.sector.is_none() {
            return Err(crate::frame_sector::SectorError::MissingRecord.into());
        }
        if source.kind == 12 && source.occluder.is_none() {
            return Err(crate::frame_occluder::OccluderError::GeometryUnavailable.into());
        }
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
                *source_frame_keys
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
        source_frame_keys
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        morph_poses
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        if source.kind == 12 {
            self.occluders
                .try_reserve(1)
                .map_err(|_| ModelError::AllocationOverflow)?;
        }
        if source.kind == 5 {
            self.sectors
                .try_reserve(1)
                .map_err(|_| ModelError::AllocationOverflow)?;
        }
        let key = self.insert(
            state,
            Some(index),
            source.name_bytes.clone(),
            source.properties.clone(),
            source.dummy_bounds,
        )?;
        if source.kind == 12 {
            self.finish_occluder_constructor(key);
        }
        if source.kind == 5 {
            self.finish_sector_constructor(crate::scene_ownership::SectorKey(key));
        }
        let fi = self.frame_index(key)?;
        self.frames[fi].source = Some(Arc::clone(source_model));
        morph_poses.push(morph);
        source_frame_keys.push(key);
        if source.kind == 7 {
            self.graph.register_target(key, I3dTargetState::default())?;
        }
        // LoadImage links earlier parents before storing the common local SRT.
        if parent.is_some() {
            self.link_frame(key, parent, 0)?;
        }
        self.graph.apply_common_load_image_fields(
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
            self.graph.apply_joint_load_image_fields(
                key,
                SMatrix {
                    elements: joint.bone_matrix,
                },
                joint.index,
            )?;
        }
        if source.kind == 5 {
            if let Some(error) = self.load_sector_record(
                crate::scene_ownership::SectorKey(key),
                source.sector.as_ref().unwrap(),
            )? {
                return Err(ModelError::NativeModelOpenResult(error));
            }
        }
        if source.kind == 12 {
            let record = source.occluder.as_ref().unwrap();
            let key = self.occluder_key(key)?;
            let mut image_error = 0;
            self.occluder_load_geometry(key, &record.vertices, &record.faces, &mut image_error)?;
            if image_error != 0 {
                return Err(crate::frame_occluder::OccluderError::ImageError(image_error).into());
            }
        }
        Ok(())
    }
    pub fn create_model(&mut self) -> Result<ModelKey, ModelError> {
        self.models
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
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
            filename: None,
            original_ipol_set: None,
        });
        Ok(key)
    }
    /// Descriptive projection of the owned-list naming helper at PC RVA0x33ac0.
    /// The model's current full name prefixes each owned frame's original local
    /// name. Hierarchy child order and model membership are distinct here.
    pub fn propagate_model_name(&mut self, model: ModelKey) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        if self.models[mi].closing {
            return Err(ModelError::CloseIncomplete(model));
        }
        let prefix = self.frame(model.root())?.name_bytes.clone();
        let prefix = &prefix[..prefix.iter().position(|&b| b == 0).unwrap_or(prefix.len())];
        for index in 0..self.models[mi].frames.len() {
            let key = self.models[mi].frames[index];
            if prefix.is_empty() {
                self.reset_frame_model_name(key)?;
            } else {
                self.set_frame_model_name(key, prefix)?;
            }
        }
        Ok(())
    }
    /// SetModelName1b800: allocate prefix+'.'+original before removing the old
    /// fast-tree registration/allocation, then register the new full name.
    pub fn set_frame_model_name(&mut self, key: FrameKey, prefix: &[u8]) -> Result<(), ModelError> {
        use crate::frame_fast_lookup::OriginalFrameName;
        let fi = self.frame_index(key)?;
        if matches!(
            self.frames[fi].name_pointer,
            OriginalFrameName::Released { .. }
        ) {
            return Err(ModelError::UnavailableOriginalName(key));
        }
        let prefix = &prefix[..prefix.iter().position(|&b| b == 0).unwrap_or(prefix.len())];
        let original = self.frames[fi]
            .original_name_bytes
            .as_deref()
            .unwrap_or(b"");
        let original = &original[..original
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(original.len())];
        let size = prefix
            .len()
            .checked_add(original.len())
            .and_then(|n| n.checked_add(2))
            .ok_or(ModelError::AllocationOverflow)?;
        let mut full = Vec::new();
        full.try_reserve_exact(size)
            .map_err(|_| ModelError::AllocationOverflow)?;
        full.extend_from_slice(prefix);
        full.push(b'.');
        full.extend_from_slice(original);
        let local = original.to_vec();
        let generation = self.fresh_name_generation()?;
        self.remove_frame_name_registration(key)?;
        let frame = &mut self.frames[fi];
        frame.name_bytes = full;
        frame.name_generation = generation;
        frame.original_name_bytes = Some(local);
        frame.name_is_null = false;
        frame.name_pointer = OriginalFrameName::Offset(prefix.len() + 1);
        if self.fast_tree.enabled {
            self.register_frame_name(key)?;
        }
        Ok(())
    }
    /// ResetModelName1b8f0 keeps the original local name and returns its pointer
    /// to the allocation start. Registration removal precedes allocation.
    pub fn reset_frame_model_name(&mut self, key: FrameKey) -> Result<(), ModelError> {
        use crate::frame_fast_lookup::OriginalFrameName;
        let fi = self.frame_index(key)?;
        let frame = &self.frames[fi];
        if matches!(frame.name_pointer, OriginalFrameName::Released { .. }) {
            return Err(ModelError::UnavailableOriginalName(key));
        }
        if frame.name_pointer == OriginalFrameName::Offset(0)
            || (frame.name_is_null && frame.name_pointer == OriginalFrameName::Null)
        {
            return Ok(());
        }
        self.remove_frame_name_registration(key)?;
        let local = self.frames[fi].original_name_bytes.as_deref();
        let Some(local) = local else {
            self.frames[fi].name_generation = self.fresh_name_generation()?;
            self.frames[fi].name_bytes.clear();
            self.frames[fi].name_is_null = true;
            return Ok(());
        };
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(
                local
                    .len()
                    .checked_add(1)
                    .ok_or(ModelError::AllocationOverflow)?,
            )
            .map_err(|_| ModelError::AllocationOverflow)?;
        owned.extend_from_slice(local);
        self.frames[fi].name_generation = self.fresh_name_generation()?;
        self.frames[fi].name_bytes = owned;
        self.frames[fi].name_is_null = false;
        self.frames[fi].name_pointer = OriginalFrameName::Offset(0);
        if self.fast_tree.enabled {
            self.register_frame_name(key)?;
        }
        Ok(())
    }
    pub fn loaded_visual_resources(
        &self,
    ) -> Option<&crate::loaded_visual_resources::LoadedVisualResources> {
        self.loaded_visual_resources.as_ref()
    }
    pub(crate) fn visual_resource_owner_mut(
        &mut self,
    ) -> Result<&mut crate::loaded_visual_resources::LoadedVisualResources, ModelError> {
        if self.loaded_visual_resources.is_none() {
            self.loaded_visual_resources = Some(
                crate::loaded_visual_resources::LoadedVisualResources::new()
                    .map_err(ModelError::LoadedVisual)?,
            );
        }
        Ok(self.loaded_visual_resources.as_mut().unwrap())
    }
    /// The live plain-object resource identity attached to this frame.
    pub fn frame_object_resources(
        &self,
        key: FrameKey,
    ) -> Result<Option<crate::i3d_object::ObjectResourceKey>, ModelError> {
        Ok(self.frame(key)?.object_resources)
    }
    /// Actual OBJ_ class word written by plain-object constructor367b0 at+1d4.
    /// A live canonical plain-object resource identity proves this class; the
    /// 4DS visual ordinal is not the native FourCC and is never substituted.
    pub(crate) fn plain_object_visual_type(&self, key: FrameKey) -> Result<u32, ModelError> {
        if self.state(key)?.kind != FrameKind::Mesh {
            return Err(ModelError::OriginalModelOpenUnavailable(
                "plain object class witness unavailable",
            ));
        }
        let object = self
            .frame(key)?
            .object_resources
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        self.loaded_visual_resources
            .as_ref()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?
            .objects()
            .object(object)
            .map_err(|_| ModelError::VisualBoundsUnavailable(key))?;
        Ok(0x5f4a424f)
    }
    /// Original object SetMesh (PC RVA 0x368e0), including same-pointer bound
    /// recopy and the null-mesh constructor bounds. Resource replacement owns
    /// its LOD/mesh effects; this operation does not invalidate frame caches.
    pub fn set_frame_mesh(
        &mut self,
        key: FrameKey,
        mesh: Option<crate::visual_resources::MeshObjectKey>,
        preserve_lods: bool,
    ) -> Result<(), ModelError> {
        let index = self.frame_index(key)?;
        let object = self.frames[index]
            .object_resources
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        let copy_bounds = self.state(key)?.visual_flags & 0x40 == 0;
        let owner = self
            .loaded_visual_resources
            .as_mut()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        let old = owner
            .object_mesh(object)
            .map_err(ModelError::LoadedVisual)?;
        owner
            .set_object_mesh(object, mesh, preserve_lods)
            .map_err(ModelError::LoadedVisual)?;
        if old == mesh && mesh.is_none() {
            return Ok(());
        }
        if let Some(mesh) = mesh {
            if copy_bounds {
                // A host-unavailable source bound must not leave the previous
                // mesh's box usable after the reference replacement succeeded.
                self.frames[index].visual_bounds = None;
                self.frames[index].visual_bounds =
                    Some(owner.mesh_bounds(mesh).map_err(ModelError::LoadedVisual)?);
            }
        } else {
            self.frames[index].visual_bounds =
                Some(crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR);
        }
        Ok(())
    }
    /// Load an independently owned CPU texture reference into the same storage
    /// used by this arena's materials. The caller must release that reference.
    pub fn load_cpu_texture(
        &mut self,
        installation: &lhr_assets::Installation,
        name: &str,
    ) -> Result<crate::visual_resources::TextureKey, ModelError> {
        self.visual_resource_owner_mut()?
            .load_cpu_texture(installation, name)
            .map_err(ModelError::LoadedVisual)
    }
    pub fn retain_cpu_texture(
        &mut self,
        texture: crate::visual_resources::TextureKey,
    ) -> Result<(), ModelError> {
        self.visual_resource_owner_mut()?
            .retain_cpu_texture(texture)
            .map_err(ModelError::LoadedVisual)
    }
    pub fn release_cpu_texture(
        &mut self,
        texture: crate::visual_resources::TextureKey,
    ) -> Result<(), ModelError> {
        self.visual_resource_owner_mut()?
            .release_cpu_texture(texture)
            .map_err(ModelError::LoadedVisual)
    }
    /// Host materialization of actual CPU vertex/index/texture ownership into
    /// the original plain ObjectResources lifecycle. Completed bindings remain
    /// owned on error. Original Create/worker/upload and GPU rendering are not
    /// implemented by this adapter; unsupported derived subtypes stay closed.
    pub fn load_plain_visual_resources(
        &mut self,
        model: ModelKey,
        installation: &lhr_assets::Installation,
    ) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        if self.models[mi].closing {
            return Err(ModelError::CloseIncomplete(model));
        }
        if self.loaded_visual_resources.is_none() {
            self.loaded_visual_resources = Some(
                crate::loaded_visual_resources::LoadedVisualResources::new()
                    .map_err(ModelError::LoadedVisual)?,
            );
        }
        for index in 0..self.models[mi].frames.len() {
            let key = self.models[mi].frames[index];
            if !matches!(
                self.state(key)?.kind,
                FrameKind::Mesh | FrameKind::Singlemesh
            ) {
                continue;
            }
            let fi = self.frame_index(key)?;
            if self.frames[fi].object_resources.is_some() {
                continue;
            }
            let source = self.frames[fi]
                .source
                .as_ref()
                .cloned()
                .ok_or(ModelError::VisualBoundsUnavailable(key))?;
            let source_index = self.frames[fi]
                .source_index
                .ok_or(ModelError::VisualBoundsUnavailable(key))?;
            let instance_object = if let Some(original_index) = source.frames[source_index].instance
            {
                if original_index >= source_index {
                    return Err(ModelError::InvalidParent {
                        index: source_index,
                        parent: original_index,
                    });
                }
                Some(
                    self.frames
                        .iter()
                        .find(|frame| {
                            frame.alive
                                && frame.source_index == Some(original_index)
                                && frame.owner == Some(model.into())
                                && frame
                                    .source
                                    .as_ref()
                                    .is_some_and(|other| Arc::ptr_eq(other, &source))
                        })
                        .and_then(|frame| frame.object_resources)
                        .ok_or(ModelError::VisualBoundsUnavailable(key))?,
                )
            } else {
                None
            };
            let owner = self.loaded_visual_resources.as_mut().unwrap();
            let object = if let Some(original) = instance_object {
                owner.load_plain_instance(original)
            } else {
                owner.load_plain_object(&source, source_index, installation)
            }
            .map_err(ModelError::LoadedVisual)?;
            // Store the owned object before a later bound projection can fail.
            self.frames[fi].object_resources = Some(object);
            let bounds = owner
                .object_bounds(object)
                .map_err(ModelError::LoadedVisual)?;
            // SetMesh368e0 copies the mesh bound volume unless flag0x40 locks it.
            // Fresh visual bounds carry the constructor sentinels until copied.
            self.frames[fi].visual_bounds = Some(if self.state(key)?.visual_flags & 0x40 == 0 {
                bounds
            } else {
                crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR
            });
            self.graph.discard_hr_bound_vol(key)?;
        }
        Ok(())
    }
    /// Explicit manager-cache reclamation used after object/scene releases.
    /// Object deletion itself frees its blocks while shared arrays stay cached.
    pub fn reclaim_unused_visual_vertices(&mut self) -> Result<(), ModelError> {
        if let Some(owner) = &mut self.loaded_visual_resources {
            owner
                .reclaim_unused_vertices()
                .map_err(ModelError::LoadedVisual)?;
        }
        Ok(())
    }
    pub fn reclaim_unused_visual_indices(&mut self) -> Result<(), ModelError> {
        if let Some(owner) = &mut self.loaded_visual_resources {
            owner
                .reclaim_unused_indices()
                .map_err(ModelError::LoadedVisual)?;
        }
        Ok(())
    }
    pub fn reclaim_unused_visual_buffers(&mut self) -> Result<(), ModelError> {
        self.reclaim_unused_visual_vertices()?;
        self.reclaim_unused_visual_indices()
    }
    /// Execute only the two loaded-resource maintenance stages of Scene Close.
    /// This is not a complete SceneCloseBackend: all other effects remain explicit
    /// boundaries, as does a scene without an installed loaded-resource owner.
    pub fn apply_scene_loaded_resource_effect(
        &mut self,
        scene: crate::scene_ownership::SceneKey,
        effect: crate::scene_ownership::SceneCloseEffect,
    ) -> Result<(), ModelError> {
        use crate::scene_ownership::{SceneCloseEffect, SceneOwnerError};
        self.scene(scene)?;
        if !matches!(
            effect,
            SceneCloseEffect::FreeUnusedVertexBuffers | SceneCloseEffect::FreeUnusedIndexBuffers
        ) {
            return Err(SceneOwnerError::BackendUnavailable(effect).into());
        }
        let owner = self
            .loaded_visual_resources
            .as_mut()
            .ok_or(SceneOwnerError::BackendUnavailable(effect))?;
        match effect {
            SceneCloseEffect::FreeUnusedVertexBuffers => owner.reclaim_unused_vertices(),
            SceneCloseEffect::FreeUnusedIndexBuffers => owner.reclaim_unused_indices(),
            _ => unreachable!(),
        }
        .map_err(ModelError::LoadedVisual)
    }
    fn check_frame_destruction(&self, key: FrameKey) -> Result<(), ModelError> {
        if self.state(key)?.kind == FrameKind::Camera {
            let camera = self.camera(self.camera_key(key)?)?;
            if !camera.constructor_complete() {
                return Err(
                    crate::frame_camera::CameraError::IncompleteConstruction(camera.key).into(),
                );
            }
        }
        if self.state(key)?.kind == FrameKind::Sector {
            self.check_sector_destruction_domain(crate::scene_ownership::SectorKey(key))?;
            self.graph.check_empty_scene_base_destruction(key)?;
            return Ok(());
        }
        if let Some(object) = self.frame(key)?.object_resources {
            let owner = self
                .loaded_visual_resources
                .as_ref()
                .ok_or(ModelError::VisualBoundsUnavailable(key))?;
            owner
                .object_mesh(object)
                .map_err(ModelError::LoadedVisual)?;
            self.graph.check_destruction_with_owned_visual(key)?;
        } else if let Some(projector) = &self.frame(key)?.projector {
            if let Some(texture) = projector.texture() {
                self.loaded_visual_resources
                    .as_ref()
                    .ok_or(ModelError::VisualBoundsUnavailable(key))?
                    .cpu_texture(texture)
                    .map_err(ModelError::LoadedVisual)?;
            }
            self.graph.check_destruction_with_owned_visual(key)?;
        } else {
            self.graph.check_destruction_with_owned_name(key)?;
        }
        Ok(())
    }
    pub fn model_filename(&self, model: ModelKey) -> Result<Option<ModelFileNameKey>, ModelError> {
        Ok(self.models[self.model_index(model)?].filename)
    }
    pub fn filename(&self, key: ModelFileNameKey) -> Result<&ModelFileName, ModelError> {
        if key.arena != self.lookup_owner.reference(0) {
            return Err(ModelError::ForeignArena);
        }
        self.filenames
            .get(key.slot)
            .and_then(Option::as_ref)
            .ok_or(ModelError::UnknownFilename(key))
    }
    /// Resolves a projected filename allocation identity through this arena's
    /// current owner. Slots are stable and never reused after their last Close.
    pub(crate) fn projected_filename_key(
        &self,
        slot: usize,
    ) -> Result<ModelFileNameKey, ModelError> {
        let key = ModelFileNameKey {
            arena: self.lookup_owner.reference(0),
            slot,
        };
        self.filename(key)?;
        Ok(key)
    }
    fn allocate_model_filename(&mut self, model: ModelKey, bytes: &[u8]) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        if self.models[mi].filename.is_some() {
            return Err(ModelError::FilenameAlreadyOwned(model));
        }
        let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(bytes.len())
            .map_err(|_| ModelError::AllocationOverflow)?;
        owned.extend_from_slice(bytes);
        self.filenames
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        let key = ModelFileNameKey {
            arena: self.lookup_owner.reference(0),
            slot: self.filenames.len(),
        };
        self.filenames.push(Some(ModelFileName {
            bytes: owned,
            references: 1,
        }));
        self.models[mi].filename = Some(key);
        Ok(())
    }
    /// Filename-pointer copy/retain observed at DuplicateHierarchy RVA0x33e37.
    /// The caller must already have closed the destination. This isolated step
    /// does not duplicate children or implement the whole DuplicateHierarchy.
    pub fn copy_model_filename_reference(
        &mut self,
        destination: ModelKey,
        source: ModelKey,
    ) -> Result<(), ModelError> {
        let si = self.model_index(source)?;
        let di = self.model_index(destination)?;
        if self.models[di].filename.is_some() {
            return Err(ModelError::FilenameAlreadyOwned(destination));
        }
        if let Some(key) = self.models[si].filename {
            let record = self.filenames[key.slot]
                .as_mut()
                .ok_or(ModelError::UnknownFilename(key))?;
            record.references = record
                .references
                .checked_add(1)
                .ok_or(ModelError::FilenameReferenceOverflow(key))?;
            self.models[di].filename = Some(key);
        }
        Ok(())
    }
    /// Materialize the admitted parsed frame subset into an existing shared
    /// arena. Root identity, other models, scene ownership and external frame
    /// references remain in that arena. As in LoadHierarchy603e0, all constructed
    /// frames transfer in file order before an intake error is returned.
    /// Mesh payload remains parsed backing; this does not create GPU resources.
    pub fn load_parsed_into_model(
        &mut self,
        model: ModelKey,
        source: Arc<Model>,
    ) -> Result<SharedModelContent, SharedModelLoadFailure> {
        self.load_parsed_into_model_for_root(model, source, None)
    }
    pub(crate) fn load_parsed_into_model_for_root(
        &mut self,
        model: ModelKey,
        source: Arc<Model>,
        root_parent: Option<FrameKey>,
    ) -> Result<SharedModelContent, SharedModelLoadFailure> {
        let mut content = SharedModelContent {
            model,
            frames: Vec::new(),
            transferred_frames: 0,
            arena: self.lookup_owner.reference(0),
            epoch: self.lifecycle_epoch(model).unwrap_or(0),
        };
        let preflight = (|| {
            let mi = self.model_index(model)?;
            if self.models[mi].closing {
                return Err(ModelError::CloseIncomplete(model));
            }
            if !self.models[mi].frames.is_empty() {
                return Err(ModelError::ModelNotEmpty(model));
            }
            Ok(())
        })();
        if let Err(error) = preflight {
            return Err(SharedModelLoadFailure {
                error,
                partial: content,
            });
        }
        let mut morph_poses = Vec::new();
        let mut result = Ok(());
        for index in 0..source.frames.len() {
            result = self.load_parsed_frame(&source, &mut content.frames, &mut morph_poses, index);
            if result.is_err() {
                break;
            }
        }
        if result.is_ok() {
            for (index, frame) in source.frames.iter().enumerate() {
                if let Some(record) = &frame.target {
                    result = I3dTargetState::from_parsed_fields(
                        record.flags,
                        &record.frame_indices,
                        &content.frames,
                    )
                    .map_err(ModelError::Target)
                    .and_then(|state| {
                        self.graph
                            .apply_target_load_image_state(content.frames[index], state)
                            .map_err(ModelError::from)
                    });
                    if result.is_err() {
                        break;
                    }
                }
            }
        }
        if let Err(error) = self.transfer_shared_model_content_for_root(&mut content, root_parent) {
            return Err(SharedModelLoadFailure {
                error,
                partial: content,
            });
        }
        if result.is_ok() && root_parent.is_none() {
            result = self.mark_loaded_model_root(model.root());
        }
        match result {
            Ok(()) => Ok(content),
            Err(error) => Err(SharedModelLoadFailure {
                error,
                partial: content,
            }),
        }
    }
    /// Resume only the pending ownership transfers of a failed host intake.
    /// The source mapping must belong to this arena and retain its creator
    /// references; successfully transferred entries are never released twice.
    pub fn transfer_shared_model_content(
        &mut self,
        content: &mut SharedModelContent,
    ) -> Result<(), ModelError> {
        self.transfer_shared_model_content_for_root(content, None)
    }
    fn transfer_shared_model_content_for_root(
        &mut self,
        content: &mut SharedModelContent,
        root_parent: Option<FrameKey>,
    ) -> Result<(), ModelError> {
        if content.arena != self.lookup_owner.reference(0) {
            return Err(ModelError::ForeignArena);
        }
        if content.epoch != self.lifecycle_epoch(content.model)? {
            return Err(ModelError::StaleParsedContent(content.model));
        }
        while content.transferred_frames < content.frames.len() {
            let key = content.frames[content.transferred_frames];
            if self.graph.links(key)?.parent.is_none() {
                self.link_frame(key, Some(root_parent.unwrap_or(content.model.root())), 0)?;
            }
            self.add_frame(content.model, key)?;
            self.release(key)?;
            content.transferred_frames += 1;
        }
        Ok(())
    }
    /// Actual Installation read and static-scene parser connected to the shared
    /// arena. Close precedes lookup, and failed replacement retains the parsed
    /// loaded prefix. A successful load owns a fresh filename descriptor.
    /// This is a host path API, not original Open335a0: it accepts .4ds directly,
    /// has no loader callbacks or animation path.
    /// The parser itself rejects malformed bytes before frame construction.
    pub fn reopen_model_asset(
        &mut self,
        model: ModelKey,
        installation: &lhr_assets::Installation,
        path: &str,
    ) -> Result<SharedModelContent, SharedModelLoadFailure> {
        let identity = self.lookup_owner.reference(0);
        let epoch = self.lifecycle_epoch(model).unwrap_or(0);
        let empty = || SharedModelContent {
            model,
            frames: Vec::new(),
            transferred_frames: 0,
            arena: identity,
            epoch,
        };
        self.close(model).map_err(|error| SharedModelLoadFailure {
            error,
            partial: empty(),
        })?;
        let bytes = installation
            .read(path)
            .map_err(|error| SharedModelLoadFailure {
                error: ModelError::Asset(error.to_string()),
                partial: empty(),
            })?;
        let parsed =
            lhr_assets::parse_four_ds_scene(&bytes).map_err(|error| SharedModelLoadFailure {
                error: ModelError::Asset(error.to_string()),
                partial: empty(),
            })?;
        let content = self.load_parsed_into_model(model, Arc::new(parsed))?;
        if let Err(error) = self.allocate_model_filename(model, path.as_bytes()) {
            return Err(SharedModelLoadFailure {
                error,
                partial: content,
            });
        }
        if let Err(error) = self.propagate_model_name(model) {
            return Err(SharedModelLoadFailure {
                error,
                partial: content,
            });
        }
        Ok(content)
    }
    /// Static zero-flag/no-callback Open composition: close first, follow the
    /// original LoadHierarchy filename rewrite, restore fast-find after loading,
    /// create a fresh filename descriptor, then propagate a named model prefix.
    /// Geometry/texture ownership is the concrete CPU adapter, not native
    /// LoadImage allocation or GPU upload. Malformed source files, animations,
    /// unsupported visuals and backend failures remain host availability errors;
    /// their native loader failure prefixes/diagnostics are not emulated.
    pub fn open_original_static_model(
        &mut self,
        model: ModelKey,
        installation: &lhr_assets::Installation,
        input_name: &str,
    ) -> Result<SharedModelContent, SharedModelLoadFailure> {
        self.open_original_static_profile(model, installation, input_name, false)
    }
    /// Original cache caller profile: flags0x10 with non-null410c50 thunk and
    /// null outer callback. Static geometry admits no animation set, and the
    /// original progress-event8 calls are disabled because flag2 is clear.
    /// This CPU resource adapter does not claim native LoadImage allocation.
    pub fn open_original_static_cache_model(
        &mut self,
        model: ModelKey,
        installation: &lhr_assets::Installation,
        input_name: &str,
    ) -> Result<SharedModelContent, SharedModelLoadFailure> {
        self.open_original_static_profile(model, installation, input_name, true)
    }
    pub fn static_model_load_profile(&self) -> Option<StaticModelLoadProfile> {
        self.static_load_profile
    }
    fn open_original_static_profile(
        &mut self,
        model: ModelKey,
        installation: &lhr_assets::Installation,
        input_name: &str,
        cache_thunk: bool,
    ) -> Result<SharedModelContent, SharedModelLoadFailure> {
        let arena = self.lookup_owner.reference(0);
        let empty = || SharedModelContent {
            model,
            frames: vec![],
            transferred_frames: 0,
            arena,
            epoch: 0,
        };
        self.close(model).map_err(|error| SharedModelLoadFailure {
            error,
            partial: empty(),
        })?;
        self.static_load_profile = Some(if cache_thunk {
            StaticModelLoadProfile::CacheThunkNoOuter
        } else {
            StaticModelLoadProfile::NoCallback
        });
        let filename = input_name.as_bytes();
        let filename = &filename[..filename
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(filename.len())];
        let path =
            original_static_geometry_path(filename).map_err(|error| SharedModelLoadFailure {
                error,
                partial: empty(),
            })?;
        let fast_find = self.fast_tree.enabled;
        let named = !self
            .frame(model.root())
            .map_err(|error| SharedModelLoadFailure {
                error,
                partial: empty(),
            })?
            .name_bytes
            .is_empty();
        if named {
            self.fast_tree.enabled = false;
        }
        let loaded = (|| {
            let bytes =
                installation
                    .read_original_path(&path)
                    .map_err(|error| SharedModelLoadFailure {
                        error: ModelError::Asset(error.to_string()),
                        partial: empty(),
                    })?;
            let source = lhr_assets::parse_four_ds_scene(&bytes).map_err(|error| {
                SharedModelLoadFailure {
                    error: ModelError::Asset(error.to_string()),
                    partial: empty(),
                }
            })?;
            let content = self.load_parsed_into_model(model, Arc::new(source))?;
            if let Err(error) = self.load_plain_visual_resources(model, installation) {
                return Err(SharedModelLoadFailure {
                    error,
                    partial: content,
                });
            }
            Ok(content)
        })();
        self.fast_tree.enabled = fast_find;
        let content = loaded?;
        if let Err(error) = self.allocate_model_filename(model, filename) {
            return Err(SharedModelLoadFailure {
                error,
                partial: content,
            });
        }
        if named {
            if let Err(error) = self.propagate_model_name(model) {
                return Err(SharedModelLoadFailure {
                    error,
                    partial: content,
                });
            }
        }
        Ok(content)
    }
    pub(crate) fn frame_index(&self, key: FrameKey) -> Result<usize, ModelError> {
        let i = key.0.checked_sub(1).ok_or(ModelError::UnknownFrame(key))? as usize;
        self.frames
            .get(i)
            .filter(|f| f.key == key && f.alive)
            .map(|_| i)
            .ok_or(ModelError::UnknownFrame(key))
    }
    pub(crate) fn model_index(&self, key: ModelKey) -> Result<usize, ModelError> {
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
    pub fn user_data(
        &self,
        key: FrameKey,
    ) -> Result<crate::actor_frames::FrameUserData, ModelError> {
        Ok(self.frame(key)?.user_data)
    }
    /// Import the caller-owned userdata record projection. This neither writes
    /// through the native pointer nor creates/retains the actor it refers to.
    pub fn set_user_data(
        &mut self,
        key: FrameKey,
        data: crate::actor_frames::FrameUserData,
    ) -> Result<(), ModelError> {
        let index = self.frame_index(key)?;
        self.frames[index].user_data = match data {
            crate::actor_frames::FrameUserData::ExternalRecord(0) => {
                crate::actor_frames::FrameUserData::Null
            }
            value => value,
        };
        Ok(())
    }
    pub fn set_actor_data(
        &mut self,
        key: FrameKey,
        data: crate::actor_world::FrameActorData,
    ) -> Result<(), ModelError> {
        self.frame_index(key)?;
        let data = match data {
            crate::actor_world::FrameActorData::Null => crate::actor_frames::FrameUserData::Null,
            crate::actor_world::FrameActorData::Present(actor) => {
                crate::actor_frames::FrameUserData::ActorRecord(
                    self.create_actor_data_record(actor)?,
                )
            }
        };
        self.set_user_data(key, data)
    }
    /// Import only the first actor word of an externally supplied record. The
    /// record belongs to this owner independently of every frame pointer to it.
    pub fn create_actor_data_record(
        &mut self,
        actor: Option<crate::script_scheduler::ActorReferenceId>,
    ) -> Result<crate::actor_frames::ActorDataRecordId, ModelError> {
        let raw = u32::try_from(self.actor_data_records.len())
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or(ModelError::AllocationOverflow)?;
        self.actor_data_records
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        let identity = crate::actor_frames::ActorDataRecordId(self.lookup_reference(FrameKey(raw)));
        self.actor_data_records
            .push(crate::actor_frames::ActorDataRecord {
                identity,
                actor,
                alive: true,
            });
        Ok(identity)
    }
    fn actor_data_record_index(
        &self,
        record: crate::actor_frames::ActorDataRecordId,
    ) -> Result<usize, ModelError> {
        let index = record
            .0
            .identity()
            .checked_sub(1)
            .ok_or(ModelError::UnknownActorDataRecord(record))? as usize;
        self.actor_data_records
            .get(index)
            .filter(|entry| entry.alive && entry.identity == record)
            .map(|_| index)
            .ok_or(ModelError::UnknownActorDataRecord(record))
    }
    pub fn actor_data_record(
        &self,
        record: crate::actor_frames::ActorDataRecordId,
    ) -> Result<Option<crate::script_scheduler::ActorReferenceId>, ModelError> {
        Ok(self.actor_data_records[self.actor_data_record_index(record)?].actor)
    }
    pub fn set_actor_data_record(
        &mut self,
        record: crate::actor_frames::ActorDataRecordId,
        actor: Option<crate::script_scheduler::ActorReferenceId>,
    ) -> Result<(), ModelError> {
        let index = self.actor_data_record_index(record)?;
        self.actor_data_records[index].actor = actor;
        Ok(())
    }
    /// Explicit host lifetime end of the external record. This is not a native
    /// record destructor. Any surviving frame pointer then fails closed on read.
    pub fn retire_actor_data_record(
        &mut self,
        record: crate::actor_frames::ActorDataRecordId,
    ) -> Result<(), ModelError> {
        let index = self.actor_data_record_index(record)?;
        self.actor_data_records[index].alive = false;
        Ok(())
    }
    pub(crate) fn set_frame_on(&mut self, key: FrameKey, on: u32) -> Result<(), ModelError> {
        self.frame_index(key)?;
        // Scene's virtual SetOn slot is the empty PC RVA 0x5240 override.
        // Sector and the other admitted kinds retain base SetOn semantics.
        if self.graph.state(key)?.kind != FrameKind::Scene {
            self.graph.set_on(key, on)?;
        }
        Ok(())
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
    /// Common base tail after a separately executed, admitted scene/sector
    /// destructor. General Release deliberately retains its derived guard.
    pub(crate) fn destroy_empty_scene_base(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let index = self.frame_index(key)?;
        if self.frames[index].owner.is_some()
            || self.frames[index].object_resources.is_some()
            || self.frames[index].projector.is_some()
        {
            return Err(crate::frame_transform::FrameError::UnsupportedDestruction(key).into());
        }
        self.graph.check_empty_scene_base_destruction(key)?;
        self.remove_frame_name_registration(key)?;
        self.graph.destroy_base(key)?;
        let frame = &mut self.frames[index];
        frame.references = 0;
        frame.alive = false;
        frame.properties = Vec::new();
        frame.name_bytes = Vec::new();
        frame.original_name_bytes = None;
        frame.source = None;
        frame.user_data = crate::actor_frames::FrameUserData::Null;
        self.last_destroyed_frame = Some(key);
        Ok(())
    }
    /// Release the owned reference. Unsupported derived effects are rejected
    /// before decrementing the last reference; completed earlier Close releases
    /// remain observable. Dead stable keys never identify a later allocation.
    pub fn release(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.check_scene_frame_available(key)?;
        self.release_inner(key)
    }
    pub(crate) fn release_inner(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let i = self.frame_index(key)?;
        if self.frames[i].references > 1 {
            self.frames[i].references -= 1;
            return Ok(());
        }
        self.check_frame_destruction(key)?;
        let owner_position = if let Some(owner) = self.frames[i].owner {
            self.check_owner_available(owner)?;
            let position = self
                .owner_frames(owner)?
                .iter()
                .position(|&k| k == key)
                .ok_or_else(|| match owner {
                    crate::scene_ownership::FrameOwner::Model(model) => {
                        ModelError::FrameNotOwned { model, frame: key }
                    }
                    crate::scene_ownership::FrameOwner::Scene(scene) => {
                        crate::scene_ownership::SceneOwnerError::FrameNotOwned { scene, frame: key }
                            .into()
                    }
                })?;
            Some((owner, position))
        } else {
            None
        };
        if self.graph.state(key)?.kind == FrameKind::Camera {
            self.destroy_camera_derived(self.camera_key(key)?)?;
        }
        if self.graph.state(key)?.kind == FrameKind::Occluder {
            self.destroy_occluder_derived(self.occluder_key(key)?)?;
        }
        if self.graph.state(key)?.kind == FrameKind::Sector {
            self.destroy_sector_derived(crate::scene_ownership::SectorKey(key))?;
        }
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
        if let Some(object) = self.frames[i].object_resources {
            self.loaded_visual_resources
                .as_mut()
                .ok_or(ModelError::VisualBoundsUnavailable(key))?
                .destroy_object(object)
                .map_err(ModelError::LoadedVisual)?;
            self.frames[i].object_resources = None;
            self.frames[i].visual_bounds = None;
        }
        if self.frames[i].projector.is_some() {
            self.release_projector_resources(key)?;
        }
        self.release_light_sectors(key)?;
        self.graph.destroy_derived(key)?;
        self.remove_frame_name_registration(key)?;
        // Derived teardown precedes base owner removal. The zero-count base
        // destructor removes membership without an additional Release.
        if let Some((owner, position)) = owner_position {
            self.owner_frames_mut(owner)?.swap_remove(position);
            self.frames[i].owner = None;
        }
        self.graph.destroy_base(key)?;
        self.last_destroyed_frame = Some(key);
        self.frames[i].references = 0;
        self.frames[i].alive = false;
        self.frames[i].properties = Vec::new();
        self.frames[i].name_bytes = Vec::new();
        self.frames[i].original_name_bytes = None;
        self.frames[i].dummy_bounds = None;
        self.frames[i].source = None;
        self.frames[i].user_data = crate::actor_frames::FrameUserData::Null;
        self.sectors.retain(|sector| sector.key.frame() != key);
        Ok(())
    }
    pub fn add_frame(&mut self, model: ModelKey, key: FrameKey) -> Result<(), ModelError> {
        self.check_scene_frame_available(key)?;
        let mi = self.model_index(model)?;
        let fi = self.frame_index(key)?;
        if self.models[mi].closing {
            return Err(ModelError::CloseIncomplete(model));
        }
        let mut ancestor = Some(crate::scene_ownership::FrameOwner::Model(model));
        while let Some(current) = ancestor {
            if current.root() == key {
                return Err(ModelError::OwnershipCycle);
            }
            ancestor = self.frame(current.root())?.owner;
        }
        if let Some(previous) = self.frames[fi].owner {
            self.check_owner_available(previous)?;
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
            self.delete_from_owner_key(previous, key)?;
        }
        if self.models[mi].frames.len() >= self.models[mi].native_capacity {
            self.models[mi].native_capacity += self.models[mi].growth;
        }
        self.models[mi].frames.push(key);
        self.frames[fi].owner = Some(model.into());
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
            self.check_frame_destruction(key)?;
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
    /// StopAnimation(index), LS3DF33890. Only the constructor-owned null set
    /// branch is admitted here; host playback cannot be substituted for+150.
    /// GetAnimationSet(index), original LS3DF RVA0x33860. Constructor-owned
    /// null IPolSet and unsigned out-of-range index return null; the external
    /// host playback object is not an original controller resource owner.
    pub fn original_model_animation_set(
        &self,
        model: ModelKey,
        index: u32,
    ) -> Result<Option<crate::interpolator::AnimSetId>, ModelError> {
        let mi = self.model_index(model)?;
        if index >= 8 {
            return Ok(None);
        }
        if self.models[mi].original_ipol_set.is_some() {
            return Err(ModelError::UnsupportedActiveModelController(model));
        }
        Ok(None)
    }
    pub fn stop_original_model_animation(
        &mut self,
        model: ModelKey,
        index: u32,
    ) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        if index >= 8 {
            return Ok(());
        }
        if self.models[mi].original_ipol_set.is_some() {
            return Err(ModelError::UnsupportedActiveModelController(model));
        }
        Ok(())
    }
    pub fn close(&mut self, model: ModelKey) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        if self.models[mi].original_ipol_set.is_some() {
            return Err(ModelError::UnsupportedActiveModelController(model));
        }
        self.models[mi].lifecycle_epoch = self.models[mi]
            .lifecycle_epoch
            .checked_add(1)
            .ok_or(ModelError::AllocationOverflow)?;
        self.models[mi].closing = true;
        while self.models[mi].close_cursor < self.models[mi].frames.len() {
            let key = self.models[mi].frames[self.models[mi].close_cursor];
            let fi = self.frame_index(key)?;
            if self.frames[fi].references <= 1 {
                self.check_frame_destruction(key)?;
            }
            self.frames[fi].owner = None;
            if let Err(error) = self.release(key) {
                // The unsupported current slot still belongs to this pending
                // host Close, including a nested model with a completed prefix.
                self.frames[fi].owner = Some(model.into());
                return Err(error);
            }
            self.models[mi].close_cursor += 1;
        }
        self.models[mi].frames.clear();
        self.models[mi].close_cursor = 0;
        self.models[mi].closing = false;
        // PC Close33500 releases filename only after the forward frame loop.
        if let Some(filename) = self.models[mi].filename {
            let record = self.filenames[filename.slot]
                .as_mut()
                .ok_or(ModelError::UnknownFilename(filename))?;
            record.references -= 1;
            if record.references <= 0 {
                self.filenames[filename.slot] = None;
            }
            self.models[mi].filename = None;
        }
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
        add(self
            .actor_data_records
            .capacity()
            .checked_mul(std::mem::size_of::<crate::actor_frames::ActorDataRecord>())
            .ok_or(ModelError::AllocationOverflow)?)?;
        add(self
            .scenes
            .capacity()
            .checked_mul(std::mem::size_of::<crate::scene_ownership::SceneFrameOwner>())
            .ok_or(ModelError::AllocationOverflow)?)?;
        add(self
            .sectors
            .capacity()
            .checked_mul(std::mem::size_of::<crate::scene_ownership::SectorFrame>())
            .ok_or(ModelError::AllocationOverflow)?)?;
        for scene in &self.scenes {
            add(scene
                .frames
                .capacity()
                .checked_mul(std::mem::size_of::<FrameKey>())
                .ok_or(ModelError::AllocationOverflow)?)?;
        }
        add(self
            .filenames
            .capacity()
            .checked_mul(std::mem::size_of::<Option<ModelFileName>>())
            .ok_or(ModelError::AllocationOverflow)?)?;
        for filename in self.filenames.iter().flatten() {
            add(filename.bytes.capacity())?;
        }
        if let Some(owner) = &self.loaded_visual_resources {
            // Backing payloads dominate CPU resource storage. Manager/descriptor
            // map allocator overhead is not represented by this estimate.
            add(owner.backend().graphics_bytes())?;
            add(owner.backend().decoded_texture_bytes())?;
        }
        for f in &self.frames {
            add(f.name_bytes.capacity())?;
            if let Some(original) = &f.original_name_bytes {
                add(original.capacity())?;
            }
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

/// Filename scan/rewrite from LoadHierarchy603e0, restricted to the provider's
/// admitted ASCII domain. The native scan excludes a dot at indices zero/one.
fn original_static_geometry_path(filename: &[u8]) -> Result<String, ModelError> {
    if filename.is_empty() {
        return Err(ModelError::NativeModelOpenResult(0x80000007));
    }
    if !filename.is_ascii() {
        return Err(ModelError::OriginalModelOpenUnavailable(
            "non-ASCII source path",
        ));
    }
    let prefix = &filename[..filename.len().min(269)];
    let dot = prefix
        .iter()
        .rposition(|&byte| byte == b'.')
        .filter(|&index| index >= 2)
        .ok_or(ModelError::NativeModelOpenResult(0x80000007))?;
    if prefix[dot..].eq_ignore_ascii_case(b".4ds") {
        return Err(ModelError::NativeModelOpenResult(0x80000007));
    }
    if dot > 265 {
        return Err(ModelError::OriginalModelOpenUnavailable(
            "native path rewrite exceeds filename buffer",
        ));
    }
    let mut path = String::from_utf8(prefix[..dot].to_vec()).unwrap();
    path.push_str(".4ds");
    Ok(path)
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
        if let Err(error) = result.arena.mark_loaded_model_root(root.root()) {
            return Err(ModelLoadFailure {
                error,
                partial: Box::new(result),
            });
        }
        result.complete = true;
        Ok(result)
    }
    fn load_frame(&mut self, index: usize) -> Result<(), ModelError> {
        self.arena.load_parsed_frame(
            &self.source,
            &mut self.source_frame_keys,
            &mut self.morph_poses,
            index,
        )
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
                self.arena.link_frame(key, Some(self.root.root()), 0)?;
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
                    .is_ok_and(|frame| frame.owner == Some(self.root.into()))
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
        self.arena.mark_loaded_model_root(self.root.root())?;
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
        self.arena.update_frame(self.root.root())?;
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
        self.arena.update_frame(self.root.root())?;
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
mod original_controller_admission_tests {
    use super::*;
    #[test]
    fn original_active_controller_is_not_replaced_by_host_playback_or_cleared() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        arena.stop_original_model_animation(model, 0).unwrap();
        for index in [0, 7, 8, u32::MAX] {
            assert_eq!(arena.original_model_animation_set(model, index), Ok(None));
        }
        let i = arena.model_index(model).unwrap();
        let epoch = arena.models[i].lifecycle_epoch;
        arena.models[i].original_ipol_set = Some(crate::scene_tick::InterpolatorId(7));
        arena.stop_original_model_animation(model, 8).unwrap();
        assert_eq!(
            arena.original_model_animation_set(model, 0),
            Err(ModelError::UnsupportedActiveModelController(model))
        );
        for index in [8, u32::MAX] {
            assert_eq!(arena.original_model_animation_set(model, index), Ok(None));
        }
        assert_eq!(
            arena.stop_original_model_animation(model, 0),
            Err(ModelError::UnsupportedActiveModelController(model))
        );
        assert_eq!(
            arena.close(model),
            Err(ModelError::UnsupportedActiveModelController(model))
        );
        assert_eq!(
            arena.models[i].original_ipol_set,
            Some(crate::scene_tick::InterpolatorId(7))
        );
        assert_eq!(arena.models[i].lifecycle_epoch, epoch);
        assert!(!arena.models[i].closing);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn frame(kind: u8, parent: Option<usize>, position: [f32; 3]) -> lhr_assets::Frame {
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
            occluder: None,
            sector: None,
            parent,
            position,
            scale: [1.; 3],
            rotation_wxyz: [1., 0., 0., 0.],
            instance: None,
            lods: Vec::new(),
        }
    }
    #[test]
    fn shared_arena_load_close_keeps_external_survivors_and_other_models() {
        let source = Arc::new(Model {
            materials: vec![],
            frames: vec![frame(6, None, [1., 2., 3.])],
        });
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let first = arena.create_model().unwrap();
        let second = arena.create_model().unwrap();
        arena.scene_add_frame(scene, first.root()).unwrap();
        arena.scene_add_frame(scene, second.root()).unwrap();
        let first_content = arena.load_parsed_into_model(first, source.clone()).unwrap();
        let second_content = arena
            .load_parsed_into_model(second, source.clone())
            .unwrap();
        let survivor = first_content.frames()[0];
        arena.retain(survivor).unwrap();
        arena.close(first).unwrap();
        assert_eq!(arena.frame(survivor).unwrap().owner, None);
        assert_eq!(arena.links(survivor).unwrap().parent, Some(first.root()));
        assert!(Arc::ptr_eq(
            arena.frame(survivor).unwrap().source.as_ref().unwrap(),
            &source
        ));
        assert_eq!(arena.model(second).unwrap().frames, second_content.frames());
        let reloaded = arena.load_parsed_into_model(first, source).unwrap();
        assert_ne!(reloaded.frames()[0], survivor);
        arena.release(survivor).unwrap();
        assert!(arena.frame(survivor).is_err());
        arena.close(first).unwrap();
        arena.close(second).unwrap();
        assert_eq!(
            arena.scene(scene).unwrap().frames(),
            &[first.root(), second.root()]
        );
    }
    #[test]
    fn shared_intake_failure_transfers_prefix_and_rejects_stale_or_foreign_resume() {
        let source = Arc::new(Model {
            materials: vec![],
            frames: vec![frame(6, None, [0.; 3]), frame(99, None, [0.; 3])],
        });
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let mut failure = arena.load_parsed_into_model(model, source).unwrap_err();
        assert!(matches!(
            failure.error,
            ModelError::UnsupportedFrame { index: 1, .. }
        ));
        assert_eq!(failure.partial.transferred_frames(), 1);
        assert_eq!(
            arena.frame(failure.partial.frames()[0]).unwrap().references,
            1
        );
        let mut other = ModelArena::default();
        other.create_model().unwrap();
        assert_eq!(
            other.transfer_shared_model_content(&mut failure.partial),
            Err(ModelError::ForeignArena)
        );
        arena.close(model).unwrap();
        assert_eq!(
            arena.transfer_shared_model_content(&mut failure.partial),
            Err(ModelError::StaleParsedContent(model))
        );
    }
    #[test]
    fn model_prefix_uses_original_names_across_repeated_rename_and_reset() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let content = arena
            .load_parsed_into_model(
                model,
                Arc::new(Model {
                    materials: vec![],
                    frames: vec![frame(6, None, [0.; 3])],
                }),
            )
            .unwrap();
        let key = content.frames()[0];
        arena.set_fast_find_enabled(true);
        arena.set_frame_name(model.root(), Some(b"taxi")).unwrap();
        arena.propagate_model_name(model).unwrap();
        assert_eq!(arena.frame(key).unwrap().name_bytes, b"taxi.fixture");
        assert_eq!(
            arena.original_name_pointer(key).unwrap(),
            crate::frame_fast_lookup::OriginalFrameName::Offset(5)
        );
        arena.set_frame_name(model.root(), Some(b"other")).unwrap();
        arena.propagate_model_name(model).unwrap();
        assert_eq!(arena.frame(key).unwrap().name_bytes, b"other.fixture");
        arena.set_frame_name(model.root(), None).unwrap();
        arena.propagate_model_name(model).unwrap();
        assert_eq!(arena.frame(key).unwrap().name_bytes, b"fixture");
        assert_eq!(
            arena.original_name_pointer(key).unwrap(),
            crate::frame_fast_lookup::OriginalFrameName::Offset(0)
        );
        arena.close(model).unwrap();
        assert_eq!(arena.fast_tree_root(), None);
    }
    #[test]
    fn filename_allocations_are_fresh_sharing_explicit_and_close_releases_last() {
        let mut arena = ModelArena::default();
        let a = arena.create_model().unwrap();
        let b = arena.create_model().unwrap();
        let c = arena.create_model().unwrap();
        arena.allocate_model_filename(a, b"models/car.4ds").unwrap();
        arena.allocate_model_filename(b, b"models/car.4ds").unwrap();
        let a_name = arena.model_filename(a).unwrap().unwrap();
        let b_name = arena.model_filename(b).unwrap().unwrap();
        assert_ne!(a_name, b_name);
        arena.copy_model_filename_reference(c, a).unwrap();
        assert_eq!(arena.filename(a_name).unwrap().references(), 2);
        arena.close(a).unwrap();
        assert_eq!(arena.filename(a_name).unwrap().references(), 1);
        arena.close(c).unwrap();
        assert!(arena.filename(a_name).is_err());
        assert_eq!(arena.filename(b_name).unwrap().references(), 1);
        let other = ModelArena::default();
        assert_eq!(
            other.filename(b_name).unwrap_err(),
            ModelError::ForeignArena
        );
    }
    #[test]
    fn vehicle_projection_reads_live_filename_ownership_and_rejects_stale_or_oversize() {
        use crate::vehicle_frame_backend::{
            VehicleFrameBackend, VehicleFrameError, VehicleFrameMemory,
        };
        use crate::vehicle_init::VehicleInitBackend;
        let mut arena = ModelArena::default();
        let a = arena.create_model().unwrap();
        let b = arena.create_model().unwrap();
        let c = arena.create_model().unwrap();
        let oversized = arena.create_model().unwrap();
        let raw_name = b"models\\car.i3d";
        arena.allocate_model_filename(a, raw_name).unwrap();
        arena.allocate_model_filename(b, raw_name).unwrap();
        arena.copy_model_filename_reference(c, a).unwrap();
        arena
            .allocate_model_filename(oversized, &vec![b'x'; 0x10000])
            .unwrap();
        let mut heap = VehicleFrameMemory::default();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        let a_frame = backend.frame_address(a.root()).unwrap();
        let b_frame = backend.frame_address(b.root()).unwrap();
        let c_frame = backend.frame_address(c.root()).unwrap();
        let oversized_frame = backend.frame_address(oversized.root()).unwrap();
        let pointer = backend.read_word(a_frame + 0x154).unwrap();
        assert_ne!(pointer, backend.read_word(b_frame + 0x154).unwrap());
        assert_eq!(pointer, backend.read_word(c_frame + 0x154).unwrap());
        assert_eq!(backend.read_word(pointer - 4).unwrap(), 2);
        for (offset, expected) in raw_name.iter().copied().chain([0]).enumerate() {
            assert_eq!(
                backend.read_byte(pointer + offset as u32).unwrap(),
                expected
            );
        }
        let beyond = pointer + raw_name.len() as u32 + 1;
        assert_eq!(
            backend.read_byte(beyond),
            Err(VehicleFrameError::Address(beyond))
        );
        assert_eq!(
            backend.write_byte(pointer, b'X'),
            Err(VehicleFrameError::Address(pointer))
        );
        assert_eq!(
            backend.free(pointer),
            Err(VehicleFrameError::Address(pointer))
        );
        assert!(matches!(
            backend.read_word(oversized_frame + 0x154),
            Err(VehicleFrameError::Address(_))
        ));
        backend.arena_mut().close(a).unwrap();
        assert_eq!(backend.read_word(a_frame + 0x154).unwrap(), 0);
        assert_eq!(backend.read_word(pointer - 4).unwrap(), 1);
        backend.arena_mut().close(c).unwrap();
        assert!(matches!(
            backend.read_byte(pointer),
            Err(VehicleFrameError::Arena(ModelError::UnknownFilename(_)))
        ));
        backend
            .arena_mut()
            .allocate_model_filename(a, raw_name)
            .unwrap();
        assert_ne!(pointer, backend.read_word(a_frame + 0x154).unwrap());
        assert!(matches!(
            backend.read_byte(pointer),
            Err(VehicleFrameError::Arena(ModelError::UnknownFilename(_)))
        ));
    }
    #[test]
    fn original_static_path_rewrites_and_preserves_the_native_extension_rejection() {
        assert_eq!(
            original_static_geometry_path(b"models/example.i3d").unwrap(),
            "models/example.4ds"
        );
        assert_eq!(
            original_static_geometry_path(b"ab.other").unwrap(),
            "ab.4ds"
        );
        for name in [
            b"a.i3d".as_slice(),
            b".i3d",
            b"ab.4ds",
            b"ab.4DS",
            b"no_extension",
            b"",
        ] {
            assert_eq!(
                original_static_geometry_path(name),
                Err(ModelError::NativeModelOpenResult(0x80000007))
            );
        }
        let mut name = b"ab.".to_vec();
        name.extend(std::iter::repeat_n(b'x', 300));
        assert_eq!(original_static_geometry_path(&name).unwrap(), "ab.4ds");
        let mut name = vec![b'x'; 267];
        name.extend_from_slice(b".z");
        assert!(matches!(
            original_static_geometry_path(&name),
            Err(ModelError::OriginalModelOpenUnavailable(_))
        ));
    }
    #[test]
    fn installation_reopen_reads_real_file_closes_before_lookup_and_retains_root() {
        let path = std::env::temp_dir().join(format!(
            "lhr-shared-model-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend([0; 5]);
        std::fs::write(path.join("fixture.4ds"), &bytes).unwrap();
        std::fs::create_dir(path.join("models")).unwrap();
        std::fs::write(path.join("models/fixture.4ds"), &bytes).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        arena
            .reopen_model_asset(model, &installation, "fixture.4ds")
            .unwrap();
        let filename = arena.model_filename(model).unwrap().unwrap();
        assert_eq!(arena.filename(filename).unwrap().bytes(), b"fixture.4ds");
        arena.set_frame_name(model.root(), Some(b"placed")).unwrap();
        arena.set_fast_find_enabled(true);
        arena
            .open_original_static_model(model, &installation, "fixture.i3d")
            .unwrap();
        assert!(arena.fast_tree.enabled);
        assert!(arena.filename(filename).is_err());
        let original_filename = arena.model_filename(model).unwrap().unwrap();
        assert_eq!(
            arena.filename(original_filename).unwrap().bytes(),
            b"fixture.i3d"
        );
        arena
            .open_original_static_model(model, &installation, "models\\fixture.i3d")
            .unwrap();
        assert!(arena.filename(original_filename).is_err());
        let original_filename = arena.model_filename(model).unwrap().unwrap();
        assert_eq!(
            arena.filename(original_filename).unwrap().bytes(),
            b"models\\fixture.i3d"
        );
        let rejected = arena
            .open_original_static_model(model, &installation, "fixture.4ds")
            .unwrap_err();
        assert_eq!(
            rejected.error,
            ModelError::NativeModelOpenResult(0x80000007)
        );
        assert!(arena.filename(original_filename).is_err());
        assert!(arena.fast_tree.enabled);

        let failure = arena
            .reopen_model_asset(model, &installation, "missing.4ds")
            .unwrap_err();
        assert!(matches!(failure.error, ModelError::Asset(_)));
        assert_eq!(arena.model_filename(model).unwrap(), None);
        assert!(arena.filename(filename).is_err());
        assert_eq!(arena.model(model).unwrap().key, model);
        std::fs::remove_dir_all(path).unwrap();
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
            assert_eq!(f.owner, Some(loaded.root.into()));
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
            assert_eq!(
                loaded.arena.frame(key).unwrap().owner,
                Some(loaded.root.into())
            );
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
            Some(failure.partial.root.into())
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
            frames: vec![frame(6, None, [0.; 3]), frame(0xfe, None, [0.; 3])],
        });
        let failure = MaterializedModel::from_parsed(source).unwrap_err();
        assert_eq!(
            failure.error,
            ModelError::UnsupportedFrame {
                index: 1,
                kind: 0xfe,
                visual: None
            }
        );
        let partial = failure.partial;
        assert!(!partial.is_complete());
        assert_eq!(partial.source_frame_keys().len(), 1);
        let key = partial.source_frame_keys()[0];
        assert_eq!(
            partial.arena.frame(key).unwrap().owner,
            Some(partial.root.into())
        );
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
        assert_eq!(loaded.arena.frame(key).unwrap().owner, Some(next.into()));
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
            Some(loaded.root.into())
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
        assert_eq!(
            partial.arena.frame(key).unwrap().owner,
            Some(partial.root.into())
        );
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
        assert_eq!(arena.frame(inner.root()).unwrap().owner, Some(outer.into()));
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

    #[test]
    fn actor_child_search_reads_local_original_name_and_preserves_script_full_name_search() {
        use crate::actor_frames::{ModelActorFrames, ACTOR_FIND_FLAGS};
        use crate::frame_lookup::{get_child_frame, ModelFrameLookup, SCRIPT_FIND_FLAGS};
        let mut target = frame(7, None, [0.; 3]);
        target.name_bytes = b"targetE".to_vec();
        target.target = Some(lhr_assets::TargetRecord {
            flags: 0,
            frame_indices: vec![],
        });
        let mut loaded = MaterializedModel::from_parsed(Arc::new(Model {
            materials: vec![],
            frames: vec![target],
        }))
        .unwrap();
        let key = loaded.source_frame_keys()[0];
        // Import the two distinct native fields as a test state. This does not
        // implement SetModelName's allocation or registration side effects.
        let index = loaded.arena.frame_index(key).unwrap();
        loaded.arena.frames[index].name_bytes = b"prefix.targetE".to_vec();
        let lookup = ModelFrameLookup::borrow(loaded.arena(), true);
        let root = lookup.reference(loaded.root()).unwrap();
        let target = lookup.reference(key).unwrap();
        assert_eq!(
            get_child_frame(&lookup, root, b"prefix.targetE", SCRIPT_FIND_FLAGS, 2),
            Ok(Some(target))
        );
        assert_eq!(
            get_child_frame(&lookup, root, b"targetE", SCRIPT_FIND_FLAGS, 2),
            Ok(None)
        );
        let backend = ModelActorFrames::borrow(loaded.arena_mut(), true);
        assert_eq!(
            backend.find_child_frame(root, b"targetE", ACTOR_FIND_FLAGS, 1),
            Ok(Some(target))
        );
        assert_eq!(
            backend.find_child_frame(root, b"prefix.targetE", ACTOR_FIND_FLAGS, 1),
            Ok(None)
        );
    }

    #[test]
    fn original_name_null_state_follows_constructor_and_zero_length_load_image() {
        let mut empty = frame(6, None, [0.; 3]);
        empty.name_bytes.clear();
        let loaded = MaterializedModel::from_parsed(Arc::new(Model {
            materials: vec![],
            frames: vec![empty, frame(6, None, [0.; 3])],
        }))
        .unwrap();
        assert_eq!(
            loaded
                .arena()
                .frame(loaded.root())
                .unwrap()
                .original_name_bytes,
            None
        );
        let keys = loaded.source_frame_keys();
        assert_eq!(
            loaded.arena().frame(keys[0]).unwrap().original_name_bytes,
            None
        );
        assert_eq!(
            loaded
                .arena()
                .frame(keys[1])
                .unwrap()
                .original_name_bytes
                .as_deref(),
            Some(b"fixture".as_slice())
        );
    }
    #[test]
    fn scene_resource_stage_rejects_unbound_and_unimplemented_effects() {
        use crate::scene_ownership::{SceneCloseEffect, SceneOwnerError};
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        for effect in [
            SceneCloseEffect::FreeUnusedVertexBuffers,
            SceneCloseEffect::FreeUnusedIndexBuffers,
        ] {
            assert!(
                matches!(arena.apply_scene_loaded_resource_effect(scene, effect),
                Err(ModelError::Scene(SceneOwnerError::BackendUnavailable(actual))) if actual == effect)
            );
        }
        assert!(arena.loaded_visual_resources().is_none());
        arena.visual_resource_owner_mut().unwrap();
        let owner = arena.loaded_visual_resources().unwrap() as *const _;
        assert!(matches!(
            arena
                .apply_scene_loaded_resource_effect(scene, SceneCloseEffect::UnloadManagedTextures),
            Err(ModelError::Scene(SceneOwnerError::BackendUnavailable(
                SceneCloseEffect::UnloadManagedTextures
            )))
        ));
        arena
            .apply_scene_loaded_resource_effect(scene, SceneCloseEffect::FreeUnusedVertexBuffers)
            .unwrap();
        arena
            .apply_scene_loaded_resource_effect(scene, SceneCloseEffect::FreeUnusedIndexBuffers)
            .unwrap();
        assert_eq!(arena.loaded_visual_resources().unwrap() as *const _, owner);
        assert_eq!(
            arena.scene(scene).unwrap().close_phase(),
            crate::scene_ownership::ClosePhase::Idle
        );
    }
}
