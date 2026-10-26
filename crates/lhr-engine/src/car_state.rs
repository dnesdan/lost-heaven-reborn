//! Canonical C_car bytes and the original Init caller frame.
use super::*;

/// ESP after C_car::Init's four saved registers. Includes its 0x258 locals,
/// saved registers, return address and model argument; input bytes are retained.
pub const CAR_INIT_STACK_BYTES: usize = 0x270;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarInitScratch {
    pub address: u32,
    pub bytes: [u8; CAR_INIT_STACK_BYTES],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarInitRegisters {
    pub ebx: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct CarInitOwned {
    pub address: u32,
    pub prefix: [u8; 0x70],
    pub scratch: CarInitScratch,
    pub registers: CarInitRegisters,
    pub post_vehicle_ready: bool,
}
pub(super) struct CarInitMemory<'a> {
    address: u32,
    prefix: &'a mut [u8; 0x70],
    scratch: &'a mut CarInitScratch,
}
impl CarInitOwned {
    pub(super) fn memory(&mut self) -> CarInitMemory<'_> {
        CarInitMemory {
            address: self.address,
            prefix: &mut self.prefix,
            scratch: &mut self.scratch,
        }
    }
}
impl CarInitMemory<'_> {
    pub(super) fn reborrow(&mut self) -> CarInitMemory<'_> {
        CarInitMemory {
            address: self.address,
            prefix: self.prefix,
            scratch: self.scratch,
        }
    }
    pub(super) fn region(&self, address: u32, size: usize) -> Option<&[u8]> {
        if let Some(offset) = address.checked_sub(self.address) {
            if let Some(bytes) = self
                .prefix
                .get(offset as usize..(offset as usize).checked_add(size)?)
            {
                return Some(bytes);
            }
        }
        let offset = address.checked_sub(self.scratch.address)? as usize;
        self.scratch.bytes.get(offset..offset.checked_add(size)?)
    }
    pub(super) fn region_mut(&mut self, address: u32, size: usize) -> Option<&mut [u8]> {
        if let Some(offset) = address.checked_sub(self.address) {
            let offset = offset as usize;
            if offset.checked_add(size)? <= self.prefix.len() {
                return self.prefix.get_mut(offset..offset + size);
            }
        }
        let offset = address.checked_sub(self.scratch.address)? as usize;
        self.scratch
            .bytes
            .get_mut(offset..offset.checked_add(size)?)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarInitStorageError {
    CarSpan,
    StackSpan,
    Overlap,
}
impl VehicleInitState {
    /// Takes caller-supplied constructor state. No original constructor or
    /// destructor runs here. The embedded Vehicle and all later car fields have
    /// exactly one owner, this state's existing byte projection at car+0x70.
    pub fn from_car_bytes(
        car: u32,
        bytes: Vec<u8>,
        scratch: CarInitScratch,
        registers: CarInitRegisters,
    ) -> Result<Self, CarInitStorageError> {
        let length = u32::try_from(bytes.len()).map_err(|_| CarInitStorageError::CarSpan)?;
        // Init reaches car+0x21d8; its explicit destructor also owns the
        // three-vector headers through car+0x2200 (last word ends at 0x2204).
        let car_end = car
            .checked_add(length)
            .ok_or(CarInitStorageError::CarSpan)?;
        if length < 0x2204 {
            return Err(CarInitStorageError::CarSpan);
        }
        let end = scratch
            .address
            .checked_add(CAR_INIT_STACK_BYTES as u32)
            .ok_or(CarInitStorageError::StackSpan)?;
        if car < end && scratch.address < car_end {
            return Err(CarInitStorageError::Overlap);
        }
        let prefix = bytes[..0x70].try_into().unwrap();
        let mut state = Self::from_bytes(bytes[0x70..].to_vec());
        state.car = Some(CarInitOwned {
            address: car,
            prefix,
            scratch,
            registers,
            post_vehicle_ready: false,
        });
        Ok(state)
    }
    pub fn car_bytes(&self) -> Option<Vec<u8>> {
        let car = self.car.as_ref()?;
        let mut bytes = Vec::with_capacity(0x70 + self.bytes.len());
        bytes.extend_from_slice(&car.prefix);
        bytes.extend_from_slice(&self.bytes);
        Some(bytes)
    }
    pub fn car_live_memory<'a>(
        &'a mut self,
        scratch: &'a mut VehicleComponentTailScratch,
    ) -> Option<VehicleComponentTailMemory<'a>> {
        let address = self.car.as_ref()?.address;
        Some(VehicleComponentTailMemory {
            car: self.car.as_mut().map(CarInitOwned::memory),
            vehicle_address: address + 0x70,
            vehicle: &mut self.bytes,
            scratch,
        })
    }
    pub fn car_init_scratch(&self) -> Option<&CarInitScratch> {
        self.car.as_ref().map(|car| &car.scratch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(car: u32, length: usize, stack: u32) -> Result<VehicleInitState, CarInitStorageError> {
        VehicleInitState::from_car_bytes(
            car,
            vec![0xa5; length],
            CarInitScratch {
                address: stack,
                bytes: [0x7d; CAR_INIT_STACK_BYTES],
            },
            CarInitRegisters {
                ebx: 1,
                ebp: 2,
                esi: 3,
                edi: 4,
            },
        )
    }
    #[test]
    fn canonical_intake_admits_complete_destructor_headers_and_checks_all_spans() {
        assert_eq!(
            state(0x10000, 0x2203, 0x80000),
            Err(CarInitStorageError::CarSpan)
        );
        assert_eq!(
            state(u32::MAX - 0x2200, 0x2204, 0x80000),
            Err(CarInitStorageError::CarSpan)
        );
        assert_eq!(
            state(0x10000, 0x2204, u32::MAX - 0x100),
            Err(CarInitStorageError::StackSpan)
        );
        assert_eq!(
            state(0x10000, 0x2204, 0x12000),
            Err(CarInitStorageError::Overlap)
        );
        let value = state(0x10000, 0x2370, 0x80000).unwrap();
        assert_eq!(value.car_bytes(), Some(vec![0xa5; 0x2370]));
        assert_eq!(
            value.car_init_scratch().unwrap().bytes,
            [0x7d; CAR_INIT_STACK_BYTES]
        );
    }

    #[test]
    fn relocated_vehicle_identity_and_reborrow_write_through_to_the_same_car() {
        for address in [0x10000, 0x1234_0000] {
            let mut car = state(address, 0x2204, 0x80000).unwrap();
            let mut scratch = VehicleComponentTailScratch {
                address: 0x90000,
                bytes: [0x7d; VEHICLE_INIT_STACK_BYTES],
            };
            let mut expected = car.car_bytes().unwrap();
            let power = 0.75f32.to_bits();
            {
                let mut memory = car.car_live_memory(&mut scratch).unwrap();
                assert_eq!(memory.vehicle_address(), address + 0x70);
                {
                    let mut borrowed = memory.reborrow();
                    let vehicle = borrowed.vehicle_address();
                    assert_eq!(borrowed.write_word(vehicle + 0x3a4, power), Some(()));
                }
                assert_eq!(memory.read_word(address + 0x70 + 0x3a4), Some(power));
            }
            expected[0x70 + 0x3a4..0x70 + 0x3a8].copy_from_slice(&power.to_le_bytes());
            assert_eq!(car.car_bytes().unwrap(), expected);
            assert_eq!(scratch.bytes, [0x7d; VEHICLE_INIT_STACK_BYTES]);
        }
    }
}
