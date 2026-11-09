//! Actual source intake at the ordered Open boundaries, using public fixtures.
use super::*;
use crate::actor_world::ActorSource;
use crate::mission_open_order::{OpenResult, OpenRunError, OpenStep};
use crate::native_precision::current_precision;
use crate::scene_projection::{SceneRenderBackend, NOT_INITIALIZED};

struct Renderer<F> {
    calls: usize,
    action: F,
}

impl<F> SceneRenderBackend for Renderer<F>
where
    F: FnMut(&ModelArena, SceneKey) -> Result<u32, ModelError>,
{
    fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
        self.calls += 1;
        (self.action)(arena, scene)
    }
}

fn renderer() -> Renderer<impl FnMut(&ModelArena, SceneKey) -> Result<u32, ModelError>> {
    Renderer {
        calls: 0,
        action: |arena: &ModelArena, scene| {
            assert!(arena.scene(scene).is_ok());
            assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
            // An opaque native return word is not a host callback failure.
            Ok(NOT_INITIALIZED)
        },
    }
}

fn open(
    session: &mut MissionLoadSession,
    renderer: &mut impl SceneRenderBackend,
) -> Result<OpenResult, MissionSourceOpenError> {
    session.open_source_in_order(
        "synthetic",
        &[],
        request(&[]).projection,
        &mut Screen::default(),
        renderer,
    )
}

fn assert_step(error: MissionSourceOpenError, expected: OpenStep) {
    let MissionSourceOpenError::Open(OpenRunError::Callback(failure)) = error else {
        panic!("unexpected source Open failure: {error:?}");
    };
    assert_eq!(failure.step, expected);
    assert!(failure.error.0.starts_with(&format!("{expected:?}:")));
}

fn assert_blocked(session: &mut MissionLoadSession) {
    let counts = session.owned_counts().unwrap();
    let failure = format!("{:?}", session.failure());
    let mut render = renderer();
    assert_eq!(
        open(session, &mut render),
        Err(MissionSourceOpenError::Admission(
            MissionLoadCallError::AlreadyEntered
        ))
    );
    assert_eq!(
        load(session, &[]),
        Err(MissionLoadCallError::AlreadyEntered)
    );
    assert_eq!(render.calls, 0);
    assert!(session.guard_game_init_for_init().is_err());
    assert!(session.mark_game_initialized_for_init().is_err());
    assert!(session.tick_restore_volume_head().is_err());
    assert!(session.initialize_owned_global_programs().is_err());
    assert_eq!(session.owned_counts().unwrap(), counts);
    assert_eq!(format!("{:?}", session.failure()), failure);
}

fn navigation() -> Vec<u8> {
    [
        0x01ab_cedfu32.to_le_bytes().as_slice(),
        1u32.to_le_bytes().as_slice(),
        &[0; 30],
    ]
    .concat()
}

fn roads() -> Vec<u8> {
    [
        crate::road_network::ROAD_VERSION.to_le_bytes().as_slice(),
        1u32.to_le_bytes().as_slice(),
        &[0; crate::road_network::ROAD_NODE_SIZE],
        0u32.to_le_bytes().as_slice(),
    ]
    .concat()
}

