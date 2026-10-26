//! Owned saved rectangle lighting for litLod. Pinned mafia-re 29b65b0,
//! I3d_lit_object.cpp:1226–1405,2087–2117,2200–2298. The atlas is a typed CPU
//! adapter for the truecolor profile, not native ITexture::Create coverage.
use super::*;
use crate::{
    buffer_managers::IndexOwnerKey,
    lightmap_atlas::{LightmapAllocationKey, LightmapAtlasRegistry, RectangleInput},
    visual_resources::CompleteIndexBuffer,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitVertexInfo {
    pub vertex: u16,
    pub rectangle: u16,
}

/// Both pending and published resources stay in the same object arena. A
/// failed manager operation therefore cannot lose the lit index/atlas owner.
#[derive(Debug, PartialEq)]
pub struct LitLightmapResources {
    /// Source rectangles before Construct transfers them into the typed atlas
    /// allocation. A poisoned index-allocation prefix retains these inputs.
    pub pending_rectangles: Vec<RectangleInput>,
    pub vertex_count: u32,
    pub vertex_info: Box<[LitVertexInfo]>,
    pub lightmap_uv: Box<[[f32; 2]]>,
    pub base_uv: Box<[[f32; 2]]>,
    pub face_rectangles: Vec<Box<[u16]>>,
    pub allocation: Option<LightmapAllocationKey>,
    pub index_buffer: CompleteIndexBuffer,
    pub index_count: u32,
    /// The host expanded buffer and postProcess are complete, independently
    /// of the source lighting-level constructed flag set after all its LODs.
    pub ready: bool,
}
impl LitLightmapResources {
    fn new() -> Self {
        Self {
            pending_rectangles: Vec::new(),
            vertex_count: 0,
            vertex_info: Box::new([]),
            lightmap_uv: Box::new([]),
            base_uv: Box::new([]),
            face_rectangles: Vec::new(),
            allocation: None,
            index_buffer: CompleteIndexBuffer::default(),
            index_count: 0,
            ready: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitLodIndexBinding {
    pub lod: PlainLodKey,
    pub owner: IndexOwnerKey,
    pub state: BindingState,
}

impl ObjectResources {
    pub fn lightmap_atlases(&self) -> &LightmapAtlasRegistry {
        &self.lightmap_atlases
    }
    pub fn lit_index_bindings(&self) -> &[LitLodIndexBinding] {
        &self.lit_indices
    }
    pub fn lightmap_lod(
        &self,
        lod: PlainLodKey,
    ) -> Result<Option<&LitLightmapResources>, ObjectError> {
        self.lod(lod)?;
        Ok(self.lightmap_lods[(lod.slot - 1) as usize].as_ref())
    }
    pub(crate) fn lightmap_lod_mut(
        &mut self,
        lod: PlainLodKey,
    ) -> Result<&mut LitLightmapResources, ObjectError> {
        self.available()?;
        self.lod(lod)?;
        self.lightmap_lods[(lod.slot - 1) as usize]
            .as_mut()
            .ok_or(ObjectError::UnsupportedVisualSubtype)
    }
    pub(crate) fn create_pending_saved_lightmap_lod(&mut self) -> Result<PlainLodKey, ObjectError> {
        let key = self.create_pending_saved_lit_lod()?;
        self.lightmap_lods[(key.slot - 1) as usize] = Some(LitLightmapResources::new());
        Ok(key)
    }
    pub(crate) fn allocate_lit_lod_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        count: u32,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        if count == 0 || count > i32::MAX as u32 {
            return Err(ManagedError::InvalidExtent.into());
        }
        let current = self.lightmap_lod_mut(lod)?;
        if current.index_buffer != CompleteIndexBuffer::default() || current.index_count != 0 {
            return Err(ObjectError::StorageAlreadyInstalled.into());
        }
        Self::reserve(&mut self.lit_indices)?;
        let position = if let Some(index) = self
            .lit_indices
            .iter()
            .position(|binding| binding.lod == lod)
        {
            if self.lit_indices[index].state != BindingState::Pending {
                return Err(ObjectError::StorageAlreadyInstalled.into());
            }
            index
        } else {
            let owner = self.managed.create_index_storage_owner()?;
            self.lit_indices.push(LitLodIndexBinding {
                lod,
                owner,
                state: BindingState::Pending,
            });
            self.lit_indices.len() - 1
        };
        let owner = self.lit_indices[position].owner;
        // Source Load uses D3DUSAGE_SOFTWAREPROCESSING for its own m_IB.
        match managed_result(
            self.managed
                .allocate_index_storage(owner, 0x10, count, backend),
        ) {
            Ok(Some(descriptor)) => {
                let data = self.lightmap_lod_mut(lod)?;
                data.index_buffer = descriptor;
                data.index_count = count;
                self.lit_indices[position].state = BindingState::Installed;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }

    pub(crate) fn allocate_lit_lightmap(&mut self, lod: PlainLodKey) -> Result<(), ObjectError> {
        if self.lightmap_lod_mut(lod)?.allocation.is_some() {
            return Err(ObjectError::StorageAlreadyInstalled);
        }
        let rectangles = std::mem::take(&mut self.lightmap_lod_mut(lod)?.pending_rectangles);
        let (allocation, result) = match self.lightmap_atlases.allocate(rectangles) {
            Ok(key) => (Some(key), Ok(())),
            Err(failure) => (
                failure.allocation,
                Err(ObjectError::Lightmap(failure.error)),
            ),
        };
        // Even a failed atlas construction can retain a real pending owner.
        self.lightmap_lod_mut(lod)?.allocation = allocation;
        result
    }

    /// Derived destruction precedes the base lod's VB release: UV arrays,
    /// LightMap/reclaimed rectangles, face assignments, lit IB, vertex info.
    pub(super) fn cleanup_lit_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        self.lod(lod)?;
        let slot = (lod.slot - 1) as usize;
        if let Some(data) = self.lightmap_lods[slot].as_mut() {
            data.base_uv = Box::new([]);
            data.lightmap_uv = Box::new([]);
            data.ready = false;
            if let Some(allocation) = data.allocation {
                if let Err(error) = self.lightmap_atlases.release(allocation) {
                    self.poisoned = true;
                    return Err(ObjectError::Lightmap(error).into());
                }
                data.allocation = None;
            }
            data.face_rectangles.clear();
            data.pending_rectangles.clear();
            let descriptor = data.index_buffer;
            let position = self
                .lit_indices
                .iter()
                .position(|binding| binding.lod == lod);
            let owner = position.map(|index| self.lit_indices[index].owner);
            if let Err(error) =
                managed_result(self.managed.free_index_storage(owner, descriptor, backend))
            {
                self.poisoned = true;
                return Err(error);
            }
            if let Some(index) = position {
                self.lit_indices[index].state = BindingState::Consumed;
            }
            self.lightmap_lods[slot] = None;
        }
        self.cleanup_lod(lod, backend)
    }

    pub(crate) fn destroy_pending_saved_lit_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        if self.lod(lod)?.owner.is_some() {
            return Err(ObjectError::LodOwned(lod).into());
        }
        self.cleanup_lit_lod(lod, backend)?;
        self.lods[(lod.slot - 1) as usize] = None;
        Ok(())
    }
}
