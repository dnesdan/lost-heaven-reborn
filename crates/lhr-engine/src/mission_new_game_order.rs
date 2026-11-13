//! Ordered mission start on a fresh session, not a substitute for the owners.
//!
//! Scope: the intro leg (game/Main.cpp:373-384: Close, Open("intro"),
//! LoadCollision, ResetMissionNumbers, Init, GameLoopSingle) is NOT composed.
//! Natively the Close at Main.cpp:385 therefore runs with the intro mission
//! opened and is a full `C_mission::Close` (Mission.cpp:510-545). Here it runs
//! on a fresh session, where the native `!opened` early return applies. That
//! precondition is recorded as [`NewMissionScope`] on every attempt, so a
//! returned Init is never reported as the complete native new-game path.
//!
//! Reference: mafia-re `113b5b6`, game/Main.cpp:385-396, the
//! `MAINMENU_NEW_GAME` continuation after the intro:
//!
//! ```text
//! act_mission->Close();
//! if (act_mission->Open("mise01", false, (uint)-1, true)) { CloseSystem(); goto end; }
//! gifCollision.LoadCollision("missions\\mise01\\tree.klz", true);
//! act_mission->ResetMissionNumbers();
//! mafiaLoadSave.SaveGameNewGame();
//! act_mission->game->Init();
//! ```
//!
//! `C_mission::Open` is GAME 0x5409d0, `g_collision::LoadCollision` GAME
//! 0x5c2b70, `G_LoadSaveClass::SaveGameNewGame` GAME 0x605ff0 and
//! `C_game::Init` GAME 0x5a0810; `ResetMissionNumbers` is inline
//! (Mission.h:290-294). `GameLoopSingle` and the trailing `Close` are outside
//! this composition. The composition is descriptive and earns no function
//! credit; every reached step is required and an unavailable owner returns
//! `Err` at its own step instead of being skipped.
#![forbid(unsafe_code)]

use crate::game_camera_projection::GameCameraProjection;
use crate::game_init_order::{InitAttempt, InitRunError, SessionInitError};
use crate::mission_load_session::{MissionLoadSession, MissionSourceOpenError};
use crate::mission_open_order::{OpenAttempt, OpenResult};

/// What this composition covers of the native `MAINMENU_NEW_GAME` case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewMissionScope {
    /// Main.cpp:385-396 on a fresh session. The intro leg (Main.cpp:373-384)
    /// was not composed, so the native full Close of the open intro mission
    /// is replaced by the not-opened early return.
    FreshSessionIntroNotComposed,
}

/// Descriptive call boundaries of the Main.cpp new-game block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewMissionStep {
    /// Outer `act_mission->Close()`. Natively the intro mission is open here
    /// (full Close); on the fresh session this composition requires, only
    /// the `!opened` early return is reachable.
    ClosePrevious,
    /// `act_mission->Open(name, false, -1, true)`; a nonzero result leaves
    /// through `CloseSystem` without any later step.
    Open,
    /// `gifCollision.LoadCollision("missions\\<name>\\tree.klz", true)`.
    LoadCollision,
    /// Inline `ResetMissionNumbers`: pending and current numbers to zero.
    ResetMissionNumbers,
    /// `mafiaLoadSave.SaveGameNewGame()`: last_stolen, steal_cars,
    /// GarageClear and freeride.active, before Init reads that state.
    SaveGameNewGame,
    /// `act_mission->game->Init()` through the ordered `InitAttempt`.
    GameInit,
}

impl NewMissionStep {
    /// Native order when Open returns `Opened` and every step returns.
    pub const ORDER: &'static [NewMissionStep] = &[
        NewMissionStep::ClosePrevious,
        NewMissionStep::Open,
        NewMissionStep::LoadCollision,
        NewMissionStep::ResetMissionNumbers,
        NewMissionStep::SaveGameNewGame,
        NewMissionStep::GameInit,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NewMissionResult {
    /// `game->Init()` returned within [`NewMissionScope`]: the intro leg was
    /// not composed and `GameLoopSingle` was not entered. This is not the
    /// complete native new-game path and not playable startup.
    InitReturned,
    /// Open returned nonzero; native leaves through `CloseSystem`.
    OpenFailed(OpenResult),
}

#[derive(Debug, PartialEq, Eq)]
pub struct NewMissionFailure<E> {
    pub step: NewMissionStep,
    pub error: E,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NewMissionRunError<E> {
    AlreadyEntered,
    Callback(NewMissionFailure<E>),
}

/// Operations over the caller's actual owners. The nested Open and Init
/// attempts belong to the driver so their reached boundaries stay
/// observable after any outcome.
pub trait NewMissionOperations {
    type Error;

    /// `ClosePrevious`, `LoadCollision`, `ResetMissionNumbers` and
    /// `SaveGameNewGame`; `Open` and `GameInit` use their own entries.
    fn execute(&mut self, step: NewMissionStep) -> Result<(), Self::Error>;
    fn open(&mut self, attempt: &mut OpenAttempt) -> Result<OpenResult, Self::Error>;
    fn game_init(&mut self, attempt: &mut InitAttempt) -> Result<(), Self::Error>;
}

/// Single-use progress for one composed start, including the nested Open
/// and Init attempts. Deliberately not Clone/Copy.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct NewMissionAttempt {
    entered: bool,
    reached: Vec<NewMissionStep>,
    open: OpenAttempt,
    open_result: Option<OpenResult>,
    init: InitAttempt,
}

impl NewMissionAttempt {
    /// The recorded composition precondition; constant for every attempt.
    pub const SCOPE: NewMissionScope = NewMissionScope::FreshSessionIntroNotComposed;

