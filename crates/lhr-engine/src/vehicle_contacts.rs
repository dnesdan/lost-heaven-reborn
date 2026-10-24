//! Original `C_Vehicle::DoWheelsCollision` over addressable caller storage.
//!
//! PC entry RVA 0xe6af0, with a hidden vector result followed by timestep,
//! down direction and the suspension-impact flag. The original PS2 linker name
//! is `DoWheelsCollision__9C_VehicleFfRC8S_vectorb`. This module does not load a
//! scene, synthesize ground, or initialize vehicle/wheel/surface records.
//! Arithmetic is bounded to finite normal binary32 or zero under PC24/RN.
//! Errors retain all stores and callbacks already reached.
use crate::{
    collision_query::{CollisionLineOutputs, CollisionLineRequest},
    native_transform::SMatrix,
    vehicle_init::VehicleInitBackend,
};

#[path = "vehicle_contacts_response.rs"]
mod response;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleContactsUnavailable<E> {
    Effect(E),
    NumericDomain,
    /// A required original library operation is outside its admitted domain.
    Library(crate::native_transform::MakeUnavailable),
}

type Result<T, E> = std::result::Result<T, VehicleContactsUnavailable<E>>;

/// Descriptive host value for the original collision record pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleContactHit {
    pub primitive: u32,
}

/// Original impact calls reached by a newly bottomed suspension. Vector values
/// replace native temporary addresses; persistent addresses remain raw words.
#[derive(Debug, PartialEq)]
pub enum VehicleContactEffect<'a> {
    /// Historical label: this is original DoDamage, PC0x4d5610,
    /// called as (point,down,0.2,magnitude,0,0), not a particle-only effect.
    ImpactParticles {
        point: u32,
        down: u32,
        magnitude: f32,
    },
    /// PC 0x4d6720(magnitude).
    ImpactSound { magnitude: f32 },
    /// Fastcall vehicle+0x2f4 after ImpactSound: ECX=context, EDX=impulse.
    /// The first stack argument aliases that SAME mutable impulse; it is not
    /// another vector or a copy. Remaining stack arguments are point, 0.2,
    /// velocity, 0, 0, 0. Mutating this borrow is visible through both native
    /// references and to the subsequent damage threshold. `velocity` is the
    /// original temporary at S+0x68, whose lifetime ends after this callback.
    ImpactCallback {
        callback: u32,
        context: u32,
        impulse: &'a mut [f32; 3],
        point: u32,
        velocity: [f32; 3],
    },
    /// PC 0x4ee950(), when inverse-mass-scaled impulse exceeds 1.4.
    ImpactDamage,
}

/// These are actual external calls in the PC body. Implementations must use
/// the same addressable storage for callbacks which inspect or mutate records.
/// Missing dependencies must return an error, never a fabricated hit/miss.
pub trait VehicleContactsBackend: VehicleInitBackend {
    /// Original primitive property word, addressed by the opaque hit identity.
    /// A scene adapter can map a host identity without inventing native addresses.
    fn contact_primitive_properties(
        &mut self,
        primitive: u32,
    ) -> std::result::Result<u32, Self::Error> {
        self.read_word(primitive)
    }
    fn contact_primitive_material(
        &mut self,
        primitive: u32,
    ) -> std::result::Result<u8, Self::Error> {
        self.read_byte(primitive.wrapping_add(3))
    }
    /// Original g_collision::TestLineV, PC 0x5d5910. A miss preserves outputs.
    /// The primitive tag must support later reads of its original property word.
    fn contact_test_line_v(
        &mut self,
        request: CollisionLineRequest,
        outputs: &mut CollisionLineOutputs,
    ) -> std::result::Result<Option<VehicleContactHit>, Self::Error>;
    /// Original fastcall at vehicle+0x310. The return replaces the local hit,
    /// but does not replace the already stored wheel+0x124 pointer.
    fn contact_filter(
        &mut self,
        callback: u32,
        context: u32,
        primitive: u32,
        wheel_index: u32,
    ) -> std::result::Result<u32, Self::Error>;
    fn contact_update_frame_world(&mut self, frame: u32) -> std::result::Result<(), Self::Error>;
    /// PC 0x47aa40, the frame direction helper used for airborne steerable wheels.
    fn contact_frame_direction(&mut self, frame: u32)
        -> std::result::Result<[f32; 3], Self::Error>;
    /// Borrowed impact temporaries are valid only during this call. Preserve
    /// persistent backend mutations when returning an error. Callback impulse
    /// writes also occur directly in the caller's one local vector.
    fn contact_effect(
        &mut self,
        vehicle: u32,
        effect: VehicleContactEffect<'_>,
    ) -> std::result::Result<(), Self::Error>;
}

