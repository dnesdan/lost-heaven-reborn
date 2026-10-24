//! Original C_Vehicle::DoDamage, PC RVA0xd5610, over the live car/frame owner.
//! The wheel suspension call supplies flags=0 and extra=0. Visual deformation
//! rows and damage callbacks remain typed dependencies with ordered effects.
use crate::{native_transform::SMatrix, vehicle_init::VehicleInitBackend};
#[path = "vehicle_damage_mesh.rs"]
mod mesh;
pub use mesh::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DamageVector {
    Address(u32),
    Value([f32; 3]),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleDamageCall {
    pub point: DamageVector,
    pub direction: DamageVector,
    pub radius: f32,
    pub strength: f32,
    pub flags: u32,
    pub extra: u32,
}
pub trait VehicleDamageBackend: VehicleDamageMeshBackend {
    fn damage_mesh_record(
        &mut self,
        vehicle: u32,
        index: u32,
        record: u32,
        call: &mut VehicleDamageCall,
        effective_strength: f32,
        factor: f32,
    ) -> Result<(), Self::Error>;
    fn damage_wheel_detach(
        &mut self,
        vehicle: u32,
        function: u32,
        context: u32,
        index: u32,
        velocity: [f32; 3],
        extra: u32,
    ) -> Result<(), Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleDamageUnavailable<E> {
    Effect(E),
    NumericDomain,
    ParticleTemplate(u32),
    StorageRange { begin: u32, end: u32, stride: u32 },
}
type Out<T, E> = Result<T, VehicleDamageUnavailable<E>>;
fn effect<E>(e: E) -> VehicleDamageUnavailable<E> {
    VehicleDamageUnavailable::Effect(e)
}
fn number<E>(x: f32) -> Out<f32, E> {
    crate::vehicle_dynamics_prepare::number::<()>(x)
        .map_err(|_| VehicleDamageUnavailable::NumericDomain)
}
macro_rules! operation {
    ($name:ident) => {
        fn $name<E>(a: f32, b: f32) -> Out<f32, E> {
            crate::vehicle_dynamics_prepare::$name::<()>(a, b)
                .map_err(|_| VehicleDamageUnavailable::NumericDomain)
        }
    };
}
operation!(add);
operation!(sub);
operation!(mul);
operation!(div);
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(effect)
}
fn byte<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a).map_err(effect)
}
fn half<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u16, B::Error> {
    Ok(u16::from_le_bytes([byte(b, a)?, byte(b, a + 1)?]))
}
fn float<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    number(f32::from_bits(word(b, a)?))
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, value: u32) -> Out<(), B::Error> {
    b.write_word(a, value).map_err(effect)
}
fn put_float<B: VehicleInitBackend>(b: &mut B, a: u32, value: f32) -> Out<(), B::Error> {
    put(b, a, value.to_bits())
}
fn vector<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([float(b, a)?, float(b, a + 4)?, float(b, a + 8)?])
}
fn source<B: VehicleInitBackend>(b: &mut B, v: DamageVector) -> Out<[f32; 3], B::Error> {
    match v {
        DamageVector::Address(a) => vector(b, a),
        DamageVector::Value(v) => {
            for x in v {
                number(x)?;
            }
            Ok(v)
        }
    }
}
fn count<B: VehicleInitBackend>(b: &mut B, h: u32, stride: u32) -> Out<u32, B::Error> {
    let begin = word(b, h)?;
    if begin == 0 {
        return Ok(0);
    }
    let end = word(b, h + 4)?;
    let n = end.wrapping_sub(begin) as i32;
    if n < 0 || !(n as u32).is_multiple_of(stride) {
        return Err(VehicleDamageUnavailable::StorageRange { begin, end, stride });
    }
    Ok(n as u32 / stride)
}
fn difference<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([sub(a[0], b[0])?, sub(a[1], b[1])?, sub(a[2], b[2])?])
}
fn squared<E>(v: [f32; 3], order: [usize; 3]) -> Out<f32, E> {
    add(
        add(
            mul(v[order[0]], v[order[0]])?,
            mul(v[order[1]], v[order[1]])?,
        )?,
        mul(v[order[2]], v[order[2]])?,
    )
}
fn cross<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([
        sub(mul(a[1], b[2])?, mul(a[2], b[1])?)?,
        sub(mul(a[2], b[0])?, mul(a[0], b[2])?)?,
        sub(mul(a[0], b[1])?, mul(a[1], b[0])?)?,
    ])
}

