//! Typed index storage for derived litLod owners, in the existing manager
//! domain. Source face groups keep their own buffers unchanged.
use super::*;

impl ManagedVisualResources {
    pub(crate) fn create_index_storage_owner(&mut self) -> Result<IndexOwnerKey, ManagedError> {
        self.available()?;
        self.managers.create_index_owner().map_err(Into::into)
    }

    pub(crate) fn allocate_index_storage<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        owner: IndexOwnerKey,
        usage: u32,
        count: u32,
        backend: &mut B,
    ) -> Result<Option<CompleteIndexBuffer>, ManagedOperationError<B::Error>> {
        self.available()?;
        Self::reserve(&mut self.index_tokens)?;
        let token = Self::next_token(self.index_tokens.len())?;
        let allocated =
            match manager_result(self.managers.allocate_index(owner, usage, count, backend)) {
                Ok(value) => value,
                Err(error) => {
                    self.poisoned = true;
                    return Err(error);
                }
            };
        if !allocated {
            return Ok(None);
        }
        let result = (|| {
            let raw = active_descriptor(&self.managers, OwnerKey::Index(owner), count)?;
            let (Some(BufferKey::Index(buffer)), Some(GraphicsBuffer::Index(graphics))) =
                (raw.buffer, raw.graphics)
            else {
                return Err(ManagedError::BindingMismatch);
            };
            let visual = if let Some(token) = self.index_tokens.iter().find(|t| t.manager == buffer)
            {
                token.visual
            } else {
                let visual = vr::IndexBufferKey(token);
                self.index_tokens.push(IndexBufferToken {
                    manager: buffer,
                    visual,
                });
                visual
            };
            Ok(CompleteIndexBuffer {
                buffer: Some(visual),
                base: raw.base,
                graphics: Some(graphics),
            })
        })();
        match result {
            Ok(block) => Ok(Some(block)),
            Err(error) => {
                self.poisoned = true;
                Err(error.into())
            }
        }
    }

    /// Validate both the actual owner lease and the complete copied descriptor.
    /// The returned usage belongs to the actual manager buffer, not the caller.
    pub(crate) fn index_storage_configuration(
        &self,
        owner: IndexOwnerKey,
        descriptor: CompleteIndexBuffer,
        count: u32,
    ) -> Result<u32, ManagedError> {
        self.available()?;
        let raw = active_descriptor(&self.managers, OwnerKey::Index(owner), count)?;
        let (Some(BufferKey::Index(buffer)), Some(GraphicsBuffer::Index(graphics))) =
            (raw.buffer, raw.graphics)
        else {
            return Err(ManagedError::BindingMismatch);
        };
        let visual = self
            .index_tokens
            .iter()
            .find(|t| t.manager == buffer)
            .ok_or(ManagedError::BindingMismatch)?
            .visual;
        if descriptor
            != (CompleteIndexBuffer {
                buffer: Some(visual),
                base: raw.base,
                graphics: Some(graphics),
            })
        {
            return Err(ManagedError::BindingMismatch);
        }
        Ok(self.managers.index_buffer(buffer)?.usage)
    }

    pub(crate) fn free_index_storage<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        owner: Option<IndexOwnerKey>,
        descriptor: CompleteIndexBuffer,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.available()?;
        let Some(owner) = owner else {
            return if descriptor == CompleteIndexBuffer::default() {
                Ok(())
            } else {
                Err(ManagedError::BindingMismatch.into())
            };
        };
        let state = self.managers.index_owner(owner)?;
        if state.lease == OwnerLease::Vacant
            && state.descriptor == Descriptor::default()
            && descriptor == CompleteIndexBuffer::default()
        {
            return Ok(());
        }
        let OwnerLease::Active {
            block,
            published: true,
        } = state.lease
        else {
            return Err(ManagedError::BindingMismatch.into());
        };
        self.index_storage_configuration(owner, descriptor, self.managers.block(block)?.count)?;
        let result = manager_result(self.managers.free_index(owner, backend));
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}
