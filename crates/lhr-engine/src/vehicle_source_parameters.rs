//! Original PC InitCar source-parameter application, RVA 0xc60a0.
//! Source, wheel and global table storage remain live backend memory. Repair
//! mode writes the original fallback values back to that same source record.
use super::component_tail::Projection;
use super::wheels::WheelsResult;
use super::*;
use crate::native_matrix::{checked_f32_add, checked_f32_mul};

pub trait VehicleSourceParametersBackend: VehicleInitBackend {
    fn parameter_update_frame_world(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn parameter_frame_world_scale(&mut self, frame: u32) -> Result<[u32; 3], Self::Error>;
    /// Actual InitSounds dependency (RVA0xeac70), after the source copy.
    /// Some supplies the canonical VehicleInitState memory; None means the
    /// backend itself owns every addressed vehicle byte. An unavailable audio
    /// implementation must reject this call. Original Setup passes flags zero.
    fn parameter_initialize_sounds(
        &mut self,
        memory: Option<VehicleComponentTailMemory<'_>>,
        vehicle: u32,
        source: u32,
    ) -> Result<(), Self::Error>;
}
impl<B: VehicleSourceParametersBackend> VehicleSourceParametersBackend for Projection<'_, B> {
    fn parameter_update_frame_world(&mut self, f: u32) -> Result<(), Self::Error> {
        self.backend.parameter_update_frame_world(f)
    }
    fn parameter_frame_world_scale(&mut self, f: u32) -> Result<[u32; 3], Self::Error> {
        self.backend.parameter_frame_world_scale(f)
    }
    fn parameter_initialize_sounds(
        &mut self,
        memory: Option<VehicleComponentTailMemory<'_>>,
        v: u32,
        s: u32,
    ) -> Result<(), Self::Error> {
        self.backend.parameter_initialize_sounds(memory, v, s)
    }
}
type R<B, T> = Result<T, VehicleWheelsUnavailable<<B as VehicleInitBackend>::Error>>;
fn numeric<E>(value: f32) -> Result<f32, VehicleWheelsUnavailable<E>> {
    if value == 0. || value.is_normal() {
        Ok(value)
    } else {
        Err(VehicleWheelsUnavailable::NumericDomain)
    }
}
fn add<E>(a: f32, b: f32) -> Result<f32, VehicleWheelsUnavailable<E>> {
    checked_f32_add(a, b).map_err(|_| VehicleWheelsUnavailable::NumericDomain)
}
fn sub<E>(a: f32, b: f32) -> Result<f32, VehicleWheelsUnavailable<E>> {
    add(a, -b)
}
fn mul<E>(a: f32, b: f32) -> Result<f32, VehicleWheelsUnavailable<E>> {
    checked_f32_mul(a, b).map_err(|_| VehicleWheelsUnavailable::NumericDomain)
}
fn div<E>(a: f32, b: f32) -> Result<f32, VehicleWheelsUnavailable<E>> {
    let result = numeric(a / b)?;
    crate::native_precision::record_div(a, b, result);
    Ok(result)
}
struct Parameters<'a, B: VehicleSourceParametersBackend> {
    b: &'a mut B,
    v: u32,
    s: u32,
    repair: bool,
}
impl<B: VehicleSourceParametersBackend> Parameters<'_, B> {
    fn word(&mut self, a: u32) -> R<B, u32> {
        self.b
            .read_word(a)
            .map_err(|e| VehicleInitUnavailable::Effect(e).into())
    }
    fn byte(&mut self, a: u32) -> R<B, u8> {
        self.b
            .read_byte(a)
            .map_err(|e| VehicleInitUnavailable::Effect(e).into())
    }
    fn put(&mut self, a: u32, x: u32) -> R<B, ()> {
        self.b
            .write_word(a, x)
            .map_err(|e| VehicleInitUnavailable::Effect(e).into())
    }
    fn put_byte(&mut self, a: u32, x: u8) -> R<B, ()> {
        self.b
            .write_byte(a, x)
            .map_err(|e| VehicleInitUnavailable::Effect(e).into())
    }
    fn float(&mut self, a: u32) -> R<B, f32> {
        numeric(f32::from_bits(self.word(a)?))
    }
    fn sf(&mut self, i: u32) -> R<B, f32> {
        self.float(self.s.wrapping_add(i.wrapping_mul(4)))
    }
    fn vf(&mut self, o: u32) -> R<B, f32> {
        self.float(self.v.wrapping_add(o))
    }
    fn vw(&mut self, o: u32) -> R<B, u32> {
        self.word(self.v.wrapping_add(o))
    }
    fn set(&mut self, a: u32, x: f32) -> R<B, ()> {
        self.put(a, x.to_bits())
    }
    fn vc(&mut self, o: u32, x: u32) -> R<B, ()> {
        self.put(self.v.wrapping_add(o), x)
    }
    fn vs(&mut self, o: u32, x: f32) -> R<B, ()> {
        self.set(self.v.wrapping_add(o), x)
    }
    fn copy_source(&mut self, i: u32, o: u32) -> R<B, ()> {
        let x = self.word(self.s.wrapping_add(i * 4))?;
        self.vc(o, x)
    }
    fn copy_vehicle(&mut self, from: u32, to: u32) -> R<B, ()> {
        let x = self.vw(from)?;
        self.vc(to, x)
    }
    fn wheel(&mut self, i: u32) -> R<B, u32> {
        let base = self.vw(0xc38)?;
        self.word(base.wrapping_add(i * 4))
    }
    fn nonempty(&mut self, i: u32) -> R<B, bool> {
        let mut a = self.s.wrapping_add(i * 4);
        let mut n = 0u32;
        loop {
            let byte = self.byte(a)?;
            a = a.wrapping_add(1);
            if byte == 0 {
                return Ok(n != 0);
            }
            n = n
                .checked_add(1)
                .ok_or(VehicleWheelsUnavailable::NumericDomain)?;
        }
    }
    fn clamp(&mut self, a: u32, max: f32) -> R<B, ()> {
        if self.float(a)? > max {
            self.set(a, max)?;
        }
        Ok(())
    }
    fn ranged(&mut self, i: u32, d: u32, low: f32, high: f32, scale: f32, back: f32) -> R<B, bool> {
        let x = self.sf(i)?;
        if x < low || x > high {
            if !self.repair {
                return Ok(false);
            }
            let old = self.float(d)?;
            self.set(self.s.wrapping_add(i * 4), mul(old, back)?)?;
        } else {
            self.set(d, mul(x, scale)?)?;
        }
        Ok(true)
    }
    fn positive(
        &mut self,
        i: u32,
        d: u32,
        min: f32,
        scale: f32,
        back: f32,
        strict: bool,
    ) -> R<B, bool> {
        let x = self.sf(i)?;
        if if strict { x <= min } else { x < min } {
            if !self.repair {
                return Ok(false);
            }
            let old = self.float(d)?;
            self.set(self.s.wrapping_add(i * 4), mul(old, back)?)?;
        } else {
            self.set(d, mul(x, scale)?)?;
        }
        Ok(true)
    }
    fn reciprocal(&mut self, i: u32, o: u32, strict: bool) -> R<B, bool> {
        let x = self.sf(i)?;
        if if strict { x <= 0. } else { x < 0. } {
            if !self.repair {
                return Ok(false);
            }
            let old = self.vf(o)?;
            self.set(self.s.wrapping_add(i * 4), div(1., old)?)?;
        } else {
            self.vs(o, div(1., x)?)?;
        }
        Ok(true)
    }
    fn percent(
        &mut self,
        i: u32,
        o: u32,
        fallback: u32,
        min_source: Option<u32>,
        speed: bool,
    ) -> R<B, bool> {
        let x = self.sf(i)?;
        let low = if let Some(j) = min_source {
            self.sf(j)?
        } else {
            0.
        };
        if x < low || x > 100.001 {
            if !self.repair {
                return Ok(false);
            }
            let old = self.vf(fallback)?;
            self.set(self.s.wrapping_add(i * 4), old)?;
        } else {
            let factor = mul(x, 0.01)?;
            let value = if speed {
                let high = self.vf(0x1f9c)?;
                let low = self.vf(0x5ec)?;
                add(mul(factor, sub(high, low)?)?, low)?
            } else {
                mul(factor, self.vf(0x388)?)?
            };
            self.vs(o, value)?;
        }
        let high = self.vf(if speed { 0x1f9c } else { 0x388 })?;
        self.clamp(self.v.wrapping_add(o), high)?;
        Ok(true)
    }
    fn volume(&mut self, i: u32, o: u32) -> R<B, bool> {
        if !self.ranged(i, self.v.wrapping_add(o), 0., 1.001, 1., 1.)? {
            return Ok(false);
        }
        self.clamp(self.v.wrapping_add(o), 1.)?;
        Ok(true)
    }
    fn pitches(&mut self, i: u32, j: u32, o: u32, p: u32) -> R<B, bool> {
        let low = self.sf(i)?;
        let high = self.sf(j)?;
        if low < 0.5 || high <= low {
            if !self.repair {
                return Ok(false);
            }
            let old = self.vf(o)?;
            self.set(self.s.wrapping_add(i * 4), old)?;
        }
        let low = self.sf(i)?;
        self.vs(o, low)?;
        let high = self.sf(j)?;
        if high > 2.01 || high < low {
            if !self.repair {
                return Ok(false);
            }
            let old = self.vf(p)?;
            self.set(self.s.wrapping_add(j * 4), old)?;
        } else {
            self.vs(p, high)?;
        }
        self.clamp(self.v.wrapping_add(p), 2.)?;
        Ok(true)
    }
    fn sound_bank(
        &mut self,
        name: u32,
        src: u32,
        dst: u32,
        fallback: u32,
        index: u32,
        flag: Option<(u32, u32)>,
    ) -> R<B, bool> {
        if !self.nonempty(name + index * 4)? {
            return Ok(true);
        }
        let i = index;
        let d = i * 4;
        if !self.pitches(src + i, src + 5 + i, dst + d, dst + 20 + d)? {
            return Ok(false);
        }
        for k in 0..4 {
            let field = src + 10 + k * 5 + i;
            let minimum = if k == 1 || k == 3 {
                Some(field - 5)
            } else {
                None
            };
            if !self.percent(
                field,
                dst + 40 + k * 20 + d,
                fallback + k * 20 + d,
                minimum,
                true,
            )? {
                return Ok(false);
            }
        }
        for k in 0..2 {
            if !self.volume(src + 30 + k * 5 + i, dst + 120 + k * 20 + d)? {
                return Ok(false);
            }
        }
        if let Some((from, to)) = flag {
            let x = self.byte(self.s.wrapping_add(from + i))?;
            self.put_byte(self.v.wrapping_add(to + i), x)?;
        }
        Ok(true)
    }
}

