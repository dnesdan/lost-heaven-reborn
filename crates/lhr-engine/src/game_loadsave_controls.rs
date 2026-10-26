//! G_LoadSaveClass::ApplyControls, PC Game.exe 0x607a60.
//!
//! Source: mafia-re f42a25d, game/ui/g_loadsave.cpp. The function runs over
//! the real profile's 60 bindings and the actual C_controls owner. It ignores
//! native false returns from SetControl and stops only at a host dependency
//! failure. Profile loading, default binding generation and menu input remain
//! separate missing dependencies.

use crate::game_controls::{
    ControlBinding, ControlBindingBackend, ControlFailure, GameControls, LiveControlBinding,
};

pub const PROFILE_CONTROL_COUNT: usize = 60;

/// Actual profile-owned binding storage. Cloning this handle aliases the same
/// 60 rows; no row snapshot is held while SetControl calls the graph or joy.
#[derive(Clone)]
pub struct ProfileBindings {
    rows: std::rc::Rc<std::cell::RefCell<[ControlBinding; PROFILE_CONTROL_COUNT]>>,
}
impl ProfileBindings {
    pub fn from_rows(rows: [ControlBinding; PROFILE_CONTROL_COUNT]) -> Self {
        Self {
            rows: std::rc::Rc::new(std::cell::RefCell::new(rows)),
        }
    }
    pub fn read(&self, index: usize) -> ControlBinding {
        self.rows.borrow()[index]
    }
    pub fn replace(&self, index: usize, binding: ControlBinding) {
        self.rows.borrow_mut()[index] = binding;
    }
}

