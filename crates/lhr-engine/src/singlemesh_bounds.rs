//! PC singlemesh bound-volume arithmetic from UpdateBVolumeProc (RVA0x54810).
//! The caller owns binding, counters, visual flags and ancestor invalidation.
//! Matrix callbacks preserve the original eager joint-cache evaluation order.
use crate::{
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::SMatrix,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SinglemeshBoundingBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl SinglemeshBoundingBox {
    /// Native empty-box sentinels, also used on the missing-mesh path.
    pub const EMPTY: Self = Self {
        min: [f32::from_bits(0x5a0e1bca); 3],
        max: [f32::from_bits(0xda0e1bca); 3],
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SinglemeshBoundVolume {
    pub bbox: SinglemeshBoundingBox,
    pub center: [f32; 3],
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinglemeshBoundsError<E> {
    JointMatrix(E),
    NumericDomain,
}

/// Compute the populated shared-definition path under PC24/RN arithmetic.
/// `initial` is the definition's base box, not a box inferred from vertices.
/// Entries follow the binding-table order. None denotes a null joint pointer;
/// invalid X extents (including NaN) skip the matrix callback. Equal X extents
/// are accepted, and Y/Z extent ordering is not checked by the original.
///
/// Each accepted entry calls `get_joint_matrix` once before transforming its
/// eight corners in the original BBoxVertices order. Thus callbacks may populate
/// ancestor caches used by subsequent entries. On error those effects persist,
/// but no partial volume is returned. This availability policy does not model
/// native partial stores outside the finite normal-f32-or-zero numeric domain.
pub fn calculate_singlemesh_bounds<E>(
    initial: SinglemeshBoundingBox,
    joint_boxes: &[Option<SinglemeshBoundingBox>],
    mut get_joint_matrix: impl FnMut(usize) -> Result<SMatrix, E>,
) -> Result<SinglemeshBoundVolume, SinglemeshBoundsError<E>> {
    let mut bbox = initial;
    for (index, joint_box) in joint_boxes.iter().enumerate() {
        let Some(joint_box) = joint_box else {
            continue;
        };
        // A negated partial-order test would hide the native unordered skip.
        if !matches!(
            joint_box.min[0].partial_cmp(&joint_box.max[0]),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ) {
            continue;
        }
        let matrix = get_joint_matrix(index).map_err(SinglemeshBoundsError::JointMatrix)?;
        // Original BBoxVertices, RVA0x6bc00: X and Y descend; Z ascends.
        for x in [joint_box.max[0], joint_box.min[0]] {
            for y in [joint_box.max[1], joint_box.min[1]] {
                for z in [joint_box.min[2], joint_box.max[2]] {
                    let point = matrix
                        .try_transform_point([x, y, z])
                        .map_err(|_| SinglemeshBoundsError::NumericDomain)?;
                    for (axis, value) in point.into_iter().enumerate() {
                        // Strict comparisons retain an existing signed zero
                        // on equality, unlike float min/max convenience APIs.
                        if value < bbox.min[axis] {
                            bbox.min[axis] = value;
                        }
                        if value > bbox.max[axis] {
                            bbox.max[axis] = value;
                        }
                    }
                }
            }
        }
    }
    let arithmetic = || {
        let mut half = [0.; 3];
        let mut center = [0.; 3];
        for axis in 0..3 {
            half[axis] = checked_f32_mul(checked_f32_add(bbox.max[axis], -bbox.min[axis])?, 0.5)?;
            center[axis] = checked_f32_add(bbox.min[axis], half[axis])?;
        }
        let radius_squared = checked_f32_add(
            checked_f32_add(
                checked_f32_mul(half[2], half[2])?,
                checked_f32_mul(half[1], half[1])?,
            )?,
            checked_f32_mul(half[0], half[0])?,
        )?;
        // A nonnegative normal-or-zero squared length has a representable
        // normal-or-zero square root; sqrt is rounded once under PC24/RN.
        Ok::<_, crate::native_matrix::MatrixArithmeticUnavailable>(SinglemeshBoundVolume {
            bbox,
            center,
            radius: radius_squared.sqrt(),
        })
    };
    arithmetic().map_err(|_| SinglemeshBoundsError::NumericDomain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_only_present_valid_x_boxes_in_table_order() {
        let valid = SinglemeshBoundingBox {
            min: [-1., 2., -3.],
            max: [1., -2., 3.],
        };
        let mut inverted = valid;
        inverted.min[0] = 2.;
        let mut unordered = valid;
        unordered.min[0] = f32::NAN;
        let mut flat = valid;
        flat.min[0] = 1.;
        let mut calls = Vec::new();
        let result = calculate_singlemesh_bounds(
            SinglemeshBoundingBox::EMPTY,
            &[
                None,
                Some(inverted),
                Some(valid),
                Some(unordered),
                Some(flat),
            ],
            |index| {
                calls.push(index);
                Ok::<_, ()>(SMatrix::identity())
            },
        )
        .unwrap();
        assert_eq!(calls, [2, 4]);
        assert_eq!(result.bbox.min, [-1., -2., -3.]);
        assert_eq!(result.bbox.max, [1., 2., 3.]);
        assert_eq!(result.center, [0.; 3]);
        assert_eq!(result.radius.to_bits(), 14_f32.sqrt().to_bits());
    }

    #[test]
    fn includes_base_box_and_preserves_equal_extrema_bits() {
        let bbox = SinglemeshBoundingBox {
            min: [-0., -2., -3.],
            max: [0., 2., 3.],
        };
        let point = SinglemeshBoundingBox {
            min: [0.; 3],
            max: [0.; 3],
        };
        let volume =
            calculate_singlemesh_bounds(bbox, &[Some(point)], |_| Ok::<_, ()>(SMatrix::identity()))
                .unwrap();
        assert_eq!(volume.bbox.min[0].to_bits(), (-0_f32).to_bits());
        assert_eq!(volume.bbox.max, bbox.max);
        assert_eq!(volume.radius.to_bits(), 13_f32.sqrt().to_bits());
    }

    #[test]
    fn sphere_preserves_native_rounding_order() {
        // Synthetic affine-box fixture: summing X/Y/Z instead changes the
        // radius by one ULP. These bounds include the definition's base box.
        let bbox = SinglemeshBoundingBox {
            min: [0xc11c3600, 0xc2078a30, 0xc2016260].map(f32::from_bits),
            max: [0x422204f0, 0x429a4b1c, 0x42c14ce8].map(f32::from_bits),
        };
        let volume = calculate_singlemesh_bounds(bbox, &[], |_| -> Result<_, ()> {
            panic!("no joint matrix is needed")
        })
        .unwrap();
        assert_eq!(
            volume.center.map(f32::to_bits),
            [0x4175eee0, 0x41ad0c08, 0x42009bb8]
        );
        assert_eq!(volume.radius.to_bits(), 0x42b177d8);
    }

    #[test]
    fn failures_keep_callback_order_and_reject_unportable_intermediates() {
        let point = SinglemeshBoundingBox {
            min: [1.; 3],
            max: [1.; 3],
        };
        let mut calls = Vec::new();
        assert_eq!(
            calculate_singlemesh_bounds(SinglemeshBoundingBox::EMPTY, &[Some(point); 3], |i| {
                calls.push(i);
                if i == 1 {
                    Err(7)
                } else {
                    Ok(SMatrix::identity())
                }
            }),
            Err(SinglemeshBoundsError::JointMatrix(7))
        );
        assert_eq!(calls, [0, 1]);
        let mut huge = SMatrix::identity();
        huge.elements[0] = f32::MAX;
        huge.elements[4] = f32::MAX;
        assert_eq!(
            calculate_singlemesh_bounds(SinglemeshBoundingBox::EMPTY, &[Some(point)], |_| {
                Ok::<_, ()>(huge)
            }),
            Err(SinglemeshBoundsError::NumericDomain)
        );
        let tiny = SinglemeshBoundingBox {
            min: [0.; 3],
            max: [f32::MIN_POSITIVE; 3],
        };
        assert_eq!(
            calculate_singlemesh_bounds(tiny, &[], |_| -> Result<_, ()> { unreachable!() }),
            Err(SinglemeshBoundsError::NumericDomain)
        );
    }
}
