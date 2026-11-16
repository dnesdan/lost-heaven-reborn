use super::*;
use crate::actor_world::ActorSource;
use crate::mission_open_order::OpenResult;

#[derive(Default)]
struct Operations {
    calls: Vec<FreerideStep>,
    fail: Option<FreerideStep>,
    open_result: Option<OpenResult>,
}
impl Operations {
    fn enter(&mut self, step: FreerideStep) -> Result<(), FreerideStep> {
        self.calls.push(step);
        if self.fail == Some(step) {
            Err(step)
        } else {
            Ok(())
        }
    }
}
impl FreerideOperations for Operations {
    type Error = FreerideStep;
    fn execute(&mut self, step: FreerideStep) -> Result<(), Self::Error> {
        self.enter(step)
    }
    fn open(
        &mut self,
        _: &mut crate::mission_open_order::OpenAttempt,
    ) -> Result<OpenResult, Self::Error> {
        self.enter(FreerideStep::Open)?;
        Ok(self.open_result.unwrap_or(OpenResult::Opened))
    }
    fn game_init(
        &mut self,
        _: &mut crate::game_init_order::InitAttempt,
    ) -> Result<(), Self::Error> {
        self.enter(FreerideStep::GameInit)
    }
}

fn fixture() -> Fixture {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    std::fs::rename(
        fixture.0.join("missions/synthetic"),
        fixture.0.join("missions/freeride"),
    )
    .unwrap();
    std::fs::create_dir_all(fixture.0.join("tables/menu")).unwrap();
    std::fs::write(
        fixture.0.join("tables/menu/loadprogress.mnu"),
        loadprogress_menu_bytes(),
    )
    .unwrap();
    let mut entry = vec![0u8; 0x44];
    entry[..8].copy_from_slice(b"FREERIDE");
    entry[0x20..0x25].copy_from_slice(b"F.tga");
    std::fs::write(fixture.0.join("tables/load.def"), entry).unwrap();
    fixture
}

fn start(
    session: &mut MissionLoadSession,
    attempt: &mut FreerideAttempt,
) -> Result<FreerideResult, FreerideRunError<FreerideError>> {
    session.start_freeride(
        attempt,
        FreerideRequest {
            model_directories: &[],
            projection: projection(),
        },
        &mut Screen::default(),
        &mut NoCameraRender,
    )
}
fn open(session: &mut MissionLoadSession) {
    assert_eq!(
        session.open_source_in_order(
            &mut Default::default(),
            FREERIDE_MISSION,
            true,
            &[],
            projection(),
            &mut Screen::default(),
            &mut NoCameraRender
        ),
        Ok(OpenResult::Opened)
    );
}

#[test]
fn native_outer_order_includes_setup_before_init_and_after_before_loop() {
    let expected = [
        FreerideStep::ClosePrevious,
        FreerideStep::Open,
        FreerideStep::LoadCollision,
        FreerideStep::ResetMissionNumbers,
        FreerideStep::SaveGameNewGame,
        FreerideStep::FreerideSetup,
        FreerideStep::GameInit,
        FreerideStep::FreerideSetupAfter,
        FreerideStep::GameLoopSingle,
        FreerideStep::Close,
    ];
    let mut operations = Operations::default();
    let mut attempt = FreerideAttempt::default();
    assert_eq!(attempt.scope(), FreerideScope::FreshSession);
    assert_eq!(attempt.run(&mut operations), Ok(FreerideResult::Returned));
    assert_eq!(operations.calls, expected);
    assert_eq!(attempt.reached(), expected);
    assert_eq!(
        attempt.run(&mut operations),
        Err(FreerideRunError::AlreadyEntered)
    );
    assert_eq!(operations.calls, expected);
}

#[test]
fn every_error_retains_only_its_reached_prefix_and_blocks_replay() {
    for (index, step) in FreerideStep::ORDER.into_iter().enumerate() {
        let mut operations = Operations {
            fail: Some(step),
            ..Default::default()
        };
        let mut attempt = FreerideAttempt::default();
        assert_eq!(
            attempt.run(&mut operations),
            Err(FreerideRunError::Callback(FreerideFailure {
                step,
                error: step
            }))
        );
        assert_eq!(operations.calls, FreerideStep::ORDER[..=index]);
        assert_eq!(attempt.reached(), operations.calls);
        assert_eq!(
            attempt.run(&mut operations),
            Err(FreerideRunError::AlreadyEntered)
        );
        assert_eq!(operations.calls.len(), index + 1);
    }
}

#[test]
fn nonopened_result_never_enters_collision_or_setup() {
    let mut operations = Operations {
        open_result: Some(OpenResult::OpenBinFailed),
        ..Default::default()
    };
    let mut attempt = FreerideAttempt::default();
    assert_eq!(
        attempt.run(&mut operations),
        Ok(FreerideResult::OpenFailed(OpenResult::OpenBinFailed))
    );
    assert_eq!(attempt.reached(), &FreerideStep::ORDER[..2]);
    assert!(!attempt.init().entered());
}

