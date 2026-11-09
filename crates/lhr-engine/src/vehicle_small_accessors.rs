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

/// RVA 0xa9c30, VA 0x4a9c30 (`C_Vehicle::GetVelocityRef`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:424-428`).
/// Returns the stored velocity vector by reference.
pub fn vehicle_velocity_ref(velocity: &[f32; 3]) -> &[f32; 3] {
    velocity
}

/// RVA 0xa9c40, VA 0x4a9c40 (`C_Vehicle::IsUnknown540`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:429-435`).
/// True while either of the 0x540/0x544 words is nonzero. The comparison is
/// the IEEE `!=` against 0.0, so NaN counts as nonzero exactly like the
/// original float comparison.
pub fn vehicle_is_unknown_540(first: f32, second: f32) -> bool {
    first != 0.0 || second != 0.0
}

/// RVA 0xa9c70, VA 0x4a9c70 (`C_Vehicle::GetUnknownC48`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:436-440`).
/// Returns the stored 0xc48 vector by reference.
pub fn vehicle_unknown_c48(stored: &[f32; 3]) -> &[f32; 3] {
    stored
}

/// RVA 0xa9c80, VA 0x4a9c80 (`C_Vehicle::GetUp`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:441-445`).
/// Returns the stored up-axis vector by reference.
pub fn vehicle_up(up: &[f32; 3]) -> &[f32; 3] {
    up
}

/// RVA 0xa9c90, VA 0x4a9c90 (`C_Vehicle::GetWheel`,
/// mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:446-450`).
/// Returns the opaque identity of wheel `index`. In-range indices mirror the
/// original `wheels[i]` read; out-of-range indices yield `None` instead of
/// the native out-of-bounds read. Wheel identities never cross as pointers.
pub fn vehicle_get_wheel(wheels: &[u32], index: i32) -> Option<u32> {
    if index < 0 {
        return None;
    }
    wheels.get(index as usize).copied()
}

/// `S_wheel::flags` collision bit copied by `GetCarWheelColl` and
/// `SetCarWheelColl` (mafia-re 113b5b6 `game/vehicles/C_Vehicle.h:90`).
pub const WHEELF_COLLISION: u32 = 8;

/// RVA 0x1e9c0, VA 0x41e9c0 (`C_car::GetLevelsCount`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:340-345`).
/// Returns the tuning-level count as the original `int` return.
pub fn car_levels_count(level_count: usize) -> i32 {
    level_count as i32
}

/// RVA 0x1e9f0, VA 0x41e9f0 (`C_car::GetActualLevel`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:347-351`).
/// Reads the stored tuning level.
pub fn car_actual_level(actual_level: i32) -> i32 {
    actual_level
}

/// One seat latch projected for `C_car::GetOwner`: the occupancy flag plus
/// the opaque owner identity (`S_car_seat` at mafia-re 113b5b6
/// `game/vehicles/C_car.h:44-51`). Pointers never cross this boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarSeatSlot {
    pub occupied: bool,
    pub owner: u32,
}

/// RVA 0x1dec0, VA 0x41dec0 (`C_car::GetOwner`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:325-332`).
/// Returns the seat owner's opaque identity, or `None` for the original
/// NULL: a negative seat, a seat past the vector, or an unoccupied seat.
pub fn car_get_owner(seats: &[CarSeatSlot], seat: i32) -> Option<u32> {
    if seat < 0 {
        return None;
    }
    let slot = seats.get(seat as usize)?;
    if slot.occupied {
        Some(slot.owner)
    } else {
        None
    }
}

/// RVA 0x29770, VA 0x429770 (`C_car::SetEnterProtect1`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:430-441`).
/// Sets or clears the seat bit in `enter_protect` and raises
/// `wake_request`. The `SetActState(ACTSTATE_ACTIVE)` call stays with the
/// actor owner. The shift is `wrapping_shl`, so huge seats wrap instead of
/// the native shift overflow; negative seats cannot be expressed.
pub fn car_set_enter_protect1(
    enter_protect: &mut u32,
    wake_request: &mut bool,
    seat: u32,
    on: bool,
) {
    let bit = 1u32.wrapping_shl(seat);
    if on {
        *enter_protect |= bit;
    } else {
        *enter_protect &= !bit;
    }
    *wake_request = true;
}

