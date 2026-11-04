//! Bounded immediate filename factory (PC 6c230/6c2c0/75f50).
//! CPU adapter selects D3DFMT_A8R8G8B8 for all three uncompressed format globals,
//! max width/height 1024, and hardware caps zero (power-of-two minimum 1, no DXT).
//! These are the controlled native comparison's globals, not detected GPU caps.
//! Admission also excludes dimensions above 1024 with ANYSIZE; that exclusion is
//! host policy rather than an original Create error.
//! It executes channel/alpha conversion into owned BGRA surface storage. Device
//! upload, presaved files, primary-image resizing, mipmaps, compression and managed Create are
//! outside this adapter and reject explicitly, rather than pretending to execute.
use super::texture_surface::{
    DeviceTextureDesc, PresavedAttempt, TextureCreationSink, UploadedCreateLevel,
};
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
    /// Original I3DERR_BADTEXTUREFORMAT (0x80000003): a loaded image whose
    /// depth the conversion switch rejects (ITexture.cpp:655-662), or an
    /// alpha image of unsupported depth (ITexture.cpp:403-411).
    BadTextureFormat,
    /// Original I3DERR_GENERIC (0x80000005): mip-scratch allocation and
    /// device-level failures inside the level loop (ITexture.cpp:696-810).
    /// Host capability failures stay Backend/Unsupported, never this word.
    GenericFailure,
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
                | Self::BadTextureFormat
                | Self::GenericFailure
        )
    }

    /// Native LS3D_RESULT word for each failure (ls3df.h:21-42, u32
    /// preserved). Host-policy rejections (Unsupported/Backend/State) keep
    /// the FAILURE word (u32::MAX, matching rw_data::FAILURE); they never
    /// forge a native I3DERR_* word. Image(UnknownFormat) maps to
    /// FILECORRUPTED via the LoadImage-false analog (ITexture.cpp:328-330);
    /// GetImageInfo's exact negative word for unknown extensions is
    /// unresolved (see OPEN).
    pub fn native_result_word(&self) -> u32 {
        match self {
            Self::InvalidParameters => 0x80000007,
            Self::BadTextureFormat => 0x80000003,
            Self::GenericFailure => 0x80000005,
            Self::Image(TextureImageError::Missing) => 0x80000001,
            Self::AlphaImage(TextureImageError::Missing) => 0x8000000b,
            Self::Image(TextureImageError::Corrupt)
            | Self::Image(TextureImageError::UnknownFormat)
            | Self::AlphaImage(TextureImageError::Corrupt)
            | Self::AlphaImage(TextureImageError::UnknownFormat) => 0x8000000a,
            Self::State(e) if *e == TextureError::HostAllocation => 0x80000002,
            // Host-only rejections (dims caps, stride guards, capability
            // absence) never forge a native word; they keep FAILURE.
            Self::Image(_)
            | Self::AlphaImage(_)
            | Self::State(_)
            | Self::Unsupported(_)
            | Self::Backend(_) => u32::MAX,
        }
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
    diagnostics: Vec<Vec<u8>>,
    device_extra: BTreeMap<u64, DeviceTextureExtra>,
    device_available: bool,
    device_log: Vec<DeviceEvent>,
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
            diagnostics: Vec::new(),
            device_extra: BTreeMap::new(),
            device_available: true,
            device_log: Vec::new(),
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
    pub fn diagnostics(&self) -> &[Vec<u8>] {
        &self.diagnostics
    }
    pub fn device_levels(&self, graphics: TextureGraphics) -> Option<&[StoredDeviceLevel]> {
        self.device_extra
            .get(&graphics.0)
            .map(|e| e.levels.as_slice())
    }
    pub fn device_desc(&self, graphics: TextureGraphics) -> Option<&DeviceTextureDesc> {
        self.device_extra.get(&graphics.0).map(|e| &e.desc)
    }
    pub fn device_log(&self) -> &[DeviceEvent] {
        &self.device_log
    }
    pub fn live_device_textures(&self) -> usize {
        self.device_extra.len()
    }
    /// Host driver-presence model: false makes creation_sink() return None so
    /// the port rejects before effects. Defaults to true. Test-only: no
    /// production backend toggles presence.
    #[cfg(test)]
    pub(crate) fn set_device_available(&mut self, available: bool) {
        self.device_available = available;
    }
    pub fn resident_bytes(&self) -> usize {
        self.graphics.values().map(|s| s.bgra.len()).sum::<usize>()
            + self.pixels.values().map(Vec::len).sum::<usize>()
            + self.strings.values().map(Vec::len).sum::<usize>()
            + self
                .device_extra
                .values()
                .map(|e| e.levels.iter().map(|l| l.bytes.len()).sum::<usize>())
                .sum::<usize>()
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
    fn log_diagnostic(&mut self, message: crate::itexture::TextureDiagnostic) {
        self.diagnostics
            .push(crate::itexture::render_diagnostic(&message));
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
        self.device_extra.remove(&k.0);
        self.owners.remove(&k.0);
        Ok(())
    }
    fn creation_sink(&mut self) -> Option<&mut dyn TextureCreationSink<Error = &'static str>> {
        if self.device_available {
            Some(self)
        } else {
            None
        }
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
impl TextureCreationSink for CpuTextureStorage {
    type Error = &'static str;
    fn create_device_texture(
        &mut self,
        owner: TextureKey,
        desc: &DeviceTextureDesc,
    ) -> Result<(TextureGraphics, u16), Self::Error> {
        if desc.width == 0 || desc.height == 0 {
            return Err("device texture dimensions");
        }
        // Full D3D chain for requested 0, clamped otherwise (native clamps).
        let mut full = 1u32;
        let (mut w, mut h) = (u32::from(desc.width), u32::from(desc.height));
        while w > 1 || h > 1 {
            w = (w >> 1).max(1);
            h = (h >> 1).max(1);
            full += 1;
        }
        let actual = if !desc.mipmaps {
            1
        } else if desc.requested_levels == 0 {
            full
        } else {
            u32::from(desc.requested_levels).min(full)
        };
        let actual = u16::try_from(actual).map_err(|_| "device level count")?;
        if actual == 0 {
            return Err("device level count");
        }
        let id = self.id()?;
        self.graphics.insert(
            id,
            CpuTextureSurface {
                width: desc.width,
                height: desc.height,
                format: desc.format,
                bgra: Vec::new(),
            },
        );
        self.device_extra.insert(
            id,
            DeviceTextureExtra {
                desc: DeviceTextureDesc { ..*desc },
                levels: Vec::new(),
            },
        );
        self.owners.insert(id, owner);
        self.device_log.push(DeviceEvent::Created {
            owner,
            width: desc.width,
            height: desc.height,
            levels: actual,
            format: desc.format,
        });
        Ok((TextureGraphics(id), actual))
    }
    fn upload_create_level(
        &mut self,
        graphics: TextureGraphics,
        level: UploadedCreateLevel<'_>,
    ) -> Result<(), Self::Error> {
        let extra = self
            .device_extra
            .get(&graphics.0)
            .ok_or("unknown device texture")?;
        if level.format != D3DFMT_A8R8G8B8 {
            // Packed-16-bit and DXT-block packing live behind a real GPU/S3TC
            // backend (see OPEN); the CPU stand-in uploads 32-bit exactly.
            return Err("device pixel format");
        }
        if level.format != extra.desc.format {
            return Err("level format mismatch");
        }
        let expected = create_level_byte_size(level.format, level.width, level.height)
            .map_err(|_| "level extent")?;
        if level.bytes.len() as u64 != expected as u64 {
            return Err("level extent");
        }
        let (mut w, mut h) = (extra.desc.width, extra.desc.height);
        for _ in 0..level.level {
            (w, h) = mipmap_level_dims(w, h);
        }
        if (w, h) != (level.width, level.height) {
            return Err("level dimensions");
        }
        if level.level == 0 {
            self.graphics
                .get_mut(&graphics.0)
                .ok_or("unknown device texture")?
                .bgra = level.bytes.to_vec();
        } else {
            self.device_extra
                .get_mut(&graphics.0)
                .ok_or("unknown device texture")?
                .levels
                .push(StoredDeviceLevel {
                    width: level.width,
                    height: level.height,
                    bytes: level.bytes.to_vec(),
                });
        }
        self.device_log.push(DeviceEvent::Uploaded {
            graphics,
            level: level.level,
            width: level.width,
            height: level.height,
            format: level.format,
        });
        Ok(())
    }
    fn compress_create_level(
        &mut self,
        graphics: TextureGraphics,
        level: u16,
        dxt: u8,
    ) -> Result<(), Self::Error> {
        if !self.device_extra.contains_key(&graphics.0) {
            return Err("unknown device texture");
        }
        // S3TC blocks are not fabricated (see OPEN); the decision is recorded.
        self.device_log.push(DeviceEvent::Compressed {
            graphics,
            level,
            dxt,
        });
        Ok(())
    }
    fn keep_create_pixels(
        &mut self,
        owner: TextureKey,
        argb: Vec<u8>,
    ) -> Result<PixelAllocation, Self::Error> {
        let id = self.id()?;
        let bytes = argb.len();
        self.pixels.insert(id, argb);
        self.owners.insert(id, owner);
        self.device_log
            .push(DeviceEvent::PixelsKept { owner, bytes });
        Ok(PixelAllocation(id))
    }
    fn try_presaved_dxt(&mut self, attempt: &PresavedAttempt) -> Result<bool, Self::Error> {
        self.device_log.push(DeviceEvent::PresavedAttempt {
            filename: attempt.filename.clone(),
            dxt: attempt.dxt,
        });
        Ok(false)
    }
    fn try_presaved_555_565(&mut self, attempt: &PresavedAttempt) -> Result<bool, Self::Error> {
        self.device_log.push(DeviceEvent::PresavedAttempt {
            filename: attempt.filename.clone(),
            dxt: 0,
        });
        Ok(false)
    }
    fn store_presaved_dxt(
        &mut self,
        graphics: TextureGraphics,
        attempt: &PresavedAttempt,
    ) -> Result<(), Self::Error> {
        if !self.device_extra.contains_key(&graphics.0) {
            return Err("unknown device texture");
        }
        self.device_log.push(DeviceEvent::PresavedStored {
            filename: attempt.filename.clone(),
            dxt: attempt.dxt,
        });
        Ok(())
    }
    fn store_presaved_555_565(
        &mut self,
        graphics: TextureGraphics,
        attempt: &PresavedAttempt,
    ) -> Result<(), Self::Error> {
        if !self.device_extra.contains_key(&graphics.0) {
            return Err("unknown device texture");
        }
        self.device_log.push(DeviceEvent::PresavedStored {
            filename: attempt.filename.clone(),
            dxt: 0,
        });
        Ok(())
    }
    fn preload_create_texture(&mut self, graphics: TextureGraphics) -> Result<(), Self::Error> {
        if !self.device_extra.contains_key(&graphics.0) {
            return Err("unknown device texture");
        }
        self.device_log.push(DeviceEvent::Preloaded { graphics });
        Ok(())
    }
    fn fill_empty_texture(
        &mut self,
        graphics: TextureGraphics,
        color: u32,
    ) -> Result<(), Self::Error> {
        let surface = self
            .graphics
            .get_mut(&graphics.0)
            .ok_or("unknown device texture")?;
        if surface.format != D3DFMT_A8R8G8B8 {
            return Err("device pixel format");
        }
        // Native Fill writes the X8R8G8B8 word per pixel over the whole level.
        surface.bgra = color
            .to_le_bytes()
            .repeat(surface.width as usize * surface.height as usize);
        self.device_log
            .push(DeviceEvent::Filled { graphics, color });
        Ok(())
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
            if let Some(hit) = self.find_cached_create_texture(name, alpha, flags)? {
                self.retain_texture(hit)?;
                return Ok(hit);
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

// ---- ITexture::Create (RVA 0x75f50) full port; immediates byte-verified at
// the VAs in the owning task record (TEST AL,0x40 @0x10075fa0 through TEST
// [EBP+0x30],0x9001 @0x10076979, AH byte tests for FILL/KEEP/HALF).

/// Original ITC_* creation flag words (upstream working names,
/// IGraph.h:632-650). ITC_HALF lives in itexture (presaved lane) and is
/// reused here; the rest are Create-ladder words.
pub const ITC_FROM_MEMORY: u32 = 0x1;
pub const ITC_ALPHA: u32 = 0x2;
pub const ITC_USE_ALPHA: u32 = 0x4;
pub const ITC_COLORKEY: u32 = 0x8;
pub const ITC_MANAGED: u32 = 0x40;
pub const ITC_DITHER: u32 = 0x80;
pub const ITC_MIPMAP: u32 = 0x100;
pub const ITC_DXT: u32 = 0x200;
pub const ITC_TRUECOLOR: u32 = 0x400;
pub const ITC_KEEP_PIXELS: u32 = 0x1000;
pub const ITC_FILL: u32 = 0x4000;
pub const ITC_EMPTY: u32 = 0x8000;
pub const ITC_NOALIGN: u32 = 0x40000;
pub const ITC_MANAGED_DXT: u32 = 0x80000;
pub const ITC_UNCACHED: u32 = 0x200000;
pub const ITC_ANYSIZE: u32 = 0x400000;
pub const ITC_QUEUED: u32 = 0x40000000;
pub const ITC_LOADED: u32 = 0x80000000;
/// Flag bits this port executes (the sibling file-only path admits
/// 2|4|8|0x1000|0x200000|0x400000; the full port adds memory/empty/mipmap/
/// dither/DXT-decision/half/fill/noalign branches).
const CREATE_EXECUTED_FLAGS: u32 = 0x1
    | 0x2
    | 0x4
    | 0x8
    | 0x80
    | 0x100
    | 0x200
    | 0x400
    | 0x1000
    | 0x2000
    | 0x4000
    | 0x8000
    | 0x40000
    | 0x200000
    | 0x400000;
/// Managed-domain bits divert to the texture manager (RVA 0x7ad20), never here.
const CREATE_MANAGED_FLAGS: u32 = 0x40 | 0x80000 | 0x40000000 | 0x80000000;

/// D3D unknowns/absents (the rest live in itexture and are reused here).
pub const D3DFMT_UNKNOWN: u32 = 0;

/// Device format globals for the ladder (native m_*TexturePixelFormat).
/// Zero means the card lacks the format and the ladder falls through
/// (ITexture.cpp:430-463).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateFormatCaps {
    pub truecolor: u32,
    pub alpha: u32,
    pub alpha1: u32,
    pub texture: u32,
    pub dxt: u32,
    pub dxt_alpha: u32,
}
/// Device limits for TEXTURE_SIZE plus the executed format globals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreateDeviceCaps {
    pub formats: CreateFormatCaps,
    pub max_width: u16,
    pub max_height: u16,
    /// Native HWCAPS_DXT: power-of-two minimum 4 instead of 1.
    pub dxt_size_minimum: bool,
}
impl CreateDeviceCaps {
    /// Bounded CPU comparison context, matching the sibling adapter: 1024
    /// limits, no DXT minimum, every uncompressed global selects
    /// D3DFMT_A8R8G8B8 and both DXT globals are absent.
    pub const CPU: Self = Self {
        formats: CreateFormatCaps {
            truecolor: 21,
            alpha: 21,
            alpha1: 21,
            texture: 21,
            dxt: 0,
            dxt_alpha: 0,
        },
        max_width: CPU_TEXTURE_MAX_WIDTH,
        max_height: CPU_TEXTURE_MAX_HEIGHT,
        dxt_size_minimum: false,
    };
}

/// Ladder output: surface format plus DXT compression via aux surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedCreateFormat {
    pub format: u32,
    pub dxt: u8,
    pub dxt_format: u32,
}
/// Original ladder incl. the TRUECOLOR assignment-in-condition. `flags`
/// already carries the file-branch BPP fixup; memory/empty apply none.
pub(crate) fn select_create_format(
    flags: u32,
    caps: &CreateFormatCaps,
) -> (u32, SelectedCreateFormat) {
    let mut flags = flags;
    let mut format;
    let mut dxt = 0u8;
    let mut dxt_format = D3DFMT_UNKNOWN;
    if flags & ITC_TRUECOLOR == 0 || {
        format = caps.truecolor;
        format
    } == 0
    {
        flags &= !ITC_TRUECOLOR;
        if flags & (ITC_ALPHA | ITC_USE_ALPHA) != 0 && flags & ITC_COLORKEY == 0 {
            if flags & ITC_DXT != 0 && caps.dxt_alpha != 0 {
                format = caps.dxt_alpha;
                dxt = PRESAVED_DXT3;
                dxt_format = caps.alpha;
            } else {
                format = caps.alpha;
            }
        } else if flags & ITC_COLORKEY != 0 {
            if flags & ITC_MIPMAP == 0 {
                if flags & ITC_DXT != 0 && caps.dxt != 0 {
                    format = caps.dxt;
                    dxt = PRESAVED_DXT1_ALPHA;
                    dxt_format = caps.alpha1;
                } else {
                    format = caps.alpha1;
                }
            } else if flags & ITC_DXT != 0 && caps.dxt_alpha != 0 {
                format = caps.dxt_alpha;
                dxt = PRESAVED_DXT3;
                dxt_format = caps.alpha;
            } else {
                format = caps.alpha;
            }
        } else if flags & ITC_DXT != 0 && caps.dxt != 0 {
            format = caps.dxt;
            dxt = PRESAVED_DXT1;
            dxt_format = caps.alpha1;
        } else {
            format = caps.texture;
        }
    }
    (
        flags,
        SelectedCreateFormat {
            format,
            dxt,
            dxt_format,
        },
    )
}

/// File-branch BPP fixup (ITexture.cpp:306-311).
pub(crate) fn apply_file_bpp_flag_fixup(flags: u32, bpp: u16) -> u32 {
    let mut flags = flags;
    if flags & ITC_ALPHA != 0 {
        if bpp == 8 || bpp == 24 {
            flags &= !ITC_ALPHA;
        }
        if bpp == 16 {
            flags |= ITC_COLORKEY;
        }
    }
    flags
}

/// TEXTURE_SIZE macro (ITexture.cpp:104-130). Caller halves first. f32
/// multiply-then-truncate matches `(ushort)(sx*scale)`; minimum 1 (4 with DXT
/// caps); zero yields the minimum. Round-up overflow saturates (the original
/// wraps its u16 counter; the host must not hang -- see OPEN).
pub(crate) fn create_texture_size(
    w: u16,
    h: u16,
    max_w: u16,
    max_h: u16,
    dxt_minimum: bool,
) -> (u16, u16) {
    let mut scale_x = 1.0f32;
    if w as f32 > max_w as f32 {
        scale_x = max_w as f32 / w as f32;
    }
    let mut scale_y = 1.0f32;
    if h as f32 > max_h as f32 {
        scale_y = max_h as f32 / h as f32;
    }
    let scale = scale_x.min(scale_y);
    let (sx, sy) = ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
    let (mut x, mut y) = (
        u32::from(dxt_minimum) * 3 + 1,
        u32::from(dxt_minimum) * 3 + 1,
    );
    while sx > x {
        x = match x.checked_shl(1) {
            Some(x) => x,
            None => return (u16::MAX, u16::MAX),
        };
    }
    while sy > y {
        y = match y.checked_shl(1) {
            Some(y) => y,
            None => return (u16::MAX, u16::MAX),
        };
    }
    (
        u16::try_from(x).unwrap_or(u16::MAX),
        u16::try_from(y).unwrap_or(u16::MAX),
    )
}

/// Axis step (ITexture.cpp:779-784): halve, clamp to 1.
pub(crate) fn mipmap_level_dims(width: u16, height: u16) -> (u16, u16) {
    ((width >> 1).max(1), (height >> 1).max(1))
}

/// Mip filter (ITexture.cpp:754-778): `dst=(c11>>3)+(c00>>3)+(c10*3>>3)+
/// (c01*3>>3)` per channel as u16. Caller passes `parent_w*4` first, then
/// each returned pitch; `dst` is reused across levels without clearing (as
/// the original reuses its scratch) while uploads read the tight head.
/// Even chains bit-exact; odd chains match except never-written tight spots
/// (indeterminate heap bytes -- OPEN). `dst` needs `parent_w*parent_h*4`.
pub(crate) fn filter_mipmap_level(
    src: &[u8],
    dst: &mut [u8],
    parent_w: u32,
    parent_h: u32,
    pitch: u32,
) -> Result<(u16, u16, u32), TextureFactoryError> {
    let (out_w, out_h) = ((parent_w >> 1).max(1), (parent_h >> 1).max(1));
    let need = parent_w
        .checked_mul(parent_h)
        .and_then(|n| n.checked_mul(4))
        .ok_or(TextureError::HostAllocation)?;
    if (dst.len() as u64) < need as u64 || (src.len() as u64) < need as u64 {
        return Err(TextureFactoryError::Unsupported("mipmap filter extent"));
    }
    let half_pitch = pitch >> 1;
    for y in (0..parent_h.saturating_sub(1)).step_by(2) {
        for x in (0..parent_w.saturating_sub(1)).step_by(2) {
            let i00 = (y * pitch + x * 4) as usize;
            let (i01, i10) = (i00 + 4, ((y + 1) * pitch + x * 4) as usize);
            let i11 = i10 + 4;
            let d = ((y >> 1) * half_pitch + (x >> 1) * 4) as usize;
            if i11 + 4 > src.len() || d + 4 > dst.len() {
                return Err(TextureFactoryError::Unsupported("mipmap filter extent"));
            }
            for c in 0..4 {
                let (c00, c01, c10, c11) = (
                    u16::from(src[i00 + c]),
                    u16::from(src[i01 + c]),
                    u16::from(src[i10 + c]),
                    u16::from(src[i11 + c]),
                );
                dst[d + c] = ((c11 >> 3) + (c00 >> 3) + ((c10 * 3) >> 3) + ((c01 * 3) >> 3)) as u8;
            }
        }
    }
    Ok((out_w as u16, out_h as u16, half_pitch))
}

/// Alpha brightness (ITexture.cpp:387/398): `(R*77+G*154+B*25)>>8` in u32.
pub(crate) fn alpha_brightness(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 77 + u32::from(g) * 154 + u32::from(b) * 25) >> 8) as u8
}

