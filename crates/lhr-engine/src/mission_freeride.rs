//! Fresh-session composition of Main.cpp's original city Freeride case.
//! The setup prefix uses the session's actual actors and model arena. Missing
//! dependencies stop execution; this module supplies neither a sandbox nor a
//! game loop and receives no original-function or runtime-parity credit.
#![forbid(unsafe_code)]

use super::*;
use crate::actor_world::ActorCreationError;
use crate::game_init_order::{InitAttempt, InitRunError, SessionInitError};
use crate::mission_content_loading::SetupActorKind;
use crate::mission_open_order::{OpenAttempt, OpenResult};
use crate::model_loading::ModelKey;
use crate::script_scheduler::ActorReferenceId;

pub const FREERIDE_MISSION: &str = "freeride";

/// Setup's native stack buffer: SUB ESP,0x124 plus four saved-register
/// pushes, with the name at ESP+0x34, leaves 0x100 bytes before saved state.
const FREERIDE_NAME_BUFFER_BYTES: usize = 0x100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreerideScope {
    FreshSession,
}

/// Descriptive phases of Main.cpp:418-439, not reconstructed functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreerideStep {
    ClosePrevious,
    Open,
    LoadCollision,
    ResetMissionNumbers,
    SaveGameNewGame,
    FreerideSetup,
    GameInit,
    FreerideSetupAfter,
    GameLoopSingle,
    Close,
}

impl FreerideStep {
    pub const ORDER: [Self; 10] = [
        Self::ClosePrevious,
        Self::Open,
        Self::LoadCollision,
        Self::ResetMissionNumbers,
        Self::SaveGameNewGame,
        Self::FreerideSetup,
        Self::GameInit,
        Self::FreerideSetupAfter,
        Self::GameLoopSingle,
        Self::Close,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreerideResult {
    /// All supplied operations returned, including the final Close. The
    /// current session adapter cannot reach this result.
    Returned,
    OpenFailed(OpenResult),
}

#[derive(Debug, PartialEq, Eq)]
pub struct FreerideFailure<E> {
    pub step: FreerideStep,
    pub error: E,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FreerideRunError<E> {
    AlreadyEntered,
    Callback(FreerideFailure<E>),
}

pub trait FreerideOperations {
    type Error;
    fn execute(&mut self, step: FreerideStep) -> Result<(), Self::Error>;
    fn open(&mut self, attempt: &mut OpenAttempt) -> Result<OpenResult, Self::Error>;
    fn game_init(&mut self, attempt: &mut InitAttempt) -> Result<(), Self::Error>;
}

/// One attempted start. A different attempt cannot replay the session's
/// source Open or setup, even after an interrupted operation.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct FreerideAttempt {
    entered: bool,
    reached: Vec<FreerideStep>,
    open: OpenAttempt,
    open_result: Option<OpenResult>,
    init: InitAttempt,
}

impl FreerideAttempt {
    pub const SCOPE: FreerideScope = FreerideScope::FreshSession;

    pub fn scope(&self) -> FreerideScope {
        Self::SCOPE
    }

    pub fn entered(&self) -> bool {
        self.entered
    }
    pub fn reached(&self) -> &[FreerideStep] {
        &self.reached
    }
    pub fn last(&self) -> Option<FreerideStep> {
        self.reached.last().copied()
    }
    pub fn open(&self) -> &OpenAttempt {
        &self.open
    }
    pub fn open_result(&self) -> Option<OpenResult> {
        self.open_result
    }
    pub fn init(&self) -> &InitAttempt {
        &self.init
    }

    pub fn run<O: FreerideOperations>(
        &mut self,
        operations: &mut O,
    ) -> Result<FreerideResult, FreerideRunError<O::Error>> {
        if self.entered {
            return Err(FreerideRunError::AlreadyEntered);
        }
        self.entered = true;
        for step in FreerideStep::ORDER {
            self.reached.push(step);
            let result = match step {
                FreerideStep::Open => operations.open(&mut self.open).map(|result| {
                    self.open_result = Some(result);
                }),
                FreerideStep::GameInit => operations.game_init(&mut self.init),
                other => operations.execute(other),
            };
            result.map_err(|error| FreerideRunError::Callback(FreerideFailure { step, error }))?;
            if step == FreerideStep::Open {
                if let Some(result) = self
                    .open_result
                    .filter(|result| *result != OpenResult::Opened)
                {
                    return Ok(FreerideResult::OpenFailed(result));
                }
            }
        }
        Ok(FreerideResult::Returned)
    }
}

/// Explicit host projection/directories; never a replacement mission or car.
pub struct FreerideRequest<'a> {
    pub model_directories: &'a [&'a [u8]],
    pub projection: GameCameraProjection,
}

