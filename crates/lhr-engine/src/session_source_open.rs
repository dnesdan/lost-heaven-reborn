//! Ordered source intake through the session's existing owners.
//!
//! OpenWorld/OpenBin reuse the CPU adapters; this is not complete native
//! C_mission::Open. Game reset, vehicle binding, progress UI and media remain
//! the disclosed host markers in mission_open_order. No parity credit.
#![forbid(unsafe_code)]

use super::*;
use crate::mission_open_order::{
    OpenAttempt, OpenOperations, OpenResult, OpenRunError, OpenStep, SessionOpenError,
};
use crate::scene_projection::SceneRenderBackend;

#[derive(Debug, PartialEq, Eq)]
pub enum MissionSourceOpenError {
    Admission(MissionLoadCallError),
    MissionAlreadyOpened,
    /// Earlier public Open helpers consumed an owner or retained effects.
    SessionNotFresh,
    InvalidMissionName(String),
    /// The retained failure() identifies the bootstrap phase and cause.
    BootstrapStopped,
    Open(OpenRunError<SessionOpenError>),
}

impl MissionLoadSession {
    /// Load actual world and scene2 owners in the represented Open order.
    /// The caller supplies the render boundary and an unused session; a prior
    /// load, interrupted attempt or active mission cannot be replayed here.
    /// Startup/Scene construction precedes this fresh-session Open sequence.
    ///
    /// The name supplies missions/<name>/scene.i3d and scene2.bin together.
    /// Model directories and projection remain explicit caller inputs. The
    /// configured volume is read from its existing session field when reached.
    /// A source prefix is admitted to scripts/Init only after the entire Open
    /// sequence returns Opened, including its final cache read. That outcome
    /// is source loading, not native GameInit, active Close or playable startup.
    ///
    /// This is the only source-loading entry: the probe and every composed
    /// new-mission caller go through it. The caller owns `attempt`, so the
    /// reached boundary sequence stays observable after any outcome.
    pub fn open_source_in_order<S: DriverSceneScreen, R: SceneRenderBackend>(
        &mut self,
        attempt: &mut OpenAttempt,
        mission: &str,
        model_directories: &[&[u8]],
        projection: GameCameraProjection,
        screen: &mut S,
        renderer: &mut R,
    ) -> Result<OpenResult, MissionSourceOpenError>
    where
        S::Error: std::fmt::Debug,
    {
        if self.discard.phase != MissionDiscardPhase::NotStarted {
            return Err(MissionSourceOpenError::Admission(
                MissionLoadCallError::TerminalDiscardEntered,
            ));
        }
        if self.entered || self.phase != MissionLoadPhase::NotStarted {
            return Err(MissionSourceOpenError::Admission(
                MissionLoadCallError::AlreadyEntered,
            ));
        }
        if self.open_mission_opened || self.game_initialized {
            return Err(MissionSourceOpenError::MissionAlreadyOpened);
        }
        // A replayed attempt guard is refused before bootstrap effects.
        if attempt.entered() {
            return Err(MissionSourceOpenError::Open(OpenRunError::AlreadyEntered));
        }
        // The standalone Open helpers do not enter this path. Their one-shot owners
        // and reached effects must not be erased by bootstrap/ClosePrevious.
        // Configuration-only mission name and volume remain caller inputs.
        if !matches!(
            self.navigation.outcome(),
            crate::mission_session_navigation::SessionNavigationOutcome::NotAttempted
        ) || !matches!(
            self.roads.outcome(),
            crate::mission_session_roads::SessionRoadsOutcome::NotAttempted
        ) || self.base_cache.has_opened_or_retained_data()
            || self.open_cache_progress != 0
            || self.open_restore_volume
            || self.open_muted
            || self.open_restored.is_some()
            || self.game_init_reset_entered
        {
            return Err(MissionSourceOpenError::SessionNotFresh);
        }
        Self::validate_open_mission_name(mission)
            .map_err(MissionSourceOpenError::InvalidMissionName)?;
        // Callbacks and unwind cannot admit a second source owner.
        self.entered = true;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            |session| {
                if let Err(cause) = session.bootstrap_source(model_directories, screen) {
                    session.retain_source_open_failure(cause);
                    return Err(MissionSourceOpenError::BootstrapStopped);
                }
                let result = attempt.run(&mut SourceOpen {
                    session,
                    mission,
                    projection,
                    screen,
                    renderer,
                });
                match result {
                    Ok(OpenResult::Opened) => {
                        session.phase = MissionLoadPhase::PrefixComplete;
                        Ok(OpenResult::Opened)
                    }
                    Ok(outcome) => Ok(outcome),
                    Err(error) => {
                        if let OpenRunError::Callback(failure) = &error {
                            session.retain_source_open_failure(MissionLoadCause::OpenStep {
                                step: failure.step,
                                detail: failure.error.0.clone(),
                            });
                        }
                        Err(MissionSourceOpenError::Open(error))
                    }
                }
            },
        )
    }

    fn retain_source_open_failure(&mut self, cause: MissionLoadCause) {
        self.failure.get_or_insert(MissionLoadFailure {
            phase: self.phase,
            cause,
        });
    }
}

