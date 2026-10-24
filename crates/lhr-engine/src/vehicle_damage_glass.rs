//! Original C_Vehicle glass-damage worker and its caller-owned callbacks.
//! Mesh/material operations use the same visual resources as deformation.
use crate::vehicle_damage::VehicleDamageMeshBackend;
use crate::vehicle_init::VehicleInitBackend;
#[cfg(test)]
#[path = "vehicle_damage_glass_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleDamageGlassCallback {
    pub function: u32,
    pub context: u32,
    pub index: u32,
    pub direction: [f32; 3],
    pub sound_name: Option<u32>,
}

pub trait VehicleDamageGlassBackend: VehicleDamageMeshBackend {
    fn damage_glass_callback(
        &mut self,
        call: VehicleDamageGlassCallback,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleGlassBreakCall {
    pub manager: u32,
    pub frame: u32,
    pub direction: Option<[f32; 3]>,
    pub sound_name: u32,
    pub retain_original: bool,
}
pub trait CarDamageGlassBackend: VehicleInitBackend {
    fn car_glass_break(&mut self, call: VehicleGlassBreakCall) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleDamageGlassError<E> {
    Host(E),
    AddressRange,
    Callback(u32),
}
type Out<B, T> =
    Result<T, VehicleDamageGlassError<<B as crate::vehicle_init::VehicleInitBackend>::Error>>;
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u32> {
    b.read_word(a).map_err(VehicleDamageGlassError::Host)
}
fn at<E>(base: u32, offset: u32) -> Result<u32, VehicleDamageGlassError<E>> {
    base.checked_add(offset)
        .ok_or(VehicleDamageGlassError::AddressRange)
}

/// PC4d6670: first installs the damaged glass material below full health;
/// exhausted health marks the live row and calls the constructor-bound hook.
pub fn vehicle_damage_glass<B: VehicleDamageGlassBackend>(
    b: &mut B,
    vehicle: u32,
    index: u32,
    direction: [f32; 3],
) -> Out<B, ()> {
    let offset = index
        .checked_mul(0x34)
        .ok_or(VehicleDamageGlassError::AddressRange)?;
    let row = at(word(b, at(vehicle, 0x218)?)?, offset)?;
    let current = f32::from_bits(word(b, at(row, 0x20)?)?);
    let initial = f32::from_bits(word(b, at(row, 0x1c)?)?);
    if current < initial {
        let mesh = word(b, at(row, 4)?)?;
        let lod = b
            .damage_virtual(mesh, 4, &[0])
            .map_err(VehicleDamageGlassError::Host)?;
        let group = b
            .damage_virtual(lod, 0x14, &[0])
            .map_err(VehicleDamageGlassError::Host)?;
        // The virtual calls may mutate their owner, so fetch its live row.
        let row = at(word(b, at(vehicle, 0x218)?)?, offset)?;
        let material = word(b, at(row, 8)?)?;
        b.damage_virtual(group, 0, &[material])
            .map_err(VehicleDamageGlassError::Host)?;
    }
    let row = at(word(b, at(vehicle, 0x218)?)?, offset)?;
    let current = f32::from_bits(word(b, at(row, 0x20)?)?);
    if current <= 0. {
        let flags_at = at(row, 0xe)?;
        let flags = b
            .read_byte(flags_at)
            .map_err(VehicleDamageGlassError::Host)?;
        b.write_byte(flags_at, flags | 1)
            .map_err(VehicleDamageGlassError::Host)?;
        let alternate = b
            .read_byte(at(vehicle, 0x491)?)
            .map_err(VehicleDamageGlassError::Host)?;
        let name = at(vehicle, if alternate != 0 { 0x1800 } else { 0x1cb0 })?;
        let mut length = 0;
        loop {
            if b.read_byte(at(name, length)?)
                .map_err(VehicleDamageGlassError::Host)?
                == 0
            {
                break;
            }
            length = length
                .checked_add(1)
                .ok_or(VehicleDamageGlassError::AddressRange)?;
        }
        let function = word(b, at(vehicle, 0x300)?)?;
        let context = word(b, at(vehicle, 0xc74)?)?;
        b.damage_glass_callback(VehicleDamageGlassCallback {
            function,
            context,
            index,
            direction,
            sound_name: (length != 0).then_some(name),
        })
        .map_err(VehicleDamageGlassError::Host)?;
    }
    Ok(())
}

/// C_car::Break_Glass, constructor-bound PC4262f0. Resolves the live part frame and forwards to
/// the game's glass manager with removal enabled, including its default sound.
pub fn car_damage_glass_callback<B: CarDamageGlassBackend>(
    b: &mut B,
    call: VehicleDamageGlassCallback,
) -> Out<B, ()> {
    if call.function != 0x4262f0 {
        return Err(VehicleDamageGlassError::Callback(call.function));
    }
    let offset = call
        .index
        .checked_mul(0x34)
        .ok_or(VehicleDamageGlassError::AddressRange)?;
    let parts = word(b, at(call.context, 0x288)?)?;
    let frame = word(b, at(parts, offset)?)?;
    let app = word(b, 0x63788c)?;
    let game = word(b, at(app, 0x24)?)?;
    b.car_glass_break(VehicleGlassBreakCall {
        manager: at(game, 0x208)?,
        frame,
        direction: Some(call.direction),
        sound_name: call.sound_name.unwrap_or(0x6323b8),
        retain_original: true,
    })
    .map_err(VehicleDamageGlassError::Host)
}
