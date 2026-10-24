//! Original plain-glass fragment merge, PC5605c0, over its live entry.
use crate::break_plain_glass::{
    glass_append_pointer, glass_read_vector, glass_write_vector, BreakPlainGlassBackend,
    GlassError, GlassPendingResource,
};
use crate::glass_subdivision::{glass_rotate_normal, glass_transform_position, GlassVertex};
use crate::native_transform::SMatrix;
use crate::ph_thing_response::{ph_thing_wide_div, PhThingNumber as R};
use crate::vehicle_init::VehicleInitBackend;

pub trait GlassMergeBackend: BreakPlainGlassBackend {
    fn glass_merge_material(&mut self, frame: u32) -> Result<u32, Self::Error>;
    fn glass_merge_read_triangle(&mut self, frame: u32) -> Result<[GlassVertex; 3], Self::Error>;
    fn glass_merge_begin_group(
        &mut self,
        material: u32,
        faces: &[[u16; 3]],
    ) -> Result<u32, Self::Error>;
    fn glass_merge_install(
        &mut self,
        frame: u32,
        group: u32,
        vertices: &[GlassVertex],
    ) -> Result<bool, Self::Error>;
}
impl<E> From<&'static str> for GlassError<E> {
    fn from(error: &'static str) -> Self {
        Self::NumericDomain(error)
    }
}
type Out<B, T> = Result<T, GlassError<<B as VehicleInitBackend>::Error>>;
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u32> {
    b.read_word(a).map_err(GlassError::Host)
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, value: u32) -> Out<B, ()> {
    b.write_word(a, value).map_err(GlassError::Host)
}
fn address<E>(a: u32, offset: u32) -> Result<u32, GlassError<E>> {
    a.checked_add(offset).ok_or(GlassError::StorageRange)
}
fn allocate<B: BreakPlainGlassBackend>(b: &mut B, bytes: u32) -> Out<B, u32> {
    let allocation = b.allocate(bytes).map_err(GlassError::Host)?;
    if allocation == 0 {
        return Err(GlassError::AllocationFailed);
    }
    b.glass_track(GlassPendingResource::Allocation(allocation));
    Ok(allocation)
}
fn free<B: BreakPlainGlassBackend>(b: &mut B, allocation: u32) -> Out<B, ()> {
    b.free(allocation).map_err(GlassError::Host)?;
    if allocation != 0 {
        b.glass_untrack(GlassPendingResource::Allocation(allocation))
            .map_err(GlassError::Host)?;
    }
    Ok(())
}

// Caller-local vector header; its pointer payload and capacity use the same
// source allocator as the entry. The original header itself lives on stack.
#[derive(Default)]
struct Parents {
    begin: u32,
    length: u32,
    capacity: u32,
}
impl Parents {
    fn append<B: BreakPlainGlassBackend>(&mut self, b: &mut B, source: u32) -> Out<B, ()> {
        if self.length == self.capacity {
            let capacity = self
                .length
                .checked_add(self.length.max(1))
                .ok_or(GlassError::StorageRange)?;
            let next = allocate(b, capacity.checked_mul(4).ok_or(GlassError::StorageRange)?)?;
            for i in 0..self.length {
                let value = word(b, address(self.begin, i * 4)?)?;
                put(b, address(next, i * 4)?, value)?;
            }
            let parent = word(b, source)?;
            put(b, address(next, self.length * 4)?, parent)?;
            free(b, self.begin)?;
            self.begin = next;
            self.capacity = capacity;
        } else {
            let parent = word(b, source)?;
            put(b, address(self.begin, self.length * 4)?, parent)?;
        }
        self.length += 1;
        Ok(())
    }
}

