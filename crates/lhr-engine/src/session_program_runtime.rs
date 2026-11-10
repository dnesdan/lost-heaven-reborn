//! Host execution over the session's existing program, actor, frame and game
//! owners. This connects already implemented operations; it does not assign
//! native scheduling order or count a new reconstructed original function.
#![forbid(unsafe_code)]

use super::*;
use crate::actor_world::ActorWorld;
use crate::game_housekeeping::GameInlineState;
use crate::mission_content_loading::{DetectorTickError, EntityTickError, GlobalProgramError};
use crate::original_program::{RuntimeBackend, RuntimeEffect, RuntimeView};
use crate::program_parameters::ScalarParameter;

/// CProgram performs and owns the Rust allocations after each successful
/// storage notification. Gameplay requests must reach an actual game owner;
/// an unavailable owner is never acknowledged as a counting-only effect.
pub struct SessionProgramEffects<'a> {
    game: &'a mut GameInlineState,
    lights: crate::mission_effects::SessionLightGraph,
}

/// Snapshot one scene's light-relevant words for a program-effects owner.
/// Selection failures surface here; a missing scene yields an empty graph
/// whose active scene is unavailable at opcode time.
fn session_light_snapshot<E>(
    session: &MissionLoadSession,
    unavailable: impl FnOnce(String) -> E,
) -> Result<crate::mission_effects::SessionLightGraph, E> {
    let key = session
        .scene()
        .map_err(|error| unavailable(format!("{error:?}")))?;
    Ok(crate::mission_effects::snapshot_session_lights(
        &session.arena,
        key,
    ))
}

/// Admission or execution failure of an explicit host actor-program pass.
/// The retained actors and programs keep every reached change on failure.
#[derive(Debug, PartialEq, Eq)]
pub enum SessionActorProgramError {
    SourceUnavailable(String),
    Entity(EntityTickError),
    Detector(DetectorTickError),
}

impl RuntimeBackend for SessionProgramEffects<'_> {
    fn effect(
        &mut self,
        _fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        use RuntimeEffect::*;
        match effect {
            SetNightMission(value) => self.game.set_night_mission(value),
            DisableAnimPreload => self.game.set_anim_preload(false),
            UseLightCache => match crate::mission_effects::collect_session_lights(&self.lights) {
                Ok(collected) => {
                    self.game.light_cache = Some(collected);
                }
                Err(error) => return Err(error),
            },
            SetCityTrafficVisible(_) => {
                return Err("session city-traffic visibility owner unavailable".into());
            }
            MissionObjectivesAdd(id) => self.game.mission_objectives_add(id as u32),
            MissionObjectivesRemove(id) => self.game.mission_objectives_remove(id as u32),
            MissionObjectivesClear => self.game.mission_objectives_clear(),
            IntroSubtitleAdd(first, second) => self.game.mission_intro_subtitle_add(first, second),
            SubtitleAdd(id) => self.game.mission_subtitle_add(id),
            // Exhaustive storage-only list: adding another gameplay effect
            // requires an explicit owner decision here at compile time.
            AllocateWorkingSource(_)
            | ReleaseWorkingSource
            | AllocateScalar
            | ConvertScalar
            | AllocateName
            | ConvertNameRegister
            | ReleaseName(_)
            | AllocatePair
            | ConvertPair(_)
            | ReleasePair(_)
            | AllocateQuad
            | ConvertQuad(_)
            | ReleaseQuad(_)
            | AllocateActState
            | ConvertActStateActor
            | ConvertActState
            | ReleaseActState(_)
            | AllocateBlock
            | ReleaseBlock(_)
            | AllocateIfFlt
            | ConvertIfFlt(_)
            | ReleaseIfFlt(_)
            | AllocateEvent
            | ConvertEventId
            | ConvertEventExtra
            | ReleaseEvent(_)
            | AllocateSetEvent
            | ConvertSetEventRegister
            | ConvertSetEventId
            | ReleaseSetEvent(_)
            | AllocateDetectorSignal
            | ConvertDetectorSignalRegister
            | ReleaseDetectorSignal(_)
            | AllocateGetActorsDist
            | ConvertGetActorsDist(_)
            | ReleaseGetActorsDist(_)
            | AllocateHumanTalk
            | ConvertHumanTalk(_)
            | ReleaseHumanTalk(_)
            | AllocateEndOfMission
            | ConvertEndOfMission(_)
            | ReleaseEndOfMission(_)
            | AllocateChangeMission
            | ReleaseChangeMission(_)
            | AllocateRemaining(_)
            | ConvertRemaining(_)
            | ReleaseRemaining(_)
            | AllocateFlow
            | ConvertFlowOperand(_)
            | ReleaseFlow(_)
            | AllocateRecords(_)
            | ReleaseOldRecordBacking
            | ReleaseScalar(_)
            | ReleaseActors
            | ReleaseFrames
            | AllocateActors(_)
            | AllocateFrames(_)
            | AllocateFloats(_)
            | ReleaseFloats => {}
        }
        Ok(())
    }
}

