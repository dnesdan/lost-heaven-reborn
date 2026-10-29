//! Mission car ownership reuses the established Vehicle compatibility backend.
//! The real base actor remains solely in ActorWorld. This module owns no model,
//! scene or second actor registry; all frame effects borrow the caller's arena.
#![forbid(unsafe_code)]

use crate::{
    car_effects::CarTrackResources,
    car_runtime::CarInitAnimationCatalog,
    frame_transform::FrameKey,
    model_loading::ModelArena,
    script_scheduler::{ActorReferenceId, FrameReferenceId},
    vehicle_bodycol::{VehicleBodycolScratch, VehicleBodycolStorage},
    vehicle_frame_backend::{VehicleFrameBackend, VehicleFrameMemory, VehicleMemoryError},
    vehicle_init::*,
};

#[path = "car_actor_source.rs"]
mod source;

const INIT_STACK: u32 = 0x0080_0000;
const CONSTRUCTOR_STACK: u32 = 0x0082_0000;

/// Explicit allocator/caller bytes supplied by the host allocation boundary.
/// The fill is not a claim that original uninitialized storage was zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarSeed {
    pub allocator_fill: u8,
    pub constructor_allocator_tag: u8,
    pub vehicle_allocator_tag: u8,
}

#[derive(Debug)]
pub struct CarActorState {
    construction: Option<CarConstructionState>,
    vehicle: Option<VehicleInitState>,
    source: Option<crate::car_state_source::CarSourceLoad>,
    identity: u32,
    fill: u8,
    vehicle_allocator_tag: u8,
    vehicle_returned_initialized: bool,
}

/// Read-only provenance for a reached Init automatic vector. These are
/// compatibility allocation identities, never semantic actor/frame pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarInitTemporaryKind {
    Seats,
    Ballast,
    Doors,
    Exhaust,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarInitTemporaryDiagnostic {
    pub kind: CarInitTemporaryKind,
    pub allocation_identity: u32,
    pub allocation_bytes: u32,
}

/// One Game resource/allocator context shared by all real car actors. The only
/// addressable storage is the existing contained vehicle compatibility backend.
/// Actor count, registration, attachment and lifetime never live in this store.
pub(crate) struct CarActorServices {
    memory: VehicleFrameMemory,
    tracks: CarTrackResources,
    precision: crate::native_precision::Pc24PrecisionState,
    tables: crate::car_actor_tables::VehicleTableResources,
}
impl std::fmt::Debug for CarActorServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CarActorServices")
            .field("allocations", &self.memory.allocations().count())
            .finish_non_exhaustive()
    }
}
impl CarActorServices {
    pub(crate) fn new(random_seed: u32) -> Result<Self, String> {
        let mut memory = VehicleFrameMemory::default();
        // Shared native temporary/collision and car-pair lists begin empty in
        // this source-only context. No original gameplay registration is run.
        for (address, bytes) in [
            (0x63a200, random_seed.to_le_bytes().to_vec()),
            (0x63d890, vec![0; 16]),
            // Explicit initial C-locale/single-byte policy, matching the
            // ModelActorFrames default-C admission. Not a generic zero map.
            (0x6d87c4, 0u32.to_le_bytes().to_vec()),
            (0x63a4cc, 1u32.to_le_bytes().to_vec()),
            (0x6bf980 + 0x449c, vec![0; 8]),
            (0x600000, vec![0; 0x200]),
            (INIT_STACK - 0x10000, vec![0; 0xfe30]),
            (INIT_STACK - 8, vec![0; 8]),
        ] {
            memory
                .map_bytes(address, &bytes)
                .map_err(|e| format!("car service storage: {e:?}"))?;
        }
        Ok(Self {
            memory,
            precision: Default::default(),
            tables: Default::default(),
            tracks: CarInitAnimationCatalog::resources()
                .map_err(|e| format!("car resource catalog: {e:?}"))?,
        })
    }
    /// Mission cars share the caller's active Game arithmetic context. A
    /// standalone host invocation owns a fallback only when no Game is active.
    /// Reentering the same Game must not reload an older sticky precision flag.
    pub(crate) fn with_precision<R>(&mut self, operation: impl FnOnce(&mut Self) -> R) -> R {
        if crate::native_precision::current_precision().is_some() {
            operation(self)
        } else {
            crate::native_precision::Pc24PrecisionState::run_owner(
                self,
                |services| &mut services.precision,
                operation,
            )
        }
    }
    pub(crate) fn table_allocations(&self) -> usize {
        self.tables.live_allocation_count()
    }
    pub(crate) fn discard_tables_host(&mut self) -> Result<usize, String> {
        self.tables
            .discard_host(&mut self.memory)
            .map_err(|error| format!("vehicle table host disposal: {error:?}"))
    }
    pub(crate) fn cached_tracks(&self) -> usize {
        self.tracks.cached_tracks().count()
    }
    pub(crate) fn allocations(&self) -> usize {
        self.memory.allocations().count()
    }
}

