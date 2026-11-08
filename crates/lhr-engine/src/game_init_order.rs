//! Ordered `C_game::Init` dependency driver, not a substitute for the owners.
//!
//! Reference: mafia-re `113b5b6`, game/mission/game.cpp, `C_game::Init`
//! (PC 0x5a0810, 0x314c bytes). Every reached operation is required;
//! unavailable dependencies return `Err`. Production wiring over the live
//! session owners follows below; VM, player, camera and HUD intake stays
//! out of scope and never becomes a silent success.
//!
//! Step order follows the native body verbatim, including progress marks and
//! `intern_load`/`anim_preload` gates. Names are descriptive; only PC
//! addresses and mafia-re-attributed PS2 names are cited as evidence.
#![forbid(unsafe_code)]

/// Descriptive phase boundaries in native `Init` order, not separately
/// reconstructed functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitStep {
    /// Refuse when already initialized (native `return false`).
    GuardNotInitialized,
    /// Mission state reset: pass flags, weather read, sound mute, score,
    /// cheats, timers, and cleared pools.
    ResetMissionState,
    /// Police shift begin: `ZacatekSmeny` + groups/follow/fight `Init`.
    PoliceBeginShift,
    /// Player navipoint clear and car-damage latch.
    PlayerNavi,
    /// Vehicle light/sound sector rebuild (`intern_load` only).
    VehicleSectors,
    /// Physics, cheat, time, program-pointer, and pool clears.
    PhysicsClear,
    /// `shvestky.GameInit`.
    PoliceGameInit,
    /// Taxi-driver flags and scream validity.
    TrafficFlags,
    /// Sound arena init, lightness clear, radar clear.
    SoundAndLightness,
    /// `GlobalProgramGameInit` then `Run("GameInitStart")`.
    GlobalProgramStart,
    /// `mafiaIndicators.GameInit` (game.cpp:334).
    IndicatorsGameInit,
    /// Box pools, static particles, `Mission_Start`, web `DeRiciConns`,
    /// force-feedback init.
    EffectsStart,
    /// Difference/crash pools clear.
    DiffClear,
    /// `blood_manager.GameInit` + frame-pool clears.
    BloodAndFramePools,
    /// Driver `FAST_FIND` save/clear, randomize, console registration.
    DriverAndConsoles,
    /// `AIManager.segments.Load`.
    SegmentsLoad,
    /// Car tyre/material/particle tables (`intern_load` only).
    CarTables,
    /// Hydrants, dabing, using-objects, wheel-animation times.
    HydrantsAndWheels,
    /// Shadow-model preload (`intern_load && anim_preload`).
    ShadowModels,
    /// Player select (last non-cutscene `ACTOR_PLAYER`), radar cars,
    /// camera-behind-player, traffic visibility, door resets.
    PlayerSelect,
    /// Per-actor `SetActState(0)` or `GameInit` + `CalmVehicle(20)`,
    /// progress 0.1 to 0.7.
    ActorsGameInit,
    /// Active/inactive/dead list rebuild + `ChangeActState`.
    ActorLists,
    /// InterCity init, traffic timers, fires clear.
    InterCityAndTraffic,
    /// Camera reset/bind/player, anim preload, dog/sledovacka/glass/debris.
    CameraAndChase,
    /// Compass, semaphores, roadblocks, telephones, signals, taxi points,
    /// city-music/scream areas, shadow tick; progress 0.7.
    MissionSystems,
    /// Track/scratch/stone/cartridge/hole/sound/sheet resources
    /// (`intern_load` only); progress 0.75 to 0.86.
    TrackResources,
    /// `rmorf.Mission_Start`, driver-state restore, indicator flags.
    RmorfAndDriverRestore,
    /// Player HUD clamp, `UpdateControls`, racing/garage init; progress 0.9.
    PlayerHud,
    /// Write buffers, `Run("GameInitEnd")`, skip frames, settings update,
    /// progress 1.0, progress teardown, volume restore.
    BuffersAndPrograms,
    /// Base-cache blocks (`base_cache.IsOpen()` only; `PrepareBaseBlocks`
    /// 0x5a3990 is itself a stub upstream).
    BaseBlocks,
    /// `initialized = true`, weather repair, player latch.
    MarkInitialized,
}

