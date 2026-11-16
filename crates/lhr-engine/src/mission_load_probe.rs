//! Headless host probe over the same owners as MissionLoadSession.
//! This command never starts a menu, actor scheduler, or mission-completion gate.

use lhr_engine::{
    actor_world::ActorWorld,
    game_camera_projection::GameCameraProjection,
    game_init_order::{InitAttempt, InitRunError, PresentedFrame},
    host_scene_adapters::InspectionScreen,
    mission_content_loading::{
        MissionProgramDiagnostic, NamedGlobalProgramOutcome, ProgramWorldLocales,
    },
    mission_load_session::{DetectorScheduleFrontier, MissionLoadCause, MissionLoadSession},
    mission_new_game_order::{NewMissionAttempt, NewMissionRequest, NewMissionRunError},
    mission_open_order::{OpenAttempt, OpenResult},
    mission_world_completeness,
    model_loading::{ModelArena, ModelError},
    native_precision::Pc24PrecisionState,
    original_program::RuntimeView,
    scene_ownership::SceneKey,
    scene_projection::SceneRenderBackend,
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

/// Headless `I3D_scene::Render` boundary for the `C_mission::Open`
/// mute+render step. Native `I3D_driver::Render` (LS3DF 0x10062030, mafia-re
/// ls3df/I3D/render/render.cpp) with no current camera bumps the scene render
/// number, clears the D3D target to the scene background and returns
/// I3DERR_GENERIC. This probe has no device, so only that camera-less return
/// is answered; the Open caller ignores it. A camera Render is refused as an
/// unavailable device instead of being reported as drawn.
#[derive(Default)]
struct ProbeOpenRender {
    calls: usize,
    /// cbLoadScene STEP `SetState(DXS_ALPHA, ALPHASTATE_BLEND)` calls.
    alpha_blend_states: usize,
    /// `GM_LoadProgress::Present` frames during Open and their last bar.
    loading_presents: usize,
    last_loading_bar: Option<f32>,
    /// Diagnostic refusal of this (0-based) loading-screen Present.
    refuse_loading_present: Option<usize>,
    /// Whether that refusal was reached.
    refused_loading_present: bool,
}

/// The headless probe's Open device takes the loading screen's device calls
/// as it takes the scene Render: counted, nothing drawn (there is no
/// `GM_Menu::Draw` owner, and the original ignores every result).
impl lhr_engine::load_scene_callback::LoadingScreenDevice for ProbeOpenRender {
    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        self.alpha_blend_states += 1;
        Ok(())
    }
    fn present_loading_screen(
        &mut self,
        frame: lhr_engine::load_scene_callback::LoadingScreenFrame<'_>,
    ) -> Result<(), String> {
        if self.refuse_loading_present == Some(self.loading_presents) {
            self.refused_loading_present = true;
            return Err(format!(
                "probe refused loading-screen present {}",
                self.loading_presents
            ));
        }
        self.loading_presents += 1;
        self.last_loading_bar = Some(frame.screen.raw_progress());
        Ok(())
    }
}

const I3DERR_GENERIC: u32 = 0x8000_0005;

impl SceneRenderBackend for ProbeOpenRender {
    fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
        self.calls += 1;
        match arena.scene_active_camera(scene)? {
            None => Ok(I3DERR_GENERIC),
            Some(_) => Err(ModelError::Asset(
                "headless probe has no I3D device for a camera Render".into(),
            )),
        }
    }
}

const PROBE_PROJECTION: GameCameraProjection = GameCameraProjection {
    global_mode: 0,
    screen_flag_46f4: false,
};

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

struct ScriptRunReport {
    outcome: NamedGlobalProgramOutcome,
    beeps: usize,
    budget_remaining: usize,
    light_cache_lights: usize,
    schedule: ScheduleReport,
    frontier: Vec<DetectorScheduleFrontier>,
}

/// Bounded GameInit scheduling diagnostic: entity/detector program init
/// plus five 16 ms scheduler ticks over the live session, with the
/// session-owned light-cache tick advanced once per tick. Counts stepped
/// programs and light evaluations; the first unavailable owner stops the
/// schedule and is reported, never skipped.
#[derive(Default)]
struct ScheduleReport {
    entity_init: usize,
    detector_init: usize,
    entity_ticks: usize,
    detector_ticks: usize,
    ticks_run: usize,
    light_tick_evals: usize,
    lights_on: u32,
    lights_off: u32,
    error: Option<String>,
}

fn run_schedule_ticks(
    session: &mut MissionLoadSession,
    budget: &mut DispatchBudget,
    diagnostic: &mut ProbeDiagnostic,
    locales: ProgramWorldLocales,
) -> ScheduleReport {
    let mut report = ScheduleReport::default();
    match session.initialize_owned_entity_programs(diagnostic, locales) {
        Ok(stepped) => report.entity_init = stepped,
        Err(error) => {
            report.error = Some(format!("entity init: {error:?}"));
            return report;
        }
    }
    match session.initialize_owned_detector_programs(diagnostic, locales) {
        Ok(stepped) => report.detector_init = stepped,
        Err(error) => {
            report.error = Some(format!("detector init: {error:?}"));
            return report;
        }
    }
    for _ in 0..5 {
        match session.tick_owned_entity_programs(16, budget, diagnostic, locales) {
            Ok(stepped) => report.entity_ticks += stepped,
            Err(error) => {
                report.error = Some(format!("entity tick: {error:?}"));
                break;
            }
        }
        match session.tick_owned_detector_programs(16, budget, diagnostic, locales) {
            Ok(stepped) => report.detector_ticks += stepped,
            Err(error) => {
                report.error = Some(format!("detector tick: {error:?}"));
                break;
            }
        }
        match session.tick_owned_light_cache(16) {
            Ok(outcome) => {
                if outcome.evaluated {
                    report.light_tick_evals += 1;
                    report.lights_on += outcome.turned_on;
                    report.lights_off += outcome.turned_off;
                }
            }
            Err(error) => {
                report.error = Some(format!("light tick: {error:?}"));
                break;
            }
        }
        report.ticks_run += 1;
    }
    report
}

