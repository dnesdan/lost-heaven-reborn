//! Original plain-glass constructor fields, Init/Done and completed-entry ring.
use crate::break_plain_glass::{BreakPlainGlassBackend, GlassError};
use crate::break_plain_glass_merge::{merge_glass_fragments, GlassMergeBackend};
use crate::vehicle_init::VehicleInitBackend;
#[cfg(test)]
#[path = "break_plain_glass_lifecycle_tests.rs"]
mod tests;

type Out<B, T> = Result<T, GlassError<<B as VehicleInitBackend>::Error>>;
fn at<E>(a: u32, offset: u32) -> Result<u32, GlassError<E>> {
    a.checked_add(offset).ok_or(GlassError::StorageRange)
}
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u32> {
    b.read_word(a).map_err(GlassError::Host)
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, value: u32) -> Out<B, ()> {
    b.write_word(a, value).map_err(GlassError::Host)
}
fn pointer_count<E>(begin: u32, end: u32) -> Result<u32, GlassError<E>> {
    if begin == 0 {
        return if end == 0 {
            Ok(0)
        } else {
            Err(GlassError::StorageRange)
        };
    }
    if end < begin || !(end - begin).is_multiple_of(4) {
        return Err(GlassError::StorageRange);
    }
    Ok((end - begin) / 4)
}
fn vector<B: VehicleInitBackend>(b: &mut B, header: u32) -> Out<B, (u32, u32, u32)> {
    let begin = word(b, at(header, 4)?)?;
    let end = word(b, at(header, 8)?)?;
    let capacity = word(b, at(header, 12)?)?;
    let length = pointer_count(begin, end)?;
    let capacity = pointer_count(begin, capacity)?;
    if length > capacity {
        return Err(GlassError::StorageRange);
    }
    Ok((begin, length, capacity))
}
fn destroy_vector<B: VehicleInitBackend>(b: &mut B, header: u32) -> Out<B, ()> {
    let begin = word(b, at(header, 4)?)?;
    b.free(begin).map_err(GlassError::Host)?;
    for offset in [4, 8, 12] {
        put(b, at(header, offset)?, 0)?;
    }
    Ok(())
}
fn assign_vector<B: VehicleInitBackend>(b: &mut B, destination: u32, source: u32) -> Out<B, ()> {
    if destination == source {
        return Ok(());
    }
    let (source_begin, source_length, _) = vector(b, source)?;
    let (mut destination_begin, _, capacity) = vector(b, destination)?;
    if source_length > capacity {
        b.free(destination_begin).map_err(GlassError::Host)?;
        destination_begin = b
            .allocate(
                source_length
                    .checked_mul(4)
                    .ok_or(GlassError::StorageRange)?,
            )
            .map_err(GlassError::Host)?;
        put(b, at(destination, 4)?, destination_begin)?;
        // Source placement-copy skips a null destination but still advances
        // its iterator. A later nonnull invalid address remains a host error.
        for index in 0..source_length {
            let target = at(destination_begin, index * 4)?;
            if target != 0 {
                let value = word(b, at(source_begin, index * 4)?)?;
                put(b, target, value)?;
            }
        }
        let end = at(destination_begin, source_length * 4)?;
        put(b, at(destination, 12)?, end)?;
        return put(b, at(destination, 8)?, end);
    }
    for index in 0..source_length {
        let value = word(b, at(source_begin, index * 4)?)?;
        put(b, at(destination_begin, index * 4)?, value)?;
    }
    put(
        b,
        at(destination, 8)?,
        at(destination_begin, source_length * 4)?,
    )
}

/// Embedded C_game constructor prefix at54752c and5479c0. Remaining manager
/// fields belong to Init; the source does not initialize their padding here.
pub fn glass_construct<B: VehicleInitBackend>(b: &mut B, manager: u32, marker: u8) -> Out<B, ()> {
    for header in [manager, at(manager, 0x10)?] {
        b.write_byte(header, marker).map_err(GlassError::Host)?;
        for offset in [4, 8, 12] {
            put(b, at(header, offset)?, 0)?;
        }
    }
    Ok(())
}

