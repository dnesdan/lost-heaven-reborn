//! Original `C_Vehicle::GearSnd` selection and its MotorRot call boundary.
//! Sound objects remain caller-supplied opaque handles. Sound playback methods
//! are explicit external effects; this module does not create an audio engine.
#![forbid(unsafe_code)]

use crate::vehicle_drivetrain::{VehicleDrivetrainState, VehicleMotorRotUnavailable};

#[path = "vehicle_sound_storage.rs"]
mod storage;
#[cfg(test)]
use storage::{store, word};
use storage::{GearSoundBank, GearSounds};

/// Supplied shared PC state, retained between vehicles and calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleGearSoundGlobals {
    pub random_seed: u32,
    /// Uninterpreted binary32 argument to the original SetVolume call.
    pub volume_bits: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleGearSoundCall {
    IsPlaying { sound: u32, query_flag: bool },
    SetOn { sound: u32, enabled: bool },
    SetVolume { sound: u32, volume_bits: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleGearSoundUnavailable {
    Storage(u32),
    /// The caller has not supplied this sound object's real behavior.
    Effect(VehicleGearSoundCall),
    /// A callback removed the sound before an unconditional native dereference.
    NullSound(VehicleGearSoundCall),
}

pub struct VehicleGearSoundContext<'a, T = Vec<u8>> {
    pub vehicle: &'a mut VehicleDrivetrainState<T>,
    pub direction: &'a mut u8,
    pub globals: &'a mut VehicleGearSoundGlobals,
}

/// Implementations supply sound behavior and retain any writes made before an
/// unavailable effect. Default methods report the exact unresolved call.
pub trait VehicleGearSoundEffects<T = Vec<u8>> {
    fn is_playing(
        &mut self,
        sound: u32,
        query_flag: bool,
        _context: VehicleGearSoundContext<'_, T>,
    ) -> Result<u8, VehicleGearSoundUnavailable> {
        Err(VehicleGearSoundUnavailable::Effect(
            VehicleGearSoundCall::IsPlaying { sound, query_flag },
        ))
    }

    fn set_on(
        &mut self,
        sound: u32,
        enabled: bool,
        _context: VehicleGearSoundContext<'_, T>,
    ) -> Result<(), VehicleGearSoundUnavailable> {
        Err(VehicleGearSoundUnavailable::Effect(
            VehicleGearSoundCall::SetOn { sound, enabled },
        ))
    }

    fn set_volume(
        &mut self,
        sound: u32,
        volume_bits: u32,
        _context: VehicleGearSoundContext<'_, T>,
    ) -> Result<(), VehicleGearSoundUnavailable> {
        Err(VehicleGearSoundUnavailable::Effect(
            VehicleGearSoundCall::SetVolume { sound, volume_bits },
        ))
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> VehicleDrivetrainState<T> {
    /// Original GearSnd: query both current sounds, choose a random variant only
    /// when neither plays, enable the selected sound, then reread its handle and
    /// shared volume. The sound-enable byte is checked by MotorRot, not here.
    pub fn gear_snd(
        &mut self,
        direction: &mut u8,
        globals: &mut VehicleGearSoundGlobals,
        effects: &mut impl VehicleGearSoundEffects<T>,
    ) -> Result<(), VehicleGearSoundUnavailable> {
        if !GearSounds::borrow(self).enabled()? {
            return Ok(());
        }
        let first = GearSounds::borrow(self).current_sound(GearSoundBank::First)?;
        let first_busy = if first != 0 {
            effects.is_playing(
                first,
                false,
                VehicleGearSoundContext {
                    vehicle: self,
                    direction,
                    globals,
                },
            )? != 0
        } else {
            false
        };
        // This second query still runs when the first sound is playing.
        let second = GearSounds::borrow(self).current_sound(GearSoundBank::Second)?;
        if second != 0
            && effects.is_playing(
                second,
                false,
                VehicleGearSoundContext {
                    vehicle: self,
                    direction,
                    globals,
                },
            )? != 0
        {
            return Ok(());
        }
        if first_busy {
            return Ok(());
        }
        // Exact PC CRT rand dependency, sharing its caller-supplied seed.
        globals.random_seed = globals
            .random_seed
            .wrapping_mul(214013)
            .wrapping_add(2531011);
        let remainder = ((globals.random_seed >> 16) & 0x7fff) % 100;
        GearSounds::borrow(self).select_random_variant(remainder)?;
        let bank = GearSounds::borrow(self).selected_bank()?;
        let selected = GearSounds::borrow(self).current_sound(bank)?;
        if selected == 0 {
            return Ok(());
        }
        effects.set_on(
            selected,
            true,
            VehicleGearSoundContext {
                vehicle: self,
                direction,
                globals,
            },
        )?;
        let selected = GearSounds::borrow(self).current_sound(bank)?;
        let call = VehicleGearSoundCall::SetVolume {
            sound: selected,
            volume_bits: globals.volume_bits,
        };
        if selected == 0 {
            return Err(VehicleGearSoundUnavailable::NullSound(call));
        }
        effects.set_volume(
            selected,
            globals.volume_bits,
            VehicleGearSoundContext {
                vehicle: self,
                direction,
                globals,
            },
        )
    }

    /// Completes MotorRot's sound dependency through the actual GearSnd body.
    /// Successful external sound effects precede the pending-gear reread/copy.
    pub fn motor_rot_with_gear_sound(
        &mut self,
        dt: f32,
        force: f32,
        direction: &mut u8,
        globals: &mut VehicleGearSoundGlobals,
        effects: &mut impl VehicleGearSoundEffects<T>,
    ) -> Result<(), VehicleMotorRotUnavailable<VehicleGearSoundUnavailable>> {
        self.motor_rot_with_shift_effect(dt, force, direction, |vehicle, direction| {
            vehicle.gear_snd(direction, globals, effects)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(state: &mut VehicleDrivetrainState, offset: u32, value: u32) {
        store(state, offset, value).unwrap();
    }

    fn state() -> VehicleDrivetrainState {
        let mut state = VehicleDrivetrainState::from_bytes(vec![0; 0x2000]);
        state.as_bytes_mut()[0xc68] = 0x20;
        state.as_bytes_mut()[0x491] = 1;
        for (offset, handle) in [(0x6a8, 11), (0x6ac, 22), (0x98c, 33), (0x990, 44)] {
            put(&mut state, offset, handle);
        }
        for (offset, value) in [
            (0x44, 1.0f32),
            (0x41c, 2.0),
            (0x548, 300.0),
            (0x554, 10.0),
            (0x574, 2.0),
            (0x59c, 1.0),
            (0x5ac, 2.0),
            (0x5d0, 4.0),
            (0x5d8, 2.0),
            (0x5dc, 0.5),
            (0x1f9c, 600.0),
        ] {
            put(&mut state, offset, value.to_bits());
        }
        put(&mut state, 0xb8, 1);
        put(&mut state, 0x55c, 1);
        put(&mut state, 0x560, 2);
        put(&mut state, 0x5e8, 2);
        state.as_bytes_mut()[0x432] = 1;
        state
    }

    fn globals() -> VehicleGearSoundGlobals {
        VehicleGearSoundGlobals {
            random_seed: 1,
            volume_bits: 0x3f000000,
        }
    }

    struct Missing;
    impl VehicleGearSoundEffects for Missing {}

    #[test]
    fn disabled_sound_gate_needs_no_handles_or_effects() {
        let mut vehicle = VehicleDrivetrainState::from_bytes(vec![0; 0xc69]);
        let before = vehicle.clone();
        let mut globals = globals();
        let previous_globals = globals;
        let mut direction = 9;
        assert_eq!(
            vehicle.gear_snd(&mut direction, &mut globals, &mut Missing),
            Ok(())
        );
        assert_eq!(vehicle, before);
        assert_eq!(globals, previous_globals);
        assert_eq!(direction, 9);
    }

    #[test]
    fn busy_first_sound_still_queries_second_using_mutated_index() {
        struct Busy(Vec<u32>);
        impl VehicleGearSoundEffects for Busy {
            fn is_playing(
                &mut self,
                handle: u32,
                paused: bool,
                context: VehicleGearSoundContext<'_>,
            ) -> Result<u8, VehicleGearSoundUnavailable> {
                assert!(!paused);
                self.0.push(handle);
                put(context.vehicle, 0x200, 1);
                *context.direction = 7;
                Ok(if self.0.len() == 1 { 0x80 } else { 0 })
            }
        }
        let mut vehicle = state();
        let mut globals = globals();
        let mut effects = Busy(vec![]);
        let mut direction = 9;
        vehicle
            .gear_snd(&mut direction, &mut globals, &mut effects)
            .unwrap();
        assert_eq!(effects.0, [11, 44]);
        assert_eq!(globals.random_seed, 1);
        assert_eq!(direction, 7);
        assert_eq!(word(&vehicle, 0x200), Ok(1));
    }

    #[test]
    fn missing_query_is_explicit_and_does_not_advance_random_state() {
        let mut vehicle = state();
        let mut globals = globals();
        let before = vehicle.clone();
        assert_eq!(
            vehicle.gear_snd(&mut 0, &mut globals, &mut Missing),
            Err(VehicleGearSoundUnavailable::Effect(
                VehicleGearSoundCall::IsPlaying {
                    sound: 11,
                    query_flag: false,
                }
            ))
        );
        assert_eq!(globals.random_seed, 1);
        assert_eq!(vehicle, before);
    }

    #[test]
    fn random_threshold_selects_both_banks_without_placeholder_playback() {
        for remainder in [79, 80] {
            let seed = (0u32..)
                .find(|seed| {
                    let next = seed.wrapping_mul(214013).wrapping_add(2531011);
                    ((next >> 16) & 0x7fff) % 100 == remainder
                })
                .unwrap();
            for (bank, base) in [(true, 11), (false, 33)] {
                struct Silent;
                impl VehicleGearSoundEffects for Silent {
                    fn is_playing(
                        &mut self,
                        _: u32,
                        _: bool,
                        _: VehicleGearSoundContext<'_>,
                    ) -> Result<u8, VehicleGearSoundUnavailable> {
                        Ok(0)
                    }
                }
                let mut vehicle = state();
                vehicle.as_bytes_mut()[0x491] = u8::from(bank);
                let mut globals = VehicleGearSoundGlobals {
                    random_seed: seed,
                    volume_bits: 0,
                };
                let index = u32::from(remainder > 79);
                assert_eq!(
                    vehicle.gear_snd(&mut 0, &mut globals, &mut Silent),
                    Err(VehicleGearSoundUnavailable::Effect(
                        VehicleGearSoundCall::SetOn {
                            sound: base + index * 11,
                            enabled: true,
                        }
                    ))
                );
                assert_eq!(word(&vehicle, 0x200), Ok(index));
                assert_eq!(
                    globals.random_seed,
                    seed.wrapping_mul(214013).wrapping_add(2531011)
                );
            }
        }
    }

    #[test]
    fn composed_sound_callbacks_precede_pending_gear_copy_and_reread_sound() {
        struct Mutating(Vec<VehicleGearSoundCall>);
        impl VehicleGearSoundEffects for Mutating {
            fn is_playing(
                &mut self,
                sound: u32,
                query_flag: bool,
                context: VehicleGearSoundContext<'_>,
            ) -> Result<u8, VehicleGearSoundUnavailable> {
                assert_eq!(word(context.vehicle, 0x55c), Ok(1));
                assert_eq!(word(context.vehicle, 0x5e8), Ok(1));
                assert_eq!(word(context.vehicle, 0x5dc), Ok(0));
                self.0
                    .push(VehicleGearSoundCall::IsPlaying { sound, query_flag });
                Ok(0)
            }
            fn set_on(
                &mut self,
                sound: u32,
                enabled: bool,
                context: VehicleGearSoundContext<'_>,
            ) -> Result<(), VehicleGearSoundUnavailable> {
                self.0.push(VehicleGearSoundCall::SetOn { sound, enabled });
                put(context.vehicle, 0x200, 1);
                put(context.vehicle, 0x560, 5);
                context.vehicle.as_bytes_mut()[0x491] = 0;
                context.globals.volume_bits = 0x7fc12345;
                *context.direction = 0x91;
                Ok(())
            }
            fn set_volume(
                &mut self,
                sound: u32,
                volume_bits: u32,
                context: VehicleGearSoundContext<'_>,
            ) -> Result<(), VehicleGearSoundUnavailable> {
                self.0
                    .push(VehicleGearSoundCall::SetVolume { sound, volume_bits });
                assert_eq!(*context.direction, 0x91);
                assert_eq!(word(context.vehicle, 0x55c), Ok(1));
                put(context.vehicle, 0x560, 6);
                Ok(())
            }
        }
        let mut vehicle = state();
        let mut globals = globals();
        let mut direction = 9;
        let mut effects = Mutating(vec![]);
        vehicle
            .motor_rot_with_gear_sound(0.25, 0.0, &mut direction, &mut globals, &mut effects)
            .unwrap();
        assert_eq!(
            effects.0,
            [
                VehicleGearSoundCall::IsPlaying {
                    sound: 11,
                    query_flag: false
                },
                VehicleGearSoundCall::IsPlaying {
                    sound: 33,
                    query_flag: false
                },
                VehicleGearSoundCall::SetOn {
                    sound: 11,
                    enabled: true
                },
                VehicleGearSoundCall::SetVolume {
                    sound: 22,
                    volume_bits: 0x7fc12345
                },
            ]
        );
        assert_eq!(word(&vehicle, 0x55c), Ok(6));
        assert_eq!(direction, 0x91);
    }

    #[test]
    fn unavailable_sound_retains_motor_prefix_and_defers_pending_gear() {
        let mut vehicle = state();
        let mut globals = globals();
        let mut direction = 9;
        assert_eq!(
            vehicle.motor_rot_with_gear_sound(
                0.25,
                0.0,
                &mut direction,
                &mut globals,
                &mut Missing
            ),
            Err(VehicleMotorRotUnavailable::Effect(
                VehicleGearSoundUnavailable::Effect(VehicleGearSoundCall::IsPlaying {
                    sound: 11,
                    query_flag: false
                })
            ))
        );
        assert_eq!(word(&vehicle, 0x55c), Ok(1));
        assert_eq!(word(&vehicle, 0x5dc), Ok(0));
        assert_eq!(word(&vehicle, 0x5e8), Ok(1));
        assert_eq!(direction, 1);
        assert_eq!(globals.random_seed, 1);
    }
}

#[cfg(test)]
#[path = "vehicle_sound_regression_tests.rs"]
mod regression_tests;
