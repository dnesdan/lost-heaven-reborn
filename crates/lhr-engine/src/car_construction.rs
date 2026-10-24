//! Original C_car construction over allocator supplied, unconstructed bytes.
//! The constructor, Init and explicit destructor share one car allocation.
use super::*;
use crate::vehicle_owner::VehicleOwnerState;

pub const CAR_CONSTRUCTION_STACK_BYTES: usize = 0x24;

/// Active constructor frame, beginning at entry ESP minus 0x20. The byte at
/// offset 0xf is untouched caller storage, copied into three member markers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarConstructionScratch {
    pub address: u32,
    pub bytes: [u8; CAR_CONSTRUCTION_STACK_BYTES],
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const CAR: u32 = 0xa123_0000;
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Read(u32),
        Write(u32, u32),
        Allocate(u32),
        Free(u32),
    }
    #[derive(Clone)]
    struct Backend {
        words: BTreeMap<u32, u32>,
        events: Vec<Event>,
        allocation: u32,
        fail: Option<usize>,
        allocation_observation: Option<Vec<u8>>,
    }
    impl Backend {
        fn new(begin: u32, values: &[u32], spare: u32) -> Self {
            let mut words = BTreeMap::from([
                (0x63d2bc, begin),
                (0x63d2c0, begin + values.len() as u32 * 4),
                (0x63d2c4, begin + (values.len() as u32 + spare) * 4),
                (0x63d2c8, u32::MAX),
                (0x63a200, 0xffff_fffe),
            ]);
            for (index, value) in values.iter().enumerate() {
                words.insert(begin + index as u32 * 4, *value);
            }
            Self {
                words,
                events: Vec::new(),
                allocation: 0x60000,
                fail: None,
                allocation_observation: None,
            }
        }
        fn event(&mut self, event: Event) -> Result<(), ()> {
            if self.fail == Some(self.events.len()) {
                return Err(());
            }
            self.events.push(event);
            Ok(())
        }
    }
    impl CarConstructionBackend for Backend {
        type Error = ();
        fn read_word(&mut self, address: u32) -> Result<u32, ()> {
            self.event(Event::Read(address))?;
            self.words.get(&address).copied().ok_or(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), ()> {
            self.event(Event::Write(address, value))?;
            self.words.insert(address, value);
            Ok(())
        }
        fn allocate_car_construction(
            &mut self,
            memory: CarConstructionMemory<'_>,
            bytes: u32,
        ) -> Result<u32, ()> {
            self.event(Event::Allocate(bytes))?;
            self.allocation_observation = Some(memory.car_bytes());
            Ok(self.allocation)
        }
        fn free_car_construction(
            &mut self,
            memory: CarConstructionMemory<'_>,
            allocation: u32,
        ) -> Result<(), ()> {
            assert_eq!(
                memory.region(CAR, 4),
                Some(0x623248u32.to_le_bytes().as_slice())
            );
            self.event(Event::Free(allocation))
        }
    }
    fn seed() -> CarConstructionState {
        CarConstructionState::from_allocator_bytes(
            CAR,
            (0..0x221c).map(|i| (i * 37 + 9) as u8).collect(),
            CarConstructionScratch {
                address: 0x80000,
                bytes: [0x7d; CAR_CONSTRUCTION_STACK_BYTES],
            },
            CarConstructionRegisters {
                ebx: 0x1234,
                ebp: 0x2345,
                esi: 0x3456,
                edi: 0x4567,
                seh: 0x5678,
            },
            CarInitScratch {
                address: 0x90000,
                bytes: [0xc3; CAR_INIT_STACK_BYTES],
            },
            CarInitRegisters {
                ebx: 1,
                ebp: 2,
                esi: 3,
                edi: 4,
            },
        )
        .unwrap()
    }
    fn word(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }
    #[test]
    fn constructor_preserves_allocator_padding_and_transfers_one_canonical_owner() {
        let mut state = seed();
        let before = state.car_bytes();
        let mut backend = Backend::new(0, &[], 0);
        assert_eq!(state.construct(&mut backend), Ok(CAR));
        let bytes = state.car_bytes();
        assert_eq!(word(&bytes, 0), 0x623618);
        assert_eq!(word(&bytes, 0x70), 0x6236b0);
        assert_eq!(word(&bytes, 0x10), 4);
        assert_eq!(word(&bytes, 0xce4), CAR);
        assert_eq!(&bytes[0xc..0x10], &[1, 0, before[0xe], before[0xf]]);
        for offset in [0x21b0, 0x21c8, 0x21f4] {
            assert_eq!(bytes[offset], 0x7d);
            assert_eq!(
                &bytes[offset + 1..offset + 4],
                &before[offset + 1..offset + 4]
            );
            assert_eq!(&bytes[offset + 4..offset + 16], &[0; 12]);
        }
        assert_eq!(bytes[0x70 + 0xd8], 0xa1);
        for offset in [0x18, 0x1c, 0x6c, 0x215c, 0x220d, 0x2214, 0x221b] {
            assert_eq!(bytes[offset], before[offset]);
        }
        assert_eq!(backend.words[&0x63d2c8], 0);
        let next = 0xffff_fffeu32.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        assert_eq!(backend.words[&0x63a200], next);
        assert_eq!(word(&bytes, 0x2158), ((next >> 16) & 0x7fff) % 5 + 5);
        assert_eq!(word(&state.scratch.bytes, 0), 0x4567);
        assert_eq!(word(&state.scratch.bytes, 4), 0x3456);
        assert_eq!(word(&state.scratch.bytes, 8), 0x1234);
        assert_eq!(word(&state.scratch.bytes, 0x10), CAR);
        assert_eq!(word(&state.scratch.bytes, 0x1c), 0);
        assert_eq!(word(&state.scratch.bytes, 0x20), 0x7d7d7d7d);
        let car = state.into_car().unwrap();
        assert_eq!(car.car_bytes(), Some(bytes));
        assert_eq!(
            car.car_init_scratch().unwrap().bytes,
            [0xc3; CAR_INIT_STACK_BYTES]
        );
    }
    #[test]
    fn registration_reverse_deduplicates_appends_and_grows_in_native_publication_order() {
        for (values, spare, expected_allocations) in [
            (vec![CAR, 0x3333, CAR], 0, 0),
            (vec![0x1111, 0x2222], 1, 0),
            (vec![0x1111, 0x2222], 0, 1),
        ] {
            let mut state = seed();
            let mut backend = Backend::new(0x50000, &values, spare);
            state.construct(&mut backend).unwrap();
            assert_eq!(
                backend
                    .events
                    .iter()
                    .filter(|e| matches!(e, Event::Allocate(_)))
                    .count(),
                expected_allocations
            );
            if values.contains(&CAR) {
                assert_eq!(
                    &backend.events[4..6],
                    &[Event::Read(0x50008), Event::Read(0x63a200)]
                );
                assert_eq!(backend.words[&0x63d2c0], 0x5000c);
            } else if spare != 0 {
                assert_eq!(backend.words[&0x50008], CAR);
                assert_eq!(backend.words[&0x63d2c0], 0x5000c);
            } else {
                assert_eq!(backend.words[&0x60000], 0x1111);
                assert_eq!(backend.words[&0x60004], 0x2222);
                assert_eq!(backend.words[&0x60008], CAR);
                assert_eq!(backend.words[&0x63d2c4], 0x60010);
                let free = backend
                    .events
                    .iter()
                    .position(|e| *e == Event::Free(0x50000))
                    .unwrap();
                assert_eq!(backend.events[free + 1], Event::Write(0x63d2c4, 0x60010));
                assert_eq!(
                    backend.allocation_observation.as_ref().map(|b| word(b, 0)),
                    Some(0x623248)
                );
            }
        }
    }
    #[test]
    fn empty_null_allocation_obeys_pointer_helper_null_store_guards() {
        let mut state = seed();
        let mut backend = Backend::new(0, &[], 0);
        backend.allocation = 0;
        state.construct(&mut backend).unwrap();
        assert_eq!(backend.words[&0x63d2bc], 0);
        assert_eq!(backend.words[&0x63d2c0], 4);
        assert_eq!(backend.words[&0x63d2c4], 4);
        assert!(!backend
            .events
            .iter()
            .any(|e| matches!(e, Event::Write(0, _))));
    }
    #[test]
    fn every_external_failure_preserves_completed_effects_and_blocks_reentry() {
        let original = Backend::new(0x50000, &[0x1111, 0x2222], 0);
        let mut completed = original.clone();
        seed().construct(&mut completed).unwrap();
        for fail in 0..completed.events.len() {
            let mut state = seed();
            let mut backend = original.clone();
            backend.fail = Some(fail);
            assert_eq!(
                state.construct(&mut backend),
                Err(CarConstructionUnavailable::Effect(()))
            );
            assert_eq!(backend.events, completed.events[..fail]);
            let mut expected = original.words.clone();
            for event in &backend.events {
                if let Event::Write(address, value) = event {
                    expected.insert(*address, *value);
                }
            }
            assert_eq!(backend.words, expected);
            assert_ne!(state.checkpoint(), CarConstructionCheckpoint::Complete);
            assert_eq!(
                state.construct(&mut backend),
                Err(CarConstructionUnavailable::AlreadyStarted)
            );
            assert!(state.into_car().is_err());
        }
    }
    #[test]
    fn next_init_call_reuses_dead_constructor_stack_with_fresh_caller_bytes() {
        let mut state = seed();
        state.construct(&mut Backend::new(0, &[], 0)).unwrap();
        let completed = state.car_bytes();
        let address = state.scratch().address;
        let incoming = CarInitScratch {
            address,
            bytes: [0x6e; CAR_INIT_STACK_BYTES],
        };
        let registers = CarInitRegisters {
            ebx: 0x11,
            ebp: 0x22,
            esi: 0x33,
            edi: 0x44,
        };
        let car = state
            .into_car_with_init_frame(incoming.clone(), registers)
            .unwrap();
        assert_eq!(car.car_bytes(), Some(completed));
        assert_eq!(car.car_init_scratch(), Some(&incoming));
        assert_eq!(car.car.as_ref().unwrap().registers, registers);
    }
    #[test]
    fn registry_allocation_alias_uses_live_bytes_across_base_vehicle_boundary() {
        let mut state = seed();
        let mut backend = Backend::new(0x50000, &[0x11223344], 0);
        backend.allocation = CAR + 0x6f;
        state.construct(&mut backend).unwrap();
        let bytes = state.car_bytes();
        assert_eq!(bytes[0x6f], 0x44);
        assert_eq!(&bytes[0x74..0x77], &[0x00, 0x23, 0xa1]);
        assert!(!backend.events.iter().any(|event| matches!(event,Event::Write(address,_) if *address >= CAR && *address < CAR+0x221c)));
        let mut memory = state.memory();
        assert_eq!(memory.write_word(CAR + 0x6f, 0xaabbccdd), Ok(true));
        assert_eq!(memory.read_word(CAR + 0x6f), Ok(Some(0xaabbccdd)));
        let last = memory.read_byte(CAR + 0x221b);
        assert_eq!(
            memory.write_word(CAR + 0x221b, 0),
            Err(CarConstructionPartialSpan)
        );
        assert_eq!(memory.read_byte(CAR + 0x221b), last);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarConstructionRegisters {
    pub ebx: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,
    pub seh: u32,
}

/// Live canonical storage available to allocator effects. An effect must retain
/// its completed prefix when reporting unavailability.
pub struct CarConstructionMemory<'a> {
    state: &'a mut VehicleInitState,
    scratch: &'a mut CarConstructionScratch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarConstructionPartialSpan;
impl CarConstructionMemory<'_> {
    pub fn read_byte(&self, address: u32) -> Option<u8> {
        self.region(address, 1).map(|bytes| bytes[0])
    }
    /// None means wholly external. A partly mapped word must never fall back
    /// to an external mirror; prefix/embedded boundary words are fully mapped.
    pub fn read_word(&self, address: u32) -> Result<Option<u32>, CarConstructionPartialSpan> {
        let bytes = std::array::from_fn::<_, 4, _>(|index| {
            self.read_byte(address.wrapping_add(index as u32))
        });
        if bytes.iter().all(Option::is_none) {
            return Ok(None);
        }
        let [Some(a), Some(b), Some(c), Some(d)] = bytes else {
            return Err(CarConstructionPartialSpan);
        };
        Ok(Some(u32::from_le_bytes([a, b, c, d])))
    }
    /// False means wholly external; a partially mapped word is unapplied.
    pub fn write_word(
        &mut self,
        address: u32,
        value: u32,
    ) -> Result<bool, CarConstructionPartialSpan> {
        if self.read_word(address)?.is_none() {
            return Ok(false);
        }
        for (index, value) in value.to_le_bytes().into_iter().enumerate() {
            self.region_mut(address.wrapping_add(index as u32), 1)
                .unwrap()[0] = value;
        }
        Ok(true)
    }
    pub fn car_address(&self) -> u32 {
        self.state.car.as_ref().unwrap().address
    }
    pub fn car_bytes(&self) -> Vec<u8> {
        self.state.car_bytes().unwrap()
    }
    pub fn region(&self, address: u32, size: usize) -> Option<&[u8]> {
        let car = self.state.car.as_ref()?;
        if let Some(offset) = address.checked_sub(car.address) {
            let offset = offset as usize;
            if offset < 0x70 {
                return car.prefix.get(offset..offset.checked_add(size)?);
            }
            if let Some(bytes) = self
                .state
                .bytes
                .get(offset.checked_sub(0x70)?..offset.checked_sub(0x70)?.checked_add(size)?)
            {
                return Some(bytes);
            }
        }
        let offset = address.checked_sub(self.scratch.address)? as usize;
        self.scratch.bytes.get(offset..offset.checked_add(size)?)
    }
    pub fn region_mut(&mut self, address: u32, size: usize) -> Option<&mut [u8]> {
        let car = self.state.car.as_mut()?;
        if let Some(offset) = address.checked_sub(car.address) {
            let offset = offset as usize;
            if offset < 0x70 {
                return car.prefix.get_mut(offset..offset.checked_add(size)?);
            }
            let start = offset.checked_sub(0x70)?;
            if start.checked_add(size)? <= self.state.bytes.len() {
                return self.state.bytes.get_mut(start..start + size);
            }
        }
        let offset = address.checked_sub(self.scratch.address)? as usize;
        self.scratch
            .bytes
            .get_mut(offset..offset.checked_add(size)?)
    }
}

/// External Game globals and pointer-vector backing. These are the same raw
/// registry and allocator later used by Init and destruction, never ActorWorld
/// copies. Local car and active stack stores belong to the canonical projection.
pub trait CarConstructionBackend {
    type Error;
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error>;
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error>;
    fn allocate_car_construction(
        &mut self,
        memory: CarConstructionMemory<'_>,
        bytes: u32,
    ) -> Result<u32, Self::Error>;
    fn free_car_construction(
        &mut self,
        memory: CarConstructionMemory<'_>,
        allocation: u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarConstructionCheckpoint {
    Unstarted,
    BaseActor,
    StaticRegistration,
    Vehicle,
    Derived,
    Random,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarConstructionUnavailable<E> {
    Effect(E),
    AlreadyStarted,
    /// Bounded host admission for a malformed or oversized native header.
    RegistryDomain,
    PartialSpan(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarConstructionStorageError {
    CarSpan,
    ConstructorStackSpan,
    Init(CarInitStorageError),
    Overlap,
}

/// A stopped constructor owns all completed effects but cannot enter Init.
/// Dropping it invokes no original cleanup; explicit recovery remains a caller
/// responsibility. No constructor can be replayed over retained ownership.
#[derive(Debug, PartialEq, Eq)]
pub struct CarConstructionState {
    state: VehicleInitState,
    scratch: CarConstructionScratch,
    registers: CarConstructionRegisters,
    checkpoint: CarConstructionCheckpoint,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CarConstructionHandoffError {
    pub state: CarConstructionState,
    /// None means construction has not completed.
    pub storage: Option<CarInitStorageError>,
}

impl CarConstructionState {
    fn read_external<B: CarConstructionBackend>(
        &mut self,
        backend: &mut B,
        address: u32,
    ) -> Result<u32, CarConstructionUnavailable<B::Error>> {
        match self
            .memory()
            .read_word(address)
            .map_err(|_| CarConstructionUnavailable::PartialSpan(address))?
        {
            Some(value) => Ok(value),
            None => backend
                .read_word(address)
                .map_err(CarConstructionUnavailable::Effect),
        }
    }
    fn write_external<B: CarConstructionBackend>(
        &mut self,
        backend: &mut B,
        address: u32,
        value: u32,
    ) -> Result<(), CarConstructionUnavailable<B::Error>> {
        if self
            .memory()
            .write_word(address, value)
            .map_err(|_| CarConstructionUnavailable::PartialSpan(address))?
        {
            Ok(())
        } else {
            backend
                .write_word(address, value)
                .map_err(CarConstructionUnavailable::Effect)
        }
    }
    pub fn from_allocator_bytes(
        address: u32,
        bytes: Vec<u8>,
        scratch: CarConstructionScratch,
        registers: CarConstructionRegisters,
        init_scratch: CarInitScratch,
        init_registers: CarInitRegisters,
    ) -> Result<Self, CarConstructionStorageError> {
        let length =
            u32::try_from(bytes.len()).map_err(|_| CarConstructionStorageError::CarSpan)?;
        let end = address
            .checked_add(length)
            .ok_or(CarConstructionStorageError::CarSpan)?;
        // Last original store is the word at +0x2210. Additional allocator bytes
        // are preserved as supplied; no probe allocation size is imposed.
        if length < 0x2214 {
            return Err(CarConstructionStorageError::CarSpan);
        }
        let stack_end = scratch
            .address
            .checked_add(CAR_CONSTRUCTION_STACK_BYTES as u32)
            .ok_or(CarConstructionStorageError::ConstructorStackSpan)?;
        let init_end = init_scratch
            .address
            .checked_add(CAR_INIT_STACK_BYTES as u32)
            .ok_or(CarConstructionStorageError::Init(
                CarInitStorageError::StackSpan,
            ))?;
        if (address < stack_end && scratch.address < end)
            || (scratch.address < init_end && init_scratch.address < stack_end)
        {
            return Err(CarConstructionStorageError::Overlap);
        }
        let state = VehicleInitState::from_car_bytes(address, bytes, init_scratch, init_registers)
            .map_err(CarConstructionStorageError::Init)?;
        Ok(Self {
            state,
            scratch,
            registers,
            checkpoint: CarConstructionCheckpoint::Unstarted,
        })
    }
    pub fn checkpoint(&self) -> CarConstructionCheckpoint {
        self.checkpoint
    }
    pub fn car_bytes(&self) -> Vec<u8> {
        self.state.car_bytes().unwrap()
    }
    pub fn scratch(&self) -> &CarConstructionScratch {
        &self.scratch
    }
    pub fn memory(&mut self) -> CarConstructionMemory<'_> {
        CarConstructionMemory {
            state: &mut self.state,
            scratch: &mut self.scratch,
        }
    }
    /// Transfer the same allocation only after the original constructor returns.
    /// Failure returns the still-owned checkpoint without releasing anything.
    #[allow(clippy::result_large_err)]
    pub fn into_car(self) -> Result<VehicleInitState, Self> {
        if self.checkpoint == CarConstructionCheckpoint::Complete {
            Ok(self.state)
        } else {
            Err(self)
        }
    }
    /// Supply the next call's actual frame after construction returns. Its
    /// address may reuse the now-dead constructor frame. The old Init seed is
    /// replaced before the owner is exposed to Init; there are no live aliases.
    #[allow(clippy::result_large_err)]
    pub fn into_car_with_init_frame(
        mut self,
        scratch: CarInitScratch,
        registers: CarInitRegisters,
    ) -> Result<VehicleInitState, CarConstructionHandoffError> {
        if self.checkpoint != CarConstructionCheckpoint::Complete {
            return Err(CarConstructionHandoffError {
                state: self,
                storage: None,
            });
        }
        let end = match scratch.address.checked_add(CAR_INIT_STACK_BYTES as u32) {
            Some(end) => end,
            None => {
                return Err(CarConstructionHandoffError {
                    state: self,
                    storage: Some(CarInitStorageError::StackSpan),
                })
            }
        };
        let car = self.state.car.as_ref().unwrap();
        let car_end = car.address + 0x70 + self.state.bytes.len() as u32;
        if car.address < end && scratch.address < car_end {
            return Err(CarConstructionHandoffError {
                state: self,
                storage: Some(CarInitStorageError::Overlap),
            });
        }
        let car = self.state.car.as_mut().unwrap();
        car.scratch = scratch;
        car.registers = registers;
        Ok(self.state)
    }
    fn byte(&mut self, offset: usize, value: u8) {
        if offset < 0x70 {
            self.state.car.as_mut().unwrap().prefix[offset] = value;
        } else {
            self.state.bytes[offset - 0x70] = value;
        }
    }
    fn word(&mut self, offset: usize, value: u32) {
        for (index, value) in value.to_le_bytes().into_iter().enumerate() {
            self.byte(offset + index, value);
        }
    }
    fn stack_word(&mut self, offset: usize, value: u32) {
        self.scratch.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    pub fn construct<B: CarConstructionBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<u32, CarConstructionUnavailable<B::Error>> {
        if self.checkpoint != CarConstructionCheckpoint::Unstarted {
            return Err(CarConstructionUnavailable::AlreadyStarted);
        }
        let address = self.state.car.as_ref().unwrap().address;
        self.checkpoint = CarConstructionCheckpoint::BaseActor;
        self.stack_word(0x1c, u32::MAX);
        self.stack_word(0x18, 0x61db28);
        self.stack_word(0x14, self.registers.seh);
        self.stack_word(8, self.registers.ebx);
        self.stack_word(4, self.registers.esi);
        self.stack_word(0, self.registers.edi);
        self.stack_word(0x10, address);
        self.word(4, 0);
        self.byte(0xc, 1);
        self.byte(0xd, 0);
        self.word(0x10, 1);
        self.word(0x14, 1);
        self.byte(0x20, 0);
        self.word(0x60, 0);
        self.byte(0x66, 0);
        self.word(0x68, 0);
        self.word(0, 0x623248);
        self.word(8, 0);
        let count = self.read_external(backend, 0x63d2c8)?;
        self.write_external(backend, 0x63d2c8, count.wrapping_add(1))?;
        self.checkpoint = CarConstructionCheckpoint::StaticRegistration;
        self.register_static(backend, address)?;
        self.stack_word(0x1c, 0);
        self.checkpoint = CarConstructionCheckpoint::Vehicle;
        let mut vehicle = VehicleOwnerState::from_bytes(std::mem::take(&mut self.state.bytes));
        // Storage was admitted before any effects, so the embedded constructor
        // cannot encounter a local span failure. Moving the bytes duplicates no
        // native allocation or resource ownership.
        vehicle
            .construct(address + 0x70)
            .expect("admitted constructor span");
        self.state.bytes = vehicle.into_bytes();
        self.checkpoint = CarConstructionCheckpoint::Derived;
        let marker = self.scratch.bytes[0xf];
        for offset in [0x21b0, 0x21c8, 0x21f4] {
            self.byte(offset, marker);
            for index in 1..4 {
                self.word(offset + index * 4, 0);
            }
        }
        self.word(0x70, 0x6236b0);
        self.word(0, 0x623618);
        self.word(0x10, 4);
        self.word(0xce4, address);
        self.byte(0x217d, 0);
        self.byte(0x217e, 0);
        for index in 0..8 {
            self.word(0x218c + index * 4, 0);
        }
        self.word(0x21e0, 0);
        self.word(0x21e4, 0);
        self.byte(0x20b2, 0);
        self.word(0x212c, 0);
        self.checkpoint = CarConstructionCheckpoint::Random;
        let random = self.read_external(backend, 0x63a200)?;
        let random = random.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        self.write_external(backend, 0x63a200, random)?;
        let random = (random >> 16) & 0x7fff;
        self.byte(0x220c, 0);
        self.word(0x2210, 0);
        self.word(0x2158, random % 5 + 5);
        self.checkpoint = CarConstructionCheckpoint::Complete;
        Ok(address)
    }

    fn register_static<B: CarConstructionBackend>(
        &mut self,
        backend: &mut B,
        address: u32,
    ) -> Result<(), CarConstructionUnavailable<B::Error>> {
        use CarConstructionUnavailable::{Effect, RegistryDomain};
        let begin = self.read_external(backend, 0x63d2bc)?;
        let end = self.read_external(backend, 0x63d2c0)?;
        let span = end
            .checked_sub(begin)
            .filter(|span| span % 4 == 0 && *span / 4 <= 65_536)
            .ok_or(RegistryDomain)?;
        if begin != 0 {
            for index in (0..span / 4).rev() {
                if self.read_external(backend, begin + index * 4)? == address {
                    return Ok(());
                }
            }
        } else if end != 0 {
            return Err(RegistryDomain);
        }
        let capacity = self.read_external(backend, 0x63d2c4)?;
        let available = capacity
            .checked_sub(end)
            .filter(|span| span % 4 == 0)
            .ok_or(RegistryDomain)?;
        if available >= 4 {
            let current_end = self.read_external(backend, 0x63d2c0)?;
            if current_end != end {
                return Err(RegistryDomain);
            }
            if current_end != 0 {
                self.write_external(backend, current_end, address)?;
            }
            let current_end = self.read_external(backend, 0x63d2c0)?;
            if current_end != end {
                return Err(RegistryDomain);
            }
            let end = self.read_external(backend, 0x63d2c0)?;
            self.write_external(backend, 0x63d2c0, end.wrapping_add(4))?;
            return Ok(());
        }
        let count = span / 4;
        let capacity = count
            .checked_add(count.max(1))
            .filter(|n| *n <= 65_536)
            .ok_or(RegistryDomain)?;
        let allocation = backend
            .allocate_car_construction(self.memory(), capacity * 4)
            .map_err(Effect)?;
        // Native reloads the old begin after allocator callbacks, retaining the
        // original end as its copy boundary.
        let mut source = self.read_external(backend, 0x63d2bc)?;
        let mut destination = allocation;
        let mut copied = 0;
        while source != end {
            if copied >= 65_536 {
                return Err(RegistryDomain);
            }
            if destination != 0 {
                let value = self.read_external(backend, source)?;
                self.write_external(backend, destination, value)?;
            }
            source = source.wrapping_add(4);
            destination = destination.wrapping_add(4);
            copied += 1;
        }
        if destination != 0 {
            self.write_external(backend, destination, address)?;
        }
        destination = destination.wrapping_add(4);
        let current_end = self.read_external(backend, 0x63d2c0)?;
        let mut source = end;
        let mut copied = 0;
        while source != current_end {
            if copied >= 65_536 {
                return Err(RegistryDomain);
            }
            if destination != 0 {
                let value = self.read_external(backend, source)?;
                self.write_external(backend, destination, value)?;
            }
            source = source.wrapping_add(4);
            destination = destination.wrapping_add(4);
            copied += 1;
        }
        // Pointer element destruction is an actual empty helper.
        let _end = self.read_external(backend, 0x63d2c0)?;
        let _begin = self.read_external(backend, 0x63d2bc)?;
        let old = self.read_external(backend, 0x63d2bc)?;
        backend
            .free_car_construction(self.memory(), old)
            .map_err(Effect)?;
        self.write_external(backend, 0x63d2c4, allocation.wrapping_add(capacity * 4))?;
        let current_begin = self.read_external(backend, 0x63d2bc)?;
        let current_count = if current_begin == 0 {
            0
        } else {
            let current_end = self.read_external(backend, 0x63d2c0)?;
            ((current_end.wrapping_sub(current_begin) as i32) >> 2) as u32
        };
        self.write_external(backend, 0x63d2bc, allocation)?;
        self.write_external(
            backend,
            0x63d2c0,
            allocation
                .wrapping_add(current_count.wrapping_mul(4))
                .wrapping_add(4),
        )
    }
}
