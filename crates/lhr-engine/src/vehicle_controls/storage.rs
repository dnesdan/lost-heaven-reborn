//! Live storage boundary for the recovered vehicle input setters.
//!
//! Names describe the fields' use by these setters, not recovered C++ member
//! names. Accesses stay lazy and write through to the single supplied owner;
//! constructing a view neither validates nor decodes the rest of the vehicle.
use super::{Result, VehicleControlUnavailable};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PowerRouting {
    Coupled,
    Independent,
    Unrecognized,
}

#[derive(Clone, Copy)]
pub(super) enum PowerChannel {
    Primary,
    Secondary,
}

/// Owned or borrowed vehicle storage. This does not construct a native vehicle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleControlState<T = Vec<u8>> {
    pub(super) bytes: T,
}

impl VehicleControlState {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl<'a> VehicleControlState<&'a mut [u8]> {
    /// Writes remain visible in the caller's owner even if a later access fails.
    pub fn from_bytes_mut(bytes: &'a mut [u8]) -> Self {
        Self { bytes }
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> VehicleControlState<T> {
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    fn byte(&self, offset: u32) -> Result<u8> {
        self.bytes
            .as_ref()
            .get(offset as usize)
            .copied()
            .ok_or(VehicleControlUnavailable::Storage(offset))
    }

    pub(super) fn word(&self, offset: u32) -> Result<u32> {
        let start = offset as usize;
        let end = start
            .checked_add(4)
            .ok_or(VehicleControlUnavailable::Storage(offset))?;
        let bytes = self
            .bytes
            .as_ref()
            .get(start..end)
            .ok_or(VehicleControlUnavailable::Storage(offset))?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    pub(super) fn float(&self, offset: u32) -> Result<f32> {
        Ok(f32::from_bits(self.word(offset)?))
    }

    pub(super) fn store(&mut self, offset: u32, value: u32) -> Result<()> {
        let start = offset as usize;
        let end = start
            .checked_add(4)
            .ok_or(VehicleControlUnavailable::Storage(offset))?;
        self.bytes
            .as_mut()
            .get_mut(start..end)
            .ok_or(VehicleControlUnavailable::Storage(offset))?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub(super) fn store_float(&mut self, offset: u32, value: f32) -> Result<()> {
        self.store(offset, value.to_bits())
    }

    pub(super) fn power_routing(&self) -> Result<PowerRouting> {
        Ok(match self.word(0xb8)? {
            0 | 2 => PowerRouting::Coupled,
            1 => PowerRouting::Independent,
            _ => PowerRouting::Unrecognized,
        })
    }

    pub(super) fn engine_enabled(&self) -> Result<bool> {
        Ok(self.byte(0xc2c)? != 0)
    }

    pub(super) fn reverse_routing_enabled(&self) -> Result<bool> {
        Ok(self.byte(0x4cc)? != 0)
    }

    pub(super) fn set_power_channel(&mut self, channel: PowerChannel, value: f32) -> Result<()> {
        self.store_float(
            match channel {
                PowerChannel::Primary => 0x3a4,
                PowerChannel::Secondary => 0x3a8,
            },
            value,
        )
    }

    pub(super) fn clear_motion_flag(&mut self) -> Result<()> {
        let flags = self.word(0x1a4)?;
        self.store(0x1a4, flags & 0xfeff_ffff)
    }

    pub(super) fn direct_brake_input(&self) -> Result<bool> {
        Ok(self.byte(0x4aa)? != 0)
    }

    pub(super) fn ramp_step(&self) -> Result<f32> {
        self.float(0x4c8)
    }

    pub(super) fn brake_ramp(&self) -> Result<f32> {
        self.float(0x3cc)
    }

    pub(super) fn set_brake_ramp(&mut self, value: f32) -> Result<()> {
        self.store_float(0x3cc, value)
    }

    pub(super) fn brake_ramp_limit(&self) -> Result<f32> {
        self.float(0x3c8)
    }

    pub(super) fn set_brake_input(&mut self, value: f32) -> Result<()> {
        self.store_float(0x5b8, value)
    }

    pub(super) fn shaped_clutch_input(&self) -> Result<bool> {
        Ok(self.byte(0x4ab)? != 0)
    }

    pub(super) fn clutch_curve(&self) -> Result<f32> {
        self.float(0x19c)
    }

    pub(super) fn set_clutch_target(&mut self, value: f32) -> Result<()> {
        self.store_float(0x5e0, value)
    }

    pub(super) fn shaped_steering_input(&self) -> Result<bool> {
        Ok(self.byte(0x4a8)? != 0)
    }

    pub(super) fn steering_curve(&self) -> Result<f32> {
        self.float(0x198)
    }

    pub(super) fn maximum_steer(&self) -> Result<f32> {
        self.float(0x610)
    }

    pub(super) fn set_steering_target(&mut self, value: f32) -> Result<()> {
        self.store_float(0x624, value)
    }

    pub(super) fn handbrake_ramp(&self) -> Result<f32> {
        self.float(0x3b8)
    }

    pub(super) fn set_handbrake_ramp(&mut self, value: f32) -> Result<()> {
        self.store_float(0x3b8, value)
    }

    pub(super) fn handbrake_ramp_limit(&self) -> Result<f32> {
        self.float(0x3b4)
    }

    pub(super) fn maximum_handbrake_force(&self) -> Result<f32> {
        self.float(0x3b0)
    }

    pub(super) fn set_handbrake_force(&mut self, value: f32) -> Result<()> {
        self.store_float(0x434, value)
    }

    pub(super) fn maximum_gear(&self) -> Result<i32> {
        Ok(self.word(0x568)? as i32)
    }

    pub(super) fn requested_gear(&self) -> Result<i32> {
        Ok(self.word(0x560)? as i32)
    }

    pub(super) fn current_gear(&self) -> Result<i32> {
        Ok(self.word(0x55c)? as i32)
    }

    pub(super) fn shift_restricted(&self) -> Result<bool> {
        Ok(self.word(0xc68)? & 0x4000 != 0)
    }

    // The condition's physical meaning is not established by these setters.
    pub(super) fn shift_condition(&self) -> Result<f32> {
        self.float(0x1ec)
    }

    pub(super) fn gear_ratio(&self, gear: i32) -> Result<f32> {
        let offset = 0x570u32.wrapping_add((gear as u32).wrapping_mul(4));
        self.float(offset)
    }

    pub(super) fn motor_rotation(&self) -> Result<f32> {
        self.float(0x548)
    }

    pub(super) fn maximum_motor_rotation(&self) -> Result<f32> {
        self.float(0x1f9c)
    }

    pub(super) fn request_gear_change(&mut self, gear: i32) -> Result<()> {
        let flags = self.word(0x5e8)?;
        self.store(0x494, 0)?;
        if flags & 0x20 != 0 {
            self.store(0x5e8, (flags & !1) | 2)?;
        }
        self.store(0x560, gear as u32)
    }
}
