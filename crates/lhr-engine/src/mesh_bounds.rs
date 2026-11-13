//! I3D_mesh_object::UpdateBoundVolume arithmetic, original PC RVA 0x31400,
//! plus BBoxVertices at PC RVA 0x6bc00 (mafia-re ls3df/IGraph/Globals.cpp:269).
//! Original PS2 symbol correspondence is independently recorded privately.
//! This represents PC24 round-to-nearest finite normal-or-zero intermediates;
//! extended-exponent, subnormal and nonfinite arithmetic remains unavailable.
//! Bbox comparisons retain native unordered skips and strict tie behavior.

use crate::native_matrix::{checked_f32_add, checked_f32_mul};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshBoundVolume {
    pub bbox: [[f32; 3]; 2],
    pub center: [f32; 3],
    pub radius: f32,
}
impl MeshBoundVolume {
    pub const CONSTRUCTOR: Self = Self {
        bbox: [
            [f32::from_bits(0x5a0e1bca); 3],
            [f32::from_bits(0xda0e1bca); 3],
        ],
        center: [0.; 3],
        radius: 0.,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshBoundsError {
    TooManyLevels,
    NumericDomain(&'static str),
}

/// Consume only the supplied active LOD prefix, in reverse order. Null levels
/// and boxes with invalid/unordered X extents are skipped; Y/Z validity is not
/// tested. No accepted levels still computes a sphere from the empty sentinels.
///
/// Host numeric guards preserve preceding projected stores: union bbox first,
/// then center, then radius. Center arithmetic is conservatively admitted before
/// all three center stores; outside this domain native center-Z conversion may
/// occur after X/Y stores. The original has no numeric-error return path. The
/// caller must mark a rejected volume unavailable, rather than use its new bbox
/// with an old sphere.
pub fn update_mesh_bounds(
    volume: &mut MeshBoundVolume,
    levels: &[Option<[[f32; 3]; 2]>],
) -> Result<(), MeshBoundsError> {
    if levels.len() > 10 {
        return Err(MeshBoundsError::TooManyLevels);
    }
    volume.bbox = MeshBoundVolume::CONSTRUCTOR.bbox;
    for bbox in levels.iter().rev().flatten() {
        if !matches!(
            bbox[0][0].partial_cmp(&bbox[1][0]),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ) {
            continue;
        }
        for (axis, (&min, &max)) in bbox[0].iter().zip(bbox[1].iter()).enumerate() {
            if volume.bbox[0][axis] > min {
                volume.bbox[0][axis] = min;
            }
            if volume.bbox[1][axis] < max {
                volume.bbox[1][axis] = max;
            }
        }
    }
    let mut half = [0.; 3];
    let mut center = [0.; 3];
    for (axis, value) in half.iter_mut().enumerate() {
        *value = checked_f32_add(volume.bbox[1][axis], -volume.bbox[0][axis])
            .and_then(|difference| checked_f32_mul(difference, 0.5))
            .map_err(|_| MeshBoundsError::NumericDomain("half extent"))?;
    }
    for (axis, value) in center.iter_mut().enumerate() {
        *value = checked_f32_add(volume.bbox[0][axis], half[axis])
            .map_err(|_| MeshBoundsError::NumericDomain("center"))?;
    }
    volume.center = center;
    let squared = (|| {
        checked_f32_add(
            checked_f32_add(
                checked_f32_mul(half[2], half[2])?,
                checked_f32_mul(half[1], half[1])?,
            )?,
            checked_f32_mul(half[0], half[0])?,
        )
    })()
    .map_err(|_| MeshBoundsError::NumericDomain("radius"))?;
    // Guarded nonnegative normal-or-zero squared length cannot under/overflow
    // in sqrt, and one f32 rounding agrees with the admitted PC24/RN domain.
    volume.radius = squared.sqrt();
    Ok(())
}

/// Original SetBoundVolume, PC RVA 0x5a730 (differential: Ghidra body 47 B,
/// 15 insn/3 BB; mafia-re I3d_visual.cpp:138-143). Order-exact: 10-dword
/// copy (bbox + bsphere) first, then clear WBOUND_VALID iff the saved flags
/// lack WORLD_SPACE (0x40). Pure copies, no arithmetic.
pub fn set_visual_bound_volume(
    target: &mut MeshBoundVolume,
    source: &MeshBoundVolume,
    flags: &mut u32,
) {
    target.bbox = source.bbox;
    target.center = source.center;
    target.radius = source.radius;
    if *flags & 0x40 == 0 {
        *flags &= !0x40000;
    }
}

/// Working name for the `I3D_visual::GetBoundVolume` lazy gate (mafia-re
/// ls3df/I3D/I3D.h:2771-2778). No LS3DF FUNCTION address exists for it: it
/// is a header inline compiled into LS3DF, and the reference marks no
/// Game.exe out-of-line copy either. When `VISF_BOUND_VALID` (0x20000,
/// I3D.h:2703) is set the stored local bound is returned untouched; otherwise
/// the caller's `update` runs the virtual `UpdateBoundProc` equivalent and
/// its fresh volume is returned. `MeshBoundVolume` is `Copy`, so the returned
/// value (not a member reference) is the observable behavior.
pub fn visual_bound_volume_or_update<E>(
    vis_flags: u32,
    stored: &MeshBoundVolume,
    update: impl FnOnce() -> Result<MeshBoundVolume, E>,
) -> Result<MeshBoundVolume, E> {
    if vis_flags & 0x20000 != 0 {
        return Ok(*stored);
    }
    update()
}

/// Working name for the `I3D_visual::GetWBoundVolume` lazy gate (mafia-re
/// ls3df/I3D/I3D.h:2762-2769). No LS3DF FUNCTION address exists for it, same
/// header-inline status as the local gate above. When `VISF_WBOUND_VALID`
/// (0x40000, I3D.h:2704) is set the stored world bound is returned untouched;
/// otherwise the caller's `update` runs the virtual `UpdateWBoundProc`
/// equivalent and its fresh volume is returned.
pub fn visual_world_bound_volume_or_update<E>(
    vis_flags: u32,
    stored: &MeshBoundVolume,
    update: impl FnOnce() -> Result<MeshBoundVolume, E>,
) -> Result<MeshBoundVolume, E> {
    if vis_flags & 0x40000 != 0 {
        return Ok(*stored);
    }
    update()
}

/// Original BBoxVertices, PC RVA 0x6bc00 (differential: REA formed body 19 BB
/// / 57 insn; mafia-re ls3df/IGraph/Globals.cpp:269-293). Exact 8-vertex slot
/// order: `for i in 0..2 { for j in 0..2 { push(i, j, min.z); push(i, j,
/// max.z); } }` where `i` selects `max.x`/`min.x` and `j` selects
/// `max.y`/`min.y`. Slot order is therefore `(max.x,max.y,min.z)`,
/// `(max.x,max.y,max.z)`, `(max.x,min.y,min.z)`, `(max.x,min.y,max.z)`,
/// `(min.x,max.y,min.z)`, `(min.x,max.y,max.z)`, `(min.x,min.y,min.z)`,
/// `(min.x,min.y,max.z)`. Pure copies, no arithmetic. The original returns a
/// pointer to a function-static `S_vector v[8]` (`GLOBAL 0x101c5230`); this
/// port returns the eight corners by value (a copy out, disclosed host
/// adaptation — no shared mutable static, thread-safe).
pub fn bbox_corner_vertices(bbox: [[f32; 3]; 2]) -> [[f32; 3]; 8] {
    let mut corners = [[0.0f32; 3]; 8];
    let mut slot = 0usize;
    for i in 0..2 {
        let x = if i == 0 { bbox[1][0] } else { bbox[0][0] };
        for j in 0..2 {
            let y = if j == 0 { bbox[1][1] } else { bbox[0][1] };
            corners[slot] = [x, y, bbox[0][2]];
            slot += 1;
            corners[slot] = [x, y, bbox[1][2]];
            slot += 1;
        }
    }
    corners
}

/// Per-vertex world transform backend for UpdateWBoundProc (PC RVA 0x5a510):
/// S_vector::operator* plus the UpdateWMatrixProc freshness gate stay with
/// the caller (backend contracts, same discipline as BBoxVertices).
pub trait VisualWorldTransform {
    type Error;
    fn transform_vertex(&mut self, vertex: [f32; 3]) -> Result<[f32; 3], Self::Error>;
}

/// Original UpdateWBoundProc world-box kernel, PC RVA 0x5a510 (differential:
/// Ghidra body 532 B, 171 insn/23 BB; mafia-re I3d_visual.cpp:114-134). The
/// caller performs the WBOUND set + world Invalidate first, then the lazy
/// ensure (virtual UpdateBoundProc slot) iff the local box is invalid
/// (local_valid=false), then the X-validity check. This kernel: ensure-first
/// ordering, X-validity (ordered Less|Equal, unordered NaN → invalid arm),
/// 8-corner transform + strict </> absorb, half/center/radius via the same
/// checked PC24 tail as update_mesh_bounds (bit-equal for identical inputs;
/// the final sqrt is host f32::sqrt, not x87 FSQRT — internal-consistency
/// claim only, not binary parity; disclosed). The &local view is the caller's post-ensure state by contract (a refresh
/// inside ensure lands on the caller's side; re-invoke to re-read). The
/// invalid arm zero-writes center+radius and keeps the invalidated bbox,
/// returning Ok (native continues). Numeric failure retains preceding stores
/// (same policy as update_mesh_bounds); BBoxVertices corner order is
/// backend-irrelevant (order-independent absorb).
pub fn update_visual_world_bound<T, E>(
    local: &MeshBoundVolume,
    local_valid: bool,
    world: &mut MeshBoundVolume,
    ensure_local: impl FnOnce() -> Result<(), E>,
    transform: &mut T,
) -> Result<(), MeshBoundsError>
where
    T: VisualWorldTransform<Error = E>,
    MeshBoundsError: From<E>,
{
    if !local_valid {
        ensure_local().map_err(MeshBoundsError::from)?;
    }
    if !matches!(
        local.bbox[0][0].partial_cmp(&local.bbox[1][0]),
        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
    ) {
        world.center = [0.; 3];
        world.radius = 0.;
        return Ok(());
    }
    let mut box_min = [f32::INFINITY; 3];
    let mut box_max = [f32::NEG_INFINITY; 3];
    for corner in 0..8 {
        let source = [
            local.bbox[corner & 1][0],
            local.bbox[(corner >> 1) & 1][1],
            local.bbox[(corner >> 2) & 1][2],
        ];
        let moved = transform
            .transform_vertex(source)
            .map_err(MeshBoundsError::from)?;
        for axis in 0..3 {
            if moved[axis] < box_min[axis] {
                box_min[axis] = moved[axis];
            }
            if moved[axis] > box_max[axis] {
                box_max[axis] = moved[axis];
            }
        }
    }
    world.bbox = [box_min, box_max];
    let mut half = [0.; 3];
    let mut center = [0.; 3];
    for (axis, value) in half.iter_mut().enumerate() {
        *value = checked_f32_add(world.bbox[1][axis], -world.bbox[0][axis])
            .and_then(|difference| checked_f32_mul(difference, 0.5))
            .map_err(|_| MeshBoundsError::NumericDomain("half extent"))?;
    }
    for (axis, value) in center.iter_mut().enumerate() {
        *value = checked_f32_add(world.bbox[0][axis], half[axis])
            .map_err(|_| MeshBoundsError::NumericDomain("center"))?;
    }
    world.center = center;
    let squared = (|| {
        checked_f32_add(
            checked_f32_add(
                checked_f32_mul(half[2], half[2])?,
                checked_f32_mul(half[1], half[1])?,
            )?,
            checked_f32_mul(half[0], half[0])?,
        )
    })()
    .map_err(|_| MeshBoundsError::NumericDomain("radius"))?;
    world.radius = squared.sqrt();
    Ok(())
}

/// LS3DF RVA 0x14dd0, VA 0x10014dd0. `I3D_bbox::Absorb` (mafia-re ab67b13
/// `ls3df/I3D/I3d_math.h:473`; PS2 `Absorb__8I3D_bboxFRC8S_vector`; the
/// out-of-line copy the linker kept lives in collision code). Grows the box
/// to hold the point with six strict `>`/`<` compares in min.x, max.x,
/// min.y, max.y, min.z, max.z order. Direct comparisons keep the native
/// NaN flow (NaN absorbs nothing). Pure copies, no arithmetic. Tier
/// contract (wave policy: no Ghidra body observed).
pub fn absorb_point_into_bbox(bbox: &mut [[f32; 3]; 2], point: [f32; 3]) {
    if bbox[0][0] > point[0] {
        bbox[0][0] = point[0];
    }
    if bbox[1][0] < point[0] {
        bbox[1][0] = point[0];
    }
    if bbox[0][1] > point[1] {
        bbox[0][1] = point[1];
    }
    if bbox[1][1] < point[1] {
        bbox[1][1] = point[1];
    }
    if bbox[0][2] > point[2] {
        bbox[0][2] = point[2];
    }
    if bbox[1][2] < point[2] {
        bbox[1][2] = point[2];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_lod_ties_keep_selected_zero_bits_and_x_only_validity() {
        let lower = [[0., -4., -1.], [0., 5., 1.]];
        let higher = [[-0., 3., -2.], [-0., -3., 2.]];
        let mut volume = MeshBoundVolume::CONSTRUCTOR;
        update_mesh_bounds(&mut volume, &[Some(lower), Some(higher)]).unwrap();
        assert_eq!(
            volume.bbox[0].map(f32::to_bits),
            [(-0_f32).to_bits(), (-4_f32).to_bits(), (-2_f32).to_bits()]
        );
        assert_eq!(
            volume.bbox[1].map(f32::to_bits),
            [(-0_f32).to_bits(), 5_f32.to_bits(), 2_f32.to_bits()]
        );
        assert_eq!(volume.center, [0., 0.5, 0.]);
        assert_eq!(volume.radius.to_bits(), 24.25_f32.sqrt().to_bits());
    }
    #[test]
    fn empty_update_computes_sentinel_sphere_and_invalid_x_skips_other_axes() {
        let mut volume = MeshBoundVolume::CONSTRUCTOR;
        let invalid = [
            [f32::NAN, f32::NEG_INFINITY, -f32::MAX],
            [0., f32::INFINITY, f32::MAX],
        ];
        update_mesh_bounds(&mut volume, &[None, Some(invalid)]).unwrap();
        assert_eq!(volume.bbox, MeshBoundVolume::CONSTRUCTOR.bbox);
        assert_eq!(volume.center, [0.; 3]);
        assert_eq!(volume.radius.to_bits(), 0x5a7623a4);
    }
    #[test]
    fn host_numeric_rejection_preserves_projected_bbox_and_center_stores() {
        let mut volume = MeshBoundVolume {
            center: [7., 8., 9.],
            radius: 12.,
            ..MeshBoundVolume::CONSTRUCTOR
        };
        let too_wide = [[-f32::MAX; 3], [f32::MAX; 3]];
        assert_eq!(
            update_mesh_bounds(&mut volume, &[Some(too_wide)]),
            Err(MeshBoundsError::NumericDomain("half extent"))
        );
        assert_eq!(volume.bbox, too_wide);
        assert_eq!(volume.center, [7., 8., 9.]);
        assert_eq!(volume.radius, 12.);
        let square_overflow = [[-1e20; 3], [1e20; 3]];
        assert_eq!(
            update_mesh_bounds(&mut volume, &[Some(square_overflow)]),
            Err(MeshBoundsError::NumericDomain("radius"))
        );
        assert_eq!(volume.bbox, square_overflow);
        assert_eq!(volume.center, [0.; 3]);
        assert_eq!(volume.radius, 12.);
    }
    #[test]
    fn radius_retains_original_z_y_x_association() {
        let mut volume = MeshBoundVolume::CONSTRUCTOR;
        let bbox = [
            [0xc11c3600, 0xc2078a30, 0xc2016260].map(f32::from_bits),
            [0x422204f0, 0x429a4b1c, 0x42c14ce8].map(f32::from_bits),
        ];
        update_mesh_bounds(&mut volume, &[Some(bbox)]).unwrap();
        assert_eq!(
            volume.center.map(f32::to_bits),
            [0x4175eee0, 0x41ad0c08, 0x42009bb8]
        );
        assert_eq!(volume.radius.to_bits(), 0x42b177d8);
    }

    struct IdentityTransform;
    impl VisualWorldTransform for IdentityTransform {
        type Error = MeshBoundsError;
        fn transform_vertex(&mut self, vertex: [f32; 3]) -> Result<[f32; 3], Self::Error> {
            Ok(vertex)
        }
    }
    struct OrderLog {
        calls: Vec<&'static str>,
    }
    impl VisualWorldTransform for OrderLog {
        type Error = MeshBoundsError;
        fn transform_vertex(&mut self, vertex: [f32; 3]) -> Result<[f32; 3], Self::Error> {
            self.calls.push("transform");
            Ok(vertex)
        }
    }
    #[test]
    fn world_bound_identity_matches_mesh_reference() {
        let local = MeshBoundVolume {
            bbox: [[-1., -2., -3.], [4., 5., 6.]],
            center: [0.; 3],
            radius: 0.,
        };
        let mut ensured = false;
        let mut world = MeshBoundVolume::CONSTRUCTOR;
        let mut backend = IdentityTransform;
        update_visual_world_bound(
            &local,
            true,
            &mut world,
            || {
                ensured = true;
                Ok(())
            },
            &mut backend,
        )
        .unwrap();
        assert!(!ensured);
        let mut reference = MeshBoundVolume::CONSTRUCTOR;
        update_mesh_bounds(&mut reference, &[Some(local.bbox)]).unwrap();
        assert_eq!(world.bbox, reference.bbox);
        assert_eq!(world.center, reference.center);
        assert_eq!(world.radius.to_bits(), reference.radius.to_bits());
    }
    #[test]
    fn world_bound_invalid_x_zeroes_sphere_and_ensures_first() {
        let local = MeshBoundVolume {
            bbox: [[f32::NAN, -2., -3.], [4., 5., 6.]],
            center: [9.; 3],
            radius: 9.,
        };
        let ensured = std::cell::Cell::new(false);
        let mut log = OrderLog { calls: Vec::new() };
        let mut world = MeshBoundVolume {
            bbox: [[7.; 3], [8.; 3]],
            center: [7.; 3],
            radius: 7.,
        };
        update_visual_world_bound(
            &local,
            false,
            &mut world,
            || {
                ensured.set(true);
                Ok::<_, MeshBoundsError>(())
            },
            &mut log,
        )
        .unwrap();
        // ensure runs before the invalid arm (native order); the arm keeps
        // the caller-invalidated bbox, zeroes the sphere, and never runs
        // the transform backend.
        assert!(ensured.get());
        assert!(log.calls.is_empty());
        assert_eq!(world.bbox, [[7.; 3], [8.; 3]]);
        assert_eq!(world.center, [0.; 3]);
        assert_eq!(world.radius, 0.);
    }
    #[test]
    fn set_bound_volume_store_order() {
        let source = MeshBoundVolume {
            bbox: [[-1., -2., -3.], [4., 5., 6.]],
            center: [1.5, 1.5, 1.5],
            radius: 6.25f32.sqrt(),
        };
        let mut target = MeshBoundVolume::CONSTRUCTOR;
        let mut flags = 0x40u32; // world space: WBOUND preserved
        set_visual_bound_volume(&mut target, &source, &mut flags);
        assert_eq!(target, source);
        let mut target = MeshBoundVolume::CONSTRUCTOR;
        let mut flags = 0u32; // local space: WBOUND cleared
        set_visual_bound_volume(&mut target, &source, &mut flags);
        assert_eq!(target, source);
        assert_eq!(flags & 0x40000, 0);
    }
    #[test]
    fn bbox_corners_follow_exact_slot_order() {
        let bbox = [[-1., -2., -3.], [4., 5., 6.]];
        assert_eq!(
            bbox_corner_vertices(bbox),
            [
                [4., 5., -3.],
                [4., 5., 6.],
                [4., -2., -3.],
                [4., -2., 6.],
                [-1., 5., -3.],
                [-1., 5., 6.],
                [-1., -2., -3.],
                [-1., -2., 6.],
            ]
        );
        // Negative-zero bit fidelity: i/j select whole words, no arithmetic.
        let signed = [[-0., -0., 1.], [0., 0., 2.]];
        let corners = bbox_corner_vertices(signed);
        assert_eq!(corners[0][0].to_bits(), 0_f32.to_bits());
        assert_eq!(corners[4][0].to_bits(), (-0_f32).to_bits());
        assert_eq!(corners[0][1].to_bits(), 0_f32.to_bits());
        assert_eq!(corners[2][1].to_bits(), (-0_f32).to_bits());
    }

    #[test]
    fn absorb_grows_each_axis_in_native_order() {
        let mut bbox = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        absorb_point_into_bbox(&mut bbox, [-1.0, 2.0, 0.5]);
        assert_eq!(bbox, [[-1.0, 0.0, 0.0], [1.0, 2.0, 1.0]]);
        absorb_point_into_bbox(&mut bbox, [0.5, 0.5, 0.5]);
        assert_eq!(bbox, [[-1.0, 0.0, 0.0], [1.0, 2.0, 1.0]]);
        let mut bbox = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        absorb_point_into_bbox(&mut bbox, [f32::NAN, f32::NAN, f32::NAN]);
        assert_eq!(bbox, [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]);
    }

    #[test]
    fn visual_bound_gate_returns_stored_volume_without_calling_update() {
        use std::cell::Cell;
        let stored = MeshBoundVolume {
            bbox: [[-1., -2., -3.], [4., 5., 6.]],
            center: [1.5, 1.5, 1.5],
            radius: 2.0,
        };
        let calls = Cell::new(0u32);
        let fresh = MeshBoundVolume::CONSTRUCTOR;
        let out: Result<MeshBoundVolume, &'static str> =
            visual_bound_volume_or_update(0x20000, &stored, || {
                calls.set(calls.get() + 1);
                Ok(fresh)
            });
        assert_eq!(out, Ok(stored));
        assert_eq!(calls.get(), 0);
        // Other validity bits do not satisfy the local gate.
        let out: Result<MeshBoundVolume, &'static str> =
            visual_bound_volume_or_update(0x40000, &stored, || {
                calls.set(calls.get() + 1);
                Ok(fresh)
            });
        assert_eq!(out, Ok(fresh));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn visual_world_bound_gate_returns_stored_volume_without_calling_update() {
        use std::cell::Cell;
        let stored = MeshBoundVolume {
            bbox: [[-1., -2., -3.], [4., 5., 6.]],
            center: [1.5, 1.5, 1.5],
            radius: 2.0,
        };
        let calls = Cell::new(0u32);
        let fresh = MeshBoundVolume::CONSTRUCTOR;
        let out: Result<MeshBoundVolume, &'static str> =
            visual_world_bound_volume_or_update(0x40000, &stored, || {
                calls.set(calls.get() + 1);
                Ok(fresh)
            });
        assert_eq!(out, Ok(stored));
        assert_eq!(calls.get(), 0);
        // The local-valid bit does not satisfy the world gate.
        let out: Result<MeshBoundVolume, &'static str> =
            visual_world_bound_volume_or_update(0x20000, &stored, || {
                calls.set(calls.get() + 1);
                Ok(fresh)
            });
        assert_eq!(out, Ok(fresh));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn visual_bound_gate_propagates_update_failure() {
        let stored = MeshBoundVolume::CONSTRUCTOR;
        let out: Result<MeshBoundVolume, &'static str> =
            visual_bound_volume_or_update(0, &stored, || Err("backend"));
        assert_eq!(out, Err("backend"));
        let out: Result<MeshBoundVolume, &'static str> =
            visual_world_bound_volume_or_update(0, &stored, || Err("backend"));
        assert_eq!(out, Err("backend"));
    }
}
