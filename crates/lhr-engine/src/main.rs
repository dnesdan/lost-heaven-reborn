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
#[path = "lab_playback.rs"]
mod lab_playback;
mod lab_state;
#[path = "menu.rs"]
mod menu;
#[path = "mission_load_probe.rs"]
mod mission_load_probe;
#[path = "neighborhood.rs"]
mod neighborhood;
use glam::{Mat4, Quat, Vec3};
use lhr_render::{Camera, Mesh, Renderer, Scene, Texture, Vertex};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};
const INITIAL_CAR_VIEWPORT: [u32; 2] = [960, 640];

/// The Lab must not interpret focus-restoration presses as application actions
/// or held input. Keep every release, including synthetic releases, so existing
/// keys are cleared. Non-Lab routing retains its previous behavior.
fn application_key(
    lab_enabled: bool,
    key: PhysicalKey,
    state: ElementState,
    is_synthetic: bool,
) -> Option<KeyCode> {
    if lab_enabled && is_synthetic && state == ElementState::Pressed {
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

struct Options {
    mission_load_probe: bool,
    probe_model_directories: Vec<String>,
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
    mission: Option<String>,
    car_inspection: bool,
    car_driving: bool,
    audio_capture: Option<PathBuf>,
    steer: Option<f32>,
    steer_steps: u32,
}
fn options() -> Result<Options, String> {
    parse_options(std::env::args().skip(1))
}
fn parse_options(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut result = Options {
        mission_load_probe: false,
        probe_model_directories: Vec::new(),
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
        mission: None,
        car_inspection: false,
        car_driving: false,
        audio_capture: None,
        steer: None,
        steer_steps: 1,
    };
    let mut provided_options = Vec::new();
    let mut steer_steps_set = false;
    while let Some(arg) = args.next() {
        provided_options.push(arg.clone());
        match arg.as_str() {
            "--mission-load-probe" => result.mission_load_probe = true,
            "--probe-model-dir" => result.probe_model_directories.push(args.next().ok_or("Missing probe model directory")?),
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
            "--help" => return Err(concat!(
                "Usage: lhr-engine [--synthetic | --root DIRECTORY [--model RELATIVE_4DS [--animation RELATIVE_5DS] | --mission NAME]] ",
                "[--car-driving | --car-inspection [--steer VALUE [--steer-steps N]]] [--textures RELATIVE_DIRECTORY] [--headless --capture PNG [--animation-steps N]] [--frames N] [--linear]. ",
                "--lab requires a build with --features engine-lab. It opens native Engine Lab controls for pause, 16 ms steps, reload, host camera and actual driving input. F1 toggles the panel; F2 pauses; F3 steps; F4 reloads. Requires a window. --capture continues to capture the scene without the Lab overlay. ",
                "--mission-load-probe --root DIRECTORY --mission NAME [--probe-model-dir DIRECTORY ...] runs the shared CPU load-session diagnostic without a window; it excludes all presentation and playback flags. It does not execute gameplay. ",
                "--audio-capture WAV records the actual mixed driving audio and requires --car-driving. ",
                "--car-driving requires --root and --model and opens the original MISE01 driving world. W accelerates, S brakes, A/D steer, Space applies the handbrake; R reloads and Escape exits. The chase camera is a host view. ",
                "--car-inspection requires --root and --model; constructs and initializes the CPU car, then renders that same live arena's camera-selected owned LOD and source distance fade, with unlit materials. A/D previews the original wheel-steering update; arrows/W/S control the camera. R explicitly reloads the car and resets live input. Lighting, other material effects, body simulation, driving and mission execution are unavailable. ",
                "--steer accepts a finite normalized request in [-1,1]; --steer-steps 0..600 (default 1) applies that request through controlled 16 ms host inspection-clock steps before presentation. This previews actual initialized wheel frames, without translating the car or claiming a driving simulation. ",
                "Headless --animation-steps 0..600 uses controlled 16 ms inspection-clock steps; omit it for one platform-clock step. ",
                "Root alone opens the supported original Czech main-menu subset (New Game/Exit). ",
                "Menu: arrows, Tab/Shift+Tab, Enter or mouse. New Game loads static INTRO; Esc loads static MISE01, then returns to menu. ",
                "Scene inspection: arrows orbit, W/S zoom. --animation alone admits a matched single-root position track. Add --skeletal-animation for sequential skeletal slot0 mode2 playback, or --sample-animation-ms MS for one independent skeletal pose. Original profile, cutscene, mission-script and gameplay execution are not implemented. ",
                "Sampling is provisional nearest (or --linear), not verified original behavior."
            ).into()),
            _ => return Err("Unknown command option".into()),
        }
    }
    if result.mission_load_probe {
        for option in &provided_options {
            if !matches!(
                option.as_str(),
                "--mission-load-probe" | "--root" | "--mission" | "--probe-model-dir"
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
    if result.synthetic == result.root.is_some() {
        return Err("Select exactly one of --synthetic or --root".into());
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
fn load(options: &Options, viewport: [u32; 2], driver_time: u32) -> Result<LoadedScene, String> {
    let Some(root) = &options.root else {
        let scene = synthetic();
        let camera = Camera::fit(&scene)?;
        return Ok(LoadedScene {
            canonical: None,
            scene_frame: None,
            scene,
            camera,
            runtime: lhr_engine::runtime::RetainedScene::default(),
            animation: None,
            car: None,
        });
    };
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
        );
    }
    if options.model.is_none() {
        let mission = options.mission.as_deref().unwrap_or("00menu");
        let mut canonical = lhr_engine::scene_render::CanonicalSceneOwner::open_at_driver_time(
            &installation,
            mission,
            viewport,
            driver_time as i32,
        )?;
        if let Some(frontier) = &canonical.frontier {
            println!("Canonical scene2 retained frontier: cursor {}, entry {:#x}: {}; mission initialization remains incomplete",frontier.cursor,frontier.entry,frontier.detail);
        }
        let camera = canonical.camera_control()?;
        let snapshot = match canonical.snapshot(viewport, 1) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let cleanup = canonical.close();
                return Err(format!("{error}; canonical Scene cleanup: {cleanup:?}"));
            }
        };
        println!("Canonical Scene active-camera rendering: {} selected material groups; source sector/portal/occluder visibility, main/backdrop/near projections; bounded material support",snapshot.scene.meshes.len());
        return Ok(LoadedScene {
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
        let model = lhr_assets::parse_four_ds(&data)
            .or_else(|_| lhr_assets::parse_four_ds_skin(&data))
            .or_else(|_| lhr_assets::parse_four_ds_singlemorph(&data))
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
            lhr_assets::parse_four_ds(&data)
                .or_else(|_| lhr_assets::parse_four_ds_skin(&data))
                .or_else(|_| lhr_assets::parse_four_ds_singlemorph(&data))
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
            println!("Bound original skeletal controller: slot0 mode2, {} ms; position/rotation, Target, morph and first-LOD CPU skin; host inspection initialization, camera and rendering; no mission integration", playback.duration_ms());
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
        canonical: None,
        scene_frame: None,
        scene,
        camera,
        runtime,
        animation,
        car: None,
    })
}
type DrivingWorldView = ([i32; 2], u64);

fn live_draw_state(
    car: &mut Option<lhr_engine::car_runtime::GuardedCarRuntime>,
    driving: bool,
    camera: &Camera,
    viewport: [u32; 2],
) -> Result<(usize, Option<DrivingWorldView>), String> {
    let Some(owner) = car.as_mut() else {
        return Ok((0, None));
    };
    let snapshot = driving_view::snapshot_scene(owner, camera, viewport, 1)?;
    let _ = driving;
    Ok((snapshot.scene.meshes.len(), None))
}
fn close_car(car: &mut Option<lhr_engine::car_runtime::GuardedCarRuntime>) -> Result<(), String> {
    if let Some(owner) = car.as_mut() {
        let cleanup = owner.close()?;
        println!("CPU car/model/scene resources released; actor registrations {}, CPU heap blocks {}, visual buffers {}, textures {}, retained scene frames {}, driver scene registrations {}, driver scene capacity {}; native device discard unexecuted (CPU projection)",cleanup.actor_registry_entries,cleanup.heap_allocations,cleanup.graphics,cleanup.textures,cleanup.retained_scene_prefix_frames,cleanup.driver_scene_registrations,cleanup.driver_scene_capacity);
    }
    *car = None;
    Ok(())
}
fn load_car(
    installation: lhr_assets::Installation,
    path: &str,
    steer: Option<f32>,
    steer_steps: u32,
    driving: bool,
    viewport: [u32; 2],
    driver_time: u32,
) -> Result<LoadedScene, String> {
    let runtime_options = lhr_engine::car_runtime::CarRuntimeOptions {
        material_driver_time: driver_time as i32,
        ..Default::default()
    };
    let opened = if driving {
        lhr_engine::car_runtime::GuardedCarRuntime::open_driving_scene(
            installation,
            path,
            runtime_options,
            "mise01",
        )
    } else {
        lhr_engine::car_runtime::GuardedCarRuntime::open(installation, path, runtime_options)
    };
    let mut owner = opened?;
    if let Some(frontier) = owner.scene2_frontier() {
        println!("Canonical driving scene2 retained frontier: cursor {}, entry {:#x}: {}; mission initialization remains incomplete",frontier.cursor,frontier.entry,frontier.detail);
    }
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
        let snapshot = driving_view::snapshot_scene(&mut owner, &camera, viewport, 1)?;
        if driving {
            driving_view::sync_audio_listener(&mut owner, &camera)?;
        }
        println!("Canonical car Scene: {} seats, {} cached animation tracks; {} selected material groups; actual active camera, source visibility and owned LOD buffers",owner.summary().seats,owner.summary().cached_tracks,snapshot.scene.meshes.len());
        if driving {
            println!("Driving controls: W throttle, S brake, A/D steer, Space handbrake; R reload, Escape exit. Fixed 16 ms simulation, same canonical car/world Scene; host chase controller authors its active camera. Mission scripts and traffic are not executed.");
        }
        Ok((snapshot.scene, camera, snapshot.frame))
    })();
    match result {
        Ok((scene, camera, scene_frame)) => Ok(LoadedScene {
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
    canonical: Option<lhr_engine::scene_render::CanonicalSceneOwner>,
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
    config: Option<wgpu::SurfaceConfiguration>,
    keys: HashSet<KeyCode>,
    steering_preview_active: bool,
    driving_clock: driving_view::FixedClock,
    driving_controls: driving_view::Controls,
    car_draws: usize,
    world_bucket: Option<DrivingWorldView>,
    audio_capture: Option<lhr_engine::runtime_audio::StereoWavCapture>,
    audio_output: Option<lhr_engine::runtime_audio::DeviceAudioOutput>,
    count: u32,
    error: Option<String>,
}
impl App {
    /// A retained scene/car has a real material timeline even without gameplay
    /// callbacks. The synthetic quad alone still has no runtime to advance.
    fn lab_runtime_available(&self) -> bool {
        !self.runtime.models.is_empty() || self.car.is_some() || self.canonical.is_some()
    }

    fn clear_lab_audio_queue(&mut self) -> Result<(), String> {
        if let Some(output) = &mut self.audio_output {
            output.clear_queue().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn reload_lab_scene(&mut self) -> Result<(), String> {
        if self.options.car_inspection || self.options.car_driving {
            self.reload_car()?;
        } else {
            self.transition(self.mode)?;
        }
        let sample = lhr_engine::clock::ClockBackend::sample_ms(&mut self.clock_backend);
        self.runtime_engine = lhr_engine::runtime::RuntimeEngine::from_sample(sample);
        println!("Engine Lab: scene reloaded; runtime time and driving tick count reset");
        Ok(())
    }

    fn lab_window_event(&mut self, event: &WindowEvent) -> Result<(), String> {
        if self.lab.is_none() {
            return Ok(());
        }
        if matches!(event, WindowEvent::Focused(false)) {
            self.lab.as_mut().unwrap().focus_lost();
            self.keys.clear();
            self.clear_lab_audio_queue()?;
        }
        let WindowEvent::KeyboardInput {
            event,
            is_synthetic,
            ..
        } = event
        else {
            return Ok(());
        };
        // Winit can synthesize held-key presses during focus changes. They
        // must not resume, step or reload a scene that focus loss just paused.
        if *is_synthetic || event.state != ElementState::Pressed || event.repeat {
            return Ok(());
        }
        match event.physical_key {
            PhysicalKey::Code(KeyCode::F1) => {
                let lab = self.lab.as_mut().unwrap();
                lab.visible = !lab.visible;
                self.keys.clear();
            }
            PhysicalKey::Code(KeyCode::F2) if self.lab_runtime_available() => {
                let lab = self.lab.as_mut().unwrap();
                let paused = !lab.playback.paused();
                lab.playback.set_paused(paused);
                self.keys.clear();
                if paused {
                    self.clear_lab_audio_queue()?;
                }
                println!(
                    "Engine Lab: paused={paused}; driver_time_ms={}; driving_ticks={}",
                    self.runtime_engine.driver_time(),
                    self.driving_clock.ticks
                );
            }
            PhysicalKey::Code(KeyCode::F3) if self.lab_runtime_available() => {
                self.lab.as_mut().unwrap().playback.request_step();
                self.clear_lab_audio_queue()?;
            }
            PhysicalKey::Code(KeyCode::F4) => self.reload_lab_scene()?,
            _ => {}
        }
        Ok(())
    }

    fn lab_diagnostics(&mut self) -> Result<engine_lab::Diagnostics, String> {
        let now = std::time::Instant::now();
        let frame_ms = now.duration_since(self.last_lab_redraw).as_secs_f32() * 1000.;
        self.last_lab_redraw = now;
        let (car, feedback) = if self.options.car_driving {
            let owner = self.car.as_mut().ok_or("Engine Lab has no driving owner")?;
            (
                Some(owner.driving_state()?),
                Some(owner.driving_control_feedback()?),
            )
        } else {
            (None, None)
        };
        Ok(engine_lab::Diagnostics {
            profile: if self.options.synthetic {
                "Public synthetic scene"
            } else if self.options.car_driving {
                "Original data · Driving"
            } else if self.options.car_inspection {
                "Original data · Car inspection"
            } else if self.animation.is_some() {
                "Original data · Animation inspection"
            } else {
                "Original data · Scene inspection"
            },
            runtime_available: self.lab_runtime_available(),
            frame_ms,
            driver_time_ms: self.runtime_engine.driver_time(),
            driving_ticks: self.driving_clock.ticks,
            scene_draws: self.scene.meshes.len(),
            vertices: self
                .scene
                .meshes
                .iter()
                .map(|mesh| mesh.vertices.len())
                .sum(),
            triangles: self
                .scene
                .meshes
                .iter()
                .map(|mesh| mesh.indices.len() / 3)
                .sum(),
            viewport: self
                .config
                .as_ref()
                .map_or(INITIAL_CAR_VIEWPORT, |config| [config.width, config.height]),
            car,
            feedback,
            driving: self.options.car_driving,
        })
    }

    fn apply_lab_camera(&mut self) -> Result<(), String> {
        let Some(lab) = &self.lab else {
            return Ok(());
        };
        let settings = lab.settings;
        settings.validate()?;
        if self.options.car_driving {
            let owner = self
                .car
                .as_mut()
                .ok_or("Engine Lab has no driving camera owner")?;
            let root = owner
                .model()
                .ok_or("Engine Lab has no driving camera model")?
                .root();
            self.camera =
                driving_view::chase_camera_with_settings(owner.arena_mut(), root, settings.chase)?;
            driving_view::sync_audio_listener(owner, &self.camera)?;
        }
        self.camera.vertical_fov_radians = settings.fov_degrees.to_radians();
        self.camera.validate()
    }

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
        // Inspection reloads preserve an explicitly selected CLI mission.
        if mode != AppMode::Inspection {
            self.options.mission = match mode {
                AppMode::IntroScene => Some("intro".into()),
                AppMode::FirstMissionScene => Some("mise01".into()),
                _ => None,
            };
        }
        let viewport = self
            .config
            .as_ref()
            .map_or(INITIAL_CAR_VIEWPORT, |c| [c.width, c.height]);
        let LoadedScene {
            canonical,
            scene_frame,
            mut scene,
            camera,
            mut runtime,
            mut animation,
            mut car,
        } = load(&self.options, viewport, self.runtime_engine.driver_time())?;
        let (car_draws, world_bucket) = match live_draw_state(
            &mut car,
            self.options.car_driving,
            &camera,
            viewport,
        ) {
            Ok(draws) => draws,
            Err(error) => {
                let cleanup = close_car(&mut car);
                let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!("{error}; new car cleanup: {cleanup:?}; new material cleanup: {material_cleanup:?}"));
            }
        };
        if mode == AppMode::Menu {
            scene
                .meshes
                .push(menu::quad(self.ui.as_ref().unwrap().pixels()));
        }
        let result = (|| {
            let adapter = self.adapter.as_ref().ok_or("Graphics adapter absent")?;
            pollster::block_on(Renderer::with_adapter(
                adapter,
                &scene,
                self.config.as_ref().unwrap().format,
                self.options.linear,
            ))
        })();
        let mut renderer = match result {
            Ok(renderer) => renderer,
            Err(error) => {
                let cleanup = close_car(&mut car);
                let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!(
                    "{error}; car cleanup: {cleanup:?}; material cleanup: {material_cleanup:?}"
                ));
            }
        };
        if let Some(owner) = &canonical {
            if let Err(error) = owner.upload_lighting(&mut renderer) {
                let cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!("{error}; new material cleanup: {cleanup:?}"));
            }
        }
        renderer
            .set_inspection_alpha_cutout(self.options.car_inspection || self.options.car_driving);
        if let Some(old) = &self.renderer {
            old.device.poll(wgpu::Maintain::Wait);
        }
        if let Err(error) = close_car(&mut self.car) {
            renderer.device.poll(wgpu::Maintain::Wait);
            let cleanup = close_car(&mut car);
            let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
            return Err(format!(
                "{error}; new car cleanup: {cleanup:?}; new material cleanup: {material_cleanup:?}"
            ));
        }
        self.surface
            .as_ref()
            .unwrap()
            .configure(&renderer.device, self.config.as_ref().unwrap());
        if let Some(owner) = &mut self.canonical {
            if let Err(error) = owner.close() {
                let cleanup = close_car(&mut car);
                let material_cleanup = close_retained_materials(&mut animation, &mut runtime);
                return Err(format!("{error}; new car cleanup: {cleanup:?}; new material cleanup: {material_cleanup:?}"));
            }
        }
        self.animation = None;
        if let Err(error) = self.runtime.close_owned_material_draws() {
            let cleanup = close_retained_materials(&mut animation, &mut runtime);
            return Err(format!("{error}; new material cleanup: {cleanup:?}"));
        }
        self.native_lab = self.lab.as_ref().map(|_| {
            let window = self.window.as_ref().unwrap();
            let format = self.config.as_ref().unwrap().format;
            match &self.native_lab {
                Some(previous) => previous.recreate(window, &renderer, format),
                None => engine_lab::NativeLab::new(window, &renderer, format),
            }
        });
        self.renderer = Some(renderer);
        self.canonical = canonical;
        self.scene_frame = scene_frame;
        self.scene = scene;
        self.camera = camera;
        self.runtime = runtime;
        self.animation = animation;
        self.car = car;
        self.car_draws = car_draws;
        self.world_bucket = world_bucket;
        self.runtime_engine.reset_clock(&mut self.clock_backend);
        self.mode = mode;
        self.keys.clear();
        self.steering_preview_active = false;
        self.driving_clock.reset();
        self.driving_controls = driving_view::Controls::default();
        if let Some(lab) = &mut self.lab {
            lab.scene_reloaded();
        }
        self.last_lab_redraw = std::time::Instant::now();
        if self.options.car_driving {
            println!("Application state: {mode:?}; driving car and original static world loaded; mission scripts and traffic are not executed");
        } else {
            println!("Application state: {mode:?}; scene assets loaded; gameplay and cutscene execution unsupported");
        }
        Ok(())
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, message: &str) {
        self.error = Some(message.into());
        event_loop.exit();
    }
    fn reload_car(&mut self) -> Result<(), String> {
        if let Some(output) = &mut self.audio_output {
            let discarded = output.clear_queue().map_err(|e| e.to_string())?;
            println!(
                "Driving reload discarded {discarded} queued PCM frames from the previous car"
            );
        }
        if let Some(renderer) = &self.renderer {
            renderer.device.poll(wgpu::Maintain::Wait);
        }
        close_car(&mut self.car)?;
        self.renderer = None;
        self.transition(AppMode::Inspection)
    }
    fn drive_car(&mut self, elapsed_ms: u32) -> Result<(), String> {
        if !self.options.car_driving {
            return Ok(());
        }
        let old_ticks = self.driving_clock.ticks;
        let ticks = self.driving_clock.advance(elapsed_ms);
        if ticks == 0 {
            return Ok(());
        }
        let keyboard = driving_view::Controls::from_keys(&self.keys);
        let controls = self
            .lab
            .as_ref()
            .map_or(keyboard, |lab| lab.controls(keyboard));
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
        for _ in 0..ticks {
            steering_limit_at_input =
                Some(owner.driving_control_feedback()?.steering_limit_radians);
            state = Some(owner.driving_step(
                driving_view::STEP_MS,
                lhr_engine::car_runtime::CarDrivingInput {
                    throttle: controls.throttle,
                    brake: controls.brake,
                    steer: controls.steer,
                    handbrake: controls.handbrake,
                },
            )?);
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
            println!("Driving state: {{\"tick\":{},\"position\":{:?},\"speed\":{},\"contacts\":{},\"throttle\":{},\"brake\":{},\"steer\":{},\"handbrake\":{},\"audio_frames\":{},\"host_unix_ms\":{},\"actual_throttle\":{},\"actual_brake\":{},\"steering_requested_radians\":{},\"steering_applied_radians\":{},\"steering_limit_radians\":{},\"steering_limit_at_input_radians\":{},\"handbrake_force\":{},\"gear\":{}}}", self.driving_clock.ticks, state.position, state.speed, state.contacts, controls.throttle, controls.brake, controls.steer, controls.handbrake, audio_frames, host_unix_ms, feedback.throttle, feedback.brake, feedback.steering_requested_radians, feedback.steering_applied_radians, feedback.steering_limit_radians, steering_limit_at_input, feedback.handbrake_force, feedback.gear);
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
            println!("Driving world presentation refreshed at source bucket {bucket:?}; {} actual static draws", self.scene.meshes.len() - self.car_draws);
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
            self.count.wrapping_add(1),
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
                } else {
                    "Lost Heaven Reborn — original menu / static scene inspection"
                })
                .with_inner_size(winit::dpi::LogicalSize::new(
                    if self.mode == AppMode::Menu { 640 } else { 960 },
                    if self.mode == AppMode::Menu { 480 } else { 640 },
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
                    self.lab.is_some(),
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
                                self.fail(event_loop, &error);
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
                            self.keys.insert(code);
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
                    if commands.reload_scene {
                        if let Err(error) = self.reload_lab_scene() {
                            self.fail(event_loop, &error);
                        }
                        // The new device has a new egui context/font atlas.
                        // Never submit old-device UI jobs after this transition.
                        return;
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
                    if let Some(lab) = &mut self.lab {
                        lab.completed_frames += 1;
                        if lab.playback.paused() {
                            println!("Engine Lab: single step {} ms; driver_time_ms={}; driving_ticks={}", step.raw_delta, step.driver_time, self.driving_clock.ticks);
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
                for key in &self.keys {
                    match key {
                        KeyCode::ArrowLeft if !self.options.car_driving => yaw += 0.025,
                        KeyCode::ArrowRight if !self.options.car_driving => yaw -= 0.025,
                        KeyCode::ArrowUp if !self.options.car_driving => pitch += 0.025,
                        KeyCode::ArrowDown if !self.options.car_driving => pitch -= 0.025,
                        KeyCode::KeyW if !self.options.car_driving => zoom *= 0.98,
                        KeyCode::KeyS if !self.options.car_driving => zoom *= 1.02,
                        _ => {}
                    }
                }
                let rotated = Quat::from_rotation_y(yaw)
                    * Quat::from_axis_angle(offset.cross(Vec3::Y).normalize_or_zero(), pitch)
                    * offset
                    * zoom;
                if rotated.normalize().dot(Vec3::Y).abs() < 0.995
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
                        let snapshot = owner.snapshot(viewport, self.count.wrapping_add(1))?;
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
        );
    }
    let LoadedScene {
        mut canonical,
        scene_frame,
        mut scene,
        camera,
        mut runtime,
        mut animation,
        mut car,
    } = load(&options, INITIAL_CAR_VIEWPORT, 0)?;
    let ui = match &options.root {
        Some(root) if options.model.is_none() && options.mission.is_none() => {
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
        animation.as_ref().map_or("static model guards only", AnimationPlayback::description)
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
            println!("Canonical active-camera Scene GPU readback complete");
            return Ok(());
        }
        if car.is_some() {
            let result = (|| {
                let owner = car.as_mut().ok_or("Canonical car owner absent")?;
                let snapshot =
                    driving_view::snapshot_scene(owner, &camera, INITIAL_CAR_VIEWPORT, 1)?;
                let instance = lhr_render::instance();
                let mut renderer = pollster::block_on(Renderer::new(
                    &instance,
                    None,
                    &snapshot.scene,
                    wgpu::TextureFormat::Rgba8Unorm,
                    options.linear,
                ))?;
                renderer.set_inspection_alpha_cutout(true);
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
    let (car_draws, world_bucket) =
        match live_draw_state(&mut car, options.car_driving, &camera, INITIAL_CAR_VIEWPORT) {
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
                println!("Driving native audio output unavailable: {error}; actual PCM mixing remains active");
                None
            }
        }
    } else {
        None
    };
    let lab = options.lab.then(engine_lab::State::default);
    let mut app = App {
        canonical,
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
        config: None,
        keys: HashSet::new(),
        steering_preview_active: false,
        driving_clock: driving_view::FixedClock::default(),
        driving_controls: driving_view::Controls::default(),
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
    fn ordinary_and_probe_modes_accept_hyphenated_original_missions() {
        for mission in ["mise02-saliery", "MISE01", "00menu", "intro"] {
            assert!(parse(&[
                "--root", "/synthetic-installation", "--mission", mission,
            ]).is_ok(), "ordinary mode rejected {mission}");
            assert!(parse(&[
                "--mission-load-probe", "--root", "/synthetic-installation",
                "--mission", mission,
            ]).is_ok(), "probe mode rejected {mission}");
        }
    }

    #[test]
    fn both_entrypoints_reject_path_traversal_and_non_ascii_missions() {
        for mission in [
            "", ".", "..", "../intro", "mise02/scene2.bin",
            "mise02\\intro", "mise02-saliery/../intro", "úvod",
        ] {
            for prefix in [
                vec!["--root", "/synthetic-installation", "--mission"],
                vec![
                    "--mission-load-probe", "--root", "/synthetic-installation",
                    "--mission",
                ],
            ] {
                let mut args = prefix;
                args.push(mission);
                assert!(parse(&args).is_err(), "accepted invalid mission {mission:?}");
            }
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
