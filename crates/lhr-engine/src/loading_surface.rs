//! Host presentation for loading frames on an existing winit/wgpu swapchain.
#![forbid(unsafe_code)]

use crate::game_init_order::PresentedFrame;
use crate::load_scene_callback::{
    LoadingScreenDevice, LoadingScreenFrame, LoadingScreenPresentBackend,
};
use crate::menu::MenuComponentPrecision;
use crate::menu_native_drawing::{MenuNativeDrawState, MenuNativeViewport};
use crate::menu_text_drawing::MenuTextPrecision;
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::SceneKey;
use crate::scene_projection::SceneRenderBackend;
use crate::scene_render::CanonicalSceneSnapshot;
use lhr_render::{Mesh, OrderedScreenBatch, Renderer};
use std::cell::Cell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use winit::window::Window;

/// Supply the reached cursor mesh in loading-frame pixel coordinates.
pub type LoadingCursor<'host> = dyn FnMut(f32, f32) -> Result<Mesh, String> + 'host;

/// Owns the surface and its recovery state across loading and ordinary draws.
/// The host must configure it on the renderer's device before presenting.
pub struct LoadingSwapchain<'window> {
    owner: SwapchainOwner<wgpu::Surface<'window>>,
    device: wgpu::Device,
}

impl<'window> LoadingSwapchain<'window> {
    pub fn new(surface: wgpu::Surface<'window>, device: &wgpu::Device) -> Self {
        Self {
            owner: SwapchainOwner::new(surface),
            device: device.clone(),
        }
    }

    /// Configure before first use or explicitly recover after an aborted frame.
    /// No acquire is allowed until configuration succeeds. Use the renderer's
    /// format and serialize other error-scope use on this device with this call.
    /// If a backend operation panics and leaves ownership uncertain, this call
    /// refuses recovery. The surface is leaked until process exit rather than
    /// running a teardown that could panic; the host must restart the process.
    pub fn configure(&self, config: &wgpu::SurfaceConfiguration) -> Result<(), String> {
        self.configure_using(&self.device, config)
    }

    /// Replace the configured device only after its configuration succeeds.
    pub fn configure_for_device(
        &mut self,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Result<(), String> {
        self.configure_using(device, config)?;
        self.device = device.clone();
        Ok(())
    }

    pub fn supports_format(&self, adapter: &wgpu::Adapter, format: wgpu::TextureFormat) -> bool {
        self.owner
            .surface()
            .get_capabilities(adapter)
            .formats
            .contains(&format)
    }

    fn configure_using(
        &self,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Result<(), String> {
        self.owner.state.configure(|| {
            run_with_cleanup(ConfigureErrorScopes::new(device), |scopes| {
                scopes.push();
                self.owner.surface().configure(device, config);
                let mut error = None;
                while scopes.remaining != 0 {
                    error = error.or(scopes.pop());
                }
                match error {
                    Some(error) => Err(format!("configure loading swapchain: {error}")),
                    None => Ok(()),
                }
            })
        })
    }

    fn require_device(&self, device: &wgpu::Device) -> Result<(), String> {
        if *device != self.device {
            return Err("loading renderer and swapchain must use the same device".into());
        }
        Ok(())
    }

    /// Encode one ordinary redraw on the same surface as loading frames.
    /// Returns false on a timeout or a successful Lost/Outdated reconfigure;
    /// neither case calls the encoder or retries that frame. An earlier
    /// aborted frame still requires an explicit successful configuration.
    pub fn present_redraw(
        &self,
        window: &Window,
        renderer: &Renderer,
        config: &wgpu::SurfaceConfiguration,
        encode: impl FnOnce(&wgpu::Texture) -> Result<Vec<wgpu::CommandBuffer>, String>,
    ) -> Result<bool, String> {
        self.require_device(&renderer.device)?;
        run_with_cleanup(None, |frame| {
            *frame = self.owner.state.acquire_for_redraw(
                || self.owner.surface().get_current_texture(),
                || self.configure(config),
            )?;
            let Some(acquired) = frame.as_ref() else {
                return Ok(false);
            };
            let commands = encode(&acquired.frame.texture)?;
            submit_and_present(frame, &renderer.queue, window, commands)?;
            Ok(true)
        })
    }
}

struct SwapchainOwner<Surface> {
    surface: Option<Surface>,
    state: SwapchainState,
}

impl<Surface> SwapchainOwner<Surface> {
    fn new(surface: Surface) -> Self {
        Self {
            surface: Some(surface),
            state: SwapchainState::default(),
        }
    }

    fn surface(&self) -> &Surface {
        self.surface.as_ref().expect("live swapchain has a surface")
    }
}

impl<Surface> Drop for SwapchainOwner<Surface> {
    fn drop(&mut self) {
        if self.state.phase.get() == SwapchainPhase::RestartRequired {
            std::mem::forget(self.surface.take());
        }
    }
}

struct ConfigureErrorScopes<'device> {
    device: &'device wgpu::Device,
    remaining: usize,
}

impl<'device> ConfigureErrorScopes<'device> {
    fn new(device: &'device wgpu::Device) -> Self {
        Self {
            device,
            remaining: 0,
        }
    }

