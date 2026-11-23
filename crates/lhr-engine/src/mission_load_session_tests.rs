//! Synthetic composition tests over actual scene, actor and source owners.
use super::*;
#[path = "mission_freeride_tests.rs"]
mod freeride;
#[path = "mission_init_presentation_tests.rs"]
mod init_presentation;
#[path = "session_actor_program_tests.rs"]
mod owned_actor_runtime;
#[path = "session_program_runtime_tests.rs"]
mod owned_runtime;
#[path = "session_source_open_tests.rs"]
mod owned_source_open;
#[path = "mission_program_disposal_tests.rs"]
mod program_disposal;
#[path = "mission_load_session_program_tests.rs"]
mod program_world;
use crate::frame_transform::FrameKey;
use crate::mission_open_order::OpenStep;
use crate::mission_resources::ModelCachePhase;
use crate::model_loading::{CpuSceneWorldPhase, ModelKey};
use crate::scene_resources::SceneScreen;

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(world: Option<&[u8]>, scene2: Option<&[u8]>, model: Option<&[u8]>) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-mission-load-session-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(path.join("missions/synthetic")).unwrap();
        std::fs::create_dir_all(path.join("models")).unwrap();
        std::fs::create_dir_all(path.join("Sounds")).unwrap();
        for (name, data) in [
            ("missions/synthetic/scene.4ds", world),
            ("missions/synthetic/scene2.bin", scene2),
            ("models/fixture.4ds", model),
        ] {
            if let Some(data) = data {
                std::fs::write(path.join(name), data).unwrap();
            }
        }
        Self(path)
    }
    fn session(&self) -> MissionLoadSession {
        let mut session = self.session_without_hud();
        session.install_indicators(fixture_hud()).unwrap();
        session
    }
    fn session_without_hud(&self) -> MissionLoadSession {
        MissionLoadSession::new(
            lhr_assets::Installation::open(&self.0).unwrap(),
            Pc24PrecisionState::new(false),
        )
        .unwrap()
    }
}
/// The `mafiaIndicators` global a session's Open needs (the loading
/// screen's `CreateCompsByDef` reads `GetLanguage`); natively it exists
/// before any mission.
/// A `loadprogress` menu definition with the window, the mission picture
/// (0x65) and name (0xc8) items and the bar (0x12c),
/// as `tables\menu\loadprogress.mnu` stores it.
pub(crate) fn loadprogress_menu_bytes() -> Vec<u8> {
    let mut menu = b"Menu".to_vec();
    for word in [1u32, 4, 0] {
        menu.extend(word.to_le_bytes());
    }
    for (id, kind) in [
        (1u32, *b"dniw"),
        (0x65, *b"ctip"),
        (0xc8, *b"txet"),
        (crate::menu::LOADPROGRESS_BAR_ID, *b"rabp"),
    ] {
        menu.extend(id.to_le_bytes());
        menu.extend(kind);
        for value in [0f32, 0., 100., 20.] {
            menu.extend(value.to_le_bytes());
        }
        menu.extend(0u32.to_le_bytes());
        menu.extend(0u32.to_le_bytes());
    }
    menu
}

pub(crate) fn fixture_hud() -> crate::indicators::IndicatorsState {
    crate::indicators::IndicatorsState::constructed(crate::indicators::TimerClock {
        hour: 9,
        minute: 0,
        second: 0,
    })
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[derive(Default)]
struct Screen {
    fail_size: bool,
    calls: usize,
}
impl SceneScreen for Screen {
    type Error = &'static str;
    fn screen_sy(&mut self) -> Result<i32, Self::Error> {
        self.calls += 1;
        if self.fail_size {
            Err("synthetic size failure")
        } else {
            Ok(600)
        }
    }
    fn screen_sx(&mut self) -> Result<i32, Self::Error> {
        self.calls += 1;
        Ok(800)
    }
}
impl DriverSceneScreen for Screen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error> {
        self.calls += 1;
        Ok([0, 0, 800, 600])
    }
}

fn projection() -> GameCameraProjection {
    GameCameraProjection {
        global_mode: 0,
        screen_flag_46f4: false,
    }
}

/// Test view of one ordered source Open (the only source-loading entry):
/// `Ok` is reserved for `Opened`; a stop keeps its detail in `failure()`.
#[derive(Debug, PartialEq, Eq)]
enum LoadStop {
    Admission(MissionLoadCallError),
    /// A native early return (`NoMedia`/`OpenBinFailed`), not a host error.
    NotOpened(crate::mission_open_order::OpenResult),
    /// Startup/cache/sound/Scene construction stopped before the driver.
    Bootstrap,
    /// The ordered driver's callback stopped at this boundary.
    Step(crate::mission_open_order::OpenStep),
    SessionNotFresh,
    MissionAlreadyOpened,
    InvalidMissionName,
    AttemptReplayed,
}

/// Camera-less headless Render: native I3D_driver::Render (LS3DF
/// 0x10062030) without a current camera clears the device and returns
/// I3DERR_GENERIC; there is no device here to clear.
struct NoCameraRender;
impl crate::scene_projection::SceneRenderBackend for NoCameraRender {
    fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
        assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
        Ok(0x8000_0005)
    }
}

/// The test device takes the loading screen's device calls; nothing draws.
impl crate::load_scene_callback::LoadingScreenDevice for NoCameraRender {
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
fn frame_prefix(kind: u8, parent: u16, name: &[u8], culling: u8) -> Vec<u8> {
    let mut bytes = vec![kind];
    bytes.extend(parent.to_le_bytes());
    for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(culling);
    bytes.push(u8::try_from(name.len()).unwrap());
    bytes.extend(name);
    bytes.push(0); // properties
    bytes
}
fn dummy(parent: u16, name: &[u8]) -> Vec<u8> {
    dummy_with_culling(parent, name, 0)
}
fn dummy_with_culling(parent: u16, name: &[u8], culling: u8) -> Vec<u8> {
    let mut bytes = frame_prefix(6, parent, name, culling);
    bytes.extend([0; 24]);
    bytes
}
fn sector() -> Vec<u8> {
    let mut bytes = frame_prefix(5, 0, b"synthetic sector", 0);
    bytes.extend(0x801u32.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(3u32.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    for value in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
        bytes.extend(value.to_le_bytes());
    }
    for index in [0u16, 1, 2] {
        bytes.extend(index.to_le_bytes());
    }
    for value in [0f32, 0., 0., 1., 1., 0.] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(0); // portals
    bytes
}
fn world(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"4DS\0".to_vec();
    bytes.extend(29u16.to_le_bytes());
    bytes.extend([0; 8]);
    bytes.extend(0u16.to_le_bytes()); // materials
    bytes.extend(u16::try_from(frames.len()).unwrap().to_le_bytes());
    for frame in frames {
        bytes.extend(frame);
    }
    bytes.push(0); // LoadAnimCount
    bytes
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
fn object(kind: u32, name: &[u8], fields: &[u8]) -> Vec<u8> {
    let mut name = name.to_vec();
    name.push(0);
    chunk(
        0x4010,
        &[
            chunk(0x4011, &kind.to_le_bytes()),
            chunk(0x10, &name),
            fields.to_vec(),
        ]
        .concat(),
    )
}
fn model_object() -> Vec<u8> {
    object(9, b"instance", &chunk(0x2012, b"fixture.4ds\0"))
}
fn sound_object(name: &[u8]) -> Vec<u8> {
    object(4, b"source sound", &chunk(0x4060, &chunk(0x10, name)))
}
fn pcm_wav() -> Vec<u8> {
    let samples = [0i16, 1024, -1024, 0];
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(44u32.to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(22_050u32.to_le_bytes());
    bytes.extend(44_100u32.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(8u32.to_le_bytes());
    for sample in samples {
        bytes.extend(sample.to_le_bytes());
    }
    bytes
}
fn load(session: &mut MissionLoadSession, directories: &[&[u8]]) -> Result<(), LoadStop> {
    load_with(session, directories, &mut Screen::default())
}
fn load_with(
    session: &mut MissionLoadSession,
    directories: &[&[u8]],
    screen: &mut Screen,
) -> Result<(), LoadStop> {
    load_with_progress(session, directories, screen, false)
}
/// Open with the native `progress` argument, as Main.cpp's mission starts.
fn load_with_progress(
    session: &mut MissionLoadSession,
    directories: &[&[u8]],
    screen: &mut Screen,
    progress: bool,
) -> Result<(), LoadStop> {
    use crate::mission_open_order::{OpenResult, OpenRunError};
    match session.open_source_in_order(
        &mut crate::mission_open_order::OpenAttempt::default(),
        "synthetic",
        progress,
        directories,
        projection(),
        screen,
        &mut NoCameraRender,
    ) {
        Ok(OpenResult::Opened) => Ok(()),
        Ok(outcome) => Err(LoadStop::NotOpened(outcome)),
        Err(MissionSourceOpenError::Admission(error)) => Err(LoadStop::Admission(error)),
        Err(MissionSourceOpenError::BootstrapStopped) => Err(LoadStop::Bootstrap),
        Err(MissionSourceOpenError::Open(OpenRunError::Callback(failure))) => {
            Err(LoadStop::Step(failure.step))
        }
        Err(MissionSourceOpenError::Open(OpenRunError::AlreadyEntered)) => {
            Err(LoadStop::AttemptReplayed)
        }
        Err(MissionSourceOpenError::SessionNotFresh) => Err(LoadStop::SessionNotFresh),
        Err(MissionSourceOpenError::MissionAlreadyOpened) => Err(LoadStop::MissionAlreadyOpened),
        Err(MissionSourceOpenError::InvalidMissionName(_)) => Err(LoadStop::InvalidMissionName),
    }
}
/// Ordered Open sets native `restore_volume` before OpenBin. Consume it with
/// the first Tick head (skip_frames 0) so a later Init boundary's own write
/// of the flag is observable in isolation.
fn consume_open_restore(session: &mut MissionLoadSession) {
    assert!(session.open_restore_volume());
    assert_eq!(session.tick_restore_volume_head(), Ok(true));
    assert!(!session.open_restore_volume());
}
fn discard(session: &mut MissionLoadSession) {
    let result = session.discard_terminal();
    assert!(
        result.is_ok(),
        "{:?}; {:?}",
        session.discard_failure(),
        session.owned_counts().unwrap()
    );
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::Complete
    );
    assert_eq!(
        session.owned_counts().unwrap(),
        MissionOwnedCounts::default()
    );
    let close = session.scene_close_report().unwrap();
    assert!(close.native_managed_residency_absent);
    assert!(!close.native_device_virtual14_executed);
    assert!(close.canonical_managed_unload_executed);
}

#[test]
fn missing_world_preserves_real_scene_and_first_error_without_reentry() {
    let fixture = Fixture::new(None, Some(&scene2(&[])), None);
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenWorld))
    );
    assert_eq!(session.phase(), MissionLoadPhase::World);
    assert_eq!(
        session.world_progress().unwrap().phase(),
        CpuSceneWorldPhase::Input
    );
    let scene = session.scene().unwrap().unwrap();
    assert_eq!(session.driver().scenes(), &[scene]);
    assert!(session.arena().scene(scene).unwrap().frames().is_empty());
    assert!(session.loader().is_none());
    assert_eq!(session.scene2_byte_count(), None);
    let error = format!("{:?}", session.failure().unwrap());
    let before = session.owned_counts().unwrap();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Admission(MissionLoadCallError::AlreadyEntered))
    );
    assert_eq!(session.owned_counts().unwrap(), before);
    assert_eq!(format!("{:?}", session.failure().unwrap()), error);
    discard(&mut session);
    assert!(session.arena().scene(scene).is_err());
    assert_eq!(format!("{:?}", session.failure().unwrap()), error);
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Admission(
            MissionLoadCallError::TerminalDiscardEntered
        ))
    );
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::AlreadyEntered)
    );
}

#[test]
fn missing_scene2_retains_world_transfer_without_map_or_camera_creation() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), None, None);
    let mut session = fixture.session();
    // Absent scene2.bin is the native nonzero OpenBin (OPENBIN_NOFILE)
    // return before any map or camera setup; no host error is retained.
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::NotOpened(
            crate::mission_open_order::OpenResult::OpenBinFailed
        ))
    );
    assert!(session.failure().is_none());
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.phase(), CpuSceneWorldPhase::Complete);
    assert_eq!(progress.staging_model(), None);
    assert_eq!(progress.transferred_frames(), 1);
    let frame = progress.frames()[0];
    // C_mission::OpenBin (GAME 0x541030) opens scene2.bin before any map or
    // camera setup, so its absence leaves neither.
    assert!(session.load_map().is_none());
    assert!(session.loader().is_none());
    let scene = session.scene().unwrap().unwrap();
    assert_eq!(session.arena().scene(scene).unwrap().frames(), &[frame]);
    assert_eq!(session.arena().scene_active_camera(scene).unwrap(), None);
    discard(&mut session);
    assert!(session.arena().frame(frame).is_err());
}

#[test]
fn malformed_actor_root_is_the_exact_frontier_and_does_not_run_later_objects() {
    let bytes = scene2(
        &[
            chunk(0xae20, &[1, 2, 3, 4]),
            chunk(0x4000, &object(9, b"must not run", &[])),
        ]
        .concat(),
    );
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenBin))
    );
    assert_eq!(session.phase(), MissionLoadPhase::Scene2Prefix);
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("wrong frontier: {:?}", session.failure());
    };
    // The actor collection reads its first child header lazily; the malformed
    // header stops the invocation before the later object root can run.
    assert_eq!(error.cursor, 22); // LS header + version chunk + reached root header
    assert_eq!(error.entry, 0x544a16);
    assert!(error.detail.contains("truncated scene chunk header"));
    let loader = session.loader().unwrap();
    assert_eq!(loader.cursor, error.cursor);
    assert_eq!(loader.version, [Some(5), Some(1)]);
    assert_eq!(loader.finished_objects, 0);
    assert_eq!(loader.load_map.map.slot(b"must not run"), None);
    let camera = loader.camera.unwrap();
    assert_eq!(
        session
            .arena()
            .scene_active_camera(session.scene().unwrap().unwrap())
            .unwrap(),
        Some(camera)
    );
    assert_eq!(session.arena().frame(camera.frame()).unwrap().references, 1);
    discard(&mut session);
    assert!(session.arena().frame(camera.frame()).is_err());
}

