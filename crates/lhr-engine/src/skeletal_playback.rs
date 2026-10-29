//! Host ownership adapter for recovered ModelTick, IPolTick and one-slot
//! position/rotation evaluation. Allocation IDs and initial dirty storage are
//! host policy. No blending, note events or resource loader is substituted.
use crate::{
    animation_controller::{AnimationResource, ControllerResourceBackend, IPolSet},
    frame_transform::FrameKey,
    interpolator::{
        AllocationId, AnimSetId, EvaluatorId, InterpolatorBackend, InterpolatorSet, TickError,
    },
    model_loading::{MaterializedModel, ModelArena, ModelError, SharedAnimationModel},
    model_tick::{ModelBackend, ModelTickState, ModelVTableId},
    rotation_animation::sample_rotation_channel,
    scene_tick::{FrameId, InterpolatorId},
    skeletal_animation::{SampledFrame, SkeletalAnimation},
};
use lhr_assets::{animation::AnimationSet, Model};
use std::{fmt, sync::Arc};

mod sealed {
    pub trait Target {}
    impl Target for super::MaterializedModel {}
    impl Target for super::SharedAnimationModel<'_> {}
}
/// Actual mutable model owners admitted by explicit host playback. This does
/// not install the original model controller or choose a game animation.
pub trait SkeletalPlaybackModel: sealed::Target {
    fn playback_mapping(&self) -> Result<(Arc<Model>, Vec<FrameKey>), ModelError>;
    fn playback_arena(&self) -> &ModelArena;
    fn playback_root(&self) -> FrameKey;
    fn playback_epoch(&self) -> Result<u64, ModelError>;
    fn playback_mark_animated(&mut self, frame: FrameKey) -> Result<(), ModelError>;
    fn playback_apply(&mut self, sample: SampledFrame) -> Result<(), ModelError>;
    fn playback_update(&mut self) -> Result<(), ModelError>;
    fn playback_prepare_pose(&mut self) -> Result<(), ModelError>;
}
impl SkeletalPlaybackModel for MaterializedModel {
    fn playback_mapping(&self) -> Result<(Arc<Model>, Vec<FrameKey>), ModelError> {
        if !self.is_complete() {
            return Err(ModelError::IncompleteModel);
        }
        Ok((Arc::clone(self.source()), self.source_frame_keys().to_vec()))
    }
    fn playback_arena(&self) -> &ModelArena {
        self.arena()
    }
    fn playback_root(&self) -> FrameKey {
        self.root()
    }
    fn playback_epoch(&self) -> Result<u64, ModelError> {
        self.lifecycle_epoch()
    }
    fn playback_mark_animated(&mut self, frame: FrameKey) -> Result<(), ModelError> {
        self.mark_animated(frame)
    }
    fn playback_apply(&mut self, sample: SampledFrame) -> Result<(), ModelError> {
        self.apply_animation_sample(sample)
    }
    fn playback_update(&mut self) -> Result<(), ModelError> {
        self.update_animation_model()
    }
    fn playback_prepare_pose(&mut self) -> Result<(), ModelError> {
        self.prepare_skin_pose()
    }
}
impl SkeletalPlaybackModel for SharedAnimationModel<'_> {
    fn playback_mapping(&self) -> Result<(Arc<Model>, Vec<FrameKey>), ModelError> {
        self.source_mapping()
    }
    fn playback_arena(&self) -> &ModelArena {
        self.arena()
    }
    fn playback_root(&self) -> FrameKey {
        self.root()
    }
    fn playback_epoch(&self) -> Result<u64, ModelError> {
        self.lifecycle_epoch()
    }
    fn playback_mark_animated(&mut self, frame: FrameKey) -> Result<(), ModelError> {
        self.mark_animated(frame)
    }
    fn playback_apply(&mut self, sample: SampledFrame) -> Result<(), ModelError> {
        self.apply_animation_sample(sample)
    }
    fn playback_update(&mut self) -> Result<(), ModelError> {
        self.update_animation_model()
    }
    fn playback_prepare_pose(&mut self) -> Result<(), ModelError> {
        self.prepare_skin_pose()
    }
}

