//! Bounded DOOR/WINDOW, BALLAST and SEAT continuation of the existing Init.
//! Descriptive working names refer to inline instructions, not new functions.
use super::components::{matrix, MatrixSource, PointSource};
use super::*;
use crate::native_matrix::{checked_f32_add, checked_f32_mul};
use crate::vehicle_bodycol::{
    VehicleBodycolBackend, VehicleBodycolStorage, VehicleBodycolUnavailable,
};
use crate::vehicle_component_records::{
    vehicle_ballast_records_insert, vehicle_component_pointers_copy,
    vehicle_component_pointers_insert, vehicle_door_records_insert, vehicle_exhaust_records_insert,
    vehicle_seat_records_insert,
};

/// Original Init locals, four saved registers, and SEH registration bytes.
pub const VEHICLE_INIT_STACK_BYTES: usize = 0x1c8;

/// Native Init stack at ESP after saving its four registers. Reached original
/// stores overwrite these bytes; untouched automatic bytes remain caller input.
/// The projection includes its wheel-name buffer and exception registration.
/// This address is stable through the complete Init attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleComponentTailScratch {
    pub address: u32,
    pub bytes: [u8; VEHICLE_INIT_STACK_BYTES],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleComponentTailStorage {
    pub bodycol: VehicleBodycolStorage,
    pub scratch: VehicleComponentTailScratch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleComponentTailFrontier {
    /// Original VA 0x4bf1aa, before wheel enumeration. Init is incomplete.
    WheelEnumeration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleComponentTailOutcome {
    Rejected(VehicleInitRejection),
    UnavailableFrontier(VehicleComponentTailFrontier),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleComponentTailUnavailable<E> {
    Prefix(VehicleComponentsUnavailable<VehicleBodycolUnavailable<E>>),
    Tail(VehicleComponentsUnavailable<E>),
    NoContinuation,
    ProjectionStorage,
}
impl<E> From<VehicleComponentsUnavailable<E>> for VehicleComponentTailUnavailable<E> {
    fn from(value: VehicleComponentsUnavailable<E>) -> Self {
        Self::Tail(value)
    }
}
impl<E> From<VehicleInitUnavailable<E>> for VehicleComponentTailUnavailable<E> {
    fn from(value: VehicleInitUnavailable<E>) -> Self {
        Self::Tail(value.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleComponentTailQuery {
    Ballast,
    Seat,
}
impl VehicleComponentTailQuery {
    pub fn pattern(self) -> &'static [u8] {
        match self {
            Self::Ballast => b"BALLAST*",
            Self::Seat => b"SEAT*",
        }
    }
    pub fn mask(self) -> u32 {
        0x10020
    }
}
/// Required original virtual effects. Enumeration preserves native traversal
/// and callback returns. The optional seat callback receives live vehicle and
/// stack storage, so its stores survive failure and affect subsequent loads.
pub trait VehicleComponentTailBackend: VehicleComponentsBackend + VehicleBodycolBackend {
    /// Actual Matrix-to-Quaternion call at frame+0x9c, source frame+0x50.
    fn materialize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error>;
    /// Frame virtual +0x38, pattern WINDOW*, mask 0x10001; zero means absent.
    fn find_window(&mut self, door: u32) -> Result<u32, Self::Error>;
    fn enumerate_tail(
        &mut self,
        root: u32,
        query: VehicleComponentTailQuery,
        target: &mut VehicleComponentTailEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
    /// Indirect vehicle+0x304, ECX=context, EDX=frame, no stack arguments.
    fn seat_callback(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        function: u32,
        context: u32,
        frame: u32,
    ) -> Result<(), Self::Error>;
}

/// Live addressable portions of the native call's vehicle and stack. Other
/// addresses belong to the backend. This view performs no snapshot/copyback.
/// Use the backend-aware word methods when an address can cross that boundary.
pub struct VehicleComponentTailMemory<'a> {
    pub(super) car: Option<CarInitMemory<'a>>,
    pub(super) vehicle_address: u32,
    pub(super) vehicle: &'a mut [u8],
    pub(super) scratch: &'a mut VehicleComponentTailScratch,
}
impl VehicleComponentTailMemory<'_> {
    pub(crate) fn reborrow(&mut self) -> VehicleComponentTailMemory<'_> {
        VehicleComponentTailMemory {
            car: self.car.as_mut().map(CarInitMemory::reborrow),
            vehicle_address: self.vehicle_address,
            vehicle: self.vehicle,
            scratch: self.scratch,
        }
    }
    fn region(&self, address: u32, size: usize) -> Option<&[u8]> {
        if let Some(car) = &self.car {
            if let Some(bytes) = car.region(address, size) {
                return Some(bytes);
            }
        }
        if let Some(offset) = address.checked_sub(self.vehicle_address) {
            if let Some(bytes) = self
                .vehicle
                .get(offset as usize..(offset as usize).checked_add(size)?)
            {
                return Some(bytes);
            }
        }
        let offset = address.checked_sub(self.scratch.address)? as usize;
        self.scratch.bytes.get(offset..offset.checked_add(size)?)
    }
    fn region_mut(&mut self, address: u32, size: usize) -> Option<&mut [u8]> {
        if let Some(car) = &mut self.car {
            if car.region(address, size).is_some() {
                return car.region_mut(address, size);
            }
        }
        if let Some(offset) = address.checked_sub(self.vehicle_address) {
            let start = offset as usize;
            if start.checked_add(size)? <= self.vehicle.len() {
                return self.vehicle.get_mut(start..start + size);
            }
        }
        let offset = address.checked_sub(self.scratch.address)? as usize;
        self.scratch
            .bytes
            .get_mut(offset..offset.checked_add(size)?)
    }
    /// Reads only fully projected words, including adjacent vehicle/stack bytes.
    /// `None` means at least one byte belongs to external storage.
    pub fn read_word(&self, address: u32) -> Option<u32> {
        let mut bytes = [0; 4];
        for (offset, byte) in bytes.iter_mut().enumerate() {
            *byte = self.read_byte(address.wrapping_add(offset as u32))?;
        }
        Some(u32::from_le_bytes(bytes))
    }
    pub fn read_byte(&self, address: u32) -> Option<u8> {
        Some(self.region(address, 1)?[0])
    }
    /// Writes only fully projected words. On `None`, no byte is changed; callers
    /// must use `write_word_with_backend` to handle mixed ownership.
    pub fn write_word(&mut self, address: u32, value: u32) -> Option<()> {
        self.read_word(address)?;
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.write_byte(address.wrapping_add(offset as u32), byte)?;
        }
        Some(())
    }
    pub fn write_byte(&mut self, address: u32, value: u8) -> Option<()> {
        self.region_mut(address, 1)?[0] = value;
        Some(())
    }

    pub fn read_byte_with_backend<B: VehicleInitBackend + ?Sized>(
        &self,
        backend: &mut B,
        address: u32,
    ) -> Result<u8, B::Error> {
        match self.read_byte(address) {
            Some(value) => Ok(value),
            None => backend.read_byte(address),
        }
    }
    pub fn write_byte_with_backend<B: VehicleInitBackend + ?Sized>(
        &mut self,
        backend: &mut B,
        address: u32,
        value: u8,
    ) -> Result<(), B::Error> {
        match self.write_byte(address, value) {
            Some(()) => Ok(()),
            None => backend.write_byte(address, value),
        }
    }

    fn touches_projection(&self, address: u32) -> bool {
        (0..4).any(|offset| self.read_byte(address.wrapping_add(offset)).is_some())
    }

    /// Keeps a wholly external read as one backend word operation. Mixed words
    /// resolve each byte through its live owner, in increasing byte order.
    pub fn read_word_with_backend<B: VehicleInitBackend + ?Sized>(
        &self,
        backend: &mut B,
        address: u32,
    ) -> Result<u32, B::Error> {
        if let Some(value) = self.read_word(address) {
            return Ok(value);
        }
        if !self.touches_projection(address) {
            return backend.read_word(address);
        }
        let mut bytes = [0; 4];
        for (offset, byte) in bytes.iter_mut().enumerate() {
            let address = address.wrapping_add(offset as u32);
            *byte = match self.read_byte(address) {
                Some(value) => value,
                None => backend.read_byte(address)?,
            };
        }
        Ok(u32::from_le_bytes(bytes))
    }

    /// Keeps wholly external writes as backend word operations. Mixed writes
    /// resolve bytes in increasing order and retain completed stores if a host
    /// effect is unavailable. This does not model hardware page-fault atomicity.
    pub fn write_word_with_backend<B: VehicleInitBackend + ?Sized>(
        &mut self,
        backend: &mut B,
        address: u32,
        value: u32,
    ) -> Result<(), B::Error> {
        if self.write_word(address, value).is_some() {
            return Ok(());
        }
        if !self.touches_projection(address) {
            return backend.write_word(address, value);
        }
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            let address = address.wrapping_add(offset as u32);
            if self.write_byte(address, byte).is_none() {
                backend.write_byte(address, byte)?;
            }
        }
        Ok(())
    }
}
pub(super) struct Projection<'a, B: ?Sized> {
    pub(super) backend: &'a mut B,
    pub(super) memory: VehicleComponentTailMemory<'a>,
}
impl<B: VehicleInitBackend + ?Sized> VehicleInitBackend for Projection<'_, B> {
    type Error = B::Error;
    fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
        match self.memory.read_byte(a) {
            Some(v) => Ok(v),
            None => self.backend.read_byte(a),
        }
    }
    fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
        self.memory.read_word_with_backend(self.backend, a)
    }
    fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
        match self.memory.write_byte(a, v) {
            Some(()) => Ok(()),
            None => self.backend.write_byte(a, v),
        }
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
        self.memory.write_word_with_backend(self.backend, a, v)
    }
    fn free(&mut self, a: u32) -> Result<(), Self::Error> {
        self.backend.free_projected(&mut self.memory, a)
    }
    fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
        self.backend.allocate_projected(&mut self.memory, n)
    }
    fn collect_frame(&mut self, v: &mut VehicleInitFrameVector, f: u32) -> Result<(), Self::Error> {
        self.backend.collect_frame(v, f)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.backend.enumerate(model, query, target)
    }
}
pub(super) fn insert_exhaust_projection<B: VehicleInitBackend>(
    backend: &mut B,
    vehicle: &mut [u8],
    vehicle_address: u32,
    scratch: &mut VehicleComponentTailScratch,
    insertion: u32,
    value: [u32; 6],
) -> Result<(), B::Error> {
    for (index, word) in value.into_iter().enumerate() {
        stack_store(scratch, 0x8c + index * 4, word);
    }
    let source = scratch.address.wrapping_add(0x8c);
    vehicle_exhaust_records_insert(
        &mut Projection {
            backend,
            memory: VehicleComponentTailMemory {
                car: None,
                vehicle_address,
                vehicle,
                scratch,
            },
        },
        vehicle_address.wrapping_add(0x450),
        insertion,
        source,
    )
    .map(|_| ())
}
fn stack_word(s: &VehicleComponentTailScratch, offset: usize) -> u32 {
    u32::from_le_bytes(s.bytes[offset..offset + 4].try_into().unwrap())
}
fn stack_store(s: &mut VehicleComponentTailScratch, offset: usize, value: u32) {
    s.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn vector(s: &VehicleComponentTailScratch, offset: usize) -> VehicleInitFrameVector {
    VehicleInitFrameVector {
        begin: stack_word(s, offset + 4),
        end: stack_word(s, offset + 8),
        capacity: stack_word(s, offset + 12),
    }
}
fn put_vector(s: &mut VehicleComponentTailScratch, offset: usize, v: VehicleInitFrameVector) {
    stack_store(s, offset + 4, v.begin);
    stack_store(s, offset + 8, v.end);
    stack_store(s, offset + 12, v.capacity);
}
fn count(v: VehicleInitFrameVector) -> u32 {
    if v.begin == 0 {
        0
    } else {
        ((v.end.wrapping_sub(v.begin) as i32) >> 2) as u32
    }
}
pub struct VehicleComponentTailEnumeration<'a> {
    car: Option<CarInitMemory<'a>>,
    scratch: &'a mut VehicleComponentTailScratch,
    vehicle: &'a mut [u8],
    vehicle_address: u32,
    offset: usize,
}
impl VehicleComponentTailEnumeration<'_> {
    pub fn visit<B: VehicleInitBackend + ?Sized>(
        &mut self,
        frame: u32,
        backend: &mut B,
    ) -> Result<u32, VehicleInitUnavailable<B::Error>> {
        let header = self.scratch.address.wrapping_add(self.offset as u32);
        backend.collect_frame_projected(
            VehicleComponentTailMemory {
                car: self.car.as_mut().map(CarInitMemory::reborrow),
                vehicle_address: self.vehicle_address,
                vehicle: self.vehicle,
                scratch: self.scratch,
            },
            header,
            frame,
        )
    }
}

