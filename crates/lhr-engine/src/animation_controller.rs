//! Owned PC I3D_IPolSet control and resource lifecycle.
//!
//! LS3DF SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6:
//! SetAnimation 0x1e580, GetFrameInterpolator 0x1ea60, unused cleanup 0x1e9e0,
//! Clear 0x1e2e0. Allocation and resource calls remain explicit dependencies.
//! No pose evaluation, callback reentrancy or native allocator implementation is
//! implied. Cleanup preserves the original count truncation, including detached
//! live allocations; `close` additionally reclaims those through host ownership.

use crate::{
    animation_pose::{POSEF_POS, POSEF_ROT, POSEF_ROT_ADD, POSEF_SCALE},
    animation_set_keys::{ANIMF_POS, ANIMF_ROT, ANIMF_SCALE},
    frame_transform::FrameKey,
    interpolator::{AllocationId, AnimSetId, EvaluatorId, InterpolatorSet, Slot},
    native_precision::{add as fadd, mul as fmul, sub as fsub},
    native_transform::SQuat,
};
use lhr_assets::{animation::AnimationSet, Model};
use std::sync::{
    atomic::{AtomicI32, AtomicU32, Ordering},
    Arc,
};

/// The native weight parameter/getter passes through x87 FLD/FSTP. With the
/// evidenced masked exceptions this quiets signaling NaNs, preserving sign and
/// payload; it leaves infinities, finite values and signed zero unchanged.
fn x87_loaded_weight(weight: f32) -> f32 {
    let bits = weight.to_bits();
    if bits & 0x7f80_0000 == 0x7f80_0000 && bits & 0x007f_ffff != 0 {
        f32::from_bits(bits | 0x0040_0000)
    } else {
        weight
    }
}

#[derive(Clone, Debug)]
pub struct AnimationResource {
    pub id: AnimSetId,
    pub source: Arc<AnimationSet>,
    /// Native animation object +8, separate from the serialized payload.
    pub default_mode: i32,
}

static NEXT_LIVE_ANIMATION: AtomicU32 = AtomicU32::new(1);

#[derive(Debug)]
struct LiveAnimationObject {
    id: AnimSetId,
    source: Arc<AnimationSet>,
    default_mode: AtomicI32,
}

/// Actual shared animation-resource owner. Separate resource objects may share
/// immutable parsed channels while retaining independent identity and mode.
/// Clones alias the same mode word; SetAnimation reads it after retain effects.
#[derive(Clone, Debug)]
pub struct LiveAnimationResource(Arc<LiveAnimationObject>);
impl LiveAnimationResource {
    pub fn new(source: Arc<AnimationSet>, default_mode: i32) -> Option<Self> {
        let id = NEXT_LIVE_ANIMATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .ok()?;
        Some(Self(Arc::new(LiveAnimationObject {
            id: AnimSetId(id),
            source,
            default_mode: AtomicI32::new(default_mode),
        })))
    }
    pub fn id(&self) -> AnimSetId {
        self.0.id
    }
    pub fn source(&self) -> &Arc<AnimationSet> {
        &self.0.source
    }
    pub fn default_mode(&self) -> i32 {
        self.0.default_mode.load(Ordering::Relaxed)
    }
    pub fn set_default_mode(&self, mode: i32) {
        self.0.default_mode.store(mode, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug)]
enum BoundAnimationResource {
    Host(AnimationResource),
    Live(LiveAnimationResource),
}
#[derive(Clone, Copy)]
enum AnimationResourceRef<'a> {
    Host(&'a AnimationResource),
    Live(&'a LiveAnimationResource),
}
impl<'a> AnimationResourceRef<'a> {
    fn id(self) -> AnimSetId {
        match self {
            Self::Host(a) => a.id,
            Self::Live(a) => a.id(),
        }
    }
    fn source(self) -> &'a Arc<AnimationSet> {
        match self {
            Self::Host(a) => &a.source,
            Self::Live(a) => a.source(),
        }
    }
    fn default_mode(self) -> i32 {
        match self {
            Self::Host(a) => a.default_mode,
            Self::Live(a) => a.default_mode(),
        }
    }
    fn retained(self) -> BoundAnimationResource {
        match self {
            Self::Host(a) => BoundAnimationResource::Host(a.clone()),
            Self::Live(a) => BoundAnimationResource::Live(a.clone()),
        }
    }
    fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Host(a), Self::Host(b)) => Arc::ptr_eq(&a.source, &b.source),
            (Self::Live(a), Self::Live(b)) => Arc::ptr_eq(&a.0, &b.0),
            _ => false,
        }
    }
}
impl BoundAnimationResource {
    fn borrowed(&self) -> AnimationResourceRef<'_> {
        match self {
            Self::Host(a) => AnimationResourceRef::Host(a),
            Self::Live(a) => AnimationResourceRef::Live(a),
        }
    }
    fn id(&self) -> AnimSetId {
        self.borrowed().id()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControllerBinding {
    pub track_index: usize,
    pub frame_index: usize,
    /// Clearing/replacing an animation clears this word, retaining other fields.
    pub flags: u16,
}

#[derive(Clone, Debug)]
pub struct ControllerEvaluator {
    pub id: EvaluatorId,
    pub destination: FrameKey,
    pub bindings: [ControllerBinding; 8],
    /// Native evaluator +8; initialized to one independently of slot weights.
    pub pose_blend: f32,
    /// Native evaluator +4 pose word (NULL = none). Eligibility for the
    /// 0x1e960 GetPoseBlend read-back is this word, not binding flags.
    pub pose: Option<PoseId>,
}

/// Refcount traffic for interpolator pose words (native `I3D_pose` ref at
/// +4, virtual Release slot 0). Poses are identity-only in this owner; the
/// `animation_pose` module owns `I3dPose` refcounts. Backends that cannot
/// retain wire the word stores only (disclosed per call site).
pub trait PoseBackend {
    fn retain_pose(&mut self, pose: PoseId);
    fn release_pose(&mut self, pose: PoseId);
}

/// Calls outside the recovered controller bodies. IDs must be nonzero, unique
/// over the backend lifetime and stable until released. Allocation failure is
/// explicit; resource retain/release and destination flag writes are infallible.
/// Backends must not synchronously mutate the controller during these methods.
pub trait ControllerResourceBackend {
    type Error;
    fn allocate_evaluator(&mut self) -> Result<EvaluatorId, Self::Error>;
    fn free_evaluator(&mut self, id: EvaluatorId);
    fn allocate_storage(&mut self, capacity: usize) -> Result<AllocationId, Self::Error>;
    /// AllocationId(0) is the original free(NULL) boundary call.
    fn free_storage(&mut self, id: AllocationId);
    fn retain_animation(&mut self, id: AnimSetId);
    fn release_animation(&mut self, id: AnimSetId);
    fn mark_animated(&mut self, destination: FrameKey);
}

/// Resolve each record against the owner's current membership and local
/// original names. This is called after preceding resource/allocation effects.
pub trait LiveControllerResourceBackend: ControllerResourceBackend {
    fn first_frame_named(&mut self, name: &[u8]) -> Result<Option<(usize, FrameKey)>, Self::Error>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControllerError<E> {
    InvalidSlot,
    InvalidFrameKeys,
    EmbeddedNameTerminator,
    AnimationIdentity,
    Resource(E),
    InvalidResourceIdentity,
    CapacityOverflow,
    HostAllocation,
    Closed,
    Poisoned,
    InvalidView,
}

struct ParsedBinding<'a, B> {
    backend: &'a mut B,
    model: &'a Model,
    frame_keys: &'a [FrameKey],
}
impl<B: ControllerResourceBackend> ControllerResourceBackend for ParsedBinding<'_, B> {
    type Error = B::Error;
    fn allocate_evaluator(&mut self) -> Result<EvaluatorId, Self::Error> {
        self.backend.allocate_evaluator()
    }
    fn free_evaluator(&mut self, id: EvaluatorId) {
        self.backend.free_evaluator(id);
    }
    fn allocate_storage(&mut self, capacity: usize) -> Result<AllocationId, Self::Error> {
        self.backend.allocate_storage(capacity)
    }
    fn free_storage(&mut self, id: AllocationId) {
        self.backend.free_storage(id);
    }
    fn retain_animation(&mut self, id: AnimSetId) {
        self.backend.retain_animation(id);
    }
    fn release_animation(&mut self, id: AnimSetId) {
        self.backend.release_animation(id);
    }
    fn mark_animated(&mut self, destination: FrameKey) {
        self.backend.mark_animated(destination);
    }
}
impl<B: ControllerResourceBackend> LiveControllerResourceBackend for ParsedBinding<'_, B> {
    fn first_frame_named(&mut self, name: &[u8]) -> Result<Option<(usize, FrameKey)>, Self::Error> {
        Ok(self
            .model
            .frames
            .iter()
            .position(|f| f.name_bytes == name)
            .map(|index| (index, self.frame_keys[index])))
    }
}

/// The eight native slots and evaluator prefix belong to one destination owner.
/// Source resources are retained once per occupied slot. Only `state` clock,
/// enabled, weight, mode and dirty fields may be changed by a tick backend;
/// animation IDs and the evaluator view are controlled by this owner.
#[derive(Debug)]
pub struct IPolSet {
    owner: FrameKey,
    state: InterpolatorSet,
    animations: [Option<BoundAnimationResource>; 8],
    storage: Vec<EvaluatorId>,
    owned: Vec<ControllerEvaluator>,
    capacity: usize,
    active_count: usize,
    allocation: AllocationId,
    poisoned: bool,
    closed: bool,
}

/// A borrowed read view of the same controller allocations/resources whose
/// clocks are mutably borrowed by Tick. No state or evaluator is copied out.
pub(crate) struct ControllerEvaluationView<'a> {
    animations: &'a [Option<BoundAnimationResource>; 8],
    storage: &'a [EvaluatorId],
    owned: &'a [ControllerEvaluator],
    allocation: &'a AllocationId,
    active_count: &'a usize,
}
impl ControllerEvaluationView<'_> {
    pub(crate) fn evaluator_at(
        &self,
        allocation: AllocationId,
        index: usize,
    ) -> Option<EvaluatorId> {
        if allocation != *self.allocation || index >= *self.active_count {
            return None;
        }
        self.storage.get(index).copied()
    }
    pub(crate) fn evaluator(&self, id: EvaluatorId) -> Option<&ControllerEvaluator> {
        self.owned.iter().find(|evaluator| evaluator.id == id)
    }
    pub(crate) fn live_animation(&self, slot: usize) -> Option<&LiveAnimationResource> {
        match self.animations.get(slot)?.as_ref()? {
            BoundAnimationResource::Live(resource) => Some(resource),
            BoundAnimationResource::Host(_) => None,
        }
    }
    pub(crate) fn live_resource(&self, id: AnimSetId) -> Option<&LiveAnimationResource> {
        self.animations
            .iter()
            .flatten()
            .find_map(|resource| match resource {
                BoundAnimationResource::Live(resource) if resource.id() == id => Some(resource),
                _ => None,
            })
    }
}

