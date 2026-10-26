//! Original-data menu and static scene inspection; mission scripts are not executed.
mod assembly;
mod driving_view;
mod menu;
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
struct Options {
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
    mission: Option<String>,
    car_inspection: bool,
    car_driving: bool,
    audio_capture: Option<PathBuf>,
    steer: Option<f32>,
    steer_steps: u32,
}
fn options() -> Result<Options, String> {
    let mut result = Options {
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
        mission: None,
        car_inspection: false,
        car_driving: false,
        audio_capture: None,
        steer: None,
        steer_steps: 1,
    };
    let mut args = std::env::args().skip(1);
    let mut steer_steps_set = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
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
            "--mission" => result.mission = Some(args.next().ok_or("Missing mission argument")?),
            "--car-inspection" => result.car_inspection = true,
            "--car-driving" => result.car_driving = true,
            "--audio-capture" => result.audio_capture = Some(args.next().ok_or("Missing audio capture output")?.into()),
            "--steer" => result.steer = Some(args.next().ok_or("Missing steering request")?.parse().map_err(|_| "Invalid steering request")?),
            "--steer-steps" => { steer_steps_set = true; result.steer_steps = args.next().ok_or("Missing steering preview step count")?.parse().map_err(|_| "Invalid steering preview step count")?; },
            "--help" => return Err(concat!(
                "Usage: lhr-engine [--synthetic | --root DIRECTORY [--model RELATIVE_4DS [--animation RELATIVE_5DS] | --mission NAME]] ",
                "[--car-driving | --car-inspection [--steer VALUE [--steer-steps N]]] [--textures RELATIVE_DIRECTORY] [--headless --capture PNG [--animation-steps N]] [--frames N] [--linear]. ",
                "--audio-capture WAV records the actual mixed driving audio and requires --car-driving. ",
                "--car-driving requires --root and --model and opens the original MISE01 driving world. W accelerates, S brakes, A/D steer, Space applies the handbrake; R reloads and Escape exits. The chase camera is a host view. ",
                "--car-inspection requires --root and --model; constructs and initializes the CPU car, then renders that same live arena's camera-selected owned LOD and source distance fade, with unlit materials. A/D previews the original wheel-steering update; arrows/W/S control the camera. R explicitly reloads the car and resets live input. Lighting, other material effects, body simulation, driving and mission execution are unavailable. ",
                "--steer accepts a finite normalized request in [-1,1]; --steer-steps 0..600 (default 1) applies that request through controlled 16 ms host inspection-clock steps before presentation. This previews actual initialized wheel frames, without translating the car or claiming a driving simulation. ",
                "Headless --animation-steps 0..600 uses controlled 16 ms inspection-clock steps; omit it for one platform-clock step. ",
                "Root alone opens the supported original Czech main-menu subset (New Game/Exit). ",
                "Menu: arrows/Enter or mouse. New Game loads static INTRO; Esc loads static MISE01, then returns to menu. ",
                "Scene inspection: arrows orbit, W/S zoom. --animation alone admits a matched single-root position track. Add --skeletal-animation for sequential skeletal slot0 mode2 playback, or --sample-animation-ms MS for one independent skeletal pose. Original profile, cutscene, mission-script and gameplay execution are not implemented. ",
                "Sampling is provisional nearest (or --linear), not verified original behavior."
            ).into()),
            _ => return Err("Unknown command option".into()),
        }
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
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return Err(
                "Mission name must contain only ASCII letters, digits and underscore".into(),
            );
        }
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
            lightmap: None,
            material: Default::default(),
            vertices: vec![
                Vertex {
                    lightmap_uv: [0.; 2],
                    color: [1.; 3],
                    position: [-1., -1., 0.],
                    uv: [0., 1.],
                },
                Vertex {
                    lightmap_uv: [0.; 2],
                    color: [1.; 3],
                    position: [1., -1., 0.],
                    uv: [1., 1.],
                },
                Vertex {
                    lightmap_uv: [0.; 2],
                    color: [1.; 3],
                    position: [1., 1., 0.],
                    uv: [1., 0.],
                },
                Vertex {
                    lightmap_uv: [0.; 2],
                    color: [1.; 3],
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
    // These are the effects rejected by strict model intake; scene intake retains
    // their metadata, but the current opaque, unlit renderer does not execute them.
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
    scene: Scene,
    camera: Camera,
    runtime: lhr_engine::runtime::RetainedScene,
    animation: Option<AnimationPlayback>,
    car: Option<lhr_engine::car_runtime::CarRuntime>,
}
fn load(options: &Options, viewport: [u32; 2]) -> Result<LoadedScene, String> {
    let Some(root) = &options.root else {
        let scene = synthetic();
        let camera = Camera::fit(&scene)?;
        return Ok(LoadedScene {
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
        );
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
    let mut animation = animation_source
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
    if let Some(AnimationPlayback::Position(playback)) = &animation {
        runtime.charge_position_binding(playback)?;
    }
    if let Some(source) = skeletal_source {
        let playback = runtime.bind_skeletal(0, source)?;
        println!("Bound original skeletal controller: slot0 mode2, {} ms; position/rotation, Target, morph and first-LOD CPU skin; host inspection initialization, camera and rendering; no mission integration", playback.duration_ms());
        animation = Some(AnimationPlayback::Skeletal(Box::new(playback)));
    }
    if matches!(animation, Some(AnimationPlayback::Position(_))) {
        println!(
            "Bound original position-only animation: slot0 mode2; one model wrapper and root mesh; native control flow, pose, frame caches and matrix/point arithmetic; host camera, handedness and rendering, no mission integration"
        );
    }
    let camera = if options.model.is_some() {
        Camera::fit(&scene)?
    } else {
        authored_camera(
            &installation,
            &scene,
            options.mission.as_deref().unwrap_or("00menu"),
            "intro_pos",
            &mut runtime,
        )?
    };
    Ok(LoadedScene {
        scene,
        camera,
        runtime,
        animation,
        car: None,
    })
}
type DrivingWorldView = ([i32; 2], u64);

fn live_draw_state(
    car: &mut Option<lhr_engine::car_runtime::CarRuntime>,
    driving: bool,
    camera: &Camera,
    viewport: [u32; 2],
) -> Result<(usize, Option<DrivingWorldView>), String> {
    let Some(owner) = car.as_mut() else {
        return Ok((0, None));
    };
    let view = lhr_engine::inspection_lod::InspectionLodView::from_camera(camera, viewport, 1.)?;
    let root = owner.model().ok_or("Car model absent")?.root();
    if driving {
        let position = owner
            .arena_mut()
            .world_matrix(root)
            .map_err(|e| e.to_string())?
            .try_transform_point([0.; 3])
            .map_err(|e| format!("Driving world position: {e:?}"))?;
        let render_root = owner.driving_scene_root()?;
        let draws = lhr_engine::car_inspection::snapshot_arena_tree_with_view(
            owner.arena_mut(),
            render_root,
            view,
        )?
        .scene
        .meshes
        .len();
        Ok((
            draws,
            Some((
                driving_view::world_bucket(position)?,
                owner.driving_world_render_revision()?,
            )),
        ))
    } else {
        Ok((
            lhr_engine::car_inspection::snapshot_car_arena_with_view(
                owner.arena_mut(),
                root,
                view,
            )?
            .scene
            .meshes
            .len(),
            None,
        ))
    }
}
fn close_car(car: &mut Option<lhr_engine::car_runtime::CarRuntime>) -> Result<(), String> {
    if let Some(owner) = car.as_mut() {
        let cleanup = owner.shutdown()?;
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
) -> Result<LoadedScene, String> {
    let opened = if driving {
        lhr_engine::car_runtime::CarRuntime::open_driving(
            installation,
            path,
            Default::default(),
            "mise01",
        )
    } else {
        lhr_engine::car_runtime::CarRuntime::open(installation, path, Default::default())
    };
    let mut owner = match opened {
        Ok(owner) => owner,
        Err(mut failure) => {
            let cleanup = failure.partial.shutdown();
            return Err(format!(
                "{}; explicit inspection cleanup: {cleanup:?}",
                failure.message
            ));
        }
    };
    let result: Result<(Scene, Camera), String> = (|| {
        let static_scene = if driving {
            Some(owner.start_driving("mise01")?)
        } else {
            None
        };
        if let Some(request) = steer {
            for _ in 0..steer_steps {
                owner.preview_wheel_steering(request, 0.016)?;
            }
            println!("Wheel steering preview: request {request}, {steer_steps} controlled 16 ms host inspection-clock steps; actual initialized wheel frames, no body movement or driving simulation");
        }
        let root = owner.model().ok_or("Car model absent")?.root();
        let camera = if driving {
            driving_view::chase_camera(owner.arena_mut(), root)?
        } else {
            lhr_engine::car_inspection::camera_from_arena_bounds(owner.arena_mut(), root)?
        };
        let view =
            lhr_engine::inspection_lod::InspectionLodView::from_camera(&camera, viewport, 1.)?;
        let snapshot = if driving {
            let render_root = owner.driving_scene_root()?;
            lhr_engine::car_inspection::snapshot_arena_tree_with_view(
                owner.arena_mut(),
                render_root,
                view,
            )?
        } else {
            lhr_engine::car_inspection::snapshot_car_arena_with_view(owner.arena_mut(), root, view)?
        };
        if driving {
            driving_view::sync_audio_listener(&mut owner, &camera)?;
        }
        println!("CPU car constructed and initialized: {} seats, {} cached animation tracks; {} visible meshes, {} draws",owner.summary().seats,owner.summary().cached_tracks,snapshot.visible_meshes,snapshot.scene.meshes.len());
        if !driving {
            println!("Car inspection: same initialized frame graph and CPU buffers; camera-selected owned LOD and distance fade, unlit texture/diffuse with source material transparency and sidedness; {} draws contain unsupported material effects. A/D previews original wheel steering; body simulation, driving and missions are unavailable.",snapshot.omitted_material_effects);
        } else {
            println!("Driving controls: W throttle, S brake, A/D steer, Space handbrake; R reload, Escape exit. Fixed 16 ms simulation, live native car frames, original static world; host chase camera and unlit rendering.");
        }
        if snapshot.unsupported_alpha_masks != 0 {
            println!(
                "Car inspection: {} draws keep unsupported non-grayscale alpha masks opaque",
                snapshot.unsupported_alpha_masks
            );
        }
        let mut scene = snapshot.scene;
        if let Some(world) = static_scene {
            scene.meshes.extend(world.meshes);
        }
        Ok((scene, camera))
    })();
    match result {
        Ok((scene, camera)) => Ok(LoadedScene {
            scene,
            camera,
            runtime: Default::default(),
            animation: None,
            car: Some(owner),
        }),
        Err(error) => {
            let cleanup = owner.shutdown();
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
    mode: AppMode,
    ui: Option<menu::MenuUi>,
    pointer: [f32; 2],
    adapter: Option<wgpu::Adapter>,
    scene: Scene,
    camera: Camera,
    runtime: lhr_engine::runtime::RetainedScene,
    animation: Option<AnimationPlayback>,
    car: Option<lhr_engine::car_runtime::CarRuntime>,
    runtime_engine: lhr_engine::runtime::RuntimeEngine,
    clock_backend: lhr_engine::runtime::PlatformClock,
    options: Options,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    renderer: Option<Renderer>,
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
        self.options.mission = match mode {
            AppMode::IntroScene => Some("intro".into()),
            AppMode::FirstMissionScene => Some("mise01".into()),
            _ => None,
        };
        let viewport = self
            .config
            .as_ref()
            .map_or(INITIAL_CAR_VIEWPORT, |c| [c.width, c.height]);
        let LoadedScene {
            mut scene,
            camera,
            runtime,
            animation,
            mut car,
        } = load(&self.options, viewport)?;
        let (car_draws, world_bucket) =
            match live_draw_state(&mut car, self.options.car_driving, &camera, viewport) {
                Ok(draws) => draws,
                Err(error) => {
                    let cleanup = close_car(&mut car);
                    return Err(format!("{error}; new car cleanup: {cleanup:?}"));
                }
            };
        if mode == AppMode::Menu {
            scene
                .meshes
                .push(menu::quad(self.ui.as_ref().unwrap().pixels()));
        }
        let result = pollster::block_on(Renderer::with_adapter(
            self.adapter.as_ref().ok_or("Graphics adapter absent")?,
            &scene,
            self.config.as_ref().unwrap().format,
            self.options.linear,
        ));
        let mut renderer = match result {
            Ok(renderer) => renderer,
            Err(error) => {
                let cleanup = close_car(&mut car);
                return Err(format!("{error}; car cleanup: {cleanup:?}"));
            }
        };
        renderer
            .set_inspection_alpha_cutout(self.options.car_inspection || self.options.car_driving);
        if let Some(old) = &self.renderer {
            old.device.poll(wgpu::Maintain::Wait);
        }
        if let Err(error) = close_car(&mut self.car) {
            renderer.device.poll(wgpu::Maintain::Wait);
            let cleanup = close_car(&mut car);
            return Err(format!("{error}; new car cleanup: {cleanup:?}"));
        }
        self.surface
            .as_ref()
            .unwrap()
            .configure(&renderer.device, self.config.as_ref().unwrap());
        self.renderer = Some(renderer);
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
        let controls = driving_view::Controls::from_keys(&self.keys);
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
            self.camera = driving_view::chase_camera(owner.arena_mut(), root)?;
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
        let view =
            lhr_engine::inspection_lod::InspectionLodView::from_camera(&self.camera, viewport, 1.)?;
        let root = if self.options.car_driving {
            owner.driving_scene_root()?
        } else {
            owner.model().ok_or("Car model absent")?.root()
        };
        let snapshot = if self.options.car_driving {
            lhr_engine::car_inspection::snapshot_arena_tree_with_view(
                owner.arena_mut(),
                root,
                view,
            )?
        } else {
            lhr_engine::car_inspection::snapshot_car_arena_with_view(owner.arena_mut(), root, view)?
        };
        let renderer = self.renderer.as_mut().ok_or("Car renderer absent")?;
        driving_view::refresh_car_draws(
            renderer,
            &mut self.scene,
            &mut self.car_draws,
            snapshot.scene.meshes,
        )?;
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
        let Some(config) =
            surface.get_default_config(&adapter, size.width.max(1), size.height.max(1))
        else {
            self.fail(event_loop, "No presentation configuration");
            return;
        };
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
        renderer
            .set_inspection_alpha_cutout(self.options.car_inspection || self.options.car_driving);
        surface.configure(&renderer.device, &config);
        self.adapter = Some(adapter);
        self.window = Some(window);
        self.surface = Some(surface);
        self.renderer = Some(renderer);
        self.config = Some(config);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Pressed && !event.repeat {
                        if self.mode == AppMode::Menu {
                            match code {
                                KeyCode::ArrowDown | KeyCode::ArrowUp => {
                                    self.ui.as_mut().unwrap().move_selection(
                                        if code == KeyCode::ArrowDown { 1 } else { -1 },
                                    );
                                    println!(
                                        "Menu focus: {}",
                                        self.ui.as_ref().unwrap().focus_id()
                                    );
                                    if let Err(error) = self.refresh_menu() {
                                        self.fail(event_loop, &error);
                                    }
                                }
                                KeyCode::Enter | KeyCode::NumpadEnter => self.activate(event_loop),
                                KeyCode::Escape => event_loop.exit(),
                                _ => {}
                            }
                        } else if code == KeyCode::KeyR
                            && (self.options.car_inspection || self.options.car_driving)
                        {
                            if let Err(error) = self.reload_car() {
                                self.fail(event_loop, &error);
                            }
                        } else if code == KeyCode::Escape {
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
            WindowEvent::CursorMoved { position, .. } if self.mode == AppMode::Menu => {
                let size = self.window.as_ref().unwrap().inner_size();
                self.pointer = menu::design_position(
                    [position.x as f32, position.y as f32],
                    [size.width, size.height],
                );
                if self.ui.as_mut().unwrap().point(self.pointer) {
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
                if self.ui.as_ref().unwrap().hit(self.pointer) {
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
                let step = match advance_bound(
                    &mut self.runtime_engine,
                    &mut self.runtime,
                    &mut self.animation,
                    &mut self.clock_backend,
                ) {
                    Ok(step) => step,
                    Err(error) => {
                        self.fail(event_loop, &format!("Bound runtime failed: {error:?}"));
                        return;
                    }
                };
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
                if let Err(error) = self.refresh_car_presentation() {
                    self.fail(event_loop, &error);
                    return;
                }
                if let (Some(surface), Some(renderer), Some(config)) =
                    (&self.surface, &self.renderer, &self.config)
                {
                    match surface.get_current_texture() {
                        Ok(frame) => {
                            let view = frame.texture.create_view(&Default::default());
                            let encoder = if self.mode == AppMode::Menu {
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
                            renderer.queue.submit(Some(encoder.finish()));
                            frame.present();
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
                if self.options.frames > 0 && self.count >= self.options.frames {
                    if let Some(path) = &self.options.capture {
                        let renderer = self.renderer.as_ref().unwrap();
                        let size = self.window.as_ref().unwrap().inner_size();
                        let result = if self.mode == AppMode::Menu {
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
    let LoadedScene {
        mut scene,
        camera,
        mut runtime,
        mut animation,
        mut car,
    } = load(&options, INITIAL_CAR_VIEWPORT)?;
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
        if car.is_some() {
            let result = (|| {
                let instance = lhr_render::instance();
                let mut renderer = pollster::block_on(Renderer::new(
                    &instance,
                    None,
                    &scene,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    options.linear,
                ))?;
                renderer.set_inspection_alpha_cutout(true);
                let result = renderer.capture(
                    INITIAL_CAR_VIEWPORT[0],
                    INITIAL_CAR_VIEWPORT[1],
                    &camera,
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
        let renderer = pollster::block_on(Renderer::new(
            &instance,
            None,
            &scene,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            options.linear,
        ))?;
        upload_bound(&mut runtime, &mut animation, &renderer)?;
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
        return Ok(());
    }
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(_) => {
            let cleanup = close_car(&mut car);
            return Err(format!(
                "Cannot initialize window event loop; car cleanup: {cleanup:?}"
            ));
        }
    };
    let (car_draws, world_bucket) =
        match live_draw_state(&mut car, options.car_driving, &camera, INITIAL_CAR_VIEWPORT) {
            Ok(draws) => draws,
            Err(error) => {
                let cleanup = close_car(&mut car);
                return Err(format!("{error}; car cleanup: {cleanup:?}"));
            }
        };
    let audio_capture = match options.audio_capture.as_ref() {
        Some(path) => match lhr_engine::runtime_audio::StereoWavCapture::create(path, 48_000) {
            Ok(capture) => Some(capture),
            Err(error) => {
                let cleanup = close_car(&mut car);
                return Err(format!("{error}; car cleanup: {cleanup:?}"));
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
    let mut app = App {
        mode,
        ui,
        pointer: [-1.0; 2],
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
