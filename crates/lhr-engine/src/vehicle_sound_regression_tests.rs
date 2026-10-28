//! Synthetic boundary and callback regressions; no original game data.
use super::*;

fn state() -> VehicleDrivetrainState {
    let mut vehicle = VehicleDrivetrainState::from_bytes(vec![0; 0xc69]);
    vehicle.as_bytes_mut()[0xc68] = 0x20;
    vehicle.as_bytes_mut()[0x491] = 1;
    for (offset, sound) in [(0x6a8, 11), (0x6ac, 22), (0x98c, 33), (0x990, 44)] {
        store(&mut vehicle, offset, sound).unwrap();
    }
    vehicle
}

fn globals() -> VehicleGearSoundGlobals {
    VehicleGearSoundGlobals {
        random_seed: 1,
        volume_bits: 0.5f32.to_bits(),
    }
}

#[test]
fn storage_view_is_lazy_and_reports_the_reached_field() {
    let mut vehicle = VehicleDrivetrainState::from_bytes(vec![]);
    let mut view = GearSounds::borrow(&mut vehicle);
    assert_eq!(
        view.enabled(),
        Err(VehicleGearSoundUnavailable::Storage(0xc68))
    );
    assert_eq!(
        view.current_sound(GearSoundBank::First),
        Err(VehicleGearSoundUnavailable::Storage(0x200))
    );
    assert_eq!(
        view.selected_bank(),
        Err(VehicleGearSoundUnavailable::Storage(0x491))
    );
    assert_eq!(
        view.select_random_variant(80),
        Err(VehicleGearSoundUnavailable::Storage(0x200))
    );

    let mut vehicle = VehicleDrivetrainState::from_bytes(vec![0; 0x204]);
    assert_eq!(
        GearSounds::borrow(&mut vehicle).current_sound(GearSoundBank::Second),
        Err(VehicleGearSoundUnavailable::Storage(0x98c))
    );
}

#[test]
fn storage_bank_selector_preserves_every_nonzero_byte() {
    let mut vehicle = state();
    for selector in 0u8..=255 {
        vehicle.as_bytes_mut()[0x491] = selector;
        assert_eq!(
            GearSounds::borrow(&mut vehicle).selected_bank().unwrap(),
            if selector == 0 {
                GearSoundBank::Second
            } else {
                GearSoundBank::First
            }
        );
    }
}

#[test]
fn storage_index_keeps_wrapping_address_arithmetic_without_clamping() {
    // Last case wraps the first bank exactly to byte zero. Expected offsets
    // are fixed fixture values, not computed by the implementation under test.
    for (index, first, second) in [
        (0, 0x6a8, 0x98c),
        (1, 0x6ac, 0x990),
        (u32::MAX, 0x6a4, 0x988),
        (0x3fff_fe56, 0, 0x2e4),
    ] {
        let mut vehicle = state();
        store(&mut vehicle, 0x200, index).unwrap();
        store(&mut vehicle, first, 17).unwrap();
        store(&mut vehicle, second, 29).unwrap();
        let view = GearSounds::borrow(&mut vehicle);
        assert_eq!(view.current_sound(GearSoundBank::First), Ok(17));
        assert_eq!(view.current_sound(GearSoundBank::Second), Ok(29));
    }
}

#[test]
fn storage_view_writes_the_borrowed_owner_and_keeps_the_random_threshold() {
    let mut bytes = [0u8; 0x204];
    for remainder in [0, 79, 80, 99] {
        {
            let mut vehicle = VehicleDrivetrainState::from_bytes_mut(&mut bytes);
            GearSounds::borrow(&mut vehicle)
                .select_random_variant(remainder)
                .unwrap();
        }
        assert_eq!(
            u32::from_le_bytes(bytes[0x200..0x204].try_into().unwrap()),
            u32::from(remainder > 79)
        );
    }
}

#[derive(Clone, Copy)]
enum AfterEnable {
    Clear,
    Fail,
    ChangeBankAndIndex,
}

struct Mutating {
    calls: Vec<VehicleGearSoundCall>,
    action: AfterEnable,
}
impl VehicleGearSoundEffects for Mutating {
    fn is_playing(
        &mut self,
        sound: u32,
        query_flag: bool,
        _: VehicleGearSoundContext<'_>,
    ) -> Result<u8, VehicleGearSoundUnavailable> {
        self.calls
            .push(VehicleGearSoundCall::IsPlaying { sound, query_flag });
        Ok(0)
    }

    fn set_on(
        &mut self,
        sound: u32,
        enabled: bool,
        context: VehicleGearSoundContext<'_>,
    ) -> Result<(), VehicleGearSoundUnavailable> {
        let call = VehicleGearSoundCall::SetOn { sound, enabled };
        self.calls.push(call);
        context.globals.volume_bits = 0x7fc12345;
        *context.direction = 0x91;
        match self.action {
            AfterEnable::Clear => {
                let offset = if sound == 11 { 0x6a8 } else { 0x98c };
                store(context.vehicle, offset, 0)?;
            }
            AfterEnable::Fail => {
                store(context.vehicle, 0x200, 1)?;
                return Err(VehicleGearSoundUnavailable::Effect(call));
            }
            AfterEnable::ChangeBankAndIndex => {
                store(context.vehicle, 0x200, 1)?;
                context.vehicle.as_bytes_mut()[0x491] = u8::from(sound != 11);
            }
        }
        Ok(())
    }

