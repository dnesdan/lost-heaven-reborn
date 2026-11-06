//! Descriptive PlayLogo caller projection, Game RVA0x15d460. AVI/Bink decoding,
//! IShow presentation and desktop file search are external dependencies. No video
//! is created or played by this module without an actual backend.
use crate::native_precision as fp;
use crate::original_system::{SystemError, SystemHandle};

/// A backend owns all created show IDs and must reject unknown/foreign handles.
/// A stopped caller leaves its show alive in that same backend; moving it to a
/// different owner or silently dropping it is not an admitted cleanup operation.
pub trait LogoBackend {
    fn find_game_file(&mut self, name: &[u8]) -> Result<Option<Vec<u8>>, SystemError>;
    fn create_show(&mut self) -> Result<SystemHandle, SystemError>;
    fn open_show(
        &mut self,
        show: SystemHandle,
        path: &[u8],
        flags: u32,
    ) -> Result<i32, SystemError>;
    fn screen_sx(&mut self) -> Result<i32, SystemError>;
    fn screen_sy(&mut self) -> Result<i32, SystemError>;
    fn set_rect(&mut self, show: SystemHandle, rect: [i32; 4]) -> Result<(), SystemError>;
    fn play(&mut self, show: SystemHandle) -> Result<(), SystemError>;
    fn render_until_key_or_end(&mut self, show: SystemHandle) -> Result<(), SystemError>;
    fn release_show(&mut self, show: SystemHandle) -> Result<(), SystemError>;
    fn ishow_client_rect(&mut self) -> Result<[i32; 4], SystemError> {
        Err(SystemError::Unsupported("IShow Win32 client rect"))
    }
    fn ishow_video_dims(&mut self) -> Result<Option<(u32, u32)>, SystemError> {
        Err(SystemError::Unsupported("IShow video dims"))
    }
    /// Cooperative yield around region-list publication (native Sleep(0)).
    /// The default never yields; faithful callers must override this hook.
    fn ishow_yield(&mut self) {}
}

fn filename(name: &[u8], extension: &[u8]) -> Result<Vec<u8>, SystemError> {
    if name.is_empty()
        || name.contains(&0)
        || name.len() + usize::from(name.last() != Some(&b'.')) + extension.len() + 1 > 256
    {
        return Err(SystemError::Unsupported(
            "PlayLogo native filename buffer domain",
        ));
    }
    let mut out = name.to_vec();
    if name.last() != Some(&b'.') {
        out.push(b'.');
    }
    out.extend_from_slice(extension);
    Ok(out)
}

