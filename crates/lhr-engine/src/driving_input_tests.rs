//! Host input scheduling: short taps, held actions, catch-up and reset.
use super::{Controls, FixedClock, KeyboardInput};
use winit::keyboard::KeyCode;

#[test]
fn released_tap_survives_redraws_without_simulation_then_reaches_one_step() {
    let mut input = KeyboardInput::default();
    let mut clock = FixedClock::default();
    input.press(KeyCode::KeyW, false);
    input.remove(&KeyCode::KeyW);
    assert!(!input.contains(&KeyCode::KeyW));
    for _ in 0..3 {
        assert_eq!(clock.advance(4), 0);
    }
    assert_eq!(clock.advance(4), 1);
    assert_eq!(input.take_driving_controls().throttle, 1.0);
    assert_eq!(input.take_driving_controls(), Controls::default());
}

#[test]
fn catch_up_steps_do_not_stretch_a_released_tap() {
    let mut input = KeyboardInput::default();
    input.press(KeyCode::Space, false);
    input.remove(&KeyCode::Space);
    let steps = FixedClock::default().advance(80);
    let samples: Vec<_> = (0..steps)
        .map(|_| input.take_driving_controls().handbrake)
        .collect();
    assert_eq!(samples, [true, false, false, false, false]);
}

#[test]
fn held_input_is_continuous_and_repeat_does_not_queue_an_extra_press() {
    let mut input = KeyboardInput::default();
    input.press(KeyCode::KeyW, false);
    for _ in 0..5 {
        assert_eq!(input.take_driving_controls().throttle, 1.0);
        input.press(KeyCode::KeyW, true);
    }
    input.remove(&KeyCode::KeyW);
    assert_eq!(input.take_driving_controls(), Controls::default());
    input.press(KeyCode::KeyW, true);
    assert_eq!(input.take_driving_controls(), Controls::default());
    assert!(!input.contains(&KeyCode::KeyW));
}

#[test]
fn focus_pause_ui_and_reload_reset_retire_held_and_pending_actions() {
    let mut input = KeyboardInput::default();
    input.press(KeyCode::KeyS, false);
    input.press(KeyCode::Space, false);
    input.remove(&KeyCode::Space);
    input.clear();
    assert!(!input.contains(&KeyCode::KeyS));
    assert_eq!(input.take_driving_controls(), Controls::default());
    input.press(KeyCode::KeyS, true);
    assert_eq!(input.take_driving_controls(), Controls::default());
    input.press(KeyCode::KeyS, false);
    assert_eq!(input.take_driving_controls().brake, 1.0);
}

#[test]
fn opposing_press_cancels_for_one_step_without_releasing_the_held_direction() {
    let mut input = KeyboardInput::default();
    input.press(KeyCode::KeyA, false);
    assert_eq!(input.take_driving_controls().steer, -1.0);
    input.press(KeyCode::KeyD, false);
    input.remove(&KeyCode::KeyD);
    assert_eq!(input.take_driving_controls().steer, 0.0);
    assert_eq!(input.take_driving_controls().steer, -1.0);
    input.remove(&KeyCode::KeyA);
    assert_eq!(input.take_driving_controls().steer, 0.0);
}

#[test]
fn non_driving_keys_keep_immediate_camera_state_without_driving_effects() {
    let mut input = KeyboardInput::default();
    input.press(KeyCode::ArrowLeft, false);
    assert!(input.contains(&KeyCode::ArrowLeft));
    assert_eq!(input.take_driving_controls(), Controls::default());
    assert!(input.contains(&KeyCode::ArrowLeft));
    input.remove(&KeyCode::ArrowLeft);
    assert!(!input.contains(&KeyCode::ArrowLeft));
    assert_eq!(input.take_driving_controls(), Controls::default());
}