#[test]
fn admitted_prefix_changes_the_same_world_frame_and_closes_owned_resources() {
    let position: Vec<u8> = [3f32, 4., 5.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let bytes = scene2(&chunk(
        0x4000,
        &object(0, b"world part", &chunk(0x20, &position)),
    ));
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
    assert!(session.failure().is_none());
    let loader = session.loader().unwrap();
    assert_eq!(loader.cursor, bytes.len());
    assert_eq!(loader.finished_objects, 1);
    let frame = session.world_progress().unwrap().frames()[0];
    assert_eq!(
        loader.load_map.map.slot(b"world part"),
        Some(Some(session.arena().lookup_reference(frame)))
    );
    assert_eq!(
        session.arena().state(frame).unwrap().local.elements[12..15],
        [3., 4., 5.]
    );
    // Source setters invalidate the lazy world cache. Resolve the actual
    // matrix through its owner instead of requiring eager cache mutation.
    let world = session
        .precision
        .run(|| session.arena.world_matrix(frame))
        .unwrap();
    assert_eq!(world.elements[12..15], [3., 4., 5.]);
    let scene = session.scene().unwrap().unwrap();
    discard(&mut session);
    let report = session.resources().cpu_close_report(scene).unwrap();
    assert!(report.native_managed_residency_absent);
    assert!(!report.native_device_virtual14_executed);
    assert!(session.arena().frame(frame).is_err());
}

#[test]
fn explicit_model_directory_loads_and_caches_actual_model_owners() {
    let bytes = scene2(&chunk(0x4000, &model_object()));
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part")])),
        Some(&bytes),
        // The bounded pristine cache-copy profile requires an enabled source
        // frame; use the same culling byte as the existing model-cache fixture.
        Some(&world(&[dummy_with_culling(0, b"part", 0x11)])),
    );
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[b"Models"]),
        Ok(()),
        "{:?}",
        session.failure()
    );
    let loader = session.loader().unwrap();
    assert_eq!(loader.finished_objects, 1);
    assert!(loader.pending_model_open.is_none());
    let instance = loader.load_map.map.slot(b"instance").unwrap().unwrap();
    let model = session
        .arena()
        .model(ModelKey(FrameKey(instance.identity())))
        .unwrap();
    assert_eq!(model.frames.len(), 1);
    assert_eq!(
        loader.load_map.map.slot(b"instance.part"),
        Some(Some(session.arena().lookup_reference(model.frames[0])))
    );
    assert_eq!(session.caches().len(MissionCacheKind::Model), 1);
    let progress = session.caches().model_open_progress().unwrap();
    assert_eq!(progress.phase, ModelCachePhase::Complete);
    assert_eq!(progress.model_open_result, Some(0));
    let cached = progress.cached_source.unwrap();
    assert_ne!(cached.root(), model.key.root());
    assert!(session.cache_memory().live_allocations() > 0);
    discard(&mut session);
    assert!(session.arena().model(cached).is_err());
}

#[test]
fn empty_search_path_remains_empty_and_does_not_find_an_existing_model() {
    let bytes = scene2(&chunk(0x4000, &model_object()));
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part")])),
        Some(&bytes),
        Some(&world(&[dummy(0, b"part")])),
    );
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let progress = session.caches().model_open_progress().unwrap();
    assert_eq!(progress.model_open_result, Some(0x80000001u32 as i32));
    assert!(session
        .caches()
        .directories(MissionCacheKind::Model)
        .is_empty());
    assert_eq!(session.caches().len(MissionCacheKind::Model), 0);
    assert_eq!(session.load_map().unwrap().map.slot(b"instance.part"), None);
    // The bounded source loader accepts the native negative HRESULT and moves
    // on. PrefixComplete consequently cannot be used as a playable-game gate.
    assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
    discard(&mut session);
}

#[test]
fn malformed_model_stops_cache_call_and_terminal_discard_preserves_interrupted_refusal() {
    let bytes = scene2(&chunk(
        0x4000,
        &[model_object(), object(9, b"must not run", &[])].concat(),
    ));
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part")])),
        Some(&bytes),
        Some(b"synthetic invalid 4DS"),
    );
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[b"Models"]),
        Err(LoadStop::Step(OpenStep::OpenBin))
    );
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("wrong frontier: {:?}", session.failure());
    };
    assert_eq!(error.entry, 0x54220a);
    let name_end = bytes
        .windows(b"fixture.4ds\0".len())
        .position(|value| value == b"fixture.4ds\0")
        .unwrap()
        + b"fixture.4ds\0".len();
    assert_eq!(error.cursor, name_end);
    let original_error = format!("{:?}", session.failure().unwrap());
    assert!(session.loader().unwrap().pending_model_open.is_some());
    assert_eq!(session.loader().unwrap().finished_objects, 0);
    assert_eq!(session.load_map().unwrap().map.slot(b"must not run"), None);
    assert!(session.caches().is_interrupted(MissionCacheKind::Model));
    assert!(!session
        .caches()
        .pending_names(MissionCacheKind::Model)
        .is_empty());
    let before = session.owned_counts().unwrap();
    let scene = session.scene().unwrap().unwrap();
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert!(matches!(
        session.discard_failure().unwrap().cause,
        MissionDiscardCause::Cache {
            kind: MissionCacheKind::Model,
            error: MissionCacheError::Interrupted
        }
    ));
    assert_eq!(session.discard_progress().caches_destructed, 0);
    assert!(session.discard_progress().scene_release_executed);
    assert!(session.arena().scene(scene).is_err());
    assert_ne!(session.owned_counts().unwrap(), before);
    assert_eq!(format!("{:?}", session.failure().unwrap()), original_error);
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::AlreadyEntered)
    );
    assert_eq!(
        load(&mut session, &[b"Models"]),
        Err(LoadStop::Admission(
            MissionLoadCallError::TerminalDiscardEntered
        ))
    );
    // Rust Drop now releases this synthetic host storage. It is not reported
    // as a completed original cache destructor or native exception rollback.
}

#[test]
fn load_map_preserves_non_utf8_names_and_continues_to_the_camera() {
    let bytes = scene2(&[]);
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"first"), dummy(1, &[0xe8])])),
        Some(&bytes),
        None,
    );
    let mut session = fixture.session();
    assert_eq!(load(&mut session, &[]), Ok(()));
    assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
    assert!(session.failure().is_none());
    let frames = session.world_progress().unwrap().frames().to_vec();
    assert_eq!(frames.len(), 2);
    assert_eq!(
        session.arena().links(frames[1]).unwrap().parent,
        Some(frames[0])
    );
    assert_eq!(session.arena().frame(frames[1]).unwrap().name_bytes, [0xe8]);
    let scene = session.scene().unwrap().unwrap();
    let sectors = session.arena().scene(scene).unwrap().sectors;
    let expected = [
        (
            b"first".as_slice(),
            session.arena().lookup_reference(frames[0]),
        ),
        (&[0xe8], session.arena().lookup_reference(frames[1])),
        (
            b"Primary sector",
            session.arena().lookup_reference(sectors[0].frame()),
        ),
        (
            b"Backdrop sector",
            session.arena().lookup_reference(sectors[1].frame()),
        ),
        (
            b"Omni sector",
            session.arena().lookup_reference(sectors[2].frame()),
        ),
    ];
    assert_eq!(
        session
            .load_map()
            .unwrap()
            .publications
            .iter()
            .map(|publication| (publication.name.as_slice(), publication.frame))
            .collect::<Vec<_>>(),
        expected
    );
    for (name, frame) in expected {
        assert_eq!(
            session.load_map().unwrap().map.slot(name),
            Some(Some(frame))
        );
    }
    // Source names are byte strings, not lossy UTF-8 presentation text.
    assert_eq!(session.load_map().unwrap().map.slot(b"\xef\xbf\xbd"), None);
    let loader = session.loader().unwrap();
    assert_eq!(loader.cursor, bytes.len());
    assert_eq!(loader.version, [Some(5), Some(1)]);
    assert_eq!(loader.finished_objects, 0);
    assert_eq!(session.scene2_byte_count(), Some(bytes.len()));
    let camera = loader
        .camera
        .expect("camera after successful map publication");
    assert_eq!(
        session.arena().scene_active_camera(scene).unwrap(),
        Some(camera)
    );
    discard(&mut session);
    for frame in frames {
        assert!(session.arena().frame(frame).is_err());
    }
    assert!(session.arena().camera(camera).is_err());
    assert!(session.arena().scene(scene).is_err());
}

#[test]
fn loaded_mission_presentation_reuses_live_scene_camera_and_disposes_normally() {
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part")])),
        Some(&scene2(&[])),
        None,
    );
    let mut session = fixture.session();
    let mut sequence = crate::scene_render::SceneSnapshotSequence::default();
    assert!(session
        .snapshot_loaded_scene([64, 64], &mut sequence)
        .is_err());
    load(&mut session, &[]).unwrap();
    let scene = session.scene().unwrap().unwrap();
    let camera = session.arena().scene_active_camera(scene).unwrap().unwrap();
    assert_eq!(session.loader().unwrap().camera, Some(camera));
    assert_eq!(
        session.resources().construction(scene).unwrap().viewport,
        [0, 0, 800, 600]
    );
    assert!(session
        .snapshot_loaded_scene([0, 64], &mut sequence)
        .is_err());
    assert_eq!(
        session.resources().construction(scene).unwrap().viewport,
        [0, 0, 800, 600]
    );
    let snapshot = session
        .snapshot_loaded_scene([64, 64], &mut sequence)
        .unwrap();
    assert_eq!(snapshot.frame.viewport, [0, 0, 64, 64]);
    assert_eq!(session.scene().unwrap(), Some(scene));
    assert_eq!(
        session.arena().scene_active_camera(scene).unwrap(),
        Some(camera)
    );
    assert!(snapshot.scene.meshes.is_empty());
    let next = session
        .snapshot_loaded_scene([128, 64], &mut sequence)
        .unwrap();
    assert_eq!(next.frame.viewport, [0, 0, 128, 64]);
    assert_eq!(
        session.resources().construction(scene).unwrap().viewport,
        [0, 0, 128, 64]
    );
    discard(&mut session);
    assert!(session
        .snapshot_loaded_scene([64, 64], &mut sequence)
        .is_err());
}

#[test]
fn failed_world_geometry_uses_actual_staging_disposal_before_scene_release() {
    let fixture = Fixture::new(Some(&world(&[sector()])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    session
        .arena
        .sector_storage_mut()
        .fail_on_allocation(Some(1));
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenWorld))
    );
    assert_eq!(session.phase(), MissionLoadPhase::World);
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.phase(), CpuSceneWorldPhase::Geometry);
    let staging = progress.staging_model().unwrap();
    assert_eq!(progress.transferred_frames(), 0);
    assert_eq!(session.arena().model(staging).unwrap().frames.len(), 1);
    assert!(session.loader().is_none());
    discard(&mut session);
    assert_eq!(session.world_progress().unwrap().staging_model(), None);
    assert!(session.arena().model(staging).is_err());
}

#[test]
fn failed_world_prepare_preserves_scene_ownership_and_the_actual_cleanup_refusal() {
    use crate::frame_sector::{SectorAllocationKind, SectorError, SectorStorageEvent};
    use crate::model_loading::ModelError;
    use crate::scene_ownership::FrameOwner;

    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part"), sector()])),
        Some(&scene2(&[])),
        None,
    );
    let mut session = fixture.session();
    // Existing allocator injection: the sector's vertices and faces load,
    // then its TemporaryPlanes allocation fails in the genuine MakeReady.
    // No world-loader callback or artificial owner state is introduced.
    session
        .arena
        .sector_storage_mut()
        .fail_on_allocation(Some(3));
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenWorld))
    );
    assert_eq!(session.phase(), MissionLoadPhase::World);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::World(CpuSceneWorldError::Model(ModelError::Sector(
            SectorError::Allocation
        )))
    ));
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.phase(), CpuSceneWorldPhase::Prepare);
    assert_eq!(progress.transferred_frames(), 2);
    assert_eq!(progress.prepared_sectors(), 0);
    assert_eq!(progress.staging_model(), None);
    let frames = progress.frames().to_vec();
    let scene = session.scene().unwrap().unwrap();
    assert_eq!(session.arena().scene(scene).unwrap().frames(), frames);
    for &frame in &frames {
        assert_eq!(
            session.arena().frame(frame).unwrap().owner,
            Some(FrameOwner::Scene(scene))
        );
    }
    assert!(matches!(
        session.arena().sector_storage().events().last(),
        Some(SectorStorageEvent::Allocate {
            kind: SectorAllocationKind::TemporaryPlanes,
            key: None,
            ..
        })
    ));
    assert!(session.loader().is_none());
    assert_eq!(session.scene2_byte_count(), None);
    let original_error = format!("{:?}", session.failure().unwrap());
    let before = session.owned_counts().unwrap();
    assert_eq!(before.sector_allocations, 2);
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    // Staging disposal has no remaining model to release. The actual Scene
    // destructor preflight refuses the interrupted sector; it cannot silently
    // turn the prior MakeReady error into native cleanup success.
    assert_eq!(session.discard_progress().phase, MissionDiscardPhase::Scene);
    assert_eq!(session.discard_progress().caches_destructed, 0);
    assert!(!session.discard_progress().scene_release_executed);
    assert!(session.scene_close_report().is_none());
    assert!(!session.discard_progress().registry_storage_released);
    assert!(matches!(
        session.discard_failure().unwrap().cause,
        MissionDiscardCause::RegisteredScene(DriverSceneError::Resources(
            SceneResourceError::Model(ModelError::Sector(SectorError::Interrupted))
        ))
    ));
    assert_eq!(session.owned_counts().unwrap(), before);
    assert_eq!(session.driver().scenes(), &[scene]);
    assert_eq!(session.arena().scene(scene).unwrap().frames(), frames);
    for &frame in &frames {
        assert_eq!(
            session.arena().frame(frame).unwrap().owner,
            Some(FrameOwner::Scene(scene))
        );
    }
    assert_eq!(format!("{:?}", session.failure().unwrap()), original_error);
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::AlreadyEntered)
    );
    // Rust Drop frees host storage without claiming the refused original
    // destructor ran. The interrupted sector's guard is deliberately retained.
}

#[test]
fn initialized_host_audio_creates_and_releases_actual_listener() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let scene = session.scene().unwrap().unwrap();
    assert_eq!(session.driver().scenes(), &[scene]);
    assert_eq!(session.owned_counts().unwrap().driver_listeners, 1);
    assert_eq!(session.owned_counts().unwrap().audio_sounds, 0);
    assert!(!session.owned_counts().unwrap().audio_listener_pose);
    discard(&mut session);
    assert!(session.arena().scene(scene).is_err());
}

#[test]
fn game_init_reset_reads_actual_scene_weather_and_stops_at_unowned_render() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let scene = session.scene().unwrap().unwrap();
    let source = &session.resources().construction(scene).unwrap().words;
    let expected = (
        crate::scene_weather::get_weather_system_param(source, 0) != 0,
        crate::scene_weather::get_weather_system_param(source, 10) as i32,
    );
    let before = *source;
    // Use non-default previous values: the native stores must truly replace
    // live game scalars, not pass solely because the host starts at zero.
    session.game_inline.set_mission_passed(true);
    session.game_inline.set_weather_buffer(71);
    session.game_init_car_sounds = 42;
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    assert_eq!(
        session.reset_game_mission_state_for_init(),
        Err("Init Scene::Render after sound mute unavailable".into()),
    );
    assert_eq!(
        session.game_init_reset_observation(),
        (false, 0, Some(expected), -1)
    );
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(
        session.resources().construction(scene).unwrap().words,
        before
    );
    assert_eq!(
        session.reset_game_mission_state_for_init(),
        Err("mission state reset already entered".into()),
    );
    discard(&mut session);
}

