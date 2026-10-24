//! Original C_car contact callback branches over the caller's live address map.
//! Opaque scene descriptors are resolved by typed accessors. Special actor,
//! breakable, water and impulse paths remain explicit recovery boundaries.
#[path = "vehicle_crash_model.rs"]
mod crash_model;
use crate::{
    vehicle_dynamics_body_collision::VehicleBodySweepHit, vehicle_init::VehicleInitBackend,
};
pub use crash_model::*;

pub trait VehicleCarContactBackend: VehicleInitBackend {
    /// PC0x426514 calls the frame/model crash worker at0x431710. The owner
    /// resolves descriptor+4; descriptors may be opaque world identities.
    fn car_contact_crash_model(
        &mut self,
        _call: VehicleCarCrashCall,
    ) -> Result<(), VehicleCarContactUnavailable<Self::Error>> {
        boundary(VehicleCarContactBoundary::ActorDisplacement)
    }
    fn car_contact_impact_burst(&mut self, call: VehicleCarImpactBurst) -> Out<(), Self::Error> {
        boundary(VehicleCarContactBoundary::ImpactParticleBurst(call.count))
    }
    fn car_contact_material(&mut self, descriptor: u32) -> Result<u8, Self::Error> {
        self.read_byte(descriptor.wrapping_add(3))
    }
    fn car_contact_properties(&mut self, descriptor: u32) -> Result<u32, Self::Error> {
        self.read_word(descriptor)
    }
    fn car_contact_actor_owner(&mut self, descriptor: u32) -> Result<u32, Self::Error> {
        self.read_word(descriptor.wrapping_add(0x44))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleCarContactBoundary {
    ActorDisplacement,
    Material28GameCallback,
    Material1fGameCallback,
    Material2bImpulse,
    DynamicActor(u32),
    BreakableSurface,
    ImpactParticleBurst(i32),
    ImpactFlash,
    ImpactOccupants,
    ImpactForceFeedback,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleCarContactUnavailable<E> {
    Effect(E),
    NumericDomain,
    UnrecoveredBranch(VehicleCarContactBoundary),
}
type Out<T, E> = Result<T, VehicleCarContactUnavailable<E>>;
fn read<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(VehicleCarContactUnavailable::Effect)
}
fn byte<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a).map_err(VehicleCarContactUnavailable::Effect)
}
fn put_byte<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, value: u8) -> Out<(), B::Error> {
    b.write_byte(a, value)
        .map_err(VehicleCarContactUnavailable::Effect)
}
fn float<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    let value = f32::from_bits(read(b, a)?);
    if value == 0. || value.is_normal() {
        Ok(value)
    } else {
        Err(VehicleCarContactUnavailable::NumericDomain)
    }
}
fn boundary<T, E>(branch: VehicleCarContactBoundary) -> Out<T, E> {
    Err(VehicleCarContactUnavailable::UnrecoveredBranch(branch))
}

/// PC 0x426d90, the callback installed at vehicle+0x310 by C_car::Init.
/// Context is the enclosing C_car, while the wheel order belongs to its one
/// embedded vehicle. Rejection preserves the original material-31 wheel store.
pub fn car_wheel_contact_filter<B: VehicleCarContactBackend + ?Sized>(
    b: &mut B,
    car: u32,
    descriptor: u32,
    wheel_index: u32,
) -> Out<u32, B::Error> {
    match b
        .car_contact_material(descriptor)
        .map_err(VehicleCarContactUnavailable::Effect)?
    {
        0x1f => {
            let order = read(b, car.wrapping_add(0xca8))?;
            let wheel = read(b, order.wrapping_add(wheel_index.wrapping_mul(4)))?;
            b.write_word(wheel.wrapping_add(0xb4), 0x1f)
                .map_err(VehicleCarContactUnavailable::Effect)?;
            Ok(0)
        }
        0x2b => Ok(0),
        _ => Ok(descriptor),
    }
}