#[derive(Debug)]
pub enum SkeletalPlaybackError {
    Unsupported(&'static str),
    Identity,
    Poisoned,
    Model(ModelError),
    Sampling(String),
    Controller(String),
    Tick(TickError),
    AllocationOverflow,
}
impl fmt::Display for SkeletalPlaybackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "skeletal playback: {self:?}")
    }
}
impl std::error::Error for SkeletalPlaybackError {}
impl From<ModelError> for SkeletalPlaybackError {
    fn from(value: ModelError) -> Self {
        Self::Model(value)
    }
}

#[derive(Debug)]
pub struct SkeletalPlayback {
    source: Arc<Model>,
    identity: crate::script_scheduler::FrameReferenceId,
    lifecycle_epoch: u64,
    binding: SkeletalAnimation,
    controller: IPolSet,
    native: ModelTickState,
    poisoned: bool,
}

// No native allocations escape this adapter: IPolSet owns Rust vectors and
// Arc references. IDs model their identity; free/retain/release callbacks are
// intentionally bookkeeping-free because the owning Rust fields perform them.
struct Resources<'a> {
    model: Option<&'a mut dyn SkeletalPlaybackModel>,
    next: u32,
}
impl ControllerResourceBackend for Resources<'_> {
    type Error = SkeletalPlaybackError;
    fn allocate_evaluator(&mut self) -> Result<EvaluatorId, Self::Error> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or(SkeletalPlaybackError::AllocationOverflow)?;
        Ok(EvaluatorId(self.next))
    }
    fn free_evaluator(&mut self, _: EvaluatorId) {}
    fn allocate_storage(&mut self, _: usize) -> Result<AllocationId, Self::Error> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or(SkeletalPlaybackError::AllocationOverflow)?;
        Ok(AllocationId(self.next))
    }
    fn free_storage(&mut self, _: AllocationId) {}
    fn retain_animation(&mut self, _: AnimSetId) {}
    fn release_animation(&mut self, _: AnimSetId) {}
    fn mark_animated(&mut self, destination: FrameKey) {
        // All keys come from this complete materialized model, preflighted
        // before entering the native-order resource transition.
        self.model
            .as_mut()
            .expect("binding resource context")
            .playback_mark_animated(destination)
            .expect("validated destination");
    }
}
impl SkeletalPlayback {
    /// Bind slot0 with unit weight and explicit loop mode2. This is a host
    /// playback request, not an inferred game animation selection/default.
    pub fn new(
        model: &mut impl SkeletalPlaybackModel,
        animation: Arc<AnimationSet>,
    ) -> Result<Self, SkeletalPlaybackError> {
        if animation.duration_ms() == 0 {
            return Err(SkeletalPlaybackError::Unsupported(
                "complete model and positive animation duration required",
            ));
        }
        let (source, keys) = model.playback_mapping()?;
        let binding = SkeletalAnimation::new(&source, Arc::clone(&animation), &keys)
            .map_err(|e| SkeletalPlaybackError::Sampling(format!("{e:?}")))?;
        if binding.bindings().is_empty() {
            return Err(SkeletalPlaybackError::Unsupported(
                "no matching animation channels",
            ));
        }
        for binding in binding.bindings() {
            if model.playback_arena().state(binding.frame)?.flags & 0x800 != 0 {
                return Err(SkeletalPlaybackError::Unsupported(
                    "animation boundary notification backend required",
                ));
            }
        }
        let identity = model
            .playback_arena()
            .lookup_reference(model.playback_root());
        let lifecycle_epoch = model.playback_epoch()?;
        let mut controller = IPolSet::new(model.playback_root(), false);
        let resource = AnimationResource {
            id: AnimSetId(1),
            source: animation,
            default_mode: 2,
        };
        controller
            .set_animation(
                &mut Resources {
                    model: Some(model),
                    next: 0,
                },
                &source,
                &keys,
                Some(&resource),
                0,
                1.,
                2,
            )
            .map_err(|e| SkeletalPlaybackError::Controller(format!("{e:?}")))?;
        let mut result = Self {
            source,
            identity,
            lifecycle_epoch,
            binding,
            controller,
            native: ModelTickState {
                vtable: ModelVTableId(0x1009c160),
                interpolator: Some(InterpolatorId(1)),
            },
            poisoned: false,
        };
        result.tick(model, 0)?;
        Ok(result)
    }
    pub fn source(&self) -> &Arc<Model> {
        &self.source
    }
    pub fn duration_ms(&self) -> u32 {
        self.binding.duration_ms()
    }
    pub fn controller(&self) -> &IPolSet {
        &self.controller
    }
    pub fn validate_model(
        &self,
        model: &dyn SkeletalPlaybackModel,
    ) -> Result<(), SkeletalPlaybackError> {
        if self.poisoned {
            return Err(SkeletalPlaybackError::Poisoned);
        }
        if self.identity
            != model
                .playback_arena()
                .lookup_reference(model.playback_root())
            || model.playback_epoch()? != self.lifecycle_epoch
        {
            return Err(SkeletalPlaybackError::Identity);
        }
        let (source, keys) = model
            .playback_mapping()
            .map_err(|_| SkeletalPlaybackError::Identity)?;
        if !Arc::ptr_eq(&self.source, &source)
            || self
                .binding
                .bindings()
                .iter()
                .any(|binding| keys.get(binding.frame_index) != Some(&binding.frame))
        {
            return Err(SkeletalPlaybackError::Identity);
        }
        // Public arena operations may retire a destination without replacing
        // the parsed source. Validate every binding before advancing the clock
        // or writing an earlier destination in the evaluator sequence.
        for binding in self.binding.bindings() {
            model.playback_arena().frame(binding.frame)?;
        }
        Ok(())
    }
    /// Consume this host playback adapter before the admitted model Close.
    /// Original Model::Close (RVA 0x33500) deletes its controller before releasing
    /// owned frames. This adapter owns that controller separately; consuming it
    /// here enforces the same ordering. Unavailable derived destruction remains
    /// an error, with any completed Close prefix retained by the model.
    pub fn close_model(self, model: &mut MaterializedModel) -> Result<(), SkeletalPlaybackError> {
        self.validate_model(model)?;
        drop(self);
        model.close()?;
        Ok(())
    }
    /// Release playback first, then reopen through the host parsed-data path.
    /// This does not implement original file lookup or filename allocation.
    pub fn reopen_model(
        self,
        model: &mut MaterializedModel,
        source: Arc<Model>,
    ) -> Result<(), SkeletalPlaybackError> {
        self.validate_model(model)?;
        drop(self);
        model.reopen_from_parsed(source)?;
        Ok(())
    }
    pub fn stop_animation(&mut self) {
        self.controller.stop_animation(0);
    }
    pub fn start_animation(&mut self) {
        self.controller.start_animation(0);
    }
    /// Rebind the same resource through SetAnimation: resets current/previous,
    /// preserves last_delta and resource identity, and makes the set dirty.
    pub fn restart_animation(
        &mut self,
        model: &mut impl SkeletalPlaybackModel,
        mode: i32,
    ) -> Result<(), SkeletalPlaybackError> {
        self.validate_model(model)?;
        if !matches!(mode, 1 | 2) {
            return Err(SkeletalPlaybackError::Unsupported(
                "only stop and loop modes admitted",
            ));
        }
        let resource = self
            .controller
            .animation(0)
            .cloned()
            .ok_or(SkeletalPlaybackError::Identity)?;
        let (_, keys) = model.playback_mapping()?;
        // Same-resource rebinding reuses every existing evaluator. Fresh IDs
        // remain distinct should that invariant ever be violated.
        let next = self
            .controller
            .active_evaluators()
            .iter()
            .map(|e| e.0)
            .chain([self.controller.state().evaluators.allocation.0])
            .max()
            .unwrap_or(0);
        let result = self
            .controller
            .set_animation(
                &mut Resources {
                    model: Some(model),
                    next,
                },
                &self.source,
                &keys,
                Some(&resource),
                0,
                1.,
                mode,
            )
            .map_err(|e| SkeletalPlaybackError::Controller(format!("{e:?}")));
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn set_anim_time(&mut self, time: u32) -> Result<(), SkeletalPlaybackError> {
        if time > self.duration_ms() {
            return Err(SkeletalPlaybackError::Unsupported(
                "time exceeds playback duration",
            ));
        }
        self.controller
            .set_anim_time(0, time as i32)
            .map_err(|e| SkeletalPlaybackError::Controller(format!("{e:?}")))
    }
    pub fn set_anim_options(&mut self, mode: i32) -> Result<(), SkeletalPlaybackError> {
        if !matches!(mode, 1 | 2) {
            return Err(SkeletalPlaybackError::Unsupported(
                "only stop and loop modes admitted",
            ));
        }
        self.controller
            .set_anim_options(0, mode)
            .map_err(|e| SkeletalPlaybackError::Controller(format!("{e:?}")))
    }
    pub fn storage_bytes(&self) -> Result<usize, SkeletalPlaybackError> {
        let a = self
            .binding
            .storage_bytes()
            .map_err(|_| SkeletalPlaybackError::AllocationOverflow)?;
        let b = self
            .binding
            .animation_storage_bytes()
            .map_err(|_| SkeletalPlaybackError::AllocationOverflow)?;
        self.controller
            .storage_bytes()
            .and_then(|n| n.checked_add(a))
            .and_then(|n| n.checked_add(b))
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>()))
            .ok_or(SkeletalPlaybackError::AllocationOverflow)
    }
    /// Run original model/controller ordering. The evaluator receives the
    /// advanced time BEFORE boundary handling; a loop subtracts duration once.
    /// Completed evaluators and controller writes survive a later failure.
    /// Numeric admission within one evaluator is preflighted before destination
    /// writes (including normalization); it does not simulate native arithmetic
    /// exceptions inside the destination write sequence. Failure poisons playback.
    pub fn tick(
        &mut self,
        model: &mut impl SkeletalPlaybackModel,
        delta: i32,
    ) -> Result<bool, SkeletalPlaybackError> {
        self.validate_model(model)?;
        if delta < 0 || delta as u32 > self.duration_ms() {
            return Err(SkeletalPlaybackError::Unsupported(
                "delta must be nonnegative and at most one duration",
            ));
        }
        if !self.controller.owns_view() {
            return Err(SkeletalPlaybackError::Unsupported(
                "controller view mismatch",
            ));
        }
        let mut state = std::mem::take(self.controller.state_mut());
        let allocation = state.evaluators.allocation;
        let mut backend = PlaybackBackend {
            controller: &self.controller,
            model,
            state: &mut state,
            allocation,
            failure: None,
        };
        let result = self.native.tick(&mut backend, delta);
        let failure = backend.failure.take();
        *self.controller.state_mut() = state;
        if let Some(error) = failure {
            self.poisoned = true;
            return Err(error);
        }
        if let Err(error) = result {
            self.poisoned = true;
            return Err(error);
        }
        if let Err(error) = model.playback_prepare_pose() {
            self.poisoned = true;
            return Err(error.into());
        }
        Ok(true)
    }
}

