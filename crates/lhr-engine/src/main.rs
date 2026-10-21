//! Original-data menu and static scene inspection; mission scripts are not executed.
mod assembly;
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
struct Options {
    root: Option<PathBuf>,
    model: Option<String>,
    capture: Option<PathBuf>,
    headless: bool,
    frames: u32,
    linear: bool,
    textures: String,
    synthetic: bool,
    mission: Option<String>,
}
fn options() -> Result<Options, String> {
    let mut result = Options {
        root: None,
        model: None,
        capture: None,
        headless: false,
        frames: 0,
        linear: false,
        textures: "maps".into(),
        synthetic: false,
        mission: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--textures" => result.textures = args.next().ok_or("Missing textures argument")?,
            "--root" => result.root = Some(args.next().ok_or("Missing root argument")?.into()),
            "--model" => result.model = Some(args.next().ok_or("Missing model argument")?),
            "--capture" => result.capture = Some(args.next().ok_or("Missing capture argument")?.into()),
            "--headless" => result.headless = true,
            "--linear" => result.linear = true,
            "--frames" => result.frames = args.next().ok_or("Missing frame count")?.parse().map_err(|_| "Invalid frame count")?,
            "--synthetic" => result.synthetic = true,
            "--mission" => result.mission = Some(args.next().ok_or("Missing mission argument")?),
            "--help" => return Err(concat!(
                "Usage: lhr-engine [--synthetic | --root DIRECTORY [--model RELATIVE_4DS | --mission NAME]] ",
                "[--textures RELATIVE_DIRECTORY] [--headless --capture PNG] [--frames N] [--linear]. ",
                "Root alone opens the supported original Czech main-menu subset (New Game/Exit). ",
                "Menu: arrows/Enter or mouse. New Game loads static INTRO; Esc loads static MISE01, then returns to menu. ",
                "Scene inspection: arrows orbit, W/S zoom. Original profile, cutscene, mission-script and gameplay execution are not implemented. ",
                "Sampling is provisional nearest (or --linear), not verified original behavior."
            ).into()),
            _ => return Err("Unknown command option".into()),
        }
    }
    if result.model.is_some() && result.root.is_none() {
        return Err("A model requires an installation root".into());
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
fn engine_position(transform: Mat4, position: [f32; 3]) -> Result<[f32; 3], String> {
    let p = transform.transform_point3(Vec3::from_array(position));
    if !p.is_finite() {
        return Err("Model vertex transform overflow".into());
    }
    Ok([p.x, p.y, -p.z])
}
fn material_effects_omitted(material: &lhr_assets::Material) -> bool {
    // These are the effects rejected by strict model intake; scene intake retains
    // their metadata, but the current opaque, unlit renderer does not execute them.
    const UNSUPPORTED_EFFECT_FLAGS: u32 =
        0x8000 | 0x80000 | 0x01000000 | 0x04000000 | 0x20000000 | 0x40000000 | 0x80000000;
    material.flags & UNSUPPORTED_EFFECT_FLAGS != 0
        || material.alpha_texture.is_some()
        || material.environment_texture.is_some()
        || material.emission != [0.0; 3]
        || material.opacity != 1.0
        || material.animation.is_some()
}
struct LoadedScene {
    scene: Scene,
    camera: Camera,
}
fn load(options: &Options) -> Result<LoadedScene, String> {
    let Some(root) = &options.root else {
        let scene = synthetic();
        let camera = Camera::fit(&scene)?;
        return Ok(LoadedScene { scene, camera });
    };
    let installation = lhr_assets::Installation::open(root)
        .map_err(|e| format!("Cannot open asset installation: {e}"))?;
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
    let model = if options.model.is_some() {
        lhr_assets::parse_four_ds(&data)
    } else {
        lhr_assets::parse_four_ds_scene(&data)
    }
    .map_err(|e| format!("Selected geometry is unsupported or malformed: {e}"))?;
    if options.model.is_none() {
        let omitted = model
            .materials
            .iter()
            .filter(|material| material_effects_omitted(material))
            .count();
        println!("Static scene subset: {} frames; {omitted} materials have effects not rendered; animation, lighting and mission scripts are not executed", model.frames.len());
    }
    let mut builder = assembly::SceneBuilder::new(&installation, &options.textures);
    builder.append(&model, Mat4::IDENTITY, None, options.model.is_some(), None)?;
    let scene = builder.finish()?;
    let camera = if options.model.is_some() {
        Camera::fit(&scene)?
    } else {
        authored_camera(
            &installation,
            &scene,
            options.mission.as_deref().unwrap_or("00menu"),
            "intro_pos",
        )?
    };
    Ok(LoadedScene { scene, camera })
}
fn authored_camera(
    installation: &lhr_assets::Installation,
    scene: &Scene,
    mission: &str,
    name: &str,
) -> Result<Camera, String> {
    let mut camera = Camera::fit(scene)?;
    let bytes = installation
        .read(&format!("missions/{mission}/scene2.bin"))
        .map_err(|e| e.to_string())?;
    let metadata = lhr_assets::parse_scene2(&bytes).map_err(|e| e.to_string())?;
    println!("Scene metadata: {} objects, {} actors, {} parsed script programs; actors and mission scripts are not executed",
        metadata.objects.len(), metadata.actors.len(), metadata.scripts.len());
    if let Some(object) = metadata
        .objects
        .iter()
        .find(|o| o.name.eq_ignore_ascii_case(name))
    {
        if let (Some(p), Some(q)) = (object.position, object.rotation_wxyz) {
            let rotation = source_rotation(q)?;
            let forward = rotation * Vec3::Z;
            camera.eye = Vec3::new(p[0], p[1], -p[2]);
            camera.target = camera.eye + Vec3::new(forward.x, forward.y, -forward.z);
            camera.up = Vec3::Y;
            println!("Authored camera: {name}; provisional 50-degree field of view");
        }
    } else {
        println!("Authored camera {name} unavailable; fitted inspection camera");
    }
    camera.validate()?;
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
    options: Options,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    renderer: Option<Renderer>,
    config: Option<wgpu::SurfaceConfiguration>,
    keys: HashSet<KeyCode>,
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
        let LoadedScene { mut scene, camera } = load(&self.options)?;
        if mode == AppMode::Menu {
            scene
                .meshes
                .push(menu::quad(self.ui.as_ref().unwrap().pixels()));
        }
        let renderer = pollster::block_on(Renderer::with_adapter(
            self.adapter.as_ref().ok_or("Graphics adapter absent")?,
            &scene,
            self.config.as_ref().unwrap().format,
            self.options.linear,
        ))?;
        self.surface
            .as_ref()
            .unwrap()
            .configure(&renderer.device, self.config.as_ref().unwrap());
        self.renderer = Some(renderer);
        self.scene = scene;
        self.camera = camera;
        self.mode = mode;
        self.keys.clear();
        println!("Application state: {mode:?}; scene assets loaded; gameplay and cutscene execution unsupported");
        Ok(())
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, message: &str) {
        self.error = Some(message.into());
        event_loop.exit();
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let Ok(window) = event_loop.create_window(
            Window::default_attributes()
                .with_title("Lost Heaven Reborn — original menu / static scene inspection")
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
        let renderer = match pollster::block_on(Renderer::with_adapter(
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
                let offset = self.camera.eye - self.camera.target;
                let mut yaw = 0.0;
                let mut pitch = 0.0;
                let mut zoom = 1.0;
                for key in &self.keys {
                    match key {
                        KeyCode::ArrowLeft => yaw += 0.025,
                        KeyCode::ArrowRight => yaw -= 0.025,
                        KeyCode::ArrowUp => pitch += 0.025,
                        KeyCode::ArrowDown => pitch -= 0.025,
                        KeyCode::KeyW => zoom *= 0.98,
                        KeyCode::KeyS => zoom *= 1.02,
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
fn run() -> Result<(), String> {
    let options = options()?;
    let LoadedScene { mut scene, camera } = load(&options)?;
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
        println!("Menu subset: New Game and Exit; original profile chooser and other actions are not implemented");
    }
    println!(
        "Original-data menu/static scene inspection; provisional {} texture sampling; {} indexed mesh groups",
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
    if options.headless {
        let instance = lhr_render::instance();
        let renderer = pollster::block_on(Renderer::new(
            &instance,
            None,
            &scene,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            options.linear,
        ))?;
        if mode == AppMode::Menu {
            renderer.capture_with_overlay(
                640,
                480,
                &camera,
                menu::DESIGN,
                options.capture.as_deref().unwrap(),
            )?;
        } else {
            renderer.capture(960, 640, &camera, options.capture.as_deref().unwrap())?;
        }
        println!("GPU readback capture complete");
        return Ok(());
    }
    let event_loop = EventLoop::new().map_err(|_| "Cannot initialize window event loop")?;
    let mut app = App {
        mode,
        ui,
        pointer: [-1.0; 2],
        adapter: None,
        scene,
        camera,
        options,
        window: None,
        surface: None,
        renderer: None,
        config: None,
        keys: HashSet::new(),
        count: 0,
        error: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|_| "Window event loop failed")?;
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
            0x8000, 0x80000, 0x01000000, 0x04000000, 0x20000000, 0x40000000, 0x80000000,
        ] {
            material.flags = flag;
            assert!(material_effects_omitted(&material), "flag {flag:08x}");
        }
        material.flags = 0x30000; // Supported clamp flags are not omitted effects.
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
        assert!(engine_position(transform, [2.0, 0.0, 0.0]).is_err());
        assert_eq!(
            engine_position(Mat4::IDENTITY, [1.0, 2.0, 3.0]).unwrap(),
            [1.0, 2.0, -3.0]
        );
    }
}
