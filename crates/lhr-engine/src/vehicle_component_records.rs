//! Addressable PC component record insertion, with descriptive working names.
//!
//! Headers contain begin/end/capacity at +4/+8/+12. The backend owns storage;
//! these routines neither acquire frame references nor release them. Copies
//! include every record word, including padding supplied by the caller.
//! Errors retain the completed read/write/allocation/free prefix. A native
//! null allocation is processed by the original placement-copy rules.

use crate::vehicle_init::VehicleInitBackend;

pub const VEHICLE_DOOR_RECORD_BYTES: u32 = 88;
pub const VEHICLE_BALLAST_RECORD_BYTES: u32 = 28;
pub const VEHICLE_SEAT_RECORD_BYTES: u32 = 36;
pub const VEHICLE_EXHAUST_RECORD_BYTES: u32 = 24;

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

// Common instruction-grounded algorithm of the three record specializations
// and the pointer specialization. No temporary value caches the source record:
// an insertion can alias a shifted record, another destination, or the header.
fn insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    count: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
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

/// Original PC 0x4d23e0: insert `count` full 88-byte DOOR records.
pub fn vehicle_door_records_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    container: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    insert(
        b,
        container,
        position,
        count,
        source,
        VEHICLE_DOOR_RECORD_BYTES,
    )
}

/// Original PC 0x42d1f0: insert `count` full 28-byte BALLAST records.
pub fn vehicle_ballast_records_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    container: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    insert(
        b,
        container,
        position,
        count,
        source,
        VEHICLE_BALLAST_RECORD_BYTES,
    )
}

/// Original PC 0x55fdc0: insert `count` full 36-byte SEAT records.
pub fn vehicle_seat_records_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    container: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    insert(
        b,
        container,
        position,
        count,
        source,
        VEHICLE_SEAT_RECORD_BYTES,
    )
}

/// The inlined 20-byte skid-model-pair insertion in Vehicle GameInit. Its
/// placement copy is PC0x537910 and preserves all five source words.
pub(crate) fn vehicle_skid_model_pairs_insert<B: VehicleInitBackend + ?Sized>(
    backend: &mut B,
    container: u32,
    position: u32,
    source: u32,
) -> Result<(), B::Error> {
    insert(backend, container, position, 1, source, 0x14)
}

/// Original PC 0x4d26e0: insert full 72-byte Vehicle collision records.
pub fn vehicle_collision_records_insert<B: VehicleInitBackend + ?Sized>(
    backend: &mut B,
    container: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    insert(backend, container, position, count, source, 0x48)
}

/// Original PC 0x448cc0: pointer insertion used by BALLAST reordering.
pub fn vehicle_component_pointers_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    container: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    insert(b, container, position, count, source, 4)
}

/// Original PC 0x564750: forward word assignment, including destination zero.
pub fn vehicle_component_pointers_copy<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut begin: u32,
    end: u32,
    mut destination: u32,
) -> Result<u32, B::Error> {
    while begin != end {
        let value = b.read_word(begin)?;
        b.write_word(destination, value)?;
        begin = add(begin, 4);
        destination = add(destination, 4);
    }
    Ok(destination)
}

/// Original PC 0x42e1f0: placement copy of one EXHAUST record.
pub fn vehicle_exhaust_record_copy<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    destination: u32,
    source: u32,
) -> Result<(), B::Error> {
    if destination != 0 {
        copy_record(b, destination, source, VEHICLE_EXHAUST_RECORD_BYTES)?;
    }
    Ok(())
}

/// Original PC 0x5bc800: forward placement copy of EXHAUST records.
pub fn vehicle_exhaust_records_copy<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    begin: u32,
    end: u32,
    destination: u32,
) -> Result<u32, B::Error> {
    placement_copy(b, destination, begin, end, VEHICLE_EXHAUST_RECORD_BYTES)
}

