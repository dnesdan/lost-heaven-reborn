//! PC C_Vehicle::SetEngineOn (RVA 0xcb5b0), over the live vehicle owner.
//! Marker-only latch siblings live here too: SetFuel, IsVehicleUnstable,
//! GetVehicleSize, GetVehiclePivotPos, LockVehicle, GetHeadPos, the materials
//! table (Set/CloseVehicleMaterials) and the particle data copy (SetParticles).
//! The projector-light and sound sector registers (GAME 0x4ce560/0x4ce760),
//! the tyre tables (GAME 0x4c5820/0x4c5b80, deep surface copies) and
//! CloseParticles (GAME 0x4c5f80, emitter LinkTo/Release) stay DEFERRED with
//! their device/sector owners.
use crate::vehicle_init::VehicleInitBackend;

pub trait VehicleEngineStateBackend: VehicleInitBackend {
    fn engine_state_sound(&mut self, vehicle: u32, enabled: bool) -> Result<(), Self::Error>;
    fn engine_state_failed_start_sound(&mut self, vehicle: u32) -> Result<(), Self::Error>;
}

/// The force branch is the original explicit caller option. Ordinary starts
/// retain the original fuel/damage guards and ordered sound dependencies.
/// `Ok(true)` also covers an unchanged state or a refused start; the sound
/// backend owns the ordinary state transition.
pub fn vehicle_set_engine_on<B: VehicleEngineStateBackend>(
    backend: &mut B,
    vehicle: u32,
    enabled: bool,
    force: bool,
) -> Result<bool, B::Error> {
    VehicleEngine { backend, vehicle }.set_enabled(enabled, force)
}

/// A borrowed view of one owner's live state, not a snapshot. Field names below
/// describe recovered uses; they are not asserted original C++ member names.
struct VehicleEngine<'a, B> {
    backend: &'a mut B,
    vehicle: u32,
}

impl<B: VehicleEngineStateBackend> VehicleEngine<'_, B> {
    fn set_enabled(&mut self, enabled: bool, force: bool) -> Result<bool, B::Error> {
        if force {
            self.force_state(enabled)?;
            return Ok(true);
        }
        // Exact byte equality is intentional: a noncanonical value such as 2
        // must not become an "already enabled" no-op through bool conversion.
        if self.engine_state_byte()? == u8::from(enabled) {
            return Ok(true);
        }
        if enabled && self.start_is_blocked()? {
            if self.is_audible()? {
                self.backend.engine_state_failed_start_sound(self.vehicle)?;
            }
            return Ok(true);
        }

        self.backend.engine_state_sound(self.vehicle, enabled)?;
        // The callback can change the state, including on an unfinished start.
        if self.engine_state_byte()? == 0 {
            self.clear_power_requests()?;
        }
        Ok(true)
    }

    fn start_is_blocked(&mut self) -> Result<bool, B::Error> {
        let check_damage = self.damage_check_enabled()?;
        let fuel = self.fuel_level()?;
        Ok(fuel <= 0.02
            || fuel.is_nan()
            || (check_damage && {
                let damage = self.damage_condition()?;
                damage <= 0.0 || damage.is_nan()
            }))
    }

    fn force_state(&mut self, enabled: bool) -> Result<(), B::Error> {
        // Keep both fade stores and their order: any backend operation can fail
        // after earlier writes have become visible to the live owner.
        self.set_sound_fade(5000.0)?;
        self.clear_stop_request()?;
        self.backend
            .write_byte(self.vehicle + layout::ENGINE_STATE, u8::from(enabled))?;
        self.backend
            .write_byte(self.vehicle + layout::SOUND_STATE, u8::from(enabled))?;
        if enabled {
            self.set_sound_fade(0.0)?;
        }
        Ok(())
    }

    fn clear_power_requests(&mut self) -> Result<(), B::Error> {
        self.backend
            .write_word(self.vehicle + layout::PRIMARY_POWER, 0)?;
        self.backend
            .write_word(self.vehicle + layout::SECONDARY_POWER, 0)?;
        self.clear_stop_request()
    }

    fn engine_state_byte(&mut self) -> Result<u8, B::Error> {
        self.backend.read_byte(self.vehicle + layout::ENGINE_STATE)
    }

    fn damage_check_enabled(&mut self) -> Result<bool, B::Error> {
        Ok(self.backend.read_word(self.vehicle + layout::FLAGS)? & 0x4000 != 0)
    }

    fn fuel_level(&mut self) -> Result<f32, B::Error> {
        self.backend
            .read_word(self.vehicle + layout::FUEL)
            .map(f32::from_bits)
    }

    fn damage_condition(&mut self) -> Result<f32, B::Error> {
        self.backend
            .read_word(self.vehicle + layout::DAMAGE_CONDITION)
            .map(f32::from_bits)
    }

    fn is_audible(&mut self) -> Result<bool, B::Error> {
        Ok(self.backend.read_byte(self.vehicle + layout::AUDIBLE)? != 0)
    }

    fn set_sound_fade(&mut self, value: f32) -> Result<(), B::Error> {
        self.backend
            .write_word(self.vehicle + layout::SOUND_FADE, value.to_bits())
    }

    fn clear_stop_request(&mut self) -> Result<(), B::Error> {
        self.backend
            .write_byte(self.vehicle + layout::STOP_REQUEST, 0)
    }
}