/// RVA 0x297b0, VA 0x4297b0 (`C_car::GetCarWheelColl`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:443-450`).
/// Copies each wheel's `WHEELF_COLLISION` flag into `out`, over the shared
/// prefix of both slices (the original bounds by `min(num, num_wheels)`).
pub fn car_get_wheel_coll(flags: &[u32], out: &mut [bool]) {
    let count = flags.len().min(out.len());
    for (index, slot) in out.iter_mut().enumerate().take(count) {
        *slot = flags[index] & WHEELF_COLLISION != 0;
    }
}

/// RVA 0x29800, VA 0x429800 (`C_car::SetCarWheelColl`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:452-464`).
/// Writes each input flag into the matching wheel's `WHEELF_COLLISION` bit,
/// over the shared prefix of both slices.
pub fn car_set_wheel_coll(flags: &mut [u32], colls: &[bool]) {
    let count = flags.len().min(colls.len());
    for (index, flag) in flags.iter_mut().enumerate().take(count) {
        if colls[index] {
            *flag |= WHEELF_COLLISION;
        } else {
            *flag &= !WHEELF_COLLISION;
        }
    }
}

/// RVA 0x29dd0, VA 0x429dd0 (`C_car::GetActorSpeed`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:593-597`).
/// Copies the stored velocity into the caller's speed vector.
pub fn car_get_actor_speed(velocity: [f32; 3]) -> [f32; 3] {
    velocity
}

/// RVA 0x29e50, VA 0x429e50 (`C_car::GetTimeParking`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:479-484`).
/// Elapsed milliseconds since `parking_time` as the original unsigned
/// subtraction (`GetGameTime() - parking_time` with 32-bit wrapping). The
/// mission-clock read stays with the mission owner; only the arithmetic
/// leaf lives here.
pub fn car_parked_duration(game_time: u32, parking_time: u32) -> u32 {
    game_time.wrapping_sub(parking_time)
}

/// Force-feedback profile scales stored by `C_car::SetForceFeedback`
/// (mafia-re 113b5b6 `game/vehicles/C_car.cpp:24-34`, globals at GAME
/// 0x6322dc-0x6322ec).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarForceFeedbackScales {
    pub volume: f32,
    pub tire_grip: f32,
    pub collisions: f32,
    pub suspension: f32,
    pub revs: f32,
}