fn fragment<B: VehicleInitBackend>(b: &mut B, entry: u32, index: u32) -> Out<B, u32> {
    let rows = word(b, address(entry, 0x10)?)?;
    let offset = index.checked_mul(0x34).ok_or(GlassError::StorageRange)?;
    word(b, address(rows, offset)?)
}
fn matrix<B: BreakPlainGlassBackend>(b: &mut B, frame: u32) -> Out<B, SMatrix> {
    if word(b, address(frame, 0xac)?)? & 0x20 == 0 {
        b.glass_update_world(frame).map_err(GlassError::Host)?;
    }
    let mut out = SMatrix::identity();
    for (i, value) in out.elements.iter_mut().enumerate() {
        *value = f32::from_bits(word(b, address(frame, 0x10 + i as u32 * 4)?)?);
    }
    Ok(out)
}
fn store_vertex<B: VehicleInitBackend>(b: &mut B, target: u32, v: GlassVertex) -> Out<B, ()> {
    for (i, value) in v
        .position
        .into_iter()
        .chain(v.normal)
        .chain(v.uv)
        .enumerate()
    {
        put(b, address(target, i as u32 * 4)?, value.to_bits())?;
    }
    Ok(())
}

/// Groups fragments by their current parent, builds one visible merged mesh
/// per parent, then unlinks/releases the consumed fragment frames in order.
pub fn merge_glass_fragments<B: GlassMergeBackend>(b: &mut B, entry: u32) -> Out<B, bool> {
    let count = word(b, address(entry, 0x14)?)?;
    if count == 0 {
        // The original dereferences its first fragment unconditionally.
        return Err(GlassError::StorageRange);
    }
    let mut parents = Parents::default();
    for index in (0..count).rev() {
        let frame = fragment(b, entry, index)?;
        let parent = word(b, address(frame, 0x120)?)?;
        let mut found = false;
        for index in (0..parents.length).rev() {
            if word(b, address(parents.begin, index * 4)?)? == parent {
                found = true;
                break;
            }
        }
        if !found {
            parents.append(b, address(frame, 0x120)?)?;
        }
    }
    let first = fragment(b, entry, 0)?;
    let material = b.glass_merge_material(first).map_err(GlassError::Host)?;
    for parent_index in (0..parents.length).rev() {
        let merged = b.glass_new_frame().map_err(GlassError::Host)?;
        if merged == 0 {
            free(b, parents.begin)?;
            return Ok(false);
        }
        b.glass_track(GlassPendingResource::Frame(merged));
        let parent = word(b, address(parents.begin, parent_index * 4)?)?;
        b.glass_frame_virtual(merged, 0x2c, &[parent, 0])
            .map_err(GlassError::Host)?;
        let mut sum = [R::load(0.)?; 3];
        let mut matching = 0u32;
        for index in (0..word(b, address(entry, 0x14)?)?).rev() {
            let frame = fragment(b, entry, index)?;
            if word(b, address(frame, 0x120)?)? == parent {
                let position = glass_read_vector(b, address(frame, 0x80)?)?;
                for (sum, value) in sum.iter_mut().zip(position) {
                    *sum = sum.add(R::load(value)?)?;
                }
                matching += 1;
            }
        }
        let z = sum[2].store()?;
        // FILD consumes the full signed count before the PC24 reciprocal;
        // converting the integer to binary32 first can lose its low bits.
        let inverse = ph_thing_wide_div(1., f64::from(matching as i32)) as f32;
        let position = [
            sum[0].mul(R::load(inverse)?)?.store()?,
            sum[1].mul(R::load(inverse)?)?.store()?,
            R::load(z)?.mul(R::load(inverse)?)?.store()?,
        ];
        let flags = word(b, address(merged, 0xac)?)?;
        if flags as u8 & 0x80 == 0 {
            glass_write_vector(b, address(merged, 0x80)?, position)?;
            put(b, address(merged, 0xac)?, flags & 0xfffffedf | 0x40000000)?;
        }
        b.glass_frame_direction(merged, [0., 1., 0.], 0.)
            .map_err(GlassError::Host)?;
        b.glass_frame_virtual(merged, 0x18, &[])
            .map_err(GlassError::Host)?;
        b.glass_place_sector(merged).map_err(GlassError::Host)?;
        let vertex_count = matching.checked_mul(3).ok_or(GlassError::StorageRange)?;
        let mut faces = Vec::new();
        for index in 0..matching {
            let start = index.wrapping_mul(3) as u16;
            faces.push([start.wrapping_add(2), start.wrapping_add(1), start]);
        }
        let group = b
            .glass_merge_begin_group(material, &faces)
            .map_err(GlassError::Host)?;
        if group == 0 {
            free(b, parents.begin)?;
            return Ok(false);
        }
        let temporary = allocate(
            b,
            vertex_count
                .checked_mul(32)
                .ok_or(GlassError::StorageRange)?,
        )?;
        let mut vertices = Vec::new();
        let mut index = 0;
        while index < word(b, address(entry, 0x14)?)? {
            let frame = fragment(b, entry, index)?;
            if word(b, address(frame, 0x120)?)? == parent {
                for vertex in b
                    .glass_merge_read_triangle(frame)
                    .map_err(GlassError::Host)?
                {
                    let offset = u32::try_from(vertices.len())
                        .map_err(|_| GlassError::StorageRange)?
                        .checked_mul(32)
                        .ok_or(GlassError::StorageRange)?;
                    store_vertex(b, address(temporary, offset)?, vertex)?;
                    vertices.push(vertex);
                }
            }
            index += 1;
        }
        let mut vertex_index = 0usize;
        let mut index = 0;
        while index < word(b, address(entry, 0x14)?)? {
            let frame = fragment(b, entry, index)?;
            if word(b, address(frame, 0x120)?)? == parent {
                let world = matrix(b, frame)?;
                for (local_index, vertex) in vertices[vertex_index..vertex_index + 3]
                    .iter_mut()
                    .enumerate()
                {
                    vertex.position = glass_transform_position(vertex.position, &world)?;
                    glass_write_vector(
                        b,
                        address(temporary, (vertex_index + local_index) as u32 * 32)?,
                        vertex.position,
                    )?;
                }
                for vertex in &vertices[vertex_index..vertex_index + 3] {
                    // RotateByMatrix returns a vector in caller scratch. The
                    // original ignores it; unlike positions, normals persist.
                    let _ = glass_rotate_normal(vertex.normal, &world)?;
                }
                let merged_world = matrix(b, merged)?;
                let (inverse, _) = merged_world
                    .inverse()
                    .map_err(|_| GlassError::NumericDomain("merged world inverse"))?;
                for (local_index, vertex) in vertices[vertex_index..vertex_index + 3]
                    .iter_mut()
                    .enumerate()
                {
                    vertex.position = glass_transform_position(vertex.position, &inverse)?;
                    glass_write_vector(
                        b,
                        address(temporary, (vertex_index + local_index) as u32 * 32)?,
                        vertex.position,
                    )?;
                }
                for vertex in &vertices[vertex_index..vertex_index + 3] {
                    let _ = glass_rotate_normal(vertex.normal, &inverse)?;
                }
                vertex_index += 3;
            }
            index += 1;
        }
        let installed = b
            .glass_merge_install(merged, group, &vertices)
            .map_err(GlassError::Host)?;
        free(b, temporary)?;
        if !installed {
            free(b, parents.begin)?;
            return Ok(false);
        }
        b.glass_frame_virtual(merged, 0x78, &[0])
            .map_err(GlassError::Host)?;
        b.glass_frame_virtual(merged, 0x18, &[])
            .map_err(GlassError::Host)?;
        glass_append_pointer(b, entry, merged)?;
        b.glass_untrack(GlassPendingResource::Frame(merged))
            .map_err(GlassError::Host)?;
    }
    let mut index = 0;
    while index < word(b, address(entry, 0x14)?)? {
        let frame = fragment(b, entry, index)?;
        b.glass_frame_virtual(frame, 0x2c, &[0, 0])
            .map_err(GlassError::Host)?;
        let frame = fragment(b, entry, index)?;
        b.glass_frame_virtual(frame, 0, &[])
            .map_err(GlassError::Host)?;
        let rows = word(b, address(entry, 0x10)?)?;
        put(b, address(rows, index * 0x34)?, 0)?;
        index += 1;
    }
    free(b, parents.begin)?;
    Ok(true)
}
