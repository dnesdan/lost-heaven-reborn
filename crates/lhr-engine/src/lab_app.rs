//! Application-side Lab commands, separate from UI passes and original owners.
use super::*;
use crate::lab_state::{DrawInfo, SceneRequest};

pub(super) fn synthetic_preset(options: &Options) -> Option<lab_scene::SyntheticPreset> {
    options
        .lab_scene
        .or_else(|| options.lab.then_some(lab_scene::SyntheticPreset::Gallery))
}

/// Prepare a selection without changing the current scene, paths, or owner.
pub(super) fn candidate_options(
    current: &Options,
    request: &SceneRequest,
) -> Result<Options, String> {
    let mut next = current.clone();
    next.model = None;
    next.mission = None;
    next.animation = None;
    next.animation_steps = None;
    next.sample_animation_ms = None;
    next.skeletal_animation = false;
    next.car_inspection = false;
    next.car_driving = false;
    next.steer = None;
    next.steer_steps = 1;
    next.synthetic = false;
    next.lab_scene = None;
    match request {
        SceneRequest::Synthetic(preset) => {
            next.synthetic = true;
            next.lab_scene = Some(*preset);
        }
        SceneRequest::Mission(name) => {
            let name = name.trim();
            if name.is_empty()
                || name.len() > 96
                || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return Err("Mission name must use ASCII letters, digits and underscore".into());
            }
            next.mission = Some(name.into());
        }
        SceneRequest::Model(path)
        | SceneRequest::CarInspection(path)
        | SceneRequest::CarDriving(path) => {
            let path = path.trim().replace('\\', "/");
            if path.len() > 512
                || !path.to_ascii_lowercase().ends_with(".4ds")
                || path.contains(':')
                || path.contains('\0')
                || path
                    .split('/')
                    .any(|part| part.is_empty() || matches!(part, "." | ".."))
            {
                return Err(
                    "Model must be a relative .4ds asset path without parent traversal".into(),
                );
            }
            next.model = Some(path);
            next.car_inspection = matches!(request, SceneRequest::CarInspection(_));
            next.car_driving = matches!(request, SceneRequest::CarDriving(_));
            if next.car_inspection || next.car_driving {
                next.textures = "maps".into();
            }
        }
    }
    if !next.synthetic && next.root.is_none() {
        return Err("Start with --root to browse your original installation".into());
    }
    Ok(next)
}

pub(super) fn scene_label(options: &Options) -> String {
    if options.synthetic {
        synthetic_preset(options).map_or("Color quad".into(), |preset| preset.label().into())
    } else if let Some(model) = &options.model {
        model.clone()
    } else {
        options.mission.as_deref().unwrap_or("00menu").into()
    }
}

impl App {
    /// Static fixtures and scenes have no recovered simulation callbacks.
    pub(super) fn lab_runtime_available(&self) -> bool {
        !self.runtime.models.is_empty()
            || (self.car.is_some() && (self.options.car_driving || self.steering_preview_active))
    }

    pub(super) fn lab_inspectable_model(&self) -> bool {
        self.canonical.is_none() && self.car.is_none() && self.mode != AppMode::Menu
    }