struct InitSceneRender {
    calls: Vec<SceneKey>,
    audio: RuntimeAudio,
    fail: bool,
}

impl crate::scene_projection::SceneRenderBackend for InitSceneRender {
    fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
        assert_eq!(self.audio.master_gain().unwrap(), 0.0);
        arena.scene(scene)?;
        self.calls.push(scene);
        if self.fail {
            return Err(ModelError::Asset(
                "injected scene render interruption".into(),
            ));
        }
        // The original caller ignores the numeric I3D_scene::Render status.
        Ok(0x8000_0009)
    }
}

#[test]
fn game_init_render_borrows_live_scene_once_then_marks_volume_restore() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    // Open needed the HUD; without it Init stops at ScoreSetOn.
    session.indicators = None;
    consume_open_restore(&mut session);
    let scene = session.scene().unwrap().unwrap();
    let mut renderer = InitSceneRender {
        calls: Vec::new(),
        audio: session.audio().clone(),
        fail: false,
    };
    assert!(!session.open_restore_volume());
    assert_eq!(
        session.reset_game_mission_state_with_render(&mut renderer),
        Err("Init ScoreSetOn mafiaIndicators owner unavailable".into()),
    );
    assert_eq!(renderer.calls, [scene]);
    assert!(session.open_restore_volume());
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(session.game_init_reset_observation().3, -1);
    assert_eq!(
        session.reset_game_mission_state_with_render(&mut renderer),
        Err("mission state reset already entered".into()),
    );
    assert_eq!(renderer.calls, [scene]);
    discard(&mut session);
}

#[test]
fn game_init_render_with_hud_completes_the_reset_span() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    session
        .install_indicators(crate::indicators::IndicatorsState::constructed(
            crate::indicators::TimerClock {
                hour: 1,
                minute: 2,
                second: 3,
            },
        ))
        .unwrap();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let mut renderer = InitSceneRender {
        calls: Vec::new(),
        audio: session.audio().clone(),
        fail: false,
    };
    // The renderer-backed entry reaches the same span end as the presented
    // driver: the span returns, the next step belongs to the driver.
    assert_eq!(
        session.reset_game_mission_state_with_render(&mut renderer),
        Ok(())
    );
    assert!(session.open_restore_volume());
    assert!(session.game_state().heal_cheat());
    assert_eq!(session.game_state().death_fade_time(), 7000);
    assert_eq!(
        session.indicators().unwrap().bonus_count(),
        crate::indicators::BONUS_NONE
    );
    discard(&mut session);
}

#[test]
fn game_init_render_failure_keeps_muted_prefix_and_does_not_mark_restore() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let scene = session.scene().unwrap().unwrap();
    let mut renderer = InitSceneRender {
        calls: Vec::new(),
        audio: session.audio().clone(),
        fail: true,
    };
    let error = session
        .reset_game_mission_state_with_render(&mut renderer)
        .unwrap_err();
    assert!(error.contains("Init Scene::Render"), "{error}");
    assert_eq!(renderer.calls, [scene]);
    assert!(!session.open_restore_volume());
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(
        session.reset_game_mission_state_with_render(&mut renderer),
        Err("mission state reset already entered".into()),
    );
    assert_eq!(renderer.calls, [scene]);
    discard(&mut session);
}

#[test]
fn mission_tick_head_restores_real_pcm_master_gain_once_after_skipped_frames() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    assert_eq!(
        session.tick_restore_volume_head(),
        Err("mission tick requires completed Open".into())
    );
    load(&mut session, &[]).unwrap();
    // Stage the already loaded owner at the native post-Open boundary.
    session.mark_open_mission_opened().unwrap();
    session.set_open_configured_volume(0.6).unwrap();
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    session.mute_audio_and_render_for_open().unwrap();
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    session.mark_open_restore_volume().unwrap();
    session.set_skip_frames_for_tick(2).unwrap();
    assert_eq!(session.tick_restore_volume_head(), Ok(false));
    assert!(session.open_restore_volume());
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    session.set_skip_frames_for_tick(0).unwrap();
    assert_eq!(session.tick_restore_volume_head(), Ok(true));
    assert!(!session.open_restore_volume());
    assert_eq!(session.open_restored(), Some(0.6));
    assert_eq!(session.audio().master_gain().unwrap(), 0.6);
    session.set_open_configured_volume(0.3).unwrap();
    assert_eq!(session.tick_restore_volume_head(), Ok(false));
    assert_eq!(session.audio().master_gain().unwrap(), 0.6);
    discard(&mut session);
    assert_eq!(
        session.set_skip_frames_for_tick(1),
        Err("terminal discard entered".into())
    );
}

#[test]
fn mission_tick_head_consumes_restore_before_a_refused_audio_write() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    session.mark_open_mission_opened().unwrap();
    session.mute_audio_and_render_for_open().unwrap();
    session.set_open_configured_volume(1.5).unwrap();
    session.mark_open_restore_volume().unwrap();
    assert!(session
        .tick_restore_volume_head()
        .unwrap_err()
        .contains("master volume outside host mixer domain"));
    assert!(!session.open_restore_volume());
    // Open's own RestoreAudio recorded 1.0; the refused write keeps it.
    assert_eq!(session.open_restored(), Some(1.0));
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(session.tick_restore_volume_head(), Ok(false));
    discard(&mut session);
}

#[test]
fn mission_tick_head_consumes_flag_even_when_sound_owner_is_absent() {
    let fixture = Fixture::new(None, None, None);
    let mut session = fixture.session();
    session.mark_open_mission_opened().unwrap();
    session.mark_open_restore_volume().unwrap();
    session.set_skip_frames_for_tick(-1).unwrap();
    assert!(!session.audio_present_for_open());
    assert_eq!(session.tick_restore_volume_head(), Ok(true));
    assert!(!session.open_restore_volume());
    assert_eq!(session.open_restored(), None);
    assert_eq!(session.tick_restore_volume_head(), Ok(false));
}

#[test]
fn scene2_sound_open_uses_real_cache_pcm_owners_and_terminal_release() {
    let bytes = scene2(&chunk(0x4000, &sound_object(b"a.wav\0")));
    let fixture = Fixture::new(Some(&world(&[])), Some(&bytes), None);
    std::fs::write(fixture.0.join("Sounds/a.wav"), pcm_wav()).unwrap();
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let before = session.owned_counts().unwrap();
    assert_eq!(before.driver_listeners, 1);
    assert_eq!(before.driver_sound_frames, 2);
    assert_eq!(before.audio_sounds, 2);
    assert_eq!(before.audio_voices, 2);
    assert_eq!(before.audio_sources, 1);
    assert_eq!(session.caches().len(MissionCacheKind::Sound), 1);
    assert_eq!(
        session
            .caches()
            .sound_open_progress()
            .unwrap()
            .sound_open_result,
        Some(0)
    );
    discard(&mut session);
}

#[test]
fn sound_cache_error_retains_first_scene2_prefix_and_refused_destructor() {
    let bytes = scene2(&chunk(0x4000, &sound_object(b"broken.wav\0")));
    let fixture = Fixture::new(Some(&world(&[])), Some(&bytes), None);
    std::fs::write(fixture.0.join("Sounds/broken.wav"), b"not PCM WAV").unwrap();
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenBin))
    );
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("expected first Scene2 Sound Open failure");
    };
    assert_eq!(error.entry, 0x542e9a);
    assert!(error.cursor > 0);
    let first = format!("{:?}", session.failure().unwrap());
    assert!(session.caches().is_interrupted(MissionCacheKind::Sound));
    let open = session.caches().sound_open_progress().unwrap();
    assert_eq!(open.flags, 0x10);
    assert_eq!(open.name, b"broken.wav");
    assert_eq!(open.path.as_deref(), Some(b"Sounds\\broken.wav".as_slice()));
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert!(matches!(
        session.discard_failure().unwrap().cause,
        MissionDiscardCause::Cache {
            kind: MissionCacheKind::Sound,
            error: MissionCacheError::Interrupted
        }
    ));
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::Cache(MissionCacheKind::Sound)
    );
    assert_eq!(format!("{:?}", session.failure().unwrap()), first);
}

#[test]
fn early_constructor_failure_is_retained_and_cannot_be_force_destroyed() {
    let fixture = Fixture::new(None, None, None);
    let mut session = fixture.session();
    let mut screen = Screen {
        fail_size: true,
        calls: 0,
    };
    assert_eq!(
        load_with(&mut session, &[], &mut screen),
        Err(LoadStop::Bootstrap)
    );
    assert_eq!(screen.calls, 1);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::SceneCreation {
            phase: DriverSceneCreatePhase::Constructor,
            ..
        }
    ));
    let scene = session.scene().unwrap().unwrap();
    let before = session.owned_counts().unwrap();
    assert!(session.arena().scene(scene).is_ok());
    assert!(session.resources().construction(scene).is_none());
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert!(matches!(session.discard_failure().unwrap().cause,
        MissionDiscardCause::UnregisteredScene(SceneResourceError::UnknownConstruction(key)) if key == scene));
    assert_eq!(session.owned_counts().unwrap(), before);
    assert!(!session.discard_progress().scene_release_executed);
}

#[test]
fn independent_sessions_cannot_resolve_each_others_map_references() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut first = fixture.session();
    let mut second = fixture.session();
    load(&mut first, &[]).unwrap();
    load(&mut second, &[]).unwrap();
    let a = first
        .load_map()
        .unwrap()
        .map
        .slot(b"part")
        .unwrap()
        .unwrap();
    let b = second
        .load_map()
        .unwrap()
        .map
        .slot(b"part")
        .unwrap()
        .unwrap();
    assert_ne!(a, b);
    assert_ne!(second.arena().lookup_reference(FrameKey(a.identity())), a);
    assert!(matches!(
        first.driver().validate_scene_domain(
            second.arena(),
            second.resources(),
            first.scene().unwrap().unwrap()
        ),
        Err(DriverSceneError::ForeignDomain)
    ));
    discard(&mut first);
    assert!(second.arena().frame(FrameKey(b.identity())).is_ok());
    discard(&mut second);
}