/// Unavailable host calls preserve all prior effects and live show ownership in
/// the backend. No invented Release/rollback follows an unsupported operation.
/// Numeric support covers finite PC24 band arithmetic and an exactly representable
/// FILD screen height and logo dimensions in 1..=65536; other x87 exponent/exception
/// domains stop explicitly.
pub fn play_logo(
    backend: &mut impl LogoBackend,
    name: &[u8],
    size: [f32; 2],
) -> Result<bool, SystemError> {
    let avi_name = filename(name, b"avi")?;
    let (path, avi) = if let Some(path) = backend.find_game_file(&avi_name)? {
        (path, true)
    } else {
        let bik_name = filename(name, b"bik")?;
        let Some(path) = backend.find_game_file(&bik_name)? else {
            return Ok(false);
        };
        (path, false)
    };
    let show = backend.create_show()?;
    if backend.open_show(show, &path, 0)? != 0 {
        if !avi {
            backend.release_show(show)?;
            return Ok(false);
        }
        let bik_name = filename(name, b"bik")?;
        let Some(path) = backend.find_game_file(&bik_name)? else {
            backend.release_show(show)?;
            return Ok(false);
        };
        if backend.open_show(show, &path, 0)? != 0 {
            backend.release_show(show)?;
            return Ok(false);
        }
    }
    let screen_height = backend.screen_sy()?;
    if screen_height.unsigned_abs() > (1 << 24)
        || !size
            .into_iter()
            .all(|value| (1.0..=65536.0).contains(&value))
    {
        return Err(SystemError::Unsupported("PlayLogo x87 band numeric domain"));
    }
    let ratio = fp::div(size[1], size[0]);
    let scaled = fp::mul(ratio, f32::from_bits(0x3faaaaab));
    let factor = fp::sub(1.0, scaled);
    let band_float = fp::mul(screen_height as f32, factor);
    if !band_float.is_finite()
        || f64::from(band_float) < f64::from(i32::MIN)
        || f64::from(band_float) > f64::from(i32::MAX)
    {
        return Err(SystemError::Unsupported("PlayLogo ftol numeric domain"));
    }
    let band = band_float.trunc() as i32;
    fp::record_precision(band_float != band as f32);
    let mut rect = [
        0,
        band / 2,
        backend.screen_sx()?,
        backend.screen_sy()?.wrapping_sub(band),
    ];
    let double_height = backend.screen_sy()?.wrapping_mul(6) / 3;
    if backend.screen_sx()? > double_height {
        rect[0] = 0;
        rect[2] = backend.screen_sx()? / 2;
    }
    let triple_height = backend.screen_sy()?.wrapping_mul(9) / 3;
    if backend.screen_sx()? > triple_height {
        rect[0] = backend.screen_sx()? / 3;
        rect[2] = backend.screen_sx()? / 3;
    }
    backend.set_rect(show, rect)?;
    backend.play(show)?;
    backend.render_until_key_or_end(show)?;
    backend.release_show(show)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        width: i32,
        height: i32,
        missing_avi: bool,
        fail_avi: bool,
        files: Vec<Vec<u8>>,
        opened: Vec<Vec<u8>>,
        rect: Option<[i32; 4]>,
        effects: Vec<&'static str>,
    }
    impl Fixture {
        fn new(width: i32, height: i32) -> Self {
            Self {
                width,
                height,
                missing_avi: false,
                fail_avi: false,
                files: vec![],
                opened: vec![],
                rect: None,
                effects: vec![],
            }
        }
    }
    impl LogoBackend for Fixture {
        fn find_game_file(&mut self, p: &[u8]) -> Result<Option<Vec<u8>>, SystemError> {
            self.files.push(p.to_vec());
            Ok((!self.missing_avi || !p.ends_with(b"avi")).then(|| p.to_vec()))
        }
        fn create_show(&mut self) -> Result<SystemHandle, SystemError> {
            self.effects.push("create");
            Ok(SystemHandle(1))
        }
        fn open_show(&mut self, _: SystemHandle, p: &[u8], _: u32) -> Result<i32, SystemError> {
            self.opened.push(p.to_vec());
            Ok(if self.fail_avi && p.ends_with(b"avi") {
                -1
            } else {
                0
            })
        }
        fn screen_sx(&mut self) -> Result<i32, SystemError> {
            self.effects.push("sx");
            Ok(self.width)
        }
        fn screen_sy(&mut self) -> Result<i32, SystemError> {
            self.effects.push("sy");
            Ok(self.height)
        }
        fn set_rect(&mut self, _: SystemHandle, r: [i32; 4]) -> Result<(), SystemError> {
            self.rect = Some(r);
            self.effects.push("rect");
            Ok(())
        }
        fn play(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            self.effects.push("play");
            Ok(())
        }
        fn render_until_key_or_end(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            self.effects.push("render");
            Ok(())
        }
        fn release_show(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            self.effects.push("release");
            Ok(())
        }
    }
    #[test]
    fn failed_avi_reuses_show_for_bink_and_preserves_native_triple_monitor_rectangle() {
        let mut f = Fixture::new(2400, 600);
        f.fail_avi = true;
        assert_eq!(play_logo(&mut f, b"logo.", [800.0, 600.0]), Ok(true));
        assert_eq!(f.files, vec![b"logo.avi".to_vec(), b"logo.bik".to_vec()]);
        assert_eq!(f.opened, f.files);
        assert_eq!(f.rect, Some([800, 0, 800, 600]));
        assert_eq!(
            f.effects,
            vec![
                "create", "sy", "sx", "sy", "sy", "sx", "sx", "sy", "sx", "sx", "sx", "rect",
                "play", "render", "release"
            ]
        );
    }
    #[test]
    fn missing_avi_and_letterbox_band_keep_original_asymmetric_bottom() {
        let mut f = Fixture::new(800, 600);
        f.missing_avi = true;
        assert_eq!(play_logo(&mut f, b"logo2", [704.0, 400.0]), Ok(true));
        assert_eq!(f.rect, Some([0, 72, 800, 455]));
        assert_eq!(f.opened, vec![b"logo2.bik".to_vec()]);
    }
    #[test]
    fn unsupported_numeric_domain_preserves_unreleased_show_after_open() {
        let mut f = Fixture::new(800, i32::MAX);
        assert_eq!(
            play_logo(&mut f, b"logo", [800.0, 600.0]),
            Err(SystemError::Unsupported("PlayLogo x87 band numeric domain"))
        );
        assert_eq!(f.effects, vec!["create", "sy"]);
        assert_eq!(f.opened.len(), 1);
    }
}

