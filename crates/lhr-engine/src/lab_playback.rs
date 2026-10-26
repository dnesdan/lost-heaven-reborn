//! Host inspection playback. A paused redraw must never call the runtime.
//! This scheduler does not change the recovered simulation step or physics.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advance {
    Paused,
    PlatformClock,
    SingleStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub advance: Advance,
    pub reset_platform_clock: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Playback {
    paused: bool,
    step_pending: bool,
    reset_clock: bool,
}

impl Playback {
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// A requested state is idempotent even if the UI makes multiple passes.
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused != paused {
            self.paused = paused;
            self.step_pending = false;
            self.reset_clock = true;
        }
    }

    /// Repeated staging before consumption still schedules exactly one tick.
    pub fn request_step(&mut self) {
        self.set_paused(true);
        self.step_pending = true;
    }

    /// Preserve the user's pause choice when the scene is explicitly reloaded.
    pub fn reset(&mut self) {
        self.step_pending = false;
        self.reset_clock = true;
    }

    pub fn focus_lost(&mut self) {
        self.set_paused(true);
        self.step_pending = false;
        self.reset_clock = true;
    }

    pub fn take_frame_plan(&mut self) -> FramePlan {
        let reset_platform_clock = std::mem::take(&mut self.reset_clock);
        let advance = if std::mem::take(&mut self.step_pending) {
            Advance::SingleStep
        } else if self.paused {
            Advance::Paused
        } else {
            Advance::PlatformClock
        };
        FramePlan {
            advance,
            reset_platform_clock,
        }
    }
}

/// Used only for one deliberate Lab step. Reset the engine clock against this
/// clock first: the following sample is exactly 16 ms later. Restore the real
/// platform-clock baseline after dispatch, without advancing the simulation.
#[derive(Default)]
pub struct SingleStepClock(u32);

impl lhr_engine::clock::ClockBackend for SingleStepClock {
    fn sample_ms(&mut self) -> u32 {
        let sample = self.0;
        self.0 = self.0.wrapping_add(16);
        sample
    }

    fn sleep_ms(&mut self, milliseconds: u32) {
        self.0 = self.0.wrapping_add(milliseconds);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_redraws_never_request_runtime_dispatch_and_resume_discards_wall_gap() {
        let mut playback = Playback::default();
        playback.set_paused(true);
        assert_eq!(
            playback.take_frame_plan(),
            FramePlan {
                advance: Advance::Paused,
                reset_platform_clock: true,
            }
        );
        for _ in 0..100 {
            assert_eq!(playback.take_frame_plan().advance, Advance::Paused);
        }
        playback.set_paused(false);
        playback.set_paused(false);
        assert_eq!(
            playback.take_frame_plan(),
            FramePlan {
                advance: Advance::PlatformClock,
                reset_platform_clock: true,
            }
        );
        assert!(!playback.take_frame_plan().reset_platform_clock);
    }

    #[test]
    fn repeated_step_staging_dispatches_once_and_remains_paused() {
        let mut playback = Playback::default();
        playback.request_step();
        playback.request_step();
        assert_eq!(playback.take_frame_plan().advance, Advance::SingleStep);
        assert!(playback.paused());
        assert_eq!(playback.take_frame_plan().advance, Advance::Paused);
        playback.request_step();
        playback.focus_lost();
        assert_eq!(playback.take_frame_plan().advance, Advance::Paused);
    }
}
