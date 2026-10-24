//! Mesh-level locks resolve the existing managed vertex token, never another buffer.
use super::*;
impl ManagedVisualResources {
    fn vertex_buffer_for_level(
        &self,
        level: MeshLevelKey,
    ) -> Result<Option<bm::VertexBufferKey>, ManagedError> {
        self.available()?;
        let descriptor = self.resources.mesh_level(level)?.vertex_buffer;
        if descriptor.graphics.is_none() {
            return Ok(None);
        }
        let Some(VertexBufferPointer::Managed(token)) = descriptor.buffer else {
            return Err(ManagedError::BindingMismatch);
        };
        self.vertex_tokens
            .iter()
            .find(|entry| entry.visual == token)
            .map(|entry| Some(entry.manager))
            .ok_or(ManagedError::BindingMismatch)
    }
    pub(crate) fn lock_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        flags: u32,
        backend: &mut B,
    ) -> Result<bool, ManagedOperationError<B::Error>> {
        let Some(buffer) = self.vertex_buffer_for_level(level)? else {
            return Ok(false);
        };
        manager_result(self.managers.lock_vertex_buffer(
            buffer,
            if flags & 1 != 0 { 0x10 } else { 0 },
            backend,
        ))?;
        Ok(true)
    }
    pub(crate) fn unlock_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        if let Some(buffer) = self.vertex_buffer_for_level(level)? {
            manager_result(self.managers.unlock_vertex_buffer(buffer, backend))?;
        }
        Ok(())
    }
}

impl ManagedVisualResources {
    pub(crate) fn lock_group_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<bool, ManagedOperationError<B::Error>> {
        self.available()?;
        let descriptor = self.resources.face_group(group)?.index_buffer;
        let Some(token) = descriptor.buffer else {
            return Ok(false);
        };
        let buffer = self
            .index_tokens
            .iter()
            .find(|entry| entry.visual == token)
            .map(|entry| entry.manager)
            .ok_or(ManagedError::BindingMismatch)?;
        manager_result(self.managers.lock_index_buffer(buffer, backend))?;
        Ok(true)
    }
}
