//! C_VBuffer Lock/Unlock over the same manager's cached locks and list.
use super::*;
impl BufferManagers {
    pub(crate) fn lock_vertex_buffer<B: BufferBackend + ?Sized>(
        &mut self,
        buffer: VertexBufferKey,
        flags: u32,
        backend: &mut B,
    ) -> Result<LockedDataKey, OperationError<B::Error>> {
        self.available()?;
        let key = BufferKey::Vertex(buffer);
        let graphics = self
            .buffer(key)?
            .graphics
            .ok_or(BufferError::InvalidLockState)?;
        if let Some(data) = self.buffer(key)?.data {
            if self.buffer(key)?.flags & 0x10 == 0 || flags & 0x10 != 0 {
                return Ok(data);
            }
            self.call(backend.unlock(graphics))?;
        }
        self.buffer_mut(key)?.flags = flags;
        let data = self.call(backend.lock(graphics, flags))?;
        self.buffer_mut(key)?.data = Some(data);
        if data.0 == 0 {
            return Err(self.invalid_backend());
        }
        if self.buffer(key)?.lock_index.is_none() {
            let index = self.vertex_locked.len();
            self.buffer_mut(key)?.lock_index = Some(index);
            self.vertex_locked.push(key);
        }
        Ok(data)
    }
    pub(crate) fn unlock_vertex_buffer<B: BufferBackend + ?Sized>(
        &mut self,
        buffer: VertexBufferKey,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.available()?;
        let key = BufferKey::Vertex(buffer);
        if self.buffer(key)?.data.is_none() {
            return Ok(());
        }
        let graphics = self
            .buffer(key)?
            .graphics
            .ok_or(BufferError::InvalidLockState)?;
        self.call(backend.unlock(graphics))?;
        self.buffer_mut(key)?.data = None;
        let index = self
            .buffer(key)?
            .lock_index
            .ok_or(BufferError::InvalidLockState)?;
        let moved = *self
            .vertex_locked
            .last()
            .ok_or(BufferError::InvalidLockState)?;
        self.buffer_mut(moved)?.lock_index = Some(index);
        self.vertex_locked.swap_remove(index);
        self.buffer_mut(key)?.lock_index = None;
        Ok(())
    }
}

impl BufferManagers {
    /// The ordinary flags0 index read/write lock used by GetFaces.
    pub(crate) fn lock_index_buffer<B: BufferBackend + ?Sized>(
        &mut self,
        buffer: IndexBufferKey,
        backend: &mut B,
    ) -> Result<LockedDataKey, OperationError<B::Error>> {
        self.available()?;
        let key = BufferKey::Index(buffer);
        let graphics = self
            .buffer(key)?
            .graphics
            .ok_or(BufferError::InvalidLockState)?;
        if let Some(data) = self.buffer(key)?.data {
            if self.buffer(key)?.flags & 0x10 == 0 {
                return Ok(data);
            }
            self.call(backend.unlock(graphics))?;
        }
        self.buffer_mut(key)?.flags = 0;
        let data = self.call(backend.lock(graphics, 0))?;
        if data.0 == 0 {
            return Err(self.invalid_backend());
        }
        self.buffer_mut(key)?.data = Some(data);
        if self.buffer(key)?.lock_index.is_none() {
            let index = self.index_locked.len();
            self.buffer_mut(key)?.lock_index = Some(index);
            self.index_locked.push(key);
        }
        Ok(data)
    }
}
