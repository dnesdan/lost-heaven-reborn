//! Bounded original-data position binding. This module deliberately excludes
//! mixed rotation/scale/note tracks and arbitrary name-based retargeting.
//!
//! LS3DF SHA `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`;
//! evidence `PC-SINGLE-ROOT-POSITION-CONTRACT-V2`: SetAnimation RVA0x1e580,
//! position branch RVA0x1d320, hierarchy/child updates RVA0x1ca60/0x1c9a0,
//! mesh update RVA0x5a390 and model update RVA0x88540. This is a bounded
//! wrapper/leaf state projection, not the complete loader or transform getter.
use lhr_assets::{animation::AnimationSet, Model};
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq)]
pub enum BindingError {
    Unsupported(&'static str),
    Animation(String),
    Frame(crate::frame_transform::FrameError),
}

/// Validated one-root position source. Native initialization and hierarchy
/// updates are separate from validation; constructing this does not tick a model.
#[derive(Debug)]
pub struct PositionSource {
    animation: AnimationSet,
    initial_position: [f32; 3],
    model: Arc<Model>,
}
impl PositionSource {
    /// Require an unchanged, separately parsed animated-model count and a full
    /// animation set. Selected-track inspection cannot authorize this binding.
    pub fn new(
        model: &Arc<Model>,
        animation_count: u8,
        animation: AnimationSet,
    ) -> Result<Self, BindingError> {
        if animation_count != 1 || model.frames.len() != 1 || animation.tracks.len() != 1 {
            return Err(BindingError::Unsupported(
                "one source frame and one animation required",
            ));
        }
        let frame = &model.frames[0];
        if frame.kind != 1
            || frame.visual != Some(0)
            || frame.parent.is_some()
            || frame.instance.is_some()
            || frame.lods.len() != 1
        {
            return Err(BindingError::Unsupported("one root standard mesh required"));
        }
        if (frame.render_flags, frame.logic_flags, frame.culling) != (0, 42, 9) {
            return Err(BindingError::Unsupported(
                "source frame flags outside verified binding domain",
            ));
        }
        let track = &animation.tracks[0];
        // This first binding intentionally limits both names to ASCII. This is
        // exact byte equality, never a lossy conversion or case-folded match.
        if !frame.name.is_ascii() || track.name != frame.name.as_bytes() {
            return Err(BindingError::Unsupported(
                "exact source frame/track name required",
            ));
        }
        let positions = track
            .position_only()
            .map_err(|error| BindingError::Animation(error.to_string()))?;
        if positions
            .key_times()
            .last()
            .is_some_and(|&last| last > animation.end_frame)
        {
            return Err(BindingError::Unsupported(
                "position key exceeds animation duration",
            ));
        }
        if animation.duration_ms() == 0 {
            return Err(BindingError::Unsupported(
                "nonzero animation duration required",
            ));
        }
        Ok(Self {
            animation,
            initial_position: frame.position,
            model: Arc::clone(model),
        })
    }
    pub fn model(&self) -> &Arc<Model> {
        &self.model
    }
    pub fn duration_ms(&self) -> u32 {
        self.animation.duration_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sources() -> (Arc<Model>, AnimationSet) {
        let mut payload = Vec::new();
        payload.extend(1_u16.to_le_bytes());
        payload.extend(20_u16.to_le_bytes());
        payload.extend(12_u32.to_le_bytes());
        payload.extend(14_u32.to_le_bytes());
        payload.extend(b"n\0");
        for value in [2_u16, 0, 2, 0, 20, 0] {
            payload.extend(value.to_le_bytes());
        }
        for value in [0_f32, 0., 0., 1., 2., 3.] {
            payload.extend(value.to_le_bytes());
        }
        let mut bytes = b"5DS\0".to_vec();
        bytes.extend(20_u16.to_le_bytes());
        bytes.extend([0_u8; 8]);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        let animation = lhr_assets::animation::parse_5ds(&bytes).unwrap();
        let model = Model {
            materials: vec![],
            frames: vec![lhr_assets::Frame {
                kind: 1,
                visual: Some(0),
                render_flags: 0,
                logic_flags: 42,
                culling: 9,
                name: "n".into(),
                name_bytes: b"n".to_vec(),
                properties: Vec::new(),
                dummy_bounds: None,
                joint: None,
                skin: None,
                morph: None,
                target: None,
                occluder: None,
                sector: None,
                parent: None,
                position: [0.; 3],
                scale: [1.; 3],
                rotation_wxyz: [1., 0., 0., 0.],
                instance: None,
                lods: vec![lhr_assets::Lod {
                    distance: 0.,
                    vertices: vec![],
                    groups: vec![],
                }],
            }],
        };
        (Arc::new(model), animation)
    }
    #[test]
    fn exact_original_name_and_full_position_track_required() {
        let (model, animation) = sources();
        assert_eq!(
            PositionSource::new(&model, 1, animation.clone())
                .unwrap()
                .duration_ms(),
            800
        );
        let mut wrong = animation.clone();
        wrong.tracks[0].name = b"N".to_vec();
        assert!(PositionSource::new(&model, 1, wrong).is_err());
        let mut mixed = animation.clone();
        mixed.tracks[0].flags = 6;
        assert!(PositionSource::new(&model, 1, mixed).is_err());
        assert!(PositionSource::new(&model, 0, animation.clone()).is_err());
        let mut other = Arc::clone(&model);
        Arc::make_mut(&mut other).frames[0].logic_flags = 0;
        assert!(PositionSource::new(&other, 1, animation).is_err());
    }
    #[test]
    fn model_and_interpolator_preserve_endpoint_before_wrap() {
        let (model, animation) = sources();
        let mut native = ModelTickState::new(ModelVTableId(0x1009c160));
        let mut playback = PositionPlayback::new(
            PositionSource::new(&model, 1, animation).unwrap(),
            crate::frame_dispatch::ObjectId(1),
            &mut native,
        )
        .unwrap();
        let expected = [
            (0, 0, 0, 2, [0., 0., 0.]),
            (200, 200, 200, 3, [0.25, 0.5, 0.75]),
            (200, 400, 400, 4, [0.5, 1., 1.5]),
            (400, 0, 800, 5, [1., 2., 3.]),
            (0, 0, 800, 5, [1., 2., 3.]),
            (40, 40, 40, 6, [0.05, 0.1, 0.15]),
        ];
        for (delta, current, previous, counter, position) in expected {
            let got = playback.tick(&mut native, delta).unwrap();
            assert_eq!(got.current, current);
            assert_eq!(got.previous, previous);
            assert_eq!(got.position, position);
            assert_eq!((got.model_flags, got.model_counter), (0x3d, 1));
            assert_eq!(
                (got.mesh_flags, got.mesh_counter, got.visual_flags),
                (9, counter, 0x2a00)
            );
            assert!(!got.dirty);
        }
        let before = playback.observation();
        assert!(playback.tick(&mut native, -1).is_err());
        assert!(playback.tick(&mut native, 801).is_err());
        assert_eq!(playback.observation(), before);
    }

    #[test]
    fn lazy_world_read_and_animation_share_flags_without_extra_finalization() {
        let (model, animation) = sources();
        let mut native = ModelTickState::new(ModelVTableId(0x1009c160));
        let mut playback = PositionPlayback::new(
            PositionSource::new(&model, 1, animation).unwrap(),
            crate::frame_dispatch::ObjectId(1),
            &mut native,
        )
        .unwrap();
        assert_eq!(playback.observation().mesh_flags, 9);
        let first = playback.world_matrix().unwrap();
        let loaded = playback.observation();
        assert_eq!((loaded.mesh_flags, loaded.mesh_counter), (0x3d, 1));
        assert_eq!(playback.world_matrix().unwrap(), first);
        assert_eq!(playback.observation(), loaded);
        assert!(playback.uploaded_world().is_none());
        playback.mark_uploaded(first);
        assert_eq!(
            playback.uploaded_world(),
            Some(first.elements.map(f32::to_bits))
        );
        playback.tick(&mut native, 200).unwrap();
        assert_eq!(
            (
                playback.observation().mesh_flags,
                playback.observation().mesh_counter
            ),
            (0x1d, 2)
        );
        let moved = playback.world_matrix().unwrap();
        assert_ne!(
            first.elements.map(f32::to_bits),
            moved.elements.map(f32::to_bits)
        );
        assert_eq!(
            (
                playback.observation().mesh_flags,
                playback.observation().mesh_counter
            ),
            (0x3d, 2)
        );
        assert_eq!(
            playback.uploaded_world(),
            Some(first.elements.map(f32::to_bits))
        );
    }

    #[test]
    fn actual_clock_phase_binding_requires_source_identity_and_supported_control_word() {
        let (model, animation) = sources();
        let mut graph = crate::runtime::RetainedScene::default();
        graph
            .retain(
                "synthetic".into(),
                Arc::clone(&model),
                glam::Mat4::IDENTITY,
                None,
                vec![],
            )
            .unwrap();
        let source = PositionSource::new(&model, 1, animation).unwrap();
        let mut playback = PositionPlayback::new(
            source,
            graph.models[0].identity,
            &mut graph.models[0].native,
        )
        .unwrap();
        struct Clock;
        impl crate::clock::ClockBackend for Clock {
            fn sample_ms(&mut self) -> u32 {
                16
            }
            fn sleep_ms(&mut self, _: u32) {
                panic!("No sleep expected");
            }
        }
        assert!(playback.storage_bytes().unwrap() > std::mem::size_of::<PositionPlayback>());
        graph.set_assembly_budget(256 * 1024 * 1024).unwrap();
        assert!(graph.charge_position_binding(&playback).is_err());
        graph.set_assembly_budget(0).unwrap();
        graph.charge_position_binding(&playback).unwrap();
        let mut runtime = crate::runtime::RuntimeEngine::from_sample(0);
        let step = runtime
            .advance_position(&mut graph, 0, &mut playback, &mut Clock)
            .unwrap();
        assert_eq!(
            (
                step.raw_delta,
                step.modified_delta,
                step.driver_time,
                step.models_dispatched
            ),
            (16, 16, 16, 1)
        );
        assert_eq!(
            graph.models[0].pose()[0].position,
            playback.observation().position
        );
        assert_ne!(graph.models[0].pose()[0].position, model.frames[0].position);
        assert!(runtime
            .dispatch_position(&mut graph, 0, &mut playback, 16, 0x033f)
            .is_err());
        graph.models[0].world = glam::Mat4::from_translation(glam::Vec3::X);
        assert!(runtime
            .dispatch_position(&mut graph, 0, &mut playback, 16, 0x003f)
            .is_err());
        graph.models[0].world = glam::Mat4::IDENTITY;
        Arc::make_mut(&mut graph.models[0].source).frames[0].name = "other".into();
        assert!(runtime
            .dispatch_position(&mut graph, 0, &mut playback, 16, 0x003f)
            .is_err());
        assert_eq!(runtime.driver_time(), 16);
    }

    #[test]
    fn unsupported_source_hierarchy_is_not_flattened() {
        let (mut model, animation) = sources();
        Arc::make_mut(&mut model).frames[0].parent = Some(0);
        assert!(PositionSource::new(&model, 1, animation.clone()).is_err());
        Arc::make_mut(&mut model).frames[0].parent = None;
        Arc::make_mut(&mut model).frames[0].instance = Some(0);
        assert!(PositionSource::new(&model, 1, animation).is_err());
    }
}

use crate::{
    frame_transform::{FrameKey, I3dFrameGraph, I3dFrameState},
    interpolator::{
        AllocationId, AnimSetId, EvaluatorId, EvaluatorView, InterpolatorBackend, InterpolatorSet,
        Slot, TickError,
    },
    model_tick::{ModelBackend, ModelTickState, ModelVTableId},
    native_transform::{SMatrix, SQuat},
    scene_tick::{FrameId, InterpolatorId},
};

/// The observed state of the supported native model-wrapper / one-mesh path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionObservation {
    pub position: [f32; 3],
    pub current: i32,
    pub previous: i32,
    pub dirty: bool,
    pub model_flags: u32,
    pub model_counter: u32,
    pub mesh_flags: u32,
    pub mesh_counter: u32,
    pub visual_flags: u32,
}
/// Native automatic slot0 setup and the bounded one-child model finalizer.
/// No callbacks, additional slots, ancestors, grandchildren or deferred entries
/// are admitted. The host owns one preallocated evaluator and seven unbound
/// slots; native allocation and full constructor/loader parity are not claimed.
/// Pose setters, finalization and lazy renderer reads share the same frame
/// flags and caches. Matrix/point arithmetic uses the verified native methods;
/// camera, handedness conversion and rasterization remain host policies.
#[derive(Debug)]
pub struct PositionPlayback {
    source: PositionSource,
    set: InterpolatorSet,
    frames: I3dFrameGraph,
    model: FrameKey,
    mesh: FrameKey,
    identity: crate::frame_dispatch::ObjectId,
    uploaded_world: Option<[u32; 16]>,
}
impl PositionPlayback {
    pub fn new(
        source: PositionSource,
        identity: crate::frame_dispatch::ObjectId,
        native: &mut ModelTickState,
    ) -> Result<Self, BindingError> {
        if native.vtable != ModelVTableId(0x1009c160) || native.interpolator.is_some() {
            return Err(BindingError::Unsupported("fresh native model required"));
        }
        if identity.0 == 0 {
            return Err(BindingError::Unsupported(
                "nonzero placement identity required",
            ));
        }
        native.interpolator = Some(InterpolatorId(identity.0));
        let mut set = InterpolatorSet::default();
        set.slots[0] = Slot {
            animset: Some(AnimSetId(1)),
            enabled: true,
            mode: 2,
            current: 0,
            previous: -40,
            last_delta: 0,
            weight: 1.,
        };
        set.dirty = true;
        set.evaluators = EvaluatorView {
            allocation: AllocationId(1),
            count: 1,
        };
        let mut frames = I3dFrameGraph::default();
        let model = frames
            .insert(I3dFrameState::model())
            .map_err(BindingError::Frame)?;
        let frame = &source.model.frames[0];
        let mesh = frames
            .insert(I3dFrameState::source_mesh(
                source.initial_position,
                frame.scale,
                SQuat {
                    wxyz: frame.rotation_wxyz,
                },
            ))
            .map_err(BindingError::Frame)?;
        frames
            .link_to(mesh, Some(model))
            .map_err(BindingError::Frame)?;
        let mut bound = Self {
            source,
            set,
            identity,
            uploaded_world: None,
            frames,
            model,
            mesh,
        };
        // LoadHierarchy invokes the real model update after SetAnimation and
        // before returning. This clears frame dirtiness, not IPol dirty state.
        PositionModelBackend {
            source: &bound.source,
            set: &mut bound.set,
            frames: &mut bound.frames,
            model: bound.model,
            mesh: bound.mesh,
            interpolator: InterpolatorId(identity.0),
        }
        .update_model(native, native.vtable)?;
        Ok(bound)
    }
    /// Host allocations retained by this binding; shared source geometry is
    /// already charged by scene assembly and is not duplicated here.
    pub fn storage_bytes(&self) -> Result<usize, BindingError> {
        let animation = &self.source.animation;
        let mut bytes = std::mem::size_of::<Self>()
            .checked_add(
                animation
                    .tracks
                    .capacity()
                    .checked_mul(std::mem::size_of::<lhr_assets::animation::AnimationTrack>())
                    .ok_or(BindingError::Unsupported("animation allocation overflow"))?,
            )
            .ok_or(BindingError::Unsupported("animation allocation overflow"))?;
        for track in &animation.tracks {
            let positions = track
                .position_only()
                .map_err(|error| BindingError::Animation(error.to_string()))?;
            let position_bytes = positions
                .storage_bytes()
                .map_err(|error| BindingError::Animation(error.to_string()))?;
            bytes = bytes
                .checked_add(track.name.capacity())
                .and_then(|value| value.checked_add(position_bytes))
                .ok_or(BindingError::Unsupported("animation allocation overflow"))?;
        }
        bytes = bytes
            .checked_add(self.frames.storage_bytes().map_err(BindingError::Frame)?)
            .ok_or(BindingError::Unsupported("animation allocation overflow"))?;
        Ok(bytes)
    }
    /// Read the original lazy world transform, mutating the shared cache flags.
    pub fn world_matrix(&mut self) -> Result<SMatrix, BindingError> {
        self.frames
            .world_matrix(self.mesh)
            .map_err(BindingError::Frame)
    }
    /// State snapshots do not trigger a lazy getter; useful for before/after
    /// renderer observations and explicit diagnostics.
    pub fn frame_states(&self) -> (I3dFrameState, I3dFrameState) {
        (
            *self.frames.state(self.model).unwrap(),
            *self.frames.state(self.mesh).unwrap(),
        )
    }
    pub(crate) fn uploaded_world(&self) -> Option<[u32; 16]> {
        self.uploaded_world
    }
    pub(crate) fn mark_uploaded(&mut self, world: SMatrix) {
        self.uploaded_world = Some(world.elements.map(f32::to_bits));
    }
    pub fn model_identity(&self) -> crate::frame_dispatch::ObjectId {
        self.identity
    }
    pub fn source_model(&self) -> &Arc<Model> {
        self.source.model()
    }
    pub fn observation(&self) -> PositionObservation {
        let (model, mesh) = self.frame_states();
        PositionObservation {
            position: mesh.local.elements[12..15].try_into().unwrap(),
            current: self.set.slots[0].current,
            previous: self.set.slots[0].previous,
            dirty: self.set.dirty,
            model_flags: model.flags,
            model_counter: model.update_count,
            mesh_flags: mesh.flags,
            mesh_counter: mesh.update_count,
            visual_flags: mesh.visual_flags,
        }
    }
    /// Positive time increments no greater than one duration preserve the native
    /// one-wrap domain. A zero delta still evaluates the initial dirty binding.
    pub fn tick(
        &mut self,
        native: &mut ModelTickState,
        delta: i32,
    ) -> Result<PositionObservation, BindingError> {
        if native.vtable != ModelVTableId(0x1009c160)
            || native.interpolator != Some(InterpolatorId(self.identity.0))
        {
            return Err(BindingError::Unsupported("bound native model required"));
        }
        if delta < 0 || delta as u32 > self.source.duration_ms() {
            return Err(BindingError::Unsupported(
                "delta outside single-wrap position domain",
            ));
        }
        let candidate = self.set.slots[0]
            .current
            .checked_add(delta)
            .ok_or(BindingError::Unsupported("position time overflow"))?;
        self.source.animation.tracks[0]
            .position_only()
            .and_then(|track| track.sample_position_channel(candidate as u32))
            .map_err(|error| BindingError::Animation(error.to_string()))?;
        let mut backend = PositionModelBackend {
            source: &self.source,
            set: &mut self.set,
            frames: &mut self.frames,
            model: self.model,
            mesh: self.mesh,
            interpolator: InterpolatorId(self.identity.0),
        };
        native.tick(&mut backend, delta)?;
        Ok(self.observation())
    }
}
struct PositionModelBackend<'a> {
    source: &'a PositionSource,
    set: &'a mut InterpolatorSet,
    frames: &'a mut I3dFrameGraph,
    model: FrameKey,
    mesh: FrameKey,
    interpolator: InterpolatorId,
}
impl ModelBackend for PositionModelBackend<'_> {
    type Error = BindingError;
    fn evaluator_count(&self, id: InterpolatorId) -> Result<u32, BindingError> {
        if id != self.interpolator {
            return Err(BindingError::Unsupported("unbound interpolator"));
        }
        Ok(self.set.evaluators.count as u32)
    }
    fn tick_interpolator(
        &mut self,
        _: &mut ModelTickState,
        id: InterpolatorId,
        delta: i32,
    ) -> Result<(), BindingError> {
        if id != self.interpolator {
            return Err(BindingError::Unsupported("unbound interpolator"));
        }
        self.set
            .tick(
                &mut PositionEvaluator {
                    source: self.source,
                    frames: self.frames,
                    mesh: self.mesh,
                },
                delta,
            )
            .map_err(|_| BindingError::Unsupported("unavailable position evaluator dependency"))
    }
    fn update_model(
        &mut self,
        _: &mut ModelTickState,
        vtable: ModelVTableId,
    ) -> Result<(), BindingError> {
        if vtable != ModelVTableId(0x1009c160) {
            return Err(BindingError::Unsupported("unavailable model finalizer"));
        }
        self.frames
            .update(self.model)
            .map_err(BindingError::Frame)?;
        Ok(())
    }
}
struct PositionEvaluator<'a> {
    source: &'a PositionSource,
    frames: &'a mut I3dFrameGraph,
    mesh: FrameKey,
}
impl InterpolatorBackend for PositionEvaluator<'_> {
    fn evaluator_at(
        &self,
        allocation: AllocationId,
        index: usize,
    ) -> Result<EvaluatorId, TickError> {
        if allocation != AllocationId(1) || index != 0 {
            return Err(TickError::InvalidEvaluatorStorage { allocation, index });
        }
        Ok(EvaluatorId(1))
    }
    fn evaluate(&mut self, set: &mut InterpolatorSet, id: EvaluatorId) -> Result<(), TickError> {
        if id != EvaluatorId(1) {
            return Err(TickError::MissingEvaluator(id));
        }
        let slot = &set.slots[0];
        if !slot.enabled || slot.weight != 1. {
            return Err(TickError::MissingEvaluator(id));
        }
        let position = self.source.animation.tracks[0]
            .position_only()
            .and_then(|track| track.sample_position_channel(slot.current as u32))
            .map_err(|_| TickError::MissingEvaluator(id))?;
        self.frames
            .assign_position(self.mesh, position)
            .map_err(|_| TickError::MissingEvaluator(id))?;
        Ok(())
    }
    fn boundary_destination(&self, id: EvaluatorId) -> Result<Option<FrameId>, TickError> {
        if id != EvaluatorId(1) {
            return Err(TickError::MissingEvaluator(id));
        }
        if self
            .frames
            .state(self.mesh)
            .map_err(|_| TickError::MissingEvaluator(id))?
            .flags
            & 0x800
            != 0
        {
            return Err(TickError::MissingFrame(FrameId(1)));
        }
        Ok(None)
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
        if id != AnimSetId(1) {
            return Err(TickError::MissingDuration(id));
        }
        Ok(self.source.duration_ms() as i32)
    }
}
