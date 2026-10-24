//! Addressable projection of Game's BODYCOL callbacks and recursive passes.
//!
//! Descriptive names map to PC entries 0x4d1060, 0x4d1490, 0x4d1520 and
//! 0x4d17f0. This does not complete vehicle Init. Records retain raw frame
//! identities; constructing Rust values acquires no native references.
use crate::vehicle_bodycol_records::{
    append_bodycol_descendant, append_bodycol_record, destroy_bodycol_temporaries,
    group_bodycol_record,
};
use crate::{
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::SMatrix,
    vehicle_init::{VehicleBodyCollisionContext, VehicleComponentTailMemory, VehicleInitBackend},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleBodycolUnavailable<E> {
    Effect(E),
    NumericDomain(&'static str),
    ProjectionStorage,
}

/// Stack storage, separate from the original native heap. The two bytes are
/// opaque caller stack contents copied by constructors before the memset;
/// callers must not infer initialized zero values for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleBodycolScratch {
    pub record: u32,
    pub leading_byte: u8,
    pub nested_byte: u8,
}

/// Effects still belonging to native frame implementations. Enumeration must
/// preserve the supplied pattern, mask, traversal and callback-return behavior.
/// Each method retains its completed effects when returning unavailability.
pub trait VehicleBodycolBackend: VehicleInitBackend {
    fn bodycol_update_world(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn bodycol_link_frame(
        &mut self,
        frame: u32,
        parent: u32,
        flags: u32,
    ) -> Result<(), Self::Error>;
    fn bodycol_remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error>;
    fn bodycol_enumerate(
        &mut self,
        frame: u32,
        pattern: &[u8],
        mask: u32,
        target: &mut VehicleBodycolEnumeration,
    ) -> Result<(), VehicleBodycolUnavailable<Self::Error>>;
}

/// Explicit disjoint address mapping used by the opt-in Init adapter. Scratch
/// spans must already be addressable stack storage in the backend; this does
/// not call the original heap allocator or initialize opaque stack bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleBodycolStorage {
    pub vehicle_address: u32,
    pub callback_scratch: VehicleBodycolScratch,
    pub descendant_scratch: u32,
}

pub struct VehicleBodycolEnumeration<'a> {
    target: BodycolEnumerationTarget<'a>,
}
enum BodycolEnumerationTarget<'a> {
    ProjectedComponents {
        memory: VehicleComponentTailMemory<'a>,
        storage: VehicleBodycolStorage,
        context: VehicleBodyCollisionContext,
    },
    ProjectedDescendants {
        memory: VehicleComponentTailMemory<'a>,
        parent_record: u32,
        name_length_with_nul: u32,
        temporary: u32,
    },
    Descendants {
        parent_record: u32,
        name_length_with_nul: u32,
        temporary: u32,
    },
    Components {
        vehicle: &'a mut [u8],
        storage: VehicleBodycolStorage,
        context: VehicleBodyCollisionContext,
    },
}
impl<'a> VehicleBodycolEnumeration<'a> {
    pub(crate) fn components_projected(
        memory: VehicleComponentTailMemory<'a>,
        storage: VehicleBodycolStorage,
        context: VehicleBodyCollisionContext,
    ) -> Self {
        Self {
            target: BodycolEnumerationTarget::ProjectedComponents {
                memory,
                storage,
                context,
            },
        }
    }
    pub(crate) fn components(
        vehicle: &'a mut [u8],
        storage: VehicleBodycolStorage,
        context: VehicleBodyCollisionContext,
    ) -> Self {
        Self {
            target: BodycolEnumerationTarget::Components {
                vehicle,
                storage,
                context,
            },
        }
    }
    pub fn visit<B: VehicleBodycolBackend + ?Sized>(
        &mut self,
        frame: u32,
        backend: &mut B,
    ) -> Result<u32, VehicleBodycolUnavailable<B::Error>> {
        match &mut self.target {
            BodycolEnumerationTarget::ProjectedComponents {
                memory,
                storage,
                context,
            } => {
                let mut projection =
                    VehicleBodycolProjection::projected(backend, memory.reborrow());
                bodycol_callback(
                    &mut projection,
                    storage.vehicle_address.wrapping_add(0x4ac),
                    *context,
                    frame,
                    storage.callback_scratch,
                )
            }
            BodycolEnumerationTarget::ProjectedDescendants {
                memory,
                parent_record,
                name_length_with_nul,
                temporary,
            } => {
                let mut projection =
                    VehicleBodycolProjection::projected(backend, memory.reborrow());
                bodycol_descendant_callback(
                    &mut projection,
                    *parent_record,
                    *name_length_with_nul,
                    frame,
                    *temporary,
                )
            }
            BodycolEnumerationTarget::Descendants {
                parent_record,
                name_length_with_nul,
                temporary,
            } => bodycol_descendant_callback(
                backend,
                *parent_record,
                *name_length_with_nul,
                frame,
                *temporary,
            ),
            BodycolEnumerationTarget::Components {
                vehicle,
                storage,
                context,
            } => {
                let mut projection = VehicleBodycolProjection::new(backend, vehicle, *storage)?;
                bodycol_callback(
                    &mut projection,
                    storage.vehicle_address.wrapping_add(0x4ac),
                    *context,
                    frame,
                    storage.callback_scratch,
                )
            }
        }
    }
}