struct ConstructorBackend<'a>(&'a mut VehicleFrameMemory);
impl CarConstructionBackend for ConstructorBackend<'_> {
    type Error = VehicleMemoryError;
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        VehicleInitBackend::read_word(self.0, address)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        VehicleInitBackend::write_word(self.0, address, value)
    }
    fn allocate_car_construction(
        &mut self,
        _: CarConstructionMemory<'_>,
        bytes: u32,
    ) -> Result<u32, Self::Error> {
        self.0.allocate(bytes)
    }
    fn free_car_construction(
        &mut self,
        _: CarConstructionMemory<'_>,
        allocation: u32,
    ) -> Result<(), Self::Error> {
        self.0.free(allocation)
    }
}
impl CarActorState {
    /// Live original temporary-buffer leases retained by an interrupted host
    /// source operation. Parameter-record scratch is diagnostic caller storage.
    pub fn retained_source_allocation_count(&self) -> usize {
        self.source.as_ref().map_or(0, |source| {
            usize::from(source.compressed().is_some())
                + usize::from(source.decoder().output().is_some())
        })
    }
    /// Terminal host cleanup only, after the ActorWorld car is dead. Dropping
    /// Rust buffers never repeats an uncertain native free or claims GameDone.
    pub(crate) fn discard_source_host(&mut self) {
        self.source = None;
    }
    pub(crate) fn init_temporaries(
        &self,
        services: &CarActorServices,
    ) -> Result<Vec<CarInitTemporaryDiagnostic>, String> {
        let Some(vehicle) = self.vehicle.as_ref() else {
            return Ok(Vec::new());
        };
        // The actual successful Vehicle::Init return proves every automatic
        // vector was freed. Subsequent Car calls reuse this dead stack scope,
        // including overwriting its SEH state with -1. Do not reinterpret it.
        if self.vehicle_returned_initialized {
            return Ok(Vec::new());
        }
        // scratch()/component_scratch() are genuine reached-construction
        // capabilities. Caller fill alone never establishes a vector owner.
        if vehicle.scratch().is_none() {
            return Ok(Vec::new());
        }
        let scratch = vehicle
            .component_tail_scratch()
            .ok_or("missing constructed Init caller storage")?;
        let stage = u32::from_le_bytes(scratch.bytes[0x1c4..0x1c8].try_into().unwrap());
        if stage > 3 {
            return Err("Init temporary construction state outside supported domain".into());
        }
        let mut result = Vec::new();
        for (kind, offset, reached) in [
            (CarInitTemporaryKind::Seats, 0x48, stage >= 3),
            (CarInitTemporaryKind::Ballast, 0x38, stage >= 2),
            (
                CarInitTemporaryKind::Doors,
                0x58,
                stage >= 1 && vehicle.component_scratch().is_some(),
            ),
            (CarInitTemporaryKind::Exhaust, 0x68, true),
        ] {
            if !reached {
                continue;
            }
            let word = |offset: usize| {
                u32::from_le_bytes(scratch.bytes[offset..offset + 4].try_into().unwrap())
            };
            let begin = word(offset + 4);
            if begin == 0 {
                continue;
            }
            // This allocator never reuses identities. Normal final cleanup
            // deliberately leaves Exhaust's old header; it owns no live block.
            let Some((_, bytes)) = services
                .memory
                .allocations()
                .find(|(identity, _)| *identity == begin)
            else {
                continue;
            };
            let bytes = u32::try_from(bytes).map_err(|_| "Init allocation extent")?;
            let end = word(offset + 8);
            let capacity = word(offset + 12);
            if capacity.checked_sub(begin) != Some(bytes)
                || !bytes.is_multiple_of(4)
                || end
                    .checked_sub(begin)
                    .is_none_or(|active| active > bytes || !active.is_multiple_of(4))
            {
                return Err(
                    "Init temporary header does not identify its actual live allocation extent"
                        .into(),
                );
            }
            result.push(CarInitTemporaryDiagnostic {
                kind,
                allocation_identity: begin,
                allocation_bytes: bytes,
            });
        }
        Ok(result)
    }
    pub(crate) fn discard_init_temporaries_host(
        &mut self,
        services: &mut CarActorServices,
    ) -> Result<(), String> {
        // Caller verifies ActorDead first. This is Rust owner disposal, not
        // native Init unwind/free replay; failed prefix bytes remain diagnostic.
        for lease in self.init_temporaries(services)? {
            services
                .memory
                .discard_host_allocation(lease.allocation_identity, lease.allocation_bytes)
                .map_err(|error| format!("Init temporary host disposal: {error:?}"))?;
        }
        Ok(())
    }
    /// Diagnostic borrow of the existing contained Vehicle compatibility owner.
    /// No base actor image is present and no mutation/copyback API is exposed.
    pub fn legacy_vehicle_storage(&self) -> Option<&[u8]> {
        self.vehicle
            .as_ref()
            .and_then(VehicleInitState::known_bytes)
    }
    pub fn known_vehicle_range(&self, offset: usize, width: usize) -> Option<&[u8]> {
        self.vehicle.as_ref()?.known_range(offset, width)
    }
    pub fn vehicle_byte_validity(&self) -> Option<&[bool]> {
        self.vehicle.as_ref().map(VehicleInitState::byte_validity)
    }
    pub(crate) fn allocation(actor: ActorReferenceId, seed: CarSeed) -> Result<Self, String> {
        // A checked, scoped compatibility translation, never another actor ID
        // or an original process pointer. Distinct actors cannot overlap.
        let identity = actor
            .identity()
            .checked_mul(0x10000)
            .and_then(|v| v.checked_add(0x1000_0000))
            .filter(|v| *v < 0x2000_0000 - 0x10000)
            .ok_or("car compatibility identity domain")?;
        let mut scratch = CarConstructionScratch {
            address: CONSTRUCTOR_STACK,
            bytes: [seed.allocator_fill; CAR_CONSTRUCTION_STACK_BYTES],
        };
        scratch.bytes[0xf] = seed.constructor_allocator_tag;
        let construction = CarConstructionState::from_actor_allocator_tail(
            identity,
            vec![seed.allocator_fill; 0x221c - 0x70],
            scratch,
            CarConstructionRegisters {
                ebx: 0,
                ebp: 0,
                esi: 0,
                edi: 0,
                seh: 0,
            },
            CarInitScratch {
                address: INIT_STACK,
                bytes: [seed.allocator_fill; CAR_INIT_STACK_BYTES],
            },
            CarInitRegisters {
                ebx: 0,
                ebp: 0,
                esi: 0,
                edi: 0,
            },
        )
        .map_err(|e| format!("car allocation: {e:?}"))?;
        Ok(Self {
            construction: Some(construction),
            vehicle: None,
            source: None,
            identity,
            fill: seed.allocator_fill,
            vehicle_allocator_tag: seed.vehicle_allocator_tag,
            vehicle_returned_initialized: false,
        })
    }
    pub(crate) fn construct(
        &mut self,
        services: &mut CarActorServices,
        publish: impl FnMut() -> Result<(), VehicleMemoryError>,
    ) -> Result<(), String> {
        self.construction
            .as_mut()
            .ok_or("car construction already transferred")?
            .construct_for_actor(
                &mut ConstructorBackend(&mut services.memory),
                self.vehicle_allocator_tag,
                publish,
            )
            .map_err(|e| format!("car construction: {e:?}"))?;
        self.vehicle = Some(
            self.construction
                .take()
                .unwrap()
                .into_car()
                .map_err(|state| {
                    self.construction = Some(state);
                    "car construction incomplete"
                })?,
        );
        Ok(())
    }
    fn storage(&self) -> VehicleComponentTailStorage {
        VehicleComponentTailStorage {
            bodycol: VehicleBodycolStorage {
                vehicle_address: self.identity + 0x70,
                callback_scratch: VehicleBodycolScratch {
                    record: 0x600000,
                    leading_byte: self.fill,
                    nested_byte: self.fill,
                },
                descendant_scratch: 0x600100,
            },
            scratch: VehicleComponentTailScratch {
                address: INIT_STACK - 0x1d0,
                bytes: [self.fill; VEHICLE_INIT_STACK_BYTES],
            },
        }
    }
    pub(crate) fn prepare_init(
        &mut self,
        services: &mut CarActorServices,
        arena: &mut ModelArena,
        frame: Option<FrameReferenceId>,
    ) -> Result<u32, String> {
        let model = match frame {
            Some(frame) => {
                let key = FrameKey(frame.identity());
                if arena.lookup_reference(key) != frame {
                    return Err("stale car model reference".into());
                }
                VehicleFrameBackend::new(arena, &mut services.memory)
                    .frame_address(key)
                    .map_err(|e| format!("car model: {e:?}"))?
            }
            None => 0,
        };
        let storage = self.storage();
        let vehicle = self.vehicle.as_mut().ok_or("car constructor incomplete")?;
        vehicle
            .set_actor_car_init_model(model)
            .map_err(|e| format!("car caller frame: {e:?}"))?;
        vehicle
            .prepare_car_init_before_base(&mut services.memory, self.identity, model, &storage)
            .map_err(|e| format!("car Init prelude: {e:?}"))?;
        Ok(model)
    }
    pub(crate) fn finish_init(
        &mut self,
        services: &mut CarActorServices,
        arena: &mut ModelArena,
        installation: Option<&lhr_assets::Installation>,
        model: u32,
    ) -> Result<bool, String> {
        services
            .with_precision(|services| self.finish_init_inner(services, arena, installation, model))
    }
    fn finish_init_inner(
        &mut self,
        services: &mut CarActorServices,
        arena: &mut ModelArena,
        installation: Option<&lhr_assets::Installation>,
        model: u32,
    ) -> Result<bool, String> {
        let storage = self.storage();
        let vehicle = self.vehicle.as_mut().ok_or("car constructor incomplete")?;
        vehicle
            .finish_car_init_after_base(&mut services.memory, self.identity, model)
            .map_err(|e| format!("car Init caller: {e:?}"))?;
        let backend = match installation {
            Some(source) => {
                VehicleFrameBackend::with_installation(arena, &mut services.memory, source)
            }
            None => VehicleFrameBackend::new(arena, &mut services.memory),
        };
        let mut backend = backend
            .with_car_track_resources(&mut services.tracks)
            .with_vehicle_tables(&mut services.tables);
        vehicle
            .init_car_after_actor(&mut backend, model, storage, || {
                self.vehicle_returned_initialized = true
            })
            .map(|outcome| outcome == CarInitOutcome::Initialized)
            .map_err(|e| format!("car Init: {e:?}"))
    }
    pub(crate) fn destroy_derived(
        &mut self,
        services: &mut CarActorServices,
        arena: &mut ModelArena,
    ) -> Result<(), String> {
        let storage = self.storage();
        let vehicle = self.vehicle.as_mut().ok_or("car constructor incomplete")?;
        vehicle
            .prepare_actor_car_destruction(storage)
            .map_err(|e| format!("car destruction context: {e:?}"))?;
        vehicle
            .destruct_car_derived(&mut VehicleFrameBackend::new(arena, &mut services.memory))
            .map_err(|e| format!("car destructor: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_lookup::ActorLifetime;

    fn car(identity: u32, fill: u8) -> CarActorState {
        CarActorState::allocation(
            ActorLifetime::for_existing(identity).reference(),
            CarSeed {
                allocator_fill: fill,
                constructor_allocator_tag: 0x79,
                vehicle_allocator_tag: 0xb3,
            },
        )
        .unwrap()
    }
    #[test]
    fn actual_actor_tail_has_no_base_image_and_shared_random_advances_between_cars() {
        let mut services = CarActorServices::new(1).unwrap();
        let mut first = car(1, 0xa5);
        let mut second = car(2, 0x3c);
        let mut publications = 0;
        first
            .construct(&mut services, || {
                publications += 1;
                Ok(())
            })
            .unwrap();
        second
            .construct(&mut services, || {
                publications += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(publications, 2);
        assert_ne!(first.identity, second.identity);
        for (state, expected) in [(&first, 6u32), (&second, 7u32)] {
            let vehicle = state.vehicle.as_ref().unwrap();
            assert!(vehicle.car_bytes().is_none()); // No cloned base image.
            let offset = 0x2158 - 0x70;
            assert_eq!(
                u32::from_le_bytes(vehicle.as_bytes()[offset..offset + 4].try_into().unwrap()),
                expected
            );
            assert_eq!(vehicle.as_bytes()[0x21b0 - 0x70], 0x79);
            for offset in [
                0xd8, 0xe8, 0xfc, 0x10c, 0x204, 0x214, 0x32c, 0x33c, 0x450, 0x4ac, 0xca0, 0xcb0,
                0xcc0, 0x1fb0, 0x1fc0, 0x1fd0, 0x1fe0, 0x1ff8,
            ] {
                assert_eq!(vehicle.as_bytes()[offset], 0xb3);
            }
        }
        assert_eq!(services.allocations(), 0);
    }
    #[test]
    fn constructor_complete_cleanup_needs_no_init_and_cannot_replay() {
        let mut services = CarActorServices::new(7).unwrap();
        let mut state = car(3, 0xa5);
        state.construct(&mut services, || Ok(())).unwrap();
        let mut arena = ModelArena::default();
        state.destroy_derived(&mut services, &mut arena).unwrap();
        assert_eq!(services.allocations(), 0);
        assert!(state
            .destroy_derived(&mut services, &mut arena)
            .unwrap_err()
            .contains("AlreadyStarted"));
    }
    #[test]
    fn null_init_keeps_reached_prelude_then_explicit_cleanup() {
        let mut services = CarActorServices::new(7).unwrap();
        let mut state = car(4, 0xd7);
        state.construct(&mut services, || Ok(())).unwrap();
        let mut arena = ModelArena::default();
        assert_eq!(
            state.prepare_init(&mut services, &mut arena, None).unwrap(),
            0
        );
        let bytes = state.vehicle.as_ref().unwrap().as_bytes();
        assert_eq!(&bytes[0x21d8 - 0x70..0x21dc - 0x70], &[0; 4]);
        assert_eq!(&bytes[0x20bc - 0x70..0x20c0 - 0x70], &[0xd7; 4]);
        assert!(state.prepare_init(&mut services, &mut arena, None).is_err());
        state.destroy_derived(&mut services, &mut arena).unwrap();
        assert_eq!(services.allocations(), 0);
    }
    #[test]
    fn real_source_bridge_converts_owned_record_and_ignores_native_apply_rejection() {
        let mut services = CarActorServices::new(1).unwrap();
        let mut state = car(5, 0);
        state.construct(&mut services, || Ok(())).unwrap();
        let mut arena = ModelArena::default();
        state.prepare_init(&mut services, &mut arena, None).unwrap();
        // A complete synthetic packed record. Zero parameters reach the actual
        // Vehicle loader's first positive-value rejection; outer LoadData must
        // still return true after genuine conversion and parameter prefix writes.
        let packed = vec![0; 4385];
        let mut compressed = b"COMP".to_vec();
        compressed.extend(4385u32.to_le_bytes());
        compressed.extend((4385u32 + 13).to_le_bytes());
        compressed.push(0xff);
        compressed.extend(packed);
        let mut payload = 1u32.to_le_bytes().to_vec();
        payload.extend((compressed.len() as u32).to_le_bytes());
        payload.extend(compressed);
        let mut reader = crate::human_actor::ActorDataReader::new(&payload);
        assert_eq!(
            state.load_source(&mut services, &mut arena, &mut reader),
            Ok(true)
        );
        assert_eq!(reader.position(), payload.len());
        let source = state.source.as_ref().unwrap();
        assert_eq!(
            source.stage(),
            crate::car_state_source::CarSourceStage::Complete
        );
        assert!(source.compressed().is_none());
        assert!(source.decoder().output().is_none());
        assert_eq!(source.parameters().read(0, 4), Some([0; 4].as_slice()));
        let bytes = state.legacy_vehicle_storage().unwrap();
        assert_eq!(&bytes[0x20bc - 0x70..0x20c0 - 0x70], &[0; 4]);
        let mut repeated = crate::human_actor::ActorDataReader::new(&payload);
        assert_eq!(
            state.load_source(&mut services, &mut arena, &mut repeated),
            Ok(true)
        );
        state.destroy_derived(&mut services, &mut arena).unwrap();
        assert_eq!(services.allocations(), 0);
    }
    #[test]
    fn interrupted_decoder_retains_buffers_through_native_destructor_until_host_discard() {
        let mut services = CarActorServices::new(1).unwrap();
        let mut state = car(6, 0);
        state.construct(&mut services, || Ok(())).unwrap();
        let mut arena = ModelArena::default();
        state.prepare_init(&mut services, &mut arena, None).unwrap();
        // Original decoder publishes an allocation, then the declared input
        // end reaches outside this actual serialized buffer.
        let mut compressed = b"COMP".to_vec();
        compressed.extend(4u32.to_le_bytes());
        compressed.extend(14u32.to_le_bytes());
        compressed.push(0xff);
        let mut payload = 1u32.to_le_bytes().to_vec();
        payload.extend((compressed.len() as u32).to_le_bytes());
        payload.extend(compressed);
        let mut reader = crate::human_actor::ActorDataReader::new(&payload);
        assert!(state
            .load_source(&mut services, &mut arena, &mut reader)
            .is_err());
        assert_eq!(state.retained_source_allocation_count(), 2);
        state.destroy_derived(&mut services, &mut arena).unwrap();
        assert_eq!(state.retained_source_allocation_count(), 2);
        state.discard_source_host();
        assert_eq!(state.retained_source_allocation_count(), 0);
        assert!(state.legacy_vehicle_storage().is_some());
        assert_eq!(services.allocations(), 0);
    }
    #[test]
    fn mission_car_precision_inherits_active_flag_and_propagates_sticky_error_prefix() {
        use crate::native_precision::{current_precision, div, Pc24PrecisionState};
        let mut services = CarActorServices::new(1).unwrap();
        let mut outer = Pc24PrecisionState::new(true);
        outer.run(|| services.with_precision(|_| assert_eq!(current_precision(), Some(true))));
        assert!(outer.precision_raised());
        assert!(!services.precision.precision_raised());
        let mut outer = Pc24PrecisionState::new(false);
        let result: Result<(), &str> = outer.run(|| {
            let result = services.with_precision(|_| {
                assert_eq!(current_precision(), Some(false));
                assert_eq!(div(1., 3.).to_bits(), 0x3eaaaaab);
                Err("reached caller interruption")
            });
            assert_eq!(current_precision(), Some(true));
            result
        });
        assert_eq!(result, Err("reached caller interruption"));
        assert!(outer.precision_raised());
        assert!(!services.precision.precision_raised());
        assert_eq!(current_precision(), None);
    }
    #[test]
    fn standalone_car_precision_owns_fallback_and_restores_absent_tls() {
        use crate::native_precision::{current_precision, div};
        let mut services = CarActorServices::new(1).unwrap();
        assert_eq!(current_precision(), None);
        services.with_precision(|_| {
            assert_eq!(current_precision(), Some(false));
            div(1., 3.);
        });
        assert!(services.precision.precision_raised());
        assert_eq!(current_precision(), None);
        services.with_precision(|_| assert_eq!(current_precision(), Some(true)));
        assert_eq!(current_precision(), None);
    }

    #[test]
    fn arbitrary_constructor_and_null_init_fill_never_establishes_temporary_ownership() {
        for fill in [0, 0x3c, 0xa5, 0xd7] {
            let mut services = CarActorServices::new(1).unwrap();
            let mut state = car(7, fill);
            state.construct(&mut services, || Ok(())).unwrap();
            assert!(state.init_temporaries(&services).unwrap().is_empty());
            state
                .prepare_init(&mut services, &mut ModelArena::default(), None)
                .unwrap();
            assert!(state.init_temporaries(&services).unwrap().is_empty());
        }
    }
    #[test]
    fn retired_vehicle_scope_never_reinterprets_stale_exhaust_or_frees_unrelated_heap() {
        let mut services = CarActorServices::new(1).unwrap();
        let mut state = car(8, 0);
        state.construct(&mut services, || Ok(())).unwrap();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        for name in [b"BODY".as_slice(), b"EXHAUST1".as_slice()] {
            let key = arena
                .insert(
                    crate::frame_transform::I3dFrameState::dummy(),
                    Some(0),
                    name.to_vec(),
                    Vec::new(),
                    Some([[-1.; 3], [1.; 3]]),
                )
                .unwrap();
            arena.add_frame(model, key).unwrap();
            arena.release(key).unwrap();
            arena.link_frame(key, Some(model.root()), 0).unwrap();
        }
        let frame = arena.lookup_reference(model.root());
        let pointer = state
            .prepare_init(&mut services, &mut arena, Some(frame))
            .unwrap();
        let outcome = state
            .vehicle
            .as_mut()
            .unwrap()
            .init_prefix(
                &mut VehicleFrameBackend::new(&mut arena, &mut services.memory),
                pointer,
            )
            .unwrap();
        assert!(matches!(
            outcome,
            VehicleInitOutcome::UnavailableFrontier(_)
        ));
        let leases = state.init_temporaries(&services).unwrap();
        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].kind, CarInitTemporaryKind::Exhaust);
        // Arrange the native return boundary: its final free leaves Exhaust's
        // header intact. The later caller reuses the dead SEH state word.
        services.memory.free(leases[0].allocation_identity).unwrap();
        let unrelated = services
            .memory
            .allocate(leases[0].allocation_bytes)
            .unwrap();
        assert_ne!(unrelated, leases[0].allocation_identity);
        state
            .vehicle
            .as_mut()
            .unwrap()
            .live_tail_memory()
            .write_word_with_backend(&mut services.memory, INIT_STACK - 0x1d0 + 0x1c4, u32::MAX)
            .unwrap();
        assert!(state.init_temporaries(&services).is_err()); // No proof of return yet.
        state.vehicle_returned_initialized = true;
        assert!(state.init_temporaries(&services).unwrap().is_empty());
        state.discard_init_temporaries_host(&mut services).unwrap();
        assert!(services.memory.allocations().any(|(id, _)| id == unrelated));
        state.destroy_derived(&mut services, &mut arena).unwrap();
        services
            .memory
            .discard_host_allocation(unrelated, leases[0].allocation_bytes)
            .unwrap();
        assert_eq!(services.allocations(), 0);
        arena.close(model).unwrap();
    }
}