type TailResult<B, T> =
    Result<T, VehicleComponentTailUnavailable<<B as VehicleInitBackend>::Error>>;
impl VehicleInitState {
    /// Runs the established prefix with the actual BODYCOL adapter, then this
    /// bounded tail with actual record helpers. No result admits a usable car.
    pub fn init_component_tail<B: VehicleComponentTailBackend>(
        &mut self,
        backend: &mut B,
        model: u32,
        mut storage: VehicleComponentTailStorage,
    ) -> TailResult<B, VehicleComponentTailOutcome> {
        if self.started {
            return Err(VehicleComponentTailUnavailable::Prefix(
                VehicleComponentsUnavailable::Init(VehicleInitUnavailable::AlreadyStarted),
            ));
        }
        let car_entry = self.car_caller_started;
        if car_entry && !std::mem::take(&mut self.car_caller_ready) {
            return Err(VehicleComponentTailUnavailable::NoContinuation);
        }
        self.validate_tail_storage::<B>(&storage)?;
        if car_entry {
            // Actual saved registers at the C_car callsite, before Init's
            // first body instruction. Other untouched stack bytes stay input.
            for (index, value) in [
                0,
                storage.bodycol.vehicle_address,
                storage.bodycol.vehicle_address.wrapping_sub(0x70),
                model,
            ]
            .into_iter()
            .enumerate()
            {
                storage.scratch.bytes[index * 4..index * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        self.tail_scratch = Some(storage.scratch.clone());
        self.tail_vehicle_address = Some(storage.bodycol.vehicle_address);
        let outcome = self.init_components(
            &mut VehicleBodycolComponents::with_exhaust_records(
                backend,
                storage.bodycol,
                &mut storage.scratch,
            ),
            model,
        );
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => return Err(VehicleComponentTailUnavailable::Prefix(error)),
        };
        match outcome {
            VehicleComponentsOutcome::Rejected(r) => Ok(VehicleComponentTailOutcome::Rejected(r)),
            VehicleComponentsOutcome::UnavailableFrontier(_) => {
                storage.scratch = self.tail_scratch.take().unwrap();
                self.continue_component_tail(backend, storage)
            }
        }
    }
    /// Consumes only the capability produced by this state's real component
    /// prefix. Even invalid mappings and unavailable effects consume it.
    pub fn continue_component_tail<B: VehicleComponentTailBackend>(
        &mut self,
        backend: &mut B,
        storage: VehicleComponentTailStorage,
    ) -> TailResult<B, VehicleComponentTailOutcome> {
        let frontier = self
            .component_tail
            .take()
            .ok_or(VehicleComponentTailUnavailable::NoContinuation)?;
        self.validate_tail_storage::<B>(&storage)?;
        let base = storage.bodycol.vehicle_address;
        self.tail_vehicle_address = Some(base);
        self.tail_scratch = Some(storage.scratch);
        let scratch = self.tail_scratch.as_mut().unwrap();
        put_vector(scratch, 0x58, self.component_scratch.unwrap());
        put_vector(scratch, 0x68, self.scratch.unwrap());
        let result = self.run_component_tail(backend, base, frontier);
        let scratch = self.tail_scratch.as_ref().unwrap();
        self.component_scratch = Some(vector(scratch, 0x58));
        self.scratch = Some(vector(scratch, 0x68));
        result
    }
    pub(super) fn validate_tail_storage<B: VehicleInitBackend>(
        &self,
        storage: &VehicleComponentTailStorage,
    ) -> TailResult<B, ()> {
        let base = storage.bodycol.vehicle_address;
        let object = self.car.as_ref().map_or((base, self.bytes.len()), |car| {
            (car.address, self.bytes.len() + 0x70)
        });
        let mut spans = vec![
            object,
            (storage.scratch.address, storage.scratch.bytes.len()),
            (storage.bodycol.callback_scratch.record, 116),
            (storage.bodycol.descendant_scratch, 60),
        ];
        if let Some(car) = &self.car {
            spans.push((car.scratch.address, car.scratch.bytes.len()));
        }
        for (i, (a, n)) in spans.iter().copied().enumerate() {
            let end = a
                .checked_add(
                    u32::try_from(n)
                        .map_err(|_| VehicleComponentTailUnavailable::ProjectionStorage)?,
                )
                .ok_or(VehicleComponentTailUnavailable::ProjectionStorage)?;
            for (b, m) in spans[..i].iter().copied() {
                let other_end = b
                    .checked_add(m as u32)
                    .ok_or(VehicleComponentTailUnavailable::ProjectionStorage)?;
                if a < other_end && b < end {
                    return Err(VehicleComponentTailUnavailable::ProjectionStorage);
                }
            }
        }
        Ok(())
    }
    pub(crate) fn live_tail_memory(&mut self) -> VehicleComponentTailMemory<'_> {
        VehicleComponentTailMemory {
            car: self.car.as_mut().map(CarInitOwned::memory),
            vehicle_address: self.tail_vehicle_address.unwrap(),
            vehicle: &mut self.bytes,
            scratch: self.tail_scratch.as_mut().unwrap(),
        }
    }
    pub fn component_tail_scratch(&self) -> Option<&VehicleComponentTailScratch> {
        self.tail_scratch.as_ref()
    }
    pub(super) fn tail_word(&self, offset: usize) -> u32 {
        stack_word(self.tail_scratch.as_ref().unwrap(), offset)
    }
    pub(super) fn tail_store(&mut self, offset: usize, value: u32) {
        stack_store(self.tail_scratch.as_mut().unwrap(), offset, value);
    }
    pub(super) fn tail_vector(&self, offset: usize) -> VehicleInitFrameVector {
        vector(self.tail_scratch.as_ref().unwrap(), offset)
    }
    pub(super) fn tail_zero(&mut self, offset: usize, words: usize) {
        for i in 0..words {
            self.tail_store(offset + i * 4, 0);
        }
    }
    // Pointer-bearing fields are mutable through the seat callback. Every
    // subsequent raw access resolves again: an indirect address may now name
    // vehicle or stack storage instead of an external frame or allocation.
    pub(super) fn tail_read_word<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        address: u32,
    ) -> Result<u32, B::Error> {
        self.tail_projection(b, self.tail_vehicle_address.unwrap())
            .read_word(address)
    }
    pub(super) fn tail_read_byte<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        address: u32,
    ) -> Result<u8, B::Error> {
        self.tail_projection(b, self.tail_vehicle_address.unwrap())
            .read_byte(address)
    }
    pub(super) fn tail_write_word<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        address: u32,
        value: u32,
    ) -> Result<(), B::Error> {
        self.tail_projection(b, self.tail_vehicle_address.unwrap())
            .write_word(address, value)
    }
    fn tail_frame<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        offset: usize,
        index: u32,
    ) -> TailResult<B, u32> {
        self.tail_read_word(
            b,
            self.tail_vector(offset)
                .begin
                .wrapping_add(index.wrapping_mul(4)),
        )
        .map_err(VehicleInitUnavailable::Effect)
        .map_err(Into::into)
    }
    fn tail_number<B: VehicleComponentTailBackend>(
        &mut self,
        b: &mut B,
        offset: usize,
        index: u32,
        suffix: u32,
    ) -> TailResult<B, i32> {
        let frame = self.tail_frame(b, offset, index)?;
        let name = self
            .tail_read_word(b, frame.wrapping_add(0x100))
            .map_err(VehicleInitUnavailable::Effect)?;
        b.parse_component_number((if name == 0 { 0x63d2a0 } else { name }).wrapping_add(suffix))
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    pub(super) fn tail_name_length<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
    ) -> TailResult<B, u32> {
        let mut address = self.load(0x4bc)?;
        let mut remaining = u32::MAX;
        while remaining != 0 {
            let byte = self
                .tail_read_byte(b, address)
                .map_err(VehicleInitUnavailable::Effect)?;
            address = address.wrapping_add(1);
            remaining -= 1;
            if byte == 0 {
                break;
            }
        }
        Ok(!remaining)
    }
    fn tail_enumerate<B: VehicleComponentTailBackend>(
        &mut self,
        b: &mut B,
        root: u32,
        query: VehicleComponentTailQuery,
        offset: usize,
    ) -> TailResult<B, ()> {
        let scratch = self.tail_scratch.as_mut().unwrap();
        scratch.bytes[offset] = scratch.bytes[0x37];
        put_vector(
            scratch,
            offset,
            VehicleInitFrameVector {
                begin: 0,
                end: 0,
                capacity: 0,
            },
        );
        scratch.bytes[0x1c4] = match query {
            VehicleComponentTailQuery::Ballast => 2,
            VehicleComponentTailQuery::Seat => 3,
        };
        b.enumerate_tail(
            root,
            query,
            &mut VehicleComponentTailEnumeration {
                car: self.car.as_mut().map(CarInitOwned::memory),
                scratch,
                offset,
                vehicle: &mut self.bytes,
                vehicle_address: self.tail_vehicle_address.unwrap(),
            },
        )?;
        Ok(())
    }
    pub(super) fn tail_projection<'a, B>(
        &'a mut self,
        b: &'a mut B,
        base: u32,
    ) -> Projection<'a, B> {
        Projection {
            backend: b,
            memory: VehicleComponentTailMemory {
                car: self.car.as_mut().map(CarInitOwned::memory),
                vehicle_address: base,
                vehicle: &mut self.bytes,
                scratch: self.tail_scratch.as_mut().unwrap(),
            },
        }
    }
    pub(super) fn tail_direction<B: VehicleComponentsBackend>(
        &mut self,
        b: &mut B,
        frame: u32,
    ) -> TailResult<B, [u32; 3]> {
        use VehicleInitUnavailable::Effect;
        let flags = self
            .tail_read_word(b, frame.wrapping_add(0xac))
            .map_err(Effect)?;
        let mut direction = [0; 3];
        if flags & 0x10 == 0 {
            self.tail_write_word(b, frame.wrapping_add(0xac), flags | 0x10)
                .map_err(Effect)?;
            b.normalize_frame_quaternion(frame).map_err(Effect)?;
            b.set_frame_rotation_matrix(frame).map_err(Effect)?;
            for (i, value) in direction.iter_mut().enumerate() {
                *value = self
                    .tail_read_word(b, frame.wrapping_add(0x70 + i as u32 * 4))
                    .map_err(Effect)?;
                self.tail_store(0x28 + i * 4, *value);
            }
        } else {
            let fail =
                || VehicleComponentsUnavailable::NumericDomain("component direction normalization");
            let x = self
                .tail_read_word(b, frame.wrapping_add(0x70))
                .map_err(Effect)?;
            self.tail_store(0x10, x);
            let y = self
                .tail_read_word(b, frame.wrapping_add(0x74))
                .map_err(Effect)?;
            self.tail_store(0x14, y);
            let yy = checked_f32_mul(f32::from_bits(y), f32::from_bits(y)).map_err(|_| fail())?;
            let z = self
                .tail_read_word(b, frame.wrapping_add(0x78))
                .map_err(Effect)?;
            self.tail_store(0x18, z);
            let [x, y, z] = [x, y, z].map(f32::from_bits);
            let yz = checked_f32_add(yy, checked_f32_mul(z, z).map_err(|_| fail())?)
                .map_err(|_| fail())?;
            let squared = checked_f32_add(yz, checked_f32_mul(x, x).map_err(|_| fail())?)
                .map_err(|_| fail())?;
            self.tail_store(0x20, squared.to_bits());
            let delta = checked_f32_add(squared, -1.).map_err(|_| fail())?;
            if f64::from(delta.abs()) >= f64::from(1e-8_f32) {
                if squared < 1e-8_f32 {
                    let axis = if x != 0. {
                        0
                    } else if z != 0. {
                        2
                    } else {
                        1
                    };
                    let value = if [x, y, z][axis] < 0. { -1f32 } else { 1f32 };
                    self.tail_store(0x10 + axis * 4, value.to_bits());
                } else {
                    let length = squared.sqrt();
                    if !length.is_normal() {
                        return Err(fail().into());
                    }
                    let reciprocal = 1. / length;
                    let exact = 1. / f64::from(length);
                    if !reciprocal.is_normal()
                        || exact.abs() < f64::from(f32::MIN_POSITIVE)
                        || exact.abs() > f64::from(f32::MAX)
                    {
                        return Err(fail().into());
                    }
                    for (i, value) in [x, y, z].into_iter().enumerate() {
                        let normalized = checked_f32_mul(value, reciprocal).map_err(|_| fail())?;
                        self.tail_store(0x10 + i * 4, normalized.to_bits());
                    }
                }
            }
            for (i, value) in direction.iter_mut().enumerate() {
                *value = self.tail_word(0x10 + i * 4);
                self.tail_store(0x28 + i * 4, *value);
            }
        }
        Ok(direction)
    }
    pub(super) fn tail_transform<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        matrix: MatrixSource,
        point: PointSource,
        output: usize,
    ) -> TailResult<B, [u32; 3]> {
        let vector = |state: &mut Self, b: &mut B, index: u32| -> TailResult<B, f32> {
            match point {
                PointSource::Local(values) => Ok(f32::from_bits(values[index as usize])),
                PointSource::External(address) => state
                    .tail_read_word(b, address.wrapping_add(index * 4))
                    .map(f32::from_bits)
                    .map_err(VehicleInitUnavailable::Effect)
                    .map_err(Into::into),
            }
        };
        let element = |state: &mut Self, b: &mut B, index: u32| -> TailResult<B, f32> {
            match matrix {
                MatrixSource::Local(value) => Ok(value.elements[index as usize]),
                MatrixSource::External(address) => state
                    .tail_read_word(b, address.wrapping_add(index * 4))
                    .map(f32::from_bits)
                    .map_err(VehicleInitUnavailable::Effect)
                    .map_err(Into::into),
            }
        };
        let fail = |_| VehicleComponentsUnavailable::NumericDomain("component point transform");
        let mut result = [0; 3];
        for (column, value) in result.iter_mut().enumerate() {
            let first = checked_f32_mul(vector(self, b, 0)?, element(self, b, column as u32)?)
                .map_err(fail)?;
            let translated =
                checked_f32_add(first, element(self, b, 12 + column as u32)?).map_err(fail)?;
            let second = checked_f32_mul(vector(self, b, 1)?, element(self, b, 4 + column as u32)?)
                .map_err(fail)?;
            let accumulated = checked_f32_add(translated, second).map_err(fail)?;
            let third = checked_f32_mul(vector(self, b, 2)?, element(self, b, 8 + column as u32)?)
                .map_err(fail)?;
            *value = checked_f32_add(accumulated, third).map_err(fail)?.to_bits();
            self.tail_store(output + column * 4, *value);
        }
        Ok(result)
    }
    fn tail_detach<B: VehicleComponentTailBackend>(
        &mut self,
        b: &mut B,
        offset: usize,
        index: u32,
    ) -> TailResult<B, ()> {
        let frame = self.tail_frame(b, offset, index)?;
        b.link_frame(frame, 0, 0)
            .map_err(VehicleInitUnavailable::Effect)?;
        let model = self.load(0x34c)?;
        let frame = self.tail_frame(b, offset, index)?;
        b.remove_model_child(model, frame)
            .map_err(VehicleInitUnavailable::Effect)?;
        Ok(())
    }
    fn run_component_tail<B: VehicleComponentTailBackend>(
        &mut self,
        b: &mut B,
        base: u32,
        frontier: VehicleComponentsFrontier,
    ) -> TailResult<B, VehicleComponentTailOutcome> {
        use VehicleInitUnavailable::Effect;
        let mut index = match frontier {
            VehicleComponentsFrontier::DoorSetup { index } => index,
            VehicleComponentsFrontier::BallastEnumeration => {
                self.tail_vector(0x58)
                    .end
                    .wrapping_sub(self.tail_vector(0x58).begin)
                    / 4
            }
        };
        while index < count(self.tail_vector(0x58)) {
            self.tail_zero(0xb4, 22);
            let frame = self.tail_frame(b, 0x58, index)?;
            let parent = self
                .tail_read_word(b, frame.wrapping_add(0x120))
                .map_err(Effect)?;
            b.prepare_frame_matrix(parent).map_err(Effect)?;
            let frame = self.tail_frame(b, 0x58, index)?;
            let position = self.tail_transform(
                b,
                MatrixSource::External(parent.wrapping_add(0x50)),
                PointSource::External(frame.wrapping_add(0x80)),
                0x80,
            )?;
            for (i, value) in position.into_iter().enumerate() {
                self.tail_store(0xc8 + i * 4, value);
            }
            self.tail_store(0xe0, 0x3fb9999a);
            self.tail_store(0xdc, 0x3fa00000);
            let frame = self.tail_frame(b, 0x58, index)?;
            self.tail_store(0xb4, frame);
            let rotation_frame = self.tail_frame(b, 0x58, index)?;
            let flags = self
                .tail_read_word(b, rotation_frame.wrapping_add(0xac))
                .map_err(Effect)?;
            if flags & 8 == 0 {
                self.tail_write_word(b, rotation_frame.wrapping_add(0xac), flags | 8)
                    .map_err(Effect)?;
                b.materialize_frame_quaternion(rotation_frame)
                    .map_err(Effect)?;
            }
            for i in 0..4 {
                let value = self
                    .tail_read_word(b, rotation_frame.wrapping_add(0x9c + i * 4))
                    .map_err(Effect)?;
                self.tail_store(0xb8 + i as usize * 4, value);
            }
            let direction = self.tail_direction(b, frame)?;
            if f32::from_bits(direction[2]) < 0. {
                self.tail_scratch.as_mut().unwrap().bytes[0xe4] |= 0xc;
            }
            // Native property lookup scans once for length, then for '[' and
            // a comma/bracket-delimited I; even the post-NUL read is retained.
            let property = self
                .tail_read_word(b, frame.wrapping_add(0xc))
                .map_err(Effect)?;
            let property = if property == 0 { 0x63d2a0 } else { property };
            let mut remaining = u32::MAX;
            let mut cursor = property;
            while remaining != 0 {
                let byte = self.tail_read_byte(b, cursor).map_err(Effect)?;
                cursor = cursor.wrapping_add(1);
                remaining -= 1;
                if byte == 0 {
                    break;
                }
            }
            if (!remaining).wrapping_sub(1) != 0 {
                let mut offset = 0u32;
                let mut byte = self.tail_read_byte(b, property).map_err(Effect)?;
                while byte != b'[' && byte != 0 {
                    offset = offset.wrapping_add(1);
                    byte = self
                        .tail_read_byte(b, property.wrapping_add(offset))
                        .map_err(Effect)?;
                }
                loop {
                    offset = offset.wrapping_add(1);
                    byte = self
                        .tail_read_byte(b, property.wrapping_add(offset))
                        .map_err(Effect)?;
                    if byte == b'I' {
                        let previous = self
                            .tail_read_byte(b, property.wrapping_add(offset).wrapping_sub(1))
                            .map_err(Effect)?;
                        if previous != b',' && previous != b'[' {
                            continue;
                        }
                    } else if byte != 0 {
                        continue;
                    }
                    if self
                        .tail_read_byte(b, property.wrapping_add(offset))
                        .map_err(Effect)?
                        == b'I'
                    {
                        self.tail_scratch.as_mut().unwrap().bytes[0xe4] |= 4;
                    }
                    break;
                }
            }
            let window = b.find_window(frame).map_err(Effect)?;
            if window != 0 {
                self.tail_store(0xe8, window);
                let x = self
                    .tail_read_word(b, window.wrapping_add(0x80))
                    .map_err(Effect)?;
                self.tail_store(0xec, x);
                let y = self
                    .tail_read_word(b, window.wrapping_add(0x84))
                    .map_err(Effect)?;
                self.tail_store(0xf0, y);
                let lowered = checked_f32_add(f32::from_bits(y), -0.5).map_err(|_| {
                    VehicleComponentsUnavailable::NumericDomain("window position subtraction")
                })?;
                let z = self
                    .tail_read_word(b, window.wrapping_add(0x88))
                    .map_err(Effect)?;
                self.tail_store(0xf4, z);
                self.tail_store(0xf8, x);
                self.tail_store(0xfc, y);
                self.tail_store(0x104, 0x3e99999a);
                self.tail_store(0x100, lowered.to_bits());
            }
            let insertion = self.load(0xcb8)?;
            let source = self
                .tail_scratch
                .as_ref()
                .unwrap()
                .address
                .wrapping_add(0xb4);
            vehicle_door_records_insert(
                &mut self.tail_projection(b, base),
                base.wrapping_add(0xcb0),
                insertion,
                1,
                source,
            )
            .map_err(Effect)?;
            index = index.wrapping_add(1);
            self.tail_store(0x1c, index);
        }
        let body = self.load(0xe68)?;
        self.tail_enumerate(b, body, VehicleComponentTailQuery::Ballast, 0x38)?;
        if self.tail_vector(0x38).begin != 0 {
            self.tail_store(0x20, count(self.tail_vector(0x38)));
        }
        if count(self.tail_vector(0x38)) != 0 {
            self.tail_store(0x1c, 1);
            let suffix = self.tail_name_length(b)?.wrapping_add(7);
            self.tail_store(0x20, suffix);
            index = 1;
            while index < count(self.tail_vector(0x38)) {
                let key = self.tail_number(b, 0x38, index, suffix)?;
                let mut previous = 0;
                while (previous as i32) < index as i32 {
                    if key < self.tail_number(b, 0x38, previous, suffix)? {
                        let position = self
                            .tail_vector(0x38)
                            .begin
                            .wrapping_add(index.wrapping_mul(4));
                        let container = self
                            .tail_scratch
                            .as_ref()
                            .unwrap()
                            .address
                            .wrapping_add(0x38);
                        vehicle_component_pointers_insert(
                            &mut self.tail_projection(b, base),
                            container,
                            position,
                            1,
                            position,
                        )
                        .map_err(Effect)?;
                        let last = self
                            .tail_read_word(b, self.tail_vector(0x38).end.wrapping_sub(4))
                            .map_err(Effect)?;
                        self.tail_write_word(
                            b,
                            self.tail_vector(0x38)
                                .begin
                                .wrapping_add(index.wrapping_mul(4))
                                .wrapping_add(4),
                            last,
                        )
                        .map_err(Effect)?;
                        let end = self.tail_vector(0x38).end;
                        vehicle_component_pointers_copy(
                            &mut self.tail_projection(b, base),
                            end,
                            end,
                            end.wrapping_sub(4),
                        )
                        .map_err(Effect)?;
                        // Original 0x4d2040 is RET 8, with no memory effects.
                        self.tail_store(0x40, self.tail_word(0x40).wrapping_sub(4));
                        break;
                    }
                    previous = previous.wrapping_add(1);
                }
                index = index.wrapping_add(1);
                self.tail_store(0x1c, index);
            }
        }
        index = 0;
        while index < count(self.tail_vector(0x38)) {
            self.tail_transformed_record(b, 0x38, index, false)?;
            let insertion = self.load(0xcc8)?;
            let source = self
                .tail_scratch
                .as_ref()
                .unwrap()
                .address
                .wrapping_add(0x8c);
            vehicle_ballast_records_insert(
                &mut self.tail_projection(b, base),
                base.wrapping_add(0xcc0),
                insertion,
                1,
                source,
            )
            .map_err(Effect)?;
            self.tail_detach(b, 0x38, index)?;
            index = index.wrapping_add(1);
        }
        let model = self.load(0x34c)?;
        self.tail_enumerate(b, model, VehicleComponentTailQuery::Seat, 0x48)?;
        if self
            .bytes
            .get(0xc70)
            .ok_or(VehicleInitUnavailable::Storage(0xc70))?
            & 2
            == 0
            && count(self.tail_vector(0x48)) < 1
        {
            // The four original frees occur even for null vector pointers.
            for offset in [0x48, 0x38, 0x58] {
                self.init_free(b, self.tail_vector(offset).begin)
                    .map_err(Effect)?;
                self.tail_zero(offset + 4, 3);
            }
            let allocation = self.tail_vector(0x68).begin;
            self.init_free(b, allocation).map_err(Effect)?;
            return Ok(VehicleComponentTailOutcome::Rejected(
                VehicleInitRejection::MissingSeat,
            ));
        }
        let suffix = self.tail_name_length(b)?.wrapping_add(4);
        self.tail_store(0x20, suffix);
        index = 0;
        while index < count(self.tail_vector(0x48)) {
            if self.tail_number(b, 0x48, index, suffix)? != index as i32 {
                let mut next = index.wrapping_add(1);
                while next < count(self.tail_vector(0x48)) {
                    if self.tail_number(b, 0x48, next, suffix)? == index as i32 {
                        let begin = self.tail_vector(0x48).begin;
                        let current = self
                            .tail_read_word(b, begin.wrapping_add(index.wrapping_mul(4)))
                            .map_err(Effect)?;
                        let other = self
                            .tail_read_word(b, begin.wrapping_add(next.wrapping_mul(4)))
                            .map_err(Effect)?;
                        self.tail_write_word(b, begin.wrapping_add(next.wrapping_mul(4)), current)
                            .map_err(Effect)?;
                        self.tail_write_word(
                            b,
                            self.tail_vector(0x48)
                                .begin
                                .wrapping_add(index.wrapping_mul(4)),
                            other,
                        )
                        .map_err(Effect)?;
                        break;
                    }
                    next = next.wrapping_add(1);
                }
            }
            index = index.wrapping_add(1);
        }
        index = 0;
        while index < count(self.tail_vector(0x48)) {
            self.tail_transformed_record(b, 0x48, index, true)?;
            let insertion = self.load(0xca8)?;
            let source = self
                .tail_scratch
                .as_ref()
                .unwrap()
                .address
                .wrapping_add(0x8c);
            vehicle_seat_records_insert(
                &mut self.tail_projection(b, base),
                base.wrapping_add(0xca0),
                insertion,
                1,
                source,
            )
            .map_err(Effect)?;
            let function = self.load(0x304)?;
            if function != 0 {
                let frame = self.tail_frame(b, 0x48, index)?;
                let context = self.load(0xc74)?;
                let memory = VehicleComponentTailMemory {
                    car: self.car.as_mut().map(CarInitOwned::memory),
                    vehicle_address: base,
                    vehicle: &mut self.bytes,
                    scratch: self.tail_scratch.as_mut().unwrap(),
                };
                b.seat_callback(memory, function, context, frame)
                    .map_err(Effect)?;
            }
            self.tail_detach(b, 0x48, index)?;
            index = index.wrapping_add(1);
        }
        index = 0;
        loop {
            let begin = self.load::<B::Error>(0xca4)?;
            if begin == 0 {
                break;
            }
            let end = self.load::<B::Error>(0xca8)?;
            let seats = ((end.wrapping_sub(begin) as i32) / 36) as u32;
            if index >= seats {
                break;
            }
            let doors = self.load::<B::Error>(0xcb4)?;
            if doors != 0 {
                let end = self.load::<B::Error>(0xcb8)?;
                if index < ((end.wrapping_sub(doors) as i32) / 88) as u32 {
                    let doors = self.load::<B::Error>(0xcb4)?;
                    let seats = self.load::<B::Error>(0xca4)?;
                    self.tail_write_word(
                        b,
                        seats.wrapping_add(index.wrapping_mul(36)).wrapping_add(32),
                        doors.wrapping_add(index.wrapping_mul(88)),
                    )
                    .map_err(Effect)?;
                }
            }
            index = index.wrapping_add(1);
        }
        self.wheel_continuation = true;
        Ok(VehicleComponentTailOutcome::UnavailableFrontier(
            VehicleComponentTailFrontier::WheelEnumeration,
        ))
    }
    pub(super) fn tail_transformed_record<B: VehicleComponentTailBackend>(
        &mut self,
        b: &mut B,
        offset: usize,
        index: u32,
        seat: bool,
    ) -> TailResult<B, ()> {
        use VehicleInitUnavailable::Effect;
        self.tail_zero(0x8c, if seat { 9 } else { 7 });
        let frame = self.tail_frame(b, offset, index)?;
        let parent = self
            .tail_read_word(b, frame.wrapping_add(0x120))
            .map_err(Effect)?;
        b.prepare_frame_matrix(parent).map_err(Effect)?;
        let mut words = [0; 16];
        for (i, word) in words.iter_mut().enumerate() {
            *word = self
                .tail_read_word(b, parent.wrapping_add(0x50 + i as u32 * 4))
                .map_err(Effect)?;
            self.tail_store(0xb4 + i * 4, *word);
        }
        let frame = self.tail_frame(b, offset, index)?;
        let position = self.tail_transform(
            b,
            MatrixSource::Local(matrix(words)),
            PointSource::External(frame.wrapping_add(0x80)),
            0x80,
        )?;
        self.tail_store(0x8c, position[0]);
        self.tail_store(0x90, position[1]);
        if seat {
            self.tail_store(0xec, 0);
        }
        self.tail_store(0x94, position[2]);
        if !seat {
            self.tail_store(0xec, 0);
        }
        self.tail_store(0xe8, 0);
        self.tail_store(0xe4, 0);
        words[12..15].fill(0);
        let frame = self.tail_frame(b, offset, index)?;
        let direction = self.tail_direction(b, frame)?;
        let direction = self.tail_transform(
            b,
            MatrixSource::Local(matrix(words)),
            PointSource::Local(direction),
            0x130,
        )?;
        for (i, value) in direction.into_iter().enumerate() {
            self.tail_store(0x98 + i * 4, value);
        }
        Ok(())
    }
}

