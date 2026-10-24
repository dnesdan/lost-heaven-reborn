//! C_car's original Actor_Filter, Actor_Col_Info and Call_Collision callbacks.
//! Their outputs are caller-local buffers; other actor fields remain in the
//! same canonical address space as the owning car and collision descriptor.
use crate::ph_thing_response::PhThingNumber as R;
use crate::vehicle_car_contacts::VehicleCarContactBackend;
use crate::vehicle_init::VehicleInitBackend;
#[cfg(test)]
#[path = "vehicle_car_body_dynamic_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleCarOtherBody {
    pub arm: [f32; 3],
    pub velocity: [f32; 3],
    pub mass: f32,
    pub inertia_response: [f32; 3],
    pub restitution: f32,
    pub correction_allowed: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleCarBodyDynamicError<E> {
    Effect(E),
    InvalidDescriptor { descriptor: u32, properties: u32 },
    NumericDomain(&'static str),
    Dependency { entry: u32, actor: u32 },
}
type Out<T, E> = Result<T, VehicleCarBodyDynamicError<E>>;
impl<E> From<&'static str> for VehicleCarBodyDynamicError<E> {
    fn from(value: &'static str) -> Self {
        Self::NumericDomain(value)
    }
}
fn word<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(VehicleCarBodyDynamicError::Effect)
}
fn float<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    Ok(f32::from_bits(word(b, a)?))
}
fn vector<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([float(b, a)?, float(b, a + 4)?, float(b, a + 8)?])
}
fn put<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    b.write_word(a, v)
        .map_err(VehicleCarBodyDynamicError::Effect)
}
fn putf<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: f32) -> Out<(), B::Error> {
    put(b, a, v.to_bits())
}
fn put_half<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u16) -> Out<(), B::Error> {
    for (i, c) in v.to_le_bytes().into_iter().enumerate() {
        b.write_byte(a + i as u32, c)
            .map_err(VehicleCarBodyDynamicError::Effect)?;
    }
    Ok(())
}
fn byte<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a).map_err(VehicleCarBodyDynamicError::Effect)
}
fn state<B: VehicleInitBackend + ?Sized>(b: &mut B, actor: u32, value: u32) -> Out<(), B::Error> {
    crate::crash_actor::crash_actor_set_state(b, actor, value).map_err(|e| match e {
        crate::crash_actor::CrashActorError::Backend(e) => VehicleCarBodyDynamicError::Effect(e),
        _ => VehicleCarBodyDynamicError::NumericDomain("actor state"),
    })
}
fn length<E>(v: [f32; 3], order: [usize; 3]) -> Out<R, E> {
    let squared = R::load(v[order[0]])?
        .mul(R::load(v[order[0]])?)?
        .add(R::load(v[order[1]])?.mul(R::load(v[order[1]])?)?)?
        .add(R::load(v[order[2]])?.mul(R::load(v[order[2]])?)?)?
        .store()?;
    if (squared.to_bits() as i32) < 0x322bcc77 {
        return Ok(R::load(0.)?);
    }
    let squared = R::load(squared)?;
    if squared.sub(R::load(1.)?)?.value().abs() < f64::from(1e-8_f32) {
        Ok(R::load(1.)?)
    } else {
        Ok(squared.sqrt()?)
    }
}
fn transform<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    matrix: u32,
    v: [f32; 3],
) -> Out<[f32; 3], B::Error> {
    let mut out = [0.; 3];
    for (column, value) in out.iter_mut().enumerate() {
        let c = column as u32 * 4;
        // Both callers use S_vector::operator*=, whose association differs
        // from the returned-value transform: Z,Y,X for X; Z,X,Y for Y/Z.
        let [a, d, e] = if column == 0 { [2, 1, 0] } else { [2, 0, 1] };
        *value = R::load(float(b, matrix + a as u32 * 0x10 + c)?)?
            .mul(R::load(v[a])?)?
            .add(R::load(float(b, matrix + d as u32 * 0x10 + c)?)?.mul(R::load(v[d])?)?)?
            .add(R::load(float(b, matrix + e as u32 * 0x10 + c)?)?.mul(R::load(v[e])?)?)?
            .add(R::load(float(b, matrix + 0x30 + c)?)?)?
            .store()?;
    }
    Ok(out)
}
fn owner<B: VehicleCarContactBackend + ?Sized>(b: &mut B, d: u32) -> Out<u32, B::Error> {
    let properties = b
        .car_contact_properties(d)
        .map_err(VehicleCarBodyDynamicError::Effect)?;
    // The original dereferences descriptor+44 only after this flag selects
    // its actor-bearing record. A missing flag is outside its pointer domain.
    if properties & 0x40 == 0 {
        return Err(VehicleCarBodyDynamicError::InvalidDescriptor {
            descriptor: d,
            properties,
        });
    }
    b.car_contact_actor_owner(d)
        .map_err(VehicleCarBodyDynamicError::Effect)
}