    fn push(&mut self) {
        for filter in [
            wgpu::ErrorFilter::Validation,
            wgpu::ErrorFilter::OutOfMemory,
            wgpu::ErrorFilter::Internal,
        ] {
            self.device.push_error_scope(filter);
            self.remaining += 1;
        }
    }

    fn pop(&mut self) -> Option<wgpu::Error> {
        self.remaining -= 1;
        pollster::block_on(self.device.pop_error_scope())
    }
}

impl Drop for ConfigureErrorScopes<'_> {
    fn drop(&mut self) {
        while self.remaining != 0 {
            let _ = self.pop();
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum SwapchainPhase {
    #[default]
    NeedsConfigure,
    RestartRequired,
    Active,
    Ready,
}

#[derive(Default)]
struct SwapchainState {
    phase: Cell<SwapchainPhase>,
}

const RESTART_REQUIRED: &str =
    "loading swapchain requires a process restart after an interrupted backend operation";

#[derive(Debug)]
enum AcquireError {
    NeedsConfigure,
    Active,
    RestartRequired,
    Surface(wgpu::SurfaceError),
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsConfigure => formatter
                .write_str("loading swapchain requires successful configuration before acquire"),
            Self::Active => {
                formatter.write_str("loading swapchain already has an active operation")
            }
            Self::RestartRequired => formatter.write_str(RESTART_REQUIRED),
            Self::Surface(error) => write!(formatter, "acquire loading swapchain image: {error}"),
        }
    }
}

impl SwapchainState {
    fn configure(&self, configure: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        match self.phase.get() {
            SwapchainPhase::Active => {
                return Err("cannot configure loading swapchain during an active operation".into());
            }
            SwapchainPhase::RestartRequired => return Err(RESTART_REQUIRED.into()),
            SwapchainPhase::Ready | SwapchainPhase::NeedsConfigure => {}
        }
        let operation = SwapchainOperation::begin(self);
        configure()?;
        operation.ready();
        Ok(())
    }

    fn acquire<Frame>(
        &self,
        acquire: impl FnOnce() -> Result<Frame, wgpu::SurfaceError>,
    ) -> Result<AcquiredFrame<'_, Frame>, AcquireError> {
        match self.phase.get() {
            SwapchainPhase::NeedsConfigure => return Err(AcquireError::NeedsConfigure),
            SwapchainPhase::Active => return Err(AcquireError::Active),
            SwapchainPhase::RestartRequired => return Err(AcquireError::RestartRequired),
            SwapchainPhase::Ready => {}
        }
        let operation = SwapchainOperation::begin(self);
        match acquire() {
            Ok(frame) => Ok(AcquiredFrame { frame, operation }),
            Err(error) => {
                if error == wgpu::SurfaceError::Timeout {
                    operation.ready();
                }
                Err(AcquireError::Surface(error))
            }
        }
    }