impl VehicleInitState {
    pub fn init_car_parameters<B: VehicleSourceParametersBackend>(
        &mut self,
        b: &mut B,
        source: u32,
        mode: u8,
    ) -> WheelsResult<B, bool> {
        let base = self
            .tail_vehicle_address
            .ok_or(VehicleWheelsUnavailable::NoContinuation)?;
        let result = {
            let mut memory = self.tail_projection(b, base);
            apply_parameters(&mut memory, base, source, mode & !2)?
        };
        if result && mode & 2 != 0 {
            b.parameter_initialize_sounds(Some(self.live_tail_memory()), base, base + 0xe6c)
                .map_err(VehicleInitUnavailable::Effect)?;
        }
        Ok(result)
    }
}
/// Direct storage entry. The backend owns all addressed vehicle bytes. For a
/// VehicleInitState projection use init_car_parameters, which supplies its live
/// memory view to the optional sound dependency instead of forwarding snapshots.
pub fn vehicle_init_car_parameters<B: VehicleSourceParametersBackend>(
    b: &mut B,
    vehicle: u32,
    source: u32,
    mode: u8,
) -> R<B, bool> {
    let accepted = apply_parameters(b, vehicle, source, mode & !2)?;
    if accepted && mode & 2 != 0 {
        b.parameter_initialize_sounds(None, vehicle, vehicle + 0xe6c)
            .map_err(VehicleInitUnavailable::Effect)?;
    }
    Ok(accepted)
}
pub(super) fn apply_parameters<B: VehicleSourceParametersBackend>(
    b: &mut B,
    vehicle: u32,
    source: u32,
    mode: u8,
) -> R<B, bool> {
    let mut p = Parameters {
        b,
        v: vehicle,
        s: source,
        repair: mode & 1 != 0,
    };
    macro_rules! valid {
        ($e:expr) => {
            if !$e? {
                return Ok(false);
            }
        };
    }
    macro_rules! field {
        ($i:expr,$o:expr,$min:expr) => {
            valid!(p.positive($i, vehicle + $o, $min, 1., 1., false));
        };
    }
    macro_rules! positive {
        ($i:expr,$o:expr) => {
            valid!(p.positive($i, vehicle + $o, 0., 1., 1., true));
        };
    }
    for (i, o) in [
        (0x184, 0x2c4),
        (0x185, 0x2c8),
        (0x186, 0x2cc),
        (0x187, 0x2d0),
    ] {
        p.copy_source(i, o)?;
    }
    for (i, o) in [(0x17f * 4, 0x98), (0x179 * 4, 0x13c)] {
        let x = p.byte(source + i)?;
        p.put_byte(vehicle + o, x)?;
    }
    field!(0x17a, 0x38c, 0.);
    field!(0x17b, 0x390, 1.);
    p.copy_source(0x17c, 0x224)?;
    for (i, o) in [(0x5f4, 0x228), (0x5f5, 0x229)] {
        let x = p.byte(source + i)?;
        p.put_byte(vehicle + o, x)?;
    }
    let x = p.byte(source + 0x5f6)?;
    p.vc(0xb8, if x == 0 { 0 } else { 2 })?;
    p.copy_source(0x17e, 0x18)?;
    let raw = p.word(source + 0x180 * 4)?;
    if raw as i32 >= 0 {
        p.vc(0x460, raw)?;
    } else if p.repair {
        let old = p.vw(0x460)?;
        p.put(source + 0x180 * 4, old)?;
    } else {
        return Ok(false);
    }
    valid!(p.positive(0x181, vehicle + 0x468, 0., 0.2777778, 3.6, false));
    field!(0x182, 0x46c, 0.);
    let low = p.sf(0x182)?;
    valid!(p.positive(0x183, vehicle + 0x470, low, 1., 1., false));
    for (i, o) in [(0x172, 0x448), (0x173, 0x44c), (0x174, 0x444)] {
        valid!(p.reciprocal(i, o, true));
    }
    for (a, d) in [(0x444, 0x438), (0x448, 0x43c), (0x44c, 0x440)] {
        p.copy_vehicle(a, d)?;
    }
    for (i, o) in [(0x44, 0x604), (0x43, 0x600), (0x42, 0x5fc)] {
        valid!(p.volume(i, o));
    }
    for k in 0..3 {
        let hi = p.vf(0x370 + 4 * k)?;
        let lo = p.vf(0x364 + 4 * k)?;
        let ratio = p.vf(0x5fc + 4 * k)?;
        p.vs(0x404 + 4 * k, add(mul(sub(hi, lo)?, ratio)?, lo)?)?;
    }
    for (a, d) in [(0x404, 0x3f8), (0x408, 0x3fc), (0x40c, 0x400)] {
        p.copy_vehicle(a, d)?;
    }
    positive!(0x45, 0x49c);
    positive!(0, 0x5f8);
    p.copy_vehicle(0x5f8, 0x5f4)?;
    p.vc(0x410, 0)?;
    let force = mul(p.vf(0x5f8)?, p.vf(0x49c)?)?;
    p.vs(0x414, -force)?;
    p.vc(0x418, 0)?;
    positive!(0x176, 0x3c8);
    valid!(p.positive(0x175, vehicle + 0x3c4, 0., 1000., 0.001, false));
    positive!(0x178, 0x3b4);
    valid!(p.positive(0x177, vehicle + 0x3b0, 0., 1000., 0.001, false));
    p.vc(0x5b0, 0x38d1b717)?;
    p.vc(0x5ac, 0x3c23d70a)?;
    let mut axle = 0u32;
    while (axle as i32) < p.vw(0x4c0)? as i32 {
        for side in 0..2u32 {
            let index = axle.wrapping_mul(2).wrapping_add(side);
            let w = p.wheel(index)?;
            for (i, o, min, scale, back, strict) in [
                (0x48, 0x88, 0., 1000., 0.001, true),
                (0x52, 0x8c, 0.009, 1., 1., true),
                (0x5c, 0x98, 0., 1., 1., true),
                (0x66, 0x9c, 0., 1., 1., true),
            ] {
                valid!(p.positive(i + axle, w + o, min, scale, back, strict));
            }
            let kind = p.word(source + (0x70 + axle) * 4)?;
            if (kind as i32) < 0 {
                if !p.repair {
                    return Ok(false);
                }
                p.put(source + (0x70 + axle) * 4, 0)?;
            } else {
                p.put(w + 0xa8, kind)?;
            }
            let begin = p.word(0x63d9c8)?;
            let mut found = -1i32;
            if begin != 0 {
                let end = p.word(0x63d9cc)?;
                let count = (end.wrapping_sub(begin) as i32) / 40;
                for slot in (0..count.max(0)).rev() {
                    let id = p.word(begin.wrapping_add(slot as u32 * 40))?;
                    if id == p.word(w + 0xa8)? {
                        found = slot;
                        break;
                    }
                }
            }
            p.put(w + 0xac, found as u32)?;
            if found < 0 {
                p.put(w + 0xa8, u32::MAX)?;
            }
            p.put(w + 0xb4, 0)?;
            for (i, o, min, scale, back, strict) in [
                (0x7a, 0xb8, 0., 1000., 0.001, true),
                (0x84, 0xbc, 1., 0.017453292, 57.295776, false),
                (0x8e, 0xc0, 0., 1., 1., false),
                (0x98, 0xc4, 0., 0.2777778, 3.6, false),
                (0xac, 0xd4, 0., 1., 1., true),
            ] {
                valid!(p.positive(i + axle, w + o, min, scale, back, strict));
            }
            for (i, o, top) in [
                (0xa2, 0xd0, 0xd4),
                (0xb6, 0xd8, 0xdc),
                (0xfc, 0xf4, 0xf8),
                (0x110, 0xfc, 0x100),
            ] {
                if i == 0xb6 {
                    valid!(p.positive(0xc0 + axle, w + 0xdc, 0., 1., 1., true));
                }
                if i == 0xfc {
                    for (si, wo, min, scale, back, strict) in [
                        (0xca, 0xe0, 0., 1., 1., false),
                        (0xd4, 0xe4, 0., 1000., 0.001, true),
                        (0xde, 0xe8, 1., 0.017453292, 57.295776, false),
                        (0xe8, 0xec, 0., 1., 1., false),
                        (0xf2, 0xf0, 0., 0.2777778, 3.6, false),
                        (0x106, 0xf8, 0., 1., 1., true),
                    ] {
                        valid!(p.positive(si + axle, w + wo, min, scale, back, strict));
                    }
                }
                if i == 0x110 {
                    valid!(p.positive(0x11a + axle, w + 0x100, 0., 1., 1., true));
                }
                let x = p.sf(i + axle)?;
                if x > 0. {
                    let hi = p.float(w + top)?;
                    p.set(w + o, sub(hi, x)?)?;
                } else {
                    if !p.repair {
                        return Ok(false);
                    }
                    let hi = p.float(w + top)?;
                    let lo = p.float(w + o)?;
                    p.set(source + (i + axle) * 4, sub(hi, lo)?)?;
                }
            }
            valid!(p.positive(0x124 + axle, w + 0x104, 0., 1., 1., false));
            valid!(p.ranged(0x12e + axle, w + 0x11c, 0., 100., 0.01, 100.));
            for (from, bit) in [(0x4e0, 2u32), (0x4ea, 1), (0x4f4, 4)] {
                let flag = p.byte(source + from + axle)?;
                let old = p.word(w + 0x120)?;
                p.put(w + 0x120, if flag == 0 { old & !bit } else { old | bit })?;
            }
            let flags = p.word(w + 0x120)?;
            p.put(w + 0x120, flags & !0x200)?;
            let mut collision = false;
            for (h, e) in [(0xdc, 0xe0), (0xec, 0xf0)] {
                if collision {
                    break;
                }
                let begin = p.vw(h)?;
                if begin != 0 {
                    let end = p.vw(e)?;
                    let count = (end.wrapping_sub(begin) as i32) / 12;
                    for n in (0..count.max(0)).rev() {
                        let row = begin.wrapping_add(n as u32 * 12);
                        let a = p.byte(row + 8)? as u16 | ((p.byte(row + 9)? as u16) << 8);
                        let flags = p.byte(row + 10)? as u16 | ((p.byte(row + 11)? as u16) << 8);
                        if flags & 1 != 0 && a as u32 == index {
                            collision = true;
                            let flags = p.word(w + 0x120)?;
                            p.put(w + 0x120, flags | 0x200)?;
                            break;
                        }
                    }
                }
            }
            let si = if side == 0 { 0x140 } else { 0x14a };
            let x = p.sf(si + axle)?;
            if x >= 0. {
                let mass = p.vf(0x3c4)?;
                p.set(w + 0x118, mul(mul(x, mass)?, 0.01)?)?;
            } else {
                if !p.repair {
                    return Ok(false);
                }
                let old = p.vf(if side == 0 { 0x136c } else { 0x1394 } + axle * 4)?;
                p.set(source + (si + axle) * 4, old)?;
            }
            valid!(p.ranged(0x154 + axle, w + 0x170, 1., 90., 0.017453292, 57.295776));
            if p.sf(0x168 + axle)? < p.sf(0x15e + axle)? {
                return Ok(false);
            }
            for (si, wo) in [(0x15e, 0x174), (0x168, 0x178)] {
                valid!(p.ranged(si + axle, w + wo, 1., 90., 0.017453292, 57.295776));
            }
            let flags = p.byte(w + 0x120)?;
            if flags & 2 != 0 {
                // Preserve the evidenced single-precision constant, one ULP below TAU.
                let circumference = mul(p.float(w + 0x10c)?, f32::from_bits(0x40c90fda))?;
                if p.vf(0x5ac)? < circumference {
                    p.vs(0x5ac, circumference)?;
                    let radius = p.word(w + 0x10c)?;
                    p.vc(0x5b0, radius)?;
                }
            }
            for (si, wo) in [
                (0x189, 0x1a8),
                (0x18a, 0x1a4),
                (0x18b, 0x1b0),
                (0x18c, 0x1ac),
                (0x18d, 0x1b4),
            ] {
                let raw = p.word(source + si * 4)?;
                p.put(w + wo, raw)?;
            }
        }
        axle = axle.wrapping_add(1);
    }
    let x = p.byte(source + 0x188 * 4)?;
    p.vc(0x14c, 0)?;
    p.put_byte(vehicle + 0x13d, x)?;
    let count = p.vw(0x4c4)? as i32;
    for i in 0..count.max(0) as u32 {
        let w = p.wheel(i)?;
        let a = p.float(w + 0x118)?;
        let b = p.vf(0x14c)?;
        p.vs(0x14c, add(a, b)?)?;
    }
    p.vc(0x42c, 0)?;
    for i in 0..count.max(0) as u32 {
        let w = p.wheel(i)?;
        if p.byte(w + 0x120)? & 2 != 0 {
            let x = p.vf(0x42c)?;
            p.vs(0x42c, add(x, 1.)?)?;
        }
    }
    if p.byte(vehicle + 0xc70)? & 2 == 0 && p.vf(0x42c)? == 0. {
        return Ok(false);
    }
    for (si, vo) in [(0x3b, 0x608), (0x3c, 0x60c)] {
        let x = p.sf(si)?;
        if x <= 0. || x > 50. {
            if !p.repair {
                return Ok(false);
            }
            let old = p.vf(vo)?;
            p.set(source + si * 4, mul(old, 57.295776)?)?;
        } else {
            p.vs(vo, mul(x, 0.017453292)?)?;
        }
    }
    let maximum = p.sf(0x3b)?;
    valid!(p.ranged(
        0x3d,
        vehicle + 0x62c,
        0.,
        50f32.min(maximum),
        0.017453292,
        57.295776
    ));
    for (si, vo) in [(0x3e, 0x614), (0x3f, 0x618), (0x40, 0x61c), (0x41, 0x620)] {
        valid!(p.positive(si, vehicle + vo, 0., 0.017453292, 57.295776, true));
    }
    for (si, vo) in [(0x35, 0x538), (0x37, 0x3d0), (0x39, 0x3d8)] {
        valid!(p.ranged(si, vehicle + vo, 0.029, 1.331, 1., 1.));
        field!(si + 1, vo + 4, 0.);
    }
    for (a, d) in [
        (0x404, 0x3e0),
        (0x408, 0x3e4),
        (0x378, 0x3e8),
        (0x404, 0x3ec),
        (0x408, 0x3f0),
        (0x36c, 0x3f4),
    ] {
        p.copy_vehicle(a, d)?;
    }
    valid!(p.positive(3, vehicle + 0x564, 0., 0.016666668, 60., false));
    let low = p.sf(3)?;
    valid!(p.positive(4, vehicle + 0x1f9c, low, 0.016666668, 60., true));
    let x = p.sf(5)?;
    if x <= 0. || x > p.sf(3)? {
        if !p.repair {
            return Ok(false);
        }
        let old = p.vf(0x5ec)?;
        p.set(source + 5 * 4, mul(old, 60.)?)?;
    } else {
        p.vs(0x5ec, mul(x, 0.016666668)?)?;
    }
    let x = p.vf(0x5ec)?;
    p.vs(0x558, mul(x, 0.5)?)?;
    let x = p.sf(0xd)?;
    if x >= 0. {
        p.vs(0x5d0, x)?;
    } else if !p.repair {
        return Ok(false);
    }
    valid!(p.reciprocal(0xe, 0x5d8, false));
    valid!(p.reciprocal(0xf, 0x5d4, false));
    let hi = p.sf(0x12)?;
    valid!(p.ranged(0x10, vehicle + 0x5c0, 0., hi, 0.016666668, 60.));
    let hi = p.sf(0x12)?;
    valid!(p.ranged(
        0x11,
        vehicle + 0x5c4,
        f32::NEG_INFINITY,
        hi,
        0.016666668,
        60.
    ));
    let hi = p.sf(3)?;
    valid!(p.ranged(
        0x12,
        vehicle + 0x5c8,
        f32::NEG_INFINITY,
        hi,
        0.016666668,
        60.
    ));
    let gear = p.sf(0x13)?;
    if !(1. ..=10.).contains(&gear) {
        if !p.repair {
            return Ok(false);
        }
        let old = p.vw(0x568)? as i32;
        let restored = old as f32;
        crate::native_precision::record_precision(i64::from(old) != restored as i64);
        p.set(source + 0x13 * 4, restored)?;
    } else {
        let integral = gear.trunc() as i32;
        crate::native_precision::record_precision(integral as f32 != gear);
        p.vc(0x568, integral as u32)?;
    }
    for i in 0..10 {
        field!(0x15 + i, 0x574 + i * 4, 0.);
    }
    field!(0x14, 0x56c, 0.);
    p.copy_vehicle(0x574, 0x570)?;
    let circumference = p.vf(0x5ac)?;
    let gear = p.vw(0x568)?;
    let ratio = p.vf(0x570u32.wrapping_add(gear.wrapping_mul(4)))?;
    let rpm = p.vf(0x1f9c)?;
    p.vs(0x388, mul(div(circumference, ratio)?, rpm)?)?;
    for i in 0..10 {
        for (si, vo, fallback, clamp_high) in [
            (0x20 + i, 0x4d8 + i * 4, 0xeec + i * 4, true),
            (0x2b + i, 0x508 + i * 4, 0xf18 + i * 4, false),
        ] {
            let x = p.sf(si)?;
            if x < 0. || x > 100.0001 {
                if !p.repair {
                    return Ok(false);
                }
                let old = p.vf(fallback)?;
                p.set(source + si * 4, old)?;
            } else {
                let high = p.vf(0x1f9c)?;
                let low = p.vf(0x5ec)?;
                p.vs(vo, add(mul(mul(x, 0.01)?, sub(high, low)?)?, low)?)?;
            }
            let value = p.vf(vo)?;
            let limit = p.vf(if clamp_high { 0x1f9c } else { 0x5ec })?;
            if if clamp_high {
                value > limit
            } else {
                value < limit
            } {
                p.vs(vo, limit)?;
            }
        }
    }
    p.vc(0x508, 0)?;
    valid!(p.positive(1, vehicle + 0x530, 0., 1000., 0.001, true));
    valid!(p.positive(2, vehicle + 0x3ac, 0., 0.01, 100., true));
    let power = mul(p.vf(0x3ac)?, p.vf(0x530)?)?;
    p.vs(0x530, power)?;
    let rpm = mul(p.sf(3)?, 0.016666668)?;
    p.vs(0x48c, rpm)?;
    p.vs(0x47c, div(power, mul(rpm, f32::from_bits(0x40c90fda))?)?)?;
    let x = p.sf(9)?;
    if x <= p.sf(5)? || p.sf(10)? < x {
        if !p.repair {
            return Ok(false);
        }
        let old = p.vf(0x484)?;
        p.set(source + 9 * 4, mul(old, 60.)?)?;
    } else {
        p.vs(0x484, mul(x, 0.016666668)?)?;
    }
    let x = p.sf(10)?;
    if x < p.sf(9)? || x >= p.sf(3)? {
        if !p.repair {
            return Ok(false);
        }
        let old = p.vf(0x488)?;
        p.set(source + 10 * 4, mul(old, 60.)?)?;
    } else {
        p.vs(0x488, mul(x, 0.016666668)?)?;
    }
    for (si, vo) in [(6, 0x478), (7, 0x474), (8, 0x480), (0xb, 0x5bc)] {
        positive!(si, vo);
    }
    let torque = mul(div(p.vf(0x5ec)?, p.vf(0x1f9c)?)?, p.vf(0x5bc)?)?;
    let friction = p.vf(0x474)?;
    p.vs(0x24, div(torque, add(torque, friction)?)?)?;
    positive!(0xc, 0x41c);
    field!(0x46, 0x1fa0, 1.);
    let fuel = p.sf(0x47)?;
    if fuel > 0. {
        let x = mul(mul(p.sf(4)?, 0.016666668)?, 0.41666666)?;
        p.vs(0x498, x)?;
        let gear = p.vw(0x568)?;
        let ratio = p.vf(0x570u32.wrapping_add(gear.wrapping_mul(4)))?;
        let x = div(x, ratio)?;
        p.vs(0x498, x)?;
        let x = div(100000., mul(x, p.vf(0x5ac)?)?)?;
        p.vs(0x498, x)?;
        p.vs(0x498, mul(div(fuel, x)?, 1.7)?)?;
    } else {
        if !p.repair {
            return Ok(false);
        }
        let old = p.vf(0xf88)?;
        p.set(source + 0x47 * 4, old)?;
    }
    if p.sf(0x1eb)? < p.sf(0x1ea)? {
        if !p.repair {
            return Ok(false);
        }
        for (si, vo) in [(0x1ea, 0x1614), (0x1eb, 0x1618)] {
            let old = p.vf(vo)?;
            p.set(source + si * 4, mul(old, 3.6)?)?;
        }
    }
    for (si, vo) in [(0x1ea, 0x188), (0x1eb, 0x18c)] {
        let x = p.sf(si)?;
        p.vs(vo, mul(x, 0.2777778)?)?;
    }
    p.copy_source(0x1ec, 0x190)?;
    let x = mul(mul(p.sf(0x1ed)?, p.vf(0x5bc)?)?, -0.01)?;
    p.vs(0x63c, x)?;
    let x = mul(mul(p.sf(0x1ee)?, p.vf(0x478)?)?, 0.01)?;
    p.vs(0x640, x)?;
    p.copy_source(0x1ef, 0x644)?;
    p.copy_source(0x1f0, 0x648)?;
    // Four adjacent records are visited backwards; float and byte arrays have
    // deliberately different strides and may overlap the source under repair.
    for i in (0..4).rev() {
        let x = mul(mul(p.sf(0x314 + i)?, 0.2777778)?, p.vf(0x5f4)?)?;
        p.vs(0x918 + i * 4, x)?;
        let x = p.byte(source + 0xc60 + i)?;
        p.put_byte(vehicle + 0x928 + i, x)?;
        let x = mul(mul(p.sf(0x444 + i)?, 0.2777778)?, p.vf(0x5f4)?)?;
        p.vs(0xc0c + i * 4, x)?;
        let x = p.byte(source + 0x1120 + i)?;
        p.put_byte(vehicle + 0xc1c + i, x)?;
    }
    for (name, si, vo, fallback) in [(0x239, 0x30d, 0x8dc, 0x1aa0), (0x365, 0x43d, 0xbd0, 0x1f60)] {
        if p.nonempty(name)? {
            valid!(p.percent(si, vo, fallback, None, true));
            let x = mul(p.sf(5)?, 0.016666668)?;
            p.vs(vo + 4, x)?;
            valid!(p.volume(si + 1, vo + 8));
        }
    }
    for (name, si, vo, fallback) in [(0x23d, 0x30f, 0x8e8, 0x1ab0), (0x369, 0x43f, 0xbdc, 0x1f70)] {
        if p.nonempty(name)? {
            valid!(p.pitches(si, si + 1, vo, vo + 4));
            valid!(p.percent(si + 2, vo + 8, fallback, None, false));
            let x = p.vw(0x388)?;
            p.vc(vo + 12, x)?;
            valid!(p.percent(si + 3, vo + 16, fallback + 4, None, false));
            valid!(p.volume(si + 4, vo + 20));
        }
    }
    for (vo, a, b) in [
        (0x900, 0x3f0ccccd, 0x3fa66666),
        (0xbf4, 0x3f0ccccd, 0x3fb33333),
    ] {
        p.vc(vo, a)?;
        p.vc(vo + 4, b)?;
        for (o, factor) in [(8, 0.01), (12, 0.6), (16, 0.3)] {
            let x = p.vf(0x388)?;
            p.vs(vo + o, mul(x, factor)?)?;
        }
        p.vc(vo + 20, 0x3f800000)?;
    }
    if p.nonempty(0x325)? {
        valid!(p.pitches(0x3bd, 0x3be, 0x9d0, 0x9d4));
        valid!(p.percent(0x3bf, 0x9d8, 0x1d68, None, true));
        valid!(p.volume(0x3c0, 0x9dc));
    }
    for i in 0..5 {
        valid!(p.sound_bank(0x1fd, 0x291, 0x6ec, 0x18d8, i, Some((0xae4, 0x78c))));
        valid!(p.sound_bank(0x329, 0x3c1, 0x9e0, 0x1d98, i, Some((0xfa4, 0xa80))));
    }
    for i in 0..5 {
        valid!(p.sound_bank(0x211, 0x2bb, 0x794, 0x1980, i, Some((0xb8c, 0x834))));
        valid!(p.sound_bank(0x33d, 0x3eb, 0xa88, 0x1e40, i, Some((0x104c, 0xb28))));
    }
    for i in 0..5 {
        valid!(p.sound_bank(0x225, 0x2e5, 0x83c, 0x1a28, i, None));
        valid!(p.sound_bank(0x351, 0x415, 0xb30, 0x1ee8, i, None));
    }
    for (si, vo, scale, gravity) in [
        (0x18e, 0x1c0, 0.2777778, true),
        (0x18f, 0x1c4, 0.2777778, true),
        (0x190, 0x1b8, 1., false),
        (0x191, 0x1bc, 1., false),
        (0x192, 0x1ac, 1., false),
        (0x193, 0x1c8, 1., false),
        (0x194, 0x1cc, 1., false),
        (0x195, 0x1d0, 0.2777778, false),
        (0x196, 0x1d4, 1., false),
        (0x197, 0x1d8, 1., false),
        (0x198, 0x1f8, 0.2777778, true),
        (0x199, 0x1fc, 0.2777778, true),
        (0x19a, 0x1f0, 1., false),
        (0x19b, 0x1f4, 1., false),
    ] {
        let mut x = mul(p.sf(si)?, scale)?;
        if gravity {
            x = mul(x, p.vf(0x5f4)?)?;
        }
        p.vs(vo, x)?;
    }
    let count = p.vw(0x4c4)? as i32;
    for i in 0..count.max(0) as u32 {
        let w = p.wheel(i)?;
        for (si, wo, scale, gravity) in [
            (0x1a0, 0x184, 0.017453292, false),
            (0x19e, 0x190, 1., false),
            (0x1a1, 0x1a0, 1., false),
            (0x19c, 0x198, 0.2777778, true),
            (0x19d, 0x19c, 0.2777778, true),
            (0x19f, 0x194, 1., false),
        ] {
            let mut x = mul(p.sf(si)?, scale)?;
            if gravity {
                x = mul(x, p.vf(0x5f4)?)?;
            }
            p.set(w + wo, x)?;
        }
    }
    for (si, vo, scale, gravity) in [
        (0x1bf, 0xc4, 0.2777778, true),
        (0x1c0, 0xc8, 0.2777778, true),
        (0x1c1, 0xcc, 1., false),
        (0x1c2, 0xd0, 1., false),
        (0x1c3, 0xd4, 1., false),
    ] {
        let mut x = mul(p.sf(si)?, scale)?;
        if gravity {
            x = mul(x, p.vf(0x5f4)?)?;
        }
        p.vs(vo, x)?;
    }
    let mut index = 0u32;
    loop {
        let begin = p.vw(0x218)?;
        if begin == 0 {
            break;
        }
        let end = p.vw(0x21c)?;
        let count = ((end.wrapping_sub(begin) as i32) / 52) as u32;
        if index >= count {
            break;
        }
        let control = p.vw(0x208)?;
        if control != 0 {
            let end = p.vw(0x20c)?;
            let count = ((end.wrapping_sub(control) as i32) / 52) as u32;
            if index < count {
                let row = begin.wrapping_add(index * 52);
                let kind = p.byte(row + 12)? as u16 | ((p.byte(row + 13)? as u16) << 8);
                let si = match kind {
                    1 => Some(0x1a2),
                    2 => Some(0x1a8),
                    3 => Some(0x1af),
                    4 => Some(0x1ce),
                    5 => Some(0x1bf),
                    6 => Some(0x1d5),
                    7 => Some(0x1c7),
                    8 => Some(0x1dc),
                    9 => Some(0x1e3),
                    _ => None,
                };
                if let Some(si) = si {
                    for (n, o) in [
                        (0, 0x10),
                        (1, 0x14),
                        (2, 0x18),
                        (if kind == 5 { 4 } else { 3 }, 0x24),
                        (if kind == 5 { 5 } else { 4 }, 0x28),
                        (if kind == 5 { 6 } else { 5 }, 0x2c),
                        (if kind == 5 { 7 } else { 6 }, 0x30),
                    ] {
                        let parameter_index = if kind == 1 && o >= 0x24 {
                            si + n - 1
                        } else {
                            si + n
                        };
                        let mut x = if kind == 1 && o == 0x18 {
                            100.
                        } else {
                            p.sf(parameter_index)?
                        };
                        if o == 0x10 || o == 0x14 {
                            x = mul(mul(x, 0.2777778)?, p.vf(0x5f4)?)?;
                        } else if o >= 0x28 {
                            x = mul(x, 0.01)?;
                        }
                        if kind == 1 && o == 0x18 {
                            x = 100.;
                        }
                        let a = p.vw(0x208)?.wrapping_add(index * 52 + o);
                        p.set(a, x)?;
                        let x = p.word(a)?;
                        let a = p.vw(0x218)?.wrapping_add(index * 52 + o);
                        p.put(a, x)?;
                    }
                    if kind == 3 || kind == 5 {
                        let x = p.sf(if kind == 3 { 0x1b6 } else { 0x1c2 })?;
                        let a = p.vw(0x208)?.wrapping_add(index * 52 + 0x1c);
                        p.set(a, x)?;
                        let x = p.word(a)?;
                        let a = p.vw(0x218)?.wrapping_add(index * 52 + 0x1c);
                        p.put(a, x)?;
                    }
                }
            }
        }
        index = index.wrapping_add(1);
    }
    p.vc(0x354, 0x3f800000)?;
    let count = p.vw(0x4c4)? as i32;
    if count > 2 {
        let first = p.wheel(0)?;
        let last = p.wheel(count as u32 - 1)?;
        let rear = p.wheel(count as u32 - 2)?;
        let first_parent = p.word(first + 8)?;
        let rear_parent = p.word(rear + 8)?;
        let distance = sub(p.float(first_parent + 0x88)?, p.float(rear_parent + 0x88)?)?;
        let model = p.word(first_parent + 0x120)?;
        let scale_z = if p.word(model + 0x120)? == 0 {
            p.float(model + 0x98)?
        } else {
            if p.byte(model + 0xac)? & 0x20 == 0 {
                p.b.parameter_update_frame_world(model)
                    .map_err(VehicleInitUnavailable::Effect)?;
            }
            numeric(f32::from_bits(
                p.b.parameter_frame_world_scale(model)
                    .map_err(VehicleInitUnavailable::Effect)?[2],
            ))?
        };
        let length = mul(scale_z, distance)?;
        let front_steer = p.byte(first + 0x120)? & 1 != 0;
        let rear_steer = p.byte(last + 0x120)? & 1 != 0;
        if front_steer || rear_steer {
            let angle = sub(
                f32::from_bits(0x40490fdb),
                add(p.vf(0x608)?, f32::from_bits(0x3fc90fdb))?,
            )?;
            // FCOS retains its extended result until the following PC24 division.
            crate::native_precision::record_precision(angle != 0.0);
            let cosine = f64::from(angle).cos();
            let ratio = numeric((f64::from(length) / cosine) as f32)?;
            let square = sub(mul(ratio, ratio)?, mul(length, length)?)?;
            let radius = numeric(square.sqrt())?;
            crate::native_precision::record_sqrt(square, radius);
            p.vs(0x354, radius)?;
            if front_steer && rear_steer {
                p.vs(0x354, mul(radius, 0.5)?)?;
            }
        } else {
            p.vc(0x350, 0x4cbebc20)?;
        }
        let radius = p.vf(0x354)?;
        p.vs(0x354, mul(radius, 1.1)?)?;
    }
    for i in 0..0x449u32 {
        let x = p.word(source.wrapping_add(i * 4))?;
        p.vc(0xe6c + i * 4, x)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Memory {
        bytes: Vec<u8>,
    }
    impl VehicleInitBackend for Memory {
        type Error = &'static str;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.bytes
                .get(a as usize)
                .copied()
                .ok_or("unavailable byte")
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            let mut b = [0; 4];
            for (i, x) in b.iter_mut().enumerate() {
                *x = self.read_byte(a + i as u32)?;
            }
            Ok(u32::from_le_bytes(b))
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            *self.bytes.get_mut(a as usize).ok_or("unavailable byte")? = v;
            Ok(())
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.write_byte(a + i as u32, b)?;
            }
            Ok(())
        }
        fn allocate(&mut self, _: u32) -> Result<u32, Self::Error> {
            Err("unexpected allocation")
        }
        fn free(&mut self, _: u32) -> Result<(), Self::Error> {
            Err("unexpected free")
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err("unexpected collector")
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            Err(VehicleInitUnavailable::Effect("unexpected enumeration"))
        }
    }
    impl VehicleSourceParametersBackend for Memory {
        fn parameter_update_frame_world(&mut self, _: u32) -> Result<(), Self::Error> {
            Err("unavailable frame")
        }
        fn parameter_frame_world_scale(&mut self, _: u32) -> Result<[u32; 3], Self::Error> {
            Err("unavailable frame")
        }
        fn parameter_initialize_sounds(
            &mut self,
            _: Option<VehicleComponentTailMemory<'_>>,
            _: u32,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err("unavailable sound")
        }
    }
    #[test]
    fn invalid_parameter_preserves_earlier_stores_and_unvisited_bytes() {
        let mut b = Memory {
            bytes: vec![0; 0x8000],
        };
        let v = 0x1000;
        let s = 0x4000;
        b.write_word(s + 0x184 * 4, 0x7fc01234).unwrap();
        b.write_word(s + 0x17a * 4, (-1f32).to_bits()).unwrap();
        b.write_word(v + 0x390, 0xdeadbeef).unwrap();
        assert_eq!(vehicle_init_car_parameters(&mut b, v, s, 0), Ok(false));
        assert_eq!(b.read_word(v + 0x2c4), Ok(0x7fc01234));
        assert_eq!(b.read_word(s + 0x17a * 4), Ok((-1f32).to_bits()));
        assert_eq!(b.read_word(v + 0x390), Ok(0xdeadbeef));
    }
    #[test]
    fn repair_updates_live_source_before_later_unavailable_numeric_operation() {
        let mut b = Memory {
            bytes: vec![0; 0x8000],
        };
        let v = 0x1000;
        let s = 0x4000;
        b.write_word(s + 0x17a * 4, (-1f32).to_bits()).unwrap();
        b.write_word(v + 0x38c, 2f32.to_bits()).unwrap();
        b.write_word(v + 0x390, 3f32.to_bits()).unwrap();
        b.write_word(v + 0x448, 0).unwrap();
        assert_eq!(
            vehicle_init_car_parameters(&mut b, v, s, 1),
            Err(VehicleWheelsUnavailable::NumericDomain)
        );
        assert_eq!(b.read_word(s + 0x17a * 4), Ok(2f32.to_bits()));
        assert_eq!(b.read_word(s + 0x17b * 4), Ok(3f32.to_bits()));
    }
}
