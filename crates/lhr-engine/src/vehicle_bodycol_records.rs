//! BODYCOL native record storage. Names in this module are descriptive.
//!
//! All addresses are inert PC identities. Headers occupy 16 bytes: a byte
//! marker, three padding bytes, and begin/end/capacity words. Record copies
//! allocate their embedded lists; frame words are copied without retaining a
//! reference. Destruction frees lists without releasing frames. No Rust Drop
//! implementation substitutes for the explicit native operations.
//!
//! Backend errors stop at the completed memory/effect prefix. In particular,
//! a null allocation is passed through the original placement-copy rules; it
//! is neither an allocation exception nor a successful empty collection.

use crate::vehicle_init::VehicleInitBackend;

pub const BODYCOL_RECORD_BYTES: u32 = 116;
pub const BODYCOL_DESCENDANT_BYTES: u32 = 60;

fn add(address: u32, offset: u32) -> u32 {
    address.wrapping_add(offset)
}

// Native pointer differences are signed i32 before the strength-reduced
// division. Keeping the signed division also preserves truncation toward zero.
fn distance(begin: u32, end: u32, stride: u32) -> u32 {
    ((end.wrapping_sub(begin) as i32) / stride as i32) as u32
}

fn size<B: VehicleInitBackend + ?Sized>(b: &mut B, h: u32, stride: u32) -> Result<u32, B::Error> {
    let begin = b.read_word(add(h, 4))?;
    if begin == 0 {
        Ok(0)
    } else {
        let end = b.read_word(add(h, 8))?;
        Ok(distance(begin, end, stride))
    }
}

fn allocation_bytes(count: u32, stride: u32) -> u32 {
    if (count as i32) < 0 {
        0
    } else {
        count.wrapping_mul(stride)
    }
}

fn words<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
    n: u32,
) -> Result<(), B::Error> {
    for i in 0..n {
        let word = b.read_word(add(src, i * 4))?;
        b.write_word(add(dst, i * 4), word)?;
    }
    Ok(())
}

/// Original 0x505370: forward placement copies, skipping only destination zero.
pub fn copy_bodycol_descendants<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut dst: u32,
    mut begin: u32,
    end: u32,
) -> Result<u32, B::Error> {
    while begin != end {
        if dst != 0 {
            words(b, dst, begin, 15)?;
        }
        begin = add(begin, 60);
        dst = add(dst, 60);
    }
    Ok(dst)
}

/// Original 0x4d29e0: repeated placement copies of a 60-byte record.
/// Its zero-count path returns zero rather than the destination address.
pub fn fill_bodycol_descendants<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut dst: u32,
    count: u32,
    source: u32,
) -> Result<u32, B::Error> {
    for _ in 0..count {
        if dst != 0 {
            words(b, dst, source, 15)?;
        }
        dst = add(dst, 60);
    }
    Ok(if count == 0 { 0 } else { dst })
}

fn copy_scalar_fields<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
) -> Result<(), B::Error> {
    words(b, add(dst, 0x20), add(src, 0x20), 14)?;
    let flag = b.read_byte(add(src, 0x58))?;
    b.write_byte(add(dst, 0x58), flag)?;
    words(b, add(dst, 0x5c), add(src, 0x5c), 6)
}

/// Original 0x4d2b30 placement copy constructor. Padding remains untouched.
/// Even empty embedded lists invoke the original allocator with zero bytes.
pub fn copy_bodycol_record<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
) -> Result<(), B::Error> {
    if dst == 0 {
        return Ok(());
    }
    for (offset, stride) in [(0, 60), (0x10, 116)] {
        let sh = add(src, offset);
        let dh = add(dst, offset);
        let marker = b.read_byte(sh)?;
        b.write_byte(dh, marker)?;
        let count = size(b, sh, stride)?;
        let allocation = b.allocate(allocation_bytes(count, stride))?;
        b.write_word(add(dh, 4), allocation)?;
        let end = b.read_word(add(sh, 8))?;
        let mut begin = b.read_word(add(sh, 4))?;
        let mut cursor = allocation;
        while begin != end {
            if stride == 60 {
                if cursor != 0 {
                    words(b, cursor, begin, 15)?;
                }
            } else {
                copy_bodycol_record(b, cursor, begin)?;
            }
            begin = add(begin, stride);
            cursor = add(cursor, stride);
        }
        b.write_word(add(dh, 8), cursor)?;
        b.write_word(add(dh, 12), cursor)?;
    }
    copy_scalar_fields(b, dst, src)
}

