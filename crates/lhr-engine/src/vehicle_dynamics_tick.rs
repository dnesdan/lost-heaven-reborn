//! Original PC C_Vehicle::Tick, canonical RVA df9a0. Milliseconds originate at
//! the caller; zero becomes one before conversion with original binary32 .001.
//! Effects and drivetrain calls operate on this same addressable owner.
use crate::native_transform::{try_vector_angle_sine_scaled, MakeUnavailable};
use crate::vehicle_dynamics_prepare::{add, div, mul, number, sub};
use crate::vehicle_init::VehicleInitBackend;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehicleTickEffect {
    SteeringWheels {
        seconds: f32,
    },
    EngineSound {
        enabled: bool,
    },
    /// Working address labels until original PS2 correspondence is established.
    ProcedureEbe40 {
        seconds: f32,
    },
    ProcedureEeb50,
    ProcedureEd910,
}
pub trait VehicleTickBackend: VehicleInitBackend {
    fn tick_effect(
        &mut self,
        vehicle: u32,
        effect: VehicleTickEffect,
    ) -> std::result::Result<(), Self::Error>;
    fn tick_set_gear(&mut self, vehicle: u32, gear: i32) -> std::result::Result<(), Self::Error>;
    fn tick_motor_force(
        &mut self,
        vehicle: u32,
        dt: f32,
        power: f32,
    ) -> std::result::Result<f32, Self::Error>;
    fn tick_motor_rot(
        &mut self,
        vehicle: u32,
        dt: f32,
        force: f32,
    ) -> std::result::Result<(), Self::Error>;
    fn tick_engine(
        &mut self,
        vehicle: u32,
        dt: f32,
        first_force: f32,
        second_force: f32,
    ) -> std::result::Result<(), Self::Error>;
    /// Original PC helper RVA cc630, output float via its second argument.
    /// None is its false return and leaves the caller's prior local unchanged.
    fn tick_maximum_moment(
        &mut self,
        vehicle: u32,
        rotation: f32,
        gear: i32,
        reverse: bool,
    ) -> std::result::Result<Option<f32>, Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleTickUnavailable<E> {
    Effect(E),
    NumericDomain,
    Library(MakeUnavailable),
    /// Other modes leave native force locals without a recovered definition.
    UnrecoveredMode(u32),
}
type Out<T, E> = std::result::Result<T, VehicleTickUnavailable<E>>;
impl<E> From<crate::vehicle_dynamics_prepare::VehicleMovePrepareUnavailable<E>>
    for VehicleTickUnavailable<E>
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
struct Tick<'a, B> {
    b: &'a mut B,
    v: u32,
}
// FILD retains the complete unsigned integer; FMUL then rounds its product
// once to PC24. Casting the integer to f32 before multiplication rounds twice.
fn milliseconds_to_seconds(milliseconds: u32) -> f32 {
    let product = u64::from(milliseconds) * u64::from(0x0083_126f_u32);
    let bits = 64 - product.leading_zeros();
    let shift = bits.saturating_sub(24);
    let mut rounded = product >> shift;
    if shift != 0 {
        let remainder = product & ((1u64 << shift) - 1);
        crate::native_precision::record_precision(remainder != 0);
        let halfway = 1u64 << (shift - 1);
        if remainder > halfway || remainder == halfway && rounded & 1 != 0 {
            rounded += 1;
        }
    }
    (rounded as f64 * 2f64.powi(shift as i32 - 33)) as f32
}
/// Original PC `_GetMaxMomentByRot`, RVA cc630, over addressable owner state.
/// A false native return is None and leaves any caller output unchanged.
/// `compute` is the original ComputeMoment dependency, shared with drivetrain.
pub fn vehicle_get_max_moment_by_rot<B, F>(
    b: &mut B,
    vehicle: u32,
    rpm: f32,
    gear: i32,
    include_drag: bool,
    mut compute: F,
) -> Out<Option<f32>, B::Error>
where
    B: VehicleInitBackend,
    F: FnMut(&mut B, u32, f32, f32) -> std::result::Result<f32, B::Error>,
{
    if gear < -1 {
        return Ok(None);
    }
    let maximum = b
        .read_word(vehicle.wrapping_add(0x568))
        .map_err(VehicleTickUnavailable::Effect)? as i32;
    if gear > maximum {
        return Ok(None);
    }
    let lower = f32::from_bits(
        b.read_word(vehicle.wrapping_add(0xe80))
            .map_err(VehicleTickUnavailable::Effect)?,
    );
    if add(rpm, f32::from_bits(0x3a83126f))? < number(lower)? {
        return Ok(None);
    }
    let upper = f32::from_bits(
        b.read_word(vehicle.wrapping_add(0xe7c))
            .map_err(VehicleTickUnavailable::Effect)?,
    );
    if sub(rpm, f32::from_bits(0x3a83126f))? > number(upper)? {
        return Ok(None);
    }
    let rotation = mul(rpm, f32::from_bits(0x3c888889))?;
    let mut moment = compute(b, vehicle, rotation, 1.).map_err(VehicleTickUnavailable::Effect)?;
    if include_drag {
        let drag = compute(b, vehicle, rotation, 0.).map_err(VehicleTickUnavailable::Effect)?;
        moment = add(drag, moment)?;
    }
    if gear != 0 {
        let ratio = f32::from_bits(
            b.read_word(
                vehicle
                    .wrapping_add(0x570)
                    .wrapping_add((gear as u32).wrapping_mul(4)),
            )
            .map_err(VehicleTickUnavailable::Effect)?,
        );
        moment = mul(moment, ratio)?;
    }
    Ok(Some(moment))
}
impl<B: VehicleTickBackend> Tick<'_, B> {
    fn w(&mut self, o: u32) -> Out<u32, B::Error> {
        self.b
            .read_word(self.v.wrapping_add(o))
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn f(&mut self, o: u32) -> Out<f32, B::Error> {
        Ok(number(f32::from_bits(self.w(o)?))?)
    }
    fn byte(&mut self, o: u32) -> Out<u8, B::Error> {
        self.b
            .read_byte(self.v.wrapping_add(o))
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn put(&mut self, o: u32, v: u32) -> Out<(), B::Error> {
        self.b
            .write_word(self.v.wrapping_add(o), v)
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn pf(&mut self, o: u32, v: f32) -> Out<(), B::Error> {
        self.put(o, v.to_bits())
    }
    fn pb(&mut self, o: u32, v: u8) -> Out<(), B::Error> {
        self.b
            .write_byte(self.v.wrapping_add(o), v)
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn copy(&mut self, to: u32, from: u32) -> Out<(), B::Error> {
        let value = self.w(from)?;
        self.put(to, value)
    }
    fn vec(&mut self, o: u32) -> Out<[f32; 3], B::Error> {
        Ok([self.f(o)?, self.f(o + 4)?, self.f(o + 8)?])
    }
    fn effect(&mut self, e: VehicleTickEffect) -> Out<(), B::Error> {
        self.b
            .tick_effect(self.v, e)
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn gear(&mut self, g: i32) -> Out<(), B::Error> {
        self.b
            .tick_set_gear(self.v, g)
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn wheel_flags(&mut self, index: u32) -> Out<u32, B::Error> {
        let pointers = self.w(0xc38)?;
        let wheel = self
            .b
            .read_word(pointers.wrapping_add(index.wrapping_mul(4)))
            .map_err(VehicleTickUnavailable::Effect)?;
        self.b
            .read_word(wheel.wrapping_add(0x120))
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn motor_force(&mut self, dt: f32, power: f32) -> Out<f32, B::Error> {
        self.b
            .tick_motor_force(self.v, dt, power)
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn motor_rot(&mut self, dt: f32, force: f32) -> Out<(), B::Error> {
        self.b
            .tick_motor_rot(self.v, dt, force)
            .map_err(VehicleTickUnavailable::Effect)
    }
    fn automatic_gear(&mut self, dt: f32, mut throttle: f32) -> Out<u32, B::Error> {
        let gear = self.w(0x55c)? as i32;
        let mut next = gear.wrapping_add(1) as u32;
        let delta = sub(
            self.f(0x548)?,
            mul(
                div(self.f(0x59c)?, self.f(0x5ac)?)?,
                self.f(0x570u32.wrapping_add((gear as u32).wrapping_mul(4)))?,
            )?,
        )?;
        let mut eligible = !(delta.abs() > 3.0 && delta > 0.0);
        if gear != 0 && eligible {
            let mut i = 0;
            while (i as i32) < self.w(0x4c4)? as i32 {
                let flags = self.wheel_flags(i)?;
                if flags & 0x202 != 0 && flags & 0x80 != 0 {
                    eligible = false;
                    break;
                }
                i += 1;
            }
        }
        let config = self.w(0xc68)?;
        if (config & 0x4000 == 0 || self.f(0x1ec)? > 0.0)
            && self.byte(0xc2c)? != 0
            && self.byte(0x4cc)? != 0
            && self.byte(0x2c0)? != 0
            && self.w(0x1a4)? & 0x1000000 == 0
            && (self.f(0x5dc)? > 0.9 || self.byte(0x5e8)? & 0x10 != 0)
            && gear != 0
            && (self.byte(0x394)? != 0 || self.byte(0x99)? != 0)
            && self.w(0x5e8)? & 0xffffffcf == 0
        {
            let flags = self.w(0x1a4)?;
            let mut selected = None;
            if gear < 0 {
                if self.f(0xc64)? < f32::from_bits(0x3fc90fdb)
                    && throttle < 0.3
                    && flags & 6 == 0
                    && self.f(0x59c)? > 1.0
                {
                    self.put(0x55c, 1)?;
                    self.put(0x560, 1)?;
                    next = 2;
                } else if self.f(0x5b8)? > 0.3
                    && self.f(0x548)? <= add(self.f(0x5ec)?, 0.01)?
                    && self.f(0x59c)? < 1.0
                {
                    let timer = add(dt, self.f(0x494)?)?;
                    self.pf(0x494, timer)?;
                    if timer > 0.002 {
                        selected = Some(1);
                    }
                }
            } else if self.f(0xc64)? > f32::from_bits(0x3fc90fdb)
                && throttle < 0.3
                && flags & 4 == 0
                && matches!(self.w(0xb8)?, 0 | 2)
                && self.f(0x59c)? > 1.0
            {
                self.put(0x55c, u32::MAX)?;
                self.put(0x560, u32::MAX)?;
                next = 0;
            } else if self.f(0x548)? > self.f(0x4d0u32.wrapping_add(next.wrapping_mul(4)))?
                && gear < (self.w(0x568)? as i32)
                && eligible
            {
                if self.f(0xc40)? > 0.0 {
                    let direction = self.vec(0xc3c)?;
                    let flat = [direction[0], 0.0, direction[2]];
                    let mass = self.f(0x5f4)?;
                    let gravity = self.f(0x49c)?;
                    let sine_mass = try_vector_angle_sine_scaled(direction, flat, [mass, 0., 0.])
                        .map_err(VehicleTickUnavailable::Library)?[0];
                    let slope = mul(sine_mass, gravity)?;
                    let rotation = mul(self.f(0x504u32.wrapping_add(next.wrapping_mul(4)))?, 60.0)?;
                    let actual_next = (self.w(0x55c)? as i32).wrapping_add(1);
                    let moment = self
                        .b
                        .tick_maximum_moment(self.v, rotation, actual_next, false)
                        .map_err(VehicleTickUnavailable::Effect)?
                        .unwrap_or(throttle);
                    if div(moment, self.f(0x5b0)?)? > number(slope)? {
                        selected = Some((self.w(0x55c)? as i32).wrapping_add(1));
                    }
                } else {
                    selected = Some(next as i32);
                }
            } else {
                let mut downshift = false;
                if self.f(0x548)? < self.f(0x500u32.wrapping_add(next.wrapping_mul(4)))?
                    && gear > 1
                    && self.f(0x5b8)? < 0.3
                {
                    let mut choice = 2i32;
                    if choice < (next as i32) {
                        throttle = div(1.0, self.f(0x56cu32.wrapping_add(next.wrapping_mul(4)))?)?;
                        let mut offset = 0x574u32;
                        loop {
                            let at = mul(mul(throttle, self.f(offset)?)?, self.f(0x548)?)?;
                            if self.f(offset.wrapping_sub(0x6c))? <= at
                                && at < self.f(offset.wrapping_sub(0x9c))?
                            {
                                break;
                            }
                            choice += 1;
                            offset = offset.wrapping_add(4);
                            if choice >= next as i32 {
                                break;
                            }
                        }
                    }
                    selected = Some(choice - 1);
                } else {
                    if gear > 1 {
                        let high = self.f(0x4d0u32.wrapping_add((gear as u32).wrapping_mul(4)))?;
                        let low = self.f(0x500u32.wrapping_add((gear as u32).wrapping_mul(4)))?;
                        downshift = self.f(0x548)? < sub(high, mul(sub(high, low)?, 0.25)?)?
                            && throttle > 0.3
                            && self.f(0x3bc)? < 0.3
                            && matches!(self.w(0xb8)?, 0 | 2);
                    }
                    if downshift {
                        selected = Some(gear - 1);
                    } else if self.f(0x548)? <= add(self.f(0x5ec)?, 0.01)?
                        && matches!(self.w(0xb8)?, 0 | 2)
                        && self.f(0x5b8)? > 0.3
                        && self.f(0x59c)? < 1.0
                    {
                        let timer = add(dt, self.f(0x494)?)?;
                        self.pf(0x494, timer)?;
                        if timer > 0.002 {
                            selected = Some(-1);
                        }
                    }
                }
            }
            if let Some(gear) = selected {
                self.gear(gear)?;
            }
        }
        Ok(next)
    }
    fn clutch(&mut self, next: u32, first_force: f32, second_force: f32) -> Out<(), B::Error> {
        if !matches!(self.w(0xb8)?, 0 | 2) || self.w(0x5e8)? & 0x20 == 0 {
            return Ok(());
        }
        let gear = self.w(0x55c)? as i32;
        let mut flags = self.w(0x5e8)?;
        if gear == 0 {
            if flags & 4 != 0 {
                self.pf(0x5cc, -1.0)?;
                self.put(0x5e8, (flags & 0xfffffff3) | 1)?;
            }
            return Ok(());
        }
        let wheel_rotation = mul(
            div(self.f(0x59c)?, self.f(0x5ac)?)?,
            self.f(0x570u32.wrapping_add((gear as u32).wrapping_mul(4)))?,
        )?;
        self.put(0x5e8, flags & 0xffffffef)?;
        let power = self.f(0x540)?;
        let threshold = self.f(0x24)?;
        if self.f(0x548)? > self.f(0x5ec)? {
            if power > threshold {
                if self.f(0x548)? < add(self.f(0x5c0)?, 0.1)? {
                    flags = (flags & 0xffffffef) | 4;
                    self.put(0x5e8, flags)?;
                }
            } else {
                if flags & 3 == 0 {
                    self.copy(0x5dc, 0x5e4)?;
                }
                flags &= 0xffffffeb;
                self.put(0x5e8, flags)?;
            }
        } else {
            self.put(0x5e8, flags & 0xffffffee)?;
            self.put(0x5dc, 0)?;
            if power <= threshold {
                self.put(0x5e8, flags & 0xffffffea)?;
                if self.f(0x59c)? < 0.7 && self.byte(0xc2c)? != 0 {
                    self.pf(0x5b8, 1.0)?;
                    let f = self.w(0x1a4)?;
                    self.put(0x1a4, f | 0x1000000)?;
                }
                let f = self.w(0x5e8)?;
                self.put(0x5e8, f | 0x10)?;
            } else {
                if self.byte(0x4cc)? != 0
                    && flags & 2 == 0
                    && gear > 1
                    && wheel_rotation
                        < self.f(0x504u32.wrapping_add((gear as u32).wrapping_mul(4)))?
                {
                    let reciprocal = div(
                        1.0,
                        self.f(0x570u32.wrapping_add((gear as u32).wrapping_mul(4)))?,
                    )?;
                    let mut choice = gear;
                    let mut offset = 0x56cu32.wrapping_add((gear as u32).wrapping_mul(4));
                    loop {
                        let projected = mul(mul(reciprocal, self.f(offset)?)?, wheel_rotation)?;
                        let current = mul(
                            div(
                                self.f(0x548)?,
                                self.f(0x56cu32.wrapping_add(next.wrapping_mul(4)))?,
                            )?,
                            self.f(offset)?,
                        )?;
                        if self.f(offset.wrapping_sub(0x6c))? <= projected
                            && current < self.f(offset.wrapping_sub(0x9c))?
                        {
                            break;
                        }
                        choice -= 1;
                        offset = offset.wrapping_sub(4);
                        if choice <= 1 {
                            break;
                        }
                    }
                    self.gear(choice)?;
                }
                let f = self.w(0x5e8)?;
                self.put(0x5e8, f | 4)?;
                self.put(0x5e8, f | 0x14)?;
            }
        }
        let gear = self.w(0x55c)? as i32;
        if self.f(0x540)? <= self.f(0x24)?
            || (gear != -1 && gear != 1 && (gear < 2 || self.byte(0x4cc)? != 0))
        {
            if gear > 1 {
                let f = self.w(0x5e8)?;
                self.put(0x5e8, f & 0xfffffffb)?;
            }
            let f = self.w(0x5e8)?;
            self.pf(0x5cc, -1.0)?;
            self.put(0x5e8, f & 0xfffffff7)?;
            if f & 0x14 == 0 {
                self.put(0x5e8, (f & 0xfffffff7) | 1)?;
            }
            return Ok(());
        }
        flags = self.w(0x5e8)?;
        if flags & 4 != 0 {
            if self.f(0x5c8)? <= wheel_rotation {
                self.pf(0x5cc, -1.0)?;
                self.put(0x5e8, (flags & 0xfffffff3) | 1)?;
                return Ok(());
            }
            self.put(0x5e8, flags & 0xfffffffe)?;
            let total = add(second_force, first_force)?;
            if self.f(0x5c8)? <= self.f(0x548)? || self.f(0x5cc)? >= 0.0 {
                if self.f(0x548)? <= self.f(0x488)? {
                    if self.f(0x5cc)? >= 0.0 {
                        if flags & 8 == 0 {
                            if self.f(0x5a0)? <= self.f(0x59c)? {
                                if wheel_rotation <= self.f(0x5c4)? {
                                    self.copy(0x5dc, 0x5cc)?;
                                } else {
                                    let a = sub(1.0, self.f(0x5cc)?)?;
                                    let c = sub(
                                        1.0,
                                        div(
                                            sub(self.f(0x5c8)?, wheel_rotation)?,
                                            sub(self.f(0x5c8)?, self.f(0x5c4)?)?,
                                        )?,
                                    )?;
                                    let clutch = add(mul(a, c)?, self.f(0x5cc)?)?;
                                    self.pf(0x5dc, clutch)?;
                                }
                            } else {
                                self.pf(0x5cc, -1.0)?;
                                self.put(0x5e8, (flags & 0xfffffffe) | 8)?;
                            }
                        } else {
                            self.copy(0x5dc, 0x5cc)?;
                        }
                    } else {
                        let denominator = mul(
                            mul(
                                self.f(0x570u32.wrapping_add((gear as u32).wrapping_mul(4)))?,
                                self.f(0x5d0)?,
                            )?,
                            self.f(0x44)?,
                        )?;
                        let clutch = div(total, denominator)?;
                        self.pf(0x5dc, clutch)?;
                        if clutch > 1.0 {
                            self.pf(0x5dc, 1.0)?;
                        }
                        self.copy(0x5cc, 0x5dc)?;
                    }
                    if total < 0.0 {
                        self.pf(0x5cc, -1.0)?;
                        let f = self.w(0x5e8)?;
                        self.put(0x5e8, f & 0xfffffff3)?;
                        self.pf(0x5dc, 1.0)?;
                    }
                } else {
                    self.put(0x5e8, flags & 0xfffffff2)?;
                    self.pf(0x5dc, 1.0)?;
                }
            } else {
                let denominator = mul(
                    mul(
                        self.f(0x570u32.wrapping_add((gear as u32).wrapping_mul(4)))?,
                        self.f(0x5d0)?,
                    )?,
                    self.f(0x44)?,
                )?;
                self.pf(0x5dc, div(total, add(denominator, denominator)?)?)?;
            }
            if self.f(0x5e4)? < self.f(0x5dc)? {
                self.pf(0x5cc, -1.0)?;
                self.copy(0x5dc, 0x5e4)?;
                let f = self.w(0x5e8)?;
                self.put(0x5e8, f & 0xfffffff3)?;
            }
        } else if self.f(0x5c0)? <= self.f(0x548)? {
            if flags & 0x14 == 0 {
                self.put(0x5e8, flags | 1)?;
            }
        } else {
            self.put(0x5e8, flags | 4)?;
        }
        Ok(())
    }
}

pub fn vehicle_tick<B: VehicleTickBackend>(
    b: &mut B,
    vehicle: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    let mut s = Tick { b, v: vehicle };
    let milliseconds = if milliseconds == 0 { 1 } else { milliseconds };
    if s.byte(0x1fa8)? != 0 {
        let engine = s.byte(0xc2c)?;
        s.pb(0xc2c, 0)?;
        s.pb(0x1fa9, engine)?;
        s.put(0x3a4, 0)?;
        s.put(0x3a8, 0)?;
    }
    let flags = s.w(0x1a4)?;
    let active = (s.w(0xc0)? as i32) > 0;
    s.put(
        0x1a4,
        if active {
            flags | 0x10000000
        } else {
            flags & 0xefffffff
        },
    )?;
    if s.f(0xc64)? > f32::from_bits(0x3fc90fdb)
        && (s.w(0x55c)? as i32) > 0
        && s.f(0x3a4)? > 0.0
        && s.byte(0x5e8)? & 0x24 != 0
        && s.f(0x59c)? > 1.0
    {
        s.pf(0x5b8, 1.0)?;
        let flags = s.w(0x1a4)?;
        s.put(0x1a4, flags | 0x1000000)?;
    }
    let flags = s.w(0x1a4)? & 0xfffffffd;
    s.put(0x1a4, flags)?;
    if s.byte(0xc60)? != 0 {
        s.put(0x1a8, 0)?;
        s.put(0x1a4, flags | 1)?;
        s.put(4, 0)?;
    }
    let dt = milliseconds_to_seconds(milliseconds);
    if s.w(0xb8)? == 2 {
        s.effect(VehicleTickEffect::SteeringWheels { seconds: dt })?;
    }
    let engine = s.byte(0xc2c)?;
    if engine != 0 {
        if s.f(0xc30)? > f32::from_bits(0x38d1b717) {
            let power = if s.w(0xb8)? == 1 {
                mul(add(s.f(0x540)?.abs(), s.f(0x544)?.abs())?, 0.5)?
            } else {
                s.f(0x540)?.abs()
            };
            let consumed = mul(
                mul(
                    add(mul(power, 0.8)?, mul(div(s.f(0x548)?, s.f(0x1f9c)?)?, 0.2)?)?,
                    s.f(0x498)?,
                )?,
                dt,
            )?;
            let fuel = sub(s.f(0xc30)?, consumed)?;
            s.pf(0xc30, fuel)?;
            if fuel < f32::from_bits(0x38d1b717) {
                s.put(0xc30, 0)?;
                s.pb(0x5f0, 1)?;
                s.put(0x3a4, 0)?;
                s.put(0x3a8, 0)?;
            }
        } else {
            s.put(0xc30, 0)?;
            s.pb(0x5f0, 1)?;
            s.put(0x3a4, 0)?;
            s.put(0x3a8, 0)?;
        }
        if s.byte(0x432)? != 0 {
            s.effect(VehicleTickEffect::ProcedureEbe40 { seconds: dt })?;
        }
    } else if s.byte(0x1fa8)? != 0 {
        s.effect(VehicleTickEffect::ProcedureEeb50)?;
    } else if s.byte(0x638)? != 0 {
        s.effect(VehicleTickEffect::EngineSound { enabled: true })?;
    }
    if s.byte(0xc68)? & 0x20 != 0 {
        if s.f(0x434)? <= 0.0 {
            s.pb(0x154, 0)?;
        } else if s.byte(0x432)? != 0 {
            s.effect(VehicleTickEffect::ProcedureEd910)?;
        }
    }
    let throttle = s.f(0x3a4)?;
    s.copy(0x544, 0x3a8)?;
    s.pf(0x540, throttle)?;
    if s.byte(0x4ab)? == 0 {
        let current = s.f(0x5e4)?;
        let target = s.f(0x5e0)?;
        if current > target {
            let next = sub(current, mul(dt, s.f(0x5d8)?)?)?;
            s.pf(0x5e4, next)?;
            if next < target {
                s.copy(0x5e4, 0x5e0)?;
            }
        } else if current < target {
            let next = add(mul(dt, s.f(0x5d8)?)?, current)?;
            s.pf(0x5e4, next)?;
            if target < next {
                s.copy(0x5e4, 0x5e0)?;
            }
        }
    } else {
        s.copy(0x5e4, 0x5e0)?;
    }
    let next = s.automatic_gear(dt, throttle)?;
    s.copy(0x3bc, 0x540)?;
    let mode = s.w(0xb8)?;
    let flags = s.w(0x1a4)?;
    s.put(0x1a4, flags & 0xfeffffff)?;
    s.copy(0x3c0, 0x544)?;
    let (mut first, mut second) = match mode {
        0 | 2 => {
            let power = s.f(0x540)?;
            let moment = s.motor_force(dt, power)?;
            let mut force = mul(moment, 0.5)?;
            if s.byte(0x4cc)? != 0 {
                let flags = s.w(0x5e8)?;
                if flags & 3 != 0 && flags & 4 == 0 {
                    let doubled = add(force, force)?;
                    let gear = s.w(0x55c)?;
                    let max = mul(
                        mul(
                            mul(
                                s.f(0x570u32.wrapping_add(gear.wrapping_mul(4)))?,
                                s.f(0x5dc)?,
                            )?,
                            s.f(0x5d0)?,
                        )?,
                        s.f(0x44)?,
                    )?;
                    if max < doubled && flags & 1 == 0 || max < mul(doubled, 0.8)? {
                        force = mul(force, div(max, doubled)?)?;
                    }
                }
            }
            (force, force)
        }
        1 => {
            let p = s.f(0x540)?;
            let mut a = mul(s.motor_force(dt, p.abs())?, 0.5)?;
            if p < 0.0 {
                a = -a;
            }
            let p = s.f(0x544)?;
            let mut c = mul(s.motor_force(dt, p.abs())?, 0.5)?;
            if p < 0.0 {
                c = -c;
            }
            (a, c)
        }
        _ => return Err(VehicleTickUnavailable::UnrecoveredMode(mode)),
    };
    first = mul(first, s.f(0x44)?)?;
    second = mul(second, s.f(0x44)?)?;
    let flags = s.w(0x1a4)?;
    if flags & 1 == 0 || flags & 0x10000000 != 0 {
        let gear = s.w(0x55c)?;
        if (s.f(0x3a4)? <= s.f(0x24)? && s.f(0x3a8)? <= s.f(0x24)? || gear == 0)
            && gear != 0
            && s.byte(0x550)? == 0
        {
            s.copy(0x54c, 0x548)?;
            s.motor_rot(dt, add(second, first)?)?;
            if s.f(0x548)? > s.f(0x5ec)? {
                let flags = s.w(0x1a4)?;
                let engine = s.w(0x5e8)?;
                s.put(0x5e8, engine & 0xffffffef)?;
                s.put(0x1a4, flags | 1)?;
            } else {
                s.put(0x5dc, 0)?;
                let flags = s.w(0x5e8)?;
                s.put(0x5e8, (flags & 0xfffffffe) | 0x10)?;
                if s.byte(0xc2c)? != 0 && s.f(0x59c)? < 0.7 {
                    s.pf(0x5b8, 1.0)?;
                    let flags = s.w(0x1a4)?;
                    s.put(0x1a4, flags | 0x1000000)?;
                }
            }
            if s.byte(0x2c0)? != 0 || s.w(0x1a4)? & 1 != 0 {
                return Ok(());
            }
            let elapsed = s.w(0x1a8)?.wrapping_add(milliseconds);
            s.put(0x1a8, elapsed)?;
            if elapsed >= 1001 {
                let flags = s.w(0x1a4)?;
                s.put(0x1a4, (flags & 0xfffffffe) | 2)?;
            }
            return Ok(());
        }
        s.put(0x1a4, flags | 1)?;
    }
    s.clutch(next, first, second)?;
    s.copy(0x5a8, 0x5a4)?;
    s.copy(0x54c, 0x548)?;
    s.copy(0x5a0, 0x59c)?;
    if s.w(0x1a4)? & 0x10000000 == 0 {
        s.b.tick_engine(s.v, dt, first, second)
            .map_err(VehicleTickUnavailable::Effect)?;
    }
    if s.byte(0x5f0)? != 0 {
        s.effect(VehicleTickEffect::EngineSound { enabled: false })?;
    }
    let flags = s.w(0x1a4)?;
    let mut changed = None;
    let count = s.w(0x4c4)?;
    if flags & 8 == 0 {
        let mut remaining = count;
        let mut contact = false;
        while remaining != 0 {
            remaining -= 1;
            if s.wheel_flags(remaining)? & 8 != 0 {
                contact = true;
                break;
            }
        }
        if !contact {
            changed = Some(flags | 8);
        }
    } else {
        let mut remaining = count;
        let mut clear = true;
        while remaining != 0 {
            remaining -= 1;
            if s.wheel_flags(remaining)? & 8 == 0 {
                clear = false;
                break;
            }
        }
        if clear {
            let mut remaining = count;
            while remaining != 0 {
                remaining -= 1;
                if s.wheel_flags(remaining)? & 0x10 != 0 {
                    clear = false;
                    break;
                }
            }
            if clear {
                changed = Some(flags & 0xfffffff7);
            }
        }
    }
    if let Some(flags) = changed {
        s.put(0x1a4, flags)?;
    }
    let dx = sub(s.f(0x320)?, s.f(8)?)?;
    let dy = sub(s.f(0x324)?, s.f(0xc)?)?;
    let dz = sub(s.f(0x328)?, s.f(0x10)?)?;
    if add(add(mul(dy, dy)?, mul(dx, dx)?)?, mul(dz, dz)?)? >= f32::from_bits(0x3cb851ec) {
        s.copy(8, 0x320)?;
        s.copy(0xc, 0x324)?;
        s.put(4, 0)?;
        s.copy(0x10, 0x328)?;
    } else {
        let time = s.w(4)?.wrapping_add(milliseconds);
        s.put(4, time)?;
    }
    let elapsed = s.w(0x1a8)?.wrapping_add(milliseconds);
    s.put(0x1a8, elapsed)?;
    if s.w(0x1a4)? & 2 != 0 {
        if elapsed > 1000 || s.w(4)? > 4000 {
            let flags = s.w(0x1a4)?;
            s.put(0x1a4, flags & 0xfffffffe)?;
        }
    } else {
        s.put(0x1a8, 0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::milliseconds_to_seconds;
    #[test]
    fn unsigned_elapsed_product_matches_original_single_rounding() {
        for (ms, bits) in [
            (1, 0x3a83126f),
            (16_777_217, 0x46831270),
            (2_147_483_648, 1_241_715_311),
            (u32::MAX, 1_250_103_919),
        ] {
            assert_eq!(milliseconds_to_seconds(ms).to_bits(), bits);
        }
        assert_ne!(
            milliseconds_to_seconds(16_777_217),
            (16_777_217u32 as f32) * 0.001
        );
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            milliseconds_to_seconds(1);
        });
        assert!(!precision.precision_raised());
        precision.run(|| {
            milliseconds_to_seconds(3);
        });
        assert!(precision.precision_raised());
    }
}