fn install_scene_visual(session: &mut MissionLoadSession) -> FrameKey {
    let mut visual = crate::model_loading::tests::frame(1, None, [0.; 3]);
    // Start with the 4DS plain OBJ_ class. Visual ordinal 1 constructs LMAP
    // directly, which would bypass the scene2 OBJ_ -> LMAP replacement.
    visual.visual = Some(0);
    visual.render_flags = 0;
    visual.logic_flags = 0;
    visual.lods.push(lhr_assets::Lod {
        distance: 0.,
        uv_only: None,
        vertices: [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            .map(|position| lhr_assets::Vertex {
                position,
                normal: [0., 0., -1.],
                uv: [0.; 2],
            })
            .to_vec(),
        groups: vec![lhr_assets::FaceGroup {
            material: 0,
            indices: vec![0, 1, 2],
        }],
    });
    let source = std::sync::Arc::new(lhr_assets::Model {
        materials: vec![lhr_assets::Material {
            flags: 0,
            ambient: [0.; 3],
            diffuse: [1.; 3],
            texture: String::new(),
            emission: [0.; 3],
            opacity: 1.,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        }],
        frames: vec![visual],
    });
    let scene = session.scene().unwrap().unwrap();
    let primary = session.arena.scene(scene).unwrap().sectors[0];
    let model = session.arena.create_model().unwrap();
    session.arena.load_parsed_into_model(model, source).unwrap();
    session
        .arena
        .load_plain_visual_resources(model, &session.installation)
        .unwrap();
    let frame = session.arena.model(model).unwrap().frames[0];
    session
        .arena
        .link_frame(frame, Some(primary.frame()), 0)
        .unwrap();
    session.arena.scene_add_frame(scene, frame).unwrap();
    session.arena.release(model.root()).unwrap();

    // Scene owns the sole remaining frame reference. Replacement's
    // Scene::DeleteFrame must therefore destroy the old visual.
    assert_eq!(session.arena.frame(frame).unwrap().references, 1);
    assert_eq!(
        session.arena.owned_object_visual_type(frame).unwrap(),
        crate::visual_replacement::OBJECT_VISUAL
    );

    frame
}

#[test]
fn terminal_disposal_counts_and_releases_actual_saved_lightmap_owners() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    assert_eq!(
        session.owned_counts().unwrap().visual_lightmaps,
        AtlasStatistics::default()
    );

    let old = install_scene_visual(&mut session);
    let mut body = Vec::new();
    body.extend(1u16.to_le_bytes()); // one rectangle
    body.extend(1u16.to_le_bytes()); // one actual face group
    body.push(1); // single-color 3x3 rectangle
    body.extend(0x00204060u32.to_le_bytes());
    body.extend(0u32.to_le_bytes()); // no dimension-only maps
    body.extend(3u32.to_le_bytes()); // three expanded vertices
    for uv in [[0f32, 0.], [1., 0.], [0., 1.]] {
        for value in uv {
            body.extend(value.to_le_bytes());
        }
    }
    for source_vertex in 0u16..3 {
        body.extend(source_vertex.to_le_bytes());
        body.extend(0u16.to_le_bytes()); // rectangle index
    }
    body.extend(3u32.to_le_bytes());
    for index in 0u16..3 {
        body.extend(index.to_le_bytes());
    }
    body.extend(1u32.to_le_bytes()); // actual face count
    body.extend(0u16.to_le_bytes()); // face rectangle assignment
    let mut payload = vec![1, 0x21, 2]; // one saved lighting, version, lightmap mode
    payload.extend(1u32.to_le_bytes());
    payload.extend(5f32.to_le_bytes());
    payload.extend(45f32.to_le_bytes());
    payload.push(1); // one LOD
    payload.extend(0u16.to_le_bytes());
    payload.extend(body);
    let bytes = scene2(&chunk(
        0x4000,
        &object(
            0,
            b"fixture",
            &[
                chunk(
                    0x4090,
                    &crate::visual_replacement::LIT_OBJECT_VISUAL.to_le_bytes(),
                ),
                chunk(0x40a0, &payload),
            ]
            .concat(),
        ),
    ));
    let scene = session.scene().unwrap().unwrap();
    let mut loader =
        MissionSceneLoader::new(&session.arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
    assert_eq!(
        loader.load_map.map.slot(b"fixture"),
        Some(Some(session.arena.lookup_reference(old)))
    );
    loader
        .load_prefix_with_resource_cache(
            &mut session.driver,
            &mut session.arena,
            &session.resources,
            &mut Screen::default(),
            GameCameraProjection {
                global_mode: 0,
                screen_flag_46f4: false,
            },
            &bytes,
            &mut session.caches,
            &session.installation,
            &mut session.cache_memory,
        )
        .unwrap();
    assert_eq!(loader.cursor, bytes.len());
    assert!(session.arena.frame(old).is_err());
    let frame = FrameKey(loader.current_frame.unwrap().identity());
    assert_ne!(frame, old);
    assert_eq!(
        session.arena.owned_object_visual_type(frame).unwrap(),
        crate::visual_replacement::LIT_OBJECT_VISUAL
    );
    session.loader = Some(loader);
    let before = session.owned_counts().unwrap();
    assert_eq!(
        before.visual_lightmaps,
        AtlasStatistics {
            live_pages: 1,
            live_allocations: 1,
        }
    );
    assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
    assert!(session.failure().is_none());
    assert_eq!(before.saved_lighting_lods, 1);

    discard(&mut session);
    assert!(session.arena().frame(frame).is_err());
    assert_eq!(
        session.owned_counts().unwrap().visual_lightmaps,
        AtlasStatistics::default()
    );
}

#[test]
fn pending_factory_creator_is_released_before_scene_teardown() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let pending = session
        .arena
        .insert(
            crate::frame_transform::I3dFrameState::dummy(),
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .unwrap();
    session.loader.as_mut().unwrap().pending_factory_creator =
        Some(session.arena.lookup_reference(pending));
    assert_eq!(session.owned_counts().unwrap().pending_factory_creator, 1);
    discard(&mut session);
    assert!(session.arena().frame(pending).is_err());
}

#[test]
fn scene2_saved_lighting_replacement_uses_current_owner_and_terminal_disposal() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let old = install_scene_visual(&mut session);
    let mut colors = vec![1, 0x21, 1]; // one lighting, version 0x21, vertex mode
    colors.extend(1u32.to_le_bytes());
    colors.extend(5f32.to_le_bytes());
    colors.extend(45f32.to_le_bytes());
    colors.push(1); // native saved-lighting level one (first of six slots)
    colors.extend(0u16.to_le_bytes());
    for color in [0xff102030u32, 0xff405060, 0xff708090] {
        colors.extend(color.to_le_bytes());
    }
    let bytes = scene2(&chunk(
        0x4000,
        &object(
            0,
            b"fixture",
            &[
                chunk(
                    0x4090,
                    &crate::visual_replacement::LIT_OBJECT_VISUAL.to_le_bytes(),
                ),
                chunk(0x40a0, &colors),
            ]
            .concat(),
        ),
    ));
    // Drive the actual current scene2 dispatcher over the session's real Scene
    // and model arena. This controlled second input is a synthetic regression,
    // not another ordered source Open admission.
    let scene = session.scene().unwrap().unwrap();
    let mut loader =
        MissionSceneLoader::new(&session.arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
    loader
        .load_prefix_with_resource_cache(
            &mut session.driver,
            &mut session.arena,
            &session.resources,
            &mut Screen::default(),
            GameCameraProjection {
                global_mode: 0,
                screen_flag_46f4: false,
            },
            &bytes,
            &mut session.caches,
            &session.installation,
            &mut session.cache_memory,
        )
        .unwrap();
    assert_eq!(loader.cursor, bytes.len());
    assert_eq!(loader.finished_objects, 1);
    assert!(session.arena.frame(old).is_err());
    let replacement = FrameKey(loader.current_frame.unwrap().identity());
    assert_ne!(replacement, old);
    assert_eq!(
        session.arena.owned_object_visual_type(replacement).unwrap(),
        crate::visual_replacement::LIT_OBJECT_VISUAL
    );
    let object = session
        .arena
        .frame_object_resources(replacement)
        .unwrap()
        .unwrap();
    assert!(matches!(
        session.arena.loaded_visual_resources().unwrap().saved_lit_lod(object, 0).unwrap(),
        Some(crate::i3d_object::saved_lightmaps::SavedLitLod::VertexColors { colors, .. })
            if colors.len() == 3
    ));
    assert_eq!(
        session
            .arena
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .saved_lighting_lod_count(),
        1
    );
    session.loader = Some(loader);
    discard(&mut session);
    assert!(session.arena().frame(replacement).is_err());
}

fn actor_record(name: &[u8], kind: u32, data: &[u8]) -> Vec<u8> {
    let mut name = name.to_vec();
    name.push(0);
    chunk(
        0xae21,
        &[
            chunk(0xae23, &name),
            chunk(0xae22, &kind.to_le_bytes()),
            chunk(0xae24, data),
        ]
        .concat(),
    )
}
fn program_record(name: &[u8], source: &[u8]) -> Vec<u8> {
    chunk(
        0xae51,
        &[
            vec![0],
            (name.len() as u32).to_le_bytes().to_vec(),
            name.to_vec(),
            (source.len() as u32).to_le_bytes().to_vec(),
            source.to_vec(),
        ]
        .concat(),
    )
}
#[test]
fn ordered_actor_and_program_roots_share_the_live_scene_map_and_terminal_owner() {
    use crate::actor_frames::FrameUserData;
    use crate::actor_world::ActorSource;
    use crate::original_program::ProgramPhase;
    let bytes = scene2(
        &[
            chunk(0x4000, &object(9, b"first", &[])),
            chunk(
                0xae20,
                &[
                    actor_record(b"first", 3, b"default ignored"),
                    actor_record(b"first", 1, b"opaque base payload"),
                    actor_record(b"later", 2, b"unreached missing frame"),
                ]
                .concat(),
            ),
            chunk(
                0xae50,
                &program_record(b"first-source", b"original unknown commands stay source"),
            ),
            chunk(0x4000, &object(9, b"later", &[])),
            chunk(
                0xae20,
                &[
                    actor_record(
                        b"first",
                        2,
                        b"duplicate userdata prevents unsupported factory",
                    ),
                    actor_record(b"later", 1, b""),
                ]
                .concat(),
            ),
            chunk(0xae50, &program_record(b"second-source", b"")),
        ]
        .concat(),
    );
    let fixture = Fixture::new(Some(&world(&[])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    assert_eq!(loader.finished_objects, 2);
    assert_eq!(loader.cursor, bytes.len());
    assert_eq!(loader.content.mission_path(), Some(b"synthetic".as_slice()));
    let actors = loader
        .content
        .actors
        .world
        .source(ActorSource::Primary)
        .to_vec();
    assert_eq!(actors.len(), 2);
    assert_eq!(loader.content.actors.world.actor_count, 2);
    assert!(loader
        .content
        .actors
        .world
        .source(ActorSource::Active)
        .is_empty());
    for (name, actor) in [
        (b"first".as_slice(), actors[0].unwrap()),
        (b"later".as_slice(), actors[1].unwrap()),
    ] {
        let frame = loader.load_map.map.slot(name).flatten().unwrap();
        assert_eq!(
            loader.content.actors.world.state(actor).unwrap().frame,
            Some(frame)
        );
        assert_eq!(loader.content.actors.world.state(actor).unwrap().byte_66, 1);
        assert!(matches!(
            session
                .arena()
                .user_data(FrameKey(frame.identity()))
                .unwrap(),
            FrameUserData::ActorRecord(_)
        ));
    }
    assert_eq!(loader.content.programs().len(), 2);
    assert_eq!(
        loader.content.programs()[0].name().unwrap().c_bytes(),
        Some(b"first-source".as_slice())
    );
    assert_eq!(
        loader.content.programs()[0].source().unwrap().c_bytes(),
        Some(b"original unknown commands stay source".as_slice())
    );
    assert!(loader
        .content
        .programs()
        .iter()
        .all(|program| program.phase() == ProgramPhase::Source));
    assert!(loader.content.live_allocation_count() > 0);
    // Frame and actor references above refer to the same owners used by disposal.
    discard(&mut session);
    let loader = session.loader().unwrap();
    assert_eq!(loader.content.actors.world.actor_count, 0);
    assert_eq!(loader.content.live_allocation_count(), 0);
    assert!(loader.content.terminal_complete());
    assert!(loader
        .content
        .programs()
        .iter()
        .all(|program| program.phase() == ProgramPhase::Dead));
}
#[test]
fn unsupported_named_actor_stops_at_kind_read_and_preserves_prior_program_and_actor() {
    use crate::actor_world::ActorSource;
    let bytes = scene2(
        &[
            chunk(0xae20, &actor_record(b"first", 1, b"")),
            chunk(0xae50, &program_record(b"retained", b"source")),
            chunk(0xae20, &actor_record(b"second", 9, b"")),
            chunk(0x4000, &object(9, b"unreached", &[])),
        ]
        .concat(),
    );
    let root = lhr_assets::scene_actor_stream::scene2_root(&bytes).unwrap();
    let actor_root = root.children().nth(3).unwrap().unwrap();
    let record = actor_root.children().next().unwrap().unwrap();
    let kind = record.children().nth(1).unwrap().unwrap();
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"first"), dummy(0, b"second")])),
        Some(&bytes),
        None,
    );
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenBin))
    );
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("wrong failure");
    };
    assert_eq!(error.entry, 0x544a16);
    assert_eq!(error.cursor, kind.offset + 10);
    assert!(error.detail.contains("UnsupportedKind(9)"));
    let loader = session.loader().unwrap();
    assert_eq!(
        loader
            .content
            .actors
            .world
            .source(ActorSource::Primary)
            .len(),
        1
    );
    assert_eq!(loader.content.programs().len(), 1);
    assert_eq!(loader.load_map.map.slot(b"unreached"), None);
    let failure = format!("{:?}", session.failure());
    discard(&mut session);
    assert_eq!(format!("{:?}", session.failure()), failure);
}
#[test]
fn box_kind_consumes_flag_payload_and_advances_cursor() {
    let mut payload = vec![2u8, 0x11];
    payload.extend(0x11111111u32.to_le_bytes());
    payload.extend(0x22222222u32.to_le_bytes());
    payload.extend(40.0f32.to_le_bytes());
    payload.extend(0.5f32.to_le_bytes());
    payload.extend(0x55555555u32.to_le_bytes());
    payload.extend(0x66666666u32.to_le_bytes());
    payload.push(0x77);
    payload.extend(0x07777777u32.to_le_bytes());
    assert_eq!(payload.len(), 31);
    let bytes = scene2(&[chunk(0xae20, &actor_record(b"box1", 35, &payload))].concat());
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"box1")])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    assert_eq!(loader.cursor, bytes.len());
    // Fixture dummy frames resolve hierarchy bounds, so the Init tail links
    // and the box survives with volume-selected words.
    use crate::actor_world::{ActorSource, ActorVtable};
    let world = &loader.content.actors.world;
    assert_eq!(world.actor_count, 1);
    let actors = world.source(ActorSource::Primary).to_vec();
    assert_eq!(actors.len(), 1);
    let actor = actors[0].unwrap();
    assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::BoxNew);
    assert_eq!(world.state(actor).unwrap().word_10, 35);
    // LoadData overwrote the Init 0x38c byte with the payload value; the
    // payload density survives the trailing clamp.
    assert_eq!(world.box_new(actor).unwrap().bytes()[0x38c], 0x11);
    assert_eq!(world.box_new(actor).unwrap().density_word(), 40.0);
    discard(&mut session);
}
#[test]
fn pumper_kind_loads_fields_and_advances_cursor() {
    use crate::actor_world::{ActorSource, ActorVtable};
    let mut payload = vec![0x7fu8];
    payload.extend(670u32.to_le_bytes());
    payload.extend(669u32.to_le_bytes());
    payload.extend(300748u32.to_le_bytes());
    payload.extend(2u32.to_le_bytes());
    payload.extend([0x1eu8, 0, 0, 0]);
    payload.extend([0x79u8, 0, 0, 0, 0x7au8, 0, 0, 0]);
    payload.extend([0x7bu8, 0, 0, 0, 0x7cu8, 0, 0, 0, 0x7du8, 0, 0, 0]);
    assert_eq!(payload.len(), 41);
    let bytes = scene2(&[chunk(0xae20, &actor_record(b"pump1", 25, &payload))].concat());
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"pump1")])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    let world = &loader.content.actors.world;
    assert_eq!(world.actor_count, 1);
    let actors = world.source(ActorSource::Primary).to_vec();
    assert_eq!(actors.len(), 1);
    let actor = actors[0].unwrap();
    assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Pumper);
    assert_eq!(world.state(actor).unwrap().word_10, 25);
    let pumper = world.pumper(actor).unwrap();
    assert_eq!(pumper.counter_a(), 670);
    assert_eq!(pumper.counter_b(), 669);
    assert_eq!(pumper.mode_word(), 2);
    assert_eq!(pumper.price_word(), 300748);
    assert_eq!(loader.cursor, bytes.len());
    discard(&mut session);
}
#[test]
fn traffic_kind_loads_header_and_elements_and_advances_cursor() {
    use crate::actor_world::{ActorSource, ActorVtable};
    let mut payload = vec![5u8, 0, 0, 0];
    payload.extend(184.0f32.to_le_bytes());
    payload.extend(150.0f32.to_le_bytes());
    payload.extend(170.0f32.to_le_bytes());
    payload.extend(6u32.to_le_bytes());
    payload.extend(1u32.to_le_bytes());
    let mut record = vec![0u8; 0x20];
    record[..9].copy_from_slice(b"fordttud\0");
    payload.extend(record);
    assert_eq!(payload.len(), 56);
    let bytes = scene2(&[chunk(0xae20, &actor_record(b"traffic1", 12, &payload))].concat());
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"traffic1")])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    let world = &loader.content.actors.world;
    assert_eq!(world.actor_count, 1);
    let actors = world.source(ActorSource::Primary).to_vec();
    assert_eq!(actors.len(), 1);
    let actor = actors[0].unwrap();
    assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::TrafficCar);
    assert_eq!(world.state(actor).unwrap().word_10, 12);
    let car = world.traffic_car(actor).unwrap();
    assert_eq!(car.count_word(), 1);
    assert_eq!(&car.elements()[..9], b"fordttud\0");
    assert_eq!(world.traffic_tuning().unit_floats, [0x3f800000; 3]);
    assert_eq!(loader.cursor, bytes.len());
    discard(&mut session);
}
#[test]
fn door_kind_loads_versioned_fields_and_advances_cursor() {
    use crate::actor_world::{ActorSource, ActorVtable};
    let mut payload = vec![8u8, 0, 0, 0, 0x11, 0x22, 0x33];
    payload.extend(0x12345678u32.to_le_bytes());
    payload.extend([0x44, 0x55]);
    payload.extend(0x9abcdef0u32.to_le_bytes());
    payload.extend(0x0fedcba9u32.to_le_bytes());
    payload.extend([0u8; 48]);
    payload.push(0x66);
    assert_eq!(payload.len(), 70);
    let bytes = scene2(&[chunk(0xae20, &actor_record(b"door1", 6, &payload))].concat());
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"door1")])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    let world = &loader.content.actors.world;
    assert_eq!(world.actor_count, 1);
    let actors = world.source(ActorSource::Primary).to_vec();
    assert_eq!(actors.len(), 1);
    let actor = actors[0].unwrap();
    assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Door);
    assert_eq!(world.state(actor).unwrap().word_10, 6);
    let door = world.door(actor).unwrap();
    assert_eq!(door.lock_flag(), 0x11);
    assert_eq!(door.timer_word(), 0x9abcdef0);
    assert_eq!(loader.cursor, bytes.len());
    discard(&mut session);
}
#[test]
fn bridge_kind_loads_linked_flap_and_skips_unlinked_prefix() {
    use crate::actor_world::{ActorSource, ActorVtable};
    let bytes = scene2(
        &[chunk(
            0xae20,
            &[
                actor_record(b"br01", 20, b""),
                actor_record(b"lone", 20, b""),
            ]
            .concat(),
        )]
        .concat(),
    );
    let fixture = Fixture::new(
        Some(&world(&[
            dummy(0, b"br01"),
            dummy(0, b"br02"),
            dummy(0, b"lone"),
        ])),
        Some(&bytes),
        None,
    );
    std::fs::create_dir_all(fixture.0.join("tables")).unwrap();
    std::fs::write(
        fixture.0.join("tables/parnik.def"),
        [4u8, 0, 0, 0, 7, 0, 0, 0],
    )
    .unwrap();
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    let world = &loader.content.actors.world;
    // br01 links against the published br02 sibling; lone has no lonf
    // sibling and is released while its record still advances the cursor.
    assert_eq!(world.actor_count, 1);
    let actors = world.source(ActorSource::Primary).to_vec();
    assert_eq!(actors.len(), 1);
    let actor = actors[0].unwrap();
    assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Bridge);
    assert_eq!(world.state(actor).unwrap().word_10, 20);
    let sibling = loader.load_map.map.slot(b"br02").flatten().unwrap();
    let bridge = world.bridge(actor).unwrap();
    assert_eq!(bridge.frame_handle(), sibling.identity());
    assert_eq!(bridge.model_l_flag(), 1);
    assert_eq!(world.bridge_sectors().rows, [[0xffff; 9]; 2]);
    assert!(world.bridge_table().is_some());
    assert_eq!(loader.cursor, bytes.len());
    discard(&mut session);
}
#[test]
fn padded_kind_field_is_closed_before_factory_failure_and_preserves_registered_prefix() {
    use crate::actor_world::ActorSource;
    let unavailable = chunk(
        0xae21,
        &[
            chunk(0xae23, b"second\0"),
            chunk(
                0xae22,
                &[9u32.to_le_bytes().as_slice(), b"unread padding"].concat(),
            ),
            chunk(0xae24, b"must not reach actor data"),
        ]
        .concat(),
    );
    let bytes = scene2(
        &[
            chunk(0xae20, &actor_record(b"first", 1, b"")),
            chunk(0xae50, &program_record(b"retained", b"source")),
            chunk(0xae20, &unavailable),
            chunk(0x4000, &object(9, b"unreached", &[])),
        ]
        .concat(),
    );
    let root = lhr_assets::scene_actor_stream::scene2_root(&bytes).unwrap();
    let actor_root = root.children().nth(3).unwrap().unwrap();
    let record = actor_root.children().next().unwrap().unwrap();
    let kind = record.children().nth(1).unwrap().unwrap();
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"first"), dummy(0, b"second")])),
        Some(&bytes),
        None,
    );
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenBin))
    );
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("wrong failure");
    };
    assert_eq!(error.entry, 0x544a16);
    assert_eq!(error.cursor, kind.offset + kind.raw.len());
    assert!(error.detail.contains("UnsupportedKind(9)"));
    let loader = session.loader().unwrap();
    assert_eq!(loader.cursor, error.cursor);
    assert_eq!(loader.content.actors.cursor, error.cursor);
    assert_eq!(
        loader
            .content
            .actors
            .world
            .source(ActorSource::Primary)
            .len(),
        1
    );
    assert_eq!(loader.content.programs().len(), 1);
    assert!(loader.content.actors.current_actor.is_none());
    assert!(loader.content.actors.pending_load_data.is_none());
    assert_eq!(loader.load_map.map.slot(b"unreached"), None);
    let failure = format!("{:?}", session.failure());
    discard(&mut session);
    assert_eq!(format!("{:?}", session.failure()), failure);
}
#[test]
fn interrupted_program_source_retains_pending_owner_and_does_not_push_or_visit_later_roots() {
    let partial = chunk(
        0xae51,
        &[
            vec![0],
            4u32.to_le_bytes().to_vec(),
            b"name".to_vec(),
            8u32.to_le_bytes().to_vec(),
            b"bad".to_vec(),
        ]
        .concat(),
    );
    let bytes = scene2(
        &[
            chunk(0xae50, &program_record(b"before", b"complete")),
            chunk(0xae50, &partial),
            chunk(0xae20, &actor_record(b"world part", 1, b"")),
        ]
        .concat(),
    );
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenBin))
    );
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("wrong failure");
    };
    assert_eq!(error.entry, 0x544b92);
    assert!(error.detail.contains("truncated chunk"));
    let loader = session.loader().unwrap();
    assert_eq!(loader.content.programs().len(), 1);
    let pending = loader.content.pending_program().unwrap();
    assert_eq!(pending.name().unwrap().c_bytes(), Some(b"name".as_slice()));
    assert!(!pending.source().unwrap().complete);
    assert_eq!(loader.content.actors.world.actor_count, 0);
    let failed = format!("{:?}", session.failure());
    discard(&mut session);
    assert_eq!(session.loader().unwrap().content.live_allocation_count(), 0);
    assert_eq!(format!("{:?}", session.failure()), failed);
}

