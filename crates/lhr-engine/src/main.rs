//! Camera inspection executable; this does not implement original gameplay.
use glam::{Mat4, Quat, Vec3};
use lhr_render::{Camera, Mesh, Renderer, Scene, Texture, Vertex};
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
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
            "--help" => return Err(concat!(
                "Usage: lhr-engine [--synthetic | --root DIRECTORY --model RELATIVE_4DS] ",
                "[--textures RELATIVE_DIRECTORY] [--headless --capture PNG] [--frames N] [--linear]. ",
                "Camera inspection: arrows orbit, W/S zoom, Esc exits. ",
                "Sampling is provisional nearest (or --linear), not verified original behavior."
            ).into()),
            _ => return Err("Unknown command option".into()),
        }
    }
    if result.root.is_some() != result.model.is_some() {
        return Err("Root and model must be provided together".into());
    }
    if result.headless && result.frames != 0 {
        return Err("Headless mode captures exactly one frame; omit --frames".into());
    }
    if result.headless && result.capture.is_none() {
        return Err("Headless mode requires a capture output".into());
    }
    if result.synthetic == result.root.is_some() {
        return Err("Select exactly one of --synthetic or --root with --model".into());
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
            texture: Some(Texture {
                width: 2,
                height: 2,
                repeat: [true, true],
                rgba: vec![
                    255, 32, 32, 255, 32, 255, 32, 255, 32, 32, 255, 255, 255, 255, 32, 255,
                ],
            }),
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
fn load(options: &Options) -> Result<Scene, String> {
    let Some(root) = &options.root else {
        return Ok(synthetic());
    };
    let installation =
        lhr_assets::Installation::open(root).map_err(|_| "Cannot open asset installation")?;
    let data = installation
        .read(options.model.as_deref().unwrap())
        .map_err(|_| "Cannot read selected model")?;
    let model = lhr_assets::parse_four_ds(&data)
        .map_err(|_| "Selected model is unsupported or malformed")?;
    let mut transforms = Vec::new();
    let mut meshes = Vec::new();
    let mut scene_bytes = 0usize;
    const MAX_SCENE_BYTES: usize = 256 * 1024 * 1024;
    for frame in &model.frames {
        let rotation = source_rotation(frame.rotation_wxyz)?;
        let local = Mat4::from_scale_rotation_translation(
            Vec3::from_array(frame.scale),
            rotation,
            Vec3::from_array(frame.position),
        );
        let transform = if let Some(parent) = frame.parent {
            *transforms
                .get(parent)
                .ok_or("Model parent ordering is unsupported")?
                * local
        } else {
            local
        };
        if !transform.is_finite() {
            return Err("Model transform overflow".into());
        }
        transforms.push(transform);
        let geometry = if let Some(index) = frame.instance {
            model.frames.get(index).ok_or("Invalid mesh instance")?
        } else {
            frame
        };
        let Some(lod) = geometry.lods.first() else {
            continue;
        };
        for group in &lod.groups {
            let geometry_bytes = lod
                .vertices
                .len()
                .checked_mul(std::mem::size_of::<Vertex>())
                .and_then(|n| {
                    group
                        .indices
                        .len()
                        .checked_mul(4)
                        .and_then(|i| n.checked_add(i))
                })
                .ok_or("Scene size overflow")?;
            if meshes.len() >= 4096 {
                return Err("Scene exceeds draw quota".into());
            }
            charge_scene(&mut scene_bytes, geometry_bytes, MAX_SCENE_BYTES)?;

            let material = model
                .materials
                .get(group.material)
                .ok_or("Invalid model material reference")?;
            let texture = if material.texture.is_empty() {
                let mut rgba: Vec<u8> = material
                    .diffuse
                    .iter()
                    .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    .collect();
                rgba.push(255);
                Some(Texture {
                    width: 1,
                    height: 1,
                    repeat: [false, false],
                    rgba,
                })
            } else {
                let name = format!("{}/{}", options.textures, material.texture);
                let bytes = installation
                    .read(&name)
                    .map_err(|_| "A model texture is unavailable")?;
                let width = bytes
                    .get(18..22)
                    .and_then(|b| b.try_into().ok())
                    .map(i32::from_le_bytes)
                    .filter(|&n| n > 0)
                    .ok_or("Invalid BMP width")? as usize;
                let height = bytes
                    .get(22..26)
                    .and_then(|b| b.try_into().ok())
                    .map(i32::from_le_bytes)
                    .filter(|&n| n != 0 && n != i32::MIN)
                    .ok_or("Invalid BMP height")?
                    .unsigned_abs() as usize;
                let texture_bytes = width
                    .checked_mul(height)
                    .and_then(|n| n.checked_mul(4))
                    .ok_or("Texture size overflow")?;
                charge_scene(&mut scene_bytes, texture_bytes, MAX_SCENE_BYTES)?;
                let decoded = lhr_assets::decode_bmp(&bytes)
                    .map_err(|_| "A model texture is unsupported or malformed")?;
                Some(Texture {
                    width: decoded.width,
                    height: decoded.height,
                    rgba: decoded.rgba,
                    repeat: [material.flags & 0x10000 == 0, material.flags & 0x20000 == 0],
                })
            };
            // Explicit source-to-engine conversion: preserve X/Y, reflect Z. Reverse
            // each triangle to retain its orientation after the handedness change.
            let vertices = lod
                .vertices
                .iter()
                .map(|v| {
                    Ok(Vertex {
                        position: engine_position(transform, v.position)?,
                        uv: v.uv,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let indices = group
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|t| [t[0], t[2], t[1]])
                .collect();
            meshes.push(Mesh {
                vertices,
                indices,
                texture,
            });
        }
    }
    if meshes.is_empty() {
        return Err("Selected model contains no supported visible geometry".into());
    }
    Ok(Scene { meshes })
}
struct App {
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
                .with_title("Lost Heaven Reborn — camera inspection")
                .with_inner_size(winit::dpi::LogicalSize::new(960, 640)),
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
                    if code == KeyCode::Escape {
                        event_loop.exit();
                    }
                    if event.state == ElementState::Pressed {
                        self.keys.insert(code);
                    } else {
                        self.keys.remove(&code);
                    }
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
                            let encoder = renderer.render(
                                &frame.texture.create_view(&Default::default()),
                                config.width,
                                config.height,
                                &self.camera,
                            );
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
                        if self
                            .renderer
                            .as_ref()
                            .unwrap()
                            .capture(960, 640, &self.camera, path)
                            .is_err()
                        {
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
    let scene = load(&options)?;
    let camera = Camera::fit(&scene)?;
    println!(
        "Camera inspection; provisional {} texture sampling; {} indexed mesh groups",
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
        renderer.capture(960, 640, &camera, options.capture.as_deref().unwrap())?;
        println!("GPU readback capture complete");
        return Ok(());
    }
    let event_loop = EventLoop::new().map_err(|_| "Cannot initialize window event loop")?;
    let mut app = App {
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
