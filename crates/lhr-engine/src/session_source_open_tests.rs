//! Actual source intake at the ordered Open boundaries, using public fixtures.
use super::*;
use crate::actor_world::ActorSource;
use crate::mission_open_order::{OpenAttempt, OpenResult, OpenRunError, OpenStep};
use crate::native_precision::current_precision;
use crate::scene_projection::{SceneRenderBackend, NOT_INITIALIZED};

struct Renderer<F> {
    calls: usize,
    action: F,
    /// Loading-screen device calls: (STEP SetState count, presented bars).
    alpha_blend_states: usize,
    loading_presents: Vec<f32>,
}

impl<F> crate::load_scene_callback::LoadingScreenDevice for Renderer<F> {
    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        self.alpha_blend_states += 1;
        Ok(())
    }
    fn present_loading_screen(
        &mut self,
        frame: crate::load_scene_callback::LoadingScreenFrame<'_>,
    ) -> Result<(), String> {
        self.loading_presents.push(frame.screen.raw_progress());
        Ok(())
    }
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
        alpha_blend_states: 0,
        loading_presents: Vec::new(),
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
    renderer: &mut (impl SceneRenderBackend + crate::load_scene_callback::LoadingScreenDevice),
) -> Result<OpenResult, MissionSourceOpenError> {
    session.open_source_in_order(
        &mut OpenAttempt::default(),
        "synthetic",
        false,
        &[],
        projection(),
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
        Err(LoadStop::Admission(MissionLoadCallError::AlreadyEntered))
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
                &mut OpenAttempt::default(),
                "synthetic",
                false,
                &[],
                projection(),
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
            alpha_blend_states: 0,
            loading_presents: Vec::new(),
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

/// Host validation stops, not native RAscend failures: a wrong root kind
/// natively skips to OPENBIN_OK, and the malformed actor root is decoded by a
/// nested reader with no native corruption mapping. Both stay host errors at
/// OpenBin, so the post-OpenBin restore and progress steps are not reached.
#[test]
fn ordered_source_open_host_scene2_validation_stop_retains_map_camera_and_first_error() {
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

/// Native OPENBIN_CORRUPTED (mafia-re Mission.cpp:901-908 and `fail:`
/// 1685-1694): a header read at end of file fails RAscend after the map and
/// camera exist. C_mission::Open (Mission.cpp:670-677) then restores the
/// volume and advances the progress mode before returning failure.
#[test]
fn ordered_source_open_short_scene2_header_is_native_open_bin_failure() {
    let truncated_child = scene2(&[1, 2, 3]);
    for bytes in [Vec::new(), vec![0x53, 0x4c, 9], truncated_child] {
        let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
        std::fs::write(
            fixture.0.join("missions/synthetic/cache.bin"),
            cache_source(),
        )
        .unwrap();
        let mut session = fixture.session();
        session.set_open_configured_volume(0.25).unwrap();
        let mut attempt = OpenAttempt::default();
        let mut render = renderer();
        assert_eq!(
            session.open_source_in_order(
                &mut attempt,
                "synthetic",
                false,
                &[],
                projection(),
                &mut Screen::default(),
                &mut render,
            ),
            Ok(OpenResult::OpenBinFailed),
            "{bytes:?}"
        );
        let through_progress = OPENED_WITH_AUDIO
            .iter()
            .position(|step| *step == OpenStep::AdvanceCacheProgress)
            .unwrap();
        assert_eq!(attempt.reached(), &OPENED_WITH_AUDIO[..=through_progress]);
        assert_eq!(render.calls, 1);
        assert_eq!(session.audio().master_gain().unwrap(), 0.25);
        assert_eq!(session.open_restored(), Some(0.25));
        assert_eq!(session.open_cache_progress(), 2);
        assert!(session.open_restore_volume());
        assert!(!session.open_mission_opened());
        assert!(!session.base_cache().is_open());
        assert!(session.failure().is_none());
        assert!(
            session
                .open_bin_corruption()
                .unwrap()
                .native_open_bin_corrupted
        );
        assert_eq!(session.scene2_byte_count(), Some(bytes.len()));
        let loader = session.loader().unwrap();
        assert!(loader.load_map.map.slot(b"world part").flatten().is_some());
        assert!(loader.camera.is_some());
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
        alpha_blend_states: 0,
        loading_presents: Vec::new(),
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
            alpha_blend_states: 0,
            loading_presents: Vec::new(),
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
        alpha_blend_states: 0,
        loading_presents: Vec::new(),
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
                Err(LoadStop::Admission(
                    MissionLoadCallError::TerminalDiscardEntered
                ))
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
            &mut OpenAttempt::default(),
            "synthetic",
            false,
            &[],
            projection(),
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

/// Native C_mission::Open (GAME 0x5409d0, mafia-re Mission.cpp:623-687) with
/// a live audio owner: both cache.bin opens are attempted whether or not the
/// file exists, so presence changes owner state, never the boundary order.
const OPENED_WITH_AUDIO: &[OpenStep] = &[
    OpenStep::ResetGame,
    OpenStep::ClosePrevious,
    OpenStep::ResetPostClose,
    OpenStep::WaitForMedia,
    OpenStep::BindVehicleScene,
    OpenStep::CopyMissionName,
    OpenStep::PrepareProgress,
    OpenStep::OpenWorld,
    OpenStep::ResetWeather,
    OpenStep::FillSectorBits,
    OpenStep::LoadNavigation,
    OpenStep::LoadRoads,
    OpenStep::ProbeCache,
    OpenStep::MuteAudioAndRender,
    OpenStep::MarkRestoreVolume,
    OpenStep::OpenBin,
    OpenStep::RestoreAudio,
    OpenStep::AdvanceCacheProgress,
    OpenStep::MarkMissionOpened,
    OpenStep::OpenCache,
    OpenStep::FinishProgress,
];

fn open_recorded(session: &mut MissionLoadSession) -> (OpenAttempt, usize) {
    let mut attempt = OpenAttempt::default();
    let mut render = renderer();
    assert_eq!(
        session.open_source_in_order(
            &mut attempt,
            "synthetic",
            false,
            &[],
            projection(),
            &mut Screen::default(),
            &mut render,
        ),
        Ok(OpenResult::Opened)
    );
    (attempt, render.calls)
}

#[test]
fn ordered_source_open_step_sequence_with_cache_bin_present() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    std::fs::write(
        fixture.0.join("missions/synthetic/cache.bin"),
        cache_source(),
    )
    .unwrap();
    let mut session = fixture.session();
    let (attempt, renders) = open_recorded(&mut session);
    assert_eq!(attempt.reached(), OPENED_WITH_AUDIO);
    assert_eq!(attempt.last(), Some(OpenStep::FinishProgress));
    assert_eq!(renders, 1);
    // ProbeCache found the file (mode 1), AdvanceCacheProgress made it 2,
    // and OpenCache consumed it through base_cache.Open.
    assert_eq!(session.open_cache_progress(), 2);
    assert!(session.base_cache().is_open());
    assert_eq!(session.base_cache().blocks().len(), 1);
    assert!(session.open_restore_volume());
    assert!(session.open_mission_opened());
    discard(&mut session);
}

#[test]
fn ordered_source_open_step_sequence_with_cache_bin_absent() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    let (attempt, renders) = open_recorded(&mut session);
    assert_eq!(attempt.reached(), OPENED_WITH_AUDIO);
    assert_eq!(attempt.last(), Some(OpenStep::FinishProgress));
    assert_eq!(renders, 1);
    // Mode stays 0 (only 1 advances); the final open's absence is the empty
    // native branch, not a missing backend.
    assert_eq!(session.open_cache_progress(), 0);
    assert!(!session.base_cache().is_open());
    assert!(session.base_cache().blocks().is_empty());
    assert!(session.open_restore_volume());
    assert!(session.open_mission_opened());
    discard(&mut session);
}

#[test]
fn ordered_source_open_refuses_a_replayed_attempt_before_bootstrap() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut first = fixture.session();
    let (mut attempt, _) = open_recorded(&mut first);
    let mut second = fixture.session();
    let mut screen = Screen::default();
    let mut render = renderer();
    assert_eq!(
        second.open_source_in_order(
            &mut attempt,
            "synthetic",
            false,
            &[],
            projection(),
            &mut screen,
            &mut render,
        ),
        Err(MissionSourceOpenError::Open(OpenRunError::AlreadyEntered))
    );
    assert_eq!((screen.calls, render.calls), (0, 0));
    assert_eq!(second.phase(), MissionLoadPhase::NotStarted);
    assert!(second.scene().unwrap().is_none());
    assert_eq!(attempt.reached(), OPENED_WITH_AUDIO);
    discard(&mut first);
}

fn seed_mission_numbers(session: &mut MissionLoadSession) {
    // current 3 (via KeepMissionNumber), pending 7.
    session.mission_counters.set_mission_number(3);
    session.mission_counters.keep_mission_number();
    session.mission_counters.set_mission_number(7);
    assert_eq!(session.mission_numbers(), (7, 3));
}

fn seed_load_save_progress(session: &mut MissionLoadSession) {
    use crate::game_loadsave_lifecycle::GarageSlot;
    session.load_save_progress.steal_cars = 0b1011;
    session.load_save_progress.last_stolen = Some(4);
    session.load_save_progress.garage.slots[0] = GarageSlot {
        car: 0x0301,
        actor: Some(4),
    };
    session.load_save_progress.garage.shown_cars[2] = 0x0301;
    session.freeride.active = true;
}

const NEW_GAME_THROUGH_INIT: &[crate::mission_new_game_order::NewMissionStep] = &[
    crate::mission_new_game_order::NewMissionStep::ClosePrevious,
    crate::mission_new_game_order::NewMissionStep::Open,
    crate::mission_new_game_order::NewMissionStep::LoadCollision,
    crate::mission_new_game_order::NewMissionStep::ResetMissionNumbers,
    crate::mission_new_game_order::NewMissionStep::SaveGameNewGame,
    crate::mission_new_game_order::NewMissionStep::GameInit,
];

#[test]
fn new_game_start_runs_save_game_new_game_then_stops_at_the_init_render() {
    use crate::game_init_order::{InitFailure, InitRunError, InitStep, SessionInitError};
    use crate::game_loadsave_lifecycle::{FreerideState, LoadProgressScreen, LoadSaveProgress};
    use crate::mission_new_game_order::{
        NewMissionAttempt, NewMissionError, NewMissionFailure, NewMissionRequest,
        NewMissionRunError, NewMissionStep,
    };
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    seed_mission_numbers(&mut session);
    seed_load_save_progress(&mut session);
    let mut attempt = NewMissionAttempt::default();
    let mut render = renderer();
    let result = session.start_new_game_mission(
        &mut attempt,
        NewMissionRequest {
            mission: "synthetic",
            model_directories: &[],
            projection: projection(),
        },
        &mut Screen::default(),
        &mut render,
    );
    // An absent tree.klz is the native dtaOpen early return: no level, and
    // Main continues to ResetMissionNumbers, SaveGameNewGame and Init. The
    // unpresented Init stops at its Scene::Render effect after the mute.
    assert_eq!(
        result,
        Err(NewMissionRunError::Callback(NewMissionFailure {
            step: NewMissionStep::GameInit,
            error: NewMissionError::Init(InitRunError::Callback(InitFailure {
                step: InitStep::ResetMissionState,
                error: SessionInitError(
                    "ResetMissionState: Init Scene::Render after sound mute unavailable".into()
                ),
            })),
        }))
    );
    assert_eq!(attempt.reached(), NEW_GAME_THROUGH_INIT);
    assert_eq!(attempt.open_result(), Some(OpenResult::Opened));
    assert_eq!(attempt.open().reached(), OPENED_WITH_AUDIO);
    assert_eq!(render.calls, 1);
    assert_eq!(session.collision().counts(), Default::default());
    assert!(!session.collision().loaded_level());
    // ResetMissionNumbers and SaveGameNewGame ran before Init was entered;
    // Main's Open (progress true) created the loading screen, which
    // SaveGameNewGame leaves alone.
    assert_eq!(session.mission_numbers(), (0, 0));
    assert_eq!(
        session.load_save_progress(),
        &LoadSaveProgress {
            progress: Some(LoadProgressScreen::for_mission_open()),
            ..Default::default()
        }
    );
    assert_eq!(session.freeride(), FreerideState::default());
    assert!(attempt.init().entered());
    assert_eq!(attempt.init().last(), Some(InitStep::ResetMissionState));
    // The stopped Init did not mark the game initialized.
    assert!(session.guard_game_init_for_init().is_ok());
    // A present file shorter than the header stops as a host error.
    let path = fixture.0.join("missions/synthetic/tree.klz");
    std::fs::write(&path, b"bad").unwrap();
    assert_eq!(
        session.load_collision_for_new_mission("synthetic"),
        Err("missions/synthetic/tree.klz: TruncatedHeader; links visited 0/0 resolved=0 SetSpace calls=0".into())
    );
    // Links the Scene does not name stay null, so neither SetSpace nor a
    // LOD0 lock is reached and the level is installed into the owner.
    std::fs::write(&path, crate::game_collision::tests::fixture(2.0)).unwrap();
    assert_eq!(session.load_collision_for_new_mission("synthetic"), Ok(()));
    let level = session.collision().level().unwrap();
    assert_eq!(level.volumes[0].frame, None);
    assert_eq!(
        level.phase(),
        crate::game_collision::CollisionLevelPhase::Coherent
    );
    assert_eq!(session.collision().load_progress().links_visited, 2);
    let counts = session.owned_counts().unwrap();
    assert_eq!(
        (counts.collision_levels, counts.collision_dynamic_grids),
        (1, 1)
    );
    // Terminal disposal clears the owner before Scene release.
    discard(&mut session);
    assert_eq!(session.collision().counts(), Default::default());
}

#[test]
fn save_game_new_game_step_clears_progress_and_stops_on_a_terminal_session() {
    use crate::game_loadsave_lifecycle::{FreerideState, LoadSaveProgress};
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    // The step reads nothing, so it needs no Open; the session starts from
    // the zeroed globals.
    assert_eq!(session.load_save_progress(), &LoadSaveProgress::default());
    seed_load_save_progress(&mut session);
    assert_eq!(session.save_game_new_game_for_new_mission(), Ok(()));
    assert_eq!(session.load_save_progress(), &LoadSaveProgress::default());
    assert!(!session.freeride().active);
    // After terminal disposal the step stops before any store.
    open_recorded(&mut session);
    seed_load_save_progress(&mut session);
    discard(&mut session);
    let before = session.load_save_progress().clone();
    assert_eq!(
        session.save_game_new_game_for_new_mission(),
        Err("terminal discard entered".into())
    );
    assert_eq!(session.load_save_progress(), &before);
    assert_eq!(session.freeride(), FreerideState { active: true });
}

#[test]
fn presented_new_game_start_reaches_the_same_init_boundary_once() {
    use crate::game_init_order::InitStep;
    use crate::game_loadsave_lifecycle::LoadSaveProgress;
    use crate::mission_new_game_order::{
        NewMissionAttempt, NewMissionError, NewMissionFailure, NewMissionRequest,
        NewMissionRunError, NewMissionStep,
    };
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    seed_load_save_progress(&mut session);
    let mut attempt = NewMissionAttempt::default();
    let mut sequence = crate::scene_render::SceneSnapshotSequence::default();
    let mut presented = 0;
    let result = session.start_new_game_mission_presented(
        &mut attempt,
        NewMissionRequest {
            mission: "synthetic",
            model_directories: &[],
            projection: projection(),
        },
        &mut Screen::default(),
        &mut renderer(),
        [64, 48],
        &mut sequence,
        |_snapshot| {
            presented += 1;
            Err("host presentation declined".into())
        },
    );
    // Every step before Init is the unpresented composition's; Init stops
    // at the same ResetMissionState render boundary, here through the
    // presentation, whose one snapshot the host declines.
    let Err(NewMissionRunError::Callback(NewMissionFailure {
        step: NewMissionStep::GameInit,
        error: NewMissionError::Init(error),
    })) = &result
    else {
        panic!("unexpected presented new-game result: {result:?}");
    };
    // The snapshot reached the host exactly once and its refusal is the
    // stop: an unpresented Init would never call the host.
    assert_eq!(presented, 1);
    assert!(
        format!("{error:?}").contains("host presentation declined"),
        "{error:?}"
    );
    assert_eq!(attempt.reached(), NEW_GAME_THROUGH_INIT);
    assert_eq!(attempt.init().last(), Some(InitStep::ResetMissionState));
    assert_eq!(
        session.load_save_progress(),
        &LoadSaveProgress {
            progress: Some(crate::game_loadsave_lifecycle::LoadProgressScreen::for_mission_open()),
            ..Default::default()
        }
    );
    assert!(!session.freeride().active);
    discard(&mut session);
}

#[test]
fn new_game_close_is_a_native_no_op_only_before_open() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    assert_eq!(session.close_before_new_mission(), Ok(()));
    assert_eq!(
        session.load_collision_for_new_mission("synthetic"),
        Err("collision intake requires a completed Open".into())
    );
    seed_mission_numbers(&mut session);
    open_recorded(&mut session);
    assert!(session
        .close_before_new_mission()
        .unwrap_err()
        .contains("no live owner"));
    // Open and the refused Close leave the numbers; the explicit inline
    // ResetMissionNumbers clears both pending and current.
    assert_eq!(session.mission_numbers(), (7, 3));
    assert_eq!(session.reset_mission_numbers_for_new_mission(), Ok(()));
    assert_eq!(session.mission_numbers(), (0, 0));
    discard(&mut session);
}

#[test]
fn load_collision_runs_in_the_session_precision_scope() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    seed_mission_numbers(&mut session);
    open_recorded(&mut session);
    // Open's own arithmetic already raised the sticky bit; clear the
    // session's state so LoadCollision's contribution is observable.
    session.precision = crate::native_precision::Pc24PrecisionState::new(false);
    assert!(!session.precision_state().precision_raised());
    // The dyn grid divides the 10 x 5 extent by 3 (inexact) and truncates:
    // that x87 arithmetic raises the sticky precision bit, which the
    // session keeps for its later steps.
    let path = fixture.0.join("missions/synthetic/tree.klz");
    std::fs::write(&path, crate::game_collision::tests::fixture(3.0)).unwrap();
    assert_eq!(session.load_collision_for_new_mission("synthetic"), Ok(()));
    assert!(session.collision().dynamic_grid().is_some());
    assert!(session.precision_state().precision_raised());
    discard(&mut session);
}

