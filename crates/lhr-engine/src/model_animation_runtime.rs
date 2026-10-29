//! Model-owned animation producer and controls. LS3DF SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! SetAnimation RVA0x337b0; model controls0x33860..0x33a60.
//! Original allocation boundaries remain explicit. No Game selection, loader,
//! scene scheduling or multi-slot blending is supplied here. Model Tick admits
//! callback-free position/rotation channels through the existing verified
//! samplers; unsupported work stops at its reached phase without rollback.
#![forbid(unsafe_code)]

use crate::{
    animation_controller::{
        ControllerError, ControllerEvaluationView, ControllerResourceBackend, IPolSet,
        LiveAnimationResource, LiveControllerResourceBackend,
    },
    frame_transform::{FrameKey, I3dFrameGraph},
    interpolator::{
        AllocationId, AnimSetId, EvaluatorId, InterpolatorBackend, InterpolatorSet, TickError,
    },
    model_loading::{LoadedFrame, ModelArena, ModelError, ModelKey},
    model_tick::{tick_live_model, LiveModelTickOwner},
    rotation_animation::sample_rotation_channel,
    scene_tick::FrameId,
    scene_tick::InterpolatorId,
};
use std::convert::Infallible;

pub const ANIMATION_OUT_OF_MEMORY: u32 = 0x8000_0002;

/// Arena-scoped identity of the actual destination owner. Only the arena can
/// create a token; equal numeric ModelKeys from different arenas stay distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelAnimationDestination {
    model: ModelKey,
    reference: crate::script_scheduler::FrameReferenceId,
}
impl ModelAnimationDestination {
    pub fn model(self) -> ModelKey {
        self.model
    }
}

/// Constructor-allocated actual owner. Its Box, IPolSet vectors and shared
/// resources are the allocations; IDs identify them without mirroring state.
#[derive(Debug)]
pub struct ModelControllerAllocation {
    destination: ModelAnimationDestination,
    controller: Box<IPolSet>,
    next_identity: u32,
    /// Host admission guard after an interrupted Tick; not a native field.
    tick_interrupted: bool,
    #[cfg(test)]
    fail_evaluator_after: Option<usize>,
    #[cfg(test)]
    test_callback: Option<TestResourceCallback>,
}
impl ModelControllerAllocation {
    pub fn new(owner: ModelAnimationDestination, allocation_dirty: bool) -> Self {
        Self {
            destination: owner,
            controller: Box::new(IPolSet::new(owner.model.root(), allocation_dirty)),
            next_identity: 0,
            tick_interrupted: false,
            #[cfg(test)]
            fail_evaluator_after: None,
            #[cfg(test)]
            test_callback: None,
        }
    }
    fn close(&mut self) {
        self.controller.close(&mut ReleaseResources);
    }
    pub(crate) fn storage_bytes(&self) -> Option<usize> {
        self.controller.storage_bytes()
    }
}
impl Drop for ModelControllerAllocation {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Debug)]
pub(crate) enum OriginalModelController {
    /// A compatibility import without its resource owner remains unsupported.
    #[allow(dead_code)]
    External(InterpolatorId),
    Owned(ModelControllerAllocation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelControllerAllocationFailure {
    pub hresult: u32,
    pub severity: u32,
    /// The original third ErrorHandler argument is null.
    pub context: Option<()>,
}

/// Invoked only for a model whose controller is absent. Implementations supply
/// the allocation's untouched dirty byte explicitly. Returning None follows
/// the native null-allocation/error-handler path; errors mean the required
/// allocation or handler operation was unavailable/interrupted.
pub trait ModelControllerFactory {
    type Error;
    fn allocate(
        &mut self,
        owner: ModelAnimationDestination,
    ) -> Result<Option<ModelControllerAllocation>, Self::Error>;
    fn allocation_failed(
        &mut self,
        failure: ModelControllerAllocationFailure,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum ModelAnimationResourceError {
    Model(ModelError),
    IdentityExhausted,
    #[cfg(test)]
    NativeEvaluatorAllocation,
}
#[derive(Debug)]
pub enum ModelAnimationCallError<E> {
    Model(ModelError),
    /// Safe-host domain; native SetAnimation has no invalid-slot guard.
    SlotOutsideDomain,
    Factory(E),
    ForeignControllerAllocation,
    Controller(ControllerError<ModelAnimationResourceError>),
}
impl<E> From<ModelError> for ModelAnimationCallError<E> {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

// Actual owner fields perform allocation/reference ownership. These callbacks
// represent destruction order; there is no second refcount or allocation map.
struct ReleaseResources;
impl ControllerResourceBackend for ReleaseResources {
    type Error = Infallible;
    fn allocate_evaluator(&mut self) -> Result<EvaluatorId, Self::Error> {
        unreachable!("close never allocates")
    }
    fn free_evaluator(&mut self, _: EvaluatorId) {}
    fn allocate_storage(&mut self, _: usize) -> Result<AllocationId, Self::Error> {
        unreachable!("close never allocates")
    }
    fn free_storage(&mut self, _: AllocationId) {}
    fn retain_animation(&mut self, _: AnimSetId) {
        unreachable!("close never retains")
    }
    fn release_animation(&mut self, _: AnimSetId) {}
    fn mark_animated(&mut self, _: FrameKey) {
        unreachable!("close never marks frames")
    }
}

// Test-only instrumentation at the existing resource callback boundaries. The
// callback mutates actual arena fields, never a detached binding projection.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestResourceEvent {
    Retain,
    Release,
    AllocateEvaluator,
}
#[cfg(test)]
type TestResourceMutation = dyn FnMut(TestResourceEvent, &mut Vec<FrameKey>, &mut Vec<LoadedFrame>);
#[cfg(test)]
struct TestResourceCallback(Box<TestResourceMutation>);
#[cfg(test)]
impl std::fmt::Debug for TestResourceCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("TestResourceCallback")
    }
}

struct LiveResources<'a> {
    graph: &'a mut I3dFrameGraph,
    frames: &'a mut Vec<LoadedFrame>,
    members: &'a mut Vec<FrameKey>,
    next_identity: &'a mut u32,
    #[cfg(test)]
    fail_evaluator_after: &'a mut Option<usize>,
    #[cfg(test)]
    test_callback: &'a mut Option<TestResourceCallback>,
}
impl LiveResources<'_> {
    #[cfg(test)]
    fn event(&mut self, event: TestResourceEvent) {
        if let Some(callback) = self.test_callback {
            (callback.0)(event, self.members, self.frames);
        }
    }
    fn identity(&mut self) -> Result<u32, ModelAnimationResourceError> {
        *self.next_identity = self
            .next_identity
            .checked_add(1)
            .ok_or(ModelAnimationResourceError::IdentityExhausted)?;
        Ok(*self.next_identity)
    }
}
impl ControllerResourceBackend for LiveResources<'_> {
    type Error = ModelAnimationResourceError;
    fn allocate_evaluator(&mut self) -> Result<EvaluatorId, Self::Error> {
        #[cfg(test)]
        self.event(TestResourceEvent::AllocateEvaluator);
        #[cfg(test)]
        if let Some(remaining) = self.fail_evaluator_after.as_mut() {
            if *remaining == 0 {
                *self.fail_evaluator_after = None;
                return Err(ModelAnimationResourceError::NativeEvaluatorAllocation);
            }
            *remaining -= 1;
        }
        self.identity().map(EvaluatorId)
    }
    fn free_evaluator(&mut self, _: EvaluatorId) {}
    fn allocate_storage(&mut self, _: usize) -> Result<AllocationId, Self::Error> {
        self.identity().map(AllocationId)
    }
    fn free_storage(&mut self, _: AllocationId) {}
    fn retain_animation(&mut self, _: AnimSetId) {
        #[cfg(test)]
        self.event(TestResourceEvent::Retain);
    }
    fn release_animation(&mut self, _: AnimSetId) {
        #[cfg(test)]
        self.event(TestResourceEvent::Release);
    }
    fn mark_animated(&mut self, destination: FrameKey) {
        // Live resolution validated the destination. This backend has no
        // callbacks capable of retiring graph nodes between resolution/mark.
        self.graph
            .mark_animated(destination)
            .expect("resolved live destination");
    }
}
fn c_name(name: &[u8]) -> &[u8] {
    &name[..name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len())]
}
impl LiveControllerResourceBackend for LiveResources<'_> {
    fn first_frame_named(&mut self, name: &[u8]) -> Result<Option<(usize, FrameKey)>, Self::Error> {
        for (index, &key) in self.members.iter().enumerate() {
            let frame = key
                .0
                .checked_sub(1)
                .and_then(|slot| self.frames.get(slot as usize))
                .filter(|frame| frame.key == key && frame.is_alive())
                .ok_or(ModelAnimationResourceError::Model(
                    ModelError::AnimationIdentity,
                ))?;
            let local = frame.original_name_bytes.as_deref().unwrap_or(b"");
            if matches!(
                frame.name_pointer,
                crate::frame_fast_lookup::OriginalFrameName::Released { .. }
            ) {
                return Err(ModelAnimationResourceError::Model(
                    ModelError::UnavailableOriginalName(key),
                ));
            }
            if c_name(local) == c_name(name) {
                self.graph
                    .state(key)
                    .map_err(|error| ModelAnimationResourceError::Model(error.into()))?;
                return Ok(Some((index, key)));
            }
        }
        Ok(None)
    }
}

