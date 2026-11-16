//! Explicit keyboard/mouse-only host input composition for mission startup.
//! This mode does not enumerate or pretend to implement joystick devices.
//! It applies the actual profile to C_controls and IGraph's CPU keyboard
//! watch-key owner. Platform event polling and C_controls::Update remain
//! separate dependencies; this does not establish interactive gameplay.

#![forbid(unsafe_code)]

use crate::force_feedback::{ForceFeedbackState, JoystickInfoBackend};
use crate::game_controls::{
    ControlAllocationSeed, ControlBindingBackend, ControlGraph, ControlJoystick, ControlRefBackend,
    GameControls,
};
use crate::game_loadsave_controls::{apply_controls, ProfileBindings};
use crate::input_keyboard::{define_keys, KeyboardState, KB_OK};

pub struct KeyboardMouseInput {
    controls: GameControls,
    profile: ProfileBindings,
    backend: KeyboardMouseBackend,
}

struct KeyboardMouseBackend {
    keyboard: KeyboardState,
    graph: ControlGraph,
}

impl KeyboardMouseInput {
    pub fn from_profile(profile: ProfileBindings) -> Result<Self, String> {
        let mut backend = KeyboardMouseBackend {
            keyboard: KeyboardState::default(),
            graph: ControlGraph(1),
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
        let initialized = controls
            .init(Some(backend.graph), &[], &mut backend)
            .map_err(|error| format!("Keyboard/mouse controls Init: {error:?}"))?;
        if !initialized {
            return Err("Keyboard/mouse controls Init refused the graph".into());
        }
        apply_controls(&mut controls, &profile, &mut backend)
            .map_err(|error| format!("Keyboard/mouse ApplyControls: {error:?}"))?;
        Ok(Self {
            controls,
            profile,
            backend,
        })
    }

    pub fn controls(&self) -> &GameControls {
        &self.controls
    }
    pub fn keyboard(&self) -> &KeyboardState {
        &self.backend.keyboard
    }
    pub fn profile(&self) -> &ProfileBindings {
        &self.profile
    }

    pub fn initialize_force_feedback(
        &mut self,
        state: &mut ForceFeedbackState,
    ) -> Result<(), String> {
        state
            .game_init(&mut self.controls, &mut self.backend)
            .map_err(|error| format!("ff_manager.GameInit (GAME 0x4446b0): {error:?}"))
    }
}

impl ControlRefBackend for KeyboardMouseBackend {
    type Error = &'static str;
    fn add_ref(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
        Err("joystick references unavailable in explicit keyboard/mouse mode")
    }
    fn release(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
        Err("joystick references unavailable in explicit keyboard/mouse mode")
    }
}

impl ControlBindingBackend for KeyboardMouseBackend {
    fn define_keys(
        &mut self,
        graph: ControlGraph,
        owner: &mut GameControls,
    ) -> Result<(), Self::Error> {
        if graph != self.graph {
            return Err("keyboard graph identity mismatch");
        }
        if define_keys(&mut self.keyboard, Some(owner.keys())) != KB_OK {
            return Err("IGraph::DefineKeys rejected the control keys");
        }
        Ok(())
    }
    fn set_axis_mode(
        &mut self,
        _: ControlJoystick,
        _: u16,
        _: u32,
        _: &mut GameControls,
    ) -> Result<(), Self::Error> {
        Err("joystick axes unavailable in explicit keyboard/mouse mode")
    }
}

impl JoystickInfoBackend for KeyboardMouseBackend {
    type Error = &'static str;
    fn force_feedback(&mut self, _: i32, _: &mut GameControls) -> Result<u8, Self::Error> {
        Err("global joystick unavailable in explicit keyboard/mouse mode")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_loadsave_controls::profile_bindings_after_load_save_init;
    use crate::game_loadsave_lifecycle::ProfileFileIntake;

    #[test]
    fn keyboard_mouse_mode_applies_real_watch_keys_without_inventing_joysticks() {
        let bindings =
            profile_bindings_after_load_save_init(ProfileFileIntake::NoProfileFile).unwrap();
        let mut input = KeyboardMouseInput::from_profile(bindings).unwrap();
        assert_eq!(input.keyboard().read_keys_buffer, *input.controls().keys());
        for key in [0xc8, 0x11, 0x1f, 0x1e, 0x20, 0x12, 0x3b, 0x2e] {
            assert!(input.keyboard().read_keys_buffer.contains(&key));
        }
        assert_eq!(input.controls().mouse_scale(), [400.0; 3]);
        assert!(input.controls().joysticks().is_empty());
        assert_eq!(input.profile().read(0x26).device, 3);
        assert_eq!(input.controls().binding(0x26).unwrap().device, 0);
        assert_eq!(input.controls().binding(59), Some(input.profile().read(59)));
        let mut state = ForceFeedbackState::from_process_state(0xa5, 0x5a, 77);
        input.initialize_force_feedback(&mut state).unwrap();
        assert_eq!(state.effect_flag_bytes(), [0, 0]);
        assert_eq!(state.reset_value(), 0);
        assert!(state.game_init_complete());
    }
}
