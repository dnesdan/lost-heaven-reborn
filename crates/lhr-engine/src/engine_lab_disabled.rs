//! Compile-time boundary for the optional native UI.
//! CLI admission rejects --lab in this build, so these methods are unreachable.
pub use crate::lab_state::{Commands, Diagnostics, Settings, State};
use lhr_render::Renderer;
use winit::{event::WindowEvent, window::Window};

pub struct UiFrame;
pub struct NativeLab;
impl NativeLab {
    pub fn new(_: &Window, _: &Renderer, _: wgpu::TextureFormat) -> Self {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn recreate(&self, _: &Window, _: &Renderer, _: wgpu::TextureFormat) -> Self {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn on_window_event(&mut self, _: &Window, _: &WindowEvent, _: bool) -> bool {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn wants_keyboard_input(&self, _: bool) -> bool {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn run(&mut self, _: &Window, _: &State, _: &Diagnostics) -> (Commands, UiFrame) {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn prepare_textures(&mut self, _: &Renderer, _: &UiFrame) {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn paint(
        &mut self,
        _: &Renderer,
        _: &mut wgpu::CommandEncoder,
        _: &wgpu::TextureView,
        _: &UiFrame,
    ) -> Vec<wgpu::CommandBuffer> {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn frame_presented(&mut self) {
        unreachable!("--lab requires --features engine-lab")
    }
    pub fn finish(&mut self, _: &Renderer, _: UiFrame) {
        unreachable!("--lab requires --features engine-lab")
    }
}
