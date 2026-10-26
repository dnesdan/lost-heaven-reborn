//! Bounded immediate filename factory (PC 6c230/6c2c0/75f50).
//! CPU adapter selects D3DFMT_A8R8G8B8 for all three uncompressed format globals,
//! max width/height 1024, and hardware caps zero (power-of-two minimum 1, no DXT).
//! These are the controlled native comparison's globals, not detected GPU caps.
//! Admission also excludes dimensions above 1024 with ANYSIZE; that exclusion is
//! host policy rather than an original Create error.
//! It executes channel/alpha conversion into owned BGRA surface storage. Device
//! upload, presaved files, primary-image resizing, mipmaps, compression and managed Create are
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
/// LoadImage RVA0x79706 nearest-pixel resize, before Create's alpha extraction.
/// Decoding each source pixel before selecting it preserves palette/channel
/// expansion while the asset decoder accounts for the original source pitch.
/// LoadImage samples bottom-up rows; this result uses the decoder's top-down
/// representation. Vertical reversal must surround sampling, not precede it.
/// Scale spills and FMUL use PC24 round-to-nearest; the source index truncates.
pub(crate) fn resize_loaded_texture(
    image: lhr_assets::Texture,
    width: u32,
    height: u32,
) -> Result<lhr_assets::Texture, TextureFactoryError> {
    let invalid = || TextureFactoryError::Unsupported("image resize dimensions");
    let source_len = image
        .width
        .checked_mul(image.height)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(invalid)?;
    let target_len = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(invalid)?;
    if image.width == 0
        || image.height == 0
        || width == 0
        || height == 0
        || image.rgba.len() != source_len as usize
    {
        return Err(invalid());
    }
    if (image.width, image.height) == (width, height) {
        return Ok(image);
    }
    let sx = crate::native_precision::pc24_integer_ratio(
        i64::from(image.width),
        i32::try_from(width).map_err(|_| invalid())?,
    );
    let sy = crate::native_precision::pc24_integer_ratio(
        i64::from(image.height),
        i32::try_from(height).map_err(|_| invalid())?,
    );
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(target_len as usize)
        .map_err(|_| TextureError::HostAllocation)?;
    for y in 0..height {
        let source_bottom_y = ((height - 1 - y) as f32 * sy).trunc() as u32;
        if source_bottom_y >= image.height {
            return Err(TextureFactoryError::Unsupported(
                "image resize sampling coordinate",
            ));
        }
        let source_y = image.height - 1 - source_bottom_y;
        for x in 0..width {
            let source_x = (x as f32 * sx).trunc() as u32;
            if source_x >= image.width || source_y >= image.height {
                return Err(TextureFactoryError::Unsupported(
                    "image resize sampling coordinate",
                ));
            }
            let offset = ((source_y * image.width + source_x) * 4) as usize;
            rgba.extend_from_slice(&image.rgba[offset..offset + 4]);
        }
    }
    Ok(lhr_assets::Texture {
        width,
        height,
        rgba,
    })
}
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
                let mut a = decode(
                    &source
                        .read_image(alpha)
                        .map_err(TextureFactoryError::AlphaImage)?,
                )
                .map_err(TextureFactoryError::AlphaImage)?;
                if !matches!(a.bpp, 8 | 24) {
                    return Err(TextureFactoryError::Unsupported("alpha source depth"));
                }
                if (a.texture.width, a.texture.height)
                    != (image.texture.width, image.texture.height)
                {
                    // Create passes the primary SOURCE dimensions to LoadImage.
                    // Its resize reads the padded source pitch and writes tight
                    // rows before brightness extraction (unlike the bypass).
                    a.texture = resize_loaded_texture(
                        a.texture,
                        image.texture.width,
                        image.texture.height,
                    )?;
                    a.source_pitch = a.texture.width * u32::from(a.bpp) / 8;
                } else if a.source_pitch != a.texture.width * u32::from(a.bpp) / 8 {
                    return Err(TextureFactoryError::Unsupported("alpha source row padding"));
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
    fn source_bmp(depth: u16, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; 54];
        bytes[..2].copy_from_slice(b"BM");
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&2u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&2u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&depth.to_le_bytes());
        bytes.extend(pixels);
        let size = bytes.len() as u32;
        bytes[2..6].copy_from_slice(&size.to_le_bytes());
        bytes
    }
    #[test]
    fn original_bmp_sources_feed_native_alpha_flags_color_key_and_owned_pixels() {
        let (mut resources, mut storage, mut source) = setup(24, &[]);
        source.files.insert(
            b"Maps\\color.bmp".to_vec(),
            source_bmp(16, &[0, 0, 0, 128, 255, 255, 31, 0]),
        );
        for (flags, expected_alpha) in [
            (2u32, [255, 0, 0, 255]),
            (0, [255; 4]),
            (8, [255, 255, 0, 255]),
        ] {
            let key = resources
                .create_texture_from_file(
                    Some(b"color.bmp"),
                    None,
                    flags | 0x1000,
                    &mut source,
                    &mut storage,
                )
                .unwrap();
            let texture = resources.texture(key).unwrap();
            assert_eq!(texture.creation_flags, flags | 0x1000);
            assert_eq!(
                texture.flags,
                (flags | 0x1000) | if flags & 2 != 0 { 8 } else { 0 }
            );
            let surface = storage.surface(texture.graphics.unwrap()).unwrap();
            assert_eq!(
                surface
                    .bgra
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[3])
                    .collect::<Vec<_>>(),
                expected_alpha
            );
            assert_eq!(&surface.bgra[..3], &[248, 248, 248]);
            assert_eq!(&surface.bgra[4..7], &[248, 0, 0]);
            let pixels = storage.pixel_bytes(texture.pixels.unwrap()).unwrap();
            assert_eq!(&pixels[..4], &[expected_alpha[0], 248, 248, 248]);
            resources.release_texture(key, &mut storage).unwrap();
            assert_eq!(storage.resident_bytes(), 0);
        }
        source.files.insert(
            b"Maps\\color.bmp".to_vec(),
            source_bmp(
                32,
                &[1, 2, 3, 0, 4, 5, 6, 17, 7, 8, 9, 128, 10, 11, 12, 255],
            ),
        );
        let key = resources
            .create_texture_from_file(Some(b"color.bmp"), None, 2, &mut source, &mut storage)
            .unwrap();
        let texture = resources.texture(key).unwrap();
        assert_eq!(texture.flags, 2);
        assert_eq!(
            storage.surface(texture.graphics.unwrap()).unwrap().bgra,
            [7, 8, 9, 128, 10, 11, 12, 255, 1, 2, 3, 0, 4, 5, 6, 17]
        );
        resources.release_texture(key, &mut storage).unwrap();
        assert_eq!(storage.resident_bytes(), 0);
    }
    fn source_indexed_tga() -> Vec<u8> {
        let mut bytes = vec![0u8; 18];
        bytes[1] = 1;
        bytes[2] = 1;
        bytes[5..7].copy_from_slice(&256u16.to_le_bytes());
        bytes[7] = 24;
        bytes[12] = 2;
        bytes[14] = 2;
        bytes[16] = 8;
        bytes[17] = 0x20;
        bytes.extend([0; 256 * 3]);
        bytes[18..24].copy_from_slice(&[11, 22, 33, 11, 22, 33]);
        bytes[18 + 255 * 3..18 + 256 * 3].copy_from_slice(&[44, 55, 66]);
        bytes.extend([0, 1, 255, 0]);
        bytes
    }
    #[test]
    fn indexed_tga_source_uses_palette_color_key_effective_flags_and_alpha_brightness() {
        let (mut resources, mut storage, mut source) = setup(32, &[255; 16]);
        source
            .files
            .insert(b"Maps\\indexed.tga".to_vec(), source_indexed_tga());
        for (flags, expected_alpha) in [(2u32, [255; 4]), (8, [0, 0, 255, 0])] {
            let key = resources
                .create_texture_from_file(
                    Some(b"indexed.tga"),
                    None,
                    flags,
                    &mut source,
                    &mut storage,
                )
                .unwrap();
            let texture = resources.texture(key).unwrap();
            assert_eq!(texture.flags, flags & !2);
            assert_eq!(texture.creation_flags, flags);
            assert_eq!(texture.color_key, Observed::Value(0x21160b));
            let surface = storage.surface(texture.graphics.unwrap()).unwrap();
            assert_eq!(
                surface
                    .bgra
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[3])
                    .collect::<Vec<_>>(),
                expected_alpha
            );
            assert_eq!(&surface.bgra[..3], &[11, 22, 33]);
            resources.release_texture(key, &mut storage).unwrap();
            assert_eq!(storage.resident_bytes(), 0);
        }
        let key = resources
            .create_texture_from_file(
                Some(b"color.tga"),
                Some(b"indexed.tga"),
                4,
                &mut source,
                &mut storage,
            )
            .unwrap();
        let surface = storage
            .surface(resources.texture(key).unwrap().graphics.unwrap())
            .unwrap();
        let dark = (33 * 77 + 22 * 154 + 11 * 25) >> 8;
        let light = (66 * 77 + 55 * 154 + 44 * 25) >> 8;
        assert_eq!(
            surface
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[3] as u32)
                .collect::<Vec<_>>(),
            [dark, dark, light, dark]
        );
        resources.release_texture(key, &mut storage).unwrap();
        assert_eq!(storage.resident_bytes(), 0);
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
    fn padded_alpha_bmp(height: u32) -> Vec<u8> {
        let offset = 54 + 256 * 4;
        let mut bytes = vec![0u8; offset + height as usize * 4];
        let size = bytes.len() as u32;
        bytes[..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&size.to_le_bytes());
        bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&2u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&height.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&8u16.to_le_bytes());
        for index in 0..256 {
            bytes[54 + index * 4..58 + index * 4].copy_from_slice(&[
                index as u8 ^ 0x55,
                255 - index as u8,
                index as u8,
                0,
            ]);
        }
        for y in 0..height as usize {
            let row = offset + (height as usize - 1 - y) * 4;
            bytes[row..row + 4].copy_from_slice(&[(y * 2) as u8, (y * 2 + 1) as u8, 0xfd, 0xfe]);
        }
        bytes
    }
    #[test]
    fn unequal_padded_alpha_uses_native_bottom_up_nearest_before_palette_brightness() {
        let (mut resources, mut storage, mut source) = setup(24, &[]);
        let mut color = tga(24, &[3, 2, 1].repeat(256));
        color[12..14].copy_from_slice(&16u16.to_le_bytes());
        color[14..16].copy_from_slice(&16u16.to_le_bytes());
        source.files.insert(b"Maps\\color.tga".to_vec(), color);
        source
            .files
            .insert(b"Maps\\alpha.bmp".to_vec(), padded_alpha_bmp(64));
        let key = resources
            .create_texture_from_file(
                Some(b"color.tga"),
                Some(b"alpha.bmp"),
                4,
                &mut source,
                &mut storage,
            )
            .unwrap();
        let surface = storage
            .surface(resources.texture(key).unwrap().graphics.unwrap())
            .unwrap();
        assert_eq!((surface.width, surface.height), (16, 16));
        for y in 0..16 {
            for x in 0..16 {
                // BMP stays bottom-up during native resize: logical rows3,7,..63.
                let index = ((3 + y * 4) * 2 + x / 8) as u32;
                let alpha = ((index * 77 + (255 - index) * 154 + (index ^ 0x55) * 25) >> 8) as u8;
                assert_eq!(
                    &surface.bgra[(y * 16 + x) * 4..(y * 16 + x + 1) * 4],
                    &[3, 2, 1, alpha]
                );
            }
        }
        let hit = resources
            .create_texture_from_file(
                Some(b"COLOR.TGA"),
                Some(b"ALPHA.BMP"),
                4,
                &mut source,
                &mut storage,
            )
            .unwrap();
        assert_eq!(key, hit);
        assert_eq!(source.reads.len(), 2);
        resources.release_texture(key, &mut storage).unwrap();
        assert_eq!(storage.live_graphics(), 1);
        resources.release_texture(hit, &mut storage).unwrap();
        assert_eq!(storage.resident_bytes(), 0);
    }
    #[test]
    fn same_size_padded_alpha_retains_explicit_extraction_boundary_and_failure_cleanup() {
        let (mut resources, mut storage, mut source) = setup(24, &[3, 2, 1].repeat(4));
        source
            .files
            .insert(b"Maps\\alpha.bmp".to_vec(), padded_alpha_bmp(2));
        let error = resources
            .create_texture_from_file(
                Some(b"color.tga"),
                Some(b"alpha.bmp"),
                4,
                &mut source,
                &mut storage,
            )
            .unwrap_err();
        assert_eq!(
            error,
            TextureFactoryError::Unsupported("alpha source row padding")
        );
        assert!(!error.is_native_creation_failure());
        assert_eq!(storage.resident_bytes(), 0);
        assert!(resources.cached().is_empty());
    }
    #[test]
    fn load_image_nearest_upsampling_keeps_raw_row_origin_and_channels() {
        let image = lhr_assets::Texture {
            width: 2,
            height: 2,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        };
        let image = resize_loaded_texture(image, 3, 3).unwrap();
        assert_eq!(
            image.rgba,
            [
                1, 2, 3, 4, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 9, 10, 11, 12, 13, 14, 15, 16,
                9, 10, 11, 12, 9, 10, 11, 12, 13, 14, 15, 16,
            ]
        );
    }
}