impl InitStep {
    /// Native order; used by tests to lock the sequence.
    pub const ORDER: &'static [InitStep] = &[
        InitStep::GuardNotInitialized,
        InitStep::ResetMissionState,
        InitStep::PoliceBeginShift,
        InitStep::PlayerNavi,
        InitStep::VehicleSectors,
        InitStep::PhysicsClear,
        InitStep::PoliceGameInit,
        InitStep::TrafficFlags,
        InitStep::SoundAndLightness,
        InitStep::GlobalProgramStart,
        InitStep::IndicatorsGameInit,
        InitStep::EffectsStart,
        InitStep::DiffClear,
        InitStep::BloodAndFramePools,
        InitStep::DriverAndConsoles,
        InitStep::SegmentsLoad,
        InitStep::CarTables,
        InitStep::HydrantsAndWheels,
        InitStep::ShadowModels,
        InitStep::PlayerSelect,
        InitStep::ActorsGameInit,
        InitStep::ActorLists,
        InitStep::InterCityAndTraffic,
        InitStep::CameraAndChase,
        InitStep::MissionSystems,
        InitStep::TrackResources,
        InitStep::RmorfAndDriverRestore,
        InitStep::PlayerHud,
        InitStep::BuffersAndPrograms,
        InitStep::BaseBlocks,
        InitStep::MarkInitialized,
    ];
}

/// Operations over the caller's actual game owners. There is no fallback
/// implementation for an unavailable dependency.
pub trait InitOperations {
    type Error;

    /// Execute the selected boundary in native order.
    fn execute(&mut self, step: InitStep) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct InitFailure<E> {
    pub step: InitStep,
    pub error: E,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InitRunError<E> {
    AlreadyEntered,
    Callback(InitFailure<E>),
}

/// Single-use progress for one invocation. Copying an unentered guard must
/// not provide two invocations of the same attempt.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct InitAttempt {
    entered: bool,
    last: Option<InitStep>,
}

impl InitAttempt {
    pub fn last(&self) -> Option<InitStep> {
        self.last
    }

    pub fn entered(&self) -> bool {
        self.entered
    }

    pub fn run<O: InitOperations>(&mut self, ops: &mut O) -> Result<(), InitRunError<O::Error>> {
        if self.entered {
            return Err(InitRunError::AlreadyEntered);
        }
        self.entered = true;
        for step in InitStep::ORDER {
            self.last = Some(*step);
            ops.execute(*step)
                .map_err(|error| InitRunError::Callback(InitFailure { step: *step, error }))?;
        }
        Ok(())
    }
}

/// Production `Init` failure over the live session owners. The display
/// always carries the originating `InitStep` prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInitError(pub String);

impl SessionInitError {
    fn prefixed(step: InitStep, detail: impl std::fmt::Display) -> Self {
        Self(format!("{step:?}: {detail}"))
    }

    fn unavailable(step: InitStep, what: &str) -> Self {
        Self(format!("{step:?}: unavailable {what} owner"))
    }
}

impl std::fmt::Display for SessionInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SessionInitError {}

/// Production wiring of the driver over the live session owners. Owned:
/// the `Init` guard (live initialized flag + open prefix), police init,
/// traffic flags, the base-cache `IsOpen` read, and the `MarkInitialized`
/// flag write. Every other boundary returns `Err` with its step prefix and
/// preserves the reached live prefix: the driver stops at the first
/// unowned step. Actor `GameInit`, scripts and rendering never run
/// here; program execution remains in the headless probe. This is the owned
/// `Init` prefix without claiming gameplay.
impl InitOperations for crate::mission_load_session::MissionLoadSession {
    type Error = SessionInitError;

