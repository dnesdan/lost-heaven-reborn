//! Plain per-object LOD ownership at the original LS3DF I3D boundary.
//!
//! PC LS3DF.dll SHA-256:
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Scoped original entries (RVAs): lod constructor 0x36c70 represented fields;
//! nondeleting cleanup 0x36ce0; scalar destructor 0x36cb0 cleanup followed here
//! by host retirement, excluding native allocation/delete and pointer return;
//! I3D_object destructor 0x36840 ten-LOD/mesh ownership prefix only, excluding
//! its visual/frame base cleanup. The I3D_object class name has original PS2
//! linker-symbol provenance; PC behavior/layout are independently evidenced.
//!
//! `Lod` is the PC diagnostic class name. Lighting/dirty field labels follow
//! descriptive upstream names; they are not claimed original linker symbols.
//! `I3dObjectResources` projects resource ownership plus ordinary cur_lod/fade
//! render state of the same I3D_object; it is not a complete native layout.
//! This host arena does not implement native Create, SetMesh, visual/frame base
//! destruction or native uploads. The bounded unconstructed lit subtype owns
//! its separate constructor state and inherited-buffer projections below.
//! Primitive retirement uses host tombstones, without native allocator/delete or pointer-return credit.
//!
//! One compatible backend domain and serialized nonreentrant callbacks are
//! required throughout. Explicit cleanup must occur before that backend goes
//! away; Drop cannot run these required effects. Read-only views remain raw
//! diagnostics after an interrupted cleanup; old slots can name retired LODs.
#[path = "lit_lod_storage.rs"]
mod lit_lod_storage;
#[path = "lit_object.rs"]
mod lit_object;
use crate::buffer_managers::{VertexOwnerKey, VertexRequest};
use crate::managed_visual_resources::{
    BindingState, ManagedError, ManagedOperationError, ManagedVisualBackend, ManagedVisualResources,
};
use crate::visual_resources::{
    CompleteVertexBuffer, FaceGroupKey, MaterialKey, MeshLevelKey, MeshObjectKey, TextureKey,
};
pub use lit_lod_storage::{
    LitLodStorage, LitSourceBinding, LitStorageCounts, LitStoragePhase, RetiringLitArrays,
};
pub use lit_object::{
    lightmap_atlas, saved_lightmaps, LitLevelResources, LitObjectResources, ObjectVisualType,
};

