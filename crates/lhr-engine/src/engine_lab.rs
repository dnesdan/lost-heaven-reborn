//! Native egui inspection tools over the same live renderer and car owner.
//! UI passes only build commands. The application applies them once before
//! scheduling simulation, and stages actual inputs once in each driving tick.

use crate::{driving_view, lab_playback};
use egui::{Color32, RichText};
use lhr_engine::car_runtime::{CarDrivingControlFeedback, CarDrivingState};
use lhr_render::Renderer;
use std::collections::HashSet;
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    window::Window,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlSource {
    #[default]
    Keyboard,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub fov_degrees: f32,
    pub chase: driving_view::ChaseSettings,
    pub control_source: ControlSource,
    pub manual: driving_view::Controls,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fov_degrees: 50.,
            chase: driving_view::ChaseSettings::default(),
            control_source: ControlSource::Keyboard,
            manual: driving_view::Controls::default(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        self.chase.validate()?;
        if !self.fov_degrees.is_finite()
            || !(30.0..=100.0).contains(&self.fov_degrees)
            || !self.manual.throttle.is_finite()
            || !(0.0..=1.0).contains(&self.manual.throttle)
            || !self.manual.brake.is_finite()
            || !(0.0..=1.0).contains(&self.manual.brake)
            || !self.manual.steer.is_finite()
            || !(-1.0..=1.0).contains(&self.manual.steer)
        {
            return Err("Engine Lab parameters are outside their supported range".into());
        }
        Ok(())
    }
}

pub struct State {
    pub visible: bool,
    pub playback: lab_playback::Playback,
    pub settings: Settings,
    pub completed_frames: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            visible: true,
            playback: lab_playback::Playback::default(),
            settings: Settings::default(),
            completed_frames: 0,
        }
    }
}

impl State {
    pub fn focus_lost(&mut self) {
        self.playback.focus_lost();
        self.settings.manual = driving_view::Controls::default();
    }

    pub fn scene_reloaded(&mut self) {
        self.playback.reset();
        self.completed_frames = 0;
        self.settings.manual = driving_view::Controls::default();
    }

    pub fn controls(&self, keyboard: driving_view::Controls) -> driving_view::Controls {
        match self.settings.control_source {
            ControlSource::Keyboard => keyboard,
            ControlSource::Manual => self.settings.manual,
        }
    }

