//! Per-record geometry and health pass of PC C_Vehicle::DoDamage,
//! 0x4d58a0..0x4d633f.
//! The original/deformed mesh addresses refer to their existing visual owners.
use super::{DamageVector, VehicleDamageCall};
use crate::native_precision::{add, div, mul, sub};
use crate::native_transform::SMatrix;
use crate::vehicle_init::VehicleInitBackend;

pub trait VehicleDamageMeshBackend: VehicleInitBackend {
    fn damage_virtual(&mut self, object: u32, slot: u32, args: &[u32]) -> Result<u32, Self::Error>;
    fn damage_update_world(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn damage_rand(&mut self) -> Result<i32, Self::Error>;
    #[allow(clippy::too_many_arguments)]
    fn damage_notification(
        &mut self,
        vehicle: u32,
        function: u32,
        context: u32,
        frame: u32,
        part_type: u16,
        point: [f32; 3],
        impulse: [f32; 3],
        changed: bool,
    ) -> Result<(), Self::Error>;
    fn damage_break_glass(
        &mut self,
        vehicle: u32,
        index: u32,
        direction: [f32; 3],
    ) -> Result<(), Self::Error>;
    #[allow(clippy::too_many_arguments)]
    fn damage_detach_part(
        &mut self,
        vehicle: u32,
        function: u32,
        context: u32,
        index: u32,
        velocity: [f32; 3],
        extra: u32,
    ) -> Result<(), Self::Error>;
    fn damage_lamp_switch(
        &mut self,
        vehicle: u32,
        index: u32,
        side: u32,
    ) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum DamageMeshError<E> {
    Host(E),
    NumericDomain,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DamageMeshResult {
    pub changed: [bool; 2],
}
type Out<B, T> = Result<T, DamageMeshError<<B as VehicleInitBackend>::Error>>;
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u32> {
    b.read_word(a).map_err(DamageMeshError::Host)
}
fn number<E>(x: f32) -> Result<f32, DamageMeshError<E>> {
    crate::vehicle_dynamics_prepare::number::<()>(x).map_err(|_| DamageMeshError::NumericDomain)
}
fn float<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, f32> {
    number(f32::from_bits(word(b, a)?))
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, x: f32) -> Out<B, ()> {
    let x = number(x)?;
    b.write_word(a, x.to_bits()).map_err(DamageMeshError::Host)
}
fn vector<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, [f32; 3]> {
    Ok([float(b, a)?, float(b, a + 4)?, float(b, a + 8)?])
}
fn virtual_call<B: VehicleDamageMeshBackend>(b: &mut B, o: u32, s: u32, a: &[u32]) -> Out<B, u32> {
    b.damage_virtual(o, s, a).map_err(DamageMeshError::Host)
}
fn limit(x: f32, maximum: f32) -> f32 {
    if maximum < x.abs() {
        if x > 0.0 {
            maximum
        } else {
            -maximum
        }
    } else {
        x
    }
}
struct Geometry {
    point: [f32; 3],
    direction: [f32; 3],
    squared_radius: f32,
    factor: f32,
}
fn level<B: VehicleDamageMeshBackend>(
    b: &mut B,
    v: u32,
    row_offset: u32,
    source: u32,
    destination: u32,
    lod: usize,
    geometry: &Geometry,
) -> Out<B, bool> {
    let original = virtual_call(b, source, 4, &[0])?;
    let vertices = virtual_call(b, destination, 4, &[0])?;
    let changed = deform_vertices(
        b,
        v,
        row_offset,
        destination,
        original,
        vertices,
        lod,
        geometry,
    )?;
    virtual_call(b, source, 8, &[])?;
    virtual_call(b, destination, 8, &[])?;
    Ok(changed)
}
#[allow(clippy::too_many_arguments)]
fn deform_vertices<B: VehicleDamageMeshBackend>(
    b: &mut B,
    v: u32,
    row_offset: u32,
    destination: u32,
    original: u32,
    vertices: u32,
    lod: usize,
    geometry: &Geometry,
) -> Out<B, bool> {
    let count = word(b, destination + 8)?;
    let mut changed = false;
    for i in (0..count).rev() {
        let target = vertices.wrapping_add(i.wrapping_mul(32));
        let reference = original.wrapping_add(i.wrapping_mul(32));
        let current = vector(b, target)?;
        let delta = std::array::from_fn::<_, 3, _>(|axis| sub(current[axis], geometry.point[axis]));
        // The two native loops deliberately associate their distances differently.
        let squared = if lod == 0 {
            add(
                add(mul(delta[0], delta[0]), mul(delta[2], delta[2])),
                mul(delta[1], delta[1]),
            )
        } else {
            add(
                add(mul(delta[2], delta[2]), mul(delta[1], delta[1])),
                mul(delta[0], delta[0]),
            )
        };
        let squared = number(squared)?;
        if squared < geometry.squared_radius {
            changed = true;
            let random = b.damage_rand().map_err(DamageMeshError::Host)?;
            let variance = add(mul(mul((random % 10) as f32, 0.1), 0.3), 0.7);
            // Native stores the displaced x/y/z before reading the reference
            // buffer and applying the accumulated-displacement clamp.
            for (axis, coordinate) in current.into_iter().enumerate() {
                let displacement = mul(mul(geometry.direction[axis], variance), geometry.factor);
                put(b, target + axis as u32 * 4, add(displacement, coordinate))?;
            }
            let current = vector(b, target)?;
            let original = vector(b, reference)?;
            let row = word(b, v + 0x218)?.wrapping_add(row_offset);
            let maxima = vector(b, row + 0x28)?;
            let offsets: [f32; 3] =
                std::array::from_fn(|axis| limit(sub(current[axis], original[axis]), maxima[axis]));
            // All final values are evaluated before the native x/y/z stores.
            let result: [f32; 3] = std::array::from_fn(|axis| add(offsets[axis], original[axis]));
            for (axis, coordinate) in result.into_iter().enumerate() {
                put(b, target + axis as u32 * 4, coordinate)?;
            }
        }
    }
    Ok(changed)
}
/// Called after the outer row flags/threshold checks and effective-strength
/// calculation. `point` and `direction` are the current original call operands.
#[allow(clippy::too_many_arguments)]
pub fn vehicle_damage_mesh_record<B: VehicleDamageMeshBackend>(
    b: &mut B,
    v: u32,
    index: u32,
    point: [f32; 3],
    direction: [f32; 3],
    radius: f32,
    factor: f32,
) -> Out<B, DamageMeshResult> {
    record(
        b,
        v,
        index,
        DamageVector::Value(point),
        DamageVector::Value(direction),
        radius,
        factor,
    )
}
#[allow(clippy::too_many_arguments)]
fn record<B: VehicleDamageMeshBackend>(
    b: &mut B,
    v: u32,
    index: u32,
    point: DamageVector,
    direction: DamageVector,
    radius: f32,
    factor: f32,
) -> Out<B, DamageMeshResult> {
    let offset = index.wrapping_mul(0x34);
    let source_rows = word(b, v + 0x208)?;
    let source_mesh = word(b, source_rows.wrapping_add(offset).wrapping_add(4))?;
    let source = virtual_call(b, source_mesh, 4, &[0])?;
    let rows = word(b, v + 0x218)?;
    let mesh = word(b, rows.wrapping_add(offset).wrapping_add(4))?;
    let destination = virtual_call(b, mesh, 4, &[0])?;
    // Locks precede world-matrix preparation in the original call order.
    let original = virtual_call(b, source, 4, &[0])?;
    let vertices = virtual_call(b, destination, 4, &[0])?;
    let rows = word(b, v + 0x218)?;
    let frame = word(b, rows.wrapping_add(offset))?;
    let frame_flags = word(b, frame + 0xac)?;
    if frame_flags & 0x20 == 0 {
        b.damage_update_world(frame)
            .map_err(DamageMeshError::Host)?;
    }
    let mut elements = [0.0; 16];
    for (i, value) in elements.iter_mut().enumerate() {
        *value = float(b, frame + 0x10 + i as u32 * 4)?;
    }
    let (mut inverse, _) = SMatrix { elements }
        .inverse()
        .map_err(|_| DamageMeshError::NumericDomain)?;
    let point_value = operand(b, point)?;
    let point = inverse
        .try_transform_point(point_value)
        .map_err(|_| DamageMeshError::NumericDomain)?;
    inverse.elements[12..15].fill(0.0);
    let direction_value = operand(b, direction)?;
    let mut direction = inverse
        .try_transform_point(direction_value)
        .map_err(|_| DamageMeshError::NumericDomain)?;
    let rows = word(b, v + 0x218)?;
    let row = rows.wrapping_add(offset);
    for (axis, value) in direction.iter_mut().enumerate() {
        *value = limit(*value, float(b, row + 0x28 + axis as u32 * 4)?);
    }
    let factor = number(factor)?;
    let squared_radius = number(mul(radius, radius))?;
    let geometry = Geometry {
        point,
        direction,
        squared_radius,
        factor,
    };
    // Reuse the already acquired locks; calling Lock again is not an original
    // operation (and a backend may expose its cached-lock state).
    let changed0 = locked_level(
        b,
        v,
        offset,
        source,
        destination,
        original,
        vertices,
        &geometry,
    )?;
    let mut changed = [changed0, false];
    let source_rows = word(b, v + 0x208)?;
    let source_mesh = word(b, source_rows.wrapping_add(offset).wrapping_add(4))?;
    if word(b, source_mesh + 8)? > 1 {
        let source = virtual_call(b, source_mesh, 4, &[1])?;
        let rows = word(b, v + 0x218)?;
        let mesh = word(b, rows.wrapping_add(offset).wrapping_add(4))?;
        let destination = virtual_call(b, mesh, 4, &[1])?;
        if source != 0 && destination != 0 {
            changed[1] = level(b, v, offset, source, destination, 1, &geometry)?;
        }
    }
    Ok(DamageMeshResult { changed })
}

// The first loop has locks acquired before the matrix operations. Keeping that
// prefix outside the common vertex loop preserves native failure side effects.
#[allow(clippy::too_many_arguments)]
fn locked_level<B: VehicleDamageMeshBackend>(
    b: &mut B,
    v: u32,
    offset: u32,
    source: u32,
    destination: u32,
    original: u32,
    vertices: u32,
    g: &Geometry,
) -> Out<B, bool> {
    let changed = deform_vertices(b, v, offset, destination, original, vertices, 0, g)?;
    virtual_call(b, source, 8, &[])?;
    virtual_call(b, destination, 8, &[])?;
    Ok(changed)
}
/// Publication follows the row's optional damage notification, including the
/// type-2 strength replacement; the caller controls that intervening ordering.
pub fn vehicle_damage_publish_mesh_record<B: VehicleDamageMeshBackend>(
    b: &mut B,
    v: u32,
    index: u32,
    changed: [bool; 2],
) -> Out<B, ()> {
    if !changed[0] && !changed[1] {
        return Ok(());
    }
    let flags = word(b, v + 0xf8)?;
    b.write_word(v + 0xf8, flags | 1)
        .map_err(DamageMeshError::Host)?;
    let offset = index.wrapping_mul(0x34);
    for (lod, altered) in changed.into_iter().enumerate() {
        if altered {
            let rows = word(b, v + 0x218)?;
            let mesh = word(b, rows.wrapping_add(offset).wrapping_add(4))?;
            let level = virtual_call(b, mesh, 4, &[lod as u32])?;
            virtual_call(b, level, 0x28, &[])?;
        }
    }
    let rows = word(b, v + 0x218)?;
    let mesh = word(b, rows.wrapping_add(offset).wrapping_add(4))?;
    virtual_call(b, mesh, 0x14, &[])?;
    let rows = word(b, v + 0x218)?;
    let frame = word(b, rows.wrapping_add(offset))?;
    virtual_call(b, frame, 0x78, &[0])?;
    let rows = word(b, v + 0x218)?;
    let frame = word(b, rows.wrapping_add(offset))?;
    virtual_call(b, frame, 0x18, &[])?;
    Ok(())
}

fn operand<B: VehicleInitBackend>(b: &mut B, value: DamageVector) -> Out<B, [f32; 3]> {
    match value {
        DamageVector::Address(a) => vector(b, a),
        DamageVector::Value(x) => Ok(x),
    }
}
fn half<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u16> {
    let low = b.read_byte(a).map_err(DamageMeshError::Host)?;
    let high = b.read_byte(a + 1).map_err(DamageMeshError::Host)?;
    Ok(u16::from_le_bytes([low, high]))
}
fn row<B: VehicleInitBackend>(b: &mut B, v: u32, index: u32) -> Out<B, u32> {
    Ok(word(b, v + 0x218)?.wrapping_add(index.wrapping_mul(0x34)))
}
fn lamp_switch<B: VehicleDamageMeshBackend>(b: &mut B, v: u32, lamp: u32) -> Out<B, ()> {
    let frame = word(b, lamp)?;
    let side = if float(b, frame + 0x80)? < 0.0 { 1 } else { 2 };
    let index = word(b, lamp + 8)?;
    b.damage_lamp_switch(v, index, side)
        .map_err(DamageMeshError::Host)
}
fn lamp_count<B: VehicleInitBackend>(b: &mut B, v: u32) -> Out<B, u32> {
    let begin = word(b, v + 0x100)?;
    if begin == 0 {
        return Ok(0);
    }
    let end = word(b, v + 0x104)?;
    let extent = end.wrapping_sub(begin) as i32;
    if extent < 0 || !(extent as u32).is_multiple_of(32) {
        return Err(DamageMeshError::NumericDomain);
    }
    Ok(extent as u32 / 32)
}
/// The full admitted row branch after the outer flags/strength threshold checks,
/// including notifications, mesh publication, part health and lamp propagation.
pub fn vehicle_damage_complete_row<B: VehicleDamageMeshBackend>(
    b: &mut B,
    v: u32,
    index: u32,
    call: &mut VehicleDamageCall,
    effective_strength: f32,
    factor: f32,
) -> Out<B, ()> {
    let result = record(b, v, index, call.point, call.direction, call.radius, factor)?;
    let changed = result.changed[0] || result.changed[1];
    let function = word(b, v + 0x314)?;
    if function != 0 {
        let direction = operand(b, call.direction)?;
        let impulse = direction.map(|value| mul(call.strength, value));
        let current = row(b, v, index)?;
        let part_type = half(b, current + 0xc)?;
        let frame = word(b, current)?;
        let context = word(b, v + 0xc74)?;
        let point = operand(b, call.point)?;
        b.damage_notification(
            v, function, context, frame, part_type, point, impulse, changed,
        )
        .map_err(DamageMeshError::Host)?;
    }
    if !changed {
        return Ok(());
    }
    let current = row(b, v, index)?;
    if half(b, current + 0xc)? == 2 {
        call.strength = effective_strength;
    }
    vehicle_damage_publish_mesh_record(b, v, index, result.changed)?;
    if call.flags & 2 != 0 {
        return Ok(());
    }
    let current = row(b, v, index)?;
    let flags = b.read_byte(current + 0xe).map_err(DamageMeshError::Host)?;
    if flags & 2 == 0 {
        let divisor = float(b, current + 0x10)?;
        let multiplier = float(b, current + 0x18)?;
        let loss = mul(div(effective_strength, divisor), multiplier);
        let health = sub(float(b, current + 0x20)?, loss);
        put(b, current + 0x20, health)?;
        let current = row(b, v, index)?;
        if float(b, current + 0x20)? < 0.0 {
            put(b, current + 0x20, 0.0)?;
        }
    }
    let current = row(b, v, index)?;
    let flags = b.read_byte(current + 0xe).map_err(DamageMeshError::Host)?;
    b.write_byte(current + 0xe, flags | 2)
        .map_err(DamageMeshError::Host)?;
    let flags = word(b, v + 0xf8)?;
    b.write_word(v + 0xf8, flags | 2)
        .map_err(DamageMeshError::Host)?;
    let current = row(b, v, index)?;
    if half(b, current + 0xc)? == 3 {
        let direction = operand(b, call.direction)?;
        b.damage_break_glass(v, index, direction)
            .map_err(DamageMeshError::Host)?;
    }
    let current = row(b, v, index)?;
    if float(b, current + 0x20)? <= 0.0 {
        let frame = word(b, current)?;
        if word(b, frame + 0xac)? & 0x20 == 0 {
            b.damage_update_world(frame)
                .map_err(DamageMeshError::Host)?;
        }
        let position = vector(b, frame + 0x40)?;
        let center = vector(b, v + 0x398)?;
        let r: [f32; 3] = std::array::from_fn(|i| sub(position[i], center[i]));
        let w = vector(b, v + 0x420)?;
        let spin = [
            sub(mul(r[2], w[1]), mul(r[1], w[2])),
            sub(mul(r[0], w[2]), mul(r[2], w[0])),
            sub(mul(r[1], w[0]), mul(r[0], w[1])),
        ];
        let linear = vector(b, v + 0x1f90)?;
        let velocity = std::array::from_fn(|i| sub(linear[i], spin[i]));
        let context = word(b, v + 0xc74)?;
        let function = word(b, v + 0x2f8)?;
        b.damage_detach_part(v, function, context, index, velocity, call.extra)
            .map_err(DamageMeshError::Host)?;
        for i in (0..lamp_count(b, v)?).rev() {
            let lamp = word(b, v + 0x100)?.wrapping_add(i * 32);
            let current = row(b, v, index)?;
            if word(b, lamp)? == word(b, current)? {
                put(b, lamp + 0x18, 0.0)?;
                let lamp = word(b, v + 0x100)?.wrapping_add(i * 32);
                let flags = word(b, lamp + 0x14)?;
                b.write_word(lamp + 0x14, flags | 1)
                    .map_err(DamageMeshError::Host)?;
                let lamp = word(b, v + 0x100)?.wrapping_add(i * 32);
                if word(b, lamp + 0x14)? & 0x80000000 != 0 {
                    lamp_switch(b, v, lamp)?;
                }
            }
        }
    } else if half(b, current + 0xc)? == 5 {
        for i in (0..lamp_count(b, v)?).rev() {
            let lamp = word(b, v + 0x100)?.wrapping_add(i * 32);
            if u32::from(half(b, lamp + 0x12)?) == index {
                let current = row(b, v, index)?;
                let health = float(b, current + 0x20)?;
                put(b, lamp + 0x18, health)?;
                let lamp = word(b, v + 0x100)?.wrapping_add(i * 32);
                if float(b, lamp + 0x18)? < float(b, v + 0xd0)? {
                    let flags = word(b, lamp + 0x14)?;
                    b.write_word(lamp + 0x14, flags | 1)
                        .map_err(DamageMeshError::Host)?;
                    let lamp = word(b, v + 0x100)?.wrapping_add(i * 32);
                    if word(b, lamp + 0x14)? & 0x80000000 != 0 {
                        lamp_switch(b, v, lamp)?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    struct Owner {
        memory: VehicleFrameMemory,
        meshes: BTreeMap<u32, Vec<u32>>,
        buffers: BTreeMap<u32, u32>,
        locks: BTreeSet<u32>,
        random: VecDeque<i32>,
        calls: Vec<(u32, u32, Vec<u32>)>,
        notifications: Vec<(bool, [f32; 3])>,
        detached: Vec<[f32; 3]>,
    }
    impl Owner {
        fn new() -> Self {
            let mut memory = VehicleFrameMemory::default();
            memory.map_bytes(0x1000, &vec![0; 0x1000]).unwrap();
            memory.map_bytes(0x2000, &vec![0; 0x1000]).unwrap();
            for a in [
                0x3000, 0x4000, 0x5000, 0x6000, 0x6100, 0x7000, 0x7100, 0x8000, 0x8100, 0x9000,
                0x9100, 0xa000, 0xa100,
            ] {
                memory.map_bytes(a, &[0; 0x100]).unwrap();
            }
            for (a, x) in [
                (0x1208, 0x3000),
                (0x1218, 0x4000),
                (0x3004, 0x5000),
                (0x4004, 0x6000),
                (0x4000, 0x7000),
                (0x5008, 2),
                (0x6008, 2),
                (0x8008, 3),
                (0x8108, 1),
                (0x9008, 3),
                (0x9108, 1),
                (0x70ac, 0x20),
            ] {
                memory.write_word(a, x).unwrap();
            }
            for (i, x) in SMatrix::identity().elements.into_iter().enumerate() {
                memory
                    .write_word(0x7010 + i as u32 * 4, x.to_bits())
                    .unwrap();
            }
            memory.write_word(0x7040, 10f32.to_bits()).unwrap();
            for (i, x) in [0.25f32, 0.5, 0.75].into_iter().enumerate() {
                memory
                    .write_word(0x4028 + i as u32 * 4, x.to_bits())
                    .unwrap();
            }
            // Original and current arrays are separate live allocations.
            for base in [0xa000, 0xb000] {
                if base == 0xb000 {
                    memory.map_bytes(base, &[0; 0x100]).unwrap();
                }
                for (i, x) in [0.0f32, 0.5, 1.0].into_iter().enumerate() {
                    memory
                        .write_word(base + i as u32 * 32, x.to_bits())
                        .unwrap();
                }
            }
            memory.map_bytes(0xb100, &[0; 0x100]).unwrap();
            Self {
                memory,
                meshes: BTreeMap::from([
                    (0x5000, vec![0x8000, 0x8100]),
                    (0x6000, vec![0x9000, 0x9100]),
                ]),
                buffers: BTreeMap::from([
                    (0x8000, 0xa000),
                    (0x8100, 0xa100),
                    (0x9000, 0xb000),
                    (0x9100, 0xb100),
                ]),
                locks: BTreeSet::new(),
                random: VecDeque::from([9, 0, 3, 9, 0, 3]),
                calls: Vec::new(),
                notifications: Vec::new(),
                detached: Vec::new(),
            }
        }
    }
    impl VehicleInitBackend for Owner {
        type Error = VehicleMemoryError;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.memory.read_byte(a)
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            self.memory.read_word(a)
        }
        fn write_byte(&mut self, a: u32, x: u8) -> Result<(), Self::Error> {
            self.memory.write_byte(a, x)
        }
        fn write_word(&mut self, a: u32, x: u32) -> Result<(), Self::Error> {
            self.memory.write_word(a, x)
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.memory.allocate(n)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.memory.free(a)
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
    impl VehicleDamageMeshBackend for Owner {
        fn damage_update_world(&mut self, _: u32) -> Result<(), Self::Error> {
            panic!("fixture world matrix already current")
        }
        fn damage_rand(&mut self) -> Result<i32, Self::Error> {
            Ok(self.random.pop_front().expect("unexpected random call"))
        }
        fn damage_notification(
            &mut self,
            _: u32,
            _: u32,
            _: u32,
            _: u32,
            _: u16,
            _: [f32; 3],
            impulse: [f32; 3],
            changed: bool,
        ) -> Result<(), Self::Error> {
            self.notifications.push((changed, impulse));
            Ok(())
        }
        fn damage_break_glass(&mut self, _: u32, _: u32, _: [f32; 3]) -> Result<(), Self::Error> {
            panic!("unexpected glass")
        }
        fn damage_detach_part(
            &mut self,
            _: u32,
            _: u32,
            _: u32,
            _: u32,
            velocity: [f32; 3],
            _: u32,
        ) -> Result<(), Self::Error> {
            self.detached.push(velocity);
            Ok(())
        }
        fn damage_lamp_switch(&mut self, _: u32, _: u32, _: u32) -> Result<(), Self::Error> {
            panic!("unexpected lamp")
        }
        fn damage_virtual(&mut self, o: u32, s: u32, a: &[u32]) -> Result<u32, Self::Error> {
            self.calls.push((o, s, a.to_vec()));
            if let Some(levels) = self.meshes.get(&o) {
                return match (s, a) {
                    (4, [i]) => Ok(levels.get(*i as usize).copied().unwrap_or(0)),
                    (0x14, []) => Ok(0),
                    _ => panic!("unexpected mesh virtual"),
                };
            }
            if let Some(&buffer) = self.buffers.get(&o) {
                return match (s, a) {
                    (4, [0]) => {
                        assert!(self.locks.insert(o));
                        Ok(buffer)
                    }
                    (8, []) => {
                        assert!(self.locks.remove(&o));
                        Ok(0)
                    }
                    (0x28, []) => Ok(0),
                    _ => panic!("unexpected level virtual"),
                };
            }
            assert!(o == 0x7000 && ((s == 0x78 && a == [0]) || (s == 0x18 && a.is_empty())));
            Ok(0)
        }
    }
    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }
    #[test]
    fn both_lods_deform_in_reverse_order_keep_reference_and_publish_afterward() {
        let mut b = Owner::new();
        let result =
            vehicle_damage_mesh_record(&mut b, 0x1000, 0, [10., 0., 0.], [5., -5., 5.], 1., 1.)
                .unwrap();
        assert_eq!(result.changed, [true, true]);
        assert!(b.locks.is_empty());
        assert_eq!(b.random.len(), 3);
        close(float(&mut b, 0xb000).unwrap(), 0.175);
        close(float(&mut b, 0xb020).unwrap(), 0.7425);
        close(float(&mut b, 0xb104).unwrap(), -0.395);
        assert_eq!(float(&mut b, 0xb040).unwrap(), 1.0); // exactly at radius
        assert_eq!(float(&mut b, 0xa000).unwrap(), 0.0);
        assert_eq!(float(&mut b, 0xa020).unwrap(), 0.5);
        assert_eq!(word(&mut b, 0x10f8).unwrap(), 0);
        vehicle_damage_publish_mesh_record(&mut b, 0x1000, 0, result.changed).unwrap();
        assert_eq!(word(&mut b, 0x10f8).unwrap(), 1);
        assert_eq!(
            &b.calls[b.calls.len() - 3..],
            &[
                (0x6000, 0x14, vec![]),
                (0x7000, 0x78, vec![0]),
                (0x7000, 0x18, vec![])
            ]
        );
    }
    #[test]
    fn repeated_hits_clamp_total_deformation_and_misses_do_not_consume_random() {
        let mut b = Owner::new();
        b.random.extend([0; 8]);
        for _ in 0..2 {
            vehicle_damage_mesh_record(&mut b, 0x1000, 0, [10., 0., 0.], [5., -5., 5.], 3., 1.)
                .unwrap();
        }
        for (axis, expected) in [0.25f32, -0.5, 0.75].into_iter().enumerate() {
            close(float(&mut b, 0xb000 + axis as u32 * 4).unwrap(), expected);
        }
        let before = b.random.len();
        let result =
            vehicle_damage_mesh_record(&mut b, 0x1000, 0, [100., 0., 0.], [5., -5., 5.], 1., 1.)
                .unwrap();
        assert_eq!(result.changed, [false, false]);
        assert_eq!(b.random.len(), before);
        let count = b.calls.len();
        vehicle_damage_publish_mesh_record(&mut b, 0x1000, 0, result.changed).unwrap();
        assert_eq!(b.calls.len(), count);
    }
    #[test]
    fn complete_row_notifies_before_strength_replacement_and_charges_health_once() {
        let mut b = Owner::new();
        b.random.extend([0; 16]);
        b.write_word(0x400c, 2).unwrap();
        b.write_word(0x1314, 0x123).unwrap();
        for (a, x) in [(0x4010, 100f32), (0x4018, 10.), (0x4020, 100.)] {
            put(&mut b, a, x).unwrap();
        }
        let mut call = VehicleDamageCall {
            point: DamageVector::Value([10., 0., 0.]),
            direction: DamageVector::Value([5., -5., 5.]),
            radius: 3.,
            strength: 100.,
            flags: 0,
            extra: 0,
        };
        vehicle_damage_complete_row(&mut b, 0x1000, 0, &mut call, 50., 0.5).unwrap();
        assert_eq!(call.strength, 50.);
        assert_eq!(b.notifications, vec![(true, [500., -500., 500.])]);
        assert_eq!(float(&mut b, 0x4020).unwrap(), 95.);
        assert_eq!(word(&mut b, 0x10f8).unwrap(), 3);
        vehicle_damage_complete_row(&mut b, 0x1000, 0, &mut call, 50., 0.5).unwrap();
        assert_eq!(float(&mut b, 0x4020).unwrap(), 95.);
        assert!(b.detached.is_empty());
        assert_eq!(b.notifications[1], (true, [250., -250., 250.]));
    }
    #[test]
    fn unchanged_row_still_notifies_and_does_not_charge_health_or_publish() {
        let mut b = Owner::new();
        b.write_word(0x1314, 0x123).unwrap();
        put(&mut b, 0x4020, 100.).unwrap();
        let mut call = VehicleDamageCall {
            point: DamageVector::Value([100., 0., 0.]),
            direction: DamageVector::Value([1., 0., 0.]),
            radius: 1.,
            strength: 10.,
            flags: 0,
            extra: 0,
        };
        vehicle_damage_complete_row(&mut b, 0x1000, 0, &mut call, 10., 1.).unwrap();
        assert_eq!(b.notifications, vec![(false, [10., 0., 0.])]);
        assert_eq!(float(&mut b, 0x4020).unwrap(), 100.);
        assert_eq!(word(&mut b, 0x10f8).unwrap(), 0);
        assert_eq!(b.random.len(), 6);
    }
    #[test]
    fn depleted_part_detaches_with_live_body_velocity_and_visual_only_keeps_health() {
        let mut b = Owner::new();
        b.random.extend([0; 16]);
        for (a, x) in [
            (0x4010, 100f32),
            (0x4018, 10.),
            (0x4020, 1.),
            (0x1424, 2.),
            (0x2f90, 3.),
            (0x2f94, 4.),
            (0x2f98, 5.),
        ] {
            put(&mut b, a, x).unwrap();
        }
        let mut call = VehicleDamageCall {
            point: DamageVector::Value([10., 0., 0.]),
            direction: DamageVector::Value([5., -5., 5.]),
            radius: 3.,
            strength: 100.,
            flags: 2,
            extra: 0,
        };
        vehicle_damage_complete_row(&mut b, 0x1000, 0, &mut call, 100., 1.).unwrap();
        assert_eq!(float(&mut b, 0x4020).unwrap(), 1.);
        assert_eq!(word(&mut b, 0x400c).unwrap() & 0x20000, 0);
        assert!(b.detached.is_empty());
        call.flags = 0;
        vehicle_damage_complete_row(&mut b, 0x1000, 0, &mut call, 100., 1.).unwrap();
        assert_eq!(float(&mut b, 0x4020).unwrap(), 0.);
        // omega=[0,2,0], r=[10,0,0]; linear - omega cross r.
        assert_eq!(b.detached, vec![[3., 4., 25.]]);
    }
}