/// Original 0x4d1410: children in ascending order, nested allocation, then
/// descendant allocation. Header zeroing occurs after each completed free.
pub fn destroy_bodycol_record<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    record: u32,
) -> Result<(), B::Error> {
    let end = b.read_word(add(record, 0x18))?;
    let begin = b.read_word(add(record, 0x14))?;
    destroy_bodycol_temporaries(b, begin, end)?;
    let allocation = b.read_word(add(record, 0x14))?;
    b.free(allocation)?;
    for offset in [0x14, 0x18, 0x1c] {
        b.write_word(add(record, offset), 0)?;
    }
    let allocation = b.read_word(add(record, 4))?;
    b.free(allocation)?;
    for offset in [4, 8, 12] {
        b.write_word(add(record, offset), 0)?;
    }
    Ok(())
}

/// Original 0x4d2f50, including its scalar-deleting flag bit.
pub fn delete_bodycol_record<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    record: u32,
    flags: u32,
) -> Result<u32, B::Error> {
    destroy_bodycol_record(b, record)?;
    if flags & 1 != 0 {
        b.free(record)?;
    }
    Ok(record)
}

/// Calls the nondeleting destructor on each element; the outer allocation is
/// deliberately left to the caller's subsequent native free operation.
pub fn destroy_bodycol_temporaries<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    mut begin: u32,
    end: u32,
) -> Result<(), B::Error> {
    while begin != end {
        destroy_bodycol_record(b, begin)?;
        begin = add(begin, 116);
    }
    Ok(())
}

// Assignment of the leading 60-byte collection: 0x4d2f70. Assignment leaves
// the header marker and padding untouched. The native replacement path frees
// the old allocation before asking for the replacement.
fn assign_descendants<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
    inline: bool,
) -> Result<(), B::Error> {
    if dst == src {
        return Ok(());
    }
    let source_begin = b.read_word(add(src, 4))?;
    let source_count = size(b, src, 60)?;
    let target_begin = b.read_word(add(dst, 4))?;
    let target_count = size(b, dst, 60)?;
    if source_count <= target_count {
        let end = b.read_word(add(src, 8))?;
        let mut s = source_begin;
        let mut d = target_begin;
        while s != end {
            words(b, d, s, 15)?;
            s = add(s, 60);
            d = add(d, 60);
        }
        let count = size(b, src, 60)?;
        let base = b.read_word(add(dst, 4))?;
        return b.write_word(add(dst, 8), add(base, count.wrapping_mul(60)));
    }
    let capacity = if target_begin == 0 {
        0
    } else {
        let cap = b.read_word(add(dst, 12))?;
        distance(target_begin, cap, 60)
    };
    if source_count <= capacity {
        let middle = add(source_begin, target_count.wrapping_mul(60));
        let mut s = source_begin;
        let mut d = target_begin;
        while s != middle {
            words(b, d, s, 15)?;
            s = add(s, 60);
            d = add(d, 60);
        }
        let end = b.read_word(add(src, 8))?;
        let dst_end = b.read_word(add(dst, 8))?;
        copy_bodycol_descendants(b, dst_end, middle, end)?;
        let count = size(b, src, 60)?;
        let base = b.read_word(add(dst, 4))?;
        b.write_word(add(dst, 8), add(base, count.wrapping_mul(60)))
    } else {
        b.free(target_begin)?;
        let count = size(b, src, 60)?;
        let allocation = b.allocate(allocation_bytes(count, 60))?;
        b.write_word(add(dst, 4), allocation)?;
        let end = b.read_word(add(src, 8))?;
        let begin = b.read_word(add(src, 4))?;
        let cursor = copy_bodycol_descendants(b, allocation, begin, end)?;
        if inline {
            b.write_word(add(dst, 12), cursor)?;
            b.write_word(add(dst, 8), cursor)
        } else {
            b.write_word(add(dst, 8), cursor)?;
            b.write_word(add(dst, 12), cursor)
        }
    }
}

