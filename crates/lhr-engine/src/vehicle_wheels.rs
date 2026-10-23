//! Original wheel collection and the wheel portion of `C_Vehicle::Init`.
//! Native addresses denote backend storage, never Rust pointers.

use super::*;

/// Original collecting callback at PC RVA 0xd0dd0. The destination is an
/// address of a linked-list head, ordinarily vehicle+0xc34. Nodes retain the
/// enumeration order. Allocation failures are native callback results; they
/// do not roll back the already allocated node or imply host unavailability.
/// No frame reference is acquired by this callback.
pub fn vehicle_wheel_callback<B: VehicleInitBackend + ?Sized>(
    backend: &mut B,
    frame: u32,
    destination: u32,
) -> Result<u32, B::Error> {
    let node = backend.allocate(0x1d4)?;
    if node == 0 {
        return Ok(0);
    }
    backend.write_byte(node.wrapping_add(0x128), frame as u8)?;
    for offset in [0x12c, 0x130, 0x134, 0] {
        backend.write_word(node.wrapping_add(offset), 0)?;
    }
    backend.write_word(node.wrapping_add(4), frame)?;
    let pointer = backend.read_word(frame.wrapping_add(0x100))?;
    let mut source = if pointer == 0 { 0x63d2a0 } else { pointer };
    let mut remaining = u32::MAX;
    while remaining != 0 {
        let byte = backend.read_byte(source)?;
        source = source.wrapping_add(1);
        remaining -= 1;
        if byte == 0 {
            break;
        }
    }
    let name = backend.allocate(!remaining)?;
    backend.write_word(node, name)?;
    if name != 0 {
        // The allocator may have changed the frame's name pointer.
        let pointer = backend.read_word(frame.wrapping_add(0x100))?;
        source = if pointer == 0 { 0x63d2a0 } else { pointer };
        let mut output = name;
        loop {
            let byte = backend.read_byte(source)?;
            source = source.wrapping_add(1);
            backend.write_byte(output, byte)?;
            output = output.wrapping_add(1);
            if byte == 0 {
                break;
            }
        }
    }
    backend.write_word(node.wrapping_add(0x1d0), 0)?;
    let mut cursor = backend.read_word(destination)?;
    if cursor == 0 {
        backend.write_word(destination, node)?;
    } else {
        loop {
            let next = backend.read_word(cursor.wrapping_add(0x1d0))?;
            if next == 0 {
                break;
            }
            cursor = next;
        }
        backend.write_word(cursor.wrapping_add(0x1d0), node)?;
    }
    Ok(0)
}

