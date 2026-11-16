//! G_LoadSaveClass::ApplyControls, PC Game.exe 0x607a60.
//!
//! Source: mafia-re f42a25d, game/ui/g_loadsave.cpp. The function runs over
//! the real profile's 60 bindings and the actual C_controls owner. It ignores
//! native false returns from SetControl and stops only at a host dependency
//! failure. Profile loading and menu input remain
//! separate missing dependencies.
//!
//! DefaultControls (RVA 0x207600) and SetBinding (RVA 0x2073c0): mafia-re
//! fe7e746, game/ui/g_loadsave.cpp, checked against fresh REA Game.exe
//! analysis. The original DIK/device/axis codes below are file/control values,
//! not host key codes. Defaults clear only with bit 0x80; the unused row 0x20
//! is otherwise retained. Neither operation loads an existing profile.

use crate::game_controls::{
    ControlBinding, ControlBindingBackend, ControlFailure, GameControls, LiveControlBinding,
};

pub const PROFILE_CONTROL_COUNT: usize = 60;
pub const DEFAULTS_PLAYER: u32 = 1;
pub const DEFAULTS_CAR: u32 = 2;
pub const DEFAULTS_CLEAR: u32 = 0x80;

const EMPTY_BINDING: ControlBinding = ControlBinding {
    half: 0,
    device: 0,
    code: 0,
    joystick_index: 0,
};

const PLAYER_DEFAULTS: &[(usize, u16, u16, i32)] = &[
    (0x00, 1, 0xc8, 0),
    (0x01, 1, 0x11, 0),
    (0x02, 1, 0xd0, 0),
    (0x03, 1, 0x1f, 0),
    (0x04, 1, 0xcb, 0),
    (0x05, 1, 0x1e, 0),
    (0x06, 1, 0xcd, 0),
    (0x07, 1, 0x20, 0),
    (0x08, 1, 0x36, 0),
    (0x09, 1, 0x2a, 0),
    (0x0a, 2, 2, 0),
    (0x0b, 1, 0x16, 0),
    (0x0e, 1, 0x9d, 0),
    (0x0f, 1, 0x10, 0),
    (0x0c, 2, 1, 0),
    (0x0d, 1, 0x21, 0),
    (0x10, 1, 0x52, 0),
    (0x11, 1, 0x12, 0),
    (0x12, 2, 0x100, 0),
    (0x13, 2, 0x200, 0),
    (0x14, 2, 0x300, 1),
    (0x15, 1, 0xc9, 0),
    (0x16, 2, 0x300, 2),
    (0x17, 1, 0xd1, 0),
    (0x18, 1, 0x17, 0),
    (0x19, 1, 0x23, 0),
    (0x1a, 1, 0x0e, 0),
    (0x1f, 1, 0x0f, 0),
    (0x1b, 1, 0x3a, 0),
    (0x1c, 1, 0x26, 0),
    (0x1d, 1, 0x13, 0),
    (0x1e, 1, 0x3b, 0),
];

const CAR_DEFAULTS: &[(usize, u16, u16, i32)] = &[
    (0x21, 1, 0xc8, 0),
    (0x22, 3, 1, 2),
    (0x23, 1, 0xd0, 0),
    (0x24, 3, 1, 1),
    (0x25, 1, 0xcb, 0),
    (0x26, 3, 0, 2),
    (0x27, 1, 0xcd, 0),
    (0x28, 3, 0, 1),
    (0x29, 1, 0x39, 0),
    (0x2a, 0, 0, 0),
    (0x2b, 1, 0x3f, 0),
    (0x2c, 0, 0, 0),
    (0x2d, 1, 0x1e, 0),
    (0x2e, 0, 0, 0),
    (0x2f, 1, 0x2c, 0),
    (0x30, 0, 0, 0),
    (0x31, 1, 0x25, 0),
    (0x32, 0, 0, 0),
    (0x33, 1, 0x33, 0),
    (0x34, 3, 0x218, 0),
    (0x35, 1, 0x34, 0),
    (0x36, 3, 0x212, 0),
    (0x37, 1, 0x2d, 0),
    (0x38, 0, 0, 0),
    (0x39, 1, 0x32, 0),
    (0x3a, 1, 0x2e, 0),
    (0x3b, 1, 0x52, 0),
];

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

    /// Host bounds admission precedes the native direct stores; an invalid
    /// index has no defined native behavior within this profile owner.
    pub fn set_binding(
        &self,
        index: i32,
        device: u16,
        code: u16,
        joystick_index: i32,
        half: i32,
    ) -> Result<(), &'static str> {
        let mut rows = self.rows.borrow_mut();
        let destination = rows
            .get_mut(index as u32 as usize)
            .ok_or("SetBinding: index outside the profile's 60 rows")?;
        destination.device = device;
        destination.code = code;
        destination.joystick_index = joystick_index;
        destination.half = half;
        Ok(())
    }

    pub fn default_controls(&self, flags: u32) {
        if flags & DEFAULTS_CLEAR != 0 {
            self.rows.borrow_mut().fill(EMPTY_BINDING);
        }
        for (flag, defaults) in [
            (DEFAULTS_PLAYER, PLAYER_DEFAULTS),
            (DEFAULTS_CAR, CAR_DEFAULTS),
        ] {
            if flags & flag != 0 {
                for &(index, device, code, half) in defaults {
                    self.set_binding(index as i32, device, code, 0, half)
                        .expect("native defaults use valid profile indices");
                }
            }
        }
    }
}

