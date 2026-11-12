// Original-data menu and static scene inspection; mission scripts are not executed.
#[path = "assembly.rs"]
mod assembly;
#[path = "driving_view.rs"]
mod driving_view;
#[cfg(feature = "engine-lab")]
#[path = "engine_lab.rs"]
mod engine_lab;
#[cfg(not(feature = "engine-lab"))]
#[path = "engine_lab_disabled.rs"]
mod engine_lab;
mod lab_app;
mod lab_camera;
mod lab_catalog;
mod lab_files;
mod lab_owner_inspection;
#[cfg(feature = "engine-lab")]
mod lab_owner_ui;
#[path = "lab_playback.rs"]
mod lab_playback;
mod lab_preset;
mod lab_scene;
mod lab_state;
#[path = "mission_load_probe.rs"]
mod mission_load_probe;
use lhr_engine::menu;
#[path = "neighborhood.rs"]
mod neighborhood;
use glam::{Mat4, Quat, Vec3};
use lhr_render::{Camera, Mesh, Renderer, Scene, Texture, Vertex};
use std::{path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};
const INITIAL_CAR_VIEWPORT: [u32; 2] = [960, 640];

/// Lab and driving input must not interpret focus-restoration presses as
/// application actions or held input. Keep every release, including synthetic
/// releases, so existing keys are cleared.
fn application_key(
    ignore_synthetic_presses: bool,
    key: PhysicalKey,
    state: ElementState,
    is_synthetic: bool,
) -> Option<KeyCode> {
    if ignore_synthetic_presses && is_synthetic && state == ElementState::Pressed {
        return None;
    }
    match key {
        PhysicalKey::Code(code) => Some(code),
        PhysicalKey::Unidentified(_) => None,
    }
}

fn menu_navigation(code: KeyCode, shift: bool) -> Option<menu::Navigation> {
    match code {
        KeyCode::ArrowLeft => Some(menu::Navigation::Previous),
        KeyCode::ArrowRight => Some(menu::Navigation::Next),
        KeyCode::Tab if shift => Some(menu::Navigation::Previous),
        KeyCode::Tab => Some(menu::Navigation::Next),
        KeyCode::ArrowUp => Some(menu::Navigation::PreviousId),
        KeyCode::ArrowDown => Some(menu::Navigation::NextId),
        _ => None,
    }
}

#[derive(Clone)]
struct Options {
    mission_load_probe: bool,
    mission_play_probe: bool,
    probe_model_directories: Vec<String>,
    probe_render_capture: Option<PathBuf>,
    root: Option<PathBuf>,
    model: Option<String>,
    animation: Option<String>,
    animation_steps: Option<u32>,
    sample_animation_ms: Option<u32>,
    skeletal_animation: bool,
    capture: Option<PathBuf>,
    headless: bool,
    frames: u32,
    linear: bool,
    textures: String,
    synthetic: bool,
    lab: bool,
    lab_scene: Option<lab_scene::SyntheticPreset>,
    lab_preset: Option<PathBuf>,
    mission: Option<String>,
    car_inspection: bool,
    car_driving: bool,
    audio_capture: Option<PathBuf>,
    steer: Option<f32>,
    steer_steps: u32,
    /// Rail-forensics probe: "X,Z" world point. Loads the real MISE01
    /// scene and prints a ground-height grid around the point plus
    /// approach-line profiles, using the same MeshGroundSpace the
    /// play probe walks on. Read-only; drives no gameplay.
    rail_profile: Option<[f32; 2]>,
}
fn options() -> Result<Options, String> {
    parse_options(std::env::args().skip(1))
}
fn parse_options(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut result = Options {
        mission_load_probe: false,
        mission_play_probe: false,
        probe_model_directories: Vec::new(),
        probe_render_capture: None,
        root: None,
        model: None,
        animation: None,
        animation_steps: None,
        sample_animation_ms: None,
        skeletal_animation: false,
        capture: None,
        headless: false,
        frames: 0,
        linear: false,
        textures: "maps".into(),
        synthetic: false,
        lab: false,
        lab_scene: None,
        lab_preset: None,
        mission: None,
        car_inspection: false,
        car_driving: false,
        audio_capture: None,
        steer: None,
        steer_steps: 1,
        rail_profile: None,
    };
    let mut provided_options = Vec::new();
    let mut steer_steps_set = false;
    while let Some(arg) = args.next() {
        provided_options.push(arg.clone());
        match arg.as_str() {
            "--mission-load-probe" => result.mission_load_probe = true,
            "--mission-play-probe" => result.mission_play_probe = true,
            "--probe-model-dir" => result.probe_model_directories.push(args.next().ok_or("Missing probe model directory")?),
            "--probe-render-capture" => result.probe_render_capture = Some(args.next().ok_or("Missing probe GPU capture output")?.into()),
            "--textures" => result.textures = args.next().ok_or("Missing textures argument")?,
            "--root" => result.root = Some(args.next().ok_or("Missing root argument")?.into()),
            "--sample-animation-ms" => result.sample_animation_ms = Some(args.next().ok_or("Missing animation sample time")?.parse().map_err(|_| "Invalid animation sample time")?),
            "--animation-steps" => result.animation_steps = Some(args.next().ok_or("Missing animation steps")?.parse().map_err(|_|"Invalid animation steps")?),
            "--skeletal-animation" => result.skeletal_animation = true,
            "--animation" => result.animation = Some(args.next().ok_or("Missing animation argument")?),
            "--model" => result.model = Some(args.next().ok_or("Missing model argument")?),
            "--capture" => result.capture = Some(args.next().ok_or("Missing capture argument")?.into()),
            "--headless" => result.headless = true,
            "--linear" => result.linear = true,
            "--frames" => result.frames = args.next().ok_or("Missing frame count")?.parse().map_err(|_| "Invalid frame count")?,
            "--synthetic" => result.synthetic = true,
            "--lab-scene" => {
                result.lab_scene = Some(match args.next().ok_or("Missing Lab scene name")?.as_str() {
                    "gallery" => lab_scene::SyntheticPreset::Gallery,
                    "courtyard" => lab_scene::SyntheticPreset::Courtyard,
                    "primitives" => lab_scene::SyntheticPreset::Primitives,
                    "lights" => lab_scene::SyntheticPreset::LightStudy,
                    _ => return Err("Lab scene must be gallery, courtyard, primitives or lights".into()),
                });
            },
            "--lab-preset" => result.lab_preset = Some(args.next().ok_or("Missing Lab preset path")?.into()),
            "--lab" => {
                if !cfg!(feature = "engine-lab") {
                    return Err("--lab requires a build with --features engine-lab".into());
                }
                result.lab = true;
            },
            "--mission" => result.mission = Some(args.next().ok_or("Missing mission argument")?),
            "--car-inspection" => result.car_inspection = true,
            "--car-driving" => result.car_driving = true,
            "--audio-capture" => result.audio_capture = Some(args.next().ok_or("Missing audio capture output")?.into()),
            "--steer" => result.steer = Some(args.next().ok_or("Missing steering request")?.parse().map_err(|_| "Invalid steering request")?),
            "--steer-steps" => { steer_steps_set = true; result.steer_steps = args.next().ok_or("Missing steering preview step count")?.parse().map_err(|_| "Invalid steering preview step count")?; },
            "--rail-profile" => {
                let pair = args.next().ok_or("Missing rail profile X,Z argument")?;
                let mut parts = pair.split(',');
                let x: f32 = parts.next().ok_or("Rail profile needs X,Z")?.parse().map_err(|_| "Invalid rail profile X")?;
                let z: f32 = parts.next().ok_or("Rail profile needs X,Z")?.parse().map_err(|_| "Invalid rail profile Z")?;
                if parts.next().is_some() {
                    return Err("Rail profile needs exactly X,Z".into());
                }
                if !x.is_finite() || !z.is_finite() {
                    return Err("Rail profile X,Z must be finite".into());
                }
                result.rail_profile = Some([x, z]);
            },
            "--help" => return Err(concat!(
                "Usage: lhr-engine [--synthetic | --root DIRECTORY [--model RELATIVE_4DS [--animation RELATIVE_5DS] | --mission NAME]] ",
                "[--car-driving | --car-inspection [--steer VALUE [--steer-steps N]]] [--textures RELATIVE_DIRECTORY] [--headless --capture PNG [--animation-steps N]] [--frames N] [--linear]. ",
                "--lab requires --features engine-lab and a native window. It opens scenes/models, environment previews, camera and model tools, presets and actual driving input. F1 panel, F2 pause, F3 16 ms step, F4 reload. --synthetic --lab starts the public gallery; --root may also supply its asset browser. --lab-scene gallery|courtyard|primitives|lights selects a public --synthetic scene, including headless captures. --lab-preset FILE.lhr-lab loads presentation settings for --lab or --headless. --capture omits the Lab overlay. ",
                "--mission-load-probe --root DIRECTORY --mission NAME [--probe-model-dir DIRECTORY ...] [--probe-render-capture PNG] runs the shared CPU load-session diagnostic without a window. Optional GPU capture renders the actual retained mission Scene, and does not execute gameplay. Other presentation and playback flags are excluded. ",
                "--mission-play-probe --root DIRECTORY plays the MISE01 slice headless against original data without a window: intro cutscene lock, on-foot walk to the taxi stand, board the staged taxi00.4ds driving world and drive 150 m to Complete. Other presentation and playback flags are excluded. ",
                "--audio-capture WAV records the actual mixed driving audio and requires --car-driving. ",
                "--car-driving requires --root and --model and opens the original MISE01 driving world. W accelerates, S brakes, A/D steer, Space applies the handbrake; R reloads and Escape exits. The chase camera is a host view. ",
                "--car-inspection requires --root and --model; constructs and initializes the CPU car, then renders that same live arena's camera-selected owned LOD and source distance fade, with unlit materials. A/D previews the original wheel-steering update; arrows/W/S control the camera. R explicitly reloads the car and resets live input. Lighting, other material effects, body simulation, driving and mission execution are unavailable. ",
                "--steer accepts a finite normalized request in [-1,1]; --steer-steps 0..600 (default 1) applies that request through controlled 16 ms host inspection-clock steps before presentation. This previews actual initialized wheel frames, without translating the car or claiming a driving simulation. ",
                "Headless --animation-steps 0..600 uses controlled 16 ms inspection-clock steps; omit it for one platform-clock step. ",
                "Root alone opens the supported original Czech main-menu subset (New Game/Exit). ",
                "Menu: arrows, Tab/Shift+Tab, Enter or mouse. New Game loads static INTRO; Esc loads MISE01 with on-foot walking (WASD, Shift runs, arrows orbit, R/F zoom, C intro cutscene, Enter skips, E boards), then returns to menu. ",
                "Scene inspection: arrows orbit, W/S zoom. --animation alone admits a matched single-root position track. Add --skeletal-animation for sequential skeletal slot0 mode2 playback, or --sample-animation-ms MS for one independent skeletal pose. Original profile, cutscene, mission-script and gameplay execution are not implemented. ",
                "Sampling is provisional nearest (or --linear), not verified original behavior."
            ).into()),
            _ => return Err("Unknown command option".into()),
        }
    }
    if result.rail_profile.is_some() {
        for option in &provided_options {
            if !matches!(option.as_str(), "--rail-profile" | "--root") {
                return Err(format!("{option} is not used by --rail-profile; omit it"));
            }
        }
        if result.root.is_none() {
            return Err("--rail-profile requires --root".into());
        }
        return Ok(result);
    }
    if result.mission_play_probe {
        for option in &provided_options {
            if !matches!(option.as_str(), "--mission-play-probe" | "--root") {
                return Err(format!(
                    "{option} is not used by --mission-play-probe; omit it"
                ));
            }
        }
        if result.root.is_none() {
            return Err("--mission-play-probe requires --root".into());
        }
        return Ok(result);
    }
    if result.mission_load_probe {
        for option in &provided_options {
            if !matches!(
                option.as_str(),
                "--mission-load-probe"
                    | "--root"
                    | "--mission"
                    | "--probe-model-dir"
                    | "--probe-render-capture"
            ) {
                return Err(format!(
                    "{option} is not used by --mission-load-probe; omit it"
                ));
            }
        }
        if result.root.is_none() || result.mission.is_none() {
            return Err("--mission-load-probe requires --root and --mission".into());
        }
        mission_load_probe::validate_inputs(
            result.mission.as_deref().unwrap(),
            &result.probe_model_directories,
        )?;
        return Ok(result);
    }
    if !result.probe_model_directories.is_empty() {
        return Err("--probe-model-dir requires --mission-load-probe".into());
    }
    if result.probe_render_capture.is_some() {
        return Err("--probe-render-capture requires --mission-load-probe".into());
    }
    if result.skeletal_animation
        && (result.animation.is_none() || result.sample_animation_ms.is_some())
    {
        return Err(
            "--skeletal-animation requires --animation and excludes --sample-animation-ms".into(),
        );
    }
    if result.sample_animation_ms.is_some()
        && (result.animation.is_none() || result.animation_steps.is_some())
    {
        return Err(
            "--sample-animation-ms requires --animation and excludes --animation-steps".into(),
        );
    }
    if let Some(steps) = result.animation_steps {
        if !result.headless || result.animation.is_none() || steps > 600 {
            return Err("--animation-steps requires headless animation and a count 0..600".into());
        }
    }
    if result.animation.is_some() && (result.root.is_none() || result.model.is_none()) {
        return Err("Animation inspection requires --root and --model".into());
    }
    if result.model.is_some() && result.root.is_none() {
        return Err("A model requires an installation root".into());
    }
    if result.car_inspection
        && (result.root.is_none()
            || result.model.is_none()
            || result.animation.is_some()
            || result.mission.is_some()
            || result.textures != "maps")
    {
        return Err("--car-inspection requires --root and --model, original maps, and no animation or mission selection".into());
    }
    if result.audio_capture.is_some() && !result.car_driving {
        return Err("--audio-capture requires --car-driving".into());
    }
    if result.car_driving
        && (result.root.is_none()
            || result.model.is_none()
            || result.car_inspection
            || result.animation.is_some()
            || result.mission.is_some()
            || result.textures != "maps"
            || result.headless
            || result.steer.is_some()
            || steer_steps_set)
    {
        return Err("--car-driving requires --root and --model, original maps, a window, and excludes inspection, steering preview, animation and mission selection".into());
    }
    if result
        .steer
        .is_some_and(|value| !value.is_finite() || !(-1.0..=1.0).contains(&value))
    {
        return Err("--steer requires a finite value in [-1,1]".into());
    }
    if (result.steer.is_some() && !result.car_inspection)
        || (steer_steps_set && result.steer.is_none())
        || result.steer_steps > 600
    {
        return Err(
            "Steering preview requires --car-inspection; --steer-steps 0..600 requires --steer"
                .into(),
        );
    }
    if result.mission.is_some() && (result.root.is_none() || result.model.is_some()) {
        return Err("A mission requires an installation root and no model".into());
    }
    if let Some(name) = &result.mission {
        mission_load_probe::validate_mission_name(name)?;
    }
    if result.lab && result.headless {
        return Err("Engine Lab requires a native window; omit --headless".into());
    }
    if result.headless && result.frames != 0 {
        return Err("Headless mode captures exactly one frame; omit --frames".into());
    }
    if result.headless && result.capture.is_none() {
        return Err("Headless mode requires a capture output".into());
    }
    if (!result.synthetic && result.root.is_none())
        || (result.synthetic && result.root.is_some() && !result.lab)
    {
        return Err(
            "Select --synthetic or --root; combining them requires --lab for the asset browser"
                .into(),
        );
    }
    if result.synthetic
        && (result.model.is_some() || result.mission.is_some() || result.animation.is_some())
    {
        return Err("A synthetic scene excludes model, mission and animation selection".into());
    }
    if result.lab_scene.is_some() && !result.synthetic {
        return Err("--lab-scene requires --synthetic".into());
    }
    if result.lab_preset.is_some() && !result.lab && !result.headless {
        return Err("--lab-preset requires --lab or --headless".into());
    }
    if result.capture.is_some() && !result.headless && result.frames == 0 {
        return Err("Window capture requires a bounded --frames count".into());
    }
    Ok(result)
}
fn synthetic() -> Scene {
    Scene {
        meshes: vec![Mesh {
            material: Default::default(),
            vertices: vec![
                Vertex {
                    position: [-1., -1., 0.],
                    uv: [0., 1.],
                },
                Vertex {
                    position: [1., -1., 0.],
                    uv: [1., 1.],
                },
                Vertex {
                    position: [1., 1., 0.],
                    uv: [1., 0.],
                },
                Vertex {
                    position: [-1., 1., 0.],
                    uv: [0., 0.],
                },
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: Some(Arc::new(Texture {
                width: 2,
                height: 2,
                repeat: [true, true],
                rgba: vec![
                    255, 32, 32, 255, 32, 255, 32, 255, 32, 32, 255, 255, 255, 255, 32, 255,
                ],
            })),
        }],
    }
}
fn charge_scene(total: &mut usize, additional: usize, limit: usize) -> Result<(), String> {
    let next = total.checked_add(additional).ok_or("Scene size overflow")?;
    if next > limit {
        return Err("Scene exceeds resource quota".into());
    }
    *total = next;
    Ok(())
}
fn source_rotation(q: [f32; 4]) -> Result<Quat, String> {
    let rotation = Quat::from_xyzw(-q[1], -q[2], -q[3], q[0]);
    if !rotation.is_finite() || (rotation.length_squared() - 1.0).abs() > 0.00001 {
        return Err("Model quaternion is not normalized".into());
    }
    Ok(rotation)
}
fn material_effects_omitted(material: &lhr_assets::Material) -> bool {
    // Legacy asset-only geometry cannot execute these effects. The assembly
    // owner separately admits animation when it binds actual live materials.
    const UNSUPPORTED_EFFECT_FLAGS: u32 =
        0x80000 | 0x01000000 | 0x04000000 | 0x20000000 | 0x40000000 | 0x80000000;
    material.flags & UNSUPPORTED_EFFECT_FLAGS != 0
        || material.alpha_texture.is_some()
        || material.environment_texture.is_some()
        || material.emission != [0.0; 3]
        || material.opacity != 1.0
        || material.animation.is_some()
}
fn parse_standalone_model(data: &[u8]) -> Result<lhr_assets::Model, lhr_assets::Error> {
    lhr_assets::parse_four_ds(data)
        .or_else(|_| lhr_assets::parse_four_ds_skin(data))
        .or_else(|_| lhr_assets::parse_four_ds_singlemorph(data))
        .or_else(|_| lhr_assets::parse_four_ds_animated_model(data))
}

enum AnimationPlayback {
    Position(Box<lhr_engine::animation_binding::PositionPlayback>),
    Skeletal(Box<lhr_engine::skeletal_playback::SkeletalPlayback>),
}
impl AnimationPlayback {
    fn description(&self) -> &'static str {
        match self {
            Self::Position(_) => "bounded position animation",
            Self::Skeletal(_) => "bounded sequential skeletal animation",
        }
    }
}
struct LoadedScene {
    snapshots: lhr_engine::scene_render::SceneSnapshotSequence,
    canonical: Option<lhr_engine::scene_render::CanonicalSceneOwner>,
    scene_frame: Option<lhr_render::SceneRenderFrame>,
    scene: Scene,
    camera: Camera,
    runtime: lhr_engine::runtime::RetainedScene,
    animation: Option<AnimationPlayback>,
    car: Option<lhr_engine::car_runtime::GuardedCarRuntime>,
}
/// Controllers must release their bindings before the actual model/material
/// owner is closed. This is explicit host cleanup, not a native destructor.
fn close_retained_materials<P>(
    playback: &mut Option<P>,
    runtime: &mut lhr_engine::runtime::RetainedScene,
) -> Result<(), String> {
    drop(playback.take());
    runtime.close_owned_material_draws()
}

/// Keep every post-admission setup error inside the same explicit lifetime
/// boundary, including camera/metadata setup after controller construction.
fn setup_retained_scene<P, T>(
    runtime: &mut lhr_engine::runtime::RetainedScene,
    playback: &mut Option<P>,
    setup: impl FnOnce(&mut lhr_engine::runtime::RetainedScene, &mut Option<P>) -> Result<T, String>,
) -> Result<T, String> {
    match setup(runtime, playback) {
        Ok(value) => Ok(value),
        Err(error) => {
            let cleanup = close_retained_materials(playback, runtime);
            Err(format!("{error}; retained material cleanup: {cleanup:?}"))
        }
    }
}
fn load(
    options: &Options,
    viewport: [u32; 2],
    driver_time: u32,
    patches: &lhr_engine::program_patch::ProgramPatchStartup,
) -> Result<LoadedScene, String> {
    if options.synthetic {
        let scene = match lab_app::synthetic_preset(options) {
            Some(preset) => preset.build()?,
            None => synthetic(),
        };
        let camera = Camera::fit(&scene)?;
        return Ok(LoadedScene {
            snapshots: Default::default(),
            canonical: None,
            scene_frame: None,
            scene,
            camera,
            runtime: lhr_engine::runtime::RetainedScene::default(),
            animation: None,
            car: None,
        });
    }
    let root = options
        .root
        .as_ref()
        .ok_or("An original scene requires an installation root")?;
    let installation = lhr_assets::Installation::open(root)
        .map_err(|e| format!("Cannot open asset installation: {e}"))?;
    if options.car_inspection || options.car_driving {
        return load_car(
            installation,
            options.model.as_deref().ok_or("Car model absent")?,
            options.steer,
            options.steer_steps,
            options.car_driving,
            viewport,
            driver_time,
            patches,
        );
    }
    if options.model.is_none() {
        let mission = options.mission.as_deref().unwrap_or("00menu");
        let mut canonical =
            lhr_engine::scene_render::CanonicalSceneOwner::open_at_driver_time_and_patches(
                &installation,
                mission,
                viewport,
                driver_time as i32,
                &patches
                    .lease()
                    .map_err(|e| format!("Startup patch owner: {e:?}"))?,
            )?;
        if let Some(frontier) = &canonical.frontier {
            println!(
                "Canonical scene2 retained frontier: cursor {}, entry {:#x}: {}; mission initialization remains incomplete",
                frontier.cursor, frontier.entry, frontier.detail
            );
        }
        let camera = canonical.camera_control()?;
        let mut snapshots = lhr_engine::scene_render::SceneSnapshotSequence::default();
        let snapshot = match canonical.snapshot(viewport, snapshots.next_number()?) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let cleanup = canonical.close();
                return Err(format!("{error}; canonical Scene cleanup: {cleanup:?}"));
            }
        };
        println!(
            "Canonical Scene active-camera rendering: {} selected material groups; source sector/portal/occluder visibility, main/backdrop/near projections; bounded material support",
            snapshot.scene.meshes.len()
        );
        return Ok(LoadedScene {
            snapshots,
            scene: snapshot.scene,
            camera,
            runtime: Default::default(),
            animation: None,
            car: None,
            canonical: Some(canonical),
            scene_frame: Some(snapshot.frame),
        });
    }
    if options
        .mission
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("mise01"))
    {
        return neighborhood::load(&installation, &options.textures);
    }
    let path = options.model.clone().unwrap_or_else(|| {
        format!(
            "missions/{}/scene.4ds",
            options.mission.as_deref().unwrap_or("00menu")
        )
    });
    let data = installation
        .read(&path)
        .map_err(|e| format!("Cannot read selected geometry: {e}"))?;
    let mut animation_sample = None;
    let mut skeletal_source = None;
    let (model, animation_source) = if options.skeletal_animation
        || options.sample_animation_ms.is_some()
    {
        let animation_path = options
            .animation
            .as_ref()
            .ok_or("Missing sample animation")?;
        let bytes = installation
            .read(animation_path)
            .map_err(|error| error.to_string())?;
        let animation =
            lhr_assets::animation::parse_5ds(&bytes).map_err(|error| error.to_string())?;
        let model = parse_standalone_model(&data)
            .map_err(|error| format!("Unsupported skeletal model source: {error}"))?;
        let animation = Arc::new(animation);
        if options.skeletal_animation {
            skeletal_source = Some(animation);
        } else {
            animation_sample = Some((animation, options.sample_animation_ms.unwrap()));
        }
        (Arc::new(model), None)
    } else if let Some(animation_path) = &options.animation {
        let animation_bytes = installation
            .read(animation_path)
            .map_err(|error| format!("Cannot read selected animation: {error}"))?;
        let source = lhr_assets::parse_four_ds_position_animation(&data, &animation_bytes)
            .map_err(|error| format!("Unsupported position animation source: {error}"))?;
        let (model, count, animation) = source.into_parts();
        let model = Arc::new(model);
        let source = lhr_engine::animation_binding::PositionSource::new(&model, count, animation)
            .map_err(|error| format!("Unsupported position binding: {error:?}"))?;
        (model, Some(source))
    } else {
        let model = if options.model.is_some() {
            parse_standalone_model(&data)
        } else {
            lhr_assets::parse_four_ds_scene(&data)
        }
        .map_err(|error| format!("Selected geometry is unsupported or malformed: {error}"))?;
        (Arc::new(model), None)
    };
    if options.model.is_none() {
        let omitted = model
            .materials
            .iter()
            .filter(|material| material_effects_omitted(material))
            .count();
        println!(
            "Static scene subset: {} frames; {omitted} materials have effects not rendered; animation, lighting and mission scripts are not executed",
            model.frames.len()
        );
    }
    let mut builder = assembly::SceneBuilder::new(&installation, &options.textures);
    builder.set_material_driver_time(driver_time as i32);
    if options.model.is_some() && animation_source.is_none() {
        builder.use_original_model_loading();
    }
    if let Some((animation, milliseconds)) = animation_sample {
        builder.sample_original_animation(animation, milliseconds);
    }
    builder.append_retained(
        &path,
        model,
        Mat4::IDENTITY,
        None,
        options.model.is_some(),
        None,
    )?;
    let (scene, mut runtime) = builder.finish()?;
    let mut animation = None;
    let camera = setup_retained_scene(&mut runtime, &mut animation, |runtime, animation| {
        *animation = animation_source
            .map(|source| {
                lhr_engine::animation_binding::PositionPlayback::new(
                    source,
                    runtime.models[0].identity,
                    &mut runtime.models[0].native,
                )
                .map(|playback| AnimationPlayback::Position(Box::new(playback)))
                .map_err(|error| format!("Position initialization failed: {error:?}"))
            })
            .transpose()?;
        if let Some(AnimationPlayback::Position(playback)) = &*animation {
            runtime.charge_position_binding(playback)?;
        }
        if let Some(source) = skeletal_source {
            let playback = runtime.bind_skeletal(0, source)?;
            println!(
                "Bound original skeletal controller: slot0 mode2, {} ms; position/rotation, Target, morph and first-LOD CPU skin; host inspection initialization, camera and rendering; no mission integration",
                playback.duration_ms()
            );
            *animation = Some(AnimationPlayback::Skeletal(Box::new(playback)));
        }
        if matches!(&*animation, Some(AnimationPlayback::Position(_))) {
            println!(
                "Bound original position-only animation: slot0 mode2; one model wrapper and root mesh; native control flow, pose, frame caches and matrix/point arithmetic; host camera, handedness and rendering, no mission integration"
            );
        }
        if options.model.is_some() {
            Camera::fit(&scene)
        } else {
            authored_camera(
                &installation,
                &scene,
                options.mission.as_deref().unwrap_or("00menu"),
                "intro_pos",
                runtime,
            )
        }
    })?;
    Ok(LoadedScene {
        snapshots: Default::default(),
        canonical: None,
        scene_frame: None,
        scene,
        camera,
        runtime,
        animation,
        car: None,
    })
}
/// Triangle soup of one render-scene mesh for the walk probes.
fn scene_mesh_tris(meshes: &[Mesh], index: usize) -> Option<Vec<[[f32; 3]; 3]>> {
    let mesh = meshes.get(index)?;
    let mut tris = Vec::new();
    for face in mesh.indices.chunks_exact(3) {
        let mut tri = [[0.0; 3]; 3];
        for (slot, corner) in tri.iter_mut().zip(face.iter()) {
            *slot = mesh.vertices.get(*corner as usize)?.position;
        }
        tris.push(tri);
    }
    if tris.is_empty() {
        None
    } else {
        Some(tris)
    }
}