/// Owned program init plus the single `GameInitStart` run over the live
/// session owners as a separate dependency diagnostic. When the ordered
/// `Init` already reached `GlobalProgramStart`, that step is the single
/// owner of both: its recorded outcome and remaining dispatch guard are
/// reused and nothing is initialized or run again. This does not bypass
/// earlier missing Init owners in a playable startup path. Runtimes
/// are discarded host-side afterwards so terminal disposal sees no retained
/// program state; this is Init, not Done.
fn run_game_init_scripts(session: &mut MissionLoadSession) -> Result<ScriptRunReport, String> {
    let mut diagnostic = ProbeDiagnostic::default();
    let record = session.game_init_programs().clone();
    let budget = DispatchBudget::new(record.budget_remaining.unwrap_or(PROBE_DISPATCH_BUDGET))
        .map_err(|error| format!("dispatch guard after Init's GameInitStart: {error:?}"));
    let mut budget = match budget {
        Ok(budget) => budget,
        Err(error) => {
            let discard = session
                .discard_global_program_runtimes_host()
                .map_err(|discard| format!("{error}; runtimes discard: {discard:?}"));
            return Err(discard.err().unwrap_or(error));
        }
    };
    // Program runtimes must be discarded before reporting no matter how the
    // run below ends; otherwise terminal disposal refuses the retained state
    // and masks the real outcome.
    let (init, outcome) = if record.initialized {
        let outcome = record.game_init_start.clone().unwrap_or_else(|| {
            Err("the global programs were initialized but GameInitStart was not reached".into())
        });
        (Ok(()), Some(outcome))
    } else {
        // Recorded on the session: a later ordered Init refuses to
        // initialize the same programs again.
        (
            Ok(()),
            Some(session.run_global_program_start(&mut budget, &mut diagnostic)),
        )
    };
    let discard = session
        .discard_global_program_runtimes_host()
        .map_err(|error| format!("{error:?}"));
    let outcome = match (init, outcome, discard) {
        (Ok(()), Some(Ok(outcome)), Ok(_)) => Ok(outcome),
        (Err(error), _, Ok(_)) => Err(error),
        (Ok(()), Some(Err(error)), Ok(_)) => Err(error),
        (Ok(()), Some(Err(run)), Err(discard)) => {
            Err(format!("{run}; runtimes discard: {discard}"))
        }
        (Ok(()), Some(Ok(_)), Err(error)) => Err(format!("runtimes discard: {error}")),
        (Err(init), _, Err(discard)) => Err(format!("{init}; runtimes discard: {discard}")),
        (Ok(()), None, _) => Err("unreachable GameInitStart admission".into()),
    }?;
    // Scheduler ticks run only after a successful GameInitStart, against
    // the same live owners and the shared never-replenished budget.
    let locales = ProgramWorldLocales {
        game_default_c_locale: true,
        ls3df_default_c_locale: true,
    };
    let schedule = run_schedule_ticks(session, &mut budget, &mut diagnostic, locales);
    // Capture the refusing arms before the tick runtimes retire below:
    // after discard every program reads Ready again.
    let frontier = session.detector_schedule_frontier();
    // Tick runtimes back out the same way: entity/detector runtimes are
    // retired explicitly so terminal disposal sees no retained VM state.
    session
        .discard_actor_program_runtimes_host()
        .map_err(|error| format!("schedule runtimes discard: {error}"))?;
    session
        .discard_global_program_runtimes_host()
        .map_err(|error| format!("schedule runtimes discard: {error:?}"))?;
    Ok(ScriptRunReport {
        outcome,
        beeps: diagnostic.beeps,
        budget_remaining: budget.remaining(),
        light_cache_lights: session
            .game_state()
            .light_cache()
            .map_or(0, |cache| cache.lights.len()),
        schedule,
        frontier,
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

/// Single-line probe rendering of mission text: newlines become spaces and
/// long entries truncate with a marker. Presentation-only; the stored text
/// and the staged ids never mutate.
fn flat_probe_text(text: &str) -> String {
    const LIMIT: usize = 240;
    let flat: String = text
        .chars()
        .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
        .collect();
    if flat.chars().count() <= LIMIT {
        flat
    } else {
        let kept: String = flat.chars().take(LIMIT).collect();
        format!("{kept}… [truncated]")
    }
}

/// Resolve staged mission-script IDs to real mission text for the report.
/// Reads the install language routing (version.txt -> textdb_<lang>.def,
/// English fallback) and the evidenced GetTextOrNULL owner (GAME 0x60fba0).
/// Objectives and Simple subtitles resolve by id; Intro pairs report raw
/// because their parameter meaning is unrecovered (STUB GAME 0x5f9480).
/// Absent files, rejected images and unknown IDs report honestly, never as
/// blank text. Read-only: no session, textdb or game state mutates.
pub fn mission_text_lines(
    root: &Path,
    objectives: &[u32],
    subtitles: &[lhr_engine::game_housekeeping::StagedSubtitle],
) -> Vec<String> {
    use lhr_engine::game_housekeeping::StagedSubtitle;
    let installation = match lhr_assets::Installation::open(root) {
        Ok(installation) => installation,
        Err(error) => return vec![format!("Mission text: installation unavailable ({error})")],
    };
    let code = match installation.read_optional("tables/version.txt") {
        Ok(Some(bytes)) => lhr_engine::original_language::language_code_from_version(&bytes),
        _ => 0x656e,
    };
    let path = lhr_engine::original_language::language_resource_paths(code)
        .map(|paths| paths.text_database)
        .unwrap_or_else(|_| "tables/textdb_en.def".into());
    let bytes = match installation.read_optional(&path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return vec![format!("Mission text: {path} absent")],
        Err(error) => return vec![format!("Mission text: {path} unreadable ({error})")],
    };
    let mut database = lhr_engine::text_database::TextDatabase::new();
    if !database.load_from_bytes(&bytes) {
        return vec![format!("Mission text: {path} rejected")];
    }
    let mut lines = Vec::new();
    for id in objectives {
        match database.get_text_or_null(*id) {
            Some(text) => lines.push(format!("Mission objective {id}: {}", flat_probe_text(text))),
            None => lines.push(format!("Mission objective {id}: <unresolved in {path}>")),
        }
    }
    for subtitle in subtitles {
        match subtitle {
            StagedSubtitle::Simple(id) => match database.get_text_or_null(*id) {
                Some(text) => {
                    lines.push(format!("Mission subtitle {id}: {}", flat_probe_text(text)))
                }
                None => lines.push(format!("Mission subtitle {id}: <unresolved in {path}>")),
            },
            StagedSubtitle::Intro(first, second) => lines.push(format!(
                "Mission intro subtitle pair: ({first}, {second}) (parameter meaning unrecovered, no text lookup)"
            )),
        }
    }
    if lines.is_empty() {
        lines.push("Mission text: no staged objectives or subtitles".into());
    }
    lines
}

/// Run the real mission script preamble (load, GameInit prefix, GameInitStart
/// plus bounded scheduling) and return the staged mission objectives and
/// subtitles the scripts actually produced, with a one-line scheduling note.
/// Scheduling may stop at the known STUB frontiers (recorded in the note);
/// that never masks the staged content, which is read regardless. The
/// session is terminally disposed before returning, so no owners leak to
/// the caller. Used to gate playable-slice completion on actual script
/// output instead of geometry alone.
pub fn staged_script_content(
    root: &Path,
    mission: &str,
) -> Result<
    (
        Vec<u32>,
        Vec<lhr_engine::game_housekeeping::StagedSubtitle>,
        String,
    ),
    String,
> {
    validate_inputs(mission, &[])?;
    let installation = lhr_assets::Installation::open(root).map_err(|error| error.to_string())?;
    let mut session = MissionLoadSession::new(installation, Pc24PrecisionState::new(false))
        .map_err(|error| format!("Mission probe initialization failed: {error:?}"))?;
    // Open's loading screen reads mafiaIndicators.GetLanguage (GM_Menu::
    // CreateCompsByDef), so the global exists before Open as natively.
    let clock = crate::host_local_time::local_timer_clock()
        .map_err(|error| format!("Mission probe HUD clock: {error}"))?;
    session.install_indicators(lhr_engine::indicators::IndicatorsState::constructed(clock))?;
    let mut screen = InspectionScreen {
        size: PROBE_SIZE,
        viewport: [0, 0, PROBE_SIZE[0], PROBE_SIZE[1]],
    };
    let opened = session
        .open_source_in_order(
            &mut OpenAttempt::default(),
            mission,
            // Script staging never runs the ordered Init driver (its
            // `_show_progress_`), so no loading screen; Main.cpp:387 passes
            // true once that driver is wired here.
            false,
            &[],
            PROBE_PROJECTION,
            &mut screen,
            &mut ProbeOpenRender::default(),
        )
        .map_err(|error| format!("{error:?}"))?;
    if opened != OpenResult::Opened {
        return Err(format!("C_mission::Open returned {opened:?}"));
    }
    session
        .run_game_init_prefix()
        .map_err(|error| format!("{error:?}"))?;
    let scripts = run_game_init_scripts(&mut session)?;
    let schedule = &scripts.schedule;
    let note = match &schedule.error {
        None => format!(
            "ticks={} entity_ticks={} detector_ticks={}",
            schedule.ticks_run, schedule.entity_ticks, schedule.detector_ticks
        ),
        Some(detail) => format!("stopped: {detail} after ticks={}", schedule.ticks_run),
    };
    let objectives = session.game_state().objectives().to_vec();
    let subtitles = session.game_state().subtitles().to_vec();
    session
        .discard_terminal()
        .map_err(|error| format!("terminal disposal: {error:?}"))?;
    if let Some(failure) = session.discard_failure() {
        return Err(format!("terminal disposal: {failure:?}"));
    }
    Ok((objectives, subtitles, note))
}

/// What `ProfileLoad` (GAME 0x6057f0) would find: it opens the relative
/// `savegame\mafiaNNN.sav` (its `sprintf` in g_loadsave.cpp `ProfileLoad`) from
/// the game's working directory, the installation root. Any such file
/// means a profile whose G_Stream bytes have no owner here.
fn profile_file_intake(
    root: &Path,
) -> Result<lhr_engine::game_loadsave_lifecycle::ProfileFileIntake, String> {
    use lhr_engine::game_loadsave_lifecycle::ProfileFileIntake;
    let entries =
        std::fs::read_dir(root).map_err(|error| format!("{}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", root.display()))?;
        if !entry.file_name().eq_ignore_ascii_case("savegame") || !entry.path().is_dir() {
            continue;
        }
        let directory = entry.path();
        let files = std::fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        for file in files {
            let file = file.map_err(|error| format!("{}: {error}", directory.display()))?;
            let name = file.file_name().to_string_lossy().to_ascii_lowercase();
            let number = name
                .strip_prefix("mafia")
                .and_then(|rest| rest.strip_suffix(".sav"));
            // `ProfileEnumFiles` enumerates slots 000-031 only
            // (`cmp esi, 0x20`, GAME 0x6054f2) and opens files.
            if number.is_some_and(|digits| {
                digits.len() == 3
                    && digits.bytes().all(|b| b.is_ascii_digit())
                    && digits.parse::<u32>().is_ok_and(|slot| slot < 0x20)
            }) && file.path().is_file()
            {
                return Ok(ProfileFileIntake::ProfileFilePresent);
            }
        }
    }
    Ok(ProfileFileIntake::NoProfileFile)
}

/// Reached `ResetMissionState` stores after the render block, read live.
fn reset_flags_observation(session: &MissionLoadSession) -> String {
    let game = session.game_state();
    let lists = session.game_lists();
    let ai = session.ai_manager();
    let effects = session.effects();
    let flags = format!(
        "score={:?} HUD bonus={:?} camera_rot_repair={} heal_cheat={} stacks_clear={} death_fade_time={} actor_lightness={} lists(stop_sounds, paused_i_shows, free_shadow_names, unknown_140)=({}, {}, {}, {}) AIManager(police unknown_24={:#x} speed_factor={} groups unlock_time={:#x} follow check_time={} records police/groups/follow/fight={}/{}/{}/{}) C_Vehicle sectors light/sound={}/{} sound_play_timer period={} sound_arena cleared={} lightness/invisible_actors={}/{} boxes/box_dests/box_placement_temp={}/{}/{} C_effects(table={} static_particles={:?} started={} last_guid={} static_emitters={})",
        game.score(),
        session.indicators().map(|hud| hud.bonus_count()),
        game.camera_rot_repair(),
        game.heal_cheat(),
        game.stacks_clear(),
        game.death_fade_time(),
        game.actor_lightness(),
        lists.stop_sounds.len(),
        lists.paused_i_shows.len(),
        lists.free_shadow_names.len(),
        lists.unknown_140.len(),
        ai.police.unknown_24(),
        ai.police.speed_factor(),
        ai.groups.unlock_time(),
        ai.follow.check_time(),
        ai.police.actions().len(),
        ai.groups.groups().len(),
        ai.follow.groups().len(),
        ai.fight.parties().len(),
        session.vehicle_sectors().light_sectors().len(),
        session.vehicle_sectors().sound_sectors().len(),
        game.sound_counters().3.period_ms(),
        session.sound_arena().is_cleared(),
        lists.lightness.len(),
        lists.invisible_actors.len(),
        lists.boxes.len(),
        lists.box_dests.len(),
        lists.box_placement_temp.len(),
        effects.effects().len(),
        effects.static_particles(),
        effects.started(),
        effects.last_guid(),
        effects.static_emitter_count()
    );
    format!(
        "{flags} diff_frames/actors={}/{} clear_diff_data={} crash_objects={}",
        lists.diff_frames.len(),
        lists.diff_actors.len(),
        game.clear_diff_data(),
        session.crash_object_count()
    )
}

pub fn run_freeride_start(root: &Path, model_directories: &[String]) -> Result<(), String> {
    use lhr_engine::mission_load_session::{FreerideAttempt, FreerideRequest, FREERIDE_MISSION};
    validate_inputs(FREERIDE_MISSION, model_directories)?;
    let installation = lhr_assets::Installation::open(root).map_err(|error| error.to_string())?;
    let mut session = MissionLoadSession::new(installation, Pc24PrecisionState::new(false))
        .map_err(|error| format!("Freeride probe initialization: {error:?}"))?;
    let clock = crate::host_local_time::local_timer_clock()?;
    session.install_indicators(lhr_engine::indicators::IndicatorsState::constructed(clock))?;
    let profile = profile_file_intake(root);
    let settings = profile.clone().and_then(|intake| {
        lhr_engine::game_loadsave_lifecycle::profile_settings_after_load_save_init(intake)
            .map_err(str::to_owned)
    });
    let settings_status = match settings {
        Ok(settings) => {
            session.adopt_profile_settings(settings)?;
            "adopted".to_owned()
        }
        Err(error) => format!("unavailable: {error}"),
    };
    let input = profile.and_then(|intake| {
        lhr_engine::game_loadsave_controls::profile_bindings_after_load_save_init(intake)
            .map_err(str::to_owned)
            .and_then(lhr_engine::session_input::KeyboardMouseInput::from_profile)
    });
    let input_status = match input {
        Ok(input) => {
            session.adopt_keyboard_mouse_input(input)?;
            "keyboard/mouse-only host startup".to_owned()
        }
        Err(error) => format!("unavailable: {error}"),
    };
    let directories: Vec<&[u8]> = model_directories
        .iter()
        .map(|path| path.as_bytes())
        .collect();
    let mut attempt = FreerideAttempt::default();
    let mut screen = InspectionScreen {
        size: PROBE_SIZE,
        viewport: [0, 0, PROBE_SIZE[0], PROBE_SIZE[1]],
    };
    let result = session.start_freeride(
        &mut attempt,
        FreerideRequest {
            model_directories: &directories,
            projection: PROBE_PROJECTION,
        },
        &mut screen,
        &mut ProbeOpenRender::default(),
    );
    let mut report = String::new();
    writeln!(
        report,
        "Original Freeride startup diagnostic; fixed mission: {FREERIDE_MISSION}"
    )
    .unwrap();
    writeln!(report, "Scope: fresh session; original active-mission Close and native repeatable lifecycle are not composed").unwrap();
    writeln!(report, "Host inputs: 800x600, full viewport, initial PC24 sticky precision flag=false; model directories={model_directories:?}; profile settings={settings_status}; input={input_status}").unwrap();
    writeln!(
        report,
        "No implicit model directory, substitute map, car selection or filename fallback"
    )
    .unwrap();
    writeln!(
        report,
        "Startup reached: {:?}; result: {result:?}",
        attempt.reached()
    )
    .unwrap();
    writeln!(
        report,
        "Open reached: {:?}; result: {:?}; failure: {:?}",
        attempt.open().reached(),
        attempt.open_result(),
        session.failure()
    )
    .unwrap();
    writeln!(
        report,
        "Freeride active: {}; setup: {:?}",
        session.freeride().active,
        session.freeride_setup_progress()
    )
    .unwrap();
    writeln!(
        report,
        "GameInit entered: {}; last: {:?}",
        attempt.init().entered(),
        attempt.init().last()
    )
    .unwrap();
    writeln!(
        report,
        "Owners before terminal disposal: {:?}",
        session.owned_counts()
    )
    .unwrap();
    let disposal = session.discard_terminal();
    writeln!(
        report,
        "Terminal disposal: {disposal:?}; phase: {:?}; failure: {:?}",
        session.discard_progress(),
        session.discard_failure()
    )
    .unwrap();
    writeln!(
        report,
        "Owners after terminal disposal: {:?}; held setup model creators: {}",
        session.owned_counts(),
        session.freeride_setup_progress().held_model_creators()
    )
    .unwrap();
    writeln!(report, "Freeride gameplay: NOT PLAYABLE; no player/car coupling, successful game ticks, traffic or repeatable Close is claimed. Terminal disposal is host cleanup, not native Close.").unwrap();
    {
        use std::io::Write as _;
        std::io::stdout()
            .lock()
            .write_all(report.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    Err(if disposal.is_ok() {
        "Freeride startup stopped at the reported native dependency; terminal disposal completed"
            .into()
    } else {
        "Freeride startup and terminal disposal stopped; both failures are reported".into()
    })
}

pub fn run(
    root: &Path,
    mission: &str,
    model_directories: &[String],
    capture: Option<&Path>,
    refuse_loading_present: Option<usize>,
) -> Result<(), String> {
    // Keep this validation at the command boundary even if a future caller
    // bypasses the main option parser. Mission paths cannot escape their root.
    validate_inputs(mission, model_directories)?;
    let installation = lhr_assets::Installation::open(root).map_err(|error| error.to_string())?;
    let mut session = MissionLoadSession::new(installation, Pc24PrecisionState::new(false))
        .map_err(|error| format!("Mission probe initialization failed: {error:?}"))?;
    // The `mafiaIndicators` global exists before any mission natively; the
    // probe constructs it with the host's local wall clock (GetLocalTime's
    // stand-in, daylight saving included) before the first Open.
    let clock = crate::host_local_time::local_timer_clock()
        .map_err(|error| format!("Mission probe HUD clock: {error}"))?;
    session.install_indicators(lhr_engine::indicators::IndicatorsState::constructed(clock))?;
    // G_LoadSaveClass::Init's profile intake runs before the main menu
    // natively; the probe takes its options from the installation's
    // savegame directory. A refused intake leaves them unset, and Init's
    // HUD GameInit reports it.
    let file_intake = profile_file_intake(root);
    let profile_intake = file_intake.clone().and_then(|intake| {
        lhr_engine::game_loadsave_lifecycle::profile_settings_after_load_save_init(intake)
            .map_err(str::to_owned)
    });
    if let Ok(settings) = &profile_intake {
        session.adopt_profile_settings(*settings)?;
    }
    let input_intake = file_intake.and_then(|intake| {
        lhr_engine::game_loadsave_controls::profile_bindings_after_load_save_init(intake)
            .map_err(str::to_owned)
            .and_then(lhr_engine::session_input::KeyboardMouseInput::from_profile)
    });
    match input_intake {
        Ok(input) => {
            session.adopt_keyboard_mouse_input(input)?;
            println!("Mission probe input: explicit keyboard/mouse-only startup; joystick discovery and event polling are not composed");
        }
        Err(error) => println!("Mission probe input unavailable: {error}"),
    }
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
    // Mission start on a fresh session (Main.cpp:385-396; the intro leg
    // Main.cpp:373-384 is not composed): outer Close, the single ordered
    // C_mission::Open, then LoadCollision, ResetMissionNumbers,
    // SaveGameNewGame and game->Init, stopping at the first unavailable owner.
    // A requested GPU capture runs at the composed Init's reached render
    // boundary, after the live audio mute; it is never retried elsewhere.
    let mut render = ProbeOpenRender {
        refuse_loading_present,
        ..ProbeOpenRender::default()
    };
    let mut new_mission = NewMissionAttempt::default();
    let mut init_capture = None;
    let mut init_loading_presents: Vec<f32> = Vec::new();
    let mut sequence = lhr_engine::scene_render::SceneSnapshotSequence::default();
    let request = NewMissionRequest {
        mission,
        model_directories: &directories,
        projection: PROBE_PROJECTION,
    };
    let new_mission_result = if let Some(path) = capture {
        session.start_new_game_mission_presented(
            &mut new_mission,
            request,
            &mut screen,
            &mut render,
            PROBE_RENDER_SIZE,
            &mut sequence,
            |frame| match frame {
                PresentedFrame::Scene(snapshot) => {
                    let result = capture_retained_mission_frame(snapshot, path);
                    init_capture = Some(result.clone());
                    result
                }
                // The loading screen has no GM_Menu::Draw owner: counted,
                // never written over the scene capture.
                PresentedFrame::LoadingScreen(frame) => {
                    init_loading_presents.push(frame.screen.raw_progress());
                    Ok(())
                }
            },
        )
    } else {
        session.start_new_game_mission(&mut new_mission, request, &mut screen, &mut render)
    };
    // The source-loading gate for every later diagnostic: Open returned
    // Opened, independent of where the post-Open composition stopped.
    let load: Result<(), ()> = match new_mission.open_result() {
        Some(OpenResult::Opened) => Ok(()),
        _ => Err(()),
    };

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
    writeln!(
        report,
        "Mission Open (C_mission::Open order): result={:?}; last={:?}; steps={}; Render calls={}; cache progress={}; restore_volume={}; opened={}",
        new_mission.open_result(),
        new_mission.open().last(),
        new_mission.open().reached().len(),
        render.calls,
        session.open_cache_progress(),
        session.open_restore_volume(),
        session.open_mission_opened()
    )
    .unwrap();
    let loading_screen = session.load_save_progress().progress.as_ref();
    writeln!(
        report,
        "Loading screen (cbLoadScene 0x5bcc10 over scene->Open): Open presents={} STEP SetState calls={} last Open bar={:?}; callback last_bit={} own screen={}; mafiaLoadSave.progress components={} bar={:?} mission picture={:?}; Init presents bars={:?} (headless: counted, not drawn)",
        render.loading_presents,
        render.alpha_blend_states,
        render.last_loading_bar,
        session.load_scene_callback().last_bit(),
        session.load_scene_callback().menu_progress().is_some(),
        loading_screen.map_or(0, |screen| screen.menu.len()),
        loading_screen.map(|screen| screen.raw_progress()),
        loading_screen
            .and_then(|screen| screen
                .menu
                .staged_string(lhr_engine::game_loadsave_lifecycle::LOADPROGRESS_MISSION_PICTURE))
            .map(|picture| picture.map(String::from_utf8_lossy)),
        init_loading_presents
    )
    .unwrap();
    if let Some(index) = render.refuse_loading_present {
        writeln!(
            report,
            "Diagnostic loading-present refusal: requested index {index}, {}",
            if render.refused_loading_present {
                "reached"
            } else {
                "not reached (normal load)"
            }
        )
        .unwrap();
    }
    let new_mission_status = match &new_mission_result {
        Ok(result) => format!("{result:?}"),
        Err(NewMissionRunError::Callback(failure)) => {
            format!("stopped at {:?}: {:?}", failure.step, failure.error)
        }
        Err(NewMissionRunError::AlreadyEntered) => "unexpected repeated attempt".into(),
    };
    writeln!(
        report,
        "Mission start on a fresh session (Main.cpp:385-396 order; intro leg Main.cpp:373-384 not composed, scope={:?}): {new_mission_status}; reached={:?}; mission numbers (next, current)={:?}; composed GameInit entered={}",
        new_mission.scope(),
        new_mission.reached(),
        session.mission_numbers(),
        new_mission.init().entered()
    )
    .unwrap();
    writeln!(
        report,
        "mafiaLoadSave.settings (G_LoadSaveClass::Init profile intake): {:?}; HUD GameInit dial/icons={:?}/{:?}",
        profile_intake,
        session.indicators().and_then(|hud| hud.dial_asset()),
        session.indicators().and_then(|hud| hud.icons_asset())
    )
    .unwrap();
    writeln!(
        report,
        "Process input adopted={}; ff_manager.GameInit complete={}; human/car flag bytes={:?}; force-feedback reset value={}",
        session.input().is_some(),
        session.force_feedback().game_init_complete(),
        session.force_feedback().effect_flag_bytes(),
        session.force_feedback().reset_value()
    )
    .unwrap();
    let progress = session.load_save_progress();
    writeln!(
        report,
        "G_LoadSaveClass new-game progress (SaveGameNewGame 0x605ff0): steal_cars={:#x} last_stolen={:?} occupied garage slots={} shown cars={} freeride.active={}",
        progress.steal_cars,
        progress.last_stolen,
        progress.garage.slots.iter().filter(|slot| slot.car != 0 || slot.actor.is_some()).count(),
        progress.garage.shown_cars.iter().filter(|&&car| car != 0).count(),
        session.freeride().active
    )
    .unwrap();
    match (&init_capture, capture) {
        (Some(Ok(())), Some(path)) => writeln!(
            report,
            "Mission GPU readback at composed Init render boundary written to {}",
            path.display()
        )
        .unwrap(),
        (Some(Err(error)), _) => writeln!(
            report,
            "Mission GPU readback at composed Init render boundary stopped: {error}"
        )
        .unwrap(),
        _ => {}
    }
    let collision = session.collision();
    writeln!(
        report,
        "Collision owner (gifCollision): progress={:?}; level installed={}; level_size={}; dynamic grid={}; LoadedLevel={}",
        collision.load_progress(),
        collision.level().is_some(),
        collision.level_size(),
        collision.dynamic_grid().is_some(),
        collision.loaded_level()
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
    // Explicit dependency diagnostic, separate from the ordered Init attempt.
    // Game effects use represented live owners and stop on unavailable ones.
    // Host runtime discard is not original GameDone or mission completion.
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
                "GameInitStart: {:?} beeps={} budget_remaining={} light_cache_lights={}",
                script.outcome, script.beeps, script.budget_remaining, script.light_cache_lights
            )
            .unwrap();
            let schedule = &script.schedule;
            match &schedule.error {
                None => writeln!(
                    report,
                    "GameInit scheduling: entity_init={} detector_init={} ticks={} entity_ticks={} detector_ticks={} light_tick_evals={} lights_on={} lights_off={}",
                    schedule.entity_init,
                    schedule.detector_init,
                    schedule.ticks_run,
                    schedule.entity_ticks,
                    schedule.detector_ticks,
                    schedule.light_tick_evals,
                    schedule.lights_on,
                    schedule.lights_off
                ),
                Some(detail) => writeln!(
                    report,
                    "GameInit scheduling stopped: {detail} after entity_init={} detector_init={} ticks={} entity_ticks={} detector_ticks={} light_tick_evals={} lights_on={} lights_off={}",
                    schedule.entity_init,
                    schedule.detector_init,
                    schedule.ticks_run,
                    schedule.entity_ticks,
                    schedule.detector_ticks,
                    schedule.light_tick_evals,
                    schedule.lights_on,
                    schedule.lights_off
                ),
            }
            .unwrap();
            let frontier = &script.frontier;
            writeln!(
                report,
                "Schedule frontier: {} interrupted detector programs",
                frontier.len()
            )
            .unwrap();
            for entry in frontier.iter().take(12) {
                writeln!(
                    report,
                    "  detector identity={} stage={:?} pc={} opcode={:?}",
                    entry.identity, entry.stage, entry.program_counter, entry.opcode
                )
                .unwrap();
            }
            if frontier.len() > 12 {
                writeln!(report, "  ... ({} more)", frontier.len() - 12).unwrap();
            }
            writeln!(
                report,
                "Mission objectives staged: {:?}; subtitles staged: {:?}",
                session.game_state().objectives(),
                session.game_state().subtitles()
            )
            .unwrap();
            for line in mission_text_lines(
                root,
                session.game_state().objectives(),
                session.game_state().subtitles(),
            ) {
                writeln!(report, "{line}").unwrap();
            }
        }
        Err(detail) => {
            writeln!(report, "GameInitStart stopped: {detail}").unwrap();
        }
    }
    writeln!(
        report,
        "Session script state: night_mission={} anim_preload={}",
        session.game_state().night_mission(),
        session.game_state().anim_preload()
    )
    .unwrap();
    // Independent ordered Init inspection AFTER the GameInitStart script
    // diagnostic above. It starts with the actual retained Scene, but is not
    // a continuation of that diagnostic or a successful full GameInit. In
    // particular, when the diagnostic initialized the global programs first,
    // this Init stops at GlobalProgramStart instead of initializing them again.
    // Keep the first failure and reached owner state visible without changing
    // whether an otherwise complete source-load probe exits successfully.
    if load.is_ok() && !new_mission.init().entered() {
        let mut attempt = InitAttempt::default();
        let result = if let Some(path) = capture {
            attempt.run_with_presentation(&mut session, PROBE_RENDER_SIZE, &mut sequence, |frame| {
                // A loading-screen Present is counted by nothing here:
                // headless, nothing drawn, the original ignores it.
                let PresentedFrame::Scene(snapshot) = frame else {
                    return Ok(());
                };
                let result = capture_retained_mission_frame(snapshot, path);
                match &result {
                    Ok(()) => writeln!(
                        report,
                        "Mission GPU readback at Init render boundary written to {}",
                        path.display()
                    )
                    .unwrap(),
                    Err(error) => writeln!(
                        report,
                        "Mission GPU readback at Init render boundary stopped: {error}"
                    )
                    .unwrap(),
                }
                init_capture = Some(result.clone());
                result
            })
        } else {
            attempt.run(&mut session)
        };
        let status = match result {
            // SessionInitError already carries the reached InitStep prefix.
            Err(InitRunError::Callback(failure)) => format!("stopped: {}", failure.error),
            Err(InitRunError::AlreadyEntered) => "unexpected repeated Init attempt".into(),
            Ok(()) => "all ordered callbacks returned (gameplay unverified)".into(),
        };
        let (passed, car_sounds, weather, buffer) = session.game_init_reset_observation();
        writeln!(
            report,
            "GameInit ordered inspection (independent, after script diagnostic): {status}; last={:?}; mission_passed={passed} car_sounds={car_sounds} weather={weather:?} weather_buffer={buffer} PCM_gain={:?}; {}",
            attempt.last(),
            session.audio().master_gain(),
            reset_flags_observation(&session)
        )
        .unwrap();
    } else if new_mission.init().entered() {
        // The composed start entered Init after SaveGameNewGame; its stop is
        // reported above. Init is one-shot, so it is not run again here.
        let (passed, car_sounds, weather, buffer) = session.game_init_reset_observation();
        writeln!(
            report,
            "GameInit ordered inspection: composed new-game start already entered Init; last={:?}; mission_passed={passed} car_sounds={car_sounds} weather={weather:?} weather_buffer={buffer} PCM_gain={:?}; {}",
            new_mission.init().last(),
            session.audio().master_gain(),
            reset_flags_observation(&session)
        )
        .unwrap();
    } else {
        writeln!(
            report,
            "GameInit ordered inspection: skipped after stopped load"
        )
        .unwrap();
    }
    // A separate CPU presentation observation over the SAME retained Scene.
    // Requested GPU work already ran at the reached Init render boundary; do
    // not retry a failed presentation or bypass an earlier Init failure here.
    if load.is_ok() {
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
            }
            Err(error) => {
                writeln!(report, "Mission presentation snapshot unavailable: {error}").unwrap();
            }
        }
    } else {
        writeln!(
            report,
            "Mission presentation snapshot: skipped after stopped load"
        )
        .unwrap();
    }
    let capture_result = if capture.is_some() {
        init_capture.unwrap_or_else(|| {
            Err(
                "requested GPU capture was not reached by ordered GameInit; see its first failure"
                    .into(),
            )
        })
    } else {
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
    writeln!(report, "Gameplay acceptance: NOT EVALUATED. This probe runs the ordered C_mission::Open source intake and reports where the new-game start stops, but does not execute later GameInit, mission scheduling, objectives or mission completion.").unwrap();
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
                let detectors = mission_world_completeness::mission_detector_standings(&scene);
                writeln!(report, "Mission detectors (kind 5): {}", detectors.len()).unwrap();
                for (name, position) in detectors.iter().take(40) {
                    writeln!(report, "  detector {name}: pos={position:?}").unwrap();
                }
                if detectors.len() > 40 {
                    writeln!(report, "  ... ({} more)", detectors.len() - 40).unwrap();
                }
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

#[cfg(test)]
mod probe_text_tests {
    use super::*;
    #[test]
    fn missing_installation_reports_unavailable_text() {
        let game = lhr_engine::game_housekeeping::GameInlineState::default();
        let lines = mission_text_lines(
            Path::new("nonexistent-probe-root"),
            game.objectives(),
            game.subtitles(),
        );
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("unavailable"));
    }
    #[test]
    fn flat_text_single_lines_and_truncates() {
        assert_eq!(flat_probe_text("a\nb\rc"), "a b c");
        let long = "x".repeat(300);
        let flat = flat_probe_text(&long);
        assert!(flat.len() < long.len());
        assert!(flat.ends_with("[truncated]"));
    }
    #[test]
    fn staged_script_content_rejects_bad_mission_without_touching_data() {
        assert!(staged_script_content(Path::new("nonexistent-probe-root"), "../bad").is_err());
    }
    #[test]
    fn staged_script_content_reports_missing_root() {
        assert!(staged_script_content(Path::new("nonexistent-probe-root"), "mise01").is_err());
    }
    #[test]
    fn profile_intake_finds_only_numbered_profile_files() {
        use lhr_engine::game_loadsave_lifecycle::ProfileFileIntake;
        let root = std::env::temp_dir().join(format!("lhr-profile-intake-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(
            profile_file_intake(&root),
            Ok(ProfileFileIntake::NoProfileFile)
        );
        let saves = root.join("SaveGame");
        std::fs::create_dir_all(&saves).unwrap();
        // A saved game and a foreign name are not profiles.
        std::fs::write(saves.join("mafia000.000"), b"").unwrap();
        std::fs::write(saves.join("mafia12.sav"), b"").unwrap();
        // Slot 032 is past ProfileEnumFiles' 0x20 bound; a directory is not
        // a profile file.
        std::fs::write(saves.join("mafia032.sav"), b"").unwrap();
        std::fs::create_dir_all(saves.join("mafia005.sav")).unwrap();
        assert_eq!(
            profile_file_intake(&root),
            Ok(ProfileFileIntake::NoProfileFile)
        );
        std::fs::write(saves.join("MAFIA031.SAV"), b"").unwrap();
        assert_eq!(
            profile_file_intake(&root),
            Ok(ProfileFileIntake::ProfileFilePresent)
        );
        std::fs::remove_dir_all(&root).unwrap();
        assert!(profile_file_intake(&root).is_err());
    }
}
