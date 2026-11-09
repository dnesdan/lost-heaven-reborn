//! Headless host probe over the same owners as MissionLoadSession.
//! This command never starts a menu, actor scheduler, or mission-completion gate.

use lhr_engine::{
    actor_world::ActorWorld,
    game_camera_projection::GameCameraProjection,
    game_init_order::{InitAttempt, InitRunError},
    host_scene_adapters::InspectionScreen,
    mission_content_loading::{
        GameGlobalProgramWorldRunner, MissionContentOwner, MissionProgramDiagnostic,
        MissionProgramWorldContext, NamedGlobalProgramOutcome, ProgramWorldLocales,
    },
    mission_load_session::{MissionLoadCause, MissionLoadRequest, MissionLoadSession},
    mission_world_completeness,
    model_loading::ModelArena,
    native_precision::Pc24PrecisionState,
    original_program::{RuntimeBackend, RuntimeEffect, RuntimeView},
    program_parameters::ScalarParameter,
    script_lookup::LookupError,
    script_scheduler::{ActorReferenceId, DispatchBudget, FrameReferenceId},
};
use std::{fmt::Write as _, path::Path};

/// Deliberate diagnostic inputs, not recovered GameInit globals. A caller that
/// needs a different original prestate should use MissionLoadSession directly.
const PROBE_SIZE: [i32; 2] = [800, 600];
const PROBE_RENDER_SIZE: [u32; 2] = [PROBE_SIZE[0] as u32, PROBE_SIZE[1] as u32];
/// Bounded dispatch budget for the single `GameInitStart` run. The VM loop
/// always terminates within it or reports budget exhaustion; nothing here
/// depends on the exact count, only on its finiteness.
const PROBE_DISPATCH_BUDGET: usize = 100_000;

/// Counting accept-all script backend for the probe. Every reached effect
/// boundary succeeds and is counted; the runtime owns all allocations.
/// This instruments scheduling, it does not implement gameplay effects.
#[derive(Default)]
struct ProbeEffects {
    calls: usize,
}

impl RuntimeBackend for ProbeEffects {
    fn effect(
        &mut self,
        _: RuntimeView<'_>,
        _: RuntimeEffect,
        _: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        self.calls += 1;
        Ok(())
    }
}

/// Headless diagnostic: counts message beeps without a listener to beep
/// through. A nonzero count stays visible in the report instead of masking
/// a gameplay event.
#[derive(Default)]
struct ProbeDiagnostic {
    beeps: usize,
}

impl MissionProgramDiagnostic for ProbeDiagnostic {
    fn message_beep(
        &mut self,
        _: &mut ActorWorld,
        _: &mut ModelArena,
        _: RuntimeView<'_>,
        _: &mut Option<ActorReferenceId>,
        _: &mut FrameReferenceId,
        _: i32,
    ) -> Result<(), LookupError> {
        self.beeps += 1;
        Ok(())
    }
}

/// Empty game-global runner: the probe ships no game programs, so the
/// mission fallback always decides. Returning `false` is the absence
/// branch, never a handled claim.
struct ProbeGame;

impl<B: RuntimeBackend, D: MissionProgramDiagnostic> GameGlobalProgramWorldRunner<B, D>
    for ProbeGame
{
    fn run_game_global(
        &mut self,
        _: &mut MissionContentOwner,
        _: &[u8],
        _: &mut DispatchBudget,
        _: &mut MissionProgramWorldContext<'_, B, D>,
    ) -> Result<bool, String> {
        Ok(false)
    }
}

struct ScriptRunReport {
    outcome: NamedGlobalProgramOutcome,
    effect_calls: usize,
    beeps: usize,
    budget_remaining: usize,
}

