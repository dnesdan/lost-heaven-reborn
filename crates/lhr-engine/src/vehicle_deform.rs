//! Original deformation-resource preparation, Game RVA0xd4d30.
//! The 52-byte records remain in caller memory; resource identities belong to
//! the shared visual owner. This helper does not own an alternate scene graph.
use super::*;
pub trait VehicleDeformBackend: VehicleInitBackend {
    fn deform_visual_mesh(&mut self, frame: u32) -> Result<u32, Self::Error>;
    fn deform_set_visual_mesh(
        &mut self,
        frame: u32,
        mesh: u32,
        flags: u32,
    ) -> Result<(), Self::Error>;
    fn deform_release_mesh(&mut self, mesh: u32) -> Result<(), Self::Error>;
    fn deform_create_mesh(&mut self) -> Result<u32, Self::Error>;
    fn deform_duplicate_mesh(&mut self, destination: u32, source: u32) -> Result<u32, Self::Error>;
    fn deform_mesh_lod_count(&mut self, mesh: u32) -> Result<u32, Self::Error>;
    fn deform_remove_mesh_lod(&mut self, mesh: u32, index: u32) -> Result<(), Self::Error>;
    fn deform_mesh_lod(&mut self, mesh: u32, index: u32) -> Result<u32, Self::Error>;
    fn deform_lod_face_group(&mut self, lod: u32, index: u32) -> Result<u32, Self::Error>;
    fn deform_face_group_material(&mut self, group: u32) -> Result<u32, Self::Error>;
    fn deform_retain_material(&mut self, material: u32) -> Result<(), Self::Error>;
    fn deform_create_material(&mut self) -> Result<u32, Self::Error>;
    fn deform_copy_material(&mut self, destination: u32, source: u32) -> Result<(), Self::Error>;
}
fn at(p: u32, o: u32) -> u32 {
    p.wrapping_add(o)
}
fn count(begin: u32, end: u32) -> u32 {
    ((end.wrapping_sub(begin) as i32) / 52) as u32
}
fn copy<B: VehicleInitBackend>(b: &mut B, d: u32, s: u32) -> Result<(), B::Error> {
    for o in (0..52).step_by(4) {
        let v = b.read_word(at(s, o))?;
        b.write_word(at(d, o), v)?;
    }
    Ok(())
}
fn placement<B: VehicleInitBackend>(b: &mut B, d: u32, s: u32) -> Result<(), B::Error> {
    if d != 0 {
        copy(b, d, s)?;
    }
    Ok(())
}
fn range<B: VehicleInitBackend>(
    b: &mut B,
    mut s: u32,
    end: u32,
    mut d: u32,
) -> Result<u32, B::Error> {
    while s != end {
        placement(b, d, s)?;
        s = at(s, 52);
        d = at(d, 52);
    }
    Ok(d)
}
fn append<B: VehicleInitBackend>(
    b: &mut B,
    h: u32,
    source: u32,
    stack: u32,
) -> Result<(), B::Error> {
    let insertion = b.read_word(at(h, 8))?;
    let capacity = b.read_word(at(h, 12))?;
    if count(insertion, capacity) < 1 {
        let begin = b.read_word(at(h, 4))?;
        let old_count = if begin == 0 {
            0
        } else {
            count(begin, insertion)
        };
        let grown = old_count.wrapping_add(old_count.max(1));
        let allocation = b.allocate(if (grown as i32) < 0 {
            0
        } else {
            grown.wrapping_mul(52)
        })?;
        let begin = b.read_word(at(h, 4))?;
        let d = range(b, begin, insertion, allocation)?;
        b.write_word(at(stack, 0x14), d)?;
        placement(b, d, source)?;
        let d = b.read_word(at(stack, 0x14))?;
        let end = b.read_word(at(h, 8))?;
        range(b, insertion, end, at(d, 52))?;
        let _end = b.read_word(at(h, 8))?;
        let _begin = b.read_word(at(h, 4))?;
        let old = b.read_word(at(h, 4))?;
        b.free(old)?;
        let old = b.read_word(at(h, 4))?;
        b.write_word(at(h, 12), at(allocation, grown.wrapping_mul(52)))?;
        let old_count = if old == 0 {
            0
        } else {
            count(old, b.read_word(at(h, 8))?)
        };
        b.write_word(at(h, 4), allocation)?;
        b.write_word(
            at(h, 8),
            at(allocation, old_count.wrapping_add(1).wrapping_mul(52)),
        )?;
    } else {
        let end = b.read_word(at(h, 8))?;
        let n = 1u32.wrapping_sub(count(insertion, end));
        let mut d = end;
        for _ in 0..n {
            placement(b, d, source)?;
            d = at(d, 52);
        }
        let end = b.read_word(at(h, 8))?;
        let mut d = insertion;
        while d != end {
            copy(b, d, source)?;
            d = at(d, 52);
        }
        let end = b.read_word(at(h, 8))?;
        b.write_word(at(h, 8), at(end, 52))?;
    }
    Ok(())
}
/// `stack` is ESP after SUB44 and four saved-register pushes (callerESP−58).
/// The bool is the original AL. Its Setup caller intentionally ignores it.
pub fn vehicle_prepare_deform<B: VehicleDeformBackend>(
    b: &mut B,
    mut vehicle: u32,
    stack: u32,
) -> Result<bool, B::Error> {
    let header = at(vehicle, 0x214);
    let begin = b.read_word(at(header, 4))?;
    b.write_word(at(stack, 0x1c), vehicle)?;
    let n = if begin == 0 {
        0
    } else {
        count(begin, b.read_word(at(header, 8))?)
    };
    let mut last = n.wrapping_sub(1);
    let begin = b.read_word(at(header, 4))?;
    b.write_word(at(stack, 0x14), last)?;
    if begin != 0 {
        let mut n = count(begin, b.read_word(at(header, 8))?);
        if n != 0 {
            let mut offset = n.wrapping_mul(52);
            let mut last_offset = last.wrapping_mul(52);
            b.write_word(at(stack, 0x18), last_offset)?;
            while n != 0 {
                let begin = b.read_word(at(header, 4))?;
                offset = offset.wrapping_sub(52);
                n = n.wrapping_sub(1);
                let record = at(begin, offset);
                let low = b.read_byte(at(record, 12))?;
                let high = b.read_byte(at(record, 13))?;
                b.write_word(at(stack, 0x10), n)?;
                if u16::from_le_bytes([low, high]) == 2 {
                    if (n as i32) < (last as i32) {
                        let end = at(begin, last_offset);
                        copy(b, at(stack, 0x20), end)?;
                        let saved_last_offset = b.read_word(at(stack, 0x18))?;
                        copy(b, end, record)?;
                        let begin = b.read_word(at(header, 4))?;
                        copy(b, at(begin, offset), at(stack, 0x20))?;
                        last = b.read_word(at(stack, 0x14))?.wrapping_sub(1);
                        last_offset = saved_last_offset.wrapping_sub(52);
                        b.write_word(at(stack, 0x14), last)?;
                        n = b.read_word(at(stack, 0x10))?;
                        last = b.read_word(at(stack, 0x14))?;
                        b.write_word(at(stack, 0x18), last_offset)?;
                    } else {
                        last = last.wrapping_sub(1);
                        last_offset = last_offset.wrapping_sub(52);
                        b.write_word(at(stack, 0x14), last)?;
                        b.write_word(at(stack, 0x18), last_offset)?;
                    }
                }
            }
        }
    }
    let mut offset = 0;
    b.write_word(at(stack, 0x10), 0)?;
    b.write_word(at(stack, 0x18), 0)?;
    loop {
        let header = at(vehicle, 0x214);
        let begin = b.read_word(at(header, 4))?;
        if begin == 0 {
            return Ok(true);
        }
        let end = b.read_word(at(header, 8))?;
        let index = b.read_word(at(stack, 0x10))?;
        if index >= count(begin, end) {
            return Ok(true);
        }
        let begin = b.read_word(at(header, 4))?;
        copy(b, at(stack, 0x20), at(begin, offset))?;
        let mesh = b.deform_create_mesh()?;
        b.write_word(at(stack, 0x24), mesh)?;
        if mesh == 0 {
            return Ok(false);
        }
        let begin = b.read_word(at(header, 4))?;
        let source = b.read_word(at(begin, offset.wrapping_add(4)))?;
        if b.deform_duplicate_mesh(mesh, source)? != 0 {
            return Ok(false);
        }
        let mut mesh = b.read_word(at(stack, 0x24))?;
        let mut lod = b.deform_mesh_lod_count(mesh)?.wrapping_sub(1);
        while (lod as i32) > 1 {
            b.deform_remove_mesh_lod(mesh, lod)?;
            mesh = b.read_word(at(stack, 0x24))?;
            lod = lod.wrapping_sub(1);
        }
        let begin = b.read_word(at(header, 4))?;
        let record = at(begin, offset);
        let low = b.read_byte(at(record, 12))?;
        let high = b.read_byte(at(record, 13))?;
        if u16::from_le_bytes([low, high]) == 3 {
            let lod = b.deform_mesh_lod(mesh, 0)?;
            let group = b.deform_lod_face_group(lod, 0)?;
            let material = b.deform_face_group_material(group)?;
            b.write_word(at(stack, 0x28), material)?;
            b.deform_retain_material(material)?;
            let begin = b.read_word(at(header, 4))?;
            let replacement = b.deform_create_material()?;
            b.write_word(at(begin, offset.wrapping_add(8)), replacement)?;
            let begin = b.read_word(at(header, 4))?;
            let material = b.deform_face_group_material(group)?;
            let replacement = b.read_word(at(begin, offset.wrapping_add(8)))?;
            b.deform_copy_material(replacement, material)?;
        }
        append(b, at(vehicle, 0x204), at(stack, 0x20), stack)?;
        let index = b.read_word(at(stack, 0x10))?;
        let current = b.read_word(at(stack, 0x18))?;
        vehicle = b.read_word(at(stack, 0x1c))?;
        b.write_word(at(stack, 0x10), index.wrapping_add(1))?;
        offset = current.wrapping_add(52);
        b.write_word(at(stack, 0x18), offset)?;
    }
}
impl<B: VehicleDeformBackend> VehicleDeformBackend for super::component_tail::Projection<'_, B> {
    fn deform_visual_mesh(&mut self, f: u32) -> Result<u32, Self::Error> {
        self.backend.deform_visual_mesh(f)
    }
    fn deform_set_visual_mesh(&mut self, f: u32, m: u32, flags: u32) -> Result<(), Self::Error> {
        self.backend.deform_set_visual_mesh(f, m, flags)
    }
    fn deform_release_mesh(&mut self, m: u32) -> Result<(), Self::Error> {
        self.backend.deform_release_mesh(m)
    }
    fn deform_create_mesh(&mut self) -> Result<u32, Self::Error> {
        self.backend.deform_create_mesh()
    }
    fn deform_duplicate_mesh(&mut self, d: u32, s: u32) -> Result<u32, Self::Error> {
        self.backend.deform_duplicate_mesh(d, s)
    }
    fn deform_mesh_lod_count(&mut self, m: u32) -> Result<u32, Self::Error> {
        self.backend.deform_mesh_lod_count(m)
    }
    fn deform_remove_mesh_lod(&mut self, m: u32, i: u32) -> Result<(), Self::Error> {
        self.backend.deform_remove_mesh_lod(m, i)
    }
    fn deform_mesh_lod(&mut self, m: u32, i: u32) -> Result<u32, Self::Error> {
        self.backend.deform_mesh_lod(m, i)
    }
    fn deform_lod_face_group(&mut self, l: u32, i: u32) -> Result<u32, Self::Error> {
        self.backend.deform_lod_face_group(l, i)
    }
    fn deform_face_group_material(&mut self, g: u32) -> Result<u32, Self::Error> {
        self.backend.deform_face_group_material(g)
    }
    fn deform_retain_material(&mut self, m: u32) -> Result<(), Self::Error> {
        self.backend.deform_retain_material(m)
    }
    fn deform_create_material(&mut self) -> Result<u32, Self::Error> {
        self.backend.deform_create_material()
    }
    fn deform_copy_material(&mut self, d: u32, s: u32) -> Result<(), Self::Error> {
        self.backend.deform_copy_material(d, s)
    }
}

