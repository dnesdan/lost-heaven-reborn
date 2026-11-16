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

fn write_car_index(fixture: &Fixture) {
    let mut bytes = vec![0; 2 * crate::car_database::CAR_INDEX_RECORD_BYTES];
    bytes[..6].copy_from_slice(b"unused");
    bytes[0xa8..0xa8 + 5].copy_from_slice(b"alpha");
    std::fs::write(fixture.0.join("tables/carindex.def"), bytes).unwrap();
}

#[test]
fn startup_database_and_selection_refuse_a_first_call_after_open() {
    let fixture = fixture();
    write_car_index(&fixture);
    let mut session = fixture.session();
    open(&mut session);
    assert!(session.entered);
    assert_eq!(
        session.car_database().intake(),
        crate::car_database::IndexIntake::NotStarted
    );
    assert_eq!(
        session.read_car_database_for_startup(),
        Err("car database intake requires a fresh session".into())
    );
    assert_eq!(
        session.car_database().intake(),
        crate::car_database::IndexIntake::NotStarted
    );
    assert_eq!(session.car_database().index_storage_bytes(), 0);
    let before = *session.freeride();
    assert!(!session.freeride_selection_adopted);
    assert_eq!(
        session.adopt_freeride_selection_host(1, 0x100, 7, 1.0, 0.0, -0.0),
        Err("Freeride selection requires fresh, unconfigured session state".into())
    );
    assert!(!session.freeride_selection_adopted);
    assert_eq!(*session.freeride(), before);
    discard(&mut session);
}

#[test]
fn diagnostic_adoption_uses_live_stores_and_cannot_replace_active() {
    let fixture = fixture();
    let mut session = fixture.session();
    session.freeride.active = true;
    session
        .adopt_freeride_selection_host(
            0xff00_0001,
            0x8000_0100,
            u32::MAX,
            f32::from_bits(0x7fc1_2345),
            -0.0,
            f32::NEG_INFINITY,
        )
        .unwrap();
    assert!(session.freeride().active);
    assert_eq!(session.freeride().car, 0xff00_0001);
    assert_eq!(session.freeride().flags, 0x8000_0100);
    assert_eq!(session.freeride().location, u32::MAX);
    assert_eq!(session.freeride().traffic_density.to_bits(), 0x7fc1_2345);
    assert_eq!(session.freeride().pedestrian_density.to_bits(), 0x8000_0000);
    assert_eq!(session.freeride().police_density.to_bits(), 0xff80_0000);
    assert!(session
        .adopt_freeride_selection_host(0, 0, 0, 0.0, 0.0, 0.0)
        .is_err());
    assert!(session.freeride().active);
    assert_eq!(session.freeride().car, 0xff00_0001);
    assert_eq!(session.freeride().location, u32::MAX);
    session.save_game_new_game_for_new_mission().unwrap();
    assert!(!session.freeride().active);
    assert_eq!(session.freeride().traffic_density.to_bits(), 0x7fc1_2345);
    open(&mut session);
    discard(&mut session);
}

#[test]
fn interrupted_database_intake_preserves_table_prefix_and_stops_setup() {
    let fixture = fixture();
    write_car_index(&fixture);
    std::fs::create_dir_all(fixture.0.join("tables/playerindex.def")).unwrap();
    let mut session = fixture.session();
    session
        .adopt_freeride_selection_host(1, 0, 1, 1.0, 1.0, 1.0)
        .unwrap();
    let error = session.read_car_database_for_startup().unwrap_err();
    assert!(error.contains("Players"), "{error}");
    assert_eq!(
        session.car_database().intake(),
        crate::car_database::IndexIntake::Interrupted(crate::car_database::IndexTable::Players)
    );
    assert_eq!(session.car_database().car_count(), 2);
    assert_eq!(session.car_database().player_count(), 0);
    assert!(session.read_car_database_for_startup().is_err());
    assert_eq!(session.car_database().car_count(), 2);
    open(&mut session);
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::CarDatabaseUnavailable)
    ));
    assert_eq!(session.freeride_setup_progress().held_model_creators(), 2);
    assert!(session
        .freeride_setup_progress()
        .resolved_car_model()
        .is_none());
    assert!(session.caches.model_open_progress().is_none());
    discard(&mut session);
}