/// Owned program init plus the single `GameInitStart` run over the live
/// session owners, in `C_game::Init` order after the scalar prefix. Runtimes
/// are discarded host-side afterwards so terminal disposal sees no retained
/// program state; this is Init, not Done.
fn run_game_init_scripts(session: &mut MissionLoadSession) -> Result<ScriptRunReport, String> {
    let mut effects = ProbeEffects::default();
    let mut diagnostic = ProbeDiagnostic::default();
    let mut game = ProbeGame;
    let mut budget =
        DispatchBudget::new(PROBE_DISPATCH_BUDGET).map_err(|error| format!("{error:?}"))?;
    // Program runtimes must be discarded before reporting no matter how the
    // run below ends; otherwise terminal disposal refuses the retained state
    // and masks the real outcome.
    let init = session
        .global_program_game_init(&mut effects)
        .map_err(|error| format!("{error:?}"));
    let outcome = match &init {
        Ok(()) => Some(
            session
                .global_program_run_in_world(
                    b"GameInitStart",
                    &mut game,
                    &mut budget,
                    &mut effects,
                    &mut diagnostic,
                    ProgramWorldLocales {
                        game_default_c_locale: true,
                        ls3df_default_c_locale: true,
                    },
                )
                .map_err(|error| format!("{error:?}")),
        ),
        Err(_) => None,
    };
    let discard = session
        .discard_global_program_runtimes_host()
        .map_err(|error| format!("{error:?}"));
    let outcome = match (init, outcome, discard) {
        (Ok(()), Some(Ok(outcome)), Ok(_)) => Ok(outcome),
        (Err(error), _, Ok(_)) => Err(error),
        (Ok(()), Some(Err(error)), Ok(_)) => Err(error),
        (Ok(()), _, Err(error)) => Err(format!("runtimes discard: {error}")),
        (Err(init), _, Err(discard)) => Err(format!("{init}; runtimes discard: {discard}")),
        (Ok(()), None, _) => Err("unreachable GameInitStart admission".into()),
    }?;
    Ok(ScriptRunReport {
        outcome,
        effect_calls: effects.calls,
        beeps: diagnostic.beeps,
        budget_remaining: budget.remaining(),
    })
}

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

fn capture_retained_mission_frame(
    snapshot: &lhr_engine::scene_render::CanonicalSceneSnapshot,
    path: &Path,
) -> Result<(), String> {
    let instance = lhr_render::instance();
    let mut renderer = pollster::block_on(lhr_render::Renderer::new(
        &instance,
        None,
        &snapshot.scene,
        wgpu::TextureFormat::Rgba8Unorm,
        false,
    ))?;
    for (index, lighting) in snapshot.lighting.iter().enumerate() {
        if let Some(lighting) = lighting {
            renderer.set_mesh_lighting(index, lighting)?;
        } else {
            renderer.clear_mesh_lighting(index)?;
        }
    }
    let capture = renderer.capture_scene_frame(
        PROBE_RENDER_SIZE[0],
        PROBE_RENDER_SIZE[1],
        &snapshot.frame,
        path,
    );
    renderer.device.poll(wgpu::Maintain::Wait);
    capture
}

