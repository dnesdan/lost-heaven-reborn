//! I3D_mesh_object::UpdateBoundVolume arithmetic, original PC RVA 0x31400.
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
}