fn assign_records<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
) -> Result<(), B::Error> {
    if dst == src {
        return Ok(());
    }
    let source_begin = b.read_word(add(src, 4))?;
    let source_count = size(b, src, 116)?;
    let target_begin = b.read_word(add(dst, 4))?;
    let target_count = size(b, dst, 116)?;
    if source_count <= target_count {
        let end = b.read_word(add(src, 8))?;
        let mut s = source_begin;
        let mut d = target_begin;
        while s != end {
            assign_bodycol_record(b, d, s)?;
            s = add(s, 116);
            d = add(d, 116);
        }
        let old_end = b.read_word(add(dst, 8))?;
        destroy_bodycol_temporaries(b, d, old_end)?;
    } else {
        let capacity = if target_begin == 0 {
            0
        } else {
            let cap = b.read_word(add(dst, 12))?;
            distance(target_begin, cap, 116)
        };
        if source_count <= capacity {
            let middle = add(source_begin, target_count.wrapping_mul(116));
            let mut s = source_begin;
            let mut d = target_begin;
            while s != middle {
                assign_bodycol_record(b, d, s)?;
                s = add(s, 116);
                d = add(d, 116);
            }
            let end = b.read_word(add(src, 8))?;
            let mut cursor = b.read_word(add(dst, 8))?;
            while s != end {
                copy_bodycol_record(b, cursor, s)?;
                s = add(s, 116);
                cursor = add(cursor, 116);
            }
        } else {
            let end = b.read_word(add(dst, 8))?;
            destroy_bodycol_temporaries(b, target_begin, end)?;
            let allocation = b.read_word(add(dst, 4))?;
            b.free(allocation)?;
            let count = size(b, src, 116)?;
            let allocation = b.allocate(allocation_bytes(count, 116))?;
            b.write_word(add(dst, 4), allocation)?;
            let end = b.read_word(add(src, 8))?;
            let mut s = b.read_word(add(src, 4))?;
            let mut cursor = allocation;
            while s != end {
                copy_bodycol_record(b, cursor, s)?;
                s = add(s, 116);
                cursor = add(cursor, 116);
            }
            b.write_word(add(dst, 8), cursor)?;
            return b.write_word(add(dst, 12), cursor);
        }
    }
    let count = size(b, src, 116)?;
    let base = b.read_word(add(dst, 4))?;
    b.write_word(add(dst, 8), add(base, count.wrapping_mul(116)))
}

/// Original 0x4d2d10 assignment, including its asymmetric self-assignment path.
pub fn assign_bodycol_record<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
) -> Result<(), B::Error> {
    // The separately emitted 60-byte assignment has the same list effects as
    // 0x4d2f70, including the placement-copy null rule on construction paths.
    assign_descendants(b, dst, src, true)?;
    assign_records(b, add(dst, 0x10), add(src, 0x10))?;
    copy_scalar_fields(b, dst, src)
}

fn construct<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
    stride: u32,
) -> Result<(), B::Error> {
    if stride == 116 {
        copy_bodycol_record(b, dst, src)
    } else if dst == 0 {
        Ok(())
    } else {
        words(b, dst, src, 15)
    }
}

fn assign<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    dst: u32,
    src: u32,
    stride: u32,
) -> Result<(), B::Error> {
    if stride == 116 {
        assign_bodycol_record(b, dst, src)
    } else {
        words(b, dst, src, 15)
    }
}

