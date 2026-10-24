//! Original C_PhThing AI over the caller's canonical actor storage.
//! PC 0x45a690 is the time clamp/subdivision wrapper; its workers retain
//! separate entry identities and all effects observe the same address space.
use crate::native_transform::SMatrix;
use crate::ph_thing_response::{
    ph_thing_collision_count, ph_thing_collision_filter, ph_thing_move_count,
    ph_thing_wide_div as wide_div, PhThingResponseBackend, PhThingResponseError,
};
use crate::vehicle_init::VehicleInitBackend;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhThingEffect {
    Dependency {
        actor: u32,
        entry: u32,
        arguments: [u32; 4],
        count: u8,
    },
    UpdateWorld {
        frame: u32,
    },
    FrameVirtual {
        frame: u32,
        slot: u32,
    },
    SetNextCollisionOwner {
        owner: u32,
        flags: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhThingLineHit {
    pub object: u32,
    pub position: [f32; 3],
    pub normal: [f32; 3],
}
pub trait PhThingAiBackend: PhThingResponseBackend {
    fn ph_thing_effect(&mut self, effect: PhThingEffect) -> Result<u32, Self::Error>;
    /// Original TestLineV, flags=2 and final argument=0. The caller brackets
    /// it with DynSetNextCollOwner on the canonical collision manager.
    fn ph_thing_test_line_v(
        &mut self,
        actor: u32,
        origin: [f32; 3],
        delta: [f32; 3],
    ) -> Result<PhThingLineHit, Self::Error>;
    fn ph_thing_actor_world_position(&mut self, actor: u32) -> Result<[f32; 3], Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhThingAiError<E> {
    Backend(E),
    NumericDomain(&'static str),
    Response(PhThingResponseError<E>),
    StorageRange,
    AllocationFailed,
}
type Out<T, E> = Result<T, PhThingAiError<E>>;
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(PhThingAiError::Backend)
}
fn float<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    Ok(f32::from_bits(word(b, a)?))
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    b.write_word(a, v).map_err(PhThingAiError::Backend)
}
fn dependency<B: PhThingAiBackend>(
    b: &mut B,
    actor: u32,
    entry: u32,
    args: &[u32],
) -> Out<u32, B::Error> {
    let mut arguments = [0; 4];
    arguments[..args.len()].copy_from_slice(args);
    b.ph_thing_effect(PhThingEffect::Dependency {
        actor,
        entry,
        arguments,
        count: args.len() as u8,
    })
    .map_err(PhThingAiError::Backend)
}
// The quotient may exceed binary32's exponent range. Keeping its PC24
// mantissa in binary64 preserves the original retained FDIV before __ftol.
fn pc24(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let bits = x.to_bits();
    let discarded = bits & ((1_u64 << 29) - 1);
    let mut retained = bits >> 29;
    if discarded > 1_u64 << 28 || discarded == 1_u64 << 28 && retained & 1 != 0 {
        retained += 1;
    }
    f64::from_bits(retained << 29)
}
fn worker<B: PhThingAiBackend>(b: &mut B, a: u32, dt: f32) -> Out<(), B::Error> {
    if word(b, a + 0x70)? == 1 {
        dependency(b, a, 0x45ade0, &[dt.to_bits()])?;
        Ok(())
    } else {
        ph_thing_move_box(b, a, dt)
    }
}
/// PC C_PhThing::AI, 0x45a690. An error preserves every reached store and
/// completed substep. Worker selection is reloaded for each original call.
pub fn ph_thing_ai<B: PhThingAiBackend>(
    b: &mut B,
    actor: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    let elapsed = float(b, actor + 0x32c)?;
    let lifetime = float(b, actor + 0x330)?;
    // Native FUCOM status also admits unordered comparisons at these gates.
    if elapsed > lifetime || word(b, actor + 0x9c)? > 4000 {
        return Ok(());
    }
    put(b, actor + 0x334, 1_f32.to_bits())?;
    let milliseconds = if milliseconds > 83 {
        let scale = wide_div(83., f64::from(milliseconds)) as f32;
        put(b, actor + 0x334, scale.to_bits())?;
        83
    } else {
        milliseconds
    };
    if milliseconds == 0 {
        return Ok(());
    }
    let dt = crate::native_precision::mul(milliseconds as f32, f32::from_bits(0x3a83126f));
    let maximum = float(b, actor + 0x20c)?;
    if dt <= maximum || maximum.is_nan() {
        return worker(b, actor, dt);
    }
    let quotient = wide_div(f64::from(dt), f64::from(maximum));
    // __ftol uses FISTP qword, then this caller consumes only EAX. Integer
    // indefinite is i64::MIN, whose low word is zero rather than i32::MIN.
    let truncated = if quotient.is_finite()
        && (-9_223_372_036_854_775_808. ..9_223_372_036_854_775_808.).contains(&quotient)
    {
        crate::native_precision::record_precision(quotient.trunc() != quotient);
        quotient.trunc() as i64
    } else {
        i64::MIN
    };
    let count = (truncated as u32).wrapping_add(1) as i32;
    let step = wide_div(f64::from(dt), f64::from(count)) as f32;
    if count <= 0 {
        return Ok(());
    }
    for _ in 0..count {
        worker(b, actor, step)?;
    }
    Ok(())
}

fn short<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u16, B::Error> {
    Ok(u16::from_le_bytes([byte(b, a)?, byte(b, a + 1)?]))
}
fn byte<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a).map_err(PhThingAiError::Backend)
}
fn putb<B: VehicleInitBackend>(b: &mut B, a: u32, v: u8) -> Out<(), B::Error> {
    b.write_byte(a, v).map_err(PhThingAiError::Backend)
}
fn putf<B: VehicleInitBackend>(b: &mut B, a: u32, v: f32) -> Out<(), B::Error> {
    put(b, a, v.to_bits())
}
fn vector<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([float(b, a)?, float(b, a + 4)?, float(b, a + 8)?])
}
fn put_vector<B: VehicleInitBackend>(b: &mut B, a: u32, v: [f32; 3]) -> Out<(), B::Error> {
    for (i, x) in v.into_iter().enumerate() {
        putf(b, a + i as u32 * 4, x)?;
    }
    Ok(())
}
fn matrix<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    for (i, x) in elements.iter_mut().enumerate() {
        *x = float(b, a + i as u32 * 4)?;
    }
    Ok(SMatrix { elements })
}
fn put_matrix<B: VehicleInitBackend>(b: &mut B, a: u32, m: SMatrix) -> Out<(), B::Error> {
    for (i, x) in m.elements.into_iter().enumerate() {
        putf(b, a + i as u32 * 4, x)?;
    }
    Ok(())
}
fn wide_add(a: f64, b: f64) -> f64 {
    let r = pc24(a + b);
    crate::native_precision::record_wide_pc24_add(a, b, r);
    r
}
fn wide_sub(a: f64, b: f64) -> f64 {
    let r = pc24(a - b);
    crate::native_precision::record_wide_pc24_sub(a, b, r);
    r
}
fn wide_mul(a: f64, b: f64) -> f64 {
    let r = pc24(a * b);
    crate::native_precision::record_wide_pc24_mul(a, b, r);
    r
}
fn wide_sqrt(a: f64) -> f64 {
    let r = pc24(a.sqrt());
    crate::native_precision::record_precision(r * r != a);
    r
}
fn spill<E>(a: f64) -> Out<f32, E> {
    let r = a as f32;
    crate::native_precision::record_precision(f64::from(r) != a);
    if r.is_finite() {
        Ok(r)
    } else {
        Err(PhThingAiError::NumericDomain("MoveBox result"))
    }
}
fn w(x: f32) -> f64 {
    f64::from(x)
}
fn plus<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([
        spill(wide_add(w(a[0]), w(b[0])))?,
        spill(wide_add(w(a[1]), w(b[1])))?,
        spill(wide_add(w(a[2]), w(b[2])))?,
    ])
}
fn minus<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([
        spill(wide_sub(w(a[0]), w(b[0])))?,
        spill(wide_sub(w(a[1]), w(b[1])))?,
        spill(wide_sub(w(a[2]), w(b[2])))?,
    ])
}
fn scaled<E>(a: [f32; 3], s: f32) -> Out<[f32; 3], E> {
    Ok([
        spill(wide_mul(w(a[0]), w(s)))?,
        spill(wide_mul(w(a[1]), w(s)))?,
        spill(wide_mul(w(a[2]), w(s)))?,
    ])
}
fn dot_order(a: [f32; 3], b: [f32; 3], o: [usize; 3]) -> f64 {
    wide_add(
        wide_add(
            wide_mul(w(a[o[0]]), w(b[o[0]])),
            wide_mul(w(a[o[1]]), w(b[o[1]])),
        ),
        wide_mul(w(a[o[2]]), w(b[o[2]])),
    )
}
fn point<E>(m: SMatrix, v: [f32; 3]) -> Out<[f32; 3], E> {
    let mut out = [0.; 3];
    for (i, x) in out.iter_mut().enumerate() {
        *x = spill(wide_add(
            wide_add(
                wide_add(wide_mul(w(v[0]), w(m.elements[i])), w(m.elements[12 + i])),
                wide_mul(w(v[1]), w(m.elements[4 + i])),
            ),
            wide_mul(w(v[2]), w(m.elements[8 + i])),
        ))?;
    }
    Ok(out)
}
fn effect<B: PhThingAiBackend>(b: &mut B, e: PhThingEffect) -> Out<u32, B::Error> {
    b.ph_thing_effect(e).map_err(PhThingAiError::Backend)
}
fn world<B: PhThingAiBackend>(b: &mut B, f: u32) -> Out<(), B::Error> {
    if byte(b, f + 0xac)? & 0x20 == 0 {
        effect(b, PhThingEffect::UpdateWorld { frame: f })?;
    }
    Ok(())
}
fn frame_world<B: PhThingAiBackend>(b: &mut B, a: u32) -> Out<SMatrix, B::Error> {
    let f = word(b, a + 0x68)?;
    world(b, f)?;
    matrix(b, f + 0x10)
}
fn normalize_corner<E>(v: [f32; 3]) -> Out<[f64; 3], E> {
    let square = dot_order(v, v, [0, 2, 1]);
    let saved = spill(square)?;
    if wide_sub(square, 1.).abs() < f64::from(f32::from_bits(0x322bcc77)) {
        return Ok(v.map(w));
    }
    if saved >= f32::from_bits(0x322bcc77) {
        let reciprocal = wide_div(1., wide_sqrt(w(saved)));
        let saved = spill(reciprocal)?;
        // Native retains normalized X while explicitly spilling Y and Z.
        return Ok([
            wide_mul(reciprocal, w(v[0])),
            w(spill(wide_mul(w(v[1]), w(saved)))?),
            w(spill(wide_mul(w(v[2]), w(saved)))?),
        ]);
    }
    let mut out = v.map(w);
    let i = if v[0] != 0. {
        0
    } else if v[2] != 0. {
        2
    } else {
        1
    };
    out[i] = if v[i] >= 0. { 1. } else { -1. };
    Ok(out)
}
fn extended_corner_delta<E>(arm: [f32; 3]) -> Out<[f32; 3], E> {
    let normal = normalize_corner(arm)?;
    let scaled = normal.map(|x| wide_mul(x, w(f32::from_bits(0x3e99999a))));
    let z = spill(scaled[2])?;
    Ok([
        spill(wide_add(scaled[0], w(arm[0])))?,
        spill(wide_add(w(arm[1]), scaled[1]))?,
        spill(wide_add(w(arm[2]), w(z)))?,
    ])
}
fn vector_count<E>(begin: u32, end: u32, stride: u32) -> Out<u32, E> {
    end.checked_sub(begin)
        .filter(|n| n.is_multiple_of(stride) && *n / stride <= 1048576)
        .map(|n| n / stride)
        .ok_or(PhThingAiError::StorageRange)
}
// Original 20-byte collision-owner vector insertion, nonaliasing one-element
// caller record. Allocation and reads remain on the canonical Game heap.
fn remember_collision_owner<B: PhThingAiBackend>(
    b: &mut B,
    a: u32,
    owner: u32,
) -> Out<(), B::Error> {
    let begin = word(b, a + 0x380)?;
    let end = word(b, a + 0x384)?;
    let len = if begin == 0 {
        0
    } else {
        vector_count::<B::Error>(begin, end, 20)?
    };
    for i in (0..len).rev() {
        if word(b, begin + i * 20 + 12)? == owner {
            return put(b, begin + i * 20 + 16, 1000);
        }
    }
    let position = b
        .ph_thing_actor_world_position(owner)
        .map_err(PhThingAiError::Backend)?;
    let end = word(b, a + 0x384)?;
    let cap = word(b, a + 0x388)?;
    if vector_count::<B::Error>(end, cap, 20)? != 0 {
        if end != 0 {
            put_vector(b, end, position)?;
            put(b, end + 12, owner)?;
            put(b, end + 16, 1000)?;
        }
        let end = word(b, a + 0x384)?;
        return put(b, a + 0x384, end.wrapping_add(20));
    }
    let begin = word(b, a + 0x380)?;
    let len = if begin == 0 {
        0
    } else {
        vector_count::<B::Error>(begin, end, 20)?
    };
    let capacity = len
        .checked_add(len.max(1))
        .filter(|n| *n <= 1048576)
        .ok_or(PhThingAiError::StorageRange)?;
    let fresh = b.allocate(capacity * 20).map_err(PhThingAiError::Backend)?;
    if fresh == 0 {
        return Err(PhThingAiError::AllocationFailed);
    }
    // Original 5bc0f0 reloads both vector bounds after allocator callbacks.
    let begin = word(b, a + 0x380)?;
    let prefix = vector_count::<B::Error>(begin, end, 20)?;
    for i in 0..prefix * 5 {
        let x = word(b, begin + i * 4)?;
        put(b, fresh + i * 4, x)?;
    }
    let inserted = fresh + prefix * 20;
    put_vector(b, inserted, position)?;
    put(b, inserted + 12, owner)?;
    put(b, inserted + 16, 1000)?;
    let current_end = word(b, a + 0x384)?;
    let suffix = vector_count::<B::Error>(end, current_end, 20)?;
    for i in 0..suffix * 5 {
        let x = word(b, end + i * 4)?;
        put(b, inserted + 20 + i * 4, x)?;
    }
    let old = word(b, a + 0x380)?;
    b.free(old).map_err(PhThingAiError::Backend)?;
    let begin = word(b, a + 0x380)?;
    put(b, a + 0x388, fresh + capacity * 20)?;
    let len = if begin == 0 {
        0
    } else {
        let end = word(b, a + 0x384)?;
        vector_count::<B::Error>(begin, end, 20)?
    };
    put(b, a + 0x380, fresh)?;
    put(b, a + 0x384, fresh + (len + 1) * 20)
}

