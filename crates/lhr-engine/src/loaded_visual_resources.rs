//! CPU ownership for admitted plain and unconstructed lit objects, using the existing visual
//! and buffer-manager lifetimes. This host intake is not native Object::Load,
//! texture Create, graphics-driver execution, or a renderer. Source vertex and
//! index bytes and decoded color/alpha/environment images remain owned until
//! their actual last resource reference is released.
use crate::{
    buffer_managers::{
        AllocationToken, BufferBackend, BufferKey, BufferKind, CreateResult, GraphicsBuffer,
        GraphicsRequest, LockedDataKey, VertexRequest,
    },
    i3d_object::{ObjectError, ObjectOperationError, ObjectResourceKey, ObjectResources},
    managed_visual_resources::ManagedVisualBackend,
    mesh_bounds::MeshBoundVolume,
    visual_resources::{
        IndexBufferHandle, MaterialKey, MeshLevelKey, RawVertexBytesKey, TextureKey,
        VertexBufferHandle,
    },
};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "car_inspection_resources.rs"]
mod car_inspection_resources;
pub use car_inspection_resources::{CpuInspectionGroup, CpuInspectionMesh};
#[path = "saved_lighting.rs"]
mod saved_lighting;
pub use saved_lighting::{SavedLightingLoadError, SavedLightingPendingCleanup};
#[path = "car_inspection_texture.rs"]
mod car_inspection_texture;
#[path = "loaded_glass.rs"]
mod glass;
#[path = "loaded_vertex_access.rs"]
mod vertex_access;
static NEXT_STORAGE_KEY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadedVisualError {
    Object(ObjectError),
    Backend(&'static str),
    Asset(String),
    UnsupportedVisual(Option<u8>),
    InvalidGeometry,
    IncompleteAdmission,
}
impl From<ObjectError> for LoadedVisualError {
    fn from(e: ObjectError) -> Self {
        Self::Object(e)
    }
}
impl From<ObjectOperationError<LoadedVisualError>> for LoadedVisualError {
    fn from(e: ObjectOperationError<LoadedVisualError>) -> Self {
        match e {
            ObjectOperationError::State(e) => e.into(),
            ObjectOperationError::Backend(e) => e,
        }
    }
}
type Result<T> = std::result::Result<T, LoadedVisualError>;
fn identity() -> Result<u64> {
    NEXT_STORAGE_KEY
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| LoadedVisualError::Backend("CPU identity overflow"))
}
fn bytes(n: usize) -> Result<Vec<u8>> {
    if n > 256 * 1024 * 1024 {
        return Err(LoadedVisualError::Backend("CPU buffer quota"));
    }
    let mut b = Vec::new();
    b.try_reserve_exact(n)
        .map_err(|_| LoadedVisualError::Backend("CPU allocation"))?;
    b.resize(n, 0);
    Ok(b)
}
#[derive(Debug)]
struct BufferAllocation {
    kind: BufferKind,
}
#[derive(Debug)]
struct BlockAllocation {
    kind: BufferKind,
    base: u32,
    count: u32,
}
#[derive(Debug)]
struct CpuGraphics {
    kind: BufferKind,
    fvf: Option<u32>,
    bytes: Vec<u8>,
    lock: Option<LockedDataKey>,
}
#[derive(Debug)]
struct CpuTexture {
    asset_key: String,
    references: u32,
    color: lhr_assets::Texture,
    alpha: Option<lhr_assets::Texture>,
    /// Observed Texture+30 mask0x10e only, consumed by Material::SetTexture.
    /// Other Create75f50 fields/bits are not represented by CPU storage.
    flags: u32,
    request_flags: u32,
    inspection_rgba8_color_key: bool,
}
#[derive(Debug, Default)]
pub struct CpuVisualStorage {
    buffers: BTreeMap<u64, Box<BufferAllocation>>,
    blocks: BTreeMap<u64, Box<BlockAllocation>>,
    graphics: BTreeMap<u64, CpuGraphics>,
    textures: BTreeMap<u64, CpuTexture>,
    raw: BTreeMap<u64, Vec<u8>>,
    resident_bytes: usize,
    discard_requests: u64,
    #[cfg(test)]
    fail_next_graphics_allocation: bool,
}
impl CpuVisualStorage {
    pub fn graphics_bytes(&self) -> usize {
        self.graphics.values().map(|g| g.bytes.len()).sum()
    }
    pub fn live_graphics(&self) -> usize {
        self.graphics.len()
    }
    /// Native Close unloads only managed pools. CPU immediate textures do not
    /// participate; managed requests must be rejected by a scene adapter.
    pub fn managed_texture_count(&self) -> usize {
        self.textures
            .values()
            .filter(|texture| texture.request_flags & 0x40 != 0)
            .count()
    }
    pub fn live_textures(&self) -> usize {
        self.textures.len()
    }
    pub fn live_allocations(&self) -> usize {
        self.buffers.len() + self.blocks.len() + self.raw.len()
    }
    pub fn decoded_texture_bytes(&self) -> usize {
        self.textures
            .values()
            .map(|t| t.color.rgba.len() + t.alpha.as_ref().map_or(0, |a| a.rgba.len()))
            .sum()
    }
    pub fn texture_references(&self, key: TextureKey) -> Option<u32> {
        self.textures.get(&key.0).map(|t| t.references)
    }
    fn graphics_id(g: GraphicsBuffer) -> u64 {
        match g {
            GraphicsBuffer::Index(h) => h.0,
            GraphicsBuffer::Vertex(h) => h.0,
        }
    }
    fn get_graphics_mut(&mut self, g: GraphicsBuffer) -> Result<&mut CpuGraphics> {
        let x = self
            .graphics
            .get_mut(&Self::graphics_id(g))
            .ok_or(LoadedVisualError::Backend("unknown CPU graphics"))?;
        if !matches!(
            (x.kind, g),
            (BufferKind::Index, GraphicsBuffer::Index(_))
                | (BufferKind::Vertex, GraphicsBuffer::Vertex(_))
        ) {
            return Err(LoadedVisualError::Backend("CPU graphics kind"));
        }
        Ok(x)
    }
    fn write_graphics(&mut self, g: GraphicsBuffer, offset: usize, data: &[u8]) -> Result<()> {
        let x = self.get_graphics_mut(g)?;
        if x.lock.is_none() {
            return Err(LoadedVisualError::Backend("CPU upload without lock"));
        }
        let end = offset
            .checked_add(data.len())
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        x.bytes
            .get_mut(offset..end)
            .ok_or(LoadedVisualError::InvalidGeometry)?
            .copy_from_slice(data);
        Ok(())
    }
    /// Host CPU upload; cached manager arrays may already be unlocked. A
    /// transient lock leaves the manager's existing lock bookkeeping unchanged.
    fn upload_graphics(
        &mut self,
        graphics: GraphicsBuffer,
        offset: usize,
        data: &[u8],
    ) -> Result<()> {
        let transient = self.get_graphics_mut(graphics)?.lock.is_none();
        if transient {
            self.lock(graphics, 0)?;
        }
        let result = self.write_graphics(graphics, offset, data);
        if transient {
            self.unlock(graphics)?;
        }
        result
    }
    fn admit_texture(
        &mut self,
        asset_key: &str,
        color: lhr_assets::Texture,
        alpha: Option<lhr_assets::Texture>,
    ) -> Result<TextureKey> {
        self.admit_texture_with_flags(asset_key, color, alpha, 0, 0)
    }
    fn admit_texture_with_flags(
        &mut self,
        asset_key: &str,
        color: lhr_assets::Texture,
        alpha: Option<lhr_assets::Texture>,
        request_flags: u32,
        flags: u32,
    ) -> Result<TextureKey> {
        let alpha = alpha
            .map(|a| {
                crate::itexture::resize_loaded_texture(a, color.width, color.height)
                    .map_err(|_| LoadedVisualError::Backend("alpha image resize"))
            })
            .transpose()?;
        let asset_key = asset_key.to_ascii_lowercase();
        let same_image = |a: &lhr_assets::Texture, b: &lhr_assets::Texture| {
            a.width == b.width && a.height == b.height && a.rgba == b.rgba
        };
        let existing = self.textures.iter().find_map(|(&id, current)| {
            (current.asset_key == asset_key
                && current.request_flags == request_flags
                && same_image(&current.color, &color)
                && match (&current.alpha, &alpha) {
                    (None, None) => true,
                    (Some(a), Some(b)) => same_image(a, b),
                    _ => false,
                })
            .then_some(TextureKey(id))
        });
        if let Some(key) = existing {
            self.retain_texture(key)?;
            return Ok(key);
        }
        let id = identity()?;
        self.textures.insert(
            id,
            CpuTexture {
                asset_key,
                references: 1,
                color,
                alpha,
                flags: flags & 0x10e,
                request_flags,
                inspection_rgba8_color_key: false,
            },
        );
        Ok(TextureKey(id))
    }
}
impl BufferBackend for CpuVisualStorage {
    type Error = LoadedVisualError;
    fn allocate_buffer(&mut self, kind: BufferKind) -> Result<AllocationToken> {
        let id = identity()?;
        self.buffers.insert(id, Box::new(BufferAllocation { kind }));
        Ok(AllocationToken(id))
    }
    fn free_buffer(&mut self, a: AllocationToken) -> Result<()> {
        let allocation = self
            .buffers
            .remove(&a.0)
            .ok_or(LoadedVisualError::Backend("unknown CPU buffer allocation"))?;
        let _kind = allocation.kind;
        Ok(())
    }
    fn allocate_block(
        &mut self,
        kind: BufferKind,
        base: u32,
        count: u32,
    ) -> Result<AllocationToken> {
        if count == 0 || base.checked_add(count).is_none() {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let id = identity()?;
        self.blocks
            .insert(id, Box::new(BlockAllocation { kind, base, count }));
        Ok(AllocationToken(id))
    }
    fn free_block(&mut self, a: AllocationToken) -> Result<()> {
        let allocation = self
            .blocks
            .remove(&a.0)
            .ok_or(LoadedVisualError::Backend("unknown CPU block allocation"))?;
        let _descriptor = (allocation.kind, allocation.base, allocation.count);
        Ok(())
    }
    fn create_graphics(&mut self, r: GraphicsRequest) -> Result<CreateResult> {
        if r.bytes == 0 {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        #[cfg(test)]
        if std::mem::take(&mut self.fail_next_graphics_allocation) {
            return Err(LoadedVisualError::Backend(
                "injected CPU graphics allocation failure",
            ));
        }
        let storage = bytes(r.bytes as usize)?;
        let id = identity()?;
        self.graphics.insert(
            id,
            CpuGraphics {
                kind: r.kind,
                fvf: r.fvf,
                bytes: storage,
                lock: None,
            },
        );
        Ok(CreateResult::Created(match r.kind {
            BufferKind::Index => GraphicsBuffer::Index(IndexBufferHandle(id)),
            BufferKind::Vertex => GraphicsBuffer::Vertex(VertexBufferHandle(id)),
        }))
    }
    fn discard_managed(&mut self) -> Result<()> {
        self.discard_requests = self
            .discard_requests
            .checked_add(1)
            .ok_or(LoadedVisualError::Backend("discard counter"))?;
        Err(LoadedVisualError::Backend(
            "CPU storage cannot discard native managed textures",
        ))
    }
    fn lock(&mut self, g: GraphicsBuffer, _flags: u32) -> Result<LockedDataKey> {
        if self.get_graphics_mut(g)?.lock.is_some() {
            return Err(LoadedVisualError::Backend("CPU graphics already locked"));
        }
        let id = LockedDataKey(identity()?);
        self.get_graphics_mut(g)?.lock = Some(id);
        Ok(id)
    }
    fn unlock(&mut self, g: GraphicsBuffer) -> Result<()> {
        let x = self.get_graphics_mut(g)?;
        if x.lock.take().is_none() {
            return Err(LoadedVisualError::Backend("CPU graphics not locked"));
        }
        Ok(())
    }
    fn release_graphics(&mut self, g: GraphicsBuffer) -> Result<()> {
        if self.get_graphics_mut(g)?.lock.is_some() {
            return Err(LoadedVisualError::Backend("CPU release locked graphics"));
        }
        self.graphics
            .remove(&Self::graphics_id(g))
            .ok_or(LoadedVisualError::Backend("unknown CPU graphics"))?;
        Ok(())
    }
    fn driver_present(&self) -> bool {
        true
    }
    fn recompute_vram(&mut self) -> Result<()> {
        self.resident_bytes = self.graphics_bytes();
        Ok(())
    }
    fn warn_unreleased_buffer(&mut self, _: BufferKey) -> Result<()> {
        Err(LoadedVisualError::Backend("CPU manager has live owners"))
    }
    fn out_of_memory(&mut self, _: BufferKind, _: u32, _: i32) -> Result<()> {
        Err(LoadedVisualError::Backend(
            "CPU graphics rejected allocation",
        ))
    }
}
impl ManagedVisualBackend for CpuVisualStorage {
    fn retain_texture(&mut self, t: TextureKey) -> Result<()> {
        let x = self
            .textures
            .get_mut(&t.0)
            .ok_or(LoadedVisualError::Backend("unknown CPU texture"))?;
        x.references = x
            .references
            .checked_add(1)
            .ok_or(LoadedVisualError::Backend("texture reference overflow"))?;
        Ok(())
    }
    fn release_texture(&mut self, t: TextureKey) -> Result<()> {
        let x = self
            .textures
            .get_mut(&t.0)
            .ok_or(LoadedVisualError::Backend("unknown CPU texture"))?;
        x.references = x
            .references
            .checked_sub(1)
            .ok_or(LoadedVisualError::Backend("texture reference underflow"))?;
        if x.references == 0 {
            self.textures.remove(&t.0);
        }
        Ok(())
    }
    fn texture_flags(&mut self, t: TextureKey) -> Result<u32> {
        self.textures
            .get(&t.0)
            .map(|t| t.flags)
            .ok_or(LoadedVisualError::Backend("unknown CPU texture"))
    }
    fn free_raw_vertices(&mut self, _: MeshLevelKey, k: RawVertexBytesKey) -> Result<()> {
        self.raw
            .remove(&k.0)
            .map(|_| ())
            .ok_or(LoadedVisualError::Backend("unknown CPU raw vertices"))
    }
}

struct PreparedMaterial {
    source: lhr_assets::Material,
    color: Option<lhr_assets::Texture>,
    alpha: Option<lhr_assets::Texture>,
    environment: Option<lhr_assets::Texture>,
    inspection_rgba8_color_key: bool,
}
/// Driver words consumed by original material Load2bd50. This context controls
/// CPU resource admission; it does not claim a created renderer or GPU device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialLoadContext {
    pub word_bfce0: u32,
    pub word_bfce4: u32,
    pub word_bfd04: u32,
    pub word_bfcd4: u32,
    pub word_bfcf4: u32,
}
impl MaterialLoadContext {
    /// Original driver Init15760 defaults at slots16,17,25,13,21.
    /// Selected explicitly for CPU intake; later runtime configuration may differ.
    pub const ORIGINAL_DRIVER_INIT_DEFAULTS: Self = Self {
        word_bfce0: 1,
        word_bfce4: 0,
        word_bfd04: 1,
        word_bfcd4: 1,
        word_bfcf4: 0,
    };
    pub fn texture_flags(self, flags: u32) -> u32 {
        let mut texture = (flags >> 5) & 0x40;
        if self.word_bfce0 != 0 {
            texture |= 0x80;
        }
        if self.word_bfce4 != 0 || flags & 0x100000 != 0 {
            texture |= 0x400;
        } else if self.word_bfd04 != 0 && flags & 0x200000 == 0 {
            texture |= 0x200;
        }
        if flags & 0x800000 != 0 && self.word_bfcd4 != 0 {
            texture |= 0x100;
        }
        if self.word_bfcf4 != 0 && flags & 0x400000 == 0 {
            texture |= 0x2000;
        }
        if flags & 0x8000 != 0 {
            texture |= 0x1000;
        }
        if flags & 0x20000000 != 0 {
            texture |= if flags & 0x1000000 != 0 { 0xa } else { 8 };
        } else if flags & 0x40000000 != 0 {
            texture |= if flags & 0x1000000 != 0 { 2 } else { 4 };
        }
        texture
    }
    /// Texture+30 values consumed by material Copy on the admitted Create75f50
    /// branch. Requested bit2 needs native source BPP behavior and is explicitly
    /// excluded unless managed bit0x40 returns before image decoding.
    pub fn base_texture_flags(self, source: &lhr_assets::Material) -> Result<u32> {
        let mut flags = self.base_texture_request_flags(source);
        if flags & 0x40 != 0 {
            return Ok(flags & 0x10e);
        }
        if flags & 2 != 0 {
            return Err(LoadedVisualError::Backend(
                "texture alpha/BPP flag domain unavailable",
            ));
        }
        if source.alpha_texture.as_ref().is_some_and(|a| !a.is_empty()) {
            flags |= 4;
        } else {
            flags &= !4;
        }
        Ok(flags & 0x10e)
    }
    pub fn base_texture_request_flags(self, source: &lhr_assets::Material) -> u32 {
        let mut flags = self.texture_flags(source.flags);
        if self.loaded_material_flags(source) & 0x40000000 != 0
            && self.loaded_material_flags(source) & 0xa1000000 == 0
            && !source.alpha_texture.as_ref().is_some_and(|a| !a.is_empty())
        {
            flags &= !4;
        }
        flags
    }
    pub fn environment_texture_flags(self, source: &lhr_assets::Material) -> u32 {
        (self.texture_flags(source.flags) & 0xffffef31)
            | 0x80000
            | if self.word_bfcd4 != 0 { 0x100 } else { 0 }
    }
    pub fn loaded_material_flags(self, source: &lhr_assets::Material) -> u32 {
        let mut flags = source.flags;
        if self.texture_flags(flags) & 0x108 == 0x108 {
            flags |= 0x40000000;
        }
        if source.texture.is_empty() {
            flags &= !0x800;
        }
        flags
    }
}
/// One owner must remain attached to its ModelArena until every bound frame has
/// been explicitly released. A failed admission retains diagnostics and blocks
/// further admissions; it is never reported as an empty successful object.
pub struct LoadedVisualResources {
    objects: ObjectResources,
    glass_pending_resources: Vec<glass::GlassResource>,
    #[cfg(test)]
    glass_fault_after_vertex_upload: bool,
    backend: CpuVisualStorage,
    sources: BTreeMap<(u64, u32), Vec<lhr_assets::Material>>,
    incomplete: bool,
    material_load_context: MaterialLoadContext,
    inspection_rgba8_color_key: bool,
}
impl std::fmt::Debug for LoadedVisualResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedVisualResources")
            .field("live_objects", &self.sources.len())
            .field("backend", &self.backend)
            .field("incomplete", &self.incomplete)
            .finish()
    }
}
impl LoadedVisualResources {
    pub fn new() -> Result<Self> {
        Ok(Self {
            objects: ObjectResources::new()?,
            glass_pending_resources: Vec::new(),
            #[cfg(test)]
            glass_fault_after_vertex_upload: false,
            backend: CpuVisualStorage::default(),
            sources: BTreeMap::new(),
            incomplete: false,
            material_load_context: MaterialLoadContext::ORIGINAL_DRIVER_INIT_DEFAULTS,
            inspection_rgba8_color_key: false,
        })
    }
    pub fn material_load_context(&self) -> MaterialLoadContext {
        self.material_load_context
    }
    pub fn set_material_load_context(&mut self, context: MaterialLoadContext) {
        self.material_load_context = context;
    }
    /// Explicit host inspection destination. Existing CPU admissions stay
    /// unchanged unless this is selected before any textures are loaded.
    pub fn enable_inspection_rgba8_color_key(&mut self) -> Result<()> {
        if !self.backend.textures.is_empty() || !self.sources.is_empty() {
            return Err(LoadedVisualError::Backend(
                "inspection format selected after resource admission",
            ));
        }
        self.inspection_rgba8_color_key = true;
        Ok(())
    }
    pub fn backend(&self) -> &CpuVisualStorage {
        &self.backend
    }
    /// Host CPU texture admission shared by loaded materials and projector
    /// owners. The returned reference belongs to the caller; no cache reference
    /// keeps the decoded image alive after the final owner releases it. This
    /// does not claim native texture Create flags or device upload behavior.
    pub fn load_cpu_texture(
        &mut self,
        installation: &lhr_assets::Installation,
        name: &str,
    ) -> Result<TextureKey> {
        let name = name.split('\0').next().unwrap_or_default();
        if name.is_empty() {
            return Err(LoadedVisualError::Backend("empty CPU texture name"));
        }
        let path = format!("maps/{name}");
        let bytes = installation
            .read(&path)
            .map_err(|error| LoadedVisualError::Asset(format!("{path}: {error}")))?;
        let image = if name.to_ascii_lowercase().ends_with(".tga") {
            lhr_assets::decode_tga(&bytes)
        } else {
            lhr_assets::decode_bmp_original(&bytes)
        }
        .map_err(|error| LoadedVisualError::Asset(format!("{path}: {error}")))?;
        self.backend.admit_texture(&format!("{path}|"), image, None)
    }
    pub fn retain_cpu_texture(&mut self, texture: TextureKey) -> Result<()> {
        self.backend.retain_texture(texture)
    }
    pub fn release_cpu_texture(&mut self, texture: TextureKey) -> Result<()> {
        self.backend.release_texture(texture)
    }
    pub fn cpu_texture(&self, texture: TextureKey) -> Result<&lhr_assets::Texture> {
        self.backend
            .textures
            .get(&texture.0)
            .map(|texture| &texture.color)
            .ok_or(LoadedVisualError::Backend("unknown CPU texture"))
    }
    pub fn objects(&self) -> &ObjectResources {
        &self.objects
    }
    pub fn has_incomplete_admission(&self) -> bool {
        self.incomplete
    }
    pub fn object_bounds(&self, key: ObjectResourceKey) -> Result<MeshBoundVolume> {
        let mesh = self
            .objects
            .object(key)?
            .mesh
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        self.objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
            .bound_volume()
            .copied()
            .ok_or(LoadedVisualError::InvalidGeometry)
    }
    pub fn source_materials(&self, key: ObjectResourceKey) -> Result<&[lhr_assets::Material]> {
        self.objects.object(key)?;
        self.sources
            .get(&(key.arena(), key.slot()))
            .map(Vec::as_slice)
            .ok_or(LoadedVisualError::InvalidGeometry)
    }
    /// Create a second plain visual owner of one already admitted mesh. The
    /// caller resolves the original instance index in its source-frame map.
    pub fn load_plain_instance(
        &mut self,
        original: ObjectResourceKey,
    ) -> Result<ObjectResourceKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let source = self.objects.object(original)?;
        if source.visual_type() != crate::i3d_object::ObjectVisualType::Plain {
            return Err(LoadedVisualError::UnsupportedVisual(Some(1)));
        }
        let mesh = source.mesh;
        let materials = self.source_materials(original)?.to_vec();
        let result = (|| {
            let object = self.objects.create_object_resources(mesh)?;
            self.sources
                .insert((object.arena(), object.slot()), materials);
            Ok(object)
        })();
        if result.is_err() {
            self.incomplete = true;
        }
        result
    }
    pub fn load_plain_object(
        &mut self,
        source: &lhr_assets::Model,
        index: usize,
        installation: &lhr_assets::Installation,
    ) -> Result<ObjectResourceKey> {
        let frame = source
            .frames
            .get(index)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if frame.kind != 1 || frame.visual != Some(0) {
            return Err(LoadedVisualError::UnsupportedVisual(frame.visual));
        }
        self.load_base_object(source, index, installation)
    }
    /// Distinct derived-class admission; constructed lighting has no producer.
    pub fn load_unconstructed_lit_object(
        &mut self,
        source: &lhr_assets::Model,
        index: usize,
        installation: &lhr_assets::Installation,
    ) -> Result<ObjectResourceKey> {
        let frame = source
            .frames
            .get(index)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if frame.kind != 1 || frame.visual != Some(1) {
            return Err(LoadedVisualError::UnsupportedVisual(frame.visual));
        }
        self.load_base_object(source, index, installation)
    }
    fn load_base_object(
        &mut self,
        source: &lhr_assets::Model,
        index: usize,
        installation: &lhr_assets::Installation,
    ) -> Result<ObjectResourceKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let frame = source
            .frames
            .get(index)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if frame.instance.is_some() {
            return Err(LoadedVisualError::Backend(
                "instance must share prior loaded mesh",
            ));
        }
        if frame.kind != 1 || !matches!(frame.visual, Some(0 | 1)) {
            return Err(LoadedVisualError::UnsupportedVisual(frame.visual));
        }
        if frame.lods.len() > 10 {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        if frame.visual == Some(1) {
            // Native null-material draw inherits culling. This bounded host
            // path requires an assigned material instead of guessing that state.
            for group in frame.lods.iter().flat_map(|lod| &lod.groups) {
                let material =
                    source
                        .materials
                        .get(group.material)
                        .ok_or(LoadedVisualError::Backend(
                            "unconstructed lit requires assigned material",
                        ))?;
                if !material.texture.is_empty()
                    && (self.material_load_context.loaded_material_flags(material) & 0xe0000000
                        != 0
                        || self.material_load_context.base_texture_flags(material)? & 0xe != 0)
                {
                    return Err(LoadedVisualError::Backend(
                        "unconstructed lit texture alpha/add/color-key unavailable",
                    ));
                }
            }
        }
        let mut materials = BTreeMap::new();
        for lod in &frame.lods {
            if lod.uv_only.is_some() {
                return Err(LoadedVisualError::Backend(
                    "UV-only LOD runtime materialization unavailable",
                ));
            }
            if lod.vertices.is_empty()
                || lod
                    .vertices
                    .iter()
                    .flat_map(|v| v.position)
                    .any(|v| !v.is_finite())
            {
                return Err(LoadedVisualError::InvalidGeometry);
            }
            for group in &lod.groups {
                if group.indices.len() % 3 != 0
                    || group
                        .indices
                        .iter()
                        .any(|i| *i > u16::MAX as u32 || *i as usize >= lod.vertices.len())
                {
                    return Err(LoadedVisualError::InvalidGeometry);
                }
                if group.material == lhr_assets::UNASSIGNED_MATERIAL
                    || materials.contains_key(&group.material)
                {
                    continue;
                }
                let m = source
                    .materials
                    .get(group.material)
                    .ok_or(LoadedVisualError::InvalidGeometry)?
                    .clone();
                if m.animation.is_some() {
                    return Err(LoadedVisualError::Backend(
                        "animated material ownership unavailable",
                    ));
                }
                let decode = |name: &str| -> Result<lhr_assets::Texture> {
                    let path = format!("maps/{name}");
                    let b = installation
                        .read(&path)
                        .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))?;
                    if name.to_ascii_lowercase().ends_with(".tga") {
                        lhr_assets::decode_tga(&b)
                    } else {
                        lhr_assets::decode_bmp_original(&b)
                    }
                    .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))
                };
                let mut inspection_keyed = false;
                let color = if m.texture.is_empty() {
                    None
                } else if self.inspection_rgba8_color_key
                    && self.material_load_context.base_texture_flags(&m)? & 0xe == 8
                    && m.alpha_texture.as_ref().is_none_or(|name| name.is_empty())
                    && !m.texture.to_ascii_lowercase().ends_with(".tga")
                {
                    let path = format!("maps/{}", m.texture);
                    let raw = installation
                        .read(&path)
                        .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))?;
                    let (image, keyed) = car_inspection_texture::decode_rgba8_color_key(&raw)
                        .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))?;
                    inspection_keyed = keyed;
                    Some(image)
                } else {
                    Some(decode(&m.texture)?)
                };
                let alpha = m
                    .alpha_texture
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .map(decode)
                    .transpose()?;
                if color.is_none() && alpha.is_some() {
                    return Err(LoadedVisualError::Backend("alpha without color"));
                }
                let alpha = match (&color, alpha) {
                    (Some(c), Some(a)) => Some(
                        crate::itexture::resize_loaded_texture(a, c.width, c.height)
                            .map_err(|_| LoadedVisualError::Backend("alpha image resize"))?,
                    ),
                    (_, alpha) => alpha,
                };
                let environment = m
                    .environment_texture
                    .as_deref()
                    .filter(|n| !n.is_empty())
                    .map(decode)
                    .transpose()?;
                materials.insert(
                    group.material,
                    PreparedMaterial {
                        source: m,
                        color,
                        alpha,
                        environment,
                        inspection_rgba8_color_key: inspection_keyed,
                    },
                );
            }
        }
        // All source validation and actual asset reads precede owned creation.
        for material in materials.values() {
            self.material_load_context
                .base_texture_flags(&material.source)?;
        }
        let result = self.admit_plain_object(frame, materials);
        if result.is_err() {
            self.incomplete = true;
        }
        result
    }
    fn admit_plain_object(
        &mut self,
        frame: &lhr_assets::Frame,
        materials: BTreeMap<usize, PreparedMaterial>,
    ) -> Result<ObjectResourceKey> {
        let mesh = self.objects.create_mesh_object()?;
        let object = if frame.visual == Some(1) {
            self.objects
                .create_unconstructed_lit_object_resources(Some(mesh))?
        } else {
            self.objects.create_object_resources(Some(mesh))?
        };
        self.objects.release_mesh_object(mesh, &mut self.backend)?;
        let mut mapped: BTreeMap<usize, MaterialKey> = BTreeMap::new();
        let mut source_materials = Vec::new();
        for (index, m) in materials {
            let key = self.objects.create_material()?;
            self.objects.install_loaded_material_scalars(
                key,
                &m.source,
                self.material_load_context.loaded_material_flags(&m.source),
            )?;
            if let Some(environment) = m.environment {
                let texture_key = format!(
                    "maps/{}|",
                    m.source.environment_texture.as_deref().unwrap_or("")
                );
                let texture = self.backend.admit_texture_with_flags(
                    &texture_key,
                    environment,
                    None,
                    self.material_load_context
                        .environment_texture_flags(&m.source),
                    self.material_load_context
                        .environment_texture_flags(&m.source)
                        & 0x10e,
                )?;
                self.objects.adopt_loaded_material_texture(
                    key,
                    Some(texture),
                    true,
                    &mut self.backend,
                )?;
                self.backend.release_texture(texture)?;
            }
            if let Some(color) = m.color {
                let texture_key = format!(
                    "maps/{}|{}",
                    m.source.texture,
                    m.source.alpha_texture.as_deref().unwrap_or("")
                );
                let texture = self.backend.admit_texture_with_flags(
                    &texture_key,
                    color,
                    m.alpha,
                    self.material_load_context
                        .base_texture_request_flags(&m.source),
                    self.material_load_context.base_texture_flags(&m.source)?,
                )?;
                if m.inspection_rgba8_color_key {
                    self.backend
                        .textures
                        .get_mut(&texture.0)
                        .ok_or(LoadedVisualError::Backend(
                            "inspection color-key texture unavailable",
                        ))?
                        .inspection_rgba8_color_key = true;
                }
                self.objects.adopt_loaded_material_texture(
                    key,
                    Some(texture),
                    false,
                    &mut self.backend,
                )?;
                self.backend.release_texture(texture)?;
            }
            source_materials.push(m.source);
            mapped.insert(index, key);
        }
        // Retain the source material records even if a later allocation fails.
        self.sources
            .insert((object.arena(), object.slot()), source_materials);
        for (index, lod) in frame.lods.iter().enumerate() {
            let level = self.objects.create_mesh_level()?;
            self.objects
                .install_loaded_lod_distance(level, lod.distance)?;
            let mut groups = Vec::new();
            for group in &lod.groups {
                if group.indices.is_empty() {
                    continue;
                }
                let key = self.objects.create_face_group()?;
                if let Some(m) = mapped.get(&group.material) {
                    self.objects
                        .set_material(key, Some(*m), &mut self.backend)?;
                }
                groups.push(key);
                if !self.objects.allocate_group_indices(
                    key,
                    (group.indices.len() / 3) as u32,
                    0,
                    &mut self.backend,
                )? {
                    return Err(LoadedVisualError::Backend("CPU index allocation rejected"));
                }
                let descriptor = self
                    .objects
                    .managed()
                    .resources()
                    .face_group(key)
                    .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
                    .index_buffer;
                let mut data = Vec::with_capacity(group.indices.len() * 2);
                for v in &group.indices {
                    data.extend_from_slice(&(*v as u16).to_le_bytes());
                }
                self.backend.upload_graphics(
                    GraphicsBuffer::Index(
                        descriptor
                            .graphics
                            .ok_or(LoadedVisualError::InvalidGeometry)?,
                    ),
                    descriptor.base as usize * 2,
                    &data,
                )?;
            }
            let request = VertexRequest {
                fvf: 0x112,
                usage: 0x10,
                stride: 32,
                count: u32::try_from(lod.vertices.len())
                    .map_err(|_| LoadedVisualError::InvalidGeometry)?,
                format: 6,
                exclusive: false,
            };
            if !self.objects.allocate_level_vertices(
                level,
                request.count,
                request,
                None,
                &mut self.backend,
            )? {
                return Err(LoadedVisualError::Backend("CPU vertex allocation rejected"));
            }
            let descriptor = self
                .objects
                .managed()
                .resources()
                .mesh_level(level)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
                .vertex_buffer;
            let mut data = Vec::with_capacity(lod.vertices.len() * 32);
            let mut bbox = [lod.vertices[0].position; 2];
            for vertex in &lod.vertices {
                for (axis, &position) in vertex.position.iter().enumerate() {
                    if position < bbox[0][axis] {
                        bbox[0][axis] = position;
                    }
                    if position > bbox[1][axis] {
                        bbox[1][axis] = position;
                    }
                }
                for value in vertex
                    .position
                    .into_iter()
                    .chain(vertex.normal)
                    .chain(vertex.uv)
                {
                    data.extend_from_slice(&value.to_bits().to_le_bytes());
                }
            }
            self.backend.upload_graphics(
                GraphicsBuffer::Vertex(
                    descriptor
                        .graphics
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
                ),
                descriptor.base as usize * 32,
                &data,
            )?;
            for group in groups {
                self.objects.append_duplicate_group(level, group)?;
            }
            self.objects.install_mesh_level_bbox(level, bbox)?;
            self.objects
                .set_lod(mesh, Some(level), index as u32, &mut self.backend)?;
            if frame.visual == Some(1) {
                let key = self.objects.create_unconstructed_lit_lod(object, index)?;
                let request = VertexRequest {
                    fvf: 0x242,
                    format: 1,
                    ..request
                };
                if !self
                    .objects
                    .allocate_plain_lod_vertices(key, request, &mut self.backend)?
                {
                    return Err(LoadedVisualError::Backend(
                        "CPU lit vertex allocation rejected",
                    ));
                }
                let descriptor = self.objects.lod(key)?.vertex_buffer;
                let mut bytes = vec![0; lod.vertices.len() * 32];
                for (vertex, bytes) in lod.vertices.iter().zip(bytes.as_chunks_mut::<32>().0) {
                    for (offset, value) in [0, 4, 8, 16, 20]
                        .into_iter()
                        .zip(vertex.position.into_iter().chain(vertex.uv))
                    {
                        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                    }
                }
                // Source hardware litLod::Create writes XYZ and UV1 only.
                // Host zero bytes in diffuse/UV2 are not original vertex data.
                self.backend.upload_graphics(
                    GraphicsBuffer::Vertex(
                        descriptor
                            .graphics
                            .ok_or(LoadedVisualError::InvalidGeometry)?,
                    ),
                    descriptor.base as usize * 32,
                    &bytes,
                )?;
                self.objects.finish_unconstructed_lit_geometry(key)?;
            }
            self.objects.release_mesh_level(level, &mut self.backend)?;
        }
        for material in mapped.into_values() {
            self.objects.release_material(material, &mut self.backend)?;
        }
        self.objects.update_mesh_bound_volume(mesh)?;
        self.objects.unlock_all_indices(&mut self.backend)?;
        self.objects.unlock_all_vertices(&mut self.backend)?;
        Ok(object)
    }
    pub fn destroy_object(&mut self, key: ObjectResourceKey) -> Result<()> {
        self.objects
            .destroy_object_resources(key, &mut self.backend)?;
        self.sources.remove(&(key.arena(), key.slot()));
        Ok(())
    }
    /// FreeUnusedVB maintenance over this owner's actual shared manager.
    /// Referenced blocks survive; individual object destruction does not sweep.
    pub fn reclaim_unused_vertices(&mut self) -> Result<()> {
        self.objects.reclaim_unused_vertices(&mut self.backend)?;
        Ok(())
    }
    /// FreeUnusedIB maintenance, independently callable after the vertex stage.
    pub fn reclaim_unused_indices(&mut self) -> Result<()> {
        self.objects.reclaim_unused_indices(&mut self.backend)?;
        Ok(())
    }
    /// Scene Close orders FreeUnusedVB (0x69820) before FreeUnusedIB (0x5b9d0).
    /// A failed vertex stage must not run the subsequent index stage.
    pub fn reclaim_unused_buffers(&mut self) -> Result<()> {
        self.reclaim_unused_vertices()?;
        self.reclaim_unused_indices()
    }
}