impl Drop for SkeletalPlayback {
    fn drop(&mut self) {
        // Close has no destination writes; release owned evaluators/resources
        // in recovered order before Rust drops the remaining host fields.
        self.controller.close(&mut Resources {
            model: None,
            next: 0,
        });
    }
}

struct PlaybackBackend<'a> {
    controller: &'a IPolSet,
    model: &'a mut dyn SkeletalPlaybackModel,
    state: &'a mut InterpolatorSet,
    allocation: AllocationId,
    failure: Option<SkeletalPlaybackError>,
}
impl ModelBackend for PlaybackBackend<'_> {
    type Error = SkeletalPlaybackError;
    fn evaluator_count(&self, id: InterpolatorId) -> Result<u32, Self::Error> {
        if id != InterpolatorId(1) {
            return Err(SkeletalPlaybackError::Identity);
        }
        u32::try_from(self.state.evaluators.count)
            .map_err(|_| SkeletalPlaybackError::AllocationOverflow)
    }
    fn tick_interpolator(
        &mut self,
        _: &mut ModelTickState,
        _: InterpolatorId,
        delta: i32,
    ) -> Result<(), Self::Error> {
        // Move the tick state out to leave no overlapping callback borrows.
        let mut state = std::mem::take(self.state);
        let result = state.tick(self, delta);
        *self.state = state;
        result.map_err(SkeletalPlaybackError::Tick)
    }
    fn update_model(
        &mut self,
        _: &mut ModelTickState,
        table: ModelVTableId,
    ) -> Result<(), Self::Error> {
        if table != ModelVTableId(0x1009c160) {
            return Err(SkeletalPlaybackError::Identity);
        }
        self.model.playback_update().map_err(Into::into)
    }
}
impl PlaybackBackend<'_> {
    fn sample_evaluator(
        &mut self,
        set: &InterpolatorSet,
        id: EvaluatorId,
    ) -> Result<(), SkeletalPlaybackError> {
        let slot = set.slots[0];
        if !slot.enabled {
            return Ok(());
        }
        let evaluator = self
            .controller
            .evaluator(id)
            .ok_or(SkeletalPlaybackError::Identity)?;
        let binding = evaluator.bindings[0];
        if binding.flags == 0 {
            return Ok(());
        }
        let resource = self
            .controller
            .animation(0)
            .ok_or(SkeletalPlaybackError::Identity)?;
        let track = &resource.source.tracks[binding.track_index];
        let time = u32::try_from(slot.current)
            .map_err(|_| SkeletalPlaybackError::Unsupported("negative evaluator time"))?;
        let position = track
            .position
            .as_ref()
            .map(|c| c.sample_position_channel(time))
            .transpose()
            .map_err(|e| SkeletalPlaybackError::Sampling(e.to_string()))?;
        let rotation = track
            .rotation
            .as_ref()
            .map(|c| sample_rotation_channel(c, time))
            .transpose()
            .map_err(|e| SkeletalPlaybackError::Sampling(format!("{e:?}")))?;
        self.model.playback_apply(SampledFrame {
            frame: evaluator.destination,
            position,
            rotation,
        })?;
        Ok(())
    }
}
impl InterpolatorBackend for PlaybackBackend<'_> {
    fn evaluator_at(
        &self,
        allocation: AllocationId,
        index: usize,
    ) -> Result<EvaluatorId, TickError> {
        // The immutable ownership list remains stable while tick state is moved.
        if allocation != self.allocation {
            return Err(TickError::InvalidEvaluatorStorage { allocation, index });
        }
        self.controller
            .active_evaluators()
            .get(index)
            .copied()
            .ok_or(TickError::InvalidEvaluatorStorage { allocation, index })
    }
    fn evaluate(&mut self, set: &mut InterpolatorSet, id: EvaluatorId) -> Result<(), TickError> {
        if let Err(error) = self.sample_evaluator(set, id) {
            self.failure = Some(error);
            return Err(TickError::MissingEvaluator(id));
        }
        Ok(())
    }
    fn boundary_destination(&self, id: EvaluatorId) -> Result<Option<FrameId>, TickError> {
        let evaluator = self
            .controller
            .evaluator(id)
            .ok_or(TickError::MissingEvaluator(id))?;
        let frame = FrameId(evaluator.destination.0);
        let state = self
            .model
            .playback_arena()
            .state(evaluator.destination)
            .map_err(|_| TickError::MissingFrame(frame))?;
        if state.flags & 0x800 != 0 {
            Err(TickError::MissingFrame(frame))
        } else {
            Ok(None)
        }
    }
    fn notify_boundary(
        &mut self,
        _: &mut InterpolatorSet,
        frame: FrameId,
        _: usize,
    ) -> Result<(), TickError> {
        Err(TickError::MissingFrame(frame))
    }
    fn duration(&mut self, _: &mut InterpolatorSet, id: AnimSetId) -> Result<i32, TickError> {
        self.controller
            .animation(0)
            .filter(|a| a.id == id)
            .map(|a| a.source.duration_ms() as i32)
            .ok_or(TickError::MissingDuration(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (MaterializedModel, Arc<AnimationSet>) {
        let model = Arc::new(Model {
            materials: vec![],
            frames: vec![lhr_assets::Frame {
                kind: 1,
                visual: Some(0),
                render_flags: 0,
                logic_flags: 0,
                culling: 0,
                name: "channel".into(),
                name_bytes: b"channel".to_vec(),
                properties: vec![],
                dummy_bounds: None,
                light: None,
                joint: None,
                skin: None,
                morph: None,
                lensflare: None,
                target: None,
                occluder: None,
                sector: None,
                parent: None,
                position: [0.; 3],
                scale: [1.; 3],
                rotation_wxyz: [1., 0., 0., 0.],
                instance: None,
                lods: vec![],
            }],
        });
        let mut payload = vec![1, 0, 10, 0];
        payload.extend(12u32.to_le_bytes());
        payload.extend(20u32.to_le_bytes());
        payload.extend(b"channel\0");
        payload.extend(2u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        payload.extend(2u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        payload.extend(10u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 10., 0., 0.] {
            payload.extend(value.to_le_bytes());
        }
        let mut bytes = b"5DS\0".to_vec();
        bytes.extend(20u16.to_le_bytes());
        bytes.extend([0u8; 8]);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        (
            MaterializedModel::from_parsed(model).unwrap(),
            Arc::new(lhr_assets::animation::parse_5ds(&bytes).unwrap()),
        )
    }
    fn x(model: &mut MaterializedModel) -> f32 {
        model.world_matrices().unwrap()[0].elements[12]
    }
    fn lifecycle_fixture() -> (MaterializedModel, Arc<AnimationSet>) {
        let (model, animation) = fixture();
        let mut source = (**model.source()).clone();
        source.frames[0].kind = 6;
        source.frames[0].visual = None;
        source.frames[0].dummy_bounds = Some([[-1.; 3], [1.; 3]]);
        (
            MaterializedModel::from_parsed(Arc::new(source)).unwrap(),
            animation,
        )
    }
    #[test]
    fn evaluates_endpoint_before_wrap_then_resumes_and_restarts_without_resetting_delta() {
        let (mut model, animation) = fixture();
        let mut playback = SkeletalPlayback::new(&mut model, animation).unwrap();
        playback.tick(&mut model, 400).unwrap();
        assert_eq!(x(&mut model), 10.);
        assert_eq!(playback.controller().state().slots[0].current, 0);
        assert_eq!(playback.controller().state().slots[0].previous, 400);
        playback.tick(&mut model, 40).unwrap();
        assert_eq!(x(&mut model), 1.);
        playback.stop_animation();
        playback.tick(&mut model, 83).unwrap();
        assert_eq!(playback.controller().state().slots[0].current, 40);
        assert_eq!(x(&mut model), 1.);
        playback.start_animation();
        playback.tick(&mut model, 40).unwrap();
        assert_eq!(x(&mut model), 2.);
        playback.restart_animation(&mut model, 1).unwrap();
        let slot = playback.controller().state().slots[0];
        assert_eq!((slot.current, slot.previous, slot.last_delta), (0, -40, 40));
        playback.tick(&mut model, 400).unwrap();
        assert!(!playback.controller().state().slots[0].enabled);
        assert_eq!(x(&mut model), 10.);
        playback.tick(&mut model, 40).unwrap();
        assert_eq!(x(&mut model), 10.);
    }
    #[test]
    fn same_source_instances_are_distinct_and_rejected_deltas_do_not_advance() {
        let (mut model, animation) = fixture();
        let mut other = MaterializedModel::from_parsed(Arc::clone(model.source())).unwrap();
        let mut playback = SkeletalPlayback::new(&mut model, animation).unwrap();
        assert!(matches!(
            playback.tick(&mut other, 40),
            Err(SkeletalPlaybackError::Identity)
        ));
        assert!(playback.tick(&mut model, -1).is_err());
        assert!(playback.tick(&mut model, 401).is_err());
        assert_eq!(playback.controller().state().slots[0].current, 0);
        assert_eq!(x(&mut model), 0.);
        playback.tick(&mut model, 40).unwrap();
        assert_eq!(x(&mut model), 1.);
    }
    #[test]
    fn consuming_close_releases_animation_resources_and_retires_destinations() {
        let (mut model, animation) = lifecycle_fixture();
        let weak = Arc::downgrade(&animation);
        let key = model.source_frame_keys()[0];
        let root = model.root();
        let playback = SkeletalPlayback::new(&mut model, animation).unwrap();
        assert!(weak.upgrade().is_some());
        playback.close_model(&mut model).unwrap();
        assert!(weak.upgrade().is_none());
        assert!(!model.is_complete());
        assert!(model.arena().frame(key).is_err());
        assert!(model.arena().frame(root).is_ok());
    }
    #[test]
    fn unavailable_visual_destructor_does_not_keep_a_consumed_controller_alive() {
        let (mut model, animation) = fixture();
        let weak = Arc::downgrade(&animation);
        let key = model.source_frame_keys()[0];
        let playback = SkeletalPlayback::new(&mut model, animation).unwrap();
        assert!(playback.close_model(&mut model).is_err());
        assert!(weak.upgrade().is_none());
        assert!(!model.is_complete());
        // Admission failure preserves this pending owner reference. Retaining
        // it externally lets Close finish without an unavailable destruction.
        assert_eq!(model.arena().frame(key).unwrap().references, 1);
        model.arena_mut().retain(key).unwrap();
        model.close().unwrap();
        let survivor = model.arena().frame(key).unwrap();
        assert_eq!(survivor.references, 1);
        assert_eq!(survivor.owner, None);
    }
    #[test]
    fn reopen_keeps_root_and_shared_source_but_requires_a_new_playback() {
        let (mut model, animation) = lifecycle_fixture();
        let source = Arc::clone(model.source());
        let mut other = MaterializedModel::from_parsed(Arc::clone(&source)).unwrap();
        let mut other_playback = SkeletalPlayback::new(&mut other, Arc::clone(&animation)).unwrap();
        let root = model.root();
        let old_key = model.source_frame_keys()[0];
        let playback = SkeletalPlayback::new(&mut model, Arc::clone(&animation)).unwrap();
        playback
            .reopen_model(&mut model, Arc::clone(&source))
            .unwrap();
        assert_eq!(model.root(), root);
        assert_ne!(model.source_frame_keys()[0], old_key);
        assert!(model.arena().frame(old_key).is_err());
        assert!(Arc::ptr_eq(model.source(), &source));
        other_playback.tick(&mut other, 40).unwrap();
        assert_eq!(x(&mut other), 1.);
        assert_eq!(x(&mut model), 0.);
        let mut new_playback = SkeletalPlayback::new(&mut model, animation).unwrap();
        new_playback.tick(&mut model, 80).unwrap();
        assert_eq!(x(&mut model), 2.);
        assert_eq!(x(&mut other), 1.);
    }
    #[test]
    fn low_level_close_invalidates_old_playback_before_advancing_time() {
        let (mut model, animation) = lifecycle_fixture();
        let mut playback = SkeletalPlayback::new(&mut model, animation).unwrap();
        playback.tick(&mut model, 40).unwrap();
        let before = playback.controller().state().slots[0];
        let owner = model
            .arena()
            .frame(model.source_frame_keys()[0])
            .unwrap()
            .owner
            .unwrap();
        let crate::scene_ownership::FrameOwner::Model(owner) = owner else {
            panic!("expected model owner")
        };
        model.arena_mut().close(owner).unwrap();
        assert!(matches!(
            playback.tick(&mut model, 40),
            Err(SkeletalPlaybackError::Identity)
        ));
        let after = playback.controller().state().slots[0];
        assert_eq!(
            (after.current, after.previous, after.last_delta),
            (before.current, before.previous, before.last_delta)
        );
    }
    #[test]
    fn destroyed_destination_is_rejected_before_controller_mutation() {
        let (mut model, animation) = lifecycle_fixture();
        let mut playback = SkeletalPlayback::new(&mut model, animation).unwrap();
        let key = model.source_frame_keys()[0];
        let before = playback.controller().state().slots[0];
        model.arena_mut().release(key).unwrap();
        assert!(playback.tick(&mut model, 40).is_err());
        assert!(playback.restart_animation(&mut model, 2).is_err());
        let after = playback.controller().state().slots[0];
        assert_eq!(
            (after.current, after.previous, after.last_delta),
            (before.current, before.previous, before.last_delta)
        );
    }
    #[test]
    fn shared_playback_borrows_actual_models_and_rejects_foreign_owner_and_reload() {
        let (model, animation) = lifecycle_fixture();
        let source = Arc::clone(model.source());
        let mut arena = ModelArena::default();
        let first = arena.create_model().unwrap();
        let first_content = arena
            .load_parsed_into_model(first, Arc::clone(&source))
            .unwrap();
        let second = arena.create_model().unwrap();
        let second_content = arena
            .load_parsed_into_model(second, Arc::clone(&source))
            .unwrap();
        let mut playback = SkeletalPlayback::new(
            &mut SharedAnimationModel::borrow(&mut arena, first).unwrap(),
            Arc::clone(&animation),
        )
        .unwrap();
        playback
            .tick(
                &mut SharedAnimationModel::borrow(&mut arena, first).unwrap(),
                80,
            )
            .unwrap();
        assert_eq!(
            arena
                .world_matrix(first_content.frames()[0])
                .unwrap()
                .elements[12],
            2.
        );
        assert_eq!(
            arena
                .world_matrix(second_content.frames()[0])
                .unwrap()
                .elements[12],
            0.
        );
        assert_eq!(arena.original_model_animation_set(first, 0).unwrap(), None);
        let before = playback.controller().state().slots[0];
        assert!(matches!(
            playback.tick(
                &mut SharedAnimationModel::borrow(&mut arena, second).unwrap(),
                40
            ),
            Err(SkeletalPlaybackError::Identity)
        ));
        let mut foreign = ModelArena::default();
        let foreign_model = foreign.create_model().unwrap();
        foreign
            .load_parsed_into_model(foreign_model, Arc::clone(&source))
            .unwrap();
        assert_eq!(first, foreign_model);
        assert!(matches!(
            playback.tick(
                &mut SharedAnimationModel::borrow(&mut foreign, foreign_model).unwrap(),
                40
            ),
            Err(SkeletalPlaybackError::Identity)
        ));
        arena.close(first).unwrap();
        let replacement = arena.load_parsed_into_model(first, source).unwrap();
        assert_ne!(replacement.frames(), first_content.frames());
        assert!(matches!(
            playback.tick(
                &mut SharedAnimationModel::borrow(&mut arena, first).unwrap(),
                40
            ),
            Err(SkeletalPlaybackError::Identity)
        ));
        let after = playback.controller().state().slots[0];
        assert_eq!(
            (after.current, after.previous, after.last_delta),
            (before.current, before.previous, before.last_delta)
        );
        // Dropping host playback never closes another owner's model or claims
        // the original native controller slot. Other models remain usable.
        let weak = Arc::downgrade(&animation);
        drop(animation);
        drop(playback);
        assert!(weak.upgrade().is_none());
        arena.close(first).unwrap();
        assert!(arena.frame(second_content.frames()[0]).is_ok());
        arena.close(second).unwrap();
    }

    #[test]
    fn shared_view_rereads_membership_and_rejects_incomplete_mapping() {
        let (model, _) = lifecycle_fixture();
        let source = Arc::clone(model.source());
        let mut arena = ModelArena::default();
        let key = arena.create_model().unwrap();
        assert!(SharedAnimationModel::borrow(&mut arena, key).is_err());
        let content = arena.load_parsed_into_model(key, source).unwrap();
        let frame = content.frames()[0];
        arena.retain(frame).unwrap();
        arena.delete_frame(key, frame).unwrap();
        assert!(SharedAnimationModel::borrow(&mut arena, key).is_err());
        assert!(arena.frame(frame).is_ok());
        arena.release(frame).unwrap();
    }

    #[test]
    fn shared_playback_refreshes_actual_owned_skin_and_retains_failed_tick_prefix() {
        let (_, animation) = lifecycle_fixture();
        let mut source = crate::model_loading::tests::skinned_source();
        let joint = &mut Arc::make_mut(&mut source).frames[1];
        joint.name = "channel".into();
        joint.name_bytes = b"channel".to_vec();
        let directory = std::env::temp_dir().join(format!(
            "lhr-shared-playback-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let content = arena
            .load_parsed_into_model(model, Arc::clone(&source))
            .unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        let visual = content.frames()[0];
        let mut playback = SkeletalPlayback::new(
            &mut SharedAnimationModel::borrow(&mut arena, model).unwrap(),
            Arc::clone(&animation),
        )
        .unwrap();
        let before = arena.owned_singlemesh_lod0_vertices(visual, 0).unwrap()[0].position;
        playback
            .tick(
                &mut SharedAnimationModel::borrow(&mut arena, model).unwrap(),
                80,
            )
            .unwrap();
        let after = arena.owned_singlemesh_lod0_vertices(visual, 0).unwrap()[0].position;
        assert_ne!(before, after);
        assert_eq!(
            arena.state(content.frames()[1]).unwrap().local.elements[12],
            2.
        );
        assert_eq!(source.frames[1].position, [4., 0., 0.]);
        drop(playback);
        arena.publish_runtime_buffers().unwrap();
        arena.close(model).unwrap();

        // Without the visual resource owner the evaluator still writes its
        // reached position/time prefix, then pose publication fails. Replay is
        // poisoned rather than silently accepting a static or detached pose.
        let bare = arena.create_model().unwrap();
        let content = arena.load_parsed_into_model(bare, source).unwrap();
        let playback = SkeletalPlayback::new(
            &mut SharedAnimationModel::borrow(&mut arena, bare).unwrap(),
            animation,
        );
        assert!(playback.is_err());
        assert_eq!(
            arena.state(content.frames()[1]).unwrap().local.elements[12],
            0.
        );
        // The constructor failed after its zero-time evaluator write; no host
        // controller was installed in the original slot or leaked to the arena.
        assert_eq!(arena.original_model_animation_set(bare, 0).unwrap(), None);
        drop(playback);
        arena
            .load_plain_visual_resources(bare, &installation)
            .unwrap();
        arena.publish_runtime_buffers().unwrap();
        arena.close(bare).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
