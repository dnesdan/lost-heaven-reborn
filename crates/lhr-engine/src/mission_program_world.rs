//! Mission-owned program execution borrowing the same actors and scene arena.
//! This host binding adds no command, scheduler, GameInit or original-function credit.
#![forbid(unsafe_code)]
use super::global_program_lifecycle::{
    MissionGlobalProgramError, NamedGlobalProgramError, NamedGlobalProgramOutcome,
};
use super::MissionContentOwner;
use crate::actor_world::{ActorPhase, ActorProgramKind, ActorSource, ActorWorld};
use crate::frame_lookup::{
    FrameLookupBackend, FrameLookupError, FrameView, ModelFrameLookup, SceneRoots,
};
use crate::frame_transform::FrameKey;
use crate::model_loading::ModelArena;
use crate::original_program::{
    CProgram, LookupRuntimeBackend, RuntimeBackend, RuntimeError, RuntimeLookupWorld, RuntimeStage,
    RuntimeView,
};
use crate::scene_ownership::SceneKey;
use crate::script_lookup::{DetectorCollisionFlags, LookupError, LookupWorldBackend};
use crate::script_scheduler::{ActorReferenceId, DispatchBudget, FrameReferenceId};

/// The two source libraries have independent CRT locale admissions.
#[derive(Clone, Copy, Debug)]
pub struct ProgramWorldLocales {
    pub game_default_c_locale: bool,
    pub ls3df_default_c_locale: bool,
}
/// A required diagnostic can mutate these actual owners before lookup rereads them.
/// It cannot replace or retire the executing program during its exclusive borrow.
/// The borrowed ActorWorld itself must remain the same owner. Embedded dispatch
/// leases reject current-actor lifetime/source mutations, while ordinary state
/// mutations and operations on other actors remain available. Replacing the
/// entire world destroys that restoration guarantee; it is detected without
/// attaching the borrowed program to the replacement or masking a panic.
pub trait MissionProgramDiagnostic {
    fn message_beep(
        &mut self,
        world: &mut ActorWorld,
        arena: &mut ModelArena,
        fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        scene: &mut FrameReferenceId,
        argument: i32,
    ) -> Result<(), LookupError>;
}
/// Borrowed invocation context, never a scene/program/actor owner. The scene
/// selection is borrowed from the caller and retains its arena identity; a
/// callback can redirect that same slot before the next reached Scene read.
/// Validation is lazy: an instruction that does not read Scene needs no Scene.
pub struct MissionProgramWorldContext<'a, B, D> {
    arena: &'a mut ModelArena,
    scene: &'a mut FrameReferenceId,
    effects: &'a mut B,
    diagnostic: &'a mut D,
    locales: ProgramWorldLocales,
}
impl<'a, B: RuntimeBackend, D: MissionProgramDiagnostic> MissionProgramWorldContext<'a, B, D> {
    pub fn new(
        arena: &'a mut ModelArena,
        scene: &'a mut FrameReferenceId,
        effects: &'a mut B,
        diagnostic: &'a mut D,
        locales: ProgramWorldLocales,
    ) -> Self {
        Self {
            arena,
            scene,
            effects,
            diagnostic,
            locales,
        }
    }
    pub fn arena_mut(&mut self) -> &mut ModelArena {
        self.arena
    }
    pub fn scene_mut(&mut self) -> &mut FrameReferenceId {
        self.scene
    }
    pub fn effects_mut(&mut self) -> &mut B {
        self.effects
    }
    /// A real game-global owner can execute its own program against the same
    /// mission world. Its collection/source ownership stays with that caller.
    pub fn call_subroutine(
        &mut self,
        program: &mut CProgram,
        world: &mut ActorWorld,
        budget: &mut DispatchBudget,
    ) -> Result<(), RuntimeError> {
        let (effects, mut live) = self.split_world(world);
        program.call_owned_subroutine(budget, &mut LookupRuntimeBackend::new(effects, &mut live))
    }
    pub fn call_process(
        &mut self,
        program: &mut CProgram,
        world: &mut ActorWorld,
        delta: i32,
        budget: &mut DispatchBudget,
    ) -> Result<(), RuntimeError> {
        let (effects, mut live) = self.split_world(world);
        program.call_owned_process(
            delta,
            budget,
            &mut LookupRuntimeBackend::new(effects, &mut live),
        )
    }
    /// Compile every Ready entity actor's embedded program that has source
    /// but no runtime yet, in host ownership order. A host dispatch lease
    /// restores the actual program on errors and unwind while the borrowed
    /// world is preserved; runtime stages and one-shot admission stay intact.
    /// Like global program init, the caller must run under a Pc24PrecisionState
    /// scope so transcendental boundary admissions can record responsibility.
    /// Ready runtimes are left untouched and uncounted. Interrupted runtimes
    /// and detached manual borrows refuse, rather than becoming successful skips.
    pub fn init_entity_programs(
        &mut self,
        world: &mut ActorWorld,
    ) -> Result<usize, EntityTickError> {
        let actors: Vec<ActorReferenceId> = world.owned_actor_references().collect();
        let mut initialized = 0usize;
        for actor in actors {
            if world.phase(actor) != Ok(ActorPhase::Ready) {
                continue;
            }
            if world.entity(actor).is_err() {
                continue;
            }
            let fresh = world
                .with_program_dispatch_lease(actor, ActorProgramKind::Entity, |_, program| {
                    init_program_once(program, self.effects)
                })
                .map_err(|_| EntityTickError::Actor(actor))?
                .map_err(EntityTickError::Runtime)?;
            initialized += usize::from(fresh);
        }
        Ok(initialized)
    }
    /// Compile every Ready detector actor's embedded program that has source
    /// but no runtime yet, in host ownership order. This uses the same host
    /// dispatch lease and interrupted-runtime admission as the entity pass.
    /// It connects the existing init pass for C_detector::GameInit (mafia-re 113b5b6
    /// game/actors/C_detector.cpp:44-58, GAME 0x432a00) C_program::Init prefix
    /// (GAME 0x461580): detector sources decode with the base table because
    /// C_detector::DecodeInstruction/DecodeParams forward to C_program
    /// (C_detector.cpp:433ce0-433d00). Like the entity pass, the caller must
    /// run under a Pc24PrecisionState scope so transcendental boundary
    /// admissions can record responsibility. Ready runtimes are left untouched
    /// and uncounted. The GameInit body beyond the Init prefix,
    /// GameDone and the collision/use-object owners stay DEFERRED; the tick
    /// itself runs through [`Self::tick_detector_programs`].
    pub fn init_detector_programs(
        &mut self,
        world: &mut ActorWorld,
    ) -> Result<usize, DetectorTickError> {
        let actors: Vec<ActorReferenceId> = world.owned_actor_references().collect();
        let mut initialized = 0usize;
        for actor in actors {
            if world.phase(actor) != Ok(ActorPhase::Ready) {
                continue;
            }
            if world.detector(actor).is_err() {
                continue;
            }
            let fresh = world
                .with_program_dispatch_lease(actor, ActorProgramKind::Detector, |_, program| {
                    init_program_once(program, self.effects)
                })
                .map_err(|_| DetectorTickError::Actor(actor))?
                .map_err(DetectorTickError::Runtime)?;
            initialized += usize::from(fresh);
        }
        Ok(initialized)
    }
    /// Step every Ready entity actor's embedded program once against the live
    /// world, in host ownership order, sharing one budget. Its host dispatch
    /// lease restores the actual program on errors and unwind while the world
    /// is preserved. A missing embedded program is a detached borrow and
    /// refuses; non-Ready and non-entity actors do not tick.
    /// Program owners are assigned at actor Init; ownerless programs refuse
    /// inside dispatch like any worldless execution. This is the AI tick
    /// transport, not a Game tick: no game clock, camera, input or objectives
    /// run here.
    pub fn tick_entity_programs(
        &mut self,
        world: &mut ActorWorld,
        delta: i32,
        budget: &mut DispatchBudget,
    ) -> Result<usize, EntityTickError> {
        let actors: Vec<ActorReferenceId> = world.owned_actor_references().collect();
        let mut stepped = 0usize;
        for actor in actors {
            if world.phase(actor) != Ok(ActorPhase::Ready) {
                continue;
            }
            if world.entity(actor).is_err() {
                continue;
            }
            world
                .with_program_dispatch_lease(actor, ActorProgramKind::Entity, |world, program| {
                    self.call_process(program, world, delta, budget)
                })
                .map_err(|_| EntityTickError::Actor(actor))?
                .map_err(EntityTickError::Runtime)?;
            stepped += 1;
        }
        Ok(stepped)
    }
    /// Step every Ready detector actor's embedded program once against the live
    /// world, in host ownership order, sharing one budget. The host dispatch
    /// lease has the same restoration and detached-borrow contract as the
    /// entity pass; non-Ready and non-detector actors do not tick.
    /// Detector-vtable programs run the C_detector::Process dispatch (GAME
    /// RVA 0x33d10) inside CallProcess: PROGRAM_STORAGE parks, INRANGE scans
    /// the live active actors, and the WAITFORUSE/WAITFORHIT/CREATEDYNCOLL/
    /// ERASEDYNCOLL arms latch before their unavailable collision/use-object
    /// owners refuse loudly. Program owners are assigned at actor Init;
    /// ownerless programs refuse inside dispatch like any worldless execution.
    /// This is the AI tick transport, not a Game tick: no game clock, camera,
    /// input or objectives run here.
    pub fn tick_detector_programs(
        &mut self,
        world: &mut ActorWorld,
        delta: i32,
        budget: &mut DispatchBudget,
    ) -> Result<usize, DetectorTickError> {
        let actors: Vec<ActorReferenceId> = world.owned_actor_references().collect();
        let mut stepped = 0usize;
        for actor in actors {
            if world.phase(actor) != Ok(ActorPhase::Ready) {
                continue;
            }
            if world.detector(actor).is_err() {
                continue;
            }
            world
                .with_program_dispatch_lease(actor, ActorProgramKind::Detector, |world, program| {
                    self.call_process(program, world, delta, budget)
                })
                .map_err(|_| DetectorTickError::Actor(actor))?
                .map_err(DetectorTickError::Runtime)?;
            stepped += 1;
        }
        Ok(stepped)
    }
    fn split_world<'b>(
        &'b mut self,
        world: &'b mut ActorWorld,
    ) -> (&'b mut B, MissionLookupWorld<'b, D>) {
        (
            &mut *self.effects,
            MissionLookupWorld {
                world,
                arena: &mut *self.arena,
                scene: &mut *self.scene,
                diagnostic: &mut *self.diagnostic,
                locales: self.locales,
            },
        )
    }
}