macro_rules! key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
key!(PlainLodKey);
key!(ObjectResourceKey);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectError {
    Poisoned,
    Managed(ManagedError),
    ForeignArena,
    IdentityOverflow,
    HostAllocation,
    UnknownLod(PlainLodKey),
    UnknownObject(ObjectResourceKey),
    InvalidIndex(u32),
    UnsupportedIndex(u32),
    SlotOccupied,
    LodOwned(PlainLodKey),
    LodDestructed(PlainLodKey),
    StorageAlreadyInstalled,
    MeshReferenceOwned(MeshObjectKey),
    MeshCompatibilityUnavailable,
    UnsupportedVisualSubtype,
}
impl From<ManagedError> for ObjectError {
    fn from(e: ManagedError) -> Self {
        Self::Managed(e)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ObjectOperationError<E> {
    State(ObjectError),
    Backend(E),
}
impl<E> From<ObjectError> for ObjectOperationError<E> {
    fn from(e: ObjectError) -> Self {
        Self::State(e)
    }
}
impl<E> From<ManagedError> for ObjectOperationError<E> {
    fn from(e: ManagedError) -> Self {
        Self::State(e.into())
    }
}
fn managed_result<T, E>(
    r: Result<T, ManagedOperationError<E>>,
) -> Result<T, ObjectOperationError<E>> {
    r.map_err(|e| match e {
        ManagedOperationError::State(e) => e.into(),
        ManagedOperationError::Backend(e) => ObjectOperationError::Backend(e),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LodState {
    Constructed,
    Destructed,
}
/// Represented fields written by the plain PC lod constructor. Padding byte
/// 0x2f and the machine vtable pointer are deliberately not fabricated fields.
#[derive(Debug, PartialEq)]
pub struct Lod {
    pub lights_changed: u32,
    pub update_count: u32,
    pub light_key: u32,
    pub vertex_buffer: CompleteVertexBuffer,
    pub u_ofs: f32,
    pub v_ofs: f32,
    pub env_u_ofs: f32,
    pub env_v_ofs: f32,
    pub dirty: bool,
    pub alpha_dirty: bool,
    pub alpha: u8,
    /// Host ownership and cleanup state, separate from the native field layout.
    pub owner: Option<ObjectResourceKey>,
    pub state: LodState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectCleanupPhase {
    Alive,
    DestroyingLods,
    ReleasingMesh,
}
#[derive(Debug, PartialEq)]
pub struct I3dObjectResources {
    visual_type: ObjectVisualType,
    /// Raw native mesh pointer projection; it is not nulled during destruction.
    pub mesh: Option<MeshObjectKey>,
    /// Ordinary render state on this same owner; constructor values from source.
    pub cur_lod: u32,
    pub fade: f32,
    /// All ten slots are cleared together only after all LOD deletes succeed.
    pub lods: [Option<PlainLodKey>; 10],
    pub phase: ObjectCleanupPhase,
    pub(crate) lit_object: Option<Box<LitObjectResources>>,
}
impl I3dObjectResources {
    pub fn visual_type(&self) -> ObjectVisualType {
        self.visual_type
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodBinding {
    pub lod: PlainLodKey,
    pub owner: VertexOwnerKey,
    pub state: BindingState,
}
/// Descriptive host arena, not an additional LS3DF DLL or full I3D_object class.
pub struct ObjectResources {
    managed: ManagedVisualResources,
    lods: Vec<Option<Lod>>,
    objects: Vec<Option<I3dObjectResources>>,
    bindings: Vec<LodBinding>,
    lit_storage: Vec<LitLodStorage>,
    poisoned: bool,
}
impl ObjectResources {
    pub fn new() -> Result<Self, ObjectError> {
        Ok(Self {
            managed: ManagedVisualResources::new()?,
            lods: Vec::new(),
            objects: Vec::new(),
            bindings: Vec::new(),
            lit_storage: Vec::new(),
            poisoned: false,
        })
    }
    pub fn managed(&self) -> &ManagedVisualResources {
        &self.managed
    }
    pub fn lod_bindings(&self) -> &[LodBinding] {
        &self.bindings
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned || self.managed.is_poisoned()
    }
    fn available(&self) -> Result<(), ObjectError> {
        if self.is_poisoned() {
            Err(ObjectError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn index(&self, arena: u64, slot: u32) -> Result<usize, ObjectError> {
        if arena != self.managed.resources().identity() {
            Err(ObjectError::ForeignArena)
        } else {
            slot.checked_sub(1)
                .map(|v| v as usize)
                .ok_or(ObjectError::IdentityOverflow)
        }
    }
    pub fn lod(&self, key: PlainLodKey) -> Result<&Lod, ObjectError> {
        self.lods
            .get(self.index(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(ObjectError::UnknownLod(key))
    }
    pub fn object(&self, key: ObjectResourceKey) -> Result<&I3dObjectResources, ObjectError> {
        self.objects
            .get(self.index(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(ObjectError::UnknownObject(key))
    }
    fn reserve<T>(v: &mut Vec<T>) -> Result<u32, ObjectError> {
        let slot = u32::try_from(v.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(ObjectError::IdentityOverflow)?;
        v.try_reserve(1).map_err(|_| ObjectError::HostAllocation)?;
        Ok(slot)
    }
    pub fn create_plain_lod(&mut self) -> Result<PlainLodKey, ObjectError> {
        self.available()?;
        let slot = Self::reserve(&mut self.lods)?;
        self.lods.push(Some(Lod {
            lights_changed: u32::MAX,
            update_count: u32::MAX,
            light_key: u32::MAX,
            vertex_buffer: CompleteVertexBuffer::default(),
            u_ofs: 0.,
            v_ofs: 0.,
            env_u_ofs: 0.,
            env_v_ofs: 0.,
            dirty: false,
            alpha_dirty: false,
            alpha: 255,
            owner: None,
            state: LodState::Constructed,
        }));
        Ok(PlainLodKey {
            arena: self.managed.resources().identity(),
            slot,
        })
    }
    /// Fresh attachment retains exactly one shared mesh reference. This is a
    /// host intake adapter, not the original constructor or general SetMesh.
    pub fn create_object_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, ObjectError> {
        self.available()?;
        let slot = Self::reserve(&mut self.objects)?;
        if let Some(mesh) = mesh {
            self.managed.retain_mesh_object(mesh)?;
        }
        self.objects.push(Some(I3dObjectResources {
            visual_type: ObjectVisualType::Plain,
            mesh,
            cur_lod: 0,
            fade: 1.,
            lods: [None; 10],
            phase: ObjectCleanupPhase::Alive,
            lit_object: None,
        }));
        Ok(ObjectResourceKey {
            arena: self.managed.resources().identity(),
            slot,
        })
    }
    /// Unique transfer into one empty slot. Signed-negative native pointer
    /// indices are outside this safe host adapter, as are slots beyond nine.
    pub fn adopt_plain_lod(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
        lod: PlainLodKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        if index > i32::MAX as u32 {
            return Err(ObjectError::UnsupportedIndex(index));
        }
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index));
        }
        if self.object(object)?.visual_type() != ObjectVisualType::Plain {
            return Err(ObjectError::UnsupportedVisualSubtype);
        }
        if self.object(object)?.lods[index as usize].is_some() {
            return Err(ObjectError::SlotOccupied);
        }
        let current = self.lod(lod)?;
        if current.owner.is_some() {
            return Err(ObjectError::LodOwned(lod));
        }
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod));
        }
        self.lods[(lod.slot - 1) as usize].as_mut().unwrap().owner = Some(object);
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lods[index as usize] = Some(lod);
        Ok(())
    }
    /// Fresh managed storage only; no original lod::Create or upload claim.
    pub fn allocate_plain_lod_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        request: VertexRequest,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        let current = self.lod(lod)?;
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod).into());
        }
        if current.vertex_buffer != CompleteVertexBuffer::default() {
            return Err(ObjectError::StorageAlreadyInstalled.into());
        }
        if request.count == 0 || request.count > i32::MAX as u32 {
            return Err(ManagedError::InvalidExtent.into());
        }
        Self::reserve(&mut self.bindings)?;
        let position = if let Some(i) = self
            .bindings
            .iter()
            .position(|b| b.lod == lod && b.state != BindingState::Consumed)
        {
            if self.bindings[i].state != BindingState::Pending {
                return Err(ObjectError::StorageAlreadyInstalled.into());
            }
            i
        } else {
            let owner = self.managed.create_vertex_storage_owner()?;
            self.bindings.push(LodBinding {
                lod,
                owner,
                state: BindingState::Pending,
            });
            self.bindings.len() - 1
        };
        let owner = self.bindings[position].owner;
        let result = managed_result(
            self.managed
                .allocate_vertex_storage(owner, request, backend),
        );
        match result {
            Ok(Some(block)) => {
                self.lods[(lod.slot - 1) as usize]
                    .as_mut()
                    .unwrap()
                    .vertex_buffer = block;
                self.bindings[position].state = BindingState::Installed;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }
    fn cleanup_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        let current = self.lod(lod)?;
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod).into());
        }
        let descriptor = current.vertex_buffer;
        let position = self
            .bindings
            .iter()
            .position(|b| b.lod == lod && b.state != BindingState::Consumed);
        let owner = position.map(|i| self.bindings[i].owner);
        let result = managed_result(self.managed.free_vertex_storage(owner, descriptor, backend));
        if let Err(e) = result {
            self.poisoned = true;
            return Err(e);
        }
        if let Some(i) = position {
            self.bindings[i].state = BindingState::Consumed;
        }
        self.lods[(lod.slot - 1) as usize].as_mut().unwrap().state = LodState::Destructed;
        Ok(())
    }
    /// Plain nondeleting cleanup. Descriptor words remain stale, as natively;
    /// host state prevents a second cleanup or adoption of the destructed record.
    pub fn destruct_plain_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        if self.lod(lod)?.owner.is_some() {
            return Err(ObjectError::LodOwned(lod).into());
        }
        self.cleanup_lod(lod, backend)
    }
    /// Cleanup plus host record retirement. No native allocator/pointer return
    /// is represented. An already nondeleting-destructed record is rejected.
    pub fn destroy_plain_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.destruct_plain_lod(lod, backend)?;
        self.lods[(lod.slot - 1) as usize] = None;
        Ok(())
    }
    /// All ten unique LOD deletes precede the single whole-array clear. Mesh
    /// Release follows with its raw pointer unchanged; visual/frame base cleanup
    /// remains outside this ownership projection. A failure stops every mutation.
    pub fn destroy_object_resources<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let current = self.object(object)?;
        // The private arena maintains unique slots and live mesh ownership.
        // Preflight all keys before the first manager/backend effect.
        for lod in current.lods.iter().flatten().chain(
            current
                .lit_object
                .iter()
                .flat_map(|lit| lit.levels.iter())
                .flat_map(|level| level.lods.iter().flatten()),
        ) {
            let state = self.lod(*lod)?;
            if state.owner != Some(object) {
                return Err(ObjectError::LodOwned(*lod).into());
            }
            if state.state != LodState::Constructed {
                return Err(ObjectError::LodDestructed(*lod).into());
            }
        }
        if let Some(mesh) = current.mesh {
            self.managed
                .resources()
                .mesh_object(mesh)
                .map_err(ManagedError::from)?;
        }
        let has_lit = current.lit_object.is_some();
        let index = (object.slot - 1) as usize;
        self.objects[index].as_mut().unwrap().phase = ObjectCleanupPhase::DestroyingLods;
        let result = (|| {
            // Derived destructor runs before I3D_object base cleanup.
            if has_lit {
                for level in 0..6 {
                    for lod_index in 0..10 {
                        let lod = self.objects[index]
                            .as_ref()
                            .unwrap()
                            .lit_object
                            .as_ref()
                            .unwrap()
                            .levels[level]
                            .lods[lod_index];
                        if let Some(lod) = lod {
                            self.cleanup_saved_lit_lod(object, level, lod_index, backend)?;
                            self.lods[(lod.slot - 1) as usize] = None;
                            self.objects[index]
                                .as_mut()
                                .unwrap()
                                .lit_object
                                .as_mut()
                                .unwrap()
                                .levels[level]
                                .lods[lod_index] = None;
                        }
                        self.objects[index]
                            .as_mut()
                            .unwrap()
                            .lit_object
                            .as_mut()
                            .unwrap()
                            .levels[level]
                            .saved_lods[lod_index] = None;
                    }
                }
            }
            for i in 0..10 {
                if let Some(lod) = self.objects[index].as_ref().unwrap().lods[i] {
                    self.cleanup_lod(lod, backend)?;
                    self.lods[(lod.slot - 1) as usize] = None;
                }
            }
            let record = self.objects[index].as_mut().unwrap();
            record.lods = [None; 10];
            record.phase = ObjectCleanupPhase::ReleasingMesh;
            if let Some(mesh) = record.mesh {
                managed_result(self.managed.release_mesh_object(mesh, backend))?;
            }
            self.objects[index] = None;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn owned_mesh_references(&self, mesh: MeshObjectKey) -> usize {
        self.objects
            .iter()
            .flatten()
            .filter(|o| o.mesh == Some(mesh) && o.phase != ObjectCleanupPhase::ReleasingMesh)
            .count()
    }
    /// External Release cannot consume a reference retained by a live object.
    pub fn release_mesh_object<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        mesh: MeshObjectKey,
        backend: &mut B,
    ) -> Result<u32, ObjectOperationError<B::Error>> {
        self.available()?;
        let total = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(ManagedError::from)?
            .references;
        if u64::from(total) <= self.owned_mesh_references(mesh) as u64 {
            return Err(ObjectError::MeshReferenceOwned(mesh).into());
        }
        managed_result(self.managed.release_mesh_object(mesh, backend))
    }
}

impl ObjectResources {
    pub fn create_material(&mut self) -> Result<MaterialKey, ObjectError> {
        self.available()?;
        self.managed.create_material().map_err(Into::into)
    }
    pub fn create_face_group(&mut self) -> Result<FaceGroupKey, ObjectError> {
        self.available()?;
        self.managed.create_face_group().map_err(Into::into)
    }
    pub fn create_mesh_level(&mut self) -> Result<MeshLevelKey, ObjectError> {
        self.available()?;
        self.managed.create_mesh_level().map_err(Into::into)
    }
    pub fn create_mesh_object(&mut self) -> Result<MeshObjectKey, ObjectError> {
        self.available()?;
        self.managed.create_mesh_object().map_err(Into::into)
    }
    pub fn retain_material(&mut self, key: MaterialKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.retain_material(key).map_err(Into::into)
    }
    pub fn retain_mesh_level(&mut self, key: MeshLevelKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.retain_mesh_level(key).map_err(Into::into)
    }
    pub fn retain_mesh_object(&mut self, key: MeshObjectKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.retain_mesh_object(key).map_err(Into::into)
    }
    pub fn link_next(
        &mut self,
        group: FaceGroupKey,
        next: FaceGroupKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.link_next(group, next).map_err(Into::into)
    }
    pub fn install_mesh_level_bbox(
        &mut self,
        key: MeshLevelKey,
        bbox: [[f32; 3]; 2],
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .install_mesh_level_bbox(key, bbox)
            .map_err(Into::into)
    }
    pub fn update_mesh_bound_volume(&mut self, key: MeshObjectKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .update_mesh_bound_volume(key)
            .map_err(Into::into)
    }
    pub fn allocate_group_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        num_faces: u32,
        usage: u32,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(
            self.managed
                .allocate_group_indices(group, num_faces, usage, backend),
        )
    }
    pub fn allocate_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        num_vertices: u32,
        request: VertexRequest,
        head: Option<FaceGroupKey>,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.allocate_level_vertices(
            level,
            num_vertices,
            request,
            head,
            backend,
        ))
    }
    pub fn release_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        backend: &mut B,
    ) -> Result<u32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.release_material(key, backend))
    }
    pub fn release_mesh_level<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshLevelKey,
        backend: &mut B,
    ) -> Result<u32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.release_mesh_level(key, backend))
    }
    pub fn destroy_face_group<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.destroy_face_group(group, backend))
    }
    pub fn set_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        material: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_material(group, material, backend))
    }
    pub fn set_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_texture(key, texture, backend))
    }
    pub fn set_env_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_env_texture(key, texture, backend))
    }
    pub fn set_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        level: Option<MeshLevelKey>,
        index: u32,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_lod(key, level, index, backend))
    }
    pub fn reclaim_unused_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.reclaim_unused_indices(backend))
    }
    pub fn reclaim_unused_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.reclaim_unused_vertices(backend))
    }
    pub fn unlock_all_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.unlock_all_indices(backend))
    }
    pub fn unlock_all_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.unlock_all_vertices(backend))
    }
}

