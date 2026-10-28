//! Synthetic scene-record regressions over real frame and mixer owners.
#![forbid(unsafe_code)]

use super::*;
use crate::frame_sound::{HostSoundBackend, SoundKey};
use crate::runtime_audio::{RuntimeAudio, SoundId, SoundKind};

fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
    [
        kind.to_le_bytes().as_slice(),
        &((payload.len() + 6) as u32).to_le_bytes(),
        payload,
    ]
    .concat()
}

fn scalar(kind: u16, value: f32) -> Vec<u8> {
    chunk(kind, &value.to_bits().to_le_bytes())
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn record(fields: &[Vec<u8>]) -> Vec<u8> {
    chunk(0x4c53, &chunk(0x4060, &fields.concat()))
}

fn input(bytes: &[u8]) -> MissionSceneInput<'_> {
    let mut input = MissionSceneInput::new(bytes).unwrap();
    assert_eq!(input.next_chunk().unwrap().unwrap().kind, 0x4060);
    input
}

fn fixture() -> (
    ModelArena,
    RuntimeAudio,
    MissionSceneLoader,
    SoundKey,
    SoundId,
) {
    let mut arena = ModelArena::default();
    let audio = RuntimeAudio::default();
    arena
        .bind_host_sound_backend(HostSoundBackend::new(1, &audio))
        .unwrap();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sound = arena.create_owned_sound_frame().unwrap();
    let id = arena.sound(sound).unwrap().audio_id().clone();
    let mut loader =
        MissionSceneLoader::new(&arena, scene).unwrap_or_else(|error| panic!("{}", error.detail));
    loader.current_frame = Some(arena.lookup_reference(sound.frame()));
    (arena, audio, loader, sound, id)
}

fn replacement(audio: &RuntimeAudio, sound: SoundKey, old: &SoundId) -> SoundId {
    audio.release_sound(old).unwrap();
    // Inject reuse through the compatibility factory, not the semantic API.
    audio.create_sound_at(sound.frame().0).unwrap();
    let id = audio.sound_id_from_handle(sound.frame().0).unwrap();
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_enabled(&id, true).unwrap();
    audio.set_sound_looping(&id, true).unwrap();
    audio.set_sound_volume(&id, 0.875).unwrap();
    id
}

fn snapshot(audio: &RuntimeAudio, sound: SoundKey) -> String {
    format!("{:?}", audio.sound(sound.frame().0).unwrap())
}

#[test]
fn scalar_sound_fields_use_the_retained_owner_in_file_order() {
    let (mut arena, audio, mut loader, sound, id) = fixture();
    let bytes = record(&[
        chunk(0x4061, &2u32.to_le_bytes()),
        scalar(0x4062, 0.75),
        scalar(0x4063, 0.125),
        scalar(0xb800, 1.5),
        chunk(0x4064, &floats(&[0.5, 1.0])),
        chunk(0x4065, &floats(&[2.0, 30.0, 0.5])),
        chunk(0x4068, &floats(&[3.0, 40.0, 0.25, 0.1])),
        chunk(0x4066, &[]),
        chunk(0x4067, &[0xff, 0x80]),
        scalar(0x4062, 0.25),
    ]);
    let mut cursor = input(&bytes);
    loader
        .read_sound(&mut arena, &mut cursor, &mut None)
        .unwrap();
    let state = audio.sound(sound.frame().0).unwrap();
    assert_eq!(state.sound_type, 2);
    assert_eq!(state.volume, 0.25);
    assert_eq!(state.out_volume, 0.125);
    assert_eq!(state.frequency, 1.5);
    assert_eq!(state.cone, [0.5, 1.0]);
    assert_eq!(state.range, [3.0, 40.0, 0.25, 0.1]);
    assert!(state.looping && state.on);
    assert_eq!(arena.sound(sound).unwrap().audio_id(), &id);
    assert_eq!(loader.object_fields.flags, 0x40);
    assert_eq!(cursor.cursor(), bytes.len());
    assert_eq!(cursor.depth(), 1);
    assert!(cursor.current().is_none());
    arena.release(sound.frame()).unwrap();
    assert_eq!(audio.live_sound_count().unwrap(), 0);
}