    fn acquire_for_redraw<Frame>(
        &self,
        acquire: impl FnOnce() -> Result<Frame, wgpu::SurfaceError>,
        reconfigure: impl FnOnce() -> Result<(), String>,
    ) -> Result<Option<AcquiredFrame<'_, Frame>>, String> {
        match self.acquire(acquire) {
            Ok(frame) => Ok(Some(frame)),
            Err(AcquireError::Surface(wgpu::SurfaceError::Timeout)) => Ok(None),
            Err(AcquireError::Surface(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated)) => {
                reconfigure()?;
                Ok(None)
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

struct SwapchainOperation<'state> {
    state: &'state SwapchainState,
    submission_started: bool,
}

impl<'state> SwapchainOperation<'state> {
    fn begin(state: &'state SwapchainState) -> Self {
        state.phase.set(SwapchainPhase::Active);
        Self {
            state,
            submission_started: false,
        }
    }

    fn ready(self) {
        self.state.phase.set(SwapchainPhase::Ready);
    }
}

impl Drop for SwapchainOperation<'_> {
    fn drop(&mut self) {
        if self.state.phase.get() == SwapchainPhase::Active {
            let phase = if self.submission_started || std::thread::panicking() {
                SwapchainPhase::RestartRequired
            } else {
                SwapchainPhase::NeedsConfigure
            };
            self.state.phase.set(phase);
        }
    }
}

struct AcquiredFrame<'state, Frame> {
    frame: Frame,
    operation: SwapchainOperation<'state>,
}

impl<Frame> AcquiredFrame<'_, Frame> {
    fn mark_submission_started(&mut self) {
        self.operation.submission_started = true;
    }

    fn present(self, present: impl FnOnce(Frame)) {
        let Self { frame, operation } = self;
        present(frame);
        operation.ready();
    }
}

fn submit_and_present(
    frame: &mut Option<AcquiredFrame<'_, wgpu::SurfaceTexture>>,
    queue: &wgpu::Queue,
    window: &Window,
    commands: impl IntoIterator<Item = wgpu::CommandBuffer>,
) -> Result<(), String> {
    frame
        .as_mut()
        .ok_or("loading swapchain image was not acquired")?
        .mark_submission_started();
    queue.submit(commands);
    window.pre_present_notify();
    frame
        .take()
        .ok_or("loading swapchain image was not acquired")?
        .present(wgpu::SurfaceTexture::present);
    Ok(())
}

fn run_with_cleanup<Resource, Output>(
    mut resource: Resource,
    run: impl FnOnce(&mut Resource) -> Output,
) -> Output {
    let result = catch_unwind(AssertUnwindSafe(|| run(&mut resource)));
    let cleanup = catch_unwind(AssertUnwindSafe(|| drop(resource)));
    match (result, cleanup) {
        (Ok(output), Ok(())) => output,
        (Ok(_), Err(payload)) | (Err(payload), Ok(())) => resume_unwind(payload),
        (Err(payload), Err(cleanup_payload)) => {
            std::mem::forget(cleanup_payload);
            resume_unwind(payload);
        }
    }
}

/// Presents on the host's swapchain configured for this renderer and window.
pub struct LoadingSurface<'host, 'window> {
    window: &'host Window,
    swapchain: &'host LoadingSwapchain<'window>,
    renderer: &'host Renderer,
    globals: &'host MenuNativeDrawState,
    component_precision: MenuComponentPrecision,
    text_precision: MenuTextPrecision,
}

impl<'host, 'window> LoadingSurface<'host, 'window> {
    pub fn new(
        window: &'host Window,
        swapchain: &'host LoadingSwapchain<'window>,
        renderer: &'host Renderer,
        globals: &'host MenuNativeDrawState,
        component_precision: MenuComponentPrecision,
        text_precision: MenuTextPrecision,
    ) -> Self {
        Self {
            window,
            swapchain,
            renderer,
            globals,
            component_precision,
            text_precision,
        }
    }

    /// Schedule a loading frame on the configured swapchain. A pre-submit
    /// panic propagates after discarding any retained image outside unwinding.
    /// If discard, submission or present panics, the swapchain is terminal.
    /// Its surface is intentionally not destroyed; restart the process.
    pub fn present(
        &self,
        frame: LoadingScreenFrame<'_>,
        cursor: Option<&mut LoadingCursor<'_>>,
    ) -> Result<(), String> {
        run_with_cleanup(
            SurfaceFrame {
                window: self.window,
                swapchain: self.swapchain,
                device: &self.renderer.device,
                queue: &self.renderer.queue,
                batch: self.renderer.ordered_screen_batch(),
                cursor,
                encoder: None,
                frame: None,
            },
            |backend| {
                frame.present(
                    self.globals,
                    self.component_precision,
                    self.text_precision,
                    backend,
                )
            },
        )
    }

