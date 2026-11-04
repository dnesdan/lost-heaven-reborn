//! Surface Draw/Preload/Fill/Compress boundary, LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6 RVAs
//! 0x787c0/0x78850/0x78860/0x78940/0x78b70/0x78cf0/0x7a380. Names are upstream
//! working names; contract key is SHA+RVA, never the name. No D3D types here:
//! every device call is an injected-sink callback in native order.
use super::{PixelAllocation, TextureGraphics, TextureKey};

/// Exact immediates observed (REA read_bytes + capstone decode cross-checked
/// against mafia-re ITexture.cpp), not host defaults.
pub const LOCK_FLAG_NO_DIRTY_UPDATE: u32 = 0x8000;
pub const LOCK_READONLY: u32 = 0x10;
pub const VERTEX_FORMAT_TLVERTEX: u32 = 4;
pub const TL_FVF: u32 = 0x1c4;
pub const DRAW_STRIDE_TLVERTEX: u32 = 0x20;
pub const SAMPLER_CLAMP: u32 = 3;
pub const SAMPLER_WRAP: u32 = 1;
pub const SAMPLER_U: u32 = 0x0d;
pub const SAMPLER_V: u32 = 0x0e;
pub const PRIM_TRIANGLESTRIP: u32 = 5;
pub const S3TC_RGB_FULL: u32 = 0x0;
pub const S3TC_RGB_ALPHA_COMPARE: u32 = 0x2;
pub const S3TC_ALPHA_EXPLICIT: u32 = 0x100;
/// Linear-luminance channel weights, bit-exact (0x3e9e00d2 etc.).
pub const S3TC_WEIGHTS: [f32; 3] = [0.3086, 0.6094, 0.0820];
/// DXT type positions on the native encode ladder (working-name enum
/// positions, not original symbols).
pub const COMPRESS_DXT1: i32 = 1;
pub const COMPRESS_DXT1_ALPHA: i32 = 2;
pub const COMPRESS_DXT3: i32 = 3;
/// The injected preload point: a zeroed TLVERTEX. The native buffer at
/// 0x100aad38 additionally carries rhw=1.0 (and z=0/specular=0) outside this
/// projection; the sink owns the full vertex encode and the test asserts the
/// buffer identity (sentinel), stride (32) and counts (1,1), not the bytes.
pub const PRELOAD_POINT: TlVertex = TlVertex {
    x: 0.0,
    y: 0.0,
    diffuse: 0,
    u: 0.0,
    v: 0.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
/// Projected TLVERTEX: only the fields Draw publishes (positions, diffuse,
/// u/v). z/rhw/specular stay sink-side; see PRELOAD_POINT.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TlVertex {
    pub x: f32,
    pub y: f32,
    pub diffuse: u32,
    pub u: f32,
    pub v: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceDesc {
    pub width: u32,
    pub height: u32,
    pub size_bytes: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockedMapping {
    pub mapping: u64,
    pub pitch: u32,
    pub writable_bytes: u32,
}

/// Injected host-surface sink covering Lock/AddDirtyRect/Fill/Compress level
/// ops. Serialized, non-reentrant; Err is an interruption, possibly after an
/// effect. `write_locked` projects the mapped-pointer stores: native writes
/// pixels in place between Lock and Unlock; the port computes the full buffer
/// and publishes it in one ordered call.
pub trait TextureSurfaceSink {
    type Error;
    fn get_surface_level(
        &mut self,
        graphics: TextureGraphics,
        level: u16,
    ) -> Result<Option<u64>, Self::Error>;
    fn lock_surface(
        &mut self,
        surface: u64,
        rect: Option<Rect>,
        flags: u32,
    ) -> Result<Option<LockedMapping>, Self::Error>;
    fn write_locked(&mut self, mapping: u64, bytes: &[u8]) -> Result<(), Self::Error>;
    fn unlock_surface(&mut self, surface: u64) -> Result<(), Self::Error>;
    fn release_surface(&mut self, surface: u64) -> Result<(), Self::Error>;
    fn add_dirty_rect(
        &mut self,
        graphics: TextureGraphics,
        rect: Option<Rect>,
    ) -> Result<i32, Self::Error>;
    fn surface_desc(
        &mut self,
        graphics: TextureGraphics,
        level: u16,
    ) -> Result<Option<SurfaceDesc>, Self::Error>;
}

/// Injected viewport/draw sink (IGraph boundary, no IGraph port).
/// Effect order is the contract; implementations must not reorder.
pub trait TextureDrawSink {
    type Error;
    fn scrn_size(&mut self) -> Result<(i32, i32), Self::Error>;
    fn intersect_rect(&mut self, dst: &mut Rect, a: &Rect, b: &Rect) -> Result<bool, Self::Error>;
    fn set_texture(&mut self, key: TextureKey) -> Result<(), Self::Error>;
    fn set_sampler(&mut self, stage: u32, kind: u32, value: u32) -> Result<(), Self::Error>;
    fn draw_strip(&mut self, prim: u32, nprim: u32, verts: &[TlVertex])
        -> Result<i32, Self::Error>;
    /// Preload-only raw path (binds the graphics handle, draws one point).
    fn scene_begun(&mut self) -> Result<bool, Self::Error>;
    fn set_tl_format(&mut self, fvf: u32) -> Result<(), Self::Error>;
    fn tl_format(&mut self) -> Result<u32, Self::Error>;
    fn bind_raw_graphics(&mut self, graphics: TextureGraphics) -> Result<(), Self::Error>;
    fn draw_point_up(&mut self, buf: &[TlVertex]) -> Result<(), Self::Error>;
    fn begin_scene(&mut self) -> Result<(), Self::Error>;
    fn end_scene(&mut self) -> Result<(), Self::Error>;
}

/// Injected S3TC encoder (vendored s3tc.lib stays out of tree per AGENTS.md).
pub trait S3tcEncodeSink {
    type Error;
    #[allow(clippy::too_many_arguments)]
    fn s3tc_encode(
        &mut self,
        src_bits: u64,
        src_pitch: u32,
        w: u32,
        h: u32,
        src_masks: [u32; 4],
        dst_bits: u64,
        encode: u32,
        weights: [f32; 3],
    ) -> Result<(), Self::Error>;
    fn debug_name(&mut self, text: Option<&str>) -> Result<(), Self::Error>;
    fn log_enabled(&mut self) -> Result<bool, Self::Error>;
}

/// Original `CompressTexture` free function, PC RVA 0x7a380. Order:
/// GetDesc, lock source (READONLY 0x10), lock dest, describe
/// (masks/encode by DXT type with the DXT1_ALPHA fallthrough sharing DXT1
/// masks), optional debug-name pair, `S3TCencode`, debug clear, unlock
/// source, unlock dest, true. Any lock miss unlocks what was acquired and
/// returns false. Masks for unknown types are zeroed (native leaves them
/// indeterminate; the host refuses to fabricate -- documented divergence).
/// `src_masks` for the encoder are selected here so the sink stays dumb.
#[allow(clippy::too_many_arguments)]
pub fn compress_texture<S>(
    sink: &mut S,
    graphics: TextureGraphics,
    desc: Option<SurfaceDesc>,
    encode_type: i32,
    src_pitch: u32,
    src_bits: u64,
    dst_bits: u64,
    log_name: Option<&str>,
) -> Result<bool, <S as TextureSurfaceSink>::Error>
where
    S: TextureSurfaceSink + S3tcEncodeSink<Error = <S as TextureSurfaceSink>::Error>,
{
    use COMPRESS_DXT1 as T1;
    use COMPRESS_DXT1_ALPHA as T1A;
    use COMPRESS_DXT3 as T3;
    let Some(_desc) = desc else {
        return Ok(false);
    };
    let Some(src_surface) = sink.get_surface_level(graphics, 0)? else {
        return Ok(false);
    };
    let Some(dst_surface) = sink.get_surface_level(graphics, 0)? else {
        return Ok(false);
    };
    let src = sink.lock_surface(src_surface, None, LOCK_READONLY)?;
    let Some(src) = src else {
        return Ok(false);
    };
    let dst = sink.lock_surface(dst_surface, None, 0)?;
    let Some(_dst) = dst else {
        sink.unlock_surface(src.mapping)?;
        return Ok(false);
    };
    let (encode, masks) = if encode_type == T1A {
        (S3TC_RGB_ALPHA_COMPARE, [0x7c00, 0x03e0, 0x001f, 0x8000])
    } else if encode_type == T1 {
        (S3TC_RGB_FULL, [0x7c00, 0x03e0, 0x001f, 0x8000])
    } else if encode_type == T3 {
        (S3TC_ALPHA_EXPLICIT, [0x0f00, 0x00f0, 0x000f, 0xf000])
    } else {
        (encode_type as u32, [0, 0, 0, 0])
    };
    if sink.log_enabled()? {
        sink.debug_name(log_name)?;
    }
    sink.s3tc_encode(
        src_bits,
        src_pitch,
        _desc.width,
        _desc.height,
        masks,
        dst_bits,
        encode,
        S3TC_WEIGHTS,
    )?;
    if sink.log_enabled()? {
        sink.debug_name(None)?;
    }
    sink.unlock_surface(src.mapping)?;
    sink.unlock_surface(_dst.mapping)?;
    Ok(true)
}

/// Device-coupled tail of ITexture::Create (LS3DF RVA 0x75f50). Pure intake,
/// flag decoding, format selection, ARGB conversion and mipmap filtering stay
/// on the port in texture_factory; every effect below crosses the existing
/// TextureGraphics/TextureSurface/PixelAllocation handle boundary. Supplied
/// handles transfer one owned reference each; the port adopts device textures
/// and kept pixel copies into the texture record or releases them before
/// returning, following install_texture discipline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceTextureDesc {
    pub width: u16,
    pub height: u16,
    pub requested_levels: u16,
    pub format: u32,
    pub mipmaps: bool,
    pub dxt: u8,
    pub dxt_format: u32,
}
/// One converted level ready for upload, in the selected surface `format`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UploadedCreateLevel<'a> {
    pub level: u16,
    pub width: u16,
    pub height: u16,
    pub bytes: &'a [u8],
    pub format: u32,
    pub dither: bool,
}
/// One presaved-file decision: the exact filename the original derives from
/// the source name, alpha basename, level count and ITC_HALF flag, plus the
/// texture dimensions a hit must match (ITexture.cpp:838-963).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresavedAttempt {
    pub filename: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub levels: u16,
    pub half: bool,
    pub dxt: u8,
    pub alpha_name: Option<Vec<u8>>,
}
/// Device-coupled Create tail. `None` rejects full creation before effects;
/// it is not a successful replacement. CPU-only storage supplies the
/// recording stand-in; real GPU backends supply upload/compression.
pub trait TextureCreationSink {
    type Error;
    /// Create the device texture and report the actual level count (native
    /// GetLevelCount after CreateTexture, ITexture.cpp:466-467).
    fn create_device_texture(
        &mut self,
        owner: TextureKey,
        desc: &DeviceTextureDesc,
    ) -> Result<(TextureGraphics, u16), Self::Error>;
    fn upload_create_level(
        &mut self,
        graphics: TextureGraphics,
        level: UploadedCreateLevel<'_>,
    ) -> Result<(), Self::Error>;
    /// S3TC compression of one level into the main surface (native
    /// CompressTexture, ITexture.cpp:98). The CPU stand-in records the call;
    /// compressed blocks require an S3TC backend (see OPEN).
    fn compress_create_level(
        &mut self,
        graphics: TextureGraphics,
        level: u16,
        dxt: u8,
    ) -> Result<(), Self::Error>;
    /// Retain the final ARGB image for ITC_KEEP_PIXELS (native m_pPixels,
    /// ITexture.cpp:822-825). Adopted or released by the port.
    fn keep_create_pixels(
        &mut self,
        owner: TextureKey,
        argb: Vec<u8>,
    ) -> Result<PixelAllocation, Self::Error>;
    /// Presaved loads (native m_TryPresavedDXT/555_565, RVAs 0x78190/0x78430).
    /// A hit skips conversion; the CPU stand-in always misses after recording
    /// the attempt in order (see OPEN).
    fn try_presaved_dxt(&mut self, attempt: &PresavedAttempt) -> Result<bool, Self::Error>;
    fn try_presaved_555_565(&mut self, attempt: &PresavedAttempt) -> Result<bool, Self::Error>;
    /// Presaved saves (native dtaOpenWrite tails, ITexture.cpp:838-963).
    /// The CPU stand-in records the decision without file effects (see OPEN).
    fn store_presaved_dxt(
        &mut self,
        graphics: TextureGraphics,
        attempt: &PresavedAttempt,
    ) -> Result<(), Self::Error>;
    fn store_presaved_555_565(
        &mut self,
        graphics: TextureGraphics,
        attempt: &PresavedAttempt,
    ) -> Result<(), Self::Error>;
    /// Driver upload kick (native Preload point draw, ITexture.cpp:1180-1200).
    fn preload_create_texture(&mut self, graphics: TextureGraphics) -> Result<(), Self::Error>;
    /// Whole-level clear for ITC_EMPTY|ITC_FILL (native Fill(NULL,0,0)).
    fn fill_empty_texture(
        &mut self,
        graphics: TextureGraphics,
        color: u32,
    ) -> Result<(), Self::Error>;
}
