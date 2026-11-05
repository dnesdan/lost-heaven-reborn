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

/// PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
/// LS3DF RVA 0x6ded0 `IGraph::GetFormatName`
/// (mafia-re ls3df/IGraph/IGraph.cpp:1337-1424, pinned at ab67b13).
/// Original `const char*` switch on `D3DFORMAT` as int, arms in the original
/// case order below (not enum order). Unknown values yield
/// `"UNSUPPORTED_FORMAT"`. Pure; no device access.
pub fn d3d_format_name(format: u32) -> &'static str {
    match format {
        0 => "D3DFMT_UNKNOWN",
        20 => "D3DFMT_R8G8B8",
        27 => "D3DFMT_R3G3B2",
        29 => "D3DFMT_A8R3G3B2",
        30 => "D3DFMT_X4R4G4B4",
        40 => "D3DFMT_A8P8",
        52 => "D3DFMT_A4L4",
        62 => "D3DFMT_X8L8V8U8",
        21 => "D3DFMT_A8R8G8B8",
        22 => "D3DFMT_X8R8G8B8",
        23 => "D3DFMT_R5G6B5",
        24 => "D3DFMT_X1R5G5B5",
        25 => "D3DFMT_A1R5G5B5",
        26 => "D3DFMT_A4R4G4B4",
        28 => "D3DFMT_A8",
        41 => "D3DFMT_P8",
        50 => "D3DFMT_L8",
        51 => "D3DFMT_A8L8",
        60 => "D3DFMT_V8U8",
        61 => "D3DFMT_L6V5U5",
        63 => "D3DFMT_Q8W8V8U8",
        65 => "D3DFMT_W11V11U10",
        71 => "D3DFMT_D32",
        73 => "D3DFMT_D15S1",
        70 => "D3DFMT_D16_LOCKABLE",
        77 => "D3DFMT_D24X8",
        79 => "D3DFMT_D24X4S4",
        64 => "D3DFMT_V16U16",
        75 => "D3DFMT_D24S8",
        80 => "D3DFMT_D16",
        100 => "D3DFMT_VERTEXDATA",
        101 => "D3DFMT_INDEX16",
        0x31545844 => "D3DFMT_DXT1",
        102 => "D3DFMT_INDEX32",
        0x32545844 => "D3DFMT_DXT2",
        0x32595559 => "D3DFMT_YUY2",
        0x33545844 => "D3DFMT_DXT3",
        0x35545844 => "D3DFMT_DXT5",
        0x34545844 => "D3DFMT_DXT4",
        0x59565955 => "D3DFMT_UYVY",
        0x7fffffff => "D3DFMT_FORCE_DWORD",
        _ => "UNSUPPORTED_FORMAT",
    }
}

/// PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
/// LS3DF RVA 0x706d0 `EncodeRLAChannel` (upstream working name, NOT original)
/// (mafia-re ls3df/IGraph/IGraph.cpp:2363-2406, pinned at ab67b13).
/// Run-length encodes one byte of each pixel, `(pixel & mask) >> shift`, as an
/// RLA channel: a run of n (2..127, capped at 127) equal bytes as `n-1` plus
/// the byte, else n (1..127, capped at 127) other bytes as `-n` (stored
/// modulo 256) plus the bytes. Returns the encoded length as `i16`
/// (`short`). The native `count` argument is folded into `src.len()`
/// (negative/short counts are caller-side rejected). Caller sizes `dst` (native writes blindly; the host panics on a
/// short buffer instead of corrupting memory).
pub fn encode_rla_channel(src: &[u32], dst: &mut [u8], mask: u32, shift: u32) -> i16 {
    let mut len: usize = 0;
    let mut left: usize = src.len();
    let mut inp: usize = 0;
    let mut outp: usize = 0;
    while left > 0 {
        let run = left > 2
            && ((src[inp] & mask) >> shift) == ((src[inp + 1] & mask) >> shift)
            && ((src[inp + 1] & mask) >> shift) == ((src[inp + 2] & mask) >> shift);
        if run {
            let mut n: usize = 2;
            while n < left {
                if ((src[inp + n] & mask) >> shift) != ((src[inp + n - 1] & mask) >> shift) {
                    break;
                }
                if n >= 127 {
                    break;
                }
                n += 1;
            }
            dst[outp] = (n - 1) as u8;
            outp += 1;
            dst[outp] = (((src[inp] & mask) >> shift) & 0xff) as u8;
            outp += 1;
            inp += n;
            len += 2;
            left -= n;
        } else {
            let mut n: usize = 1;
            while n < left {
                if left - n > 2
                    && ((src[inp + n] & mask) >> shift) == ((src[inp + n + 1] & mask) >> shift)
                    && ((src[inp + n + 1] & mask) >> shift) == ((src[inp + n + 2] & mask) >> shift)
                {
                    break;
                }
                if n >= 127 {
                    break;
                }
                n += 1;
            }
            // Low byte of the negated run length (matches the native
            // `-(n)` byte store exactly; `n <= 127` above, so the narrow
            // is exact and the wrap never traps).
            dst[outp] = 0u8.wrapping_sub(n as u8);
            outp += 1;
            len += 1;
            for _ in 0..n {
                dst[outp] = (((src[inp] & mask) >> shift) & 0xff) as u8;
                outp += 1;
                len += 1;
                inp += 1;
                left -= 1;
            }
        }
    }
    len as i16
}

