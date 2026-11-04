//! Scene-owned animation producer and controls. LS3DF SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! SetAnimation RVA 0x4b0f0 (differential: Ghidra body 54 insn/9 BB); scene
//! controls 0x4b1b0..0x4b390 (contract: Ghidra formed no body at those
//! entries, entry/mid/RET byte probes + mafia-re I3d_scene.cpp:857-980).
//! Mirrors model_animation_runtime.rs on the scene owner. Scene and model
//! controllers are distinct owners sharing only the IPolSet machinery: a
//! scene allocation never aliases a model allocation and vice versa.
//! Native failure words are preserved: 0x80000007 INVALIDPARAMS (slot),
//! 0x80000006 OBJECTNOTFOUND (absent controller), 0x80000002 OUTOFMEMORY.
//! Upstream working names (SetAnimation etc., mafia-re), not evidenced
//! original symbols.
#![forbid(unsafe_code)]

use crate::{
    animation_controller::{
        ControllerError, ControllerResourceBackend, IPolSet, LiveAnimationResource,
        LiveControllerResourceBackend,
    },
    frame_transform::{FrameKey, I3dFrameGraph},
    interpolator::{AllocationId, AnimSetId, EvaluatorId},
    model_animation_runtime::ANIMATION_OUT_OF_MEMORY,
    model_loading::{LoadedFrame, ModelArena, ModelError},
    scene_ownership::{SceneKey, SceneOwnerError},
    scene_tick::InterpolatorId,
};
use std::convert::Infallible;

/// Arena-scoped identity of the scene destination owner. Distinct from
/// ModelAnimationDestination: equal numeric roots across a model and a scene
/// never compare equal through this token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneAnimationDestination {
    scene: SceneKey,
    reference: crate::script_scheduler::FrameReferenceId,
}

/// Constructor-allocated scene owner. Box + IPolSet vectors are the
/// allocations; the host dirty seed (false) stands in for the native
/// constructor's unwritten dirty byte, exactly like the model twin.
#[derive(Debug)]
pub struct SceneControllerAllocation {
    destination: SceneAnimationDestination,
    controller: Box<IPolSet>,
    next_identity: u32,
    #[cfg(test)]
    fail_evaluator_after: Option<usize>,
}
impl SceneControllerAllocation {
    pub fn new(owner: SceneAnimationDestination, allocation_dirty: bool) -> Self {
        Self {
            destination: owner,
            controller: Box::new(IPolSet::new(owner.scene.root(), allocation_dirty)),
            next_identity: 0,
            #[cfg(test)]
            fail_evaluator_after: None,
        }
    }
    fn close(&mut self) {
        self.controller.close(&mut ReleaseSceneResources);
    }
}
impl Drop for SceneControllerAllocation {
    fn drop(&mut self) {
        self.close();
    }
}

#[derive(Debug)]
pub(crate) enum OriginalSceneController {
    /// A compatibility import without its resource owner remains unsupported.
    #[allow(dead_code)]
    External(InterpolatorId),
    Owned(SceneControllerAllocation),
}

/// Scene SetAnimation performs no ErrorHandler call on allocation failure:
/// the 0x4b0f0 body shows dbgPrintf + 0x80000002 only (differential vs the
/// model twin, which routes through allocation_failed). The factory is
/// therefore allocate-only.
pub trait SceneControllerFactory {
    type Error;
    fn allocate(
        &mut self,
        owner: SceneAnimationDestination,
    ) -> Result<Option<SceneControllerAllocation>, Self::Error>;
}

