//! Small `C_Vehicle` / `C_car` getters, setters and state bits.
//!
//! Pure rewrites of the tiny inline accessors at the end of
//! `C_Vehicle.h` / `C_car.h`. Each function documents its original GAME RVA
//! (VA = RVA + 0x400000) and the mafia-re source line at pin 113b5b6.
//! Light setters return the wake-up edge explicitly: the original calls the
//! `wake_up` callback through `callback_context` (`C_Vehicle.h:308`), which
//! the dynamics/collision owner performs. No callback is invoked here.

/// `VLIGHT_INDICATOR_AUTO` bit (mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:105`).
pub const VLIGHT_INDICATOR_AUTO: u32 = 0x8;
/// `VLIGHT_LIGHT` bit (mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:107`).
pub const VLIGHT_LIGHT: u32 = 0x80;
/// `VLIGHT_INDICATOR_L` bit (mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:102`).
pub const VLIGHT_INDICATOR_L: u32 = 0x1;
/// `VLIGHT_INDICATOR_R` bit (mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:103`).
pub const VLIGHT_INDICATOR_R: u32 = 0x2;
/// `VLIGHT_MAIN` bit (mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:113`).
pub const VLIGHT_MAIN: u32 = 0x10000;
/// `VLIGHT_PROJECTORS` mask (mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:117`).
pub const VLIGHT_PROJECTORS: u32 = 0xff00000;

/// RVA 0x7b1f0, VA 0x47b1f0 (`C_Vehicle::GetNumSeats`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:179-183`).
/// Returns the seat count as the original `int` return.
pub fn vehicle_get_num_seats(seat_count: usize) -> i32 {
    seat_count as i32
}

/// RVA 0x7b220, VA 0x47b220 (`C_Vehicle::SetIndicatorAuto`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:286-293`).
/// Sets or clears `VLIGHT_INDICATOR_AUTO`. Never wakes the vehicle.
pub fn vehicle_set_indicator_auto(lights: u32, on: bool) -> u32 {
    if on {
        lights | VLIGHT_INDICATOR_AUTO
    } else {
        lights & !VLIGHT_INDICATOR_AUTO
    }
}

/// RVA 0x7b250, VA 0x47b250 (`C_Vehicle::SetLight`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:333-342`).
/// Sets or clears `VLIGHT_LIGHT`. The boolean is the deferred wake-up edge.
pub fn vehicle_set_light(lights: u32, on: bool) -> (u32, bool) {
    let next = if on {
        lights | VLIGHT_LIGHT
    } else {
        lights & !VLIGHT_LIGHT
    };
    (next, true)
}

/// RVA 0x7b280, VA 0x47b280 (`C_Vehicle::SetIndicatorL`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:345-357`).
/// No-op while `VLIGHT_INDICATOR_AUTO` is set. Wakes only when switching on.
pub fn vehicle_set_indicator_l(lights: u32, on: bool) -> (u32, bool) {
    if lights & VLIGHT_INDICATOR_AUTO != 0 {
        return (lights, false);
    }
    if on {
        (lights | VLIGHT_INDICATOR_L, true)
    } else {
        (lights & !VLIGHT_INDICATOR_L, false)
    }
}

/// RVA 0x7b2c0, VA 0x47b2c0 (`C_Vehicle::SetIndicatorR`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:359-371`).
/// Mirrors `SetIndicatorL` for the right indicator.
pub fn vehicle_set_indicator_r(lights: u32, on: bool) -> (u32, bool) {
    if lights & VLIGHT_INDICATOR_AUTO != 0 {
        return (lights, false);
    }
    if on {
        (lights | VLIGHT_INDICATOR_R, true)
    } else {
        (lights & !VLIGHT_INDICATOR_R, false)
    }
}

/// RVA 0x7b300, VA 0x47b300 (`C_Vehicle::SetMainLights`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:372-381`).
/// Sets or clears `VLIGHT_MAIN`. The boolean is the deferred wake-up edge.
pub fn vehicle_set_main_lights(lights: u32, on: bool) -> (u32, bool) {
    let next = if on {
        lights | VLIGHT_MAIN
    } else {
        lights & !VLIGHT_MAIN
    };
    (next, true)
}

/// RVA 0x7b340, VA 0x47b340 (`C_Vehicle::SetProjectors`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:384-391`).
/// Replaces the `VLIGHT_PROJECTORS` field with the masked input, as the
/// original's two statements do. The boolean is the deferred wake-up edge.
pub fn vehicle_set_projectors(lights: u32, projectors: u32) -> (u32, bool) {
    let next = (lights & !VLIGHT_PROJECTORS) | (projectors & VLIGHT_PROJECTORS);
    (next, true)
}