/// C_PhThing::MoveBox, PC0x45d210, PS2 original symbol MoveBox__9C_PhThingFf.
/// Gravity, rotation, fourteen corner line tests and collision response execute
/// against the actor's original fields and its owning frame, in native order.
pub fn ph_thing_move_box<B: PhThingAiBackend>(b: &mut B, a: u32, dt: f32) -> Out<(), B::Error> {
    let ratio = wide_div(w(dt), w(float(b, a + 0x210)?));
    let toggled = byte(b, a + 0x74)? == 0;
    putb(b, a + 0x74, toggled as u8)?;
    let acceleration = [
        wide_mul(ratio, w(float(b, a + 0x248)?)),
        wide_mul(ratio, w(float(b, a + 0x24c)?)),
        wide_mul(ratio, w(float(b, a + 0x250)?)),
    ];
    // Y/Z products are explicitly spilled before their velocity additions.
    let y = spill(acceleration[1])?;
    let z = spill(acceleration[2])?;
    let x = spill(wide_add(acceleration[0], w(float(b, a + 0x220)?)))?;
    putf(b, a + 0x220, x)?;
    let y = spill(wide_add(w(y), w(float(b, a + 0x224)?)))?;
    putf(b, a + 0x224, y)?;
    let z = spill(wide_add(w(z), w(float(b, a + 0x228)?)))?;
    putf(b, a + 0x228, z)?;
    let saved_position = vector(b, a + 0x24)?;
    let saved_direction = vector(b, a + 0x30)?;
    let saved_up = vector(b, a + 0xac)?;
    let saved_right = vector(b, a + 0xa0)?;
    let old_center = vector(b, a + 0x84)?;
    let displacement = scaled(vector(b, a + 0x220)?, dt)?;
    ph_thing_move_count(b, a, dt, displacement).map_err(PhThingAiError::Response)?;
    let mut transform = frame_world(b, a)?;
    let center = if byte(b, a + 0x76)? == 0 {
        vector(b, a + 0x24)?
    } else {
        point(transform, vector(b, a + 0x78)?)?
    };
    put_vector(b, a + 0x84, center)?;
    let velocity = vector(b, a + 0x220)?;
    if dot_order(velocity, velocity, [0, 1, 2]) < 1. {
        putb(b, a + 0x74, 1)?;
    }
    let mut owners = [0; 14];
    let mut corners = [[0.; 3]; 14];
    let mut normals = [[0.; 3]; 14];
    let mut hits = [[0.; 3]; 14];
    let mut correction = [0.; 3];
    let mut any = false;
    putb(b, a + 0x310, 0)?;
    putb(b, a + 0x311, 0)?;
    let mut skip_actor = false;
    for i in (0..14).rev() {
        corners[i] = point(transform, vector(b, a + 0x268 + i as u32 * 12)?)?;
        let arm = minus(corners[i], old_center)?;
        if dot_order(arm, vector(b, a + 0x220)?, [0, 2, 1]) <= 0. && byte(b, a + 0x74)? == 0 {
            continue;
        }
        let delta = extended_corner_delta(arm)?;
        effect(
            b,
            PhThingEffect::SetNextCollisionOwner {
                owner: a + 0xb8,
                flags: 0,
            },
        )?;
        let hit = b
            .ph_thing_test_line_v(a, old_center, delta)
            .map_err(PhThingAiError::Backend)?;
        effect(
            b,
            PhThingEffect::SetNextCollisionOwner { owner: 0, flags: 0 },
        )?;
        if hit.object == 0 {
            continue;
        }
        put_vector(b, a + 0x100, hit.normal)?;
        hits[i] = hit.position;
        let descriptor = ph_thing_collision_filter(b, hit.object, &mut skip_actor)
            .map_err(PhThingAiError::Response)?;
        if descriptor == 0 {
            continue;
        }
        let material = b
            .ph_thing_collision_descriptor(descriptor)
            .map_err(PhThingAiError::Backend)?;
        if material.properties & 0x40 != 0 {
            owners[i] = material.owner;
        }
        if skip_actor {
            owners[i] = 0;
        }
        putb(b, a + 0x350 + i as u32 * 2, material.material)?;
        putb(b, a + 0x351 + i as u32 * 2, 0)?;
        let normal = vector(b, a + 0x100)?;
        let corner_dot = spill(dot_order(corners[i], normal, [2, 1, 0]))?;
        let separation = wide_sub(w(corner_dot), dot_order(hits[i], normal, [2, 1, 0]));
        putf(b, a + 0x324, spill(separation)?)?;
        if separation <= 0. {
            any = true;
            normals[i] = normal;
            let mask = short(b, a + 0x310)? | 1 << i;
            let bytes = mask.to_le_bytes();
            putb(b, a + 0x310, bytes[0])?;
            putb(b, a + 0x311, bytes[1])?;
        }
    }
    if any {
        let velocity = vector(b, a + 0x220)?;
        if dot_order(velocity, velocity, [0, 1, 2]) > w(f32::from_bits(0x3f19999a)) {
            any = false;
        }
        let mut response_count = 0_f32;
        for i in (0..14).rev() {
            if short(b, a + 0x310)? & (1 << i) == 0 {
                continue;
            }
            let center = vector(b, a + 0x84)?;
            let arm = std::array::from_fn::<_, 3, _>(|j| wide_sub(w(corners[i][j]), w(center[j])));
            let angular = vector(b, a + 0x22c)?;
            // Native forms arm cross angular, then subtracts it from velocity.
            let cross = [
                wide_sub(
                    wide_mul(arm[2], w(angular[1])),
                    wide_mul(arm[1], w(angular[2])),
                ),
                wide_sub(
                    wide_mul(arm[0], w(angular[2])),
                    wide_mul(arm[2], w(angular[0])),
                ),
                wide_sub(
                    wide_mul(arm[1], w(angular[0])),
                    wide_mul(arm[0], w(angular[1])),
                ),
            ];
            let cx = spill(cross[0])?;
            let cy = spill(cross[1])?;
            let velocity = vector(b, a + 0x220)?;
            let contact = wide_add(
                wide_add(
                    wide_mul(wide_sub(w(velocity[2]), cross[2]), w(normals[i][2])),
                    wide_mul(wide_sub(w(velocity[1]), w(cy)), w(normals[i][1])),
                ),
                wide_mul(wide_sub(w(velocity[0]), w(cx)), w(normals[i][0])),
            );
            if contact < 0. {
                if !any {
                    any = true;
                    put_vector(b, a + 0x24, saved_position)?;
                    put_vector(b, a + 0xac, saved_up)?;
                    put_vector(b, a + 0x30, saved_direction)?;
                    put_vector(b, a + 0xa0, saved_right)?;
                }
                ph_thing_collision_count(
                    b,
                    a,
                    a + 0x220,
                    a + 0x22c,
                    normals[i],
                    corners[i],
                    dt,
                    i as u32,
                    owners[i],
                )
                .map_err(PhThingAiError::Response)?;
                response_count = spill(wide_add(w(response_count), 1.))?;
            }
            if owners[i] != 0 {
                remember_collision_owner(b, a, owners[i])?;
            }
        }
        if response_count > 0.5 {
            let displacement = scaled(vector(b, a + 0x220)?, dt)?;
            ph_thing_move_count(b, a, dt, displacement).map_err(PhThingAiError::Response)?;
        }
        transform = frame_world(b, a)?;
        for i in (0..14).rev() {
            if short(b, a + 0x310)? & (1 << i) == 0 {
                continue;
            }
            corners[i] = point(transform, vector(b, a + 0x268 + i as u32 * 12)?)?;
            let separation = wide_sub(
                dot_order(corners[i], normals[i], [2, 1, 0]),
                dot_order(hits[i], normals[i], [1, 2, 0]),
            );
            putf(b, a + 0x324, spill(separation)?)?;
            if separation <= 0. {
                let candidate = [
                    spill(wide_mul(-separation, w(normals[i][0])))?,
                    spill(wide_mul(-separation, w(normals[i][1])))?,
                    spill(wide_mul(-separation, w(normals[i][2])))?,
                ];
                if owners[i] == 0
                    && dot_order(correction, correction, [0, 2, 1])
                        < dot_order(candidate, candidate, [2, 1, 0])
                {
                    correction = candidate;
                }
            }
        }
    }
    let corrected = plus(correction, vector(b, a + 0x24)?)?;
    put_vector(b, a + 0x24, corrected)?;
    let frame = word(b, a + 0x68)?;
    if byte(b, frame + 0xac)? & 0x80 == 0 {
        put_vector(b, frame + 0x80, corrected)?;
        let flags = word(b, frame + 0xac)?;
        put(b, frame + 0xac, flags & 0xfffffedf | 0x40000000)?;
    }
    effect(b, PhThingEffect::FrameVirtual { frame, slot: 0x18 })?;
    let transform = frame_world(b, a)?;
    put_matrix(b, a + 0x10c, transform)?;
    put_matrix(b, a + 0x14c, transform)?;
    let inverse = transform
        .inverse()
        .map_err(|_| PhThingAiError::NumericDomain("MoveBox inverse"))?
        .0;
    put_matrix(b, a + 0x14c, inverse)?;
    put_matrix(b, a + 0x18c, transform)?;
    for o in [0x1bc, 0x1c0, 0x1c4] {
        put(b, a + o, 0)?;
    }
    put_matrix(b, a + 0x1cc, inverse)?;
    for o in [0x1fc, 0x200, 0x204] {
        put(b, a + o, 0)?;
    }
    let center = if byte(b, a + 0x76)? != 0 {
        point(transform, vector(b, a + 0x78)?)?
    } else {
        vector(b, a + 0x24)?
    };
    put_vector(b, a + 0x84, center)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ph_thing_response::PhThingCollisionDescriptor,
        vehicle_init::{
            VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery,
            VehicleInitUnavailable,
        },
    };
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Backend {
        bytes: BTreeMap<u32, u8>,
        calls: Vec<PhThingEffect>,
        queries: Vec<([f32; 3], [f32; 3])>,
        fail_at: Option<usize>,
        next: u32,
        floor: bool,
    }
    impl Backend {
        fn word(&mut self, a: u32, v: u32) {
            for (i, c) in v.to_le_bytes().into_iter().enumerate() {
                self.bytes.insert(a + i as u32, c);
            }
        }
        fn float(&mut self, a: u32, v: f32) {
            self.word(a, v.to_bits());
        }
        fn vector(&mut self, a: u32, v: [f32; 3]) {
            for (i, x) in v.into_iter().enumerate() {
                self.float(a + i as u32 * 4, x);
            }
        }
        fn matrix(&mut self, a: u32, v: SMatrix) {
            for (i, x) in v.elements.into_iter().enumerate() {
                self.float(a + i as u32 * 4, x);
            }
        }
        fn actor(mode: u32) -> Self {
            let mut b = Self {
                next: 0x10000,
                ..Self::default()
            };
            b.word(0x1070, mode);
            b.float(0x120c, 0.028);
            b.float(0x1330, 1.);
            b
        }
    }
    impl VehicleInitBackend for Backend {
        type Error = &'static str;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            Ok(*self.bytes.get(&a).unwrap_or(&0))
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            Ok(u32::from_le_bytes([
                self.read_byte(a)?,
                self.read_byte(a + 1)?,
                self.read_byte(a + 2)?,
                self.read_byte(a + 3)?,
            ]))
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            self.bytes.insert(a, v);
            Ok(())
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.word(a, v);
            Ok(())
        }
        fn free(&mut self, _: u32) -> Result<(), Self::Error> {
            Ok(())
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            let a = self.next;
            self.next += n;
            Ok(a)
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            unreachable!()
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
    impl PhThingResponseBackend for Backend {
        fn ph_thing_collision_descriptor(
            &mut self,
            _: u32,
        ) -> Result<PhThingCollisionDescriptor, Self::Error> {
            Ok(PhThingCollisionDescriptor {
                properties: 0,
                material: 1,
                owner: 0,
            })
        }
    }
    impl PhThingAiBackend for Backend {
        fn ph_thing_effect(&mut self, e: PhThingEffect) -> Result<u32, Self::Error> {
            self.calls.push(e);
            if self.fail_at == Some(self.calls.len()) {
                return Err("injected dependency failure");
            }
            if let PhThingEffect::UpdateWorld { frame } = e {
                let position = vector(self, frame + 0x80).unwrap();
                let mut m = matrix(self, frame + 0x50).unwrap();
                m.elements[12..15].copy_from_slice(&position);
                self.matrix(frame + 0x10, m);
                self.word(frame + 0xac, 0x20);
            }
            Ok(0)
        }
        fn ph_thing_test_line_v(
            &mut self,
            _: u32,
            o: [f32; 3],
            d: [f32; 3],
        ) -> Result<PhThingLineHit, Self::Error> {
            self.queries.push((o, d));
            if self.floor {
                use crate::collision_line::Triangle;
                use crate::collision_query::{
                    test_line_v, CollisionLineOutputs, CollisionLineRequest, CollisionQueryCell,
                    CollisionQueryGrid, CollisionTraversalState,
                };
                let triangles = [
                    Triangle {
                        properties: 3,
                        vertices: [[-10., 0., -10.], [-10., 0., 10.], [10., 0., 10.]],
                        plane: [0., -1., 0., 0.],
                        identity: 1,
                    },
                    Triangle {
                        properties: 4,
                        vertices: [[-10., 0., -10.], [10., 0., -10.], [10., 0., 10.]],
                        plane: [0., -1., 0., 0.],
                        identity: 2,
                    },
                ];
                let cells = [CollisionQueryCell {
                    max_y: 10.,
                    triangles: &triangles,
                    identity: 1,
                }];
                let grid = CollisionQueryGrid {
                    origin_xz: [-20.; 2],
                    cell_size_xz: [40.; 2],
                    width: 1,
                    height: 1,
                    cells: &cells,
                    dynamic_candidates_present: false,
                };
                let mut outputs = CollisionLineOutputs {
                    point: Some([0.; 3]),
                    normal: Some([0.; 3]),
                };
                let hit = test_line_v(
                    Some(&grid),
                    &CollisionLineRequest {
                        origin: o,
                        delta: d,
                        selector: 2,
                        reject_mask: 0,
                    },
                    &mut outputs,
                    &mut CollisionTraversalState::default(),
                )
                .unwrap();
                return Ok(PhThingLineHit {
                    object: hit.map_or(0, |h| h.primitive),
                    position: outputs.point.unwrap(),
                    normal: outputs.normal.unwrap(),
                });
            }
            Ok(PhThingLineHit {
                object: 0,
                position: [0.; 3],
                normal: [0.; 3],
            })
        }
        fn ph_thing_actor_world_position(&mut self, a: u32) -> Result<[f32; 3], Self::Error> {
            Ok(vector(self, a + 0x24).unwrap())
        }
    }
    #[test]
    fn ai_gates_zero_clamp_and_subdivision_preserve_native_dispatch() {
        let mut b = Backend::actor(1);
        b.float(0x132c, 2.);
        b.word(0x1334, 0xdeadbeef);
        ph_thing_ai(&mut b, 0x1000, 16).unwrap();
        assert_eq!(b.read_word(0x1334).unwrap(), 0xdeadbeef);
        b.float(0x132c, 0.);
        b.word(0x109c, 4001);
        ph_thing_ai(&mut b, 0x1000, 16).unwrap();
        assert_eq!(b.read_word(0x1334).unwrap(), 0xdeadbeef);
        b.word(0x109c, 0);
        ph_thing_ai(&mut b, 0x1000, 0).unwrap();
        assert!(b.calls.is_empty());
        assert_eq!(b.read_word(0x1334).unwrap(), 1_f32.to_bits());
        ph_thing_ai(&mut b, 0x1000, 100).unwrap();
        assert_eq!(b.read_word(0x1334).unwrap(), (83_f32 / 100.).to_bits());
        let dt = (83_f32 * f32::from_bits(0x3a83126f)) / 3.;
        assert_eq!(b.calls.len(), 3);
        for call in &b.calls {
            assert_eq!(
                *call,
                PhThingEffect::Dependency {
                    actor: 0x1000,
                    entry: 0x45ade0,
                    arguments: [dt.to_bits(), 0, 0, 0],
                    count: 1
                }
            );
        }
    }
    #[test]
    fn ai_dependency_failure_retains_scale_and_completed_prefix() {
        let mut b = Backend::actor(1);
        b.fail_at = Some(2);
        assert_eq!(
            ph_thing_ai(&mut b, 0x1000, 100),
            Err(PhThingAiError::Backend("injected dependency failure"))
        );
        assert_eq!(b.calls.len(), 2);
        assert_eq!(b.read_word(0x1334).unwrap(), (83_f32 / 100.).to_bits());
    }
    #[test]
    fn ai_subdivision_consumes_low_dword_of_qword_ftol_result() {
        let dt = crate::native_precision::mul(1., f32::from_bits(0x3a83126f));
        // The finite quotient2^32 and both integer-indefinite cases have
        // EAX=0 after FISTP qword; INC dispatches one full-time worker.
        for maximum in [dt * 2_f32.powi(-32), 0., f32::from_bits(1)] {
            let mut b = Backend::actor(1);
            b.float(0x120c, maximum);
            ph_thing_ai(&mut b, 0x1000, 1).unwrap();
            assert_eq!(
                b.calls,
                [PhThingEffect::Dependency {
                    actor: 0x1000,
                    entry: 0x45ade0,
                    arguments: [dt.to_bits(), 0, 0, 0],
                    count: 1,
                }]
            );
        }
    }
    #[test]
    fn move_box_applies_gravity_and_queries_all_corners_on_the_owning_frame() {
        let mut b = Backend::actor(2);
        b.word(0x1068, 0x4000);
        b.bytes.insert(0x1076, 1);
        b.vector(0x1024, [0., 2., 0.]);
        b.vector(0x1084, [0., 2., 0.]);
        b.vector(0x1030, [0., 0., 1.]);
        b.vector(0x10a0, [1., 0., 0.]);
        b.vector(0x10ac, [0., 1., 0.]);
        b.float(0x1210, 10.);
        b.vector(0x1248, [0., -100., 0.]);
        b.vector(0x1100, [0., 1., 0.]);
        b.matrix(0x114c, SMatrix::identity());
        b.matrix(0x4010, SMatrix::identity());
        b.matrix(0x4050, SMatrix::identity());
        b.vector(0x4080, [0., 2., 0.]);
        b.word(0x40ac, 0x20);
        for i in 0..14 {
            b.vector(
                0x1268 + i * 12,
                [
                    if i & 1 == 0 { -0.5 } else { 0.5 },
                    if i & 2 == 0 { -0.5 } else { 0.5 },
                    if i & 4 == 0 { -0.5 } else { 0.5 },
                ],
            );
        }
        ph_thing_ai(&mut b, 0x1000, 16).unwrap();
        let dt = 16_f32 * f32::from_bits(0x3a83126f);
        let vy = (wide_div(w(dt), 10.) * -100.) as f32;
        let expected_y = 2_f32 + dt * vy;
        assert_eq!(
            vector(&mut b, 0x1220).unwrap().map(f32::to_bits),
            [0., vy, 0.].map(f32::to_bits)
        );
        assert_eq!(vector(&mut b, 0x1024).unwrap(), [0., expected_y, 0.]);
        assert_eq!(vector(&mut b, 0x1084).unwrap(), [0., expected_y, 0.]);
        assert_eq!(vector(&mut b, 0x4080).unwrap(), [0., expected_y, 0.]);
        assert_eq!(b.queries.len(), 14);
        for (origin, _) in &b.queries {
            assert_eq!(*origin, [0., 2., 0.]);
        }
        assert_eq!(short(&mut b, 0x1310).unwrap(), 0);
        assert_eq!(matrix(&mut b, 0x110c).unwrap().elements[13], expected_y);
        assert_eq!(matrix(&mut b, 0x114c).unwrap().elements[13], -expected_y);
        assert_eq!(matrix(&mut b, 0x118c).unwrap().elements[13], 0.);
    }
    #[test]
    fn move_box_floor_contacts_execute_the_original_line_and_impulse_workers() {
        let mut b = Backend::actor(2);
        b.floor = true;
        b.word(0x1068, 0x4000);
        b.bytes.insert(0x1076, 1);
        b.vector(0x1024, [0., 0.49, 0.]);
        b.vector(0x1084, [0., 0.49, 0.]);
        b.vector(0x1030, [0., 0., 1.]);
        b.vector(0x10a0, [1., 0., 0.]);
        b.vector(0x10ac, [0., 1., 0.]);
        b.float(0x1210, 10.);
        b.vector(0x1214, [1.; 3]);
        b.float(0x1238, 0.5);
        b.float(0x123c, 1.01);
        b.float(0x1240, 0.04);
        b.float(0x1244, 1250.);
        b.float(0x1258, 40.);
        b.vector(0x1248, [0., -100., 0.]);
        b.vector(0x1100, [0., 1., 0.]);
        b.matrix(0x114c, SMatrix::identity());
        b.matrix(0x4010, SMatrix::identity());
        b.matrix(0x4050, SMatrix::identity());
        b.vector(0x4080, [0., 0.49, 0.]);
        b.word(0x40ac, 0x20);
        for i in 0..14 {
            b.vector(
                0x1268 + i * 12,
                [
                    if i & 1 == 0 { -0.5 } else { 0.5 },
                    if i & 2 == 0 { -0.5 } else { 0.5 },
                    if i & 4 == 0 { -0.5 } else { 0.5 },
                ],
            );
        }
        ph_thing_ai(&mut b, 0x1000, 16).unwrap();
        assert_eq!(b.queries.len(), 14);
        assert_ne!(short(&mut b, 0x1310).unwrap(), 0);
        assert!(float(&mut b, 0x1224).unwrap() > -0.16);
        let m = matrix(&mut b, 0x110c).unwrap();
        for i in 0..14 {
            let v = vector(&mut b, 0x1268 + i * 12).unwrap();
            assert!(point::<&str>(m, v).unwrap()[1] >= -1e-6);
        }
        assert!(float(&mut b, 0x1370).unwrap() > 0.);
    }
    #[test]
    fn corner_extension_retains_subnormal_products_until_the_native_spill() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        let delta =
            precision.run(|| extended_corner_delta::<&str>([f32::from_bits(5), 1., 0.]).unwrap());
        assert_eq!(delta[0].to_bits(), 6);
        assert_eq!((f32::from_bits(5) * 0.3 + f32::from_bits(5)).to_bits(), 7);
        assert_eq!(delta[1], 1.3);
        assert!(precision.precision_raised());
    }
    #[test]
    fn retained_division_preserves_large_fild_and_extended_exponents() {
        assert_eq!(
            (wide_div(83., f64::from(u32::MAX)) as f32).to_bits(),
            0x32a60000
        );
        assert_eq!(
            wide_div(2_f64.powi(-149), 2_f64.powi(120)),
            2_f64.powi(-269)
        );
        assert_eq!(wide_div(-1., 4.), -0.25);
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            assert_eq!(wide_div(1., 3.), f64::from(1_f32 / 3.));
        });
        assert!(precision.precision_raised());
    }
}