// 116-byte insertion is 0x4d2050; the 60-byte variant is inline in 0x4d1520.
fn insert<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    count: u32,
    source: u32,
    stride: u32,
) -> Result<(), B::Error> {
    let old_end = b.read_word(add(header, 8))?;
    let cap = b.read_word(add(header, 12))?;
    if distance(old_end, cap, stride) < count {
        let begin = b.read_word(add(header, 4))?;
        let len = if begin == 0 {
            0
        } else {
            distance(begin, old_end, stride)
        };
        let new_count = len.wrapping_add(len.max(count));
        let allocation = b.allocate(allocation_bytes(new_count, stride))?;
        let mut cursor = allocation;
        let mut s = b.read_word(add(header, 4))?;
        while s != position {
            construct(b, cursor, s, stride)?;
            s = add(s, stride);
            cursor = add(cursor, stride);
        }
        for _ in 0..count {
            construct(b, cursor, source, stride)?;
            cursor = add(cursor, stride);
        }
        let end = b.read_word(add(header, 8))?;
        let mut s = position;
        while s != end {
            construct(b, cursor, s, stride)?;
            s = add(s, stride);
            cursor = add(cursor, stride);
        }
        if stride == 116 {
            let end = b.read_word(add(header, 8))?;
            let begin = b.read_word(add(header, 4))?;
            destroy_bodycol_temporaries(b, begin, end)?;
        }
        let allocation_to_free = b.read_word(add(header, 4))?;
        b.free(allocation_to_free)?;
        b.write_word(
            add(header, 12),
            add(allocation, new_count.wrapping_mul(stride)),
        )?;
        // Original computes the old size after free and before replacing begin.
        let len = size(b, header, stride)?;
        b.write_word(add(header, 4), allocation)?;
        b.write_word(
            add(header, 8),
            add(allocation, len.wrapping_add(count).wrapping_mul(stride)),
        )
    } else {
        let tail = distance(position, old_end, stride);
        let bytes = count.wrapping_mul(stride);
        if tail < count {
            let end = b.read_word(add(header, 8))?;
            let mut s = position;
            while s != end {
                construct(b, add(s, bytes), s, stride)?;
                s = add(s, stride);
            }
            let end = b.read_word(add(header, 8))?;
            let missing = count.wrapping_sub(distance(position, end, stride));
            let mut cursor = end;
            for _ in 0..missing {
                construct(b, cursor, source, stride)?;
                cursor = add(cursor, stride);
            }
            let end = b.read_word(add(header, 8))?;
            let mut cursor = position;
            while cursor != end {
                assign(b, cursor, source, stride)?;
                cursor = add(cursor, stride);
            }
        } else if count != 0 {
            let mut s = old_end.wrapping_sub(bytes);
            let mut d = old_end;
            while s != old_end {
                construct(b, d, s, stride)?;
                s = add(s, stride);
                d = add(d, stride);
            }
            let mut d = b.read_word(add(header, 8))?;
            let mut s = d.wrapping_sub(bytes);
            while s != position {
                s = s.wrapping_sub(stride);
                d = d.wrapping_sub(stride);
                if stride == 116 {
                    assign_descendants(b, d, s, false)?;
                    assign_records(b, add(d, 0x10), add(s, 0x10))?;
                    copy_scalar_fields(b, d, s)?;
                } else {
                    words(b, d, s, 15)?;
                }
            }
            let mut d = position;
            let end = add(position, bytes);
            while d != end {
                assign(b, d, source, stride)?;
                d = add(d, stride);
            }
        }
        if count != 0 || tail < count {
            let end = b.read_word(add(header, 8))?;
            b.write_word(add(header, 8), add(end, bytes))?;
        }
        Ok(())
    }
}

pub fn insert_bodycol_records<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    position: u32,
    count: u32,
    source: u32,
) -> Result<(), B::Error> {
    insert(b, header, position, count, source, 116)
}

pub fn append_bodycol_record<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    source: u32,
) -> Result<(), B::Error> {
    let end = b.read_word(add(header, 8))?;
    insert_bodycol_records(b, header, end, 1, source)
}

pub fn append_bodycol_descendant<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    source: u32,
) -> Result<(), B::Error> {
    let end = b.read_word(add(header, 8))?;
    insert(b, header, end, 1, source, 60)
}