fn float_to_int_rla(v: f32) -> i32 {
    v.round_ties_even() as i32
}

/// PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
/// LS3DF RVA 0x727d0 `IGraph::ResizeImage`
/// (mafia-re ls3df/IGraph/IGraph.cpp:3361-3537, pinned at ab67b13).
/// Scales a 24- (`bpp` 3) or 32-bit (`bpp` 4) image into `dst`. `pitch` and
/// `dst_pitch` are byte strides; `dst` is `None` for the native NULL (allocate
/// here) or `Some` for a caller buffer (native reuses `*dst` blindly).
/// With `filter` and a downscale the area each pixel covers is averaged;
/// otherwise the nearest pixel is taken. Binary32 source order throughout:
/// `(sx-1)/dst` ratios on the filter path, `sx/dst` on the nearest path,
/// `MRG_ZERO` is `1e-8f32`, `FloatToInt` is FISTP ties-even, empty areas yield
/// magenta (255,0,255,255). Only `bpp` 3/4 execute; other strides return false
/// (the original logs and proceeds: log policy withheld, host rejects).
/// Zero dims, short pitches, short buffers and allocation failure return false
/// (native would fault or wrap; host rejects).
#[allow(clippy::too_many_arguments)]
pub fn resize_image(
    sx: usize,
    sy: usize,
    pitch: usize,
    src: &[u8],
    dst_sx: usize,
    dst_sy: usize,
    dst_pitch: usize,
    dst: &mut Option<Vec<u8>>,
    bpp: usize,
    filter: bool,
) -> bool {
    const MRG_ZERO: f32 = 1e-8_f32;
    if bpp != 3 && bpp != 4 {
        return false;
    }
    if sx == 0 || sy == 0 || dst_sx == 0 || dst_sy == 0 {
        return false;
    }
    if dst_pitch < dst_sx.saturating_mul(bpp) {
        return false;
    }
    let src_need = (sy.saturating_sub(1))
        .saturating_mul(pitch)
        .saturating_add(sx.saturating_mul(bpp));
    if src.len() < src_need {
        return false;
    }
    let need_pitched = dst_sy.saturating_mul(dst_pitch);
    let need_tight = dst_sx.saturating_mul(dst_sy).saturating_mul(bpp);
    let need = need_pitched.max(need_tight);
    if need == 0 {
        return false;
    }
    if dst.is_none() {
        let mut v = Vec::new();
        if v.try_reserve_exact(need).is_err() {
            return false;
        }
        v.resize(need, 0);
        *dst = Some(v);
    }
    let buf = dst.as_mut().unwrap();
    if buf.len() < need {
        return false;
    }
    if filter && sx >= dst_sx && sy >= dst_sy {
        let x_ratio: f32 = (sx as f32 - 1.0) / dst_sx as f32;
        let y_ratio: f32 = (sy as f32 - 1.0) / dst_sy as f32;
        let mut dst_off: usize = 0;
        for y in 0..dst_sy {
            for x in 0..dst_sx {
                let fy0: f32 = y as f32 * y_ratio;
                let fy1: f32 = (y as f32 + 1.0) * y_ratio;
                let iy0: i32 = fy0 as i32 + 1;
                let iy1: i32 = fy1 as i32;
                let mut r: f32 = 0.0;
                let mut g: f32 = 0.0;
                let mut b: f32 = 0.0;
                let mut a: f32 = 0.0;
                let mut weight: f32 = 0.0;
                let (wy0, wy1): (f32, f32);
                if iy0 > iy1 {
                    wy0 = fy1 - fy0;
                    wy1 = 0.0;
                } else {
                    wy0 = iy0 as f32 - fy0;
                    wy1 = fy1 - iy1 as f32;
                }
                let rows: i32 = 0.max(iy1 - iy0);
                let fx0: f32 = x as f32 * x_ratio;
                let fx1: f32 = (x as f32 + 1.0) * x_ratio;
                let ix0: i32 = fx0 as i32 + 1;
                let ix1: i32 = fx1 as i32;
                let (wx0, wx1): (f32, f32);
                if ix0 > ix1 {
                    wx0 = fx1 - fx0;
                    wx1 = 0.0;
                } else {
                    wx0 = ix0 as f32 - fx0;
                    wx1 = fx1 - ix1 as f32;
                }
                let cols: i32 = 0.max(ix1 - ix0);
                if wy0 > MRG_ZERO {
                    let pos: usize = ((iy0 - 1) as usize) * pitch + ((ix0 - 1) as usize) * bpp;
                    if wx0 > MRG_ZERO {
                        r = src[pos] as f32 * wx0 * wy0;
                        g = src[pos + 1] as f32 * wx0 * wy0;
                        b = src[pos + 2] as f32 * wx0 * wy0;
                        if bpp == 4 {
                            a = src[pos + 3] as f32 * wx0 * wy0;
                        }
                        weight = wx0 * wy0;
                    }
                    if cols > 0 {
                        for i in 1..=cols {
                            let o: usize = pos + (i as usize) * bpp;
                            r += src[o] as f32 * wy0;
                            g += src[o + 1] as f32 * wy0;
                            b += src[o + 2] as f32 * wy0;
                            if bpp == 4 {
                                a += src[o + 3] as f32 * wy0;
                            }
                        }
                        weight += cols as f32 * wy0;
                    }
                    if wx1 > MRG_ZERO {
                        let o: usize = pos + ((cols + 1) as usize) * bpp;
                        r += src[o] as f32 * wx1 * wy0;
                        g += src[o + 1] as f32 * wx1 * wy0;
                        b += src[o + 2] as f32 * wx1 * wy0;
                        if bpp == 4 {
                            a += src[o + 3] as f32 * wx1 * wy0;
                        }
                        weight += wx1 * wy0;
                    }
                }
                if rows > 0 {
                    if wx0 > MRG_ZERO {
                        for j in 0..rows {
                            let pos: usize =
                                ((iy0 + j) as usize) * pitch + ((ix0 - 1) as usize) * bpp;
                            r += src[pos] as f32 * wx0;
                            g += src[pos + 1] as f32 * wx0;
                            b += src[pos + 2] as f32 * wx0;
                            if bpp == 4 {
                                a += src[pos + 3] as f32 * wx0;
                            }
                        }
                        weight += rows as f32 * wx0;
                    }
                    if cols > 0 {
                        // Faithful original quirk (IGraph.cpp:3451-3456): pos
                        // already adds `i`, then the access adds `i` again,
                        // reading into pitch padding. The host panics past the
                        // slice where native reads heap garbage (disclosed).
                        for j in 0..rows {
                            for i in 0..cols {
                                let pos: usize =
                                    ((iy0 + j) as usize) * pitch + ((ix0 + i) as usize) * bpp;
                                let o: usize = pos + (i as usize) * bpp;
                                r += src[o] as f32;
                                g += src[o + 1] as f32;
                                b += src[o + 2] as f32;
                                if bpp == 4 {
                                    a += src[o + 3] as f32;
                                }
                            }
                        }
                        weight += (cols * rows) as f32;
                    }
                    if wx1 > MRG_ZERO {
                        for j in 0..rows {
                            let pos: usize =
                                ((iy0 + j) as usize) * pitch + ((ix0 + cols) as usize) * bpp;
                            r += src[pos] as f32 * wx1;
                            g += src[pos + 1] as f32 * wx1;
                            b += src[pos + 2] as f32 * wx1;
                            if bpp == 4 {
                                a += src[pos + 3] as f32 * wx1;
                            }
                        }
                        weight += rows as f32 * wx1;
                    }
                }
                if wy1 > MRG_ZERO {
                    let pos: usize = ((iy0 + rows) as usize) * pitch + ((ix0 - 1) as usize) * bpp;
                    if wx0 > MRG_ZERO {
                        r += src[pos] as f32 * wx0 * wy1;
                        g += src[pos + 1] as f32 * wx0 * wy1;
                        b += src[pos + 2] as f32 * wx0 * wy1;
                        if bpp == 4 {
                            a += src[pos + 3] as f32 * wx0 * wy1;
                        }
                        weight += wx0 * wy1;
                    }
                    if cols > 0 {
                        for i in 1..=cols {
                            let o: usize = pos + (i as usize) * bpp;
                            r += src[o] as f32 * wy1;
                            g += src[o + 1] as f32 * wy1;
                            b += src[o + 2] as f32 * wy1;
                            if bpp == 4 {
                                a += src[o + 3] as f32 * wy1;
                            }
                        }
                        weight += cols as f32 * wy1;
                    }
                    if wx1 > MRG_ZERO {
                        let o: usize = pos + ((cols + 1) as usize) * bpp;
                        r += src[o] as f32 * wx1 * wy1;
                        g += src[o + 1] as f32 * wx1 * wy1;
                        b += src[o + 2] as f32 * wx1 * wy1;
                        if bpp == 4 {
                            a += src[o + 3] as f32 * wx1 * wy1;
                        }
                        weight += wx1 * wy1;
                    }
                }
                if weight <= MRG_ZERO {
                    r = 255.0;
                    g = 0.0;
                    b = 255.0;
                    a = 255.0;
                } else {
                    let k: f32 = 1.0 / weight;
                    r *= k;
                    g *= k;
                    b *= k;
                    a *= k;
                }
                buf[dst_off] = float_to_int_rla(r) as u8;
                buf[dst_off + 1] = float_to_int_rla(g) as u8;
                buf[dst_off + 2] = float_to_int_rla(b) as u8;
                if bpp == 4 {
                    buf[dst_off + 3] = float_to_int_rla(a) as u8;
                }
                dst_off += bpp;
            }
            dst_off += dst_pitch - dst_sx * bpp;
        }
    } else {
        let x_ratio: f32 = sx as f32 / dst_sx as f32;
        let y_ratio: f32 = sy as f32 / dst_sy as f32;
        let mut dst_off: usize = 0;
        for y in 0..dst_sy {
            for x in 0..dst_sx {
                let sy0: usize = (y as f32 * y_ratio) as usize;
                let sx0: usize = (x as f32 * x_ratio) as usize;
                let s: usize = sy0 * pitch + sx0 * bpp;
                buf[dst_off..dst_off + bpp].copy_from_slice(&src[s..s + bpp]);
                dst_off += bpp;
            }
            dst_off += dst_pitch - dst_sx * bpp;
        }
    }
    true
}

