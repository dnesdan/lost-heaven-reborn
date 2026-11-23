//! A one-shot window host for the original-data Freeride startup diagnostic.
#![forbid(unsafe_code)]

use lhr_engine::{
    game_init_order::PresentedFrame,
    load_scene_callback::LoadingScreenFrame,
    loading_surface::{LoadingSurface, LoadingSwapchain},
    menu::{MenuComponentCreateContext, MenuComponentPrecision},
    menu_native_drawing::MenuNativeDrawState,
    menu_text_drawing::MenuTextPrecision,
    scene_render::CanonicalSceneSnapshot,
};
use lhr_render::{Renderer, Scene};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

pub(super) struct FreerideWindow {
    window: Arc<Window>,
    swapchain: LoadingSwapchain<'static>,
    renderer: RefCell<Renderer>,
    config: wgpu::SurfaceConfiguration,
    globals: MenuNativeDrawState,
    draw_count: Cell<usize>,
}

pub(super) fn parse_pc24_layout(raw: &str) -> Result<MenuComponentCreateContext, String> {
    let mut fields = raw.split(',');
    let mut next = || {
        fields
            .next()
            .ok_or("Window layout requires SCALE_X,SCALE_Y,LEFT_MARGIN")?
            .parse::<f32>()
            .map_err(|_| "Invalid window layout number")
    };
    let screen_scale = [next()?, next()?];
    let left_margin_x = next()?;
    if fields.next().is_some() {
        return Err("Window layout requires exactly SCALE_X,SCALE_Y,LEFT_MARGIN".into());
    }
    if !screen_scale
        .into_iter()
        .all(|value| value.is_finite() && value > 0.0)
        || !left_margin_x.is_finite()
    {
        return Err(
            "Window layout requires positive finite scales and a finite left margin".into(),
        );
    }
    Ok(MenuComponentCreateContext {
        screen_scale,
        left_margin_x,
        precision: MenuComponentPrecision::Pc24Nearest,
    })
}

impl FreerideWindow {
    fn new(event_loop: &ActiveEventLoop) -> Result<Self, String> {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Freeride startup diagnostic")
                        .with_inner_size(PhysicalSize::new(800, 600)),
                )
                .map_err(|error| format!("Freeride window: {error}"))?,
        );
        let instance = lhr_render::instance();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("Freeride surface: {error}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .ok_or("Freeride window has no surface-compatible graphics adapter")?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("Freeride window has no presentation configuration")?;
        let format = config.format.remove_srgb_suffix();
        if !surface.get_capabilities(&adapter).formats.contains(&format) {
            return Err("Canonical Scene requires an unorm presentation target".into());
        }
        config.format = format;
        let renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &Scene { meshes: Vec::new() },
            format,
            false,
        ))?;
        let swapchain = LoadingSwapchain::new(surface, &renderer.device);
        swapchain.configure(&config)?;
        Ok(Self {
            window,
            swapchain,
            renderer: RefCell::new(renderer),
            config,
            globals: MenuNativeDrawState::new(0, None, [0; 2]),
            draw_count: Cell::new(0),
        })
    }

    pub(super) fn target(&self) -> [u32; 2] {
        [self.config.width, self.config.height]
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), String> {
        self.config.width = size.width;
        self.config.height = size.height;
        self.swapchain.configure(&self.config)
    }

    fn loading<'host>(&'host self, renderer: &'host Renderer) -> LoadingSurface<'host, 'static> {
        LoadingSurface::new(
            &self.window,
            &self.swapchain,
            renderer,
            &self.globals,
            MenuComponentPrecision::Pc24Nearest,
            MenuTextPrecision::Pc24Nearest,
        )
    }

    pub(super) fn present_loading(&self, frame: LoadingScreenFrame<'_>) -> Result<(), String> {
        self.loading(&self.renderer.borrow()).present(frame, None)
    }

    pub(super) fn present_init(&self, frame: PresentedFrame<'_>) -> Result<(), String> {
        if let PresentedFrame::Scene(snapshot) = &frame {
            self.upload_scene(snapshot)?;
        }
        let renderer = self.renderer.borrow();
        self.loading(&renderer)
            .present_init(frame, None, |snapshot| {
                let [width, height] = self.target();
                let scheduled = self.swapchain.present_redraw(
                    &self.window,
                    &renderer,
                    &self.config,
                    |texture| {
                        let view = texture.create_view(&Default::default());
                        let encoder =
                            renderer.render_scene_frame(&view, width, height, &snapshot.frame)?;
                        Ok(vec![encoder.finish()])
                    },
                )?;
                if scheduled {
                    Ok(())
                } else {
                    Err("Freeride Init scene frame was not scheduled; no replay".into())
                }
            })
    }

    fn upload_scene(&self, snapshot: &CanonicalSceneSnapshot) -> Result<(), String> {
        let mesh_count = snapshot.scene.meshes.len();
        if snapshot.lighting.len() != mesh_count {
            return Err("Freeride Init scene has inconsistent mesh lighting".into());
        }
        let mut renderer = self.renderer.borrow_mut();
        renderer.replace_meshes(0..self.draw_count.get(), &snapshot.scene.meshes)?;
        self.draw_count.set(mesh_count);
        for (index, lighting) in snapshot.lighting.iter().enumerate() {
            if let Some(lighting) = lighting {
                renderer.set_mesh_lighting(index, lighting)?;
            } else {
                renderer.clear_mesh_lighting(index)?;
            }
        }
        Ok(())
    }
}

pub(super) fn run(run: impl FnOnce(&FreerideWindow) -> Result<(), String>) -> Result<(), String> {
    struct Startup<Run> {
        run: Option<Run>,
        host: Option<FreerideWindow>,
        result: Option<Result<(), String>>,
    }

    impl<Run: FnOnce(&FreerideWindow) -> Result<(), String>> ApplicationHandler for Startup<Run> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.host.is_some() || self.result.is_some() {
                return;
            }
            match FreerideWindow::new(event_loop) {
                Ok(host) => {
                    host.window.request_redraw();
                    self.host = Some(host);
                }
                Err(error) => {
                    self.result = Some(Err(error));
                    event_loop.exit();
                }
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window: WindowId,
            event: WindowEvent,
        ) {
            let Some(host) = self.host.as_mut().filter(|host| host.window.id() == window) else {
                return;
            };
            if self.result.is_some() {
                return;
            }
            let result = match event {
                WindowEvent::Resized(size) if size.width != 0 && size.height != 0 => {
                    match host.resize(size) {
                        Ok(()) => {
                            host.window.request_redraw();
                            return;
                        }
                        Err(error) => Err(error),
                    }
                }
                WindowEvent::RedrawRequested
                    if host.window.inner_size().width != 0
                        && host.window.inner_size().height != 0 =>
                {
                    match self.run.take() {
                        Some(run) => run(host),
                        None => return,
                    }
                }
                WindowEvent::CloseRequested => Err("Freeride window closed before startup".into()),
                _ => return,
            };
            self.result = Some(result);
            event_loop.exit();
        }
    }

    let event_loop = EventLoop::new().map_err(|error| format!("Freeride event loop: {error}"))?;
    let mut startup = Startup {
        run: Some(run),
        host: None,
        result: None,
    };
    event_loop
        .run_app(&mut startup)
        .map_err(|error| format!("Freeride event loop: {error}"))?;
    startup
        .result
        .unwrap_or_else(|| Err("Freeride window ended before startup".into()))
}