pub fn run(
    root: &Path,
    mission: &str,
    model_directories: &[String],
    capture: Option<&Path>,
) -> Result<(), String> {
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
    append_world_completeness_report(&mut report, root, mission, &session);
    append_navigation_report(&mut report, &session);
    // GameInit follows Open in native order; run the owned prefix only over
    // a completed load, never over a partial prefix.
    let game_init = if load.is_ok() {
        session
            .run_game_init_prefix()
            .map_err(|error| format!("{error:?}"))
    } else {
        Err("skipped after stopped load".into())
    };
    // Owned program init plus the single GameInitStart run, in Init order
    // after the scalar prefix. Runtimes are discarded before reporting so
    // terminal disposal starts clean.
    let scripts = if matches!(game_init, Ok(())) {
        run_game_init_scripts(&mut session)
    } else {
        Err("skipped after stopped GameInit prefix".into())
    };
    match &game_init {
        Ok(()) => {
            let police = session.police_init();
            let traffic = session.traffic_flags();
            writeln!(
                report,
                "GameInit prefix: police active={} speeds={}/{} throttle_lists={}/{}/{}; traffic taxi_driver={} in_taxi={} scream={}",
                police.active(),
                police.speed_limit(),
                police.speed_limit_arrest(),
                police.record_count(),
                police.road_block_car_count() + police.road_block_iris_count(),
                police.do_rici_actor_count(),
                traffic.taxi_driver_enabled(),
                traffic.player_in_taxi(),
                traffic.valid_scream()
            )
            .unwrap();
        }
        Err(detail) => {
            writeln!(report, "GameInit prefix stopped: {detail}").unwrap();
        }
    }
    match &scripts {
        Ok(script) => {
            writeln!(
                report,
                "GameInitStart: {:?} effect_calls={} beeps={} budget_remaining={}",
                script.outcome, script.effect_calls, script.beeps, script.budget_remaining
            )
            .unwrap();
        }
        Err(detail) => {
            writeln!(report, "GameInitStart stopped: {detail}").unwrap();
        }
    }
    // Independent ordered Init inspection AFTER the GameInitStart script
    // diagnostic above. It starts with the actual retained Scene, but is not
    // a continuation of that diagnostic or a successful full GameInit. In
    // particular the true Init driver stops before GlobalProgramStart while
    // this command separately exercises that script with counting effects.
    // Keep the first failure and reached owner state visible without changing
    // whether an otherwise complete source-load probe exits successfully.
    if load.is_ok() {
        let mut attempt = InitAttempt::default();
        let result = attempt.run(&mut session);
        let status = match result {
            // SessionInitError already carries the reached InitStep prefix.
            Err(InitRunError::Callback(failure)) => format!("stopped: {}", failure.error),
            Err(InitRunError::AlreadyEntered) => "unexpected repeated Init attempt".into(),
            Ok(()) => "all ordered callbacks returned (gameplay unverified)".into(),
        };
        let (passed, car_sounds, weather, buffer) = session.game_init_reset_observation();
        writeln!(
            report,
            "GameInit ordered inspection (independent, after script diagnostic): {status}; last={:?}; mission_passed={passed} car_sounds={car_sounds} weather={weather:?} weather_buffer={buffer} PCM_gain={:?}",
            attempt.last(),
            session.audio().master_gain()
        )
        .unwrap();
    } else {
        writeln!(report, "GameInit ordered inspection: skipped after stopped load").unwrap();
    }
    // A separate presentation probe over the SAME retained mission Scene and
    // its active camera. Optional GPU readback uses the generated scene frame
    // without creating a second load owner or starting gameplay.
    let capture_result = if load.is_ok() {
        let mut sequence = lhr_engine::scene_render::SceneSnapshotSequence::default();
        match session.snapshot_loaded_scene(PROBE_RENDER_SIZE, &mut sequence) {
            Ok(snapshot) => {
                writeln!(
                    report,
                    "Mission presentation snapshot: scene meshes={} source frames={} main/backdrop/near draws={}/{}/{} viewport={:?} (CPU only)",
                    snapshot.scene.meshes.len(),
                    snapshot.draw_frames.len(),
                    snapshot.frame.main.draws.len(),
                    snapshot.frame.backdrop.draws.len(),
                    snapshot.frame.near.draws.len(),
                    snapshot.frame.viewport,
                )
                .unwrap();
                if let Some(path) = capture {
                    let result = capture_retained_mission_frame(&snapshot, path);
                    match &result {
                        Ok(()) => writeln!(report, "Mission GPU readback written to {}", path.display()).unwrap(),
                        Err(error) => writeln!(report, "Mission GPU readback stopped: {error}").unwrap(),
                    }
                    result
                } else {
                    Ok(())
                }
            }
            Err(error) => {
                writeln!(report, "Mission presentation snapshot unavailable: {error}").unwrap();
                if capture.is_some() {
                    Err(format!("mission snapshot unavailable for requested GPU capture: {error}"))
                } else {
                    Ok(())
                }
            }
        }
    } else {
        writeln!(report, "Mission presentation snapshot: skipped after stopped load").unwrap();
        Ok(())
    };
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
    writeln!(report, "Gameplay acceptance: NOT EVALUATED. This probe installs the reached actor/program sources, but does not execute complete MissionOpen, later GameInit, mission scheduling, objectives or mission completion.").unwrap();
    {
        use std::io::Write as _;
        std::io::stdout()
            .lock()
            .write_all(report.as_bytes())
            .map_err(|error| format!("Could not write mission probe report: {error}"))?;
    }
    let result = match (load, game_init, scripts, discard) {
        (Ok(()), Ok(()), Ok(_), Ok(())) => Ok(()),
        (Err(_), _, _, Ok(())) => Err("Mission load probe stopped at the reported load boundary; terminal disposal completed".into()),
        (Ok(()), _, _, Err(_)) => Err("Mission prefix completed, but terminal disposal stopped at the reported ownership boundary".into()),
        (Err(_), _, _, Err(_)) => Err("Mission load and terminal disposal both stopped; their first failures are reported separately".into()),
        (Ok(()), Err(_), _, Ok(())) => Err("Mission prefix completed, but the GameInit prefix stopped at the reported boundary; terminal disposal completed".into()),
        (Ok(()), Ok(()), Err(_), Ok(())) => Err("Mission prefix and GameInit scalars completed, but GameInitStart stopped at the reported boundary; terminal disposal completed".into()),
    };
    result.and(capture_result)
}