#[derive(Debug)]
pub enum FreerideError {
    Step(String),
    Open(MissionSourceOpenError),
    Setup(FreerideSetupError),
    Init(InitRunError<SessionInitError>),
}

/// Descriptive reached operations in FreerideSetup (GAME RVA 0x20e9d0).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FreerideSetupStep {
    Activate,
    CreatePlayer,
    CreateCar,
    CreatePlayerModel,
    NamePlayerModel,
    CreateCarModel,
    NameCarModel,
    ResolveCarModel,
    OpenCarModel,
    LocateStartFrame,
}

#[derive(Clone, Copy, Debug)]
struct ModelCreator {
    model: ModelKey,
    reference: FrameReferenceId,
    held: bool,
}

/// Lifetime/progress evidence, not another actor, configuration or scene owner.
#[derive(Debug, Default)]
pub struct FreerideSetupProgress {
    entered: bool,
    reached: Vec<FreerideSetupStep>,
    player: Option<ActorReferenceId>,
    car: Option<ActorReferenceId>,
    player_model: Option<ModelCreator>,
    car_model: Option<ModelCreator>,
    resolved_car_model: Option<Vec<u8>>,
    car_cache_result: Option<i32>,
    start_marker: Option<Vec<u8>>,
}

impl FreerideSetupProgress {
    pub fn entered(&self) -> bool {
        self.entered
    }
    pub fn reached(&self) -> &[FreerideSetupStep] {
        &self.reached
    }
    pub fn player(&self) -> Option<ActorReferenceId> {
        self.player
    }
    pub fn car(&self) -> Option<ActorReferenceId> {
        self.car
    }
    pub fn player_model(&self) -> Option<ModelKey> {
        self.player_model.map(|creator| creator.model)
    }
    pub fn car_model(&self) -> Option<ModelKey> {
        self.car_model.map(|creator| creator.model)
    }
    pub fn held_model_creators(&self) -> usize {
        [self.player_model, self.car_model]
            .iter()
            .flatten()
            .filter(|creator| creator.held)
            .count()
    }
    pub fn resolved_car_model(&self) -> Option<&[u8]> {
        self.resolved_car_model.as_deref()
    }
    pub fn car_cache_result(&self) -> Option<i32> {
        self.car_cache_result
    }
    pub fn start_marker(&self) -> Option<&[u8]> {
        self.start_marker.as_deref()
    }
}

#[derive(Debug)]
pub enum FreerideSetupError {
    Admission(String),
    AlreadyEntered,
    Actor {
        step: FreerideSetupStep,
        error: ActorCreationError,
    },
    NullActor(FreerideSetupStep),
    Model {
        step: FreerideSetupStep,
        error: String,
    },
    /// Startup intake was not completed, including a retained host failure.
    CarDatabaseUnavailable,
    /// Neither a reconstructed menu nor explicit diagnostic input supplied a choice.
    SelectionUnavailable,
    /// The native caller ignores GetCarModel's AL at 0x60ea4c..0x60ea51.
    /// A false return leaves its buffer unwritten; host execution stops rather
    /// than passing invented or uninitialized filename bytes to cache Open.
    CarModelNotWritten,
    /// Source/destination access would leave the defined native buffer domain.
    CarModelLookup(crate::car_database::CarDatabaseError),
    /// A host cache failure, not a native HRESULT; reached owners are retained.
    CarModelCache(String),
    /// The next native call needs Scene's virtual masked FindChildFrame walk.
    SceneNamedLookupUnavailable,
}