// ---------------------------------------------------------------------------
// IShow behavioral contracts (LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//
// CPU-only prefixes per sibling mafia-re @ 113b5b6 (ls3df/IGraph/IShow.cpp,
// ls3df/IGraph/IGraph.h). Every entry below is tier contract: reviewed
// upstream source plus pinning tests. No Ghidra bodies were re-observed here;
// REA body notes below repeat the assigned 16B and 531B shapes. The
// GetVolume/GetDecodedFrameNum/IsStreamPlaying-dB entries likewise have no
// Ghidra proc; their byte proofs are cited in the function docs.
//
// Working names: IShow:: method names are PS2-evidenced original names
// per IGraph.h:424-433 (PS2 IShow.cpp) plus the inline header originals
// GetVolume (IGraph.h:454) and GetDecodedFrameNum (IGraph.h:469);
// IsStreamPlaying (IGraph.h:481-483) is PC-only-guessed and lives in
// runtime_audio; Rust spelling is snake_case per repo rule and each
// function doc records its RVA/VA and mafia-re file:line. All IShow data
// members used here (reference count, init flag, rectangle, region list,
// video source dims) are working-guess projections per source, including
// m_Volume/m_DecodedFrameNum (IGraph.h:510,513 — members guessed except
// m_RefCount); the native member offsets are not mirrored on the host
// (named fields, no raw layout).
//
// Scope: CPU logic only. Win32 window/client selection, GetClientRect,
// DirectShow/COM graph, Bink decoding, Ogg/DirectSound submits and thread
// yields stay behind LogoBackend (ishow_client_rect, ishow_video_dims,
// ishow_yield). Pure float vertex layout is host f32 arithmetic; the original
// x87 single-precision path is exactly representable for the pinned vectors.
//
// Ownership: typed state throughout, no byte maps. The region slice borrows
// the live show; an empty list yields None on the host (native takes
// &list[0] and assumes a region exists), disclosed hardening.
//
// Withheld (no claims, no functions): Open/Close/Play/Stop/Pause/Event/Seek/
// Volume-submit/Render paths (DirectShow/Bink/Ogg device submits); the CPU
// dB sub-map is ported as ishow_volume_db in runtime_audio. Construction-
// covered, no new functions: empty IShow ctor/dtor field defaults (covered by
// struct construction in tests).
//
/// LS3D_RESULT words used by the IShow contracts (ls3df.h:20-35).
pub const ISHOW_OK: i32 = 0;
/// Not initialized word for the IShow guard prefix (ls3df.h:29).
pub const ISHOW_NOTINITIALIZED: i32 = 0x8000_0009u32 as i32;

/// Transformed lit vertex projection (IGraph.h:380-385 LS3D_TLVERTEX, working
/// guess name per source). Specular is retained verbatim; PrepareVerts never
/// writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct IshowVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rhw: f32,
    pub diffuse: u32,
    pub specular: u32,
    pub u: f32,
    pub v: f32,
}

impl Default for IshowVertex {
    /// Host-chosen initials (native leaves verts uninitialized until
    /// PrepareVerts and never writes specular): deterministic hardening.
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rhw: 1.0,
            diffuse: 0xffff_ffff,
            specular: 0,
            u: 0.0,
            v: 0.0,
        }
    }
}

/// One movie tile in its own texture (IGraph.h:406-413 S_show_region, working
/// guess name per source). The texture object stays caller-owned; only its
/// width/height words needed for u/v are stored. Native reads the texture
/// m_Width/m_Height (ushort) live; the host copies must be re-synced by the
/// caller if the texture is resized (u32 widening disclosed).
#[derive(Clone, Debug, PartialEq)]
pub struct IshowRegion {
    pub x: i32,
    pub y: i32,
    pub sx: i32,
    pub sy: i32,
    pub tex_width: u32,
    pub tex_height: u32,
    pub verts: [IshowVertex; 4],
}