// Original C_Vehicle flag bits owned by this slice (mafia-re 113b5b6
// game/vehicles/C_Vehicle.h:120-122: VEHF_UNSTABLE is IsVehicleUnstable's,
// VEHF_LOCKED holds while LockVehicle's count is above 0).
pub const VEHF_UNSTABLE: u32 = 0x8;
pub const VEHF_LOCKED: u32 = 0x10000000;

/// GAME 0x4cb6e0 C_Vehicle::SetFuel (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:148-156; contract game/vehicles/C_Vehicle.h:186-187):
/// fill the tank up to `fuel`, at most its size, and report true. A NaN fuel
/// compares false against the size and is stored as-is, exactly like the
/// original's `if (fuel > tank_size)` guard.
pub fn vehicle_set_fuel<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
    fuel: f32,
) -> Result<bool, B::Error> {
    let capacity = f32::from_bits(backend.read_word(vehicle + layout::TANK_SIZE)?);
    let clamped = if fuel > capacity { capacity } else { fuel };
    backend.write_word(vehicle + layout::FUEL, clamped.to_bits())?;
    Ok(true)
}

/// GAME 0x4cd5b0 C_Vehicle::IsVehicleUnstable (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:170-174; contract game/vehicles/C_Vehicle.h:228):
/// whether VEHF_UNSTABLE is set in the vehicle flags.
pub fn vehicle_is_unstable<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<bool, B::Error> {
    Ok(backend.read_word(vehicle + layout::VEHICLE_FLAGS)? & VEHF_UNSTABLE != 0)
}

/// GAME 0x4cd5c0 C_Vehicle::GetVehicleSize (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:176-180; contract game/vehicles/C_Vehicle.h:234-235):
/// the raw words of one of the two vehicle sizes (mafia-re
/// game/vehicles/C_Vehicle.h:458-460), the second one when `second` is set.
/// Returned as raw words so no float meaning is invented; the caller owns
/// the S_vector interpretation.
pub fn vehicle_get_size<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
    second: bool,
) -> Result<[u32; 3], B::Error> {
    let base = vehicle + if second { layout::SIZE_2 } else { layout::SIZE };
    Ok([
        backend.read_word(base)?,
        backend.read_word(base + 4)?,
        backend.read_word(base + 8)?,
    ])
}

/// GAME 0x4cd5e0 C_Vehicle::GetVehiclePivotPos (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:182-186; contract game/vehicles/C_Vehicle.h:236-237):
/// the raw words of the pivot position in the vehicle (mafia-re
/// game/vehicles/C_Vehicle.h:461-462), the second one when `second` is set.
pub fn vehicle_get_pivot_pos<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
    second: bool,
) -> Result<[u32; 3], B::Error> {
    let base = vehicle
        + if second {
            layout::PIVOT_2
        } else {
            layout::PIVOT
        };
    Ok([
        backend.read_word(base)?,
        backend.read_word(base + 4)?,
        backend.read_word(base + 8)?,
    ])
}

/// GAME 0x4cd600 C_Vehicle::LockVehicle (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:188-205; contract game/vehicles/C_Vehicle.h:188-189):
/// count the locks up (`lock`) or down, the vehicle locked (VEHF_LOCKED) while
/// any remain. Reads the count, then the flags, then stores the count and the
/// flags, matching the original's local/ordering. Wrapping arithmetic keeps
/// the host from panicking where the original int would overflow.
pub fn vehicle_lock_vehicle<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
    lock: bool,
) -> Result<(), B::Error> {
    let count = backend.read_word(vehicle + layout::LOCK_COUNT)? as i32;
    let count = if lock {
        count.wrapping_add(1)
    } else {
        count.wrapping_sub(1)
    };
    let flags = backend.read_word(vehicle + layout::VEHICLE_FLAGS)?;
    backend.write_word(vehicle + layout::LOCK_COUNT, count as u32)?;
    backend.write_word(
        vehicle + layout::VEHICLE_FLAGS,
        if count > 0 {
            flags | VEHF_LOCKED
        } else {
            flags & !VEHF_LOCKED
        },
    )
}

/// One entry of the vehicles' shared material table (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.h:42-59: S_VEH_MATERIAL, 0x30 bytes). Only the
/// conversion-table index keeps its meaning here; the rest is carried as raw
/// words with no member meaning assigned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VehicleMaterialEntry {
    /// S_VEH_MATERIAL `id` (mafia-re 113b5b6 game/vehicles/C_Vehicle.h:46),
    /// its index in materialList. Typed `u8` because the original indexes
    /// `s_material_conv_tbl[0x100]` by it.
    pub id: u8,
    /// The remaining 0x2c bytes of the record as raw words.
    pub payload: [u32; 11],
}

/// The vehicles' shared material table and its per-id entry index (mafia-re
/// 113b5b6 game/vehicles/C_Vehicle.cpp:24-26: `v_materials` and
/// `s_material_conv_tbl[0x100]`). No Default: `[u8; 256]` has none;
/// construct explicitly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleMaterialTable {
    pub entries: Vec<VehicleMaterialEntry>,
    pub conv: [u8; 0x100],
}