/// Original visual deformation-discovery callback at0x4d1a80. The supplied
/// callback stack covers its0x48-byte locals/register span and two arguments.
/// No resource reference is acquired merely by storing a pointer in its record.
pub fn vehicle_collect_deform<B: VehicleDeformBackend>(
    b: &mut B,
    frame: u32,
    _header: u32,
    stack: u32,
) -> Result<u32, B::Error> {
    for o in (0x14..0x48).step_by(4) {
        b.write_word(at(stack, o), 0)?;
    }
    b.write_word(at(stack, 0x14), frame)?;
    let mut property = b.read_word(at(frame, 0xc))?;
    if property == 0 {
        property = 0x63d2a0;
    }
    let mut length = 0u32;
    while b.read_byte(at(property, length))? != 0 {
        length = length.wrapping_add(1);
    }
    if length <= 11 {
        return Ok(0);
    }
    let mut i = 0u32;
    let mut byte = b.read_byte(property)?;
    while byte != b'[' {
        if i >= length {
            return Ok(0);
        }
        i = i.wrapping_add(1);
        byte = b.read_byte(at(property, i))?;
    }
    if i >= length {
        return Ok(0);
    }
    i = i.wrapping_add(1);
    let start = i;
    byte = b.read_byte(at(property, i))?;
    while byte != b']' {
        if i >= length {
            return Ok(0);
        }
        i = i.wrapping_add(1);
        byte = b.read_byte(at(property, i))?;
    }
    if i >= length {
        return Ok(0);
    }
    let length = i.wrapping_sub(start);
    if (length as i32) < 10 {
        return Ok(0);
    }
    let mut kind = 0u16;
    for (tag, value) in [
        (b"DEFORM BODY".as_slice(), 1),
        (b"DEFORM BUMPER", 2),
        (b"DEFORM GLASS", 3),
        (b"DEFORM LIGHT", 5),
        (b"DEFORM SPZ", 7),
        (b"DEFORM MIRROR", 4),
        (b"DEFORM WING", 6),
        (b"DEFORM DOOR", 8),
        (b"DEFORM ROOF", 9),
    ] {
        if length < tag.len() as u32 {
            continue;
        }
        let mut matched = true;
        for (j, &expected) in tag.iter().enumerate() {
            if b.read_byte(at(property, start.wrapping_add(j as u32)))? != expected {
                matched = false;
                break;
            }
        }
        if matched {
            kind = value;
            break;
        }
    }
    if kind == 0 {
        return Ok(0);
    }
    for (i, x) in kind.to_le_bytes().into_iter().enumerate() {
        b.write_byte(at(stack, 0x20 + i as u32), x)?;
    }
    let mesh = b.deform_create_mesh()?;
    b.write_word(at(stack, 0x18), mesh)?;
    if mesh == 0 {
        return Ok(2);
    }
    let frame = b.read_word(at(stack, 0x14))?;
    let source = b.deform_visual_mesh(frame)?;
    if b.deform_duplicate_mesh(mesh, source)? != 0 {
        return Ok(2);
    }
    let mesh = b.read_word(at(stack, 0x18))?;
    if mesh == 0 {
        return Ok(2);
    }
    let frame = b.read_word(at(stack, 0x14))?;
    b.deform_set_visual_mesh(frame, mesh, 0)?;
    let mesh = b.read_word(at(stack, 0x18))?;
    b.deform_release_mesh(mesh)?;
    let header = b.read_word(at(stack, 0x50))?;
    append_discovery(b, header, at(stack, 0x14), stack)?;
    Ok(0)
}
fn append_discovery<B: VehicleInitBackend>(
    b: &mut B,
    h: u32,
    source: u32,
    stack: u32,
) -> Result<(), B::Error> {
    let insertion = b.read_word(at(h, 8))?;
    let capacity = b.read_word(at(h, 12))?;
    if count(insertion, capacity) < 1 {
        let begin = b.read_word(at(h, 4))?;
        let old_count = if begin == 0 {
            0
        } else {
            count(begin, insertion)
        };
        let grown = old_count.wrapping_add(old_count.max(1));
        b.write_word(at(stack, 0x50), grown)?;
        let allocation = b.allocate(if (grown as i32) < 0 {
            0
        } else {
            grown.wrapping_mul(52)
        })?;
        let begin = b.read_word(at(h, 4))?;
        b.write_word(at(stack, 0x10), allocation)?;
        let d = range(b, begin, insertion, allocation)?;
        placement(b, d, source)?;
        let end = b.read_word(at(h, 8))?;
        if insertion != end {
            let delta = at(d, 52).wrapping_sub(insertion);
            b.write_word(at(stack, 0x4c), delta)?;
            let mut s = insertion;
            let mut first = true;
            loop {
                let delta = if first {
                    delta
                } else {
                    b.read_word(at(stack, 0x4c))?
                };
                placement(b, at(s, delta), s)?;
                s = at(s, 52);
                if s == end {
                    break;
                }
                first = false;
            }
        }
        let old = b.read_word(at(h, 4))?;
        b.free(old)?;
        let grown = b.read_word(at(stack, 0x50))?;
        let allocation = b.read_word(at(stack, 0x10))?;
        let old = b.read_word(at(h, 4))?;
        b.write_word(at(h, 12), at(allocation, grown.wrapping_mul(52)))?;
        let n = if old == 0 {
            0
        } else {
            count(old, b.read_word(at(h, 8))?)
        };
        b.write_word(at(h, 4), allocation)?;
        b.write_word(at(h, 8), at(allocation, n.wrapping_add(1).wrapping_mul(52)))?;
    } else {
        let end = b.read_word(at(h, 8))?;
        let n = 1u32.wrapping_sub(count(insertion, end));
        let mut d = end;
        for _ in 0..n {
            placement(b, d, source)?;
            d = at(d, 52);
        }
        let end = b.read_word(at(h, 8))?;
        let mut d = insertion;
        while d != end {
            copy(b, d, source)?;
            d = at(d, 52);
        }
        let end = b.read_word(at(h, 8))?;
        b.write_word(at(h, 8), at(end, 52))?;
    }
    Ok(())
}