#[path = "vehicle_deform_resources.rs"]
mod deform_resources;
#[path = "vehicle_glass_resources.rs"]
mod vehicle_glass_resources;

#[cfg(test)]
mod tests {
    use super::*;
    fn image(value: u8) -> lhr_assets::Texture {
        lhr_assets::Texture {
            width: 1,
            height: 1,
            rgba: vec![value, value, value, 255],
        }
    }
    #[test]
    fn empty_alpha_is_normalized_before_managed_texture_flags_are_projected() {
        let source = lhr_assets::Material {
            flags: 0x40000800,
            ambient: [1.; 3],
            diffuse: [1.; 3],
            texture: "diffuse.bmp".into(),
            emission: [0.; 3],
            opacity: 1.,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        };
        let context = MaterialLoadContext {
            word_bfce0: 0,
            word_bfce4: 0,
            word_bfd04: 0,
            word_bfcd4: 0,
            word_bfcf4: 0,
        };
        assert_eq!(context.texture_flags(source.flags | 0x8000), 0x1044);
        assert_eq!(context.texture_flags(source.flags), 0x44);
        assert_eq!(context.base_texture_request_flags(&source), 0x40);
        assert_eq!(context.base_texture_flags(&source).unwrap(), 0);
    }
    #[test]
    fn texture_sharing_retains_color_and_alpha_until_actual_last_release() {
        let mut storage = CpuVisualStorage::default();
        let first = storage
            .admit_texture("maps/a.bmp|a+.bmp", image(1), Some(image(2)))
            .unwrap();
        let shared = storage
            .admit_texture("MAPS/A.BMP|A+.BMP", image(1), Some(image(2)))
            .unwrap();
        assert_eq!(first, shared);
        assert_eq!(storage.texture_references(first), Some(2));
        assert_eq!(storage.decoded_texture_bytes(), 8);
        let different = storage
            .admit_texture("maps/a.bmp|a+.bmp", image(3), Some(image(2)))
            .unwrap();
        assert_ne!(first, different);
        storage.release_texture(first).unwrap();
        assert_eq!(storage.decoded_texture_bytes(), 16);
        storage.release_texture(shared).unwrap();
        assert_eq!(storage.texture_references(first), None);
        assert!(storage.release_texture(first).is_err());
        storage.release_texture(different).unwrap();
        assert_eq!(storage.decoded_texture_bytes(), 0);
    }
    #[test]
    fn unequal_alpha_cpu_owner_uses_primary_extent_and_native_row_selection() {
        let mut storage = CpuVisualStorage::default();
        let color = lhr_assets::Texture {
            width: 16,
            height: 16,
            rgba: [1, 2, 3, 255].repeat(256),
        };
        let alpha = lhr_assets::Texture {
            width: 2,
            height: 64,
            rgba: (0..64u8)
                .flat_map(|y| [y, 1, 2, 255, y, 3, 4, 255])
                .collect(),
        };
        let key = storage
            .admit_texture_with_flags(
                "maps/light.bmp|alpha.bmp",
                color.clone(),
                Some(alpha.clone()),
                0x1384,
                4,
            )
            .unwrap();
        let actual = storage.textures[&key.0].alpha.as_ref().unwrap();
        assert_eq!((actual.width, actual.height), (16, 16));
        for y in 0..16 {
            for x in 0..16 {
                let offset = (y * 16 + x) * 4;
                assert_eq!(actual.rgba[offset], (3 + y * 4) as u8);
                assert_eq!(actual.rgba[offset + 1], if x < 8 { 1 } else { 3 });
            }
        }
        assert_eq!(storage.textures[&key.0].color.rgba, color.rgba);
        let hit = storage
            .admit_texture_with_flags("MAPS/LIGHT.BMP|ALPHA.BMP", color, Some(alpha), 0x1384, 4)
            .unwrap();
        assert_eq!(key, hit);
        assert_eq!(storage.texture_references(key), Some(2));
        storage.release_texture(key).unwrap();
        assert_eq!(storage.decoded_texture_bytes(), 16 * 16 * 8);
        storage.release_texture(hit).unwrap();
        assert_eq!(storage.decoded_texture_bytes(), 0);
    }
    #[test]
    fn graphics_owns_written_bytes_and_rejects_foreign_or_locked_release() {
        let mut storage = CpuVisualStorage::default();
        let mut foreign = CpuVisualStorage::default();
        let CreateResult::Created(graphics) = storage
            .create_graphics(GraphicsRequest {
                kind: BufferKind::Index,
                bytes: 8,
                usage: 0,
                pool: crate::buffer_managers::Pool::Managed,
                fvf: None,
            })
            .unwrap()
        else {
            panic!()
        };
        assert!(foreign.lock(graphics, 0).is_err());
        assert!(storage.write_graphics(graphics, 0, &[1]).is_err());
        storage.lock(graphics, 0).unwrap();
        storage.write_graphics(graphics, 2, &[4, 5, 6]).unwrap();
        assert!(storage.release_graphics(graphics).is_err());
        assert_eq!(
            storage.get_graphics_mut(graphics).unwrap().bytes,
            [0, 0, 4, 5, 6, 0, 0, 0]
        );
        storage.unlock(graphics).unwrap();
        storage.release_graphics(graphics).unwrap();
        assert_eq!(storage.graphics_bytes(), 0);
    }
    #[test]
    fn instance_last_release_frees_material_images_and_blocks_before_manager_reclaim() {
        let mut owner = LoadedVisualResources::new().unwrap();
        let mesh = owner.objects.create_mesh_object().unwrap();
        let object = owner.objects.create_object_resources(Some(mesh)).unwrap();
        owner
            .objects
            .release_mesh_object(mesh, &mut owner.backend)
            .unwrap();
        owner
            .sources
            .insert((object.arena(), object.slot()), vec![]);
        let material = owner.objects.create_material().unwrap();
        let texture = owner
            .backend
            .admit_texture("maps/x.bmp|", image(4), None)
            .unwrap();
        owner
            .objects
            .set_texture(material, Some(texture), &mut owner.backend)
            .unwrap();
        owner.backend.release_texture(texture).unwrap();
        let group = owner.objects.create_face_group().unwrap();
        owner
            .objects
            .set_material(group, Some(material), &mut owner.backend)
            .unwrap();
        owner
            .objects
            .release_material(material, &mut owner.backend)
            .unwrap();
        assert!(owner
            .objects
            .allocate_group_indices(group, 1, 0, &mut owner.backend)
            .unwrap());
        let level = owner.objects.create_mesh_level().unwrap();
        assert!(owner
            .objects
            .allocate_level_vertices(
                level,
                3,
                VertexRequest {
                    fvf: 0x112,
                    usage: 0,
                    stride: 32,
                    count: 3,
                    format: 0,
                    exclusive: false
                },
                Some(group),
                &mut owner.backend
            )
            .unwrap());
        owner
            .objects
            .install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
            .unwrap();
        owner
            .objects
            .set_lod(mesh, Some(level), 0, &mut owner.backend)
            .unwrap();
        owner
            .objects
            .release_mesh_level(level, &mut owner.backend)
            .unwrap();
        owner.objects.update_mesh_bound_volume(mesh).unwrap();
        owner
            .objects
            .unlock_all_indices(&mut owner.backend)
            .unwrap();
        owner
            .objects
            .unlock_all_vertices(&mut owner.backend)
            .unwrap();
        let instance = owner.load_plain_instance(object).unwrap();
        assert_eq!(
            owner.object_bounds(object).unwrap(),
            owner.object_bounds(instance).unwrap()
        );
        assert!(owner
            .objects
            .object(instance)
            .unwrap()
            .lods
            .iter()
            .all(Option::is_none));
        owner.destroy_object(object).unwrap();
        assert_eq!(owner.backend.live_textures(), 1);
        let retained = owner.backend.live_graphics();
        assert!(retained > 0);
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_graphics(), retained);
        owner.destroy_object(instance).unwrap();
        assert_eq!(owner.backend.live_textures(), 0);
        assert!(owner.backend.live_graphics() > 0);
        owner.reclaim_unused_vertices().unwrap();
        assert!(!owner.backend.graphics.is_empty());
        assert!(owner
            .backend
            .graphics
            .values()
            .all(|g| g.kind == BufferKind::Index));
        owner.reclaim_unused_indices().unwrap();
        assert_eq!(owner.backend.live_allocations(), 0);
        assert_eq!(owner.backend.live_graphics(), 0);
    }
    fn plain_frame_fixture() -> lhr_assets::Frame {
        lhr_assets::Frame {
            kind: 1,
            visual: Some(0),
            render_flags: 0,
            logic_flags: 0,
            culling: 0,
            name: "plain".into(),
            name_bytes: b"plain".to_vec(),
            properties: vec![],
            dummy_bounds: None,
            light: None,
            joint: None,
            target: None,
            occluder: None,
            sector: None,
            skin: None,
            morph: None,
            parent: None,
            position: [0.; 3],
            scale: [1.; 3],
            rotation_wxyz: [1., 0., 0., 0.],
            instance: None,
            lods: vec![lhr_assets::Lod {
                distance: 0.,
                uv_only: None,
                vertices: vec![
                    lhr_assets::Vertex {
                        position: [-1., 0., 0.],
                        normal: [0., 1., 0.],
                        uv: [0., 0.],
                    },
                    lhr_assets::Vertex {
                        position: [1., 0., 0.],
                        normal: [0., 1., 0.],
                        uv: [1., 0.],
                    },
                    lhr_assets::Vertex {
                        position: [0., 1., 0.],
                        normal: [0., 1., 0.],
                        uv: [0., 1.],
                    },
                ],
                groups: (0..3)
                    .map(|_| lhr_assets::FaceGroup {
                        material: lhr_assets::UNASSIGNED_MATERIAL,
                        indices: vec![0, 1, 2],
                    })
                    .collect(),
            }],
        }
    }
    #[test]
    fn uv_only_lod_has_an_explicit_runtime_boundary_before_owned_allocation() {
        let root = std::env::temp_dir().join(format!(
            "lhr-uv-only-{}-{}",
            std::process::id(),
            identity().unwrap()
        ));
        std::fs::create_dir(&root).unwrap();
        let installation = lhr_assets::Installation::open(&root).unwrap();
        let mut frame = plain_frame_fixture();
        let mut uv_lod = frame.lods[0].clone();
        uv_lod.uv_only = Some(uv_lod.vertices.iter().map(|v| v.uv).collect());
        uv_lod.vertices.clear();
        frame.lods.push(uv_lod);
        let source = lhr_assets::Model {
            materials: vec![],
            frames: vec![frame],
        };
        let mut owner = LoadedVisualResources::new().unwrap();
        let error = owner
            .load_plain_object(&source, 0, &installation)
            .unwrap_err();
        assert!(matches!(
            error,
            LoadedVisualError::Backend("UV-only LOD runtime materialization unavailable")
        ));
        assert!(owner.sources.is_empty());
        assert!(!owner.incomplete);
        drop(installation);
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn ordinary_open_ended_last_lod_keeps_rendering_without_fade_or_cull() {
        let mut frame = plain_frame_fixture();
        frame.lods[0].distance = 25.;
        let mut last = frame.lods[0].clone();
        last.distance = 0.;
        frame.lods.push(last);
        let mut owner = LoadedVisualResources::new().unwrap();
        let object = owner.admit_plain_object(&frame, BTreeMap::new()).unwrap();
        assert_eq!(
            owner.select_render_lod(object, 1e6, 1., 1., 0, 0).unwrap(),
            RenderLodSelection::Selected { index: 1, fade: 1. }
        );
        assert_eq!(owner.objects.object(object).unwrap().cur_lod, 1);
    }
    #[test]
    fn loaded_group_order_and_cached_array_upload_survive_two_objects() {
        let mut frame = plain_frame_fixture();
        frame.lods[0].distance = 17.25;
        let mut second = frame.lods[0].clone();
        second.distance = 93.5;
        frame.lods.push(second);
        let mut owner = LoadedVisualResources::new().unwrap();
        let a = owner.admit_plain_object(&frame, BTreeMap::new()).unwrap();
        let b = owner.admit_plain_object(&frame, BTreeMap::new()).unwrap();
        let mesh = owner.objects.object(a).unwrap().mesh.unwrap();
        let level = owner
            .objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .unwrap()
            .levels[0]
            .unwrap();
        let mut group = owner
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .unwrap()
            .fgroups;
        let mut bases = Vec::new();
        while let Some(key) = group {
            let record = owner.objects.managed().resources().face_group(key).unwrap();
            bases.push(record.index_buffer.base);
            group = record.next;
        }
        assert_eq!(bases, vec![0, 3, 6]);
        assert_eq!(
            owner.object_bounds(a).unwrap().bbox,
            [[-1., 0., 0.], [1., 1., 0.]]
        );
        let copy = owner.deform_create_empty_mesh().unwrap();
        owner.deform_duplicate_mesh(copy, Some(mesh)).unwrap();
        for (index, expected) in [(0, 17.25f32), (1, 93.5f32)] {
            for model in [mesh, copy] {
                let level = owner.deform_mesh_lod(model, 0, index).unwrap().unwrap();
                assert_eq!(
                    owner
                        .objects
                        .managed()
                        .resources()
                        .mesh_level(level)
                        .unwrap()
                        .lod_dist
                        .to_bits(),
                    expected.to_bits()
                );
            }
        }
        owner.deform_release_mesh(copy).unwrap();
        owner.destroy_object(a).unwrap();
        owner.destroy_object(b).unwrap();
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_allocations(), 0);
        assert_eq!(owner.backend.graphics_bytes(), 0);
    }
    fn admit_live_vertex_buffer(owner: &mut LoadedVisualResources) -> MeshLevelKey {
        let level = owner.objects.create_mesh_level().unwrap();
        assert!(owner
            .objects
            .allocate_level_vertices(
                level,
                3,
                VertexRequest {
                    fvf: 0x112,
                    usage: 0,
                    stride: 32,
                    count: 3,
                    format: 0,
                    exclusive: false,
                },
                None,
                &mut owner.backend
            )
            .unwrap());
        level
    }
    #[test]
    fn scene_resource_stages_preserve_live_vertices_and_survivor_then_reclaim_indices() {
        let frame = plain_frame_fixture();
        // Scene stages use the installed arena owner, including a mesh survivor.
        let mut arena = crate::model_loading::ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let resources = arena.visual_resource_owner_mut().unwrap();
        let original = resources
            .admit_plain_object(&frame, BTreeMap::new())
            .unwrap();
        let survivor = resources.load_plain_instance(original).unwrap();
        resources.destroy_object(original).unwrap();
        let retained = resources.backend.live_graphics();
        use crate::scene_ownership::SceneCloseEffect::{
            FreeUnusedIndexBuffers, FreeUnusedVertexBuffers,
        };
        arena
            .apply_scene_loaded_resource_effect(scene, FreeUnusedVertexBuffers)
            .unwrap();
        arena
            .apply_scene_loaded_resource_effect(scene, FreeUnusedIndexBuffers)
            .unwrap();
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend
                .live_graphics(),
            retained
        );
        arena
            .visual_resource_owner_mut()
            .unwrap()
            .destroy_object(survivor)
            .unwrap();
        let level = admit_live_vertex_buffer(arena.visual_resource_owner_mut().unwrap());
        assert!(arena
            .loaded_visual_resources()
            .unwrap()
            .backend
            .graphics
            .values()
            .any(|g| g.kind == BufferKind::Vertex));
        arena
            .apply_scene_loaded_resource_effect(scene, FreeUnusedVertexBuffers)
            .unwrap();
        assert!(arena
            .loaded_visual_resources()
            .unwrap()
            .backend
            .graphics
            .values()
            .any(|g| g.kind == BufferKind::Vertex));
        // Fully freed vertex buffers are destroyed by the targeted owner release.
        let resources = arena.visual_resource_owner_mut().unwrap();
        resources
            .objects
            .release_mesh_level(level, &mut resources.backend)
            .unwrap();
        assert!(arena
            .loaded_visual_resources()
            .unwrap()
            .backend
            .graphics
            .values()
            .all(|g| g.kind == BufferKind::Index));
        assert!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend
                .live_graphics()
                > 0
        );
        arena
            .apply_scene_loaded_resource_effect(scene, FreeUnusedIndexBuffers)
            .unwrap();
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend
                .live_allocations(),
            0
        );
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend
                .live_graphics(),
            0
        );
    }
    #[test]
    fn combined_reclamation_stops_before_indices_after_vertex_release_failure() {
        let frame = plain_frame_fixture();
        // A failed targeted vertex release poisons the shared manager;
        // combined maintenance must stop before touching cached index buffers.
        let mut interrupted = LoadedVisualResources::new().unwrap();
        let object = interrupted
            .admit_plain_object(&frame, BTreeMap::new())
            .unwrap();
        interrupted.destroy_object(object).unwrap();
        let level = admit_live_vertex_buffer(&mut interrupted);
        let indices: Vec<_> = interrupted
            .backend
            .graphics
            .iter()
            .filter(|(_, g)| g.kind == BufferKind::Index)
            .map(|(id, _)| *id)
            .collect();
        assert!(!indices.is_empty());
        // Controlled loss of CPU backing exercises an actual backend error.
        let vertex = interrupted
            .backend
            .graphics
            .iter()
            .find(|(_, g)| g.kind == BufferKind::Vertex)
            .map(|(id, _)| *id)
            .unwrap();
        interrupted.backend.graphics.remove(&vertex);
        assert!(interrupted
            .objects
            .release_mesh_level(level, &mut interrupted.backend)
            .is_err());
        assert!(interrupted.reclaim_unused_buffers().is_err());
        assert!(indices
            .iter()
            .all(|id| interrupted.backend.graphics.contains_key(id)));
    }
}