pub(super) fn insert_special_point_projection<B: VehicleInitBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    insertion: u32,
    source_offset: u32,
) -> Result<(), B::Error> {
    let vehicle_address = memory.vehicle_address;
    let source = memory.scratch.address.wrapping_add(source_offset);
    vehicle_special_points_insert(
        &mut Projection { backend, memory },
        vehicle_address.wrapping_add(0x1fb0),
        insertion,
        source,
    )
    .map(|_| ())
}
pub(super) fn insert_special_control_projection<B: VehicleInitBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    insertion: u32,
) -> Result<(), B::Error> {
    let vehicle_address = memory.vehicle_address;
    let source = memory.scratch.address.wrapping_add(0x20);
    vehicle_special_controls_insert(
        &mut Projection { backend, memory },
        vehicle_address.wrapping_add(0x1ff8),
        insertion,
        source,
    )
    .map(|_| ())
}
pub(super) fn insert_exhaust_live_projection<B: VehicleInitBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    insertion: u32,
) -> Result<(), B::Error> {
    let vehicle_address = memory.vehicle_address;
    let source = memory.scratch.address.wrapping_add(0x8c);
    vehicle_exhaust_records_insert(
        &mut Projection { backend, memory },
        vehicle_address.wrapping_add(0x450),
        insertion,
        source,
    )
    .map(|_| ())
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemoryBackend {
        bytes: BTreeMap<u32, u8>,
        events: Vec<(&'static str, u32)>,
        fail: Option<u32>,
    }
    impl VehicleInitBackend for MemoryBackend {
        type Error = u32;
        fn read_byte(&mut self, address: u32) -> Result<u8, u32> {
            self.events.push(("read_byte", address));
            if self.fail == Some(address) {
                return Err(address);
            }
            Ok(*self.bytes.get(&address).unwrap_or(&0xee))
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), u32> {
            self.events.push(("write_byte", address));
            if self.fail == Some(address) {
                return Err(address);
            }
            self.bytes.insert(address, value);
            Ok(())
        }
        fn read_word(&mut self, address: u32) -> Result<u32, u32> {
            self.events.push(("read_word", address));
            Ok(0x12345678)
        }
        fn write_word(&mut self, address: u32, _: u32) -> Result<(), u32> {
            self.events.push(("write_word", address));
            Ok(())
        }
        fn allocate(&mut self, _: u32) -> Result<u32, u32> {
            unreachable!()
        }
        fn free(&mut self, _: u32) -> Result<(), u32> {
            unreachable!()
        }
        fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), u32> {
            unreachable!()
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<u32>> {
            unreachable!()
        }
    }

    #[test]
    fn mixed_words_resolve_each_owner_at_both_edges_of_both_projections() {
        for edge in [
            0x1000u32,
            0x3400,
            0x5000,
            0x5000 + VEHICLE_INIT_STACK_BYTES as u32,
        ] {
            for preceding in 1..=3 {
                let mut vehicle = [0x11; 0x2400];
                let mut scratch = VehicleComponentTailScratch {
                    address: 0x5000,
                    bytes: [0x22; VEHICLE_INIT_STACK_BYTES],
                };
                let mut backend = MemoryBackend::default();
                let mut projection = Projection {
                    backend: &mut backend,
                    memory: VehicleComponentTailMemory {
                        car: None,
                        vehicle_address: 0x1000,
                        vehicle: &mut vehicle,
                        scratch: &mut scratch,
                    },
                };
                let address = edge - preceding;
                let expected = std::array::from_fn(|i| {
                    projection
                        .memory
                        .read_byte(address + i as u32)
                        .unwrap_or(0xee)
                });
                assert_eq!(
                    projection.read_word(address),
                    Ok(u32::from_le_bytes(expected))
                );
                projection.backend.events.clear();
                assert_eq!(projection.write_word(address, 0x7d008060), Ok(()));
                for (offset, byte) in [0x60, 0x80, 0, 0x7d].into_iter().enumerate() {
                    let address = address + offset as u32;
                    match projection.memory.read_byte(address) {
                        Some(actual) => {
                            assert_eq!(actual, byte);
                            assert!(!projection.backend.bytes.contains_key(&address));
                        }
                        None => assert_eq!(projection.backend.bytes.get(&address), Some(&byte)),
                    }
                }
                assert!(projection
                    .backend
                    .events
                    .iter()
                    .all(|(op, _)| *op == "write_byte"));
                assert_eq!(projection.read_word(address), Ok(0x7d008060));
            }
        }
    }

    #[test]
    fn local_only_words_preflight_and_cross_adjacent_projections() {
        let mut vehicle = [0x11; 8];
        let mut scratch = VehicleComponentTailScratch {
            address: 0x1008,
            bytes: [0x22; VEHICLE_INIT_STACK_BYTES],
        };
        let mut memory = VehicleComponentTailMemory {
            car: None,
            vehicle_address: 0x1000,
            vehicle: &mut vehicle,
            scratch: &mut scratch,
        };
        assert_eq!(memory.read_word(0x1006), Some(0x22221111));
        assert_eq!(memory.write_word(0x1006, 0x44332211), Some(()));
        assert_eq!(memory.read_word(0x1006), Some(0x44332211));
        assert_eq!(
            memory.write_word(0x1006 + VEHICLE_INIT_STACK_BYTES as u32, 0),
            None
        );
        assert_eq!(memory.read_byte(0x1142), Some(0x22));
        assert_eq!(memory.read_byte(0x1143), Some(0x22));
        let mut backend = MemoryBackend::default();
        assert_eq!(
            memory.read_word_with_backend(&mut backend, 0x1006),
            Ok(0x44332211)
        );
        assert_eq!(
            memory.write_word_with_backend(&mut backend, 0x1006, 0),
            Ok(())
        );
        assert!(backend.events.is_empty());
    }

    #[test]
    fn wholly_external_words_keep_backend_word_operations() {
        let mut vehicle = [0; 8];
        let mut scratch = VehicleComponentTailScratch {
            address: 0x5000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        };
        let mut memory = VehicleComponentTailMemory {
            car: None,
            vehicle_address: 0x1000,
            vehicle: &mut vehicle,
            scratch: &mut scratch,
        };
        let mut backend = MemoryBackend::default();
        assert_eq!(
            memory.read_word_with_backend(&mut backend, 0x4000),
            Ok(0x12345678)
        );
        assert_eq!(
            memory.write_word_with_backend(&mut backend, 0x4000, 1),
            Ok(())
        );
        assert_eq!(
            backend.events,
            [("read_word", 0x4000), ("write_word", 0x4000)]
        );
    }

    #[test]
    fn mixed_word_unavailability_retains_only_completed_byte_effects() {
        for address in [0xffeu32, 0x1006] {
            let mut vehicle = [0x11; 8];
            let mut scratch = VehicleComponentTailScratch {
                address: 0x5000,
                bytes: [0; VEHICLE_INIT_STACK_BYTES],
            };
            let mut memory = VehicleComponentTailMemory {
                car: None,
                vehicle_address: 0x1000,
                vehicle: &mut vehicle,
                scratch: &mut scratch,
            };
            let failure = if address < 0x1000 { 0xfff } else { 0x1009 };
            let mut backend = MemoryBackend {
                fail: Some(failure),
                ..MemoryBackend::default()
            };
            assert_eq!(
                memory.read_word_with_backend(&mut backend, address),
                Err(failure)
            );
            backend.events.clear();
            assert_eq!(
                memory.write_word_with_backend(&mut backend, address, 0x44332211),
                Err(failure)
            );
            assert_eq!(
                backend.bytes.get(&(failure - 1)),
                Some(&if address < 0x1000 { 0x11 } else { 0x33 })
            );
            assert!(!backend.bytes.contains_key(&failure));
            if address < 0x1000 {
                assert_eq!(memory.read_word(0x1000), Some(0x11111111));
            } else {
                assert_eq!(memory.read_byte(0x1006), Some(0x11));
                assert_eq!(memory.read_byte(0x1007), Some(0x22));
            }
            assert_eq!(backend.events.last(), Some(&("write_byte", failure)));
        }
    }

    #[test]
    fn mixed_word_addresses_wrap_in_the_native_u32_domain() {
        let mut vehicle = [0x11; 8];
        let mut scratch = VehicleComponentTailScratch {
            address: 0x5000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        };
        let mut memory = VehicleComponentTailMemory {
            car: None,
            vehicle_address: 0,
            vehicle: &mut vehicle,
            scratch: &mut scratch,
        };
        let mut backend = MemoryBackend::default();
        assert_eq!(
            memory.read_word_with_backend(&mut backend, u32::MAX - 1),
            Ok(0x1111eeee)
        );
        assert_eq!(
            memory.write_word_with_backend(&mut backend, u32::MAX - 1, 0x44332211),
            Ok(())
        );
        assert_eq!(memory.read_byte(0), Some(0x33));
        assert_eq!(memory.read_byte(1), Some(0x44));
        assert_eq!(backend.bytes.get(&(u32::MAX - 1)), Some(&0x11));
        assert_eq!(backend.bytes.get(&u32::MAX), Some(&0x22));
    }
}
