//! Owned PC I3D_IPolSet control and resource lifecycle.
//!
//! LS3DF SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6:
//! SetAnimation 0x1e580, GetFrameInterpolator 0x1ea60, unused cleanup 0x1e9e0,
//! Clear 0x1e2e0. Allocation and resource calls remain explicit dependencies.
//! No pose evaluation, callback reentrancy or native allocator implementation is
//! implied. Cleanup preserves the original count truncation, including detached
//! live allocations; `close` additionally reclaims those through host ownership.

use crate::{
    frame_transform::FrameKey,
    interpolator::{AllocationId, AnimSetId, EvaluatorId, InterpolatorSet, Slot},
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

impl IPolSet {
    pub(crate) fn borrow_tick_parts(
        &mut self,
    ) -> Result<
        (&mut InterpolatorSet, ControllerEvaluationView<'_>),
        ControllerError<std::convert::Infallible>,
    > {
        if self.closed {
            return Err(ControllerError::Closed);
        }
        if self.poisoned {
            return Err(ControllerError::Poisoned);
        }
        if !self.owns_view() {
            return Err(ControllerError::InvalidView);
        }
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
}
