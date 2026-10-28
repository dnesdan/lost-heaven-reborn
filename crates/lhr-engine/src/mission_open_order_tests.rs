//! Synthetic live owners for the ordered boundary; not production MissionOpen.
use super::*;
use std::cell::Cell;

const FULL_ORDER: [OpenStep; 21] = [
    OpenStep::ResetGame,
    OpenStep::ClosePrevious,
    OpenStep::ResetPostClose,
    OpenStep::WaitForMedia,
    OpenStep::BindVehicleScene,
    OpenStep::CopyMissionName,
    OpenStep::PrepareProgress,
    OpenStep::OpenWorld,
    OpenStep::ResetWeather,
    OpenStep::FillSectorBits,
    OpenStep::LoadNavigation,
    OpenStep::LoadRoads,
    OpenStep::ProbeCache,
    OpenStep::MuteAudioAndRender,
    OpenStep::MarkRestoreVolume,
    OpenStep::OpenBin,
    OpenStep::RestoreAudio,
    OpenStep::AdvanceCacheProgress,
    OpenStep::MarkMissionOpened,
    OpenStep::OpenCache,
    OpenStep::FinishProgress,
];

#[derive(Clone, Copy)]
struct Audio {
    identity: u32,
    volume: f32,
}

#[derive(Default)]
struct Ops {
    trace: Vec<OpenStep>,
    fail: Option<OpenStep>,
    media: bool,
    bin: bool,
    initial_cache: bool,
    final_cache: bool,
    cache_probes: usize,
    cache_consumed: bool,
    audio: Option<Audio>,
    audio_queries: Cell<usize>,
    after_render_audio: Option<Option<Audio>>,
    after_bin_audio: Option<Option<Audio>>,
    after_bin_volume: Option<f32>,
    after_bin_progress: Option<i32>,
    after_restore_progress: Option<i32>,
    configured_volume: f32,
    progress: i32,
    restore_requested: bool,
    saw_restore_at_bin: bool,
    mission_opened: bool,
    muted: Vec<u32>,
    restored: Vec<(u32, f32)>,
}

impl Ops {
    fn ready() -> Self {
        Self {
            media: true,
            bin: true,
            initial_cache: true,
            final_cache: true,
            audio: Some(Audio {
                identity: 1,
                volume: 0.5,
            }),
            configured_volume: 0.75,
            progress: 99,
            ..Self::default()
        }
    }

    fn enter(&mut self, step: OpenStep) -> Result<(), &'static str> {
        self.trace.push(step);
        if self.fail == Some(step) {
            return Err("synthetic failure");
        }
        Ok(())
    }
}

impl OpenOperations for Ops {
    type Error = &'static str;

    fn execute(&mut self, step: OpenStep) -> Result<(), Self::Error> {
        self.enter(step)?;
        match step {
            OpenStep::ProbeCache => {
                self.progress = 0;
                self.cache_probes += 1;
                if self.initial_cache {
                    self.progress = 1;
                }
            }
            OpenStep::MuteAudioAndRender => {
                let audio = self.audio.as_mut().expect("live audio before render");
                self.muted.push(audio.identity);
                audio.volume = 0.0;
                if let Some(replacement) = self.after_render_audio.take() {
                    self.audio = replacement;
                }
            }
            OpenStep::MarkRestoreVolume => self.restore_requested = true,
            OpenStep::RestoreAudio => {
                let audio = self.audio.as_mut().expect("live audio after OpenBin");
                audio.volume = self.configured_volume;
                self.restored.push((audio.identity, audio.volume));
                if let Some(progress) = self.after_restore_progress.take() {
                    self.progress = progress;
                }
            }
            OpenStep::AdvanceCacheProgress => {
                if self.progress == 1 {
                    self.progress = 2;
                }
            }
            OpenStep::MarkMissionOpened => self.mission_opened = true,
            OpenStep::OpenCache => {
                assert!(self.mission_opened);
                self.cache_probes += 1;
                self.cache_consumed = self.final_cache;
            }
            OpenStep::WaitForMedia | OpenStep::OpenBin => {
                panic!("result-bearing operation used the wrong entry point");
            }
            _ => {}
        }
        Ok(())
    }

