//! Ordered initialization uses the retained scene at the reached render call.
use super::*;
use crate::game_init_order::{InitAttempt, InitRunError, InitStep};
use crate::scene_render::SceneSnapshotSequence;

#[test]
fn ordered_init_presents_after_mute_and_stops_before_later_owners() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
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
    assert_eq!(failure.step, InitStep::ResetMissionState);
    assert!(failure.error.0.ends_with("stop_sounds owner unavailable"));
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
fn failed_presentation_retains_muted_prefix_without_restore_or_retry() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
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
    assert!(
        InitAttempt::default()
            .run_with_presentation(&mut session, [64, 64], &mut sequence, |_| panic!("retry"))
            .is_err()
    );
    assert_eq!(sequence.next_number(), Ok(2));
    discard(&mut session);
}

#[test]
fn unavailable_snapshot_never_calls_presenter_or_skips_the_reached_prefix() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
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
    let before = session.owned_counts().unwrap();
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
                panic!("injected presentation unwind");
            });
        }))
        .is_err()
    );
    assert_eq!(attempt.last(), Some(InitStep::ResetMissionState));
    assert_eq!(attempt.run(&mut session), Err(InitRunError::AlreadyEntered));
    assert!(!session.open_restore_volume());
    assert_eq!(session.owned_counts().unwrap(), before);
    discard(&mut session);
}
