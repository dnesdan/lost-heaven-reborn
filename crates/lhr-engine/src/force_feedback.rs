//! C_ff_manager::GameInit, PC Game.exe RVA 0x446b0, SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Reference: mafia-re fe7e746 game/input/C_ff_manager.cpp and fresh REA
//! ev_a75ff6c6299369b9dc4944bb4f236873af70a89f07873fd1d2b471dcb35bd8cb.
//! Both GetInfo calls read byte +0x14 of their output (ESP+0x28 after
//! callee argument cleanup); the decompiler's stack aliases differ.
//! Only the reached scalar state is represented, not the whole constructor,
//! effect vectors, effect creation, ticking or destruction. GetInfo uses the
//! process-global joy inventory, not C_controls' retained joystick list.

#![forbid(unsafe_code)]

use crate::game_controls::GameControls;

pub trait JoystickInfoBackend {
    type Error;
    fn force_feedback(
        &mut self,
        global_index: i32,
        controls: &mut GameControls,
    ) -> Result<u8, Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ForceFeedbackFailure<Error> {
    MissingFirstBinding,
    Backend(Error),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ForceFeedbackState {
    human_effects: u8,
    car_effects: u8,
    reset_value: i32,
    game_init_complete: bool,
}

impl ForceFeedbackState {
    /// Caller-supplied process state. The session supplies the zeroed global
    /// and InitSystem::Reset state; this is not the native constructor.
    pub fn from_process_state(human_effects: u8, car_effects: u8, reset_value: i32) -> Self {
        Self {
            human_effects,
            car_effects,
            reset_value,
            game_init_complete: false,
        }
    }

    pub fn human_effects(&self) -> bool {
        self.human_effects != 0
    }
    pub fn car_effects(&self) -> bool {
        self.car_effects != 0
    }
    pub fn effect_flag_bytes(&self) -> [u8; 2] {
        [self.human_effects, self.car_effects]
    }
    pub fn reset_value(&self) -> i32 {
        self.reset_value
    }
    pub fn game_init_complete(&self) -> bool {
        self.game_init_complete
    }

    pub fn game_init<Backend: JoystickInfoBackend>(
        &mut self,
        controls: &mut GameControls,
        backend: &mut Backend,
    ) -> Result<(), ForceFeedbackFailure<Backend::Error>> {
        self.game_init_complete = false;
        let mut binding = controls
            .binding(1)
            .ok_or(ForceFeedbackFailure::MissingFirstBinding)?;
        self.human_effects = if binding.device == 3 {
            backend
                .force_feedback(binding.joystick_index, controls)
                .map_err(ForceFeedbackFailure::Backend)?
        } else {
            0
        };
        controls.copy_control(0x26, Some(&mut binding));
        if binding.device == 3 {
            let car_effects = backend
                .force_feedback(binding.joystick_index, controls)
                .map_err(ForceFeedbackFailure::Backend)?;
            self.reset_value = 0;
            self.car_effects = car_effects;
        } else {
            self.car_effects = 0;
            self.reset_value = 0;
        }
        self.game_init_complete = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_controls::{
        ControlAllocationSeed, ControlBinding, ControlBindingBackend, ControlGraph,
        ControlJoystick, ControlRefBackend, LiveControlBinding,
    };

    struct Backend {
        calls: Vec<i32>,
        replies: std::collections::VecDeque<Result<u8, &'static str>>,
        redirect_second: Option<ControlBinding>,
    }
    impl ControlRefBackend for Backend {
        type Error = &'static str;
        fn add_ref(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
        fn release(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    impl ControlBindingBackend for Backend {
        fn define_keys(
            &mut self,
            _: ControlGraph,
            _: &mut GameControls,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
        fn set_axis_mode(
            &mut self,
            _: ControlJoystick,
            _: u16,
            _: u32,
            _: &mut GameControls,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    impl JoystickInfoBackend for Backend {
        type Error = &'static str;
        fn force_feedback(
            &mut self,
            index: i32,
            controls: &mut GameControls,
        ) -> Result<u8, Self::Error> {
            self.calls.push(index);
            if let Some(binding) = self.redirect_second.take() {
                assert!(controls.replace_binding_slot(0x26, binding));
            }
            self.replies.pop_front().expect("unexpected joystick read")
        }
    }
    fn setup(
        last_slot: Option<i32>,
        replies: Vec<Result<u8, &'static str>>,
    ) -> (GameControls, Backend) {
        let mut backend = Backend {
            calls: Vec::new(),
            replies: replies.into(),
            redirect_second: None,
        };
        let mut controls = GameControls::construct(ControlAllocationSeed {
            last_input: [0; 12],
            keys: [0; 64],
            key_refs: [0; 64],
            mouse_refs: [0; 4],
            key_mask: 0,
            mouse_mask: 0,
            name: [0; 32],
        });
        if let Some(index) = last_slot {
            controls
                .set_control(LiveControlBinding::Null, index, &mut backend)
                .unwrap();
        }
        (controls, backend)
    }
    fn joy(index: i32) -> ControlBinding {
        ControlBinding {
            device: 3,
            code: 0,
            half: 0,
            joystick_index: index,
        }
    }

    #[test]
    fn missing_first_control_is_not_a_zero_binding() {
        let (mut controls, mut backend) = setup(None, vec![]);
        let mut state = ForceFeedbackState::from_process_state(4, 5, 6);
        assert_eq!(
            state.game_init(&mut controls, &mut backend),
            Err(ForceFeedbackFailure::MissingFirstBinding)
        );
        assert_eq!(state.effect_flag_bytes(), [4, 5]);
        assert_eq!(state.reset_value(), 6);
        assert!(!state.game_init_complete());
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn second_out_of_range_control_reuses_first_binding() {
        let (mut controls, mut backend) = setup(Some(1), vec![Ok(0x80), Ok(2)]);
        controls.replace_binding_slot(1, joy(7));
        let mut state = ForceFeedbackState::from_process_state(4, 5, 6);
        state.game_init(&mut controls, &mut backend).unwrap();
        assert_eq!(backend.calls, [7, 7]);
        assert_eq!(state.effect_flag_bytes(), [0x80, 2]);
        assert!(state.human_effects() && state.car_effects());
        assert_eq!(state.reset_value(), 0);
    }

    #[test]
    fn second_binding_is_read_after_first_get_info_callback() {
        let (mut controls, mut backend) = setup(Some(0x26), vec![Ok(1), Ok(0)]);
        controls.replace_binding_slot(1, joy(2));
        backend.redirect_second = Some(joy(9));
        let mut state = ForceFeedbackState::from_process_state(4, 5, 6);
        state.game_init(&mut controls, &mut backend).unwrap();
        assert_eq!(backend.calls, [2, 9]);
        assert_eq!(state.effect_flag_bytes(), [1, 0]);
    }

    #[test]
    fn get_info_failure_preserves_the_reached_store_prefix() {
        for fail_first in [true, false] {
            let replies = if fail_first {
                vec![Err("device")]
            } else {
                vec![Ok(3), Err("device")]
            };
            let (mut controls, mut backend) = setup(Some(0x26), replies);
            controls.replace_binding_slot(1, joy(-1));
            controls.replace_binding_slot(0x26, joy(4));
            let mut state = ForceFeedbackState::from_process_state(4, 5, 6);
            assert_eq!(
                state.game_init(&mut controls, &mut backend),
                Err(ForceFeedbackFailure::Backend("device"))
            );
            assert_eq!(
                state.effect_flag_bytes(),
                [if fail_first { 4 } else { 3 }, 5]
            );
            assert_eq!(state.reset_value(), 6);
            assert!(!state.game_init_complete());
            assert_eq!(
                backend.calls,
                if fail_first { vec![-1] } else { vec![-1, 4] }
            );
        }
    }

    #[test]
    fn non_joystick_bindings_do_not_read_the_global_inventory() {
        let (mut controls, mut backend) = setup(Some(0x26), vec![]);
        let mut state = ForceFeedbackState::from_process_state(4, 5, 6);
        state.game_init(&mut controls, &mut backend).unwrap();
        assert!(backend.calls.is_empty());
        assert_eq!(state.effect_flag_bytes(), [0, 0]);
        assert!(state.game_init_complete());
    }
}
