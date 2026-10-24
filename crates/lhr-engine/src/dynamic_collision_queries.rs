//! Original g_collision::LineInDynamics (PC 0x5c5040). Dynamic descriptors
//! remain canonical addresses; the shared query also carries static record tags.
//! The numeric profile is PC24/RN with finite normal-or-zero intermediates.
#[path = "dynamic_collision_box.rs"]
pub mod box_query;
#[path = "dynamic_collision_sphere.rs"]
pub mod sphere;

use crate::{
    collision_line::LineQuery,
    collision_volumes::{Volume, VolumeGeometry},
    vehicle_init::VehicleInitBackend,
};

#[derive(Debug, PartialEq)]
pub enum DynamicLineError<E> {
    Backend(E),
    NumericDomain,
    StorageRange,
    UnsupportedHook { callback: u32, object: u32 },
}
type Out<T, E> = Result<T, DynamicLineError<E>>;
fn read<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(DynamicLineError::Backend)
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
macro_rules! op {
    ($name:ident) => {
        fn $name<E>(a: f32, b: f32) -> Out<f32, E> {
            crate::collision_swept::$name(a, b).map_err(|_| DynamicLineError::NumericDomain)
        }
    };
}
op!(add);
op!(sub);
op!(mul);
op!(div);
fn sqrt<E>(x: f32) -> Out<f32, E> {
    crate::collision_swept::sqrt(x).map_err(|_| DynamicLineError::NumericDomain)
}
fn dot<E>(a: [f32; 3], b: [f32; 3], order: [usize; 3]) -> Out<f32, E> {
    add(
        add(
            mul(a[order[0]], b[order[0]])?,
            mul(a[order[1]], b[order[1]])?,
        )?,
        mul(a[order[2]], b[order[2]])?,
    )
}
fn integer<E>(x: f32) -> Out<i32, E> {
    if !x.is_finite() || !(-2147483648.0..2147483648.0).contains(&x) {
        return Err(DynamicLineError::NumericDomain);
    }
    let n = x.trunc() as i32;
    crate::native_precision::record_precision(n as f32 != x);
    Ok(n)
}
fn length<E>(v: [f32; 3]) -> Out<f32, E> {
    let s = dot(v, v, [0, 1, 2])?;
    if (s.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if sub(s, 1.)?.abs() < 1e-8 {
        Ok(1.)
    } else {
        sqrt(s)
    }
}

/// Visit every original cell/list entry, retaining mask-before-group order.
/// No alternate registry, deduplication, or candidate-as-hit shortcut is used.
pub fn line_in_dynamics<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    selector: u32,
    q: &mut LineQuery,
) -> Out<bool, B::Error> {
    if selector == u32::MAX {
        return Ok(false);
    }
    let grid = read(b, manager + 0x28c)?;
    if grid == 0 {
        return Ok(false);
    }
    let width = read(b, grid + 0x10)?;
    let height = read(b, grid + 0x14)?;
    if width == 0 || height == 0 || width.checked_mul(height).is_none_or(|n| n > 1 << 20) {
        return Err(DynamicLineError::StorageRange);
    }
    let mut begin = [0; 2];
    let mut end = [0; 2];
    for axis in 0..2 {
        let offset = axis as u32 * 4;
        let inverse = div(1., f(b, grid + 8 + offset)?)?;
        let minimum = f(b, grid + offset)?;
        begin[axis] = integer(sub(mul(sub(q.min_xz[axis], minimum)?, inverse)?, 1.)?)?.max(0);
        let hi = integer(add(mul(sub(q.max_xz[axis], minimum)?, inverse)?, 2.)?)? as u32;
        end[axis] = hi.min([width, height][axis]) as i32;
    }
    let mut hit = false;
    for z in begin[1]..end[1] {
        for x in begin[0]..end[0] {
            let table = read(b, grid + 0x18)?;
            let cell = read(b, table + (z as u32 * width + x as u32) * 4)?;
            if cell == 0 {
                continue;
            }
            let mut at = read(b, cell + 0xc)?;
            let end = read(b, cell + 0x10)?;
            if end < at || (end - at) % 4 != 0 || (end - at) / 4 > 1 << 20 {
                return Err(DynamicLineError::StorageRange);
            }
            while at < end {
                let object = read(b, at)?;
                at += 4;
                if read(b, object + 8)? & selector != 0 {
                    continue;
                }
                let count = read(b, manager + 0x2bc)?;
                if count > 1 << 20 {
                    return Err(DynamicLineError::StorageRange);
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
                if line_in_dynamic_object(b, manager, object, q)? {
                    hit = true
                }
            }
        }
    }
    Ok(hit)
}

/// One eligible descriptor, including its original bounding-sphere/XZ rejects.
/// Exposed separately for original-runtime caller capsules.
pub fn line_in_dynamic_object<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    object: u32,
    q: &mut LineQuery,
) -> Out<bool, B::Error> {
    let center = vec3(b, object + 0x10)?;
    let square = dot(q.delta, q.delta, [0, 1, 2])?;
    let distance = if square >= 1e-8 {
        let offset = [
            sub(center[0], q.origin[0])?,
            sub(center[1], q.origin[1])?,
            sub(center[2], q.origin[2])?,
        ];
        let t = div(dot(offset, q.delta, [0, 2, 1])?, square)?;
        let residual = [
            sub(add(q.origin[0], mul(t, q.delta[0])?)?, center[0])?,
            sub(add(q.origin[1], mul(t, q.delta[1])?)?, center[1])?,
            sub(add(q.origin[2], mul(t, q.delta[2])?)?, center[2])?,
        ];
        length(residual)?
    } else {
        -1e16
    };
    let radius = f(b, object + 0x1c)?;
    if distance > radius
        || q.min_xz[0] > add(radius, center[0])?
        || sub(center[0], radius)? > q.max_xz[0]
        || q.min_xz[1] > add(radius, center[2])?
        || sub(center[2], radius)? > q.max_xz[1]
    {
        return Ok(false);
    }
    let kind = read(b, object)? as u8;
    match kind {
        0x40 | 0x41 => {
            let min = vec3(b, object + 0x24)?;
            let max = vec3(b, object + 0x30)?;
            let geometry = if kind == 0x40 {
                let forward = read(b, object + 0x3c)?;
                let inverse = read(b, object + 0x40)?;
                VolumeGeometry::XtObb {
                    min: [0.; 3],
                    max: [0.; 3],
                    extent_min: min,
                    extent_max: max,
                    matrix: matrix(b, forward)?,
                    inverse: matrix(b, inverse)?,
                }
            } else {
                VolumeGeometry::Aabb { min, max }
            };
            // The shared leaf is original 0x5c5b20, with the same imported
            // mutating point transforms and RotateByMatrix operation order.
            let volume = Volume {
                properties: if kind == 0x40 { 0x80 } else { 0x81 },
                identity: object,
                geometry,
            };
            let mask = q.reject_mask;
            q.reject_mask = 0;
            let result = crate::collision_volumes::line_in_volume(&volume, q)
                .map_err(|_| DynamicLineError::NumericDomain);
            q.reject_mask = mask;
            result
        }
        0x42 | 0x44 => {
            let offset = [
                sub(center[0], q.origin[0])?,
                sub(center[1], q.origin[1])?,
                sub(center[2], q.origin[2])?,
            ];
            let projection = dot(offset, q.delta, [2, 1, 0])?;
            // tCLineParam+44 stores Z,Y,X squared displacement from TestLineV.
            let delta_square = dot(q.delta, q.delta, [2, 1, 0])?;
            let discriminant = sub(
                mul(projection, projection)?,
                mul(
                    sub(dot(offset, offset, [2, 1, 0])?, mul(radius, radius)?)?,
                    delta_square,
                )?,
            )?;
            if discriminant < 0. {
                return Ok(false);
            }
            let t = div(sub(projection, sqrt(discriminant)?)?, delta_square)?;
            if t < 0. || t >= q.best_fraction {
                return Ok(false);
            }
            for (axis, c) in center.into_iter().enumerate() {
                q.normal[axis] = sub(c, add(mul(t, q.delta[axis])?, q.origin[axis])?)?;
            }
            q.best_fraction = t;
            q.primitive = Some(object);
            Ok(true)
        }
        0x48 => {
            let callback = read(b, manager + 0x2c4)?;
            if callback != 0 {
                Err(DynamicLineError::UnsupportedHook { callback, object })
            } else {
                Ok(false)
            }
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision_query::*, dynamic_collision_manager::*, vehicle_frame_backend::VehicleFrameMemory,
    };
    const MANAGER: u32 = 0x1000;
    const FIRST: u32 = 0x3000;
    const SECOND: u32 = 0x3100;
    fn put(b: &mut VehicleFrameMemory, a: u32, x: f32) {
        b.write_word(a, x.to_bits()).unwrap()
    }
    fn setup() -> VehicleFrameMemory {
        let mut b = VehicleFrameMemory::default();
        b.map_bytes(MANAGER, &[0; 0x300]).unwrap();
        initialize_dynamic_grid(
            &mut b,
            MANAGER,
            DynamicCollisionGridSource {
                min_xz: [-20.; 2],
                max_xz: [20.; 2],
                desired_cell_size: 20.,
            },
        )
        .unwrap();
        b
    }
    fn object(b: &mut VehicleFrameMemory, a: u32, kind: u8, center: [f32; 3]) {
        b.map_bytes(a, &[0; 0x48]).unwrap();
        b.write_word(a, u32::from(kind)).unwrap();
        for (i, x) in center.into_iter().enumerate() {
            put(b, a + 0x10 + i as u32 * 4, x)
        }
        put(b, a + 0x1c, 2.);
        for i in 0..3 {
            put(b, a + 0x24 + i * 4, -1.);
            put(b, a + 0x30 + i * 4, 1.);
        }
        if kind == 0x40 {
            for (base, sign) in [(a + 0x80, 1.), (a + 0xc0, -1.)] {
                b.map_bytes(base, &[0; 64]).unwrap();
                let mut m = crate::native_transform::SMatrix::identity().elements;
                for i in 0..3 {
                    m[12 + i] = center[i] * sign;
                }
                for (i, x) in m.into_iter().enumerate() {
                    put(b, base + i as u32 * 4, x)
                }
            }
            b.write_word(a + 0x3c, a + 0x80).unwrap();
            b.write_word(a + 0x40, a + 0xc0).unwrap();
        }
        dyn_insert(b, MANAGER, a).unwrap();
    }
    fn query() -> LineQuery {
        LineQuery {
            origin: [-3., 0., 0.],
            delta: [6., 0., 0.],
            min_xz: [-3., 0.],
            max_xz: [3., 0.],
            reject_mask: 0x20,
            best_fraction: 1.,
            normal: [7., 8., 9.],
            primitive: None,
        }
    }
    #[test]
    fn canonical_cells_masks_groups_and_strict_nearest_keep_first_identity() {
        let mut b = setup();
        object(&mut b, FIRST, 0x41, [0.; 3]);
        object(&mut b, SECOND, 0x41, [0.; 3]);
        let mut q = query();
        assert_eq!(line_in_dynamics(&mut b, MANAGER, 2, &mut q), Ok(true));
        assert_eq!(q.primitive, Some(FIRST));
        assert_eq!(q.normal, [1., 0., 0.]);
        assert_eq!(q.best_fraction, 1. / 3.);
        b.write_word(FIRST + 8, 2).unwrap();
        let mut q = query();
        assert_eq!(line_in_dynamics(&mut b, MANAGER, 2, &mut q), Ok(true));
        assert_eq!(q.primitive, Some(SECOND));
        b.write_word(MANAGER + 0x2b4, SECOND).unwrap();
        b.write_word(MANAGER + 0x2b8, MANAGER + 0x2b4).unwrap();
        b.write_word(MANAGER + 0x2bc, 1).unwrap();
        let mut q = query();
        let before = q;
        assert_eq!(line_in_dynamics(&mut b, MANAGER, 2, &mut q), Ok(false));
        assert_eq!(q, before);
    }
    #[test]
    fn eligible_broadphase_candidate_miss_is_not_a_hit_and_obb_uses_real_inverse() {
        let mut b = setup();
        object(&mut b, FIRST, 0x40, [0.5, 0., 0.]);
        let mut q = query();
        assert_eq!(line_in_dynamics(&mut b, MANAGER, 2, &mut q), Ok(true));
        assert_eq!(q.best_fraction, 2.5 / 6.);
        assert_eq!(q.normal, [1., 0., 0.]);
        let mut q = query();
        q.origin[1] = 1.5;
        let before = q;
        assert_eq!(line_in_dynamics(&mut b, MANAGER, 2, &mut q), Ok(false));
        assert_eq!(q, before);
    }
    #[test]
    fn dynamic_then_static_uses_one_fraction_with_first_equal_tie() {
        let mut b = setup();
        object(&mut b, FIRST, 0x41, [0.; 3]);
        let r = CollisionLineRequest {
            origin: [-3., 0.25, 0.25],
            delta: [6., 0., 0.],
            selector: 2,
            reject_mask: 0,
        };
        for (x, expected) in [(-1., FIRST), (-2., 91)] {
            let (q, min_y) = crate::collision_query::prepare_line_query(&r).unwrap();
            let mut q = q;
            line_in_dynamics(&mut b, MANAGER, 2, &mut q).unwrap();
            let t = crate::collision_line::Triangle {
                properties: 1,
                vertices: [[x, 0., 0.], [x, 0., 2.], [x, 2., 0.]],
                plane: [1., 0., 0., -x],
                identity: 91,
            };
            let cells = [CollisionQueryCell {
                max_y: 2.,
                triangles: std::slice::from_ref(&t),
                identity: 5,
            }];
            let grid = CollisionQueryGrid {
                origin_xz: [-10.; 2],
                cell_size_xz: [20.; 2],
                width: 1,
                height: 1,
                cells: &cells,
                dynamic_candidates_present: true,
            };
            let mut out = CollisionLineOutputs {
                point: Some([99.; 3]),
                normal: Some([99.; 3]),
            };
            let mut traversal = CollisionTraversalState::default();
            let hit = crate::collision_query::test_line_v_grid_continue(
                &grid,
                &r,
                &mut out,
                &mut traversal,
                q,
                min_y,
            )
            .unwrap()
            .unwrap();
            assert_eq!(hit.primitive, expected);
            assert_eq!(out.normal, Some([-1., -0., -0.]));
            assert_eq!(out.point, Some([x, 0.25, 0.25]));
        }
    }
}
