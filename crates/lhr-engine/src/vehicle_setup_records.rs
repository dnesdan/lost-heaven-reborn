//! Original repeated 12-byte Setup record insertion, PC RVA 0x20f1f0.
use super::VehicleInitBackend;

fn add(a: u32, b: u32) -> u32 {
    a.wrapping_add(b)
}

fn distance(begin: u32, end: u32, stride: u32) -> u32 {
    let bytes = end.wrapping_sub(begin) as i32;
    // The pointer specialization uses SAR, whereas the record specializations
    // use signed constant division with truncation toward zero.
    if stride == 4 {
        (bytes >> 2) as u32
    } else {
        (bytes / stride as i32) as u32
    }
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

/// Original 0x60f1f0. Copies preserve live source/header aliases and completed
/// allocation, free and field-write prefixes. This entry returns no value.
pub fn vehicle_setup_records_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    let stride = 12;
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

        // The pointer version stores capacity before re-reading begin. Record
        // versions read begin first. Both read end after the capacity store.
        let new_capacity = add(allocation, capacity_count.wrapping_mul(stride));
        let begin = if stride == 4 {
            b.write_word(add(header, 12), new_capacity)?;
            b.read_word(add(header, 4))?
        } else {
            let begin = b.read_word(add(header, 4))?;
            b.write_word(add(header, 12), new_capacity)?;
            begin
        };
        let end = if begin == 0 {
            None
        } else {
            Some(b.read_word(add(header, 8))?)
        };
        b.write_word(add(header, 4), allocation)?;
        let size = end.map_or(0, |end| distance(begin, end, stride));
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

#[cfg(test)]
mod tests {
    use super::super::wheels::tests::{Effect, Memory};
    use super::*;
    fn seeded(size: u32, capacity: u32) -> Memory {
        let mut b = Memory {
            allocations: [0x6000].into(),
            ..Memory::default()
        };
        b.word(0x1004, 0x3000);
        b.word(0x1008, 0x3000 + size * 12);
        b.word(0x100c, 0x3000 + capacity * 12);
        for index in 0..size {
            for word in 0..3 {
                b.word(0x3000 + index * 12 + word * 4, index * 10 + word);
            }
        }
        for word in 0..3 {
            b.word(0x2000 + word * 4, 100 + word);
        }
        b
    }
    #[test]
    fn all_growth_and_spare_capacity_branches_preserve_record_order() {
        for size in 0..=4 {
            for capacity in size..=size + 4 {
                for count in 0..=4 {
                    for position in 0..=size {
                        let mut b = seeded(size, capacity);
                        vehicle_setup_records_insert(
                            &mut b,
                            0x1000,
                            0x3000 + position * 12,
                            count,
                            0x2000,
                        )
                        .unwrap();
                        let begin = b.get(0x1004);
                        let mut expected: Vec<[u32; 3]> = (0..size)
                            .map(|i| [i * 10, i * 10 + 1, i * 10 + 2])
                            .collect();
                        for _ in 0..count {
                            expected.insert(position as usize, [100, 101, 102]);
                        }
                        assert_eq!(b.get(0x1008), begin + expected.len() as u32 * 12);
                        for (record, words) in expected.iter().enumerate() {
                            for (word, value) in words.iter().enumerate() {
                                assert_eq!(
                                    b.get(begin + record as u32 * 12 + word as u32 * 4),
                                    *value
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn each_unavailable_effect_preserves_completed_memory_and_allocation_prefix() {
        for (size, capacity, position, count) in [
            (0, 0, 0, 1),
            (3, 3, 1, 2),
            (3, 7, 2, 3),
            (4, 6, 1, 2),
            (3, 5, 1, 0),
        ] {
            let input = seeded(size, capacity);
            let mut full = input.clone();
            vehicle_setup_records_insert(&mut full, 0x1000, 0x3000 + position * 12, count, 0x2000)
                .unwrap();
            for failed in 0..full.effects.len() {
                let mut b = input.clone();
                b.fail = Some(failed);
                assert_eq!(
                    vehicle_setup_records_insert(
                        &mut b,
                        0x1000,
                        0x3000 + position * 12,
                        count,
                        0x2000
                    ),
                    Err(())
                );
                assert_eq!(b.effects, full.effects[..failed]);
                let mut expected = input.clone();
                for effect in &b.effects {
                    if let Effect::Write(address, bytes) = effect {
                        expected.put(*address, bytes);
                    }
                }
                assert_eq!(b.bytes, expected.bytes);
            }
        }
    }
}