impl MissionLoadSession {
    /// Host startup intake before source Open, using the same loose/DTA source
    /// owner as the mission. This does not compose AllDefault or the native
    /// repeated Init calls, allocation failures, or dtaRead short reads.
    /// Native application state is session-owned here until an application
    /// owner exists; it survives terminal mission disposal, not a new session.
    pub fn read_car_database_for_startup(&mut self) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.entered || self.freeride_setup.entered {
            return Err("car database intake requires a fresh session".into());
        }
        self.car_database
            .read_index_tables(|path| {
                self.installation
                    .read_optional(&path.replace('\\', "/"))
                    .map_err(|error| error.to_string())
            })
            .map_err(|error| format!("{error:?}"))
    }

    pub fn car_database(&self) -> &crate::car_database::CarDatabase {
        &self.car_database
    }

    /// Explicit diagnostic input to the real StoreA/StoreB state, not a
    /// reconstructed GM_FreeRide menu selection or a profile-file decode.
    /// No model filename, active-byte replacement or implicit choice is accepted.
    /// StoreA/StoreB update the actual owner before Open; Setup reads it lazily.
    pub fn adopt_freeride_selection_host(
        &mut self,
        car: u32,
        flags: u32,
        location: u32,
        traffic: f32,
        pedestrians: f32,
        police: f32,
    ) -> Result<(), String> {
        self.open_terminal_guard()?;
        if self.entered || self.freeride_setup.entered || self.freeride_selection_adopted {
            return Err("Freeride selection requires fresh, unconfigured session state".into());
        }
        self.freeride
            .store_a(car, flags, traffic, pedestrians, police);
        self.freeride.store_b(location);
        self.freeride_selection_adopted = true;
        Ok(())
    }

    pub fn freeride_setup_progress(&self) -> &FreerideSetupProgress {
        &self.freeride_setup
    }

    /// GAME SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
    /// FreerideSetup RVA 0x20e9d0, REA evidence
    /// ev_c7f08b476d7733f470d1388358a77f9c60cbbdb4f72dea4c2c4cfc1fc0f43bd8.
    /// Only the prefix through car cache Open is composed. Constructor seeds remain
    /// the content owner's disclosed host policy, not original startup state.
    pub fn setup_freeride(&mut self) -> Result<(), FreerideSetupError> {
        self.open_terminal_guard()
            .map_err(FreerideSetupError::Admission)?;
        if self.freeride_setup.entered {
            return Err(FreerideSetupError::AlreadyEntered);
        }
        if self.phase != MissionLoadPhase::PrefixComplete
            || !self.open_mission_opened
            || self.open_mission_name.as_deref() != Some(FREERIDE_MISSION.as_bytes())
            || self.failure.is_some()
            || self.game_init_reset_entered
            || self.loader.is_none()
        {
            return Err(FreerideSetupError::Admission(
                "requires the complete original freeride source Open, before GameInit".into(),
            ));
        }
        self.freeride_setup.entered = true;
        Pc24PrecisionState::run_owner(
            self,
            |session| &mut session.precision,
            Self::setup_freeride_inner,
        )
    }

    fn setup_freeride_inner(&mut self) -> Result<(), FreerideSetupError> {
        self.freeride_setup
            .reached
            .push(FreerideSetupStep::Activate);
        self.freeride.active = true;
        for (step, kind) in [
            (FreerideSetupStep::CreatePlayer, SetupActorKind::Player),
            (FreerideSetupStep::CreateCar, SetupActorKind::Car),
        ] {
            self.freeride_setup.reached.push(step);
            let result = self
                .loader
                .as_mut()
                .unwrap()
                .content
                .create_setup_actor(kind);
            let identity = match &result {
                Ok(identity) => *identity,
                Err(error) => error.actor,
            };
            match kind {
                SetupActorKind::Player => self.freeride_setup.player = identity,
                SetupActorKind::Car => self.freeride_setup.car = identity,
            }
            result
                .map_err(|error| FreerideSetupError::Actor { step, error })?
                .ok_or(FreerideSetupError::NullActor(step))?;
        }
        for (create, rename, name, player) in [
            (
                FreerideSetupStep::CreatePlayerModel,
                FreerideSetupStep::NamePlayerModel,
                b"Tommy".as_slice(),
                true,
            ),
            (
                FreerideSetupStep::CreateCarModel,
                FreerideSetupStep::NameCarModel,
                b"TomyhoAuto".as_slice(),
                false,
            ),
        ] {
            self.freeride_setup.reached.push(create);
            let scene = self
                .scene()
                .map_err(|error| FreerideSetupError::Model {
                    step: create,
                    error: format!("{error:?}"),
                })?
                .ok_or_else(|| FreerideSetupError::Model {
                    step: create,
                    error: "Scene unavailable".into(),
                })?;
            let model = self
                .driver
                .create_model_frame(&mut self.arena, &self.resources, scene)
                .map_err(|error| FreerideSetupError::Model {
                    step: create,
                    error: format!("{error:?}"),
                })?;
            let creator = ModelCreator {
                model,
                reference: self.arena.lookup_reference(model.root()),
                held: true,
            };
            if player {
                self.freeride_setup.player_model = Some(creator);
            } else {
                self.freeride_setup.car_model = Some(creator);
            }
            self.freeride_setup.reached.push(rename);
            self.arena
                .rename_original_model(model, Some(name))
                .map_err(|error| FreerideSetupError::Model {
                    step: rename,
                    error: format!("{error:?}"),
                })?;
        }
        self.freeride_setup
            .reached
            .push(FreerideSetupStep::ResolveCarModel);
        if self.car_database.intake() != crate::car_database::IndexIntake::Complete {
            return Err(FreerideSetupError::CarDatabaseUnavailable);
        }
        if !self.freeride_selection_adopted {
            return Err(FreerideSetupError::SelectionUnavailable);
        }
        let mut name = [0u8; FREERIDE_NAME_BUFFER_BYTES];
        if !self
            .car_database
            .get_car_model(self.freeride.car, &mut name, false)
            .map_err(FreerideSetupError::CarModelLookup)?
        {
            return Err(FreerideSetupError::CarModelNotWritten);
        }
        let length = name.iter().position(|byte| *byte == 0).unwrap();
        self.freeride_setup.resolved_car_model = Some(name[..length].to_vec());
        self.freeride_setup
            .reached
            .push(FreerideSetupStep::OpenCarModel);
        let result = self
            .caches
            .open_static_model(
                &mut self.arena,
                &self.installation,
                self.freeride_setup.car_model.unwrap().model,
                &name[..length],
                &mut self.cache_memory,
            )
            .map_err(|error| FreerideSetupError::CarModelCache(format!("{error:?}")))?;
        self.freeride_setup.car_cache_result = Some(result);
        self.freeride_setup.start_marker =
            Some(format!("emeth_{}", self.freeride.location as i32).into_bytes());
        self.freeride_setup
            .reached
            .push(FreerideSetupStep::LocateStartFrame);
        Err(FreerideSetupError::SceneNamedLookupUnavailable)
    }

    pub(super) fn release_freeride_model_creators(&mut self) -> Result<(), ModelError> {
        for slot in [
            &mut self.freeride_setup.car_model,
            &mut self.freeride_setup.player_model,
        ] {
            if let Some(creator) = slot.as_mut().filter(|creator| creator.held) {
                if self.arena.lookup_reference(creator.model.root()) != creator.reference {
                    return Err(ModelError::UnknownFrame(creator.model.root()));
                }
                self.arena.release(creator.model.root())?;
                creator.held = false;
            }
        }
        Ok(())
    }

    /// Original city Freeride only, over fresh session owners. Setup precedes
    /// Init; unavailable setup never falls through to campaign or car runtime.
    pub fn start_freeride<S, R>(
        &mut self,
        attempt: &mut FreerideAttempt,
        request: FreerideRequest<'_>,
        screen: &mut S,
        renderer: &mut R,
    ) -> Result<FreerideResult, FreerideRunError<FreerideError>>
    where
        S: DriverSceneScreen,
        S::Error: std::fmt::Debug,
        R: crate::scene_projection::SceneRenderBackend
            + crate::load_scene_callback::LoadingScreenDevice,
    {
        attempt.run(&mut SessionFreeride {
            session: self,
            request,
            screen,
            renderer,
        })
    }
}

