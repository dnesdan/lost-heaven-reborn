//! PC C_Vehicle::SetEngineOn (RVA 0xcb5b0), over the live vehicle owner.
use crate::vehicle_init::VehicleInitBackend;

pub trait VehicleEngineStateBackend: VehicleInitBackend {
    fn engine_state_sound(&mut self, vehicle: u32, enabled: bool) -> Result<(), Self::Error>;
    fn engine_state_failed_start_sound(&mut self, vehicle: u32) -> Result<(), Self::Error>;
}

/// The force branch is the original explicit caller option. Ordinary starts
/// retain the original fuel/damage guards and ordered sound dependencies.
pub fn vehicle_set_engine_on<B: VehicleEngineStateBackend>(
    b: &mut B,
    v: u32,
    enabled: bool,
    force: bool,
) -> Result<bool, B::Error> {
    if force {
        b.write_word(v + 0xc20, 5000f32.to_bits())?;
        b.write_byte(v + 0x5f0, 0)?;
        b.write_byte(v + 0xc2c, u8::from(enabled))?;
        b.write_byte(v + 0x638, u8::from(enabled))?;
        if enabled {
            b.write_word(v + 0xc20, 0)?;
        }
        return Ok(true);
    }
    if u8::from(enabled) == b.read_byte(v + 0xc2c)? {
        return Ok(true);
    }
    if enabled {
        let restricted = b.read_word(v + 0xc68)? & 0x4000 != 0;
        let fuel = f32::from_bits(b.read_word(v + 0xc30)?);
        let unavailable = fuel <= 0.02
            || fuel.is_nan()
            || (restricted && {
                let damage = f32::from_bits(b.read_word(v + 0x1b4)?);
                damage <= 0. || damage.is_nan()
            });
        if unavailable {
            if b.read_byte(v + 0x432)? != 0 {
                b.engine_state_failed_start_sound(v)?;
            }
            return Ok(true);
        }
    }
    b.engine_state_sound(v, enabled)?;
    if b.read_byte(v + 0xc2c)? == 0 {
        b.write_word(v + 0x3a4, 0)?;
        b.write_word(v + 0x3a8, 0)?;
        b.write_byte(v + 0x5f0, 0)?;
    }
    Ok(true)
}
