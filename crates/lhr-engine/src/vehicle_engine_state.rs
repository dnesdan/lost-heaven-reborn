//! PC C_Vehicle::SetEngineOn (RVA 0xcb5b0), over the live vehicle owner.
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
}

#[cfg(test)]
#[path = "vehicle_engine_state_tests.rs"]
mod tests;