#[cfg(test)]
mod texture_surface_cpu_tests {
    use super::*;
    #[test]
    fn format_name_keeps_original_case_order() {
        assert_eq!(d3d_format_name(0), "D3DFMT_UNKNOWN");
        assert_eq!(d3d_format_name(21), "D3DFMT_A8R8G8B8");
        assert_eq!(d3d_format_name(23), "D3DFMT_R5G6B5");
        assert_eq!(d3d_format_name(0x31545844), "D3DFMT_DXT1");
        assert_eq!(d3d_format_name(0x33545844), "D3DFMT_DXT3");
        assert_eq!(d3d_format_name(0x7fffffff), "D3DFMT_FORCE_DWORD");
        assert_eq!(d3d_format_name(9999), "UNSUPPORTED_FORMAT");
        assert_eq!(d3d_format_name(70), "D3DFMT_D16_LOCKABLE");
        assert_eq!(d3d_format_name(101), "D3DFMT_INDEX16");
    }
    #[test]
    fn rla_run_and_literal_round_trip() {
        let src = [10u32, 10, 10, 20, 30];
        let mut dst = [0u8; 16];
        let len = encode_rla_channel(&src, &mut dst, 0xff, 0);
        assert_eq!(len, 5);
        // Run of 3 -> [n-1, byte]; 2 literals -> [-2, bytes].
        assert_eq!(dst[..5].to_vec(), vec![2u8, 10, 254u8, 20, 30]);
        let src2 = [7u32, 7, 7, 7];
        let mut dst2 = [0u8; 16];
        let len2 = encode_rla_channel(&src2, &mut dst2, 0xff, 0);
        assert_eq!(len2, 2);
        assert_eq!(dst2[..2].to_vec(), vec![3u8, 7]);
        let mut dst3 = [0u8; 16];
        let len3 = encode_rla_channel(&[1u32, 2, 3], &mut dst3, 0xff00, 8);
        // All three masked values are 0 with left=3 > 2: run arm ([n-1, byte]).
        assert_eq!(len3, 2);
        assert_eq!(dst3[..2].to_vec(), vec![2u8, 0]);
    }
    #[test]
    fn resize_nearest_copies_exact_pixels() {
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let mut out: Option<Vec<u8>> = None;
        assert!(resize_image(2, 2, 6, &src, 2, 2, 6, &mut out, 3, false));
        assert_eq!(out.unwrap(), src.to_vec());
        let mut out2: Option<Vec<u8>> = None;
        assert!(resize_image(2, 2, 6, &src, 1, 1, 3, &mut out2, 3, false));
        assert_eq!(out2.unwrap(), vec![1u8, 2, 3]);
    }
    #[test]
    fn resize_filter_downscale_averages_and_rejects_stride() {
        let src = [10u8, 20, 30, 10, 20, 30, 10, 20, 30, 10, 20, 30];
        let mut out: Option<Vec<u8>> = None;
        assert!(resize_image(2, 2, 6, &src, 1, 1, 3, &mut out, 3, true));
        assert_eq!(out.unwrap(), vec![10u8, 20, 30]);
        assert!(!resize_image(2, 2, 6, &src, 1, 1, 3, &mut None, 2, true));
        assert!(!resize_image(0, 2, 6, &src, 1, 1, 3, &mut None, 3, false));
    }
}