fn write_loading_screen_tables(fixture: &Fixture) {
    let menu = fixture.0.join("tables/menu");
    std::fs::create_dir_all(&menu).unwrap();
    std::fs::write(
        menu.join("loadprogress.mnu"),
        super::loadprogress_menu_bytes(),
    )
    .unwrap();
    let mut entry = vec![0u8; 0x44];
    entry[..9].copy_from_slice(b"SYNTHETIC");
    entry[0x20..0x25].copy_from_slice(b"S.tga");
    entry[0x40..].copy_from_slice(&0x77u32.to_le_bytes());
    std::fs::write(fixture.0.join("tables/load.def"), entry).unwrap();
}

fn open_with_progress(
    session: &mut MissionLoadSession,
    progress: bool,
    render: &mut (impl SceneRenderBackend + crate::load_scene_callback::LoadingScreenDevice),
) -> Result<OpenResult, MissionSourceOpenError> {
    session.open_source_in_order(
        &mut OpenAttempt::default(),
        "synthetic",
        progress,
        &[],
        projection(),
        &mut Screen::default(),
        render,
    )
}

#[test]
fn open_reports_the_scene_load_to_the_mafia_load_save_screen() {
    use crate::game_loadsave_lifecycle::LOADPROGRESS_MISSION_PICTURE;
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"a"), dummy(0, b"b")])),
        Some(&scene2(&[])),
        None,
    );
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    let mut render = renderer();
    assert_eq!(
        open_with_progress(&mut session, true, &mut render),
        Ok(OpenResult::Opened)
    );
    // BEGIN presents the created screen with an empty bar. Frame 0's
    // LoadProgress(0/2 + 0.5) passes the 0.02 gate (bit 0.25), frame 1's
    // repeats 0.5 and is gated; the scene's END is the last STEP at half the
    // bar. Each shown STEP sets the blend state, then presents.
    assert_eq!(session.world_source_counts(), Some((0, 2)));
    assert_eq!(render.loading_presents, [0.0, 0.25f32 * 0.7, 0.5f32 * 0.7]);
    assert_eq!(render.alpha_blend_states, 2);
    let callback = session.load_scene_callback();
    assert_eq!(callback.menu_progress(), None);
    assert_eq!(callback.last_bit(), 0.5);
    // Open's FinishProgress END keeps the mafiaLoadSave screen, created
    // with the definition's components and load.def's picture and name
    // for the act mission ("synthetic", _stricmp-equal to "SYNTHETIC").
    let screen = session.load_save_progress().progress.as_ref().unwrap();
    assert_eq!(screen.menu.len(), 4);
    assert_eq!(screen.raw_progress(), 0.5 * 0.7);
    assert_eq!(
        screen.menu.staged_string(LOADPROGRESS_MISSION_PICTURE),
        Some(Some(&b"S.tga"[..]))
    );
    assert_eq!(
        screen
            .menu
            .get_text_id(crate::game_loadsave_lifecycle::LOADPROGRESS_MISSION_NAME),
        0x77
    );
    assert_eq!(render.calls, 1);
    discard(&mut session);
}