    pub(super) fn clear_lab_audio_queue(&mut self) -> Result<(), String> {
        if let Some(output) = &mut self.audio_output {
            output.clear_queue().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(super) fn reload_lab_scene(&mut self) -> Result<(), String> {
        self.transition(self.mode)?;
        let sample = lhr_engine::clock::ClockBackend::sample_ms(&mut self.clock_backend);
        self.runtime_engine = lhr_engine::runtime::RuntimeEngine::from_sample(sample);
        self.lab_status("Scene reloaded; simulation and preview clocks reset");
        Ok(())
    }

    pub(super) fn lab_status(&mut self, message: impl Into<String>) {
        let message = message.into();
        println!("Engine Lab: {message}");
        if let Some(lab) = &mut self.lab {
            lab.status = message;
        }
    }

    pub(super) fn lab_window_event(&mut self, event: &WindowEvent) -> Result<(), String> {
        if self.lab.is_none() {
            return Ok(());
        }
        if matches!(event, WindowEvent::Focused(false)) {
            self.lab.as_mut().unwrap().focus_lost();
            self.keys.clear();
            self.lab_drag = None;
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
        if *is_synthetic || event.state != ElementState::Pressed || event.repeat {
            return Ok(());
        }
        match event.physical_key {
            PhysicalKey::Code(KeyCode::F1) => {
                let lab = self.lab.as_mut().unwrap();
                lab.visible = !lab.visible;
                self.keys.clear();
                self.lab_drag = None;
            }
            PhysicalKey::Code(KeyCode::F2) if self.lab_runtime_available() => {
                let lab = self.lab.as_mut().unwrap();
                let paused = !lab.playback.paused();
                lab.playback.set_paused(paused);
                self.keys.clear();
                if paused {
                    self.clear_lab_audio_queue()?;
                }
                self.lab_status(format!("Paused: {paused}"));
            }
            PhysicalKey::Code(KeyCode::F3) if self.lab_runtime_available() => {
                self.lab.as_mut().unwrap().playback.request_step();
                self.clear_lab_audio_queue()?;
            }
            PhysicalKey::Code(KeyCode::F4) => {
                if let Err(error) = self.reload_lab_scene() {
                    if self.error.is_some() {
                        return Err(error);
                    }
                    self.lab_status(format!("Reload failed: {error}"));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Observe releases even if egui consumed them; viewport drags cannot latch.
    pub(super) fn lab_pointer_event(
        &mut self,
        event: &WindowEvent,
        consumed: bool,
    ) -> Result<(), String> {
        if self.lab.is_none() {
            return Ok(());
        }
        let locked = self.options.car_driving;
        let speed = self.lab.as_ref().unwrap().settings.camera_speed;
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let previous = self.lab_pointer.replace([position.x, position.y]);
                if !consumed && !locked {
                    if let (Some(previous), Some(button)) = (previous, self.lab_drag) {
                        let scale = self
                            .window
                            .as_ref()
                            .map_or(1., |window| window.scale_factor());
                        let dx = ((position.x - previous[0]) / scale) as f32;
                        let dy = ((position.y - previous[1]) / scale) as f32;
                        match button {
                            MouseButton::Right => lab_camera::orbit(
                                &mut self.camera,
                                -dx * 0.005 * speed,
                                dy * 0.005 * speed,
                                1.,
                            )?,
                            MouseButton::Middle => lab_camera::pan(
                                &mut self.camera,
                                -dx * 0.0015 * speed,
                                dy * 0.0015 * speed,
                            )?,
                            _ => {}
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button,
                ..
            } => {
                if self.lab_drag == Some(*button) {
                    self.lab_drag = None;
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if !consumed && !locked => {
                if matches!(button, MouseButton::Right | MouseButton::Middle) {
                    self.lab_drag = Some(*button);
                }
            }
            WindowEvent::MouseWheel { delta, .. } if !consumed && !locked => {
                let amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => *y,
                    winit::event::MouseScrollDelta::PixelDelta(position) => {
                        (position.y / 80.) as f32
                    }
                };
                lab_camera::orbit(
                    &mut self.camera,
                    0.,
                    0.,
                    (-amount * 0.12 * speed).clamp(-2., 2.).exp(),
                )?;
            }
            WindowEvent::CursorLeft { .. } | WindowEvent::Focused(false) => {
                self.lab_pointer = None;
                self.lab_drag = None;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn lab_diagnostics(&mut self) -> Result<engine_lab::Diagnostics, String> {
        let now = std::time::Instant::now();
        let frame_ms = now.duration_since(self.last_lab_redraw).as_secs_f32() * 1000.;
        self.last_lab_redraw = now;
        self.lab_delta_seconds = (frame_ms * 0.001).clamp(0., 0.1);
        let (car, feedback) = if self.options.car_driving {
            let owner = self.car.as_mut().ok_or("Engine Lab has no driving owner")?;
            (
                Some(owner.driving_state()?),
                Some(owner.driving_control_feedback()?),
            )
        } else {
            (None, None)
        };
        let source_draws = self
            .scene
            .meshes
            .len()
            .saturating_sub(usize::from(self.mode == AppMode::Menu));
        // Draw indices are stable only for synthetic/standalone model previews.
        let draws = if self.lab_inspectable_model() {
            self.scene
                .meshes
                .iter()
                .enumerate()
                .map(|(index, mesh)| DrawInfo {
                    index,
                    label: self
                        .lab_draw_labels
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| format!("Material group {}", index + 1)),
                    vertices: mesh.vertices.len(),
                    triangles: mesh.indices.len() / 3,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(engine_lab::Diagnostics {
            profile: if self.options.synthetic {
                "Public procedural scene"
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
            scene_draws: source_draws,
            vertices: self
                .scene
                .meshes
                .iter()
                .take(source_draws)
                .map(|mesh| mesh.vertices.len())
                .sum(),
            triangles: self
                .scene
                .meshes
                .iter()
                .take(source_draws)
                .map(|mesh| mesh.indices.len() / 3)
                .sum(),
            viewport: self
                .config
                .as_ref()
                .map_or(INITIAL_CAR_VIEWPORT, |config| [config.width, config.height]),
            car,
            feedback,
            driving: self.options.car_driving,
            scene_label: scene_label(&self.options),
            camera_eye: self.camera.eye.to_array(),
            camera_target: self.camera.target.to_array(),
            inspectable_model: self.lab_inspectable_model(),
            draws,
            source_draws,
            assets_available: self.options.root.is_some(),
            has_bookmark: self.lab_bookmark.is_some(),
            camera_locked: self.options.car_driving,
        })
    }

    /// True means a new device was published: the caller must discard old UI jobs.
    pub(super) fn apply_lab_actions(
        &mut self,
        commands: &engine_lab::Commands,
    ) -> Result<bool, String> {
        if commands.discover_assets {
            let root = self
                .options
                .root
                .as_deref()
                .ok_or("Start with --root to browse original assets")?;
            let catalog = lab_catalog::LabCatalog::discover(root)?;
            let status = format!(
                "Found {} scenes and {} models; loader support varies by asset",
                catalog.missions.len(),
                catalog.models.len()
            );
            self.lab.as_mut().unwrap().catalog = catalog;
            self.lab_status(status);
        }
        if commands.load_preset {
            let path = &self.lab.as_ref().unwrap().drafts.preset_path;
            let settings = lab_files::load(std::path::Path::new(path))?;
            self.lab.as_mut().unwrap().settings = settings;
            self.keys.clear();
            self.lab_drag = None;
            self.lab_turntable_degrees = 0.;
            self.lab_status("Preset loaded; live vehicle inputs remain neutral");
        }
        if commands.save_preset {
            let lab = self.lab.as_ref().unwrap();
            lab_files::save(std::path::Path::new(&lab.drafts.preset_path), &lab.settings)?;
            self.lab_status("Preset saved");
        }
        if let Some(request) = &commands.open_scene {
            let candidate = candidate_options(&self.options, request)?;
            self.transition_with_options(AppMode::Inspection, candidate)?;
            self.lab_status(format!("Opened {}", scene_label(&self.options)));
            return Ok(true);
        }
        if commands.reload_scene {
            self.reload_lab_scene()?;
            return Ok(true);
        }
        if commands.reset_model {
            self.lab_turntable_degrees = 0.;
        }
        if commands.reset_camera && self.options.car_driving {
            let settings = &mut self.lab.as_mut().unwrap().settings;
            settings.fov_degrees = 50.;
            settings.chase = driving_view::ChaseSettings::default();
        }
        if !self.options.car_driving {
            if commands.reset_camera {
                self.camera = self.lab_camera_home.camera();
                self.lab.as_mut().unwrap().settings.fov_degrees = self
                    .camera
                    .vertical_fov_radians
                    .to_degrees()
                    .clamp(30., 100.);
            }
            if commands.fit_selection && self.lab_inspectable_model() {
                let settings = self.lab.as_ref().unwrap().settings;
                let transform = self.lab_model_matrix();
                let viewport = self
                    .config
                    .as_ref()
                    .map_or(INITIAL_CAR_VIEWPORT, |config| [config.width, config.height]);
                self.camera = lab_camera::fit(
                    &self.scene,
                    transform,
                    settings.selected_draw,
                    settings.fov_degrees,
                    viewport,
                )?;
            }
            if let Some(view) = commands.camera_view {
                lab_camera::set_view(&mut self.camera, view)?;
            }
            if commands.restore_bookmark {
                self.camera = self
                    .lab_bookmark
                    .ok_or("No camera bookmark for this scene")?
                    .camera();
                self.lab.as_mut().unwrap().settings.fov_degrees = self
                    .camera
                    .vertical_fov_radians
                    .to_degrees()
                    .clamp(30., 100.);
                self.lab_status("Camera bookmark restored");
            }
            if commands.capture_bookmark {
                // Capture the same FOV this frame is about to present.
                self.camera.vertical_fov_radians =
                    self.lab.as_ref().unwrap().settings.fov_degrees.to_radians();
                self.lab_bookmark = Some(lab_camera::Bookmark::capture(&self.camera));
                self.lab_status("Camera bookmark stored for this scene");
            }
        }
        Ok(false)
    }

    fn lab_model_matrix(&self) -> Mat4 {
        if self.lab_inspectable_model() {
            if let Some(lab) = &self.lab {
                return lab_camera::model_matrix(
                    lab.settings.transform,
                    self.lab_pivot,
                    self.lab_turntable_degrees,
                );
            }
        }
        Mat4::IDENTITY
    }

    pub(super) fn advance_lab_preview(
        &mut self,
        simulation_step: Option<&lhr_engine::runtime::RuntimeFrame>,
    ) -> Result<(), String> {
        let Some(lab) = &self.lab else {
            return Ok(());
        };
        let settings = lab.settings;
        let focused = self.window.as_ref().is_none_or(|window| window.has_focus());
        let seconds = if !focused || !settings.animate_preview {
            0.
        } else if self.lab_runtime_available() && lab.playback.paused() {
            simulation_step.map_or(0., |step| step.raw_delta as f32 * 0.001)
        } else {
            self.lab_delta_seconds
        };
        self.lab_preview_seconds = (self.lab_preview_seconds + seconds).rem_euclid(3600.);
        self.lab_turntable_degrees =
            (self.lab_turntable_degrees + settings.turntable_speed * seconds).rem_euclid(360.);
        if !self.options.car_driving {
            lab_camera::orbit(
                &mut self.camera,
                settings.auto_orbit.to_radians() * seconds,
                0.,
                1.,
            )?;
        }
        Ok(())
    }

    pub(super) fn apply_lab_camera(&mut self) -> Result<(), String> {
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

    pub(super) fn apply_lab_presentation(&mut self) -> Result<(), String> {
        let Some(lab) = &self.lab else {
            return Ok(());
        };
        let settings = lab.settings;
        let transform = self.lab_model_matrix();
        let selected = settings
            .selected_draw
            .filter(|index| *index < self.scene.meshes.len());
        let hidden: Vec<usize> = if self.lab_inspectable_model() {
            (0..self.scene.meshes.len())
                .filter(|index| {
                    selected.is_some_and(|selected| {
                        (settings.isolate_draw && *index != selected)
                            || (settings.hide_selected && *index == selected)
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        if let Some(renderer) = &mut self.renderer {
            renderer.set_lab_environment(
                settings.environment,
                self.camera.eye.to_array(),
                self.lab_preview_seconds,
            )?;
            renderer.set_lab_model_transform(transform)?;
            renderer.set_lab_hidden_draws(&hidden)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_requests_keep_the_asset_root_and_clear_incompatible_modes() {
        let current = parse_options(
            [
                "--root",
                "/unused",
                "--model",
                "models/test.4ds",
                "--car-inspection",
                "--steer",
                "0.5",
            ]
            .map(str::to_string)
            .into_iter(),
        )
        .unwrap();
        let next = candidate_options(
            &current,
            &SceneRequest::Synthetic(lab_scene::SyntheticPreset::Courtyard),
        )
        .unwrap();
        assert_eq!(next.root, current.root);
        assert!(next.synthetic);
        assert!(!next.car_inspection && !next.car_driving);
        assert!(next.steer.is_none() && next.model.is_none());
        assert!(current.car_inspection);
        let next = candidate_options(&next, &SceneRequest::Mission("mise01".into())).unwrap();
        assert!(!next.synthetic);
        assert_eq!(next.mission.as_deref(), Some("mise01"));
    }

    #[test]
    fn invalid_asset_requests_leave_source_options_untouched() {
        let current = parse_options(["--synthetic"].map(str::to_string).into_iter()).unwrap();
        for path in [
            "../model.4ds",
            "/model.4ds",
            "C:\\model.4ds",
            "models/a.5ds",
            "models//a.4ds",
        ] {
            assert!(candidate_options(&current, &SceneRequest::Model(path.into())).is_err());
        }
        assert!(candidate_options(&current, &SceneRequest::Mission("mise01".into())).is_err());
        assert!(candidate_options(&current, &SceneRequest::Mission("../bad".into())).is_err());
        assert!(current.synthetic && current.root.is_none());
    }

    #[test]
    fn failed_source_load_keeps_active_scene_camera_and_lab_settings() {
        let mut options = parse_options(["--synthetic"].map(str::to_string).into_iter()).unwrap();
        // An absent root exercises failure before publication without a GPU or
        // proprietary fixture. The active public scene must remain usable.
        options.root = Some(
            std::env::temp_dir().join(format!("lhr-absent-installation-{}", std::process::id())),
        );
        let loaded = load(&options, INITIAL_CAR_VIEWPORT).unwrap();
        let home = lab_camera::Bookmark::capture(&loaded.camera);
        let mut state = engine_lab::State::default();
        state.settings.fov_degrees = 65.;
        let mut app = App {
            canonical: None,
            scene_frame: None,
            mode: AppMode::Inspection,
            ui: None,
            pointer: menu::Pointer::default(),
            adapter: None,
            scene: loaded.scene,
            camera: loaded.camera,
            runtime: loaded.runtime,
            animation: None,
            car: None,
            runtime_engine: lhr_engine::runtime::RuntimeEngine::from_sample(0),
            clock_backend: lhr_engine::runtime::PlatformClock::default(),
            options,
            window: None,
            surface: None,
            renderer: None,
            lab: Some(state),
            native_lab: None,
            last_lab_redraw: std::time::Instant::now(),
            lab_camera_home: home,
            lab_bookmark: Some(home),
            lab_pivot: Vec3::ZERO,
            lab_draw_labels: Vec::new(),
            lab_preview_seconds: 12.,
            lab_turntable_degrees: 30.,
            lab_delta_seconds: 0.,
            lab_pointer: None,
            lab_drag: None,
            config: None,
            keys: HashSet::new(),
            steering_preview_active: false,
            driving_clock: driving_view::FixedClock::default(),
            driving_controls: driving_view::Controls::default(),
            car_draws: 0,
            world_bucket: None,
            audio_capture: None,
            audio_output: None,
            count: 0,
            error: None,
        };
        let before = app.scene.meshes[0].vertices[0].position;
        let candidate = candidate_options(
            &app.options,
            &SceneRequest::Model("models/missing.4ds".into()),
        )
        .unwrap();
        assert!(app
            .transition_with_options(AppMode::Inspection, candidate)
            .is_err());
        assert!(app.options.synthetic && app.options.model.is_none());
        assert_eq!(app.scene.meshes[0].vertices[0].position, before);
        assert_eq!(lab_camera::Bookmark::capture(&app.camera), home);
        assert_eq!(app.lab.as_ref().unwrap().settings.fov_degrees, 65.);
        assert_eq!(app.lab_bookmark, Some(home));
        assert_eq!(app.lab_preview_seconds, 12.);
        assert!(app.error.is_none());
    }
}
