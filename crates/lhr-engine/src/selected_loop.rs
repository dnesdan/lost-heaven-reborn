//! Recovered selected Game loop (RVA 0x1be750), with required host bindings.
//! Original SHA256: 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! This controller does not initialize a mission, infer modes, or supply no-op helpers.
use crate::frame_dispatch::ObjectId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphBinding {
    pub object: ObjectId,
    pub vtable: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputView {
    pub allocation: u32,
    pub count: usize,
}
/// Live fields supplied by a verified adapter, not a native context constructor.
#[derive(Clone, Debug)]
pub struct LoopState {
    pub graph: GraphBinding,
    pub mission: Option<ObjectId>,
    pub scene: Option<ObjectId>,
    pub input: Option<InputView>,
    /// None represents a null mapping-vector begin; otherwise its signed /12 count.
    pub input_clear_count: Option<i32>,
    pub presentation_counter: i32,
    pub context_70: u32,
    pub context_74: u32,
    pub post_update_enabled: u8,
    pub fade_enabled: u8,
    pub reload_pending: u8,
    pub fast_escape: u8,
    pub control_word: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphOperation {
    NullTimer,
    ProcessWinMessages,
    UpdateMouseData,
    UpdateKeyboardData,
    ReadKey,
    GetTimer {
        minimum: i32,
        maximum: i32,
    },
    /// Query with the native argument zero.
    GetShiftKeys,
    TranslateKey {
        key: u8,
        shift: u8,
    },
    Present,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitReason {
    InactiveAtEntry,
    MissionStopped,
    MissionStatusOne,
    ModalExit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopFlow {
    Continue,
    Exited(ExitReason),
}
#[derive(Debug, PartialEq, Eq)]
pub enum LoopError<E> {
    Backend(E),
    MissingMission,
    MissingScene,
    MissingInputBacking,
    NegativeInputCount,
    NotEntered,
    AlreadyEntered,
    AlreadyExited,
}
/// Every callback may mutate live state. Lookup and buffer-write methods represent
/// native field/memory access and must reject unknown or unreadable identities.
/// Missing dependency implementations must return errors, never implicit success.
/// Graph operations identify both the object and the dispatch table, which may
/// differ after a callback replaces the global graph during key translation.
pub trait SelectedLoopBackend {
    type Error;
    fn graph(
        &mut self,
        state: &mut LoopState,
        binding: GraphBinding,
        operation: GraphOperation,
    ) -> Result<u32, Self::Error>;
    fn mission_active(&self, mission: ObjectId) -> Result<u8, Self::Error>;
    fn mission_status(&self, mission: ObjectId) -> Result<u32, Self::Error>;
    fn input_at(&self, allocation: u32, index: usize) -> Result<ObjectId, Self::Error>;
    /// Call the input object update with the native argument one.
    fn input_object(&mut self, state: &mut LoopState, object: ObjectId) -> Result<(), Self::Error>;
    fn update_input(&mut self, state: &mut LoopState) -> Result<(), Self::Error>;
    fn mission_key(
        &mut self,
        state: &mut LoopState,
        mission: ObjectId,
        key: u8,
    ) -> Result<u8, Self::Error>;
    fn input_buffer(&mut self, state: &mut LoopState, index: u8) -> Result<ObjectId, Self::Error>;
    fn clear_input_entry(&mut self, buffer: ObjectId, index: usize) -> Result<(), Self::Error>;
    fn modify_delta(&mut self, state: &mut LoopState, delta: &mut i32) -> Result<(), Self::Error>;
    fn dispatch(&mut self, state: &mut LoopState, delta: i32) -> Result<(), Self::Error>;
    fn post_dispatch(&mut self, state: &mut LoopState, delta: i32) -> Result<(), Self::Error>;
    fn optional_update(&mut self, state: &mut LoopState, delta: i32) -> Result<(), Self::Error>;
    fn pause_query(&mut self, state: &mut LoopState) -> Result<u8, Self::Error>;
    fn pause_reset(&mut self, state: &mut LoopState) -> Result<(), Self::Error>;
    fn mission_pause(
        &mut self,
        state: &mut LoopState,
        mission: ObjectId,
    ) -> Result<(), Self::Error>;
    /// Required modal dependency: allocate 0x30 bytes, construct when nonnull,
    /// assign the recovered modal dispatch table, then run modal mode (DL=0,arg=1).
    /// An adapter lacking any part of this sequence must return an error.
    fn open_modal(&mut self, state: &mut LoopState) -> Result<u32, Self::Error>;
    fn mission_resume(
        &mut self,
        state: &mut LoopState,
        mission: ObjectId,
        resume: bool,
    ) -> Result<(), Self::Error>;
    fn mission_abort(
        &mut self,
        state: &mut LoopState,
        mission: ObjectId,
    ) -> Result<(), Self::Error>;
    fn reset_sequence(&mut self, state: &mut LoopState) -> Result<(), Self::Error>;
    fn stop_sequence(&mut self, state: &mut LoopState) -> Result<(), Self::Error>;
    fn post_main(&mut self, state: &mut LoopState, delta: i32) -> Result<(), Self::Error>;
    fn scene_end(&mut self, state: &mut LoopState, scene: ObjectId) -> Result<(), Self::Error>;
    fn fade_update(&mut self, state: &mut LoopState, delta: i32) -> Result<(), Self::Error>;
    fn reload(&mut self, state: &mut LoopState) -> Result<(), Self::Error>;
    /// Apply the live numerical policy. This models the native FLDCW, not a
    /// restoration of the caller's previous control word.
    fn set_control_word(&mut self, state: &mut LoopState, word: u16) -> Result<(), Self::Error>;
}

#[derive(Debug, Default)]
pub struct SelectedLoop {
    entered: bool,
    exited: bool,
}
impl SelectedLoop {
    pub fn new() -> Self {
        Self::default()
    }
    /// Reset the clock once, including an inactive entry. A failed callback can
    /// already have effects; errors are not rolled back or silently retried.
    pub fn enter<B: SelectedLoopBackend>(
        &mut self,
        state: &mut LoopState,
        backend: &mut B,
    ) -> Result<LoopFlow, LoopError<B::Error>> {
        if self.entered {
            return Err(LoopError::AlreadyEntered);
        }
        self.entered = true;
        let result = (|| {
            graph(backend, state, GraphOperation::NullTimer)?;
            if active(backend, state)? == 0 {
                self.exit(ExitReason::InactiveAtEntry)
            } else {
                Ok(LoopFlow::Continue)
            }
        })();
        if result.is_err() {
            self.exited = true;
        }
        result
    }
    /// Execute one complete native iteration. Continue requests another step
    /// without another clock reset. An error poisons this controller so a partial
    /// iteration cannot accidentally be continued or replayed.
    pub fn step<B: SelectedLoopBackend>(
        &mut self,
        state: &mut LoopState,
        backend: &mut B,
    ) -> Result<LoopFlow, LoopError<B::Error>> {
        let result = self.step_inner(state, backend);
        if self.entered && result.is_err() {
            self.exited = true;
        }
        result
    }
    fn step_inner<B: SelectedLoopBackend>(
        &mut self,
        state: &mut LoopState,
        backend: &mut B,
    ) -> Result<LoopFlow, LoopError<B::Error>> {
        if !self.entered {
            return Err(LoopError::NotEntered);
        }
        if self.exited {
            return Err(LoopError::AlreadyExited);
        }
        graph(backend, state, GraphOperation::ProcessWinMessages)?;
        graph(backend, state, GraphOperation::UpdateMouseData)?;
        graph(backend, state, GraphOperation::UpdateKeyboardData)?;
        let mut key = graph(backend, state, GraphOperation::ReadKey)? as u8;
        let mut delta = graph(
            backend,
            state,
            GraphOperation::GetTimer {
                minimum: 16,
                maximum: 83,
            },
        )? as i32;
        // Cache the count, but reload the backing allocation before each call.
        let count = state.input.map_or(0, |view| view.count);
        for index in (0..count).rev() {
            let allocation = state
                .input
                .ok_or(LoopError::MissingInputBacking)?
                .allocation;
            let object = backend
                .input_at(allocation, index)
                .map_err(LoopError::Backend)?;
            backend
                .input_object(state, object)
                .map_err(LoopError::Backend)?;
        }
        backend.update_input(state).map_err(LoopError::Backend)?;
        let second_key = graph(backend, state, GraphOperation::ReadKey)? as u8;
        if second_key != 0 {
            let captured = state.graph;
            let mission = mission(state)?;
            let shift = backend
                .graph(state, captured, GraphOperation::GetShiftKeys)
                .map_err(LoopError::Backend)? as u8
                & 15;
            let translation_binding = GraphBinding {
                object: state.graph.object,
                vtable: captured.vtable,
            };
            let translated = backend
                .graph(
                    state,
                    translation_binding,
                    GraphOperation::TranslateKey {
                        key: second_key,
                        shift,
                    },
                )
                .map_err(LoopError::Backend)? as u8;
            if backend
                .mission_key(state, mission, translated)
                .map_err(LoopError::Backend)?
                != 0
            {
                key = 0;
                let first = backend.input_buffer(state, 0).map_err(LoopError::Backend)?;
                let second = backend.input_buffer(state, 1).map_err(LoopError::Backend)?;
                if let Some(count) = state.input_clear_count {
                    let count =
                        usize::try_from(count).map_err(|_| LoopError::NegativeInputCount)?;
                    for index in (0..count).rev() {
                        backend
                            .clear_input_entry(first, index)
                            .map_err(LoopError::Backend)?;
                        backend
                            .clear_input_entry(second, index)
                            .map_err(LoopError::Backend)?;
                    }
                }
            }
        }
        if active(backend, state)? != 0 {
            backend
                .modify_delta(state, &mut delta)
                .map_err(LoopError::Backend)?;
        }
        backend.dispatch(state, delta).map_err(LoopError::Backend)?;
        backend
            .post_dispatch(state, delta)
            .map_err(LoopError::Backend)?;
        if state.post_update_enabled != 0 {
            backend
                .optional_update(state, delta)
                .map_err(LoopError::Backend)?;
        }
        if key == 1 {
            let paused = backend.pause_query(state).map_err(LoopError::Backend)?;
            if paused != 0 && state.fast_escape != 0 {
                backend.pause_reset(state).map_err(LoopError::Backend)?;
            } else {
                let before_modal = mission(state)?;
                backend
                    .mission_pause(state, before_modal)
                    .map_err(LoopError::Backend)?;
                let result = backend.open_modal(state).map_err(LoopError::Backend)?;
                let after_modal = mission(state)?;
                backend
                    .mission_resume(state, after_modal, result != 0x21)
                    .map_err(LoopError::Backend)?;
                match result {
                    0x22 => {
                        let current = mission(state)?;
                        backend
                            .mission_abort(state, current)
                            .map_err(LoopError::Backend)?;
                    }
                    0x21 => {
                        if backend.pause_query(state).map_err(LoopError::Backend)? != 0 {
                            backend.pause_reset(state).map_err(LoopError::Backend)?;
                            backend.stop_sequence(state).map_err(LoopError::Backend)?;
                        }
                        return self.exit(ExitReason::ModalExit);
                    }
                    0x13 => backend.reset_sequence(state).map_err(LoopError::Backend)?,
                    _ => {}
                }
            }
        } else if (key == 0x39 || key == 0x1c)
            && backend.pause_query(state).map_err(LoopError::Backend)? != 0
        {
            backend.pause_reset(state).map_err(LoopError::Backend)?;
        }
        match backend
            .mission_status(mission(state)?)
            .map_err(LoopError::Backend)?
        {
            1 => return self.exit(ExitReason::MissionStatusOne),
            3 => {
                state.context_70 = 0;
                state.context_74 = 0;
                backend.reset_sequence(state).map_err(LoopError::Backend)?;
            }
            _ => {}
        }
        backend
            .post_main(state, delta)
            .map_err(LoopError::Backend)?;
        let scene = state.scene.ok_or(LoopError::MissingScene)?;
        backend
            .scene_end(state, scene)
            .map_err(LoopError::Backend)?;
        if state.presentation_counter > 0 {
            state.presentation_counter -= 1;
        } else {
            graph(backend, state, GraphOperation::Present)?;
        }
        if state.fade_enabled != 0 {
            backend
                .fade_update(state, delta)
                .map_err(LoopError::Backend)?;
        }
        if state.reload_pending != 0 {
            state.reload_pending = 0;
            backend.reload(state).map_err(LoopError::Backend)?;
        }
        backend
            .set_control_word(state, 0x003f)
            .map_err(LoopError::Backend)?;
        state.control_word = 0x003f;
        if active(backend, state)? == 0 {
            self.exit(ExitReason::MissionStopped)
        } else {
            Ok(LoopFlow::Continue)
        }
    }
    fn exit<E>(&mut self, reason: ExitReason) -> Result<LoopFlow, LoopError<E>> {
        self.exited = true;
        Ok(LoopFlow::Exited(reason))
    }
}
fn mission<E>(state: &LoopState) -> Result<ObjectId, LoopError<E>> {
    state.mission.ok_or(LoopError::MissingMission)
}
fn active<B: SelectedLoopBackend>(
    backend: &B,
    state: &LoopState,
) -> Result<u8, LoopError<B::Error>> {
    backend
        .mission_active(mission(state)?)
        .map_err(LoopError::Backend)
}
fn graph<B: SelectedLoopBackend>(
    backend: &mut B,
    state: &mut LoopState,
    operation: GraphOperation,
) -> Result<u32, LoopError<B::Error>> {
    backend
        .graph(state, state.graph, operation)
        .map_err(LoopError::Backend)
}
