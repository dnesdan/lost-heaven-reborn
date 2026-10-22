//! Owned PC singlemesh binding fields. DBuild/findJoint/GetVertexJoint names
//! have PS2 linker provenance; their PC algorithms differ from the PS2 bodies.
//! The graph owns the loaded-definition bounds path; rendering/resource
//! backends are separate. An unwritten pointer slot stays unavailable.
use crate::{frame_transform::FrameKey, singlemesh_bounds::SinglemeshBoundVolume};

/// PC SM_INFO vertex counts consumed by GetVertexJoint (RVA0x54dd0).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SinglemeshJointRange {
    pub num_verts: u32,
    pub num_blend: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_joint_boundaries_negative_inputs_and_wrapping_counts() {
        let binding = I3dSinglemeshBinding {
            mesh_present: true,
            joints: vec![Some(FrameKey(1)), Some(FrameKey(2)), None],
            ..Default::default()
        };
        let ranges = [
            SinglemeshJointRange {
                num_verts: 2,
                num_blend: 1,
            },
            SinglemeshJointRange {
                num_verts: 0,
                num_blend: 2,
            },
            SinglemeshJointRange {
                num_verts: 1,
                num_blend: 0,
            },
        ];
        for (vertex, expected) in [(-1, 1), (0, 1), (2, 1), (3, 2), (4, 2)] {
            assert_eq!(
                binding.get_vertex_joint(vertex, Some(&ranges)),
                Ok(Some(FrameKey(expected)))
            );
        }
        assert_eq!(
            binding.get_vertex_joint(5, Some(&ranges)),
            Err(SinglemeshBindingError::UninitializedJointSlot(2))
        );
        assert_eq!(binding.get_vertex_joint(6, Some(&ranges)), Ok(None));
        assert_eq!(binding.get_vertex_joint(0, None), Ok(None));
        assert_eq!(
            binding.get_vertex_joint(0, Some(&[])),
            Err(SinglemeshBindingError::DefinitionTooShort)
        );
        // Wrapped count MAX+2 is one; INT_MIN minus one becomes positive.
        let range = [SinglemeshJointRange {
            num_verts: u32::MAX,
            num_blend: 2,
        }];
        assert_eq!(
            binding.get_vertex_joint(0, Some(&range)),
            Ok(Some(FrameKey(1)))
        );
        assert_eq!(
            binding.get_vertex_joint(i32::MIN, Some(&range)),
            Err(SinglemeshBindingError::DefinitionTooShort)
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinglemeshBindingError {
    DefinitionTooShort,
    UninitializedJointSlot(usize),
}

/// Native false returns, or the precise boundary reached before DBuild's
/// unavailable UpdateBVolumeProc call. The final outcome is not native success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DBuildBindingOutcome {
    NoMesh,
    NoJoints,
    BoundVolumeUpdateRequired,
}

#[derive(Clone, Debug, PartialEq)]
pub struct I3dSinglemeshBinding {
    pub(crate) mesh_present: bool,
    // None denotes an unwritten allocation slot, not a native null pointer.
    pub(crate) joints: Vec<Option<FrameKey>>,
    pub(crate) bvol_joints_changed: u32,
    pub(crate) bound_volume: Option<SinglemeshBoundVolume>,
}

impl Default for I3dSinglemeshBinding {
    fn default() -> Self {
        Self {
            mesh_present: false,
            joints: Vec::new(),
            // Observed PC singlemesh constructor store at +0x228.
            bvol_joints_changed: (-400_i32) as u32,
            bound_volume: None,
        }
    }
}

impl I3dSinglemeshBinding {
    pub fn mesh_present(&self) -> bool {
        self.mesh_present
    }

    pub fn joints(&self) -> &[Option<FrameKey>] {
        &self.joints
    }

    pub fn bvol_joints_changed(&self) -> u32 {
        self.bvol_joints_changed
    }

    /// PC RVA0x54dd0, after the caller has selected a valid LOD slot. None
    /// represents either a null LOD or its null definition. Counts and vertex
    /// subtraction wrap at 32 bits before the native signed-negative test.
    /// Missing definition records and unwritten joint pointers are host errors.
    pub fn get_vertex_joint(
        &self,
        mut vertex: i32,
        lod_definition: Option<&[SinglemeshJointRange]>,
    ) -> Result<Option<FrameKey>, SinglemeshBindingError> {
        let Some(definition) = lod_definition else {
            return Ok(None);
        };
        for (index, joint) in self.joints.iter().enumerate() {
            let range = definition
                .get(index)
                .ok_or(SinglemeshBindingError::DefinitionTooShort)?;
            vertex = vertex.wrapping_sub(range.num_verts.wrapping_add(range.num_blend) as i32);
            if vertex < 0 {
                return joint
                    .map(Some)
                    .ok_or(SinglemeshBindingError::UninitializedJointSlot(index));
            }
        }
        Ok(None)
    }
}