#[test]
fn forward_sound_links_resolve_only_after_interleaved_actor_program_and_later_area_roots() {
    let bytes = scene2(
        &[
            chunk(
                0x4000,
                &object(
                    4,
                    b"pending sound",
                    &chunk(0x4060, &chunk(0xb201, b"later area\0")),
                ),
            ),
            chunk(0xae20, &actor_record(b"world part", 1, b"")),
            chunk(
                0xae50,
                &program_record(b"source between roots", b"source only"),
            ),
            chunk(0x4000, &object(14, b"later area", &[])),
        ]
        .concat(),
    );
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    assert!(loader.pending_sound_area_links.is_empty());
    let sound_frame = FrameKey(
        loader
            .load_map
            .map
            .slot(b"pending sound")
            .flatten()
            .unwrap()
            .identity(),
    );
    let area_frame = FrameKey(
        loader
            .load_map
            .map
            .slot(b"later area")
            .flatten()
            .unwrap()
            .identity(),
    );
    let sound = session.arena().sound_key(sound_frame).unwrap();
    let area = session.arena().area_key(area_frame).unwrap();
    assert_eq!(session.arena().sound(sound).unwrap().areas(), &[area]);
    assert_eq!(session.arena().area(area).unwrap().sounds, [sound_frame]);
    assert_eq!(session.owned_counts().unwrap().mission_actors, 1);
    assert_eq!(session.owned_counts().unwrap().mission_program_sources, 1);
    discard(&mut session);
}

#[test]
fn failed_derived_load_data_keeps_registration_and_pending_bytes_then_disposes_real_owners() {
    use crate::actor_frames::ModelActorFrames;
    use crate::actor_world::FrameActorData;
    use crate::actor_world::{ActorPhase, ActorSource};
    let bytes = scene2(
        &[
            chunk(0xae20, &actor_record(b"world part", 5, &[9, 8, 7])),
            chunk(0xae50, &program_record(b"must not load", b"source")),
        ]
        .concat(),
    );
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
    let mut session = fixture.session();
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Step(OpenStep::OpenBin))
    );
    let loader = session.loader().unwrap();
    let actor = loader.content.actors.world.source(ActorSource::Primary)[0].unwrap();
    let frame = loader.load_map.map.slot(b"world part").flatten().unwrap();
    assert_eq!(
        loader.content.actors.world.phase(actor),
        Ok(ActorPhase::LoadingInterrupted)
    );
    assert_eq!(
        loader.content.actors.pending_load_data.as_deref(),
        Some([9, 8, 7].as_slice())
    );
    assert!(loader.content.programs().is_empty());
    assert_eq!(
        ModelActorFrames::borrow(&mut session.arena, true)
            .actor_data(frame)
            .unwrap(),
        FrameActorData::Present(Some(actor))
    );
    let initial_failure = format!("{:?}", session.failure());
    discard(&mut session);
    assert_eq!(
        session
            .loader()
            .unwrap()
            .content
            .actors
            .world
            .inspect(actor)
            .unwrap()
            .0,
        ActorPhase::Dead
    );
    assert_eq!(format!("{:?}", session.failure()), initial_failure);
}

#[test]
fn failed_railway_init_releases_derived_allocation_and_continues_with_next_record() {
    use crate::actor_world::ActorSource;
    let bytes = scene2(&chunk(
        0xae20,
        &[
            actor_record(b"world part", 8, b"must not load for rejected dummy frame"),
            actor_record(b"world part", 1, b""),
        ]
        .concat(),
    ));
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let loader = session.loader().unwrap();
    assert_eq!(loader.content.actors.world.actor_count, 1);
    assert_eq!(
        loader
            .content
            .actors
            .world
            .source(ActorSource::Primary)
            .len(),
        1
    );
    assert_eq!(
        loader.content.actors.world.owned_actor_references().count(),
        1
    );
    discard(&mut session);
}

type PatchRow<'a> = (&'a [u8], Option<&'a [u8]>, Option<&'a [u8]>, &'a [u8]);
fn patch_file(rows: &[PatchRow<'_>]) -> Vec<u8> {
    let mut payload = Vec::new();
    for (index, (mission, name, frame, source)) in rows.iter().enumerate() {
        for value in [Some(*mission), *name, *frame, Some(*source)] {
            payload.extend((value.map_or(0, |value| value.len()) as i32).to_le_bytes());
            if let Some(value) = value {
                payload.extend(value.iter().map(|byte| byte ^ (index % 13 + 0x85) as u8));
            }
        }
    }
    chunk(0x64, &payload)
}
fn write_patch(fixture: &Fixture, file: &str, bytes: &[u8]) {
    std::fs::create_dir_all(fixture.0.join("patch")).unwrap();
    std::fs::write(fixture.0.join("patch").join(file), bytes).unwrap();
}
fn patched_scene_input() -> Vec<u8> {
    let source = program_record(b"detector", b"authored detector source");
    let data = [vec![0], 9u32.to_le_bytes().to_vec(), source[6..].to_vec()].concat();
    scene2(
        &[
            chunk(0xae20, &actor_record(b"world part", 5, &data)),
            chunk(
                0xae50,
                &program_record(b"standalone", b"authored standalone source"),
            ),
        ]
        .concat(),
    )
}
fn patch_rows(source: &'static [u8]) -> Vec<u8> {
    patch_file(&[
        (b"synthetic\0", Some(b"standalone\0"), None, source),
        (
            b"synthetic\0",
            Some(b"detector\0"),
            Some(b"world part\0"),
            source,
        ),
    ])
}
fn assert_patched_content(loader: &MissionSceneLoader, expected: &[u8]) {
    use crate::actor_world::ActorSource;
    assert_eq!(loader.content.programs().len(), 1);
    assert_eq!(
        loader.content.programs()[0].source().unwrap().c_bytes(),
        Some(expected)
    );
    let world = &loader.content.actors.world;
    let actor = world.source(ActorSource::Primary)[0].unwrap();
    assert_eq!(
        world
            .detector(actor)
            .unwrap()
            .program()
            .unwrap()
            .source()
            .unwrap()
            .c_bytes(),
        Some(expected)
    );
}

#[test]
fn startup_a4_replaces_standalone_and_detector_sources_from_one_borrowed_table() {
    use crate::program_patch::ProgramPatchStartupPhase;
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part")])),
        Some(&patched_scene_input()),
        None,
    );
    write_patch(&fixture, "a1.bin", &patch_rows(b"a1 replacement\0"));
    write_patch(&fixture, "a4.bin", &patch_rows(b"a4 replacement\0"));
    let mut session = fixture.session();
    let identity = session.startup_patches().table() as *const _;
    load(&mut session, &[]).unwrap();
    assert_eq!(session.startup_patches().table() as *const _, identity);
    assert_eq!(
        session.startup_patches().phase(),
        ProgramPatchStartupPhase::Ready
    );
    assert_eq!(session.startup_patches().table().replacements().len(), 4);
    assert_patched_content(session.loader().unwrap(), b"a4 replacement");
    discard(&mut session);
    assert_eq!(
        session.startup_patches().phase(),
        ProgramPatchStartupPhase::Unloaded
    );
    assert!(session.startup_patches().table().replacements().is_empty());
}

#[test]
fn startup_patch_failure_retains_complete_file_and_record_prefix_before_scene_creation() {
    use crate::program_patch::{ProgramPatchStartupError, ProgramPatchStartupPhase};
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    write_patch(&fixture, "a1.bin", &patch_rows(b"before failure\0"));
    // a3 admits one whole record, then reaches a malformed second record.
    let first = patch_file(&[(b"synthetic\0", None, None, b"retained a3\0")]);
    write_patch(
        &fixture,
        "a3.bin",
        &chunk(0x64, &[&first[6..], &[1, 2, 3]].concat()),
    );
    write_patch(&fixture, "a4.bin", &patch_rows(b"must not load\0"));
    let mut session = fixture.session();
    assert_eq!(load(&mut session, &[]), Err(LoadStop::Bootstrap));
    assert_eq!(session.phase(), MissionLoadPhase::StartupPatches);
    assert_eq!(
        session.startup_patches().phase(),
        ProgramPatchStartupPhase::Stopped(2)
    );
    assert_eq!(session.startup_patches().table().replacements().len(), 3);
    assert!(matches!(
        session.startup_patches().failure(),
        Some(ProgramPatchStartupError::Data {
            path: "patch/a3.bin",
            ..
        })
    ));
    assert!(session.scene().unwrap().is_none());
    assert_eq!(
        session.owned_counts().unwrap(),
        MissionOwnedCounts::default()
    );
    let failure = format!("{:?}", session.failure());
    assert_eq!(
        load(&mut session, &[]),
        Err(LoadStop::Admission(MissionLoadCallError::AlreadyEntered))
    );
    assert_eq!(format!("{:?}", session.failure()), failure);
    session.discard_terminal().unwrap();
    assert_eq!(
        session.startup_patches().phase(),
        ProgramPatchStartupPhase::Unloaded
    );
}

#[test]
fn canonical_scene_reloads_borrow_same_startup_table_without_rereading_patch_files() {
    use crate::program_patch::{ProgramPatchStartup, ProgramPatchStartupError};
    use crate::scene_render::CanonicalSceneOwner;
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"world part")])),
        Some(&patched_scene_input()),
        None,
    );
    write_patch(&fixture, "a1.bin", &patch_rows(b"old\0"));
    write_patch(&fixture, "a4.bin", &patch_rows(b"startup a4\0"));
    let installation = lhr_assets::Installation::open(&fixture.0).unwrap();
    let mut startup = ProgramPatchStartup::default();
    startup.initialize(&installation).unwrap();
    let table_identity = startup.table() as *const _;
    for time in [0, 31] {
        let mut scene = CanonicalSceneOwner::open_at_driver_time_and_patches(
            &installation,
            "synthetic",
            [800, 600],
            time,
            &startup.lease().unwrap(),
        )
        .unwrap();
        assert!(scene.frontier.is_none(), "{:?}", scene.frontier);
        assert_patched_content(scene.loader.as_ref().unwrap(), b"startup a4");
        assert_eq!(startup.table() as *const _, table_identity);
        assert_eq!(startup.table().replacements().len(), 4);
        assert_eq!(
            startup.unload_after_sources(),
            Err(ProgramPatchStartupError::SourcesRetainTable)
        );
        assert_eq!(startup.table().replacements().len(), 4);
        assert_patched_content(scene.loader.as_ref().unwrap(), b"startup a4");
        scene.close().unwrap();
        // A later mission cannot silently reload globals from changed files.
        write_patch(&fixture, "a4.bin", &patch_rows(b"later disk data\0"));
    }
    assert_eq!(
        startup.initialize(&installation),
        Err(ProgramPatchStartupError::AlreadyEntered)
    );
    assert_eq!(startup.table().replacements().len(), 4);
    startup.unload_after_sources().unwrap();
    assert!(startup.table().replacements().is_empty());
}

