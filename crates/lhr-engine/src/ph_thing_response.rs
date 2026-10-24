//! Original C_PhThing MoveCount45ddb0, CollisionCount45e510 and
//! CollisionFilter4601f0. All fields belong to the caller's canonical storage.
use crate::{
    native_transform::{SMatrix, SQuat},
    vehicle_init::VehicleInitBackend,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhThingResponseError<E> {
    Backend(E),
    NumericDomain(&'static str),
}
impl<E> From<&'static str> for PhThingResponseError<E> {
    fn from(value: &'static str) -> Self {
        Self::NumericDomain(value)
    }
}
type Out<T, E> = Result<T, PhThingResponseError<E>>;

/// PC24 arithmetic retains the x87 exponent until an explicit FST spill.
/// Binary64 holds the exact 24-bit dyadic operands and their wider exponent;
/// each arithmetic instruction rounds its mantissa separately.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct PhThingNumber(f64);
// Methods model individual checked x87 instructions and explicit stores.
// Keep instruction names visible instead of implicit operator evaluation.
#[allow(clippy::should_implement_trait)]
impl PhThingNumber {
    pub fn load(value: f32) -> Result<Self, &'static str> {
        if value.is_finite() {
            Ok(Self(f64::from(value)))
        } else {
            Err("nonfinite physical operand")
        }
    }
    fn rounded(value: f64) -> Result<Self, &'static str> {
        if !value.is_finite() || value != 0. && !value.is_normal() {
            return Err("physical retained exponent domain");
        }
        let bits = value.to_bits();
        let discarded = bits & ((1_u64 << 29) - 1);
        let mut retained = bits >> 29;
        if discarded > 1_u64 << 28 || discarded == 1_u64 << 28 && retained & 1 != 0 {
            retained += 1;
        }
        let result = f64::from_bits(retained << 29);
        if result.is_finite() {
            Ok(Self(result))
        } else {
            Err("physical retained exponent overflow")
        }
    }
    pub fn add(self, other: Self) -> Result<Self, &'static str> {
        let result = Self::rounded(self.0 + other.0)?;
        crate::native_precision::record_wide_pc24_add(self.0, other.0, result.0);
        Ok(result)
    }
    pub fn sub(self, other: Self) -> Result<Self, &'static str> {
        let result = Self::rounded(self.0 - other.0)?;
        crate::native_precision::record_wide_pc24_sub(self.0, other.0, result.0);
        Ok(result)
    }
    pub fn mul(self, other: Self) -> Result<Self, &'static str> {
        let result = Self::rounded(self.0 * other.0)?;
        crate::native_precision::record_wide_pc24_mul(self.0, other.0, result.0);
        Ok(result)
    }
    pub fn div(self, other: Self) -> Result<Self, &'static str> {
        Self::rounded(ph_thing_wide_div(self.0, other.0))
    }
    pub fn sqrt(self) -> Result<Self, &'static str> {
        let result = Self::rounded(self.0.sqrt())?;
        crate::native_precision::record_precision(result.0 * result.0 != self.0);
        Ok(result)
    }
    pub fn store(self) -> Result<f32, &'static str> {
        let value = self.0 as f32;
        if !value.is_finite() {
            return Err("physical binary32 spill overflow");
        }
        crate::native_precision::record_precision(f64::from(value) != self.0);
        Ok(value)
    }
    pub fn value(self) -> f64 {
        self.0
    }
    fn neg(self) -> Self {
        Self(-self.0)
    }
    fn abs(self) -> Self {
        Self(self.0.abs())
    }
}
/// Exact dyadic quotient rounded directly to PC24, including FILD's full
/// uint32 operand. An intermediate binary64 division could double-round it.
pub fn ph_thing_wide_div(a: f64, b: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() || a == 0. || b == 0. {
        return a / b;
    }
    let unpack = |x: f64| {
        let bits = x.abs().to_bits();
        (
            (bits & ((1_u64 << 52) - 1)) | (1_u64 << 52),
            ((bits >> 52) & 2047) as i32 - 1075,
        )
    };
    let (an, ae) = unpack(a);
    let (bn, be) = unpack(b);
    let mut power = (64 - an.leading_zeros()) as i32 - (64 - bn.leading_zeros()) as i32;
    if power >= 0 {
        if u128::from(an) < (u128::from(bn) << power) {
            power -= 1;
        }
    } else if (u128::from(an) << (-power)) < u128::from(bn) {
        power -= 1;
    }
    let shift = 23 - power;
    let (numerator, denominator) = if shift >= 0 {
        (u128::from(an) << shift, u128::from(bn))
    } else {
        (u128::from(an), u128::from(bn) << (-shift))
    };
    let mut quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder * 2 > denominator || remainder * 2 == denominator && quotient & 1 != 0 {
        quotient += 1;
    }
    crate::native_precision::record_precision(remainder != 0);
    let exponent = ae - be + power - 23;
    let result = quotient as f64 * 2_f64.powi(exponent);
    if a.is_sign_negative() != b.is_sign_negative() {
        -result
    } else {
        result
    }
}
pub(crate) fn ph_thing_norm_squared(
    vector: [f32; 3],
    order: [usize; 3],
) -> Result<PhThingNumber, &'static str> {
    let [x, y, z] = order.map(|i| PhThingNumber::load(vector[i]));
    let (x, y, z) = (x?, y?, z?);
    x.mul(x)?.add(y.mul(y)?)?.add(z.mul(z)?)
}
fn n(value: f32) -> Result<PhThingNumber, &'static str> {
    PhThingNumber::load(value)
}
fn word<B: VehicleInitBackend>(b: &mut B, address: u32) -> Out<u32, B::Error> {
    b.read_word(address).map_err(PhThingResponseError::Backend)
}
fn float<B: VehicleInitBackend>(b: &mut B, address: u32) -> Out<f32, B::Error> {
    Ok(f32::from_bits(word(b, address)?))
}
fn put<B: VehicleInitBackend>(b: &mut B, address: u32, value: u32) -> Out<(), B::Error> {
    b.write_word(address, value)
        .map_err(PhThingResponseError::Backend)
}
fn vector<B: VehicleInitBackend>(b: &mut B, address: u32) -> Out<[f32; 3], B::Error> {
    Ok([
        float(b, address)?,
        float(b, address.wrapping_add(4))?,
        float(b, address.wrapping_add(8))?,
    ])
}
fn put_vector<B: VehicleInitBackend>(
    b: &mut B,
    address: u32,
    value: [f32; 3],
) -> Out<(), B::Error> {
    for (i, x) in value.into_iter().enumerate() {
        put(b, address.wrapping_add(i as u32 * 4), x.to_bits())?;
    }
    Ok(())
}
fn matrix<B: VehicleInitBackend>(b: &mut B, address: u32) -> Out<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    for (i, x) in elements.iter_mut().enumerate() {
        *x = float(b, address.wrapping_add(i as u32 * 4))?;
    }
    Ok(SMatrix { elements })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhThingCollisionDescriptor {
    pub properties: u8,
    pub material: u8,
    pub owner: u32,
}
pub trait PhThingResponseBackend: VehicleInitBackend {
    /// Static world identities are semantic lookups; dynamic descriptors can
    /// read their actual caller-owned memory. No identity is cast to a pointer.
    fn ph_thing_collision_descriptor(
        &mut self,
        descriptor: u32,
    ) -> Result<PhThingCollisionDescriptor, Self::Error>;
}
/// The native bool-reference writes one byte only on the reached actor path.
/// A raw caller-stack byte keeps its original representation otherwise.
pub trait PhThingCollisionFlag {
    fn set_from_actor_kind(&mut self, value: bool);
}
impl PhThingCollisionFlag for bool {
    fn set_from_actor_kind(&mut self, value: bool) {
        *self = value;
    }
}
impl PhThingCollisionFlag for u8 {
    fn set_from_actor_kind(&mut self, value: bool) {
        *self = u8::from(value);
    }
}
/// Original CollisionFilter4601f0. The incoming output byte is retained when
/// the descriptor has no nonnull dynamic actor; point/normal are unused.
pub fn ph_thing_collision_filter<B: PhThingResponseBackend, F: PhThingCollisionFlag>(
    b: &mut B,
    descriptor: u32,
    skip_actor: &mut F,
) -> Out<u32, B::Error> {
    let source = b
        .ph_thing_collision_descriptor(descriptor)
        .map_err(PhThingResponseError::Backend)?;
    if source.properties & 0x40 != 0 && source.owner != 0 {
        skip_actor.set_from_actor_kind(word(b, source.owner.wrapping_add(0x10))? == 4);
    }
    Ok(if source.material == 0x1f {
        0
    } else {
        descriptor
    })
}