/// Opaque pose handle. `I3d_pose` has no covered entries: identity only.
/// Upstream working name `I3D_pose` (mafia-re), not an evidenced original.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PoseId(pub u32);

/// One `I3D_pose_set` entry: owned frame-name bytes + opaque pose.
/// Native entry array lives at `set+8`, count at `set+0x10`; each entry's
/// name pointer at `*entry+8` (null = empty string).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoseSetEntry {
    pub frame_name: Vec<u8>,
    pub pose: PoseId,
}

impl ControllerEvaluator {
    /// LS3DF RVA 0x1d2f0 `I3D_interpolator::SetPoseBlend` (contract: no Ghidra
    /// body — `LAB_1001d2f0`, `analyze_function` not_found; `read_bytes`
    /// shows FLD/FSTP store to +8 + `RET 8`). Upstream name, not an evidenced
    /// original symbol. Bit-exact store; no clamp, no NaN policy change.
    pub fn store_pose_blend_bits(&mut self, blend_bits: u32) {
        self.pose_blend = f32::from_bits(blend_bits);
    }

    /// LS3DF RVA 0x1d300 `I3D_interpolator::ClearAnimation` (contract: no
    /// body, `LAB_1001d300`; bytes show flags-byte clear + `I3D_OK`).
    /// Upstream name, not evidenced original. Zeroes exactly the
    /// binding-flags word, preserving track/frame indices. The native bound
    /// check is unevidenced — the host maps out-of-range to `InvalidSlot`,
    /// never to a word write.
    pub fn clear_slot_flags(
        &mut self,
        slot: usize,
    ) -> Result<u32, ControllerError<std::convert::Infallible>> {
        let b = self
            .bindings
            .get_mut(slot)
            .ok_or(ControllerError::InvalidSlot)?;
        let (track, frame) = (b.track_index, b.frame_index);
        b.flags = 0;
        debug_assert_eq!((b.track_index, b.frame_index), (track, frame));
        Ok(0)
    }

    /// LS3DF RVA 0x1e230 `I3D_interpolator::SetPose` (differential: 29 asm /
    /// 7 BB). Upstream name, not evidenced original. Same-pointer → false,
    /// no backend call, dirty untouched. Else dirty FIRST, then release-old
    /// (word still holds the old pointer — never an intermediate NULL),
    /// store-new, retain-new. Widths: pose word +4, dirty byte at set+4.
    pub fn set_interpolator_pose<B: PoseBackend>(
        &mut self,
        backend: &mut B,
        pose: Option<PoseId>,
        dirty: &mut bool,
    ) -> bool {
        let old = self.pose;
        if old == pose {
            return false;
        }
        *dirty = true;
        if let Some(o) = old {
            backend.release_pose(o);
        }
        self.pose = pose;
        if let Some(new) = pose {
            backend.retain_pose(new);
        }
        true
    }

    /// LS3DF RVA 0x1d320 `I3D_interpolator::Update`
    /// (mafia-re ls3df/I3D/anim/I3d_interpolator.cpp:79-228).
    /// Evidenced original name (PS2 build; header notes names are the PS2
    /// build unless noted). Probe verified a formed Ghidra body
    /// (FUN_1001d320); already ledger-credited (frozen port `5DS isolated
    /// position-channel intake and sampling`): NO overlay entry here.
    /// Confirm-only full evaluator.
    /// Channel sampling itself reuses the existing rot/pos/scale branch
    /// samplers (`rotation_animation::sample_rotation_channel`,
    /// `lhr_assets` position/scale channels); this method is the missing
    /// channel-major blending plus pose plus outcome ordering. Per-slot
    /// `samples` are the already-interpolated values for this evaluator at
    /// the slots current times (None = no sampled value on the admitted
    /// domain, skipped without effect). Frame stores (`SetPos`, `m_Rot`
    /// plus flag word, `SetScale`), the null-frame early-out and note
    /// dispatch stay caller backend work (disclosed): `destination` is
    /// always a live owner here, detached views are skipped by the caller,
    /// and notes keep the existing unsupported contract in
    /// `model_animation_runtime`. Ordered weight comparisons preserve NaN
    /// (`!(w <= 0)` admits NaN to sampling, `!(w >= 1)` blends NaN);
    /// `pose_blend` uses the native ordered `> 0` / `>= 1` (NaN skips).
    /// Position/scale blends use the native `*v * b + acc * (1 - b)` order
    /// under the crate PC24 pipeline; rotation blends use the native
    /// spherical `Slerp(_, _, blend, false)` (spherical arm via
    /// `rotation_animation::try_slerp`, availability-bound).
    pub fn update_interpolator(
        &self,
        slots: &[Slot; 8],
        samples: &[UpdateChannelSample; 8],
        pose: Option<UpdatePoseSample>,
    ) -> Result<UpdateOutcome, UpdateInterpolatorError> {
        let mut pos = [0.0f32; 3];
        let mut num_pos: usize = 0;
        for i in 0..8 {
            if u32::from(self.bindings[i].flags) & ANIMF_POS == 0 {
                continue;
            }
            let slot = &slots[i];
            if !slot.enabled {
                continue;
            }
            let weight = slot.weight;
            if !(weight <= 0.0) {
            } else {
                continue;
            }
            let Some(value) = samples[i].position else {
                continue;
            };
            if num_pos != 0 && !(weight >= 1.0) {
                let complement = fsub(1.0, weight);
                pos = [
                    fadd(fmul(value[0], weight), fmul(pos[0], complement)),
                    fadd(fmul(value[1], weight), fmul(pos[1], complement)),
                    fadd(fmul(value[2], weight), fmul(pos[2], complement)),
                ];
            } else {
                pos = value;
            }
            num_pos += 1;
        }
        let mut rot = [1.0f32, 0.0, 0.0, 0.0];
        let mut num_rot: usize = 0;
        for i in 0..8 {
            if u32::from(self.bindings[i].flags) & ANIMF_ROT == 0 {
                continue;
            }
            let slot = &slots[i];
            if !slot.enabled {
                continue;
            }
            let weight = slot.weight;
            if !(weight <= 0.0) {
            } else {
                continue;
            }
            let Some(value) = samples[i].rotation else {
                continue;
            };
            if num_rot != 0 && !(weight >= 1.0) {
                let blended = crate::rotation_animation::try_slerp(
                    SQuat { wxyz: rot },
                    SQuat { wxyz: value },
                    weight,
                )?;
                rot = blended.wxyz;
                num_rot += 1;
            } else {
                rot = value;
                num_rot = 1;
            }
        }
        let mut scale = [0.0f32; 3];
        let mut num_scale: usize = 0;
        for i in 0..8 {
            if u32::from(self.bindings[i].flags) & ANIMF_SCALE == 0 {
                continue;
            }
            let slot = &slots[i];
            if !slot.enabled {
                continue;
            }
            let weight = slot.weight;
            if !(weight <= 0.0) {
            } else {
                continue;
            }
            let Some(value) = samples[i].scale else {
                continue;
            };
            if num_scale != 0 && !(weight >= 1.0) {
                let complement = fsub(1.0, weight);
                scale = [
                    fadd(fmul(value[0], weight), fmul(scale[0], complement)),
                    fadd(fmul(value[1], weight), fmul(scale[1], complement)),
                    fadd(fmul(value[2], weight), fmul(scale[2], complement)),
                ];
            } else {
                scale = value;
            }
            num_scale += 1;
        }
        if let Some(pose) = pose {
            let blend = self.pose_blend;
            if blend > 0.0 {
                if blend >= 1.0 {
                    if pose.flags & POSEF_POS != 0 {
                        pos = pose.pos;
                    }
                    if pose.flags & POSEF_SCALE != 0 {
                        scale = pose.scale;
                    }
                    if pose.flags & POSEF_ROT_ADD != 0 {
                        if num_rot != 0 {
                            num_rot += 1;
                            rot = SQuat { wxyz: pose.rot }.compose(SQuat { wxyz: rot }).wxyz;
                        } else {
                            rot = pose.rot;
                            num_rot = 1;
                        }
                    } else if pose.flags & POSEF_ROT != 0 {
                        rot = pose.rot;
                        num_rot = 1;
                    }
                } else {
                    if pose.flags & POSEF_POS != 0 {
                        if num_pos == 0 {
                            pos = pose.pos;
                        } else {
                            let complement = fsub(1.0, blend);
                            pos = [
                                fadd(fmul(pose.pos[0], blend), fmul(pos[0], complement)),
                                fadd(fmul(pose.pos[1], blend), fmul(pos[1], complement)),
                                fadd(fmul(pose.pos[2], blend), fmul(pos[2], complement)),
                            ];
                        }
                        num_pos += 1;
                    }
                    if pose.flags & POSEF_SCALE != 0 {
                        if num_scale == 0 {
                            scale = pose.scale;
                        } else {
                            let complement = fsub(1.0, blend);
                            scale = [
                                fadd(fmul(pose.scale[0], blend), fmul(scale[0], complement)),
                                fadd(fmul(pose.scale[1], blend), fmul(scale[1], complement)),
                                fadd(fmul(pose.scale[2], blend), fmul(scale[2], complement)),
                            ];
                        }
                        num_scale += 1;
                    }
                    if pose.flags & POSEF_ROT_ADD != 0 {
                        if num_rot == 0 {
                            rot = pose.rot;
                            num_rot = 1;
                        } else {
                            num_rot += 1;
                            let mut scaled = SQuat { wxyz: pose.rot };
                            scaled.wxyz[0] = fmul(scaled.wxyz[0], blend);
                            rot = scaled.compose(SQuat { wxyz: rot }).wxyz;
                        }
                    } else if pose.flags & POSEF_ROT != 0 {
                        if num_rot == 0 {
                            rot = pose.rot;
                        } else {
                            rot = crate::rotation_animation::try_slerp(
                                SQuat { wxyz: rot },
                                SQuat { wxyz: pose.rot },
                                blend,
                            )?
                            .wxyz;
                        }
                        num_rot += 1;
                    }
                }
            }
        }
        Ok(UpdateOutcome {
            position: (num_pos != 0).then_some(pos),
            rotation: (num_rot != 0).then_some(rot),
            scale: (num_scale != 0).then_some(scale),
            num_pos,
            num_rot,
            num_scale,
        })
    }
}

/// Per-slot already-sampled channel values for
/// `ControllerEvaluator::update_interpolator` (RVA 0x1d320). Sampling reuses
/// the existing rot/pos/scale branch samplers; None means the admitted
/// domain had no value (skipped, never a zero substitute).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpdateChannelSample {
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
}

/// Pose fields for `update_interpolator`. Flags are `POSEF_*`
/// (`animation_pose`); `pos`/`scale`/`rot` are the `GetPos1`/`GetScale1`/
/// `GetRot1` words. Presence is the `pose` word (`Some` = nonnull).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpdatePoseSample {
    pub flags: u32,
    pub pos: [f32; 3],
    pub scale: [f32; 3],
    pub rot: [f32; 4],
}

