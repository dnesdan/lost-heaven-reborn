//! Owned projection of PC I3D_joint fields used by constructor, destructor,
//! LoadImage and UpdateV and GetJointMatrix (LS3DF RVAs 0x1ed30, 0x1ede0,
//! 0x1ef10, 0x1ee30 and 0x56ba0). The class/method names
//! have PS2 symbol provenance; the PC member labels are descriptive. This is
//! neither the PS2 layout nor a deformation backend. The owned frame graph
//! provides the matrix cache and singlemesh binding operations.
use crate::{
    frame_transform::{FrameError, FrameKey, FrameKind},
    model_loading::{ModelArena, ModelError},
    native_transform::SMatrix,
};

/// JOINTF_LOADED 0x1 (I3D.h:2033): bone_matrix and index came from the file.
pub const JOINTF_LOADED: u32 = 0x1;

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

impl ModelArena {
    /// Original I3D_joint::~I3D_joint, PC RVA 0x1ede0 (already ledger-credited,
    /// frozen port `LS3DF bounded base, dummy and unbound Joint destruction`:
    /// NO overlay entry here; probe-verified formed Ghidra body, confirm-only;
    /// mafia-re ls3df/I3D/frames/I3d_joint.cpp:24). Derived-only unlink: the index is
    /// read once before the stores; when a singlemesh is linked, the indexed
    /// slot is cleared and a live occupant's backlink is cleared with it. A
    /// vacant slot clears idempotently; an unlinked joint is a no-op. Missing
    /// bindings and out-of-range indices are explicit unavailability (native
    /// dereferences); hierarchy and base retirement remain caller-owned.
    pub fn destroy_joint(&mut self, key: FrameKey) -> Result<(), ModelError> {
        if self.state(key)?.kind != FrameKind::Joint {
            return Err(ModelError::Frame(FrameError::NotJoint(key)));
        }
        let (singlemesh, index) = {
            let joint = self.graph.joint_state(key)?;
            (joint.singlemesh, joint.index)
        };
        let Some(target) = singlemesh else {
            return Ok(());
        };
        let previous = self.graph.clear_singlemesh_joint_slot(target, index)?;
        if let Some(occupant) = previous {
            if occupant != key {
                self.graph.assign_joint_singlemesh_target(occupant, None)?;
            }
        }
        // Host idempotency: the retiring storage is discarded by the base
        // tail, but a later derived re-entry (release_inner) must observe an
        // unlinked joint rather than a dangling slot. Native needs no store.
        self.graph.assign_joint_singlemesh_target(key, None)?;
        Ok(())
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
    use crate::frame_transform::I3dFrameState;

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

    #[test]
    fn destructor_clears_indexed_slot_and_evicts_occupant_backlink() {
        // PC RVA 0x1ede0 (mafia-re I3d_joint.cpp:24): index read once.
        let mut arena = ModelArena::default();
        let mesh = arena
            .insert(I3dFrameState::mesh(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let joint = arena
            .insert(I3dFrameState::joint(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        let other = arena
            .insert(I3dFrameState::joint(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        // Unlinked joint is a no-op that preserves liveness.
        arena.destroy_joint(joint).unwrap();
        assert!(arena.frame(joint).is_ok());
        // Non-joint frames are rejected before any store.
        assert!(matches!(
            arena.destroy_joint(mesh),
            Err(ModelError::Frame(FrameError::NotJoint(_)))
        ));
        // Linked pair via DBuild: each slot holds its own joint.
        let mut single_state = I3dFrameState::mesh();
        single_state.kind = FrameKind::Singlemesh;
        let single = arena
            .insert(single_state, None, Vec::new(), Vec::new(), None)
            .unwrap();
        arena
            .graph
            .apply_joint_load_image_fields(joint, SMatrix::identity(), 0)
            .unwrap();
        arena
            .graph
            .apply_joint_load_image_fields(other, SMatrix::identity(), 1)
            .unwrap();
        arena.link_frame(joint, Some(single), 0).unwrap();
        arena.link_frame(other, Some(single), 0).unwrap();
        arena.graph.d_build_binding(single).unwrap();
        assert_eq!(
            arena.graph.singlemesh_binding(single).unwrap().joints(),
            &[Some(joint), Some(other)]
        );
        // Retiring the first joint clears its own slot only.
        arena.destroy_joint(joint).unwrap();
        assert_eq!(
            arena.graph.singlemesh_binding(single).unwrap().joints(),
            &[None, Some(other)]
        );
        assert_eq!(
            arena.graph.joint_state(other).unwrap().singlemesh,
            Some(single)
        );
        // Mismatched index evicts the occupant: point joint at slot 1.
        arena
            .graph
            .apply_joint_load_image_fields(joint, SMatrix::identity(), 1)
            .unwrap();
        arena
            .graph
            .assign_joint_singlemesh_target(joint, Some(single))
            .unwrap();
        arena.destroy_joint(joint).unwrap();
        assert_eq!(
            arena.graph.singlemesh_binding(single).unwrap().joints(),
            &[None, None]
        );
        assert_eq!(arena.graph.joint_state(other).unwrap().singlemesh, None);
        // Vacant slot clears idempotently.
        arena.destroy_joint(joint).unwrap();
        assert_eq!(
            arena.graph.singlemesh_binding(single).unwrap().joints(),
            &[None, None]
        );
        // Bare visual frames have no visual-teardown path in this projection:
        // plain release rejects their last reference (UnsupportedDestruction,
        // same guard as model_loading.rs:5375), so only the joints retire and
        // the visuals keep their creator holds.
        arena.release(joint).unwrap();
        arena.release(other).unwrap();
        assert_eq!(arena.frame(mesh).unwrap().references, 1);
        assert_eq!(arena.frame(single).unwrap().references, 1);
    }
}
