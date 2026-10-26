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

enum TickContinuation {
    Continue,
    Complete,
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

// Descriptive phases of this Rust implementation, not recovered native symbols.
impl<B: VehicleTickBackend> Tick<'_, B> {
    fn prepare_motion_state(&mut self) -> Out<(), B::Error> {
        if self.byte(0x1fa8)? != 0 {
            let engine = self.byte(0xc2c)?;
            self.pb(0xc2c, 0)?;
            self.pb(0x1fa9, engine)?;
            self.put(0x3a4, 0)?;
            self.put(0x3a8, 0)?;
        }
        let flags = self.w(0x1a4)?;
        let active = (self.w(0xc0)? as i32) > 0;
        self.put(
            0x1a4,
            if active {
                flags | 0x10000000
            } else {
                flags & 0xefffffff
            },
        )?;
        if self.f(0xc64)? > f32::from_bits(0x3fc90fdb)
            && (self.w(0x55c)? as i32) > 0
            && self.f(0x3a4)? > 0.0
            && self.byte(0x5e8)? & 0x24 != 0
            && self.f(0x59c)? > 1.0
        {
            self.pf(0x5b8, 1.0)?;
            let flags = self.w(0x1a4)?;
            self.put(0x1a4, flags | 0x1000000)?;
        }
        let flags = self.w(0x1a4)? & 0xfffffffd;
        self.put(0x1a4, flags)?;
        if self.byte(0xc60)? != 0 {
            self.put(0x1a8, 0)?;
            self.put(0x1a4, flags | 1)?;
            self.put(4, 0)?;
        }
        Ok(())
    }

    fn update_fuel_and_engine_effects(&mut self, dt: f32) -> Out<(), B::Error> {
        let engine = self.byte(0xc2c)?;
        if engine != 0 {
            if self.f(0xc30)? > f32::from_bits(0x38d1b717) {
                let power = if self.w(0xb8)? == 1 {
                    mul(add(self.f(0x540)?.abs(), self.f(0x544)?.abs())?, 0.5)?
                } else {
                    self.f(0x540)?.abs()
                };
                let consumed = mul(
                    mul(
                        add(
                            mul(power, 0.8)?,
                            mul(div(self.f(0x548)?, self.f(0x1f9c)?)?, 0.2)?,
                        )?,
                        self.f(0x498)?,
                    )?,
                    dt,
                )?;
                let fuel = sub(self.f(0xc30)?, consumed)?;
                self.pf(0xc30, fuel)?;
                if fuel < f32::from_bits(0x38d1b717) {
                    self.put(0xc30, 0)?;
                    self.pb(0x5f0, 1)?;
                    self.put(0x3a4, 0)?;
                    self.put(0x3a8, 0)?;
                }
            } else {
                self.put(0xc30, 0)?;
                self.pb(0x5f0, 1)?;
                self.put(0x3a4, 0)?;
                self.put(0x3a8, 0)?;
            }
            if self.byte(0x432)? != 0 {
                self.effect(VehicleTickEffect::ProcedureEbe40 { seconds: dt })?;
            }
        } else if self.byte(0x1fa8)? != 0 {
            self.effect(VehicleTickEffect::ProcedureEeb50)?;
        } else if self.byte(0x638)? != 0 {
            self.effect(VehicleTickEffect::EngineSound { enabled: true })?;
        }
        if self.byte(0xc68)? & 0x20 != 0 {
            if self.f(0x434)? <= 0.0 {
                self.pb(0x154, 0)?;
            } else if self.byte(0x432)? != 0 {
                self.effect(VehicleTickEffect::ProcedureEd910)?;
            }
        }
        Ok(())
    }

    fn prepare_drivetrain_controls(&mut self, dt: f32) -> Out<f32, B::Error> {
        let throttle = self.f(0x3a4)?;
        self.copy(0x544, 0x3a8)?;
        self.pf(0x540, throttle)?;
        if self.byte(0x4ab)? == 0 {
            let current = self.f(0x5e4)?;
            let target = self.f(0x5e0)?;
            if current > target {
                let next = sub(current, mul(dt, self.f(0x5d8)?)?)?;
                self.pf(0x5e4, next)?;
                if next < target {
                    self.copy(0x5e4, 0x5e0)?;
                }
            } else if current < target {
                let next = add(mul(dt, self.f(0x5d8)?)?, current)?;
                self.pf(0x5e4, next)?;
                if target < next {
                    self.copy(0x5e4, 0x5e0)?;
                }
            }
        } else {
            self.copy(0x5e4, 0x5e0)?;
        }
        Ok(throttle)
    }