fn assert_nonfresh_preserved(session: &mut MissionLoadSession) {
    let counts = session.owned_counts().unwrap();
    let navigation = (
        session.navigation().outcome().clone(),
        session.navigation().node_count(),
        session.navigation().link_count(),
        session.navigation().live_allocations(),
        session.navigation().live_bytes(),
    );
    let roads = (
        session.roads().outcome().clone(),
        session.roads().network().cloned(),
    );
    let cache = session.base_cache().clone();
    let precision = session.precision_state();
    let failures = format!("{:?}/{:?}", session.failure(), session.discard_failure());
    let inputs = (
        session.open_mission_name().map(<[u8]>::to_vec),
        session.open_configured_volume().to_bits(),
        session.open_cache_progress(),
        session.open_restore_volume(),
        session.open_muted(),
        session.open_restored().map(f32::to_bits),
    );
    assert_eq!(session.phase(), MissionLoadPhase::NotStarted);
    assert!(!session.entered);
    assert!(session.scene().unwrap().is_none());
    let mut screen = Screen::default();
    let mut render = renderer();
    for _ in 0..2 {
        assert_eq!(
            session.open_source_in_order(
                "synthetic",
                &[],
                request(&[]).projection,
                &mut screen,
                &mut render,
            ),
            Err(MissionSourceOpenError::SessionNotFresh)
        );
        assert_eq!(screen.calls, 0);
        assert_eq!(render.calls, 0);
        assert_eq!(session.owned_counts().unwrap(), counts);
        assert_eq!(
            (
                session.navigation().outcome().clone(),
                session.navigation().node_count(),
                session.navigation().link_count(),
                session.navigation().live_allocations(),
                session.navigation().live_bytes(),
            ),
            navigation
        );
        assert_eq!(
            (
                session.roads().outcome().clone(),
                session.roads().network().cloned()
            ),
            roads
        );
        assert_eq!(session.base_cache(), &cache);
        assert_eq!(session.precision_state(), precision);
        assert_eq!(current_precision(), None);
        assert_eq!(
            format!("{:?}/{:?}", session.failure(), session.discard_failure()),
            failures
        );
        assert_eq!(
            (
                session.open_mission_name().map(<[u8]>::to_vec),
                session.open_configured_volume().to_bits(),
                session.open_cache_progress(),
                session.open_restore_volume(),
                session.open_muted(),
                session.open_restored().map(f32::to_bits),
            ),
            inputs
        );
        assert_eq!(session.phase(), MissionLoadPhase::NotStarted);
        assert!(!session.entered);
        assert!(!session.open_mission_opened());
        assert!(session.scene().unwrap().is_none());
    }
}

#[test]
fn ordered_source_open_preserves_previously_attempted_navigation_and_roads() {
    use crate::mission_session_navigation::SessionNavigationOutcome as Navigation;
    use crate::mission_session_roads::SessionRoadsOutcome as Roads;
    for navigation in [true, false] {
        for state in ["loaded", "absent", "failed", "unavailable"] {
            let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
            let path = fixture.0.join("missions/synthetic").join(if navigation {
                "check.bin"
            } else {
                "road.bin"
            });
            match state {
                "loaded" => std::fs::write(
                    &path,
                    if navigation {
                        self::navigation()
                    } else {
                        roads()
                    },
                )
                .unwrap(),
                "absent" => {}
                "failed" => std::fs::write(&path, b"bad").unwrap(),
                "unavailable" => std::fs::create_dir(&path).unwrap(),
                _ => unreachable!(),
            }
            let mut session = fixture.session();
            session.set_open_mission_name("retained input").unwrap();
            session.set_open_configured_volume(0.625).unwrap();
            let result = if navigation {
                session.load_navigation_for_open("synthetic")
            } else {
                session.load_roads_for_open("synthetic")
            };
            assert_eq!(result.is_ok(), matches!(state, "loaded" | "absent"));
            if navigation {
                assert!(matches!(
                    (state, session.navigation().outcome()),
                    ("loaded", Navigation::Loaded { .. })
                        | ("absent", Navigation::Absent { .. })
                        | ("failed", Navigation::Failed { .. })
                        | ("unavailable", Navigation::SourceUnavailable { .. })
                ));
            } else {
                assert!(matches!(
                    (state, session.roads().outcome()),
                    ("loaded", Roads::Loaded { .. })
                        | ("absent", Roads::Absent { .. })
                        | ("failed", Roads::Failed { .. })
                        | ("unavailable", Roads::SourceUnavailable { .. })
                ));
            }
            assert_nonfresh_preserved(&mut session);
            // In particular, a Failed navigation owner is still interrupted;
            // admission must not try a destructor or force it back to empty.
        }
    }
}

