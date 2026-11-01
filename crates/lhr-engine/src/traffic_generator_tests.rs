//! Authored synthetic streams compared to the private eight-case original x86
//! probe. No instruction bytes, original input or decompiler output is included.
use super::*;
use crate::traffic_generator::{TrafficChunkReader, TrafficHeap};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn seed() -> TrafficGeneratorSeed {
    TrafficGeneratorSeed {
        bytes_71_73: [0xa5; 3],
        word_78: 0xa5a5a5a5,
        words_80_84: [0xa5a5a5a5; 2],
        word_a4: 0xa5a5a5a5,
        words_ac_b4: [0xa5a5a5a5; 3],
    }
}
fn base() -> BaseActorSeed {
    BaseActorSeed {
        word_0e: 0xa5a5,
        actor_id: 0xa5a5a5a5,
        state_1c: 0xa5a5a5a5,
        bytes_21_5f: [0xa5; 63],
        bytes_64_65: [0xa5; 2],
        byte_67: 0xa5,
        release_frame: 0,
        bytes_6d_6f: [0xa5; 3],
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Event {
    Allocate(TrafficBackingKind, u32),
    Free(TrafficBackingKind, Option<TrafficAllocationId>),
    FreeActor,
}
#[derive(Default)]
struct Heap {
    resources: TrafficHeap,
    events: Vec<Event>,
    fail_at: Option<usize>,
    panic: bool,
    null_kind: Option<TrafficBackingKind>,
    null_actor: bool,
    frame_data: Option<FrameActorData>,
    fail_find: bool,
    fail_release_frame: bool,
}
impl Heap {
    fn after_effect(&self) -> Result<(), String> {
        if self.fail_at == Some(self.events.len()) {
            assert!(!self.panic, "post-effect interruption");
            return Err("post-effect interruption".into());
        }
        Ok(())
    }
}
impl ActorCollectionBackend for Heap {
    fn allocate_collection(
        &mut self,
        _: &ActorWorld,
        _: ActorSource,
        _: u32,
    ) -> Result<ActorStorageId, String> {
        Ok(ActorStorageId(99))
    }
    fn free_collection(
        &mut self,
        _: &ActorWorld,
        _: ActorSource,
        _: Option<ActorStorageId>,
    ) -> Result<(), String> {
        Ok(())
    }
}
impl ActorAllocationBackend for Heap {
    fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
        unreachable!()
    }
    fn allocate_traffic_generator(
        &mut self,
        bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, TrafficGeneratorSeed)>, String> {
        assert_eq!(bytes, 0xb8);
        if self.null_actor {
            return Ok(None);
        }
        Ok(Some((0x10000, base(), seed())))
    }
    fn allocate_traffic_backing(
        &mut self,
        _: &ActorWorld,
        actor: ActorReferenceId,
        kind: TrafficBackingKind,
        bytes: u32,
    ) -> Result<Option<TrafficAllocationId>, String> {
        self.events.push(Event::Allocate(kind, bytes));
        let allocation = if self.null_kind == Some(kind) {
            None
        } else {
            Some(self.resources.allocate(actor, kind, bytes)?)
        };
        self.after_effect()?;
        Ok(allocation)
    }
}
impl ActorDeallocationBackend for Heap {
    fn free_actor(&mut self, _: &ActorWorld, _: u32) -> Result<(), String> {
        self.events.push(Event::FreeActor);
        self.after_effect()
    }
    fn free_traffic_backing(
        &mut self,
        _: &ActorWorld,
        actor: ActorReferenceId,
        kind: TrafficBackingKind,
        allocation: Option<TrafficAllocationId>,
    ) -> Result<(), String> {
        self.events.push(Event::Free(kind, allocation));
        self.resources.free(actor, kind, allocation)?;
        self.after_effect()
    }
    fn discard_traffic_backings_host(
        &mut self,
        _: &ActorWorld,
        actor: ActorReferenceId,
    ) -> Result<(), String> {
        self.resources.discard_actor_host(actor);
        Ok(())
    }
}
impl ActorFrameBackend for Heap {
    fn actor_data(
        &mut self,
        _: &mut ActorWorld,
        _: FrameReferenceId,
    ) -> Result<FrameActorData, String> {
        Ok(self.frame_data.unwrap_or(FrameActorData::Null))
    }
    fn clear_actor_data(&mut self, _: &mut ActorWorld, _: FrameReferenceId) -> Result<(), String> {
        self.frame_data = Some(FrameActorData::Null);
        Ok(())
    }
    fn find_child(
        &mut self,
        _: &mut ActorWorld,
        _: FrameReferenceId,
        _: &[u8],
        _: u32,
    ) -> Result<Option<FrameReferenceId>, String> {
        assert!(!self.fail_find, "find unwind");
        Ok(None)
    }
    fn set_child_enabled(
        &mut self,
        _: &mut ActorWorld,
        _: FrameReferenceId,
        _: u32,
    ) -> Result<(), String> {
        unreachable!()
    }
    fn release_frame(&mut self, _: &mut ActorWorld, _: FrameReferenceId) -> Result<(), String> {
        assert!(!self.fail_release_frame, "release frame unwind");
        Ok(())
    }
}
fn fixture() -> (ActorWorld, ActorReferenceId, Heap) {
    let backing = SourceBacking {
        storage: Some(ActorStorageId(1)),
        capacity: 8,
    };
    let mut world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
    let mut heap = Heap::default();
    let actor = world.create_actor(18, &mut heap).unwrap().unwrap();
    (world, actor, heap)
}
fn payload(version: u32, count: u32) -> Vec<u8> {
    let mut bytes = version.to_le_bytes().to_vec();
    bytes.push(0x7e);
    for word in [
        0x3f800000u32,
        0x40000000,
        0x40400000,
        0x40800000,
        0x40a00000,
    ] {
        bytes.extend(word.to_le_bytes());
    }
    if version == 1 {
        bytes.extend(0x87654321u32.to_le_bytes());
    }
    bytes.extend(count.to_le_bytes());
    for i in 0..count {
        for j in 0..17 {
            bytes.push((i * 17 + j) as u8);
        }
    }
    for i in 0..count {
        bytes.extend(if i == 0 { 0xfffffff0u32 } else { 0x30 }.to_le_bytes());
    }
    bytes
}
fn finish(world: &mut ActorWorld, actor: ActorReferenceId, heap: &mut Heap) {
    world
        .delete_traffic_generator(actor, 1, &mut Heap::default(), heap)
        .unwrap();
    assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
    assert_eq!(world.actor_count, 0);
    assert!(world.source(ActorSource::Static).is_empty());
    assert_eq!(heap.resources.live_allocations(), 0);
}
#[test]
fn native_constructor_and_unknown_version_retain_exact_scalar_and_owner_state() {
    let (mut world, actor, mut heap) = fixture();
    let traffic = world.traffic_generator(actor).unwrap();
    assert_eq!(traffic.untouched_seed(), &seed());
    assert_eq!(traffic.enabled_byte(), Some(1));
    assert_eq!(
        traffic.scalar_bits().each_ref().map(TrafficWord::bits),
        [
            Some(0x42c80000),
            Some(0x42c80000),
            Some(0x42480000),
            Some(0x42b40000),
            Some(0x42480000)
        ]
    );
    assert_eq!(traffic.constructor_zero(), 0);
    assert_eq!(traffic.count_bits(), Some(0));
    assert_eq!(traffic.total_bits(), Some(10));
    assert_eq!(
        world.state(actor).unwrap().vtable,
        ActorVtable::TrafficGenerator
    );
    assert_eq!(world.state(actor).unwrap().word_10, 18);
    let unknown = 9u32.to_le_bytes();
    let mut reader = TrafficChunkReader::new(&unknown);
    assert!(world
        .load_traffic_generator_data(actor, &mut reader, &mut heap)
        .unwrap());
    assert_eq!(reader.position(), 4);
    assert!(heap.events.is_empty());
    assert_eq!(
        world.traffic_generator(actor).unwrap().total_bits(),
        Some(10)
    );
    finish(&mut world, actor, &mut heap);
}
#[test]
fn native_version_zero_and_one_replace_reverse_free_and_destruct_without_resetting_count() {
    for version in [0, 1] {
        let (mut world, actor, mut heap) = fixture();
        let old = payload(1, 2);
        world
            .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&old), &mut heap)
            .unwrap();
        let pointers = world.traffic_generator(actor).unwrap().records().unwrap();
        let pointer_id = pointers.allocation;
        let records: Vec<_> = pointers
            .records()
            .iter()
            .map(|r| match r {
                TrafficRecordSlot::Owned(r) => r.allocation,
                _ => unreachable!(),
            })
            .collect();
        let weight_id = world
            .traffic_generator(actor)
            .unwrap()
            .weights()
            .unwrap()
            .allocation;
        heap.events.clear();
        let bytes = payload(version, 2);
        let mut reader = TrafficChunkReader::new(&bytes);
        assert!(world
            .load_traffic_generator_data(actor, &mut reader, &mut heap)
            .unwrap());
        assert_eq!(reader.position(), if version == 0 { 71 } else { 75 });
        assert_eq!(
            &heap.events[..3],
            &[
                Event::Free(TrafficBackingKind::Record, Some(records[1])),
                Event::Free(TrafficBackingKind::Record, Some(records[0])),
                Event::Free(TrafficBackingKind::RecordPointers, Some(pointer_id))
            ]
        );
        assert_eq!(
            heap.events[6],
            Event::Free(TrafficBackingKind::Weights, Some(weight_id))
        );
        let traffic = world.traffic_generator(actor).unwrap();
        assert_eq!(
            traffic.total_bits(),
            Some(if version == 0 { 0x20 } else { 0x87654321 })
        );
        assert_eq!(traffic.count_bits(), Some(2));
        assert_eq!(traffic.enabled_byte(), Some(0x7e));
        assert_eq!(traffic.untouched_seed().word_78, 0xa5a5a5a5);
        for (i, slot) in traffic.records().unwrap().records().iter().enumerate() {
            let TrafficRecordSlot::Owned(record) = slot else {
                panic!()
            };
            assert_eq!(
                *record.bytes(),
                std::array::from_fn(|j| Some((i * 17 + j) as u8))
            );
        }
        assert_eq!(traffic.weights().unwrap().word(0), Some(0xfffffff0));
        assert_eq!(traffic.weights().unwrap().word(1), Some(0x30));
        let unknown = 5u32.to_le_bytes();
        let count = heap.events.len();
        world
            .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&unknown), &mut heap)
            .unwrap();
        assert_eq!(heap.events.len(), count);
        heap.events.clear();
        finish(&mut world, actor, &mut heap);
        assert!(matches!(
            heap.events.as_slice(),
            [
                Event::Free(TrafficBackingKind::Record, Some(_)),
                Event::Free(TrafficBackingKind::Record, Some(_)),
                Event::Free(TrafficBackingKind::RecordPointers, Some(_)),
                Event::Free(TrafficBackingKind::Weights, Some(_)),
                Event::FreeActor
            ]
        ));
        assert_eq!(
            world.inspect_traffic_generator(actor).unwrap().count_bits(),
            Some(2)
        );
    }
}
#[test]
fn native_bounded_records_skip_then_weights_read_at_same_cursor() {
    let (mut world, actor, mut heap) = fixture();
    let full = payload(0, 2);
    let mut bytes = full[..29].to_vec();
    bytes.extend(0xfffffff0u32.to_le_bytes());
    bytes.extend(0x30u32.to_le_bytes());
    let mut reader = TrafficChunkReader::new(&bytes);
    world
        .load_traffic_generator_data(actor, &mut reader, &mut heap)
        .unwrap();
    assert_eq!(reader.position(), 37);
    let traffic = world.traffic_generator(actor).unwrap();
    assert_eq!(traffic.total_bits(), Some(0x20));
    for slot in traffic.records().unwrap().records() {
        let TrafficRecordSlot::Owned(record) = slot else {
            panic!()
        };
        assert_eq!(record.bytes(), &[None; 17]);
    }
    finish(&mut world, actor, &mut heap);
}
#[test]
fn native_physical_short_record_retains_unknown_suffix_and_version_one_total() {
    let (mut world, actor, mut heap) = fixture();
    let full = payload(1, 1);
    let mut reader = TrafficChunkReader::with_declared_end(&full[..41], 128);
    world
        .load_traffic_generator_data(actor, &mut reader, &mut heap)
        .unwrap();
    assert_eq!(reader.position(), 41);
    let traffic = world.traffic_generator(actor).unwrap();
    let TrafficRecordSlot::Owned(record) = &traffic.records().unwrap().records()[0] else {
        panic!()
    };
    assert_eq!(&record.bytes()[..8], &(0..8).map(Some).collect::<Vec<_>>());
    assert_eq!(&record.bytes()[8..], &[None; 9]);
    assert_eq!(traffic.weights().unwrap().bytes(), &[None; 4]);
    assert_eq!(traffic.total_bits(), Some(0x87654321));
    finish(&mut world, actor, &mut heap);
}
#[test]
fn unknown_reached_version_or_weight_stops_after_its_actual_prefix() {
    for bytes in [vec![0, 0], payload(0, 1)[..37].to_vec()] {
        let (mut world, actor, mut heap) = fixture();
        let mut reader = TrafficChunkReader::with_declared_end(&bytes, 128);
        assert!(world
            .load_traffic_generator_data(actor, &mut reader, &mut heap)
            .is_err());
        assert_eq!(world.phase(actor), Ok(ActorPhase::LoadingInterrupted));
        let reached = heap.events.len();
        assert!(world
            .load_traffic_generator_data(actor, &mut reader, &mut heap)
            .is_err());
        assert_eq!(heap.events.len(), reached);
        world
            .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
            .unwrap();
        assert_eq!(heap.resources.live_allocations(), 0);
        assert_eq!(world.actor_count, 0);
    }
}
#[test]
fn each_post_effect_heap_error_and_unwind_latches_without_replay_and_host_discards_actual_leases() {
    // The replacement has eight real allocator/free effects; post-effect
    // failures include old frees and allocations not yet returned/published.
    for panic in [false, true] {
        for fail_at in 1..=8 {
            let (mut world, actor, mut heap) = fixture();
            let bytes = payload(1, 2);
            world
                .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
                .unwrap();
            heap.events.clear();
            heap.fail_at = Some(fail_at);
            heap.panic = panic;
            let result = catch_unwind(AssertUnwindSafe(|| {
                world.load_traffic_generator_data(
                    actor,
                    &mut TrafficChunkReader::new(&bytes),
                    &mut heap,
                )
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            assert_eq!(world.phase(actor), Ok(ActorPhase::LoadingInterrupted));
            let reached = heap.events.len();
            assert!(world
                .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
                .is_err());
            assert_eq!(heap.events.len(), reached);
            heap.fail_at = None;
            world
                .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
                .unwrap();
            assert_eq!(heap.resources.live_allocations(), 0);
            assert_eq!(world.actor_count, 0);
        }
    }
}
#[test]
fn each_derived_destructor_free_error_and_unwind_prevents_replay() {
    for panic in [false, true] {
        for fail_at in 1..=4 {
            let (mut world, actor, mut heap) = fixture();
            let bytes = payload(1, 2);
            world
                .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
                .unwrap();
            heap.events.clear();
            heap.fail_at = Some(fail_at);
            heap.panic = panic;
            let result = catch_unwind(AssertUnwindSafe(|| {
                world.destroy_traffic_generator(actor, &mut Heap::default(), &mut heap)
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            assert_eq!(world.phase(actor), Ok(ActorPhase::DestructionInterrupted));
            let reached = heap.events.len();
            assert!(world
                .destroy_traffic_generator(actor, &mut Heap::default(), &mut heap)
                .is_err());
            assert_eq!(heap.events.len(), reached);
            heap.fail_at = None;
            world
                .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
                .unwrap();
            assert_eq!(heap.resources.live_allocations(), 0);
            assert_eq!(world.actor_count, 0);
        }
    }
}
#[test]
fn traffic_init_is_actual_base_and_caught_unwind_keeps_attachment_prefix() {
    use crate::frame_lookup::FrameLifetime;
    let frame = FrameLifetime::for_existing(77);
    let (mut world, actor, mut heap) = fixture();
    assert_eq!(world.init(actor, None, &mut heap), Ok(false));
    heap.fail_find = true;
    assert!(catch_unwind(AssertUnwindSafe(|| world.init(
        actor,
        Some(frame.reference()),
        &mut heap
    )))
    .is_err());
    assert_eq!(
        world.phase(actor),
        Ok(ActorPhase::InitializationInterrupted)
    );
    assert_eq!(world.state(actor).unwrap().frame, Some(frame.reference()));
    assert_eq!(world.attachment_counter, 1);
    assert!(world
        .init(actor, Some(frame.reference()), &mut heap)
        .is_err());
    heap.fail_find = false;
    world
        .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
        .unwrap();
    assert_eq!(world.actor_count, 0);
}

#[test]
fn reader_post_write_error_and_unwind_retain_live_scalar_record_and_weight_bytes() {
    struct Reader<'a> {
        inner: TrafficChunkReader<'a>,
        calls: usize,
        stop: usize,
        panic: bool,
    }
    impl TrafficDataReader for Reader<'_> {
        fn read_bounded(&mut self, destination: &mut [Option<u8>]) -> Result<bool, String> {
            self.calls += 1;
            let result = self.inner.read_bounded(destination)?;
            if self.calls == self.stop {
                destination[0] = Some(0x99);
                assert!(!self.panic, "read post-write unwind");
                return Err("read post-write error".into());
            }
            Ok(result)
        }
    }
    for panic in [false, true] {
        for stop in [3, 10, 11] {
            let (mut world, actor, mut heap) = fixture();
            let bytes = payload(1, 1);
            let mut reader = Reader {
                inner: TrafficChunkReader::new(&bytes),
                calls: 0,
                stop,
                panic,
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                world.load_traffic_generator_data(actor, &mut reader, &mut heap)
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            assert_eq!(world.phase(actor), Ok(ActorPhase::LoadingInterrupted));
            let traffic = world.traffic_generator(actor).unwrap();
            match stop {
                3 => assert_eq!(traffic.scalar_bits()[0].bits(), Some(0x3f800099)),
                10 => {
                    let TrafficRecordSlot::Owned(record) = &traffic.records().unwrap().records()[0]
                    else {
                        panic!()
                    };
                    assert_eq!(record.bytes()[0], Some(0x99));
                }
                11 => assert_eq!(traffic.weights().unwrap().word(0), Some(0xffffff99)),
                _ => unreachable!(),
            }
            world
                .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
                .unwrap();
            assert_eq!(heap.resources.live_allocations(), 0);
            assert_eq!(world.actor_count, 0);
        }
    }
}
#[test]
fn null_allocations_preserve_reached_publication_and_null_read_bound_skips() {
    for kind in [
        TrafficBackingKind::RecordPointers,
        TrafficBackingKind::Record,
        TrafficBackingKind::Weights,
    ] {
        let (mut world, actor, mut heap) = fixture();
        heap.null_kind = Some(kind);
        let bytes = payload(1, 1);
        assert!(world
            .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
            .is_err());
        let traffic = world.traffic_generator(actor).unwrap();
        match kind {
            TrafficBackingKind::RecordPointers => {
                assert!(traffic.records().is_none());
                assert!(traffic.pending_record.is_some());
            }
            TrafficBackingKind::Record => assert!(matches!(
                traffic.records().unwrap().records()[0],
                TrafficRecordSlot::Null
            )),
            TrafficBackingKind::Weights => assert!(traffic.weights().is_none()),
        }
        world
            .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
            .unwrap();
        assert_eq!(heap.resources.live_allocations(), 0);
    }
    let (mut world, actor, mut heap) = fixture();
    heap.null_kind = Some(TrafficBackingKind::Record);
    let full = payload(0, 1);
    let mut bytes = full[..29].to_vec();
    bytes.extend(3u32.to_le_bytes());
    let mut reader = TrafficChunkReader::new(&bytes);
    // The null record is never dereferenced: its 17-byte bound check fails.
    assert!(world
        .load_traffic_generator_data(actor, &mut reader, &mut heap)
        .unwrap());
    assert_eq!(reader.position(), 33);
    assert_eq!(
        world.traffic_generator(actor).unwrap().total_bits(),
        Some(3)
    );
    finish(&mut world, actor, &mut heap);
}
#[test]
fn count_bounds_and_count_backing_mismatch_stop_only_at_reached_allocation_or_slot() {
    for count in [u32::MAX, 0x40000000] {
        let (mut world, actor, mut heap) = fixture();
        let mut bytes = payload(1, 0);
        bytes[29..33].copy_from_slice(&count.to_le_bytes());
        assert!(world
            .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
            .is_err());
        assert_eq!(
            heap.events.last(),
            Some(&Event::Allocate(
                TrafficBackingKind::RecordPointers,
                count.wrapping_mul(4)
            ))
        );
        assert_eq!(
            world.traffic_generator(actor).unwrap().count_bits(),
            Some(count)
        );
        assert!(world.traffic_generator(actor).unwrap().records().is_some());
        world
            .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
            .unwrap();
        assert_eq!(heap.resources.live_allocations(), 0);
    }
    let (mut world, actor, mut heap) = fixture();
    let bytes = payload(1, 1);
    world
        .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
        .unwrap();
    world.traffic_mut(actor).unwrap().count = TrafficWord::known(2);
    heap.events.clear();
    let bytes = payload(0, 0);
    assert!(world
        .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
        .is_err());
    assert!(heap.events.is_empty()); // reverse first slot is already outside backing
    assert_eq!(
        world.traffic_generator(actor).unwrap().total_bits(),
        Some(0)
    );
    assert_eq!(
        world.traffic_generator(actor).unwrap().scalar_bits()[0].bits(),
        Some(0x3f800000)
    );
    world
        .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
        .unwrap();
    assert_eq!(heap.resources.live_allocations(), 0);
}
#[test]
fn actual_heap_rejects_foreign_kind_owner_domain_and_stale_generation() {
    let (mut world, actor, mut heap) = fixture();
    let other_lifetime = crate::script_lookup::ActorLifetime::for_existing(actor.identity());
    let other = other_lifetime.reference();
    let id = heap
        .resources
        .allocate(actor, TrafficBackingKind::Record, 17)
        .unwrap();
    assert!(TrafficHeap::default()
        .free(actor, TrafficBackingKind::Record, Some(id))
        .is_err());
    assert!(heap
        .resources
        .free(other, TrafficBackingKind::Record, Some(id))
        .is_err());
    assert!(heap
        .resources
        .free(actor, TrafficBackingKind::Weights, Some(id))
        .is_err());
    assert_eq!(heap.resources.live_allocations(), 1);
    heap.resources
        .free(actor, TrafficBackingKind::Record, Some(id))
        .unwrap();
    assert!(heap
        .resources
        .free(actor, TrafficBackingKind::Record, Some(id))
        .is_err());
    finish(&mut world, actor, &mut heap);
    assert!(world
        .load_traffic_generator_data(
            actor,
            &mut TrafficChunkReader::new(&9u32.to_le_bytes()),
            &mut heap
        )
        .is_err());
}
#[test]
fn inherited_destructor_unwind_retains_count_and_static_prefix_without_derived_free_replay() {
    let (mut world, actor, mut heap) = fixture();
    let bytes = payload(1, 1);
    world
        .load_traffic_generator_data(actor, &mut TrafficChunkReader::new(&bytes), &mut heap)
        .unwrap();
    let frame = crate::frame_lookup::FrameLifetime::for_existing(88);
    world.state_mut_inner(actor).unwrap().frame = Some(frame.reference());
    world.state_mut_inner(actor).unwrap().release_frame = 1;
    let mut frames = Heap {
        fail_release_frame: true,
        ..Heap::default()
    };
    assert!(
        catch_unwind(AssertUnwindSafe(|| world.destroy_traffic_generator(
            actor,
            &mut frames,
            &mut heap
        )))
        .is_err()
    );
    assert_eq!(world.phase(actor), Ok(ActorPhase::DestructionInterrupted));
    assert_eq!(world.actor_count, 0);
    assert_eq!(world.source(ActorSource::Static), &[Some(actor)]);
    assert_eq!(heap.resources.live_allocations(), 0);
    let reached = heap.events.len();
    assert!(world
        .destroy_traffic_generator(actor, &mut Heap::default(), &mut heap)
        .is_err());
    assert!(world
        .discard_traffic_generator_host(actor, &mut Heap::default(), &mut heap)
        .is_err());
    assert_eq!(heap.events.len(), reached);
    assert_eq!(
        world.inspect_traffic_generator(actor).unwrap().count_bits(),
        Some(1)
    );
}

#[test]
fn false_base_init_and_null_derived_factory_do_not_publish_a_successful_actor() {
    let (mut world, actor, mut heap) = fixture();
    let other_lifetime = crate::script_lookup::ActorLifetime::for_existing(123);
    heap.frame_data = Some(FrameActorData::Present(Some(other_lifetime.reference())));
    let frame = crate::frame_lookup::FrameLifetime::for_existing(99);
    assert_eq!(
        world.init(actor, Some(frame.reference()), &mut heap),
        Ok(false)
    );
    assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
    assert_eq!(world.state(actor).unwrap().frame, None);
    assert_eq!(world.attachment_counter, 0);
    assert_eq!(
        world.release_traffic_generator(actor, &mut Heap::default(), &mut heap),
        Ok(0)
    );
    assert_eq!(world.actor_count, 0);
    assert!(world.source(ActorSource::Static).is_empty());
    heap.null_actor = true;
    assert_eq!(world.create_actor(18, &mut heap), Ok(None));
    assert_eq!(world.actor_count, 0);
    assert_eq!(heap.resources.live_allocations(), 0);
}

#[test]
fn chunk_remaining_length_uses_pc32_wrapped_subtraction_and_signed_guard() {
    let bytes = 1u32.to_le_bytes();
    for end in [0x80000000usize, 0xffffffff] {
        let mut reader = TrafficChunkReader::with_declared_end(&bytes, end);
        let mut destination = [None; 4];
        assert_eq!(reader.read_bounded(&mut destination), Ok(false));
        assert_eq!(reader.read_null_bounded(4), Ok(false));
        assert_eq!(reader.position(), 0);
        assert_eq!(destination, [None; 4]);
    }
    let mut reader = TrafficChunkReader::with_declared_end(&bytes, 0x7fffffff);
    let mut destination = [None; 4];
    assert_eq!(reader.read_bounded(&mut destination), Ok(true));
    assert_eq!(destination, bytes.map(Some));
    assert_eq!(reader.position(), 4);
    // High-bit EBX is a signed negative native request; this safe adapter does
    // not reinterpret it as a normal unsigned bound skip or huge host slice.
    assert!(reader.read_null_bounded(0x80000000).is_err());
    assert_eq!(reader.position(), 4);
    reader.set_readable(false);
    assert_eq!(reader.read_null_bounded(0x80000000), Ok(false));
    // Same 32-bit arithmetic when both absolute file positions have a high bit.
    // Public readers start at0; this explicit binary-boundary fixture supplies
    // a later Tell without allocating a multi-gigabyte physical input.
    let mut reader = TrafficChunkReader::with_declared_end(&[], 0x80000020);
    reader.set_position_for_test(0x80000010);
    assert_eq!(reader.read_bounded(&mut [None; 17]), Ok(false));
    assert!(reader.read_null_bounded(4).is_err());
}