impl MissionLoadSession {
    /// Read the authoritative represented game fields. This is a borrow, not
    /// an inspector copy used as a replacement gameplay owner.
    pub fn game_state(&self) -> &GameInlineState {
        &self.game_inline
    }

    /// Compile fresh embedded entity programs with this session's actual
    /// effects owner and precision scope. Ready programs are left alone;
    /// interrupted or terminal owners refuse. This is an explicit host pass,
    /// not actor GameInit or admission of a native scheduling order.
    pub fn initialize_owned_entity_programs<D: MissionProgramDiagnostic>(
        &mut self,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<usize, SessionActorProgramError> {
        self.with_owned_actor_programs(diagnostic, locales, |world, context| {
            context
                .init_entity_programs(world)
                .map_err(SessionActorProgramError::Entity)
        })
    }

    /// Compile fresh embedded detector programs with the same owner and
    /// admission contract as [`Self::initialize_owned_entity_programs`].
    /// Detector GameInit and collision/use-object dependencies remain separate.
    pub fn initialize_owned_detector_programs<D: MissionProgramDiagnostic>(
        &mut self,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<usize, SessionActorProgramError> {
        self.with_owned_actor_programs(diagnostic, locales, |world, context| {
            context
                .init_detector_programs(world)
                .map_err(SessionActorProgramError::Detector)
        })
    }

    /// Execute one explicit entity-program pass against this session's live
    /// actors, scene and represented game fields. The caller retains the same
    /// budget across passes; this call never replenishes it. Programs return
    /// to their actor on failure/unwind with interrupted state intact, subject
    /// to the diagnostic's borrowed-owner contract. This is not a Game tick.
    pub fn tick_owned_entity_programs<D: MissionProgramDiagnostic>(
        &mut self,
        delta: i32,
        budget: &mut DispatchBudget,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<usize, SessionActorProgramError> {
        self.with_owned_actor_programs(diagnostic, locales, |world, context| {
            context
                .tick_entity_programs(world, delta, budget)
                .map_err(SessionActorProgramError::Entity)
        })
    }

    /// Execute one explicit detector-program pass with the same owner and
    /// budget contract as [`Self::tick_owned_entity_programs`]. The caller
    /// selects when each kind runs; no native cross-kind order is inferred.
    pub fn tick_owned_detector_programs<D: MissionProgramDiagnostic>(
        &mut self,
        delta: i32,
        budget: &mut DispatchBudget,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<usize, SessionActorProgramError> {
        self.with_owned_actor_programs(diagnostic, locales, |world, context| {
            context
                .tick_detector_programs(world, delta, budget)
                .map_err(SessionActorProgramError::Detector)
        })
    }

    fn with_owned_actor_programs<D: MissionProgramDiagnostic, R>(
        &mut self,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
        operation: impl FnOnce(
            &mut ActorWorld,
            &mut MissionProgramWorldContext<'_, SessionProgramEffects<'_>, D>,
        ) -> Result<R, SessionActorProgramError>,
    ) -> Result<R, SessionActorProgramError> {
        self.global_program_source_admission()
            .map_err(SessionActorProgramError::SourceUnavailable)?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                let lights = session_light_snapshot(session, |detail| {
                    SessionActorProgramError::SourceUnavailable(detail)
                })?;
                let scene = session.scene.as_mut().ok_or_else(|| {
                    SessionActorProgramError::SourceUnavailable("session Scene unavailable".into())
                })?;
                let mut effects = SessionProgramEffects {
                    game: &mut session.game_inline,
                    lights,
                };
                operation(
                    &mut session.loader.as_mut().unwrap().content.actors.world,
                    &mut MissionProgramWorldContext::new(
                        &mut session.arena,
                        scene,
                        &mut effects,
                        diagnostic,
                        locales,
                    ),
                )
            },
        )
    }

    /// Compile the retained mission-global source owners with Rust-owned
    /// allocations. This explicit dependency call does not run C_game::Init,
    /// GameInitStart or any actor; it must be scheduled by its actual caller.
    pub fn initialize_owned_global_programs(
        &mut self,
    ) -> Result<(), GlobalProgramError<MissionGlobalProgramError>> {
        self.global_program_admission()
            .map_err(GlobalProgramError::EntryCountUnavailable)?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                let lights = session_light_snapshot(session, |detail| {
                    GlobalProgramError::EntryCountUnavailable(
                        MissionGlobalProgramError::SourceUnavailable(detail),
                    )
                })?;
                session
                    .loader
                    .as_mut()
                    .unwrap()
                    .content
                    .global_program_game_init(&mut SessionProgramEffects {
                        game: &mut session.game_inline,
                        lights,
                    })
            },
        )
    }