/// PC 0x426340, installed at vehicle+0x30c. This admits the original ordinary
/// static descriptor path and completed early returns. Unsupported branches
/// retain all preceding canonical writes, including the first water/game flag.
pub fn car_body_contact_filter<B: VehicleCarContactBackend + ?Sized>(
    b: &mut B,
    car: u32,
    hit: &mut VehicleBodySweepHit,
) -> Out<u32, B::Error> {
    let descriptor = hit.descriptor;
    match b
        .car_contact_material(descriptor)
        .map_err(VehicleCarContactUnavailable::Effect)?
    {
        0x29 => {
            if float(b, car.wrapping_add(0x60c))? > 0.1
                && b.car_contact_properties(descriptor)
                    .map_err(VehicleCarContactUnavailable::Effect)?
                    & 0xc0
                    != 0
            {
                let velocity = impact_vector(b, car.wrapping_add(0x2000))?;
                let direction = body_crash_direction(velocity)?;
                let point = [
                    impact_add(impact_mul(direction[0], 3.)?, hit.position[0])?,
                    impact_add(impact_mul(direction[1], 3.)?, hit.position[1])?,
                    impact_add(impact_mul(direction[2], 3.)?, hit.position[2])?,
                ];
                b.car_contact_crash_model(VehicleCarCrashCall {
                    descriptor,
                    point,
                    velocity,
                    scale: 2.,
                })?;
                return Ok(0);
            }
        }
        0x28 => {
            put_byte(b, car.wrapping_add(0x2219), 1)?;
            return boundary(VehicleCarContactBoundary::Material28GameCallback);
        }
        0x1f => {
            if byte(b, car.wrapping_add(0x2219))? == 0 {
                put_byte(b, car.wrapping_add(0x2219), 1)?;
                return boundary(VehicleCarContactBoundary::Material1fGameCallback);
            }
            return Ok(0);
        }
        0x2b => {
            if byte(b, car.wrapping_add(0x20f2))? == 0 {
                return boundary(VehicleCarContactBoundary::Material2bImpulse);
            }
            put_byte(b, car.wrapping_add(0x20f2), 1)?;
            return Ok(0);
        }
        _ => {
            let properties = b
                .car_contact_properties(descriptor)
                .map_err(VehicleCarContactUnavailable::Effect)?;
            let owner = if properties & 0x40 != 0 {
                b.car_contact_actor_owner(descriptor)
                    .map_err(VehicleCarContactUnavailable::Effect)?
            } else {
                0
            };
            if owner != 0 {
                if byte(b, car.wrapping_add(0x221a))? != 0 {
                    return Ok(0);
                }
                // Original 426ad1: only actor type 0x1f enters the wheel
                // intersection/burst callback. Ordinary physical objects keep
                // their real descriptor as a collision, including crash debris.
                if read(b, owner.wrapping_add(0x10))? != 0x1f
                    || read(b, car.wrapping_add(0x534))? == 0
                {
                    return Ok(descriptor);
                }
                return boundary(VehicleCarContactBoundary::DynamicActor(owner));
            }
            if properties & 0x20000 != 0 {
                let begin = read(b, car.wrapping_add(0xd14))?;
                if begin != 0
                    && (read(b, car.wrapping_add(0xd18))?.wrapping_sub(begin) as i32 / 0x24) != 0
                    && float(b, car.wrapping_add(0x60c))? > 6.
                {
                    return boundary(VehicleCarContactBoundary::BreakableSurface);
                }
            }
        }
    }
    Ok(descriptor)
}

/// Local operands of the material41 frame/model crash call. The point is an
/// original temporary; neither the hit nor the car's position is translated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleCarCrashCall {
    pub descriptor: u32,
    pub point: [f32; 3],
    pub velocity: [f32; 3],
    pub scale: f32,
}
fn body_crash_direction<E>(velocity: [f32; 3]) -> Out<[f32; 3], E> {
    let mut direction = velocity.map(|x| -x);
    let squared = impact_add(
        impact_add(
            impact_mul(direction[0], direction[0])?,
            impact_mul(direction[1], direction[1])?,
        )?,
        impact_mul(direction[2], direction[2])?,
    )?;
    if f64::from(impact_sub(squared, 1.)?.abs()) >= 1e-8 {
        if squared >= 1e-8_f32 {
            let root = impact_number(squared.sqrt())?;
            crate::native_precision::record_sqrt(squared, root);
            let reciprocal = impact_div(1., root)?;
            for x in &mut direction {
                *x = impact_mul(*x, reciprocal)?;
            }
        } else {
            let axis = if direction[0] != 0. {
                0
            } else if direction[2] != 0. {
                2
            } else {
                1
            };
            direction[axis] = if direction[axis] < 0. { -1. } else { 1. };
        }
    }
    Ok(direction)
}