    pub fn scope(&self) -> NewMissionScope {
        Self::SCOPE
    }

    pub fn entered(&self) -> bool {
        self.entered
    }

    pub fn last(&self) -> Option<NewMissionStep> {
        self.reached.last().copied()
    }

    /// Entered steps in invocation order.
    pub fn reached(&self) -> &[NewMissionStep] {
        &self.reached
    }

    /// The nested `C_mission::Open` attempt.
    pub fn open(&self) -> &OpenAttempt {
        &self.open
    }

    /// The native Open return, when Open returned without a host error.
    pub fn open_result(&self) -> Option<OpenResult> {
        self.open_result
    }

    /// The nested `C_game::Init` attempt; unentered unless reached.
    pub fn init(&self) -> &InitAttempt {
        &self.init
    }

    pub fn run<O: NewMissionOperations>(
        &mut self,
        ops: &mut O,
    ) -> Result<NewMissionResult, NewMissionRunError<O::Error>> {
        if self.entered {
            return Err(NewMissionRunError::AlreadyEntered);
        }
        self.entered = true;
        let fail =
            |step| move |error| NewMissionRunError::Callback(NewMissionFailure { step, error });
        self.reached.push(NewMissionStep::ClosePrevious);
        ops.execute(NewMissionStep::ClosePrevious)
            .map_err(fail(NewMissionStep::ClosePrevious))?;
        self.reached.push(NewMissionStep::Open);
        let opened = ops
            .open(&mut self.open)
            .map_err(fail(NewMissionStep::Open))?;
        self.open_result = Some(opened);
        if opened != OpenResult::Opened {
            return Ok(NewMissionResult::OpenFailed(opened));
        }
        for step in [
            NewMissionStep::LoadCollision,
            NewMissionStep::ResetMissionNumbers,
            NewMissionStep::SaveGameNewGame,
        ] {
            self.reached.push(step);
            ops.execute(step).map_err(fail(step))?;
        }
        self.reached.push(NewMissionStep::GameInit);
        ops.game_init(&mut self.init)
            .map_err(fail(NewMissionStep::GameInit))?;
        Ok(NewMissionResult::InitReturned)
    }
}

/// Explicit host inputs for the session-backed start. Projection and model
/// directories are caller choices, not recovered GameInit globals.
pub struct NewMissionRequest<'a> {
    pub mission: &'a str,
    pub model_directories: &'a [&'a [u8]],
    pub projection: GameCameraProjection,
}

/// First failure of the session-backed start, by owner.
#[derive(Debug, PartialEq, Eq)]
pub enum NewMissionError {
    /// A host-side step boundary; the text carries the step prefix.
    Step(String),
    Open(MissionSourceOpenError),
    Init(InitRunError<SessionInitError>),
}

impl MissionLoadSession {
    /// Compose the Main.cpp new-game mission start over this session's live
    /// owners: the single ordered source Open, then the post-Open steps. The
    /// first unavailable owner stops the composition with its step; nothing
    /// later runs, and Init is entered only after SaveGameNewGame returned.
    pub fn start_new_game_mission<S, R>(
        &mut self,
        attempt: &mut NewMissionAttempt,
        request: NewMissionRequest<'_>,
        screen: &mut S,
        renderer: &mut R,
    ) -> Result<NewMissionResult, NewMissionRunError<NewMissionError>>
    where
        S: crate::driver_scene::DriverSceneScreen,
        S::Error: std::fmt::Debug,
        R: crate::scene_projection::SceneRenderBackend,
    {
        attempt.run(&mut SessionNewMission {
            session: self,
            request,
            screen,
            renderer,
        })
    }
}

struct SessionNewMission<'a, S, R> {
    session: &'a mut MissionLoadSession,
    request: NewMissionRequest<'a>,
    screen: &'a mut S,
    renderer: &'a mut R,
}

impl<S, R> NewMissionOperations for SessionNewMission<'_, S, R>
where
    S: crate::driver_scene::DriverSceneScreen,
    S::Error: std::fmt::Debug,
    R: crate::scene_projection::SceneRenderBackend,
{
    type Error = NewMissionError;

    fn execute(&mut self, step: NewMissionStep) -> Result<(), Self::Error> {
        let result = match step {
            NewMissionStep::ClosePrevious => self.session.close_before_new_mission(),
            NewMissionStep::LoadCollision => self
                .session
                .load_collision_for_new_mission(self.request.mission),
            NewMissionStep::ResetMissionNumbers => {
                self.session.reset_mission_numbers_for_new_mission()
            }
            // G_LoadSaveClass (mafiaLoadSave) is not a session owner; its
            // garage/steal/freeride state must not be invented here.
            NewMissionStep::SaveGameNewGame => Err(
                "no live G_LoadSaveClass owner for last_stolen, steal_cars, GarageClear and freeride.active"
                    .into(),
            ),
            NewMissionStep::Open | NewMissionStep::GameInit => {
                Err("dispatched through its own entry".into())
            }
        };
        result.map_err(|detail| NewMissionError::Step(format!("{step:?}: {detail}")))
    }

    fn open(&mut self, attempt: &mut OpenAttempt) -> Result<OpenResult, Self::Error> {
        self.session
            .open_source_in_order(
                attempt,
                self.request.mission,
                self.request.model_directories,
                self.request.projection,
                self.screen,
                self.renderer,
            )
            .map_err(NewMissionError::Open)
    }

    fn game_init(&mut self, attempt: &mut InitAttempt) -> Result<(), Self::Error> {
        attempt.run(self.session).map_err(NewMissionError::Init)
    }
}

#[cfg(test)]
#[path = "mission_new_game_order_tests.rs"]
mod tests;
