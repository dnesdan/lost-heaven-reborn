//! Mission-owned program execution borrowing the same actors and scene arena.
//! This host binding adds no command, scheduler, GameInit or original-function credit.
#![forbid(unsafe_code)]
use super::global_program_lifecycle::{
    MissionGlobalProgramError, NamedGlobalProgramError, NamedGlobalProgramOutcome,
};
use super::MissionContentOwner;
use crate::actor_world::{ActorPhase, ActorSource, ActorWorld};
use crate::frame_lookup::{
    FrameLookupBackend, FrameLookupError, FrameView, ModelFrameLookup, SceneRoots,
};
use crate::frame_transform::FrameKey;
use crate::model_loading::ModelArena;
use crate::original_program::{
    CProgram, LookupRuntimeBackend, RuntimeBackend, RuntimeError, RuntimeLookupWorld, RuntimeView,
};
use crate::scene_ownership::SceneKey;
use crate::script_lookup::{LookupError, LookupWorldBackend};
use crate::script_scheduler::{ActorReferenceId, DispatchBudget, FrameReferenceId};

/// The two source libraries have independent CRT locale admissions.
#[derive(Clone, Copy, Debug)]
pub struct ProgramWorldLocales {
    pub game_default_c_locale: bool,
    pub ls3df_default_c_locale: bool,
}
/// A required diagnostic can mutate these actual owners before lookup rereads them.
/// It cannot replace or retire the executing program during its exclusive borrow.
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
    /// but no runtime yet, in ownership order. Each program is taken out of
    /// its actor, initialized through this context's effects backend, and
    /// returned on every path including failed inits. This mirrors the global
    /// program init; like it, the caller must run under a Pc24PrecisionState
    /// scope so transcendental boundary admissions can record responsibility.
    /// Already-initialized programs are left untouched and uncounted.
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
            let program = world
                .take_entity_program(actor)
                .map_err(|_| EntityTickError::Actor(actor))?;
            let Some(mut program) = program else {
                continue;
            };
            let freshly_initialized = program.runtime().is_none();
            let result = if freshly_initialized {
                program
                    .init_fresh_runtime(&mut *self.effects)
                    .map(|_| ())
                    .map_err(EntityTickError::Runtime)
            } else {
                Ok(())
            };
            world
                .return_entity_program(actor, Some(program))
                .map_err(|_| EntityTickError::Actor(actor))?;
            result?;
            if freshly_initialized {
                initialized += 1;
            }
        }
        Ok(initialized)
    }
    /// Step every Ready entity actor's embedded program once against the live
    /// world, in ownership order, sharing one budget. Each program is taken
    /// out of its actor, run through this context, and returned on every path
    /// including failed ticks. Actors without a program (consumed or never
    /// loaded) are skipped; non-Ready and non-entity actors do not tick.
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
            let program = world
                .take_entity_program(actor)
                .map_err(|_| EntityTickError::Actor(actor))?;
            let Some(mut program) = program else {
                continue;
            };
            let result = self.call_process(&mut program, world, delta, budget);
            world
                .return_entity_program(actor, Some(program))
                .map_err(|_| EntityTickError::Actor(actor))?;
            result.map_err(EntityTickError::Runtime)?;
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
    fn set_detector_signal(&mut self, actor: ActorReferenceId, on: bool) -> Result<(), LookupError> {
        match self.world.set_detector_signal(actor, on) {
            Ok(()) => Ok(()),
            // The native SETSIGNAL type gate stores nothing; only real host
            // defects (stale actors and below) propagate.
            Err(crate::actor_world::ActorWorldError::NotDetector) => Ok(()),
            result => result.map_err(|error| LookupError::Backend(format!("{error:?}"))),
        }
    }
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
    fn frame_world_position(
        &mut self,
        frame: FrameReferenceId,
    ) -> Result<[f32; 3], LookupError> {
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

/// One entity-actor tick step failed: which actor, and the dispatch error.
/// A failed tick still returns the taken program before reporting.
#[derive(Debug, PartialEq, Eq)]
pub enum EntityTickError {
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
