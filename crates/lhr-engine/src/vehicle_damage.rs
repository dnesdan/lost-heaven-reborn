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

/// One staged C_Vehicle breakage virtual intent: the dispatch below
/// resolved the receiver object and the vtable slot with the exact stack
/// arguments the native call carries, but the receiving classes are not
/// identified (unknown heap vtables), so no frame owner applies them.
/// Staged MissionHelpLog-style for the future component owner; the taxi
/// keeps the already-applied StaticResponse outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BreakageCall {
    /// Receiver object address as carried by the native call.
    pub object: u32,
    /// Vtable slot in bytes (0x18, 0x5c or 0x70).
    pub slot: u32,
    /// Callee-order stack arguments (last pushed first).
    pub args: [u32; 2],
    /// How many entries of [`BreakageCall::args`] are live.
    pub argc: u8,
}

/// One staged lamp/part-breakage dispatch: the DoDamage inputs plus the
/// virtual intents the native tail would have invoked, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LampBreakRequest {
    pub vehicle: u32,
    pub index: u32,
    pub side: u32,
    pub calls: Vec<BreakageCall>,
}

/// Original C_Vehicle breakage-dispatch helper, PC GAME 0x4d6580 (Game
/// RVA 0xd6580; working name — the original symbol is unrecovered).
/// Called from C_Vehicle::DoDamage's lamp arm (GAME 0x4d57c6, ECX =
/// vehicle, stack `(index, side)` with side 1/2 from the frame+0x80
/// sign) and from two further DoDamage arms (0x4d6261, 0x4d632b).
///
/// Complete 76-instruction REA walk (entry ev_f0711b3024, RET
/// ev_f057bb380f): null v+0x110 list returns at once; otherwise the
/// record count comes from the signed IMUL 0x92492493 + SAR 4 + bias
/// divide-by-28 over the [v+0x110, v+0x114) span (0x1C records), the
/// +0x4 fields are scanned in reverse for `index`, and on a match of
/// record m the low word at +0x18 is AND-masked with `side`, after
/// which surviving bit 0 fires the +0x70 virtual with the +0x14
/// source (bit 1 alone uses the +0x10 source) and three further
/// virtuals always fire (+0x18 on the +0x4 object, +0x5c with 1.0f on
/// the +0x8 object, +0x18 on the +0x8 object). The record topology
/// meaning is unrecovered beyond the arithmetic; the virtual
/// receivers' classes are unknown, so every virtual is staged, never
/// applied. Out-of-range memory reports instead of faulting (safe
/// divergence: native would read heap slack or fault).
pub fn vehicle_breakage_dispatch<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    index: u32,
    side: u32,
) -> Out<Vec<BreakageCall>, B::Error> {
    /// MSVC signed divide-by-28 idiom (IMUL 0x92492493, ADD, SAR 4,
    /// sign-bias correction), replicated with wrapping arithmetic.
    fn breakage_count(span: u32) -> i32 {
        let product = (0x92492493u32 as i32 as i64).wrapping_mul(span as i32 as i64);
        let mut edx = (product >> 32) as i32;
        edx = edx.wrapping_add(span as i32);
        edx >>= 4;
        let bias = ((edx as u32) >> 31) as i32;
        edx.wrapping_add(bias)
    }
    let mut calls = Vec::new();
    let begin = word(b, vehicle.wrapping_add(0x110))?;
    if begin == 0 {
        return Ok(calls);
    }
    let end = word(b, vehicle.wrapping_add(0x114))?;
    let mut remaining = breakage_count(end.wrapping_sub(begin));
    if remaining == 0 {
        return Ok(calls);
    }
    // Reverse scan of the +0x4 fields for `index`; EDX counts down to
    // the matched record number (0x4d65c6 loop with the per-iteration
    // live reread of the list head).
    let matched = loop {
        let stride = (remaining as u32)
            .wrapping_mul(7)
            .wrapping_mul(4)
            .wrapping_add(4);
        let record = word(b, begin.wrapping_add(stride).wrapping_sub(0x1c))?;
        remaining = remaining.wrapping_sub(1);
        if record == index {
            break Some(remaining);
        }
        if remaining == 0 {
            break None;
        }
    };
    let Some(matched) = matched else {
        return Ok(calls);
    };
    // Operate on the matched record: AND the +0x18 state word with the
    // side mask (low 16 bits only, high word preserved), then test its
    // low byte for the +0x70 variant while the tail always runs.
    let record = begin.wrapping_add((matched as u32).wrapping_mul(0x1c));
    let flag_addr = record.wrapping_add(0x18);
    let old = word(b, flag_addr)?;
    let mask = (side & 0xffff) as u16;
    put(
        b,
        flag_addr,
        (old & 0xffff_0000) | u32::from(old as u16 & mask),
    )?;
    let kept = (word(b, flag_addr)? & 0xff) as u8;
    if kept & 1 != 0 {
        let object = word(b, record.wrapping_add(0x4))?;
        let source = word(b, record.wrapping_add(0x14))?;
        calls.push(BreakageCall {
            object,
            slot: 0x70,
            args: [object, source],
            argc: 2,
        });
    } else if kept & 2 != 0 {
        let object = word(b, record.wrapping_add(0x4))?;
        let source = word(b, record.wrapping_add(0x10))?;
        calls.push(BreakageCall {
            object,
            slot: 0x70,
            args: [object, source],
            argc: 2,
        });
    }
    let lamp = word(b, record.wrapping_add(0x4))?;
    calls.push(BreakageCall {
        object: lamp,
        slot: 0x18,
        args: [lamp, 0],
        argc: 1,
    });
    let second = word(b, record.wrapping_add(0x8))?;
    calls.push(BreakageCall {
        object: second,
        slot: 0x5c,
        args: [second, 0x3f80_0000],
        argc: 2,
    });
    let second = word(b, record.wrapping_add(0x8))?;
    calls.push(BreakageCall {
        object: second,
        slot: 0x18,
        args: [second, 0],
        argc: 1,
    });
    Ok(calls)
}