fn dot(a: [f32; 3], b: [f32; 3], order: [usize; 3]) -> Result<PhThingNumber, &'static str> {
    let [x, y, z] = order;
    n(a[x])?
        .mul(n(b[x])?)?
        .add(n(a[y])?.mul(n(b[y])?)?)?
        .add(n(a[z])?.mul(n(b[z])?)?)
}
fn cross(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], &'static str> {
    Ok([
        n(a[1])?
            .mul(n(b[2])?)?
            .sub(n(a[2])?.mul(n(b[1])?)?)?
            .store()?,
        n(a[2])?
            .mul(n(b[0])?)?
            .sub(n(a[0])?.mul(n(b[2])?)?)?
            .store()?,
        n(a[0])?
            .mul(n(b[1])?)?
            .sub(n(a[1])?.mul(n(b[0])?)?)?
            .store()?,
    ])
}
fn scale(v: [f32; 3], factor: PhThingNumber) -> Result<[f32; 3], &'static str> {
    Ok([
        n(v[0])?.mul(factor)?.store()?,
        n(v[1])?.mul(factor)?.store()?,
        n(v[2])?.mul(factor)?.store()?,
    ])
}
fn plus(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], &'static str> {
    Ok([
        n(a[0])?.add(n(b[0])?)?.store()?,
        n(a[1])?.add(n(b[1])?)?.store()?,
        n(a[2])?.add(n(b[2])?)?.store()?,
    ])
}
fn minus(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], &'static str> {
    Ok([
        n(a[0])?.sub(n(b[0])?)?.store()?,
        n(a[1])?.sub(n(b[1])?)?.store()?,
        n(a[2])?.sub(n(b[2])?)?.store()?,
    ])
}
fn transform_assign(m: SMatrix, v: [f32; 3]) -> Result<[f32; 3], &'static str> {
    let e = m.elements;
    let mut out = [0.; 3];
    for (column, x) in out.iter_mut().enumerate() {
        let order = if column == 0 { [2, 1, 0] } else { [2, 0, 1] };
        let [a, b, c] = order;
        *x = n(e[a * 4 + column])?
            .mul(n(v[a])?)?
            .add(n(e[b * 4 + column])?.mul(n(v[b])?)?)?
            .add(n(e[c * 4 + column])?.mul(n(v[c])?)?)?
            .add(n(e[12 + column])?)?
            .store()?;
    }
    Ok(out)
}
fn matrix_product(a: SMatrix, b: SMatrix) -> Result<SMatrix, &'static str> {
    let mut out = SMatrix::identity();
    for (row, output) in out.elements.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        for (column, output) in output[..3].iter_mut().enumerate() {
            let mut value = n(a.elements[row * 4])?.mul(n(b.elements[column])?)?;
            if row == 3 {
                value = value.add(n(b.elements[12 + column])?)?;
            }
            value = value
                .add(n(a.elements[row * 4 + 1])?.mul(n(b.elements[4 + column])?)?)?
                .add(n(a.elements[row * 4 + 2])?.mul(n(b.elements[8 + column])?)?)?;
            *output = value.store()?;
        }
    }
    Ok(out)
}
const EPSILON: f32 = f32::from_bits(0x322b_cc77);
fn normalized(mut value: [f32; 3]) -> Result<[f32; 3], &'static str> {
    // Original inline XYZ norm has an explicit binary32 stack spill.
    let squared = ph_thing_norm_squared(value, [0, 1, 2])?.store()?;
    if n(squared)?.sub(n(1.)?)?.abs().value() >= f64::from(EPSILON) {
        if squared >= EPSILON {
            value = scale(value, n(1.)?.div(n(squared)?.sqrt()?)?)?;
        } else {
            let axis = if value[0] != 0. {
                0
            } else if value[2] != 0. {
                2
            } else {
                1
            };
            value[axis] = if value[axis] < 0. { -1. } else { 1. };
        }
    }
    Ok(value)
}
fn normalize_memory<B: VehicleInitBackend>(b: &mut B, address: u32) -> Out<(), B::Error> {
    let value = vector(b, address)?;
    let squared = ph_thing_norm_squared(value, [0, 1, 2])?.store()?;
    if n(squared)?.sub(n(1.)?)?.abs().value() < f64::from(EPSILON) {
        return Ok(());
    }
    if squared >= EPSILON {
        let factor = n(1.)?.div(n(squared)?.sqrt()?)?;
        for (i, x) in value.into_iter().enumerate() {
            put(
                b,
                address.wrapping_add(i as u32 * 4),
                n(x)?.mul(factor)?.store()?.to_bits(),
            )?;
        }
    } else {
        let axis = if value[0] != 0. {
            0
        } else if value[2] != 0. {
            2
        } else {
            1
        };
        put(
            b,
            address.wrapping_add(axis as u32 * 4),
            if value[axis] < 0. {
                (-1_f32).to_bits()
            } else {
                1_f32.to_bits()
            },
        )?;
    }
    Ok(())
}
fn compose(a: SQuat, b: SQuat) -> Result<SQuat, &'static str> {
    let [aw, ax, ay, az] = a.wxyz;
    let [bw, bx, by, bz] = b.wxyz;
    let scalar = n(aw)?.mul(n(bw)?)?.store()?;
    let w = n(scalar)?
        .sub(dot([ax, ay, az], [bx, by, bz], [2, 1, 0])?)?
        .store()?;
    let av = scale([ax, ay, az], n(bw)?)?;
    let bv = scale([bx, by, bz], n(aw)?)?;
    let value = plus(plus(bv, av)?, cross([ax, ay, az], [bx, by, bz])?)?;
    Ok(SQuat {
        wxyz: [w, value[0], value[1], value[2]],
    })
}
/// Original MoveCount45ddb0, retained PS2 MoveCount__9C_PhThingFfRC8S_vector.
pub fn ph_thing_move_count<B: PhThingResponseBackend>(
    b: &mut B,
    actor: u32,
    dt: f32,
    displacement: [f32; 3],
) -> Out<(), B::Error> {
    let delta_x = n(dt)?
        .mul(n(float(b, actor.wrapping_add(0x22c))?)?)?
        .store()?;
    let delta_y = n(dt)?.mul(n(float(b, actor.wrapping_add(0x230))?)?)?;
    let mut local = matrix(b, actor.wrapping_add(0x14c))?;
    let delta_y = delta_y.store()?;
    let delta_z = n(dt)?
        .mul(n(float(b, actor.wrapping_add(0x234))?)?)?
        .store()?;
    local.elements[12..15].fill(0.);
    let delta = transform_assign(local, [delta_x, delta_y, delta_z])?;
    let side = vector(b, actor.wrapping_add(0xa0))?;
    let up = vector(b, actor.wrapping_add(0xac))?;
    let direction = vector(b, actor.wrapping_add(0x30))?;
    let q = |axis, angle| {
        SQuat::try_make_axis_angle(axis, angle).map_err(|_| "physical axis-angle dependency")
    };
    let rotation = compose(
        compose(q(side, delta[0])?, q(up, delta[1])?)?,
        q(direction, delta[2])?,
    )?;
    let mut m = SMatrix::identity();
    m.try_set_rot3(rotation)
        .map_err(|_| "physical RotationMatrix dependency")?;
    put_vector(b, actor.wrapping_add(0x30), transform_assign(m, direction)?)?;
    put_vector(b, actor.wrapping_add(0xac), transform_assign(m, up)?)?;
    if b.read_byte(actor.wrapping_add(0x76))
        .map_err(PhThingResponseError::Backend)?
        != 0
    {
        let center = vector(b, actor.wrapping_add(0x84))?;
        let mut from = SMatrix::identity();
        from.elements[12..15].copy_from_slice(&center.map(|x| -x));
        let mut to = SMatrix::identity();
        to.elements[12..15].copy_from_slice(&center);
        let rotate = matrix_product(matrix_product(from, m)?, to)?;
        let position = vector(b, actor.wrapping_add(0x24))?;
        put_vector(
            b,
            actor.wrapping_add(0x24),
            transform_assign(rotate, position)?,
        )?;
    }
    let direction = vector(b, actor.wrapping_add(0x30))?;
    let up = vector(b, actor.wrapping_add(0xac))?;
    let side = cross(up, direction)?;
    put_vector(b, actor.wrapping_add(0xa0), side)?;
    put_vector(b, actor.wrapping_add(0xac), cross(direction, side)?)?;
    for offset in [0xa0, 0xac, 0x30] {
        normalize_memory(b, actor.wrapping_add(offset))?;
    }
    for (i, x) in displacement.into_iter().enumerate() {
        let address = actor.wrapping_add(0x24 + i as u32 * 4);
        let previous = float(b, address)?;
        let result = if i == 0 {
            n(previous)?.add(n(x)?)?
        } else {
            n(x)?.add(n(previous)?)?
        };
        put(b, address, result.store()?.to_bits())?;
    }
    let frame = word(b, actor.wrapping_add(0x68))?;
    if b.read_byte(frame.wrapping_add(0xac))
        .map_err(PhThingResponseError::Backend)?
        & 0x80
        == 0
    {
        let position = vector(b, actor.wrapping_add(0x24))?;
        put_vector(b, frame.wrapping_add(0x80), position)?;
        let flags = word(b, frame.wrapping_add(0xac))?;
        put(
            b,
            frame.wrapping_add(0xac),
            flags & 0xffff_fedf | 0x4000_0000,
        )?;
    }
    let frame = word(b, actor.wrapping_add(0x68))?;
    let mut basis = matrix(b, frame.wrapping_add(0x50))?;
    let direction = vector(b, actor.wrapping_add(0x30))?;
    let up = vector(b, actor.wrapping_add(0xac))?;
    let outcome = basis.try_set_dir3(direction, up);
    for index in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
        put(
            b,
            frame.wrapping_add(0x50 + index * 4),
            basis.elements[index as usize].to_bits(),
        )?;
    }
    outcome.map_err(|_| "physical SetDir3 dependency")?;
    let flags = word(b, frame.wrapping_add(0xac))?;
    put(
        b,
        frame.wrapping_add(0xac),
        flags & 0xffff_fed3 | 0x4000_0010,
    )
}

