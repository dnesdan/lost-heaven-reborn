//! Caller-owned g_collision dynamic-cell lifecycle. Original PC entries:
//! DynOptimizeTick 0x5c3610, DynInsert 0x5c36c0, DynErase 0x5c38b0 and
//! DynUpdate 0x5c3ac0. Names are retained PS2 symbols cross-checked against the
//! PC bodies. All authoritative pointers, membership, pool and pending lists
//! live in the shared vehicle-memory backend; Rust stores no parallel lists.
use crate::{collision_swept, vehicle_init::VehicleInitBackend};

const MAX_WORDS: u32 = 1 << 20;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicCollisionGridSource {
    pub min_xz: [f32; 2],
    pub max_xz: [f32; 2],
    pub desired_cell_size: f32,
}
#[derive(Debug, PartialEq, Eq)]
pub enum DynamicCollisionError<E> {
    Backend(E),
    NumericDomain,
    InvalidGrid,
    StorageRange,
    AllocationFailed,
    StorageQuotaExceeded,
}
type Out<T, E> = Result<T, DynamicCollisionError<E>>;
fn read<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(DynamicCollisionError::Backend)
}
fn write<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    b.write_word(a, v).map_err(DynamicCollisionError::Backend)
}
fn float<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    Ok(f32::from_bits(read(b, a)?))
}
fn allocate<B: VehicleInitBackend + ?Sized>(b: &mut B, n: u32) -> Out<u32, B::Error> {
    if n > MAX_WORDS * 4 {
        return Err(DynamicCollisionError::StorageQuotaExceeded);
    }
    let a = b.allocate(n).map_err(DynamicCollisionError::Backend)?;
    if a == 0 {
        return Err(DynamicCollisionError::AllocationFailed);
    }
    Ok(a)
}
fn free<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<(), B::Error> {
    b.free(a).map_err(DynamicCollisionError::Backend)
}
fn arithmetic<T, E>(r: Result<T, collision_swept::SphereSweepError>) -> Out<T, E> {
    r.map_err(|_| DynamicCollisionError::NumericDomain)
}
fn count<E>(begin: u32, end: u32) -> Out<u32, E> {
    let n = end
        .checked_sub(begin)
        .filter(|n| n % 4 == 0)
        .ok_or(DynamicCollisionError::StorageRange)?
        / 4;
    if n > MAX_WORDS {
        return Err(DynamicCollisionError::StorageQuotaExceeded);
    }
    Ok(n)
}
fn vector<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    h: u32,
) -> Out<(u32, u32, u32, u32), B::Error> {
    let begin = read(b, h + 4)?;
    let end = read(b, h + 8)?;
    let cap = read(b, h + 12)?;
    let len = if begin == 0 {
        if end != 0 || cap != 0 {
            return Err(DynamicCollisionError::StorageRange);
        }
        0
    } else {
        let n = count(begin, end)?;
        if n > count(begin, cap)? {
            return Err(DynamicCollisionError::StorageRange);
        }
        n
    };
    Ok((begin, end, cap, len))
}
/// Original pointer-vector append allocation policy used by these workers.
/// Byte zero of the native header is retained; no alternate vector is created.
fn append<B: VehicleInitBackend + ?Sized>(b: &mut B, h: u32, value: u32) -> Out<(), B::Error> {
    let (begin, end, cap, len) = vector(b, h)?;
    if end == cap {
        let capacity = len
            .checked_add(len.max(1))
            .filter(|n| *n <= MAX_WORDS)
            .ok_or(DynamicCollisionError::StorageQuotaExceeded)?;
        let fresh = allocate(b, capacity * 4)?;
        for i in 0..len {
            let word = read(b, begin + i * 4)?;
            write(b, fresh + i * 4, word)?;
        }
        write(b, fresh + len * 4, value)?;
        free(b, begin)?;
        write(b, h + 12, fresh + capacity * 4)?;
        write(b, h + 8, fresh + (len + 1) * 4)?;
        write(b, h + 4, fresh)?;
    } else {
        write(b, end, value)?;
        write(b, h + 8, end + 4)?;
    }
    Ok(())
}
fn erase_value<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    h: u32,
    value: u32,
) -> Out<bool, B::Error> {
    let (begin, end, _, len) = vector(b, h)?;
    for i in 0..len {
        let at = begin + i * 4;
        if read(b, at)? == value {
            let mut cursor = at + 4;
            while cursor != end {
                let word = read(b, cursor)?;
                write(b, cursor - 4, word)?;
                cursor += 4;
            }
            write(b, h + 8, end - 4)?;
            return Ok(true);
        }
    }
    Ok(false)
}
fn empty<B: VehicleInitBackend + ?Sized>(b: &mut B, h: u32) -> Out<bool, B::Error> {
    Ok(vector(b, h)?.3 == 0)
}
fn truncate<E>(value: f32) -> Out<u32, E> {
    if !value.is_finite() || !(-2147483648.0..2147483648.0).contains(&value) {
        return Err(DynamicCollisionError::NumericDomain);
    }
    let result = value.trunc() as i32;
    crate::native_precision::record_precision(result as f32 != value);
    Ok(result as u32)
}