    pub fn apply(&mut self, commands: &Commands) -> Result<(), String> {
        commands.settings.validate()?;
        self.settings = commands.settings;
        if let Some(paused) = commands.paused {
            self.playback.set_paused(paused);
        }
        if commands.step {
            self.playback.request_step();
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Commands {
    pub settings: Settings,
    pub paused: Option<bool>,
    pub step: bool,
    pub reload_scene: bool,
}

impl Commands {
    fn new(settings: Settings) -> Self {
        Self {
            settings,
            paused: None,
            step: false,
            reload_scene: false,
        }
    }
}

#[derive(Default)]
pub struct Diagnostics {
    pub profile: &'static str,
    /// True only when the application has actual retained model callbacks or
    /// an active owned car driving/steering callback to dispatch.
    pub runtime_available: bool,
    /// Interval between redraw requests, not a GPU duration measurement.
    pub frame_ms: f32,
    pub driver_time_ms: u32,
    pub driving_ticks: u64,
    pub scene_draws: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub viewport: [u32; 2],
    pub car: Option<CarDrivingState>,
    pub feedback: Option<CarDrivingControlFeedback>,
    pub driving: bool,
}

pub struct UiFrame {
    jobs: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    screen: egui_wgpu::ScreenDescriptor,
}

#[derive(Default)]
struct PointerCapture {
    last_event: Option<WindowEvent>,
    buttons: HashSet<MouseButton>,
    visible: bool,
    layout_ready: bool,
}

impl PointerCapture {
    /// egui-winit queues pointer movement until Context::run. Use the current
    /// event position against the actual last layout, so a move and click
    /// between redraws cannot activate a menu item underneath the Lab.
    fn route(
        &mut self,
        context: &egui::Context,
        event: &WindowEvent,
        visible: bool,
        pixels_per_point: f32,
    ) -> Option<bool> {
        if visible && !self.visible {
            self.layout_ready = false;
        }
        self.visible = visible;
        match event {
            WindowEvent::CursorMoved { .. } => self.last_event = Some(event.clone()),
            WindowEvent::CursorLeft { .. } => self.last_event = None,
            WindowEvent::Focused(false) => self.buttons.clear(),
            _ => {}
        }
        if !visible {
            self.buttons.clear();
        }
        if !matches!(
            event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
        ) {
            return None;
        }
        let over_area = if let Some(WindowEvent::CursorMoved { position, .. }) = &self.last_event {
            let position = egui::pos2(
                position.x as f32 / pixels_per_point,
                position.y as f32 / pixels_per_point,
            );
            context
                .layer_id_at(position)
                .is_some_and(|layer| layer.order != egui::Order::Background)
        } else {
            false
        };
        // Until the first new paint, the OS window can still display the old
        // Lab panel while this fresh Context has no areas. Do not let pointer
        // actions leak through that short device/visibility transition.
        let captured = visible
            && (!self.layout_ready
                || over_area
                || !self.buttons.is_empty()
                || context.is_using_pointer());
        if let WindowEvent::MouseInput { state, button, .. } = event {
            match state {
                ElementState::Pressed if captured => {
                    self.buttons.insert(*button);
                }
                ElementState::Released => {
                    self.buttons.remove(button);
                }
                _ => {}
            }
        }
        Some(captured)
    }
}

/// Context, winit bridge and GPU resources are recreated together whenever the
/// application publishes a new wgpu device. Keep user settings in State above.
pub struct NativeLab {
    context: egui::Context,
    input: egui_winit::State,
    painter: egui_wgpu::Renderer,
    pointer: PointerCapture,
}

impl NativeLab {
    pub fn new(window: &Window, renderer: &Renderer, format: wgpu::TextureFormat) -> Self {
        let context = egui::Context::default();
        context.set_visuals(egui::Visuals::dark());
        context.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8., 8.);
            style.visuals.window_fill = Color32::from_rgb(21, 25, 32);
            style.visuals.selection.bg_fill = Color32::from_rgb(39, 94, 121);
        });
        let mut input = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(renderer.device.limits().max_texture_dimension_2d as usize),
        );
        // Reload replaces egui on the same already-focused native window;
        // another Focused(true) event is not guaranteed after device recreation.
        input.egui_input_mut().focused = window.has_focus();
        egui_winit::update_viewport_info(
            input
                .egui_input_mut()
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default(),
            &context,
            window,
            true,
        );
        let painter = egui_wgpu::Renderer::new(&renderer.device, format, None, 1, false);
        Self {
            context,
            input,
            painter,
            pointer: PointerCapture::default(),
        }
    }

    /// Keep the last real pointer location when replacing the device on the
    /// same window. A stationary pointer need not emit another CursorMoved;
    /// without this replay egui-winit would ignore its next button event.
    pub fn recreate(
        &self,
        window: &Window,
        renderer: &Renderer,
        format: wgpu::TextureFormat,
    ) -> Self {
        let mut next = Self::new(window, renderer, format);
        if let Some(event) = &self.pointer.last_event {
            let _ = next.input.on_window_event(window, event);
            next.pointer.last_event = Some(event.clone());
        }
        next
    }

    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent, visible: bool) -> bool {
        let pointer_capture = self.pointer.route(
            &self.context,
            event,
            visible,
            egui_winit::pixels_per_point(&self.context, window),
        );
        let response = self.input.on_window_event(window, event);
        if response.repaint {
            window.request_redraw();
        }
        pointer_capture.unwrap_or(visible && response.consumed)
    }

    pub fn wants_keyboard_input(&self, visible: bool) -> bool {
        visible && self.context.wants_keyboard_input()
    }

    pub fn run(
        &mut self,
        window: &Window,
        state: &State,
        diagnostics: &Diagnostics,
    ) -> (Commands, UiFrame) {
        egui_winit::update_viewport_info(
            self.input
                .egui_input_mut()
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default(),
            &self.context,
            window,
            false,
        );
        let raw = self.input.take_egui_input(window);
        let mut commands = Commands::new(state.settings);
        let output = self.context.run(raw, |context| {
            // A discarded egui pass must not toggle pause again, enqueue a
            // second step, reload the scene, or advance the vehicle.
            // Accumulate idempotent requests: later passes can lack the input
            // event that produced an earlier command, so do not reset them.
            if state.visible {
                show(context, state, diagnostics, &mut commands);
            }
        });
        self.input
            .handle_platform_output(window, output.platform_output);
        let jobs = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let size = window.inner_size();
        (
            commands,
            UiFrame {
                jobs,
                textures: output.textures_delta,
                screen: egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [size.width, size.height],
                    pixels_per_point: output.pixels_per_point,
                },
            },
        )
    }

    /// Apply texture deltas even when the surface is Lost/Outdated/Timeout.
    /// Otherwise a lost first frame can permanently lose the font atlas.
    pub fn prepare_textures(&mut self, renderer: &Renderer, frame: &UiFrame) {
        for (id, delta) in &frame.textures.set {
            self.painter
                .update_texture(&renderer.device, &renderer.queue, *id, delta);
        }
    }

    pub fn paint(
        &mut self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame: &UiFrame,
    ) -> Vec<wgpu::CommandBuffer> {
        let callbacks = self.painter.update_buffers(
            &renderer.device,
            &renderer.queue,
            encoder,
            &frame.jobs,
            &frame.screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Engine Lab overlay"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.painter.render(&mut pass, &frame.jobs, &frame.screen);
        }
        callbacks
    }

    /// Called only after the new UI command buffer was submitted and the
    /// surface frame presented. A skipped surface frame keeps input captured.
    pub fn frame_presented(&mut self) {
        self.pointer.layout_ready = true;
    }

    /// After the frame's submission, or after deliberately dropping a surface
    /// frame. Never free textures before rendering the jobs that reference them.
    pub fn finish(&mut self, renderer: &Renderer, frame: UiFrame) {
        // A lost surface can skip the render submission after update_texture
        // queued a write. Flush those writes before destroying any texture;
        // a set+free delta must not leave a destroyed pending destination.
        if !frame.textures.free.is_empty() {
            renderer.queue.submit(std::iter::empty());
        }
        for id in &frame.textures.free {
            self.painter.free_texture(id);
        }
    }
}