#[test]
fn ordered_source_open_preserves_legacy_open_bin_failure_owners() {
    let fixture = Fixture::new(Some(&world(&[])), None, None);
    std::fs::write(fixture.0.join("missions/synthetic/check.bin"), navigation()).unwrap();
    std::fs::write(fixture.0.join("missions/synthetic/road.bin"), roads()).unwrap();
    let mut session = fixture.session();
    session.set_open_mission_name("synthetic").unwrap();
    session.set_open_configured_volume(0.25).unwrap();
    let mut attempt = crate::mission_open_order::OpenAttempt::default();
    assert_eq!(attempt.run(&mut session), Ok(OpenResult::OpenBinFailed));
    assert_eq!(attempt.last(), Some(OpenStep::AdvanceCacheProgress));
    assert_eq!(session.navigation().node_count(), 1);
    assert_eq!(session.roads().node_count(), 1);
    assert!(session.open_restore_volume());
    assert_nonfresh_preserved(&mut session);
}

fn cache_source() -> Vec<u8> {
    fn text(bytes: &[u8]) -> Vec<u8> {
        [(bytes.len() as u32).to_le_bytes().as_slice(), bytes].concat()
    }
    let mut model = text(b"car.i3d");
    for value in [1f32, 2., 3., 1., 0., 0., 0., 1., 2., 1.] {
        model.extend(value.to_le_bytes());
    }
    model.extend(7u32.to_le_bytes());
    for value in [2f32, 3., 4.] {
        model.extend(value.to_le_bytes());
    }
    let mut block = text(b"district");
    block.extend([0; 19 * 4]);
    block.extend(chunk(2000, &model));
    chunk(
        500,
        &[1u32.to_le_bytes().as_slice(), &chunk(1000, &block)].concat(),
    )
}

#[test]
fn ordered_source_open_preserves_existing_cache_and_open_effect_markers() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    std::fs::write(
        fixture.0.join("missions/synthetic/cache.bin"),
        cache_source(),
    )
    .unwrap();
    let mut cached = fixture.session();
    assert!(cached.open_cache_bin_for_open("synthetic").unwrap());
    assert!(cached.base_cache().is_open());
    assert_eq!(cached.base_cache().blocks().len(), 1);
    assert_eq!(cached.base_cache().model_sets()[0].name, b"car.i3d");
    assert_nonfresh_preserved(&mut cached);
    cached.close_previous_for_open().unwrap();
    assert!(!cached.base_cache().is_open());
    assert!(cached.base_cache().blocks().is_empty());
    assert_eq!(
        cached.base_cache().unknown_28(),
        crate::city_base_cache::BASE_CACHE_OPEN_UNKNOWN_28
    );
    assert_nonfresh_preserved(&mut cached);

    let mut probed = fixture.session();
    assert!(probed.probe_cache_bin_for_open("synthetic").unwrap());
    assert_eq!(probed.open_cache_progress(), 1);
    assert!(!probed.base_cache().is_open());
    assert_nonfresh_preserved(&mut probed);

    let mut restore_pending = fixture.session();
    restore_pending.mark_open_restore_volume().unwrap();
    assert_nonfresh_preserved(&mut restore_pending);
}

#[test]
fn ordered_source_open_accepts_configuration_without_consumed_owners() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    session
        .set_open_mission_name("previous configuration")
        .unwrap();
    session.set_open_configured_volume(0.625).unwrap();
    let mut render = renderer();
    assert_eq!(open(&mut session, &mut render), Ok(OpenResult::Opened));
    assert_eq!(render.calls, 1);
    assert_eq!(session.open_mission_name(), Some(b"synthetic".as_slice()));
    assert_eq!(session.audio().master_gain().unwrap(), 0.625);
    discard(&mut session);
}