/// RVA 0x2bb90, VA 0x42bb90 (`C_car::SetForceFeedback`,
/// mafia-re 113b5b6 `game/vehicles/C_car.cpp:485-493`).
/// Stores the five profile scales in argument order.
pub fn car_set_force_feedback(
    slot: &mut CarForceFeedbackScales,
    volume: f32,
    tire_grip: f32,
    collisions: f32,
    suspension: f32,
    revs: f32,
) {
    slot.volume = volume;
    slot.tire_grip = tire_grip;
    slot.collisions = collisions;
    slot.suspension = suspension;
    slot.revs = revs;
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

    #[test]
    fn rva_a9c30_get_velocity_ref_returns_stored_vector() {
        // RVA 0xa9c30: const S_vector & return of the stored velocity.
        let velocity = [1.0f32, -2.0, 0.5];
        assert_eq!(vehicle_velocity_ref(&velocity), &[1.0f32, -2.0, 0.5]);
    }

    #[test]
    fn rva_a9c40_is_unknown_540_flags_any_nonzero_word() {
        // RVA 0xa9c40: true when either word is nonzero, IEEE compare.
        assert!(!vehicle_is_unknown_540(0.0, 0.0));
        assert!(!vehicle_is_unknown_540(-0.0, 0.0));
        assert!(vehicle_is_unknown_540(1.0, 0.0));
        assert!(vehicle_is_unknown_540(0.0, -1.0));
        assert!(vehicle_is_unknown_540(f32::NAN, 0.0));
    }

    #[test]
    fn rva_a9c70_get_unknown_c48_returns_stored_vector() {
        // RVA 0xa9c70: const S_vector & return of the 0xc48 vector.
        let stored = [0.25f32, 0.5, 1.0];
        assert_eq!(vehicle_unknown_c48(&stored), &[0.25f32, 0.5, 1.0]);
    }

    #[test]
    fn rva_a9c80_get_up_returns_stored_vector() {
        // RVA 0xa9c80: const S_vector & return of the up axis.
        let up = [0.0f32, 0.0, 1.0];
        assert_eq!(vehicle_up(&up), &[0.0f32, 0.0, 1.0]);
    }

    #[test]
    fn rva_a9c90_get_wheel_returns_identity_or_none() {
        // RVA 0xa9c90: wheels[i]; out-of-range yields None, never OOB.
        let wheels = [11u32, 22, 33];
        assert_eq!(vehicle_get_wheel(&wheels, 0), Some(11));
        assert_eq!(vehicle_get_wheel(&wheels, 2), Some(33));
        assert_eq!(vehicle_get_wheel(&wheels, 3), None);
        assert_eq!(vehicle_get_wheel(&wheels, -1), None);
        assert_eq!(vehicle_get_wheel(&[], 0), None);
    }

    #[test]
    fn rva_1e9c0_get_levels_count_returns_vector_size() {
        // RVA 0x1e9c0: v_levels.size() as int.
        assert_eq!(car_levels_count(0), 0);
        assert_eq!(car_levels_count(4), 4);
    }

    #[test]
    fn rva_1e9f0_get_actual_level_reads_store() {
        // RVA 0x1e9f0: plain member read.
        assert_eq!(car_actual_level(0), 0);
        assert_eq!(car_actual_level(2), 2);
        assert_eq!(car_actual_level(-1), -1);
    }

    #[test]
    fn rva_1dec0_get_owner_gates_seat_and_occupancy() {
        // RVA 0x1dec0: NULL for bad seats and unoccupied seats.
        let seats = [
            CarSeatSlot {
                occupied: true,
                owner: 0x1234,
            },
            CarSeatSlot {
                occupied: false,
                owner: 0x5678,
            },
        ];
        assert_eq!(car_get_owner(&seats, 0), Some(0x1234));
        assert_eq!(car_get_owner(&seats, 1), None);
        assert_eq!(car_get_owner(&seats, 2), None);
        assert_eq!(car_get_owner(&seats, -1), None);
        assert_eq!(car_get_owner(&[], 0), None);
    }

    #[test]
    fn rva_29770_set_enter_protect1_sets_bit_and_wakes() {
        // RVA 0x29770: bit set/clear plus wake_request = true.
        let mut protect = 0u32;
        let mut wake = false;
        car_set_enter_protect1(&mut protect, &mut wake, 2, true);
        assert_eq!(protect, 0b100);
        assert!(wake);
        car_set_enter_protect1(&mut protect, &mut wake, 0, true);
        assert_eq!(protect, 0b101);
        car_set_enter_protect1(&mut protect, &mut wake, 2, false);
        assert_eq!(protect, 0b001);
        assert!(wake);
    }

    #[test]
    fn rva_297b0_get_car_wheel_coll_copies_collision_bits() {
        // RVA 0x297b0: colls[i] = flags & WHEELF_COLLISION, bounded prefix.
        let flags = [WHEELF_COLLISION, 0, 0x40000000 | WHEELF_COLLISION];
        let mut out = [false; 2];
        car_get_wheel_coll(&flags, &mut out);
        assert_eq!(out, [true, false]);
        let mut empty: [bool; 0] = [];
        car_get_wheel_coll(&flags, &mut empty);
    }

    #[test]
    fn rva_29800_set_car_wheel_coll_writes_collision_bits() {
        // RVA 0x29800: set/clear WHEELF_COLLISION, other bits preserved.
        let mut flags = [0u32, WHEELF_COLLISION | 0x10, 0x20];
        car_set_wheel_coll(&mut flags, &[true, false, true]);
        assert_eq!(flags, [WHEELF_COLLISION, 0x10, 0x20 | WHEELF_COLLISION]);
        let mut untouched = [0x10u32];
        car_set_wheel_coll(&mut untouched, &[]);
        assert_eq!(untouched, [0x10]);
    }

    #[test]
    fn rva_29dd0_get_actor_speed_copies_velocity() {
        // RVA 0x29dd0: speed = velocity.
        assert_eq!(car_get_actor_speed([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
        assert_eq!(car_get_actor_speed([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn rva_29e50_parked_duration_wraps_like_unsigned_sub() {
        // RVA 0x29e50: GetGameTime() - parking_time, 32-bit wrapping.
        assert_eq!(car_parked_duration(1000, 400), 600);
        assert_eq!(car_parked_duration(400, 400), 0);
        assert_eq!(car_parked_duration(0, 1), u32::MAX);
    }

    #[test]
    fn rva_2bb90_set_force_feedback_stores_five_scales() {
        // RVA 0x2bb90: five static float stores in argument order.
        let mut scales = CarForceFeedbackScales::default();
        car_set_force_feedback(&mut scales, 1.0, 2.0, 3.0, 4.0, 5.0);
        assert_eq!(
            scales,
            CarForceFeedbackScales {
                volume: 1.0,
                tire_grip: 2.0,
                collisions: 3.0,
                suspension: 4.0,
                revs: 5.0,
            }
        );
    }
}
