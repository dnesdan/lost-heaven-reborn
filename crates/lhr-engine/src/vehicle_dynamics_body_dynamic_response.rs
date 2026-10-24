//! Original two-body branch of C_Vehicle::BodyCollision, 4e96a3..4e9d79.
//! The caller has already published 63d9e4 and computed the shared contact
//! point, approaching velocity and penetration. Actor callbacks own the other
//! body's canonical writes; no static impulse substitutes for that body.
use super::*;
use crate::{ph_thing_response::PhThingNumber as R, vehicle_car_body_dynamic::VehicleCarOtherBody};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleBodyDynamicPrefix {
    pub point: [f32; 3],
    pub arm: [f32; 3],
    pub velocity: [f32; 3],
    pub normal: [f32; 3],
    pub correction: [f32; 3],
    pub damage_allowed: bool,
}
fn n<E>(x: f32) -> Out<R, E> {
    R::load(x).map_err(|_| VehicleBodyStaticUnavailable::NumericDomain)
}
fn math<T, E>(x: std::result::Result<T, &'static str>) -> Out<T, E> {
    x.map_err(|_| VehicleBodyStaticUnavailable::NumericDomain)
}
fn rf<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    let x = f32::from_bits(word(b, a)?);
    n(x)?;
    Ok(x)
}
fn cross_stored<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    let mut out = [0.; 3];
    for (i, [j, k]) in [[1, 2], [2, 0], [0, 1]].into_iter().enumerate() {
        out[i] = math(math(n(a[j])?.mul(n(b[k])?))?.sub(math(n(a[k])?.mul(n(b[j])?))?))?
            .store()
            .map_err(|_| VehicleBodyStaticUnavailable::NumericDomain)?;
    }
    Ok(out)
}
fn dot<E>(a: [f32; 3], b: [f32; 3], order: [usize; 3]) -> Out<R, E> {
    let mut p = [n(0.)?; 3];
    for i in 0..3 {
        p[i] = math(n(a[i])?.mul(n(b[i])?))?;
    }
    math(math(p[order[0]].add(p[order[1]]))?.add(p[order[2]]))
}
fn matrix_transform<B: VehicleInitBackend>(
    b: &mut B,
    address: u32,
    v: [f32; 3],
) -> Out<[f32; 3], B::Error> {
    let mut m = [0.; 16];
    for i in [0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14] {
        m[i] = rf(b, address + i as u32 * 4)?;
    }
    let orders = [[2, 1, 0], [2, 0, 1], [2, 0, 1]];
    let mut out = [0.; 3];
    for axis in 0..3 {
        let order = orders[axis];
        let p = |i: usize| math(n(m[i * 4 + axis])?.mul(n(v[i])?));
        let sum = math(math(p(order[0])?.add(p(order[1])?))?.add(p(order[2])?))?;
        out[axis] = math(sum.add(n(m[12 + axis])?))?
            .store()
            .map_err(|_| VehicleBodyStaticUnavailable::NumericDomain)?;
    }
    Ok(out)
}
fn inverse_response<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    torque: [f32; 3],
) -> Out<[f32; 3], B::Error> {
    let mut t = matrix_transform(b, v + 0xda8, torque)?;
    for (i, x) in t.iter_mut().enumerate() {
        *x = math(n(*x)?.mul(n(rf(b, v + 0x438 + i as u32 * 4)?)?))?
            .store()
            .map_err(|_| VehicleBodyStaticUnavailable::NumericDomain)?;
    }
    matrix_transform(b, v + 0xd68, t)
}
fn length_xzy<E>(v: [f32; 3]) -> Out<f32, E> {
    let square = math(dot(v, v, [0, 2, 1])?.store())?;
    if (square.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if math(n(square)?.sub(n(1.)?))?.value().abs() < f64::from(1e-8_f32) {
        Ok(1.)
    } else {
        math(math(n(square)?.sqrt())?.store())
    }
}
pub(super) fn empty_other_body() -> VehicleCarOtherBody {
    VehicleCarOtherBody {
        arm: [0.; 3],
        velocity: [0.; 3],
        mass: 0.,
        inertia_response: [0.; 3],
        restitution: 0.,
        correction_allowed: 0,
    }
}

/// Full original dynamic response after the flags callback selects it.
pub fn vehicle_body_dynamic_response<B: VehicleBodyStaticResponseBackend>(
    b: &mut B,
    vehicle: u32,
    contact: VehicleBodyStaticContact,
    call: &mut VehicleBodyCollisionCall,
    prefix: VehicleBodyDynamicPrefix,
) -> Out<bool, B::Error> {
    let VehicleBodyDynamicPrefix {
        mut point,
        arm,
        mut velocity,
        mut normal,
        correction,
        damage_allowed,
    } = prefix;
    let mut other_point = [0.; 3];
    for i in 0..3 {
        other_point[i] = math(
            math(n(-contact.radius)?.mul(n(normal[i])?))?.add(n(contact.resolved_center[i])?),
        )?
        .store()
        .map_err(|_| VehicleBodyStaticUnavailable::NumericDomain)?;
    }
    let mut other = empty_other_body();
    if !b.body_dynamic_properties(vehicle, contact.descriptor, point, normal, &mut other)? {
        return Ok(false);
    }
    let mut relative = [n(0.)?; 3];
    for i in 0..3 {
        relative[i] = math(n(velocity[i])?.sub(n(other.velocity[i])?))?;
    }
    let mut projected = [n(0.)?; 3];
    for i in 0..3 {
        projected[i] = math(relative[i].mul(n(normal[i])?))?;
    }
    let approach = math(math(math(projected[0].add(projected[2]))?.add(projected[1]))?.store())?;
    let response = inverse_response(b, vehicle, cross_stored(arm, normal)?)?;
    // Original stack keeps both cross products live through the denominator.
    // c = own response cross arm, d = other response cross other arm.
    let retained_cross = |a: [f32; 3], c: [f32; 3]| -> Out<[R; 3], B::Error> {
        Ok([
            math(math(n(a[1])?.mul(n(c[2])?))?.sub(math(n(a[2])?.mul(n(c[1])?))?))?,
            math(math(n(c[0])?.mul(n(a[2])?))?.sub(math(n(a[0])?.mul(n(c[2])?))?))?,
            math(math(n(a[0])?.mul(n(c[1])?))?.sub(math(n(c[0])?.mul(n(a[1])?))?))?,
        ])
    };
    let c = retained_cross(response, arm)?;
    let d = retained_cross(other.inertia_response, other.arm)?;
    // 4e9920..9975: mOther^-1,dZ,dY,cZ,cY,dX,cX,mSelf^-1.
    let mut denominator = math(n(1.)?.div(n(other.mass)?))?;
    for (value, axis) in [
        (d[2], 2),
        (d[1], 1),
        (c[2], 2),
        (c[1], 1),
        (d[0], 0),
        (c[0], 0),
    ] {
        denominator = math(denominator.add(math(value.mul(n(normal[axis])?))?))?;
    }
    denominator = math(denominator.add(math(n(1.)?.div(n(rf(b, vehicle + 0x5f4)?)?))?))?;
    if denominator.value().abs() < f64::from(f32::from_bits(0x358637bd)) {
        denominator = n(f32::from_bits(0x358637bd))?;
    }
    let strength = math(n(approach)?.div(denominator))?;
    let mut impulse = [0.; 3];
    for i in 0..3 {
        impulse[i] = math(math(n(normal[i])?.mul(strength))?.store())?;
    }
    let mass = rf(b, vehicle + 0x5f4)?;
    b.body_dynamic_apply(
        vehicle,
        contact.descriptor,
        &mut other_point,
        &mut impulse,
        &mut normal,
        mass,
    )?;
    for x in &mut impulse {
        *x = math(math(n(*x)?.mul(n(other.restitution)?))?.store())?;
    }
    let inverse_mass = math(n(1.)?.div(n(rf(b, vehicle + 0x5f4)?)?))?;
    let dx = math(n(impulse[0])?.mul(inverse_mass))?;
    let dy = math(math(n(impulse[1])?.mul(inverse_mass))?.store())?;
    let dz = math(math(n(impulse[2])?.mul(inverse_mass))?.store())?;
    let vx = math(dx.add(n(rf(b, vehicle + 0x1f90)?)?))?;
    store(b, vehicle + 0x1f90, math(vx.store())?.to_bits())?;
    for (i, delta) in [(1, dy), (2, dz)] {
        let value = math(math(n(delta)?.add(n(rf(b, vehicle + 0x1f90 + i * 4)?)?))?.store())?;
        store(b, vehicle + 0x1f90 + i * 4, value.to_bits())?;
    }
    let angular_impulse = inverse_response(b, vehicle, cross_stored(arm, impulse)?)?;
    for (i, x) in angular_impulse.into_iter().enumerate() {
        let address = vehicle + 0x420 + i as u32 * 4;
        let value = math(math(n(rf(b, address)?)?.sub(n(x)?))?.store())?;
        store(b, address, value.to_bits())?;
    }
    let magnitude = length_xzy(impulse)?;
    if n(magnitude)?.value() > n(call.auxiliary_scalar)?.value() {
        call.auxiliary_scalar = magnitude;
        call.auxiliary_vector = impulse;
    }
    if damage_allowed {
        if word(b, vehicle + 0xc68)? & 0x8000 != 0 {
            let mut effect = VehicleBodyStaticEffect::Damage {
                point,
                normal,
                radius: contact.radius,
                strength: call.auxiliary_scalar,
            };
            let outcome = b.body_static_effect(vehicle, &mut effect);
            if let VehicleBodyStaticEffect::Damage {
                point: p,
                normal: n,
                ..
            } = effect
            {
                point = p;
                normal = n;
            }
            outcome.map_err(VehicleBodyStaticUnavailable::Effect)?;
        }
        if word(b, vehicle + 0xc68)? & 0x4000 != 0 {
            b.body_static_effect(
                vehicle,
                &mut VehicleBodyStaticEffect::AdditionalDamage {
                    strength: call.auxiliary_scalar,
                },
            )
            .map_err(VehicleBodyStaticUnavailable::Effect)?;
        }
    }
    let function = word(b, vehicle + 0x2f4)?;
    if function != 0 {
        let context = word(b, vehicle + 0xc74)?;
        let mut effect = VehicleBodyStaticEffect::Callback {
            function,
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
    if b.read_byte(vehicle + 0x432)
        .map_err(VehicleBodyStaticUnavailable::Effect)?
        != 0
    {
        let grounded = b
            .read_byte(vehicle + 0x394)
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
    if other.correction_allowed != 0
        && dot(call.correction_second, call.correction_second, [0, 1, 2])?.value()
            < dot(correction, correction, [0, 2, 1])?.value()
    {
        call.correction_second = correction;
    }
    // The original dynamic branch jumps directly to ea91b. The static damage
    // timer and tangent/friction suffix are not executed for a dynamic response.
    let _ = (point, velocity, impulse, normal);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
    const V: u32 = 0x1000;
    const DESC: u32 = 0x5000;
    const CONTROL: u32 = 0x6000;
    fn event(b: &mut VehicleFrameMemory, v: u32) {
        let n = b.read_word(CONTROL + 12).unwrap();
        b.write_word(CONTROL + 0x100 + n * 4, v).unwrap();
        b.write_word(CONTROL + 12, n + 1).unwrap();
    }
    impl VehicleBodyStaticResponseBackend for VehicleFrameMemory {
        fn body_dynamic_flags(&mut self, _: u32, _: u32, _: &mut bool) -> Out<bool, Self::Error> {
            event(self, 1);
            Ok(false)
        }
        fn body_dynamic_properties(
            &mut self,
            _: u32,
            _: u32,
            _: [f32; 3],
            _: [f32; 3],
            out: &mut VehicleCarOtherBody,
        ) -> Out<bool, Self::Error> {
            event(self, 2);
            out.correction_allowed = 0;
            if self.read_word(CONTROL).unwrap() == 0 {
                return Ok(false);
            }
            *out = VehicleCarOtherBody {
                arm: [0.; 3],
                velocity: [0.; 3],
                mass: 1.,
                inertia_response: [0.; 3],
                restitution: -1.,
                correction_allowed: 1,
            };
            Ok(true)
        }
        fn body_dynamic_apply(
            &mut self,
            _: u32,
            _: u32,
            point: &mut [f32; 3],
            impulse: &mut [f32; 3],
            _: &mut [f32; 3],
            mass: f32,
        ) -> Out<(), Self::Error> {
            event(self, 3);
            for (i, x) in point
                .iter()
                .chain(impulse.iter())
                .copied()
                .chain([mass])
                .enumerate()
            {
                self.write_word(CONTROL + 0x200 + i as u32 * 4, x.to_bits())
                    .unwrap();
            }
            let factor = f32::from_bits(self.read_word(CONTROL + 4).unwrap());
            for x in impulse {
                *x = crate::native_precision::mul(*x, factor);
            }
            Ok(())
        }
        fn body_static_effect(
            &mut self,
            _: u32,
            e: &mut VehicleBodyStaticEffect,
        ) -> Result<(), Self::Error> {
            event(
                self,
                match e {
                    VehicleBodyStaticEffect::Sound { .. } => 4,
                    VehicleBodyStaticEffect::Callback { .. } => 5,
                    VehicleBodyStaticEffect::Damage { .. } => 6,
                    VehicleBodyStaticEffect::AdditionalDamage { .. } => 7,
                },
            );
            Ok(())
        }
        fn body_static_damage_timer(
            &mut self,
            _: u32,
            _: u32,
            _: [f32; 3],
            _: u32,
        ) -> Result<bool, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
    }
    fn setup() -> (
        VehicleFrameMemory,
        VehicleBodyStaticContact,
        VehicleBodyCollisionCall,
    ) {
        let mut b = VehicleFrameMemory::default();
        b.map_bytes(V, &[0; 0x2300]).unwrap();
        b.map_bytes(DESC, &[0; 0x48]).unwrap();
        b.map_bytes(CONTROL, &[0; 0x400]).unwrap();
        b.map_bytes(0x7000, &[0; 0x20]).unwrap();
        b.map_bytes(0x63d9e4, &[0; 4]).unwrap();
        b.write_word(DESC, 0x10040).unwrap();
        b.write_word(DESC + 0x44, 0x7000).unwrap();
        b.write_word(0x7010, 0x1a).unwrap();
        b.write_word(CONTROL, 1).unwrap();
        b.write_word(CONTROL + 4, 1_f32.to_bits()).unwrap();
        b.write_word(V + 0x5f4, 1_f32.to_bits()).unwrap();
        b.write_word(V + 0x398, (-1_f32).to_bits()).unwrap();
        b.write_word(V + 0x1f90, (-2_f32).to_bits()).unwrap();
        b.write_byte(V + 0x432, 1).unwrap();
        for base in [V + 0xda8, V + 0xd68] {
            for (i, x) in SMatrix::identity().elements.into_iter().enumerate() {
                b.write_word(base + i as u32 * 4, x.to_bits()).unwrap();
            }
        }
        for i in 0..3 {
            b.write_word(V + 0x438 + i * 4, 1_f32.to_bits()).unwrap();
        }
        let contact = VehicleBodyStaticContact {
            sphere_record: V + 0x2000,
            descriptor: DESC,
            previous_center: [0.; 3],
            resolved_center: [1., 0., 0.],
            normal: [1., 0., 0.],
            radius: 1.,
        };
        let call = VehicleBodyCollisionCall {
            collision_records: 0,
            correction_first: [9., 0., 0.],
            correction_second: [0.; 3],
            auxiliary_scalar: 0.,
            auxiliary_vector: [0.; 3],
            previous_up: [0., 1., 0.],
            milliseconds: 16,
        };
        (b, contact, call)
    }
    #[test]
    fn dynamic_pair_uses_other_mass_and_callback_then_updates_second_correction_without_static_friction(
    ) {
        let (mut b, contact, mut call) = setup();
        assert_eq!(
            vehicle_body_static_response(&mut b, V, contact, &mut call),
            Ok(true)
        );
        assert_eq!(b.read_word(0x63d9e4).unwrap(), 0x7000);
        assert_eq!(b.read_byte(V + 0x28).unwrap(), 1);
        assert_eq!(f32::from_bits(b.read_word(V + 0x1f90).unwrap()), -1.);
        assert_eq!(call.auxiliary_scalar, 1.);
        assert_eq!(call.auxiliary_vector, [1., -0., -0.]);
        assert_eq!(call.correction_first, [9., 0., 0.]);
        assert_eq!(call.correction_second, [1., 0., 0.]);
        let events: Vec<_> = (0..b.read_word(CONTROL + 12).unwrap())
            .map(|i| b.read_word(CONTROL + 0x100 + i * 4).unwrap())
            .collect();
        assert_eq!(events, [1, 2, 3, 4]);
        let args: Vec<_> = (0..7)
            .map(|i| f32::from_bits(b.read_word(CONTROL + 0x200 + i * 4).unwrap()))
            .collect();
        assert_eq!(args, [0., 0., 0., -1., -0., -0., 1.]);
    }
    #[test]
    fn apply_callback_impulse_mutation_is_used_before_restitution_and_car_writes() {
        let (mut b, contact, mut call) = setup();
        b.write_word(CONTROL + 4, 0.5_f32.to_bits()).unwrap();
        vehicle_body_static_response(&mut b, V, contact, &mut call).unwrap();
        assert_eq!(f32::from_bits(b.read_word(V + 0x1f90).unwrap()), -1.5);
        assert_eq!(call.auxiliary_scalar, 0.5);
        assert_eq!(call.auxiliary_vector, [0.5, -0., -0.]);
    }
    #[test]
    fn receding_contact_skips_actor_callbacks_and_failed_properties_keeps_completed_prefix() {
        let (mut b, contact, mut call) = setup();
        b.write_word(V + 0x1f90, 2_f32.to_bits()).unwrap();
        assert_eq!(
            vehicle_body_static_response(&mut b, V, contact, &mut call),
            Ok(false)
        );
        assert_eq!(b.read_word(CONTROL + 12).unwrap(), 0);
        assert_eq!(b.read_word(0x63d9e4).unwrap(), 0x7000);
        b.write_word(V + 0x1f90, (-2_f32).to_bits()).unwrap();
        b.write_word(CONTROL, 0).unwrap();
        assert_eq!(
            vehicle_body_static_response(&mut b, V, contact, &mut call),
            Ok(false)
        );
        assert_eq!(b.read_word(CONTROL + 12).unwrap(), 2);
        assert_eq!(f32::from_bits(b.read_word(V + 0x1f90).unwrap()), -2.);
        assert_eq!(call.auxiliary_scalar, 0.);
    }
}
