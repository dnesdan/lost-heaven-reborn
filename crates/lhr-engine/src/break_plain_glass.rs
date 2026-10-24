//! Original plain-glass manager resource orchestration (Game PC 0x562170).
//! Temporary geometry is derived from the same live mesh; persistent fragments
//! and manager vectors remain in the caller's canonical frame/heap owners.
use crate::glass_subdivision::{GlassGeometry, GlassVertex};
use crate::native_precision::{add, mul};
use crate::native_transform::SMatrix;
use crate::vehicle_init::VehicleInitBackend;

#[derive(Debug)]
pub enum GlassError<E> {
    Host(E),
    StorageRange,
    AllocationFailed,
    NumericDomain(&'static str),
    Dependency(&'static str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassPendingResource {
    Frame(u32),
    Allocation(u32),
    Material(u32),
}
pub trait BreakPlainGlassBackend: VehicleInitBackend {
    fn glass_frame_virtual(
        &mut self,
        frame: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, Self::Error>;
    fn glass_update_world(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn glass_frame_direction(
        &mut self,
        frame: u32,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), Self::Error>;
    fn glass_new_frame(&mut self) -> Result<u32, Self::Error>;
    fn glass_source_geometry(&mut self, frame: u32) -> Result<GlassGeometry, Self::Error>;
    fn glass_new_material(&mut self) -> Result<u32, Self::Error>;
    fn glass_copy_source_material(&mut self, frame: u32, material: u32) -> Result<(), Self::Error>;
    fn glass_release_material(&mut self, material: u32) -> Result<(), Self::Error>;
    fn glass_begin_fragment(&mut self, material: u32) -> Result<u32, Self::Error>;
    fn glass_cleanup_resources(&mut self) -> Result<(), Self::Error>;
    fn glass_install_fragment(
        &mut self,
        frame: u32,
        group: u32,
        vertices: &[GlassVertex; 6],
    ) -> Result<bool, Self::Error>;
    fn glass_place_sector(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn glass_debustize(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn glass_ground_plane(
        &mut self,
        position: [f32; 3],
    ) -> Result<([f32; 3], [f32; 3]), Self::Error>;
    fn glass_play_sound(&mut self, name: u32, position: [f32; 3]) -> Result<(), Self::Error>;
    fn glass_test_line(
        &mut self,
        origin: [f32; 3],
        delta: [f32; 3],
        selector: u32,
        flags: u32,
        point: &mut [f32; 3],
        normal: &mut [f32; 3],
    ) -> Result<u32, Self::Error>;
    fn glass_merge_fragments(&mut self, entry: u32) -> Result<bool, Self::Error>;
    fn glass_allocator_marker(&self) -> u8;
    fn glass_track(&mut self, resource: GlassPendingResource);
    fn glass_untrack(&mut self, resource: GlassPendingResource) -> Result<(), Self::Error>;
    fn glass_pending_last(&self) -> Option<GlassPendingResource>;
}

pub fn glass_read_float<B: VehicleInitBackend>(
    b: &mut B,
    address: u32,
) -> Result<f32, GlassError<B::Error>> {
    b.read_word(address)
        .map(f32::from_bits)
        .map_err(GlassError::Host)
}
pub fn glass_write_float<B: VehicleInitBackend>(
    b: &mut B,
    address: u32,
    value: f32,
) -> Result<(), GlassError<B::Error>> {
    b.write_word(address, value.to_bits())
        .map_err(GlassError::Host)
}
pub fn glass_read_vector<B: VehicleInitBackend>(
    b: &mut B,
    address: u32,
) -> Result<[f32; 3], GlassError<B::Error>> {
    Ok([
        glass_read_float(b, address)?,
        glass_read_float(b, address + 4)?,
        glass_read_float(b, address + 8)?,
    ])
}
pub fn glass_write_vector<B: VehicleInitBackend>(
    b: &mut B,
    address: u32,
    vector: [f32; 3],
) -> Result<(), GlassError<B::Error>> {
    for (i, value) in vector.into_iter().enumerate() {
        glass_write_float(b, address + i as u32 * 4, value)?;
    }
    Ok(())
}
pub fn glass_random<B: VehicleInitBackend>(b: &mut B) -> Result<i32, GlassError<B::Error>> {
    let old = b.read_word(0x63a200).map_err(GlassError::Host)?;
    let next = old.wrapping_mul(214013).wrapping_add(2531011);
    b.write_word(0x63a200, next).map_err(GlassError::Host)?;
    Ok(((next >> 16) & 0x7fff) as i32)
}
fn world_matrix<B: BreakPlainGlassBackend>(
    b: &mut B,
    frame: u32,
) -> Result<SMatrix, GlassError<B::Error>> {
    b.glass_update_world(frame).map_err(GlassError::Host)?;
    let mut matrix = SMatrix::identity();
    for (i, word) in matrix.elements.iter_mut().enumerate() {
        *word = glass_read_float(b, frame + 0x10 + i as u32 * 4)?;
    }
    Ok(matrix)
}

fn allocated<B: BreakPlainGlassBackend>(
    b: &mut B,
    bytes: u32,
) -> Result<u32, GlassError<B::Error>> {
    let p = b.allocate(bytes).map_err(GlassError::Host)?;
    if p == 0 {
        return Err(GlassError::AllocationFailed);
    }
    b.glass_track(GlassPendingResource::Allocation(p));
    let fill = b.glass_allocator_marker();
    for i in 0..bytes {
        b.write_byte(p + i, fill).map_err(GlassError::Host)?;
    }
    Ok(p)
}
fn free_temporary<B: BreakPlainGlassBackend>(
    b: &mut B,
    p: u32,
) -> Result<(), GlassError<B::Error>> {
    b.free(p).map_err(GlassError::Host)?;
    b.glass_untrack(GlassPendingResource::Allocation(p))
        .map_err(GlassError::Host)
}
fn count<B: VehicleInitBackend>(
    b: &mut B,
    header: u32,
    stride: u32,
) -> Result<(u32, u32, u32), GlassError<B::Error>> {
    let start = b.read_word(header + 4).map_err(GlassError::Host)?;
    let end = b.read_word(header + 8).map_err(GlassError::Host)?;
    let cap = b.read_word(header + 12).map_err(GlassError::Host)?;
    if start == 0 {
        return if end == 0 && cap == 0 {
            Ok((0, 0, 0))
        } else {
            Err(GlassError::StorageRange)
        };
    }
    if end < start
        || cap < end
        || !(end - start).is_multiple_of(stride)
        || !(cap - start).is_multiple_of(stride)
    {
        return Err(GlassError::StorageRange);
    }
    Ok((start, (end - start) / stride, (cap - start) / stride))
}
/// Pointer-vector append4824c0, reached at its end iterator.
pub fn glass_append_pointer<B: VehicleInitBackend>(
    b: &mut B,
    header: u32,
    value: u32,
) -> Result<(), GlassError<B::Error>> {
    let (old, len, capacity) = count(b, header, 4)?;
    if len < capacity {
        b.write_word(old + len * 4, value)
            .map_err(GlassError::Host)?;
        return b
            .write_word(header + 8, old + (len + 1) * 4)
            .map_err(GlassError::Host);
    }
    let next_capacity = len
        .checked_add(len.max(1))
        .ok_or(GlassError::StorageRange)?;
    let next = b
        .allocate(
            next_capacity
                .checked_mul(4)
                .ok_or(GlassError::StorageRange)?,
        )
        .map_err(GlassError::Host)?;
    if next == 0 {
        return Err(GlassError::AllocationFailed);
    }
    for i in 0..len {
        let word = b.read_word(old + i * 4).map_err(GlassError::Host)?;
        b.write_word(next + i * 4, word).map_err(GlassError::Host)?;
    }
    b.write_word(next + len * 4, value)
        .map_err(GlassError::Host)?;
    b.free(old).map_err(GlassError::Host)?;
    for (offset, word) in [
        (4, next),
        (8, next + (len + 1) * 4),
        (12, next + next_capacity * 4),
    ] {
        b.write_word(header + offset, word)
            .map_err(GlassError::Host)?;
    }
    Ok(())
}
/// Descriptor copy-constructor564770: deep pointer-vector storage, shallow
/// fragment-array identity/count. It does not AddRef each borrowed scalar.
pub fn glass_copy_entry<B: VehicleInitBackend>(
    b: &mut B,
    destination: u32,
    source: u32,
) -> Result<(), GlassError<B::Error>> {
    let marker = b.read_byte(source).map_err(GlassError::Host)?;
    b.write_byte(destination, marker)
        .map_err(GlassError::Host)?;
    let (start, len, _) = count(b, source, 4)?;
    let next = b
        .allocate(len.checked_mul(4).ok_or(GlassError::StorageRange)?)
        .map_err(GlassError::Host)?;
    if next == 0 && len != 0 {
        return Err(GlassError::AllocationFailed);
    }
    b.write_word(destination + 4, next)
        .map_err(GlassError::Host)?;
    for i in 0..len {
        let word = b.read_word(start + i * 4).map_err(GlassError::Host)?;
        b.write_word(next + i * 4, word).map_err(GlassError::Host)?;
    }
    for (offset, word) in [(8, next + len * 4), (12, next + len * 4)] {
        b.write_word(destination + offset, word)
            .map_err(GlassError::Host)?;
    }
    for offset in [0x10, 0x14] {
        let word = b.read_word(source + offset).map_err(GlassError::Host)?;
        b.write_word(destination + offset, word)
            .map_err(GlassError::Host)?;
    }
    Ok(())
}
/// Original5640d0 end insertion with count1, including descriptor copies when
/// its vector reallocates. Other insertion positions are not this caller ABI.
pub fn glass_append_entry<B: VehicleInitBackend>(
    b: &mut B,
    header: u32,
    entry: u32,
) -> Result<(), GlassError<B::Error>> {
    let (old, len, capacity) = count(b, header, 24)?;
    if len < capacity {
        glass_copy_entry(b, old + len * 24, entry)?;
        return b
            .write_word(header + 8, old + (len + 1) * 24)
            .map_err(GlassError::Host);
    }
    let cap = len
        .checked_add(len.max(1))
        .ok_or(GlassError::StorageRange)?;
    let next = b
        .allocate(cap.checked_mul(24).ok_or(GlassError::StorageRange)?)
        .map_err(GlassError::Host)?;
    if next == 0 {
        return Err(GlassError::AllocationFailed);
    }
    for i in 0..len {
        glass_copy_entry(b, next + i * 24, old + i * 24)?;
    }
    glass_copy_entry(b, next + len * 24, entry)?;
    for i in 0..len {
        let vector = b.read_word(old + i * 24 + 4).map_err(GlassError::Host)?;
        b.free(vector).map_err(GlassError::Host)?;
    }
    b.free(old).map_err(GlassError::Host)?;
    for (offset, word) in [(4, next), (8, next + (len + 1) * 24), (12, next + cap * 24)] {
        b.write_word(header + offset, word)
            .map_err(GlassError::Host)?;
    }
    Ok(())
}

fn normalized(vector: [f32; 3]) -> Result<[f32; 3], &'static str> {
    use crate::ph_thing_response::PhThingNumber as R;
    let load = R::load;
    let [x, y, z] = vector;
    let square = load(z)?
        .mul(load(z)?)?
        .add(load(y)?.mul(load(y)?)?)?
        .add(load(x)?.mul(load(x)?)?)?;
    let spilled = square.store()?;
    let mut result = vector;
    if square.sub(load(1.)?)?.value().abs() >= f64::from(1e-8_f32) {
        if spilled >= 1e-8 {
            let inverse = load(1.)?.div(load(spilled)?.sqrt()?)?;
            for value in &mut result {
                *value = load(*value)?.mul(inverse)?.store()?;
            }
        } else {
            let i = if x != 0. {
                0
            } else if z != 0. {
                2
            } else {
                1
            };
            result[i] = if result[i] < 0. { -1. } else { 1. };
        }
    }
    Ok(result)
}
/// Original563610's centroid/direction and six-vertex double-sided shard.
pub fn glass_fragment<B: BreakPlainGlassBackend>(
    b: &mut B,
    frame: u32,
    material: u32,
    input: [GlassVertex; 3],
) -> Result<bool, GlassError<B::Error>> {
    let group = b.glass_begin_fragment(material).map_err(GlassError::Host)?;
    use crate::ph_thing_response::PhThingNumber as R;
    let load = R::load;
    let numeric = GlassError::NumericDomain;
    let mut center = [0.; 3];
    for (axis, value) in center.iter_mut().enumerate() {
        *value = load(input[1].position[axis])
            .map_err(numeric)?
            .add(load(input[0].position[axis]).map_err(numeric)?)
            .map_err(numeric)?
            .add(load(input[2].position[axis]).map_err(numeric)?)
            .map_err(numeric)?
            .mul(load(f32::from_bits(0x3eaa_aaab)).map_err(numeric)?)
            .map_err(numeric)?
            .store()
            .map_err(numeric)?;
    }
    let zero = load(0.).map_err(numeric)?;
    let mut a = [zero; 3];
    let mut edge = [zero; 3];
    for i in 0..3 {
        a[i] = load(input[0].position[i])
            .map_err(numeric)?
            .sub(load(input[1].position[i]).map_err(numeric)?)
            .map_err(numeric)?;
        if i < 2 {
            a[i] = load(a[i].store().map_err(numeric)?).map_err(numeric)?;
        }
        edge[i] = load(input[2].position[i])
            .map_err(numeric)?
            .sub(load(input[1].position[i]).map_err(numeric)?)
            .map_err(numeric)?;
    }
    let cross = [
        a[1].mul(edge[2])
            .map_err(numeric)?
            .sub(a[2].mul(edge[1]).map_err(numeric)?)
            .map_err(numeric)?
            .store()
            .map_err(numeric)?,
        a[2].mul(edge[0])
            .map_err(numeric)?
            .sub(a[0].mul(edge[2]).map_err(numeric)?)
            .map_err(numeric)?
            .store()
            .map_err(numeric)?,
        a[0].mul(edge[1])
            .map_err(numeric)?
            .sub(a[1].mul(edge[0]).map_err(numeric)?)
            .map_err(numeric)?
            .store()
            .map_err(numeric)?,
    ];
    let normal = normalized(cross).map_err(numeric)?;
    let flags = b.read_word(frame + 0xac).map_err(GlassError::Host)?;
    if flags & 0x80 == 0 {
        glass_write_vector(b, frame + 0x80, center)?;
        b.write_word(frame + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)
            .map_err(GlassError::Host)?;
    }
    b.glass_frame_direction(frame, normal, 0.)
        .map_err(GlassError::Host)?;
    let world = world_matrix(b, frame)?;
    let (inverse, _) = world
        .inverse()
        .map_err(|_| GlassError::NumericDomain("glass fragment inverse"))?;
    let mut front = input;
    for vertex in &mut front {
        vertex.position =
            crate::glass_subdivision::glass_transform_position(vertex.position, &inverse)
                .map_err(numeric)?;
        // The original RotateByMatrix writes its separate stack scratch
        // destination; source vertex normals remain unchanged.
        let _scratch_normal =
            crate::glass_subdivision::glass_rotate_normal(vertex.normal, &inverse)
                .map_err(numeric)?;
    }
    let mut vertices = [front[0], front[1], front[2], front[0], front[2], front[1]];
    for vertex in &mut vertices[3..] {
        vertex.normal = vertex.normal.map(|v| mul(v, -1.));
    }
    b.glass_install_fragment(frame, group, &vertices)
        .map_err(GlassError::Host)
}

pub fn break_plain_glass<B: BreakPlainGlassBackend>(
    b: &mut B,
    manager: u32,
    frame: u32,
    direction: Option<[f32; 3]>,
    sound_name: u32,
    retain_original: bool,
) -> Result<(), GlassError<B::Error>> {
    if b.read_word(frame + 0x110).map_err(GlassError::Host)? != 1
        || b.read_word(frame + 0x1d4).map_err(GlassError::Host)? != 0x5f4a424f
    {
        return Ok(());
    }
    if retain_original {
        glass_append_pointer(b, manager, frame)?;
        let reference = b.read_word(frame + 4).map_err(GlassError::Host)?;
        b.write_word(frame + 4, reference.wrapping_add(1))
            .map_err(GlassError::Host)?;
    }
    b.glass_frame_virtual(frame, 0x24, &[0])
        .map_err(GlassError::Host)?;
    let world = world_matrix(b, frame)?;
    let position = [world.elements[12], world.elements[13], world.elements[14]];
    let (mut point, normal) = b.glass_ground_plane(position).map_err(GlassError::Host)?;
    point[1] = add(point[1], 0.05);
    let material = b.glass_new_material().map_err(GlassError::Host)?;
    b.glass_track(GlassPendingResource::Material(material));
    b.glass_copy_source_material(frame, material)
        .map_err(GlassError::Host)?;
    let source = b.glass_source_geometry(frame).map_err(GlassError::Host)?;
    let geometry = crate::glass_subdivision::subdivide_glass(&source, &world, || glass_random(b))
        .map_err(|error| match error {
        crate::glass_subdivision::GlassSubdivisionError::Random(error) => error,
        _ => GlassError::NumericDomain("glass subdivision"),
    })?;
    let size = u32::try_from(geometry.faces.len()).map_err(|_| GlassError::StorageRange)?;
    let fragments = allocated(b, size.checked_mul(0x34).ok_or(GlassError::StorageRange)?)?;
    let entry = allocated(b, 24)?;
    let marker = b.glass_allocator_marker();
    b.write_byte(entry, marker).map_err(GlassError::Host)?;
    for offset in [4, 8, 12] {
        b.write_word(entry + offset, 0).map_err(GlassError::Host)?;
    }
    b.write_word(entry + 0x10, fragments)
        .map_err(GlassError::Host)?;
    b.write_word(entry + 0x14, size).map_err(GlassError::Host)?;
    let app = b.read_word(0x63788c).map_err(GlassError::Host)?;
    let scene = b.read_word(app + 0x10).map_err(GlassError::Host)?;
    let sector = b.read_word(scene + 0x210).map_err(GlassError::Host)?;
    let mut frames = Vec::with_capacity(size as usize);
    for i in (0..size).rev() {
        let piece = b.glass_new_frame().map_err(GlassError::Host)?;
        if piece == 0 {
            return Err(GlassError::AllocationFailed);
        }
        b.glass_track(GlassPendingResource::Frame(piece));
        b.glass_frame_virtual(piece, 0x2c, &[sector, 0])
            .map_err(GlassError::Host)?;
        b.glass_frame_virtual(piece, 0x18, &[])
            .map_err(GlassError::Host)?;
        let face = geometry.faces[i as usize];
        let mut input = [geometry.vertices[0]; 3];
        for (n, index) in face.into_iter().enumerate() {
            input[n] = *geometry
                .vertices
                .get(index as usize)
                .ok_or(GlassError::StorageRange)?;
        }
        if !glass_fragment(b, piece, material, input)? {
            return Err(GlassError::Dependency("glass shard resource admission"));
        }
        let row = fragments + i * 0x34;
        b.write_word(row, piece).map_err(GlassError::Host)?;
        let velocity = if let Some(vector) = direction {
            let gain = mul((glass_random(b)? % 100 - 20) as f32, 0.01);
            vector.map(|value| mul(value, gain))
        } else {
            [0.; 3]
        };
        glass_write_vector(b, row + 4, velocity)?;
        glass_write_float(b, row + 0x10, 0.)?;
        let angular_speed = mul((glass_random(b)? % 120 + 20) as f32, 0.1);
        glass_write_float(b, row + 0x14, angular_speed)?;
        glass_write_vector(b, row + 0x18, point)?;
        glass_write_vector(b, row + 0x24, normal)?;
        for offset in [0x30, 0x31, 0x32] {
            b.write_byte(row + offset, 1).map_err(GlassError::Host)?;
        }
        b.glass_debustize(piece).map_err(GlassError::Host)?;
        b.glass_frame_virtual(piece, 0x24, &[1])
            .map_err(GlassError::Host)?;
        b.glass_place_sector(piece).map_err(GlassError::Host)?;
        b.glass_frame_virtual(piece, 0x18, &[])
            .map_err(GlassError::Host)?;
        frames.push(piece);
    }
    b.glass_release_material(material)
        .map_err(GlassError::Host)?;
    b.glass_untrack(GlassPendingResource::Material(material))
        .map_err(GlassError::Host)?;
    glass_append_entry(b, manager + 0x10, entry)?;
    for frame in frames {
        b.glass_untrack(GlassPendingResource::Frame(frame))
            .map_err(GlassError::Host)?;
    }
    b.glass_untrack(GlassPendingResource::Allocation(fragments))
        .map_err(GlassError::Host)?;
    // The local descriptor owns no borrowed-frame vector; its copy constructor
    // allocated the destination's (possibly empty) vector independently.
    free_temporary(b, entry)?;
    b.glass_update_world(frame).map_err(GlassError::Host)?;
    let position = glass_read_vector(b, frame + 0x40)?;
    b.glass_play_sound(sound_name, position)
        .map_err(GlassError::Host)?;
    Ok(())
}

pub fn cleanup_glass_pending<B: BreakPlainGlassBackend>(
    b: &mut B,
) -> Result<(), GlassError<B::Error>> {
    b.glass_cleanup_resources().map_err(GlassError::Host)?;
    while let Some(resource) = b.glass_pending_last() {
        match resource {
            GlassPendingResource::Frame(frame) => {
                b.glass_frame_virtual(frame, 0, &[])
                    .map_err(GlassError::Host)?;
            }
            GlassPendingResource::Allocation(p) => {
                b.free(p).map_err(GlassError::Host)?;
            }
            GlassPendingResource::Material(material) => {
                b.glass_release_material(material)
                    .map_err(GlassError::Host)?;
            }
        }
        b.glass_untrack(resource).map_err(GlassError::Host)?;
    }
    Ok(())
}
