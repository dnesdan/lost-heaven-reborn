//! C_Debris::GenerateDebris (PC0x565060; PS2GenerateDebris__8C_Debris
//! correspondence3bc110). The zero-count profile returns void before any
//! manager, point, parameter, random, factory or floating-point operation.
//! Live generation remains an explicit dependency, rather than fake debris.
use crate::vehicle_init::VehicleInitBackend;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DebrisPoint {
    Address(u32),
    Value([f32; 3]),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebrisGeneration {
    pub point: DebrisPoint,
    pub count: i32,
    /// Original three scalar arguments, in source order.
    pub scalars: [f32; 3],
    pub velocity: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DebrisUnavailable<E> {
    Effect(E),
    Generation { manager: u32, count: i32 },
}
/// The original56507c TEST checks exactly zero. A negative count is not a
/// zero-count profile. The return is unit: this source caller observes no bool.
pub fn generate_debris<B: VehicleInitBackend + ?Sized>(
    _backend: &mut B,
    manager: u32,
    call: DebrisGeneration,
) -> Result<(), DebrisUnavailable<B::Error>> {
    if call.count == 0 {
        return Ok(());
    }
    Err(DebrisUnavailable::Generation {
        manager,
        count: call.count,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{native_precision::Pc24PrecisionState, vehicle_frame_backend::VehicleFrameMemory};
    fn call(count: i32) -> DebrisGeneration {
        DebrisGeneration {
            point: DebrisPoint::Address(0),
            count,
            scalars: [f32::NAN, f32::INFINITY, f32::NEG_INFINITY],
            velocity: Some(1),
        }
    }
    #[test]
    fn zero_count_preserves_precision_and_reads_neither_owner_nor_unmapped_operands() {
        for incoming in [false, true] {
            let mut memory = VehicleFrameMemory::default();
            let mut precision = Pc24PrecisionState::new(incoming);
            assert_eq!(
                precision.run(|| generate_debris(&mut memory, 0, call(0))),
                Ok(())
            );
            assert_eq!(precision.precision_raised(), incoming);
            assert_eq!(memory.allocations().count(), 0);
        }
    }
    #[test]
    fn nonzero_and_negative_counts_remain_explicit_generation_dependencies() {
        let mut memory = VehicleFrameMemory::default();
        for count in [-1, 1] {
            assert_eq!(
                generate_debris(&mut memory, 0x1234, call(count)),
                Err(DebrisUnavailable::Generation {
                    manager: 0x1234,
                    count
                })
            );
            assert_eq!(memory.allocations().count(), 0);
        }
    }
}
