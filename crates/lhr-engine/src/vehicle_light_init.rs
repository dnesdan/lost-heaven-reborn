//! Original Game light discovery and pairing, PC RVA0xc1420.
//! Native identities address the caller's live backend storage.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleLightQuery {
    Rear,
    Interior,
    Indicator,
    Front,
    Brake,
    Back,
    Projector,
    ProjectorChildren,
}
impl VehicleLightQuery {
    pub fn pattern(self) -> &'static [u8] {
        match self {
            Self::Rear => b"LIGHTRE*",
            Self::Interior => b"LIGHTIN*",
            Self::Indicator => b"LIGHTBI*",
            Self::Front => b"LIGHTLI*",
            Self::Brake => b"LIGHTBR*",
            Self::Back => b"LIGHTBA*",
            Self::Projector => b"PROJECTOR*",
            Self::ProjectorChildren => b"LIGHT*",
        }
    }
    pub fn mask(self) -> u32 {
        if self == Self::Projector {
            0x10020
        } else {
            0x10021
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleLightInitUnavailable<E> {
    Effect(E),
    NumericDomain,
    NoContinuation,
}
impl<E> From<E> for VehicleLightInitUnavailable<E> {
    fn from(e: E) -> Self {
        Self::Effect(e)
    }
}

pub struct VehicleLightEnumeration<'a> {
    header: u32,
    memory: Option<VehicleComponentTailMemory<'a>>,
}
impl VehicleLightEnumeration<'_> {
    pub fn visit<B: VehicleInitBackend + ?Sized>(
        &mut self,
        b: &mut B,
        frame: u32,
    ) -> Result<u32, B::Error> {
        if let Some(memory) = &mut self.memory {
            vehicle_collect_frame(
                &mut component_tail::Projection {
                    backend: b,
                    memory: VehicleComponentTailMemory {
                        vehicle_address: memory.vehicle_address,
                        vehicle: &mut *memory.vehicle,
                        scratch: &mut *memory.scratch,
                    },
                },
                self.header,
                frame,
            )?;
        } else {
            vehicle_collect_frame(b, self.header, frame)?;
        }
        Ok(0)
    }
}
/// Required original callees. Projector constructors must provide the actual
/// visual/light/texture ownership effects or report unavailable.
pub trait VehicleLightInitBackend: VehicleLightResourceBackend {
    fn enumerate_vehicle_lights(
        &mut self,
        root: u32,
        query: VehicleLightQuery,
        target: &mut VehicleLightEnumeration<'_>,
    ) -> Result<(), Self::Error>;
    /// Frame FindChildFrame virtual+0x38, BILLBOARD*, mask0x1ffff.
    fn find_vehicle_light_billboard(&mut self, frame: u32) -> Result<u32, Self::Error>;
    fn set_vehicle_light_on(&mut self, frame: u32, on: u32) -> Result<(), Self::Error>;
}
type LightResult<B, T> = Result<T, VehicleLightInitUnavailable<<B as VehicleInitBackend>::Error>>;
fn a(p: u32, n: u32) -> u32 {
    p.wrapping_add(n)
}
fn word<B: VehicleInitBackend>(b: &mut B, p: u32) -> LightResult<B, u32> {
    b.read_word(p).map_err(Into::into)
}
fn store<B: VehicleInitBackend>(b: &mut B, p: u32, v: u32) -> LightResult<B, ()> {
    b.write_word(p, v).map_err(Into::into)
}
fn short<B: VehicleInitBackend>(b: &mut B, p: u32, v: u16) -> LightResult<B, ()> {
    for (i, x) in v.to_le_bytes().into_iter().enumerate() {
        b.write_byte(a(p, i as u32), x)?;
    }
    Ok(())
}
fn copy<B: VehicleInitBackend>(b: &mut B, d: u32, s: u32, n: u32) -> LightResult<B, ()> {
    for o in (0..n).step_by(4) {
        let v = word(b, a(s, o))?;
        store(b, a(d, o), v)?;
    }
    Ok(())
}

