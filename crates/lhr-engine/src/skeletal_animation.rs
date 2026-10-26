//! Owned, bounded projection of original animation record-to-frame binding.
//!
//! SetAnimation RVA0x1e580, LS3DF SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Records visit the frame list in source order and select the first exact,
//! case-sensitive name match. A later record for that same frame replaces its
//! channel binding without changing evaluator order. Reference counting,
//! controller replacement/restart, native allocation and cleanup are not ported.
//! Sampling is pure: the caller applies outputs and advances its own clock.

use crate::{
    frame_transform::FrameKey,
    native_transform::SQuat,
    rotation_animation::{sample_rotation_channel, RotationUnavailable},
};
use lhr_assets::{animation::AnimationSet, Model};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SkeletalAnimationError {
    FrameKeyCount,
    DuplicateFrameKey {
        frame_index: usize,
    },
    EmbeddedNameTerminator,
    UnsupportedTrack {
        track_index: usize,
        flags: u16,
    },
    MissingChannel {
        track_index: usize,
    },
    TimeOutsideDuration {
        current_ms: u32,
        duration_ms: u32,
    },
    Position {
        track_index: usize,
        message: String,
    },
    Rotation {
        track_index: usize,
        source: RotationUnavailable,
    },
    AllocationOverflow,
}

/// Final channel source for one evaluator, ordered by first frame encounter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkeletalBinding {
    pub track_index: usize,
    pub frame_index: usize,
    pub frame: FrameKey,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampledFrame {
    pub frame: FrameKey,
    pub position: Option<[f32; 3]>,
    /// Already normalized by the recovered evaluator destination path.
    pub rotation: Option<SQuat>,
}

/// Retains immutable source identities. Model storage and animation storage are
/// shared through Arc; callers must budget each shared source once separately.
#[derive(Debug)]
pub struct SkeletalAnimation {
    model: Arc<Model>,
    animation: Arc<AnimationSet>,
    bindings: Vec<SkeletalBinding>,
}

impl SkeletalAnimation {
    /// `source_frame_keys` corresponds exactly to `model.frames` source order.
    /// Only final matched tracks with flags2,4,6 are admitted. Unmatched tracks
    /// are ignored, as in the original record scan. Duplicate records overwrite
    /// before channel validation, so an earlier replaced channel is not played.
    pub fn new(
        model: &Arc<Model>,
        animation: Arc<AnimationSet>,
        source_frame_keys: &[FrameKey],
    ) -> Result<Self, SkeletalAnimationError> {
        if model.frames.len() != source_frame_keys.len() {
            return Err(SkeletalAnimationError::FrameKeyCount);
        }
        let mut keys = HashSet::with_capacity(source_frame_keys.len());
        for (frame_index, frame) in source_frame_keys.iter().enumerate() {
            if !keys.insert(frame.0) {
                return Err(SkeletalAnimationError::DuplicateFrameKey { frame_index });
            }
        }
        // Native matching uses C strings; reject embedded terminators instead
        // of inventing equality for bytes following a native string terminator.
        if model
            .frames
            .iter()
            .any(|frame| frame.name_bytes.contains(&0))
            || animation.tracks.iter().any(|track| track.name.contains(&0))
        {
            return Err(SkeletalAnimationError::EmbeddedNameTerminator);
        }
        let mut bindings: Vec<SkeletalBinding> =
            Vec::with_capacity(animation.tracks.len().min(model.frames.len()));
        for (track_index, track) in animation.tracks.iter().enumerate() {
            let Some(frame_index) = model
                .frames
                .iter()
                .position(|frame| frame.name_bytes == track.name)
            else {
                continue;
            };
            let binding = SkeletalBinding {
                track_index,
                frame_index,
                frame: source_frame_keys[frame_index],
            };
            if let Some(previous) = bindings.iter_mut().find(|b| b.frame_index == frame_index) {
                *previous = binding;
            } else {
                bindings.push(binding);
            }
        }
        for binding in &bindings {
            let track = &animation.tracks[binding.track_index];
            if !matches!(track.flags, 2 | 4 | 6) || track.scale.is_some() {
                return Err(SkeletalAnimationError::UnsupportedTrack {
                    track_index: binding.track_index,
                    flags: track.flags,
                });
            }
            if (track.flags & 2 != 0) != track.position.is_some()
                || (track.flags & 4 != 0) != track.rotation.is_some()
            {
                return Err(SkeletalAnimationError::MissingChannel {
                    track_index: binding.track_index,
                });
            }
        }
        Ok(Self {
            model: Arc::clone(model),
            animation,
            bindings,
        })
    }