#[test]
fn retained_global_runtime_blocks_terminal_without_effects_until_explicit_host_discard() {
    struct Runtime;
    impl crate::original_program::RuntimeBackend for Runtime {
        fn effect(
            &mut self,
            _: crate::original_program::RuntimeView<'_>,
            _: crate::original_program::RuntimeEffect,
            _: Option<&crate::program_parameters::ScalarParameter>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    let bytes = scene2(&chunk(0xae50, &program_record(b"guard", b"dim_act 1")));
    let fixture = Fixture::new(Some(&world(&[])), Some(&bytes), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    session.global_program_game_init(&mut Runtime).unwrap();
    assert!(session
        .loader()
        .unwrap()
        .content
        .has_retained_program_runtime());
    let before = session.owned_counts().unwrap();
    let progress = *session.discard_progress();
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::ProgramRuntimeRetained)
    );
    assert_eq!(session.discard_progress(), &progress);
    assert_eq!(session.owned_counts().unwrap(), before);
    assert!(session.discard_failure().is_none());
    assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
    let reports = session.discard_global_program_runtimes_host().unwrap();
    assert_eq!(reports.len(), 1);
    assert!(!session
        .loader()
        .unwrap()
        .content
        .has_retained_program_runtime());
    discard(&mut session);
}

struct ProgressSink {
    reports: Vec<(u32, u32)>,
}
impl MissionProgressSink for ProgressSink {
    fn report_mission_progress(&mut self, progress_bits: u32, step: u32) {
        self.reports.push((progress_bits, step));
    }
}

#[test]
fn show_progress_unscaled_mode_passes_chunk_ratio_through() {
    // GAME 0x540950, mafia-re `game/mission/Mission.cpp:591-598`: raw
    // position-over-size with no `_can_show_progress_` rescale.
    assert_eq!(show_progress_value(0, 100, 0), 0.0);
    assert_eq!(show_progress_value(50, 100, 0), 0.5);
    assert_eq!(show_progress_value(100, 100, 0), 1.0);
}

#[test]
fn show_progress_empty_file_reports_full_bar() {
    // GAME 0x540950, mafia-re `game/mission/Mission.cpp:594-597`: the
    // zero-size branch reports 1.0 before the mode switch runs.
    assert_eq!(show_progress_value(0, 0, 0), 1.0);
    assert_eq!(show_progress_value(0, 0, 1), 0.2);
    assert_eq!(show_progress_value(0, 0, 2), 1.0);
}

#[test]
fn show_progress_cache_modes_rescale_into_bar_bands() {
    // GAME 0x540950, mafia-re `game/mission/Mission.cpp:598-606`: mode 1 is
    // the first fifth (`* 0.2`), mode 2 the remaining four (`* 0.8 + 0.2`).
    assert_eq!(show_progress_value(0, 100, 1), 0.0);
    assert_eq!(show_progress_value(100, 100, 1), 0.2);
    assert_eq!(show_progress_value(0, 100, 2), 0.2);
    assert_eq!(show_progress_value(100, 100, 2), 1.0);
    let mid = show_progress_value(50, 100, 2);
    assert!((mid - 0.6).abs() < f32::EPSILON, "mid-band {mid}");
}

#[test]
fn show_progress_other_modes_leave_ratio_untouched() {
    for mode in [-1, 3, 100] {
        assert_eq!(show_progress_value(50, 100, mode), 0.5, "mode {mode}");
    }
}

#[test]
fn show_progress_bits_round_trip_through_float_word() {
    // The original passes `*(uint *)&progress`; the bits must decode back to
    // the reported value (mafia-re `game/mission/Mission.cpp:607`).
    for (pos, size, mode) in [(0, 100, 0), (50, 100, 1), (50, 100, 2), (7, 0, 2)] {
        let bits = show_progress_bits(pos, size, mode);
        assert_eq!(f32::from_bits(bits), show_progress_value(pos, size, mode));
    }
}

#[test]
fn show_progress_session_reads_live_cache_mode() {
    let fixture = Fixture::new(None, None, None);
    let session = fixture.session();
    assert_eq!(session.open_cache_progress(), 0);
    let mut sink = ProgressSink {
        reports: Vec::new(),
    };
    session.show_progress_for_open(&mut sink, 50, 100);
    assert_eq!(
        sink.reports,
        vec![(0.5f32.to_bits(), MISSION_PROGRESS_STEP)]
    );
}

fn assert_load_progress_screen_contents(
    actual: &crate::game_loadsave_lifecycle::LoadProgressScreen,
    expected: &crate::game_loadsave_lifecycle::LoadProgressScreen,
) {
    assert_eq!(actual.title, expected.title);
    assert_eq!(actual.mission_part, expected.mission_part);
    assert_eq!(actual.mission_name, expected.mission_name);
    assert_eq!(
        actual.uses_native_components().unwrap(),
        expected.uses_native_components().unwrap()
    );
    assert_eq!(actual.menu_base_for_test(), expected.menu_base_for_test());
    actual
        .with_components_for_test(|menu| {
            expected
                .with_components_for_test(|expected_menu| {
                    assert_eq!(menu, expected_menu);
                    Ok(())
                })
                .unwrap();
            Ok(())
        })
        .unwrap();
}

#[test]
fn open_prepare_progress_creates_the_loading_screen_only_when_asked() {
    use crate::game_loadsave_lifecycle::LoadProgressScreen;
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    // Without the progress argument no screen is created.
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    assert_eq!(session.load_save_progress().progress, None);
    discard(&mut session);
    // With it, Open creates GM_LoadProgress(0, 0.7f, NULL).
    let mut session = fixture.session();
    load_with_progress(&mut session, &[], &mut Screen::default(), true).unwrap();
    let screen = session.load_save_progress().progress.as_ref().unwrap();
    let original_menu = screen.menu_identity_for_test();
    assert_load_progress_screen_contents(screen, &LoadProgressScreen::for_mission_open());
    // The screen is a mafiaLoadSave global: disposal keeps it.
    discard(&mut session);
    let screen = session.load_save_progress().progress.as_ref().unwrap();
    assert_eq!(screen.menu_identity_for_test(), original_menu);
    assert_load_progress_screen_contents(screen, &LoadProgressScreen::for_mission_open());
    // A live screen is kept, not replaced (the NULL test at 0x540a7f).
    let mut session = fixture.session();
    let kept = LoadProgressScreen::new(0x2cf, 0.25, Some(b"mise02".to_vec()));
    let original_menu = kept.menu_identity_for_test();
    session.load_save_progress.progress = Some(kept);
    load_with_progress(&mut session, &[], &mut Screen::default(), true).unwrap();
    let screen = session.load_save_progress().progress.as_ref().unwrap();
    assert_eq!(screen.menu_identity_for_test(), original_menu);
    assert_load_progress_screen_contents(
        screen,
        &LoadProgressScreen::new(0x2cf, 0.25, Some(b"mise02".to_vec())),
    );
    // After terminal disposal the step stops before creating a screen.
    session.load_save_progress.progress = None;
    session.open_progress_argument = true;
    discard(&mut session);
    assert!(session.prepare_progress_for_open().is_err());
    assert_eq!(session.load_save_progress().progress, None);
}

fn fixture_managed_menu_layout() -> crate::menu::MenuComponentCreateContext {
    crate::menu::MenuComponentCreateContext {
        screen_scale: [1.0, 1.0],
        left_margin_x: 0.0,
        precision: crate::menu::MenuComponentPrecision::Pc24Nearest,
    }
}

fn write_managed_menu_definition(fixture: &Fixture, filename: &str, child: (u32, [u8; 4])) {
    let directory = fixture.0.join("tables/menu");
    std::fs::create_dir_all(&directory).unwrap();
    let mut definition = b"Menu".to_vec();
    for word in [1u32, 2, 0] {
        definition.extend(word.to_le_bytes());
    }
    for (id, kind) in [(1u32, *b"cniw"), child] {
        definition.extend(id.to_le_bytes());
        definition.extend(kind);
        for coordinate in [0.0_f32, 0.0, 20.0, 5.0] {
            definition.extend(coordinate.to_le_bytes());
        }
        definition.extend(0u32.to_le_bytes());
        definition.extend(crate::main_menu::COMPONENT_VISIBLE.to_le_bytes());
    }
    std::fs::write(directory.join(filename), definition).unwrap();
}

fn write_managed_menu_texture(fixture: &Fixture, filename: &str) {
    let directory = fixture.0.join("maps");
    std::fs::create_dir_all(&directory).unwrap();
    let mut tga = vec![0; 18];
    tga[2] = 2;
    tga[12] = 1;
    tga[14] = 1;
    tga[16] = 32;
    tga[17] = 0x20;
    tga.extend([3, 2, 1, 255]);
    std::fs::write(directory.join(filename), tga).unwrap();
}

#[test]
fn startup_indicator_fonts_use_session_texture_owner_until_terminal_discard() {
    let fixture = Fixture::new(None, None, None);
    let tables = fixture.0.join("tables");
    std::fs::create_dir_all(&tables).unwrap();
    std::fs::write(tables.join("version.txt"), b"en").unwrap();
    for (language, suffix) in [("en", b"_pc"), ("cz", b"_cz")] {
        let mut data = vec![0; 56 + 256 * 2];
        data[..4].copy_from_slice(b"Lang");
        data[4..6].copy_from_slice(language.as_bytes());
        data[40..43].copy_from_slice(suffix);
        let glyph_offset = 56 + usize::from(b'A') * 2;
        data[glyph_offset..glyph_offset + 2].copy_from_slice(&23u16.to_le_bytes());
        std::fs::write(tables.join(format!("lang_{language}.def")), data).unwrap();
    }
    for font in 0..4 {
        write_managed_menu_texture(&fixture, &format!("font{font}_pc.tga"));
        let mut gmf = vec![0; 0x101c];
        gmf[..4].copy_from_slice(b"GFnt");
        gmf[8..12].copy_from_slice(&(1.0 + font as f32).to_le_bytes());
        std::fs::write(tables.join(format!("font{font}_pc.gmf")), gmf).unwrap();
    }
    let mut session = fixture.session();
    session.select_loading_menu_startup_language().unwrap();
    session.create_loading_indicator_assets().unwrap();
    let (textures, storage) = session.menu_draw_resources.cpu_texture_context().unwrap();
    let keys: Vec<_> = (0..4)
        .map(|font| {
            let view = session.menu_draw_resources.font(font).unwrap();
            assert_eq!(view.metrics.height_scale, 1.0 + font as f32);
            assert_eq!(view.glyph_map[usize::from(b'A')], 23);
            let atlas = view.atlas.unwrap();
            let surface = session.menu_draw_resources.texture_surface(atlas).unwrap();
            assert_eq!(surface.bgra, [3, 2, 1, 255]);
            let graphics = textures.texture(atlas).unwrap().graphics.unwrap();
            assert!(std::ptr::eq(surface, storage.surface(graphics).unwrap()));
            atlas
        })
        .collect();
    assert!(session.create_loading_indicator_assets().is_err());
    session.discard_terminal().unwrap();
    assert!(keys
        .iter()
        .all(|key| session.menu_draw_resources.texture_surface(*key).is_err()));
    assert!(session.create_loading_indicator_assets().is_err());
}

fn loading_side_texture(
    screen: &crate::game_loadsave_lifecycle::LoadProgressScreen,
) -> crate::itexture::TextureKey {
    screen
        .with_components_for_test(|menu| {
            Ok(*menu
                .find_component_by_id(crate::game_loadsave_lifecycle::LOADPROGRESS_SIDE)
                .unwrap()
                .texture_key()
                .unwrap()
                .unwrap())
        })
        .unwrap()
}

#[test]
fn managed_loading_menu_survives_open_and_releases_before_scene_disposal() {
    use crate::load_scene_callback::{CBM_MISSION_PROGRESS, LOAD_PROGRESS_BEGIN};
    use crate::menu_shared_resources::MenuSharedSound;
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    session
        .configure_loading_menu_layout(fixture_managed_menu_layout())
        .unwrap();
    load_with_progress(&mut session, &[], &mut Screen::default(), true).unwrap();
    assert_eq!(session.menu_shared_resources.instance_count(), 1);
    let screen = session.load_save_progress.progress.as_ref().unwrap();
    let identity = screen.menu_identity_for_test();
    assert!(screen.uses_native_components().unwrap());
    assert!(session.load_scene_callback.menu_progress().is_none());
    let sound = session
        .menu_shared_resources
        .sound(MenuSharedSound::Open)
        .unwrap()
        .unwrap();
    assert!(session.arena.sound(sound).is_ok());

    session
        .report_load_scene(
            CBM_MISSION_PROGRESS,
            0,
            LOAD_PROGRESS_BEGIN,
            &mut NoCameraRender,
        )
        .unwrap();
    assert_eq!(session.menu_shared_resources.instance_count(), 1);
    assert_eq!(
        session.menu_shared_resources.sound(MenuSharedSound::Open),
        Ok(Some(sound))
    );
    assert_eq!(
        session
            .load_save_progress
            .progress
            .as_ref()
            .unwrap()
            .menu_identity_for_test(),
        identity
    );

    discard(&mut session);
    assert_eq!(session.menu_shared_resources.instance_count(), 0);
    assert!(session.load_save_progress.progress.is_none());
    assert!(identity.upgrade().is_none());
    assert!(session.arena.sound(sound).is_err());
}

#[test]
fn managed_loading_menu_callback_rejects_overwrite_then_releases_on_end() {
    use crate::game_loadsave_lifecycle::LOADPROGRESS_SIDE;
    use crate::load_scene_callback::{
        CBM_MISSION_PROGRESS, LOAD_PROGRESS_BEGIN, LOAD_PROGRESS_END,
    };
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    write_managed_menu_definition(&fixture, "loadprogress.mnu", (LOADPROGRESS_SIDE, *b"ttcr"));
    write_managed_menu_texture(&fixture, "loadbok.tga");
    let mut session = fixture.session();
    session
        .configure_loading_menu_layout(fixture_managed_menu_layout())
        .unwrap();
    load(&mut session, &[]).unwrap();
    assert_eq!(session.menu_shared_resources.instance_count(), 0);
    session
        .report_load_scene(
            CBM_MISSION_PROGRESS,
            0,
            LOAD_PROGRESS_BEGIN,
            &mut NoCameraRender,
        )
        .unwrap();
    let screen = session.load_scene_callback().menu_progress().unwrap();
    let identity = screen.menu_identity_for_test();
    let component_texture = loading_side_texture(screen);
    assert_eq!(screen.component_count(), Ok(2));
    assert_eq!(screen.staged_mission_picture(), Ok(None));
    assert_eq!(
        session
            .menu_draw_resources
            .texture_surface(component_texture)
            .unwrap()
            .bgra,
        [3, 2, 1, 255]
    );
    assert_eq!(session.menu_shared_resources.instance_count(), 1);
    assert_eq!(
        session.report_load_scene(
            CBM_MISSION_PROGRESS,
            0,
            LOAD_PROGRESS_BEGIN,
            &mut NoCameraRender,
        ),
        Err("loading-menu BEGIN would detach an owned callback screen".into())
    );
    assert_eq!(session.menu_shared_resources.instance_count(), 1);
    assert_eq!(
        session
            .load_scene_callback
            .menu_progress()
            .unwrap()
            .menu_identity_for_test(),
        identity
    );
    session
        .report_load_scene(
            CBM_MISSION_PROGRESS,
            0,
            LOAD_PROGRESS_END,
            &mut NoCameraRender,
        )
        .unwrap();
    assert_eq!(session.menu_shared_resources.instance_count(), 0);
    assert!(identity.upgrade().is_none());
    assert!(session.load_scene_callback().menu_progress().is_none());
    assert!(session
        .menu_draw_resources
        .texture_surface(component_texture)
        .is_err());
    discard(&mut session);
}

#[test]
fn managed_loading_menu_constructor_error_keeps_shared_prefix_unpublished() {
    use crate::menu_shared_resources::{MenuSharedResourceStep, MenuSharedSound};
    let fixture = Fixture::new(None, None, None);
    let mut session = fixture.session();
    let mut layout = fixture_managed_menu_layout();
    layout.precision = crate::menu::MenuComponentPrecision::Unestablished;
    assert!(session.configure_loading_menu_layout(layout).is_err());
    session
        .configure_loading_menu_layout(fixture_managed_menu_layout())
        .unwrap();
    session.open_progress_argument = true;
    let expected = MenuSharedResourceStep::CreateSound(MenuSharedSound::Open);
    assert!(session
        .prepare_progress_for_open()
        .unwrap_err()
        .contains("CreateSound"));
    assert_eq!(
        session.menu_shared_resources.interrupted_step(),
        Some(expected)
    );
    assert_eq!(session.menu_shared_resources.instance_count(), 0);
    assert!(session.load_save_progress.progress.is_none());
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::LoadingMenus
    );
    assert_eq!(
        session.menu_shared_resources.interrupted_step(),
        Some(expected)
    );
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::AlreadyEntered)
    );
}