/// IShow CPU projection (IGraph.h:431-534, working-guess members per source;
/// m_Volume/m_DecodedFrameNum are working-guess names per IGraph.h:510,513
/// (members guessed except m_RefCount); native offsets +0x3C/+0x54 cited in
/// the getter docs and kept as named fields on the host, not raw layout).
/// Reference count, init flag, screen rectangle, region list, volume and
/// decoded frame number only; graph, Bink, Ogg, DirectSound and thread
/// handles stay behind the backend.
#[derive(Clone, Debug, PartialEq)]
pub struct Ishow {
    pub ref_count: i32,
    pub is_init: bool,
    pub rect: [i32; 4],
    pub regions: Vec<IshowRegion>,
    /// Working-guess m_Volume (IGraph.h:510, native +0x3C matching the
    /// SetVolume store). Default matches the native ctor 1.0f
    /// (IShow.cpp:127,147).
    pub volume: f32,
    /// Working-guess m_DecodedFrameNum (IGraph.h:513, native +0x54,
    /// counted by the renderer).
    pub decoded_frame_num: i32,
}

impl Default for Ishow {
    /// Host-chosen initials: ref 1 and zeroed rect mirror the native ctor;
    /// volume starts at the native 1.0f initial (IShow.cpp:127,147); decoded frame
    /// number 0 matches native.
    fn default() -> Self {
        Self {
            ref_count: 1,
            is_init: false,
            rect: [0, 0, 0, 0],
            regions: Vec::new(),
            volume: 1.0,
            decoded_frame_num: 0,
        }
    }
}

/// LS3DF RVA 0x73570, VA 0x10073570. IShow::AddRef PS2-evidenced original name
/// (mafia-re ls3df/IGraph/IShow.cpp:189): pre-increment, return new count.
/// 16B body, pure CPU.
pub fn ishow_add_ref(show: &mut Ishow) -> i32 {
    show.ref_count = show.ref_count.wrapping_add(1);
    show.ref_count
}

/// LS3DF RVA 0x740e0, VA 0x100740e0. IShow::IsInit PS2-evidenced original name
/// (mafia-re ls3df/IGraph/IShow.cpp:589): init flag read, pure CPU.
pub fn ishow_is_init(show: &Ishow) -> bool {
    show.is_init
}

/// LS3DF RVA 0x75820, VA 0x10075820. IShow::GetVolume PS2-evidenced original
/// name (mafia-re ls3df/IGraph/IGraph.h:454; method not PC-only-marked,
/// so the PS2 name stands — unlike the guessed members below): volume word
/// copy, pure CPU. No
/// Ghidra proc was observed here; byte proof is MOV EAX,[ESP+4] then FLD
/// dword [EAX+0x3C] then RET 4, reading float m_Volume at +0x3C matching the
/// SetVolume store. Host copies the bit-exact f32 with no conversion.
pub fn ishow_get_volume(show: &Ishow) -> f32 {
    show.volume
}

/// LS3DF RVA 0x75830, VA 0x10075830. IShow::GetDecodedFrameNum PS2-evidenced
/// original name (mafia-re ls3df/IGraph/IGraph.h:469; method not
/// PC-only-marked, so the PS2 name stands): decoded frame counter
/// read, pure CPU. No Ghidra proc was observed here; byte proof is MOV
/// EAX,[EAX+0x54] then RET 4, reading i32 m_DecodedFrameNum at +0x54.
pub fn ishow_get_decoded_frame_num(show: &Ishow) -> i32 {
    show.decoded_frame_num
}

