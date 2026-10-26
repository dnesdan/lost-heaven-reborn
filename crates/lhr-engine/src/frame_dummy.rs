//! I3D_dummy's real constructor/local box in the canonical frame arena.
//! PC LS3DF e211c850... RVA1a1e0; Game08398317... helper546d10.
//! The Game box helper copies six DWORDs only: it does not normalize, update
//! transforms, change cache flags or write the base hierarchical bound volume.
use super::*;

impl ModelArena {
    pub fn create_dummy_frame(&mut self) -> Result<FrameKey, ModelError> {
        self.insert(
            I3dFrameState::dummy(),
            None,
            Vec::new(),
            Vec::new(),
            Some(crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR.bbox),
        )
    }
    pub fn dummy_bound_box(&self, key: FrameKey) -> Result<[[f32; 3]; 2], ModelError> {
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        self.frame(key)?
            .dummy_bounds
            .ok_or(ModelError::DummyFrameRequired(key))
    }
    /// Game546d10's six raw stores. Nonfinite bit patterns are retained just as
    /// ordinary file-loaded dummy bounds; consumers own their numeric domains.
    pub fn set_dummy_bound_box(
        &mut self,
        key: FrameKey,
        bounds: [[f32; 3]; 2],
    ) -> Result<(), ModelError> {
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        let index = self.frame_index(key)?;
        self.frames[index].dummy_bounds = Some(bounds);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn genuine_dummy_constructor_box_copy_and_last_release_share_one_owner() {
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        assert_eq!(arena.state(key).unwrap().kind, FrameKind::Dummy);
        assert_eq!(arena.state(key).unwrap().dummy_flags, 0);
        assert_eq!(
            arena.dummy_bound_box(key).unwrap(),
            crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR.bbox
        );
        assert_eq!(arena.frame(key).unwrap().references, 1);
        assert_eq!(arena.frame(key).unwrap().owner, None);
        let state = *arena.state(key).unwrap();
        let bounds = [[f32::from_bits(0x7fc01234), -0., -2.], [4., 5., 6.]];
        arena.set_dummy_bound_box(key, bounds).unwrap();
        assert_eq!(
            arena
                .dummy_bound_box(key)
                .unwrap()
                .map(|r| r.map(f32::to_bits)),
            bounds.map(|r| r.map(f32::to_bits))
        );
        assert_eq!(*arena.state(key).unwrap(), state);
        arena.release(key).unwrap();
        assert!(arena.frame(key).is_err());
        assert_eq!(arena.live_frame_count(), 0);
    }
}