impl Default for VehicleMaterialTable {
    /// Empty table with a cleared conversion map (`[u8; 256]` has no
    /// deriveable Default).
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            conv: [0; 0x100],
        }
    }
}

/// One entry of the vehicles' shared particle table (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.h:63-70: S_VEH_PARTICLE, 0x1c bytes), data only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VehicleParticleEntry {
    /// Opaque emitter identity copied with the entry. Never dereferenced here;
    /// `LinkTo`/`Release` belong to the device/sector owner, so CloseParticles
    /// (GAME 0x4c5f80) stays DEFERRED with it.
    pub emitter: u32,
    /// `velocity` (PP_VELOCITY, C_Vehicle.h:68) as raw words.
    pub velocity: [u32; 3],
    /// `velocity_end` (PP_VELOCITY_END, C_Vehicle.h:69) as raw words.
    pub velocity_end: [u32; 3],
}

/// GAME 0x4c5c10 C_Vehicle::SetVehicleMaterials (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:73-85; contract game/vehicles/C_Vehicle.h:158):
/// clear the table, rebuild the conversion table, then push a copy of each
/// entry and record its index under its id. The `while (i--)` clear leaves
/// index 0xff untouched, preserved here. A truncating `as u8` mirrors the
/// original's implicit uint-to-uchar conversion. Always returns true.
pub fn vehicle_set_materials(
    table: &mut VehicleMaterialTable,
    materials: &[VehicleMaterialEntry],
) -> bool {
    vehicle_close_materials(table);
    // GAME 0x4c5c10's `int i = 0xff; while (i--) tbl[i] = 0;` clears 0xfe..=0.
    table.conv[..0xff].fill(0);
    for (index, entry) in materials.iter().enumerate() {
        table.entries.push(*entry);
        table.conv[usize::from(entry.id)] = index as u8;
    }
    true
}

/// GAME 0x4c5c80 C_Vehicle::CloseVehicleMaterials (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:87-91; contract game/vehicles/C_Vehicle.h:159):
/// clear `v_materials`. The conversion table is left as-is, like the original.
pub fn vehicle_close_materials(table: &mut VehicleMaterialTable) {
    table.entries.clear();
}

/// GAME 0x4c5cc0 C_Vehicle::SetParticles (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:93-100; contract game/vehicles/C_Vehicle.h:161):
/// data-only copy of `v_particle`. The original first runs CloseParticles,
/// whose emitter unlink/release belong to the device owner (DEFERRED); only
/// the vector clear is performed here. Always returns true.
pub fn vehicle_set_particles(
    particles: &mut Vec<VehicleParticleEntry>,
    source: &[VehicleParticleEntry],
) -> bool {
    particles.clear();
    particles.extend_from_slice(source);
    true
}

/// GAME 0x4cc620 C_Vehicle::GetHeadPos (mafia-re 113b5b6
/// game/vehicles/C_Vehicle.cpp:164-168; contract game/vehicles/C_Vehicle.h:226):
/// the raw words of the driver's head position in the vehicle (mafia-re
/// 113b5b6 game/vehicles/C_Vehicle.h:471). Returned by value as raw words so
/// no float meaning is invented; the caller owns the S_vector interpretation.
pub fn vehicle_get_head_pos<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<[u32; 3], B::Error> {
    Ok([
        backend.read_word(vehicle + layout::HEAD_POS)?,
        backend.read_word(vehicle + layout::HEAD_POS + 4)?,
        backend.read_word(vehicle + layout::HEAD_POS + 8)?,
    ])
}

/// RVA 0xcd590, VA 0x4cd590 (`C_Vehicle::GetRotations`, mafia-re fe7e746
/// game/vehicles/C_Vehicle.cpp:170-177 FUNCTION: GAME 0x4cd590; contract
/// game/vehicles/C_Vehicle.h:222-223): the engine's revolutions a minute,
/// 0.0 while it is off. Reads the `engine_on` byte at vehicle+0xc2c and the
/// `rotations` float at vehicle+0x548; when on, returns `rotations * 60.0`.
/// A NaN `rotations` propagates through the product like the original multiply.
pub fn vehicle_get_rotations<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<f32, B::Error> {
    if backend.read_byte(vehicle + layout::ENGINE_STATE)? == 0 {
        return Ok(0.0);
    }
    Ok(f32::from_bits(backend.read_word(vehicle + layout::ROTATIONS)?) * 60.0)
}

/// Working name `vehicle_sounds_enabled` (no standalone entry: header-inline
/// `C_Vehicle::SoundsEnabled`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:205-210,
/// `return sounds_enabled` at vehicle+0x432): whether the vehicle's sounds are on.
pub fn vehicle_sounds_enabled<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<bool, B::Error> {
    Ok(backend.read_byte(vehicle + layout::SOUNDS_ENABLED)? != 0)
}

/// Working name `vehicle_get_max_rotations` (no standalone entry: header-inline
/// `C_Vehicle::GetMaxRotations`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:248-251,
/// `return max_rotations * 60.0f` at vehicle+0x1f9c): the tachometer's full-scale
/// revolutions a minute.
pub fn vehicle_get_max_rotations<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<f32, B::Error> {
    Ok(f32::from_bits(backend.read_word(vehicle + layout::MAX_ROTATIONS)?) * 60.0)
}

