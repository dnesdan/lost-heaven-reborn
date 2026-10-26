//! Managed presentation storage attached to the actual derived LOD owner.
//! These are descriptive host records, not recovered C++ field layouts. CPU
//! saved payload clones never own copied external buffer descriptors.
use super::*;
use crate::buffer_managers::{IndexOwnerKey, OwnerLease};
use crate::visual_resources::CompleteIndexBuffer;
use saved_lightmaps::{SavedLitLod, SavedLitVertex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LitStoragePhase {
    Unprepared,
    Preparing,
    Ready,
    Retiring,
    Retired,
    Interrupted,
}
#[derive(Debug, PartialEq)]
pub enum RetiringLitArrays {
    // The host stores VI and UV in one vector. Keep that actual allocation
    // reachable until after the independent IB free; no native array ABI claim.
    Lightmap {
        vertices: Vec<SavedLitVertex>,
        indices: Vec<u16>,
    },
    VertexColors(Vec<u32>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitSourceBinding {
    pub level: MeshLevelKey,
    pub vertices: CompleteVertexBuffer,
    pub vertex_count: u32,
    pub revision: u64,
}
#[derive(Debug, PartialEq)]
pub struct LitLodStorage {
    pub lod: PlainLodKey,
    pub phase: LitStoragePhase,
    pub source_binding: Option<LitSourceBinding>,
    pub index_owner: Option<IndexOwnerKey>,
    pub index_buffer: CompleteIndexBuffer,
    pub index_count: u32,
    pub index_state: BindingState,
    /// Moved allocations retained across a failed derived IB cleanup.
    pub retiring_arrays: Option<RetiringLitArrays>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LitStorageCounts {
    pub ready_lods: usize,
    pub preparing_lods: usize,
    pub retiring_lods: usize,
    pub interrupted_lods: usize,
    pub live_vertex_owners: usize,
    pub live_index_owners: usize,
}
impl LitLodStorage {
    pub(super) fn new(lod: PlainLodKey) -> Self {
        Self {
            lod,
            phase: LitStoragePhase::Unprepared,
            source_binding: None,
            index_owner: None,
            index_buffer: CompleteIndexBuffer::default(),
            index_count: 0,
            index_state: BindingState::Pending,
            retiring_arrays: None,
        }
    }
}
impl ObjectResources {
    pub fn lit_lod_storage_records(&self) -> &[LitLodStorage] {
        &self.lit_storage
    }
    pub fn lit_storage_counts(&self) -> LitStorageCounts {
        let mut counts = LitStorageCounts::default();
        for storage in &self.lit_storage {
            match storage.phase {
                LitStoragePhase::Ready => counts.ready_lods += 1,
                LitStoragePhase::Preparing => counts.preparing_lods += 1,
                LitStoragePhase::Retiring => counts.retiring_lods += 1,
                LitStoragePhase::Interrupted => counts.interrupted_lods += 1,
                LitStoragePhase::Unprepared | LitStoragePhase::Retired => (),
            }
            if storage.index_owner.is_some_and(|owner| {
                self.managed
                    .managers()
                    .index_owner(owner)
                    .is_ok_and(|s| matches!(s.lease, OwnerLease::Active { .. }))
            }) {
                counts.live_index_owners += 1;
            }
            for binding in self
                .bindings
                .iter()
                .filter(|b| b.lod == storage.lod && b.state != BindingState::Consumed)
            {
                if self
                    .managed
                    .managers()
                    .vertex_owner(binding.owner)
                    .is_ok_and(|s| matches!(s.lease, OwnerLease::Active { .. }))
                {
                    counts.live_vertex_owners += 1;
                }
            }
        }
        counts
    }
    pub(crate) fn lit_lod_storage(&self, lod: PlainLodKey) -> Result<&LitLodStorage, ObjectError> {
        self.lod(lod)?;
        self.lit_storage
            .iter()
            .find(|s| s.lod == lod)
            .ok_or(ObjectError::UnsupportedVisualSubtype)
    }
    fn lit_storage_position(&self, lod: PlainLodKey) -> Result<usize, ObjectError> {
        self.lod(lod)?;
        self.lit_storage
            .iter()
            .position(|s| s.lod == lod)
            .ok_or(ObjectError::UnsupportedVisualSubtype)
    }
    pub(crate) fn begin_lit_preparation(&mut self, lod: PlainLodKey) -> Result<(), ObjectError> {
        self.available()?;
        let i = self.lit_storage_position(lod)?;
        if self.lit_storage[i].phase != LitStoragePhase::Unprepared {
            return Err(ObjectError::StorageAlreadyInstalled);
        }
        self.lit_storage[i].phase = LitStoragePhase::Preparing;
        Ok(())
    }
    pub(crate) fn finish_lit_preparation(
        &mut self,
        lod: PlainLodKey,
        source: LitSourceBinding,
    ) -> Result<(), ObjectError> {
        self.available()?;
        let i = self.lit_storage_position(lod)?;
        if self.lit_storage[i].phase != LitStoragePhase::Preparing {
            return Err(ObjectError::StorageAlreadyInstalled);
        }
        self.lit_storage[i].phase = LitStoragePhase::Ready;
        self.lit_storage[i].source_binding = Some(source);
        self.lods[(lod.slot - 1) as usize].as_mut().unwrap().dirty = false;
        Ok(())
    }
    pub(crate) fn interrupt_lit_preparation(&mut self, lod: PlainLodKey, poison: bool) {
        if let Some(storage) = self.lit_storage.iter_mut().find(|s| s.lod == lod) {
            storage.phase = LitStoragePhase::Interrupted;
        }
        self.poisoned |= poison;
    }
    pub(crate) fn release_lit_render_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let descriptor = self.lod(key)?.vertex_buffer;
        let position = self
            .bindings
            .iter()
            .position(|b| b.lod == key && b.state != BindingState::Consumed);
        let owner = position.map(|i| self.bindings[i].owner);
        if let Err(error) =
            managed_result(self.managed.free_vertex_storage(owner, descriptor, backend))
        {
            self.poisoned = true;
            if let Some(s) = self.lit_storage.iter_mut().find(|s| s.lod == key) {
                s.phase = LitStoragePhase::Interrupted;
            }
            return Err(error);
        }
        if let Some(i) = position {
            self.bindings[i].state = BindingState::Consumed;
        }
        let lod = self.lods[(key.slot - 1) as usize].as_mut().unwrap();
        lod.vertex_buffer = CompleteVertexBuffer::default();
        lod.dirty = true;
        lod.lights_changed = u32::MAX;
        lod.light_key = u32::MAX;
        if let Some(s) = self.lit_storage.iter_mut().find(|s| s.lod == key) {
            s.phase = LitStoragePhase::Unprepared;
            s.source_binding = None;
        }
        Ok(())
    }
    pub(crate) fn allocate_saved_lit_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        count: u32,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        let i = self.lit_storage_position(lod)?;
        if self.lit_storage[i].phase != LitStoragePhase::Preparing || count == 0 {
            return Err(ObjectError::StorageAlreadyInstalled.into());
        }
        if self.lit_storage[i].index_state == BindingState::Installed {
            self.saved_lit_index_descriptor(lod, count)?;
            return Ok(true);
        }
        if self.lit_storage[i].index_owner.is_none() {
            self.lit_storage[i].index_owner = Some(self.managed.create_index_storage_owner()?);
        }
        let owner = self.lit_storage[i].index_owner.unwrap();
        match managed_result(
            self.managed
                .allocate_index_storage(owner, 0x10, count, backend),
        ) {
            Ok(Some(descriptor)) => {
                self.lit_storage[i].index_buffer = descriptor;
                self.lit_storage[i].index_count = count;
                self.lit_storage[i].index_state = BindingState::Installed;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
    pub(crate) fn saved_lit_index_descriptor(
        &self,
        lod: PlainLodKey,
        count: u32,
    ) -> Result<CompleteIndexBuffer, ObjectError> {
        self.available()?;
        let storage = self.lit_lod_storage(lod)?;
        if storage.index_state != BindingState::Installed || storage.index_count != count {
            return Err(ManagedError::BindingMismatch.into());
        }
        let owner = storage.index_owner.ok_or(ManagedError::BindingMismatch)?;
        if self
            .managed
            .index_storage_configuration(owner, storage.index_buffer, count)?
            != 0x10
        {
            return Err(ManagedError::BindingMismatch.into());
        }
        Ok(storage.index_buffer)
    }
    /// Derived payload/atlas retirement precedes IB release; the actual moved
    /// VI/color allocation remains owned if that release interrupts. Base VB
    /// cleanup follows only after the derived release completed.
    pub(super) fn cleanup_saved_lit_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        level: usize,
        index: usize,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let key = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?
            .levels
            .get(level)
            .ok_or(ObjectError::InvalidIndex(level as u32))?
            .lods
            .get(index)
            .copied()
            .flatten()
            .ok_or(ObjectError::InvalidIndex(index as u32))?;
        if self.lod(key)?.owner != Some(object) {
            return Err(ObjectError::LodOwned(key).into());
        }
        let position = self.lit_storage_position(key)?;
        let payload = self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[level]
            .saved_lods[index]
            .take();
        self.lit_storage[position].phase = LitStoragePhase::Retiring;
        self.lit_storage[position].retiring_arrays = match payload {
            Some(SavedLitLod::Lightmap(lod)) => {
                let saved_lightmaps::SavedLightmapLod {
                    vertices,
                    indices,
                    face_rectangles,
                    placements,
                    atlas,
                    _allocation,
                    ..
                } = lod;
                drop(atlas);
                drop(_allocation);
                drop(face_rectangles);
                drop(placements);
                Some(RetiringLitArrays::Lightmap { vertices, indices })
            }
            Some(SavedLitLod::VertexColors { colors, .. }) => {
                Some(RetiringLitArrays::VertexColors(colors))
            }
            None => None,
        };
        let descriptor = self.lit_storage[position].index_buffer;
        let owner = self.lit_storage[position].index_owner;
        if let Err(error) =
            managed_result(self.managed.free_index_storage(owner, descriptor, backend))
        {
            self.lit_storage[position].phase = LitStoragePhase::Interrupted;
            self.poisoned = true;
            return Err(error);
        }
        self.lit_storage[position].index_state = BindingState::Consumed;
        self.lit_storage[position].retiring_arrays = None;
        if let Err(error) = self.cleanup_lod(key, backend) {
            self.lit_storage[position].phase = LitStoragePhase::Interrupted;
            return Err(error);
        }
        self.lit_storage[position].phase = LitStoragePhase::Retired;
        Ok(())
    }
}