/// Actor_Filter, PC425970. ECX is current C_car; EDX is the other descriptor.
/// The original pops but never reads or writes its trailing bool-reference.
pub fn car_body_collision_flags<B: VehicleCarContactBackend + ?Sized>(
    b: &mut B,
    car: u32,
    descriptor: u32,
    _damage_allowed: &mut u8,
) -> Out<bool, B::Error> {
    let actor = owner(b, descriptor)?;
    if actor == 0 {
        return Ok(false);
    }
    Ok(match word(b, actor + 0x10)? {
        6 | 8 | 0x1e | 0x16 => true,
        4 => word(b, actor + 0x214)? & 0x1000_0000 != 0,
        2 | 0x1b | 0x13 => f32::from_bits(word(b, car + 0x60c)?) < 4.,
        _ => false,
    })
}

/// Actor_Col_Info, PC425190. ECX is descriptor; EDX is point. Seven stack
/// pointers are normal, arm, velocity, mass, inertia response, restitution,
/// correction flag, respectively. The original returns AL and pops0x1c.
pub fn car_body_other_properties<B: VehicleCarContactBackend + ?Sized>(
    b: &mut B,
    descriptor: u32,
    point: [f32; 3],
    normal: [f32; 3],
    out: &mut VehicleCarOtherBody,
) -> Out<bool, B::Error> {
    let actor = owner(b, descriptor)?;
    out.correction_allowed = 0;
    if actor == 0 {
        out.arm = [0.; 3];
        out.velocity = [0.; 3];
        out.mass = f32::from_bits(0x49435000);
        out.inertia_response = [0.; 3];
        out.restitution = -1.;
        out.correction_allowed = 1;
        return Ok(true);
    }
    let kind = word(b, actor + 0x10)?;
    match kind {
        6 | 8 | 0x16 | 0x1e => {
            out.arm = [0.; 3];
            out.velocity = [0.; 3];
            out.mass = f32::from_bits(0x49435000);
            out.inertia_response = [0.; 3];
            out.restitution = -1.;
            out.correction_allowed = 1;
            Ok(true)
        }
        2 | 0x13 | 0x19 | 0x1b | 0x15 => {
            out.arm = [0.; 3];
            out.velocity = [0.; 3];
            out.mass = if kind == 0x15 { 30. } else { 80. };
            out.inertia_response = [0.; 3];
            out.restitution = -1.;
            Ok(true)
        }
        0x11 | 0x1a | 0x23 => {
            ph_thing_other_properties(b, actor, point, normal, out)?;
            out.restitution = f32::from_bits(if kind == 0x11 { 0xbf8ccccd } else { 0xbfa66666 });
            Ok(true)
        }
        4 => Err(VehicleCarBodyDynamicError::Dependency {
            entry: 0x4cc4b0,
            actor,
        }),
        9 => Err(VehicleCarBodyDynamicError::Dependency {
            entry: 0x413f30,
            actor,
        }),
        _ => Ok(false),
    }
}