struct SessionFreeride<'a, S, R> {
    session: &'a mut MissionLoadSession,
    request: FreerideRequest<'a>,
    screen: &'a mut S,
    renderer: &'a mut R,
}

impl<S, R> FreerideOperations for SessionFreeride<'_, S, R>
where
    S: DriverSceneScreen,
    S::Error: std::fmt::Debug,
    R: crate::scene_projection::SceneRenderBackend
        + crate::load_scene_callback::LoadingScreenDevice,
{
    type Error = FreerideError;
    fn execute(&mut self, step: FreerideStep) -> Result<(), Self::Error> {
        let result = match step {
            FreerideStep::ClosePrevious => self.session.close_before_new_mission(),
            FreerideStep::LoadCollision => self
                .session
                .load_collision_for_new_mission(FREERIDE_MISSION),
            FreerideStep::ResetMissionNumbers => {
                self.session.reset_mission_numbers_for_new_mission()
            }
            FreerideStep::SaveGameNewGame => self.session.save_game_new_game_for_new_mission(),
            FreerideStep::FreerideSetup => {
                return self.session.setup_freeride().map_err(FreerideError::Setup)
            }
            FreerideStep::FreerideSetupAfter
            | FreerideStep::GameLoopSingle
            | FreerideStep::Close => Err("native operation is not composed".into()),
            FreerideStep::Open | FreerideStep::GameInit => {
                Err("dispatched through its own entry".into())
            }
        };
        result.map_err(|detail| FreerideError::Step(format!("{step:?}: {detail}")))
    }
    fn open(&mut self, attempt: &mut OpenAttempt) -> Result<OpenResult, Self::Error> {
        self.session
            .open_source_in_order(
                attempt,
                FREERIDE_MISSION,
                true,
                self.request.model_directories,
                self.request.projection,
                self.screen,
                self.renderer,
            )
            .map_err(FreerideError::Open)
    }
    fn game_init(&mut self, attempt: &mut InitAttempt) -> Result<(), Self::Error> {
        attempt.run(self.session).map_err(FreerideError::Init)
    }
}
