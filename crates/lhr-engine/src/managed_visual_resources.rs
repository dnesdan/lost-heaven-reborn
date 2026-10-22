//! Bounded host bridge between visual ownership and actual IB/VB managers.
//!
//! This descriptive façade allocates lifetime storage, not geometry uploads or
//! reconstructed CreateFaces/CreateVertexMesh. Mutable arenas stay private so
//! copied descriptors cannot be invalidated by forced manager destruction.
//! Read-only views include interrupted diagnostic state after an error.
//!
//! Every operation must use the same compatible live backend identity domain.
//! Callbacks are serialized and must not reenter this façade. External resources
//! require explicit cleanup while that backend exists; Drop cannot release them.
use crate::buffer_managers::{
    self as bm, BufferBackend, BufferKey, BufferManagers, Descriptor, GraphicsBuffer,
    IndexOwnerKey, OwnerKey, OwnerLease, VertexOwnerKey, VertexRequest,
};
use crate::visual_resources::{
    self as vr, CompleteIndexBuffer, CompleteVertexBuffer, FaceGroupKey, MaterialKey, MeshLevelKey,
    MeshObjectKey, RawVertexBytesKey, TextureKey, VertexBufferPointer, VisualResourceBackend,
    VisualResourceError, VisualResourceOperationError, VisualResources,
};