/// Working name `vehicle_get_odometer` (no standalone entry: header-inline
/// `C_Vehicle::GetOdometer`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:252-255,
/// `return odometer` at vehicle+0x1fa4): the tachometer's distance counter.
pub fn vehicle_get_odometer<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<f32, B::Error> {
    Ok(f32::from_bits(
        backend.read_word(vehicle + layout::ODOMETER)?,
    ))
}

/// Working name `vehicle_get_tank_fuel` (no standalone entry: header-inline
/// `C_Vehicle::GetTankFuel`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:256-259,
/// `return tank_fuel` at vehicle+0xc30): the fuel currently in the tank.
pub fn vehicle_get_tank_fuel<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<f32, B::Error> {
    Ok(f32::from_bits(backend.read_word(vehicle + layout::FUEL)?))
}

/// Working name `vehicle_get_tank_size` (no standalone entry: header-inline
/// `C_Vehicle::GetTankSize`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:284-289,
/// `return tank_size` at vehicle+0x1fa0): `SetFuel`'s upper bound.
pub fn vehicle_get_tank_size<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<f32, B::Error> {
    Ok(f32::from_bits(
        backend.read_word(vehicle + layout::TANK_SIZE)?,
    ))
}

/// Working name `vehicle_is_unknown_2000000` (no standalone entry: header-inline
/// `C_Vehicle::IsUnknown2000000`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:269-273,
/// `return (vehicle_flags >> 25) & 1` at vehicle+0x1a4): whether the
/// VEHF_UNKNOWN_2000000 bridge bit is set.
pub fn vehicle_is_unknown_2000000<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<bool, B::Error> {
    Ok(((backend.read_word(vehicle + layout::VEHICLE_FLAGS)? >> 25) & 1) != 0)
}

/// Working name `vehicle_get_auto_gearing` (no standalone entry: header-inline
/// `C_Vehicle::GetAutoGearing`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:244-247,
/// `return auto_gearing` at vehicle+0x4cc; only the setter has a standalone
/// entry, GAME 0x4a9c00): whether automatic gearing is selected.
pub fn vehicle_get_auto_gearing<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<bool, B::Error> {
    Ok(backend.read_byte(vehicle + layout::AUTO_GEARING)? != 0)
}

/// Working name `vehicle_set_analog_inputs` (no standalone entry: header-inline
/// `C_Vehicle::SetAnalogInputs`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:232-240,
/// four bool stores at vehicle+0x4a8-0x4ab): which of steering, throttle, brake
/// and clutch take the secondary binding's value.
pub fn vehicle_set_analog_inputs<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
    steer: bool,
    power: bool,
    brake: bool,
    clutch: bool,
) -> Result<(), B::Error> {
    backend.write_byte(vehicle + layout::ANALOG_STEER, u8::from(steer))?;
    backend.write_byte(vehicle + layout::ANALOG_POWER, u8::from(power))?;
    backend.write_byte(vehicle + layout::ANALOG_BRAKE, u8::from(brake))?;
    backend.write_byte(vehicle + layout::ANALOG_CLUTCH, u8::from(clutch))
}

/// Working name `vehicle_set_light_unknown_800` (no standalone entry: header-inline
/// `C_Vehicle::SetLightUnknown800`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:335-344,
/// sets/clears VLIGHT_UNKNOWN_800 in `lights` at vehicle+0x11c with no wake-up
/// edge): the 0x800 light bit.
pub fn vehicle_set_light_unknown_800<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
    on: bool,
) -> Result<(), B::Error> {
    let lights = backend.read_word(vehicle + layout::LIGHTS)?;
    backend.write_word(
        vehicle + layout::LIGHTS,
        if on { lights | 0x800 } else { lights & !0x800 },
    )
}

/// Working name `vehicle_set_light_unknown_8000` (no standalone entry: header-inline
/// `C_Vehicle::SetLightUnknown8000`, mafia-re fe7e746 game/vehicles/C_Vehicle.h:345-351,
/// sets/clears VLIGHT_UNKNOWN_8000 in `lights` at vehicle+0x11c with no wake-up
/// edge): the 0x8000 light bit.
pub fn vehicle_set_light_unknown_8000<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
    on: bool,
) -> Result<(), B::Error> {
    let lights = backend.read_word(vehicle + layout::LIGHTS)?;
    backend.write_word(
        vehicle + layout::LIGHTS,
        if on {
            lights | 0x8000
        } else {
            lights & !0x8000
        },
    )
}

/// Working name `car_get_color` (no standalone entry: header-inline `C_car::GetColor`,
/// mafia-re fe7e746 game/vehicles/C_car.h:175-176, `return color` at car+0x211c,
/// i.e. vehicle+0x20ac past the C_Vehicle base at car+0x70): the model's variant
/// passed to the car database.
pub fn car_get_color<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: u32,
) -> Result<u32, B::Error> {
    backend.read_word(vehicle + layout::COLOR)
}

/// RVA 0x1c790, VA 0x41c790 (`C_car::GetUSindex`, mafia-re fe7e746
/// game/vehicles/C_car.cpp:704-708 STUB: GAME 0x41c790; declaration
/// game/vehicles/C_car.h:168-169): the seat of the car's using object, -1 for
/// one of no seat. The using-object-to-seat mapping owner is missing here, so
/// this is an explicit unsupported boundary; no `-1` default is invented.
pub const CAR_GET_USINDEX_ENTRY: u32 = 0x41c790;