/// LS3DF RVA 0x74380, VA 0x10074380. IShow::m_PrepareVerts PS2-evidenced
/// original name (mafia-re ls3df/IGraph/IShow.cpp:717): pure CPU float layout,
/// 531B proc. Scales the movie to the rectangle: Bink dims when present else
/// texture-renderer dims, passed as video_dims; None (neither source) returns
/// without touching vertices like the native early return. Scale uses
/// rect right/bottom directly (IShow.cpp:722-726 `m_Rect.right / Width`,
/// not width-minus-left), diffuse is opaque white,
/// z is 0, rhw is 1, u/v are sx/tex-width and sy/tex-height. Specular is
/// preserved. Host f32 division yields inf or NaN on zero dims like x87 singles.
pub fn ishow_prepare_verts(show: &mut Ishow, video_dims: Option<(u32, u32)>) {
    let Some((vid_w, vid_h)) = video_dims else {
        return;
    };
    let scale_x = show.rect[2] as f32 / vid_w as f32;
    let scale_y = show.rect[3] as f32 / vid_h as f32;
    // Native walks the list back-to-front; per-region writes are independent,
    // so the host walks front-to-back with identical values (order disclosed).
    for region in show.regions.iter_mut().rev() {
        let u = region.sx as f32 / region.tex_width as f32;
        let v = region.sy as f32 / region.tex_height as f32;
        let x0 = region.x as f32 * scale_x + show.rect[0] as f32;
        let y0 = region.y as f32 * scale_y + show.rect[1] as f32;
        let x1 = region.x.wrapping_add(region.sx) as f32 * scale_x + show.rect[0] as f32;
        let y1 = region.y.wrapping_add(region.sy) as f32 * scale_y + show.rect[1] as f32;
        region.verts[0].diffuse = 0xffff_ffff;
        region.verts[0].x = x0;
        region.verts[0].y = y0;
        region.verts[0].z = 0.0;
        region.verts[0].rhw = 1.0;
        region.verts[0].u = 0.0;
        region.verts[0].v = 0.0;
        region.verts[1].diffuse = 0xffff_ffff;
        region.verts[1].x = x1;
        region.verts[1].y = y0;
        region.verts[1].z = 0.0;
        region.verts[1].rhw = 1.0;
        region.verts[1].u = u;
        region.verts[1].v = 0.0;
        region.verts[2].diffuse = 0xffff_ffff;
        region.verts[2].x = x0;
        region.verts[2].y = y1;
        region.verts[2].z = 0.0;
        region.verts[2].rhw = 1.0;
        region.verts[2].u = 0.0;
        region.verts[2].v = v;
        region.verts[3].diffuse = 0xffff_ffff;
        region.verts[3].x = x1;
        region.verts[3].y = y1;
        region.verts[3].z = 0.0;
        region.verts[3].rhw = 1.0;
        region.verts[3].u = u;
        region.verts[3].v = v;
    }
}

/// LS3DF RVA 0x745a0, VA 0x100745a0. IShow::SetRect PS2-evidenced original name
/// (mafia-re ls3df/IGraph/IShow.cpp:763): rect resolve, store, then prepare.
/// A Some rect copies four words; None resolves through the backend Win32
/// client rect (native HWND child/main choice plus GetClientRect stay behind
/// ishow_client_rect). The stored rect then prepares with the backend video
/// dims (Bink vs renderer choice stays behind ishow_video_dims). Always
/// I3D_OK past the backend boundary; backend errors propagate unchanged.
/// A video-dims error leaves the already-stored rect in place (no rollback,
/// matching the native store-before-prepare order).
pub fn ishow_set_rect(
    show: &mut Ishow,
    rect: Option<[i32; 4]>,
    backend: &mut impl LogoBackend,
) -> Result<i32, SystemError> {
    let resolved = match rect {
        Some(words) => words,
        None => backend.ishow_client_rect()?,
    };
    show.rect = resolved;
    let dims = backend.ishow_video_dims()?;
    ishow_prepare_verts(show, dims);
    Ok(ISHOW_OK)
}

/// LS3DF RVA 0x74640, VA 0x10074640. IShow::GetRect PS2-evidenced original name
/// (mafia-re ls3df/IGraph/IShow.cpp:778): guard !m_Init returns 0x80000009,
/// else four-word copy, I3D_OK. Pure CPU.
pub fn ishow_get_rect(show: &Ishow, out: &mut [i32; 4]) -> i32 {
    if !show.is_init {
        return ISHOW_NOTINITIALIZED;
    }
    *out = show.rect;
    ISHOW_OK
}