fn append_navigation_report(report: &mut String, session: &MissionLoadSession) {
    use lhr_engine::mission_session_navigation::SessionNavigationOutcome;
    match session.navigation().outcome() {
        SessionNavigationOutcome::NotAttempted => {
            writeln!(report, "Navigation graph: not attempted").unwrap();
        }
        SessionNavigationOutcome::Absent { path } => {
            writeln!(report, "Navigation graph: absent ({path})").unwrap();
        }
        SessionNavigationOutcome::Loaded {
            path,
            source_bytes,
            nodes,
            links,
            read_calls,
            short_reads,
        } => {
            writeln!(
                report,
                "Navigation graph: {path} nodes={nodes} links={links} source_bytes={source_bytes} reads={read_calls} short_reads={short_reads}"
            )
            .unwrap();
        }
        SessionNavigationOutcome::SourceUnavailable { path, detail } => {
            writeln!(
                report,
                "Navigation graph: source unavailable ({path}): {detail}"
            )
            .unwrap();
        }
        SessionNavigationOutcome::Failed { path, detail } => {
            writeln!(report, "Navigation graph: failed ({path}): {detail}").unwrap();
        }
    }
}

fn append_world_completeness_report(
    report: &mut String,
    root: &Path,
    mission: &str,
    session: &MissionLoadSession,
) {
    let files = match mission_world_completeness::mission_world_files(mission) {
        Ok(files) => files,
        Err(error) => {
            writeln!(
                report,
                "World completeness: mission name rejected: {error:?}"
            )
            .unwrap();
            return;
        }
    };
    // Diagnostics use their own installation handle; live session owners stay
    // untouched. A resolver failure is reported, not promoted into load state.
    let installation = match lhr_assets::Installation::open(root) {
        Ok(installation) => installation,
        Err(error) => {
            writeln!(
                report,
                "World completeness: installation reopen failed: {}",
                error
            )
            .unwrap();
            return;
        }
    };
    match mission_world_completeness::read_world_file_sizes(&installation, &files) {
        Ok(states) => {
            for state in &states {
                writeln!(
                    report,
                    "World file: {} present={} bytes={}",
                    state.path, state.present, state.bytes
                )
                .unwrap();
            }
        }
        Err(error) => {
            writeln!(report, "World file intake stopped: {error:?}").unwrap();
            return;
        }
    }
    let road_path = files.road.clone();
    match installation.read_optional(&road_path) {
        Ok(Some(bytes)) => match lhr_engine::road_network::parse_road_network(&bytes) {
            Ok(network) => writeln!(
                report,
                "Road network: nodes={} links={} (session-owned nodes={} links={})",
                network.nodes.len(),
                network.links.len(),
                session.roads().node_count(),
                session.roads().link_count()
            )
            .unwrap(),
            Err(error) => writeln!(report, "Road network parse stopped: {error:?}").unwrap(),
        },
        Ok(None) => writeln!(report, "Road network: absent").unwrap(),
        Err(error) => writeln!(report, "Road network read stopped: {}", error).unwrap(),
    }
    writeln!(
        report,
        "Base cache: open={} blocks={} model_sets={} (`Open` 0x4013f0 pending)",
        session.base_cache().is_open(),
        session.base_cache().blocks().len(),
        session.base_cache().model_sets().len()
    )
    .unwrap();
    let cache_path = files.cache.clone();
    match installation.read_optional(&cache_path) {
        Ok(Some(bytes)) => match mission_world_completeness::summarize_city_cache(&bytes) {
            Ok(summary) => writeln!(
                report,
                "City cache: parts={} objects={} unknown_chunks={}",
                summary.parts, summary.objects, summary.unknown_chunks
            )
            .unwrap(),
            Err(error) => writeln!(report, "City cache parse stopped: {}", error.detail).unwrap(),
        },
        Ok(None) => writeln!(report, "City cache: absent").unwrap(),
        Err(error) => writeln!(report, "City cache read stopped: {}", error).unwrap(),
    }
    let scene2_path = format!("missions/{mission}/scene2.bin");
    match installation.read_optional(&scene2_path) {
        Ok(Some(bytes)) => match lhr_assets::parse_scene2(&bytes) {
            Ok(scene) => {
                let actors = mission_world_completeness::resolve_mission_actors(&scene);
                writeln!(
                    report,
                    "Mission actors: player_found={} taxi_found={} player_pos={:?} taxi_pos={:?}; scene objects={} actors={} scripts={} unknown_chunks={}",
                    actors.player_found,
                    actors.taxi_found,
                    actors.player_world_position,
                    actors.taxi_world_position,
                    scene.objects.len(),
                    scene.actors.len(),
                    scene.scripts.len(),
                    scene.unknown_chunks
                )
                .unwrap();
            }
            Err(error) => {
                writeln!(report, "Mission actor metadata parse stopped: {}", error).unwrap()
            }
        },
        Ok(None) => writeln!(report, "Mission actor metadata: scene2 absent").unwrap(),
        Err(error) => writeln!(report, "Mission actor metadata read stopped: {}", error).unwrap(),
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
        let world = &loader.content.actors.world;
        for actor in world.owned_actor_references() {
            if let Some(traffic) = world.inspect_traffic_generator(actor) {
                let unknown_record_bytes: usize = traffic
                    .records()
                    .into_iter()
                    .flat_map(|p| p.records())
                    .filter_map(|slot| {
                        if let lhr_engine::traffic_generator::TrafficRecordSlot::Owned(record) =
                            slot
                        {
                            Some(record.bytes().iter().filter(|byte| byte.is_none()).count())
                        } else {
                            None
                        }
                    })
                    .sum();
                let unknown_weight_bytes = traffic.weights().map_or(0, |weights| {
                    weights.bytes().iter().filter(|byte| byte.is_none()).count()
                });
                writeln!(report, "Traffic generator actor {}: phase={:?}; count={:?}; total bits={:?}; record slots={}; weight bytes={}; unknown record bytes={}; unknown weight bytes={}",
                    actor.identity(), world.phase(actor), traffic.count_bits(), traffic.total_bits(),
                    traffic.records().map_or(0, |records| records.records().len()),
                    traffic.weights().map_or(0, |weights| weights.bytes().len()),
                    unknown_record_bytes, unknown_weight_bytes).unwrap();
            }
            if let Some(car) = world.inspect_car(actor) {
                writeln!(
                    report,
                    "Car actor {}: Init temporaries={:?}; retained source allocations={}",
                    actor.identity(),
                    world.inspect_car_init_temporaries(actor),
                    car.retained_source_allocation_count()
                )
                .unwrap();
            }
        }
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