/// Stored outcome of `update_interpolator`: values are `Some` exactly when
/// the corresponding native `num_*` counter admits the frame store
/// (`num_pos`/`num_rot`/`num_scale` nonzero, including pose-driven counts).
/// Normalization and flag words stay caller work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpdateOutcome {
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
    pub num_pos: usize,
    pub num_rot: usize,
    pub num_scale: usize,
}

/// Availability boundary of `update_interpolator`: rotation blending goes
/// through the bounded spherical `Slerp` (angles past 1.5 rad and
/// non-ordinary intermediates are unavailable, never substituted).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateInterpolatorError {
    Rotation(crate::rotation_animation::RotationUnavailable),
}

impl From<crate::rotation_animation::RotationUnavailable> for UpdateInterpolatorError {
    fn from(source: crate::rotation_animation::RotationUnavailable) -> Self {
        Self::Rotation(source)
    }
}

impl IPolSet {
    pub(crate) fn validate_live_view(
        &self,
    ) -> Result<(), ControllerError<std::convert::Infallible>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        if !self.owns_view() {
            return Err(ControllerError::InvalidView);
        }
        Ok(())
    }
    pub(crate) fn borrow_tick_parts(
        &mut self,
    ) -> Result<
        (&mut InterpolatorSet, ControllerEvaluationView<'_>),
        ControllerError<std::convert::Infallible>,
    > {
        self.validate_live_view()?;
        Ok((
            &mut self.state,
            ControllerEvaluationView {
                animations: &self.animations,
                storage: &self.storage,
                owned: &self.owned,
                allocation: &self.allocation,
                active_count: &self.active_count,
            },
        ))
    }
    /// Constructor stores at 0x337d2..0x33809 zero the eight slots and vector,
    /// and set growth to32. They leave the dirty byte unwritten. The host must
    /// supply that allocation byte explicitly, rather than infer a native zero.
    pub fn new(owner: FrameKey, allocation_dirty: bool) -> Self {
        Self {
            owner,
            state: InterpolatorSet {
                dirty: allocation_dirty,
                ..Default::default()
            },
            animations: std::array::from_fn(|_| None),
            storage: Vec::new(),
            owned: Vec::new(),
            capacity: 0,
            active_count: 0,
            allocation: AllocationId(0),
            poisoned: false,
            closed: false,
        }
    }
    pub fn owner(&self) -> FrameKey {
        self.owner
    }
    pub fn state(&self) -> &InterpolatorSet {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut InterpolatorSet {
        &mut self.state
    }
    /// Existing parsed host resource view; actual shared objects use
    /// `live_animation`. `animation_id` covers both admitted resource kinds.
    pub fn animation(&self, slot: usize) -> Option<&AnimationResource> {
        match self.animations.get(slot)?.as_ref()? {
            BoundAnimationResource::Host(a) => Some(a),
            BoundAnimationResource::Live(_) => None,
        }
    }
    pub fn live_animation(&self, slot: usize) -> Option<&LiveAnimationResource> {
        match self.animations.get(slot)?.as_ref()? {
            BoundAnimationResource::Live(a) => Some(a),
            BoundAnimationResource::Host(_) => None,
        }
    }
    pub fn animation_id(&self, slot: usize) -> Option<AnimSetId> {
        self.animations
            .get(slot)?
            .as_ref()
            .map(BoundAnimationResource::id)
    }
    pub fn evaluator(&self, id: EvaluatorId) -> Option<&ControllerEvaluator> {
        self.owned.iter().find(|e| e.id == id)
    }
    pub fn active_evaluators(&self) -> &[EvaluatorId] {
        &self.storage[..self.active_count]
    }
    pub fn evaluator_capacity(&self) -> usize {
        self.capacity
    }
    pub fn owned_evaluator_count(&self) -> usize {
        self.owned.len()
    }

    /// Bound source identities and allocation ownership are not exposed through
    /// the tick state. Validate them before accepting a manually supplied view.
    pub fn owns_view(&self) -> bool {
        self.state.evaluators.count == self.active_count
            && self.state.evaluators.allocation == self.allocation
            && self
                .state
                .slots
                .iter()
                .zip(&self.animations)
                .all(|(s, a)| s.animset == a.as_ref().map(|a| a.id()))
    }

    /// Exact first destination identity match, append after initializing a new
    /// evaluator, grow pointer storage by32, then set destination flag0x10000000.
    pub fn get_frame_interpolator<B: ControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        destination: FrameKey,
    ) -> Result<EvaluatorId, ControllerError<B::Error>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        if !self.owns_view() {
            return Err(ControllerError::InvalidView);
        }
        if let Some(id) = self
            .active_evaluators()
            .iter()
            .copied()
            .find(|id| self.evaluator(*id).unwrap().destination == destination)
        {
            return Ok(id);
        }
        self.owned
            .try_reserve(1)
            .map_err(|_| ControllerError::HostAllocation)?;
        let id = backend
            .allocate_evaluator()
            .map_err(ControllerError::Resource)?;
        if id.0 == 0 || self.owned.iter().any(|e| e.id == id) {
            self.poisoned = true;
            return Err(ControllerError::InvalidResourceIdentity);
        }
        // Retain ownership even if host vector allocation fails; native has no
        // recoverable vector-allocation failure branch and would dereference0.
        self.owned.push(ControllerEvaluator {
            id,
            destination,
            bindings: [ControllerBinding::default(); 8],
            pose_blend: 1.0,
            pose: None,
        });
        let count = self.state.evaluators.count;
        if self.capacity <= count {
            self.capacity = self
                .capacity
                .checked_add(32)
                .ok_or(ControllerError::CapacityOverflow)?;
            let allocation = match backend.allocate_storage(self.capacity) {
                Ok(allocation) => allocation,
                Err(error) => {
                    self.poisoned = true;
                    return Err(ControllerError::Resource(error));
                }
            };
            if allocation.0 == 0 || allocation == self.state.evaluators.allocation {
                self.poisoned = true;
                return Err(ControllerError::InvalidResourceIdentity);
            }
            backend.free_storage(self.state.evaluators.allocation);
            self.state.evaluators.allocation = allocation;
            self.allocation = allocation;
        }
        if count < self.storage.len() {
            self.storage[count] = id;
        } else {
            if self.storage.try_reserve(1).is_err() {
                self.poisoned = true;
                return Err(ControllerError::HostAllocation);
            }
            self.storage.push(id);
        }
        self.state.evaluators.count += 1;
        self.active_count += 1;
        backend.mark_animated(destination);
        Ok(id)
    }

    /// LS3DF RVA 0x1eba0 `I3D_IPolSet::GetFrameInterpolator(const char*)`
    /// (differential: 127 asm / 22 BB). Name-resolved variant of
    /// `get_frame_interpolator` (0x1ea60, reused worker — never re-claimed).
    /// Upstream name, not an evidenced original symbol. Order: active-list
    /// name scan conceptually first (native byte-pair strcmp), then the
    /// owner search (`first_frame_named`, backend contract for
    /// FindChildFrame+0x2ffff), then allocate-or-hit. The host resolves the
    /// name to a key first and lets the worker scan-hit on destination
    /// identity — observably identical (no duplicate allocation, miss has no
    /// effects, dirty untouched) minus the resolver domain, disclosed.
    /// Miss returns None with NO effects.
    pub fn get_frame_interpolator_named<B: LiveControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        frame_name: &[u8],
    ) -> Result<Option<EvaluatorId>, ControllerError<B::Error>> {
        self.pose_common_guard()?;
        if frame_name.contains(&0) {
            return Err(ControllerError::EmbeddedNameTerminator);
        }
        let Some((_, key)) = backend
            .first_frame_named(frame_name)
            .map_err(ControllerError::Resource)?
        else {
            return Ok(None);
        };
        Ok(Some(self.get_frame_interpolator(backend, key)?))
    }

    /// SetAnimation changes resource references only when object identity changes.
    /// The same nonnull source restarts, rebinds and resets previous to-40, while
    /// last_delta is preserved. A null-to-null call has no effect at all.
    #[allow(clippy::too_many_arguments)] // Native arguments plus explicit source graph/backend.
    pub fn set_animation<B: ControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        model: &Model,
        frame_keys: &[FrameKey],
        animation: Option<&AnimationResource>,
        slot: usize,
        weight: f32,
        mode: i32,
    ) -> Result<(), ControllerError<B::Error>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        if !self.owns_view() {
            return Err(ControllerError::InvalidView);
        }
        if slot >= 8 {
            return Err(ControllerError::InvalidSlot);
        }
        if model.frames.len() != frame_keys.len()
            || frame_keys
                .iter()
                .enumerate()
                .any(|(i, k)| frame_keys[..i].contains(k))
        {
            return Err(ControllerError::InvalidFrameKeys);
        }
        if model.frames.iter().any(|f| f.name_bytes.contains(&0))
            || animation.is_some_and(|a| a.source.tracks.iter().any(|t| t.name.contains(&0)))
        {
            return Err(ControllerError::EmbeddedNameTerminator);
        }
        let mut backend = ParsedBinding {
            backend,
            model,
            frame_keys,
        };
        self.set_animation_resolved(
            &mut backend,
            animation.map(AnimationResourceRef::Host),
            slot,
            weight,
            mode,
        )
    }

    /// Actual resource handle and live owner resolver. Safe-host slot admission
    /// remains explicit; native SetAnimation itself has no out-of-range guard.
    pub fn set_live_animation<B: LiveControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        animation: Option<&LiveAnimationResource>,
        slot: usize,
        weight: f32,
        mode: i32,
    ) -> Result<(), ControllerError<B::Error>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        if !self.owns_view() {
            return Err(ControllerError::InvalidView);
        }
        if slot >= 8 {
            return Err(ControllerError::InvalidSlot);
        }
        self.set_animation_resolved(
            backend,
            animation.map(AnimationResourceRef::Live),
            slot,
            weight,
            mode,
        )
    }
    fn set_animation_resolved<B: LiveControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        animation: Option<AnimationResourceRef<'_>>,
        slot: usize,
        weight: f32,
        mode: i32,
    ) -> Result<(), ControllerError<B::Error>> {
        if let Some(a) = animation {
            if a.id().0 == 0
                || self
                    .animations
                    .iter()
                    .flatten()
                    .any(|old| (old.id() == a.id()) != old.borrowed().same(a))
            {
                return Err(ControllerError::AnimationIdentity);
            }
        }
        let same = match (&self.animations[slot], animation) {
            (None, None) => true,
            (Some(a), Some(b)) => a.borrowed().same(b),
            _ => false,
        };
        if !same {
            if let Some(old) = self.animations[slot].take() {
                let active = self.active_evaluators().to_vec();
                for id in active {
                    self.owned.iter_mut().find(|e| e.id == id).unwrap().bindings[slot].flags = 0;
                }
                backend.release_animation(old.id());
            }
            self.state.slots[slot].animset = animation.map(|a| a.id());
            self.animations[slot] = animation.map(|a| a.retained());
            if let Some(new) = animation {
                backend.retain_animation(new.id());
            } else {
                self.state.slots[slot].enabled = false;
                self.release_unused_interpolators(backend);
                return Ok(());
            }
        }
        let Some(animation) = animation else {
            return Ok(());
        };
        let s = &mut self.state.slots[slot];
        s.enabled = true;
        s.current = 0;
        s.previous = -40;
        s.weight = x87_loaded_weight(weight);
        s.mode = if mode == 0 {
            animation.default_mode()
        } else {
            mode
        };
        for (track_index, track) in animation.source().tracks.iter().enumerate() {
            let Some((frame_index, destination)) = backend
                .first_frame_named(&track.name)
                .map_err(ControllerError::Resource)?
            else {
                continue;
            };
            let id = self.get_frame_interpolator(backend, destination)?;
            self.owned.iter_mut().find(|e| e.id == id).unwrap().bindings[slot] =
                ControllerBinding {
                    track_index,
                    frame_index,
                    flags: track.flags,
                };
        }
        self.release_unused_interpolators(backend);
        self.state.dirty = true;
        Ok(())
    }

    /// Original reverse walk writes count=index after every deletion. It does
    /// not compact the vector. An enabled slot keeps every evaluator alive,
    /// regardless of whether that evaluator has a channel for that slot.
    pub fn release_unused_interpolators<B: ControllerResourceBackend>(&mut self, backend: &mut B) {
        let initial_count = self.active_count;
        for index in (0..initial_count).rev() {
            let id = self.storage[index];
            let evaluator = self.evaluator(id).unwrap();
            // Native checks the pose word FIRST (0x1e9e0 skips pose-holding
            // interpolators even with no animations/flags); without this,
            // wave-2 pose stores would make pose-only evaluators collectible.
            if evaluator.pose.is_some() {
                continue;
            }
            if !self.state.slots.iter().any(|s| s.enabled)
                && evaluator.bindings.iter().all(|b| b.flags == 0)
            {
                backend.free_evaluator(id);
                self.owned.retain(|e| e.id != id);
                self.state.evaluators.count = index;
                self.active_count = index;
            }
        }
    }

    /// Original Clear releases active evaluators in forward order, then all
    /// slot references, and zeroes slots. Storage/capacity and dirty are kept.
    /// Detached allocations remain owned until close; they cannot be evaluated.
    pub fn clear<B: ControllerResourceBackend>(&mut self, backend: &mut B) {
        for id in self.active_evaluators().to_vec() {
            backend.free_evaluator(id);
            self.owned.retain(|e| e.id != id);
        }
        self.state.evaluators.count = 0;
        self.active_count = 0;
        for animation in &mut self.animations {
            if let Some(animation) = animation.take() {
                backend.release_animation(animation.id());
            }
        }
        self.state.slots = [Slot::default(); 8];
    }
    /// Original destructor clears then releases pointer storage. Reclaiming any
    /// detached native allocations is an explicit additional host safety step.
    pub fn close<B: ControllerResourceBackend>(&mut self, backend: &mut B) {
        if self.closed {
            return;
        }
        self.clear(backend);
        for evaluator in self.owned.drain(..) {
            backend.free_evaluator(evaluator.id);
        }
        backend.free_storage(self.allocation);
        self.allocation = AllocationId(0);
        self.state.evaluators = Default::default();
        self.capacity = 0;
        self.closed = true;
    }

    /// Model StopAnimation/StartAnimation wrappers silently ignore invalid slots.
    /// Start resumes the stored time; neither operation makes the set dirty.
    pub fn stop_animation(&mut self, slot: usize) {
        if let Some(s) = self.state.slots.get_mut(slot) {
            s.enabled = false;
        }
    }
    pub fn start_animation(&mut self, slot: usize) {
        if let Some(s) = self.state.slots.get_mut(slot) {
            s.enabled = true;
        }
    }
    pub fn set_anim_time(
        &mut self,
        slot: usize,
        time: i32,
    ) -> Result<(), ControllerError<std::convert::Infallible>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        let s = self
            .state
            .slots
            .get_mut(slot)
            .ok_or(ControllerError::InvalidSlot)?;
        s.current = time;
        s.previous = time;
        self.state.dirty = true;
        Ok(())
    }
    pub fn anim_time(&self, slot: usize) -> i32 {
        self.state.slots.get(slot).map_or(0, |s| s.current)
    }
    #[allow(clippy::manual_clamp)] // Preserve the observed ordered x87 comparisons and IEEE bits.
    pub fn set_anim_blend(
        &mut self,
        slot: usize,
        weight: f32,
    ) -> Result<(), ControllerError<std::convert::Infallible>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        let s = self
            .state
            .slots
            .get_mut(slot)
            .ok_or(ControllerError::InvalidSlot)?;
        let weight = x87_loaded_weight(weight);
        // Ordered comparisons preserve quiet NaN bits and signed zero as x87 does.
        s.weight = if weight < 0.0 {
            0.0
        } else if weight > 1.0 {
            1.0
        } else {
            weight
        };
        self.state.dirty = true;
        Ok(())
    }
    pub fn anim_blend(&self, slot: usize) -> f32 {
        self.state
            .slots
            .get(slot)
            .map_or(0.0, |s| x87_loaded_weight(s.weight))
    }
    pub fn set_anim_options(
        &mut self,
        slot: usize,
        mode: i32,
    ) -> Result<(), ControllerError<std::convert::Infallible>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        self.state
            .slots
            .get_mut(slot)
            .ok_or(ControllerError::InvalidSlot)?
            .mode = mode;
        Ok(())
    }
    pub fn anim_options(&self, slot: usize) -> i32 {
        self.state.slots.get(slot).map_or(0, |s| s.mode)
    }

    /// RVA 0x34090 NumInterpolators (LS3DF.dll e211c850…): null set -> 0
    /// Upstream working name, not an evidenced original symbol.
    /// else evaluator count (u32 word at ipol+0xf0). No Ghidra body in
    /// session -> tier contract per the 0x2b830 standard. Saturates at
    /// u32::MAX on hostile views (width admission, not a native branch).
    /// Closed views read 0 (null-set analog; native has no close state).
    pub fn num_interpolators(&self) -> u32 {
        if self.closed {
            return 0;
        }
        u32::try_from(self.state.evaluators.count).unwrap_or(u32::MAX)
    }
    /// RVA 0x340b0 GetInterpolators: None = no set (native NULL);
    /// Upstream working name, not an evidenced original symbol.
    /// Some(view) = base at +0xe8. Empty set yields Some(&[]). Borrowed
    /// live view; never a detached copy. No Ghidra body -> contract.
    pub fn interpolator_storage(&self) -> Option<&[EvaluatorId]> {
        if self.closed {
            return None;
        }
        Some(self.active_evaluators())
    }
    fn pose_common_guard<E>(&self) -> Result<(), ControllerError<E>> {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        if !self.owns_view() {
            return Err(ControllerError::InvalidView);
        }
        Ok(())
    }
    /// Helper-phase worker for the SetPose entry loops (no function credit).
    /// Resolves each entry name in order against live membership (native
    /// byte-compare-then-owner-search order); prefix effects are kept on
    /// miss (no rollback), mirroring the native loops.
    fn resolve_pose_entries<B: LiveControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        entries: &[PoseSetEntry],
    ) -> Result<Vec<EvaluatorId>, ControllerError<B::Error>> {
        let mut out = Vec::new();
        // Host-defensive: test-sized entry lists cannot force this
        // reservation to fail on a 64-bit host (native new[] OOM analog).
        out.try_reserve(entries.len())
            .map_err(|_| ControllerError::HostAllocation)?;
        for entry in entries {
            if entry.frame_name.contains(&0) {
                return Err(ControllerError::EmbeddedNameTerminator);
            }
            // Case-sensitive resolve first (native strcmp loop), then the
            // owner's search on miss. The flag-domain difference (native
            // 0x2ffff vs scene 0x4ffff) is recorded, not emulated, here:
            // the host resolver is the same first_frame_named seam
            // SetAnimation uses.
            let Some((_, key)) = backend
                .first_frame_named(&entry.frame_name)
                .map_err(ControllerError::Resource)?
            else {
                return Err(ControllerError::InvalidResourceIdentity);
            };
            out.push(self.get_frame_interpolator(backend, key)?);
        }
        Ok(out)
    }
    /// RVA 0x340d0 helper (SetPose(set) application; overlay 0x1e860 is
    /// contract — word stores ported, retain traffic unwired): apply pose
    /// ids in entry order, dirty only on full success. Miss ->
    /// OBJECTNOTFOUND with prior applies kept and dirty untouched.
    /// Upstream working name, not an evidenced original symbol.
    pub fn set_pose_set<B: LiveControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        entries: &[PoseSetEntry],
    ) -> Result<(), ControllerError<B::Error>> {
        self.pose_common_guard()?;
        // Native interleaved order (0x1e860): per entry, resolve THEN apply
        // (virtual slot +4: same-word no-op, else retain traffic + store);
        // a miss returns OBJECTNOTFOUND with prior applies and allocations
        // kept — no rollback. Retain traffic has no seam on this backend
        // (disclosed); the word stores below are the credited behavior.
        for entry in entries {
            let ids = self.resolve_pose_entries(backend, std::slice::from_ref(entry))?;
            let id = ids.into_iter().next().unwrap();
            if let Some(ev) = self.owned.iter_mut().find(|ev| ev.id == id) {
                ev.pose = Some(entry.pose);
            }
        }
        self.state.dirty = true;
        Ok(())
    }
    /// RVA 0x34170 helper (SetPose(frame) application): exact-identity
    /// Upstream working name, not an evidenced original symbol.
    /// destination targeting. Body-backed entry; helper phase, no credit.
    pub fn set_pose_frame<B: ControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        destination: FrameKey,
        pose: PoseId,
    ) -> Result<(), ControllerError<B::Error>> {
        self.pose_common_guard()?;
        let id = self.get_frame_interpolator(backend, destination)?;
        // 0x1e230 apply order on the resolved evaluator (retain traffic has
        // no seam on this backend — disclosed; the word store is credited).
        if let Some(e) = self.owned.iter_mut().find(|e| e.id == id) {
            e.pose = Some(pose);
        }
        self.state.dirty = true;
        Ok(())
    }
    /// RVA 0x34210 helper (SetPose(name) application): name-resolved
    /// Upstream working name, not an evidenced original symbol.
    /// targeting. Body-backed entry; helper phase, no credit.
    pub fn set_pose_named<B: LiveControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        frame_name: &[u8],
        pose: PoseId,
    ) -> Result<(), ControllerError<B::Error>> {
        self.pose_common_guard()?;
        if frame_name.contains(&0) {
            return Err(ControllerError::EmbeddedNameTerminator);
        }
        let Some((_, key)) = backend
            .first_frame_named(frame_name)
            .map_err(ControllerError::Resource)?
        else {
            return Err(ControllerError::InvalidResourceIdentity);
        };
        let id = self.get_frame_interpolator(backend, key)?;
        if let Some(e) = self.owned.iter_mut().find(|e| e.id == id) {
            e.pose = Some(pose);
        }
        self.state.dirty = true;
        Ok(())
    }
    /// RVA 0x342b0 application (via the native per-interpolator loop):
    /// Upstream working name, not an evidenced original symbol.
    /// bit-exact store to every interpolator's +8, then changed=1. Null set
    /// is the caller's OBJECTNOTFOUND (no effect). Explicit precision
    /// policy: raw f32 bits preserved exactly — NO [0,1] clamp (unlike
    /// set_anim_blend's ordered x87 clamp) and no NaN quieting evidenced
    /// on this path. Closed/poisoned/detached views are rejected (host-only
    /// guards; native has no such branches, so they surface as Err, never
    /// as words — same rule as all fallible pose paths).
    pub fn set_pose_blend_all(
        &mut self,
        blend_bits: u32,
    ) -> Result<(), ControllerError<std::convert::Infallible>> {
        self.pose_common_guard()?;
        let blend = f32::from_bits(blend_bits);
        // Native iterates the ACTIVE interpolator list (0x1e8c0); detached
        // host-only evaluators are skipped exactly like missing ones.
        let active = self.active_evaluators().to_vec();
        for e in &mut self.owned {
            if active.contains(&e.id) {
                e.store_pose_blend_bits(blend.to_bits());
            }
        }
        self.state.dirty = true;
        Ok(())
    }
    /// RVA 0x342e0 application: changed=1 FIRST, then per-entry +8 stores
    /// Upstream working name, not an evidenced original symbol.
    /// in set order; miss keeps dirty+prefix (no rollback). Differential
    /// (0x1e900 body 94B/43insn/8BB); this worker is the credited behavior.
    pub fn set_pose_blend_set<B: LiveControllerResourceBackend>(
        &mut self,
        backend: &mut B,
        blend_bits: u32,
        entries: &[PoseSetEntry],
    ) -> Result<(), ControllerError<B::Error>> {
        self.pose_common_guard()?;
        self.state.dirty = true;
        // Native interleaved order (0x1e900): per entry, resolve THEN apply
        // (+8 store); a miss keeps dirty, prefix stores and allocations.
        for entry in entries {
            let ids = self.resolve_pose_entries(backend, std::slice::from_ref(entry))?;
            let id = ids.into_iter().next().unwrap();
            let blend = f32::from_bits(blend_bits);
            if let Some(e) = self.owned.iter_mut().find(|e| e.id == id) {
                e.store_pose_blend_bits(blend.to_bits());
            }
        }
        Ok(())
    }
    /// RVA 0x1e960 application: first interpolator whose pose word is set
    /// (+4 != 0, i.e. `pose.is_some()`), else +0.0. Upstream working name,
    /// not an evidenced original symbol. Bit-exact read-back; null/empty/
    /// closed -> 0x00000000. Differential (20 asm / 5 BB). Corrects the
    /// earlier binding-flags proxy (flags belong to animation slots, not to
    /// the pose word the body actually tests).
    pub fn pose_blend(&self) -> u32 {
        if self.closed {
            return 0x0000_0000;
        }
        for id in self.active_evaluators() {
            if let Some(e) = self.evaluator(*id) {
                if e.pose.is_some() {
                    return e.pose_blend.to_bits();
                }
            }
        }
        0x0000_0000
    }
    /// LS3DF RVA 0x1e9a0 `I3D_IPolSet::ResetAllPoses` (differential: 25 asm /
    /// 7 BB). Per-entry pose word != 0 → clear pose; unconditional trailing
    /// dirty. Clocks/weights/modes/bindings and `pose_blend` words are
    /// untouched. Release traffic needs a `PoseBackend` — use
    /// `reset_all_poses_seam` when one is available; this word-only form
    /// documents the gap instead of inventing retain/release calls.
    /// Upstream name, not an evidenced original symbol.
    pub fn reset_all_poses(&mut self) {
        // Native iterates the ACTIVE list (0x1e9a0); see blend-all note.
        let active = self.active_evaluators().to_vec();
        for evaluator in &mut self.owned {
            if active.contains(&evaluator.id) {
                evaluator.pose = None;
            }
        }
        self.state.dirty = true;
    }
    /// Backend-threaded `ResetAllPoses`: dirty FIRST (each native entry goes
    /// through `SetPose(NULL)`, which stores dirty before its Release), then
    /// per-entry releases in index order with the word clears. Order:
    /// dirty, releases, clears.
    pub fn reset_all_poses_seam<B: PoseBackend>(&mut self, backend: &mut B) {
        self.state.dirty = true;
        let active = self.active_evaluators().to_vec();
        for evaluator in &mut self.owned {
            if !active.contains(&evaluator.id) {
                continue;
            }
            // Release-while-word-holds-old (same order as
            // set_interpolator_pose): a re-entrant backend observes the old
            // pointer, never an intermediate None.
            if let Some(old) = evaluator.pose {
                backend.release_pose(old);
            }
            evaluator.pose = None;
        }
    }

    pub fn storage_bytes(&self) -> Option<usize> {
        self.storage
            .capacity()
            .checked_mul(std::mem::size_of::<EvaluatorId>())?
            .checked_add(
                self.owned
                    .capacity()
                    .checked_mul(std::mem::size_of::<ControllerEvaluator>())?,
            )?
            .checked_add(std::mem::size_of::<Self>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, Default)]
    struct Resources {
        next: u32,
        events: Vec<(u32, u32)>,
        fail: Option<u32>,
        fail_storage: bool,
    }
    impl ControllerResourceBackend for Resources {
        type Error = ();
        fn allocate_evaluator(&mut self) -> Result<EvaluatorId, ()> {
            self.next += 1;
            if self.fail == Some(self.next) {
                self.events.push((8, self.next));
                return Err(());
            }
            self.events.push((1, self.next));
            Ok(EvaluatorId(self.next))
        }
        fn free_evaluator(&mut self, id: EvaluatorId) {
            self.events.push((2, id.0));
        }
        fn allocate_storage(&mut self, n: usize) -> Result<AllocationId, ()> {
            self.events.push((3, n as u32));
            if self.fail_storage {
                return Err(());
            }
            Ok(AllocationId(n as u32))
        }
        fn free_storage(&mut self, id: AllocationId) {
            self.events.push((4, id.0));
        }
        fn retain_animation(&mut self, id: AnimSetId) {
            self.events.push((5, id.0));
        }
        fn release_animation(&mut self, id: AnimSetId) {
            self.events.push((6, id.0));
        }
        fn mark_animated(&mut self, id: FrameKey) {
            self.events.push((7, id.0));
        }
    }
    fn model() -> Model {
        Model {
            materials: vec![],
            frames: [b"A".as_slice(), b"B".as_slice(), b"A".as_slice()]
                .into_iter()
                .map(|name| lhr_assets::Frame {
                    kind: 10,
                    visual: None,
                    render_flags: 0,
                    logic_flags: 0,
                    culling: 0,
                    name: String::from_utf8_lossy(name).into_owned(),
                    name_bytes: name.to_vec(),
                    properties: vec![],
                    dummy_bounds: None,
                    light: None,
                    joint: None,
                    target: None,
                    occluder: None,
                    sector: None,
                    skin: None,
                    morph: None,
                    lensflare: None,
                    parent: None,
                    position: [0.; 3],
                    scale: [1.; 3],
                    rotation_wxyz: [1., 0., 0., 0.],
                    instance: None,
                    lods: vec![],
                })
                .collect(),
        }
    }
    fn resource(id: u32, tracks: &[(&[u8], u16)]) -> AnimationResource {
        AnimationResource {
            id: AnimSetId(id),
            default_mode: 2,
            source: Arc::new(AnimationSet {
                timestamp: [0; 8],
                end_frame: 10,
                tracks: tracks
                    .iter()
                    .map(|(n, f)| lhr_assets::animation::AnimationTrack {
                        name: n.to_vec(),
                        flags: *f,
                        reserved: 0,
                        rotation: None,
                        position: None,
                        scale: None,
                    })
                    .collect(),
            }),
        }
    }
    const KEYS: [FrameKey; 3] = [FrameKey(11), FrameKey(12), FrameKey(13)];

    #[test]
    fn replacement_restart_and_reference_order() {
        let (mut c, mut b, m) = (
            IPolSet::new(FrameKey(1), false),
            Resources::default(),
            model(),
        );
        let a = resource(1, &[(b"A", 6), (b"B", 2), (b"A", 4)]);
        let other = resource(2, &[(b"B", 4)]);
        assert_eq!(Arc::strong_count(&a.source), 1);
        c.set_animation(&mut b, &m, &KEYS, Some(&a), 0, 1., 0)
            .unwrap();
        assert_eq!(c.active_evaluators(), &[EvaluatorId(1), EvaluatorId(2)]);
        assert_eq!(
            c.evaluator(EvaluatorId(1)).unwrap().destination,
            FrameKey(11)
        );
        assert_eq!(
            c.evaluator(EvaluatorId(1)).unwrap().bindings[0].track_index,
            2
        );
        assert_eq!(Arc::strong_count(&a.source), 2);
        c.state.slots[0].last_delta = 23;
        c.state.slots[0].current = 160;
        let before = b.events.len();
        c.set_animation(&mut b, &m, &KEYS, Some(&a), 0, 0.5, 1)
            .unwrap();
        assert_eq!(b.events.len(), before);
        assert_eq!(
            (
                c.state.slots[0].current,
                c.state.slots[0].previous,
                c.state.slots[0].last_delta
            ),
            (0, -40, 23)
        );
        c.set_animation(&mut b, &m, &KEYS, Some(&other), 0, -2., 0)
            .unwrap();
        assert_eq!(&b.events[before..], &[(6, 1), (5, 2)]);
        assert_eq!(c.evaluator(EvaluatorId(1)).unwrap().bindings[0].flags, 0);
        assert_eq!(c.state.slots[0].weight, -2.);
        assert_eq!(Arc::strong_count(&a.source), 1);
        c.set_animation(&mut b, &m, &KEYS, None, 0, 1., 0).unwrap();
        assert_eq!(c.active_evaluators(), &[]);
        assert_eq!(&b.events[b.events.len() - 3..], &[(6, 2), (2, 2), (2, 1)]);
        assert!(c.state.dirty);
        c.close(&mut b);
        assert_eq!(b.events.last(), Some(&(4, 32)));
    }
    #[test]
    fn shared_slots_stop_resume_and_control_setters() {
        let (mut c, mut b, m) = (
            IPolSet::new(FrameKey(1), false),
            Resources::default(),
            model(),
        );
        let a = resource(1, &[(b"A", 2)]);
        for slot in [0, 7] {
            c.set_animation(&mut b, &m, &KEYS, Some(&a), slot, 1., 0)
                .unwrap();
        }
        assert_eq!(Arc::strong_count(&a.source), 3);
        c.state.dirty = false;
        c.set_anim_time(0, 80).unwrap();
        c.state.dirty = false;
        c.stop_animation(0);
        c.start_animation(0);
        assert!(!c.state.dirty);
        assert_eq!(c.anim_time(0), 80);
        c.set_anim_options(0, 3).unwrap();
        assert!(!c.state.dirty);
        c.set_anim_blend(0, f32::from_bits(0x7fc12345)).unwrap();
        assert_eq!(c.anim_blend(0).to_bits(), 0x7fc12345);
        c.set_anim_blend(0, -0.).unwrap();
        assert_eq!(c.anim_blend(0).to_bits(), 0x80000000);
        c.set_anim_blend(0, 2.).unwrap();
        assert_eq!(c.anim_blend(0), 1.);
        c.set_anim_blend(0, -2.).unwrap();
        assert_eq!(c.anim_blend(0), 0.);
        c.close(&mut b);
        assert_eq!(Arc::strong_count(&a.source), 1);
        assert_eq!(b.events.iter().filter(|x| x.0 == 6).count(), 2);
    }
    #[test]
    fn cleanup_truncates_prefix_and_close_reclaims_detached() {
        let (mut c, mut b) = (IPolSet::new(FrameKey(1), false), Resources::default());
        for k in KEYS {
            c.get_frame_interpolator(&mut b, k).unwrap();
        }
        c.owned[2].bindings[0].flags = 4;
        c.release_unused_interpolators(&mut b);
        assert_eq!(c.state.evaluators.count, 0);
        assert_eq!(c.owned_evaluator_count(), 1);
        assert_eq!(&b.events[b.events.len() - 2..], &[(2, 2), (2, 1)]);
        let new = c.get_frame_interpolator(&mut b, FrameKey(13)).unwrap();
        assert_eq!(new, EvaluatorId(4));
        c.clear(&mut b);
        assert_eq!(c.owned_evaluator_count(), 1);
        c.close(&mut b);
        assert_eq!(&b.events[b.events.len() - 3..], &[(2, 4), (2, 3), (4, 32)]);
        let before = b.events.len();
        c.close(&mut b);
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn binding_failure_preserves_actual_prefix_and_reference() {
        let (mut c, mut b, m) = (
            IPolSet::new(FrameKey(1), false),
            Resources {
                fail: Some(2),
                ..Default::default()
            },
            model(),
        );
        let a = resource(1, &[(b"A", 2), (b"B", 4)]);
        assert_eq!(
            c.set_animation(&mut b, &m, &KEYS, Some(&a), 0, 1., 0),
            Err(ControllerError::Resource(()))
        );
        assert_eq!(c.active_evaluators(), &[EvaluatorId(1)]);
        assert!(c.state.slots[0].enabled);
        assert!(!c.state.dirty);
        assert_eq!(c.state.slots[0].previous, -40);
        assert_eq!(Arc::strong_count(&a.source), 2);
        c.close(&mut b);
        assert_eq!(Arc::strong_count(&a.source), 1);
    }
    #[test]
    fn grows_in_native_chunks_and_clear_preserves_storage_and_dirty() {
        let (mut c, mut b) = (IPolSet::new(FrameKey(1), true), Resources::default());
        for k in 1..=33 {
            c.get_frame_interpolator(&mut b, FrameKey(k)).unwrap();
        }
        assert_eq!(c.evaluator_capacity(), 64);
        assert_eq!(c.state.evaluators.allocation, AllocationId(64));
        assert_eq!(
            b.events
                .iter()
                .filter(|x| x.0 == 3)
                .copied()
                .collect::<Vec<_>>(),
            vec![(3, 32), (3, 64)]
        );
        c.clear(&mut b);
        assert!(c.state.dirty);
        assert_eq!(c.evaluator_capacity(), 64);
        assert_eq!(c.state.evaluators.allocation, AllocationId(64));
        c.close(&mut b);
    }
    #[test]
    fn corrupted_tick_view_is_rejected_and_unreadable_native_storage_failure_is_poisoned() {
        let (mut c, mut b) = (IPolSet::new(FrameKey(1), false), Resources::default());
        c.get_frame_interpolator(&mut b, FrameKey(11)).unwrap();
        c.state_mut().evaluators.count = usize::MAX;
        assert!(!c.owns_view());
        assert_eq!(c.active_evaluators(), &[EvaluatorId(1)]);
        assert_eq!(
            c.get_frame_interpolator(&mut b, FrameKey(12)),
            Err(ControllerError::InvalidView)
        );
        c.close(&mut b);
        assert_eq!(&b.events[b.events.len() - 2..], &[(2, 1), (4, 32)]);
        let (mut c, mut b) = (
            IPolSet::new(FrameKey(1), false),
            Resources {
                fail_storage: true,
                ..Default::default()
            },
        );
        assert_eq!(
            c.get_frame_interpolator(&mut b, FrameKey(11)),
            Err(ControllerError::Resource(()))
        );
        assert!(c.active_evaluators().is_empty());
        assert_eq!(c.owned_evaluator_count(), 1);
        assert_eq!(
            c.get_frame_interpolator(&mut b, FrameKey(11)),
            Err(ControllerError::Poisoned)
        );
        c.close(&mut b);
        assert_eq!(&b.events[b.events.len() - 2..], &[(2, 1), (4, 0)]);
    }
    #[test]
    fn native_weight_loads_quiet_signaling_nans_and_getter_preserves_storage() {
        let (mut c, mut b, m) = (
            IPolSet::new(FrameKey(1), false),
            Resources::default(),
            model(),
        );
        let a = resource(1, &[(b"A", 2)]);
        let cases = [
            (0x7f81_2345, 0x7fc1_2345, 0x7fc1_2345),
            (0xff81_2345, 0xffc1_2345, 0xffc1_2345),
            (0x7fc1_2345, 0x7fc1_2345, 0x7fc1_2345),
            (0xffc1_2345, 0xffc1_2345, 0xffc1_2345),
            (0x7f80_0000, 0x7f80_0000, 0x3f80_0000),
            (0xff80_0000, 0xff80_0000, 0),
            (0x8000_0000, 0x8000_0000, 0x8000_0000),
            (0, 0, 0),
        ];
        for (input, loaded, clamped) in cases {
            c.set_animation(&mut b, &m, &KEYS, Some(&a), 0, f32::from_bits(input), 0)
                .unwrap();
            assert_eq!(c.state().slots[0].weight.to_bits(), loaded);
            c.set_anim_blend(0, f32::from_bits(input)).unwrap();
            assert_eq!(c.state().slots[0].weight.to_bits(), clamped);
            c.state_mut().slots[0].weight = f32::from_bits(input);
            assert_eq!(c.anim_blend(0).to_bits(), loaded);
            assert_eq!(c.state().slots[0].weight.to_bits(), input);
        }
        let before = c.state().slots[0].weight.to_bits();
        assert_eq!(
            c.set_anim_blend(8, f32::from_bits(0x7f81_2345)),
            Err(ControllerError::InvalidSlot)
        );
        assert_eq!(c.state().slots[0].weight.to_bits(), before);
        assert_eq!(c.anim_blend(8).to_bits(), 0);
        c.close(&mut b);
    }

    #[test]
    fn live_binding_rereads_names_membership_and_shared_default_after_effects() {
        struct LiveBackend {
            allocator: Resources,
            members: Vec<(FrameKey, Vec<u8>)>,
            resource: LiveAnimationResource,
            allocations: usize,
        }
        impl ControllerResourceBackend for LiveBackend {
            type Error = ();
            fn allocate_evaluator(&mut self) -> Result<EvaluatorId, ()> {
                self.allocations += 1;
                if self.allocations == 1 {
                    self.members[1].1 = b"two".to_vec();
                    self.members.swap(0, 1);
                }
                self.allocator.allocate_evaluator()
            }
            fn free_evaluator(&mut self, id: EvaluatorId) {
                self.allocator.free_evaluator(id);
            }
            fn allocate_storage(&mut self, capacity: usize) -> Result<AllocationId, ()> {
                self.allocator.allocate_storage(capacity)
            }
            fn free_storage(&mut self, id: AllocationId) {
                self.allocator.free_storage(id);
            }
            fn retain_animation(&mut self, id: AnimSetId) {
                self.members[0].1 = b"one".to_vec();
                self.resource.set_default_mode(7);
                self.allocator.retain_animation(id);
            }
            fn release_animation(&mut self, id: AnimSetId) {
                self.allocator.release_animation(id);
            }
            fn mark_animated(&mut self, destination: FrameKey) {
                self.allocator.mark_animated(destination);
            }
        }
        impl LiveControllerResourceBackend for LiveBackend {
            fn first_frame_named(&mut self, name: &[u8]) -> Result<Option<(usize, FrameKey)>, ()> {
                Ok(self
                    .members
                    .iter()
                    .enumerate()
                    .find(|(_, (_, local))| local == name)
                    .map(|(index, (key, _))| (index, *key)))
            }
        }
        let parsed = resource(1, &[(b"one", 2), (b"two", 4)]).source;
        let resource = LiveAnimationResource::new(Arc::clone(&parsed), 2).unwrap();
        let independent = LiveAnimationResource::new(parsed, 99).unwrap();
        assert_ne!(resource.id(), independent.id());
        let mut backend = LiveBackend {
            allocator: Resources::default(),
            members: vec![(KEYS[0], b"before".to_vec()), (KEYS[1], b"before".to_vec())],
            resource: resource.clone(),
            allocations: 0,
        };
        let mut controller = IPolSet::new(FrameKey(1), false);
        controller
            .set_live_animation(&mut backend, Some(&resource), 0, 1., 0)
            .unwrap();
        assert_eq!(controller.anim_options(0), 7);
        assert_eq!(resource.default_mode(), 7);
        assert_eq!(independent.default_mode(), 99);
        let evaluators = controller.active_evaluators();
        let first = controller.evaluator(evaluators[0]).unwrap();
        let second = controller.evaluator(evaluators[1]).unwrap();
        assert_eq!(
            (first.destination, first.bindings[0].frame_index),
            (KEYS[0], 0)
        );
        assert_eq!(
            (second.destination, second.bindings[0].frame_index),
            (KEYS[1], 0)
        );
        // A same-object restart reads the resource's current mode without a
        // second retain. A separate resource with identical channels replaces it.
        resource.set_default_mode(3);
        controller
            .set_live_animation(&mut backend, Some(&resource), 0, 1., 0)
            .unwrap();
        assert_eq!(controller.anim_options(0), 3);
        controller
            .set_live_animation(&mut backend, Some(&independent), 0, 1., 0)
            .unwrap();
        assert_eq!(controller.animation_id(0), Some(independent.id()));
        assert_eq!(controller.anim_options(0), 99);
        controller.close(&mut backend);
    }
    #[test]
    fn pose_count_storage_blend_bit_exactness_and_targeting_matrix() {
        // RVAs 0x34090/0x340b0/0x342b0/0x34310: count/storage words,
        // bit-exact blend (no clamp, no quieting), empty read-back.
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Resources::default();
        assert_eq!(controller.num_interpolators(), 0);
        assert!(controller.interpolator_storage().unwrap().is_empty());
        assert_eq!(controller.pose_blend(), 0x0000_0000);
        assert!(!controller.state().dirty);
        // Bit-exactness incl. values SetAnimBlend would clamp/quiet.
        for bits in [
            0x7fc1_2345u32,
            0x7f81_2345,
            0xffc1_2345,
            0x7f80_0000,
            0xff80_0000,
            0x8000_0000,
            0x0000_0000,
            0x3f80_0000,
            0xc000_0000,
        ] {
            controller
                .get_frame_interpolator(&mut backend, FrameKey(11))
                .unwrap();
            // Eligibility for the native +4!=0 word: the evaluator pose word
            // (RVA 0x1e960 correction — binding flags are the wrong proxy).
            controller.owned[0].pose = Some(PoseId(77));
            controller.set_pose_blend_all(bits).unwrap();
            assert_eq!(controller.pose_blend(), bits, "bits {bits:#x}");
            assert!(controller.state().dirty);
            controller.clear(&mut backend);
        }
        // Width admission: hostile views saturate, never wrap the word.
        controller.state_mut().evaluators.count = usize::MAX;
        assert_eq!(controller.num_interpolators(), u32::MAX);
        controller.state_mut().evaluators.count = 0;
        // Closed views read as absent (native NULL analog).
        controller.close(&mut backend);
        assert_eq!(controller.interpolator_storage(), None);
        assert_eq!(controller.num_interpolators(), 0);
        assert_eq!(controller.pose_blend(), 0x0000_0000);
    }
    #[test]
    fn pose_entry_loops_resolve_in_order_with_prefix_kept_on_miss() {
        // RVAs 0x340d0/0x34170/0x34210/0x342e0: resolve order, miss words,
        // dirty-first rule for blend-set.
        struct Named(Resources);
        impl ControllerResourceBackend for Named {
            type Error = ();
            fn allocate_evaluator(&mut self) -> Result<EvaluatorId, ()> {
                self.0.allocate_evaluator()
            }
            fn free_evaluator(&mut self, id: EvaluatorId) {
                self.0.free_evaluator(id)
            }
            fn allocate_storage(&mut self, capacity: usize) -> Result<AllocationId, ()> {
                self.0.allocate_storage(capacity)
            }
            fn free_storage(&mut self, id: AllocationId) {
                self.0.free_storage(id)
            }
            fn retain_animation(&mut self, id: AnimSetId) {
                self.0.retain_animation(id)
            }
            fn release_animation(&mut self, id: AnimSetId) {
                self.0.release_animation(id)
            }
            fn mark_animated(&mut self, destination: FrameKey) {
                self.0.mark_animated(destination)
            }
        }
        impl LiveControllerResourceBackend for Named {
            fn first_frame_named(&mut self, name: &[u8]) -> Result<Option<(usize, FrameKey)>, ()> {
                Ok((name == b"A").then_some((0, FrameKey(11))))
            }
        }
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Named(Resources::default());
        // Hit via set/frame/named paths alike.
        controller
            .set_pose_named(&mut backend, b"A", PoseId(1))
            .unwrap();
        assert_eq!(controller.num_interpolators(), 1);
        assert_eq!(controller.interpolator_storage().unwrap().len(), 1);
        // Miss -> InvalidResourceIdentity (0x80000006 word); prefix kept.
        assert_eq!(
            controller.set_pose_named(&mut backend, b"missing", PoseId(1)),
            Err(ControllerError::InvalidResourceIdentity)
        );
        assert_eq!(controller.num_interpolators(), 1);
        controller
            .set_pose_frame(&mut backend, FrameKey(12), PoseId(2))
            .unwrap();
        assert_eq!(controller.num_interpolators(), 2);
        controller
            .set_pose_set(
                &mut backend,
                &[PoseSetEntry {
                    frame_name: b"A".to_vec(),
                    pose: PoseId(3),
                }],
            )
            .unwrap();
        assert!(controller.state().dirty);
        // Dirty-first order for blend-set: a miss keeps dirty set.
        controller.state_mut().dirty = false;
        assert_eq!(
            controller.set_pose_blend_set(
                &mut backend,
                0x3f80_0000,
                &[PoseSetEntry {
                    frame_name: b"missing".to_vec(),
                    pose: PoseId(9),
                }],
            ),
            Err(ControllerError::InvalidResourceIdentity)
        );
        assert!(controller.state().dirty);
        // Failure prefix: embedded NUL rejected before any effect.
        let before = (controller.num_interpolators(), controller.state().dirty);
        assert_eq!(
            controller.set_pose_named(&mut backend, b"A ", PoseId(1)),
            Err(ControllerError::EmbeddedNameTerminator)
        );
        assert_eq!(
            (controller.num_interpolators(), controller.state().dirty),
            before
        );
        controller.close(&mut backend);
    }
    #[test]
    fn reset_all_poses_clears_pose_words_and_dirties_keeping_rest() {
        // RVA 0x1e9a0: per-entry pose clear + unconditional trailing dirty;
        // blends, slot clocks, modes, bindings untouched.
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Resources::default();
        controller
            .get_frame_interpolator(&mut backend, FrameKey(11))
            .unwrap();
        controller.owned[0].pose = Some(PoseId(5));
        controller.owned[0].pose_blend = 0.25;
        controller.state_mut().slots[0].current = 42;
        controller.state_mut().dirty = false;
        controller.reset_all_poses();
        assert!(controller.owned.iter().all(|e| e.pose.is_none()));
        assert_eq!(
            controller
                .owned
                .iter()
                .map(|e| e.pose_blend)
                .collect::<Vec<_>>(),
            vec![0.25; controller.owned.len()]
        );
        assert_eq!(controller.state().slots[0].current, 42);
        assert!(controller.state().dirty);
        controller.close(&mut backend);
    }
    #[test]
    fn interpolator_pose_word_store_and_same_ptr_noop() {
        // RVA 0x1e230: same-pointer no-op keeps dirty clear; else dirty
        // first, release-old, store, retain-new.
        struct PLog {
            ops: Vec<&'static str>,
        }
        impl PoseBackend for PLog {
            fn retain_pose(&mut self, _: PoseId) {
                self.ops.push("retain");
            }
            fn release_pose(&mut self, _: PoseId) {
                self.ops.push("release");
            }
        }
        let mut e = ControllerEvaluator {
            id: EvaluatorId(1),
            destination: FrameKey(1),
            bindings: [ControllerBinding::default(); 8],
            pose_blend: 1.0,
            pose: None,
        };
        let mut b = PLog { ops: vec![] };
        let mut dirty = false;
        assert!(!e.set_interpolator_pose(&mut b, None, &mut dirty));
        assert!(b.ops.is_empty() && !dirty);
        assert!(e.set_interpolator_pose(&mut b, Some(PoseId(7)), &mut dirty));
        assert!(dirty && b.ops == ["retain"]);
        dirty = false;
        assert!(!e.set_interpolator_pose(&mut b, Some(PoseId(7)), &mut dirty));
        assert!(!dirty && b.ops.len() == 1);
        assert!(e.set_interpolator_pose(&mut b, None, &mut dirty));
        assert_eq!(b.ops, ["retain", "release"]);
        assert_eq!(e.pose, None);
    }
    #[test]
    fn pose_blend_bits_and_slot_clear_words() {
        // RVAs 0x1d2f0/0x1d300: bit-exact store incl. NaN payloads; the slot
        // clear zeroes only the flags word and returns I3D_OK.
        let mut e = ControllerEvaluator {
            id: EvaluatorId(2),
            destination: FrameKey(3),
            bindings: [ControllerBinding::default(); 8],
            pose_blend: 1.0,
            pose: None,
        };
        for bits in [
            0x0000_0000u32,
            0x3F80_0000,
            0x7FC0_0001,
            0xFFC0_0001,
            0x7F80_0000,
        ] {
            e.store_pose_blend_bits(bits);
            assert_eq!(e.pose_blend.to_bits(), bits);
        }
        e.bindings[2] = ControllerBinding {
            track_index: 5,
            frame_index: 7,
            flags: 0x2F,
        };
        assert_eq!(e.clear_slot_flags(2).unwrap(), 0);
        assert_eq!(
            e.bindings[2],
            ControllerBinding {
                track_index: 5,
                frame_index: 7,
                flags: 0,
            }
        );
        assert!(e.clear_slot_flags(8).is_err());
    }
    #[test]
    fn pose_blend_eligibility_is_pose_word_not_flags() {
        // RVA 0x1e960 correction: flags without a pose read +0.0; a pose
        // without flags reads exact bits.
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Resources::default();
        controller
            .get_frame_interpolator(&mut backend, FrameKey(11))
            .unwrap();
        controller.owned[0].bindings[0].flags = 0xFFFF;
        controller.owned[0].pose_blend = 0.5;
        assert_eq!(controller.pose_blend(), 0x0000_0000);
        controller.owned[0].pose = Some(PoseId(3));
        assert_eq!(controller.pose_blend(), 0.5f32.to_bits());
        controller.close(&mut backend);
    }
    #[test]
    fn reset_all_poses_seam_dirties_first_then_releases_in_order() {
        // RVA 0x1e9a0 seam: dirty first (each entry's SetPose(NULL) dirties
        // before its Release), then per-entry releases in index order;
        // blends and slots bit-identical afterwards.
        struct PLog {
            ops: Vec<u32>,
        }
        impl PoseBackend for PLog {
            fn retain_pose(&mut self, _: PoseId) {}
            fn release_pose(&mut self, p: PoseId) {
                self.ops.push(p.0);
            }
        }
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Resources::default();
        controller
            .get_frame_interpolator(&mut backend, FrameKey(11))
            .unwrap();
        controller
            .get_frame_interpolator(&mut backend, FrameKey(12))
            .unwrap();
        controller.owned[0].pose = Some(PoseId(9));
        controller.owned[1].pose = Some(PoseId(4));
        controller.owned[0].pose_blend = 0.25;
        controller.state_mut().dirty = false;
        let mut pb = PLog { ops: vec![] };
        controller.reset_all_poses_seam(&mut pb);
        assert_eq!(pb.ops, [9, 4]);
        assert!(controller.owned.iter().all(|e| e.pose.is_none()));
        assert_eq!(controller.owned[0].pose_blend, 0.25);
        assert!(controller.state().dirty);
        controller.close(&mut backend);
    }
    #[test]
    fn named_getter_miss_has_no_effects_and_scan_hits_by_identity() {
        // RVA 0x1eba0: miss resolves to None with no allocation and dirty
        // untouched; a second hit reuses the worker identity scan.
        struct Miss(Resources);
        impl ControllerResourceBackend for Miss {
            type Error = ();
            fn allocate_evaluator(&mut self) -> Result<EvaluatorId, ()> {
                self.0.allocate_evaluator()
            }
            fn free_evaluator(&mut self, id: EvaluatorId) {
                self.0.free_evaluator(id)
            }
            fn allocate_storage(&mut self, capacity: usize) -> Result<AllocationId, ()> {
                self.0.allocate_storage(capacity)
            }
            fn free_storage(&mut self, id: AllocationId) {
                self.0.free_storage(id)
            }
            fn retain_animation(&mut self, id: AnimSetId) {
                self.0.retain_animation(id)
            }
            fn release_animation(&mut self, id: AnimSetId) {
                self.0.release_animation(id)
            }
            fn mark_animated(&mut self, destination: FrameKey) {
                self.0.mark_animated(destination)
            }
        }
        impl LiveControllerResourceBackend for Miss {
            fn first_frame_named(&mut self, _: &[u8]) -> Result<Option<(usize, FrameKey)>, ()> {
                Ok(None)
            }
        }
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Miss(Resources::default());
        assert_eq!(
            controller
                .get_frame_interpolator_named(&mut backend, b"missing")
                .unwrap(),
            None
        );
        assert!(!controller.state().dirty);
        assert_eq!(controller.num_interpolators(), 0);
        controller.close(&mut backend);
    }
    #[test]
    fn multi_entry_prefix_applies_before_miss_without_rollback() {
        // 0x1e860/0x1e900 interleave: [hit, miss] applies entry0 (pose word
        // + blend-set dirty header) then returns OBJECTNOTFOUND with the
        // prefix kept; set_pose_set leaves dirty untouched.
        struct TwoHit(Resources);
        impl ControllerResourceBackend for TwoHit {
            type Error = ();
            fn allocate_evaluator(&mut self) -> Result<EvaluatorId, ()> {
                self.0.allocate_evaluator()
            }
            fn free_evaluator(&mut self, id: EvaluatorId) {
                self.0.free_evaluator(id)
            }
            fn allocate_storage(&mut self, capacity: usize) -> Result<AllocationId, ()> {
                self.0.allocate_storage(capacity)
            }
            fn free_storage(&mut self, id: AllocationId) {
                self.0.free_storage(id)
            }
            fn retain_animation(&mut self, id: AnimSetId) {
                self.0.retain_animation(id)
            }
            fn release_animation(&mut self, id: AnimSetId) {
                self.0.release_animation(id)
            }
            fn mark_animated(&mut self, destination: FrameKey) {
                self.0.mark_animated(destination)
            }
        }
        impl LiveControllerResourceBackend for TwoHit {
            fn first_frame_named(&mut self, name: &[u8]) -> Result<Option<(usize, FrameKey)>, ()> {
                Ok((name == b"A").then_some((0, FrameKey(11))))
            }
        }
        let entries = [
            PoseSetEntry {
                frame_name: b"A".to_vec(),
                pose: PoseId(21),
            },
            PoseSetEntry {
                frame_name: b"missing".to_vec(),
                pose: PoseId(22),
            },
        ];
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = TwoHit(Resources::default());
        assert_eq!(
            controller.set_pose_set(&mut backend, &entries),
            Err(ControllerError::InvalidResourceIdentity)
        );
        let id = controller
            .active_evaluators()
            .iter()
            .copied()
            .find(|id| controller.evaluator(*id).unwrap().destination == FrameKey(11))
            .unwrap();
        assert_eq!(controller.evaluator(id).unwrap().pose, Some(PoseId(21)));
        assert!(!controller.state().dirty);
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = TwoHit(Resources::default());
        assert_eq!(
            controller.set_pose_blend_set(&mut backend, 0x3f80_0000, &entries),
            Err(ControllerError::InvalidResourceIdentity)
        );
        let id = controller
            .active_evaluators()
            .iter()
            .copied()
            .find(|id| controller.evaluator(*id).unwrap().destination == FrameKey(11))
            .unwrap();
        assert_eq!(
            controller.evaluator(id).unwrap().pose_blend.to_bits(),
            0x3f80_0000
        );
        assert!(controller.state().dirty);
        controller.close(&mut backend);
    }
    #[test]
    fn sweeper_keeps_pose_holding_evaluators() {
        // 0x1e9e0 pose arm: the sweeper skips pose-holding interpolators
        // even with no animations and clear flags.
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Resources::default();
        let id = controller
            .get_frame_interpolator(&mut backend, FrameKey(11))
            .unwrap();
        controller.owned[0].pose = Some(PoseId(77));
        controller.release_unused_interpolators(&mut backend);
        assert_eq!(controller.num_interpolators(), 1);
        assert_eq!(controller.evaluator(id).unwrap().pose, Some(PoseId(77)));
        controller.close(&mut backend);
    }
    #[test]
    fn detached_evaluators_are_skipped_by_blend_and_reset() {
        // Active-only property: a host-only owned entry outside the active
        // list keeps its pose word and blend through blend-all/reset/seam.
        struct PLog {
            ops: Vec<u32>,
        }
        impl PoseBackend for PLog {
            fn retain_pose(&mut self, _: PoseId) {}
            fn release_pose(&mut self, p: PoseId) {
                self.ops.push(p.0);
            }
        }
        let mut controller = IPolSet::new(FrameKey(1), false);
        let mut backend = Resources::default();
        controller
            .get_frame_interpolator(&mut backend, FrameKey(11))
            .unwrap();
        controller.owned.push(ControllerEvaluator {
            id: EvaluatorId(999),
            destination: FrameKey(12),
            bindings: [ControllerBinding::default(); 8],
            pose_blend: 0.5,
            pose: Some(PoseId(77)),
        });
        controller.set_pose_blend_all(0x3f80_0000).unwrap();
        controller.reset_all_poses();
        let mut pb = PLog { ops: vec![] };
        controller.reset_all_poses_seam(&mut pb);
        let detached = controller
            .owned
            .iter()
            .find(|e| e.id == EvaluatorId(999))
            .unwrap();
        assert_eq!(detached.pose, Some(PoseId(77)));
        assert_eq!(detached.pose_blend.to_bits(), 0.5f32.to_bits());
        assert!(pb.ops.is_empty());
        assert_eq!(controller.num_interpolators(), 1);
        controller.close(&mut backend);
    }
}