/// Required original frame and CRT operations for wheel initialization.
pub trait VehicleWheelsBackend: VehicleInitBackend {
    /// Original CRT decimal conversion at RVA 0x210bf5.
    fn parse_wheel_number(&mut self, address: u32) -> Result<i32, Self::Error>;
    /// Model EnumFrames, mask 0x10021, pattern `WH*`, callback RVA 0xd0dd0.
    fn enumerate_wheels(
        &mut self,
        model: u32,
        target: &mut VehicleWheelEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
    /// Original CRT isalpha (RVA 0x210ca8), including the active locale.
    /// Its argument is the sign-extended native char, not an unsigned byte.
    fn wheel_name_letter(&mut self, character: i32) -> Result<bool, Self::Error>;
    /// LS3DF UpdateHRBoundVolW, called only if frame+0xac bit 0x100 is clear.
    fn update_wheel_world_bounds(&mut self, frame: u32) -> Result<(), Self::Error>;
}

pub struct VehicleWheelEnumeration<'a> {
    state: &'a mut VehicleInitState,
    base: u32,
}
impl VehicleWheelEnumeration<'_> {
    pub fn visit<B: VehicleInitBackend>(
        &mut self,
        frame: u32,
        backend: &mut B,
    ) -> Result<u32, VehicleInitUnavailable<B::Error>> {
        vehicle_wheel_callback(
            &mut self.state.tail_projection(backend, self.base),
            frame,
            self.base.wrapping_add(0xc34),
        )
        .map_err(VehicleInitUnavailable::Effect)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleWheelRejection {
    MissingWheels,
    InvalidWheelLetter,
    InvalidWheelParent,
    InvalidWheelCount,
    WheelOrderAllocation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleWheelsFrontier {
    /// Original VA 0x4bf5f9, immediately before C_Vehicle parameter setup.
    ParameterSetup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleWheelsOutcome {
    Rejected(VehicleWheelRejection),
    UnavailableFrontier(VehicleWheelsFrontier),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleWheelsUnavailable<E> {
    Tail(VehicleComponentTailUnavailable<E>),
    Init(VehicleInitUnavailable<E>),
    NoContinuation,
    NumericDomain,
}
impl<E> From<VehicleInitUnavailable<E>> for VehicleWheelsUnavailable<E> {
    fn from(value: VehicleInitUnavailable<E>) -> Self {
        Self::Init(value)
    }
}
impl<E> From<VehicleComponentTailUnavailable<E>> for VehicleWheelsUnavailable<E> {
    fn from(value: VehicleComponentTailUnavailable<E>) -> Self {
        Self::Tail(value)
    }
}
pub(super) type WheelsResult<B, T> =
    Result<T, VehicleWheelsUnavailable<<B as VehicleInitBackend>::Error>>;

impl VehicleInitState {
    /// Consumes the genuine post-SEAT continuation once. The original stack
    /// name buffer starts at stack+0x13c in the same live Init projection.
    /// This method retains each completed store/effect on host unavailability.
    pub fn continue_wheels<B: VehicleWheelsBackend>(
        &mut self,
        backend: &mut B,
    ) -> WheelsResult<B, VehicleWheelsOutcome> {
        if !std::mem::take(&mut self.wheel_continuation) {
            return Err(VehicleWheelsUnavailable::NoContinuation);
        }
        let base = self.tail_vehicle_address.unwrap();
        let model = self.load(0x34c)?;
        backend.enumerate_wheels(model, &mut VehicleWheelEnumeration { state: self, base })?;
        let outcome = self.initialize_wheels(backend);
        // The four temporary vectors belong to Init, not to the wheel nodes.
        // They survive an unavailable dependency and are released only at an
        // original rejection/return path.
        if matches!(outcome, Ok(VehicleWheelsOutcome::Rejected(_))) {
            self.release_init_vectors(backend)?;
        }
        outcome
    }

    pub(super) fn release_init_vectors<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
    ) -> WheelsResult<B, ()> {
        for offset in [0x48, 0x38, 0x58] {
            self.init_free(b, self.tail_vector(offset).begin)
                .map_err(VehicleInitUnavailable::Effect)?;
            self.tail_zero(offset + 4, 3);
        }
        self.init_free(b, self.tail_vector(0x68).begin)
            .map_err(VehicleInitUnavailable::Effect)?;
        self.component_scratch = Some(self.tail_vector(0x58));
        self.scratch = Some(self.tail_vector(0x68));
        Ok(())
    }

    pub(super) fn wheel_word<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        a: u32,
    ) -> WheelsResult<B, u32> {
        self.tail_read_word(b, a)
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    pub(super) fn wheel_byte<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        a: u32,
    ) -> WheelsResult<B, u8> {
        self.tail_read_byte(b, a)
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    pub(super) fn wheel_store<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        a: u32,
        v: u32,
    ) -> WheelsResult<B, ()> {
        self.tail_write_word(b, a, v)
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    fn wheel_buffer<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        suffix: &[u8],
    ) -> WheelsResult<B, u32> {
        let mut source = self.load(0x4bc)?;
        let buffer = self
            .tail_scratch
            .as_ref()
            .unwrap()
            .address
            .wrapping_add(0x13c);
        let mut output = buffer;
        loop {
            let byte = self.wheel_byte(b, source)?;
            source = source.wrapping_add(1);
            self.tail_projection(b, self.tail_vehicle_address.unwrap())
                .write_byte(output, byte)
                .map_err(VehicleInitUnavailable::Effect)?;
            output = output.wrapping_add(1);
            if byte == 0 {
                break;
            }
        }
        output = output.wrapping_sub(1);
        // Original two stores append five characters and their terminator.
        self.wheel_store(
            b,
            output,
            u32::from_le_bytes(suffix[..4].try_into().unwrap()),
        )?;
        for (index, byte) in suffix[4..].iter().enumerate() {
            self.tail_projection(b, self.tail_vehicle_address.unwrap())
                .write_byte(output.wrapping_add(4 + index as u32), *byte)
                .map_err(VehicleInitUnavailable::Effect)?;
        }
        Ok(buffer)
    }
    fn wheel_parent_matches<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        parent: u32,
        buffer: u32,
        length: u32,
    ) -> WheelsResult<B, bool> {
        let name = self.wheel_word(b, parent.wrapping_add(0x100))?;
        let name = if name == 0 { 0x63d2a0 } else { name };
        for offset in 0..length {
            let left = self.wheel_byte(b, name.wrapping_add(offset))?;
            let right = self.wheel_byte(b, buffer.wrapping_add(offset))?;
            if left != right {
                return Ok(false);
            }
            if left == 0 {
                return Ok(true);
            }
        }
        Ok(true)
    }
    fn initialize_wheels<B: VehicleWheelsBackend>(
        &mut self,
        b: &mut B,
    ) -> WheelsResult<B, VehicleWheelsOutcome> {
        use VehicleInitUnavailable::Effect;
        use VehicleWheelRejection::*;
        use VehicleWheelsOutcome::Rejected;
        let mut node = self.load::<B::Error>(0xc34)?;
        if self
            .bytes
            .get(0xc70)
            .copied()
            .ok_or(VehicleInitUnavailable::Storage(0xc70))?
            & 2
            == 0
            && node == 0
        {
            return Ok(Rejected(MissingWheels));
        }
        let mut count = 0u32;
        self.tail_store(0x1c, count);
        while node != 0 {
            let length = self.tail_name_length(b)?.wrapping_sub(1);
            let name = self.wheel_word(b, node)?;
            let letter = self.wheel_byte(b, name.wrapping_add(length).wrapping_add(3))?;
            if !b.wheel_name_letter(letter as i8 as i32).map_err(Effect)? {
                return Ok(Rejected(InvalidWheelLetter));
            }
            let frame = self.wheel_word(b, node.wrapping_add(4))?;
            let parent = self.wheel_word(b, frame.wrapping_add(0x120))?;
            let buffer = self.wheel_buffer(b, b".DWHR\0")?;
            let mut size = 0u32;
            while self.wheel_byte(b, buffer.wrapping_add(size))? != 0 {
                size = size.wrapping_add(1);
            }
            self.tail_store(0x20, size);
            if !self.wheel_parent_matches(b, parent, buffer, size)? {
                self.wheel_buffer(b, b".DWHL\0")?;
                if !self.wheel_parent_matches(b, parent, buffer, size)? {
                    return Ok(Rejected(InvalidWheelParent));
                }
            }
            let frame = self.wheel_word(b, node.wrapping_add(4))?;
            let parent = self.wheel_word(b, frame.wrapping_add(0x120))?;
            self.wheel_store(b, node.wrapping_add(0x120), 0)?;
            self.wheel_store(b, node.wrapping_add(8), parent)?;
            if self.wheel_word(b, frame.wrapping_add(0xac))? & 0x100 == 0 {
                b.update_wheel_world_bounds(frame).map_err(Effect)?;
            }
            let frame_again = self.wheel_word(b, node.wrapping_add(4))?;
            for i in 0..10 {
                let value = self.wheel_word(b, frame.wrapping_add(0xd8 + 4 * i))?;
                self.tail_store(0x8c + i as usize * 4, value);
            }
            let difference = |high: u32, low: u32| {
                crate::native_matrix::checked_f32_add(f32::from_bits(high), -f32::from_bits(low))
                    .map(f32::abs)
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)
            };
            let radius = crate::native_matrix::checked_f32_mul(
                difference(self.tail_word(0x9c), self.tail_word(0x90))?,
                0.5,
            )
            .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            self.wheel_store(b, node.wrapping_add(0x10c), radius.to_bits())?;
            let inner = crate::native_matrix::checked_f32_mul(radius, 0.8)
                .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            self.wheel_store(b, node.wrapping_add(0x110), inner.to_bits())?;
            let width = difference(self.tail_word(0x98), self.tail_word(0x8c))?;
            self.wheel_store(b, node.wrapping_add(0x138), width.to_bits())?;
            let position = self.wheel_word(b, frame_again.wrapping_add(0x80))?;
            self.wheel_store(b, node.wrapping_add(0xc), position)?;
            node = self.wheel_word(b, node.wrapping_add(0x1d0))?;
            count = count.wrapping_add(1);
            self.tail_store(0x1c, count);
        }
        if self.bytes[0xc70] & 2 == 0 && ((count as i32) < 4 || count & 1 != 0) {
            return Ok(Rejected(InvalidWheelCount));
        }
        self.store(0x4c4, count)?;
        self.store(0x4c0, ((count as i32) / 2) as u32)?;
        self.tail_store(0x1c, count.wrapping_mul(4));
        let order = self
            .init_allocate(b, count.wrapping_mul(4))
            .map_err(Effect)?;
        self.store(0xc38, order)?;
        if self.bytes[0xc70] & 2 == 0 && order == 0 {
            return Ok(Rejected(WheelOrderAllocation));
        }
        let suffix = self.tail_name_length(b)?.wrapping_add(2);
        let mut slot = 0i32;
        let mut axle = 0i32;
        self.tail_store(0x20, 0);
        while slot < self.load::<B::Error>(0x4c4)? as i32 {
            for &side in b"LR" {
                let mut found = self.load::<B::Error>(0xc34)?;
                while found != 0 {
                    let name = self.wheel_word(b, found)?;
                    let number = b
                        .parse_wheel_number(name.wrapping_add(suffix).wrapping_add(1))
                        .map_err(Effect)?;
                    if number == axle {
                        let name = self.wheel_word(b, found)?;
                        let letter = self.wheel_byte(b, name.wrapping_add(suffix))?;
                        if letter == side || letter == side.to_ascii_lowercase() {
                            break;
                        }
                    }
                    found = self.wheel_word(b, found.wrapping_add(0x1d0))?;
                }
                let array = self.load::<B::Error>(0xc38)?;
                self.wheel_store(b, array.wrapping_add((slot as u32).wrapping_mul(4)), found)?;
                slot = slot.wrapping_add(1);
            }
            axle = axle.wrapping_add(1);
            self.tail_store(0x20, axle as u32);
        }
        self.wheel_setup_ready = true;
        Ok(VehicleWheelsOutcome::UnavailableFrontier(
            VehicleWheelsFrontier::ParameterSetup,
        ))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) enum Effect {
        Allocate(u32),
        Read(u32, usize),
        Write(u32, Vec<u8>),
        Free(u32),
        Bounds(u32),
        Detach(u32),
        Release(u32),
        Find(u32, VehicleActorTarget),
        On(u32, bool),
    }
    #[derive(Clone, Default)]
    pub(crate) struct Memory {
        pub(crate) bytes: BTreeMap<u32, u8>,
        pub(crate) allocations: VecDeque<u32>,
        pub(crate) effects: Vec<Effect>,
        pub(crate) fail: Option<usize>,
        pub(crate) replacement_name: Option<(u32, u32)>,
        pub(crate) frames: Vec<u32>,
        pub(crate) projected_free_writes: Vec<(u32, u32)>,
    }
    impl Memory {
        pub(crate) fn effect(&mut self, effect: Effect) -> Result<(), ()> {
            if self.fail == Some(self.effects.len()) {
                return Err(());
            }
            self.effects.push(effect);
            Ok(())
        }
        pub(crate) fn put(&mut self, address: u32, bytes: &[u8]) {
            for (index, byte) in bytes.iter().copied().enumerate() {
                self.bytes.insert(address + index as u32, byte);
            }
        }
        pub(crate) fn word(&mut self, address: u32, value: u32) {
            self.put(address, &value.to_le_bytes());
        }
        pub(crate) fn get(&self, address: u32) -> u32 {
            u32::from_le_bytes(std::array::from_fn(|i| {
                *self.bytes.get(&(address + i as u32)).unwrap_or(&0xa5)
            }))
        }
    }
    impl VehicleInitBackend for Memory {
        type Error = ();
        fn read_byte(&mut self, address: u32) -> Result<u8, ()> {
            self.effect(Effect::Read(address, 1))?;
            self.bytes.get(&address).copied().ok_or(())
        }
        fn read_word(&mut self, address: u32) -> Result<u32, ()> {
            self.effect(Effect::Read(address, 4))?;
            Ok(self.get(address))
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), ()> {
            self.effect(Effect::Write(address, vec![value]))?;
            self.put(address, &[value]);
            Ok(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), ()> {
            self.effect(Effect::Write(address, value.to_le_bytes().to_vec()))?;
            self.word(address, value);
            Ok(())
        }
        fn allocate(&mut self, bytes: u32) -> Result<u32, ()> {
            self.effect(Effect::Allocate(bytes))?;
            if bytes != 0x1d4 {
                if let Some((frame, replacement)) = self.replacement_name.take() {
                    self.word(frame + 0x100, replacement);
                }
            }
            Ok(self.allocations.pop_front().unwrap_or(0))
        }
        fn free(&mut self, address: u32) -> Result<(), ()> {
            self.effect(Effect::Free(address))
        }
        fn free_projected(
            &mut self,
            memory: &mut VehicleComponentTailMemory<'_>,
            address: u32,
        ) -> Result<(), ()> {
            self.free(address)?;
            for (address, value) in &self.projected_free_writes {
                memory.write_word(*address, *value).ok_or(())?;
            }
            Ok(())
        }
        fn collect_frame(&mut self, v: &mut VehicleInitFrameVector, frame: u32) -> Result<(), ()> {
            if v.begin == 0 {
                v.begin = self.allocate(256)?;
                v.end = v.begin;
                v.capacity = v.begin + 256;
            }
            self.write_word(v.end, frame)?;
            v.end += 4;
            Ok(())
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<()>> {
            panic!("unused")
        }
    }
    impl VehicleWheelsBackend for Memory {
        fn parse_wheel_number(&mut self, address: u32) -> Result<i32, ()> {
            let mut address = address;
            let mut number = 0i32;
            loop {
                let byte = self.read_byte(address)?;
                if !byte.is_ascii_digit() {
                    return Ok(number);
                }
                number = number * 10 + (byte - b'0') as i32;
                address += 1;
            }
        }
        fn enumerate_wheels(
            &mut self,
            _: u32,
            target: &mut VehicleWheelEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<()>> {
            for frame in self.frames.clone() {
                target.visit(frame, self)?;
            }
            Ok(())
        }
        fn wheel_name_letter(&mut self, character: i32) -> Result<bool, ()> {
            Ok((character as u8).is_ascii_alphabetic())
        }
        fn update_wheel_world_bounds(&mut self, frame: u32) -> Result<(), ()> {
            self.effect(Effect::Bounds(frame))
        }
    }
    fn callback_memory() -> Memory {
        let mut memory = Memory {
            allocations: [0x5000, 0x6000].into(),
            ..Memory::default()
        };
        memory.word(0x2000 + 0x100, 0x3000);
        memory.put(0x3000, b"car.WHL0\0");
        memory.word(0x1000, 0x4000);
        memory.word(0x4000 + 0x1d0, 0);
        memory
    }
    #[test]
    fn collecting_callback_rereads_header_mutated_by_free_through_live_projection() {
        let mut state = VehicleInitState::from_bytes(vec![0; 0x2400]);
        state.tail_vehicle_address = Some(0x10000);
        state.tail_scratch = Some(VehicleComponentTailScratch {
            address: 0x8000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        });
        state.tail_store(0x6c, 0x3000);
        state.tail_store(0x70, 0x3004);
        state.tail_store(0x74, 0x3004);
        let mut backend = Memory {
            allocations: [0x6000].into(),
            projected_free_writes: vec![(0x8070, 0x300c)],
            ..Memory::default()
        };
        backend.word(0x3000, 0x2000);
        assert_eq!(
            backend.collect_frame_projected(state.live_tail_memory(), 0x8068, 0x2100),
            Ok(0)
        );
        assert_eq!(
            state.tail_vector(0x68),
            VehicleInitFrameVector {
                begin: 0x6000,
                end: 0x6010,
                capacity: 0x6008
            }
        );
        assert_eq!(backend.get(0x6000), 0x2000);
        assert_eq!(backend.get(0x6004), 0x2100);
        assert_eq!(state.tail_word(0x70), 0x6010);
    }

    #[test]
    fn callback_appends_without_clearing_uninitialized_node_fields() {
        let mut b = callback_memory();
        assert_eq!(vehicle_wheel_callback(&mut b, 0x2000, 0x1000), Ok(0));
        assert_eq!(b.get(0x1000), 0x4000);
        assert_eq!(b.get(0x4000 + 0x1d0), 0x5000);
        assert_eq!(b.get(0x5000), 0x6000);
        assert_eq!(b.get(0x5004), 0x2000);
        assert_eq!(b.get(0x5008), 0xa5a5_a5a5);
        assert_eq!(b.get(0x5128), 0xa5a5_a500);
        assert_eq!(b.get(0x51d0), 0);
        assert_eq!(b.get(0x6000), u32::from_le_bytes(*b"car."));
    }
    #[test]
    fn callback_null_allocations_have_distinct_native_ownership() {
        let mut absent = callback_memory();
        absent.allocations = [0].into();
        assert_eq!(vehicle_wheel_callback(&mut absent, 0x2000, 0x1000), Ok(0));
        assert_eq!(absent.effects, [Effect::Allocate(0x1d4)]);
        let mut nameless = callback_memory();
        nameless.allocations = [0x5000, 0].into();
        assert_eq!(vehicle_wheel_callback(&mut nameless, 0x2000, 0x1000), Ok(0));
        assert_eq!(nameless.get(0x5000), 0);
        assert_eq!(nameless.get(0x4000 + 0x1d0), 0x5000);
    }
    #[test]
    fn callback_rereads_name_after_allocator_and_preserves_every_unavailable_prefix() {
        let mut initial = callback_memory();
        initial.put(0x3500, b"X\0");
        initial.replacement_name = Some((0x2000, 0x3500));
        let mut baseline = initial.clone();
        vehicle_wheel_callback(&mut baseline, 0x2000, 0x1000).unwrap();
        assert_eq!(baseline.bytes[&0x6000], b'X');
        assert_eq!(baseline.bytes[&0x6001], 0);
        for failure in 0..baseline.effects.len() {
            let mut b = initial.clone();
            b.fail = Some(failure);
            assert_eq!(vehicle_wheel_callback(&mut b, 0x2000, 0x1000), Err(()));
            assert_eq!(b.effects, baseline.effects[..failure]);
            let mut expected = initial.clone();
            for effect in &b.effects {
                match effect {
                    Effect::Write(address, bytes) => expected.put(*address, bytes),
                    Effect::Allocate(size) if *size != 0x1d4 => expected.word(0x2100, 0x3500),
                    _ => {}
                }
            }
            assert_eq!(b.bytes, expected.bytes);
        }
    }
    fn wheel_state(names: &[&[u8]]) -> (VehicleInitState, Memory) {
        let mut state = VehicleInitState::from_bytes(vec![0; 0x2400]);
        state.tail_scratch = Some(VehicleComponentTailScratch {
            address: 0x8000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        });
        state.tail_vehicle_address = Some(0x10000);
        state.wheel_continuation = true;
        state.store::<()>(0x4bc, 0x3000).unwrap();
        state
            .store::<()>(0xc34, if names.is_empty() { 0 } else { 0x4000 })
            .unwrap();
        let mut b = Memory {
            allocations: [0x7000].into(),
            ..Memory::default()
        };
        b.put(0x3000, b"car\0");
        b.put(0x3200, b"car.DWHL0\0");
        for (i, name) in names.iter().enumerate() {
            let node = 0x4000 + i as u32 * 0x200;
            let frame = 0x1000 + i as u32 * 0x200;
            let name_address = 0x3400 + i as u32 * 32;
            b.word(node, name_address);
            b.put(name_address, name);
            b.word(node + 4, frame);
            b.word(
                node + 0x1d0,
                if i + 1 == names.len() {
                    0
                } else {
                    node + 0x200
                },
            );
            b.word(frame + 0x120, 0x2000);
            b.word(frame + 0xac, 0x100);
            b.word(frame + 0x80, (i as f32).to_bits());
            for (j, value) in [-0.2f32, -0.5, -0.5, 0.2, 0.5, 0.5, 0., 0., 0., 1.]
                .into_iter()
                .enumerate()
            {
                b.word(frame + 0xd8 + j as u32 * 4, value.to_bits());
            }
        }
        b.word(0x2100, 0x3200);
        (state, b)
    }
    #[test]
    fn wheel_geometry_and_order_keep_traversal_list_but_store_left_right_axles() {
        let (mut state, mut b) =
            wheel_state(&[b"car.WHR1\0", b"car.WHL0\0", b"car.WHR0\0", b"car.WHL1\0"]);
        assert_eq!(
            state.continue_wheels(&mut b),
            Ok(VehicleWheelsOutcome::UnavailableFrontier(
                VehicleWheelsFrontier::ParameterSetup
            ))
        );
        assert_eq!(state.load::<()>(0x4c4), Ok(4));
        assert_eq!(state.load::<()>(0x4c0), Ok(2));
        assert_eq!(
            (0..4).map(|i| b.get(0x7000 + i * 4)).collect::<Vec<_>>(),
            [0x4200, 0x4400, 0x4600, 0x4000]
        );
        assert_eq!(b.get(0x410c), 0.5f32.to_bits());
        assert_eq!(b.get(0x4110), 0.4f32.to_bits());
        assert_eq!(b.get(0x4138), 0.4f32.to_bits());
        assert_eq!(b.get(0x41d0), 0x4200);
        assert_eq!(
            state.continue_wheels(&mut b),
            Err(VehicleWheelsUnavailable::NoContinuation)
        );
    }
    #[test]
    fn rejected_count_frees_only_temporary_vectors_and_keeps_initialized_wheels() {
        let (mut state, mut b) = wheel_state(&[b"car.WHL0\0", b"car.WHR0\0"]);
        for (offset, begin) in [
            (0x48, 0x9000),
            (0x38, 0x9100),
            (0x58, 0x9200),
            (0x68, 0x9300),
        ] {
            state.tail_store(offset + 4, begin);
            state.tail_store(offset + 8, begin + 4);
            state.tail_store(offset + 12, begin + 16);
        }
        assert_eq!(
            state.continue_wheels(&mut b),
            Ok(VehicleWheelsOutcome::Rejected(
                VehicleWheelRejection::InvalidWheelCount
            ))
        );
        assert_eq!(
            b.effects
                .iter()
                .filter_map(|e| if let Effect::Free(a) = e {
                    Some(*a)
                } else {
                    None
                })
                .collect::<Vec<_>>(),
            [0x9000, 0x9100, 0x9200, 0x9300]
        );
        assert_eq!(b.get(0x410c), 0.5f32.to_bits());
        assert_eq!(state.load::<()>(0xc34), Ok(0x4000));
        assert_eq!(state.tail_vector(0x48).begin, 0);
        assert_eq!(state.tail_vector(0x68).begin, 0x9300);
    }
    #[test]
    fn absent_order_entries_are_native_null_stores_not_a_new_rejection() {
        let (mut state, mut b) =
            wheel_state(&[b"car.WHL0\0", b"car.WHL0\0", b"car.WHR2\0", b"car.WHR2\0"]);
        assert!(matches!(
            state.continue_wheels(&mut b),
            Ok(VehicleWheelsOutcome::UnavailableFrontier(_))
        ));
        assert_eq!(
            (0..4).map(|i| b.get(0x7000 + 4 * i)).collect::<Vec<_>>(),
            [0x4000, 0, 0, 0]
        );
    }
}