fn ph_thing_other_properties<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    actor: u32,
    point: [f32; 3],
    normal: [f32; 3],
    out: &mut VehicleCarOtherBody,
) -> Out<(), B::Error> {
    for (i, value) in out.arm.iter_mut().enumerate() {
        *value = R::load(point[i])?
            .sub(R::load(float(b, actor + 0x84 + i as u32 * 4)?)?)?
            .store()?;
    }
    let arm = out.arm.map(R::load);
    let arm = [arm[0]?, arm[1]?, arm[2]?];
    let angular = vector(b, actor + 0x22c)?.map(R::load);
    let angular = [angular[0]?, angular[1]?, angular[2]?];
    let cross_x = angular[1].mul(arm[2])?.sub(angular[2].mul(arm[1])?)?;
    let cross_y = arm[0].mul(angular[2])?.sub(angular[0].mul(arm[2])?)?;
    // Original45ff19 spills Z before its final subtraction; X/Y remain
    // retained x87 values until their respective velocity stores.
    let cross_z = angular[0]
        .mul(arm[1])?
        .sub(arm[0].mul(angular[1])?)?
        .store()?;
    out.velocity[0] = R::load(float(b, actor + 0x220)?)?.sub(cross_x)?.store()?;
    out.velocity[1] = R::load(float(b, actor + 0x224)?)?.sub(cross_y)?.store()?;
    out.velocity[2] = R::load(float(b, actor + 0x228)?)?
        .sub(R::load(cross_z)?)?
        .store()?;
    out.mass = float(b, actor + 0x210)?;
    let normal = normal.map(R::load);
    let normal = [normal[0]?, normal[1]?, normal[2]?];
    out.inertia_response = [
        normal[2]
            .mul(arm[1])?
            .sub(normal[1].mul(arm[2])?)?
            .store()?,
        normal[0]
            .mul(arm[2])?
            .sub(normal[2].mul(arm[0])?)?
            .store()?,
        normal[1]
            .mul(arm[0])?
            .sub(normal[0].mul(arm[1])?)?
            .store()?,
    ];
    out.inertia_response = transform(b, actor + 0x1cc, out.inertia_response)?;
    for (i, value) in out.inertia_response.iter_mut().enumerate() {
        *value = R::load(*value)?
            .mul(R::load(float(b, actor + 0x214 + i as u32 * 4)?)?)?
            .store()?;
    }
    out.inertia_response = transform(b, actor + 0x18c, out.inertia_response)?;
    Ok(())
}

/// Call_Collision, PC425480. ECX is current C_car; EDX is descriptor. The
/// four original arguments are point, mutable impulse, normal and self mass.
pub fn car_body_apply_other_impulse<B: VehicleCarContactBackend + ?Sized>(
    b: &mut B,
    car: u32,
    descriptor: u32,
    point: &mut [f32; 3],
    impulse: &mut [f32; 3],
    _normal: &mut [f32; 3],
    _self_mass: f32,
) -> Out<(), B::Error> {
    let actor = owner(b, descriptor)?;
    if actor == 0 {
        return Ok(());
    }
    // Virtual+78 receives sourceCar, point, impulse, normal, selfMass and a
    // final frame. This is the genuine C_crash_obj implementation; other
    // actor vtables retain their explicit dispatch boundary.
    if word(b, actor)? != 0x623918 {
        return Err(VehicleCarBodyDynamicError::Dependency {
            entry: 0x425480,
            actor,
        });
    }
    car_body_crash_collision(b, actor, *point, *impulse)?;
    if byte(b, car + 0x20f5)? != 0 {
        let app = word(b, 0x63788c)?;
        let game = word(b, app + 0x24)?;
        if byte(b, game + 0x2da0)? != 0 {
            return Err(VehicleCarBodyDynamicError::Dependency {
                entry: 0x49cc90,
                actor: car,
            });
        }
    }
    Ok(())
}

/// C_crash_obj's virtual+78, PC4324c0. Its other four arguments are not
/// read by this body; the original tail forwards them intact to45f520.
pub fn car_body_crash_collision<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    actor: u32,
    point: [f32; 3],
    impulse: [f32; 3],
) -> Out<bool, B::Error> {
    let magnitude = length::<B::Error>(impulse, [0, 1, 2])?;
    let destructible = word(b, actor + 0x38c)? & 1 != 0;
    if destructible {
        let health = R::load(float(b, actor + 0x390)?)?.sub(magnitude)?.store()?;
        putf(b, actor + 0x390, health)?;
    }
    for (i, value) in impulse.into_iter().enumerate() {
        putf(b, actor + 0x3a0 + i as u32 * 4, value)?;
    }
    let can_move = byte(b, actor + 0x3be)? != 0;
    putf(b, actor + 0x3ac, -impulse[0])?;
    putf(b, actor + 0x3b0, -impulse[1])?;
    putf(b, actor + 0x3b8, magnitude.store()?)?;
    putf(b, actor + 0x3b4, -impulse[2])?;
    if can_move {
        return car_body_ph_thing_collision(b, actor, point, impulse);
    }
    if destructible && float(b, actor + 0x390)? < 0. {
        state(b, actor, 0)?;
    }
    Ok(false)
}