/// PC560db0. Its newly constructed cache array is explicitly cleared by the
/// original memset, including descriptor markers and all pointer fields.
pub fn glass_init<B: VehicleInitBackend>(b: &mut B, manager: u32, cache_size: u32) -> Out<B, bool> {
    for (offset, value) in [
        (0x2c, 0),
        (0x30, 0xc1000000),
        (0x28, cache_size),
        (0x34, 0),
        (0x24, cache_size),
    ] {
        put(b, at(manager, offset)?, value)?;
    }
    let bytes = cache_size
        .checked_mul(24)
        .and_then(|n| n.checked_add(4))
        .ok_or(GlassError::StorageRange)?;
    let allocation = b.allocate(bytes).map_err(GlassError::Host)?;
    let array = if allocation == 0 {
        0
    } else {
        at(allocation, 4)?
    };
    if allocation != 0 {
        put(b, allocation, cache_size)?;
        for offset in (0..cache_size * 24).step_by(4) {
            put(b, at(array, offset)?, 0)?;
        }
    }
    put(b, at(manager, 0x20)?, array)?;
    if array == 0 {
        return Ok(false);
    }
    let begin = word(b, at(manager, 0x14)?)?;
    let end = word(b, at(manager, 0x18)?)?;
    if end < begin || !(end - begin).is_multiple_of(24) {
        return Err(GlassError::StorageRange);
    }
    for offset in (0..end - begin).step_by(24) {
        destroy_vector(b, at(begin, offset)?)?;
    }
    put(b, at(manager, 0x18)?, begin)?;
    Ok(true)
}

// Releases the frames/fragment array selected by one descriptor, retaining
// its pointer-vector capacity for subsequent assignment or destruction.
fn dispose_entry<B: BreakPlainGlassBackend>(b: &mut B, entry: u32) -> Out<B, ()> {
    let (_, length, _) = vector(b, entry)?;
    if length != 0 {
        for index in (0..length).rev() {
            let begin = word(b, at(entry, 4)?)?;
            let frame = word(b, at(begin, index * 4)?)?;
            b.glass_frame_virtual(frame, 0x2c, &[0, 0])
                .map_err(GlassError::Host)?;
            let begin = word(b, at(entry, 4)?)?;
            let frame = word(b, at(begin, index * 4)?)?;
            b.glass_frame_virtual(frame, 0, &[])
                .map_err(GlassError::Host)?;
        }
    } else if word(b, at(entry, 0x10)?)? != 0 {
        let count = word(b, at(entry, 0x14)?)?;
        for index in (0..count).rev() {
            let rows = word(b, at(entry, 0x10)?)?;
            let frame = word(
                b,
                at(
                    rows,
                    index.checked_mul(0x34).ok_or(GlassError::StorageRange)?,
                )?,
            )?;
            if frame != 0 {
                b.glass_frame_virtual(frame, 0, &[])
                    .map_err(GlassError::Host)?;
            }
        }
    }
    let rows = word(b, at(entry, 0x10)?)?;
    if rows != 0 {
        b.free(rows).map_err(GlassError::Host)?;
    }
    Ok(())
}