fn init_program_once<B: RuntimeBackend>(
    program: &mut CProgram,
    effects: &mut B,
) -> Result<bool, RuntimeError> {
    match program.runtime() {
        Some(runtime) if runtime.stage == RuntimeStage::Ready => Ok(false),
        Some(_) => Err(RuntimeError::Domain("runtime not ready")),
        None => program.init_fresh_runtime(effects).map(|_| true),
    }
}

/// Required game-global call, before any mission member or borrowed lookup view
/// is selected. A missing game owner must return an error, never a default miss.
pub trait GameGlobalProgramWorldRunner<B: RuntimeBackend, D: MissionProgramDiagnostic> {
    fn run_game_global(
        &mut self,
        mission: &mut MissionContentOwner,
        name: &[u8],
        budget: &mut DispatchBudget,
        context: &mut MissionProgramWorldContext<'_, B, D>,
    ) -> Result<bool, String>;
}

struct MissionLookupWorld<'a, D> {
    world: &'a mut ActorWorld,
    arena: &'a mut ModelArena,
    scene: &'a mut FrameReferenceId,
    diagnostic: &'a mut D,
    locales: ProgramWorldLocales,
}
impl<D> FrameLookupBackend for MissionLookupWorld<'_, D> {
    fn default_c_locale(&self) -> bool {
        self.locales.ls3df_default_c_locale
    }
    fn supports_empty_query(&self) -> bool {
        false
    }
    fn frame(&self, reference: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
        ModelFrameLookup::arena_frame(self.arena, reference)
    }
}
impl<D> LookupWorldBackend for MissionLookupWorld<'_, D> {
    fn diagnostic_default_c_locale(&self) -> bool {
        self.locales.game_default_c_locale
    }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        Err(LookupError::Backend(
            "lookup requires the actual program owner".into(),
        ))
    }
    fn actor_frame(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<FrameReferenceId>, LookupError> {
        self.world.lookup_actor_frame(actor)
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.world.source(ActorSource::Primary))
    }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> {
        Ok(self.world.players)
    }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.world.source(ActorSource::Static))
    }
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> {
        Ok(self)
    }
    fn scene(&self) -> Result<SceneRoots, LookupError> {
        let reference = *self.scene;
        let key = FrameKey(reference.identity());
        if self.arena.lookup_reference(key) != reference {
            return Err(FrameLookupError::StaleFrame(reference).into());
        }
        let scene = self
            .arena
            .scene(SceneKey(key))
            .map_err(|error| LookupError::Backend(format!("program Scene: {error:?}")))?;
        Ok(SceneRoots {
            primary: Some(self.arena.lookup_reference(scene.sectors[0].frame())),
            backdrop: Some(self.arena.lookup_reference(scene.sectors[1].frame())),
        })
    }
    fn message_beep(&mut self, _: i32) -> Result<(), LookupError> {
        Err(LookupError::Backend(
            "diagnostic requires live program state".into(),
        ))
    }
    fn load_entity_process_state(
        &mut self,
        owner: Option<ActorReferenceId>,
    ) -> Result<crate::entity_actor::EntityProcessState, LookupError> {
        let actor =
            owner.ok_or_else(|| LookupError::Backend("entity command without owner".into()))?;
        self.world
            .entity_process_state(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn store_entity_process_state(
        &mut self,
        owner: Option<ActorReferenceId>,
        state: &crate::entity_actor::EntityProcessState,
    ) -> Result<(), LookupError> {
        let actor =
            owner.ok_or_else(|| LookupError::Backend("entity command without owner".into()))?;
        self.world
            .store_entity_process_state(actor, state)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn set_actor_act_state(
        &mut self,
        actor: ActorReferenceId,
        state: u32,
    ) -> Result<(), LookupError> {
        self.world
            .set_actor_act_state(actor, state)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn human_add_weapon(
        &mut self,
        actor: ActorReferenceId,
        id: u16,
        loaded: i32,
        spare: i32,
    ) -> Result<(), LookupError> {
        match self.world.human_add_weapon(actor, id, loaded, spare) {
            // The native type gate breaks silently; only real host defects
            // (stale actors and below) propagate.
            Err(crate::actor_world::ActorWorldError::NotHuman) => Ok(()),
            result => result.map_err(|error| LookupError::Backend(format!("{error:?}"))),
        }
    }
    fn detector_signal(&self, actor: ActorReferenceId) -> Result<Option<bool>, LookupError> {
        match self.world.detector_signal(actor) {
            Ok(signal) => Ok(Some(signal)),
            // The native ISSIGNAL type gate falls through to the next
            // command; only real host defects propagate.
            Err(crate::actor_world::ActorWorldError::NotDetector) => Ok(None),
            Err(error) => Err(LookupError::Backend(format!("{error:?}"))),
        }
    }
    fn set_detector_signal(
        &mut self,
        actor: ActorReferenceId,
        on: bool,
    ) -> Result<(), LookupError> {
        match self.world.set_detector_signal(actor, on) {
            Ok(()) => Ok(()),
            // The native SETSIGNAL type gate stores nothing; only real host
            // defects (stale actors and below) propagate.
            Err(crate::actor_world::ActorWorldError::NotDetector) => Ok(()),
            result => result.map_err(|error| LookupError::Backend(format!("{error:?}"))),
        }
    }
    fn detector_human_types(&self, actor: ActorReferenceId) -> Result<u32, LookupError> {
        // The INRANGE owner is implicit, so unlike ISSIGNAL there is no
        // silent type gate: a non-detector reports like any host defect.
        self.world
            .detector_human_types(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn active_actors(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        // The native C_detector::Process scan vector
        // (`act_mission->game->active_actors`, C_detector.cpp:212).
        Ok(self.world.source(ActorSource::Active))
    }
    fn detector_inrange_candidate(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<(Option<FrameReferenceId>, u32)>, LookupError> {
        self.world
            .detector_inrange_candidate(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn detector_collision_flags(
        &self,
        actor: ActorReferenceId,
    ) -> Result<DetectorCollisionFlags, LookupError> {
        self.world
            .detector_collision_flags(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn set_detector_collision_flags(
        &mut self,
        actor: ActorReferenceId,
        flags: DetectorCollisionFlags,
    ) -> Result<(), LookupError> {
        self.world
            .set_detector_collision_flags(actor, flags)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    // Host empty-registry driver for C_detector::EraseColls (mafia-re
    // game/actors/C_detector.cpp:146-166, GAME 0x433ae0): the live world
    // holds no colls/matrices/num storage yet, so the native loop over
    // num_colls == 0 is a no-op and the null-array deletes are safe. Validate
    // the live detector owner, then succeed vacuously without inventing a
    // registry. CreateColls (STUB GAME 0x4330b0) and CreateUO (STUB GAME
    // 0x433b80) keep the trait default refusal, so WAITFORHIT, CREATEDYNCOLL
    // and WAITFORUSE still stop loudly there; ERASEDYNCOLL now advances.
    fn detector_erase_colls(&mut self, actor: ActorReferenceId) -> Result<(), LookupError> {
        self.world
            .detector_collision_flags(actor)
            .map(|_| ())
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    // detector_create_colls and detector_create_use_object keep the trait
    // default refusal (UnsupportedWorldBackend): the dynamic-collision
    // registry and the using-object registry have no owner yet, so the
    // WAITFORHIT, CREATEDYNCOLL and WAITFORUSE arms stop loudly there.
    fn set_program_event(
        &mut self,
        actor: ActorReferenceId,
        event_id: i32,
    ) -> Result<bool, LookupError> {
        self.world
            .program_set_event(actor, event_id)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn actor_dead(&self, actor: ActorReferenceId) -> Result<bool, LookupError> {
        self.world
            .actor_dead(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn frame_world_position(&mut self, frame: FrameReferenceId) -> Result<[f32; 3], LookupError> {
        let key = FrameKey(frame.identity());
        if self.arena.lookup_reference(key) != frame {
            return Err(crate::frame_lookup::FrameLookupError::StaleFrame(frame).into());
        }
        let matrix = self
            .arena
            .world_matrix(key)
            .map_err(|error| LookupError::Backend(format!("program frame world pos: {error:?}")))?;
        matrix
            .try_transform_point([0.0; 3])
            .map_err(|error| LookupError::Backend(format!("program frame world pos: {error:?}")))
    }
    // Original CMD_FRM_SETON frame switch (mafia-re
    // game/mission/C_program.cpp:3134): the core arena on/off store over
    // the live frame. Sound frames take the deferred device switch with
    // no arena store, matching the native break; every other kind stores
    // through the evidenced SetOn owner. Chase-camera, placement,
    // projector and shadow side effects stay deferred (reported, never
    // applied), mirroring the outcome contract.
    fn set_frame_on(
        &mut self,
        frame: FrameReferenceId,
        on: bool,
    ) -> Result<crate::script_lookup::FrameSetOnOutcome, LookupError> {
        let key = FrameKey(frame.identity());
        if self.arena.lookup_reference(key) != frame {
            return Err(crate::frame_lookup::FrameLookupError::StaleFrame(frame).into());
        }
        let kind = self
            .arena
            .state(key)
            .map(|state| state.kind)
            .map_err(|error| LookupError::Backend(format!("program frame switch: {error:?}")))?;
        if kind == crate::frame_transform::FrameKind::Sound {
            return Ok(crate::script_lookup::FrameSetOnOutcome {
                core_applied: false,
                device_effects_deferred: true,
            });
        }
        self.arena
            .set_frame_on(key, u32::from(on))
            .map_err(|error| LookupError::Backend(format!("program frame switch: {error:?}")))?;
        Ok(crate::script_lookup::FrameSetOnOutcome {
            core_applied: true,
            device_effects_deferred: true,
        })
    }
    fn human_car(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<Option<ActorReferenceId>>, LookupError> {
        self.world
            .human_car(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn car_speed(&self, actor: ActorReferenceId) -> Result<Option<f32>, LookupError> {
        self.world
            .car_speed(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn is_car_usable(&self, actor: ActorReferenceId) -> Result<Option<bool>, LookupError> {
        self.world
            .is_car_usable(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn set_car_break_motor(
        &mut self,
        actor: ActorReferenceId,
        on: bool,
    ) -> Result<bool, LookupError> {
        self.world
            .set_car_break_motor(actor, on)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn force_stop_car(&mut self, actor: ActorReferenceId) -> Result<bool, LookupError> {
        self.world
            .force_stop_car(actor)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
    fn set_player_controls_locked(&mut self, locked: bool) -> Result<(), LookupError> {
        self.world
            .set_player_controls_locked(locked)
            .map_err(|error| LookupError::Backend(format!("{error:?}")))
    }
}
impl<D: MissionProgramDiagnostic> RuntimeLookupWorld for MissionLookupWorld<'_, D> {
    fn runtime_message_beep(
        &mut self,
        fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        argument: i32,
    ) -> Result<(), LookupError> {
        self.diagnostic
            .message_beep(self.world, self.arena, fields, owner, self.scene, argument)
    }
}

/// One entity init/tick pass failed at actor admission or runtime dispatch.
/// The host lease restores the program before reporting while its borrowed
/// ActorWorld is preserved; it never resets interrupted runtime admission.
#[derive(Debug, PartialEq, Eq)]
pub enum EntityTickError {
    Actor(ActorReferenceId),
    Runtime(RuntimeError),
}

/// One detector init/tick pass failed at actor admission or runtime dispatch.
/// The host lease preserves the same owner and interruption contract. This
/// mirrors [`EntityTickError`] for the detector transport, covering both the
/// GameInit-prefix compile pass and the C_detector::Process tick.
#[derive(Debug, PartialEq, Eq)]
pub enum DetectorTickError {
    Actor(ActorReferenceId),
    Runtime(RuntimeError),
}

impl MissionContentOwner {
    /// Run one already-initialized source-owned program by index against the
    /// live world through the caller's context. Program owners ride the
    /// existing Init-time assignment; scheduling state stays with the caller.
    pub fn process_program_in_world<B: RuntimeBackend, D: MissionProgramDiagnostic>(
        &mut self,
        index: usize,
        delta: i32,
        budget: &mut DispatchBudget,
        context: &mut MissionProgramWorldContext<'_, B, D>,
    ) -> Result<(), MissionGlobalProgramError> {
        self.available()
            .map_err(MissionGlobalProgramError::SourceUnavailable)?;
        if self.pending_program.is_some() {
            return Err(MissionGlobalProgramError::PendingSource);
        }
        let program = self
            .programs
            .get_mut(index)
            .ok_or(MissionGlobalProgramError::MissingCurrentProgram(index))?;
        context
            .call_process(program, &mut self.actors.world, delta, budget)
            .map_err(MissionGlobalProgramError::Runtime)
    }
    pub fn global_program_run_in_world<B, D, G>(
        &mut self,
        name: &[u8],
        game: &mut G,
        budget: &mut DispatchBudget,
        context: &mut MissionProgramWorldContext<'_, B, D>,
    ) -> Result<NamedGlobalProgramOutcome, NamedGlobalProgramError>
    where
        B: RuntimeBackend,
        D: MissionProgramDiagnostic,
        G: GameGlobalProgramWorldRunner<B, D>,
    {
        self.named_program_run_admission(name)?;
        if game
            .run_game_global(self, name, budget, context)
            .map_err(NamedGlobalProgramError::GameOwner)?
        {
            return Ok(NamedGlobalProgramOutcome::GameHandled);
        }
        let Some(index) = self.named_mission_program_index(name)? else {
            return Ok(NamedGlobalProgramOutcome::NoMatch);
        };
        let program = self
            .programs
            .get_mut(index)
            .ok_or(NamedGlobalProgramError::MissingCurrentProgram(index))?;
        context
            .call_subroutine(program, &mut self.actors.world, budget)
            .map_err(|error| NamedGlobalProgramError::Runtime { index, error })?;
        Ok(NamedGlobalProgramOutcome::MissionHandled { index })
    }
}

#[cfg(test)]
#[path = "mission_program_world_tests.rs"]
mod tests;