#[test]
fn open_without_progress_uses_and_destroys_the_callbacks_own_screen() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"a")])), Some(&scene2(&[])), None);
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    let mut render = renderer();
    assert_eq!(
        open_with_progress(&mut session, false, &mut render),
        Ok(OpenResult::Opened)
    );
    // GM_LoadProgress(0, 1.0f, NULL): the scene's END shows bit 0.5 as 0.5.
    assert_eq!(render.loading_presents.first(), Some(&0.0));
    assert_eq!(render.loading_presents.last(), Some(&0.5));
    // FinishProgress's END destroyed it; mafiaLoadSave has none.
    assert_eq!(session.load_scene_callback().menu_progress(), None);
    assert_eq!(session.load_save_progress().progress, None);
    discard(&mut session);
}

#[test]
fn open_without_the_hud_stops_at_the_loading_screen_before_the_world() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"a")])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    let mut render = renderer();
    let error = open_with_progress(&mut session, true, &mut render).unwrap_err();
    let MissionSourceOpenError::Open(OpenRunError::Callback(failure)) = error else {
        panic!("unexpected Open failure: {error:?}");
    };
    assert_eq!(failure.step, OpenStep::OpenWorld);
    assert!(
        failure
            .error
            .0
            .contains("cbLoadScene: GM_Menu::CreateCompsByDef: mafiaIndicators"),
        "{}",
        failure.error
    );
    // BEGIN precedes the file read: nothing was loaded or presented.
    assert_eq!(session.world_source_counts(), None);
    assert!(render.loading_presents.is_empty());
    assert!(session.load_save_progress().progress.is_some());
    discard(&mut session);
}