/// PC560f40. Ring storage is freed without clearing manager+20, as in the
/// original; its caller must pair Init/Done once for each initialized phase.
pub fn glass_done<B: BreakPlainGlassBackend>(b: &mut B, manager: u32) -> Out<B, bool> {
    let cache = word(b, at(manager, 0x20)?)?;
    if cache != 0 {
        let count = word(b, at(manager, 0x28)?)?;
        for index in (0..count).rev() {
            dispose_entry(
                b,
                at(
                    cache,
                    index.checked_mul(24).ok_or(GlassError::StorageRange)?,
                )?,
            )?;
        }
        let cache = word(b, at(manager, 0x20)?)?;
        let count = word(b, cache.checked_sub(4).ok_or(GlassError::StorageRange)?)?;
        for index in (0..count).rev() {
            destroy_vector(
                b,
                at(
                    cache,
                    index.checked_mul(24).ok_or(GlassError::StorageRange)?,
                )?,
            )?;
        }
        b.free(cache.checked_sub(4).ok_or(GlassError::StorageRange)?)
            .map_err(GlassError::Host)?;
    }
    let begin = word(b, at(manager, 0x14)?)?;
    let end = word(b, at(manager, 0x18)?)?;
    if end < begin || !(end - begin).is_multiple_of(24) {
        return Err(GlassError::StorageRange);
    }
    let count = (end - begin) / 24;
    for index in (0..count).rev() {
        dispose_entry(b, at(begin, index * 24)?)?;
    }
    for index in 0..count {
        destroy_vector(b, at(begin, index * 24)?)?;
    }
    put(b, at(manager, 0x18)?, begin)?;
    let (_, originals, _) = vector(b, manager)?;
    for index in (0..originals).rev() {
        let begin = word(b, at(manager, 4)?)?;
        let frame = word(b, at(begin, index * 4)?)?;
        b.glass_frame_virtual(frame, 0x24, &[1])
            .map_err(GlassError::Host)?;
        let begin = word(b, at(manager, 4)?)?;
        let frame = word(b, at(begin, index * 4)?)?;
        b.glass_frame_virtual(frame, 0, &[])
            .map_err(GlassError::Host)?;
    }
    let begin = word(b, at(manager, 4)?)?;
    put(b, at(manager, 8)?, begin)?;
    Ok(true)
}

/// Tick561d68..561fdb: merge, evict the next ring slot, transfer the completed
/// entry and remove it from the active vector by assigning the last entry.
pub fn glass_finish_entry<B: GlassMergeBackend>(b: &mut B, manager: u32, index: u32) -> Out<B, ()> {
    let offset = index.checked_mul(24).ok_or(GlassError::StorageRange)?;
    let begin = word(b, at(manager, 0x14)?)?;
    let entry = at(begin, offset)?;
    let _ = merge_glass_fragments(b, entry)?;
    let next = word(b, at(manager, 0x24)?)?.wrapping_add(1);
    put(b, at(manager, 0x24)?, next)?;
    let capacity = word(b, at(manager, 0x28)?)?;
    if next as i32 >= capacity as i32 {
        put(b, at(manager, 0x24)?, 0)?;
    }
    let ring_index = word(b, at(manager, 0x24)?)?;
    let cache = word(b, at(manager, 0x20)?)?;
    let destination = at(
        cache,
        ring_index.checked_mul(24).ok_or(GlassError::StorageRange)?,
    )?;
    dispose_entry(b, destination)?;
    assign_vector(b, destination, entry)?;
    for offset in [0x10, 0x14] {
        let value = word(b, at(entry, offset)?)?;
        put(b, at(destination, offset)?, value)?;
    }
    let end = word(b, at(manager, 0x18)?)?;
    let last = end.checked_sub(24).ok_or(GlassError::StorageRange)?;
    let begin = word(b, at(manager, 0x14)?)?;
    let entry = at(begin, offset)?;
    if entry != last {
        assign_vector(b, entry, last)?;
    }
    for offset in [0x10, 0x14] {
        let value = word(b, at(last, offset)?)?;
        put(b, at(entry, offset)?, value)?;
    }
    destroy_vector(b, last)?;
    let end = word(b, at(manager, 0x18)?)?;
    put(
        b,
        at(manager, 0x18)?,
        end.checked_sub(24).ok_or(GlassError::StorageRange)?,
    )
}

/// Caller-owned outer vector storage after Done; frame/array ownership was
/// already consumed by the source lifecycle worker above.
pub fn glass_free_caller_storage<B: VehicleInitBackend>(b: &mut B, manager: u32) -> Out<B, ()> {
    destroy_vector(b, at(manager, 0x10)?)?;
    destroy_vector(b, manager)
}
