//! Model-owned animation producer and controls. LS3DF SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! SetAnimation RVA0x337b0; model controls0x33860..0x33a60.
//! Original allocation boundaries remain explicit. No Game selection, loader,
//! evaluator Tick, scene scheduling or multi-slot evaluation is supplied here.
#![forbid(unsafe_code)]

use crate::{
    animation_controller::{
        ControllerError, ControllerResourceBackend, IPolSet, LiveAnimationResource,
        LiveControllerResourceBackend,
    },
    frame_transform::{FrameKey, I3dFrameGraph},
    interpolator::{AllocationId, AnimSetId, EvaluatorId},
    model_loading::{LoadedFrame, ModelArena, ModelError, ModelKey},
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
}