    fn set_volume(
        &mut self,
        sound: u32,
        volume_bits: u32,
        _: VehicleGearSoundContext<'_>,
    ) -> Result<(), VehicleGearSoundUnavailable> {
        self.calls
            .push(VehicleGearSoundCall::SetVolume { sound, volume_bits });
        Ok(())
    }
}

#[test]
fn enable_callback_removing_sound_reports_null_before_volume_call() {
    let mut vehicle = state();
    let mut shared = globals();
    let mut direction = 0;
    let mut effects = Mutating {
        calls: vec![],
        action: AfterEnable::Clear,
    };
    assert_eq!(
        vehicle.gear_snd(&mut direction, &mut shared, &mut effects),
        Err(VehicleGearSoundUnavailable::NullSound(
            VehicleGearSoundCall::SetVolume {
                sound: 0,
                volume_bits: 0x7fc12345,
            }
        ))
    );
    assert_eq!(effects.calls.len(), 3);
    assert_eq!(direction, 0x91);
    assert_eq!(word(&vehicle, 0x6a8), Ok(0));
    assert_eq!(shared.volume_bits, 0x7fc12345);
}

#[test]
fn enable_callback_failure_keeps_its_writes_and_does_not_call_volume() {
    let mut vehicle = state();
    let mut shared = globals();
    let mut direction = 0;
    let mut effects = Mutating {
        calls: vec![],
        action: AfterEnable::Fail,
    };
    assert_eq!(
        vehicle.gear_snd(&mut direction, &mut shared, &mut effects),
        Err(VehicleGearSoundUnavailable::Effect(
            VehicleGearSoundCall::SetOn {
                sound: 11,
                enabled: true,
            }
        ))
    );
    assert_eq!(effects.calls.len(), 3);
    assert_eq!(word(&vehicle, 0x200), Ok(1));
    assert_eq!(direction, 0x91);
    assert_eq!(shared.volume_bits, 0x7fc12345);
    assert_eq!(
        shared.random_seed,
        1u32.wrapping_mul(214013).wrapping_add(2531011)
    );
}

#[test]
fn enable_callback_rereads_index_and_volume_but_keeps_the_selected_bank() {
    for (selector, enabled_sound, volume_sound) in [(1, 11, 22), (0, 33, 44)] {
        let mut vehicle = state();
        vehicle.as_bytes_mut()[0x491] = selector;
        let mut shared = globals();
        let mut effects = Mutating {
            calls: vec![],
            action: AfterEnable::ChangeBankAndIndex,
        };
        vehicle.gear_snd(&mut 0, &mut shared, &mut effects).unwrap();
        assert_eq!(
            effects.calls,
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
                    sound: enabled_sound,
                    enabled: true
                },
                VehicleGearSoundCall::SetVolume {
                    sound: volume_sound,
                    volume_bits: 0x7fc12345
                },
            ]
        );
    }
}

#[test]
fn busy_first_query_does_not_hide_a_second_query_failure() {
    struct Queries(Vec<u32>);
    impl VehicleGearSoundEffects for Queries {
        fn is_playing(
            &mut self,
            sound: u32,
            query_flag: bool,
            context: VehicleGearSoundContext<'_>,
        ) -> Result<u8, VehicleGearSoundUnavailable> {
            self.0.push(sound);
            if self.0.len() == 1 {
                store(context.vehicle, 0x200, 1)?;
                return Ok(0x80);
            }
            Err(VehicleGearSoundUnavailable::Effect(
                VehicleGearSoundCall::IsPlaying { sound, query_flag },
            ))
        }
    }
    let mut vehicle = state();
    let mut shared = globals();
    let mut effects = Queries(vec![]);
    assert_eq!(
        vehicle.gear_snd(&mut 0, &mut shared, &mut effects),
        Err(VehicleGearSoundUnavailable::Effect(
            VehicleGearSoundCall::IsPlaying {
                sound: 44,
                query_flag: false,
            }
        ))
    );
    assert_eq!(effects.0, [11, 44]);
    assert_eq!(shared.random_seed, 1);
    assert_eq!(word(&vehicle, 0x200), Ok(1));
}

#[test]
fn changed_storage_after_queries_keeps_rng_advance_before_failed_choice_write() {
    struct Queries(usize);
    impl VehicleGearSoundEffects for Queries {
        fn is_playing(
            &mut self,
            _: u32,
            _: bool,
            context: VehicleGearSoundContext<'_>,
        ) -> Result<u8, VehicleGearSoundUnavailable> {
            self.0 += 1;
            if self.0 == 2 {
                *context.vehicle = VehicleDrivetrainState::from_bytes(vec![0x5a; 0x203]);
                context.globals.random_seed = 7;
            }
            Ok(0)
        }
    }
    let mut vehicle = state();
    let mut shared = globals();
    let mut effects = Queries(0);
    assert_eq!(
        vehicle.gear_snd(&mut 0, &mut shared, &mut effects),
        Err(VehicleGearSoundUnavailable::Storage(0x200))
    );
    assert_eq!(effects.0, 2);
    assert_eq!(
        shared.random_seed,
        7u32.wrapping_mul(214013).wrapping_add(2531011)
    );
    assert_eq!(&vehicle.as_bytes()[0x200..], &[0x5a; 3]);
}
