//! Production `Init` prefix over live session owners with fake backends.
//! Pins native order, unavailable-dependency prefixes and the preserved
//! scalar prefix; never runs actor `GameInit`, scripts or rendering.
use super::*;
use crate::game_init_car_tables::{CarTablesOperation, CarTablesOutcome, GameInitCarTablesState};
use crate::mission_load_session::MissionLoadSession;
use crate::native_precision::Pc24PrecisionState;

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-game-init-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(path.join("missions/mise01")).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn session(&self) -> MissionLoadSession {
        let installation = lhr_assets::Installation::open(&self.0).unwrap();
        let mut session =
            MissionLoadSession::new(installation, Pc24PrecisionState::new(false)).unwrap();
        // Open's loading screen reads mafiaIndicators.GetLanguage.
        session
            .install_indicators(crate::indicators::IndicatorsState::constructed(
                crate::indicators::TimerClock {
                    hour: 9,
                    minute: 0,
                    second: 0,
                },
            ))
            .unwrap();
        session
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
    [
        kind.to_le_bytes().as_slice(),
        &u32::try_from(payload.len() + 6).unwrap().to_le_bytes(),
        payload,
    ]
    .concat()
}

fn scene2(body: &[u8]) -> Vec<u8> {
    chunk(0x4c53, &[chunk(1, &[5, 0, 1, 0]), body.to_vec()].concat())
}

fn cache_bytes() -> Vec<u8> {
    fn text(value: &str) -> Vec<u8> {
        [
            (value.len() as u32).to_le_bytes().as_slice(),
            value.as_bytes(),
        ]
        .concat()
    }
    let mut body = text("car.i3d");
    for value in [1f32, 2., 3., 1., 0., 0., 0., 1., 2., 1.] {
        body.extend(value.to_le_bytes());
    }
    body.extend(7u32.to_le_bytes());
    for value in [2f32, 3., 4.] {
        body.extend(value.to_le_bytes());
    }
    let mut part = text("district");
    part.extend([0; 19 * 4]);
    part.extend(chunk(2000, &body));
    let mut root = 1u32.to_le_bytes().to_vec();
    root.extend(chunk(1000, &part));
    chunk(500, &root)
}

fn open_mise01(session: &mut MissionLoadSession) {
    use crate::mission_open_order::{OpenAttempt, OpenResult};
    session.set_open_mission_name("mise01").unwrap();
    session.set_open_configured_volume(0.75).unwrap();
    assert_eq!(OpenAttempt::default().run(session), Ok(OpenResult::Opened));
}