    /// Routes Init's scene frame back to the existing host 3D presenter.
    pub fn present_init(
        &self,
        frame: PresentedFrame<'_>,
        cursor: Option<&mut LoadingCursor<'_>>,
        present_scene: impl FnOnce(&CanonicalSceneSnapshot) -> Result<(), String>,
    ) -> Result<(), String> {
        match frame {
            PresentedFrame::Scene(scene) => present_scene(scene),
            PresentedFrame::LoadingScreen(frame) => self.present(frame, cursor),
        }
    }

    pub fn for_open<'adapter, 'cursor>(
        &'adapter self,
        scene: &'adapter mut dyn LoadingOpenSceneBackend,
        cursor: Option<&'adapter mut LoadingCursor<'cursor>>,
    ) -> OpenLoadingSurface<'adapter, 'host, 'window, 'cursor> {
        OpenLoadingSurface {
            loading: self,
            scene,
            cursor,
        }
    }
}

/// Open's 3D device remains responsible for the STEP blend effect. This is
/// separate from the blend pipeline used by the loading-screen color pass.
pub trait LoadingOpenSceneBackend: SceneRenderBackend {
    fn set_loading_step_alpha_blend(&mut self) -> Result<(), String>;
}

pub struct OpenLoadingSurface<'adapter, 'host, 'window, 'cursor> {
    loading: &'adapter LoadingSurface<'host, 'window>,
    scene: &'adapter mut dyn LoadingOpenSceneBackend,
    cursor: Option<&'adapter mut LoadingCursor<'cursor>>,
}

impl SceneRenderBackend for OpenLoadingSurface<'_, '_, '_, '_> {
    fn render_scene(&mut self, arena: &ModelArena, scene: SceneKey) -> Result<u32, ModelError> {
        self.scene.render_scene(arena, scene)
    }
}

impl LoadingScreenDevice for OpenLoadingSurface<'_, '_, '_, '_> {
    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        self.scene.set_loading_step_alpha_blend()
    }

    fn present_loading_screen(&mut self, frame: LoadingScreenFrame<'_>) -> Result<(), String> {
        self.loading.present(frame, self.cursor.as_deref_mut())
    }
}

/// A frame-local pass with fixed blend and no depth attachment. Reset and
/// final Z restore do not mutate 3D state; the host's scene pipeline owns it.
/// Aborting before submit drops the image and requires reconfiguration before
/// another acquire. Queued uploads remain.
struct SurfaceFrame<'host, 'window, 'borrow, 'cursor> {
    window: &'host Window,
    swapchain: &'host LoadingSwapchain<'window>,
    device: &'host wgpu::Device,
    queue: &'host wgpu::Queue,
    batch: OrderedScreenBatch<'host>,
    cursor: Option<&'borrow mut LoadingCursor<'cursor>>,
    encoder: Option<wgpu::CommandEncoder>,
    frame: Option<AcquiredFrame<'host, wgpu::SurfaceTexture>>,
}

impl SurfaceFrame<'_, '_, '_, '_> {
    fn target(&self) -> Result<&wgpu::Texture, String> {
        self.frame
            .as_ref()
            .map(|frame| &frame.frame.texture)
            .ok_or_else(|| "loading swapchain image has not been acquired".into())
    }
}