    fn media_ready(&mut self) -> Result<bool, Self::Error> {
        self.enter(OpenStep::WaitForMedia)?;
        Ok(self.media)
    }

    fn open_bin(&mut self) -> Result<bool, Self::Error> {
        self.enter(OpenStep::OpenBin)?;
        self.saw_restore_at_bin = self.restore_requested;
        if let Some(replacement) = self.after_bin_audio.take() {
            self.audio = replacement;
        }
        if let Some(volume) = self.after_bin_volume.take() {
            self.configured_volume = volume;
        }
        if let Some(progress) = self.after_bin_progress.take() {
            self.progress = progress;
        }
        Ok(self.bin)
    }

    fn audio_present(&self) -> bool {
        self.audio_queries.set(self.audio_queries.get() + 1);
        self.audio.is_some()
    }
}

#[test]
fn success_keeps_reference_order_and_optional_branches() {
    let mut ops = Ops::ready();
    let mut attempt = OpenAttempt::default();
    assert!(!attempt.entered());
    assert_eq!(attempt.last(), None);
    assert_eq!(attempt.run(&mut ops), Ok(OpenResult::Opened));
    assert_eq!(ops.trace.as_slice(), &FULL_ORDER);
    assert!(attempt.entered());
    assert_eq!(attempt.last(), Some(OpenStep::FinishProgress));
    assert_eq!(ops.audio_queries.get(), 2);
    assert_eq!(ops.cache_probes, 2);
    assert_eq!(ops.progress, 2);
    assert!(ops.saw_restore_at_bin && ops.mission_opened && ops.cache_consumed);
}

#[test]
fn failed_bin_still_restores_volume_and_advances_progress_before_return() {
    let mut ops = Ops {
        bin: false,
        ..Ops::ready()
    };
    let mut attempt = OpenAttempt::default();
    assert_eq!(attempt.run(&mut ops), Ok(OpenResult::OpenBinFailed));
    assert_eq!(ops.trace.as_slice(), &FULL_ORDER[..18]);
    assert_eq!(attempt.last(), Some(OpenStep::AdvanceCacheProgress));
    assert_eq!(ops.restored, [(1, 0.75)]);
    assert_eq!(ops.progress, 2);
    assert_eq!(ops.cache_probes, 1);
    assert!(!ops.mission_opened && !ops.cache_consumed);
}

#[test]
fn media_early_exit_includes_post_close_reset_and_no_scene_effects() {
    let mut ops = Ops {
        media: false,
        ..Ops::ready()
    };
    let mut attempt = OpenAttempt::default();
    assert_eq!(attempt.run(&mut ops), Ok(OpenResult::NoMedia));
    assert_eq!(ops.trace.as_slice(), &FULL_ORDER[..4]);
    assert_eq!(attempt.last(), Some(OpenStep::WaitForMedia));
    assert_eq!(ops.audio_queries.get(), 0);
    assert_eq!(ops.cache_probes, 0);
    assert!(!ops.restore_requested && !ops.mission_opened);
}

#[test]
fn every_callback_error_stops_at_its_exact_prefix_and_cannot_replay() {
    for (index, step) in FULL_ORDER.into_iter().enumerate() {
        let mut ops = Ops {
            fail: Some(step),
            ..Ops::ready()
        };
        let mut attempt = OpenAttempt::default();
        assert_eq!(
            attempt.run(&mut ops),
            Err(OpenRunError::Callback(OpenFailure {
                step,
                error: "synthetic failure",
            })),
            "{step:?}"
        );
        assert_eq!(ops.trace.as_slice(), &FULL_ORDER[..=index]);
        assert_eq!(attempt.last(), Some(step));
        assert!(attempt.entered());
        let reached = ops.trace.clone();
        assert_eq!(attempt.run(&mut ops), Err(OpenRunError::AlreadyEntered));
        assert_eq!(ops.trace, reached);
        assert_eq!(attempt.last(), Some(step));
    }
}