#[test]
fn ordered_source_open_retains_real_world_actors_programs_and_restarts_cleanly() {
    let bytes = scene2(
        &[
            chunk(0xae20, &actor_record(b"world part", 1, b"")),
            chunk(0xae50, &program_record(b"source", b"dim_act 1")),
        ]
        .concat(),
    );
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
    for _ in 0..2 {
        std::fs::write(fixture.0.join("missions/synthetic/check.bin"), navigation()).unwrap();
        std::fs::write(fixture.0.join("missions/synthetic/road.bin"), roads()).unwrap();
        let mut session = fixture.session();
        session.set_open_configured_volume(0.375).unwrap();
        let audio = session.audio().clone();
        let mut render = Renderer {
            calls: 0,
            action: |arena: &ModelArena, scene| {
                assert_eq!(audio.master_gain().unwrap(), 0.0);
                assert_eq!(arena.scene(scene).unwrap().frames().len(), 1);
                assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
                // Both earlier inputs are already consumed; later OpenBin
                // still sees its own source. There is no post-render preload.
                std::fs::write(fixture.0.join("missions/synthetic/check.bin"), b"bad").unwrap();
                std::fs::write(fixture.0.join("missions/synthetic/road.bin"), b"bad").unwrap();
                Ok(NOT_INITIALIZED)
            },
        };
        assert_eq!(open(&mut session, &mut render), Ok(OpenResult::Opened));
        assert_eq!(render.calls, 1);
        assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
        assert!(session.open_mission_opened());
        assert_eq!(session.open_mission_name(), Some(b"synthetic".as_slice()));
        assert_eq!(session.audio().master_gain().unwrap(), 0.375);
        assert_eq!(session.open_restored(), Some(0.375));
        assert_eq!(session.navigation().node_count(), 1);
        assert_eq!(session.roads().node_count(), 1);
        let scene = session.scene().unwrap().unwrap();
        let frame = session.world_progress().unwrap().frames()[0];
        let loader = session.loader().unwrap();
        assert_eq!(loader.cursor, bytes.len());
        assert_eq!(loader.content.mission_path(), Some(b"synthetic".as_slice()));
        assert_eq!(
            loader.load_map.map.slot(b"world part").flatten(),
            Some(session.arena().lookup_reference(frame))
        );
        let actor = loader.content.actors.world.source(ActorSource::Primary)[0].unwrap();
        assert_eq!(
            loader.content.actors.world.state(actor).unwrap().frame,
            Some(session.arena().lookup_reference(frame))
        );
        assert_eq!(
            loader.content.programs()[0].source().unwrap().c_bytes(),
            Some(b"dim_act 1".as_slice())
        );
        assert_eq!(
            session.arena().scene_active_camera(scene).unwrap(),
            loader.camera
        );
        session.initialize_owned_global_programs().unwrap();
        session.discard_global_program_runtimes_host().unwrap();
        discard(&mut session);
        assert_eq!(
            session.owned_counts().unwrap(),
            MissionOwnedCounts::default()
        );
        assert!(session.arena().frame(frame).is_err());
        assert_eq!(current_precision(), None);
    }
}

#[test]
fn ordered_source_open_rejects_present_invalid_world_before_later_effects() {
    let fixture = Fixture::new(Some(b"not a 4DS world"), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    let mut render = renderer();
    assert_step(
        open(&mut session, &mut render).unwrap_err(),
        OpenStep::OpenWorld,
    );
    assert_eq!(render.calls, 0);
    assert_eq!(session.phase(), MissionLoadPhase::World);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::World(_)
    ));
    assert!(session.loader().is_none());
    assert_eq!(session.scene2_byte_count(), None);
    assert!(!session.open_muted());
    assert_blocked(&mut session);
    discard(&mut session);
}

