//! Ordered initialization uses the retained scene at the reached render call.
use super::*;
use crate::game_init_order::{InitAttempt, InitRunError, InitStep};
use crate::scene_render::SceneSnapshotSequence;

fn hud() -> crate::indicators::IndicatorsState {
    crate::indicators::IndicatorsState::constructed(crate::indicators::TimerClock {
        hour: 9,
        minute: 0,
        second: 0,
    })
}

/// Stage the C_game state the reached `Init` span overwrites, so every
/// clear and store is observable. The returned sound frame is the borrowed
/// identity the stopped-sound record names; the caller releases it.
fn stage_dirty_game_state(session: &mut MissionLoadSession) -> crate::frame_sound::SoundKey {
    let sound = session.arena.create_owned_sound_frame().unwrap();
    let lists = &mut session.game_lists;
    lists
        .stop_sounds
        .push(crate::game_housekeeping::StoppedSound {
            sound,
            volume: 0.5,
            volume_out: 0.25,
            looped: true,
            time: 40,
        });
    lists.paused_i_shows.extend([3, -1]);
    lists
        .free_shadow_names
        .push(vec![0u8; 32].into_boxed_slice());
    lists.unknown_140.extend([11, 12]);
    *session.game_inline.score_mut() = crate::game_housekeeping::ScoreState {
        score: 7,
        score_on: true,
    };
    session.game_inline.set_actor_lightness(true);
    sound
}

#[test]
fn ordered_init_presents_after_mute_and_stops_before_later_owners() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    session.install_indicators(hud()).unwrap();
    load(&mut session, &[]).unwrap();
    let scene = session.scene().unwrap().unwrap();
    let camera = session.arena().scene_active_camera(scene).unwrap();
    let counts = session.owned_counts().unwrap();
    let audio = session.audio().clone();
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let mut presentations = 0;
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |frame| {
        assert_eq!(audio.master_gain().unwrap(), 0.0);
        assert_eq!(frame.frame.viewport, [0, 0, 64, 64]);
        presentations += 1;
        Ok(())
    });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the next unavailable owner must stop Init");
    };
    // The whole ResetMissionState span completes; the AI manager's police
    // shift (GAME 0x53ba50) is the next unowned native step.
    assert_eq!(failure.step, InitStep::PoliceBeginShift);
    assert!(failure.error.0.contains("ZacatekSmeny, GAME 0x53ba50"));
    assert_eq!(presentations, 1);
    assert!(session.open_restore_volume());
    assert_eq!(session.scene().unwrap(), Some(scene));
    assert_eq!(session.arena().scene_active_camera(scene).unwrap(), camera);
    assert_eq!(session.owned_counts().unwrap(), counts);
    assert!(!session.police_init().active());
    assert!(!session.traffic_flags().valid_scream());
    assert_eq!(sequence.next_number(), Ok(2));
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("Init must not replay presentation");
        }),
        Err(InitRunError::AlreadyEntered)
    );
    discard(&mut session);
}

#[test]
fn presented_init_render_unlocks_every_buffer_before_presentation() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    session.install_indicators(hud()).unwrap();
    load(&mut session, &[]).unwrap();
    let visual = super::install_scene_visual(&mut session);
    // LoadCollision's LOD0 lock, kept like the native pointer.
    let view = session
        .arena
        .lock_visual_lod0_vertices(visual)
        .unwrap()
        .unwrap();
    let locked = |session: &MissionLoadSession| {
        let managers = session
            .arena()
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .managed()
            .managers();
        (
            managers.vertex_locked().to_vec(),
            managers.index_locked().to_vec(),
        )
    };
    let (held, _) = locked(&session);
    assert_eq!(held.len(), 1);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let mut presentations = 0;
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
        presentations += 1;
        Ok(())
    });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the next unavailable owner must stop Init");
    };
    assert_eq!(failure.step, InitStep::PoliceBeginShift);
    assert_eq!(presentations, 1);
    // I3D_driver::Render (LS3DF 0x10062030) unlocks the vertex manager
    // (0x100697e0) and then the index manager before it draws: no buffer
    // stays locked and the lock lists are empty.
    assert_eq!(locked(&session), (vec![], vec![]));
    // The collision's live view keeps reading the same storage.
    assert_eq!(view.position(1), Some([1., 0., 0.]));
    // The next access takes a fresh backend lock instead of reusing the
    // stale pointer and flags.
    let again = session
        .arena
        .lock_visual_lod0_vertices(visual)
        .unwrap()
        .unwrap();
    assert!(again.same_buffer(&view));
    assert_eq!(locked(&session).0, held);
    drop((view, again));
    discard(&mut session);
}