/// LS3DF RVA 0x74680, VA 0x10074680. IShow::GetVideoRect PS2-evidenced original
/// name (mafia-re ls3df/IGraph/IShow.cpp:788): guard !m_Init returns
/// 0x80000009 leaving out untouched; else zero output and fill right/bottom
/// from the backend dims. Backend None (no Bink and no texture renderer)
/// zeroes then returns 0x80000009 like the native null-renderer arm. The
/// Bink-vs-renderer branch stays behind ishow_video_dims; uint-to-LONG
/// narrowing wraps via as cast.
pub fn ishow_get_video_rect(
    show: &Ishow,
    out: &mut [i32; 4],
    backend: &mut impl LogoBackend,
) -> Result<i32, SystemError> {
    if !show.is_init {
        return Ok(ISHOW_NOTINITIALIZED);
    }
    *out = [0, 0, 0, 0];
    let Some((width, height)) = backend.ishow_video_dims()? else {
        return Ok(ISHOW_NOTINITIALIZED);
    };
    out[2] = width as i32;
    out[3] = height as i32;
    Ok(ISHOW_OK)
}

/// LS3DF RVA 0x74ae0, VA 0x10074ae0. IShow::GetRegionCnt PS2-evidenced original
/// name (mafia-re ls3df/IGraph/IShow.cpp:935): region counter read, pure CPU.
pub fn ishow_region_count(show: &Ishow) -> i32 {
    show.regions.len() as i32
}

/// LS3DF RVA 0x74af0, VA 0x10074af0. IShow::GetRegions PS2-evidenced original
/// name (mafia-re ls3df/IGraph/IShow.cpp:942): thread yield then base pointer.
/// The Sleep(0) yield stays behind ishow_yield; the base pointer is the live
/// region slice. Empty yields None on the host (native assumes a region),
/// disclosed hardening.
pub fn ishow_regions<'a>(
    show: &'a mut Ishow,
    backend: &mut impl LogoBackend,
) -> Option<&'a mut [IshowRegion]> {
    backend.ishow_yield();
    if show.regions.is_empty() {
        return None;
    }
    Some(&mut show.regions[..])
}

#[cfg(test)]
mod ishow_contract_tests {
    use super::*;

    struct Fake {
        client: [i32; 4],
        client_err: bool,
        dims_err: bool,
        dims: Option<(u32, u32)>,
        yields: usize,
    }

    impl Fake {
        fn new(client: [i32; 4], dims: Option<(u32, u32)>) -> Self {
            Self {
                client,
                client_err: false,
                dims_err: false,
                dims,
                yields: 0,
            }
        }
    }

    impl LogoBackend for Fake {
        fn find_game_file(&mut self, _: &[u8]) -> Result<Option<Vec<u8>>, SystemError> {
            Err(SystemError::Unsupported("IShow test file search"))
        }
        fn create_show(&mut self) -> Result<SystemHandle, SystemError> {
            Err(SystemError::Unsupported("IShow test create"))
        }
        fn open_show(&mut self, _: SystemHandle, _: &[u8], _: u32) -> Result<i32, SystemError> {
            Err(SystemError::Unsupported("IShow test open"))
        }
        fn screen_sx(&mut self) -> Result<i32, SystemError> {
            Err(SystemError::Unsupported("IShow test sx"))
        }
        fn screen_sy(&mut self) -> Result<i32, SystemError> {
            Err(SystemError::Unsupported("IShow test sy"))
        }
        fn set_rect(&mut self, _: SystemHandle, _: [i32; 4]) -> Result<(), SystemError> {
            Err(SystemError::Unsupported("IShow test set_rect"))
        }
        fn play(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            Err(SystemError::Unsupported("IShow test play"))
        }
        fn render_until_key_or_end(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            Err(SystemError::Unsupported("IShow test render"))
        }
        fn release_show(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            Err(SystemError::Unsupported("IShow test release"))
        }
        fn ishow_client_rect(&mut self) -> Result<[i32; 4], SystemError> {
            if self.client_err {
                return Err(SystemError::Unsupported("IShow test client err"));
            }
            Ok(self.client)
        }
        fn ishow_video_dims(&mut self) -> Result<Option<(u32, u32)>, SystemError> {
            if self.dims_err {
                return Err(SystemError::Unsupported("IShow test dims err"));
            }
            Ok(self.dims)
        }
        fn ishow_yield(&mut self) {
            self.yields += 1;
        }
    }

    fn sample_show() -> Ishow {
        Ishow {
            ref_count: 1,
            is_init: false,
            rect: [0, 0, 0, 0],
            regions: Vec::new(),
            volume: 1.0,
            decoded_frame_num: 0,
        }
    }