/// Original 0x4d1360: backward depth-first search, append to the first record
/// whose frame is the incoming frame's parent. The parent is reread at each
/// recursive entry as in the native routine.
pub fn group_bodycol_record<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    source: u32,
) -> Result<bool, B::Error> {
    let frame = b.read_word(add(source, 0x20))?;
    let parent = b.read_word(add(frame, 0x120))?;
    let mut count = size(b, header, 116)?;
    let mut offset = count.wrapping_mul(116);
    while count != 0 {
        let begin = b.read_word(add(header, 4))?;
        offset = offset.wrapping_sub(116);
        count = count.wrapping_sub(1);
        let record = add(begin, offset);
        let existing_frame = b.read_word(add(record, 0x20))?;
        if existing_frame == parent {
            let begin = b.read_word(add(header, 4))?;
            let nested = add(add(begin, count.wrapping_mul(116)), 0x10);
            let end = b.read_word(add(nested, 8))?;
            insert_bodycol_records(b, nested, end, 1, source)?;
            return Ok(true);
        }
        if group_bodycol_record(b, add(record, 0x10), source)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Memory {
        bytes: BTreeMap<u32, u8>,
        cursor: u32,
        effects: Vec<(u32, u32)>,
        fail_allocate: Option<usize>,
        null_allocate: Option<usize>,
        allocations: usize,
    }
    impl Memory {
        fn reserve(&mut self, address: u32, size: u32, byte: u8) {
            for offset in 0..size {
                self.bytes.insert(address + offset, byte);
            }
        }
        fn word(&mut self, address: u32, word: u32) {
            self.write_word(address, word).unwrap();
        }
        fn header(&mut self, address: u32, begin: u32, len: u32, capacity: u32, stride: u32) {
            self.reserve(address, 16, 0);
            self.word(address + 4, begin);
            self.word(address + 8, begin + len * stride);
            self.word(address + 12, begin + capacity * stride);
        }
        fn record(&mut self, address: u32, frame: u32) {
            self.reserve(address, 116, 0);
            self.word(address + 0x20, frame);
        }
        fn get(&mut self, address: u32) -> u32 {
            self.read_word(address).unwrap()
        }
    }
    impl VehicleInitBackend for Memory {
        type Error = u32;
        fn read_byte(&mut self, a: u32) -> Result<u8, u32> {
            self.bytes.get(&a).copied().ok_or(a)
        }
        fn read_word(&mut self, a: u32) -> Result<u32, u32> {
            let mut bytes = [0; 4];
            for (i, byte) in bytes.iter_mut().enumerate() {
                *byte = self.read_byte(a + i as u32)?;
            }
            Ok(u32::from_le_bytes(bytes))
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), u32> {
            let byte = self.bytes.get_mut(&a).ok_or(a)?;
            *byte = v;
            Ok(())
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), u32> {
            for (i, byte) in v.to_le_bytes().iter().enumerate() {
                self.write_byte(a + i as u32, *byte)?;
            }
            Ok(())
        }
        fn allocate(&mut self, size: u32) -> Result<u32, u32> {
            self.allocations += 1;
            if self.fail_allocate == Some(self.allocations) {
                self.effects.push((1, size));
                return Err(0xffff_fffe);
            }
            if self.null_allocate == Some(self.allocations) {
                self.effects.push((1, size));
                return Ok(0);
            }
            if self.cursor == 0 {
                self.cursor = 0x10000;
            }
            let address = self.cursor;
            self.cursor += size.div_ceil(16) * 16 + 32;
            self.reserve(address, size, 0xa5);
            self.effects.push((1, size));
            Ok(address)
        }
        fn free(&mut self, address: u32) -> Result<(), u32> {
            self.effects.push((2, address));
            Ok(())
        }
        fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), u32> {
            unreachable!()
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<u32>> {
            unreachable!()
        }
    }

    #[test]
    fn copied_lists_own_distinct_allocations_and_destroy_nested_first() {
        let mut b = Memory::default();
        b.record(0x1000, 0x7000);
        b.header(0x1000, 0x2000, 1, 3, 60);
        b.reserve(0x2000, 60, 0x6a);
        b.header(0x1010, 0x3000, 1, 2, 116);
        b.record(0x3000, 0x8000);
        b.reserve(0x4000, 116, 0xcc);
        copy_bodycol_record(&mut b, 0x4000, 0x1000).unwrap();
        let descendants = b.get(0x4004);
        let children = b.get(0x4014);
        assert_ne!(descendants, 0x2000);
        assert_ne!(children, 0x3000);
        assert_eq!(b.get(descendants), 0x6a6a6a6a);
        assert_eq!(b.get(children + 0x20), 0x8000);
        assert_eq!(b.read_byte(0x4001), Ok(0xcc));
        assert_eq!(b.read_byte(0x4059), Ok(0xcc));
        assert_eq!(b.effects, [(1, 60), (1, 116), (1, 0), (1, 0)]);
        let child_descendants = b.get(children + 4);
        let child_children = b.get(children + 0x14);
        b.effects.clear();
        destroy_bodycol_record(&mut b, 0x4000).unwrap();
        assert_eq!(
            b.effects,
            [
                (2, child_children),
                (2, child_descendants),
                (2, children),
                (2, descendants)
            ]
        );
        assert_eq!(b.get(0x4004), 0);
        assert_eq!(b.get(0x4014), 0);
        assert_eq!(b.get(0x4020), 0x7000);
        assert_eq!(b.get(0x1004), 0x2000);
    }

    #[test]
    fn grouping_searches_last_record_first_and_copies_incoming_record() {
        let mut b = Memory::default();
        b.header(0x1000, 0x2000, 2, 2, 116);
        b.record(0x2000, 0x5000);
        b.record(0x2074, 0x5000);
        b.record(0x3000, 0x6000);
        b.reserve(0x6120, 4, 0);
        b.word(0x6120, 0x5000);
        assert_eq!(group_bodycol_record(&mut b, 0x1000, 0x3000), Ok(true));
        assert_eq!(b.get(0x2014), 0);
        let inserted = b.get(0x2088);
        assert_ne!(inserted, 0);
        assert_eq!(b.get(inserted + 0x20), 0x6000);
        assert_eq!(b.get(0x3004), 0);
    }

    #[test]
    fn insertion_copies_before_destroying_old_nested_ownership() {
        let mut b = Memory::default();
        b.header(0x1000, 0x2000, 1, 1, 116);
        b.record(0x2000, 0x5000);
        b.record(0x3000, 0x6000);
        append_bodycol_record(&mut b, 0x1000, 0x3000).unwrap();
        assert_eq!(
            b.effects,
            [
                (1, 232),
                (1, 0),
                (1, 0),
                (1, 0),
                (1, 0),
                (2, 0),
                (2, 0),
                (2, 0x2000)
            ]
        );
        let begin = b.get(0x1004);
        assert_eq!(b.get(begin + 0x20), 0x5000);
        assert_eq!(b.get(begin + 116 + 0x20), 0x6000);
        assert_eq!(b.get(0x1008), begin + 232);
        assert_eq!(b.get(0x100c), begin + 232);
    }

    #[test]
    fn effect_failure_retains_completed_prefix_and_does_not_free_old_storage() {
        let mut b = Memory::default();
        b.header(0x1000, 0x2000, 1, 1, 116);
        b.record(0x2000, 0x5000);
        b.record(0x3000, 0x6000);
        b.fail_allocate = Some(3);
        assert_eq!(
            append_bodycol_record(&mut b, 0x1000, 0x3000),
            Err(0xffff_fffe)
        );
        assert_eq!(b.effects, [(1, 232), (1, 0), (1, 0)]);
        assert_eq!(b.get(0x1004), 0x2000);
        assert_eq!(b.get(0x1008), 0x2074);
        assert_eq!(b.get(0x100c), 0x2074);
        assert_ne!(b.get(0x10004), 0);
    }

    #[test]
    fn in_place_middle_insertion_deep_assigns_and_preserves_markers() {
        let mut b = Memory::default();
        b.header(0x1000, 0x2000, 2, 4, 116);
        for (a, frame) in [(0x2000, 0x5000), (0x2074, 0x6000), (0x3000, 0x7000)] {
            b.record(a, frame);
        }
        b.reserve(0x20e8, 232, 0xcc);
        b.write_byte(0x2074, 0x42).unwrap();
        b.write_byte(0x3000, 0x61).unwrap();
        insert_bodycol_records(&mut b, 0x1000, 0x2000, 1, 0x3000).unwrap();
        assert_eq!(b.get(0x2020), 0x7000);
        assert_eq!(b.get(0x2094), 0x5000);
        assert_eq!(b.get(0x2108), 0x6000);
        assert_eq!(b.read_byte(0x2074), Ok(0x42));
        assert_eq!(b.read_byte(0x20e8), Ok(0x42));
        assert_eq!(b.get(0x1008), 0x215c);
    }
    #[test]
    fn null_placement_destination_skips_only_the_first_copy() {
        let mut b = Memory::default();
        b.header(0x1000, 0x2000, 1, 1, 116);
        b.record(0x2000, 0x5000);
        b.record(0x3000, 0x6000);
        b.null_allocate = Some(1);
        assert_eq!(append_bodycol_record(&mut b, 0x1000, 0x3000), Err(116));
        assert_eq!(b.effects, [(1, 232)]);
        assert_eq!(b.get(0x1004), 0x2000);
        assert_eq!(b.get(0x1008), 0x2074);
        assert_eq!(b.get(0x2014), 0);
    }

    #[test]
    fn zero_fill_returns_zero_without_accessing_source_or_destination() {
        let mut b = Memory::default();
        assert_eq!(
            fill_bodycol_descendants(&mut b, 0x12345678, 0, 0xffffffff),
            Ok(0)
        );
        assert!(b.bytes.is_empty());
        assert!(b.effects.is_empty());
    }
}
