//! Per-wheel force and torque phase of PC C_Vehicle::Move, RVA e2fb0..e41dc.
//! This internal phase follows the actual wheel preparation and force setup;
//! it does not initialize parameters or replace rigid-body/contact integration.
//! Every reached store survives an unavailable library/effect dependency.
use crate::{
    native_transform::{
        try_acos_sin_cos_products, try_vector_angle_sine, try_wheel_friction_limit, MakeUnavailable,
    },
    vehicle_dynamics_forces::VehicleMoveForceSetup,
    vehicle_dynamics_prepare::{
        add, div, float, mul, number, result, store, store_float, sub, word,
        VehicleMovePrepareUnavailable, VehicleMovePrepared,
    },
    vehicle_init::VehicleInitBackend,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleMoveWheelForcesUnavailable<E> {
    Prepare(VehicleMovePrepareUnavailable<E>),
    Library(MakeUnavailable),
}
impl<E> From<VehicleMovePrepareUnavailable<E>> for VehicleMoveWheelForcesUnavailable<E> {
    fn from(x: VehicleMovePrepareUnavailable<E>) -> Self {
        Self::Prepare(x)
    }
}
type Out<T, E> = Result<T, VehicleMoveWheelForcesUnavailable<E>>;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleMoveWheelForces {
    pub linear_force: [f32; 3],
    pub torque: [f32; 3],
    pub contact_normal_sum: [f32; 3],
    pub any_contact: bool,
    pub drive_force_sum: f32,
    pub saved_traction_control: u8,
    pub saved_brake_bits: u32,
}
fn vector<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([
        float(b, a)?,
        float(b, a.wrapping_add(4))?,
        float(b, a.wrapping_add(8))?,
    ])
}
fn write_vector<B: VehicleInitBackend>(b: &mut B, a: u32, v: [f32; 3]) -> Out<(), B::Error> {
    for (i, x) in v.into_iter().enumerate() {
        store_float(b, a.wrapping_add(i as u32 * 4), x)?;
    }
    Ok(())
}
// Original inline squared lengths keep every product on the x87 stack until
// the sum's FST. A tiny product can be below binary32's exponent range while
// its final sum remains ordinary. All terms here are nonnegative, so an f64
// sum of two PC24 values is exact whenever the smaller term can affect PC24
// rounding (exponent distance <=24); otherwise it cannot cross a midpoint.
fn square_wide_ordered<E>(v: [f32; 3], order: [usize; 3]) -> Out<f64, E> {
    for x in v {
        number(x)?;
    }
    fn pc24(value: f64) -> f64 {
        let bits = value.to_bits();
        let discarded = bits & ((1_u64 << 29) - 1);
        let mut retained = bits >> 29;
        let halfway = 1_u64 << 28;
        if discarded > halfway || discarded == halfway && retained & 1 != 0 {
            retained += 1;
        }
        f64::from_bits(retained << 29)
    }
    let product = |x: f32| {
        let x = f64::from(x);
        let value = pc24(x * x);
        crate::native_precision::record_wide_pc24_mul(x, x, value);
        value
    };
    let first = product(v[order[0]]);
    let second = product(v[order[1]]);
    let partial = pc24(first + second);
    crate::native_precision::record_wide_pc24_add(first, second, partial);
    let third = product(v[order[2]]);
    let wide = pc24(partial + third);
    crate::native_precision::record_wide_pc24_add(partial, third, wide);
    Ok(wide)
}
fn square<E>(v: [f32; 3]) -> Out<f64, E> {
    // These callers compare the retained sum directly (for example
    // PC0x4e314c); introducing an FST here would change sticky precision.
    square_wide_ordered(v, [0, 1, 2])
}
fn square_yzx<E>(v: [f32; 3]) -> Out<f32, E> {
    let wide = square_wide_ordered(v, [1, 2, 0])?;
    // These original inline length/normalization copies do FST before their
    // raw binary32 cutoff. Preserve actual subnormal/zero rounding here.
    let value = wide as f32;
    if !value.is_finite() {
        return Err(VehicleMovePrepareUnavailable::NumericDomain.into());
    }
    crate::native_precision::record_precision(f64::from(value) != wide);
    Ok(value)
}
fn scale<E>(v: [f32; 3], s: f32) -> Out<[f32; 3], E> {
    Ok([mul(v[0], s)?, mul(v[1], s)?, mul(v[2], s)?])
}
fn plus<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([add(a[0], b[0])?, add(a[1], b[1])?, add(a[2], b[2])?])
}
fn minus<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([sub(a[0], b[0])?, sub(a[1], b[1])?, sub(a[2], b[2])?])
}
fn dot<E>(a: [f32; 3], b: [f32; 3]) -> Out<f32, E> {
    Ok(add(
        add(mul(a[2], b[2])?, mul(a[1], b[1])?)?,
        mul(a[0], b[0])?,
    )?)
}
fn reverse_cross<E>(a: [f32; 3], b: [f32; 3]) -> Out<[f32; 3], E> {
    Ok([
        sub(mul(a[2], b[1])?, mul(a[1], b[2])?)?,
        sub(mul(a[0], b[2])?, mul(a[2], b[0])?)?,
        sub(mul(a[1], b[0])?, mul(a[0], b[1])?)?,
    ])
}
fn root<E>(s: f32) -> Out<f32, E> {
    number(s)?;
    let value = result(s.sqrt(), (s as f64).sqrt())?;
    crate::native_precision::record_sqrt(s, value);
    Ok(value)
}
fn near_unit<E>(s: f32) -> Out<bool, E> {
    if s.is_subnormal() {
        // A squared-length FST can be subnormal. Original FSUB one rounds
        // exactly to -1 under PC24, raising precision before the small-vector
        // fallback; it does not perform binary32 subnormal arithmetic.
        crate::native_precision::record_wide_pc24_sub(f64::from(s), 1., -1.);
        return Ok(false);
    }
    Ok((sub(s, 1.)? as f64).abs() < 1e-8)
}
fn length_yzx<E>(v: [f32; 3]) -> Out<f32, E> {
    let s = square_yzx(v)?;
    if (s.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if near_unit(s)? {
        Ok(1.)
    } else {
        root(s)
    }
}
fn normalize<E>(mut v: [f32; 3], s: f32) -> Out<[f32; 3], E> {
    if near_unit(s)? {
        return Ok(v);
    }
    if s >= 1e-8_f32 {
        return scale(v, div(1., root(s)?)?);
    }
    let axis = if v[0] != 0. {
        0
    } else if v[2] != 0. {
        2
    } else {
        1
    };
    v[axis] = if v[axis] < 0. { -1. } else { 1. };
    Ok(v)
}
fn rb<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a)
        .map_err(|e| VehicleMovePrepareUnavailable::Effect(e).into())
}
fn wb<B: VehicleInitBackend>(b: &mut B, a: u32, x: u8) -> Out<(), B::Error> {
    b.write_byte(a, x)
        .map_err(|e| VehicleMovePrepareUnavailable::Effect(e).into())
}
fn wheel<B: VehicleInitBackend>(b: &mut B, v: u32, i: u32) -> Out<u32, B::Error> {
    let list = word(b, v.wrapping_add(0xc38))?;
    Ok(word(b, list.wrapping_add(i.wrapping_mul(4)))?)
}
fn material_record<B: VehicleInitBackend>(b: &mut B, w: u32) -> Out<u32, B::Error> {
    let kind = word(b, w.wrapping_add(0xac))?;
    let root = word(b, 0x63d9c8)?;
    let head = word(
        b,
        root.wrapping_add(kind.wrapping_mul(0x28))
            .wrapping_add(0x24),
    )?;
    let begin = word(b, head.wrapping_add(4))?;
    let index = word(b, w.wrapping_add(0xb0))?;
    Ok(begin.wrapping_add(index.wrapping_mul(0x5c)))
}
fn slip_force<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    w: u32,
    velocity: [f32; 3],
    direction: [f32; 3],
) -> Out<([f32; 3], f32), B::Error> {
    let projected = scale(direction, dot(direction, velocity)?)?;
    let mut force = minus(projected, velocity)?;
    if !(number(float(b, v.wrapping_add(0x434))?)? >= 0.001_f32
        && word(b, w.wrapping_add(0x120))? & 4 != 0)
    {
        let sine = try_vector_angle_sine(direction, velocity)
            .map_err(VehicleMoveWheelForcesUnavailable::Library)?;
        store_float(b, w.wrapping_add(0x108), sine)?;
        let squared = square_yzx(force)?;
        if (squared.to_bits() as i32) >= 0x322bcc77 && !near_unit(squared)? && root(squared)? > 1. {
            if sine > number(float(b, w.wrapping_add(0x170))?)?
                && square(velocity)? > f64::from(0.0001_f32)
            {
                let flags = word(b, w.wrapping_add(0x120))?;
                store(b, w.wrapping_add(0x120), flags | 0x10)?;
            }
            if sine > 0.6_f32 && square(velocity)? > 225. {
                let flags = word(b, w.wrapping_add(0x120))?;
                store(b, w.wrapping_add(0x120), flags | 0x100)?;
            }
            force = normalize(force, squared)?;
        }
        let flags = word(b, w.wrapping_add(0x120))?;
        let threshold = if word(b, w.wrapping_add(0xac))? == 0 {
            float(
                b,
                w.wrapping_add(if (flags as i32) < 0 { 0xe8 } else { 0xbc }),
            )?
        } else {
            let record = material_record(b, w)?;
            float(
                b,
                record.wrapping_add(if (flags as i32) < 0 { 0x34 } else { 8 }),
            )?
        };
        if sine < number(threshold)? {
            force = scale(force, div(sine, threshold)?)?;
        }
    }
    force = scale(force, float(b, w.wrapping_add(0xa4))?)?;
    let lateral_length = length_yzx(force)?;
    let flags = word(b, w.wrapping_add(0x120))?;
    let resistance = if word(b, w.wrapping_add(0xac))? == 0 {
        float(
            b,
            w.wrapping_add(if (flags as i32) < 0 { 0x104 } else { 0xe0 }),
        )?
    } else {
        let record = material_record(b, w)?;
        float(
            b,
            record.wrapping_add(if (flags as i32) < 0 { 0x58 } else { 0x2c }),
        )?
    };
    let mut drag = mul(
        div(resistance, float(b, w.wrapping_add(0x10c))?)?,
        float(b, w.wrapping_add(0x90))?,
    )?;
    if flags & 0x10000 != 0 {
        drag = -drag;
    }
    force = plus(force, scale(direction, drag)?)?;
    Ok((force, lateral_length))
}
#[allow(clippy::too_many_arguments)] // Explicit original force-phase inputs and live storage.
fn brake_force<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    w: u32,
    index: u32,
    setup: &VehicleMoveForceSetup,
    velocity: [f32; 3],
    direction: [f32; 3],
    mut force: [f32; 3],
) -> Out<[f32; 3], B::Error> {
    let handbrake = number(float(b, v.wrapping_add(0x434))?)?;
    if handbrake > 0. && word(b, w.wrapping_add(0x120))? & 4 != 0 {
        if rb(b, v.wrapping_add(0xbc))? != 0 {
            force = scale(velocity, -number(float(b, w.wrapping_add(0xa0))?)?)?;
            let flags = word(b, w.wrapping_add(0x120))?;
            store(b, w.wrapping_add(0x120), flags | 0x20)?;
        } else {
            if handbrake > number(float(b, w.wrapping_add(0xa0))?)? {
                let flags = word(b, w.wrapping_add(0x120))?;
                store(b, w.wrapping_add(0x120), flags | 0x20)?;
            }
            let amount = if word(b, w.wrapping_add(0x120))? & 0x10000 != 0 {
                -handbrake
            } else {
                handbrake
            };
            force = plus(force, scale(direction, amount)?)?;
        }
        if square(velocity)? > 1. && word(b, w.wrapping_add(0x120))? & 0x20 != 0 {
            let flags = word(b, w.wrapping_add(0x120))?;
            store(b, w.wrapping_add(0x120), flags | 0x10)?;
        }
    }
    let saved = word(b, v.wrapping_add(0x5b8))?;
    let mut assist = false;
    if rb(b, v.wrapping_add(0x229))? != 0 {
        let other = if number(setup.local_angular_velocity[1])? > 0. && index == 1 {
            Some(word(b, v.wrapping_add(0x4c4))?.wrapping_sub(2))
        } else if number(setup.local_angular_velocity[1])? <= 0. && index == 0 {
            Some(word(b, v.wrapping_add(0x4c4))?.wrapping_sub(1))
        } else {
            None
        };
        if let Some(i) = other {
            let other = wheel(b, v, i)?;
            if add(
                float(b, other.wrapping_add(0xbc))?,
                f32::from_bits(0x3c8efa2e),
            )? < number(float(b, other.wrapping_add(0x108))?)?
            {
                force = [0.; 3];
                store_float(b, v.wrapping_add(0x5b8), 1.)?;
                assist = true;
            }
        }
    }
    let brake = number(float(b, v.wrapping_add(0x5b8))?)?;
    if brake > 0. || assist {
        let mut amount = mul(float(b, w.wrapping_add(0x118))?, brake)?;
        if rb(b, v.wrapping_add(0x228))? != 0 || assist {
            let available = sub(
                float(b, w.wrapping_add(0xa0))?,
                mul(length_yzx(force)?, 0.7_f32)?,
            )?;
            let available = if available < 0. { 0. } else { available };
            if amount > available {
                amount = available;
            }
        }
        let max = number(float(b, w.wrapping_add(0xa0))?)?;
        let dynamic = amount <= max
            && (rb(b, v.wrapping_add(0x5e8))? & 0x30 == 0
                || number(float(b, v.wrapping_add(0x59c))?)? < 0.7_f32);
        if dynamic {
            let amount = if word(b, w.wrapping_add(0x120))? & 0x10000 != 0 {
                -amount
            } else {
                amount
            };
            force = plus(force, scale(direction, amount)?)?;
        } else {
            let mut flags = word(b, w.wrapping_add(0x120))? | 0x20;
            store(b, w.wrapping_add(0x120), flags)?;
            if square(velocity)? > 1. {
                flags |= 0x10;
                store(b, w.wrapping_add(0x120), flags)?;
            }
            force = scale(velocity, -max)?;
        }
    }
    store(b, v.wrapping_add(0x5b8), saved)?;
    Ok(force)
}
#[allow(clippy::too_many_arguments)] // Retain the original force channels and accumulation order.
fn motor_force<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    w: u32,
    channel: usize,
    prepared: &VehicleMovePrepared,
    setup: &VehicleMoveForceSetup,
    velocity: [f32; 3],
    direction: [f32; 3],
    lateral: f32,
    mut force: [f32; 3],
    drive_sum: &mut f32,
) -> Out<[f32; 3], B::Error> {
    let flags = word(b, w.wrapping_add(0x120))?;
    if flags & 0x202 == 0 {
        return Ok(force);
    }
    let gear = word(b, v.wrapping_add(0x55c))? as i32;
    if gear == 0 {
        return Ok(force);
    }
    let grounded_override = rb(b, v.wrapping_add(0x1a4))? & 4 != 0
        && number(float(b, v.wrapping_add(0x3a4))?)? == 0.
        && number(float(b, v.wrapping_add(0x3a8))?)? == 0.;
    if grounded_override || rb(b, v.wrapping_add(0xc2c))? == 0 {
        return scale(velocity, -number(float(b, w.wrapping_add(0xa0))?)?);
    }
    let radius = number(float(b, w.wrapping_add(0x10c))?)?;
    let mut amount = if rb(b, v.wrapping_add(0x99))? == 0 {
        div(setup.limited_motor_force[1 - channel], radius)?
    } else {
        div(
            mul(
                setup.limited_motor_force[1 - channel],
                float(b, w.wrapping_add(0xa0))?,
            )?,
            mul(prepared.driven_normal_force[channel], radius)?,
        )?
    };
    if gear >= word(b, v.wrapping_add(0x224))? as i32 {
        let lat = number(float(b, w.wrapping_add(0xa4))?)?;
        let max = number(float(b, w.wrapping_add(0xa0))?)?;
        let lat2 = mul(lat, lat)?;
        let max2 = mul(max, max)?;
        let term = mul(mul(max2, lateral)?, lateral)?;
        let radicand = div(sub(mul(max2, lat2)?, term)?, lat2)?;
        // Native FSQRT may receive a negative PC24 rounding residual here.
        // Its unordered FCOM against the retained force takes JNZ at e3b06,
        // discarding that transient NaN and keeping the motor force unchanged.
        // Model this reached comparison; do not clamp the residual to zero.
        if radicand >= 0. {
            let bound = root(radicand)?;
            if amount > bound {
                amount = bound;
            }
        }
    }
    if sub(prepared.wheel_speed, float(b, v.wrapping_add(0x59c))?)?.abs() > 2.
        && number(float(b, w.wrapping_add(0xa0))?)? < amount.abs()
    {
        if square(velocity)? > f64::from(0.1_f32) {
            let flags = word(b, w.wrapping_add(0x120))?;
            store(b, w.wrapping_add(0x120), flags | 0x10)?;
        }
        let flags = word(b, w.wrapping_add(0x120))?;
        store(b, w.wrapping_add(0x120), flags | 0x80)?;
        wb(b, v.wrapping_add(0x550), 1)?;
    }
    let mode = word(b, v.wrapping_add(0xb8))?;
    let directional = if mode != 0 && mode != 2 {
        amount
    } else if gear > 0 {
        if number(float(b, v.wrapping_add(0xc64))?)? < f32::from_bits(0x3fc90fdb) {
            amount.abs()
        } else {
            amount
        }
    } else if gear < 0 {
        if number(float(b, v.wrapping_add(0xc64))?)? > f32::from_bits(0x3fc90fdb) {
            -amount
        } else {
            -amount.abs()
        }
    } else {
        0.
    };
    force = plus(force, scale(direction, directional)?)?;
    *drive_sum = add(amount.abs(), *drive_sum)?;
    Ok(force)
}
/// Exact phase input must be produced by preparation/setup for this invocation.
/// Traction-control/brake restoration at PC e41dc belongs to the next phase.
pub fn vehicle_move_wheel_forces<B: VehicleInitBackend>(
    b: &mut B,
    v: u32,
    prepared: VehicleMovePrepared,
    setup: VehicleMoveForceSetup,
) -> Out<VehicleMoveWheelForces, B::Error> {
    let mut linear = setup.linear_force;
    let mut torque = setup.torque;
    let mut normals = [0.; 3];
    let mut any = false;
    let mut drive_sum = 0.;
    let mut channel = 0;
    let count = word(b, v.wrapping_add(0x4c4))?;
    for index in (0..count).rev() {
        let w = wheel(b, v, index)?;
        let flags = word(b, w.wrapping_add(0x120))? & 0xfffffe4f;
        store(b, w.wrapping_add(0x120), flags)?;
        if flags & 8 != 0 && flags & 0x40000000 == 0 {
            any = true;
            let velocity = vector(b, w.wrapping_add(0x4c))?;
            let direction = vector(b, w.wrapping_add(0x58))?;
            let (force, lateral) = slip_force(b, v, w, velocity, direction)?;
            let force = brake_force(b, v, w, index, &setup, velocity, direction, force)?;
            let mut force = motor_force(
                b,
                v,
                w,
                channel,
                &prepared,
                &setup,
                velocity,
                direction,
                lateral,
                force,
                &mut drive_sum,
            )?;
            store(b, w.wrapping_add(0x17c), 0)?;
            store(b, w.wrapping_add(0x180), 0)?;
            let max_long = float(b, w.wrapping_add(0xa0))?;
            let max_lat = float(b, w.wrapping_add(0xa4))?;
            let [limit_lateral, limit_longitudinal, limit] =
                try_wheel_friction_limit(force, direction, max_long, max_lat)
                    .map_err(VehicleMoveWheelForcesUnavailable::Library)?;
            let magnitude = length_yzx(force)?;
            if magnitude > limit {
                // This copy of Normalize accumulates Y,Z,X before its X/Y/Z stores.
                let squared = square_yzx(force)?;
                force = scale(normalize(force, squared)?, limit)?;
            } else {
                let ratio = div(magnitude, limit)?;
                let lateral = mul(ratio, limit_lateral)?;
                let arg = div(lateral, max_lat)?;
                let sine_factor = mul(ratio, limit_longitudinal)?;
                let [sine, cosine] = try_acos_sin_cos_products(arg, sine_factor, lateral)
                    .map_err(VehicleMoveWheelForcesUnavailable::Library)?;
                store_float(b, w.wrapping_add(0x17c), sine)?;
                store_float(b, w.wrapping_add(0x180), cosine)?;
            }
            let long = float(b, w.wrapping_add(0xa0))?;
            let brake = float(b, w.wrapping_add(0x118))?;
            let selected = if number(long)? < number(brake)? {
                long
            } else {
                brake
            };
            let sum = add(selected, float(b, v.wrapping_add(0x148))?)?;
            store_float(b, v.wrapping_add(0x148), sum)?;
            let sum = add(
                float(b, w.wrapping_add(0xa4))?,
                float(b, v.wrapping_add(0x144))?,
            )?;
            store_float(b, v.wrapping_add(0x144), sum)?;
            write_vector(b, w.wrapping_add(0x7c), force)?;
            if word(b, w.wrapping_add(0x120))? & 0x202 != 0 {
                wb(b, v.wrapping_add(0x394), 1)?;
                let mode = word(b, v.wrapping_add(0xb8))?;
                let raw = mul(
                    float(b, w.wrapping_add(0xa0))?,
                    float(b, w.wrapping_add(0x10c))?,
                )?;
                let addition = if mode == 0 || mode == 2 {
                    Some(raw)
                } else if mode == 1 {
                    Some(mul(
                        number(float(
                            b,
                            v.wrapping_add(if channel == 1 { 0x540 } else { 0x544 }),
                        )?)?
                        .abs(),
                        raw,
                    )?)
                } else {
                    None
                };
                if let Some(value) = addition {
                    let sum = add(value, float(b, v.wrapping_add(0x554))?)?;
                    store_float(b, v.wrapping_add(0x554), sum)?;
                }
            }
            let suspension = vector(b, w.wrapping_add(0x34))?;
            linear = plus(linear, suspension)?;
            let center = vector(b, v.wrapping_add(0x398))?;
            let arm = minus(vector(b, w.wrapping_add(0x28))?, center)?;
            torque = plus(reverse_cross(arm, suspension)?, torque)?;
            linear = plus(linear, force)?;
            let arm = minus(
                vector(b, w.wrapping_add(0x1c))?,
                vector(b, v.wrapping_add(0x398))?,
            )?;
            torque = plus(reverse_cross(arm, force)?, torque)?;
            normals = plus(normals, vector(b, w.wrapping_add(0x40))?)?;
        }
        channel ^= 1;
    }
    Ok(VehicleMoveWheelForces {
        linear_force: linear,
        torque,
        contact_normal_sum: normals,
        any_contact: any,
        drive_force_sum: drive_sum,
        saved_traction_control: setup.saved_traction_control,
        saved_brake_bits: setup.saved_brake_bits,
    })
}

