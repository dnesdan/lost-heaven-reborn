//! Live serialized parameter views at the existing vehicle compatibility edge.
//! Input/output buffers stay in CarSourceLoad. Reconstructed conversion and
//! application operate directly on those owners and the actual vehicle tail.
use super::*;
use crate::car_state_source::*;
use crate::vehicle_frame_backend::VehicleFrameError;

const PACKED: u32 = 0x0090_0000;
const PARAMETERS: u32 = 0x0088_0000;
type FrameError = VehicleFrameError<VehicleMemoryError>;

impl CarSourceReader for crate::human_actor::ActorDataReader<'_> {
    fn read_bounded(&mut self, destination: &mut [u8]) -> bool {
        crate::human_actor::ActorDataReader::read_bounded(self, destination)
    }
}
struct SourceMemory<'a> {
    frames: VehicleFrameBackend<'a, VehicleFrameMemory>,
    packed: Option<&'a CarDataBuffer>,
    parameters: &'a mut CarDataBuffer,
}
impl SourceMemory<'_> {
    fn record_read(&self, address: u32, width: usize) -> Option<Result<&[u8], FrameError>> {
        if let Some(offset) = address
            .checked_sub(PARAMETERS)
            .filter(|v| (*v as usize) < self.parameters.len())
        {
            return Some(self.parameters.read(offset as usize, width).ok_or_else(|| {
                VehicleFrameError::UnavailableGameEffect(format!(
                    "uninitialized car parameter bytes offset {offset:#x}, width {width}"
                ))
            }));
        }
        if let Some(packed) = self.packed {
            if let Some(offset) = address
                .checked_sub(PACKED)
                .filter(|v| (*v as usize) < packed.len())
            {
                return Some(packed.read(offset as usize, width).ok_or(
                    VehicleFrameError::UnavailableFrameMethod("uninitialized packed car bytes"),
                ));
            }
        }
        None
    }
}
impl VehicleInitBackend for SourceMemory<'_> {
    type Error = FrameError;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        if let Some(value) = self.record_read(address, 1) {
            return value.map(|v| v[0]);
        }
        self.frames.read_byte(address)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        if let Some(value) = self.record_read(address, 4) {
            return value.map(|v| u32::from_le_bytes(v.try_into().unwrap()));
        }
        self.frames.read_word(address)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        if let Some(offset) = address
            .checked_sub(PARAMETERS)
            .filter(|v| (*v as usize) < self.parameters.len())
        {
            return self
                .parameters
                .write(offset as usize, &[value])
                .then_some(())
                .ok_or(VehicleFrameError::Address(address));
        }
        self.frames.write_byte(address, value)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        if let Some(offset) = address
            .checked_sub(PARAMETERS)
            .filter(|v| (*v as usize) < self.parameters.len())
        {
            return self
                .parameters
                .write(offset as usize, &value.to_le_bytes())
                .then_some(())
                .ok_or(VehicleFrameError::Address(address));
        }
        self.frames.write_word(address, value)
    }
    fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
        self.frames.free(allocation)
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        self.frames.allocate(bytes)
    }
    fn collect_frame(
        &mut self,
        vector: &mut VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.frames.collect_frame(vector, frame)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.frames.enumerate(model, query, target)
    }
}
impl CarGameInitBackend for SourceMemory<'_> {
    fn game_init_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitEffect,
    ) -> Result<u32, Self::Error> {
        self.frames.game_init_effect(memory, effect)
    }
}
impl VehicleSourceParametersBackend for SourceMemory<'_> {
    fn parameter_copy_word(
        &mut self,
        mut memory: Option<VehicleComponentTailMemory<'_>>,
        destination: u32,
        source: u32,
    ) -> Result<(), Self::Error> {
        let mut captured = [(0, false); 4];
        // Native MOVSD loads a complete word before its store. Capture all four
        // source bytes/validity bits before any potentially overlapping write.
        for (offset, byte) in captured.iter_mut().enumerate() {
            let address = source.wrapping_add(offset as u32);
            *byte = if let Some(value) = memory
                .as_ref()
                .and_then(|memory| memory.projected_opaque_byte(address))
            {
                value
            } else if let Some(offset) = address
                .checked_sub(PARAMETERS)
                .filter(|offset| (*offset as usize) < self.parameters.len())
            {
                self.parameters
                    .opaque_byte(offset as usize)
                    .ok_or(VehicleFrameError::Address(address))?
            } else if let Some(packed) = self.packed {
                if let Some(offset) = address
                    .checked_sub(PACKED)
                    .filter(|offset| (*offset as usize) < packed.len())
                {
                    packed
                        .opaque_byte(offset as usize)
                        .ok_or(VehicleFrameError::Address(address))?
                } else {
                    (self.read_byte(address)?, true)
                }
            } else {
                (self.read_byte(address)?, true)
            };
        }
        if let Some(memory) = &mut memory {
            if memory.write_opaque_word(destination, captured).is_some() {
                return Ok(());
            }
        }
        if !captured.iter().all(|(_, known)| *known) {
            return Err(VehicleFrameError::UnavailableGameEffect(
                "opaque unknown word destination lacks actual validity storage".into(),
            ));
        }
        let value = u32::from_le_bytes(captured.map(|(value, _)| value));
        if let Some(memory) = &mut memory {
            memory.write_word_with_backend(self, destination, value)
        } else {
            self.write_word(destination, value)
        }
    }
    fn parameter_update_frame_world(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.frames.parameter_update_frame_world(frame)
    }
    fn parameter_frame_world_scale(&mut self, frame: u32) -> Result<[u32; 3], Self::Error> {
        self.frames.parameter_frame_world_scale(frame)
    }
    fn parameter_initialize_sounds(
        &mut self,
        memory: Option<VehicleComponentTailMemory<'_>>,
        vehicle: u32,
        source: u32,
    ) -> Result<(), Self::Error> {
        self.frames
            .parameter_initialize_sounds(memory, vehicle, source)
    }
}
struct SourceBackend<'a> {
    vehicle: &'a mut VehicleInitState,
    services: &'a mut CarActorServices,
    arena: &'a mut ModelArena,
    identity: u32,
    scratch: VehicleComponentTailScratch,
}
impl CompAllocationBackend for SourceBackend<'_> {
    type Error = String;
    fn allocate_decoded(
        &mut self,
        size: u32,
        _: CompPublication,
    ) -> Result<Option<CarDataBuffer>, Self::Error> {
        source_allocation(size)
    }
}
fn source_allocation(size: u32) -> Result<Option<CarDataBuffer>, String> {
    // A host resource limit is not the native allocator's null return.
    if size > 16 * 1024 * 1024 {
        return Err("car serialized allocation outside host domain".into());
    }
    Ok(Some(CarDataBuffer::uninitialized(size as usize)))
}
impl CarSourceBackend for SourceBackend<'_> {
    fn allocate_compressed(&mut self, size: u32) -> Result<Option<CarDataBuffer>, Self::Error> {
        source_allocation(size)
    }
    fn free_buffer(&mut self, _: CarDataAllocation, _: &CarDataBuffer) -> Result<(), Self::Error> {
        // The actual Rust serialized-buffer allocation is released by its one
        // CarSourceLoad owner immediately after this successful free boundary.
        Ok(())
    }
    fn convert_parameters(
        &mut self,
        packed: &CarDataBuffer,
        parameters: &mut CarDataBuffer,
    ) -> Result<(), Self::Error> {
        let memory = self
            .vehicle
            .car_live_memory(&mut self.scratch)
            .ok_or("car conversion storage unavailable")?;
        let mut backend = SourceMemory {
            frames: VehicleFrameBackend::new(self.arena, &mut self.services.memory)
                .with_vehicle_tables(&mut self.services.tables),
            packed: Some(packed),
            parameters,
        };
        car_convert_packed_vehicle_parameters(
            &mut backend,
            memory,
            self.identity,
            PACKED,
            PARAMETERS,
            false,
        )
        .map_err(|e| format!("car packed conversion: {e:?}"))
    }
    fn apply_parameters(&mut self, parameters: &mut CarDataBuffer) -> Result<bool, Self::Error> {
        let mut backend = SourceMemory {
            frames: VehicleFrameBackend::new(self.arena, &mut self.services.memory)
                .with_vehicle_tables(&mut self.services.tables),
            packed: None,
            parameters,
        };
        self.vehicle
            .init_car_parameters(&mut backend, PARAMETERS, 0)
            .map_err(|e| format!("car source parameters: {e:?}"))
    }
}
impl CarActorState {
    pub(crate) fn load_source(
        &mut self,
        services: &mut CarActorServices,
        arena: &mut ModelArena,
        reader: &mut crate::human_actor::ActorDataReader<'_>,
    ) -> Result<bool, String> {
        services.with_precision(|services| self.load_source_inner(services, arena, reader))
    }
    fn load_source_inner(
        &mut self,
        services: &mut CarActorServices,
        arena: &mut ModelArena,
        reader: &mut crate::human_actor::ActorDataReader<'_>,
    ) -> Result<bool, String> {
        let scratch = self.storage().scratch;
        if self.source.as_ref().is_some_and(|source| {
            matches!(
                source.stage(),
                CarSourceStage::Complete | CarSourceStage::Rejected
            )
        }) {
            // Each successful/native-false call has fresh caller storage.
            // Interrupted calls retain their exact owners and cannot replay.
            self.source = None;
        }
        let source = self
            .source
            .get_or_insert_with(|| CarSourceLoad::new(CarDataBuffer::uninitialized(4388)));
        let mut backend = SourceBackend {
            vehicle: self.vehicle.as_mut().ok_or("car constructor incomplete")?,
            services,
            arena,
            identity: self.identity,
            scratch,
        };
        source
            .load(reader, &mut backend)
            .map_err(|e| format!("car LoadData: {e:?}"))
    }
}