    fn sample_region() -> IshowRegion {
        IshowRegion {
            x: 5,
            y: 7,
            sx: 10,
            sy: 9,
            tex_width: 20,
            tex_height: 18,
            verts: [
                IshowVertex::default(),
                IshowVertex::default(),
                IshowVertex::default(),
                IshowVertex::default(),
            ],
        }
    }

    #[test]
    fn ishow_add_ref_increments() {
        let mut show = sample_show();
        assert_eq!(ishow_add_ref(&mut show), 2);
        assert_eq!(show.ref_count, 2);
        assert_eq!(ishow_add_ref(&mut show), 3);
    }

    #[test]
    fn ishow_is_init_reads_flag() {
        let mut show = sample_show();
        assert!(!ishow_is_init(&show));
        show.is_init = true;
        assert!(ishow_is_init(&show));
    }

    #[test]
    fn ishow_get_volume_round_trip_incl_default() {
        // Default matches the native 1.0f initial; the getter copies the
        // bit-exact f32.
        let def = Ishow::default();
        assert_eq!(def.volume.to_bits(), 1.0f32.to_bits());
        assert_eq!(ishow_get_volume(&def).to_bits(), 1.0f32.to_bits());
        let mut show = sample_show();
        assert_eq!(ishow_get_volume(&show).to_bits(), 1.0f32.to_bits());
        show.volume = 0.75;
        assert_eq!(ishow_get_volume(&show).to_bits(), 0.75f32.to_bits());
        show.volume = 1.0;
        assert_eq!(ishow_get_volume(&show).to_bits(), 1.0f32.to_bits());
    }

    #[test]
    fn ishow_get_decoded_frame_num_default_and_set() {
        let def = Ishow::default();
        assert_eq!(def.decoded_frame_num, 0);
        assert_eq!(ishow_get_decoded_frame_num(&def), 0);
        let mut show = sample_show();
        assert_eq!(ishow_get_decoded_frame_num(&show), 0);
        show.decoded_frame_num = 42;
        assert_eq!(ishow_get_decoded_frame_num(&show), 42);
    }

    #[test]
    fn ishow_prepare_verts_scales_movie_to_rect() {
        let mut show = sample_show();
        show.rect = [10, 20, 400, 300];
        show.regions.push(sample_region());
        ishow_prepare_verts(&mut show, Some((200, 150)));
        let verts = &show.regions[0].verts;
        assert_eq!((verts[0].x, verts[0].y), (20.0, 34.0));
        assert_eq!((verts[1].x, verts[1].y), (40.0, 34.0));
        assert_eq!((verts[2].x, verts[2].y), (20.0, 52.0));
        assert_eq!((verts[3].x, verts[3].y), (40.0, 52.0));
        assert_eq!(
            (verts[1].u, verts[2].v, verts[3].u, verts[3].v),
            (0.5, 0.5, 0.5, 0.5)
        );
        assert_eq!(verts[0].diffuse, 0xffff_ffff);
        assert_eq!((verts[0].z, verts[0].rhw), (0.0, 1.0));
    }

    #[test]
    fn ishow_prepare_verts_without_source_keeps_verts() {
        let mut show = sample_show();
        show.rect = [10, 20, 400, 300];
        show.regions.push(sample_region());
        let before = show.clone();
        ishow_prepare_verts(&mut show, None);
        assert_eq!(show, before);
    }

    #[test]
    fn ishow_prepare_verts_zero_dims_and_specular_and_empty() {
        // Host f32 division by zero yields inf like x87 singles; specular is
        // preserved (native never stores it here); empty regions are a no-op.
        let mut show = sample_show();
        show.rect = [0, 0, 100, 100];
        let mut region = sample_region();
        region.tex_width = 0;
        region.verts[0].specular = 0x1234_5678;
        show.regions.push(region);
        ishow_prepare_verts(&mut show, Some((50, 50)));
        assert!(show.regions[0].verts[1].u.is_infinite());
        assert_eq!(show.regions[0].verts[0].specular, 0x1234_5678);
        let mut empty = sample_show();
        let before = empty.clone();
        ishow_prepare_verts(&mut empty, Some((50, 50)));
        assert_eq!(empty, before);
    }

