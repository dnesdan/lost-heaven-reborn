//! Headless host probe over the same owners as MissionLoadSession.
//! This command never starts a menu, actor scheduler, or mission-completion gate.

use lhr_engine::{
    game_camera_projection::GameCameraProjection,
    host_scene_adapters::InspectionScreen,
    mission_load_session::{MissionLoadCause, MissionLoadRequest, MissionLoadSession},
    native_precision::Pc24PrecisionState,
};
use std::{fmt::Write as _, path::Path};

/// Deliberate diagnostic inputs, not recovered GameInit globals. A caller that
/// needs a different original prestate should use MissionLoadSession directly.
const PROBE_SIZE: [i32; 2] = [800, 600];

pub(super) fn validate_mission_name(mission: &str) -> Result<(), String> {
    if mission.is_empty()
        || mission.len() > 64
        || !mission
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(
            "Mission name must contain 1..=64 ASCII letters, digits, underscores or hyphens".into(),
        );
    }
    Ok(())
}

pub(super) fn validate_inputs(mission: &str, model_directories: &[String]) -> Result<(), String> {
    validate_mission_name(mission)?;
    for directory in model_directories {
        if directory.is_empty() || !directory.is_ascii() || directory.contains('\0') {
            return Err("Probe model directories must be nonempty ASCII paths without NUL".into());
        }
    }
    Ok(())
}

pub fn run(root: &Path, mission: &str, model_directories: &[String]) -> Result<(), String> {
    // Keep this validation at the command boundary even if a future caller
    // bypasses the main option parser. Mission paths cannot escape their root.
    validate_inputs(mission, model_directories)?;
    let installation = lhr_assets::Installation::open(root).map_err(|error| error.to_string())?;
    let mut session = MissionLoadSession::new(installation, Pc24PrecisionState::new(false))
        .map_err(|error| format!("Mission probe initialization failed: {error:?}"))?;
    let world_input = format!("missions/{mission}/scene.i3d");
    let scene2_path = format!("missions/{mission}/scene2.bin");
    let directories: Vec<&[u8]> = model_directories
        .iter()
        .map(|path| path.as_bytes())
        .collect();
    let mut screen = InspectionScreen {
        size: PROBE_SIZE,
        viewport: [0, 0, PROBE_SIZE[0], PROBE_SIZE[1]],
    };
    let load = session.load(
        MissionLoadRequest {
            world_input: &world_input,
            scene2_path: &scene2_path,
            model_directories: &directories,
            projection: GameCameraProjection {
                global_mode: 0,
                screen_flag_46f4: false,
            },
        },
        &mut screen,
    );

    // Build the evidence before terminal disposal makes frame identities
    // historical. Always attempt genuine disposal before emitting the report;
    // an output failure must not short-circuit ownership cleanup.
    let mut report = String::new();
    writeln!(report, "Mission load probe: {mission}").unwrap();
    writeln!(
        report,
        "World input: {world_input}; scene2 input: {scene2_path}"
    )
    .unwrap();
    writeln!(report, "Host profile: 800x600, full viewport; projection mode=0, screen flag=false; requested headless Rust PCM backend; initial PC24 precision=false").unwrap();
    writeln!(
        report,
        "Sound cache AddDir input: Sounds (stored as Sounds\\); model cache AddDir inputs in order: {model_directories:?}"
    )
    .unwrap();
    writeln!(
        report,
        "An empty list installs no implicit model search directory."
    )
    .unwrap();
    append_load_report(&mut report, &session);
    writeln!(
        report,
        "Owners before terminal disposal: {:?}",
        session.owned_counts()
    )
    .unwrap();
    let discard = session.discard_terminal();
    writeln!(
        report,
        "Terminal disposal: {:?}",
        session.discard_progress()
    )
    .unwrap();
    writeln!(
        report,
        "Scene CPU Close: {:?}",
        session.scene_close_report()
    )
    .unwrap();
    if let Some(failure) = session.discard_failure() {
        writeln!(
            report,
            "Disposal failure (separate from load failure): {failure:?}"
        )
        .unwrap();
    }
    writeln!(
        report,
        "Owners after terminal disposal: {:?}",
        session.owned_counts()
    )
    .unwrap();
    writeln!(report, "Gameplay acceptance: NOT EVALUATED. This probe does not execute MissionOpen, actor installation, scripts, objectives or mission completion.").unwrap();
    {
        use std::io::Write as _;
        std::io::stdout()
            .lock()
            .write_all(report.as_bytes())
            .map_err(|error| format!("Could not write mission probe report: {error}"))?;
    }
    match (load, discard) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(_), Ok(())) => Err("Mission load probe stopped at the reported load boundary; terminal disposal completed".into()),
        (Ok(()), Err(_)) => Err("Mission prefix completed, but terminal disposal stopped at the reported ownership boundary".into()),
        (Err(_), Err(_)) => Err("Mission load and terminal disposal both stopped; their first failures are reported separately".into()),
    }
}