#[test]
fn init_stops_at_first_unowned_step_with_live_prefix_preserved() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    // The ordered Open probe is not yet a native Scene constructor. Init must
    // stop on the missing live owner after the first two scalar stores.
    assert_eq!(session.scene().unwrap(), None);
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    let mut attempt = InitAttempt::default();
    let error = attempt.run(&mut session).unwrap_err();
    match &error {
        InitRunError::Callback(failure) => {
            assert_eq!(failure.step, InitStep::ResetMissionState);
            assert!(
                failure.error.to_string().starts_with("ResetMissionState:"),
                "{}",
                failure.error
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(attempt.last(), Some(InitStep::ResetMissionState));
    // Native stores before the first unowned Scene read are retained. Weather
    // and later sound/AI/UI work must not be silently fabricated.
    assert_eq!(session.game_init_reset_observation(), (false, 0, None, 0),);
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    assert_eq!(
        session.reset_game_mission_state_for_init(),
        Err("mission state reset already entered".into())
    );
    assert_eq!(session.game_init_reset_observation().2, None);
    // Unowned steps never ran: police/traffic stay at cleared-statics
    // defaults (17.0/28.0/valid-scream arrive only with their owners).
    assert!(!session.police_init().active());
    assert_eq!(session.police_init().speed_limit(), 0.0);
    assert_eq!(session.police_init().speed_limit_arrest(), 0.0);
    assert!(!session.traffic_flags().taxi_driver_enabled());
    assert!(!session.traffic_flags().player_in_taxi());
    assert!(!session.traffic_flags().valid_scream());
    // The police shift begin after the stopped span never ran: AIManager
    // keeps its constructor state (zero scalars, GAME 0x538760).
    assert_eq!(
        session.ai_manager(),
        &crate::ai_manager::AiManager::default()
    );
    assert!(session.base_cache().is_open());
    assert_eq!(session.read_base_blocks_for_init(), Ok(true));
    let visited = attempt.last().unwrap();
    assert_eq!(visited, InitStep::ResetMissionState);
    assert_eq!(attempt.run(&mut session), Err(InitRunError::AlreadyEntered));
}

#[test]
fn guard_without_open_prefix_reports_its_step() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let mut attempt = InitAttempt::default();
    let error = attempt.run(&mut session).unwrap_err();
    match &error {
        InitRunError::Callback(failure) => {
            assert_eq!(failure.step, InitStep::GuardNotInitialized);
            assert!(
                failure
                    .error
                    .to_string()
                    .starts_with("GuardNotInitialized:"),
                "{}",
                failure.error
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(attempt.last(), Some(InitStep::GuardNotInitialized));
}

#[test]
fn police_begin_shift_requires_the_open_prefix_and_keeps_the_manager() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    session.ai_manager_mut().groups.stage(Vec::new(), 77, 2);
    assert_eq!(
        session.police_begin_shift_for_init(),
        Err("game init requires completed open prefix".into())
    );
    assert_eq!(session.ai_manager().groups.unlock_time(), 77);
    assert_eq!(session.ai_manager().police.unknown_24(), 0);

    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    assert_eq!(session.execute(InitStep::PoliceBeginShift), Ok(()));
    assert_eq!(session.ai_manager().groups.unlock_time(), u32::MAX);
    assert_eq!(session.ai_manager().police.unknown_24(), 0x0fff_ffff);
    // The standalone Open driver passes no progress argument, so no loading
    // screen exists and `_show_progress_` is the native no-op.
    assert_eq!(session.load_save_progress().progress, None);
    assert_eq!(session.execute(InitStep::PlayerNavi), Ok(()));
    // Open's inline ResetIntern set intern_load (+0x363c) and cleared
    // intern_loaded; the gate reads it and enters the sector rebuild.
    assert!(session.game_state().intern_load());
    assert!(!session.game_state().intern_loaded());
    {
        use crate::game_housekeeping::SectorLightBackend;
        let stale = crate::frame_transform::FrameKey(u32::MAX - 1);
        let sectors = session.vehicle_sectors_mut();
        sectors.add_sector_for_projector_lights(stale);
        sectors.add_sector_for_sounds(stale);
    }
    // This probe Open built no native Scene: both erases ran, then the
    // primary-sector read stops the walk with nothing registered.
    let error = session.execute(InitStep::VehicleSectors).unwrap_err();
    assert_eq!(
        error.0,
        "VehicleSectors: Init cbEnumSectors: no mission Scene for the primary sector"
    );
    assert!(session.vehicle_sectors().light_sectors().is_empty());
    assert!(session.vehicle_sectors().sound_sectors().is_empty());
}

fn seed_player_statics(session: &mut MissionLoadSession) {
    use crate::script_scheduler::FrameReferenceId;
    let statics = session.player_statics_mut();
    statics.set_player_navi_point(Some(FrameReferenceId::from_existing(5)));
    statics.set_can_car_damage(true);
}

#[test]
fn player_navi_without_a_loading_screen_clears_the_player_statics() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    seed_player_statics(&mut session);
    let seeded = session.player_statics();
    // Before the Open prefix the step stops before any store.
    assert_eq!(
        session.player_navi_for_init(),
        Err("game init requires completed open prefix".into())
    );
    assert_eq!(session.player_statics(), seeded);

    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    seed_player_statics(&mut session);
    assert_eq!(session.execute(InitStep::PlayerNavi), Ok(()));
    // SetPlayerNaviPoint(NULL) at 0x5a09c1, then `can_car_damage = false`.
    assert_eq!(session.player_statics().navi_frame(), None);
    assert!(!session.player_statics().can_car_damage());
    assert_eq!(session.load_save_progress().progress, None);
}

#[test]
fn unpresented_player_navi_with_a_loading_screen_stops_at_its_present() {
    use crate::game_loadsave_lifecycle::LoadProgressScreen;
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    let screen = LoadProgressScreen::for_mission_open();
    let original_menu = screen.menu_identity_for_test();
    session.load_save_progress_mut().progress = Some(screen);
    seed_player_statics(&mut session);
    let seeded = session.player_statics();
    let error = session.execute(InitStep::PlayerNavi).unwrap_err();
    // SetProgressGameInit(0.0) runs (an uncreated screen has no bar, the
    // native SetFloat no-op); Present needs the host presentation the
    // unpresented Init has not.
    assert!(
        error.0.starts_with(
            "PlayerNavi: Init _show_progress_(0.0): GM_LoadProgress::Present (GAME 0x5e3d40) needs host presentation"
        ),
        "{error}"
    );
    // The later stores are not reached and the screen stays live.
    assert_eq!(session.player_statics(), seeded);
    let screen = session.load_save_progress().progress.as_ref().unwrap();
    assert_eq!(screen.menu_identity_for_test(), original_menu);
    assert_eq!(screen.title, 0);
    assert_eq!(
        screen.mission_part,
        crate::game_loadsave_lifecycle::MISSION_OPEN_PROGRESS_PART
    );
    assert_eq!(screen.mission_name, None);
    assert!(!screen.uses_native_components().unwrap());
    assert_eq!(
        screen.menu_base_for_test(),
        (None, crate::menu_native_drawing::MENUF_DRAW_PARENT)
    );
    screen
        .with_components_for_test(|menu| {
            assert_eq!(menu, &crate::menu::MenuStack::new());
            Ok(())
        })
        .unwrap();
}

#[test]
fn restart_repeats_prefix_and_unowned_stop() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    for _ in 0..2 {
        let mut session = fixture.session();
        open_mise01(&mut session);
        let mut attempt = InitAttempt::default();
        let error = attempt.run(&mut session).unwrap_err();
        assert!(matches!(
            error,
            InitRunError::Callback(InitFailure {
                step: InitStep::ResetMissionState,
                ..
            })
        ));
        assert!(!session.police_init().active());
        assert!(!session.traffic_flags().valid_scream());
    }
}

#[test]
fn mark_initialized_arms_the_guard_against_rerun() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    session.mark_game_initialized_for_init().unwrap();
    // The guard now reports the already-initialized boundary.
    let err = session.guard_game_init_for_init().unwrap_err();
    assert_eq!(err, "game already initialized");
    assert_eq!(
        session.reset_game_mission_state_for_init(),
        Err("game already initialized".into())
    );
}