/// 8-bit palette quantization per format (ITexture.cpp:503-519; RGBQUAD is
/// B,G,R). Formats outside the original's labels stay indeterminate there;
/// the port rejects them (unreachable under CPU caps -- OPEN).
fn quantize_palette_color(r: u8, g: u8, b: u8, format: u32) -> Result<u32, TextureFactoryError> {
    match format {
        D3DFMT_R5G6B5 => Ok(u32::from(b >> 3) | u32::from(r >> 3) << 11 | u32::from(g >> 2) << 5),
        D3DFMT_A8R8G8B8 => Ok(u32::from(b) | u32::from(r) << 16 | u32::from(g) << 8),
        D3DFMT_A4R4G4B4 | D3DFMT_DXT3 => {
            Ok(u32::from(b >> 4) | u32::from(r >> 4) << 8 | u32::from(g >> 4) << 4)
        }
        D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5 | D3DFMT_DXT1 => {
            Ok(u32::from(b >> 3) | u32::from(r >> 3) << 10 | u32::from(g >> 3) << 5)
        }
        _ => Err(TextureFactoryError::Unsupported("color-key quantization")),
    }
}

pub(crate) struct ConvertedCreateImage {
    pub argb: Vec<u8>,
    pub color_key: u32,
}
/// Raw source rows for conversion: bottom-up rows at `stride` in native
/// file order (BGR[A] bytes; 8-bit indices; 16-bit words). The caller flips
/// top-origin TGA rows beforehand; BMP/memory rows arrive bottom-up.
/// `palette` is raw palette bytes with entry stride 3 (TGA) or 4 (BMP).
pub(crate) struct RawCreateRows<'a> {
    pub rows: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bpp: u16,
    pub palette: Option<(&'a [u8], u32)>,
    pub palette_key: Option<[u8; 3]>,
}
/// ARGB conversion core (ITexture.cpp:499-663) over raw rows. Bottom-up emit
/// writing sequentially, exactly as the original; alpha indexes
/// `[(h-1-i)*w+j]`, reproducing the `opt` walk. Channel positions below are
/// raw-file positions (B=byte0 of 24-bit pixels, R=byte2), matching the
/// original's word arithmetic: 16-bit expansion is bit-identical to its
/// shifts, and the all-zero check reproduces `c==m_ColorKey(0)` exactly.
pub(crate) fn convert_create_pixels(
    raw: &RawCreateRows<'_>,
    alpha_plane: Option<&[u8]>,
    flags: u32,
    format: u32,
) -> Result<ConvertedCreateImage, TextureFactoryError> {
    let (width, height, bpp, stride) = (raw.width, raw.height, raw.bpp, raw.stride);
    if !matches!(bpp, 8 | 16 | 24 | 32) {
        return Err(TextureFactoryError::BadTextureFormat);
    }
    let bytes_per_pixel = u32::from(bpp) / 8;
    let row_len = width
        .checked_mul(bytes_per_pixel)
        .ok_or(TextureError::HostAllocation)?;
    if (stride as u64) < row_len as u64 || (height as u64) * (stride as u64) > raw.rows.len() as u64
    {
        return Err(TextureFactoryError::Unsupported("conversion image extent"));
    }
    if let Some(plane) = alpha_plane {
        // u64 product: the u32 multiply wraps in release on hostile dims.
        if (plane.len() as u64) != u64::from(width) * u64::from(height) {
            return Err(TextureFactoryError::Unsupported("conversion image extent"));
        }
    }
    let color_key = if bpp == 8 {
        let key = raw.palette_key.unwrap_or([0, 0, 0]);
        quantize_palette_color(key[0], key[1], key[2], format)?
    } else {
        0
    };
    let len = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .ok_or(TextureError::HostAllocation)?;
    let mut argb = Vec::new();
    argb.try_reserve_exact(len as usize)
        .map_err(|_| TextureError::HostAllocation)?;
    for i in (0..height).rev() {
        for j in 0..width {
            let bytes_per_pixel = u32::from(bpp) / 8;
            let o = (i * raw.stride + j * bytes_per_pixel) as usize;
            // Raw pixel bytes at the original's positions.
            let (r, g, b, a, mut raw_word);
            raw_word = 0;
            match bpp {
                8 => {
                    let (pal, entry_stride) = raw.palette.ok_or(TextureError::HostAllocation)?;
                    let e = raw.rows[o] as usize * entry_stride as usize;
                    let q = pal
                        .get(e..e + entry_stride as usize)
                        .ok_or(TextureError::HostAllocation)?;
                    // RGBQUAD/BGR triple both store B,G,R in order; entry 0
                    // is the color key in R,G,B.
                    r = q[2];
                    g = q[1];
                    b = q[0];
                    a = 255;
                }
                16 => {
                    let word = u16::from_le_bytes([raw.rows[o], raw.rows[o + 1]]);
                    r = ((word & 0x7c00) >> 7) as u8;
                    g = ((word & 0x03e0) >> 2) as u8;
                    b = ((word & 0x001f) << 3) as u8;
                    a = if word & 0x8000 != 0 { 0xff } else { 0 };
                    raw_word = word;
                }
                24 => {
                    b = raw.rows[o];
                    g = raw.rows[o + 1];
                    r = raw.rows[o + 2];
                    a = 255;
                }
                _ => {
                    b = raw.rows[o];
                    g = raw.rows[o + 1];
                    r = raw.rows[o + 2];
                    a = raw.rows[o + 3];
                }
            }
            let alpha = if matches!(bpp, 16 | 32) && flags & ITC_ALPHA != 0 {
                a
            } else if let Some(plane) = alpha_plane {
                plane[((height - 1 - i) * width + j) as usize]
            } else if flags & ITC_COLORKEY != 0 {
                match bpp {
                    8 => {
                        let color = quantize_palette_color(r, g, b, format)?;
                        if color == color_key {
                            0
                        } else {
                            0xff
                        }
                    }
                    16 => {
                        // Raw word comparison (c == m_ColorKey(0)): 0x8000
                        // alone still counts as opaque.
                        if raw_word == 0 {
                            0
                        } else {
                            0xff
                        }
                    }
                    _ => {
                        if r == 0 {
                            0
                        } else {
                            0xff
                        }
                    }
                }
            } else {
                0xff
            };
            argb.extend_from_slice(&[alpha, r, g, b]);
        }
    }
    Ok(ConvertedCreateImage { argb, color_key })
}
/// In-memory BMP intake (ITexture.cpp:270-291 via LoadImage from_memory,
/// ITexture.cpp:1449-1453: type forced to IMAGE_BMP, buffer is
/// `BITMAPINFOHEADER + palette + rows`). The stride rule (`if (!type)`,
/// IMAGE_BMP=0) overwrites any header stride, so the header field is ignored
/// exactly as the original ignores it. Unsupported depths skip row decoding;
/// they reject later at conversion, after device creation and presaved
/// attempt 1, matching the original's order (deferred BADTEXTUREFORMAT).
pub(crate) struct MemoryCreateImage {
    /// Byte range of the bottom-up raw rows inside the source buffer.
    pub rows: std::ops::Range<usize>,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bpp: u16,
    /// Byte length of the raw palette (entries * 4), if 8-bit.
    pub palette_len: usize,
    pub palette_key: Option<[u8; 3]>,
    pub deferred_bad_format: bool,
}
pub(crate) fn decode_memory_create_image(
    buffer: &[u8],
    flags: u32,
) -> Result<MemoryCreateImage, TextureFactoryError> {
    // Numeric widths from the ABI: i32 dims, u16 depth, u32 compression/count.
    let header = buffer
        .get(..40)
        .ok_or(TextureFactoryError::Image(TextureImageError::Corrupt))?;
    let (width, height) = (
        i32::from_le_bytes(header[4..8].try_into().unwrap()),
        i32::from_le_bytes(header[8..12].try_into().unwrap()),
    );
    let bpp = u16::from_le_bytes([header[14], header[15]]);
    let compression = u32::from_le_bytes(header[16..20].try_into().unwrap());
    let clr_used = u32::from_le_bytes(header[32..36].try_into().unwrap());
    if compression != 0 {
        // Native "Unsupported compresion" => false => FILECORRUPTED.
        return Err(TextureFactoryError::Image(TextureImageError::Corrupt));
    }
    if width <= 0 || height <= 0 {
        // Native size math wraps and reads out of bounds; the host rejects.
        return Err(TextureFactoryError::Image(TextureImageError::Corrupt));
    }
    let (width, height) = (width as u32, height as u32);
    if width > 8192 || height > 8192 || u64::from(width) * u64::from(height) > 16_777_216 {
        return Err(TextureFactoryError::Image(TextureImageError::Unsupported));
    }
    let entries = if clr_used == 0 && bpp <= 8 {
        1u32 << bpp
    } else {
        clr_used
    };
    let pixels_offset = 40u32
        .checked_add(entries.checked_mul(4).ok_or(TextureError::HostAllocation)?)
        .ok_or(TextureError::HostAllocation)?;
    let supported = matches!(bpp, 8 | 16 | 24 | 32);
    let bytes_per_pixel = u32::from(bpp) / 8;
    let tight = width
        .checked_mul(bytes_per_pixel)
        .ok_or(TextureError::HostAllocation)?;
    let stride = if flags & ITC_NOALIGN != 0 || tight % 4 == 0 {
        tight
    } else {
        (tight / 4 + 1) * 4
    };
    // Native reads the full pitch*height rectangle in one dtaRead.
    let extent = u64::from(pixels_offset)
        .checked_add(u64::from(stride) * u64::from(height))
        .ok_or(TextureError::HostAllocation)?;
    if extent > buffer.len() as u64 {
        return Err(TextureFactoryError::Image(TextureImageError::Corrupt));
    }
    if !supported {
        return Ok(MemoryCreateImage {
            rows: 0..0,
            width,
            height,
            stride,
            bpp,
            palette_len: 0,
            palette_key: None,
            deferred_bad_format: true,
        });
    }
    let palette = &buffer[40..pixels_offset as usize];
    if bpp == 8 && palette.len() < 4 {
        return Err(TextureFactoryError::Image(TextureImageError::Corrupt));
    }
    // RGBQUAD entry 0 is B,G,R,reserved; the key stores R,G,B.
    let palette_len = if bpp == 8 { palette.len() } else { 0 };
    let palette_key = if bpp == 8 {
        Some([palette[2], palette[1], palette[0]])
    } else {
        None
    };
    Ok(MemoryCreateImage {
        rows: pixels_offset as usize..(pixels_offset as usize + stride as usize * height as usize),
        width,
        height,
        stride,
        bpp,
        palette_len,
        palette_key,
        deferred_bad_format: false,
    })
}