/// The dynamic-grid continuation inside LoadCollision, PC 0x5c33d3..0x5c354e.
/// The caller supplies the original KLZ bounds and raw header+0x20 field.
/// Existing allocations and reached writes are retained on an error.
pub fn initialize_dynamic_grid<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    source: DynamicCollisionGridSource,
) -> Out<u32, B::Error> {
    if source.desired_cell_size <= 0. {
        return Err(DynamicCollisionError::InvalidGrid);
    }
    let mut grid = read(b, manager + 0x28c)?;
    if grid != 0 {
        let old = read(b, grid + 0x18)?;
        free(b, old)?;
    } else {
        grid = allocate(b, 0x1c)?;
        write(b, manager + 0x28c, grid)?;
    }
    let extents = [
        arithmetic(collision_swept::sub(source.max_xz[0], source.min_xz[0]))?,
        arithmetic(collision_swept::sub(source.max_xz[1], source.min_xz[1]))?,
    ];
    if extents.iter().any(|x| *x <= 0.) {
        return Err(DynamicCollisionError::InvalidGrid);
    }
    let width = truncate(arithmetic(collision_swept::div(
        extents[0],
        source.desired_cell_size,
    ))?)?;
    write(b, grid + 0x10, width)?;
    let height = truncate(arithmetic(collision_swept::div(
        extents[1],
        source.desired_cell_size,
    ))?)?;
    write(b, grid + 0x14, height)?;
    let width = width.max(1);
    let height = height.max(1);
    write(b, grid + 0x10, width)?;
    write(b, grid + 0x14, height)?;
    let cells = width
        .checked_mul(height)
        .filter(|n| *n <= MAX_WORDS)
        .ok_or(DynamicCollisionError::StorageQuotaExceeded)?;
    let step_x = arithmetic(collision_swept::div(extents[0], width as f32))?;
    write(b, grid + 8, step_x.to_bits())?;
    let step_z = arithmetic(collision_swept::div(extents[1], height as f32))?;
    write(b, grid + 12, step_z.to_bits())?;
    write(b, grid, source.min_xz[0].to_bits())?;
    write(b, grid + 4, source.min_xz[1].to_bits())?;
    let table = allocate(b, cells * 4)?;
    write(b, grid + 0x18, table)?;
    for i in 0..cells {
        write(b, table + i * 4, 0)?;
    }
    for h in [manager + 0x290, manager + 0x2a0] {
        let begin = read(b, h + 4)?;
        write(b, h + 8, begin)?;
    }
    Ok(grid)
}
fn location<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    grid: u32,
    object: u32,
) -> Out<(u32, u32, u32), B::Error> {
    let width = read(b, grid + 0x10)?;
    let height = read(b, grid + 0x14)?;
    if width == 0 || height == 0 {
        return Err(DynamicCollisionError::InvalidGrid);
    }
    let mut indexes = [0; 2];
    for (axis, (position, min, step)) in [
        (object + 0x10, grid, grid + 8),
        (object + 0x18, grid + 4, grid + 12),
    ]
    .into_iter()
    .enumerate()
    {
        let p = float(b, position)?;
        let m = float(b, min)?;
        let s = float(b, step)?;
        indexes[axis] = truncate(arithmetic(collision_swept::div(
            arithmetic(collision_swept::sub(p, m))?,
            s,
        ))?)?;
    }
    let x = indexes[0].min(width - 1);
    let z = indexes[1].min(height - 1);
    let table = read(b, grid + 0x18)?;
    let index = width
        .checked_mul(z)
        .and_then(|v| v.checked_add(x))
        .filter(|n| *n < MAX_WORDS)
        .ok_or(DynamicCollisionError::StorageQuotaExceeded)?;
    Ok((x, z, table + index * 4))
}
fn new_cell<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    object: u32,
    x: u32,
    z: u32,
    slot: u32,
    publish: bool,
) -> Out<u32, B::Error> {
    let (_, end, _, len) = vector(b, manager + 0x290)?;
    let cell = if len != 0 {
        let cell = read(b, end - 4)?;
        if publish {
            write(b, object + 0xc, cell)?;
        }
        write(b, manager + 0x298, end - 4)?;
        cell
    } else {
        let cell = allocate(b, 0x18)?;
        write(b, cell + 0xc, 0)?;
        b.write_byte(cell + 8, object as u8)
            .map_err(DynamicCollisionError::Backend)?;
        write(b, cell + 0x10, 0)?;
        write(b, cell + 0x14, 0)?;
        if publish {
            write(b, object + 0xc, cell)?;
        }
        let data = allocate(b, 0x20)?;
        free(b, 0)?;
        write(b, cell + 0x14, data + 0x20)?;
        write(b, cell + 0xc, data)?;
        write(b, cell + 0x10, data)?;
        cell
    };
    write(b, slot, cell)?;
    write(b, cell, x)?;
    write(b, cell + 4, z)?;
    Ok(cell)
}
/// Original DynInsert, PC RVA 0x1c36c0.
pub fn dyn_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    object: u32,
) -> Out<(), B::Error> {
    let grid = read(b, manager + 0x28c)?;
    if grid == 0 {
        return Ok(());
    }
    if read(b, object + 0xc)? != 0 {
        return Ok(());
    }
    let (x, z, slot) = location(b, grid, object)?;
    let mut cell = read(b, slot)?;
    write(b, object + 0xc, cell)?;
    if cell == 0 {
        cell = new_cell(b, manager, object, x, z, slot, true)?;
    }
    append(b, cell + 8, object)
}
/// Original DynErase, PC RVA 0x1c38b0. Empty cells are deferred, not freed here.
pub fn dyn_erase<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    object: u32,
) -> Out<(), B::Error> {
    if read(b, manager + 0x28c)? == 0 {
        return Ok(());
    }
    let cell = read(b, object + 0xc)?;
    if cell == 0 {
        return Ok(());
    }
    if erase_value(b, cell + 8, object)? && empty(b, cell + 8)? {
        append(b, manager + 0x2a0, cell)?;
    }
    write(b, object + 0xc, 0)
}
/// Original DynUpdate, PC RVA 0x1c3ac0. Moving within the same cell is a no-op.
pub fn dyn_update<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    object: u32,
) -> Out<(), B::Error> {
    let grid = read(b, manager + 0x28c)?;
    if grid == 0 {
        return Ok(());
    }
    let (x, z, slot) = location(b, grid, object)?;
    let old = read(b, object + 0xc)?;
    let mut cell = read(b, slot)?;
    if cell == old {
        return Ok(());
    }
    if old != 0 && erase_value(b, old + 8, object)? && empty(b, old + 8)? {
        append(b, manager + 0x2a0, old)?;
    }
    if cell == 0 {
        cell = new_cell(b, manager, object, x, z, slot, false)?;
    }
    append(b, cell + 8, object)?;
    write(b, object + 0xc, cell)
}
/// Original DynOptimizeTick, PC RVA 0x1c3610. Reused cells retain their reserved
/// word storage. A pending cell that received another object is not recycled.
pub fn dyn_optimize_tick<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
) -> Out<(), B::Error> {
    loop {
        let (_, end, _, len) = vector(b, manager + 0x2a0)?;
        if len == 0 {
            break;
        }
        let cell = read(b, end - 4)?;
        if empty(b, cell + 8)? {
            let grid = read(b, manager + 0x28c)?;
            if grid == 0 {
                return Err(DynamicCollisionError::InvalidGrid);
            }
            let x = read(b, cell)?;
            let z = read(b, cell + 4)?;
            let width = read(b, grid + 0x10)?;
            let height = read(b, grid + 0x14)?;
            if x >= width || z >= height {
                return Err(DynamicCollisionError::InvalidGrid);
            }
            let table = read(b, grid + 0x18)?;
            write(b, table + (z * width + x) * 4, 0)?;
            append(b, manager + 0x290, cell)?;
        }
        write(b, manager + 0x2a8, end - 4)?;
    }
    Ok(())
}

