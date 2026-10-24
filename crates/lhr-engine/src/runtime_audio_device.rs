//! Native output of already mixed PCM through CPAL (ALSA/CoreAudio).
//! The device consumes queued samples; it never advances gameplay's mixer.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

use super::{AudioError, Result};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceAudioStatus {
    pub pushed_frames: u64,
    pub played_frames: u64,
    pub underflow_frames: u64,
    pub discarded_frames: u64,
    pub queued_frames: usize,
    pub stream_error: Option<String>,
}

struct Queue {
    samples: VecDeque<[f32; 2]>,
    status: DeviceAudioStatus,
}

pub struct DeviceAudioOutput {
    stream: cpal::Stream,
    queue: Arc<Mutex<Queue>>,
    sample_rate: u32,
    started: bool,
    device_name: String,
}

impl DeviceAudioOutput {
    /// Uses the requested rate so capture and device consume identical PCM.
    /// Missing devices and unsupported formats are explicit errors.
    pub fn open_default(sample_rate: u32) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| AudioError("no native audio output device".into()))?;
        let device_name = device
            .name()
            .map_err(|e| AudioError(format!("audio device name: {e}")))?;
        let supported = device
            .supported_output_configs()
            .map_err(|e| AudioError(format!("audio device formats: {e}")))?;
        let mut candidates: Vec<_> = supported
            .filter(|c| {
                c.channels() >= 1
                    && c.channels() <= 8
                    && c.min_sample_rate().0 <= sample_rate
                    && c.max_sample_rate().0 >= sample_rate
                    && matches!(
                        c.sample_format(),
                        cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::U16
                    )
            })
            .collect();
        candidates.sort_by_key(|c| {
            (
                c.channels() != 2,
                c.sample_format() != cpal::SampleFormat::F32,
                c.channels(),
            )
        });
        let supported = candidates
            .into_iter()
            .next()
            .ok_or_else(|| {
                AudioError(format!(
                    "native audio device does not support {sample_rate} Hz PCM"
                ))
            })?
            .with_sample_rate(cpal::SampleRate(sample_rate));
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let queue = Arc::new(Mutex::new(Queue {
            samples: VecDeque::with_capacity(sample_rate as usize * 2),
            status: DeviceAudioStatus::default(),
        }));
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, queue.clone()),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, queue.clone()),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, queue.clone()),
            _ => unreachable!(),
        }?;
        Ok(Self {
            stream,
            queue,
            sample_rate,
            started: false,
            device_name,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Queues up to two seconds; overflow is reported rather than dropping PCM
    /// and allowing a capture to conceal a broken live output path.
    pub fn push(&mut self, samples: &[f32]) -> Result<()> {
        if !samples.len().is_multiple_of(2) || !samples.iter().all(|s| s.is_finite()) {
            return Err(AudioError("invalid PCM for native audio output".into()));
        }
        let mut queue = self
            .queue
            .lock()
            .map_err(|_| AudioError("audio output queue poisoned".into()))?;
        if let Some(error) = &queue.status.stream_error {
            return Err(AudioError(format!("native audio stream: {error}")));
        }
        if queue.samples.len().saturating_add(samples.len() / 2) > self.sample_rate as usize * 2 {
            return Err(AudioError("native audio queue exceeded two seconds".into()));
        }
        queue
            .samples
            .extend(samples.as_chunks::<2>().0.iter().map(|s| [s[0], s[1]]));
        queue.status.pushed_frames = queue
            .status
            .pushed_frames
            .saturating_add((samples.len() / 2) as u64);
        drop(queue);
        if !self.started {
            self.stream
                .play()
                .map_err(|e| AudioError(format!("start native audio stream: {e}")))?;
            self.started = true;
        }
        Ok(())
    }

    pub fn status(&self) -> Result<DeviceAudioStatus> {
        let queue = self
            .queue
            .lock()
            .map_err(|_| AudioError("audio output queue poisoned".into()))?;
        let mut status = queue.status.clone();
        status.queued_frames = queue.samples.len();
        Ok(status)
    }

    /// A reload discards queued frames from the released car explicitly.
    pub fn clear_queue(&mut self) -> Result<usize> {
        let mut queue = self
            .queue
            .lock()
            .map_err(|_| AudioError("audio output queue poisoned".into()))?;
        let discarded = queue.samples.len();
        queue.samples.clear();
        queue.status.discarded_frames = queue
            .status
            .discarded_frames
            .saturating_add(discarded as u64);
        Ok(discarded)
    }
}

fn build<T: Sample + SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<Mutex<Queue>>,
) -> Result<cpal::Stream> {
    let channels = usize::from(config.channels);
    let errors = queue.clone();
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                let Ok(mut queue) = queue.lock() else {
                    output.fill(T::from_sample(0.0));
                    return;
                };
                for frame in output.chunks_mut(channels) {
                    let pcm = match queue.samples.pop_front() {
                        Some(pcm) => {
                            queue.status.played_frames =
                                queue.status.played_frames.saturating_add(1);
                            pcm
                        }
                        None => {
                            queue.status.underflow_frames =
                                queue.status.underflow_frames.saturating_add(1);
                            [0.0, 0.0]
                        }
                    };
                    if channels == 1 {
                        frame[0] = T::from_sample((pcm[0] + pcm[1]) * 0.5);
                    } else {
                        frame[0] = T::from_sample(pcm[0]);
                        frame[1] = T::from_sample(pcm[1]);
                        for channel in &mut frame[2..] {
                            *channel = T::from_sample(0.0);
                        }
                    }
                }
            },
            move |error| {
                if let Ok(mut queue) = errors.lock() {
                    queue.status.stream_error = Some(error.to_string());
                }
            },
            None,
        )
        .map_err(|e| AudioError(format!("create native audio stream: {e}")))
}