impl ModelArena {
    /// Native successful/delegate HRESULT, or host/required-boundary failure.
    /// No evaluator Tick occurs here. Even source=None lazily creates an owner.
    #[allow(clippy::too_many_arguments)]
    pub fn set_original_model_animation<F: ModelControllerFactory>(
        &mut self,
        model: ModelKey,
        resource: Option<&LiveAnimationResource>,
        index: u32,
        weight: f32,
        mode: i32,
        factory: &mut F,
    ) -> Result<u32, ModelAnimationCallError<F::Error>> {
        if index >= 8 {
            return Err(ModelAnimationCallError::SlotOutsideDomain);
        }
        let mi = self.model_index(model)?;
        if self.models[mi].closing {
            return Err(ModelError::CloseIncomplete(model).into());
        }
        if matches!(
            self.models[mi].original_ipol_set,
            Some(OriginalModelController::External(_))
        ) {
            return Err(ModelError::UnsupportedActiveModelController(model).into());
        }
        if self.models[mi].original_ipol_set.is_none() {
            let destination = ModelAnimationDestination {
                model,
                reference: self.lookup_reference(model.root()),
            };
            let allocation = factory
                .allocate(destination)
                .map_err(ModelAnimationCallError::Factory)?;
            if allocation
                .as_ref()
                .is_some_and(|allocation| allocation.destination != destination)
            {
                return Err(ModelAnimationCallError::ForeignControllerAllocation);
            }
            // Publish before handler/delegate. None remains genuinely absent;
            // delegate failure preserves a newly published actual owner.
            self.models[mi].original_ipol_set = allocation.map(OriginalModelController::Owned);
            if self.models[mi].original_ipol_set.is_none() {
                factory
                    .allocation_failed(ModelControllerAllocationFailure {
                        hresult: ANIMATION_OUT_OF_MEMORY,
                        severity: 3,
                        context: None,
                    })
                    .map_err(ModelAnimationCallError::Factory)?;
                return Ok(ANIMATION_OUT_OF_MEMORY);
            }
        }
        let loaded = &mut self.models[mi];
        let Some(OriginalModelController::Owned(allocation)) = &mut loaded.original_ipol_set else {
            unreachable!("published actual owner")
        };
        let result = allocation.controller.set_live_animation(
            &mut LiveResources {
                graph: &mut self.graph,
                frames: &mut self.frames,
                members: &mut loaded.frames,
                next_identity: &mut allocation.next_identity,
                #[cfg(test)]
                fail_evaluator_after: &mut allocation.fail_evaluator_after,
                #[cfg(test)]
                test_callback: &mut allocation.test_callback,
            },
            resource,
            index as usize,
            weight,
            mode,
        );
        match result {
            Ok(()) => Ok(0),
            #[cfg(test)]
            Err(ControllerError::Resource(
                ModelAnimationResourceError::NativeEvaluatorAllocation,
            )) => Ok(ANIMATION_OUT_OF_MEMORY),
            Err(error) => Err(ModelAnimationCallError::Controller(error)),
        }
    }

    pub fn original_model_controller(
        &self,
        model: ModelKey,
    ) -> Result<Option<&IPolSet>, ModelError> {
        let loaded = self.model(model)?;
        match &loaded.original_ipol_set {
            None => Ok(None),
            Some(OriginalModelController::Owned(allocation)) => Ok(Some(&allocation.controller)),
            Some(OriginalModelController::External(_)) => {
                Err(ModelError::UnsupportedActiveModelController(model))
            }
        }
    }
    pub(crate) fn original_model_controller_mut(
        &mut self,
        model: ModelKey,
    ) -> Result<Option<&mut IPolSet>, ModelError> {
        let mi = self.model_index(model)?;
        match &mut self.models[mi].original_ipol_set {
            None => Ok(None),
            Some(OriginalModelController::Owned(allocation)) => {
                Ok(Some(&mut allocation.controller))
            }
            Some(OriginalModelController::External(_)) => {
                Err(ModelError::UnsupportedActiveModelController(model))
            }
        }
    }
    pub(crate) fn close_original_model_controller(
        &mut self,
        model: ModelKey,
    ) -> Result<(), ModelError> {
        let mi = self.model_index(model)?;
        match &mut self.models[mi].original_ipol_set {
            None => return Ok(()),
            Some(OriginalModelController::External(_)) => {
                return Err(ModelError::UnsupportedActiveModelController(model))
            }
            Some(OriginalModelController::Owned(allocation)) => allocation.close(),
        }
        // Destruction completed while the owner remained published.
        self.models[mi].original_ipol_set = None;
        Ok(())
    }
    pub fn clear_original_model_animation(&mut self, model: ModelKey) -> Result<(), ModelError> {
        if let Some(controller) = self.original_model_controller_mut(model)? {
            controller.clear(&mut ReleaseResources);
        }
        Ok(())
    }
    pub fn start_original_model_animation(
        &mut self,
        model: ModelKey,
        index: u32,
    ) -> Result<(), ModelError> {
        self.model_index(model)?;
        if index < 8 {
            if let Some(controller) = self.original_model_controller_mut(model)? {
                controller.start_animation(index as usize);
            }
        }
        Ok(())
    }
    pub fn set_original_model_animation_time(
        &mut self,
        model: ModelKey,
        index: u32,
        time: i32,
    ) -> Result<(), ModelError> {
        self.model_index(model)?;
        if index < 8 {
            if let Some(controller) = self.original_model_controller_mut(model)? {
                controller
                    .set_anim_time(index as usize, time)
                    .map_err(ModelError::AnimationController)?;
            }
        }
        Ok(())
    }
    pub fn original_model_animation_time(
        &self,
        model: ModelKey,
        index: u32,
    ) -> Result<i32, ModelError> {
        if index >= 8 {
            self.model(model)?;
            return Ok(0);
        }
        Ok(self
            .original_model_controller(model)?
            .map_or(0, |controller| controller.anim_time(index as usize)))
    }
    pub fn set_original_model_animation_blend(
        &mut self,
        model: ModelKey,
        index: u32,
        blend: f32,
    ) -> Result<(), ModelError> {
        self.model_index(model)?;
        if index < 8 {
            if let Some(controller) = self.original_model_controller_mut(model)? {
                controller
                    .set_anim_blend(index as usize, blend)
                    .map_err(ModelError::AnimationController)?;
            }
        }
        Ok(())
    }
    pub fn original_model_animation_blend(
        &self,
        model: ModelKey,
        index: u32,
    ) -> Result<f32, ModelError> {
        if index >= 8 {
            self.model(model)?;
            return Ok(0.);
        }
        Ok(self
            .original_model_controller(model)?
            .map_or(0., |controller| controller.anim_blend(index as usize)))
    }
    pub fn set_original_model_animation_options(
        &mut self,
        model: ModelKey,
        index: u32,
        options: i32,
    ) -> Result<(), ModelError> {
        self.model_index(model)?;
        if index < 8 {
            if let Some(controller) = self.original_model_controller_mut(model)? {
                controller
                    .set_anim_options(index as usize, options)
                    .map_err(ModelError::AnimationController)?;
            }
        }
        Ok(())
    }
    pub fn original_model_animation_options(
        &self,
        model: ModelKey,
        index: u32,
    ) -> Result<i32, ModelError> {
        if index >= 8 {
            self.model(model)?;
            return Ok(0);
        }
        Ok(self
            .original_model_controller(model)?
            .map_or(0, |controller| controller.anim_options(index as usize)))
    }
}