#[test]
fn actual_freeride_prefix_uses_one_actor_owner_and_retained_model_creators() {
    let fixture = fixture();
    let mut session = fixture.session();
    let mut attempt = FreerideAttempt::default();
    let result = start(&mut session, &mut attempt);
    assert!(
        matches!(
            result,
            Err(FreerideRunError::Callback(FreerideFailure {
                step: FreerideStep::FreerideSetup,
                error: FreerideError::Setup(FreerideSetupError::CarDatabaseUnavailable)
            }))
        ),
        "{result:?}; {:?}",
        session.failure()
    );
    assert_eq!(attempt.reached(), &FreerideStep::ORDER[..6]);
    assert_eq!(attempt.open_result(), Some(OpenResult::Opened));
    assert!(!attempt.init().entered());
    assert!(session.freeride().active);
    let progress = session.freeride_setup_progress();
    assert_eq!(
        progress.reached(),
        &[
            FreerideSetupStep::Activate,
            FreerideSetupStep::CreatePlayer,
            FreerideSetupStep::CreateCar,
            FreerideSetupStep::CreatePlayerModel,
            FreerideSetupStep::NamePlayerModel,
            FreerideSetupStep::CreateCarModel,
            FreerideSetupStep::NameCarModel,
            FreerideSetupStep::ResolveCarModel
        ]
    );
    assert_eq!(progress.held_model_creators(), 2);
    let player = progress.player().unwrap();
    let car = progress.car().unwrap();
    assert_ne!(player, car);
    for (model, name) in [
        (progress.player_model().unwrap(), b"Tommy".as_slice()),
        (progress.car_model().unwrap(), b"TomyhoAuto".as_slice()),
    ] {
        assert_eq!(
            session.arena.frame_name_or_empty(model.root()).unwrap(),
            name
        );
        assert_eq!(session.arena.links(model.root()).unwrap().parent, None);
        assert_eq!(session.arena.frame(model.root()).unwrap().owner, None);
    }
    let actors = &session.loader.as_ref().unwrap().content.actors.world;
    assert_eq!(actors.owned_actor_references().count(), 2);
    assert!(actors.player(player).is_ok());
    assert!(actors.car(car).is_ok());
    assert!(actors.is_global_actor(Some(player)));
    assert!(actors.is_global_actor(Some(car)));
    assert_eq!(actors.global_actor_count(), 2);
    assert!(actors.source(ActorSource::Primary).is_empty());
    let counts = session.owned_counts().unwrap();
    assert!(matches!(
        start(&mut session, &mut attempt),
        Err(FreerideRunError::AlreadyEntered)
    ));
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::AlreadyEntered)
    ));
    assert!(start(&mut session, &mut FreerideAttempt::default()).is_err());
    assert_eq!(session.owned_counts().unwrap(), counts);
    discard(&mut session);
    assert_eq!(session.freeride_setup_progress().held_model_creators(), 0);
    assert!(session.setup_freeride().is_err());
}

#[test]
fn interrupted_freeride_setup_cannot_enter_common_init() {
    for missing_scene in [false, true] {
        let fixture = fixture();
        let mut session = fixture.session();
        open(&mut session);
        let original_scene = session.scene_reference().unwrap();
        if missing_scene {
            session.scene = None;
        }
        assert!(session.setup_freeride().is_err());
        session.scene = Some(original_scene);
        let counts = session.owned_counts().unwrap();
        let reached = session.freeride_setup_progress().reached().to_vec();
        let mut init = crate::game_init_order::InitAttempt::default();
        assert!(init.run(&mut session).is_err());
        assert_eq!(
            init.last(),
            Some(crate::game_init_order::InitStep::GuardNotInitialized)
        );
        assert!(!session.game_init_reset_entered);
        assert_eq!(session.freeride_setup_progress().reached(), reached);
        assert_eq!(session.owned_counts().unwrap(), counts);
        discard(&mut session);
    }
}

#[test]
fn setup_requires_complete_original_open_before_init_or_disposal() {
    let fixture = fixture();
    let mut session = fixture.session();
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::Admission(_))
    ));
    assert!(!session.freeride_setup_progress().entered());
    open(&mut session);
    session.game_init_reset_entered = true;
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::Admission(_))
    ));
    assert!(!session.freeride().active);
    assert!(!session.freeride_setup_progress().entered());
    discard(&mut session);
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::Admission(_))
    ));
}

#[test]
fn another_mission_cannot_be_used_as_freeride() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::Admission(_))
    ));
    assert!(!session.freeride_setup_progress().entered());
    assert!(!session.freeride().active);
    discard(&mut session);
}