fn segments_test_session(fixture: &Fixture) -> MissionLoadSession {
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    session
}

fn segments_test_header(count: u32) -> Vec<u8> {
    let mut bytes = b"AISEG-V1.1-JIPO\0".to_vec();
    bytes.extend(count.to_le_bytes());
    bytes
}

fn segments_test_record(position: [u32; 3], indices: &[u32], data: [u8; 16]) -> Vec<u8> {
    let mut bytes = 4u32.to_le_bytes().to_vec();
    bytes.extend(0x9876_5432u32.to_le_bytes());
    for bits in position {
        bytes.extend(bits.to_le_bytes());
    }
    bytes.extend(u32::try_from(indices.len()).unwrap().to_le_bytes());
    for index in indices {
        bytes.extend(index.to_le_bytes());
    }
    bytes.extend(data);
    bytes
}

#[test]
fn segments_init_uses_the_live_ai_owner_and_terminal_cleanup_clears_it() {
    use crate::game_ai_segments::SegmentsLoadState;
    let fixture = Fixture::new();
    let position = [0x8000_0000, 0x7fc0_1234, 0x3f80_0000];
    let indices = [0, u32::MAX, 7];
    let data = [0xa5; 16];
    let mut bytes = segments_test_header(1);
    bytes.extend(segments_test_record(position, &indices, data));
    fixture.write("missions/mise01/clusters.seg", &bytes);
    let mut session = segments_test_session(&fixture);
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Cleared
    );
    assert_eq!(session.segments_load_for_init(), Ok(true));
    let owner = &session.ai_manager().segments;
    assert_eq!(owner.state(), SegmentsLoadState::Loaded);
    assert_eq!(owner.declared_segment_count(), Some(1));
    assert_eq!(owner.num_segments(), 1);
    let segment = owner.get_segment(0).unwrap();
    assert_eq!(segment.flags(), 4);
    assert_eq!(segment.unknown_word(), Some(0x9876_5432));
    assert_eq!(segment.position_bits(), Some(position));
    assert_eq!(segment.indices(), indices);
    assert_eq!(segment.data().unwrap().bytes(), data);
    let counts = session.owned_counts().unwrap();
    assert_eq!(counts.ai_segment_clusters, 1);
    assert_eq!(counts.ai_segments, 1);
    assert_eq!(counts.ai_pending_segments, 0);
    session.ai_manager_mut().game_init_police_begin_shift();
    assert_eq!(
        session
            .ai_manager()
            .segments
            .get_segment(0)
            .unwrap()
            .indices(),
        indices
    );
    session.discard_terminal().unwrap();
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Cleared
    );
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}

