//! Authored WAVs and deterministic reader callbacks, not original-game evidence.
use super::*;
use crate::frame_transform::FrameKey;
use crate::runtime_audio::{PlaybackQuery, PlaybackState, SoundKind};
use std::cell::RefCell;

fn wav(channels: u16) -> Vec<u8> {
    let samples = vec![8192i16; usize::from(channels) * 16];
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + samples.len() as u32 * 2).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(4u32.to_le_bytes());
    bytes.extend((4 * u32::from(channels) * 2).to_le_bytes());
    bytes.extend((channels * 2).to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((samples.len() as u32 * 2).to_le_bytes());
    bytes.extend(samples.into_iter().flat_map(i16::to_le_bytes));
    bytes
}

fn loaded() -> (RuntimeAudio, SoundId) {
    let audio = RuntimeAudio::default();
    let id = audio.create_scene_sound(FrameKey(7)).unwrap();
    assert!(audio
        .load_sound_with_optional_id(&id, "a.wav", 0, || Ok(Some(wav(1))))
        .unwrap());
    (audio, id)
}

fn snapshot(audio: &RuntimeAudio) -> String {
    format!("{:?}", audio.sound(7).unwrap())
}

#[test]
fn released_and_reused_target_is_not_written_or_published_after_read() {
    for legacy in [false, true] {
        let (audio, id) = loaded();
        let replacement_state = RefCell::new(String::new());
        let read = || {
            assert!(audio.sound(7).unwrap().source.is_none());
            audio.release_sound(&id).unwrap();
            let replacement = audio.create_scene_sound(FrameKey(7)).unwrap();
            audio.set_sound_volume(&replacement, 0.75).unwrap();
            audio.set_sound_enabled(&replacement, true).unwrap();
            *replacement_state.borrow_mut() = snapshot(&audio);
            Ok(Some(wav(1)))
        };
        let result = if legacy {
            audio.load_sound_with_optional(7, "new.wav", 8, read)
        } else {
            audio.load_sound_with_optional_id(&id, "new.wav", 8, read)
        };
        assert_eq!(result, Err(AudioError("stale sound identity".into())));
        assert_eq!(snapshot(&audio), *replacement_state.borrow());
        assert!(!audio.lock().unwrap().sources.contains_key("new.wav"));
        assert_eq!(audio.live_source_count().unwrap(), 0);
        assert_eq!(audio.live_voice_count().unwrap(), 0);
    }
}

#[test]
fn missing_file_does_not_hide_release_or_handle_reuse_during_read() {
    for recreate in [false, true] {
        let (audio, id) = loaded();
        let result = audio.load_sound_with_optional_id(&id, "missing.wav", 0, || {
            audio.release_sound(&id).unwrap();
            if recreate {
                audio.create_sound_at(7).unwrap();
            }
            Ok(None)
        });
        assert_eq!(result, Err(AudioError("stale sound identity".into())));
        assert_eq!(audio.live_sound_count().unwrap(), usize::from(recreate));
        assert!(!audio.lock().unwrap().sources.contains_key("missing.wav"));
    }
}

#[test]
fn successful_read_after_release_without_replacement_stays_unpublished() {
    let (audio, id) = loaded();
    let result = audio.load_sound_with_optional_id(&id, "new.wav", 0, || {
        audio.release_sound(&id).unwrap();
        Ok(Some(wav(1)))
    });
    assert_eq!(result, Err(AudioError("stale sound identity".into())));
    assert_eq!(audio.live_sound_count().unwrap(), 0);
    assert!(!audio.lock().unwrap().sources.contains_key("new.wav"));
}

#[test]
fn foreign_and_stale_ids_fail_before_reader_or_destination_mutation() {
    let (audio, id) = loaded();
    let (other, foreign) = loaded();
    let before = snapshot(&audio);
    assert_eq!(
        audio.load_sound_with_optional_id(&foreign, "new.wav", 0, || panic!("unexpected I/O")),
        Err(AudioError("sound belongs to a different audio owner".into()))
    );
    assert_eq!(snapshot(&audio), before);
    audio.release_sound(&id).unwrap();
    let replacement = audio.create_scene_sound(FrameKey(7)).unwrap();
    audio.set_sound_volume(&replacement, 0.75).unwrap();
    let before = snapshot(&audio);
    assert_eq!(
        audio.load_sound_with_optional_id(&id, "new.wav", 0, || panic!("unexpected I/O")),
        Err(AudioError("stale sound identity".into()))
    );
    assert_eq!(snapshot(&audio), before);
    other.validate_sound_id(&foreign).unwrap();
}