#[test]
fn vehicle_sectors_rebuild_the_light_sectors_from_the_live_primary_sector() {
    use crate::frame_transform::FrameKey;
    use crate::game_housekeeping::SectorLightBackend;
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part"), sector()])),
        Some(&scene2(&[])),
        None,
    );
    let mut session = fixture.session();
    assert_eq!(open(&mut session, &mut renderer()), Ok(OpenResult::Opened));
    // Open's ResetIntern: intern_load set, intern_loaded clear.
    assert!(session.game_inline.intern_load());
    assert!(!session.game_inline.intern_loaded());
    let scene = session.scene().unwrap().unwrap();
    let primary = session.arena.scene_primary_sector(scene).unwrap().frame();
    // Entries a previous fill (or the script writer) left behind.
    let stale = FrameKey(u32::MAX - 1);
    session
        .vehicle_sectors_mut()
        .add_sector_for_projector_lights(stale);
    session.vehicle_sectors_mut().add_sector_for_sounds(stale);
    assert_eq!(session.vehicle_sectors_for_init(), Ok(()));
    let named = |key| session.arena.frame_name_or_empty(key).unwrap().to_vec();
    // GAME 0x5a09d6-0x5a0a12: both statics erased, then cbEnumSectors over
    // `scene->primary_sector`: the primary sector registers itself, then
    // its sector child; the dummy world part is walked but is no sector.
    let sectors = session.vehicle_sectors();
    assert_eq!(sectors.light_sectors().len(), 2);
    assert_eq!(sectors.light_sectors()[0], primary);
    let names: Vec<_> = sectors
        .light_sectors()
        .iter()
        .map(|key| named(*key))
        .collect();
    assert_eq!(names, [&b"Primary sector"[..], b"synthetic sector"]);
    assert!(sectors.sound_sectors().is_empty());
    // The keys borrow the scene's frames: disposal empties both statics
    // before the scene closes, and every owner reaches zero.
    discard(&mut session);
    assert!(session.vehicle_sectors().light_sectors().is_empty());
    assert!(session.vehicle_sectors().sound_sectors().is_empty());
}