#[test]
fn retry_after_each_native_result_has_no_callbacks_or_live_reads() {
    for (media, bin, result) in [
        (true, true, OpenResult::Opened),
        (true, false, OpenResult::OpenBinFailed),
        (false, true, OpenResult::NoMedia),
    ] {
        let mut ops = Ops {
            media,
            bin,
            ..Ops::ready()
        };
        let mut attempt = OpenAttempt::default();
        assert_eq!(attempt.run(&mut ops), Ok(result));
        let reached = ops.trace.clone();
        let reads = ops.audio_queries.get();
        assert_eq!(attempt.run(&mut ops), Err(OpenRunError::AlreadyEntered));
        assert_eq!(ops.trace, reached);
        assert_eq!(ops.audio_queries.get(), reads);
    }
}

#[test]
fn absent_audio_skips_its_callbacks_but_cache_is_still_tried_twice() {
    let mut ops = Ops {
        audio: None,
        initial_cache: false,
        final_cache: false,
        ..Ops::ready()
    };
    assert_eq!(OpenAttempt::default().run(&mut ops), Ok(OpenResult::Opened));
    let expected: Vec<_> = FULL_ORDER
        .into_iter()
        .filter(|step| !matches!(step, OpenStep::MuteAudioAndRender | OpenStep::RestoreAudio))
        .collect();
    assert_eq!(ops.trace, expected);
    assert_eq!(ops.cache_probes, 2);
    assert!(!ops.cache_consumed);
    assert!(ops.saw_restore_at_bin && ops.restore_requested);
    assert_eq!(ops.progress, 0);
}

#[test]
fn audio_presence_is_reread_after_both_native_open_bin_results() {
    for before in [false, true] {
        for after in [false, true] {
            for bin in [false, true] {
                let mut ops = Ops {
                    bin,
                    audio: before.then_some(Audio {
                        identity: 1,
                        volume: 0.5,
                    }),
                    after_bin_audio: Some(after.then_some(Audio {
                        identity: 2,
                        volume: 0.25,
                    })),
                    ..Ops::ready()
                };
                let expected = if bin {
                    OpenResult::Opened
                } else {
                    OpenResult::OpenBinFailed
                };
                assert_eq!(OpenAttempt::default().run(&mut ops), Ok(expected));
                assert_eq!(ops.muted, if before { vec![1] } else { vec![] });
                assert_eq!(
                    ops.restored,
                    if after { vec![(2, 0.75)] } else { vec![] }
                );
                assert_eq!(ops.audio_queries.get(), 2);
            }
        }
    }
}

#[test]
fn volume_restoration_uses_live_configuration_and_replacement_owner() {
    let mut ops = Ops {
        after_bin_audio: Some(Some(Audio {
            identity: 42,
            volume: 0.1,
        })),
        after_bin_volume: Some(0.125),
        ..Ops::ready()
    };
    OpenAttempt::default().run(&mut ops).unwrap();
    assert_eq!(ops.muted, [1]);
    assert_eq!(ops.restored, [(42, 0.125)]);
    assert_eq!(ops.audio.unwrap().volume, 0.125);
}

#[test]
fn audio_removal_during_render_is_visible_after_open_bin() {
    let mut ops = Ops {
        after_render_audio: Some(None),
        ..Ops::ready()
    };
    OpenAttempt::default().run(&mut ops).unwrap();
    assert_eq!(ops.muted, [1]);
    assert!(ops.restored.is_empty());
    assert!(!ops.trace.contains(&OpenStep::RestoreAudio));
    assert!(ops.saw_restore_at_bin);
}