/// Raw bottom-up rows for a file payload (TGA/BMP): TGA rows are tight at
/// 18 + id_length (flipped when top-origin, desc bit 0x20); BMP rows use the
/// standard padded stride at bfOffBits (validated) and arrive bottom-up.
/// Returns (rows, stride). Palette extraction stays with the caller: TGA
/// color-map spec vs BMP biClrUsed differ. Native reads the same layouts
/// through LoadImage (ITexture.cpp:359-417 for alpha, :328 for primary).
fn extract_file_rows(
    file_bytes: &[u8],
    info: &lhr_assets::texture_source::TextureImageInfo,
    noalign: bool,
) -> Result<(Vec<u8>, u32), TextureFactoryError> {
    let corrupt = || TextureFactoryError::Image(TextureImageError::Corrupt);
    // All extent math is checked: info dims come from the decoder header
    // probe, so hostile values must report Corrupt, never wrap or panic.
    let bytes_per_pixel = u32::from(info.bpp) / 8;
    let tight = (u64::from(info.width) * u64::from(bytes_per_pixel)) as usize;
    if tight as u64 != u64::from(info.width) * u64::from(bytes_per_pixel) {
        return Err(corrupt());
    }
    if file_bytes.starts_with(b"BM") {
        // NOALIGN rule shared with the memory branch and LoadImage
        // (ITexture.cpp:1516-1520).
        let stride = if noalign || tight.is_multiple_of(4) {
            tight
        } else {
            tight
                .checked_add(3)
                .map(|v| v / 4 * 4)
                .ok_or_else(corrupt)?
        };
        let off = u32::from_le_bytes(
            file_bytes
                .get(10..14)
                .ok_or_else(corrupt)?
                .try_into()
                .unwrap(),
        ) as usize;
        let end = off
            .checked_add(
                stride
                    .checked_mul(info.height as usize)
                    .ok_or_else(corrupt)?,
            )
            .ok_or_else(corrupt)?;
        if off < 54 || end > file_bytes.len() {
            return Err(corrupt());
        }
        let stride32 = u32::try_from(stride).map_err(|_| corrupt())?;
        Ok((file_bytes[off..end].to_vec(), stride32))
    } else {
        let id = file_bytes.first().copied().unwrap_or(0) as usize;
        let desc = file_bytes.get(17).copied().unwrap_or(0);
        let start = 18usize.checked_add(id).ok_or_else(corrupt)?;
        let end = start
            .checked_add(
                tight
                    .checked_mul(info.height as usize)
                    .ok_or_else(corrupt)?,
            )
            .ok_or_else(corrupt)?;
        let mut rows = file_bytes.get(start..end).ok_or_else(corrupt)?.to_vec();
        if desc & 0x20 != 0 {
            let pitch = tight;
            let mut tmp = vec![0u8; pitch];
            for pair in 0..info.height as usize / 2 {
                let (lo, hi) = (pair * pitch, (info.height as usize - 1 - pair) * pitch);
                tmp.copy_from_slice(&rows[lo..lo + pitch]);
                rows.copy_within(hi..hi + pitch, lo);
                rows[hi..hi + pitch].copy_from_slice(&tmp);
            }
        }
        let tight32 = u32::try_from(tight).map_err(|_| corrupt())?;
        Ok((rows, tight32))
    }
}

/// Brightness plane over raw rows (alpha sources): `(R*77+G*154+B*25)>>8`.
/// 24-bit reads raw B,G,R positions; 8-bit expands through the raw palette
/// (entry stride carried in `raw`). Other depths have no brightness meaning.
pub(crate) fn brightness_plane_raw(
    raw: &RawCreateRows<'_>,
) -> Result<Vec<u8>, TextureFactoryError> {
    if !matches!(raw.bpp, 8 | 24) {
        return Err(TextureFactoryError::Unsupported("alpha brightness depth"));
    }
    let mut plane = Vec::new();
    plane
        .try_reserve_exact(
            usize::try_from(u64::from(raw.width) * u64::from(raw.height))
                .map_err(|_| TextureFactoryError::Unsupported("alpha brightness extent"))?,
        )
        .map_err(|_| TextureError::HostAllocation)?;
    for y in 0..raw.height {
        for x in 0..raw.width {
            let (r, g, b) = if raw.bpp == 24 {
                let o = (y * raw.stride + x * 3) as usize;
                (raw.rows[o + 2], raw.rows[o + 1], raw.rows[o])
            } else {
                let (pal, entry_stride) = raw.palette.ok_or(TextureError::HostAllocation)?;
                let o = (y * raw.stride + x) as usize;
                let e = raw.rows[o] as usize * entry_stride as usize;
                let q = pal
                    .get(e..e + entry_stride as usize)
                    .ok_or(TextureError::HostAllocation)?;
                (q[2], q[1], q[0])
            };
            plane.push(alpha_brightness(r, g, b));
        }
    }
    Ok(plane)
}

