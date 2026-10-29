//! Synthetic actual-arena ownership regressions for Vehicle Frame+8 handling.
use super::*;
use crate::actor_frames::{FrameUserData, ModelActorFrames};
use crate::actor_world::FrameActorData;

#[test]
fn retiring_shared_actor_record_keeps_alias_pointers_and_unrelated_record_alive() {
    let mut arena = ModelArena::default();
    let first = arena.create_model().unwrap().root();
    let alias = arena.create_model().unwrap().root();
    let unrelated = arena.create_model().unwrap().root();
    let shared = arena.create_actor_data_record(None).unwrap();
    let other = arena.create_actor_data_record(None).unwrap();
    for frame in [first, alias] {
        arena
            .set_user_data(frame, FrameUserData::ActorRecord(shared))
            .unwrap();
    }
    arena
        .set_user_data(unrelated, FrameUserData::ActorRecord(other))
        .unwrap();
    let mut memory = VehicleFrameMemory::default();
    let (first_address, alias_address, other_address, token, other_token);
    {
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        first_address = backend.frame_address(first).unwrap();
        alias_address = backend.frame_address(alias).unwrap();
        other_address = backend.frame_address(unrelated).unwrap();
        token = backend.read_word(first_address + 8).unwrap();
        assert_eq!(backend.read_word(alias_address + 8), Ok(token));
        other_token = backend.read_word(other_address + 8).unwrap();
        assert_ne!(token, other_token);
        assert!((0xff00_0000..=0xffff_fff0).contains(&token));
        // Compatibility allocation identity is not an invented actor-word map.
        assert!(backend.read_word(token).is_err());
        backend.free(token).unwrap();
        assert!(backend.read_word(first_address + 8).is_err());
        assert!(backend.read_word(alias_address + 8).is_err());
        assert_eq!(backend.read_word(other_address + 8), Ok(other_token));
    }
    assert_eq!(arena.live_actor_data_record_count(), 1);
    assert!(arena.actor_data_record(shared).is_err());
    assert_eq!(arena.actor_data_record(other), Ok(None));
    for frame in [first, alias] {
        assert_eq!(
            arena.user_data(frame),
            Ok(FrameUserData::ActorRecord(shared))
        );
        let reference = arena.lookup_reference(frame);
        assert!(ModelActorFrames::borrow(&mut arena, true)
            .actor_data(reference)
            .is_err());
    }
    // Native free and the following Frame+8 null store are separate effects.
    {
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        backend.write_word(first_address + 8, 0).unwrap();
        assert_eq!(backend.read_word(first_address + 8), Ok(0));
        assert!(backend.read_word(alias_address + 8).is_err());
        assert!(backend.free(token).is_err());
        assert_eq!(backend.read_word(other_address + 8), Ok(other_token));
    }
    assert_eq!(
        arena.user_data(alias),
        Ok(FrameUserData::ActorRecord(shared))
    );
    assert_eq!(arena.live_actor_data_record_count(), 1);
    assert_eq!(memory.allocations().count(), 0);
}

#[test]
fn clearing_one_pointer_does_not_free_a_record_still_referenced_by_an_alias() {
    let mut arena = ModelArena::default();
    let first = arena.create_model().unwrap().root();
    let second = arena.create_model().unwrap().root();
    let record = arena.create_actor_data_record(None).unwrap();
    for frame in [first, second] {
        arena
            .set_user_data(frame, FrameUserData::ActorRecord(record))
            .unwrap();
    }
    let mut memory = VehicleFrameMemory::default();
    let token;
    {
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let first_address = backend.frame_address(first).unwrap();
        let second_address = backend.frame_address(second).unwrap();
        token = backend.read_word(first_address + 8).unwrap();
        backend.write_word(first_address + 8, 0).unwrap();
        assert_eq!(backend.read_word(first_address + 8), Ok(0));
        assert_eq!(backend.read_word(second_address + 8), Ok(token));
    }
    assert_eq!(arena.live_actor_data_record_count(), 1);
    let reference = arena.lookup_reference(second);
    assert_eq!(
        ModelActorFrames::borrow(&mut arena, true).actor_data(reference),
        Ok(FrameActorData::Present(None))
    );
    {
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        backend.free(token).unwrap();
    }
    assert_eq!(arena.live_actor_data_record_count(), 0);
    assert_eq!(
        arena.user_data(second),
        Ok(FrameUserData::ActorRecord(record))
    );
}