#[test]
fn segments_rejected_header_is_false_but_not_an_init_error() {
    use crate::game_ai_segments::SegmentsLoadState;
    let fixture = Fixture::new();
    fixture.write("missions/mise01/clusters.seg", &[0x5a; 16]);
    let mut session = segments_test_session(&fixture);
    assert_eq!(session.segments_load_for_init(), Ok(false));
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::RejectedHeader
    );
    assert_eq!(session.owned_counts().unwrap().ai_segment_clusters, 1);
    session.discard_terminal().unwrap();
    assert_eq!(session.owned_counts().unwrap(), Default::default());

    let mut session = segments_test_session(&fixture);
    assert_eq!(session.execute(InitStep::SegmentsLoad), Ok(()));
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::RejectedHeader
    );
    assert!(session
        .execute(InitStep::CarTables)
        .unwrap_err()
        .0
        .starts_with("CarTables:"));
    session.discard_terminal().unwrap();
}

#[test]
fn segments_missing_file_preserves_initialized_cluster_and_refuses_replay() {
    use crate::game_ai_segments::{SegmentsLoadStage, SegmentsLoadState};
    let fixture = Fixture::new();
    let mut session = segments_test_session(&fixture);
    let error = session.execute(InitStep::SegmentsLoad).unwrap_err();
    assert!(
        error
            .0
            .starts_with("SegmentsLoad: AIManager.segments.Load:"),
        "{error}"
    );
    assert!(
        error
            .0
            .contains(r#"MissingOriginalAsset("Missions\\mise01\\clusters.seg")"#),
        "{error}"
    );
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Interrupted(SegmentsLoadStage::Open)
    );
    let counts = session.owned_counts().unwrap();
    assert_eq!(
        (
            counts.ai_segment_clusters,
            counts.ai_segments,
            counts.ai_pending_segments
        ),
        (1, 0, 0)
    );
    fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
    assert_eq!(
        session.segments_load_for_init(),
        Err("AIManager.segments.Load already entered; no replay".into())
    );
    assert_eq!(session.owned_counts().unwrap(), counts);
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Interrupted(SegmentsLoadStage::Open)
    );
    session.discard_terminal().unwrap();
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}

#[test]
fn segments_short_record_keeps_installed_and_pending_prefix_until_cleanup() {
    use crate::game_ai_segments::{SegmentsLoadStage, SegmentsLoadState};
    let fixture = Fixture::new();
    let mut bytes = segments_test_header(2);
    bytes.extend(segments_test_record([1, 2, 3], &[4, 9], [0x17; 16]));
    bytes.extend(4u32.to_le_bytes());
    bytes.extend(0xabcd_1234u32.to_le_bytes());
    bytes.extend(0x9988_7766u32.to_le_bytes());
    fixture.write("missions/mise01/clusters.seg", &bytes);
    let mut session = segments_test_session(&fixture);
    let error = session.execute(InitStep::SegmentsLoad).unwrap_err();
    assert!(
        error
            .0
            .contains("ShortNativeRead { requested: 12, received: 4 }"),
        "{error}"
    );
    let owner = &session.ai_manager().segments;
    assert_eq!(
        owner.state(),
        SegmentsLoadState::Interrupted(SegmentsLoadStage::Position { segment: 1 })
    );
    assert_eq!(owner.declared_segment_count(), Some(2));
    assert_eq!(owner.get_segment(0).unwrap().indices(), [4, 9]);
    assert_eq!(
        owner.pending_segment().unwrap().unknown_word(),
        Some(0xabcd_1234)
    );
    assert_eq!(owner.pending_segment().unwrap().position_bits(), None);
    let counts = session.owned_counts().unwrap();
    assert_eq!(
        (
            counts.ai_segment_clusters,
            counts.ai_segments,
            counts.ai_pending_segments
        ),
        (1, 1, 1)
    );
    session.discard_terminal().unwrap();
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Cleared
    );
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}