/// Original PC 0x4d2a70: repeated placement copy; zero count returns zero.
pub fn vehicle_exhaust_records_fill<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    destination: u32,
    count: u32,
    source: u32,
) -> Result<u32, B::Error> {
    placement_fill(b, destination, count, source, VEHICLE_EXHAUST_RECORD_BYTES)?;
    Ok(if count == 0 {
        0
    } else {
        add(
            destination,
            count.wrapping_mul(VEHICLE_EXHAUST_RECORD_BYTES),
        )
    })
}

/// Original PC 0x5470c0: signed EXHAUST element distance, or zero for null begin.
pub fn vehicle_exhaust_records_size<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    container: u32,
) -> Result<u32, B::Error> {
    let begin = b.read_word(add(container, 4))?;
    Ok(if begin == 0 {
        0
    } else {
        let end = b.read_word(add(container, 8))?;
        distance(begin, end, VEHICLE_EXHAUST_RECORD_BYTES)
    })
}

/// Original PC 0x42cff0: insert one EXHAUST record and return its new address.
/// Unlike the three component insertion entries, this entry has no count argument.
pub fn vehicle_exhaust_records_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    container: u32,
    position: u32,
    source: u32,
) -> Result<u32, B::Error> {
    let stride = VEHICLE_EXHAUST_RECORD_BYTES;
    let begin = b.read_word(add(container, 4))?;
    let index = distance(begin, position, stride);
    let end = b.read_word(add(container, 8))?;
    let capacity = b.read_word(add(container, 12))?;
    if distance(end, capacity, stride) < 1 {
        let size = if begin == 0 {
            0
        } else {
            distance(begin, end, stride)
        };
        let capacity_count = size.wrapping_add(size.max(1));
        let allocation_count = if (capacity_count as i32) < 0 {
            0
        } else {
            capacity_count
        };
        let allocation = b.allocate(allocation_count.wrapping_mul(stride))?;
        let mut cursor = b.read_word(add(container, 4))?;
        let mut destination = allocation;
        while cursor != position {
            vehicle_exhaust_record_copy(b, destination, cursor)?;
            cursor = add(cursor, stride);
            destination = add(destination, stride);
        }
        vehicle_exhaust_record_copy(b, destination, source)?;
        let end = b.read_word(add(container, 8))?;
        vehicle_exhaust_records_copy(b, position, end, add(destination, stride))?;
        // Arguments of the original 0x4d2040 RET8 no-op are still loaded.
        let _destruction_end = b.read_word(add(container, 8))?;
        let _destruction_begin = b.read_word(add(container, 4))?;
        let old_allocation = b.read_word(add(container, 4))?;
        b.free(old_allocation)?;
        b.write_word(
            add(container, 12),
            add(allocation, capacity_count.wrapping_mul(stride)),
        )?;
        let size = vehicle_exhaust_records_size(b, container)?;
        b.write_word(add(container, 4), allocation)?;
        b.write_word(
            add(container, 8),
            add(allocation, size.wrapping_add(1).wrapping_mul(stride)),
        )?;
    } else {
        if distance(position, end, stride) < 1 {
            vehicle_exhaust_records_copy(b, position, end, add(position, stride))?;
            let end = b.read_word(add(container, 8))?;
            let remaining = 1u32.wrapping_sub(distance(position, end, stride));
            vehicle_exhaust_records_fill(b, end, remaining, source)?;
            let end = b.read_word(add(container, 8))?;
            assign_fill(b, position, end, source, stride)?;
        } else {
            vehicle_exhaust_records_copy(b, end.wrapping_sub(stride), end, end)?;
            let mut destination = b.read_word(add(container, 8))?;
            let mut cursor = destination.wrapping_sub(stride);
            while cursor != position {
                cursor = cursor.wrapping_sub(stride);
                destination = destination.wrapping_sub(stride);
                copy_record(b, destination, cursor, stride)?;
            }
            assign_fill(b, position, add(position, stride), source, stride)?;
        }
        let end = b.read_word(add(container, 8))?;
        b.write_word(add(container, 8), add(end, stride))?;
    }
    let begin = b.read_word(add(container, 4))?;
    Ok(add(begin, index.wrapping_mul(stride)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    use std::collections::BTreeMap;

    const HEADER: u32 = 0x100;
    const OLD: u32 = 0x1000;
    const NEW: u32 = 0x4000;
    const SOURCE: u32 = 0x8000;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Read(u32),
        Write(u32, u32),
        Allocate(u32),
        Free(u32),
    }

    #[derive(Clone, Default)]
    struct Memory {
        words: BTreeMap<u32, u32>,
        events: Vec<Event>,
        fail: Option<usize>,
        null: bool,
        change_free_header: bool,
    }

    impl Memory {
        fn event(&mut self, event: Event) -> Result<(), usize> {
            let index = self.events.len();
            if self.fail == Some(index) {
                return Err(index);
            }
            self.events.push(event);
            Ok(())
        }

        fn fixture(stride: u32, length: u32, capacity: u32) -> Self {
            let mut b = Self::default();
            b.words.insert(HEADER + 4, OLD);
            b.words.insert(HEADER + 8, OLD + length * stride);
            b.words.insert(HEADER + 12, OLD + capacity * stride);
            for offset in (0..(capacity + 8) * stride).step_by(4) {
                b.words.insert(OLD + offset, 0x10000 + offset);
                b.words.insert(NEW + offset, 0xdeadbeef);
            }
            for offset in (0..stride).step_by(4) {
                b.words.insert(SOURCE + offset, 0xa0000 + offset);
            }
            b
        }

        fn records(&self, begin: u32, n: u32, stride: u32) -> Vec<Vec<u32>> {
            (0..n)
                .map(|i| {
                    (0..stride)
                        .step_by(4)
                        .map(|offset| self.words[&(begin + i * stride + offset)])
                        .collect()
                })
                .collect()
        }
    }

    impl VehicleInitBackend for Memory {
        type Error = usize;
        fn read_word(&mut self, address: u32) -> Result<u32, usize> {
            self.event(Event::Read(address))?;
            Ok(*self.words.get(&address).unwrap_or(&0))
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), usize> {
            self.event(Event::Write(address, value))?;
            self.words.insert(address, value);
            Ok(())
        }
        fn allocate(&mut self, bytes: u32) -> Result<u32, usize> {
            self.event(Event::Allocate(bytes))?;
            Ok(if self.null { 0 } else { NEW })
        }
        fn free(&mut self, address: u32) -> Result<(), usize> {
            self.event(Event::Free(address))?;
            if self.change_free_header {
                self.words.insert(HEADER + 4, 0x9000);
                self.words.insert(HEADER + 8, 0x9000 + 2 * 88);
            }
            Ok(())
        }
        fn read_byte(&mut self, _: u32) -> Result<u8, usize> {
            unreachable!()
        }
        fn write_byte(&mut self, _: u32, _: u8) -> Result<(), usize> {
            unreachable!()
        }
        fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), usize> {
            unreachable!()
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<usize>> {
            unreachable!()
        }
    }

    #[test]
    fn insertion_branches_preserve_all_record_words() {
        for stride in [4, 20, 28, 36, 72, 88] {
            for length in 0..5 {
                for capacity in length..7 {
                    for position in 0..=length {
                        for count in 0..5 {
                            let mut b = Memory::fixture(stride, length, capacity);
                            let mut expected = b.records(OLD, length, stride);
                            let record = b.records(SOURCE, 1, stride).remove(0);
                            expected.splice(
                                position as usize..position as usize,
                                std::iter::repeat_n(record, count as usize),
                            );
                            insert(
                                &mut b,
                                HEADER,
                                OLD + position * stride,
                                count,
                                SOURCE,
                                stride,
                            )
                            .unwrap();
                            let begin = b.words[&(HEADER + 4)];
                            assert_eq!(b.records(begin, length + count, stride), expected);
                            assert_eq!(b.words[&(HEADER + 8)], begin + (length + count) * stride);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn alias_reads_observe_shift_before_fill() {
        for stride in [4, 20, 28, 36, 72, 88] {
            let mut b = Memory::fixture(stride, 4, 8);
            let before = b.records(OLD, 4, stride);
            insert(&mut b, HEADER, OLD, 1, OLD + stride, stride).unwrap();
            assert_eq!(
                b.records(OLD, 5, stride),
                vec![
                    before[0].clone(),
                    before[0].clone(),
                    before[1].clone(),
                    before[2].clone(),
                    before[3].clone()
                ]
            );
        }
    }

    #[test]
    fn failure_stops_at_each_exact_storage_or_effect_prefix() {
        for stride in [4, 20, 28, 36, 72, 88] {
            for (length, capacity, position, count) in [(3, 3, 1, 2), (3, 8, 2, 4), (3, 8, 0, 1)] {
                let initial = Memory::fixture(stride, length, capacity);
                let mut full = initial.clone();
                insert(
                    &mut full,
                    HEADER,
                    OLD + position * stride,
                    count,
                    SOURCE,
                    stride,
                )
                .unwrap();
                let mut expected = initial.words.clone();
                for (index, event) in full.events.iter().enumerate() {
                    let mut failed = initial.clone();
                    failed.fail = Some(index);
                    assert_eq!(
                        insert(
                            &mut failed,
                            HEADER,
                            OLD + position * stride,
                            count,
                            SOURCE,
                            stride
                        ),
                        Err(index)
                    );
                    assert_eq!(failed.events, full.events[..index]);
                    assert_eq!(failed.words, expected);
                    if let Event::Write(address, value) = event {
                        expected.insert(*address, *value);
                    }
                }
            }
        }
    }

    #[test]
    fn free_mutation_is_read_before_header_replacement() {
        let mut b = Memory::fixture(88, 1, 1);
        b.change_free_header = true;
        vehicle_door_records_insert(&mut b, HEADER, OLD + 88, 1, SOURCE).unwrap();
        assert_eq!(b.words[&(HEADER + 8)], NEW + 3 * 88);
        let tail = b
            .events
            .iter()
            .position(|e| *e == Event::Free(OLD))
            .unwrap();
        assert_eq!(
            &b.events[tail + 1..],
            &[
                Event::Read(HEADER + 4),
                Event::Write(HEADER + 12, NEW + 2 * 88),
                Event::Read(HEADER + 8),
                Event::Write(HEADER + 4, NEW),
                Event::Write(HEADER + 8, NEW + 3 * 88)
            ]
        );
    }

    #[test]
    fn null_placement_skips_only_the_first_destination() {
        let mut b = Memory::fixture(28, 0, 0);
        b.words.insert(HEADER + 4, 0);
        b.words.insert(HEADER + 8, 0);
        b.words.insert(HEADER + 12, 0);
        b.null = true;
        vehicle_ballast_records_insert(&mut b, HEADER, 0, 2, SOURCE).unwrap();
        assert!(!b.words.contains_key(&0));
        assert_eq!(b.words[&28], 0xa0000);
        assert_eq!(b.words[&(HEADER + 8)], 56);
    }

    #[test]
    fn zero_count_still_reads_capacity_and_preserves_header() {
        let mut b = Memory::fixture(36, 2, 2);
        let words = b.words.clone();
        vehicle_seat_records_insert(&mut b, HEADER, OLD, 0, SOURCE).unwrap();
        assert_eq!(b.words, words);
        assert_eq!(
            b.events,
            vec![Event::Read(HEADER + 8), Event::Read(HEADER + 12)]
        );
    }

    #[test]
    fn forward_pointer_assignment_preserves_overlap_and_return() {
        let mut b = Memory::fixture(4, 4, 5);
        assert_eq!(
            vehicle_component_pointers_copy(&mut b, OLD, OLD + 16, OLD + 4),
            Ok(OLD + 20)
        );
        assert_eq!(b.records(OLD, 5, 4), vec![vec![0x10000]; 5]);
    }

    #[test]
    fn signed_difference_and_wrapping_are_not_unsigned_division() {
        assert_eq!(distance(100, 99, 88), 0);
        assert_eq!(distance(100, 11, 88), u32::MAX);
        assert_eq!(distance(100, 99, 4), u32::MAX);
        assert_eq!(distance(u32::MAX - 27, 0, 28), 1);
    }

    #[test]
    fn exhaust_insertion_returns_original_index_after_growth_or_shift() {
        for length in 0..6 {
            for capacity in length..8 {
                for position in 0..=length {
                    let mut b = Memory::fixture(24, length, capacity);
                    let mut expected = b.records(OLD, length, 24);
                    expected.insert(position as usize, b.records(SOURCE, 1, 24).remove(0));
                    let inserted =
                        vehicle_exhaust_records_insert(&mut b, HEADER, OLD + position * 24, SOURCE)
                            .unwrap();
                    let begin = b.words[&(HEADER + 4)];
                    assert_eq!(inserted, begin + position * 24);
                    assert_eq!(b.records(begin, length + 1, 24), expected);
                }
            }
        }
    }

    #[test]
    fn exhaust_failures_preserve_copy_noop_argument_and_header_prefixes() {
        for (length, capacity, position) in [(3, 3, 1), (3, 8, 3), (3, 8, 0)] {
            let initial = Memory::fixture(24, length, capacity);
            let mut full = initial.clone();
            vehicle_exhaust_records_insert(&mut full, HEADER, OLD + position * 24, SOURCE).unwrap();
            let mut expected = initial.words.clone();
            for (index, event) in full.events.iter().enumerate() {
                let mut failed = initial.clone();
                failed.fail = Some(index);
                assert_eq!(
                    vehicle_exhaust_records_insert(
                        &mut failed,
                        HEADER,
                        OLD + position * 24,
                        SOURCE
                    ),
                    Err(index)
                );
                assert_eq!(failed.events, full.events[..index]);
                assert_eq!(failed.words, expected);
                if let Event::Write(address, value) = event {
                    expected.insert(*address, *value);
                }
            }
            if capacity == length {
                let free = full
                    .events
                    .iter()
                    .position(|e| *e == Event::Free(OLD))
                    .unwrap();
                assert_eq!(
                    &full.events[free - 3..free],
                    &[
                        Event::Read(HEADER + 8),
                        Event::Read(HEADER + 4),
                        Event::Read(HEADER + 4)
                    ]
                );
                assert_eq!(
                    &full.events[free + 1..free + 4],
                    &[
                        Event::Write(HEADER + 12, NEW + 6 * 24),
                        Event::Read(HEADER + 4),
                        Event::Read(HEADER + 8)
                    ]
                );
            }
        }
    }

    #[test]
    fn exhaust_helpers_preserve_zero_returns_null_placement_and_aliasing() {
        let mut b = Memory::fixture(24, 3, 5);
        vehicle_exhaust_record_copy(&mut b, 0, SOURCE).unwrap();
        assert!(b.events.is_empty());
        assert_eq!(vehicle_exhaust_records_fill(&mut b, NEW, 0, SOURCE), Ok(0));
        assert_eq!(vehicle_exhaust_records_copy(&mut b, OLD, OLD, NEW), Ok(NEW));
        assert!(b.events.is_empty());
        let first = b.records(OLD, 1, 24).remove(0);
        assert_eq!(
            vehicle_exhaust_records_copy(&mut b, OLD, OLD + 3 * 24, OLD + 24),
            Ok(OLD + 4 * 24)
        );
        assert_eq!(b.records(OLD, 4, 24), vec![first; 4]);
        b.words.insert(HEADER + 4, 0);
        b.events.clear();
        assert_eq!(vehicle_exhaust_records_size(&mut b, HEADER), Ok(0));
        assert_eq!(b.events, vec![Event::Read(HEADER + 4)]);
    }
}