/// Original dynamic-query eligibility prefix shared by LineInDynamics,
/// SphereInDynamics and BoxInDynamic: expanded cell range, membership order,
/// candidate+8 selector rejection, then exact descriptor identity exclusion.
/// A returned candidate still requires the original dynamic narrow phase; it
/// must not be turned into a fabricated hit/miss. None proves that no narrow
/// phase is reached, including when every registered object is excluded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicQueryKind {
    Line,
    Sphere,
    Box,
}
pub fn first_unfiltered_dynamic_candidate<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    min_xz: [f32; 2],
    max_xz: [f32; 2],
    selector: u32,
    kind: DynamicQueryKind,
) -> Out<Option<u32>, B::Error> {
    if selector == u32::MAX {
        return Ok(None);
    }
    let grid = read(b, manager + 0x28c)?;
    if grid == 0 {
        return Ok(None);
    }
    let width = read(b, grid + 0x10)?;
    let height = read(b, grid + 0x14)?;
    if width == 0 || height == 0 || width.checked_mul(height).is_none_or(|n| n > MAX_WORDS) {
        return Err(DynamicCollisionError::InvalidGrid);
    }
    let mut begin = [0_i32; 2];
    let mut end = [0_i32; 2];
    for axis in 0..2 {
        let min = float(b, grid + axis as u32 * 4)?;
        let step = float(b, grid + 8 + axis as u32 * 4)?;
        let inverse = arithmetic(collision_swept::div(1., step))?;
        let lo = arithmetic(collision_swept::mul(
            arithmetic(collision_swept::sub(min_xz[axis], min))?,
            inverse,
        ))?;
        let hi = arithmetic(collision_swept::mul(
            arithmetic(collision_swept::sub(max_xz[axis], min))?,
            inverse,
        ))?;
        begin[axis] = (truncate(arithmetic(collision_swept::sub(lo, 1.))?)? as i32).max(0);
        let high = truncate(arithmetic(collision_swept::add(hi, 2.))?)?;
        let limit = [width, height][axis];
        end[axis] = high.min(limit) as i32;
    }
    for z in begin[1]..end[1] {
        for x in begin[0]..end[0] {
            let table = read(b, grid + 0x18)?;
            let cell = read(b, table + (z as u32 * width + x as u32) * 4)?;
            if cell == 0 {
                continue;
            }
            let (begin, _, _, len) = vector(b, cell + 8)?;
            for i in 0..len {
                let object = read(b, begin + i * 4)?;
                if kind != DynamicQueryKind::Sphere && read(b, object + 8)? & selector != 0 {
                    continue;
                }
                let count = read(b, manager + 0x2bc)?;
                if count > MAX_WORDS {
                    return Err(DynamicCollisionError::StorageQuotaExceeded);
                }
                let mut excluded = false;
                if count != 0 {
                    let list = read(b, manager + 0x2b8)?;
                    for j in (0..count).rev() {
                        if read(b, list + j * 4)? == object {
                            excluded = true;
                            break;
                        }
                    }
                }
                if !excluded
                    && (kind != DynamicQueryKind::Sphere || read(b, object + 8)? & selector == 0)
                {
                    return Ok(Some(object));
                }
            }
        }
    }
    Ok(None)
}