type ResultFor<B, T> = Result<T, VehicleBodycolUnavailable<<B as VehicleInitBackend>::Error>>;
fn read<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> ResultFor<B, u32> {
    b.read_word(a).map_err(VehicleBodycolUnavailable::Effect)
}
fn byte<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> ResultFor<B, u8> {
    b.read_byte(a).map_err(VehicleBodycolUnavailable::Effect)
}
fn write<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u32) -> ResultFor<B, ()> {
    b.write_word(a, v)
        .map_err(VehicleBodycolUnavailable::Effect)
}
fn put_byte<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u8) -> ResultFor<B, ()> {
    b.write_byte(a, v)
        .map_err(VehicleBodycolUnavailable::Effect)
}
fn zero_words<B: VehicleInitBackend + ?Sized>(b: &mut B, dst: u32, count: u32) -> ResultFor<B, ()> {
    for i in 0..count {
        write(b, dst.wrapping_add(i * 4), 0)?;
    }
    Ok(())
}
fn number<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> ResultFor<B, f32> {
    read(b, a).map(f32::from_bits)
}
fn multiply<E>(a: f32, b: f32) -> Result<f32, VehicleBodycolUnavailable<E>> {
    checked_f32_mul(a, b)
        .map_err(|_| VehicleBodycolUnavailable::NumericDomain("BODYCOL multiplication"))
}
fn add<E>(a: f32, b: f32) -> Result<f32, VehicleBodycolUnavailable<E>> {
    checked_f32_add(a, b).map_err(|_| VehicleBodycolUnavailable::NumericDomain("BODYCOL addition"))
}
fn radius<E>(bounds: [u32; 6]) -> Result<f32, VehicleBodycolUnavailable<E>> {
    let y_max = f32::from_bits(bounds[4]);
    let y_min = f32::from_bits(bounds[1]);
    // FSUB, FABS, true double 0.5, then a binary32 spill before scale.
    let difference = add(y_max, -y_min)?;
    multiply(difference.abs(), 0.5)
}
fn bounds<B: VehicleInitBackend + ?Sized>(b: &mut B, frame: u32) -> ResultFor<B, [u32; 6]> {
    let mut out = [0; 6];
    for (i, value) in out.iter_mut().enumerate() {
        *value = read(b, frame.wrapping_add(0x140 + i as u32 * 4))?;
    }
    Ok(out)
}
fn name<B: VehicleInitBackend + ?Sized>(b: &mut B, frame: u32) -> ResultFor<B, u32> {
    let value = read(b, frame.wrapping_add(0x100))?;
    Ok(if value == 0 { 0x63d2a0 } else { value })
}
fn scale<B: VehicleBodycolBackend + ?Sized>(
    b: &mut B,
    frame: u32,
    parent: u32,
) -> ResultFor<B, [u32; 3]> {
    if parent == 0 {
        let x = read(b, frame.wrapping_add(0x90))?;
        let y = read(b, frame.wrapping_add(0x94))?;
        let z = read(b, frame.wrapping_add(0x98))?;
        return Ok([x, y, z]);
    }
    if byte(b, frame.wrapping_add(0xac))? & 0x20 == 0 {
        b.bodycol_update_world(frame)
            .map_err(VehicleBodycolUnavailable::Effect)?;
    }
    // Native GetScale reads each row Z,Y,X, with its components loaded
    // z,y,x before evaluating x*x+y*y+z*z. Check one row before reading
    // the next so an unsupported numeric row preserves its read prefix.
    let mut out = [0; 3];
    for row in [2, 1, 0] {
        let mut matrix = SMatrix { elements: [0.; 16] };
        for column in [2, 1, 0] {
            matrix.elements[row * 4 + column] =
                number(b, frame.wrapping_add(0x10 + (row * 4 + column) as u32 * 4))?;
        }
        // Unused rows are zero and cannot fail the checked primitive.
        out[row] = matrix
            .try_get_scale()
            .map_err(|_| VehicleBodycolUnavailable::NumericDomain("BODYCOL GetScale"))?[row]
            .to_bits();
    }
    Ok(out)
}