#[test]
fn managed_loading_menu_disposes_component_texture_after_set_on_error() {
    use crate::game_loadsave_lifecycle::LOADPROGRESS_SIDE;
    use crate::load_scene_callback::{CBM_MISSION_PROGRESS, LOAD_PROGRESS_BEGIN};
    use crate::menu_shared_resources::{
        MenuSharedResourceStep, MenuSharedResourcesError, MenuSharedSound, MenuSharedTexture,
    };

    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    write_managed_menu_definition(&fixture, "loadprogress.mnu", (LOADPROGRESS_SIDE, *b"ttcr"));
    write_managed_menu_texture(&fixture, "loadbok.tga");
    write_managed_menu_texture(&fixture, "kulicky.tga");
    let mut session = fixture.session();
    session
        .configure_loading_menu_layout(fixture_managed_menu_layout())
        .unwrap();
    load(&mut session, &[]).unwrap();
    session.open_progress_argument = true;
    session.prepare_progress_for_open().unwrap();
    let open_sound = session
        .menu_shared_resources
        .sound(MenuSharedSound::Open)
        .unwrap()
        .unwrap();
    let progress_texture = session
        .menu_shared_resources
        .texture(MenuSharedTexture::Progress)
        .unwrap()
        .unwrap();
    session
        .audio
        .release_sound(session.arena.sound(open_sound).unwrap().audio_id())
        .unwrap();

    let step = MenuSharedResourceStep::SetOn {
        sound: MenuSharedSound::Open,
        on: false,
        immediate: true,
    };
    let error = session
        .report_load_scene(
            CBM_MISSION_PROGRESS,
            0,
            LOAD_PROGRESS_BEGIN,
            &mut NoCameraRender,
        )
        .unwrap_err();
    assert!(
        error.starts_with(&format!("menu {step:?}: menu SetOn: ")),
        "{error}"
    );
    assert_eq!(session.menu_shared_resources.interrupted_step(), Some(step));
    let screen = session.load_save_progress.progress.as_ref().unwrap();
    let identity = screen.menu_identity_for_test();
    let component_texture = loading_side_texture(screen);
    assert_eq!(
        session
            .menu_draw_resources
            .texture_surface(component_texture)
            .unwrap()
            .bgra,
        [3, 2, 1, 255]
    );

    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert!(
        matches!(session.discard_failure(), Some(MissionDiscardFailure { phase: MissionDiscardPhase::LoadingMenus, cause: MissionDiscardCause::LoadingMenus(error) }) if *error == MenuSharedResourcesError::Interrupted(step).to_string())
    );
    let screen = session.load_save_progress.progress.as_ref().unwrap();
    assert_eq!(screen.menu_identity_for_test(), identity);
    assert!(!screen.uses_native_components().unwrap());
    assert!(screen.has_shared_instance());
    assert!(session
        .menu_draw_resources
        .texture_surface(component_texture)
        .is_err());
    assert!(session
        .menu_draw_resources
        .texture_surface(progress_texture)
        .is_ok());
    assert_eq!(
        session
            .menu_shared_resources
            .retained_sound(MenuSharedSound::Open),
        Some(open_sound)
    );
    assert!(session.arena.sound(open_sound).is_ok());
    assert_eq!(session.menu_shared_resources.instance_count(), 1);
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::AlreadyEntered)
    );
    assert_eq!(session.menu_shared_resources.interrupted_step(), Some(step));
}

#[test]
fn managed_loading_menu_end_keeps_release_prefix_without_replay() {
    use crate::game_loadsave_lifecycle::LOADPROGRESS_SIDE;
    use crate::load_scene_callback::{
        CBM_MISSION_PROGRESS, LOAD_PROGRESS_BEGIN, LOAD_PROGRESS_END,
    };
    use crate::menu_shared_resources::{
        MenuSharedResourceStep, MenuSharedResourcesError, MenuSharedSound, MenuSharedTexture,
    };

    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    write_managed_menu_definition(&fixture, "loadprogress.mnu", (LOADPROGRESS_SIDE, *b"ttcr"));
    write_managed_menu_texture(&fixture, "loadbok.tga");
    for name in ["papir5a.tga", "papir3.tga", "sponka.tga", "kulicky.tga"] {
        write_managed_menu_texture(&fixture, name);
    }
    let mut session = fixture.session();
    session
        .configure_loading_menu_layout(fixture_managed_menu_layout())
        .unwrap();
    load(&mut session, &[]).unwrap();
    let mut device = NoCameraRender;
    session
        .report_load_scene(CBM_MISSION_PROGRESS, 0, LOAD_PROGRESS_BEGIN, &mut device)
        .unwrap();
    let screen = session.load_scene_callback.menu_progress().unwrap();
    let identity = screen.menu_identity_for_test();
    let component_texture = loading_side_texture(screen);
    let [top, bottom, clip, progress] = MenuSharedTexture::ALL.map(|kind| {
        session
            .menu_shared_resources
            .texture(kind)
            .unwrap()
            .unwrap()
    });
    let sounds = MenuSharedSound::ALL
        .map(|kind| session.menu_shared_resources.sound(kind).unwrap().unwrap());
    assert_eq!(session.menu_shared_resources.instance_count(), 1);
    session.menu_draw_resources.release_texture(bottom).unwrap();
    for key in [component_texture, top, clip, progress] {
        assert!(session.menu_draw_resources.texture_surface(key).is_ok());
    }

    let step = MenuSharedResourceStep::ReleaseTexture(MenuSharedTexture::MenuBottom);
    let error = session
        .report_load_scene(CBM_MISSION_PROGRESS, 0, LOAD_PROGRESS_END, &mut device)
        .unwrap_err();
    assert!(
        error.starts_with(&format!(
            "menu {step:?}: menu ITexture::Release interrupted: "
        )),
        "{error}"
    );
    assert_eq!(
        session.report_load_scene(CBM_MISSION_PROGRESS, 0, LOAD_PROGRESS_END, &mut device),
        Err(MenuSharedResourcesError::InstanceNotLive.to_string())
    );
    assert_eq!(session.menu_shared_resources.interrupted_step(), Some(step));
    assert_eq!(session.menu_shared_resources.instance_count(), 0);
    assert_eq!(
        MenuSharedTexture::ALL.map(|kind| session.menu_shared_resources.retained_texture(kind)),
        [None, Some(bottom), Some(clip), Some(progress)]
    );
    assert_eq!(
        MenuSharedSound::ALL.map(|kind| session.menu_shared_resources.retained_sound(kind)),
        sounds.map(Some)
    );
    for sound in sounds {
        assert!(session.arena.sound(sound).is_ok());
    }
    for key in [component_texture, top, bottom] {
        assert!(session.menu_draw_resources.texture_surface(key).is_err());
    }
    for key in [clip, progress] {
        assert!(session.menu_draw_resources.texture_surface(key).is_ok());
    }
    let screen = session.load_scene_callback.menu_progress().unwrap();
    assert_eq!(screen.menu_identity_for_test(), identity);
    assert!(screen.has_shared_instance());
    assert!(!screen.uses_native_components().unwrap());
}

#[derive(Debug, PartialEq)]
enum OpenLoadingEffect {
    Present(crate::load_scene_callback::LoadingScreenSource, f32),
    Blend,
    RenderScene,
}

const FIXTURE_OPEN_LOADING_EFFECTS: [OpenLoadingEffect; 4] = [
    OpenLoadingEffect::Present(
        crate::load_scene_callback::LoadingScreenSource::MafiaLoadSave,
        0.0,
    ),
    OpenLoadingEffect::Blend,
    OpenLoadingEffect::Present(
        crate::load_scene_callback::LoadingScreenSource::MafiaLoadSave,
        0.35,
    ),
    OpenLoadingEffect::RenderScene,
];

