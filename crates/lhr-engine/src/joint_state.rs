//! Owned projection of PC I3D_joint fields used by constructor, LoadImage and
//! UpdateV (LS3DF RVAs 0x1ed30, 0x1ef10 and 0x1ee30). The class/method names
//! have PS2 symbol provenance; the PC member labels are descriptive. This is
//! neither the PS2 layout nor a singlemesh binding or deformation backend.
use crate::{frame_transform::FrameKey, native_transform::SMatrix};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct I3dJointState {
    /// PC +0x140: the matrix read verbatim from the joint image.
    pub bone_matrix: SMatrix,
    /// PC +0x180: retained without assuming a dense or unique joint index.
    pub index: u32,
    /// PC +0x184: constructor copy; LoadImage does not replace this matrix.
    pub matrix_184: SMatrix,
    /// PC +0x1c4 is not initialized by the constructor. None is a host
    /// availability marker, never a substitute identity joint matrix.
    pub joint_matrix: Option<SMatrix>,
    pub flags: u32,
    /// Stable frame identity replaces the native pointer at +0x208.
    pub singlemesh: Option<FrameKey>,
}

impl Default for I3dJointState {
    fn default() -> Self {
        let mut bone_matrix = SMatrix::identity();
        for index in [0, 5, 10] {
            bone_matrix.elements[index] = f32::from_bits(0x3c23d70a);
        }
        Self {
            bone_matrix,
            index: 0,
            matrix_184: bone_matrix,
            joint_matrix: None,
            flags: 0,
            singlemesh: None,
        }
    }
}

impl I3dJointState {
    /// Derived-field stores after a complete common record and 68-byte joint
    /// payload. Raw float bits are preserved; no inverse or normalization runs.
    pub fn apply_load_image_fields(&mut self, bone_matrix: SMatrix, index: u32) {
        self.bone_matrix = bone_matrix;
        self.index = index;
        self.flags = 1;
    }

    /// Perform the joint's own UpdateV stores and return the target of the
    /// remaining singlemesh invalidations, if that path is taken.
    pub(crate) fn invalidate_for_update(&mut self, frame_flags: u32) -> Option<FrameKey> {
        if frame_flags & 0x40002000 == 0 {
            return None;
        }
        self.flags &= !0x1c;
        if frame_flags & 0x40000000 == 0 {
            return None;
        }
        self.singlemesh
    }
}

/// Only the counter consumed by Joint::UpdateV; common frame and visual flags
/// live in the frame graph. This projection supplies no singlemesh callbacks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SinglemeshInvalidationState {
    pub joints_changed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_leaves_joint_cache_unavailable_and_load_preserves_other_fields() {
        let mut state = I3dJointState::default();
        assert_eq!(state.bone_matrix.elements[0].to_bits(), 0x3c23d70a);
        assert_eq!(state.bone_matrix.elements[5].to_bits(), 0x3c23d70a);
        assert_eq!(state.bone_matrix.elements[10].to_bits(), 0x3c23d70a);
        assert_eq!(state.bone_matrix.elements[15], 1.);
        assert_eq!(state.joint_matrix, None);
        let initial_copy = state.matrix_184;
        let mut image = SMatrix::identity();
        image.elements[3] = f32::from_bits(0x7fc01234);
        image.elements[12] = -0.;
        state.singlemesh = Some(FrameKey(7));
        state.flags = u32::MAX;
        state.apply_load_image_fields(image, u32::MAX);
        assert_eq!(state.bone_matrix.elements[3].to_bits(), 0x7fc01234);
        assert_eq!(state.bone_matrix.elements[12].to_bits(), 0x80000000);
        assert_eq!(state.index, u32::MAX);
        assert_eq!(state.flags, 1);
        assert_eq!(state.matrix_184, initial_copy);
        assert_eq!(state.joint_matrix, None);
        assert_eq!(state.singlemesh, Some(FrameKey(7)));
    }

    #[test]
    fn changed_only_invalidates_joint_while_update_requests_singlemesh_work() {
        for (frame_flags, flags, target) in [
            (0, u32::MAX, None),
            (0x2000, 0xffffffe3, None),
            (0x40000000, 0xffffffe3, Some(FrameKey(2))),
            (0x40002000, 0xffffffe3, Some(FrameKey(2))),
        ] {
            let mut state = I3dJointState {
                flags: u32::MAX,
                singlemesh: Some(FrameKey(2)),
                ..Default::default()
            };
            assert_eq!(state.invalidate_for_update(frame_flags), target);
            assert_eq!(state.flags, flags);
        }
    }
}