#[test]
fn three_word_range_keeps_the_original_fixed_middle_volume() {
    let (mut arena, audio, mut loader, sound, _) = fixture();
    let bytes = record(&[chunk(0x4065, &floats(&[2.0, 30.0, 0.5]))]);
    loader
        .read_sound(&mut arena, &mut input(&bytes), &mut None)
        .unwrap();
    assert_eq!(
        audio.sound(sound.frame().0).unwrap().range,
        [2.0, 30.0, 0.5, 1.0]
    );
}

#[test]
fn short_fields_stop_at_the_read_after_disabling_and_resetting_loop() {
    for (kind, length, entry) in [
        (0x4061, 4, 0x542d95),
        (0x4062, 4, 0x542d95),
        (0x4063, 4, 0x542d95),
        (0xb800, 4, 0x542d95),
        (0x4064, 8, 0x5432c4),
        (0x4065, 12, 0x543111),
        (0x4068, 16, 0x5435ac),
    ] {
        for available in 0..length {
            let (mut arena, audio, mut loader, sound, id) = fixture();
            audio.set_sound_enabled(&id, true).unwrap();
            audio.set_sound_looping(&id, true).unwrap();
            let bytes = record(&[chunk(kind, &vec![0; available]), scalar(0x4062, 0.5)]);
            let mut cursor = input(&bytes);
            let error = loader
                .read_sound(&mut arena, &mut cursor, &mut None)
                .unwrap_err();
            assert_eq!(error.0, entry, "kind {kind:x}, length {available}");
            assert_eq!(cursor.cursor(), 18);
            assert_eq!(cursor.current().unwrap().kind, kind);
            assert_eq!(cursor.depth(), 2);
            let state = audio.sound(sound.frame().0).unwrap();
            assert!(!state.on && !state.looping);
            assert_eq!(state.volume, 1.0);
            assert_eq!(loader.object_fields.flags, 0);
        }
    }
}

#[test]
fn invalid_fields_are_closed_before_error_and_keep_preceding_writes() {
    for (kind, payload, entry) in [
        (0x4061, 4u32.to_le_bytes().to_vec(), 0x542dad),
        (0x4062, floats(&[f32::NAN]), 0x542dad),
        (0x4063, floats(&[f32::INFINITY]), 0x542dad),
        (0xb800, floats(&[0.0]), 0x542dad),
        (0x4064, floats(&[0.5, f32::NAN]), 0x5432c4),
        (0x4065, floats(&[1.0, f32::INFINITY, 0.5]), 0x5431c6),
        (0x4068, floats(&[1.0, 20.0, 0.5, f32::NAN]), 0x5435ad),
    ] {
        let (mut arena, audio, mut loader, sound, _) = fixture();
        let prefix = scalar(0x4062, 0.25);
        let failing = chunk(kind, &payload);
        let reached = 12 + prefix.len() + failing.len();
        let bytes = record(&[prefix, failing, scalar(0x4062, 0.75)]);
        let mut cursor = input(&bytes);
        let error = loader
            .read_sound(&mut arena, &mut cursor, &mut None)
            .unwrap_err();
        assert_eq!(error.0, entry, "kind {kind:x}");
        assert_eq!(cursor.cursor(), reached);
        assert!(cursor.current().is_none());
        assert_eq!(cursor.depth(), 2);
        let state = audio.sound(sound.frame().0).unwrap();
        assert_eq!(state.volume, 0.25);
        assert_eq!(state.sound_type, 0);
        assert_eq!(state.frequency, 1.0);
        assert!(!state.on);
        assert_eq!(loader.object_fields.flags, 0);
    }
}