fn show(context: &egui::Context, state: &State, d: &Diagnostics, commands: &mut Commands) {
    egui::Window::new("Engine Lab")
        .id(egui::Id::new("engine_lab"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14., 14.))
        .default_width(318.)
        .resizable(false)
        .collapsible(false)
        .show(context, |ui| {
            egui::ScrollArea::vertical()
                .max_height((context.screen_rect().height() - 100.).max(160.))
                .show(ui, |ui| {
                    ui.label(RichText::new(d.profile).color(Color32::from_rgb(146, 190, 204)));
                    ui.small("F1 panel · F2 pause · F3 step · F4 reload");
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                d.runtime_available,
                                egui::Button::new(if state.playback.paused() {
                                    "Resume"
                                } else {
                                    "Pause"
                                }),
                            )
                            .clicked()
                        {
                            commands.paused = Some(!state.playback.paused());
                        }
                        if ui
                            .add_enabled(d.runtime_available, egui::Button::new("Step 16 ms"))
                            .clicked()
                        {
                            commands.step = true;
                        }
                        if ui.button("Reload scene").clicked() {
                            commands.reload_scene = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        if !d.runtime_available {
                            ui.strong("Static scene");
                            ui.label("No simulation clock");
                        } else {
                            ui.strong(if state.playback.paused() {
                                "Paused"
                            } else {
                                "Running"
                            });
                            ui.label(format!(
                                "{} ms · {} updates",
                                d.driver_time_ms, state.completed_frames
                            ));
                        }
                    });
                    ui.separator();
                    ui.heading("Camera");
                    ui.small("Host view");
                    ui.add(
                        egui::Slider::new(&mut commands.settings.fov_degrees, 30.0..=100.0)
                            .text("FOV °"),
                    );
                    ui.add_enabled_ui(d.driving, |ui| {
                        ui.add(
                            egui::Slider::new(&mut commands.settings.chase.distance, 3.0..=20.0)
                                .text("Distance"),
                        );
                        ui.add(
                            egui::Slider::new(&mut commands.settings.chase.height, 0.5..=8.0)
                                .text("Height"),
                        );
                        ui.add(
                            egui::Slider::new(&mut commands.settings.chase.look_ahead, 0.0..=10.0)
                                .text("Look ahead"),
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Default view").clicked() {
                            commands.settings.fov_degrees = 50.;
                            commands.settings.chase = driving_view::ChaseSettings::default();
                        }
                        if ui
                            .add_enabled(d.driving, egui::Button::new("Close view"))
                            .clicked()
                        {
                            commands.settings.fov_degrees = 55.;
                            commands.settings.chase = driving_view::ChaseSettings {
                                distance: 4.5,
                                height: 1.8,
                                look_ahead: 1.4,
                            };
                        }
                        if ui
                            .add_enabled(d.driving, egui::Button::new("Wide"))
                            .clicked()
                        {
                            commands.settings.fov_degrees = 65.;
                            commands.settings.chase = driving_view::ChaseSettings {
                                distance: 12.,
                                height: 4.5,
                                look_ahead: 3.,
                            };
                        }
                    });
                    ui.separator();
                    ui.heading("Vehicle inputs");
                    if !d.driving {
                        ui.small("Open a car in driving mode to test its actual controls.");
                    }
                    ui.add_enabled_ui(d.driving, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut commands.settings.control_source,
                                ControlSource::Keyboard,
                                "Keyboard",
                            );
                            ui.selectable_value(
                                &mut commands.settings.control_source,
                                ControlSource::Manual,
                                "Manual",
                            );
                        });
                        if commands.settings.control_source == ControlSource::Keyboard {
                            ui.small("W throttle · S brake · A/D steer · Space handbrake");
                        }
                        ui.add_enabled_ui(
                            commands.settings.control_source == ControlSource::Manual,
                            |ui| {
                                ui.add(
                                    egui::Slider::new(
                                        &mut commands.settings.manual.throttle,
                                        0.0..=1.0,
                                    )
                                    .text("Throttle"),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut commands.settings.manual.brake,
                                        0.0..=1.0,
                                    )
                                    .text("Brake"),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut commands.settings.manual.steer,
                                        -1.0..=1.0,
                                    )
                                    .text("Steering"),
                                );
                                ui.checkbox(&mut commands.settings.manual.handbrake, "Handbrake");
                                if ui.button("Neutral inputs").clicked() {
                                    commands.settings.manual = driving_view::Controls::default();
                                }
                            },
                        );
                    });
                    if let Some(feedback) = d.feedback {
                        ui.small(format!(
                            "Applied throttle {:.3} · brake {:.3}",
                            feedback.throttle, feedback.brake
                        ));
                        ui.small(format!(
                            "Steering {:.3} / {:.3} rad · gear {}",
                            feedback.steering_applied_radians,
                            feedback.steering_limit_radians,
                            feedback.gear
                        ));
                    }
                    ui.separator();
                    egui::CollapsingHeader::new("Live diagnostics")
                        .default_open(true)
                        .show(ui, |ui| {
                            if d.frame_ms.is_finite() && d.frame_ms > 0. {
                                ui.monospace(format!(
                                    "Frame interval  {:6.2} ms  ({:.0} Hz)",
                                    d.frame_ms,
                                    1000. / d.frame_ms
                                ));
                            }
                            ui.monospace(format!("Scene draws     {}", d.scene_draws));
                            ui.monospace(format!("Vertices        {}", d.vertices));
                            ui.monospace(format!("Triangles       {}", d.triangles));
                            ui.monospace(format!(
                                "Viewport        {} × {}",
                                d.viewport[0], d.viewport[1]
                            ));
                            if let Some(car) = d.car {
                                ui.monospace(format!("Vehicle ticks   {}", d.driving_ticks));
                                ui.monospace(format!("Source speed    {:.3}", car.speed));
                                ui.monospace(format!("Contacts        {}", car.contacts));
                                ui.monospace(format!(
                                    "Position        {:.2}, {:.2}, {:.2}",
                                    car.position[0], car.position[1], car.position[2]
                                ));
                            }
                        });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_move_and_click_before_redraw_use_real_lab_areas_and_retain_drag() {
        use winit::{dpi::PhysicalPosition, event::DeviceId};
        let context = egui::Context::default();
        let state = State::default();
        let mut commands = Commands::new(state.settings);
        let outside = egui::pos2(2., 2.);
        // egui swaps its current/previous pass state at end_pass. Establish
        // both layouts before testing the between-redraw routing race.
        for _ in 0..2 {
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960., 640.),
                    )),
                    events: vec![egui::Event::PointerMoved(outside)],
                    ..Default::default()
                },
                |context| show(context, &state, &Diagnostics::default(), &mut commands),
            );
        }
        let area = context
            .memory(|memory| memory.area_rect(egui::Id::new("engine_lab")))
            .unwrap();
        assert!(!area.contains(outside));
        assert!(!context.wants_pointer_input());
        let pixels_per_point = 2.;
        let moved = |position: egui::Pos2| WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(position.x * pixels_per_point),
                f64::from(position.y * pixels_per_point),
            ),
        };
        let click = |state| WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button: MouseButton::Left,
        };
        let mut pointer = PointerCapture {
            visible: true,
            layout_ready: true,
            ..Default::default()
        };
        assert_eq!(
            pointer.route(&context, &moved(area.center()), true, pixels_per_point),
            Some(true)
        );
        // No Context::run has occurred since the old outside pointer position.
        assert!(!context.wants_pointer_input());
        assert_eq!(
            pointer.route(
                &context,
                &click(ElementState::Pressed),
                true,
                pixels_per_point
            ),
            Some(true)
        );
        assert_eq!(
            pointer.route(&context, &moved(outside), true, pixels_per_point),
            Some(true)
        );
        assert_eq!(
            pointer.route(
                &context,
                &click(ElementState::Released),
                true,
                pixels_per_point
            ),
            Some(true)
        );
        assert_eq!(
            pointer.route(
                &context,
                &click(ElementState::Pressed),
                true,
                pixels_per_point
            ),
            Some(false)
        );
        assert_eq!(
            pointer.route(&context, &moved(area.center()), false, pixels_per_point),
            Some(false)
        );

        // A same-window device reload keeps the last real cursor event, but
        // its fresh Context has no laid-out areas until the next paint.
        let fresh = egui::Context::default();
        assert!(fresh.layer_id_at(area.center()).is_none());
        let mut recreated = PointerCapture {
            last_event: pointer.last_event.clone(),
            ..Default::default()
        };
        assert_eq!(
            recreated.route(
                &fresh,
                &click(ElementState::Pressed),
                true,
                pixels_per_point
            ),
            Some(true)
        );
        assert_eq!(
            recreated.route(
                &fresh,
                &click(ElementState::Released),
                true,
                pixels_per_point
            ),
            Some(true)
        );
    }

    #[test]
    fn repeated_ui_commands_stage_one_step_and_one_complete_control_source() {
        let mut state = State::default();
        let manual = driving_view::Controls {
            throttle: 0.4,
            brake: 0.2,
            steer: -0.3,
            handbrake: true,
        };
        let keyboard = driving_view::Controls {
            throttle: 1.,
            brake: 0.,
            steer: 1.,
            handbrake: false,
        };
        let mut command = Commands::new(Settings {
            control_source: ControlSource::Manual,
            manual,
            ..Settings::default()
        });
        command.step = true;
        state.apply(&command).unwrap();
        state.apply(&command).unwrap();
        assert_eq!(state.controls(keyboard), manual);
        assert_eq!(
            state.playback.take_frame_plan().advance,
            lab_playback::Advance::SingleStep
        );
        assert_eq!(
            state.playback.take_frame_plan().advance,
            lab_playback::Advance::Paused
        );
        let mut keyboard_command = Commands::new(state.settings);
        keyboard_command.settings.control_source = ControlSource::Keyboard;
        state.apply(&keyboard_command).unwrap();
        assert_eq!(state.controls(keyboard), keyboard);
    }

    #[test]
    fn invalid_ui_settings_do_not_mutate_state_and_focus_loss_cancels_pending_input() {
        let mut state = State::default();
        let original = state.settings;
        let mut invalid = Commands::new(original);
        invalid.settings.manual.throttle = f32::NAN;
        invalid.paused = Some(true);
        invalid.step = true;
        assert!(state.apply(&invalid).is_err());
        assert_eq!(state.settings, original);
        assert!(!state.playback.paused());
        state.settings.control_source = ControlSource::Manual;
        state.settings.manual.throttle = 1.;
        state.playback.request_step();
        state.focus_lost();
        assert_eq!(state.settings.manual, driving_view::Controls::default());
        assert!(state.playback.paused());
        assert_eq!(
            state.playback.take_frame_plan().advance,
            lab_playback::Advance::Paused
        );
    }
}