#[derive(Debug)]
pub enum SceneAnimationResourceError {
    Model(ModelError),
    IdentityExhausted,
    #[cfg(test)]
    NativeEvaluatorAllocation,
}
#[derive(Debug)]
pub enum SceneAnimationCallError<E> {
    Model(ModelError),
    Factory(E),
    ForeignControllerAllocation,
    Controller(ControllerError<SceneAnimationResourceError>),
}
impl<E> From<ModelError> for SceneAnimationCallError<E> {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

struct ReleaseSceneResources;
impl ControllerResourceBackend for ReleaseSceneResources {
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

struct LiveSceneResources<'a> {
    graph: &'a mut I3dFrameGraph,
    frames: &'a mut Vec<LoadedFrame>,
    members: &'a mut Vec<FrameKey>,
    next_identity: &'a mut u32,
    #[cfg(test)]
    fail_evaluator_after: &'a mut Option<usize>,
}
impl LiveSceneResources<'_> {
    fn identity(&mut self) -> Result<u32, SceneAnimationResourceError> {
        *self.next_identity = self
            .next_identity
            .checked_add(1)
            .ok_or(SceneAnimationResourceError::IdentityExhausted)?;
        Ok(*self.next_identity)
    }
}
impl ControllerResourceBackend for LiveSceneResources<'_> {
    type Error = SceneAnimationResourceError;
    fn allocate_evaluator(&mut self) -> Result<EvaluatorId, Self::Error> {
        #[cfg(test)]
        if let Some(remaining) = self.fail_evaluator_after.as_mut() {
            if *remaining == 0 {
                *self.fail_evaluator_after = None;
                return Err(SceneAnimationResourceError::NativeEvaluatorAllocation);
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
    fn retain_animation(&mut self, _: AnimSetId) {}
    fn release_animation(&mut self, _: AnimSetId) {}
    fn mark_animated(&mut self, destination: FrameKey) {
        self.graph
            .mark_animated(destination)
            .expect("resolved live destination");
    }
}
fn scene_c_name(name: &[u8]) -> &[u8] {
    &name[..name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len())]
}
impl LiveControllerResourceBackend for LiveSceneResources<'_> {
    fn first_frame_named(&mut self, name: &[u8]) -> Result<Option<(usize, FrameKey)>, Self::Error> {
        for (index, &key) in self.members.iter().enumerate() {
            let frame = key
                .0
                .checked_sub(1)
                .and_then(|slot| self.frames.get(slot as usize))
                .filter(|frame| frame.key == key && frame.is_alive())
                .ok_or(SceneAnimationResourceError::Model(
                    ModelError::AnimationIdentity,
                ))?;
            let local = frame.original_name_bytes.as_deref().unwrap_or(b"");
            if matches!(
                frame.name_pointer,
                crate::frame_fast_lookup::OriginalFrameName::Released { .. }
            ) {
                return Err(SceneAnimationResourceError::Model(
                    ModelError::UnavailableOriginalName(key),
                ));
            }
            if scene_c_name(local) == scene_c_name(name) {
                self.graph
                    .state(key)
                    .map_err(|error| SceneAnimationResourceError::Model(error.into()))?;
                return Ok(Some((index, key)));
            }
        }
        Ok(None)
    }
}

/// Guard-only remap for the fallible word-returning scene setters: native has
/// no Closed/Poisoned/InvalidView branches, so these surface as Err, never as
/// words — same rule as all fallible model pose paths.
fn scene_guard<E>(
    result: Result<(), ControllerError<Infallible>>,
) -> Result<(), ControllerError<E>> {
    result.map_err(|guard| match guard {
        ControllerError::Closed => ControllerError::Closed,
        ControllerError::Poisoned => ControllerError::Poisoned,
        ControllerError::InvalidView => ControllerError::InvalidView,
        _ => unreachable!("scene slot-guard only"),
    })
}

