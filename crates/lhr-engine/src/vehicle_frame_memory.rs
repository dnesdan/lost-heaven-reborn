//! Owned addressable storage for the vehicle projection. Allocations are never
//! reused, and bytes must be written before they can be read. This records no
//! invented values for native uninitialized allocation or stack bytes.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleMemoryError {
    Address(u32),
    Uninitialized(u32),
    Overlap(u32),
    Capacity,
    FrameAdapterRequired,
    IntrinsicAdapterRequired,
    CallbackRequired { function: u32, context: u32 },
}
struct Block {
    bytes: Vec<u8>,
    initialized: Vec<bool>,
    heap: bool,
}
/// Heap storage and explicitly supplied caller stack spans. Allocation failure
/// returns the native null pointer. Unsupported gameplay callbacks fail rather
/// than manufacturing collision registration or actor ownership.
pub struct VehicleFrameMemory {
    blocks: BTreeMap<u32, Block>,
    cursor: u32,
    limit: u32,
}
impl Default for VehicleFrameMemory {
    fn default() -> Self {
        Self {
            blocks: BTreeMap::new(),
            cursor: 0x0100_0000,
            limit: 0x1000_0000,
        }
    }
}
impl VehicleFrameMemory {
    pub fn map_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), VehicleMemoryError> {
        let end = address
            .checked_add(u32::try_from(bytes.len()).map_err(|_| VehicleMemoryError::Capacity)?)
            .ok_or(VehicleMemoryError::Address(address))?;
        if bytes.is_empty()
            || address == 0
            || address < FRAME_END && FRAME_BASE < end
            || address < FILENAME_END && PROPERTY_BASE < end
            || address < self.limit && self.cursor < end
        {
            return Err(VehicleMemoryError::Overlap(address));
        }
        if self
            .blocks
            .iter()
            .any(|(&base, block)| address < base + block.bytes.len() as u32 && base < end)
        {
            return Err(VehicleMemoryError::Overlap(address));
        }
        self.blocks.insert(
            address,
            Block {
                bytes: bytes.to_vec(),
                initialized: vec![true; bytes.len()],
                heap: false,
            },
        );
        Ok(())
    }
    pub fn allocations(&self) -> impl Iterator<Item = (u32, usize)> + '_ {
        self.blocks
            .iter()
            .filter(|(_, block)| block.heap)
            .map(|(&base, block)| (base, block.bytes.len()))
    }
    fn at(&self, address: u32) -> Result<(&Block, usize), VehicleMemoryError> {
        let (&base, block) = self
            .blocks
            .range(..=address)
            .next_back()
            .ok_or(VehicleMemoryError::Address(address))?;
        let offset = (address - base) as usize;
        if offset >= block.bytes.len() {
            return Err(VehicleMemoryError::Address(address));
        }
        Ok((block, offset))
    }
    fn at_mut(&mut self, address: u32) -> Result<(&mut Block, usize), VehicleMemoryError> {
        let (&base, block) = self
            .blocks
            .range_mut(..=address)
            .next_back()
            .ok_or(VehicleMemoryError::Address(address))?;
        let offset = (address - base) as usize;
        if offset >= block.bytes.len() {
            return Err(VehicleMemoryError::Address(address));
        }
        Ok((block, offset))
    }
}
impl VehicleInitBackend for VehicleFrameMemory {
    type Error = VehicleMemoryError;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        let (block, offset) = self.at(address)?;
        if !block.initialized[offset] {
            return Err(VehicleMemoryError::Uninitialized(address));
        }
        Ok(block.bytes[offset])
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        let mut out = [0; 4];
        for (i, value) in out.iter_mut().enumerate() {
            *value = self.read_byte(address.wrapping_add(i as u32))?;
        }
        Ok(u32::from_le_bytes(out))
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        let (block, offset) = self.at_mut(address)?;
        block.bytes[offset] = value;
        block.initialized[offset] = true;
        Ok(())
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        for (i, value) in value.to_le_bytes().into_iter().enumerate() {
            self.write_byte(address.wrapping_add(i as u32), value)?;
        }
        Ok(())
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        let size = bytes.max(1);
        let Some(end) = self.cursor.checked_add(size) else {
            return Ok(0);
        };
        let Some(next) = end.checked_add(3).map(|v| v & !3) else {
            return Ok(0);
        };
        if next > self.limit {
            return Ok(0);
        }
        let mut data = Vec::new();
        data.try_reserve_exact(size as usize)
            .map_err(|_| VehicleMemoryError::Capacity)?;
        data.resize(size as usize, 0);
        let mut initialized = Vec::new();
        initialized
            .try_reserve_exact(size as usize)
            .map_err(|_| VehicleMemoryError::Capacity)?;
        initialized.resize(size as usize, false);
        let address = self.cursor;
        self.cursor = next;
        self.blocks.insert(
            address,
            Block {
                bytes: data,
                initialized,
                heap: true,
            },
        );
        Ok(address)
    }
    fn free(&mut self, address: u32) -> Result<(), Self::Error> {
        if address == 0 {
            return Ok(());
        }
        if !self.blocks.get(&address).is_some_and(|block| block.heap) {
            return Err(VehicleMemoryError::Address(address));
        }
        self.blocks.remove(&address);
        Ok(())
    }
    fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::IntrinsicAdapterRequired)
    }
    fn enumerate(
        &mut self,
        _: u32,
        _: VehicleInitQuery,
        _: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        Err(VehicleInitUnavailable::Effect(
            VehicleMemoryError::FrameAdapterRequired,
        ))
    }
}
impl VehicleComponentsBackend for VehicleFrameMemory {
    fn prepare_frame_matrix(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn normalize_frame_quaternion(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn set_frame_rotation_matrix(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn insert_exhaust(&mut self, _: &mut [u8], _: u32, _: [u32; 6]) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::IntrinsicAdapterRequired)
    }
    fn insert_special_point(
        &mut self,
        _: &mut [u8],
        _: u32,
        _: [u32; 3],
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::IntrinsicAdapterRequired)
    }
    fn insert_special_control(
        &mut self,
        _: &mut [u8],
        _: u32,
        _: u32,
        _: u8,
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::IntrinsicAdapterRequired)
    }
    fn special_collision_callback(
        &mut self,
        function: u32,
        context: u32,
        _: u32,
        _: u32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::CallbackRequired { function, context })
    }
    fn link_frame(&mut self, _: u32, _: u32, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn remove_model_child(&mut self, _: u32, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn enumerate_components(
        &mut self,
        _: u32,
        _: VehicleComponentsQuery,
        _: &mut VehicleComponentsEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        Err(VehicleInitUnavailable::Effect(
            VehicleMemoryError::FrameAdapterRequired,
        ))
    }
    fn body_collision_callback(
        &mut self,
        _: &mut [u8],
        _: VehicleBodyCollisionContext,
        _: u32,
    ) -> Result<u32, Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn finish_body_collision_callbacks(
        &mut self,
        _: &mut [u8],
        _: VehicleBodyCollisionContext,
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn update_model_world_matrix(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn initialize_body_collision(
        &mut self,
        _: &mut [u8],
        _: u32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn parse_component_number(&mut self, _: u32) -> Result<i32, Self::Error> {
        Err(VehicleMemoryError::IntrinsicAdapterRequired)
    }
}
impl VehicleBodycolBackend for VehicleFrameMemory {
    fn bodycol_update_world(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn bodycol_link_frame(&mut self, _: u32, _: u32, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn bodycol_remove_model_child(&mut self, _: u32, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn bodycol_enumerate(
        &mut self,
        _: u32,
        _: &[u8],
        _: u32,
        _: &mut VehicleBodycolEnumeration<'_>,
    ) -> Result<(), VehicleBodycolUnavailable<Self::Error>> {
        Err(VehicleBodycolUnavailable::Effect(
            VehicleMemoryError::FrameAdapterRequired,
        ))
    }
}
impl VehicleComponentTailBackend for VehicleFrameMemory {
    fn materialize_frame_quaternion(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn find_window(&mut self, _: u32) -> Result<u32, Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn enumerate_tail(
        &mut self,
        _: u32,
        _: VehicleComponentTailQuery,
        _: &mut VehicleComponentTailEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        Err(VehicleInitUnavailable::Effect(
            VehicleMemoryError::FrameAdapterRequired,
        ))
    }
    fn seat_callback(
        &mut self,
        _: VehicleComponentTailMemory<'_>,
        function: u32,
        context: u32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::CallbackRequired { function, context })
    }
}

impl VehicleInitFinishBackend for VehicleFrameMemory {
    fn enumerate_init_centre(
        &mut self,
        _: u32,
        _: &mut VehicleInitCentreEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        Err(VehicleInitUnavailable::Effect(
            VehicleMemoryError::FrameAdapterRequired,
        ))
    }
    fn init_centre_callback(
        &mut self,
        _: VehicleComponentTailMemory<'_>,
        function: u32,
        context: u32,
        _: u32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::CallbackRequired { function, context })
    }
    fn update_init_model(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn update_init_model_bounds(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
}
impl VehicleDestructorBackend for VehicleFrameMemory {
    fn detach_vehicle_frame(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn release_vehicle_object(&mut self, object: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::Address(object))
    }
}

impl VehicleSourceParametersBackend for VehicleFrameMemory {
    fn parameter_update_frame_world(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn parameter_frame_world_scale(&mut self, _: u32) -> Result<[u32; 3], Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn parameter_initialize_sounds(
        &mut self,
        _: Option<VehicleComponentTailMemory<'_>>,
        _: u32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::IntrinsicAdapterRequired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifetime_rejects_uninitialized_stale_and_stack_free() {
        let mut memory = VehicleFrameMemory::default();
        let a = memory.allocate(7).unwrap();
        assert_eq!(
            memory.read_byte(a),
            Err(VehicleMemoryError::Uninitialized(a))
        );
        memory.write_word(a, 0x12345678).unwrap();
        assert_eq!(memory.read_word(a), Ok(0x12345678));
        assert_eq!(
            memory.read_byte(a + 4),
            Err(VehicleMemoryError::Uninitialized(a + 4))
        );
        memory.free(a).unwrap();
        assert_eq!(memory.read_word(a), Err(VehicleMemoryError::Address(a)));
        let b = memory.allocate(7).unwrap();
        assert_ne!(a, b);
        memory.map_bytes(0x1000, &[1, 2, 3]).unwrap();
        assert_eq!(
            memory.free(0x1000),
            Err(VehicleMemoryError::Address(0x1000))
        );
        assert_eq!(memory.read_byte(0x1000), Ok(1));
    }
}