#[test]
fn vehicle_sectors_skip_the_rebuild_when_intern_load_is_clear() {
    use crate::frame_transform::FrameKey;
    use crate::game_housekeeping::SectorLightBackend;
    let fixture = Fixture::new(Some(&world(&[sector()])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    assert_eq!(open(&mut session, &mut renderer()), Ok(OpenResult::Opened));
    let stale = FrameKey(u32::MAX - 1);
    session
        .vehicle_sectors_mut()
        .add_sector_for_projector_lights(stale);
    session.vehicle_sectors_mut().add_sector_for_sounds(stale);
    // A clear intern_load skips the whole span (`jz 0x5a0a17`): neither
    // static is erased and nothing is walked.
    session.game_inline = crate::game_housekeeping::GameInlineState::default();
    assert_eq!(session.vehicle_sectors_for_init(), Ok(()));
    assert_eq!(session.vehicle_sectors().light_sectors(), [stale]);
    assert_eq!(session.vehicle_sectors().sound_sectors(), [stale]);
    discard(&mut session);
    assert!(session.vehicle_sectors().light_sectors().is_empty());
    assert!(session.vehicle_sectors().sound_sectors().is_empty());
}

/// A camera-less Open device whose `refuse_at`-th loading-screen Present
/// (0-based) is refused by the host.
struct RefusingDevice {
    presents: Vec<f32>,
    refuse_at: Option<usize>,
}

impl SceneRenderBackend for RefusingDevice {
    fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
        assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
        Ok(NOT_INITIALIZED)
    }
}

impl crate::load_scene_callback::LoadingScreenDevice for RefusingDevice {
    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn present_loading_screen(
        &mut self,
        frame: crate::load_scene_callback::LoadingScreenFrame<'_>,
    ) -> Result<(), String> {
        if self.refuse_at == Some(self.presents.len()) {
            return Err("host refused the present".into());
        }
        self.presents.push(frame.screen.raw_progress());
        Ok(())
    }
}

fn four_frame_world() -> Vec<u8> {
    world(&[
        dummy(0, b"f0"),
        dummy(0, b"f1"),
        dummy(0, b"f2"),
        dummy(0, b"f3"),
    ])
}

fn assert_world_stop(session: &MissionLoadSession, error: MissionSourceOpenError) {
    let MissionSourceOpenError::Open(OpenRunError::Callback(failure)) = error else {
        panic!("unexpected Open failure: {error:?}");
    };
    assert_eq!(failure.step, OpenStep::OpenWorld);
    assert!(
        failure.error.0.contains("host refused the present"),
        "{}",
        failure.error
    );
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::World(CpuSceneWorldError::Model(ModelError::LoadCallback(_)))
    ));
}

