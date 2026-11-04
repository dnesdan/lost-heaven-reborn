//! LoadImage free function (LS3DF RVA 0x79150) and m_Convert method
//! (LS3DF RVA 0x798c0). PC LS3DF.dll SHA-256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! LoadImage/m_Convert are upstream working names with PC correspondence
//! (callers into 0x79150; 10 incoming into 0x798c0); the provider names them
//! FUN_*. REA ev_43afedf1 (1895 B, 602 asm, 89 BB, 10 callees incl.
//! dtaOpen/dtaRead/dtaClose/dtaSeek/dtaGetTime thunks) and ev_4a7eac23
//! (2682 B, 863 asm, 89 BB, jump table at 0x1007a33c). No unsafe. Widths from
//! x86 immediates, not host aliases.
use super::factory::{ITC_DITHER, ITC_NOALIGN};
use super::{TextureKey, TextureResources};
use crate::itexture::D3DFMT_A1R5G5B5;
use crate::itexture::D3DFMT_A4R4G4B4;
use crate::itexture::D3DFMT_A8R8G8B8;
use crate::itexture::D3DFMT_L8;
use crate::itexture::D3DFMT_R5G6B5;
use crate::itexture::D3DFMT_X1R5G5B5;

const DTA_INVALID_HANDLE: i32 = -1;
const BITMAPINFO_LEN: usize = 40;
const TGA_HEADER_LEN: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitmapInfo {
    pub width: u32,
    pub height: u32,
    /// biBitCount WORD (SHR 3 source for byte depth).
    pub bpp: u16,
    pub clr_used: u32,
    /// biXPelsPerMeter LONG reused as pitch.
    pub pitch: u32,
    pub size_image: u32,
}