/// Nearest resize of a single-channel plane (alpha LoadImage sx/sy path).
/// Uses the same float scale + truncation as the LoadImage scaler
/// (`width/sx` float, `(x*scale) as usize`): integer floor division can
/// disagree by 1 px on non-divisible sizes through float rounding, so the
/// plane follows the evidenced float indexing exactly.
pub(crate) fn resize_plane(
    plane: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Result<Vec<u8>, TextureFactoryError> {
    if dst_w == 0 || dst_h == 0 {
        return Err(TextureFactoryError::Unsupported("alpha plane extent"));
    }
    let src_len = usize::try_from(src_w as u64 * src_h as u64)
        .map_err(|_| TextureFactoryError::Unsupported("alpha plane extent"))?;
    if plane.len() != src_len {
        return Err(TextureFactoryError::Unsupported("alpha plane extent"));
    }
    let mut out = Vec::new();
    out.try_reserve_exact(
        usize::try_from(dst_w as u64 * dst_h as u64)
            .map_err(|_| TextureFactoryError::Unsupported("alpha plane extent"))?,
    )
    .map_err(|_| TextureError::HostAllocation)?;
    let scale_x = src_w as f32 / dst_w as f32;
    let scale_y = src_h as f32 / dst_h as f32;
    for y in 0..dst_h {
        for x in 0..dst_w {
            let sx = (x as f32 * scale_x) as usize;
            let sy = (y as f32 * scale_y) as usize;
            out.push(plane[sy * src_w as usize + sx]);
        }
    }
    Ok(out)
}

/// Presaved DXT filename (ITexture.cpp:838-860): strip the last 4 source
/// bytes, append `[_alpha]_levels[L].DXn` (alpha suffix only for DXT3).
pub fn presaved_dxt_filename(
    name: &[u8],
    alpha_base: Option<&[u8]>,
    levels: u16,
    half: bool,
    dxt: u8,
) -> Result<Vec<u8>, TextureFactoryError> {
    let invalid = || TextureFactoryError::Unsupported("presaved filename");
    let mut file = name
        .get(..name.len().checked_sub(4).ok_or_else(invalid)?)
        .ok_or_else(invalid)?
        .to_vec();
    if let Some(alpha) = alpha_base {
        if dxt == PRESAVED_DXT3 {
            let base = alpha
                .get(..alpha.len().checked_sub(4).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            file.push(b'_');
            file.extend_from_slice(base);
        }
    }
    file.push(b'_');
    file.extend_from_slice(levels.to_string().as_bytes());
    if half {
        file.push(b'L');
    }
    file.extend_from_slice(b".DX");
    file.push(b'0' + dxt);
    Ok(file)
}

/// Presaved 555/565 filename (ITexture.cpp:905-923): `.565` for R5G6B5 else `.555`.
pub fn presaved_555_filename(
    name: &[u8],
    levels: u16,
    half: bool,
    format: u32,
) -> Result<Vec<u8>, TextureFactoryError> {
    let invalid = || TextureFactoryError::Unsupported("presaved filename");
    let mut file = name
        .get(..name.len().checked_sub(4).ok_or_else(invalid)?)
        .ok_or_else(invalid)?
        .to_vec();
    file.push(b'_');
    file.extend_from_slice(levels.to_string().as_bytes());
    if half {
        file.push(b'L');
    }
    file.push(b'.');
    file.push(b'5');
    file.push(if format == D3DFMT_R5G6B5 { b'6' } else { b'5' });
    file.push(b'5');
    Ok(file)
}

/// D3D surface byte size per level (native desc.Size accumulation,
/// ITexture.cpp:733). DXT blocks `(w+3)/4*(h+3)/4` times 8/16; L8 and other
/// formats have no Create meaning here. u16 dims keep products in u32 range.
pub(crate) fn create_level_byte_size(
    format: u32,
    w: u16,
    h: u16,
) -> Result<u32, TextureFactoryError> {
    let pixels = u32::from(w) * u32::from(h);
    let bytes = match format {
        D3DFMT_A8R8G8B8 | D3DFMT_X8R8G8B8 => pixels.checked_mul(4),
        D3DFMT_R8G8B8 => pixels.checked_mul(3),
        D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5 | D3DFMT_A4R4G4B4 => {
            pixels.checked_mul(2)
        }
        D3DFMT_DXT1 => u32::from(w)
            .div_ceil(4)
            .checked_mul(u32::from(h).div_ceil(4))
            .and_then(|blocks| blocks.checked_mul(8)),
        D3DFMT_DXT3 => u32::from(w)
            .div_ceil(4)
            .checked_mul(u32::from(h).div_ceil(4))
            .and_then(|blocks| blocks.checked_mul(16)),
        _ => return Err(TextureFactoryError::Unsupported("device surface size")),
    };
    bytes.ok_or(TextureError::HostAllocation.into())
}

/// Reached Create diagnostics with the original dbgPrintf prefixes preserved
/// byte-for-byte (the report Vec is the injected diagnostic sink).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateWarning {
    EmptyWithoutFlag,
    AlphaNameWithoutFlag { name: Vec<u8>, alpha: Vec<u8> },
    UseAlphaWithoutName { name: Vec<u8> },
}
impl CreateWarning {
    pub fn native_prefix(&self) -> &'static str {
        match self {
            Self::EmptyWithoutFlag => "ITexture::Create() - Warning! Neither Texture nor Alpha was specified, but ITC_EMPTY wasn't used!",
            Self::AlphaNameWithoutFlag { .. } => "ITexture::Create() - Warning! AlphaName was specified, but ITC_USE_ALPHA wasn't! (%s, %s)",
            Self::UseAlphaWithoutName { .. } => "ITexture::Create() - Warning! ITC_USE_ALPHA specified, but ( AlphaName == NULL ) !!  %s",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreateReport {
    pub warnings: Vec<CreateWarning>,
}

/// One uploaded mip level and its dimensions (CPU device stand-in).
pub struct StoredDeviceLevel {
    pub width: u16,
    pub height: u16,
    pub bytes: Vec<u8>,
}
/// Device texture behind the sink: descriptor plus extra (non-0) levels.
pub struct DeviceTextureExtra {
    pub desc: DeviceTextureDesc,
    pub levels: Vec<StoredDeviceLevel>,
}
/// Ordered device-tail record (native call order preserved by the port).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    Created {
        owner: TextureKey,
        width: u16,
        height: u16,
        levels: u16,
        format: u32,
    },
    Uploaded {
        graphics: TextureGraphics,
        level: u16,
        width: u16,
        height: u16,
        format: u32,
    },
    Compressed {
        graphics: TextureGraphics,
        level: u16,
        dxt: u8,
    },
    PixelsKept {
        owner: TextureKey,
        bytes: usize,
    },
    PresavedAttempt {
        filename: Vec<u8>,
        dxt: u8,
    },
    PresavedStored {
        filename: Vec<u8>,
        dxt: u8,
    },
    Preloaded {
        graphics: TextureGraphics,
    },
    Filled {
        graphics: TextureGraphics,
        color: u32,
    },
}

impl TextureResources {
    /// IGraph::CreateITexture cache prefix (IGraph.cpp:583-620): reverse
    /// cached-list search on creation flags plus case-insensitive full-name
    /// (and alpha-name when ITC_USE_ALPHA) match. Shared by both factories.
    fn find_cached_create_texture(
        &self,
        name: &[u8],
        alpha: Option<&[u8]>,
        flags: u32,
    ) -> Result<Option<TextureKey>, TextureFactoryError> {
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
                && (flags & ITC_USE_ALPHA == 0 || matches(t.alpha_name, alpha)?)
            {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }
}

#[allow(clippy::too_many_arguments)]
fn upload_full_level(
    storage: &mut CpuTextureStorage,
    graphics: TextureGraphics,
    aux: Option<TextureGraphics>,
    level: u16,
    bytes: &[u8],
    width: u16,
    height: u16,
    selected: &SelectedCreateFormat,
    dither_main: bool,
    dither_aux: bool,
) -> Result<(), TextureFactoryError> {
    // ITexture.cpp:736-750 and 786-799: with an aux surface the ARGB converts
    // there and CompressTexture moves it into the main surface; otherwise it
    // converts into the main surface with the DXT type as no-dither flag.
    if let Some(aux_graphics) = aux {
        {
            let sink = storage
                .creation_sink()
                .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
            sink.upload_create_level(
                aux_graphics,
                UploadedCreateLevel {
                    level,
                    width,
                    height,
                    bytes,
                    format: selected.dxt_format,
                    dither: dither_aux,
                },
            )
            .map_err(TextureFactoryError::Backend)?;
        }
        {
            let sink = storage
                .creation_sink()
                .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
            sink.compress_create_level(graphics, level, selected.dxt)
                .map_err(TextureFactoryError::Backend)?;
        }
    } else {
        let sink = storage
            .creation_sink()
            .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
        sink.upload_create_level(
            graphics,
            UploadedCreateLevel {
                level,
                width,
                height,
                bytes,
                format: selected.format,
                dither: dither_main,
            },
        )
        .map_err(TextureFactoryError::Backend)?;
    }
    Ok(())
}

impl TextureResources {
    /// ITexture::Create (RVA 0x75f50) immediate port: file (TGA/BMP via the
    /// image source), memory (BITMAPINFOHEADER buffer) and empty branches with
    /// the original flag fixups, format ladder, mip chain and error words.
    /// The IGraph::CreateITexture cache search runs first as the sibling's
    /// does; managed-domain flags reject explicitly. Over-cap images scale
    /// down through TEXTURE_SIZE as the original scales them (unlike the
    /// sibling's fixed-cap rejection); non-power-of-two sources without
    /// ITC_ANYSIZE resize through the LoadImage-family nearest scaler, with
    /// IGraph::ResizeImage indexing fidelity unverified (see OPEN). Device
    /// tails cross the TextureCreationSink boundary in original order.
    /// Errors own no returned reference: the fresh texture is released
    /// through its actual destructor on failure, as the sibling does.
    #[allow(clippy::too_many_arguments)]
    pub fn create_texture_full(
        &mut self,
        name: Option<&[u8]>,
        memory: Option<&[u8]>,
        alpha: Option<&[u8]>,
        flags: u32,
        sx: u16,
        sy: u16,
        levels: u16,
        caps: &CreateDeviceCaps,
        source: &mut impl TextureImageSource,
        storage: &mut CpuTextureStorage,
    ) -> Result<(TextureKey, CreateReport), TextureFactoryError> {
        self.available()?;
        let name = name.map(cbytes);
        let alpha = alpha.map(cbytes);
        if name.is_some_and(|n| !n.is_ascii()) || alpha.is_some_and(|n| !n.is_ascii()) {
            return Err(TextureFactoryError::Unsupported("filename encoding"));
        }
        if flags & CREATE_MANAGED_FLAGS != 0 {
            return Err(TextureFactoryError::Unsupported("managed texture domain"));
        }
        if flags & !CREATE_EXECUTED_FLAGS != 0 {
            return Err(TextureFactoryError::Unsupported("Create flags"));
        }
        if flags & ITC_FROM_MEMORY != 0 && memory.is_none() {
            // Native reads a NULL header pointer (ITexture.cpp:272-273).
            return Err(TextureFactoryError::InvalidParameters);
        }
        if flags & ITC_FROM_MEMORY == 0 && memory.is_some() {
            // Native would parse the pointer as a filename; reject the
            // contradictory intake instead of misreading it.
            return Err(TextureFactoryError::InvalidParameters);
        }
        let mut flags = flags;
        if alpha.is_some_and(|a| !a.is_empty()) {
            flags |= ITC_USE_ALPHA;
        }
        if flags & (ITC_UNCACHED | ITC_FROM_MEMORY) == 0 {
            // The IGraph-level search needs a name; nameless (empty-branch)
            // creates skip it, as there is nothing to match.
            if let Some(search_name) = name.filter(|n| !n.is_empty()) {
                if let Some(hit) = self.find_cached_create_texture(search_name, alpha, flags)? {
                    self.retain_texture(hit)?;
                    return Ok((hit, CreateReport::default()));
                }
            }
        }
        let key = self.create_igraph_texture()?;
        let result = self.create_full_inner(
            key, name, memory, alpha, flags, sx, sy, levels, caps, source, storage,
        );
        if result.is_err() {
            self.release_texture(key, storage)?;
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn create_full_inner(
        &mut self,
        key: TextureKey,
        name: Option<&[u8]>,
        memory: Option<&[u8]>,
        alpha: Option<&[u8]>,
        flags: u32,
        sx: u16,
        sy: u16,
        levels: u16,
        caps: &CreateDeviceCaps,
        source: &mut impl TextureImageSource,
        storage: &mut CpuTextureStorage,
    ) -> Result<(TextureKey, CreateReport), TextureFactoryError> {
        let mut report = CreateReport::default();
        // Native Close() prefix (ITexture.cpp:251).
        self.close_texture(key, storage)?;
        self.textures.texture_mut(key).flags = flags;
        self.textures.texture_mut(key).creation_flags = flags;
        let from_memory = flags & ITC_FROM_MEMORY != 0;
        let mut effective = flags;
        let mut full_name_bytes: Vec<u8> = Vec::new();
        let mut src_w: u32 = 0;
        let mut src_h: u32 = 0;
        let mut src_bpp: u16 = 0;
        // Bottom-up raw rows at src_stride (native file order for BMP and
        // bottom-origin TGA; top-origin TGA rows are flipped after load).
        let mut src_rows: Vec<u8> = Vec::new();
        let mut src_stride: u32 = 0;
        // Raw palette bytes with entry stride 3 (TGA) or 4 (BMP).
        let mut src_palette: Vec<u8> = Vec::new();
        let mut src_pal_stride: u32 = 0;
        let mut src_palette_key: Option<[u8; 3]> = None;
        let mut deferred_bad_format = false;
        let (mut tex_w, mut tex_h): (u16, u16);
        if from_memory {
            // ITexture.cpp:270-291. m_Flags = flags | ITC_UNCACHED.
            effective = flags | ITC_UNCACHED;
            self.textures.texture_mut(key).flags = effective;
            let buffer = memory.ok_or(TextureFactoryError::InvalidParameters)?;
            let image = decode_memory_create_image(buffer, effective)?;
            tex_w = if sx > 0 {
                sx
            } else {
                u16::try_from(image.width)
                    .map_err(|_| TextureFactoryError::Image(TextureImageError::Corrupt))?
            };
            tex_h = if sy > 0 {
                sy
            } else {
                u16::try_from(image.height)
                    .map_err(|_| TextureFactoryError::Image(TextureImageError::Corrupt))?
            };
            let t = self.textures.texture_mut(key);
            t.width = Observed::Value(tex_w);
            t.height = Observed::Value(tex_h);
            let (w, h) = if effective & ITC_HALF != 0 {
                (tex_w >> 1, tex_h >> 1)
            } else {
                (tex_w, tex_h)
            };
            // The memory branch always rounds (no ITC_ANYSIZE check, 282-287).
            (tex_w, tex_h) =
                create_texture_size(w, h, caps.max_width, caps.max_height, caps.dxt_size_minimum);
            src_w = image.width;
            src_h = image.height;
            src_bpp = image.bpp;
            src_stride = image.stride;
            src_rows = buffer[image.rows.clone()].to_vec();
            if image.bpp == 8 {
                // palette_len is already bytes (entries * 4).
                let pal = buffer
                    .get(40..40 + image.palette_len)
                    .ok_or(TextureFactoryError::Image(TextureImageError::Corrupt))?;
                src_palette = pal.to_vec();
                src_pal_stride = 4;
                src_palette_key = image.palette_key;
            }
            deferred_bad_format = image.deferred_bad_format;
        } else if let Some(requested) = name {
            // ITexture.cpp:292-331. m_Flags = flags & ~ITC_EMPTY.
            effective = flags & !ITC_EMPTY;
            self.textures.texture_mut(key).flags = effective;
            let full = full_name(source, requested)?;
            let file_key = self.textures.texture(key)?.file_name;
            self.strings
                .assign(file_key, Some(full.as_slice()), storage)?;
            // GetImageInfo analog: header-only probe before any pixel load.
            let file_bytes = source
                .read_image(&full)
                .map_err(TextureFactoryError::Image)?;
            let info = lhr_assets::texture_source::inspect_texture_image(&file_bytes)
                .map_err(decode_error)
                .map_err(TextureFactoryError::Image)?;
            let rep = self
                .strings
                .string(file_key)?
                .rep
                .ok_or(TextureError::IntakeAllocationRejected)?;
            // Basename view (ITexture.cpp:299-305).
            self.textures.texture_mut(key).name = Observed::Value(Some(RepView {
                rep,
                offset: basename_offset(&full),
            }));
            let width = u16::try_from(info.width)
                .map_err(|_| TextureFactoryError::Unsupported("image dimensions"))?;
            let height = u16::try_from(info.height)
                .map_err(|_| TextureFactoryError::Unsupported("image dimensions"))?;
            let t = self.textures.texture_mut(key);
            t.width = Observed::Value(width);
            t.height = Observed::Value(height);
            full_name_bytes = full;
            effective = apply_file_bpp_flag_fixup(effective, info.bpp);
            self.textures.texture_mut(key).flags = effective;
            tex_w = if sx > 0 { sx } else { width };
            tex_h = if sy > 0 { sy } else { height };
            let (w, h) = if effective & ITC_HALF != 0 {
                (tex_w >> 1, tex_h >> 1)
            } else {
                (tex_w, tex_h)
            };
            if effective & ITC_ANYSIZE == 0 {
                (tex_w, tex_h) = create_texture_size(
                    w,
                    h,
                    caps.max_width,
                    caps.max_height,
                    caps.dxt_size_minimum,
                );
            } else {
                (tex_w, tex_h) = (w, h);
            }
            // LoadImage analog (ITexture.cpp:328): raw rows land here in
            // original order (bottom-up; top-origin TGA rows flip inside the
            // extractor). Unsupported depths defer to the conversion stage
            // so device creation and presaved attempt 1 keep their order
            // (BADTEXTUREFORMAT surfaces there, ITexture.cpp:655-662).
            src_bpp = info.bpp;
            if !matches!(info.bpp, 8 | 16 | 24 | 32) {
                src_w = info.width;
                src_h = info.height;
                deferred_bad_format = true;
            } else {
                let (rows, stride) =
                    extract_file_rows(&file_bytes, &info, effective & ITC_NOALIGN != 0)?;
                src_stride = stride;
                src_rows = rows;
                if info.bpp == 8 {
                    let is_tga = !file_bytes.starts_with(b"BM");
                    if is_tga {
                        // Color-map specification at header bytes 5..8: entry
                        // count (u16) and entry size in bits. The map follows
                        // the id field (18 + id).
                        let id = file_bytes.first().copied().unwrap_or(0) as usize;
                        let count = u16::from_le_bytes(
                            file_bytes
                                .get(5..7)
                                .ok_or(TextureFactoryError::Image(TextureImageError::Corrupt))?
                                .try_into()
                                .unwrap(),
                        ) as usize;
                        let entry_bits = file_bytes.get(7).copied().unwrap_or(0);
                        if entry_bits != 24 && entry_bits != 32 {
                            return Err(TextureFactoryError::Image(TextureImageError::Corrupt));
                        }
                        let pal = file_bytes
                            .get(18 + id..18 + id + count * entry_bits as usize / 8)
                            .ok_or(TextureFactoryError::Image(TextureImageError::Corrupt))?;
                        // TGA entries are B,G,R[,A]; pad to RGBQUAD stride.
                        src_palette = vec![0u8; count * 4];
                        for (i, chunk) in pal.chunks_exact(entry_bits as usize / 8).enumerate() {
                            src_palette[i * 4..i * 4 + 3].copy_from_slice(&chunk[..3]);
                        }
                        src_pal_stride = 4;
                        src_palette_key = Some([src_palette[2], src_palette[1], src_palette[0]]);
                    } else {
                        // BMP palette at the fixed color-table offset (biClrUsed
                        // at bytes 46..50, 0 means 1<<bpp); rows already landed
                        // above at bfOffBits with the shared stride rule.
                        // biClrUsed at bytes 46..50 (0 means 1<<bpp).
                        let clr = u32::from_le_bytes(
                            file_bytes
                                .get(46..50)
                                .ok_or(TextureFactoryError::Image(TextureImageError::Corrupt))?
                                .try_into()
                                .unwrap(),
                        );
                        let entries = if clr == 0 { 1u32 << 8 } else { clr };
                        let pal = file_bytes
                            .get(54..54 + entries as usize * 4)
                            .ok_or(TextureFactoryError::Image(TextureImageError::Corrupt))?;
                        src_palette = pal.to_vec();
                        src_pal_stride = 4;
                        src_palette_key = Some([pal[2], pal[1], pal[0]]);
                    }
                }
                src_w = info.width;
                src_h = info.height;
            }
        } else {
            // ITexture.cpp:332-349. REA anchor: the empty warning string
            // @0x100ab0b0 referenced from 0x10076454.
            if flags & ITC_EMPTY == 0 {
                report.warnings.push(CreateWarning::EmptyWithoutFlag);
                effective |= ITC_EMPTY;
                self.textures.texture_mut(key).flags = effective;
            }
            let t = self.textures.texture_mut(key);
            t.width = Observed::Value(sx);
            t.height = Observed::Value(sy);
            tex_w = sx;
            tex_h = sy;
            let (w, h) = if effective & ITC_HALF != 0 {
                (tex_w >> 1, tex_h >> 1)
            } else {
                (tex_w, tex_h)
            };
            if effective & ITC_ANYSIZE == 0 {
                (tex_w, tex_h) = create_texture_size(
                    w,
                    h,
                    caps.max_width,
                    caps.max_height,
                    caps.dxt_size_minimum,
                );
            } else {
                (tex_w, tex_h) = (w, h);
            }
        }
        // Alpha-name warning (ITexture.cpp:351-355; REA anchor @0x100ab050
        // referenced from 0x1007657e).
        if alpha.is_some_and(|a| !a.is_empty()) && effective & ITC_USE_ALPHA == 0 {
            report.warnings.push(CreateWarning::AlphaNameWithoutFlag {
                name: full_name_bytes.clone(),
                alpha: alpha.unwrap_or_default().to_vec(),
            });
            effective |= ITC_USE_ALPHA;
            self.textures.texture_mut(key).flags = effective;
        }
        // Alpha intake (ITexture.cpp:356-424). Filename assigns before the
        // load (358-359); load failure is NOFILE1; only 8/24-bit sources are
        // admitted (BADTEXTUREFORMAT otherwise). The empty branch still loads
        // (and discards) the plane; its leak at 664-666 is plugged (see OPEN).
        let mut alpha_plane: Option<Vec<u8>> = None;
        let mut alpha_base: Option<Vec<u8>> = None;
        if effective & ITC_USE_ALPHA != 0 {
            if let Some(alpha_name) = alpha.filter(|a| !a.is_empty()) {
                let afull = full_name(source, alpha_name)?;
                let akey = self.textures.texture(key)?.alpha_file_name;
                self.strings.assign(akey, Some(afull.as_slice()), storage)?;
                let abytes = source
                    .read_image(&afull)
                    .map_err(TextureFactoryError::AlphaImage)?;
                // Alpha intake reads raw rows like the primary (ITexture.cpp
                // :359-417 load the alpha through LoadImage with the source
                // dims as sx/sy). Only 8/24-bit sources are admitted
                // (BADTEXTUREFORMAT otherwise); padded strides are honored,
                // not assumed tight -- a padded-BMP alpha exercises this
                // (see OPEN for the native tight-walk question).
                let ainfo = lhr_assets::texture_source::inspect_texture_image(&abytes)
                    .map_err(decode_error)
                    .map_err(TextureFactoryError::AlphaImage)?;
                if !matches!(ainfo.bpp, 8 | 24) {
                    return Err(TextureFactoryError::BadTextureFormat);
                }
                let (arows, astride) = extract_file_rows(
                    &abytes,
                    &ainfo,
                    effective & ITC_NOALIGN != 0,
                )
                .map_err(|e| match e {
                    TextureFactoryError::Image(inner) => TextureFactoryError::AlphaImage(inner),
                    other => other,
                })?;
                let arep = self
                    .strings
                    .string(akey)?
                    .rep
                    .ok_or(TextureError::IntakeAllocationRejected)?;
                self.textures.texture_mut(key).alpha_name = Observed::Value(Some(RepView {
                    rep: arep,
                    offset: basename_offset(&afull),
                }));
                // Brightness plane at alpha dims, then nearest resize to the
                // source dims (LoadImage sx/sy path). Brightness is linear,
                // so plane-resize equals resize-then-brightness exactly;
                // empty textures have no source dims, matching the 0x0
                // bypass (`!sx || !sy`, ITexture.cpp:1597-1602).
                let araw = RawCreateRows {
                    rows: &arows,
                    width: ainfo.width,
                    height: ainfo.height,
                    stride: astride,
                    bpp: ainfo.bpp,
                    palette: None,
                    palette_key: None,
                };
                // 8-bit alpha sources expand through their container palette
                // before brightness (mirrors the primary 8-bit path).
                let mut plane = if ainfo.bpp == 8 {
                    let is_tga = !abytes.starts_with(b"BM");
                    let (pal_bytes, entry_stride) = if is_tga {
                        let id = abytes.first().copied().unwrap_or(0) as usize;
                        let count = u16::from_le_bytes(
                            abytes
                                .get(5..7)
                                .ok_or(TextureFactoryError::AlphaImage(TextureImageError::Corrupt))?
                                .try_into()
                                .unwrap(),
                        ) as usize;
                        let entry_bits = abytes.get(7).copied().unwrap_or(0);
                        if entry_bits != 24 && entry_bits != 32 {
                            return Err(TextureFactoryError::AlphaImage(
                                TextureImageError::Corrupt,
                            ));
                        }
                        (
                            abytes
                                .get(18 + id..18 + id + count * entry_bits as usize / 8)
                                .ok_or(TextureFactoryError::AlphaImage(
                                    TextureImageError::Corrupt,
                                ))?,
                            3u32,
                        )
                    } else {
                        let clr = u32::from_le_bytes(
                            abytes
                                .get(46..50)
                                .ok_or(TextureFactoryError::AlphaImage(TextureImageError::Corrupt))?
                                .try_into()
                                .unwrap(),
                        );
                        let entries = if clr == 0 { 1u32 << 8 } else { clr };
                        (
                            abytes.get(54..54 + entries as usize * 4).ok_or(
                                TextureFactoryError::AlphaImage(TextureImageError::Corrupt),
                            )?,
                            4u32,
                        )
                    };
                    let expanded = RawCreateRows {
                        rows: &arows,
                        width: ainfo.width,
                        height: ainfo.height,
                        stride: astride,
                        bpp: ainfo.bpp,
                        palette: Some((pal_bytes, entry_stride)),
                        palette_key: None,
                    };
                    brightness_plane_raw(&expanded).map_err(|e| match e {
                        TextureFactoryError::Image(inner) => TextureFactoryError::AlphaImage(inner),
                        other => other,
                    })?
                } else {
                    brightness_plane_raw(&araw).map_err(|e| match e {
                        TextureFactoryError::Image(inner) => TextureFactoryError::AlphaImage(inner),
                        other => other,
                    })?
                };
                if src_w != 0 && src_h != 0 && (ainfo.width, ainfo.height) != (src_w, src_h) {
                    plane = resize_plane(&plane, ainfo.width, ainfo.height, src_w, src_h)?;
                }
                alpha_plane = Some(plane);
                alpha_base = Some(afull[basename_offset(&afull) as usize..].to_vec());
            } else if effective & ITC_EMPTY == 0 {
                // REA anchor: @0x100aaf80 referenced from 0x10076839.
                report.warnings.push(CreateWarning::UseAlphaWithoutName {
                    name: full_name_bytes.clone(),
                });
                effective &= !ITC_USE_ALPHA;
                self.textures.texture_mut(key).flags = effective;
            }
        }
        // Format ladder (ITexture.cpp:426-464) and device creation (466-467;
        // the original leaves CreateTexture's result unchecked -- host failure
        // surfaces as Backend, never a native word).
        let (adjusted, selected) = select_create_format(effective, &caps.formats);
        effective = adjusted;
        self.textures.texture_mut(key).flags = effective;
        self.textures.texture_mut(key).format = Observed::Value(selected.format);
        let num_levels = if effective & ITC_MIPMAP == 0 {
            1
        } else {
            levels
        };
        let desc = DeviceTextureDesc {
            width: tex_w,
            height: tex_h,
            requested_levels: num_levels,
            format: selected.format,
            mipmaps: effective & ITC_MIPMAP != 0,
            dxt: selected.dxt,
            dxt_format: selected.dxt_format,
        };
        let (graphics, actual) = {
            let sink = storage
                .creation_sink()
                .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
            sink.create_device_texture(key, &desc)
                .map_err(TextureFactoryError::Backend)?
        };
        if actual == 0 {
            return Err(TextureFactoryError::Unsupported("device level count"));
        }
        let t = self.textures.texture_mut(key);
        t.graphics = Some(graphics);
        t.levels = Observed::Value(actual);
        t.width = Observed::Value(tex_w);
        t.height = Observed::Value(tex_h);
        let half = effective & ITC_HALF != 0;
        // Presaved attempt 1 (ITexture.cpp:469-489), skipped for
        // EMPTY|KEEP_PIXELS|FROM_MEMORY (TEST 0x9001 @0x10076979).
        if effective & (ITC_EMPTY | ITC_KEEP_PIXELS | ITC_FROM_MEMORY) == 0 {
            let attempt = if selected.dxt == PRESAVED_DXT1 {
                Some((
                    presaved_dxt_filename(&full_name_bytes, None, actual, half, selected.dxt)?,
                    true,
                ))
            } else if selected.dxt > 1 {
                Some((
                    presaved_dxt_filename(
                        &full_name_bytes,
                        alpha_base.as_deref(),
                        actual,
                        half,
                        selected.dxt,
                    )?,
                    true,
                ))
            } else if matches!(
                selected.format,
                D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5
            ) {
                Some((
                    presaved_555_filename(&full_name_bytes, actual, half, selected.format)?,
                    false,
                ))
            } else {
                None
            };
            if let Some((filename, is_dxt)) = attempt {
                let probe = PresavedAttempt {
                    filename,
                    width: tex_w,
                    height: tex_h,
                    levels: actual,
                    half,
                    dxt: selected.dxt,
                    alpha_name: alpha_base.clone(),
                };
                let hit = {
                    let sink = storage
                        .creation_sink()
                        .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
                    if is_dxt {
                        sink.try_presaved_dxt(&probe)
                            .map_err(TextureFactoryError::Backend)?
                    } else {
                        sink.try_presaved_555_565(&probe)
                            .map_err(TextureFactoryError::Backend)?
                    }
                };
                if hit {
                    // ITexture.cpp:479-488: init, list, Preload, early return.
                    self.textures.texture_mut(key).initialized = true;
                    self.register_cache(
                        key,
                        if effective & ITC_UNCACHED == 0 {
                            CacheList::Cached
                        } else {
                            CacheList::Uncached
                        },
                    )?;
                    let sink = storage
                        .creation_sink()
                        .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
                    sink.preload_create_texture(graphics)
                        .map_err(TextureFactoryError::Backend)?;
                    return Ok((key, report));
                }
            }
        }
        // ARGB conversion (ITexture.cpp:491-667), skipped for EMPTY.
        let empty = effective & ITC_EMPTY != 0;
        let mut argb: Vec<u8> = Vec::new();
        if !empty {
            if deferred_bad_format {
                return Err(TextureFactoryError::BadTextureFormat);
            }
            let raw_view = RawCreateRows {
                rows: &src_rows,
                width: src_w,
                height: src_h,
                stride: src_stride,
                bpp: src_bpp,
                palette: if src_palette.is_empty() {
                    None
                } else {
                    Some((src_palette.as_slice(), src_pal_stride))
                },
                palette_key: src_palette_key,
            };
            let converted = convert_create_pixels(
                &raw_view,
                alpha_plane.as_deref(),
                effective,
                selected.format,
            )?;
            self.textures.texture_mut(key).color_key = Observed::Value(converted.color_key);
            argb = converted.argb;
            // Primary resize to texture dims (ITexture.cpp:669-677); kept
            // pixels are the resized image, as the original reassigns
            // m_pPixels before the KEEP decision.
            if u32::from(tex_w) != src_w || u32::from(tex_h) != src_h {
                let resized = resize_loaded_texture(
                    lhr_assets::Texture {
                        width: src_w,
                        height: src_h,
                        rgba: std::mem::take(&mut argb),
                    },
                    u32::from(tex_w),
                    u32::from(tex_h),
                )?;
                argb = resized.rgba;
            }
        }
        // Presaved attempt 2, DXT only, post-resize (ITexture.cpp:679-689).
        if !from_memory && selected.dxt >= 1 {
            let probe = PresavedAttempt {
                filename: presaved_dxt_filename(
                    &full_name_bytes,
                    alpha_base.as_deref(),
                    actual,
                    half,
                    selected.dxt,
                )?,
                width: tex_w,
                height: tex_h,
                levels: actual,
                half,
                dxt: selected.dxt,
                alpha_name: alpha_base.clone(),
            };
            let hit = {
                let sink = storage
                    .creation_sink()
                    .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
                sink.try_presaved_dxt(&probe)
                    .map_err(TextureFactoryError::Backend)?
            };
            if hit {
                self.textures.texture_mut(key).initialized = true;
                self.register_cache(
                    key,
                    if effective & ITC_UNCACHED == 0 {
                        CacheList::Cached
                    } else {
                        CacheList::Uncached
                    },
                )?;
                let sink = storage
                    .creation_sink()
                    .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
                sink.preload_create_texture(graphics)
                    .map_err(TextureFactoryError::Backend)?;
                return Ok((key, report));
            }
        }
        // Mip scratch (ITexture.cpp:695-705): the level_src copy failure is
        // GENERIC while the level_dst failure is OUTOFMEM -- the words differ
        // natively and the port preserves the split.
        let mut aux: Option<TextureGraphics> = None;
        // Ping-pong scratch pair plus current pitch (never cleared, as the
        // original never clears its scratch; see filter_mipmap_level).
        let mut mip: Option<(Vec<u8>, Vec<u8>, u32)> = None;
        if !empty && actual > 1 {
            let mut src_scratch = Vec::new();
            src_scratch
                .try_reserve_exact(argb.len())
                .map_err(|_| TextureFactoryError::GenericFailure)?;
            src_scratch.extend_from_slice(&argb);
            let dst_len = (u32::from(tex_w) * u32::from(tex_h) * 4) as usize;
            let mut dst_scratch = Vec::new();
            dst_scratch
                .try_reserve_exact(dst_len)
                .map_err(|_| TextureError::HostAllocation)?;
            dst_scratch.resize(dst_len, 0);
            mip = Some((src_scratch, dst_scratch, u32::from(tex_w) * 4));
        }
        if !empty && selected.dxt_format != D3DFMT_UNKNOWN {
            // ITexture.cpp:707-714; creation failure is GENERIC natively.
            let aux_desc = DeviceTextureDesc {
                format: selected.dxt_format,
                ..desc
            };
            let (aux_graphics, _) = {
                let sink = storage
                    .creation_sink()
                    .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
                sink.create_device_texture(key, &aux_desc)
                    .map_err(|_| TextureFactoryError::GenericFailure)?
            };
            aux = Some(aux_graphics);
        }
        // Level loop (ITexture.cpp:715-813). m_MemSize accumulates with u32
        // wrap (native `+=`), including EMPTY levels.
        let mut mem_size: u32 = 0;
        let mut level_error: Option<TextureFactoryError> = None;
        let (mut cw, mut ch) = (tex_w, tex_h);
        let dither_main = effective & ITC_DITHER != 0 && selected.dxt == 0;
        let dither_aux = effective & ITC_DITHER != 0;
        for level in 0..actual {
            match create_level_byte_size(selected.format, cw, ch) {
                Ok(size) => mem_size = mem_size.wrapping_add(size),
                Err(_) => {
                    level_error = Some(TextureFactoryError::GenericFailure);
                    break;
                }
            }
            if !empty {
                if level == 0 {
                    if let Err(e) = upload_full_level(
                        storage,
                        graphics,
                        aux,
                        0,
                        &argb,
                        cw,
                        ch,
                        &selected,
                        dither_main,
                        dither_aux,
                    ) {
                        level_error = Some(e);
                        break;
                    }
                } else {
                    let Some((src_buf, dst_buf, pitch)) = mip.as_mut() else {
                        level_error = Some(TextureFactoryError::GenericFailure);
                        break;
                    };
                    let step = (|| {
                        let (nw, nh, next_pitch) = filter_mipmap_level(
                            src_buf,
                            dst_buf,
                            u32::from(cw),
                            u32::from(ch),
                            *pitch,
                        )?;
                        let tight = (u32::from(nw) * u32::from(nh) * 4) as usize;
                        let mut bytes = Vec::new();
                        bytes
                            .try_reserve_exact(tight)
                            .map_err(|_| TextureFactoryError::GenericFailure)?;
                        bytes.extend_from_slice(&dst_buf[..tight]);
                        std::mem::swap(src_buf, dst_buf);
                        *pitch = next_pitch;
                        Ok((bytes, nw, nh))
                    })();
                    match step {
                        Ok((bytes, nw, nh)) => {
                            if let Err(e) = upload_full_level(
                                storage,
                                graphics,
                                aux,
                                level,
                                &bytes,
                                nw,
                                nh,
                                &selected,
                                dither_main,
                                dither_aux,
                            ) {
                                level_error = Some(e);
                                break;
                            }
                            (cw, ch) = (nw, nh);
                        }
                        Err(e) => {
                            level_error = Some(e);
                            break;
                        }
                    }
                }
            } else {
                (cw, ch) = mipmap_level_dims(cw, ch);
            }
        }
        // Aux release (native dxt_texture->Release, ITexture.cpp:826-827).
        if let Some(aux_graphics) = aux {
            let released = storage
                .release_texture_graphics(aux_graphics)
                .map_err(TextureFactoryError::Backend);
            if level_error.is_none() {
                released?;
            }
        }
        if let Some(e) = level_error {
            return Err(e);
        }
        self.textures.texture_mut(key).memory_size = mem_size;
        if effective & ITC_EMPTY != 0 {
            // ITexture.cpp:815-821: FILL, uncached list, init, return -- the
            // empty path never calls Preload.
            if effective & ITC_FILL != 0 {
                let sink = storage
                    .creation_sink()
                    .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
                sink.fill_empty_texture(graphics, 0)
                    .map_err(TextureFactoryError::Backend)?;
            }
            self.textures.texture_mut(key).initialized = true;
            self.register_cache(key, CacheList::Uncached)?;
            return Ok((key, report));
        }
        if effective & ITC_KEEP_PIXELS == 0 {
            // Native delete[] m_pPixels (ITexture.cpp:822-825); ours is a
            // local and simply drops.
        } else {
            let pixels = {
                let sink = storage
                    .creation_sink()
                    .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
                sink.keep_create_pixels(key, argb)
                    .map_err(TextureFactoryError::Backend)?
            };
            self.textures.texture_mut(key).pixels = Some(pixels);
        }
        self.textures.texture_mut(key).initialized = true;
        self.register_cache(
            key,
            if effective & ITC_UNCACHED == 0 {
                CacheList::Cached
            } else {
                CacheList::Uncached
            },
        )?;
        {
            let sink = storage
                .creation_sink()
                .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
            sink.preload_create_texture(graphics)
                .map_err(TextureFactoryError::Backend)?;
        }
        // Save tails (ITexture.cpp:838-963). Failures still return OK natively
        // ("Can't create %s !!!"); the CPU stand-in only records (see OPEN).
        // Empty names cannot occur here (file branch always names; memory and
        // empty branches return before the tails... except EMPTY with DXT
        // flags, where the original dereferences NULL -- the host skips the
        // save instead of faulting).
        if !from_memory && !full_name_bytes.is_empty() && selected.dxt > 0 {
            let attempt = PresavedAttempt {
                filename: presaved_dxt_filename(
                    &full_name_bytes,
                    alpha_base.as_deref(),
                    actual,
                    half,
                    selected.dxt,
                )?,
                width: tex_w,
                height: tex_h,
                levels: actual,
                half,
                dxt: selected.dxt,
                alpha_name: alpha_base.clone(),
            };
            let sink = storage
                .creation_sink()
                .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
            sink.store_presaved_dxt(graphics, &attempt)
                .map_err(TextureFactoryError::Backend)?;
        }
        if !from_memory
            && !full_name_bytes.is_empty()
            && matches!(
                selected.format,
                D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5
            )
        {
            let attempt = PresavedAttempt {
                filename: presaved_555_filename(&full_name_bytes, actual, half, selected.format)?,
                width: tex_w,
                height: tex_h,
                levels: actual,
                half,
                dxt: 0,
                alpha_name: None,
            };
            let sink = storage
                .creation_sink()
                .ok_or(TextureFactoryError::Unsupported("device creation sink"))?;
            sink.store_presaved_555_565(graphics, &attempt)
                .map_err(TextureFactoryError::Backend)?;
        }
        Ok((key, report))
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

    #[test]
    fn create_flag_words_match_shipped_immediates() {
        assert_eq!(
            (ITC_FROM_MEMORY, ITC_ALPHA, ITC_USE_ALPHA, ITC_COLORKEY),
            (0x1, 0x2, 0x4, 0x8)
        );
        assert_eq!(
            (ITC_MANAGED, ITC_MIPMAP, ITC_DXT, ITC_TRUECOLOR),
            (0x40, 0x100, 0x200, 0x400)
        );
        assert_eq!(
            (ITC_KEEP_PIXELS, ITC_HALF, ITC_FILL, ITC_EMPTY),
            (0x1000, 0x2000, 0x4000, 0x8000)
        );
        assert_eq!(
            (ITC_NOALIGN, ITC_UNCACHED, ITC_ANYSIZE),
            (0x40000, 0x200000, 0x400000)
        );
        assert_eq!(ITC_EMPTY | ITC_KEEP_PIXELS | ITC_FROM_MEMORY, 0x9001);
        assert_eq!(ITC_UNCACHED | ITC_FROM_MEMORY, 0x200001);
        // Dither is the low-byte sign bit (IGraph.h:637; shipped body tests
        // `MOV AL,[EDI+0x30]/TEST AL,AL/JNS`), not the LOADED dword bit.
        assert_eq!(ITC_DITHER, 0x80);
        assert_eq!(ITC_LOADED, 0x80000000);
    }
    #[test]
    fn resize_plane_follows_float_scaler_on_odd_sizes() {
        // 3-wide plane down to 2: float scale 1.5 samples x=0 and x=1
        // (1*1.5=1.5 truncates to 1), matching the LoadImage scaler.
        assert_eq!(resize_plane(&[10, 20, 30], 3, 1, 2, 1).unwrap(), [10, 20]);
        // 5-wide down to 2: x=1 samples (1*2.5)=2.
        assert_eq!(resize_plane(&[1, 2, 3, 4, 5], 5, 1, 2, 1).unwrap(), [1, 3]);
    }
    #[test]
    fn format_ladder_selects_every_native_branch() {
        let caps = |t: u32, a: u32, a1: u32, tex: u32, d: u32, da: u32| CreateFormatCaps {
            truecolor: t,
            alpha: a,
            alpha1: a1,
            texture: tex,
            dxt: d,
            dxt_alpha: da,
        };
        let full = caps(21, 22, 23, 24, 25, 26);
        assert_eq!(
            select_create_format(ITC_TRUECOLOR, &full).1,
            SelectedCreateFormat {
                format: 21,
                dxt: 0,
                dxt_format: 0
            }
        );
        assert_eq!(
            select_create_format(ITC_TRUECOLOR, &caps(0, 22, 23, 24, 25, 26))
                .1
                .format,
            24
        );
        assert_eq!(
            select_create_format(ITC_TRUECOLOR, &caps(0, 22, 23, 24, 25, 26)).0 & ITC_TRUECOLOR,
            0
        );
        assert_eq!(
            select_create_format(ITC_ALPHA | ITC_USE_ALPHA, &full).1,
            SelectedCreateFormat {
                format: 22,
                dxt: 0,
                dxt_format: 0
            }
        );
        assert_eq!(
            select_create_format(ITC_ALPHA | ITC_USE_ALPHA | ITC_DXT, &full).1,
            SelectedCreateFormat {
                format: 26,
                dxt: 3,
                dxt_format: 22
            }
        );
        assert_eq!(
            select_create_format(ITC_ALPHA | ITC_DXT, &caps(21, 22, 23, 24, 25, 0))
                .1
                .format,
            22
        );
        assert_eq!(select_create_format(ITC_COLORKEY, &full).1.format, 23);
        assert_eq!(
            select_create_format(ITC_COLORKEY | ITC_DXT, &full).1,
            SelectedCreateFormat {
                format: 25,
                dxt: 2,
                dxt_format: 23
            }
        );
        assert_eq!(
            select_create_format(ITC_COLORKEY | ITC_MIPMAP | ITC_DXT, &full).1,
            SelectedCreateFormat {
                format: 26,
                dxt: 3,
                dxt_format: 22
            }
        );
        assert_eq!(
            select_create_format(ITC_COLORKEY | ITC_MIPMAP, &caps(21, 22, 23, 24, 0, 0))
                .1
                .format,
            22
        );
        assert_eq!(
            select_create_format(ITC_DXT, &full).1,
            SelectedCreateFormat {
                format: 25,
                dxt: 1,
                dxt_format: 23
            }
        );
        assert_eq!(
            select_create_format(ITC_DXT, &caps(21, 22, 23, 24, 0, 0))
                .1
                .format,
            24
        );
        assert_eq!(select_create_format(0, &full).1.format, 24);
    }
    #[test]
    fn texture_size_matches_native_macro() {
        assert_eq!(create_texture_size(100, 100, 1024, 1024, false), (128, 128));
        assert_eq!(create_texture_size(3, 5, 1024, 1024, false), (4, 8));
        assert_eq!(create_texture_size(25, 25, 1024, 1024, false), (32, 32));
        assert_eq!(
            create_texture_size(2048, 512, 1024, 1024, false),
            (1024, 256)
        );
        assert_eq!(create_texture_size(3, 3, 1024, 1024, true), (4, 4));
        assert_eq!(create_texture_size(0, 0, 1024, 1024, false), (1, 1));
        assert_eq!(
            create_texture_size(1024, 1024, 1024, 1024, false),
            (1024, 1024)
        );
        assert_eq!(create_texture_size(129, 129, 1024, 1024, false), (256, 256));
    }
    #[test]
    fn mipmap_filter_matches_native_weighting() {
        // Hand-computed: c00=100 c01=104 c10=108 c11=112 ->
        // (112>>3)+(100>>3)+(324>>3)+(312>>3) = 14+12+40+39 = 105.
        let parent = [100u8, 0, 0, 0, 104, 0, 0, 0, 108, 0, 0, 0, 112, 0, 0, 0];
        let mut dst = [0u8; 16];
        assert_eq!(
            filter_mipmap_level(&parent, &mut dst, 2, 2, 8).unwrap(),
            (1, 1, 4)
        );
        assert_eq!(&dst[..4], &[105, 0, 0, 0]);
        let parent = [64u8; 64];
        let mut dst = [0u8; 64];
        assert_eq!(
            filter_mipmap_level(&parent, &mut dst, 4, 4, 16).unwrap(),
            (2, 2, 8)
        );
        assert_eq!(&dst[..16], &[64u8; 16]);
    }
    #[test]
    fn alpha_brightness_and_filenames_match_native_layout() {
        assert_eq!(alpha_brightness(80, 40, 20), 50);
        assert_eq!(alpha_brightness(0, 0, 0), 0);
        assert_eq!(alpha_brightness(255, 255, 255), 255);
        assert_eq!(
            presaved_dxt_filename(b"maps\\wall.tga", None, 4, false, 1).unwrap(),
            b"maps\\wall_4.DX1"
        );
        assert_eq!(
            presaved_dxt_filename(b"maps\\wall.tga", Some(b"alpha.tga"), 4, true, 3).unwrap(),
            b"maps\\wall_alpha_4L.DX3"
        );
        assert_eq!(
            presaved_dxt_filename(b"maps\\wall.tga", Some(b"alpha.tga"), 4, false, 2).unwrap(),
            b"maps\\wall_4.DX2"
        );
        assert_eq!(
            presaved_555_filename(b"maps\\wall.tga", 4, false, 23).unwrap(),
            b"maps\\wall_4.565"
        );
        assert_eq!(
            presaved_555_filename(b"maps\\wall.tga", 4, true, 24).unwrap(),
            b"maps\\wall_4L.555"
        );
        assert_eq!(create_level_byte_size(21, 4, 4).unwrap(), 64);
        assert_eq!(create_level_byte_size(D3DFMT_DXT1, 4, 4).unwrap(), 8);
        assert_eq!(create_level_byte_size(D3DFMT_DXT3, 5, 5).unwrap(), 64);
    }
    fn memory_bmp_24(width: u32, height: u32, rows_bottom_up: &[u8]) -> Vec<u8> {
        // Tightly packed rows stand in for the tested stride; padded strides
        // are covered by the NOALIGN unit below.
        let mut header = vec![0u8; 40];
        header[0..4].copy_from_slice(&40u32.to_le_bytes());
        header[4..8].copy_from_slice(&(width as i32).to_le_bytes());
        header[8..12].copy_from_slice(&(height as i32).to_le_bytes());
        header[12..14].copy_from_slice(&1u16.to_le_bytes());
        header[14..16].copy_from_slice(&24u16.to_le_bytes());
        let mut out = header;
        out.extend_from_slice(rows_bottom_up);
        out
    }
    #[test]
    fn synthetic_full_create_covers_file_memory_empty_and_error_words() {
        // File branch via the existing Source harness (2x2x32 top-origin TGA).
        let (mut r, mut b, mut s) =
            setup(32, &[3, 2, 1, 17, 6, 5, 4, 34, 9, 8, 7, 51, 12, 11, 10, 68]);
        let (key, report) = r
            .create_texture_full(
                Some(b"color.tga"),
                None,
                None,
                ITC_ALPHA,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b,
            )
            .unwrap();
        assert!(report.warnings.is_empty());
        let graphics = {
            let t = r.texture(key).unwrap();
            assert!(t.initialized);
            assert_eq!(
                (t.width, t.height),
                (Observed::Value(2), Observed::Value(2))
            );
            assert_eq!(t.format, Observed::Value(21));
            assert_eq!((t.flags, t.creation_flags), (ITC_ALPHA, ITC_ALPHA));
            assert_eq!(t.levels, Observed::Value(1));
            t.graphics.unwrap()
        };
        // Converted store is top-down like the sibling's: the loop reads
        // bottom-up-first, so dst[0] is the top row (file row 0 here).
        assert_eq!(
            b.surface(graphics).unwrap().bgra,
            [17, 1, 2, 3, 34, 4, 5, 6, 51, 7, 8, 9, 68, 10, 11, 12]
        );
        assert!(matches!(
            b.device_log(),
            [
                DeviceEvent::Created { .. },
                DeviceEvent::Uploaded { level: 0, .. },
                DeviceEvent::Preloaded { .. }
            ]
        ));
        // Cache hit owns a second reference without new effects.
        let log_len = b.device_log().len();
        let hit = r
            .create_texture_full(
                Some(b"COLOR.TGA"),
                None,
                None,
                ITC_ALPHA,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b,
            )
            .unwrap()
            .0;
        assert_eq!(hit, key);
        assert_eq!(b.device_log().len(), log_len);
        // Memory branch on the same bytes: INFOHEADER + rows (stride
        // recomputed, never trusted).
        let mut membuf = vec![0u8; 40];
        membuf[0..4].copy_from_slice(&40u32.to_le_bytes());
        membuf[4..8].copy_from_slice(&2i32.to_le_bytes());
        membuf[8..12].copy_from_slice(&2i32.to_le_bytes());
        membuf[12..14].copy_from_slice(&1u16.to_le_bytes());
        membuf[14..16].copy_from_slice(&32u16.to_le_bytes());
        // Memory branch on the same image: BMP rows run bottom-up, so the
        // buffer holds the file rows reversed (same pixels, both origins).
        membuf.extend_from_slice(&[9, 8, 7, 51, 12, 11, 10, 68, 3, 2, 1, 17, 6, 5, 4, 34]);
        let (mkey, _) = r
            .create_texture_full(
                None,
                Some(&membuf),
                None,
                ITC_FROM_MEMORY | ITC_ALPHA,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b,
            )
            .unwrap();
        let mt = r.texture(mkey).unwrap();
        assert_eq!(mt.flags & ITC_UNCACHED, ITC_UNCACHED);
        assert_eq!(r.uncached(), &[mkey]);
        assert_eq!(
            b.surface(mt.graphics.unwrap()).unwrap().bgra,
            b.surface(graphics).unwrap().bgra
        );
        // Empty branches: warning, fill bytes, uncached, no preload.
        let (ekey, ereport) = r
            .create_texture_full(
                None,
                None,
                None,
                0,
                64,
                32,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b,
            )
            .unwrap();
        assert_eq!(ereport.warnings, [CreateWarning::EmptyWithoutFlag]);
        assert_eq!(r.texture(ekey).unwrap().flags & ITC_EMPTY, ITC_EMPTY);
        let (fkey, _) = r
            .create_texture_full(
                None,
                None,
                None,
                ITC_EMPTY | ITC_FILL,
                4,
                4,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b,
            )
            .unwrap();
        assert_eq!(
            b.surface(r.texture(fkey).unwrap().graphics.unwrap())
                .unwrap()
                .bgra,
            [0u8; 64]
        );
        // Failures: words, residue-free, unpoisoned.
        assert_eq!(
            r.create_texture_full(
                Some(b"missing.tga"),
                None,
                None,
                0,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::Image(TextureImageError::Missing))
        );
        assert_eq!(
            TextureFactoryError::Image(TextureImageError::Missing).native_result_word(),
            0x80000001
        );
        assert_eq!(
            TextureFactoryError::BadTextureFormat.native_result_word(),
            0x80000003
        );
        assert_eq!(
            TextureFactoryError::InvalidParameters.native_result_word(),
            0x80000007
        );
        assert_eq!(
            TextureFactoryError::Unsupported("x").native_result_word(),
            u32::MAX
        );
        // Host-only image rejections never forge the FILECORRUPTED word.
        assert_eq!(
            TextureFactoryError::Image(TextureImageError::Unsupported).native_result_word(),
            u32::MAX
        );
        assert_eq!(
            TextureFactoryError::Image(TextureImageError::Backend("x".into())).native_result_word(),
            u32::MAX
        );
        assert_eq!(
            TextureFactoryError::AlphaImage(TextureImageError::Unsupported).native_result_word(),
            u32::MAX
        );
        // Native image failures keep their words.
        assert_eq!(
            TextureFactoryError::Image(TextureImageError::Corrupt).native_result_word(),
            0x8000000a
        );
        assert_eq!(
            TextureFactoryError::Image(TextureImageError::UnknownFormat).native_result_word(),
            0x8000000a
        );
        assert_eq!(
            TextureFactoryError::AlphaImage(TextureImageError::Missing).native_result_word(),
            0x8000000b
        );
        assert_eq!(
            r.create_texture_full(
                None,
                None,
                None,
                ITC_FROM_MEMORY,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::InvalidParameters)
        );
        assert_eq!(
            r.create_texture_full(
                Some(b"color.tga"),
                None,
                None,
                ITC_MANAGED,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::Unsupported("managed texture domain"))
        );
        assert_eq!(
            r.create_texture_full(
                Some(b"color.tga"),
                None,
                None,
                0x10,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::Unsupported("Create flags"))
        );
        b.set_device_available(false);
        assert_eq!(
            r.create_texture_full(
                Some(b"color.tga"),
                None,
                None,
                0x200000,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::Unsupported("device creation sink"))
        );
        b.set_device_available(true);
        r.release_texture(key, &mut b).unwrap();
        r.release_texture(hit, &mut b).unwrap();
        r.release_texture(mkey, &mut b).unwrap();
        r.release_texture(ekey, &mut b).unwrap();
        r.release_texture(fkey, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn synthetic_full_create_rejects_odd_depth_late_with_native_word() {
        // 4-bit BMP intake reaches conversion (device + presaved attempt 1
        // run first), then surfaces BADTEXTUREFORMAT with effects retained.
        let (mut r, mut b, mut s) = setup(32, &[0u8; 16]);
        s.files
            .insert(b"Maps\\odd.bmp".to_vec(), source_bmp(4, &[0u8; 64]));
        assert_eq!(
            r.create_texture_full(
                Some(b"odd.bmp"),
                None,
                None,
                0,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::BadTextureFormat)
        );
        assert_eq!(
            TextureFactoryError::BadTextureFormat.native_result_word(),
            0x80000003
        );
        assert_eq!(b.resident_bytes(), 0);
        // Compressed memory buffers fail at intake, before any device effect.
        let mut bad = memory_bmp_24(2, 2, &[0u8; 12]);
        bad[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            r.create_texture_full(
                None,
                Some(&bad),
                None,
                ITC_FROM_MEMORY,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::Image(TextureImageError::Corrupt))
        );
        assert_eq!(b.live_device_textures(), 0);
    }

    struct RealDtaHarness {
        installation: lhr_assets::Installation,
        names: BTreeMap<Vec<u8>, String>,
    }
    impl RealDtaHarness {
        fn open() -> Option<Self> {
            let root =
                std::env::var_os("LHR_ORIGINAL_INSTALLATION").map(std::path::PathBuf::from)?;
            let installation = match lhr_assets::Installation::open(&root) {
                Ok(installation) => installation,
                Err(error) => {
                    eprintln!("SKIP real-DTA Create tests: cannot open installation: {error:?}");
                    return None;
                }
            };
            let mut paths = match installation.asset_paths("maps") {
                Ok(paths) => paths,
                Err(error) => {
                    eprintln!("SKIP real-DTA Create tests: no maps catalog: {error:?}");
                    return None;
                }
            };
            paths.sort();
            let mut names = BTreeMap::new();
            let mut scanned = 0usize;
            for path in &paths {
                let lower = path.to_ascii_lowercase();
                if !(lower.ends_with(".tga") || lower.ends_with(".bmp")) {
                    continue;
                }
                if scanned >= 24 {
                    break;
                }
                scanned += 1;
                if installation.read(path).is_err() {
                    continue;
                }
                let base = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
                names
                    .entry(base.into_bytes())
                    .or_insert_with(|| path.clone());
            }
            Some(Self {
                installation,
                names,
            })
        }
    }
    struct DtaTextureSource<'a> {
        harness: &'a mut RealDtaHarness,
    }
    impl TextureImageSource for DtaTextureSource<'_> {
        fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            let text = std::str::from_utf8(path).map_err(|_| TextureImageError::UnknownFormat)?;
            let base = text
                .rsplit('\\')
                .next()
                .unwrap_or(text)
                .to_ascii_lowercase();
            let full = self
                .harness
                .names
                .get(base.as_bytes())
                .ok_or(TextureImageError::Missing)?;
            self.harness
                .installation
                .read(full)
                .map_err(|_| TextureImageError::Missing)
        }
    }
    /// Independently written oracle for the conversion rules over real
    /// decoded bytes (same native reference as the port, separate
    /// transcription; raw channel positions, bottom-up emit).
    fn expected_create_argb(rgba: &[u8], width: u32, height: u32, bpp: u16, flags: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(width as usize * height as usize * 4);
        for row in (0..height as usize).rev() {
            for col in 0..width as usize {
                let p = &rgba[(row * width as usize + col) * 4..][..4];
                let a = if matches!(bpp, 16 | 32) && flags & ITC_ALPHA != 0 {
                    p[3]
                } else if flags & ITC_COLORKEY != 0 && bpp != 8 {
                    if p[..3] == [0, 0, 0] {
                        0
                    } else {
                        0xff
                    }
                } else {
                    0xff
                };
                out.extend_from_slice(&[p[2], p[1], p[0], a]);
            }
        }
        out
    }
    #[test]
    fn real_dta_create_runs_file_memory_and_empty_branches() {
        let Some(mut harness) = RealDtaHarness::open() else {
            eprintln!("SKIP: LHR_ORIGINAL_INSTALLATION is not set; needs the original game data");
            return;
        };
        struct Payload {
            base: Vec<u8>,
            bytes: Vec<u8>,
            width: u32,
            height: u32,
            bpp: u16,
            bmp: bool,
        }
        let mut payloads = Vec::new();
        for (base, full) in harness.names.clone() {
            if payloads.len() >= 8 {
                break;
            }
            let Ok(bytes) = harness.installation.read(&full) else {
                continue;
            };
            if bytes.len() > 1_048_576 {
                continue;
            }
            let Ok(info) = lhr_assets::texture_source::inspect_texture_image(&bytes) else {
                continue;
            };
            let bmp = bytes.starts_with(b"BM");
            if bmp
                && (bytes.len() < 54 || i32::from_le_bytes(bytes[22..26].try_into().unwrap()) <= 0)
            {
                // Memory equivalence needs standard bottom-up BMPs.
                continue;
            }
            if info.width < 2 || info.width > 512 || info.height < 2 || info.height > 512 {
                continue;
            }
            if !info.width.is_power_of_two() || !info.height.is_power_of_two() {
                // Keeps the byte asserts resize-free (HALF covers scaling).
                continue;
            }
            if !matches!(info.bpp, 8 | 16 | 24 | 32) {
                continue;
            }
            payloads.push(Payload {
                base,
                bytes,
                width: info.width,
                height: info.height,
                bpp: info.bpp,
                bmp,
            });
        }
        if payloads.is_empty() {
            panic!(
                "LHR_ORIGINAL_INSTALLATION is set but no pow2 TGA/BMP payload found under maps/"
            );
        }
        for payload in &payloads {
            let name = std::str::from_utf8(&payload.base).unwrap();
            let depth_flags = if payload.bpp == 32 { ITC_ALPHA } else { 0 };
            // File branch, then cache-hit repeat on shared resources.
            let mut resources = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let (key, _) = {
                let mut source = DtaTextureSource {
                    harness: &mut harness,
                };
                match resources.create_texture_full(
                    Some(name.as_bytes()),
                    None,
                    None,
                    depth_flags,
                    0,
                    0,
                    0,
                    &CreateDeviceCaps::CPU,
                    &mut source,
                    &mut storage,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!(
                            "DBG payload={name} dims={}x{} bpp={} err={e:?}",
                            payload.width, payload.height, payload.bpp
                        );
                        panic!("first create failed");
                    }
                }
            };
            let (graphics, file_flags) = {
                let texture = resources.texture(key).unwrap();
                assert!(texture.initialized);
                assert_eq!(texture.width, Observed::Value(payload.width as u16));
                assert_eq!(texture.height, Observed::Value(payload.height as u16));
                assert_eq!(texture.format, Observed::Value(D3DFMT_A8R8G8B8));
                assert_eq!(texture.levels, Observed::Value(1));
                assert_eq!(texture.memory_size, payload.width * payload.height * 4);
                (texture.graphics.unwrap(), texture.flags)
            };
            let decoded = lhr_assets::texture_source::decode_texture_image(&payload.bytes).unwrap();
            // 8-bit sources quantize through the palette; the oracle below
            // covers non-8-bit depths, 8-bit asserts dims + order only.
            if payload.bpp != 8 {
                assert_eq!(
                    storage.surface(graphics).unwrap().bgra,
                    expected_create_argb(
                        &decoded.texture.rgba,
                        payload.width,
                        payload.height,
                        payload.bpp,
                        file_flags
                    )
                );
            }
            // ALPHA fixup split on real depths: 8/24-bit clears, 16-bit adds
            // COLORKEY, 32-bit keeps; creation_flags always retains the ask.
            {
                let mut source = DtaTextureSource {
                    harness: &mut harness,
                };
                let (akey, _) = resources
                    .create_texture_full(
                        Some(name.as_bytes()),
                        None,
                        None,
                        ITC_ALPHA,
                        0,
                        0,
                        0,
                        &CreateDeviceCaps::CPU,
                        &mut source,
                        &mut storage,
                    )
                    .unwrap();
                let alpha_tex = resources.texture(akey).unwrap();
                assert_eq!(alpha_tex.creation_flags, ITC_ALPHA);
                if payload.bpp == 32 {
                    assert_eq!(alpha_tex.flags, ITC_ALPHA);
                } else if payload.bpp == 16 {
                    assert_eq!(alpha_tex.flags, ITC_ALPHA | ITC_COLORKEY);
                } else {
                    assert_eq!(alpha_tex.flags, 0);
                }
                resources.release_texture(akey, &mut storage).unwrap();
            }
            // Cache-hit repeat shares the key and adds no device effects.
            let log_len = storage.device_log().len();
            let hit = {
                let mut source = DtaTextureSource {
                    harness: &mut harness,
                };
                resources
                    .create_texture_full(
                        Some(name.as_bytes()),
                        None,
                        None,
                        depth_flags,
                        0,
                        0,
                        0,
                        &CreateDeviceCaps::CPU,
                        &mut source,
                        &mut storage,
                    )
                    .unwrap()
                    .0
            };
            assert_eq!(hit, key);
            assert_eq!(storage.device_log().len(), log_len);
            // HALF scales through TEXTURE_SIZE on real bytes.
            {
                let mut source = DtaTextureSource {
                    harness: &mut harness,
                };
                let (hkey, _) = resources
                    .create_texture_full(
                        Some(name.as_bytes()),
                        None,
                        None,
                        depth_flags | ITC_HALF | ITC_UNCACHED,
                        0,
                        0,
                        0,
                        &CreateDeviceCaps::CPU,
                        &mut source,
                        &mut storage,
                    )
                    .unwrap();
                let half = resources.texture(hkey).unwrap();
                assert_eq!(half.width, Observed::Value(payload.width as u16 / 2));
                assert_eq!(half.height, Observed::Value(payload.height as u16 / 2));
                resources.release_texture(hkey, &mut storage).unwrap();
            }
            // Mip chain: full count, halving dims, upload per level. The
            // device log accumulates across this payload's creates, so the
            // upload count is measured as an increment.
            {
                let uploads_before = storage
                    .device_log()
                    .iter()
                    .filter(|e| matches!(e, DeviceEvent::Uploaded { .. }))
                    .count();
                let mut source = DtaTextureSource {
                    harness: &mut harness,
                };
                let (mkey, _) = resources
                    .create_texture_full(
                        Some(name.as_bytes()),
                        None,
                        None,
                        depth_flags | ITC_MIPMAP | ITC_UNCACHED,
                        0,
                        0,
                        0,
                        &CreateDeviceCaps::CPU,
                        &mut source,
                        &mut storage,
                    )
                    .unwrap();
                let mtex = resources.texture(mkey).unwrap();
                let mut chain = 1u32;
                let (mut w, mut h) = (payload.width, payload.height);
                while w > 1 || h > 1 {
                    w = (w >> 1).max(1);
                    h = (h >> 1).max(1);
                    chain += 1;
                }
                assert_eq!(mtex.levels, Observed::Value(chain as u16));
                let mgraphics = mtex.graphics.unwrap();
                let stored = storage.device_levels(mgraphics).unwrap();
                assert_eq!(stored.len() as u32, chain - 1);
                let (mut w, mut h) = (payload.width as u16 / 2, payload.height as u16 / 2);
                for level in stored {
                    assert_eq!((level.width, level.height), (w, h));
                    w = (w >> 1).max(1);
                    h = (h >> 1).max(1);
                }
                let uploads = storage
                    .device_log()
                    .iter()
                    .filter(|e| matches!(e, DeviceEvent::Uploaded { .. }))
                    .count();
                assert_eq!((uploads - uploads_before) as u32, chain);
                resources.release_texture(mkey, &mut storage).unwrap();
            }
            // File-vs-memory equivalence for BMP payloads: the file header is
            // a standard container, so bytes[14..] are exactly the native
            // memory layout (stride is recomputed, never trusted).
            if payload.bmp {
                let (mmkey, _) = {
                    let mut source = DtaTextureSource {
                        harness: &mut harness,
                    };
                    resources
                        .create_texture_full(
                            None,
                            Some(&payload.bytes[14..]),
                            None,
                            ITC_FROM_MEMORY | depth_flags,
                            0,
                            0,
                            0,
                            &CreateDeviceCaps::CPU,
                            &mut source,
                            &mut storage,
                        )
                        .unwrap()
                };
                let mem_tex = resources.texture(mmkey).unwrap();
                assert_eq!(mem_tex.creation_flags, ITC_FROM_MEMORY | depth_flags);
                assert_eq!(mem_tex.flags & ITC_UNCACHED, ITC_UNCACHED);
                assert_eq!(
                    storage.surface(mem_tex.graphics.unwrap()).unwrap().bgra,
                    storage.surface(graphics).unwrap().bgra
                );
                resources.release_texture(mmkey, &mut storage).unwrap();
            }
            resources.release_texture(key, &mut storage).unwrap();
            resources.release_texture(hit, &mut storage).unwrap();
            assert_eq!(storage.resident_bytes(), 0);
            assert!(!resources.is_poisoned());
        }
        // Empty + failure tails (no DTA payload needed, same session shape).
        let mut resources = TextureResources::new().unwrap();
        let mut storage = CpuTextureStorage::default();
        let mut source = DtaTextureSource {
            harness: &mut harness,
        };
        let (ekey, ereport) = resources
            .create_texture_full(
                None,
                None,
                None,
                ITC_EMPTY | ITC_FILL,
                4,
                4,
                0,
                &CreateDeviceCaps::CPU,
                &mut source,
                &mut storage,
            )
            .unwrap();
        assert_eq!(
            storage
                .surface(resources.texture(ekey).unwrap().graphics.unwrap())
                .unwrap()
                .bgra,
            [0u8; 64]
        );
        assert!(ereport.warnings.is_empty());
        assert!(matches!(
            storage.device_log(),
            [DeviceEvent::Created { .. }, DeviceEvent::Filled { .. }]
        ));
        assert_eq!(resources.texture(ekey).unwrap().memory_size, 64);
        resources.release_texture(ekey, &mut storage).unwrap();
        assert_eq!(
            resources.create_texture_full(
                Some(b"no-such-texture.tga"),
                None,
                None,
                0,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut source,
                &mut storage
            ),
            Err(TextureFactoryError::Image(TextureImageError::Missing))
        );
        assert_eq!(storage.resident_bytes(), 0);
        assert!(resources.cached().is_empty() && resources.uncached().is_empty());
        assert!(!resources.is_poisoned());
    }

    #[test]
    fn padded_bmp_alpha_uses_stride_aware_raw_rows() {
        // 2x2 24-bit alpha BMP with padded stride 8 (tight would be 6).
        // Brightness is read at raw B,G,R positions per row:
        // row0: (30,20,10)->22, (60,50,40)->52; row1: (90,80,70)->82, (120,110,100)->112.
        let mut alpha = vec![0u8; 54];
        alpha[..2].copy_from_slice(b"BM");
        alpha[10..14].copy_from_slice(&54u32.to_le_bytes());
        alpha[14..18].copy_from_slice(&40u32.to_le_bytes());
        alpha[18..22].copy_from_slice(&2i32.to_le_bytes());
        alpha[22..26].copy_from_slice(&2i32.to_le_bytes());
        alpha[26..28].copy_from_slice(&1u16.to_le_bytes());
        alpha[28..30].copy_from_slice(&24u16.to_le_bytes());
        alpha.extend_from_slice(&[
            10, 20, 30, 40, 50, 60, 0, 0, 70, 80, 90, 100, 110, 120, 0, 0,
        ]);
        // 2x2x24 tight main image (top-origin TGA for variety).
        let mut main = vec![0u8; 18];
        main[2] = 2;
        main[12..16].copy_from_slice(&[2, 0, 2, 0]);
        main[16] = 24;
        main.extend_from_slice(&[1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4]);
        let (mut r, mut b, mut s) = setup(32, &[0u8; 16]);
        s.files.insert(b"Maps\\main.tga".to_vec(), main);
        s.files.insert(b"Maps\\padalpha.bmp".to_vec(), alpha);
        let (key, _) = r
            .create_texture_full(
                Some(b"main.tga"),
                None,
                Some(b"padalpha.bmp"),
                ITC_ALPHA,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b,
            )
            .unwrap();
        // Bottom-up emit: the alpha plane is bottom-up like the rows, so
        // the bottom source row pairs with plane row 0 first.
        assert_eq!(
            b.surface(r.texture(key).unwrap().graphics.unwrap())
                .unwrap()
                .bgra,
            [22, 3, 3, 3, 52, 4, 4, 4, 82, 1, 1, 1, 112, 2, 2, 2]
        );
        r.release_texture(key, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
    }
    #[test]
    fn noalign_bmp_reads_tight_rows() {
        // 2x2x24 BMP stored tightly (66 B); without NOALIGN the padded
        // stride overruns into Corrupt, with NOALIGN it decodes.
        let mut tight = vec![0u8; 54];
        tight[..2].copy_from_slice(b"BM");
        tight[10..14].copy_from_slice(&54u32.to_le_bytes());
        tight[14..18].copy_from_slice(&40u32.to_le_bytes());
        tight[18..22].copy_from_slice(&2i32.to_le_bytes());
        tight[22..26].copy_from_slice(&2i32.to_le_bytes());
        tight[26..28].copy_from_slice(&1u16.to_le_bytes());
        tight[28..30].copy_from_slice(&24u16.to_le_bytes());
        tight.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        let (mut r, mut b, mut s) = setup(32, &[0u8; 16]);
        s.files.insert(b"Maps\\tight.bmp".to_vec(), tight);
        assert_eq!(
            r.create_texture_full(
                Some(b"tight.bmp"),
                None,
                None,
                0,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b
            ),
            Err(TextureFactoryError::Image(TextureImageError::Corrupt))
        );
        let (key, _) = r
            .create_texture_full(
                Some(b"tight.bmp"),
                None,
                None,
                ITC_NOALIGN,
                0,
                0,
                0,
                &CreateDeviceCaps::CPU,
                &mut s,
                &mut b,
            )
            .unwrap();
        // Bottom-up emit: last file row converts first, channels BGR->ARGB.
        assert_eq!(
            b.surface(r.texture(key).unwrap().graphics.unwrap())
                .unwrap()
                .bgra,
            [255, 9, 8, 7, 255, 12, 11, 10, 255, 3, 2, 1, 255, 6, 5, 4]
        );
        r.release_texture(key, &mut b).unwrap();
        assert_eq!(b.resident_bytes(), 0);
    }
}