#[derive(Debug)]
pub enum ModelAnimationTickError {
    Model(ModelError),
    Controller(ControllerError<Infallible>),
    Interpolator(TickError),
    /// Host admission lock after a prior interrupted operation, not a native
    /// poison flag. Close remains available and releases the actual owner.
    Interrupted,
    Unsupported {
        evaluator: Option<EvaluatorId>,
        slot: Option<usize>,
        feature: &'static str,
    },
    Sampling {
        evaluator: EvaluatorId,
        slot: usize,
        message: String,
    },
}
impl From<ModelError> for ModelAnimationTickError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

struct OwnedModelTick<'a> {
    arena: &'a mut ModelArena,
    model: ModelKey,
    pose_written: bool,
}
impl LiveModelTickOwner for OwnedModelTick<'_> {
    type Error = ModelAnimationTickError;
    fn active_evaluator_count(&self) -> Result<Option<u32>, Self::Error> {
        self.arena
            .original_model_controller(self.model)?
            .map(|controller| {
                u32::try_from(controller.state().evaluators.count).map_err(|_| {
                    ModelAnimationTickError::Unsupported {
                        evaluator: None,
                        slot: None,
                        feature: "evaluator count exceeds native width",
                    }
                })
            })
            .transpose()
    }
    fn tick_current_controller(&mut self, delta: i32) -> Result<(), Self::Error> {
        let mi = self.arena.model_index(self.model)?;
        let Some(OriginalModelController::Owned(allocation)) =
            &mut self.arena.models[mi].original_ipol_set
        else {
            return Err(ModelError::UnsupportedActiveModelController(self.model).into());
        };
        if allocation.tick_interrupted {
            return Err(ModelAnimationTickError::Interrupted);
        }
        let (state, view) = allocation
            .controller
            .borrow_tick_parts()
            .map_err(ModelAnimationTickError::Controller)?;
        let mut backend = OwnedInterpolatorTick {
            view,
            graph: &mut self.arena.graph,
            failure: None,
            pose_written: false,
        };
        // This is the actual stored InterpolatorSet. It remains in place and
        // callbacks receive that same mutable state, including reached clocks.
        let result = state.tick(&mut backend, delta);
        self.pose_written |= backend.pose_written;
        if let Some(error) = backend.failure {
            return Err(error);
        }
        result.map_err(ModelAnimationTickError::Interpolator)
    }
    fn update_current_model(&mut self) -> Result<(), Self::Error> {
        // Actual live frame-kind dispatch replaces a native vtable read. No
        // copied ModelTickState or cached virtual-table projection is used.
        self.arena
            .update_frame(self.model.root())
            .map_err(Into::into)
    }
}