#[test]
fn reader_errors_keep_the_reached_detach_and_do_not_restore_old_pcm() {
    for malformed in [false, true] {
        let (audio, id) = loaded();
        audio.set_sound_volume(&id, 0.75).unwrap();
        let old = Arc::downgrade(audio.sound(7).unwrap().source.as_ref().unwrap());
        let result = audio.load_sound_with_optional_id(&id, "bad.wav", 0, || {
            assert!(audio.0.try_lock().is_ok());
            assert!(audio.sound(7).unwrap().source.is_none());
            if malformed {
                Ok(Some(vec![0; 8]))
            } else {
                Err(AudioError("synthetic read error".into()))
            }
        });
        let expected = if malformed {
            "invalid or unsupported PCM WAV"
        } else {
            "synthetic read error"
        };
        assert_eq!(result, Err(AudioError(expected.into())));
        assert!(old.upgrade().is_none());
        assert!(audio.sound(7).unwrap().source.is_none());
        assert_eq!(audio.sound(7).unwrap().volume, 0.75);
        assert!(!audio.lock().unwrap().sources.contains_key("bad.wav"));
        audio.validate_sound_id(&id).unwrap();
    }
}

#[test]
fn decode_error_precedes_post_read_identity_check_without_touching_replacement() {
    let (audio, id) = loaded();
    let result = audio.load_sound_with_optional_id(&id, "bad.wav", 0, || {
        audio.release_sound(&id).unwrap();
        audio.create_sound_at(7).unwrap();
        Ok(Some(vec![0; 8]))
    });
    assert_eq!(
        result,
        Err(AudioError("invalid or unsupported PCM WAV".into()))
    );
    assert!(audio.sound(7).unwrap().source.is_none());
    assert!(!audio.lock().unwrap().sources.contains_key("bad.wav"));
}