    fn compute_drive_forces(&mut self, dt: f32) -> Out<(f32, f32), B::Error> {
        self.copy(0x3bc, 0x540)?;
        let mode = self.w(0xb8)?;
        let flags = self.w(0x1a4)?;
        self.put(0x1a4, flags & 0xfeffffff)?;
        self.copy(0x3c0, 0x544)?;
        let (mut first, mut second) = match mode {
            0 | 2 => {
                let power = self.f(0x540)?;
                let moment = self.motor_force(dt, power)?;
                let mut force = mul(moment, 0.5)?;
                if self.byte(0x4cc)? != 0 {
                    let flags = self.w(0x5e8)?;
                    if flags & 3 != 0 && flags & 4 == 0 {
                        let doubled = add(force, force)?;
                        let gear = self.w(0x55c)?;
                        let max = mul(
                            mul(
                                mul(
                                    self.f(0x570u32.wrapping_add(gear.wrapping_mul(4)))?,
                                    self.f(0x5dc)?,
                                )?,
                                self.f(0x5d0)?,
                            )?,
                            self.f(0x44)?,
                        )?;
                        if max < doubled && flags & 1 == 0 || max < mul(doubled, 0.8)? {
                            force = mul(force, div(max, doubled)?)?;
                        }
                    }
                }
                (force, force)
            }
            1 => {
                let p = self.f(0x540)?;
                let mut a = mul(self.motor_force(dt, p.abs())?, 0.5)?;
                if p < 0.0 {
                    a = -a;
                }
                let p = self.f(0x544)?;
                let mut c = mul(self.motor_force(dt, p.abs())?, 0.5)?;
                if p < 0.0 {
                    c = -c;
                }
                (a, c)
            }
            _ => return Err(VehicleTickUnavailable::UnrecoveredMode(mode)),
        };
        first = mul(first, self.f(0x44)?)?;
        second = mul(second, self.f(0x44)?)?;
        Ok((first, second))
    }

    fn update_motion_activity(
        &mut self,
        milliseconds: u32,
        dt: f32,
        first: f32,
        second: f32,
    ) -> Out<TickContinuation, B::Error> {
        let flags = self.w(0x1a4)?;
        if flags & 1 == 0 || flags & 0x10000000 != 0 {
            let gear = self.w(0x55c)?;
            if (self.f(0x3a4)? <= self.f(0x24)? && self.f(0x3a8)? <= self.f(0x24)? || gear == 0)
                && gear != 0
                && self.byte(0x550)? == 0
            {
                self.copy(0x54c, 0x548)?;
                self.motor_rot(dt, add(second, first)?)?;
                if self.f(0x548)? > self.f(0x5ec)? {
                    let flags = self.w(0x1a4)?;
                    let engine = self.w(0x5e8)?;
                    self.put(0x5e8, engine & 0xffffffef)?;
                    self.put(0x1a4, flags | 1)?;
                } else {
                    self.put(0x5dc, 0)?;
                    let flags = self.w(0x5e8)?;
                    self.put(0x5e8, (flags & 0xfffffffe) | 0x10)?;
                    if self.byte(0xc2c)? != 0 && self.f(0x59c)? < 0.7 {
                        self.pf(0x5b8, 1.0)?;
                        let flags = self.w(0x1a4)?;
                        self.put(0x1a4, flags | 0x1000000)?;
                    }
                }
                if self.byte(0x2c0)? != 0 || self.w(0x1a4)? & 1 != 0 {
                    return Ok(TickContinuation::Complete);
                }
                let elapsed = self.w(0x1a8)?.wrapping_add(milliseconds);
                self.put(0x1a8, elapsed)?;
                if elapsed >= 1001 {
                    let flags = self.w(0x1a4)?;
                    self.put(0x1a4, (flags & 0xfffffffe) | 2)?;
                }
                return Ok(TickContinuation::Complete);
            }
            self.put(0x1a4, flags | 1)?;
        }
        Ok(TickContinuation::Continue)
    }