/// Original 0x4d1060. `temporary` is the caller's 116-byte stack record.
pub fn bodycol_callback<B: VehicleBodycolBackend + ?Sized>(
    b: &mut B,
    header: u32,
    context: VehicleBodyCollisionContext,
    frame: u32,
    temporary: VehicleBodycolScratch,
) -> ResultFor<B, u32> {
    let t = temporary.record;
    put_byte(b, t, temporary.leading_byte)?;
    zero_words(b, t.wrapping_add(4), 3)?;
    // Embedded container constructor 0x5479c0; bounds ctor 0x401c60 is RET.
    put_byte(b, t.wrapping_add(0x10), temporary.nested_byte)?;
    zero_words(b, t.wrapping_add(0x14), 3)?;
    zero_words(b, t, 29)?;
    let frame_name = name(b, frame)?;
    write(b, t.wrapping_add(0x20), frame)?;
    let flag = byte(
        b,
        frame_name
            .wrapping_add(context.name_length_with_nul)
            .wrapping_add(7),
    )? == b'B';
    let original_bounds = bounds(b, frame)?;
    write(
        b,
        t.wrapping_add(0x54),
        radius::<B::Error>(original_bounds)?.to_bits(),
    )?;
    put_byte(b, t.wrapping_add(0x58), flag as u8)?;
    let parent = read(b, frame.wrapping_add(0x120))?;
    let first_scale = scale(b, frame, parent)?;
    let scaled_radius = multiply(
        f32::from_bits(first_scale[1]),
        number(b, t.wrapping_add(0x54))?,
    )?;
    let position_x = read(b, frame.wrapping_add(0x80))?;
    write(b, t.wrapping_add(0x54), scaled_radius.to_bits())?;
    let position_y = read(b, frame.wrapping_add(0x84))?;
    write(b, t.wrapping_add(0x24), position_x)?;
    write(b, t.wrapping_add(0x28), position_y)?;
    let parent = read(b, frame.wrapping_add(0x120))?;
    let position_z = read(b, frame.wrapping_add(0x88))?;
    write(b, t.wrapping_add(0x2c), position_z)?;
    for (i, value) in original_bounds.into_iter().enumerate() {
        write(b, t.wrapping_add(0x5c + i as u32 * 4), value)?;
    }
    let min_scale = scale(b, frame, parent)?;
    let min_x = multiply(
        f32::from_bits(min_scale[0]),
        number(b, t.wrapping_add(0x5c))?,
    )?;
    let parent = read(b, frame.wrapping_add(0x120))?;
    write(b, t.wrapping_add(0x5c), min_x.to_bits())?;
    for axis in [1, 2] {
        let a = t.wrapping_add(0x5c + axis * 4);
        let v = multiply(number(b, a)?, f32::from_bits(min_scale[axis as usize]))?;
        write(b, a, v.to_bits())?;
    }
    let max_scale = scale(b, frame, parent)?;
    let max_x = multiply(
        f32::from_bits(max_scale[0]),
        number(b, t.wrapping_add(0x68))?,
    )?;
    let retained_frame = read(b, t.wrapping_add(0x20))?;
    write(b, t.wrapping_add(0x68), max_x.to_bits())?;
    for axis in [1, 2] {
        let a = t.wrapping_add(0x68 + axis * 4);
        let v = multiply(number(b, a)?, f32::from_bits(max_scale[axis as usize]))?;
        write(b, a, v.to_bits())?;
    }
    let parent = read(b, retained_frame.wrapping_add(0x120))?;
    if parent == context.model
        || !group_bodycol_record(b, header, t).map_err(VehicleBodycolUnavailable::Effect)?
    {
        append_bodycol_record(b, header, t).map_err(VehicleBodycolUnavailable::Effect)?;
    }
    // Explicit native destruction/free order; no Rust Drop rewinds a failure.
    let nested_end = read(b, t.wrapping_add(0x18))?;
    let nested_begin = read(b, t.wrapping_add(0x14))?;
    destroy_bodycol_temporaries(b, nested_begin, nested_end)
        .map_err(VehicleBodycolUnavailable::Effect)?;
    let nested_begin = read(b, t.wrapping_add(0x14))?;
    b.free(nested_begin)
        .map_err(VehicleBodycolUnavailable::Effect)?;
    zero_words(b, t.wrapping_add(0x14), 3)?;
    let descendant_begin = read(b, t.wrapping_add(4))?;
    b.free(descendant_begin)
        .map_err(VehicleBodycolUnavailable::Effect)?;
    Ok(0)
}

/// Original 0x4d1520. Only direct children other than BODYCOL are collected.
pub fn bodycol_descendant_callback<B: VehicleBodycolBackend + ?Sized>(
    b: &mut B,
    parent_record: u32,
    name_length_with_nul: u32,
    frame: u32,
    temporary: u32,
) -> ResultFor<B, u32> {
    zero_words(b, temporary, 15)?;
    let parent = read(b, frame.wrapping_add(0x120))?;
    if parent != read(b, parent_record.wrapping_add(0x20))? {
        return Ok(0);
    }
    let suffix = name(b, frame)?.wrapping_add(name_length_with_nul);
    // Actual seven-byte CRT comparison with the original BODYCOL literal.
    let mut last = 0;
    for (i, expected) in b"BODYCOL".iter().enumerate() {
        last = i;
        if byte(b, suffix.wrapping_add(i as u32))? != *expected {
            break;
        }
    }
    // _strncmp reloads the last compared bytes after REPE CMPSB.
    let last_byte = byte(b, suffix.wrapping_add(last as u32))?;
    let matches = last_byte == b"BODYCOL"[last];
    if matches {
        return Ok(0);
    }
    let parent = read(b, frame.wrapping_add(0x120))?;
    let original_bounds = bounds(b, frame)?;
    write(
        b,
        temporary.wrapping_add(0x34),
        radius::<B::Error>(original_bounds)?.to_bits(),
    )?;
    let frame_scale = scale(b, frame, parent)?;
    let value = multiply(
        f32::from_bits(frame_scale[1]),
        number(b, temporary.wrapping_add(0x34))?,
    )?;
    write(b, temporary, frame)?;
    let x = read(b, frame.wrapping_add(0x80))?;
    write(b, temporary.wrapping_add(0x34), value.to_bits())?;
    let y = read(b, frame.wrapping_add(0x84))?;
    write(b, temporary.wrapping_add(4), x)?;
    write(b, temporary.wrapping_add(8), y)?;
    let z = read(b, frame.wrapping_add(0x88))?;
    write(b, temporary.wrapping_add(0xc), z)?;
    append_bodycol_descendant(b, parent_record, temporary)
        .map_err(VehicleBodycolUnavailable::Effect)?;
    Ok(0)
}