    fn execute(&mut self, step: InitStep) -> Result<(), Self::Error> {
        match step {
            InitStep::GuardNotInitialized => self
                .guard_game_init_for_init()
                .map_err(|detail| SessionInitError::prefixed(step, detail)),
            InitStep::ResetMissionState => self
                .reset_game_mission_state_for_init()
                .map_err(|detail| SessionInitError::prefixed(step, detail)),
            InitStep::PoliceBeginShift => {
                Err(SessionInitError::unavailable(step, "police shift begin"))
            }
            InitStep::PlayerNavi => Err(SessionInitError::unavailable(step, "player navipoint")),
            InitStep::VehicleSectors => Err(SessionInitError::unavailable(step, "vehicle sectors")),
            InitStep::PhysicsClear => Err(SessionInitError::unavailable(step, "physics clear")),
            InitStep::PoliceGameInit => self
                .apply_police_game_init_for_init()
                .map_err(|detail| SessionInitError::prefixed(step, detail)),
            InitStep::TrafficFlags => self
                .apply_traffic_flags_for_init()
                .map_err(|detail| SessionInitError::prefixed(step, detail)),
            InitStep::SoundAndLightness => {
                Err(SessionInitError::unavailable(step, "sound/lightness"))
            }
            InitStep::GlobalProgramStart => {
                Err(SessionInitError::unavailable(step, "program-runtime"))
            }
            InitStep::IndicatorsGameInit => Err(SessionInitError::unavailable(
                step,
                "indicators HUD (IndicatorsState::game_init_texture_plan/apply, GAME 0x5f73c0; no HUD field on this session yet)",
            )),
            InitStep::EffectsStart => Err(SessionInitError::unavailable(step, "effects start")),
            InitStep::DiffClear => Err(SessionInitError::unavailable(step, "diff pools")),
            InitStep::BloodAndFramePools => {
                Err(SessionInitError::unavailable(step, "blood/frame pools"))
            }
            InitStep::DriverAndConsoles => {
                Err(SessionInitError::unavailable(step, "driver consoles"))
            }
            InitStep::SegmentsLoad => Err(SessionInitError::unavailable(step, "segments load")),
            InitStep::CarTables => Err(SessionInitError::unavailable(step, "car tables")),
            InitStep::HydrantsAndWheels => {
                Err(SessionInitError::unavailable(step, "hydrants/wheels"))
            }
            InitStep::ShadowModels => Err(SessionInitError::unavailable(step, "shadow models")),
            InitStep::PlayerSelect => Err(SessionInitError::unavailable(step, "player")),
            InitStep::ActorsGameInit => Err(SessionInitError::unavailable(step, "actor GameInit")),
            InitStep::ActorLists => Err(SessionInitError::unavailable(step, "actor-list")),
            InitStep::InterCityAndTraffic => {
                Err(SessionInitError::unavailable(step, "inter-city traffic"))
            }
            InitStep::CameraAndChase => Err(SessionInitError::unavailable(step, "camera")),
            InitStep::MissionSystems => Err(SessionInitError::unavailable(step, "mission systems")),
            InitStep::TrackResources => Err(SessionInitError::unavailable(step, "track resources")),
            InitStep::RmorfAndDriverRestore => {
                Err(SessionInitError::unavailable(step, "rmorf restore"))
            }
            InitStep::PlayerHud => Err(SessionInitError::unavailable(
                step,
                "player HUD clamp (IndicatorsState::apply_player_hud, game.cpp:937-947; no HUD/human fields on this session yet)",
            )),
            InitStep::BuffersAndPrograms => {
                Err(SessionInitError::unavailable(step, "program-runtime"))
            }
            InitStep::BaseBlocks => self
                .read_base_blocks_for_init()
                .map(|_| ())
                .map_err(|detail| SessionInitError::prefixed(step, detail)),
            InitStep::MarkInitialized => self
                .mark_game_initialized_for_init()
                .map_err(|detail| SessionInitError::prefixed(step, detail)),
        }
    }
}

#[cfg(test)]
#[path = "game_init_session_tests.rs"]
mod session_tests;

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorder {
        visited: Vec<InitStep>,
        fail_at: Option<InitStep>,
    }

