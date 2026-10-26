//! Host diagnostic state and scheduling requests shared by both build profiles.
use crate::{driving_view, lab_playback};
use lhr_engine::car_runtime::{CarDrivingControlFeedback, CarDrivingState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(feature = "engine-lab"), allow(dead_code))] // Only the UI selects Manual.
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
    #[cfg(feature = "engine-lab")]
    pub(crate) fn new(settings: Settings) -> Self {
        Self {
            settings,
            paused: None,
            step: false,
            reload_scene: false,
        }
    }
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
}