/// Original 0x4d1490: reverse iteration, followed by recursion into each child list.
pub fn bodycol_descendant_pass<B: VehicleBodycolBackend + ?Sized>(
    b: &mut B,
    header: u32,
    name_length_with_nul: u32,
    temporary60: u32,
) -> ResultFor<B, ()> {
    let begin = read(b, header.wrapping_add(4))?;
    if begin == 0 {
        return Ok(());
    }
    let end = read(b, header.wrapping_add(8))?;
    let count = (end.wrapping_sub(begin) as i32 / 116) as u32;
    if count == 0 {
        return Ok(());
    }
    let mut offset = count.wrapping_mul(116);
    for _ in 0..count {
        offset = offset.wrapping_sub(116);
        let base = read(b, header.wrapping_add(4))?;
        let record = base.wrapping_add(offset);
        let base = read(b, header.wrapping_add(4))?;
        let frame = read(b, base.wrapping_add(offset).wrapping_add(0x20))?;
        let mut target = VehicleBodycolEnumeration {
            target: BodycolEnumerationTarget::Descendants {
                parent_record: record,
                name_length_with_nul,
                temporary: temporary60,
            },
        };
        b.bodycol_enumerate(frame, b"*", 0x10020, &mut target)?;
        let base = read(b, header.wrapping_add(4))?;
        bodycol_descendant_pass(
            b,
            base.wrapping_add(offset).wrapping_add(0x10),
            name_length_with_nul,
            temporary60,
        )?;
    }
    Ok(())
}

/// Addressable imported mutating S_vector::operator*=, LS3DF RVA0x2e920.
/// All arithmetic finishes before the native Z,X,Y stores, preserving aliases.
fn transform_in_place<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    point: u32,
    matrix: u32,
) -> ResultFor<B, ()> {
    let mut out = [0.; 3];
    for (axis, terms) in [
        (0, [(8, 2), (4, 1), (0, 0)]),
        (1, [(9, 2), (1, 0), (5, 1)]),
        (2, [(10, 2), (2, 0), (6, 1)]),
    ] {
        let first = number(b, matrix.wrapping_add(terms[0].0 * 4))?;
        let first_point = number(b, point.wrapping_add(terms[0].1 * 4))?;
        let first = multiply(first, first_point)?;
        let second = number(b, matrix.wrapping_add(terms[1].0 * 4))?;
        let second_point = number(b, point.wrapping_add(terms[1].1 * 4))?;
        let second = multiply(second, second_point)?;
        let sum = add(first, second)?;
        let third = number(b, matrix.wrapping_add(terms[2].0 * 4))?;
        let third_point = number(b, point.wrapping_add(terms[2].1 * 4))?;
        let third = multiply(third, third_point)?;
        let sum = add(sum, third)?;
        let translation = number(b, matrix.wrapping_add((12 + axis as u32) * 4))?;
        out[axis] = add(sum, translation)?;
    }
    for axis in [2, 0, 1] {
        write(
            b,
            point.wrapping_add(axis * 4),
            out[axis as usize].to_bits(),
        )?;
    }
    Ok(())
}
fn world_matrix<B: VehicleBodycolBackend + ?Sized>(b: &mut B, frame: u32) -> ResultFor<B, u32> {
    if byte(b, frame.wrapping_add(0xac))? & 0x20 == 0 {
        b.bodycol_update_world(frame)
            .map_err(VehicleBodycolUnavailable::Effect)?;
    }
    Ok(frame.wrapping_add(0x10))
}
fn record_at<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    offset: u32,
) -> ResultFor<B, u32> {
    read(b, header.wrapping_add(4)).map(|base| base.wrapping_add(offset))
}
fn descendant_at<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    header: u32,
    offset: u32,
    child_offset: u32,
) -> ResultFor<B, u32> {
    let record = record_at(b, header, offset)?;
    read(b, record.wrapping_add(4)).map(|base| base.wrapping_add(child_offset))
}