#[test]
fn stale_frame_is_rejected_before_disable_loop_reset_or_nested_input() {
    let (mut arena, audio, mut loader, sound, old) = fixture();
    let new = replacement(&audio, sound, &old);
    let before = snapshot(&audio, sound);
    let bytes = record(&[scalar(0x4062, 0.25)]);
    let mut cursor = input(&bytes);
    let error = loader
        .read_sound(&mut arena, &mut cursor, &mut None)
        .unwrap_err();
    assert_eq!(error, (0x542c6f, "stale sound identity".into()));
    assert_eq!(cursor.cursor(), 12);
    assert_eq!(cursor.current().unwrap().kind, 0x4060);
    assert_eq!(cursor.depth(), 1);
    assert_eq!(snapshot(&audio, sound), before);
    assert_eq!(arena.sound(sound).unwrap().audio_id(), &old);
    audio.validate_sound_id(&new).unwrap();
}

#[test]
fn callback_reuse_cannot_redirect_any_subsequent_sound_setter() {
    for (kind, payload, entry) in [
        (0x4061, 1u32.to_le_bytes().to_vec(), 0x542dad),
        (0x4061, u32::MAX.to_le_bytes().to_vec(), 0x542dad),
        (0x4062, floats(&[0.25]), 0x542dad),
        (0x4062, floats(&[f32::NAN]), 0x542dad),
        (0x4063, floats(&[0.125]), 0x542dad),
        (0xb800, floats(&[2.0]), 0x542dad),
        (0x4064, floats(&[0.5, 1.0]), 0x5432c4),
        (0x4065, floats(&[2.0, 30.0, 0.5]), 0x5431c6),
        (0x4068, floats(&[3.0, 40.0, 0.25, 0.1]), 0x5435ad),
        (0x4066, vec![7, 8, 9], 0x5432d1),
    ] {
        let (mut arena, audio, mut loader, sound, old) = fixture();
        let filename = chunk(0x10, b"synthetic.wav\0");
        let field_start = 12 + filename.len();
        let bytes = record(&[filename, chunk(kind, &payload)]);
        let mut cursor = input(&bytes);
        let mut before = String::new();
        let mut calls = 0;
        let error = {
            let mut open =
                |_: &mut ModelArena, frame: FrameKey, name: &[u8], request: ResourceOpenKind| {
                    assert_eq!(frame, sound.frame());
                    assert_eq!(name, b"synthetic.wav");
                    assert!(matches!(request, ResourceOpenKind::Sound));
                    calls += 1;
                    replacement(&audio, sound, &old);
                    before = snapshot(&audio, sound);
                    Ok(0)
                };
            loader
                .read_sound(&mut arena, &mut cursor, &mut Some(&mut open))
                .unwrap_err()
        };
        assert_eq!(calls, 1);
        assert_eq!(
            error,
            (entry, "stale sound identity".into()),
            "kind {kind:x}"
        );
        assert_eq!(snapshot(&audio, sound), before);
        assert_eq!(arena.sound(sound).unwrap().audio_id(), &old);
        assert_eq!(loader.object_fields.flags, 0);
        if kind == 0x4066 {
            // Loop is applied before CloseChunk, unlike the scalar fields.
            assert_eq!(cursor.cursor(), field_start + 6);
            assert_eq!(cursor.current().unwrap().kind, kind);
        } else {
            assert_eq!(cursor.cursor(), bytes.len());
            assert!(cursor.current().is_none());
        }
        assert_eq!(cursor.depth(), 2);
    }
}

#[test]
fn final_enable_failure_keeps_the_completed_record_prefix() {
    let (mut arena, audio, mut loader, sound, old) = fixture();
    let bytes = record(&[chunk(0x10, b"synthetic.wav\0"), chunk(0x4067, &[1, 2, 3])]);
    let mut cursor = input(&bytes);
    let mut before = String::new();
    let error = {
        let mut open = |_: &mut ModelArena, _: FrameKey, _: &[u8], _: ResourceOpenKind| {
            replacement(&audio, sound, &old);
            before = snapshot(&audio, sound);
            Ok(0)
        };
        loader
            .read_sound(&mut arena, &mut cursor, &mut Some(&mut open))
            .unwrap_err()
    };
    assert_eq!(error, (0x54394f, "stale sound identity".into()));
    assert_eq!(cursor.cursor(), bytes.len());
    assert_eq!(cursor.depth(), 1);
    assert!(cursor.current().is_none());
    assert_eq!(loader.object_fields.flags, 0x40);
    assert_eq!(snapshot(&audio, sound), before);
    assert_eq!(arena.sound(sound).unwrap().audio_id(), &old);
}