impl ObjectResources {
    pub(crate) fn copy_mesh_level_scalar_fields(
        &mut self,
        destination: MeshLevelKey,
        source: MeshLevelKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .copy_mesh_level_scalar_fields(destination, source)
            .map_err(Into::into)
    }
    pub(crate) fn reset_empty_mesh_bounds(
        &mut self,
        mesh: MeshObjectKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .reset_empty_mesh_bounds(mesh)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub(crate) fn clear_duplicate_mesh<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        mesh: MeshObjectKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.clear_duplicate_mesh(mesh, backend))
    }
    pub(crate) fn copy_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        destination: MaterialKey,
        source: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.copy_material(destination, source, backend))
    }
}

impl ObjectResources {
    /// Resource-owning effects of object SetMesh368e0. The arena caller owns
    /// visual flags and the visual bbox copy/reset after this prefix.
    pub(crate) fn set_object_mesh<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        mesh: Option<MeshObjectKey>,
        preserve_lods: bool,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        if self.object(object)?.visual_type() != ObjectVisualType::Plain {
            return Err(ObjectError::UnsupportedVisualSubtype.into());
        }
        let old = self.object(object)?.mesh;
        if old == mesh {
            return Ok(());
        }
        if let Some(mesh) = mesh {
            self.managed
                .resources()
                .mesh_object(mesh)
                .map_err(ManagedError::from)?;
        }
        if preserve_lods && mesh.is_some() {
            return Err(ObjectError::MeshCompatibilityUnavailable.into());
        }
        let index = (object.slot - 1) as usize;
        let result = (|| {
            {
                for slot in 0..10 {
                    if let Some(lod) = self.objects[index].as_ref().unwrap().lods[slot] {
                        self.cleanup_lod(lod, backend)?;
                        self.lods[(lod.slot - 1) as usize] = None;
                    }
                }
            }
            self.objects[index].as_mut().unwrap().lods = [None; 10];
            if let Some(old) = old {
                managed_result(self.managed.release_mesh_object(old, backend))?;
            }
            self.objects[index].as_mut().unwrap().mesh = mesh;
            if let Some(mesh) = mesh {
                self.managed.retain_mesh_object(mesh)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
}

impl ObjectResources {
    pub(crate) fn append_duplicate_group(
        &mut self,
        level: MeshLevelKey,
        group: FaceGroupKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .append_duplicate_group(level, group)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub fn install_loaded_material_scalars(
        &mut self,
        key: MaterialKey,
        source: &lhr_assets::Material,
        flags: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .install_loaded_material_scalars(key, source, flags)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub(crate) fn install_loaded_lod_distance(
        &mut self,
        level: MeshLevelKey,
        distance: f32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .install_loaded_lod_distance(level, distance)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub(crate) fn adopt_loaded_material_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        environment: bool,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.adopt_loaded_material_texture(
            key,
            texture,
            environment,
            backend,
        ))
    }
}

impl ObjectResources {
    pub(crate) fn lock_mesh_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        flags: u32,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.lock_level_vertices(level, flags, backend))
    }
    pub(crate) fn unlock_mesh_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.unlock_level_vertices(level, backend))
    }
    pub(crate) fn update_object_vertices(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        if self.object(object)?.visual_type() != ObjectVisualType::Plain {
            return Err(ObjectError::UnsupportedVisualSubtype);
        }
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index));
        }
        if let Some(lod) = self.object(object)?.lods[index as usize] {
            let index = self.index(lod.arena, lod.slot)?;
            self.lods[index]
                .as_mut()
                .ok_or(ObjectError::UnknownLod(lod))?
                .dirty = true;
        }
        Ok(())
    }
}

