use super::LoadingSwapchain;
use lhr_render::{Renderer, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::{Window, WindowId};

pub(crate) fn with_swapchain(
    test: impl FnOnce(&Window, &LoadingSwapchain<'_>, &Renderer, &wgpu::SurfaceConfiguration),
) {
    struct App<Test> {
        test: Option<Test>,
    }
    impl<Test> ApplicationHandler for App<Test>
    where
        Test: FnOnce(&Window, &LoadingSwapchain<'_>, &Renderer, &wgpu::SurfaceConfiguration),
    {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let test = self.test.take().unwrap();
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Loading swapchain test")
                        .with_inner_size(PhysicalSize::new(320, 240)),
                )
                .unwrap();
            let size = window.inner_size();
            let instance = lhr_render::instance();
            let surface = instance.create_surface(&window).unwrap();
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }))
                .unwrap_or_else(|| {
                    let adapters: Vec<_> = instance
                        .enumerate_adapters(wgpu::Backends::VULKAN | wgpu::Backends::METAL)
                        .into_iter()
                        .map(|adapter| (adapter.get_info(), surface.get_capabilities(&adapter)))
                        .collect();
                    panic!("swapchain test requires a window-compatible GPU: {adapters:?}");
                });
            let config = surface
                .get_default_config(&adapter, size.width, size.height)
                .unwrap();
            println!(
                "loading swapchain adapter: {:?}, target: {:?}, size: {size:?}",
                adapter.get_info(),
                config.format
            );
            let renderer = pollster::block_on(Renderer::with_adapter(
                &adapter,
                &Scene { meshes: vec![] },
                config.format,
                false,
            ))
            .unwrap();
            renderer
                .device
                .push_error_scope(wgpu::ErrorFilter::Validation);
            let swapchain = LoadingSwapchain::new(surface, &renderer.device);
            swapchain.configure(&config).unwrap();
            test(&window, &swapchain, &renderer, &config);
            renderer.device.poll(wgpu::Maintain::Wait);
            if let Some(error) = pollster::block_on(renderer.device.pop_error_scope()) {
                panic!("loading swapchain GPU validation: {error}");
            }
            event_loop.exit();
        }

        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    let mut app = App { test: Some(test) };
    event_loop.run_app(&mut app).unwrap();
    assert!(app.test.is_none());
}