/// Highest real-geometry ground under `(x, z)` in a render scene.
fn scene_ground_at(
    space: &lhr_engine::mission_play::MeshGroundSpace,
    meshes: &[Mesh],
    x: f32,
    z: f32,
    top_y: f32,
    drop: f32,
) -> Option<f32> {
    space.ground_below(&|index| scene_mesh_tris(meshes, index), x, z, top_y, drop)
}

/// Headless end-to-end exercise of the MISE01 playable slice against the
/// user's original data: intro cutscene lock over the real recorder state
/// machine, on-foot walk to the staged taxi stand on REAL scene geometry,
/// board the real `models/taxi00.4ds` driving world and drive 150 m to
/// `Complete`. Prints every stage transition; fails loudly on the first
/// stage that does not advance. No window or GPU is required.
/// Rail forensics: ground-height grids from the real MISE01 scene around
/// a boxed pocket, using the same MeshGroundSpace the play probe walks
/// on. Read-only; drives no gameplay. The tunnel rail at [-1318,153]
/// beached the taxi beyond any wheel choreography (reverse 0.0 m with
/// the motor screaming), so the profile decides what the rail is: a
/// narrow ridge shows as a 1-2 m wide high line in the grid, a pit edge
/// as a one-sided drop, a flat floor as level ground (i.e. not geometry
/// at all — then the beach is a response/friction artifact).
fn rail_profile_probe(options: &Options, center: [f32; 2]) -> Result<(), String> {
    use lhr_engine::mission_play::MeshGroundSpace;
    let root = options.root.as_deref().ok_or("Probe needs --root")?;
    let installation = lhr_assets::Installation::open(root).map_err(|e| e.to_string())?;
    let mut patches = lhr_engine::program_patch::ProgramPatchStartup::default();
    patches
        .initialize(&installation)
        .map_err(|e| format!("Startup program patches: {e:?}"))?;
    let mut candidate = options.clone();
    candidate.mission = Some("mise01".into());
    let loaded = load(&candidate, INITIAL_CAR_VIEWPORT, 0, &patches)?;
    println!(
        "probe rail: {} static meshes loaded for MISE01",
        loaded.scene.meshes.len()
    );
    let ground = MeshGroundSpace::build(loaded.scene.meshes.len(), &|index| {
        loaded.scene.meshes.get(index).and_then(|mesh| {
            let verts: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
            lhr_engine::mission_play::mesh_aabb(&verts)
        })
    });
    let tris_of = |index: usize| scene_mesh_tris(&loaded.scene.meshes, index);
    // Two ceilings: +30 finds the surface/deck above, -20 finds the tunnel
    // roadway the taxi beached on (taxi origin sat at -25.3).
    for (tag, top_y, max_drop) in [("deck", 30.0f32, 200.0f32), ("road", -20.0f32, 40.0f32)] {
        println!(
            "probe rail grid {tag} top={top_y:.1} around [{:.1}, {:.1}]:",
            center[0], center[1]
        );
        for dz in -4..=4 {
            let mut row = format!("{:>4} ", dz * 2);
            for dx in -4..=4 {
                let x = center[0] + dx as f32 * 2.0;
                let z = center[1] + dz as f32 * 2.0;
                match ground.ground_below(&tris_of, x, z, top_y, max_drop) {
                    Some(h) => row.push_str(&format!("{h:7.1}")),
                    None => row.push_str("    ---"),
                }
            }
            println!("{row}");
        }
    }
    // Approach lines from the pocket toward the escape headings the probe
    // tried (135 deg SE and 0 deg N), 0-15 m in 1 m steps, road ceiling.
    for deg in [0.0f32, 135.0] {
        let rad = deg.to_radians();
        let mut line = format!("probe rail line {deg:.0}deg:");
        for d in 0..=15 {
            let x = center[0] + rad.sin() * d as f32;
            let z = center[1] + rad.cos() * d as f32;
            match ground.ground_below(&tris_of, x, z, -20.0, 40.0) {
                Some(h) => line.push_str(&format!(" {h:.1}")),
                None => line.push_str(" ---"),
            }
        }
        println!("{line}");
    }
    Ok(())
}