/// RVA 0x7b380, VA 0x47b380 (`C_Vehicle::GetProjectors`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:392-396`).
/// Returns the masked projector field.
pub fn vehicle_get_projectors(lights: u32) -> u32 {
    lights & VLIGHT_PROJECTORS
}

/// RVA 0x7b390, VA 0x47b390 (`C_Vehicle::GetNumWheels`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:397-401`).
/// Returns the stored wheel count.
pub fn vehicle_get_num_wheels(num_wheels: i32) -> i32 {
    num_wheels
}

/// Tank-hit store written by `C_car::SetTankHitCount`
/// (mafia-re 113b5b6 `game/vehicles/C_car.h:287-289`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarTankHits {
    /// `tank_hits_left` (mafia-re 113b5b6 `game/vehicles/C_car.h:289`).
    pub left: i32,
    /// `tank_hit_count` (mafia-re 113b5b6 `game/vehicles/C_car.h:290`).
    pub count: i32,
}

/// RVA 0x7b3a0, VA 0x47b3a0 (`C_car::SetTankHitCount`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:177-181`).
/// Stores `count` into both tank-hit words.
pub fn car_set_tank_hit_count(hits: &mut CarTankHits, count: i32) {
    hits.left = count;
    hits.count = count;
}

/// RVA 0x7b3c0, VA 0x47b3c0 (`C_car::SetMustSteal`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:182-186`).
/// Stores the must-steal flag.
pub fn car_set_must_steal(slot: &mut bool, on: bool) {
    *slot = on;
}

/// RVA 0x7b3d0, VA 0x47b3d0 (`C_car::SetDestroyMotor`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:187-191`).
/// Stores the destroy-motor flag.
pub fn car_set_destroy_motor(slot: &mut bool, destroy: bool) {
    *slot = destroy;
}

/// RVA 0x7b400, VA 0x47b400 (`C_car::SetShowDamage`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:202-206`).
/// Stores the show-damage flag.
pub fn car_set_show_damage(slot: &mut bool, show: bool) {
    *slot = show;
}

/// RVA 0x7b490, VA 0x47b490 (`C_car::SetUnbreakable`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:238-242`).
/// Stores the raw `CARF_UNBREAKABLE` flags word.
pub fn car_set_unbreakable(slot: &mut u32, flags: u32) {
    *slot = flags;
}

/// RVA 0x7b4a0, VA 0x47b4a0 (`SetRecordPlaying`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:340-344`).
/// Stores the global cutscene-record flag.
pub fn car_set_record_playing(slot: &mut bool, on: bool) {
    *slot = on;
}

/// RVA 0x7b0e0, VA 0x47b0e0 (`C_Vehicle::GetSpecialDynamicColls`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:260-264`).
/// Reads the `special_dynamic_colls` flag at 0x1faa.
pub fn vehicle_get_special_dynamic_colls(flag: bool) -> bool {
    flag
}

/// RVA 0x7b100, VA 0x47b100 (`C_Vehicle::SetVelocity`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:270-277`).
/// Stores the velocity vector and refreshes `speed` as its magnitude.
/// Returns the new speed and the deferred wake-up edge (always requested;
/// the owner invokes `wake_up` through `callback_context` when present).
pub fn vehicle_set_velocity(velocity: [f32; 3]) -> ([f32; 3], f32, bool) {
    let speed = (velocity[0] * velocity[0]
        + velocity[1] * velocity[1]
        + velocity[2] * velocity[2])
        .sqrt();
    (velocity, speed, true)
}

/// RVA 0x7b1b0, VA 0x47b1b0 (`C_Vehicle::SetAngularVelocity`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:278-284`).
/// Stores the angular velocity. The boolean is the deferred wake-up edge
/// (always requested; the owner invokes `wake_up` when present).
pub fn vehicle_set_angular_velocity(angular_velocity: [f32; 3]) -> ([f32; 3], bool) {
    (angular_velocity, true)
}

/// RVA 0x7b3f0, VA 0x47b3f0 (`C_car::GetModelName`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:197-201`).
/// Returns the model-name field up to the first NUL byte, mirroring the
/// original `char *` return without exposing the trailing fill.
pub fn car_get_model_name(model_name: &[u8]) -> &[u8] {
    let end = model_name.iter().position(|&b| b == 0).unwrap_or(model_name.len());
    &model_name[..end]
}

