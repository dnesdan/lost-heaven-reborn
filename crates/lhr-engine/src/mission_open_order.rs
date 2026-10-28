//! Ordered C_mission::Open dependency driver, not a substitute for the owners.
//! Reference: mafia-re dc867c3 game/mission/Mission.cpp, C_mission::Open.
//! Every operation is a required callback; unsupported operations must return Err.
//! This does not yet connect the production mission runtime.
#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenStep {
    ResetGame,
    ClosePrevious,
    ResetPostClose,
    WaitForMedia,
    BindVehicleScene,
    PrepareProgress,
    OpenWorld,
    ResetWeather,
    FillSectorBits,
    LoadNavigation,
    LoadRoads,
    ProbeCache,
    MuteAudioAndRender,
    OpenBin,
    RestoreAudio,
    MarkMissionOpened,
    OpenCache,
    FinishProgress,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenResult {
    Opened,
    NoMedia,
    OpenBinFailed,
}
#[derive(Debug, PartialEq, Eq)]
pub struct OpenFailure<E> {
    pub step: OpenStep,
    pub error: E,
}
/// The callback must use the caller's existing Scene, navigation, roads, audio
/// and cache owners. No callback may silently succeed when its work is absent.
pub trait OpenOperations {
    type Error;
    fn execute(&mut self, step: OpenStep) -> Result<(), Self::Error>;
    /// Return false only for the original WaitForCD early exit.
    fn media_ready(&mut self) -> Result<bool, Self::Error>;
    /// Return false only when the original OpenBin reports failure.
    fn open_bin(&mut self) -> Result<bool, Self::Error>;
    /// True if the cache.bin existence probe found the file.
    fn cache_present(&self) -> bool;
    /// True when the native audio backend exists.
    fn audio_present(&self) -> bool;
}
#[derive(Debug, PartialEq, Eq)]
pub enum OpenRunError<E> {
    AlreadyEntered,
    Callback(OpenFailure<E>),
}
/// Single-use progress. A failure never retries already executed operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpenAttempt {
    entered: bool,
    last: Option<OpenStep>,
}
impl Default for OpenAttempt {
    fn default() -> Self { Self { entered: false, last: None } }
}
impl OpenAttempt {
    pub fn last(&self) -> Option<OpenStep> { self.last }
    pub fn entered(&self) -> bool { self.entered }
    pub fn run<O: OpenOperations>(&mut self, ops: &mut O) -> Result<OpenResult, OpenRunError<O::Error>> {
        // Refuse replay even after a native early return or an interrupted callback.
        if self.entered { return Err(OpenRunError::AlreadyEntered); }
        self.entered = true;
        macro_rules! step {
            ($step:expr) => {{
                self.last = Some($step);
                ops.execute($step).map_err(|error| OpenRunError::Callback(OpenFailure { step: $step, error }))?;
            }};
        }
        step!(OpenStep::ResetGame);
        step!(OpenStep::ClosePrevious);
        step!(OpenStep::ResetPostClose);
        self.last = Some(OpenStep::WaitForMedia);
        if !ops.media_ready().map_err(|error| OpenRunError::Callback(OpenFailure { step: OpenStep::WaitForMedia, error }))? {
            return Ok(OpenResult::NoMedia);
        }
        step!(OpenStep::BindVehicleScene);
        step!(OpenStep::PrepareProgress);
        step!(OpenStep::OpenWorld);
        step!(OpenStep::ResetWeather);
        step!(OpenStep::FillSectorBits);
        step!(OpenStep::LoadNavigation);
        step!(OpenStep::LoadRoads);
        step!(OpenStep::ProbeCache);
        let cache = ops.cache_present();
        let audio = ops.audio_present();
        if audio { step!(OpenStep::MuteAudioAndRender); }
        self.last = Some(OpenStep::OpenBin);
        // The original restores audio even when OpenBin returns a failure code.
        // An unavailable backend is a terminal error; it is not a native code.
        let opened = ops.open_bin().map_err(|error| OpenRunError::Callback(OpenFailure { step: OpenStep::OpenBin, error }))?;
        if audio { step!(OpenStep::RestoreAudio); }
        if !opened { return Ok(OpenResult::OpenBinFailed); }
        step!(OpenStep::MarkMissionOpened);
        if cache { step!(OpenStep::OpenCache); }
        step!(OpenStep::FinishProgress);
        Ok(OpenResult::Opened)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Ops {
        trace: Vec<OpenStep>,
        fail: Option<OpenStep>,
        media: bool,
        bin: bool,
        cache: bool,
        audio: bool,
    }
    impl OpenOperations for Ops {
        type Error = &'static str;
        fn execute(&mut self, step: OpenStep) -> Result<(), Self::Error> {
            self.trace.push(step);
            if self.fail == Some(step) { Err("failed") } else { Ok(()) }
        }
        fn media_ready(&mut self) -> Result<bool, Self::Error> {
            self.trace.push(OpenStep::WaitForMedia);
            if self.fail == Some(OpenStep::WaitForMedia) { Err("failed") } else { Ok(self.media) }
        }
        fn open_bin(&mut self) -> Result<bool, Self::Error> {
            self.trace.push(OpenStep::OpenBin);
            if self.fail == Some(OpenStep::OpenBin) { Err("failed") } else { Ok(self.bin) }
        }
        fn cache_present(&self) -> bool { self.cache }
        fn audio_present(&self) -> bool { self.audio }
    }
    #[test]
    fn success_keeps_reference_order_and_optional_branches() {
        let mut ops = Ops { media: true, bin: true, cache: true, audio: true, ..Ops::default() };
        assert_eq!(OpenAttempt::default().run(&mut ops), Ok(OpenResult::Opened));
        assert_eq!(ops.trace, [
            OpenStep::ResetGame, OpenStep::ClosePrevious, OpenStep::ResetPostClose, OpenStep::WaitForMedia,
            OpenStep::BindVehicleScene, OpenStep::PrepareProgress, OpenStep::OpenWorld, OpenStep::ResetWeather,
            OpenStep::FillSectorBits, OpenStep::LoadNavigation, OpenStep::LoadRoads,
            OpenStep::ProbeCache, OpenStep::MuteAudioAndRender, OpenStep::OpenBin,
            OpenStep::RestoreAudio, OpenStep::MarkMissionOpened, OpenStep::OpenCache, OpenStep::FinishProgress
        ]);
    }
    #[test]
    fn failed_bin_still_restores_volume_but_skips_cache_and_finish() {
        let mut ops = Ops { media: true, bin: false, cache: true, audio: true, ..Ops::default() };
        assert_eq!(OpenAttempt::default().run(&mut ops), Ok(OpenResult::OpenBinFailed));
        assert_eq!(ops.trace.last(), Some(&OpenStep::RestoreAudio));
        assert!(!ops.trace.contains(&OpenStep::OpenCache));
    }
    #[test]
    fn media_early_exit_has_no_scene_effects() {
        let mut ops = Ops::default();
        assert_eq!(OpenAttempt::default().run(&mut ops), Ok(OpenResult::NoMedia));
        assert_eq!(ops.trace, [OpenStep::ResetGame, OpenStep::ClosePrevious, OpenStep::WaitForMedia]);
    }
    #[test]
    fn first_error_is_retained_and_later_callbacks_are_not_reached() {
        let mut ops = Ops { media: true, bin: true, fail: Some(OpenStep::LoadRoads), ..Ops::default() };
        let mut attempt = OpenAttempt::default();
        assert_eq!(attempt.run(&mut ops), Err(OpenRunError::Callback(OpenFailure { step: OpenStep::LoadRoads, error: "failed" })));
        assert_eq!(attempt.last(), Some(OpenStep::LoadRoads));
        assert_eq!(ops.trace.last(), Some(&OpenStep::LoadRoads));
        assert!(!ops.trace.contains(&OpenStep::OpenBin));
    }
    #[test]
    fn retry_is_a_typed_error_without_repeating_callbacks() {
        let mut ops = Ops { media: true, bin: true, ..Ops::default() };
        let mut attempt = OpenAttempt::default();
        assert_eq!(attempt.run(&mut ops), Ok(OpenResult::Opened));
        let reached = ops.trace.clone();
        assert_eq!(attempt.run(&mut ops), Err(OpenRunError::AlreadyEntered));
        assert_eq!(ops.trace, reached);
    }
    #[test]
    fn absent_optional_audio_and_cache_skip_their_callbacks() {
        let mut ops = Ops { media: true, bin: true, ..Ops::default() };
        assert_eq!(OpenAttempt::default().run(&mut ops), Ok(OpenResult::Opened));
        assert!(!ops.trace.contains(&OpenStep::MuteAudioAndRender));
        assert!(!ops.trace.contains(&OpenStep::RestoreAudio));
        assert!(!ops.trace.contains(&OpenStep::OpenCache));
    }
}