fn append_load_report(report: &mut String, session: &MissionLoadSession) {
    writeln!(report, "Load phase: {:?}", session.phase()).unwrap();
    writeln!(
        report,
        "Host PCM backend bound to driver: {}",
        session.driver().host_sound_backend().is_some()
    )
    .unwrap();
    if let Some(world) = session.world_progress() {
        writeln!(report, "World: {:?}; {} source frames, {} transferred, {} prepared sectors; staging model retained={}",
            world.phase(), world.frames().len(), world.transferred_frames(),
            world.prepared_sectors(), world.staging_model().is_some()).unwrap();
    }
    if let Some(map) = session.load_map() {
        writeln!(report, "Load-map publications: {}", map.publications.len()).unwrap();
    }
    if let Some(loader) = session.loader() {
        writeln!(report, "Scene2: cursor={} / {:?} bytes; completed objects={}; current kind={:#x}; current frame retained={}; camera retained={}",
            loader.cursor, session.scene2_byte_count(), loader.finished_objects,
            loader.current_kind, loader.current_frame.is_some(), loader.camera.is_some()).unwrap();
        writeln!(
            report,
            "Pending scene2 effects: model_open={}, occluder_geometry={}, light_sector_link={}, visual_creator={}, factory_creator={}",
            loader.pending_model_open.is_some(),
            loader.pending_occluder_geometry.is_some(),
            loader.pending_light_sector_link.is_some(),
            loader.pending_visual_replacement.is_some(),
            loader.pending_factory_creator.is_some()
        )
        .unwrap();
    }
    if let Some(progress) = session.caches().model_open_progress() {
        writeln!(report, "Model cache: phase={:?}; directory index={}; cached source retained={}; native ModelOpen result={:?}",
            progress.phase, progress.directory, progress.cached_source.is_some(),
            progress.model_open_result).unwrap();
    }
    if let Some(progress) = session.caches().sound_open_progress() {
        writeln!(report, "Sound cache: phase={:?}; name={:?}; flags={:#x}; directory index={}; attempted path={:?}; cached source retained={}; sound Open result={:?}; cache interrupted={}",
            progress.phase, progress.name, progress.flags, progress.directory,
            progress.path, progress.cached_source.is_some(), progress.sound_open_result,
            session.caches().is_interrupted(lhr_engine::mission_resources::MissionCacheKind::Sound)).unwrap();
    }
    match session.audio().load_failures() {
        Ok(failures) => {
            for failure in failures {
                writeln!(report, "Host PCM load failure: {failure:?}").unwrap();
            }
        }
        Err(error) => {
            writeln!(
                report,
                "Host PCM load failure diagnostics unavailable: {error:?}"
            )
            .unwrap();
        }
    }
    writeln!(
        report,
        "Retained PC24 precision flag: {}",
        session.precision_state().precision_raised()
    )
    .unwrap();
    if let Some(failure) = session.failure() {
        match &failure.cause {
            MissionLoadCause::Scene2Prefix(error) => {
                writeln!(
                    report,
                    "First load failure: phase={:?}; cursor={}; reached entry={:#010x}; {}",
                    failure.phase, error.cursor, error.entry, error.detail
                )
                .unwrap();
            }
            _ => writeln!(report, "First load failure: {failure:?}").unwrap(),
        }
    } else {
        writeln!(
            report,
            "Only the admitted prefix completed; this is not a playable mission result."
        )
        .unwrap();
    }
}
