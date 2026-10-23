//! Original Game record insertion at 0x60efd0 (12-byte point) and 0x548040
//! (8-byte control). Descriptive names; headers hold begin/end/capacity at
//! +4/+8/+12. Control padding is copied verbatim from caller storage.
use super::VehicleInitBackend;
fn at(a: u32, n: u32) -> u32 {
    a.wrapping_add(n)
}
fn distance(begin: u32, end: u32, stride: u32) -> u32 {
    let bytes = end.wrapping_sub(begin) as i32;
    if stride == 8 {
        (bytes >> 3) as u32
    } else {
        (bytes / 12) as u32
    }
}
fn copy<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    destination: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
    for offset in (0..stride).step_by(4) {
        let value = b.read_word(at(source, offset))?;
        b.write_word(at(destination, offset), value)?;
    }
    Ok(())
}
fn placement<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    destination: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
    if destination != 0 {
        copy(b, destination, source, stride)?;
    }
    Ok(())
}
fn range<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut begin: u32,
    end: u32,
    mut destination: u32,
    stride: u32,
) -> Result<(), B::Error> {
    while begin != end {
        placement(b, destination, begin, stride)?;
        begin = at(begin, stride);
        destination = at(destination, stride);
    }
    Ok(())
}
fn fill<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut destination: u32,
    count: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
    for _ in 0..count {
        placement(b, destination, source, stride)?;
        destination = at(destination, stride);
    }
    Ok(())
}
fn size<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    stride: u32,
) -> Result<u32, B::Error> {
    let begin = b.read_word(at(header, 4))?;
    if begin == 0 {
        Ok(0)
    } else {
        Ok(distance(begin, b.read_word(at(header, 8))?, stride))
    }
}
fn insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    source: u32,
    stride: u32,
) -> Result<u32, B::Error> {
    let (begin, end, capacity) = if stride == 8 {
        let end = b.read_word(at(header, 8))?;
        let capacity = b.read_word(at(header, 12))?;
        let begin = b.read_word(at(header, 4))?;
        (begin, end, capacity)
    } else {
        let begin = b.read_word(at(header, 4))?;
        let end = b.read_word(at(header, 8))?;
        let capacity = b.read_word(at(header, 12))?;
        (begin, end, capacity)
    };
    let index = distance(begin, position, stride);
    if distance(end, capacity, stride) < 1 {
        let count = if begin == 0 {
            0
        } else {
            distance(begin, end, stride)
        };
        let grown = count.wrapping_add(count.max(1));
        let bytes = if (grown as i32) < 0 {
            0
        } else {
            grown.wrapping_mul(stride)
        };
        let allocation = b.allocate(bytes)?;
        let mut old = b.read_word(at(header, 4))?;
        let mut destination = allocation;
        while old != position {
            placement(b, destination, old, stride)?;
            old = at(old, stride);
            destination = at(destination, stride);
        }
        placement(b, destination, source, stride)?;
        let end = b.read_word(at(header, 8))?;
        range(b, position, end, at(destination, stride), stride)?;
        let _end = b.read_word(at(header, 8))?;
        let _begin = b.read_word(at(header, 4))?;
        let old = b.read_word(at(header, 4))?;
        b.free(old)?;
        b.write_word(at(header, 12), at(allocation, grown.wrapping_mul(stride)))?;
        let count = size(b, header, stride)?;
        let end = at(allocation, count.wrapping_add(1).wrapping_mul(stride));
        if stride == 8 {
            b.write_word(at(header, 8), end)?;
            b.write_word(at(header, 4), allocation)?;
            // Control's growth branch returns its retained allocation, unlike
            // the point helper and both in-place branches' final header read.
            return Ok(at(allocation, index.wrapping_mul(stride)));
        }
        b.write_word(at(header, 4), allocation)?;
        b.write_word(at(header, 8), end)?;
    } else {
        if distance(position, end, stride) < 1 {
            range(b, position, end, at(position, stride), stride)?;
            let end = b.read_word(at(header, 8))?;
            fill(
                b,
                end,
                1u32.wrapping_sub(distance(position, end, stride)),
                source,
                stride,
            )?;
            let end = b.read_word(at(header, 8))?;
            let mut destination = position;
            while destination != end {
                copy(b, destination, source, stride)?;
                destination = at(destination, stride);
            }
        } else {
            range(b, end.wrapping_sub(stride), end, end, stride)?;
            let mut destination = b.read_word(at(header, 8))?;
            let mut previous = destination.wrapping_sub(stride);
            while previous != position {
                previous = previous.wrapping_sub(stride);
                destination = destination.wrapping_sub(stride);
                copy(b, destination, previous, stride)?;
            }
            copy(b, position, source, stride)?;
        }
        let end = b.read_word(at(header, 8))?;
        b.write_word(at(header, 8), at(end, stride))?;
    }
    let begin = b.read_word(at(header, 4))?;
    Ok(at(begin, index.wrapping_mul(stride)))
}
/// Original 0x60efd0, (position,source), return address of the inserted point.
pub fn vehicle_special_points_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    source: u32,
) -> Result<u32, B::Error> {
    insert(b, header, position, source, 12)
}
/// Original 0x548040, (position,source), return address of the inserted control.
pub fn vehicle_special_controls_insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    source: u32,
) -> Result<u32, B::Error> {
    insert(b, header, position, source, 8)
}
/// Original EnumFrames collecting callback at0x4d0eb0. The header is live
/// addressable storage throughout allocation, copies, destruction and free.
pub fn vehicle_collect_frame<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    frame: u32,
) -> Result<u32, B::Error> {
    let insertion = b.read_word(at(header, 8))?;
    let capacity = b.read_word(at(header, 12))?;
    if ((capacity.wrapping_sub(insertion) as i32 >> 2) as u32) < 1 {
        let begin = b.read_word(at(header, 4))?;
        let size = if begin == 0 {
            0
        } else {
            (insertion.wrapping_sub(begin) as i32 >> 2) as u32
        };
        let count = size.wrapping_add(size.max(1));
        let bytes = if (count as i32) < 0 {
            0
        } else {
            count.wrapping_mul(4)
        };
        let allocation = b.allocate(bytes)?;
        let begin = b.read_word(at(header, 4))?;
        range(b, begin, insertion, allocation, 4)?;
        let destination = allocation.wrapping_add(insertion.wrapping_sub(begin));
        if destination != 0 {
            b.write_word(destination, frame)?;
        }
        let end = b.read_word(at(header, 8))?;
        range(b, insertion, end, at(destination, 4), 4)?;
        let _destroy_end = b.read_word(at(header, 8))?;
        let _destroy_begin = b.read_word(at(header, 4))?;
        let old = b.read_word(at(header, 4))?;
        b.free(old)?;
        let old_begin = b.read_word(at(header, 4))?;
        b.write_word(
            at(header, 12),
            allocation.wrapping_add(count.wrapping_mul(4)),
        )?;
        let size = if old_begin == 0 {
            0
        } else {
            let end = b.read_word(at(header, 8))?;
            (end.wrapping_sub(old_begin) as i32 >> 2) as u32
        };
        b.write_word(at(header, 4), allocation)?;
        b.write_word(
            at(header, 8),
            allocation
                .wrapping_add(size.wrapping_mul(4))
                .wrapping_add(4),
        )?;
    } else {
        // Saved insertion equals the initially loaded end. The initial range
        // copy is empty, then the native helper rereads the live end.
        let end = b.read_word(at(header, 8))?;
        let suffix = (end.wrapping_sub(insertion) as i32 >> 2) as u32;
        let count = 1u32.wrapping_sub(suffix);
        for index in 0..count {
            let destination = end.wrapping_add(index.wrapping_mul(4));
            if destination != 0 {
                b.write_word(destination, frame)?;
            }
        }
        let end = b.read_word(at(header, 8))?;
        let mut destination = insertion;
        while destination != end {
            b.write_word(destination, frame)?;
            destination = at(destination, 4);
        }
        let end = b.read_word(at(header, 8))?;
        b.write_word(at(header, 8), at(end, 4))?;
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Read(u32),
        Write(u32, u32),
        Allocate(u32),
        Free(u32),
    }
    struct Memory {
        words: BTreeMap<u32, u32>,
        events: Vec<Event>,
        before: Vec<BTreeMap<u32, u32>>,
        fail: Option<usize>,
        on_free: Option<(u32, u32)>,
    }
    impl Memory {
        fn event(&mut self, event: Event) -> Result<(), &'static str> {
            self.before.push(self.words.clone());
            self.events.push(event);
            if self.fail == Some(self.events.len() - 1) {
                Err("unavailable")
            } else {
                Ok(())
            }
        }
        fn seeded(stride: u32, size: u32, capacity: u32) -> Self {
            let mut words = BTreeMap::from([
                (
                    0x1004,
                    if size == 0 && capacity == 0 {
                        0
                    } else {
                        0x3000
                    },
                ),
                (
                    0x1008,
                    if size == 0 && capacity == 0 {
                        0
                    } else {
                        0x3000 + size * stride
                    },
                ),
                (
                    0x100c,
                    if capacity == 0 {
                        0
                    } else {
                        0x3000 + capacity * stride
                    },
                ),
            ]);
            for i in 0..capacity * stride / 4 {
                words.insert(0x3000 + i * 4, 0x100 + i);
            }
            for i in 0..stride / 4 {
                words.insert(0x2000 + i * 4, 0xabcd0000 + i);
            }
            Self {
                words,
                events: Vec::new(),
                before: Vec::new(),
                fail: None,
                on_free: None,
            }
        }
    }
    impl VehicleInitBackend for Memory {
        type Error = &'static str;
        fn read_byte(&mut self, _: u32) -> Result<u8, Self::Error> {
            unreachable!()
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            self.event(Event::Read(a))?;
            self.words.get(&a).copied().ok_or("address")
        }
        fn write_byte(&mut self, _: u32, _: u8) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.event(Event::Write(a, v))?;
            self.words.insert(a, v);
            Ok(())
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.event(Event::Allocate(n))?;
            Ok(0x8000)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.event(Event::Free(a))?;
            if let Some((a, v)) = self.on_free {
                self.words.insert(a, v);
            }
            Ok(())
        }
        fn collect_frame(
            &mut self,
            _: &mut crate::vehicle_init::VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: crate::vehicle_init::VehicleInitQuery,
            _: &mut crate::vehicle_init::VehicleInitEnumeration<'_>,
        ) -> Result<(), crate::vehicle_init::VehicleInitUnavailable<Self::Error>> {
            unreachable!()
        }
    }
    #[test]
    fn inserts_all_record_words_in_growth_and_both_in_place_branches() {
        for stride in [8, 12] {
            for size in [0, 1, 4] {
                for capacity in [size, size + 2] {
                    for index in 0..=size {
                        let mut b = Memory::seeded(stride, size, capacity);
                        let begin = b.words[&0x1004];
                        let position = begin + index * stride;
                        let old = (0..size)
                            .map(|i| {
                                (0..stride / 4)
                                    .map(|w| b.words[&(begin + i * stride + w * 4)])
                                    .collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>();
                        let source = (0..stride / 4)
                            .map(|w| b.words[&(0x2000 + w * 4)])
                            .collect::<Vec<_>>();
                        let mut expected = old;
                        expected.insert(index as usize, source);
                        let result = insert(&mut b, 0x1000, position, 0x2000, stride).unwrap();
                        let begin = b.words[&0x1004];
                        assert_eq!(result, begin + index * stride);
                        assert_eq!(b.words[&0x1008], begin + (size + 1) * stride);
                        for (i, record) in expected.iter().enumerate() {
                            for (w, value) in record.iter().enumerate() {
                                assert_eq!(
                                    b.words[&(begin + i as u32 * stride + w as u32 * 4)],
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
    fn each_memory_failure_preserves_exact_completed_prefix() {
        for stride in [8, 12] {
            for (size, capacity, index) in [(0, 0, 0), (3, 3, 1), (3, 5, 0), (3, 5, 3)] {
                let mut full = Memory::seeded(stride, size, capacity);
                let position = full.words[&0x1004] + index * stride;
                insert(&mut full, 0x1000, position, 0x2000, stride).unwrap();
                for failure in 0..full.events.len() {
                    let mut b = Memory::seeded(stride, size, capacity);
                    b.fail = Some(failure);
                    assert_eq!(
                        insert(&mut b, 0x1000, position, 0x2000, stride),
                        Err("unavailable")
                    );
                    assert_eq!(b.events, full.events[..=failure]);
                    assert_eq!(b.words, full.before[failure]);
                }
            }
        }
    }
    #[test]
    fn free_side_header_change_is_reread_after_capacity_store() {
        for stride in [8, 12] {
            let mut b = Memory::seeded(stride, 1, 1);
            b.on_free = Some((0x1008, 0x3000 + 3 * stride));
            insert(&mut b, 0x1000, 0x3000 + stride, 0x2000, stride).unwrap();
            assert_eq!(b.words[&0x1008], 0x8000 + 4 * stride);
            assert_eq!(b.words[&0x100c], 0x8000 + 2 * stride);
            let tail = &b.events[b
                .events
                .iter()
                .position(|e| matches!(e, Event::Free(..)))
                .unwrap()
                + 1..];
            assert_eq!(tail[0], Event::Write(0x100c, 0x8000 + 2 * stride));
            assert_eq!(tail[1], Event::Read(0x1004));
            assert_eq!(tail[2], Event::Read(0x1008));
            assert_eq!(
                tail[3],
                Event::Write(
                    if stride == 8 { 0x1008 } else { 0x1004 },
                    if stride == 8 {
                        0x8000 + 4 * stride
                    } else {
                        0x8000
                    }
                )
            );
        }
    }
}
