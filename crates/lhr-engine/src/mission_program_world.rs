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
    /// Step every Ready entity actor's embedded program once against the live
    /// world, in ownership order, sharing one budget. Each program is taken
    /// out of its actor, run through the caller's context, and returned on
    /// every path including failed ticks. Actors without a program (consumed
    /// or destructured) are skipped; non-Ready actors do not tick. Program
    /// owners are assigned at actor Init; ownerless programs refuse inside
    /// dispatch like any worldless execution. This is the AI tick transport,
    /// not a Game tick: no game clock, camera, input or objectives run here.
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
