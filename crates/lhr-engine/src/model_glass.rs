//! The original driver's ordinary Object factory on the existing arena.
use super::*;
impl ModelArena {
    pub(crate) fn create_glass_object(&mut self) -> Result<FrameKey, ModelError> {
        let key = self.insert(I3dFrameState::mesh(), None, Vec::new(), Vec::new(), None)?;
        let object = match self
            .visual_resource_owner_mut()
            .and_then(|owner| owner.glass_new_object().map_err(ModelError::LoadedVisual))
        {
            Ok(object) => object,
            Err(error) => {
                // Only the common frame constructor completed. It has no
                // attached Object tail/resource; its actual base release is
                // sufficient before returning this host construction error.
                // The full Mesh release requires its Object tail and must
                // not run here. Unwind only the completed common constructor.
                self.graph.destroy_base(key)?;
                let index = self.frame_index(key)?;
                self.frames[index].references = 0;
                self.frames[index].alive = false;
                self.last_destroyed_frame = Some(key);
                return Err(error);
            }
        };
        let index = self.frame_index(key)?;
        self.frames[index].object_resources = Some(object);
        self.frames[index].visual_bounds = Some(crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR);
        Ok(key)
    }
}

impl ModelArena {
    pub(crate) fn cleanup_glass_resources(&mut self) -> Result<(), ModelError> {
        if let Some(owner) = self.loaded_visual_resources.as_mut() {
            owner
                .glass_cleanup_resources()
                .map_err(ModelError::LoadedVisual)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_object_tail_releases_the_completed_common_frame_prefix() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let before = arena.frames.iter().filter(|f| f.alive).count();
        let owner = arena.visual_resource_owner_mut().unwrap();
        owner.mark_glass_admission_unavailable_for_test();
        assert!(arena.create_glass_object().is_err());
        assert_eq!(arena.frames.iter().filter(|f| f.alive).count(), before);
        arena.release(model.root()).unwrap();
    }
}