// Both reached 32-byte record specializations append at end. Their placement
// copies omit a null destination; allocation/free/header updates remain ordered.
fn append<B: VehicleInitBackend>(b: &mut B, h: u32, s: u32) -> LightResult<B, ()> {
    let end = word(b, a(h, 8))?;
    let cap = word(b, a(h, 12))?;
    if (((cap.wrapping_sub(end) as i32) >> 5) as u32) < 1 {
        let begin = word(b, a(h, 4))?;
        let count = if begin == 0 {
            0
        } else {
            ((end.wrapping_sub(begin) as i32) >> 5) as u32
        };
        let capacity = count.wrapping_add(count.max(1));
        let alloc_count = if (capacity as i32) < 0 { 0 } else { capacity };
        let destination = b.allocate(alloc_count.wrapping_mul(32))?;
        let mut source = word(b, a(h, 4))?;
        let mut out = destination;
        while source != end {
            if out != 0 {
                copy(b, out, source, 32)?;
            }
            source = a(source, 32);
            out = a(out, 32);
        }
        if out != 0 {
            copy(b, out, s, 32)?;
        }
        let old = word(b, a(h, 4))?;
        b.free(old)?;
        let begin = word(b, a(h, 4))?;
        store(b, a(h, 12), a(destination, capacity.wrapping_mul(32)))?;
        let count = if begin == 0 {
            0
        } else {
            let end = word(b, a(h, 8))?;
            ((end.wrapping_sub(begin) as i32) >> 5) as u32
        };
        store(b, a(h, 4), destination)?;
        store(
            b,
            a(h, 8),
            a(destination, count.wrapping_add(1).wrapping_mul(32)),
        )?;
    } else {
        if end != 0 {
            copy(b, end, s, 32)?;
        }
        let end = word(b, a(h, 8))?;
        store(b, a(h, 8), a(end, 32))?;
    }
    Ok(())
}
fn vector_count<B: VehicleInitBackend>(b: &mut B, h: u32, stride: u32) -> LightResult<B, u32> {
    let begin = word(b, a(h, 4))?;
    Ok(if begin == 0 {
        0
    } else {
        let end = word(b, a(h, 8))?;
        ((end.wrapping_sub(begin) as i32) / stride as i32) as u32
    })
}
fn record<B: VehicleInitBackend>(b: &mut B, h: u32, i: u32, stride: u32) -> LightResult<B, u32> {
    let begin = word(b, a(h, 4))?;
    Ok(a(begin, i.wrapping_mul(stride)))
}
fn relative_side<B: VehicleLightInitBackend>(
    b: &mut B,
    v: u32,
    f: u32,
    stack: u32,
) -> LightResult<B, bool> {
    let model = word(b, a(v, 0x34c))?;
    if b.read_byte(a(model, 0xac))? & 0x20 == 0 {
        b.update_model_world_matrix(model)?;
    }
    copy(b, a(stack, 0x58), a(model, 0x10), 64)?;
    copy(b, a(stack, 0x98), a(model, 0x10), 64)?;
    let mut words = [0; 16];
    for (i, x) in words.iter_mut().enumerate() {
        *x = word(b, a(stack, 0x98 + i as u32 * 4))?;
    }
    let (inverse, _) = super::components::matrix(words)
        .inverse()
        .map_err(|_| VehicleLightInitUnavailable::NumericDomain)?;
    for (i, x) in inverse.elements.into_iter().enumerate() {
        store(b, a(stack, 0x58 + i as u32 * 4), x.to_bits())?;
    }
    if b.read_byte(a(f, 0xac))? & 0x20 == 0 {
        b.update_model_world_matrix(f)?;
    }
    let mut position = [0; 3];
    for (i, x) in position.iter_mut().enumerate() {
        *x = word(b, a(f, 0x40 + i as u32 * 4))?;
    }
    let point = inverse
        .try_transform_point(position.map(f32::from_bits))
        .map_err(|_| VehicleLightInitUnavailable::NumericDomain)?;
    for (i, x) in point.into_iter().enumerate() {
        store(b, a(stack, 0x48 + i as u32 * 4), x.to_bits())?;
    }
    // Native tests C0|C3: negative, zero and unordered take the lower kind.
    Ok(!matches!(
        point[0].partial_cmp(&0.),
        Some(std::cmp::Ordering::Greater)
    ))
}

