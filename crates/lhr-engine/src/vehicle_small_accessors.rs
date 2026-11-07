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
}