/// Live projection used by the real graph enumerator. Callback scratch follows
/// EnumFrames/EnumSubtree's original recursive stack, not an auxiliary buffer.
pub struct VehicleDeformEnumeration<'a> {
    memory: VehicleComponentTailMemory<'a>,
    setup_stack: u32,
}
impl<'a> VehicleDeformEnumeration<'a> {
    pub fn new(memory: VehicleComponentTailMemory<'a>, setup_stack: u32) -> Self {
        Self {
            memory,
            setup_stack,
        }
    }
    pub fn model<B: VehicleInitBackend>(&mut self, b: &mut B) -> Result<u32, B::Error> {
        let p = self.memory.vehicle_address.wrapping_add(0x34c);
        self.memory.read_word_with_backend(b, p)
    }
    pub fn visit<B: VehicleDeformBackend>(
        &mut self,
        b: &mut B,
        frame: u32,
        depth: u32,
    ) -> Result<u32, B::Error> {
        let header = self.memory.vehicle_address.wrapping_add(0x214);
        let stack = self
            .setup_stack
            .wrapping_sub(0xa0)
            .wrapping_sub(depth.wrapping_mul(0x24));
        let mut projection = super::component_tail::Projection {
            backend: b,
            memory: VehicleComponentTailMemory {
                car: self.memory.car.as_mut().map(CarInitMemory::reborrow),
                vehicle_address: self.memory.vehicle_address,
                vehicle: self.memory.vehicle,
                scratch: self.memory.scratch,
            },
        };
        projection.write_word(at(stack, 0x4c), frame)?;
        projection.write_word(at(stack, 0x50), header)?;
        vehicle_collect_deform(&mut projection, frame, header, stack)
    }
    pub fn prepare<B: VehicleDeformBackend>(&mut self, b: &mut B) -> Result<(), B::Error> {
        let vehicle = self.memory.vehicle_address;
        let mut projection = super::component_tail::Projection {
            backend: b,
            memory: VehicleComponentTailMemory {
                car: self.memory.car.as_mut().map(CarInitMemory::reborrow),
                vehicle_address: vehicle,
                vehicle: self.memory.vehicle,
                scratch: self.memory.scratch,
            },
        };
        vehicle_prepare_deform(
            &mut projection,
            vehicle,
            self.setup_stack.wrapping_sub(0x58),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
    struct FactoryFailure {
        memory: VehicleFrameMemory,
        calls: u32,
    }
    impl FactoryFailure {
        fn new() -> Self {
            let mut memory = VehicleFrameMemory::default();
            memory.map_bytes(0x1000, &vec![0xa5; 0x6000]).unwrap();
            Self { memory, calls: 0 }
        }
    }
    impl VehicleInitBackend for FactoryFailure {
        type Error = VehicleMemoryError;
        fn read_byte(&mut self, p: u32) -> Result<u8, Self::Error> {
            self.memory.read_byte(p)
        }
        fn read_word(&mut self, p: u32) -> Result<u32, Self::Error> {
            self.memory.read_word(p)
        }
        fn write_byte(&mut self, p: u32, v: u8) -> Result<(), Self::Error> {
            self.memory.write_byte(p, v)
        }
        fn write_word(&mut self, p: u32, v: u32) -> Result<(), Self::Error> {
            self.memory.write_word(p, v)
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.memory.allocate(n)
        }
        fn free(&mut self, p: u32) -> Result<(), Self::Error> {
            self.memory.free(p)
        }
        fn collect_frame(
            &mut self,
            v: &mut VehicleInitFrameVector,
            f: u32,
        ) -> Result<(), Self::Error> {
            self.memory.collect_frame(v, f)
        }
        fn enumerate(
            &mut self,
            m: u32,
            q: VehicleInitQuery,
            t: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            self.memory.enumerate(m, q, t)
        }
    }
    impl VehicleDeformBackend for FactoryFailure {
        fn deform_create_mesh(&mut self) -> Result<u32, Self::Error> {
            self.calls += 1;
            Ok(0)
        }
        fn deform_visual_mesh(&mut self, _: u32) -> Result<u32, Self::Error> {
            panic!("factory failure precedes mesh read")
        }
        fn deform_set_visual_mesh(&mut self, _: u32, _: u32, _: u32) -> Result<(), Self::Error> {
            panic!("factory failure precedes SetMesh")
        }
        fn deform_release_mesh(&mut self, _: u32) -> Result<(), Self::Error> {
            panic!("factory failure precedes Release")
        }
        fn deform_duplicate_mesh(&mut self, _: u32, _: u32) -> Result<u32, Self::Error> {
            panic!("factory failure precedes Duplicate")
        }
        fn deform_mesh_lod_count(&mut self, _: u32) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn deform_remove_mesh_lod(&mut self, _: u32, _: u32) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn deform_mesh_lod(&mut self, _: u32, _: u32) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn deform_lod_face_group(&mut self, _: u32, _: u32) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn deform_face_group_material(&mut self, _: u32) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn deform_retain_material(&mut self, _: u32) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn deform_create_material(&mut self) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn deform_copy_material(&mut self, _: u32, _: u32) -> Result<(), Self::Error> {
            unreachable!()
        }
    }
    #[test]
    fn discovery_uses_original_space_tags_and_keeps_factory_failure_prefix() {
        for (property, kind) in [
            (b"[DEFORM BODY]".as_slice(), 1),
            (b"[DEFORM BODY,I]", 1),
            (b"[DEFORM GLASS]", 3),
            (b"x[DEFORM SPZ]", 7),
            (b"[DEFORM_BODY]", 0),
            (b"[unknown][DEFORM BODY]", 0),
        ] {
            let mut b = FactoryFailure::new();
            b.write_word(0x100c, 0x2000).unwrap();
            for (i, &value) in property.iter().chain([0].iter()).enumerate() {
                b.write_byte(0x2000 + i as u32, value).unwrap();
            }
            b.write_word(0x404c, 0x1000).unwrap();
            b.write_word(0x4050, 0x3000).unwrap();
            let result = vehicle_collect_deform(&mut b, 0x1000, 0x3000, 0x4000).unwrap();
            assert_eq!(result, if kind == 0 { 0 } else { 2 });
            assert_eq!(b.calls, u32::from(kind != 0));
            assert_eq!(b.read_word(0x4014), Ok(0x1000));
            assert_eq!(b.read_word(0x4018), Ok(0));
            assert_eq!(b.read_word(0x4020), Ok(kind));
            assert_eq!(b.read_word(0x3004), Ok(0xa5a5a5a5));
        }
    }
    #[test]
    fn preparation_partitions_bumpers_before_factory_failure_without_rollback() {
        let mut b = FactoryFailure::new();
        let vehicle = 0x1000;
        let stack = 0x4000;
        let source = 0x6000;
        b.write_word(vehicle + 0x218, source).unwrap();
        b.write_word(vehicle + 0x21c, source + 4 * 52).unwrap();
        for (i, kind) in [2, 1, 2, 3].into_iter().enumerate() {
            let record = source + i as u32 * 52;
            for o in (0..52).step_by(4) {
                b.write_word(record + o, 0).unwrap();
            }
            b.write_word(record, i as u32 + 10).unwrap();
            b.write_word(record + 4, 0x9000 + i as u32).unwrap();
            b.write_word(record + 12, kind).unwrap();
        }
        assert_eq!(vehicle_prepare_deform(&mut b, vehicle, stack), Ok(false));
        assert_eq!(b.calls, 1);
        let kinds = (0..4)
            .map(|i| b.read_word(source + i * 52 + 12).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(kinds, [3, 1, 2, 2]);
        assert_eq!(b.read_word(stack + 0x20), Ok(13));
        assert_eq!(b.read_word(stack + 0x24), Ok(0));
        assert_eq!(b.read_word(stack + 0x2c), Ok(3));
        assert_eq!(b.read_word(vehicle + 0x208), Ok(0xa5a5a5a5));
    }
}
