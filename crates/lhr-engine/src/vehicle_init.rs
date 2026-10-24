//! Original PC `C_Vehicle::Init`, component discovery and explicit ownership.
//!
//! A frontier is an unfinished native initialization, never a usable vehicle.
//! Addresses are inert 32-bit identities. The backend owns external storage,
//! references and allocation effects; dropping this projection releases none.

#[path = "car_state.rs"]
mod car_state;
#[path = "vehicle_components.rs"]
mod components;
pub use car_state::*;
use car_state::{CarInitMemory, CarInitOwned};
#[path = "car_construction.rs"]
mod car_construction;
pub use car_construction::*;
#[path = "car_init.rs"]
mod car_init;
pub use car_init::*;
#[path = "car_caller_cleanup.rs"]
mod car_caller_cleanup;
pub use car_caller_cleanup::*;
pub use components::*;
#[path = "vehicle_component_tail.rs"]
mod component_tail;
pub use component_tail::*;
#[path = "vehicle_wheels.rs"]
mod wheels;
pub use wheels::*;
#[path = "vehicle_axis_records.rs"]
mod axis_records;
pub use axis_records::*;
#[path = "vehicle_axis.rs"]
mod axis;
pub use axis::*;
#[path = "vehicle_axis_update.rs"]
mod axis_update;
pub use axis_update::*;
#[path = "vehicle_init_finish.rs"]
mod finish;
pub use finish::*;
#[path = "vehicle_car_caller.rs"]
mod car_caller;
pub use car_caller::*;
#[path = "vehicle_destructor.rs"]
mod destructor;
pub use destructor::*;

#[path = "vehicle_source_parameters.rs"]
mod source_parameters;
pub use source_parameters::*;
#[path = "vehicle_light_init.rs"]
mod light_init;
pub use light_init::*;

#[path = "vehicle_light_projectors.rs"]
mod light_projectors;
pub use light_projectors::*;

#[path = "vehicle_setup_records.rs"]
mod setup_records;
pub use setup_records::*;

#[path = "vehicle_setup_defaults.rs"]
mod setup_defaults;
pub use setup_defaults::*;

#[path = "vehicle_setup.rs"]
mod setup;
pub use setup::*;
#[path = "vehicle_init_complete.rs"]
mod complete;
pub use complete::*;

#[path = "vehicle_deform.rs"]
mod deform;
pub use deform::*;

/// Original frame enumeration requests in this initialization prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleInitQuery {
    Body,
    Exhaust,
    SpecialCollision,
    Head,
}

impl VehicleInitQuery {
    pub fn pattern(self) -> &'static [u8] {
        match self {
            Self::Body => b"BODY",
            Self::Exhaust => b"EXHAUST*",
            Self::SpecialCollision => b"SPECCOLL*",
            Self::Head => b"HEAD",
        }
    }

    pub fn mask(self) -> u32 {
        match self {
            Self::Body => 0x21,
            Self::Exhaust | Self::SpecialCollision => 0x10021,
            Self::Head => 0x20,
        }
    }
}

/// External vector allocation retained at an unfinished initialization frontier.
/// These fields are original addresses, not a Rust collection or owned pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleInitFrameVector {
    pub begin: u32,
    pub end: u32,
    pub capacity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleInitRejection {
    NullModel,
    WrongFrameType,
    NameAllocation,
    MissingBody,
    MissingSeat,
}

/// The next original operation lies outside the implemented prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleInitFrontier {
    /// Reverse EXHAUST iteration, beginning with BODY matrix preparation.
    ExhaustTransform { index: u32 },
    /// Forward SPECCOLL iteration, beginning with its six-word data copy.
    SpecialCollisionTransform,
    /// BODY-relative HEAD transform and model-child detachment.
    HeadTransform,
    /// BODYCOL enumeration and its different native callback.
    BodyCollisionEnumeration,
}

