//! Addressable C_car 160-byte component record insertion.
use crate::vehicle_init::VehicleInitBackend;

pub const CAR_COMPONENT_RECORD_BYTES: u32 = 160;

fn add(a: u32, b: u32) -> u32 {
    a.wrapping_add(b)
}

fn distance(begin: u32, end: u32, stride: u32) -> u32 {
    let bytes = end.wrapping_sub(begin) as i32;
    (bytes / stride as i32) as u32
}

fn copy_record<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    destination: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
    for offset in (0..stride).step_by(4) {
        let value = b.read_word(add(source, offset))?;
        b.write_word(add(destination, offset), value)?;
    }
    Ok(())
}

fn placement_copy<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut destination: u32,
    mut begin: u32,
    end: u32,
    stride: u32,
) -> Result<u32, B::Error> {
    while begin != end {
        if destination != 0 {
            copy_record(b, destination, begin, stride)?;
        }
        begin = add(begin, stride);
        destination = add(destination, stride);
    }
    Ok(destination)
}

fn placement_fill<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut destination: u32,
    count: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
    for _ in 0..count {
        if destination != 0 {
            copy_record(b, destination, source, stride)?;
        }
        destination = add(destination, stride);
    }
    Ok(())
}

fn assign_fill<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut destination: u32,
    end: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
    while destination != end {
        copy_record(b, destination, source, stride)?;
        destination = add(destination, stride);
    }
    Ok(())
}

/// PC entry 0x42db00 (RVA 0x2db00): insert full 160-byte records.
/// Sources are read word by word at each copy, preserving insertion aliases.
/// Host failures keep the completed allocation/copy/free/header-store prefix.
pub fn car_component_records_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    let stride = CAR_COMPONENT_RECORD_BYTES;
    let end = b.read_word(add(header, 8))?;
    let capacity = b.read_word(add(header, 12))?;
    let inserted_bytes = count.wrapping_mul(stride);
    if distance(end, capacity, stride) < count {
        let begin = b.read_word(add(header, 4))?;
        let size = if begin == 0 {
            0
        } else {
            distance(begin, end, stride)
        };
        let capacity_count = size.wrapping_add(size.max(count));
        let allocation_count = if (capacity_count as i32) < 0 {
            0
        } else {
            capacity_count
        };
        let allocation = b.allocate(allocation_count.wrapping_mul(stride))?;
        let begin = b.read_word(add(header, 4))?;
        let middle = placement_copy(b, allocation, begin, position, stride)?;
        placement_fill(b, middle, count, source, stride)?;
        let end = b.read_word(add(header, 8))?;
        placement_copy(b, add(middle, inserted_bytes), position, end, stride)?;
        let old_allocation = b.read_word(add(header, 4))?;
        b.free(old_allocation)?;

        // This specialization stores capacity before re-reading the live
        // begin/end fields. Allocator callbacks and header aliases remain live.
        let new_capacity = add(allocation, capacity_count.wrapping_mul(stride));
        b.write_word(add(header, 12), new_capacity)?;
        let begin = b.read_word(add(header, 4))?;
        let size = if begin == 0 {
            0
        } else {
            let end = b.read_word(add(header, 8))?;
            distance(begin, end, stride)
        };
        b.write_word(add(header, 4), allocation)?;
        b.write_word(
            add(header, 8),
            add(allocation, size.wrapping_add(count).wrapping_mul(stride)),
        )?;
    } else if distance(position, end, stride) < count {
        placement_copy(b, add(position, inserted_bytes), position, end, stride)?;
        let end = b.read_word(add(header, 8))?;
        let remaining = count.wrapping_sub(distance(position, end, stride));
        placement_fill(b, end, remaining, source, stride)?;
        let end = b.read_word(add(header, 8))?;
        assign_fill(b, position, end, source, stride)?;
        let end = b.read_word(add(header, 8))?;
        b.write_word(add(header, 8), add(end, inserted_bytes))?;
    } else if count != 0 {
        placement_copy(b, end, end.wrapping_sub(inserted_bytes), end, stride)?;
        let mut destination = b.read_word(add(header, 8))?;
        let mut cursor = destination.wrapping_sub(inserted_bytes);
        while cursor != position {
            cursor = cursor.wrapping_sub(stride);
            destination = destination.wrapping_sub(stride);
            copy_record(b, destination, cursor, stride)?;
        }
        assign_fill(b, position, add(position, inserted_bytes), source, stride)?;
        let end = b.read_word(add(header, 8))?;
        b.write_word(add(header, 8), add(end, inserted_bytes))?;
    }
    Ok(())
}
