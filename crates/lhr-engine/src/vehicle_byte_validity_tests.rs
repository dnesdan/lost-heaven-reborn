//! Synthetic ownership/validity regressions over actual compatibility storage.
use super::*;
use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};

#[test]
fn owned_unknown_never_reads_external_alias_and_known_write_needs_no_old_value() {
    let mut vehicle = [10, 20, 30, 40, 50, 60, 70, 80];
    let mut known = [true, false, true, true, true, true, true, true];
    let mut scratch = VehicleComponentTailScratch {
        address: 0x2000,
        bytes: [0; VEHICLE_INIT_STACK_BYTES],
    };
    let mut memory = VehicleComponentTailMemory {
        car: None,
        vehicle_address: 0x1000,
        vehicle: &mut vehicle,
        vehicle_known: Some(&mut known),
        scratch: &mut scratch,
    };
    let mut host = VehicleFrameMemory::default();
    host.map_bytes(0x1000, &[0x99; 8]).unwrap(); // An external alias must not rescue unknown owned bytes.
    assert_eq!(
        memory.read_word_with_backend(&mut host, 0x1000),
        Err(VehicleMemoryError::Uninitialized(0x1001))
    );
    assert_eq!(
        memory.reborrow().read_byte_with_backend(&mut host, 0x1001),
        Err(VehicleMemoryError::Uninitialized(0x1001))
    );
    {
        let mut projection = component_tail::Projection {
            backend: &mut host,
            memory: memory.reborrow(),
        };
        assert_eq!(
            projection.read_byte(0x1001),
            Err(VehicleMemoryError::Uninitialized(0x1001))
        );
        assert_eq!(
            projection.read_word(0x1000),
            Err(VehicleMemoryError::Uninitialized(0x1001))
        );
    }
    memory
        .write_word_with_backend(&mut host, 0x1000, 0x12345678)
        .unwrap();
    assert_eq!(
        memory.read_word_with_backend(&mut host, 0x1000),
        Ok(0x12345678)
    );
    assert_eq!(host.read_word(0x1000), Ok(0x99999999));
    assert!(known[..4].iter().all(|known| *known));
}

#[test]
fn actual_vehicle_owner_diagnostics_and_reborrows_preserve_unknown_record_bits() {
    let car = 0x10000;
    let mut state = VehicleInitState::from_car_bytes(
        car,
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
    .unwrap();
    let mut scratch = VehicleComponentTailScratch {
        address: 0x900000,
        bytes: [0; VEHICLE_INIT_STACK_BYTES],
    };
    let address = car + 0x70 + 0xe6c + 0x4fc;
    let mut host = VehicleFrameMemory::default();
    {
        let mut memory = state.car_live_memory(&mut scratch).unwrap();
        memory
            .write_opaque_word(
                address,
                [(0x11, true), (0x22, true), (0xa5, false), (0xd7, false)],
            )
            .unwrap();
        assert_eq!(
            memory.reborrow().read_word_with_backend(&mut host, address),
            Err(VehicleMemoryError::Uninitialized(address + 2))
        );
        assert_eq!(
            memory.read_byte_with_backend(&mut host, address + 1),
            Ok(0x22)
        );
    }
    assert!(state.known_bytes().is_none());
    assert!(state.car_bytes().is_none());
    assert_eq!(
        state.known_range(0xe6c + 0x4fc, 2),
        Some([0x11, 0x22].as_slice())
    );
    assert!(state.known_range(0xe6c + 0x4fc, 4).is_none());
    state
        .car_live_memory(&mut scratch)
        .unwrap()
        .write_word_with_backend(&mut host, address, 0x44332211)
        .unwrap();
    assert!(state.known_bytes().is_some());
    assert_eq!(
        state.known_range(0xe6c + 0x4fc, 4),
        Some([0x11, 0x22, 0x33, 0x44].as_slice())
    );
}
