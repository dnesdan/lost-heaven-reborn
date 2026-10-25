//! Owned plain-material, face-group, mesh-level and mesh-object lifecycles.
//!
//! Original LS3DF SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6:
//! material constructor/Release/SetTexture/SetEnvTexture RVAs 0x2ade0/0x2aef0/
//! 0x2af90/0x2b000; face-group constructor/SetMaterial/deleting destructor
//! 0x30560/0x30580/0x30960; mesh-level constructor/Release 0x30900/0x309b0.
//! Names have original PS2 symbol provenance. This owns reference relationships,
//! not native pointers or GPU managers. Texture references and buffer-block
//! destruction require a concrete backend; there is no implicit no-op backend.
//! Mesh-object constructor/Release/SetLOD/UpdateBoundVolume use RVAs
//! 0x31320/0x313a0/0x31550/0x31400. Animated materials, geometry allocation
//! and rendering are separate.
//! The mesh constructor constant uses the verified PC24 round-to-nearest context;
//! alternate original floating-control modes are not represented.
//!
//! Keys are arena-scoped and never reused. Accepted counts stay within positive
//! signed i32; native negative/wrapped counts are not admitted. Intake helpers
//! transfer unique buffer/chain ownership and are host adapters, not reconstructed
//! loading or AddFGroup methods. A backend failure preserves completed effects
//! and poisons mutation: failed calls may have effects and cannot be replayed.
//! Read-only access remains available for inspecting that interrupted prefix.
//! Resources require explicit release with their backend; Drop cannot perform it.

use crate::mesh_bounds::{update_mesh_bounds, MeshBoundVolume, MeshBoundsError};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ARENA: AtomicU64 = AtomicU64::new(1);