impl ModelArena {
    pub fn original_scene_controller(
        &self,
        scene: SceneKey,
    ) -> Result<Option<&IPolSet>, ModelError> {
        let si = self.scene_index(scene)?;
        match &self.scenes[si].scene_animation {
            None => Ok(None),
            Some(OriginalSceneController::Owned(allocation)) => Ok(Some(&allocation.controller)),
            Some(OriginalSceneController::External(_)) => {
                Err(SceneOwnerError::UnsupportedActiveSceneController(scene).into())
            }
        }
    }
    pub(crate) fn original_scene_controller_mut(
        &mut self,
        scene: SceneKey,
    ) -> Result<Option<&mut IPolSet>, ModelError> {
        let si = self.scene_index(scene)?;
        match &mut self.scenes[si].scene_animation {
            None => Ok(None),
            Some(OriginalSceneController::Owned(allocation)) => {
                Ok(Some(&mut allocation.controller))
            }
            Some(OriginalSceneController::External(_)) => {
                Err(SceneOwnerError::UnsupportedActiveSceneController(scene).into())
            }
        }
    }
    /// Lifecycle helper (no function credit): Close/DeleteInterpolation
    /// hookup stays with the Close-effect owner.
    pub(crate) fn close_original_scene_controller(
        &mut self,
        scene: SceneKey,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        match &mut self.scenes[si].scene_animation {
            None => return Ok(()),
            Some(OriginalSceneController::External(_)) => {
                return Err(SceneOwnerError::UnsupportedActiveSceneController(scene).into());
            }
            Some(OriginalSceneController::Owned(allocation)) => allocation.close(),
        }
        self.scenes[si].scene_animation = None;
        Ok(())
    }
    /// Lifecycle helper (no function credit): releases slot references and
    /// zeroes slots; storage/capacity/dirty kept per Clear semantics.
    pub fn clear_original_scene_animation(&mut self, scene: SceneKey) -> Result<(), ModelError> {
        if let Some(controller) = self.original_scene_controller_mut(scene)? {
            controller.clear(&mut ReleaseSceneResources);
        }
        Ok(())
    }

