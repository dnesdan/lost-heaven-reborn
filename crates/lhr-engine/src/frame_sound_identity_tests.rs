//! Scene lifecycle regressions using only synthetic owners and legacy faults.
use super::*;
use crate::runtime_audio::SoundFrameOwner;

fn owners() -> (ModelArena, RuntimeAudio, HostSoundBackend) {
    let mut arena = ModelArena::default();
    let audio = RuntimeAudio::default();
    let backend = HostSoundBackend::new(1, &audio);
    arena.bind_host_sound_backend(backend.clone()).unwrap();
    (arena, audio, backend)
}

fn replace_legacy_sound(audio: &RuntimeAudio, key: SoundKey) -> SoundId {
    audio.sound_virtual(key.frame().0, 0, &[]).unwrap();
    audio.create_sound_at(key.frame().0).unwrap();
    let replacement = audio.sound_id_from_handle(key.frame().0).unwrap();
    audio.set_sound_enabled(&replacement, true).unwrap();
    audio.set_sound_volume(&replacement, 0.75).unwrap();
    replacement
}

#[test]
fn frame_retains_factory_identity_through_sector_edits_and_release() {
    let (mut arena, audio, backend) = owners();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sector = arena.scene(scene).unwrap().sectors[0];
    let sound = arena.create_owned_sound_frame().unwrap();
    let id = arena.sound(sound).unwrap().audio_id().clone();
    assert_eq!(id, audio.sound_id_from_handle(sound.frame().0).unwrap());
    assert_eq!(
        audio.sound_frame_owner(sound.frame().0).unwrap(),
        SoundFrameOwner::Scene(sound.frame())
    );
    arena.add_sound_to_sector(sector, sound).unwrap();
    audio.clone().set_sound_volume(&id, 0.25).unwrap();
    arena.delete_sound_from_sector(sector, sound).unwrap();
    assert_eq!(arena.sound(sound).unwrap().audio_id(), &id);
    arena.release(sound.frame()).unwrap();
    assert!(audio.validate_sound_id(&id).is_err());
    assert_eq!(audio.live_sound_count().unwrap(), 0);
    assert!(backend.sounds().unwrap().is_empty());
}

#[test]
fn stale_frame_teardown_cannot_stop_unlink_or_release_a_replacement_sound() {
    let (mut arena, audio, backend) = owners();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sector = arena.scene(scene).unwrap().sectors[0];
    let area = arena.create_area().unwrap();
    let sound = arena.create_owned_sound_frame().unwrap();
    let old = arena.sound(sound).unwrap().audio_id().clone();
    arena.add_sound_to_sector(sector, sound).unwrap();
    arena.add_sound_to_area(area, sound).unwrap();
    let replacement = replace_legacy_sound(&audio, sound);
    audio.add_sound_sector(&replacement, sector.frame()).unwrap();
    let before = format!("{:?}", audio.sound(sound.frame().0).unwrap());

    assert!(matches!(
        arena.destroy_sound_derived(sound),
        Err(ModelError::Sound(SoundError::BackendUnavailable))
    ));
    assert_eq!(format!("{:?}", audio.sound(sound.frame().0).unwrap()), before);
    assert_eq!(arena.sound(sound).unwrap().audio_id(), &old);
    assert_eq!(arena.sound(sound).unwrap().sectors(), &[sector]);
    assert_eq!(arena.sound(sound).unwrap().areas(), &[area]);
    assert_eq!(arena.sector(sector).unwrap().sounds, [sound.frame()]);
    assert_eq!(arena.area(area).unwrap().sounds(), &[sound.frame()]);
    assert_eq!(backend.sounds().unwrap(), [sound]);
    audio.validate_sound_id(&replacement).unwrap();
}

#[test]
fn failed_registry_append_keeps_the_identity_for_unregistered_frame_cleanup() {
    let (mut arena, audio, backend) = owners();
    // Model a failed append without introducing an allocator failure hook.
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = backend.registry.lock().unwrap();
        panic!("synthetic registry failure");
    }));
    assert!(poisoned.is_err());
    let failure = arena.create_owned_sound_frame().unwrap_err();
    assert!(matches!(
        failure.error,
        ModelError::Sound(SoundError::RegistryUnavailable)
    ));
    let sound = failure.frame.unwrap();
    let id = arena.sound(sound).unwrap().audio_id().clone();
    assert!(!arena.sound(sound).unwrap().registry_member);
    audio.validate_sound_id(&id).unwrap();
    arena.release(sound.frame()).unwrap();
    assert_eq!(audio.live_sound_count().unwrap(), 0);
    assert!(audio.validate_sound_id(&id).is_err());
}

#[test]
fn failed_sector_add_preserves_reached_scene_prefix_but_not_replacement_audio() {
    let (mut arena, audio, _) = owners();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sector = arena.scene(scene).unwrap().sectors[0];
    let sound = arena.create_owned_sound_frame().unwrap();
    let replacement = replace_legacy_sound(&audio, sound);
    let before = format!("{:?}", audio.sound(sound.frame().0).unwrap());
    assert!(arena.add_sound_to_sector(sector, sound).is_err());
    // Original list writes precede the backend operation. Keep this explicit
    // interrupted prefix; never retry by refreshing the frame's SoundId.
    assert_eq!(arena.sound(sound).unwrap().sectors(), &[sector]);
    assert_eq!(arena.sector(sector).unwrap().sounds, [sound.frame()]);
    assert_eq!(format!("{:?}", audio.sound(sound.frame().0).unwrap()), before);
    audio.validate_sound_id(&replacement).unwrap();
}

#[test]
fn failed_sector_delete_preserves_reached_prefix_and_replacement_membership() {
    let (mut arena, audio, _) = owners();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sector = arena.scene(scene).unwrap().sectors[0];
    let sound = arena.create_owned_sound_frame().unwrap();
    arena.add_sound_to_sector(sector, sound).unwrap();
    let replacement = replace_legacy_sound(&audio, sound);
    audio.add_sound_sector(&replacement, sector.frame()).unwrap();
    let before = format!("{:?}", audio.sound(sound.frame().0).unwrap());
    assert!(arena.delete_sound_from_sector(sector, sound).is_err());
    // The reciprocal removal precedes the PCM operation; the sector's own
    // removal follows it and must not be reached on failure.
    assert!(arena.sound(sound).unwrap().sectors().is_empty());
    assert_eq!(arena.sector(sector).unwrap().sounds, [sound.frame()]);
    assert_eq!(format!("{:?}", audio.sound(sound.frame().0).unwrap()), before);
    audio.validate_sound_id(&replacement).unwrap();
}
