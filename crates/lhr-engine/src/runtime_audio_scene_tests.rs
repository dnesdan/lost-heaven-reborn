//! Synthetic scene-factory and retained-identity regressions.
use super::*;

#[test]
fn scene_factory_matches_legacy_initial_state_and_clock() {
    let typed = RuntimeAudio::default();
    let legacy = RuntimeAudio::default();
    typed.lock().unwrap().clock_ms = 4321.75;
    legacy.lock().unwrap().clock_ms = 4321.75;
    let frame = FrameKey(7);
    let id = typed.create_scene_sound(frame).unwrap();
    legacy
        .create_frame_sound_at(frame.0, SoundFrameOwner::Scene(frame))
        .unwrap();
    assert_eq!(id, typed.sound_id_from_handle(frame.0).unwrap());
    assert_eq!(
        format!("{:?}", typed.sound(frame.0).unwrap()),
        format!("{:?}", legacy.sound(frame.0).unwrap())
    );
    assert_eq!(
        typed.sound_frame_owner(frame.0).unwrap(),
        SoundFrameOwner::Scene(frame)
    );
}

#[test]
fn scene_and_legacy_factories_share_nonreused_generations() {
    let audio = RuntimeAudio::default();
    let first = audio.create_scene_sound(FrameKey(7)).unwrap();
    audio.release_sound(&first).unwrap();
    audio.create_sound_at(7).unwrap();
    let middle = audio.sound_id_from_handle(7).unwrap();
    audio.release_sound(&middle).unwrap();
    let last = audio.create_scene_sound(FrameKey(7)).unwrap();
    assert_eq!(
        (first.generation, middle.generation, last.generation),
        (1, 2, 3)
    );
    assert!(audio.validate_sound_id(&first).is_err());
    assert!(audio.validate_sound_id(&middle).is_err());
    audio.clone().validate_sound_id(&last).unwrap();
    assert!(RuntimeAudio::default().validate_sound_id(&last).is_err());
}

#[test]
fn scene_factory_rejects_invalid_duplicate_and_exhausted_creation_without_writes() {
    let audio = RuntimeAudio::default();
    assert!(audio.create_scene_sound(FrameKey(0)).is_err());
    assert_eq!(audio.lock().unwrap().next_sound_generation, 0);
    let id = audio.create_scene_sound(FrameKey(7)).unwrap();
    assert!(audio.create_scene_sound(FrameKey(7)).is_err());
    assert_eq!(audio.lock().unwrap().next_sound_generation, 1);
    assert_eq!(audio.live_sound_count().unwrap(), 1);
    audio.lock().unwrap().next_sound_generation = u64::MAX;
    assert!(audio.create_scene_sound(FrameKey(8)).is_err());
    assert!(!audio.owns(8));
    assert_eq!(audio.lock().unwrap().next_sound_generation, u64::MAX);
    audio.validate_sound_id(&id).unwrap();
}

#[test]
fn identity_preflight_has_no_playback_or_parameter_side_effects() {
    let audio = RuntimeAudio::default();
    let id = audio.create_scene_sound(FrameKey(7)).unwrap();
    audio.set_sound_volume(&id, 0.75).unwrap();
    audio.set_sound_enabled(&id, true).unwrap();
    let before = format!("{:?}", audio.sound(7).unwrap());
    audio.validate_sound_id(&id).unwrap();
    assert_eq!(format!("{:?}", audio.sound(7).unwrap()), before);
}

#[test]
fn typed_sector_edits_reject_stale_ids_and_preserve_replacement_membership() {
    let audio = RuntimeAudio::default();
    let old = audio.create_scene_sound(FrameKey(7)).unwrap();
    audio.add_sound_sector(&old, FrameKey(11)).unwrap();
    audio.add_sound_sector(&old, FrameKey(11)).unwrap();
    assert_eq!(audio.sound(7).unwrap().sectors.len(), 1);
    audio.delete_sound_sector(&old, FrameKey(99)).unwrap();
    assert!(audio.sound(7).unwrap().sectors.contains(&11));
    audio.sound_virtual(7, 0, &[]).unwrap();
    let replacement = audio.create_scene_sound(FrameKey(7)).unwrap();
    audio.add_sound_sector(&replacement, FrameKey(11)).unwrap();
    let before = format!("{:?}", audio.sound(7).unwrap());
    assert!(audio.add_sound_sector(&old, FrameKey(12)).is_err());
    assert!(audio.delete_sound_sector(&old, FrameKey(11)).is_err());
    assert_eq!(format!("{:?}", audio.sound(7).unwrap()), before);
    audio.delete_sound_sector(&replacement, FrameKey(11)).unwrap();
    assert!(audio.sound(7).unwrap().sectors.is_empty());
}