    /// Named program dispatch with the required Game-global owner first and
    /// the current mission collection as fallback. Both receive the same
    /// session-bound effect backend; an unavailable Game owner must return an
    /// error rather than claiming that its collection is empty.
    pub fn run_owned_global_program_in_world<D, G>(
        &mut self,
        name: &[u8],
        game: &mut G,
        budget: &mut DispatchBudget,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<NamedGlobalProgramOutcome, NamedGlobalProgramError>
    where
        D: MissionProgramDiagnostic,
        G: for<'a> GameGlobalProgramWorldRunner<SessionProgramEffects<'a>, D>,
    {
        self.global_program_source_admission()
            .map_err(NamedGlobalProgramError::SourceUnavailable)?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                let lights = session_light_snapshot(session, |detail| {
                    NamedGlobalProgramError::SourceUnavailable(detail)
                })?;
                let scene = session.scene.as_mut().ok_or_else(|| {
                    NamedGlobalProgramError::SourceUnavailable("session Scene unavailable".into())
                })?;
                let mut effects = SessionProgramEffects {
                    game: &mut session.game_inline,
                    lights,
                };
                session
                    .loader
                    .as_mut()
                    .unwrap()
                    .content
                    .global_program_run_in_world(
                        name,
                        game,
                        budget,
                        &mut MissionProgramWorldContext::new(
                            &mut session.arena,
                            scene,
                            &mut effects,
                            diagnostic,
                            locales,
                        ),
                    )
            },
        )
    }

    /// Execute a retained program against this session's actual actors,
    /// frames and represented game fields. The caller supplies the real
    /// diagnostic boundary, elapsed time and shared dispatch budget. No
    /// independent world or accept-all gameplay backend is installed.
    ///
    /// Game/actor scheduling, player/taxi control and unsupported command
    /// owners remain separate dependencies; a successful call is not a tick
    /// of the full game. Failed dispatch retains the actual program prefix.
    pub fn process_owned_program_in_world<D: MissionProgramDiagnostic>(
        &mut self,
        index: usize,
        delta: i32,
        budget: &mut DispatchBudget,
        diagnostic: &mut D,
        locales: ProgramWorldLocales,
    ) -> Result<(), MissionGlobalProgramError> {
        self.global_program_admission()?;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                let lights = session_light_snapshot(session, |detail| {
                    MissionGlobalProgramError::SourceUnavailable(detail)
                })?;
                let scene = session.scene.as_mut().ok_or_else(|| {
                    MissionGlobalProgramError::SourceUnavailable("session Scene unavailable".into())
                })?;
                let mut effects = SessionProgramEffects {
                    game: &mut session.game_inline,
                    lights,
                };
                session
                    .loader
                    .as_mut()
                    .unwrap()
                    .content
                    .process_program_in_world(
                        index,
                        delta,
                        budget,
                        &mut MissionProgramWorldContext::new(
                            &mut session.arena,
                            scene,
                            &mut effects,
                            diagnostic,
                            locales,
                        ),
                    )
            },
        )
    }
}