#[test]
fn segments_completed_load_is_not_replaced_by_another_init_attempt() {
    let fixture = Fixture::new();
    let mut bytes = segments_test_header(1);
    bytes.extend(segments_test_record([5, 6, 7], &[11], [0x29; 16]));
    fixture.write("missions/mise01/clusters.seg", &bytes);
    let mut session = segments_test_session(&fixture);
    assert_eq!(session.execute(InitStep::SegmentsLoad), Ok(()));
    let counts = session.owned_counts().unwrap();
    fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
    assert!(session
        .execute(InitStep::SegmentsLoad)
        .unwrap_err()
        .0
        .contains("already entered; no replay"));
    assert_eq!(
        session
            .ai_manager()
            .segments
            .get_segment(0)
            .unwrap()
            .indices(),
        [11]
    );
    assert_eq!(session.owned_counts().unwrap(), counts);
    session.discard_terminal().unwrap();
}

#[test]
fn segments_admission_failure_does_not_consume_the_load_attempt() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
    let mut session = fixture.session();
    assert_eq!(
        session.segments_load_for_init(),
        Err("game init requires completed open prefix".into())
    );
    assert!(session.ai_manager().segments.clusters().is_empty());
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    open_mise01(&mut session);
    assert_eq!(session.segments_load_for_init(), Ok(true));
    session.discard_terminal().unwrap();
    let progress = *session.discard_progress();
    assert!(session.segments_load_for_init().is_err());
    assert!(session.ai_manager().segments.clusters().is_empty());
    assert_eq!(session.discard_progress(), &progress);
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}

struct CarTablesTestDevice;

impl crate::scene_projection::SceneRenderBackend for CarTablesTestDevice {
    fn render_scene(
        &mut self,
        arena: &crate::model_loading::ModelArena,
        scene: crate::scene_ownership::SceneKey,
    ) -> Result<u32, crate::model_loading::ModelError> {
        assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
        Ok(crate::scene_projection::NOT_INITIALIZED)
    }
}

impl crate::load_scene_callback::LoadingScreenDevice for CarTablesTestDevice {
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

fn open_mise01_for_car_tables(fixture: &Fixture, session: &mut MissionLoadSession) {
    use crate::game_camera_projection::GameCameraProjection;
    use crate::host_scene_adapters::InspectionScreen;
    use crate::mission_open_order::{OpenAttempt, OpenResult};

    let mut world = b"4DS\0".to_vec();
    world.extend(29u16.to_le_bytes());
    world.extend([0; 8]);
    world.extend(0u16.to_le_bytes());
    world.extend(0u16.to_le_bytes());
    world.push(0);
    fixture.write("missions/mise01/scene.4ds", &world);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    assert_eq!(
        session.open_source_in_order(
            &mut OpenAttempt::default(),
            "mise01",
            false,
            &[],
            GameCameraProjection {
                global_mode: 0,
                screen_flag_46f4: false,
            },
            &mut InspectionScreen {
                size: [800, 600],
                viewport: [0, 0, 800, 600],
            },
            &mut CarTablesTestDevice,
        ),
        Ok(OpenResult::Opened),
        "{:?}",
        session.failure()
    );
    assert!(session.game_state().intern_load());
    assert_car_table_allocations(session, 0);
}

fn assert_car_table_allocations(session: &MissionLoadSession, expected: usize) {
    let counts = session.owned_counts().unwrap();
    assert_eq!(counts.mission_car_heap_allocations, expected);
    assert_eq!(counts.mission_car_table_allocations, expected);
}

fn assert_car_tables(
    session: &MissionLoadSession,
    state: GameInitCarTablesState,
    loader_returns: [Option<bool>; 3],
) {
    assert_eq!(session.init_car_tables().state(), state);
    assert_eq!(session.init_car_tables().loader_returns(), loader_returns);
}

fn assert_car_tables_no_replay(
    session: &mut MissionLoadSession,
    state: GameInitCarTablesState,
    loader_returns: [Option<bool>; 3],
) {
    let counts = session.owned_counts().unwrap();
    let error = session.execute(InitStep::CarTables).unwrap_err();
    assert!(error.0.starts_with("CarTables:"), "{error}");
    assert!(error.0.contains("no replay"), "{error}");
    assert_car_tables(session, state, loader_returns);
    assert_eq!(session.owned_counts().unwrap(), counts);
}

const EMPTY_TYRE_FILE: [u8; 8] = [1, 0, 0, 0, 0, 0, 0, 0];

fn prepare_startup_indicators(fixture: &Fixture, session: &mut MissionLoadSession) {
    fixture.write("tables/version.txt", b"en");
    let mut language = vec![0; 56 + 256 * 2];
    language[..4].copy_from_slice(b"Lang");
    language[4..6].copy_from_slice(b"en");
    fixture.write("tables/lang_en.def", &language);
    session.select_loading_menu_startup_language().unwrap();
    session.create_loading_indicator_assets().unwrap();
}

#[test]
fn startup_definition_intake_refuses_an_already_consumed_navigation_step() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    prepare_startup_indicators(&fixture, &mut session);
    session.load_navigation_for_open("mise01").unwrap();
    assert!(session
        .read_definition_tables_for_startup()
        .unwrap_err()
        .contains("SessionNotFresh"));
    session.discard_terminal().unwrap();
}

