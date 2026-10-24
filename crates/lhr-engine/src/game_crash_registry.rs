//! Original C_game registries reached by CreateCrashModel. The same caller
//! projection owns the records and vector allocations; frame values are inert
//! identities, never host pointers.
use crate::vehicle_init::VehicleInitBackend;

const MAX_RECORDS: u32 = 1 << 20;
#[derive(Debug, PartialEq, Eq)]
pub enum CrashRegistryError<E> {
    Backend(E),
    StorageRange,
    AllocationFailed,
}
type Out<T, E> = Result<T, CrashRegistryError<E>>;
fn read<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(CrashRegistryError::Backend)
}
fn write<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    b.write_word(a, v).map_err(CrashRegistryError::Backend)
}
fn count<E>(begin: u32, end: u32, stride: u32) -> Out<u32, E> {
    end.checked_sub(begin)
        .filter(|n| n.is_multiple_of(stride) && n / stride <= MAX_RECORDS)
        .map(|n| n / stride)
        .ok_or(CrashRegistryError::StorageRange)
}

fn remove<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    frame: u32,
    stride: u32,
) -> Out<(), B::Error> {
    if frame == 0 {
        return Ok(());
    }
    let begin = read(b, header)?;
    if begin == 0 {
        return Ok(());
    }
    let end = read(b, header + 4)?;
    let len = count(begin, end, stride)?;
    for i in (0..len).rev() {
        if read(b, begin + i * stride)? != frame {
            continue;
        }
        // Both functions erase the last matching row by copying the last
        // complete record over it. No frame reference operation is reached.
        let last = read(b, header + 4)? - stride;
        let destination = read(b, header)? + i * stride;
        for offset in (0..stride).step_by(4) {
            let value = read(b, last + offset)?;
            write(b, destination + offset, value)?;
        }
        let end = read(b, header + 4)?;
        write(b, header + 4, end - stride)?;
        return Ok(());
    }
    Ok(())
}

/// C_game::DelSemaphore, PC 0x5b6d50: descending search, twenty-byte rows.
pub fn del_semaphore<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    game: u32,
    frame: u32,
) -> Out<(), B::Error> {
    remove(b, game + 0x1a8, frame, 20)
}

/// C_game::DelTelephones, PC 0x5b7440: descending search, twelve-byte rows.
pub fn del_telephones<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    game: u32,
    frame: u32,
) -> Out<(), B::Error> {
    remove(b, game + 0x1c8, frame, 12)
}

/// Pointer-vector append embedded in CreateCrashModel, PC 0x431857..0x4319ca.
/// The original vector does not AddRef its elements. Its capacity remains
/// owned by the C_game caller until that caller destroys the vector.
pub fn retain_hidden_model<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    game: u32,
    frame: u32,
) -> Out<(), B::Error> {
    let header = game + 0x10;
    let end = read(b, header + 8)?;
    let capacity = read(b, header + 12)?;
    let available = count(end, capacity, 4)?;
    if available != 0 {
        write(b, end, frame)?;
        let end = read(b, header + 8)?;
        return write(b, header + 8, end + 4);
    }
    let begin = read(b, header + 4)?;
    let len = if begin == 0 { 0 } else { count(begin, end, 4)? };
    let new_capacity = len
        .checked_add(len.max(1))
        .filter(|n| *n <= MAX_RECORDS)
        .ok_or(CrashRegistryError::StorageRange)?;
    let fresh = b
        .allocate(new_capacity * 4)
        .map_err(CrashRegistryError::Backend)?;
    if fresh == 0 {
        return Err(CrashRegistryError::AllocationFailed);
    }
    for i in 0..len {
        let value = read(b, begin + i * 4)?;
        write(b, fresh + i * 4, value)?;
    }
    write(b, fresh + len * 4, frame)?;
    // The original code reloads the old header after the allocator effects.
    let old = read(b, header + 4)?;
    b.free(old).map_err(CrashRegistryError::Backend)?;
    write(b, header + 12, fresh + new_capacity * 4)?;
    let old = read(b, header + 4)?;
    let len = if old == 0 {
        0
    } else {
        let end = read(b, header + 8)?;
        count(old, end, 4)?
    };
    write(b, header + 4, fresh)?;
    write(b, header + 8, fresh + len * 4 + 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;
    const GAME: u32 = 0x1000;
    fn setup() -> VehicleFrameMemory {
        let mut b = VehicleFrameMemory::default();
        b.map_bytes(GAME, &[0; 0x200]).unwrap();
        b
    }
    #[test]
    fn duplicate_semaphore_removal_uses_last_match_and_swaps_entire_row() {
        let mut b = setup();
        let rows = b.allocate(80).unwrap();
        write(&mut b, GAME + 0x1a8, rows).unwrap();
        write(&mut b, GAME + 0x1ac, rows + 80).unwrap();
        for (i, frame) in [7, 9, 7, 11].into_iter().enumerate() {
            for offset in (0..20).step_by(4) {
                write(&mut b, rows + i as u32 * 20 + offset, frame + offset).unwrap();
            }
        }
        del_semaphore(&mut b, GAME, 7).unwrap();
        assert_eq!(read(&mut b, GAME + 0x1ac).unwrap(), rows + 60);
        assert_eq!(read(&mut b, rows).unwrap(), 7);
        assert_eq!(read(&mut b, rows + 20).unwrap(), 9);
        for offset in (0..20).step_by(4) {
            assert_eq!(read(&mut b, rows + 40 + offset).unwrap(), 11 + offset);
        }
    }
    #[test]
    fn telephone_removal_preserves_nonmatching_rows_and_empty_guards() {
        let mut b = setup();
        del_telephones(&mut b, GAME, 1).unwrap();
        let rows = b.allocate(36).unwrap();
        write(&mut b, GAME + 0x1c8, rows).unwrap();
        write(&mut b, GAME + 0x1cc, rows + 36).unwrap();
        for i in 0..9 {
            write(&mut b, rows + i * 4, i + 1).unwrap();
        }
        del_telephones(&mut b, GAME, 0).unwrap();
        del_telephones(&mut b, GAME, 999).unwrap();
        assert_eq!(read(&mut b, GAME + 0x1cc).unwrap(), rows + 36);
        del_telephones(&mut b, GAME, 1).unwrap();
        assert_eq!(read(&mut b, GAME + 0x1cc).unwrap(), rows + 24);
        for i in 0..3 {
            assert_eq!(read(&mut b, rows + i * 4).unwrap(), i + 7);
            assert_eq!(read(&mut b, rows + 12 + i * 4).unwrap(), i + 4);
        }
    }
    #[test]
    fn hidden_world_vector_grows_and_releases_old_capacity_without_frame_refs() {
        let mut b = setup();
        for i in 1..=9 {
            retain_hidden_model(&mut b, GAME, i * 13).unwrap();
        }
        let begin = read(&mut b, GAME + 0x14).unwrap();
        assert_eq!(read(&mut b, GAME + 0x18).unwrap(), begin + 36);
        assert_eq!(read(&mut b, GAME + 0x1c).unwrap(), begin + 64);
        assert_eq!(b.allocations().count(), 1);
        for i in 0..9 {
            assert_eq!(read(&mut b, begin + i * 4).unwrap(), (i + 1) * 13);
        }
        b.free(begin).unwrap();
        assert_eq!(b.allocations().count(), 0);
    }
}