/// `stack` denotes ESP after the original EH/prologue (0xec bytes below the
/// caller's pre-call ESP). It must already be addressable; opaque leading
/// bytes are retained. No artificial original heap is allocated for stack.
pub fn vehicle_light_init<B: VehicleLightInitBackend>(
    b: &mut B,
    vehicle: u32,
    stack: u32,
) -> LightResult<B, ()> {
    let marker = b.read_byte(a(stack, 0x27))?;
    b.write_byte(a(stack, 0x10), marker)?;
    for off in [0x14, 0x18, 0x1c] {
        store(b, a(stack, off), 0)?;
    }
    let header = a(stack, 0x10);
    let temporary = a(stack, 0x28);
    for query in [
        VehicleLightQuery::Rear,
        VehicleLightQuery::Interior,
        VehicleLightQuery::Indicator,
        VehicleLightQuery::Front,
        VehicleLightQuery::Brake,
        VehicleLightQuery::Back,
    ] {
        let model = word(b, a(vehicle, 0x34c))?;
        b.enumerate_vehicle_lights(
            model,
            query,
            &mut VehicleLightEnumeration {
                header,
                memory: None,
            },
        )?;
        let mut index = 0;
        while index < vector_count(b, header, 4)? {
            let slot = record(b, header, index, 4)?;
            let frame = word(b, slot)?;
            for off in (0..32).step_by(4) {
                store(b, a(temporary, off), 0)?;
            }
            store(b, temporary, frame)?;
            let billboard = b.find_vehicle_light_billboard(frame)?;
            store(b, a(temporary, 4), billboard)?;
            if billboard != 0 {
                b.set_vehicle_light_on(billboard, 0)?;
            }
            let frame = word(b, temporary)?;
            let visual = vehicle_create_light_projector(
                b,
                vehicle,
                frame,
                a(temporary, 12),
                stack.wrapping_sub(0xb4),
            )?;
            store(b, a(temporary, 8), visual)?;
            if visual != 0 {
                b.set_vehicle_light_on(visual, 0)?;
            }
            let kind = match query {
                VehicleLightQuery::Rear => 7,
                VehicleLightQuery::Brake => 8,
                VehicleLightQuery::Back => 9,
                VehicleLightQuery::Interior
                | VehicleLightQuery::Indicator
                | VehicleLightQuery::Front => {
                    let frame = word(b, temporary)?;
                    let low = relative_side(b, vehicle, frame, stack)?;
                    let base = match query {
                        VehicleLightQuery::Interior => 1,
                        VehicleLightQuery::Indicator => 3,
                        _ => 5,
                    };
                    base + u16::from(!low)
                }
                _ => unreachable!(),
            };
            short(b, a(temporary, 16), kind)?;
            append(b, a(vehicle, 0xfc), temporary)?;
            index = index.wrapping_add(1);
        }
        // Original pointer erase/copy range is empty; backing capacity survives.
        let begin = word(b, a(header, 4))?;
        store(b, a(header, 8), begin)?;
    }
    // Bind light records to the most recent exact component, or to its nearest
    // ancestor represented in the component collection. Parent/owner differ.
    let mut count = vector_count(b, a(vehicle, 0xfc), 32)?;
    while count != 0 {
        count = count.wrapping_sub(1);
        let light = record(b, a(vehicle, 0xfc), count, 32)?;
        short(b, a(light, 0x12), 0xffff)?;
        let frame = word(b, light)?;
        let mut component = vector_count(b, a(vehicle, 0x214), 52)?;
        let mut found = None;
        while component != 0 {
            component -= 1;
            let c = record(b, a(vehicle, 0x214), component, 52)?;
            if word(b, c)? == frame && (word(b, a(c, 12))? as u16) == 5 {
                found = Some(component);
                break;
            }
        }
        if let Some(index) = found {
            short(b, a(light, 0x12), index as u16)?;
            continue;
        }
        let mut parent = word(b, a(frame, 0x120))?;
        while parent != word(b, a(vehicle, 0x34c))? {
            let mut component = vector_count(b, a(vehicle, 0x214), 52)?;
            let mut found = None;
            while component != 0 {
                component -= 1;
                let c = record(b, a(vehicle, 0x214), component, 52)?;
                if word(b, c)? == parent {
                    found = Some(component);
                    break;
                }
            }
            if let Some(index) = found {
                short(b, a(light, 0x12), index as u16)?;
                break;
            }
            parent = word(b, a(parent, 0x120))?;
        }
    }
    let model = word(b, a(vehicle, 0x34c))?;
    b.enumerate_vehicle_lights(
        model,
        VehicleLightQuery::Projector,
        &mut VehicleLightEnumeration {
            header,
            memory: None,
        },
    )?;
    let mut index = 0;
    while index < vector_count(b, header, 4)? {
        let marker = b.read_byte(a(stack, 0x27))?;
        let children = a(stack, 0x48);
        b.write_byte(children, marker)?;
        for off in [4, 8, 12] {
            store(b, a(children, off), 0)?;
        }
        let entry = record(b, header, index, 4)?;
        let frame = word(b, entry)?;
        b.enumerate_vehicle_lights(
            frame,
            VehicleLightQuery::ProjectorChildren,
            &mut VehicleLightEnumeration {
                header: children,
                memory: None,
            },
        )?;
        if vector_count(b, children, 4)? == 2 {
            let first = record(b, children, 0, 4)?;
            let first = word(b, first)?;
            let second = record(b, children, 1, 4)?;
            let second = word(b, second)?;
            let find = |b: &mut B, frame: u32| -> LightResult<B, Option<u32>> {
                let mut count = vector_count(b, a(vehicle, 0xfc), 32)?;
                while count != 0 {
                    count -= 1;
                    let r = record(b, a(vehicle, 0xfc), count, 32)?;
                    if word(b, r)? == frame && word(b, a(r, 8))? == 0 {
                        return Ok(Some(count));
                    }
                }
                Ok(None)
            };
            if let Some(first) = find(b, first)? {
                if let Some(second) = find(b, second)? {
                    if first != second {
                        let first_record = record(b, a(vehicle, 0xfc), first, 32)?;
                        let entry = record(b, header, index, 4)?;
                        let frame = word(b, entry)?;
                        let visual = vehicle_create_shared_projector(
                            b,
                            vehicle,
                            frame,
                            a(first_record, 12),
                            stack.wrapping_sub(0xc8),
                        )?;
                        store(b, a(first_record, 8), visual)?;
                        let first_record = record(b, a(vehicle, 0xfc), first, 32)?;
                        let visual = word(b, a(first_record, 8))?;
                        let second_record = record(b, a(vehicle, 0xfc), second, 32)?;
                        store(b, a(second_record, 8), visual)?;
                        let first_record = record(b, a(vehicle, 0xfc), first, 32)?;
                        let light = word(b, a(first_record, 12))?;
                        let second_record = record(b, a(vehicle, 0xfc), second, 32)?;
                        store(b, a(second_record, 12), light)?;
                        let first_record = record(b, a(vehicle, 0xfc), first, 32)?;
                        if word(b, a(first_record, 8))? != 0 {
                            for i in [first, second] {
                                let r = record(b, a(vehicle, 0xfc), i, 32)?;
                                let flags = word(b, a(r, 0x14))?;
                                store(b, a(r, 0x14), flags | 0x8000_0000)?;
                            }
                            let projects =
                                vector_count(b, a(vehicle, 0x10c), 28)?.wrapping_sub(1) as u16;
                            let first_record = record(b, a(vehicle, 0xfc), first, 32)?;
                            short(b, a(first_record, 0x1c), projects)?;
                            let second_record = record(b, a(vehicle, 0xfc), second, 32)?;
                            short(b, a(second_record, 0x1c), projects)?;
                        }
                    }
                }
            }
        }
        let allocation = word(b, a(children, 4))?;
        b.free(allocation)?;
        for off in [4, 8, 12] {
            store(b, a(children, off), 0)?;
        }
        index = index.wrapping_add(1);
    }
    let allocation = word(b, a(header, 4))?;
    b.free(allocation)?;
    Ok(())
}