#[test]
fn standalone_open_without_a_scene_cannot_return_to_startup_definitions() {
    use crate::mission_open_order::{OpenAttempt, OpenStep};

    let fixture = Fixture::new();
    let mut session = fixture.session();
    prepare_startup_indicators(&fixture, &mut session);
    session.set_open_mission_name("mise01").unwrap();
    let mut attempt = OpenAttempt::default();
    assert!(attempt.run(&mut session).is_err());
    assert_eq!(attempt.last(), Some(OpenStep::OpenWorld));
    assert!(session.game_state().intern_load());
    assert!(session
        .read_definition_tables_for_startup()
        .unwrap_err()
        .contains("SessionNotFresh"));
    session.discard_terminal().unwrap();
}

#[test]
fn car_tables_use_definitions_retained_before_open_and_stop_at_particles() {
    use crate::game_definition_tables::DefinitionKind;

    for missing in [None, Some(DefinitionKind::Voices)] {
        let fixture = Fixture::new();
        fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
        for kind in DefinitionKind::ORDER {
            if Some(kind) != missing {
                fixture.write(kind.path(), &vec![0; kind.stride() + 1]);
            }
        }
        let mut session = fixture.session();
        let early = session.read_definition_tables_for_startup().unwrap_err();
        assert!(early.contains("indicator asset prefix"), "{early}");
        assert_eq!(
            session.startup_definition_count(DefinitionKind::Particles),
            None
        );
        prepare_startup_indicators(&fixture, &mut session);
        assert_eq!(
            session.read_definition_tables_for_startup(),
            Ok(missing.is_none())
        );
        fixture.write(
            DefinitionKind::Materials.path(),
            &vec![0; 257 * DefinitionKind::Materials.stride()],
        );
        fixture.write(DefinitionKind::Particles.path(), &[]);
        assert_eq!(
            session.startup_definition_count(DefinitionKind::Particles),
            Some(1)
        );
        assert!(session
            .read_definition_tables_for_startup()
            .unwrap_err()
            .contains("no replay"));
        open_mise01_for_car_tables(&fixture, &mut session);
        session.segments_load_for_init().unwrap();
        let error = session.execute(InitStep::CarTables).unwrap_err();
        assert!(error.0.starts_with("CarTables: LoadParticles:"), "{error}");
        assert!(error.0.contains("definition_count: 1"), "{error}");
        let stopped = GameInitCarTablesState::Interrupted(CarTablesOperation::LoadParticles);
        let returns = [Some(false), Some(true), None];
        assert_car_tables(&session, stopped, returns);
        assert_car_table_allocations(&session, 3);
        assert_car_tables_no_replay(&mut session, stopped, returns);
        session.discard_terminal().unwrap();
        assert_eq!(session.owned_counts().unwrap(), Default::default());
    }
}