struct OwnedInterpolatorTick<'a> {
    view: ControllerEvaluationView<'a>,
    graph: &'a mut I3dFrameGraph,
    failure: Option<ModelAnimationTickError>,
    pose_written: bool,
}
impl OwnedInterpolatorTick<'_> {
    fn unsupported(
        evaluator: EvaluatorId,
        slot: usize,
        feature: &'static str,
    ) -> ModelAnimationTickError {
        ModelAnimationTickError::Unsupported {
            evaluator: Some(evaluator),
            slot: Some(slot),
            feature,
        }
    }
    fn evaluate_frame(
        &mut self,
        set: &InterpolatorSet,
        id: EvaluatorId,
    ) -> Result<(), ModelAnimationTickError> {
        let evaluator = self
            .view
            .evaluator(id)
            .ok_or(ModelAnimationTickError::Interpolator(
                TickError::MissingEvaluator(id),
            ))?;
        let mut position = None;
        let mut rotation = None;
        let mut contributor = None;
        // The original helper is channel-major: position slots, rotation slots,
        // scale slots, pose, destination writes, then note dispatch. Numeric
        // sampling is preflighted within this evaluator; no native arithmetic
        // exception-prefix claim is made outside the verified sampler domain.
        for channel in [2u16, 4, 8] {
            for slot_index in 0..8 {
                let binding = evaluator.bindings[slot_index];
                let slot = &set.slots[slot_index];
                if binding.flags & channel == 0 || !slot.enabled {
                    continue;
                }
                // Native ordered comparisons skip nonpositive weights; NaNs
                // reach the sampling branch and are explicitly unadmitted.
                if slot.weight <= 0. {
                    continue;
                }
                if channel == 8 {
                    return Err(Self::unsupported(
                        id,
                        slot_index,
                        "scale channel evaluation",
                    ));
                }
                if slot.weight.to_bits() != 1f32.to_bits() {
                    return Err(Self::unsupported(id, slot_index, "non-unit slot weight"));
                }
                if contributor.is_some_and(|previous| previous != slot_index) {
                    return Err(Self::unsupported(
                        id,
                        slot_index,
                        "multiple contributing slots",
                    ));
                }
                contributor = Some(slot_index);
                let resource = self.view.live_animation(slot_index).ok_or(
                    ModelAnimationTickError::Interpolator(TickError::MissingAnimationSet {
                        slot: slot_index,
                    }),
                )?;
                let track = resource
                    .source()
                    .tracks
                    .get(binding.track_index)
                    .ok_or_else(|| Self::unsupported(id, slot_index, "bound track unavailable"))?;
                let time = u32::try_from(slot.current)
                    .map_err(|_| Self::unsupported(id, slot_index, "negative evaluator time"))?;
                if channel == 2 {
                    position = Some(
                        track
                            .position
                            .as_ref()
                            .ok_or_else(|| {
                                Self::unsupported(id, slot_index, "position channel unavailable")
                            })?
                            .sample_position_channel(time)
                            .map_err(|error| ModelAnimationTickError::Sampling {
                                evaluator: id,
                                slot: slot_index,
                                message: error.to_string(),
                            })?,
                    );
                } else {
                    rotation = Some(
                        sample_rotation_channel(
                            track.rotation.as_ref().ok_or_else(|| {
                                Self::unsupported(id, slot_index, "rotation channel unavailable")
                            })?,
                            time,
                        )
                        .map_err(|error| {
                            ModelAnimationTickError::Sampling {
                                evaluator: id,
                                slot: slot_index,
                                message: format!("{error:?}"),
                            }
                        })?,
                    );
                }
            }
        }
        let destination = evaluator.destination;
        if let Some(position) = position {
            let unlocked = self
                .graph
                .state(destination)
                .map_err(ModelError::from)?
                .flags
                & 0x80
                == 0;
            self.graph
                .assign_position(destination, position)
                .map_err(ModelError::from)?;
            self.pose_written |= unlocked;
        }
        if let Some(rotation) = rotation {
            self.graph
                .apply_evaluated_rotation(destination, rotation)
                .map_err(ModelError::from)?;
            self.pose_written = true;
        }
        let frame_flags = self
            .graph
            .state(destination)
            .map_err(ModelError::from)?
            .flags;
        if frame_flags & 0x1200 != 0 {
            for slot_index in 0..8 {
                if set.slots[slot_index].enabled && evaluator.bindings[slot_index].flags & 0x30 != 0
                {
                    return Err(Self::unsupported(
                        id,
                        slot_index,
                        "note callback/resource owner",
                    ));
                }
            }
        }
        Ok(())
    }
}
impl InterpolatorBackend for OwnedInterpolatorTick<'_> {
    fn evaluator_at(
        &self,
        allocation: AllocationId,
        index: usize,
    ) -> Result<EvaluatorId, TickError> {
        self.view
            .evaluator_at(allocation, index)
            .ok_or(TickError::InvalidEvaluatorStorage { allocation, index })
    }
    fn evaluate(
        &mut self,
        set: &mut InterpolatorSet,
        evaluator: EvaluatorId,
    ) -> Result<(), TickError> {
        if let Err(error) = self.evaluate_frame(set, evaluator) {
            self.failure = Some(error);
            return Err(TickError::MissingEvaluator(evaluator));
        }
        Ok(())
    }
    fn boundary_destination(&self, evaluator: EvaluatorId) -> Result<Option<FrameId>, TickError> {
        let evaluator = self
            .view
            .evaluator(evaluator)
            .ok_or(TickError::MissingEvaluator(evaluator))?;
        let frame = FrameId(evaluator.destination.0);
        let flags = self
            .graph
            .state(evaluator.destination)
            .map_err(|_| TickError::MissingFrame(frame))?
            .flags;
        Ok((flags & 0x800 != 0).then_some(frame))
    }
    fn notify_boundary(
        &mut self,
        _: &mut InterpolatorSet,
        frame: FrameId,
        slot: usize,
    ) -> Result<(), TickError> {
        self.failure = Some(ModelAnimationTickError::Unsupported {
            evaluator: None,
            slot: Some(slot),
            feature: "animation boundary callback owner",
        });
        Err(TickError::MissingFrame(frame))
    }
    fn duration(&mut self, _: &mut InterpolatorSet, id: AnimSetId) -> Result<i32, TickError> {
        // end_frame is u16 and duration=end_frame*40, so this conversion is exact.
        self.view
            .live_resource(id)
            .map(|resource| resource.source().duration_ms() as i32)
            .ok_or(TickError::MissingDuration(id))
    }
}

