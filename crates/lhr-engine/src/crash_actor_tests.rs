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
fn crash_ai_flag_dispatch_reports_each_unowned_effect_entry_in_table_order() {
    // Live probe abort (C:\Temp\probe_uturn.log, tick 34500 at
    // [-1427.4,-219.7]): StaticResponse ran crash_actor_ai over a prop
    // actor and died on Dependency(5644656) = 0x562170. The ported
    // flag-mask dispatch resolves each unowned effect routine before any
    // effect call runs, so the sim can stage the intent and continue.
    // Priority follows table order: mask 8 beats mask 2.
    for (flags, entry) in [(0x3u32, 0x562170u32), (0xbu32, 0x5abe10u32)] {
        let mut memory = VehicleFrameMemory::default();
        let actor = memory.allocate(0x400).unwrap();
        let world = memory.allocate(0x200).unwrap();
        memory.write_word(actor + 0x38c, flags).unwrap();
        memory
            .write_word(actor + 0x390, (-1.0f32).to_bits())
            .unwrap();
        memory.write_word(actor + 0x68, world).unwrap();
        memory.write_byte(world + 0xac, 0x20).unwrap();
        assert_eq!(
            crash_actor_ai(&mut memory, actor, 16),
            Err(CrashActorError::Dependency(entry))
        );
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