#[test]
fn ordered_source_open_missing_scene2_restores_audio_without_map_or_camera() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), None, None);
    let mut session = fixture.session();
    session.set_open_configured_volume(0.25).unwrap();
    let mut render = renderer();
    assert_eq!(
        open(&mut session, &mut render),
        Ok(OpenResult::OpenBinFailed)
    );
    assert_eq!(render.calls, 1);
    assert_eq!(
        session.phase(),
        MissionLoadPhase::OpenStep(OpenStep::AdvanceCacheProgress)
    );
    assert_eq!(session.audio().master_gain().unwrap(), 0.25);
    assert!(session.open_restore_volume());
    assert!(!session.open_mission_opened());
    assert!(session.failure().is_none());
    assert!(session.loader().is_none());
    assert!(session.load_map().is_none());
    let scene = session.scene().unwrap().unwrap();
    assert_eq!(session.arena().scene_active_camera(scene).unwrap(), None);
    assert_eq!(session.scene2_byte_count(), None);
    assert_blocked(&mut session);
    discard(&mut session);
}

#[test]
fn ordered_source_open_corruption_retains_actual_map_camera_and_first_error() {
    for bytes in [
        b"invalid root".to_vec(),
        scene2(&chunk(0xae20, &[1, 2, 3, 4])),
    ] {
        let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
        let mut session = fixture.session();
        let mut render = renderer();
        assert_step(
            open(&mut session, &mut render).unwrap_err(),
            OpenStep::OpenBin,
        );
        assert_eq!(session.phase(), MissionLoadPhase::Scene2Prefix);
        assert!(matches!(
            session.failure().unwrap().cause,
            MissionLoadCause::Scene2Prefix(_)
        ));
        assert_eq!(session.scene2_byte_count(), Some(bytes.len()));
        let loader = session.loader().unwrap();
        assert!(loader.load_map.map.slot(b"world part").flatten().is_some());
        assert!(loader.camera.is_some());
        assert_eq!(session.audio().master_gain().unwrap(), 0.0);
        assert_eq!(session.open_restored(), None);
        assert!(session.open_restore_volume());
        assert!(!session.open_mission_opened());
        assert_blocked(&mut session);
        discard(&mut session);
    }
}

#[test]
fn ordered_source_open_reads_scene2_once_after_render_callback_changes_it() {
    let original = scene2(&chunk(0x4000, &object(9, b"old source", &[])));
    let replacement = scene2(&chunk(0x4000, &object(9, b"new source", &[])));
    let fixture = Fixture::new(Some(&world(&[])), Some(&original), None);
    let mut session = fixture.session();
    let mut render = Renderer {
        calls: 0,
        action: |arena: &ModelArena, scene| {
            assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
            std::fs::write(
                fixture.0.join("missions/synthetic/scene2.bin"),
                &replacement,
            )
            .unwrap();
            Ok(0)
        },
    };
    assert_eq!(open(&mut session, &mut render), Ok(OpenResult::Opened));
    assert_eq!(render.calls, 1);
    assert_eq!(
        session.scene2_bytes.as_deref(),
        Some(replacement.as_slice())
    );
    let map = &session.load_map().unwrap().map;
    assert!(map.slot(b"new source").flatten().is_some());
    assert_eq!(map.slot(b"old source"), None);
    discard(&mut session);
}

