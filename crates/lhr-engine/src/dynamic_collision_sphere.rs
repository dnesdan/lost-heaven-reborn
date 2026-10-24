//! Original g_collision::SphereInDynamics, PC5cafb0. Registered cells and
//! descriptors remain in the caller's canonical heap; query state is shared
//! with the following static pass rather than reconstructed after a hit.
use crate::{
    collision_swept::{SphereParam, SphereSweepError},
    ph_thing_response::{ph_thing_norm_squared, PhThingNumber as R},
    vehicle_init::VehicleInitBackend,
};

#[derive(Debug, PartialEq)]
pub enum DynamicSphereError<E> {
    Backend(E),
    NumericDomain(&'static str),
    StorageRange,
    Sphere(SphereSweepError),
}
impl<E> From<&'static str> for DynamicSphereError<E> {
    fn from(e: &'static str) -> Self {
        Self::NumericDomain(e)
    }
}
impl<E> From<SphereSweepError> for DynamicSphereError<E> {
    fn from(e: SphereSweepError) -> Self {
        Self::Sphere(e)
    }
}
type Out<T, E> = Result<T, DynamicSphereError<E>>;
fn read<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(DynamicSphereError::Backend)
}
fn put<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    b.write_word(a, v).map_err(DynamicSphereError::Backend)
}
fn f<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    Ok(f32::from_bits(read(b, a)?))
}
fn vec3<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([f(b, a)?, f(b, a + 4)?, f(b, a + 8)?])
}
fn matrix<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<[f32; 16], B::Error> {
    let mut m = [0.; 16];
    for (i, x) in m.iter_mut().enumerate() {
        *x = f(b, a + i as u32 * 4)?;
    }
    Ok(m)
}
fn n(x: f32) -> Result<R, &'static str> {
    R::load(x)
}
fn dot(a: [f32; 3], b: [f32; 3], order: [usize; 3]) -> Result<R, &'static str> {
    let [x, y, z] = order;
    n(a[x])?
        .mul(n(b[x])?)?
        .add(n(a[y])?.mul(n(b[y])?)?)?
        .add(n(a[z])?.mul(n(b[z])?)?)
}
fn difference(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], &'static str> {
    Ok([
        n(a[0])?.sub(n(b[0])?)?.store()?,
        n(a[1])?.sub(n(b[1])?)?.store()?,
        n(a[2])?.sub(n(b[2])?)?.store()?,
    ])
}
fn scale(a: [f32; 3], v: R) -> Result<[f32; 3], &'static str> {
    Ok([
        n(a[0])?.mul(v)?.store()?,
        n(a[1])?.mul(v)?.store()?,
        n(a[2])?.mul(v)?.store()?,
    ])
}
fn plus(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 3], &'static str> {
    Ok([
        n(a[0])?.add(n(b[0])?)?.store()?,
        n(a[1])?.add(n(b[1])?)?.store()?,
        n(a[2])?.add(n(b[2])?)?.store()?,
    ])
}
const EPS: f32 = f32::from_bits(0x322bcc77);
fn length(a: [f32; 3]) -> Result<R, &'static str> {
    let square = ph_thing_norm_squared(a, [0, 1, 2])?.store()?;
    if (square.to_bits() as i32) < EPS.to_bits() as i32 {
        n(0.)
    } else if n(square)?.sub(n(1.)?)?.value().abs() < f64::from(EPS) {
        n(1.)
    } else {
        n(square)?.sqrt()
    }
}
fn normalized(mut a: [f32; 3], order: [usize; 3]) -> Result<[f32; 3], &'static str> {
    let square = ph_thing_norm_squared(a, order)?.store()?;
    if n(square)?.sub(n(1.)?)?.value().abs() >= f64::from(EPS) {
        if square >= EPS {
            a = scale(a, n(1.)?.div(n(square)?.sqrt()?)?)?;
        } else {
            let i = if a[0] != 0. {
                0
            } else if a[2] != 0. {
                2
            } else {
                1
            };
            a[i] = if a[i] < 0. { -1. } else { 1. };
        }
    }
    Ok(a)
}
fn transform(a: [f32; 3], m: [f32; 16]) -> Result<[f32; 3], &'static str> {
    let mut result = [0.; 3];
    for (c, x) in result.iter_mut().enumerate() {
        let [i, j, k] = if c == 0 { [2, 1, 0] } else { [2, 0, 1] };
        *x = n(m[i * 4 + c])?
            .mul(n(a[i])?)?
            .add(n(m[j * 4 + c])?.mul(n(a[j])?)?)?
            .add(n(m[k * 4 + c])?.mul(n(a[k])?)?)?
            .add(n(m[12 + c])?)?
            .store()?;
    }
    Ok(result)
}
fn rotate(a: [f32; 3], m: [f32; 16], normalise: bool) -> Result<[f32; 3], &'static str> {
    let mut result = [0.; 3];
    for (c, x) in result.iter_mut().enumerate() {
        *x = dot(a, [m[c], m[4 + c], m[8 + c]], [0, 1, 2])?.store()?;
    }
    if normalise {
        for (c, x) in result.iter_mut().enumerate() {
            let square = ph_thing_norm_squared([m[c], m[4 + c], m[8 + c]], [0, 1, 2])?;
            if square.sub(n(1.)?)?.value().abs() > f64::from(EPS) {
                *x = n(*x)?.div(square.sqrt()?)?.store()?;
            }
        }
    }
    Ok(result)
}
fn uniform_scale(m: [f32; 16]) -> Result<R, &'static str> {
    let x = length([m[0], m[1], m[2]])?.store()?;
    let y = length([m[4], m[5], m[6]])?.store()?;
    let z = length([m[8], m[9], m[10]])?;
    z.add(n(y)?)?
        .add(n(x)?)?
        .mul(n(f32::from_bits(0x3eaaaaab))?)
}
fn integer<E>(x: R) -> Out<i32, E> {
    let v = x.value();
    if !(-2147483648.0..2147483648.0).contains(&v) {
        return Err(DynamicSphereError::NumericDomain(
            "dynamic sphere cell conversion",
        ));
    }
    let result = v.trunc() as i32;
    crate::native_precision::record_precision(f64::from(result) != v);
    Ok(result)
}