#[test]
fn a_refused_first_step_stops_the_world_before_its_frame() {
    let fixture = Fixture::new(Some(&four_frame_world()), Some(&scene2(&[])), None);
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    // Present 0 is BEGIN's; present 1 is the first STEP, frame 0's
    // LoadProgress(0.5) at LS3DF 0x10060eef, before frame 0 is read.
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: Some(1),
    };
    let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
    assert_world_stop(&session, error);
    assert_eq!(device.presents, [0.0]);
    // Nothing of the world was read: no frame, no transfer, no resources,
    // no sector preparation.
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.phase(), CpuSceneWorldPhase::Geometry);
    assert!(progress.frames().is_empty());
    assert_eq!(progress.transferred_frames(), 0);
    assert_eq!(progress.prepared_sectors(), 0);
    let scene = session.scene().unwrap().unwrap();
    assert!(session.arena().scene(scene).unwrap().frames().is_empty());
    // The refused STEP's bar write happened (SetProgressMissionOpen precedes
    // Present); last_bit was not stored.
    assert_eq!(
        session
            .load_save_progress()
            .progress
            .as_ref()
            .unwrap()
            .raw_progress(),
        0.25 * 0.7
    );
    assert_eq!(session.load_scene_callback().last_bit(), 0.0);
    discard(&mut session);
}

#[test]
fn a_refused_later_step_keeps_only_the_frames_read_before_it() {
    let fixture = Fixture::new(Some(&four_frame_world()), Some(&scene2(&[])), None);
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    // Frame 0 reports 0.5 (shown), frame 1 repeats 0.5 (gated), frame 2
    // reports 0.75: refused, before frame 2 is read.
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: Some(2),
    };
    let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
    assert_world_stop(&session, error);
    assert_eq!(device.presents, [0.0, 0.25f32 * 0.7]);
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.frames().len(), 2);
    assert_eq!(progress.transferred_frames(), 0);
    assert_eq!(session.load_scene_callback().last_bit(), 0.25);
    discard(&mut session);
}