pub fn vehicle_do_damage<B: VehicleDamageBackend>(
    b: &mut B,
    v: u32,
    call: &mut VehicleDamageCall,
) -> Out<(), B::Error> {
    let visual_only = call.flags & 2 != 0;
    if !visual_only && (call.flags & 1 != 0 || float(b, v + 0xc8)? < number(call.strength)?) {
        let percentage = mul(float(b, v + 0xd4)?, 0.01)?;
        // The original loads the lamp header between these two FMULs.
        let begin = word(b, v + 0x100)?;
        let loss = mul(
            div(
                sub(call.strength, mul(percentage, call.strength)?)?,
                float(b, v + 0xc4)?,
            )?,
            float(b, v + 0xcc)?,
        )?;
        let n = if begin == 0 {
            0
        } else {
            let end = word(b, v + 0x104)?;
            let bytes = end.wrapping_sub(begin) as i32;
            if bytes < 0 || !(bytes as u32).is_multiple_of(0x20) {
                return Err(VehicleDamageUnavailable::StorageRange {
                    begin,
                    end,
                    stride: 0x20,
                });
            }
            bytes as u32 / 0x20
        };
        for i in (0..n).rev() {
            let begin = word(b, v + 0x100)?;
            let record = begin + i * 0x20;
            if byte(b, record + 0x14)? & 1 != 0 {
                continue;
            }
            let part = half(b, record + 0x12)?;
            if (part as i16) >= 0 {
                let parts = word(b, v + 0x218)?;
                if half(b, parts + u32::from(part) * 0x34 + 0xc)? == 5 {
                    continue;
                }
            }
            let frame = word(b, record)?;
            if byte(b, frame + 0xac)? & 0x20 == 0 {
                b.damage_update_world(frame).map_err(effect)?;
            }
            let d = difference(vector(b, frame + 0x40)?, source(b, call.point)?)?;
            if squared(d, [2, 1, 0])? >= mul(call.radius, call.radius)? {
                continue;
            }
            let begin = word(b, v + 0x100)?;
            let record = begin + i * 0x20;
            let health = sub(float(b, record + 0x18)?, loss)?;
            put_float(b, record + 0x18, health)?;
            let begin = word(b, v + 0x100)?;
            let record = begin + i * 0x20;
            if float(b, record + 0x18)? < float(b, v + 0xd0)? {
                let flags = word(b, record + 0x14)?;
                put(b, record + 0x14, flags | 1)?;
            }
            let begin = word(b, v + 0x100)?;
            let record = begin + i * 0x20;
            if float(b, record + 0x18)? < 0. {
                put(b, record + 0x18, 0)?;
            }
            let begin = word(b, v + 0x100)?;
            let record = begin + i * 0x20;
            if (word(b, record + 0x14)? as i32) < 0 {
                let frame = word(b, record)?;
                let side = if float(b, frame + 0x80)? < 0. { 1 } else { 2 };
                let index = word(b, record + 8)?;
                b.damage_lamp_switch(v, index, side).map_err(effect)?;
            }
        }
    }
    let _squared_radius = mul::<B::Error>(call.radius, call.radius)?;
    let n = count(b, v + 0x218, 0x34)?;
    for i in (0..n).rev() {
        let begin = word(b, v + 0x218)?;
        let record = begin + i * 0x34;
        if byte(b, record + 0xe)? & 1 != 0 {
            continue;
        }
        if call.flags & 1 == 0 && float(b, record + 0x14)? >= number(call.strength)? {
            continue;
        }
        let effective = sub(
            call.strength,
            mul(mul(float(b, record + 0x24)?, 0.01)?, call.strength)?,
        )?;
        let factor = div(effective, float(b, record + 0x10)?)?;
        let factor = if factor > 1. { 1. } else { factor };
        b.damage_mesh_record(v, i, record, call, effective, factor)
            .map_err(effect)?;
    }
    if !visual_only && word(b, v + 0xc68)? & 0x4000 != 0 {
        let point = source(b, call.point)?;
        let mut elements = [0.; 16];
        for (i, x) in elements.iter_mut().enumerate() {
            *x = f32::from_bits(word(b, v + 0xd28 + i as u32 * 4)?);
        }
        let point = SMatrix { elements }
            .try_transform_point(point)
            .map_err(|_| VehicleDamageUnavailable::NumericDomain)?;
        let n = word(b, v + 0x4c4)?;
        for i in (0..n).rev() {
            let wheels = word(b, v + 0xc38)?;
            let wheel = word(b, wheels + i * 4)?;
            if word(b, wheel + 0x120)? & 0x40000000 != 0 {
                continue;
            }
            if number(call.strength)? <= float(b, wheel + 0x19c)? {
                continue;
            }
            let square = squared(difference(vector(b, wheel + 0x10)?, point)?, [0, 2, 1])?;
            let near = if (square.to_bits() as i32) < 0x322bcc77 {
                true
            } else if f64::from(sub(square, 1.)?.abs()) < 1e-8 {
                false
            } else {
                let length = square.sqrt();
                crate::native_precision::record_sqrt(square, length);
                number(length)? < 1.
            };
            if !near {
                continue;
            }
            let flags = word(b, v + 0xf8)?;
            put(b, v + 0xf8, flags | 2)?;
            let flags = word(b, wheel + 0x120)?;
            if flags & 0x10000000 != 0 {
                continue;
            }
            let percent = mul(float(b, wheel + 0x194)?, 0.01)?;
            put(b, wheel + 0x120, flags | 0x10000000)?;
            let loss = mul(
                div(
                    sub(call.strength, mul(percent, call.strength)?)?,
                    float(b, wheel + 0x198)?,
                )?,
                float(b, wheel + 0x190)?,
            )?;
            let health = sub(float(b, wheel + 0x18c)?, loss)?;
            put_float(b, wheel + 0x18c, health)?;
            if health < 0. {
                put(b, wheel + 0x18c, 0)?;
                let r = difference(vector(b, wheel + 0x1c)?, vector(b, v + 0x398)?)?;
                let spin = cross(r, vector(b, v + 0x420)?)?;
                let velocity = difference(vector(b, v + 0x1f90)?, spin)?;
                let function = word(b, v + 0x2fc)?;
                let context = word(b, v + 0xc74)?;
                b.damage_wheel_detach(v, function, context, i, velocity, call.extra)
                    .map_err(effect)?;
            }
        }
    }
    Ok(())
}