impl LoadingScreenPresentBackend for SurfaceFrame<'_, '_, '_, '_> {
    fn reset_render_props(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn clear_color_and_depth(&mut self, argb: u32, depth: f32) -> Result<(), String> {
        if argb != 0 || depth != 1.0 {
            return Err("loading pass only supports black clear and default depth".into());
        }
        Ok(())
    }

    fn begin_scene(&mut self) -> Result<(), String> {
        self.swapchain.require_device(self.device)?;
        let owner = &self.swapchain.owner;
        self.frame = Some(
            owner
                .state
                .acquire(|| owner.surface().get_current_texture())
                .map_err(|error| error.to_string())?,
        );
        Ok(())
    }

    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn set_z_buffer(&mut self, _enabled: bool) -> Result<(), String> {
        Ok(())
    }

    fn screen_height(&mut self) -> Result<u32, String> {
        Ok(self.target()?.height())
    }

    fn screen_width(&mut self) -> Result<u32, String> {
        Ok(self.target()?.width())
    }

    fn set_viewport(&mut self, viewport: MenuNativeViewport) -> Result<(), String> {
        let target = self.target()?;
        if viewport.x != 0
            || viewport.y != 0
            || viewport.width != target.width()
            || viewport.height != target.height()
            || viewport.min_z != 0.0
            || viewport.max_z != 1.0
        {
            return Err("loading pass requires the full swapchain viewport".into());
        }
        Ok(())
    }

    fn draw_menu_mesh(&mut self, mesh: Mesh) -> Result<(), String> {
        self.batch.push(mesh)
    }

    fn draw_cursor(&mut self, x: f32, y: f32) -> Result<(), String> {
        let cursor = self
            .cursor
            .as_deref_mut()
            .ok_or("loading cursor mesh source is unavailable")?;
        self.batch.push(cursor(x, y)?)
    }

    fn end_scene(&mut self) -> Result<(), String> {
        self.encoder = Some(self.batch.encode(self.target()?)?);
        Ok(())
    }

    fn present_scene(&mut self) -> Result<(), String> {
        let encoder = self
            .encoder
            .take()
            .ok_or("loading swapchain pass was not encoded")?;
        let commands = encoder.finish();
        submit_and_present(&mut self.frame, self.queue, self.window, [commands])
    }
}

#[cfg(all(test, target_os = "windows"))]
#[path = "loading_surface_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
mod recovery_tests {
    use super::*;

    struct SurfaceDrop<'counts> {
        drops: &'counts Cell<usize>,
    }

    impl Drop for SurfaceDrop<'_> {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }

    struct Image<'state> {
        state: &'state SwapchainState,
        discarded: &'state Cell<usize>,
        fail_discard: bool,
    }

    impl Drop for Image<'_> {
        fn drop(&mut self) {
            assert!(!std::thread::panicking());
            assert!(self.state.phase.get() == SwapchainPhase::Active);
            self.discarded.set(self.discarded.get() + 1);
            assert!(!self.fail_discard, "synthetic discard panic");
        }
    }

    fn assert_awaiting_configuration(state: &SwapchainState) {
        assert_eq!(
            state
                .acquire::<()>(|| panic!("must not acquire"))
                .err()
                .map(|error| error.to_string())
                .as_deref(),
            Some("loading swapchain requires successful configuration before acquire")
        );
    }

    fn assert_restart_required(state: &SwapchainState) {
        for _ in 0..2 {
            assert_eq!(
                state.configure(|| panic!("must not configure")),
                Err(RESTART_REQUIRED.into())
            );
            assert_eq!(
                state
                    .acquire::<()>(|| panic!("must not acquire"))
                    .err()
                    .map(|error| error.to_string()),
                Some(RESTART_REQUIRED.into())
            );
        }
    }

    #[test]
    fn requires_initial_configuration_and_allows_retry_after_timeout() {
        let state = SwapchainState::default();
        assert_awaiting_configuration(&state);
        state.configure(|| Ok(())).unwrap();
        let error = wgpu::SurfaceError::Timeout;
        assert_eq!(
            state
                .acquire::<()>(|| Err(error.clone()))
                .err()
                .map(|error| error.to_string()),
            Some(format!("acquire loading swapchain image: {error}"))
        );
        let frame = state.acquire(|| Ok(42)).unwrap();
        frame.present(|image| assert_eq!(image, 42));
        state.acquire(|| Ok(())).unwrap().present(|_| {});
    }

    #[test]
    fn uncertain_acquire_error_requires_configuration() {
        let state = SwapchainState::default();
        state.configure(|| Ok(())).unwrap();
        let error = wgpu::SurfaceError::Other;
        assert_eq!(
            state
                .acquire::<()>(|| Err(error.clone()))
                .err()
                .map(|error| error.to_string()),
            Some(format!("acquire loading swapchain image: {error}"))
        );
        assert_awaiting_configuration(&state);
    }

    #[test]
    fn redraw_skips_timeout_and_reconfigures_lost_or_outdated_without_reacquiring() {
        let state = SwapchainState::default();
        state.configure(|| Ok(())).unwrap();
        let acquired = Cell::new(0);
        let configured = Cell::new(0);
        for (error, expected_configurations) in [
            (wgpu::SurfaceError::Timeout, 0),
            (wgpu::SurfaceError::Lost, 1),
            (wgpu::SurfaceError::Outdated, 2),
        ] {
            let acquisitions_before = acquired.get();
            let frame = state
                .acquire_for_redraw::<()>(
                    || {
                        acquired.set(acquired.get() + 1);
                        Err(error)
                    },
                    || {
                        assert_awaiting_configuration(&state);
                        state.configure(|| {
                            configured.set(configured.get() + 1);
                            Ok(())
                        })
                    },
                )
                .unwrap();
            assert!(frame.is_none());
            assert_eq!(acquired.get(), acquisitions_before + 1);
            assert_eq!(configured.get(), expected_configurations);
            state.acquire(|| Ok(())).unwrap().present(|_| {});
        }
        let result = state.acquire_for_redraw::<()>(
            || Err(wgpu::SurfaceError::Lost),
            || state.configure(|| Err("configuration failure".into())),
        );
        assert_eq!(result.err().as_deref(), Some("configuration failure"));
        assert_awaiting_configuration(&state);
    }

    #[test]
    fn redraw_and_loading_share_active_and_interrupted_states() {
        let state = SwapchainState::default();
        let blocked_redraw = || {
            state
                .acquire_for_redraw::<()>(
                    || panic!("blocked redraw acquired an image"),
                    || panic!("blocked redraw attempted configuration"),
                )
                .err()
        };
        state.configure(|| Ok(())).unwrap();
        let loading = state.acquire(|| Ok(())).unwrap();
        assert_eq!(blocked_redraw(), Some(AcquireError::Active.to_string()));
        drop(loading);
        assert_eq!(
            blocked_redraw(),
            Some(AcquireError::NeedsConfigure.to_string())
        );
        state.configure(|| Ok(())).unwrap();
        let mut redraw = state
            .acquire_for_redraw(|| Ok(()), || panic!("successful acquire reconfigured"))
            .unwrap()
            .unwrap();
        assert!(matches!(
            state.acquire::<()>(|| panic!("loading cannot acquire during redraw")),
            Err(AcquireError::Active)
        ));
        redraw.mark_submission_started();
        drop(redraw);
        assert_restart_required(&state);
        assert_eq!(blocked_redraw().as_deref(), Some(RESTART_REQUIRED));
    }

    #[test]
    fn aborted_image_is_dropped_before_explicit_successful_recovery() {
        struct Image<'state>(&'state SwapchainState, &'state Cell<usize>);
        impl Drop for Image<'_> {
            fn drop(&mut self) {
                assert!(self.0.configure(|| panic!("image is still live")).is_err());
                self.1.set(self.1.get() + 1);
            }
        }

        let state = SwapchainState::default();
        let discarded = Cell::new(0);
        state.configure(|| Ok(())).unwrap();
        let frame = state.acquire(|| Ok(Image(&state, &discarded))).unwrap();
        assert!(state.configure(|| panic!("active frame")).is_err());
        assert!(state.acquire::<()>(|| panic!("active frame")).is_err());
        drop(frame);
        assert_eq!(discarded.get(), 1);
        assert_awaiting_configuration(&state);
        assert_eq!(
            state.configure(|| Err("backend rejected configuration".into())),
            Err("backend rejected configuration".into())
        );
        assert_awaiting_configuration(&state);
        state.configure(|| Ok(())).unwrap();
        state.acquire(|| Ok(())).unwrap().present(|_| {});
        state.acquire(|| Ok(())).unwrap().present(|_| {});
    }

    #[test]
    fn in_flight_operations_reject_reentry_until_they_finish() {
        let state = SwapchainState::default();
        let assert_busy = || {
            assert!(state.configure(|| panic!("recursive configure")).is_err());
            assert!(state.acquire::<()>(|| panic!("recursive acquire")).is_err());
        };
        state
            .configure(|| {
                assert_busy();
                Ok(())
            })
            .unwrap();
        state
            .acquire(|| {
                assert_busy();
                Ok(())
            })
            .unwrap()
            .present(|_| assert_busy());
        state.configure(|| Ok(())).unwrap();
    }

    #[test]
    fn pre_submit_panic_discards_outside_unwind_and_requires_configuration() {
        let drops = Cell::new(0);
        let owner = SwapchainOwner::new(SurfaceDrop { drops: &drops });
        let state = &owner.state;
        let discarded = Cell::new(0);
        state.configure(|| Ok(())).unwrap();
        let payload = catch_unwind(AssertUnwindSafe(|| {
            run_with_cleanup(None, |frame| {
                *frame = Some(
                    state
                        .acquire(|| {
                            Ok(Image {
                                state,
                                discarded: &discarded,
                                fail_discard: false,
                            })
                        })
                        .unwrap(),
                );
                std::panic::panic_any("cursor panic");
            });
        }))
        .unwrap_err();
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"cursor panic"));
        assert_eq!(discarded.get(), 1);
        assert_awaiting_configuration(state);
        state.configure(|| Ok(())).unwrap();
        state.acquire(|| Ok(())).unwrap().present(|_| {});
        drop(owner);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn interrupted_submit_discards_outside_unwind_but_requires_restart() {
        let state = SwapchainState::default();
        let discarded = Cell::new(0);
        state.configure(|| Ok(())).unwrap();
        let frame = state
            .acquire(|| {
                Ok(Image {
                    state: &state,
                    discarded: &discarded,
                    fail_discard: false,
                })
            })
            .unwrap();
        let payload = catch_unwind(AssertUnwindSafe(|| {
            run_with_cleanup(frame, |frame| {
                frame.mark_submission_started();
                panic!("submit panic");
            });
        }))
        .unwrap_err();
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"submit panic"));
        assert_eq!(discarded.get(), 1);
        assert_restart_required(&state);
    }

    #[test]
    fn discard_panic_preserves_first_panic_and_skips_surface_teardown_during_unwind() {
        let discarded = Cell::new(0);
        let drops = Cell::new(0);
        let payload = catch_unwind(AssertUnwindSafe(|| {
            let owner = SwapchainOwner::new(SurfaceDrop { drops: &drops });
            let state = &owner.state;
            state.configure(|| Ok(())).unwrap();
            let frame = state
                .acquire(|| {
                    Ok(Image {
                        state,
                        discarded: &discarded,
                        fail_discard: true,
                    })
                })
                .unwrap();
            run_with_cleanup(frame, |_| panic!("original frame panic"));
        }))
        .unwrap_err();
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"original frame panic")
        );
        assert_eq!(discarded.get(), 1);
        assert_eq!(drops.get(), 0);
    }

    #[test]
    fn discard_panic_after_returned_error_propagates_and_skips_surface_teardown() {
        let discarded = Cell::new(0);
        let drops = Cell::new(0);
        let owner = SwapchainOwner::new(SurfaceDrop { drops: &drops });
        let state = &owner.state;
        state.configure(|| Ok(())).unwrap();
        let frame = state
            .acquire(|| {
                Ok(Image {
                    state,
                    discarded: &discarded,
                    fail_discard: true,
                })
            })
            .unwrap();
        let payload = catch_unwind(AssertUnwindSafe(|| {
            run_with_cleanup(frame, |_| Err::<(), _>("cursor error"))
        }))
        .unwrap_err();
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"synthetic discard panic")
        );
        assert_eq!(discarded.get(), 1);
        assert_restart_required(state);
        drop(owner);
        assert_eq!(drops.get(), 0);
    }

    #[test]
    fn panicking_backend_operations_cannot_reconfigure_or_acquire_again() {
        let configure_state = SwapchainState::default();
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let _ = configure_state.configure(|| panic!("configure panic"));
        }))
        .is_err());
        assert_restart_required(&configure_state);

        let acquire_state = SwapchainState::default();
        acquire_state.configure(|| Ok(())).unwrap();
        assert!(catch_unwind(AssertUnwindSafe(|| {
            let _ = acquire_state.acquire::<()>(|| panic!("acquire panic"));
        }))
        .is_err());
        assert_restart_required(&acquire_state);

        let present_state = SwapchainState::default();
        present_state.configure(|| Ok(())).unwrap();
        let frame = present_state.acquire(|| Ok(())).unwrap();
        assert!(catch_unwind(AssertUnwindSafe(|| {
            frame.present(|_| panic!("present panic"));
        }))
        .is_err());
        assert_restart_required(&present_state);
    }
}