pub fn apply_controls<B: ControlBindingBackend>(
    controls: &mut GameControls,
    bindings: &ProfileBindings,
    backend: &mut B,
) -> Result<(), ControlFailure<B::Error>> {
    controls.clear_controls();
    for index in 0..PROFILE_CONTROL_COUNT {
        // The PC caller discards SetControl's bool, including a failed slot.
        // Its argument points into the actual profile row. SetControl reads
        // that row at its own reached read points, including after callbacks.
        let read = || bindings.read(index);
        let _ = controls.set_control(LiveControlBinding::External(&read), index as i32, backend)?;
    }
    controls.set_mouse_scale(400, 400, 400);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_controls::{
        ControlAllocationSeed, ControlGraph, ControlJoystick, ControlRefBackend,
    };

    struct Backend {
        definitions: usize,
        fail_first_definition: bool,
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
            self.definitions += 1;
            if self.fail_first_definition {
                Err("graph")
            } else {
                Ok(())
            }
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
    fn seed() -> ControlAllocationSeed {
        ControlAllocationSeed {
            last_input: [0xa5; 12],
            keys: [0xa5; 64],
            key_refs: [0xa5; 64],
            mouse_refs: [0xa5; 4],
            key_mask: 0x5a5a_5a5a_a5a5_a5a5,
            mouse_mask: 0xa5a5_a5a5,
            name: [0xa5; 32],
        }
    }
    #[test]
    fn profile_applies_sixty_live_bindings_and_publishes_scale_last() {
        let mut controls = GameControls::construct(seed());
        let mut backend = Backend {
            definitions: 0,
            fail_first_definition: false,
        };
        controls
            .init(Some(ControlGraph(1)), &[], &mut backend)
            .unwrap();
        let mut bindings = [ControlBinding {
            half: 0,
            device: 0,
            code: 0,
            joystick_index: 0,
        }; PROFILE_CONTROL_COUNT];
        bindings[0].device = 1;
        bindings[0].code = 37;
        let bindings = ProfileBindings::from_rows(bindings);
        apply_controls(&mut controls, &bindings, &mut backend).unwrap();
        assert_eq!(backend.definitions, 1);
        assert_eq!(controls.binding(0), Some(bindings.read(0)));
        assert_eq!(controls.binding(59), Some(bindings.read(59)));
        assert_eq!(controls.mouse_scale(), [400.0; 3]);
    }
    #[test]
    fn graph_failure_leaves_control_prefix_and_original_scale() {
        let mut controls = GameControls::construct(seed());
        let mut backend = Backend {
            definitions: 0,
            fail_first_definition: false,
        };
        controls
            .init(Some(ControlGraph(1)), &[], &mut backend)
            .unwrap();
        backend.fail_first_definition = true;
        let mut bindings = [ControlBinding {
            half: 0,
            device: 0,
            code: 0,
            joystick_index: 0,
        }; PROFILE_CONTROL_COUNT];
        bindings[0].device = 1;
        bindings[0].code = 37;
        let bindings = ProfileBindings::from_rows(bindings);
        assert_eq!(
            apply_controls(&mut controls, &bindings, &mut backend),
            Err(ControlFailure::Backend("graph"))
        );
        assert_eq!(backend.definitions, 1);
        assert_eq!(
            controls.binding(0),
            Some(ControlBinding {
                half: 0,
                device: 0,
                code: 0,
                joystick_index: 0
            })
        );
        assert_eq!(controls.binding(1), None);
        assert_eq!(controls.mouse_scale(), [20.0; 3]);
    }

    struct MutatingProfileBackend {
        profile: ProfileBindings,
        callbacks: usize,
        fail_first: bool,
    }
    impl ControlRefBackend for MutatingProfileBackend {
        type Error = &'static str;
        fn add_ref(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
        fn release(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    impl ControlBindingBackend for MutatingProfileBackend {
        fn define_keys(
            &mut self,
            _: ControlGraph,
            _: &mut GameControls,
        ) -> Result<(), Self::Error> {
            self.callbacks += 1;
            if self.callbacks == 1 {
                let mut current = self.profile.read(0);
                current.code = 38;
                self.profile.replace(0, current);
                let mut later = self.profile.read(1);
                later.device = 1;
                later.code = 39;
                self.profile.replace(1, later);
                if self.fail_first {
                    return Err("graph");
                }
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
            Ok(())
        }
    }

    #[test]
    fn profile_callback_changes_current_and_later_real_rows() {
        let mut controls = GameControls::construct(seed());
        let mut rows = [ControlBinding {
            half: 0,
            device: 0,
            code: 0,
            joystick_index: 0,
        }; PROFILE_CONTROL_COUNT];
        rows[0].device = 1;
        rows[0].code = 37;
        let profile = ProfileBindings::from_rows(rows);
        let mut backend = MutatingProfileBackend {
            profile: profile.clone(),
            callbacks: 0,
            fail_first: false,
        };
        controls
            .init(Some(ControlGraph(1)), &[], &mut backend)
            .unwrap();
        apply_controls(&mut controls, &profile, &mut backend).unwrap();
        assert_eq!(backend.callbacks, 2);
        assert_eq!(controls.binding(0), Some(profile.read(0)));
        assert_eq!(controls.binding(0).unwrap().code, 38);
        assert_eq!(controls.binding(1), Some(profile.read(1)));
        assert_eq!(controls.binding(1).unwrap().code, 39);
        assert_eq!(controls.keys()[0], 37);
        assert_eq!(controls.keys()[1], 39);
        assert_eq!(controls.mouse_scale(), [400.0; 3]);
    }

    #[test]
    fn profile_callback_failure_keeps_current_and_later_row_changes_but_stops_apply() {
        let mut controls = GameControls::construct(seed());
        let mut rows = [ControlBinding {
            half: 0,
            device: 0,
            code: 0,
            joystick_index: 0,
        }; PROFILE_CONTROL_COUNT];
        rows[0].device = 1;
        rows[0].code = 37;
        let profile = ProfileBindings::from_rows(rows);
        let mut backend = MutatingProfileBackend {
            profile: profile.clone(),
            callbacks: 0,
            fail_first: true,
        };
        controls
            .init(Some(ControlGraph(1)), &[], &mut backend)
            .unwrap();
        assert_eq!(
            apply_controls(&mut controls, &profile, &mut backend),
            Err(ControlFailure::Backend("graph"))
        );
        assert_eq!(backend.callbacks, 1);
        assert_eq!(profile.read(0).code, 38);
        assert_eq!(profile.read(1).code, 39);
        assert_eq!(controls.binding(0).unwrap().device, 0);
        assert_eq!(controls.binding(1), None);
        assert_eq!(controls.keys()[0], 37);
        assert_eq!(controls.allocation_seed().key_refs[0], 0);
        assert_eq!(controls.mouse_scale(), [20.0; 3]);
    }
}