/// Original recursive transform/detach pass 0x4d17f0. A true return propagates
/// the presence of an M-marked record; inherited retention alone does not set it.
pub fn bodycol_transform_detach<B: VehicleBodycolBackend + ?Sized>(
    b: &mut B,
    header: u32,
    inverse: u32,
    model: u32,
    name_length_with_nul: u32,
    inherited: bool,
) -> ResultFor<B, bool> {
    let begin = read(b, header.wrapping_add(4))?;
    if begin == 0 {
        return Ok(false);
    }
    let end = read(b, header.wrapping_add(8))?;
    let count = (end.wrapping_sub(begin) as i32 / 116) as u32;
    if count == 0 {
        return Ok(false);
    }
    let mut retained = false;
    let mut offset = count.wrapping_mul(116);
    for _ in 0..count {
        let base = read(b, header.wrapping_add(4))?;
        offset = offset.wrapping_sub(116);
        let record = base.wrapping_add(offset);
        let frame = read(b, record.wrapping_add(0x20))?;
        let first_name = name(b, frame)?;
        let marked = if byte(
            b,
            first_name
                .wrapping_add(name_length_with_nul)
                .wrapping_add(7),
        )? == b'M'
        {
            true
        } else {
            let second_name = name(b, frame)?;
            byte(
                b,
                second_name
                    .wrapping_add(name_length_with_nul)
                    .wrapping_add(8),
            )? == b'M'
        };
        if marked {
            retained = true;
        }
        let descendants_begin = read(b, record.wrapping_add(4))?;
        if descendants_begin != 0 {
            let descendants_end = read(b, record.wrapping_add(8))?;
            let descendants_count =
                (descendants_end.wrapping_sub(descendants_begin) as i32 / 60) as u32;
            let mut child_offset = descendants_count.wrapping_mul(60);
            for _ in 0..descendants_count {
                child_offset = child_offset.wrapping_sub(60);
                if inherited || marked {
                    continue;
                }
                let record = record_at(b, header, offset)?;
                let frame = read(b, record.wrapping_add(0x20))?;
                let matrix = world_matrix(b, frame)?;
                let child = descendant_at(b, header, offset, child_offset)?;
                transform_in_place(b, child.wrapping_add(4), matrix)?;
                let child = descendant_at(b, header, offset, child_offset)?;
                transform_in_place(b, child.wrapping_add(4), inverse)?;
                let child = descendant_at(b, header, offset, child_offset)?;
                let child_frame = read(b, child)?;
                b.bodycol_link_frame(child_frame, 0, 0)
                    .map_err(VehicleBodycolUnavailable::Effect)?;
                let child = descendant_at(b, header, offset, child_offset)?;
                let child_frame = read(b, child)?;
                b.bodycol_remove_model_child(model, child_frame)
                    .map_err(VehicleBodycolUnavailable::Effect)?;
                let child = descendant_at(b, header, offset, child_offset)?;
                write(b, child, 0)?;
            }
        }
        let record = record_at(b, header, offset)?;
        let child_retained = bodycol_transform_detach(
            b,
            record.wrapping_add(0x10),
            inverse,
            model,
            name_length_with_nul,
            inherited || marked,
        )?;
        if child_retained {
            retained = true;
        }
        if inherited || marked || child_retained {
            continue;
        }
        let record = record_at(b, header, offset)?;
        let frame = read(b, record.wrapping_add(0x20))?;
        let parent = read(b, frame.wrapping_add(0x120))?;
        let matrix = world_matrix(b, parent)?;
        let record = record_at(b, header, offset)?;
        transform_in_place(b, record.wrapping_add(0x24), matrix)?;
        let record = record_at(b, header, offset)?;
        transform_in_place(b, record.wrapping_add(0x24), inverse)?;
        let record = record_at(b, header, offset)?;
        let max_x = number(b, record.wrapping_add(0x68))?;
        let x = number(b, record.wrapping_add(0x24))?;
        write(b, record.wrapping_add(0x68), add(max_x, x)?.to_bits())?;
        for (position, bound) in [(0x28, 0x6c), (0x2c, 0x70)] {
            let a = number(b, record.wrapping_add(position))?;
            let c = number(b, record.wrapping_add(bound))?;
            write(b, record.wrapping_add(bound), add(a, c)?.to_bits())?;
        }
        let record = record_at(b, header, offset)?;
        for (position, bound) in [(0x24, 0x5c), (0x28, 0x60), (0x2c, 0x64)] {
            let a = number(b, record.wrapping_add(position))?;
            let c = number(b, record.wrapping_add(bound))?;
            write(b, record.wrapping_add(bound), add(a, c)?.to_bits())?;
        }
        let record = record_at(b, header, offset)?;
        let frame = read(b, record.wrapping_add(0x20))?;
        b.bodycol_link_frame(frame, 0, 0)
            .map_err(VehicleBodycolUnavailable::Effect)?;
        let record = record_at(b, header, offset)?;
        let frame = read(b, record.wrapping_add(0x20))?;
        b.bodycol_remove_model_child(model, frame)
            .map_err(VehicleBodycolUnavailable::Effect)?;
        let record = record_at(b, header, offset)?;
        write(b, record.wrapping_add(0x20), 0)?;
    }
    Ok(retained)
}