/// RVA 0x7b410, VA 0x47b410 (`C_car::Lock`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:207-214`).
/// Counts the `CAR_LOCK` locks up or down with wrapping `i32` arithmetic.
pub fn car_lock(locks: i32, lock: bool) -> i32 {
    if lock {
        locks.wrapping_add(1)
    } else {
        locks.wrapping_sub(1)
    }
}

/// RVA 0x7b440, VA 0x47b440 (`C_car::SetTemporary`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:215-219`).
/// Stores the temporary-car flag.
pub fn car_set_temporary(slot: &mut bool, temporary: bool) {
    *slot = temporary;
}

/// RVA 0x7b450, VA 0x47b450 (`C_car::SetShowEnergy`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:221-227`).
/// Stores the new value and returns the previous one.
pub fn car_set_show_energy(slot: &mut i32, show: i32) -> i32 {
    let old = *slot;
    *slot = show;
    old
}

/// RVA 0x7b470, VA 0x47b470 (`C_car::SetUnknown217c`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:228-232`).
/// Stores the flag at 0x217c.
pub fn car_set_unknown_217c(slot: &mut bool, on: bool) {
    *slot = on;
}

/// RVA 0x7b480, VA 0x47b480 (`C_car::SetAxis`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:233-237`).
/// Stores the axis word (`CAR_SETAXIS` sets 1).
pub fn car_set_axis(slot: &mut i32, value: i32) {
    *slot = value;
}

/// RVA 0x7b4b0, VA 0x47b4b0 (`C_car::SetDynamicCollsOff`,
/// mafia-re 113b5b6 `game/vehicles/C_car.h:246-250`).
/// Maps to `SetDynamicCollsOn(!off, GetUnknown1fac())`: returns the
/// `(on, unknown)` pair the owner stores. Collider attach/detach and the
/// wake-up edge stay deferred to the collision owner.
pub fn car_set_dynamic_colls_off(off: bool, keep_unknown: bool) -> (bool, bool) {
    (!off, keep_unknown)
}

/// RVA 0xa9bf0, VA 0x4a9bf0 (`C_Vehicle::GetEngineOn`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:404-408`).
/// Reads the engine-running flag.
pub fn vehicle_get_engine_on(engine_on: bool) -> bool {
    engine_on
}

/// RVA 0xa9c00, VA 0x4a9c00 (`C_Vehicle::SetAutoGearing`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:409-413`).
/// Stores the automatic-gearing flag.
pub fn vehicle_set_auto_gearing(slot: &mut bool, automatic: bool) {
    *slot = automatic;
}

/// RVA 0xa9c10, VA 0x4a9c10 (`C_Vehicle::SetHorn`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:414-418`).
/// Stores the horn flag.
pub fn vehicle_set_horn(slot: &mut bool, on: bool) {
    *slot = on;
}