/// Original row-major dynamic cell traversal. Group exclusion precedes the
/// descriptor's reject flags in this worker, unlike LineInDynamics.
pub fn sphere_in_dynamics<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    selector: u32,
    q: &mut SphereParam,
) -> Out<bool, B::Error> {
    let grid = read(b, manager + 0x28c)?;
    if grid == 0 {
        return Ok(false);
    }
    let width = read(b, grid + 0x10)?;
    let height = read(b, grid + 0x14)?;
    if width > i32::MAX as u32
        || height > i32::MAX as u32
        || width.checked_mul(height).is_none_or(|n| n > 1 << 20)
    {
        return Err(DynamicSphereError::StorageRange);
    }
    let mut begin = [0; 2];
    let mut end = [0; 2];
    for axis in 0..2 {
        let off = axis as u32 * 4;
        let inverse = n(1.)?.div(n(f(b, grid + 8 + off)?)?)?;
        let minimum = f(b, grid + off)?;
        begin[axis] = integer(
            n(q.min_xz[axis])?
                .sub(n(minimum)?)?
                .mul(inverse)?
                .sub(n(1.)?)?,
        )?
        .max(0);
        end[axis] = (integer(
            n(q.max_xz[axis])?
                .sub(n(minimum)?)?
                .mul(inverse)?
                .add(n(2.)?)?,
        )? as u32)
            .min([width, height][axis]) as i32;
    }
    let mut hit = false;
    for z in begin[1]..end[1] {
        for x in begin[0]..end[0] {
            let live_grid = read(b, manager + 0x28c)?;
            let table = read(b, live_grid + 0x18)?;
            let row_width = read(b, live_grid + 0x10)?;
            let cell = read(b, table + (z as u32 * row_width + x as u32) * 4)?;
            if cell == 0 {
                continue;
            }
            let mut at = read(b, cell + 0xc)?;
            loop {
                let end = read(b, cell + 0x10)?;
                if at >= end {
                    break;
                }
                if !end.wrapping_sub(at).is_multiple_of(4) || end.wrapping_sub(at) / 4 > 1 << 20 {
                    return Err(DynamicSphereError::StorageRange);
                }
                let object = read(b, at)?;
                at = at.wrapping_add(4);
                let count = read(b, manager + 0x2bc)?;
                if count > 1 << 20 {
                    return Err(DynamicSphereError::StorageRange);
                }
                let mut excluded = false;
                if count != 0 {
                    let list = read(b, manager + 0x2b8)?;
                    for i in (0..count).rev() {
                        if read(b, list + i * 4)? == object {
                            excluded = true;
                            break;
                        }
                    }
                }
                if excluded {
                    continue;
                }
                if read(b, object + 8)? & selector != 0 {
                    continue;
                }
                match sphere_in_dynamic_object(b, manager, object, q)? {
                    DynamicSphereObjectResult::Miss => {}
                    DynamicSphereObjectResult::Hit => hit = true,
                    DynamicSphereObjectResult::Abort => return Ok(false),
                }
            }
        }
    }
    Ok(hit)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicSphereObjectResult {
    Miss,
    Hit,
    Abort,
}

/// One eligible descriptor. Abort is the original cylinder branch's early
/// false return, and deliberately retains any preceding query mutations.
pub fn sphere_in_dynamic_object<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    object: u32,
    q: &mut SphereParam,
) -> Out<DynamicSphereObjectResult, B::Error> {
    let center = vec3(b, object + 0x10)?;
    let square = ph_thing_norm_squared(q.delta, [0, 1, 2])?.store()?;
    let distance = if square >= EPS {
        let offset = difference(center, q.origin)?;
        let t = dot(q.delta, offset, [2, 1, 0])?.div(n(square)?)?.store()?;
        length(difference(plus(q.origin, scale(q.delta, n(t)?)?)?, center)?)?
    } else {
        n(-1e16)?
    };
    let radius = f(b, object + 0x1c)?;
    if distance.value() > n(q.radius)?.add(n(radius)?)?.value()
        || n(radius)?.add(n(center[0])?)?.value() < f64::from(q.min_xz[0])
        || n(center[0])?.sub(n(radius)?)?.value() > f64::from(q.max_xz[0])
        || n(radius)?.add(n(center[2])?)?.value() < f64::from(q.min_xz[1])
        || n(center[2])?.sub(n(radius)?)?.value() > f64::from(q.max_xz[1])
    {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    match read(b, object)? as u8 {
        0x40 | 0x46 => {
            let scaled = read(b, object)? as u8 == 0x46;
            let mut local = *q;
            let inverse = read(b, object + 0x40)?;
            local.origin = transform(q.origin, matrix(b, inverse)?)?;
            let inverse = read(b, object + 0x40)?;
            local.end = transform(q.end, matrix(b, inverse)?)?;
            if scaled {
                let inverse = read(b, object + 0x40)?;
                local.radius = uniform_scale(matrix(b, inverse)?)?
                    .mul(n(local.radius)?)?
                    .store()?;
            }
            local.delta = difference(local.end, local.origin)?;
            local.direction = normalized(local.delta, [2, 1, 0])?;
            let min = vec3(b, object + 0x24)?;
            let max = vec3(b, object + 0x30)?;
            if !crate::collision_swept_aabox::sphere_in_aa_box(min, max, &mut local)? {
                return Ok(DynamicSphereObjectResult::Miss);
            }
            q.point = local.point;
            let forward = read(b, object + 0x3c)?;
            q.point = transform(q.point, matrix(b, forward)?)?;
            q.best_distance_squared = local.best_distance_squared;
            let forward = read(b, object + 0x3c)?;
            q.normal = rotate(local.normal, matrix(b, forward)?, scaled)?;
            q.primitive = Some(object);
            Ok(DynamicSphereObjectResult::Hit)
        }
        0x41 => {
            let min = vec3(b, object + 0x24)?;
            let max = vec3(b, object + 0x30)?;
            if crate::collision_swept_aabox::sphere_in_aa_box(min, max, q)? {
                q.primitive = Some(object);
                Ok(DynamicSphereObjectResult::Hit)
            } else {
                Ok(DynamicSphereObjectResult::Miss)
            }
        }
        0x42 => sphere_with_ground_bias(b, object, center, radius, q),
        0x43 => cylinder(object, center, f(b, object + 0x20)?, q).map_err(Into::into),
        0x44 => sphere(object, center, radius, q).map_err(Into::into),
        0x47 => {
            if b.read_byte(manager + 0x2c0)
                .map_err(DynamicSphereError::Backend)?
                != 0
            {
                let owner = read(b, object + 0x44)?;
                if read(b, owner + 0x10)? == 0x1d {
                    actor_hint(b, object, q.delta)?;
                }
            }
            Ok(DynamicSphereObjectResult::Miss)
        }
        _ => Ok(DynamicSphereObjectResult::Miss),
    }
}

fn at_distance(q: &SphereParam, distance: f32) -> Result<[f32; 3], &'static str> {
    // Native Z product has its own stack spill before the final additions.
    let z = n(distance)?.mul(n(q.direction[2])?)?.store()?;
    Ok([
        n(distance)?
            .mul(n(q.direction[0])?)?
            .add(n(q.origin[0])?)?
            .store()?,
        n(distance)?
            .mul(n(q.direction[1])?)?
            .add(n(q.origin[1])?)?
            .store()?,
        n(z)?.add(n(q.origin[2])?)?.store()?,
    ])
}
fn sphere(
    object: u32,
    center: [f32; 3],
    radius: f32,
    q: &mut SphereParam,
) -> Result<DynamicSphereObjectResult, &'static str> {
    let relative = [
        n(center[0])?.sub(n(q.origin[0])?)?,
        n(center[1])?.sub(n(q.origin[1])?)?,
        n(center[2])?.sub(n(q.origin[2])?)?,
    ];
    let projection = relative[2]
        .mul(n(q.direction[2])?)?
        .add(relative[1].mul(n(q.direction[1])?)?)?
        .add(relative[0].mul(n(q.direction[0])?)?)?
        .store()?;
    if projection <= 0. {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    let sum = n(q.radius)?.add(n(radius)?)?;
    let square = relative[2]
        .mul(relative[2])?
        .add(relative[1].mul(relative[1])?)?
        .add(relative[0].mul(relative[0])?)?;
    let discriminant = sum
        .mul(sum)?
        .sub(square.sub(n(projection)?.mul(n(projection)?)?)?)?;
    if discriminant.value() < 0. {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    let distance = n(projection)?.sub(discriminant.sqrt()?)?.store()?;
    if distance < -q.radius {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    let squared = n(distance)?.mul(n(distance)?)?;
    if squared.value() >= f64::from(q.best_distance_squared) {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    q.point = at_distance(q, distance)?;
    q.normal = difference(center, q.point)?;
    q.best_distance_squared = squared.store()?;
    q.primitive = Some(object);
    Ok(DynamicSphereObjectResult::Hit)
}
fn sphere_with_ground_bias<B: VehicleInitBackend + ?Sized>(
    _b: &mut B,
    object: u32,
    center: [f32; 3],
    radius: f32,
    q: &mut SphereParam,
) -> Out<DynamicSphereObjectResult, B::Error> {
    let x = n(center[0])?.sub(n(q.origin[0])?)?.store()?;
    let y = n(center[1])?.sub(n(q.origin[1])?)?;
    let z = n(center[2])?.sub(n(q.origin[2])?)?.store()?;
    let zz = n(z)?.mul(n(z)?)?.store()?;
    let xx = n(x)?.mul(n(x)?)?.store()?;
    let squared = y.mul(y)?.add(n(xx)?)?.add(n(zz)?)?.store()?;
    let combined = n(q.radius)?.add(n(radius)?)?.store()?;
    let projected = y
        .mul(n(q.direction[1])?)?
        .add(n(z)?.mul(n(q.direction[2])?)?)?
        .add(n(x)?.mul(n(q.direction[0])?)?)?
        .store()?;
    if projected <= 0. {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    let combined_squared = n(combined)?.mul(n(combined)?)?.store()?;
    if squared < combined_squared {
        let horizontal = n(xx)?.add(n(zz)?)?.store()?;
        if (horizontal.to_bits() as i32) >= EPS.to_bits() as i32 {
            let length = if n(horizontal)?.sub(n(1.)?)?.value().abs() < f64::from(EPS) {
                n(1.)?
            } else {
                n(horizontal)?.sqrt()?
            };
            if length.value() >= f64::from(EPS) {
                let factor = n(combined)?.div(length)?;
                let point_x = n(center[0])?.sub(n(x)?.mul(factor)?)?.store()?;
                let point_z_product = n(z)?.mul(factor)?.store()?;
                q.point = [
                    point_x,
                    q.origin[1],
                    n(center[2])?.sub(n(point_z_product)?)?.store()?,
                ];
                q.normal = [x, f32::from_bits(0x3c23d70a), z];
                q.best_distance_squared = 0.;
                q.primitive = Some(object);
                return Ok(DynamicSphereObjectResult::Hit);
            }
        }
    }
    let discriminant =
        n(combined_squared)?.sub(n(squared)?.sub(n(projected)?.mul(n(projected)?)?)?)?;
    if discriminant.value() < 0. {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    let distance = n(projected)?.sub(discriminant.sqrt()?)?.store()?;
    if distance < -q.radius {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    let best = n(distance)?.mul(n(distance)?)?;
    if best.value() >= f64::from(q.best_distance_squared) {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    q.point = at_distance(q, distance)?;
    q.normal = difference(center, q.point)?;
    if q.normal[1] < 0. {
        q.normal[1] = 0.;
    }
    q.best_distance_squared = best.store()?;
    q.primitive = Some(object);
    Ok(DynamicSphereObjectResult::Hit)
}
fn cylinder(
    object: u32,
    center: [f32; 3],
    radius: f32,
    q: &mut SphereParam,
) -> Result<DynamicSphereObjectResult, &'static str> {
    let relative = [
        n(center[0])?.sub(n(q.origin[0])?)?.store()?,
        0.,
        n(center[2])?.sub(n(q.origin[2])?)?.store()?,
    ];
    let square = n(q.direction[2])?
        .mul(n(q.direction[2])?)?
        .add(n(q.direction[0])?.mul(n(q.direction[0])?)?)?
        .store()?;
    let horizontal_length = if (square.to_bits() as i32) < EPS.to_bits() as i32 {
        n(0.)?
    } else if n(square)?.sub(n(1.)?)?.value().abs() < f64::from(EPS) {
        n(1.)?
    } else {
        n(square)?.sqrt()?
    };
    if horizontal_length.value().abs() < f64::from(EPS) {
        return Ok(DynamicSphereObjectResult::Abort);
    }
    let inverse = n(1.)?.div(horizontal_length)?;
    let projection = n(relative[2])?
        .mul(n(q.direction[2])?.mul(inverse)?)?
        .add(n(q.direction[0])?.mul(inverse)?.mul(n(relative[0])?)?)?
        .add(inverse.mul(n(0.)?)?.mul(n(0.)?)?)?;
    if projection.value() <= 0. {
        return Ok(DynamicSphereObjectResult::Miss);
    }
    let sum = n(radius)?.add(n(q.radius)?)?;
    let discriminant = sum.mul(sum)?.sub(
        n(relative[2])?
            .mul(n(relative[2])?)?
            .add(n(relative[0])?.mul(n(relative[0])?)?)?
            .sub(projection.mul(projection)?)?,
    )?;
    if discriminant.value() <= 0. {
        return Ok(DynamicSphereObjectResult::Abort);
    }
    let distance = projection
        .sub(discriminant.sqrt()?)?
        .mul(inverse)?
        .store()?;
    if distance < -q.radius {
        return Ok(DynamicSphereObjectResult::Abort);
    }
    let best = n(distance)?.mul(n(distance)?)?;
    if best.value() >= f64::from(q.best_distance_squared) {
        return Ok(DynamicSphereObjectResult::Abort);
    }
    q.point = at_distance(q, distance)?;
    q.normal = difference(center, q.point)?;
    q.normal[1] = 0.;
    q.best_distance_squared = best.store()?;
    q.primitive = Some(object);
    Ok(DynamicSphereObjectResult::Hit)
}
fn actor_hint<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    object: u32,
    delta: [f32; 3],
) -> Out<(), B::Error> {
    let x = n(delta[0])?.mul(n(100.)?)?;
    let unchanged = read(b, object + 0x64)?;
    put(b, object + 0x64, unchanged)?;
    let x = x.store()?;
    let z = n(delta[2])?.mul(n(100.)?)?;
    let prior_x = f(b, object + 0x60)?;
    put(b, object + 0x60, n(x)?.add(n(prior_x)?)?.store()?.to_bits())?;
    let prior_z = f(b, object + 0x68)?;
    put(b, object + 0x68, z.add(n(prior_z)?)?.store()?.to_bits())?;
    for axis in [0x60, 0x68] {
        let magnitude = f(b, object + axis)?.abs();
        if magnitude > 5. {
            let ratio = n(5.)?.div(n(magnitude)?)?;
            for coordinate in [0x60, 0x68] {
                let value = f(b, object + coordinate)?;
                put(
                    b,
                    object + coordinate,
                    ratio.mul(n(value)?)?.store()?.to_bits(),
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision_swept::SphereSweepRequest,
        native_transform::SMatrix,
        vehicle_init::{
            VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery,
            VehicleInitUnavailable,
        },
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
        fn word(&mut self, a: u32, v: u32) {
            self.write_word(a, v).unwrap();
        }
        fn float(&mut self, a: u32, v: f32) {
            self.word(a, v.to_bits());
        }
        fn vector(&mut self, a: u32, v: [f32; 3]) {
            for (i, x) in v.into_iter().enumerate() {
                self.float(a + i as u32 * 4, x);
            }
        }
        fn matrix(&mut self, a: u32, m: [f32; 16]) {
            for (i, x) in m.into_iter().enumerate() {
                self.float(a + i as u32 * 4, x);
            }
        }
        fn fixture(kind: u32) -> Self {
            let mut b = Self::new();
            b.word(0x100 + 0x28c, 0x1000);
            b.vector(0x1000, [-10., -10., 20.]);
            b.float(0x100c, 20.);
            b.word(0x1010, 1);
            b.word(0x1014, 1);
            b.word(0x1018, 0x2000);
            b.word(0x2000, 0x2100);
            b.word(0x210c, 0x2200);
            b.word(0x2110, 0x2204);
            b.word(0x2200, 0x3000);
            b.word(0x3000, kind);
            b.vector(0x3010, [0.; 3]);
            b.float(0x301c, 2.);
            b.float(0x3020, 1.);
            b.vector(0x3024, [-1.; 3]);
            b.vector(0x3030, [1.; 3]);
            b.word(0x303c, 0x4000);
            b.word(0x3040, 0x4100);
            b.matrix(0x4000, SMatrix::identity().elements);
            b.matrix(0x4100, SMatrix::identity().elements);
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
            Err("unexpected allocate")
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            unreachable!()
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
    fn query() -> SphereParam {
        SphereParam::new(&SphereSweepRequest {
            origin: [0., 3., 0.],
            delta: [0., -4., 0.],
            radius: 0.25,
            selector: 2,
            reject_mask: 0,
        })
        .unwrap()
    }
    #[test]
    fn canonical_transformed_box_hits_original_local_geometry() {
        let mut b = Backend::fixture(0x40);
        let mut q = query();
        assert_eq!(sphere_in_dynamics(&mut b, 0x100, 2, &mut q), Ok(true));
        assert_eq!(q.primitive, Some(0x3000));
        assert_eq!(q.point, [0., 1.25, 0.]);
        assert_eq!(q.best_distance_squared, 3.0625);
        assert_eq!(q.normal, [0., -1., 0.]);
    }
    #[test]
    fn uniform_scaled_box_retains_native_local_distance_and_normal_column_division() {
        let mut b = Backend::fixture(0x46);
        let mut forward = SMatrix::identity().elements;
        let mut inverse = forward;
        for i in [0, 5, 10] {
            forward[i] = 2.;
            inverse[i] = 0.5;
        }
        b.matrix(0x4000, forward);
        b.matrix(0x4100, inverse);
        b.float(0x301c, 4.);
        let mut q = SphereParam::new(&SphereSweepRequest {
            origin: [0., 4., 0.],
            delta: [0., -4., 0.],
            radius: 0.5,
            selector: 2,
            reject_mask: 0,
        })
        .unwrap();
        assert_eq!(sphere_in_dynamics(&mut b, 0x100, 2, &mut q), Ok(true));
        assert_eq!(q.point, [0., 2.5, 0.]);
        assert_eq!(q.normal, [0., -1., 0.]);
        assert_eq!(q.best_distance_squared, 0.5625);
    }
    #[test]
    fn excluded_identity_precedes_any_candidate_field_read() {
        let mut b = Backend::fixture(0x40);
        b.word(0x2200, 0xffff_1000);
        b.word(0x100 + 0x2b8, 0x2500);
        b.word(0x100 + 0x2bc, 1);
        b.word(0x2500, 0xffff_1000);
        assert_eq!(
            sphere_in_dynamics(&mut b, 0x100, 2, &mut query()),
            Ok(false)
        );
    }
    #[test]
    fn original_cylinder_early_false_retains_an_earlier_box_hit() {
        let mut b = Backend::fixture(0x41);
        b.word(0x2110, 0x2208);
        b.word(0x2204, 0x3100);
        b.word(0x3100, 0x43);
        b.float(0x311c, 2.);
        b.float(0x3120, 1.);
        let mut q = query();
        assert_eq!(sphere_in_dynamics(&mut b, 0x100, 2, &mut q), Ok(false));
        assert_eq!(q.primitive, Some(0x3000));
        assert_eq!(q.point, [0., 1.25, 0.]);
    }
    #[test]
    fn sphere_inside_push_preserves_original_ground_normal_bias() {
        let mut b = Backend::fixture(0x42);
        b.float(0x301c, 1.);
        let mut q = SphereParam::new(&SphereSweepRequest {
            origin: [0.5, 0., 0.],
            delta: [-0.25, 0., 0.],
            radius: 0.5,
            selector: 2,
            reject_mask: 0,
        })
        .unwrap();
        assert_eq!(sphere_in_dynamics(&mut b, 0x100, 2, &mut q), Ok(true));
        assert_eq!(q.point, [1.5, 0., 0.]);
        assert_eq!(q.normal, [-0.5, f32::from_bits(0x3c23d70a), 0.]);
        assert_eq!(q.best_distance_squared, 0.);
    }
    #[test]
    fn actor_hint_side_effect_never_becomes_a_hit() {
        let mut b = Backend::fixture(0x47);
        b.float(0x301c, 100.);
        b.word(0x3044, 0x5000);
        b.word(0x5010, 0x1d);
        b.write_byte(0x100 + 0x2c0, 1).unwrap();
        b.word(0x3064, 0xdead_beef);
        let mut q = SphereParam::new(&SphereSweepRequest {
            origin: [0., 3., 0.],
            delta: [1., 0., 2.],
            radius: 0.25,
            selector: 2,
            reject_mask: 0,
        })
        .unwrap();
        assert_eq!(sphere_in_dynamics(&mut b, 0x100, 2, &mut q), Ok(false));
        assert_eq!(f(&mut b, 0x3060).unwrap(), 2.5);
        assert_eq!(f(&mut b, 0x3068).unwrap(), 5.);
        assert_eq!(read(&mut b, 0x3064).unwrap(), 0xdead_beef);
        assert_eq!(q.primitive, None);
    }
}