#[cfg(test)]
mod update_interpolator_tests {
    use super::*;
    use crate::interpolator::Slot;

    fn evaluator_with(flags: [u16; 8], pose_blend: f32) -> ControllerEvaluator {
        let mut bindings = [ControllerBinding::default(); 8];
        for (binding, flag) in bindings.iter_mut().zip(flags) {
            binding.flags = flag;
        }
        ControllerEvaluator {
            id: EvaluatorId(1),
            destination: FrameKey(11),
            bindings,
            pose_blend,
            pose: None,
        }
    }

    fn slots_with(weights: [f32; 8], enabled: [bool; 8]) -> [Slot; 8] {
        let mut slots = [Slot::default(); 8];
        for (index, slot) in slots.iter_mut().enumerate() {
            slot.weight = weights[index];
            slot.enabled = enabled[index];
        }
        slots
    }

    fn empty_samples() -> [UpdateChannelSample; 8] {
        [UpdateChannelSample {
            position: None,
            rotation: None,
            scale: None,
        }; 8]
    }

    #[test]
    fn update_blends_positions_in_native_order_and_skips_guards() {
        // LS3DF RVA 0x1d320 pos loop (mafia-re I3d_interpolator.cpp:79):
        // flags first, then ordered blend/on, then `*v * b + acc * (1 - b)`.
        let evaluator = evaluator_with([2, 2, 2, 0, 0, 0, 0, 0], 1.0);
        let slots = slots_with(
            [1.0, 0.25, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [true, true, true, true, true, true, true, true],
        );
        let mut samples = empty_samples();
        samples[0].position = Some([8.0, 0.0, 0.0]);
        samples[1].position = Some([0.0, 4.0, 0.0]);
        samples[2].position = Some([99.0, 99.0, 99.0]);
        let outcome = evaluator
            .update_interpolator(&slots, &samples, None)
            .unwrap();
        assert_eq!(outcome.num_pos, 2);
        assert_eq!(outcome.position, Some([6.0, 1.0, 0.0]));
        assert_eq!(outcome.rotation, None);
        assert_eq!(outcome.scale, None);
    }

    #[test]
    fn update_replaces_rotation_on_unit_blend_and_slerps_otherwise() {
        // Rot loop (I3d_interpolator.cpp:122): first sets, second Slerps.
        // Identical inputs stay bit-exact through the linear Slerp arm.
        let evaluator = evaluator_with([4, 4, 0, 0, 0, 0, 0, 0], 1.0);
        let slots = slots_with(
            [1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [true, true, true, true, true, true, true, true],
        );
        let mut samples = empty_samples();
        samples[0].rotation = Some([1.0, 0.0, 0.0, 0.0]);
        samples[1].rotation = Some([1.0, 0.0, 0.0, 0.0]);
        let outcome = evaluator
            .update_interpolator(&slots, &samples, None)
            .unwrap();
        assert_eq!(outcome.num_rot, 2);
        assert_eq!(outcome.rotation, Some([1.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn update_pose_full_replaces_while_blended_pose_adds() {
        // Pose arms (I3d_interpolator.cpp:200-260): full POS replaces the
        // local without touching the count (no store when count is zero);
        // blended POS blends and bumps the count.
        let mut evaluator = evaluator_with([0, 0, 0, 0, 0, 0, 0, 0], 1.0);
        evaluator.pose = Some(PoseId(9));
        let slots = slots_with(
            [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [false, false, false, false, false, false, false, false],
        );
        let pose = UpdatePoseSample {
            flags: crate::animation_pose::POSEF_POS,
            pos: [5.0, 6.0, 7.0],
            scale: [0.0, 0.0, 0.0],
            rot: [1.0, 0.0, 0.0, 0.0],
        };
        let outcome = evaluator
            .update_interpolator(&slots, &empty_samples(), Some(pose))
            .unwrap();
        assert_eq!(outcome.num_pos, 0);
        assert_eq!(outcome.position, None);
        let blended = evaluator_with([2, 0, 0, 0, 0, 0, 0, 0], 0.5);
        let mut slots = slots_with(
            [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [true, false, false, false, false, false, false, false],
        );
        slots[0].weight = 1.0;
        let mut samples = empty_samples();
        samples[0].position = Some([8.0, 0.0, 0.0]);
        let outcome = blended
            .update_interpolator(&slots, &samples, Some(pose))
            .unwrap();
        assert_eq!(outcome.num_pos, 2);
        assert_eq!(outcome.position, Some([6.5, 3.0, 3.5]));
    }

    #[test]
    fn update_nan_weight_is_admitted_by_ordered_comparison() {
        // `!(w <= 0)` admits NaN where `w > 0` would skip it.
        let evaluator = evaluator_with([2, 0, 0, 0, 0, 0, 0, 0], 1.0);
        let mut slots = slots_with(
            [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            [true, true, true, true, true, true, true, true],
        );
        slots[0].weight = f32::NAN;
        let mut samples = empty_samples();
        samples[0].position = Some([1.0, 2.0, 3.0]);
        let outcome = evaluator
            .update_interpolator(&slots, &samples, None)
            .unwrap();
        assert_eq!(outcome.num_pos, 1);
        assert_eq!(outcome.position, Some([1.0, 2.0, 3.0]));
    }
}