/// Host failure for the `C_car::GetUSindex` boundary: the original seat mapping
/// is unavailable, distinct from a found seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarUsindexStubError {
    /// The original mapping owner is missing.
    Unsupported {
        /// Original entry VA, [`CAR_GET_USINDEX_ENTRY`].
        entry: u32,
    },
}

pub fn car_get_usindex_unsupported(_object: u32) -> Result<i32, CarUsindexStubError> {
    Err(CarUsindexStubError::Unsupported {
        entry: CAR_GET_USINDEX_ENTRY,
    })
}

// PC storage mapping stays local to this view. Accessors deliberately read on
// demand and use the existing address arithmetic and backend error semantics.
mod layout {
    pub(super) const DAMAGE_CONDITION: u32 = 0x1b4;
    pub(super) const PRIMARY_POWER: u32 = 0x3a4;
    pub(super) const SECONDARY_POWER: u32 = 0x3a8;
    pub(super) const AUDIBLE: u32 = 0x432;
    pub(super) const STOP_REQUEST: u32 = 0x5f0;
    pub(super) const SOUND_STATE: u32 = 0x638;
    pub(super) const SOUND_FADE: u32 = 0xc20;
    pub(super) const ENGINE_STATE: u32 = 0xc2c;
    pub(super) const FUEL: u32 = 0xc30;
    pub(super) const FLAGS: u32 = 0xc68;
    // C_Vehicle latch slice (mafia-re 113b5b6 game/vehicles/C_Vehicle.h):
    // lock_count 0xc0 (:454), sizes 0x158/0x164 (:459-460), pivots
    // 0x170/0x17c (:461-462), vehicle_flags 0x1a4 (:464), head_pos
    // 0x2d4 (:471), tank_size 0x1fa0 (:549); tank_fuel reuses FUEL
    // 0xc30 above (:521).
    pub(super) const LOCK_COUNT: u32 = 0xc0;
    pub(super) const SIZE: u32 = 0x158;
    pub(super) const SIZE_2: u32 = 0x164;
    pub(super) const PIVOT: u32 = 0x170;
    pub(super) const PIVOT_2: u32 = 0x17c;
    pub(super) const VEHICLE_FLAGS: u32 = 0x1a4;
    pub(super) const HEAD_POS: u32 = 0x2d4;
    pub(super) const TANK_SIZE: u32 = 0x1fa0;
    // Small-latch slice (mafia-re fe7e746 game/vehicles/C_Vehicle.h):
    // lights 0x11c (:497), sounds_enabled 0x432 (:534, shared with AUDIBLE
    // above), analog inputs 0x4a8-0x4ab (:544-548), auto_gearing 0x4cc (:555),
    // rotations 0x548 (:559-561), max_rotations 0x1f9c (:610),
    // odometer 0x1fa4 (:612); car color at car+0x211c (mafia-re fe7e746
    // game/vehicles/C_car.h:288) is vehicle+0x20ac past the C_Vehicle base at
    // car+0x70.
    pub(super) const LIGHTS: u32 = 0x11c;
    pub(super) const SOUNDS_ENABLED: u32 = 0x432;
    pub(super) const ANALOG_STEER: u32 = 0x4a8;
    pub(super) const ANALOG_POWER: u32 = 0x4a9;
    pub(super) const ANALOG_BRAKE: u32 = 0x4aa;
    pub(super) const ANALOG_CLUTCH: u32 = 0x4ab;
    pub(super) const AUTO_GEARING: u32 = 0x4cc;
    pub(super) const ROTATIONS: u32 = 0x548;
    pub(super) const MAX_ROTATIONS: u32 = 0x1f9c;
    pub(super) const ODOMETER: u32 = 0x1fa4;
    pub(super) const COLOR: u32 = 0x20ac;
}

#[cfg(test)]
#[path = "vehicle_engine_state_tests.rs"]
mod tests;

