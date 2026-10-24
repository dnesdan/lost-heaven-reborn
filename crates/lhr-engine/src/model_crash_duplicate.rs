//! Pristine static model cache duplication, sharing the actual mesh owner.
use super::*;
impl ModelArena {
    pub(crate) fn clear_crash_frame_properties(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let index = self.frame_index(key)?;
        self.frames[index].properties = Vec::new();
        Ok(())
    }
    pub(crate) fn duplicate_pristine_crash_model(
        &mut self,
        destination: ModelKey,
        source: ModelKey,
    ) -> Result<(), ModelError> {
        let source_frames = self.model(source)?.frames.clone();
        let first = *source_frames
            .first()
            .ok_or(ModelError::ModelNotEmpty(source))?;
        let backing = self
            .frame(first)?
            .source
            .as_ref()
            .cloned()
            .ok_or(ModelError::VisualBoundsUnavailable(first))?;
        if source_frames.len() != backing.frames.len() {
            return Err(ModelError::ModelNotEmpty(source));
        }
        for (i, &key) in source_frames.iter().enumerate() {
            let frame = self.frame(key)?;
            if frame.source_index != Some(i)
                || !frame
                    .source
                    .as_ref()
                    .is_some_and(|other| Arc::ptr_eq(other, &backing))
            {
                return Err(ModelError::ModelNotEmpty(source));
            }
            let state = self.state(key)?;
            let original = &backing.frames[i];
            if state.local.elements[12..15]
                .iter()
                .zip(original.position)
                .any(|(a, b)| a.to_bits() != b.to_bits())
                || state
                    .scale
                    .iter()
                    .zip(original.scale)
                    .any(|(a, b)| a.to_bits() != b.to_bits())
                || state
                    .rotation
                    .wxyz
                    .iter()
                    .zip(original.rotation_wxyz)
                    .any(|(a, b)| a.to_bits() != b.to_bits())
                || frame.name_bytes != original.name_bytes
                || frame.properties != original.properties
                || state.flags & 1 == 0
            {
                return Err(ModelError::ModelNotEmpty(source));
            }
            let expected_parent = original
                .parent
                .map(|index| source_frames[index])
                .or(Some(source.root()));
            if self.links(key)?.parent != expected_parent {
                return Err(ModelError::ModelNotEmpty(source));
            }
            if !matches!(
                self.state(key)?.kind,
                FrameKind::Mesh
                    | FrameKind::Dummy
                    | FrameKind::Target
                    | FrameKind::Sector
                    | FrameKind::Model
            ) {
                return Err(ModelError::VisualBoundsUnavailable(key));
            }
        }
        self.close(destination)?;
        let content = self
            .load_parsed_into_model(destination, backing)
            .map_err(|e| e.error)?;
        self.copy_model_filename_reference(destination, source)?;
        for (&from, &to) in source_frames.iter().zip(content.frames()) {
            let src = self.frame_index(from)?;
            let dst = self.frame_index(to)?;
            if let Some(original) = self.frames[src].object_resources {
                let object = self
                    .visual_resource_owner_mut()?
                    .load_plain_instance(original)
                    .map_err(ModelError::LoadedVisual)?;
                self.frames[dst].object_resources = Some(object);
                self.frames[dst].visual_bounds = self.frames[src].visual_bounds;
                self.graph.discard_hr_bound_vol(to)?;
            }
        }
        Ok(())
    }
}