#[test]
fn source_callback_changes_are_live_and_negative_hresult_does_not_skip_fields() {
    let (mut arena, audio, mut loader, sound, id) = fixture();
    let bytes = record(&[chunk(0x10, b"synthetic.wav\0"), scalar(0x4062, 0.25)]);
    let mut cursor = input(&bytes);
    {
        let mut open = |_: &mut ModelArena, _: FrameKey, _: &[u8], _: ResourceOpenKind| {
            audio.set_sound_volume(&id, 0.625).unwrap();
            audio.set_sound_looping(&id, true).unwrap();
            Ok(0x8000_0001u32 as i32)
        };
        loader
            .read_sound(&mut arena, &mut cursor, &mut Some(&mut open))
            .unwrap();
    }
    let state = audio.sound(sound.frame().0).unwrap();
    assert_eq!(state.volume, 0.25);
    assert!(state.looping && state.on);
    assert_eq!(arena.sound(sound).unwrap().audio_id(), &id);
    assert_eq!(cursor.cursor(), bytes.len());
    assert_eq!(loader.object_fields.flags, 0x40);
}

#[test]
fn source_callback_error_retains_its_writes_and_closed_filename_cursor() {
    let (mut arena, audio, mut loader, sound, id) = fixture();
    let prefix = scalar(0x4062, 0.25);
    let filename = chunk(0x10, b"synthetic.wav\0");
    let reached = 12 + prefix.len() + filename.len();
    let bytes = record(&[prefix, filename, scalar(0x4062, 0.75)]);
    let mut cursor = input(&bytes);
    let error = {
        let mut open = |_: &mut ModelArena, _: FrameKey, _: &[u8], _: ResourceOpenKind| {
            assert_eq!(audio.sound(sound.frame().0).unwrap().volume, 0.25);
            audio.set_sound_volume(&id, 0.625).unwrap();
            Err("synthetic interrupted open".to_string())
        };
        loader
            .read_sound(&mut arena, &mut cursor, &mut Some(&mut open))
            .unwrap_err()
    };
    assert_eq!(error, (0x542e9a, "synthetic interrupted open".into()));
    assert_eq!(cursor.cursor(), reached);
    assert!(cursor.current().is_none());
    assert_eq!(cursor.depth(), 2);
    let state = audio.sound(sound.frame().0).unwrap();
    assert_eq!(state.volume, 0.625);
    assert!(!state.on && !state.looping);
    assert_eq!(loader.object_fields.flags, 0);
}

#[test]
fn absent_or_wrong_kind_frame_keeps_the_record_unconsumed() {
    let (mut arena, audio, mut loader, sound, id) = fixture();
    audio.set_sound_enabled(&id, true).unwrap();
    audio.set_sound_looping(&id, true).unwrap();
    let before = snapshot(&audio, sound);
    let sector = arena.scene(loader.scene).unwrap().sectors[0].frame();
    for current in [None, Some(arena.lookup_reference(sector))] {
        loader.current_frame = current;
        let bytes = record(&[scalar(0x4062, 0.25)]);
        let mut cursor = input(&bytes);
        loader
            .read_sound(&mut arena, &mut cursor, &mut None)
            .unwrap();
        assert_eq!(cursor.cursor(), 12);
        assert_eq!(cursor.depth(), 1);
        assert_eq!(cursor.current().unwrap().kind, 0x4060);
        assert_eq!(snapshot(&audio, sound), before);
    }
}