#[cfg(test)]
mod latch_tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
        VehicleUninitializedStorage,
    };
    use std::collections::BTreeMap;

    const VEHICLE: u32 = 0x1000;

    struct LatchBackend {
        bytes: BTreeMap<u32, u8>,
    }
    impl LatchBackend {
        fn with_words(words: &[(u32, u32)]) -> Self {
            let mut backend = Self {
                bytes: BTreeMap::new(),
            };
            for &(address, value) in words {
                backend.put_word(address, value);
            }
            backend
        }
        fn put_word(&mut self, address: u32, value: u32) {
            for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
                self.bytes.insert(address + i as u32, byte);
            }
        }
        fn get_word(&self, address: u32) -> u32 {
            u32::from_le_bytes(std::array::from_fn(|i| self.bytes[&(address + i as u32)]))
        }
    }
    impl VehicleInitBackend for LatchBackend {
        type Error = String;
        fn read_byte(&mut self, address: u32) -> Result<u8, String> {
            self.bytes
                .get(&address)
                .copied()
                .ok_or_else(|| String::from(VehicleUninitializedStorage { address }))
        }
        fn read_word(&mut self, address: u32) -> Result<u32, String> {
            Ok(u32::from_le_bytes([
                self.read_byte(address)?,
                self.read_byte(address + 1)?,
                self.read_byte(address + 2)?,
                self.read_byte(address + 3)?,
            ]))
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), String> {
            self.bytes.insert(address, value);
            Ok(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), String> {
            for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
                self.write_byte(address + i as u32, byte)?;
            }
            Ok(())
        }
        fn allocate(&mut self, _: u32) -> Result<u32, String> {
            Err("unexpected allocation".into())
        }
        fn free(&mut self, _: u32) -> Result<(), String> {
            Err("unexpected free".into())
        }
        fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), String> {
            Err("unexpected collector".into())
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<String>> {
            Err(VehicleInitUnavailable::Effect(
                "unexpected enumeration".into(),
            ))
        }
    }

    #[test]
    fn set_fuel_clamps_to_tank_size() {
        // GAME 0x4cb6e0 C_Vehicle::SetFuel: tank 50, fuel 60 stores 50.
        let mut backend = LatchBackend::with_words(&[
            (VEHICLE + layout::TANK_SIZE, 50.0f32.to_bits()),
            (VEHICLE + layout::FUEL, 0),
        ]);
        assert_eq!(vehicle_set_fuel(&mut backend, VEHICLE, 60.0), Ok(true));
        assert_eq!(backend.get_word(VEHICLE + layout::FUEL), 50.0f32.to_bits());
        // Below capacity stores the fuel unchanged.
        assert_eq!(vehicle_set_fuel(&mut backend, VEHICLE, 30.0), Ok(true));
        assert_eq!(backend.get_word(VEHICLE + layout::FUEL), 30.0f32.to_bits());
    }

    #[test]
    fn is_unstable_reads_vehf_unstable_bit() {
        // GAME 0x4cd5b0 C_Vehicle::IsVehicleUnstable: flags & VEHF_UNSTABLE.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::VEHICLE_FLAGS, 0)]);
        assert_eq!(vehicle_is_unstable(&mut backend, VEHICLE), Ok(false));
        backend.put_word(VEHICLE + layout::VEHICLE_FLAGS, VEHF_UNSTABLE);
        assert_eq!(vehicle_is_unstable(&mut backend, VEHICLE), Ok(true));
    }

    #[test]
    fn size_and_pivot_select_branch_by_flag() {
        // GAME 0x4cd5c0/0x4cd5e0: false reads the first vector, true the second.
        let mut backend = LatchBackend::with_words(&[
            (VEHICLE + layout::SIZE, 1),
            (VEHICLE + layout::SIZE + 4, 2),
            (VEHICLE + layout::SIZE + 8, 3),
            (VEHICLE + layout::SIZE_2, 4),
            (VEHICLE + layout::SIZE_2 + 4, 5),
            (VEHICLE + layout::SIZE_2 + 8, 6),
            (VEHICLE + layout::PIVOT, 7),
            (VEHICLE + layout::PIVOT + 4, 8),
            (VEHICLE + layout::PIVOT + 8, 9),
            (VEHICLE + layout::PIVOT_2, 10),
            (VEHICLE + layout::PIVOT_2 + 4, 11),
            (VEHICLE + layout::PIVOT_2 + 8, 12),
        ]);
        assert_eq!(
            vehicle_get_size(&mut backend, VEHICLE, false),
            Ok([1, 2, 3])
        );
        assert_eq!(vehicle_get_size(&mut backend, VEHICLE, true), Ok([4, 5, 6]));
        assert_eq!(
            vehicle_get_pivot_pos(&mut backend, VEHICLE, false),
            Ok([7, 8, 9])
        );
        assert_eq!(
            vehicle_get_pivot_pos(&mut backend, VEHICLE, true),
            Ok([10, 11, 12])
        );
    }

    #[test]
    fn close_materials_empties_entries_and_keeps_conv() {
        // GAME 0x4c5c10/0x4c5c80: two entries set, then closed; entries are
        // empty while the conversion table keeps its mapping (Close touches
        // only v_materials).
        let mut table = VehicleMaterialTable::default();
        let materials = [
            VehicleMaterialEntry {
                id: 3,
                payload: [1; 11],
            },
            VehicleMaterialEntry {
                id: 9,
                payload: [2; 11],
            },
        ];
        assert!(vehicle_set_materials(&mut table, &materials));
        assert_eq!(table.entries.len(), 2);
        vehicle_close_materials(&mut table);
        assert!(table.entries.is_empty());
        assert_eq!(table.conv[3], 0);
        assert_eq!(table.conv[9], 1);
    }

    #[test]
    fn set_materials_rebuilds_count_and_conv() {
        // GAME 0x4c5c10: three entries land with count 3 and conv[id] == index;
        // a stale slot is cleared while index 0xff keeps its quirk value.
        let mut table = VehicleMaterialTable::default();
        table.conv[5] = 7;
        table.conv[0xff] = 7;
        let materials = [
            VehicleMaterialEntry {
                id: 2,
                payload: [10; 11],
            },
            VehicleMaterialEntry {
                id: 4,
                payload: [20; 11],
            },
            VehicleMaterialEntry {
                id: 6,
                payload: [30; 11],
            },
        ];
        assert!(vehicle_set_materials(&mut table, &materials));
        assert_eq!(table.entries.len(), 3);
        assert_eq!(table.entries, materials);
        assert_eq!([table.conv[2], table.conv[4], table.conv[6]], [0, 1, 2]);
        assert_eq!(table.conv[5], 0);
        assert_eq!(table.conv[0xff], 7);
    }

    #[test]
    fn head_pos_reads_default_and_stored_words() {
        // GAME 0x4cc620 C_Vehicle::GetHeadPos: zeroed head reads default, then
        // the stored words.
        let mut backend = LatchBackend::with_words(&[
            (VEHICLE + layout::HEAD_POS, 0),
            (VEHICLE + layout::HEAD_POS + 4, 0),
            (VEHICLE + layout::HEAD_POS + 8, 0),
        ]);
        assert_eq!(vehicle_get_head_pos(&mut backend, VEHICLE), Ok([0, 0, 0]));
        backend.put_word(VEHICLE + layout::HEAD_POS, 1);
        backend.put_word(VEHICLE + layout::HEAD_POS + 4, 2);
        backend.put_word(VEHICLE + layout::HEAD_POS + 8, 3);
        assert_eq!(vehicle_get_head_pos(&mut backend, VEHICLE), Ok([1, 2, 3]));
    }

    #[test]
    fn set_particles_copies_entries() {
        // GAME 0x4c5cc0 C_Vehicle::SetParticles: stale contents are replaced by
        // an equal copy of the source, emitter tokens included.
        let mut particles = vec![VehicleParticleEntry {
            emitter: 9,
            velocity: [7; 3],
            velocity_end: [8; 3],
        }];
        let source = [
            VehicleParticleEntry {
                emitter: 1,
                velocity: [1, 2, 3],
                velocity_end: [4, 5, 6],
            },
            VehicleParticleEntry {
                emitter: 0,
                velocity: [0; 3],
                velocity_end: [0; 3],
            },
        ];
        assert!(vehicle_set_particles(&mut particles, &source));
        assert_eq!(particles, source);
    }

    #[test]
    fn lock_counts_and_holds_locked_while_any_remain() {
        // GAME 0x4cd600 C_Vehicle::LockVehicle: true/true/false lands on
        // count 2/1 with VEHF_LOCKED held; draining to 0 clears it.
        let mut backend = LatchBackend::with_words(&[
            (VEHICLE + layout::LOCK_COUNT, 0),
            (VEHICLE + layout::VEHICLE_FLAGS, 0),
        ]);
        vehicle_lock_vehicle(&mut backend, VEHICLE, true).unwrap();
        vehicle_lock_vehicle(&mut backend, VEHICLE, true).unwrap();
        assert_eq!(backend.get_word(VEHICLE + layout::LOCK_COUNT), 2);
        assert_eq!(
            backend.get_word(VEHICLE + layout::VEHICLE_FLAGS) & VEHF_LOCKED,
            VEHF_LOCKED
        );
        vehicle_lock_vehicle(&mut backend, VEHICLE, false).unwrap();
        assert_eq!(backend.get_word(VEHICLE + layout::LOCK_COUNT), 1);
        assert_eq!(
            backend.get_word(VEHICLE + layout::VEHICLE_FLAGS) & VEHF_LOCKED,
            VEHF_LOCKED
        );
        vehicle_lock_vehicle(&mut backend, VEHICLE, false).unwrap();
        assert_eq!(backend.get_word(VEHICLE + layout::LOCK_COUNT), 0);
        assert_eq!(
            backend.get_word(VEHICLE + layout::VEHICLE_FLAGS) & VEHF_LOCKED,
            0
        );
    }

    #[test]
    fn rva_cd590_get_rotations_scales_or_zero() {
        // GAME RVA 0xcd590 (VA 0x4cd590) C_Vehicle::GetRotations: engine on
        // scales rotations by 60, engine off reads 0, NaN propagates.
        let mut backend = LatchBackend::with_words(&[
            (VEHICLE + layout::ENGINE_STATE, 1),
            (VEHICLE + layout::ROTATIONS, 1.5f32.to_bits()),
        ]);
        assert_eq!(vehicle_get_rotations(&mut backend, VEHICLE), Ok(90.0));
        backend.put_word(VEHICLE + layout::ENGINE_STATE, 0);
        assert_eq!(vehicle_get_rotations(&mut backend, VEHICLE), Ok(0.0));
        backend.put_word(VEHICLE + layout::ENGINE_STATE, 1);
        backend.put_word(VEHICLE + layout::ROTATIONS, f32::NAN.to_bits());
        assert!(vehicle_get_rotations(&mut backend, VEHICLE)
            .unwrap()
            .is_nan());
    }

    #[test]
    fn rva_1c790_get_usindex_reports_unsupported() {
        // C_car::GetUSindex GAME RVA 0x1c790 (VA 0x41c790): the seat mapping
        // owner is missing, so the boundary reports unsupported.
        assert_eq!(CAR_GET_USINDEX_ENTRY, 0x41c790);
        assert_eq!(
            car_get_usindex_unsupported(0x1000),
            Err(CarUsindexStubError::Unsupported { entry: 0x41c790 })
        );
    }

    #[test]
    fn sounds_enabled_reads_flag() {
        // Header-inline C_Vehicle::SoundsEnabled: nonzero byte reads true.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::SOUNDS_ENABLED, 0)]);
        assert_eq!(vehicle_sounds_enabled(&mut backend, VEHICLE), Ok(false));
        backend.put_word(VEHICLE + layout::SOUNDS_ENABLED, 1);
        assert_eq!(vehicle_sounds_enabled(&mut backend, VEHICLE), Ok(true));
    }

    #[test]
    fn max_rotations_scales_by_sixty() {
        // Header-inline C_Vehicle::GetMaxRotations: max_rotations * 60.
        let mut backend =
            LatchBackend::with_words(&[(VEHICLE + layout::MAX_ROTATIONS, 2.0f32.to_bits())]);
        assert_eq!(vehicle_get_max_rotations(&mut backend, VEHICLE), Ok(120.0));
    }

    #[test]
    fn odometer_reads_store() {
        // Header-inline C_Vehicle::GetOdometer: stored float round-trips.
        let mut backend =
            LatchBackend::with_words(&[(VEHICLE + layout::ODOMETER, 1000.0f32.to_bits())]);
        assert_eq!(vehicle_get_odometer(&mut backend, VEHICLE), Ok(1000.0));
    }

    #[test]
    fn tank_fuel_and_size_read_stores() {
        // Header-inline C_Vehicle::GetTankFuel/GetTankSize: stored floats.
        let mut backend = LatchBackend::with_words(&[
            (VEHICLE + layout::FUEL, 25.5f32.to_bits()),
            (VEHICLE + layout::TANK_SIZE, 50.0f32.to_bits()),
        ]);
        assert_eq!(vehicle_get_tank_fuel(&mut backend, VEHICLE), Ok(25.5));
        assert_eq!(vehicle_get_tank_size(&mut backend, VEHICLE), Ok(50.0));
    }

    #[test]
    fn unknown_2000000_tests_bit_25() {
        // Header-inline C_Vehicle::IsUnknown2000000: (flags >> 25) & 1.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::VEHICLE_FLAGS, 0)]);
        assert_eq!(vehicle_is_unknown_2000000(&mut backend, VEHICLE), Ok(false));
        backend.put_word(VEHICLE + layout::VEHICLE_FLAGS, 1 << 25);
        assert_eq!(vehicle_is_unknown_2000000(&mut backend, VEHICLE), Ok(true));
        backend.put_word(VEHICLE + layout::VEHICLE_FLAGS, 1 << 26);
        assert_eq!(vehicle_is_unknown_2000000(&mut backend, VEHICLE), Ok(false));
        backend.put_word(VEHICLE + layout::VEHICLE_FLAGS, u32::MAX);
        assert_eq!(vehicle_is_unknown_2000000(&mut backend, VEHICLE), Ok(true));
    }

    #[test]
    fn auto_gearing_reads_flag() {
        // Header-inline C_Vehicle::GetAutoGearing: nonzero byte reads true.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::AUTO_GEARING, 0)]);
        assert_eq!(vehicle_get_auto_gearing(&mut backend, VEHICLE), Ok(false));
        backend.put_word(VEHICLE + layout::AUTO_GEARING, 1);
        assert_eq!(vehicle_get_auto_gearing(&mut backend, VEHICLE), Ok(true));
    }

    #[test]
    fn analog_inputs_store_four_bytes() {
        // Header-inline C_Vehicle::SetAnalogInputs: four bool stores in order.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::ANALOG_STEER, 0)]);
        vehicle_set_analog_inputs(&mut backend, VEHICLE, true, false, true, false).unwrap();
        assert_eq!(
            backend.get_word(VEHICLE + layout::ANALOG_STEER),
            0x0001_0001
        );
        vehicle_set_analog_inputs(&mut backend, VEHICLE, false, true, false, true).unwrap();
        assert_eq!(
            backend.get_word(VEHICLE + layout::ANALOG_STEER),
            0x0100_0100
        );
    }

    #[test]
    fn light_unknown_800_sets_and_clears_bit() {
        // Header-inline C_Vehicle::SetLightUnknown800: bit 0x800 only.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::LIGHTS, 0xFFFF_FFFF)]);
        vehicle_set_light_unknown_800(&mut backend, VEHICLE, false).unwrap();
        assert_eq!(backend.get_word(VEHICLE + layout::LIGHTS), !0x800);
        vehicle_set_light_unknown_800(&mut backend, VEHICLE, true).unwrap();
        assert_eq!(backend.get_word(VEHICLE + layout::LIGHTS), u32::MAX);
    }

    #[test]
    fn light_unknown_8000_sets_and_clears_bit() {
        // Header-inline C_Vehicle::SetLightUnknown8000: bit 0x8000 only.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::LIGHTS, 0)]);
        vehicle_set_light_unknown_8000(&mut backend, VEHICLE, true).unwrap();
        assert_eq!(backend.get_word(VEHICLE + layout::LIGHTS), 0x8000);
        vehicle_set_light_unknown_8000(&mut backend, VEHICLE, false).unwrap();
        assert_eq!(backend.get_word(VEHICLE + layout::LIGHTS), 0);
    }

    #[test]
    fn color_reads_car_variant_word() {
        // Header-inline C_car::GetColor: car+0x211c word round-trips.
        let mut backend = LatchBackend::with_words(&[(VEHICLE + layout::COLOR, 0x1234_5678)]);
        assert_eq!(car_get_color(&mut backend, VEHICLE), Ok(0x1234_5678));
    }
}