#[test]
fn ordered_source_open_render_error_and_unwind_keep_mute_precision_and_admission() {
    use std::panic::{catch_unwind, panic_any, AssertUnwindSafe};
    use std::sync::Arc;
    for unwind in [false, true] {
        let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
        let mut session = fixture.session();
        let audio = session.audio().clone();
        let token = Arc::new(());
        let expected_token = token.clone();
        let mut render = Renderer {
            calls: 0,
            action: |arena: &ModelArena, scene| {
                assert_eq!(audio.master_gain().unwrap(), 0.0);
                assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
                assert!(current_precision().is_some());
                assert_eq!(
                    crate::native_precision::pc24_integer_ratio(1, 3).to_bits(),
                    0x3eaa_aaab
                );
                assert_eq!(current_precision(), Some(true));
                if unwind {
                    panic_any(token.clone());
                }
                Err(ModelError::OriginalModelOpenUnavailable(
                    "synthetic render refusal",
                ))
            },
        };
        let result = catch_unwind(AssertUnwindSafe(|| open(&mut session, &mut render)));
        if unwind {
            let payload = result.unwrap_err().downcast::<Arc<()>>().unwrap();
            assert!(Arc::ptr_eq(&payload, &expected_token));
            assert!(session.failure().is_none());
        } else {
            assert_step(result.unwrap().unwrap_err(), OpenStep::MuteAudioAndRender);
        }
        assert_eq!(render.calls, 1);
        assert_eq!(
            session.phase(),
            MissionLoadPhase::OpenStep(OpenStep::MuteAudioAndRender)
        );
        assert!(session.precision_state().precision_raised());
        assert_eq!(current_precision(), None);
        assert_eq!(session.audio().master_gain().unwrap(), 0.0);
        assert!(!session.open_restore_volume());
        assert_eq!(session.open_restored(), None);
        assert_eq!(session.scene2_byte_count(), None);
        assert!(session.loader().is_none());
        assert_blocked(&mut session);
        discard(&mut session);
    }
}

#[test]
fn ordered_source_open_cache_tail_error_keeps_loaded_sources_unadmitted() {
    let bytes = scene2(&chunk(0xae50, &program_record(b"source", b"dim_act 1")));
    let fixture = Fixture::new(Some(&world(&[])), Some(&bytes), None);
    let cache = fixture.0.join("missions/synthetic/cache.bin");
    std::fs::write(&cache, []).unwrap();
    let mut session = fixture.session();
    session.set_open_configured_volume(0.5).unwrap();
    let mut render = Renderer {
        calls: 0,
        action: |_: &ModelArena, _| {
            std::fs::remove_file(&cache).unwrap();
            std::fs::create_dir(&cache).unwrap();
            Ok(0)
        },
    };
    assert_step(
        open(&mut session, &mut render).unwrap_err(),
        OpenStep::OpenCache,
    );
    assert_eq!(
        session.phase(),
        MissionLoadPhase::OpenStep(OpenStep::OpenCache)
    );
    assert!(session.open_mission_opened());
    assert_eq!(session.open_cache_progress(), 2);
    assert_eq!(session.open_restored(), Some(0.5));
    assert_eq!(session.loader().unwrap().content.programs().len(), 1);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::OpenStep {
            step: OpenStep::OpenCache,
            ..
        }
    ));
    assert_blocked(&mut session);
    assert!(session.open_restore_volume());
    assert!(session
        .snapshot_loaded_scene(
            [800, 600],
            &mut crate::scene_render::SceneSnapshotSequence::default()
        )
        .is_err());
    discard(&mut session);
}

