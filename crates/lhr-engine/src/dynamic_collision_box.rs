//! Original g_collision::BoxInDynamic, PC 0x5c4230. All grid membership,
//! descriptor words and excluded owners are borrowed from the canonical heap.
use crate::{
    collision_swept_box::{box_dynamic_overlap, BoxDynamicGeometry, BoxParam},
    ph_thing_response::PhThingNumber,
    vehicle_init::VehicleInitBackend,
};

#[derive(Clone, Debug, PartialEq)]
pub enum DynamicBoxError<E> {
    Backend(E),
    NumericDomain,
    StorageRange,
}
type Out<T, E> = Result<T, DynamicBoxError<E>>;
fn read<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(DynamicBoxError::Backend)
}
fn float<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    Ok(f32::from_bits(read(b, a)?))
}
fn vector<B: VehicleInitBackend + ?Sized>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([float(b, a)?, float(b, a + 4)?, float(b, a + 8)?])
}
fn n<E>(v: f32) -> Out<PhThingNumber, E> {
    PhThingNumber::load(v).map_err(|_| DynamicBoxError::NumericDomain)
}
fn math<T, E>(v: Result<T, &'static str>) -> Out<T, E> {
    v.map_err(|_| DynamicBoxError::NumericDomain)
}
fn integer<E>(v: PhThingNumber) -> Out<i32, E> {
    let v = v.value();
    if !(-2147483648.0..2147483648.0).contains(&v) {
        return Err(DynamicBoxError::NumericDomain);
    }
    let result = v.trunc() as i32;
    crate::native_precision::record_precision(f64::from(result) != v);
    Ok(result)
}
/// The entry itself always examines the dynamic mask. Its TestDBox caller
/// suppresses this call entirely when that mask is UINT_MAX.
pub fn box_in_dynamics<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    manager: u32,
    p: &BoxParam,
) -> Out<bool, B::Error> {
    let grid = read(b, manager + 0x28c)?;
    if grid == 0 {
        return Ok(false);
    }
    let width = read(b, grid + 0x10)?;
    let height = read(b, grid + 0x14)?;
    if width == 0 || height == 0 || width.checked_mul(height).is_none_or(|x| x > 1 << 20) {
        return Err(DynamicBoxError::StorageRange);
    }
    let mut begin = [0; 2];
    let mut end = [0; 2];
    for axis in 0..2 {
        let offset = axis as u32 * 4;
        let inverse = math(n(1.)?.div(n(float(b, grid + 8 + offset)?)?))?;
        let minimum = n(float(b, grid + offset)?)?;
        let component = axis * 2;
        let lo =
            math(math(math(n(p.world_min[component])?.sub(minimum))?.mul(inverse))?.sub(n(1.)?))?;
        begin[axis] = integer(lo)?.max(0);
        let hi =
            math(math(math(n(p.world_max[component])?.sub(minimum))?.mul(inverse))?.add(n(2.)?))?;
        end[axis] = (integer(hi)? as u32).min([width, height][axis]) as i32;
    }
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
                return Err(DynamicBoxError::StorageRange);
            }
            while at < end {
                let object = read(b, at)?;
                at += 4;
                if read(b, object + 8)? & p.dynamic_mask != 0 {
                    continue;
                }
                let count = read(b, manager + 0x2bc)?;
                if count > 1 << 20 {
                    return Err(DynamicBoxError::StorageRange);
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
                let geometry = match read(b, object)? as u8 {
                    0x40 | 0x42 | 0x44 => BoxDynamicGeometry::SphereBound {
                        center: vector(b, object + 0x10)?,
                        radius: float(b, object + 0x1c)?,
                    },
                    0x41 => BoxDynamicGeometry::Aabb {
                        min: vector(b, object + 0x24)?,
                        max: vector(b, object + 0x30)?,
                    },
                    _ => continue,
                };
                if box_dynamic_overlap(p, geometry).map_err(|_| DynamicBoxError::NumericDomain)? {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dynamic_collision_manager::{
            dyn_insert, initialize_dynamic_grid, DynamicCollisionGridSource,
        },
        vehicle_frame_backend::VehicleFrameMemory,
    };
    const MANAGER: u32 = 0x1000;
    const OBJECT: u32 = 0x3000;
    fn put(b: &mut VehicleFrameMemory, a: u32, v: f32) {
        b.write_word(a, v.to_bits()).unwrap()
    }
    fn setup(kind: u8) -> VehicleFrameMemory {
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
        b.map_bytes(OBJECT, &[0; 0x48]).unwrap();
        b.write_word(OBJECT, u32::from(kind)).unwrap();
        put(&mut b, OBJECT + 0x1c, 1.);
        for axis in 0..3 {
            put(&mut b, OBJECT + 0x24 + axis * 4, -1.);
            put(&mut b, OBJECT + 0x30 + axis * 4, 1.);
        }
        dyn_insert(&mut b, MANAGER, OBJECT).unwrap();
        b
    }
    fn param() -> BoxParam {
        BoxParam {
            world_min: [-1.; 3],
            world_max: [1.; 3],
            center: [2., 0., 0.],
            extended_center: [0.; 3],
            half_diagonal_squared: 1.,
            extended_diagonal_squared: 1.,
            dynamic_mask: 2,
            static_mask: 0,
        }
    }
    #[test]
    fn canonical_sphere_bound_keeps_native_tangent_and_uintmax_entry_semantics() {
        for kind in [0x40, 0x42, 0x44] {
            let mut b = setup(kind);
            let mut p = param();
            assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(true));
            p.center[0] = f32::from_bits(2_f32.to_bits() + 1);
            assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(false));
            p.center[0] = -2.;
            p.dynamic_mask = u32::MAX;
            assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(true));
        }
    }
    #[test]
    fn mask_and_owner_group_precede_descriptor_geometry_and_unknown_types_skip() {
        let mut b = setup(0x40);
        put(&mut b, OBJECT + 0x10, f32::NAN);
        b.write_word(OBJECT + 8, 2).unwrap();
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &param()), Ok(false));
        b.write_word(OBJECT + 8, 0).unwrap();
        b.write_word(MANAGER + 0x2bc, 1).unwrap();
        b.write_word(MANAGER + 0x2b8, MANAGER + 0x2b4).unwrap();
        b.write_word(MANAGER + 0x2b4, OBJECT).unwrap();
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &param()), Ok(false));
        b.write_word(MANAGER + 0x2bc, 0).unwrap();
        b.write_word(OBJECT, 0x43).unwrap();
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &param()), Ok(false));
        b.write_word(OBJECT, 0x40).unwrap();
        assert_eq!(
            box_in_dynamics(&mut b, MANAGER, &param()),
            Err(DynamicBoxError::NumericDomain)
        );
    }
    #[test]
    fn aabb_inclusive_axis_checks_and_null_grid_leave_canonical_memory_unchanged() {
        let mut b = setup(0x41);
        let mut p = param();
        p.world_min = [1.; 3];
        p.world_max = [2.; 3];
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(true));
        p.world_min[2] = f32::from_bits(1_f32.to_bits() + 1);
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(false));
        let before: Vec<_> = (0..0x48)
            .map(|i| b.read_byte(OBJECT + i).unwrap())
            .collect();
        b.write_word(MANAGER + 0x28c, 0).unwrap();
        p.center = [f32::NAN; 3];
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(false));
        assert_eq!(
            before,
            (0..0x48)
                .map(|i| b.read_byte(OBJECT + i).unwrap())
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn retained_square_comparison_accepts_finite_tiny_inputs_without_a_false_float_spill() {
        let mut b = setup(0x42);
        put(&mut b, OBJECT + 0x1c, 2_f32.powi(-100));
        let mut p = param();
        p.center = [2_f32.powi(-100), 0., 0.];
        p.half_diagonal_squared = 0.;
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(true));
        p.center[0] = f32::from_bits(p.center[0].to_bits() + 1);
        assert_eq!(box_in_dynamics(&mut b, MANAGER, &p), Ok(false));
    }
}