/// Original fastcall C_car impact callback, PC 0x4259e0. The first stack
/// impulse pointer is unused by this body; EDX supplies `contact_impulse`.
/// Both wheel callers' alias and body callers' distinct locals retain their
/// ownership, because the callback only reads those vectors.
pub use crate::debris::DebrisPoint as VehicleCarImpactPoint;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleCarImpactBurst {
    pub point: VehicleCarImpactPoint,
    pub count: i32,
    /// Original ordered trailing words: three float operands and the final word.
    pub parameters: [u32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleCarImpactCall {
    pub point: VehicleCarImpactPoint,
    pub contact_impulse: [f32; 3],
    pub velocity: [f32; 3],
    pub dynamic_owner: u32,
    pub descriptor: u32,
}
fn impact_number<E>(x: f32) -> Out<f32, E> {
    crate::vehicle_dynamics_prepare::number::<()>(x)
        .map_err(|_| VehicleCarContactUnavailable::NumericDomain)
}
macro_rules! impact_operation {
    ($name:ident, $source:ident) => {
        fn $name<E>(a: f32, b: f32) -> Out<f32, E> {
            crate::vehicle_dynamics_prepare::$source::<()>(a, b)
                .map_err(|_| VehicleCarContactUnavailable::NumericDomain)
        }
    };
}
impact_operation!(impact_add, add);
impact_operation!(impact_mul, mul);
impact_operation!(impact_sub, sub);
impact_operation!(impact_div, div);
fn impact_angle<E>(
    a: [f32; 3],
    b: [f32; 3],
) -> Out<crate::native_transform::RetainedVectorAngle, E> {
    crate::native_transform::try_retained_vector_angle(a, b)
        .map_err(|_| VehicleCarContactUnavailable::NumericDomain)
}
fn impact_compare<E>(
    angle: &crate::native_transform::RetainedVectorAngle,
    threshold: f32,
) -> Out<std::cmp::Ordering, E> {
    angle
        .compare(threshold)
        .map_err(|_| VehicleCarContactUnavailable::NumericDomain)
}
fn impact_vector<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    address: u32,
) -> Out<[f32; 3], B::Error> {
    Ok([
        float(b, address)?,
        float(b, address + 4)?,
        float(b, address + 8)?,
    ])
}
/// Complete ordinary static/no-occupant path. Particle, occupant and force
/// feedback dependencies retain precise original call boundaries; they never
/// produce a fabricated successful effect.
pub fn car_impact_callback<B: VehicleCarContactBackend + ?Sized>(
    b: &mut B,
    car: u32,
    call: VehicleCarImpactCall,
) -> Out<(), B::Error> {
    use std::cmp::Ordering::{Greater, Less};
    for x in call.contact_impulse {
        impact_number::<B::Error>(x)?;
    }
    // The inline helper accumulates x²+y²+z², then FSQRT under PC24.
    let length = impact_number(crate::native_transform::vector_length(call.contact_impulse))?;
    let strength = impact_div(length, float(b, car + 0x664)?)?;
    let material = if call.descriptor == 0 {
        0
    } else {
        b.car_contact_material(call.descriptor)
            .map_err(VehicleCarContactUnavailable::Effect)?
    };
    let particles = if call.dynamic_owner != 0 {
        true
    } else {
        let row = byte(b, 0x63d8b0 + u32::from(material))?;
        let table = read(b, 0x63d9b4)?;
        (read(b, table + u32::from(row) * 0x30 + 0x28)? as i32) < 0
    };
    if particles {
        // Original 0x425a9b..425ac5 prepares the burst's count before the
        // first external call 0x565060. FTOL truncation also raises precision.
        let mut count = 0;
        if strength > 6. {
            if strength >= 2147483648. {
                return Err(VehicleCarContactUnavailable::NumericDomain);
            }
            count = strength as i32;
            crate::native_precision::record_precision(strength != count as f32);
            count = count.min(10);
        }
        b.car_contact_impact_burst(VehicleCarImpactBurst {
            point: call.point,
            count,
            parameters: [0x3d4ccccd, 0x3f666666, 0x40c00000, 0],
        })?;
        // 425af0..5b56 computes this length even when the following particle
        // emission guards suppress the second helper entirely.
        for value in call.velocity {
            impact_number::<B::Error>(value)?;
        }
        let velocity_length =
            impact_number::<B::Error>(crate::native_transform::vector_length(call.velocity))?;
        if strength > 5. {
            let count = strength as i32;
            crate::native_precision::record_precision(strength != count as f32);
            return boundary(VehicleCarContactBoundary::ImpactParticleBurst(
                count.min(15),
            ));
        }
        if call.dynamic_owner == 0 && velocity_length > 2. {
            // Secondary565720 directional-emission profile is a separate
            // unrecovered call, rather than a successful fake emission.
            return boundary(VehicleCarContactBoundary::ImpactFlash);
        }
    }
    let low = f32::from_bits(0x3f47ae14);
    let high = f32::from_bits(0x4016b852);
    let angle = impact_angle(call.contact_impulse, [0., 1., 0.])?;
    if impact_compare(&angle, low)? == Greater && impact_compare(&angle, high)? == Less {
        let direction = impact_vector(b, car + 0xcac)?;
        let angle = impact_angle(call.contact_impulse, direction)?;
        if impact_compare(&angle, low)? == Less {
            if call.dynamic_owner == 0 {
                put_byte(b, car + 0x2215, 1)?;
            }
            put_byte(b, car + 0x2217, 1)?;
        } else if impact_compare(&angle, high)? == Greater {
            if call.dynamic_owner == 0 {
                put_byte(b, car + 0x2214, 1)?;
            }
            put_byte(b, car + 0x2216, 1)?;
        }
    }
    let threshold = if call.dynamic_owner == 0 {
        f32::from_bits(0x4104cccd)
    } else {
        4.
    };
    let mut amplitude = impact_mul(impact_sub(strength, threshold)?, f32::from_bits(0x406d70a4))?;
    let up = impact_vector(b, car + 0xcc4)?;
    let angle = impact_angle(call.contact_impulse, up)?;
    if impact_compare(&angle, f32::from_bits(0x402ccccd))? == Greater
        && call.dynamic_owner == 0
        && byte(b, car + 0x108)? == 0
    {
        amplitude = amplitude.abs();
    }
    if amplitude > 0. {
        let begin = read(b, car + 0xd14)?;
        if begin != 0 && (read(b, car + 0xd18)?.wrapping_sub(begin) as i32 / 0x24) != 0 {
            return boundary(VehicleCarContactBoundary::ImpactOccupants);
        }
    }
    if byte(b, car + 0x20f5)? != 0 {
        let global = read(b, 0x63788c)?;
        let game = read(b, global + 0x24)?;
        if byte(b, game + 0x2da0)? != 0 {
            return boundary(VehicleCarContactBoundary::ImpactForceFeedback);
        }
    }
    let magnitude = impact_mul(float(b, car + 0x664)?, strength)?;
    b.write_word(car + 0x216c, 0)
        .map_err(VehicleCarContactUnavailable::Effect)?;
    if magnitude > float(b, car + 0x20b8)? {
        let percentage = impact_mul(float(b, car + 0x20c0)?, 0.01)?;
        let reduction = impact_mul(percentage, magnitude)?;
        let remaining = impact_sub(magnitude, reduction)?;
        let quotient = impact_div(remaining, float(b, car + 0x20b4)?)?;
        let loss = impact_mul(quotient, float(b, car + 0x20bc)?)?;
        let health = impact_sub(float(b, car + 0x20c4)?, loss)?;
        b.write_word(car + 0x20c4, health.to_bits())
            .map_err(VehicleCarContactUnavailable::Effect)?;
        if health < 0. {
            b.write_word(car + 0x20c4, 0)
                .map_err(VehicleCarContactUnavailable::Effect)?;
        }
        put_byte(b, car + 0x20c8, 1)?;
    }
    if call.dynamic_owner != 0 {
        b.write_word(car + 0x20cc, call.dynamic_owner)
            .map_err(VehicleCarContactUnavailable::Effect)?;
    }
    let flags = read(b, car + 0x168)?;
    b.write_word(car + 0x168, flags & !1)
        .map_err(VehicleCarContactUnavailable::Effect)
}

#[cfg(test)]
mod impact_tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;
    impl VehicleCarContactBackend for VehicleFrameMemory {
        fn car_contact_impact_burst(
            &mut self,
            call: VehicleCarImpactBurst,
        ) -> Out<(), Self::Error> {
            crate::debris::generate_debris(
                self,
                0,
                crate::debris::DebrisGeneration {
                    point: call.point,
                    count: call.count,
                    scalars: [
                        f32::from_bits(call.parameters[0]),
                        f32::from_bits(call.parameters[1]),
                        f32::from_bits(call.parameters[2]),
                    ],
                    velocity: if call.parameters[3] == 0 {
                        None
                    } else {
                        Some(call.parameters[3])
                    },
                },
            )
            .map_err(|error| match error {
                crate::debris::DebrisUnavailable::Effect(e) => {
                    VehicleCarContactUnavailable::Effect(e)
                }
                crate::debris::DebrisUnavailable::Generation { count, .. } => {
                    VehicleCarContactUnavailable::UnrecoveredBranch(
                        VehicleCarContactBoundary::ImpactParticleBurst(count),
                    )
                }
            })
        }
        fn car_contact_crash_model(
            &mut self,
            call: VehicleCarCrashCall,
        ) -> Result<(), VehicleCarContactUnavailable<Self::Error>> {
            self.write_word(0x6000, call.descriptor)
                .map_err(VehicleCarContactUnavailable::Effect)?;
            for (i, value) in call
                .point
                .into_iter()
                .chain(call.velocity)
                .chain([call.scale])
                .enumerate()
            {
                self.write_word(0x6004 + i as u32 * 4, value.to_bits())
                    .map_err(VehicleCarContactUnavailable::Effect)?;
            }
            Ok(())
        }
    }
    #[test]
    fn ordinary_dynamic_actor_keeps_real_descriptor_and_type31_zero_wheels_returns_early() {
        let mut b = memory();
        b.map_bytes(0x5000, &[0; 0x48]).unwrap();
        b.map_bytes(0x7000, &[0; 0x20]).unwrap();
        b.write_word(0x5000, 0x40).unwrap();
        b.write_word(0x5044, 0x7000).unwrap();
        b.write_word(0x7010, 0x1a).unwrap();
        let mut hit = VehicleBodySweepHit {
            descriptor: 0x5000,
            position: [1., 2., 3.],
            normal: [0., 1., 0.],
        };
        assert_eq!(car_body_contact_filter(&mut b, CAR, &mut hit), Ok(0x5000));
        b.write_word(0x7010, 0x1f).unwrap();
        assert_eq!(car_body_contact_filter(&mut b, CAR, &mut hit), Ok(0x5000));
        b.write_word(CAR + 0x534, 1).unwrap();
        assert_eq!(
            car_body_contact_filter(&mut b, CAR, &mut hit),
            Err(VehicleCarContactUnavailable::UnrecoveredBranch(
                VehicleCarContactBoundary::DynamicActor(0x7000)
            ))
        );
        b.write_byte(CAR + 0x221a, 1).unwrap();
        assert_eq!(car_body_contact_filter(&mut b, CAR, &mut hit), Ok(0));
    }
    const CAR: u32 = 0x1000;
    fn memory() -> VehicleFrameMemory {
        let mut m = VehicleFrameMemory::default();
        m.map_bytes(CAR, &[0; 0x2300]).unwrap();
        m.map_bytes(0x63d8b0, &[0; 0x108]).unwrap();
        m.map_bytes(0x4000, &[0; 48]).unwrap();
        m.write_word(0x63d9b4, 0x4000).unwrap();
        for (offset, value) in [
            (0x664, 100_f32),
            (0x20b4, 100.),
            (0x20b8, 2.),
            (0x20bc, 100.),
            (0x20c4, 100.),
            (0xcc8, 1.),
            (0xcb4, 1.),
        ] {
            m.write_word(CAR + offset, value.to_bits()).unwrap();
        }
        m.write_word(CAR + 0x168, 3).unwrap();
        m
    }
    fn call(impulse: [f32; 3]) -> VehicleCarImpactCall {
        VehicleCarImpactCall {
            point: VehicleCarImpactPoint::Value([0.; 3]),
            contact_impulse: impulse,
            velocity: [0.; 3],
            dynamic_owner: 0,
            descriptor: 0,
        }
    }
    #[test]
    fn crash_material_calls_frame_worker_with_original_temporaries_then_rejects_hit() {
        let mut m = memory();
        m.map_bytes(0x5000, &[0; 4]).unwrap();
        m.map_bytes(0x6000, &[0; 32]).unwrap();
        m.write_word(0x5000, 0x29000080).unwrap();
        m.write_word(CAR + 0x60c, 5_f32.to_bits()).unwrap();
        for (offset, value) in [(0x2000, 3_f32), (0x2004, 4.), (0x2008, 0.)] {
            m.write_word(CAR + offset, value.to_bits()).unwrap();
        }
        let mut hit = VehicleBodySweepHit {
            descriptor: 0x5000,
            position: [10., 20., 30.],
            normal: [0., 1., 0.],
        };
        assert_eq!(car_body_contact_filter(&mut m, CAR, &mut hit).unwrap(), 0);
        assert_eq!(m.read_word(0x6000).unwrap(), 0x5000);
        let values = (0..7)
            .map(|i| f32::from_bits(m.read_word(0x6004 + i * 4).unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(values, [8.2, 17.6, 30., 3., 4., 0., 2.]);
        assert_eq!(hit.position, [10., 20., 30.]);
        assert_eq!(f32::from_bits(m.read_word(CAR + 0x2000).unwrap()), 3.);
    }
    #[test]
    fn crash_material_speed_and_classification_guards_skip_worker() {
        let mut m = memory();
        m.map_bytes(0x5000, &[0; 4]).unwrap();
        m.write_word(0x5000, 0x29000080).unwrap();
        m.write_word(CAR + 0x60c, 0.1_f32.to_bits()).unwrap();
        let mut hit = VehicleBodySweepHit {
            descriptor: 0x5000,
            position: [0.; 3],
            normal: [0., 1., 0.],
        };
        assert_eq!(
            car_body_contact_filter(&mut m, CAR, &mut hit).unwrap(),
            0x5000
        );
        m.write_word(CAR + 0x60c, 1_f32.to_bits()).unwrap();
        m.write_word(0x5000, 0x29000001).unwrap();
        assert_eq!(
            car_body_contact_filter(&mut m, CAR, &mut hit).unwrap(),
            0x5000
        );
    }
    #[test]
    fn zero_debris_count_continues_dynamic_impact_angles_health_and_flags() {
        let mut m = memory();
        let mut input = call([0., 10., 0.]);
        input.point = VehicleCarImpactPoint::Address(0);
        input.dynamic_owner = 0x7000;
        input.velocity = [3., 4., 0.];
        car_impact_callback(&mut m, CAR, input).unwrap();
        assert_eq!(f32::from_bits(m.read_word(CAR + 0x20c4).unwrap()), 90.);
        assert_eq!(m.read_word(CAR + 0x20cc).unwrap(), 0x7000);
        assert_eq!(m.read_byte(CAR + 0x20c8).unwrap(), 1);
        assert_eq!(m.read_word(CAR + 0x168).unwrap(), 2);
        assert_eq!(m.read_word(CAR + 0x216c).unwrap(), 0);
    }
    #[test]
    fn static_impact_applies_health_and_clears_collision_bit() {
        let mut m = memory();
        car_impact_callback(&mut m, CAR, call([0., 10., 0.])).unwrap();
        assert_eq!(f32::from_bits(m.read_word(CAR + 0x20c4).unwrap()), 90.);
        assert_eq!(m.read_byte(CAR + 0x20c8).unwrap(), 1);
        assert_eq!(m.read_word(CAR + 0x168).unwrap(), 2);
        assert_eq!(m.read_word(CAR + 0x216c).unwrap(), 0);
    }
    #[test]
    fn sideways_impact_uses_context_direction_and_publishes_original_flags() {
        let mut m = memory();
        m.write_word(CAR + 0xcac, 1_f32.to_bits()).unwrap();
        m.write_word(CAR + 0xcb4, 0).unwrap();
        car_impact_callback(&mut m, CAR, call([10., 0., 0.])).unwrap();
        assert_eq!(m.read_byte(CAR + 0x2215).unwrap(), 1);
        assert_eq!(m.read_byte(CAR + 0x2217).unwrap(), 1);
        assert_eq!(m.read_byte(CAR + 0x2214).unwrap(), 0);
        assert_eq!(m.read_byte(CAR + 0x2216).unwrap(), 0);
    }
    #[test]
    fn unsupported_particle_call_stops_before_later_health_and_flag_stores() {
        let mut m = memory();
        m.write_word(0x4028, u32::MAX).unwrap();
        assert_eq!(
            car_impact_callback(&mut m, CAR, call([0., 700., 0.])),
            Err(VehicleCarContactUnavailable::UnrecoveredBranch(
                VehicleCarContactBoundary::ImpactParticleBurst(7)
            ))
        );
        assert_eq!(m.read_word(CAR + 0x168).unwrap(), 3);
        assert_eq!(f32::from_bits(m.read_word(CAR + 0x20c4).unwrap()), 100.);
    }
}