#[test]
fn a_later_world_error_keeps_the_step_presents_made_before_it() {
    use crate::frame_sector::SectorError;
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part"), sector()])),
        Some(&scene2(&[])),
        None,
    );
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    // The sector's TemporaryPlanes allocation fails in MakeReady, after
    // every frame was read and reported.
    session
        .arena
        .sector_storage_mut()
        .fail_on_allocation(Some(3));
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: None,
    };
    let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
    let MissionSourceOpenError::Open(OpenRunError::Callback(failure)) = error else {
        panic!("unexpected Open failure: {error:?}");
    };
    assert_eq!(failure.step, OpenStep::OpenWorld);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::World(CpuSceneWorldError::Model(ModelError::Sector(
            SectorError::Allocation
        )))
    ));
    // BEGIN, frame 0's STEP, then LoadHierarchy's END (bit 0.5) after the
    // transfer and the root update; MakeReadySectors fails after it.
    assert_eq!(device.presents, [0.0, 0.25f32 * 0.7, 0.5f32 * 0.7]);
    assert_eq!(session.load_scene_callback().last_bit(), 0.5);
    // As without the loading screen, the interrupted sector refuses the
    // Scene destructor's preflight (failed_world_prepare_preserves_...).
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
}

/// A world with `materials` untextured materials before its frames.
fn world_with_materials(materials: usize, frames: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"4DS\0".to_vec();
    bytes.extend(29u16.to_le_bytes());
    bytes.extend([0; 8]);
    bytes.extend(u16::try_from(materials).unwrap().to_le_bytes());
    for _ in 0..materials {
        bytes.extend(0u32.to_le_bytes()); // flags
        for value in [0.5f32, 0.5, 0.5, 0.5, 0.5, 0.5, 0., 0., 0., 1.] {
            bytes.extend(value.to_le_bytes()); // ambient, diffuse, emission, opacity
        }
        bytes.push(0); // no texture
    }
    bytes.extend(u16::try_from(frames.len()).unwrap().to_le_bytes());
    for frame in frames {
        bytes.extend(frame);
    }
    bytes.push(0); // LoadAnimCount
    bytes
}

#[test]
fn a_missing_or_corrupt_world_reports_end_before_the_error() {
    for world_bytes in [None, Some(&b"not a 4DS world"[..])] {
        let fixture = Fixture::new(world_bytes, Some(&scene2(&[])), None);
        write_loading_screen_tables(&fixture);
        let mut session = fixture.session();
        let mut device = RefusingDevice {
            presents: Vec::new(),
            refuse_at: None,
        };
        let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
        let MissionSourceOpenError::Open(OpenRunError::Callback(failure)) = error else {
            panic!("unexpected Open failure: {error:?}");
        };
        assert_eq!(failure.step, OpenStep::OpenWorld);
        assert!(matches!(
            session.failure().unwrap().cause,
            MissionLoadCause::World(CpuSceneWorldError::Model(ModelError::Asset(_)))
        ));
        // BEGIN, then LoadHierarchy's END on the failed exit (the scene's
        // END is a STEP at bit 0.5), then the error.
        assert_eq!(device.presents, [0.0, 0.5f32 * 0.7]);
        assert_eq!(session.load_scene_callback().last_bit(), 0.5);
        assert!(session.world_progress().unwrap().frames().is_empty());
        discard(&mut session);
    }
}

#[test]
fn an_unsupported_frame_reports_end_before_the_error() {
    let mut joint = super::frame_prefix(10, 0, b"joint", 0);
    joint.extend([0; 68]);
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"f0"), joint])),
        Some(&scene2(&[])),
        None,
    );
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: None,
    };
    open_with_progress(&mut session, true, &mut device).unwrap_err();
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::World(CpuSceneWorldError::Model(
            ModelError::UnsupportedFrame { .. }
        ))
    ));
    // The adapter's frame-type pre-pass precedes the frame points, so only
    // BEGIN and the END of the failed exit are shown.
    assert_eq!(device.presents, [0.0, 0.5f32 * 0.7]);
    discard(&mut session);
}

#[test]
fn a_refused_end_after_an_error_is_a_host_stop() {
    let fixture = Fixture::new(None, Some(&scene2(&[])), None);
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: Some(1),
    };
    let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
    assert_world_stop(&session, error);
    assert_eq!(device.presents, [0.0]);
    discard(&mut session);
}

#[test]
fn ls3df_gate_filters_steps_before_cb_load_scene() {
    // 67 frames: frame 0 reports 0.5 (bit 0.25 shown). Frame 4's
    // 2/67 + 0.5 passes LS3DF's gate (lastProgress becomes 0.52985) but
    // cbLoadScene skips it (bit change 0.0149). Frame 6's 3/67 + 0.5 is
    // below 0.52985 + 0.02: LS3DF refuses it, where cbLoadScene alone would
    // show bit 0.27239. Frame 8's 4/67 + 0.5 passes both.
    let frames: Vec<Vec<u8>> = (0..67)
        .map(|index| dummy(0, format!("f{index}").as_bytes()))
        .collect();
    let fixture = Fixture::new(Some(&world(&frames)), Some(&scene2(&[])), None);
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: None,
    };
    assert_eq!(
        open_with_progress(&mut session, true, &mut device),
        Ok(OpenResult::Opened)
    );
    let bar = |frame: usize| (frame / 2) as f32 / 67.0 + 0.5;
    assert_eq!(device.presents[1], bar(0) * 0.5 * 0.7);
    assert_eq!(device.presents[2], bar(8) * 0.5 * 0.7);
    assert_ne!(device.presents[2], bar(6) * 0.5 * 0.7);
    discard(&mut session);
}