/// RVA 0xa9c20, VA 0x4a9c20 (`C_Vehicle::GetSiren`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:419-423`).
/// Reads the siren flag.
pub fn vehicle_get_siren(siren: bool) -> bool {
    siren
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rva_7b1f0_get_num_seats_returns_count() {
        // RVA 0x7b1f0: seats.size() as int.
        assert_eq!(vehicle_get_num_seats(0), 0);
        assert_eq!(vehicle_get_num_seats(4), 4);
    }

    #[test]
    fn rva_7b220_set_indicator_auto_toggles_only_auto_bit() {
        // RVA 0x7b220: other bits preserved, no wake modelled.
        assert_eq!(vehicle_set_indicator_auto(0, true), VLIGHT_INDICATOR_AUTO);
        assert_eq!(
            vehicle_set_indicator_auto(VLIGHT_INDICATOR_AUTO | VLIGHT_LIGHT, false),
            VLIGHT_LIGHT
        );
    }

    #[test]
    fn rva_7b250_set_light_toggles_light_bit_and_wakes() {
        // RVA 0x7b250: always wakes.
        assert_eq!(vehicle_set_light(0, true), (VLIGHT_LIGHT, true));
        assert_eq!(vehicle_set_light(VLIGHT_LIGHT, false), (0, true));
        assert_eq!(vehicle_set_light(VLIGHT_MAIN, true), (VLIGHT_MAIN | VLIGHT_LIGHT, true));
    }

    #[test]
    fn rva_7b280_set_indicator_l_respects_auto_and_wakes_on_enable() {
        // RVA 0x7b280: AUTO blocks; wake only when switching on.
        assert_eq!(
            vehicle_set_indicator_l(VLIGHT_INDICATOR_AUTO, true),
            (VLIGHT_INDICATOR_AUTO, false)
        );
        assert_eq!(vehicle_set_indicator_l(0, true), (VLIGHT_INDICATOR_L, true));
        assert_eq!(
            vehicle_set_indicator_l(VLIGHT_INDICATOR_L, false),
            (0, false)
        );
    }

    #[test]
    fn rva_7b2c0_set_indicator_r_respects_auto_and_wakes_on_enable() {
        // RVA 0x7b2c0: mirror of the L branch.
        assert_eq!(
            vehicle_set_indicator_r(VLIGHT_INDICATOR_AUTO, true),
            (VLIGHT_INDICATOR_AUTO, false)
        );
        assert_eq!(vehicle_set_indicator_r(0, true), (VLIGHT_INDICATOR_R, true));
        assert_eq!(
            vehicle_set_indicator_r(VLIGHT_INDICATOR_R, false),
            (0, false)
        );
    }

    #[test]
    fn rva_7b300_set_main_lights_toggles_main_bit_and_wakes() {
        // RVA 0x7b300: always wakes.
        assert_eq!(vehicle_set_main_lights(0, true), (VLIGHT_MAIN, true));
        assert_eq!(vehicle_set_main_lights(VLIGHT_MAIN, false), (0, true));
    }

    #[test]
    fn rva_7b340_set_projectors_masks_field_and_wakes() {
        // RVA 0x7b340: field replace with masked input, always wakes.
        assert_eq!(
            vehicle_set_projectors(0, 0xffff_ffff),
            (VLIGHT_PROJECTORS, true)
        );
        assert_eq!(
            vehicle_set_projectors(VLIGHT_PROJECTORS | VLIGHT_LIGHT, 0),
            (VLIGHT_LIGHT, true)
        );
    }

    #[test]
    fn rva_7b380_get_projectors_returns_masked_field() {
        // RVA 0x7b380: lights & VLIGHT_PROJECTORS.
        assert_eq!(vehicle_get_projectors(0), 0);
        assert_eq!(
            vehicle_get_projectors(VLIGHT_PROJECTORS | VLIGHT_LIGHT),
            VLIGHT_PROJECTORS
        );
    }

    #[test]
    fn rva_7b390_get_num_wheels_returns_store() {
        // RVA 0x7b390: plain member read.
        assert_eq!(vehicle_get_num_wheels(4), 4);
        assert_eq!(vehicle_get_num_wheels(0), 0);
    }

    #[test]
    fn rva_7b3a0_set_tank_hit_count_writes_both_words() {
        // RVA 0x7b3a0: tank_hits_left = tank_hit_count = count.
        let mut hits = CarTankHits::default();
        car_set_tank_hit_count(&mut hits, 3);
        assert_eq!(hits, CarTankHits { left: 3, count: 3 });
        car_set_tank_hit_count(&mut hits, -1);
        assert_eq!(hits, CarTankHits { left: -1, count: -1 });
    }

    #[test]
    fn rva_7b3c0_set_must_steal_stores_flag() {
        // RVA 0x7b3c0: plain bool store.
        let mut flag = false;
        car_set_must_steal(&mut flag, true);
        assert!(flag);
        car_set_must_steal(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_7b3d0_set_destroy_motor_stores_flag() {
        // RVA 0x7b3d0: plain bool store.
        let mut flag = false;
        car_set_destroy_motor(&mut flag, true);
        assert!(flag);
        car_set_destroy_motor(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_7b400_set_show_damage_stores_flag() {
        // RVA 0x7b400: plain bool store.
        let mut flag = true;
        car_set_show_damage(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_7b490_set_unbreakable_stores_raw_flags() {
        // RVA 0x7b490: raw uint store, CARF_UNBREAKABLE = 0x4000.
        let mut flags = 0u32;
        car_set_unbreakable(&mut flags, 0x4000);
        assert_eq!(flags, 0x4000);
        car_set_unbreakable(&mut flags, 0);
        assert_eq!(flags, 0);
    }

    #[test]
    fn rva_7b4a0_set_record_playing_stores_global() {
        // RVA 0x7b4a0: global record_playing store.
        let mut flag = false;
        car_set_record_playing(&mut flag, true);
        assert!(flag);
        car_set_record_playing(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_7b0e0_get_special_dynamic_colls_reads_flag() {
        // RVA 0x7b0e0: plain member read at 0x1faa.
        assert!(vehicle_get_special_dynamic_colls(true));
        assert!(!vehicle_get_special_dynamic_colls(false));
    }

    #[test]
    fn rva_7b100_set_velocity_stores_vector_speed_and_wakes() {
        // RVA 0x7b100: velocity store + speed = magnitude + wake edge.
        let (velocity, speed, wake) = vehicle_set_velocity([3.0, 4.0, 0.0]);
        assert_eq!(velocity, [3.0, 4.0, 0.0]);
        assert_eq!(speed, 5.0);
        assert!(wake);
        let (_, speed, _) = vehicle_set_velocity([0.0, 0.0, 0.0]);
        assert_eq!(speed, 0.0);
    }

    #[test]
    fn rva_7b1b0_set_angular_velocity_stores_and_wakes() {
        // RVA 0x7b1b0: angular velocity store + wake edge.
        let (stored, wake) = vehicle_set_angular_velocity([1.0, -2.0, 0.5]);
        assert_eq!(stored, [1.0, -2.0, 0.5]);
        assert!(wake);
    }

    #[test]
    fn rva_7b3f0_get_model_name_trims_at_nul() {
        // RVA 0x7b3f0: char * return without trailing fill.
        let mut field = [0x41u8; 32];
        field[4] = 0;
        assert_eq!(car_get_model_name(&field), b"AAAA".as_slice());
        let full = [0x42u8; 8];
        assert_eq!(car_get_model_name(&full), full.as_slice());
        assert!(car_get_model_name(&[]).is_empty());
    }

    #[test]
    fn rva_7b410_lock_counts_up_and_down() {
        // RVA 0x7b410: locks++/locks-- with no saturation.
        assert_eq!(car_lock(0, true), 1);
        assert_eq!(car_lock(1, false), 0);
        assert_eq!(car_lock(i32::MAX, true), i32::MIN);
    }

    #[test]
    fn rva_7b440_set_temporary_stores_flag() {
        // RVA 0x7b440: plain bool store.
        let mut flag = false;
        car_set_temporary(&mut flag, true);
        assert!(flag);
        car_set_temporary(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_7b450_set_show_energy_returns_old() {
        // RVA 0x7b450: old value returned, new value stored.
        let mut slot = 0;
        assert_eq!(car_set_show_energy(&mut slot, 1), 0);
        assert_eq!(slot, 1);
        assert_eq!(car_set_show_energy(&mut slot, 5), 1);
        assert_eq!(slot, 5);
    }

    #[test]
    fn rva_7b470_set_unknown_217c_stores_flag() {
        // RVA 0x7b470: plain bool store.
        let mut flag = false;
        car_set_unknown_217c(&mut flag, true);
        assert!(flag);
        car_set_unknown_217c(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_7b480_set_axis_stores_word() {
        // RVA 0x7b480: plain int store (CAR_SETAXIS sets 1).
        let mut axis = 0;
        car_set_axis(&mut axis, 1);
        assert_eq!(axis, 1);
        car_set_axis(&mut axis, -1);
        assert_eq!(axis, -1);
    }

    #[test]
    fn rva_7b4b0_set_dynamic_colls_off_negates_and_keeps_second() {
        // RVA 0x7b4b0: SetDynamicCollsOn(!off, GetUnknown1fac()).
        assert_eq!(car_set_dynamic_colls_off(true, true), (false, true));
        assert_eq!(car_set_dynamic_colls_off(false, false), (true, false));
        assert_eq!(car_set_dynamic_colls_off(true, false), (false, false));
    }

    #[test]
    fn rva_a9bf0_get_engine_on_reads_flag() {
        // RVA 0xa9bf0: plain member read.
        assert!(vehicle_get_engine_on(true));
        assert!(!vehicle_get_engine_on(false));
    }

    #[test]
    fn rva_a9c00_set_auto_gearing_stores_flag() {
        // RVA 0xa9c00: plain bool store.
        let mut flag = false;
        vehicle_set_auto_gearing(&mut flag, true);
        assert!(flag);
        vehicle_set_auto_gearing(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_a9c10_set_horn_stores_flag() {
        // RVA 0xa9c10: plain bool store.
        let mut flag = false;
        vehicle_set_horn(&mut flag, true);
        assert!(flag);
        vehicle_set_horn(&mut flag, false);
        assert!(!flag);
    }

    #[test]
    fn rva_a9c20_get_siren_reads_flag() {
        // RVA 0xa9c20: plain member read.
        assert!(vehicle_get_siren(true));
        assert!(!vehicle_get_siren(false));
    }
}