    #[test]
    fn ishow_set_rect_copies_and_prepares() {
        let mut show = sample_show();
        show.regions.push(sample_region());
        let mut backend = Fake::new([0, 0, 640, 480], Some((200, 150)));
        assert_eq!(
            ishow_set_rect(&mut show, Some([10, 20, 400, 300]), &mut backend),
            Ok(0)
        );
        assert_eq!(show.rect, [10, 20, 400, 300]);
        assert_eq!(show.regions[0].verts[0].x, 20.0);
    }

    #[test]
    fn ishow_set_rect_null_resolves_client_rect() {
        let mut show = sample_show();
        let mut backend = Fake::new([1, 2, 640, 480], None);
        assert_eq!(ishow_set_rect(&mut show, None, &mut backend), Ok(0));
        assert_eq!(show.rect, [1, 2, 640, 480]);
    }

    #[test]
    fn ishow_set_rect_client_error_propagates_and_keeps_rect() {
        let mut show = sample_show();
        show.rect = [1, 2, 3, 4];
        let mut backend = Fake::new([0, 0, 640, 480], None);
        backend.client_err = true;
        assert!(ishow_set_rect(&mut show, None, &mut backend).is_err());
        assert_eq!(show.rect, [1, 2, 3, 4]);
    }

    #[test]
    fn ishow_set_rect_dims_error_keeps_stored_rect() {
        // Store-before-prepare: a dims failure propagates with the new rect
        // already in place (no rollback, native order).
        let mut show = sample_show();
        let mut backend = Fake::new([0, 0, 640, 480], Some((200, 150)));
        backend.dims_err = true;
        assert!(ishow_set_rect(&mut show, Some([10, 20, 400, 300]), &mut backend).is_err());
        assert_eq!(show.rect, [10, 20, 400, 300]);
    }

    #[test]
    fn ishow_get_rect_guards_and_copies() {
        let mut show = sample_show();
        let mut out = [9, 9, 9, 9];
        assert_eq!(ishow_get_rect(&show, &mut out), ISHOW_NOTINITIALIZED);
        // Guard leaves out untouched.
        assert_eq!(out, [9, 9, 9, 9]);
        show.is_init = true;
        show.rect = [1, 2, 3, 4];
        assert_eq!(ishow_get_rect(&show, &mut out), 0);
        assert_eq!(out, [1, 2, 3, 4]);
    }

    #[test]
    fn ishow_get_video_rect_guards_and_fills() {
        let mut show = sample_show();
        let mut backend = Fake::new([0, 0, 0, 0], Some((320, 200)));
        let mut out = [9, 9, 9, 9];
        assert_eq!(
            ishow_get_video_rect(&show, &mut out, &mut backend),
            Ok(ISHOW_NOTINITIALIZED)
        );
        assert_eq!(out, [9, 9, 9, 9]);
        show.is_init = true;
        assert_eq!(ishow_get_video_rect(&show, &mut out, &mut backend), Ok(0));
        assert_eq!(out, [0, 0, 320, 200]);
        let mut empty = Fake::new([0, 0, 0, 0], None);
        assert_eq!(
            ishow_get_video_rect(&show, &mut out, &mut empty),
            Ok(ISHOW_NOTINITIALIZED)
        );
        assert_eq!(out, [0, 0, 0, 0]);
    }

    #[test]
    fn ishow_region_count_reads() {
        let mut show = sample_show();
        assert_eq!(ishow_region_count(&show), 0);
        show.regions.push(sample_region());
        show.regions.push(sample_region());
        assert_eq!(ishow_region_count(&show), 2);
    }

    #[test]
    fn ishow_regions_yields_and_returns_base() {
        let mut show = sample_show();
        let mut backend = Fake::new([0, 0, 0, 0], None);
        assert!(ishow_regions(&mut show, &mut backend).is_none());
        assert_eq!(backend.yields, 1);
        show.regions.push(sample_region());
        let len = ishow_regions(&mut show, &mut backend).unwrap().len();
        assert_eq!(len, 1);
        assert_eq!(backend.yields, 2);
        // The returned slice borrows the live list: mutation reflects back.
        show.regions.push(sample_region());
        let slice = ishow_regions(&mut show, &mut backend).unwrap();
        slice[0].x = 42;
        assert_eq!(show.regions[0].x, 42);
    }
}
