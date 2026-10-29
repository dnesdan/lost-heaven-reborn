//! Source-opening regressions with authored PCM and deterministic I/O callbacks.
use super::*;
use std::cell::Cell;

fn wav(channels: u16) -> Vec<u8> {
    let samples = [8192i16, 16384, 0, -8192];
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(44u32.to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(4u32.to_le_bytes());
    bytes.extend((4 * u32::from(channels) * 2).to_le_bytes());
    bytes.extend((channels * 2).to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(8u32.to_le_bytes());
    bytes.extend(samples.into_iter().flat_map(i16::to_le_bytes));
    bytes
}

fn sound(audio: &RuntimeAudio, handle: u32) -> SoundId {
    audio.create_sound_at(handle).unwrap();
    audio.sound_id_from_handle(handle).unwrap()
}

fn loaded() -> (RuntimeAudio, SoundId) {
    let audio = RuntimeAudio::default();
    let id = sound(&audio, 7);
    audio
        .load_sound_source_with(&id, "a.wav", 8, || Ok(Some(wav(1))))
        .unwrap();
    (audio, id)
}

fn snapshot(audio: &RuntimeAudio, handle: u32) -> String {
    format!("{:?}", audio.sound(handle).unwrap())
}

fn replace(audio: &RuntimeAudio, old: &SoundId, handle: u32) -> SoundId {
    audio.release_sound(old).unwrap();
    let replacement = sound(audio, handle);
    audio.set_sound_volume(&replacement, 0.75).unwrap();
    audio.set_sound_enabled(&replacement, true).unwrap();
    replacement
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-source-identity-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn installation(&self) -> Installation {
        Installation::open(&self.0).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn retained_source_open_preserves_flags_and_fixed_pcm_output() {
    let (audio, id) = loaded();
    assert_eq!(audio.read_sound_word(7, 0x198).unwrap(), 0x218);
    assert_eq!(audio.read_sound_word(7, 0x140).unwrap(), 0x21ff);
    audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_enabled(&id, true).unwrap();
    let mut output = [0.0; 4];
    audio.mix_stereo(4, &mut output).unwrap();
    assert_eq!(output, [0.25, 0.25, 0.5, 0.5]);
    assert_eq!(audio.sound_id_from_handle(7).unwrap(), id);
}

#[test]
fn foreign_or_stale_id_is_rejected_before_read_or_source_release() {
    let (audio, old) = loaded();
    let replacement = replace(&audio, &old, 7);
    let other = RuntimeAudio::default();
    let foreign = sound(&other, 7);
    let before = snapshot(&audio, 7);
    let reads = Cell::new(0);
    for id in [&old, &foreign] {
        assert!(
            audio
                .load_sound_source_with(id, "other.wav", 0, || {
                    reads.set(reads.get() + 1);
                    Ok(Some(wav(1)))
                })
                .is_err()
        );
        assert_eq!(snapshot(&audio, 7), before);
    }
    assert_eq!(reads.get(), 0);
    audio.validate_sound_id(&replacement).unwrap();
}

#[test]
fn release_and_reuse_during_io_cannot_publish_to_replacement_or_cache() {
    let (audio, old) = loaded();
    let mut before = String::new();
    let result = audio.load_sound_source_with(&old, "new.wav", 0, || {
        assert!(audio.sound(7).unwrap().source.is_none());
        replace(&audio, &old, 7);
        before = snapshot(&audio, 7);
        Ok(Some(wav(1)))
    });
    assert_eq!(result, Err(AudioError("stale sound identity".into())));
    assert_eq!(snapshot(&audio, 7), before);
    assert!(!audio.lock().unwrap().sources.contains_key("new.wav"));
    assert_eq!(audio.live_source_count().unwrap(), 0);
}

#[test]
fn legacy_entry_also_retains_its_initial_incarnation_across_io() {
    let (audio, old) = loaded();
    let mut before = String::new();
    let result = audio.load_sound_with_optional(7, "new.wav", 0, || {
        replace(&audio, &old, 7);
        before = snapshot(&audio, 7);
        Ok(Some(wav(1)))
    });
    assert_eq!(result, Err(AudioError("stale sound identity".into())));
    assert_eq!(snapshot(&audio, 7), before);
    assert!(!audio.lock().unwrap().sources.contains_key("new.wav"));
}

#[test]
fn release_without_reuse_during_io_does_not_publish_a_source() {
    let (audio, old) = loaded();
    let result = audio.load_sound_source_with(&old, "new.wav", 0, || {
        audio.release_sound(&old).unwrap();
        Ok(Some(wav(1)))
    });
    assert_eq!(result, Err(AudioError("stale sound identity".into())));
    assert_eq!(audio.live_sound_count().unwrap(), 0);
    assert!(!audio.lock().unwrap().sources.contains_key("new.wav"));
}

#[test]
fn missing_file_does_not_hide_release_and_reuse_during_io() {
    let (audio, old) = loaded();
    let mut before = String::new();
    let result = audio.load_sound_source_with(&old, "missing.wav", 0, || {
        replace(&audio, &old, 7);
        before = snapshot(&audio, 7);
        Ok(None)
    });
    assert_eq!(result, Err(AudioError("stale sound identity".into())));
    assert_eq!(snapshot(&audio, 7), before);
}

#[test]
fn read_and_decode_errors_keep_their_reached_precedence() {
    for decode_failure in [false, true] {
        let (audio, old) = loaded();
        let mut before = String::new();
        let result = audio.load_sound_source_with(&old, "broken.wav", 0, || {
            replace(&audio, &old, 7);
            before = snapshot(&audio, 7);
            if decode_failure {
                Ok(Some(vec![0; 8]))
            } else {
                Err(AudioError("synthetic read failure".into()))
            }
        });
        let expected = if decode_failure {
            "invalid or unsupported PCM WAV"
        } else {
            "synthetic read failure"
        };
        assert_eq!(result, Err(AudioError(expected.into())));
        assert_eq!(snapshot(&audio, 7), before);
        assert!(!audio.lock().unwrap().sources.contains_key("broken.wav"));
    }
}

#[test]
fn missing_or_failed_replacement_keeps_old_voice_release_and_live_parameters() {
    for failure in 0..3 {
        let (audio, id) = loaded();
        audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
        audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
        audio.set_sound_playback(&id, true, true).unwrap();
        let flags = audio.read_sound_word(7, 0x140).unwrap();
        let result = audio.load_sound_source_with(&id, "broken.wav", 0, || {
            let state = audio.sound(7).unwrap();
            assert!(state.source.is_none());
            assert!(!state.voice_playing);
            assert_eq!(state.current_time_ms(), 0);
            match failure {
                0 => Ok(None),
                1 => Err(AudioError("synthetic read failure".into())),
                _ => Ok(Some(vec![0; 8])),
            }
        });
        if failure == 0 {
            assert_eq!(result, Ok(false));
        } else {
            assert!(result.is_err());
        }
        let state = audio.sound(7).unwrap();
        assert!(state.on);
        assert!(state.source.is_none());
        assert!(!state.voice_playing);
        assert_eq!(state.native_tail[0], flags);
        assert_eq!(audio.live_source_count().unwrap(), 0);
    }
}

#[test]
fn callback_parameter_and_listener_changes_are_read_live_on_attachment() {
    let (audio, id) = loaded();
    audio
        .load_sound_source_with(&id, "next.wav", 0, || {
            audio.set_sound_volume(&id, 0.75)?;
            audio.set_sound_frequency(&id, 2.0)?;
            audio.set_sound_kind(&id, SoundKind::Ambient)?;
            audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None)?;
            audio.set_sound_enabled(&id, true)?;
            Ok(Some(wav(1)))
        })
        .unwrap();
    let state = audio.sound(7).unwrap();
    assert_eq!(
        (state.volume, state.frequency, state.sound_type),
        (0.75, 2.0, 3)
    );
    assert_eq!(state.state, PlaybackState::Play);
    // Original Open overwrites dirty/play bits after its internal SetOn/Seek.
    assert_eq!(state.native_tail[0], 0x21ff);
    assert!(!state.voice_playing);
}

#[test]
fn exact_name_preserves_voice_and_case_cache_hit_keeps_first_source_flags() {
    let (audio, id) = loaded();
    audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_playback(&id, true, true).unwrap();
    audio.mix_stereo(4, &mut [0.0; 2]).unwrap();
    let before = snapshot(&audio, 7);
    audio
        .load_sound_source_with(&id, "a.wav", 0, || panic!("exact name must not read"))
        .unwrap();
    assert_eq!(snapshot(&audio, 7), before);
    let keeper = sound(&audio, 8);
    audio
        .load_sound_source_with(&keeper, "A.WAV", 0, || panic!("cache hit must not read"))
        .unwrap();
    audio
        .load_sound_source_with(&id, "A.WAV", 0, || panic!("cache hit must not read"))
        .unwrap();
    let reopened = audio.sound(7).unwrap();
    assert_eq!(reopened.current_time_ms(), 0);
    assert_eq!(reopened.open_flags, 0x218);
    assert!(!reopened.voice_playing);
    assert!(Arc::ptr_eq(
        reopened.source.as_ref().unwrap(),
        audio.sound(8).unwrap().source.as_ref().unwrap()
    ));
}

#[test]
fn stereo_open_keeps_original_kind_coercion_and_source_flags() {
    let audio = RuntimeAudio::default();
    let id = sound(&audio, 7);
    audio.set_sound_kind(&id, SoundKind::Spot).unwrap();
    audio
        .load_sound_source_with(&id, "stereo.wav", 8, || Ok(Some(wav(2))))
        .unwrap();
    let state = audio.sound(7).unwrap();
    assert_eq!(state.sound_type, 3);
    assert_eq!(state.open_flags, 0x18);
    assert_eq!(state.source.as_ref().unwrap().channels, 2);
}

#[test]
fn cache_reopen_uses_source_identity_and_preserves_destination_parameters() {
    let scratch = Scratch::new();
    let installation = scratch.installation();
    let (audio, source) = loaded();
    let destination = sound(&audio, 8);
    audio.set_sound_volume(&destination, 0.75).unwrap();
    audio.set_sound_frequency(&destination, 1.5).unwrap();
    assert_eq!(
        audio
            .reopen_cached_sound(&destination, &source, &installation)
            .unwrap(),
        0
    );
    let target = audio.sound(8).unwrap();
    assert_eq!((target.volume, target.frequency), (0.75, 1.5));
    assert!(Arc::ptr_eq(
        target.source.as_ref().unwrap(),
        audio.sound(7).unwrap().source.as_ref().unwrap()
    ));
    assert_eq!(audio.sound_id_from_handle(8).unwrap(), destination);
}

#[test]
fn cache_reopen_rejects_stale_source_before_touching_destination() {
    let scratch = Scratch::new();
    let installation = scratch.installation();
    let (audio, source) = loaded();
    let destination = sound(&audio, 8);
    replace(&audio, &source, 7);
    let before = snapshot(&audio, 8);
    assert_eq!(
        audio.reopen_cached_sound(&destination, &source, &installation),
        Err(AudioError("stale sound identity".into()))
    );
    assert_eq!(snapshot(&audio, 8), before);
}

#[test]
fn cache_reopen_rejects_stale_destination_without_refreshing_it() {
    let scratch = Scratch::new();
    let installation = scratch.installation();
    let (audio, source) = loaded();
    let destination = sound(&audio, 8);
    replace(&audio, &destination, 8);
    let before = snapshot(&audio, 8);
    assert_eq!(
        audio.reopen_cached_sound(&destination, &source, &installation),
        Err(AudioError("stale sound identity".into()))
    );
    assert_eq!(snapshot(&audio, 8), before);
}

#[test]
fn original_open_keeps_missing_hresult_and_path_error_before_identity() {
    let scratch = Scratch::new();
    let installation = scratch.installation();
    let (audio, id) = loaded();
    assert_eq!(
        audio
            .open_sound_source(&id, "absent.wav", 0, &installation)
            .unwrap(),
        0x8000_0001u32 as i32
    );
    replace(&audio, &id, 7);
    assert_eq!(
        audio.open_sound_source(&id, "\u{e9}.wav", 0, &installation),
        Err(AudioError(
            "non-ASCII sound path requires original filename encoding".into()
        ))
    );
    assert_eq!(
        audio.open_sound_source(&id, "absent.wav", 0, &installation),
        Err(AudioError("stale sound identity".into()))
    );
}

#[test]
fn self_cache_reopen_preserves_pcm_voice_and_retained_identity() {
    let scratch = Scratch::new();
    let installation = scratch.installation();
    let (audio, id) = loaded();
    audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_playback(&id, true, true).unwrap();
    audio.mix_stereo(4, &mut [0.0; 2]).unwrap();
    let before = snapshot(&audio, 7);
    let source = audio.sound(7).unwrap().source.unwrap();
    // There is no a.wav in this installation. Self-open must use its exact-name
    // fast path, not detach and reload, even when source and destination alias.
    assert_eq!(audio.reopen_cached_sound(&id, &id, &installation), Ok(0));
    assert_eq!(snapshot(&audio, 7), before);
    assert_eq!(audio.sound_position_ms(&id).unwrap(), 250);
    assert!(Arc::ptr_eq(
        &source,
        audio.sound(7).unwrap().source.as_ref().unwrap()
    ));
    assert_eq!(audio.sound_id_from_handle(7).unwrap(), id);
}

#[test]
fn source_channel_count_controls_cone_reset_without_changing_live_parameters() {
    for channels in [1, 2] {
        let audio = RuntimeAudio::default();
        let id = sound(&audio, 7);
        audio.set_sound_kind(&id, SoundKind::Spot).unwrap();
        audio.set_sound_cone(&id, [0.25, 0.5]).unwrap();
        audio.set_sound_volume(&id, 0.75).unwrap();
        audio
            .load_sound_source_with(&id, "channels.wav", 8, || Ok(Some(wav(channels))))
            .unwrap();
        let state = audio.sound(7).unwrap();
        let expected_cone = if channels == 1 {
            [std::f32::consts::FRAC_PI_3, std::f32::consts::FRAC_PI_2]
        } else {
            [0.25, 0.5]
        };
        assert_eq!(state.cone, expected_cone);
        assert_eq!(state.volume, 0.75);
        assert_eq!(state.sound_type, if channels == 1 { 2 } else { 3 });
        assert_eq!(state.open_flags, if channels == 1 { 0x218 } else { 0x18 });
        audio.validate_sound_id(&id).unwrap();
    }
}