fn operand<E>(x: f32) -> Result<f32, E> {
    if x == 0. || x.is_normal() {
        Ok(x)
    } else {
        Err(VehicleContactsUnavailable::NumericDomain)
    }
}
fn arithmetic<E>(x: f32, wide: f64) -> Result<f32, E> {
    if !x.is_finite()
        || (wide != 0. && wide.abs() < f32::MIN_POSITIVE as f64)
        || wide.abs() > f32::MAX as f64
    {
        Err(VehicleContactsUnavailable::NumericDomain)
    } else {
        Ok(x)
    }
}
fn add<E>(a: f32, b: f32) -> Result<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a + b, a as f64 + b as f64)?;
    crate::native_precision::record_add(a, b, result);
    Ok(result)
}
fn sub<E>(a: f32, b: f32) -> Result<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a - b, a as f64 - b as f64)?;
    crate::native_precision::record_sub(a, b, result);
    Ok(result)
}
fn mul<E>(a: f32, b: f32) -> Result<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a * b, a as f64 * b as f64)?;
    crate::native_precision::record_mul(a, b, result);
    Ok(result)
}
fn div<E>(a: f32, b: f32) -> Result<f32, E> {
    let (a, b) = (operand(a)?, operand(b)?);
    let result = arithmetic(a / b, a as f64 / b as f64)?;
    crate::native_precision::record_div(a, b, result);
    Ok(result)
}
fn va<E>(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], E> {
    Ok([add(a[0], b[0])?, add(a[1], b[1])?, add(a[2], b[2])?])
}
fn vs<E>(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], E> {
    Ok([sub(a[0], b[0])?, sub(a[1], b[1])?, sub(a[2], b[2])?])
}
fn scale<E>(v: [f32; 3], s: f32) -> Result<[f32; 3], E> {
    Ok([mul(v[0], s)?, mul(v[1], s)?, mul(v[2], s)?])
}
fn cross<E>(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], E> {
    Ok([
        sub(mul(a[1], b[2])?, mul(a[2], b[1])?)?,
        sub(mul(a[2], b[0])?, mul(a[0], b[2])?)?,
        sub(mul(a[0], b[1])?, mul(a[1], b[0])?)?,
    ])
}
fn dot<E>(a: [f32; 3], b: [f32; 3]) -> Result<f32, E> {
    add(add(mul(a[2], b[2])?, mul(a[1], b[1])?)?, mul(a[0], b[0])?)
}
fn square_xyz<E>(a: [f32; 3]) -> Result<f32, E> {
    add(add(mul(a[0], a[0])?, mul(a[1], a[1])?)?, mul(a[2], a[2])?)
}
fn square_xzy<E>(a: [f32; 3]) -> Result<f32, E> {
    add(add(mul(a[0], a[0])?, mul(a[2], a[2])?)?, mul(a[1], a[1])?)
}
fn rw<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Result<u32, B::Error> {
    b.read_word(a).map_err(VehicleContactsUnavailable::Effect)
}
fn ww<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u32) -> Result<(), B::Error> {
    b.write_word(a, v)
        .map_err(VehicleContactsUnavailable::Effect)
}
fn rf<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Result<f32, B::Error> {
    operand(f32::from_bits(rw(b, a)?))
}
fn wf<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: f32) -> Result<(), B::Error> {
    ww(b, a, v.to_bits())
}
fn rv<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Result<[f32; 3], B::Error> {
    Ok([
        rf(b, a)?,
        rf(b, a.wrapping_add(4))?,
        rf(b, a.wrapping_add(8))?,
    ])
}
fn wv<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: [f32; 3]) -> Result<(), B::Error> {
    for (i, x) in v.into_iter().enumerate() {
        wf(b, a.wrapping_add(i as u32 * 4), x)?;
    }
    Ok(())
}
fn matrix<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Result<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    // The native affine vector helper reads only these twelve elements.
    for i in [0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14] {
        elements[i] = rf(b, a.wrapping_add(i as u32 * 4))?;
    }
    Ok(SMatrix { elements })
}
fn transform<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    a: u32,
    v: [f32; 3],
) -> Result<[f32; 3], B::Error> {
    matrix(b, a)?
        .try_transform_point(v)
        .map_err(|_| VehicleContactsUnavailable::NumericDomain)
}
fn wheel_at<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    v: u32,
    index: u32,
) -> Result<u32, B::Error> {
    let list = rw(b, v.wrapping_add(0xc38))?;
    rw(b, list.wrapping_add(index.wrapping_mul(4)))
}
fn wheel_origin<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    v: u32,
    w: u32,
) -> Result<[f32; 3], B::Error> {
    let shift = rf(b, w.wrapping_add(0xc))?;
    let zero = mul(shift, 0.)?;
    let p = [
        add(shift, rf(b, w.wrapping_add(0x10))?)?,
        add(zero, rf(b, w.wrapping_add(0x14))?)?,
        add(zero, rf(b, w.wrapping_add(0x18))?)?,
    ];
    transform(b, v.wrapping_add(0xce8), p)
}
fn suspension<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    w: u32,
    flags: u32,
) -> Result<f32, B::Error> {
    rf(
        b,
        w.wrapping_add(if (flags as i32) < 0 { 0x110 } else { 0x10c }),
    )
}
fn wheel_delta<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    w: u32,
    flags: u32,
    down: [f32; 3],
) -> Result<[f32; 3], B::Error> {
    let length = add(
        add(suspension(b, w, flags)?, rf(b, w.wrapping_add(0x8c))?)?,
        rf(b, w.wrapping_add(0x1c0))?,
    )?;
    scale(down, length)
}
fn wheel_offset<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    v: u32,
    w: u32,
    flags: u32,
) -> Result<[f32; 3], B::Error> {
    let length = add(suspension(b, w, flags)?, rf(b, w.wrapping_add(0x1c0))?)?;
    scale(rv(b, v.wrapping_add(0xc54))?, length)
}
#[derive(Clone, Copy)]
struct Plane {
    primitive: u32,
    normal: [f32; 3],
    distance: f32,
}
fn intersect<E>(
    plane: Plane,
    origin: [f32; 3],
    delta: [f32; 3],
    cached: bool,
) -> Result<Option<[f32; 3]>, E> {
    // Inline denominator uses X,Z,Y, while S_vector::Dot uses Z,Y,X.
    let denominator = add(
        add(
            mul(delta[0], plane.normal[0])?,
            mul(delta[2], plane.normal[2])?,
        )?,
        mul(delta[1], plane.normal[1])?,
    )?;
    if (denominator.abs() as f64) < 1e-8 {
        return Ok(None);
    }
    let t = -div(
        add(dot(plane.normal, origin)?, plane.distance)?,
        denominator,
    )?;
    let point = va(origin, scale(delta, t)?)?;
    let distance = vs(point, origin)?;
    let length2 = if cached {
        add(
            add(
                mul(distance[0], distance[0])?,
                mul(distance[2], distance[2])?,
            )?,
            mul(distance[1], distance[1])?,
        )?
    } else {
        dot(distance, distance)?
    };
    if length2 <= square_xzy(delta)? {
        Ok(Some(point))
    } else {
        Ok(None)
    }
}
type ContactCastHit = (u32, [f32; 3], [f32; 3]);
fn cast<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    v: u32,
    origin: [f32; 3],
    delta: [f32; 3],
) -> Result<Option<ContactCastHit>, B::Error> {
    let request = CollisionLineRequest {
        origin,
        delta,
        selector: rw(b, v.wrapping_add(0x9c))? as i32,
        reject_mask: rw(b, v.wrapping_add(0xa0))?,
    };
    let mut outputs = CollisionLineOutputs {
        point: Some([0.; 3]),
        normal: Some([0.; 3]),
    };
    let hit = b
        .contact_test_line_v(request, &mut outputs)
        .map_err(VehicleContactsUnavailable::Effect)?;
    Ok(hit.map(|h| (h.primitive, outputs.point.unwrap(), outputs.normal.unwrap())))
}
fn cache_pass<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    v: u32,
    down: [f32; 3],
    count: u32,
) -> Result<bool, B::Error> {
    let mut stationary = true;
    if rf(b, v.wrapping_add(0x59c))? >= f32::from_bits(0x3fc80347) {
        stationary = false;
    }
    if stationary {
        for i in (0..count).rev() {
            let w = wheel_at(b, v, i)?;
            if rw(b, w.wrapping_add(0x120))? & 0x40000000 != 0 {
                continue;
            }
            if square_xyz(rv(b, w.wrapping_add(0x4c))?)? > 1.5625
                || square_xyz(rv(b, w.wrapping_add(0x70))?)? > 0.0025
            {
                stationary = false;
                break;
            }
        }
    }
    if !stationary {
        return Ok(false);
    }
    for i in (0..count).rev() {
        let w = wheel_at(b, v, i)?;
        let flags = rw(b, w.wrapping_add(0x120))?;
        if flags & 0x40000000 != 0 || flags & 8 == 0 {
            continue;
        }
        let normal = rv(b, w.wrapping_add(0x40))?;
        let previous = rv(b, w.wrapping_add(0x28))?;
        let plane = Plane {
            primitive: 0,
            normal,
            distance: -dot(normal, previous)?,
        };
        let origin = wheel_origin(b, v, w)?;
        let delta = wheel_delta(b, w, flags, down)?;
        let Some(point) = intersect(plane, origin, delta, true)? else {
            continue;
        };
        ww(b, w.wrapping_add(0x120), flags | 0x20000000)?;
        wv(b, w.wrapping_add(0x28), point)?;
        let offset = va(wheel_offset(b, v, w, flags)?, vs(point, origin)?)?;
        let position = va(origin, offset)?;
        let difference = vs(position, rv(b, w.wrapping_add(0x1c))?)?;
        for (dst, delta) in [0x64, 0x70].into_iter().zip([difference, difference]) {
            for (j, d) in delta.into_iter().enumerate() {
                let a = w.wrapping_add(dst + j as u32 * 4);
                let value = add(d, rf(b, a)?)?;
                wf(b, a, value)?;
            }
        }
        wv(b, w.wrapping_add(0x1c), position)?;
    }
    Ok(true)
}
fn surface_index<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    table: u32,
    material: u32,
) -> Result<u32, B::Error> {
    let head = rw(b, 0x63d9c8)?;
    let descriptor = rw(
        b,
        head.wrapping_add(table.wrapping_mul(0x28))
            .wrapping_add(0x24),
    )?;
    let begin = rw(b, descriptor.wrapping_add(4))?;
    let count = if begin == 0 {
        0
    } else {
        (rw(b, descriptor.wrapping_add(8))?.wrapping_sub(begin) as i32 / 0x5c) as u32
    };
    let mut cursor = begin.wrapping_add(count.wrapping_mul(0x5c));
    for i in (0..count).rev() {
        cursor = cursor.wrapping_sub(0x5c);
        if rw(b, cursor)? == material {
            return Ok(i);
        }
    }
    Ok(u32::MAX)
}
/// Reconstruct the original placement/collision pass through PC 0x4e7c0d.
/// This is an explicitly separate stage, not whole-function success. The full
/// entry below continues it with frame, suspension and impact response.
pub fn vehicle_wheels_collision_placement<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    v: u32,
    down: [f32; 3],
) -> Result<(), B::Error> {
    let count = rw(b, v.wrapping_add(0x4c4))?;
    let mut planes = [None, None];
    let mut cached = false;
    if count != 0 && rw(b, v.wrapping_add(0x1a4))? & 0x2000000 == 0 {
        cached = cache_pass(b, v, down, count)?;
        if !cached && rw(b, v.wrapping_add(0xc68))? & 0x800 == 0 {
            let delta = scale(down, 3.)?;
            for (i, offset) in [0x4c, 0x58].into_iter().enumerate() {
                let local = rv(b, v.wrapping_add(offset))?;
                let origin = transform(b, v.wrapping_add(0xce8), local)?;
                if let Some((primitive, point, normal)) = cast(b, v, origin, delta)? {
                    // Original coarse probes use X,Z,Y accumulation.
                    let distance = -add(
                        add(mul(point[0], normal[0])?, mul(point[2], normal[2])?)?,
                        mul(point[1], normal[1])?,
                    )?;
                    planes[i] = Some(Plane {
                        primitive,
                        normal,
                        distance,
                    });
                }
            }
        }
    }
    let count = rw(b, v.wrapping_add(0x4c4))?;
    for i in (0..count).rev() {
        let w = wheel_at(b, v, i)?;
        let flags = rw(b, w.wrapping_add(0x120))?;
        if flags & 0x40000000 != 0
            || (flags & 0x20000000 != 0 && rw(b, v.wrapping_add(0x1a4))? & 0x2000000 == 0)
        {
            continue;
        }
        let delta = wheel_delta(b, w, flags, down)?;
        let origin = wheel_origin(b, v, w)?;
        let hit = if rw(b, v.wrapping_add(0xc68))? & 0x800 != 0 || cached {
            cast(b, v, origin, delta)?
        } else {
            let plane_index =
                usize::from(rf(b, w.wrapping_add(0x18))? <= rf(b, v.wrapping_add(0x40c))?);
            match planes[plane_index] {
                Some(plane) => intersect(plane, origin, delta, false)?
                    .map(|point| (plane.primitive, point, plane.normal)),
                None => None,
            }
        };
        let primitive = hit.map_or(0, |h| h.0);
        ww(b, w.wrapping_add(0x124), primitive)?;
        let callback = rw(b, v.wrapping_add(0x310))?;
        let mut filtered = primitive;
        if callback != 0 && primitive != 0 {
            let context = rw(b, v.wrapping_add(0xc74))?;
            filtered = b
                .contact_filter(callback, context, primitive, i)
                .map_err(VehicleContactsUnavailable::Effect)?;
        }
        let accepted = filtered != 0 && rw(b, w.wrapping_add(0xa8))? as i32 >= 0;
        let offset;
        if accepted {
            let (_, point, normal) = hit.unwrap();
            wv(b, w.wrapping_add(0x40), normal)?;
            wv(b, w.wrapping_add(0x28), point)?;
            let difference = vs(point, origin)?;
            let flags = rw(b, w.wrapping_add(0x120))?;
            let contact_flags = flags | 8;
            let contacted_offset = va(wheel_offset(b, v, w, flags)?, difference)?;
            ww(b, w.wrapping_add(0x120), contact_flags)?;
            let material = b
                .contact_primitive_material(filtered)
                .map_err(VehicleContactsUnavailable::Effect)? as u32;
            if rw(b, w.wrapping_add(0xb4))? != material {
                let table = rw(b, w.wrapping_add(0xac))?;
                if table as i32 > 0 {
                    let index = surface_index(b, table, material)?;
                    ww(b, w.wrapping_add(0xb0), index)?;
                }
                let reset = add(
                    add(rf(b, w.wrapping_add(0x1bc))?, rf(b, w.wrapping_add(0x1b8))?)?,
                    1.,
                )?;
                wf(b, w.wrapping_add(0x1c8), reset)?;
            }
            if rw(b, w.wrapping_add(0xac))? as i32 > 0 && (rw(b, w.wrapping_add(0xb0))? as i32) < 0
            {
                miss_point(b, v, w, origin, delta)?;
                offset = va(wheel_offset(b, v, w, contact_flags)?, delta)?;
                ww(b, w.wrapping_add(0x120), flags & !8)?;
            } else {
                offset = contacted_offset;
            }
            ww(b, w.wrapping_add(0xb4), material)?;
        } else {
            miss_point(b, v, w, origin, delta)?;
            let flags = rw(b, w.wrapping_add(0x120))?;
            offset = va(wheel_offset(b, v, w, flags)?, delta)?;
            ww(b, w.wrapping_add(0x120), flags & !8)?;
        }
        let position = va(origin, offset)?;
        let difference = vs(position, rv(b, w.wrapping_add(0x1c))?)?;
        for (j, d) in difference.into_iter().enumerate() {
            let a = w.wrapping_add(0x64 + j as u32 * 4);
            let value = add(d, rf(b, a)?)?;
            wf(b, a, value)?;
        }
        wv(b, w.wrapping_add(0x1c), position)?;
        wv(b, w.wrapping_add(0x70), [0.; 3])?;
    }
    Ok(())
}
fn miss_point<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    v: u32,
    w: u32,
    origin: [f32; 3],
    delta: [f32; 3],
) -> Result<(), B::Error> {
    let up = rv(b, v.wrapping_add(0xc54))?;
    let radius = rf(b, w.wrapping_add(0x1c0))?;
    // Original computes delta+origin, then adds this correction.
    let point = va(va(delta, origin)?, scale(up, radius)?)?;
    wv(b, w.wrapping_add(0x28), point)
}
/// Original entry. The vector result is committed only after both passes.
/// `output` is the hidden result buffer; errors leave it unchanged while
/// retaining partial caller-memory writes and external effects.
pub fn vehicle_do_wheels_collision<B: VehicleContactsBackend + ?Sized>(
    b: &mut B,
    vehicle: u32,
    timestep: f32,
    down: [f32; 3],
    suspension_impacts: bool,
    output: &mut [f32; 3],
) -> Result<(), B::Error> {
    vehicle_wheels_collision_placement(b, vehicle, down)?;
    *output = response::respond(b, vehicle, timestep, suspension_impacts)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    const BASE: u32 = 0x10000;
    const W: u32 = 0x16000;
    const F: u32 = 0x19000;
    const PRIMITIVE: u32 = 0x1a000;
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Error {
        Storage(u32),
        Unexpected,
        Filter,
        Impact,
    }
    #[derive(Clone, Copy)]
    struct ImpactTest {
        original_impulse: f32,
        replacement: Option<[f32; 3]>,
        persistent_write: Option<(u32, u32)>,
        fail: bool,
    }
    #[derive(Debug, PartialEq)]
    struct ImpactObservation {
        callback: u32,
        context: u32,
        impulse_before: [f32; 3],
        impulse_after: [f32; 3],
        point: u32,
        velocity: [f32; 3],
    }
    struct Backend {
        bytes: Vec<u8>,
        hit: Option<([f32; 3], [f32; 3])>,
        queries: Vec<CollisionLineRequest>,
        filter_error: bool,
        impact: Option<ImpactTest>,
        effects: Vec<&'static str>,
        observed_impact: Option<ImpactObservation>,
    }
    impl Backend {
        fn new() -> Self {
            Self {
                bytes: vec![0; 0x10000],
                hit: None,
                queries: Vec::new(),
                filter_error: false,
                impact: None,
                effects: Vec::new(),
                observed_impact: None,
            }
        }
        fn word(&self, a: u32) -> u32 {
            let o = (a - BASE) as usize;
            u32::from_le_bytes(self.bytes[o..o + 4].try_into().unwrap())
        }
        fn put(&mut self, a: u32, x: u32) {
            let o = (a - BASE) as usize;
            self.bytes[o..o + 4].copy_from_slice(&x.to_le_bytes());
        }
        fn float(&mut self, a: u32, x: f32) {
            self.put(a, x.to_bits());
        }
        fn vec(&mut self, a: u32, v: [f32; 3]) {
            for (i, x) in v.into_iter().enumerate() {
                self.float(a + i as u32 * 4, x);
            }
        }
        fn get_vec(&self, a: u32) -> [f32; 3] {
            [self.word(a), self.word(a + 4), self.word(a + 8)].map(f32::from_bits)
        }
        fn identity(&mut self, a: u32) {
            for (i, x) in SMatrix::identity().elements.into_iter().enumerate() {
                self.float(a + i as u32 * 4, x);
            }
        }
        fn wheel(&mut self) {
            self.put(BASE + 0x4c4, 1);
            self.put(BASE + 0xc38, BASE + 0x5000);
            self.put(BASE + 0x5000, W);
            self.put(BASE + 0x1a4, 0x2000000);
            self.put(BASE + 0xc68, 0x800);
            self.put(BASE + 0x9c, u32::MAX);
            self.identity(BASE + 0xce8);
            self.identity(BASE + 0xd28);
            self.identity(BASE + 0xd68);
            self.identity(BASE + 0xda8);
            self.vec(BASE + 0xc54, [0., 1., 0.]);
            self.vec(BASE + 0xc3c, [1., 0., 0.]);
            self.vec(BASE + 0xc48, [0., 0., -1.]);
            self.vec(BASE + 0x438, [1., 1., 1.]);
            self.float(BASE + 0x5f4, 2.);
            self.put(W + 8, F);
            self.put(F + 0xac, 0x20);
            self.float(W + 0x14, 1.);
            self.float(W + 0x8c, 0.3);
            self.float(W + 0x88, 10.);
            self.float(W + 0x10c, 0.5);
            self.float(W + 0x110, 0.4);
        }
    }
    impl VehicleInitBackend for Backend {
        type Error = Error;
        fn read_byte(&mut self, a: u32) -> std::result::Result<u8, Error> {
            self.bytes
                .get(a.wrapping_sub(BASE) as usize)
                .copied()
                .ok_or(Error::Storage(a))
        }
        fn read_word(&mut self, a: u32) -> std::result::Result<u32, Error> {
            let o = a.wrapping_sub(BASE) as usize;
            self.bytes
                .get(o..o + 4)
                .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
                .ok_or(Error::Storage(a))
        }
        fn write_byte(&mut self, a: u32, x: u8) -> std::result::Result<(), Error> {
            *self
                .bytes
                .get_mut(a.wrapping_sub(BASE) as usize)
                .ok_or(Error::Storage(a))? = x;
            Ok(())
        }
        fn write_word(&mut self, a: u32, x: u32) -> std::result::Result<(), Error> {
            let o = a.wrapping_sub(BASE) as usize;
            self.bytes
                .get_mut(o..o + 4)
                .ok_or(Error::Storage(a))?
                .copy_from_slice(&x.to_le_bytes());
            Ok(())
        }
        fn free(&mut self, _: u32) -> std::result::Result<(), Error> {
            Err(Error::Unexpected)
        }
        fn allocate(&mut self, _: u32) -> std::result::Result<u32, Error> {
            Err(Error::Unexpected)
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> std::result::Result<(), Error> {
            Err(Error::Unexpected)
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> std::result::Result<(), VehicleInitUnavailable<Error>> {
            Err(VehicleInitUnavailable::Effect(Error::Unexpected))
        }
    }
    impl VehicleContactsBackend for Backend {
        fn contact_test_line_v(
            &mut self,
            r: CollisionLineRequest,
            o: &mut CollisionLineOutputs,
        ) -> std::result::Result<Option<VehicleContactHit>, Error> {
            self.queries.push(r);
            Ok(self.hit.map(|(p, n)| {
                o.point = Some(p);
                o.normal = Some(n);
                VehicleContactHit {
                    primitive: PRIMITIVE,
                }
            }))
        }
        fn contact_filter(
            &mut self,
            _: u32,
            _: u32,
            p: u32,
            _: u32,
        ) -> std::result::Result<u32, Error> {
            assert_eq!(self.word(W + 0x124), p);
            if self.filter_error {
                Err(Error::Filter)
            } else {
                Ok(p)
            }
        }
        fn contact_update_frame_world(&mut self, _: u32) -> std::result::Result<(), Error> {
            Err(Error::Unexpected)
        }
        fn contact_frame_direction(&mut self, _: u32) -> std::result::Result<[f32; 3], Error> {
            Err(Error::Unexpected)
        }
        fn contact_effect(
            &mut self,
            vehicle: u32,
            effect: VehicleContactEffect<'_>,
        ) -> std::result::Result<(), Error> {
            assert_eq!(vehicle, BASE);
            let config = self.impact.ok_or(Error::Unexpected)?;
            match effect {
                VehicleContactEffect::ImpactParticles {
                    point,
                    down,
                    magnitude,
                } => {
                    assert_eq!(point, W + 0x1c);
                    assert_eq!(down, BASE + 0xc54);
                    assert_eq!(magnitude, config.original_impulse);
                    self.effects.push("particles");
                }
                VehicleContactEffect::ImpactSound { magnitude } => {
                    assert_eq!(magnitude, config.original_impulse);
                    self.effects.push("sound");
                }
                VehicleContactEffect::ImpactCallback {
                    callback,
                    context,
                    impulse,
                    point,
                    velocity,
                } => {
                    self.effects.push("callback");
                    let impulse_before = *impulse;
                    if let Some(replacement) = config.replacement {
                        *impulse = replacement;
                    }
                    self.observed_impact = Some(ImpactObservation {
                        callback,
                        context,
                        impulse_before,
                        impulse_after: *impulse,
                        point,
                        velocity,
                    });
                    if let Some((address, value)) = config.persistent_write {
                        self.put(address, value);
                    }
                    if config.fail {
                        return Err(Error::Impact);
                    }
                }
                VehicleContactEffect::ImpactDamage => self.effects.push("damage"),
            }
            Ok(())
        }
    }
    #[test]
    fn empty_wheel_set_ignores_unread_numeric_arguments() {
        let mut b = Backend::new();
        let before = b.bytes.clone();
        let mut output = [7.; 3];
        vehicle_do_wheels_collision(&mut b, BASE, f32::NAN, [f32::NAN; 3], true, &mut output)
            .unwrap();
        assert_eq!(output, [0.; 3]);
        assert_eq!(b.bytes, before);
        assert!(b.queries.is_empty());
    }
    #[test]
    fn original_displacement_uses_wheel_geometry_and_transformed_origin() {
        let mut b = Backend::new();
        b.wheel();
        b.float(W + 0xc, 2.);
        b.vec(BASE + 0xce8 + 48, [10., 20., 30.]);
        vehicle_wheels_collision_placement(&mut b, BASE, [0., -1., 0.]).unwrap();
        assert_eq!(b.queries.len(), 1);
        assert_eq!(b.queries[0].origin, [12., 21., 30.]);
        assert_eq!(b.queries[0].delta, [0., -0.8, 0.]);
        assert_eq!(b.get_vec(W + 0x28), [12., 20.2, 30.]);
        assert_eq!(b.get_vec(W + 0x1c), [12., 20.7, 30.]);
    }
    #[test]
    fn failed_filter_preserves_native_pointer_store_and_defers_result() {
        let mut b = Backend::new();
        b.wheel();
        b.hit = Some(([0., 0.5, 0.], [0., 1., 0.]));
        b.put(BASE + 0x310, 0x1234);
        b.filter_error = true;
        let before = b.get_vec(W + 0x1c);
        let mut output = [7.; 3];
        assert_eq!(
            vehicle_do_wheels_collision(&mut b, BASE, 0.02, [0., -1., 0.], false, &mut output),
            Err(VehicleContactsUnavailable::Effect(Error::Filter))
        );
        assert_eq!(b.word(W + 0x124), PRIMITIVE);
        assert_eq!(b.get_vec(W + 0x1c), before);
        assert_eq!(output, [7.; 3]);
    }
    #[test]
    fn bottomed_wheel_cancels_normal_velocity_with_mass_scaled_impulse() {
        let mut b = Backend::new();
        b.wheel();
        b.hit = Some(([0., 1., 0.], [0., 1., 0.]));
        b.vec(BASE + 0x398, [0., 1., 0.]);
        b.vec(BASE + 0x1f90, [0., -1., 0.]);
        let mut output = [7.; 3];
        vehicle_do_wheels_collision(&mut b, BASE, 0.02, [0., -1., 0.], true, &mut output).unwrap();
        assert_eq!(b.get_vec(BASE + 0x1f90), [0.; 3]);
        assert_eq!(b.get_vec(BASE + 0x420), [0.; 3]);
        assert_eq!(b.get_vec(W + 0x4c), [0.; 3]);
        assert_eq!(output, [1., 0., 0.]);
        assert_ne!(b.word(W + 0x120) & 0x40, 0);
        assert_eq!(f32::from_bits(b.word(W + 0x90)), 3.);
    }
    fn impact_backend(config: ImpactTest) -> Backend {
        let mut b = Backend::new();
        b.wheel();
        b.hit = Some(([1., 1., 0.], [0., 1., 0.]));
        b.vec(BASE + 0x398, [0., 1., 0.]);
        b.vec(BASE + 0x1f90, [0., -1.5 * config.original_impulse, 0.]);
        b.put(BASE + 0xc68, 0xc820);
        b.put(BASE + 0x2f4, 0x1234);
        b.put(BASE + 0xc74, 0x5678);
        b.bytes[0x432] = 1;
        b.impact = Some(config);
        b
    }

    #[test]
    fn impact_callback_mutates_one_shared_impulse_before_damage_only() {
        for (original_impulse, replacement, mass_after, damage) in [
            (2., None, None, false),
            (2., Some([0., 8., 0.]), None, true),
            (4., None, None, true),
            (4., Some([0.; 3]), None, false),
            (4., Some([0., 4., 0.]), Some(4_f32), false),
        ] {
            let mut b = impact_backend(ImpactTest {
                original_impulse,
                replacement,
                persistent_write: mass_after.map(|mass| (BASE + 0x5f4, mass.to_bits())),
                fail: false,
            });
            let mut output = [7.; 3];
            vehicle_do_wheels_collision(&mut b, BASE, 0.02, [0., -1., 0.], true, &mut output)
                .unwrap();
            assert_eq!(
                b.observed_impact,
                Some(ImpactObservation {
                    callback: 0x1234,
                    context: 0x5678,
                    impulse_before: [0., original_impulse, 0.],
                    impulse_after: replacement.unwrap_or([0., original_impulse, 0.]),
                    point: W + 0x1c,
                    velocity: [0., -1.5 * original_impulse, 0.],
                })
            );
            let mut expected = vec!["particles", "sound", "callback"];
            if damage {
                expected.push("damage");
            }
            assert_eq!(b.effects, expected);
            // Both accumulators were updated before the callback. Replacing
            // its shared local affects Damage, never these earlier impulses.
            assert_eq!(b.get_vec(BASE + 0x1f90), [0., -original_impulse, 0.]);
            assert_eq!(b.get_vec(BASE + 0x420), [0., 0., -original_impulse]);
            assert_eq!(output, [1., 0., 0.]);
        }
    }

    #[test]
    fn unavailable_impact_callback_preserves_writes_and_defers_local_deltas() {
        let mut b = impact_backend(ImpactTest {
            original_impulse: 4.,
            replacement: Some([0.; 3]),
            persistent_write: Some((BASE + 0x300, 0xfeed_beef)),
            fail: true,
        });
        let mut output = [7.; 3];
        assert_eq!(
            vehicle_do_wheels_collision(&mut b, BASE, 0.02, [0., -1., 0.], true, &mut output),
            Err(VehicleContactsUnavailable::Effect(Error::Impact))
        );
        assert_eq!(b.word(BASE + 0x300), 0xfeed_beef);
        assert_eq!(b.effects, ["particles", "sound", "callback"]);
        assert_eq!(b.observed_impact.as_ref().unwrap().impulse_after, [0.; 3]);
        assert_ne!(b.word(W + 0x120) & 0x40, 0);
        assert_eq!(f32::from_bits(b.word(F + 0x84)), 1.);
        assert_eq!(b.get_vec(BASE + 0x1f90), [0., -6., 0.]);
        assert_eq!(b.get_vec(BASE + 0x420), [0.; 3]);
        assert_eq!(output, [7.; 3]);
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn rolling_wheel_angles_keep_the_original_periodic_contact_direction() {
        for angle in [0., std::f32::consts::TAU, 100.] {
            let mut b = Backend::new();
            b.wheel();
            b.hit = Some(([0., 0.5, 0.], [0., 1., 0.]));
            b.float(W + 0x188, 0.05);
            b.float(W + 0x114, angle);
            let mut output = [7.; 3];
            vehicle_do_wheels_collision(&mut b, BASE, 0.02, [0., -1., 0.], false, &mut output)
                .unwrap();
            assert!(output.into_iter().all(f32::is_finite));
            assert!((output.into_iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 0.000001);
        }
    }
}