#[cfg(test)]
mod external_texture_tests {
    use super::*;

    #[test]
    fn actual_file_texture_shares_material_storage_and_releases_last_owner() {
        let root = std::env::temp_dir().join(format!(
            "lhr-projector-texture-{}-{}",
            std::process::id(),
            identity().unwrap()
        ));
        std::fs::create_dir_all(root.join("maps")).unwrap();
        let mut tga = vec![0; 18];
        tga[2] = 2;
        tga[12] = 1;
        tga[14] = 1;
        tga[16] = 24;
        tga[17] = 0x20;
        tga.extend_from_slice(&[3, 5, 7]);
        std::fs::write(root.join("maps/shared.tga"), tga).unwrap();
        let installation = lhr_assets::Installation::open(&root).unwrap();
        let mut resources = LoadedVisualResources::new().unwrap();
        let material = resources
            .backend
            .admit_texture(
                "maps/shared.tga|",
                lhr_assets::Texture {
                    width: 1,
                    height: 1,
                    rgba: vec![7, 5, 3, 255],
                },
                None,
            )
            .unwrap();
        let projector = resources
            .load_cpu_texture(&installation, "shared.tga")
            .unwrap();
        assert_eq!(projector, material);
        assert_eq!(resources.backend().texture_references(projector), Some(2));
        resources.retain_cpu_texture(projector).unwrap();
        resources.release_cpu_texture(material).unwrap();
        resources.release_cpu_texture(projector).unwrap();
        assert_eq!(
            resources.cpu_texture(projector).unwrap().rgba,
            [7, 5, 3, 255]
        );
        assert_eq!(resources.backend().texture_references(projector), Some(1));
        resources.release_cpu_texture(projector).unwrap();
        assert!(resources.cpu_texture(projector).is_err());
        assert_eq!(resources.backend().decoded_texture_bytes(), 0);
        assert!(resources
            .load_cpu_texture(&installation, "missing.tga")
            .is_err());
        assert_eq!(resources.backend().live_textures(), 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Distinguishes an actual selected level from the one-past-last render cull.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderLodSelection {
    Selected { index: u32, fade: f32 },
    Missing { index: u32 },
    Culled { index: u32 },
}
impl LoadedVisualResources {
    /// Source no-callback selection on the SAME live object owner. Lit objects
    /// use common GetObjectLOD but skip the ordinary SetFade block.
    /// No Texture::Request, callback dispatch or special visual is invented.
    pub fn select_render_lod(
        &mut self,
        object: ObjectResourceKey,
        distance_squared: f32,
        quality: f32,
        world_scale: f32,
        frame_flags: u32,
        visual_flags: u32,
    ) -> Result<RenderLodSelection> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        if frame_flags & 0x400 != 0 {
            return Err(LoadedVisualError::Backend("LOD callback unavailable"));
        }
        if visual_flags & (0x10 | 0x40 | 0x10000) != 0 {
            return Err(LoadedVisualError::Backend(
                "texture-driven, world-space or proxy LOD unavailable",
            ));
        }
        let record = self.objects.object(object)?;
        let lit = record.visual_type() == crate::i3d_object::ObjectVisualType::Lit;
        let Some(mesh) = record.mesh else {
            return Ok(RenderLodSelection::Missing { index: 0 });
        };
        let mesh = self
            .objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        let count = mesh.num_lods as usize;
        if count == 0 {
            return Ok(RenderLodSelection::Missing { index: 0 });
        }
        if count > mesh.levels.len() {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let distances = mesh.levels[..count]
            .iter()
            .map(|key| {
                key.map(|key| {
                    self.objects
                        .managed()
                        .resources()
                        .mesh_level(key)
                        .map(|level| level.lod_dist)
                        .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
                })
                .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        let levels = mesh.levels;
        let index = self
            .objects
            .get_object_lod(object, distance_squared, quality, world_scale)
            .map_err(|_| LoadedVisualError::Backend("GetObjectLOD unavailable"))?;
        if index as usize == count {
            return Ok(RenderLodSelection::Culled { index });
        }
        let Some(level) = levels[index as usize] else {
            return Ok(RenderLodSelection::Missing { index });
        };
        let level = self
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        if level.fgroups.is_none() || level.num_vertices == 0 {
            return Ok(RenderLodSelection::Missing { index });
        }
        // Source lit m_AddFGroups calls GetObjectLOD but has no ordinary
        // last-LOD SetFade block. Do not mutate fade or base alpha-dirty slots.
        if lit {
            return Ok(RenderLodSelection::Selected { index, fade: 1. });
        }
        let fade = crate::i3d_object::render_fade(
            distance_squared,
            quality,
            world_scale,
            index as usize,
            &distances,
        )
        .map_err(|_| LoadedVisualError::Backend("LOD fade arithmetic unavailable"))?;
        self.objects
            .set_render_fade(object, fade)
            .map_err(|_| LoadedVisualError::Backend("SetFade unavailable"))?;
        Ok(RenderLodSelection::Selected { index, fade })
    }
}

#[cfg(test)]
mod unconstructed_lit_admission_tests {
    use super::*;
    #[test]
    fn lit_rejects_unsupported_material_branches_before_asset_reads_or_owned_creation() {
        let path = std::env::temp_dir().join(format!(
            "lhr-lit-boundary-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.visual = Some(1);
        frame.lods = vec![lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: vec![
                lhr_assets::Vertex {
                    position: [0.; 3],
                    normal: [0.; 3],
                    uv: [0.; 2]
                };
                3
            ],
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            }],
        }];
        let mut source = lhr_assets::Model {
            frames: vec![frame],
            materials: vec![lhr_assets::Material {
                flags: 0x40000,
                ambient: [0.; 3],
                diffuse: [1.; 3],
                texture: "missing.bmp".into(),
                emission: [0.; 3],
                opacity: 1.,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
        };
        for flags in [0x20000000, 0x40000000, 0x80000000] {
            source.materials[0].flags = flags | 0x40000;
            let mut owner = LoadedVisualResources::new().unwrap();
            assert!(matches!(
                owner.load_unconstructed_lit_object(&source, 0, &installation),
                Err(LoadedVisualError::Backend(
                    "unconstructed lit texture alpha/add/color-key unavailable"
                ))
            ));
            assert_eq!(owner.backend.live_allocations(), 0);
            assert_eq!(owner.backend.live_graphics(), 0);
            assert_eq!(owner.backend.live_textures(), 0);
            assert!(owner.sources.is_empty());
            assert!(!owner.incomplete);
        }
        source.materials[0].flags = 0x40000;
        source.materials[0].alpha_texture = Some("missing-alpha.bmp".into());
        let mut owner = LoadedVisualResources::new().unwrap();
        assert!(matches!(
            owner.load_unconstructed_lit_object(&source, 0, &installation),
            Err(LoadedVisualError::Backend(
                "unconstructed lit texture alpha/add/color-key unavailable"
            ))
        ));
        // Stage 1's retained UV-only records must keep their runtime boundary
        // after Stage 8a routes both Plain and Lit through shared admission.
        source.materials[0].alpha_texture = None;
        let full_lod = source.frames[0].lods[0].clone();
        source.frames[0].lods[0].uv_only = Some(vec![[0.; 2]; 3]);
        source.frames[0].lods[0].vertices.clear();
        assert!(matches!(
            owner.load_unconstructed_lit_object(&source, 0, &installation),
            Err(LoadedVisualError::Backend(
                "UV-only LOD runtime materialization unavailable"
            ))
        ));
        assert_eq!(owner.backend.live_allocations(), 0);
        assert_eq!(owner.backend.live_graphics(), 0);
        assert_eq!(owner.backend.live_textures(), 0);
        assert!(owner.sources.is_empty());
        assert!(!owner.incomplete);
        source.frames[0].lods[0] = full_lod;
        source.frames[0].lods[0].groups[0].material = lhr_assets::UNASSIGNED_MATERIAL;
        assert!(matches!(
            owner.load_unconstructed_lit_object(&source, 0, &installation),
            Err(LoadedVisualError::Backend(
                "unconstructed lit requires assigned material"
            ))
        ));
        assert!(owner.load_plain_object(&source, 0, &installation).is_err());
        source.frames[0].visual = Some(4);
        assert_eq!(
            owner.load_unconstructed_lit_object(&source, 0, &installation),
            Err(LoadedVisualError::UnsupportedVisual(Some(4)))
        );
        assert_eq!(owner.backend.live_allocations(), 0);
        assert!(!owner.incomplete);
        std::fs::remove_dir_all(path).unwrap();
    }
}