/// Base physical-actor impulse worker at PC45f520. It updates the same
/// actor's linear/angular velocity and collision-sound peak records.
pub fn car_body_ph_thing_collision<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    actor: u32,
    point: [f32; 3],
    impulse: [f32; 3],
) -> Out<bool, B::Error> {
    state(b, actor, 0)?;
    put(b, actor + 0x9c, 0)?;
    put(b, actor + 0x32c, 0)?;
    let mut arm = [R::load(0.)?; 3];
    for (i, value) in arm.iter_mut().enumerate() {
        *value = R::load(point[i])?.sub(R::load(float(b, actor + 0x84 + i as u32 * 4)?)?)?;
    }
    let factor = R::load(f32::from_bits(0x3f8ccccd))?;
    // X/Y spill; Z remains retained across the inverse-mass arithmetic.
    let sx = R::load(R::load(impulse[0])?.mul(factor)?.store()?)?;
    let sy = R::load(R::load(impulse[1])?.mul(factor)?.store()?)?;
    let sz = R::load(impulse[2])?.mul(factor)?;
    let inverse_mass = R::load(1.)?.div(R::load(float(b, actor + 0x210)?)?)?;
    let dx = sx.mul(inverse_mass)?;
    let dy = sy.mul(inverse_mass)?.store()?;
    let dz = sz.mul(inverse_mass)?.store()?;
    let x = dx.add(R::load(float(b, actor + 0x220)?)?)?.store()?;
    putf(b, actor + 0x220, x)?;
    let y = R::load(dy)?
        .add(R::load(float(b, actor + 0x224)?)?)?
        .store()?;
    putf(b, actor + 0x224, y)?;
    let z = R::load(dz)?
        .add(R::load(float(b, actor + 0x228)?)?)?
        .store()?;
    putf(b, actor + 0x228, z)?;
    let angular = [
        sz.mul(arm[1])?.sub(sy.mul(arm[2])?)?.store()?,
        arm[2].mul(sx)?.sub(sz.mul(arm[0])?)?.store()?,
        sy.mul(arm[0])?.sub(arm[1].mul(sx)?)?.store()?,
    ];
    let mut angular = transform(b, actor + 0x1cc, angular)?;
    for (i, value) in angular.iter_mut().enumerate() {
        *value = R::load(*value)?
            .mul(R::load(float(b, actor + 0x214 + i as u32 * 4)?)?)?
            .store()?;
    }
    let angular = transform(b, actor + 0x18c, angular)?;
    for (i, value) in angular.into_iter().enumerate() {
        let value = R::load(float(b, actor + 0x22c + i as u32 * 4)?)?
            .sub(R::load(value)?)?
            .store()?;
        putf(b, actor + 0x22c + i as u32 * 4, value)?;
    }
    let magnitude = length::<B::Error>(impulse, [2, 1, 0])?;
    if word(b, actor + 0x34c)? as i32 >= 0
        && magnitude.value() > f64::from(float(b, actor + 0x370)?)
    {
        let material = word(b, actor + 0x34c)? as u16;
        put_half(b, actor + 0x36c, 0)?;
        putf(b, actor + 0x370, magnitude.store()?)?;
        put_half(b, actor + 0x350, material)?;
        for (i, value) in point.into_iter().enumerate() {
            putf(b, actor + 0x33c + i as u32 * 4, value)?;
        }
    }
    Ok(true)
}

/// Call_Collision1, PC4256b0, reached only by BodyCollision's static branch.
/// The original switch has no operation for ordinary physical/crash actors.
pub fn car_body_static_notification<B: VehicleCarContactBackend + ?Sized>(
    b: &mut B,
    _car: u32,
    descriptor: u32,
    _point: &mut [f32; 3],
    _linear_velocity: &mut [f32; 3],
    _normal: &mut [f32; 3],
) -> Out<(), B::Error> {
    let actor = owner(b, descriptor)?;
    if actor == 0 {
        return Ok(());
    }
    match word(b, actor + 0x10)? {
        2 | 0x13 | 0x1b | 8 => Err(VehicleCarBodyDynamicError::Dependency {
            entry: 0x4256b0,
            actor,
        }),
        _ => Ok(()),
    }
}