    fn advance_vehicle_engine(&mut self, dt: f32, first: f32, second: f32) -> Out<(), B::Error> {
        self.copy(0x5a8, 0x5a4)?;
        self.copy(0x54c, 0x548)?;
        self.copy(0x5a0, 0x59c)?;
        if self.w(0x1a4)? & 0x10000000 == 0 {
            self.b
                .tick_engine(self.v, dt, first, second)
                .map_err(VehicleTickUnavailable::Effect)?;
        }
        if self.byte(0x5f0)? != 0 {
            self.effect(VehicleTickEffect::EngineSound { enabled: false })?;
        }
        Ok(())
    }

    fn update_wheel_contact_flags(&mut self) -> Out<(), B::Error> {
        let flags = self.w(0x1a4)?;
        let mut changed = None;
        let count = self.w(0x4c4)?;
        if flags & 8 == 0 {
            let mut remaining = count;
            let mut contact = false;
            while remaining != 0 {
                remaining -= 1;
                if self.wheel_flags(remaining)? & 8 != 0 {
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
                if self.wheel_flags(remaining)? & 8 == 0 {
                    clear = false;
                    break;
                }
            }
            if clear {
                let mut remaining = count;
                while remaining != 0 {
                    remaining -= 1;
                    if self.wheel_flags(remaining)? & 0x10 != 0 {
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
            self.put(0x1a4, flags)?;
        }
        Ok(())
    }

    fn update_motion_timers(&mut self, milliseconds: u32) -> Out<(), B::Error> {
        let dx = sub(self.f(0x320)?, self.f(8)?)?;
        let dy = sub(self.f(0x324)?, self.f(0xc)?)?;
        let dz = sub(self.f(0x328)?, self.f(0x10)?)?;
        if add(add(mul(dy, dy)?, mul(dx, dx)?)?, mul(dz, dz)?)? >= f32::from_bits(0x3cb851ec) {
            self.copy(8, 0x320)?;
            self.copy(0xc, 0x324)?;
            self.put(4, 0)?;
            self.copy(0x10, 0x328)?;
        } else {
            let time = self.w(4)?.wrapping_add(milliseconds);
            self.put(4, time)?;
        }
        let elapsed = self.w(0x1a8)?.wrapping_add(milliseconds);
        self.put(0x1a8, elapsed)?;
        if self.w(0x1a4)? & 2 != 0 {
            if elapsed > 1000 || self.w(4)? > 4000 {
                let flags = self.w(0x1a4)?;
                self.put(0x1a4, flags & 0xfffffffe)?;
            }
        } else {
            self.put(0x1a8, 0)?;
        }
        Ok(())
    }
}

pub fn vehicle_tick<B: VehicleTickBackend>(
    b: &mut B,
    vehicle: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    let mut tick = Tick { b, v: vehicle };
    let milliseconds = if milliseconds == 0 { 1 } else { milliseconds };
    tick.prepare_motion_state()?;
    // Conversion can raise the precision flag only after the initial stores.
    let dt = milliseconds_to_seconds(milliseconds);
    if tick.w(0xb8)? == 2 {
        tick.effect(VehicleTickEffect::SteeringWheels { seconds: dt })?;
    }
    tick.update_fuel_and_engine_effects(dt)?;
    let throttle = tick.prepare_drivetrain_controls(dt)?;
    let next = tick.automatic_gear(dt, throttle)?;
    let (first, second) = tick.compute_drive_forces(dt)?;
    if let TickContinuation::Complete =
        tick.update_motion_activity(milliseconds, dt, first, second)?
    {
        return Ok(());
    }
    tick.clutch(next, first, second)?;
    tick.advance_vehicle_engine(dt, first, second)?;
    tick.update_wheel_contact_flags()?;
    tick.update_motion_timers(milliseconds)
}

#[cfg(test)]
#[path = "vehicle_dynamics_tick_tests.rs"]
mod tick_tests;

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
