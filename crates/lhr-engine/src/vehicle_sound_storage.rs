//! The legacy vehicle layout used by GearSnd, isolated from its effect order.
//! Views borrow the caller's canonical storage. Binding does not validate or
//! decode it eagerly; each method reads only when that operation is reached.
#![forbid(unsafe_code)]

use super::VehicleGearSoundUnavailable;
use crate::vehicle_drivetrain::VehicleDrivetrainState;

/// Descriptive bank order, not a claim about original member names/direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GearSoundBank {
    First,
    Second,
}

pub(super) struct GearSounds<'a, T> {
    vehicle: &'a mut VehicleDrivetrainState<T>,
}

impl<'a, T: AsRef<[u8]> + AsMut<[u8]>> GearSounds<'a, T> {
    pub(super) fn borrow(vehicle: &'a mut VehicleDrivetrainState<T>) -> Self {
        Self { vehicle }
    }

    pub(super) fn enabled(&self) -> Result<bool, VehicleGearSoundUnavailable> {
        Ok(byte(self.vehicle, 0xc68)? & 0x20 != 0)
    }

    pub(super) fn selected_bank(&self) -> Result<GearSoundBank, VehicleGearSoundUnavailable> {
        Ok(if byte(self.vehicle, 0x491)? != 0 {
            GearSoundBank::First
        } else {
            GearSoundBank::Second
        })
    }

    pub(super) fn current_sound(
        &self,
        bank: GearSoundBank,
    ) -> Result<u32, VehicleGearSoundUnavailable> {
        let base: u32 = match bank {
            GearSoundBank::First => 0x6a8,
            GearSoundBank::Second => 0x98c,
        };
        // Callbacks may replace the index. Preserve the original 32-bit address
        // arithmetic and reached bounds error, rather than clamping to 0 or 1.
        word(
            self.vehicle,
            base.wrapping_add(word(self.vehicle, 0x200)?.wrapping_mul(4)),
        )
    }

    pub(super) fn select_random_variant(
        &mut self,
        remainder: u32,
    ) -> Result<(), VehicleGearSoundUnavailable> {
        // Keep both writes and their order. The shared RNG has already advanced
        // when either write fails; this view must not roll that prefix back.
        store(self.vehicle, 0x200, remainder)?;
        store(self.vehicle, 0x200, u32::from(remainder > 79))
    }
}

fn byte<T: AsRef<[u8]> + AsMut<[u8]>>(
    state: &VehicleDrivetrainState<T>,
    offset: u32,
) -> Result<u8, VehicleGearSoundUnavailable> {
    state
        .as_bytes()
        .get(offset as usize)
        .copied()
        .ok_or(VehicleGearSoundUnavailable::Storage(offset))
}

pub(super) fn word<T: AsRef<[u8]> + AsMut<[u8]>>(
    state: &VehicleDrivetrainState<T>,
    offset: u32,
) -> Result<u32, VehicleGearSoundUnavailable> {
    let start = offset as usize;
    let end = start
        .checked_add(4)
        .ok_or(VehicleGearSoundUnavailable::Storage(offset))?;
    let bytes = state
        .as_bytes()
        .get(start..end)
        .ok_or(VehicleGearSoundUnavailable::Storage(offset))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

pub(super) fn store<T: AsRef<[u8]> + AsMut<[u8]>>(
    state: &mut VehicleDrivetrainState<T>,
    offset: u32,
    value: u32,
) -> Result<(), VehicleGearSoundUnavailable> {
    let start = offset as usize;
    let end = start
        .checked_add(4)
        .ok_or(VehicleGearSoundUnavailable::Storage(offset))?;
    state
        .as_bytes_mut()
        .get_mut(start..end)
        .ok_or(VehicleGearSoundUnavailable::Storage(offset))?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