fn mission_play_probe(options: &Options) -> Result<(), String> {
    use lhr_engine::mission_play::{
        mesh_aabb, CutscenePlayback, MeshGroundSpace, Mise01Progress, Mise01Stage, OnFootPlayer,
        WalkInput,
    };
    let root = options.root.as_deref().ok_or("Probe needs --root")?;
    // 1. Cutscene lock over the real recorder predicate and state machine.
    let mut progress = Mise01Progress::fresh();
    let mut show = lhr_engine::cutscene_owner::CutsceneShowStart::default();
    let mut recorder = lhr_engine::cutscene_owner::CutsceneOwner::constructed(
        lhr_engine::cutscene_owner::CutsceneConstructorSeed {
            outer_allocator_byte: 0,
            manager_address_high_byte: 0,
        },
    );
    let mut cutscene = CutscenePlayback::idle();
    assert_eq!(progress.stage(), Mise01Stage::WatchIntro);
    if !cutscene.start(8000) {
        return Err("Cutscene refused to start".into());
    }
    recorder.recorder_set_state(
        lhr_engine::cutscene_owner::CUTSCENE_STATE_1,
        &mut [],
        &mut [],
    );
    recorder.stage_play_ticks(8000);
    show.set_show_start(1);
    // Dubbing runs the real DabingTick on a staged subtitle channel,
    // like vfunc_49 does; the silent audio probe stages no voice.
    let mut dub = lhr_engine::dabing::DabingOwner::new();
    dub.dabing_init();
    dub.begin_subtitle_only(0, 48)
        .map_err(|e| format!("Subtitle channel unavailable: {e:?}"))?;
    let dub_audio = lhr_engine::mission_play::SilentDabingAudio;
    let mut dub_faces = lhr_engine::mission_play::FaceWeightLog::default();
    let start_fade = dub
        .channel(0)
        .map_err(|e| format!("Subtitle channel unreadable: {e:?}"))?
        .fade_ms;
    if !recorder.recorder_is_playing() {
        return Err("Real IsPlaying predicate did not lock".into());
    }
    let mut cut_ticks = 0u32;
    let mut mid_fade = start_fade;
    while recorder.recorder_is_playing() {
        recorder.stage_play_ticks(recorder.play_ticks().saturating_sub(16));
        let timer_done = cutscene.advance(16);
        dub.dabing_tick(&dub_audio, &mut dub_faces, 16);
        cut_ticks += 1;
        if cut_ticks > 1000 {
            return Err("Cutscene did not finish within 1000 ticks".into());
        }
        if cut_ticks == 75 {
            mid_fade = dub
                .channel(0)
                .map_err(|e| format!("Subtitle channel unreadable: {e:?}"))?
                .fade_ms;
        }
        if timer_done {
            recorder.stage_play_ticks(0);
            recorder.recorder_set_state(
                lhr_engine::cutscene_owner::CUTSCENE_STOPPED,
                &mut [],
                &mut [],
            );
        }
    }
    let end_fade = dub
        .channel(0)
        .map_err(|e| format!("Subtitle channel unreadable: {e:?}"))?
        .fade_ms;
    dub.recorder_close_reset(&mut dub_faces, &dub_audio);
    show.set_show_start(0);
    progress.cutscene_done();
    println!(
        "probe cutscene: real IsPlaying lock for {cut_ticks} ticks, state={} prev={} ticks={} show={}; dub fade {start_fade}->{mid_fade}->{end_fade} face_writes={}; stage={:?}",
        recorder.recorder_get_state(),
        recorder.previous_state(),
        recorder.play_ticks(),
        show.staged_ticks(),
        dub_faces.writes().len(),
        progress.stage()
    );
    if progress.stage() != Mise01Stage::ReachTaxiStand || show.staged_ticks() != 0 {
        return Err("Cutscene did not hand over to ReachTaxiStand".into());
    }
    // 1b. Real script gate: GameInitStart plus bounded scheduling must stage
    // mission objectives before the on-foot leg is authorized. Geometric
    // arrival alone never completes the mission without them.
    let (script_objectives, script_subtitles, script_note) =
        mission_load_probe::staged_script_content(root, "mise01")?;
    if script_objectives.is_empty() {
        return Err(
            "GameInitStart staged no objectives; completion refused without real script output"
                .into(),
        );
    }
    println!(
        "probe scripts: GameInitStart staged objectives {:?}, subtitles {:?}; scheduling {}",
        script_objectives, script_subtitles, script_note
    );
    // 2. Load the real MISE01 scene and walk 25 m east on its geometry.
    let installation = lhr_assets::Installation::open(root).map_err(|e| e.to_string())?;
    let mut patches = lhr_engine::program_patch::ProgramPatchStartup::default();
    patches
        .initialize(&installation)
        .map_err(|e| format!("Startup program patches: {e:?}"))?;
    let mut candidate = options.clone();
    candidate.mission = Some("mise01".into());
    let loaded = load(&candidate, INITIAL_CAR_VIEWPORT, 0, &patches)?;
    println!(
        "probe scene: {} static meshes loaded for MISE01",
        loaded.scene.meshes.len()
    );
    println!(
        "probe camera: eye [{:.1}, {:.1}, {:.1}] target [{:.1}, {:.1}, {:.1}]",
        loaded.camera.eye.x,
        loaded.camera.eye.y,
        loaded.camera.eye.z,
        loaded.camera.target.x,
        loaded.camera.target.y,
        loaded.camera.target.z
    );
    {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut n = 0u64;
        for mesh in &loaded.scene.meshes {
            for v in &mesh.vertices {
                for a in 0..3 {
                    min[a] = min[a].min(v.position[a]);
                    max[a] = max[a].max(v.position[a]);
                }
                n += 1;
            }
        }
        println!("probe scene bounds: [{min:.1?}] [{max:.1?}] verts={n}");
    }
    let ground = MeshGroundSpace::build(loaded.scene.meshes.len(), &|index| {
        loaded.scene.meshes.get(index).and_then(|mesh| {
            let verts: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
            mesh_aabb(&verts)
        })
    });
    let tris_of = |index: usize| scene_mesh_tris(&loaded.scene.meshes, index);
    // Real spawn positions from the MISE01 scene2 actor metadata.
    let (player_spot, taxi_spot) =
        lhr_engine::mission_play::mission_actor_spawn(&installation, "mise01")?;
    println!(
        "probe actors: player [{:.1}, {:.1}, {:.1}], taxi [{:.1}, {:.1}, {:.1}]",
        player_spot[0], player_spot[1], player_spot[2], taxi_spot[0], taxi_spot[1], taxi_spot[2]
    );
    let spawn_ground = ground
        .ground_below(
            &tris_of,
            player_spot[0],
            player_spot[2],
            player_spot[1] + 100.0,
            200.0,
        )
        .or_else(|| {
            ground.ground_below(
                &tris_of,
                player_spot[0],
                player_spot[2],
                player_spot[1] + 2.0,
                10.0,
            )
        })
        .unwrap_or(player_spot[1]);
    // Walk toward the taxi: face it from the spawn.
    let to_taxi = [taxi_spot[0] - player_spot[0], taxi_spot[2] - player_spot[2]];
    let cam_yaw = to_taxi[0].atan2(to_taxi[1]);
    let mut player = OnFootPlayer::spawn([player_spot[0], spawn_ground, player_spot[2]], cam_yaw);
    if !progress.stage_taxi_stand([taxi_spot[0], spawn_ground, taxi_spot[2]], 3.0) {
        return Err("Taxi stand refused to stage".into());
    }
    let mut walk_ticks = 0u32;
    let mut min_ground = f32::INFINITY;
    let mut max_ground = f32::NEG_INFINITY;
    let mut blocked_ticks = 0u32;
    while progress.stage() == Mise01Stage::ReachTaxiStand {
        let pos = player.pos();
        let ground_now = ground.ground_below(&tris_of, pos[0], pos[2], pos[1] + 0.6, 4.0);
        if let Some(h) = ground_now {
            min_ground = min_ground.min(h);
            max_ground = max_ground.max(h);
        }
        let before = pos;
        player.step(
            16,
            WalkInput {
                forward: 1.0,
                strafe: 0.0,
                run: false,
            },
            cam_yaw,
            ground_now,
            |from, delta| {
                let (rest, blocked) = ground.sweep_walls(&tris_of, from, delta);
                if blocked {
                    blocked_ticks += 1;
                }
                blocked.then(|| lhr_engine::human_move_coll::StepContact {
                    newpos: rest,
                    material: 0,
                    owner_arm: false,
                })
            },
        );
        progress.player_moved(player.pos());
        walk_ticks += 1;
        if walk_ticks > 3000 {
            return Err(format!(
                "Walker never reached the stand (pos {:?})",
                player.pos()
            ));
        }
        let _ = before;
    }
    println!(
        "probe walk: reached the stand after {walk_ticks} ticks ({blocked_ticks} wall-blocked) at pos [{:.2}, {:.2}, {:.2}], real ground range [{min_ground:.2}, {max_ground:.2}]; stage={:?}",
        player.pos()[0],
        player.pos()[1],
        player.pos()[2],
        progress.stage()
    );
    // Wall-sweep proof on real geometry: 200 m rays in 8 compass headings.
    let stand = player.pos();
    // Diagnostic grid: ground heights around the stand expose whether the
    // render scene covers the metadata spawn frame.
    let mut grid_hits = 0u32;
    let mut grid_min = f32::INFINITY;
    let mut grid_max = f32::NEG_INFINITY;
    for gx in -2..=2 {
        for gz in -2..=2 {
            if let Some(h) = ground.ground_below(
                &tris_of,
                stand[0] + gx as f32 * 5.0,
                stand[2] + gz as f32 * 5.0,
                stand[1] + 2.0,
                50.0,
            ) {
                grid_hits += 1;
                grid_min = grid_min.min(h);
                grid_max = grid_max.max(h);
            }
        }
    }
    println!(
        "probe grid: {grid_hits}/25 grounded around the stand, range [{grid_min:.2}, {grid_max:.2}]"
    );
    // High-ray diagnostic: is there any geometry above the metadata spawn?
    {
        let mut hits = 0u32;
        let mut hi_min = f32::INFINITY;
        let mut hi_max = f32::NEG_INFINITY;
        for gx in -2..=2 {
            for gz in -2..=2 {
                if let Some(h) = ground.ground_below(
                    &tris_of,
                    stand[0] + gx as f32 * 5.0,
                    stand[2] + gz as f32 * 5.0,
                    stand[1] + 100.0,
                    200.0,
                ) {
                    hits += 1;
                    hi_min = hi_min.min(h);
                    hi_max = hi_max.max(h);
                }
            }
        }
        println!("probe high-ray grid: {hits}/25 grounded, range [{hi_min:.2}, {hi_max:.2}]");
    }
    let mut blocked_dirs = 0u32;
    for k in 0..8 {
        let heading = k as f32 * std::f32::consts::FRAC_PI_4;
        let (rest, blocked) = ground.sweep_walls(
            &tris_of,
            stand,
            [heading.sin() * 200.0, 0.0, heading.cos() * 200.0],
        );
        let travelled = (rest[0] - stand[0]).hypot(rest[2] - stand[2]);
        if blocked {
            blocked_dirs += 1;
        }
        println!(
            "probe walls: heading {:>3}° travelled {travelled:6.1} m blocked={blocked}",
            (k as f32 * 45.0) as u32,
        );
    }
    if blocked_dirs == 0 {
        return Err("Wall sweep met no MISE01 geometry in 8 headings".into());
    }
    // 3. Board the real taxi driving world and drive the Dijkstra path to
    // the mission end detector. Reuses the installation and patches from
    // the scene load.
    let road_bytes = installation
        .read_optional("missions/mise01/road.bin")
        .map_err(|e| e.to_string())?
        .ok_or("MISE01 road.bin absent")?;
    let network = lhr_engine::road_network::parse_road_network(&road_bytes)
        .map_err(|e| format!("Road network parse stopped: {e:?}"))?;
    println!(
        "probe roads: {} nodes loaded for the destination",
        network.nodes.len()
    );
    // Destination anchor: the mission end detector from scene2 metadata
    // (detector_konec: real MISE01 end-of-drive position, name-attested).
    // Looked up before the installation moves into the taxi world below.
    let konec = lhr_engine::mission_play::mission_end_detector(&installation, "mise01")
        .map_err(|error| format!("Mission end detector: {error}"))?;
    println!(
        "probe konec: detector_konec at [{:.1}, {:.1}, {:.1}]",
        konec[0], konec[1], konec[2]
    );
    // Scene2 object census anchor: parsed before installation moves into
    // the taxi world below. The first boxed window reports named objects
    // near the wedge so a prop vs. pocket can be told apart.
    let scene_objects: Vec<(String, [f32; 3])> = {
        let bytes = installation
            .read_optional("missions/mise01/scene2.bin")
            .map_err(|e| e.to_string())?
            .ok_or("MISE01 scene2.bin absent")?;
        let scene =
            lhr_assets::parse_scene2(&bytes).map_err(|e| format!("Scene2 parse stopped: {e:?}"))?;
        scene
            .objects
            .iter()
            .filter_map(|o| o.world_position.map(|p| (o.name.clone(), p)))
            .collect()
    };
    println!(
        "probe scene objects: {} with world positions",
        scene_objects.len()
    );
    let lease = patches
        .lease()
        .map_err(|e| format!("Startup patch owner: {e:?}"))?;
    let mut taxi = lhr_engine::car_runtime::GuardedCarRuntime::open_driving_scene_with_patches(
        installation,
        "models/taxi00.4ds",
        lhr_engine::car_runtime::CarRuntimeOptions {
            material_driver_time: 0,
            ..Default::default()
        },
        "mise01",
        &lease,
    )
    .map_err(|e| e.to_string())?;
    taxi.start_driving("mise01").map_err(|e| e.to_string())?;
    // Read the taxi spawn pose with one neutral step, then drive two ticks
    // straight to seed motion.
    let spawn = taxi
        .driving_step(16, lhr_engine::car_runtime::CarDrivingInput::default())
        .map_err(|e| e.to_string())?
        .position;
    let mut straight = spawn;
    for _ in 0..2 {
        straight = taxi
            .driving_step(
                16,
                lhr_engine::car_runtime::CarDrivingInput {
                    throttle: 1.0,
                    brake: 0.0,
                    steer: 0.0,
                    handbrake: false,
                    reverse: false,
                },
            )
            .map_err(|e| e.to_string())?
            .position;
    }
    // Konec-anchored waypoint drive: the Dijkstra path from the taxi
    // spawn to the road node nearest detector_konec (real MISE01
    // end-of-drive position, name-attested). Reverse gear exists for
    // backing out of the alley pinch near [240,144]; lamp contacts
    // stage instead of aborting since the 0xd6580 dispatch. Blind
    // waypoints still cannot promise zero prop contact, so the probe
    // fails loudly with its boxed position if the taxi wedges.
    let node_count = network.nodes.len();
    let node_pos = |i: usize| network.nodes.get(i).map(|node| node.position());
    let neighbors = |node: usize, out: &mut Vec<(usize, f32)>| {
        if let Some(record) = network.nodes.get(node) {
            for lane in 0..lhr_engine::road_network::ROAD_LANES {
                let neighbor = record.lane_neighbor(lane);
                if neighbor != 0xFFFF {
                    out.push((neighbor as usize, record.lane_weight(lane)));
                }
            }
        }
    };
    // Learned trap ledger: boxed poses that defeated escape steering,
    // K-turn rocking and the clutch lift (open-diff burnout with no
    // native traction control to brake the free wheel). Seeded with
    // twenty-four evidenced furniture pockets: [-1314,-227] (clutch
    // stall-shudder wedge), [-628,164] (beached/dangling driven wheels),
    // [-147,-163] (escape + K-turn + lift all defeated), [-1265.7,-426.9]
    // (all wheels in contact on flat ground, 0.0 m/s — hung on a low
    // rail), [-1307.8,-702.0] / [-1305.8,-714.0] (rejoin pair),
    // [-1349.6,-638.1] (K-turn eaten by a latched U-turn),
    // [-1431.6,-248.9] (U-turn fix verified live, then re-wedged),
    // [-1276.4,-7.5,84.7] (beached, 3 wheels dangling),
    // [-1257.6,-7.9,71.5] (probe_rejoin: rejoin fires after the
    // box-reset fix, K-turn runs 5.4->9.9->11.9 m to wp 2/54, then
    // beaches 25 m on), [-1287.0,-6.7,-706.1] / [-1298.0,-6.5,-718.8]
    // (probe_p10: same furniture field as seeds 5-6, rotation churn),
    // [-1253.7,-7.7,24.8] / [-1237.8,-7.8,40.3] (probe_pullout: rejoin
    // fires, K-turn runs full phases, wedges 25 m past wp 1 on flat
    // ground with all wheels in contact — stall-shudder equilibrium),
    // [-1290.0,-6.7,-714.5] / [-1280.7,-6.6,-704.8] (probe crash3 run:
    // same field again, 2 m-apart re-learns; ledger churn),
    // [-1428.0,-6.8,-234.1] / [-1438.8,-6.9,-229.2] (probe p16: rejoin
    // 2/2 start pocket, stall-shudder equilibrium on flat ground, K-turn
    // + extensions + sag-lift all firing but 0.0-0.5 m/s),
    // [-1429.0,-7.2,-274.6] / [-1441.8,-7.1,-269.0] (probe_cadence:
    // same pocket, rejoin 2/2 restarts inside it and boxes at wp 1),
    // [-1598.6,4.2,-323.3] / [-1600.7,4.2,-299.6] (probe_ground: new
    // field past the old one at street level, rejoin 1-2/3 stops),
    // [-1318.3,-25.3,153.6] (probe_cap140: half-beached, 2 wheels in
    // contact, motor ~24 against locked wheels, rejoin 1-3/3 stops),
    // [-1305.3,-7.1,14.3] (probe_carry2: 3 wheels dangling, east
    // ground at taxi level — possible exit side).
    // Edges whose segment
    // passes within 45 m of a trap get a large additive penalty, so
    // Dijkstra prefers drivable streets; a pocket on the only link
    // still routes through it (finite penalty), which the probe then
    // reports as usual instead of failing the search.
    let traps: std::cell::RefCell<Vec<[f32; 3]>> = std::cell::RefCell::new(vec![
        [-1314.0, -7.0, -227.0],
        [-628.0, 4.4, 165.0],
        [-147.5, 13.6, -162.7],
        [-1265.7, -7.1, -426.9],
        [-1307.8, -6.6, -702.0],
        [-1305.8, -6.7, -714.0],
        [-1349.6, -6.3, -638.1],
        [-1431.6, -6.9, -248.9],
        [-1276.4, -7.5, 84.7],
        [-1257.6, -7.9, 71.5],
        [-1287.0, -6.7, -706.1],
        [-1298.0, -6.5, -718.8],
        [-1253.7, -7.7, 24.8],
        [-1237.8, -7.8, 40.3],
        [-1290.0, -6.7, -714.5],
        [-1280.7, -6.6, -704.8],
        [-1428.0, -6.8, -234.1],
        [-1438.8, -6.9, -229.2],
        [-1429.0, -7.2, -274.6],
        [-1441.8, -7.1, -269.0],
        [-1598.6, 4.2, -323.3],
        [-1600.7, 4.2, -299.6],
        [-1318.3, -25.3, 153.6],
        [-1305.3, -7.1, 14.3],
    ]);
    // Seeds are never evicted; learned pockets rotate in the slots above
    // the seed count so the ledger stays bounded while surviving new
    // pockets past the cap. The remove index is guarded: at cap with no
    // learned entries there is nothing to evict, so the ledger grows by
    // one and logs it instead of panicking.
    let seed_trap_count = traps.borrow().len();
    const TRAP_LEDGER_CAP: usize = 28;
    let penalized = |node: usize, out: &mut Vec<(usize, f32)>| {
        neighbors(node, out);
        let traps = traps.borrow();
        let Some(a) = node_pos(node) else {
            return;
        };
        for (other, weight) in out.iter_mut() {
            if let Some(p) = node_pos(*other) {
                *weight += lhr_engine::mission_play::trap_edge_penalty(a, p, &traps);
            }
        }
    };
    let mut dest_node = None;
    let mut dest_gap = f32::INFINITY;
    for i in 0..node_count {
        if let Some(p) = node_pos(i) {
            let gap = (p[0] - konec[0]).hypot(p[2] - konec[2]);
            if gap < dest_gap {
                dest_gap = gap;
                dest_node = Some(i);
            }
        }
    }
    let dest_node = dest_node.ok_or("No road node near detector_konec")?;
    let (konec_path, konec_graph_m) = lhr_engine::mission_play::road_graph_path(
        &node_pos, &neighbors, node_count, spawn, dest_node,
    )
    .ok_or("No road path from spawn to detector_konec")?;
    println!(
        "probe konec route: node {dest_node} at [{:.1}, {:.1}, {:.1}], {:.0} m by graph, {} waypoints",
        konec[0], konec[1], konec[2], konec_graph_m, konec_path.len()
    );
    // Driven leg: the konec-anchored waypoint list. The staged
    // destination is detector_konec itself (the real mission end); the
    // final waypoint is its nearest road node ~1 m away, well inside the
    // 12 m acceptance radius.
    let mut waypoints = Vec::with_capacity(konec_path.len());
    for index in &konec_path {
        let Some(p) = node_pos(*index) else {
            return Err(format!("Konec path node {index} lost its position"));
        };
        waypoints.push(p);
    }
    let waypoints = waypoints;
    // Diagnostic geometry: spawn pose, seeding direction and the first
    // waypoints, so a start-line box can be attributed (temporary).
    println!(
        "probe drive geometry: spawn [{:.1}, {:.1}, {:.1}] straight [{:.1}, {:.1}, {:.1}]",
        spawn[0], spawn[1], spawn[2], straight[0], straight[1], straight[2]
    );
    for (n, wp) in waypoints.iter().enumerate() {
        let prev = if n == 0 { spawn } else { waypoints[n - 1] };
        let leg = (wp[0] - prev[0]).hypot(wp[2] - prev[2]);
        println!(
            "probe drive: waypoint {}/{} at [{:.1}, {:.1}, {:.1}] leg={:.0} m",
            n + 1,
            waypoints.len(),
            wp[0],
            wp[1],
            wp[2],
            leg
        );
    }
    if let Some(last_wp) = waypoints.last() {
        println!(
            "probe drive: final waypoint {}/{} at [{:.1}, {:.1}, {:.1}]",
            waypoints.len(),
            waypoints.len(),
            last_wp[0],
            last_wp[1],
            last_wp[2]
        );
    }
    // The taxi-centered sweep above runs on walk-scene triangles, not the
    // driving-world collision the car actually hits, so an all-free
    // result does not clear car props (lamp posts, fences). It stays as
    // a record of that coverage gap.
    for k in 0..8 {
        let heading = k as f32 * std::f32::consts::FRAC_PI_4;
        let (rest, blocked) = ground.sweep_walls(
            &tris_of,
            spawn,
            [heading.sin() * 30.0, 0.0, heading.cos() * 30.0],
        );
        let travelled = (rest[0] - spawn[0]).hypot(rest[2] - spawn[2]);
        println!(
            "probe lot walls: heading {:>3}° travelled {travelled:5.1} m blocked={blocked}",
            (k as f32 * 45.0) as u32,
        );
    }
    // Reverse calibration already measured: 150 full-brake reverse ticks
    // move the taxi 6.5 m backward (speed ramping 1.3/3.6/6.2), so the
    // reverse gear path is proven and the K-turn unstick can trust it.
    // The calibration drive itself is reverted: it parked the taxi 6.5 m
    // north, closer to the z≈196 wedge, and pursuit starts clean from
    // the seeded pose instead.
    progress.boarded_taxi();
    if progress.stage() != Mise01Stage::DrivingToDestination {
        return Err("Boarding did not hand over to DrivingToDestination".into());
    }
    if !progress.stage_destination(konec, 12.0) {
        return Err("Konec destination refused to stage".into());
    }
    // Pursuit drive: steer toward the node along the rolling direction,
    // easing off in turns. Two straight ticks already ran, so seed from
    // there. Reverse uses the evidenced `set_gear(-1)` plus the native
    // Coupled brake-as-throttle mapping (negative throttle stays
    // rejected), so a boxed taxi backs out instead of pushing forever.
    let mut odometer = 0.0f32;
    let mut drive_ticks = 0u32;
    let mut stall_ticks = 0u32;
    let mut unstick = 0u32;
    // Pull-out extension rounds used (max 2 per K-turn, see rolling exit).
    let mut unstick_extend = 0u32;
    // Refresh the slip latch from live wheel diagnostics: any driven
    // wheel slipping right now means the open differential feeds a
    // dangling wheel. Runs at every K-turn trigger (not just in escape
    // pushes), so reverse never works from a stale latch — and with no
    // slip it leaves the latch clear, so reverse stays unbraked.
    let mut refresh_slip_latch = |taxi: &mut lhr_engine::car_runtime::GuardedCarRuntime,
                                  slip: &mut bool| {
        if let Ok(rows) = taxi.driving_wheel_diagnostics() {
            *slip = rows
                .iter()
                .any(|row| row[4] & 0x2 != 0 && row[4] & 0x80 != 0);
        }
    };
    let mut unstick_from = [0.0f32; 3];
    let mut boxed_windows = 0u32;
    // Steering side latched when a wedge triggers the unstick: steer
    // toward the target side (err>=0 means target left, which needs
    // negative steer). The pull-out phase arcs out toward it.
    let mut escape_steer = 0.0f32;
    let mut min_gap = f32::INFINITY;
    // Lot exit south: the taxi spawns facing south with the konec route
    // north behind it, and the parking spot wedges any standing 180°
    // turn (contacts=4 at once, ~1 m per 120 ticks). The south corridor
    // is proven open car territory (157 m green drive), so exit 40 m
    // south first; the U-turn onto the northbound route happens in the
    // open, where full-lock circling has room instead of a wall.
    let mut exit_pos = straight;
    let mut exit_ticks = 0u32;
    while spawn[2] - exit_pos[2] < 40.0 && exit_ticks < 625 {
        exit_pos = taxi
            .driving_step(
                16,
                lhr_engine::car_runtime::CarDrivingInput {
                    throttle: 1.0,
                    brake: 0.0,
                    steer: 0.0,
                    handbrake: false,
                    reverse: false,
                },
            )
            .map_err(|e| e.to_string())?
            .position;
        exit_ticks += 1;
    }
    println!(
        "probe lot exit: [{:.1}, {:.1}, {:.1}] after {exit_ticks} ticks",
        exit_pos[0], exit_pos[1], exit_pos[2]
    );
    if spawn[2] - exit_pos[2] < 10.0 {
        return Err("Lot exit south made no progress; the spawn faces blocking geometry".into());
    }
    // Rejoin from the exit: the street graph may join south of the pocket
    // without doubling back through the north corner that boxed the taxi
    // at z≈196. Dijkstra from the exit pose decides; if the only link
    // runs back through the corner, the driven list will say so.
    let (drive_path, drive_graph_m) = lhr_engine::mission_play::road_graph_path(
        &node_pos, &penalized, node_count, exit_pos, dest_node,
    )
    .ok_or("No road path from lot exit to detector_konec")?;
    let mut waypoints = Vec::with_capacity(drive_path.len());
    // Subdivide long straight legs so pursuit tracks the road instead of
    // cutting across blocks: the 107 m straight cut past wp18 left the
    // road and hung the taxi on street furniture, whose stall-drag
    // trapped the clutch in the native recompute arm (push samples:
    // 0x5dc 1.0->0.17-0.47 within 15 ticks, wheels exactly 0, moved
    // 0.0 m). Midpoints are collinear on straight roads (harmless) and
    // pull pursuit back onto curved roads between distant nodes.
    {
        let mut from = exit_pos;
        for index in &drive_path {
            let Some(p) = node_pos(*index) else {
                return Err(format!("Drive path node {index} lost its position"));
            };
            let leg = (p[0] - from[0]).hypot(p[2] - from[2]);
            let cuts = (leg / 25.0).floor() as usize;
            for c in 1..=cuts {
                let t = c as f32 / (cuts + 1) as f32;
                waypoints.push([
                    from[0] + (p[0] - from[0]) * t,
                    from[1] + (p[1] - from[1]) * t,
                    from[2] + (p[2] - from[2]) * t,
                ]);
            }
            waypoints.push(p);
            from = p;
        }
    }
    println!(
        "probe drive path: {} waypoints from lot exit, {:.0} m by penalized graph ({} known traps)",
        waypoints.len(),
        drive_graph_m,
        traps.borrow().len()
    );
    // Route divergence: plain vs penalized Dijkstra from the same exit
    // pose. Decides cut-vertex (identical corridors: the pocket is the
    // only link, penalty cannot help) vs penalty weakness (corridors
    // differ yet the taxi still wedges: the subdivided pursuit legs cut
    // back into the pocket between graph nodes).
    {
        let plain = lhr_engine::mission_play::road_graph_path(
            &node_pos, &neighbors, node_count, exit_pos, dest_node,
        )
        .ok_or("No plain road path from lot exit to detector_konec")?;
        let first_diverge = drive_path
            .iter()
            .zip(plain.0.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(drive_path.len().min(plain.0.len()));
        let mut plain_min = f32::INFINITY;
        let mut pen_min = f32::INFINITY;
        for index in plain.0.iter() {
            if let Some(p) = node_pos(*index) {
                for t in traps.borrow().iter() {
                    plain_min = plain_min.min((p[0] - t[0]).hypot(p[2] - t[2]));
                }
            }
        }
        for index in drive_path.iter() {
            if let Some(p) = node_pos(*index) {
                for t in traps.borrow().iter() {
                    pen_min = pen_min.min((p[0] - t[0]).hypot(p[2] - t[2]));
                }
            }
        }
        let div_info = drive_path
            .get(first_diverge)
            .zip(plain.0.get(first_diverge))
            .map(|(a, b)| {
                let pa = node_pos(*a).unwrap_or([f32::NAN; 3]);
                let pb = node_pos(*b).unwrap_or([f32::NAN; 3]);
                format!(
                    "pen node {a} [{:.1},{:.1},{:.1}] vs plain node {b} [{:.1},{:.1},{:.1}]",
                    pa[0], pa[1], pa[2], pb[0], pb[1], pb[2]
                )
            })
            .unwrap_or_else(|| "identical prefix (one path is a prefix of the other)".to_string());
        println!(
            "probe route divergence: plain {} nodes {:.0} m, penalized {} nodes {:.0} m, first diverge at leg {first_diverge}: {div_info}",
            plain.0.len(),
            plain.1,
            drive_path.len(),
            drive_graph_m,
        );
        println!(
            "probe route divergence: min node distance to a trap: plain {plain_min:.1} m, penalized {pen_min:.1} m ({})",
            if first_diverge >= drive_path.len().min(plain.0.len()) && drive_path.len() == plain.0.len() {
                "SAME corridor, cut-vertex suspect"
            } else {
                "DEFLECTED corridor"
            }
        );
    }
    for (n, wp) in waypoints.iter().enumerate().take(6) {
        println!(
            "probe drive: leg {}/{} at [{:.1}, {:.1}, {:.1}]",
            n + 1,
            waypoints.len(),
            wp[0],
            wp[1],
            wp[2]
        );
    }
    let mut prev = exit_pos;
    let mut last = lhr_engine::car_runtime::CarDrivingState::default();
    last.position = exit_pos;
    let mut waypoint = 0usize;
    let mut stuck_pos = exit_pos;
    // Bounded rejoins: a persistent wedge means the straight leg the
    // taxi is pushing is not drivable (prop row, uncut corner), so let
    // Dijkstra route around it from the boxed pose instead of failing.
    // The destination anchor never moves: completion still requires
    // arriving within 12 m of detector_konec itself. Budget 5:
    // probe_esc400 proved rejoins hop forward through fields instead of
    // looping (each rejoin starts in new terrain, persistence walks
    // 10 m+ per escape), but 3 hops ran out inside the [-1263,91]
    // terrace field; fail-fast survives via the tick cap plus the
    // 5-window box rule.
    let mut rejoins = 0u32;
    // Latched U-turn: when the target sits behind a moving taxi, the
    // per-tick err≈±π sign chatters and the taxi S-curves away instead of
    // circling. Latch full lock toward the target side until motion
    // aligns with it, then resume pursuit.
    let mut uturn: Option<f32> = None;
    // Rail-carry rising edge: per-tick low/high sweep ahead while
    // rolling; true only on the low-blocked/high-free signature, so
    // the 8 m/s hold cannot stick on anywhere else.
    let mut rail_carry_was = false;
    // Geometry-directed escape: when the taxi boxes, sweep the driving
    // collision in 8 headings and pursue a virtual target 40 m along the
    // freest one instead of K-turning blindly into the same walls. The
    // K-turn stays as the fallback for sealed pockets. Holds a virtual
    // target plus its expiry tick.
    let mut escape: Option<([f32; 3], u32)> = None;
    // Persisted escape heading: when an escape actually moves the taxi
    // (>= 2.5 m since it started), the next box keeps pushing that same
    // heading instead of rotating to a new one — rotating every window
    // walks the taxi in circles inside the pocket (probe_cap100 ground
    // out 60000 ticks at wp 140/189 this way). A heading that moved
    // nothing falls back to rotation; freedom (unboxed driving) clears
    // the latch.
    let mut escape_deg: Option<f32> = None;
    let mut escape_origin = [0.0f32; 3];
    // Clutch lift: coast ticks with both pedals released at the start of
    // an escape. A boxed taxi pushes with a disengaged clutch (0x5dc~0.06,
    // flags 0x2c); driving on would keep the copy-path trap pinned by
    // wedged_clutch_copy_path_self_sustains_under_full_throttle. Coasting
    // runs the native power<=threshold arm, which snaps 0x5dc to max and
    // requests Engaging (pinned by
    // wedged_clutch_throttle_lift_snaps_progress_to_max_and_requests_engage),
    // so the escape pursuit that follows pushes with a live clutch.
    // Keep the lift SHORT (3 ticks): the snap lands on the first coast
    // tick, while every further coast tick drags the motor down toward
    // the static wheels through the freshly engaged clutch. A 10-tick
    // lift decayed the motor 43->~20, below the torque knee, so the
    // first push tick recomputed the clutch straight back down (push
    // samples: 0x5dc 1.0->0.17-0.47 within 15 ticks, 0x5e8 0x20->0x24,
    // motor 14-33, wheels exactly 0, moved 0.0 m). Three ticks keep the
    // motor up in the torque band for the push.
    let mut lift = 0u32;
    // Sag-triggered re-lift clock: the escape-start lift snaps 0x5dc to
    // max, but sustained pushing against drag lets the native recompute
    // arm drag it back down (probe_seeds4 at [-1429,-557]: 0x5dc
    // 1.0->0.36, flags 0x20->0x24, 0.3-0.6 m per 70 ticks). Re-coasting
    // re-snaps it. Cooldown keeps the taxi mostly pushing.
    let mut last_sag_lift = 0u32;
    // Push-phase sampler: after the lift, dump the clutch words a few
    // ticks into the escape push while the speed is still ~0. The lift
    // snap is proven live, yet the taxi never moves; these samples tell
    // whether the clutch holds 1.0 through the push (geometric lock) or
    // disengages instantly (re-trap under load).
    let mut push_marks: Vec<u32> = Vec::new();
    let mut lift_end_pos = [0.0f32; 3];
    let mut lift_end_tick = 0u32;
    // Approach heading: last nonzero rolling direction, refreshed while
    // moving. Velocity steering needs motion to compute a correction, so
    // a boxed taxi would otherwise push straight ahead (steer identically
    // zero) instead of arcing toward the escape target. The escape
    // trigger latches full lock toward the target side from this
    // heading; velocity steering takes over as soon as the taxi rolls.
    let mut approach = [0.0f32, 1.0];
    let mut escape_lock = 1.0f32;
    // Latched wheel-slip state from the push sampler: any driven wheel
    // with 0x80 set while the taxi stands still means the open
    // differential feeds a dangling wheel. The escape push then holds
    // the handbrake (rear wheels only, gradual ramp) to brake the
    // spinner and send torque back to the gripping side — the native
    // traction response, as host choreography.
    let mut wheel_slip = false;
    while progress.stage() == Mise01Stage::DrivingToDestination {
        // Geometry-directed escape overrides the staged waypoint until it
        // expires or the taxi arrives within 3 m of the virtual target.
        if let Some((esc_target, until)) = escape {
            if drive_ticks > until {
                escape = None;
            } else {
                let esc_gap =
                    (esc_target[0] - last.position[0]).hypot(esc_target[2] - last.position[2]);
                if esc_gap <= 3.0 {
                    escape = None;
                    stuck_pos = last.position;
                }
            }
        }
        let mut target = waypoints[waypoint];
        let mut escaping = false;
        if let Some((esc_target, _)) = escape {
            target = esc_target;
            escaping = true;
        }
        let to_wp = [target[0] - last.position[0], target[2] - last.position[2]];
        let gap = to_wp[0].hypot(to_wp[1]);
        if !escaping {
            min_gap = min_gap.min(gap);
        }
        // Velocity steering: wheels follow the rolling direction, and
        // stay straight until the taxi rolls. Steering from a persistent
        // heading instead turned the taxi into the lot/lamp geometry
        // while still boxed, so this stays with the proven behavior.
        let vel = [last.position[0] - prev[0], last.position[2] - prev[2]];
        let speed_hint = vel[0].hypot(vel[1]);
        if speed_hint > 0.5 {
            approach = vel;
        }
        let mut steer = 0.0f32;
        let mut aligned = true;
        // Heading error to the waypoint, reused by steering and by the
        // corner-speed cap below. Zero while standing still.
        let mut err = 0.0f32;
        if speed_hint > 1e-4 {
            let cross = vel[0] * to_wp[1] - vel[1] * to_wp[0];
            let dot = vel[0] * to_wp[0] + vel[1] * to_wp[1];
            err = cross.atan2(dot);
            aligned = dot > 0.5 * speed_hint * gap;
            if uturn.is_none()
                && unstick == 0
                && !aligned
                && dot < -0.3 * speed_hint * gap
                && last.speed.abs() > 2.0
            {
                // Target behind: latch full lock toward its side. Cross>0
                // puts the target LEFT of motion, which needs negative
                // steer (positive steer turns RIGHT). Never latch mid
                // K-turn: the reverse phases legitimately move with the
                // target behind, and a fresh latch would zero `unstick`
                // below and eat the rest of the cycle (no phase sample
                // ever fired at [-1431.6,-248.9]).
                uturn = Some(if cross >= 0.0 { -1.0 } else { 1.0 });
                println!(
                    "probe drive: U-turn latched at [{:.1}, {:.1}, {:.1}] speed={:.1}",
                    last.position[0], last.position[1], last.position[2], last.speed
                );
            }
            if let Some(latch) = uturn {
                if aligned {
                    uturn = None;
                } else {
                    steer = latch;
                }
            }
            if uturn.is_none() && steer == 0.0 {
                // Negative gain: `err > 0` puts the target to the LEFT of
                // motion, but positive steer turns RIGHT (D key is +1), so
                // the correction must be negated. The unnegated form steered
                // away from the target and boxed the taxi in the west wall.
                steer = (-2.0 * err).clamp(-1.0, 1.0);
            }
        }
        // A latched U-turn overrides the K-turn: rotation in the open is
        // progress, never a wedge. If the taxi stops mid-turn, drop the
        // latch so the K-turn unstick can work free instead. While a
        // K-turn runs it owns pedals and steering outright: an override
        // here would zero `unstick` mid-cycle and strand the taxi in the
        // pocket it was rocking out of.
        if speed_hint <= 1e-4 {
            uturn = None;
        }
        if uturn.is_some() && unstick == 0 {
            steer = uturn.unwrap_or(0.0);
        }
        // Static escape steering: the velocity block above leaves steer
        // identically zero while standing still, so an escape push would
        // run straight ahead into whatever the nose faces. Hold the
        // latched full lock toward the escape target's side instead —
        // unless the approach heading already points at the target, in
        // which case pushing straight is correct and a full lock would
        // only grind the taxi into the slot walls. Velocity steering
        // resumes as soon as the taxi rolls.
        if escaping && speed_hint <= 1e-4 && unstick == 0 {
            let to_esc = [target[0] - last.position[0], target[2] - last.position[2]];
            let approach_len = approach[0].hypot(approach[1]);
            let esc_len = to_esc[0].hypot(to_esc[1]);
            let aligned = approach_len > 1e-4
                && esc_len > 1e-4
                && approach[0] * to_esc[0] + approach[1] * to_esc[1] > 0.7 * approach_len * esc_len;
            if !aligned {
                steer = escape_lock;
            }
        }
        // Adaptive speed: near stops and sharp corners take it slow so the
        // pursuit follows streets instead of ramming through them. The
        // 37 m/s blast that triggered a part-detach abort came from
        // flat-out throttle in an alley; cap by gap with a low ceiling.
        // A min()-form cap (3+11*align) proved too slow here: crawling
        // into the [41,108] node hung the taxi on street furniture it
        // previously carried through, so the cap stays multiplicative.
        let align = err.cos().clamp(0.2, 1.0);
        let target_speed = (gap * 0.5 * align).clamp(3.0, 14.0);
        let speed = last.speed.abs();
        // Rail carry: a low (+0.1 m) blockage with a free +0.4 m line
        // 30 m ahead is a rail/beam the pan would high-center on at
        // crawl speed — probe_pen6k beached exactly this way at
        // [-1318,153] (reverse 0.0 m, motor screaming) and then proved
        // the corridor a cut-vertex, so it must be crossed, not
        // avoided. Hold at least 8 m/s to carry over instead of slowing
        // onto it; the corner cap still governs everywhere else. No
        // speed gate: the gated version never fired once in probe_rail
        // (the crawl toward the rail runs below it), and the zero-length
        // direction check already skips standstill. Grounded-ahead
        // requirement: probe_rbrake proved the unguarded version floors
        // it straight through a guardrail lip and off the world edge
        // (y=-26 to y=-11000 free fall, carry still firing mid-air) —
        // a low rail at a cliff lip has the identical sweep signature
        // as a crossable rail. Require level ground 25 m out, same
        // rule as the escape-heading filter. Precedent: the [41,108]
        // node, previously carried through at speed but hung on
        // furniture when crawled into. Logged on rising edge only;
        // fail-safes untouched (a beach still drops speed to ~0 and the
        // box machinery takes over).
        {
            let mut carry = false;
            if !escaping && unstick == 0 {
                let len = vel[0].hypot(vel[1]);
                if len > 1e-4 {
                    let (dx, dz) = (vel[0] / len, vel[1] / len);
                    if let Some(world) = taxi.driving_collision() {
                        let mut travelled = |h: f32| {
                            let request = lhr_engine::collision_query::CollisionLineRequest {
                                origin: [
                                    last.position[0] + dx * 2.0,
                                    last.position[1] + h,
                                    last.position[2] + dz * 2.0,
                                ],
                                delta: [dx * 30.0, 0.0, dz * 30.0],
                                selector: -1,
                                reject_mask: 0,
                            };
                            let mut outputs = lhr_engine::collision_query::CollisionLineOutputs {
                                point: None,
                                normal: None,
                            };
                            let mut trav =
                                lhr_engine::collision_query::CollisionTraversalState::default();
                            match world.test_line_v(&request, &mut outputs, &mut trav) {
                                Ok(Some(hit)) => hit.fraction * 30.0,
                                Ok(None) => 30.0,
                                Err(_) => 0.0,
                            }
                        };
                        let low = travelled(0.1);
                        let high = travelled(0.4);
                        let mut grounded_ahead = false;
                        if low < 30.0 && high >= 29.0 {
                            let request = lhr_engine::collision_query::CollisionLineRequest {
                                origin: [
                                    last.position[0] + dx * 25.0,
                                    last.position[1] + 4.0,
                                    last.position[2] + dz * 25.0,
                                ],
                                delta: [0.0, -30.0, 0.0],
                                selector: -1,
                                reject_mask: 0,
                            };
                            let mut outputs = lhr_engine::collision_query::CollisionLineOutputs {
                                point: None,
                                normal: None,
                            };
                            let mut trav =
                                lhr_engine::collision_query::CollisionTraversalState::default();
                            if let Ok(Some(hit)) =
                                world.test_line_v(&request, &mut outputs, &mut trav)
                            {
                                let gy = last.position[1] + 4.0 - hit.fraction * 30.0;
                                grounded_ahead = gy >= last.position[1] - 3.0;
                            }
                        }
                        carry = low < 30.0 && high >= 29.0 && grounded_ahead;
                    }
                }
            }
            if carry && !rail_carry_was {
                println!(
                    "probe drive: rail carry at [{:.1}, {:.1}, {:.1}] speed={:.1}",
                    last.position[0], last.position[1], last.position[2], last.speed
                );
            }
            rail_carry_was = carry;
            if carry {
                // Hold speed over the rail; never brake into it.
            }
        }
        let target_speed = if rail_carry_was {
            target_speed.max(8.0)
        } else {
            target_speed
        };
        // Sag-triggered re-lift: while an escape push grinds at ~0 speed,
        // sample the live clutch progress every 12 ticks; below 0.95
        // (0x5dc < 0x3f733333) the native recompute arm has re-trapped it
        // under load, so coast 3 ticks to re-snap to max. Measured hold
        // (probe_sag2 traces at [-1429,-245]): snap to 1.0, droop below
        // 0.9 within ~10 ticks, decay to 0.15-0.5 — so the re-lift must
        // fire nearly every sample, not every 150 ticks (that cadence
        // left the clutch sagged ~90% of the grind and the taxi never
        // broke 0.5 m/s at [-1441,-269]). Bounded cost: 3 coast ticks
        // per re-lift. Never during the reverse K-turn (unstick owns
        // the pedals there) or an active lift coast.
        // Trajectory log: every 10th escape-push tick records the clutch
        // words, so the snap→decay curve is measurable per grind pocket
        // (hold duration decides the re-lift cadence).
        if escaping && unstick == 0 && lift == 0 && speed < 1.5 && drive_ticks % 10 == 0 {
            if let Ok(words) = taxi.driving_vehicle_words() {
                let mut line = format!(
                    "probe sag trace tick={drive_ticks} pos=[{:.1},{:.1},{:.1}] speed={:.1}:",
                    last.position[0], last.position[1], last.position[2], last.speed
                );
                for (offset, value) in words {
                    if matches!(offset, 0x548 | 0x5dc | 0x5e8 | 0x59c | 0x5cc | 0x540) {
                        match value {
                            Some(word) => line.push_str(&format!(" {offset:#x}={word:#x}")),
                            None => line.push_str(&format!(" {offset:#x}=?")),
                        }
                    }
                }
                println!("{line}");
            }
        }
        if escaping
            && unstick == 0
            && lift == 0
            && speed < 1.5
            && drive_ticks.wrapping_sub(last_sag_lift) >= 12
            && drive_ticks % 12 == 0
        {
            if let Ok(words) = taxi.driving_vehicle_words() {
                let mut progress_bits = 0x3f80_0000u32;
                for (offset, value) in words {
                    if offset == 0x5dc {
                        progress_bits = value.unwrap_or(0x3f80_0000);
                    }
                }
                if progress_bits < 0x3f73_3333 {
                    lift = 3;
                    last_sag_lift = drive_ticks;
                    // No wheel_slip reset here (unlike a fresh escape):
                    // the slip latch is what holds the brake-torque
                    // handbrake through the grind, and probe_p20 shows
                    // brake-torqued pushes moving 1-2.5 m at 2-4 m/s with
                    // the clutch pinned at 1.0 while unbraked pushes sit
                    // at 0.0-0.3 m. Clearing it on every re-lift flickers
                    // the brake off exactly when torque transfer works.
                    println!(
                        "probe drive: clutch sag lift at [{:.1}, {:.1}, {:.1}] 0x5dc={progress_bits:#x}",
                        last.position[0], last.position[1], last.position[2]
                    );
                }
            }
        }
        let (throttle, brake, handbrake, reverse) = if unstick > 0 {
            // K-turn in three phases, mirroring the proven reverse
            // calibration (150 full-brake straight ticks move 6.5 m):
            // back out STRAIGHT first at full brake (the entry line just
            // drove here, so it is known free), then rotate with full
            // lock once there is room behind, then pull OUT forward on an
            // arc toward the target side. The earlier 40-tick half-brake
            // straight phase never got rolling (~0.3 m per window at the
            // [-1314,-227] wedge), and ending the unstick at ~zero speed
            // resumes pursuit with steer=0 straight back into the prop.
            // Reverse uses native gear -1 with the brake as motive power.
            unstick -= 1;
            if unstick == 249 {
                unstick_from = last.position;
            }
            if unstick >= 100 {
                steer = 0.0;
                // Slip-gated handbrake in reverse too: the open-diff trap
                // is direction-blind (probe_aimed: right-rear dangles and
                // slips while the taxi sits at 0.0 m/s), and the refreshed
                // latch tracks live slip, so with no slip this is
                // identical to before (handbrake off).
                (0.0, 1.0, wheel_slip, true)
            } else if unstick >= 40 {
                steer = 1.0;
                (0.0, 1.0, wheel_slip, true)
            } else {
                steer = escape_steer;
                // Rolling exit: ending the fixed 40-tick pull-out at ~0
                // speed resumes pursuit straight back into the prop
                // (probe_p12 exit 1: pull-outs ended at 0.3-0.5 m/s, taxi
                // re-boxed at wp 1). Hold the pull-out up to two extra
                // 40-tick rounds until the taxi rolls at 3+ m/s, so
                // velocity steering takes over meaningfully. Bounded, so
                // a truly beached taxi still ends the K-turn.
                if unstick == 1 && speed < 3.0 && unstick_extend < 2 {
                    unstick = 39;
                    unstick_extend += 1;
                    println!(
                        "probe drive: pull-out extended ({unstick_extend}/2) at speed {speed:.1}"
                    );
                }
                (0.6, 0.0, false, false)
            }
        } else if lift > 0 {
            // Clutch lift coast: both pedals released. Steering is left
            // alone, so the escape lock (latched above) already pre-turns
            // the wheels during the coast. The unstick reverse above
            // keeps priority: it already coasts on the brake-as-throttle
            // mapping, so the lift only ever shapes forward driving.
            lift -= 1;
            if lift == 0 {
                println!(
                    "probe drive: clutch lift done, pushing toward [{:.1}, {:.1}, {:.1}]",
                    target[0], target[1], target[2]
                );
                // Decisive readout: did the native snap arm actually run
                // live? Compare 0x5dc/0x5e8 here against the boxed snapshot
                // (0x5dc~0.06, 0x5e8=0x2c): 0x5dc=1.0/0x5e8=0x21 means the
                // lift coast re-engaged, anything else names the live arm
                // that overrode it.
                match taxi.driving_vehicle_words() {
                    Ok(words) => {
                        let mut line = String::from("probe lift clutch:");
                        for (offset, value) in words {
                            if matches!(
                                offset,
                                0x55c
                                    | 0x3a4
                                    | 0x3a8
                                    | 0x5b8
                                    | 0x3cc
                                    | 0x548
                                    | 0x5e0
                                    | 0x5dc
                                    | 0x5e8
                                    | 0x540
                                    | 0x59c
                                    | 0x1a4
                                    | 0x5d0
                                    | 0x570
                                    | 0x574
                                    | 0x5ac
                                    | 0x4cc
                                    | 0x488
                            ) {
                                match value {
                                    Some(word) => line.push_str(&format!(" {offset:#x}={word:#x}")),
                                    None => line.push_str(&format!(" {offset:#x}=?")),
                                }
                            }
                        }
                        println!("{line}");
                    }
                    Err(error) => println!("probe lift clutch stopped: {error}"),
                }
                // Sample the push: clutch/gear/motor at +15/+40/+70 ticks
                // into the escape pursuit, plus motion since the lift.
                push_marks = vec![drive_ticks + 15, drive_ticks + 40, drive_ticks + 70];
                lift_end_pos = last.position;
                lift_end_tick = drive_ticks;
            }
            (0.0, 0.0, false, false)
        } else if escaping && last.speed.abs() < 1.5 {
            // Brake-torqued escape push with throttle feathering: steady
            // full throttle sustains the open-diff burnout equilibrium
            // (probe_brakefix: wheels turning in contact, clutch 1.0,
            // brake on or off, 0.0 m per push — the model never re-grips
            // under a steady pedal, same class as the pinned clutch trap).
            // Pulse 20 ticks full / 10 ticks 0.05: the low phase sits
            // below the 0x24 power threshold (~0.108), so it takes the
            // snap arm like a mini-lift while the taxi still rolls —
            // pinned by sagged_stall_low_throttle_boundary_follows_power_
            // threshold_arm (0.05/0.1 re-engage, 0.15+ grind). The old 0.3
            // low sat above the threshold and ground in the trap. The
            // handbrake stays slip-gated as before (a foot brake would
            // zero the power channels in SetBrake; the handbrake runs on
            // its own force channel, braking only the free spinner).
            let feathered = if drive_ticks % 30 < 20 {
                1.0 - 0.5 * steer.abs()
            } else {
                0.05
            };
            (feathered, 0.0, wheel_slip, false)
        } else if speed > target_speed + 2.0 {
            (0.0, 0.8, false, false)
        } else if speed > target_speed {
            (0.2, 0.0, false, false)
        } else {
            (1.0 - 0.5 * steer.abs(), 0.0, false, false)
        };
        // K-turn phase-boundary sample: does reverse engage the clutch
        // (Disengage->Engage cycle from the gear change) and does the
        // taxi move while backing/rotating/pulling out? Fires once per
        // boundary per K-turn. Values 100/40/1 only occur via the
        // decrement in the unstick arm above, so no extra gating needed.
        if unstick == 100 || unstick == 40 || unstick == 1 {
            let phase = if unstick == 100 {
                "reverse-straight done"
            } else if unstick == 40 {
                "reverse-rotate done"
            } else {
                "pull-out done"
            };
            match taxi.driving_vehicle_words() {
                Ok(words) => {
                    let k_moved = (last.position[0] - unstick_from[0])
                        .hypot(last.position[2] - unstick_from[2]);
                    let mut line = format!(
                        "probe kturn {phase}: moved {k_moved:.1} m pos=[{:.1},{:.1},{:.1}]:",
                        last.position[0], last.position[1], last.position[2]
                    );
                    for (offset, value) in words {
                        if matches!(
                            offset,
                            0x55c
                                | 0x560
                                | 0x3a4
                                | 0x548
                                | 0x5e0
                                | 0x5dc
                                | 0x5e8
                                | 0x540
                                | 0x59c
                                | 0x1a4
                        ) {
                            match value {
                                Some(word) => line.push_str(&format!(" {offset:#x}={word:#x}")),
                                None => line.push_str(&format!(" {offset:#x}=?")),
                            }
                        }
                    }
                    println!("{line}");
                }
                Err(error) => println!("probe kturn sample stopped: {error}"),
            }
        }
        prev = last.position;
        last = taxi
            .driving_step(
                16,
                lhr_engine::car_runtime::CarDrivingInput {
                    throttle,
                    brake,
                    steer,
                    handbrake,
                    reverse,
                },
            )
            .map_err(|e| e.to_string())?;
        odometer += last.speed.abs() * 0.016;
        progress.taxi_moved(last.position);
        progress.drive_distance(odometer);
        // Push-phase sample: clutch words plus motion since the lift end,
        // taken while the escape pursuit pushes at ~0 speed.
        if escaping {
            if let Some(at) = push_marks.iter().position(|&m| m == drive_ticks) {
                push_marks.remove(at);
                let pushed =
                    (last.position[0] - lift_end_pos[0]).hypot(last.position[2] - lift_end_pos[2]);
                match taxi.driving_vehicle_words() {
                    Ok(words) => {
                        let mut line = format!(
                            "probe push +{} ticks: moved {pushed:.1} m speed={:.1} pos=[{:.1},{:.1},{:.1}] brake_torque={}:",
                            drive_ticks.wrapping_sub(lift_end_tick),
                            last.speed,
                            last.position[0],
                            last.position[1],
                            last.position[2],
                            wheel_slip
                        );
                        for (offset, value) in words {
                            if matches!(
                                offset,
                                0x55c
                                    | 0x3a4
                                    | 0x3a8
                                    | 0x5b8
                                    | 0x3cc
                                    | 0x548
                                    | 0x5e0
                                    | 0x5dc
                                    | 0x5e8
                                    | 0x540
                                    | 0x59c
                                    | 0x1a4
                                    | 0x5d0
                                    | 0x570
                                    | 0x574
                                    | 0x5ac
                                    | 0x4cc
                                    | 0x488
                            ) {
                                match value {
                                    Some(word) => line.push_str(&format!(" {offset:#x}={word:#x}")),
                                    None => line.push_str(&format!(" {offset:#x}=?")),
                                }
                            }
                        }
                        println!("{line}");
                    }
                    Err(error) => println!("probe push sample stopped: {error}"),
                }
                // Wheel positions, live contact (0x8), force gating and
                // spin during the push: which wheels are driven (0x2),
                // touching right now (0x8), slipping (0x80); centers are
                // car-local while the stored contact point (+0x28) is in
                // world frame, so report touch-vs-taxi height (dy) instead
                // of mixing frames. A touch at ground level with all
                // contacts set yet zero motion plus changing spin means
                // the body hangs on a low rail the +0.4 m rays fly over.
                // Any slipping driven wheel latches the brake-torque
                // handbrake for the next pushes.
                match taxi.driving_wheel_diagnostics() {
                    Ok(rows) => {
                        // Refresh the slip latch from live data: brake only
                        // while a driven wheel is actually slipping now.
                        // probe_staged2 showed a permanent latch holding the
                        // handbrake on with clutch 1.0, locked wheels and
                        // 0.0-0.4 m per push after the slip was long gone.
                        wheel_slip = rows
                            .iter()
                            .any(|row| row[4] & 0x2 != 0 && row[4] & 0x80 != 0);
                        for (i, row) in rows.iter().enumerate() {
                            let pos = [
                                f32::from_bits(row[6]),
                                f32::from_bits(row[7]),
                                f32::from_bits(row[8]),
                            ];
                            let touched = [
                                f32::from_bits(row[10]),
                                f32::from_bits(row[11]),
                                f32::from_bits(row[12]),
                            ];
                            let flags = row[4];
                            println!(
                                "probe push wheel {i}: pos=[{:.1},{:.1},{:.1}] touch=[{:.1},{:.1},{:.1}] dy={:.2} spin={:.2} flags={flags:#x} driven={} contact={} slip={}",
                                pos[0],
                                pos[1],
                                pos[2],
                                touched[0],
                                touched[1],
                                touched[2],
                                touched[1] - last.position[1],
                                f32::from_bits(row[9]),
                                flags & 0x2 != 0,
                                flags & 0x8 != 0,
                                flags & 0x80 != 0,
                            );
                        }
                    }
                    Err(error) => println!("probe push wheels stopped: {error}"),
                }
            }
        }
        // Tight cornering: intermediate waypoints flip at 6 m, not 15 m.
        // The 15 m radius flipped to the westbound leg 9 m short of the
        // [241.8, 200.7] corner and drove the taxi into the wall segment
        // south of the street gap (wedge at z≈196). A 3 m trial flipped
        // too late instead: the taxi drove into the node furniture at
        // [41.2, 107.7] and wedged 5 m short of the node. 6 m stays: it
        // steers around intersection props before reaching them.
        // Waypoint flips stay with the staged route only; the escape leg
        // has its own 3 m arrival check above.
        if !escaping && gap <= 6.0 && waypoint + 1 < waypoints.len() {
            waypoint += 1;
            let heading_deg = vel[0].atan2(vel[1]);
            println!(
                "probe drive: waypoint {}/{} at [{:.1}, {:.1}, {:.1}] heading={:.0}° err={:.0}°",
                waypoint + 1,
                waypoints.len(),
                last.position[0],
                last.position[1],
                last.position[2],
                heading_deg.to_degrees(),
                err.to_degrees()
            );
        }
        drive_ticks += 1;
        if drive_ticks % 300 == 0 {
            println!(
                "probe drive trace tick={drive_ticks} pos=[{:.1},{:.1},{:.1}] speed={:.1} contacts={} gap={gap:.1}",
                last.position[0],
                last.position[1],
                last.position[2],
                last.speed,
                last.contacts
            );
        }
        if last.speed.abs() < 0.5 && unstick == 0 {
            stall_ticks += 1;
        } else {
            stall_ticks = 0;
        }
        // No real progress while driving means pushing into something the
        // speedometer cannot see. First sweep the driving collision in 8
        // headings and escape along the freest one nearest the target
        // bearing (a virtual 40 m target, pursued with the same velocity
        // steering); the blind K-turn stays only for sealed pockets. Negative throttle stays
        // out: Coupled routing rejects it. Gated on low speed so an
        // open-space U-turn circle (fast but positionally tight) is
        // never mistaken for a wedge. Five consecutive boxed windows
        // fail fast instead of burning the whole tick budget. The box
        // bar is crawl-aware (1.0 m, not 2.5 m): the feathered grind
        // moves 0.5-0.9 m per 70-tick push with the motor screaming
        // against dangling wheels (probe_rejoin3 at [-1483,-308]),
        // so ~1.5 m per 120-tick window. Calling that "boxed" burns a
        // rejoin every 5 windows for a taxi that is actually crawling
        // out; sub-meter windows still box and escalate as before.
        if drive_ticks % 120 == 119 {
            let moved = (last.position[0] - stuck_pos[0]).hypot(last.position[2] - stuck_pos[2]);
            if moved < 1.0 && last.speed.abs() < 1.5 && unstick == 0 && escape.is_none() {
                stall_ticks = 0;
                // Wedge census on the body collision the car actually
                // hits (the walk-scene sweep cannot see car props).
                // Escape along the free heading closest to the target
                // bearing: breaking ties by raw travel retreated NE,
                // away from the route, while west lay open toward it.
                let target_bearing =
                    (target[0] - last.position[0]).atan2(target[2] - last.position[2]);
                // The same target-side heading every window is a
                // deterministic loop (the taxi re-wedges identically), so
                // rotate through the free headings: sort by angular
                // distance to the target, then take the boxed-count-th
                // option. Every window tries a new direction; the K-turn
                // stays only for sealed pockets.
                let mut options: Vec<(f32, f32, f32)> = Vec::new();
                // Sweep at bumper height (+0.4 m), not +1 m: a +1 m ray
                // flies over curbs, medians and rubble that still beach
                // the body, while every wall the car cannot pass under
                // also reaches down to +0.4 m.
                let sweep_y = last.position[1] + 0.4;
                if let Some(world) = taxi.driving_collision() {
                    for k in 0..8 {
                        let heading = k as f32 * std::f32::consts::FRAC_PI_4;
                        let request = lhr_engine::collision_query::CollisionLineRequest {
                            origin: [last.position[0], sweep_y, last.position[2]],
                            delta: [heading.sin() * 30.0, 0.0, heading.cos() * 30.0],
                            selector: -1,
                            reject_mask: 0,
                        };
                        let mut outputs = lhr_engine::collision_query::CollisionLineOutputs {
                            point: None,
                            normal: None,
                        };
                        let mut trav =
                            lhr_engine::collision_query::CollisionTraversalState::default();
                        let travelled = match world.test_line_v(&request, &mut outputs, &mut trav) {
                            Ok(Some(hit)) => hit.fraction * 30.0,
                            Ok(None) => 30.0,
                            Err(error) => {
                                println!(
                                    "probe wedge walls: heading {:>3}° sweep stopped: {error:?}",
                                    (k as f32 * 45.0) as u32
                                );
                                0.0
                            }
                        };
                        println!(
                            "probe wedge walls: heading {:>3}° travelled {travelled:5.1} m",
                            (k as f32 * 45.0) as u32
                        );
                        if travelled >= 5.0 {
                            // Shortest-arc distance on the circle: the raw
                            // absolute difference can exceed 2π when the
                            // target bearing is negative, which previously
                            // wrapped to a negative turn and always won.
                            let two_pi = 2.0 * std::f32::consts::PI;
                            let mut turn = (heading - target_bearing).abs() % two_pi;
                            if turn > std::f32::consts::PI {
                                turn = two_pi - turn;
                            }
                            options.push((turn, k as f32 * 45.0, travelled));
                        }
                    }
                }
                // Low sweep at axle height (+0.1 m): the +0.4 m rays fly
                // over curbs, rails and rubble that still beach the floor
                // pan. A heading free at +0.4 m but blocked at +0.1 m names
                // the low prop. The escape choice keeps the +0.4 m rule so
                // this run stays comparable; the low map only attributes.
                if let Some(world) = taxi.driving_collision() {
                    for k in 0..8 {
                        let heading = k as f32 * std::f32::consts::FRAC_PI_4;
                        let request = lhr_engine::collision_query::CollisionLineRequest {
                            origin: [last.position[0], last.position[1] + 0.1, last.position[2]],
                            delta: [heading.sin() * 30.0, 0.0, heading.cos() * 30.0],
                            selector: -1,
                            reject_mask: 0,
                        };
                        let mut outputs = lhr_engine::collision_query::CollisionLineOutputs {
                            point: None,
                            normal: None,
                        };
                        let mut trav =
                            lhr_engine::collision_query::CollisionTraversalState::default();
                        let travelled = match world.test_line_v(&request, &mut outputs, &mut trav) {
                            Ok(Some(hit)) => hit.fraction * 30.0,
                            Ok(None) => 30.0,
                            Err(_) => 0.0,
                        };
                        println!(
                            "probe wedge low walls: heading {:>3}° travelled {travelled:5.1} m",
                            (k as f32 * 45.0) as u32
                        );
                    }
                }
                options.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                // Trap-aware escape: the waypoint graph avoids pockets,
                // but a 40 m escape target down a "free" heading can land
                // back inside one — the taxi ground itself into [-1276,84]
                // from escapes at wp 140/189. Prefer headings whose escape
                // target stays clear of the ledger; only when every free
                // heading ends near a trap (taxi boxed inside a penalty
                // disc) fall back to the nearest-target order.
                {
                    let traps = traps.borrow();
                    let clearance = |deg: f32| -> f32 {
                        let rad = deg.to_radians();
                        let tx = last.position[0] + rad.sin() * 40.0;
                        let tz = last.position[2] + rad.cos() * 40.0;
                        traps
                            .iter()
                            .map(|t| (tx - t[0]).hypot(tz - t[2]))
                            .fold(f32::INFINITY, f32::min)
                    };
                    let clear: Vec<(f32, f32, f32)> = options
                        .iter()
                        .copied()
                        .filter(|opt| clearance(opt.1) >= 25.0)
                        .collect();
                    if clear.is_empty() {
                        println!(
                            "probe drive: escape inside penalty disc, keeping all {} headings",
                            options.len()
                        );
                    } else if clear.len() < options.len() {
                        println!(
                            "probe drive: trap-aware escape keeps {}/{} headings",
                            clear.len(),
                            options.len()
                        );
                        options = clear;
                        options.sort_by(|a, b| {
                            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                        });
                    }
                    drop(traps);
                }
                // Ground-ahead check: a "free" heading whose 40 m target
                // has no ground (hole) or sits over a cliff drop leads
                // the taxi off the mapped world — probe_box10 ground
                // itself out past [-1614,-416] and fell to y=-10239.
                // Probe straight down from +4 m at each escape target;
                // require finite ground no deeper than 3 m below the
                // taxi. If no heading passes (taxi already off-map),
                // keep all and let the existing machinery work.
                if let Some(world) = taxi.driving_collision() {
                    let grounded: Vec<(f32, f32, f32)> = options
                        .iter()
                        .copied()
                        .filter(|opt| {
                            let rad = opt.1.to_radians();
                            let tx = last.position[0] + rad.sin() * 40.0;
                            let tz = last.position[2] + rad.cos() * 40.0;
                            let request = lhr_engine::collision_query::CollisionLineRequest {
                                origin: [tx, last.position[1] + 4.0, tz],
                                delta: [0.0, -30.0, 0.0],
                                selector: -1,
                                reject_mask: 0,
                            };
                            let mut outputs = lhr_engine::collision_query::CollisionLineOutputs {
                                point: None,
                                normal: None,
                            };
                            let mut trav =
                                lhr_engine::collision_query::CollisionTraversalState::default();
                            match world.test_line_v(&request, &mut outputs, &mut trav) {
                                Ok(Some(hit)) => {
                                    last.position[1] + 4.0 - hit.fraction * 30.0
                                        >= last.position[1] - 3.0
                                }
                                Ok(None) => false,
                                Err(_) => false,
                            }
                        })
                        .collect();
                    if grounded.is_empty() {
                        println!(
                            "probe drive: no escape target grounded, keeping all {} headings",
                            options.len()
                        );
                    } else if grounded.len() < options.len() {
                        println!(
                            "probe drive: ground-aware escape keeps {}/{} headings",
                            grounded.len(),
                            options.len()
                        );
                        options = grounded;
                        options.sort_by(|a, b| {
                            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                        });
                    }
                }
                // boxed_windows is still the pre-increment count here, so
                // the first box takes options[0] (closest to the target)
                // and later boxes rotate through the rest — unless the
                // previous escape moved the taxi, in which case keep
                // pushing its heading: rotation made the probe wander the
                // pocket instead of leaving it.
                let mut pick = options
                    .get(boxed_windows as usize % options.len().max(1))
                    .copied();
                if let Some(deg) = escape_deg {
                    let moved_since = (last.position[0] - escape_origin[0])
                        .hypot(last.position[2] - escape_origin[2]);
                    // Crawl-aware persistence: keep any heading that moves
                    // the taxi at all (>= 1.0 m). The feathered crawl yields
                    // 0.3-1.9 m per push (probe_temp), so the old 2.5 m bar
                    // never engaged and the probe rotated through headings
                    // instead of grinding one way out of the pocket.
                    if moved_since >= 1.0 {
                        if let Some(kept) = options.iter().find(|o| o.1 == deg).copied() {
                            println!(
                                "probe drive: keeping productive escape heading {deg:.0}° (moved {moved_since:.1} m)"
                            );
                            pick = Some(kept);
                        }
                    }
                }
                boxed_windows += 1;
                // First box only: name the neighborhood (scene2 objects
                // with world positions) and dump wheel state, so a prop
                // vs. pocket vs. beached car can be told apart. The taxi
                // barely moves in every mode here, which no wall layout
                // alone explains.
                if boxed_windows == 1 {
                    let mut near: Vec<(&str, f32)> = scene_objects
                        .iter()
                        .map(|(name, p)| {
                            (
                                name.as_str(),
                                (p[0] - last.position[0]).hypot(p[2] - last.position[2]),
                            )
                        })
                        .filter(|(_, d)| *d < 25.0)
                        .collect();
                    near.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    println!(
                        "probe wedge census: {} scene objects within 25 m",
                        near.len()
                    );
                    for (name, d) in near.iter().take(25) {
                        println!("probe wedge census: {name} d={d:.1}");
                    }
                    match taxi.driving_wheel_diagnostics() {
                        Ok(rows) => {
                            println!("probe wedge wheels: {rows:?}");
                            for (i, row) in rows.iter().enumerate() {
                                let pos = [
                                    f32::from_bits(row[6]),
                                    f32::from_bits(row[7]),
                                    f32::from_bits(row[8]),
                                ];
                                let touched = [
                                    f32::from_bits(row[10]),
                                    f32::from_bits(row[11]),
                                    f32::from_bits(row[12]),
                                ];
                                let flags = row[4];
                                println!(
                                    "probe wedge wheel {i}: pos=[{:.1},{:.1},{:.1}] touch=[{:.1},{:.1},{:.1}] dy={:.2} flags={flags:#x} driven={} contact={} slip={}",
                                    pos[0],
                                    pos[1],
                                    pos[2],
                                    touched[0],
                                    touched[1],
                                    touched[2],
                                    touched[1] - last.position[1],
                                    flags & 0x2 != 0,
                                    flags & 0x8 != 0,
                                    flags & 0x80 != 0,
                                );
                            }
                        }
                        Err(error) => println!("probe wedge wheels stopped: {error}"),
                    }
                    // Contact points: tell WHAT the taxi touches (ground
                    // under the wheels vs. a prop wedged against the body).
                    // Truncated: the ring holds up to 64 entries.
                    match taxi.driving_contact_queries() {
                        Ok(queries) => {
                            let mut short = queries;
                            short.truncate(1500);
                            println!("probe wedge contacts: {short}");
                        }
                        Err(error) => println!("probe wedge contacts stopped: {error}"),
                    }
                    // Staged intents: a grind that keeps staging crash-AI
                    // intents is stuck on knockable props (popel bins),
                    // not on static geometry — the knock-away has no
                    // owner yet so the props never clear.
                    match taxi.driving_staged_intents() {
                        Ok((lamps, parts, crash, occupants, materials, last_crash)) => {
                            println!("probe wedge staged: lamps={lamps} parts={parts} crash={crash} occupants={occupants} materials={materials} last_crash={last_crash:?}");
                        }
                        Err(error) => println!("probe wedge staged stopped: {error}"),
                    }
                    // Drivetrain words: tell a dead engine or refused
                    // gearbox apart from real geometry. A car that cannot
                    // move on flat open ground with full throttle is a
                    // control/state failure, not a wall.
                    match taxi.driving_vehicle_words() {
                        Ok(words) => {
                            let mut line = String::from("probe wedge drivetrain:");
                            for (offset, value) in words {
                                match value {
                                    Some(word) => line.push_str(&format!(" {offset:#x}={word:#x}")),
                                    None => line.push_str(&format!(" {offset:#x}=?")),
                                }
                            }
                            println!("{line}");
                        }
                        Err(error) => println!("probe wedge drivetrain stopped: {error}"),
                    }
                }
                // Heightmap census: downward rays from +4 m in a 3 m
                // cross around the taxi. The +4 m origin stays under the
                // metro deck (+4.7 over a taxi at -7.1): a +30 m origin
                // hits the deck first and reports it as ground. If the
                // street under the body reads at/above chassis level
                // while walls are near, the taxi is beached; if level
                // with only sides blocked, it is a pocket the escape or
                // K-turn can work out of.
                if let Some(world) = taxi.driving_collision() {
                    for (ox, oz, tag) in [
                        (0.0, 0.0, "c"),
                        (3.0, 0.0, "e"),
                        (-3.0, 0.0, "w"),
                        (0.0, 3.0, "n"),
                        (0.0, -3.0, "s"),
                    ] {
                        let request = lhr_engine::collision_query::CollisionLineRequest {
                            origin: [
                                last.position[0] + ox,
                                last.position[1] + 4.0,
                                last.position[2] + oz,
                            ],
                            delta: [0.0, -30.0, 0.0],
                            selector: -1,
                            reject_mask: 0,
                        };
                        let mut outputs = lhr_engine::collision_query::CollisionLineOutputs {
                            point: None,
                            normal: None,
                        };
                        let mut trav =
                            lhr_engine::collision_query::CollisionTraversalState::default();
                        let ground = match world.test_line_v(&request, &mut outputs, &mut trav) {
                            Ok(Some(hit)) => last.position[1] + 4.0 - hit.fraction * 30.0,
                            Ok(None) => f32::INFINITY,
                            Err(_) => f32::NAN,
                        };
                        println!(
                            "probe wedge ground {tag}: y={ground:.1} vs taxi y={:.1}",
                            last.position[1]
                        );
                    }
                }
                if let Some((_, pick_deg, pick_travel)) = pick {
                    let rad = pick_deg.to_radians();
                    let best_travel = pick_travel;
                    // Slot check: the center ray is thin, the taxi is ~2 m
                    // wide. Parallel rays at ±1 m lateral tell a narrow
                    // slot (sides blocked) from open road or a low
                    // beach obstacle (all three free while the car sits).
                    // Lateral rays use the same +0.4 m bumper height as
                    // the main sweep so low props are not missed.
                    let mut side = [0.0f32; 2];
                    if let Some(world) = taxi.driving_collision() {
                        for (i, s) in [-1.0f32, 1.0].iter().enumerate() {
                            let request = lhr_engine::collision_query::CollisionLineRequest {
                                origin: [
                                    last.position[0] + rad.cos() * s,
                                    last.position[1] + 0.4,
                                    last.position[2] - rad.sin() * s,
                                ],
                                delta: [rad.sin() * 30.0, 0.0, rad.cos() * 30.0],
                                selector: -1,
                                reject_mask: 0,
                            };
                            let mut outputs = lhr_engine::collision_query::CollisionLineOutputs {
                                point: None,
                                normal: None,
                            };
                            let mut trav =
                                lhr_engine::collision_query::CollisionTraversalState::default();
                            side[i] = match world.test_line_v(&request, &mut outputs, &mut trav) {
                                Ok(Some(hit)) => hit.fraction * 30.0,
                                Ok(None) => 30.0,
                                Err(_) => 0.0,
                            };
                        }
                    }
                    let esc_target = if gap < 60.0 {
                        // Aimed grind: the blocked waypoint itself is near
                        // (every observed rejoin loop stalls 9-40 m short
                        // of wp 1 while escapes wander off on radial 40 m
                        // virtual targets, then re-box at the same spot).
                        // Grind straight at the waypoint instead, so push,
                        // lift, sag re-lift and K-turn all work toward the
                        // path; arrival within 3 m still ends the escape
                        // early and the 6 m flip advances the route.
                        target
                    } else {
                        [
                            last.position[0] + rad.sin() * 40.0,
                            last.position[1],
                            last.position[2] + rad.cos() * 40.0,
                        ]
                    };
                    if boxed_windows == 3 {
                        // Forced rocking: two escapes with steer locked did
                        // not move the taxi (beached body, dangling drive
                        // wheels spinning freely while the taxi rests on a
                        // prop). Run one K-turn cycle instead of a third
                        // identical push: reverse out, rotate with full
                        // lock, pull out forward. Escape stays clear so
                        // the K-turn owns the recovery; the next window
                        // re-sweeps from the new pose. Drop the U-turn
                        // latch first: a latched U-turn zeroes `unstick`
                        // every tick below (rotation-in-the-open
                        // override), which silently eats the whole K-turn
                        // — the trigger message would print but no phase
                        // sample ever fires, as seen at [-1349.6,-638.1].
                        unstick = 250;
                        unstick_from = last.position;
                        uturn = None;
                        unstick_extend = 0;
                        refresh_slip_latch(&mut taxi, &mut wheel_slip);
                        escape_steer = if err >= 0.0 { -1.0 } else { 1.0 };
                        escape = None;
                        escape_deg = None;
                        lift = 0;
                        println!(
                            "probe drive: boxed {boxed_windows}x, escape stalled twice, K-turn rocking from [{:.1}, {:.1}, {:.1}] wp {}/{} gap={gap:.1}",
                            last.position[0],
                            last.position[1],
                            last.position[2],
                            waypoint + 1,
                            waypoints.len()
                        );
                    } else {
                        // Crawl-aware escape duration: 400 ticks, not 100.
                        // The feathered grind moves 0.3-1.9 m per push; 100
                        // ticks (~1.5 m) always re-boxed at the same streak
                        // count and 200 still burned a rejoin every 5
                        // windows in the [-1305,14] field (probe_p24: K-turn
                        // cycles move ~5 m but each escape only proved ~2
                        // m). 400 ticks lets one escape accumulate ~6 m and
                        // either clear the 1 m box bar outright or prove the
                        // heading for the persistence latch — the escape
                        // becomes the field-crossing locomotion and rejoins
                        // fire only on true dead stalls. Arrival within 3 m
                        // of the virtual target still ends it early.
                        escape = Some((esc_target, drive_ticks + 400));
                        // Latch this heading and origin for persistence:
                        // the next box keeps pushing here if it moved the
                        // taxi, instead of rotating to a new heading.
                        escape_deg = Some(pick_deg);
                        escape_origin = last.position;
                        // Lift first: snap the disengaged clutch back to max
                        // before the escape pursuit pushes. Three ticks: the
                        // snap lands on the first coast tick (second from an
                        // even motion flag), while longer coasts drag the
                        // motor below the torque knee through the engaged
                        // clutch and the push re-traps instantly.
                        lift = 3;
                        // Fresh slip assessment for this escape: the brake
                        // torque applies only once a driven wheel is actually
                        // seen slipping, never pre-emptively. The latch then
                        // persists across sag re-lifts within this escape
                        // (no reset at the sag site), so the brake does not
                        // flicker off mid-grind; a new escape re-assesses.
                        wheel_slip = false;
                        // Static steering latch: velocity steering is
                        // identically zero while standing still, so latch full
                        // lock toward the escape target's side from the
                        // approach heading. Positive steer turns right, so a
                        // target left of the approach (cross > 0) takes
                        // negative lock. Without this the push runs straight
                        // ahead (steer 0) into whatever the nose faces.
                        {
                            let to_esc = [
                                esc_target[0] - last.position[0],
                                esc_target[2] - last.position[2],
                            ];
                            let cross = approach[0] * to_esc[1] - approach[1] * to_esc[0];
                            escape_lock = if cross >= 0.0 { -1.0 } else { 1.0 };
                        }
                        println!(
                        "probe drive: boxed {boxed_windows}x, escaping toward [{:.1}, {:.1}, {:.1}] free {best_travel:.1} m at {pick_deg:.0}° slot=[{:.1},{:.1}]",
                        esc_target[0], esc_target[1], esc_target[2], side[0], side[1]
                    );
                    } // end escape (vs forced K-turn above)
                } else {
                    // Sealed pocket: no free heading at all. Same K-turn
                    // fallback, with the U-turn latch dropped for the same
                    // reason as the forced-rocking trigger above.
                    unstick = 250;
                    unstick_from = last.position;
                    uturn = None;
                    unstick_extend = 0;
                    refresh_slip_latch(&mut taxi, &mut wheel_slip);
                    escape = None;
                    escape_deg = None;
                    escape_steer = if err >= 0.0 { -1.0 } else { 1.0 };
                    println!(
                        "probe drive: no progress in 120 ticks (moved {moved:.1} m, boxed {boxed_windows}x), pocket sealed (no free heading), K-turn from [{:.1}, {:.1}, {:.1}] wp {}/{} gap={gap:.1}",
                        last.position[0],
                        last.position[1],
                        last.position[2],
                        waypoint + 1,
                        waypoints.len()
                    );
                }
                if boxed_windows >= 5 {
                    if rejoins < 5 {
                        rejoins += 1;
                        // Learn this pocket: penalize its nodes so the
                        // rejoin routes around the furniture instead of
                        // back into the same trap. Seeds are never evicted;
                        // learned pockets rotate once the ledger hits the
                        // cap, so the per-edge scan stays bounded while
                        // surviving new pockets.
                        let mut traps = traps.borrow_mut();
                        if traps.len() >= TRAP_LEDGER_CAP {
                            if traps.len() > seed_trap_count {
                                traps.remove(seed_trap_count);
                                println!(
                                    "probe drive: trap ledger rotated, still {} pockets",
                                    traps.len() + 1
                                );
                            } else {
                                println!(
                                    "probe drive: trap ledger full of seeds, growing to {} pockets",
                                    traps.len() + 1
                                );
                            }
                        }
                        traps.push(last.position);
                        println!(
                            "probe drive: trap ledger now {} pockets, latest at [{:.1}, {:.1}, {:.1}]",
                            traps.len(),
                            last.position[0],
                            last.position[1],
                            last.position[2]
                        );
                        drop(traps);
                        let (new_path, new_graph_m) = lhr_engine::mission_play::road_graph_path(
                            &node_pos,
                            &penalized,
                            node_count,
                            last.position,
                            dest_node,
                        )
                        .ok_or("Rejoin path from boxed pose to detector_konec lost")?;
                        waypoints.clear();
                        // Same leg subdivision as the initial drive path:
                        // rejoined routes inherit the same long straight
                        // cuts, so cut them the same way.
                        {
                            let mut from = last.position;
                            for index in &new_path {
                                let Some(p) = node_pos(*index) else {
                                    return Err(format!(
                                        "Rejoin path node {index} lost its position"
                                    ));
                                };
                                let leg = (p[0] - from[0]).hypot(p[2] - from[2]);
                                let cuts = (leg / 25.0).floor() as usize;
                                for c in 1..=cuts {
                                    let t = c as f32 / (cuts + 1) as f32;
                                    waypoints.push([
                                        from[0] + (p[0] - from[0]) * t,
                                        from[1] + (p[1] - from[1]) * t,
                                        from[2] + (p[2] - from[2]) * t,
                                    ]);
                                }
                                waypoints.push(p);
                                from = p;
                            }
                        }
                        waypoint = 0;
                        boxed_windows = 0;
                        unstick = 0;
                        uturn = None;
                        lift = 0;
                        last_sag_lift = drive_ticks;
                        escape_deg = None;
                        wheel_slip = false;
                        println!(
                            "probe drive: rejoin {rejoins}/5 from [{:.1}, {:.1}, {:.1}]: {} waypoints, {:.0} m by graph",
                            last.position[0], last.position[1], last.position[2],
                            waypoints.len(), new_graph_m
                        );
                        for (n, wp) in waypoints.iter().enumerate().take(6) {
                            println!(
                                "probe drive: releg {}/{} at [{:.1}, {:.1}, {:.1}]",
                                n + 1,
                                waypoints.len(),
                                wp[0],
                                wp[1],
                                wp[2]
                            );
                        }
                    } else {
                        return Err(format!(
                            "Taxi boxed at waypoint {}/{} gap {gap:.1} m after {boxed_windows} still windows",
                            waypoint + 1,
                            waypoints.len()
                        ));
                    }
                }
            } else if moved >= 2.5 && unstick == 0 && escape.is_none() {
                // Freedom reset only for unchoreographed motion: escape
                // pushes, lift coasts and K-turn cycles move the taxi
                // without freeing it (the wp-140 grind in probe_cap100
                // reset the counter every third window via K-turn meters
                // and never reached the 5-window rejoin, looping 60000
                // ticks). Only staged-route driving forgives a box streak,
                // and it also drops the persisted escape heading.
                boxed_windows = 0;
                escape_deg = None;
            }
            stuck_pos = last.position;
        }
        if stall_ticks > 150 {
            stall_ticks = 0;
            unstick = 250;
            unstick_extend = 0;
            refresh_slip_latch(&mut taxi, &mut wheel_slip);
            escape_steer = if err >= 0.0 { -1.0 } else { 1.0 };
            println!(
                "probe drive: stalled at waypoint {}/{} gap {gap:.1} m, reversing out",
                waypoint + 1,
                waypoints.len()
            );
        }
        // Tick budget: the deflected konec route is ~7.1 km by graph
        // plus escape/K-turn detours (probe_ground drove 6068.5 m in
        // 100000 ticks with zero aborts before the cap fired mid-drive
        // at wp 7/39, still moving at 4.9 m/s with closest gap 4.1 m).
        // 140000 keeps a fail-fast ceiling while fitting the
        // proven-healthy long drive; wedges still exit early via the
        // rejoin budget.
        if drive_ticks > 140000 {
            return Err(format!(
                "Taxi never reached detector_konec (waypoint {}/{} closest gap {min_gap:.1} m, odometer {odometer:.1} m)",
                waypoint + 1,
                waypoints.len()
            ));
        }
    }
    println!(
        "probe drive: reached [{:.1}, {:.1}, {:.1}] after {drive_ticks} ticks, odometer {odometer:.1} m, min gap {min_gap:.1} m; stage={:?}",
        last.position[0],
        last.position[1],
        last.position[2],
        progress.stage()
    );
    if progress.stage() != Mise01Stage::Complete {
        return Err("Destination arrival did not complete the mission".into());
    }
    println!(
        "probe completion: staged objectives {:?} authorize Complete",
        script_objectives
    );
    for line in mission_load_probe::mission_text_lines(root, &script_objectives, &script_subtitles)
    {
        println!("probe completion: {line}");
    }
    println!("MISSION PLAY PROBE COMPLETE: WatchIntro -> ReachTaxiStand -> BoardTaxi -> DrivingToDestination -> Complete");
    Ok(())
}
type DrivingWorldView = ([i32; 2], u64);

fn live_draw_state(
    car: &mut Option<lhr_engine::car_runtime::GuardedCarRuntime>,
    driving: bool,
    camera: &Camera,
    viewport: [u32; 2],
    snapshots: &mut lhr_engine::scene_render::SceneSnapshotSequence,
) -> Result<(usize, Option<DrivingWorldView>), String> {
    let Some(owner) = car.as_mut() else {
        return Ok((0, None));
    };
    let snapshot = driving_view::snapshot_scene(owner, camera, viewport, snapshots.next_number()?)?;
    let _ = driving;
    Ok((snapshot.scene.meshes.len(), None))
}
fn close_car(car: &mut Option<lhr_engine::car_runtime::GuardedCarRuntime>) -> Result<(), String> {
    if let Some(owner) = car.as_mut() {
        let cleanup = owner.close()?;
        println!(
            "CPU car/model/scene resources released; actor registrations {}, CPU heap blocks {}, visual buffers {}, textures {}, retained scene frames {}, driver scene registrations {}, driver scene capacity {}; native device discard unexecuted (CPU projection)",
            cleanup.actor_registry_entries,
            cleanup.heap_allocations,
            cleanup.graphics,
            cleanup.textures,
            cleanup.retained_scene_prefix_frames,
            cleanup.driver_scene_registrations,
            cleanup.driver_scene_capacity
        );
    }
    *car = None;
    Ok(())
}
#[allow(clippy::too_many_arguments)] // Existing driving inputs plus the actual startup owner.
fn load_car(
    installation: lhr_assets::Installation,
    path: &str,
    steer: Option<f32>,
    steer_steps: u32,
    driving: bool,
    viewport: [u32; 2],
    driver_time: u32,
    patches: &lhr_engine::program_patch::ProgramPatchStartup,
) -> Result<LoadedScene, String> {
    let runtime_options = lhr_engine::car_runtime::CarRuntimeOptions {
        material_driver_time: driver_time as i32,
        ..Default::default()
    };
    let opened = if driving {
        lhr_engine::car_runtime::GuardedCarRuntime::open_driving_scene_with_patches(
            installation,
            path,
            runtime_options,
            "mise01",
            &patches
                .lease()
                .map_err(|e| format!("Startup patch owner: {e:?}"))?,
        )
    } else {
        lhr_engine::car_runtime::GuardedCarRuntime::open(installation, path, runtime_options)
    };
    let mut owner = opened?;
    if let Some(frontier) = owner.scene2_frontier() {
        println!(
            "Canonical driving scene2 retained frontier: cursor {}, entry {:#x}: {}; mission initialization remains incomplete",
            frontier.cursor, frontier.entry, frontier.detail
        );
    }
    let mut snapshots = lhr_engine::scene_render::SceneSnapshotSequence::default();
    let result: Result<(Scene, Camera, lhr_render::SceneRenderFrame), String> = (|| {
        if driving {
            let _ = owner.start_driving("mise01")?;
        }
        if let Some(request) = steer {
            for _ in 0..steer_steps {
                owner.preview_wheel_steering(request, 0.016)?;
            }
        }
        let root = owner.model().ok_or("Car model absent")?.root();
        let camera = if driving {
            driving_view::chase_camera(owner.arena_mut(), root)?
        } else {
            lhr_engine::car_inspection::camera_from_arena_bounds(owner.arena_mut(), root)?
        };
        let snapshot =
            driving_view::snapshot_scene(&mut owner, &camera, viewport, snapshots.next_number()?)?;
        if driving {
            driving_view::sync_audio_listener(&mut owner, &camera)?;
        }
        println!(
            "Canonical car Scene: {} seats, {} cached animation tracks; {} selected material groups; actual active camera, source visibility and owned LOD buffers",
            owner.summary().seats,
            owner.summary().cached_tracks,
            snapshot.scene.meshes.len()
        );
        if driving {
            println!(
                "Driving controls: W throttle, S brake, A/D steer, Space handbrake; R reload, Escape exit. Fixed 16 ms simulation, same canonical car/world Scene; host chase controller authors its active camera. Mission scripts and traffic are not executed."
            );
        }
        Ok((snapshot.scene, camera, snapshot.frame))
    })();
    match result {
        Ok((scene, camera, scene_frame)) => Ok(LoadedScene {
            snapshots,
            canonical: None,
            scene_frame: Some(scene_frame),
            scene,
            camera,
            runtime: Default::default(),
            animation: None,
            car: Some(owner),
        }),
        Err(error) => {
            let cleanup = owner.close();
            Err(format!("{error}; explicit inspection cleanup: {cleanup:?}"))
        }
    }
}

fn authored_camera(
    installation: &lhr_assets::Installation,
    scene: &Scene,
    mission: &str,
    name: &str,
    runtime: &mut lhr_engine::runtime::RetainedScene,
) -> Result<Camera, String> {
    let mut camera = Camera::fit(scene)?;
    let bytes = installation
        .read(&format!("missions/{mission}/scene2.bin"))
        .map_err(|e| e.to_string())?;
    let metadata = lhr_assets::parse_scene2(&bytes).map_err(|e| e.to_string())?;
    println!(
        "Scene metadata: {} objects, {} actors, {} parsed script programs; actors and mission scripts are not executed",
        metadata.objects.len(),
        metadata.actors.len(),
        metadata.scripts.len()
    );
    if let Some(object) = metadata
        .objects
        .iter()
        .find(|o| o.name.eq_ignore_ascii_case(name))
    {
        if let (Some(p), Some(q)) = (object.position, object.rotation_wxyz) {
            let rotation = source_rotation(q)?;
            let forward = rotation * Vec3::Z;
            let up = rotation * Vec3::Y;
            camera.eye = Vec3::new(p[0], p[1], -p[2]);
            camera.target = camera.eye + Vec3::new(forward.x, forward.y, -forward.z);
            // I3D_camera::UpdateV uses the complete frame basis. Preserve the
            // authored roll through the same PC-to-host reflection as forward.
            // This host metadata view does not resolve original camera ownership.
            camera.up = Vec3::new(up.x, up.y, -up.z);
            println!("Authored camera: {name}; provisional 50-degree field of view");
        }
    } else {
        println!("Authored camera {name} unavailable; fitted inspection camera");
    }
    camera.validate()?;
    runtime.retain_metadata(metadata)?;
    Ok(camera)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppMode {
    Menu,
    IntroScene,
    FirstMissionScene,
    Inspection,
}
struct App {
    snapshots: lhr_engine::scene_render::SceneSnapshotSequence,
    canonical: Option<lhr_engine::scene_render::CanonicalSceneOwner>,
    startup_patches: lhr_engine::program_patch::ProgramPatchStartup,
    scene_frame: Option<lhr_render::SceneRenderFrame>,
    mode: AppMode,
    ui: Option<menu::MenuUi>,
    pointer: menu::Pointer,
    menu_shift: bool,
    adapter: Option<wgpu::Adapter>,
    scene: Scene,
    camera: Camera,
    runtime: lhr_engine::runtime::RetainedScene,
    animation: Option<AnimationPlayback>,
    car: Option<lhr_engine::car_runtime::GuardedCarRuntime>,
    runtime_engine: lhr_engine::runtime::RuntimeEngine,
    clock_backend: lhr_engine::runtime::PlatformClock,
    options: Options,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    renderer: Option<Renderer>,
    lab: Option<engine_lab::State>,
    native_lab: Option<engine_lab::NativeLab>,
    last_lab_redraw: std::time::Instant,
    lab_camera_home: lab_camera::Bookmark,
    lab_bookmark: Option<lab_camera::Bookmark>,
    lab_pivot: Vec3,
    lab_draw_labels: Vec<String>,
    lab_preview_seconds: f32,
    lab_turntable_degrees: f32,
    lab_delta_seconds: f32,
    lab_pointer: Option<[f64; 2]>,
    lab_drag: Option<MouseButton>,
    config: Option<wgpu::SurfaceConfiguration>,
    keys: driving_view::KeyboardInput,
    steering_preview_active: bool,
    driving_clock: driving_view::FixedClock,
    driving_controls: driving_view::Controls,
    walk_player: Option<lhr_engine::mission_play::OnFootPlayer>,
    walk_ground: lhr_engine::mission_play::MeshGroundSpace,
    roads: Option<lhr_engine::road_network::RoadNetwork>,
    dabing: lhr_engine::dabing::DabingOwner,
    dabing_audio: lhr_engine::mission_play::SilentDabingAudio,
    dabing_faces: lhr_engine::mission_play::FaceWeightLog,
    cutscene: lhr_engine::mission_play::CutscenePlayback,
    cutscene_show: lhr_engine::cutscene_owner::CutsceneShowStart,
    recorder: lhr_engine::cutscene_owner::CutsceneOwner,
    last_taxi_pos: [f32; 3],
    mise01: lhr_engine::mission_play::Mise01Progress,
    driving_taxi: bool,
    taxi_odometer_m: f32,
    last_drive_speed: f32,
    car_draws: usize,
    world_bucket: Option<DrivingWorldView>,
    audio_capture: Option<lhr_engine::runtime_audio::StereoWavCapture>,
    audio_output: Option<lhr_engine::runtime_audio::DeviceAudioOutput>,
    count: u32,
    error: Option<String>,
}
impl App {
    fn refresh_menu(&self) -> Result<(), String> {
        if let (Some(ui), Some(renderer)) = (&self.ui, &self.renderer) {
            renderer.update_texture(self.scene.meshes.len() - 1, &ui.pixels())?;
        }
        Ok(())
    }
    fn activate(&mut self, event_loop: &ActiveEventLoop) {
        if self.mode != AppMode::Menu {
            return;
        }
        let action = self.ui.as_ref().unwrap().activate();
        println!("Menu activate: {action:?}");
        match action {
            menu::Action::NewGame => {
                if let Err(error) = self.transition(AppMode::IntroScene) {
                    self.fail(event_loop, &error);
                }
            }
            menu::Action::Quit => event_loop.exit(),
            _ => println!("Selected original menu action is not implemented; menu retained"),
        }
    }
    fn transition(&mut self, mode: AppMode) -> Result<(), String> {
        let mut candidate = self.options.clone();
        // Returning from a Lab model/demo to the original menu must clear the
        // previous selection. Inspection reloads retain every selected option.
        if mode != AppMode::Inspection {
            candidate = lab_app::candidate_options(
                &candidate,
                &lab_state::SceneRequest::Mission("00menu".into()),
            )?;
            candidate.mission = match mode {
                AppMode::IntroScene => Some("intro".into()),
                AppMode::FirstMissionScene => Some("mise01".into()),
                _ => None,
            };
        }
        self.transition_with_options(mode, candidate)
    }
    fn transition_with_options(&mut self, mode: AppMode, candidate: Options) -> Result<(), String> {
        let viewport = self
            .config
            .as_ref()
            .map_or(INITIAL_CAR_VIEWPORT, |c| [c.width, c.height]);
        let LoadedScene {
            mut snapshots,
            canonical,
            scene_frame,
            mut scene,
            camera,
            mut runtime,
            mut animation,
            mut car,
        } = load(
            &candidate,
            viewport,
            self.runtime_engine.driver_time(),
            &self.startup_patches,
        )?;
        let (car_draws, world_bucket) = match live_draw_state(
            &mut car,
            candidate.car_driving,
            &camera,
            viewport,
            &mut snapshots,
        ) {
            Ok(draws) => draws,
            Err(error) => {
                let cleanup = close_car(&mut car);
                let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!(
                    "{error}; new car cleanup: {cleanup:?}; new material cleanup: {material_cleanup:?}"
                ));
            }
        };
        if mode == AppMode::Menu {
            scene
                .meshes
                .push(menu::quad(self.ui.as_ref().unwrap().pixels()));
        }
        // Stage the device, target format and lighting before touching active
        // owners. Unsupported source content leaves the current view usable.
        let prepared = (|| -> Result<_, String> {
            let adapter = self.adapter.as_ref().ok_or("Graphics adapter absent")?;
            let surface = self.surface.as_ref().ok_or("Presentation surface absent")?;
            let mut config = self
                .config
                .clone()
                .ok_or("Presentation configuration absent")?;
            let desired = if canonical.is_some() || car.is_some() {
                config.format.remove_srgb_suffix()
            } else {
                config.format.add_srgb_suffix()
            };
            if surface.get_capabilities(adapter).formats.contains(&desired) {
                config.format = desired;
            } else if canonical.is_some() || car.is_some() {
                return Err("Canonical Scene requires an unorm presentation target".into());
            }
            let mut renderer = pollster::block_on(Renderer::with_adapter(
                adapter,
                &scene,
                config.format,
                candidate.linear,
            ))?;
            if let Some(owner) = &canonical {
                owner.upload_lighting(&mut renderer)?;
            }
            renderer.set_inspection_alpha_cutout(candidate.car_inspection || candidate.car_driving);
            if let Some(lab) = &self.lab {
                renderer.set_lab_environment(
                    lab.settings.environment,
                    camera.eye.to_array(),
                    0.,
                )?;
            }
            self.clear_lab_audio_queue()?;
            Ok((renderer, config))
        })();
        let (renderer, config) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let cleanup = close_car(&mut car);
                let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!(
                    "{error}; car cleanup: {cleanup:?}; material cleanup: {material_cleanup:?}"
                ));
            }
        };
        if let Some(old) = &self.renderer {
            old.device.poll(wgpu::Maintain::Wait);
        }
        if let Err(error) = close_car(&mut self.car) {
            self.error = Some(error.clone());
            renderer.device.poll(wgpu::Maintain::Wait);
            let cleanup = close_car(&mut car);
            let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
            return Err(format!(
                "{error}; new car cleanup: {cleanup:?}; new material cleanup: {material_cleanup:?}"
            ));
        }
        if let Some(owner) = &mut self.canonical {
            if let Err(error) = owner.close() {
                self.error = Some(error.clone());
                let cleanup = close_car(&mut car);
                let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!(
                    "{error}; new car cleanup: {cleanup:?}; new material cleanup: {material_cleanup:?}"
                ));
            }
        }
        self.animation = None;
        if let Err(error) = self.runtime.close_owned_material_draws() {
            self.error = Some(error.clone());
            let cleanup = close_retained_materials(&mut animation, &mut runtime);
            return Err(format!("{error}; new material cleanup: {cleanup:?}"));
        }
        self.surface
            .as_ref()
            .unwrap()
            .configure(&renderer.device, &config);
        self.native_lab = self.lab.as_ref().map(|_| {
            let window = self.window.as_ref().unwrap();
            let format = config.format;
            match &self.native_lab {
                Some(previous) => previous.recreate(window, &renderer, format),
                None => engine_lab::NativeLab::new(window, &renderer, format),
            }
        });
        self.config = Some(config);
        self.renderer = Some(renderer);
        self.snapshots = snapshots;
        self.canonical = canonical;
        self.scene_frame = scene_frame;
        self.lab_camera_home = lab_camera::Bookmark::capture(&camera);
        self.lab_pivot = Camera::fit(&scene).map_or(camera.target, |fit| fit.target);
        self.lab_draw_labels = if candidate.synthetic {
            lab_app::synthetic_preset(&candidate)
                .map(|preset| preset.mesh_labels())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        self.lab_bookmark = None;
        self.lab_pointer = None;
        self.lab_drag = None;
        self.lab_preview_seconds = 0.;
        self.lab_turntable_degrees = 0.;
        self.lab_delta_seconds = 0.;
        self.scene = scene;
        self.camera = camera;
        self.runtime = runtime;
        self.animation = animation;
        self.car = car;
        self.car_draws = car_draws;
        self.world_bucket = world_bucket;
        self.options = candidate;
        if self.options.car_driving {
            if self.audio_output.is_none() {
                match lhr_engine::runtime_audio::DeviceAudioOutput::open_default(48_000) {
                    Ok(output) => self.audio_output = Some(output),
                    Err(error) => println!(
                        "Driving native audio unavailable: {error}; PCM mixing remains active"
                    ),
                }
            }
        } else {
            self.audio_output = None;
        }
        // Preserve the driver timeline used to admit the replacement owners;
        // only rebase the host sampling clock after a potentially slow load.
        self.runtime_engine.reset_clock(&mut self.clock_backend);
        self.mode = mode;
        self.keys.clear();
        self.steering_preview_active = false;
        self.driving_clock.reset();
        self.driving_controls = driving_view::Controls::default();
        if mode == AppMode::FirstMissionScene {
            self.spawn_mission_walk();
        } else {
            self.walk_player = None;
            self.walk_ground = lhr_engine::mission_play::MeshGroundSpace::default();
            self.roads = None;
            self.dabing = lhr_engine::dabing::DabingOwner::new();
            self.dabing_faces = lhr_engine::mission_play::FaceWeightLog::default();
            self.cutscene = lhr_engine::mission_play::CutscenePlayback::idle();
            self.cutscene_show = lhr_engine::cutscene_owner::CutsceneShowStart::default();
            self.recorder = lhr_engine::cutscene_owner::CutsceneOwner::constructed(
                lhr_engine::cutscene_owner::CutsceneConstructorSeed {
                    outer_allocator_byte: 0,
                    manager_address_high_byte: 0,
                },
            );
            self.last_taxi_pos = [0.0; 3];
            self.mise01 = lhr_engine::mission_play::Mise01Progress::fresh();
            self.driving_taxi = false;
            self.taxi_odometer_m = 0.0;
            self.last_drive_speed = 0.0;
        }
        if let Some(lab) = &mut self.lab {
            lab.scene_reloaded();
        }
        self.last_lab_redraw = std::time::Instant::now();
        if self.options.car_driving {
            println!(
                "Application state: {mode:?}; driving car and original static world loaded; mission scripts and traffic are not executed"
            );
        } else {
            println!(
                "Application state: {mode:?}; scene assets loaded; on-foot walk, host cutscene stand-in and objective tracking active in MISE01; mission scripts, traffic and car boarding unsupported"
            );
        }
        Ok(())
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, message: &str) {
        self.error = Some(message.into());
        event_loop.exit();
    }
    fn reload_car(&mut self) -> Result<(), String> {
        self.transition(AppMode::Inspection)
    }
    /// True while the on-foot mission slice owns WASD in the mission scene.
    fn walk_mode(&self) -> bool {
        self.mode == AppMode::FirstMissionScene && self.walk_player.is_some()
    }
    /// Refresh the window title with the current MISE01 objective.
    fn show_objective(&self) {
        if let Some(window) = &self.window {
            window.set_title(self.mise01.hud_line());
        }
    }
    /// Mirror of the `C_recorder::Close` dubbing hook (GAME 0x59c6a0,
    /// `game/mission/C_recorder.cpp:585`): reset faces, data and sounds
    /// through the real `DabingReset` callee.
    fn recorder_close_dabing(&mut self) {
        let (faces, audio) = (&mut self.dabing_faces, &self.dabing_audio);
        self.dabing.recorder_close_reset(faces, audio);
    }
    /// Spawn the on-foot player at the real MISE01 actor position after the
    /// scene loads (player/taxi from scene2 metadata; the camera-focus 25 m
    /// marker is only the degraded fallback). The drivable taxi is the real
    /// `models/taxi00.4ds` driving world for `mise01`; if it fails to open,
    /// walking and the cutscene stand-in still work and boarding reports it.
    /// Ground heights come from the loaded static scene geometry through
    /// `MeshGroundSpace`; wall sweeps stay deferred.
    fn spawn_mission_walk(&mut self) {
        let focus = self.camera.target;
        let mut forward = [focus.x - self.camera.eye.x, focus.z - self.camera.eye.z];
        let length = forward[0].hypot(forward[1]).max(1e-3);
        forward[0] /= length;
        forward[1] /= length;
        // Snapshot the static scene geometry once; later-merged car draws sit
        // past the covered prefix and are ignored by the probe.
        self.walk_ground =
            lhr_engine::mission_play::MeshGroundSpace::build(self.scene.meshes.len(), &|index| {
                self.scene.meshes.get(index).and_then(|mesh| {
                    let verts: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
                    lhr_engine::mission_play::mesh_aabb(&verts)
                })
            });
        // Real spawn positions from the MISE01 scene2 actor metadata. The
        // camera-focus fallback only runs when the metadata is absent.
        let (spawn_xz, stand_xz) = match self.options.root.as_deref() {
            Some(root) => match lhr_assets::Installation::open(root)
                .map_err(|e| e.to_string())
                .and_then(|installation| {
                    lhr_engine::mission_play::mission_actor_spawn(&installation, "mise01")
                }) {
                Ok((player, taxi)) => {
                    println!(
                        "MISE01 actors from scene2: player [{:.1}, {:.1}, {:.1}], taxi [{:.1}, {:.1}, {:.1}]",
                        player[0], player[1], player[2], taxi[0], taxi[1], taxi[2]
                    );
                    ([player[0], player[2]], [taxi[0], taxi[2]])
                }
                Err(error) => {
                    println!("MISE01 actor metadata unavailable ({error}); focus fallback");
                    (
                        [focus.x, focus.z],
                        [focus.x + forward[0] * 25.0, focus.z + forward[1] * 25.0],
                    )
                }
            },
            None => (
                [focus.x, focus.z],
                [focus.x + forward[0] * 25.0, focus.z + forward[1] * 25.0],
            ),
        };
        // The scene2 actor height can sit far below the static geometry, so
        // the spawn snap casts a high ray once; per-frame probes keep the
        // tight step-up window above the feet.
        let spawn_y = scene_ground_at(
            &self.walk_ground,
            &self.scene.meshes,
            spawn_xz[0],
            spawn_xz[1],
            focus.y + 100.0,
            200.0,
        )
        .or_else(|| {
            scene_ground_at(
                &self.walk_ground,
                &self.scene.meshes,
                spawn_xz[0],
                spawn_xz[1],
                focus.y + 2.0,
                10.0,
            )
        })
        .unwrap_or(focus.y);
        self.walk_player = Some(lhr_engine::mission_play::OnFootPlayer::spawn(
            [spawn_xz[0], spawn_y, spawn_xz[1]],
            forward[0].atan2(forward[1]),
        ));
        // Frame the real spawn.
        let to_player = Vec3::new(
            spawn_xz[0] - self.camera.target.x,
            0.0,
            spawn_xz[1] - self.camera.target.z,
        );
        self.camera.target += to_player;
        self.camera.eye += to_player;
        self.cutscene = lhr_engine::mission_play::CutscenePlayback::idle();
        self.cutscene_show = lhr_engine::cutscene_owner::CutsceneShowStart::default();
        self.dabing.dabing_init();
        self.dabing_faces = lhr_engine::mission_play::FaceWeightLog::default();
        self.recorder = lhr_engine::cutscene_owner::CutsceneOwner::constructed(
            lhr_engine::cutscene_owner::CutsceneConstructorSeed {
                outer_allocator_byte: 0,
                manager_address_high_byte: 0,
            },
        );
        self.mise01 = lhr_engine::mission_play::Mise01Progress::fresh();
        self.driving_taxi = false;
        self.taxi_odometer_m = 0.0;
        self.last_drive_speed = 0.0;
        self.last_taxi_pos = [0.0; 3];
        self.mise01
            .stage_taxi_stand([stand_xz[0], spawn_y, stand_xz[1]], 3.0);
        let taxi = self.options.root.as_ref().map(|root| {
            lhr_assets::Installation::open(root)
                .map_err(|e| e.to_string())
                .and_then(|installation| {
                    let lease = self
                        .startup_patches
                        .lease()
                        .map_err(|e| format!("Startup patch owner: {e:?}"))?;
                    // Real MISE01 road graph for the driving destination.
                    match installation
                        .read_optional("missions/mise01/road.bin")
                        .map_err(|e| e.to_string())?
                    {
                        Some(bytes) => match lhr_engine::road_network::parse_road_network(&bytes) {
                            Ok(network) => {
                                println!(
                                    "MISE01 roads: {} nodes loaded for the destination",
                                    network.nodes.len()
                                );
                                self.roads = Some(network);
                            }
                            Err(error) => {
                                println!("MISE01 roads parse stopped: {error:?}");
                            }
                        },
                        None => println!("MISE01 roads: road.bin absent"),
                    }
                    lhr_engine::car_runtime::GuardedCarRuntime::open_driving_scene_with_patches(
                        installation,
                        "models/taxi00.4ds",
                        lhr_engine::car_runtime::CarRuntimeOptions {
                            material_driver_time: self.runtime_engine.driver_time() as i32,
                            ..Default::default()
                        },
                        "mise01",
                        &lease,
                    )
                    .map_err(|e| e.to_string())
                })
                .and_then(|mut taxi| taxi.start_driving("mise01").map(|_| taxi).map_err(|e| e))
        });
        match taxi {
            Some(Ok(taxi)) => {
                self.car = Some(taxi);
                println!("MISE01 taxi staged: models/taxi00.4ds driving world ready");
            }
            Some(Err(error)) => {
                println!("MISE01 taxi unavailable: {error}; walking still works");
            }
            None => {}
        }
        println!(
            "MISE01 on foot: spawn [{:.1}, {:.1}, {:.1}]; stand [{:.1}, {:.1}, {:.1}]; {}",
            spawn_xz[0],
            spawn_y,
            spawn_xz[1],
            stand_xz[0],
            spawn_y,
            stand_xz[1],
            self.mise01.hud_line()
        );
        self.show_objective();
    }
    /// Advance walking, cutscene playback, taxi driving and objectives.
    fn walk_mission(&mut self, elapsed_ms: u32) {
        if !self.walk_mode() {
            return;
        }
        if self.driving_taxi {
            self.taxi_odometer_m += self.last_drive_speed.abs() * elapsed_ms.max(1) as f32 * 0.001;
            // Real destination: the road node nearest the mission end
            // detector (detector_konec from scene2 metadata). Falls back to
            // the odometer threshold when the graph or metadata is
            // unavailable.
            if self.mise01.destination().is_none() {
                let konec = self.options.root.as_deref().and_then(|root| {
                    lhr_assets::Installation::open(root)
                        .map_err(|e| e.to_string())
                        .and_then(|installation| {
                            lhr_engine::mission_play::mission_end_detector(&installation, "mise01")
                        })
                        .ok()
                });
                if let (Some(network), Some(konec)) = (&self.roads, konec) {
                    let mut best: Option<(usize, [f32; 3])> = None;
                    let mut best_gap = f32::INFINITY;
                    for (i, record) in network.nodes.iter().enumerate() {
                        let p = record.position();
                        let gap = (p[0] - konec[0]).hypot(p[2] - konec[2]);
                        if gap < best_gap {
                            best_gap = gap;
                            best = Some((i, p));
                        }
                    }
                    if let Some((node, pos)) = best {
                        if self.mise01.stage_destination(pos, 12.0) {
                            println!(
                                "Destination: detector_konec road node {node} at [{:.1}, {:.1}, {:.1}]; drive into the 12 m marker",
                                pos[0], pos[1], pos[2]
                            );
                            self.show_objective();
                        }
                    }
                }
            }
            let before = self.mise01.stage();
            self.mise01.taxi_moved(self.last_taxi_pos);
            self.mise01.drive_distance(self.taxi_odometer_m);
            if self.mise01.stage() != before {
                println!(
                    "Objective: {} (odometer {:.0} m)",
                    self.mise01.hud_line(),
                    self.taxi_odometer_m
                );
                self.show_objective();
            }
            return;
        }
        if self.recorder.recorder_is_playing() {
            // Host clock driver: the native play-ticks decrement point stays
            // unrecovered, so the staged word counts down here while the real
            // IsPlaying predicate holds the lock. Dubbing runs the real
            // DabingTick each frame like vfunc_49 does.
            let left = self.recorder.play_ticks().saturating_sub(elapsed_ms);
            self.recorder.stage_play_ticks(left);
            let finished = self.cutscene.advance(elapsed_ms);
            let (faces, audio) = (&mut self.dabing_faces, &self.dabing_audio);
            self.dabing.dabing_tick(audio, faces, elapsed_ms);
            // Slow cinematic pan while the cutscene holds the player.
            let offset = self.camera.eye - self.camera.target;
            let rotated = Quat::from_rotation_y(0.002) * offset;
            self.camera.eye = self.camera.target + rotated;
            if finished || left == 0 {
                self.recorder.stage_play_ticks(0);
                self.recorder.recorder_set_state(
                    lhr_engine::cutscene_owner::CUTSCENE_STOPPED,
                    &mut [],
                    &mut [],
                );
                self.recorder_close_dabing();
                self.cutscene_show.set_show_start(0);
                self.mise01.cutscene_done();
                println!(
                    "Cutscene finished (state {}); {}",
                    self.recorder.recorder_get_state(),
                    self.mise01.hud_line()
                );
                self.show_objective();
            }
            return;
        }
        let forward = u8::from(self.keys.contains(&KeyCode::KeyW)) as f32
            - u8::from(self.keys.contains(&KeyCode::KeyS)) as f32;
        let strafe = u8::from(self.keys.contains(&KeyCode::KeyD)) as f32
            - u8::from(self.keys.contains(&KeyCode::KeyA)) as f32;
        let run =
            self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        if forward != 0.0 || strafe != 0.0 {
            let offset = self.camera.eye - self.camera.target;
            let cam_yaw = (-offset.x).atan2(-offset.z);
            let before = self.mise01.stage();
            // Real-geometry ground under the feet and wall sweep against the
            // static scene; the void means a fall. One slide bounce; corners
            // stop the step.
            if let Some(player) = &mut self.walk_player {
                let ground_here = scene_ground_at(
                    &self.walk_ground,
                    &self.scene.meshes,
                    player.pos()[0],
                    player.pos()[2],
                    player.pos()[1] + 0.6,
                    4.0,
                );
                player.step(
                    elapsed_ms,
                    lhr_engine::mission_play::WalkInput {
                        forward,
                        strafe,
                        run,
                    },
                    cam_yaw,
                    ground_here,
                    |from, delta| {
                        let (rest, blocked) = self.walk_ground.sweep_walls(
                            &|index| scene_mesh_tris(&self.scene.meshes, index),
                            from,
                            delta,
                        );
                        blocked.then(|| lhr_engine::human_move_coll::StepContact {
                            newpos: rest,
                            material: 0,
                            owner_arm: false,
                        })
                    },
                );
                let pos = player.pos();
                // Hard camera follow on the ground plane; orbit kept.
                let delta = Vec3::new(
                    pos[0] - self.camera.target.x,
                    0.0,
                    pos[2] - self.camera.target.z,
                );
                self.camera.target += delta;
                self.camera.eye += delta;
                self.mise01.player_moved(pos);
            }
            if self.mise01.stage() != before {
                println!("Objective: {}", self.mise01.hud_line());
                self.show_objective();
            }
        }
    }
    fn drive_car(&mut self, elapsed_ms: u32) -> Result<(), String> {
        if !self.options.car_driving && !self.driving_taxi {
            return Ok(());
        }
        let old_ticks = self.driving_clock.ticks;
        let ticks = self.driving_clock.advance(elapsed_ms);
        if ticks == 0 {
            return Ok(());
        }
        let settings = self
            .lab
            .as_ref()
            .map_or_else(engine_lab::Settings::default, |lab| lab.settings);
        let viewport = self
            .config
            .as_ref()
            .map_or(INITIAL_CAR_VIEWPORT, |c| [c.width, c.height]);
        let owner = self.car.as_mut().ok_or("Driving car owner absent")?;
        let mut state = None;
        let mut steering_limit_at_input = None;
        let mut last_controls = None;
        for _ in 0..ticks {
            let keyboard = self.keys.take_driving_controls();
            let controls = self
                .lab
                .as_ref()
                .map_or(keyboard, |lab| lab.controls(keyboard));
            steering_limit_at_input =
                Some(owner.driving_control_feedback()?.steering_limit_radians);
            state = Some(owner.driving_step(
                driving_view::STEP_MS,
                lhr_engine::car_runtime::CarDrivingInput {
                    throttle: controls.throttle,
                    brake: controls.brake,
                    steer: controls.steer,
                    handbrake: controls.handbrake,
                    // Interactive driving stays forward-only; reverse gear
                    // is a probe choreography input for now.
                    reverse: false,
                },
            )?);
            last_controls = Some(controls);
            let root = owner.model().ok_or("Driving car model absent")?.root();
            self.camera =
                driving_view::chase_camera_with_settings(owner.arena_mut(), root, settings.chase)?;
            self.camera.vertical_fov_radians = settings.fov_degrees.to_radians();
            driving_view::control_scene_camera(owner, &self.camera, viewport)?;
            driving_view::sync_audio_listener(owner, &self.camera)?;
            let mut samples = [0.0_f32; 1536];
            owner.driving_mix_audio(48_000, &mut samples)?;
            if let Some(capture) = &mut self.audio_capture {
                capture.push(&samples).map_err(|e| e.to_string())?;
            }
            if let Some(output) = &mut self.audio_output {
                output.push(&samples).map_err(|e| e.to_string())?;
            }
        }
        let state = state.ok_or("Driving scheduler dispatched no tick")?;
        self.last_drive_speed = state.speed;
        self.last_taxi_pos = state.position;
        let controls = last_controls.ok_or("Driving scheduler dispatched no input")?;
        if self.audio_capture.is_some()
            || controls != self.driving_controls
            || old_ticks / 63 != self.driving_clock.ticks / 63
        {
            let audio_frames = self
                .audio_capture
                .as_ref()
                .map_or(0, |capture| capture.frames());
            let host_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis();
            let feedback = owner.driving_control_feedback()?;
            let steering_limit_at_input =
                steering_limit_at_input.ok_or("Driving scheduler dispatched no input feedback")?;
            println!(
                "Driving state: {{\"tick\":{},\"position\":{:?},\"speed\":{},\"contacts\":{},\"throttle\":{},\"brake\":{},\"steer\":{},\"handbrake\":{},\"audio_frames\":{},\"host_unix_ms\":{},\"actual_throttle\":{},\"actual_brake\":{},\"steering_requested_radians\":{},\"steering_applied_radians\":{},\"steering_limit_radians\":{},\"steering_limit_at_input_radians\":{},\"handbrake_force\":{},\"gear\":{}}}",
                self.driving_clock.ticks,
                state.position,
                state.speed,
                state.contacts,
                controls.throttle,
                controls.brake,
                controls.steer,
                controls.handbrake,
                audio_frames,
                host_unix_ms,
                feedback.throttle,
                feedback.brake,
                feedback.steering_requested_radians,
                feedback.steering_applied_radians,
                feedback.steering_limit_radians,
                steering_limit_at_input,
                feedback.handbrake_force,
                feedback.gear
            );
        }
        self.driving_controls = controls;
        if owner.has_canonical_scene_world() {
            return Ok(());
        }
        let renderer = self.renderer.as_mut().ok_or("Driving renderer absent")?;
        let bucket = driving_view::world_bucket(state.position)?;
        let revision = owner.driving_world_render_revision()?;
        if self.world_bucket != Some((bucket, revision)) {
            let world = owner.driving_render_world_near(state.position, 200.)?;
            renderer.replace_meshes(self.car_draws..self.scene.meshes.len(), &world.meshes)?;
            self.scene.meshes.truncate(self.car_draws);
            self.scene.meshes.extend(world.meshes);
            self.world_bucket = Some((bucket, owner.driving_world_render_revision()?));
            println!(
                "Driving world presentation refreshed at source bucket {bucket:?}; {} actual static draws",
                self.scene.meshes.len() - self.car_draws
            );
        }
        Ok(())
    }
    /// Refresh after current camera/orbit/chase/resize and pose updates, so
    /// selected owned levels and fade reach the actual draw list every redraw.
    fn refresh_car_presentation(&mut self) -> Result<(), String> {
        let viewport = self
            .config
            .as_ref()
            .map_or(INITIAL_CAR_VIEWPORT, |c| [c.width, c.height]);
        let Some(owner) = self.car.as_mut() else {
            return Ok(());
        };
        owner
            .arena_mut()
            .set_material_driver_time(self.runtime_engine.driver_time() as i32)
            .map_err(|error| error.to_string())?;
        let snapshot = driving_view::snapshot_scene(
            owner,
            &self.camera,
            viewport,
            self.snapshots.next_number()?,
        )?;
        let renderer = self.renderer.as_mut().ok_or("Car renderer absent")?;
        let mut count = self.scene.meshes.len();
        driving_view::refresh_car_draws(
            renderer,
            &mut self.scene,
            &mut count,
            snapshot.scene.meshes,
        )?;
        for (i, lighting) in snapshot.lighting.iter().enumerate() {
            if let Some(lighting) = lighting {
                renderer.set_mesh_lighting(i, lighting)?;
            } else {
                renderer.clear_mesh_lighting(i)?;
            }
        }
        self.car_draws = count;
        self.scene_frame = Some(snapshot.frame);
        Ok(())
    }
    fn preview_car_steering(&mut self, elapsed_ms: u32) -> Result<(), String> {
        if !self.steering_preview_active {
            return Ok(());
        }
        let owner = self.car.as_mut().ok_or("Wheel steering owner absent")?;
        let request = u8::from(self.keys.contains(&KeyCode::KeyD)) as f32
            - u8::from(self.keys.contains(&KeyCode::KeyA)) as f32;
        // The original C_car AI/Update callers cap elapsed time at 83 ms.
        owner.preview_wheel_steering(request, elapsed_ms.min(83) as f32 * 0.001)?;
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let Ok(window) = event_loop.create_window(
            Window::default_attributes()
                .with_title(if self.options.car_driving {
                    "Lost Heaven Reborn — original-data driving"
                } else if self.options.car_inspection {
                    "Lost Heaven Reborn — car inspection / wheel steering preview"
                } else if self.lab.is_some() {
                    "Lost Heaven Reborn — Engine Lab"
                } else {
                    "Lost Heaven Reborn — original menu / static scene inspection"
                })
                .with_inner_size(winit::dpi::LogicalSize::new(
                    if self.lab.is_some() {
                        1280
                    } else if self.mode == AppMode::Menu {
                        640
                    } else {
                        960
                    },
                    if self.lab.is_some() {
                        800
                    } else if self.mode == AppMode::Menu {
                        480
                    } else {
                        640
                    },
                )),
        ) else {
            self.fail(event_loop, "Cannot create inspection window");
            return;
        };
        let window = Arc::new(window);
        let instance = lhr_render::instance();
        let Ok(surface) = instance.create_surface(window.clone()) else {
            self.fail(event_loop, "Cannot create presentation surface");
            return;
        };
        let Some(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            }))
        else {
            self.fail(event_loop, "No presentation adapter");
            return;
        };
        let size = window.inner_size();
        let Some(mut config) =
            surface.get_default_config(&adapter, size.width.max(1), size.height.max(1))
        else {
            self.fail(event_loop, "No presentation configuration");
            return;
        };
        if self.canonical.is_some() || self.car.is_some() {
            let linear_format = config.format.remove_srgb_suffix();
            if !surface
                .get_capabilities(&adapter)
                .formats
                .contains(&linear_format)
            {
                self.fail(
                    event_loop,
                    "Canonical Scene requires an unorm presentation target",
                );
                return;
            }
            config.format = linear_format;
        }
        let mut renderer = match pollster::block_on(Renderer::with_adapter(
            &adapter,
            &self.scene,
            config.format,
            self.options.linear,
        )) {
            Ok(r) => r,
            Err(e) => {
                self.fail(event_loop, &e);
                return;
            }
        };
        if let Some(owner) = &self.canonical {
            if let Err(error) = owner.upload_lighting(&mut renderer) {
                self.fail(event_loop, &error);
                return;
            }
        }
        renderer
            .set_inspection_alpha_cutout(self.options.car_inspection || self.options.car_driving);
        surface.configure(&renderer.device, &config);
        self.native_lab = self
            .lab
            .as_ref()
            .map(|_| engine_lab::NativeLab::new(&window, &renderer, config.format));
        self.adapter = Some(adapter);
        self.window = Some(window);
        self.surface = Some(surface);
        self.renderer = Some(renderer);
        self.config = Some(config);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Err(error) = self.lab_window_event(&event) {
            self.fail(event_loop, &error);
            return;
        }
        // Always track the real location, including motion captured by the
        // Lab. Only admitted events below may change menu hover or activate it.
        match &event {
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer.observe([position.x as f32, position.y as f32]);
            }
            WindowEvent::CursorLeft { .. } => self.pointer.leave(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.menu_shift = modifiers.state().shift_key();
            }
            _ => {}
        }
        let consumed = match (&mut self.native_lab, &self.window, &self.lab) {
            (Some(native), Some(window), Some(lab)) => {
                native.on_window_event(window, &event, lab.visible)
            }
            _ => false,
        };
        if let Err(error) = self.lab_pointer_event(&event, consumed) {
            self.lab_status(format!("Camera motion rejected: {error}"));
        }
        // Releases must reach the gameplay key set even while a numeric UI
        // field owns keyboard focus; otherwise W/A/D can stay latched.
        if let WindowEvent::KeyboardInput { event, .. } = &event {
            if let PhysicalKey::Code(code) = event.physical_key {
                if event.state == ElementState::Released {
                    self.keys.remove(&code);
                }
                if self.lab.is_some()
                    && matches!(code, KeyCode::F1 | KeyCode::F2 | KeyCode::F3 | KeyCode::F4)
                {
                    return;
                }
            }
        }
        if consumed
            && matches!(
                &event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::CursorMoved { .. }
            )
        {
            if matches!(
                &event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        ..
                    }
            ) {
                self.keys.clear();
            }
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.menu_shift = false;
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if let Some(code) = application_key(
                    self.lab.is_some() || self.options.car_driving,
                    event.physical_key,
                    event.state,
                    is_synthetic,
                ) {
                    if event.state == ElementState::Pressed {
                        if self.mode == AppMode::Menu {
                            if let Some(navigation) = menu_navigation(code, self.menu_shift) {
                                if self.ui.as_mut().unwrap().navigate(navigation) {
                                    println!(
                                        "Menu focus: {}",
                                        self.ui.as_ref().unwrap().focus_id()
                                    );
                                    if let Err(error) = self.refresh_menu() {
                                        self.fail(event_loop, &error);
                                    }
                                }
                            } else if !event.repeat {
                                match code {
                                    KeyCode::Enter | KeyCode::NumpadEnter => {
                                        self.activate(event_loop)
                                    }
                                    KeyCode::Escape => event_loop.exit(),
                                    _ => {}
                                }
                            }
                        } else if code == KeyCode::KeyR
                            && !event.repeat
                            && (self.options.car_inspection || self.options.car_driving)
                        {
                            if let Err(error) = self.reload_car() {
                                if self.lab.is_some() && self.error.is_none() {
                                    self.lab_status(format!("Reload failed: {error}"));
                                } else {
                                    self.fail(event_loop, &error);
                                }
                            }
                        } else if self.walk_mode() && !event.repeat {
                            match code {
                                KeyCode::KeyC => {
                                    if self.recorder.recorder_is_playing() {
                                        self.recorder.stage_play_ticks(0);
                                        self.recorder.recorder_set_state(
                                            lhr_engine::cutscene_owner::CUTSCENE_STATE_0,
                                            &mut [],
                                            &mut [],
                                        );
                                        self.recorder_close_dabing();
                                        self.cutscene_show.set_show_start(0);
                                        self.cutscene =
                                            lhr_engine::mission_play::CutscenePlayback::idle();
                                        println!("Cutscene stopped");
                                    } else if self.mise01.stage()
                                        == lhr_engine::mission_play::Mise01Stage::WatchIntro
                                    {
                                        self.cutscene.start(8000);
                                        // Real recorder binding: the native state
                                        // machine and show-start global stage the
                                        // playback; the recorded-actor Tick and the
                                        // play-ticks decrement point stay deferred,
                                        // so the host counts the staged word down.
                                        // Dubbing runs the real DabingTick on a
                                        // staged subtitle channel; vfunc_49 calls
                                        // DabingTick(time) the same way.
                                        self.recorder.recorder_set_state(
                                            lhr_engine::cutscene_owner::CUTSCENE_STATE_1,
                                            &mut [],
                                            &mut [],
                                        );
                                        self.recorder.stage_play_ticks(8000);
                                        self.cutscene_show.set_show_start(1);
                                        self.dabing.dabing_init();
                                        if self.dabing.begin_subtitle_only(0, 48).is_err() {
                                            println!("Cutscene subtitle channel unavailable");
                                        }
                                        println!("Intro cutscene playing (8 s host stand-in)");
                                    } else {
                                        println!("No cutscene staged at this objective");
                                    }
                                }
                                KeyCode::Enter | KeyCode::NumpadEnter => {
                                    self.recorder.stage_play_ticks(0);
                                    self.recorder.recorder_set_state(
                                        lhr_engine::cutscene_owner::CUTSCENE_STATE_0,
                                        &mut [],
                                        &mut [],
                                    );
                                    self.recorder_close_dabing();
                                    self.cutscene_show.set_show_start(0);
                                    self.cutscene =
                                        lhr_engine::mission_play::CutscenePlayback::idle();
                                    self.mise01.cutscene_done();
                                    println!("Intro skipped; {}", self.mise01.hud_line());
                                    self.show_objective();
                                }
                                KeyCode::KeyE => {
                                    if self.mise01.stage()
                                        == lhr_engine::mission_play::Mise01Stage::BoardTaxi
                                    {
                                        if self.car.is_some() {
                                            self.mise01.boarded_taxi();
                                            self.driving_taxi = true;
                                            self.taxi_odometer_m = 0.0;
                                            println!(
                                                "Boarded the taxi; {}",
                                                self.mise01.hud_line()
                                            );
                                            self.show_objective();
                                        } else {
                                            println!("Taxi failed to stage; walking still works");
                                        }
                                    } else {
                                        println!(
                                            "No taxi to board here; {}",
                                            self.mise01.hud_line()
                                        );
                                    }
                                }
                                _ => {}
                            }
                        } else if code == KeyCode::Escape && !event.repeat {
                            let next = if self.mode == AppMode::IntroScene {
                                AppMode::FirstMissionScene
                            } else {
                                AppMode::Menu
                            };
                            if self.ui.is_some() {
                                if let Err(error) = self.transition(next) {
                                    self.fail(event_loop, &error);
                                }
                            } else {
                                event_loop.exit();
                            }
                        }
                    }
                    if self.mode != AppMode::Menu {
                        if self.options.car_inspection
                            && matches!(code, KeyCode::KeyA | KeyCode::KeyD)
                            && event.state == ElementState::Pressed
                        {
                            self.steering_preview_active = true;
                        }
                        if event.state == ElementState::Pressed {
                            self.keys.press(code, event.repeat);
                        } else {
                            self.keys.remove(&code);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { .. } if self.mode == AppMode::Menu => {
                let size = self.window.as_ref().unwrap().inner_size();
                if self
                    .pointer
                    .hover(self.ui.as_mut().unwrap(), [size.width, size.height])
                {
                    println!(
                        "Menu pointer focus: {}",
                        self.ui.as_ref().unwrap().focus_id()
                    );
                    if let Err(error) = self.refresh_menu() {
                        self.fail(event_loop, &error);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if self.mode == AppMode::Menu => {
                let size = self.window.as_ref().unwrap().inner_size();
                if self
                    .pointer
                    .select_for_click(self.ui.as_mut().unwrap(), [size.width, size.height])
                {
                    self.activate(event_loop);
                }
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let (Some(config), Some(surface), Some(renderer)) =
                        (&mut self.config, &self.surface, &self.renderer)
                    {
                        config.width = size.width;
                        config.height = size.height;
                        surface.configure(&renderer.device, config);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.window.as_ref().is_some_and(|w| {
                    let size = w.inner_size();
                    size.width == 0 || size.height == 0
                }) {
                    return;
                }
                let mut lab_frame = None;
                if self.lab.is_some() {
                    let diagnostics = match self.lab_diagnostics() {
                        Ok(diagnostics) => diagnostics,
                        Err(error) => {
                            self.fail(event_loop, &error);
                            return;
                        }
                    };
                    let (commands, frame) = self.native_lab.as_mut().unwrap().run(
                        self.window.as_ref().unwrap(),
                        self.lab.as_ref().unwrap(),
                        &diagnostics,
                    );
                    let was_paused = self.lab.as_ref().unwrap().playback.paused();
                    if let Err(error) = self.lab.as_mut().unwrap().apply(&commands) {
                        self.fail(event_loop, &error);
                        return;
                    }
                    let paused = self.lab.as_ref().unwrap().playback.paused();
                    if (paused && !was_paused) || commands.step {
                        if let Err(error) = self.clear_lab_audio_queue() {
                            self.fail(event_loop, &error);
                            return;
                        }
                    }
                    if was_paused != paused {
                        self.keys.clear();
                        println!(
                            "Engine Lab: paused={paused}; driver_time_ms={}; driving_ticks={}",
                            self.runtime_engine.driver_time(),
                            self.driving_clock.ticks
                        );
                    }
                    match self.apply_lab_actions(&commands) {
                        // The new device has a new egui context/font atlas.
                        // Never submit old-device jobs after publication.
                        Ok(true) => return,
                        Ok(false) => {}
                        Err(error) if self.error.is_some() => {
                            self.fail(event_loop, &error);
                            return;
                        }
                        Err(error) => self.lab_status(format!("Action failed: {error}")),
                    }
                    if self
                        .native_lab
                        .as_ref()
                        .unwrap()
                        .wants_keyboard_input(self.lab.as_ref().unwrap().visible)
                    {
                        self.keys.clear();
                    }
                    lab_frame = Some(frame);
                }
                let runtime_available = self.lab_runtime_available();
                let step = match advance_lab_frame(
                    &mut self.runtime_engine,
                    &mut self.runtime,
                    &mut self.animation,
                    &mut self.clock_backend,
                    self.lab.as_mut().map(|lab| &mut lab.playback),
                    runtime_available,
                ) {
                    Ok(step) => step,
                    Err(error) => {
                        self.fail(event_loop, &format!("Bound runtime failed: {error:?}"));
                        return;
                    }
                };
                if let Err(error) = self.advance_lab_preview(step.as_ref()) {
                    self.lab_status(format!("Preview motion rejected: {error}"));
                }
                if let Some(step) = step {
                    if self.count == 0 && self.car.is_none() {
                        println!(
                            "Bound runtime: raw delta {}, modified {}, driver time {}, {} retained model ticks dispatched; authored actors/scripts remain unbound",
                            step.raw_delta,
                            step.modified_delta,
                            step.driver_time,
                            step.models_dispatched
                        );
                    }
                    if let Err(error) = self.drive_car(step.raw_delta) {
                        self.fail(event_loop, &error);
                        return;
                    }
                    if let Err(error) = self.preview_car_steering(step.modified_delta) {
                        self.fail(event_loop, &error);
                        return;
                    }
                    self.walk_mission(step.raw_delta);
                    if let Some(lab) = &mut self.lab {
                        lab.completed_frames += 1;
                        if lab.playback.paused() {
                            println!(
                                "Engine Lab: single step {} ms; driver_time_ms={}; driving_ticks={}",
                                step.raw_delta, step.driver_time, self.driving_clock.ticks
                            );
                        }
                    }
                }

                if let Some(renderer) = &self.renderer {
                    if let Err(error) =
                        upload_bound(&mut self.runtime, &mut self.animation, renderer)
                    {
                        self.fail(event_loop, &error);
                        return;
                    }
                }
                let offset = self.camera.eye - self.camera.target;
                let mut yaw = 0.0;
                let mut pitch = 0.0;
                let mut zoom = 1.0;
                let motion_scale = self.lab.as_ref().map_or(1., |lab| {
                    self.lab_delta_seconds * 60. * lab.settings.camera_speed
                });
                let mut pan = [0.; 2];
                for key in self.keys.held_keys() {
                    match key {
                        KeyCode::ArrowLeft if !self.options.car_driving => {
                            yaw += 0.025 * motion_scale
                        }
                        KeyCode::ArrowRight if !self.options.car_driving => {
                            yaw -= 0.025 * motion_scale
                        }
                        KeyCode::ArrowUp if !self.options.car_driving => {
                            pitch += 0.025 * motion_scale
                        }
                        KeyCode::ArrowDown if !self.options.car_driving => {
                            pitch -= 0.025 * motion_scale
                        }
                        KeyCode::KeyW if !self.options.car_driving && !self.walk_mode() => {
                            zoom *= 0.98_f32.powf(motion_scale)
                        }
                        KeyCode::KeyS if !self.options.car_driving && !self.walk_mode() => {
                            zoom *= 1.02_f32.powf(motion_scale)
                        }
                        KeyCode::KeyR if self.walk_mode() => zoom *= 0.98_f32.powf(motion_scale),
                        KeyCode::KeyF if self.walk_mode() => zoom *= 1.02_f32.powf(motion_scale),
                        KeyCode::KeyJ if self.lab.is_some() && !self.options.car_driving => {
                            pan[0] -= 0.01 * motion_scale
                        }
                        KeyCode::KeyL if self.lab.is_some() && !self.options.car_driving => {
                            pan[0] += 0.01 * motion_scale
                        }
                        KeyCode::KeyI if self.lab.is_some() && !self.options.car_driving => {
                            pan[1] += 0.01 * motion_scale
                        }
                        KeyCode::KeyK if self.lab.is_some() && !self.options.car_driving => {
                            pan[1] -= 0.01 * motion_scale
                        }
                        _ => {}
                    }
                }
                let rotated = Quat::from_rotation_y(yaw)
                    * Quat::from_axis_angle(offset.cross(Vec3::Y).normalize_or_zero(), pitch)
                    * offset
                    * zoom;
                if self.lab.is_some() {
                    if let Err(error) = lab_camera::orbit(&mut self.camera, yaw, pitch, zoom)
                        .and_then(|_| lab_camera::pan(&mut self.camera, pan[0], pan[1]))
                    {
                        self.lab_status(format!("Camera motion rejected: {error}"));
                    }
                } else if rotated.normalize().dot(Vec3::Y).abs() < 0.995
                    && rotated.length() > self.camera.near * 2.0
                    && rotated.length() < self.camera.far * 0.8
                {
                    self.camera.eye = self.camera.target + rotated;
                }
                if let Err(error) = self.apply_lab_camera() {
                    self.fail(event_loop, &error);
                    return;
                }
                if let Some(owner) = &mut self.canonical {
                    let viewport = self
                        .config
                        .as_ref()
                        .map_or(INITIAL_CAR_VIEWPORT, |c| [c.width, c.height]);
                    let result = (|| -> Result<(), String> {
                        owner
                            .arena
                            .set_material_driver_time(self.runtime_engine.driver_time() as i32)
                            .map_err(|error| error.to_string())?;
                        owner.set_inspection_camera_pose(&self.camera)?;
                        if self.lab.is_some() {
                            owner.set_inspection_camera_fov(&self.camera, viewport)?;
                        }
                        let snapshot = owner.snapshot(viewport, self.snapshots.next_number()?)?;
                        let renderer = self
                            .renderer
                            .as_mut()
                            .ok_or("Canonical Scene renderer absent")?;
                        let count = self
                            .scene
                            .meshes
                            .len()
                            .saturating_sub(usize::from(self.mode == AppMode::Menu));
                        let mut count = count;
                        driving_view::refresh_car_draws(
                            renderer,
                            &mut self.scene,
                            &mut count,
                            snapshot.scene.meshes,
                        )?;
                        owner.upload_lighting(renderer)?;
                        self.scene_frame = Some(snapshot.frame);
                        Ok(())
                    })();
                    if let Err(error) = result {
                        self.fail(event_loop, &error);
                        return;
                    }
                }
                if let Err(error) = self.refresh_car_presentation() {
                    self.fail(event_loop, &error);
                    return;
                }
                if let Some(renderer) = &mut self.renderer {
                    if let Err(error) = refresh_owned_material_draws(
                        &mut self.runtime,
                        &mut self.scene,
                        renderer,
                        self.runtime_engine.driver_time(),
                    ) {
                        self.fail(event_loop, &error);
                        return;
                    }
                }
                if let Err(error) = self.apply_lab_presentation() {
                    self.fail(event_loop, &error);
                    return;
                }
                if let (Some(native), Some(renderer), Some(ui)) =
                    (&mut self.native_lab, &self.renderer, &lab_frame)
                {
                    native.prepare_textures(renderer, ui);
                }
                if let (Some(surface), Some(renderer), Some(config)) =
                    (&self.surface, &self.renderer, &self.config)
                {
                    match surface.get_current_texture() {
                        Ok(frame) => {
                            let view = frame.texture.create_view(&Default::default());
                            let mut encoder = if let Some(scene_frame) = &self.scene_frame {
                                let result = if self.mode == AppMode::Menu {
                                    renderer.render_scene_frame_with_overlay(
                                        &view,
                                        config.width,
                                        config.height,
                                        scene_frame,
                                        menu::DESIGN,
                                    )
                                } else {
                                    renderer.render_scene_frame(
                                        &view,
                                        config.width,
                                        config.height,
                                        scene_frame,
                                    )
                                };
                                match result {
                                    Ok(encoder) => encoder,
                                    Err(error) => {
                                        self.fail(event_loop, &error);
                                        return;
                                    }
                                }
                            } else if self.mode == AppMode::Menu {
                                renderer.render_with_overlay(
                                    &view,
                                    config.width,
                                    config.height,
                                    &self.camera,
                                    menu::DESIGN,
                                )
                            } else {
                                renderer.render(&view, config.width, config.height, &self.camera)
                            };
                            let callbacks = match (&mut self.native_lab, &lab_frame) {
                                (Some(native), Some(ui)) => {
                                    native.paint(renderer, &mut encoder, &view, ui)
                                }
                                _ => Vec::new(),
                            };
                            renderer.queue.submit(
                                callbacks
                                    .into_iter()
                                    .chain(std::iter::once(encoder.finish())),
                            );
                            frame.present();
                            if let (Some(native), Some(_)) = (&mut self.native_lab, &lab_frame) {
                                native.frame_presented();
                            }
                            self.count += 1;
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            surface.configure(&renderer.device, config)
                        }
                        Err(wgpu::SurfaceError::Timeout) => {}
                        Err(_) => {
                            self.fail(event_loop, "Presentation failed");
                            return;
                        }
                    }
                }
                if let (Some(native), Some(ui)) = (&mut self.native_lab, lab_frame) {
                    native.finish(self.renderer.as_ref().unwrap(), ui);
                }
                if self.options.frames > 0 && self.count >= self.options.frames {
                    if let Some(path) = &self.options.capture {
                        let renderer = self.renderer.as_ref().unwrap();
                        let size = self.window.as_ref().unwrap().inner_size();
                        let result = if let Some(frame) = &self.scene_frame {
                            if self.mode == AppMode::Menu {
                                renderer.capture_scene_frame_with_overlay(
                                    size.width,
                                    size.height,
                                    frame,
                                    menu::DESIGN,
                                    path,
                                )
                            } else {
                                renderer.capture_scene_frame(size.width, size.height, frame, path)
                            }
                        } else if self.mode == AppMode::Menu {
                            renderer.capture_with_overlay(
                                size.width,
                                size.height,
                                &self.camera,
                                menu::DESIGN,
                                path,
                            )
                        } else {
                            renderer.capture(size.width, size.height, &self.camera, path)
                        };
                        if result.is_err() {
                            self.fail(event_loop, "Capture failed");
                            return;
                        }
                    }
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
/// A paused Lab redraw performs no runtime dispatch. Single-step uses the
/// existing bound runtime with an explicit 16 ms host clock and then restores
/// the platform baseline, so wall time spent paused cannot become a catch-up.
fn advance_lab_frame<B: lhr_engine::clock::ClockBackend + ?Sized>(
    engine: &mut lhr_engine::runtime::RuntimeEngine,
    graph: &mut lhr_engine::runtime::RetainedScene,
    animation: &mut Option<AnimationPlayback>,
    clock: &mut B,
    playback: Option<&mut lab_playback::Playback>,
    runtime_available: bool,
) -> Result<Option<lhr_engine::runtime::RuntimeFrame>, lhr_engine::runtime::RuntimeError> {
    if playback.is_some() && !runtime_available {
        // Keep the platform baseline current without dispatching an empty
        // synthetic "simulation". A later real callback starts without a gap.
        engine.reset_clock(clock);
        return Ok(None);
    }
    let plan = playback.map_or(
        lab_playback::FramePlan {
            advance: lab_playback::Advance::PlatformClock,
            reset_platform_clock: false,
        },
        lab_playback::Playback::take_frame_plan,
    );
    if plan.reset_platform_clock {
        engine.reset_clock(clock);
    }
    match plan.advance {
        lab_playback::Advance::Paused => Ok(None),
        lab_playback::Advance::PlatformClock => {
            advance_bound(engine, graph, animation, clock).map(Some)
        }
        lab_playback::Advance::SingleStep => {
            let mut controlled = lab_playback::SingleStepClock::default();
            engine.reset_clock(&mut controlled);
            let result = advance_bound(engine, graph, animation, &mut controlled);
            engine.reset_clock(clock);
            result.map(Some)
        }
    }
}

#[cfg(test)]
mod lab_runtime_tests {
    use super::*;

    #[test]
    fn lab_keyboard_route_blocks_synthetic_legacy_actions_and_keeps_releases() {
        for code in [
            KeyCode::KeyR,
            KeyCode::Enter,
            KeyCode::NumpadEnter,
            KeyCode::Escape,
            KeyCode::ArrowDown,
            KeyCode::KeyW,
            KeyCode::F2,
        ] {
            let key = PhysicalKey::Code(code);
            assert_eq!(
                application_key(true, key, ElementState::Pressed, true),
                None
            );
            assert_eq!(
                application_key(true, key, ElementState::Pressed, false),
                Some(code)
            );
            assert_eq!(
                application_key(true, key, ElementState::Released, true),
                Some(code)
            );
            assert_eq!(
                application_key(false, key, ElementState::Pressed, true),
                Some(code)
            );
        }
    }

    #[derive(Default)]
    struct Clock {
        now: u32,
        samples: u32,
        slept: u32,
    }
    impl lhr_engine::clock::ClockBackend for Clock {
        fn sample_ms(&mut self) -> u32 {
            self.samples += 1;
            self.now
        }
        fn sleep_ms(&mut self, milliseconds: u32) {
            self.slept += milliseconds;
            self.now = self.now.wrapping_add(milliseconds);
        }
    }

    fn graph() -> lhr_engine::runtime::RetainedScene {
        let mut graph = lhr_engine::runtime::RetainedScene::default();
        let source = Arc::new(lhr_assets::Model {
            materials: Vec::new(),
            frames: vec![lhr_assets::Frame {
                kind: 7,
                visual: None,
                render_flags: 0,
                logic_flags: 0,
                culling: 0,
                name: "lab_clock_fixture".into(),
                name_bytes: b"lab_clock_fixture".to_vec(),
                properties: Vec::new(),
                dummy_bounds: None,
                light: None,
                joint: None,
                skin: None,
                morph: None,
                target: None,
                occluder: None,
                sector: None,
                lensflare: None,
                parent: None,
                position: [0.; 3],
                scale: [1.; 3],
                rotation_wxyz: [1., 0., 0., 0.],
                instance: None,
                lods: Vec::new(),
            }],
        });
        graph
            .retain(
                "lab_clock_fixture".into(),
                source,
                glam::Mat4::IDENTITY,
                None,
                Vec::new(),
            )
            .unwrap();
        graph
    }

    #[test]
    fn lab_pause_and_single_step_reach_the_actual_driver_once() {
        let mut engine = lhr_engine::runtime::RuntimeEngine::from_sample(0);
        let mut graph = graph();
        let mut animation = None;
        let mut clock = Clock::default();
        let mut playback = lab_playback::Playback::default();
        playback.set_paused(true);
        for _ in 0..80 {
            clock.now += 1000;
            assert!(advance_lab_frame(
                &mut engine,
                &mut graph,
                &mut animation,
                &mut clock,
                Some(&mut playback),
                true
            )
            .unwrap()
            .is_none());
        }
        assert_eq!(engine.driver_time(), 0);
        assert_eq!(clock.samples, 1);
        assert_eq!(clock.slept, 0);
        playback.request_step();
        playback.request_step();
        let frame = advance_lab_frame(
            &mut engine,
            &mut graph,
            &mut animation,
            &mut clock,
            Some(&mut playback),
            true,
        )
        .unwrap()
        .unwrap();
        assert_eq!(frame.raw_delta, 16);
        assert_eq!(frame.modified_delta, 16);
        assert_eq!(frame.driver_time, 16);
        assert_eq!(frame.models_dispatched, 1);
        assert!(playback.paused());
        assert!(advance_lab_frame(
            &mut engine,
            &mut graph,
            &mut animation,
            &mut clock,
            Some(&mut playback),
            true
        )
        .unwrap()
        .is_none());
        assert_eq!(engine.driver_time(), 16);
    }

    #[test]
    fn lab_resume_drops_paused_wall_time_and_default_path_keeps_platform_timing() {
        let mut engine = lhr_engine::runtime::RuntimeEngine::from_sample(0);
        let mut graph = graph();
        let mut animation = None;
        let mut clock = Clock {
            now: 50,
            ..Default::default()
        };
        let normal = advance_lab_frame(
            &mut engine,
            &mut graph,
            &mut animation,
            &mut clock,
            None,
            true,
        )
        .unwrap()
        .unwrap();
        assert_eq!(normal.raw_delta, 50);
        let mut playback = lab_playback::Playback::default();
        playback.set_paused(true);
        assert!(advance_lab_frame(
            &mut engine,
            &mut graph,
            &mut animation,
            &mut clock,
            Some(&mut playback),
            true
        )
        .unwrap()
        .is_none());
        clock.now = 50_000;
        playback.set_paused(false);
        let resumed = advance_lab_frame(
            &mut engine,
            &mut graph,
            &mut animation,
            &mut clock,
            Some(&mut playback),
            true,
        )
        .unwrap()
        .unwrap();
        assert_eq!(resumed.raw_delta, 16);
        assert_eq!(resumed.driver_time, normal.driver_time + 16);
        assert_eq!(clock.slept, 16);
    }

    #[test]
    fn static_lab_has_no_dispatch_or_fake_simulation_updates() {
        let mut engine = lhr_engine::runtime::RuntimeEngine::from_sample(0);
        let mut graph = lhr_engine::runtime::RetainedScene::default();
        let mut animation = None;
        let mut clock = Clock::default();
        let mut playback = lab_playback::Playback::default();
        for _ in 0..100 {
            clock.now += 1000;
            assert!(advance_lab_frame(
                &mut engine,
                &mut graph,
                &mut animation,
                &mut clock,
                Some(&mut playback),
                false
            )
            .unwrap()
            .is_none());
        }
        assert_eq!(engine.driver_time(), 0);
        assert_eq!(clock.slept, 0);
    }
}

fn upload_bound(
    graph: &mut lhr_engine::runtime::RetainedScene,
    animation: &mut Option<AnimationPlayback>,
    renderer: &Renderer,
) -> Result<usize, String> {
    match animation {
        Some(AnimationPlayback::Position(playback)) => {
            graph.upload_position_changed(renderer, playback)
        }
        Some(AnimationPlayback::Skeletal(playback)) => {
            graph.upload_skeletal_changed(renderer, 0, playback)
        }
        None => graph.upload_changed(renderer),
    }
}

fn refresh_owned_material_draws(
    runtime: &mut lhr_engine::runtime::RetainedScene,
    scene: &mut Scene,
    renderer: &mut Renderer,
    driver_time: u32,
) -> Result<(), String> {
    for update in runtime.snapshot_animated_material_draws(driver_time as i32)? {
        let range = update.draw_range;
        if range.end > scene.meshes.len() || range.len() != update.snapshot.scene.meshes.len() {
            return Err("Owned animated material draw range no longer matches the scene".into());
        }
        renderer.replace_meshes(range.clone(), &update.snapshot.scene.meshes)?;
        for (index, lighting) in update.snapshot.lighting.iter().enumerate() {
            if let Some(lighting) = lighting {
                renderer.set_mesh_lighting(range.start + index, lighting)?;
            } else {
                renderer.clear_mesh_lighting(range.start + index)?;
            }
        }
        scene.meshes.splice(range, update.snapshot.scene.meshes);
    }
    Ok(())
}
fn advance_bound<B: lhr_engine::clock::ClockBackend + ?Sized>(
    engine: &mut lhr_engine::runtime::RuntimeEngine,
    graph: &mut lhr_engine::runtime::RetainedScene,
    animation: &mut Option<AnimationPlayback>,
    clock: &mut B,
) -> Result<lhr_engine::runtime::RuntimeFrame, lhr_engine::runtime::RuntimeError> {
    match animation {
        Some(AnimationPlayback::Position(playback)) => {
            engine.advance_position(graph, 0, playback, clock)
        }
        Some(AnimationPlayback::Skeletal(playback)) => {
            engine.advance_skeletal(graph, 0, playback, clock)
        }
        None => engine.advance_static(graph, clock),
    }
}
/// Deterministic headless inspection clock; not original wall-clock execution.
struct InspectionClock(u32);
impl lhr_engine::clock::ClockBackend for InspectionClock {
    fn sample_ms(&mut self) -> u32 {
        self.0 += 16;
        self.0
    }
    fn sleep_ms(&mut self, milliseconds: u32) {
        self.0 += milliseconds;
    }
}
fn run() -> Result<(), String> {
    let options = options()?;
    if options.mission_load_probe {
        return mission_load_probe::run(
            options.root.as_deref().unwrap(),
            options.mission.as_deref().unwrap(),
            &options.probe_model_directories,
            options.probe_render_capture.as_deref(),
        );
    }
    if options.mission_play_probe {
        return mission_play_probe(&options);
    }
    if let Some(center) = options.rail_profile {
        return rail_profile_probe(&options, center);
    }
    // These optional startup files are read once, before any Scene/source owner.
    // Keep their actual table across App transitions and inspection reloads.
    let mut startup_patches = lhr_engine::program_patch::ProgramPatchStartup::default();
    if let Some(root) = &options.root {
        let installation = lhr_assets::Installation::open(root).map_err(|e| e.to_string())?;
        startup_patches
            .initialize(&installation)
            .map_err(|e| format!("Startup program patches: {e:?}"))?;
    }
    let startup_settings = options
        .lab_preset
        .as_deref()
        .map(lab_files::load)
        .transpose()?;
    let LoadedScene {
        mut snapshots,
        mut canonical,
        mut scene_frame,
        mut scene,
        mut camera,
        mut runtime,
        mut animation,
        mut car,
    } = load(&options, INITIAL_CAR_VIEWPORT, 0, &startup_patches)?;
    if let Some(settings) = startup_settings {
        camera.vertical_fov_radians = settings.fov_degrees.to_radians();
        camera.validate()?;
        if let Some(owner) = &mut canonical {
            owner.set_inspection_camera_fov(&camera, INITIAL_CAR_VIEWPORT)?;
            let snapshot = owner.snapshot(INITIAL_CAR_VIEWPORT, snapshots.next_number()?)?;
            scene = snapshot.scene;
            scene_frame = Some(snapshot.frame);
        }
    }
    let ui = match &options.root {
        Some(root)
            if !options.synthetic && options.model.is_none() && options.mission.is_none() =>
        {
            let installation = lhr_assets::Installation::open(root).map_err(|e| e.to_string())?;
            Some(menu::MenuUi::load(&installation)?)
        }
        _ => None,
    };
    let mode = if ui.is_some() {
        AppMode::Menu
    } else {
        AppMode::Inspection
    };
    if let Some(ui) = &ui {
        scene.meshes.push(menu::quad(ui.pixels()));
    }
    println!("Application state: {mode:?}");
    if ui.is_some() {
        println!(
            "Menu subset: New Game and Exit; original profile chooser and other actions are not implemented"
        );
    }
    println!(
        "Original-data {} inspection; provisional {} texture sampling; {} indexed mesh groups",
        if options.car_driving {
            "driving car and world"
        } else if options.car_inspection {
            "initialized car"
        } else {
            animation
                .as_ref()
                .map_or("menu/static scene", AnimationPlayback::description)
        },
        if options.linear { "linear" } else { "nearest" },
        scene.meshes.len()
    );
    println!(
        "Mode: {}",
        if options.headless {
            "offscreen single-frame capture"
        } else if options.frames > 0 {
            "window with bounded presented frames"
        } else {
            "interactive window"
        }
    );
    let mut clock_backend = lhr_engine::runtime::PlatformClock::default();
    let initial_sample = lhr_engine::clock::ClockBackend::sample_ms(&mut clock_backend);
    let mut runtime_engine = lhr_engine::runtime::RuntimeEngine::from_sample(initial_sample);
    println!(
        "Retained runtime binding: {} placements, {} source frames; {}; host inspection population/order (not original mission initialization)",
        runtime.models.len(),
        runtime
            .models
            .iter()
            .map(|model| model.pose().len())
            .sum::<usize>(),
        animation
            .as_ref()
            .map_or("static model guards only", AnimationPlayback::description)
    );
    if options.headless {
        if let Some(owner) = &mut canonical {
            let result = (|| {
                let instance = lhr_render::instance();
                let mut renderer = pollster::block_on(Renderer::new(
                    &instance,
                    None,
                    &scene,
                    wgpu::TextureFormat::Rgba8Unorm,
                    options.linear,
                ))?;
                owner.upload_lighting(&mut renderer)?;
                if let Some(settings) = startup_settings {
                    renderer.set_lab_environment(
                        settings.environment,
                        camera.eye.to_array(),
                        0.,
                    )?;
                }
                let frame = scene_frame.as_ref().ok_or("Canonical Scene frame absent")?;
                if mode == AppMode::Menu {
                    renderer.capture_scene_frame_with_overlay(
                        INITIAL_CAR_VIEWPORT[0],
                        INITIAL_CAR_VIEWPORT[1],
                        frame,
                        menu::DESIGN,
                        options.capture.as_deref().unwrap(),
                    )
                } else {
                    renderer.capture_scene_frame(
                        INITIAL_CAR_VIEWPORT[0],
                        INITIAL_CAR_VIEWPORT[1],
                        frame,
                        options.capture.as_deref().unwrap(),
                    )
                }
            })();
            let cleanup = owner.close();
            result?;
            cleanup?;
            startup_patches
                .unload_after_sources()
                .map_err(|e| format!("Startup patch cleanup: {e:?}"))?;
            println!("Canonical active-camera Scene GPU readback complete");
            return Ok(());
        }
        if car.is_some() {
            let result = (|| {
                let owner = car.as_mut().ok_or("Canonical car owner absent")?;
                let snapshot = driving_view::snapshot_scene(
                    owner,
                    &camera,
                    INITIAL_CAR_VIEWPORT,
                    snapshots.next_number()?,
                )?;
                let instance = lhr_render::instance();
                let mut renderer = pollster::block_on(Renderer::new(
                    &instance,
                    None,
                    &snapshot.scene,
                    wgpu::TextureFormat::Rgba8Unorm,
                    options.linear,
                ))?;
                renderer.set_inspection_alpha_cutout(true);
                if let Some(settings) = startup_settings {
                    renderer.set_lab_environment(
                        settings.environment,
                        camera.eye.to_array(),
                        0.,
                    )?;
                }
                for (i, lighting) in snapshot.lighting.iter().enumerate() {
                    if let Some(lighting) = lighting {
                        renderer.set_mesh_lighting(i, lighting)?;
                    } else {
                        renderer.clear_mesh_lighting(i)?;
                    }
                }
                let result = renderer.capture_scene_frame(
                    INITIAL_CAR_VIEWPORT[0],
                    INITIAL_CAR_VIEWPORT[1],
                    &snapshot.frame,
                    options.capture.as_deref().unwrap(),
                );
                renderer.device.poll(wgpu::Maintain::Wait);
                result
            })();
            let cleanup = close_car(&mut car);
            result?;
            cleanup?;
            startup_patches
                .unload_after_sources()
                .map_err(|e| format!("Startup patch cleanup: {e:?}"))?;
            println!("Initialized car GPU readback capture complete");
            return Ok(());
        }
        let result = (|| -> Result<(), String> {
            if let Some(steps) = options.animation_steps {
                runtime_engine = lhr_engine::runtime::RuntimeEngine::from_sample(0);
                let mut inspection_clock = InspectionClock(0);
                for _ in 0..steps {
                    advance_bound(
                        &mut runtime_engine,
                        &mut runtime,
                        &mut animation,
                        &mut inspection_clock,
                    )
                    .map_err(|error| format!("Bound inspection runtime failed: {error:?}"))?;
                }
                println!(
                    "Headless animation inspection: {steps} controlled clock steps, {} ms; host time policy, not native wall-clock execution",
                    steps * 16
                );
            } else {
                let step = advance_bound(
                    &mut runtime_engine,
                    &mut runtime,
                    &mut animation,
                    &mut clock_backend,
                )
                .map_err(|error| format!("Bound runtime failed: {error:?}"))?;
                println!(
                    "Bound runtime: raw delta {}, driver time {}, {} retained model ticks dispatched",
                    step.raw_delta, step.driver_time, step.models_dispatched
                );
            }
            let instance = lhr_render::instance();
            let mut renderer = pollster::block_on(Renderer::new(
                &instance,
                None,
                &scene,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                options.linear,
            ))?;
            if let Some(settings) = startup_settings {
                renderer.set_lab_environment(settings.environment, camera.eye.to_array(), 0.)?;
                if mode != AppMode::Menu {
                    let pivot = Camera::fit(&scene)?.target;
                    renderer.set_lab_model_transform(lab_camera::model_matrix(
                        settings.transform,
                        pivot,
                        0.,
                    ))?;
                }
            }
            upload_bound(&mut runtime, &mut animation, &renderer)?;
            refresh_owned_material_draws(
                &mut runtime,
                &mut scene,
                &mut renderer,
                runtime_engine.driver_time(),
            )?;
            if mode == AppMode::Menu {
                renderer.capture_with_overlay(
                    640,
                    480,
                    &camera,
                    menu::DESIGN,
                    options.capture.as_deref().unwrap(),
                )?;
            } else {
                renderer.capture(
                    INITIAL_CAR_VIEWPORT[0],
                    INITIAL_CAR_VIEWPORT[1],
                    &camera,
                    options.capture.as_deref().unwrap(),
                )?;
            }
            println!("GPU readback capture complete");
            Ok(())
        })();
        drop(animation);
        let cleanup = runtime.close_owned_material_draws();
        result?;
        cleanup?;
        startup_patches
            .unload_after_sources()
            .map_err(|e| format!("Startup patch cleanup: {e:?}"))?;
        return Ok(());
    }
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(_) => {
            let cleanup = close_car(&mut car);
            let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
            return Err(format!(
                "Cannot initialize window event loop; car cleanup: {cleanup:?}; material cleanup: {material_cleanup:?}"
            ));
        }
    };
    let (car_draws, world_bucket) = match live_draw_state(
        &mut car,
        options.car_driving,
        &camera,
        INITIAL_CAR_VIEWPORT,
        &mut snapshots,
    ) {
        Ok(draws) => draws,
        Err(error) => {
            let cleanup = close_car(&mut car);
            let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
            return Err(format!(
                "{error}; car cleanup: {cleanup:?}; material cleanup: {material_cleanup:?}"
            ));
        }
    };
    let audio_capture = match options.audio_capture.as_ref() {
        Some(path) => match lhr_engine::runtime_audio::StereoWavCapture::create(path, 48_000) {
            Ok(capture) => Some(capture),
            Err(error) => {
                let cleanup = close_car(&mut car);
                let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!(
                    "{error}; car cleanup: {cleanup:?}; material cleanup: {material_cleanup:?}"
                ));
            }
        },
        None => None,
    };
    let audio_output = if options.car_driving {
        match lhr_engine::runtime_audio::DeviceAudioOutput::open_default(48_000) {
            Ok(output) => {
                println!(
                    "Driving native audio output: {} at 48000 Hz; same PCM as capture",
                    output.device_name()
                );
                Some(output)
            }
            Err(error) => {
                println!(
                    "Driving native audio output unavailable: {error}; actual PCM mixing remains active"
                );
                None
            }
        }
    } else {
        None
    };
    let lab = options.lab.then(|| {
        let mut lab = engine_lab::State::default();
        if let Some(settings) = startup_settings {
            lab.settings = settings;
        }
        if let Some(path) = &options.lab_preset {
            lab.drafts.preset_path = path.to_string_lossy().into_owned();
        }
        if let Some(model) = &options.model {
            lab.drafts.model_path = model.clone();
        }
        if let Some(mission) = &options.mission {
            lab.drafts.mission_path = mission.clone();
        }
        if let Some(root) = &options.root {
            match lab_catalog::LabCatalog::discover(root) {
                Ok(catalog) => lab.catalog = catalog,
                Err(error) => {
                    lab.status =
                        format!("Asset scan failed: {error}; direct paths remain available")
                }
            }
        }
        lab
    });
    let lab_camera_home = lab_camera::Bookmark::capture(&camera);
    let lab_pivot = Camera::fit(&scene).map_or(camera.target, |fit| fit.target);
    let lab_draw_labels = if options.synthetic {
        lab_app::synthetic_preset(&options)
            .map(|preset| preset.mesh_labels())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut app = App {
        snapshots,
        canonical,
        startup_patches,
        scene_frame,
        mode,
        ui,
        pointer: menu::Pointer::default(),
        menu_shift: false,
        adapter: None,
        scene,
        camera,
        runtime,
        runtime_engine,
        animation,
        car,
        clock_backend,
        options,
        window: None,
        surface: None,
        renderer: None,
        lab,
        native_lab: None,
        last_lab_redraw: std::time::Instant::now(),
        lab_camera_home,
        lab_bookmark: None,
        lab_pivot,
        lab_draw_labels,
        lab_preview_seconds: 0.,
        lab_turntable_degrees: 0.,
        lab_delta_seconds: 0.,
        lab_pointer: None,
        lab_drag: None,
        config: None,
        keys: driving_view::KeyboardInput::default(),
        steering_preview_active: false,
        driving_clock: driving_view::FixedClock::default(),
        driving_controls: driving_view::Controls::default(),
        walk_player: None,
        walk_ground: lhr_engine::mission_play::MeshGroundSpace::default(),
        roads: None,
        dabing: lhr_engine::dabing::DabingOwner::new(),
        dabing_audio: lhr_engine::mission_play::SilentDabingAudio,
        dabing_faces: lhr_engine::mission_play::FaceWeightLog::default(),
        cutscene: lhr_engine::mission_play::CutscenePlayback::idle(),
        cutscene_show: lhr_engine::cutscene_owner::CutsceneShowStart::default(),
        recorder: lhr_engine::cutscene_owner::CutsceneOwner::constructed(
            lhr_engine::cutscene_owner::CutsceneConstructorSeed {
                outer_allocator_byte: 0,
                manager_address_high_byte: 0,
            },
        ),
        last_taxi_pos: [0.0; 3],
        mise01: lhr_engine::mission_play::Mise01Progress::fresh(),
        driving_taxi: false,
        taxi_odometer_m: 0.0,
        last_drive_speed: 0.0,
        car_draws,
        world_bucket,
        audio_capture,
        audio_output,
        count: 0,
        error: None,
    };
    let event_result = event_loop
        .run_app(&mut app)
        .map_err(|_| "Window event loop failed");
    if let Some(renderer) = &app.renderer {
        renderer.device.poll(wgpu::Maintain::Wait);
    }
    if let Some(output) = &app.audio_output {
        println!("Driving native audio output status: {:?}", output.status());
    }
    app.audio_output = None;
    let cleanup = close_car(&mut app.car);
    let scene_cleanup = app.canonical.as_mut().map_or(Ok(()), |owner| owner.close());
    app.animation = None;
    let material_cleanup = app.runtime.close_owned_material_draws();
    let audio_result = match app.audio_capture.take() {
        Some(capture) => {
            println!(
                "Driving audio capture: {} actual mixed stereo frames at 48000 Hz",
                capture.frames()
            );
            capture.finish().map_err(|e| e.to_string())
        }
        None => Ok(()),
    };
    event_result?;
    cleanup?;
    scene_cleanup?;
    material_cleanup?;
    audio_result?;
    if let Some(error) = app.error {
        return Err(error);
    }
    // Failed transitions can retain a distinct Scene in the cleanup registry.
    // Do not unload startup source data while any such owner remains retained.
    if !lhr_engine::scene_render::cleanup_failure_slots().is_empty() {
        return Err("Canonical Scene source cleanup remains interrupted".into());
    }
    app.startup_patches
        .unload_after_sources()
        .map_err(|e| format!("Startup patch cleanup: {e:?}"))?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated public triangle, not an original asset. This passes through the
    // same serialized 4DS source and CLI loader as an interactive Lab model.
    fn animated_cli_model(visual: u8, flags: u32, load_animation_count: u8) -> Vec<u8> {
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(flags.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 0., 0., 0., 1.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.push(8);
        bytes.extend(b"fx00.bmp");
        if flags & 0x04000000 != 0 {
            bytes.extend([3, 0]);
            for value in [0i32, 10, 0, 0] {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.extend(1u16.to_le_bytes());
        bytes.extend([1, visual, 0, 0]);
        bytes.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([1, 4]);
        bytes.extend(b"mesh");
        bytes.push(0);
        bytes.extend(0u16.to_le_bytes());
        bytes.push(1);
        bytes.extend(0f32.to_le_bytes());
        bytes.extend(3u16.to_le_bytes());
        for position in [[-1f32, -1., 0.], [1., -1., 0.], [0., 1., 0.]] {
            for value in position.into_iter().chain([0., 0., 1., 0., 0.]) {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.push(1);
        bytes.extend(1u16.to_le_bytes());
        for value in [0u16, 1, 2, 1] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.push(load_animation_count);
        bytes
    }

    #[test]
    fn cli_animated_model_source_reaches_owned_material_draws_and_preserves_guards() {
        // Reuse the established bitmap fixture and release its initial owner.
        let (path, _, mut initial_runtime) = assembly::tests::retained_animated_fixture();
        initial_runtime.close_owned_material_draws().unwrap();
        std::fs::create_dir_all(path.join("models")).unwrap();
        let model_path = path.join("models/animated.4ds");
        let mut args = vec![
            "--root".into(),
            path.to_string_lossy().into_owned(),
            "--model".into(),
            "models/animated.4ds".into(),
        ];
        if cfg!(feature = "engine-lab") {
            args.push("--lab".into());
        }
        let options = parse_options(args.into_iter()).unwrap();
        let patches = lhr_engine::program_patch::ProgramPatchStartup::default();
        let bytes = animated_cli_model(0, 0x14048000, 0);
        assert!(lhr_assets::parse_four_ds(&bytes).is_err());
        assert!(lhr_assets::parse_four_ds_skin(&bytes).is_err());
        assert!(lhr_assets::parse_four_ds_singlemorph(&bytes).is_err());
        std::fs::write(&model_path, &bytes).unwrap();
        let mut loaded = load(&options, [640, 480], 100, &patches).unwrap();
        assert!(loaded.runtime.has_owned_material_draws());
        assert_eq!(
            loaded.scene.meshes[0].texture.as_ref().unwrap().rgba,
            [255, 0, 0, 255]
        );
        let updates = loaded
            .runtime
            .snapshot_animated_material_draws(110)
            .unwrap();
        assert_eq!(
            updates[0].snapshot.scene.meshes[0]
                .texture
                .as_ref()
                .unwrap()
                .rgba,
            [0, 255, 0, 255]
        );
        loaded.runtime.close_owned_material_draws().unwrap();
        // Actual load/source admission must retain animation and lit guards.
        for (visual, flags, count, expected) in [
            (0, 0x14048000, 1, "animated model unsupported"),
            (0, 0x01000000, 0, "requires an animated material"),
            (
                1,
                0x14048000,
                0,
                "Static unconstructed lit animation unavailable",
            ),
            (0, 0x94048000, 0, "unsupported material effects"),
        ] {
            std::fs::write(&model_path, animated_cli_model(visual, flags, count)).unwrap();
            let error = match load(&options, [640, 480], 100, &patches) {
                Ok(_) => panic!("unexpected admission: {expected}"),
                Err(error) => error,
            };
            assert!(error.contains(expected), "{error}");
        }
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn post_admission_setup_error_drops_playback_before_closing_real_material_owner() {
        use std::{cell::Cell, rc::Rc};
        struct PlaybackObserver {
            source: Arc<lhr_assets::Model>,
            source_owners_at_drop: Rc<Cell<usize>>,
        }
        impl Drop for PlaybackObserver {
            fn drop(&mut self) {
                self.source_owners_at_drop
                    .set(Arc::strong_count(&self.source));
            }
        }
        let (path, scene, mut runtime) = assembly::tests::retained_animated_fixture();
        assert!(runtime.has_owned_material_draws());
        assert_eq!(
            scene.meshes[0].texture.as_ref().unwrap().rgba,
            [255, 0, 0, 255]
        );
        let observed = Rc::new(Cell::new(0));
        let source = Arc::clone(&runtime.models[0].source);
        let owners_before_cleanup = Arc::strong_count(&source);
        let mut playback = Some(PlaybackObserver {
            source,
            source_owners_at_drop: Rc::clone(&observed),
        });
        let error = setup_retained_scene(&mut runtime, &mut playback, |runtime, playback| {
            assert!(runtime.has_owned_material_draws());
            assert!(playback.is_some());
            Err::<(), _>("injected post-admission camera setup failure".into())
        })
        .unwrap_err();
        assert!(error.starts_with("injected post-admission camera setup failure;"));
        assert!(error.contains("retained material cleanup: Ok(())"));
        assert!(playback.is_none());
        // Close releases actual loaded frame/source ownership. Its references
        // must still exist when the controller observer is dropped.
        assert_eq!(observed.get(), owners_before_cleanup);
        assert!(Arc::strong_count(&runtime.models[0].source) < owners_before_cleanup - 1);
        assert!(!runtime.has_owned_material_draws());
        std::fs::remove_dir_all(path).unwrap();
    }

    struct AuthoredViewFixture(PathBuf);
    impl AuthoredViewFixture {
        fn new(position: [f32; 3], rotation: [f32; 4]) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "lhr-authored-view-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("missions/synthetic")).unwrap();
            let chunk = |kind: u16, payload: &[u8]| {
                let mut bytes = kind.to_le_bytes().to_vec();
                bytes.extend((payload.len() as u32 + 6).to_le_bytes());
                bytes.extend(payload);
                bytes
            };
            let mut object = chunk(0x10, b"authored_view\0");
            object.extend(chunk(
                0x20,
                &position
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ));
            object.extend(chunk(
                0x22,
                &rotation
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ));
            std::fs::write(
                root.join("missions/synthetic/scene2.bin"),
                chunk(0x4c53, &chunk(0x4000, &chunk(0x4010, &object))),
            )
            .unwrap();
            Self(root)
        }
        fn load(
            &self,
            retained: &mut lhr_engine::runtime::RetainedScene,
        ) -> Result<Camera, String> {
            let scene = synthetic();
            authored_camera(
                &lhr_assets::Installation::open(&self.0).unwrap(),
                &scene,
                "synthetic",
                "AUTHORED_VIEW",
                retained,
            )
        }
    }
    impl Drop for AuthoredViewFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn close_vector(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).abs().max_element() < 1.0e-5,
            "{actual:?} != {expected:?}"
        );
    }
    #[test]
    fn authored_camera_preserves_roll_and_reflects_the_entire_source_basis() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        // Source SetRot3 rows for q=(cos45,0,0,sin45): up=+X, forward=+Z.
        let fixture = AuthoredViewFixture::new([2., 3., 4.], [half, 0., 0., half]);
        let mut retained = lhr_engine::runtime::RetainedScene::default();
        let camera = fixture.load(&mut retained).unwrap();
        close_vector(camera.eye, Vec3::new(2., 3., -4.));
        close_vector(camera.target - camera.eye, Vec3::NEG_Z);
        close_vector(camera.up, Vec3::X);
        // A world point along source camera up must appear above its center.
        let view = Mat4::look_at_rh(camera.eye, camera.target, camera.up);
        close_vector(view.transform_point3(camera.eye + Vec3::X), Vec3::Y);
        let metadata = retained.metadata().unwrap();
        assert_eq!(
            metadata.objects[0].rotation_wxyz,
            Some([half, 0., 0., half])
        );
        assert_eq!(metadata.objects[0].position, Some([2., 3., 4.]));
    }
    #[test]
    fn authored_camera_accepts_vertical_forward_with_its_authored_up() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        // Source rows: forward=+Y, up=-Z. Z reflection makes host up=+Z.
        let fixture = AuthoredViewFixture::new([0.; 3], [half, half, 0., 0.]);
        let mut retained = lhr_engine::runtime::RetainedScene::default();
        let camera = fixture.load(&mut retained).unwrap();
        close_vector(camera.target - camera.eye, Vec3::Y);
        close_vector(camera.up, Vec3::Z);
        camera.validate().unwrap();
        assert!(retained.metadata().is_some());
    }
    #[test]
    fn authored_camera_invalid_inputs_do_not_retain_metadata_or_supply_a_fallback() {
        for (position, rotation) in [
            ([0.; 3], [0.; 4]),
            ([0.; 3], [2., 0., 0., 0.]),
            ([0.; 3], [f32::NAN, 0., 0., 0.]),
            ([f32::INFINITY, 0., 0.], [1., 0., 0., 0.]),
            // Finite eye too large for the host renderer's supported basis.
            ([1.0e9, 0., 0.], [1., 0., 0., 0.]),
        ] {
            let fixture = AuthoredViewFixture::new(position, rotation);
            let mut retained = lhr_engine::runtime::RetainedScene::default();
            assert!(fixture.load(&mut retained).is_err());
            assert!(retained.metadata().is_none());
        }
    }
    #[test]
    fn effect_diagnostics_include_flag_only_materials_without_optional_payloads() {
        let mut material = lhr_assets::Material {
            flags: 0,
            ambient: [0.0; 3],
            diffuse: [1.0; 3],
            texture: String::new(),
            emission: [0.0; 3],
            opacity: 1.0,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        };
        assert!(!material_effects_omitted(&material));
        for flag in [
            0x80000, 0x01000000, 0x04000000, 0x20000000, 0x40000000, 0x80000000,
        ] {
            material.flags = flag;
            assert!(material_effects_omitted(&material), "flag {flag:08x}");
        }
        material.flags = 0x38000; // Clamp and keep-pixels flags are not omitted effects.
        assert!(!material_effects_omitted(&material));
        material.opacity = 0.5;
        assert!(material_effects_omitted(&material));
    }
    #[test]
    fn aggregate_budget_rejects_many_individually_small_allocations() {
        let mut total = 0;
        charge_scene(&mut total, 40, 100).unwrap();
        charge_scene(&mut total, 40, 100).unwrap();
        assert!(charge_scene(&mut total, 40, 100).is_err());
        assert_eq!(total, 80);
        assert!(charge_scene(&mut total, usize::MAX, usize::MAX).is_err());
    }
    #[test]
    fn source_quaternion_rejects_nonunit_and_zero_without_changing_valid_rotation() {
        assert!(source_rotation([0.0; 4]).is_err());
        assert!(source_rotation([2.0, 0.0, 0.0, 0.0]).is_err());
        assert_eq!(
            source_rotation([1.0, 0.0, 0.0, 0.0]).unwrap(),
            Quat::IDENTITY
        );
    }
    #[test]
    fn finite_inputs_that_overflow_transformed_vertex_are_rejected() {
        let transform = Mat4::from_scale(Vec3::splat(f32::MAX));
        assert!(
            lhr_engine::runtime::GeometryTransform::Inspection(transform)
                .render_point([2.0, 0.0, 0.0])
                .is_err()
        );
        assert_eq!(
            lhr_engine::runtime::GeometryTransform::Inspection(Mat4::IDENTITY)
                .render_point([1.0, 2.0, 3.0])
                .unwrap(),
            [1.0, 2.0, -3.0]
        );
    }
}

#[cfg(test)]
mod mission_probe_option_tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        parse_options(args.iter().map(|arg| (*arg).into()))
    }

    #[test]
    fn campaign_directory_names_are_accepted_by_cli_and_direct_probe_validation() {
        for name in ["mise02-saliery", "mise03-morello", "MISE01", "mission_02"] {
            let args = ["--root", "/synthetic-installation", "--mission", name];
            assert_eq!(parse(&args).unwrap().mission.as_deref(), Some(name));
            let mut probe_args = args.to_vec();
            probe_args.push("--mission-load-probe");
            assert_eq!(parse(&probe_args).unwrap().mission.as_deref(), Some(name));
            assert!(mission_load_probe::validate_inputs(name, &[]).is_ok());
        }
        let longest = "a".repeat(64);
        assert!(parse(&["--root", "/synthetic-installation", "--mission", &longest]).is_ok());
        assert!(mission_load_probe::validate_inputs(&longest, &[]).is_ok());
    }

    #[test]
    fn cli_and_direct_probe_reject_unsafe_or_overlong_mission_names() {
        let overlong = "a".repeat(65);
        for name in [
            "",
            "/mise02-saliery",
            "mise02/morello",
            "mise02\\morello",
            ".",
            "..",
            "../mise02",
            "mise02..",
            "mise02 morello",
            "mise02\tmorello",
            "mise02\nmorello",
            "mise02\0morello",
            "mise02-č",
            &overlong,
        ] {
            let args = ["--root", "/synthetic-installation", "--mission", name];
            assert!(parse(&args).is_err(), "accepted mission name: {name:?}");
            let mut probe_args = args.to_vec();
            probe_args.push("--mission-load-probe");
            assert!(parse(&probe_args).is_err(), "accepted probe name: {name:?}");
            assert!(mission_load_probe::validate_inputs(name, &[]).is_err());
        }
    }

    #[test]
    fn probe_keeps_explicit_directory_order_and_has_no_implicit_directory() {
        let base = [
            "--mission-load-probe",
            "--root",
            "/synthetic-installation",
            "--mission",
            "MISE01",
        ];
        let options = parse(&base).unwrap();
        assert!(options.mission_load_probe);
        assert!(options.probe_model_directories.is_empty());
        assert!(options.probe_render_capture.is_none());
        let mut args = base.to_vec();
        args.extend([
            "--probe-model-dir",
            "models/first",
            "--probe-model-dir",
            "models/second",
        ]);
        assert_eq!(
            parse(&args).unwrap().probe_model_directories,
            ["models/first", "models/second"]
        );
        args.extend(["--probe-render-capture", "mission.png"]);
        assert_eq!(
            parse(&args).unwrap().probe_render_capture,
            Some(PathBuf::from("mission.png"))
        );
        assert!(parse(&["--probe-render-capture", "mission.png"]).is_err());
    }

    #[test]
    fn probe_rejects_even_explicit_default_valued_presentation_flags() {
        for extra in [
            vec!["--lab"],
            vec!["--synthetic"],
            vec!["--headless"],
            vec!["--frames", "0"],
            vec!["--textures", "maps"],
            vec!["--linear"],
            vec!["--capture", "unused.png"],
            vec!["--model", "model.4ds"],
            vec!["--car-driving"],
            vec!["--car-inspection"],
            vec!["--animation", "animation.5ds"],
            vec!["--skeletal-animation"],
            vec!["--animation-steps", "0"],
            vec!["--sample-animation-ms", "0"],
            vec!["--steer", "0"],
            vec!["--steer-steps", "1"],
            vec!["--audio-capture", "unused.wav"],
        ] {
            let mut args = vec![
                "--mission-load-probe",
                "--root",
                "/synthetic-installation",
                "--mission",
                "MISE01",
            ];
            args.extend(extra);
            assert!(
                parse(&args).is_err(),
                "accepted incompatible flags: {args:?}"
            );
        }
    }

    #[test]
    fn probe_requires_its_inputs_and_does_not_change_existing_window_selection() {
        for args in [
            vec!["--mission-load-probe"],
            vec!["--mission-load-probe", "--root", "/synthetic-installation"],
            vec![
                "--mission-load-probe",
                "--root",
                "/synthetic-installation",
                "--mission",
                "../MISE01",
            ],
            vec![
                "--mission-load-probe",
                "--root",
                "/synthetic-installation",
                "--mission",
                "MISE01",
                "--probe-model-dir",
                "",
            ],
            vec!["--synthetic", "--probe-model-dir", "models"],
        ] {
            assert!(parse(&args).is_err(), "accepted invalid probe: {args:?}");
        }
        if cfg!(feature = "engine-lab") {
            let synthetic = parse(&["--synthetic", "--lab"]).unwrap();
            assert!(synthetic.synthetic && synthetic.lab && !synthetic.mission_load_probe);
        } else {
            assert!(parse(&["--synthetic", "--lab"])
                .err()
                .unwrap()
                .contains("--features engine-lab"));
        }
        let mission = parse(&["--root", "/synthetic-installation", "--mission", "MISE01"]).unwrap();
        assert!(!mission.mission_load_probe);
    }
}