impl ModelArena {
    /// Explicit native model Tick ordering on its actual owned controller.
    /// Returns whether the nonnull/nonempty entry guard admitted the body.
    /// This does not select an animation or schedule a scene/game update.
    pub fn tick_original_model_animation(
        &mut self,
        model: ModelKey,
        delta: i32,
    ) -> Result<bool, ModelAnimationTickError> {
        let mi = self.model_index(model)?;
        if self.models[mi].closing {
            return Err(ModelError::CloseIncomplete(model).into());
        }
        let mut owner = OwnedModelTick {
            arena: self,
            model,
            pose_written: false,
        };
        let result = tick_live_model(&mut owner, delta);
        if result.is_err() {
            if let Some(OriginalModelController::Owned(allocation)) =
                &mut owner.arena.models[mi].original_ipol_set
            {
                allocation.tick_interrupted = true;
            }
            if owner.pose_written {
                owner.arena.interrupt_owned_model_animation_pose(model)?;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lhr_assets::{
        animation::{AnimationSet, AnimationTrack},
        Model,
    };
    use std::sync::Arc;

    #[derive(Default)]
    struct Factory {
        dirty: bool,
        null: bool,
        interrupt_handler: bool,
        fail_evaluator_after: Option<usize>,
        test_callback: Option<TestResourceCallback>,
        calls: usize,
        failures: Vec<ModelControllerAllocationFailure>,
    }
    impl ModelControllerFactory for Factory {
        type Error = &'static str;
        fn allocate(
            &mut self,
            owner: ModelAnimationDestination,
        ) -> Result<Option<ModelControllerAllocation>, Self::Error> {
            self.calls += 1;
            if self.null {
                return Ok(None);
            }
            let mut allocation = ModelControllerAllocation::new(owner, self.dirty);
            allocation.fail_evaluator_after = self.fail_evaluator_after;
            allocation.test_callback = self.test_callback.take();
            Ok(Some(allocation))
        }
        fn allocation_failed(
            &mut self,
            failure: ModelControllerAllocationFailure,
        ) -> Result<(), Self::Error> {
            self.failures.push(failure);
            if self.interrupt_handler {
                return Err("error handler interrupted");
            }
            Ok(())
        }
    }
    fn parsed(names: &[&[u8]]) -> Arc<Model> {
        Arc::new(Model {
            materials: vec![],
            frames: names
                .iter()
                .map(|name| {
                    let mut frame = crate::model_loading::tests::frame(6, None, [0.; 3]);
                    frame.name = String::from_utf8_lossy(name).into_owned();
                    frame.name_bytes = name.to_vec();
                    frame
                })
                .collect(),
        })
    }
    fn resource(names: &[&[u8]], mode: i32) -> LiveAnimationResource {
        LiveAnimationResource::new(
            Arc::new(AnimationSet {
                timestamp: [0; 8],
                end_frame: 10,
                tracks: names
                    .iter()
                    .map(|name| AnimationTrack {
                        name: name.to_vec(),
                        flags: 2,
                        reserved: 0,
                        rotation: None,
                        position: None,
                        scale: None,
                    })
                    .collect(),
            }),
            mode,
        )
        .unwrap()
    }
    fn moving_source(records: &[(&[u8], u16)]) -> Arc<AnimationSet> {
        let mut payload = vec![0; 4 + records.len() * 8];
        payload[..2].copy_from_slice(&(records.len() as u16).to_le_bytes());
        payload[2..4].copy_from_slice(&10u16.to_le_bytes());
        for (index, (name, flags)) in records.iter().enumerate() {
            let offset = payload.len() as u32;
            payload[4 + index * 8..8 + index * 8].copy_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(name);
            payload.push(0);
            let offset = payload.len() as u32;
            payload[8 + index * 8..12 + index * 8].copy_from_slice(&offset.to_le_bytes());
            payload.extend(flags.to_le_bytes());
            payload.extend(0u16.to_le_bytes());
            for channel in [4, 2, 8] {
                if flags & channel == 0 {
                    continue;
                }
                payload.extend(2u16.to_le_bytes());
                payload.extend(0u16.to_le_bytes());
                payload.extend(10u16.to_le_bytes());
                payload.extend(0u16.to_le_bytes());
                let values: Vec<f32> = match channel {
                    4 => vec![1., 0., 0., 0., 1., 0.001, 0., 0.],
                    2 => vec![0., 0., 0., 10., 0., 0.],
                    _ => vec![1., 1., 1., 2., 2., 2.],
                };
                for value in values {
                    payload.extend(value.to_le_bytes());
                }
            }
        }
        let mut bytes = b"5DS\0".to_vec();
        bytes.extend(20u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        Arc::new(lhr_assets::animation::parse_5ds(&bytes).unwrap())
    }
    fn moving(records: &[(&[u8], u16)]) -> LiveAnimationResource {
        LiveAnimationResource::new(moving_source(records), 2).unwrap()
    }
    struct SkinFixture {
        directory: std::path::PathBuf,
        arena: ModelArena,
        model: ModelKey,
        keys: Vec<FrameKey>,
        source: Arc<Model>,
    }
    fn skin_fixture() -> SkinFixture {
        let mut source = crate::model_loading::tests::skinned_source();
        for (index, name) in [(1, b"first".as_slice()), (2, b"second".as_slice())] {
            let frame = &mut Arc::make_mut(&mut source).frames[index];
            frame.name = String::from_utf8_lossy(name).into_owned();
            frame.name_bytes = name.to_vec();
        }
        let directory = std::env::temp_dir().join(format!(
            "lhr-owned-animation-tick-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let mut arena = ModelArena::default();
        let (model, keys) = load(&mut arena, Arc::clone(&source));
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        SkinFixture {
            directory,
            arena,
            model,
            keys,
            source,
        }
    }
    fn load(arena: &mut ModelArena, source: Arc<Model>) -> (ModelKey, Vec<FrameKey>) {
        let model = arena.create_model().unwrap();
        let content = arena.load_parsed_into_model(model, source).unwrap();
        (model, content.frames().to_vec())
    }

    #[test]
    fn null_source_still_lazily_publishes_owner_and_preserves_allocator_dirty() {
        for dirty in [false, true] {
            let mut arena = ModelArena::default();
            let model = arena.create_model().unwrap();
            let mut factory = Factory {
                dirty,
                ..Factory::default()
            };
            let baseline_bytes = arena.storage_bytes().unwrap();
            assert!(arena.original_model_controller(model).unwrap().is_none());
            assert_eq!(
                arena
                    .set_original_model_animation(model, None, 0, 1., 0, &mut factory)
                    .unwrap(),
                0
            );
            let controller = arena.original_model_controller(model).unwrap().unwrap();
            assert_eq!(controller.state().dirty, dirty);
            assert_eq!(controller.owner(), model.root());
            assert_eq!(controller.state().evaluators.count, 0);
            assert!(arena.storage_bytes().unwrap() > baseline_bytes);
            arena
                .set_original_model_animation(model, None, 7, 1., 0, &mut factory)
                .unwrap();
            assert_eq!(factory.calls, 1);
            assert!(factory.failures.is_empty());
            assert!(matches!(
                arena.set_original_model_animation(model, None, 8, 1., 0, &mut factory),
                Err(ModelAnimationCallError::SlotOutsideDomain)
            ));
            assert_eq!(factory.calls, 1);
            arena.close(model).unwrap();
            assert!(arena.original_model_controller(model).unwrap().is_none());
            assert_eq!(arena.storage_bytes().unwrap(), baseline_bytes);
        }
    }

    #[test]
    fn null_allocation_reports_native_arguments_and_preserves_handler_interruption() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let mut factory = Factory {
            null: true,
            ..Factory::default()
        };
        assert_eq!(
            arena
                .set_original_model_animation(model, None, 0, 1., 0, &mut factory)
                .unwrap(),
            ANIMATION_OUT_OF_MEMORY
        );
        assert!(arena.original_model_controller(model).unwrap().is_none());
        assert_eq!(
            factory.failures,
            vec![ModelControllerAllocationFailure {
                hresult: ANIMATION_OUT_OF_MEMORY,
                severity: 3,
                context: None
            }]
        );
        factory.interrupt_handler = true;
        assert!(matches!(
            arena.set_original_model_animation(model, None, 0, 1., 0, &mut factory),
            Err(ModelAnimationCallError::Factory(
                "error handler interrupted"
            ))
        ));
        assert_eq!(factory.calls, 2);
        assert_eq!(factory.failures.len(), 2);
        assert!(arena.original_model_controller(model).unwrap().is_none());
    }

    #[test]
    fn actual_membership_original_names_and_null_names_drive_binding() {
        let source = parsed(&[b"same", b"same"]);
        let mut arena = ModelArena::default();
        let (model, keys) = load(&mut arena, Arc::clone(&source));
        arena
            .set_frame_model_name(keys[0], b"renamed.full")
            .unwrap();
        let mut factory = Factory::default();
        let animation = resource(&[b"same"], 2);
        arena
            .set_original_model_animation(model, Some(&animation), 0, 1., 0, &mut factory)
            .unwrap();
        let controller = arena.original_model_controller(model).unwrap().unwrap();
        assert_eq!(
            controller
                .evaluator(controller.active_evaluators()[0])
                .unwrap()
                .destination,
            keys[0]
        );
        assert_eq!(
            arena.frame(keys[0]).unwrap().name_bytes,
            b"renamed.full.same"
        );
        assert_eq!(source.frames[0].name_bytes, b"same");
        // Actual membership order, independent of parsed source order.
        let mi = arena.model_index(model).unwrap();
        arena.models[mi].frames.swap(0, 1);
        arena
            .set_original_model_animation(model, Some(&animation), 1, 1., 0, &mut factory)
            .unwrap();
        let controller = arena.original_model_controller(model).unwrap().unwrap();
        assert_eq!(
            controller
                .evaluator(controller.active_evaluators()[1])
                .unwrap()
                .destination,
            keys[1]
        );
        assert_eq!(factory.calls, 1);
        // A null original name compares as the native empty string; changed
        // parsed backing cannot supply a different name for this live frame.
        arena.clear_original_model_animation(model).unwrap();
        let fi = arena.frame_index(keys[1]).unwrap();
        arena.frames[fi].original_name_bytes = None;
        let empty = resource(&[b""], 1);
        arena
            .set_original_model_animation(model, Some(&empty), 0, 1., 0, &mut factory)
            .unwrap();
        let controller = arena.original_model_controller(model).unwrap().unwrap();
        assert_eq!(
            controller
                .evaluator(controller.active_evaluators()[0])
                .unwrap()
                .destination,
            keys[1]
        );
        arena.close(model).unwrap();
    }

    #[test]
    fn delegate_failure_keeps_published_controller_resource_and_retryable_prefix() {
        let mut arena = ModelArena::default();
        let (model, keys) = load(&mut arena, parsed(&[b"first", b"second", b"last"]));
        let animation = resource(&[b"first", b"second", b"last"], 9);
        let mut factory = Factory {
            fail_evaluator_after: Some(1),
            dirty: false,
            ..Factory::default()
        };
        assert_eq!(
            arena
                .set_original_model_animation(model, Some(&animation), 0, 4., 0, &mut factory)
                .unwrap(),
            ANIMATION_OUT_OF_MEMORY
        );
        let controller = arena.original_model_controller(model).unwrap().unwrap();
        let slot = controller.state().slots[0];
        assert_eq!(
            (
                slot.current,
                slot.previous,
                slot.weight,
                slot.mode,
                slot.enabled
            ),
            (0, -40, 4., 9, true)
        );
        assert!(!controller.state().dirty);
        assert_eq!(controller.animation_id(0), Some(animation.id()));
        assert_eq!(controller.state().evaluators.count, 1);
        assert!(
            matches!(crate::model_loading::SharedAnimationModel::borrow(&mut arena, model), Err(ModelError::UnsupportedActiveModelController(key)) if key == model)
        );
        assert_ne!(arena.state(keys[0]).unwrap().flags & 0x1000_0000, 0);
        assert_eq!(arena.state(keys[1]).unwrap().flags & 0x1000_0000, 0);
        assert_eq!(arena.state(keys[2]).unwrap().flags & 0x1000_0000, 0);
        assert!(factory.failures.is_empty());
        // Retry through the same authoritative owner; no lazy reallocation.
        arena
            .set_original_model_animation(model, Some(&animation), 0, 1., 0, &mut factory)
            .unwrap();
        assert_eq!(factory.calls, 1);
        assert!(
            arena
                .original_model_controller(model)
                .unwrap()
                .unwrap()
                .state()
                .dirty
        );
        assert_ne!(arena.state(keys[0]).unwrap().flags & 0x1000_0000, 0);
        assert_eq!(
            arena
                .original_model_controller(model)
                .unwrap()
                .unwrap()
                .state()
                .evaluators
                .count,
            3
        );
        arena.close(model).unwrap();
    }

    #[test]
    fn controls_shared_resources_clear_and_close_use_the_same_actual_owner() {
        let mut arena = ModelArena::default();
        let source = parsed(&[b"matched"]);
        let (first, _) = load(&mut arena, Arc::clone(&source));
        let (second, _) = load(&mut arena, source);
        let animation = resource(&[b"matched"], 2);
        let weak = Arc::downgrade(animation.source());
        let mut factory = Factory::default();
        assert_eq!(arena.original_model_animation_time(first, 0).unwrap(), 0);
        arena
            .set_original_model_animation_time(first, 0, 99)
            .unwrap();
        assert!(arena.original_model_controller(first).unwrap().is_none());
        for model in [first, second] {
            arena
                .set_original_model_animation(model, Some(&animation), 0, 1., 0, &mut factory)
                .unwrap();
        }
        arena
            .set_original_model_animation_time(first, 0, -5)
            .unwrap();
        arena.stop_original_model_animation(first, 0).unwrap();
        assert_eq!(arena.original_model_animation_time(first, 0).unwrap(), -5);
        assert_eq!(arena.original_model_animation_time(second, 0).unwrap(), 0);
        arena.start_original_model_animation(first, 0).unwrap();
        arena
            .set_original_model_animation_blend(first, 0, -0.)
            .unwrap();
        assert_eq!(
            arena
                .original_model_animation_blend(first, 0)
                .unwrap()
                .to_bits(),
            (-0f32).to_bits()
        );
        animation.set_default_mode(6);
        arena
            .set_original_model_animation(first, Some(&animation), 0, 1., 0, &mut factory)
            .unwrap();
        assert_eq!(arena.original_model_animation_options(first, 0).unwrap(), 6);
        assert_eq!(
            arena.original_model_animation_options(second, 0).unwrap(),
            2
        );
        assert_eq!(
            arena.original_model_animation_set(first, 0).unwrap(),
            Some(animation.id())
        );
        let capacity = arena
            .original_model_controller(first)
            .unwrap()
            .unwrap()
            .evaluator_capacity();
        arena.clear_original_model_animation(first).unwrap();
        let controller = arena.original_model_controller(first).unwrap().unwrap();
        assert_eq!(controller.evaluator_capacity(), capacity);
        assert_eq!(controller.state().evaluators.count, 0);
        assert_eq!(arena.original_model_animation_set(first, 0).unwrap(), None);
        drop(animation);
        arena.close(first).unwrap();
        assert!(weak.upgrade().is_some());
        arena.close(second).unwrap();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn close_releases_actual_controller_before_a_failed_member_destructor() {
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.name = "matched".into();
        frame.name_bytes = b"matched".to_vec();
        let mut arena = ModelArena::default();
        let (model, keys) = load(
            &mut arena,
            Arc::new(Model {
                materials: vec![],
                frames: vec![frame],
            }),
        );
        let animation = resource(&[b"matched"], 2);
        let weak = Arc::downgrade(animation.source());
        arena
            .set_original_model_animation(
                model,
                Some(&animation),
                0,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        drop(animation);
        assert!(arena.close(model).is_err());
        assert!(weak.upgrade().is_none());
        assert!(arena.original_model_controller(model).unwrap().is_none());
        let mut factory = Factory::default();
        assert!(matches!(
            arena.set_original_model_animation(model, None, 0, 1., 0, &mut factory),
            Err(ModelAnimationCallError::Model(ModelError::CloseIncomplete(
                _
            )))
        ));
        assert_eq!(factory.calls, 0);
        arena.retain(keys[0]).unwrap();
        arena.close(model).unwrap();
        assert!(arena.frame(keys[0]).is_ok());
    }

    #[test]
    fn live_binding_accepts_appended_members_and_rejects_a_foreign_factory_owner() {
        let mut arena = ModelArena::default();
        let (first, _) = load(&mut arena, parsed(&[b"base"]));
        let (second, keys) = load(&mut arena, parsed(&[b"external"]));
        arena.retain(keys[0]).unwrap();
        arena.delete_frame(second, keys[0]).unwrap();
        arena.add_frame(first, keys[0]).unwrap();
        arena.release(keys[0]).unwrap();
        let animation = resource(&[b"external"], 2);
        arena
            .set_original_model_animation(
                first,
                Some(&animation),
                0,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        let controller = arena.original_model_controller(first).unwrap().unwrap();
        assert_eq!(
            controller
                .evaluator(controller.active_evaluators()[0])
                .unwrap()
                .destination,
            keys[0]
        );
        assert_eq!(
            controller
                .evaluator(controller.active_evaluators()[0])
                .unwrap()
                .bindings[0]
                .frame_index,
            1
        );
        arena.close(first).unwrap();
        arena.close(second).unwrap();

        struct ForeignFactory(ModelAnimationDestination);
        impl ModelControllerFactory for ForeignFactory {
            type Error = Infallible;
            fn allocate(
                &mut self,
                _: ModelAnimationDestination,
            ) -> Result<Option<ModelControllerAllocation>, Infallible> {
                Ok(Some(ModelControllerAllocation::new(self.0, false)))
            }
            fn allocation_failed(
                &mut self,
                _: ModelControllerAllocationFailure,
            ) -> Result<(), Infallible> {
                panic!("host admission does not call native error handler")
            }
        }
        assert!(matches!(
            arena.set_original_model_animation(
                first,
                None,
                0,
                1.,
                0,
                &mut ForeignFactory(ModelAnimationDestination {
                    model: second,
                    reference: arena.lookup_reference(second.root())
                })
            ),
            Err(ModelAnimationCallError::ForeignControllerAllocation)
        ));
        assert!(arena.original_model_controller(first).unwrap().is_none());
        assert!(arena.original_model_controller(second).unwrap().is_none());
        let mut foreign = ModelArena::default();
        let foreign_model = foreign.create_model().unwrap();
        assert_eq!(first, foreign_model);
        let destination = ModelAnimationDestination {
            model: foreign_model,
            reference: foreign.lookup_reference(foreign_model.root()),
        };
        assert!(matches!(
            arena.set_original_model_animation(
                first,
                None,
                0,
                1.,
                0,
                &mut ForeignFactory(destination)
            ),
            Err(ModelAnimationCallError::ForeignControllerAllocation)
        ));
        assert!(arena.original_model_controller(first).unwrap().is_none());
    }

    #[test]
    fn actual_owner_rereads_callback_mutated_members_names_and_resource_mode() {
        let mut arena = ModelArena::default();
        let source = parsed(&[b"before", b"before"]);
        let (model, keys) = load(&mut arena, Arc::clone(&source));
        let animation = resource(&[b"one", b"two"], 2);
        let alias = animation.clone();
        let callback_keys = keys.clone();
        let mut allocations = 0;
        let mut factory = Factory {
            test_callback: Some(TestResourceCallback(Box::new(
                move |event, members, frames| {
                    let first = (callback_keys[0].0 - 1) as usize;
                    let second = (callback_keys[1].0 - 1) as usize;
                    if event == TestResourceEvent::Retain {
                        frames[first].original_name_bytes = Some(b"one".to_vec());
                        frames[first].name_bytes = b"one".to_vec();
                        alias.set_default_mode(7);
                    }
                    if event == TestResourceEvent::AllocateEvaluator {
                        allocations += 1;
                        if allocations == 1 {
                            frames[second].original_name_bytes = Some(b"two".to_vec());
                            frames[second].name_bytes = b"two".to_vec();
                            members.swap(0, 1);
                        }
                    }
                },
            ))),
            ..Factory::default()
        };
        arena
            .set_original_model_animation(model, Some(&animation), 0, 1., 0, &mut factory)
            .unwrap();
        let controller = arena.original_model_controller(model).unwrap().unwrap();
        assert_eq!(controller.anim_options(0), 7);
        let evaluators = controller.active_evaluators();
        assert_eq!(evaluators.len(), 2);
        assert_eq!(
            controller.evaluator(evaluators[0]).unwrap().destination,
            keys[0]
        );
        assert_eq!(
            controller.evaluator(evaluators[1]).unwrap().destination,
            keys[1]
        );
        assert_eq!(
            controller.evaluator(evaluators[1]).unwrap().bindings[0].frame_index,
            0
        );
        assert_eq!(arena.model(model).unwrap().frames, [keys[1], keys[0]]);
        assert_eq!(source.frames[0].name_bytes, b"before");
        assert_eq!(source.frames[1].name_bytes, b"before");
        arena.close(model).unwrap();
    }

    #[test]
    fn owned_tick_uses_actual_clock_endpoint_wrap_stop_and_bound_destination() {
        let mut arena = ModelArena::default();
        let (model, keys) = load(&mut arena, parsed(&[b"moving"]));
        let animation = moving(&[(b"moving", 2)]);
        arena
            .set_original_model_animation(
                model,
                Some(&animation),
                7,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        // Binding is already complete. Changing the current/local name cannot
        // redirect Tick to a newly resolved parsed or scene frame.
        let fi = arena.frame_index(keys[0]).unwrap();
        arena.frames[fi].original_name_bytes = Some(b"renamed".to_vec());
        assert!(arena.tick_original_model_animation(model, 0).unwrap());
        arena.tick_original_model_animation(model, 40).unwrap();
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 1.);
        arena
            .set_original_model_animation_time(model, 7, 360)
            .unwrap();
        arena.tick_original_model_animation(model, 40).unwrap();
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 10.);
        let slot = arena
            .original_model_controller(model)
            .unwrap()
            .unwrap()
            .state()
            .slots[7];
        assert_eq!((slot.current, slot.previous, slot.last_delta), (0, 400, 40));
        arena.tick_original_model_animation(model, 40).unwrap();
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 1.);
        arena.stop_original_model_animation(model, 7).unwrap();
        arena.tick_original_model_animation(model, 83).unwrap();
        assert_eq!(arena.original_model_animation_time(model, 7).unwrap(), 40);
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 1.);
        arena.start_original_model_animation(model, 7).unwrap();
        arena.tick_original_model_animation(model, 40).unwrap();
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 2.);
        arena.close(model).unwrap();
    }

    #[test]
    fn null_and_zero_evaluator_owners_skip_clock_and_root_update() {
        let mut arena = ModelArena::default();
        let (model, _) = load(&mut arena, parsed(&[b"present"]));
        arena
            .graph
            .assign_position(model.root(), [3., 0., 0.])
            .unwrap();
        let before = *arena.state(model.root()).unwrap();
        assert!(!arena.tick_original_model_animation(model, 40).unwrap());
        assert_eq!(*arena.state(model.root()).unwrap(), before);
        let unmatched = moving(&[(b"absent", 8)]);
        arena
            .set_original_model_animation(
                model,
                Some(&unmatched),
                0,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        assert!(!arena.tick_original_model_animation(model, 40).unwrap());
        assert_eq!(*arena.state(model.root()).unwrap(), before);
        let slot = arena
            .original_model_controller(model)
            .unwrap()
            .unwrap()
            .state()
            .slots[0];
        assert_eq!((slot.current, slot.previous, slot.last_delta), (0, -40, 0));
        assert!(slot.enabled);
        assert!(
            arena
                .original_model_controller(model)
                .unwrap()
                .unwrap()
                .state()
                .dirty
        );
        arena.close(model).unwrap();
    }

    #[test]
    fn disabled_unmatched_and_zero_weight_channels_are_not_prescanned() {
        let mut arena = ModelArena::default();
        let (model, keys) = load(&mut arena, parsed(&[b"moving"]));
        let animation = moving(&[(b"moving", 2), (b"unmatched-scale", 8)]);
        let scale = moving(&[(b"moving", 8)]);
        let mut factory = Factory::default();
        arena
            .set_original_model_animation(model, Some(&animation), 0, 1., 0, &mut factory)
            .unwrap();
        arena
            .set_original_model_animation(model, Some(&scale), 1, 1., 0, &mut factory)
            .unwrap();
        arena.stop_original_model_animation(model, 1).unwrap();
        arena.tick_original_model_animation(model, 40).unwrap();
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 1.);
        arena.start_original_model_animation(model, 1).unwrap();
        arena
            .set_original_model_animation_blend(model, 1, 0.)
            .unwrap();
        arena.tick_original_model_animation(model, 40).unwrap();
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 2.);
        assert_eq!(arena.original_model_animation_time(model, 1).unwrap(), 40);
        arena.close(model).unwrap();
    }

    #[test]
    fn scale_and_multiple_slots_stop_before_current_evaluator_destination_writes() {
        for multislot in [false, true] {
            let mut arena = ModelArena::default();
            let (model, keys) = load(&mut arena, parsed(&[b"moving"]));
            arena.graph.assign_position(keys[0], [17., 0., 0.]).unwrap();
            arena.update_frame(model.root()).unwrap();
            let mut factory = Factory::default();
            let animation = moving(&[(b"moving", if multislot { 2 } else { 10 })]);
            arena
                .set_original_model_animation(model, Some(&animation), 0, 1., 0, &mut factory)
                .unwrap();
            let second = moving(&[(b"moving", 4)]);
            if multislot {
                arena
                    .set_original_model_animation(model, Some(&second), 1, 1., 0, &mut factory)
                    .unwrap();
            }
            let result = arena.tick_original_model_animation(model, 40);
            let expected = if multislot {
                "multiple contributing slots"
            } else {
                "scale channel evaluation"
            };
            assert!(
                matches!(result, Err(ModelAnimationTickError::Unsupported { feature, .. }) if feature == expected)
            );
            assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 17.);
            let slot = arena
                .original_model_controller(model)
                .unwrap()
                .unwrap()
                .state()
                .slots[0];
            assert_eq!(
                (slot.current, slot.previous, slot.last_delta),
                (40, -40, 40)
            );
            assert!(matches!(
                arena.tick_original_model_animation(model, 40),
                Err(ModelAnimationTickError::Interrupted)
            ));
            assert_eq!(arena.original_model_animation_time(model, 0).unwrap(), 40);
            arena.close(model).unwrap();
        }
    }

    #[test]
    fn boundary_callback_guard_runs_after_evaluation_and_previous_time_write() {
        let mut arena = ModelArena::default();
        let (model, keys) = load(&mut arena, parsed(&[b"moving"]));
        let animation = moving(&[(b"moving", 2)]);
        arena
            .set_original_model_animation(
                model,
                Some(&animation),
                0,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        arena.graph.change_flags(keys[0], 0, 0x800).unwrap();
        // Flag0x800 does not require notification during an in-range tick.
        arena.tick_original_model_animation(model, 40).unwrap();
        arena
            .set_original_model_animation_time(model, 0, 360)
            .unwrap();
        arena
            .graph
            .assign_position(model.root(), [3., 0., 0.])
            .unwrap();
        let root_before = *arena.state(model.root()).unwrap();
        assert!(matches!(
            arena.tick_original_model_animation(model, 40),
            Err(ModelAnimationTickError::Unsupported {
                feature: "animation boundary callback owner",
                ..
            })
        ));
        let slot = arena
            .original_model_controller(model)
            .unwrap()
            .unwrap()
            .state()
            .slots[0];
        assert_eq!(
            (slot.current, slot.previous, slot.last_delta),
            (400, 400, 40)
        );
        assert!(slot.enabled);
        assert_eq!(arena.state(keys[0]).unwrap().local.elements[12], 10.);
        assert_eq!(*arena.state(model.root()).unwrap(), root_before);
        arena.close(model).unwrap();
    }

    #[test]
    fn two_owned_ticks_refresh_actual_selected_skin_without_detached_pose_or_source_changes() {
        let SkinFixture {
            directory,
            mut arena,
            model,
            keys,
            source,
        } = skin_fixture();
        let visual = keys[0];
        let animation = moving(&[(b"first", 6)]);
        arena
            .set_original_model_animation(
                model,
                Some(&animation),
                0,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        arena.tick_original_model_animation(model, 0).unwrap();
        assert!(arena.owned_singlemesh_lod0_vertices(visual, 0).is_err());
        arena
            .prepare_owned_selected_singlemesh_lod(visual, 0)
            .unwrap();
        let first = arena.owned_singlemesh_lod0_vertices(visual, 0).unwrap()[0].position;
        arena.tick_original_model_animation(model, 80).unwrap();
        assert!(arena.owned_singlemesh_lod0_vertices(visual, 0).is_err());
        arena
            .prepare_owned_selected_singlemesh_lod(visual, 0)
            .unwrap();
        let second = arena.owned_singlemesh_lod0_vertices(visual, 0).unwrap()[0].position;
        assert_ne!(first, second);
        assert_eq!(arena.state(keys[1]).unwrap().local.elements[12], 2.);
        assert_eq!(source.frames[1].position, [4., 0., 0.]);
        assert_eq!(source.frames[0].lods[0].vertices[0].position, [1., 2., 3.]);
        assert_eq!(arena.original_model_animation_time(model, 0).unwrap(), 80);
        arena.publish_runtime_buffers().unwrap();
        arena.close(model).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_later_evaluator_keeps_clock_and_pose_prefix_but_cannot_present_old_skin_or_replay() {
        let SkinFixture {
            directory,
            mut arena,
            model,
            keys,
            ..
        } = skin_fixture();
        let visual = keys[0];
        assert!(arena.owned_singlemesh_lod0_vertices(visual, 0).is_ok());
        let animation = moving(&[(b"first", 2), (b"second", 8)]);
        arena
            .set_original_model_animation(
                model,
                Some(&animation),
                0,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        let root = *arena.state(model.root()).unwrap();
        assert!(matches!(
            arena.tick_original_model_animation(model, 40),
            Err(ModelAnimationTickError::Unsupported {
                feature: "scale channel evaluation",
                ..
            })
        ));
        assert_eq!(arena.state(keys[1]).unwrap().local.elements[12], 1.);
        assert_eq!(arena.state(keys[2]).unwrap().local.elements[12], 0.);
        assert_eq!(*arena.state(model.root()).unwrap(), root);
        let slot = arena
            .original_model_controller(model)
            .unwrap()
            .unwrap()
            .state()
            .slots[0];
        assert_eq!(
            (slot.current, slot.previous, slot.last_delta),
            (40, -40, 40)
        );
        let expected = ModelError::SkinPose(crate::singlemesh_pose::SkinPoseError::Source(
            "model animation pose update interrupted",
        ));
        assert_eq!(
            arena.owned_singlemesh_lod0_vertices(visual, 0),
            Err(expected.clone())
        );
        assert_eq!(
            arena.prepare_owned_selected_singlemesh_lod(visual, 0),
            Err(expected)
        );
        assert!(matches!(
            arena.tick_original_model_animation(model, 40),
            Err(ModelAnimationTickError::Interrupted)
        ));
        let after = arena
            .original_model_controller(model)
            .unwrap()
            .unwrap()
            .state()
            .slots[0];
        assert_eq!(
            (after.current, after.previous, after.last_delta),
            (slot.current, slot.previous, slot.last_delta)
        );
        arena.publish_runtime_buffers().unwrap();
        arena.close(model).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn note_dispatch_guard_is_reached_after_current_evaluator_destination_writes() {
        let SkinFixture {
            directory,
            mut arena,
            model,
            keys,
            ..
        } = skin_fixture();
        let mut source = moving_source(&[(b"first", 2)]);
        // File intake deliberately rejects unavailable note payloads. This
        // synthetic resource reaches the required callback-owner guard itself.
        Arc::make_mut(&mut source).tracks[0].flags |= 0x10;
        let animation = LiveAnimationResource::new(source, 2).unwrap();
        arena
            .set_original_model_animation(
                model,
                Some(&animation),
                0,
                1.,
                0,
                &mut Factory::default(),
            )
            .unwrap();
        arena.graph.change_flags(keys[1], 0, 0x200).unwrap();
        assert!(matches!(
            arena.tick_original_model_animation(model, 40),
            Err(ModelAnimationTickError::Unsupported {
                feature: "note callback/resource owner",
                ..
            })
        ));
        assert_eq!(arena.state(keys[1]).unwrap().local.elements[12], 1.);
        assert!(matches!(
            arena.owned_singlemesh_lod0_vertices(keys[0], 0),
            Err(ModelError::SkinPose(_))
        ));
        let slot = arena
            .original_model_controller(model)
            .unwrap()
            .unwrap()
            .state()
            .slots[0];
        assert_eq!((slot.current, slot.previous), (40, -40));
        arena.publish_runtime_buffers().unwrap();
        arena.close(model).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