#[test]
fn returned_zero_particle_count_completes_car_tables_without_reopening_definitions() {
    use crate::game_definition_tables::DefinitionKind;

    for missing in [
        None,
        Some(DefinitionKind::Particles),
        Some(DefinitionKind::Materials),
    ] {
        let fixture = Fixture::new();
        fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
        for kind in DefinitionKind::ORDER {
            if Some(kind) != missing {
                let size = if kind == DefinitionKind::Particles {
                    kind.stride() - 1
                } else {
                    kind.stride()
                };
                fixture.write(kind.path(), &vec![0x55; size]);
            }
        }
        let mut session = fixture.session();
        prepare_startup_indicators(&fixture, &mut session);
        assert_eq!(
            session.read_definition_tables_for_startup(),
            Ok(missing.is_none())
        );
        fixture.write(
            DefinitionKind::Particles.path(),
            &vec![0; DefinitionKind::Particles.stride()],
        );
        open_mise01_for_car_tables(&fixture, &mut session);
        assert_eq!(session.segments_load_for_init(), Ok(true));
        assert_eq!(
            session.startup_definition_count(DefinitionKind::Particles),
            Some(0)
        );
        assert_eq!(session.execute(InitStep::CarTables), Ok(()));
        let state = GameInitCarTablesState::Complete(CarTablesOutcome::Loaded {
            tyres: false,
            materials: true,
            particles: true,
        });
        let returns = [Some(false), Some(true), Some(true)];
        assert_car_tables(&session, state, returns);
        assert_car_table_allocations(
            &session,
            if missing == Some(DefinitionKind::Materials) {
                3
            } else {
                4
            },
        );
        assert_car_tables_no_replay(&mut session, state, returns);
        let error = session.execute(InitStep::HydrantsAndWheels).unwrap_err();
        assert!(error.0.starts_with("HydrantsAndWheels:"), "{error}");
        session.discard_terminal().unwrap();
        assert_eq!(session.owned_counts().unwrap(), Default::default());
    }
}

#[test]
fn interrupted_startup_definitions_are_not_admitted_to_car_tables() {
    use crate::game_definition_tables::DefinitionKind;

    let fixture = Fixture::new();
    fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
    for kind in [DefinitionKind::Items, DefinitionKind::Materials] {
        fixture.write(kind.path(), &vec![0; kind.stride()]);
    }
    std::fs::create_dir(fixture.0.join(DefinitionKind::Particles.path())).unwrap();
    let mut session = fixture.session();
    prepare_startup_indicators(&fixture, &mut session);
    assert!(session.read_definition_tables_for_startup().is_err());
    assert_eq!(
        session.startup_definition_count(DefinitionKind::Particles),
        None
    );
    assert!(session
        .read_definition_tables_for_startup()
        .unwrap_err()
        .contains("no replay"));
    open_mise01_for_car_tables(&fixture, &mut session);
    session.segments_load_for_init().unwrap();
    assert_car_tables_stops_after_tyres(&mut session, false);
}

#[test]
fn car_tables_cannot_start_until_segments_returns() {
    use crate::game_ai_segments::{SegmentsLoadStage, SegmentsLoadState};

    let fixture = Fixture::new();
    fixture.write("tables/tyres.bin", &EMPTY_TYRE_FILE);
    let mut session = fixture.session();
    let counts = session.owned_counts().unwrap();
    let error = session.execute(InitStep::CarTables).unwrap_err();
    assert!(error.0.starts_with("CarTables:"), "{error}");
    assert!(error.0.contains("completed open prefix"), "{error}");
    assert_car_tables(&session, GameInitCarTablesState::Ready, [None; 3]);
    assert_eq!(session.owned_counts().unwrap(), counts);

    prepare_startup_indicators(&fixture, &mut session);
    assert_eq!(session.read_definition_tables_for_startup(), Ok(false));
    open_mise01_for_car_tables(&fixture, &mut session);
    let counts = session.owned_counts().unwrap();
    let error = session.car_tables_for_init().unwrap_err();
    assert!(error.contains("SegmentsLoad"), "{error}");
    assert_car_tables(&session, GameInitCarTablesState::Ready, [None; 3]);
    assert_eq!(session.owned_counts().unwrap(), counts);

    let error = session.execute(InitStep::SegmentsLoad).unwrap_err();
    assert!(error.0.starts_with("SegmentsLoad:"), "{error}");
    assert!(error.0.contains("MissingOriginalAsset"), "{error}");
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Interrupted(SegmentsLoadStage::Open)
    );
    let counts = session.owned_counts().unwrap();
    let error = session.execute(InitStep::CarTables).unwrap_err();
    assert!(error.0.starts_with("CarTables:"), "{error}");
    assert!(error.0.contains("SegmentsLoad"), "{error}");
    assert_car_tables(&session, GameInitCarTablesState::Ready, [None; 3]);
    assert_eq!(session.owned_counts().unwrap(), counts);
    session.discard_terminal().unwrap();
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}

