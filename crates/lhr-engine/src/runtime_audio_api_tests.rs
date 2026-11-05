use super::*;
use crate::runtime_audio::SoundState;

fn wav() -> Vec<u8> {
    let samples = [8192i16; 64];
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + samples.len() as u32 * 2).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(8u32.to_le_bytes());
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((samples.len() as u32 * 2).to_le_bytes());
    bytes.extend(samples.into_iter().flat_map(i16::to_le_bytes));
    bytes
}

fn loaded() -> (RuntimeAudio, SoundId) {
    let audio = RuntimeAudio::default();
    audio.create_sound_at(7).unwrap();
    audio
        .load_sound_with(7, "synthetic.wav", || Ok(wav()))
        .unwrap();
    audio.set_listener([0.; 3], [1., 0., 0.], None).unwrap();
    let id = audio.sound_id_from_handle(7).unwrap();
    (audio, id)
}

fn state_words(state: &SoundState) -> Vec<u64> {
    let mut words = vec![
        u64::from(state.handle),
        state.generation,
        u64::from(state.open_flags),
        u64::from(state.parent),
        u64::from(state.on),
        u64::from(state.looping),
        u64::from(state.volume.to_bits()),
        u64::from(state.out_volume.to_bits()),
        u64::from(state.frequency.to_bits()),
        u64::from(state.sound_type),
        u64::from(state.state.native_byte()),
        u64::from(state.world_scale.to_bits()),
        u64::from(state.voice_playing),
        state.frame_position.to_bits(),
        u64::from(state.clock_ms),
        u64::from(state.start_time),
        u64::from(state.pause_time),
        u64::from(state.pause_pos),
    ];
    for values in [
        state.range.as_slice(),
        &state.cone,
        &state.position,
        &state.velocity,
        &state.direction,
    ] {
        words.extend(values.iter().map(|value| u64::from(value.to_bits())));
    }
    words.extend(state.native_tail.map(u64::from));
    words.extend(state.sectors.iter().copied().map(u64::from));
    words
}

#[test]
fn identities_are_owner_scoped_but_runtime_clones_share_them() {
    let (first, id) = loaded();
    let (second, other) = loaded();
    assert_ne!(id, other);
    assert!(second.set_sound_volume(&id, 0.25).is_err());
    assert!(second.release_sound(&id).is_err());
    assert_eq!(second.sound(7).unwrap().volume, 1.);
    assert_eq!(second.live_sound_count().unwrap(), 1);
    first.clone().set_sound_volume(&id, 0.25).unwrap();
    assert_eq!(first.sound(7).unwrap().volume, 0.25);
    assert_eq!(first.sound_id_from_handle(7).unwrap(), id);
    fn send_sync<T: Send + Sync>() {}
    send_sync::<SoundId>();
}

#[test]
fn legacy_release_and_handle_reuse_never_revive_an_old_id() {
    let (audio, old) = loaded();
    audio.sound_virtual(7, 0, &[]).unwrap();
    audio.create_sound_at(7).unwrap();
    let current = audio.sound_id_from_handle(7).unwrap();
    assert_ne!(old, current);
    assert!(audio.set_sound_volume(&old, 0.25).is_err());
    assert!(audio.release_sound(&old).is_err());
    assert!(audio.sound_position_ms(&old).is_err());
    assert_eq!(audio.sound(7).unwrap().volume, 1.);
    audio.set_sound_volume(&current, 0.5).unwrap();
    audio.release_sound(&current).unwrap();
    assert!(audio.release_sound(&current).is_err());
    assert_eq!(audio.live_sound_count().unwrap(), 0);
}

