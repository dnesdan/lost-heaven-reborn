//! Original BodyCollision contact response. Static impulse/friction and the
//! two-body dynamic response share the original point/approach prefix.
#[path = "vehicle_dynamics_body_dynamic_response.rs"]
mod dynamic_response;
use crate::native_transform::{try_vector_angle_sine_scaled, MakeUnavailable, SMatrix};
use crate::vehicle_dynamics_body::{transform_assign, VehicleBodyCollisionCall};
use crate::vehicle_dynamics_prepare::{
    add, div, float, mul, number, result, store, store_float, sub, word,
};
use crate::vehicle_init::VehicleInitBackend;
pub use dynamic_response::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleBodyStaticContact {
    pub sphere_record: u32,
    pub descriptor: u32,
    pub previous_center: [f32; 3],
    pub resolved_center: [f32; 3],
    pub normal: [f32; 3],
    pub radius: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehicleBodyStaticEffect {
    Damage {
        point: [f32; 3],
        normal: [f32; 3],
        radius: f32,
        strength: f32,
    },
    AdditionalDamage {
        strength: f32,
    },
    Callback {
        function: u32,
        context: u32,
        impulse: [f32; 3],
        /// Fastcall EDX points at this distinct local contact impulse.
        contact_impulse: [f32; 3],
        point: [f32; 3],
        radius: f32,
        velocity: [f32; 3],
        /// Original fifth stack argument, reloaded from shared63d9e4.
        dynamic_owner: u32,
        /// Static response passes null in the original sixth stack argument.
        descriptor: u32,
        /// Original seventh stack argument, zero for this static caller.
        flags: u32,
    },
    Sound {
        strength: f32,
        grounded: u8,
    },
}
pub trait VehicleBodyStaticResponseBackend: VehicleInitBackend {
    /// Scene adapters resolve these through their owned opaque descriptors.
    fn body_static_descriptor_properties(
        &mut self,
        descriptor: u32,
    ) -> std::result::Result<u32, Self::Error> {
        self.read_word(descriptor)
    }
    fn body_static_descriptor_owner(
        &mut self,
        descriptor: u32,
    ) -> std::result::Result<u32, Self::Error> {
        self.read_word(descriptor.wrapping_add(0x44))
    }

    fn body_dynamic_flags(
        &mut self,
        _vehicle: u32,
        descriptor: u32,
        _damage_allowed: &mut bool,
    ) -> std::result::Result<bool, VehicleBodyStaticUnavailable<Self::Error>> {
        let owner = self
            .body_static_descriptor_owner(descriptor)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        Err(VehicleBodyStaticUnavailable::DynamicOwner(owner))
    }
    fn body_dynamic_properties(
        &mut self,
        _vehicle: u32,
        descriptor: u32,
        _point: [f32; 3],
        _normal: [f32; 3],
        _output: &mut crate::vehicle_car_body_dynamic::VehicleCarOtherBody,
    ) -> std::result::Result<bool, VehicleBodyStaticUnavailable<Self::Error>> {
        let owner = self
            .body_static_descriptor_owner(descriptor)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        Err(VehicleBodyStaticUnavailable::DynamicOwner(owner))
    }
    fn body_dynamic_apply(
        &mut self,
        _vehicle: u32,
        descriptor: u32,
        _point: &mut [f32; 3],
        _impulse: &mut [f32; 3],
        _normal: &mut [f32; 3],
        _self_mass: f32,
    ) -> std::result::Result<(), VehicleBodyStaticUnavailable<Self::Error>> {
        let owner = self
            .body_static_descriptor_owner(descriptor)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        Err(VehicleBodyStaticUnavailable::DynamicOwner(owner))
    }

    fn body_dynamic_static_notify(
        &mut self,
        _vehicle: u32,
        descriptor: u32,
        _point: &mut [f32; 3],
        _linear_velocity: &mut [f32; 3],
        _normal: &mut [f32; 3],
    ) -> std::result::Result<(), VehicleBodyStaticUnavailable<Self::Error>> {
        let owner = self
            .body_static_descriptor_owner(descriptor)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        Err(VehicleBodyStaticUnavailable::DynamicOwner(owner))
    }

    fn body_static_effect(
        &mut self,
        vehicle: u32,
        effect: &mut VehicleBodyStaticEffect,
    ) -> std::result::Result<(), Self::Error>;
    fn body_static_damage_timer(
        &mut self,
        vehicle: u32,
        milliseconds: u32,
        point: [f32; 3],
        material: u32,
    ) -> std::result::Result<bool, Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleBodyStaticUnavailable<E> {
    Effect(E),
    NumericDomain,
    Library(MakeUnavailable),
    DynamicOwner(u32),
}
type Out<T, E> = std::result::Result<T, VehicleBodyStaticUnavailable<E>>;
impl<E> From<crate::vehicle_dynamics_prepare::VehicleMovePrepareUnavailable<E>>
    for VehicleBodyStaticUnavailable<E>
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
fn vec<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([
        float(b, a)?,
        float(b, a.wrapping_add(4))?,
        float(b, a.wrapping_add(8))?,
    ])
}
fn mat<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    for (i, x) in elements.iter_mut().enumerate() {
        *x = float(b, a.wrapping_add(i as u32 * 4))?;
    }
    Ok(SMatrix { elements })
}
fn cross<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([
        sub(mul(a[1], b[2])?, mul(a[2], b[1])?)?,
        sub(mul(a[2], b[0])?, mul(a[0], b[2])?)?,
        sub(mul(a[0], b[1])?, mul(a[1], b[0])?)?,
    ])
}
fn dot_xzy<E>(a: [f32; 3], b: [f32; 3]) -> Out<f32, E> {
    Ok(add(
        add(mul(a[0], b[0])?, mul(a[2], b[2])?)?,
        mul(a[1], b[1])?,
    )?)
}
fn dot_zyx<E>(a: [f32; 3], b: [f32; 3]) -> Out<f32, E> {
    Ok(add(
        add(mul(a[2], b[2])?, mul(a[1], b[1])?)?,
        mul(a[0], b[0])?,
    )?)
}
fn square_xyz<E>(v: [f32; 3]) -> Out<f32, E> {
    Ok(add(
        add(mul(v[0], v[0])?, mul(v[1], v[1])?)?,
        mul(v[2], v[2])?,
    )?)
}
fn square_zyx<E>(v: [f32; 3]) -> Out<f32, E> {
    Ok(add(
        add(mul(v[2], v[2])?, mul(v[1], v[1])?)?,
        mul(v[0], v[0])?,
    )?)
}
fn square_root<E>(s: f32) -> Out<f32, E> {
    let value = result(s.sqrt(), f64::from(s).sqrt())?;
    crate::native_precision::record_sqrt(s, value);
    Ok(value)
}
fn length<E>(v: [f32; 3]) -> Out<f32, E> {
    let s = square_zyx(v)?;
    if (s.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if f64::from(sub(s, 1.)?.abs()) < 1e-8 {
        Ok(1.)
    } else {
        Ok(square_root(s)?)
    }
}
fn normalize<E>(mut v: [f32; 3]) -> Out<[f32; 3], E> {
    let s = square_zyx(v)?;
    if f64::from(sub(s, 1.)?.abs()) >= 1e-8 {
        if s >= f32::from_bits(0x322bcc77) {
            let r = div(1., square_root(s)?)?;
            for x in &mut v {
                *x = mul(r, *x)?;
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
fn inverse_inertia<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    torque: [f32; 3],
) -> Out<[f32; 3], B::Error> {
    let mut local = transform_assign(mat(b, v.wrapping_add(0xda8))?, torque)?;
    for (i, x) in local.iter_mut().enumerate() {
        *x = mul(*x, float(b, v.wrapping_add(0x438 + i as u32 * 4))?)?;
    }
    Ok(transform_assign(mat(b, v.wrapping_add(0xd68))?, local)?)
}
fn coefficient<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    material: u32,
    offset: u32,
    override_offset: u32,
) -> Out<f32, B::Error> {
    if b.read_byte(v.wrapping_add(0x13c))
        .map_err(VehicleBodyStaticUnavailable::Effect)?
        != 0
    {
        return Ok(float(b, v.wrapping_add(override_offset))?);
    }
    let index = b
        .read_byte(0x63d8b0u32.wrapping_add(material))
        .map_err(VehicleBodyStaticUnavailable::Effect)?;
    let table = word(b, 0x63d9b4)?;
    Ok(float(
        b,
        table.wrapping_add(u32::from(index) * 0x30 + offset),
    )?)
}

/// Static branch at PC e93d3/e9d7e..ea91b. The returned bool indicates an
/// approaching contact received the original response. No world query is
/// replaced here: `contact` must be the actual preceding fine sphere result.
pub fn vehicle_body_static_response<B: VehicleBodyStaticResponseBackend>(
    b: &mut B,
    vehicle: u32,
    contact: VehicleBodyStaticContact,
    call: &mut VehicleBodyCollisionCall,
) -> Out<bool, B::Error> {
    let flags = b
        .body_static_descriptor_properties(contact.descriptor)
        .map_err(VehicleBodyStaticUnavailable::Effect)?;
    if flags & 0x10000 != 0 {
        b.write_byte(vehicle.wrapping_add(0x28), 1)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
    }
    if flags & 0x40 != 0 {
        let owner = b
            .body_static_descriptor_owner(contact.descriptor)
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        store(b, 0x63d9e4, owner)?;
    } else {
        store(b, 0x63d9e4, 0)?;
    }
    let material = flags >> 24;
    let mut n = contact.normal;
    let mut point = [0.; 3];
    for i in 0..3 {
        point[i] = add(
            mul(-number(contact.radius)?, n[i])?,
            contact.previous_center[i],
        )?;
    }
    let center = vec(b, vehicle.wrapping_add(0x398))?;
    let arm = [
        sub(point[0], center[0])?,
        sub(point[1], center[1])?,
        sub(point[2], center[2])?,
    ];
    let angular = vec(b, vehicle.wrapping_add(0x420))?;
    let rotational = cross(arm, angular)?;
    let linear = vec(b, vehicle.wrapping_add(0x1f90))?;
    let mut velocity = [
        add(linear[0], rotational[0])?,
        add(linear[1], rotational[1])?,
        add(linear[2], rotational[2])?,
    ];
    let approach = dot_xzy(velocity, n)?;
    if approach >= 0. {
        return Ok(false);
    }
    let previous_surface = [
        sub(contact.resolved_center[0], mul(n[0], contact.radius)?)?,
        sub(contact.resolved_center[1], mul(n[1], contact.radius)?)?,
        sub(contact.resolved_center[2], mul(n[2], contact.radius)?)?,
    ];
    let old_plane = add(
        add(
            mul(n[2], contact.previous_center[2])?,
            mul(n[1], contact.previous_center[1])?,
        )?,
        mul(n[0], contact.previous_center[0])?,
    )?;
    // Internal vector dot at PC RVA7570 adds the Z/Y products before X.
    let surface_plane = dot_zyx(previous_surface, n)?;
    let penetration = sub(contact.radius, sub(old_plane, surface_plane)?)?;
    let correction = if penetration > 0.05 {
        [
            mul(n[0], penetration)?,
            mul(n[1], penetration)?,
            mul(n[2], penetration)?,
        ]
    } else {
        [0.; 3]
    };
    let mut damage_allowed = true;
    let mut notify_static = false;
    if word(b, 0x63d9e4)? != 0 {
        let static_only = b.body_dynamic_flags(vehicle, contact.descriptor, &mut damage_allowed)?;
        notify_static = static_only;
        if word(b, 0x63d9e4)? != 0 && !static_only {
            return vehicle_body_dynamic_response(
                b,
                vehicle,
                contact,
                call,
                VehicleBodyDynamicPrefix {
                    point,
                    arm,
                    velocity,
                    normal: n,
                    correction,
                    damage_allowed,
                },
            );
        }
    }
    let response = coefficient(b, vehicle, material, 0x1c, 0x390)?;
    let numerator = mul(-response, approach)?;
    let angular_response = inverse_inertia(b, vehicle, cross(arm, n)?)?;
    let lever = cross(angular_response, arm)?;
    let inverse_mass = div(1., float(b, vehicle.wrapping_add(0x5f4))?)?;
    let mut denominator = add(dot_xzy(lever, n)?, inverse_mass)?;
    if f64::from(denominator.abs()) < f64::from(f32::from_bits(0x358637bd)) {
        denominator = 1e-6;
    }
    let strength = div(numerator, denominator)?;
    let mut impulse = [
        mul(n[0], strength)?,
        mul(n[1], strength)?,
        mul(n[2], strength)?,
    ];
    for (i, x) in impulse.into_iter().enumerate() {
        let a = vehicle.wrapping_add(0x1f90 + i as u32 * 4);
        let v = add(mul(x, inverse_mass)?, float(b, a)?)?;
        store_float(b, a, v)?;
    }
    let angular_impulse = inverse_inertia(b, vehicle, cross(arm, impulse)?)?;
    for (i, x) in angular_impulse.into_iter().enumerate() {
        let a = vehicle.wrapping_add(0x420 + i as u32 * 4);
        let v = sub(float(b, a)?, x)?;
        store_float(b, a, v)?;
    }
    let magnitude = length(impulse)?;
    if magnitude > number(call.auxiliary_scalar)? {
        call.auxiliary_scalar = magnitude;
        call.auxiliary_vector = impulse;
    }
    let config = word(b, vehicle.wrapping_add(0xc68))?;
    if damage_allowed && config & 0x8000 != 0 {
        let mut effect = VehicleBodyStaticEffect::Damage {
            point,
            normal: n,
            radius: contact.radius,
            strength: call.auxiliary_scalar,
        };
        let outcome = b.body_static_effect(vehicle, &mut effect);
        if let VehicleBodyStaticEffect::Damage {
            point: p, normal, ..
        } = effect
        {
            point = p;
            n = normal;
        }
        outcome.map_err(VehicleBodyStaticUnavailable::Effect)?;
    }
    if damage_allowed && word(b, vehicle.wrapping_add(0xc68))? & 0x4000 != 0 {
        b.body_static_effect(
            vehicle,
            &mut VehicleBodyStaticEffect::AdditionalDamage {
                strength: call.auxiliary_scalar,
            },
        )
        .map_err(VehicleBodyStaticUnavailable::Effect)?;
    }
    let callback = word(b, vehicle.wrapping_add(0x2f4))?;
    if callback != 0 {
        let context = word(b, vehicle.wrapping_add(0xc74))?;
        let mut effect = VehicleBodyStaticEffect::Callback {
            function: callback,
            context,
            impulse: call.auxiliary_vector,
            contact_impulse: impulse,
            point,
            radius: contact.radius,
            velocity,
            dynamic_owner: word(b, 0x63d9e4)?,
            descriptor: 0,
            flags: 0,
        };
        let outcome = b.body_static_effect(vehicle, &mut effect);
        if let VehicleBodyStaticEffect::Callback {
            impulse: maximum,
            contact_impulse,
            point: p,
            velocity: v,
            ..
        } = effect
        {
            call.auxiliary_vector = maximum;
            impulse = contact_impulse;
            point = p;
            velocity = v;
        }
        outcome.map_err(VehicleBodyStaticUnavailable::Effect)?;
    }
    if b.read_byte(vehicle.wrapping_add(0x432))
        .map_err(VehicleBodyStaticUnavailable::Effect)?
        != 0
    {
        let grounded = b
            .read_byte(vehicle.wrapping_add(0x394))
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        b.body_static_effect(
            vehicle,
            &mut VehicleBodyStaticEffect::Sound {
                strength: magnitude,
                grounded,
            },
        )
        .map_err(VehicleBodyStaticUnavailable::Effect)?;
    }
    if div(magnitude, float(b, vehicle.wrapping_add(0x5f4))?)? > 5.
        || number(float(b, vehicle.wrapping_add(0x59c))?)? > 2.
    {
        let timer =
            word(b, contact.sphere_record.wrapping_add(0x38))?.wrapping_add(call.milliseconds);
        store(b, contact.sphere_record.wrapping_add(0x38), timer)?;
        if b.body_static_damage_timer(vehicle, timer, point, material)
            .map_err(VehicleBodyStaticUnavailable::Effect)?
        {
            store(b, contact.sphere_record.wrapping_add(0x38), 0)?;
        }
    }
    if square_xyz(call.correction_first)? < dot_xzy(correction, correction)? {
        let mut accept = true;
        if word(b, 0x63d9e4)? != 0 {
            let mut other = empty_other_body();
            // Original static fallback ignores the callback's AL result and
            // reads only its correction-allowed output byte.
            b.body_dynamic_properties(vehicle, contact.descriptor, point, n, &mut other)?;
            accept = other.correction_allowed != 0;
        }
        if accept {
            call.correction_first = correction;
        }
    }
    if notify_static && word(b, vehicle.wrapping_add(0x2e8))? != 0 {
        let mut linear_velocity = vec(b, vehicle.wrapping_add(0x1f90))?;
        b.body_dynamic_static_notify(
            vehicle,
            contact.descriptor,
            &mut point,
            &mut linear_velocity,
            &mut n,
        )?;
        for (i, value) in linear_velocity.into_iter().enumerate() {
            store_float(b, vehicle.wrapping_add(0x1f90 + i as u32 * 4), value)?;
        }
    }
    let direction = normalize(velocity)?;
    let tangent = normalize(cross(cross(direction, n)?, n)?)?;
    let friction = mul(
        length(impulse)?,
        coefficient(b, vehicle, material, 0x18, 0x38c)?,
    )?;
    let friction = [
        mul(tangent[0], friction)?,
        mul(tangent[1], friction)?,
        mul(tangent[2], friction)?,
    ];
    let friction = try_vector_angle_sine_scaled(velocity, n, friction)
        .map_err(VehicleBodyStaticUnavailable::Library)?;
    for (i, x) in friction.into_iter().enumerate() {
        let a = vehicle.wrapping_add(0x1f90 + i as u32 * 4);
        let v = add(mul(inverse_mass, x)?, float(b, a)?)?;
        store_float(b, a, v)?;
    }
    let angular_impulse = inverse_inertia(b, vehicle, cross(arm, friction)?)?;
    for (i, x) in angular_impulse.into_iter().enumerate() {
        let a = vehicle.wrapping_add(0x420 + i as u32 * 4);
        let v = sub(float(b, a)?, x)?;
        store_float(b, a, v)?;
    }
    Ok(true)
}