#[test]
fn commit_uses_live_parameters_and_listener_changed_by_reader() {
    let audio = RuntimeAudio::default();
    let id = audio.create_scene_sound(FrameKey(7)).unwrap();
    assert!(audio
        .load_sound_with_optional_id(&id, "live.wav", 8, || {
            assert!(audio.0.try_lock().is_ok());
            audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
            audio.set_sound_volume(&id, 0.25).unwrap();
            audio.set_sound_frequency(&id, 2.0).unwrap();
            audio.set_sound_enabled(&id, true).unwrap();
            audio
                .set_listener([0.0; 3], [1.0, 0.0, 0.0], None)
                .unwrap();
            Ok(Some(wav(1)))
        })
        .unwrap());
    let state = audio.sound(7).unwrap();
    assert_eq!(
        (state.volume, state.frequency, state.sound_type),
        (0.25, 2.0, 3)
    );
    assert_eq!(state.state, PlaybackState::Play);
    assert_eq!(state.open_flags, 0x218);
    assert_eq!(state.native_tail[0], 0x21ff);
    // Preserve Open's reset of pending-play flags; do not claim an automatic voice.
    assert!(!audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    audio.set_sound_playback(&id, true, true).unwrap();
    let mut samples = [0.0; 4];
    audio.mix_stereo(4, &mut samples).unwrap();
    assert_eq!(samples, [0.0625; 4]);
}

#[test]
fn exact_name_fast_path_keeps_voice_position_flags_and_skips_reader() {
    let (audio, id) = loaded();
    audio
        .set_listener([0.0; 3], [1.0, 0.0, 0.0], None)
        .unwrap();
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_playback(&id, true, true).unwrap();
    audio.mix_stereo(4, &mut [0.0; 4]).unwrap();
    let before = snapshot(&audio);
    assert!(audio
        .load_sound_with_optional_id(&id, "a.wav", 8, || panic!("unexpected I/O"))
        .unwrap());
    assert_eq!(snapshot(&audio), before);
    assert_eq!(audio.sound_position_ms(&id).unwrap(), 500);
}

#[test]
fn shared_source_keeps_first_open_flags_and_destination_parameters() {
    let audio = RuntimeAudio::default();
    let first = audio.create_scene_sound(FrameKey(7)).unwrap();
    let second = audio.create_scene_sound(FrameKey(8)).unwrap();
    audio
        .load_sound_with_optional_id(&first, "a.wav", 8, || Ok(Some(wav(1))))
        .unwrap();
    audio.set_sound_volume(&second, 0.75).unwrap();
    audio.set_sound_frequency(&second, 1.5).unwrap();
    audio
        .load_sound_with_optional_id(&second, "A.WAV", 0, || panic!("cache hit read"))
        .unwrap();
    let a = audio.sound(7).unwrap();
    let b = audio.sound(8).unwrap();
    assert!(Arc::ptr_eq(
        a.source.as_ref().unwrap(),
        b.source.as_ref().unwrap()
    ));
    assert_eq!((a.open_flags, b.open_flags), (0x218, 0x218));
    assert_eq!((b.volume, b.frequency), (0.75, 1.5));
}

#[test]
fn stereo_source_preserves_coercion_and_source_capabilities() {
    let audio = RuntimeAudio::default();
    let id = audio.create_scene_sound(FrameKey(7)).unwrap();
    audio.set_sound_kind(&id, SoundKind::Point).unwrap();
    audio.set_sound_cone(&id, [0.25, 0.5]).unwrap();
    audio
        .load_sound_with_optional_id(&id, "stereo.wav", 8, || Ok(Some(wav(2))))
        .unwrap();
    let state = audio.sound(7).unwrap();
    assert_eq!(state.sound_type, 3);
    assert_eq!(state.open_flags, 0x18);
    assert_eq!(state.cone, [0.25, 0.5]);
    assert_eq!(state.native_tail[0], 0x21ff);
}

struct TempInstallation {
    path: std::path::PathBuf,
    installation: Installation,
}
impl TempInstallation {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-retained-source-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("a.wav"), wav(1)).unwrap();
        let installation = Installation::open(&path).unwrap();
        Self { path, installation }
    }
}
impl Drop for TempInstallation {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn cached_reopen_uses_retained_source_and_preserves_destination_and_self_open() {
    let files = TempInstallation::new();
    let audio = RuntimeAudio::default();
    let source = audio.create_scene_sound(FrameKey(7)).unwrap();
    let destination = audio.create_scene_sound(FrameKey(8)).unwrap();
    assert_eq!(
        audio
            .open_original_sound_id(&source, "a.wav", 8, &files.installation)
            .unwrap(),
        0
    );
    std::fs::remove_file(files.path.join("a.wav")).unwrap();
    audio.set_sound_volume(&destination, 0.75).unwrap();
    assert_eq!(
        audio
            .reopen_cached_sound(&destination, &source, &files.installation)
            .unwrap(),
        0
    );
    assert_eq!(audio.sound(8).unwrap().volume, 0.75);
    assert_eq!(audio.live_source_count().unwrap(), 1);
    let before = snapshot(&audio);
    assert_eq!(
        audio
            .reopen_cached_sound(&source, &source, &files.installation)
            .unwrap(),
        0
    );
    assert_eq!(snapshot(&audio), before);
    audio.release_sound(&source).unwrap();
    audio.create_sound_at(7).unwrap();
    let before_destination = format!("{:?}", audio.sound(8).unwrap());
    assert_eq!(
        audio.reopen_cached_sound(&destination, &source, &files.installation),
        Err(AudioError("stale sound identity".into()))
    );
    assert_eq!(format!("{:?}", audio.sound(8).unwrap()), before_destination);
}

#[test]
fn original_open_reports_absence_and_preserves_filename_error_precedence() {
    let files = TempInstallation::new();
    let audio = RuntimeAudio::default();
    let id = audio.create_scene_sound(FrameKey(7)).unwrap();
    assert_eq!(
        audio
            .open_original_sound_id(&id, "absent.wav", 0, &files.installation)
            .unwrap(),
        0x8000_0001u32 as i32
    );
    audio.release_sound(&id).unwrap();
    assert_eq!(
        audio.open_original_sound_id(&id, "\u{e9}.wav", 0, &files.installation),
        Err(AudioError(
            "non-ASCII sound path requires original filename encoding".into()
        ))
    );
    assert_eq!(
        audio.open_original_sound_id(&id, "absent.wav", 0, &files.installation),
        Err(AudioError("stale sound identity".into()))
    );
}
