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
            VisualResources::mesh_level_lock_flags(level.slot(), flags),
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
    /// UnlockFaces graphics half: resolves the group's managed index token like
    /// `lock_group_indices` and unlocks exactly that buffer. Bufferless CPU-only
    /// groups are a clean no-op (no native Unlock target exists for them).
    pub(crate) fn unlock_group_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.available()?;
        let descriptor = self.resources.face_group(group)?.index_buffer;
        let Some(token) = descriptor.buffer else {
            return Ok(());
        };
        let buffer = self
            .index_tokens
            .iter()
            .find(|entry| entry.visual == token)
            .map(|entry| entry.manager)
            .ok_or(ManagedError::BindingMismatch)?;
        manager_result(self.managers.unlock_index_buffer(buffer, backend))
    }
    /// GetFaces pairing: null-check first (native returns NULL before any Lock),
    /// then the existing graphics lock, then the live CPU array. A graphics-lock
    /// failure propagates before the record flag is set (native Lock failed → no pointer).
    pub fn get_faces<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<Option<&[vr::I3dTriface]>, ManagedOperationError<B::Error>> {
        self.available()?;
        if self.resources.face_group(group)?.num_faces == 0 {
            return Ok(None);
        }
        self.lock_group_indices(group, backend)?;
        self.resources.get_faces(group).map_err(Into::into)
    }
    /// UnlockFaces pairing: graphics unlock first so a failure leaves the CPU flag
    /// set consistently with the still-held graphics lock.
    pub fn unlock_faces<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.available()?;
        self.unlock_group_indices(group, backend)?;
        self.resources.unlock_faces(group).map_err(Into::into)
    }
}
