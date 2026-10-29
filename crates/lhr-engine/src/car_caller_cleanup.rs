//! Explicit original C_car destruction and its base-actor collection removal.
//! Init's false return never invokes this entry. Dropping state releases none.
use super::*;

/// Original 0x5fa720: erase only the first matching pair from the Game list.
fn remove_car_pair<B: VehicleInitBackend>(b: &mut B, car: u32) -> Result<(), B::Error> {
    let mut current = b.read_word(0x6bf980 + 0x449c)?;
    let end = b.read_word(0x6bf980 + 0x44a0)?;
    while current < end {
        let key = b.read_word(current)?;
        current = current.wrapping_add(8);
        if key == car {
            let end = b.read_word(0x6bf980 + 0x44a0)?;
            let mut destination = current.wrapping_sub(8);
            while current != end {
                let first = b.read_word(current)?;
                current = current.wrapping_add(8);
                b.write_word(destination, first)?;
                let second = b.read_word(current.wrapping_sub(4))?;
                b.write_word(destination.wrapping_add(4), second)?;
                destination = destination.wrapping_add(8);
            }
            let end = b.read_word(0x6bf980 + 0x44a0)?;
            b.write_word(0x6bf980 + 0x44a0, end.wrapping_sub(8))?;
            return Ok(());
        }
    }
    Ok(())
}
fn erase_word<B: VehicleInitBackend>(
    b: &mut B,
    end_field: u32,
    mut cursor: u32,
) -> Result<(), B::Error> {
    let end = b.read_word(end_field)?;
    let mut next = cursor.wrapping_add(4);
    while next != end {
        let value = b.read_word(next)?;
        b.write_word(cursor, value)?;
        next = next.wrapping_add(4);
        cursor = cursor.wrapping_add(4);
    }
    let end = b.read_word(end_field)?;
    b.write_word(end_field, end.wrapping_sub(4))
}
/// Original 0x59d1f0 with the destructor's cleanup argument zero. The
/// matching cache record loses its actor pointer before ordered raw erasure.
fn remove_actor_cache_record<B: VehicleInitBackend>(b: &mut B, actor: u32) -> Result<(), B::Error> {
    let initial_begin = b.read_word(0x647998)?;
    let mut index = 0u32;
    let mut offset = 0u32;
    loop {
        if initial_begin == 0 {
            break;
        }
        let end = b.read_word(0x64799c)?;
        let count = ((end.wrapping_sub(initial_begin) as i32) / 0x58) as u32;
        if index >= count {
            break;
        }
        let begin = b.read_word(0x647998)?;
        if b.read_word(begin.wrapping_add(offset).wrapping_add(0x50))? == actor {
            let begin = b.read_word(0x647998)?;
            b.write_word(
                begin
                    .wrapping_add(index.wrapping_mul(0x58))
                    .wrapping_add(0x50),
                0,
            )?;
            let begin = b.read_word(0x647998)?;
            let end = b.read_word(0x64799c)?;
            let mut destination = begin.wrapping_add(index.wrapping_mul(0x58));
            let mut source = destination.wrapping_add(0x58);
            while source != end {
                for offset in (0..0x58).step_by(4) {
                    let value = b.read_word(source.wrapping_add(offset))?;
                    b.write_word(destination.wrapping_add(offset), value)?;
                }
                source = source.wrapping_add(0x58);
                destination = destination.wrapping_add(0x58);
            }
            let end = b.read_word(0x64799c)?;
            b.write_word(0x64799c, end.wrapping_sub(0x58))?;
            break;
        }
        index = index.wrapping_add(1);
        offset = offset.wrapping_add(0x58);
    }
    Ok(())
}
/// Original 0x599010's two actor-pointer searches with cleanup flag zero.
fn remove_actor_from_manager<B: VehicleInitBackend>(b: &mut B, actor: u32) -> Result<(), B::Error> {
    remove_actor_cache_record(b, actor)?;
    for (begin_field, end_field, clear_flag) in
        [(0x647908, 0x64790c, true), (0x647918, 0x64791c, false)]
    {
        let initial_begin = b.read_word(begin_field)?;
        let mut index = 0u32;
        loop {
            if initial_begin == 0 {
                break;
            }
            let end = b.read_word(end_field)?;
            let count = ((end.wrapping_sub(initial_begin) as i32) >> 2) as u32;
            if index >= count {
                break;
            }
            let begin = b.read_word(begin_field)?;
            if b.read_word(begin.wrapping_add(index * 4))? == actor {
                if clear_flag {
                    let flags = b.read_word(actor.wrapping_add(4))?;
                    b.write_word(actor.wrapping_add(4), flags & !0x10)?;
                }
                let begin = b.read_word(begin_field)?;
                erase_word(b, end_field, begin.wrapping_add(index * 4))?;
                return Ok(());
            }
            index = index.wrapping_add(1);
        }
    }
    Ok(())
}
/// Original 0x407f50: erase the last match by replacing it with the last entry.
fn remove_actor_from_static<B: VehicleInitBackend>(b: &mut B, actor: u32) -> Result<(), B::Error> {
    let begin = b.read_word(0x63d2bc)?;
    if begin == 0 {
        return Ok(());
    }
    let end = b.read_word(0x63d2c0)?;
    let mut count = ((end.wrapping_sub(begin) as i32) >> 2) as u32;
    while count != 0 {
        let value = b.read_word(begin.wrapping_add(count * 4).wrapping_sub(4))?;
        count = count.wrapping_sub(1);
        if value == actor {
            let last = b.read_word(end.wrapping_sub(4))?;
            b.write_word(begin.wrapping_add(count * 4), last)?;
            // Native computes erase(end-1,end), whose suffix is empty.
            let _end = b.read_word(0x63d2c0)?;
            let end = b.read_word(0x63d2c0)?;
            b.write_word(0x63d2c0, end.wrapping_sub(4))?;
            break;
        }
    }
    Ok(())
}
pub(crate) fn actor_destruct<B: VehicleDestructorBackend>(
    b: &mut B,
    actor: u32,
) -> Result<(), B::Error> {
    b.write_word(actor, 0x623248)?;
    let count = b.read_word(0x63d2c8)?;
    b.write_word(0x63d2c8, count.wrapping_sub(1))?;
    remove_actor_from_manager(b, actor)?;
    if b.read_byte(actor.wrapping_add(0x6c))? != 0 {
        let model = b.read_word(actor.wrapping_add(0x68))?;
        if model != 0 {
            b.release_vehicle_object(model)?;
        }
    }
    remove_actor_from_static(b, actor)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarDestructorUnavailable<E> {
    NoStorage,
    AlreadyStarted,
    Effect(E),
}
impl VehicleInitState {
    /// Original non-deleting destructor at 0x41bcb0, over the same canonical
    /// car bytes. An interrupted release/free sequence cannot be replayed.
    pub fn destruct_car<B: VehicleDestructorBackend>(
        &mut self,
        b: &mut B,
    ) -> Result<(), CarDestructorUnavailable<B::Error>> {
        self.destruct_car_parts(b, true)
    }
    /// Derived cleanup only; its caller owns the real typed base actor and
    /// invokes that destructor after this exact ordered prefix succeeds.
    pub(crate) fn destruct_car_derived<B: VehicleDestructorBackend>(
        &mut self,
        b: &mut B,
    ) -> Result<(), CarDestructorUnavailable<B::Error>> {
        self.destruct_car_parts(b, false)
    }
    fn destruct_car_parts<B: VehicleDestructorBackend>(
        &mut self,
        b: &mut B,
        owns_base: bool,
    ) -> Result<(), CarDestructorUnavailable<B::Error>> {
        if self.destructor_started {
            return Err(CarDestructorUnavailable::AlreadyStarted);
        }
        self.destructor_started = true;
        self.continuation = None;
        self.component_tail = None;
        self.wheel_continuation = false;
        self.wheel_setup_ready = false;
        self.final_setup_ready = false;
        let owned = self
            .car
            .as_mut()
            .ok_or(CarDestructorUnavailable::NoStorage)?;
        owned.post_vehicle_ready = false;
        let car = owned.address;
        if self.tail_scratch.is_none() {
            return Err(CarDestructorUnavailable::NoStorage);
        }
        let mut projection = self.tail_projection(b, car + 0x70);
        let result = (|| {
            if owns_base {
                projection.write_word(car, 0x623618)?;
            }
            projection.write_word(car + 0x70, 0x6236b0)?;
            remove_car_pair(&mut projection, car)?;
            for offset in [0x21f8, 0x21cc, 0x21b4] {
                let allocation = projection.read_word(car + offset)?;
                projection.free(allocation)?;
                for member in [0, 4, 8] {
                    projection.write_word(car + offset + member, 0)?;
                }
            }
            vehicle_destruct(&mut projection, car + 0x70)?;
            if owns_base {
                actor_destruct(&mut projection, car)
            } else {
                Ok(())
            }
        })();
        result.map_err(CarDestructorUnavailable::Effect)
    }
}

#[cfg(test)]
mod tests {
    use super::super::wheels::tests::{Effect, Memory};
    use super::*;
    fn fixture() -> (VehicleInitState, Memory) {
        let mut bytes = vec![0; 0x2370];
        bytes[4..8].copy_from_slice(&0x10u32.to_le_bytes());
        for (offset, value) in [(0x21f8, 0x9000u32), (0x21cc, 0x9100), (0x21b4, 0x9200)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&(value + 8).to_le_bytes());
            bytes[offset + 8..offset + 12].copy_from_slice(&(value + 16).to_le_bytes());
        }
        let mut state = VehicleInitState::from_car_bytes(
            0x10000,
            bytes,
            CarInitScratch {
                address: 0x80000,
                bytes: [0xa5; CAR_INIT_STACK_BYTES],
            },
            CarInitRegisters {
                ebx: 1,
                ebp: 2,
                esi: 3,
                edi: 4,
            },
        )
        .unwrap();
        state.tail_vehicle_address = Some(0x10070);
        state.tail_scratch = Some(VehicleComponentTailScratch {
            address: 0x7fe30,
            bytes: [0x7d; VEHICLE_INIT_STACK_BYTES],
        });
        let mut b = Memory::default();
        b.word(0x63d894, 0);
        b.word(0x63d898, 0);
        b.word(0x63d2c8, 1);
        b.word(0x6c3e1c, 0x20000);
        b.word(0x6c3e20, 0x20018);
        for (i, value) in [0x10000, 10, 0x5555, 20, 0x10000, 30]
            .into_iter()
            .enumerate()
        {
            b.word(0x20000 + i as u32 * 4, value);
        }
        b.word(0x647998, 0x21000);
        b.word(0x64799c, 0x210b0);
        b.put(0x21000, &[0x11; 0xb0]);
        b.word(0x21050, 0x10000);
        b.word(0x210a8, 0x5555);
        b.word(0x647908, 0x22000);
        b.word(0x64790c, 0x2200c);
        for (i, value) in [0x5555, 0x10000, 0x6666].into_iter().enumerate() {
            b.word(0x22000 + i as u32 * 4, value);
        }
        b.word(0x647918, 0);
        b.word(0x64791c, 0);
        b.word(0x63d2bc, 0x23000);
        b.word(0x63d2c0, 0x23010);
        for (i, value) in [0x10000, 0x8888, 0x10000, 0x9999].into_iter().enumerate() {
            b.word(0x23000 + i as u32 * 4, value);
        }
        (state, b)
    }
    fn field(state: &VehicleInitState, offset: usize) -> u32 {
        u32::from_le_bytes(
            state.car_bytes().unwrap()[offset..offset + 4]
                .try_into()
                .unwrap(),
        )
    }
    #[test]
    fn actual_car_cleanup_removes_each_original_membership_with_its_own_order() {
        let (mut state, mut b) = fixture();
        assert_eq!(state.destruct_car(&mut b), Ok(()));
        assert_eq!(b.get(0x6c3e20), 0x20010);
        assert_eq!(b.get(0x20000), 0x5555);
        assert_eq!(b.get(0x20008), 0x10000);
        assert_eq!(b.get(0x64799c), 0x21058);
        assert_eq!(b.get(0x21050), 0x5555);
        assert_eq!(b.get(0x64790c), 0x22008);
        assert_eq!(b.get(0x22004), 0x6666);
        assert_eq!(field(&state, 4) & 0x10, 0);
        assert_eq!(b.get(0x63d2c0), 0x2300c);
        assert_eq!(b.get(0x23000), 0x10000);
        assert_eq!(b.get(0x23008), 0x9999);
        assert_eq!(b.get(0x63d2c8), 0);
        assert_eq!(field(&state, 0), 0x623248);
        assert_eq!(field(&state, 0x70), 0x624df0);
        let frees: Vec<u32> = b
            .effects
            .iter()
            .filter_map(|e| {
                if let Effect::Free(p) = e {
                    Some(*p)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(&frees[..3], &[0x9000, 0x9100, 0x9200]);
        for offset in [
            0x21f8, 0x21fc, 0x2200, 0x21cc, 0x21d0, 0x21d4, 0x21b4, 0x21b8, 0x21bc,
        ] {
            assert_eq!(field(&state, offset), 0);
        }
        assert_eq!(
            state.destruct_car(&mut b),
            Err(CarDestructorUnavailable::AlreadyStarted)
        );
    }
    #[test]
    fn failed_car_free_keeps_completed_cleanup_and_cannot_be_replayed() {
        let (mut complete, mut baseline) = fixture();
        complete.destruct_car(&mut baseline).unwrap();
        let fail = baseline
            .effects
            .iter()
            .position(|effect| *effect == Effect::Free(0x9100))
            .unwrap();
        let (mut state, mut b) = fixture();
        b.fail = Some(fail);
        assert_eq!(
            state.destruct_car(&mut b),
            Err(CarDestructorUnavailable::Effect(()))
        );
        assert_eq!(field(&state, 0x21f8), 0);
        assert_eq!(field(&state, 0x21cc), 0x9100);
        assert_eq!(field(&state, 0x21b4), 0x9200);
        assert_eq!(b.get(0x63d2c8), 1);
        assert_eq!(b.effects, baseline.effects[..fail]);
        assert_eq!(
            state.destruct_car(&mut b),
            Err(CarDestructorUnavailable::AlreadyStarted)
        );
    }
}