#[cfg(test)]
mod validity_tests {
    use super::*;
    fn state() -> (VehicleInitState, VehicleComponentTailScratch) {
        (
            VehicleInitState::from_car_bytes(
                0x10000,
                vec![0; 0x221c],
                CarInitScratch {
                    address: 0x800000,
                    bytes: [0; CAR_INIT_STACK_BYTES],
                },
                CarInitRegisters {
                    ebx: 0,
                    ebp: 0,
                    esi: 0,
                    edi: 0,
                },
            )
            .unwrap(),
            VehicleComponentTailScratch {
                address: 0x900000,
                bytes: [0; VEHICLE_INIT_STACK_BYTES],
            },
        )
    }
    #[test]
    fn opaque_movsd_captures_all_source_bytes_before_partial_overlap_store() {
        let (mut state, mut scratch) = state();
        let mut arena = ModelArena::default();
        let mut host = VehicleFrameMemory::default();
        let mut parameters = CarDataBuffer::uninitialized(4388);
        let mut backend = SourceMemory {
            frames: VehicleFrameBackend::new(&mut arena, &mut host),
            packed: None,
            parameters: &mut parameters,
        };
        let mut memory = state.car_live_memory(&mut scratch).unwrap();
        let source = 0x10070 + 0x10;
        memory
            .write_opaque_word(source, [(10, true), (20, false), (30, true), (40, false)])
            .unwrap();
        backend
            .parameter_copy_word(Some(memory.reborrow()), source + 1, source)
            .unwrap();
        assert_eq!(
            (0..5)
                .map(|offset| memory.projected_opaque_byte(source + offset).unwrap())
                .collect::<Vec<_>>(),
            [(10, true), (10, true), (20, false), (30, true), (40, false)]
        );
        let error = memory
            .read_byte_with_backend(&mut backend, source + 2)
            .unwrap_err();
        assert_eq!(
            error,
            FrameError::from(VehicleUninitializedStorage {
                address: source + 2
            })
        );
    }
    #[test]
    fn source_mixed_word_copies_validity_but_numeric_read_remains_unavailable() {
        let (mut state, mut scratch) = state();
        let mut arena = ModelArena::default();
        let mut host = VehicleFrameMemory::default();
        let mut parameters = CarDataBuffer::uninitialized(4388);
        parameters.write(0x4fc, &[0x11, 0x22]);
        let mut backend = SourceMemory {
            frames: VehicleFrameBackend::new(&mut arena, &mut host),
            packed: None,
            parameters: &mut parameters,
        };
        assert!(backend.read_word(PARAMETERS + 0x4fc).is_err());
        let destination = 0x10070 + 0xe6c + 0x4fc;
        let mut memory = state.car_live_memory(&mut scratch).unwrap();
        backend
            .parameter_copy_word(Some(memory.reborrow()), destination, PARAMETERS + 0x4fc)
            .unwrap();
        assert_eq!(
            memory.read_byte_with_backend(&mut backend, destination + 1),
            Ok(0x22)
        );
        assert_eq!(
            memory
                .read_word_with_backend(&mut backend, destination)
                .unwrap_err(),
            FrameError::from(VehicleUninitializedStorage {
                address: destination + 2
            })
        );
        assert!(backend.read_word(PARAMETERS + 0x4fc).is_err());
        assert_eq!(
            backend.parameters.read(0x4fc, 2),
            Some([0x11, 0x22].as_slice())
        );
        assert!(backend.parameters.read(0x4fc, 4).is_none());
    }
}