#[test]
fn final_cache_open_does_not_reuse_the_initial_probe_result() {
    for initial_cache in [false, true] {
        for final_cache in [false, true] {
            let mut ops = Ops {
                initial_cache,
                final_cache,
                ..Ops::ready()
            };
            assert_eq!(OpenAttempt::default().run(&mut ops), Ok(OpenResult::Opened));
            assert_eq!(ops.cache_probes, 2);
            assert_eq!(ops.cache_consumed, final_cache);
            assert_eq!(ops.progress, if initial_cache { 2 } else { 0 });
            assert_eq!(ops.trace.as_slice(), &FULL_ORDER);
        }
    }
}

#[test]
fn progress_advance_uses_only_the_live_mode_after_open_bin() {
    for initial_cache in [false, true] {
        for mode in [-1, 0, 1, 2, 99] {
            for bin in [false, true] {
                let mut ops = Ops {
                    initial_cache,
                    bin,
                    after_bin_progress: Some(mode),
                    ..Ops::ready()
                };
                OpenAttempt::default().run(&mut ops).unwrap();
                assert_eq!(ops.progress, if mode == 1 { 2 } else { mode });
            }
        }
    }
}

#[test]
fn progress_reread_follows_the_audio_restoration_callback() {
    for mode in [0, 1, 2, 99] {
        let mut ops = Ops {
            after_bin_progress: Some(1),
            after_restore_progress: Some(mode),
            ..Ops::ready()
        };
        OpenAttempt::default().run(&mut ops).unwrap();
        assert_eq!(ops.progress, if mode == 1 { 2 } else { mode });
        let restore = ops
            .trace
            .iter()
            .position(|step| *step == OpenStep::RestoreAudio)
            .unwrap();
        assert_eq!(ops.trace[restore + 1], OpenStep::AdvanceCacheProgress);
    }
}

#[test]
fn restore_flag_is_set_before_open_bin_even_without_audio() {
    for bin in [false, true] {
        let mut ops = Ops {
            audio: None,
            bin,
            ..Ops::ready()
        };
        OpenAttempt::default().run(&mut ops).unwrap();
        let index = ops
            .trace
            .iter()
            .position(|step| *step == OpenStep::OpenBin)
            .unwrap();
        assert_eq!(ops.trace[index - 1], OpenStep::MarkRestoreVolume);
        assert!(ops.restore_requested && ops.saw_restore_at_bin);
    }
}

#[test]
fn failed_mute_does_not_premark_restore_volume_or_enter_open_bin() {
    let mut ops = Ops {
        fail: Some(OpenStep::MuteAudioAndRender),
        ..Ops::ready()
    };
    assert!(OpenAttempt::default().run(&mut ops).is_err());
    assert!(!ops.restore_requested);
    assert!(!ops.trace.contains(&OpenStep::OpenBin));
}

#[test]
fn host_open_bin_error_is_not_a_native_failure_or_compensation_request() {
    let mut ops = Ops {
        fail: Some(OpenStep::OpenBin),
        ..Ops::ready()
    };
    let mut attempt = OpenAttempt::default();
    assert_eq!(
        attempt.run(&mut ops),
        Err(OpenRunError::Callback(OpenFailure {
            step: OpenStep::OpenBin,
            error: "synthetic failure",
        }))
    );
    assert_eq!(ops.trace.as_slice(), &FULL_ORDER[..16]);
    assert!(ops.restore_requested);
    assert_eq!(ops.audio.unwrap().volume, 0.0);
    assert_eq!(ops.audio_queries.get(), 1);
    assert_eq!(ops.progress, 1);
    assert!(!ops.mission_opened);
}

#[test]
fn later_cache_error_keeps_the_already_written_open_state() {
    let mut ops = Ops {
        fail: Some(OpenStep::OpenCache),
        ..Ops::ready()
    };
    let mut attempt = OpenAttempt::default();
    assert!(attempt.run(&mut ops).is_err());
    assert_eq!(attempt.last(), Some(OpenStep::OpenCache));
    assert!(ops.mission_opened);
    assert_eq!(ops.restored, [(1, 0.75)]);
    assert_eq!(ops.progress, 2);
    assert!(!ops.trace.contains(&OpenStep::FinishProgress));
}