// Virtual methods retain their underlying resource owner. Inline loads/stores
// continue through the live vehicle/Init-stack projection.
use crate::vehicle_bodycol::{
    VehicleBodycolBackend, VehicleBodycolEnumeration, VehicleBodycolUnavailable,
};
impl<B: VehicleComponentsBackend> VehicleComponentsBackend for component_tail::Projection<'_, B> {
    fn prepare_frame_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.prepare_frame_matrix(frame)
    }
    fn normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.normalize_frame_quaternion(frame)
    }
    fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.set_frame_rotation_matrix(frame)
    }
    fn insert_exhaust(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        value: [u32; 6],
    ) -> Result<(), Self::Error> {
        self.backend.insert_exhaust(vehicle, insertion, value)
    }
    fn insert_special_point(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        point: [u32; 3],
    ) -> Result<(), Self::Error> {
        self.backend.insert_special_point(vehicle, insertion, point)
    }
    fn insert_special_control(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        kind: u32,
        enabled: u8,
    ) -> Result<(), Self::Error> {
        self.backend
            .insert_special_control(vehicle, insertion, kind, enabled)
    }
    fn insert_exhaust_projected(
        &mut self,
        _vehicle: &mut [u8],
        _scratch: &mut VehicleComponentTailScratch,
        _insertion: u32,
        _value: [u32; 6],
    ) -> Result<(), VehicleComponentsUnavailable<Self::Error>> {
        self.backend
            .insert_exhaust_projected(_vehicle, _scratch, _insertion, _value)
    }
    fn insert_special_point_projected(
        &mut self,
        _vehicle: &mut [u8],
        _scratch: &mut VehicleComponentTailScratch,
        _insertion: u32,
        _source_offset: u32,
        _point: [u32; 3],
    ) -> Result<(), VehicleComponentsUnavailable<Self::Error>> {
        self.backend.insert_special_point_projected(
            _vehicle,
            _scratch,
            _insertion,
            _source_offset,
            _point,
        )
    }
    fn insert_special_control_projected(
        &mut self,
        _vehicle: &mut [u8],
        _scratch: &mut VehicleComponentTailScratch,
        _insertion: u32,
        _kind: u32,
        _enabled: u8,
    ) -> Result<(), VehicleComponentsUnavailable<Self::Error>> {
        self.backend
            .insert_special_control_projected(_vehicle, _scratch, _insertion, _kind, _enabled)
    }
    fn special_collision_callback(
        &mut self,
        function: u32,
        context: u32,
        frame: u32,
        control_pointer: u32,
        index: u32,
    ) -> Result<(), Self::Error> {
        self.backend
            .special_collision_callback(function, context, frame, control_pointer, index)
    }
    fn link_frame(&mut self, frame: u32, parent: u32, flags: u32) -> Result<(), Self::Error> {
        self.backend.link_frame(frame, parent, flags)
    }
    fn remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error> {
        self.backend.remove_model_child(model, frame)
    }
    fn enumerate_components(
        &mut self,
        model: u32,
        query: VehicleComponentsQuery,
        target: &mut VehicleComponentsEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.backend.enumerate_components(model, query, target)
    }
    fn body_collision_callback(
        &mut self,
        vehicle: &mut [u8],
        context: VehicleBodyCollisionContext,
        frame: u32,
    ) -> Result<u32, Self::Error> {
        self.backend
            .body_collision_callback(vehicle, context, frame)
    }
    fn finish_body_collision_callbacks(
        &mut self,
        vehicle: &mut [u8],
        context: VehicleBodyCollisionContext,
    ) -> Result<(), Self::Error> {
        self.backend
            .finish_body_collision_callbacks(vehicle, context)
    }
    fn update_model_world_matrix(&mut self, model: u32) -> Result<(), Self::Error> {
        self.backend.update_model_world_matrix(model)
    }
    fn initialize_body_collision(
        &mut self,
        vehicle: &mut [u8],
        model: u32,
        name_length_with_nul: u32,
    ) -> Result<(), Self::Error> {
        self.backend
            .initialize_body_collision(vehicle, model, name_length_with_nul)
    }
    fn parse_component_number(&mut self, address: u32) -> Result<i32, Self::Error> {
        self.backend.parse_component_number(address)
    }
}
impl<B: VehicleBodycolBackend> VehicleBodycolBackend for component_tail::Projection<'_, B> {
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
        target: &mut VehicleBodycolEnumeration,
    ) -> Result<(), VehicleBodycolUnavailable<Self::Error>> {
        self.backend.bodycol_enumerate(frame, pattern, mask, target)
    }
}
impl<B: VehicleComponentTailBackend> VehicleComponentTailBackend
    for component_tail::Projection<'_, B>
{
    fn materialize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.materialize_frame_quaternion(frame)
    }
    fn find_window(&mut self, door: u32) -> Result<u32, Self::Error> {
        self.backend.find_window(door)
    }
    fn enumerate_tail(
        &mut self,
        root: u32,
        query: VehicleComponentTailQuery,
        target: &mut VehicleComponentTailEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.backend.enumerate_tail(root, query, target)
    }
    fn seat_callback(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        function: u32,
        context: u32,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.backend.seat_callback(memory, function, context, frame)
    }
}
impl<B: VehicleAxisUpdateBackend> VehicleAxisUpdateBackend for component_tail::Projection<'_, B> {
    fn set_axis_frame_on(&mut self, frame: u32, on: u32) -> Result<(), Self::Error> {
        self.backend.set_axis_frame_on(frame, on)
    }
    fn set_axis_frame_direction(
        &mut self,
        frame: u32,
        direction: [u32; 3],
    ) -> Result<(), Self::Error> {
        self.backend.set_axis_frame_direction(frame, direction)
    }
    fn set_axis_matrix_direction(
        &mut self,
        frame: u32,
        direction: [u32; 3],
        up: [u32; 3],
    ) -> Result<(), Self::Error> {
        self.backend.set_axis_matrix_direction(frame, direction, up)
    }
}
impl<B: VehicleLightResourceBackend> VehicleLightResourceBackend
    for component_tail::Projection<'_, B>
{
    fn create_vehicle_light_visual(&mut self, subtype: u32) -> Result<u32, Self::Error> {
        self.backend.create_vehicle_light_visual(subtype)
    }
    fn create_vehicle_light_frame(&mut self, kind: u32) -> Result<u32, Self::Error> {
        self.backend.create_vehicle_light_frame(kind)
    }
    fn vehicle_light_virtual(
        &mut self,
        object: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, Self::Error> {
        self.backend.vehicle_light_virtual(object, slot, args)
    }
    fn vehicle_light_texture_cache(&mut self, name: &[u8]) -> Result<(u32, u32), Self::Error> {
        self.backend.vehicle_light_texture_cache(name)
    }
}
impl<B: VehicleInitBackend + ?Sized> component_tail::Projection<'_, B> {
    // The enumerator delegates visits to the underlying frame backend. Carry
    // the same live mapping across that boundary for allocator callbacks too.
    fn enumerate_lights_with_projection<R>(
        &mut self,
        header: u32,
        enumerate: impl FnOnce(&mut B, &mut VehicleLightEnumeration<'_>) -> R,
    ) -> R {
        let mut target = VehicleLightEnumeration {
            header,
            memory: Some(VehicleComponentTailMemory {
                vehicle_address: self.memory.vehicle_address,
                vehicle: &mut *self.memory.vehicle,
                scratch: &mut *self.memory.scratch,
            }),
        };
        enumerate(self.backend, &mut target)
    }
}
impl<B: VehicleLightInitBackend> VehicleLightInitBackend for component_tail::Projection<'_, B> {
    fn enumerate_vehicle_lights(
        &mut self,
        root: u32,
        query: VehicleLightQuery,
        target: &mut VehicleLightEnumeration<'_>,
    ) -> Result<(), Self::Error> {
        self.enumerate_lights_with_projection(target.header, |backend, target| {
            backend.enumerate_vehicle_lights(root, query, target)
        })
    }
    fn find_vehicle_light_billboard(&mut self, frame: u32) -> Result<u32, Self::Error> {
        self.backend.find_vehicle_light_billboard(frame)
    }
    fn set_vehicle_light_on(&mut self, frame: u32, on: u32) -> Result<(), Self::Error> {
        self.backend.set_vehicle_light_on(frame, on)
    }
}
impl VehicleInitState {
    pub(super) fn initialize_lights<B: VehicleLightInitBackend>(
        &mut self,
        b: &mut B,
        stack: u32,
    ) -> Result<(), VehicleLightInitUnavailable<B::Error>> {
        let base = self
            .tail_vehicle_address
            .ok_or(VehicleLightInitUnavailable::NoContinuation)?;
        if self.tail_scratch.is_none() {
            return Err(VehicleLightInitUnavailable::NoContinuation);
        }
        vehicle_light_init(&mut self.tail_projection(b, base), base, stack)
    }
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use std::collections::BTreeMap;

