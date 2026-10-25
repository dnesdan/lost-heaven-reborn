//! Lazy LS3DF frame caches and the closed model/mesh/dummy/joint hierarchy subset.
//! UpdateWMatrixProc is an original DLL export, RVA0x1c7d0. UpdateMatrix,
//! LinkTo, RemoveChild and Update have retained PS2 symbols; PC entries are
//! 0x1a6e0,0x1b9d0,0x1bf20,0x1ca60. Setter names are descriptive views of
//! observed inline operations, not additional original symbols. ModelArena owns
//! references; this graph provides bounded model/dummy/joint/Target teardown.
//! ModelArena also composes attached plain-object resource teardown before the
//! visual/base destructor. Unbound/derived visuals and external registrations
//! remain unavailable.
//! Only LinkTo flags0/1,
//! model/mesh/dummy/joint/Target virtual updates and the owned deferred Target queue
//! are represented.
//! Joint cache and singlemesh discovery/binding use owned graph identities;
//! loaded-definition bounds and inherited visual updates are represented;
//! singlemesh rendering/resource backends remain separate.
use crate::{
    joint_state::{I3dJointState, SinglemeshInvalidationState},
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::{SMatrix, SQuat},
    singlemesh_binding::{DBuildBindingOutcome, I3dSinglemeshBinding},
    singlemesh_bounds::{
        calculate_singlemesh_bounds, SinglemeshBoundVolume, SinglemeshBoundingBox,
        SinglemeshBoundsError,
    },
    target_state::{
        I3dTargetState, TargetFrameBackend, TargetRegistry, TargetUpdateError, TargetUpdateQueue,
    },
};
const DIRTY: u32 = 0x40002000;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameKey(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    Camera,
    Occluder,
    Scene,
    Sector,
    Model,
    Mesh,
    /// Native visual type1, projector subtype PROJ.
    Projector,
    Light,
    Sound,
    Dummy,
    Joint,
    Target,
    /// A Joint::UpdateV target with optional owned binding fields. Only an
    /// owned binding admits the inherited visual update and loaded bounds path.
    Singlemesh,
}
/// Fields observed by these methods, not a complete native object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct I3dFrameState {
    pub kind: FrameKind,
    pub local: SMatrix,
    pub world: SMatrix,
    pub scale: [f32; 3],
    pub rotation: SQuat,
    pub flags: u32,
    pub world_scale: f32,
    pub update_count: u32,
    /// Original frame+0x13c: the driver's render number of the last render.
    /// Constructor value only; actual rendering writes require its producer.
    pub render_number: u32,
    pub visual_flags: u32,
    /// Native dummy-only field at +0x198; independent of mesh visual flags.
    pub dummy_flags: u32,
    /// Projector-only native cache word +0x258, cleared by UpdateV464a0.
    pub projector_cache_258: u32,
}
impl I3dFrameState {
    /// Actual model/base-frame constructor field projection.
    pub fn model() -> Self {
        Self {
            kind: FrameKind::Model,
            local: SMatrix::identity(),
            world: SMatrix::identity(),
            scale: [1.; 3],
            rotation: SQuat {
                wxyz: [1., 0., 0., 0.],
            },
            flags: 0x4000003d,
            world_scale: 1.,
            update_count: 0,
            render_number: u32::MAX,
            visual_flags: 0,
            dummy_flags: 0,
            projector_cache_258: 0,
        }
    }
    /// Original standard Object/LitObject common constructor projection,
    /// RVAs0x367b0/0x21570. Mesh payload and visual subtype remain loader-owned.
    pub fn mesh() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Mesh;
        state.visual_flags = 0x2a00;
        state
    }
    /// Original dummy constructor projection, RVA0x1a1e0. Bounds and other
    /// derived payload fields are owned by the loader arena, outside this graph.
    pub fn dummy() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Dummy;
        state
    }
    /// Common I3D_frame constructor used by I3D_sound, PC RVA0x57650.
    /// Sound sources, playback and the derived tail are owned by RuntimeAudio.
    pub fn sound() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Sound;
        state
    }
    /// Common base-frame projection of the PC Joint constructor, RVA0x1ed30.
    /// The graph separately owns the derived joint fields.
    pub fn joint() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Joint;
        state
    }
    /// Original Target base-frame constructor projection, RVA 0x59490.
    /// Derived links and target flags are registered separately.
    pub fn target() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Target;
        state
    }
    /// Supported FrameLoadImage result. Caller separately proves mesh type/flags.
    pub fn source_mesh(position: [f32; 3], scale: [f32; 3], rotation: SQuat) -> Self {
        let mut s = Self::model();
        s.kind = FrameKind::Mesh;
        s.local.elements[12..15].copy_from_slice(&position);
        s.scale = scale;
        s.rotation = rotation;
        s.flags = 0x40000009;
        s.visual_flags = 0x2a00;
        s
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameLinks {
    pub parent: Option<FrameKey>,
    pub child: Option<FrameKey>,
    pub previous: Option<FrameKey>,
    pub next: Option<FrameKey>,
    pub child_count: u32,
}
#[derive(Clone, Debug)]
struct Node {
    alive: bool,
    state: I3dFrameState,
    links: FrameLinks,
    joint: Option<I3dJointState>,
    light: Option<crate::frame_light::I3dLightState>,
    singlemesh: Option<SinglemeshInvalidationState>,
    binding: Option<I3dSinglemeshBinding>,
    // Host availability markers, not original frame fields. An interrupted
    // guarded computation must not later masquerade as a valid native cache.
    local_error: Option<FrameError>,
    world_error: Option<FrameError>,
    joint_error: Option<FrameError>,
    singlemesh_bound_error: Option<FrameError>,
}
#[derive(Clone, Copy, Debug)]
struct Visit {
    key: FrameKey,
    child: Option<FrameKey>,
    dirty: bool,
}
#[derive(Debug, Default)]
pub struct I3dFrameGraph {
    nodes: Vec<Node>,
    world_path: Vec<FrameKey>,
    visits: Vec<Visit>,
    targets: TargetRegistry,
    target_updates: TargetUpdateQueue,
    deferred_error: Option<FrameError>,
    /// Sector +0x140 invalidation counters shared with registered lights.
    pub(crate) sector_light_changes: std::collections::BTreeMap<u32, u32>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    UnknownFrame(FrameKey),
    IdentityOverflow,
    Cycle,
    NumericDomain(&'static str),
    QuaternionFromMatrixUnavailable,
    AllocationOverflow,
    UnsupportedLinkFlags(u32),
    UnsupportedLinkState,
    UnsupportedDestruction(FrameKey),
    UnsupportedFrameUpdate(FrameKey),
    UnsupportedDestructionRegistrations(FrameKey, u32),
    DirectionFromVectorsUnavailable,
    NotJoint(FrameKey),
    InvalidSinglemeshTarget(FrameKey),
    UnsupportedSinglemeshUpdate(FrameKey),
    JointMatrixUnavailable(FrameKey),
    SinglemeshBindingUnavailable(FrameKey),
    SinglemeshBoundsUnavailable(FrameKey),
    SinglemeshBoundsDefinitionTooShort(FrameKey),
    NotTarget(FrameKey),
    TargetUnavailable(FrameKey),
    UnsupportedTargetFlags(FrameKey, u32),
    TargetDirectionUnavailable(FrameKey),
}
impl I3dFrameGraph {
    pub fn insert(&mut self, state: I3dFrameState) -> Result<FrameKey, FrameError> {
        let id = u32::try_from(self.nodes.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(FrameError::IdentityOverflow)?;
        let required = self.nodes.len() + 1;
        self.nodes
            .try_reserve(1)
            .map_err(|_| FrameError::AllocationOverflow)?;
        self.world_path
            .try_reserve(required.saturating_sub(self.world_path.len()))
            .map_err(|_| FrameError::AllocationOverflow)?;
        self.visits
            .try_reserve(required.saturating_sub(self.visits.len()))
            .map_err(|_| FrameError::AllocationOverflow)?;
        self.nodes.push(Node {
            alive: true,
            state,
            links: FrameLinks::default(),
            joint: (state.kind == FrameKind::Joint).then(I3dJointState::default),
            light: None,
            singlemesh: None,
            binding: None,
            local_error: None,
            world_error: None,
            joint_error: None,
            singlemesh_bound_error: None,
        });
        // Retain bounded traversal storage so the scene charges all allocations
        // before ticking or lazily rendering; traversal cannot grow past nodes.
        Ok(FrameKey(id))
    }
    fn index(&self, key: FrameKey) -> Result<usize, FrameError> {
        let i = key.0.checked_sub(1).ok_or(FrameError::UnknownFrame(key))? as usize;
        if self.nodes.get(i).is_none_or(|node| !node.alive) {
            return Err(FrameError::UnknownFrame(key));
        }
        Ok(i)
    }
    fn node(&self, key: FrameKey) -> Result<&Node, FrameError> {
        Ok(&self.nodes[self.index(key)?])
    }
    fn node_mut(&mut self, key: FrameKey) -> Result<&mut Node, FrameError> {
        let i = self.index(key)?;
        Ok(&mut self.nodes[i])
    }
    /// Preflight the represented model/dummy/joint/Target destructor paths.
    /// Visual resources and external registrations
    /// require their concrete backends. Rejection leaves the graph unchanged.
    #[cfg(test)]
    pub(crate) fn check_destruction(&self, key: FrameKey) -> Result<(), FrameError> {
        self.check_destruction_excluding(key, 0, false, false)
    }
    pub(crate) fn check_destruction_with_owned_name(
        &self,
        key: FrameKey,
    ) -> Result<(), FrameError> {
        self.check_destruction_excluding(key, 0x2000_0000, false, false)
    }
    /// Plain ObjectResources must be owned and released by the arena before
    /// invoking visual5a370/base1a9a0. Unbound or derived visuals stay rejected.
    pub(crate) fn check_destruction_with_owned_visual(
        &self,
        key: FrameKey,
    ) -> Result<(), FrameError> {
        self.check_destruction_excluding(key, 0x2000_0000, true, false)
    }
    /// Caller has executed the represented empty Scene/Sector derived tail.
    pub(crate) fn check_empty_scene_base_destruction(
        &self,
        key: FrameKey,
    ) -> Result<(), FrameError> {
        if !matches!(self.state(key)?.kind, FrameKind::Scene | FrameKind::Sector) {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        self.check_destruction_excluding(key, 0x2000_0000, false, true)
    }
    fn check_destruction_excluding(
        &self,
        key: FrameKey,
        handled: u32,
        plain_visual: bool,
        empty_scene: bool,
    ) -> Result<(), FrameError> {
        let node = self.node(key)?;
        if !matches!(
            node.state.kind,
            FrameKind::Model
                | FrameKind::Dummy
                | FrameKind::Joint
                | FrameKind::Target
                | FrameKind::Light
                | FrameKind::Sound
                | FrameKind::Camera
                | FrameKind::Occluder
        ) && !(empty_scene && matches!(node.state.kind, FrameKind::Scene | FrameKind::Sector))
            && !(plain_visual && matches!(node.state.kind, FrameKind::Mesh | FrameKind::Projector))
        {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        if node.state.kind == FrameKind::Light && node.light.is_none() {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        let registrations = (node.state.flags & !handled)
            & (0x20000000
                | 0x8000
                | 0x400000
                | if node.state.kind == FrameKind::Dummy {
                    0x80000000
                } else {
                    0
                });
        if registrations != 0 {
            return Err(FrameError::UnsupportedDestructionRegistrations(
                key,
                registrations,
            ));
        }
        if node.state.kind == FrameKind::Target {
            if self.target_updates.is_flushing() || self.target_updates.pending().contains(&key) {
                return Err(FrameError::UnsupportedDestruction(key));
            }
            for &linked in self.target_state(key)?.frames() {
                self.index(linked)?;
            }
        }
        // Native removes only the first owner/occurrence. Never turn remaining
        // raw dangling pointers into apparently usable stable graph identities.
        let mut incoming = 0usize;
        for (index, target) in self.nodes.iter().enumerate() {
            if target.alive && target.state.kind == FrameKind::Target {
                let target_key = FrameKey((index + 1) as u32);
                if target_key != key {
                    if let Ok(state) = self.targets.state(target_key) {
                        incoming += state.frames().iter().filter(|&&frame| frame == key).count();
                    }
                }
            }
        }
        let own_target_clears_bit =
            node.state.kind == FrameKind::Target && self.target_state(key)?.frames().contains(&key);
        if incoming > 1
            || (incoming != 0 && (node.state.flags & 0x4000 == 0 || own_target_clears_bit))
        {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        if node.state.kind == FrameKind::Joint {
            let joint = node.joint.as_ref().unwrap();
            for (index, target) in self.nodes.iter().enumerate() {
                if let Some(binding) = &target.binding {
                    for (slot, &linked) in binding.joints.iter().enumerate() {
                        if linked == Some(key)
                            && (joint.singlemesh != Some(FrameKey((index + 1) as u32))
                                || joint.index as usize != slot)
                        {
                            return Err(FrameError::UnsupportedDestruction(key));
                        }
                    }
                }
            }
        }
        if let Some(joint) = node.joint {
            if let Some(target) = joint.singlemesh {
                let binding = self.singlemesh_binding(target)?;
                let slot = binding
                    .joints
                    .get(joint.index as usize)
                    .ok_or(FrameError::SinglemeshBindingUnavailable(target))?;
                // None is an unavailable allocation slot in this projection;
                // it cannot be mistaken for an observed native null pointer.
                let linked = slot.ok_or(FrameError::SinglemeshBindingUnavailable(target))?;
                self.joint_state(linked)?;
            }
        }
        Ok(())
    }
    /// Derived joint unlink precedes common parent/child detachment. Hierarchy
    /// links carry no references; detaching children never releases them and
    /// RemoveChild deliberately preserves their matrix caches.
    pub(crate) fn destroy_derived(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if let Some(joint) = self.node(key)?.joint {
            if let Some(target) = joint.singlemesh {
                let linked = self.singlemesh_binding(target)?.joints[joint.index as usize].unwrap();
                self.node_mut(linked)?.joint.as_mut().unwrap().singlemesh = None;
                self.node_mut(target)?.binding.as_mut().unwrap().joints[joint.index as usize] =
                    None;
            }
        }
        if self.state(key)?.kind == FrameKind::Target {
            let mut targets = std::mem::take(&mut self.targets);
            let result = targets.destroy_target(key, |linked| {
                self.node_mut(linked)
                    .expect("preflighted target link")
                    .state
                    .flags &= !0x4000;
            });
            self.targets = targets;
            result.map_err(|_| FrameError::TargetUnavailable(key))?;
        }
        Ok(())
    }
    /// The arena removes owner membership between derived and base effects.
    pub(crate) fn destroy_base(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.flags & 0x4000 != 0 {
            let mut flags = self.state(key)?.flags;
            self.targets.remove_from_any_target(key, &mut flags);
            self.node_mut(key)?.state.flags = flags;
        }
        if let Some(parent) = self.links(key)?.parent {
            self.remove_child(parent, key)?;
        }
        while let Some(child) = self.links(key)?.child {
            self.remove_child(key, child)?;
        }
        let node = self.node_mut(key)?;
        node.alive = false;
        node.joint = None;
        Ok(())
    }
    #[cfg(test)]
    fn destroy(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.check_destruction(key)?;
        self.destroy_derived(key)?;
        self.destroy_base(key)
    }
    pub(crate) fn camera_projection_changed(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind != FrameKind::Camera {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        let state = &mut self.node_mut(key)?.state;
        state.update_count = state.update_count.wrapping_add(1);
        Ok(())
    }
    pub(crate) fn change_flags(
        &mut self,
        key: FrameKey,
        clear: u32,
        set: u32,
    ) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.flags = (node.state.flags & !clear) | set;
        Ok(())
    }
    pub fn state(&self, key: FrameKey) -> Result<&I3dFrameState, FrameError> {
        Ok(&self.node(key)?.state)
    }
    /// Direct native field stores used by reconstructed callers. Unlike SetRot,
    /// SetScale or LinkTo, a store does not invoke additional invalidation. The
    /// caller must restrict writes to its evidenced offsets and preserve their
    /// order; hierarchy, derived bindings and lifetime remain graph-owned.
    pub(crate) fn begin_light_constructor(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind != FrameKind::Light || self.node(key)?.light.is_some() {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        self.node_mut(key)?.light = Some(crate::frame_light::I3dLightState::constructor_prefix());
        Ok(())
    }
    pub(crate) fn light_state(
        &self,
        key: FrameKey,
    ) -> Result<&crate::frame_light::I3dLightState, FrameError> {
        self.node(key)?
            .light
            .as_ref()
            .ok_or(FrameError::UnsupportedFrameUpdate(key))
    }
    pub(crate) fn light_state_mut(
        &mut self,
        key: FrameKey,
    ) -> Result<&mut crate::frame_light::I3dLightState, FrameError> {
        self.node_mut(key)?
            .light
            .as_mut()
            .ok_or(FrameError::UnsupportedFrameUpdate(key))
    }
    pub(crate) fn projected_state_mut(
        &mut self,
        key: FrameKey,
    ) -> Result<&mut I3dFrameState, FrameError> {
        Ok(&mut self.node_mut(key)?.state)
    }
    /// Common SetOn store, PC RVA0x1b400: only the low argument byte controls
    /// flag bit0. Matrix/volume cache flags and descendants are untouched.
    pub(crate) fn set_on(&mut self, key: FrameKey, on: u32) -> Result<(), FrameError> {
        let flags = &mut self.node_mut(key)?.state.flags;
        *flags = (*flags & !1) | u32::from(on as u8 != 0);
        Ok(())
    }
    /// GetFrameInterpolator RVA0x1ea60 marks a newly bound destination.
    pub(crate) fn mark_animated(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.node_mut(key)?.state.flags |= 0x10000000;
        Ok(())
    }
    /// Register the parsed Target links after all model identities exist.
    /// LoadImage sets membership bit 0x4000 for every nonzero reference, while
    /// preserving duplicates and registration order in its owned registry.
    pub fn register_target(
        &mut self,
        key: FrameKey,
        state: I3dTargetState,
    ) -> Result<(), FrameError> {
        if self.state(key)?.kind != FrameKind::Target {
            return Err(FrameError::NotTarget(key));
        }
        for &linked in state.frames() {
            self.index(linked)?;
        }
        self.target_updates
            .reserve_pending(self.nodes.len())
            .map_err(|_| FrameError::TargetUnavailable(key))?;
        // All keys were preflighted; successful insertion is followed only by
        // infallible stores, including repeated stores for duplicate references.
        self.targets
            .insert(key, state)
            .map_err(|_| FrameError::TargetUnavailable(key))?;
        for index in 0..self.targets.state(key).unwrap().frames().len() {
            let linked = self.targets.state(key).unwrap().frames()[index];
            self.node_mut(linked)?.state.flags |= 0x4000;
        }
        Ok(())
    }

    /// Replace the empty constructor registration after parsed references have
    /// resolved. Unlike AddToTarget, LoadImage preserves duplicate ownership.
    pub(crate) fn apply_target_load_image_state(
        &mut self,
        key: FrameKey,
        state: I3dTargetState,
    ) -> Result<(), FrameError> {
        if !self.target_state(key)?.frames().is_empty() {
            return Err(FrameError::TargetUnavailable(key));
        }
        for &linked in state.frames() {
            self.index(linked)?;
        }
        *self
            .targets
            .state_mut(key)
            .map_err(|_| FrameError::TargetUnavailable(key))? = state;
        for index in 0..self.target_state(key)?.frames().len() {
            let linked = self.target_state(key)?.frames()[index];
            self.node_mut(linked)?.state.flags |= 0x4000;
        }
        Ok(())
    }

    pub fn target_state(&self, key: FrameKey) -> Result<&I3dTargetState, FrameError> {
        self.index(key)?;
        self.targets
            .state(key)
            .map_err(|_| FrameError::TargetUnavailable(key))
    }

    pub fn joint_state(&self, key: FrameKey) -> Result<&I3dJointState, FrameError> {
        self.node(key)?
            .joint
            .as_ref()
            .ok_or(FrameError::NotJoint(key))
    }
    /// Import a known PC joint projection, including an available native cache.
    /// This host intake does not execute the original constructor or loader.
    pub fn insert_joint_state(
        &mut self,
        mut state: I3dFrameState,
        joint: I3dJointState,
    ) -> Result<FrameKey, FrameError> {
        if let Some(target) = joint.singlemesh {
            self.singlemesh_invalidation_state(target)?;
        }
        state.kind = FrameKind::Joint;
        let key = self.insert(state)?;
        self.node_mut(key)?.joint = Some(joint);
        Ok(key)
    }

    /// Original GetJointMatrix, PC RVA0x56ba0. The valid flag is set before
    /// UpdateMatrix, and each completed ancestor product replaces the cache.
    /// The first model/visual ancestor is excluded. A cached parent joint
    /// terminates the walk after one product. Unsupported arithmetic preserves
    /// preceding stores and marks the interrupted cache unavailable to callers.
    pub fn get_joint_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        if self.joint_state(key)?.flags & 0x10 != 0 {
            if let Some(error) = self.node(key)?.joint_error {
                return Err(error);
            }
            return self
                .joint_state(key)?
                .joint_matrix
                .ok_or(FrameError::JointMatrixUnavailable(key));
        }
        self.node_mut(key)?.joint.as_mut().unwrap().flags |= 0x10;
        let result = (|| {
            let local = self.local_matrix(key)?;
            self.node_mut(key)?.joint.as_mut().unwrap().joint_matrix = Some(local);
            let mut parent = self.links(key)?.parent;
            while let Some(at) = parent {
                let kind = self.state(at)?.kind;
                if matches!(
                    kind,
                    FrameKind::Model
                        | FrameKind::Mesh
                        | FrameKind::Projector
                        | FrameKind::Singlemesh
                ) {
                    break;
                }
                let cached = kind == FrameKind::Joint && self.joint_state(at)?.flags & 0x10 != 0;
                let right = if cached {
                    self.get_joint_matrix(at)?
                } else {
                    self.local_matrix(at)?
                };
                let product = self
                    .joint_state(key)?
                    .joint_matrix
                    .unwrap()
                    .try_mul_affine(&right)
                    .map_err(|_| FrameError::NumericDomain("GetJointMatrix multiply"))?;
                self.node_mut(key)?.joint.as_mut().unwrap().joint_matrix = Some(product);
                if cached {
                    break;
                }
                parent = self.links(at)?.parent;
            }
            Ok(self.joint_state(key)?.joint_matrix.unwrap())
        })();
        self.node_mut(key)?.joint_error = result.as_ref().err().copied();
        result
    }
    /// Complete derived LoadImage payload; the caller applies common fields
    /// first. Matrix/index bits are retained even outside numeric getter domains.
    pub fn apply_joint_load_image_fields(
        &mut self,
        key: FrameKey,
        bone_matrix: SMatrix,
        index: u32,
    ) -> Result<(), FrameError> {
        self.node_mut(key)?
            .joint
            .as_mut()
            .ok_or(FrameError::NotJoint(key))?
            .apply_load_image_fields(bone_matrix, index);
        self.node_mut(key)?.joint_error = None;
        Ok(())
    }
    /// Import the mesh-presence and invalidation fields needed by the binding
    /// prefix. This is not a native singlemesh constructor or geometry loader.
    pub fn insert_singlemesh_binding(
        &mut self,
        state: I3dFrameState,
        invalidation: SinglemeshInvalidationState,
        mesh_present: bool,
    ) -> Result<FrameKey, FrameError> {
        let key = self.insert_singlemesh_invalidation_target(state, invalidation)?;
        self.node_mut(key)?.binding = Some(I3dSinglemeshBinding {
            mesh_present,
            ..Default::default()
        });
        Ok(key)
    }
    pub fn singlemesh_binding(&self, key: FrameKey) -> Result<&I3dSinglemeshBinding, FrameError> {
        self.node(key)?
            .binding
            .as_ref()
            .ok_or(FrameError::SinglemeshBindingUnavailable(key))
    }
    /// Last successfully calculated bound, available only while the native
    /// bound-valid flag is set and no interrupted calculation is pending.
    pub fn singlemesh_bound_volume(
        &self,
        key: FrameKey,
    ) -> Result<SinglemeshBoundVolume, FrameError> {
        let node = self.node(key)?;
        if let Some(error) = node.singlemesh_bound_error {
            return Err(error);
        }
        if node.state.visual_flags & 0x20000 == 0 {
            return Err(FrameError::SinglemeshBoundsUnavailable(key));
        }
        self.singlemesh_binding(key)?
            .bound_volume
            .ok_or(FrameError::SinglemeshBoundsUnavailable(key))
    }
    /// DBuild for an explicitly supplied, loaded first-LOD skin definition.
    /// After binding, the original bounds traversal eagerly evaluates joint
    /// matrices before the final DBuild visual-flag store. Missing mesh/empty
    /// discovery retains the native false result. The caller owns the validated
    /// LOD/definition relationship; this does not load resources or supply the
    /// missing-LOD and missing-definition fallback backends.
    pub fn d_build_loaded_singlemesh(
        &mut self,
        key: FrameKey,
        initial_bbox: SinglemeshBoundingBox,
        joint_boxes: &[SinglemeshBoundingBox],
    ) -> Result<bool, FrameError> {
        match self.d_build_binding(key)? {
            DBuildBindingOutcome::NoMesh | DBuildBindingOutcome::NoJoints => return Ok(false),
            DBuildBindingOutcome::BoundVolumeUpdateRequired => {}
        }
        self.update_singlemesh_loaded_bounds(key, initial_bbox, joint_boxes)?;
        let state = &mut self.node_mut(key)?.state;
        if state.visual_flags & 0x40 == 0 {
            state.visual_flags &= !0x40000;
        }
        Ok(true)
    }
    /// Populated-definition UpdateBVolumeProc path (PC RVA0x54810), after a
    /// successful binding allocation. The counter store precedes arithmetic
    /// and joint-cache work. On unsupported numeric input, completed joint
    /// cache writes remain, but no native partial volume is claimed or exposed.
    pub fn update_singlemesh_loaded_bounds(
        &mut self,
        key: FrameKey,
        initial_bbox: SinglemeshBoundingBox,
        joint_boxes: &[SinglemeshBoundingBox],
    ) -> Result<(), FrameError> {
        self.singlemesh_binding(key)?;
        let changed = self.singlemesh_invalidation_state(key)?.joints_changed;
        self.node_mut(key)?
            .binding
            .as_mut()
            .unwrap()
            .bvol_joints_changed = changed;
        let result = (|| {
            let binding = self.singlemesh_binding(key)?;
            if !binding.mesh_present || binding.joints.is_empty() {
                return Err(FrameError::SinglemeshBoundsUnavailable(key));
            }
            let count = binding.joints.len();
            if joint_boxes.len() < count {
                return Err(FrameError::SinglemeshBoundsDefinitionTooShort(key));
            }
            // Unwritten PC allocation slots are never interpreted as null.
            if binding.joints.iter().any(Option::is_none) {
                return Err(FrameError::SinglemeshBindingUnavailable(key));
            }
            let boxes: Vec<_> = joint_boxes[..count].iter().copied().map(Some).collect();
            let volume = calculate_singlemesh_bounds(initial_bbox, &boxes, |index| {
                let joint = self.singlemesh_binding(key)?.joints[index].unwrap();
                self.get_joint_matrix(joint)
            })
            .map_err(|error| match error {
                SinglemeshBoundsError::JointMatrix(error) => error,
                SinglemeshBoundsError::NumericDomain => {
                    FrameError::NumericDomain("singlemesh bound volume")
                }
            })?;
            let node = self.node_mut(key)?;
            node.binding.as_mut().unwrap().bound_volume = Some(volume);
            node.state.visual_flags = (node.state.visual_flags & !0x40000) | 0x20000;
            self.discard_hr_bound_vol(key)
        })();
        self.node_mut(key)?.singlemesh_bound_error = result.as_ref().err().copied();
        result
    }
    /// Singlemesh bounds virtual wrapper, PC RVA0x547e0 (singlemorph RVA0x57580
    /// tail-jumps here). A matching Joint-change counter only sets the valid
    /// bit. Otherwise the populated-definition calculation stores the counter
    /// before its graph/arithmetic work. An interrupted host calculation remains
    /// unavailable; its equal counter must not publish an incomplete volume.
    pub fn update_singlemesh_loaded_bounds_cached(
        &mut self,
        key: FrameKey,
        initial_bbox: SinglemeshBoundingBox,
        joint_boxes: &[SinglemeshBoundingBox],
    ) -> Result<(), FrameError> {
        let node = self.node(key)?;
        if let Some(error) = node.singlemesh_bound_error {
            return Err(error);
        }
        let stored = self.singlemesh_binding(key)?.bvol_joints_changed;
        let changed = self.singlemesh_invalidation_state(key)?.joints_changed;
        if stored != changed {
            self.update_singlemesh_loaded_bounds(key, initial_bbox, joint_boxes)
        } else {
            self.node_mut(key)?.state.visual_flags |= 0x20000;
            Ok(())
        }
    }
    /// Original PC findJoint, RVA0x54d80: descendants in sibling-head preorder,
    /// with at most 64 joints. Nested visual nodes do not stop the PC search.
    /// The receiver itself is excluded. Owned links are cycle-free.
    pub fn find_joint(&self, key: FrameKey) -> Result<Vec<FrameKey>, FrameError> {
        let mut found = Vec::new();
        let mut at = self.links(key)?.child;
        while let Some(current) = at {
            if self.state(current)?.kind == FrameKind::Joint {
                if found.len() == 64 {
                    // Native breaks this sibling level; unwind to its parent.
                    at = self.find_joint_next(key, current, false)?;
                    continue;
                }
                found.push(current);
            }
            at = match self.links(current)?.child {
                Some(child) => Some(child),
                None => self.find_joint_next(key, current, true)?,
            };
        }
        Ok(found)
    }
    fn find_joint_next(
        &self,
        root: FrameKey,
        mut current: FrameKey,
        include_sibling: bool,
    ) -> Result<Option<FrameKey>, FrameError> {
        if include_sibling {
            if let Some(next) = self.links(current)?.next {
                return Ok(Some(next));
            }
        }
        while let Some(parent) = self.links(current)?.parent {
            if parent == root {
                break;
            }
            if let Some(next) = self.links(parent)?.next {
                return Ok(Some(next));
            }
            current = parent;
        }
        Ok(None)
    }
    /// DBuild's binding prefix, PC RVA0x54c20. Old backreferences are cleared
    /// before checking mesh presence. Each dense slot takes the first matching
    /// index in findJoint order. Unwritten native allocation slots remain host
    /// unavailable markers. Bounds and the subsequent visual-flag store are
    /// deliberately pending; BoundVolumeUpdateRequired is not a true return.
    pub fn d_build_binding(&mut self, key: FrameKey) -> Result<DBuildBindingOutcome, FrameError> {
        self.singlemesh_binding(key)?;
        // An unwritten slot is not safe to dereference on the next native call.
        if self
            .singlemesh_binding(key)?
            .joints
            .iter()
            .any(Option::is_none)
        {
            return Err(FrameError::SinglemeshBindingUnavailable(key));
        }
        let old = std::mem::take(&mut self.node_mut(key)?.binding.as_mut().unwrap().joints);
        for joint in old.into_iter().flatten() {
            self.node_mut(joint)?.joint.as_mut().unwrap().singlemesh = None;
        }
        if !self.singlemesh_binding(key)?.mesh_present {
            return Ok(DBuildBindingOutcome::NoMesh);
        }
        let found = self.find_joint(key)?;
        if found.is_empty() {
            return Ok(DBuildBindingOutcome::NoJoints);
        }
        let mut slots = vec![None; found.len()];
        for (index, slot) in slots.iter_mut().enumerate() {
            for &joint in &found {
                if self.joint_state(joint)?.index == index as u32 {
                    *slot = Some(joint);
                    self.node_mut(joint)?.joint.as_mut().unwrap().singlemesh = Some(key);
                    break;
                }
            }
        }
        self.node_mut(key)?.binding.as_mut().unwrap().joints = slots;
        Ok(DBuildBindingOutcome::BoundVolumeUpdateRequired)
    }
    /// Import only a known singlemesh frame/counter projection for linked
    /// Joint::UpdateV. This does not construct or load a native singlemesh.
    pub fn insert_singlemesh_invalidation_target(
        &mut self,
        mut state: I3dFrameState,
        invalidation: SinglemeshInvalidationState,
    ) -> Result<FrameKey, FrameError> {
        state.kind = FrameKind::Singlemesh;
        let key = self.insert(state)?;
        self.node_mut(key)?.singlemesh = Some(invalidation);
        Ok(key)
    }
    pub fn singlemesh_invalidation_state(
        &self,
        key: FrameKey,
    ) -> Result<&SinglemeshInvalidationState, FrameError> {
        self.node(key)?
            .singlemesh
            .as_ref()
            .ok_or(FrameError::InvalidSinglemeshTarget(key))
    }
    /// Assign the observed pointer field using an owned target. This low-level
    /// projection does not populate the target's DBuild joint-index array.
    pub fn assign_joint_singlemesh_target(
        &mut self,
        key: FrameKey,
        target: Option<FrameKey>,
    ) -> Result<(), FrameError> {
        self.joint_state(key)?;
        if let Some(target) = target {
            self.singlemesh_invalidation_state(target)?;
        }
        self.node_mut(key)?.joint.as_mut().unwrap().singlemesh = target;
        Ok(())
    }
    pub fn links(&self, key: FrameKey) -> Result<FrameLinks, FrameError> {
        Ok(self.node(key)?.links)
    }
    pub fn storage_bytes(&self) -> Result<usize, FrameError> {
        let binding_bytes = self
            .nodes
            .iter()
            .try_fold(0usize, |total, node| {
                let bytes = match &node.binding {
                    Some(binding) => binding
                        .joints
                        .capacity()
                        .checked_mul(std::mem::size_of::<Option<FrameKey>>())?,
                    None => 0,
                };
                total.checked_add(bytes)
            })
            .ok_or(FrameError::AllocationOverflow)?;
        self.nodes
            .capacity()
            .checked_mul(std::mem::size_of::<Node>())
            .and_then(|n| {
                n.checked_add(
                    self.world_path
                        .capacity()
                        .checked_mul(std::mem::size_of::<FrameKey>())?,
                )
            })
            .and_then(|n| n.checked_add(binding_bytes))
            .and_then(|n| n.checked_add(self.targets.storage_bytes().ok()?))
            .and_then(|n| n.checked_add(self.target_updates.storage_bytes().ok()?))
            .and_then(|n| {
                n.checked_add(
                    self.visits
                        .capacity()
                        .checked_mul(std::mem::size_of::<Visit>())?,
                )
            })
            .ok_or(FrameError::AllocationOverflow)
    }
    /// Common field projection of FrameLoadImage, RVA0x1d1c0, after a
    /// complete record has been validated. The loader first constructs and
    /// links the frame, and separately owns raw names/properties. These stores
    /// ignore position lock and preserve quaternion bits without normalization.
    /// Short reads, name-cache effects and original allocation are outside scope.
    pub fn apply_common_load_image_fields(
        &mut self,
        key: FrameKey,
        position: [f32; 3],
        scale: [f32; 3],
        rotation: SQuat,
        raw_logic_flags: u8,
    ) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.local.elements[12..15].copy_from_slice(&position);
        node.state.scale = scale;
        node.state.rotation = rotation;
        node.state.flags = (node.state.flags & 0xffffff00) | u32::from(raw_logic_flags);
        node.state.flags = (node.state.flags & 0xfffffe8b) | 0x40000008;
        // The native mask invalidates both caches. Subsequent getters recheck
        // the complete new SRT rather than retaining a previous host interruption.
        node.local_error = None;
        node.world_error = None;
        Ok(())
    }
    /// Observed LoadHierarchy EndLoad root store, followed by original Update.
    /// Resource loading remains outside this field projection.
    pub fn mark_loaded_root(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.mark_loaded_root_fields(key)?;
        self.update(key)
    }
    pub(crate) fn mark_loaded_root_fields(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let state = &mut self.node_mut(key)?.state;
        state.flags = (state.flags & 0xfffffeff) | 0x40000000;
        Ok(())
    }
    /// Original bound-cache invalidation at RVA0x1cc00, used by model Close.
    /// Sector type5 stops both direct receiver and ancestor invalidation before
    /// any of its flags are cleared.
    pub fn discard_hr_bound_vol(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind == FrameKind::Sector {
            return Ok(());
        }
        self.node_mut(key)?.state.flags &= 0xfffffebf;
        let mut parent = self.links(key)?.parent;
        while let Some(key) = parent {
            let node = self.node_mut(key)?;
            if node.state.flags & 0x40 == 0 || node.state.kind == FrameKind::Sector {
                break;
            }
            node.state.flags &= 0xfffffebf;
            parent = node.links.parent;
        }
        Ok(())
    }
    /// Copies even equal values, preserving signed zero. Flag0x80 locks position.
    pub fn assign_position(&mut self, key: FrameKey, p: [f32; 3]) -> Result<(), FrameError> {
        let s = &mut self.node_mut(key)?.state;
        if s.flags & 0x80 == 0 {
            s.local.elements[12..15].copy_from_slice(&p);
            s.flags = (s.flags & 0xfffffedf) | 0x40000000;
        }
        Ok(())
    }
    /// Scale stores precede the guarded matrix-to-quaternion dependency.
    pub fn assign_scale(&mut self, key: FrameKey, scale: [f32; 3]) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.scale = scale;
        if node.state.flags & 8 == 0 {
            match SQuat::try_make_from_matrix(&node.state.local) {
                Ok(rotation) => node.state.rotation = rotation,
                Err(_) => {
                    // Preserve native stores/flags preceding the unsupported
                    // dependency, while keeping interrupted caches unavailable.
                    let error = FrameError::QuaternionFromMatrixUnavailable;
                    node.local_error = Some(error);
                    node.world_error = Some(error);
                    return Err(error);
                }
            }
        }
        node.state.flags = (node.state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }
    /// Observed animation assignment: copy, original Normalize, then invalidate.
    /// Availability is checked before Normalize; a later matrix operation must
    /// recheck its resulting quaternion, including tiny normalized components.
    pub fn assign_rotation(
        &mut self,
        key: FrameKey,
        mut rotation: SQuat,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        if !rotation.is_verified_input() {
            return Err(FrameError::NumericDomain("Normalize"));
        }
        rotation.normalize();
        let state = &mut self.node_mut(key)?.state;
        state.rotation = rotation;
        state.flags = (state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }
    /// Assign a quaternion already normalized by the original track evaluator.
    /// Preserve its exact bits; running Normalize again would change samples.
    /// The input has the same numeric availability guard as assign_rotation.
    pub fn apply_evaluated_rotation(
        &mut self,
        key: FrameKey,
        rotation: SQuat,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        if !rotation.is_verified_input() {
            return Err(FrameError::NumericDomain("evaluated rotation"));
        }
        let state = &mut self.node_mut(key)?.state;
        state.rotation = rotation;
        state.flags = (state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }

    /// Original SetDirWorld (RVA 0x1b040), for a frame with a parent and a
    /// roll whose absolute value is at most the native double 1e-8 threshold.
    /// The parent cache is requested before inverse/direction arithmetic. A
    /// singular parent uses the original inverse identity fallback. SetDir3
    /// retains its native partial writes on an unavailable numeric dependency;
    /// host error markers prevent those caches from appearing complete later.
    /// Parentless SetDir and roll rotations above that threshold remain unavailable.
    pub fn set_direction_world(
        &mut self,
        key: FrameKey,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        let result = (|| {
            let parent = self
                .links(key)?
                .parent
                .ok_or(FrameError::TargetDirectionUnavailable(key))?;
            let parent_world = self.world_matrix(parent)?;
            let (inverse, _) = parent_world
                .inverse()
                .map_err(|_| FrameError::NumericDomain("SetDirWorld inverse"))?;
            let direction_matrix = crate::target_direction::try_target_direction_matrix(direction)
                .map_err(|_| FrameError::NumericDomain("SetDirWorld direction"))?;
            if !roll.is_finite() || f64::from(roll.abs()) > f64::from(1e-8_f32) {
                return Err(FrameError::TargetDirectionUnavailable(key));
            }
            let relative = direction_matrix
                .try_mul_affine(&inverse)
                .map_err(|_| FrameError::NumericDomain("SetDirWorld multiply"))?;
            self.node_mut(key)?
                .state
                .local
                .try_set_dir3(
                    relative.elements[8..11].try_into().unwrap(),
                    relative.elements[4..7].try_into().unwrap(),
                )
                .map_err(|_| FrameError::DirectionFromVectorsUnavailable)?;
            let state = &mut self.node_mut(key)?.state;
            state.flags = (state.flags & 0xfffffed3) | 0x40000010;
            Ok(())
        })();
        let node = self.node_mut(key)?;
        node.local_error = result.as_ref().err().copied();
        node.world_error = result.as_ref().err().copied();
        result
    }

    /// RemoveChild preserves the detached child's world cache, even if now stale.
    pub fn remove_child(&mut self, parent: FrameKey, child: FrameKey) -> Result<(), FrameError> {
        self.index(parent)?;
        let l = self.links(child)?;
        if l.parent != Some(parent) {
            return Ok(());
        }
        let p = self.node_mut(parent)?;
        p.links.child_count = p.links.child_count.wrapping_sub(1);
        if let Some(prev) = l.previous {
            self.node_mut(prev)?.links.next = l.next;
            if let Some(next) = l.next {
                self.node_mut(next)?.links.previous = Some(prev);
            }
        } else if let Some(next) = l.next {
            self.node_mut(parent)?.links.child = Some(next);
            self.node_mut(next)?.links.previous = None;
        } else {
            let p = self.node_mut(parent)?;
            p.links.child = None;
            p.links.child_count = 0;
        }
        let c = self.node_mut(child)?;
        c.links.parent = None;
        c.links.previous = None;
        c.links.next = None;
        self.node_mut(parent)?.state.flags &= 0xfffffebf;
        Ok(())
    }
    /// Native LinkTo flags0: prepend, preserve local pose, invalidate this world
    /// cache. Descendant caches stay unchanged until Update. Cycles are rejected.
    pub fn link_to(&mut self, child: FrameKey, parent: Option<FrameKey>) -> Result<(), FrameError> {
        self.link_to_with_flags(child, parent, 0).map(|_| ())
    }
    /// Original LinkTo, RVA0x1b9d0, for flags0 and world-preserving flags1.
    /// The native return value is zero, including the unchanged-parent path.
    /// Flags1 computes the old world cache before unlinking, then derives the
    /// new local position, scale and direction in native write order. Availability
    /// failures retain preceding link/cache/pose writes and poison interrupted
    /// caches. Unsupported flag2/4 ownership/callback paths are explicit errors.
    pub fn link_to_with_flags(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, FrameError> {
        self.link_to_projection(child, parent, flags, false, |graph, _, old, new| {
            if graph.closest_sector(old)? == graph.closest_sector(new)? {
                Ok(())
            } else {
                Err(FrameError::UnsupportedLinkState)
            }
        })
    }
    pub(crate) fn link_to_with_scene_context(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
        transition: impl FnMut(
            &mut Self,
            FrameKey,
            Option<FrameKey>,
            Option<FrameKey>,
        ) -> Result<(), FrameError>,
    ) -> Result<u32, FrameError> {
        self.link_to_projection(child, parent, flags, true, transition)
    }
    fn link_to_projection(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
        allow_hr: bool,
        mut transition: impl FnMut(
            &mut Self,
            FrameKey,
            Option<FrameKey>,
            Option<FrameKey>,
        ) -> Result<(), FrameError>,
    ) -> Result<u32, FrameError> {
        if flags > 1 {
            return Err(FrameError::UnsupportedLinkFlags(flags));
        }
        let result = self.link_to_inner(child, parent, flags, allow_hr, &mut transition);
        if let Err(
            error @ (FrameError::NumericDomain(_)
            | FrameError::QuaternionFromMatrixUnavailable
            | FrameError::DirectionFromVectorsUnavailable),
        ) = result
        {
            let node = self.node_mut(child)?;
            node.local_error = Some(error);
            node.world_error = Some(error);
        }
        result
    }
    pub(crate) fn closest_sector(
        &self,
        mut frame: Option<FrameKey>,
    ) -> Result<Option<FrameKey>, FrameError> {
        let mut remaining = self.nodes.len();
        while let Some(key) = frame {
            if remaining == 0 {
                return Err(FrameError::Cycle);
            }
            remaining -= 1;
            if self.state(key)?.kind == FrameKind::Sector {
                return Ok(Some(key));
            }
            frame = self.links(key)?.parent;
        }
        Ok(None)
    }
    fn link_to_inner(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
        allow_hr: bool,
        transition: &mut impl FnMut(
            &mut Self,
            FrameKey,
            Option<FrameKey>,
            Option<FrameKey>,
        ) -> Result<(), FrameError>,
    ) -> Result<u32, FrameError> {
        let old = self.links(child)?.parent;
        if old == parent {
            return Ok(0);
        }
        if !allow_hr && flags == 1 && self.state(child)?.flags & 0x1c0000 != 0 {
            return Err(FrameError::UnsupportedLinkState);
        }
        let mut at = parent;
        let mut count = 0;
        while let Some(k) = at {
            if k == child {
                return Err(FrameError::Cycle);
            }
            at = self.links(k)?.parent;
            count += 1;
            if count > self.nodes.len() {
                return Err(FrameError::Cycle);
            }
        }
        if flags & 1 != 0 {
            self.world_matrix(child)?;
        }
        if let Some(old) = old {
            self.remove_child(old, child)?;
        }
        if let Some(p) = parent {
            let first = self.links(p)?.child;
            if let Some(first) = first {
                self.node_mut(first)?.links.previous = Some(child);
            }
            let c = self.node_mut(child)?;
            c.links.parent = Some(p);
            c.links.previous = None;
            c.links.next = first;
            let p = self.node_mut(p)?;
            p.links.child = Some(child);
            p.links.child_count = p.links.child_count.wrapping_add(1);
            p.state.flags &= 0xfffffebf;
        }
        if flags & 1 == 0 {
            let s = &mut self.node_mut(child)?.state;
            s.flags = (s.flags & !0x20) | 0x40000000;
        } else {
            let relative = if let Some(p) = parent {
                let parent_world = self.world_matrix(p)?;
                // Native discards Inverse's false flag and uses its identity
                // fallback for a singular parent.
                let (inverse, _) = parent_world
                    .inverse()
                    .map_err(|_| FrameError::NumericDomain("LinkTo inverse"))?;
                let child_world = self.world_matrix(child)?;
                if !child_world.is_verified_input() || !inverse.is_verified_input() {
                    return Err(FrameError::NumericDomain("LinkTo multiply"));
                }
                let relative = child_world.mul_affine(&inverse);
                if !relative.is_verified_input() {
                    return Err(FrameError::NumericDomain("LinkTo multiply"));
                }
                relative
            } else {
                self.world_matrix(child)?
            };
            self.assign_position(child, relative.elements[12..15].try_into().unwrap())?;
            let scale = relative
                .try_get_scale()
                .map_err(|_| FrameError::NumericDomain("LinkTo GetScale"))?;
            // This setter intentionally derives any missing quaternion from the
            // previous local matrix, before SetDir3 replaces the basis.
            self.assign_scale(child, scale)?;
            self.node_mut(child)?
                .state
                .local
                .try_set_dir3(
                    relative.elements[8..11].try_into().unwrap(),
                    relative.elements[4..7].try_into().unwrap(),
                )
                .map_err(|_| FrameError::DirectionFromVectorsUnavailable)?;
            let node = self.node_mut(child)?;
            node.state.flags = (node.state.flags & 0xfffffed3) | 0x40000010;
            node.local_error = None;
            node.world_error = None;
        }
        let hr = self.state(child)?.flags & 0x1c0000;
        if hr != 0 {
            let mut at = parent;
            while let Some(frame) = at {
                self.node_mut(frame)?.state.flags |= hr;
                at = self.links(frame)?.parent;
            }
            let mut at = old;
            while let Some(frame) = at {
                let mask = match self.state(frame)?.kind {
                    FrameKind::Sector => 0xffebffff,
                    FrameKind::Occluder => 0xfff3ffff,
                    _ => 0xffe3ffff,
                };
                self.node_mut(frame)?.state.flags &= mask;
                // Actual intersection of three different HR masks is zero.
                // The native loop walks siblings but never loads child flags.
                let mut child = self.links(frame)?.child;
                let mut remaining = self.nodes.len();
                while let Some(key) = child {
                    if remaining == 0 {
                        return Err(FrameError::Cycle);
                    }
                    remaining -= 1;
                    child = self.links(key)?.next;
                }
                at = self.links(frame)?.parent;
            }
            transition(self, child, old, parent)?;
        }
        Ok(0)
    }
    /// Original PC0x1a6e0. Preserve flag writes before an unavailable dependency.
    pub fn update_matrix(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let result = self.update_matrix_inner(key);
        if let Ok(node) = self.node_mut(key) {
            node.local_error = result.as_ref().err().copied();
        }
        result
    }
    fn update_matrix_inner(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let flags = self.state(key)?.flags;
        if flags & 0x10 == 0 {
            self.node_mut(key)?.state.flags |= 0x14;
            let s = *self.state(key)?;
            self.node_mut(key)?
                .state
                .local
                .try_set_rot3(s.rotation)
                .map_err(|_| FrameError::NumericDomain("SetRot3"))?;
        } else if flags & 4 == 0 {
            if flags & 8 == 0 {
                let rotation = SQuat::try_make_from_matrix(&self.state(key)?.local)
                    .map_err(|_| FrameError::QuaternionFromMatrixUnavailable)?;
                self.node_mut(key)?.state.rotation = rotation;
            }
            self.node_mut(key)?.state.flags |= 0xc;
        } else {
            if let Some(error) = self.node(key)?.local_error {
                return Err(error);
            }
            return Ok(());
        }
        for row in 0..3 {
            for col in 0..3 {
                let i = row * 4 + col;
                let s = self.state(key)?;
                self.node_mut(key)?.state.local.elements[i] =
                    checked_f32_mul(s.local.elements[i], s.scale[row])
                        .map_err(|_| FrameError::NumericDomain("local row scaling"))?;
            }
        }
        Ok(())
    }
    pub fn local_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        self.update_matrix(key)?;
        Ok(self.state(key)?.local)
    }
    /// Observed inline guard. A valid cache skips the exported procedure entirely.
    pub fn world_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        if self.state(key)?.flags & 0x20 == 0 {
            self.update_w_matrix_proc(key)?;
        } else if let Some(error) = self.node(key)?.world_error {
            return Err(error);
        }
        Ok(self.state(key)?.world)
    }
    /// Original export0x1c7d0. An explicit stack preserves child-local-before-parent
    /// ordering without a host recursion limit or an invented native update budget.
    pub fn update_w_matrix_proc(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let mut path = std::mem::take(&mut self.world_path);
        path.clear();
        let result = (|| {
            let mut at = key;
            loop {
                self.update_matrix(at)?;
                path.push(at);
                match self.links(at)?.parent {
                    Some(p) if self.state(p)?.flags & 0x20 == 0 => at = p,
                    _ => break,
                }
            }
            for &k in path.iter().rev() {
                let result = self.assemble_world(k);
                self.node_mut(k)?.world_error = result.as_ref().err().copied();
                result?;
            }
            Ok(())
        })();
        self.world_path = path;
        if let Ok(node) = self.node_mut(key) {
            node.world_error = result.as_ref().err().copied();
        }
        result
    }
    fn assemble_world(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let s = *self.state(key)?;
        let parent = self.links(key)?.parent;
        let world = if let Some(p) = parent {
            if let Some(error) = self.node(p)?.world_error {
                return Err(error);
            }
            let right = self.state(p)?.world;
            s.local
                .try_mul_affine(&right)
                .map_err(|_| FrameError::NumericDomain("matrix multiply"))?
        } else {
            s.local
        };
        let unavailable = |_| FrameError::NumericDomain("world scale");
        let zy = checked_f32_add(s.scale[2], s.scale[1]).map_err(unavailable)?;
        // The parent branch adds X before the matrix copy; the root copies first.
        let total = if parent.is_some() {
            Some(checked_f32_add(zy, s.scale[0]).map_err(unavailable)?)
        } else {
            None
        };
        self.node_mut(key)?.state.world = world;
        let mut scale = match total {
            Some(v) => v,
            None => checked_f32_add(zy, s.scale[0]).map_err(unavailable)?,
        };
        if let Some(p) = parent {
            scale = checked_f32_mul(scale, self.state(p)?.world_scale).map_err(unavailable)?;
        }
        scale = checked_f32_mul(scale, f32::from_bits(0x3eaaaaab)).map_err(unavailable)?;
        let s = &mut self.node_mut(key)?.state;
        s.world_scale = scale;
        s.flags |= 0x20;
        Ok(())
    }
    fn enter_update(&mut self, key: FrameKey) -> Result<Visit, FrameError> {
        let n = self.node_mut(key)?;
        if n.state.kind == FrameKind::Scene {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        // Only the owned binding projection admits the evidenced inherited
        // visual update. The legacy target-only projection remains guarded.
        if n.state.kind == FrameKind::Singlemesh && n.binding.is_none() {
            return Err(FrameError::UnsupportedSinglemeshUpdate(key));
        }
        let dirty = n.state.flags & DIRTY != 0;
        if dirty {
            n.state.update_count = n.state.update_count.wrapping_add(1);
            if matches!(
                n.state.kind,
                FrameKind::Mesh | FrameKind::Projector | FrameKind::Singlemesh
            ) && n.state.visual_flags & 0x40 == 0
            {
                n.state.visual_flags &= !0x40000;
            }
        }
        Ok(Visit {
            key,
            child: n.links.child,
            dirty,
        })
    }
    /// Observed UpdateV effects for the represented frame kinds. Dummy's
    /// original entry is RVA0x1a330; it clears its own bound flag only if dirty.
    pub fn update_v(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if matches!(
            self.state(key)?.kind,
            FrameKind::Scene | FrameKind::Sector | FrameKind::Camera | FrameKind::Occluder
        ) {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        if let Some(error) = self.deferred_error {
            return Err(error);
        }
        if self.state(key)?.kind == FrameKind::Light {
            return self.update_light_v(key);
        }
        if self.state(key)?.kind == FrameKind::Target {
            let flags = self.state(key)?.flags;
            let result = self
                .target_updates
                .update_v(key, flags)
                .map_err(|_| FrameError::TargetUnavailable(key));
            if let Err(error) = result {
                self.deferred_error = Some(error);
            }
            return result;
        }
        if self.state(key)?.kind == FrameKind::Singlemesh && self.node(key)?.binding.is_none() {
            return Err(FrameError::UnsupportedSinglemeshUpdate(key));
        }
        if self.state(key)?.kind == FrameKind::Joint {
            let node = self.node_mut(key)?;
            if node.state.flags & DIRTY != 0 {
                node.joint_error = None;
            }
            let target = node
                .joint
                .as_mut()
                .unwrap()
                .invalidate_for_update(node.state.flags);
            if let Some(target) = target {
                let target_node = self.node_mut(target)?;
                let singlemesh = target_node
                    .singlemesh
                    .as_mut()
                    .ok_or(FrameError::InvalidSinglemeshTarget(target))?;
                singlemesh.joints_changed = singlemesh.joints_changed.wrapping_add(1);
                target_node.state.visual_flags &= !0x60000;
                self.discard_hr_bound_vol(target)?;
                let flags = &mut self.node_mut(target)?.state.flags;
                *flags = (*flags & 0xfffffebf) | 0x40000000;
            }
            return Ok(());
        }
        let s = &mut self.node_mut(key)?.state;
        if s.flags & DIRTY != 0 {
            if matches!(
                s.kind,
                FrameKind::Mesh | FrameKind::Projector | FrameKind::Singlemesh
            ) && s.visual_flags & 0x40 == 0
            {
                s.visual_flags &= !0x1c0000;
                if s.kind == FrameKind::Projector {
                    s.projector_cache_258 = 0;
                }
            } else if s.kind == FrameKind::Dummy {
                s.dummy_flags &= !0x10;
            }
        }
        Ok(())
    }
    /// Original Update/recursive-child effects for model/standard-mesh/dummy/joint
    /// objects and owned singlemesh bindings, followed by deferred Target updates;
    /// target-only singlemesh projections cannot enter their own update.
    pub fn update(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if matches!(
            self.state(key)?.kind,
            FrameKind::Camera | FrameKind::Sector | FrameKind::Occluder
        ) {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        self.update_with_camera(key, |_, key| Err(FrameError::UnsupportedFrameUpdate(key)))
    }
    pub(crate) fn update_with_camera(
        &mut self,
        key: FrameKey,
        mut camera_update: impl FnMut(&mut Self, FrameKey) -> Result<(), FrameError>,
    ) -> Result<(), FrameError> {
        if let Some(error) = self.deferred_error {
            return Err(error);
        }
        let mut visits = std::mem::take(&mut self.visits);
        visits.clear();
        let result = (|| {
            visits.push(self.enter_update(key)?);
            while let Some(v) = visits.last().copied() {
                if let Some(child) = v.child {
                    if v.dirty {
                        let s = &mut self.node_mut(child)?.state;
                        s.flags = (s.flags & 0xfffffedf) | 0x2000;
                    }
                    visits.push(self.enter_update(child)?);
                } else {
                    let done = visits.pop().unwrap();
                    if let Some(p) = visits.last_mut() {
                        if matches!(
                            self.state(done.key)?.kind,
                            FrameKind::Camera | FrameKind::Sector | FrameKind::Occluder
                        ) {
                            camera_update(self, done.key)?;
                        } else {
                            self.update_v(done.key)?;
                        }
                        if self.state(done.key)?.flags & DIRTY != 0 {
                            self.node_mut(done.key)?.state.flags &= !DIRTY;
                            let s = &mut self.node_mut(p.key)?.state;
                            s.flags = (s.flags & 0xfffffebf) | 0x2000;
                        }
                        p.child = self.links(done.key)?.next;
                    }
                }
            }
            if matches!(
                self.state(key)?.kind,
                FrameKind::Camera | FrameKind::Sector | FrameKind::Occluder
            ) {
                camera_update(self, key)?;
            } else {
                self.update_v(key)?;
            }
            if self.state(key)?.kind != FrameKind::Sector {
                self.node_mut(key)?.state.flags &= 0xfffffebf;
                let mut parent = self.links(key)?.parent;
                while let Some(p) = parent {
                    if self.state(p)?.flags & 0x40 == 0 || self.state(p)?.kind == FrameKind::Sector
                    {
                        break;
                    }
                    self.node_mut(p)?.state.flags &= 0xfffffebf;
                    parent = self.links(p)?.parent;
                }
            }
            let s = &mut self.node_mut(key)?.state;
            if matches!(
                s.kind,
                FrameKind::Mesh | FrameKind::Projector | FrameKind::Singlemesh
            ) && s.flags & DIRTY != 0
                && s.visual_flags & 0x40 == 0
            {
                s.visual_flags &= !0x40000;
            }
            s.flags &= !DIRTY;
            self.flush_target_updates()?;
            Ok(())
        })();
        self.visits = visits;
        result
    }
    fn flush_target_updates(&mut self) -> Result<(), FrameError> {
        let Some(count) = self.target_updates.begin_flush() else {
            return Ok(());
        };
        // Moving the registry avoids cloning linked-frame allocations. Nested
        // Update calls observe the queue's reentrancy guard and skip this flush.
        let targets = std::mem::take(&mut self.targets);
        let result = (|| {
            for index in 0..count {
                let key = self.target_updates.pending()[index];
                let target = targets
                    .state(key)
                    .map_err(|_| FrameError::TargetUnavailable(key))?;
                target
                    .update_frames_with_backend(key, self)
                    .map_err(|error| match error {
                        TargetUpdateError::World(error) => error,
                        TargetUpdateError::UnsupportedTargetFlags(flags) => {
                            FrameError::UnsupportedTargetFlags(key, flags)
                        }
                        TargetUpdateError::NumericDomain => {
                            FrameError::NumericDomain("Target UpdateFrames")
                        }
                    })?;
            }
            Ok(())
        })();
        self.targets = targets;
        match result {
            Ok(()) => {
                self.target_updates.finish_flush();
                Ok(())
            }
            Err(error) => {
                self.deferred_error = Some(error);
                Err(error)
            }
        }
    }
}

impl TargetFrameBackend for I3dFrameGraph {
    type Error = FrameError;
    fn world_matrix(&mut self, frame: FrameKey) -> Result<SMatrix, Self::Error> {
        I3dFrameGraph::world_matrix(self, frame)
    }
    fn set_direction_world(
        &mut self,
        frame: FrameKey,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), Self::Error> {
        I3dFrameGraph::set_direction_world(self, frame, direction, roll)
    }
    fn update_frame(&mut self, frame: FrameKey) -> Result<(), Self::Error> {
        self.update(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joint_destruction_clears_the_indexed_slot_joint_and_preserves_other_slots() {
        let mut graph = I3dFrameGraph::default();
        let mesh = graph
            .insert_singlemesh_binding(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 7 },
                true,
            )
            .unwrap();
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        let other = graph.insert(I3dFrameState::joint()).unwrap();
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().index = 1;
        graph
            .assign_joint_singlemesh_target(joint, Some(mesh))
            .unwrap();
        graph
            .assign_joint_singlemesh_target(other, Some(mesh))
            .unwrap();
        graph
            .node_mut(mesh)
            .unwrap()
            .binding
            .as_mut()
            .unwrap()
            .joints = vec![None, Some(other)];
        graph.destroy(joint).unwrap();
        assert_eq!(graph.state(joint), Err(FrameError::UnknownFrame(joint)));
        assert_eq!(graph.joint_state(other).unwrap().singlemesh, None);
        assert_eq!(
            graph.singlemesh_binding(mesh).unwrap().joints(),
            &[None, None]
        );
        assert_eq!(
            graph
                .singlemesh_invalidation_state(mesh)
                .unwrap()
                .joints_changed,
            7
        );
        assert_eq!(
            graph.destroy(mesh),
            Err(FrameError::UnsupportedDestruction(mesh))
        );
    }

    #[test]
    fn target_destruction_guards_pending_queue_and_dangling_duplicate_links() {
        let mut graph = I3dFrameGraph::default();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph
            .register_target(
                target,
                I3dTargetState::from_parsed_fields(0, &[1, 1], &[linked]).unwrap(),
            )
            .unwrap();
        assert_eq!(
            graph.destroy(linked),
            Err(FrameError::UnsupportedDestruction(linked))
        );
        graph.destroy(target).unwrap();
        assert_eq!(graph.state(linked).unwrap().flags & 0x4000, 0);
        graph.destroy(linked).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph
            .register_target(target, I3dTargetState::default())
            .unwrap();
        graph.update_v(target).unwrap();
        assert_eq!(
            graph.destroy(target),
            Err(FrameError::UnsupportedDestruction(target))
        );
        assert_eq!(graph.target_updates.pending(), &[target]);
        assert!(graph.target_state(target).is_ok());
    }

    #[test]
    fn unimplemented_registration_cleanup_rejects_before_any_graph_mutation() {
        for flag in [0x20000000, 0x8000, 0x400000, 0x80000000] {
            let mut graph = I3dFrameGraph::default();
            let parent = graph.insert(I3dFrameState::model()).unwrap();
            let child = graph.insert(I3dFrameState::dummy()).unwrap();
            graph.link_to(child, Some(parent)).unwrap();
            graph.node_mut(child).unwrap().state.flags |= flag;
            let links = graph.links(parent).unwrap();
            assert_eq!(
                graph.destroy(child),
                Err(FrameError::UnsupportedDestructionRegistrations(child, flag))
            );
            assert_eq!(graph.links(parent).unwrap(), links);
            assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        }
    }

    #[test]
    fn target_registration_retains_links_sets_membership_and_charges_storage() {
        let mut graph = I3dFrameGraph::default();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let before = graph.storage_bytes().unwrap();
        let state = I3dTargetState::from_parsed_fields(0, &[1, 1], &[linked]).unwrap();
        graph.register_target(target, state).unwrap();
        assert_eq!(
            graph.target_state(target).unwrap().frames(),
            [linked, linked]
        );
        assert_ne!(graph.state(linked).unwrap().flags & 0x4000, 0);
        assert!(graph.storage_bytes().unwrap() > before);
        assert_eq!(
            graph.register_target(linked, I3dTargetState::default()),
            Err(FrameError::NotTarget(linked))
        );
    }

    #[test]
    fn target_zero_flags_flushes_world_after_root_dirty_clear_and_retains_storage() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.link_to(target, Some(root)).unwrap();
        graph.assign_position(target, [2., 3., 4.]).unwrap();
        graph
            .register_target(
                target,
                I3dTargetState::from_parsed_fields(0, &[1], &[linked]).unwrap(),
            )
            .unwrap();
        let before = graph.storage_bytes().unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.state(root).unwrap().flags & DIRTY, 0);
        assert_eq!(graph.state(target).unwrap().flags & DIRTY, 0);
        assert_ne!(graph.state(target).unwrap().flags & 0x20, 0);
        assert_eq!(
            graph.state(target).unwrap().world.elements[12..15],
            [2., 3., 4.]
        );
        assert!(graph.target_updates.pending().is_empty());
        assert!(!graph.target_updates.is_flushing());
        assert_eq!(graph.storage_bytes().unwrap(), before);
    }

    #[test]
    fn deferred_target_failure_preserves_forward_prefix_and_poisoned_queue() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let mut targets = Vec::new();
        for flags in [0, 2, 0] {
            let target = graph.insert(I3dFrameState::target()).unwrap();
            graph.assign_position(target, [1., 2., 3.]).unwrap();
            graph
                .register_target(
                    target,
                    I3dTargetState::from_parsed_fields(flags, &[1], &[linked]).unwrap(),
                )
                .unwrap();
            graph.update_v(target).unwrap();
            targets.push(target);
        }
        let error = FrameError::UnsupportedTargetFlags(targets[1], 2);
        assert_eq!(graph.update(root), Err(error));
        assert_eq!(graph.state(root).unwrap().flags & DIRTY, 0);
        assert_ne!(graph.state(targets[0]).unwrap().flags & 0x20, 0);
        assert_ne!(graph.state(targets[1]).unwrap().flags & 0x20, 0);
        assert_eq!(graph.state(targets[2]).unwrap().flags & 0x20, 0);
        assert!(graph.target_updates.is_flushing());
        assert_eq!(graph.target_updates.pending(), targets);
        let root_state = *graph.state(root).unwrap();
        assert_eq!(graph.update(root), Err(error));
        assert_eq!(graph.update_v(targets[2]), Err(error));
        assert_eq!(*graph.state(root).unwrap(), root_state);
        assert_eq!(graph.target_updates.pending(), targets);
    }

    #[test]
    fn target_update_v_checks_active_and_dirty_only_and_retains_duplicates() {
        let mut graph = I3dFrameGraph::default();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph
            .register_target(target, I3dTargetState::default())
            .unwrap();
        graph.node_mut(target).unwrap().state.flags = 0x40000000;
        graph.update_v(target).unwrap();
        assert!(graph.target_updates.pending().is_empty());
        graph.node_mut(target).unwrap().state.flags = 1;
        graph.update_v(target).unwrap();
        assert!(graph.target_updates.pending().is_empty());
        graph.node_mut(target).unwrap().state.flags = 0x2001;
        graph.update_v(target).unwrap();
        graph.update_v(target).unwrap();
        assert_eq!(graph.target_updates.pending(), [target, target]);
    }

    #[test]
    fn target_look_at_reentrant_update_drops_appended_queue_after_snapshot() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph.link_to(linked, Some(root)).unwrap();
        graph.link_to(target, Some(linked)).unwrap();
        graph.assign_position(target, [1., 0., 0.]).unwrap();
        graph
            .register_target(
                target,
                I3dTargetState::from_parsed_fields(1, &[1], &[linked]).unwrap(),
            )
            .unwrap();
        graph.update(root).unwrap();
        assert_eq!(
            graph.state(linked).unwrap().local.elements[8..11],
            [1., 0., 0.]
        );
        // Ordinary traversal and the Target callback both update this frame.
        // Its Target child appends while the deferred queue is already busy.
        assert_eq!(graph.state(linked).unwrap().update_count, 2);
        assert!(graph.target_updates.pending().is_empty());
        assert!(!graph.target_updates.is_flushing());
        assert!(graph.deferred_error.is_none());
    }

    #[test]
    fn direction_world_uses_parent_inverse_and_reports_unavailable_backends() {
        let mut graph = I3dFrameGraph::default();
        let mut parent_state = I3dFrameState::model();
        parent_state.world.elements[0] = 2.;
        parent_state.world.elements[5] = 0.5;
        let parent = graph.insert(parent_state).unwrap();
        let child = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        graph.set_direction_world(child, [1., 0., 1.], 0.).unwrap();
        let state = graph.state(child).unwrap();
        assert_eq!(state.flags & 0x4000003c, 0x40000010);
        assert!(state.local.elements[8] < state.local.elements[10]);
        let isolated = graph.insert(I3dFrameState::dummy()).unwrap();
        let error = FrameError::TargetDirectionUnavailable(isolated);
        assert_eq!(
            graph.set_direction_world(isolated, [1., 0., 0.], 0.),
            Err(error)
        );
        assert_eq!(graph.world_matrix(isolated), Err(error));
        let roll_error = FrameError::TargetDirectionUnavailable(child);
        assert_eq!(
            graph.set_direction_world(child, [1., 0., 1.], 1.),
            Err(roll_error)
        );
        assert_eq!(graph.state(child).unwrap().flags & 0x4000003c, 0x40000010);
    }

    #[test]
    fn evaluated_rotation_preserves_sample_bits_without_second_normalization() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::dummy()).unwrap();
        let rotation = SQuat {
            wxyz: [0.5, -0.25, 0.125, -0.],
        };
        graph.node_mut(key).unwrap().state.flags = 0x1234ffff;
        graph.apply_evaluated_rotation(key, rotation).unwrap();
        assert_eq!(
            graph.state(key).unwrap().rotation.wxyz.map(f32::to_bits),
            rotation.wxyz.map(f32::to_bits)
        );
        assert_eq!(
            graph.state(key).unwrap().flags,
            (0x1234ffff & 0xfffffecb) | 0x40000008
        );
        let before = *graph.state(key).unwrap();
        assert_eq!(
            graph.apply_evaluated_rotation(
                key,
                SQuat {
                    wxyz: [f32::NAN, 0., 0., 0.]
                }
            ),
            Err(FrameError::NumericDomain("evaluated rotation"))
        );
        assert_eq!(*graph.state(key).unwrap(), before);
    }

    #[test]
    fn joint_matrix_stops_at_visual_and_reuses_cached_parent() {
        let mut graph = I3dFrameGraph::default();
        let visual = graph.insert(mesh([100., 0., 0.])).unwrap();
        let mut dummy_state = I3dFrameState::dummy();
        dummy_state.local.elements[12] = 10.;
        let dummy = graph.insert(dummy_state).unwrap();
        let parent = graph.insert(I3dFrameState::joint()).unwrap();
        let child = graph.insert(I3dFrameState::joint()).unwrap();
        graph.assign_position(parent, [2., 0., 0.]).unwrap();
        graph.assign_position(child, [3., 0., 0.]).unwrap();
        graph.link_to(dummy, Some(visual)).unwrap();
        graph.link_to(parent, Some(dummy)).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        assert_eq!(graph.get_joint_matrix(parent).unwrap().elements[12], 12.);
        graph.assign_position(dummy, [20., 0., 0.]).unwrap();
        // Native cached joint bypasses the modified ancestor and own local work.
        assert_eq!(graph.get_joint_matrix(child).unwrap().elements[12], 15.);
        assert_eq!(graph.state(visual).unwrap().flags & 0x14, 0);
        graph.update(parent).unwrap();
        assert_eq!(graph.get_joint_matrix(child).unwrap().elements[12], 25.);
        assert_eq!(graph.joint_state(parent).unwrap().flags & 0x10, 0);
    }

    #[test]
    fn joint_matrix_early_valid_flag_cannot_hide_failed_computation() {
        let mut graph = I3dFrameGraph::default();
        let mut state = I3dFrameState::joint();
        state.flags = 0;
        state.rotation.wxyz[0] = f32::NAN;
        let joint = graph.insert(state).unwrap();
        let error = FrameError::NumericDomain("SetRot3");
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        assert_eq!(graph.joint_state(joint).unwrap().flags, 0x10);
        assert_eq!(graph.state(joint).unwrap().flags, 0x14);
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        graph
            .assign_rotation(
                joint,
                SQuat {
                    wxyz: [1., 0., 0., 0.],
                },
            )
            .unwrap();
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        graph.update_v(joint).unwrap();
        assert_eq!(graph.get_joint_matrix(joint), Ok(SMatrix::identity()));

        let mut state = I3dFrameState::dummy();
        state.flags = 0x14;
        state.local.elements[0] = f32::MAX;
        let dummy = graph.insert(state).unwrap();
        graph.link_to(joint, Some(dummy)).unwrap();
        graph.assign_scale(joint, [2., 1., 1.]).unwrap();
        graph.update_v(joint).unwrap();
        let error = FrameError::NumericDomain("GetJointMatrix multiply");
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        assert_eq!(
            graph
                .joint_state(joint)
                .unwrap()
                .joint_matrix
                .unwrap()
                .elements[0],
            2.
        );
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
    }

    #[test]
    fn joint_hot_cache_copies_raw_bits_and_missing_storage_is_unavailable() {
        let mut graph = I3dFrameGraph::default();
        let mut cache = SMatrix::identity();
        cache.elements[0] = f32::from_bits(0x7fc01234);
        cache.elements[12] = -0.;
        let state = I3dJointState {
            flags: 0x10,
            joint_matrix: Some(cache),
            ..Default::default()
        };
        let joint = graph
            .insert_joint_state(I3dFrameState::joint(), state)
            .unwrap();
        let before = *graph.state(joint).unwrap();
        assert_eq!(
            graph
                .get_joint_matrix(joint)
                .unwrap()
                .elements
                .map(f32::to_bits),
            cache.elements.map(f32::to_bits)
        );
        assert_eq!(*graph.state(joint).unwrap(), before);
        let missing = graph
            .insert_joint_state(
                I3dFrameState::joint(),
                I3dJointState {
                    flags: 0x10,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            graph.get_joint_matrix(missing),
            Err(FrameError::JointMatrixUnavailable(missing))
        );
    }

    #[test]
    fn binding_uses_preorder_first_match_and_keeps_bounds_pending() {
        let mut graph = I3dFrameGraph::default();
        let target = graph
            .insert_singlemesh_binding(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 7 },
                true,
            )
            .unwrap();
        let first = graph.insert(I3dFrameState::joint()).unwrap();
        let nested = graph.insert(mesh([0.; 3])).unwrap();
        let second = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .apply_joint_load_image_fields(second, SMatrix::identity(), 1)
            .unwrap();
        graph.link_to(nested, Some(target)).unwrap();
        graph.link_to(first, Some(target)).unwrap();
        graph.link_to(second, Some(nested)).unwrap();
        assert_eq!(graph.find_joint(target).unwrap(), vec![first, second]);
        let before = *graph.state(target).unwrap();
        assert_eq!(
            graph.d_build_binding(target),
            Ok(DBuildBindingOutcome::BoundVolumeUpdateRequired)
        );
        assert_eq!(
            graph.singlemesh_binding(target).unwrap().joints(),
            &[Some(first), Some(second)]
        );
        assert_eq!(*graph.state(target).unwrap(), before);
        assert_eq!(graph.joint_state(first).unwrap().singlemesh, Some(target));
        graph.remove_child(nested, second).unwrap();
        graph.d_build_binding(target).unwrap();
        assert_eq!(graph.joint_state(second).unwrap().singlemesh, None);

        let duplicate = graph.insert(I3dFrameState::joint()).unwrap();
        graph.link_to(duplicate, Some(target)).unwrap();
        graph.d_build_binding(target).unwrap();
        assert_eq!(
            graph.singlemesh_binding(target).unwrap().joints(),
            &[Some(duplicate), None]
        );
        assert_eq!(graph.joint_state(first).unwrap().singlemesh, None);
        assert_eq!(
            graph.d_build_binding(target),
            Err(FrameError::SinglemeshBindingUnavailable(target))
        );
        graph.update(target).unwrap();
    }

    #[test]
    fn binding_empty_paths_and_search_limit_are_explicit() {
        let mut graph = I3dFrameGraph::default();
        for (present, expected) in [
            (false, DBuildBindingOutcome::NoMesh),
            (true, DBuildBindingOutcome::NoJoints),
        ] {
            let key = graph
                .insert_singlemesh_binding(
                    I3dFrameState::mesh(),
                    SinglemeshInvalidationState { joints_changed: 0 },
                    present,
                )
                .unwrap();
            assert_eq!(graph.d_build_binding(key), Ok(expected));
        }
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut expected = Vec::new();
        for _ in 0..80 {
            let joint = graph.insert(I3dFrameState::joint()).unwrap();
            graph.link_to(joint, Some(root)).unwrap();
            expected.insert(0, joint);
        }
        expected.truncate(64);
        assert_eq!(graph.find_joint(root).unwrap(), expected);
        let bytes = graph.storage_bytes().unwrap();
        graph.find_joint(root).unwrap();
        assert_eq!(graph.storage_bytes().unwrap(), bytes);
    }

    #[test]
    fn loaded_d_build_eagerly_caches_bounds_before_palette_work() {
        let mut graph = I3dFrameGraph::default();
        let target = graph
            .insert_singlemesh_binding(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 23 },
                true,
            )
            .unwrap();
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            (-400_i32) as u32
        );
        let parent = graph.insert(I3dFrameState::joint()).unwrap();
        let child = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .apply_joint_load_image_fields(child, SMatrix::identity(), 1)
            .unwrap();
        graph.assign_position(parent, [2., 0., 0.]).unwrap();
        graph.assign_position(child, [3., 0., 0.]).unwrap();
        graph.link_to(parent, Some(target)).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        let bbox = SinglemeshBoundingBox {
            min: [-1.; 3],
            max: [1.; 3],
        };
        assert!(graph
            .d_build_loaded_singlemesh(target, SinglemeshBoundingBox::EMPTY, &[bbox; 2])
            .unwrap());
        assert_eq!(graph.joint_state(parent).unwrap().flags & 0x10, 0x10);
        assert_eq!(graph.joint_state(child).unwrap().flags & 0x10, 0x10);
        let volume = graph.singlemesh_bound_volume(target).unwrap();
        assert_eq!(volume.bbox.min, [1., -1., -1.]);
        assert_eq!(volume.bbox.max, [6., 1., 1.]);
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            23
        );
        assert_eq!(graph.state(target).unwrap().visual_flags & 0x60000, 0x20000);
        // The cached wrapper does not read a skin definition or rebuild links.
        graph
            .update_singlemesh_loaded_bounds_cached(target, bbox, &[])
            .unwrap();
        assert_eq!(graph.singlemesh_bound_volume(target).unwrap(), volume);
        graph.update_v(child).unwrap();
        assert_eq!(
            graph.singlemesh_bound_volume(target),
            Err(FrameError::SinglemeshBoundsUnavailable(target))
        );
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            23
        );
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            24
        );
        // Counter copy and prior cache effects persist if the next definition
        // cannot provide every record. A stale volume is no longer exposed.
        assert_eq!(
            graph.update_singlemesh_loaded_bounds_cached(target, bbox, &[]),
            Err(FrameError::SinglemeshBoundsDefinitionTooShort(target))
        );
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            24
        );
        assert_eq!(
            graph.singlemesh_bound_volume(target),
            Err(FrameError::SinglemeshBoundsDefinitionTooShort(target))
        );
        // The native counter write preceded failure. Host admission must not
        // turn that equal counter into a successfully published stale volume.
        assert_eq!(
            graph.update_singlemesh_loaded_bounds_cached(target, bbox, &[bbox; 2]),
            Err(FrameError::SinglemeshBoundsDefinitionTooShort(target))
        );
        graph
            .update_singlemesh_loaded_bounds(target, SinglemeshBoundingBox::EMPTY, &[bbox; 2])
            .unwrap();
        assert_eq!(graph.singlemesh_bound_volume(target).unwrap(), volume);
    }
    #[test]
    fn joint_update_invalidates_linked_singlemesh_and_only_cached_ancestors() {
        let mut graph = I3dFrameGraph::default();
        let far = graph.insert(I3dFrameState::model()).unwrap();
        let stop = graph.insert(I3dFrameState::dummy()).unwrap();
        let near = graph.insert(I3dFrameState::dummy()).unwrap();
        let target = graph
            .insert_singlemesh_invalidation_target(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState {
                    joints_changed: u32::MAX,
                },
            )
            .unwrap();
        graph.link_to(stop, Some(far)).unwrap();
        graph.link_to(near, Some(stop)).unwrap();
        graph.link_to(target, Some(near)).unwrap();
        for key in [far, near, target] {
            graph.node_mut(key).unwrap().state.flags = 0x175;
        }
        graph.node_mut(stop).unwrap().state.flags = 0x135;
        graph.node_mut(target).unwrap().state.visual_flags = 0xffffffff;
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(target))
            .unwrap();
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().flags = u32::MAX;
        let own_frame_flags = graph.state(joint).unwrap().flags;
        graph.update_v(joint).unwrap();
        assert_eq!(graph.joint_state(joint).unwrap().flags, 0xffffffe3);
        assert_eq!(graph.state(joint).unwrap().flags, own_frame_flags);
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            0
        );
        assert_eq!(graph.state(target).unwrap().visual_flags, 0xfff9ffff);
        assert_eq!(graph.state(target).unwrap().flags, 0x40000035);
        assert_eq!(graph.state(near).unwrap().flags, 0x35);
        assert_eq!(graph.state(stop).unwrap().flags, 0x135);
        assert_eq!(graph.state(far).unwrap().flags, 0x175);
        assert_eq!(
            graph.update_v(target),
            Err(FrameError::UnsupportedSinglemeshUpdate(target))
        );
        assert_eq!(
            graph.update(target),
            Err(FrameError::UnsupportedSinglemeshUpdate(target))
        );
    }

    #[test]
    fn inherited_joint_change_does_not_invalidate_singlemesh_vertices() {
        let mut graph = I3dFrameGraph::default();
        let target = graph
            .insert_singlemesh_invalidation_target(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 7 },
            )
            .unwrap();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph.link_to(joint, Some(parent)).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(target))
            .unwrap();
        graph.node_mut(joint).unwrap().state.flags = 0x3d;
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().flags = 0x1f;
        let target_before = *graph.state(target).unwrap();
        graph.update(parent).unwrap();
        assert_eq!(graph.joint_state(joint).unwrap().flags, 3);
        assert_eq!(graph.state(joint).unwrap().flags & DIRTY, 0);
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            7
        );
        assert_eq!(*graph.state(target).unwrap(), target_before);
        assert_eq!(
            graph.assign_joint_singlemesh_target(joint, Some(parent)),
            Err(FrameError::InvalidSinglemeshTarget(parent))
        );
        assert_eq!(graph.joint_state(joint).unwrap().singlemesh, Some(target));
    }

    fn mesh(position: [f32; 3]) -> I3dFrameState {
        I3dFrameState::source_mesh(
            position,
            [1.; 3],
            SQuat {
                wxyz: [1., 0., 0., 0.],
            },
        )
    }
    #[test]
    fn row_scaling_retains_earlier_stores_before_an_unavailable_product() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([0.; 3]);
        state.flags = 0x18;
        state.local.elements[0] = 2.;
        state.local.elements[1] = f32::MIN_POSITIVE;
        state.scale = [0.5, 1., 1.];
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("local row scaling"))
        );
        assert_eq!(graph.state(key).unwrap().local.elements[0], 1.);
        assert_eq!(
            graph.state(key).unwrap().local.elements[1],
            f32::MIN_POSITIVE
        );
        assert_eq!(graph.state(key).unwrap().flags, 0x1c);
        assert_eq!(
            graph.world_matrix(key),
            Err(FrameError::NumericDomain("local row scaling"))
        );
        assert_eq!(graph.state(key).unwrap().local.elements[0], 1.);
    }
    #[test]
    fn small_srt_and_quaternion_spills_are_available_but_overflow_is_guarded() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut state = mesh([1e-20, 0., 0.]);
        state.scale = [1e-20; 3];
        let child = graph.insert(state).unwrap();
        graph.link_to(child, Some(root)).unwrap();
        let world = graph.world_matrix(child).unwrap();
        assert_eq!(world.elements[0], 1e-20);
        assert_eq!(world.elements[12], 1e-20);
        let mut state = mesh([0.; 3]);
        state.rotation.wxyz = [0.5, 1e-23, 0., 0.];
        let tiny = graph.insert(state).unwrap();
        let matrix = graph.local_matrix(tiny).unwrap();
        assert_eq!(matrix.elements[6], -1e-23);
        assert_eq!(matrix.elements[9], 1e-23);
        state.rotation.wxyz = [0.5, f32::MAX, 0., 0.];
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        assert_eq!(graph.state(key).unwrap().local, state.local);
        assert_eq!(graph.state(key).unwrap().flags, state.flags | 0x14);
        assert_eq!(
            graph.world_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
    }
    #[test]
    fn common_image_fields_are_raw_stores_after_linking() {
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::mesh()).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        graph.node_mut(child).unwrap().state.flags |= 0x20000080;
        let position = [f32::from_bits(0x80000000), f32::from_bits(0x7fc01234), 7.];
        let rotation = SQuat {
            wxyz: [2., -3., 4., -0.],
        };
        graph
            .apply_common_load_image_fields(child, position, [-2., 0., 3.], rotation, 0x80)
            .unwrap();
        let state = graph.state(child).unwrap();
        assert_eq!(
            state.local.elements[12..15]
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            position.map(f32::to_bits)
        );
        assert_eq!(state.scale, [-2., 0., 3.]);
        assert_eq!(
            state.rotation.wxyz.map(f32::to_bits),
            rotation.wxyz.map(f32::to_bits)
        );
        assert_eq!(state.flags, 0x60000088);
        assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        assert_eq!(state.world, SMatrix::identity());
    }
    #[test]
    fn dummy_virtual_update_has_its_own_flags_and_clean_update_discards_bounds() {
        let mut graph = I3dFrameGraph::default();
        let mut state = I3dFrameState::dummy();
        state.flags = 0x140;
        state.dummy_flags = 0x31;
        state.visual_flags = u32::MAX;
        let key = graph.insert(state).unwrap();
        graph.update_v(key).unwrap();
        assert_eq!(graph.state(key).unwrap().dummy_flags, 0x31);
        graph.update(key).unwrap();
        assert_eq!(graph.state(key).unwrap().flags, 0);
        assert_eq!(graph.state(key).unwrap().update_count, 0);
        graph.mark_loaded_root(key).unwrap();
        assert_eq!(graph.state(key).unwrap().dummy_flags, 0x21);
        assert_eq!(graph.state(key).unwrap().visual_flags, u32::MAX);
        assert_eq!(graph.state(key).unwrap().update_count, 1);
        assert_eq!(graph.state(key).unwrap().flags, 0);
    }
    #[test]
    fn bound_invalidation_stops_at_first_clean_ancestor() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let middle = graph.insert(I3dFrameState::dummy()).unwrap();
        let leaf = graph.insert(I3dFrameState::mesh()).unwrap();
        graph.link_to(middle, Some(root)).unwrap();
        graph.link_to(leaf, Some(middle)).unwrap();
        graph.node_mut(root).unwrap().state.flags = 0x140;
        graph.node_mut(middle).unwrap().state.flags = 0x100;
        graph.node_mut(leaf).unwrap().state.flags = 0x140;
        graph.discard_hr_bound_vol(leaf).unwrap();
        assert_eq!(graph.state(leaf).unwrap().flags, 0);
        assert_eq!(graph.state(middle).unwrap().flags, 0x100);
        assert_eq!(graph.state(root).unwrap().flags, 0x140);
        graph.node_mut(middle).unwrap().state.flags |= 0x40;
        graph.discard_hr_bound_vol(leaf).unwrap();
        assert_eq!(graph.state(middle).unwrap().flags, 0);
        assert_eq!(graph.state(root).unwrap().flags, 0);
    }
    #[test]
    fn world_preserving_reparent_detach_and_locked_position_follow_native_rules() {
        for locked in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let a = graph.insert(mesh([10., 0., 0.])).unwrap();
            let b = graph.insert(mesh([20., 0., 0.])).unwrap();
            let mut state = mesh([3., 0., 0.]);
            if locked {
                state.flags |= 0x80;
            }
            let child = graph.insert(state).unwrap();
            graph.link_to(child, Some(a)).unwrap();
            assert_eq!(graph.world_matrix(child).unwrap().elements[12], 13.);
            assert_eq!(graph.link_to_with_flags(child, Some(b), 1), Ok(0));
            assert_eq!(graph.links(child).unwrap().parent, Some(b));
            assert_eq!(
                graph.state(child).unwrap().local.elements[12],
                if locked { 3. } else { -7. }
            );
            assert_eq!(
                graph.world_matrix(child).unwrap().elements[12],
                if locked { 23. } else { 13. }
            );
            graph.link_to_with_flags(child, None, 1).unwrap();
            assert_eq!(
                graph.world_matrix(child).unwrap().elements[12],
                if locked { 3. } else { 13. }
            );
        }
    }
    #[test]
    fn relink_errors_preserve_native_prefix_writes_and_mark_caches_unavailable() {
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(mesh([10., 0., 0.])).unwrap();
        let mut state = mesh([3., 0., 0.]);
        state.flags = 0x30;
        state.world.elements[12] = 7.;
        state.local.elements[1] = f32::NAN;
        let child = graph.insert(state).unwrap();
        assert_eq!(
            graph.link_to_with_flags(child, Some(parent), 2),
            Err(FrameError::UnsupportedLinkFlags(2))
        );
        assert_eq!(graph.links(child).unwrap().parent, None);
        assert_eq!(
            graph.link_to_with_flags(child, Some(parent), 1),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        assert_eq!(graph.state(child).unwrap().local.elements[12], -3.);
        assert_eq!(graph.state(child).unwrap().scale, [1.; 3]);
        assert_eq!(graph.state(child).unwrap().flags, 0x40000010);
        assert_eq!(
            graph.world_matrix(child),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
    }
    #[test]
    fn singular_parent_uses_native_inverse_identity_fallback() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([10., 0., 0.]);
        state.flags = 0x1c;
        for i in [0, 5, 10] {
            state.local.elements[i] = 0.;
        }
        let parent = graph.insert(state).unwrap();
        let child = graph.insert(mesh([3., 0., 0.])).unwrap();
        assert_eq!(graph.link_to_with_flags(child, Some(parent), 1), Ok(0));
        assert_eq!(graph.state(child).unwrap().local.elements[12], 3.);
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 10.);
    }
    #[test]
    fn parent_changes_stay_cached_until_update_then_repeated_reads_are_clean() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(mesh([1., 0., 0.])).unwrap();
        graph.link_to(child, Some(root)).unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 1.);
        graph.assign_position(root, [10., 0., 0.]).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 1.);
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 11.);
        assert_eq!(
            (
                graph.state(root).unwrap().update_count,
                graph.state(child).unwrap().update_count
            ),
            (2, 2)
        );
        let state = *graph.state(child).unwrap();
        graph.world_matrix(child).unwrap();
        assert_eq!(*graph.state(child).unwrap(), state);
    }
    #[test]
    fn native_head_order_and_remove_child_do_not_invent_matrix_invalidation() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let a = graph.insert(mesh([1., 0., 0.])).unwrap();
        let b = graph.insert(mesh([2., 0., 0.])).unwrap();
        graph.link_to(a, Some(root)).unwrap();
        graph.link_to(b, Some(root)).unwrap();
        assert_eq!(graph.links(root).unwrap().child, Some(b));
        assert_eq!(graph.links(b).unwrap().next, Some(a));
        assert_eq!(graph.links(a).unwrap().previous, Some(b));
        graph.assign_position(root, [10., 0., 0.]).unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        graph.remove_child(root, a).unwrap();
        assert_eq!(graph.links(a).unwrap().parent, None);
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        graph.update_w_matrix_proc(a).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 1.);
        graph.link_to(a, Some(root)).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        let root_links = graph.links(root).unwrap();
        assert_eq!(graph.link_to(root, Some(a)), Err(FrameError::Cycle));
        assert_eq!(graph.links(root).unwrap(), root_links);
    }
    #[test]
    fn equal_and_signed_zero_position_writes_invalidate_but_lock_suppresses() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(mesh([0.; 3])).unwrap();
        graph.world_matrix(key).unwrap();
        graph.assign_position(key, [-0., 0., -0.]).unwrap();
        assert_eq!(
            graph.state(key).unwrap().local.elements[12].to_bits(),
            0x80000000
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x20, 0);
        assert_ne!(graph.state(key).unwrap().flags & 0x40000000, 0);
        let mut state = mesh([1., 2., 3.]);
        state.flags |= 0x80;
        let locked = graph.insert(state).unwrap();
        graph.assign_position(locked, [9.; 3]).unwrap();
        assert_eq!(*graph.state(locked).unwrap(), state);
    }
    #[test]
    fn admitted_matrix_rotation_is_stored_before_cache_flags_and_scaling() {
        for setter in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let mut state = mesh([10., 20., 30.]);
            state.flags = 0x10;
            state.local.elements[0] = 2.;
            state.local.elements[5] = 0.5;
            state.local.elements[10] = -4.;
            state.rotation.wxyz = [0., 1., 0., 0.];
            state.scale = [3., 4., 5.];
            let key = graph.insert(state).unwrap();
            if setter {
                graph.assign_scale(key, [3., 4., 5.]).unwrap();
                assert_eq!(graph.state(key).unwrap().flags, 0x40000008);
                assert_eq!(graph.state(key).unwrap().local, state.local);
            }
            graph.update_matrix(key).unwrap();
            let got = *graph.state(key).unwrap();
            assert_eq!(
                got.rotation.wxyz.map(f32::to_bits),
                [1.0f32.to_bits(), 0, 0, 0]
            );
            let expected_diagonal = if setter { [3., 4., 5.] } else { [6., 2., -20.] };
            assert_eq!(
                [
                    got.local.elements[0],
                    got.local.elements[5],
                    got.local.elements[10]
                ],
                expected_diagonal
            );
            assert_eq!(&got.local.elements[12..15], &[10., 20., 30.]);
            assert_eq!(got.flags, if setter { 0x4000001c } else { 0x1c });
            graph.update_matrix(key).unwrap();
            assert_eq!(*graph.state(key).unwrap(), got);
        }
    }

    #[test]
    fn missing_axis_quaternion_reaches_frame_scale_and_world_cache_consumers() {
        for setter in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let mut state = mesh([10., 20., 30.]);
            state.flags = 0x10;
            state.local.elements[0] = 0.;
            state.rotation.wxyz = [0., 1., 0., 0.];
            state.scale = [2., 3., 4.];
            let key = graph.insert(state).unwrap();
            if setter {
                graph.assign_scale(key, [2., 3., 4.]).unwrap();
                assert_eq!(graph.state(key).unwrap().flags, 0x40000008);
            }
            let local = graph.local_matrix(key).unwrap();
            assert_eq!(
                graph.state(key).unwrap().rotation.wxyz.map(f32::to_bits),
                [1f32.to_bits(), 0, 0, 0]
            );
            assert_eq!(
                [local.elements[0], local.elements[5], local.elements[10]],
                if setter { [2., 3., 4.] } else { [0., 3., 4.] }
            );
            assert_eq!(&local.elements[12..15], &[10., 20., 30.]);
            assert_eq!(graph.world_matrix(key).unwrap(), local);
            let stable = *graph.state(key).unwrap();
            assert_eq!(graph.world_matrix(key).unwrap(), local);
            assert_eq!(*graph.state(key).unwrap(), stable);
        }
    }

    #[test]
    fn numeric_matrix_quaternion_failure_keeps_prior_scale_store_and_flag_order() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([0.; 3]);
        state.flags = 0x10;
        state.local.elements[0] = f32::from_bits(1);
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.assign_scale(key, [2., 3., 4.]),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
        assert_eq!(graph.state(key).unwrap().flags, 0x10);
        assert_eq!(
            graph.update_matrix(key),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.state(key).unwrap().flags, 0x10);
        for flags in [0x30, 0x34] {
            let mut cached = mesh([0.; 3]);
            cached.flags = flags;
            cached.local.elements[0] = f32::from_bits(1);
            let key = graph.insert(cached).unwrap();
            assert!(graph.world_matrix(key).is_ok());
            assert_eq!(
                graph.assign_scale(key, [2., 3., 4.]),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            assert_eq!(graph.state(key).unwrap().flags, flags);
            assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
            assert_eq!(
                graph.world_matrix(key),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            assert_eq!(
                graph.local_matrix(key),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            graph
                .assign_rotation(
                    key,
                    SQuat {
                        wxyz: [1., 0., 0., 0.],
                    },
                )
                .unwrap();
            assert!(graph.world_matrix(key).is_ok());
        }
        let mut bad = mesh([0.; 3]);
        bad.rotation.wxyz[0] = f32::NAN;
        let key = graph.insert(bad).unwrap();
        assert_eq!(
            graph.update_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x14, 0x14);
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        graph
            .assign_rotation(
                key,
                SQuat {
                    wxyz: [1., 0., 0., 0.],
                },
            )
            .unwrap();
        assert!(graph.local_matrix(key).is_ok());
        let mut state = I3dFrameState::model();
        state.scale = [f32::MAX; 3];
        let huge = graph.insert(state).unwrap();
        assert!(graph.update_w_matrix_proc(huge).is_err());
        assert!(graph.world_matrix(huge).is_err());
        graph.assign_scale(huge, [1.; 3]).unwrap();
        assert!(graph.world_matrix(huge).is_ok());
    }
    #[test]
    fn nonunit_source_quaternion_and_world_scale_follow_native_order() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([1., 2., 3.]);
        state.rotation.wxyz = [1.25, 2., 3., 4.];
        state.scale = [100_000_000., -100_000_000., 1.];
        state.local.elements[3] = 7.;
        let key = graph.insert(state).unwrap();
        graph.update_w_matrix_proc(key).unwrap();
        let got = graph.state(key).unwrap();
        assert_eq!(got.local.elements[0], 100_000_000.);
        assert_eq!(got.local.elements[5], -100_000_000.);
        assert_eq!(got.local.elements[10], 1.);
        assert_eq!(got.local.elements[3], 7.);
        assert_eq!(got.world.elements[12..15], [1., 2., 3.]);
        assert_eq!(got.world_scale.to_bits(), 0);
    }
    #[test]
    fn deep_valid_tree_uses_precharged_traversal_storage_without_recursion_budget() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut last = root;
        for _ in 0..512 {
            let key = graph.insert(mesh([1., 0., 0.])).unwrap();
            graph.link_to(key, Some(last)).unwrap();
            last = key;
        }
        let bytes = graph.storage_bytes().unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(last).unwrap().elements[12], 512.);
        assert_eq!(graph.storage_bytes().unwrap(), bytes);
    }
}