#[test]
fn sound_ids_do_not_keep_the_mixer_or_decoded_source_alive() {
    let (audio, id) = loaded();
    let owner = Arc::downgrade(&audio.0);
    let source = Arc::downgrade(audio.sound(7).unwrap().source.as_ref().unwrap());
    let another_id = id.clone();
    drop(audio);
    assert!(owner.upgrade().is_none());
    assert!(source.upgrade().is_none());
    assert_eq!(id, another_id);
    let replacement = RuntimeAudio::default();
    replacement.create_sound_at(7).unwrap();
    assert!(replacement.set_sound_enabled(&id, true).is_err());
}

#[test]
fn source_replacement_preserves_sound_incarnation_without_retaining_old_pcm() {
    let (audio, id) = loaded();
    let old_source = Arc::downgrade(audio.sound(7).unwrap().source.as_ref().unwrap());
    audio
        .load_sound_with(7, "replacement.wav", || Ok(wav()))
        .unwrap();
    assert!(old_source.upgrade().is_none());
    assert_eq!(audio.sound_id_from_handle(7).unwrap(), id);
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_playback(&id, true, true).unwrap();
    assert!(audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
}

#[test]
fn generation_exhaustion_fails_before_publishing_a_new_sound() {
    let (audio, id) = loaded();
    audio.lock().unwrap().next_sound_generation = u64::MAX;
    assert!(audio.create_sound_at(8).is_err());
    assert!(!audio.owns(8));
    assert_eq!(audio.live_sound_count().unwrap(), 1);
    assert_eq!(audio.lock().unwrap().next_sound_generation, u64::MAX);
    audio.set_sound_volume(&id, 0.25).unwrap();
    assert_eq!(audio.sound(7).unwrap().volume, 0.25);
}

#[test]
fn named_operations_match_legacy_dispatch_state_and_pcm() {
    type Setter = fn(&RuntimeAudio, &SoundId) -> Result<()>;
    let (named, id) = loaded();
    let (legacy, _) = loaded();
    let steps: Vec<(Setter, u32, Vec<u32>)> = vec![
        (
            |a, s| a.set_sound_kind(s, SoundKind::Ambient),
            0x58,
            vec![3],
        ),
        (
            |a, s| a.set_sound_range(s, [1., 10., 0.5, 0.25]),
            0x5c,
            [1f32, 10., 0.5, 0.25].map(f32::to_bits).to_vec(),
        ),
        (
            |a, s| a.set_sound_cone(s, [0.5, 1.]),
            0x60,
            [0.5f32, 1.].map(f32::to_bits).to_vec(),
        ),
        (
            |a, s| a.set_sound_outside_volume(s, 0.25),
            0x64,
            vec![0.25f32.to_bits()],
        ),
        (
            |a, s| a.set_sound_volume(s, 0.5),
            0x68,
            vec![0.5f32.to_bits()],
        ),
        (|a, s| a.set_sound_looping(s, true), 0x6c, vec![1]),
        (
            |a, s| a.set_sound_frequency(s, 2.),
            0x74,
            vec![2f32.to_bits()],
        ),
        (|a, s| a.set_sound_enabled(s, true), 0x24, vec![1]),
        (|a, s| a.seek_sound_ms(s, 250), 0x70, vec![250]),
        (|a, s| a.set_sound_playback(s, true, true), 0x88, vec![1, 1]),
        (|a, s| a.update_sound(s), 0x90, vec![]),
        (|a, s| a.set_sound_looping(s, false), 0x6c, vec![0]),
    ];
    let mut audible = false;
    for (setter, slot, arguments) in steps {
        setter(&named, &id).unwrap();
        assert_eq!(legacy.sound_virtual(7, slot, &arguments).unwrap(), 0);
        assert_eq!(
            state_words(&named.sound(7).unwrap()),
            state_words(&legacy.sound(7).unwrap())
        );
        assert_eq!(
            named.sound_position_ms(&id).unwrap(),
            legacy.sound_virtual(7, 0x80, &[]).unwrap()
        );
        assert_eq!(
            named.sound_duration_ms(&id).unwrap(),
            legacy.sound_virtual(7, 0x84, &[]).unwrap()
        );
        for (query, flag) in [(PlaybackQuery::Logical, 0), (PlaybackQuery::Voice, 1)] {
            assert_eq!(
                u32::from(named.sound_is_playing(&id, query).unwrap()),
                legacy.sound_virtual(7, 0x54, &[flag]).unwrap()
            );
        }
        let (mut left, mut right) = ([0.; 4], [0.; 4]);
        named.mix_stereo(8, &mut left).unwrap();
        legacy.mix_stereo(8, &mut right).unwrap();
        assert_eq!(left.map(f32::to_bits), right.map(f32::to_bits));
        audible |= left.iter().any(|sample| *sample != 0.);
        assert_eq!(
            state_words(&named.sound(7).unwrap()),
            state_words(&legacy.sound(7).unwrap())
        );
    }
    assert!(audible);
    named.release_sound(&id).unwrap();
    legacy.sound_virtual(7, 0, &[]).unwrap();
    assert_eq!(named.live_voice_count().unwrap(), 0);
    assert_eq!(
        named.live_source_count().unwrap(),
        legacy.live_source_count().unwrap()
    );
}

#[test]
fn invalid_parameters_and_stale_ids_preserve_existing_state_and_error_precedence() {
    let (audio, id) = loaded();
    let before = state_words(&audio.sound(7).unwrap());
    assert!(audio.set_sound_volume(&id, f32::NAN).is_err());
    assert!(audio
        .set_sound_range(&id, [1., f32::INFINITY, 0., 0.])
        .is_err());
    assert!(audio.set_sound_cone(&id, [0., f32::NEG_INFINITY]).is_err());
    assert!(audio.set_sound_frequency(&id, 0.).is_err());
    assert_eq!(state_words(&audio.sound(7).unwrap()), before);
    assert_eq!(
        audio
            .sound_virtual(999, 0x68, &[f32::NAN.to_bits()])
            .unwrap_err()
            .0,
        "unowned sound 0x3e7"
    );
    assert!(audio
        .sound_virtual(999, 0xff, &[])
        .unwrap_err()
        .0
        .contains("unsupported sound vtable slot"));
    assert!(audio
        .sound_virtual(999, 0x68, &[])
        .unwrap_err()
        .0
        .contains("argument count"));
    audio.release_sound(&id).unwrap();
    assert_eq!(
        audio.set_sound_volume(&id, f32::NAN).unwrap_err().0,
        "stale sound identity"
    );
}

#[test]
fn deferred_play_and_upper_cone_cap_remain_distinct_from_immediate_update() {
    let (audio, id) = loaded();
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_enabled(&id, true).unwrap();
    assert!(audio.sound_is_playing(&id, PlaybackQuery::Logical).unwrap());
    assert!(!audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    audio.update_sound(&id).unwrap();
    assert!(audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    audio.set_sound_cone(&id, [-1., 7.]).unwrap();
    assert_eq!(audio.sound(7).unwrap().cone, [-1., std::f32::consts::PI]);
    audio.set_sound_volume(&id, -0.).unwrap();
    audio.set_sound_volume(&id, 0.).unwrap();
    assert_eq!(audio.sound(7).unwrap().volume.to_bits(), (-0f32).to_bits());
}

#[test]
fn stereo_kind_coercion_matches_the_existing_non_null_rule() {
    let (audio, id) = loaded();
    let mut stereo = wav();
    stereo[22..24].copy_from_slice(&2u16.to_le_bytes());
    stereo[28..32].copy_from_slice(&32u32.to_le_bytes());
    stereo[32..34].copy_from_slice(&4u16.to_le_bytes());
    audio
        .load_sound_with(7, "stereo.wav", || Ok(stereo))
        .unwrap();
    for kind in [SoundKind::Point, SoundKind::Spot, SoundKind::Ambient] {
        audio.set_sound_kind(&id, kind).unwrap();
        assert_eq!(audio.sound(7).unwrap().sound_type, 3);
    }
    audio.set_sound_kind(&id, SoundKind::Null).unwrap();
    assert_eq!(audio.sound(7).unwrap().sound_type, 0);
}

#[test]
fn immediate_update_failure_preserves_the_reached_enable_prefix() {
    let (named, id) = loaded();
    let (legacy, _) = loaded();
    for audio in [&named, &legacy] {
        audio.sound_virtual(7, 0x58, &[2]).unwrap();
        audio.set_position(7, [0.; 3], [f32::MAX; 3]).unwrap();
    }
    let error = named.set_sound_playback(&id, true, true).unwrap_err();
    assert_eq!(legacy.sound_virtual(7, 0x88, &[1, 1]).unwrap_err(), error);
    assert!(named.sound(7).unwrap().on);
    assert_eq!(
        state_words(&named.sound(7).unwrap()),
        state_words(&legacy.sound(7).unwrap())
    );
}

#[test]
fn pin_setter_guards_flags_and_caps() {
    // RVAs 0x57d30/0x57dd0/0x57eb0/0x57f90/0x57fd0/0x580e0/0x58120.
    let (audio, id) = loaded();
    let flag = |h| audio.read_sound_word(h, 0x140).unwrap();
    let before = flag(7);
    audio.set_sound_volume(&id, 1.0).unwrap();
    assert_eq!(flag(7), before);
    audio.set_sound_volume(&id, 0.5).unwrap();
    assert_ne!(flag(7) & 0x8, 0);
    audio.set_sound_range(&id, [1.0, 10.0, 1.0, 0.0]).unwrap();
    audio.set_sound_range(&id, [2.0, 20.0, 0.5, 0.25]).unwrap();
    assert_ne!(flag(7) & 0x4, 0);
    assert_eq!(audio.read_sound_word(7, 0x168).unwrap(), 2f32.to_bits());
    audio.set_sound_cone(&id, [0.5, 7.0]).unwrap();
    let cone = audio.sound(7).unwrap().cone;
    assert_eq!(cone[1].to_bits(), std::f32::consts::PI.to_bits());
    assert_eq!(cone[0].to_bits(), 0.5f32.to_bits());
    assert_ne!(flag(7) & 0x40, 0);
    audio.set_sound_outside_volume(&id, 0.25).unwrap();
    assert_ne!(flag(7) & 0x10, 0);
    audio.set_sound_frequency(&id, 2.0).unwrap();
    assert_ne!(flag(7) & 0x20, 0);
    assert!(audio.set_sound_frequency(&id, 0.0).is_err());
    assert!(audio.set_sound_frequency(&id, f32::NAN).is_err());
    assert_eq!(audio.sound(7).unwrap().frequency.to_bits(), 2f32.to_bits());
    audio.set_velocity(7, [1.0, 2.0, 3.0]).unwrap();
    assert_ne!(flag(7) & 0x2000, 0);
    assert_eq!(audio.sound(7).unwrap().velocity, [1.0, 2.0, 3.0]);
    audio.set_sound_kind(&id, SoundKind::Point).unwrap();
    assert_ne!(flag(7) & 0x100, 0);
    let words_before = state_words(&audio.sound(7).unwrap());
    assert!(audio.set_sound_volume(&id, f32::NAN).is_err());
    assert_eq!(state_words(&audio.sound(7).unwrap()), words_before);
}
#[test]
fn pin_getters_with_synthetic_vectors() {
    // RVAs 0x581d0/0x58210/0x58230/0x58250 (+0x58060 seek interplay).
    let (audio, id) = loaded();
    audio.set_sound_range(&id, [2.0, 20.0, 0.5, 0.25]).unwrap();
    audio.set_sound_cone(&id, [0.6, 1.2]).unwrap();
    assert_eq!(audio.read_sound_word(7, 0x168).unwrap(), 2f32.to_bits());
    assert_eq!(audio.read_sound_word(7, 0x170).unwrap(), 20f32.to_bits());
    assert_eq!(audio.read_sound_word(7, 0x17c).unwrap(), 0.5f32.to_bits());
    assert_eq!(audio.read_sound_word(7, 0x184).unwrap(), 0.25f32.to_bits());
    assert_eq!(audio.read_sound_word(7, 0x1a0).unwrap(), 0.6f32.to_bits());
    assert_eq!(audio.read_sound_word(7, 0x1a8).unwrap(), 1.2f32.to_bits());
    // GetRange/GetCone trivial copies read back the stored words.
    assert_eq!(audio.sound_range(&id).unwrap(), [2.0, 20.0, 0.5, 0.25]);
    assert_eq!(audio.sound_cone(&id).unwrap(), [0.6, 1.2]);
    let dur = audio.sound_duration_ms(&id).unwrap();
    assert!(dur > 0);
    audio.set_sound_frequency(&id, 2.0).unwrap();
    assert_eq!(audio.sound_duration_ms(&id).unwrap(), dur);
    audio.seek_sound_ms(&id, 0).unwrap();
    assert_eq!(audio.sound_position_ms(&id).unwrap(), 0);
    audio.create_sound_at(9).unwrap();
    let empty = audio.sound_id_from_handle(9).unwrap();
    assert_eq!(audio.sound_position_ms(&empty).unwrap(), 0);
    assert_eq!(audio.sound_duration_ms(&empty).unwrap(), 0);
    audio.release_sound(&empty).unwrap();
    assert_eq!(audio.live_sound_count().unwrap(), 1);
}
#[test]
fn pin_is_playing_logical_boundary_vectors() {
    // RVA 0x57c40 boundary vectors incl. wrap + real/logical split.
    let (audio, id) = loaded();
    audio.set_sound_kind(&id, SoundKind::Ambient).unwrap();
    audio.set_sound_enabled(&id, true).unwrap();
    assert!(audio.sound_is_playing(&id, PlaybackQuery::Logical).unwrap());
    assert!(!audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    audio.update_sound(&id).unwrap();
    assert!(audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    assert_eq!(audio.sound_virtual(7, 0x54, &[0]).unwrap(), 1);
    assert_eq!(audio.sound_virtual(7, 0x54, &[1]).unwrap(), 1);
    audio.seek_sound_ms(&id, u32::MAX / 2).unwrap();
    audio.update_sound(&id).unwrap();
    assert_eq!(
        audio.sound(7).unwrap().state,
        crate::runtime_audio::PlaybackState::Stop
    );
}
#[test]
fn pin_seton_update_parity_and_range_clamp() {
    // RVAs 0x58280/0x58530/0x58540: ForceUpdate==Update parity, SetOn
    // order, scaled-range clamp edge, out-of-range pause keeps cursor.
    let (audio, id) = loaded();
    audio.set_sound_kind(&id, SoundKind::Point).unwrap();
    audio.set_sound_playback(&id, true, false).unwrap();
    assert!(!audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    audio.sound_virtual(7, 0x90, &[]).unwrap();
    assert!(audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    audio.set_sound_range(&id, [10.0, 10.0, 0.0, 0.5]).unwrap();
    audio.set_world_scale(7, 1.0).unwrap();
    audio.update_sound(&id).unwrap();
    let s = audio.sound(7).unwrap();
    assert_eq!((s.range[0], s.range[1], s.range[2]), (10.0, 10.0, 0.0));
    audio
        .set_position(7, [1000.0, 0.0, 0.0], [0.0, 0.0, 1.0])
        .unwrap();
    audio.update_sound(&id).unwrap();
    assert!(!audio.sound_is_playing(&id, PlaybackQuery::Voice).unwrap());
    assert_eq!(audio.live_sound_count().unwrap(), 1);
}