    impl InitOperations for Recorder {
        type Error = &'static str;

        fn execute(&mut self, step: InitStep) -> Result<(), Self::Error> {
            self.visited.push(step);
            if self.fail_at == Some(step) {
                return Err("unavailable owner");
            }
            Ok(())
        }
    }

    #[test]
    fn order_matches_native_init_body() {
        // Guard first, mark-initialized last; player select precedes actor
        // init; programs close the sequence after resources.
        assert_eq!(
            InitStep::ORDER.first(),
            Some(&InitStep::GuardNotInitialized)
        );
        assert_eq!(InitStep::ORDER.last(), Some(&InitStep::MarkInitialized));
        let position = |step| InitStep::ORDER.iter().position(|s| *s == step).unwrap();
        assert!(position(InitStep::PlayerSelect) < position(InitStep::ActorsGameInit));
        assert!(position(InitStep::ActorsGameInit) < position(InitStep::CameraAndChase));
        assert!(position(InitStep::GlobalProgramStart) < position(InitStep::BuffersAndPrograms));
        assert!(position(InitStep::TrackResources) < position(InitStep::RmorfAndDriverRestore));
        assert_eq!(InitStep::ORDER.len(), 31);
        let mut distinct = InitStep::ORDER.to_vec();
        distinct.sort_by_key(|s| *s as u8);
        distinct.dedup_by_key(|s| *s as u8);
        assert_eq!(distinct.len(), InitStep::ORDER.len());
    }

    #[test]
    fn successful_run_visits_every_step_once() {
        let mut ops = Recorder {
            visited: Vec::new(),
            fail_at: None,
        };
        let mut attempt = InitAttempt::default();
        assert_eq!(attempt.run(&mut ops), Ok(()));
        assert_eq!(ops.visited, InitStep::ORDER);
        assert_eq!(attempt.last(), Some(InitStep::MarkInitialized));
        assert_eq!(attempt.run(&mut ops), Err(InitRunError::AlreadyEntered));
        assert_eq!(ops.visited, InitStep::ORDER);
    }

    #[test]
    fn failure_retains_prefix_and_refuses_replay() {
        let mut ops = Recorder {
            visited: Vec::new(),
            fail_at: Some(InitStep::PoliceGameInit),
        };
        let mut attempt = InitAttempt::default();
        assert_eq!(
            attempt.run(&mut ops),
            Err(InitRunError::Callback(InitFailure {
                step: InitStep::PoliceGameInit,
                error: "unavailable owner",
            }))
        );
        assert_eq!(attempt.last(), Some(InitStep::PoliceGameInit));
        assert_eq!(
            &ops.visited,
            &InitStep::ORDER[..=InitStep::ORDER
                .iter()
                .position(|s| *s == InitStep::PoliceGameInit)
                .unwrap()]
        );
        assert_eq!(attempt.run(&mut ops), Err(InitRunError::AlreadyEntered));
    }

    #[test]
    fn indicators_steps_delegate_to_hud_owner() {
        // The session still lacks the HUD field, so its own arms stay
        // unavailable; the live owner contract is `IndicatorsInit`, which
        // executes exactly the two indicator-owned steps.
        use crate::indicators::{IndicatorsInit, IndicatorsState, TimerClock, INDF_PLAYER};
        let mut state = IndicatorsState::constructed(TimerClock {
            hour: 9,
            minute: 0,
            second: 0,
        });
        let mut ops = IndicatorsInit {
            state: &mut state,
            night_mission: false,
            speedometer_mph: true,
            player_energy_fraction: Some(0.5),
        };
        ops.execute(InitStep::IndicatorsGameInit).unwrap();
        assert_eq!(ops.state.dial_asset(), Some("2intmph.tga"));
        ops.execute(InitStep::PlayerHud).unwrap();
        assert_eq!(ops.state.show_flags(), INDF_PLAYER);
        assert_eq!(ops.state.lives(), 50);
        let error = ops.execute(InitStep::PoliceBeginShift).unwrap_err();
        assert_eq!(error.step, InitStep::PoliceBeginShift);
    }
}