macro_rules! resource_key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name {
            arena: u64,
            slot: u32,
        }
        impl $name {
            pub fn arena(self) -> u64 {
                self.arena
            }
            pub fn slot(self) -> u32 {
                self.slot
            }
        }
    };
}
resource_key!(MaterialKey);
resource_key!(FaceGroupKey);
resource_key!(MeshLevelKey);
resource_key!(MeshObjectKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureKey(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexBufferKey(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndexBufferHandle(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexBufferKey(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexBufferHandle(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawVertexBytesKey(pub u64);

/// COMPLETE_IB_STRUCT projection: the manager owns a block at buffer/base;
/// the graphics handle names its shared allocation, not another owned reference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompleteIndexBuffer {
    pub buffer: Option<IndexBufferKey>,
    pub base: u32,
    pub graphics: Option<IndexBufferHandle>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexBufferPointer {
    Managed(VertexBufferKey),
    RawBytes(RawVertexBytesKey),
}
/// COMPLETE_VB_STRUCT projection. A zero-vertex nonnull buffer denotes raw
/// bytes in the original destructor, so that ownership is typed explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompleteVertexBuffer {
    pub buffer: Option<VertexBufferPointer>,
    pub base: u32,
    pub graphics: Option<VertexBufferHandle>,
}

/// Every operation on one arena must use the same compatible external identity
/// domain and ownership backend; an unrelated backend cannot reuse numeric IDs.
/// Descriptor intake transfers exclusive ownership across *all* arenas sharing
/// that backend. Per-arena alias checks cannot establish cross-arena exclusivity.
/// IDs identify live external resources over this backend's lifetime. Texture
/// retain/release own one reference per material slot, including aliased slots.
/// Manager callbacks free one buffer/base block, never the entire shared graphics
/// handle. They receive null descriptors too, matching the original call boundary.
/// An error may have effects: the arena stops and never retries any operation.
/// Callbacks must not synchronously mutate this resource arena.
pub trait VisualResourceBackend {
    type Error;
    fn retain_texture(&mut self, texture: TextureKey) -> Result<(), Self::Error>;
    fn release_texture(&mut self, texture: TextureKey) -> Result<(), Self::Error>;
    fn texture_flags(&mut self, texture: TextureKey) -> Result<u32, Self::Error>;
    fn free_index_block(
        &mut self,
        owner: FaceGroupKey,
        block: CompleteIndexBuffer,
    ) -> Result<(), Self::Error>;
    fn free_vertex_block(
        &mut self,
        owner: MeshLevelKey,
        block: CompleteVertexBuffer,
    ) -> Result<(), Self::Error>;
    fn free_raw_vertices(
        &mut self,
        owner: MeshLevelKey,
        bytes: RawVertexBytesKey,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKey {
    Material(MaterialKey),
    FaceGroup(FaceGroupKey),
    MeshLevel(MeshLevelKey),
    MeshObject(MeshObjectKey),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceGroupOwner {
    Next(FaceGroupKey),
    MeshLevel(MeshLevelKey),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualResourceError {
    IdentityOverflow,
    HostAllocation,
    ForeignArena,
    UnknownMaterial(MaterialKey),
    UnknownFaceGroup(FaceGroupKey),
    UnknownMeshLevel(MeshLevelKey),
    UnknownMeshObject(MeshObjectKey),
    MeshLevelReferenceOwned(MeshLevelKey),
    InvalidLodIndex(u32),
    UnsupportedLodIndex(u32),
    MeshBounds(MeshBoundsError),
    ReferenceOverflow(ResourceKey),
    MaterialReferenceOwned(MaterialKey),
    FaceGroupOwned(FaceGroupKey),
    InvalidFaceGroupChain,
    StorageAlreadyInstalled,
    InvalidVertexStorage,
    InvalidExternalIdentity,
    BufferBlockAlreadyOwned,
    Poisoned,
}
#[derive(Debug, PartialEq, Eq)]
pub enum VisualResourceOperationError<E> {
    Resource(VisualResourceError),
    Backend(E),
}
impl<E> From<VisualResourceError> for VisualResourceOperationError<E> {
    fn from(value: VisualResourceError) -> Self {
        Self::Resource(value)
    }
}

#[derive(Debug, PartialEq)]
pub struct I3dMaterial {
    pub references: u32,
    pub flags: u32,
    pub texture: Option<TextureKey>,
    pub env_texture: Option<TextureKey>,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub emissive: [f32; 3],
    pub alpha: f32,
    pub env_alpha: f32,
    external_references: u32,
}
impl I3dMaterial {
    pub fn external_references(&self) -> u32 {
        self.external_references
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct I3dFaceGroup {
    pub material: Option<MaterialKey>,
    pub num_faces: u32,
    pub next: Option<FaceGroupKey>,
    pub index_buffer: CompleteIndexBuffer,
    pub owner: Option<FaceGroupOwner>,
}
#[derive(Debug, PartialEq)]
pub struct I3dMeshLevel {
    pub references: u32,
    pub num_vertices: u32,
    pub vertex_buffer: CompleteVertexBuffer,
    pub num_fgroups: u32,
    pub fgroups: Option<FaceGroupKey>,
    pub bbox: [[f32; 3]; 2],
    pub lod_dist: f32,
    pub mesh_flags: u32,
    pub env_scale: f32,
    storage_installed: bool,
    external_references: u32,
    orphaned_references: u32,
}
impl I3dMeshLevel {
    pub fn external_references(&self) -> u32 {
        self.external_references
    }
    /// Native references stranded when a mesh object retires with nonnull
    /// slots above its truncated num_lods. They are never implicitly released.
    pub fn orphaned_references(&self) -> u32 {
        self.orphaned_references
    }
}

#[derive(Debug, PartialEq)]
pub struct I3dMeshObject {
    pub references: u32,
    pub num_lods: u32,
    pub levels: [Option<MeshLevelKey>; 10],
    /// Raw state for diagnosing native-order prefixes. Use bound_volume() to
    /// require an available complete result rather than an interrupted sphere.
    pub bounds: MeshBoundVolume,
    pub u_ofs: f32,
    pub v_ofs: f32,
    pub env_u_ofs: f32,
    pub env_v_ofs: f32,
    bounds_available: bool,
}
impl I3dMeshObject {
    pub fn bound_volume(&self) -> Option<&MeshBoundVolume> {
        (self.references != 0 && self.bounds_available).then_some(&self.bounds)
    }
}

#[derive(Debug)]
pub struct VisualResources {
    identity: u64,
    materials: Vec<Option<I3dMaterial>>,
    face_groups: Vec<Option<I3dFaceGroup>>,
    mesh_levels: Vec<Option<I3dMeshLevel>>,
    mesh_objects: Vec<Option<I3dMeshObject>>,
    poisoned: bool,
}
impl VisualResources {
    pub fn new() -> Result<Self, VisualResourceError> {
        let identity = NEXT_ARENA
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| VisualResourceError::IdentityOverflow)?;
        Ok(Self {
            identity,
            materials: Vec::new(),
            face_groups: Vec::new(),
            mesh_levels: Vec::new(),
            mesh_objects: Vec::new(),
            poisoned: false,
        })
    }
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    fn available(&self) -> Result<(), VisualResourceError> {
        if self.poisoned {
            Err(VisualResourceError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn slot(&self, arena: u64, slot: u32) -> Result<usize, VisualResourceError> {
        if arena != self.identity {
            return Err(VisualResourceError::ForeignArena);
        }
        Ok(slot
            .checked_sub(1)
            .ok_or(VisualResourceError::IdentityOverflow)? as usize)
    }
    pub fn material(&self, key: MaterialKey) -> Result<&I3dMaterial, VisualResourceError> {
        self.materials
            .get(self.slot(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(VisualResourceError::UnknownMaterial(key))
    }
    pub fn face_group(&self, key: FaceGroupKey) -> Result<&I3dFaceGroup, VisualResourceError> {
        self.face_groups
            .get(self.slot(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(VisualResourceError::UnknownFaceGroup(key))
    }
    pub fn mesh_level(&self, key: MeshLevelKey) -> Result<&I3dMeshLevel, VisualResourceError> {
        self.mesh_levels
            .get(self.slot(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(VisualResourceError::UnknownMeshLevel(key))
    }
    pub fn mesh_object(&self, key: MeshObjectKey) -> Result<&I3dMeshObject, VisualResourceError> {
        self.mesh_objects
            .get(self.slot(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(VisualResourceError::UnknownMeshObject(key))
    }
    fn material_mut(&mut self, key: MaterialKey) -> Result<&mut I3dMaterial, VisualResourceError> {
        let i = self.slot(key.arena, key.slot)?;
        self.materials
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(VisualResourceError::UnknownMaterial(key))
    }
    fn face_group_mut(
        &mut self,
        key: FaceGroupKey,
    ) -> Result<&mut I3dFaceGroup, VisualResourceError> {
        let i = self.slot(key.arena, key.slot)?;
        self.face_groups
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(VisualResourceError::UnknownFaceGroup(key))
    }
    fn mesh_level_mut(
        &mut self,
        key: MeshLevelKey,
    ) -> Result<&mut I3dMeshLevel, VisualResourceError> {
        let i = self.slot(key.arena, key.slot)?;
        self.mesh_levels
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(VisualResourceError::UnknownMeshLevel(key))
    }
    fn mesh_object_mut(
        &mut self,
        key: MeshObjectKey,
    ) -> Result<&mut I3dMeshObject, VisualResourceError> {
        let i = self.slot(key.arena, key.slot)?;
        self.mesh_objects
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(VisualResourceError::UnknownMeshObject(key))
    }
    fn push<T>(slots: &mut Vec<Option<T>>, value: T) -> Result<u32, VisualResourceError> {
        let key = u32::try_from(slots.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(VisualResourceError::IdentityOverflow)?;
        slots
            .try_reserve(1)
            .map_err(|_| VisualResourceError::HostAllocation)?;
        slots.push(Some(value));
        Ok(key)
    }
    pub fn create_material(&mut self) -> Result<MaterialKey, VisualResourceError> {
        self.available()?;
        let slot = Self::push(
            &mut self.materials,
            I3dMaterial {
                references: 1,
                external_references: 1,
                flags: 1,
                texture: None,
                env_texture: None,
                ambient: [1.; 3],
                diffuse: [1.; 3],
                emissive: [0.; 3],
                alpha: 1.,
                env_alpha: 1.,
            },
        )?;
        Ok(MaterialKey {
            arena: self.identity,
            slot,
        })
    }
    pub fn create_face_group(&mut self) -> Result<FaceGroupKey, VisualResourceError> {
        self.available()?;
        let slot = Self::push(
            &mut self.face_groups,
            I3dFaceGroup {
                material: None,
                num_faces: 0,
                next: None,
                index_buffer: CompleteIndexBuffer::default(),
                owner: None,
            },
        )?;
        Ok(FaceGroupKey {
            arena: self.identity,
            slot,
        })
    }
    pub fn create_mesh_level(&mut self) -> Result<MeshLevelKey, VisualResourceError> {
        self.available()?;
        let slot = Self::push(
            &mut self.mesh_levels,
            I3dMeshLevel {
                references: 1,
                num_vertices: 0,
                vertex_buffer: CompleteVertexBuffer::default(),
                num_fgroups: 0,
                fgroups: None,
                bbox: [
                    [f32::from_bits(0x5a0e1bca); 3],
                    [f32::from_bits(0xda0e1bca); 3],
                ],
                lod_dist: 0.,
                mesh_flags: 0,
                env_scale: f32::from_bits(0x403504f3),
                storage_installed: false,
                external_references: 1,
                orphaned_references: 0,
            },
        )?;
        Ok(MeshLevelKey {
            arena: self.identity,
            slot,
        })
    }
    pub fn create_mesh_object(&mut self) -> Result<MeshObjectKey, VisualResourceError> {
        self.available()?;
        let slot = Self::push(
            &mut self.mesh_objects,
            I3dMeshObject {
                references: 1,
                num_lods: 0,
                levels: [None; 10],
                bounds: MeshBoundVolume::CONSTRUCTOR,
                u_ofs: 0.,
                v_ofs: 0.,
                env_u_ofs: 0.,
                env_v_ofs: 0.,
                bounds_available: true,
            },
        )?;
        Ok(MeshObjectKey {
            arena: self.identity,
            slot,
        })
    }
    /// Host intake of a known level box, independent of vertex allocation.
    /// Raw bits are retained; UpdateBoundVolume supplies the original X-validity
    /// tests and arithmetic admission. Existing object volumes stay cached.
    pub fn install_mesh_level_bbox(
        &mut self,
        key: MeshLevelKey,
        bbox: [[f32; 3]; 2],
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        self.mesh_level_mut(key)?.bbox = bbox;
        Ok(())
    }
    fn increment(references: u32, key: ResourceKey) -> Result<u32, VisualResourceError> {
        references
            .checked_add(1)
            .filter(|&n| n <= i32::MAX as u32)
            .ok_or(VisualResourceError::ReferenceOverflow(key))
    }
    pub fn retain_material(&mut self, key: MaterialKey) -> Result<(), VisualResourceError> {
        self.available()?;
        let count = Self::increment(self.material(key)?.references, ResourceKey::Material(key))?;
        let material = self.material_mut(key)?;
        material.references = count;
        material.external_references += 1;
        Ok(())
    }
    fn backend<T, E>(
        &mut self,
        result: Result<T, E>,
    ) -> Result<T, VisualResourceOperationError<E>> {
        result.map_err(|error| {
            self.poisoned = true;
            VisualResourceOperationError::Backend(error)
        })
    }
    fn release_material_reference<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        backend: &mut B,
    ) -> Result<u32, VisualResourceOperationError<B::Error>> {
        let count = self.material(key)?.references - 1;
        self.material_mut(key)?.references = count;
        if count != 0 {
            return Ok(count);
        }
        let texture = self.material(key)?.texture;
        let env_texture = self.material(key)?.env_texture;
        if let Some(texture) = texture {
            self.backend(backend.release_texture(texture))?;
        }
        if let Some(texture) = env_texture {
            self.backend(backend.release_texture(texture))?;
        }
        self.materials[(key.slot - 1) as usize] = None;
        Ok(0)
    }
    pub fn release_material<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        backend: &mut B,
    ) -> Result<u32, VisualResourceOperationError<B::Error>> {
        self.available()?;
        if self.material(key)?.external_references == 0 {
            return Err(VisualResourceError::MaterialReferenceOwned(key).into());
        }
        self.material_mut(key)?.external_references -= 1;
        self.release_material_reference(key, backend)
    }
    /// Equality is checked before any release, reference change or flag store.
    /// For replacement the native order is release old, store new, retain new.
    pub fn set_material<B: VisualResourceBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        material: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        self.available()?;
        let old = self.face_group(group)?.material;
        if let Some(key) = material {
            self.material(key)?;
        }
        if old == material {
            return Ok(());
        }
        if let Some(key) = material {
            Self::increment(self.material(key)?.references, ResourceKey::Material(key))?;
        }
        if let Some(key) = old {
            self.release_material_reference(key, backend)?;
        }
        self.face_group_mut(group)?.material = material;
        if let Some(key) = material {
            self.material_mut(key)?.references += 1;
        }
        Ok(())
    }
    pub fn set_texture<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        self.available()?;
        let old = self.material(key)?.texture;
        if texture.is_some_and(|t| t.0 == 0) {
            return Err(VisualResourceError::InvalidExternalIdentity.into());
        }
        if old == texture {
            return Ok(());
        }
        if let Some(old) = old {
            self.backend(backend.release_texture(old))?;
        }
        let material = self.material_mut(key)?;
        material.flags &= 0x1bfbffff;
        material.texture = texture;
        if let Some(texture) = texture {
            self.backend(backend.retain_texture(texture))?;
            let flags = self.backend(backend.texture_flags(texture))?;
            let material = self.material_mut(key)?;
            if flags & 0x6 != 0 {
                material.flags |= 0x40000000;
            }
            if flags & 0x8 != 0 {
                material.flags |= 0x20000000;
                if flags & 0x100 != 0 {
                    material.flags |= 0x40000000;
                }
            }
            material.flags |= 0x40000;
        }
        Ok(())
    }
    pub fn set_env_texture<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        self.available()?;
        let old = self.material(key)?.env_texture;
        if texture.is_some_and(|t| t.0 == 0) {
            return Err(VisualResourceError::InvalidExternalIdentity.into());
        }
        if old == texture {
            return Ok(());
        }
        if let Some(old) = old {
            self.backend(backend.release_texture(old))?;
        }
        self.material_mut(key)?.env_texture = texture;
        if let Some(texture) = texture {
            self.backend(backend.retain_texture(texture))?;
            self.material_mut(key)?.flags |= 0x80000;
        } else {
            self.material_mut(key)?.flags &= !0x80000;
        }
        Ok(())
    }
    /// Host ownership intake. The chain's head must be unattached; a tail has
    /// exactly one incoming owner and cannot be independently destroyed.
    pub fn link_next(
        &mut self,
        group: FaceGroupKey,
        next: FaceGroupKey,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        let head = self.face_group(group)?;
        let tail = self.face_group(next)?;
        if head.owner.is_some() {
            return Err(VisualResourceError::FaceGroupOwned(group));
        }
        if tail.owner.is_some() {
            return Err(VisualResourceError::FaceGroupOwned(next));
        }
        if head.next.is_some() {
            return Err(VisualResourceError::InvalidFaceGroupChain);
        }
        let chain = self.chain(next)?;
        if chain.contains(&group) {
            return Err(VisualResourceError::InvalidFaceGroupChain);
        }
        self.face_group_mut(group)?.next = Some(next);
        self.face_group_mut(next)?.owner = Some(FaceGroupOwner::Next(group));
        Ok(())
    }
    fn chain(&self, head: FaceGroupKey) -> Result<Vec<FaceGroupKey>, VisualResourceError> {
        let mut chain = Vec::new();
        let mut at = Some(head);
        while let Some(key) = at {
            if chain.len() >= self.face_groups.len() {
                return Err(VisualResourceError::InvalidFaceGroupChain);
            }
            let group = self.face_group(key)?;
            if let Some(&previous) = chain.last() {
                if group.owner != Some(FaceGroupOwner::Next(previous)) {
                    return Err(VisualResourceError::InvalidFaceGroupChain);
                }
            }
            if let Some(material) = group.material {
                self.material(material)?;
            }
            chain
                .try_reserve(1)
                .map_err(|_| VisualResourceError::HostAllocation)?;
            chain.push(key);
            at = group.next;
        }
        Ok(chain)
    }
    pub fn install_index_block(
        &mut self,
        group: FaceGroupKey,
        num_faces: u32,
        block: CompleteIndexBuffer,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        let current = self.face_group(group)?;
        if current.num_faces != 0 || current.index_buffer != CompleteIndexBuffer::default() {
            return Err(VisualResourceError::StorageAlreadyInstalled);
        }
        if num_faces > i32::MAX as u32
            || block.buffer.is_some_and(|b| b.0 == 0)
            || block.graphics.is_some_and(|b| b.0 == 0)
        {
            return Err(VisualResourceError::InvalidExternalIdentity);
        }
        if block.buffer.is_some()
            && self
                .face_groups
                .iter()
                .flatten()
                .any(|g| g.index_buffer.buffer == block.buffer && g.index_buffer.base == block.base)
        {
            return Err(VisualResourceError::BufferBlockAlreadyOwned);
        }
        let current = self.face_group_mut(group)?;
        current.num_faces = num_faces;
        current.index_buffer = block;
        Ok(())
    }
    fn check_mesh_level_storage_fresh(
        &self,
        level: MeshLevelKey,
    ) -> Result<(), VisualResourceError> {
        let current = self.mesh_level(level)?;
        if current.storage_installed
            || current.references != 1
            || current.external_references != 1
            || current.orphaned_references != 0
        {
            return Err(VisualResourceError::StorageAlreadyInstalled);
        }
        Ok(())
    }
    /// Shared host-intake preflight. No ownership changes or backend effects.
    /// The bridge uses this before allocating a manager block; the installer
    /// repeats it to preserve its independent public admission contract.
    pub(crate) fn check_mesh_level_storage_intake(
        &self,
        level: MeshLevelKey,
        head: Option<FaceGroupKey>,
    ) -> Result<u32, VisualResourceError> {
        self.available()?;
        self.check_mesh_level_storage_fresh(level)?;
        let count = if let Some(head) = head {
            if self.face_group(head)?.owner.is_some() {
                return Err(VisualResourceError::FaceGroupOwned(head));
            }
            u32::try_from(self.chain(head)?.len())
                .map_err(|_| VisualResourceError::IdentityOverflow)?
        } else {
            0
        };
        Ok(count)
    }
    pub fn install_mesh_level_storage(
        &mut self,
        level: MeshLevelKey,
        num_vertices: u32,
        block: CompleteVertexBuffer,
        head: Option<FaceGroupKey>,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        self.check_mesh_level_storage_fresh(level)?;
        if num_vertices > i32::MAX as u32 || block.graphics.is_some_and(|b| b.0 == 0) {
            return Err(VisualResourceError::InvalidExternalIdentity);
        }
        match block.buffer {
            Some(VertexBufferPointer::Managed(key)) if num_vertices == 0 || key.0 == 0 => {
                return Err(VisualResourceError::InvalidVertexStorage);
            }
            Some(VertexBufferPointer::RawBytes(key)) if num_vertices != 0 || key.0 == 0 => {
                return Err(VisualResourceError::InvalidVertexStorage);
            }
            _ => {}
        }
        if block.buffer.is_some()
            && self.mesh_levels.iter().flatten().any(|level| {
                match (level.vertex_buffer.buffer, block.buffer) {
                    (
                        Some(VertexBufferPointer::Managed(a)),
                        Some(VertexBufferPointer::Managed(b)),
                    ) => a == b && level.vertex_buffer.base == block.base,
                    (
                        Some(VertexBufferPointer::RawBytes(a)),
                        Some(VertexBufferPointer::RawBytes(b)),
                    ) => a == b,
                    _ => false,
                }
            })
        {
            return Err(VisualResourceError::BufferBlockAlreadyOwned);
        }
        let count = self.check_mesh_level_storage_intake(level, head)?;
        if let Some(head) = head {
            self.face_group_mut(head)?.owner = Some(FaceGroupOwner::MeshLevel(level));
        }
        let current = self.mesh_level_mut(level)?;
        current.num_vertices = num_vertices;
        current.vertex_buffer = block;
        current.num_fgroups = count;
        current.fgroups = head;
        current.storage_installed = true;
        Ok(())
    }
    fn destroy_chain<B: VisualResourceBackend + ?Sized>(
        &mut self,
        chain: &[FaceGroupKey],
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        for &key in chain {
            let group = self.face_group(key)?;
            let material = group.material;
            let block = group.index_buffer;
            if let Some(material) = material {
                self.release_material_reference(material, backend)?;
            }
            self.backend(backend.free_index_block(key, block))?;
        }
        // Recursive native delete frees allocations tail first, after all
        // material/IB callbacks. Preserve that order without host recursion.
        for &key in chain.iter().rev() {
            self.face_groups[(key.slot - 1) as usize] = None;
        }
        Ok(())
    }
    pub fn destroy_face_group<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        self.available()?;
        if self.face_group(key)?.owner.is_some() {
            return Err(VisualResourceError::FaceGroupOwned(key).into());
        }
        let chain = self.chain(key)?;
        self.destroy_chain(&chain, backend)
    }
    pub fn retain_mesh_level(&mut self, key: MeshLevelKey) -> Result<(), VisualResourceError> {
        self.available()?;
        let count = Self::increment(
            self.mesh_level(key)?.references,
            ResourceKey::MeshLevel(key),
        )?;
        let level = self.mesh_level_mut(key)?;
        level.references = count;
        level.external_references += 1;
        Ok(())
    }
    pub fn release_mesh_level<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MeshLevelKey,
        backend: &mut B,
    ) -> Result<u32, VisualResourceOperationError<B::Error>> {
        self.available()?;
        if self.mesh_level(key)?.external_references == 0 {
            return Err(VisualResourceError::MeshLevelReferenceOwned(key).into());
        }
        // Reserve/validate destruction traversal before consuming external
        // ownership. A host allocation rejection has no native-order prefix.
        let chain = if self.mesh_level(key)?.references == 1 {
            self.mesh_level(key)?
                .fgroups
                .map(|head| self.chain(head))
                .transpose()?
        } else {
            None
        };
        self.mesh_level_mut(key)?.external_references -= 1;
        self.release_mesh_level_reference(key, chain, backend)
    }
    fn release_mesh_level_reference<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MeshLevelKey,
        prepared_chain: Option<Vec<FaceGroupKey>>,
        backend: &mut B,
    ) -> Result<u32, VisualResourceOperationError<B::Error>> {
        let level = self.mesh_level(key)?;
        let chain = if level.references == 1 {
            match prepared_chain {
                Some(chain) => Some(chain),
                None => level.fgroups.map(|head| self.chain(head)).transpose()?,
            }
        } else {
            None
        };
        let count = self.mesh_level(key)?.references - 1;
        self.mesh_level_mut(key)?.references = count;
        if count != 0 {
            return Ok(count);
        }
        if let Some(chain) = chain {
            self.destroy_chain(&chain, backend)?;
        }
        let level = self.mesh_level(key)?;
        let block = level.vertex_buffer;
        if let Some(VertexBufferPointer::RawBytes(bytes)) = block.buffer {
            self.backend(backend.free_raw_vertices(key, bytes))?;
        } else {
            self.backend(backend.free_vertex_block(key, block))?;
        }
        self.mesh_levels[(key.slot - 1) as usize] = None;
        Ok(0)
    }
    pub fn retain_mesh_object(&mut self, key: MeshObjectKey) -> Result<(), VisualResourceError> {
        self.available()?;
        let count = Self::increment(
            self.mesh_object(key)?.references,
            ResourceKey::MeshObject(key),
        )?;
        self.mesh_object_mut(key)?.references = count;
        Ok(())
    }
    /// Original SetLOD for nonnegative signed indices. Native invalid-parameter
    /// results for indices 10..=i32::MAX become InvalidLodIndex. Higher u32 inputs
    /// represent excluded native negative-index memory access and are rejected
    /// as UnsupportedLodIndex before any ownership changes.
    /// Null replacement truncates num_lods but never clears higher slots.
    pub fn set_lod<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        level: Option<MeshLevelKey>,
        index: u32,
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        self.available()?;
        if index > i32::MAX as u32 {
            return Err(VisualResourceError::UnsupportedLodIndex(index).into());
        }
        if index >= 10 {
            return Err(VisualResourceError::InvalidLodIndex(index).into());
        }
        let old = self.mesh_object(key)?.levels[index as usize];
        if let Some(level) = level {
            self.mesh_level(level)?;
        }
        if old != level {
            if let Some(level) = level {
                Self::increment(
                    self.mesh_level(level)?.references,
                    ResourceKey::MeshLevel(level),
                )?;
            }
            if let Some(old) = old {
                if let Err(error) = self.release_mesh_level_reference(old, None, backend) {
                    self.mesh_object_mut(key)?.bounds_available = false;
                    return Err(error);
                }
            }
            self.mesh_object_mut(key)?.levels[index as usize] = level;
            if let Some(level) = level {
                self.mesh_level_mut(level)?.references += 1;
                let object = self.mesh_object_mut(key)?;
                if object.num_lods <= index {
                    object.num_lods = index + 1;
                }
            } else {
                let object = self.mesh_object_mut(key)?;
                if object.num_lods > index {
                    object.num_lods = index;
                }
            }
        } else if level.is_none() {
            return Ok(());
        }
        // Ownership stores precede original UpdateBoundVolume. Host numeric
        // guards retain them and preceding projected bound stores, then poison;
        // the original has no arithmetic-error branch.
        self.update_mesh_bound_volume(key)?;
        Ok(())
    }
    pub fn update_mesh_bound_volume(
        &mut self,
        key: MeshObjectKey,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        let object = self.mesh_object(key)?;
        let count = object.num_lods as usize;
        let mut boxes = [None; 10];
        for (index, bbox) in boxes.iter_mut().enumerate().take(count) {
            if let Some(level) = object.levels[index] {
                *bbox = Some(self.mesh_level(level)?.bbox);
            }
        }
        let object = self.mesh_object_mut(key)?;
        object.bounds_available = false;
        let result = update_mesh_bounds(&mut object.bounds, &boxes[..count]);
        if let Err(error) = result {
            self.poisoned = true;
            return Err(VisualResourceError::MeshBounds(error));
        }
        self.mesh_object_mut(key)?.bounds_available = true;
        Ok(())
    }
    /// Forward release rereads the active count, as the original does, within
    /// the serialized nonreentrant backend domain. Higher hidden slots are not
    /// released. Their native leaked references remain as explicit orphan counts.
    pub fn release_mesh_object<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        backend: &mut B,
    ) -> Result<u32, VisualResourceOperationError<B::Error>> {
        self.available()?;
        let count = self.mesh_object(key)?.references - 1;
        self.mesh_object_mut(key)?.references = count;
        if count != 0 {
            return Ok(count);
        }
        // Cache bits remain diagnostic while owned levels are being destroyed.
        self.mesh_object_mut(key)?.bounds_available = false;
        let result = (|| {
            let mut index = 0;
            while index < self.mesh_object(key)?.num_lods {
                if let Some(level) = self.mesh_object(key)?.levels[index as usize] {
                    self.release_mesh_level_reference(level, None, backend)?;
                }
                index += 1;
            }
            let object = self.mesh_object(key)?;
            let hidden = object.levels;
            let start = object.num_lods as usize;
            for level in hidden[start..].iter().flatten() {
                // Bookkeeping changes ownership, not the original ref count.
                self.mesh_level_mut(*level)?.orphaned_references += 1;
            }
            self.mesh_objects[(key.slot - 1) as usize] = None;
            Ok::<_, VisualResourceOperationError<B::Error>>(0)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}

impl VisualResources {
    /// Scalar stores at LOD Duplicate31000, before vertex and group copying.
    /// This narrow hook does not duplicate any resource reference or allocation.
    pub(crate) fn copy_mesh_level_scalar_fields(
        &mut self,
        destination: MeshLevelKey,
        source: MeshLevelKey,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        let flags = self.mesh_level(source)?.mesh_flags;
        self.mesh_level_mut(destination)?.mesh_flags = flags;
        let env = self.mesh_level(source)?.env_scale;
        self.mesh_level_mut(destination)?.env_scale = env;
        let distance = self.mesh_level(source)?.lod_dist;
        self.mesh_level_mut(destination)?.lod_dist = distance;
        Ok(())
    }
    /// Constructor bounds reset after Mesh Duplicate315e0 clears all slots.
    pub(crate) fn reset_empty_mesh_bounds(
        &mut self,
        mesh: MeshObjectKey,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        let record = self.mesh_object_mut(mesh)?;
        if record.levels.iter().any(Option::is_some) {
            return Err(VisualResourceError::InvalidFaceGroupChain);
        }
        record.num_lods = 0;
        record.bounds = MeshBoundVolume::CONSTRUCTOR;
        record.u_ofs = 0.;
        record.v_ofs = 0.;
        record.bounds_available = true;
        Ok(())
    }
}

impl VisualResources {
    /// Duplicate315e0 releases all ten slots before clearing the array. Bounds
    /// remain untouched for a nonnull source until the subsequent SetLOD calls.
    pub(crate) fn clear_duplicate_mesh<B: VisualResourceBackend + ?Sized>(
        &mut self,
        mesh: MeshObjectKey,
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        self.available()?;
        let slots = self.mesh_object(mesh)?.levels;
        let result = (|| {
            for level in slots.into_iter().flatten() {
                self.release_mesh_level_reference(level, None, backend)?;
            }
            let r = self.mesh_object_mut(mesh)?;
            r.num_lods = 0;
            r.levels = [None; 10];
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Material Copy2b0b0. Raw scalar setters preserve NaNs and signed zeros;
    /// texture setters supply the actual release/retain and flag effects.
    pub(crate) fn copy_material<B: VisualResourceBackend + ?Sized>(
        &mut self,
        destination: MaterialKey,
        source: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<i32, VisualResourceOperationError<B::Error>> {
        self.available()?;
        self.material(destination)?;
        let Some(source) = source else {
            return Ok(0x80000007u32 as i32);
        };
        let flags = self.material(source)?.flags & 0xfbffffff;
        self.material_mut(destination)?.flags = flags;
        let value = self.material(source)?.alpha;
        self.material_mut(destination)?.alpha = value;
        let value = self.material(source)?.ambient;
        self.material_mut(destination)?.ambient = value;
        let value = self.material(source)?.diffuse;
        self.material_mut(destination)?.diffuse = value;
        let value = self.material(source)?.emissive;
        self.material_mut(destination)?.emissive = value;
        let value = self.material(source)?.env_alpha;
        self.material_mut(destination)?.env_alpha = value;
        let texture = self.material(source)?.texture;
        self.set_texture(destination, texture, backend)?;
        let texture = self.material(source)?.env_texture;
        self.set_env_texture(destination, texture, backend)?;
        Ok(0)
    }
}

impl VisualResources {
    pub(crate) fn lookup_mesh(&self, slot: u32) -> Result<MeshObjectKey, VisualResourceError> {
        let key = MeshObjectKey {
            arena: self.identity,
            slot,
        };
        self.mesh_object(key)?;
        Ok(key)
    }
    pub(crate) fn lookup_lod(&self, slot: u32) -> Result<MeshLevelKey, VisualResourceError> {
        let key = MeshLevelKey {
            arena: self.identity,
            slot,
        };
        self.mesh_level(key)?;
        Ok(key)
    }
    pub(crate) fn lookup_face_group(&self, slot: u32) -> Result<FaceGroupKey, VisualResourceError> {
        let key = FaceGroupKey {
            arena: self.identity,
            slot,
        };
        self.face_group(key)?;
        Ok(key)
    }
    pub(crate) fn lookup_material(&self, slot: u32) -> Result<MaterialKey, VisualResourceError> {
        let key = MaterialKey {
            arena: self.identity,
            slot,
        };
        self.material(key)?;
        Ok(key)
    }
}

impl VisualResources {
    /// Reached AddFGroup31220 branch with the original append argument set.
    /// Material-derived flags/scale precede linking; a numeric exclusion keeps
    /// those completed stores and leaves the new group unattached.
    pub(crate) fn append_duplicate_group(
        &mut self,
        level: MeshLevelKey,
        group: FaceGroupKey,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        let new = self.face_group(group)?;
        if new.owner.is_some() || new.next.is_some() {
            return Err(VisualResourceError::FaceGroupOwned(group));
        }
        let flags = new
            .material
            .map(|key| self.material(key).map(|m| m.flags))
            .transpose()?;
        let count = self
            .mesh_level(level)?
            .num_fgroups
            .checked_add(1)
            .ok_or(VisualResourceError::IdentityOverflow)?;
        if let Some(flags) = flags {
            if flags & 0x40000 != 0 {
                self.mesh_level_mut(level)?.mesh_flags |= 0x10;
            }
            if flags & 0x8000000 != 0 {
                self.mesh_level_mut(level)?.mesh_flags |= 0x20;
            }
            if flags & 0x80000 != 0 {
                let r = self.mesh_level_mut(level)?;
                r.mesh_flags = ((r.mesh_flags | 8) & !7) | ((flags >> 12) & 7);
                let scale = f32::from_bits(0x403504f3) / (flags & 255) as f32;
                if !(scale == 0. || scale.is_normal()) {
                    self.poisoned = true;
                    return Err(VisualResourceError::MeshBounds(
                        MeshBoundsError::NumericDomain("LOD environment scale"),
                    ));
                }
                self.mesh_level_mut(level)?.env_scale = scale;
            }
        }
        if let Some(head) = self.mesh_level(level)?.fgroups {
            let tail = *self
                .chain(head)?
                .last()
                .ok_or(VisualResourceError::InvalidFaceGroupChain)?;
            self.face_group_mut(tail)?.next = Some(group);
            self.face_group_mut(group)?.owner = Some(FaceGroupOwner::Next(tail));
        } else {
            self.mesh_level_mut(level)?.fgroups = Some(group);
            self.face_group_mut(group)?.owner = Some(FaceGroupOwner::MeshLevel(level));
        }
        self.mesh_level_mut(level)?.num_fgroups = count;
        Ok(())
    }
}

impl VisualResources {
    /// Host admission of the serialized scalar block read by Load2bd50. Raw
    /// flags precede the original contiguous ambient/diffuse/emissive/alpha
    /// fields; env-alpha exists only when serialized flag0x80000 is present.
    /// Native texture-load flag derivation remains a separate backend contract.
    pub fn install_loaded_material_scalars(
        &mut self,
        key: MaterialKey,
        source: &lhr_assets::Material,
        flags: u32,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        let material = self.material_mut(key)?;
        material.flags = flags;
        material.ambient = source.ambient;
        material.diffuse = source.diffuse;
        material.emissive = source.emission;
        material.alpha = source.opacity;
        if source.flags & 0x80000 != 0 {
            material.env_alpha = source
                .reflection
                .ok_or(VisualResourceError::InvalidExternalIdentity)?;
        }
        Ok(())
    }
}

impl VisualResources {
    pub(crate) fn install_loaded_lod_distance(
        &mut self,
        level: MeshLevelKey,
        distance: f32,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        self.mesh_level_mut(level)?.lod_dist = distance;
        Ok(())
    }
}

impl VisualResources {
    /// Fresh loaded-material slot admission. Native Load receives references
    /// directly from the cache; it does not call SetTexture or SetEnvTexture.
    pub(crate) fn adopt_loaded_material_texture<B: VisualResourceBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        environment: bool,
        backend: &mut B,
    ) -> Result<(), VisualResourceOperationError<B::Error>> {
        self.available()?;
        let current = self.material(key)?;
        if (if environment {
            current.env_texture
        } else {
            current.texture
        })
        .is_some()
        {
            return Err(VisualResourceError::StorageAlreadyInstalled.into());
        }
        if let Some(texture) = texture {
            self.backend(backend.retain_texture(texture))?;
        }
        if environment {
            self.material_mut(key)?.env_texture = texture;
        } else {
            self.material_mut(key)?.texture = texture;
        }
        Ok(())
    }
}

impl VisualResources {
    pub(crate) fn clear_mesh_level_environment(
        &mut self,
        level: MeshLevelKey,
    ) -> Result<(), VisualResourceError> {
        self.available()?;
        self.mesh_level_mut(level)?.mesh_flags &= !8;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Retain(TextureKey),
        Release(TextureKey),
        Flags(TextureKey),
        Index(FaceGroupKey, CompleteIndexBuffer),
        Vertex(MeshLevelKey, CompleteVertexBuffer),
        Raw(MeshLevelKey, RawVertexBytesKey),
    }
    #[derive(Default)]
    struct Backend {
        textures: BTreeMap<u64, (u32, u32)>,
        indices: BTreeSet<(u64, u32)>,
        vertices: BTreeSet<(u64, u32)>,
        raw: BTreeMap<u64, Vec<u8>>,
        events: Vec<Event>,
        fail_before: Option<usize>,
        fail_after_release: bool,
    }
    impl Backend {
        fn texture(&mut self, key: u64, flags: u32) -> TextureKey {
            assert!(self.textures.insert(key, (1, flags)).is_none());
            TextureKey(key)
        }
        fn event(&mut self, event: Event) -> Result<(), &'static str> {
            self.events.push(event);
            if self.fail_before == Some(self.events.len()) {
                Err("unavailable")
            } else {
                Ok(())
            }
        }
    }
    impl VisualResourceBackend for Backend {
        type Error = &'static str;
        fn retain_texture(&mut self, texture: TextureKey) -> Result<(), Self::Error> {
            self.event(Event::Retain(texture))?;
            self.textures
                .get_mut(&texture.0)
                .ok_or("unknown texture")?
                .0 += 1;
            Ok(())
        }
        fn release_texture(&mut self, texture: TextureKey) -> Result<(), Self::Error> {
            self.event(Event::Release(texture))?;
            let count = &mut self
                .textures
                .get_mut(&texture.0)
                .ok_or("unknown texture")?
                .0;
            *count -= 1;
            if *count == 0 {
                self.textures.remove(&texture.0);
            }
            if self.fail_after_release {
                Err("interrupted after effect")
            } else {
                Ok(())
            }
        }
        fn texture_flags(&mut self, texture: TextureKey) -> Result<u32, Self::Error> {
            self.event(Event::Flags(texture))?;
            self.textures
                .get(&texture.0)
                .map(|v| v.1)
                .ok_or("unknown texture")
        }
        fn free_index_block(
            &mut self,
            owner: FaceGroupKey,
            block: CompleteIndexBuffer,
        ) -> Result<(), Self::Error> {
            self.event(Event::Index(owner, block))?;
            if let Some(buffer) = block.buffer {
                assert!(self.indices.remove(&(buffer.0, block.base)));
            }
            Ok(())
        }
        fn free_vertex_block(
            &mut self,
            owner: MeshLevelKey,
            block: CompleteVertexBuffer,
        ) -> Result<(), Self::Error> {
            self.event(Event::Vertex(owner, block))?;
            if let Some(VertexBufferPointer::Managed(buffer)) = block.buffer {
                assert!(self.vertices.remove(&(buffer.0, block.base)));
            }
            Ok(())
        }
        fn free_raw_vertices(
            &mut self,
            owner: MeshLevelKey,
            bytes: RawVertexBytesKey,
        ) -> Result<(), Self::Error> {
            self.event(Event::Raw(owner, bytes))?;
            assert!(self.raw.remove(&bytes.0).is_some());
            Ok(())
        }
    }

    #[test]
    fn shared_material_chain_releases_real_dependencies_in_native_order() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let texture = backend.texture(1, 0x108);
        let a = resources.create_material().unwrap();
        let b = resources.create_material().unwrap();
        resources
            .set_texture(a, Some(texture), &mut backend)
            .unwrap();
        resources
            .set_env_texture(a, Some(texture), &mut backend)
            .unwrap();
        backend.release_texture(texture).unwrap();
        let groups = [
            resources.create_face_group().unwrap(),
            resources.create_face_group().unwrap(),
            resources.create_face_group().unwrap(),
        ];
        for (group, material) in [(groups[0], a), (groups[1], b), (groups[2], a)] {
            resources
                .set_material(group, Some(material), &mut backend)
                .unwrap();
        }
        resources.release_material(a, &mut backend).unwrap();
        resources.release_material(b, &mut backend).unwrap();
        let first = CompleteIndexBuffer {
            buffer: Some(IndexBufferKey(5)),
            base: 0,
            graphics: Some(IndexBufferHandle(9)),
        };
        let third = CompleteIndexBuffer {
            buffer: Some(IndexBufferKey(5)),
            base: 30,
            graphics: Some(IndexBufferHandle(9)),
        };
        backend.indices.extend([(5, 0), (5, 30)]);
        // Face count zero does not suppress ownership of an actual index block.
        resources.install_index_block(groups[0], 0, first).unwrap();
        resources.install_index_block(groups[2], 10, third).unwrap();
        resources.link_next(groups[1], groups[2]).unwrap();
        resources.link_next(groups[0], groups[1]).unwrap();
        let level = resources.create_mesh_level().unwrap();
        backend.raw.insert(7, vec![1, 2, 3]);
        resources
            .install_mesh_level_storage(
                level,
                0,
                CompleteVertexBuffer {
                    buffer: Some(VertexBufferPointer::RawBytes(RawVertexBytesKey(7))),
                    ..Default::default()
                },
                Some(groups[0]),
            )
            .unwrap();
        resources.mesh_level_mut(level).unwrap().num_fgroups = 0;
        resources.retain_mesh_level(level).unwrap();
        backend.events.clear();
        assert_eq!(resources.release_mesh_level(level, &mut backend), Ok(1));
        assert!(backend.events.is_empty());
        assert_eq!(resources.release_mesh_level(level, &mut backend), Ok(0));
        assert_eq!(
            backend.events,
            [
                Event::Index(groups[0], first),
                Event::Index(groups[1], CompleteIndexBuffer::default()),
                Event::Release(texture),
                Event::Release(texture),
                Event::Index(groups[2], third),
                Event::Raw(level, RawVertexBytesKey(7))
            ]
        );
        assert!(
            backend.textures.is_empty() && backend.indices.is_empty() && backend.raw.is_empty()
        );
        assert!(matches!(
            resources.material(a),
            Err(VisualResourceError::UnknownMaterial(_))
        ));
        assert!(matches!(
            resources.material(b),
            Err(VisualResourceError::UnknownMaterial(_))
        ));
        for group in groups {
            assert!(resources.face_group(group).is_err());
        }
        assert!(resources.mesh_level(level).is_err());
    }

    #[test]
    fn same_material_with_sole_owned_reference_is_noop_and_replacement_releases_old_first() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let old = resources.create_material().unwrap();
        let new = resources.create_material().unwrap();
        let texture = backend.texture(1, 0);
        resources
            .set_texture(old, Some(texture), &mut backend)
            .unwrap();
        backend.release_texture(texture).unwrap();
        let group = resources.create_face_group().unwrap();
        resources
            .set_material(group, Some(old), &mut backend)
            .unwrap();
        resources.release_material(old, &mut backend).unwrap();
        backend.events.clear();
        resources
            .set_material(group, Some(old), &mut backend)
            .unwrap();
        assert_eq!(resources.material(old).unwrap().references, 1);
        assert!(backend.events.is_empty());
        assert_eq!(
            resources.release_material(old, &mut backend),
            Err(VisualResourceError::MaterialReferenceOwned(old).into())
        );
        resources
            .set_material(group, Some(new), &mut backend)
            .unwrap();
        assert_eq!(backend.events, [Event::Release(texture)]);
        assert!(resources.material(old).is_err());
        assert_eq!(resources.material(new).unwrap().references, 2);
        assert_eq!(resources.face_group(group).unwrap().material, Some(new));
        resources.release_material(new, &mut backend).unwrap();
        resources.set_material(group, None, &mut backend).unwrap();
        assert!(resources.material(new).is_err());
        resources.destroy_face_group(group, &mut backend).unwrap();
    }

    #[test]
    fn presentation_consumes_effective_set_texture_flags_instead_of_raw_alpha_bits() {
        use lhr_render::{AlphaMode, FrontFace, MaterialQueue, RenderMaterial};
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let material = resources.create_material().unwrap();
        resources.material_mut(material).unwrap().flags = 0xe0000001;
        let opaque = backend.texture(1, 0);
        resources
            .set_texture(material, Some(opaque), &mut backend)
            .unwrap();
        let RenderMaterial::Original(state) = crate::presentation_material::live_material(
            resources.material(material).unwrap(),
            true,
            [0.; 3],
            FrontFace::CounterClockwise,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(
            (state.alpha, state.queue),
            (AlphaMode::Opaque, MaterialQueue::Opaque)
        );
        let keyed_mipmaps = backend.texture(2, 0x108);
        resources
            .set_texture(material, Some(keyed_mipmaps), &mut backend)
            .unwrap();
        let RenderMaterial::Original(state) = crate::presentation_material::live_material(
            resources.material(material).unwrap(),
            true,
            [0.; 3],
            FrontFace::CounterClockwise,
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(
            (state.alpha, state.queue),
            (AlphaMode::Blend, MaterialQueue::Transparent)
        );
    }

    #[test]
    fn texture_flags_alias_slots_and_equality_match_material_contract() {
        for (flags, expected) in [
            (0, 0x40001),
            (2, 0x40040001),
            (4, 0x40040001),
            (8, 0x20040001),
            (0x108, 0x60040001),
            (0x106, 0x40040001),
        ] {
            let mut resources = VisualResources::new().unwrap();
            let mut backend = Backend::default();
            let material = resources.create_material().unwrap();
            let texture = backend.texture(1, flags);
            resources
                .set_texture(material, Some(texture), &mut backend)
                .unwrap();
            assert_eq!(resources.material(material).unwrap().flags, expected);
            resources
                .set_env_texture(material, Some(texture), &mut backend)
                .unwrap();
            assert_eq!(
                resources.material(material).unwrap().flags,
                expected | 0x80000
            );
            assert_eq!(backend.textures[&1].0, 3);
            // Same-pointer paths preserve even inconsistent already-stored bits.
            resources.material_mut(material).unwrap().flags = 0x12345678;
            backend.events.clear();
            resources
                .set_texture(material, Some(texture), &mut backend)
                .unwrap();
            resources
                .set_env_texture(material, Some(texture), &mut backend)
                .unwrap();
            assert!(backend.events.is_empty());
            assert_eq!(resources.material(material).unwrap().flags, 0x12345678);
            resources.set_texture(material, None, &mut backend).unwrap();
            assert_eq!(
                resources.material(material).unwrap().flags,
                0x12345678 & 0x1bfbffff
            );
            resources
                .set_env_texture(material, None, &mut backend)
                .unwrap();
            assert_eq!(backend.textures[&1].0, 1);
            assert_eq!(
                resources.material(material).unwrap().flags,
                0x12345678 & 0x1bfbffff & !0x80000
            );
            resources.release_material(material, &mut backend).unwrap();
            backend.release_texture(texture).unwrap();
        }
    }

    #[test]
    fn failed_second_dependency_preserves_prefix_and_cannot_replay() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let material = resources.create_material().unwrap();
        let a = backend.texture(1, 0);
        let b = backend.texture(2, 0);
        resources
            .set_texture(material, Some(a), &mut backend)
            .unwrap();
        resources
            .set_env_texture(material, Some(b), &mut backend)
            .unwrap();
        backend.release_texture(a).unwrap();
        backend.release_texture(b).unwrap();
        backend.events.clear();
        backend.fail_before = Some(2);
        assert_eq!(
            resources.release_material(material, &mut backend),
            Err(VisualResourceOperationError::Backend("unavailable"))
        );
        assert!(resources.is_poisoned());
        assert!(!backend.textures.contains_key(&1));
        assert_eq!(backend.textures[&2].0, 1);
        let interrupted = resources.material(material).unwrap();
        assert_eq!(interrupted.references, 0);
        assert_eq!(
            (interrupted.texture, interrupted.env_texture),
            (Some(a), Some(b))
        );
        assert_eq!(
            resources.release_material(material, &mut backend),
            Err(VisualResourceError::Poisoned.into())
        );
        assert_eq!(backend.events, [Event::Release(a), Event::Release(b)]);
    }

    #[test]
    fn failed_release_with_effect_does_not_publish_or_retain_replacement() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let material = resources.create_material().unwrap();
        let old = backend.texture(1, 0x108);
        let new = backend.texture(2, 0);
        resources
            .set_texture(material, Some(old), &mut backend)
            .unwrap();
        backend.release_texture(old).unwrap();
        let before_flags = resources.material(material).unwrap().flags;
        backend.events.clear();
        backend.fail_after_release = true;
        assert_eq!(
            resources.set_texture(material, Some(new), &mut backend),
            Err(VisualResourceOperationError::Backend(
                "interrupted after effect"
            ))
        );
        assert!(!backend.textures.contains_key(&1));
        assert_eq!(backend.textures[&2].0, 1);
        assert_eq!(resources.material(material).unwrap().texture, Some(old));
        assert_eq!(resources.material(material).unwrap().flags, before_flags);
        assert_eq!(
            resources.set_env_texture(material, Some(new), &mut backend),
            Err(VisualResourceError::Poisoned.into())
        );
        assert_eq!(backend.events, [Event::Release(old)]);
    }

    #[test]
    fn foreign_dead_and_owned_keys_reject_before_effects_and_slots_never_reuse() {
        let mut a = VisualResources::new().unwrap();
        let mut b = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let material = a.create_material().unwrap();
        let foreign = b.create_material().unwrap();
        assert_eq!(material.slot(), foreign.slot());
        let head = a.create_face_group().unwrap();
        let tail = a.create_face_group().unwrap();
        assert_eq!(
            a.set_material(head, Some(foreign), &mut backend),
            Err(VisualResourceError::ForeignArena.into())
        );
        a.link_next(head, tail).unwrap();
        assert_eq!(
            a.destroy_face_group(tail, &mut backend),
            Err(VisualResourceError::FaceGroupOwned(tail).into())
        );
        assert!(a.link_next(tail, head).is_err());
        assert!(a.link_next(head, head).is_err());
        assert!(backend.events.is_empty());
        a.release_material(material, &mut backend).unwrap();
        assert_eq!(
            a.set_material(head, Some(material), &mut backend),
            Err(VisualResourceError::UnknownMaterial(material).into())
        );
        let replacement = a.create_material().unwrap();
        assert!(replacement.slot() > material.slot());
        a.destroy_face_group(head, &mut backend).unwrap();
        assert!(a.destroy_face_group(head, &mut backend).is_err());
    }

    #[test]
    fn block_ownership_tracks_manager_base_instead_of_shared_graphics_handle() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let a = resources.create_face_group().unwrap();
        let b = resources.create_face_group().unwrap();
        let first = CompleteIndexBuffer {
            buffer: Some(IndexBufferKey(1)),
            base: 0,
            graphics: Some(IndexBufferHandle(2)),
        };
        resources.install_index_block(a, 1, first).unwrap();
        assert_eq!(
            resources.install_index_block(b, 1, first),
            Err(VisualResourceError::BufferBlockAlreadyOwned)
        );
        resources
            .install_index_block(b, 1, CompleteIndexBuffer { base: 3, ..first })
            .unwrap();
        let level_a = resources.create_mesh_level().unwrap();
        let level_b = resources.create_mesh_level().unwrap();
        let vertices = CompleteVertexBuffer {
            buffer: Some(VertexBufferPointer::Managed(VertexBufferKey(3))),
            base: 0,
            graphics: Some(VertexBufferHandle(4)),
        };
        resources
            .install_mesh_level_storage(level_a, 3, vertices, Some(a))
            .unwrap();
        assert_eq!(
            resources.install_mesh_level_storage(level_b, 3, vertices, Some(b)),
            Err(VisualResourceError::BufferBlockAlreadyOwned)
        );
        assert_eq!(resources.face_group(b).unwrap().owner, None);
        resources
            .install_mesh_level_storage(
                level_b,
                3,
                CompleteVertexBuffer {
                    base: 3,
                    ..vertices
                },
                Some(b),
            )
            .unwrap();
        assert_eq!(
            resources.destroy_face_group(a, &mut backend),
            Err(VisualResourceError::FaceGroupOwned(a).into())
        );
        backend.indices.extend([(1, 0), (1, 3)]);
        backend.vertices.extend([(3, 0), (3, 3)]);
        resources.release_mesh_level(level_a, &mut backend).unwrap();
        resources.release_mesh_level(level_b, &mut backend).unwrap();
        assert!(backend.indices.is_empty() && backend.vertices.is_empty());
        assert_eq!(
            backend.events.last(),
            Some(&Event::Vertex(
                level_b,
                CompleteVertexBuffer {
                    base: 3,
                    ..vertices
                }
            ))
        );
    }

    #[test]
    fn null_managed_cleanup_and_invalid_raw_branch_are_explicit() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let level = resources.create_mesh_level().unwrap();
        let raw = CompleteVertexBuffer {
            buffer: Some(VertexBufferPointer::RawBytes(RawVertexBytesKey(1))),
            ..Default::default()
        };
        assert_eq!(
            resources.install_mesh_level_storage(level, 1, raw, None),
            Err(VisualResourceError::InvalidVertexStorage)
        );
        assert_eq!(resources.release_mesh_level(level, &mut backend), Ok(0));
        assert_eq!(
            backend.events,
            [Event::Vertex(level, CompleteVertexBuffer::default())]
        );
    }

    #[test]
    fn signed_positive_domain_is_checked_before_replacing_old_ownership() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let old = resources.create_material().unwrap();
        let new = resources.create_material().unwrap();
        let group = resources.create_face_group().unwrap();
        resources
            .set_material(group, Some(old), &mut backend)
            .unwrap();
        resources.release_material(old, &mut backend).unwrap();
        resources.material_mut(new).unwrap().references = i32::MAX as u32;
        assert_eq!(
            resources.set_material(group, Some(new), &mut backend),
            Err(VisualResourceError::ReferenceOverflow(ResourceKey::Material(new)).into())
        );
        assert_eq!(resources.face_group(group).unwrap().material, Some(old));
        assert_eq!(resources.material(old).unwrap().references, 1);
        assert!(backend.events.is_empty());
        let level = resources.create_mesh_level().unwrap();
        resources.mesh_level_mut(level).unwrap().references = i32::MAX as u32;
        assert_eq!(
            resources.retain_mesh_level(level),
            Err(VisualResourceError::ReferenceOverflow(
                ResourceKey::MeshLevel(level)
            ))
        );
    }

    #[test]
    fn deep_owned_chain_teardown_uses_no_recursive_host_stack() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let tail = resources.create_face_group().unwrap();
        let mut head = tail;
        for _ in 0..1024 {
            let new = resources.create_face_group().unwrap();
            resources.link_next(new, head).unwrap();
            head = new;
        }
        resources.destroy_face_group(head, &mut backend).unwrap();
        assert_eq!(backend.events.len(), 1025);
        assert!(resources.face_group(tail).is_err());
    }
    #[test]
    fn mesh_objects_share_duplicate_level_slots_without_consuming_external_ownership() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let level = resources.create_mesh_level().unwrap();
        resources
            .install_mesh_level_bbox(level, [[-1., -2., -3.], [1., 2., 3.]])
            .unwrap();
        let first = resources.create_mesh_object().unwrap();
        let second = resources.create_mesh_object().unwrap();
        resources
            .set_lod(first, Some(level), 0, &mut backend)
            .unwrap();
        resources
            .set_lod(first, Some(level), 2, &mut backend)
            .unwrap();
        resources
            .set_lod(second, Some(level), 1, &mut backend)
            .unwrap();
        assert_eq!(resources.mesh_level(level).unwrap().references, 4);
        resources.release_mesh_level(level, &mut backend).unwrap();
        assert_eq!(
            resources.mesh_level(level).unwrap().external_references(),
            0
        );
        assert_eq!(
            resources.release_mesh_level(level, &mut backend),
            Err(VisualResourceError::MeshLevelReferenceOwned(level).into())
        );
        assert_eq!(resources.release_mesh_object(first, &mut backend), Ok(0));
        assert_eq!(resources.mesh_level(level).unwrap().references, 1);
        assert!(backend.events.is_empty());
        assert_eq!(resources.release_mesh_object(second, &mut backend), Ok(0));
        assert_eq!(
            backend.events,
            [Event::Vertex(level, CompleteVertexBuffer::default())]
        );
        assert!(resources.mesh_level(level).is_err());
        assert!(resources.mesh_object(first).is_err());
        let replacement = resources.create_mesh_object().unwrap();
        assert!(replacement.slot() > second.slot());
    }

    #[test]
    fn truncation_retains_hidden_references_and_final_release_preserves_native_orphans() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let mesh = resources.create_mesh_object().unwrap();
        let shared = resources.create_mesh_level().unwrap();
        let middle = resources.create_mesh_level().unwrap();
        resources
            .install_mesh_level_bbox(shared, [[-1.; 3], [1.; 3]])
            .unwrap();
        resources
            .install_mesh_level_bbox(middle, [[-2.; 3], [2.; 3]])
            .unwrap();
        resources
            .set_lod(mesh, Some(shared), 0, &mut backend)
            .unwrap();
        resources
            .set_lod(mesh, Some(middle), 1, &mut backend)
            .unwrap();
        resources
            .set_lod(mesh, Some(shared), 3, &mut backend)
            .unwrap();
        resources.release_mesh_level(shared, &mut backend).unwrap();
        resources.release_mesh_level(middle, &mut backend).unwrap();
        resources.set_lod(mesh, None, 1, &mut backend).unwrap();
        assert_eq!(resources.mesh_object(mesh).unwrap().num_lods, 1);
        assert_eq!(resources.mesh_object(mesh).unwrap().levels[3], Some(shared));
        assert_eq!(resources.mesh_level(shared).unwrap().references, 2);
        // A same nonnull hidden pointer does not extend num_lods.
        resources
            .set_lod(mesh, Some(shared), 3, &mut backend)
            .unwrap();
        assert_eq!(resources.mesh_object(mesh).unwrap().num_lods, 1);
        backend.events.clear();
        resources.release_mesh_object(mesh, &mut backend).unwrap();
        assert!(backend.events.is_empty());
        let leaked = resources.mesh_level(shared).unwrap();
        assert_eq!(
            (
                leaked.references,
                leaked.external_references(),
                leaked.orphaned_references()
            ),
            (1, 0, 1)
        );
        assert_eq!(
            resources.release_mesh_level(shared, &mut backend),
            Err(VisualResourceError::MeshLevelReferenceOwned(shared).into())
        );
        assert_eq!(
            resources.install_mesh_level_storage(shared, 0, CompleteVertexBuffer::default(), None),
            Err(VisualResourceError::StorageAlreadyInstalled)
        );
        resources.retain_mesh_level(shared).unwrap();
        assert_eq!(resources.release_mesh_level(shared, &mut backend), Ok(1));
        assert_eq!(
            resources.mesh_level(shared).unwrap().orphaned_references(),
            1
        );
    }

    #[test]
    fn hidden_slot_overwrite_and_reexpansion_preserve_each_owned_reference() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let mesh = resources.create_mesh_object().unwrap();
        let a = resources.create_mesh_level().unwrap();
        let b = resources.create_mesh_level().unwrap();
        let c = resources.create_mesh_level().unwrap();
        for (index, level) in [(0, a), (1, b), (3, c)] {
            resources
                .install_mesh_level_bbox(level, [[-(index as f32 + 1.); 3], [index as f32 + 1.; 3]])
                .unwrap();
            resources
                .set_lod(mesh, Some(level), index, &mut backend)
                .unwrap();
        }
        resources.set_lod(mesh, None, 0, &mut backend).unwrap();
        assert_eq!(resources.mesh_object(mesh).unwrap().num_lods, 0);
        assert_eq!(resources.mesh_object(mesh).unwrap().levels[1], Some(b));
        resources.set_lod(mesh, Some(a), 4, &mut backend).unwrap();
        assert_eq!(resources.mesh_object(mesh).unwrap().num_lods, 5);
        assert_eq!(
            resources.mesh_object(mesh).unwrap().bounds.bbox,
            [[-4.; 3], [4.; 3]]
        );
        resources.set_lod(mesh, None, 1, &mut backend).unwrap();
        assert_eq!(resources.mesh_object(mesh).unwrap().num_lods, 1);
        // Clearing a hidden nonnull pointer releases it without extending count.
        resources.set_lod(mesh, None, 3, &mut backend).unwrap();
        assert_eq!(resources.mesh_object(mesh).unwrap().num_lods, 1);
        assert_eq!(resources.mesh_level(c).unwrap().references, 1);
        resources.set_lod(mesh, None, 4, &mut backend).unwrap();
        for level in [a, b, c] {
            assert_eq!(resources.release_mesh_level(level, &mut backend), Ok(0));
        }
        resources.release_mesh_object(mesh, &mut backend).unwrap();
    }

    #[test]
    fn set_lod_same_null_skips_bounds_but_same_nonnull_refreshes_without_retain() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let mesh = resources.create_mesh_object().unwrap();
        let level = resources.create_mesh_level().unwrap();
        resources
            .install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
            .unwrap();
        resources
            .set_lod(mesh, Some(level), 3, &mut backend)
            .unwrap();
        resources.release_mesh_level(level, &mut backend).unwrap();
        let old = resources.mesh_object(mesh).unwrap().bounds;
        resources
            .install_mesh_level_bbox(level, [[-5.; 3], [5.; 3]])
            .unwrap();
        resources.set_lod(mesh, None, 0, &mut backend).unwrap();
        assert_eq!(resources.mesh_object(mesh).unwrap().num_lods, 4);
        assert_eq!(resources.mesh_object(mesh).unwrap().bounds, old);
        resources
            .set_lod(mesh, Some(level), 3, &mut backend)
            .unwrap();
        assert_eq!(
            resources.mesh_object(mesh).unwrap().bounds.bbox,
            [[-5.; 3], [5.; 3]]
        );
        assert_eq!(resources.mesh_level(level).unwrap().references, 1);
        assert!(backend.events.is_empty());
        resources.release_mesh_object(mesh, &mut backend).unwrap();
    }

    #[test]
    fn lod_replacement_bounds_rejection_retains_new_ownership_and_unavailable_volume_prefix() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let mesh = resources.create_mesh_object().unwrap();
        let old = resources.create_mesh_level().unwrap();
        let new = resources.create_mesh_level().unwrap();
        resources
            .install_mesh_level_bbox(old, [[0.; 3], [2.; 3]])
            .unwrap();
        resources
            .install_mesh_level_bbox(new, [[-1e20; 3], [1e20; 3]])
            .unwrap();
        resources.set_lod(mesh, Some(old), 0, &mut backend).unwrap();
        resources.release_mesh_level(old, &mut backend).unwrap();
        let radius = resources.mesh_object(mesh).unwrap().bounds.radius;
        assert_eq!(
            resources.set_lod(mesh, Some(new), 0, &mut backend),
            Err(VisualResourceError::MeshBounds(MeshBoundsError::NumericDomain("radius")).into())
        );
        assert!(resources.is_poisoned());
        assert!(resources.mesh_level(old).is_err());
        assert_eq!(resources.mesh_level(new).unwrap().references, 2);
        let object = resources.mesh_object(mesh).unwrap();
        assert_eq!(object.levels[0], Some(new));
        assert_eq!(object.num_lods, 1);
        assert_eq!(object.bounds.bbox, [[-1e20; 3], [1e20; 3]]);
        assert_eq!(object.bounds.center, [0.; 3]);
        assert_eq!(object.bounds.radius, radius);
        assert!(object.bound_volume().is_none());
        assert_eq!(
            resources.release_mesh_object(mesh, &mut backend),
            Err(VisualResourceError::Poisoned.into())
        );
        assert_eq!(
            backend.events,
            [Event::Vertex(old, CompleteVertexBuffer::default())]
        );
    }

    #[test]
    fn object_release_backend_failure_preserves_completed_prefix_and_never_replays() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let mesh = resources.create_mesh_object().unwrap();
        let a = resources.create_mesh_level().unwrap();
        let b = resources.create_mesh_level().unwrap();
        resources.set_lod(mesh, Some(a), 0, &mut backend).unwrap();
        resources.set_lod(mesh, Some(b), 1, &mut backend).unwrap();
        resources.release_mesh_level(a, &mut backend).unwrap();
        resources.release_mesh_level(b, &mut backend).unwrap();
        backend.fail_before = Some(2);
        assert_eq!(
            resources.release_mesh_object(mesh, &mut backend),
            Err(VisualResourceOperationError::Backend("unavailable"))
        );
        assert!(resources.is_poisoned());
        assert!(resources.mesh_level(a).is_err());
        assert_eq!(resources.mesh_level(b).unwrap().references, 0);
        let object = resources.mesh_object(mesh).unwrap();
        assert_eq!(object.references, 0);
        assert!(object.bound_volume().is_none());
        assert_eq!(object.levels[..2], [Some(a), Some(b)]);
        assert_eq!(
            resources.release_mesh_object(mesh, &mut backend),
            Err(VisualResourceError::Poisoned.into())
        );
        assert_eq!(
            backend.events,
            [
                Event::Vertex(a, CompleteVertexBuffer::default()),
                Event::Vertex(b, CompleteVertexBuffer::default())
            ]
        );
    }

    #[test]
    fn set_lod_backend_failure_does_not_publish_or_retain_replacement() {
        let mut resources = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let mesh = resources.create_mesh_object().unwrap();
        let old = resources.create_mesh_level().unwrap();
        let new = resources.create_mesh_level().unwrap();
        resources.set_lod(mesh, Some(old), 0, &mut backend).unwrap();
        resources.release_mesh_level(old, &mut backend).unwrap();
        backend.fail_before = Some(1);
        assert_eq!(
            resources.set_lod(mesh, Some(new), 0, &mut backend),
            Err(VisualResourceOperationError::Backend("unavailable"))
        );
        assert_eq!(resources.mesh_level(old).unwrap().references, 0);
        assert_eq!(resources.mesh_level(new).unwrap().references, 1);
        assert_eq!(resources.mesh_object(mesh).unwrap().levels[0], Some(old));
        assert!(resources
            .mesh_object(mesh)
            .unwrap()
            .bound_volume()
            .is_none());
        assert!(resources.is_poisoned());
    }

    #[test]
    fn mesh_lod_indices_foreign_keys_and_overflow_reject_before_native_order_effects() {
        let mut resources = VisualResources::new().unwrap();
        let mut foreign = VisualResources::new().unwrap();
        let mut backend = Backend::default();
        let mesh = resources.create_mesh_object().unwrap();
        let old = resources.create_mesh_level().unwrap();
        resources.set_lod(mesh, Some(old), 0, &mut backend).unwrap();
        resources.release_mesh_level(old, &mut backend).unwrap();
        assert_eq!(
            resources.install_mesh_level_storage(old, 0, CompleteVertexBuffer::default(), None),
            Err(VisualResourceError::StorageAlreadyInstalled)
        );
        assert_eq!(
            resources.set_lod(mesh, None, 10, &mut backend),
            Err(VisualResourceError::InvalidLodIndex(10).into())
        );
        assert_eq!(
            resources.set_lod(mesh, None, u32::MAX, &mut backend),
            Err(VisualResourceError::UnsupportedLodIndex(u32::MAX).into())
        );
        let foreign_level = foreign.create_mesh_level().unwrap();
        assert_eq!(
            resources.set_lod(mesh, Some(foreign_level), 0, &mut backend),
            Err(VisualResourceError::ForeignArena.into())
        );
        let full = resources.create_mesh_level().unwrap();
        resources.mesh_level_mut(full).unwrap().references = i32::MAX as u32;
        assert_eq!(
            resources.set_lod(mesh, Some(full), 0, &mut backend),
            Err(VisualResourceError::ReferenceOverflow(ResourceKey::MeshLevel(full)).into())
        );
        assert_eq!(resources.mesh_object(mesh).unwrap().levels[0], Some(old));
        assert_eq!(resources.mesh_level(old).unwrap().references, 1);
        assert!(backend.events.is_empty());
        resources.mesh_object_mut(mesh).unwrap().references = i32::MAX as u32;
        assert_eq!(
            resources.retain_mesh_object(mesh),
            Err(VisualResourceError::ReferenceOverflow(
                ResourceKey::MeshObject(mesh)
            ))
        );
    }
}
