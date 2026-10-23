//! Original C_Vehicle destructor at PC RVA0xc1ec0. Explicit destruction is
//! separate from a failed Init return and from dropping a Rust projection.
use super::component_tail::Projection;
use super::*;
use crate::vehicle_bodycol_records::destroy_bodycol_temporaries;

pub trait VehicleDestructorBackend: VehicleInitBackend {
    /// Original frame LinkTo(0,0), virtual+0x2c.
    fn detach_vehicle_frame(&mut self, frame: u32) -> Result<(), Self::Error>;
    /// Original virtual slot0 Release, for a model/frame/resource identity.
    fn release_vehicle_object(&mut self, object: u32) -> Result<(), Self::Error>;
}
impl<B: VehicleDestructorBackend> VehicleDestructorBackend for Projection<'_, B> {
    fn detach_vehicle_frame(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.detach_vehicle_frame(frame)
    }
    fn release_vehicle_object(&mut self, object: u32) -> Result<(), Self::Error> {
        self.backend.release_vehicle_object(object)
    }
}
fn dispose_vector<B: VehicleInitBackend>(b: &mut B, header: u32) -> Result<(), B::Error> {
    let allocation = b.read_word(header.wrapping_add(4))?;
    b.free(allocation)?;
    for offset in [4, 8, 12] {
        b.write_word(header.wrapping_add(offset), 0)?;
    }
    Ok(())
}
fn erase_records<B: VehicleInitBackend>(
    b: &mut B,
    header: u32,
    mut first: u32,
    mut last: u32,
    stride: u32,
) -> Result<(), B::Error> {
    let end = b.read_word(header.wrapping_add(8))?;
    while last != end {
        for offset in (0..stride).step_by(4) {
            let value = b.read_word(last.wrapping_add(offset))?;
            b.write_word(first.wrapping_add(offset), value)?;
        }
        last = last.wrapping_add(stride);
        first = first.wrapping_add(stride);
    }
    b.write_word(header.wrapping_add(8), first)
}
fn record_count<B: VehicleInitBackend>(
    b: &mut B,
    header: u32,
    stride: u32,
) -> Result<u32, B::Error> {
    let begin = b.read_word(header.wrapping_add(4))?;
    if begin == 0 {
        return Ok(0);
    }
    let bytes = b.read_word(header.wrapping_add(8))?.wrapping_sub(begin) as i32;
    Ok(if stride == 32 {
        (bytes >> 5) as u32
    } else {
        (bytes / stride as i32) as u32
    })
}
fn field<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    header: u32,
    offset: u32,
    member: u32,
) -> Result<u32, B::Error> {
    let begin = b.read_word(vehicle.wrapping_add(header).wrapping_add(4))?;
    b.read_word(begin.wrapping_add(offset).wrapping_add(member))
}
/// Runs the complete explicit destructor against addressable storage. No
/// scalar-deleting wrapper/free of `vehicle` itself is performed by this entry.
/// Every external failure keeps completed releases, frees and header stores.
/// The caller must invoke this only once for an initialized constructor state.
pub fn vehicle_destruct<B: VehicleDestructorBackend>(
    b: &mut B,
    vehicle: u32,
) -> Result<(), B::Error> {
    b.write_word(vehicle, 0x624df0)?;
    let name = b.read_word(vehicle.wrapping_add(0x4bc))?;
    if name != 0 {
        b.free(name)?;
    }
    let mut node = b.read_word(vehicle.wrapping_add(0xc34))?;
    while node != 0 {
        let next = b.read_word(node.wrapping_add(0x1d0))?;
        b.write_word(vehicle.wrapping_add(0xc34), next)?;
        let name = b.read_word(node)?;
        if name != 0 {
            b.free(name)?;
        }
        dispose_vector(b, node.wrapping_add(0x128))?;
        b.free(node)?;
        node = b.read_word(vehicle.wrapping_add(0xc34))?;
    }
    let order = b.read_word(vehicle.wrapping_add(0xc38))?;
    if order != 0 {
        b.free(order)?;
    }
    let mut count = record_count(b, vehicle.wrapping_add(0xfc), 32)?;
    let mut offset = count.wrapping_mul(32);
    while count != 0 {
        offset = offset.wrapping_sub(32);
        let begin = b.read_word(vehicle.wrapping_add(0x100))?;
        if b.read_word(begin.wrapping_add(offset).wrapping_add(8))? != 0
            && b.read_word(begin.wrapping_add(offset).wrapping_add(0x14))? & 0x80000000 == 0
        {
            let frame = b.read_word(begin.wrapping_add(offset).wrapping_add(8))?;
            b.detach_vehicle_frame(frame)?;
            let frame = field(b, vehicle, 0xfc, offset, 8)?;
            b.release_vehicle_object(frame)?;
            let frame = field(b, vehicle, 0xfc, offset, 12)?;
            if frame != 0 {
                b.detach_vehicle_frame(frame)?;
                let frame = field(b, vehicle, 0xfc, offset, 12)?;
                b.release_vehicle_object(frame)?;
            }
        }
        count = count.wrapping_sub(1);
    }
    let end = b.read_word(vehicle.wrapping_add(0x104))?;
    let begin = b.read_word(vehicle.wrapping_add(0x100))?;
    erase_records(b, vehicle.wrapping_add(0xfc), begin, end, 32)?;
    let end = b.read_word(0x63d898)?;
    let begin = b.read_word(0x63d894)?;
    erase_records(b, 0x63d890, begin, end, 4)?;
    count = record_count(b, vehicle.wrapping_add(0x10c), 28)?;
    offset = count.wrapping_mul(28);
    while count != 0 {
        offset = offset.wrapping_sub(28);
        let frame = field(b, vehicle, 0x10c, offset, 4)?;
        b.detach_vehicle_frame(frame)?;
        for member in [4, 0x10, 0x14, 0xc] {
            let object = field(b, vehicle, 0x10c, offset, member)?;
            b.release_vehicle_object(object)?;
        }
        let frame = field(b, vehicle, 0x10c, offset, 8)?;
        if frame != 0 {
            b.detach_vehicle_frame(frame)?;
            let frame = field(b, vehicle, 0x10c, offset, 8)?;
            b.release_vehicle_object(frame)?;
        }
        count = count.wrapping_sub(1);
    }
    let _end = b.read_word(vehicle.wrapping_add(0x114))?;
    let begin = b.read_word(vehicle.wrapping_add(0x110))?;
    let _end = b.read_word(vehicle.wrapping_add(0x114))?;
    b.write_word(vehicle.wrapping_add(0x114), begin)?;
    for (header, members) in [(0x204, &[4u32, 8][..]), (0x214, &[8u32][..])] {
        count = record_count(b, vehicle.wrapping_add(header), 52)?;
        offset = count.wrapping_mul(52);
        while count != 0 {
            offset = offset.wrapping_sub(52);
            for member in members {
                let object = field(b, vehicle, header, offset, *member)?;
                if object != 0 {
                    b.release_vehicle_object(object)?;
                }
            }
            count = count.wrapping_sub(1);
        }
    }
    let model = b.read_word(vehicle.wrapping_add(0x34c))?;
    if model != 0 {
        let properties = b.read_word(model.wrapping_add(8))?;
        b.free(properties)?;
        b.write_word(model.wrapping_add(8), 0)?;
        let model = b.read_word(vehicle.wrapping_add(0x34c))?;
        b.release_vehicle_object(model)?;
    }
    for header in [0x1ff8, 0x1fe0, 0x1fd0, 0x1fc0, 0x1fb0, 0xcc0, 0xcb0, 0xca0] {
        dispose_vector(b, vehicle.wrapping_add(header))?;
    }
    let end = b.read_word(vehicle.wrapping_add(0x4b4))?;
    let begin = b.read_word(vehicle.wrapping_add(0x4b0))?;
    destroy_bodycol_temporaries(b, begin, end)?;
    dispose_vector(b, vehicle.wrapping_add(0x4ac))?;
    dispose_vector(b, vehicle.wrapping_add(0x450))?;
    for header in [0x33c, 0x32c, 0x214, 0x204] {
        // Original empty element-destruction helper reads the native arguments.
        let _end = b.read_word(vehicle.wrapping_add(header + 8))?;
        let _begin = b.read_word(vehicle.wrapping_add(header + 4))?;
        dispose_vector(b, vehicle.wrapping_add(header))?;
    }
    for header in [0x10c, 0xfc, 0xe8, 0xd8] {
        dispose_vector(b, vehicle.wrapping_add(header))?;
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleDestructorUnavailable<E> {
    NoStorage,
    AlreadyStarted,
    Effect(E),
}
impl VehicleInitState {
    /// Explicitly destroys an attached projection, including rejected Init
    /// ownership. A partial host failure cannot be replayed over released state.
    pub fn destruct<B: VehicleDestructorBackend>(
        &mut self,
        b: &mut B,
    ) -> Result<(), VehicleDestructorUnavailable<B::Error>> {
        if self.destructor_started {
            return Err(VehicleDestructorUnavailable::AlreadyStarted);
        }
        self.destructor_started = true;
        self.continuation = None;
        self.component_tail = None;
        self.wheel_continuation = false;
        self.wheel_setup_ready = false;
        self.final_setup_ready = false;
        let base = self
            .tail_vehicle_address
            .ok_or(VehicleDestructorUnavailable::NoStorage)?;
        if self.tail_scratch.is_none() {
            return Err(VehicleDestructorUnavailable::NoStorage);
        }
        vehicle_destruct(&mut self.tail_projection(b, base), base)
            .map_err(VehicleDestructorUnavailable::Effect)
    }
}

#[cfg(test)]
mod tests {
    use super::super::wheels::tests::{Effect, Memory};
    use super::*;
    impl VehicleDestructorBackend for Memory {
        fn detach_vehicle_frame(&mut self, frame: u32) -> Result<(), ()> {
            self.effect(Effect::Detach(frame))
        }
        fn release_vehicle_object(&mut self, object: u32) -> Result<(), ()> {
            self.effect(Effect::Release(object))
        }
    }
    fn fixture() -> Memory {
        let mut b = Memory::default();
        b.put(0x10000, &[0; 0x2400]);
        b.word(0x63d894, 0);
        b.word(0x63d898, 0);
        b.word(0x104bc, 0x3000);
        b.word(0x10c34, 0x4000);
        b.word(0x10c38, 0x6000);
        b.word(0x4000, 0x5000);
        b.word(0x41d0, 0x4200);
        b.word(0x412c, 0x7000);
        b.word(0x4200, 0);
        b.word(0x43d0, 0);
        b.word(0x432c, 0);
        b.word(0x1034c, 0x2000);
        b.word(0x2008, 0x8000);
        b
    }
    #[test]
    fn rejected_init_persistent_ownership_is_destroyed_explicitly_in_native_order() {
        let mut b = fixture();
        vehicle_destruct(&mut b, 0x10000).unwrap();
        let effects = b
            .effects
            .iter()
            .filter(|e| matches!(e, Effect::Free(_) | Effect::Release(_)))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            &effects[..9],
            &[
                Effect::Free(0x3000),
                Effect::Free(0x5000),
                Effect::Free(0x7000),
                Effect::Free(0x4000),
                Effect::Free(0),
                Effect::Free(0x4200),
                Effect::Free(0x6000),
                Effect::Free(0x8000),
                Effect::Release(0x2000)
            ]
        );
        assert_eq!(b.get(0x10c34), 0);
        assert_eq!(b.get(0x412c), 0);
        assert_eq!(b.get(0x2008), 0);
        assert_eq!(b.get(0x104bc), 0x3000);
        assert_eq!(b.get(0x10c38), 0x6000);
        assert_eq!(b.get(0x1034c), 0x2000);
        assert_eq!(b.get(0x10000), 0x624df0);
    }
    #[test]
    fn destructor_unavailability_keeps_every_observed_effect_and_store_prefix() {
        let initial = fixture();
        let mut baseline = initial.clone();
        vehicle_destruct(&mut baseline, 0x10000).unwrap();
        for fail in 0..baseline.effects.len() {
            let mut b = initial.clone();
            b.fail = Some(fail);
            assert_eq!(vehicle_destruct(&mut b, 0x10000), Err(()));
            assert_eq!(b.effects, baseline.effects[..fail]);
            let mut expected = initial.clone();
            for event in &b.effects {
                if let Effect::Write(address, bytes) = event {
                    expected.put(*address, bytes);
                }
            }
            assert_eq!(b.bytes, expected.bytes);
        }
    }
    #[test]
    fn release_loops_walk_reverse_and_reload_container_after_virtual_calls() {
        let mut b = fixture();
        b.word(0x10100, 0x9000);
        b.word(0x10104, 0x9040);
        for (record, frame, flags) in [(0x9000, 0xa000, 0x80000000), (0x9020, 0xa100, 0)] {
            b.word(record + 8, frame);
            b.word(record + 12, frame + 0x100);
            b.word(record + 0x14, flags);
        }
        vehicle_destruct(&mut b, 0x10000).unwrap();
        let effects = b
            .effects
            .iter()
            .filter(|e| matches!(e, Effect::Detach(_) | Effect::Release(_)))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            effects,
            [
                Effect::Detach(0xa100),
                Effect::Release(0xa100),
                Effect::Detach(0xa200),
                Effect::Release(0xa200),
                Effect::Release(0x2000)
            ]
        );
    }
}