fn inverse_inertia<B: VehicleInitBackend>(
    b: &mut B,
    actor: u32,
    value: [f32; 3],
) -> Out<[f32; 3], B::Error> {
    let mut result = transform_assign(matrix(b, actor.wrapping_add(0x1cc))?, value)?;
    for (i, x) in result.iter_mut().enumerate() {
        *x = n(*x)?
            .mul(n(float(b, actor.wrapping_add(0x214 + i as u32 * 4))?)?)?
            .store()?;
    }
    Ok(transform_assign(
        matrix(b, actor.wrapping_add(0x18c))?,
        result,
    )?)
}
fn vector_length(value: [f32; 3]) -> Result<f32, &'static str> {
    let square = ph_thing_norm_squared(value, [0, 1, 2])?.store()?;
    if (square.to_bits() as i32) < EPSILON.to_bits() as i32 {
        Ok(0.)
    } else if n(square)?.sub(n(1.)?)?.abs().value() < f64::from(EPSILON) {
        Ok(1.)
    } else {
        n(square)?.sqrt()?.store()
    }
}
fn add_memory<B: VehicleInitBackend>(
    b: &mut B,
    address: u32,
    value: [f32; 3],
) -> Out<(), B::Error> {
    for (i, x) in value.into_iter().enumerate() {
        let at = address.wrapping_add(i as u32 * 4);
        let previous = float(b, at)?;
        put(b, at, n(x)?.add(n(previous)?)?.store()?.to_bits())?;
    }
    Ok(())
}
/// Original CollisionCount45e510. The scalar and final actor stack arguments
/// are retained in the API but unused by this native body. The corner index's
/// low16 bits are published only when the contact raises the peak impulse.
#[allow(clippy::too_many_arguments)]
pub fn ph_thing_collision_count<B: PhThingResponseBackend>(
    b: &mut B,
    actor: u32,
    linear_address: u32,
    angular_address: u32,
    normal: [f32; 3],
    point: [f32; 3],
    _dt: f32,
    index: u32,
    _hit_actor: u32,
) -> Out<(), B::Error> {
    let center = vector(b, actor.wrapping_add(0x84))?;
    let arm = minus(point, center)?;
    let angular = vector(b, actor.wrapping_add(0x22c))?;
    let rotational = cross(angular, arm)?;
    let linear = vector(b, actor.wrapping_add(0x220))?;
    let velocity = minus(linear, rotational)?;
    let inverse = inverse_inertia(b, actor, cross(arm, normal)?)?;
    let approach = dot(velocity, normal, [1, 2, 0])?;
    if approach.value() >= 0. {
        return Ok(());
    }
    let coefficient = float(b, actor.wrapping_add(0x23c))?;
    let numerator = n(coefficient)?.neg().mul(approach)?.store()?;
    let lever = cross(inverse, arm)?;
    let mass = float(b, actor.wrapping_add(0x210))?;
    let inverse_mass = n(1.)?.div(n(mass)?)?.store()?;
    let mut denominator = dot(lever, normal, [1, 2, 0])?.add(n(inverse_mass)?)?;
    let floor = f32::from_bits(0x3586_37bd);
    if denominator.abs().value() < f64::from(floor) {
        denominator = n(floor)?;
    }
    let impulse = scale(normal, n(numerator)?.div(denominator)?)?;
    add_memory(b, linear_address, scale(impulse, n(inverse_mass)?)?)?;
    let angular_impulse = inverse_inertia(b, actor, cross(arm, impulse)?)?;
    for (i, x) in angular_impulse.into_iter().enumerate() {
        let address = angular_address.wrapping_add(i as u32 * 4);
        let previous = float(b, address)?;
        put(b, address, n(previous)?.sub(n(x)?)?.store()?.to_bits())?;
    }
    let magnitude = vector_length(impulse)?;
    if magnitude > float(b, actor.wrapping_add(0x370))? {
        put(b, actor.wrapping_add(0x370), magnitude.to_bits())?;
        for (i, byte) in (index as u16).to_le_bytes().into_iter().enumerate() {
            b.write_byte(actor.wrapping_add(0x36c + i as u32), byte)
                .map_err(PhThingResponseError::Backend)?;
        }
        put_vector(b, actor.wrapping_add(0x33c), point)?;
    }
    let direction = normalized(velocity)?;
    let tangent = normalized(cross(cross(direction, normal)?, normal)?)?;
    let friction = float(b, actor.wrapping_add(0x238))?;
    let timestep = float(b, actor.wrapping_add(0x254))?;
    let mass = float(b, actor.wrapping_add(0x210))?;
    let friction_impulse = crate::native_transform::try_ph_thing_friction_products(
        velocity, normal, magnitude, friction, timestep, mass, tangent,
    )
    .map_err(|_| "physical retained friction dependency")?;
    let mass = float(b, actor.wrapping_add(0x210))?;
    let inverse_mass = n(1.)?.div(n(mass)?)?.store()?;
    add_memory(
        b,
        linear_address,
        scale(friction_impulse, n(inverse_mass)?)?,
    )?;
    let angular_impulse = inverse_inertia(b, actor, cross(friction_impulse, arm)?)?;
    add_memory(b, angular_address, angular_impulse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    struct Backend {
        bytes: Vec<u8>,
    }
    impl Backend {
        fn new() -> Self {
            Self {
                bytes: vec![0; 65536],
            }
        }
        fn set(&mut self, address: u32, value: f32) {
            self.write_word(address, value.to_bits()).unwrap();
        }
        fn set_vector(&mut self, address: u32, value: [f32; 3]) {
            put_vector(self, address, value).unwrap();
        }
        fn set_matrix(&mut self, address: u32, value: SMatrix) {
            for (i, x) in value.elements.into_iter().enumerate() {
                self.set(address + i as u32 * 4, x);
            }
        }
        fn actor() -> Self {
            let mut b = Self::new();
            for address in [0x114c, 0x118c, 0x11cc, 0x2050] {
                b.set_matrix(address, SMatrix::identity());
            }
            b.write_word(0x1068, 0x2000).unwrap();
            b.set_vector(0x1030, [0., 0., 1.]);
            b.set_vector(0x10a0, [1., 0., 0.]);
            b.set_vector(0x10ac, [0., 1., 0.]);
            b.set_vector(0x1214, [1.; 3]);
            b.set(0x1210, 2.);
            b.set(0x1238, 0.5);
            b.set(0x123c, 1.5);
            b.set(0x1254, 0.125);
            b
        }
    }
    impl VehicleInitBackend for Backend {
        type Error = &'static str;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.bytes.get(a as usize).copied().ok_or("address")
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            Ok(u32::from_le_bytes(
                self.bytes
                    .get(a as usize..a as usize + 4)
                    .ok_or("address")?
                    .try_into()
                    .unwrap(),
            ))
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            *self.bytes.get_mut(a as usize).ok_or("address")? = v;
            Ok(())
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.bytes
                .get_mut(a as usize..a as usize + 4)
                .ok_or("address")?
                .copy_from_slice(&v.to_le_bytes());
            Ok(())
        }
        fn free(&mut self, _: u32) -> Result<(), Self::Error> {
            Err("unexpected free")
        }
        fn allocate(&mut self, _: u32) -> Result<u32, Self::Error> {
            Err("unexpected allocation")
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err("unexpected collect")
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            unreachable!()
        }
    }
    impl PhThingResponseBackend for Backend {
        fn ph_thing_collision_descriptor(
            &mut self,
            a: u32,
        ) -> Result<PhThingCollisionDescriptor, Self::Error> {
            let properties = self.read_byte(a)?;
            let owner = if properties & 0x40 != 0 {
                self.read_word(a + 0x44)?
            } else {
                0
            };
            Ok(PhThingCollisionDescriptor {
                properties,
                owner,
                material: self.read_byte(a + 3)?,
            })
        }
    }
    #[test]
    fn retained_products_spill_subnormals_only_at_fst_and_track_precision() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            let value = n(f32::MIN_POSITIVE).unwrap().mul(n(0.5).unwrap()).unwrap();
            assert_eq!(value.value(), 2_f64.powi(-127));
            assert_eq!(value.store().unwrap().to_bits(), 0x0040_0000);
        });
        assert!(!precision.precision_raised());
        let value = precision.run(|| {
            n(f32::MIN_POSITIVE)
                .unwrap()
                .mul(n(2_f32.powi(-24)).unwrap())
                .unwrap()
        });
        assert_eq!(value.value(), 2_f64.powi(-150));
        assert!(!precision.precision_raised());
        assert_eq!(precision.run(|| value.store().unwrap()).to_bits(), 0);
        assert!(precision.precision_raised());
    }
    #[test]
    fn collision_filter_preserves_incoming_byte_and_only_water_rejects() {
        let mut b = Backend::new();
        let descriptor = 0x3000;
        b.write_byte(descriptor + 3, 1).unwrap();
        let mut skip = true;
        assert_eq!(
            ph_thing_collision_filter(&mut b, descriptor, &mut skip),
            Ok(descriptor)
        );
        assert!(skip);
        b.write_byte(descriptor, 0x40).unwrap();
        assert_eq!(
            ph_thing_collision_filter(&mut b, descriptor, &mut skip),
            Ok(descriptor)
        );
        assert!(skip);
        b.write_word(descriptor + 0x44, 0x4000).unwrap();
        b.write_word(0x4010, 5).unwrap();
        assert_eq!(
            ph_thing_collision_filter(&mut b, descriptor, &mut skip),
            Ok(descriptor)
        );
        assert!(!skip);
        b.write_word(0x4010, 4).unwrap();
        b.write_byte(descriptor + 3, 31).unwrap();
        assert_eq!(
            ph_thing_collision_filter(&mut b, descriptor, &mut skip),
            Ok(0)
        );
        assert!(skip);
    }
    #[test]
    fn move_count_publishes_displacement_and_preserves_frame_lock() {
        let mut b = Backend::actor();
        b.set_vector(0x1024, [4., 5., 6.]);
        ph_thing_move_count(&mut b, 0x1000, 0., [1., 2., 3.]).unwrap();
        assert_eq!(vector(&mut b, 0x1024).unwrap(), [5., 7., 9.]);
        assert_eq!(vector(&mut b, 0x2080).unwrap(), [5., 7., 9.]);
        assert_eq!(word(&mut b, 0x20ac).unwrap(), 0x4000_0010);
        b.write_word(0x20ac, 0x80).unwrap();
        b.set_vector(0x2080, [42.; 3]);
        ph_thing_move_count(&mut b, 0x1000, 0., [0.; 3]).unwrap();
        assert_eq!(vector(&mut b, 0x2080).unwrap(), [42.; 3]);
        assert_eq!(word(&mut b, 0x20ac).unwrap(), 0x4000_0090);
    }
    #[test]
    fn move_count_rotates_about_the_authored_center_before_displacement() {
        let mut b = Backend::actor();
        b.write_byte(0x1076, 1).unwrap();
        b.set_vector(0x1084, [3., 4., 5.]);
        b.set_vector(0x1024, [4., 4., 5.]);
        b.set_vector(0x122c, [0., 0., 1.]);
        ph_thing_move_count(
            &mut b,
            0x1000,
            std::f32::consts::FRAC_PI_2,
            [0.25, 0.5, -1.],
        )
        .unwrap();
        let position = vector(&mut b, 0x1024).unwrap();
        for (actual, expected) in position.into_iter().zip([3.25, 3.5, 4.]) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert_eq!(vector(&mut b, 0x2080).unwrap(), position);
        assert!((vector(&mut b, 0x10ac).unwrap()[0] - 1.).abs() < 1e-6);
    }
    #[test]
    fn separating_contact_leaves_canonical_storage_untouched() {
        let mut b = Backend::actor();
        b.set_vector(0x1220, [1., 2., 0.]);
        let before = b.bytes.clone();
        ph_thing_collision_count(
            &mut b,
            0x1000,
            0x1220,
            0x122c,
            [0., 1., 0.],
            [1., 0., 0.],
            0.5,
            13,
            0,
        )
        .unwrap();
        assert_eq!(b.bytes, before);
    }
    #[test]
    fn normal_and_clamped_friction_impulses_publish_peak_and_low16_corner() {
        let mut b = Backend::actor();
        b.set_vector(0x1220, [1., -2., 0.]);
        b.write_word(0x136c, 0x1234_5678).unwrap();
        ph_thing_collision_count(
            &mut b,
            0x1000,
            0x1220,
            0x122c,
            [0., 1., 0.],
            [1., 0., 0.],
            999.,
            0xdead_abcd,
            0,
        )
        .unwrap();
        assert_eq!(vector(&mut b, 0x1220).unwrap(), [0.8125, -1., 0.]);
        assert_eq!(vector(&mut b, 0x122c).unwrap(), [0., 0., -2.]);
        assert_eq!(float(&mut b, 0x1370).unwrap(), 2.);
        assert_eq!(word(&mut b, 0x136c).unwrap(), 0x1234_abcd);
        assert_eq!(vector(&mut b, 0x133c).unwrap(), [1., 0., 0.]);
    }
}
