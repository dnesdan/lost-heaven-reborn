//! Bounded immediate filename factory (PC 6c230/6c2c0/75f50).
//! CPU adapter selects D3DFMT_A8R8G8B8 for all three uncompressed format globals,
//! max width/height 1024, and hardware caps zero (power-of-two minimum 1, no DXT).
//! These are the controlled native comparison's globals, not detected GPU caps.
//! Admission also excludes dimensions above 1024 with ANYSIZE; that exclusion is
//! host policy rather than an original Create error.
//! It executes channel/alpha conversion into owned BGRA surface storage. Device
//! upload, presaved files, resizing, mipmaps, compression and managed Create are
//! outside this adapter and reject explicitly, rather than pretending to execute.
use super::*;
use crate::buffer_managers::{
    AllocationToken, BufferKey, BufferKind, CreateResult, GraphicsBuffer, GraphicsRequest,
    LockedDataKey,
};
use crate::c_str::StringAllocation;
use std::collections::BTreeMap;

/// Fixed CPU admission context, matching the controlled original comparison.
pub const CPU_TEXTURE_MAX_WIDTH: u16 = 1024;
pub const CPU_TEXTURE_MAX_HEIGHT: u16 = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureImageError {
    Missing,
    UnknownFormat,
    Corrupt,
    Unsupported,
    Backend(String),
}
pub trait TextureImageSource {
    /// Implements FindMapFile against the caller's ordered map directories.
    /// None selects the native fallback `Maps\\` + requested name.
    fn find_map_file(&mut self, _name: &[u8]) -> Result<Option<Vec<u8>>, TextureImageError> {
        Ok(None)
    }
    fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, TextureImageError>;
}
#[derive(Debug, PartialEq, Eq)]
pub enum TextureFactoryError {
    State(TextureError),
    InvalidParameters,
    Image(TextureImageError),
    AlphaImage(TextureImageError),
    Unsupported(&'static str),
    Backend(&'static str),
}
impl TextureFactoryError {
    /// Only these errors model an original negative LS3D_RESULT. A caller that
    /// ignores native factory failure must still propagate host capability errors.
    pub fn is_native_creation_failure(&self) -> bool {
        matches!(
            self,
            Self::InvalidParameters
                | Self::Image(
                    TextureImageError::Missing
                        | TextureImageError::UnknownFormat
                        | TextureImageError::Corrupt
                )
                | Self::AlphaImage(
                    TextureImageError::Missing
                        | TextureImageError::UnknownFormat
                        | TextureImageError::Corrupt
                )
        )
    }
}
impl From<TextureError> for TextureFactoryError {
    fn from(e: TextureError) -> Self {
        Self::State(e)
    }
}
impl From<StringError> for TextureFactoryError {
    fn from(e: StringError) -> Self {
        Self::State(e.into())
    }
}
impl From<TextureOperationError<&'static str>> for TextureFactoryError {
    fn from(e: TextureOperationError<&'static str>) -> Self {
        match e {
            TextureOperationError::State(e) => Self::State(e),
            TextureOperationError::Backend(e) => Self::Backend(e),
            TextureOperationError::Manager(_) => Self::Unsupported("managed texture"),
        }
    }
}
impl From<StringOperationError<&'static str>> for TextureFactoryError {
    fn from(e: StringOperationError<&'static str>) -> Self {
        match e {
            StringOperationError::State(e) => e.into(),
            StringOperationError::Backend(e) => Self::Backend(e),
        }
    }
}
#[derive(Debug)]
pub struct CpuTextureSurface {
    pub width: u16,
    pub height: u16,
    pub format: u32,
    pub bgra: Vec<u8>,
}
/// Actual host ownership backing for immediate ITexture allocations. Identities
/// are unique in this backend; every canonical Close removes its owned entries.
pub struct CpuTextureStorage {
    identity: u64,
    strings: BTreeMap<u64, Vec<u8>>,
    graphics: BTreeMap<u64, CpuTextureSurface>,
    pixels: BTreeMap<u64, Vec<u8>>,
    owners: BTreeMap<u64, TextureKey>,
    texture_section: Option<std::sync::MutexGuard<'static, ()>>,
}
impl Default for CpuTextureStorage {
    fn default() -> Self {
        static NEXT_STORAGE: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT_STORAGE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("CPU texture backend identity exhausted");
        Self {
            identity,
            strings: BTreeMap::new(),
            graphics: BTreeMap::new(),
            pixels: BTreeMap::new(),
            owners: BTreeMap::new(),
            texture_section: None,
        }
    }
}
impl CpuTextureStorage {
    pub fn identity(&self) -> u64 {
        self.identity
    }
    fn id(&mut self) -> Result<u64, &'static str> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| "texture allocation identity overflow")
    }
    pub fn surface(&self, texture: TextureGraphics) -> Option<&CpuTextureSurface> {
        self.graphics.get(&texture.0)
    }
    pub fn pixel_bytes(&self, pixels: PixelAllocation) -> Option<&[u8]> {
        self.pixels.get(&pixels.0).map(Vec::as_slice)
    }
    pub fn live_graphics(&self) -> usize {
        self.graphics.len()
    }
    pub fn live_pixels(&self) -> usize {
        self.pixels.len()
    }
    pub fn live_strings(&self) -> usize {
        self.strings.len()
    }
    pub fn resident_bytes(&self) -> usize {
        self.graphics.values().map(|s| s.bgra.len()).sum::<usize>()
            + self.pixels.values().map(Vec::len).sum::<usize>()
            + self.strings.values().map(Vec::len).sum::<usize>()
    }
}
impl CStrBackend for CpuTextureStorage {
    type Error = &'static str;
    fn allocate_string(&mut self, n: u32) -> Result<Option<StringAllocation>, Self::Error> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(n as usize)
            .map_err(|_| "texture string allocation")?;
        bytes.resize(n as usize, 0);
        let id = self.id()?;
        self.strings.insert(id, bytes);
        Ok(Some(StringAllocation(id)))
    }
    fn free_string(&mut self, k: StringAllocation) -> Result<(), Self::Error> {
        self.strings
            .remove(&k.0)
            .map(|_| ())
            .ok_or("unknown texture string")
    }
}
impl BufferBackend for CpuTextureStorage {
    type Error = &'static str;
    fn allocate_buffer(&mut self, _: BufferKind) -> Result<AllocationToken, Self::Error> {
        Err("texture-only backend")
    }
    fn free_buffer(&mut self, _: AllocationToken) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
    fn allocate_block(
        &mut self,
        _: BufferKind,
        _: u32,
        _: u32,
    ) -> Result<AllocationToken, Self::Error> {
        Err("texture-only backend")
    }
    fn free_block(&mut self, _: AllocationToken) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
    fn create_graphics(&mut self, _: GraphicsRequest) -> Result<CreateResult, Self::Error> {
        Err("texture-only backend")
    }
    fn discard_managed(&mut self) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
    fn lock(&mut self, _: GraphicsBuffer, _: u32) -> Result<LockedDataKey, Self::Error> {
        Err("texture-only backend")
    }
    fn unlock(&mut self, _: GraphicsBuffer) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
    fn release_graphics(&mut self, _: GraphicsBuffer) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
    fn driver_present(&self) -> bool {
        false
    }
    fn recompute_vram(&mut self) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
    fn warn_unreleased_buffer(&mut self, _: BufferKey) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
    fn out_of_memory(&mut self, _: BufferKind, _: u32, _: i32) -> Result<(), Self::Error> {
        Err("texture-only backend")
    }
}
impl ManagedTextureEffects for CpuTextureStorage {
    type Error = &'static str;
    fn enter_texture_section(&mut self) -> Result<(), Self::Error> {
        static SECTION: std::sync::Mutex<()> = std::sync::Mutex::new(());
        if self.texture_section.is_some() {
            return Err("texture section already held");
        }
        self.texture_section = Some(SECTION.lock().map_err(|_| "poisoned texture section")?);
        Ok(())
    }
    fn leave_texture_section(&mut self) -> Result<(), Self::Error> {
        drop(
            self.texture_section
                .take()
                .ok_or("texture section not held")?,
        );
        Ok(())
    }
    fn delete_managed_filename(
        &mut self,
        _: Option<crate::texture_manager::ManagedFilenameAllocation>,
    ) -> Result<(), Self::Error> {
        Err("immediate-only texture backend")
    }
    fn delete_managed_record(&mut self, _: ManagedRecordKey) -> Result<(), Self::Error> {
        Err("immediate-only texture backend")
    }
}
impl TextureBackend for CpuTextureStorage {
    fn managed_texture_effects(
        &mut self,
    ) -> Option<&mut dyn ManagedTextureEffects<Error = &'static str>> {
        Some(self)
    }

    fn unlock_surface(&mut self, _: TextureSurface) -> Result<(), &'static str> {
        Err("no locked CPU texture surface")
    }
    fn release_surface(&mut self, _: TextureSurface) -> Result<(), &'static str> {
        Err("no borrowed CPU texture surface")
    }
    fn release_texture_graphics(&mut self, k: TextureGraphics) -> Result<(), &'static str> {
        self.graphics
            .remove(&k.0)
            .ok_or("unknown CPU texture graphics")?;
        self.owners.remove(&k.0);
        Ok(())
    }
    fn delete_pixels(&mut self, k: PixelAllocation) -> Result<(), &'static str> {
        self.pixels
            .remove(&k.0)
            .ok_or("unknown CPU texture pixels")?;
        self.owners.remove(&k.0);
        Ok(())
    }
    fn free_raw_vertices(
        &mut self,
        _: MeshLevelKey,
        _: RawVertexBytesKey,
    ) -> Result<(), &'static str> {
        Err("texture-only backend")
    }
}
fn cbytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())]
}
fn basename_offset(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .rposition(|b| *b == b'\\')
        .map_or(bytes.len() as u32, |n| n as u32 + 1)
}
fn full_name(
    source: &mut impl TextureImageSource,
    name: &[u8],
) -> Result<Vec<u8>, TextureFactoryError> {
    if let Some(path) = source
        .find_map_file(name)
        .map_err(TextureFactoryError::Image)?
    {
        if !path.is_empty() {
            return Ok(path);
        }
    }
    let mut full = b"Maps\\".to_vec();
    full.extend_from_slice(name);
    Ok(full)
}
type Image = lhr_assets::texture_source::DecodedTextureImage;
fn decode_error(e: lhr_assets::texture_source::TextureImageDecodeError) -> TextureImageError {
    match e {
        lhr_assets::texture_source::TextureImageDecodeError::Unsupported(_) => {
            TextureImageError::Unsupported
        }
        lhr_assets::texture_source::TextureImageDecodeError::Corrupt(_) => {
            TextureImageError::Corrupt
        }
    }
}
fn decode(bytes: &[u8]) -> Result<Image, TextureImageError> {
    lhr_assets::texture_source::decode_texture_image(bytes).map_err(decode_error)
}
impl TextureResources {
    /// Execute Unload's critical section for this actual immediate-only world.
    /// A populated managed domain needs its own capable storage backend.
    pub fn unload_cpu_immediate_textures(
        &mut self,
        storage: &mut CpuTextureStorage,
    ) -> Result<(), TextureFactoryError> {
        self.available()?;
        let manager = &self.textures.manager;
        if manager.pools().iter().any(|p| !p.is_empty())
            || !manager.active_records().is_empty()
            || manager.queue().iter().any(Option::is_some)
            || self
                .textures
                .slots
                .iter()
                .flatten()
                .any(|t| t.state == TextureState::Live && t.flags & 0xc0000040 != 0)
        {
            return Err(TextureFactoryError::Unsupported("managed texture domain"));
        }
        self.prove_cpu_immediate_domain(storage)?;
        self.unload_managed_textures(storage)?;
        Ok(())
    }
    /// Prove allocation ownership before a bounded CPU-only device/manager
    /// operation. Public ownership intake is not evidence of CPU residency.
    pub fn prove_cpu_immediate_domain(
        &self,
        storage: &CpuTextureStorage,
    ) -> Result<(), TextureFactoryError> {
        self.available()?;
        for (index, t) in self
            .textures
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.as_ref().map(|t| (i, t)))
        {
            if t.state != TextureState::Live {
                continue;
            }
            let key = TextureKey {
                arena: self.textures.identity,
                slot: index as u32 + 1,
            };
            if t.intake_installed
                || t.managed_admitted
                || t.managed_prepared
                || t.managed_created
                || t.lock_surface.is_some()
                || t.managed_record.is_some()
                || t.flags & 0xc0000040 != 0
                || t.graphics.is_some_and(|g| {
                    !storage.graphics.contains_key(&g.0) || storage.owners.get(&g.0) != Some(&key)
                })
                || t.pixels.is_some_and(|p| {
                    !storage.pixels.contains_key(&p.0) || storage.owners.get(&p.0) != Some(&key)
                })
                || (t.initialized && t.graphics.is_none())
            {
                return Err(TextureFactoryError::Unsupported(
                    "foreign or opaque texture residency",
                ));
            }
        }
        Ok(())
    }
    /// Original blank factory's UID sequence, distinct from host arena identity.
    pub fn create_igraph_texture(&mut self) -> Result<TextureKey, TextureError> {
        let key = self.create_texture()?;
        self.next_texture_uid += 1;
        self.textures.texture_mut(key).uid = self.next_texture_uid;
        if self.next_texture_uid >= 0x7fff {
            self.next_texture_uid = 0;
        }
        Ok(key)
    }
    /// Filename/cache factory. Errors own no returned reference; the fresh
    /// canonical texture is released through its actual destructor on failure.
    pub fn create_texture_from_file(
        &mut self,
        name: Option<&[u8]>,
        alpha: Option<&[u8]>,
        mut flags: u32,
        source: &mut impl TextureImageSource,
        storage: &mut CpuTextureStorage,
    ) -> Result<TextureKey, TextureFactoryError> {
        self.available()?;
        let name = name.map(cbytes);
        let alpha = alpha.map(cbytes);
        if name.is_some_and(|n| !n.is_ascii()) || alpha.is_some_and(|n| !n.is_ascii()) {
            return Err(TextureFactoryError::Unsupported("filename encoding"));
        }
        if flags & 4 != 0 && !alpha.is_some_and(|a| !a.is_empty()) {
            // Native can publish a cached object with a null alpha-name and
            // later pass that null to strcmpi. This unchecked domain is excluded.
            return Err(TextureFactoryError::Unsupported(
                "alpha flag without alpha filename",
            ));
        }
        if alpha.is_some_and(|a| !a.is_empty()) {
            flags |= 4;
        }
        if flags & 0x200001 == 0 {
            let name = name
                .filter(|n| !n.is_empty())
                .ok_or(TextureFactoryError::InvalidParameters)?;
            for key in self.textures.cached.iter().rev().copied() {
                let t = self.textures.live(key)?;
                let matches = |view: Observed<Option<RepView>>,
                               bytes: Option<&[u8]>|
                 -> Result<bool, TextureFactoryError> {
                    match (view, bytes) {
                        (Observed::Value(Some(view)), Some(bytes)) => {
                            Ok(self.strings.view(view)?.eq_ignore_ascii_case(bytes))
                        }
                        _ => Ok(false),
                    }
                };
                if t.creation_flags == flags
                    && matches(t.name, Some(name))?
                    && (flags & 4 == 0 || matches(t.alpha_name, alpha)?)
                {
                    self.retain_texture(key)?;
                    return Ok(key);
                }
            }
        }
        // This bounded adapter has no substitute paths for unsupported Create branches.
        if flags & !(2 | 4 | 8 | 0x1000 | 0x200000 | 0x400000) != 0 {
            return Err(TextureFactoryError::Unsupported("Create flags"));
        }
        let key = self.create_igraph_texture()?;
        let result = (|| {
            let name = name.ok_or(TextureFactoryError::Unsupported("empty texture"))?;
            let full = full_name(source, name)?;
            let alpha_full = alpha
                .filter(|a| !a.is_empty())
                .map(|a| full_name(source, a))
                .transpose()?;
            self.create_immediate_file(key, &full, alpha_full.as_deref(), flags, source, storage)?;
            Ok(key)
        })();
        if result.is_err() {
            self.release_texture(key, storage)?;
        }
        result
    }
    fn create_immediate_file(
        &mut self,
        key: TextureKey,
        name: &[u8],
        alpha: Option<&[u8]>,
        flags: u32,
        source: &mut impl TextureImageSource,
        storage: &mut CpuTextureStorage,
    ) -> Result<(), TextureFactoryError> {
        self.close_texture(key, storage)?;
        let t = self.textures.texture_mut(key);
        t.flags = flags;
        t.creation_flags = flags;
        let file = t.file_name;
        self.strings.assign(file, Some(name), storage)?;
        let extension = name
            .iter()
            .rposition(|b| *b == b'.')
            .and_then(|at| name.get(at + 1))
            .copied();
        let bitmap = match extension.map(|b| b.to_ascii_lowercase()) {
            Some(b'b') => true,
            Some(b't') => false,
            _ => return Err(TextureFactoryError::Image(TextureImageError::UnknownFormat)),
        };
        let source_bytes = source
            .read_image(name)
            .map_err(TextureFactoryError::Image)?;
        if bitmap != source_bytes.starts_with(b"BM") {
            return Err(TextureFactoryError::Unsupported(
                "source extension/container mismatch",
            ));
        }
        let info = lhr_assets::texture_source::inspect_texture_image(&source_bytes)
            .map_err(decode_error)
            .map_err(TextureFactoryError::Image)?;
        let width = u16::try_from(info.width)
            .map_err(|_| TextureFactoryError::Unsupported("image dimensions"))?;
        let height = u16::try_from(info.height)
            .map_err(|_| TextureFactoryError::Unsupported("image dimensions"))?;
        let rep = self
            .strings
            .string(file)?
            .rep
            .ok_or(TextureError::IntakeAllocationRejected)?;
        let t = self.textures.texture_mut(key);
        t.name = Observed::Value(Some(RepView {
            rep,
            offset: basename_offset(name),
        }));
        t.width = Observed::Value(width);
        t.height = Observed::Value(height);
        if flags & 2 != 0 && matches!(info.bpp, 8 | 24) {
            t.flags &= !2;
        }
        if flags & 2 != 0 && info.bpp == 16 {
            t.flags |= 8;
        }
        if width > CPU_TEXTURE_MAX_WIDTH || height > CPU_TEXTURE_MAX_HEIGHT {
            return Err(TextureFactoryError::Unsupported(
                "CPU texture dimensions exceed fixed caps",
            ));
        }
        if flags & 0x400000 == 0 && (!width.is_power_of_two() || !height.is_power_of_two()) {
            return Err(TextureFactoryError::Unsupported("image resize"));
        }
        let image = decode(&source_bytes).map_err(TextureFactoryError::Image)?;
        let alpha_image = if t.flags & 4 != 0 {
            if let Some(alpha) = alpha {
                let field = t.alpha_file_name;
                self.strings.assign(field, Some(alpha), storage)?;
                let a = decode(
                    &source
                        .read_image(alpha)
                        .map_err(TextureFactoryError::AlphaImage)?,
                )
                .map_err(TextureFactoryError::AlphaImage)?;
                if !matches!(a.bpp, 8 | 24) {
                    return Err(TextureFactoryError::Unsupported("alpha source depth"));
                }
                if a.source_pitch != a.texture.width * u32::from(a.bpp) / 8 {
                    // Native alpha extraction walks consecutive pixels without
                    // skipping BMP row padding. That behavior is not reproduced
                    // by the packed RGBA decoder; reject rather than repair it.
                    return Err(TextureFactoryError::Unsupported("alpha source row padding"));
                }
                if (a.texture.width, a.texture.height)
                    != (image.texture.width, image.texture.height)
                {
                    return Err(TextureFactoryError::Unsupported("alpha resize"));
                }
                let rep = self
                    .strings
                    .string(field)?
                    .rep
                    .ok_or(TextureError::IntakeAllocationRejected)?;
                self.textures.texture_mut(key).alpha_name = Observed::Value(Some(RepView {
                    rep,
                    offset: basename_offset(alpha),
                }));
                Some(a)
            } else {
                self.textures.texture_mut(key).flags &= !4;
                None
            }
        } else {
            None
        };
        let flags = self.textures.texture(key)?.flags;
        let mut bgra = Vec::new();
        bgra.try_reserve_exact(image.texture.rgba.len())
            .map_err(|_| TextureError::HostAllocation)?;
        let color_key = image.palette_key.map_or(0, |p| {
            (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32
        });
        for (i, p) in image.texture.rgba.as_chunks::<4>().0.iter().enumerate() {
            let a = if flags & 2 != 0 {
                p[3]
            } else if let Some(alpha) = &alpha_image {
                let p = &alpha.texture.rgba[i * 4..i * 4 + 4];
                ((p[0] as u32 * 77 + p[1] as u32 * 154 + p[2] as u32 * 25) >> 8) as u8
            } else if flags & 8 != 0
                && image.palette_key.map_or(
                    if image.bpp == 16 {
                        p.iter().all(|v| *v == 0)
                    } else {
                        p[2] == 0
                    },
                    |key| p[..3] == key,
                )
            {
                0
            } else {
                255
            };
            bgra.extend_from_slice(&[p[2], p[1], p[0], a]);
        }
        let id = storage.id().map_err(TextureFactoryError::Backend)?;
        let memory_size = u32::try_from(bgra.len()).map_err(|_| TextureError::HostAllocation)?;
        storage.graphics.insert(
            id,
            CpuTextureSurface {
                width,
                height,
                format: 21,
                bgra,
            },
        );
        storage.owners.insert(id, key);
        let t = self.textures.texture_mut(key);
        t.graphics = Some(TextureGraphics(id));
        t.format = Observed::Value(21);
        t.levels = Observed::Value(1);
        t.memory_size = memory_size;
        t.color_key = Observed::Value(color_key);
        if flags & 0x1000 != 0 {
            let mut argb = Vec::new();
            argb.try_reserve_exact(image.texture.rgba.len())
                .map_err(|_| TextureError::HostAllocation)?;
            for p in storage.graphics[&id].bgra.as_chunks::<4>().0.iter() {
                argb.extend_from_slice(&[p[3], p[2], p[1], p[0]]);
            }
            let id = storage.id().map_err(TextureFactoryError::Backend)?;
            storage.pixels.insert(id, argb);
            storage.owners.insert(id, key);
            self.textures.texture_mut(key).pixels = Some(PixelAllocation(id));
        }
        let list = if flags & 0x200000 == 0 {
            &mut self.textures.cached
        } else {
            &mut self.textures.uncached
        };
        list.try_reserve(1)
            .map_err(|_| TextureError::HostAllocation)?;
        list.push(key);
        self.textures.texture_mut(key).initialized = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Source {
        files: BTreeMap<Vec<u8>, Vec<u8>>,
        reads: Vec<Vec<u8>>,
        found: Option<Vec<u8>>,
    }
    impl TextureImageSource for Source {
        fn find_map_file(&mut self, _: &[u8]) -> Result<Option<Vec<u8>>, TextureImageError> {
            Ok(self.found.clone())
        }
        fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            self.reads.push(path.to_vec());
            self.files
                .get(path)
                .cloned()
                .ok_or(TextureImageError::Missing)
        }
    }
    fn tga(depth: u8, pixels: &[u8]) -> Vec<u8> {
        let mut b = vec![0; 18];
        b[2] = 2;
        b[12] = 2;
        b[14] = 2;
        b[16] = depth;
        b[17] = 0x20;
        b.extend_from_slice(pixels);
        b
    }
    fn setup(depth: u8, pixels: &[u8]) -> (TextureResources, CpuTextureStorage, Source) {
        let mut source = Source::default();
        source
            .files
            .insert(b"Maps\\color.tga".to_vec(), tga(depth, pixels));
        (
            TextureResources::new().unwrap(),
            CpuTextureStorage::default(),
            source,
        )
    }
    #[test]
    fn canonical_cache_case_flags_and_last_reference_owns_surface() {
        let (mut r, mut b, mut s) =
            setup(32, &[3, 2, 1, 17, 6, 5, 4, 34, 9, 8, 7, 51, 12, 11, 10, 68]);
        let a = r
            .create_texture_from_file(Some(b"color.tga"), None, 2, &mut s, &mut b)
            .unwrap();
        let hit = r
            .create_texture_from_file(Some(b"COLOR.TGA"), None, 2, &mut s, &mut b)
            .unwrap();
        assert_eq!(a, hit);
        assert_eq!(s.reads.len(), 1);
        assert_eq!(r.texture(a).unwrap().references, 2);
        let t = r.texture(a).unwrap();
        assert_eq!(t.uid, 1);
        assert_eq!(t.flags, 2);
        assert_eq!(t.creation_flags, 2);
        assert_eq!(
            b.surface(t.graphics.unwrap()).unwrap().bgra,
            [3, 2, 1, 17, 6, 5, 4, 34, 9, 8, 7, 51, 12, 11, 10, 68]
        );
        let plain = r
            .create_texture_from_file(Some(b"color.tga"), None, 0, &mut s, &mut b)
            .unwrap();
        assert_ne!(a, plain);
        assert_eq!(r.texture(plain).unwrap().uid, 2);
        assert_eq!(
            b.surface(r.texture(plain).unwrap().graphics.unwrap())
                .unwrap()
                .bgra[3],
            255
        );
        r.release_texture(a, &mut b).unwrap();
        assert_eq!(b.live_graphics(), 2);
        assert_eq!(r.cached().len(), 2);
        r.release_texture(hit, &mut b).unwrap();
        assert_eq!(b.live_graphics(), 1);
        assert_eq!(r.cached(), &[plain]);
        r.release_texture(plain, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
        assert!(r.cached().is_empty());
    }
    #[test]
    fn source_bpp_changes_effective_alpha_but_retains_request_identity() {
        let (mut r, mut b, mut s) = setup(24, &[3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]);
        let a = r
            .create_texture_from_file(Some(b"color.tga"), None, 2, &mut s, &mut b)
            .unwrap();
        assert_eq!(r.texture(a).unwrap().flags, 0);
        assert_eq!(r.texture(a).unwrap().creation_flags, 2);
        assert!(b
            .surface(r.texture(a).unwrap().graphics.unwrap())
            .unwrap()
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 255));
        r.release_texture(a, &mut b).unwrap();
        s.files.insert(
            b"Maps\\color.tga".to_vec(),
            tga(16, &[0, 0, 0, 128, 255, 255, 31, 0]),
        );
        let a = r
            .create_texture_from_file(Some(b"color.tga"), None, 2, &mut s, &mut b)
            .unwrap();
        assert_eq!(r.texture(a).unwrap().flags, 10);
        assert_eq!(
            b.surface(r.texture(a).unwrap().graphics.unwrap())
                .unwrap()
                .bgra,
            [0, 0, 0, 0, 0, 0, 0, 255, 248, 248, 248, 255, 248, 0, 0, 0]
        );
        r.release_texture(a, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
    }
    #[test]
    fn alpha_brightness_cache_identity_and_missing_source_cleanup() {
        let (mut r, mut b, mut s) =
            setup(32, &[3, 2, 1, 17, 6, 5, 4, 34, 9, 8, 7, 51, 12, 11, 10, 68]);
        s.files.insert(
            b"Maps\\alpha.tga".to_vec(),
            tga(24, &[20, 40, 80, 0, 0, 0, 255, 255, 255, 10, 20, 30]),
        );
        let a = r
            .create_texture_from_file(Some(b"color.tga"), Some(b"alpha.tga"), 0, &mut s, &mut b)
            .unwrap();
        let t = r.texture(a).unwrap();
        assert_eq!(t.flags, 4);
        assert_eq!(t.creation_flags, 4);
        assert_eq!(
            b.surface(t.graphics.unwrap())
                .unwrap()
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[3])
                .collect::<Vec<_>>(),
            [50, 0, 255, 22]
        );
        assert_eq!(
            r.create_texture_from_file(Some(b"COLOR.TGA"), Some(b"ALPHA.TGA"), 0, &mut s, &mut b)
                .unwrap(),
            a
        );
        r.release_texture(a, &mut b).unwrap();
        r.release_texture(a, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
        assert_eq!(
            r.create_texture_from_file(Some(b"missing.tga"), None, 2, &mut s, &mut b),
            Err(TextureFactoryError::Image(TextureImageError::Missing))
        );
        assert_eq!(b.resident_bytes(), 0);
        assert!(r.cached().is_empty());
        assert!(!r.is_poisoned());
        assert_eq!(
            r.create_texture_from_file(Some(b"color.tga"), Some(b"missing.tga"), 2, &mut s, &mut b),
            Err(TextureFactoryError::AlphaImage(TextureImageError::Missing))
        );
        assert_eq!(b.resident_bytes(), 0);
        assert!(!r.is_poisoned());
        let a = r
            .create_texture_from_file(Some(b"color.tga"), None, 2, &mut s, &mut b)
            .unwrap();
        assert_eq!(r.texture(a).unwrap().uid, 4);
        r.release_texture(a, &mut b).unwrap();
    }
    #[test]
    fn uncached_owns_distinct_allocations_and_keep_pixels_is_released() {
        let (mut r, mut b, mut s) = setup(24, &[3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]);
        let a = r
            .create_texture_from_file(Some(b"color.tga"), None, 0x201002, &mut s, &mut b)
            .unwrap();
        let c = r
            .create_texture_from_file(Some(b"color.tga"), None, 0x201002, &mut s, &mut b)
            .unwrap();
        assert_ne!(a, c);
        assert_eq!(r.uncached().len(), 2);
        assert_eq!(b.live_pixels(), 2);
        r.release_texture(a, &mut b).unwrap();
        assert_eq!(b.live_pixels(), 1);
        r.release_texture(c, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
    }
    #[test]
    fn failed_pixel_load_preserves_create_prefix_until_actual_release() {
        let (mut r, mut b, mut s) = setup(24, &[]);
        let key = r.create_igraph_texture().unwrap();
        let error = r
            .create_immediate_file(key, b"Maps\\color.tga", None, 2, &mut s, &mut b)
            .unwrap_err();
        assert_eq!(
            error,
            TextureFactoryError::Image(TextureImageError::Corrupt)
        );
        let t = r.texture(key).unwrap();
        assert_eq!(t.width, Observed::Value(2));
        assert_eq!(t.height, Observed::Value(2));
        assert_eq!(t.flags, 0);
        assert_eq!(t.creation_flags, 2);
        assert!(!t.initialized);
        let Observed::Value(Some(view)) = t.name else {
            panic!("source name prefix absent")
        };
        assert_eq!(r.strings().view(view).unwrap(), b"color.tga");
        assert_eq!(b.live_graphics(), 0);
        assert!(b.live_strings() > 0);
        r.release_texture(key, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
    }
    #[test]
    fn actual_immediate_world_unload_enters_and_leaves_critical_section() {
        let (mut r, mut b, mut s) = setup(24, &[3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]);
        let key = r
            .create_texture_from_file(Some(b"color.tga"), None, 2, &mut s, &mut b)
            .unwrap();
        assert!(!r.texture_manager().is_seeded());
        r.unload_cpu_immediate_textures(&mut b).unwrap();
        assert!(b.texture_section.is_none());
        assert_eq!(b.live_graphics(), 1);
        assert_eq!(r.texture(key).unwrap().references, 1);
        r.release_texture(key, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
    }
    #[test]
    fn fixed_caps_admit_boundary_and_reject_cap_resize_before_pixel_load() {
        let (mut r, mut b, mut s) = setup(24, &[]);
        let mut bytes = tga(24, &vec![31; 1024 * 3]);
        bytes[12..14].copy_from_slice(&1024u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&1u16.to_le_bytes());
        s.files.insert(b"Maps\\color.tga".to_vec(), bytes.clone());
        let key = r
            .create_texture_from_file(Some(b"color.tga"), None, 2, &mut s, &mut b)
            .unwrap();
        assert_eq!(r.texture(key).unwrap().width, Observed::Value(1024));
        r.release_texture(key, &mut b).unwrap();
        bytes[12..14].copy_from_slice(&2048u16.to_le_bytes());
        bytes.truncate(18);
        s.files.insert(b"Maps\\color.tga".to_vec(), bytes);
        for flags in [2, 2 | 0x400000] {
            let error = r
                .create_texture_from_file(Some(b"color.tga"), None, flags, &mut s, &mut b)
                .unwrap_err();
            assert_eq!(
                error,
                TextureFactoryError::Unsupported("CPU texture dimensions exceed fixed caps")
            );
            assert!(!error.is_native_creation_failure());
            assert_eq!(b.resident_bytes(), 0);
        }
    }
}
