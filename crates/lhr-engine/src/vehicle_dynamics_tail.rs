//! Final internal phase of the original six-float C_Vehicle::Move.
//!
//! Starts at PC RVA e6103 after the force/body solver. This phase does not
//! advance position, replace that solver, or constitute a complete Move entry.
//! Persistent storage is the same addressable vehicle, frames and wheel owner.
use crate::{native_transform::SMatrix, vehicle_init::VehicleInitBackend};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleMoveTailUnavailable<E> {
    Effect(E),
    NumericDomain,
    Matrix,
}
type Out<T, E> = Result<T, VehicleMoveTailUnavailable<E>>;

pub trait VehicleMoveTailBackend: VehicleInitBackend {
    fn move_update_world_matrix(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn move_angle_to(&mut self, first: [f32; 3], second: [f32; 3]) -> Result<f32, Self::Error>;
    fn move_set_steer(&mut self, vehicle: u32, value: f32) -> Result<(), Self::Error>;
    fn move_update_steering_wheels(&mut self, vehicle: u32, dt: f32) -> Result<(), Self::Error>;
    fn move_wheels_collision(
        &mut self,
        vehicle: u32,
        dt: f32,
        down: [f32; 3],
        impact: bool,
    ) -> Result<[f32; 3], Self::Error>;
    fn move_motor_rot(&mut self, vehicle: u32, dt: f32, force: f32) -> Result<(), Self::Error>;
}

fn number<E>(x: f32) -> Out<f32, E> {
    if x.is_normal() || x == 0. {
        Ok(x)
    } else {
        Err(VehicleMoveTailUnavailable::NumericDomain)
    }
}
fn arithmetic<E>(x: f32, wide: f64) -> Out<f32, E> {
    if !x.is_finite()
        || (wide != 0. && wide.abs() < f32::MIN_POSITIVE as f64)
        || wide.abs() > f32::MAX as f64
    {
        Err(VehicleMoveTailUnavailable::NumericDomain)
    } else {
        Ok(x)
    }
}
fn add<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    let value = arithmetic(a + b, a as f64 + b as f64)?;
    crate::native_precision::record_add(a, b, value);
    Ok(value)
}
fn sub<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    let value = arithmetic(a - b, a as f64 - b as f64)?;
    crate::native_precision::record_sub(a, b, value);
    Ok(value)
}
fn mul<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    let value = arithmetic(a * b, a as f64 * b as f64)?;
    crate::native_precision::record_mul(a, b, value);
    Ok(value)
}
fn div<E>(a: f32, b: f32) -> Out<f32, E> {
    let (a, b) = (number(a)?, number(b)?);
    let value = arithmetic(a / b, a as f64 / b as f64)?;
    crate::native_precision::record_div(a, b, value);
    Ok(value)
}
fn square<E>(v: [f32; 3]) -> Out<f32, E> {
    add(add(mul(v[0], v[0])?, mul(v[1], v[1])?)?, mul(v[2], v[2])?)
}
fn near_unit<E>(s: f32) -> Out<bool, E> {
    Ok((sub(s, 1.)? as f64).abs() < 1e-8_f32 as f64)
}
fn root<E>(s: f32) -> Out<f32, E> {
    let value = arithmetic(s.sqrt(), (s as f64).sqrt())?;
    crate::native_precision::record_sqrt(s, value);
    Ok(value)
}
fn normalize<E>(v: [f32; 3]) -> Out<[f32; 3], E> {
    let s = square(v)?;
    if near_unit(s)? {
        return Ok(v);
    }
    if s >= 1e-8_f32 {
        let r = div(1., root(s)?)?;
        return Ok([mul(r, v[0])?, mul(r, v[1])?, mul(r, v[2])?]);
    }
    let axis = if v[0] != 0. {
        0
    } else if v[2] != 0. {
        2
    } else {
        1
    };
    let mut out = v;
    out[axis] = if v[axis] < 0. { -1. } else { 1. };
    Ok(out)
}
fn length<E>(v: [f32; 3]) -> Out<f32, E> {
    let s = square(v)?;
    if s < 1e-8_f32 {
        Ok(0.)
    } else if near_unit(s)? {
        Ok(1.)
    } else {
        root(s)
    }
}
struct Tail<'a, B> {
    b: &'a mut B,
    v: u32,
}
impl<B: VehicleMoveTailBackend> Tail<'_, B> {
    fn word(&mut self, a: u32) -> Out<u32, B::Error> {
        self.b
            .read_word(a)
            .map_err(VehicleMoveTailUnavailable::Effect)
    }
    fn float(&mut self, a: u32) -> Out<f32, B::Error> {
        number(f32::from_bits(self.word(a)?))
    }
    fn put(&mut self, a: u32, x: u32) -> Out<(), B::Error> {
        self.b
            .write_word(a, x)
            .map_err(VehicleMoveTailUnavailable::Effect)
    }
    fn pf(&mut self, a: u32, x: f32) -> Out<(), B::Error> {
        self.put(a, x.to_bits())
    }
    fn vector(&mut self, a: u32) -> Out<[f32; 3], B::Error> {
        Ok([self.float(a)?, self.float(a + 4)?, self.float(a + 8)?])
    }
    fn pv(&mut self, a: u32, v: [f32; 3]) -> Out<(), B::Error> {
        for (i, x) in v.into_iter().enumerate() {
            self.pf(a + i as u32 * 4, x)?;
        }
        Ok(())
    }
    fn copy(&mut self, d: u32, s: u32, n: u32) -> Out<(), B::Error> {
        for i in 0..n {
            let x = self.word(s + i * 4)?;
            self.put(d + i * 4, x)?;
        }
        Ok(())
    }
    fn matrix(&mut self, a: u32) -> Out<SMatrix, B::Error> {
        let mut m = [0.; 16];
        for (i, x) in m.iter_mut().enumerate() {
            *x = f32::from_bits(self.word(a + i as u32 * 4)?);
        }
        Ok(SMatrix { elements: m })
    }
    fn run(&mut self, args: [f32; 6]) -> Out<bool, B::Error> {
        let v = self.v;
        let model = self.word(v + 0x34c)?;
        if self
            .b
            .read_byte(model + 0xac)
            .map_err(VehicleMoveTailUnavailable::Effect)?
            & 0x20
            == 0
        {
            self.b
                .move_update_world_matrix(model)
                .map_err(VehicleMoveTailUnavailable::Effect)?;
        }
        self.copy(v + 0xce8, model + 0x10, 16)?;
        self.copy(v + 0xd28, model + 0x10, 16)?;
        let (inverse, _) = self
            .matrix(v + 0xd28)?
            .inverse()
            .map_err(|_| VehicleMoveTailUnavailable::Matrix)?;
        for (i, x) in inverse.elements.into_iter().enumerate() {
            self.pf(v + 0xd28 + i as u32 * 4, x)?;
        }
        self.copy(v + 0xd68, v + 0xce8, 16)?;
        self.copy(v + 0xda8, v + 0xd28, 16)?;
        for o in [0xd98, 0xd9c, 0xda0, 0xdd8, 0xddc, 0xde0] {
            self.put(v + o, 0)?;
        }
        let up = self.vector(v + 0xc54)?;
        let down = up.map(|x| -x);
        let point = self.vector(v + 0x3f8)?;
        let center = self
            .matrix(v + 0xce8)?
            .try_transform_point(point)
            .map_err(|_| VehicleMoveTailUnavailable::Matrix)?;
        self.pv(v + 0x398, center)?;
        if self
            .b
            .read_byte(v + 0x3c)
            .map_err(VehicleMoveTailUnavailable::Effect)?
            != 0
        {
            let target = self.vector(v + 0x30)?;
            let pos = self.vector(v + 0x320)?;
            let delta = [
                sub(target[0], pos[0])?,
                sub(target[1], pos[1])?,
                sub(target[2], pos[2])?,
            ];
            let dir = self.vector(v + 0xc3c)?;
            let angle = self
                .b
                .move_angle_to(delta, dir)
                .map_err(VehicleMoveTailUnavailable::Effect)?;
            let side = self.vector(v + 0xc48)?;
            let dot = add(
                add(mul(delta[2], side[2])?, mul(delta[1], side[1])?)?,
                mul(delta[0], side[0])?,
            )?;
            let steer = div(angle, self.float(v + 0x610)?)?;
            let steer = if dot > 0. {
                steer.min(1.)
            } else {
                (-steer).max(-1.)
            };
            self.b
                .move_set_steer(v, steer)
                .map_err(VehicleMoveTailUnavailable::Effect)?;
            let dt = mul(args[0], self.float(0x623614)?)?;
            self.b
                .move_update_steering_wheels(v, dt)
                .map_err(VehicleMoveTailUnavailable::Effect)?;
        }
        self.put(v + 0x5a4, 0)?;
        let velocity = self.vector(v + 0x1f90)?;
        let speed_square = square(velocity)?;
        let mut sleeping = speed_square < 1e-8_f32
            || (!near_unit(speed_square)? && root(speed_square)? <= self.float(0x62348c)?);
        if sleeping {
            let angular = self.vector(v + 0x420)?;
            let angular_square = square(angular)?;
            sleeping = angular_square < 1e-8_f32
                || (!near_unit(angular_square)?
                    && root(angular_square)? <= self.float(0x624e48)?);
        }
        if sleeping {
            sleeping = self
                .b
                .read_byte(v + 0x394)
                .map_err(VehicleMoveTailUnavailable::Effect)?
                != 0
                && self
                    .b
                    .read_byte(v + 0xc60)
                    .map_err(VehicleMoveTailUnavailable::Effect)?
                    == 0;
        }
        self.copy(v + 0x358, v + 0x1f90, 3)?;
        let normalized = normalize(self.vector(v + 0x358)?)?;
        self.pv(v + 0x358, normalized)?;
        // Reload after the normalization stores, as the native pass does.
        let speed = length(self.vector(v + 0x1f90)?)?;
        self.pf(v + 0x59c, speed)?;
        let contact = self
            .b
            .move_wheels_collision(v, args[0], down, true)
            .map_err(VehicleMoveTailUnavailable::Effect)?;
        let contact_square = if sleeping {
            add(
                add(mul(contact[1], contact[1])?, mul(contact[2], contact[2])?)?,
                mul(contact[0], contact[0])?,
            )?
        } else {
            square(contact)?
        };
        let projected = if contact_square > self.float(0x6232fc)? {
            let normal = normalize(contact)?;
            let velocity = self.vector(v + 0x358)?;
            let dot = add(
                add(mul(normal[1], velocity[1])?, mul(normal[2], velocity[2])?)?,
                mul(normal[0], velocity[0])?,
            )?;
            mul(dot.abs(), self.float(v + 0x59c)?)?
        } else {
            self.float(v + 0x59c)?
        };
        self.pf(v + 0x5a4, projected)?;
        if sleeping {
            if self.float(v + 0x3a4)? <= self.float(0x6233e4)? {
                for o in [
                    0x1f90, 0x1f94, 0x1f98, 0x358, 0x35c, 0x360, 0x59c, 0x420, 0x424, 0x428,
                ] {
                    self.put(v + o, 0)?;
                }
            }
            let flags = self.word(v + 0x1a4)? | 2;
            self.put(v + 0x1a4, flags)?;
        }
        match self.word(v + 0xb8)? {
            0 | 2 => {
                let force = add(args[4], args[5])?;
                self.b
                    .move_motor_rot(v, args[0], force)
                    .map_err(VehicleMoveTailUnavailable::Effect)?;
            }
            1 => {
                let force = if self.float(v + 0x540)? > 0. {
                    args[4]
                } else {
                    -args[4]
                };
                let force = add(force, force)?;
                self.b
                    .move_motor_rot(v, args[0], force)
                    .map_err(VehicleMoveTailUnavailable::Effect)?;
            }
            _ => {}
        }
        let dir = self.vector(v + 0xc3c)?;
        let velocity = self.vector(v + 0x1f90)?;
        let angle = self
            .b
            .move_angle_to(dir, velocity)
            .map_err(VehicleMoveTailUnavailable::Effect)?;
        self.pf(v + 0xc64, angle)?;
        Ok(true)
    }
}

/// Executes only the post-body-solver phase. `arguments` retain the original
/// Move argument order; callers must establish its earlier persistent state.
/// Local dead stack timing and original callee stack storage are not projected.
pub fn vehicle_move_tail<B: VehicleMoveTailBackend>(
    backend: &mut B,
    vehicle: u32,
    arguments: [f32; 6],
) -> Out<bool, B::Error> {
    Tail {
        b: backend,
        v: vehicle,
    }
    .run(arguments)
}