#[test]
fn failed_source_open_cannot_be_promoted_to_freeride_setup() {
    let fixture = fixture();
    std::fs::remove_file(fixture.0.join("missions/freeride/scene2.bin")).unwrap();
    let mut session = fixture.session();
    let mut attempt = FreerideAttempt::default();
    assert!(matches!(
        start(&mut session, &mut attempt),
        Ok(FreerideResult::OpenFailed(OpenResult::OpenBinFailed))
    ));
    assert_eq!(attempt.last(), Some(FreerideStep::Open));
    assert!(!session.freeride_setup_progress().entered());
    assert!(session.setup_freeride().is_err());
    assert!(!attempt.init().entered());
    discard(&mut session);
}

#[test]
fn scene_failure_retains_created_actors_without_replaying_setup() {
    let fixture = fixture();
    let mut session = fixture.session();
    open(&mut session);
    let original_scene = session.scene_reference().unwrap();
    session.scene = None;
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::Model {
            step: FreerideSetupStep::CreatePlayerModel,
            ..
        })
    ));
    assert!(session.freeride().active);
    assert!(session.freeride_setup_progress().player().is_some());
    assert!(session.freeride_setup_progress().car().is_some());
    assert_eq!(session.freeride_setup_progress().held_model_creators(), 0);
    assert_eq!(
        session
            .loader
            .as_ref()
            .unwrap()
            .content
            .actors
            .world
            .owned_actor_references()
            .count(),
        2
    );
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::AlreadyEntered)
    ));
    session.scene = Some(original_scene);
    discard(&mut session);
}

struct CacheTailFailure(std::path::PathBuf);
impl crate::scene_projection::SceneRenderBackend for CacheTailFailure {
    fn render_scene(&mut self, _: &ModelArena, _: SceneKey) -> Result<u32, ModelError> {
        std::fs::remove_file(&self.0).unwrap();
        std::fs::create_dir(&self.0).unwrap();
        Ok(0)
    }
}
impl crate::load_scene_callback::LoadingScreenDevice for CacheTailFailure {
    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn present_loading_screen(
        &mut self,
        _: crate::load_scene_callback::LoadingScreenFrame<'_>,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn native_opened_flag_before_failed_cache_tail_does_not_admit_setup() {
    let fixture = fixture();
    let path = fixture.0.join("missions/freeride/cache.bin");
    std::fs::write(&path, []).unwrap();
    let mut session = fixture.session();
    let mut attempt = FreerideAttempt::default();
    let result = session.start_freeride(
        &mut attempt,
        FreerideRequest {
            model_directories: &[],
            projection: projection(),
        },
        &mut Screen::default(),
        &mut CacheTailFailure(path),
    );
    assert!(matches!(
        result,
        Err(FreerideRunError::Callback(FreerideFailure {
            step: FreerideStep::Open,
            ..
        }))
    ));
    assert_eq!(attempt.open().last(), Some(OpenStep::OpenCache));
    assert!(session.open_mission_opened());
    assert_eq!(attempt.open_result(), None);
    assert!(session.loader().is_some());
    assert!(session.setup_freeride().is_err());
    assert!(!session.freeride_setup_progress().entered());
    assert!(!session.freeride().active);
    assert!(!attempt.init().entered());
    discard(&mut session);
}

#[test]
fn bootstrap_failure_never_reaches_setup_and_cannot_be_replayed() {
    let fixture = fixture();
    std::fs::write(fixture.0.join("missions/freeride/scene.4ds"), b"bad").unwrap();
    let mut session = fixture.session();
    let mut attempt = FreerideAttempt::default();
    let result = start(&mut session, &mut attempt);
    assert!(matches!(
        result,
        Err(FreerideRunError::Callback(FreerideFailure {
            step: FreerideStep::Open,
            ..
        }))
    ));
    assert!(!session.freeride_setup_progress().entered());
    assert!(session.setup_freeride().is_err());
    let counts = session.owned_counts().unwrap();
    assert!(matches!(
        start(&mut session, &mut attempt),
        Err(FreerideRunError::AlreadyEntered)
    ));
    assert_eq!(session.owned_counts().unwrap(), counts);
    discard(&mut session);
}

#[test]
fn stale_setup_creator_stops_terminal_cleanup_without_releasing_other_roots() {
    let fixture = fixture();
    let mut session = fixture.session();
    assert!(start(&mut session, &mut FreerideAttempt::default()).is_err());
    let car_model = session.freeride_setup_progress().car_model().unwrap();
    session.arena.release(car_model.root()).unwrap();
    assert!(session.discard_terminal().is_err());
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::FreerideModels
    );
    assert!(matches!(
        session.discard_failure().unwrap().cause,
        MissionDiscardCause::FreerideModel(ModelError::UnknownFrame(_))
    ));
    assert_eq!(session.freeride_setup_progress().held_model_creators(), 2);
    assert_eq!(
        session
            .loader()
            .unwrap()
            .content
            .actors
            .world
            .owned_actor_references()
            .count(),
        0
    );
    let player = session.freeride_setup_progress().player_model().unwrap();
    assert!(session.arena.frame(player.root()).is_ok());
    let counts = session.owned_counts().unwrap();
    assert!(session.discard_terminal().is_err());
    assert_eq!(session.owned_counts().unwrap(), counts);
}