#[derive(Debug)]
pub struct LoadedImage {
    /// Bottom-up rows at the (possibly padded) pitch.
    pub bits: Vec<u8>,
    /// 1024 B RGBQUAD palette, or None.
    pub palette: Option<Vec<u8>>,
    pub info: BitmapInfo,
    /// dtaGetTime passthrough supplied by the caller.
    pub file_time: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadImageError {
    UnknownExtension,
    OpenFailed,
    ReadFailed,
    UnsupportedCompression,
    HostAllocation,
    InvalidParameters,
}

/// Typed DTA/file view for the live path. `read` performs one exact-size
/// native `dtaRead`; short data is `None` (native `<=0`/short branches).
/// Borrows live handles; reads when the original does.
pub trait LoadImageBackend {
    fn open(&mut self, name: &[u8]) -> i32;
    fn get_time(&mut self, handle: i32) -> Option<u64>;
    fn read(&mut self, handle: i32, out: &mut [u8]) -> Option<u32>;
    fn seek(&mut self, handle: i32, off: u32);
    fn close(&mut self, handle: i32);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bmp,
    Tga,
}

fn ext_kind(name: &[u8]) -> Option<Kind> {
    // Last-dot wins (strchr loop takes the final extension).
    let dot = name.iter().rposition(|b| *b == b'.')?;
    match name.get(dot + 1)?.to_ascii_lowercase() {
        b't' => Some(Kind::Tga),
        b'b' => Some(Kind::Bmp),
        _ => None,
    }
}

/// Free function: file-or-memory TGA/BMP load with sx/sy nearest scaling and
/// vertical flip. `file_bytes` is the DTA-selected copy (never falls back to
/// another provider); `memory` is the from_memory BITMAPINFOHEADER+palette
/// image. The backend path mirrors the native dtaOpen/dtaRead/dtaClose order
/// (including close-on-read-failure and the flip-alloc leak); the byte paths
/// never touch the backend.
#[allow(clippy::too_many_arguments)]
pub fn load_image(
    name: &[u8],
    sx: u32,
    sy: u32,
    flags: u32,
    from_memory: bool,
    memory: Option<&[u8]>,
    file_bytes: Option<&[u8]>,
    file_time: Option<u64>,
    handle: i32,
    backend: Option<&mut dyn LoadImageBackend>,
) -> Result<LoadedImage, LoadImageError> {
    let kind: Kind;
    let mut mem: &[u8] = &[];
    let mut file: &[u8] = &[];
    let mut live: Option<(i32, &mut dyn LoadImageBackend)> = None;
    if from_memory {
        kind = Kind::Bmp;
        mem = memory.ok_or(LoadImageError::InvalidParameters)?;
    } else {
        kind = ext_kind(name).ok_or(LoadImageError::UnknownExtension)?;
        if let Some(bytes) = file_bytes {
            file = bytes;
        } else if let Some(be) = backend {
            let h = if handle == DTA_INVALID_HANDLE {
                let h = be.open(name);
                if h == DTA_INVALID_HANDLE {
                    return Err(LoadImageError::OpenFailed);
                }
                h
            } else {
                handle
            };
            live = Some((h, be));
        } else if handle == DTA_INVALID_HANDLE {
            return Err(LoadImageError::OpenFailed);
        } else {
            return Err(LoadImageError::InvalidParameters);
        }
    }
    // Header parse.
    let mut live_time: Option<u64> = None;
    let (width, height, bpp, clr_used, flip) = if from_memory {
        let info = mem
            .get(..BITMAPINFO_LEN)
            .ok_or(LoadImageError::ReadFailed)?;
        mem = mem.get(BITMAPINFO_LEN..).unwrap_or(&[]);
        (
            u32::from_le_bytes(info[4..8].try_into().unwrap()),
            u32::from_le_bytes(info[8..12].try_into().unwrap()),
            u16::from_le_bytes([info[14], info[15]]),
            u32::from_le_bytes(info[32..36].try_into().unwrap()),
            false,
        )
    } else if file_bytes.is_some() || live.is_none() {
        parse_file_header(kind, &mut file)?
    } else {
        // Live backend path: exact-size reads in native order. The backend
        // time fills the caller's out-param (native dtaGetTime into `time`).
        let (h, be) = live.as_mut().map(|(h, be)| (*h, &mut **be)).unwrap();
        live_time = be.get_time(h);
        read_file_header(kind, h, be)?
    };
    // Backend time shadows the caller-supplied default (native out-param).
    let file_time = live_time.or(file_time);
    if kind == Kind::Bmp && bpp % 8 != 0 {
        // biCompression/bpp sanity lives in the header probe; non-byte
        // depths cannot address rows below.
        return Err(LoadImageError::UnsupportedCompression);
    }
    // Defense-only dimension caps mirroring the factory memory path
    // (8192/16M pixels): native size math wraps and reads out of bounds;
    // the host rejects instead of trapping on huge vec allocations.
    if width > 8192 || height > 8192 || u64::from(width) * u64::from(height) > 16_777_216 {
        return Err(LoadImageError::HostAllocation);
    }
    let bpp_bytes = (bpp / 8) as u32;
    // Pitch rule (TEST 0x40000; dword-aligned else +4). TGA keeps the tight
    // pitch derived in its header probe (bpp_bytes * width).
    let pitch = if kind == Kind::Bmp {
        let row = bpp_bytes
            .checked_mul(width)
            .ok_or(LoadImageError::ReadFailed)?;
        let dwords = row / 4;
        if flags & ITC_NOALIGN != 0 || dwords * 4 == row {
            row
        } else {
            dwords
                .checked_add(1)
                .and_then(|q| q.checked_mul(4))
                .ok_or(LoadImageError::ReadFailed)?
        }
    } else {
        bpp_bytes
            .checked_mul(width)
            .ok_or(LoadImageError::ReadFailed)?
    };
    let mut clr = clr_used;
    if clr == 0 && bpp <= 8 {
        clr = 1u32
            .checked_shl(bpp.into())
            .ok_or(LoadImageError::ReadFailed)?;
    }
    // Palette (1024 B zeroed + exact fill).
    let mut palette = vec![0u8; 1024];
    if clr != 0 {
        if from_memory {
            let need = (clr as usize)
                .checked_mul(4)
                .ok_or(LoadImageError::ReadFailed)?;
            let take = mem.get(..need).ok_or(LoadImageError::ReadFailed)?;
            palette[..need].copy_from_slice(take);
            mem = mem.get(need..).unwrap_or(&[]);
        } else if file_bytes.is_some() || live.is_none() {
            fill_palette_bytes(kind, &mut palette, clr, &mut file)?;
        } else {
            let (h, be) = live.as_mut().map(|(h, be)| (*h, &mut **be)).unwrap();
            fill_palette_live(kind, &mut palette, clr, h, be)?;
        }
    }
    let palette = if clr != 0 { Some(palette) } else { None };
    let size = pitch
        .checked_mul(height)
        .ok_or(LoadImageError::ReadFailed)? as usize;
    // Pixels (fallible reserve: native OOMs here; the host reports it).
    let mut bits = Vec::new();
    bits.try_reserve_exact(size)
        .map_err(|_| LoadImageError::HostAllocation)?;
    bits.resize(size, 0);
    if from_memory {
        let take = mem.get(..size).ok_or(LoadImageError::ReadFailed)?;
        bits.copy_from_slice(take);
    } else if file_bytes.is_some() || live.is_none() {
        let take = file.get(..size).ok_or(LoadImageError::ReadFailed)?;
        bits.copy_from_slice(take);
    } else {
        let (h, be) = live.as_mut().map(|(h, be)| (*h, &mut **be)).unwrap();
        if be.read(h, &mut bits) != Some(size as u32) {
            be.close(h);
            return Err(LoadImageError::ReadFailed);
        }
        be.close(h);
    }
    // Vertical flip (triple REP MOVS row swap, h/2 pairs).
    if flip {
        let mut rowbuf = Vec::new();
        rowbuf
            .try_reserve_exact(pitch as usize)
            .map_err(|_| LoadImageError::HostAllocation)?;
        rowbuf.resize(pitch as usize, 0);
        let (mut top, mut bottom) = (0usize, (height as usize - 1) * pitch as usize);
        for _ in 0..height as usize / 2 {
            rowbuf.copy_from_slice(&bits[top..top + pitch as usize]);
            bits.copy_within(bottom..bottom + pitch as usize, top);
            bits[bottom..bottom + pitch as usize].copy_from_slice(&rowbuf);
            top += pitch as usize;
            bottom -= pitch as usize;
        }
    }
    let info = BitmapInfo {
        width,
        height,
        bpp,
        clr_used: clr,
        pitch,
        size_image: size as u32,
    };
    if sx == 0 || sy == 0 || (sx == width && sy == height) {
        return Ok(LoadedImage {
            bits,
            palette,
            info,
            file_time,
        });
    }
    // Nearest scale (FILD/FIDIV floats, __ftol truncation toward zero).
    // Scale dims share the defense caps (native wraps; host rejects).
    if sx > 8192 || sy > 8192 || u64::from(sx) * u64::from(sy) > 16_777_216 {
        return Err(LoadImageError::HostAllocation);
    }
    let scale_x = width as f32 / sx as f32;
    let scale_y = height as f32 / sy as f32;
    let row = bpp_bytes
        .checked_mul(sx)
        .ok_or(LoadImageError::ReadFailed)? as usize;
    let new_size = row
        .checked_mul(sy as usize)
        .ok_or(LoadImageError::ReadFailed)?;
    let mut scaled = Vec::new();
    scaled
        .try_reserve_exact(new_size)
        .map_err(|_| LoadImageError::HostAllocation)?;
    scaled.resize(new_size, 0);
    for y in 0..sy as usize {
        for x in 0..sx as usize {
            let sx0 = (x as f32 * scale_x) as usize;
            let sy0 = (y as f32 * scale_y) as usize;
            let src = sy0 * pitch as usize + sx0 * bpp_bytes as usize;
            let dst = y * row + x * bpp_bytes as usize;
            scaled[dst..dst + bpp_bytes as usize]
                .copy_from_slice(&bits[src..src + bpp_bytes as usize]);
        }
    }
    let info = BitmapInfo {
        width: sx,
        height: sy,
        size_image: new_size as u32,
        pitch: row as u32,
        ..info
    };
    Ok(LoadedImage {
        bits: scaled,
        palette,
        info,
        file_time,
    })
}

fn parse_file_header(
    kind: Kind,
    file: &mut &[u8],
) -> Result<(u32, u32, u16, u32, bool), LoadImageError> {
    match kind {
        Kind::Bmp => {
            // 0x0E file header (size-checked) + 0x28 info; biCompression must
            // be BI_RGB (native reads on regardless, then refuses).
            file.get(..14).ok_or(LoadImageError::ReadFailed)?;
            *file = file.get(14..).unwrap_or(&[]);
            let info = file
                .get(..BITMAPINFO_LEN)
                .ok_or(LoadImageError::ReadFailed)?;
            *file = file.get(BITMAPINFO_LEN..).unwrap_or(&[]);
            let compression = u32::from_le_bytes(info[16..20].try_into().unwrap());
            if compression != 0 {
                return Err(LoadImageError::UnsupportedCompression);
            }
            Ok((
                u32::from_le_bytes(info[4..8].try_into().unwrap()),
                u32::from_le_bytes(info[8..12].try_into().unwrap()),
                u16::from_le_bytes([info[14], info[15]]),
                u32::from_le_bytes(info[32..36].try_into().unwrap()),
                false,
            ))
        }
        Kind::Tga => {
            // 0x12 TGA header; id_length skipped; 0x1000 warns only (never
            // reversed); 0x2000 sets the flip.
            let hdr = file
                .get(..TGA_HEADER_LEN)
                .ok_or(LoadImageError::ReadFailed)?;
            let id = hdr[0] as usize;
            let desc = hdr[17];
            let width = u16::from_le_bytes([hdr[12], hdr[13]]) as u32;
            let height = u16::from_le_bytes([hdr[14], hdr[15]]) as u32;
            let bpp = hdr[16] as u16;
            *file = file.get(TGA_HEADER_LEN + id..).unwrap_or(&[]);
            Ok((width, height, bpp, 0, desc & 0x20 != 0))
        }
    }
}

fn fill_palette_bytes(
    kind: Kind,
    palette: &mut [u8],
    clr: u32,
    file: &mut &[u8],
) -> Result<(), LoadImageError> {
    match kind {
        Kind::Bmp => {
            let need = (clr as usize)
                .checked_mul(4)
                .ok_or(LoadImageError::ReadFailed)?;
            let take = file.get(..need).ok_or(LoadImageError::ReadFailed)?;
            palette.get_mut(..need).ok_or(LoadImageError::ReadFailed)?[..need]
                .copy_from_slice(take);
            *file = file.get(need..).unwrap_or(&[]);
            Ok(())
        }
        Kind::Tga => {
            for i in 0..clr as usize {
                let take = file.get(..3).ok_or(LoadImageError::ReadFailed)?;
                palette[i * 4..i * 4 + 3].copy_from_slice(take);
                *file = file.get(3..).unwrap_or(&[]);
            }
            Ok(())
        }
    }
}

fn read_file_header(
    kind: Kind,
    handle: i32,
    be: &mut dyn LoadImageBackend,
) -> Result<(u32, u32, u16, u32, bool), LoadImageError> {
    // Exact-size reads in native order; any short read closes and fails.
    let fail = |be: &mut dyn LoadImageBackend| {
        be.close(handle);
        LoadImageError::ReadFailed
    };
    match kind {
        Kind::Bmp => {
            let mut fh = [0u8; 14];
            if be.read(handle, &mut fh) != Some(14) {
                return Err(fail(be));
            }
            let mut info = [0u8; 40];
            if be.read(handle, &mut info) != Some(40) {
                return Err(fail(be));
            }
            if u32::from_le_bytes(info[16..20].try_into().unwrap()) != 0 {
                be.close(handle);
                return Err(LoadImageError::UnsupportedCompression);
            }
            Ok((
                u32::from_le_bytes(info[4..8].try_into().unwrap()),
                u32::from_le_bytes(info[8..12].try_into().unwrap()),
                u16::from_le_bytes([info[14], info[15]]),
                u32::from_le_bytes(info[32..36].try_into().unwrap()),
                false,
            ))
        }
        Kind::Tga => {
            let mut hdr = [0u8; 18];
            if be.read(handle, &mut hdr) != Some(18) {
                return Err(fail(be));
            }
            let id = hdr[0] as usize;
            if id > 0 {
                let mut skip = vec![0u8; id];
                if be.read(handle, &mut skip) != Some(id as u32) {
                    return Err(fail(be));
                }
            }
            Ok((
                u16::from_le_bytes([hdr[12], hdr[13]]) as u32,
                u16::from_le_bytes([hdr[14], hdr[15]]) as u32,
                hdr[16] as u16,
                0,
                hdr[17] & 0x20 != 0,
            ))
        }
    }
}

fn fill_palette_live(
    kind: Kind,
    palette: &mut [u8],
    clr: u32,
    handle: i32,
    be: &mut dyn LoadImageBackend,
) -> Result<(), LoadImageError> {
    let fail = |be: &mut dyn LoadImageBackend| {
        be.close(handle);
        LoadImageError::ReadFailed
    };
    match kind {
        Kind::Bmp => {
            let need = (clr as usize)
                .checked_mul(4)
                .ok_or(LoadImageError::ReadFailed)?;
            if be.read(handle, &mut palette[..need]) != Some(need as u32) {
                return Err(fail(be));
            }
            Ok(())
        }
        Kind::Tga => {
            for i in 0..clr as usize {
                let mut rgb = [0u8; 3];
                if be.read(handle, &mut rgb) != Some(3) {
                    return Err(fail(be));
                }
                palette[i * 4..i * 4 + 3].copy_from_slice(&rgb);
            }
            Ok(())
        }
    }
}

/// m_Convert error domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvertError {
    InvalidState,
    InvalidParameters,
    UnsupportedFormat,
}

