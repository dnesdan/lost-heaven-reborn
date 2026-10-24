use super::*;
use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
impl CrashActorBackend for VehicleFrameMemory {
    fn crash_actor_effect(&mut self, effect: CrashActorEffect) -> Result<u32, Self::Error> {
        let entry = match effect {
            CrashActorEffect::Dependency { entry, .. } => entry,
            _ => 0,
        };
        Err(VehicleMemoryError::CallbackRequired {
            function: entry,
            context: 0,
        })
    }
}
#[test]
fn crash_construction_and_final_release_use_one_allocation_and_empty_the_real_registry() {
    let mut memory = VehicleFrameMemory::default();
    for (base, len) in [(0x63d2b8, 20), (0x647998, 12), (0x647908, 32)] {
        memory.map_bytes(base, &vec![0; len]).unwrap();
    }
    let actor = memory.allocate(0x3c0).unwrap();
    for i in 0..0x3c0 {
        memory.write_byte(actor + i, 0xa5).unwrap();
    }
    crash_actor_construct(&mut memory, actor, 0x7d).unwrap();
    assert_eq!(memory.read_word(actor).unwrap(), 0x623918);
    assert_eq!(memory.read_word(actor + 0x10).unwrap(), 0x1a);
    assert_eq!(memory.read_word(actor + 0x394).unwrap(), 10_f32.to_bits());
    assert_eq!(memory.read_byte(actor + 0x37c).unwrap(), 0x7d);
    assert_eq!(memory.read_byte(actor + 0x3be).unwrap(), 0xa5);
    let registry = memory.read_word(0x63d2bc).unwrap();
    assert_eq!(memory.read_word(registry).unwrap(), actor);
    assert_eq!(memory.read_word(0x63d2c0).unwrap(), registry + 4);
    assert_eq!(memory.read_word(0x63d2c8).unwrap(), 1);
    assert_eq!(memory.allocations().count(), 2);
    crash_actor_release(&mut memory, actor).unwrap();
    assert_eq!(memory.read_word(0x63d2c8).unwrap(), 0);
    assert_eq!(memory.read_word(0x63d2c0).unwrap(), registry);
    assert_eq!(memory.allocations().collect::<Vec<_>>(), [(registry, 4)]);
    memory.free(registry).unwrap();
    assert_eq!(memory.allocations().count(), 0);
}