#[test]
fn setup_refuses_unterminated_and_oversized_names_before_cache_open() {
    for unterminated in [true, false] {
        let fixture = fixture();
        let mut bytes = vec![b'x'; if unterminated { 0xa8 } else { 2 * 0xa8 }];
        if !unterminated {
            *bytes.last_mut().unwrap() = 0;
        }
        std::fs::write(fixture.0.join("tables/carindex.def"), bytes).unwrap();
        let mut session = fixture.session();
        session.read_car_database_for_startup().unwrap();
        session
            .adopt_freeride_selection_host(0, 0, 1, 1.0, 1.0, 1.0)
            .unwrap();
        open(&mut session);
        let result = session.setup_freeride();
        if unterminated {
            assert!(
                matches!(
                    result,
                    Err(FreerideSetupError::CarModelLookup(
                        crate::car_database::CarDatabaseError::UnterminatedCarId
                    ))
                ),
                "{result:?}"
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(FreerideSetupError::CarModelLookup(
                        crate::car_database::CarDatabaseError::OutputExtent {
                            required: 342,
                            available: 256
                        }
                    ))
                ),
                "{result:?}"
            );
        }
        assert_eq!(
            session.freeride_setup_progress().reached().last(),
            Some(&FreerideSetupStep::ResolveCarModel)
        );
        assert_eq!(session.freeride_setup_progress().held_model_creators(), 2);
        assert!(session
            .freeride_setup_progress()
            .resolved_car_model()
            .is_none());
        assert!(session.freeride_setup_progress().start_marker().is_none());
        assert!(session.caches.model_open_progress().is_none());
        assert!(matches!(
            session.setup_freeride(),
            Err(FreerideSetupError::AlreadyEntered)
        ));
        discard(&mut session);
    }
}

#[test]
fn database_intake_uses_the_session_source_and_does_not_invent_a_selection() {
    let fixture = fixture();
    write_car_index(&fixture);
    std::fs::write(fixture.0.join("tables/playerindex.def"), [0u8; 36]).unwrap();
    let mut session = fixture.session();
    session.read_car_database_for_startup().unwrap();
    assert_eq!(session.car_database().car_count(), 2);
    assert_eq!(session.car_database().player_count(), 1);
    assert!(session.read_car_database_for_startup().is_err());
    open(&mut session);
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::SelectionUnavailable)
    ));
    assert_eq!(session.freeride().car, 0);
    assert_eq!(session.freeride_setup_progress().held_model_creators(), 2);
    assert!(session
        .freeride_setup_progress()
        .resolved_car_model()
        .is_none());
    assert!(session.read_car_database_for_startup().is_err());
    discard(&mut session);
    assert_eq!(session.car_database().index_storage_bytes(), 2 * 0xa8 + 36);
}

#[test]
fn invalid_selected_car_does_not_open_a_fallback_model_or_replay() {
    let fixture = fixture();
    write_car_index(&fixture);
    let mut session = fixture.session();
    session.read_car_database_for_startup().unwrap();
    let mut state = crate::game_loadsave_lifecycle::FreerideState::default();
    state.store_a(2, 0x100, 1.0, 0.0, -0.0);
    state.store_b(7);
    session
        .adopt_freeride_selection_host(2, 0x100, 7, 1.0, 0.0, -0.0)
        .unwrap();
    assert!(session
        .adopt_freeride_selection_host(2, 0x100, 7, 1.0, 0.0, -0.0)
        .is_err());
    session.save_game_new_game_for_new_mission().unwrap();
    assert_eq!(session.freeride(), &state);
    open(&mut session);
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::CarModelNotWritten)
    ));
    assert_eq!(
        session.freeride_setup_progress().reached().last(),
        Some(&FreerideSetupStep::ResolveCarModel)
    );
    assert!(session.caches.model_open_progress().is_none());
    assert!(session
        .adopt_freeride_selection_host(2, 0x100, 7, 1.0, 0.0, -0.0)
        .is_err());
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::AlreadyEntered)
    ));
    discard(&mut session);
    assert!(session
        .adopt_freeride_selection_host(2, 0x100, 7, 1.0, 0.0, -0.0)
        .is_err());
}