impl ObjectResources {
    pub(crate) fn lock_face_group_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.lock_group_indices(group, backend))
    }
}

impl ObjectResources {
    pub(crate) fn clear_mesh_level_environment(
        &mut self,
        level: MeshLevelKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .clear_mesh_level_environment(level)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_managers::{
        AllocationToken, BufferBackend, BufferKey, BufferKind, CreateResult, GraphicsBuffer,
        GraphicsRequest, LockedDataKey, OwnerLease,
    };
    use crate::visual_resources::{self as vr, RawVertexBytesKey};
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
        fail_release_number: Option<usize>,
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
                fail_release_number: None,
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
            if self.fail_release || self.fail_release_number == Some(self.releases().len()) {
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
    fn shared_mesh(
        r: &mut ObjectResources,
        b: &mut Backend,
    ) -> (MeshObjectKey, MeshLevelKey, MaterialKey) {
        let material = r.create_material().unwrap();
        let head = r.create_face_group().unwrap();
        let tail = r.create_face_group().unwrap();
        r.set_material(head, Some(material), b).unwrap();
        r.set_material(tail, Some(material), b).unwrap();
        r.allocate_group_indices(head, 1, 0, b).unwrap();
        r.allocate_group_indices(tail, 2, 0, b).unwrap();
        r.link_next(head, tail).unwrap();
        let level = r.create_mesh_level().unwrap();
        r.allocate_level_vertices(level, 4, request(4), Some(head), b)
            .unwrap();
        r.install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
            .unwrap();
        let mesh = r.create_mesh_object().unwrap();
        r.set_lod(mesh, Some(level), 0, b).unwrap();
        r.release_mesh_level(level, b).unwrap();
        r.release_material(material, b).unwrap();
        (mesh, level, material)
    }
    #[test]
    fn render_lod_early_returns_preserve_cur_state_and_scale_is_lazy() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, level, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .cur_lod = 7;
        assert_eq!(r.get_object_lod(object, f32::NAN, f32::NAN, 0.), Ok(0));
        assert_eq!(r.object(object).unwrap().cur_lod, 7); // lod_dist<=0 early return
        r.install_loaded_lod_distance(level, 100.).unwrap();
        assert_eq!(r.get_object_lod(object, 100., 1., 1.), Ok(0));
        assert_eq!(r.get_object_lod(object, 100.01, 1., 1.), Ok(1));
        assert_eq!(r.object(object).unwrap().cur_lod, 1); // cull sentinel is stored, not a level
        assert_eq!(
            r.get_object_lod(object, 50., 1., 0.),
            Err(ObjectLodError::NumericDomain)
        );
        assert_eq!(r.object(object).unwrap().cur_lod, 1);
    }
    #[test]
    fn render_fade_skips_null_mesh_and_hidden_private_lod_slots_after_truncation() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let null = r.create_object_resources(None).unwrap();
        r.set_render_fade(null, f32::NAN).unwrap();
        assert_eq!(r.object(null).unwrap().fade, 1.);
        let (mesh, level, _) = shared_mesh(&mut r, &mut b);
        r.set_lod(mesh, Some(level), 1, &mut b).unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let first = r.create_plain_lod().unwrap();
        let hidden = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.adopt_plain_lod(object, 1, hidden).unwrap();
        r.set_lod(mesh, None, 1, &mut b).unwrap(); // actual num_lods truncation, object slot1 retained
        r.set_render_fade(object, 0.5).unwrap();
        assert!(r.lod(first).unwrap().alpha_dirty);
        assert!(!r.lod(hidden).unwrap().alpha_dirty);
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .alpha_dirty = false;
        r.set_render_fade(object, 0.5001).unwrap();
        assert_eq!(r.object(object).unwrap().fade, 0.5001);
        assert!(!r.lod(first).unwrap().alpha_dirty); // same packed byte, still stores float
    }
    #[test]
    fn constructor_nondelete_and_null_object_preserve_represented_fields_without_fake_callbacks() {
        let mut r = crate::ls3df::i3d_object::ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let key = r.create_plain_lod().unwrap();
        let lod = r.lod(key).unwrap();
        assert_eq!(
            (lod.lights_changed, lod.update_count, lod.light_key),
            (u32::MAX, u32::MAX, u32::MAX)
        );
        assert_eq!(lod.vertex_buffer, CompleteVertexBuffer::default());
        assert_eq!(
            [
                lod.u_ofs.to_bits(),
                lod.v_ofs.to_bits(),
                lod.env_u_ofs.to_bits(),
                lod.env_v_ofs.to_bits()
            ],
            [0; 4]
        );
        assert!(!lod.dirty && !lod.alpha_dirty);
        assert_eq!(lod.alpha, 255);
        r.destruct_plain_lod(key, &mut b).unwrap();
        assert_eq!(r.lod(key).unwrap().state, LodState::Destructed);
        assert!(b.events.is_empty());
        let object = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.adopt_plain_lod(object, 0, key),
            Err(ObjectError::LodDestructed(key))
        );
        assert!(r.destroy_plain_lod(key, &mut b).is_err());
        r.destroy_object_resources(object, &mut b).unwrap();
        assert!(r.object(object).is_err());
        assert!(b.events.is_empty());
        assert!(!r.is_poisoned());
    }
    #[test]
    fn two_objects_own_distinct_lod_blocks_but_keep_shared_mesh_and_graphics_until_last_owner() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, level, material) = shared_mesh(&mut r, &mut b);
        let a = r.create_object_resources(Some(mesh)).unwrap();
        let z = r.create_object_resources(Some(mesh)).unwrap();
        let la = r.create_plain_lod().unwrap();
        let lz = r.create_plain_lod().unwrap();
        r.allocate_plain_lod_vertices(la, request(3), &mut b)
            .unwrap();
        r.allocate_plain_lod_vertices(lz, request(5), &mut b)
            .unwrap();
        r.adopt_plain_lod(a, 0, la).unwrap();
        r.adopt_plain_lod(z, 9, lz).unwrap();
        let avb = r.lod(la).unwrap().vertex_buffer;
        let zvb = r.lod(lz).unwrap().vertex_buffer;
        assert_eq!(avb.buffer, zvb.buffer);
        assert_eq!(avb.graphics, zvb.graphics);
        assert_ne!(avb.base, zvb.base);
        assert_eq!(
            avb.graphics,
            r.managed()
                .resources()
                .mesh_level(level)
                .unwrap()
                .vertex_buffer
                .graphics
        );
        assert_eq!(r.release_mesh_object(mesh, &mut b).unwrap(), 2);
        let before = b.events.len();
        assert_eq!(
            r.release_mesh_object(mesh, &mut b),
            Err(ObjectOperationError::State(
                ObjectError::MeshReferenceOwned(mesh)
            ))
        );
        assert_eq!(b.events.len(), before);
        r.destroy_object_resources(a, &mut b).unwrap();
        assert!(r.object(a).is_err());
        assert!(r.lod(la).is_err());
        assert!(b.releases().is_empty());
        assert_eq!(
            r.managed()
                .resources()
                .mesh_object(mesh)
                .unwrap()
                .references,
            1
        );
        assert!(r.managed().resources().material(material).is_ok());
        assert_eq!(r.lod_bindings()[0].state, BindingState::Consumed);
        assert_eq!(r.lod_bindings()[1].state, BindingState::Installed);
        r.destroy_object_resources(z, &mut b).unwrap();
        assert!(r.lod(lz).is_err());
        assert!(r.managed().resources().mesh_object(mesh).is_err());
        assert!(r.managed().resources().material(material).is_err());
        assert_eq!(b.releases().len(), 1);
        assert!(matches!(b.releases()[0], GraphicsBuffer::Vertex(_)));
        assert_eq!(r.managed().managers().index_buffers().len(), 1);
        r.reclaim_unused_indices(&mut b).unwrap();
        assert!(b.allocations.is_empty());
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn all_ten_slots_destroy_in_index_order_even_when_allocated_in_reverse() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let object = r.create_object_resources(None).unwrap();
        let mut expected = vec![None; 10];
        for i in (0..10).rev() {
            let lod = r.create_plain_lod().unwrap();
            let mut q = request(1);
            q.exclusive = true;
            r.allocate_plain_lod_vertices(lod, q, &mut b).unwrap();
            let raw = r.lod(lod).unwrap().vertex_buffer;
            expected[i] = Some(GraphicsBuffer::Vertex(raw.graphics.unwrap()));
            r.adopt_plain_lod(object, i as u32, lod).unwrap();
        }
        r.destroy_object_resources(object, &mut b).unwrap();
        assert_eq!(
            b.releases(),
            expected.into_iter().flatten().collect::<Vec<_>>()
        );
        assert!(r
            .lod_bindings()
            .iter()
            .all(|x| x.state == BindingState::Consumed));
        assert!(b.allocations.is_empty());
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn interrupted_second_lod_keeps_every_raw_slot_until_complete_loop_and_never_replays_prefix() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let mesh = r.create_mesh_object().unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        r.release_mesh_object(mesh, &mut b).unwrap();
        let first = r.create_plain_lod().unwrap();
        let last = r.create_plain_lod().unwrap();
        let mut q = request(1);
        q.exclusive = true;
        r.allocate_plain_lod_vertices(first, q, &mut b).unwrap();
        r.allocate_plain_lod_vertices(last, q, &mut b).unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.adopt_plain_lod(object, 9, last).unwrap();
        let before_slots = r.object(object).unwrap().lods;
        b.fail_release_number = Some(2);
        assert_eq!(
            r.destroy_object_resources(object, &mut b),
            Err(ObjectOperationError::Backend(
                "graphics released then failed"
            ))
        );
        assert!(r.is_poisoned());
        let raw = r.object(object).unwrap();
        assert_eq!(raw.lods, before_slots);
        assert_eq!(raw.mesh, Some(mesh));
        assert_eq!(raw.phase, ObjectCleanupPhase::DestroyingLods);
        assert!(r.lod(first).is_err());
        assert_eq!(r.lod(last).unwrap().state, LodState::Constructed);
        assert_eq!(r.lod_bindings()[0].state, BindingState::Consumed);
        assert_eq!(r.lod_bindings()[1].state, BindingState::Installed);
        assert_eq!(
            r.managed()
                .managers()
                .vertex_owner(r.lod_bindings()[1].owner)
                .unwrap()
                .lease,
            OwnerLease::Freed
        );
        assert_eq!(
            r.managed()
                .resources()
                .mesh_object(mesh)
                .unwrap()
                .references,
            1
        );
        let before = b.events.len();
        assert_eq!(r.create_plain_lod(), Err(ObjectError::Poisoned));
        assert_eq!(
            r.release_mesh_object(mesh, &mut b),
            Err(ObjectOperationError::State(ObjectError::Poisoned))
        );
        assert!(r.destroy_object_resources(object, &mut b).is_err());
        assert!(r.reclaim_unused_vertices(&mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn mesh_release_failure_sees_all_lod_slots_cleared_but_keeps_raw_mesh_pointer() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, material) = shared_mesh(&mut r, &mut b);
        let texture = TextureKey(91);
        b.textures.insert(91, 1);
        r.set_texture(material, Some(texture), &mut b).unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        r.release_mesh_object(mesh, &mut b).unwrap();
        let lod = r.create_plain_lod().unwrap();
        r.allocate_plain_lod_vertices(lod, request(1), &mut b)
            .unwrap();
        r.adopt_plain_lod(object, 4, lod).unwrap();
        b.fail_texture = Some(texture);
        assert_eq!(
            r.destroy_object_resources(object, &mut b),
            Err(ObjectOperationError::Backend(
                "texture released then failed"
            ))
        );
        let raw = r.object(object).unwrap();
        assert_eq!(raw.lods, [None; 10]);
        assert_eq!(raw.mesh, Some(mesh));
        assert_eq!(raw.phase, ObjectCleanupPhase::ReleasingMesh);
        assert!(r.lod(lod).is_err());
        assert_eq!(
            r.managed()
                .resources()
                .mesh_object(mesh)
                .unwrap()
                .references,
            0
        );
        assert!(r.is_poisoned());
        assert_eq!(b.textures[&91], 1);
    }
    #[test]
    fn standalone_nondelete_preserves_stale_descriptor_and_cannot_be_reused_or_adopted() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let lod = r.create_plain_lod().unwrap();
        r.allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap();
        let block = r.lod(lod).unwrap().vertex_buffer;
        r.destruct_plain_lod(lod, &mut b).unwrap();
        assert_eq!(r.lod(lod).unwrap().vertex_buffer, block);
        assert_eq!(r.lod(lod).unwrap().state, LodState::Destructed);
        assert_eq!(r.lod_bindings()[0].state, BindingState::Consumed);
        assert_eq!(
            r.managed()
                .managers()
                .vertex_owner(r.lod_bindings()[0].owner)
                .unwrap()
                .lease,
            OwnerLease::Freed
        );
        assert!(b.allocations.is_empty());
        let object = r.create_object_resources(None).unwrap();
        let before = b.events.len();
        assert_eq!(
            r.adopt_plain_lod(object, 0, lod),
            Err(ObjectError::LodDestructed(lod))
        );
        assert!(r
            .allocate_plain_lod_vertices(lod, request(2), &mut b)
            .is_err());
        assert!(r.destruct_plain_lod(lod, &mut b).is_err());
        assert_eq!(b.events.len(), before);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn unique_adoption_foreign_keys_indices_and_double_storage_reject_before_effects() {
        let mut r = ObjectResources::new().unwrap();
        let mut other = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let foreign = other.create_plain_lod().unwrap();
        let lod = r.create_plain_lod().unwrap();
        let a = r.create_object_resources(None).unwrap();
        let z = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.adopt_plain_lod(a, 0, foreign),
            Err(ObjectError::ForeignArena)
        );
        assert_eq!(
            r.adopt_plain_lod(a, 10, lod),
            Err(ObjectError::InvalidIndex(10))
        );
        assert_eq!(
            r.adopt_plain_lod(a, u32::MAX, lod),
            Err(ObjectError::UnsupportedIndex(u32::MAX))
        );
        r.adopt_plain_lod(a, 0, lod).unwrap();
        assert_eq!(
            r.adopt_plain_lod(z, 0, lod),
            Err(ObjectError::LodOwned(lod))
        );
        assert!(r.destroy_plain_lod(lod, &mut b).is_err());
        assert!(b.events.is_empty());
        r.allocate_plain_lod_vertices(lod, request(1), &mut b)
            .unwrap();
        let before = b.events.len();
        assert!(r
            .allocate_plain_lod_vertices(lod, request(1), &mut b)
            .is_err());
        assert_eq!(b.events.len(), before);
        r.destroy_object_resources(a, &mut b).unwrap();
        assert!(!r.is_poisoned());
        assert!(b.allocations.is_empty());
    }
    #[test]
    fn graphics_false_retains_pending_plain_owner_and_retry_keeps_identity() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let lod = r.create_plain_lod().unwrap();
        b.rejects.extend([-1, -2, -3]);
        assert!(!r
            .allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap());
        assert!(!r.is_poisoned());
        let owner = r.lod_bindings()[0].owner;
        assert_eq!(
            r.managed().managers().vertex_owner(owner).unwrap().lease,
            OwnerLease::Vacant
        );
        assert_eq!(
            r.lod(lod).unwrap().vertex_buffer,
            CompleteVertexBuffer::default()
        );
        assert!(b.allocations.is_empty());
        assert!(r
            .allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap());
        assert_eq!(r.lod_bindings()[0].owner, owner);
        r.destroy_plain_lod(lod, &mut b).unwrap();
        assert!(r.lod(lod).is_err());
        assert!(b.allocations.is_empty());
    }
}

#[path = "object_lod.rs"]
mod object_lod;
pub(crate) use object_lod::render_fade;
pub use object_lod::ObjectLodError;
