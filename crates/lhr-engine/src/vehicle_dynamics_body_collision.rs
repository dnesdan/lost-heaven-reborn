//! Original PC C_Vehicle::BodyCollision, canonical RVA e8af0. Candidate sorting,
//! coarse/fine swept queries and static response share the caller's address map.
//! Dynamic owners and nested volumes retain their explicit typed boundaries.
use crate::native_transform::SMatrix;
use crate::vehicle_dynamics_body::VehicleBodyCollisionCall;
use crate::vehicle_dynamics_prepare::{
    add, div, float, mul, number, result, store_float, sub, word,
};
use crate::vehicle_init::VehicleInitBackend;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehicleBodyCollisionFilter {
    Group(u32),
    Actors { begin: u32, count: u32 },
    Clear,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleBodySphereSweep {
    pub record: u32,
    pub start: [f32; 3],
    pub displacement: [f32; 3],
    pub radius: f32,
    pub mask: u32,
    pub flags: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleBodyVolumeSweep {
    pub record: u32,
    pub volume: u32,
    pub previous: SMatrix,
    pub current: SMatrix,
    pub displacement: [f32; 3],
    pub auxiliary: [f32; 3],
    pub up: [f32; 3],
    pub mask: u32,
    pub flags: u32,
}
pub trait VehicleBodyCollisionBackend: VehicleInitBackend {
    /// Original static scratch-vector initialization/clear. Its ownership,
    /// allocator and atexit registration are host dependencies, not fabricated
    /// native byte markers. Existing allocations must be retained for reuse.
    fn body_collision_initialize(&mut self) -> std::result::Result<(), Self::Error>;
    fn body_collision_clear_order(&mut self) -> std::result::Result<(), Self::Error>;
    /// Original sorted-vector insertion, PC RVA eaa00. Header is the actual
    /// global body/fine header; position is an element index. The adapter must
    /// preserve its allocator/marker word at +0 and begin/end/capacity at +4,
    /// +8 and +0xc, respectively. It must
    /// evaluate the live begin pointer, perform native ownership/allocation
    /// effects, then insert the two words (record index and distance bits).
    fn body_collision_insert(
        &mut self,
        header: u32,
        position: u32,
        record_index: u32,
        distance: f32,
    ) -> std::result::Result<(), Self::Error>;
    fn body_collision_update_world(&mut self, frame: u32) -> std::result::Result<(), Self::Error>;
    fn body_collision_filter(
        &mut self,
        filter: VehicleBodyCollisionFilter,
    ) -> std::result::Result<(), Self::Error>;
    /// Original swept sphere, not the line intersection approximation.
    fn body_collision_sphere(
        &mut self,
        query: VehicleBodySphereSweep,
    ) -> std::result::Result<bool, Self::Error>;
    fn body_collision_volume(
        &mut self,
        query: VehicleBodyVolumeSweep,
    ) -> std::result::Result<bool, Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleBodyCollisionUnavailable<E> {
    Effect(E),
    NumericDomain,
    StorageRange { begin: u32, end: u32 },
    StaticResponse(crate::vehicle_dynamics_body_response::VehicleBodyStaticUnavailable<E>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleBodySweepHit {
    pub descriptor: u32,
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// Additional original operations reached after a coarse body hit. A nested
/// call must share the original scratch owner; it cannot be a no-op callback.
pub trait VehicleBodyCollisionResponseBackend:
    VehicleBodyCollisionBackend
    + crate::vehicle_dynamics_body_response::VehicleBodyStaticResponseBackend
{
    fn body_collision_clear_fine_order(&mut self) -> std::result::Result<(), Self::Error>;
    fn body_collision_fine_sphere(
        &mut self,
        query: VehicleBodySphereSweep,
    ) -> std::result::Result<Option<VehicleBodySweepHit>, Self::Error>;
    /// Original virtual/callback at vehicle+30c, context at vehicle+c74. It can
    /// reject the hit by returning zero or map its descriptor to actor storage.
    /// Position and normal are the original mutable reference arguments. The
    /// backend preserves writes to the shared normal scratch even on failure.
    fn body_collision_map_descriptor(
        &mut self,
        vehicle: u32,
        function: u32,
        context: u32,
        hit: &mut VehicleBodySweepHit,
    ) -> std::result::Result<u32, Self::Error>;
    fn body_collision_nested(
        &mut self,
        vehicle: u32,
        header: u32,
        call: &mut VehicleBodyCollisionCall,
    ) -> std::result::Result<bool, Self::Error>;
}
type Out<T, E> = std::result::Result<T, VehicleBodyCollisionUnavailable<E>>;
impl<E> From<crate::vehicle_dynamics_prepare::VehicleMovePrepareUnavailable<E>>
    for VehicleBodyCollisionUnavailable<E>
{
    fn from(e: crate::vehicle_dynamics_prepare::VehicleMovePrepareUnavailable<E>) -> Self {
        match e {
            crate::vehicle_dynamics_prepare::VehicleMovePrepareUnavailable::Effect(e) => {
                Self::Effect(e)
            }
            crate::vehicle_dynamics_prepare::VehicleMovePrepareUnavailable::NumericDomain => {
                Self::NumericDomain
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleBodyCollisionOutcome {
    NoContact,
    Contact,
    /// The original next enumerates/sorts fine spheres and performs impulse,
    /// dynamic-body, damage and recursive collision response.
    ContactResponseRequired {
        record: u32,
        sphere_hit: bool,
        volume_hit: bool,
    },
}
fn vector<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([
        float(b, a)?,
        float(b, a.wrapping_add(4))?,
        float(b, a.wrapping_add(8))?,
    ])
}
fn put_vector<B: VehicleInitBackend>(b: &mut B, a: u32, v: [f32; 3]) -> Out<(), B::Error> {
    for (i, x) in v.into_iter().enumerate() {
        store_float(b, a.wrapping_add(i as u32 * 4), x)?;
    }
    Ok(())
}
fn matrix<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    for (i, x) in elements.iter_mut().enumerate() {
        *x = float(b, a.wrapping_add(i as u32 * 4))?;
    }
    Ok(SMatrix { elements })
}
fn transform<E>(m: SMatrix, v: [f32; 3]) -> Out<[f32; 3], E> {
    m.try_transform_point(v)
        .map_err(|_| VehicleBodyCollisionUnavailable::NumericDomain)
}
fn squared<E>(v: [f32; 3]) -> Out<f32, E> {
    Ok(add(
        add(mul(v[0], v[0])?, mul(v[2], v[2])?)?,
        mul(v[1], v[1])?,
    )?)
}
fn normalized<E>(mut v: [f32; 3]) -> Out<[f32; 3], E> {
    let s = squared(v)?;
    if f64::from(sub(s, 1.)?.abs()) >= 1e-8 {
        if s >= f32::from_bits(0x322bcc77) {
            let r = div(1., square_root(s)?)?;
            for x in &mut v {
                *x = mul(*x, r)?;
            }
        } else {
            let i = if v[0] != 0. {
                0
            } else if v[2] != 0. {
                2
            } else {
                1
            };
            v[i] = if v[i] < 0. { -1. } else { 1. };
        }
    }
    Ok(v)
}
fn square_root<E>(s: f32) -> Out<f32, E> {
    let value = result(s.sqrt(), f64::from(s).sqrt())?;
    crate::native_precision::record_sqrt(s, value);
    Ok(value)
}
fn length<E>(v: [f32; 3]) -> Out<f32, E> {
    // Original candidate length adds z²+y²+x².
    let s = add(add(mul(v[2], v[2])?, mul(v[1], v[1])?)?, mul(v[0], v[0])?)?;
    if (s.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if f64::from(sub(s, 1.)?.abs()) < 1e-8 {
        Ok(1.)
    } else {
        Ok(square_root(s)?)
    }
}

// The two original scratch vectors are shared across callbacks and recursive
// BodyCollision calls. Read their canonical headers/entries, never a detached
// Rust ordering snapshot. An insertion backend must publish these same words.
fn order_count<B: VehicleInitBackend>(b: &mut B, header: u32) -> Out<u32, B::Error> {
    let begin = word(b, header.wrapping_add(4))?;
    if begin == 0 {
        return Ok(0);
    }
    let end = word(b, header.wrapping_add(8))?;
    let span = end
        .checked_sub(begin)
        .filter(|span| span % 8 == 0)
        .ok_or(VehicleBodyCollisionUnavailable::StorageRange { begin, end })?;
    Ok(span / 8)
}
fn order_insertion<B: VehicleInitBackend>(
    b: &mut B,
    header: u32,
    distance: f32,
) -> Out<u32, B::Error> {
    let begin = word(b, header.wrapping_add(4))?;
    if begin == 0 {
        return Ok(0);
    }
    let end = word(b, header.wrapping_add(8))?;
    let mut remaining = end
        .checked_sub(begin)
        .filter(|span| span % 8 == 0)
        .ok_or(VehicleBodyCollisionUnavailable::StorageRange { begin, end })?
        / 8;
    while remaining != 0 {
        remaining -= 1;
        let key = number(float(
            b,
            begin
                .wrapping_add(remaining.wrapping_mul(8))
                .wrapping_add(4),
        )?)?;
        if distance < key {
            return Ok(remaining + 1);
        }
    }
    Ok(0)
}
fn order_index<B: VehicleInitBackend>(
    b: &mut B,
    header: u32,
    remaining: u32,
) -> Out<u32, B::Error> {
    let begin = word(b, header.wrapping_add(4))?;
    Ok(word(b, begin.wrapping_add(remaining.wrapping_mul(8)))?)
}

/// Caller/header admission is explicit because the original operates on valid
/// C++ vectors. The public Rust host rejects malformed ranges before indexing.
/// Native allocator/global scratch parity is separate from this query route.
fn candidate_pass<B, F>(
    b: &mut B,
    vehicle: u32,
    header: u32,
    call: &mut VehicleBodyCollisionCall,
    mut response: F,
) -> Out<VehicleBodyCollisionOutcome, B::Error>
where
    B: VehicleBodyCollisionBackend,
    F: FnMut(
        &mut B,
        u32,
        SMatrix,
        [f32; 3],
        &mut VehicleBodyCollisionCall,
    ) -> Out<Option<bool>, B::Error>,
{
    b.body_collision_initialize()
        .map_err(VehicleBodyCollisionUnavailable::Effect)?;
    let frame = word(b, vehicle.wrapping_add(0x34c))?;
    if b.read_byte(frame.wrapping_add(0xac))
        .map_err(VehicleBodyCollisionUnavailable::Effect)?
        & 0x20
        == 0
    {
        b.body_collision_update_world(frame)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
    }
    let current = matrix(b, frame.wrapping_add(0x10))?;
    let begin = word(b, header.wrapping_add(4))?;
    let count = if begin == 0 {
        0
    } else {
        let end = word(b, header.wrapping_add(8))?;
        let span = end
            .checked_sub(begin)
            .ok_or(VehicleBodyCollisionUnavailable::StorageRange { begin, end })?;
        if span % 0x74 != 0 {
            return Err(VehicleBodyCollisionUnavailable::StorageRange { begin, end });
        }
        span / 0x74
    };
    for index in (0..count).rev() {
        let records = word(b, header.wrapping_add(4))?;
        let record = records.wrapping_add(index.wrapping_mul(0x74));
        let local = vector(b, record.wrapping_add(0x24))?;
        let previous = matrix(b, vehicle.wrapping_add(0xde8))?;
        let start = transform(previous, local)?;
        put_vector(b, record.wrapping_add(0x48), start)?;
        let local = vector(b, record.wrapping_add(0x24))?;
        let end = transform(current, local)?;
        put_vector(b, record.wrapping_add(0x30), end)?;
        for i in 0..3 {
            let a = record.wrapping_add(0x30 + i * 4);
            let old = float(b, record.wrapping_add(0x48 + i * 4))?;
            let value = sub(float(b, a)?, old)?;
            store_float(b, a, value)?;
        }
    }
    let velocity = normalized(vector(b, vehicle.wrapping_add(0x1f90))?)?;
    let mut distant = [0.; 3];
    for (i, x) in distant.iter_mut().enumerate() {
        *x = add(
            mul(velocity[i], 1000.)?,
            float(b, vehicle.wrapping_add(0x398 + i as u32 * 4))?,
        )?;
    }
    let inverse = matrix(b, vehicle.wrapping_add(0xd28))?;
    let distant = crate::vehicle_dynamics_body::transform_assign(inverse, distant)?;
    b.body_collision_clear_order()
        .map_err(VehicleBodyCollisionUnavailable::Effect)?;
    let begin = word(b, header.wrapping_add(4))?;
    let count = if begin == 0 {
        0
    } else {
        let end = word(b, header.wrapping_add(8))?;
        let span = end
            .checked_sub(begin)
            .ok_or(VehicleBodyCollisionUnavailable::StorageRange { begin, end })?;
        if span % 0x74 != 0 {
            return Err(VehicleBodyCollisionUnavailable::StorageRange { begin, end });
        }
        span / 0x74
    };
    for index in (0..count).rev() {
        let begin = word(b, header.wrapping_add(4))?;
        let record = begin.wrapping_add(index.wrapping_mul(0x74));
        let center = vector(b, record.wrapping_add(0x24))?;
        let displacement = [
            sub(distant[0], center[0])?,
            sub(distant[1], center[1])?,
            sub(distant[2], center[2])?,
        ];
        let distance = sub(length(displacement)?, float(b, record.wrapping_add(0x54))?)?;
        // Original insertion scans backward until new<old, then inserts after
        // that element. Equality therefore places the new record before ties.
        let insertion = order_insertion(b, 0x63d9fc, distance)?;
        b.body_collision_insert(0x63d9fc, insertion, index, distance)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
    }
    let mut any_response = false;
    let mut remaining = order_count(b, 0x63d9fc)?;
    while remaining != 0 {
        remaining -= 1;
        let index = order_index(b, 0x63d9fc, remaining)?;
        let begin = word(b, header.wrapping_add(4))?;
        let record = begin.wrapping_add(index.wrapping_mul(0x74));
        let filter = if b
            .read_byte(vehicle.wrapping_add(0x1faa))
            .map_err(VehicleBodyCollisionUnavailable::Effect)?
            == 0
        {
            VehicleBodyCollisionFilter::Group(vehicle.wrapping_add(0x22c))
        } else {
            let begin = word(b, vehicle.wrapping_add(0x1fd4))?;
            let count = if begin == 0 {
                0
            } else {
                word(b, vehicle.wrapping_add(0x1fd8))?.wrapping_sub(begin) >> 2
            };
            VehicleBodyCollisionFilter::Actors { begin, count }
        };
        b.body_collision_filter(filter)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
        let volume = b
            .read_byte(record.wrapping_add(0x58))
            .map_err(VehicleBodyCollisionUnavailable::Effect)?
            != 0;
        let (sphere_hit, volume_hit) = if !volume {
            let query = VehicleBodySphereSweep {
                record,
                start: vector(b, record.wrapping_add(0x48))?,
                displacement: vector(b, record.wrapping_add(0x30))?,
                radius: number(float(b, record.wrapping_add(0x54))?)?,
                mask: word(b, vehicle.wrapping_add(0xa4))?,
                flags: word(b, vehicle.wrapping_add(0xa8))?,
            };
            (
                b.body_collision_sphere(query)
                    .map_err(VehicleBodyCollisionUnavailable::Effect)?,
                false,
            )
        } else {
            let query = VehicleBodyVolumeSweep {
                record,
                volume: record.wrapping_add(0x5c),
                previous: matrix(b, vehicle.wrapping_add(0xde8))?,
                current,
                displacement: vector(b, record.wrapping_add(0x30))?,
                auxiliary: call.auxiliary_vector,
                up: vector(b, vehicle.wrapping_add(0xc54))?,
                mask: word(b, vehicle.wrapping_add(0xa4))?,
                flags: word(b, vehicle.wrapping_add(0xa8))?,
            };
            (
                false,
                b.body_collision_volume(query)
                    .map_err(VehicleBodyCollisionUnavailable::Effect)?,
            )
        };
        b.body_collision_filter(VehicleBodyCollisionFilter::Clear)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
        if sphere_hit || volume_hit {
            match response(b, record, current, distant, call)? {
                Some(hit) => any_response |= hit,
                None => {
                    return Ok(VehicleBodyCollisionOutcome::ContactResponseRequired {
                        record,
                        sphere_hit,
                        volume_hit,
                    });
                }
            }
        }
    }
    Ok(if any_response {
        VehicleBodyCollisionOutcome::Contact
    } else {
        VehicleBodyCollisionOutcome::NoContact
    })
}

/// Complete original candidate-miss route; a coarse hit exposes the response boundary.
pub fn vehicle_body_collision_no_contact<B: VehicleBodyCollisionBackend>(
    b: &mut B,
    vehicle: u32,
    call: &mut VehicleBodyCollisionCall,
) -> Out<VehicleBodyCollisionOutcome, B::Error> {
    candidate_pass(b, vehicle, call.collision_records, call, |_, _, _, _, _| {
        Ok(None)
    })
}

fn fine_count<B: VehicleInitBackend>(b: &mut B, record: u32) -> Out<u32, B::Error> {
    let begin = word(b, record.wrapping_add(4))?;
    Ok(if begin == 0 {
        0
    } else {
        let end = word(b, record.wrapping_add(8))?;
        let span = end
            .checked_sub(begin)
            .ok_or(VehicleBodyCollisionUnavailable::StorageRange { begin, end })?;
        if span % 0x3c != 0 {
            return Err(VehicleBodyCollisionUnavailable::StorageRange { begin, end });
        }
        span / 0x3c
    })
}

fn fine_response<B: VehicleBodyCollisionResponseBackend>(
    b: &mut B,
    vehicle: u32,
    record: u32,
    current: SMatrix,
    distant: [f32; 3],
    call: &mut VehicleBodyCollisionCall,
) -> Out<bool, B::Error> {
    use crate::vehicle_dynamics_body_response::{
        vehicle_body_static_response, VehicleBodyStaticContact,
    };
    b.body_collision_clear_fine_order()
        .map_err(VehicleBodyCollisionUnavailable::Effect)?;
    let count = fine_count(b, record)?;
    for index in (0..count).rev() {
        let begin = word(b, record.wrapping_add(4))?;
        let sphere = begin.wrapping_add(index.wrapping_mul(0x3c));
        let center = vector(b, sphere.wrapping_add(4))?;
        let displacement = [
            sub(distant[0], center[0])?,
            sub(distant[1], center[1])?,
            sub(distant[2], center[2])?,
        ];
        let distance = sub(length(displacement)?, float(b, sphere.wrapping_add(0x34))?)?;
        let insertion = order_insertion(b, 0x63d9ec, distance)?;
        b.body_collision_insert(0x63d9ec, insertion, index, distance)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
    }
    let mut responded = false;
    // The native loop reloads the record range after all insertion effects;
    // its per-iteration indices still come from the shared global ordering.
    let mut remaining = fine_count(b, record)?;
    while remaining != 0 {
        remaining -= 1;
        let index = order_index(b, 0x63d9ec, remaining)?;
        let begin = word(b, record.wrapping_add(4))?;
        let sphere = begin.wrapping_add(index.wrapping_mul(0x3c));
        let local = vector(b, sphere.wrapping_add(4))?;
        let previous = matrix(b, vehicle.wrapping_add(0xde8))?;
        let start = transform(previous, local)?;
        put_vector(b, sphere.wrapping_add(0x28), start)?;
        let local = vector(b, sphere.wrapping_add(4))?;
        let end = transform(current, local)?;
        put_vector(b, sphere.wrapping_add(0x10), end)?;
        for i in 0..3 {
            let a = sphere.wrapping_add(0x10 + i * 4);
            let old = float(b, sphere.wrapping_add(0x28 + i * 4))?;
            let value = sub(float(b, a)?, old)?;
            store_float(b, a, value)?;
        }
        let filter = if b
            .read_byte(vehicle.wrapping_add(0x1faa))
            .map_err(VehicleBodyCollisionUnavailable::Effect)?
            == 0
        {
            VehicleBodyCollisionFilter::Group(vehicle.wrapping_add(0x22c))
        } else {
            let begin = word(b, vehicle.wrapping_add(0x1fd4))?;
            let count = if begin == 0 {
                0
            } else {
                word(b, vehicle.wrapping_add(0x1fd8))?.wrapping_sub(begin) >> 2
            };
            VehicleBodyCollisionFilter::Actors { begin, count }
        };
        b.body_collision_filter(filter)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
        let query = VehicleBodySphereSweep {
            record: sphere,
            start: vector(b, sphere.wrapping_add(0x28))?,
            displacement: vector(b, sphere.wrapping_add(0x10))?,
            radius: number(float(b, sphere.wrapping_add(0x34))?)?,
            mask: word(b, vehicle.wrapping_add(0xa4))?,
            flags: word(b, vehicle.wrapping_add(0xa8))?,
        };
        let hit = b
            .body_collision_fine_sphere(query)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
        b.body_collision_filter(VehicleBodyCollisionFilter::Clear)
            .map_err(VehicleBodyCollisionUnavailable::Effect)?;
        if let Some(mut hit) = hit {
            let callback = word(b, vehicle.wrapping_add(0x30c))?;
            let descriptor = if callback == 0 {
                hit.descriptor
            } else {
                let context = word(b, vehicle.wrapping_add(0xc74))?;
                b.body_collision_map_descriptor(vehicle, callback, context, &mut hit)
                    .map_err(VehicleBodyCollisionUnavailable::Effect)?
            };
            if descriptor != 0 {
                let contact = VehicleBodyStaticContact {
                    sphere_record: sphere,
                    descriptor,
                    previous_center: vector(b, sphere.wrapping_add(0x28))?,
                    resolved_center: hit.position,
                    normal: hit.normal,
                    radius: number(float(b, sphere.wrapping_add(0x34))?)?,
                };
                responded |= vehicle_body_static_response(b, vehicle, contact, call)
                    .map_err(VehicleBodyCollisionUnavailable::StaticResponse)?;
            }
        }
    }
    let children = word(b, record.wrapping_add(0x14))?;
    if children != 0 {
        let end = word(b, record.wrapping_add(0x18))?;
        if end != children {
            responded |= b
                .body_collision_nested(vehicle, record.wrapping_add(0x10), call)
                .map_err(VehicleBodyCollisionUnavailable::Effect)?;
        }
    }
    Ok(responded)
}

/// Original body candidates plus fine static-sphere response. Dynamic contacts
/// and nested calls are explicit dependencies retaining their completed prefix.
/// Scratch allocation and global bytes belong to the backend's canonical
/// original owner; scans and traversal observe that same live storage.
pub fn vehicle_body_collision<B: VehicleBodyCollisionResponseBackend>(
    b: &mut B,
    vehicle: u32,
    call: &mut VehicleBodyCollisionCall,
) -> Out<bool, B::Error> {
    let header = call.collision_records;
    let outcome = candidate_pass(
        b,
        vehicle,
        header,
        call,
        |b, record, current, distant, call| {
            Ok(Some(fine_response(
                b, vehicle, record, current, distant, call,
            )?))
        },
    )?;
    Ok(matches!(outcome, VehicleBodyCollisionOutcome::Contact))
}

/// Host availability errors of the canonical single-element collision-order
/// insertion. The original body caller always supplies count one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleCollisionOrderUnavailable<E> {
    Effect(E),
    StorageRange,
    AllocationFailed,
}
struct CollisionOrderAccess<'a, B: ?Sized>(&'a mut B);
impl<B: VehicleInitBackend + ?Sized> CollisionOrderAccess<'_, B> {
    fn read_word(
        &mut self,
        a: u32,
    ) -> std::result::Result<u32, VehicleCollisionOrderUnavailable<B::Error>> {
        self.0
            .read_word(a)
            .map_err(VehicleCollisionOrderUnavailable::Effect)
    }
    fn write_word(
        &mut self,
        a: u32,
        value: u32,
    ) -> std::result::Result<(), VehicleCollisionOrderUnavailable<B::Error>> {
        self.0
            .write_word(a, value)
            .map_err(VehicleCollisionOrderUnavailable::Effect)
    }
    fn allocate(
        &mut self,
        n: u32,
    ) -> std::result::Result<u32, VehicleCollisionOrderUnavailable<B::Error>> {
        self.0
            .allocate(n)
            .map_err(VehicleCollisionOrderUnavailable::Effect)
    }
    fn free(
        &mut self,
        a: u32,
    ) -> std::result::Result<(), VehicleCollisionOrderUnavailable<B::Error>> {
        self.0
            .free(a)
            .map_err(VehicleCollisionOrderUnavailable::Effect)
    }
}
/// Original 4eaa00 collision-order insertion, restricted to the reached
/// eight-byte, count-one call contract. Position is an element index. Header
/// reloads after allocator/free effects preserve the original publication order.
pub fn insert_collision_order<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    h: u32,
    position: u32,
    index: u32,
    distance: f32,
) -> std::result::Result<(), VehicleCollisionOrderUnavailable<B::Error>> {
    let mut access = CollisionOrderAccess(b);
    let begin = access.read_word(h + 4)?;
    let end = access.read_word(h + 8)?;
    let capacity = access.read_word(h + 12)?;
    let length = if begin == 0 {
        0
    } else {
        end.checked_sub(begin)
            .filter(|n| n % 8 == 0)
            .ok_or(VehicleCollisionOrderUnavailable::StorageRange)?
            / 8
    };
    if position > length {
        return Err(VehicleCollisionOrderUnavailable::StorageRange);
    }
    if capacity.wrapping_sub(end) / 8 >= 1 {
        for i in (position..length).rev() {
            let a = access.read_word(begin + i * 8)?;
            let b = access.read_word(begin + i * 8 + 4)?;
            access.write_word(begin + (i + 1) * 8, a)?;
            access.write_word(begin + (i + 1) * 8 + 4, b)?;
        }
        access.write_word(begin + position * 8, index)?;
        access.write_word(begin + position * 8 + 4, distance.to_bits())?;
        let end = access.read_word(h + 8)?;
        return access.write_word(h + 8, end + 8);
    }
    let slots = length
        .checked_add(length.max(1))
        .ok_or(VehicleCollisionOrderUnavailable::StorageRange)?;
    let next = access.allocate(
        slots
            .checked_mul(8)
            .ok_or(VehicleCollisionOrderUnavailable::StorageRange)?,
    )?;
    if next == 0 {
        return Err(VehicleCollisionOrderUnavailable::AllocationFailed);
    }
    // Preserve the original prefix/value/suffix copy and publication order.
    let insertion = begin.wrapping_add(position.wrapping_mul(8));
    let mut source = access.read_word(h + 4)?;
    let mut target = next;
    while source != insertion {
        let first = access.read_word(source)?;
        let second = access.read_word(source + 4)?;
        access.write_word(target, first)?;
        access.write_word(target + 4, second)?;
        source = source.wrapping_add(8);
        target = target.wrapping_add(8);
    }
    access.write_word(target, index)?;
    access.write_word(target + 4, distance.to_bits())?;
    target = target.wrapping_add(8);
    let live_end = access.read_word(h + 8)?;
    let mut source = insertion;
    while source != live_end {
        let a = access.read_word(source)?;
        let b = access.read_word(source + 4)?;
        access.write_word(target, a)?;
        access.write_word(target + 4, b)?;
        source = source.wrapping_add(8);
        target = target.wrapping_add(8);
    }
    let old = access.read_word(h + 4)?;
    access.free(old)?;
    access.write_word(h + 12, next + slots * 8)?;
    let old = access.read_word(h + 4)?;
    let count = if old == 0 {
        0
    } else {
        access.read_word(h + 8)?.wrapping_sub(old) / 8
    };
    access.write_word(h + 4, next)?;
    access.write_word(h + 8, next + (count + 1) * 8)
}

/// Original5c3d10/5c3d60 owner-filter writes on the shared collision manager.
/// The list pointer and singleton identity are preserved by the zero-owner path.
pub fn set_body_collision_filter<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    filter: VehicleBodyCollisionFilter,
) -> std::result::Result<(), B::Error> {
    const MANAGER: u32 = 0x647f48;
    match filter {
        VehicleBodyCollisionFilter::Group(group) => {
            if group != 0 {
                b.write_word(MANAGER + 0x2bc, 1)?;
                b.write_word(MANAGER + 0x2b8, MANAGER + 0x2b4)?;
                b.write_word(MANAGER + 0x2b4, group)?;
            } else {
                b.write_word(MANAGER + 0x2bc, 0)?;
            }
            b.write_word(MANAGER + 0x2b0, 0)
        }
        VehicleBodyCollisionFilter::Clear => {
            b.write_word(MANAGER + 0x2bc, 0)?;
            b.write_word(MANAGER + 0x2b0, 0)
        }
        VehicleBodyCollisionFilter::Actors { begin, count } => {
            b.write_word(MANAGER + 0x2b8, begin)?;
            b.write_word(MANAGER + 0x2bc, count)?;
            b.write_word(MANAGER + 0x2b0, 0)
        }
    }
}

#[cfg(test)]
#[path = "vehicle_dynamics_body_collision_tests.rs"]
mod tests;