/// Explicit external dependencies, in addition to actual buffer-manager effects.
/// No texture/raw-byte implementation or no-op fallback is supplied here.
pub trait ManagedVisualBackend: BufferBackend {
    fn retain_texture(&mut self, texture: TextureKey) -> Result<(), Self::Error>;
    fn release_texture(&mut self, texture: TextureKey) -> Result<(), Self::Error>;
    fn texture_flags(&mut self, texture: TextureKey) -> Result<u32, Self::Error>;
    fn free_raw_vertices(
        &mut self,
        level: MeshLevelKey,
        bytes: RawVertexBytesKey,
    ) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedError {
    Poisoned,
    Visual(VisualResourceError),
    Manager(bm::BufferError),
    InvalidExtent,
    BindingMismatch,
    HostAllocation,
    IdentityOverflow,
}
impl From<VisualResourceError> for ManagedError {
    fn from(e: VisualResourceError) -> Self {
        Self::Visual(e)
    }
}
impl From<bm::BufferError> for ManagedError {
    fn from(e: bm::BufferError) -> Self {
        Self::Manager(e)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ManagedOperationError<E> {
    State(ManagedError),
    Backend(E),
}
impl<E> From<ManagedError> for ManagedOperationError<E> {
    fn from(e: ManagedError) -> Self {
        Self::State(e)
    }
}
impl<E> From<VisualResourceError> for ManagedOperationError<E> {
    fn from(e: VisualResourceError) -> Self {
        Self::State(e.into())
    }
}
impl<E> From<bm::BufferError> for ManagedOperationError<E> {
    fn from(e: bm::BufferError) -> Self {
        Self::State(e.into())
    }
}
fn manager_result<T, E>(
    r: Result<T, bm::OperationError<E>>,
) -> Result<T, ManagedOperationError<E>> {
    r.map_err(|e| match e {
        bm::OperationError::State(e) => e.into(),
        bm::OperationError::Backend(e) => ManagedOperationError::Backend(e),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Pending includes a retryable graphics rejection or an interrupted allocation.
/// Installed/Consumed distinguish complete admission and successful owner free;
/// an interrupted free may leave Installed with a diagnostic Freed manager lease.
pub enum BindingState {
    Pending,
    Installed,
    Consumed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexBinding {
    pub group: FaceGroupKey,
    pub owner: IndexOwnerKey,
    pub state: BindingState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexBinding {
    pub level: MeshLevelKey,
    pub owner: VertexOwnerKey,
    pub state: BindingState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Façade-local visual token for a typed manager identity, never a graphics ID.
pub struct IndexBufferToken {
    pub manager: bm::IndexBufferKey,
    pub visual: vr::IndexBufferKey,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexBufferToken {
    pub manager: bm::VertexBufferKey,
    pub visual: vr::VertexBufferKey,
}

pub struct ManagedVisualResources {
    resources: VisualResources,
    managers: BufferManagers,
    indices: Vec<IndexBinding>,
    vertices: Vec<VertexBinding>,
    index_tokens: Vec<IndexBufferToken>,
    vertex_tokens: Vec<VertexBufferToken>,
    poisoned: bool,
}
impl ManagedVisualResources {
    pub fn new() -> Result<Self, ManagedError> {
        Ok(Self {
            resources: VisualResources::new()?,
            managers: BufferManagers::new()?,
            indices: Vec::new(),
            vertices: Vec::new(),
            index_tokens: Vec::new(),
            vertex_tokens: Vec::new(),
            poisoned: false,
        })
    }
    pub fn resources(&self) -> &VisualResources {
        &self.resources
    }
    pub fn managers(&self) -> &BufferManagers {
        &self.managers
    }
    pub fn index_bindings(&self) -> &[IndexBinding] {
        &self.indices
    }
    pub fn vertex_bindings(&self) -> &[VertexBinding] {
        &self.vertices
    }
    pub fn index_tokens(&self) -> &[IndexBufferToken] {
        &self.index_tokens
    }
    pub fn vertex_tokens(&self) -> &[VertexBufferToken] {
        &self.vertex_tokens
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned || self.resources.is_poisoned() || self.managers.is_poisoned()
    }
    fn available(&self) -> Result<(), ManagedError> {
        if self.is_poisoned() {
            Err(ManagedError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn reserve<T>(v: &mut Vec<T>) -> Result<(), ManagedError> {
        v.try_reserve(1).map_err(|_| ManagedError::HostAllocation)
    }
    fn next_token(len: usize) -> Result<u64, ManagedError> {
        u64::try_from(len)
            .ok()
            .and_then(|v| v.checked_add(1))
            .ok_or(ManagedError::IdentityOverflow)
    }
    fn visual_operation<T, B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
        operation: impl FnOnce(
            &mut VisualResources,
            &mut BridgeBackend<'_, B>,
        ) -> Result<
            T,
            VisualResourceOperationError<ManagedOperationError<B::Error>>,
        >,
    ) -> Result<T, ManagedOperationError<B::Error>> {
        self.available()?;
        let mut adapter = BridgeBackend {
            managers: &mut self.managers,
            indices: &mut self.indices,
            vertices: &mut self.vertices,
            index_tokens: &self.index_tokens,
            vertex_tokens: &self.vertex_tokens,
            backend,
        };
        operation(&mut self.resources, &mut adapter).map_err(|e| match e {
            VisualResourceOperationError::Resource(e) => e.into(),
            VisualResourceOperationError::Backend(e) => {
                self.poisoned = true;
                e
            }
        })
    }
    /// Allocates three indices per face. No index bytes are uploaded.
    /// Graphics rejection returns false and leaves a retryable vacant owner.
    /// Once manager allocation is entered, any error conservatively makes the
    /// façade unavailable; a manager State error can follow partial publication.
    pub fn allocate_group_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        num_faces: u32,
        usage: u32,
        backend: &mut B,
    ) -> Result<bool, ManagedOperationError<B::Error>> {
        self.available()?;
        let count = num_faces
            .checked_mul(3)
            .filter(|v| *v > 0 && *v <= i32::MAX as u32)
            .ok_or(ManagedError::InvalidExtent)?;
        let current = self.resources.face_group(group)?;
        if current.num_faces != 0 || current.index_buffer != CompleteIndexBuffer::default() {
            return Err(VisualResourceError::StorageAlreadyInstalled.into());
        }
        Self::reserve(&mut self.indices)?;
        Self::reserve(&mut self.index_tokens)?;
        let token = Self::next_token(self.index_tokens.len())?;
        let position = if let Some(i) = self.indices.iter().position(|b| b.group == group) {
            if self.indices[i].state != BindingState::Pending {
                return Err(ManagedError::BindingMismatch.into());
            }
            i
        } else {
            let owner = self.managers.create_index_owner()?;
            self.indices.push(IndexBinding {
                group,
                owner,
                state: BindingState::Pending,
            });
            self.indices.len() - 1
        };
        let owner = self.indices[position].owner;
        let result = self.managers.allocate_index(owner, usage, count, backend);
        let allocated = match manager_result(result) {
            Ok(v) => v,
            Err(e) => {
                self.poisoned = true;
                return Err(e);
            }
        };
        if !allocated {
            return Ok(false);
        }
        let result = (|| {
            let raw = active_descriptor(&self.managers, OwnerKey::Index(owner), count)?;
            let (Some(BufferKey::Index(buffer)), Some(GraphicsBuffer::Index(graphics))) =
                (raw.buffer, raw.graphics)
            else {
                return Err(ManagedError::BindingMismatch);
            };
            let visual = if let Some(t) = self.index_tokens.iter().find(|t| t.manager == buffer) {
                t.visual
            } else {
                let visual = vr::IndexBufferKey(token);
                self.index_tokens.push(IndexBufferToken {
                    manager: buffer,
                    visual,
                });
                visual
            };
            self.resources.install_index_block(
                group,
                num_faces,
                CompleteIndexBuffer {
                    buffer: Some(visual),
                    base: raw.base,
                    graphics: Some(graphics),
                },
            )?;
            self.indices[position].state = BindingState::Installed;
            Ok(())
        })();
        if let Err(e) = result {
            self.poisoned = true;
            return Err(e.into());
        }
        Ok(true)
    }
    /// Allocates managed lifetime storage; vertex upload and bbox remain explicit.
    /// Primitive freshness/chain checks precede effects. Any error after entering
    /// manager allocation conservatively makes the façade unavailable, preserving
    /// its pending owner and any completed allocation/admission prefix.
    pub fn allocate_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        num_vertices: u32,
        request: VertexRequest,
        head: Option<FaceGroupKey>,
        backend: &mut B,
    ) -> Result<bool, ManagedOperationError<B::Error>> {
        self.available()?;
        if num_vertices == 0 || num_vertices > i32::MAX as u32 || request.count != num_vertices {
            return Err(ManagedError::InvalidExtent.into());
        }
        self.resources
            .check_mesh_level_storage_intake(level, head)?;
        Self::reserve(&mut self.vertices)?;
        Self::reserve(&mut self.vertex_tokens)?;
        let token = Self::next_token(self.vertex_tokens.len())?;
        let position = if let Some(i) = self.vertices.iter().position(|b| b.level == level) {
            if self.vertices[i].state != BindingState::Pending {
                return Err(ManagedError::BindingMismatch.into());
            }
            i
        } else {
            let owner = self.managers.create_vertex_owner()?;
            self.vertices.push(VertexBinding {
                level,
                owner,
                state: BindingState::Pending,
            });
            self.vertices.len() - 1
        };
        let owner = self.vertices[position].owner;
        let result = self.managers.allocate_vertex(owner, request, backend);
        let allocated = match manager_result(result) {
            Ok(v) => v,
            Err(e) => {
                self.poisoned = true;
                return Err(e);
            }
        };
        if !allocated {
            return Ok(false);
        }
        let result = (|| {
            let raw = active_descriptor(&self.managers, OwnerKey::Vertex(owner), num_vertices)?;
            let (Some(BufferKey::Vertex(buffer)), Some(GraphicsBuffer::Vertex(graphics))) =
                (raw.buffer, raw.graphics)
            else {
                return Err(ManagedError::BindingMismatch);
            };
            let visual = if let Some(t) = self.vertex_tokens.iter().find(|t| t.manager == buffer) {
                t.visual
            } else {
                let visual = vr::VertexBufferKey(token);
                self.vertex_tokens.push(VertexBufferToken {
                    manager: buffer,
                    visual,
                });
                visual
            };
            self.resources.install_mesh_level_storage(
                level,
                num_vertices,
                CompleteVertexBuffer {
                    buffer: Some(VertexBufferPointer::Managed(visual)),
                    base: raw.base,
                    graphics: Some(graphics),
                },
                head,
            )?;
            self.vertices[position].state = BindingState::Installed;
            Ok(())
        })();
        if let Err(e) = result {
            self.poisoned = true;
            return Err(e.into());
        }
        Ok(true)
    }
}
fn active_descriptor(
    managers: &BufferManagers,
    owner: OwnerKey,
    count: u32,
) -> Result<Descriptor, ManagedError> {
    let state = managers.owner(owner)?;
    let OwnerLease::Active {
        block,
        published: true,
    } = state.lease
    else {
        return Err(ManagedError::BindingMismatch);
    };
    let block = managers.block(block)?;
    if block.owner != Some(owner)
        || block.count != count
        || state.descriptor.buffer != Some(block.buffer)
        || state.descriptor.base != block.base
        || state.descriptor.graphics != managers.buffer(block.buffer)?.graphics
    {
        return Err(ManagedError::BindingMismatch);
    }
    Ok(state.descriptor)
}
struct BridgeBackend<'a, B: ManagedVisualBackend + ?Sized> {
    managers: &'a mut BufferManagers,
    indices: &'a mut [IndexBinding],
    vertices: &'a mut [VertexBinding],
    index_tokens: &'a [IndexBufferToken],
    vertex_tokens: &'a [VertexBufferToken],
    backend: &'a mut B,
}
impl<B: ManagedVisualBackend + ?Sized> VisualResourceBackend for BridgeBackend<'_, B> {
    type Error = ManagedOperationError<B::Error>;
    fn retain_texture(&mut self, t: TextureKey) -> Result<(), Self::Error> {
        self.backend
            .retain_texture(t)
            .map_err(ManagedOperationError::Backend)
    }
    fn release_texture(&mut self, t: TextureKey) -> Result<(), Self::Error> {
        self.backend
            .release_texture(t)
            .map_err(ManagedOperationError::Backend)
    }
    fn texture_flags(&mut self, t: TextureKey) -> Result<u32, Self::Error> {
        self.backend
            .texture_flags(t)
            .map_err(ManagedOperationError::Backend)
    }
    fn free_raw_vertices(
        &mut self,
        l: MeshLevelKey,
        b: RawVertexBytesKey,
    ) -> Result<(), Self::Error> {
        self.backend
            .free_raw_vertices(l, b)
            .map_err(ManagedOperationError::Backend)
    }
    fn free_index_block(
        &mut self,
        group: FaceGroupKey,
        descriptor: CompleteIndexBuffer,
    ) -> Result<(), Self::Error> {
        let Some(binding) = self.indices.iter_mut().find(|b| b.group == group) else {
            return if descriptor == CompleteIndexBuffer::default() {
                Ok(())
            } else {
                Err(ManagedError::BindingMismatch.into())
            };
        };
        if binding.state == BindingState::Pending && descriptor == CompleteIndexBuffer::default() {
            let owner = self.managers.index_owner(binding.owner)?;
            return if owner.lease == OwnerLease::Vacant && owner.descriptor == Descriptor::default()
            {
                Ok(())
            } else {
                Err(ManagedError::BindingMismatch.into())
            };
        }
        if binding.state != BindingState::Installed {
            return Err(ManagedError::BindingMismatch.into());
        }
        let state = self.managers.index_owner(binding.owner)?;
        let OwnerLease::Active {
            block,
            published: true,
        } = state.lease
        else {
            return Err(ManagedError::BindingMismatch.into());
        };
        let count = self.managers.block(block)?.count;
        let raw = active_descriptor(self.managers, OwnerKey::Index(binding.owner), count)?;
        let (Some(BufferKey::Index(buffer)), Some(GraphicsBuffer::Index(graphics))) =
            (raw.buffer, raw.graphics)
        else {
            return Err(ManagedError::BindingMismatch.into());
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
            return Err(ManagedError::BindingMismatch.into());
        }
        manager_result(self.managers.free_index(binding.owner, self.backend))?;
        binding.state = BindingState::Consumed;
        Ok(())
    }
    fn free_vertex_block(
        &mut self,
        level: MeshLevelKey,
        descriptor: CompleteVertexBuffer,
    ) -> Result<(), Self::Error> {
        let Some(binding) = self.vertices.iter_mut().find(|b| b.level == level) else {
            return if descriptor == CompleteVertexBuffer::default() {
                Ok(())
            } else {
                Err(ManagedError::BindingMismatch.into())
            };
        };
        if binding.state == BindingState::Pending && descriptor == CompleteVertexBuffer::default() {
            let owner = self.managers.vertex_owner(binding.owner)?;
            return if owner.lease == OwnerLease::Vacant && owner.descriptor == Descriptor::default()
            {
                Ok(())
            } else {
                Err(ManagedError::BindingMismatch.into())
            };
        }
        if binding.state != BindingState::Installed {
            return Err(ManagedError::BindingMismatch.into());
        }
        let state = self.managers.vertex_owner(binding.owner)?;
        let OwnerLease::Active {
            block,
            published: true,
        } = state.lease
        else {
            return Err(ManagedError::BindingMismatch.into());
        };
        let count = self.managers.block(block)?.count;
        let raw = active_descriptor(self.managers, OwnerKey::Vertex(binding.owner), count)?;
        let (Some(BufferKey::Vertex(buffer)), Some(GraphicsBuffer::Vertex(graphics))) =
            (raw.buffer, raw.graphics)
        else {
            return Err(ManagedError::BindingMismatch.into());
        };
        let visual = self
            .vertex_tokens
            .iter()
            .find(|t| t.manager == buffer)
            .ok_or(ManagedError::BindingMismatch)?
            .visual;
        if descriptor
            != (CompleteVertexBuffer {
                buffer: Some(VertexBufferPointer::Managed(visual)),
                base: raw.base,
                graphics: Some(graphics),
            })
        {
            return Err(ManagedError::BindingMismatch.into());
        }
        manager_result(self.managers.free_vertex(binding.owner, self.backend))?;
        binding.state = BindingState::Consumed;
        Ok(())
    }
}

impl ManagedVisualResources {
    pub fn create_material(&mut self) -> Result<MaterialKey, ManagedError> {
        self.available()?;
        self.resources.create_material().map_err(Into::into)
    }
    pub fn create_face_group(&mut self) -> Result<FaceGroupKey, ManagedError> {
        self.available()?;
        self.resources.create_face_group().map_err(Into::into)
    }
    pub fn create_mesh_level(&mut self) -> Result<MeshLevelKey, ManagedError> {
        self.available()?;
        self.resources.create_mesh_level().map_err(Into::into)
    }
    pub fn create_mesh_object(&mut self) -> Result<MeshObjectKey, ManagedError> {
        self.available()?;
        self.resources.create_mesh_object().map_err(Into::into)
    }
    pub fn retain_material(&mut self, key: MaterialKey) -> Result<(), ManagedError> {
        self.available()?;
        self.resources.retain_material(key).map_err(Into::into)
    }
    pub fn retain_mesh_level(&mut self, key: MeshLevelKey) -> Result<(), ManagedError> {
        self.available()?;
        self.resources.retain_mesh_level(key).map_err(Into::into)
    }
    pub fn retain_mesh_object(&mut self, key: MeshObjectKey) -> Result<(), ManagedError> {
        self.available()?;
        self.resources.retain_mesh_object(key).map_err(Into::into)
    }
    pub fn link_next(
        &mut self,
        group: FaceGroupKey,
        next: FaceGroupKey,
    ) -> Result<(), ManagedError> {
        self.available()?;
        self.resources.link_next(group, next).map_err(Into::into)
    }
    pub fn install_mesh_level_bbox(
        &mut self,
        key: MeshLevelKey,
        bbox: [[f32; 3]; 2],
    ) -> Result<(), ManagedError> {
        self.available()?;
        self.resources
            .install_mesh_level_bbox(key, bbox)
            .map_err(Into::into)
    }
    pub fn update_mesh_bound_volume(&mut self, key: MeshObjectKey) -> Result<(), ManagedError> {
        self.available()?;
        self.resources
            .update_mesh_bound_volume(key)
            .map_err(Into::into)
    }
    pub fn release_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        backend: &mut B,
    ) -> Result<u32, ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.release_material(key, b))
    }
    pub fn release_mesh_level<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshLevelKey,
        backend: &mut B,
    ) -> Result<u32, ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.release_mesh_level(key, b))
    }
    pub fn release_mesh_object<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        backend: &mut B,
    ) -> Result<u32, ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.release_mesh_object(key, b))
    }
    pub fn destroy_face_group<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.destroy_face_group(group, b))
    }
    pub fn set_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        material: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.set_material(group, material, b))
    }
    pub fn set_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.set_texture(key, texture, b))
    }
    pub fn set_env_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.set_env_texture(key, texture, b))
    }
    pub fn set_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        level: Option<MeshLevelKey>,
        index: u32,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.visual_operation(backend, |r, b| r.set_lod(key, level, index, b))
    }
    pub fn reclaim_unused_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.available()?;
        let result = manager_result(self.managers.reclaim_unused_indices(backend));
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn reclaim_unused_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.available()?;
        let result = manager_result(self.managers.reclaim_unused_vertices(backend));
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn unlock_all_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.available()?;
        let result = manager_result(self.managers.unlock_all_indices(backend));
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub fn unlock_all_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ManagedOperationError<B::Error>> {
        self.available()?;
        let result = manager_result(self.managers.unlock_all_vertices(backend));
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm::{AllocationToken, BufferKind, CreateResult, GraphicsRequest, LockedDataKey};
    use std::collections::{BTreeMap, VecDeque};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        BufferNew(BufferKind, AllocationToken),
        BufferFree(AllocationToken),
        BlockNew(BufferKind, u32, u32, AllocationToken),
        BlockFree(AllocationToken),
        Create(GraphicsRequest),
        Discard,
        Lock(GraphicsBuffer, u32),
        Unlock(GraphicsBuffer),
        Release(GraphicsBuffer),
        Vram,
        Oom,
        TextureRetain(TextureKey),
        TextureRelease(TextureKey),
    }
    /// Required callbacks backed by tracked CPU fixture storage; this is not GPU
    /// execution or a native texture implementation.
    struct Backend {
        next: u64,
        allocations: BTreeMap<u64, bool>,
        graphics: BTreeMap<u64, (bool, Vec<u8>)>,
        textures: BTreeMap<u64, u32>,
        events: Vec<Event>,
        rejects: VecDeque<i32>,
        fail_texture: Option<TextureKey>,
        fail_release: bool,
        fail_block: bool,
    }
    impl Default for Backend {
        fn default() -> Self {
            Self {
                next: 1,
                allocations: BTreeMap::new(),
                graphics: BTreeMap::new(),
                textures: BTreeMap::new(),
                events: Vec::new(),
                rejects: VecDeque::new(),
                fail_texture: None,
                fail_release: false,
                fail_block: false,
            }
        }
    }
    impl Backend {
        fn next(&mut self) -> u64 {
            let n = self.next;
            self.next += 1;
            n
        }
        fn graphics_id(g: GraphicsBuffer) -> u64 {
            match g {
                GraphicsBuffer::Index(h) => h.0,
                GraphicsBuffer::Vertex(h) => h.0,
            }
        }
        fn releases(&self) -> Vec<GraphicsBuffer> {
            self.events
                .iter()
                .filter_map(|e| {
                    if let Event::Release(g) = e {
                        Some(*g)
                    } else {
                        None
                    }
                })
                .collect()
        }
    }
    impl BufferBackend for Backend {
        type Error = &'static str;
        fn allocate_buffer(&mut self, k: BufferKind) -> Result<AllocationToken, Self::Error> {
            let t = AllocationToken(self.next());
            self.allocations.insert(t.0, true);
            self.events.push(Event::BufferNew(k, t));
            Ok(t)
        }
        fn free_buffer(&mut self, t: AllocationToken) -> Result<(), Self::Error> {
            assert_eq!(self.allocations.remove(&t.0), Some(true));
            self.events.push(Event::BufferFree(t));
            Ok(())
        }
        fn allocate_block(
            &mut self,
            k: BufferKind,
            b: u32,
            n: u32,
        ) -> Result<AllocationToken, Self::Error> {
            if self.fail_block {
                return Err("block failure");
            }
            let t = AllocationToken(self.next());
            self.allocations.insert(t.0, false);
            self.events.push(Event::BlockNew(k, b, n, t));
            Ok(t)
        }
        fn free_block(&mut self, t: AllocationToken) -> Result<(), Self::Error> {
            assert_eq!(self.allocations.remove(&t.0), Some(false));
            self.events.push(Event::BlockFree(t));
            Ok(())
        }
        fn create_graphics(&mut self, r: GraphicsRequest) -> Result<CreateResult, Self::Error> {
            self.events.push(Event::Create(r));
            if let Some(hr) = self.rejects.pop_front() {
                return Ok(CreateResult::Rejected(hr));
            }
            let id = self.next();
            self.graphics.insert(id, (false, vec![0; r.bytes as usize]));
            Ok(CreateResult::Created(match r.kind {
                BufferKind::Index => GraphicsBuffer::Index(vr::IndexBufferHandle(id)),
                BufferKind::Vertex => GraphicsBuffer::Vertex(vr::VertexBufferHandle(id)),
            }))
        }
        fn discard_managed(&mut self) -> Result<(), Self::Error> {
            self.events.push(Event::Discard);
            Ok(())
        }
        fn lock(&mut self, g: GraphicsBuffer, f: u32) -> Result<LockedDataKey, Self::Error> {
            let storage = self.graphics.get_mut(&Self::graphics_id(g)).unwrap();
            assert!(!storage.0);
            storage.0 = true;
            self.events.push(Event::Lock(g, f));
            Ok(LockedDataKey(self.next()))
        }
        fn unlock(&mut self, g: GraphicsBuffer) -> Result<(), Self::Error> {
            let storage = self.graphics.get_mut(&Self::graphics_id(g)).unwrap();
            assert!(storage.0);
            storage.0 = false;
            self.events.push(Event::Unlock(g));
            Ok(())
        }
        fn release_graphics(&mut self, g: GraphicsBuffer) -> Result<(), Self::Error> {
            let storage = self.graphics.remove(&Self::graphics_id(g)).unwrap();
            assert!(!storage.0);
            self.events.push(Event::Release(g));
            if self.fail_release {
                Err("graphics released then failed")
            } else {
                Ok(())
            }
        }
        fn driver_present(&self) -> bool {
            true
        }
        fn recompute_vram(&mut self) -> Result<(), Self::Error> {
            self.events.push(Event::Vram);
            Ok(())
        }
        fn warn_unreleased_buffer(&mut self, _: BufferKey) -> Result<(), Self::Error> {
            panic!("bridge must not destroy a used buffer")
        }
        fn out_of_memory(&mut self, _: BufferKind, _: u32, _: i32) -> Result<(), Self::Error> {
            self.events.push(Event::Oom);
            Ok(())
        }
    }
    impl ManagedVisualBackend for Backend {
        fn retain_texture(&mut self, t: TextureKey) -> Result<(), Self::Error> {
            *self.textures.get_mut(&t.0).ok_or("unknown texture")? += 1;
            self.events.push(Event::TextureRetain(t));
            Ok(())
        }
        fn release_texture(&mut self, t: TextureKey) -> Result<(), Self::Error> {
            let refs = self.textures.get_mut(&t.0).ok_or("unknown texture")?;
            assert!(*refs > 0);
            *refs -= 1;
            self.events.push(Event::TextureRelease(t));
            if self.fail_texture == Some(t) {
                Err("texture released then failed")
            } else {
                Ok(())
            }
        }
        fn texture_flags(&mut self, t: TextureKey) -> Result<u32, Self::Error> {
            self.textures.get(&t.0).ok_or("unknown texture")?;
            Ok(0)
        }
        fn free_raw_vertices(
            &mut self,
            _: MeshLevelKey,
            _: RawVertexBytesKey,
        ) -> Result<(), Self::Error> {
            Err("raw vertices unavailable")
        }
    }
    fn request(count: u32) -> VertexRequest {
        VertexRequest {
            fvf: 0x112,
            usage: 0,
            stride: 32,
            count,
            format: 0,
            exclusive: false,
        }
    }
    fn one_level(r: &mut ManagedVisualResources, b: &mut Backend) -> (MeshLevelKey, FaceGroupKey) {
        let group = r.create_face_group().unwrap();
        let level = r.create_mesh_level().unwrap();
        assert!(r.allocate_group_indices(group, 2, 0, b).unwrap());
        assert!(r
            .allocate_level_vertices(level, 4, request(4), Some(group), b)
            .unwrap());
        r.install_mesh_level_bbox(level, [[-1., -2., -3.], [3., 4., 5.]])
            .unwrap();
        (level, group)
    }
    #[test]
    fn shared_mesh_graph_releases_real_blocks_and_distinguishes_ib_cache_from_vb_reclaim() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let material = r.create_material().unwrap();
        let a = r.create_face_group().unwrap();
        let z = r.create_face_group().unwrap();
        r.set_material(a, Some(material), &mut b).unwrap();
        r.set_material(z, Some(material), &mut b).unwrap();
        assert!(r.allocate_group_indices(a, 2, 0, &mut b).unwrap());
        assert!(r.allocate_group_indices(z, 3, 0, &mut b).unwrap());
        let first = r.resources().face_group(a).unwrap().index_buffer;
        let second = r.resources().face_group(z).unwrap().index_buffer;
        assert_eq!(first.buffer, second.buffer);
        assert_eq!(first.graphics, second.graphics);
        assert_eq!((first.base, second.base), (0, 6));
        assert_eq!(r.index_tokens().len(), 1);
        let owners = [r.index_bindings()[0].owner, r.index_bindings()[1].owner];
        r.link_next(a, z).unwrap();
        let level = r.create_mesh_level().unwrap();
        assert!(r
            .allocate_level_vertices(level, 8, request(8), Some(a), &mut b)
            .unwrap());
        r.install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
            .unwrap();
        let mesh1 = r.create_mesh_object().unwrap();
        let mesh2 = r.create_mesh_object().unwrap();
        r.set_lod(mesh1, Some(level), 0, &mut b).unwrap();
        r.set_lod(mesh2, Some(level), 0, &mut b).unwrap();
        assert_eq!(r.release_mesh_level(level, &mut b).unwrap(), 2);
        assert_eq!(r.release_material(material, &mut b).unwrap(), 2);
        let before = b.events.len();
        assert_eq!(r.release_mesh_object(mesh1, &mut b).unwrap(), 0);
        assert_eq!(b.events.len(), before);
        assert_eq!(r.resources().mesh_level(level).unwrap().references, 1);
        assert_eq!(r.release_mesh_object(mesh2, &mut b).unwrap(), 0);
        assert!(r.resources().mesh_level(level).is_err());
        assert!(r.resources().face_group(a).is_err());
        assert!(r.resources().material(material).is_err());
        assert!(r
            .index_bindings()
            .iter()
            .all(|x| x.state == BindingState::Consumed));
        assert_eq!(r.vertex_bindings()[0].state, BindingState::Consumed);
        for owner in owners {
            let state = r.managers().index_owner(owner).unwrap();
            assert_eq!(state.lease, OwnerLease::Freed);
            assert!(state.descriptor.buffer.is_some());
        }
        assert_eq!(b.releases().len(), 1);
        assert!(matches!(b.releases()[0], GraphicsBuffer::Vertex(_)));
        assert!(r.managers().vertex_buffers().is_empty());
        let ib = r.managers().index_buffers()[0];
        let state = r.managers().buffer(ib).unwrap();
        assert_eq!(state.num_free, state.capacity);
        assert_eq!(state.free_blocks.len(), 1);
        r.reclaim_unused_indices(&mut b).unwrap();
        assert_eq!(b.releases().len(), 2);
        assert!(r.managers().index_buffers().is_empty());
        assert!(b.allocations.is_empty());
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn two_levels_share_graphics_and_reused_vertex_format_does_not_become_a_new_search_key() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let (a, ga) = one_level(&mut r, &mut b);
        let gb = r.create_face_group().unwrap();
        let z = r.create_mesh_level().unwrap();
        assert!(r.allocate_group_indices(gb, 1, 0, &mut b).unwrap());
        let mut reused = request(2);
        reused.stride = 0;
        reused.format = 123;
        assert!(r
            .allocate_level_vertices(z, 2, reused, Some(gb), &mut b)
            .unwrap());
        assert_eq!(r.vertex_tokens().len(), 1);
        assert_eq!(
            r.resources().mesh_level(a).unwrap().vertex_buffer.graphics,
            r.resources().mesh_level(z).unwrap().vertex_buffer.graphics
        );
        r.release_mesh_level(a, &mut b).unwrap();
        assert!(r.resources().face_group(ga).is_err());
        assert!(b.releases().is_empty());
        assert_eq!(r.index_bindings()[0].state, BindingState::Consumed);
        assert_eq!(r.index_bindings()[1].state, BindingState::Installed);
        r.release_mesh_level(z, &mut b).unwrap();
        assert_eq!(b.releases().len(), 1);
        r.reclaim_unused_indices(&mut b).unwrap();
        assert!(b.allocations.is_empty());
    }
    #[test]
    fn hidden_orphaned_level_keeps_its_real_allocations_alive() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let (level, _) = one_level(&mut r, &mut b);
        let mesh = r.create_mesh_object().unwrap();
        r.set_lod(mesh, Some(level), 0, &mut b).unwrap();
        r.set_lod(mesh, Some(level), 2, &mut b).unwrap();
        r.release_mesh_level(level, &mut b).unwrap();
        r.set_lod(mesh, None, 0, &mut b).unwrap();
        assert_eq!(r.resources().mesh_object(mesh).unwrap().num_lods, 0);
        let before = b.events.len();
        r.release_mesh_object(mesh, &mut b).unwrap();
        assert_eq!(b.events.len(), before);
        let state = r.resources().mesh_level(level).unwrap();
        assert_eq!(
            (
                state.references,
                state.external_references(),
                state.orphaned_references()
            ),
            (1, 0, 1)
        );
        assert_eq!(r.vertex_bindings()[0].state, BindingState::Installed);
        r.reclaim_unused_indices(&mut b).unwrap();
        r.reclaim_unused_vertices(&mut b).unwrap();
        assert!(b.releases().is_empty());
        assert_eq!(
            r.release_mesh_level(level, &mut b),
            Err(ManagedOperationError::State(ManagedError::Visual(
                VisualResourceError::MeshLevelReferenceOwned(level)
            )))
        );
    }
    #[test]
    fn later_texture_failure_keeps_earlier_consumed_binding_and_blocks_every_mutation() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let a = r.create_face_group().unwrap();
        let z = r.create_face_group().unwrap();
        let material = r.create_material().unwrap();
        let texture = TextureKey(42);
        b.textures.insert(texture.0, 1);
        r.set_texture(material, Some(texture), &mut b).unwrap();
        r.set_material(z, Some(material), &mut b).unwrap();
        r.release_material(material, &mut b).unwrap();
        r.allocate_group_indices(a, 1, 0, &mut b).unwrap();
        r.allocate_group_indices(z, 1, 0, &mut b).unwrap();
        r.link_next(a, z).unwrap();
        let level = r.create_mesh_level().unwrap();
        r.allocate_level_vertices(level, 3, request(3), Some(a), &mut b)
            .unwrap();
        b.fail_texture = Some(texture);
        assert_eq!(
            r.release_mesh_level(level, &mut b),
            Err(ManagedOperationError::Backend(
                "texture released then failed"
            ))
        );
        assert!(r.is_poisoned());
        assert_eq!(b.textures[&42], 1);
        assert_eq!(r.index_bindings()[0].state, BindingState::Consumed);
        assert_eq!(r.index_bindings()[1].state, BindingState::Installed);
        assert_eq!(r.vertex_bindings()[0].state, BindingState::Installed);
        assert!(r.resources().face_group(a).is_ok());
        assert_eq!(
            r.managers().index_owner(r.indices[0].owner).unwrap().lease,
            OwnerLease::Freed
        );
        let before = b.events.len();
        assert_eq!(r.create_face_group(), Err(ManagedError::Poisoned));
        assert_eq!(
            r.release_mesh_level(level, &mut b),
            Err(ManagedOperationError::State(ManagedError::Poisoned))
        );
        assert_eq!(
            r.reclaim_unused_indices(&mut b),
            Err(ManagedOperationError::State(ManagedError::Poisoned))
        );
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn failed_vb_graphics_release_preserves_manager_free_prefix_without_replaying() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let (level, _) = one_level(&mut r, &mut b);
        b.fail_release = true;
        assert_eq!(
            r.release_mesh_level(level, &mut b),
            Err(ManagedOperationError::Backend(
                "graphics released then failed"
            ))
        );
        assert!(r.is_poisoned());
        assert!(r.managers().is_poisoned());
        assert!(r.resources().is_poisoned());
        assert_eq!(r.vertex_bindings()[0].state, BindingState::Installed);
        assert_eq!(
            r.managers()
                .vertex_owner(r.vertices[0].owner)
                .unwrap()
                .lease,
            OwnerLease::Freed
        );
        assert!(r.managers().vertex_buffers().is_empty());
        assert_eq!(b.releases().len(), 1);
        let before = b.events.len();
        assert!(r.release_mesh_level(level, &mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn graphics_rejection_keeps_pending_owner_and_retry_installs_only_successful_storage() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let group = r.create_face_group().unwrap();
        b.rejects.extend([-1, -2, -3]);
        assert!(!r.allocate_group_indices(group, 1, 0, &mut b).unwrap());
        assert!(!r.is_poisoned());
        let owner = r.index_bindings()[0].owner;
        assert_eq!(r.indices[0].state, BindingState::Pending);
        assert_eq!(
            r.managers().index_owner(owner).unwrap().lease,
            OwnerLease::Vacant
        );
        assert_eq!(
            r.resources().face_group(group).unwrap().index_buffer,
            CompleteIndexBuffer::default()
        );
        assert!(r.index_tokens().is_empty());
        assert!(b.allocations.is_empty());
        assert!(r.allocate_group_indices(group, 1, 0, &mut b).unwrap());
        assert_eq!(r.index_bindings()[0].owner, owner);
        assert_eq!(r.index_tokens().len(), 1);
        r.destroy_face_group(group, &mut b).unwrap();
        r.reclaim_unused_indices(&mut b).unwrap();
        assert!(b.allocations.is_empty());
    }
    #[test]
    fn rejected_vertex_storage_keeps_chain_unowned_until_successful_retry() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let head = r.create_face_group().unwrap();
        let tail = r.create_face_group().unwrap();
        r.allocate_group_indices(head, 1, 0, &mut b).unwrap();
        r.allocate_group_indices(tail, 1, 0, &mut b).unwrap();
        r.link_next(head, tail).unwrap();
        let level = r.create_mesh_level().unwrap();
        b.rejects.extend([-1, -2, -3]);
        assert!(!r
            .allocate_level_vertices(level, 3, request(3), Some(head), &mut b)
            .unwrap());
        assert!(!r.is_poisoned());
        let owner = r.vertex_bindings()[0].owner;
        assert_eq!(
            r.managers().vertex_owner(owner).unwrap().lease,
            OwnerLease::Vacant
        );
        let state = r.resources().mesh_level(level).unwrap();
        assert_eq!(
            (state.num_vertices, state.num_fgroups, state.fgroups),
            (0, 0, None)
        );
        assert_eq!(state.vertex_buffer, CompleteVertexBuffer::default());
        assert_eq!(r.resources().face_group(head).unwrap().owner, None);
        assert_eq!(
            r.resources().face_group(tail).unwrap().owner,
            Some(vr::FaceGroupOwner::Next(head))
        );
        assert!(r.vertex_tokens().is_empty());
        assert!(r
            .allocate_level_vertices(level, 3, request(3), Some(head), &mut b)
            .unwrap());
        assert_eq!(r.vertex_bindings()[0].owner, owner);
        assert_eq!(
            r.resources().face_group(head).unwrap().owner,
            Some(vr::FaceGroupOwner::MeshLevel(level))
        );
        assert_eq!(r.resources().mesh_level(level).unwrap().num_fgroups, 2);
        r.release_mesh_level(level, &mut b).unwrap();
        r.reclaim_unused_indices(&mut b).unwrap();
        assert!(b.allocations.is_empty());
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn preflight_rejects_extent_foreign_owned_and_installed_storage_without_callbacks() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let foreign = ManagedVisualResources::new()
            .unwrap()
            .create_mesh_level()
            .unwrap();
        let group = r.create_face_group().unwrap();
        let level = r.create_mesh_level().unwrap();
        assert!(r.allocate_group_indices(group, 0, 0, &mut b).is_err());
        assert!(r
            .allocate_group_indices(group, u32::MAX, 0, &mut b)
            .is_err());
        assert!(r
            .allocate_level_vertices(foreign, 2, request(2), None, &mut b)
            .is_err());
        assert!(r
            .allocate_level_vertices(level, 3, request(2), None, &mut b)
            .is_err());
        assert!(b.events.is_empty());
        assert!(r.index_bindings().is_empty());
        assert!(r.vertex_bindings().is_empty());
        assert!(!r.is_poisoned());
        let other = r.create_mesh_level().unwrap();
        r.allocate_level_vertices(other, 2, request(2), Some(group), &mut b)
            .unwrap();
        let before = b.events.len();
        assert!(r
            .allocate_level_vertices(level, 2, request(2), Some(group), &mut b)
            .is_err());
        assert!(r
            .allocate_level_vertices(other, 2, request(2), None, &mut b)
            .is_err());
        assert_eq!(b.events.len(), before);
        assert!(!r.is_poisoned());
        r.release_mesh_level(other, &mut b).unwrap();
        assert!(b.allocations.is_empty());
    }
    #[test]
    fn allocation_backend_failure_retains_pending_owner_and_poisons_aggregate() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend {
            fail_block: true,
            ..Backend::default()
        };
        let group = r.create_face_group().unwrap();
        assert_eq!(
            r.allocate_group_indices(group, 1, 0, &mut b),
            Err(ManagedOperationError::Backend("block failure"))
        );
        assert!(r.is_poisoned());
        assert_eq!(r.index_bindings()[0].state, BindingState::Pending);
        assert_eq!(
            r.resources().face_group(group).unwrap().index_buffer,
            CompleteIndexBuffer::default()
        );
        assert!(!b.allocations.is_empty());
        let before = b.events.len();
        assert!(r.allocate_group_indices(group, 1, 0, &mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn exact_null_storage_needs_no_binding_or_external_buffer_success() {
        let mut r = ManagedVisualResources::new().unwrap();
        let mut b = Backend::default();
        let group = r.create_face_group().unwrap();
        r.destroy_face_group(group, &mut b).unwrap();
        let level = r.create_mesh_level().unwrap();
        r.release_mesh_level(level, &mut b).unwrap();
        assert!(b.events.is_empty());
        assert!(!r.is_poisoned());
    }
}