    /// RVA 0x4b0f0 SetAnimation (LS3DF.dll e211c850…): Ghidra body
    /// FUN_1004b0f0 (54 insns, 9 BBs) -> tier differential. Bounds word
    /// BEFORE lazy alloc; 0xf8 owner at scene+0x178 with the model-twin ctor
    /// shape; null-alloc -> 0x80000002 with NO handler call (allocate-only
    /// factory); else the shared delegate HRESULT. check_owner_available is
    /// reused host admission without native evidence.
    #[allow(clippy::too_many_arguments)]
    pub fn set_original_scene_animation<F: SceneControllerFactory>(
        &mut self,
        scene: SceneKey,
        resource: Option<&LiveAnimationResource>,
        index: u32,
        weight: f32,
        mode: i32,
        factory: &mut F,
    ) -> Result<u32, SceneAnimationCallError<F::Error>> {
        if index >= 8 {
            return Ok(0x8000_0007);
        }
        let si = self.scene_index(scene)?;
        self.check_owner_available(scene.into())?;
        if matches!(
            self.scenes[si].scene_animation,
            Some(OriginalSceneController::External(_))
        ) {
            return Err(SceneAnimationCallError::Model(
                SceneOwnerError::UnsupportedActiveSceneController(scene).into(),
            ));
        }
        if self.scenes[si].scene_animation.is_none() {
            let destination = SceneAnimationDestination {
                scene,
                reference: self.lookup_reference(scene.root()),
            };
            let allocation = factory
                .allocate(destination)
                .map_err(SceneAnimationCallError::Factory)?;
            if allocation
                .as_ref()
                .is_some_and(|allocation| allocation.destination != destination)
            {
                return Err(SceneAnimationCallError::ForeignControllerAllocation);
            }
            // Publish-before-delegate, mirroring the native store-then-test:
            // None stays genuinely absent and reports OOM with no handler.
            self.scenes[si].scene_animation = allocation.map(OriginalSceneController::Owned);
            if self.scenes[si].scene_animation.is_none() {
                return Ok(ANIMATION_OUT_OF_MEMORY);
            }
        }
        let loaded = &mut self.scenes[si];
        let Some(OriginalSceneController::Owned(allocation)) = &mut loaded.scene_animation else {
            unreachable!("published actual owner")
        };
        let result = allocation.controller.set_live_animation(
            &mut LiveSceneResources {
                graph: &mut self.graph,
                frames: &mut self.frames,
                members: &mut loaded.frames,
                next_identity: &mut allocation.next_identity,
                #[cfg(test)]
                fail_evaluator_after: &mut allocation.fail_evaluator_after,
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
                SceneAnimationResourceError::NativeEvaluatorAllocation,
            )) => Ok(ANIMATION_OUT_OF_MEMORY),
            Err(error) => Err(SceneAnimationCallError::Controller(error)),
        }
    }

    /// RVA 0x4b1b0 GetAnimationSet: OOB/null/empty -> None, else the slot
    /// resource identity. No Ghidra body -> contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word. Pure read. A Host
    /// variant (unreachable by construction: scene slots only ever hold Live
    /// resources) projects to None — disclosed, never a pointer.
    pub fn original_scene_animation_set(
        &self,
        scene: SceneKey,
        index: u32,
    ) -> Result<Option<AnimSetId>, ModelError> {
        if index >= 8 {
            self.scene_index(scene)?;
            return Ok(None);
        }
        Ok(self
            .original_scene_controller(scene)?
            .and_then(|controller| controller.animation_id(index as usize)))
    }

    /// RVA 0x4b1e0 StopAnimation: silent ignore OOB/null; else on=false with
    /// no dirty effect. No Ghidra body -> contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word.
    pub fn stop_original_scene_animation(
        &mut self,
        scene: SceneKey,
        index: u32,
    ) -> Result<(), ModelError> {
        self.scene_index(scene)?;
        if index < 8 {
            if let Some(controller) = self.original_scene_controller_mut(scene)? {
                controller.stop_animation(index as usize);
            }
        }
        Ok(())
    }

    /// RVA 0x4b210 SetAnimBlend: OOB -> 0x80000007, null -> 0x80000006, else
    /// the ordered x87 [0,1] clamp (NaN quieted/passthrough per the byte
    /// probe), store + changed=1 -> 0. No Ghidra body -> contract. Blend math
    /// reuses set_anim_blend bit-for-bit. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word. Guard failures (Closed/Poisoned/
    /// InvalidView — host-only, no native branch) surface as Err, never words.
    pub fn set_original_scene_animation_blend(
        &mut self,
        scene: SceneKey,
        index: u32,
        blend: f32,
    ) -> Result<u32, SceneAnimationCallError<Infallible>> {
        if index >= 8 {
            self.scene_index(scene)?;
            return Ok(0x8000_0007);
        }
        self.check_owner_available(scene.into())?;
        let Some(controller) = self.original_scene_controller_mut(scene)? else {
            return Ok(0x8000_0006);
        };
        scene_guard(controller.set_anim_blend(index as usize, blend))
            .map_err(SceneAnimationCallError::Controller)?;
        Ok(0)
    }

    /// RVA 0x4b290 GetAnimBlend: OOB/null -> 0.0, else stored blend.
    /// No Ghidra body -> contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word.
    pub fn original_scene_animation_blend(
        &self,
        scene: SceneKey,
        index: u32,
    ) -> Result<f32, ModelError> {
        if index >= 8 {
            self.scene(scene)?;
            return Ok(0.);
        }
        Ok(self
            .original_scene_controller(scene)?
            .map_or(0., |controller| controller.anim_blend(index as usize)))
    }

    /// RVA 0x4b2d0 SetAnimTime: OOB -> 0x80000007, null -> 0x80000006, else
    /// time AND last_time store (bytes prove the >=395 branch is compiled
    /// in) + changed=1 -> 0. No Ghidra body -> contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word.
    pub fn set_original_scene_animation_time(
        &mut self,
        scene: SceneKey,
        index: u32,
        time: i32,
    ) -> Result<u32, SceneAnimationCallError<Infallible>> {
        if index >= 8 {
            self.scene_index(scene)?;
            return Ok(0x8000_0007);
        }
        self.check_owner_available(scene.into())?;
        let Some(controller) = self.original_scene_controller_mut(scene)? else {
            return Ok(0x8000_0006);
        };
        scene_guard(controller.set_anim_time(index as usize, time))
            .map_err(SceneAnimationCallError::Controller)?;
        Ok(0)
    }

    /// RVA 0x4b310 GetAnimTime: OOB/null -> 0. No Ghidra body -> contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word.
    pub fn original_scene_animation_time(
        &self,
        scene: SceneKey,
        index: u32,
    ) -> Result<i32, ModelError> {
        if index >= 8 {
            self.scene(scene)?;
            return Ok(0);
        }
        Ok(self
            .original_scene_controller(scene)?
            .map_or(0, |controller| controller.anim_time(index as usize)))
    }

    /// RVA 0x4b340 AnimIsOn: OOB/null -> false, else the slot on-byte.
    /// No Ghidra body -> contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word. Direct state read like the model twin.
    pub fn original_scene_animation_is_on(
        &self,
        scene: SceneKey,
        index: u32,
    ) -> Result<bool, ModelError> {
        self.scene_index(scene)?;
        if index >= 8 {
            return Ok(false);
        }
        Ok(self
            .original_scene_controller(scene)?
            .is_some_and(|controller| {
                controller
                    .state()
                    .slots
                    .get(index as usize)
                    .is_some_and(|slot| slot.enabled)
            }))
    }

    /// RVA 0x4b370 NumInterpolators: null -> 0 else count word.
    /// No Ghidra body -> contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word.
    pub fn original_scene_num_interpolators(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self
            .original_scene_controller(scene)?
            .map_or(0, |controller| controller.num_interpolators()))
    }

    /// RVA 0x4b390 GetInterpolators: None = no set (native NULL); Some(view)
    /// = live storage base; an empty set yields Some(&[]). No Ghidra body ->
    /// contract. Host admission: the scene is validated even on the OOB arm (native touches only index+ipol_set); a dead scene errors instead of the null word. Borrow-checked view instead of the native storage pointer.
    pub fn original_scene_interpolators(
        &self,
        scene: SceneKey,
    ) -> Result<Option<&[EvaluatorId]>, ModelError> {
        Ok(self
            .original_scene_controller(scene)?
            .and_then(|controller| controller.interpolator_storage()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lhr_assets::animation::{AnimationSet, AnimationTrack};
    use std::sync::Arc;

    #[derive(Default)]
    struct Factory {
        dirty: bool,
        null: bool,
        fail_evaluator_after: Option<usize>,
        calls: usize,
    }
    impl SceneControllerFactory for Factory {
        type Error = &'static str;
        fn allocate(
            &mut self,
            owner: SceneAnimationDestination,
        ) -> Result<Option<SceneControllerAllocation>, Self::Error> {
            self.calls += 1;
            if self.null {
                return Ok(None);
            }
            let mut allocation = SceneControllerAllocation::new(owner, self.dirty);
            allocation.fail_evaluator_after = self.fail_evaluator_after;
            Ok(Some(allocation))
        }
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
    fn scene_with_frames(arena: &mut ModelArena, names: &[&[u8]]) -> (SceneKey, Vec<FrameKey>) {
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let mut keys = Vec::new();
        for name in names {
            let key = arena
                .insert(
                    crate::frame_transform::I3dFrameState::dummy(),
                    None,
                    Vec::new(),
                    Vec::new(),
                    None,
                )
                .unwrap();
            arena.set_frame_name(key, Some(name)).unwrap();
            arena.scene_add_frame(scene, key).unwrap();
            arena.release(key).unwrap();
            keys.push(key);
        }
        (scene, keys)
    }

    #[test]
    fn set_animation_bounds_word_precedes_lazy_owner_and_null_reports_oom() {
        let mut arena = ModelArena::default();
        let (scene, _) = scene_with_frames(&mut arena, &[b"a"]);
        let mut factory = Factory::default();
        assert_eq!(
            arena
                .set_original_scene_animation(scene, None, 8, 1., 0, &mut factory)
                .unwrap(),
            0x8000_0007
        );
        assert_eq!(factory.calls, 0);
        assert!(arena.original_scene_controller(scene).unwrap().is_none());
        assert_eq!(
            arena
                .set_original_scene_animation(scene, None, 0, 1., 0, &mut factory)
                .unwrap(),
            0
        );
        assert_eq!(factory.calls, 1);
        let controller = arena.original_scene_controller(scene).unwrap().unwrap();
        assert_eq!(controller.owner(), scene.root());
        assert!(!controller.state().dirty);
        factory.null = true;
        let mut arena2 = ModelArena::default();
        let (scene2, _) = scene_with_frames(&mut arena2, &[b"a"]);
        assert_eq!(
            arena2
                .set_original_scene_animation(scene2, None, 0, 1., 0, &mut factory)
                .unwrap(),
            ANIMATION_OUT_OF_MEMORY
        );
        assert!(arena2.original_scene_controller(scene2).unwrap().is_none());
    }

    #[test]
    fn foreign_allocation_is_rejected_without_publishing() {
        let mut arena = ModelArena::default();
        let (scene, _) = scene_with_frames(&mut arena, &[b"a"]);
        let other = arena.construct_scene_owner_prefix().unwrap();
        let other_reference = arena.lookup_reference(other.root());
        struct Foreign {
            scene: SceneKey,
            reference: crate::script_scheduler::FrameReferenceId,
        }
        impl SceneControllerFactory for Foreign {
            type Error = &'static str;
            fn allocate(
                &mut self,
                _: SceneAnimationDestination,
            ) -> Result<Option<SceneControllerAllocation>, Self::Error> {
                // Answers for another scene than the requested one.
                Ok(Some(SceneControllerAllocation::new(
                    SceneAnimationDestination {
                        scene: self.scene,
                        reference: self.reference,
                    },
                    false,
                )))
            }
        }
        let mut foreign = Foreign {
            scene: other,
            reference: other_reference,
        };
        assert!(matches!(
            arena.set_original_scene_animation(scene, None, 0, 1., 0, &mut foreign),
            Err(SceneAnimationCallError::ForeignControllerAllocation)
        ));
        assert!(arena.original_scene_controller(scene).unwrap().is_none());
        // A factory error surfaces as Factory, never as a word.
        struct Failing;
        impl SceneControllerFactory for Failing {
            type Error = &'static str;
            fn allocate(
                &mut self,
                _: SceneAnimationDestination,
            ) -> Result<Option<SceneControllerAllocation>, Self::Error> {
                Err("backend unavailable")
            }
        }
        assert!(matches!(
            arena.set_original_scene_animation(scene, None, 0, 1., 0, &mut Failing),
            Err(SceneAnimationCallError::Factory(_))
        ));
        assert!(arena.original_scene_controller(scene).unwrap().is_none());
    }

    #[test]
    fn slot_bounds_null_words_and_silent_stop() {
        let mut arena = ModelArena::default();
        let (scene, _) = scene_with_frames(&mut arena, &[b"a"]);
        let mut factory = Factory::default();
        for index in [0, 7, 8, 99, u32::MAX] {
            assert_eq!(
                arena.original_scene_animation_set(scene, index).unwrap(),
                None
            );
            assert_eq!(
                arena.original_scene_animation_blend(scene, index).unwrap(),
                0.
            );
            assert_eq!(
                arena.original_scene_animation_time(scene, index).unwrap(),
                0
            );
            assert!(!arena.original_scene_animation_is_on(scene, index).unwrap());
            arena.stop_original_scene_animation(scene, index).unwrap();
        }
        assert!(arena.original_scene_interpolators(scene).unwrap().is_none());
        assert_eq!(arena.original_scene_num_interpolators(scene).unwrap(), 0);
        assert_eq!(
            arena
                .set_original_scene_animation_blend(scene, 0, 0.5)
                .unwrap(),
            0x8000_0006
        );
        assert_eq!(
            arena
                .set_original_scene_animation_time(scene, 0, 9)
                .unwrap(),
            0x8000_0006
        );
        assert_eq!(
            arena
                .set_original_scene_animation_blend(scene, 8, 0.5)
                .unwrap(),
            0x8000_0007
        );
        assert_eq!(
            arena
                .set_original_scene_animation_time(scene, 8, 9)
                .unwrap(),
            0x8000_0007
        );
        arena
            .set_original_scene_animation(scene, None, 0, 1., 0, &mut factory)
            .unwrap();
        assert_eq!(
            arena
                .set_original_scene_animation_blend(scene, 8, 0.5)
                .unwrap(),
            0x8000_0007
        );
        arena.stop_original_scene_animation(scene, 8).unwrap();
    }

    #[test]
    fn blend_bit_exactness_matches_x87_clamp_path() {
        let mut arena = ModelArena::default();
        let (scene, _) = scene_with_frames(&mut arena, &[b"a"]);
        let mut factory = Factory::default();
        let anim = resource(&[b"a"], 2);
        arena
            .set_original_scene_animation(scene, Some(&anim), 0, 1., 0, &mut factory)
            .unwrap();
        for (input, stored) in [
            (0x3f800000u32, 0x3f800000u32),
            (0x40000000, 0x3f800000),
            (0xbf800000, 0x00000000),
            (0x80000000, 0x80000000),
            (0x00000000, 0x00000000),
            (0x7f812345, 0x7fc12345),
            (0x7fc12345, 0x7fc12345),
            (0x7f800000, 0x3f800000),
            (0xff800000, 0x00000000),
        ] {
            arena
                .set_original_scene_animation_blend(scene, 0, f32::from_bits(input))
                .unwrap();
            assert_eq!(
                arena
                    .original_scene_animation_blend(scene, 0)
                    .unwrap()
                    .to_bits(),
                stored,
                "input {input:#x}"
            );
        }
        let before = arena
            .original_scene_animation_blend(scene, 0)
            .unwrap()
            .to_bits();
        assert_eq!(
            arena
                .set_original_scene_animation_blend(scene, 1, 0.25)
                .unwrap(),
            0
        );
        assert_eq!(
            arena
                .original_scene_animation_blend(scene, 0)
                .unwrap()
                .to_bits(),
            before
        );
    }

    #[test]
    fn time_writes_both_clocks_and_is_on_follows_stop() {
        let mut arena = ModelArena::default();
        let (scene, _) = scene_with_frames(&mut arena, &[b"a"]);
        let mut factory = Factory::default();
        let anim = resource(&[b"a"], 2);
        arena
            .set_original_scene_animation(scene, Some(&anim), 3, 1., 0, &mut factory)
            .unwrap();
        assert!(arena.original_scene_animation_is_on(scene, 3).unwrap());
        assert_eq!(
            arena
                .set_original_scene_animation_time(scene, 3, -40)
                .unwrap(),
            0
        );
        assert_eq!(arena.original_scene_animation_time(scene, 3).unwrap(), -40);
        let controller = arena.original_scene_controller(scene).unwrap().unwrap();
        assert_eq!(controller.state().slots[3].current, -40);
        assert_eq!(controller.state().slots[3].previous, -40);
        arena.stop_original_scene_animation(scene, 3).unwrap();
        assert!(!arena.original_scene_animation_is_on(scene, 3).unwrap());
        assert_eq!(arena.original_scene_animation_time(scene, 3).unwrap(), -40);
    }

    #[test]
    fn scene_and_model_controllers_stay_independent() {
        let mut arena = ModelArena::default();
        let (scene, _) = scene_with_frames(&mut arena, &[b"a"]);
        let model = arena.create_model().unwrap();
        let mut factory = Factory::default();
        let anim = resource(&[b"a"], 2);
        arena
            .set_original_scene_animation(scene, Some(&anim), 0, 0.5, 0, &mut factory)
            .unwrap();
        struct ModelFactory;
        impl crate::model_animation_runtime::ModelControllerFactory for ModelFactory {
            type Error = std::convert::Infallible;
            fn allocate(
                &mut self,
                owner: crate::model_animation_runtime::ModelAnimationDestination,
            ) -> Result<
                Option<crate::model_animation_runtime::ModelControllerAllocation>,
                Self::Error,
            > {
                Ok(Some(
                    crate::model_animation_runtime::ModelControllerAllocation::new(owner, false),
                ))
            }
            fn allocation_failed(
                &mut self,
                _: crate::model_animation_runtime::ModelControllerAllocationFailure,
            ) -> Result<(), Self::Error> {
                Ok(())
            }
        }
        arena
            .set_original_model_animation(model, None, 0, 1., 0, &mut ModelFactory)
            .unwrap();
        arena
            .set_original_scene_animation_blend(scene, 0, 0.25)
            .unwrap();
        arena
            .set_original_model_animation_blend(model, 0, 0.75)
            .unwrap();
        assert_eq!(
            arena.original_scene_animation_blend(scene, 0).unwrap(),
            0.25
        );
        assert_eq!(
            arena.original_model_animation_blend(model, 0).unwrap(),
            0.75
        );
        assert_ne!(
            arena
                .original_scene_controller(scene)
                .unwrap()
                .unwrap()
                .owner(),
            arena
                .original_model_controller(model)
                .unwrap()
                .unwrap()
                .owner()
        );
    }

    #[test]
    fn residue_poison_and_close_discipline() {
        let mut arena = ModelArena::default();
        let (scene, _) = scene_with_frames(&mut arena, &[b"a"]);
        let mut factory = Factory {
            fail_evaluator_after: Some(0),
            ..Factory::default()
        };
        let anim = resource(&[b"a"], 2);
        // The failing evaluator allocation reports the OOM word; the
        // published owner stays present (delegate failure preserves it).
        assert_eq!(
            arena
                .set_original_scene_animation(scene, Some(&anim), 0, 1., 0, &mut factory)
                .unwrap(),
            ANIMATION_OUT_OF_MEMORY
        );
        arena.clear_original_scene_animation(scene).unwrap();
        assert_eq!(arena.original_scene_animation_blend(scene, 0).unwrap(), 0.);
        arena.close_original_scene_controller(scene).unwrap();
        assert!(arena.original_scene_controller(scene).unwrap().is_none());
        assert!(arena.original_scene_interpolators(scene).unwrap().is_none());
        assert_eq!(arena.original_scene_num_interpolators(scene).unwrap(), 0);
        assert_eq!(arena.original_scene_animation_blend(scene, 0).unwrap(), 0.);
    }

    #[test]
    fn membership_binding_resolves_scene_frames_and_reports_sets() {
        let mut arena = ModelArena::default();
        let (scene, keys) = scene_with_frames(&mut arena, &[b"first", b"second"]);
        let mut factory = Factory::default();
        let anim = resource(&[b"first", b"missing"], 7);
        arena
            .set_original_scene_animation(scene, Some(&anim), 2, 0.5, 0, &mut factory)
            .unwrap();
        let id = arena
            .original_scene_animation_set(scene, 2)
            .unwrap()
            .unwrap();
        assert_eq!(id, anim.id());
        assert!(arena.original_scene_animation_is_on(scene, 2).unwrap());
        assert_eq!(arena.original_scene_num_interpolators(scene).unwrap(), 1);
        let base = arena.original_scene_interpolators(scene).unwrap().unwrap();
        assert_eq!(base.len(), 1);
        let evaluator = arena
            .original_scene_controller(scene)
            .unwrap()
            .unwrap()
            .evaluator(base[0])
            .unwrap();
        assert_eq!(evaluator.destination, keys[0]);
        assert_eq!(arena.original_scene_animation_set(scene, 0).unwrap(), None);
    }
}
