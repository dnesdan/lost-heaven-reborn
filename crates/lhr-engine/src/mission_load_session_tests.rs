//! Synthetic composition tests over actual scene, actor and source owners.
use super::*;
use crate::frame_transform::FrameKey;
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
        MissionLoadSession::new(
            lhr_assets::Installation::open(&self.0).unwrap(),
            Pc24PrecisionState::new(false),
        )
        .unwrap()
    }
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

fn request<'a>(directories: &'a [&'a [u8]]) -> MissionLoadRequest<'a> {
    MissionLoadRequest {
        world_input: "missions/synthetic/scene.i3d",
        scene2_path: "missions/synthetic/scene2.bin",
        model_directories: directories,
        projection: GameCameraProjection {
            global_mode: 0,
            screen_flag_46f4: false,
        },
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
fn load(
    session: &mut MissionLoadSession,
    directories: &[&[u8]],
) -> Result<(), MissionLoadCallError> {
    session.load(request(directories), &mut Screen::default())
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
    assert_eq!(session.phase(), MissionLoadPhase::World);
    assert_eq!(
        session.world_progress().unwrap().phase(),
        CpuSceneWorldPhase::Input
    );
    let scene = session.scene().unwrap();
    assert_eq!(session.driver().scenes(), &[scene]);
    assert!(session.arena().scene(scene).unwrap().frames().is_empty());
    assert!(session.loader().is_none());
    assert_eq!(session.scene2_byte_count(), None);
    let error = format!("{:?}", session.failure().unwrap());
    let before = session.owned_counts().unwrap();
    assert_eq!(
        load(&mut session, &[]),
        Err(MissionLoadCallError::AlreadyEntered)
    );
    assert_eq!(session.owned_counts().unwrap(), before);
    assert_eq!(format!("{:?}", session.failure().unwrap()), error);
    discard(&mut session);
    assert!(session.arena().scene(scene).is_err());
    assert_eq!(format!("{:?}", session.failure().unwrap()), error);
    assert_eq!(
        load(&mut session, &[]),
        Err(MissionLoadCallError::TerminalDiscardEntered)
    );
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::AlreadyEntered)
    );
}

#[test]
fn missing_scene2_retains_world_transfer_and_actual_name_map_without_camera_creation() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"world part")])), None, None);
    let mut session = fixture.session();
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
    assert_eq!(session.phase(), MissionLoadPhase::Scene2Input);
    let progress = session.world_progress().unwrap();
    assert_eq!(progress.phase(), CpuSceneWorldPhase::Complete);
    assert_eq!(progress.staging_model(), None);
    assert_eq!(progress.transferred_frames(), 1);
    let frame = progress.frames()[0];
    let live = session.arena().lookup_reference(frame);
    assert_eq!(
        session.load_map().unwrap().map.slot(b"world part"),
        Some(Some(live))
    );
    assert_eq!(session.load_map().unwrap().publications.len(), 4);
    assert!(session.loader().unwrap().camera.is_none());
    assert_eq!(session.loader().unwrap().cursor, 0);
    let scene = session.scene().unwrap();
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
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
            .scene_active_camera(session.scene().unwrap())
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
    let scene = session.scene().unwrap();
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
        Err(MissionLoadCallError::Stopped)
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
    let scene = session.scene().unwrap();
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
        Err(MissionLoadCallError::TerminalDiscardEntered)
    );
    // Rust Drop now releases this synthetic host storage. It is not reported
    // as a completed original cache destructor or native exception rollback.
}

#[test]
fn load_map_failure_keeps_the_published_prefix_without_creating_a_camera() {
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"first"), dummy(1, &[0xe8])])),
        Some(&scene2(&[])),
        None,
    );
    let mut session = fixture.session();
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
    assert_eq!(session.phase(), MissionLoadPhase::LoadMap);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::LoadMap(_)
    ));
    assert!(session.loader().is_none());
    assert_eq!(session.load_map().unwrap().publications.len(), 1);
    assert_eq!(session.load_map().unwrap().publications[0].name, b"first");
    assert_eq!(
        session.load_map().unwrap().map.slot(b"Primary sector"),
        None
    );
    assert_eq!(session.scene2_byte_count(), None);
    assert_eq!(
        session
            .arena()
            .scene_active_camera(session.scene().unwrap())
            .unwrap(),
        None
    );
    discard(&mut session);
}

#[test]
fn failed_world_geometry_uses_actual_staging_disposal_before_scene_release() {
    let fixture = Fixture::new(Some(&world(&[sector()])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    session
        .arena
        .sector_storage_mut()
        .fail_on_allocation(Some(1));
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
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
    let scene = session.scene().unwrap();
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
    let scene = session.scene().unwrap();
    assert_eq!(session.driver().scenes(), &[scene]);
    assert_eq!(session.owned_counts().unwrap().driver_listeners, 1);
    assert_eq!(session.owned_counts().unwrap().audio_sounds, 0);
    assert!(!session.owned_counts().unwrap().audio_listener_pose);
    discard(&mut session);
    assert!(session.arena().scene(scene).is_err());
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
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
        session.load(request(&[]), &mut screen),
        Err(MissionLoadCallError::Stopped)
    );
    assert_eq!(screen.calls, 1);
    assert!(matches!(
        session.failure().unwrap().cause,
        MissionLoadCause::SceneCreation {
            phase: DriverSceneCreatePhase::Constructor,
            ..
        }
    ));
    let scene = session.scene().unwrap();
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
            first.scene().unwrap()
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
    let scene = session.scene().unwrap();
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
    let scene = session.scene().unwrap();
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
    // not another MissionLoadSession::load admission.
    let scene = session.scene().unwrap();
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
            chunk(0xae20, &actor_record(b"second", 2, b"")),
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("wrong failure");
    };
    assert_eq!(error.entry, 0x544a16);
    assert_eq!(error.cursor, kind.offset + 10);
    assert!(error.detail.contains("UnsupportedKind(2)"));
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
fn padded_kind_field_is_closed_before_factory_failure_and_preserves_registered_prefix() {
    use crate::actor_world::ActorSource;
    let unavailable = chunk(
        0xae21,
        &[
            chunk(0xae23, b"second\0"),
            chunk(
                0xae22,
                &[2u32.to_le_bytes().as_slice(), b"unread padding"].concat(),
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
    let MissionLoadCause::Scene2Prefix(error) = &session.failure().unwrap().cause else {
        panic!("wrong failure");
    };
    assert_eq!(error.entry, 0x544a16);
    assert_eq!(error.cursor, kind.offset + kind.raw.len());
    assert!(error.detail.contains("UnsupportedKind(2)"));
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
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
    assert_eq!(load(&mut session, &[]), Err(MissionLoadCallError::Stopped));
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