impl From<VehicleMoveWheelForces> for crate::vehicle_dynamics_body::VehicleMoveBodyForces {
    fn from(value: VehicleMoveWheelForces) -> Self {
        Self {
            linear_force: value.linear_force,
            torque: value.torque,
            contact_normal_sum: value.contact_normal_sum,
            any_contact: value.any_contact,
            drive_force_sum: value.drive_force_sum,
        }
    }
}

#[cfg(test)]
mod retained_square_tests {
    use super::*;
    fn dyadic(exponent: u32) -> f32 {
        f32::from_bits((127 - exponent) << 23)
    }
    #[test]
    fn retained_tiny_products_survive_until_the_normal_sum() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        let squared =
            precision.run(|| square_yzx::<()>([dyadic(60), dyadic(85), dyadic(70)]).unwrap());
        assert_eq!(squared.to_bits(), ((127 - 120) << 23) | 8);
        assert!(precision.precision_raised());
        let mut spill = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            spill.run(|| square_yzx::<()>([dyadic(100), 0., 0.]).unwrap()),
            0.
        );
        assert!(spill.precision_raised());
        let mut retained = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            retained.run(|| square::<()>([dyadic(100), 0., 0.]).unwrap()),
            2_f64.powi(-200)
        );
        assert!(!retained.precision_raised());
        assert_eq!(length_yzx::<()>([dyadic(70), 0., 0.]).unwrap(), 0.);
        assert_eq!(
            square_yzx::<()>([dyadic(70), 0., 0.]).unwrap().to_bits(),
            1 << 9
        );
    }
    #[test]
    fn retained_square_tracks_exact_and_lost_small_terms() {
        let mut exact = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            exact.run(|| square_yzx::<()>([dyadic(60), 0., 0.]).unwrap()),
            dyadic(120)
        );
        assert!(!exact.precision_raised());
        let mut inexact = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            inexact.run(|| square_yzx::<()>([dyadic(60), dyadic(100), 0.]).unwrap()),
            dyadic(120)
        );
        assert!(inexact.precision_raised());
    }
}