#[test]
fn actual_cache_model_open_precedes_location_and_retains_the_same_creators() {
    let fixture = fixture();
    write_car_index(&fixture);
    std::fs::create_dir_all(fixture.0.join("Models")).unwrap();
    std::fs::write(
        fixture.0.join("Models/alpha00.4ds"),
        world(&[dummy_with_culling(0, b"part", 0x11)]),
    )
    .unwrap();
    let mut session = fixture.session();
    session.read_car_database_for_startup().unwrap();
    session
        .adopt_freeride_selection_host(1, 0x100, u32::MAX, 0.5, 0.25, 0.75)
        .unwrap();
    let mut attempt = FreerideAttempt::default();
    let result = session.start_freeride(
        &mut attempt,
        FreerideRequest {
            model_directories: &[b"Models"],
            projection: projection(),
        },
        &mut Screen::default(),
        &mut NoCameraRender,
    );
    assert!(
        matches!(
            result,
            Err(FreerideRunError::Callback(FreerideFailure {
                step: FreerideStep::FreerideSetup,
                error: FreerideError::Setup(FreerideSetupError::SceneNamedLookupUnavailable)
            }))
        ),
        "{result:?}"
    );
    let progress = session.freeride_setup_progress();
    assert_eq!(
        progress.resolved_car_model(),
        Some(b"alpha00.i3d".as_slice())
    );
    assert_eq!(progress.car_cache_result(), Some(0));
    assert_eq!(progress.start_marker(), Some(b"emeth_-1".as_slice()));
    assert_eq!(
        &progress.reached()[8..],
        &[
            FreerideSetupStep::OpenCarModel,
            FreerideSetupStep::LocateStartFrame
        ]
    );
    assert!(!attempt.init().entered());
    let car_model = progress.car_model().unwrap();
    assert_eq!(
        session.arena.frame_name_or_empty(car_model.root()).unwrap(),
        b"TomyhoAuto"
    );
    assert_eq!(session.arena.model(car_model).unwrap().frames.len(), 1);
    assert_eq!(session.arena.links(car_model.root()).unwrap().parent, None);
    assert_eq!(session.arena.frame(car_model.root()).unwrap().owner, None);
    assert_eq!(session.freeride().flags, 0x100);
    assert!(session
        .loader
        .as_ref()
        .unwrap()
        .content
        .actors
        .world
        .source(ActorSource::Primary)
        .is_empty());
    discard(&mut session);
    assert_eq!(session.freeride_setup_progress().held_model_creators(), 0);
}

#[test]
fn host_cache_failure_retains_prefix_without_reading_the_location() {
    let fixture = fixture();
    write_car_index(&fixture);
    std::fs::create_dir_all(fixture.0.join("Models")).unwrap();
    std::fs::write(fixture.0.join("Models/alpha00.4ds"), b"broken").unwrap();
    let mut session = fixture.session();
    session.read_car_database_for_startup().unwrap();
    session
        .adopt_freeride_selection_host(1, 0, 7, 0.0, 0.0, 0.0)
        .unwrap();
    let result = session.start_freeride(
        &mut FreerideAttempt::default(),
        FreerideRequest {
            model_directories: &[b"Models"],
            projection: projection(),
        },
        &mut Screen::default(),
        &mut NoCameraRender,
    );
    assert!(
        matches!(
            result,
            Err(FreerideRunError::Callback(FreerideFailure {
                error: FreerideError::Setup(FreerideSetupError::CarModelCache(_)),
                ..
            }))
        ),
        "{result:?}"
    );
    assert_eq!(
        session.freeride_setup_progress().reached().last(),
        Some(&FreerideSetupStep::OpenCarModel)
    );
    assert!(session.freeride_setup_progress().start_marker().is_none());
    assert!(session
        .freeride_setup_progress()
        .car_cache_result()
        .is_none());
    assert!(matches!(
        session.setup_freeride(),
        Err(FreerideSetupError::AlreadyEntered)
    ));
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::Cache(MissionCacheKind::Model)
    );
    assert_eq!(
        session.discard_failure().unwrap().phase,
        MissionDiscardPhase::Cache(MissionCacheKind::Model)
    );
    assert!(matches!(
        session.discard_failure().unwrap().cause,
        MissionDiscardCause::Cache {
            kind: MissionCacheKind::Model,
            ..
        }
    ));
    assert_eq!(session.freeride_setup_progress().held_model_creators(), 0);
}

#[test]
fn native_cache_hresult_is_ignored_before_the_named_lookup_boundary() {
    let fixture = fixture();
    write_car_index(&fixture);
    let mut session = fixture.session();
    session.read_car_database_for_startup().unwrap();
    session
        .adopt_freeride_selection_host(1, 0, 7, 0.0, 0.0, 0.0)
        .unwrap();
    let result = start(&mut session, &mut FreerideAttempt::default());
    assert!(
        matches!(
            result,
            Err(FreerideRunError::Callback(FreerideFailure {
                error: FreerideError::Setup(FreerideSetupError::SceneNamedLookupUnavailable),
                ..
            }))
        ),
        "{result:?}"
    );
    assert_eq!(
        session.freeride_setup_progress().car_cache_result(),
        Some(0x8000_0001u32 as i32)
    );
    assert_eq!(
        session.freeride_setup_progress().start_marker(),
        Some(b"emeth_7".as_slice())
    );
    assert_eq!(
        session
            .arena
            .model(session.freeride_setup_progress().car_model().unwrap())
            .unwrap()
            .frames
            .len(),
        0
    );
    discard(&mut session);
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
