//! Ordered C_mission::Open dependency driver, not a substitute for the owners.
//!
//! Reference: mafia-re dc867c3238488de27d094bb79b6550e392566e3c,
//! game/mission/Mission.cpp, C_mission::Open (PC entry 0x5409d0).
//! Every reached operation is required; unavailable dependencies return Err.
//! This is not yet connected to the production mission runtime.
#![forbid(unsafe_code)]

/// Descriptive callback boundaries, not separately reconstructed functions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenStep {
    /// ResetUnknown363c, then clear the mission's unknown_8, before Close.
    ResetGame,
    ClosePrevious,
    /// Clear unknown_78 and the actor reference counter after Close returns.
    ResetPostClose,
    WaitForMedia,
    BindVehicleScene,
    CopyMissionName,
    /// Create progress UI only when requested and no existing owner is present.
    PrepareProgress,
    OpenWorld,
    ResetWeather,
    FillSectorBits,
    /// Optional check.bin intake; source errors must not become absence.
    LoadNavigation,
    /// Optional road.bin intake, after navigation and before OpenBin.
    LoadRoads,
    /// Set progress mode to zero, try cache.bin, close it on success, then set
    /// mode to one. This probe does not decide whether the later open is tried.
    ProbeCache,
    /// Use the live audio owner to mute, then render the active mission scene.
    MuteAudioAndRender,
    /// Set the original restore_volume flag even when audio is absent.
    MarkRestoreVolume,
    OpenBin,
    /// Use the audio owner and configured volume that exist AFTER OpenBin.
    RestoreAudio,
    /// Read live progress mode: change one to two, leaving every other value.
    /// This occurs after audio restoration, including native OpenBin failure.
    AdvanceCacheProgress,
    MarkMissionOpened,
    /// Try a fresh optional cache.bin open regardless of ProbeCache's result.
    /// On success consume the retained stream through base_cache.Open and close
    /// it; genuine absence is the original empty branch, not a missing backend.
    OpenCache,
    FinishProgress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenResult {
    /// The callbacks returned successfully, not proof of GameInit or gameplay.
    Opened,
    NoMedia,
    OpenBinFailed,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpenFailure<E> {
    pub step: OpenStep,
    pub error: E,
}

/// Operations over the caller's actual scene, navigation, roads, audio and cache
/// owners. There is no fallback implementation for an unavailable dependency.
/// The caller retains mission inputs and state; this driver does not mirror them.
pub trait OpenOperations {
    type Error;

    /// Execute the selected boundary in its documented order. Optional file
    /// opens happen here, not through cached presence flags in the driver.
    fn execute(&mut self, step: OpenStep) -> Result<(), Self::Error>;
    /// Return false only for the original WaitForCD early exit.
    fn media_ready(&mut self) -> Result<bool, Self::Error>;
    /// Return false for a native nonzero OpenBin result; Err is a distinct host
    /// interruption. Preserve all completed effects in either case.
    fn open_bin(&mut self) -> Result<bool, Self::Error>;
    /// Read current audio-owner presence at each call. OpenBin and rendering
    /// may change it; do not cache this observation inside the backend either.
    fn audio_present(&self) -> bool;
}

#[derive(Debug, PartialEq, Eq)]
pub enum OpenRunError<E> {
    AlreadyEntered,
    Callback(OpenFailure<E>),
}

/// Single-use progress for one invocation. Deliberately not Clone/Copy: copying
/// an unentered guard must not provide two invocations of the same attempt.
/// A new attempt still requires the caller to validate its real owner lifecycle.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct OpenAttempt {
    entered: bool,
    last: Option<OpenStep>,
}

impl OpenAttempt {
    pub fn last(&self) -> Option<OpenStep> {
        self.last
    }

    pub fn entered(&self) -> bool {
        self.entered
    }

    pub fn run<O: OpenOperations>(
        &mut self,
        ops: &mut O,
    ) -> Result<OpenResult, OpenRunError<O::Error>> {
        // No replay, including after a native early return or callback error.
        if self.entered {
            return Err(OpenRunError::AlreadyEntered);
        }
        self.entered = true;
        macro_rules! step {
            ($step:expr) => {{
                self.last = Some($step);
                ops.execute($step).map_err(|error| {
                    OpenRunError::Callback(OpenFailure { step: $step, error })
                })?;
            }};
        }
        step!(OpenStep::ResetGame);
        step!(OpenStep::ClosePrevious);
        step!(OpenStep::ResetPostClose);
        self.last = Some(OpenStep::WaitForMedia);
        if !ops.media_ready().map_err(|error| {
            OpenRunError::Callback(OpenFailure {
                step: OpenStep::WaitForMedia,
                error,
            })
        })? {
            return Ok(OpenResult::NoMedia);
        }
        step!(OpenStep::BindVehicleScene);
        step!(OpenStep::CopyMissionName);
        step!(OpenStep::PrepareProgress);
        step!(OpenStep::OpenWorld);
        step!(OpenStep::ResetWeather);
        step!(OpenStep::FillSectorBits);
        step!(OpenStep::LoadNavigation);
        step!(OpenStep::LoadRoads);
        step!(OpenStep::ProbeCache);
        if ops.audio_present() {
            step!(OpenStep::MuteAudioAndRender);
        }
        step!(OpenStep::MarkRestoreVolume);
        self.last = Some(OpenStep::OpenBin);
        // Host Err is terminal, not a native return code or a cleanup request.
        let opened = ops.open_bin().map_err(|error| {
            OpenRunError::Callback(OpenFailure {
                step: OpenStep::OpenBin,
                error,
            })
        })?;
        // The reference rereads isound here. A cached pre-OpenBin flag could
        // skip a newly created owner or try to restore an owner already gone.
        if ops.audio_present() {
            step!(OpenStep::RestoreAudio);
        }
        step!(OpenStep::AdvanceCacheProgress);
        if !opened {
            return Ok(OpenResult::OpenBinFailed);
        }
        step!(OpenStep::MarkMissionOpened);
        // The reference tries ROpen again even if the first probe found no file.
        step!(OpenStep::OpenCache);
        step!(OpenStep::FinishProgress);
        Ok(OpenResult::Opened)
    }
}

#[cfg(test)]
#[path = "mission_open_order_tests.rs"]
mod tests;