#[test]
fn ordered_source_open_navigation_or_roads_failure_precedes_render_and_scene2() {
    for (name, step) in [
        ("check.bin", OpenStep::LoadNavigation),
        ("road.bin", OpenStep::LoadRoads),
    ] {
        let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
        std::fs::write(fixture.0.join("missions/synthetic").join(name), b"bad").unwrap();
        let mut session = fixture.session();
        let mut render = renderer();
        assert_step(open(&mut session, &mut render).unwrap_err(), step);
        assert_eq!(render.calls, 0);
        assert!(session.world_progress().is_some());
        assert_eq!(session.scene2_byte_count(), None);
        assert!(!session.open_muted());
        assert_blocked(&mut session);
        let first_failure = format!("{:?}", session.failure().unwrap());
        if step == OpenStep::LoadNavigation {
            // The native navigation component was constructed before its
            // short read. Its existing interrupted destructor refuses; keep
            // that owner and the subsequent Scene instead of forcing cleanup.
            let before = session.owned_counts().unwrap();
            assert!(session.navigation().live_allocations() > 0);
            let scene = session.scene().unwrap().unwrap();
            assert_eq!(
                session.discard_terminal(),
                Err(MissionDiscardCallError::Stopped)
            );
            assert_eq!(
                session.discard_progress().phase,
                MissionDiscardPhase::Navigation
            );
            assert!(matches!(
                session.discard_failure().unwrap().cause,
                MissionDiscardCause::Navigation(SessionNavigationCause::Component(
                    crate::mission_components::ComponentError::Interrupted
                ))
            ));
            assert_eq!(session.owned_counts().unwrap(), before);
            assert!(session.arena().scene(scene).is_ok());
            assert!(!session.discard_progress().scene_release_executed);
            let cleanup_failure = format!("{:?}", session.discard_failure().unwrap());
            assert_eq!(
                session.discard_terminal(),
                Err(MissionDiscardCallError::AlreadyEntered)
            );
            assert_eq!(
                open(&mut session, &mut render),
                Err(MissionSourceOpenError::Admission(
                    MissionLoadCallError::TerminalDiscardEntered
                ))
            );
            assert_eq!(
                load(&mut session, &[]),
                Err(MissionLoadCallError::TerminalDiscardEntered)
            );
            assert_eq!(session.owned_counts().unwrap(), before);
            assert_eq!(
                format!("{:?}", session.discard_failure().unwrap()),
                cleanup_failure
            );
        } else {
            // The road parser publishes no owner on failure; its normal
            // terminal cleanup still has to release every retained resource.
            discard(&mut session);
        }
        assert_eq!(format!("{:?}", session.failure().unwrap()), first_failure);
    }
}

#[test]
fn ordered_source_open_admission_precedes_construction_or_rendering() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut already_loaded = fixture.session();
    load(&mut already_loaded, &[]).unwrap();
    let before = already_loaded.owned_counts().unwrap();
    let mut render = renderer();
    assert_eq!(
        open(&mut already_loaded, &mut render),
        Err(MissionSourceOpenError::Admission(
            MissionLoadCallError::AlreadyEntered
        ))
    );
    assert_eq!(already_loaded.owned_counts().unwrap(), before);
    discard(&mut already_loaded);
    assert_eq!(
        open(&mut already_loaded, &mut render),
        Err(MissionSourceOpenError::Admission(
            MissionLoadCallError::TerminalDiscardEntered
        ))
    );
    let mut already_open = fixture.session();
    already_open.mark_open_mission_opened().unwrap();
    let before = already_open.owned_counts().unwrap();
    assert_eq!(
        open(&mut already_open, &mut render),
        Err(MissionSourceOpenError::MissionAlreadyOpened)
    );
    assert_eq!(already_open.owned_counts().unwrap(), before);
    assert!(already_open.scene().unwrap().is_none());
    assert_eq!(render.calls, 0);
}

#[test]
fn ordered_source_open_constructor_failure_retains_original_cleanup_boundary() {
    let fixture = Fixture::new(None, None, None);
    let mut session = fixture.session();
    let mut screen = Screen {
        fail_size: true,
        calls: 0,
    };
    let mut render = renderer();
    assert_eq!(
        session.open_source_in_order(
            "synthetic",
            &[],
            request(&[]).projection,
            &mut screen,
            &mut render
        ),
        Err(MissionSourceOpenError::BootstrapStopped)
    );
    assert_eq!(screen.calls, 1);
    assert_eq!(render.calls, 0);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::SceneCreation {
            phase: DriverSceneCreatePhase::Constructor,
            ..
        }
    ));
    assert_blocked(&mut session);
    let before = session.owned_counts().unwrap();
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert_eq!(session.owned_counts().unwrap(), before);
    assert!(!session.discard_progress().scene_release_executed);
    assert_eq!(current_precision(), None);
}