fn fmod_f32(a: f32, d: f64) -> f32 {
    ((a as f64) % d) as f32
}
fn float_to_int(v: f32) -> i32 {
    v as i32
}

impl TextureResources {
    /// Thin adapter: resolves live flags via the typed view, then calls the
    /// free function with empty file context.
    #[allow(clippy::too_many_arguments)]
    pub fn load_image_for_texture(
        &self,
        key: TextureKey,
        name: &[u8],
        sx: u32,
        sy: u32,
        extra: u32,
        from_memory: bool,
        memory: Option<&[u8]>,
        file_bytes: Option<&[u8]>,
        backend: Option<&mut dyn LoadImageBackend>,
    ) -> Result<LoadedImage, LoadImageError> {
        let flags = self
            .texture(key)
            .map_err(|_| LoadImageError::InvalidParameters)?
            .flags
            | extra;
        load_image(
            name,
            sx,
            sy,
            flags,
            from_memory,
            memory,
            file_bytes,
            None,
            DTA_INVALID_HANDLE,
            backend,
        )
    }

    /// Original m_Convert (LS3DF RVA 0x798c0): ARGB bytes (A,R,G,B order per
    /// the `__asm` esi order) to packed dest. `dst_pitch` is the locked-rect
    /// Pitch; the caller sizes dest. Bit-exact plain paths; dither is
    /// Floyd-Steinberg with 7/3/5/1 weights and 0.0625 error scale. `format`
    /// selects A4R4G4B4/R5G6B5/X1-A1/L8/A8R8G8B8; anything else is
    /// Unsupported (native default unlocks and returns true without writes --
    /// the host refuses instead; see OPEN).
    ///
    /// x87 width note: 32-bit float ops throughout, double only for the fmod
    /// divisor; FISTP rounding vs host `as` needs x86_64/ARM64 bit comparison
    /// (see OPEN). `flags` is reread live like the original's two reads.
    /// Dither bit is ITC_DITHER=0x80 (IGraph.h:637): the shipped body tests
    /// the low-byte sign (`MOV AL,[EDI+0x30] @0x100798dc; TEST AL,AL
    /// @0x100798df; JNS @0x100798e1`), not a dword `<0` (that would be
    /// ITC_LOADED=0x80000000).
    #[allow(clippy::too_many_arguments)]
    pub fn convert_argb_level(
        &self,
        key: TextureKey,
        src: &[u8],
        sx: u32,
        sy: u32,
        no_dither: i32,
        format: u32,
        dst_pitch: u32,
        dst: &mut [u8],
    ) -> Result<(), ConvertError> {
        let t = self.texture(key).map_err(|_| ConvertError::InvalidState)?;
        let flags_first = t.flags;
        let (sxn, syn) = (sx as usize, sy as usize);
        // Checked dims up front: hostile sx/sy must report InvalidParameters,
        // never wrap the length check and panic on slicing below.
        let px = sxn
            .checked_mul(syn)
            .ok_or(ConvertError::InvalidParameters)?;
        let need_src = px.checked_mul(4).ok_or(ConvertError::InvalidParameters)?;
        if src.len() < need_src {
            return Err(ConvertError::InvalidParameters);
        }
        let n = sxn
            .checked_add(2)
            .and_then(|w| syn.checked_add(1).and_then(|h| w.checked_mul(h)))
            .ok_or(ConvertError::InvalidParameters)?;
        // Fallible reserve: native OOMs the error buffer; the host reports.
        let mut err: Vec<[f32; 4]> = Vec::new();
        if no_dither == 0 && flags_first & ITC_DITHER != 0 {
            err.try_reserve_exact(n)
                .map_err(|_| ConvertError::InvalidParameters)?;
            err.resize(n, [0.0f32; 4]);
        }
        let flags = self
            .texture(key)
            .map_err(|_| ConvertError::InvalidState)?
            .flags;
        let dither = no_dither == 0 && flags & ITC_DITHER != 0;
        // Error terms share the layout err[y*(sxn+2)+x+1] = [r,g,b,a].
        // Plain paths never touch `err` (it stays empty unless dithering).
        let stride = sxn + 2;
        let at = |x: usize, y: usize, dx: isize, dy: isize| {
            ((y as isize + dy) * stride as isize + (x as isize + dx + 1)) as usize
        };
        match format {
            D3DFMT_A4R4G4B4 => {
                convert_16(
                    &mut err,
                    at,
                    src,
                    sxn,
                    syn,
                    dst,
                    dst_pitch,
                    |a: f32| fmod_f32(a, 8.0),
                    |ir: i32, ig: i32, ib: i32, ia: i32| {
                        ((((ia >> 4) & 0xF) << 12)
                            | ((ir & 0xF0) << 4)
                            | (ig & 0xF0)
                            | ((ib >> 4) & 0xF)) as u16
                    },
                    dither,
                )?;
            }
            D3DFMT_R5G6B5 => {
                convert_16_rgb(
                    &mut err,
                    at,
                    src,
                    sxn,
                    syn,
                    dst,
                    dst_pitch,
                    |r: f32| fmod_f32(r, 8.0),
                    |g: f32| fmod_f32(g, 4.0),
                    |b: f32| fmod_f32(b, 8.0),
                    |ir: i32, ig: i32, ib: i32| {
                        ((((ir >> 3) & 0x1F) << 11) | ((ig & 0xFC) << 3) | ((ib >> 3) & 0x1F))
                            as u16
                    },
                    dither,
                )?;
            }
            D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5 => {
                convert_16(
                    &mut err,
                    at,
                    src,
                    sxn,
                    syn,
                    dst,
                    dst_pitch,
                    |a: f32| fmod_f32(a, 128.0),
                    |ir: i32, ig: i32, ib: i32, ia: i32| {
                        ((((ia >> 7) & 0x1) << 15)
                            | ((ir & 0xF8) << 7)
                            | ((ig & 0xF8) << 2)
                            | ((ib >> 3) & 0x1F)) as u16
                    },
                    dither,
                )?;
            }
            D3DFMT_A8R8G8B8 => {
                for y in 0..syn {
                    for x in 0..sxn {
                        let s = (y * sxn + x) * 4;
                        dst_slot(dst, y, x, dst_pitch, 4)?.copy_from_slice(&src[s..s + 4]);
                    }
                }
            }
            D3DFMT_L8 => {
                let need = sxn
                    .checked_mul(syn)
                    .and_then(|n| n.checked_mul(3))
                    .ok_or(ConvertError::InvalidParameters)?;
                if dst.len() < need {
                    return Err(ConvertError::InvalidParameters);
                }
                let (p0, rest) = dst.split_at_mut(px);
                let (p1, p2) = rest.split_at_mut(px);
                for i in 0..px {
                    p0[i] = src[i * 4 + 1];
                    p1[i] = src[i * 4 + 2];
                    p2[i] = src[i * 4 + 3];
                }
            }
            _ => return Err(ConvertError::UnsupportedFormat),
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
/// Dest slot with checked row math: a short caller buffer reports
/// InvalidParameters instead of panicking (native writes blindly; the host
/// refuses). Source indexing stays direct: the entry gate proves
/// `s + 4 <= sxn * syn * 4 <= src.len()` with checked products.
fn dst_slot(
    dst: &mut [u8],
    y: usize,
    x: usize,
    dst_pitch: u32,
    bytes_per_pixel: usize,
) -> Result<&mut [u8], ConvertError> {
    let d = (y as u64)
        .checked_mul(dst_pitch as u64)
        .and_then(|v| v.checked_add((x as u64).checked_mul(bytes_per_pixel as u64)?))
        .ok_or(ConvertError::InvalidParameters)?;
    let d: usize = d.try_into().map_err(|_| ConvertError::InvalidParameters)?;
    let end = d
        .checked_add(bytes_per_pixel)
        .ok_or(ConvertError::InvalidParameters)?;
    dst.get_mut(d..end).ok_or(ConvertError::InvalidParameters)
}

#[allow(clippy::too_many_arguments)]
fn convert_16(
    err: &mut [[f32; 4]],
    at: impl Copy + Fn(usize, usize, isize, isize) -> usize,
    src: &[u8],
    sxn: usize,
    syn: usize,
    dst: &mut [u8],
    dst_pitch: u32,
    emod: impl Copy + Fn(f32) -> f32,
    pack: impl Copy + Fn(i32, i32, i32, i32) -> u16,
    dither: bool,
) -> Result<(), ConvertError> {
    // Channels [r,g,b,a]; alpha error term uses the same modulus for the
    // A4 (mod 8) and A1 (mod 128) paths via `emod`.
    if !dither {
        for y in 0..syn {
            for x in 0..sxn {
                let s = (y * sxn + x) * 4;
                let v = pack(
                    src[s + 1] as i32,
                    src[s + 2] as i32,
                    src[s + 3] as i32,
                    src[s] as i32,
                );
                dst_slot(dst, y, x, dst_pitch, 2)?.copy_from_slice(&v.to_le_bytes());
            }
        }
        return Ok(());
    }
    for y in 0..syn {
        for x in 0..sxn {
            let s = (y * sxn + x) * 4;
            let e = err[at(x, y, 0, 0)];
            let (mut a, mut r, mut g, mut b) = (
                src[s] as f32 + e[3],
                src[s + 1] as f32 + e[0],
                src[s + 2] as f32 + e[1],
                src[s + 3] as f32 + e[2],
            );
            let (ea, er, eg, eb) = (
                emod(a) * 0.0625,
                emod(r) * 0.0625,
                emod(g) * 0.0625,
                emod(b) * 0.0625,
            );
            err[at(x, y, 1, 0)][3] += ea * 7.0;
            err[at(x, y, 1, 0)][0] += er * 7.0;
            err[at(x, y, 1, 0)][1] += eg * 7.0;
            err[at(x, y, 1, 0)][2] += eb * 7.0;
            err[at(x, y, -1, 1)][3] += ea * 3.0;
            err[at(x, y, -1, 1)][0] += er * 3.0;
            err[at(x, y, -1, 1)][1] += eg * 3.0;
            err[at(x, y, -1, 1)][2] += eb * 3.0;
            err[at(x, y, 0, 1)][3] += ea * 5.0;
            err[at(x, y, 0, 1)][0] += er * 5.0;
            err[at(x, y, 0, 1)][1] += eg * 5.0;
            err[at(x, y, 0, 1)][2] += eb * 5.0;
            err[at(x, y, 1, 1)][3] += ea;
            err[at(x, y, 1, 1)][0] += er;
            err[at(x, y, 1, 1)][1] += eg;
            err[at(x, y, 1, 1)][2] += eb;
            a = a.min(255.0);
            r = r.min(255.0);
            g = g.min(255.0);
            b = b.min(255.0);
            let (ia, ir, ig, ib) = (
                float_to_int(a),
                float_to_int(r),
                float_to_int(g),
                float_to_int(b),
            );
            let v = pack(ir, ig, ib, ia);
            dst_slot(dst, y, x, dst_pitch, 2)?.copy_from_slice(&v.to_le_bytes());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn convert_16_rgb(
    err: &mut [[f32; 4]],
    at: impl Copy + Fn(usize, usize, isize, isize) -> usize,
    src: &[u8],
    sxn: usize,
    syn: usize,
    dst: &mut [u8],
    dst_pitch: u32,
    rmod: impl Copy + Fn(f32) -> f32,
    gmod: impl Copy + Fn(f32) -> f32,
    bmod: impl Copy + Fn(f32) -> f32,
    pack: impl Copy + Fn(i32, i32, i32) -> u16,
    dither: bool,
) -> Result<(), ConvertError> {
    if !dither {
        for y in 0..syn {
            for x in 0..sxn {
                let s = (y * sxn + x) * 4;
                let v = pack(src[s + 1] as i32, src[s + 2] as i32, src[s + 3] as i32);
                dst_slot(dst, y, x, dst_pitch, 2)?.copy_from_slice(&v.to_le_bytes());
            }
        }
        return Ok(());
    }
    for y in 0..syn {
        for x in 0..sxn {
            let s = (y * sxn + x) * 4;
            let e = err[at(x, y, 0, 0)];
            let (mut r, mut g, mut b) = (
                src[s + 1] as f32 + e[0],
                src[s + 2] as f32 + e[1],
                src[s + 3] as f32 + e[2],
            );
            let (er, eg, eb) = (rmod(r) * 0.0625, gmod(g) * 0.0625, bmod(b) * 0.0625);
            err[at(x, y, 1, 0)][0] += er * 7.0;
            err[at(x, y, 1, 0)][1] += eg * 7.0;
            err[at(x, y, 1, 0)][2] += eb * 7.0;
            err[at(x, y, -1, 1)][0] += er * 3.0;
            err[at(x, y, -1, 1)][1] += eg * 3.0;
            err[at(x, y, -1, 1)][2] += eb * 3.0;
            err[at(x, y, 0, 1)][0] += er * 5.0;
            err[at(x, y, 0, 1)][1] += eg * 5.0;
            err[at(x, y, 0, 1)][2] += eb * 5.0;
            err[at(x, y, 1, 1)][0] += er;
            err[at(x, y, 1, 1)][1] += eg;
            err[at(x, y, 1, 1)][2] += eb;
            r = r.min(255.0);
            g = g.min(255.0);
            b = b.min(255.0);
            let (ir, ig, ib) = (float_to_int(r), float_to_int(g), float_to_int(b));
            let v = pack(ir, ig, ib);
            dst_slot(dst, y, x, dst_pitch, 2)?.copy_from_slice(&v.to_le_bytes());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itexture::TextureResources;

    #[test]
    fn load_image_rejects_unknown_extension() {
        assert_eq!(
            load_image(b"a.png", 0, 0, 0, false, None, Some(&[]), None, -1, None).unwrap_err(),
            LoadImageError::UnknownExtension
        );
        assert_eq!(
            load_image(b"noname", 0, 0, 0, false, None, Some(&[]), None, -1, None).unwrap_err(),
            LoadImageError::UnknownExtension
        );
    }

    #[test]
    fn load_image_rejects_hostile_dimensions_before_allocation() {
        // 9000x9000 24-bit BMP header: over the 8192/16M defense caps, so
        // the host rejects instead of attempting a ~240 MB allocation
        // (native size math wraps; see OPEN).
        let mut bmp = vec![0u8; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&9000i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&9000i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        assert_eq!(
            load_image(b"a.bmp", 0, 0, 0, false, None, Some(&bmp), None, -1, None).unwrap_err(),
            LoadImageError::HostAllocation
        );
        // Hostile scale dims reject the same way.
        let mut tga = vec![0u8; 18];
        tga[2] = 2;
        tga[12..16].copy_from_slice(&[2, 0, 2, 0]);
        tga[16] = 24;
        tga.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        assert_eq!(
            load_image(
                b"a.tga",
                100000,
                100000,
                0,
                false,
                None,
                Some(&tga),
                None,
                -1,
                None
            )
            .unwrap_err(),
            LoadImageError::HostAllocation
        );
    }

    #[test]
    fn load_image_empty_is_identity_without_backend() {
        let mut bmp = vec![0u8; 54];
        bmp[..2].copy_from_slice(b"BM");
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&2i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&2i32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12, 0, 0]);
        let img = load_image(b"a.bmp", 0, 0, 0, false, None, Some(&bmp), None, -1, None).unwrap();
        assert_eq!((img.info.width, img.info.height, img.info.bpp), (2, 2, 24));
        assert_eq!(img.info.pitch, 8);
        assert_eq!(
            img.bits,
            [1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12, 0, 0]
        );
        assert_eq!(img.palette, None);
    }

    #[test]
    fn load_image_tga_flip_and_scale_paths() {
        // 2x2 24-bit bottom-origin TGA (desc 0x00), rows bottom-up.
        let mut tga = vec![0u8; 18];
        tga[2] = 2;
        tga[12..16].copy_from_slice(&[2, 0, 2, 0]);
        tga[16] = 24;
        tga.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        let img = load_image(b"a.tga", 0, 0, 0, false, None, Some(&tga), None, -1, None).unwrap();
        assert_eq!((img.info.width, img.info.height), (2, 2));
        assert_eq!(img.bits, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        // Top-origin (0x20) flips to bottom-up.
        let mut tga2 = tga.clone();
        tga2[17] = 0x20;
        let img2 = load_image(b"a.tga", 0, 0, 0, false, None, Some(&tga2), None, -1, None).unwrap();
        assert_eq!(img2.bits, [7, 8, 9, 10, 11, 12, 1, 2, 3, 4, 5, 6]);
        // Explicit same-size scale keeps bytes.
        let img3 = load_image(b"a.tga", 2, 2, 0, false, None, Some(&tga), None, -1, None).unwrap();
        assert_eq!(img3.bits, img.bits);
        // 1x1 downscale samples (0,0).
        let img4 = load_image(b"a.tga", 1, 1, 0, false, None, Some(&tga), None, -1, None).unwrap();
        assert_eq!(img4.bits, [1, 2, 3]);
        assert_eq!(
            (img4.info.width, img4.info.height, img4.info.pitch),
            (1, 1, 3)
        );
    }

    #[test]
    fn convert_plain_paths_pixel_exact() {
        let mut r = TextureResources::new().unwrap();
        let k = r.create_texture().unwrap();
        // ARGB order per __asm: [A,R,G,B].
        let src = [0xF0, 0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78];
        let mut dst = [0u8; 4];
        r.convert_argb_level(k, &src, 2, 1, 1, D3DFMT_A4R4G4B4, 4, &mut dst)
            .unwrap();
        assert_eq!(dst, [0xCE, 0xFA, 0x57, 0x13]);
        let mut dst = [0u8; 4];
        r.convert_argb_level(k, &src, 2, 1, 1, D3DFMT_R5G6B5, 4, &mut dst)
            .unwrap();
        assert_eq!(dst, [0x7D, 0xAE, 0xAF, 0x32]);
        let mut dst = [0u8; 4];
        r.convert_argb_level(k, &src, 2, 1, 1, D3DFMT_A1R5G5B5, 4, &mut dst)
            .unwrap();
        assert_eq!(dst, [0x3D, 0xD7, 0x4F, 0x19]);
        let mut dst = [0u8; 8];
        r.convert_argb_level(k, &src, 2, 1, 1, D3DFMT_A8R8G8B8, 8, &mut dst)
            .unwrap();
        assert_eq!(dst, src);
        let mut dst = [0u8; 6];
        r.convert_argb_level(k, &src, 2, 1, 1, D3DFMT_L8, 0, &mut dst)
            .unwrap();
        assert_eq!(dst, [0xAB, 0x34, 0xCD, 0x56, 0xEF, 0x78]);
    }

    #[test]
    fn convert_dither_seeds_deterministic_and_differ_from_plain() {
        let mut r = TextureResources::new().unwrap();
        let k = r.create_texture().unwrap();
        r.textures.texture_mut(k).flags = ITC_DITHER;
        let src: Vec<u8> = (0..4)
            .flat_map(|i| [128u8, (i * 7) as u8, (i * 13) as u8, (i * 29) as u8])
            .collect();
        for (fmt, pitch) in [
            (D3DFMT_A4R4G4B4, 8),
            (D3DFMT_R5G6B5, 8),
            (D3DFMT_A1R5G5B5, 8),
        ] {
            let mut plain = [0u8; 8];
            let mut dith = [0u8; 8];
            r.convert_argb_level(k, &src, 4, 1, 1, fmt, pitch, &mut plain)
                .unwrap();
            r.convert_argb_level(k, &src, 4, 1, 0, fmt, pitch, &mut dith)
                .unwrap();
            let mut dith2 = [0u8; 8];
            r.convert_argb_level(k, &src, 4, 1, 0, fmt, pitch, &mut dith2)
                .unwrap();
            assert_eq!(dith, dith2);
            assert_ne!(plain, dith, "fmt {fmt:#x} dither must move LSBs");
        }
    }
}