/// No variant reports successful completion of the original Init function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleInitOutcome {
    Rejected(VehicleInitRejection),
    UnavailableFrontier(VehicleInitFrontier),
}

/// Host unavailability preserves all preceding original stores and effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleInitUnavailable<E> {
    Storage(u32),
    Effect(E),
    /// Re-entering a stopped prefix would overwrite retained native ownership.
    AlreadyStarted,
    ProjectionBackendUnavailable,
}

/// Explicit dependencies of the original initialization prefix.
///
/// Memory operations address external storage disjoint from the vehicle byte
/// projection. Native free/allocation may change external storage, including
/// the model's name pointer; Init reads that pointer again before copying.
/// An unavailable effect must retain its own completed effect prefix.
///
/// `enumerate` is the model's original virtual enumeration operation. Its host
/// implementation must apply the supplied pattern/mask, preserve traversal
/// order, call `target.visit` for each matching frame, and preserve the original callback-return semantics.
/// It must not manufacture a BODY or silently treat an unavailable model as an
/// empty enumeration. `collect_frame` is the still external original collecting
/// callback, including its vector allocation, copying and failure behavior.
/// There are deliberately no default implementations or assumed successes.
pub trait VehicleInitBackend {
    type Error;

    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error>;
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error>;
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error>;
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error>;
    fn free(&mut self, allocation: u32) -> Result<(), Self::Error>;
    /// Returning zero is the original allocator's null result, not unavailability.
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error>;
    /// Allocator effects reached through a live projection receive that same
    /// mapping. The ordinary heap implementation need not access it; an effect
    /// that observes or mutates caller memory must do so through this view.
    fn allocate_projected(
        &mut self,
        _memory: &mut VehicleComponentTailMemory<'_>,
        bytes: u32,
    ) -> Result<u32, Self::Error> {
        self.allocate(bytes)
    }
    fn free_projected(
        &mut self,
        _memory: &mut VehicleComponentTailMemory<'_>,
        allocation: u32,
    ) -> Result<(), Self::Error> {
        self.free(allocation)
    }
    fn collect_frame(
        &mut self,
        vector: &mut VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error>;
    /// Original collecting callback over the same live vehicle/stack mapping.
    /// Allocation and free callbacks can alter the header before its next load.
    fn collect_frame_projected(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        header: u32,
        frame: u32,
    ) -> Result<u32, VehicleInitUnavailable<Self::Error>> {
        vehicle_collect_frame(
            &mut component_tail::Projection {
                backend: self,
                memory,
            },
            header,
            frame,
        )
        .map_err(VehicleInitUnavailable::Effect)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
}

/// Callback destination supplied to the actual model enumeration dependency.
/// The backend invokes `visit` for each original callback in traversal order.
/// The single-slot callback immediately writes its destination; repeated calls
/// overwrite it in that order.
pub struct VehicleInitEnumeration<'a> {
    target: EnumerationTarget<'a>,
}

enum EnumerationTarget<'a> {
    Body(&'a mut VehicleInitState),
    ProjectedCollect(&'a mut VehicleInitState),
    ProjectedHead(&'a mut VehicleInitState),
}

impl VehicleInitEnumeration<'_> {
    pub fn visit<B: VehicleInitBackend + ?Sized>(
        &mut self,
        frame: u32,
        backend: &mut B,
    ) -> Result<u32, VehicleInitUnavailable<B::Error>> {
        match &mut self.target {
            EnumerationTarget::Body(state) => {
                state.store(0xe68, frame)?;
                Ok(0)
            }
            EnumerationTarget::ProjectedHead(state) => {
                state.head = frame;
                state.component_stack_words(0x78, &[frame]);
                Ok(0)
            }
            EnumerationTarget::ProjectedCollect(state) => {
                let result = if state.tail_scratch.is_some() {
                    let header = state
                        .tail_scratch
                        .as_ref()
                        .unwrap()
                        .address
                        .wrapping_add(0x68);
                    let result =
                        backend.collect_frame_projected(state.live_tail_memory(), header, frame);
                    state.scratch = Some(state.tail_vector(0x68));
                    result.map(|_| ())
                } else {
                    backend
                        .collect_frame(state.scratch.as_mut().unwrap(), frame)
                        .map_err(VehicleInitUnavailable::Effect)
                };
                result?;
                Ok(0)
            }
        }
    }
}

/// Caller-provided bytes plus retained temporary native ownership.
/// Constructing or dropping this value is not original construction/destruction.
/// It intentionally does not implement Clone: copying native resource tokens
/// would not retain their references or duplicate their ownership.
#[derive(Debug, PartialEq, Eq)]
pub struct VehicleInitState {
    bytes: Vec<u8>,
    car: Option<CarInitOwned>,
    started: bool,
    scratch: Option<VehicleInitFrameVector>,
    head: u32,
    continuation: Option<VehicleInitFrontier>,
    name_length: u32,
    component_scratch: Option<VehicleInitFrameVector>,
    component_tail: Option<VehicleComponentsFrontier>,
    tail_scratch: Option<VehicleComponentTailScratch>,
    tail_vehicle_address: Option<u32>,
    wheel_continuation: bool,
    wheel_setup_ready: bool,
    car_caller_started: bool,
    car_caller_ready: bool,
    final_setup_ready: bool,
    init_geometry_saved_esi: Option<u32>,
    destructor_started: bool,
}

impl VehicleInitState {
    pub(super) fn init_allocate<B: VehicleInitBackend>(
        &mut self,
        backend: &mut B,
        bytes: u32,
    ) -> Result<u32, B::Error> {
        if self.tail_scratch.is_some() && self.tail_vehicle_address.is_some() {
            backend.allocate_projected(&mut self.live_tail_memory(), bytes)
        } else {
            backend.allocate(bytes)
        }
    }
    pub(super) fn init_free<B: VehicleInitBackend>(
        &mut self,
        backend: &mut B,
        allocation: u32,
    ) -> Result<(), B::Error> {
        if self.tail_scratch.is_some() && self.tail_vehicle_address.is_some() {
            backend.free_projected(&mut self.live_tail_memory(), allocation)
        } else {
            backend.free(allocation)
        }
    }
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            car: None,
            started: false,
            scratch: None,
            head: 0,
            continuation: None,
            name_length: 0,
            component_scratch: None,
            component_tail: None,
            tail_scratch: None,
            tail_vehicle_address: None,
            wheel_continuation: false,
            wheel_setup_ready: false,
            car_caller_started: false,
            car_caller_ready: false,
            final_setup_ready: false,
            init_geometry_saved_esi: None,
            destructor_started: false,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn scratch(&self) -> Option<VehicleInitFrameVector> {
        self.scratch.map(|value| {
            if self.tail_scratch.is_some() {
                self.tail_vector(0x68)
            } else {
                value
            }
        })
    }

    pub fn head(&self) -> u32 {
        self.head
    }

    fn load<E>(&self, offset: u32) -> Result<u32, VehicleInitUnavailable<E>> {
        let start = offset as usize;
        let bytes = self
            .bytes
            .get(start..start + 4)
            .ok_or(VehicleInitUnavailable::Storage(offset))?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn store<E>(&mut self, offset: u32, value: u32) -> Result<(), VehicleInitUnavailable<E>> {
        let start = offset as usize;
        let bytes = self
            .bytes
            .get_mut(start..start + 4)
            .ok_or(VehicleInitUnavailable::Storage(offset))?;
        bytes.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn name_pointer<B: VehicleInitBackend>(
        &self,
        backend: &mut B,
    ) -> Result<u32, VehicleInitUnavailable<B::Error>> {
        let model = self.load(0x34c)?;
        let pointer = backend
            .read_word(model.wrapping_add(0x100))
            .map_err(VehicleInitUnavailable::Effect)?;
        Ok(if pointer == 0 { 0x0063_d2a0 } else { pointer })
    }

    fn frontier(&mut self, frontier: VehicleInitFrontier) -> VehicleInitOutcome {
        self.continuation = Some(frontier);
        VehicleInitOutcome::UnavailableFrontier(frontier)
    }

    /// Executes only the evidenced attachment and discovery prefix.
    ///
    /// Rejections preserve the original retained model reference and allocated
    /// name. A frontier retains any native scratch vector without freeing it;
    /// this method does not execute later transforms, BODYCOL, parameters,
    /// wheels or cleanup. `continue_components` consumes a retained genuine
    /// frontier through its separately required dependencies. Do not use a
    /// stopped projection as a complete vehicle or retry Init over it. No native faults or exception unwinding are modeled.
    pub fn init_prefix<B: VehicleInitBackend>(
        &mut self,
        backend: &mut B,
        model: u32,
    ) -> Result<VehicleInitOutcome, VehicleInitUnavailable<B::Error>> {
        use VehicleInitOutcome::Rejected;
        use VehicleInitUnavailable::Effect;
        if self.started {
            return Err(VehicleInitUnavailable::AlreadyStarted);
        }
        self.started = true;
        self.component_stack_words(0x1c0, &[0x61eadc, u32::MAX]);
        self.store(0x34c, model)?;
        if model == 0 {
            return Ok(Rejected(VehicleInitRejection::NullModel));
        }
        let references = backend.read_word(model.wrapping_add(4)).map_err(Effect)?;
        backend
            .write_word(model.wrapping_add(4), references.wrapping_add(1))
            .map_err(Effect)?;
        let attached = self.load::<B::Error>(0x34c)?;
        if backend
            .read_word(attached.wrapping_add(0x110))
            .map_err(Effect)?
            != 9
        {
            return Ok(Rejected(VehicleInitRejection::WrongFrameType));
        }
        let previous = self.load::<B::Error>(0x4bc)?;
        if previous != 0 {
            self.init_free(backend, previous).map_err(Effect)?;
            self.store(0x4bc, 0)?;
        }
        let mut source = self.name_pointer(backend)?;
        let mut remaining = u32::MAX;
        while remaining != 0 {
            let byte = backend.read_byte(source).map_err(Effect)?;
            source = source.wrapping_add(1);
            remaining -= 1;
            if byte == 0 {
                break;
            }
        }
        // The native scan includes NUL, then reserves one additional byte.
        let allocation = self
            .init_allocate(backend, (!remaining).wrapping_add(1))
            .map_err(Effect)?;
        self.store(0x4bc, allocation)?;
        if allocation == 0 {
            return Ok(Rejected(VehicleInitRejection::NameAllocation));
        }
        source = self.name_pointer(backend)?;
        let mut destination = allocation;
        loop {
            let byte = backend.read_byte(source).map_err(Effect)?;
            source = source.wrapping_add(1);
            backend.write_byte(destination, byte).map_err(Effect)?;
            destination = destination.wrapping_add(1);
            if byte == 0 {
                break;
            }
        }
        let attached = self.load::<B::Error>(0x34c)?;
        self.store(0xe68, 0)?;
        backend.enumerate(
            attached,
            VehicleInitQuery::Body,
            &mut VehicleInitEnumeration {
                target: EnumerationTarget::Body(self),
            },
        )?;
        if self.load::<B::Error>(0xe68)? == 0 {
            return Ok(Rejected(VehicleInitRejection::MissingBody));
        }
        self.scratch = Some(VehicleInitFrameVector {
            begin: 0,
            end: 0,
            capacity: 0,
        });
        if let Some(scratch) = self.tail_scratch.as_ref() {
            self.component_stack_byte(0x68, scratch.bytes[0x37]);
        }
        self.mirror_component_vector(0x68, self.scratch.unwrap());
        self.component_stack_words(0x1c4, &[0]);
        let attached = self.load::<B::Error>(0x34c)?;
        backend.enumerate(
            attached,
            VehicleInitQuery::Exhaust,
            &mut VehicleInitEnumeration {
                target: EnumerationTarget::ProjectedCollect(self),
            },
        )?;
        let vector = self.scratch.as_mut().unwrap();
        if vector.begin != 0 {
            let count = (vector.end.wrapping_sub(vector.begin) as i32) >> 2;
            if count != 0 {
                return Ok(self.frontier(VehicleInitFrontier::ExhaustTransform {
                    index: (count as u32).wrapping_sub(1),
                }));
            }
        }
        // Native erase(begin,end) has an empty suffix here and stores end=begin.
        vector.end = vector.begin;
        self.mirror_component_vector(0x68, self.scratch.unwrap());
        let attached = self.load::<B::Error>(0x34c)?;
        backend.enumerate(
            attached,
            VehicleInitQuery::SpecialCollision,
            &mut VehicleInitEnumeration {
                target: EnumerationTarget::ProjectedCollect(self),
            },
        )?;
        let vector = self.scratch.unwrap();
        if vector.begin != 0 && ((vector.end.wrapping_sub(vector.begin) as i32) >> 2) as u32 != 0 {
            return Ok(self.frontier(VehicleInitFrontier::SpecialCollisionTransform));
        }
        self.head = 0;
        self.component_stack_words(0x78, &[0]);
        let attached = self.load::<B::Error>(0x34c)?;
        backend.enumerate(
            attached,
            VehicleInitQuery::Head,
            &mut VehicleInitEnumeration {
                target: EnumerationTarget::ProjectedHead(self),
            },
        )?;
        if self.head != 0 {
            return Ok(self.frontier(VehicleInitFrontier::HeadTransform));
        }
        self.store(0x2d4, 0)?;
        self.store(0x2d8, 0)?;
        self.store(0x2dc, 0)?;
        // Original scans the copied name before calling the BODYCOL enumerator.
        let mut name = self.load::<B::Error>(0x4bc)?;
        let mut remaining = u32::MAX;
        while remaining != 0 {
            let byte = backend.read_byte(name).map_err(Effect)?;
            name = name.wrapping_add(1);
            remaining -= 1;
            if byte == 0 {
                break;
            }
        }
        self.name_length = !remaining;
        Ok(self.frontier(VehicleInitFrontier::BodyCollisionEnumeration))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const MODEL: u32 = 0x1000;
    const NAME: u32 = 0x2000;
    const ALLOCATION: u32 = 0x3000;
    const OLD_NAME: u32 = 0x4000;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        ReadByte(u32),
        ReadWord(u32),
        WriteByte(u32, u8),
        WriteWord(u32, u32),
        Free(u32),
        Allocate(u32),
        Enumerate(VehicleInitQuery),
        Collect(u32),
    }

    struct Backend {
        memory: BTreeMap<u32, u8>,
        events: Vec<Event>,
        fail: Option<usize>,
        allocation: u32,
        frames: Vec<(VehicleInitQuery, Vec<u32>)>,
        callback_results: Vec<u32>,
        replace_name_on_allocation: Option<u32>,
    }

    impl Backend {
        fn new() -> Self {
            let mut result = Self {
                memory: BTreeMap::new(),
                events: Vec::new(),
                fail: None,
                allocation: ALLOCATION,
                frames: vec![(VehicleInitQuery::Body, vec![0x5100, 0x5200])],
                callback_results: Vec::new(),
                replace_name_on_allocation: None,
            };
            result.put_word(MODEL + 4, u32::MAX);
            result.put_word(MODEL + 0x110, 9);
            result.put_word(MODEL + 0x100, NAME);
            result.put_bytes(NAME, b"synthetic-car\0");
            result.put_bytes(0x63d2a0, b"\0");
            result
        }

        fn put_word(&mut self, at: u32, value: u32) {
            self.put_bytes(at, &value.to_le_bytes());
        }

        fn put_bytes(&mut self, at: u32, values: &[u8]) {
            for (index, byte) in values.iter().enumerate() {
                self.memory.insert(at + index as u32, *byte);
            }
        }

        fn event(&mut self, event: Event) -> Result<(), &'static str> {
            let index = self.events.len();
            self.events.push(event);
            if self.fail == Some(index) {
                Err("effect unavailable")
            } else {
                Ok(())
            }
        }
    }

    impl VehicleInitBackend for Backend {
        type Error = &'static str;
        fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
            self.event(Event::ReadByte(address))?;
            self.memory.get(&address).copied().ok_or("missing byte")
        }
        fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
            self.event(Event::ReadWord(address))?;
            let mut bytes = [0; 4];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = *self
                    .memory
                    .get(&(address + index as u32))
                    .ok_or("missing word")?;
            }
            Ok(u32::from_le_bytes(bytes))
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
            self.event(Event::WriteByte(address, value))?;
            self.memory.insert(address, value);
            Ok(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            self.event(Event::WriteWord(address, value))?;
            self.put_word(address, value);
            Ok(())
        }
        fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
            self.event(Event::Free(allocation))
        }
        fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
            self.event(Event::Allocate(bytes))?;
            if let Some(name) = self.replace_name_on_allocation {
                self.put_word(MODEL + 0x100, name);
            }
            Ok(self.allocation)
        }
        fn collect_frame(
            &mut self,
            vector: &mut VehicleInitFrameVector,
            frame: u32,
        ) -> Result<(), Self::Error> {
            self.event(Event::Collect(frame))?;
            // Explicit synthetic native-vector effect; no allocator fidelity is
            // inferred from this controlled dependency implementation.
            if vector.begin == 0 {
                vector.begin = 0x8000;
                vector.end = 0x8000;
                vector.capacity = 0x9000;
            }
            self.put_word(vector.end, frame);
            vector.end += 4;
            Ok(())
        }
        fn enumerate(
            &mut self,
            model: u32,
            query: VehicleInitQuery,
            target: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            assert_eq!(model, MODEL);
            self.event(Event::Enumerate(query))
                .map_err(VehicleInitUnavailable::Effect)?;
            let frames = self
                .frames
                .iter()
                .find(|(kind, _)| *kind == query)
                .map(|(_, frames)| frames.clone())
                .unwrap_or_default();
            for frame in frames {
                let result = target.visit(frame, self)?;
                self.callback_results.push(result);
            }
            Ok(())
        }
    }

    fn state() -> VehicleInitState {
        let mut result = VehicleInitState::from_bytes(vec![0x7d; 0x2008]);
        result.store::<()>(0x4bc, OLD_NAME).unwrap();
        result
    }

    #[test]
    fn absent_optional_components_reach_explicit_body_collision_frontier() {
        let mut backend = Backend::new();
        let mut state = state();
        assert_eq!(
            state.init_prefix(&mut backend, MODEL),
            Ok(VehicleInitOutcome::UnavailableFrontier(
                VehicleInitFrontier::BodyCollisionEnumeration
            ))
        );
        assert_eq!(state.load::<()>(0xe68), Ok(0x5200));
        assert_eq!(state.load::<()>(0x34c), Ok(MODEL));
        assert_eq!(state.load::<()>(0x4bc), Ok(ALLOCATION));
        for offset in [0x2d4, 0x2d8, 0x2dc] {
            assert_eq!(state.load::<()>(offset), Ok(0));
        }
        assert_eq!(state.as_bytes()[0x2d3], 0x7d);
        assert_eq!(state.as_bytes()[0x2e0], 0x7d);
        assert_eq!(backend.callback_results, [0, 0]);
        assert_eq!(
            &backend.events[..5],
            &[
                Event::ReadWord(MODEL + 4),
                Event::WriteWord(MODEL + 4, 0),
                Event::ReadWord(MODEL + 0x110),
                Event::Free(OLD_NAME),
                Event::ReadWord(MODEL + 0x100),
            ]
        );
        assert!(backend.events.contains(&Event::Allocate(15)));
        let enumerations: Vec<_> = backend
            .events
            .iter()
            .filter_map(|event| {
                if let Event::Enumerate(query) = event {
                    Some(*query)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            enumerations,
            [
                VehicleInitQuery::Body,
                VehicleInitQuery::Exhaust,
                VehicleInitQuery::SpecialCollision,
                VehicleInitQuery::Head
            ]
        );
        assert_eq!(
            state.init_prefix(&mut backend, MODEL),
            Err(VehicleInitUnavailable::AlreadyStarted)
        );
    }

    #[test]
    fn null_and_wrong_type_leave_original_failure_prefix() {
        let mut backend = Backend::new();
        let mut null = state();
        assert_eq!(
            null.init_prefix(&mut backend, 0),
            Ok(VehicleInitOutcome::Rejected(
                VehicleInitRejection::NullModel
            ))
        );
        assert!(backend.events.is_empty());
        assert_eq!(null.load::<()>(0x34c), Ok(0));
        assert_eq!(null.load::<()>(0x4bc), Ok(OLD_NAME));
        backend.put_word(MODEL + 0x110, 8);
        let mut wrong = state();
        assert_eq!(
            wrong.init_prefix(&mut backend, MODEL),
            Ok(VehicleInitOutcome::Rejected(
                VehicleInitRejection::WrongFrameType
            ))
        );
        assert_eq!(backend.events.len(), 3);
        assert_eq!(wrong.load::<()>(0x34c), Ok(MODEL));
        assert_eq!(wrong.load::<()>(0x4bc), Ok(OLD_NAME));
        assert_eq!(backend.memory[&(MODEL + 4)], 0);
    }

    #[test]
    fn null_allocator_and_missing_body_retain_distinct_ownership() {
        let mut backend = Backend::new();
        backend.allocation = 0;
        let mut unavailable = state();
        assert_eq!(
            unavailable.init_prefix(&mut backend, MODEL),
            Ok(VehicleInitOutcome::Rejected(
                VehicleInitRejection::NameAllocation
            ))
        );
        assert_eq!(unavailable.load::<()>(0x4bc), Ok(0));
        assert_eq!(unavailable.load::<()>(0xe68), Ok(0x7d7d7d7d));
        assert!(!backend
            .events
            .iter()
            .any(|event| matches!(event, Event::Enumerate(_))));
        let mut backend = Backend::new();
        backend.frames.clear();
        let mut missing = state();
        assert_eq!(
            missing.init_prefix(&mut backend, MODEL),
            Ok(VehicleInitOutcome::Rejected(
                VehicleInitRejection::MissingBody
            ))
        );
        assert_eq!(missing.load::<()>(0xe68), Ok(0));
        assert_eq!(missing.load::<()>(0x4bc), Ok(ALLOCATION));
        assert_eq!(missing.scratch(), None);
    }

    #[test]
    fn null_name_uses_empty_literal_and_allocator_reread_is_observed() {
        let mut backend = Backend::new();
        backend.put_word(MODEL + 0x100, 0);
        backend.replace_name_on_allocation = Some(0x6000);
        backend.put_bytes(0x6000, b"changed\0");
        let mut state = state();
        state.init_prefix(&mut backend, MODEL).unwrap();
        assert!(backend.events.contains(&Event::Allocate(2)));
        let copied: Vec<u8> = (0..8)
            .map(|index| backend.memory[&(ALLOCATION + index)])
            .collect();
        assert_eq!(copied, b"changed\0");
        assert!(!backend.memory.contains_key(&(ALLOCATION + 8)));
    }

    #[test]
    fn nonempty_components_stop_before_transform_effects_in_original_order() {
        for (query, expected, count) in [
            (
                VehicleInitQuery::Exhaust,
                VehicleInitFrontier::ExhaustTransform { index: 1 },
                2,
            ),
            (
                VehicleInitQuery::SpecialCollision,
                VehicleInitFrontier::SpecialCollisionTransform,
                2,
            ),
            (
                VehicleInitQuery::Head,
                VehicleInitFrontier::HeadTransform,
                0,
            ),
        ] {
            let mut backend = Backend::new();
            backend.frames.push((query, vec![0x6100, 0x6200]));
            let mut state = state();
            assert_eq!(
                state.init_prefix(&mut backend, MODEL),
                Ok(VehicleInitOutcome::UnavailableFrontier(expected))
            );
            assert_eq!(state.load::<()>(0x2d4), Ok(0x7d7d7d7d));
            assert_eq!(
                state.head(),
                if query == VehicleInitQuery::Head {
                    0x6200
                } else {
                    0
                }
            );
            let vector = state.scratch().unwrap();
            assert_eq!(vector.end.wrapping_sub(vector.begin) / 4, count);
            assert!(backend.callback_results.iter().all(|result| *result == 0));
            assert!(
                !backend
                    .events
                    .contains(&Event::Enumerate(VehicleInitQuery::Head))
                    || query == VehicleInitQuery::Head
            );
        }
    }

    #[test]
    fn failed_effects_keep_access_and_write_prefixes() {
        let mut baseline = Backend::new();
        state().init_prefix(&mut baseline, MODEL).unwrap();
        for index in 0..baseline.events.len() {
            let mut backend = Backend::new();
            backend.fail = Some(index);
            let mut state = state();
            assert_eq!(
                state.init_prefix(&mut backend, MODEL),
                Err(VehicleInitUnavailable::Effect("effect unavailable"))
            );
            assert_eq!(backend.events, baseline.events[..=index]);
            assert_eq!(state.load::<()>(0x34c), Ok(MODEL));
            let allocated = baseline.events[..index]
                .iter()
                .any(|event| matches!(event, Event::Allocate(_)));
            let freed = baseline.events[..index].contains(&Event::Free(OLD_NAME));
            assert_eq!(
                state.load::<()>(0x4bc),
                Ok(if allocated {
                    ALLOCATION
                } else if freed {
                    0
                } else {
                    OLD_NAME
                })
            );
        }
    }

    #[test]
    fn callback_and_storage_unavailability_preserve_completed_mutations() {
        let mut backend = Backend::new();
        backend
            .frames
            .push((VehicleInitQuery::Exhaust, vec![0x6100, 0x6200]));
        let mut baseline = state();
        baseline.init_prefix(&mut backend, MODEL).unwrap();
        let failure = backend
            .events
            .iter()
            .rposition(|event| *event == Event::Collect(0x6200))
            .unwrap();
        let mut backend = Backend::new();
        backend
            .frames
            .push((VehicleInitQuery::Exhaust, vec![0x6100, 0x6200]));
        backend.fail = Some(failure);
        let mut state = state();
        assert_eq!(
            state.init_prefix(&mut backend, MODEL),
            Err(VehicleInitUnavailable::Effect("effect unavailable"))
        );
        assert_eq!(
            state.scratch(),
            Some(VehicleInitFrameVector {
                begin: 0x8000,
                end: 0x8004,
                capacity: 0x9000,
            })
        );
        let mut short = VehicleInitState::from_bytes(vec![0x7d; 0x34f]);
        let mut backend = Backend::new();
        assert_eq!(
            short.init_prefix(&mut backend, MODEL),
            Err(VehicleInitUnavailable::Storage(0x34c))
        );
        assert!(short.as_bytes().iter().all(|byte| *byte == 0x7d));
        assert!(backend.events.is_empty());
    }
}