#[test]
fn managed_loading_menu_uses_startup_language_and_draws_live_progress_texture() {
    use crate::load_scene_callback::{
        LoadingScreenDevice, LoadingScreenFrame, LoadingScreenPresentBackend, LoadingScreenSource,
        CBM_MISSION_PROGRESS, LOAD_PROGRESS_BEGIN,
    };
    use crate::menu::MenuComponentPrecision;
    use crate::menu_draw_resources::MenuFontLanguage;
    use crate::menu_native_drawing::{MenuNativeDrawState, MenuNativeViewport};
    use crate::menu_shared_resources::MenuSharedTexture;
    use crate::menu_text_drawing::MenuTextPrecision;
    use crate::scene_projection::SceneRenderBackend;

    #[derive(Default)]
    struct DrawProgress {
        open_effects: Vec<OpenLoadingEffect>,
        progress: Vec<(f32, crate::itexture::TextureKey)>,
        submitted: Vec<lhr_render::Mesh>,
        refuse_second_mesh: bool,
        update_progress_after_gray: Option<crate::menu_native_drawing::MenuNativeMenuIdentity>,
        ended: usize,
        finished: usize,
    }

    impl SceneRenderBackend for DrawProgress {
        fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
            let result = NoCameraRender.render_scene(arena, scene)?;
            self.open_effects.push(OpenLoadingEffect::RenderScene);
            Ok(result)
        }
    }

    impl LoadingScreenDevice for DrawProgress {
        fn set_alpha_blend_state(&mut self) -> Result<(), String> {
            self.open_effects.push(OpenLoadingEffect::Blend);
            Ok(())
        }

        fn present_loading_screen(&mut self, frame: LoadingScreenFrame<'_>) -> Result<(), String> {
            assert_eq!(frame.source, LoadingScreenSource::MafiaLoadSave);
            let draw = frame.draw_components(
                u32::MAX,
                crate::menu_text_drawing::MenuTextPrecision::Unestablished,
            )?;
            draw.result?;
            assert_eq!(draw.scene.meshes.len(), 2);
            for mesh in &draw.scene.meshes {
                assert_eq!(mesh.texture.as_ref().unwrap().rgba, [1, 2, 3, 255]);
            }
            frame.present(
                &MenuNativeDrawState::new(0, None, [0, 0]),
                MenuComponentPrecision::Unestablished,
                MenuTextPrecision::Unestablished,
                self,
            )?;
            assert_eq!(self.submitted.len(), draw.scene.meshes.len());
            for (index, (submitted, inspected)) in
                self.submitted.iter().zip(&draw.scene.meshes).enumerate()
            {
                assert_eq!(submitted.vertices.len(), inspected.vertices.len());
                assert_eq!(submitted.indices, inspected.indices);
                assert_eq!(submitted.texture.as_ref().unwrap().rgba, [1, 2, 3, 255]);
                if index == 1 && self.update_progress_after_gray.is_some() {
                    assert_eq!(inspected.vertices[3].uv, [0.7, 1.0]);
                    assert_eq!(submitted.vertices[3].uv, [0.75, 1.0]);
                    assert_eq!(submitted.vertices[3].position[0], 15.0);
                    continue;
                }
                for (submitted_vertex, inspected_vertex) in
                    submitted.vertices.iter().zip(&inspected.vertices)
                {
                    assert_eq!(submitted_vertex.position, inspected_vertex.position);
                    assert_eq!(submitted_vertex.uv, inspected_vertex.uv);
                }
            }
            let progress = frame.raw_progress()?;
            self.progress
                .push((progress, frame.progress_texture_for_test()?.unwrap()));
            self.open_effects
                .push(OpenLoadingEffect::Present(frame.source, progress));
            Ok(())
        }
    }

    impl LoadingScreenPresentBackend for DrawProgress {
        fn reset_render_props(&mut self) -> Result<(), String> {
            self.submitted.clear();
            Ok(())
        }

        fn clear_color_and_depth(&mut self, argb: u32, depth: f32) -> Result<(), String> {
            assert_eq!((argb, depth), (0, 1.0));
            Ok(())
        }

        fn begin_scene(&mut self) -> Result<(), String> {
            assert!(self.submitted.is_empty());
            Ok(())
        }

        fn set_alpha_blend_state(&mut self) -> Result<(), String> {
            assert!(self.submitted.is_empty());
            Ok(())
        }

        fn set_z_buffer(&mut self, enabled: bool) -> Result<(), String> {
            assert_eq!(self.submitted.len(), if enabled { 2 } else { 0 });
            Ok(())
        }

        fn screen_height(&mut self) -> Result<u32, String> {
            Ok(480)
        }

        fn screen_width(&mut self) -> Result<u32, String> {
            Ok(640)
        }

        fn set_viewport(&mut self, viewport: MenuNativeViewport) -> Result<(), String> {
            assert_eq!(
                viewport,
                MenuNativeViewport {
                    x: 0,
                    y: 0,
                    width: 640,
                    height: 480,
                    min_z: 0.0,
                    max_z: 1.0,
                },
            );
            Ok(())
        }

        fn draw_menu_mesh(&mut self, mesh: lhr_render::Mesh) -> Result<(), String> {
            if self.refuse_second_mesh && self.submitted.len() == 1 {
                return Err("second loading mesh refused".to_owned());
            }
            self.submitted.push(mesh);
            if self.submitted.len() == 1 {
                if let Some(identity) = &self.update_progress_after_gray {
                    identity
                        .upgrade()
                        .unwrap()
                        .with_components_mut(|components| {
                            components.set_float(crate::menu::LOADPROGRESS_BAR_ID, 0.75)
                        })
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        }

        fn draw_cursor(&mut self, _: f32, _: f32) -> Result<(), String> {
            Err("loading menu was unexpectedly active".to_owned())
        }

        fn end_scene(&mut self) -> Result<(), String> {
            assert_eq!(self.submitted.len(), 2);
            self.ended += 1;
            Ok(())
        }

        fn present_scene(&mut self) -> Result<(), String> {
            self.finished += 1;
            Ok(())
        }
    }

    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    write_managed_menu_definition(
        &fixture,
        "loadprogress_pl.mnu",
        (crate::menu::LOADPROGRESS_BAR_ID, *b"rabp"),
    );
    std::fs::write(fixture.0.join("tables/version.txt"), b"pl").unwrap();
    for language in ["pl", "cz"] {
        let mut data = vec![0; 56 + 256 * 2];
        data[..4].copy_from_slice(b"Lang");
        data[4..6].copy_from_slice(language.as_bytes());
        std::fs::write(fixture.0.join(format!("tables/lang_{language}.def")), data).unwrap();
    }
    write_managed_menu_texture(&fixture, "kulicky.tga");

    let mut session = fixture.session();
    session.select_loading_menu_startup_language().unwrap();
    assert_eq!(session.indicators.as_ref().unwrap().get_language(), 0x6c70);
    assert_eq!(
        &session
            .menu_draw_resources
            .language(MenuFontLanguage::Credits)
            .unwrap()
            .raw_header[4..6],
        b"cz"
    );
    session
        .configure_loading_menu_layout(fixture_managed_menu_layout())
        .unwrap();
    let mut renderer = DrawProgress::default();
    let result = session.open_source_in_order(
        &mut crate::mission_open_order::OpenAttempt::default(),
        "synthetic",
        true,
        &[],
        projection(),
        &mut Screen::default(),
        &mut renderer,
    );
    assert_eq!(result, Ok(crate::mission_open_order::OpenResult::Opened));
    assert_eq!(renderer.open_effects, FIXTURE_OPEN_LOADING_EFFECTS);
    let key = session
        .menu_shared_resources
        .texture(MenuSharedTexture::Progress)
        .unwrap()
        .unwrap();
    assert!(!renderer.progress.is_empty());
    assert!(renderer
        .progress
        .iter()
        .all(|&(_, drawn_key)| drawn_key == key));
    let screen = session.load_save_progress.progress.as_ref().unwrap();
    let identity = screen.menu_identity_for_test();
    let progress = screen.raw_progress().unwrap();
    let presented = renderer.progress.len();
    assert_eq!(
        session.report_load_scene(CBM_MISSION_PROGRESS, 0, LOAD_PROGRESS_BEGIN, &mut renderer),
        Err("loading-menu Create would detach live components".into())
    );
    let screen = session.load_save_progress.progress.as_ref().unwrap();
    assert_eq!(screen.menu_identity_for_test(), identity);
    assert_eq!(screen.raw_progress(), Ok(progress));
    assert_eq!(session.menu_shared_resources.instance_count(), 1);
    assert_eq!(renderer.progress.len(), presented);
    session
        .player_navi_for_init_presented(&mut |frame| renderer.present_loading_screen(frame))
        .unwrap();
    assert_eq!(
        renderer.progress.get(presented..),
        Some(&[(0.7f32, key)][..])
    );
    assert_eq!(renderer.finished, presented + 1);
    renderer.refuse_second_mesh = true;
    let error = session
        .player_navi_for_init_presented(&mut |frame| renderer.present_loading_screen(frame))
        .unwrap_err();
    assert!(error.contains("Components"), "{error}");
    assert!(error.contains("second loading mesh refused"), "{error}");
    assert_eq!(renderer.submitted.len(), 1);
    assert_eq!(renderer.ended, presented + 1);
    assert_eq!(renderer.finished, presented + 1);
    assert_eq!(renderer.progress.len(), presented + 1);
    assert_eq!(
        session
            .load_save_progress
            .progress
            .as_ref()
            .unwrap()
            .menu_identity_for_test(),
        identity
    );
    renderer.refuse_second_mesh = false;
    renderer.update_progress_after_gray = Some(identity);
    session
        .player_navi_for_init_presented(&mut |frame| renderer.present_loading_screen(frame))
        .unwrap();
    assert_eq!(renderer.progress.last(), Some(&(0.75, key)));
    assert_eq!(renderer.ended, presented + 2);
    assert_eq!(renderer.finished, presented + 2);
    discard(&mut session);
    assert_eq!(
        session
            .menu_shared_resources
            .texture(MenuSharedTexture::Progress),
        Ok(None)
    );
}

fn english_managed_loading_fixture() -> Fixture {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    write_managed_menu_definition(
        &fixture,
        "loadprogress_en.mnu",
        (crate::menu::LOADPROGRESS_BAR_ID, *b"rabp"),
    );
    std::fs::write(fixture.0.join("tables/version.txt"), b"en").unwrap();
    let mut language = vec![0; 56 + 256 * 2];
    language[..6].copy_from_slice(b"Langen");
    std::fs::write(fixture.0.join("tables/lang_en.def"), language).unwrap();
    write_managed_menu_texture(&fixture, "kulicky.tga");
    fixture
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a Windows desktop and a window-compatible GPU"]
fn managed_loading_open_and_init_dispatch_on_one_surface_with_explicit_recovery() {
    use crate::game_init_order::{InitAttempt, InitRunError, InitStep, PresentedFrame};
    use crate::load_scene_callback::{
        LoadingScreenDevice, LoadingScreenFrame, LoadingScreenSource,
    };
    use crate::loading_surface::{
        tests::with_swapchain, LoadingOpenSceneBackend, LoadingSurface, OpenLoadingSurface,
    };
    use crate::menu::MenuComponentPrecision;
    use crate::menu_native_drawing::MenuNativeDrawState;
    use crate::menu_text_drawing::MenuTextPrecision;
    use crate::scene_projection::SceneRenderBackend;
    use crate::scene_render::SceneSnapshotSequence;
    use std::cell::RefCell;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    struct OpenScene;

    impl SceneRenderBackend for OpenScene {
        fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
            NoCameraRender.render_scene(arena, scene)
        }
    }

    impl LoadingOpenSceneBackend for OpenScene {
        fn set_loading_step_alpha_blend(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    struct RecordedOpen<'adapter, 'host, 'window, 'cursor> {
        device: OpenLoadingSurface<'adapter, 'host, 'window, 'cursor>,
        effects: Vec<OpenLoadingEffect>,
    }

    impl SceneRenderBackend for RecordedOpen<'_, '_, '_, '_> {
        fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
            let result = self.device.render_scene(arena, scene)?;
            self.effects.push(OpenLoadingEffect::RenderScene);
            Ok(result)
        }
    }

    impl LoadingScreenDevice for RecordedOpen<'_, '_, '_, '_> {
        fn set_alpha_blend_state(&mut self) -> Result<(), String> {
            self.device.set_alpha_blend_state()?;
            self.effects.push(OpenLoadingEffect::Blend);
            Ok(())
        }

        fn present_loading_screen(&mut self, frame: LoadingScreenFrame<'_>) -> Result<(), String> {
            let effect = OpenLoadingEffect::Present(frame.source, frame.raw_progress()?);
            self.device.present_loading_screen(frame)?;
            self.effects.push(effect);
            Ok(())
        }
    }

    fn cursor_mesh(cursor_x: f32, cursor_y: f32) -> lhr_render::Mesh {
        lhr_render::Mesh {
            vertices: [
                [cursor_x, cursor_y, 0.0],
                [cursor_x + 8.0, cursor_y, 0.0],
                [cursor_x, cursor_y + 8.0, 0.0],
            ]
            .into_iter()
            .map(|position| lhr_render::Vertex {
                position,
                uv: [0.0; 2],
            })
            .collect(),
            indices: vec![0, 1, 2],
            texture: None,
            material: lhr_render::RenderMaterial::OriginalNull {
                color: [1.; 4],
                sort_center: [0.; 3],
                visual_z_bias: false,
            },
        }
    }

    with_swapchain(|window, swapchain, renderer, config| {
        let fixture = english_managed_loading_fixture();
        let mut session = fixture.session();
        init_presentation::adopt_fresh_install_settings(&mut session);
        session.select_loading_menu_startup_language().unwrap();
        session
            .configure_loading_menu_layout(fixture_managed_menu_layout())
            .unwrap();
        let globals = MenuNativeDrawState::new(0, None, [9, 17]);
        let presenter = || {
            LoadingSurface::new(
                window,
                swapchain,
                renderer,
                &globals,
                MenuComponentPrecision::Pc24Nearest,
                MenuTextPrecision::Unestablished,
            )
        };
        let mut cursor_calls = vec![];
        let mut cursor = |x, y| {
            cursor_calls.push([x, y]);
            assert!(swapchain
                .configure(config)
                .unwrap_err()
                .contains("active operation"));
            Err("synthetic cursor failure".into())
        };
        let loading = presenter();
        let mut scene = OpenScene;
        let mut open = RecordedOpen {
            device: loading.for_open(&mut scene, Some(&mut cursor)),
            effects: Vec::new(),
        };
        let result = session.open_source_in_order(
            &mut crate::mission_open_order::OpenAttempt::default(),
            "synthetic",
            true,
            &[],
            projection(),
            &mut Screen::default(),
            &mut open,
        );
        assert_eq!(result, Ok(crate::mission_open_order::OpenResult::Opened));
        assert_eq!(open.effects, FIXTURE_OPEN_LOADING_EFFECTS);
        assert_eq!(
            session
                .load_save_progress()
                .progress
                .as_ref()
                .unwrap()
                .component_count(),
            Ok(2)
        );

        let identity = session
            .load_save_progress()
            .progress
            .as_ref()
            .unwrap()
            .menu_identity_for_test();
        globals.set_active_menu_window(Some(identity));
        let init_effects = RefCell::new(Vec::new());
        let mut init_cursor = |cursor_x, cursor_y| {
            assert_eq!([cursor_x, cursor_y], [9., 17.]);
            init_effects.borrow_mut().push("Init cursor");
            Ok(cursor_mesh(cursor_x, cursor_y))
        };
        let mut sequence = SceneSnapshotSequence::default();
        let target = [config.width, config.height];
        let result = InitAttempt::default().run_with_presentation(
            &mut session,
            target,
            &mut sequence,
            |frame| {
                let loading = if let PresentedFrame::LoadingScreen(frame) = &frame {
                    assert_eq!(frame.source, LoadingScreenSource::MafiaLoadSave);
                    assert_eq!(frame.raw_progress(), Ok(0.7));
                    true
                } else {
                    false
                };
                presenter().present_init(frame, Some(&mut init_cursor), |snapshot| {
                    assert_eq!(snapshot.frame.viewport, [0, 0, target[0], target[1]]);
                    init_effects.borrow_mut().push("scene callback");
                    Ok(())
                })?;
                if loading {
                    init_effects.borrow_mut().push("loading present");
                }
                Ok(())
            },
        );
        let Err(InitRunError::Callback(failure)) = result else {
            panic!("the next unavailable owner must stop Init");
        };
        assert_eq!(failure.step, InitStep::EffectsStart, "{}", failure.error);
        assert_eq!(
            failure.error.0,
            format!("EffectsStart: {EFFECTS_START_STOP}")
        );
        assert_eq!(
            *init_effects.borrow(),
            ["scene callback", "Init cursor", "loading present"]
        );
        assert_eq!(sequence.next_number(), Ok(2));
        assert!(swapchain
            .present_redraw(window, renderer, config, |texture| {
                assert_eq!([texture.width(), texture.height()], target);
                let view = texture.create_view(&Default::default());
                let encoder = renderer.render_screen(
                    &view,
                    target[0],
                    target[1],
                    [target[0] as f32, target[1] as f32],
                );
                Ok(vec![encoder.finish()])
            })
            .unwrap());

        let error = session
            .player_navi_for_init_presented(&mut |frame| presenter().present(frame, None))
            .unwrap_err();
        assert!(
            error.contains("loading cursor mesh source is unavailable"),
            "{error}"
        );
        let mut unexpected_cursor = |_, _| panic!("blocked frame reached cursor");
        let mut assert_blocked = |session: &mut MissionLoadSession| {
            let error = session
                .player_navi_for_init_presented(&mut |frame| {
                    presenter().present(frame, Some(&mut unexpected_cursor))
                })
                .unwrap_err();
            assert!(error.contains("BeginScene: loading swapchain requires successful configuration before acquire"), "{error}");
        };
        assert_blocked(&mut session);
        let error = swapchain
            .present_redraw(window, renderer, config, |_| {
                panic!("aborted loading frame did not block redraw")
            })
            .unwrap_err();
        assert!(
            error.contains("requires successful configuration before acquire"),
            "{error}"
        );

        let mut invalid = config.clone();
        invalid.width = 0;
        let error = swapchain.configure(&invalid).unwrap_err();
        assert!(error.starts_with("configure loading swapchain:"), "{error}");
        assert_blocked(&mut session);
        swapchain.configure(config).unwrap();

        let error = session
            .player_navi_for_init_presented(&mut |frame| {
                presenter().present(frame, Some(&mut cursor))
            })
            .unwrap_err();
        assert!(error.contains("synthetic cursor failure"), "{error}");
        assert_eq!(cursor_calls, [[9., 17.]]);
        assert_blocked(&mut session);
        swapchain.configure(config).unwrap();

        let mut panicking_cursor = |x, y| {
            assert_eq!([x, y], [9., 17.]);
            panic!("synthetic cursor panic");
        };
        let payload = catch_unwind(AssertUnwindSafe(|| {
            session.player_navi_for_init_presented(&mut |frame| {
                presenter().present(frame, Some(&mut panicking_cursor))
            })
        }))
        .unwrap_err();
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"synthetic cursor panic")
        );
        assert_blocked(&mut session);
        swapchain.configure(config).unwrap();

        let mut visible_cursor_calls = vec![];
        let mut visible_cursor = |x, y| {
            visible_cursor_calls.push([x, y]);
            Ok(cursor_mesh(x, y))
        };
        session
            .player_navi_for_init_presented(&mut |frame| {
                presenter().present(frame, Some(&mut visible_cursor))
            })
            .unwrap();
        assert_eq!(visible_cursor_calls, [[9., 17.]]);
        discard(&mut session);
    });
}