/// PC 0x4df510: original material particle timer gates. A live template's
/// property/emission path remains explicit until its original owner is bound.
pub fn vehicle_damage_timer<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    _milliseconds: u32,
    _point: [f32; 3],
    material: u32,
) -> Out<bool, B::Error> {
    if word(b, v + 0xc68)? & 0x200 == 0 {
        return Ok(false);
    }
    let index = u32::from(byte(b, 0x63d8b0 + material)?);
    let table = word(b, 0x63d9b4)?;
    let raw = word(b, table + index * 0x30 + 0x28)? as i32;
    let index = raw.unsigned_abs();
    if index == 0 {
        return Ok(false);
    }
    let begin = word(b, 0x63d8a4)?;
    if begin == 0 {
        return Ok(false);
    }
    let end = word(b, 0x63d8a8)?;
    let length = end.wrapping_sub(begin) as i32;
    if length < 0 || !(length as u32).is_multiple_of(0x1c) {
        return Err(VehicleDamageUnavailable::StorageRange {
            begin,
            end,
            stride: 0x1c,
        });
    }
    if index >= length as u32 / 0x1c {
        return Ok(false);
    }
    let template = word(b, begin + index * 0x1c)?;
    if template == 0 {
        return Ok(false);
    }
    Err(VehicleDamageUnavailable::ParticleTemplate(template))
}

#[cfg(test)]
mod timer_tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;
    #[test]
    fn disabled_damage_particles_do_not_read_unbound_managers() {
        let mut m = VehicleFrameMemory::default();
        m.map_bytes(0x1000, &[0; 0x1000]).unwrap();
        assert!(!vehicle_damage_timer(&mut m, 0x1000, 16, [0.; 3], 0).unwrap());
    }
    #[test]
    fn material_particle_id_uses_signed_absolute_and_real_template_registry() {
        let mut m = VehicleFrameMemory::default();
        m.map_bytes(0x1000, &[0; 0x1000]).unwrap();
        m.map_bytes(0x63d8a4, &[0; 0x114]).unwrap();
        m.map_bytes(0x3000, &[0; 0x30]).unwrap();
        m.map_bytes(0x4000, &[0; 0x54]).unwrap();
        m.write_word(0x1c68, 0x200).unwrap();
        m.write_word(0x63d9b4, 0x3000).unwrap();
        m.write_word(0x3028, (-2_i32) as u32).unwrap();
        assert!(!vehicle_damage_timer(&mut m, 0x1000, 16, [0.; 3], 0).unwrap());
        m.write_word(0x63d8a4, 0x4000).unwrap();
        m.write_word(0x63d8a8, 0x4054).unwrap();
        assert!(!vehicle_damage_timer(&mut m, 0x1000, 16, [0.; 3], 0).unwrap());
        m.write_word(0x4038, 0x5000).unwrap();
        assert_eq!(
            vehicle_damage_timer(&mut m, 0x1000, 16, [0.; 3], 0),
            Err(VehicleDamageUnavailable::ParticleTemplate(0x5000))
        );
    }
}