#[test]
fn a_refused_material_step_stops_before_that_material_and_disposes() {
    // Materials 0 and 1 report 0 (LS3DF-gated); material 2 reports
    // (2 / 2) / 4 = 0.25, the first STEP: refused before it is read, so no
    // frame, staging model or END follows.
    let fixture = Fixture::new(
        Some(&world_with_materials(4, &[dummy(0, b"f0")])),
        Some(&scene2(&[])),
        None,
    );
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: Some(1),
    };
    let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
    assert_world_stop(&session, error);
    assert_eq!(device.presents, [0.0]);
    assert_eq!(
        session
            .load_save_progress()
            .progress
            .as_ref()
            .unwrap()
            .raw_progress(),
        0.125 * 0.7
    );
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.source_counts(), Some((4, 1)));
    assert_eq!(progress.phase(), CpuSceneWorldPhase::Input);
    assert_eq!(progress.staging_model(), None);
    assert!(progress.frames().is_empty());
    discard(&mut session);
    let counts = session.owned_counts().unwrap();
    assert_eq!(counts.live_frames, 0);
    assert_eq!(counts.visual_textures, 0);
    assert_eq!(counts.visual_allocations, 0);
}

#[test]
fn a_refused_success_end_stops_before_make_ready_sectors() {
    // LoadHierarchy's success END follows the transfer and the root update
    // and precedes I3D_scene::Open's MakeReadySectors: refusing it leaves
    // both frames in the Scene and the sector unprepared.
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part"), sector()])),
        Some(&scene2(&[])),
        None,
    );
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    // Presents: BEGIN, frame 0's STEP, then END (refused).
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: Some(2),
    };
    let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
    assert_world_stop(&session, error);
    assert_eq!(device.presents, [0.0, 0.25f32 * 0.7]);
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.phase(), CpuSceneWorldPhase::Prepare);
    assert_eq!(progress.transferred_frames(), 2);
    assert_eq!(progress.prepared_sectors(), 0);
    discard(&mut session);
}

/// A one-vertex, one-face mesh frame (kind 1, visual 0) using material 1.
fn mesh_frame(parent: u16, name: &[u8]) -> Vec<u8> {
    let mut bytes = vec![1, 0, 0, 0];
    bytes.extend(parent.to_le_bytes());
    for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(0); // culling
    bytes.push(u8::try_from(name.len()).unwrap());
    bytes.extend(name);
    bytes.push(0); // properties
    bytes.extend(0u16.to_le_bytes()); // no instance
    bytes.push(1); // one LOD
    bytes.extend(0f32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    for value in [1f32, 2., 3., 0., 1., 0., 0.25, 0.75] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(1); // one face group
    bytes.extend(1u16.to_le_bytes());
    bytes.extend([0; 6]);
    bytes.extend(1u16.to_le_bytes()); // material 1
    bytes
}

#[test]
fn a_step_stop_after_mesh_frames_leaves_creator_references_that_dispose() {
    // Mesh 0 (root), mesh 1 (child of 0), meshes 2 and 3. Frame 0 reports
    // 0.5 (shown), frame 1 repeats it (gated), frame 2 reports 0.75:
    // refused before frame 2 is read.
    let fixture = Fixture::new(
        Some(&world_with_materials(
            1,
            &[
                mesh_frame(0, b"m0"),
                mesh_frame(1, b"m1"),
                mesh_frame(0, b"m2"),
                mesh_frame(0, b"m3"),
            ],
        )),
        Some(&scene2(&[])),
        None,
    );
    write_loading_screen_tables(&fixture);
    let mut session = fixture.session();
    let mut device = RefusingDevice {
        presents: Vec::new(),
        refuse_at: Some(2),
    };
    let error = open_with_progress(&mut session, true, &mut device).unwrap_err();
    assert_world_stop(&session, error);
    let progress = session.world_progress().unwrap();
    let frames = progress.frames().to_vec();
    assert_eq!(frames.len(), 2);
    assert!(progress.creator_references_pending());
    assert_eq!(progress.transferred_frames(), 0);
    // The stop ran no transfer: mesh 0 is not linked to the primary sector,
    // mesh 1 is linked to mesh 0 as LoadImage read it, nothing is on the
    // staging model or in the Scene, and no resource was admitted.
    let arena = session.arena();
    assert_eq!(arena.links(frames[0]).unwrap().parent, None);
    assert_eq!(arena.links(frames[1]).unwrap().parent, Some(frames[0]));
    let staging = progress.staging_model().unwrap();
    assert!(arena.model(staging).unwrap().frames.is_empty());
    let scene = session.scene().unwrap().unwrap();
    assert!(arena.scene(scene).unwrap().frames().is_empty());
    for &frame in &frames {
        assert_eq!(arena.frame(frame).unwrap().object_resources, None);
        assert_eq!(arena.frame(frame).unwrap().owner, None);
    }
    discard(&mut session);
    let counts = session.owned_counts().unwrap();
    assert_eq!(counts.live_frames, 0);
    assert_eq!(counts.driver_scenes, 0);
    assert_eq!(counts.visual_allocations, 0);
}