#[test]
fn foreign_stale_and_double_free_tokens_do_not_retire_other_records_or_publish_pointers() {
    let mut first_arena = ModelArena::default();
    let first_frame = first_arena.create_model().unwrap().root();
    let first_record = first_arena.create_actor_data_record(None).unwrap();
    first_arena
        .set_user_data(first_frame, FrameUserData::ActorRecord(first_record))
        .unwrap();
    let mut first_memory = VehicleFrameMemory::default();
    let foreign_token = {
        let mut backend = VehicleFrameBackend::new(&mut first_arena, &mut first_memory);
        let address = backend.frame_address(first_frame).unwrap();
        backend.read_word(address + 8).unwrap()
    };
    let mut second_arena = ModelArena::default();
    let second_frame = second_arena.create_model().unwrap().root();
    let second_record = second_arena.create_actor_data_record(None).unwrap();
    let mut second_memory = VehicleFrameMemory::default();
    let token;
    {
        let mut backend = VehicleFrameBackend::new(&mut second_arena, &mut second_memory);
        let address = backend.frame_address(second_frame).unwrap();
        assert!(backend.free(foreign_token).is_err());
        assert!(backend.write_word(address + 8, foreign_token).is_err());
        assert_eq!(backend.read_word(address + 8), Ok(0));
        token = backend
            .arena()
            .actor_data_record_vehicle_token(second_record)
            .unwrap();
        assert_ne!(token, foreign_token);
        backend.write_word(address + 8, token).unwrap();
        backend.free(token).unwrap();
        assert!(backend.free(token).is_err());
        backend.write_word(address + 8, 0).unwrap();
        assert!(backend.write_word(address + 8, token).is_err());
        assert_eq!(backend.read_word(address + 8), Ok(0));
    }
    assert_eq!(second_arena.live_actor_data_record_count(), 0);
    assert_eq!(first_arena.live_actor_data_record_count(), 1);
    assert_eq!(first_arena.actor_data_record(first_record), Ok(None));
    // Arena-local record indices may repeat; process-wide compatibility tokens
    // may not. A retired token cannot name a later allocation in either arena.
    let later = second_arena.create_actor_data_record(None).unwrap();
    let later_token = second_arena.actor_data_record_vehicle_token(later).unwrap();
    assert_ne!(later_token, token);
    assert_ne!(later_token, foreign_token);
    assert_eq!(second_memory.allocations().count(), 0);
    assert_eq!(first_memory.allocations().count(), 0);
}

#[test]
fn projected_free_through_live_vehicle_reborrow_retires_record_before_separate_pointer_clear() {
    let mut arena = ModelArena::default();
    let frame = arena.create_model().unwrap().root();
    let alias = arena.create_model().unwrap().root();
    let record = arena.create_actor_data_record(None).unwrap();
    for key in [frame, alias] {
        arena
            .set_user_data(key, FrameUserData::ActorRecord(record))
            .unwrap();
    }
    // Explicit synthetic compatibility storage. This does not execute or claim
    // the original constructor; it supplies the actual projected-free API view.
    let mut vehicle = VehicleInitState::from_car_bytes(
        0x0050_0000,
        vec![0; 0x221c],
        CarInitScratch {
            address: 0x0070_0000,
            bytes: [0; CAR_INIT_STACK_BYTES],
        },
        CarInitRegisters {
            ebx: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
        },
    )
    .unwrap();
    let mut scratch = VehicleComponentTailScratch {
        address: 0x0071_0000,
        bytes: [0; VEHICLE_INIT_STACK_BYTES],
    };
    let mut heap = VehicleFrameMemory::default();
    {
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        let address = backend.frame_address(frame).unwrap();
        let alias_address = backend.frame_address(alias).unwrap();
        let token = backend.read_word(address + 8).unwrap();
        let mut memory = vehicle.car_live_memory(&mut scratch).unwrap();
        // This is the route used by the destructor's live Projection backend,
        // including a reborrow of the same authoritative caller/vehicle memory.
        backend
            .free_projected(&mut memory.reborrow(), token)
            .unwrap();
        assert_eq!(backend.arena().live_actor_data_record_count(), 0);
        assert_eq!(
            backend.arena().user_data(frame),
            Ok(FrameUserData::ActorRecord(record))
        );
        assert_eq!(
            backend.arena().user_data(alias),
            Ok(FrameUserData::ActorRecord(record))
        );
        assert!(memory
            .read_word_with_backend(&mut backend, address + 8)
            .is_err());
        assert!(memory
            .read_word_with_backend(&mut backend, alias_address + 8)
            .is_err());
        memory
            .write_word_with_backend(&mut backend, address + 8, 0)
            .unwrap();
        assert_eq!(
            memory.read_word_with_backend(&mut backend, address + 8),
            Ok(0)
        );
        assert!(backend
            .free_projected(&mut memory.reborrow(), token)
            .is_err());
        assert!(memory
            .read_word_with_backend(&mut backend, alias_address + 8)
            .is_err());
    }
    assert_eq!(arena.user_data(frame), Ok(FrameUserData::Null));
    assert_eq!(
        arena.user_data(alias),
        Ok(FrameUserData::ActorRecord(record))
    );
    assert_eq!(heap.allocations().count(), 0);
}