/// A live borrow overlays the projected vehicle bytes. Stores immediately
/// update the state, including on errors; there is no snapshot or copyback.
/// Frame effects and allocated records remain in disjoint backend storage as
/// required by VehicleInitBackend. Descendant enumeration callbacks touch
/// external records and scratch only, so they can visit the underlying backend.
pub(crate) struct VehicleBodycolProjection<'a, B: ?Sized> {
    backend: &'a mut B,
    storage: BodycolMemory<'a>,
    base: u32,
}
enum BodycolMemory<'a> {
    Legacy(&'a mut [u8]),
    Projected(VehicleComponentTailMemory<'a>),
}

impl<'a, B: VehicleInitBackend + ?Sized> VehicleBodycolProjection<'a, B> {
    pub(crate) fn projected(backend: &'a mut B, memory: VehicleComponentTailMemory<'a>) -> Self {
        Self {
            backend,
            base: 0,
            storage: BodycolMemory::Projected(memory),
        }
    }
    pub(crate) fn new(
        backend: &'a mut B,
        vehicle: &'a mut [u8],
        storage: VehicleBodycolStorage,
    ) -> ResultFor<B, Self> {
        let length = u32::try_from(vehicle.len())
            .map_err(|_| VehicleBodycolUnavailable::ProjectionStorage)?;
        let end = storage
            .vehicle_address
            .checked_add(length)
            .ok_or(VehicleBodycolUnavailable::ProjectionStorage)?;
        if length < 0xd68 {
            return Err(VehicleBodycolUnavailable::ProjectionStorage);
        }
        let spans = [
            (storage.callback_scratch.record, 116u32),
            (storage.descendant_scratch, 60u32),
        ];
        let mut scratch_ends = [0; 2];
        for (index, (start, size)) in spans.into_iter().enumerate() {
            let scratch_end = start
                .checked_add(size)
                .ok_or(VehicleBodycolUnavailable::ProjectionStorage)?;
            if start < end && storage.vehicle_address < scratch_end {
                return Err(VehicleBodycolUnavailable::ProjectionStorage);
            }
            scratch_ends[index] = scratch_end;
        }
        if spans[0].0 < scratch_ends[1] && spans[1].0 < scratch_ends[0] {
            return Err(VehicleBodycolUnavailable::ProjectionStorage);
        }
        Ok(Self {
            backend,
            storage: BodycolMemory::Legacy(vehicle),
            base: storage.vehicle_address,
        })
    }
    fn offset(&self, address: u32, bytes: usize) -> Option<usize> {
        let BodycolMemory::Legacy(vehicle) = &self.storage else {
            return None;
        };
        let offset = address.checked_sub(self.base)? as usize;
        offset
            .checked_add(bytes)
            .filter(|end| *end <= vehicle.len())
            .map(|_| offset)
    }
}
impl<B: VehicleInitBackend + ?Sized> VehicleInitBackend for VehicleBodycolProjection<'_, B> {
    type Error = B::Error;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        if let BodycolMemory::Projected(memory) = &mut self.storage {
            return memory.read_byte_with_backend(self.backend, address);
        }
        if let Some(offset) = self.offset(address, 1) {
            let BodycolMemory::Legacy(vehicle) = &self.storage else {
                unreachable!()
            };
            Ok(vehicle[offset])
        } else {
            self.backend.read_byte(address)
        }
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        if let BodycolMemory::Projected(memory) = &mut self.storage {
            return memory.read_word_with_backend(self.backend, address);
        }
        if let Some(offset) = self.offset(address, 4) {
            let BodycolMemory::Legacy(vehicle) = &self.storage else {
                unreachable!()
            };
            Ok(u32::from_le_bytes(
                vehicle[offset..offset + 4].try_into().unwrap(),
            ))
        } else {
            self.backend.read_word(address)
        }
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        if let BodycolMemory::Projected(memory) = &mut self.storage {
            return memory.write_byte_with_backend(self.backend, address, value);
        }
        if let Some(offset) = self.offset(address, 1) {
            let BodycolMemory::Legacy(vehicle) = &mut self.storage else {
                unreachable!()
            };
            vehicle[offset] = value;
            Ok(())
        } else {
            self.backend.write_byte(address, value)
        }
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        if let BodycolMemory::Projected(memory) = &mut self.storage {
            return memory.write_word_with_backend(self.backend, address, value);
        }
        if let Some(offset) = self.offset(address, 4) {
            let BodycolMemory::Legacy(vehicle) = &mut self.storage else {
                unreachable!()
            };
            vehicle[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            Ok(())
        } else {
            self.backend.write_word(address, value)
        }
    }
    fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
        match &mut self.storage {
            BodycolMemory::Projected(memory) => self.backend.free_projected(memory, allocation),
            BodycolMemory::Legacy(_) => self.backend.free(allocation),
        }
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        match &mut self.storage {
            BodycolMemory::Projected(memory) => self.backend.allocate_projected(memory, bytes),
            BodycolMemory::Legacy(_) => self.backend.allocate(bytes),
        }
    }
    fn collect_frame(
        &mut self,
        vector: &mut crate::vehicle_init::VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.backend.collect_frame(vector, frame)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: crate::vehicle_init::VehicleInitQuery,
        target: &mut crate::vehicle_init::VehicleInitEnumeration<'_>,
    ) -> Result<(), crate::vehicle_init::VehicleInitUnavailable<Self::Error>> {
        self.backend.enumerate(model, query, target)
    }
}
impl<B: VehicleBodycolBackend + ?Sized> VehicleBodycolBackend for VehicleBodycolProjection<'_, B> {
    fn bodycol_update_world(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.bodycol_update_world(frame)
    }
    fn bodycol_link_frame(
        &mut self,
        frame: u32,
        parent: u32,
        flags: u32,
    ) -> Result<(), Self::Error> {
        self.backend.bodycol_link_frame(frame, parent, flags)
    }
    fn bodycol_remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error> {
        self.backend.bodycol_remove_model_child(model, frame)
    }
    fn bodycol_enumerate(
        &mut self,
        frame: u32,
        pattern: &[u8],
        mask: u32,
        target: &mut VehicleBodycolEnumeration<'_>,
    ) -> Result<(), VehicleBodycolUnavailable<Self::Error>> {
        if let BodycolMemory::Projected(memory) = &mut self.storage {
            if let BodycolEnumerationTarget::Descendants {
                parent_record,
                name_length_with_nul,
                temporary,
            } = &target.target
            {
                let mut projected = VehicleBodycolEnumeration {
                    target: BodycolEnumerationTarget::ProjectedDescendants {
                        memory: memory.reborrow(),
                        parent_record: *parent_record,
                        name_length_with_nul: *name_length_with_nul,
                        temporary: *temporary,
                    },
                };
                return self
                    .backend
                    .bodycol_enumerate(frame, pattern, mask, &mut projected);
            }
        }
        self.backend.bodycol_enumerate(frame, pattern, mask, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    struct Memory {
        words: [u32; 32],
        stores: Vec<(u32, u32)>,
        fail_store: Option<usize>,
    }
    impl Memory {
        fn new() -> Self {
            let mut words = [0; 32];
            for (index, value) in SMatrix::identity().elements.into_iter().enumerate() {
                words[index] = value.to_bits();
            }
            words[0] = 2f32.to_bits();
            words[5] = 3f32.to_bits();
            words[10] = 4f32.to_bits();
            words[12] = 5f32.to_bits();
            words[13] = 6f32.to_bits();
            words[14] = 7f32.to_bits();
            Self {
                words,
                stores: vec![],
                fail_store: None,
            }
        }
    }
    impl VehicleInitBackend for Memory {
        type Error = &'static str;
        fn read_byte(&mut self, _: u32) -> Result<u8, Self::Error> {
            panic!("unexpected byte read")
        }
        fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
            self.words
                .get((address / 4) as usize)
                .copied()
                .ok_or("read")
        }
        fn write_byte(&mut self, _: u32, _: u8) -> Result<(), Self::Error> {
            panic!("unexpected byte write")
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            let index = self.stores.len();
            self.stores.push((address, value));
            if self.fail_store == Some(index) {
                return Err("store");
            }
            *self.words.get_mut((address / 4) as usize).ok_or("write")? = value;
            Ok(())
        }
        fn free(&mut self, _: u32) -> Result<(), Self::Error> {
            panic!("unexpected free")
        }
        fn allocate(&mut self, _: u32) -> Result<u32, Self::Error> {
            panic!("unexpected allocation")
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            panic!("unexpected collection")
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            panic!("unexpected enumeration")
        }
    }
    #[test]
    fn mutating_transform_keeps_aliases_and_original_z_x_y_stores() {
        let mut memory = Memory::new();
        // Point aliases the matrix translation; every input is read before
        // the first store, as in the imported mutating operator.
        transform_in_place(&mut memory, 48, 0).unwrap();
        assert_eq!(
            memory.stores,
            [
                (56, 35f32.to_bits()),
                (48, 15f32.to_bits()),
                (52, 24f32.to_bits())
            ]
        );
    }
    #[test]
    fn mutating_transform_store_failure_keeps_only_completed_native_stores() {
        for failure in 0..3 {
            let mut memory = Memory::new();
            let before = memory.words;
            memory.fail_store = Some(failure);
            assert_eq!(
                transform_in_place(&mut memory, 48, 0),
                Err(VehicleBodycolUnavailable::Effect("store"))
            );
            assert_eq!(memory.stores.len(), failure + 1);
            for (index, (address, value)) in memory.stores.iter().enumerate() {
                assert_eq!(
                    memory.words[(address / 4) as usize],
                    if index < failure {
                        *value
                    } else {
                        before[(address / 4) as usize]
                    }
                );
            }
        }
    }
    #[test]
    fn projection_rejects_scratch_aliases_and_wrapping_addresses_before_effects() {
        for (base, callback, descendant) in [
            (0x1000, 0x1100, 0x8000),
            (0x1000, 0x8000, 0x8040),
            (u32::MAX - 8, 0x8000, 0x9000),
        ] {
            let mut memory = Memory::new();
            let mut vehicle = vec![0; 0x2008];
            let storage = VehicleBodycolStorage {
                vehicle_address: base,
                callback_scratch: VehicleBodycolScratch {
                    record: callback,
                    leading_byte: 0x23,
                    nested_byte: 0x42,
                },
                descendant_scratch: descendant,
            };
            assert!(matches!(
                VehicleBodycolProjection::new(&mut memory, &mut vehicle, storage),
                Err(VehicleBodycolUnavailable::ProjectionStorage)
            ));
            assert!(memory.stores.is_empty());
        }
    }
}