#[test]
fn presented_init_runs_the_reset_span_in_native_order_with_the_hud() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    let mut indicators = hud();
    indicators.bonus_set_count(7);
    session.install_indicators(indicators).unwrap();
    load(&mut session, &[]).unwrap();
    let sound = stage_dirty_game_state(&mut session);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the police shift must stop Init");
    };
    assert_eq!(failure.step, InitStep::PoliceBeginShift);
    // GAME 0x5a08c3-0x5a098c: lists emptied, ScoreSetOn(false) hides the
    // HUD bonus (0x7fffffff), score zeroed, flags stored.
    assert_eq!(
        session.game_lists(),
        &crate::game_housekeeping::GameInitLists::default()
    );
    let game = session.game_state();
    assert_eq!(
        game.score(),
        crate::game_housekeeping::ScoreState::default()
    );
    assert_eq!(
        session.indicators().unwrap().bonus_count(),
        crate::indicators::BONUS_NONE
    );
    assert!(!game.camera_rot_repair());
    assert!(game.heal_cheat());
    assert!(game.stacks_clear());
    assert_eq!(game.death_fade_time(), 7000);
    assert!(!game.actor_lightness());
    // Later steps never ran.
    assert!(!session.police_init().active());
    // Clearing the record released nothing: the sound is still live.
    session.arena.release(sound.frame()).unwrap();
    discard(&mut session);
}

#[test]
fn presented_init_without_hud_stops_at_score_set_on_keeping_earlier_clears() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let sound = stage_dirty_game_state(&mut session);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the absent HUD must stop Init");
    };
    assert_eq!(failure.step, InitStep::ResetMissionState);
    assert!(failure
        .error
        .0
        .ends_with("Init ScoreSetOn mafiaIndicators owner unavailable"));
    assert!(session.open_restore_volume());
    let lists = session.game_lists();
    assert!(lists.stop_sounds.is_empty());
    assert!(lists.paused_i_shows.is_empty());
    assert!(lists.free_shadow_names.is_empty());
    // The direct score_on store precedes the call; nothing after it runs.
    let game = session.game_state();
    assert_eq!(
        game.score(),
        crate::game_housekeeping::ScoreState {
            score: 7,
            score_on: false,
        }
    );
    assert!(!game.heal_cheat());
    assert!(!game.stacks_clear());
    assert_eq!(game.death_fade_time(), 0);
    assert!(game.actor_lightness());
    assert_eq!(lists.unknown_140, [11, 12]);
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(())),
        Err(InitRunError::AlreadyEntered)
    );
    session.arena.release(sound.frame()).unwrap();
    discard(&mut session);
}

#[test]
fn indicators_install_once_and_keep_the_live_owner() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    assert!(session.indicators().is_none());
    let mut first = hud();
    first.bonus_set_count(3);
    session.install_indicators(first).unwrap();
    assert_eq!(
        session.install_indicators(hud()),
        Err("mafiaIndicators already installed".into())
    );
    assert_eq!(session.indicators().unwrap().bonus_count(), 3);
}

#[test]
fn failed_presentation_retains_muted_prefix_without_restore_or_retry() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let mut attempt = InitAttempt::default();
    let mut sequence = SceneSnapshotSequence::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
        Err("injected GPU submission failure".into())
    });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("presentation failure must propagate");
    };
    assert_eq!(failure.step, InitStep::ResetMissionState);
    assert!(failure.error.0.contains("injected GPU submission failure"));
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(session.game_init_reset_observation().3, -1);
    assert!(!session.open_restore_volume());
    // Even a new outer attempt cannot repeat an interrupted owner's effects.
    assert!(InitAttempt::default()
        .run_with_presentation(&mut session, [64, 64], &mut sequence, |_| panic!("retry"))
        .is_err());
    assert_eq!(sequence.next_number(), Ok(2));
    discard(&mut session);
}

#[test]
fn unavailable_snapshot_never_calls_presenter_or_skips_the_reached_prefix() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let mut sequence = SceneSnapshotSequence::default();
    let result =
        InitAttempt::default().run_with_presentation(&mut session, [0, 64], &mut sequence, |_| {
            panic!("invalid snapshot must not be presented")
        });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("snapshot failure must propagate");
    };
    assert_eq!(failure.step, InitStep::ResetMissionState);
    assert!(failure.error.0.contains("snapshot viewport"));
    assert_eq!(sequence.next_number(), Ok(1));
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(session.game_init_reset_observation().3, -1);
    assert!(!session.open_restore_volume());
    discard(&mut session);
}

#[test]
fn admission_failure_does_not_create_a_snapshot_or_call_presentation() {
    let fixture = Fixture::new(None, None, None);
    let mut session = fixture.session();
    let mut sequence = SceneSnapshotSequence::default();
    let result =
        InitAttempt::default().run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("unloaded scene")
        });
    assert!(
        matches!(result, Err(InitRunError::Callback(failure)) if failure.step == InitStep::GuardNotInitialized)
    );
    assert_eq!(sequence.next_number(), Ok(1));
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    assert!(!session.open_restore_volume());
}

#[test]
fn presentation_unwind_keeps_one_shot_guards_and_actual_owners() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let before = session.owned_counts().unwrap();
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("injected presentation unwind");
        });
    }))
    .is_err());
    assert_eq!(attempt.last(), Some(InitStep::ResetMissionState));
    assert_eq!(attempt.run(&mut session), Err(InitRunError::AlreadyEntered));
    assert!(!session.open_restore_volume());
    assert_eq!(session.owned_counts().unwrap(), before);
    discard(&mut session);
}