    const HEADER: u32 = 0x9000;
    const VEHICLE: u32 = 0x5000;
    const STACK: u32 = 0x6000;
    const FRAME: u32 = 0x1234;

    struct CollectorBackend {
        words: BTreeMap<u32, u32>,
        projected: bool,
        fail_free: bool,
        events: Vec<&'static str>,
    }
    impl CollectorBackend {
        fn new(projected: bool, fail_free: bool) -> Self {
            Self {
                words: BTreeMap::from([
                    (HEADER + 4, 0x3000),
                    (HEADER + 8, 0x3004),
                    (HEADER + 12, 0x3004),
                    (0x3000, 0xeeee),
                ]),
                projected,
                fail_free,
                events: Vec::new(),
            }
        }
    }
    impl VehicleInitBackend for CollectorBackend {
        type Error = &'static str;
        fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
            let word = self.read_word(address & !3)?;
            Ok((word >> ((address & 3) * 8)) as u8)
        }
        fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
            self.words.get(&address).copied().ok_or("unmapped read")
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
            let shift = (address & 3) * 8;
            let word = self.words.entry(address & !3).or_default();
            *word = (*word & !(255 << shift)) | (u32::from(value) << shift);
            Ok(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            self.words.insert(address, value);
            Ok(())
        }
        fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
            assert!(!self.projected, "delegation lost the live allocation view");
            assert_eq!(bytes, 8);
            self.events.push("allocate");
            Ok(0x4000)
        }
        fn free(&mut self, address: u32) -> Result<(), Self::Error> {
            assert!(!self.projected, "delegation lost the live free view");
            assert_eq!(address, 0x3000);
            self.events.push("free");
            Ok(())
        }
        fn allocate_projected(
            &mut self,
            memory: &mut VehicleComponentTailMemory<'_>,
            bytes: u32,
        ) -> Result<u32, Self::Error> {
            assert_eq!(bytes, 8);
            self.events.push("allocate_projected");
            assert_eq!(memory.write_word(VEHICLE + 0x24, 0xa1), Some(()));
            assert_eq!(memory.write_word(STACK + 0x1c, 0xb1), Some(()));
            Ok(0x4000)
        }
        fn free_projected(
            &mut self,
            memory: &mut VehicleComponentTailMemory<'_>,
            address: u32,
        ) -> Result<(), Self::Error> {
            assert_eq!(address, 0x3000);
            self.events.push("free_projected");
            assert_eq!(memory.read_word(VEHICLE + 0x24), Some(0xa1));
            assert_eq!(memory.read_word(STACK + 0x1c), Some(0xb1));
            assert_eq!(memory.write_word(VEHICLE + 0x28, 0xa2), Some(()));
            assert_eq!(memory.write_word(STACK + 0x20, 0xb2), Some(()));
            // The collector must reload this external header after free too.
            self.words.insert(HEADER + 8, 0x3000);
            if self.fail_free {
                Err("free unavailable")
            } else {
                Ok(())
            }
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            unreachable!("the original addressable collector must run")
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            unreachable!()
        }
    }

    #[test]
    fn delegated_light_collector_keeps_live_allocator_and_free_mutations() {
        for fail_free in [false, true] {
            let mut backend = CollectorBackend::new(true, fail_free);
            let mut vehicle = [0; 0x80];
            let mut scratch = VehicleComponentTailScratch {
                address: STACK,
                bytes: [0; VEHICLE_INIT_STACK_BYTES],
            };
            let result = component_tail::Projection {
                backend: &mut backend,
                memory: VehicleComponentTailMemory {
                    vehicle_address: VEHICLE,
                    vehicle: &mut vehicle,
                    scratch: &mut scratch,
                },
            }
            .enumerate_lights_with_projection(HEADER, |backend, target| {
                // Real enumerators invoke the target with their concrete backend.
                target.visit(backend, FRAME)
            });
            assert_eq!(
                result,
                if fail_free {
                    Err("free unavailable")
                } else {
                    Ok(0)
                }
            );
            assert_eq!(backend.events, ["allocate_projected", "free_projected"]);
            assert_eq!(&vehicle[0x24..0x2c], &[0xa1, 0, 0, 0, 0xa2, 0, 0, 0]);
            assert_eq!(&scratch.bytes[0x1c..0x24], &[0xb1, 0, 0, 0, 0xb2, 0, 0, 0]);
            assert_eq!(backend.words[&0x4000], 0xeeee);
            assert_eq!(backend.words[&0x4004], FRAME);
            if fail_free {
                assert_eq!(backend.words[&(HEADER + 4)], 0x3000);
                assert_eq!(backend.words[&(HEADER + 8)], 0x3000);
                assert_eq!(backend.words[&(HEADER + 12)], 0x3004);
            } else {
                assert_eq!(backend.words[&(HEADER + 4)], 0x4000);
                assert_eq!(backend.words[&(HEADER + 8)], 0x4004);
                assert_eq!(backend.words[&(HEADER + 12)], 0x4008);
            }
        }
    }

    #[test]
    fn standalone_light_collector_uses_its_backend_storage() {
        let mut backend = CollectorBackend::new(false, false);
        let result = VehicleLightEnumeration {
            header: HEADER,
            memory: None,
        }
        .visit(&mut backend, FRAME);
        assert_eq!(result, Ok(0));
        assert_eq!(backend.events, ["allocate", "free"]);
        assert_eq!(backend.words[&0x4000], 0xeeee);
        assert_eq!(backend.words[&0x4004], FRAME);
        assert_eq!(backend.words[&(HEADER + 8)], 0x4008);
    }
}