/// Original DynWipeOut field/ownership continuation, PC RVA0x1c3da0.
/// `scratch_header` is a caller-owned, fresh 16-byte temporary vector header;
/// its item buffer uses the same allocator as the manager. The original uses
/// a temporary stack header. Object backpointers are deliberately untouched.
pub fn dyn_wipe_out<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    scratch_header: u32,
) -> Out<(), B::Error> {
    let grid = read(b, manager + 0x28c)?;
    if grid == 0 {
        return Ok(());
    }
    let width = read(b, grid + 0x10)?;
    let height = read(b, grid + 0x14)?;
    if width.checked_mul(height).is_none_or(|n| n > MAX_WORDS) {
        return Err(DynamicCollisionError::InvalidGrid);
    }
    for z in 0..height {
        for x in 0..width {
            let table = read(b, grid + 0x18)?;
            let slot = table + (z * width + x) * 4;
            let cell = read(b, slot)?;
            if cell != 0 && !empty(b, cell + 8)? {
                let begin = read(b, cell + 0xc)?;
                write(b, cell + 0x10, begin)?;
                append(b, manager + 0x2a0, cell)?;
                write(b, slot, 0)?;
            }
        }
    }
    for offset in [4, 8, 12] {
        write(b, scratch_header + offset, 0)?;
    }
    for source_header in [manager + 0x290, manager + 0x2a0] {
        let (begin, _, _, len) = vector(b, source_header)?;
        for i in 0..len {
            let cell = read(b, begin + i * 4)?;
            let (temporary, _, _, count) = vector(b, scratch_header)?;
            let mut found = false;
            for j in 0..count {
                if read(b, temporary + j * 4)? == cell {
                    found = true;
                    break;
                }
            }
            if !found {
                append(b, scratch_header, cell)?;
            }
        }
        write(b, source_header + 8, begin)?;
    }
    let (begin, _, _, len) = vector(b, scratch_header)?;
    for i in 0..len {
        let cell = read(b, begin + i * 4)?;
        if cell == 0 {
            return Err(DynamicCollisionError::StorageRange);
        }
        let data = read(b, cell + 0xc)?;
        write(b, cell + 0x10, data)?;
        free(b, data)?;
        for offset in [0xc, 0x10, 0x14] {
            write(b, cell + offset, 0)?;
        }
        free(b, cell)?;
    }
    write(b, scratch_header + 8, begin)?;
    free(b, begin)?;
    for offset in [4, 8, 12] {
        write(b, scratch_header + offset, 0)?;
    }
    Ok(())
}
/// Original ReleaseCollision, PC RVA0x1c28d0. This does not delete cell owners;
/// DynWipeOut must run first so resetting pool ends cannot lose their pointers.
pub fn release_collision<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
) -> Out<(), B::Error> {
    let static_grid = read(b, manager + 0x284)?;
    if static_grid != 0 {
        let cells = read(b, static_grid + 0x2c)?;
        if cells != 0 {
            free(b, cells)?;
        }
        free(b, static_grid)?;
        write(b, manager + 0x284, 0)?;
    }
    let dynamic = read(b, manager + 0x28c)?;
    if dynamic != 0 {
        let cells = read(b, dynamic + 0x18)?;
        free(b, cells)?;
        free(b, dynamic)?;
        write(b, manager + 0x28c, 0)?;
        for header in [manager + 0x290, manager + 0x2a0] {
            let begin = read(b, header + 4)?;
            write(b, header + 8, begin)?;
        }
    }
    Ok(())
}
/// Original manager destructor storage tail, PC RVA0x1c2880. Cell ownership
/// and grids have already been released; only vector capacity buffers remain.
pub fn destroy_manager_storage<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
) -> Out<(), B::Error> {
    for header in [manager + 0x2a0, manager + 0x290] {
        let begin = read(b, header + 4)?;
        free(b, begin)?;
        for offset in [4, 8, 12] {
            write(b, header + offset, 0)?;
        }
    }
    Ok(())
}
/// Host teardown composition after all actors completed their own teardown.
/// The ordering is required to release precisely this manager's owned graph.
pub fn release_owned_dynamic_grid<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    scratch_header: u32,
) -> Out<(), B::Error> {
    dyn_wipe_out(b, manager, scratch_header)?;
    release_collision(b, manager)?;
    destroy_manager_storage(b, manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;
    const MANAGER: u32 = 0x1000;
    const OBJECT: u32 = 0x2000;
    fn setup() -> VehicleFrameMemory {
        let mut b = VehicleFrameMemory::default();
        b.map_bytes(MANAGER, &[0; 0x300]).unwrap();
        initialize_dynamic_grid(
            &mut b,
            MANAGER,
            DynamicCollisionGridSource {
                min_xz: [0.; 2],
                max_xz: [40.; 2],
                desired_cell_size: 20.,
            },
        )
        .unwrap();
        b
    }
    fn object(b: &mut VehicleFrameMemory, address: u32, x: f32, z: f32, mask: u32) {
        b.map_bytes(address, &[0; 0x48]).unwrap();
        write(b, address + 8, mask).unwrap();
        write(b, address + 0x10, x.to_bits()).unwrap();
        write(b, address + 0x18, z.to_bits()).unwrap();
    }
    fn table(b: &mut VehicleFrameMemory) -> u32 {
        let grid = read(b, MANAGER + 0x28c).unwrap();
        read(b, grid + 0x18).unwrap()
    }
    #[test]
    fn membership_moves_defer_empty_cells_and_reuse_owned_cell_storage() {
        let mut b = setup();
        object(&mut b, OBJECT, 5., 5., 0x201);
        dyn_insert(&mut b, MANAGER, OBJECT).unwrap();
        let first = read(&mut b, OBJECT + 0xc).unwrap();
        let storage = read(&mut b, first + 0xc).unwrap();
        let cell_table = table(&mut b);
        assert_eq!(read(&mut b, cell_table).unwrap(), first);
        dyn_insert(&mut b, MANAGER, OBJECT).unwrap();
        assert_eq!(vector(&mut b, first + 8).unwrap().3, 1);
        write(&mut b, OBJECT + 0x10, 25_f32.to_bits()).unwrap();
        dyn_update(&mut b, MANAGER, OBJECT).unwrap();
        let second = read(&mut b, OBJECT + 0xc).unwrap();
        assert_ne!(first, second);
        assert_eq!(read(&mut b, cell_table).unwrap(), first);
        assert_eq!(vector(&mut b, MANAGER + 0x2a0).unwrap().3, 1);
        dyn_optimize_tick(&mut b, MANAGER).unwrap();
        assert_eq!(read(&mut b, cell_table).unwrap(), 0);
        assert_eq!(vector(&mut b, MANAGER + 0x290).unwrap().3, 1);
        write(&mut b, OBJECT + 0x10, 5_f32.to_bits()).unwrap();
        write(&mut b, OBJECT + 0x18, 25_f32.to_bits()).unwrap();
        dyn_update(&mut b, MANAGER, OBJECT).unwrap();
        assert_eq!(read(&mut b, OBJECT + 0xc).unwrap(), first);
        assert_eq!(read(&mut b, first + 0xc).unwrap(), storage);
        assert_eq!(
            (
                read(&mut b, first).unwrap(),
                read(&mut b, first + 4).unwrap()
            ),
            (0, 1)
        );
        dyn_optimize_tick(&mut b, MANAGER).unwrap();
        dyn_erase(&mut b, MANAGER, OBJECT).unwrap();
        assert_eq!(read(&mut b, OBJECT + 0xc).unwrap(), 0);
        dyn_optimize_tick(&mut b, MANAGER).unwrap();
        assert_eq!(vector(&mut b, MANAGER + 0x290).unwrap().3, 2);
        for i in 0..4 {
            assert_eq!(read(&mut b, cell_table + i * 4).unwrap(), 0);
        }
    }
    #[test]
    fn pending_repopulated_cell_is_kept_and_query_masks_and_groups_use_heap_order() {
        let mut b = setup();
        object(&mut b, OBJECT, 5., 5., 0x201);
        object(&mut b, OBJECT + 0x100, 6., 6., 0);
        dyn_insert(&mut b, MANAGER, OBJECT).unwrap();
        let cell = read(&mut b, OBJECT + 0xc).unwrap();
        dyn_erase(&mut b, MANAGER, OBJECT).unwrap();
        dyn_insert(&mut b, MANAGER, OBJECT + 0x100).unwrap();
        dyn_optimize_tick(&mut b, MANAGER).unwrap();
        let cell_table = table(&mut b);
        assert_eq!(read(&mut b, cell_table).unwrap(), cell);
        assert_eq!(vector(&mut b, MANAGER + 0x290).unwrap().3, 0);
        write(&mut b, MANAGER + 0x2bc, 1).unwrap();
        write(&mut b, MANAGER + 0x2b8, MANAGER + 0x2b4).unwrap();
        write(&mut b, MANAGER + 0x2b4, OBJECT + 0x100).unwrap();
        for kind in [
            DynamicQueryKind::Line,
            DynamicQueryKind::Sphere,
            DynamicQueryKind::Box,
        ] {
            assert_eq!(
                first_unfiltered_dynamic_candidate(&mut b, MANAGER, [4.; 2], [7.; 2], 2, kind)
                    .unwrap(),
                None
            );
        }
        write(&mut b, MANAGER + 0x2bc, 0).unwrap();
        assert_eq!(
            first_unfiltered_dynamic_candidate(
                &mut b,
                MANAGER,
                [4.; 2],
                [7.; 2],
                2,
                DynamicQueryKind::Sphere
            )
            .unwrap(),
            Some(OBJECT + 0x100)
        );
        write(&mut b, OBJECT + 0x108, 0x201).unwrap();
        assert_eq!(
            first_unfiltered_dynamic_candidate(
                &mut b,
                MANAGER,
                [4.; 2],
                [7.; 2],
                0xf02,
                DynamicQueryKind::Line
            )
            .unwrap(),
            None
        );
    }
    #[test]
    fn teardown_deduplicates_active_pending_and_pooled_cells_without_touching_backpointers() {
        let mut b = setup();
        b.map_bytes(0x7000, &[0; 16]).unwrap();
        for (i, x, z) in [(0, 5., 5.), (1, 25., 5.), (2, 5., 25.)] {
            let address = OBJECT + i * 0x100;
            object(&mut b, address, x, z, 0);
            dyn_insert(&mut b, MANAGER, address).unwrap();
        }
        dyn_erase(&mut b, MANAGER, OBJECT).unwrap();
        dyn_optimize_tick(&mut b, MANAGER).unwrap();
        dyn_erase(&mut b, MANAGER, OBJECT + 0x100).unwrap();
        object(&mut b, OBJECT + 0x300, 25., 5., 0);
        dyn_insert(&mut b, MANAGER, OBJECT + 0x300).unwrap();
        let old = read(&mut b, OBJECT + 0x30c).unwrap();
        assert_ne!(old, 0);
        release_owned_dynamic_grid(&mut b, MANAGER, 0x7000).unwrap();
        assert_eq!(read(&mut b, OBJECT + 0x30c).unwrap(), old);
        assert_eq!(read(&mut b, MANAGER + 0x28c).unwrap(), 0);
        for offset in [0x294, 0x298, 0x29c, 0x2a4, 0x2a8, 0x2ac] {
            assert_eq!(read(&mut b, MANAGER + offset).unwrap(), 0);
        }
        assert_eq!(b.allocations().count(), 0);
    }
    #[test]
    fn insertion_grows_original_word_vector_and_negative_indices_use_unsigned_clamp() {
        let mut b = setup();
        for i in 0..9 {
            let object_address = OBJECT + i * 0x100;
            object(&mut b, object_address, 5., 5., 0);
            dyn_insert(&mut b, MANAGER, object_address).unwrap();
        }
        let cell = read(&mut b, OBJECT + 0xc).unwrap();
        let (begin, _, cap, len) = vector(&mut b, cell + 8).unwrap();
        assert_eq!(len, 9);
        assert_eq!(count::<()>(begin, cap).unwrap(), 16);
        for i in 0..9 {
            assert_eq!(read(&mut b, begin + i * 4).unwrap(), OBJECT + i * 0x100);
        }
        object(&mut b, OBJECT + 0x1000, -25., 5., 0);
        dyn_insert(&mut b, MANAGER, OBJECT + 0x1000).unwrap();
        let edge = read(&mut b, OBJECT + 0x100c).unwrap();
        assert_eq!(read(&mut b, edge).unwrap(), 1);
    }
}