struct SourceOpen<'a, S, R> {
    session: &'a mut MissionLoadSession,
    mission: &'a str,
    projection: GameCameraProjection,
    screen: &'a mut S,
    renderer: &'a mut R,
}

impl<S, R> SourceOpen<'_, S, R> {
    fn source_error(&mut self, step: OpenStep, cause: MissionLoadCause) -> SessionOpenError {
        let detail = format!("{step:?}: {cause:?}");
        self.session.retain_source_open_failure(cause);
        SessionOpenError(detail)
    }

    fn scene(&self, step: OpenStep) -> Result<SceneKey, SessionOpenError> {
        self.session
            .scene()
            .map_err(|error| SessionOpenError(format!("{step:?}: live Scene: {error:?}")))?
            .ok_or_else(|| SessionOpenError(format!("{step:?}: Scene unavailable")))
    }
}

impl<S: DriverSceneScreen, R: SceneRenderBackend> OpenOperations for SourceOpen<'_, S, R>
where
    S::Error: std::fmt::Debug,
{
    type Error = SessionOpenError;

    fn execute(&mut self, step: OpenStep) -> Result<(), Self::Error> {
        self.session.phase = MissionLoadPhase::OpenStep(step);
        match step {
            OpenStep::CopyMissionName => self
                .session
                .set_open_mission_name(self.mission)
                .map_err(|detail| SessionOpenError(format!("{step:?}: {detail}"))),
            OpenStep::OpenWorld => {
                let scene = self.scene(step)?;
                let input = MissionLoadSession::open_world_path(self.mission);
                self.session
                    .load_world_source(scene, &input)
                    .map_err(|cause| self.source_error(step, cause))
            }
            OpenStep::ResetWeather => {
                let scene = self.scene(step)?;
                self.session
                    .prepare_source_weather(scene)
                    .map_err(|cause| self.source_error(step, cause))
            }
            OpenStep::FillSectorBits => {
                let scene = self.scene(step)?;
                self.session
                    .fill_source_sector_groups(scene)
                    .map_err(|cause| self.source_error(step, cause))
            }
            OpenStep::LoadNavigation => {
                self.session.phase = MissionLoadPhase::Navigation;
                let path = MissionLoadSession::open_check_path(self.mission);
                self.session
                    .navigation
                    .load(&self.session.installation, &path)
                    .map(|_| ())
                    .map_err(|cause| self.source_error(step, MissionLoadCause::Navigation(cause)))
            }
            OpenStep::LoadRoads => {
                self.session.phase = MissionLoadPhase::Roads;
                let path = MissionLoadSession::open_road_path(self.mission);
                self.session
                    .roads
                    .load(&self.session.installation, &path)
                    .map(|_| ())
                    .map_err(|cause| self.source_error(step, MissionLoadCause::Roads(cause)))
            }
            OpenStep::MuteAudioAndRender => {
                self.session
                    .mute_audio_and_render_for_open()
                    .map_err(|detail| SessionOpenError(format!("{step:?}: {detail}")))?;
                let scene = self.scene(step)?;
                // The original ignores Render's return word. A host error
                // still stops before restore_volume and before OpenBin reads.
                let _status = self
                    .session
                    .arena
                    .scene_render_with_backend(scene, self.renderer)
                    .map_err(|error| SessionOpenError(format!("{step:?}: Render: {error:?}")))?;
                Ok(())
            }
            _ => self.session.execute(step),
        }
    }

    fn media_ready(&mut self) -> Result<bool, Self::Error> {
        self.session.phase = MissionLoadPhase::OpenStep(OpenStep::WaitForMedia);
        // Same explicit installation-only host policy as the existing driver;
        // this is not a reconstructed CD/media owner.
        self.session.media_ready()
    }

    fn open_bin(&mut self) -> Result<bool, Self::Error> {
        let step = OpenStep::OpenBin;
        self.session.phase = MissionLoadPhase::Scene2Input;
        let path = MissionLoadSession::open_scene2_path(self.mission);
        // C_mission::OpenBin (mafia-re 113b5b6, GAME 0x541030)
        // opens the file before map/camera setup. Read once, after Render:
        // absence has no map/camera effects; corruption is decoded later.
        self.session.scene2_bytes =
            self.session
                .installation
                .read_optional(&path)
                .map_err(|error| {
                    self.source_error(
                        step,
                        MissionLoadCause::Scene2Input {
                            path: path.clone(),
                            detail: error.to_string(),
                        },
                    )
                })?;
        if self.session.scene2_bytes.is_none() {
            return Ok(false);
        }
        let scene = self.scene(step)?;
        self.session
            .initialize_source_map(scene, &path)
            .map_err(|cause| self.source_error(step, cause))?;
        match self
            .session
            .load_source_scene2_prefix(self.projection, self.screen)
        {
            Ok(()) => Ok(true),
            // A failed RAscend is OPENBIN_CORRUPTED: the map and camera stay,
            // and Open still restores audio and advances progress before it
            // returns failure. It is retained as data, not a host failure.
            Err(MissionLoadCause::Scene2Prefix(error)) if error.native_open_bin_corrupted => {
                self.session.open_bin_corruption = Some(error);
                Ok(false)
            }
            Err(cause) => Err(self.source_error(step, cause)),
        }
    }

    fn audio_present(&self) -> bool {
        self.session.audio_present_for_open()
    }
}