#[cfg(test)]
mod car_projection_tests {
    use super::*;
    use crate::vehicle_init::{
        CarInitRegisters, CarInitScratch, VehicleComponentTailScratch, VehicleInitEnumeration,
        VehicleInitFrameVector, VehicleInitQuery, VehicleInitState, VehicleInitUnavailable,
        CAR_INIT_STACK_BYTES, VEHICLE_INIT_STACK_BYTES,
    };
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Memory {
        bytes: BTreeMap<u32, u8>,
        frees: usize,
        fail_free: bool,
    }
    impl VehicleInitBackend for Memory {
        type Error = ();
        fn read_byte(&mut self, p: u32) -> Result<u8, ()> {
            Ok(*self.bytes.get(&p).unwrap_or(&0))
        }
        fn read_word(&mut self, p: u32) -> Result<u32, ()> {
            let mut v = [0; 4];
            for (i, b) in v.iter_mut().enumerate() {
                *b = self.read_byte(p + i as u32)?;
            }
            Ok(u32::from_le_bytes(v))
        }
        fn write_byte(&mut self, p: u32, v: u8) -> Result<(), ()> {
            self.bytes.insert(p, v);
            Ok(())
        }
        fn write_word(&mut self, p: u32, v: u32) -> Result<(), ()> {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.write_byte(p + i as u32, b)?;
            }
            Ok(())
        }
        fn free(&mut self, _: u32) -> Result<(), ()> {
            panic!("BODYCOL free lost its canonical projection")
        }
        fn allocate(&mut self, _: u32) -> Result<u32, ()> {
            panic!("BODYCOL allocation lost its canonical projection")
        }
        fn allocate_projected(
            &mut self,
            m: &mut VehicleComponentTailMemory<'_>,
            _: u32,
        ) -> Result<u32, ()> {
            m.write_word(0x10018, 0x11111111).unwrap();
            m.write_word(0x121d8, 0x22222222).unwrap();
            m.write_word(0x80100, 0x33333333).unwrap();
            Ok(0x90000)
        }
        fn free_projected(
            &mut self,
            m: &mut VehicleComponentTailMemory<'_>,
            _: u32,
        ) -> Result<(), ()> {
            self.frees += 1;
            m.write_word(0x1001c, 0x44444444).unwrap();
            if self.fail_free {
                Err(())
            } else {
                Ok(())
            }
        }
        fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), ()> {
            unreachable!()
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<()>> {
            unreachable!()
        }
    }
    impl VehicleBodycolBackend for Memory {
        fn bodycol_update_world(&mut self, _: u32) -> Result<(), ()> {
            unreachable!()
        }
        fn bodycol_link_frame(&mut self, _: u32, _: u32, _: u32) -> Result<(), ()> {
            unreachable!()
        }
        fn bodycol_remove_model_child(&mut self, _: u32, _: u32) -> Result<(), ()> {
            unreachable!()
        }
        fn bodycol_enumerate(
            &mut self,
            _: u32,
            _: &[u8],
            _: u32,
            target: &mut VehicleBodycolEnumeration<'_>,
        ) -> Result<(), VehicleBodycolUnavailable<()>> {
            target.visit(0x6000, self).map(|_| ())
        }
    }
    fn run(
        fail_free: bool,
    ) -> (
        VehicleInitState,
        Memory,
        Result<(), VehicleBodycolUnavailable<()>>,
    ) {
        let mut bytes = vec![0; 0x2370];
        bytes[0x1000 + 0x20..0x1000 + 0x24].copy_from_slice(&0u32.to_le_bytes());
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
        let mut scratch = VehicleComponentTailScratch {
            address: 0x7fe30,
            bytes: [0x7d; VEHICLE_INIT_STACK_BYTES],
        };
        let mut b = Memory {
            fail_free,
            ..Default::default()
        };
        b.write_word(0x6100, 0x7000).unwrap();
        b.write_byte(0x7000, b'X').unwrap();
        for p in [0x6090, 0x6094, 0x6098] {
            b.write_word(p, 1f32.to_bits()).unwrap();
        }
        let memory = state.car_live_memory(&mut scratch).unwrap();
        let mut projection = VehicleBodycolProjection::projected(&mut b, memory);
        let mut target = VehicleBodycolEnumeration {
            target: BodycolEnumerationTarget::Descendants {
                parent_record: 0x11000,
                name_length_with_nul: 0,
                temporary: 0x80100,
            },
        };
        let result = projection.bodycol_enumerate(0x5000, b"*", 0, &mut target);
        (state, b, result)
    }
    #[test]
    fn nested_bodycol_allocation_and_free_observe_one_car_and_both_stack_owners() {
        let (state, mut b, result) = run(false);
        assert_eq!(result, Ok(()));
        let bytes = state.car_bytes().unwrap();
        assert_eq!(&bytes[0x18..0x1c], &0x11111111u32.to_le_bytes());
        assert_eq!(&bytes[0x1c..0x20], &0x44444444u32.to_le_bytes());
        assert_eq!(&bytes[0x21d8..0x21dc], &0x22222222u32.to_le_bytes());
        assert_eq!(b.read_word(0x90000), Ok(0x33333333));
        assert_eq!(
            &state.car_init_scratch().unwrap().bytes[0x100..0x104],
            &0x33333333u32.to_le_bytes()
        );
        assert_eq!(b.frees, 1);
    }
    #[test]
    fn nested_bodycol_failed_free_keeps_canonical_allocator_mutations_and_copy_prefix() {
        let (state, mut b, result) = run(true);
        assert_eq!(result, Err(VehicleBodycolUnavailable::Effect(())));
        let bytes = state.car_bytes().unwrap();
        assert_eq!(&bytes[0x1c..0x20], &0x44444444u32.to_le_bytes());
        assert_eq!(&bytes[0x21d8..0x21dc], &0x22222222u32.to_le_bytes());
        assert_eq!(b.read_word(0x90000), Ok(0x33333333));
        assert_eq!(&bytes[0x1004..0x1008], &0u32.to_le_bytes());
    }
}
