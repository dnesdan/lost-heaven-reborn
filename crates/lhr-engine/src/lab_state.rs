//! Host diagnostic state and scheduling requests shared by both build profiles.
use crate::{driving_view, lab_playback};
use lhr_engine::car_runtime::{CarDrivingControlFeedback, CarDrivingState};
use lhr_render::LabEnvironment;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(feature = "engine-lab"), allow(dead_code))] // Only the UI selects Manual.
pub enum ControlSource {
    #[default]
    Keyboard,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelTransform {
    pub translation: [f32; 3],
    pub rotation_degrees: [f32; 3],
    pub scale: f32,
}
impl Default for ModelTransform {
    fn default() -> Self {
        Self {
            translation: [0.; 3],
            rotation_degrees: [0.; 3],
            scale: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "engine-lab"), allow(dead_code))]
pub enum CameraView {
    Front,
    Back,
    Left,
    Right,
    Top,
    Isometric,
}
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(feature = "engine-lab"), allow(dead_code))]
pub enum SceneRequest {
    Synthetic(crate::lab_scene::SyntheticPreset),
    Mission(String),
    Model(String),
    CarInspection(String),
    CarDriving(String),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(feature = "engine-lab"), allow(dead_code))]
pub enum LabTab {
    #[default]
    Scene,
    Environment,
    Camera,
    Model,
    Vehicle,
    Diagnostics,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiDrafts {
    pub tab: LabTab,
    pub mission_path: String,
    pub model_path: String,
    pub search: String,
    pub preset_path: String,
}
impl Default for UiDrafts {
    fn default() -> Self {
        Self {
            tab: LabTab::default(),
            mission_path: "mise01".into(),
            model_path: String::new(),
            search: String::new(),
            preset_path: "engine-lab.lhr-lab".into(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub fov_degrees: f32,
    pub chase: driving_view::ChaseSettings,
    pub control_source: ControlSource,
    pub manual: driving_view::Controls,
    pub environment: LabEnvironment,
    pub animate_preview: bool,
    pub camera_speed: f32,
    pub auto_orbit: f32,
    pub turntable_speed: f32,
    pub transform: ModelTransform,
    pub selected_draw: Option<usize>,
    pub isolate_draw: bool,
    pub hide_selected: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fov_degrees: 50.,
            chase: driving_view::ChaseSettings::default(),
            control_source: ControlSource::Keyboard,
            manual: driving_view::Controls::default(),
            environment: LabEnvironment::default(),
            animate_preview: true,
            camera_speed: 1.,
            auto_orbit: 0.,
            turntable_speed: 0.,
            transform: ModelTransform::default(),
            selected_draw: None,
            isolate_draw: false,
            hide_selected: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        self.chase.validate()?;
        self.environment.validate()?;
        if !self.camera_speed.is_finite()
            || !(0.1..=10.).contains(&self.camera_speed)
            || !self.auto_orbit.is_finite()
            || !(-90. ..=90.).contains(&self.auto_orbit)
            || !self.turntable_speed.is_finite()
            || !(-180. ..=180.).contains(&self.turntable_speed)
            || !self.transform.scale.is_finite()
            || !(0.01..=100.).contains(&self.transform.scale)
            || self
                .transform
                .translation
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 10000.)
            || self
                .transform
                .rotation_degrees
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 180.)
        {
            return Err(
                "Engine Lab camera or model parameters are outside their supported range".into(),
            );
        }
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
    pub drafts: UiDrafts,
    pub catalog: crate::lab_catalog::LabCatalog,
    pub status: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            visible: true,
            playback: lab_playback::Playback::default(),
            settings: Settings::default(),
            completed_frames: 0,
            drafts: UiDrafts::default(),
            catalog: crate::lab_catalog::LabCatalog::default(),
            status: "Ready".into(),
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
        self.settings.selected_draw = None;
        self.settings.isolate_draw = false;
        self.settings.hide_selected = false;
        self.settings.transform = ModelTransform::default();
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
        self.drafts = commands.drafts.clone();
        if let Some(paused) = commands.paused {
            self.playback.set_paused(paused);
        }
        if commands.step {
            self.playback.request_step();
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Commands {
    pub settings: Settings,
    pub paused: Option<bool>,
    pub step: bool,
    pub reload_scene: bool,
    pub drafts: UiDrafts,
    pub open_scene: Option<SceneRequest>,
    pub discover_assets: bool,
    pub reset_camera: bool,
    pub camera_view: Option<CameraView>,
    pub capture_bookmark: bool,
    pub restore_bookmark: bool,
    pub save_preset: bool,
    pub load_preset: bool,
    pub fit_selection: bool,
    pub reset_model: bool,
}

impl Commands {
    #[cfg(feature = "engine-lab")]
    pub(crate) fn from_state(state: &State) -> Self {
        let mut commands = Self::new(state.settings);
        commands.drafts = state.drafts.clone();
        commands
    }
    #[cfg(feature = "engine-lab")]
    pub(crate) fn new(settings: Settings) -> Self {
        Self {
            settings,
            paused: None,
            step: false,
            reload_scene: false,
            drafts: UiDrafts::default(),
            open_scene: None,
            discover_assets: false,
            reset_camera: false,
            camera_view: None,
            capture_bookmark: false,
            restore_bookmark: false,
            save_preset: false,
            load_preset: false,
            fit_selection: false,
            reset_model: false,
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "engine-lab"), allow(dead_code))]
pub struct DrawInfo {
    pub index: usize,
    pub label: String,
    pub vertices: usize,
    pub triangles: usize,
}
#[derive(Default)]
#[cfg_attr(not(feature = "engine-lab"), allow(dead_code))] // Read by the optional UI.
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
    pub scene_label: String,
    pub camera_eye: [f32; 3],
    pub camera_target: [f32; 3],
    pub inspectable_model: bool,
    pub draws: Vec<DrawInfo>,
    pub source_draws: usize,
    pub assets_available: bool,
    pub has_bookmark: bool,
    pub camera_locked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_clears_scene_local_edits_but_retains_environment_and_pause_choice() {
        let mut state = State::default();
        state.settings.environment.enabled = true;
        state.settings.fov_degrees = 65.;
        state.settings.transform.translation = [1., 2., 3.];
        state.settings.selected_draw = Some(9);
        state.settings.isolate_draw = true;
        state.settings.hide_selected = true;
        state.settings.manual.throttle = 1.;
        state.playback.request_step();
        state.completed_frames = 12;
        state.scene_reloaded();
        assert_eq!(state.settings.transform, ModelTransform::default());
        assert_eq!(state.settings.selected_draw, None);
        assert!(!state.settings.isolate_draw);
        assert!(!state.settings.hide_selected);
        assert_eq!(state.settings.manual, driving_view::Controls::default());
        assert!(state.settings.environment.enabled);
        assert_eq!(state.settings.fov_degrees, 65.);
        assert_eq!(state.completed_frames, 0);
        assert!(state.playback.paused());
        assert_eq!(
            state.playback.take_frame_plan().advance,
            lab_playback::Advance::Paused
        );
    }

    #[test]
    fn host_settings_reject_nonfinite_and_out_of_range_values() {
        let mut settings = Settings {
            camera_speed: f32::NAN,
            ..Settings::default()
        };
        assert!(settings.validate().is_err());
        settings = Settings::default();
        settings.auto_orbit = 91.;
        assert!(settings.validate().is_err());
        settings = Settings::default();
        settings.turntable_speed = f32::INFINITY;
        assert!(settings.validate().is_err());
        settings = Settings::default();
        settings.transform.translation[2] = 10001.;
        assert!(settings.validate().is_err());
        settings = Settings::default();
        settings.transform.rotation_degrees[1] = -181.;
        assert!(settings.validate().is_err());
        settings = Settings::default();
        settings.transform.scale = 0.;
        assert!(settings.validate().is_err());
    }
}