#[cfg(test)]
mod breakage_tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;

    fn memory() -> VehicleFrameMemory {
        VehicleFrameMemory::default()
    }

    /// One 0x1C record list at `base` with `fields[i]` as the +0x4 key
    /// words and zeroed remainder; vehicle word at `vehicle` points at it.
    fn list(memory: &mut VehicleFrameMemory, vehicle: u32, base: u32, fields: &[u32]) {
        memory.map_bytes(vehicle & !0xfff, &[0; 0x1000]).unwrap();
        memory
            .map_bytes(base, &vec![0; fields.len() * 0x1c])
            .unwrap();
        memory.write_word(vehicle + 0x110, base).unwrap();
        memory
            .write_word(vehicle + 0x114, base + fields.len() as u32 * 0x1c)
            .unwrap();
        for (i, field) in fields.iter().enumerate() {
            memory
                .write_word(base + i as u32 * 0x1c + 0x4, *field)
                .unwrap();
        }
    }

    #[test]
    fn null_and_empty_lists_stage_nothing() {
        let mut m = memory();
        m.map_bytes(0x1000, &[0; 0x1000]).unwrap();
        // Null head: the JNZ-not-taken path returns at once.
        assert_eq!(
            vehicle_breakage_dispatch(&mut m, 0x1000, 7, 1).unwrap(),
            vec![]
        );
        // Empty span: count 0, DEC wraps to -1, JL returns.
        m.write_word(0x1110, 0x2000).unwrap();
        m.write_word(0x1114, 0x2000).unwrap();
        m.map_bytes(0x2000, &[0; 4]).unwrap();
        assert_eq!(
            vehicle_breakage_dispatch(&mut m, 0x1000, 7, 1).unwrap(),
            vec![]
        );
    }

    #[test]
    fn unmatched_index_stages_nothing_and_writes_nothing() {
        let mut m = memory();
        list(&mut m, 0x1000, 0x2000, &[0x11, 0x22]);
        m.write_word(0x2000 + 0x18, 0x93).unwrap();
        m.write_word(0x2000 + 0x1c + 0x18, 0x93).unwrap();
        assert_eq!(
            vehicle_breakage_dispatch(&mut m, 0x1000, 0x99, 1).unwrap(),
            vec![]
        );
        // Reverse scan read the +0x4 keys but the AND never ran.
        assert_eq!(m.read_word(0x2000 + 0x18).unwrap(), 0x93);
        assert_eq!(m.read_word(0x2000 + 0x1c + 0x18).unwrap(), 0x93);
        assert_eq!(m.read_word(0x2000 + 0x4).unwrap(), 0x11);
        assert_eq!(m.read_word(0x2000 + 0x1c + 0x4).unwrap(), 0x22);
    }

    #[test]
    fn side_mask_clears_the_other_bit_and_selects_the_slot70_source() {
        let mut m = memory();
        // Two records; the +0x4 keys are small ids, +0x8/+0x10/+0x14 are
        // fake object addresses, +0x18 words hold both state bits.
        list(&mut m, 0x1000, 0x2000, &[0x11, 0x22]);
        for (i, bits) in [0x93u32, 0x93].iter().enumerate() {
            let rec = 0x2000 + i as u32 * 0x1c;
            m.write_word(rec + 0x8, 0x5000 + i as u32).unwrap();
            m.write_word(rec + 0x10, 0x6000 + i as u32).unwrap();
            m.write_word(rec + 0x14, 0x7000 + i as u32).unwrap();
            m.write_word(rec + 0x18, *bits).unwrap();
        }
        // Match the second record (index 1) with side 1: every bit but
        // bit 0 clears, bit 0 survives and selects the +0x14 source.
        let calls = vehicle_breakage_dispatch(&mut m, 0x1000, 0x22, 1).unwrap();
        assert_eq!(m.read_word(0x2000 + 0x1c + 0x18).unwrap() & 0xffff, 0x01);
        // Untouched record keeps both bits.
        assert_eq!(m.read_word(0x2000 + 0x18).unwrap() & 0xffff, 0x93);
        assert_eq!(
            calls,
            vec![
                BreakageCall {
                    object: 0x22,
                    slot: 0x70,
                    args: [0x22, 0x7001],
                    argc: 2
                },
                BreakageCall {
                    object: 0x22,
                    slot: 0x18,
                    args: [0x22, 0],
                    argc: 1
                },
                BreakageCall {
                    object: 0x5001,
                    slot: 0x5c,
                    args: [0x5001, 0x3f80_0000],
                    argc: 2
                },
                BreakageCall {
                    object: 0x5001,
                    slot: 0x18,
                    args: [0x5001, 0],
                    argc: 1
                },
            ]
        );
        // Same record with side 2: every bit but bit 1 clears, bit 1
        // survives and selects the +0x10 source instead.
        m.write_word(0x2000 + 0x1c + 0x18, 0x93).unwrap();
        let calls = vehicle_breakage_dispatch(&mut m, 0x1000, 0x22, 2).unwrap();
        assert_eq!(m.read_word(0x2000 + 0x1c + 0x18).unwrap() & 0xffff, 0x02);
        assert_eq!(calls[0].args, [0x22, 0x6001]);
        assert_eq!(calls.len(), 4);
    }

    #[test]
    fn neither_state_bit_skips_only_the_slot70_call() {
        let mut m = memory();
        list(&mut m, 0x1000, 0x2000, &[0x44]);
        m.write_word(0x2008, 0x5100).unwrap();
        m.write_word(0x2018, 0x90).unwrap();
        let calls = vehicle_breakage_dispatch(&mut m, 0x1000, 0x44, 1).unwrap();
        // Bit 0 was clear, so the AND leaves neither bit: no 0x70 call,
        // but the three tail calls still stage.
        assert_eq!(m.read_word(0x2018).unwrap() & 0xffff, 0x00);
        assert_eq!(
            calls.iter().map(|call| call.slot).collect::<Vec<_>>(),
            vec![0x18, 0x5c, 0x18]
        );
        assert_eq!(calls[0].object, 0x44);
    }

    #[test]
    fn first_record_match_operates_like_any_other() {
        let mut m = memory();
        list(&mut m, 0x1000, 0x2000, &[0x77, 0x88]);
        m.write_word(0x2008, 0x5100).unwrap();
        m.write_word(0x2010, 0x6000).unwrap();
        m.write_word(0x2014, 0x7000).unwrap();
        m.write_word(0x2018, 0x3).unwrap();
        let calls = vehicle_breakage_dispatch(&mut m, 0x1000, 0x77, 2).unwrap();
        assert_eq!(m.read_word(0x2018).unwrap() & 0xffff, 0x2);
        assert_eq!(calls.len(), 4);
        // Bit 1 with side 2 selects the +0x10 source (0x6000), proving
        // the match on record 0 still runs the operate phase.
        assert_eq!(
            calls[0],
            BreakageCall {
                object: 0x77,
                slot: 0x70,
                args: [0x77, 0x6000],
                argc: 2
            }
        );
    }
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