    pub fn model(&self) -> &Arc<Model> {
        &self.model
    }

    pub fn animation(&self) -> &Arc<AnimationSet> {
        &self.animation
    }

    pub fn bindings(&self) -> &[SkeletalBinding] {
        &self.bindings
    }

    pub fn duration_ms(&self) -> u32 {
        self.animation.duration_ms()
    }

    /// Binding owner plus its actual vector capacity; excludes shared model,
    /// shared animation, Arc control blocks and the temporary sample result.
    pub fn storage_bytes(&self) -> Result<usize, SkeletalAnimationError> {
        self.bindings
            .capacity()
            .checked_mul(std::mem::size_of::<SkeletalBinding>())
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Self>()))
            .ok_or(SkeletalAnimationError::AllocationOverflow)
    }

    /// Animation payload accounting for the caller that owns its shared-source
    /// budget. Includes every retained record, including unmatched channels.
    /// Excludes Arc control blocks and model storage. Charge this once per Arc.
    pub fn animation_storage_bytes(&self) -> Result<usize, SkeletalAnimationError> {
        let mut bytes = self
            .animation
            .tracks
            .capacity()
            .checked_mul(std::mem::size_of::<lhr_assets::animation::AnimationTrack>())
            .and_then(|n| n.checked_add(std::mem::size_of::<AnimationSet>()))
            .ok_or(SkeletalAnimationError::AllocationOverflow)?;
        for track in &self.animation.tracks {
            bytes = bytes
                .checked_add(track.name.capacity())
                .ok_or(SkeletalAnimationError::AllocationOverflow)?;
            for channel in [
                track.position.as_ref().map(|c| c.storage_bytes()),
                track.rotation.as_ref().map(|c| c.storage_bytes()),
                track.scale.as_ref().map(|c| c.storage_bytes()),
            ]
            .into_iter()
            .flatten()
            {
                let size = channel.map_err(|_| SkeletalAnimationError::AllocationOverflow)?;
                bytes = bytes
                    .checked_add(size)
                    .ok_or(SkeletalAnimationError::AllocationOverflow)?;
            }
        }
        Ok(bytes)
    }

    /// Return one result per bound frame, in evaluator creation order. No
    /// looping or clock conversion occurs. The endpoint is admitted; a caller
    /// must handle controller boundaries before sampling outside this duration.
    /// An error returns no partial pose and does not modify either source.
    pub fn sample(&self, current_ms: u32) -> Result<Vec<SampledFrame>, SkeletalAnimationError> {
        let duration_ms = self.duration_ms();
        if current_ms > duration_ms {
            return Err(SkeletalAnimationError::TimeOutsideDuration {
                current_ms,
                duration_ms,
            });
        }
        let mut output = Vec::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            let track = &self.animation.tracks[binding.track_index];
            let position = track
                .position
                .as_ref()
                .map(|channel| channel.sample_position_channel(current_ms))
                .transpose()
                .map_err(|error| SkeletalAnimationError::Position {
                    track_index: binding.track_index,
                    message: error.to_string(),
                })?;
            let rotation = track
                .rotation
                .as_ref()
                .map(|channel| sample_rotation_channel(channel, current_ms))
                .transpose()
                .map_err(|source| SkeletalAnimationError::Rotation {
                    track_index: binding.track_index,
                    source,
                })?;
            output.push(SampledFrame {
                frame: binding.frame,
                position,
                rotation,
            });
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(names: &[&[u8]]) -> Arc<Model> {
        Arc::new(Model {
            materials: vec![],
            frames: names
                .iter()
                .map(|name| lhr_assets::Frame {
                    kind: 10,
                    visual: None,
                    render_flags: 0,
                    logic_flags: 0,
                    culling: 0,
                    name: String::from_utf8_lossy(name).into_owned(),
                    name_bytes: name.to_vec(),
                    properties: vec![],
                    dummy_bounds: None,
                    light: None,
                    joint: None,
                    target: None,
                    occluder: None,
                    sector: None,
                    skin: None,
                    morph: None,
                    lensflare: None,
                    parent: None,
                    position: [7., 8., 9.],
                    scale: [1.; 3],
                    rotation_wxyz: [1., 0., 0., 0.],
                    instance: None,
                    lods: vec![],
                })
                .collect(),
        })
    }

    fn animation(records: &[(&[u8], u16)], spherical: bool) -> AnimationSet {
        let mut payload = vec![0u8; 4 + records.len() * 8];
        payload[..2].copy_from_slice(&(records.len() as u16).to_le_bytes());
        payload[2..4].copy_from_slice(&10u16.to_le_bytes());
        for (index, (name, flags)) in records.iter().enumerate() {
            let offset = payload.len() as u32;
            payload[4 + index * 8..8 + index * 8].copy_from_slice(&offset.to_le_bytes());
            payload.extend_from_slice(name);
            payload.push(0);
            let offset = payload.len() as u32;
            payload[8 + index * 8..12 + index * 8].copy_from_slice(&offset.to_le_bytes());
            payload.extend(flags.to_le_bytes());
            payload.extend(0u16.to_le_bytes());
            for channel in [4, 2, 8] {
                if flags & channel == 0 {
                    continue;
                }
                payload.extend(2u16.to_le_bytes());
                payload.extend(0u16.to_le_bytes());
                payload.extend(10u16.to_le_bytes());
                payload.extend(0u16.to_le_bytes());
                let values: Vec<f32> = if channel == 4 {
                    if spherical {
                        vec![1., 0., 0., 0., 0., 1., 0., 0.]
                    } else {
                        vec![1., 0., 0., 0., 1., 0.001, 0., 0.]
                    }
                } else {
                    vec![0., 0., 0., 2., 4., 6.]
                };
                for value in values {
                    payload.extend(value.to_le_bytes());
                }
            }
        }
        let mut data = b"5DS\0".to_vec();
        data.extend(20u16.to_le_bytes());
        data.extend([0u8; 8]);
        data.extend((payload.len() as u32).to_le_bytes());
        data.extend(payload);
        lhr_assets::animation::parse_5ds(&data).unwrap()
    }

    #[test]
    fn exact_raw_names_first_frame_last_record_and_source_order() {
        let source = model(&[b"same", b"same", b"A", b"\xff"]);
        let animation = Arc::new(animation(
            &[
                (b"A", 2),
                (b"same", 4),
                (b"missing", 2),
                (b"same", 6),
                (b"\xff", 2),
            ],
            false,
        ));
        let binding = SkeletalAnimation::new(
            &source,
            Arc::clone(&animation),
            &[FrameKey(8), FrameKey(9), FrameKey(10), FrameKey(11)],
        )
        .unwrap();
        assert!(Arc::ptr_eq(binding.model(), &source));
        assert!(Arc::ptr_eq(binding.animation(), &animation));
        assert_eq!(
            binding.bindings(),
            &[
                SkeletalBinding {
                    track_index: 0,
                    frame_index: 2,
                    frame: FrameKey(10)
                },
                SkeletalBinding {
                    track_index: 3,
                    frame_index: 0,
                    frame: FrameKey(8)
                },
                SkeletalBinding {
                    track_index: 4,
                    frame_index: 3,
                    frame: FrameKey(11)
                },
            ]
        );
        let sample = binding.sample(200).unwrap();
        assert_eq!(
            sample.iter().map(|value| value.frame).collect::<Vec<_>>(),
            [FrameKey(10), FrameKey(8), FrameKey(11)]
        );
        assert_eq!(sample[1].position, Some([1., 2., 3.]));
        assert!(sample[1].rotation.is_some());
        assert_eq!(source.frames[0].position, [7., 8., 9.]);
    }

    #[test]
    fn structural_and_channel_errors_are_explicit() {
        let source = model(&[b"a", b"A"]);
        let parsed = animation(&[(b"a", 2)], false);
        assert!(matches!(
            SkeletalAnimation::new(&source, Arc::new(parsed.clone()), &[]),
            Err(SkeletalAnimationError::FrameKeyCount)
        ));
        assert!(matches!(
            SkeletalAnimation::new(
                &source,
                Arc::new(parsed.clone()),
                &[FrameKey(1), FrameKey(1)]
            ),
            Err(SkeletalAnimationError::DuplicateFrameKey { frame_index: 1 })
        ));
        let mut missing = parsed.clone();
        missing.tracks[0].flags = 4;
        assert!(matches!(
            SkeletalAnimation::new(&source, Arc::new(missing), &[FrameKey(1), FrameKey(2)]),
            Err(SkeletalAnimationError::MissingChannel { track_index: 0 })
        ));
        let scale = animation(&[(b"a", 10)], false);
        assert!(matches!(
            SkeletalAnimation::new(&source, Arc::new(scale), &[FrameKey(1), FrameKey(2)]),
            Err(SkeletalAnimationError::UnsupportedTrack { flags: 10, .. })
        ));
        let mut notes = parsed;
        notes.tracks[0].flags = 16;
        assert!(matches!(
            SkeletalAnimation::new(&source, Arc::new(notes), &[FrameKey(1), FrameKey(2)]),
            Err(SkeletalAnimationError::UnsupportedTrack { flags: 16, .. })
        ));
        let binding = SkeletalAnimation::new(
            &source,
            Arc::new(animation(&[(b"A", 2)], false)),
            &[FrameKey(1), FrameKey(2)],
        )
        .unwrap();
        assert_eq!(binding.bindings()[0].frame, FrameKey(2));
        assert!(binding.sample(400).is_ok());
        assert!(matches!(
            binding.sample(401),
            Err(SkeletalAnimationError::TimeOutsideDuration {
                current_ms: 401,
                duration_ms: 400
            })
        ));
        let nul_source = model(&[b"a\0suffix"]);
        assert!(matches!(
            SkeletalAnimation::new(
                &nul_source,
                Arc::new(animation(&[(b"a", 2)], false)),
                &[FrameKey(1)]
            ),
            Err(SkeletalAnimationError::EmbeddedNameTerminator)
        ));
    }

    #[test]
    fn unmatched_and_replaced_records_do_not_play_unavailable_channels() {
        let source = model(&[b"a"]);
        let parsed = animation(&[(b"missing", 8), (b"a", 8), (b"a", 2)], false);
        let binding = SkeletalAnimation::new(&source, Arc::new(parsed), &[FrameKey(3)]).unwrap();
        assert_eq!(binding.bindings()[0].track_index, 2);
        assert_eq!(binding.sample(200).unwrap()[0].position, Some([1., 2., 3.]));
        let no_match = SkeletalAnimation::new(
            &source,
            Arc::new(animation(&[(b"A", 8)], false)),
            &[FrameKey(3)],
        )
        .unwrap();
        assert!(no_match.sample(0).unwrap().is_empty());
    }

    #[test]
    fn spherical_rotation_failure_returns_no_partial_pose() {
        let source = model(&[b"a", b"b"]);
        let parsed = animation(&[(b"a", 2), (b"b", 4)], true);
        let binding =
            SkeletalAnimation::new(&source, Arc::new(parsed), &[FrameKey(1), FrameKey(2)]).unwrap();
        assert!(matches!(
            binding.sample(200),
            Err(SkeletalAnimationError::Rotation {
                track_index: 1,
                source: RotationUnavailable::SphericalInterpolation
            })
        ));
        assert_eq!(source.frames[0].position, [7., 8., 9.]);
    }

    #[test]
    fn accounting_includes_capacity_and_shared_animation_separately() {
        let source = model(&[b"a"]);
        let binding = SkeletalAnimation::new(
            &source,
            Arc::new(animation(&[(b"a", 6), (b"unmatched", 8)], false)),
            &[FrameKey(1)],
        )
        .unwrap();
        assert_eq!(
            binding.storage_bytes().unwrap(),
            std::mem::size_of::<SkeletalAnimation>()
                + binding.bindings.capacity() * std::mem::size_of::<SkeletalBinding>()
        );
        let animation = binding.animation();
        let mut expected = std::mem::size_of::<AnimationSet>()
            + animation.tracks.capacity()
                * std::mem::size_of::<lhr_assets::animation::AnimationTrack>();
        for track in &animation.tracks {
            expected += track.name.capacity();
            if let Some(channel) = &track.position {
                expected += channel.storage_bytes().unwrap();
            }
            if let Some(channel) = &track.rotation {
                expected += channel.storage_bytes().unwrap();
            }
            if let Some(channel) = &track.scale {
                expected += channel.storage_bytes().unwrap();
            }
        }
        assert_eq!(binding.animation_storage_bytes().unwrap(), expected);
    }
}