fn assert_car_tables_stops_after_tyres(session: &mut MissionLoadSession, tyres: bool) {
    let error = session.execute(InitStep::CarTables).unwrap_err();
    assert!(error.0.starts_with("CarTables: LoadMaterials:"), "{error}");
    let state = GameInitCarTablesState::Interrupted(CarTablesOperation::LoadMaterials);
    let returns = [Some(tyres), None, None];
    assert_car_tables(session, state, returns);
    assert_car_table_allocations(session, 1);
    assert_car_tables_no_replay(session, state, returns);
    session.discard_terminal().unwrap();
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}

#[test]
fn car_tables_reaches_materials_after_missing_tyres_returns_false() {
    use crate::game_ai_segments::SegmentsLoadState;

    let fixture = Fixture::new();
    fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
    let mut session = fixture.session();
    open_mise01_for_car_tables(&fixture, &mut session);
    assert_eq!(session.segments_load_for_init(), Ok(true));
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Loaded
    );
    assert_car_tables_stops_after_tyres(&mut session, false);
}

#[test]
fn car_tables_reaches_materials_with_empty_tyres_after_rejected_segments_header() {
    use crate::game_ai_segments::SegmentsLoadState;

    let fixture = Fixture::new();
    fixture.write("missions/mise01/clusters.seg", &[0x5a; 16]);
    fixture.write("tables/tyres.bin", &EMPTY_TYRE_FILE);
    let mut session = fixture.session();
    open_mise01_for_car_tables(&fixture, &mut session);
    assert_eq!(session.execute(InitStep::SegmentsLoad), Ok(()));
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::RejectedHeader
    );
    assert_car_tables_stops_after_tyres(&mut session, true);
}

#[test]
fn car_tables_truncated_tyres_keep_reached_allocations_without_a_return() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
    let mut tyres = Vec::new();
    for word in [1u32, 1, 0] {
        tyres.extend(word.to_le_bytes());
    }
    tyres.extend([0; 32]);
    for word in [0u32, 1, 0, 72] {
        tyres.extend(word.to_le_bytes());
    }
    tyres.extend([0xaa, 0xbb]);
    fixture.write("tables/tyres.bin", &tyres);
    let mut session = fixture.session();
    open_mise01_for_car_tables(&fixture, &mut session);
    assert_eq!(session.execute(InitStep::SegmentsLoad), Ok(()));
    let error = session.execute(InitStep::CarTables).unwrap_err();
    assert!(error.0.starts_with("CarTables: LoadTyres:"), "{error}");
    assert!(
        error
            .0
            .contains("ShortRead { field: Payload, expected: 72, actual: 2 }"),
        "{error}"
    );
    let state = GameInitCarTablesState::Interrupted(CarTablesOperation::LoadTyres);
    assert_car_tables(&session, state, [None; 3]);
    assert_car_table_allocations(&session, 3);

    fixture.write("tables/tyres.bin", &EMPTY_TYRE_FILE);
    assert_car_tables_no_replay(&mut session, state, [None; 3]);
    session.discard_terminal().unwrap();
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}

#[test]
fn car_tables_skips_with_intern_load_clear_after_segments_returns() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/clusters.seg", &segments_test_header(0));
    let mut session = fixture.session();
    session.set_open_mission_name("mise01").unwrap();
    assert!(session.open_bin_probe_for_open("mise01").unwrap());
    session.mark_open_mission_opened().unwrap();
    assert!(!session.game_state().intern_load());

    assert_eq!(session.segments_load_for_init(), Ok(true));
    let counts = session.owned_counts().unwrap();
    assert_eq!(session.car_tables_for_init(), Ok(CarTablesOutcome::Skipped));
    let state = GameInitCarTablesState::Complete(CarTablesOutcome::Skipped);
    assert_car_tables(&session, state, [None; 3]);
    assert_car_table_allocations(&session, 0);
    assert_eq!(session.owned_counts().unwrap(), counts);
    assert_car_tables_no_replay(&mut session, state, [None; 3]);
    session.discard_terminal().unwrap();
    assert_eq!(session.owned_counts().unwrap(), Default::default());
}