/// Bounded startup adoption, not a port of AllDefault/ProfileLoad. Never
/// replace a present profile with defaults while its decoder is unavailable.
pub fn profile_bindings_after_load_save_init(
    intake: crate::game_loadsave_lifecycle::ProfileFileIntake,
) -> Result<ProfileBindings, &'static str> {
    use crate::game_loadsave_lifecycle::ProfileFileIntake;
    match intake {
        ProfileFileIntake::NoProfileFile => {
            let bindings = ProfileBindings::from_rows([EMPTY_BINDING; PROFILE_CONTROL_COUNT]);
            bindings.default_controls(0xffff);
            Ok(bindings)
        }
        ProfileFileIntake::ProfileFilePresent => {
            Err("ProfileLoad: existing profile bindings require the unported G_Stream decoder")
        }
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

    fn sentinel_profile() -> ProfileBindings {
        ProfileBindings::from_rows(
            [ControlBinding {
                half: -17,
                device: 0x1234,
                code: 0xabcd,
                joystick_index: -9,
            }; PROFILE_CONTROL_COUNT],
        )
    }

    #[test]
    fn defaults_flags_preserve_other_groups_and_unused_row() {
        for flags in [0, 1, 2, 3, 4, 8, 0x80, 0x81, 0x82, 0xffff] {
            let profile = sentinel_profile();
            let alias = profile.clone();
            let sentinel = profile.read(0x20);
            profile.default_controls(flags);
            for index in 0..PROFILE_CONTROL_COUNT {
                let default = if flags & DEFAULTS_PLAYER != 0 {
                    PLAYER_DEFAULTS.iter().find(|entry| entry.0 == index)
                } else {
                    None
                }
                .or_else(|| {
                    if flags & DEFAULTS_CAR != 0 {
                        CAR_DEFAULTS.iter().find(|entry| entry.0 == index)
                    } else {
                        None
                    }
                });
                let expected = default.map_or(
                    if flags & DEFAULTS_CLEAR != 0 {
                        EMPTY_BINDING
                    } else {
                        sentinel
                    },
                    |&(_, device, code, half)| ControlBinding {
                        half,
                        device,
                        code,
                        joystick_index: 0,
                    },
                );
                assert_eq!(
                    alias.read(index),
                    expected,
                    "flags {flags:#x}, row {index:#x}"
                );
            }
        }
    }

    #[test]
    fn set_binding_preserves_widths_and_rejects_outside_storage() {
        let profile = sentinel_profile();
        let unchanged = profile.read(0);
        for index in [-1, 60, i32::MAX] {
            assert!(profile.set_binding(index, 1, 2, 3, 4).is_err());
        }
        profile
            .set_binding(59, 0xffff, 0x8000, i32::MIN, i32::MAX)
            .unwrap();
        assert_eq!(
            profile.read(59),
            ControlBinding {
                device: 0xffff,
                code: 0x8000,
                joystick_index: i32::MIN,
                half: i32::MAX,
            }
        );
        assert_eq!(profile.read(0), unchanged);
    }

    #[test]
    fn startup_defaults_do_not_replace_existing_profile() {
        use crate::game_loadsave_lifecycle::ProfileFileIntake;
        assert!(
            profile_bindings_after_load_save_init(ProfileFileIntake::ProfileFilePresent).is_err()
        );
        let profile =
            profile_bindings_after_load_save_init(ProfileFileIntake::NoProfileFile).unwrap();
        assert_eq!((profile.read(1).device, profile.read(1).code), (1, 0x11));
        assert_eq!(
            profile.read(0x26),
            ControlBinding {
                device: 3,
                code: 0,
                half: 2,
                joystick_index: 0
            }
        );
        assert_eq!(profile.read(0x20), EMPTY_BINDING);
        assert_eq!(profile.read(0x34).code, 0x218);
        assert_eq!(profile.read(0x36).code, 0x212);
    }

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
