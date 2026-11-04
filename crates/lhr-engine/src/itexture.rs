//! PC ITexture lifetime and concrete material integration.
//! Original PC LS3DF.dll SHA-256:
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Scoped RVAs: ctor0x75c10, Release0x75cd0, nondeleting dtor0x75c50,
//! Close0x78680, Unlock0x78820; managed removal0x7aee0 is defined in texture_manager. ITexture/C_str names have original PS2
//! provenance; PC layout/order are independently evidenced, not PS2 equivalence.
//! TextureResources owns canonical texture identity. The factory module adds a
//! bounded immediate filename/Create CPU adapter; GPU upload and Lock remain
//! external. Constructor-unwritten fields remain unavailable.
//! Managed ownership requires an admitted seeded world and real optional backend
//! capability; unknown managed/queued/pool state rejects before effects.
//! Required surface/graphics/pixels/string/buffer effects are external, serialized
//! and nonreentrant in one compatible live identity domain. Explicit cleanup is
//! necessary; Drop cannot run these callbacks. Host tombstones do not represent
//! native ITexture allocation/delete. Read-only getters include interrupted state.
use crate::buffer_managers::{self as bm, BufferBackend, VertexRequest};
use crate::c_str::{CStrBackend, CStrKey, CStrings, RepView, StringError, StringOperationError};
use crate::i3d_object::{
    ObjectError, ObjectOperationError, ObjectResourceKey, ObjectResources, PlainLodKey,
};
use crate::managed_visual_resources::ManagedVisualBackend;
use crate::texture_manager::{
    CreationFormats, ManagedOperationError, ManagedTextureEffects, ManagedWorldIntake,
    ManagerError, RegistryIntake, TextureManager,
};
use crate::visual_resources::{
    self as vr, FaceGroupKey, MaterialKey, MeshLevelKey, MeshObjectKey, RawVertexBytesKey,
};
#[path = "texture_factory.rs"]
mod factory;
pub use factory::*;
#[path = "texture_load_image.rs"]
pub mod texture_load_image;
#[path = "texture_surface.rs"]
pub mod texture_surface;
use self::texture_surface::{
    Rect, TextureDrawSink, TextureSurfaceSink, TlVertex, LOCK_FLAG_NO_DIRTY_UPDATE, PRELOAD_POINT,
    PRIM_TRIANGLESTRIP, SAMPLER_CLAMP, SAMPLER_U, SAMPLER_V, SAMPLER_WRAP, TL_FVF,
    VERTEX_FORMAT_TLVERTEX,
};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureKey {
    arena: u64,
    slot: u32,
}
impl TextureKey {
    pub fn arena(self) -> u64 {
        self.arena
    }
    pub fn slot(self) -> u32 {
        self.slot
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureGraphics(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureSurface(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PixelAllocation(pub u64);
/// Stable allocation identity of a managed record, distinct from active membership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManagedRecordKey(pub u64);
pub trait TextureBackend:
    BufferBackend + CStrBackend<Error = <Self as BufferBackend>::Error>
{
    /// Optional capability lookup is pure and stable throughout each call.
    /// None explicitly rejects managed cleanup before effects; it is not a
    /// successful replacement. Real implementations supply every required effect.
    fn managed_texture_effects(
        &mut self,
    ) -> Option<&mut dyn ManagedTextureEffects<Error = <Self as BufferBackend>::Error>> {
        None
    }
    /// Infallible native diagnostic side effect (`dbgPrintf`). Required, not
    /// defaulted: silently dropping the Reload/SaveToTGA log lines would
    /// diverge from the evidenced stub and error blocks.
    fn log_diagnostic(&mut self, message: TextureDiagnostic);
    fn duplicate_effects(
        &mut self,
    ) -> Option<&mut dyn TextureDuplicateEffects<Error = <Self as BufferBackend>::Error>> {
        None
    }
    fn tga_effects(
        &mut self,
    ) -> Option<&mut dyn TextureTgaEffects<Error = <Self as BufferBackend>::Error>> {
        None
    }
    fn presaved_effects(
        &mut self,
    ) -> Option<&mut dyn TexturePresavedEffects<Error = <Self as BufferBackend>::Error>> {
        None
    }
    fn unlock_surface(
        &mut self,
        surface: TextureSurface,
    ) -> Result<(), <Self as BufferBackend>::Error>;
    fn release_surface(
        &mut self,
        surface: TextureSurface,
    ) -> Result<(), <Self as BufferBackend>::Error>;
    fn release_texture_graphics(
        &mut self,
        texture: TextureGraphics,
    ) -> Result<(), <Self as BufferBackend>::Error>;
    fn delete_pixels(
        &mut self,
        pixels: PixelAllocation,
    ) -> Result<(), <Self as BufferBackend>::Error>;
    fn free_raw_vertices(
        &mut self,
        level: MeshLevelKey,
        bytes: RawVertexBytesKey,
    ) -> Result<(), <Self as BufferBackend>::Error>;
    fn creation_sink(
        &mut self,
    ) -> Option<&mut dyn texture_surface::TextureCreationSink<Error = <Self as BufferBackend>::Error>>
    {
        None
    }
}
/// LS3D_RESULT codes reached by the Duplicate/Reload/SaveToTGA ports
/// (mafia-re ls3df.h:21-35). PC LS3DF.dll SHA-256
/// e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
pub const I3D_OK: u32 = 0;
pub const I3DERR_OUTOFMEM: u32 = 0x80000002;
pub const I3DERR_UNSUPPORTED: u32 = 0x8000000e;
/// Evidenced D3D8 texture formats (SaveToTGA CMP immediates).
pub const D3DFMT_R8G8B8: u32 = 20;
pub const D3DFMT_A8R8G8B8: u32 = 21;
pub const D3DFMT_X8R8G8B8: u32 = 22;
pub const D3DFMT_R5G6B5: u32 = 23;
pub const D3DFMT_X1R5G5B5: u32 = 24;
pub const D3DFMT_A1R5G5B5: u32 = 25;
pub const D3DFMT_A4R4G4B4: u32 = 26;
pub const D3DFMT_L8: u32 = 50;
pub const D3DFMT_DXT1: u32 = 0x31545844;
pub const D3DFMT_DXT2: u32 = 0x32545844;
pub const D3DFMT_DXT3: u32 = 0x33545844;
pub const D3DFMT_DXT4: u32 = 0x34545844;
pub const D3DFMT_DXT5: u32 = 0x35545844;
/// Flag bit selecting the presaved `L` name suffix (mafia-re IGraph.h ITC_HALF).
pub const ITC_HALF: u32 = 0x2000;
/// Presaved DXT selectors (mafia-re ITexture.cpp:85-87).
pub const PRESAVED_DXT1: u8 = 1;
pub const PRESAVED_DXT1_ALPHA: u8 = 2;
pub const PRESAVED_DXT3: u8 = 3;
/// Presaved payload versions (static words at 0x1009d8d0/0x1009d8d2).
pub const PRESAVED_DXT_VERSION: u16 = 9;
pub const PRESAVED_TEX_VERSION: u16 = 1;
/// Native presaved name scratch size (`char file[256]`).
pub const PRESAVED_NAME_CAP: usize = 256;

/// Byte-exact native diagnostic lines. The `Compresed` typo is original
/// (LS3DF string at 0x100ab118).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureDiagnostic {
    ReloadNotImplemented,
    TgaCompressedPixelFormat(Vec<u8>),
    TgaCannotCreateFile(Vec<u8>),
    TgaCannotLockTexture,
}
/// Pure renderer for [`TextureDiagnostic`]; unit-tested byte-for-byte.
pub fn render_diagnostic(line: &TextureDiagnostic) -> Vec<u8> {
    match line {
        TextureDiagnostic::ReloadNotImplemented => {
            b"ITexture::Reload() - WAS NOT IMPLEMENTED !".to_vec()
        }
        TextureDiagnostic::TgaCompressedPixelFormat(name) => {
            let mut out = b"!! ITexture::SaveToTGA() - Cannot create file ".to_vec();
            out.extend_from_slice(name);
            out.extend_from_slice(b"  Compresed pixel format isn't supported");
            out
        }
        TextureDiagnostic::TgaCannotCreateFile(name) => {
            let mut out = b"!! ITexture::SaveToTGA() - Cannot create file ".to_vec();
            out.extend_from_slice(name);
            out
        }
        TextureDiagnostic::TgaCannotLockTexture => {
            b"!! ITexture::SaveToTGA() - Cannot lock texture".to_vec()
        }
    }
}
/// Opaque host handle for a `dtaOpenWrite` TGA stream. Byte buffers stay at
/// this platform/file boundary per the compatibility policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TgaFile(pub u64);
/// Level-0 lock snapshot for SaveToTGA. The original reads live surface rows
/// while writing; no mutating callback runs between its Lock and per-row
/// writes, so capturing bytes plus pitch at lock time preserves order and
/// values.
pub enum TgaLevelLock {
    Locked {
        data: Vec<u8>,
        pitch: u32,
    },
    /// Native `Lock` null path: log `Cannot lock texture`, keep the file
    /// open, return false. The leaked handle is preserved, not repaired.
    Failed,
}
/// Opaque host handle for a `dtaOpen` presaved probe stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PresavedFile(pub u64);
/// GPU/file effects for `Duplicate` (RVA 0x75d10). `None` capability rejects
/// before effects. `clone_pixels`/`duplicate_graphics` returning `None` are
/// clean native allocation failures (OUTOFMEM / host allocation); only `Err`
/// poisons.
pub trait TextureDuplicateEffects {
    type Error;
    fn clone_pixels(
        &mut self,
        src: PixelAllocation,
        bytes: u32,
    ) -> Result<Option<PixelAllocation>, Self::Error>;
    /// `CreateTexture` equivalent: managed pool (1), usage 0, then per-level
    /// copies via [`TextureDuplicateEffects::copy_graphics_levels`].
    fn duplicate_graphics(
        &mut self,
        src: TextureGraphics,
        width: u16,
        height: u16,
        levels: u16,
        format: u32,
    ) -> Result<Option<TextureGraphics>, Self::Error>;
    fn copy_graphics_levels(
        &mut self,
        src: TextureGraphics,
        dst: TextureGraphics,
        levels: u16,
    ) -> Result<(), Self::Error>;
    /// `Preload` equivalent (vtable +0x24): driver-upload draw, after the
    /// uncached-list push and `m_Init = true`.
    fn preload_texture(&mut self, graphics: TextureGraphics) -> Result<(), Self::Error>;
}
/// GPU/file effects for `SaveToTGA` (RVA 0x78e40). `open_tga_write` `None`
/// is the clean create-failure path; `TgaLevelLock::Failed` is the clean
/// lock-failure path with the file left open, as in the original.
pub trait TextureTgaEffects {
    type Error;
    fn open_tga_write(&mut self, name: &[u8]) -> Result<Option<TgaFile>, Self::Error>;
    fn write_tga(&mut self, file: TgaFile, bytes: &[u8]) -> Result<(), Self::Error>;
    fn close_tga(&mut self, file: TgaFile) -> Result<(), Self::Error>;
    fn lock_level_zero(&mut self, graphics: TextureGraphics) -> Result<TgaLevelLock, Self::Error>;
    fn unlock_level_zero(&mut self, graphics: TextureGraphics) -> Result<(), Self::Error>;
}
/// File/surface effects for the presaved probes (RVAs 0x78190/0x78430).
/// `open_presaved` `None` is `DTA_INVALID_HANDLE` (clean miss, no close).
/// Reads are exact-size: short data is a backend error, since the original
/// performs unchecked full `dtaRead`s. FILETIME payloads are read and
/// discarded -- the original never compares them (no `CompareFileTime` edge
/// in either body).
pub trait TexturePresavedEffects {
    type Error;
    fn open_presaved(&mut self, name: &[u8]) -> Result<Option<PresavedFile>, Self::Error>;
    fn read_presaved(&mut self, file: PresavedFile, out: &mut [u8]) -> Result<(), Self::Error>;
    fn close_presaved(&mut self, file: PresavedFile) -> Result<(), Self::Error>;
    /// `GetSurfaceLevel(i)->GetDesc().Size` per level; summed into `m_MemSize`
    /// with 32-bit wrapping, as in the original accumulation.
    fn presaved_level_size(
        &mut self,
        graphics: TextureGraphics,
        level: u16,
    ) -> Result<u32, Self::Error>;
    /// `LockRect`/`dtaRead`/`UnlockRect` triple as one ordered effect. The
    /// original checks no lock status here, so there is no failure path.
    fn write_presaved_level(
        &mut self,
        graphics: TextureGraphics,
        level: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observed<T> {
    Unavailable,
    Value(T),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureState {
    Live,
    Destructed,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ITexture {
    pub uid: u16,
    pub graphics: Option<TextureGraphics>,
    pub lock_surface: Option<TextureSurface>,
    pub initialized: bool,
    pub references: u32,
    pub external_references: u32,
    pub material_references: u32,
    pub file_name: CStrKey,
    pub name: Observed<Option<RepView>>,
    pub alpha_file_name: CStrKey,
    pub alpha_name: Observed<Option<RepView>>,
    pub width: Observed<u16>,
    pub height: Observed<u16>,
    pub format: Observed<u32>,
    pub flags: u32,
    pub creation_flags: u32,
    pub color_key: Observed<u32>,
    pub pixels: Option<PixelAllocation>,
    /// Width is an upstream type projection; lifecycle code does not access it.
    /// Constructor leaves all four bytes at +0x40..43 unwritten; high padding
    /// is excluded, and Close preserves this selected low-word projection.
    pub levels: Observed<u16>,
    pub memory_size: u32,
    pub managed_record: Option<ManagedRecordKey>,
    pub state: TextureState,
    token: vr::TextureKey,
    intake_installed: bool,
    managed_admitted: bool,
    managed_prepared: bool,
    managed_created: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum AlphaName<'a> {
    Null,
    Bytes(&'a [u8]),
    ShareFile,
}
/// Fresh ownership intake, not native Create or data upload. Each supplied COM
/// handle transfers one real owned reference; pixels transfer unique ownership.
pub struct TextureIntake<'a> {
    pub uid: u16,
    pub graphics: Option<TextureGraphics>,
    pub lock_surface: Option<TextureSurface>,
    pub pixels: Option<PixelAllocation>,
    pub initialized: bool,
    pub width: u16,
    pub height: u16,
    pub format: u32,
    pub flags: u32,
    pub creation_flags: u32,
    pub color_key: u32,
    pub levels: u16,
    pub memory_size: u32,
    pub file_name: Option<&'a [u8]>,
    pub name_offset: Option<u32>,
    pub alpha_file_name: AlphaName<'a>,
    pub alpha_name_offset: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheList {
    Cached,
    Uncached,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureError {
    Poisoned,
    Object(ObjectError),
    String(StringError),
    ForeignArena,
    UnknownTexture(TextureKey),
    InvalidState,
    ManagedUnsupported,
    ReferenceOverflow,
    ReferenceOwned,
    InvalidIdentity,
    IntakeAlreadyInstalled,
    IntakeAllocationRejected,
    InvalidNameView,
    DuplicateCache,
    HostAllocation,
    IdentityOverflow,
    UnknownExternalString,
    Manager(ManagerError),
}
impl From<ManagerError> for TextureError {
    fn from(e: ManagerError) -> Self {
        Self::Manager(e)
    }
}
impl From<ObjectError> for TextureError {
    fn from(e: ObjectError) -> Self {
        Self::Object(e)
    }
}
impl From<StringError> for TextureError {
    fn from(e: StringError) -> Self {
        Self::String(e)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum TextureOperationError<E> {
    State(TextureError),
    Backend(E),
    Manager(ManagedOperationError<E>),
}
impl<E> From<ManagerError> for TextureOperationError<E> {
    fn from(e: ManagerError) -> Self {
        Self::State(e.into())
    }
}
impl<E> From<TextureError> for TextureOperationError<E> {
    fn from(e: TextureError) -> Self {
        Self::State(e)
    }
}
impl<E> From<StringError> for TextureOperationError<E> {
    fn from(e: StringError) -> Self {
        Self::State(e.into())
    }
}
impl<E> From<ObjectError> for TextureOperationError<E> {
    fn from(e: ObjectError) -> Self {
        Self::State(e.into())
    }
}
fn string_result<T, E>(
    r: Result<T, StringOperationError<E>>,
) -> Result<T, TextureOperationError<E>> {
    r.map_err(|e| match e {
        StringOperationError::State(e) => e.into(),
        StringOperationError::Backend(e) => TextureOperationError::Backend(e),
    })
}
/// filename-builder error projection for the presaved probes. The builders
/// only fail on <4B source names with `Unsupported("presaved filename")`
/// (native `strlen - 4` underflow); the port refuses with `InvalidNameView`
/// instead of faulting. Any other factory error is unreachable from these
/// builders and maps the same way rather than forging a native word.
fn presaved_name_error<E>(error: TextureFactoryError) -> TextureOperationError<E> {
    match error {
        TextureFactoryError::Unsupported(_) => {
            TextureOperationError::State(TextureError::InvalidNameView)
        }
        TextureFactoryError::State(error) => TextureOperationError::State(error),
        _ => TextureOperationError::State(TextureError::InvalidNameView),
    }
}
struct Textures {
    identity: u64,
    slots: Vec<Option<ITexture>>,
    cached: Vec<TextureKey>,
    uncached: Vec<TextureKey>,
    poisoned: bool,
    manager: TextureManager,
}
impl Textures {
    fn texture(&self, k: TextureKey) -> Result<&ITexture, TextureError> {
        if k.arena != self.identity {
            return Err(TextureError::ForeignArena);
        }
        self.slots
            .get(k.slot.wrapping_sub(1) as usize)
            .and_then(Option::as_ref)
            .ok_or(TextureError::UnknownTexture(k))
    }
    fn texture_mut(&mut self, k: TextureKey) -> &mut ITexture {
        self.slots[(k.slot - 1) as usize].as_mut().unwrap()
    }
    fn live(&self, k: TextureKey) -> Result<&ITexture, TextureError> {
        let t = self.texture(k)?;
        if t.state != TextureState::Live {
            return Err(TextureError::InvalidState);
        }
        Ok(t)
    }
    fn token(&self, token: vr::TextureKey) -> Result<TextureKey, TextureError> {
        self.slots
            .iter()
            .position(|r| r.as_ref().is_some_and(|r| r.token == token))
            .map(|i| TextureKey {
                arena: self.identity,
                slot: i as u32 + 1,
            })
            .ok_or(TextureError::InvalidIdentity)
    }
    fn domain(&self, k: TextureKey) -> Result<(), TextureError> {
        let t = self.live(k)?;
        if t.managed_created && t.flags & 0x40 == 0 {
            if t.graphics.is_some() || t.flags & 0xc0000000 != 0 {
                return Err(TextureError::ManagedUnsupported);
            }
            return Ok(());
        }
        if t.managed_prepared && !t.managed_created {
            if t.graphics.is_none() && t.managed_record.is_none() && !self.manager.has_links(k) {
                return Ok(());
            }
            return Err(TextureError::ManagedUnsupported);
        }
        if t.managed_admitted {
            self.manager
                .validate_lifetime(k, t.flags, t.managed_record, t.graphics)?;
            Ok(())
        } else if t.flags & 0xc0000040 != 0
            || t.managed_record.is_some()
            || self.manager.has_links(k)
        {
            Err(TextureError::ManagedUnsupported)
        } else {
            Ok(())
        }
    }
    fn preflight_close<B: TextureBackend + ?Sized>(
        &self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<(), TextureError> {
        self.domain(k)?;
        let t = self.texture(k)?;
        if t.managed_created && t.flags & 0x40 == 0 && self.manager.has_links(k) {
            return Err(TextureError::ManagedUnsupported);
        }
        if t.managed_admitted || (t.managed_prepared && t.flags & 0x40 != 0) {
            self.manager
                .validate_close(k, t.flags, t.managed_record, t.graphics)?;
            let effects = b
                .managed_texture_effects()
                .ok_or(TextureError::ManagedUnsupported)?;
            if self.manager.requires_creation_effects() && effects.creation_effects().is_none() {
                return Err(TextureError::ManagedUnsupported);
            }
        }
        Ok(())
    }
    fn remove_managed<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.domain(k)?;
        let effects = b
            .managed_texture_effects()
            .ok_or(TextureError::ManagedUnsupported)?;
        let flags = &mut self.slots[(k.slot - 1) as usize].as_mut().unwrap().flags;
        self.manager
            .remove(k, flags, effects)
            .map_err(TextureOperationError::Manager)
    }
    fn can_retain(&self, k: TextureKey) -> Result<(), TextureError> {
        if self.live(k)?.references >= i32::MAX as u32 {
            Err(TextureError::ReferenceOverflow)
        } else {
            Ok(())
        }
    }
    fn retain(&mut self, k: TextureKey, material: bool) -> Result<(), TextureError> {
        self.domain(k)?;
        self.can_retain(k)?;
        let t = self.texture_mut(k);
        t.references += 1;
        if material {
            t.material_references += 1;
        } else {
            t.external_references += 1;
        }
        Ok(())
    }
    fn call<T, E>(&mut self, r: Result<T, E>) -> Result<T, TextureOperationError<E>> {
        r.map_err(|e| {
            self.poisoned = true;
            TextureOperationError::Backend(e)
        })
    }
    /// Best-effort presaved-stream close before a backend failure propagates.
    /// Header-miss, levels-0, graphics-None and OOM paths all close first;
    /// backend-error paths (host-only: native reads are unchecked) do the
    /// same so no path leaks an open stream. A close failure is discarded —
    /// the original error wins. Poisoning stays with the caller (via the
    /// outer `call` or the method tail), exactly like the sibling ports.
    fn close_presaved_on_error<T, E>(
        &mut self,
        fx: &mut dyn TexturePresavedEffects<Error = E>,
        file: PresavedFile,
        op: Result<T, TextureOperationError<E>>,
    ) -> Result<T, TextureOperationError<E>> {
        match op {
            ok @ Ok(_) => ok,
            Err(error) => {
                let _ = self.call(fx.close_presaved(file));
                Err(error)
            }
        }
    }
    fn unlock<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        if let Some(surface) = self.texture(k)?.lock_surface {
            self.call(b.unlock_surface(surface))?;
            let surface = self
                .texture(k)?
                .lock_surface
                .ok_or(TextureError::InvalidState)?;
            self.call(b.release_surface(surface))?;
            self.texture_mut(k).lock_surface = None;
        }
        Ok(())
    }
    fn close<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        strings: &mut CStrings,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.preflight_close(k, b)?;
        let result = (|| {
            self.unlock(k, b)?;
            if self.texture(k)?.flags & 0x40 != 0 {
                self.remove_managed(k, b)?;
                let t = self.texture_mut(k);
                t.managed_record = None;
                t.graphics = None;
                t.managed_admitted = false;
                t.managed_prepared = false;
                t.managed_created = false;
            }

            if let Some(pixels) = self.texture(k)?.pixels {
                self.call(b.delete_pixels(pixels))?;
                self.texture_mut(k).pixels = None;
            }
            let t = self.texture_mut(k);
            t.width = Observed::Value(0);
            t.height = Observed::Value(0);
            t.format = Observed::Value(0);
            t.flags = 0;
            t.color_key = Observed::Value(0);
            t.memory_size = 0;
            let (file, alpha) = (t.file_name, t.alpha_file_name);
            string_result(strings.assign(file, Some(b""), b))?;
            self.texture_mut(k).name = Observed::Value(None);
            string_result(strings.assign(alpha, Some(b""), b))?;
            self.texture_mut(k).alpha_name = Observed::Value(None);
            if let Some(i) = self.cached.iter().position(|x| *x == k) {
                self.cached.swap_remove(i);
            }
            if let Some(i) = self.uncached.iter().position(|x| *x == k) {
                self.uncached.swap_remove(i);
            }
            if let Some(graphics) = self.texture(k)?.graphics {
                self.call(b.release_texture_graphics(graphics))?;
                self.texture_mut(k).graphics = None;
            }
            self.texture_mut(k).initialized = false;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn destruct<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        strings: &mut CStrings,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.preflight_close(k, b)?;
        let result = (|| {
            self.close(k, strings, b)?;
            let t = self.texture(k)?;
            let (alpha, file) = (t.alpha_file_name, t.file_name);
            string_result(strings.destruct_string(alpha, b))?;
            string_result(strings.destruct_string(file, b))?;
            self.texture_mut(k).state = TextureState::Destructed;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn release<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        material: bool,
        strings: &mut CStrings,
        b: &mut B,
    ) -> Result<u32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.domain(k)?;
        let t = self.texture(k)?;
        if (if material {
            t.material_references
        } else {
            t.external_references
        }) == 0
        {
            return Err(TextureError::ReferenceOwned.into());
        }
        if t.references == 1 {
            self.preflight_close(k, b)?;
        }
        let t = self.texture_mut(k);
        t.references -= 1;
        if material {
            t.material_references -= 1;
        } else {
            t.external_references -= 1;
        }
        let remaining = t.references;
        if remaining != 0 {
            return Ok(remaining);
        }
        self.destruct(k, strings, b)?;
        self.slots[(k.slot - 1) as usize] = None;
        Ok(0)
    }
}
/// Concrete host integration. Private worlds protect material-held texture
/// references and embedded C_str fields from external release/mutation.
pub struct TextureResources {
    objects: ObjectResources,
    strings: CStrings,
    textures: Textures,
    external_strings: Vec<CStrKey>,
    next_texture_uid: u16,
    poisoned: bool,
}
impl TextureResources {
    pub fn new() -> Result<Self, TextureError> {
        let objects = ObjectResources::new()?;
        let identity = objects.managed().resources().identity();
        Ok(Self {
            objects,
            strings: CStrings::new()?,
            textures: Textures {
                identity,
                slots: Vec::new(),
                cached: Vec::new(),
                uncached: Vec::new(),
                poisoned: false,
                manager: TextureManager::new(identity),
            },
            external_strings: Vec::new(),
            next_texture_uid: 0,
            poisoned: false,
        })
    }
    pub fn objects(&self) -> &ObjectResources {
        &self.objects
    }
    pub fn strings(&self) -> &CStrings {
        &self.strings
    }
    pub fn texture(&self, k: TextureKey) -> Result<&ITexture, TextureError> {
        self.textures.texture(k)
    }
    pub fn cached(&self) -> &[TextureKey] {
        &self.textures.cached
    }
    pub fn uncached(&self) -> &[TextureKey] {
        &self.textures.uncached
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
            || self.objects.is_poisoned()
            || self.textures.poisoned
            || self.textures.manager.is_poisoned()
            || self.strings.is_poisoned()
    }
    fn available(&self) -> Result<(), TextureError> {
        if self.is_poisoned() {
            Err(TextureError::Poisoned)
        } else {
            Ok(())
        }
    }
    pub fn create_texture(&mut self) -> Result<TextureKey, TextureError> {
        self.available()?;
        let slot = u32::try_from(self.textures.slots.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(TextureError::IdentityOverflow)?;
        self.textures
            .slots
            .try_reserve(1)
            .map_err(|_| TextureError::HostAllocation)?;
        self.strings.reserve_strings(2)?;
        let token = NEXT_TOKEN
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| TextureError::IdentityOverflow)?;
        let file = self.strings.create_null()?;
        let alpha = self.strings.create_null()?;
        self.textures.slots.push(Some(ITexture {
            uid: 0,
            graphics: None,
            lock_surface: None,
            initialized: false,
            references: 1,
            external_references: 1,
            material_references: 0,
            file_name: file,
            name: Observed::Unavailable,
            alpha_file_name: alpha,
            alpha_name: Observed::Unavailable,
            width: Observed::Unavailable,
            height: Observed::Unavailable,
            format: Observed::Unavailable,
            flags: 0,
            creation_flags: 0,
            color_key: Observed::Unavailable,
            pixels: None,
            levels: Observed::Unavailable,
            memory_size: 0,
            managed_record: None,
            state: TextureState::Live,
            token: vr::TextureKey(token),
            intake_installed: false,
            managed_admitted: false,
            managed_prepared: false,
            managed_created: false,
        }));
        Ok(TextureKey {
            arena: self.textures.identity,
            slot,
        })
    }
    fn check_view(bytes: Option<&[u8]>, offset: Option<u32>) -> Result<(), TextureError> {
        if let Some(offset) = offset {
            let bytes = bytes.ok_or(TextureError::InvalidNameView)?;
            let len = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            if offset as usize > len {
                return Err(TextureError::InvalidNameView);
            }
        }
        Ok(())
    }
    /// Fresh intake adopts one reference per COM slot, a unique pixel block and
    /// independently owned names. After preflight, payload ownership transfers
    /// before string callbacks, and remains recorded on any subsequent error.
    /// It is not file loading or original Create.
    pub fn install_texture<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        input: TextureIntake<'_>,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        self.textures.domain(k)?;
        let t = self.textures.live(k)?;
        if t.intake_installed
            || t.managed_admitted
            || t.managed_prepared
            || t.managed_created
            || t.references != 1
            || t.external_references != 1
        {
            return Err(TextureError::IntakeAlreadyInstalled.into());
        }
        if input.flags & 0xc0000040 != 0 {
            return Err(TextureError::ManagedUnsupported.into());
        }
        if input.graphics.is_some_and(|x| x.0 == 0)
            || input.lock_surface.is_some_and(|x| x.0 == 0)
            || input.pixels.is_some_and(|x| x.0 == 0)
        {
            return Err(TextureError::InvalidIdentity.into());
        }
        if input.pixels.is_some()
            && self
                .textures
                .slots
                .iter()
                .flatten()
                .any(|t| t.pixels == input.pixels)
        {
            return Err(TextureError::InvalidIdentity.into());
        }
        let alpha = match input.alpha_file_name {
            AlphaName::Null => None,
            AlphaName::Bytes(s) => Some(s),
            AlphaName::ShareFile => input.file_name,
        };
        Self::check_view(input.file_name, input.name_offset)?;
        Self::check_view(alpha, input.alpha_name_offset)?;
        if let Some(bytes) = input.file_name {
            CStrings::validate_source(bytes)?;
        }
        if let Some(bytes) = alpha {
            CStrings::validate_source(bytes)?;
        }
        let (file_key, alpha_key) = (t.file_name, t.alpha_file_name);
        // Ownership transfers here, before the first external string callback.
        // An interrupted intake retains these adopted handles diagnostically;
        // it never silently leaves them outside a poisoned arena's records.
        let t = self.textures.texture_mut(k);
        t.uid = input.uid;
        t.graphics = input.graphics;
        t.lock_surface = input.lock_surface;
        t.pixels = input.pixels;
        t.initialized = input.initialized;
        t.width = Observed::Value(input.width);
        t.height = Observed::Value(input.height);
        t.format = Observed::Value(input.format);
        t.flags = input.flags;
        t.creation_flags = input.creation_flags;
        t.color_key = Observed::Value(input.color_key);
        t.levels = Observed::Value(input.levels);
        t.memory_size = input.memory_size;
        t.intake_installed = true;
        let result = (|| {
            string_result(self.strings.assign(file_key, input.file_name, b))?;
            match input.alpha_file_name {
                AlphaName::ShareFile => {
                    string_result(self.strings.alias_into(alpha_key, file_key, b))?
                }
                _ => string_result(self.strings.assign(alpha_key, alpha, b))?,
            }
            let file_rep = self.strings.string(file_key)?.rep;
            let alpha_rep = self.strings.string(alpha_key)?.rep;
            let name = match input.name_offset {
                Some(offset) => Some(RepView {
                    rep: file_rep.ok_or(TextureError::IntakeAllocationRejected)?,
                    offset,
                }),
                None => None,
            };
            let alpha_name = match input.alpha_name_offset {
                Some(offset) => Some(RepView {
                    rep: alpha_rep.ok_or(TextureError::IntakeAllocationRejected)?,
                    offset,
                }),
                None => None,
            };
            t.name = Observed::Value(name);
            t.alpha_name = Observed::Value(alpha_name);
            t.intake_installed = true;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Host cache intake is unique per list; native duplicate-membership states
    /// that would leave a retired pointer after one removal are not admitted.
    pub fn register_cache(&mut self, k: TextureKey, list: CacheList) -> Result<(), TextureError> {
        self.available()?;
        self.textures.domain(k)?;
        let list = match list {
            CacheList::Cached => &mut self.textures.cached,
            CacheList::Uncached => &mut self.textures.uncached,
        };
        if list.contains(&k) {
            return Err(TextureError::DuplicateCache);
        }
        list.try_reserve(1)
            .map_err(|_| TextureError::HostAllocation)?;
        list.push(k);
        Ok(())
    }
    pub fn texture_manager(&self) -> &TextureManager {
        &self.textures.manager
    }
    fn scalar_allocations(&self) -> Result<Vec<u64>, TextureError> {
        let mut values = Vec::new();
        let count = self.strings.allocation_tokens().count() + self.textures.slots.len();
        values
            .try_reserve(count)
            .map_err(|_| TextureError::HostAllocation)?;
        values.extend(self.strings.allocation_tokens());
        values.extend(
            self.textures
                .slots
                .iter()
                .flatten()
                .filter_map(|t| t.pixels.map(|p| p.0)),
        );
        Ok(values)
    }
    /// Adopts explicit pointer-array storage. No constructor defaults are guessed;
    /// active seeded entries must already occupy its active prefix. Drop cannot
    /// free the final array; explicit whole-manager shutdown remains unsupported.
    pub fn configure_managed_registry(
        &mut self,
        input: RegistryIntake,
    ) -> Result<(), TextureError> {
        self.available()?;
        let forbidden = self.scalar_allocations()?;
        self.textures
            .manager
            .configure_registry(input, &forbidden)?;
        Ok(())
    }
    /// Host caller preparation only: flags/create_flags are assigned separately
    /// from Add, which never writes them or initialized. Accepts low flags with
    /// managed0x40 optional; fresh flags0 supports direct Add/remove diagnostics.
    /// No prior texture payload/weak links may be overwritten.
    pub fn prepare_managed_texture(
        &mut self,
        key: TextureKey,
        flags: u32,
    ) -> Result<(), TextureError> {
        self.available()?;
        let t = self.textures.live(key)?;
        if flags & 0xc0000000 != 0
            || t.managed_prepared
            || t.managed_created
            || t.managed_admitted
            || t.intake_installed
            || t.references != 1
            || t.external_references != 1
            || t.graphics.is_some()
            || t.lock_surface.is_some()
            || t.pixels.is_some()
            || t.managed_record.is_some()
            || self.textures.manager.has_links(key)
        {
            return Err(TextureError::ManagedUnsupported);
        }
        let t = self.textures.texture_mut(key);
        t.flags = flags;
        t.creation_flags = flags;
        t.managed_prepared = true;
        Ok(())
    }
    /// Original diagnostic-supported AddTextureToTextureManager (RVA0x7ad20).
    /// Source is raw bytes excluding NUL, length5..253. Formats and registry are
    /// serialized stable snapshots: callback reentry/global mutation is excluded.
    /// Caller state preparation is separate; this entry leaves flags/init intact.
    /// Record/list allocation None is host unsupported+poison, filename None is
    /// genuine false after record delete/diagnostic. After effects start, errors
    /// retain all published/pending ownership without rollback.
    #[allow(clippy::too_many_arguments)]
    pub fn add_texture_to_texture_manager<B: TextureBackend + ?Sized>(
        &mut self,
        key: TextureKey,
        name: &[u8],
        flags: u32,
        formats: CreationFormats,
        b: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let t = self.textures.live(key)?;
        if !t.managed_prepared
            || t.managed_created
            || t.managed_record.is_some()
            || self.textures.manager.has_links(key)
        {
            return Err(TextureError::ManagedUnsupported.into());
        }
        self.textures.manager.preflight_add(name)?;
        let forbidden = self.scalar_allocations()?;
        let effects = b
            .managed_texture_effects()
            .ok_or(TextureError::ManagedUnsupported)?;
        if effects.creation_effects().is_none() {
            return Err(TextureError::ManagedUnsupported.into());
        }
        let t = self.textures.slots[(key.slot - 1) as usize]
            .as_mut()
            .unwrap();
        self.textures
            .manager
            .add(key, name, flags, formats, &forbidden, effects, |record| {
                t.graphics = None;
                t.managed_record = Some(record);
                t.managed_created = true;
                t.managed_admitted = t.flags & 0x40 != 0;
            })
            .map_err(TextureOperationError::Manager)
    }
    /// Atomic seeded-world ownership intake, not Add/Request/worker execution.
    /// Existing texture graphics must be null: pool graphics are adopted as
    /// separately owned references, then the texture receives only a borrow.
    pub fn install_managed_world(&mut self, input: ManagedWorldIntake) -> Result<(), TextureError> {
        self.install_managed_world_inner(input, false)
    }
    /// Explicit cleanup-world intake. Unlike normal world intake, permits live
    /// pool owners whose loaded bit is already clear. All ownership, allocation,
    /// queue and lifetime guards remain; removal guards are unchanged.
    pub fn install_managed_unload_world(
        &mut self,
        input: ManagedWorldIntake,
    ) -> Result<(), TextureError> {
        self.install_managed_world_inner(input, true)
    }
    fn install_managed_world_inner(
        &mut self,
        input: ManagedWorldIntake,
        allow_unloaded_owners: bool,
    ) -> Result<(), TextureError> {
        self.available()?;
        for binding in &input.bindings {
            let t = self.textures.live(binding.texture)?;
            if t.managed_admitted
                || t.managed_prepared
                || t.managed_created
                || t.graphics.is_some()
                || t.managed_record.is_some()
                || t.flags & 0xc0000040 != 0
            {
                return Err(TextureError::ManagedUnsupported);
            }
        }
        // These scalar allocations share the backend's unique live-allocation
        // identity domain. Never adopt a buffer already owned by C_str/pixels.
        let aliases = |raw: u64| {
            self.strings.owns_allocation(raw)
                || self
                    .textures
                    .slots
                    .iter()
                    .flatten()
                    .any(|t| t.pixels.is_some_and(|p| p.0 == raw))
        };
        for record in &input.records {
            if aliases(record.allocation.0) || record.file_name.is_some_and(|a| aliases(a.0)) {
                return Err(ManagerError::InvalidIdentity.into());
            }
        }
        let bindings = self
            .textures
            .manager
            .install(input, allow_unloaded_owners)?;
        for binding in bindings {
            let t = self.textures.texture_mut(binding.texture);
            t.flags = binding.flags;
            t.managed_record = binding.record;
            t.graphics = binding.graphics;
            t.managed_admitted = true;
        }
        Ok(())
    }
    /// Finite Request producer (upstream working name), PC RVA0x7a910.
    /// Returns current availability; enqueueing returns false. Frame is copied
    /// bit-for-bit to record+0x10 before testing loaded/queued/full state.
    pub fn request_texture<B: TextureBackend + ?Sized>(
        &mut self,
        key: TextureKey,
        frame_counter: u32,
        b: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let t = self.textures.live(key)?;
        if t.flags & 0x40 == 0 {
            return Ok(true);
        }
        let record = t.managed_record;
        let effects = b
            .managed_texture_effects()
            .ok_or(TextureError::ManagedUnsupported)?;
        let flags = &mut self.textures.slots[(key.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .flags;
        self.textures
            .manager
            .request(key, record, flags, frame_counter, effects)
            .map_err(TextureOperationError::Manager)
    }
    /// Admits typed fields from existing backend record storage for the worker.
    pub fn configure_managed_worker_records(
        &mut self,
        input: &[(
            crate::texture_manager::RecordId,
            crate::texture_manager::ManagedRecordFields,
        )],
    ) -> Result<(), TextureError> {
        self.available()?;
        self.textures.manager.configure_worker_records(input)?;
        Ok(())
    }
    /// Complete finite scheduled RenderThreadProc (upstream working name),
    /// PC RVA0x7a9b0. Requires an explicit terminating backend running schedule.
    pub fn run_managed_texture_worker<B: TextureBackend + ?Sized>(
        &mut self,
        input: crate::texture_manager::WorkerInput,
        b: &mut B,
    ) -> Result<u32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        for key in self.textures.manager.unconsumed_queue_owners().chain(
            self.textures
                .manager
                .pools()
                .iter()
                .flatten()
                .filter_map(|s| s.owner),
        ) {
            self.textures.live(key)?;
        }
        let forbidden = self.scalar_allocations()?;
        let effects = b
            .managed_texture_effects()
            .ok_or(TextureError::ManagedUnsupported)?;
        self.textures
            .manager
            .worker(input, &forbidden, &mut self.textures.slots, effects)
            .map_err(TextureOperationError::Manager)
    }
    /// Adopts release-specific system references and native pool-array storage.
    pub fn configure_managed_release(
        &mut self,
        input: crate::texture_manager::ManagedReleaseIntake,
    ) -> Result<(), TextureError> {
        self.available()?;
        let forbidden = self.scalar_allocations()?;
        self.textures.manager.configure_release(input, &forbidden)?;
        Ok(())
    }
    /// Finite RestoreManagedTextures (upstream working name), PC RVA0x7a710.
    pub fn restore_managed_textures<B: TextureBackend + ?Sized>(
        &mut self,
        input: crate::texture_manager::RestoreInput,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        for owner in self
            .textures
            .manager
            .pools()
            .iter()
            .flatten()
            .filter_map(|s| s.owner)
        {
            self.textures.live(owner)?;
        }
        let forbidden = self.scalar_allocations()?;
        let effects = b
            .managed_texture_effects()
            .ok_or(TextureError::ManagedUnsupported)?;
        let slots = &mut self.textures.slots;
        self.textures
            .manager
            .restore(
                input,
                &forbidden,
                |owner| {
                    slots[(owner.slot - 1) as usize].as_mut().unwrap().flags &= 0x7fffffff;
                },
                effects,
            )
            .map_err(TextureOperationError::Manager)
    }
    /// Finite ReleaseManagedTextures (upstream working name), PC RVA0x7a610.
    /// Borrowed texture graphics, records and queue entries survive teardown.
    pub fn release_managed_textures<B: TextureBackend + ?Sized>(
        &mut self,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        for owner in self
            .textures
            .manager
            .pools()
            .iter()
            .flatten()
            .filter_map(|s| s.owner)
        {
            self.textures.live(owner)?;
        }
        let effects = b
            .managed_texture_effects()
            .ok_or(TextureError::ManagedUnsupported)?;
        let slots = &mut self.textures.slots;
        self.textures
            .manager
            .release(
                |owner| {
                    slots[(owner.slot - 1) as usize].as_mut().unwrap().flags &= 0x7fffffff;
                },
                effects,
            )
            .map_err(TextureOperationError::Manager)
    }
    /// Standalone UnloadManagedTextures (upstream working name), PC RVA0x7afe0.
    /// Clears loaded flags and weak owners only; borrowed graphics survive.
    pub fn unload_managed_textures<B: TextureBackend + ?Sized>(
        &mut self,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        // Cleanup accepts already-unloaded owners, but never dead identities.
        for owner in self
            .textures
            .manager
            .pools()
            .iter()
            .flatten()
            .filter_map(|s| s.owner)
        {
            self.textures.live(owner)?;
        }
        let effects = b
            .managed_texture_effects()
            .ok_or(TextureError::ManagedUnsupported)?;
        let slots = &mut self.textures.slots;
        self.textures
            .manager
            .unload(
                |owner| {
                    slots[(owner.slot - 1) as usize].as_mut().unwrap().flags &= 0x7fffffff;
                },
                effects,
            )
            .map_err(TextureOperationError::Manager)
    }
    /// Original manager-removal projection. It preserves the texture's raw
    /// managed-record and borrowed graphics pointers, including stale identity.
    pub fn remove_managed_texture<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        self.textures.remove_managed(k, b)
    }
    pub fn retain_texture(&mut self, k: TextureKey) -> Result<(), TextureError> {
        self.available()?;
        self.textures.retain(k, false)
    }
    pub fn release_texture<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<u32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        self.textures.release(k, false, &mut self.strings, b)
    }
    pub fn unlock_texture<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        self.textures.domain(k)?;
        self.textures.unlock(k, b)
    }
    pub fn close_texture<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        self.textures.close(k, &mut self.strings, b)
    }
    fn req_value<T: Copy>(v: Observed<T>) -> Result<T, TextureError> {
        match v {
            Observed::Value(x) => Ok(x),
            _ => Err(TextureError::InvalidState),
        }
    }
    /// Original `Duplicate` projection, PC RVA 0x75d10 (vtable slot 1).
    /// Order-exact: Close(dst); null/uninit src returns I3D_OK with no
    /// further effects; flags/create-flags/C_str shares/dims/colorkey;
    /// pixels clone (clean I3DERR_OUTOFMEM, effects retained); levels and
    /// memsize; graphics duplicate + level copies (absent source graphics
    /// is InvalidState; a refused device allocation is HostAllocation
    /// instead of the native null-dereference; effects retained in both
    /// cases); uncached push; init=true; Preload. Returns the LS3D_RESULT
    /// code, never faults.
    pub fn duplicate_texture<B: TextureBackend + ?Sized>(
        &mut self,
        dst: TextureKey,
        src: Option<TextureKey>,
        b: &mut B,
    ) -> Result<u32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        self.textures.close(dst, &mut self.strings, b)?;
        let Some(src) = src else { return Ok(I3D_OK) };
        self.textures.live(src)?;
        if !self.textures.texture(src)?.initialized {
            return Ok(I3D_OK);
        }
        // Scalar snapshot before any mutation; reads happen when the original
        // reads them (flags/names first, dims/pixels next, levels last).
        let snap = {
            let t = self.textures.texture(src)?;
            (
                t.flags,
                t.creation_flags,
                t.file_name,
                t.alpha_file_name,
                t.name,
                t.alpha_name,
                t.width,
                t.height,
                t.color_key,
                t.pixels,
                t.levels,
                t.memory_size,
                t.graphics,
                t.format,
            )
        };
        let result = (|| {
            let d = self.textures.texture_mut(dst);
            d.flags = snap.0;
            d.creation_flags = snap.1;
            // C_str refcount-share: alias the source reps, copying the
            // basename views onto the shared bytes.
            let (dst_file, dst_alpha) = {
                let d = self.textures.texture(dst)?;
                (d.file_name, d.alpha_file_name)
            };
            let src_file_rep = self.strings.string(snap.2)?.rep;
            if src_file_rep.is_some() {
                string_result(self.strings.alias_into(dst_file, snap.2, b))?;
            } else {
                string_result(self.strings.assign(dst_file, None, b))?;
            }
            let src_alpha_rep = self.strings.string(snap.3)?.rep;
            if src_alpha_rep.is_some() {
                string_result(self.strings.alias_into(dst_alpha, snap.3, b))?;
            } else {
                string_result(self.strings.assign(dst_alpha, None, b))?;
            }
            let d = self.textures.texture_mut(dst);
            d.name = snap.4;
            d.alpha_name = snap.5;
            d.width = snap.6;
            d.height = snap.7;
            d.color_key = snap.8;
            let (w, h) = (Self::req_value(snap.6)?, Self::req_value(snap.7)?);
            if let Some(src_pixels) = snap.9 {
                let bytes = (w as u32).wrapping_mul(h as u32).wrapping_mul(4);
                let fx = b.duplicate_effects().ok_or(TextureError::HostAllocation)?;
                let cloned = self.textures.call(fx.clone_pixels(src_pixels, bytes))?;
                let Some(cloned) = cloned else {
                    return Ok(I3DERR_OUTOFMEM);
                };
                self.textures.texture_mut(dst).pixels = Some(cloned);
            }
            let d = self.textures.texture_mut(dst);
            d.levels = snap.10;
            d.memory_size = snap.11;
            let format = Self::req_value(snap.13)?;
            let src_graphics = snap.12.ok_or(TextureError::InvalidState)?;
            let levels = Self::req_value(snap.10)?;
            let fx = b.duplicate_effects().ok_or(TextureError::HostAllocation)?;
            let dst_graphics =
                self.textures
                    .call(fx.duplicate_graphics(src_graphics, w, h, levels, format))?;
            let Some(dst_graphics) = dst_graphics else {
                return Err(TextureError::HostAllocation.into());
            };
            self.textures.texture_mut(dst).graphics = Some(dst_graphics);
            if levels != 0 {
                let fx = b.duplicate_effects().ok_or(TextureError::HostAllocation)?;
                self.textures
                    .call(fx.copy_graphics_levels(src_graphics, dst_graphics, levels))?;
            }
            self.register_cache(dst, CacheList::Uncached)?;
            self.textures.texture_mut(dst).initialized = true;
            let fx = b.duplicate_effects().ok_or(TextureError::HostAllocation)?;
            self.textures.call(fx.preload_texture(dst_graphics))?;
            Ok(I3D_OK)
        })();
        if result.is_err() {
            self.poisoned = true;
            self.textures.poisoned = true;
        }
        result
    }
    /// Original `Reload` stub, PC RVA 0x75f30 (vtable slot 5). The 5-insn
    /// body never reads `this`: no key, no state access, single log side
    /// effect, single `I3DERR_UNSUPPORTED` return. Infallible by construction.
    pub fn reload_texture<B: TextureBackend + ?Sized>(&self, b: &mut B) -> u32 {
        b.log_diagnostic(TextureDiagnostic::ReloadNotImplemented);
        I3DERR_UNSUPPORTED
    }
    /// Original `IsInit`, PC RVA 0x78670 (vtable slot 3): direct `m_Init`
    /// read (this+0x10, AL only; upper EAX preserved for callers).
    pub fn is_texture_init(&self, k: TextureKey) -> Result<bool, TextureError> {
        Ok(self.textures.texture(k)?.initialized)
    }
    /// Evidenced format to TGA-bits table (jump table at 0x1007911c with
    /// index bytes at 0x1007912c): 20->24, 21/22/26->32, 23/24/25->16,
    /// 50->24, else 0 (refuse).
    fn tga_bits_per_pixel(format: u32) -> u8 {
        match format {
            D3DFMT_A8R8G8B8 | D3DFMT_X8R8G8B8 | D3DFMT_A4R4G4B4 => 32,
            D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5 => 16,
            D3DFMT_R8G8B8 | D3DFMT_L8 => 24,
            _ => 0,
        }
    }
    /// Original `SaveToTGA`, PC RVA 0x78e40 (vtable slot 13). Level-0
    /// snapshot to uncompressed TGA: DXT1-5 refuse with the
    /// (typo-preserving) log line; R5G6B5 and A4R4G4B4 convert bit-exact
    /// (`(c>>1)&0x7fe0|(c&0x1f)`; nibble-spread `SHL 4` tail); rows run
    /// bottom-up with pitch; `dtaClose` precedes `Unlock`; lock failure keeps
    /// the file open. Zero dimensions write a header-only file and return
    /// true (host extent guard replaces the original's out-of-bounds read).
    pub fn save_texture_to_tga<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        name: &[u8],
        b: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let t = self.textures.live(k)?;
        let (format, width, height, graphics) = (t.format, t.width, t.height, t.graphics);
        let format = Self::req_value(format)?;
        if matches!(
            format,
            D3DFMT_DXT1 | D3DFMT_DXT2 | D3DFMT_DXT3 | D3DFMT_DXT4 | D3DFMT_DXT5
        ) {
            b.log_diagnostic(TextureDiagnostic::TgaCompressedPixelFormat(name.to_vec()));
            return Ok(false);
        }
        let (width, height) = (Self::req_value(width)?, Self::req_value(height)?);
        let bpp = Self::tga_bits_per_pixel(format);
        let mut header = [0u8; 18];
        header[2] = 2;
        header[12..14].copy_from_slice(&width.to_le_bytes());
        header[14..16].copy_from_slice(&height.to_le_bytes());
        header[16] = bpp;
        let result = (|| {
            let fx = b.tga_effects().ok_or(TextureError::HostAllocation)?;
            let Some(file) = self.textures.call(fx.open_tga_write(name))? else {
                b.log_diagnostic(TextureDiagnostic::TgaCannotCreateFile(name.to_vec()));
                return Ok(false);
            };
            self.textures.call(fx.write_tga(file, &header))?;
            let graphics = graphics.ok_or(TextureError::InvalidState)?;
            let lock = self.textures.call(fx.lock_level_zero(graphics))?;
            let TgaLevelLock::Locked { data, pitch } = lock else {
                b.log_diagnostic(TextureDiagnostic::TgaCannotLockTexture);
                return Ok(false);
            };
            // Host extent guard (no native equivalent; the original would
            // read out of bounds). Unlock + close before the clean error.
            // Converted formats read 2 input bytes per pixel; direct rows
            // read the TGA row width.
            let row_bytes = (u32::from(bpp) / 8) * u32::from(width);
            let in_row = if format == D3DFMT_R5G6B5 || format == D3DFMT_A4R4G4B4 {
                u32::from(width) * 2
            } else {
                row_bytes
            };
            let ok = u32::from(height)
                .checked_sub(1)
                .and_then(|h| h.checked_mul(pitch))
                .and_then(|start| start.checked_add(in_row))
                .is_some_and(|end| (end as usize) <= data.len())
                || height == 0;
            if !ok {
                self.textures.call(fx.unlock_level_zero(graphics))?;
                self.textures.call(fx.close_tga(file))?;
                return Err(TextureError::InvalidState.into());
            }
            let mut row16 = vec![0u16; width as usize];
            let mut row32 = vec![0u32; width as usize];
            for y in 0..height as usize {
                let o = (height as usize - 1 - y) * pitch as usize;
                let row = &data[o..];
                if format == D3DFMT_R5G6B5 {
                    for (x, slot) in row16.iter_mut().enumerate() {
                        let c = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                        *slot = ((c >> 1) & 0x7fe0) | (c & 0x1f);
                    }
                    let mut bytes = Vec::with_capacity(row16.len() * 2);
                    for v in &row16 {
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                    self.textures.call(fx.write_tga(file, &bytes))?;
                } else if format == D3DFMT_A4R4G4B4 {
                    for (x, slot) in row32.iter_mut().enumerate() {
                        let c = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                        *slot = (((c & 0xf000) as u32) << 16)
                            | (((c & 0x0f00) as u32) << 12)
                            | (((c & 0x00f0) as u32) << 8)
                            | (((c & 0x000f) as u32) << 4);
                    }
                    let mut bytes = Vec::with_capacity(row32.len() * 4);
                    for v in &row32 {
                        bytes.extend_from_slice(&v.to_le_bytes());
                    }
                    self.textures.call(fx.write_tga(file, &bytes))?;
                } else {
                    let n = row_bytes as usize;
                    self.textures.call(fx.write_tga(file, &row[..n]))?;
                }
            }
            drop(row16);
            drop(row32);
            self.textures.call(fx.close_tga(file))?;
            self.textures.call(fx.unlock_level_zero(graphics))?;
            Ok(true)
        })();
        if result.is_err() {
            self.poisoned = true;
            self.textures.poisoned = true;
        }
        result
    }
    /// Original `m_TryPresavedDXT` payload loop, PC RVA 0x78190 (upstream
    /// working name, NOT original). LS3DF.dll SHA-256
    /// e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
    /// REA provenance (provider ghidra, same SHA): dossier ev_b6a92418
    /// (body 0x10078190-0x10078423, 660B/20BB/222 insn), assembly
    /// ev_a85415b5, truncated decompiler ev_8fbef404.
    /// Filename probe reuses [`presaved_dxt_filename`]; the payload order is
    /// exact: open (None = `DTA_INVALID_HANDLE`, clean miss, no close) ->
    /// magic `D,X,T` (4B read, first 3 compared; REA asm @0x100782ea/f9/04)
    /// -> version u16le == 9 (@0x1007831f; mafia-re ITexture.cpp:78) ->
    /// dims two i32le vs u16 width/height (@0x10078340-56) -> dead FILETIME
    /// 8B read, plus a second dead 8B read iff `dxt == PRESAVED_DXT3`
    /// (@0x1007835c/69-7b; values never compared: no `CompareFileTime`
    /// edge) -> `m_MemSize = 0` -> per level 0..levels:
    /// `presaved_level_size` (GetSurfaceLevel/GetDesc projection,
    /// @0x1007839c/ab), `wrapping_add` into `memory_size` (native plain ADD
    /// @0x100783b7, wraps by construction), exact-size `read_presaved`
    /// (native unchecked `dtaRead` @0x100783d8; short data is a backend
    /// error per the [`TexturePresavedEffects`] contract),
    /// `write_presaved_level` (LockRect/`dtaRead`/UnlockRect triple as one
    /// ordered effect; the original checks no lock status, @0x100783ca-d8,
    /// so the trait offers no lock-failure path) -> close -> true. Any
    /// header miss closes the stream and returns false (@0x1007840f);
    /// backend errors poison like the sibling TGA/duplicate ports. Ghidra
    /// pseudocode truncates after the version read, so assembly is primary.
    pub fn try_presaved_dxt<B: TextureBackend + ?Sized>(
        &mut self,
        key: TextureKey,
        name: &[u8],
        alpha_name: Option<&[u8]>,
        dxt: u8,
        b: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let t = self.textures.live(key)?;
        // Pure scalar snapshot before effects. The DXT path never reads
        // m_Format, so format stays out of the snapshot.
        let (width, height, levels, flags, graphics) = (
            Self::req_value(t.width)?,
            Self::req_value(t.height)?,
            Self::req_value(t.levels)?,
            t.flags,
            t.graphics,
        );
        let result = (|| {
            let half = flags & ITC_HALF != 0;
            let filename = presaved_dxt_filename(name, alpha_name, levels, half, dxt)
                .map_err(presaved_name_error)?;
            let fx = b.presaved_effects().ok_or(TextureError::HostAllocation)?;
            let Some(file) = self.textures.call(fx.open_presaved(&filename))? else {
                return Ok(false);
            };
            let mut magic = [0u8; 4];
            let op = self.textures.call(fx.read_presaved(file, &mut magic));
            self.textures.close_presaved_on_error(fx, file, op)?;
            if magic[0] != b'D' || magic[1] != b'X' || magic[2] != b'T' {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(false);
            }
            let mut version = [0xffu8; 2];
            let op = self.textures.call(fx.read_presaved(file, &mut version));
            self.textures.close_presaved_on_error(fx, file, op)?;
            if u16::from_le_bytes(version) != PRESAVED_DXT_VERSION {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(false);
            }
            let mut dims = [0u8; 8];
            let op = self.textures.call(fx.read_presaved(file, &mut dims));
            self.textures.close_presaved_on_error(fx, file, op)?;
            // Exact-size arrays: infallible by construction (expect, not a
            // fabricated allocation error).
            let (sx, sy) = (
                i32::from_le_bytes(
                    dims[0..4]
                        .try_into()
                        .expect("presaved dims are exact-size arrays"),
                ),
                i32::from_le_bytes(
                    dims[4..8]
                        .try_into()
                        .expect("presaved dims are exact-size arrays"),
                ),
            );
            if sx != i32::from(width) || sy != i32::from(height) {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(false);
            }
            // Dead FILETIME reads, preserved as effects: the payload is
            // issued and discarded, never compared.
            let mut file_time = [0u8; 8];
            let op = self.textures.call(fx.read_presaved(file, &mut file_time));
            self.textures.close_presaved_on_error(fx, file, op)?;
            if dxt == PRESAVED_DXT3 {
                let mut file_alpha_time = [0u8; 8];
                let op = self
                    .textures
                    .call(fx.read_presaved(file, &mut file_alpha_time));
                self.textures.close_presaved_on_error(fx, file, op)?;
            }
            self.textures.texture_mut(key).memory_size = 0;
            // Deferred to loop entry, as in the original: zero levels never
            // touch the device handle, so a missing one still returns true.
            // A missing handle with levels to load closes the stream before
            // the clean rejection (same close-then-error order as the OOM
            // path below; native always has a surface here).
            if levels == 0 {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(true);
            }
            let Some(graphics) = graphics else {
                self.textures.call(fx.close_presaved(file))?;
                return Err(TextureError::InvalidState.into());
            };
            for level in 0..levels {
                let op = self.textures.call(fx.presaved_level_size(graphics, level));
                let size = self.textures.close_presaved_on_error(fx, file, op)?;
                let t = self.textures.texture_mut(key);
                t.memory_size = t.memory_size.wrapping_add(size);
                let mut bytes = Vec::new();
                if bytes.try_reserve_exact(size as usize).is_err() {
                    self.textures.call(fx.close_presaved(file))?;
                    return Err(TextureError::HostAllocation.into());
                }
                bytes.resize(size as usize, 0);
                let op = self.textures.call(fx.read_presaved(file, &mut bytes));
                self.textures.close_presaved_on_error(fx, file, op)?;
                let op = self
                    .textures
                    .call(fx.write_presaved_level(graphics, level, &bytes));
                self.textures.close_presaved_on_error(fx, file, op)?;
            }
            self.textures.call(fx.close_presaved(file))?;
            Ok(true)
        })();
        if result.is_err() {
            self.poisoned = true;
            self.textures.poisoned = true;
        }
        result
    }
    /// Original `m_TryPresaved_555_565` payload loop, PC RVA 0x78430
    /// (upstream working name, NOT original). Same shape as
    /// REA provenance (provider ghidra, same SHA): dossier ev_bfb86613
    /// (body 0x10078430-0x10078661, 562B/18BB/195 insn), assembly
    /// ev_b509057d, truncated decompiler ev_335861ec.
    /// [`TextureResources::try_presaved_dxt`] with the 16-bit header:
    /// magic `T,E,X` (@0x10078541/4c/57), version u16le == 1 (@0x10078577;
    /// mafia-re ITexture.cpp:82), one dead FILETIME 8B read (@0x100785b4;
    /// no `CompareFileTime` edge), and the `.565`/`.555` probe from
    /// [`presaved_555_filename`] (R5G6B5 select, `CMP EDX,0x17`
    /// @0x100784fa). The surface format comes from the record (native
    /// `m_Format`; there is no format parameter natively). Level loop and
    /// tails mirror the DXT entry (@0x100785dc-2c, close tails
    /// @0x10078638/4d).
    pub fn try_presaved_555_565<B: TextureBackend + ?Sized>(
        &mut self,
        key: TextureKey,
        name: &[u8],
        b: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let t = self.textures.live(key)?;
        let (width, height, levels, flags, format, graphics) = (
            Self::req_value(t.width)?,
            Self::req_value(t.height)?,
            Self::req_value(t.levels)?,
            t.flags,
            Self::req_value(t.format)?,
            t.graphics,
        );
        let result = (|| {
            let half = flags & ITC_HALF != 0;
            let filename =
                presaved_555_filename(name, levels, half, format).map_err(presaved_name_error)?;
            let fx = b.presaved_effects().ok_or(TextureError::HostAllocation)?;
            let Some(file) = self.textures.call(fx.open_presaved(&filename))? else {
                return Ok(false);
            };
            let mut magic = [0u8; 4];
            let op = self.textures.call(fx.read_presaved(file, &mut magic));
            self.textures.close_presaved_on_error(fx, file, op)?;
            if magic[0] != b'T' || magic[1] != b'E' || magic[2] != b'X' {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(false);
            }
            let mut version = [0xffu8; 2];
            let op = self.textures.call(fx.read_presaved(file, &mut version));
            self.textures.close_presaved_on_error(fx, file, op)?;
            if u16::from_le_bytes(version) != PRESAVED_TEX_VERSION {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(false);
            }
            let mut dims = [0u8; 8];
            let op = self.textures.call(fx.read_presaved(file, &mut dims));
            self.textures.close_presaved_on_error(fx, file, op)?;
            // Exact-size arrays: infallible by construction (expect, not a
            // fabricated allocation error).
            let (sx, sy) = (
                i32::from_le_bytes(
                    dims[0..4]
                        .try_into()
                        .expect("presaved dims are exact-size arrays"),
                ),
                i32::from_le_bytes(
                    dims[4..8]
                        .try_into()
                        .expect("presaved dims are exact-size arrays"),
                ),
            );
            if sx != i32::from(width) || sy != i32::from(height) {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(false);
            }
            // Dead FILETIME read, preserved as an effect.
            let mut file_time = [0u8; 8];
            let op = self.textures.call(fx.read_presaved(file, &mut file_time));
            self.textures.close_presaved_on_error(fx, file, op)?;
            self.textures.texture_mut(key).memory_size = 0;
            // Zero levels return true without touching the device handle
            // (same deferred check as the DXT loop entry).
            if levels == 0 {
                self.textures.call(fx.close_presaved(file))?;
                return Ok(true);
            }
            let Some(graphics) = graphics else {
                self.textures.call(fx.close_presaved(file))?;
                return Err(TextureError::InvalidState.into());
            };
            for level in 0..levels {
                let op = self.textures.call(fx.presaved_level_size(graphics, level));
                let size = self.textures.close_presaved_on_error(fx, file, op)?;
                let t = self.textures.texture_mut(key);
                t.memory_size = t.memory_size.wrapping_add(size);
                let mut bytes = Vec::new();
                if bytes.try_reserve_exact(size as usize).is_err() {
                    self.textures.call(fx.close_presaved(file))?;
                    return Err(TextureError::HostAllocation.into());
                }
                bytes.resize(size as usize, 0);
                let op = self.textures.call(fx.read_presaved(file, &mut bytes));
                self.textures.close_presaved_on_error(fx, file, op)?;
                let op = self
                    .textures
                    .call(fx.write_presaved_level(graphics, level, &bytes));
                self.textures.close_presaved_on_error(fx, file, op)?;
            }
            self.textures.call(fx.close_presaved(file))?;
            Ok(true)
        })();
        if result.is_err() {
            self.poisoned = true;
            self.textures.poisoned = true;
        }
        result
    }
    /// callbacks (host analog of the `m_pLockSurface` early-NULL); else
    /// GetSurfaceLevel then LockRect with `LOCK_FLAG_NO_DIRTY_UPDATE`,
    /// publishing pitch and bits. No dirty update here.
    pub fn surface_lock<S: TextureSurfaceSink>(
        &mut self,
        graphics: TextureGraphics,
        rect: Option<Rect>,
        level: u16,
        sink: &mut S,
    ) -> Result<Option<(u32, u64)>, TextureOperationError<S::Error>> {
        self.available()?;
        if graphics.0 == 0 {
            return Ok(None);
        }
        let Some(surface) = self
            .textures
            .call(sink.get_surface_level(graphics, level))?
        else {
            return Ok(None);
        };
        let Some(mapping) =
            self.textures
                .call(sink.lock_surface(surface, rect, LOCK_FLAG_NO_DIRTY_UPDATE))?
        else {
            return Ok(None);
        };
        Ok(Some((mapping.pitch, mapping.mapping)))
    }
    /// Original `AddDirtyRect`, PC RVA 0x78850: pure tailcall forward; the
    /// sink code returns verbatim and the rect pointer passes untouched.
    pub fn surface_add_dirty<S: TextureSurfaceSink>(
        &mut self,
        graphics: TextureGraphics,
        rect: Option<Rect>,
        sink: &mut S,
    ) -> Result<i32, TextureOperationError<S::Error>> {
        self.available()?;
        self.textures.call(sink.add_dirty_rect(graphics, rect))
    }
    /// Original `Preload`, PC RVA 0x78860: point-draw upload with the
    /// scene-begun branch. Order-exact: begun path is
    /// [maybe set VF][bind][draw-point]; otherwise
    /// [begin][maybe set VF][bind][draw-point][end]; returns native 0.
    pub fn preload_texture<S: TextureDrawSink>(
        &mut self,
        graphics: TextureGraphics,
        sink: &mut S,
    ) -> Result<(), TextureOperationError<S::Error>> {
        self.available()?;
        if self.textures.call(sink.scene_begun())? {
            if self.textures.call(sink.tl_format())? != VERTEX_FORMAT_TLVERTEX {
                self.textures.call(sink.set_tl_format(TL_FVF))?;
            }
            self.textures.call(sink.bind_raw_graphics(graphics))?;
            self.textures.call(sink.draw_point_up(&[PRELOAD_POINT]))?;
        } else {
            self.textures.call(sink.begin_scene())?;
            if self.textures.call(sink.tl_format())? != VERTEX_FORMAT_TLVERTEX {
                self.textures.call(sink.set_tl_format(TL_FVF))?;
            }
            self.textures.call(sink.bind_raw_graphics(graphics))?;
            self.textures.call(sink.draw_point_up(&[PRELOAD_POINT]))?;
            self.textures.call(sink.end_scene())?;
        }
        Ok(())
    }
    /// Original `Fill`, PC RVA 0x78940: GetDesc, Lock, per-format synthetic
    /// writes over the locked mapping, Unlock, AddDirtyRect, true. Extents
    /// come from `rect` when given (`right-left`/`bottom-top`, ITexture.cpp
    /// :1217-1218), else the desc extents. Stride is the locked pitch: each
    /// row advances by pitch while pixel data advances tightly, so subrect
    /// rows never touch neighboring bytes. L8 ignores pitch (three contiguous
    /// planes). A lock reporting less than one row unlocks and returns
    /// false (native would overrun; the host refuses instead of truncating).
    /// The desc miss and the lock miss both return false (the lock
    /// miss unlocks first, mirroring the fail-path Unlock); unsupported
    /// formats unlock then return false with no dirty.
    pub fn fill_texture<S: TextureSurfaceSink>(
        &mut self,
        graphics: TextureGraphics,
        level: u16,
        rect: Option<Rect>,
        color: u32,
        format: u32,
        sink: &mut S,
    ) -> Result<bool, TextureOperationError<S::Error>> {
        self.available()?;
        let Some(desc) = self.textures.call(sink.surface_desc(graphics, level))? else {
            return Ok(false);
        };
        let Some(surface) = self
            .textures
            .call(sink.get_surface_level(graphics, level))?
        else {
            return Ok(false);
        };
        let Some(mapping) = self.textures.call(sink.lock_surface(surface, rect, 0))? else {
            self.textures.call(sink.unlock_surface(surface))?;
            return Ok(false);
        };
        let (sx, sy) = match rect {
            Some(r) => (
                (r.right - r.left).max(0) as u32,
                (r.bottom - r.top).max(0) as u32,
            ),
            None => (desc.width, desc.height),
        };
        let pitch = mapping.pitch;
        // Short-pitch guard: native would overrun the lock; the host unlocks
        // and reports false instead of truncating row pixels silently.
        // All size math is checked: hostile extents unlock and report
        // instead of wrapping or panicking.
        let total: usize = match format {
            D3DFMT_A8R8G8B8 | D3DFMT_X8R8G8B8 | D3DFMT_R8G8B8 | D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5
            | D3DFMT_A1R5G5B5 | D3DFMT_A4R4G4B4 => {
                let bpp = match format {
                    D3DFMT_R8G8B8 => 3,
                    D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5 | D3DFMT_A4R4G4B4 => 2,
                    _ => 4,
                };
                let row = sx.checked_mul(bpp).ok_or(TextureError::HostAllocation)?;
                if pitch < row {
                    self.textures.call(sink.unlock_surface(surface))?;
                    return Ok(false);
                }
                pitch.checked_mul(sy).ok_or(TextureError::HostAllocation)? as usize
            }
            D3DFMT_L8 => sx
                .checked_mul(sy)
                .and_then(|n| n.checked_mul(3))
                .ok_or(TextureError::HostAllocation)? as usize,
            _ => {
                self.textures.call(sink.unlock_surface(surface))?;
                return Ok(false);
            }
        };
        let mut out = Vec::new();
        if out.try_reserve_exact(total).is_err() {
            self.textures.call(sink.unlock_surface(surface))?;
            return Err(TextureError::HostAllocation.into());
        }
        // Per-row emission: pixel bytes advance tightly, the write cursor
        // advances by pitch, so subrect rows stay inside their own rows.
        // Capacity is reserved above, so no further allocation can trap.
        let body: Option<Vec<u8>> = match format {
            D3DFMT_A8R8G8B8 | D3DFMT_X8R8G8B8 => {
                for _ in 0..sy as usize {
                    let base = out.len();
                    for _ in 0..sx as usize {
                        out.extend_from_slice(&color.to_le_bytes());
                    }
                    out.resize(base + pitch as usize, 0);
                }
                Some(out)
            }
            D3DFMT_R8G8B8 => {
                let [_, r, g, b] = color.to_be_bytes();
                for _ in 0..sy as usize {
                    let base = out.len();
                    for _ in 0..sx as usize {
                        out.extend_from_slice(&[r, g, b]);
                    }
                    out.resize(base + pitch as usize, 0);
                }
                Some(out)
            }
            D3DFMT_R5G6B5 | D3DFMT_X1R5G5B5 | D3DFMT_A1R5G5B5 | D3DFMT_A4R4G4B4 => {
                for _ in 0..sy as usize {
                    let base = out.len();
                    for _ in 0..sx as usize {
                        out.extend_from_slice(&(color as u16).to_le_bytes());
                    }
                    out.resize(base + pitch as usize, 0);
                }
                Some(out)
            }
            D3DFMT_L8 => {
                let [_, r, g, b] = color.to_be_bytes();
                for _ in 0..total / 3 {
                    out.extend_from_slice(&[r, g, b]);
                }
                Some(out)
            }
            _ => None,
        };
        let Some(body) = body else {
            self.textures.call(sink.unlock_surface(surface))?;
            return Ok(false);
        };
        self.textures
            .call(sink.write_locked(mapping.mapping, &body))?;
        self.textures.call(sink.unlock_surface(surface))?;
        self.textures.call(sink.add_dirty_rect(graphics, rect))?;
        Ok(true)
    }
    /// Original `Draw(x,y)`, PC RVA 0x78b70: screen-clipped quad blit.
    /// Order: scrn size, intersect (offscreen returns true with no draw),
    /// set texture, clamp U/V, strip draw of 2, on success wrap U/V restore.
    /// A negative draw returns false with no restore.
    pub fn draw_texture_at<S: TextureDrawSink>(
        &mut self,
        key: TextureKey,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        sink: &mut S,
    ) -> Result<bool, TextureOperationError<S::Error>> {
        self.available()?;
        let rect = Rect {
            left: x,
            top: y,
            right: x + width as i32,
            bottom: y + height as i32,
        };
        self.draw_texture_rect_inner(key, rect, None, sink)
    }
    /// Original `Draw(rect,colors)`, PC RVA 0x78cf0: tinted stretched blit.
    /// Colors publish before positions, both before set_texture; same
    /// clamp/draw/wrap epilogue as `Draw(x,y)`.
    pub fn draw_texture_rect<S: TextureDrawSink>(
        &mut self,
        key: TextureKey,
        rect: Rect,
        colors: [u32; 4],
        sink: &mut S,
    ) -> Result<bool, TextureOperationError<S::Error>> {
        self.available()?;
        self.draw_texture_rect_inner(key, rect, Some(colors), sink)
    }
    fn draw_texture_rect_inner<S: TextureDrawSink>(
        &mut self,
        key: TextureKey,
        rect: Rect,
        colors: Option<[u32; 4]>,
        sink: &mut S,
    ) -> Result<bool, TextureOperationError<S::Error>> {
        let (sx, sy) = self.textures.call(sink.scrn_size())?;
        let screen = Rect {
            left: 0,
            top: 0,
            right: sx,
            bottom: sy,
        };
        let mut vis = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if !self
            .textures
            .call(sink.intersect_rect(&mut vis, &rect, &screen))?
        {
            return Ok(true);
        }
        let diffuse = colors.unwrap_or([0xffff_ffff; 4]);
        let quad = [
            TlVertex {
                x: rect.left as f32,
                y: rect.top as f32,
                diffuse: diffuse[0],
                u: 0.0,
                v: 0.0,
            },
            TlVertex {
                x: rect.right as f32,
                y: rect.top as f32,
                diffuse: diffuse[1],
                u: 1.0,
                v: 0.0,
            },
            TlVertex {
                x: rect.left as f32,
                y: rect.bottom as f32,
                diffuse: diffuse[2],
                u: 0.0,
                v: 1.0,
            },
            TlVertex {
                x: rect.right as f32,
                y: rect.bottom as f32,
                diffuse: diffuse[3],
                u: 1.0,
                v: 1.0,
            },
        ];
        self.textures.call(sink.set_texture(key))?;
        self.textures
            .call(sink.set_sampler(0, SAMPLER_U, SAMPLER_CLAMP))?;
        self.textures
            .call(sink.set_sampler(0, SAMPLER_V, SAMPLER_CLAMP))?;
        if self
            .textures
            .call(sink.draw_strip(PRIM_TRIANGLESTRIP, 2, &quad))?
            < 0
        {
            return Ok(false);
        }
        self.textures
            .call(sink.set_sampler(0, SAMPLER_U, SAMPLER_WRAP))?;
        self.textures
            .call(sink.set_sampler(0, SAMPLER_V, SAMPLER_WRAP))?;
        Ok(true)
    }
    pub fn destruct_texture<B: TextureBackend + ?Sized>(
        &mut self,
        k: TextureKey,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let t = self.textures.live(k)?;
        if t.references != 1 || t.external_references != 1 {
            return Err(TextureError::ReferenceOwned.into());
        }
        self.textures.destruct(k, &mut self.strings, b)
    }
    pub fn copy_file_name(&mut self, k: TextureKey, alpha: bool) -> Result<CStrKey, TextureError> {
        self.available()?;
        self.textures.domain(k)?;
        let t = self.textures.live(k)?;
        let source = if alpha {
            t.alpha_file_name
        } else {
            t.file_name
        };
        self.external_strings
            .try_reserve(1)
            .map_err(|_| TextureError::HostAllocation)?;
        let key = self.strings.copy_string(source)?;
        self.external_strings.push(key);
        Ok(key)
    }
    pub fn destroy_string<B: TextureBackend + ?Sized>(
        &mut self,
        key: CStrKey,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let position = self
            .external_strings
            .iter()
            .position(|x| *x == key)
            .ok_or(TextureError::UnknownExternalString)?;
        string_result(self.strings.destroy_string(key, b))?;
        self.external_strings.swap_remove(position);
        Ok(())
    }
    fn run_objects<T, B: TextureBackend + ?Sized>(
        &mut self,
        b: &mut B,
        op: impl FnOnce(
            &mut ObjectResources,
            &mut Adapter<'_, B>,
        ) -> Result<
            T,
            ObjectOperationError<TextureOperationError<<B as BufferBackend>::Error>>,
        >,
    ) -> Result<T, TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        // A seeded managed world requires a complete stable backend for object
        // operations, before any ancestor reference/resource effect. This host
        // availability bound also excludes duplicate-record cleanup until direct
        // removals have reduced each target to at most one active record.
        for (i, t) in self.textures.slots.iter().enumerate() {
            if t.as_ref().is_some_and(|t| {
                t.state == TextureState::Live
                    && (t.managed_admitted || t.managed_prepared || t.managed_created)
            }) {
                self.textures.preflight_close(
                    TextureKey {
                        arena: self.textures.identity,
                        slot: i as u32 + 1,
                    },
                    b,
                )?;
            }
        }

        let mut adapter = Adapter {
            textures: &mut self.textures,
            strings: &mut self.strings,
            backend: b,
        };
        op(&mut self.objects, &mut adapter).map_err(|e| match e {
            ObjectOperationError::State(e) => e.into(),
            ObjectOperationError::Backend(e) => {
                self.poisoned = true;
                e
            }
        })
    }
    pub fn set_texture<B: TextureBackend + ?Sized>(
        &mut self,
        material: MaterialKey,
        new: Option<TextureKey>,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.set_material_texture(material, new, false, b)
    }
    pub fn set_env_texture<B: TextureBackend + ?Sized>(
        &mut self,
        material: MaterialKey,
        new: Option<TextureKey>,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.set_material_texture(material, new, true, b)
    }
    pub fn set_env_texture_ex<B: TextureBackend + ?Sized>(
        &mut self,
        material: MaterialKey,
        new: Option<TextureKey>,
        blend: u32,
        mode: u32,
        alpha: f32,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        // Live existence read, mirroring the native this-dereference; the
        // Ex path recomputes flags unconditionally (no token comparison).
        let current_env = self
            .objects
            .managed()
            .resources()
            .material(material)
            .map_err(crate::managed_visual_resources::ManagedError::from)
            .map_err(ObjectError::from)?
            .env_texture;
        let token = new
            .map(|k| {
                self.textures.domain(k)?;
                self.textures.live(k).map(|t| t.token)
            })
            .transpose()?;
        // Same overflow preflight as the non-Ex path: a saturated texture
        // reports a clean TextureError (no poison, no effects) instead of
        // surfacing as a backend failure with partial effects below.
        // Skipped for same-token calls exactly like the non-Ex path, since
        // those perform no refcount traffic (flags/alpha still recompute).
        if token != current_env {
            if let Some(k) = new {
                self.textures.can_retain(k)?;
            }
            if let Some(old) = current_env {
                let key = self.textures.token(old)?;
                self.textures.domain(key)?;
            }
        }
        self.run_objects(b, |o, a| {
            o.set_env_texture_ex(material, token, blend, mode, alpha, a)
        })
    }
    pub fn set_material_ambient(
        &mut self,
        material: MaterialKey,
        color: [f32; 3],
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_ambient(material, color)
            .map_err(Into::into)
    }
    pub fn set_material_diffuse(
        &mut self,
        material: MaterialKey,
        color: [f32; 3],
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_diffuse(material, color)
            .map_err(Into::into)
    }
    pub fn set_material_emissive(
        &mut self,
        material: MaterialKey,
        color: [f32; 3],
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_emissive(material, color)
            .map_err(Into::into)
    }
    pub fn set_material_alpha(
        &mut self,
        material: MaterialKey,
        alpha: f32,
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_alpha(material, alpha)
            .map_err(Into::into)
    }
    pub fn set_material_two_sided(&mut self, material: MaterialKey) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_two_sided(material)
            .map_err(Into::into)
    }
    pub fn set_material_env_type(
        &mut self,
        material: MaterialKey,
        blend: u32,
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_env_type(material, blend)
            .map_err(Into::into)
    }
    pub fn set_material_env_mode(
        &mut self,
        material: MaterialKey,
        mode: u32,
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_env_mode(material, mode)
            .map_err(Into::into)
    }
    pub fn set_material_env_scale(
        &mut self,
        material: MaterialKey,
        scale: u32,
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .set_material_env_scale(material, scale)
            .map_err(Into::into)
    }
    fn set_material_texture<B: TextureBackend + ?Sized>(
        &mut self,
        material: MaterialKey,
        new: Option<TextureKey>,
        env: bool,
        b: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.available()?;
        let current = self
            .objects
            .managed()
            .resources()
            .material(material)
            .map_err(crate::managed_visual_resources::ManagedError::from)
            .map_err(ObjectError::from)?;
        let old = if env {
            current.env_texture
        } else {
            current.texture
        };
        let token = new
            .map(|k| {
                self.textures.domain(k)?;
                self.textures.live(k).map(|t| t.token)
            })
            .transpose()?;
        if token != old {
            if let Some(k) = new {
                self.textures.can_retain(k)?;
            }
            if let Some(old) = old {
                let key = self.textures.token(old)?;
                self.textures.domain(key)?;
            }
        }
        self.run_objects(b, |o, a| {
            if env {
                o.set_env_texture(material, token, a)
            } else {
                o.set_texture(material, token, a)
            }
        })
    }
}
struct Adapter<'a, B: TextureBackend + ?Sized> {
    textures: &'a mut Textures,
    strings: &'a mut CStrings,
    backend: &'a mut B,
}
impl<B: TextureBackend + ?Sized> ManagedVisualBackend for Adapter<'_, B> {
    fn retain_texture(&mut self, t: vr::TextureKey) -> Result<(), Self::Error> {
        let key = self.textures.token(t)?;
        self.textures.retain(key, true).map_err(Into::into)
    }
    fn release_texture(&mut self, t: vr::TextureKey) -> Result<(), Self::Error> {
        let key = self.textures.token(t)?;
        self.textures
            .release(key, true, self.strings, self.backend)
            .map(|_| ())
    }
    fn texture_flags(&mut self, t: vr::TextureKey) -> Result<u32, Self::Error> {
        let key = self.textures.token(t)?;
        Ok(self.textures.live(key)?.flags)
    }
    fn free_raw_vertices(
        &mut self,
        l: MeshLevelKey,
        r: RawVertexBytesKey,
    ) -> Result<(), Self::Error> {
        self.backend
            .free_raw_vertices(l, r)
            .map_err(TextureOperationError::Backend)
    }
}

impl<B: TextureBackend + ?Sized> BufferBackend for Adapter<'_, B> {
    type Error = TextureOperationError<<B as BufferBackend>::Error>;
    fn allocate_buffer(
        &mut self,
        kind: bm::BufferKind,
    ) -> Result<bm::AllocationToken, Self::Error> {
        BufferBackend::allocate_buffer(self.backend, kind).map_err(TextureOperationError::Backend)
    }
    fn free_buffer(&mut self, allocation: bm::AllocationToken) -> Result<(), Self::Error> {
        BufferBackend::free_buffer(self.backend, allocation).map_err(TextureOperationError::Backend)
    }
    fn allocate_block(
        &mut self,
        kind: bm::BufferKind,
        base: u32,
        count: u32,
    ) -> Result<bm::AllocationToken, Self::Error> {
        BufferBackend::allocate_block(self.backend, kind, base, count)
            .map_err(TextureOperationError::Backend)
    }
    fn free_block(&mut self, allocation: bm::AllocationToken) -> Result<(), Self::Error> {
        BufferBackend::free_block(self.backend, allocation).map_err(TextureOperationError::Backend)
    }
    fn create_graphics(
        &mut self,
        request: bm::GraphicsRequest,
    ) -> Result<bm::CreateResult, Self::Error> {
        BufferBackend::create_graphics(self.backend, request)
            .map_err(TextureOperationError::Backend)
    }
    fn discard_managed(&mut self) -> Result<(), Self::Error> {
        BufferBackend::discard_managed(self.backend).map_err(TextureOperationError::Backend)
    }
    fn lock(
        &mut self,
        graphics: bm::GraphicsBuffer,
        flags: u32,
    ) -> Result<bm::LockedDataKey, Self::Error> {
        BufferBackend::lock(self.backend, graphics, flags).map_err(TextureOperationError::Backend)
    }
    fn unlock(&mut self, graphics: bm::GraphicsBuffer) -> Result<(), Self::Error> {
        BufferBackend::unlock(self.backend, graphics).map_err(TextureOperationError::Backend)
    }
    fn release_graphics(&mut self, graphics: bm::GraphicsBuffer) -> Result<(), Self::Error> {
        BufferBackend::release_graphics(self.backend, graphics)
            .map_err(TextureOperationError::Backend)
    }
    fn recompute_vram(&mut self) -> Result<(), Self::Error> {
        BufferBackend::recompute_vram(self.backend).map_err(TextureOperationError::Backend)
    }
    fn warn_unreleased_buffer(&mut self, buffer: bm::BufferKey) -> Result<(), Self::Error> {
        BufferBackend::warn_unreleased_buffer(self.backend, buffer)
            .map_err(TextureOperationError::Backend)
    }
    fn out_of_memory(
        &mut self,
        kind: bm::BufferKind,
        bytes: u32,
        hresult: i32,
    ) -> Result<(), Self::Error> {
        BufferBackend::out_of_memory(self.backend, kind, bytes, hresult)
            .map_err(TextureOperationError::Backend)
    }
    fn driver_present(&self) -> bool {
        self.backend.driver_present()
    }
}
impl TextureResources {
    pub fn create_material(&mut self) -> Result<MaterialKey, TextureError> {
        self.available()?;
        self.objects.create_material().map_err(Into::into)
    }
    pub fn create_face_group(&mut self) -> Result<FaceGroupKey, TextureError> {
        self.available()?;
        self.objects.create_face_group().map_err(Into::into)
    }
    pub fn create_mesh_level(&mut self) -> Result<MeshLevelKey, TextureError> {
        self.available()?;
        self.objects.create_mesh_level().map_err(Into::into)
    }
    pub fn create_mesh_object(&mut self) -> Result<MeshObjectKey, TextureError> {
        self.available()?;
        self.objects.create_mesh_object().map_err(Into::into)
    }
    pub fn create_plain_lod(&mut self) -> Result<PlainLodKey, TextureError> {
        self.available()?;
        self.objects.create_plain_lod().map_err(Into::into)
    }
    pub fn create_object_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, TextureError> {
        self.available()?;
        self.objects
            .create_object_resources(mesh)
            .map_err(Into::into)
    }
    pub fn retain_material(&mut self, key: MaterialKey) -> Result<(), TextureError> {
        self.available()?;
        self.objects.retain_material(key).map_err(Into::into)
    }
    pub fn retain_mesh_level(&mut self, key: MeshLevelKey) -> Result<(), TextureError> {
        self.available()?;
        self.objects.retain_mesh_level(key).map_err(Into::into)
    }
    pub fn retain_mesh_object(&mut self, key: MeshObjectKey) -> Result<(), TextureError> {
        self.available()?;
        self.objects.retain_mesh_object(key).map_err(Into::into)
    }
    pub fn link_next(
        &mut self,
        head: FaceGroupKey,
        next: FaceGroupKey,
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects.link_next(head, next).map_err(Into::into)
    }
    pub fn install_mesh_level_bbox(
        &mut self,
        key: MeshLevelKey,
        bbox: [[f32; 3]; 2],
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .install_mesh_level_bbox(key, bbox)
            .map_err(Into::into)
    }
    pub fn update_mesh_bound_volume(&mut self, key: MeshObjectKey) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .update_mesh_bound_volume(key)
            .map_err(Into::into)
    }
    pub fn adopt_plain_lod(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
        lod: PlainLodKey,
    ) -> Result<(), TextureError> {
        self.available()?;
        self.objects
            .adopt_plain_lod(object, index, lod)
            .map_err(Into::into)
    }
    pub fn allocate_group_indices<B: TextureBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        num_faces: u32,
        usage: u32,
        backend: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| {
            o.allocate_group_indices(group, num_faces, usage, b)
        })
    }
    pub fn allocate_level_vertices<B: TextureBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        num_vertices: u32,
        request: VertexRequest,
        head: Option<FaceGroupKey>,
        backend: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| {
            o.allocate_level_vertices(level, num_vertices, request, head, b)
        })
    }
    pub fn allocate_plain_lod_vertices<B: TextureBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        request: VertexRequest,
        backend: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| {
            o.allocate_plain_lod_vertices(lod, request, b)
        })
    }
    pub fn release_material<B: TextureBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        backend: &mut B,
    ) -> Result<u32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.release_material(key, b))
    }
    pub fn release_mesh_level<B: TextureBackend + ?Sized>(
        &mut self,
        key: MeshLevelKey,
        backend: &mut B,
    ) -> Result<u32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.release_mesh_level(key, b))
    }
    pub fn release_mesh_object<B: TextureBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        backend: &mut B,
    ) -> Result<u32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.release_mesh_object(key, b))
    }
    pub fn destroy_face_group<B: TextureBackend + ?Sized>(
        &mut self,
        key: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.destroy_face_group(key, b))
    }
    pub fn destroy_plain_lod<B: TextureBackend + ?Sized>(
        &mut self,
        key: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.destroy_plain_lod(key, b))
    }
    pub fn destruct_plain_lod<B: TextureBackend + ?Sized>(
        &mut self,
        key: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.destruct_plain_lod(key, b))
    }
    pub fn destroy_object_resources<B: TextureBackend + ?Sized>(
        &mut self,
        key: ObjectResourceKey,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.destroy_object_resources(key, b))
    }
    pub fn set_material<B: TextureBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        material: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.set_material(group, material, b))
    }
    pub fn create_faces<B: TextureBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        num: u32,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.create_faces(group, num, b))
    }
    pub fn set_faces<B: TextureBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        faces: Option<&[vr::I3dTriface]>,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.set_faces(group, faces, b))
    }
    /// Owned copy of the live locked array (copied while locked from borrowed
    /// state; the pairing flag stays set until `unlock_faces`). The inner layers
    /// keep the borrow; only this outermost façade copies.
    pub fn get_faces<B: TextureBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<Option<Vec<vr::I3dTriface>>, TextureOperationError<<B as BufferBackend>::Error>>
    {
        self.run_objects(backend, |o, b| {
            o.get_faces(group, b)
                .map(|faces| faces.map(|faces| faces.to_vec()))
        })
    }
    pub fn unlock_faces<B: TextureBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.unlock_faces(group, b))
    }
    pub fn duplicate_face_group<B: TextureBackend + ?Sized>(
        &mut self,
        destination: FaceGroupKey,
        source: FaceGroupKey,
        backend: &mut B,
    ) -> Result<i32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| {
            o.duplicate_face_group(destination, source, b)
        })
    }
    pub fn detach_face_group_material<B: TextureBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<i32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.detach_face_group_material(group, b))
    }
    pub fn set_lod<B: TextureBackend + ?Sized>(
        &mut self,
        mesh: MeshObjectKey,
        level: Option<MeshLevelKey>,
        index: u32,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.set_lod(mesh, level, index, b))
    }
    pub fn get_lod(
        &mut self,
        key: MeshObjectKey,
        index: u32,
    ) -> Result<Option<MeshLevelKey>, TextureError> {
        self.available()?;
        self.objects.get_lod(key, index).map_err(Into::into)
    }
    pub fn is_mesh_compatible(
        &mut self,
        first: MeshObjectKey,
        second: MeshObjectKey,
    ) -> Result<bool, TextureError> {
        self.available()?;
        self.objects
            .is_mesh_compatible(first, second)
            .map_err(Into::into)
    }
    pub fn mesh_object_stats(
        &mut self,
        key: MeshObjectKey,
    ) -> Result<crate::visual_resources::I3dMeshStats, TextureError> {
        self.available()?;
        self.objects.mesh_object_stats(key).map_err(Into::into)
    }
    pub fn detach_mesh_materials<B: TextureBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        backend: &mut B,
    ) -> Result<i32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.detach_mesh_materials(key, b))
    }
    pub fn duplicate_mesh_object<B: TextureBackend + ?Sized>(
        &mut self,
        destination: MeshObjectKey,
        source: Option<MeshObjectKey>,
        backend: &mut B,
    ) -> Result<i32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| {
            o.duplicate_mesh_object(destination, source, b)
        })
    }
    pub fn set_mesh_level_vertices<B: TextureBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        verts: Option<&[u8]>,
        num: u32,
        backend: &mut B,
    ) -> Result<i32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| {
            o.set_mesh_level_vertices(level, verts, num, b)
        })
    }
    pub fn create_mesh_level_vertices<B: TextureBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        num: u32,
        backend: &mut B,
    ) -> Result<bool, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.create_mesh_level_vertices(level, num, b))
    }
    pub fn duplicate_mesh_level<B: TextureBackend + ?Sized>(
        &mut self,
        destination: MeshLevelKey,
        source: Option<MeshLevelKey>,
        backend: &mut B,
    ) -> Result<i32, TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| {
            o.duplicate_mesh_level(destination, source, b)
        })
    }
    pub fn reclaim_unused_indices<B: TextureBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.reclaim_unused_indices(b))
    }
    pub fn reclaim_unused_vertices<B: TextureBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.reclaim_unused_vertices(b))
    }
    pub fn unlock_all_indices<B: TextureBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.unlock_all_indices(b))
    }
    pub fn unlock_all_vertices<B: TextureBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.unlock_all_vertices(b))
    }
}

#[cfg(test)]
mod tests {
    use super::texture_surface::{LockedMapping, S3tcEncodeSink, SurfaceDesc};
    use super::*;
    use crate::c_str::{CRepKey, StringAllocation};
    use crate::texture_manager::*;
    use std::collections::BTreeMap;
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        StringNew(u32, StringAllocation),
        StringNone(u32),
        StringFree(StringAllocation),
        SurfaceUnlock(TextureSurface),
        SurfaceFree(TextureSurface),
        TextureFree(TextureGraphics),
        PixelsFree(PixelAllocation),
        BufferFree(bm::AllocationToken),
        BlockFree(bm::AllocationToken),
        BufferGraphicsFree(bm::GraphicsBuffer),
        Enter,
        Leave,
        ManagedFilename(Option<ManagedFilenameAllocation>),
        ManagedRecord(ManagedRecordKey),
        CreateRecord(ManagedRecordKey),
        RecordWrite(RecordWrite),
        CreateFilename(u32, Option<ManagedFilenameAllocation>),
        CopyFilename(Vec<u8>),
        CreateArray(u32, Option<ManagedRegistryAllocation>),
        ArrayWrite(u32, ManagedRecordKey),
        DeleteArray(Option<ManagedRegistryAllocation>),
        LowMemory,
        LastFrame(ManagedRecordKey, u32),
        DeletePool(Option<ManagedPoolAllocation>),
        TextureDup(TextureGraphics, u16, u16, u16, u32),
        TextureDupLevels(TextureGraphics, TextureGraphics, u16),
        TexturePreload(TextureGraphics),
        TgaOpen(Vec<u8>),
    }
    #[derive(Default)]
    struct WorkerFileBackend {
        bytes: Vec<u8>,
        position: usize,
    }
    impl lhr_assets::rw_data::Effects for WorkerFileBackend {
        type Error = &'static str;
        fn set_file_pointer(&mut self, _: u32, _: i32, _: u32) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn close_handle(&mut self, _: u32) -> Result<bool, Self::Error> {
            Ok(true)
        }
        fn delete_file(&mut self, _: &[u8]) -> Result<bool, Self::Error> {
            unreachable!()
        }
        fn file_attributes(&mut self, _: &[u8]) -> Result<u32, Self::Error> {
            unreachable!()
        }
        fn set_file_attributes(&mut self, _: &[u8], _: u32) -> Result<bool, Self::Error> {
            unreachable!()
        }
        fn move_file(&mut self, _: &[u8], _: &[u8]) -> Result<bool, Self::Error> {
            unreachable!()
        }
        fn free_name(&mut self, _: u64) -> Result<(), Self::Error> {
            unreachable!()
        }
        fn free_block(&mut self, allocation: u64) -> Result<(), Self::Error> {
            assert_eq!(allocation, 1);
            Ok(())
        }
        fn free_wav(&mut self, _: u64) -> Result<(), Self::Error> {
            unreachable!()
        }
    }
    impl lhr_assets::rw_data::ReadEffects for WorkerFileBackend {
        fn seek_read_file(&mut self, _: u32, position: u32) -> Result<(u32, u32), Self::Error> {
            self.position = position as usize;
            Ok((position, 0))
        }
        fn read_file(
            &mut self,
            _: u32,
            output: &mut [u8],
        ) -> Result<lhr_assets::rw_data::ReadFileResult, Self::Error> {
            let source = self.bytes.get(self.position..).unwrap_or(&[]);
            let count = output.len().min(source.len());
            output[..count].copy_from_slice(&source[..count]);
            self.position += count;
            Ok(lhr_assets::rw_data::ReadFileResult {
                success: true,
                bytes_read: count as u32,
            })
        }
    }
    #[derive(Default)]
    struct Backend {
        next: u64,
        events: Vec<Event>,
        strings: BTreeMap<u64, u32>,
        surfaces: BTreeMap<u64, u32>,
        textures: BTreeMap<u64, u32>,
        pixels: BTreeMap<u64, Vec<u8>>,
        allocations: BTreeMap<u64, bool>,
        buffers: BTreeMap<u64, bool>,
        null_strings: usize,
        fail_string_alloc: bool,
        fail_string_free: bool,
        fail_surface_free: bool,
        fail_texture_free: bool,
        managed_enabled: bool,
        worker_enabled: bool,
        worker_checks: u32,
        worker_frame: u32,
        worker_fail: Option<ManagedStage>,
        worker_events: Vec<ManagedStage>,
        worker_data: Vec<u8>,
        worker_files: Option<lhr_assets::rw_data::Handles<WorkerFileBackend>>,
        worker_creates: Vec<ManagedTextureCreate>,
        section_held: bool,
        fail_enter: bool,
        fail_filename: bool,
        fail_record: bool,
        fail_leave: bool,
        managed_files: BTreeMap<u64, bool>,
        managed_records: BTreeMap<u64, bool>,
        creation_enabled: bool,
        request_enabled: bool,
        release_enabled: bool,
        fail_pool: bool,
        fail_release_surface: Option<TextureSurface>,
        pool_arrays: BTreeMap<u64, bool>,
        fail_last_frame: bool,
        filename_none: bool,
        record_none: bool,
        array_none: bool,
        fail_copy_array: bool,
        filename_bytes: BTreeMap<u64, Vec<u8>>,
        arrays: BTreeMap<u64, Vec<Option<ManagedRecordKey>>>,
        diagnostics: Vec<Vec<u8>>,
        dup_clone_log: Vec<(PixelAllocation, u32)>,
        dup_fail_clone_oom: bool,
        dup_fail_graphics_none: bool,
        no_dup_effects: bool,
        no_tga_effects: bool,
        tga_files: BTreeMap<u64, TgaTestFile>,
        tga_next: u64,
        tga_fail_open: bool,
        tga_lock: Option<(Vec<u8>, u32)>,
        tga_lock_fail: bool,
        tga_unlocks: Vec<u64>,
        presaved_enabled: bool,
        presaved_blobs: BTreeMap<Vec<u8>, Vec<u8>>,
        presaved_open: BTreeMap<u64, PresavedTestFile>,
        presaved_next: u64,
        presaved_opens: Vec<Vec<u8>>,
        presaved_read_lens: Vec<usize>,
        presaved_level_sizes: Vec<u32>,
        presaved_expected_graphics: Option<TextureGraphics>,
        presaved_writes: Vec<(u16, Vec<u8>)>,
    }
    #[derive(Default)]
    struct TgaTestFile {
        bytes: Vec<u8>,
        closed: bool,
    }
    #[derive(Default)]
    struct PresavedTestFile {
        bytes: Vec<u8>,
        pos: usize,
        closed: bool,
        closes: u32,
    }
    impl Backend {
        fn next(&mut self) -> u64 {
            self.next += 1;
            self.next
        }
        fn input(&mut self) -> TextureIntake<'static> {
            let graphics = TextureGraphics(self.next());
            let surface = TextureSurface(self.next());
            let pixels = PixelAllocation(self.next());
            self.textures.insert(graphics.0, 1);
            self.surfaces.insert(surface.0, 1);
            self.pixels.insert(pixels.0, vec![0; 32]);
            TextureIntake {
                uid: 0x1234,
                graphics: Some(graphics),
                lock_surface: Some(surface),
                pixels: Some(pixels),
                initialized: true,
                width: 64,
                height: 32,
                format: 21,
                flags: 0x10e,
                creation_flags: 0xfeed0123,
                color_key: 0x10203040,
                levels: 3,
                memory_size: 8192,
                file_name: Some(b"maps\\wall.tga"),
                name_offset: Some(5),
                alpha_file_name: AlphaName::Bytes(b"maps\\wall_a.tga"),
                alpha_name_offset: Some(5),
            }
        }
        fn buffer_id(g: bm::GraphicsBuffer) -> u64 {
            match g {
                bm::GraphicsBuffer::Index(h) => h.0,
                bm::GraphicsBuffer::Vertex(h) => h.0,
            }
        }
    }
    impl TextureDuplicateEffects for Backend {
        type Error = &'static str;
        fn clone_pixels(
            &mut self,
            src: PixelAllocation,
            bytes: u32,
        ) -> Result<Option<PixelAllocation>, Self::Error> {
            if self.dup_fail_clone_oom {
                return Ok(None);
            }
            let data = self.pixels.get(&src.0).cloned().unwrap_or_default();
            assert_eq!(
                data.len() as u64,
                u64::from(bytes),
                "test pixels must match w*h*4"
            );
            let id = PixelAllocation(self.next());
            self.dup_clone_log.push((src, bytes));
            self.pixels.insert(id.0, data);
            Ok(Some(id))
        }
        fn duplicate_graphics(
            &mut self,
            src: TextureGraphics,
            width: u16,
            height: u16,
            levels: u16,
            format: u32,
        ) -> Result<Option<TextureGraphics>, Self::Error> {
            if self.dup_fail_graphics_none {
                return Ok(None);
            }
            let id = TextureGraphics(self.next());
            self.textures.insert(id.0, 1);
            self.events
                .push(Event::TextureDup(src, width, height, levels, format));
            Ok(Some(id))
        }
        fn copy_graphics_levels(
            &mut self,
            src: TextureGraphics,
            dst: TextureGraphics,
            levels: u16,
        ) -> Result<(), Self::Error> {
            self.events.push(Event::TextureDupLevels(src, dst, levels));
            Ok(())
        }
        fn preload_texture(&mut self, graphics: TextureGraphics) -> Result<(), Self::Error> {
            self.events.push(Event::TexturePreload(graphics));
            Ok(())
        }
    }
    impl TextureTgaEffects for Backend {
        type Error = &'static str;
        fn open_tga_write(&mut self, name: &[u8]) -> Result<Option<TgaFile>, Self::Error> {
            if self.tga_fail_open {
                return Ok(None);
            }
            self.tga_next += 1;
            let id = self.tga_next;
            self.tga_files.insert(
                id,
                TgaTestFile {
                    bytes: Vec::new(),
                    closed: false,
                },
            );
            let _ = name;
            self.events.push(Event::TgaOpen(name.to_vec()));
            Ok(Some(TgaFile(id)))
        }
        fn write_tga(&mut self, file: TgaFile, bytes: &[u8]) -> Result<(), Self::Error> {
            let f = self.tga_files.get_mut(&file.0).ok_or("unknown tga file")?;
            if f.closed {
                return Err("write to closed tga file");
            }
            f.bytes.extend_from_slice(bytes);
            Ok(())
        }
        fn close_tga(&mut self, file: TgaFile) -> Result<(), Self::Error> {
            let f = self.tga_files.get_mut(&file.0).ok_or("unknown tga file")?;
            f.closed = true;
            Ok(())
        }
        fn lock_level_zero(
            &mut self,
            graphics: TextureGraphics,
        ) -> Result<TgaLevelLock, Self::Error> {
            if self.tga_lock_fail {
                return Ok(TgaLevelLock::Failed);
            }
            let (data, pitch) = self.tga_lock.clone().unwrap_or_default();
            let _ = graphics;
            Ok(TgaLevelLock::Locked { data, pitch })
        }
        fn unlock_level_zero(&mut self, graphics: TextureGraphics) -> Result<(), Self::Error> {
            self.tga_unlocks.push(graphics.0);
            Ok(())
        }
    }
    impl TexturePresavedEffects for Backend {
        type Error = &'static str;
        fn open_presaved(&mut self, name: &[u8]) -> Result<Option<PresavedFile>, Self::Error> {
            self.presaved_opens.push(name.to_vec());
            let Some(bytes) = self.presaved_blobs.get(name).cloned() else {
                return Ok(None);
            };
            self.presaved_next += 1;
            let id = self.presaved_next;
            self.presaved_open.insert(
                id,
                PresavedTestFile {
                    bytes,
                    pos: 0,
                    closed: false,
                    closes: 0,
                },
            );
            Ok(Some(PresavedFile(id)))
        }
        fn read_presaved(&mut self, file: PresavedFile, out: &mut [u8]) -> Result<(), Self::Error> {
            let f = self
                .presaved_open
                .get_mut(&file.0)
                .ok_or("unknown presaved file")?;
            if f.closed {
                return Err("read from closed presaved file");
            }
            let end = f
                .pos
                .checked_add(out.len())
                .ok_or("presaved read overflow")?;
            let src = f.bytes.get(f.pos..end).ok_or("short presaved read")?;
            out.copy_from_slice(src);
            f.pos = end;
            self.presaved_read_lens.push(out.len());
            Ok(())
        }
        fn close_presaved(&mut self, file: PresavedFile) -> Result<(), Self::Error> {
            let f = self
                .presaved_open
                .get_mut(&file.0)
                .ok_or("unknown presaved file")?;
            if f.closed {
                return Err("double presaved close");
            }
            f.closed = true;
            f.closes += 1;
            Ok(())
        }
        fn presaved_level_size(
            &mut self,
            graphics: TextureGraphics,
            level: u16,
        ) -> Result<u32, Self::Error> {
            assert_eq!(Some(graphics), self.presaved_expected_graphics);
            self.presaved_level_sizes
                .get(level as usize)
                .copied()
                .ok_or("unknown presaved level")
        }
        fn write_presaved_level(
            &mut self,
            graphics: TextureGraphics,
            level: u16,
            bytes: &[u8],
        ) -> Result<(), Self::Error> {
            assert_eq!(Some(graphics), self.presaved_expected_graphics);
            self.presaved_writes.push((level, bytes.to_vec()));
            Ok(())
        }
    }
    impl CStrBackend for Backend {
        type Error = &'static str;
        fn allocate_string(&mut self, n: u32) -> Result<Option<StringAllocation>, Self::Error> {
            if self.fail_string_alloc {
                return Err("string allocation");
            }
            if self.null_strings > 0 {
                self.null_strings -= 1;
                self.events.push(Event::StringNone(n));
                return Ok(None);
            }
            let a = StringAllocation(self.next());
            assert!(self.strings.insert(a.0, n).is_none());
            self.events.push(Event::StringNew(n, a));
            Ok(Some(a))
        }
        fn free_string(&mut self, a: StringAllocation) -> Result<(), Self::Error> {
            assert!(self.strings.remove(&a.0).is_some());
            self.events.push(Event::StringFree(a));
            if self.fail_string_free {
                Err("string freed then failed")
            } else {
                Ok(())
            }
        }
    }
    impl BufferBackend for Backend {
        type Error = &'static str;
        fn allocate_buffer(
            &mut self,
            _: bm::BufferKind,
        ) -> Result<bm::AllocationToken, Self::Error> {
            let a = bm::AllocationToken(self.next());
            self.allocations.insert(a.0, true);
            Ok(a)
        }
        fn free_buffer(&mut self, a: bm::AllocationToken) -> Result<(), Self::Error> {
            assert_eq!(self.allocations.remove(&a.0), Some(true));
            self.events.push(Event::BufferFree(a));
            Ok(())
        }
        fn allocate_block(
            &mut self,
            _: bm::BufferKind,
            _: u32,
            _: u32,
        ) -> Result<bm::AllocationToken, Self::Error> {
            let a = bm::AllocationToken(self.next());
            self.allocations.insert(a.0, false);
            Ok(a)
        }
        fn free_block(&mut self, a: bm::AllocationToken) -> Result<(), Self::Error> {
            assert_eq!(self.allocations.remove(&a.0), Some(false));
            self.events.push(Event::BlockFree(a));
            Ok(())
        }
        fn create_graphics(
            &mut self,
            r: bm::GraphicsRequest,
        ) -> Result<bm::CreateResult, Self::Error> {
            let id = self.next();
            self.buffers.insert(id, false);
            Ok(bm::CreateResult::Created(match r.kind {
                bm::BufferKind::Index => bm::GraphicsBuffer::Index(vr::IndexBufferHandle(id)),
                bm::BufferKind::Vertex => bm::GraphicsBuffer::Vertex(vr::VertexBufferHandle(id)),
            }))
        }
        fn discard_managed(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn lock(
            &mut self,
            g: bm::GraphicsBuffer,
            _: u32,
        ) -> Result<bm::LockedDataKey, Self::Error> {
            let locked = self.buffers.get_mut(&Self::buffer_id(g)).unwrap();
            assert!(!*locked);
            *locked = true;
            Ok(bm::LockedDataKey(self.next()))
        }
        fn unlock(&mut self, g: bm::GraphicsBuffer) -> Result<(), Self::Error> {
            let locked = self.buffers.get_mut(&Self::buffer_id(g)).unwrap();
            assert!(*locked);
            *locked = false;
            Ok(())
        }
        fn release_graphics(&mut self, g: bm::GraphicsBuffer) -> Result<(), Self::Error> {
            assert_eq!(self.buffers.remove(&Self::buffer_id(g)), Some(false));
            self.events.push(Event::BufferGraphicsFree(g));
            Ok(())
        }
        fn driver_present(&self) -> bool {
            true
        }
        fn recompute_vram(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
        fn warn_unreleased_buffer(&mut self, _: bm::BufferKey) -> Result<(), Self::Error> {
            panic!("used buffer forced teardown forbidden")
        }
        fn out_of_memory(&mut self, _: bm::BufferKind, _: u32, _: i32) -> Result<(), Self::Error> {
            Err("fixture graphics should succeed")
        }
    }
    impl ManagedTextureEffects for Backend {
        fn worker_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureWorkerEffects<Error = Self::Error>> {
            if self.worker_enabled {
                Some(self)
            } else {
                None
            }
        }
        fn restore_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureRestoreEffects<Error = Self::Error>> {
            if self.worker_enabled {
                Some(self)
            } else {
                None
            }
        }
        type Error = &'static str;
        fn release_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureReleaseEffects<Error = Self::Error>> {
            if self.release_enabled {
                Some(self)
            } else {
                None
            }
        }
        fn creation_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureCreationEffects<Error = Self::Error>> {
            if self.creation_enabled {
                Some(self)
            } else {
                None
            }
        }
        fn request_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureRequestEffects<Error = Self::Error>> {
            if self.request_enabled {
                Some(self)
            } else {
                None
            }
        }
        fn enter_texture_section(&mut self) -> Result<(), Self::Error> {
            self.events.push(Event::Enter);
            assert!(!self.section_held);
            if self.fail_enter {
                return Err("enter rejected without lock");
            }
            self.section_held = true;
            Ok(())
        }
        fn leave_texture_section(&mut self) -> Result<(), Self::Error> {
            assert!(self.section_held);
            self.events.push(Event::Leave);
            if self.fail_leave {
                return Err("leave failed");
            }
            self.section_held = false;
            Ok(())
        }
        fn delete_managed_filename(
            &mut self,
            a: Option<ManagedFilenameAllocation>,
        ) -> Result<(), Self::Error> {
            assert!(self.section_held);
            self.events.push(Event::ManagedFilename(a));
            if let Some(a) = a {
                assert_eq!(self.managed_files.remove(&a.0), Some(true));
            }
            if self.fail_filename {
                Err("filename freed then failed")
            } else {
                Ok(())
            }
        }
        fn delete_managed_record(&mut self, a: ManagedRecordKey) -> Result<(), Self::Error> {
            assert!(self.section_held || self.creation_enabled);
            self.events.push(Event::ManagedRecord(a));
            assert_eq!(self.managed_records.remove(&a.0), Some(true));
            if self.fail_record {
                Err("record freed then failed")
            } else {
                Ok(())
            }
        }
    }
    impl ManagedTextureReleaseEffects for Backend {
        type Error = &'static str;
        fn can_release_surface(&self, s: TextureSurface) -> bool {
            self.surfaces.contains_key(&s.0)
        }
        fn can_release_texture_graphics(&self, g: TextureGraphics) -> bool {
            self.textures.contains_key(&g.0)
        }
        fn can_delete_managed_pool(&self, p: Option<ManagedPoolAllocation>) -> bool {
            p.is_none_or(|p| self.pool_arrays.contains_key(&p.0))
        }
        fn release_surface(&mut self, s: TextureSurface) -> Result<(), Self::Error> {
            assert!(!self.section_held);
            TextureBackend::release_surface(self, s)?;
            if self.fail_release_surface == Some(s) {
                Err("selected surface released then failed")
            } else {
                Ok(())
            }
        }
        fn release_texture_graphics(&mut self, g: TextureGraphics) -> Result<(), Self::Error> {
            assert!(!self.section_held);
            TextureBackend::release_texture_graphics(self, g)
        }
        fn delete_managed_pool(
            &mut self,
            p: Option<ManagedPoolAllocation>,
        ) -> Result<(), Self::Error> {
            assert!(!self.section_held);
            self.events.push(Event::DeletePool(p));
            if let Some(p) = p {
                assert_eq!(self.pool_arrays.remove(&p.0), Some(true));
            }
            if self.fail_pool {
                Err("pool freed then failed")
            } else {
                Ok(())
            }
        }
    }
    impl ManagedTextureRequestEffects for Backend {
        type Error = &'static str;
        fn can_write_managed_last_frame(&self, record: ManagedRecordKey) -> bool {
            self.managed_records.contains_key(&record.0)
        }
        fn write_managed_last_frame(
            &mut self,
            record: ManagedRecordKey,
            frame: u32,
        ) -> Result<(), Self::Error> {
            assert!(self.section_held);
            assert!(self.managed_records.contains_key(&record.0));
            self.events.push(Event::LastFrame(record, frame));
            if self.fail_last_frame {
                Err("frame write failed")
            } else {
                Ok(())
            }
        }
    }
    impl ManagedTextureCreationEffects for Backend {
        type Error = &'static str;
        fn allocate_managed_record(
            &mut self,
            bytes: u32,
        ) -> Result<Option<ManagedRecordKey>, Self::Error> {
            assert_eq!(bytes, 20);
            if self.record_none {
                return Ok(None);
            }
            let k = ManagedRecordKey(self.next());
            self.managed_records.insert(k.0, true);
            self.events.push(Event::CreateRecord(k));
            Ok(Some(k))
        }
        fn write_managed_record(
            &mut self,
            _: ManagedRecordKey,
            value: RecordWrite,
        ) -> Result<(), Self::Error> {
            self.events.push(Event::RecordWrite(value));
            Ok(())
        }
        fn allocate_managed_filename(
            &mut self,
            bytes: u32,
        ) -> Result<Option<ManagedFilenameAllocation>, Self::Error> {
            let k = if self.filename_none {
                None
            } else {
                let k = ManagedFilenameAllocation(self.next());
                self.managed_files.insert(k.0, true);
                Some(k)
            };
            self.events.push(Event::CreateFilename(bytes, k));
            Ok(k)
        }
        fn copy_managed_filename(
            &mut self,
            k: ManagedFilenameAllocation,
            bytes: &[u8],
        ) -> Result<(), Self::Error> {
            self.filename_bytes.insert(k.0, bytes.to_vec());
            self.events.push(Event::CopyFilename(bytes.to_vec()));
            Ok(())
        }
        fn allocate_managed_registry(
            &mut self,
            bytes: u32,
        ) -> Result<Option<ManagedRegistryAllocation>, Self::Error> {
            let k = if self.array_none {
                None
            } else {
                let k = ManagedRegistryAllocation(self.next());
                self.arrays.insert(k.0, vec![None; bytes as usize / 4]);
                Some(k)
            };
            self.events.push(Event::CreateArray(bytes, k));
            Ok(k)
        }
        fn write_managed_registry_entry(
            &mut self,
            a: ManagedRegistryAllocation,
            i: u32,
            r: ManagedRecordKey,
        ) -> Result<(), Self::Error> {
            self.arrays.get_mut(&a.0).unwrap()[i as usize] = Some(r);
            self.events.push(Event::ArrayWrite(i, r));
            if self.fail_copy_array {
                Err("array write then failed")
            } else {
                Ok(())
            }
        }
        fn delete_managed_registry(
            &mut self,
            a: Option<ManagedRegistryAllocation>,
        ) -> Result<(), Self::Error> {
            if let Some(a) = a {
                self.arrays.remove(&a.0).unwrap();
            }
            self.events.push(Event::DeleteArray(a));
            Ok(())
        }
        fn managed_texture_low_memory(&mut self) -> Result<(), Self::Error> {
            self.events.push(Event::LowMemory);
            Ok(())
        }
    }
    impl TextureBackend for Backend {
        fn managed_texture_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureEffects<Error = &'static str>> {
            if self.managed_enabled {
                Some(self)
            } else {
                None
            }
        }
        fn log_diagnostic(&mut self, message: TextureDiagnostic) {
            self.diagnostics.push(render_diagnostic(&message));
        }
        fn duplicate_effects(
            &mut self,
        ) -> Option<&mut dyn TextureDuplicateEffects<Error = &'static str>> {
            if self.no_dup_effects {
                None
            } else {
                Some(self)
            }
        }
        fn tga_effects(&mut self) -> Option<&mut dyn TextureTgaEffects<Error = &'static str>> {
            if self.no_tga_effects {
                None
            } else {
                Some(self)
            }
        }
        fn presaved_effects(
            &mut self,
        ) -> Option<&mut dyn TexturePresavedEffects<Error = &'static str>> {
            if self.presaved_enabled {
                Some(self)
            } else {
                None
            }
        }

        fn unlock_surface(&mut self, s: TextureSurface) -> Result<(), &'static str> {
            assert!(self.surfaces.contains_key(&s.0));
            self.events.push(Event::SurfaceUnlock(s));
            Ok(())
        }
        fn release_surface(&mut self, s: TextureSurface) -> Result<(), &'static str> {
            let n = self.surfaces.get_mut(&s.0).unwrap();
            *n -= 1;
            if *n == 0 {
                self.surfaces.remove(&s.0);
            }
            self.events.push(Event::SurfaceFree(s));
            if self.fail_surface_free {
                Err("surface released then failed")
            } else {
                Ok(())
            }
        }
        fn release_texture_graphics(&mut self, t: TextureGraphics) -> Result<(), &'static str> {
            let n = self.textures.get_mut(&t.0).unwrap();
            *n -= 1;
            if *n == 0 {
                self.textures.remove(&t.0);
            }
            self.events.push(Event::TextureFree(t));
            if self.fail_texture_free {
                Err("texture released then failed")
            } else {
                Ok(())
            }
        }
        fn delete_pixels(&mut self, p: PixelAllocation) -> Result<(), &'static str> {
            assert!(self.pixels.remove(&p.0).is_some());
            self.events.push(Event::PixelsFree(p));
            Ok(())
        }
        fn free_raw_vertices(
            &mut self,
            _: MeshLevelKey,
            _: RawVertexBytesKey,
        ) -> Result<(), &'static str> {
            Err("raw geometry not supplied")
        }
    }
    impl Backend {
        fn worker_event(&mut self, stage: ManagedStage) -> Result<(), &'static str> {
            self.worker_events.push(stage);
            if self.worker_fail == Some(stage) {
                Err("worker interrupted")
            } else {
                Ok(())
            }
        }
    }
    impl ManagedTextureWorkerEffects for Backend {
        type Error = &'static str;
        fn managed_worker_running(&mut self) -> Result<bool, Self::Error> {
            self.worker_event(ManagedStage::Running)?;
            let run = self.worker_checks > 0;
            self.worker_checks = self.worker_checks.saturating_sub(1);
            Ok(run)
        }
        fn managed_worker_frame(&mut self) -> Result<u32, Self::Error> {
            self.worker_event(ManagedStage::Frame)?;
            Ok(self.worker_frame)
        }
        fn managed_worker_sleep(&mut self, ms: u32) -> Result<(), Self::Error> {
            assert_eq!(ms, 0);
            assert!(!self.section_held);
            self.worker_event(ManagedStage::Sleep)
        }
        fn lock_managed_surface(
            &mut self,
            s: TextureSurface,
            flags: u32,
        ) -> Result<Option<ManagedLockedRect>, Self::Error> {
            assert!(self.surfaces.contains_key(&s.0));
            assert_eq!(flags, 0);
            self.worker_event(ManagedStage::LockSurface)?;
            self.worker_data = vec![0xa5; 64];
            Ok(Some(ManagedLockedRect {
                mapping: 1,
                pitch: -7,
                writable_bytes: 64,
            }))
        }
        fn open_managed_file(
            &mut self,
            f: ManagedFilenameAllocation,
            mode: u32,
        ) -> Result<Option<ManagedFile>, Self::Error> {
            assert!(self.managed_files.contains_key(&f.0));
            assert_eq!(mode, 0);
            self.worker_event(ManagedStage::OpenFile)?;
            // Host admission, followed by reconstructed dtaSeek/dtaRead/dtaClose.
            // This synthetic fixture does not reconstruct dtaOpen.
            let mut bytes = vec![0xcc; 22];
            bytes.extend_from_slice(&[1, 2, 3]);
            let files = lhr_assets::rw_data::Handles::new(WorkerFileBackend { bytes, position: 0 });
            let handle = files
                .admit(lhr_assets::rw_data::HandleRecord {
                    file: 42,
                    flags: 1,
                    cursor: lhr_assets::rw_data::ReadCursor::new(25),
                    cache_valid: false,
                    temporary_name: 0,
                    temporary_name_bytes: vec![],
                    block: 1,
                    wav: 0,
                    timestamp: 0,
                })
                .unwrap();
            files
                .attach_loose_cache(
                    handle,
                    lhr_assets::rw_data::LooseReadCache {
                        bytes: vec![0; 32768],
                        block_start_file_pointer: 0,
                        block_size: 0,
                    },
                )
                .unwrap();
            self.worker_files = Some(files);
            assert_eq!(handle, 0);
            Ok(Some(ManagedFile(u64::from(handle))))
        }
        fn seek_managed_file(
            &mut self,
            f: ManagedFile,
            o: i32,
            origin: u32,
        ) -> Result<(), Self::Error> {
            assert_eq!((f, o, origin), (ManagedFile(0), 22, 1));
            self.worker_event(ManagedStage::SeekFile)?;
            assert_eq!(
                self.worker_files
                    .as_ref()
                    .unwrap()
                    .dta_seek(f.0 as u32, o, origin)
                    .unwrap(),
                22
            );
            Ok(())
        }
        fn read_managed_file(
            &mut self,
            f: ManagedFile,
            m: ManagedLockedRect,
            n: u32,
        ) -> Result<(), Self::Error> {
            assert_eq!(f, ManagedFile(0));
            assert_eq!(m.mapping, 1);
            assert_eq!(n, 64);
            assert_eq!(
                self.worker_files
                    .as_ref()
                    .unwrap()
                    .dta_read(f.0 as u32, Some(&mut self.worker_data[..n as usize]))
                    .unwrap(),
                3
            );
            self.worker_event(ManagedStage::ReadFile)
        }
        fn close_managed_file(&mut self, f: ManagedFile) -> Result<(), Self::Error> {
            assert_eq!(f, ManagedFile(0));
            self.worker_event(ManagedStage::CloseFile)?;
            assert!(self
                .worker_files
                .as_ref()
                .unwrap()
                .dta_close(f.0 as u32)
                .unwrap());
            Ok(())
        }
        fn unlock_managed_surface(&mut self, _: TextureSurface) -> Result<(), Self::Error> {
            self.worker_event(ManagedStage::UnlockSurface)
        }
        fn update_managed_texture(
            &mut self,
            s: TextureGraphics,
            d: TextureGraphics,
        ) -> Result<(), Self::Error> {
            assert!(self.textures.contains_key(&s.0));
            assert!(self.textures.contains_key(&d.0));
            assert_eq!(&self.worker_data[..4], &[1, 2, 3, 0xa5]);
            self.worker_event(ManagedStage::UpdateTexture)
        }
        fn release_managed_worker_reference(
            &mut self,
            _: TextureKey,
            refs: u32,
            g: TextureGraphics,
            flags: u32,
        ) -> Result<(), Self::Error> {
            assert!(self.section_held);
            assert!(refs >= 2);
            assert!(self.textures.contains_key(&g.0));
            assert_eq!(flags & 0xc0000000, 0x80000000);
            self.worker_event(ManagedStage::ReleaseWorkerReference)
        }
    }
    impl ManagedTextureRestoreEffects for Backend {
        type Error = &'static str;
        fn create_managed_texture(
            &mut self,
            _: RestoreTarget,
            r: ManagedTextureCreate,
        ) -> Result<Option<TextureGraphics>, Self::Error> {
            self.worker_creates.push(r);
            assert_eq!(r.width, r.height);
            assert_eq!(r.levels, 1);
            assert_eq!(r.usage, 0);
            let g = TextureGraphics(self.next());
            self.textures.insert(g.0, 1);
            self.worker_event(ManagedStage::CreateTexture)?;
            Ok(Some(g))
        }
        fn get_managed_surface_level(
            &mut self,
            _: RestoreTarget,
            _: TextureGraphics,
            l: u32,
        ) -> Result<Option<TextureSurface>, Self::Error> {
            assert_eq!(l, 0);
            let s = TextureSurface(self.next());
            self.surfaces.insert(s.0, 1);
            self.worker_event(ManagedStage::GetSurfaceLevel)?;
            Ok(Some(s))
        }
        fn get_managed_surface_desc(
            &mut self,
            _: RestoreTarget,
            _: TextureSurface,
        ) -> Result<ManagedSurfaceDesc, Self::Error> {
            self.worker_event(ManagedStage::GetSurfaceDesc)?;
            Ok(ManagedSurfaceDesc([23, 3, 0, 2, 64, 0, 0x10009, 0x2000b]))
        }
        fn allocate_managed_pool(
            &mut self,
            _: u32,
        ) -> Result<Option<ManagedPoolAllocation>, Self::Error> {
            let a = ManagedPoolAllocation(self.next());
            self.pool_arrays.insert(a.0, true);
            self.worker_event(ManagedStage::AllocatePool)?;
            Ok(Some(a))
        }
        fn copy_managed_pool_slot(
            &mut self,
            _: ManagedPoolAllocation,
            _: ManagedPoolAllocation,
            _: u32,
        ) -> Result<(), Self::Error> {
            self.worker_event(ManagedStage::CopyPool)
        }
        fn clear_managed_pool_owner(&mut self, _: RestoreTarget) -> Result<(), Self::Error> {
            self.worker_event(ManagedStage::WritePoolOwner)
        }
    }
    fn managed_fixture(
        r: &mut TextureResources,
        b: &mut Backend,
        duplicate: bool,
        missing: bool,
    ) -> (TextureKey, ManagedWorldIntake) {
        let key = r.create_texture().unwrap();
        let mut input = b.input();
        let graphics = input.graphics.take().unwrap();
        r.install_texture(key, input, b).unwrap();
        let mut records = Vec::new();
        if !missing {
            for i in 0..if duplicate { 2 } else { 1 } {
                let file = ManagedFilenameAllocation(b.next());
                b.managed_files.insert(file.0, true);
                let allocation = ManagedRecordKey(b.next());
                b.managed_records.insert(allocation.0, true);
                records.push(RecordIntake {
                    owner: key,
                    allocation,
                    file_name: Some(file),
                    preserved: vec![i as u8; 12],
                });
            }
        }
        let record = records.first().map(|r| r.allocation);
        let mut queue = [None; 256];
        if !missing {
            for i in [0, 1, 7, 128, 254, 255] {
                queue[i] = Some(key);
            }
        }
        let pools = std::array::from_fn(|pool| {
            (0..2)
                .map(|index| {
                    let g = if pool == 0 && index == 0 {
                        graphics
                    } else {
                        let g = TextureGraphics(b.next());
                        b.textures.insert(g.0, 1);
                        g
                    };
                    let surface = TextureSurface(b.next());
                    b.surfaces.insert(surface.0, 1);
                    PoolIntake {
                        owner: if index == 0 { Some(key) } else { None },
                        preserved: [(pool * 2 + index) as u8; 40],
                        resources: PoolResources {
                            graphics: Some(g),
                            surface: Some(surface),
                        },
                    }
                })
                .collect()
        });
        (
            key,
            ManagedWorldIntake {
                records,
                queue,
                counters: QueueCounters {
                    out: 17,
                    put: 91,
                    len: if missing { 0 } else { 6 },
                },
                pools,
                bindings: vec![ManagedBinding {
                    texture: key,
                    flags: if missing { 0x8000014e } else { 0xc000014e },
                    record,
                    graphics: Some(graphics),
                }],
            },
        )
    }
    fn request(n: u32) -> VertexRequest {
        VertexRequest {
            fvf: 0x112,
            usage: 0,
            stride: 32,
            count: n,
            format: 0,
            exclusive: false,
        }
    }
    fn file_rep(r: &TextureResources, k: TextureKey, alpha: bool) -> Option<CRepKey> {
        let t = r.texture(k).unwrap();
        r.strings()
            .string(if alpha {
                t.alpha_file_name
            } else {
                t.file_name
            })
            .unwrap()
            .rep
    }
    #[test]
    fn c_str_shared_alias_and_self_view_copy_before_release_with_owned_budget() {
        let mut s = CStrings::new().unwrap();
        let mut b = Backend::default();
        let a = s.create_null().unwrap();
        s.assign(a, Some(b"abcdef"), &mut b).unwrap();
        let rep = s.string(a).unwrap().rep.unwrap();
        let z = s.copy_string(a).unwrap();
        assert_eq!(s.rep(rep).unwrap().references, 2);
        assert_eq!(
            s.release_rep(rep, &mut b),
            Err(StringOperationError::State(StringError::ReferenceOwned))
        );
        s.assign_from_view(a, RepView { rep, offset: 2 }, &mut b)
            .unwrap();
        let new = s.string(a).unwrap().rep.unwrap();
        assert_eq!(s.rep(new).unwrap().bytes, b"cdef");
        assert_eq!(s.rep(rep).unwrap().references, 1);
        s.destroy_string(z, &mut b).unwrap();
        s.destroy_string(a, &mut b).unwrap();
        assert!(b.strings.is_empty());
        let bare = s.create_rep(b"x", &mut b).unwrap().unwrap();
        s.retain_rep(bare).unwrap();
        s.release_rep(bare, &mut b).unwrap();
        assert_eq!(s.rep(bare).unwrap().references, 1);
        s.release_rep(bare, &mut b).unwrap();
        assert!(s.rep(bare).is_err());
        assert!(b.strings.is_empty());
    }
    #[test]
    fn c_str_empty_null_and_returning_null_allocator_remain_distinct() {
        let mut s = CStrings::new().unwrap();
        let mut b = Backend::default();
        let k = s.create_null().unwrap();
        s.assign(k, Some(b""), &mut b).unwrap();
        let first = s.string(k).unwrap().rep.unwrap();
        assert_eq!(s.rep(first).unwrap().size, 0);
        s.assign(k, Some(b""), &mut b).unwrap();
        assert_ne!(s.string(k).unwrap().rep, Some(first));
        b.null_strings = 1;
        s.assign(k, Some(b"unavailable"), &mut b).unwrap();
        assert_eq!(s.string(k).unwrap().rep, None);
        assert!(b.strings.is_empty());
        s.assign(k, None, &mut b).unwrap();
        assert!(!s.is_poisoned());
        assert!(b.events.contains(&Event::StringNone(20)));
    }
    #[test]
    fn c_str_release_error_preserves_pending_new_rep_and_does_not_replay() {
        let mut s = CStrings::new().unwrap();
        let mut b = Backend::default();
        let k = s.create_null().unwrap();
        s.assign(k, Some(b"old"), &mut b).unwrap();
        let old = s.string(k).unwrap().rep;
        b.fail_string_free = true;
        assert_eq!(
            s.assign(k, Some(b"new"), &mut b),
            Err(StringOperationError::Backend("string freed then failed"))
        );
        let state = s.string(k).unwrap();
        assert_eq!(state.rep, old);
        assert!(state.pending.is_some());
        assert_eq!(s.rep(state.pending.unwrap()).unwrap().bytes, b"new");
        assert_eq!(s.rep(old.unwrap()).unwrap().references, 0);
        assert!(s.is_poisoned());
        let before = b.events.len();
        assert!(s.assign(k, None, &mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn constructor_unknown_fields_and_repeated_close_allocate_fresh_empty_reps() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let t = r.texture(k).unwrap();
        assert_eq!(
            (t.width, t.height, t.format, t.color_key, t.levels),
            (
                Observed::Unavailable,
                Observed::Unavailable,
                Observed::Unavailable,
                Observed::Unavailable,
                Observed::Unavailable
            )
        );
        assert_eq!(t.name, Observed::Unavailable);
        assert_eq!(
            (t.references, t.external_references, t.material_references),
            (1, 1, 0)
        );
        r.close_texture(k, &mut b).unwrap();
        let first = file_rep(&r, k, false);
        assert_eq!(r.texture(k).unwrap().width, Observed::Value(0));
        assert_eq!(r.texture(k).unwrap().levels, Observed::Unavailable);
        r.close_texture(k, &mut b).unwrap();
        assert_ne!(file_rep(&r, k, false), first);
        assert_eq!(
            b.events
                .iter()
                .filter(|e| matches!(e, Event::StringNew(9, _)))
                .count(),
            4
        );
        r.release_texture(k, &mut b).unwrap();
        assert!(r.texture(k).is_err());
        assert!(b.strings.is_empty());
    }
    #[test]
    fn close_orders_surface_pixels_names_cache_and_graphics_but_keeps_creation_state() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let input = b.input();
        let (surface, pixels, graphics) = (
            input.lock_surface.unwrap(),
            input.pixels.unwrap(),
            input.graphics.unwrap(),
        );
        r.install_texture(k, input, &mut b).unwrap();
        let a = r.create_texture().unwrap();
        let z = r.create_texture().unwrap();
        for key in [k, a, z] {
            r.register_cache(key, CacheList::Cached).unwrap();
            r.register_cache(key, CacheList::Uncached).unwrap();
        }
        assert_eq!(
            r.register_cache(k, CacheList::Cached),
            Err(TextureError::DuplicateCache)
        );
        b.events.clear();
        r.close_texture(k, &mut b).unwrap();
        assert_eq!(
            &b.events[..3],
            &[
                Event::SurfaceUnlock(surface),
                Event::SurfaceFree(surface),
                Event::PixelsFree(pixels)
            ]
        );
        assert_eq!(b.events.last(), Some(&Event::TextureFree(graphics)));
        assert_eq!(r.cached(), &[z, a]);
        assert_eq!(r.uncached(), &[z, a]);
        let t = r.texture(k).unwrap();
        assert!(!t.initialized);
        assert_eq!(
            (t.uid, t.creation_flags, t.levels),
            (0x1234, 0xfeed0123, Observed::Value(3))
        );
        assert_eq!(t.name, Observed::Value(None));
        assert_eq!((t.references, t.flags, t.memory_size), (1, 0, 0));
        for key in [k, a, z] {
            r.release_texture(key, &mut b).unwrap();
        }
        assert!(b.strings.is_empty());
    }
    #[test]
    fn aliased_material_slots_own_three_refs_and_final_release_drops_alpha_then_primary() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let input = b.input();
        r.install_texture(k, input, &mut b).unwrap();
        let a = r.create_material().unwrap();
        let z = r.create_material().unwrap();
        r.set_texture(a, Some(k), &mut b).unwrap();
        r.set_env_texture(a, Some(k), &mut b).unwrap();
        r.set_texture(z, Some(k), &mut b).unwrap();
        assert_eq!(
            (
                r.texture(k).unwrap().references,
                r.texture(k).unwrap().material_references
            ),
            (4, 3)
        );
        let before = b.events.len();
        r.set_texture(a, Some(k), &mut b).unwrap();
        assert_eq!(b.events.len(), before);
        r.release_texture(k, &mut b).unwrap();
        assert_eq!(
            r.release_texture(k, &mut b),
            Err(TextureOperationError::State(TextureError::ReferenceOwned))
        );
        r.release_material(a, &mut b).unwrap();
        assert_eq!(r.texture(k).unwrap().references, 1);
        assert!(b.textures.len() == 1);
        b.events.clear();
        r.release_material(z, &mut b).unwrap();
        let empty: Vec<_> = b
            .events
            .iter()
            .filter_map(|e| {
                if let Event::StringNew(9, a) = e {
                    Some(*a)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(empty.len(), 2);
        assert_eq!(
            &b.events[b.events.len() - 2..],
            &[Event::StringFree(empty[1]), Event::StringFree(empty[0])]
        );
        assert!(b.strings.is_empty());
        assert!(b.textures.is_empty());
        assert!(r.texture(k).is_err());
    }
    #[test]
    fn real_object_material_texture_chain_shares_resources_until_last_object() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let texture = r.create_texture().unwrap();
        let input = b.input();
        r.install_texture(texture, input, &mut b).unwrap();
        let a = r.create_material().unwrap();
        let z = r.create_material().unwrap();
        r.set_texture(a, Some(texture), &mut b).unwrap();
        r.set_env_texture(a, Some(texture), &mut b).unwrap();
        r.set_texture(z, Some(texture), &mut b).unwrap();
        r.release_texture(texture, &mut b).unwrap();
        let g1 = r.create_face_group().unwrap();
        let g2 = r.create_face_group().unwrap();
        r.set_material(g1, Some(a), &mut b).unwrap();
        r.set_material(g2, Some(z), &mut b).unwrap();
        r.allocate_group_indices(g1, 1, 0, &mut b).unwrap();
        r.allocate_group_indices(g2, 1, 0, &mut b).unwrap();
        r.link_next(g1, g2).unwrap();
        r.release_material(a, &mut b).unwrap();
        r.release_material(z, &mut b).unwrap();
        let level = r.create_mesh_level().unwrap();
        r.allocate_level_vertices(level, 4, request(4), Some(g1), &mut b)
            .unwrap();
        r.install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
            .unwrap();
        let mesh = r.create_mesh_object().unwrap();
        r.set_lod(mesh, Some(level), 0, &mut b).unwrap();
        r.release_mesh_level(level, &mut b).unwrap();
        let o1 = r.create_object_resources(Some(mesh)).unwrap();
        let o2 = r.create_object_resources(Some(mesh)).unwrap();
        for o in [o1, o2] {
            let lod = r.create_plain_lod().unwrap();
            r.allocate_plain_lod_vertices(lod, request(1), &mut b)
                .unwrap();
            r.adopt_plain_lod(o, 9, lod).unwrap();
        }
        r.release_mesh_object(mesh, &mut b).unwrap();
        r.destroy_object_resources(o1, &mut b).unwrap();
        assert_eq!(r.texture(texture).unwrap().references, 3);
        assert!(b.events.iter().all(|e| !matches!(e, Event::TextureFree(_))));
        r.destroy_object_resources(o2, &mut b).unwrap();
        assert!(r.texture(texture).is_err());
        assert!(b.textures.is_empty());
        assert!(b.strings.is_empty());
        assert!(b.surfaces.is_empty());
        assert!(b.pixels.is_empty());
        assert_eq!(r.objects().managed().managers().index_buffers().len(), 1);
        r.reclaim_unused_indices(&mut b).unwrap();
        assert!(b.allocations.is_empty());
        assert!(b.buffers.is_empty());
    }
    #[test]
    fn shared_names_external_copy_and_nondelete_leave_raw_rep_pointers_protected() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let mut input = b.input();
        input.alpha_file_name = AlphaName::ShareFile;
        input.alpha_name_offset = Some(5);
        r.install_texture(k, input, &mut b).unwrap();
        let rep = file_rep(&r, k, false).unwrap();
        assert_eq!(file_rep(&r, k, true), Some(rep));
        let external = r.copy_file_name(k, false).unwrap();
        assert_eq!(r.strings().rep(rep).unwrap().references, 3);
        let embedded = r.texture(k).unwrap().file_name;
        assert_eq!(
            r.destroy_string(embedded, &mut b),
            Err(TextureOperationError::State(
                TextureError::UnknownExternalString
            ))
        );
        r.close_texture(k, &mut b).unwrap();
        assert_eq!(r.strings().rep(rep).unwrap().references, 1);
        r.destroy_string(external, &mut b).unwrap();
        r.destruct_texture(k, &mut b).unwrap();
        assert_eq!(r.texture(k).unwrap().state, TextureState::Destructed);
        let key = r.texture(k).unwrap().file_name;
        assert!(r.strings().string(key).unwrap().destructed);
        assert!(r.strings().string(key).unwrap().rep.is_some());
        assert!(b.strings.is_empty());
        assert!(r.release_texture(k, &mut b).is_err());
    }
    #[test]
    fn standalone_close_does_not_recompute_existing_material_flags() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let input = b.input();
        r.install_texture(k, input, &mut b).unwrap();
        let m = r.create_material().unwrap();
        r.set_texture(m, Some(k), &mut b).unwrap();
        let flags = r.objects().managed().resources().material(m).unwrap().flags;
        r.close_texture(k, &mut b).unwrap();
        assert_eq!(r.texture(k).unwrap().flags, 0);
        assert_eq!(
            r.objects().managed().resources().material(m).unwrap().flags,
            flags
        );
        r.release_texture(k, &mut b).unwrap();
        r.release_material(m, &mut b).unwrap();
        assert!(b.strings.is_empty());
    }
    #[test]
    fn surface_release_error_keeps_prefix_and_blocks_all_arenas_without_replay() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let input = b.input();
        let surface = input.lock_surface;
        r.install_texture(k, input, &mut b).unwrap();
        b.fail_surface_free = true;
        assert_eq!(
            r.close_texture(k, &mut b),
            Err(TextureOperationError::Backend(
                "surface released then failed"
            ))
        );
        assert_eq!(r.texture(k).unwrap().lock_surface, surface);
        assert!(r.texture(k).unwrap().pixels.is_some());
        assert!(r.texture(k).unwrap().initialized);
        assert!(r.is_poisoned());
        let before = b.events.len();
        assert_eq!(r.create_object_resources(None), Err(TextureError::Poisoned));
        assert!(r.release_texture(k, &mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn intake_failure_keeps_adopted_payload_handles_in_poisoned_texture() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let input = b.input();
        let handles = (input.graphics, input.lock_surface, input.pixels);
        b.fail_string_alloc = true;
        assert_eq!(
            r.install_texture(k, input, &mut b),
            Err(TextureOperationError::Backend("string allocation"))
        );
        let t = r.texture(k).unwrap();
        assert_eq!((t.graphics, t.lock_surface, t.pixels), handles);
        assert!(t.initialized);
        assert!(r.is_poisoned());
        assert_eq!(b.textures.len(), 1);
        assert_eq!(b.surfaces.len(), 1);
        assert_eq!(b.pixels.len(), 1);
        let before = b.events.len();
        assert!(r.close_texture(k, &mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn foreign_dead_and_managed_inputs_reject_before_old_texture_or_payload_effects() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        for flag in [0x40, 0x40000000, 0x80000000] {
            let mut input = b.input();
            input.flags = flag;
            let before = b.events.len();
            assert_eq!(
                r.install_texture(k, input, &mut b),
                Err(TextureOperationError::State(
                    TextureError::ManagedUnsupported
                ))
            );
            assert_eq!(b.events.len(), before);
        }
        r.textures.texture_mut(k).managed_record = Some(ManagedRecordKey(99));
        let before = b.events.len();
        assert_eq!(
            r.release_texture(k, &mut b),
            Err(TextureOperationError::State(
                TextureError::ManagedUnsupported
            ))
        );
        assert_eq!(r.texture(k).unwrap().references, 1);
        assert_eq!(b.events.len(), before);
        r.textures.texture_mut(k).managed_record = None;
        let m = r.create_material().unwrap();
        r.set_texture(m, Some(k), &mut b).unwrap();
        r.release_texture(k, &mut b).unwrap();
        let mut other = TextureResources::new().unwrap();
        let foreign = other.create_texture().unwrap();
        let dead = r.create_texture().unwrap();
        r.release_texture(dead, &mut b).unwrap();
        let before = b.events.len();
        assert_eq!(
            r.set_texture(m, Some(foreign), &mut b),
            Err(TextureOperationError::State(TextureError::ForeignArena))
        );
        assert!(r.set_texture(m, Some(dead), &mut b).is_err());
        assert_eq!(b.events.len(), before);
        assert_eq!(r.texture(k).unwrap().references, 1);
        assert!(!r.is_poisoned());
        r.release_material(m, &mut b).unwrap();
    }
    #[test]
    fn graphics_release_error_preserves_cleared_cache_and_names_but_not_final_init_store() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let input = b.input();
        let graphics = input.graphics;
        r.install_texture(k, input, &mut b).unwrap();
        r.register_cache(k, CacheList::Cached).unwrap();
        r.register_cache(k, CacheList::Uncached).unwrap();
        b.fail_texture_free = true;
        assert_eq!(
            r.close_texture(k, &mut b),
            Err(TextureOperationError::Backend(
                "texture released then failed"
            ))
        );
        let t = r.texture(k).unwrap();
        assert_eq!(t.graphics, graphics);
        assert!(t.initialized);
        assert_eq!(t.lock_surface, None);
        assert_eq!(t.pixels, None);
        assert_eq!(t.flags, 0);
        assert_eq!(t.name, Observed::Value(None));
        assert!(r.cached().is_empty() && r.uncached().is_empty());
        assert_eq!(
            r.strings()
                .rep(file_rep(&r, k, false).unwrap())
                .unwrap()
                .size,
            0
        );
        assert!(r.is_poisoned());
        let before = b.events.len();
        assert!(r.release_texture(k, &mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn replacement_ref_overflow_rejects_before_destroying_old_and_equality_skips_increment() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let old = r.create_texture().unwrap();
        let new = r.create_texture().unwrap();
        let m = r.create_material().unwrap();
        r.set_texture(m, Some(old), &mut b).unwrap();
        r.release_texture(old, &mut b).unwrap();
        let t = r.textures.texture_mut(new);
        t.references = i32::MAX as u32;
        t.external_references = i32::MAX as u32;
        let before = b.events.len();
        assert_eq!(
            r.set_texture(m, Some(new), &mut b),
            Err(TextureOperationError::State(
                TextureError::ReferenceOverflow
            ))
        );
        assert_eq!(r.texture(old).unwrap().references, 1);
        assert_eq!(b.events.len(), before);
        let t = r.textures.texture_mut(old);
        t.references = i32::MAX as u32;
        t.external_references = i32::MAX as u32 - 1;
        r.set_texture(m, Some(old), &mut b).unwrap();
        assert_eq!(r.texture(old).unwrap().references, i32::MAX as u32);
        assert_eq!(b.events.len(), before);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn close_allocation_none_is_native_null_assignment_without_early_return() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = r.create_texture().unwrap();
        let input = b.input();
        r.install_texture(k, input, &mut b).unwrap();
        b.null_strings = 1;
        r.close_texture(k, &mut b).unwrap();
        assert_eq!(file_rep(&r, k, false), None);
        assert!(file_rep(&r, k, true).is_some());
        assert!(!r.is_poisoned());
        r.release_texture(k, &mut b).unwrap();
        assert!(b.strings.is_empty());
    }
    #[test]
    fn managed_remove_preserves_stale_fields_counters_and_all_pool_owned_resources() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let (k, input) = managed_fixture(&mut r, &mut b, false, false);
        let counters = input.counters;
        let record = input.records[0].allocation;
        let filename = input.records[0].file_name;
        let graphics = input.bindings[0].graphics;
        r.install_managed_world(input).unwrap();
        let record_id = r.texture_manager().active_records()[0];
        b.events.clear();
        assert!(r.remove_managed_texture(k, &mut b).unwrap());
        assert_eq!(
            b.events,
            vec![
                Event::Enter,
                Event::ManagedFilename(filename),
                Event::ManagedRecord(record),
                Event::Leave
            ]
        );
        assert_eq!(r.texture(k).unwrap().managed_record, Some(record));
        assert_eq!(r.texture(k).unwrap().graphics, graphics);
        assert_eq!(r.texture(k).unwrap().flags, 0x14e);
        assert_eq!(r.texture_manager().counters(), counters);
        assert!(r.texture_manager().queue().iter().all(Option::is_none));
        assert!(r.texture_manager().active_records().is_empty());
        assert_eq!(
            r.texture_manager().record(record_id).unwrap().phase,
            RecordPhase::RecordFreed
        );
        for (pool, slots) in r.texture_manager().pools().iter().enumerate() {
            for (index, slot) in slots.iter().enumerate() {
                assert_eq!(slot.owner, None);
                assert_eq!(slot.preserved, [(pool * 2 + index) as u8; 40]);
                assert!(slot.resources.graphics.is_some());
                assert!(slot.resources.surface.is_some());
            }
        }
        assert_eq!(b.textures.len(), 6);
        assert_eq!(b.surfaces.len(), 7);
        b.events.clear();
        r.remove_managed_texture(k, &mut b).unwrap();
        assert_eq!(b.events, vec![Event::Enter, Event::Leave]);
        r.close_texture(k, &mut b).unwrap();
        assert_eq!(r.texture(k).unwrap().managed_record, None);
        assert_eq!(r.texture(k).unwrap().graphics, None);
        assert_eq!(r.texture(k).unwrap().flags, 0);
        r.release_texture(k, &mut b).unwrap();
        assert!(b.strings.is_empty());
        assert_eq!(b.textures.len(), 6);
        assert_eq!(b.surfaces.len(), 6);
        assert!(!b.events.iter().any(|e| matches!(e, Event::TextureFree(_))));
    }
    #[test]
    fn managed_duplicate_records_select_reverse_match_and_close_waits_for_safe_count() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let (k, mut input) = managed_fixture(&mut r, &mut b, true, false);
        let first = input.records[0].allocation;
        let last = input.records[1].allocation;
        let filename = input.records[1].file_name.take();
        b.managed_files.remove(&filename.unwrap().0);
        r.install_managed_world(input).unwrap();
        let before = b.events.len();
        assert!(r.close_texture(k, &mut b).is_err());
        assert_eq!(b.events.len(), before);
        assert!(!r.is_poisoned());
        r.remove_managed_texture(k, &mut b).unwrap();
        assert!(b.events.contains(&Event::ManagedFilename(None)));
        assert!(b.events.contains(&Event::ManagedRecord(last)));
        assert_eq!(r.texture_manager().active_records().len(), 1);
        assert_eq!(
            r.texture_manager()
                .record(r.texture_manager().active_records()[0])
                .unwrap()
                .allocation,
            first
        );
        assert_eq!(r.texture(k).unwrap().managed_record, Some(first));
        r.close_texture(k, &mut b).unwrap();
        assert!(r.texture_manager().active_records().is_empty());
        assert!(b.managed_records.is_empty());
        r.release_texture(k, &mut b).unwrap();
    }
    #[test]
    fn managed_missing_record_still_clears_loaded_pool_owners_without_graphics_release() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let (k, input) = managed_fixture(&mut r, &mut b, false, true);
        r.install_managed_world(input).unwrap();
        b.events.clear();
        r.remove_managed_texture(k, &mut b).unwrap();
        assert_eq!(b.events, vec![Event::Enter, Event::Leave]);
        assert!(r
            .texture_manager()
            .pools()
            .iter()
            .flatten()
            .all(|s| s.owner.is_none()));
        assert!(r.texture(k).unwrap().graphics.is_some());
        assert_eq!(r.texture(k).unwrap().flags, 0x14e);
        r.release_texture(k, &mut b).unwrap();
        assert_eq!(b.textures.len(), 6);
    }
    #[test]
    fn managed_missing_capability_precedes_last_decrement_unlock_and_nondelete_poison() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let (k, input) = managed_fixture(&mut r, &mut b, false, false);
        r.install_managed_world(input).unwrap();
        let lock = r.texture(k).unwrap().lock_surface;
        let before = b.events.len();
        assert_eq!(
            r.release_texture(k, &mut b),
            Err(TextureOperationError::State(
                TextureError::ManagedUnsupported
            ))
        );
        assert_eq!(
            r.destruct_texture(k, &mut b),
            Err(TextureOperationError::State(
                TextureError::ManagedUnsupported
            ))
        );
        assert_eq!(r.texture(k).unwrap().references, 1);
        assert_eq!(r.texture(k).unwrap().lock_surface, lock);
        assert_eq!(b.events.len(), before);
        assert!(!r.is_poisoned());
        b.managed_enabled = true;
        r.destruct_texture(k, &mut b).unwrap();
        assert_eq!(r.texture(k).unwrap().state, TextureState::Destructed);
        assert_eq!(b.textures.len(), 6);
    }
    #[test]
    fn managed_body_failure_attempts_one_leave_and_retains_both_errors_and_prefix() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let (k, input) = managed_fixture(&mut r, &mut b, false, false);
        r.install_managed_world(input).unwrap();
        let id = r.texture_manager().active_records()[0];
        b.events.clear();
        b.fail_filename = true;
        b.fail_leave = true;
        assert_eq!(
            r.remove_managed_texture(k, &mut b),
            Err(TextureOperationError::Manager(
                ManagedOperationError::Backend {
                    stage: ManagedStage::Filename,
                    error: "filename freed then failed",
                    leave_error: Some("leave failed")
                }
            ))
        );
        assert_eq!(
            r.texture_manager().critical_state(),
            CriticalState::LeaveFailed
        );
        assert_eq!(
            r.texture_manager().record(id).unwrap().phase,
            RecordPhase::FreeingFilename
        );
        assert_eq!(r.texture_manager().active_records(), &[id]);
        assert!(r.texture_manager().queue().iter().all(Option::is_none));
        assert!(r
            .texture_manager()
            .pools()
            .iter()
            .flatten()
            .any(|s| s.owner == Some(k)));
        assert_eq!(r.texture(k).unwrap().flags, 0xc000014e);
        assert!(r.is_poisoned());
        assert_eq!(
            b.events
                .iter()
                .filter(|e| matches!(e, Event::Leave))
                .count(),
            1
        );
        let before = b.events.len();
        assert!(r.remove_managed_texture(k, &mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn managed_enter_failure_after_release_preserves_zero_ref_and_unlocked_surface_prefix() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let (k, input) = managed_fixture(&mut r, &mut b, false, false);
        r.install_managed_world(input).unwrap();
        b.fail_enter = true;
        b.events.clear();
        assert_eq!(
            r.release_texture(k, &mut b),
            Err(TextureOperationError::Manager(
                ManagedOperationError::Backend {
                    stage: ManagedStage::Enter,
                    error: "enter rejected without lock",
                    leave_error: None
                }
            ))
        );
        assert_eq!(r.texture(k).unwrap().references, 0);
        assert_eq!(r.texture(k).unwrap().lock_surface, None);
        assert_eq!(r.texture_manager().critical_state(), CriticalState::NotHeld);
        assert!(r.texture_manager().queue().iter().any(Option::is_some));
        assert!(!b.section_held);
        assert!(!b.events.contains(&Event::Leave));
        assert!(r.is_poisoned());
    }
    #[test]
    fn managed_leave_failure_keeps_completed_body_but_not_close_pointer_clears() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let (k, input) = managed_fixture(&mut r, &mut b, false, false);
        r.install_managed_world(input).unwrap();
        b.fail_leave = true;
        assert_eq!(
            r.close_texture(k, &mut b),
            Err(TextureOperationError::Manager(
                ManagedOperationError::Backend {
                    stage: ManagedStage::Leave,
                    error: "leave failed",
                    leave_error: None
                }
            ))
        );
        assert!(r.texture_manager().active_records().is_empty());
        assert!(r
            .texture_manager()
            .pools()
            .iter()
            .flatten()
            .all(|p| p.owner.is_none()));
        assert_eq!(r.texture(k).unwrap().flags, 0x14e);
        assert!(r.texture(k).unwrap().managed_record.is_some());
        assert!(r.texture(k).unwrap().graphics.is_some());
        assert!(r.texture(k).unwrap().pixels.is_some());
        assert!(r.is_poisoned());
    }
    #[test]
    fn managed_intake_guards_scalar_allocation_aliases_owned_graphics_and_queue_gates() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let (_key, mut input) = managed_fixture(&mut r, &mut b, false, false);
        input.counters.len = 0;
        assert!(r.install_managed_world(input).is_err());
        assert!(!r.texture_manager().is_seeded());
        assert!(!r.is_poisoned());
        let mut r = TextureResources::new().unwrap();
        let (k, mut input) = managed_fixture(&mut r, &mut b, false, false);
        let rep = file_rep(&r, k, false).unwrap();
        input.records[0].file_name = Some(ManagedFilenameAllocation(
            r.strings().rep(rep).unwrap().allocation.0,
        ));
        let before = b.events.len();
        assert_eq!(
            r.install_managed_world(input),
            Err(TextureError::Manager(ManagerError::InvalidIdentity))
        );
        assert_eq!(b.events.len(), before);
        assert!(!r.texture_manager().is_seeded());
        let mut r = TextureResources::new().unwrap();
        let (k, input) = managed_fixture(&mut r, &mut b, false, false);
        let graphics = input.bindings[0].graphics;
        r.textures.texture_mut(k).graphics = graphics;
        assert_eq!(
            r.install_managed_world(input),
            Err(TextureError::ManagedUnsupported)
        );
        assert!(!r.is_poisoned());
    }
    #[test]
    fn managed_seeded_constructor_cannot_be_overwritten_by_normal_ownership_intake() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let key = r.create_texture().unwrap();
        let filename = ManagedFilenameAllocation(b.next());
        b.managed_files.insert(filename.0, true);
        let record = ManagedRecordKey(b.next());
        b.managed_records.insert(record.0, true);
        r.install_managed_world(ManagedWorldIntake {
            records: vec![RecordIntake {
                owner: key,
                allocation: record,
                file_name: Some(filename),
                preserved: Vec::new(),
            }],
            queue: [None; 256],
            counters: QueueCounters::default(),
            pools: [Vec::new(), Vec::new(), Vec::new()],
            bindings: vec![ManagedBinding {
                texture: key,
                flags: 0x40,
                record: Some(record),
                graphics: None,
            }],
        })
        .unwrap();
        let input = TextureIntake {
            uid: 0,
            graphics: None,
            lock_surface: None,
            pixels: None,
            initialized: false,
            width: 0,
            height: 0,
            format: 0,
            flags: 0,
            creation_flags: 0,
            color_key: 0,
            levels: 0,
            memory_size: 0,
            file_name: Some(b"overwrite"),
            name_offset: Some(0),
            alpha_file_name: AlphaName::Null,
            alpha_name_offset: None,
        };
        let before = b.events.len();
        assert_eq!(
            r.install_texture(key, input, &mut b),
            Err(TextureOperationError::State(
                TextureError::IntakeAlreadyInstalled
            ))
        );
        assert_eq!(b.events.len(), before);
        assert_eq!(r.texture(key).unwrap().flags, 0x40);
        assert_eq!(r.texture(key).unwrap().managed_record, Some(record));
        assert!(!r.is_poisoned());
        r.release_texture(key, &mut b).unwrap();
        assert!(b.managed_files.is_empty());
        assert!(b.managed_records.is_empty());
        assert!(b.strings.is_empty());
    }
    #[test]
    fn managed_object_chain_preflights_backend_before_ancestor_effects_then_releases_only_owned_resources(
    ) {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let (k, input) = managed_fixture(&mut r, &mut b, false, false);
        r.install_managed_world(input).unwrap();
        let material = r.create_material().unwrap();
        r.set_texture(material, Some(k), &mut b).unwrap();
        r.set_env_texture(material, Some(k), &mut b).unwrap();
        r.release_texture(k, &mut b).unwrap();
        let group = r.create_face_group().unwrap();
        r.set_material(group, Some(material), &mut b).unwrap();
        r.release_material(material, &mut b).unwrap();
        r.allocate_group_indices(group, 1, 0, &mut b).unwrap();
        let level = r.create_mesh_level().unwrap();
        r.allocate_level_vertices(level, 3, request(3), Some(group), &mut b)
            .unwrap();
        r.install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
            .unwrap();
        let mesh = r.create_mesh_object().unwrap();
        r.set_lod(mesh, Some(level), 0, &mut b).unwrap();
        r.release_mesh_level(level, &mut b).unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        r.release_mesh_object(mesh, &mut b).unwrap();
        b.managed_enabled = false;
        let before = b.events.len();
        assert_eq!(
            r.destroy_object_resources(object, &mut b),
            Err(TextureOperationError::State(
                TextureError::ManagedUnsupported
            ))
        );
        assert_eq!(b.events.len(), before);
        assert_eq!(
            r.objects().object(object).unwrap().phase,
            crate::i3d_object::ObjectCleanupPhase::Alive
        );
        assert_eq!(r.texture(k).unwrap().references, 2);
        assert!(!r.is_poisoned());
        b.managed_enabled = true;
        r.destroy_object_resources(object, &mut b).unwrap();
        assert!(r.texture(k).is_err());
        assert!(b.managed_files.is_empty());
        assert!(b.managed_records.is_empty());
        assert!(b.strings.is_empty());
        assert_eq!(b.textures.len(), 6);
        assert_eq!(b.surfaces.len(), 6);
        assert!(!b.events.iter().any(|e| matches!(e, Event::TextureFree(_))));
        r.reclaim_unused_indices(&mut b).unwrap();
        assert!(b.allocations.is_empty());
        assert!(b.buffers.is_empty());
    }
    fn creation_world(flags: u32) -> (TextureResources, Backend, TextureKey) {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            creation_enabled: true,
            ..Backend::default()
        };
        let t = r.create_texture().unwrap();
        r.configure_managed_registry(RegistryIntake {
            capacity: 0,
            growth: 2,
            allocation: None,
        })
        .unwrap();
        r.prepare_managed_texture(t, flags).unwrap();
        b.events.clear();
        (r, b, t)
    }
    fn add(
        r: &mut TextureResources,
        b: &mut Backend,
        t: TextureKey,
        name: &[u8],
        flags: u32,
    ) -> Result<bool, TextureOperationError<&'static str>> {
        r.add_texture_to_texture_manager(
            t,
            name,
            flags,
            CreationFormats {
                dxt: 0x31545844,
                normal: 0x17,
            },
            b,
        )
    }
    #[test]
    fn unload_sparse_duplicate_and_already_unloaded_owners_preserves_every_other_resource() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            request_enabled: true,
            ..Backend::default()
        };
        let (loaded, mut input) = managed_fixture(&mut r, &mut b, false, false);
        let unloaded = r.create_texture().unwrap();
        input.bindings.push(ManagedBinding {
            texture: unloaded,
            flags: 0x40000140,
            record: None,
            graphics: None,
        });
        input.pools[0][1].owner = Some(unloaded);
        input.pools[1][0].owner = None;
        input.pools[2][1].owner = Some(loaded);
        // The ordinary intake remains strict. Cleanup intake changes only the
        // loaded-bit requirement, admitting its already-unloaded-owner path.
        assert!(r.install_managed_world(input).is_err());
        let (loaded2, mut input) = managed_fixture(&mut r, &mut b, false, false);
        input.bindings.push(ManagedBinding {
            texture: unloaded,
            flags: 0x40000140,
            record: None,
            graphics: None,
        });
        input.pools[0][1].owner = Some(unloaded);
        input.pools[1][0].owner = None;
        input.pools[2][1].owner = Some(loaded2);
        let queue = input.queue;
        let counters = input.counters;
        let preserved: Vec<_> = input
            .pools
            .iter()
            .flatten()
            .map(|s| (s.preserved, s.resources))
            .collect();
        r.install_managed_unload_world(input).unwrap();
        let record = r.texture_manager().active_records()[0];
        let graphics = r.texture(loaded2).unwrap().graphics;
        let references = r.texture(loaded2).unwrap().references;
        let textures = b.textures.clone();
        let surfaces = b.surfaces.clone();
        let files = b.managed_files.clone();
        let records = b.managed_records.clone();
        assert!(r.request_texture(loaded2, 122, &mut b).unwrap());
        let record_before = format!("{:?}", r.texture_manager().record(record).unwrap());
        b.events.clear();
        r.unload_managed_textures(&mut b).unwrap();
        assert_eq!(b.events, [Event::Enter, Event::Leave]);
        assert_eq!(r.texture(loaded2).unwrap().flags, 0x4000014e);
        assert_eq!(r.texture(unloaded).unwrap().flags, 0x40000140);
        assert_eq!(r.texture(loaded2).unwrap().graphics, graphics);
        assert_eq!(r.texture(loaded2).unwrap().references, references);
        assert_eq!(r.texture_manager().queue(), &queue);
        assert_eq!(r.texture_manager().counters(), counters);
        assert_eq!(
            format!("{:?}", r.texture_manager().record(record).unwrap()),
            record_before
        );
        assert!(r
            .texture_manager()
            .pools()
            .iter()
            .flatten()
            .all(|s| s.owner.is_none()));
        assert_eq!(
            r.texture_manager()
                .pools()
                .iter()
                .flatten()
                .map(|s| (s.preserved, s.resources))
                .collect::<Vec<_>>(),
            preserved
        );
        assert_eq!(
            (
                b.textures.clone(),
                b.surfaces.clone(),
                b.managed_files.clone(),
                b.managed_records.clone()
            ),
            (textures, surfaces, files, records)
        );
        b.events.clear();
        r.unload_managed_textures(&mut b).unwrap();
        assert_eq!(b.events, [Event::Enter, Event::Leave]);
        // Already queued Request refreshes storage but cannot claim availability;
        // the intentionally stale borrowed graphics survives both operations.
        assert!(!r.request_texture(loaded2, 123, &mut b).unwrap());
        assert_eq!(r.texture(loaded2).unwrap().graphics, graphics);
        assert_eq!(
            r.texture_manager().record(record).unwrap().last_frame,
            Some(123)
        );
        assert_eq!(r.texture_manager().counters(), counters);
        assert_eq!(r.texture(loaded).unwrap().flags & 0x40, 0);
        r.close_texture(loaded2, &mut b).unwrap();
        assert!(r.texture(loaded2).unwrap().graphics.is_none());
        assert!(r.texture(loaded2).unwrap().managed_record.is_none());
        // The texture borrow was cleared, with pool-owned references retained.
        assert_eq!(b.textures.len(), 12);
    }
    #[test]
    fn unload_loaded_request_becomes_queued_without_releasing_stale_graphics() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            request_enabled: true,
            ..Backend::default()
        };
        let (key, mut input) = managed_fixture(&mut r, &mut b, false, false);
        input.bindings[0].flags &= !0x40000000;
        input.queue = [None; 256];
        input.counters.len = 0;
        r.install_managed_world(input).unwrap();
        let graphics = r.texture(key).unwrap().graphics;
        let textures = b.textures.clone();
        assert!(r.request_texture(key, 5, &mut b).unwrap());
        r.unload_managed_textures(&mut b).unwrap();
        assert!(!r.request_texture(key, 6, &mut b).unwrap());
        assert_eq!(r.texture(key).unwrap().flags & 0xc0000000, 0x40000000);
        assert_eq!(r.texture_manager().queue()[91], Some(key));
        assert_eq!(
            r.texture_manager().counters(),
            QueueCounters {
                out: 17,
                put: 92,
                len: 1
            }
        );
        assert_eq!(r.texture(key).unwrap().graphics, graphics);
        assert_eq!(b.textures, textures);
        r.close_texture(key, &mut b).unwrap();
        assert!(r.texture(key).unwrap().graphics.is_none());
        assert_eq!(b.textures, textures);
    }
    #[test]
    fn unload_manager_visits_pool_order_reverse_slots_and_duplicate_owners() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            ..Backend::default()
        };
        let keys: Vec<_> = (0..4).map(|_| r.create_texture().unwrap()).collect();
        let mut pools: [Vec<PoolIntake>; 3] = std::array::from_fn(|_| {
            (0..6)
                .map(|_| PoolIntake {
                    owner: None,
                    preserved: [0; 40],
                    resources: PoolResources {
                        graphics: None,
                        surface: None,
                    },
                })
                .collect()
        });
        let visits = [
            (0, 5, 0),
            (0, 2, 1),
            (0, 0, 0),
            (1, 4, 2),
            (1, 1, 3),
            (2, 5, 1),
            (2, 3, 0),
        ];
        for (pool, slot, owner) in visits {
            pools[pool][slot].owner = Some(keys[owner]);
        }
        r.install_managed_unload_world(ManagedWorldIntake {
            records: vec![],
            queue: [None; 256],
            counters: QueueCounters {
                out: 0,
                put: 0,
                len: 0,
            },
            pools,
            bindings: keys
                .iter()
                .map(|&texture| ManagedBinding {
                    texture,
                    flags: 0x40,
                    record: None,
                    graphics: None,
                })
                .collect(),
        })
        .unwrap();
        let mut seen = Vec::new();
        r.textures
            .manager
            .unload(|owner| seen.push(owner), &mut b)
            .unwrap();
        assert_eq!(seen, visits.map(|(_, _, owner)| keys[owner]));
        assert!(r
            .texture_manager()
            .pools()
            .iter()
            .flatten()
            .all(|s| s.owner.is_none()));
        seen.clear();
        r.textures
            .manager
            .unload(|owner| seen.push(owner), &mut b)
            .unwrap();
        assert!(seen.is_empty());
        assert_eq!(
            b.events,
            [Event::Enter, Event::Leave, Event::Enter, Event::Leave]
        );
    }
    #[test]
    fn unload_empty_and_missing_backend_have_exact_lock_behavior() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        assert!(r.unload_managed_textures(&mut b).is_err());
        assert!(b.events.is_empty());
        b.managed_enabled = true;
        r.unload_managed_textures(&mut b).unwrap();
        assert_eq!(b.events, [Event::Enter, Event::Leave]);
    }
    #[test]
    fn unload_lock_failures_poison_and_retain_the_completed_prefix() {
        for enter in [true, false] {
            let mut r = TextureResources::new().unwrap();
            let mut b = Backend {
                managed_enabled: true,
                ..Backend::default()
            };
            let (key, input) = managed_fixture(&mut r, &mut b, false, false);
            r.install_managed_world(input).unwrap();
            b.fail_enter = enter;
            b.fail_leave = !enter;
            b.events.clear();
            assert_eq!(
                r.unload_managed_textures(&mut b),
                Err(TextureOperationError::Manager(
                    ManagedOperationError::Backend {
                        stage: if enter {
                            ManagedStage::Enter
                        } else {
                            ManagedStage::Leave
                        },
                        error: if enter {
                            "enter rejected without lock"
                        } else {
                            "leave failed"
                        },
                        leave_error: None,
                    }
                ))
            );
            assert_eq!(r.texture(key).unwrap().flags & 0x80000000 != 0, enter);
            assert_eq!(
                r.texture_manager().pools()[0][0].owner,
                if enter { Some(key) } else { None }
            );
            assert_eq!(
                b.events,
                if enter {
                    vec![Event::Enter]
                } else {
                    vec![Event::Enter, Event::Leave]
                }
            );
            assert!(r.texture_manager().is_poisoned());
            let count = b.events.len();
            assert!(r.unload_managed_textures(&mut b).is_err());
            assert_eq!(b.events.len(), count);
        }
    }
    #[test]
    fn request_actual_creation_repeat_remove_close_preserves_weak_queue_contract() {
        let (mut r, mut b, t) = creation_world(0x40);
        b.request_enabled = true;
        add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
        let id = r.texture_manager().active_records()[0];
        let record = r.texture(t).unwrap().managed_record.unwrap();
        b.events.clear();
        assert!(!r.request_texture(t, u32::MAX, &mut b).unwrap());
        assert_eq!(
            b.events,
            vec![
                Event::Enter,
                Event::LastFrame(record, u32::MAX),
                Event::Leave
            ]
        );
        assert_eq!(
            r.texture_manager().counters(),
            QueueCounters {
                out: 0,
                put: 1,
                len: 1
            }
        );
        assert_eq!(r.texture_manager().queue()[0], Some(t));
        assert_eq!(r.texture(t).unwrap().references, 1);
        assert!(!r.request_texture(t, 7, &mut b).unwrap());
        assert_eq!(r.texture_manager().counters().len, 1);
        assert_eq!(
            r.texture_manager()
                .record(id)
                .unwrap()
                .fields
                .unwrap()
                .last_frame,
            7
        );
        r.remove_managed_texture(t, &mut b).unwrap();
        assert_eq!(r.texture_manager().queue()[0], None);
        assert_eq!(r.texture_manager().counters().len, 1);
        b.events.clear();
        assert!(r.request_texture(t, 8, &mut b).is_err());
        assert!(b.events.is_empty());
        r.close_texture(t, &mut b).unwrap();
        r.release_texture(t, &mut b).unwrap();
        assert!(b.managed_records.is_empty());
    }
    #[test]
    fn request_seeded_loaded_queued_full_and_wrap_refresh_before_return() {
        for (flags, len, put, expected, enqueue) in [
            (0x80000040, 3, 999, true, false),
            (0x40000040, 3, 999, false, false),
            (0x40, 256, 999, false, false),
            (0x40, 3, 255, false, true),
        ] {
            let mut r = TextureResources::new().unwrap();
            let mut b = Backend {
                managed_enabled: true,
                request_enabled: true,
                ..Backend::default()
            };
            let (t, mut input) = managed_fixture(&mut r, &mut b, false, false);
            // Keep adopted graphics owned by pools, but avoid falsely binding
            // borrowed graphics to a texture outside its loaded route.
            input.bindings[0].flags = flags;
            input.bindings[0].graphics = None;
            for pool in &mut input.pools {
                for slot in pool {
                    slot.owner = None;
                }
            }
            input.queue = [None; 256];
            if enqueue {
                input.queue[255] = Some(t);
                input.bindings[0].flags |= 0x40000000;
            }
            input.counters = QueueCounters { out: 19, put, len };
            r.install_managed_world(input).unwrap();
            if enqueue {
                r.textures.texture_mut(t).flags &= !0x40000000;
            }
            let id = r.texture_manager().active_records()[0];
            b.events.clear();
            assert_eq!(r.request_texture(t, 0x87654321, &mut b).unwrap(), expected);
            assert_eq!(
                r.texture_manager().record(id).unwrap().last_frame,
                Some(0x87654321)
            );
            assert_eq!(b.events.len(), 3);
            assert_eq!(
                r.texture_manager().counters(),
                QueueCounters {
                    out: 19,
                    put: if enqueue { 0 } else { put },
                    len: len + i32::from(enqueue)
                }
            );
            assert_eq!(
                r.texture_manager().queue()[255],
                if enqueue { Some(t) } else { None }
            );
        }
    }
    #[test]
    fn request_nonmanaged_shortcut_and_missing_capability_are_preeffect() {
        let (mut r, mut b, t) = creation_world(0);
        assert!(r.request_texture(t, 99, &mut b).unwrap());
        assert!(b.events.is_empty());
        let (mut r, mut b, t) = creation_world(0x40);
        add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
        b.events.clear();
        assert!(matches!(
            r.request_texture(t, 1, &mut b),
            Err(TextureOperationError::Manager(
                ManagedOperationError::State(ManagerError::RequestUnavailable)
            ))
        ));
        assert!(b.events.is_empty());
        assert!(!r.is_poisoned());
    }
    #[test]
    fn request_storage_domain_enter_and_leave_failures_preserve_exact_prefix() {
        for failure in 0..3 {
            let (mut r, mut b, t) = creation_world(0x40);
            add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
            b.request_enabled = true;
            let record = r.texture(t).unwrap().managed_record.unwrap();
            if failure == 0 {
                b.managed_records.remove(&record.0);
            }
            b.fail_enter = failure == 1;
            b.fail_leave = failure == 2;
            b.events.clear();
            assert!(r.request_texture(t, 5, &mut b).is_err());
            assert_eq!(b.events.len(), [0, 1, 3][failure]);
            assert_eq!(r.is_poisoned(), failure != 0);
            assert_eq!(r.texture_manager().counters().len, i32::from(failure == 2));
            assert_eq!(r.texture(t).unwrap().flags & 0x40000000 != 0, failure == 2);
        }
    }
    #[test]
    fn request_leave_failure_retains_completed_enqueue_and_frame_without_retry() {
        let (mut r, mut b, t) = creation_world(0x40);
        add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
        let id = r.texture_manager().active_records()[0];
        b.request_enabled = true;
        b.fail_leave = true;
        b.events.clear();
        assert_eq!(
            r.request_texture(t, 77, &mut b),
            Err(TextureOperationError::Manager(
                ManagedOperationError::Backend {
                    stage: ManagedStage::Leave,
                    error: "leave failed",
                    leave_error: None
                }
            ))
        );
        assert_eq!(
            r.texture_manager()
                .record(id)
                .unwrap()
                .fields
                .unwrap()
                .last_frame,
            77
        );
        assert_eq!(r.texture_manager().record(id).unwrap().last_frame, Some(77));
        assert_eq!(r.texture_manager().queue()[0], Some(t));
        assert_eq!(
            r.texture_manager().counters(),
            QueueCounters {
                out: 0,
                put: 1,
                len: 1
            }
        );
        assert_eq!(r.texture(t).unwrap().flags, 0x40000040);
        assert_eq!(
            r.texture_manager().critical_state(),
            CriticalState::LeaveFailed
        );
        assert!(r.is_poisoned());
        assert_eq!(b.events.len(), 3);
        assert!(r.request_texture(t, 78, &mut b).is_err());
        assert_eq!(b.events.len(), 3);
    }
    #[test]
    fn request_invalid_put_is_host_guard_before_enter_or_record_write() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            request_enabled: true,
            ..Backend::default()
        };
        let (t, mut input) = managed_fixture(&mut r, &mut b, false, false);
        input.bindings[0].flags = 0x40;
        input.bindings[0].graphics = None;
        input.queue = [None; 256];
        for pool in &mut input.pools {
            for slot in pool {
                slot.owner = None;
            }
        }
        input.counters.put = 256;
        r.install_managed_world(input).unwrap();
        b.events.clear();
        assert_eq!(
            r.request_texture(t, 5, &mut b),
            Err(TextureOperationError::Manager(
                ManagedOperationError::State(ManagerError::InvalidCount)
            ))
        );
        assert!(b.events.is_empty());
        assert!(!r.is_poisoned());
    }
    #[test]
    fn request_write_and_leave_failure_preserve_both_errors_without_queue_mutation() {
        let (mut r, mut b, t) = creation_world(0x40);
        add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
        b.request_enabled = true;
        b.fail_last_frame = true;
        b.fail_leave = true;
        b.events.clear();
        assert_eq!(
            r.request_texture(t, 3, &mut b),
            Err(TextureOperationError::Manager(
                ManagedOperationError::Backend {
                    stage: ManagedStage::WriteRecord,
                    error: "frame write failed",
                    leave_error: Some("leave failed")
                }
            ))
        );
        assert_eq!(b.events.len(), 3);
        assert_eq!(r.texture_manager().counters(), QueueCounters::default());
        assert_eq!(r.texture(t).unwrap().flags, 0x40);
        assert!(r.is_poisoned());
        let count = b.events.len();
        assert!(r.request_texture(t, 4, &mut b).is_err());
        assert_eq!(b.events.len(), count);
    }
    #[test]
    fn actual_add_prepared_close_releases_record_and_keeps_caller_flags_unchanged() {
        let (mut r, mut b, t) = creation_world(0x40);
        assert!(add(&mut r, &mut b, t, b"stone2.bmp", 0).unwrap());
        let texture = r.texture(t).unwrap();
        assert_eq!(
            (texture.flags, texture.creation_flags, texture.initialized),
            (0x40, 0x40, false)
        );
        let record = r
            .texture_manager()
            .record(r.texture_manager().active_records()[0])
            .unwrap();
        assert_eq!(
            record.filename_bytes.as_deref(),
            Some(b"stone2_1.565\0".as_slice())
        );
        assert_eq!(
            record.fields.unwrap(),
            ManagedRecordFields {
                format: 0x17,
                pool: 2,
                last_frame: 0
            }
        );
        assert_eq!(r.release_texture(t, &mut b).unwrap(), 0);
        assert!(b.managed_records.is_empty());
        assert!(b.managed_files.is_empty());
        assert_eq!(r.texture_manager().registry().unwrap().capacity, 2);
        assert_eq!(b.arrays.len(), 1);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn flags_zero_add_remove_close_preserves_stale_pointer_and_blocks_unsafe_close() {
        let (mut r, mut b, t) = creation_world(0);
        add(&mut r, &mut b, t, b"x0.tga", 0).unwrap();
        let ptr = r.texture(t).unwrap().managed_record;
        let n = b.events.len();
        assert!(r.close_texture(t, &mut b).is_err());
        assert_eq!(b.events.len(), n);
        assert!(!r.is_poisoned());
        r.remove_managed_texture(t, &mut b).unwrap();
        r.close_texture(t, &mut b).unwrap();
        assert_eq!(r.texture(t).unwrap().managed_record, ptr);
        assert_eq!(r.texture(t).unwrap().flags, 0);
        r.release_texture(t, &mut b).unwrap();
    }
    #[test]
    fn filename_none_is_false_deletes_record_diagnoses_and_allows_retry() {
        let (mut r, mut b, t) = creation_world(0x40);
        b.filename_none = true;
        assert!(!add(&mut r, &mut b, t, b"stone1.bmp", 0).unwrap());
        assert!(r.texture(t).unwrap().managed_record.is_none());
        assert!(!r.is_poisoned());
        assert_eq!(r.texture_manager().registry().unwrap().capacity, 0);
        assert!(b.managed_records.is_empty());
        assert_eq!(b.events.last(), Some(&Event::LowMemory));
        assert!(!b
            .events
            .iter()
            .any(|e| matches!(e, Event::Enter | Event::CreateArray(..))));
        b.filename_none = false;
        assert!(add(&mut r, &mut b, t, b"stone1.bmp", 0x200).unwrap());
        r.release_texture(t, &mut b).unwrap();
    }
    #[test]
    fn growth_copies_active_order_then_deletes_old_array_and_removal_updates_swap_slot() {
        let (mut r, mut b, t) = creation_world(0x40);
        let mut keys = vec![t];
        for _ in 0..2 {
            let k = r.create_texture().unwrap();
            r.prepare_managed_texture(k, 0x40).unwrap();
            keys.push(k);
        }
        for k in &keys {
            add(&mut r, &mut b, *k, b"test0.bmp", 0).unwrap();
        }
        assert_eq!(r.texture_manager().registry().unwrap().capacity, 4);
        let a = r.texture_manager().registry().unwrap().allocation.unwrap();
        let records: Vec<_> = r
            .texture_manager()
            .active_records()
            .iter()
            .map(|&id| r.texture_manager().record(id).unwrap().allocation)
            .collect();
        assert_eq!(
            b.arrays[&a.0][..3],
            records.iter().copied().map(Some).collect::<Vec<_>>()
        );
        r.remove_managed_texture(keys[0], &mut b).unwrap();
        assert_eq!(b.arrays[&a.0][0], Some(records[2]));
        for k in keys {
            r.release_texture(k, &mut b).unwrap();
        }
        assert!(b.managed_records.is_empty());
    }
    #[test]
    fn format_branches_signed_pool_and_byte_lengths_preserve_original_domain() {
        for (name, flags, formats, suffix, pool) in [
            (
                b"a\x80.bmp".as_slice(),
                0x200,
                CreationFormats { dxt: 7, normal: 23 },
                b"_1.DX1\0".as_slice(),
                -176,
            ),
            (
                b"a\xff.bmp".as_slice(),
                0x200,
                CreationFormats { dxt: 0, normal: 17 },
                b"_5.565\0".as_slice(),
                -49,
            ),
            (
                b"0.bmp".as_slice(),
                0,
                CreationFormats { dxt: 7, normal: 23 },
                b"_1.565\0".as_slice(),
                0,
            ),
        ] {
            let (mut r, mut b, t) = creation_world(0x40);
            r.add_texture_to_texture_manager(t, name, flags, formats, &mut b)
                .unwrap();
            let record = r
                .texture_manager()
                .record(r.texture_manager().active_records()[0])
                .unwrap();
            assert_eq!(record.fields.unwrap().pool, pool);
            assert!(record.filename_bytes.as_ref().unwrap().ends_with(suffix));
            let writes: Vec<_> = b
                .events
                .iter()
                .filter_map(|e| {
                    if let Event::RecordWrite(w) = e {
                        Some(*w)
                    } else {
                        None
                    }
                })
                .collect();
            assert!(
                matches!(writes[2], RecordWrite::Format(_))
                    == (flags & 0x200 != 0 && formats.dxt != 0)
            );
            r.release_texture(t, &mut b).unwrap();
        }
        let (mut r, mut b, t) = creation_world(0x40);
        let name = vec![b'1'; 253];
        assert!(add(&mut r, &mut b, t, &name, 0).unwrap());
        assert_eq!(
            r.texture_manager()
                .record(r.texture_manager().active_records()[0])
                .unwrap()
                .filename_bytes
                .as_ref()
                .unwrap()
                .len(),
            256
        );
    }
    #[test]
    fn invalid_or_unavailable_creation_is_preeffect_retryable() {
        let (mut r, mut b, t) = creation_world(0x40);
        let n = b.events.len();
        assert!(add(&mut r, &mut b, t, b"bad", 0).is_err());
        assert_eq!(b.events.len(), n);
        b.creation_enabled = false;
        assert!(add(&mut r, &mut b, t, b"stone1.bmp", 0).is_err());
        assert_eq!(b.events.len(), n);
        assert!(!r.is_poisoned());
        b.creation_enabled = true;
        add(&mut r, &mut b, t, b"stone1.bmp", 0).unwrap();
        let n = b.events.len();
        assert!(add(&mut r, &mut b, t, b"stone1.bmp", 0).is_err());
        assert_eq!(b.events.len(), n);
    }
    #[test]
    fn array_none_preserves_published_texture_and_expanded_capacity_without_rollback() {
        let (mut r, mut b, t) = creation_world(0x40);
        b.array_none = true;
        assert!(add(&mut r, &mut b, t, b"stone1.bmp", 0).is_err());
        assert!(r.is_poisoned());
        assert!(r.texture(t).unwrap().managed_record.is_some());
        assert_eq!(r.texture_manager().registry().unwrap().capacity, 2);
        assert!(r.texture_manager().registry().unwrap().allocation.is_none());
        assert!(r.texture_manager().active_records().is_empty());
        let p = r.texture_manager().creation_progress().unwrap();
        assert!(p.published);
        assert!(p.record.is_some() && p.filename.is_some());
        assert_eq!(b.managed_records.len(), 1);
        let n = b.events.len();
        assert!(r.remove_managed_texture(t, &mut b).is_err());
        assert_eq!(b.events.len(), n);
    }
    #[test]
    fn record_none_is_host_unsupported_and_never_native_false() {
        let (mut r, mut b, t) = creation_world(0x40);
        b.record_none = true;
        assert!(matches!(
            add(&mut r, &mut b, t, b"stone1.bmp", 0),
            Err(TextureOperationError::Manager(
                ManagedOperationError::State(ManagerError::UnsupportedAllocation)
            ))
        ));
        assert!(r.is_poisoned());
        assert!(r.texture(t).unwrap().managed_record.is_none());
        assert!(!b.events.contains(&Event::LowMemory));
    }
    #[test]
    fn partial_registry_copy_error_retains_old_and_new_arrays_and_published_record() {
        let (mut r, mut b, t) = creation_world(0x40);
        add(&mut r, &mut b, t, b"stone1.bmp", 0).unwrap();
        let u = r.create_texture().unwrap();
        r.prepare_managed_texture(u, 0x40).unwrap();
        add(&mut r, &mut b, u, b"stone2.bmp", 0).unwrap();
        let old = r.texture_manager().registry().unwrap().allocation;
        let v = r.create_texture().unwrap();
        r.prepare_managed_texture(v, 0x40).unwrap();
        b.fail_copy_array = true;
        assert!(add(&mut r, &mut b, v, b"stone3.bmp", 0).is_err());
        assert!(r.is_poisoned());
        assert_eq!(r.texture_manager().registry().unwrap().allocation, old);
        assert_eq!(b.arrays.len(), 2);
        assert_eq!(r.texture_manager().active_records().len(), 2);
        assert!(r
            .texture_manager()
            .creation_progress()
            .unwrap()
            .new_array
            .is_some());
    }
    #[test]
    fn direct_removal_requires_creation_capability_before_any_delete_and_can_retry() {
        let (mut r, mut b, t) = creation_world(0x40);
        add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
        let u = r.create_texture().unwrap();
        r.prepare_managed_texture(u, 0x40).unwrap();
        add(&mut r, &mut b, u, b"two0.bmp", 0).unwrap();
        let n = b.events.len();
        b.creation_enabled = false;
        assert!(r.remove_managed_texture(t, &mut b).is_err());
        assert_eq!(b.events.len(), n);
        assert!(!r.is_poisoned());
        assert_eq!(b.managed_records.len(), 2);
        b.creation_enabled = true;
        r.remove_managed_texture(t, &mut b).unwrap();
        assert_eq!(r.texture_manager().active_records().len(), 1);
    }
    #[test]
    fn removal_write_error_keeps_decremented_count_and_pending_swap_without_replay() {
        let (mut r, mut b, t) = creation_world(0x40);
        add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
        let u = r.create_texture().unwrap();
        r.prepare_managed_texture(u, 0x40).unwrap();
        add(&mut r, &mut b, u, b"two0.bmp", 0).unwrap();
        let last = r.texture_manager().active_records()[1];
        b.fail_copy_array = true;
        assert!(r.remove_managed_texture(t, &mut b).is_err());
        assert!(r.is_poisoned());
        assert!(!b.section_held);
        assert_eq!(r.texture_manager().active_records().len(), 1);
        assert_eq!(r.texture_manager().pending_registry_swap(), Some((0, last)));
        assert_eq!(b.managed_records.len(), 1);
        let n = b.events.len();
        assert!(r.remove_managed_texture(t, &mut b).is_err());
        assert_eq!(b.events.len(), n);
    }
    #[test]
    fn removed_last_still_copies_freed_pointer_into_inactive_registry_slot() {
        let (mut r, mut b, t) = creation_world(0x40);
        add(&mut r, &mut b, t, b"one0.bmp", 0).unwrap();
        let u = r.create_texture().unwrap();
        r.prepare_managed_texture(u, 0x40).unwrap();
        add(&mut r, &mut b, u, b"two0.bmp", 0).unwrap();
        let ptr = r.texture(u).unwrap().managed_record.unwrap();
        b.events.clear();
        r.remove_managed_texture(u, &mut b).unwrap();
        let copy = b
            .events
            .iter()
            .position(|e| *e == Event::ArrayWrite(1, ptr))
            .unwrap();
        let delete = b
            .events
            .iter()
            .position(|e| *e == Event::ManagedRecord(ptr))
            .unwrap();
        assert!(copy > delete);
        assert_eq!(r.texture_manager().active_records().len(), 1);
    }
    fn release_world() -> (TextureResources, Backend, TextureKey, Vec<Event>) {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            release_enabled: true,
            ..Backend::default()
        };
        let (key, input) = managed_fixture(&mut r, &mut b, false, false);
        let mut expected = vec![Event::Enter, Event::Leave];
        let mut system = [PoolResources {
            graphics: None,
            surface: None,
        }; 3];
        let pools = std::array::from_fn(|i| {
            // A separate owned system reference may alias the pool's reference.
            let resources = input.pools[i][0].resources;
            system[i] = resources;
            *b.textures.get_mut(&resources.graphics.unwrap().0).unwrap() += 1;
            *b.surfaces.get_mut(&resources.surface.unwrap().0).unwrap() += 1;
            expected.push(Event::SurfaceFree(resources.surface.unwrap()));
            expected.push(Event::TextureFree(resources.graphics.unwrap()));
            for slot in input.pools[i].iter().rev() {
                expected.push(Event::SurfaceFree(slot.resources.surface.unwrap()));
                expected.push(Event::TextureFree(slot.resources.graphics.unwrap()));
            }
            let allocation = ManagedPoolAllocation(b.next());
            b.pool_arrays.insert(allocation.0, true);
            expected.push(Event::DeletePool(Some(allocation)));
            ManagedPoolStorage {
                allocation: Some(allocation),
                capacity: 4,
                growth: 0x400,
            }
        });
        r.install_managed_world(input).unwrap();
        r.configure_managed_release(ManagedReleaseIntake {
            system,
            pools,
            pool_memory: [11, 22, 33],
        })
        .unwrap();
        b.events.clear();
        (r, b, key, expected)
    }
    #[test]
    fn release_managed_actual_order_alias_refs_retained_borrows_queue_records_and_repeat() {
        let (mut r, mut b, key, expected) = release_world();
        let graphics = r.texture(key).unwrap().graphics;
        let record = r.texture(key).unwrap().managed_record;
        let queue = *r.texture_manager().queue();
        let counters = r.texture_manager().counters();
        r.release_managed_textures(&mut b).unwrap();
        assert_eq!(b.events, expected);
        assert_eq!(r.texture(key).unwrap().graphics, graphics);
        assert_eq!(r.texture(key).unwrap().managed_record, record);
        assert_eq!(r.texture_manager().queue(), &queue);
        assert_eq!(r.texture_manager().counters(), counters);
        assert_eq!(r.texture(key).unwrap().flags, 0x4000014e);
        assert!(b.textures.is_empty());
        assert_eq!(b.surfaces.len(), 1); // Separate ITexture lock surface.
        assert!(b.pool_arrays.is_empty());
        let state = r.texture_manager().release_state().unwrap();
        assert_eq!(state.pool_memory, [0; 3]);
        assert!(state
            .pools
            .iter()
            .all(|p| p.capacity == 0 && p.allocation.is_none() && p.growth == 0x400));
        b.events.clear();
        r.release_managed_textures(&mut b).unwrap();
        assert_eq!(
            b.events,
            vec![
                Event::Enter,
                Event::Leave,
                Event::DeletePool(None),
                Event::DeletePool(None),
                Event::DeletePool(None)
            ]
        );
        r.close_texture(key, &mut b).unwrap();
        assert!(!b.events.iter().any(|e| matches!(e, Event::TextureFree(_))));
    }
    #[test]
    fn release_managed_preflight_entire_domain_precedes_unload_and_allows_retry() {
        let (mut r, mut b, key, _) = release_world();
        b.release_enabled = false;
        assert!(r.release_managed_textures(&mut b).is_err());
        b.release_enabled = true;
        let last = r.texture_manager().pools()[2][0].resources.surface.unwrap();
        let refs = b.surfaces.remove(&last.0).unwrap();
        assert!(r.release_managed_textures(&mut b).is_err());
        assert!(b.events.is_empty());
        assert_eq!(r.texture(key).unwrap().flags, 0xc000014e);
        b.surfaces.insert(last.0, refs);
        r.release_managed_textures(&mut b).unwrap();
    }
    #[test]
    fn release_managed_errors_retain_exact_completed_prefix_and_never_retry() {
        for failure in 0..5 {
            let (mut r, mut b, key, expected) = release_world();
            b.fail_enter = failure == 0;
            b.fail_leave = failure == 1;
            b.fail_surface_free = failure == 2;
            b.fail_texture_free = failure == 3;
            b.fail_pool = failure == 4;
            assert!(r.release_managed_textures(&mut b).is_err());
            assert!(r.texture_manager().is_poisoned());
            let prefix_len = [1, 2, 3, 4, 9][failure];
            assert_eq!(b.events, expected[..prefix_len]);
            let state = r.texture_manager().release_state().unwrap();
            assert_eq!(state.system[0].surface.is_none(), failure >= 3);
            assert_eq!(state.system[0].graphics.is_none(), failure == 4);
            assert_eq!(state.pool_memory, [11, 22, 33]);
            assert_eq!(r.texture_manager().pools()[0].len(), 2);
            assert_eq!(
                r.texture(key).unwrap().flags & 0x80000000,
                if failure == 0 { 0x80000000 } else { 0 }
            );
            let before = b.events.len();
            assert!(r.release_managed_textures(&mut b).is_err());
            assert_eq!(b.events.len(), before);
        }
    }
    #[test]
    fn release_managed_second_pool_slot_error_keeps_native_pointers_and_completed_first_pool() {
        let (mut r, mut b, _, expected) = release_world();
        let initial = *r.texture_manager().release_state().unwrap();
        let resources = r.texture_manager().pools()[1][1].resources;
        b.fail_release_surface = resources.surface;
        assert!(r.release_managed_textures(&mut b).is_err());
        assert_eq!(b.events, expected[..12]);
        let state = r.texture_manager().release_state().unwrap();
        assert_eq!(
            state.pools[0],
            ManagedPoolStorage {
                allocation: None,
                capacity: 0,
                growth: initial.pools[0].growth
            }
        );
        assert_eq!(state.pool_memory, [0, 22, 33]);
        assert_eq!(state.pools[1..], initial.pools[1..]);
        assert_eq!(
            state.system[1],
            PoolResources {
                graphics: None,
                surface: None
            }
        );
        assert_eq!(state.system[2], initial.system[2]);
        assert!(r.texture_manager().pools()[0].is_empty());
        assert_eq!(r.texture_manager().pools()[1][1].resources, resources);
        assert!(!b.surfaces.contains_key(&resources.surface.unwrap().0));
        assert!(b.textures.contains_key(&resources.graphics.unwrap().0));
        assert_eq!(r.texture_manager().pools()[2].len(), 2);
        assert!(r.texture_manager().is_poisoned());
        let before = b.events.clone();
        assert!(r.release_managed_textures(&mut b).is_err());
        assert_eq!(b.events, before);
    }
    #[test]
    fn release_managed_null_slot_and_invalid_storage_reject_without_effects() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            release_enabled: true,
            ..Backend::default()
        };
        let (_, mut input) = managed_fixture(&mut r, &mut b, false, false);
        input.pools[2][1].resources.surface = None;
        r.install_managed_world(input).unwrap();
        let empty = ManagedPoolStorage {
            allocation: None,
            capacity: 0,
            growth: 19,
        };
        let mut state = ManagedReleaseIntake {
            system: [PoolResources {
                graphics: None,
                surface: None,
            }; 3],
            pools: [empty; 3],
            pool_memory: [1; 3],
        };
        assert!(r.configure_managed_release(state).is_err());
        for p in &mut state.pools {
            let a = ManagedPoolAllocation(b.next());
            b.pool_arrays.insert(a.0, true);
            p.allocation = Some(a);
            p.capacity = 2;
        }
        r.configure_managed_release(state).unwrap();
        b.events.clear();
        assert!(r.release_managed_textures(&mut b).is_err());
        assert!(b.events.is_empty());
        assert!(!r.texture_manager().is_poisoned());
    }
    fn worker_world() -> (TextureResources, Backend, TextureKey) {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            managed_enabled: true,
            release_enabled: true,
            creation_enabled: true,
            request_enabled: true,
            worker_enabled: true,
            worker_checks: 1,
            worker_frame: 5,
            ..Backend::default()
        };
        let (key, mut world) = managed_fixture(&mut r, &mut b, false, false);
        world.queue = [None; 256];
        world.counters = QueueCounters::default();
        world.bindings[0].flags = 0x40;
        world.bindings[0].graphics = None;
        for slot in world.pools.iter_mut().flatten() {
            slot.owner = None;
        }
        let system = std::array::from_fn(|i| world.pools[i][0].resources);
        let pools = std::array::from_fn(|_| {
            let a = ManagedPoolAllocation(b.next());
            b.pool_arrays.insert(a.0, true);
            ManagedPoolStorage {
                allocation: Some(a),
                capacity: 2,
                growth: 2,
            }
        });
        for resource in system {
            *b.textures.get_mut(&resource.graphics.unwrap().0).unwrap() += 1;
            *b.surfaces.get_mut(&resource.surface.unwrap().0).unwrap() += 1;
        }
        r.install_managed_world(world).unwrap();
        r.configure_managed_release(ManagedReleaseIntake {
            system,
            pools,
            pool_memory: [128; 3],
        })
        .unwrap();
        let record = r.texture_manager().active_records()[0];
        r.configure_managed_worker_records(&[(
            record,
            ManagedRecordFields {
                format: 23,
                pool: 0,
                last_frame: 5,
            },
        )])
        .unwrap();
        r.unload_managed_textures(&mut b).unwrap();
        // Existing producer queue starts with nulls, exercising skip-to-empty.
        r.request_texture(key, 5, &mut b).unwrap();
        (r, b, key)
    }
    #[test]
    fn worker_complete_schedule_restore_request_short_read_and_zero_handle() {
        let (mut r, mut b, key) = worker_world();
        r.configure_managed_registry(RegistryIntake {
            capacity: 4096,
            growth: 4096,
            allocation: Some(ManagedRegistryAllocation(b.next())),
        })
        .unwrap();
        r.restore_managed_textures(
            RestoreInput {
                flags: 0,
                formats: CreationFormats { dxt: 0, normal: 23 },
                dimensions: [8; 3],
                budgets: [1; 3],
                max_slots: 8,
            },
            &mut b,
        )
        .unwrap();
        // Restore's descriptor dimensions differ from the original table.
        assert_eq!(r.texture_manager().pools()[0].len(), 1);
        b.worker_checks = 2;
        b.worker_events.clear();
        let refs = r.texture(key).unwrap().references;
        r.run_managed_texture_worker(
            WorkerInput {
                max_running_checks: 3,
                max_slots: 8,
                dimensions: [8; 3],
            },
            &mut b,
        )
        .unwrap();
        assert_eq!(r.texture(key).unwrap().references, refs);
        assert_eq!(r.texture(key).unwrap().width, Observed::Value(9));
        assert_eq!(r.texture(key).unwrap().height, Observed::Value(11));
        assert_eq!(r.texture(key).unwrap().flags & 0xc0000000, 0x80000000);
        assert_eq!(&b.worker_data[..4], &[1, 2, 3, 0xa5]);
        assert!(r.texture_manager().worker_progress().unwrap().complete);
        assert!(b
            .worker_events
            .contains(&ManagedStage::ReleaseWorkerReference));
        assert!(!b.section_held);
        // Every producer overwrite, including wrap to a consumed position,
        // reactivates the weak queue owner without changing historical pointers.
        for _ in 0..256 {
            r.unload_managed_textures(&mut b).unwrap();
            r.request_texture(key, 5, &mut b).unwrap();
            assert_eq!(r.texture_manager().unconsumed_queue_owners().count(), 1);
            b.worker_checks = 1;
            r.run_managed_texture_worker(
                WorkerInput {
                    max_running_checks: 2,
                    max_slots: 8,
                    dimensions: [8; 3],
                },
                &mut b,
            )
            .unwrap();
            assert_eq!(r.texture_manager().unconsumed_queue_owners().count(), 0);
        }
    }
    #[test]
    fn worker_false_entry_has_only_running_effect_and_failures_keep_prefix() {
        let (mut r, mut b, _) = worker_world();
        b.worker_checks = 0;
        b.worker_events.clear();
        r.run_managed_texture_worker(
            WorkerInput {
                max_running_checks: 1,
                max_slots: 8,
                dimensions: [8; 3],
            },
            &mut b,
        )
        .unwrap();
        assert_eq!(b.worker_events, [ManagedStage::Running]);
        for stage in [
            ManagedStage::LockSurface,
            ManagedStage::ReadFile,
            ManagedStage::ReleaseWorkerReference,
        ] {
            let (mut r, mut b, key) = worker_world();
            // Replace arbitrary intake descriptor bytes with real Restore output.
            r.configure_managed_registry(RegistryIntake {
                capacity: 4096,
                growth: 4096,
                allocation: Some(ManagedRegistryAllocation(b.next())),
            })
            .unwrap();
            r.restore_managed_textures(
                RestoreInput {
                    flags: 0,
                    formats: CreationFormats { dxt: 0, normal: 23 },
                    dimensions: [8; 3],
                    budgets: [1; 3],
                    max_slots: 8,
                },
                &mut b,
            )
            .unwrap();
            b.worker_checks = 2;
            b.worker_fail = Some(stage);
            let refs = r.texture(key).unwrap().references;
            assert!(r
                .run_managed_texture_worker(
                    WorkerInput {
                        max_running_checks: 3,
                        max_slots: 8,
                        dimensions: [8; 3]
                    },
                    &mut b
                )
                .is_err());
            assert!(r.is_poisoned());
            assert!(!b.section_held);
            assert_eq!(r.texture(key).unwrap().references, refs + 1);
            assert_eq!(
                r.texture_manager()
                    .worker_progress()
                    .unwrap()
                    .temporary_reference,
                Some(key)
            );
            let before = b.worker_events.clone();
            assert!(r
                .run_managed_texture_worker(
                    WorkerInput {
                        max_running_checks: 3,
                        max_slots: 8,
                        dimensions: [8; 3]
                    },
                    &mut b
                )
                .is_err());
            assert_eq!(before, b.worker_events);
        }
    }
    #[test]
    fn worker_later_free_frame_threshold_growth_then_release_and_close() {
        for frame in [0, 1, 5] {
            let (mut r, mut b, first) = worker_world();
            r.configure_managed_registry(RegistryIntake {
                capacity: 4096,
                growth: 4096,
                allocation: Some(ManagedRegistryAllocation(b.next())),
            })
            .unwrap();
            r.restore_managed_textures(
                RestoreInput {
                    flags: 0,
                    formats: CreationFormats { dxt: 0, normal: 23 },
                    dimensions: [8; 3],
                    budgets: [65, 1, 1],
                    max_slots: 8,
                },
                &mut b,
            )
            .unwrap();
            b.worker_frame = frame;
            r.request_texture(first, frame, &mut b).unwrap();
            b.worker_checks = 1;
            let input = WorkerInput {
                max_running_checks: 2,
                max_slots: 8,
                dimensions: [8; 3],
            };
            r.run_managed_texture_worker(input, &mut b).unwrap();
            b.arrays.insert(
                r.texture_manager()
                    .registry()
                    .unwrap()
                    .allocation
                    .unwrap()
                    .0,
                vec![None; 4096],
            );
            let second = r.create_texture().unwrap();
            r.prepare_managed_texture(second, 0x40).unwrap();
            add(&mut r, &mut b, second, b"second0.bmp", 0).unwrap();
            r.request_texture(second, frame, &mut b).unwrap();
            b.worker_checks = 1;
            r.run_managed_texture_worker(input, &mut b).unwrap();
            assert_eq!(
                r.texture_manager().pools()[0].len(),
                if frame == 1 { 3 } else { 2 }
            );
            if frame == 1 {
                assert_eq!(b.worker_creates.last().unwrap().width, 8);
                assert_eq!(r.texture_manager().pools()[0][1].owner, None);
            }
            r.release_managed_textures(&mut b).unwrap();
            r.release_texture(first, &mut b).unwrap();
            r.release_texture(second, &mut b).unwrap();
            assert!(!r.is_poisoned());
            // Consumed native pointers still exist after object destruction.
            // A later stopped worker must not dereference these weak histories.
            b.worker_checks = 0;
            r.run_managed_texture_worker(input, &mut b).unwrap();
        }
    }
    #[test]
    fn worker_stale_skip_consumes_without_sleep_and_keeps_pointer() {
        let (mut r, mut b, key) = worker_world();
        r.request_texture(key, 0, &mut b).unwrap();
        b.worker_checks = 1;
        b.worker_events.clear();
        let out = r.texture_manager().counters().out;
        r.run_managed_texture_worker(
            WorkerInput {
                max_running_checks: 2,
                max_slots: 8,
                dimensions: [8; 3],
            },
            &mut b,
        )
        .unwrap();
        assert_eq!(r.texture_manager().counters().len, 0);
        assert_eq!(r.texture_manager().queue()[out as usize], Some(key));
        assert_eq!(r.texture(key).unwrap().flags & 0x40000000, 0);
        assert!(!b.worker_events.contains(&ManagedStage::Sleep));
    }

    #[allow(clippy::too_many_arguments)]
    fn presaved_texture(
        r: &mut TextureResources,
        b: &mut Backend,
        width: u16,
        height: u16,
        levels: u16,
        flags: u32,
        format: u32,
        graphics: Option<TextureGraphics>,
    ) -> TextureKey {
        let k = r.create_texture().unwrap();
        // Backend-side live count for the record-level graphics handle
        // (install_texture owns the record; the backend owns the count).
        if let Some(graphics) = graphics {
            b.textures.insert(graphics.0, 1);
        }
        r.install_texture(
            k,
            TextureIntake {
                uid: 0x1234,
                graphics,
                lock_surface: None,
                pixels: None,
                initialized: false,
                width,
                height,
                format,
                flags,
                creation_flags: flags,
                color_key: 0,
                levels,
                memory_size: 0x1234_5678,
                file_name: Some(b"maps\\probe.tga"),
                name_offset: None,
                alpha_file_name: AlphaName::Null,
                alpha_name_offset: None,
            },
            b,
        )
        .unwrap();
        k
    }
    fn presaved_file_id(b: &Backend) -> u64 {
        assert_eq!(b.presaved_open.len(), 1);
        *b.presaved_open.keys().next().unwrap()
    }
    fn dxt_file(levels_payload: &[u8], width: u16, height: u16, dxt3: bool) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"DXT\0");
        v.extend_from_slice(&PRESAVED_DXT_VERSION.to_le_bytes());
        v.extend_from_slice(&(width as i32).to_le_bytes());
        v.extend_from_slice(&(height as i32).to_le_bytes());
        v.extend_from_slice(&[0x11u8; 8]); // dead FILETIME (nonzero proves no compare)
        if dxt3 {
            v.extend_from_slice(&[0x22u8; 8]); // dead alpha FILETIME
        }
        v.extend_from_slice(levels_payload);
        v
    }
    fn tex_file(levels_payload: &[u8], width: u16, height: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"TEX\0");
        v.extend_from_slice(&PRESAVED_TEX_VERSION.to_le_bytes());
        v.extend_from_slice(&(width as i32).to_le_bytes());
        v.extend_from_slice(&(height as i32).to_le_bytes());
        v.extend_from_slice(&[0x33u8; 8]); // dead FILETIME
        v.extend_from_slice(levels_payload);
        v
    }
    #[test]
    fn presaved_dxt_hit_records_exact_bytes_and_memsize() {
        // RVA 0x78190: open->magic->version->dims->dead time->levels->close.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let graphics = TextureGraphics(b.next());
        let key = presaved_texture(&mut r, &mut b, 4, 4, 2, 0, D3DFMT_R5G6B5, Some(graphics));
        b.presaved_expected_graphics = Some(graphics);
        b.presaved_level_sizes = vec![8, 8];
        let payload: Vec<u8> = (0u8..16).collect();
        let name = presaved_dxt_filename(b"maps\\w_1.tga", None, 2, false, PRESAVED_DXT1).unwrap();
        b.presaved_blobs
            .insert(name.clone(), dxt_file(&payload, 4, 4, false));
        assert!(r
            .try_presaved_dxt(key, b"maps\\w_1.tga", None, PRESAVED_DXT1, &mut b)
            .unwrap());
        assert_eq!(r.texture(key).unwrap().memory_size, 16);
        assert_eq!(
            b.presaved_writes,
            vec![(0, payload[..8].to_vec()), (1, payload[8..].to_vec())]
        );
        // Reads: magic4, version2, dims8, dead time8, then one exact read
        // per level (levels=2 here).
        assert_eq!(b.presaved_read_lens, vec![4, 2, 8, 8, 8, 8]);
        let id = presaved_file_id(&b);
        assert_eq!(
            (
                b.presaved_open[&id].pos,
                b.presaved_open[&id].closed,
                b.presaved_open[&id].closes
            ),
            (4 + 2 + 8 + 8 + 8 + 8, true, 1)
        );
        assert_eq!(b.presaved_opens, vec![name]);
        assert!(!r.is_poisoned());
        r.release_texture(key, &mut b).unwrap();
    }
    #[test]
    fn presaved_dxt3_dead_alpha_time_does_not_affect_outcome() {
        // The alpha FILETIME is consumed and discarded: two different values
        // give the same hit (native never compares them).
        for times in [[0xAAu8; 8], [0x00u8; 8]] {
            let mut r = TextureResources::new().unwrap();
            let mut b = Backend {
                presaved_enabled: true,
                ..Backend::default()
            };
            let graphics = TextureGraphics(b.next());
            let key = presaved_texture(&mut r, &mut b, 4, 4, 1, 0, D3DFMT_R5G6B5, Some(graphics));
            b.presaved_expected_graphics = Some(graphics);
            b.presaved_level_sizes = vec![16];
            let mut file = dxt_file(&[0x5Au8; 16], 4, 4, true);
            file[14..22].copy_from_slice(&times);
            let name = presaved_dxt_filename(
                b"maps\\w_a.tga",
                Some(b"maps\\w_b.tga"),
                1,
                false,
                PRESAVED_DXT3,
            )
            .unwrap();
            b.presaved_blobs.insert(name, file);
            assert!(r
                .try_presaved_dxt(
                    key,
                    b"maps\\w_a.tga",
                    Some(b"maps\\w_b.tga"),
                    PRESAVED_DXT3,
                    &mut b
                )
                .unwrap());
            assert_eq!(b.presaved_read_lens, vec![4, 2, 8, 8, 8, 16]);
            assert_eq!(r.texture(key).unwrap().memory_size, 16);
            r.release_texture(key, &mut b).unwrap();
        }
    }
    #[test]
    fn presaved_555_hit_and_miss_paths_close_stream() {
        // Hit.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let graphics = TextureGraphics(b.next());
        let key = presaved_texture(&mut r, &mut b, 4, 2, 1, 0, D3DFMT_R5G6B5, Some(graphics));
        b.presaved_expected_graphics = Some(graphics);
        b.presaved_level_sizes = vec![16];
        let payload = vec![0x77u8; 16];
        let name = presaved_555_filename(b"maps\\w_9.tga", 1, false, D3DFMT_R5G6B5).unwrap();
        b.presaved_blobs
            .insert(name.clone(), tex_file(&payload, 4, 2));
        assert!(r
            .try_presaved_555_565(key, b"maps\\w_9.tga", &mut b)
            .unwrap());
        assert_eq!(b.presaved_writes, vec![(0, payload.clone())]);
        // Effect order mirrors the DXT path with the single dead read.
        assert_eq!(b.presaved_read_lens, vec![4, 2, 8, 8, 16]);
        assert_eq!(r.texture(key).unwrap().memory_size, 16);
        // Misses: bad magic / bad version / wrong dims. Each closes the
        // stream, writes nothing, keeps memsize, stays unpoisoned.
        let miss_cases: Vec<(Vec<u8>, Vec<u8>)> = vec![
            {
                let mut f = tex_file(&payload, 4, 2);
                f[0] = b'X';
                (f, b"maps\\w_b.tga".to_vec())
            },
            {
                let mut f = tex_file(&payload, 4, 2);
                f[4] ^= 0xff;
                (f, b"maps\\w_c.tga".to_vec())
            },
            (tex_file(&payload, 5, 2), b"maps\\w_d.tga".to_vec()),
        ];
        for (file, probe_name) in miss_cases {
            let mut r2 = TextureResources::new().unwrap();
            let mut b2 = Backend {
                presaved_enabled: true,
                ..Backend::default()
            };
            let g2 = TextureGraphics(b2.next());
            let k2 = presaved_texture(&mut r2, &mut b2, 4, 2, 1, 0, D3DFMT_R5G6B5, Some(g2));
            b2.presaved_expected_graphics = Some(g2);
            b2.presaved_level_sizes = vec![16];
            let probe = presaved_555_filename(&probe_name, 1, false, D3DFMT_R5G6B5).unwrap();
            b2.presaved_blobs.insert(probe, file);
            assert_eq!(r2.texture(k2).unwrap().memory_size, 0x1234_5678);
            assert!(!r2.try_presaved_555_565(k2, &probe_name, &mut b2).unwrap());
            assert!(b2.presaved_writes.is_empty());
            assert_eq!(r2.texture(k2).unwrap().memory_size, 0x1234_5678);
            let id = presaved_file_id(&b2);
            assert_eq!(
                (b2.presaved_open[&id].closed, b2.presaved_open[&id].closes),
                (true, 1)
            );
            assert!(!r2.is_poisoned());
            r2.release_texture(k2, &mut b2).unwrap();
        }
        // Missing file: open None, no close, false.
        let mut r3 = TextureResources::new().unwrap();
        let mut b3 = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let g3 = TextureGraphics(b3.next());
        let k3 = presaved_texture(&mut r3, &mut b3, 4, 2, 1, 0, D3DFMT_R5G6B5, Some(g3));
        assert!(!r3
            .try_presaved_555_565(k3, b"maps\\nope.tga", &mut b3)
            .unwrap());
        assert!(b3.presaved_open.is_empty() && b3.presaved_read_lens.is_empty());
        assert!(!r3.is_poisoned());
        r3.release_texture(k3, &mut b3).unwrap();
        r.release_texture(key, &mut b).unwrap();
    }
    #[test]
    fn presaved_zero_levels_skips_graphics_but_still_gates_header() {
        // Zero levels never touch the device handle (deferred graphics
        // check): header gates still run, memsize resets to 0, true.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let key = presaved_texture(&mut r, &mut b, 8, 8, 0, 0, D3DFMT_R5G6B5, None);
        let name = presaved_dxt_filename(b"maps\\w_0.tga", None, 0, false, PRESAVED_DXT1).unwrap();
        b.presaved_blobs.insert(name, dxt_file(&[], 8, 8, false));
        assert!(r
            .try_presaved_dxt(key, b"maps\\w_0.tga", None, PRESAVED_DXT1, &mut b)
            .unwrap());
        assert_eq!(r.texture(key).unwrap().memory_size, 0);
        assert_eq!(b.presaved_read_lens, vec![4, 2, 8, 8]);
        r.release_texture(key, &mut b).unwrap();
    }
    #[test]
    fn presaved_missing_graphics_closes_then_rejects_after_header() {
        // Header gates pass (dead reads consumed), then the missing device
        // handle closes the stream and reports InvalidState with poison.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let key = presaved_texture(&mut r, &mut b, 4, 4, 1, 0, D3DFMT_R5G6B5, None);
        let name = presaved_dxt_filename(b"maps\\w_g.tga", None, 1, false, PRESAVED_DXT1).unwrap();
        b.presaved_blobs
            .insert(name, dxt_file(&[0u8; 8], 4, 4, false));
        assert_eq!(
            r.try_presaved_dxt(key, b"maps\\w_g.tga", None, PRESAVED_DXT1, &mut b),
            Err(TextureOperationError::State(TextureError::InvalidState))
        );
        assert_eq!(b.presaved_read_lens, vec![4, 2, 8, 8]);
        let id = presaved_file_id(&b);
        assert!(b.presaved_open[&id].closed);
        assert!(r.is_poisoned());
        // No release: poisoned arenas refuse release via available().
    }
    #[test]
    fn presaved_short_read_is_backend_error_and_poisons() {
        // Exact-size reads: a 1B-truncated level payload is a backend error.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let graphics = TextureGraphics(b.next());
        let key = presaved_texture(&mut r, &mut b, 4, 4, 1, 0, D3DFMT_R5G6B5, Some(graphics));
        b.presaved_expected_graphics = Some(graphics);
        b.presaved_level_sizes = vec![8];
        let name = presaved_dxt_filename(b"maps\\w_s.tga", None, 1, false, PRESAVED_DXT1).unwrap();
        let mut file = dxt_file(&[0u8; 8], 4, 4, false);
        file.truncate(file.len() - 1); // exact-size read now short
        b.presaved_blobs.insert(name, file);
        assert_eq!(
            r.try_presaved_dxt(key, b"maps\\w_s.tga", None, PRESAVED_DXT1, &mut b),
            Err(TextureOperationError::Backend("short presaved read"))
        );
        assert!(r.is_poisoned());
        // Same close discipline as the oversize path: the stream closes
        // before the backend error propagates.
        let id = presaved_file_id(&b);
        assert!((b.presaved_open[&id].closed, b.presaved_open[&id].closes) == (true, 1));
        // No release: poisoned arenas refuse release via available(), so the
        // error-path tests assert poison instead of residue discipline.
    }
    #[test]
    fn presaved_level_oversize_closes_and_reports_backend_error() {
        // A level size far beyond the payload exercises the same
        // close-before-error order as the OOM path (which needs a >RAM
        // reservation and is not deterministically testable): exact-size
        // read goes short -> Backend error + closed stream + poison.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let graphics = TextureGraphics(b.next());
        let key = presaved_texture(&mut r, &mut b, 4, 4, 1, 0, D3DFMT_R5G6B5, Some(graphics));
        b.presaved_expected_graphics = Some(graphics);
        b.presaved_level_sizes = vec![1 << 20];
        let name = presaved_dxt_filename(b"maps\\w_o.tga", None, 1, false, PRESAVED_DXT1).unwrap();
        b.presaved_blobs
            .insert(name, dxt_file(&[0u8; 8], 4, 4, false));
        assert_eq!(
            r.try_presaved_dxt(key, b"maps\\w_o.tga", None, PRESAVED_DXT1, &mut b),
            Err(TextureOperationError::Backend("short presaved read"))
        );
        assert!(r.is_poisoned());
        let id = presaved_file_id(&b);
        assert!(b.presaved_open[&id].closed);
        // No release, and release-after-poison is refused: poisoned arenas
        // gate release via available().
        assert_eq!(
            r.release_texture(key, &mut b),
            Err(TextureOperationError::State(TextureError::Poisoned))
        );
    }
    #[test]
    fn presaved_effects_absent_rejects_as_host_allocation() {
        // No presaved capability: clean HostAllocation before any effect.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default(); // presaved_enabled == false
        let graphics = TextureGraphics(b.next());
        let key = presaved_texture(&mut r, &mut b, 4, 4, 1, 0, D3DFMT_R5G6B5, Some(graphics));
        assert_eq!(
            r.try_presaved_dxt(key, b"maps\\w_x.tga", None, PRESAVED_DXT1, &mut b),
            Err(TextureOperationError::State(TextureError::HostAllocation))
        );
        assert!(b.presaved_opens.is_empty());
        assert!(r.is_poisoned());
        // No release: poisoned arenas refuse release via available().
    }
    /// Independent level-size oracle for presaved payloads (same native
    /// `desc.Size` reference as the port, separate transcription): 16-bit
    /// levels use a DWORD-aligned D3D pitch (single-entry finding on
    /// maps/02_9.565: the w=1 tail levels are 8B+4B, not tight 4B+2B —
    /// assumed across 16-bit presaveds); DXT levels are block-rounded: type 2
    /// (DXT1-alpha) shares the DXT1 8B-block arm (mafia-re ITexture.cpp:84-87
    /// type digits, :1981-1994 shared CompressTexture arm), only DXT3 packs
    /// 16B blocks. Widths halve per level with a 1 minimum.
    fn presaved_level_sizes_oracle(
        dxt: u8,
        mut w: u32,
        mut h: u32,
        levels: u16,
    ) -> Option<Vec<u32>> {
        let mut out = Vec::new();
        for _ in 0..levels {
            let size: u64 = if dxt == 0 {
                // D3D desc.Size uses a DWORD-aligned pitch (proven by real
                // maps/02_9.565: the w=1 tail levels are 8B+4B, not 4B+2B).
                let pitch = (u64::from(w) * 2).div_ceil(4) * 4;
                pitch * u64::from(h)
            } else {
                let blocks = u64::from(w.div_ceil(4)) * u64::from(h.div_ceil(4));
                // 16B blocks on == DXT3 only: type 2 shares the DXT1 8B arm.
                blocks * if dxt == PRESAVED_DXT3 { 16 } else { 8 }
            };
            out.push(u32::try_from(size).ok()?);
            w = (w >> 1).max(1);
            h = (h >> 1).max(1);
        }
        Some(out)
    }

    #[test]
    fn real_dta_presaved_payload_loop_matches_game_bytes() {
        // RVAs 0x78190/0x78430: filename probe + payload loop (magic, version,
        // dims, dead FILETIME reads, per-level exact bytes, wrapping memsize)
        // executed on REAL presaved entries from the game DTAs. Read-only
        // harness: env-absent loud-SKIP + early return, env-set-but-empty
        // panic, per-entry residue asserts, unpoisoned on clean paths.
        let root = match std::env::var_os("LHR_ORIGINAL_INSTALLATION") {
            Some(root) => std::path::PathBuf::from(root),
            None => {
                eprintln!(
                    "SKIP: LHR_ORIGINAL_INSTALLATION is not set; needs the original game data"
                );
                return;
            }
        };
        let installation = lhr_assets::Installation::open(&root)
            .expect("LHR_ORIGINAL_INSTALLATION must open as a game installation");
        let paths = match installation.asset_paths("maps") {
            Ok(paths) => paths,
            Err(error) => {
                eprintln!("SKIP real-DTA presaved tests: no maps catalog: {error:?}");
                return;
            }
        };
        let lower = |p: &String| p.to_ascii_lowercase();
        // Version-gated pick: the native loop accepts DXT version 9 and TEX
        // version 1 only (REA version CMPs); the archives also carry older
        // DXT generations (v2/3/4/6/7/8, e.g. maps/w_1.dx1 is v3) which the
        // original itself refuses. Candidates are version-checked by header.
        let version_ok = |path: &str| -> bool {
            let Ok(bytes) = installation.read(path) else {
                return false;
            };
            if bytes.len() < 6 {
                return false;
            }
            let ver = u16::from_le_bytes([bytes[4], bytes[5]]);
            if path.to_ascii_lowercase().contains(".dx") {
                ver == PRESAVED_DXT_VERSION
            } else {
                ver == PRESAVED_TEX_VERSION
            }
        };
        let pick = |exts: &[&str], alpha: bool, preferred: &[&str]| -> Option<String> {
            let mut candidates: Vec<String> = paths
                .iter()
                .filter(|p| exts.iter().any(|e| lower(p).ends_with(e)))
                .cloned()
                .collect();
            candidates.sort();
            for want in preferred {
                if let Some(hit) = candidates.iter().find(|p| lower(p).ends_with(want)) {
                    if version_ok(hit) {
                        return Some(hit.clone());
                    }
                }
            }
            // DXT3-with-alpha-infix wants a stem with 2+ underscores.
            candidates.into_iter().find(|p| {
                if alpha && p.rsplit('/').next().unwrap_or(p).matches('_').count() < 2 {
                    return false;
                }
                version_ok(p)
            })
        };
        let dx1 = pick(&[".dx1", ".dx2"], false, &["maps/w_1.dx1"]);
        let tex = pick(&[".555", ".565"], false, &["maps/02_9.565"]);
        let dx3 = pick(&[".dx3"], true, &["maps/ug_kole2_6.dx3"]);
        if dx1.is_none() || tex.is_none() || dx3.is_none() {
            panic!(
                "LHR_ORIGINAL_INSTALLATION is set but presaved kinds are missing under maps/ (dx1={dx1:?}, tex={tex:?}, dx3={dx3:?})"
            );
        }
        for (path, with_alpha) in [
            (dx1.unwrap(), false),
            (tex.unwrap(), false),
            (dx3.unwrap(), true),
        ] {
            // Logged so passing runs record which archive entries (and which
            // DXT generation, re-parsed from the entry extension at drive
            // time) actually executed.
            eprintln!("driven presaved entry: {path}");
            drive_real_presaved_entry(&installation, &path, with_alpha);
        }
    }

    fn drive_real_presaved_entry(
        installation: &lhr_assets::Installation,
        path: &str,
        with_alpha: bool,
    ) {
        let bytes = installation.read(path).unwrap();
        let file_name = path.rsplit('/').next().unwrap_or(path);
        // --- Independent header parse (widths from the instruction evidence:
        // 4B magic, u16le version, two i32le dims, 8B dead times). ---
        assert!(bytes.len() >= 22, "presaved entry too short: {path}");
        let (magic, ver, sx, sy) = (
            &bytes[0..4],
            u16::from_le_bytes([bytes[4], bytes[5]]),
            i32::from_le_bytes(bytes[6..10].try_into().unwrap()),
            i32::from_le_bytes(bytes[10..14].try_into().unwrap()),
        );
        let is_dxt_entry = file_name.to_ascii_lowercase().contains(".dx");
        let dxt: u8 = if is_dxt_entry {
            assert_eq!(&magic[..3], b"DXT", "bad presaved magic: {path}");
            assert_eq!(ver, PRESAVED_DXT_VERSION, "bad presaved version: {path}");
            let ext = file_name.rsplit('.').next().unwrap_or("");
            ext.to_ascii_uppercase()
                .strip_prefix("DX")
                .and_then(|n| n.parse().ok())
                .unwrap_or(1)
        } else {
            assert_eq!(&magic[..3], b"TEX", "bad presaved magic: {path}");
            assert_eq!(ver, PRESAVED_TEX_VERSION, "bad presaved version: {path}");
            0
        };
        let header_len = 22 + if dxt == PRESAVED_DXT3 { 8 } else { 0 };
        assert!(
            bytes.len() >= header_len,
            "presaved entry truncated in header: {path}"
        );
        // --- Filename tail: `<head>_<levels>[L].<ext>; DXT3 head may carry
        // the alpha infix (`<name>_<alpha>`). ---
        let stem = file_name
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(file_name);
        let (head, tail) = stem.rsplit_once('_').unwrap_or((stem, ""));
        let half = tail.ends_with(['L', 'l']);
        let levels: u16 = tail
            .trim_end_matches(['L', 'l'])
            .parse()
            .unwrap_or(u16::MAX);
        assert!(
            levels != u16::MAX && levels <= 16,
            "unparsable presaved levels: {path}"
        );
        // --- Level split: the ONLY split consistent with the oracle must
        // exactly consume the file AND match the filename level count. ---
        let (w0, h0) = (u32::try_from(sx).unwrap(), u32::try_from(sy).unwrap());
        let sizes = presaved_level_sizes_oracle(dxt, w0, h0, levels).unwrap();
        let total: u64 = header_len as u64 + sizes.iter().map(|s| *s as u64).sum::<u64>();
        assert_eq!(
            total as usize,
            bytes.len(),
            "presaved payload does not split under the Size model: {path}"
        );
        // --- Probe-name check against the REAL entry name (basename,
        // case-insensitive): exercises the alpha-infix branch for DXT3. ---
        let (src, alpha) = if with_alpha {
            let (n, a) = head.split_once('_').unwrap_or((head, ""));
            (
                format!("{n}.tga").into_bytes(),
                Some(format!("{a}.tga").into_bytes()),
            )
        } else {
            (format!("{head}.tga").into_bytes(), None)
        };
        let format = if dxt == 0 {
            if file_name.to_ascii_lowercase().ends_with(".565") {
                D3DFMT_R5G6B5
            } else {
                D3DFMT_X1R5G5B5
            }
        } else {
            D3DFMT_R5G6B5 // DXT path never reads format; any evidence word will do
        };
        let probe = if dxt == 0 {
            presaved_555_filename(&src, levels, half, format).unwrap()
        } else {
            presaved_dxt_filename(&src, alpha.as_deref(), levels, half, dxt).unwrap()
        };
        assert_eq!(
            probe.to_ascii_lowercase(),
            file_name.as_bytes().to_ascii_lowercase(),
            "probe filename diverges from the real entry: {path}"
        );
        // --- Drive the loop with a synthetic-graphics backend. ---
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let graphics = TextureGraphics(b.next());
        let key = presaved_texture(
            &mut r,
            &mut b,
            u16::try_from(sx).unwrap(),
            u16::try_from(sy).unwrap(),
            levels,
            if half { ITC_HALF } else { 0 },
            format,
            Some(graphics),
        );
        b.presaved_expected_graphics = Some(graphics);
        b.presaved_level_sizes = sizes.clone();
        b.presaved_blobs.insert(probe.clone(), bytes.clone());
        let hit = if dxt == 0 {
            r.try_presaved_555_565(key, &src, &mut b).unwrap()
        } else {
            r.try_presaved_dxt(key, &src, alpha.as_deref(), dxt, &mut b)
                .unwrap()
        };
        assert!(hit, "real presaved entry missed: {path}");
        let expected_mem: u32 = sizes.iter().fold(0u32, |a, s| a.wrapping_add(*s));
        assert_eq!(r.texture(key).unwrap().memory_size, expected_mem);
        // Per-level exact bytes + dead-read position math.
        let mut pos = header_len;
        let mut expected_reads = vec![4usize, 2, 8, 8];
        if dxt == PRESAVED_DXT3 {
            expected_reads.insert(4, 8);
        }
        for (level, size) in sizes.iter().enumerate() {
            assert_eq!(
                b.presaved_writes[level],
                (level as u16, bytes[pos..pos + *size as usize].to_vec()),
                "level {level} bytes diverge: {path}"
            );
            pos += *size as usize;
            expected_reads.push(*size as usize);
        }
        assert_eq!(
            b.presaved_read_lens, expected_reads,
            "effect order diverged: {path}"
        );
        assert_eq!(pos, bytes.len());
        let id = presaved_file_id(&b);
        assert!((b.presaved_open[&id].closed, b.presaved_open[&id].closes) == (true, 1));
        assert!(!r.is_poisoned());
        // --- Miss paths on clones of the REAL bytes. ---
        let mutators: Vec<fn(&mut Vec<u8>)> = vec![
            |f: &mut Vec<u8>| f[0] ^= 0xff,
            |f: &mut Vec<u8>| f[4] ^= 0xff,
            |f: &mut Vec<u8>| f[6] ^= 0xff,
        ];
        for mutate in mutators {
            let mut bad = bytes.clone();
            mutate(&mut bad);
            let mut r2 = TextureResources::new().unwrap();
            let mut b2 = Backend {
                presaved_enabled: true,
                ..Backend::default()
            };
            let g2 = TextureGraphics(b2.next());
            let k2 = presaved_texture(
                &mut r2,
                &mut b2,
                u16::try_from(sx).unwrap(),
                u16::try_from(sy).unwrap(),
                levels,
                if half { ITC_HALF } else { 0 },
                format,
                Some(g2),
            );
            b2.presaved_expected_graphics = Some(g2);
            b2.presaved_level_sizes = sizes.clone();
            b2.presaved_blobs.insert(probe.clone(), bad);
            let miss = if dxt == 0 {
                r2.try_presaved_555_565(k2, &src, &mut b2).unwrap()
            } else {
                r2.try_presaved_dxt(k2, &src, alpha.as_deref(), dxt, &mut b2)
                    .unwrap()
            };
            assert!(!miss, "corrupt real entry accepted: {path}");
            assert!(b2.presaved_writes.is_empty());
            let id2 = presaved_file_id(&b2);
            assert!(
                (b2.presaved_open[&id2].closed, b2.presaved_open[&id2].closes) == (true, 1),
                "miss leaked the stream: {path}"
            );
            assert!(!r2.is_poisoned());
            r2.release_texture(k2, &mut b2).unwrap();
        }
        // Missing file -> false with no close.
        let mut r3 = TextureResources::new().unwrap();
        let mut b3 = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let g3 = TextureGraphics(b3.next());
        let k3 = presaved_texture(
            &mut r3,
            &mut b3,
            u16::try_from(sx).unwrap(),
            u16::try_from(sy).unwrap(),
            levels,
            0,
            format,
            Some(g3),
        );
        let miss = if dxt == 0 {
            r3.try_presaved_555_565(k3, b"maps\\no-such-file.tga", &mut b3)
                .unwrap()
        } else {
            r3.try_presaved_dxt(k3, b"maps\\no-such-file.tga", None, dxt, &mut b3)
                .unwrap()
        };
        assert!(!miss);
        assert!(b3.presaved_open.is_empty());
        r3.release_texture(k3, &mut b3).unwrap();
        r.release_texture(key, &mut b).unwrap();
    }
    #[test]
    fn real_dta_stale_presaved_generation_misses_and_closes() {
        // Genuine stale archive entries (e.g. maps/w_1.dx1, DXT version 3)
        // take the version-miss path with close+false — the same refusal the
        // original itself performs (REA version CMP). This executes refusal
        // on real stale bytes, not just synthetic mutations.
        let root = match std::env::var_os("LHR_ORIGINAL_INSTALLATION") {
            Some(root) => std::path::PathBuf::from(root),
            None => {
                eprintln!(
                    "SKIP: LHR_ORIGINAL_INSTALLATION is not set; needs the original game data"
                );
                return;
            }
        };
        let installation = lhr_assets::Installation::open(&root)
            .expect("LHR_ORIGINAL_INSTALLATION must open as a game installation");
        let bytes = installation
            .read("maps/w_1.dx1")
            .expect("stale entry present");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 3);
        let sx = i32::from_le_bytes(bytes[6..10].try_into().unwrap());
        let sy = i32::from_le_bytes(bytes[10..14].try_into().unwrap());
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend {
            presaved_enabled: true,
            ..Backend::default()
        };
        let graphics = TextureGraphics(b.next());
        let key = presaved_texture(
            &mut r,
            &mut b,
            u16::try_from(sx).unwrap(),
            u16::try_from(sy).unwrap(),
            1,
            0,
            D3DFMT_R5G6B5,
            Some(graphics),
        );
        b.presaved_expected_graphics = Some(graphics);
        b.presaved_level_sizes = vec![8];
        let name = presaved_dxt_filename(b"maps\\w_1.tga", None, 1, false, PRESAVED_DXT1).unwrap();
        b.presaved_blobs.insert(name, bytes);
        assert!(!r
            .try_presaved_dxt(key, b"maps\\w_1.tga", None, PRESAVED_DXT1, &mut b)
            .unwrap());
        assert!(b.presaved_writes.is_empty());
        let id = presaved_file_id(&b);
        assert!((b.presaved_open[&id].closed, b.presaved_open[&id].closes) == (true, 1));
        assert!(!r.is_poisoned());
        r.release_texture(key, &mut b).unwrap();
    }
    fn install_live_texture(
        r: &mut TextureResources,
        b: &mut Backend,
        width: u16,
        height: u16,
        format: u32,
        levels: u16,
    ) -> TextureKey {
        let k = r.create_texture().unwrap();
        let graphics = TextureGraphics(b.next());
        b.textures.insert(graphics.0, 1);
        let pixels = PixelAllocation(b.next());
        b.pixels
            .insert(pixels.0, vec![0x5a; width as usize * height as usize * 4]);
        r.install_texture(
            k,
            TextureIntake {
                uid: 7,
                graphics: Some(graphics),
                lock_surface: None,
                pixels: Some(pixels),
                initialized: true,
                width,
                height,
                format,
                flags: 1,
                creation_flags: 1,
                color_key: 0,
                levels,
                memory_size: width as u32 * height as u32 * 4,
                file_name: Some(b"maps\\wall.tga"),
                name_offset: Some(5),
                alpha_file_name: AlphaName::Null,
                alpha_name_offset: None,
            },
            b,
        )
        .unwrap();
        k
    }
    #[test]
    fn reload_stub_logs_once_and_returns_unsupported() {
        let r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        assert_eq!(
            TextureResources::reload_texture(&r, &mut b),
            I3DERR_UNSUPPORTED
        );
        assert_eq!(
            b.diagnostics,
            [b"ITexture::Reload() - WAS NOT IMPLEMENTED !".to_vec()]
        );
        assert!(!r.is_poisoned());
    }
    #[test]
    fn is_init_reads_flag() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = install_live_texture(&mut r, &mut b, 4, 4, 21, 1);
        assert!(r.is_texture_init(k).unwrap());
        r.close_texture(k, &mut b).unwrap();
        // Close preserves the record with initialized=false.
        assert!(!r.is_texture_init(k).unwrap());
    }
    #[test]
    fn render_diagnostic_is_byte_exact() {
        assert_eq!(
            render_diagnostic(&TextureDiagnostic::ReloadNotImplemented),
            b"ITexture::Reload() - WAS NOT IMPLEMENTED !"
        );
        assert_eq!(
            render_diagnostic(&TextureDiagnostic::TgaCompressedPixelFormat(b"wall.tga".to_vec())),
            b"!! ITexture::SaveToTGA() - Cannot create file wall.tga  Compresed pixel format isn't supported".to_vec()
        );
        assert_eq!(
            render_diagnostic(&TextureDiagnostic::TgaCannotCreateFile(
                b"wall.tga".to_vec()
            )),
            b"!! ITexture::SaveToTGA() - Cannot create file wall.tga".to_vec()
        );
        assert_eq!(
            render_diagnostic(&TextureDiagnostic::TgaCannotLockTexture),
            b"!! ITexture::SaveToTGA() - Cannot lock texture".to_vec()
        );
    }
    #[test]
    fn duplicate_copies_live_texture() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let src = install_live_texture(&mut r, &mut b, 4, 2, 21, 1);
        // Exact-size pixels for the 4x2x4 clone (native trusts dims).
        let dst = r.create_texture().unwrap();
        r.close_texture(dst, &mut b).unwrap();
        b.events.clear();
        assert_eq!(r.duplicate_texture(dst, Some(src), &mut b).unwrap(), I3D_OK);
        let (s, d) = (r.texture(src).unwrap(), r.texture(dst).unwrap());
        assert_eq!(
            (
                d.flags,
                d.creation_flags,
                d.width,
                d.height,
                d.levels,
                d.memory_size
            ),
            (
                s.flags,
                s.creation_flags,
                s.width,
                s.height,
                s.levels,
                s.memory_size
            )
        );
        assert_eq!(d.color_key, s.color_key);
        assert_eq!(d.name, s.name);
        assert_ne!(d.pixels, s.pixels);
        assert_eq!(
            b.pixels[&d.pixels.unwrap().0],
            b.pixels[&s.pixels.unwrap().0]
        );
        assert!(d.initialized);
        assert!(r.uncached().contains(&dst));
        assert_eq!(b.dup_clone_log, [(s.pixels.unwrap(), 32)]);
        assert!(b
            .events
            .iter()
            .any(|e| matches!(e, Event::TexturePreload(_))));
    }
    #[test]
    fn duplicate_null_and_uninit_are_clean_noops() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let dst = r.create_texture().unwrap();
        r.close_texture(dst, &mut b).unwrap();
        b.events.clear();
        assert_eq!(r.duplicate_texture(dst, None, &mut b).unwrap(), I3D_OK);
        assert!(b.dup_clone_log.is_empty());
        let src = install_live_texture(&mut r, &mut b, 2, 2, 21, 1);
        r.textures.texture_mut(src).initialized = false;
        assert_eq!(r.duplicate_texture(dst, Some(src), &mut b).unwrap(), I3D_OK);
        assert!(b.dup_clone_log.is_empty());
    }
    #[test]
    fn duplicate_oom_keeps_effects_and_returns_code() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let src = install_live_texture(&mut r, &mut b, 2, 2, 21, 1);
        let dst = r.create_texture().unwrap();
        r.close_texture(dst, &mut b).unwrap();
        b.dup_fail_clone_oom = true;
        assert_eq!(
            r.duplicate_texture(dst, Some(src), &mut b).unwrap(),
            I3DERR_OUTOFMEM
        );
        // Flags/names copied before the failed clone (effects retained).
        assert_eq!(r.texture(dst).unwrap().flags, r.texture(src).unwrap().flags);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn duplicate_missing_graphics_is_clean() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let src = install_live_texture(&mut r, &mut b, 2, 2, 21, 1);
        let dst = r.create_texture().unwrap();
        r.close_texture(dst, &mut b).unwrap();
        b.dup_fail_graphics_none = true;
        assert!(r.duplicate_texture(dst, Some(src), &mut b).is_err());
    }
    #[test]
    fn save_tga_refuses_all_dxt_with_exact_log() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        for format in [
            D3DFMT_DXT1,
            D3DFMT_DXT2,
            D3DFMT_DXT3,
            D3DFMT_DXT4,
            D3DFMT_DXT5,
        ] {
            let k = install_live_texture(&mut r, &mut b, 2, 2, format, 1);
            b.diagnostics.clear();
            assert!(!r.save_texture_to_tga(k, b"wall.tga", &mut b).unwrap());
            assert_eq!(
                b.diagnostics,
                [b"!! ITexture::SaveToTGA() - Cannot create file wall.tga  Compresed pixel format isn't supported".to_vec()]
            );
            assert!(b.tga_files.is_empty());
        }
    }
    #[test]
    fn save_tga_create_and_lock_failures() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = install_live_texture(&mut r, &mut b, 2, 2, 21, 1);
        b.tga_fail_open = true;
        assert!(!r.save_texture_to_tga(k, b"out.tga", &mut b).unwrap());
        assert_eq!(
            b.diagnostics,
            [b"!! ITexture::SaveToTGA() - Cannot create file out.tga".to_vec()]
        );
        b.tga_fail_open = false;
        b.tga_lock_fail = true;
        assert!(!r.save_texture_to_tga(k, b"out.tga", &mut b).unwrap());
        assert_eq!(
            b.diagnostics[1],
            b"!! ITexture::SaveToTGA() - Cannot lock texture".to_vec()
        );
        // Lock failure keeps the file open (native leak preserved).
        assert!(!b.tga_files.values().next().unwrap().closed);
    }
    #[test]
    fn save_tga_round_trip_decodes_via_tga_decoder() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = install_live_texture(&mut r, &mut b, 2, 2, 21, 1);
        // Bottom-up rows with pitch 8: [3,2,1,17, 6,5,4,34] then [9,8,7,51, 12,11,10,68].
        b.tga_lock = Some((
            vec![3, 2, 1, 17, 6, 5, 4, 34, 9, 8, 7, 51, 12, 11, 10, 68],
            8,
        ));
        assert!(r.save_texture_to_tga(k, b"out.tga", &mut b).unwrap());
        let file = b.tga_files.values().next().unwrap();
        assert!(file.closed);
        assert_eq!(
            &file.bytes[..18],
            &[0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 32, 0]
        );
        // File holds the top row first (bottom-origin TGA): the decoder
        // reverses rows and swaps BGR to RGB, recovering bottom-up RGBA.
        assert_eq!(
            &file.bytes[18..],
            &[9, 8, 7, 51, 12, 11, 10, 68, 3, 2, 1, 17, 6, 5, 4, 34]
        );
        let decoded = lhr_assets::texture_source::decode_texture_image(&file.bytes).unwrap();
        assert_eq!((decoded.texture.width, decoded.texture.height), (2, 2));
        assert_eq!(
            decoded.texture.rgba,
            [1, 2, 3, 17, 4, 5, 6, 34, 7, 8, 9, 51, 10, 11, 12, 68]
        );
        assert_eq!(b.tga_unlocks, [r.texture(k).unwrap().graphics.unwrap().0]);
    }
    #[test]
    fn save_tga_conversions_are_bit_exact() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        // R5G6B5: (0xFFFF>>1)&0x7fe0|(0xFFFF&0x1f) = 0x3FFF; 0 -> 0.
        let k = install_live_texture(&mut r, &mut b, 2, 1, 23, 1);
        b.tga_lock = Some((vec![0xff, 0xff, 0x00, 0x00], 4));
        assert!(r.save_texture_to_tga(k, b"a.tga", &mut b).unwrap());
        assert_eq!(
            &b.tga_files.values().next().unwrap().bytes[18..],
            &[0xff, 0x7f, 0x00, 0x00]
        );
        // A4R4G4B4 0xABCD -> 0xA0B0C0D0.
        let k2 = install_live_texture(&mut r, &mut b, 1, 1, 26, 1);
        b.tga_lock = Some((vec![0xcd, 0xab], 2));
        assert!(r.save_texture_to_tga(k2, b"b.tga", &mut b).unwrap());
        let second = b.tga_files.values().nth(1).unwrap();
        assert_eq!(&second.bytes[18..], &[0xd0, 0xc0, 0xb0, 0xa0]);
        // Zero dimensions write a header-only file and return true.
        let k3 = install_live_texture(&mut r, &mut b, 2, 2, 21, 1);
        r.textures.texture_mut(k3).width = Observed::Value(0);
        r.textures.texture_mut(k3).height = Observed::Value(0);
        b.tga_lock = Some((vec![], 0));
        assert!(r.save_texture_to_tga(k3, b"c.tga", &mut b).unwrap());
        assert_eq!(b.tga_files.values().nth(2).unwrap().bytes.len(), 18);
    }
    #[test]
    fn duplicate_effects_absent_rejects_cleanly() {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let src = install_live_texture(&mut r, &mut b, 2, 2, 21, 1);
        let dst = r.create_texture().unwrap();
        r.close_texture(dst, &mut b).unwrap();
        b.no_dup_effects = true;
        assert!(r.duplicate_texture(dst, Some(src), &mut b).is_err());
        // Close + copies done before the capability gate (effects retained).
        assert_eq!(r.texture(dst).unwrap().flags, r.texture(src).unwrap().flags);
        assert!(r.is_poisoned());
    }

    struct FakeSurface {
        calls: Vec<String>,
        surfaces: BTreeMap<u64, (Vec<u8>, u32)>,
        level_pitch: u32,
        next: u64,
        desc: Option<SurfaceDesc>,
        lock_fail: bool,
        level_fail: bool,
    }
    impl FakeSurface {
        fn id(&mut self) -> u64 {
            self.next += 1;
            self.next
        }
    }
    impl TextureSurfaceSink for FakeSurface {
        type Error = &'static str;
        fn get_surface_level(
            &mut self,
            graphics: TextureGraphics,
            level: u16,
        ) -> Result<Option<u64>, Self::Error> {
            self.calls.push(format!("level:{level}"));
            if self.level_fail {
                return Ok(None);
            }
            let id = self.id();
            self.surfaces.insert(id, (Vec::new(), self.level_pitch));
            let _ = graphics;
            Ok(Some(id))
        }
        fn lock_surface(
            &mut self,
            surface: u64,
            rect: Option<Rect>,
            flags: u32,
        ) -> Result<Option<LockedMapping>, Self::Error> {
            self.calls.push(format!("lock:{flags}"));
            let Some((data, pitch)) = self.surfaces.get(&surface).cloned() else {
                return Ok(None);
            };
            if self.lock_fail {
                return Ok(None);
            }
            let _ = rect;
            Ok(Some(LockedMapping {
                mapping: surface,
                pitch,
                writable_bytes: data.len() as u32,
            }))
        }
        fn write_locked(&mut self, mapping: u64, bytes: &[u8]) -> Result<(), Self::Error> {
            self.calls.push(format!("write:{}", bytes.len()));
            let (data, _) = self.surfaces.get_mut(&mapping).ok_or("unknown mapping")?;
            data.clear();
            data.extend_from_slice(bytes);
            Ok(())
        }
        fn unlock_surface(&mut self, surface: u64) -> Result<(), Self::Error> {
            self.calls.push("unlock".into());
            if !self.surfaces.contains_key(&surface) {
                return Err("unknown surface");
            }
            Ok(())
        }
        fn release_surface(&mut self, surface: u64) -> Result<(), Self::Error> {
            self.calls.push("release".into());
            self.surfaces.remove(&surface);
            Ok(())
        }
        fn add_dirty_rect(
            &mut self,
            _graphics: TextureGraphics,
            rect: Option<Rect>,
        ) -> Result<i32, Self::Error> {
            self.calls.push(format!("dirty:{}", rect.is_some()));
            Ok(-5)
        }
        fn surface_desc(
            &mut self,
            _graphics: TextureGraphics,
            _level: u16,
        ) -> Result<Option<SurfaceDesc>, Self::Error> {
            self.calls.push("desc".into());
            Ok(self.desc)
        }
    }
    struct FakeDraw {
        calls: Vec<String>,
        begun: bool,
        format: u32,
        quads: Vec<[TlVertex; 4]>,
        strip_fail: bool,
        intersect: bool,
    }
    impl TextureDrawSink for FakeDraw {
        type Error = &'static str;
        fn scrn_size(&mut self) -> Result<(i32, i32), Self::Error> {
            self.calls.push("size".into());
            Ok((800, 600))
        }
        fn intersect_rect(
            &mut self,
            dst: &mut Rect,
            _a: &Rect,
            _b: &Rect,
        ) -> Result<bool, Self::Error> {
            self.calls.push("intersect".into());
            *dst = Rect {
                left: 0,
                top: 0,
                right: 1,
                bottom: 1,
            };
            Ok(self.intersect)
        }
        fn set_texture(&mut self, _key: TextureKey) -> Result<(), Self::Error> {
            self.calls.push("tex".into());
            Ok(())
        }
        fn set_sampler(&mut self, _stage: u32, kind: u32, value: u32) -> Result<(), Self::Error> {
            self.calls.push(format!("samp:{kind}:{value}"));
            Ok(())
        }
        fn draw_strip(
            &mut self,
            prim: u32,
            nprim: u32,
            verts: &[TlVertex],
        ) -> Result<i32, Self::Error> {
            self.calls
                .push(format!("strip:{prim}:{nprim}:{}", verts.len()));
            if self.strip_fail {
                return Ok(-1);
            }
            self.quads.push(verts.try_into().map_err(|_| "quad")?);
            Ok(0)
        }
        fn scene_begun(&mut self) -> Result<bool, Self::Error> {
            self.calls.push("begun".into());
            Ok(self.begun)
        }
        fn set_tl_format(&mut self, fvf: u32) -> Result<(), Self::Error> {
            self.calls.push(format!("setf:{fvf:#x}"));
            self.format = fvf;
            Ok(())
        }
        fn tl_format(&mut self) -> Result<u32, Self::Error> {
            self.calls.push("format".into());
            Ok(self.format)
        }
        fn bind_raw_graphics(&mut self, _graphics: TextureGraphics) -> Result<(), Self::Error> {
            self.calls.push("bind".into());
            Ok(())
        }
        fn draw_point_up(&mut self, buf: &[TlVertex]) -> Result<(), Self::Error> {
            self.calls.push(format!("point:{}", buf.len()));
            Ok(())
        }
        fn begin_scene(&mut self) -> Result<(), Self::Error> {
            self.calls.push("begin".into());
            Ok(())
        }
        fn end_scene(&mut self) -> Result<(), Self::Error> {
            self.calls.push("end".into());
            Ok(())
        }
    }
    fn surface_world() -> (TextureResources, Backend, FakeSurface, TextureGraphics) {
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let k = install_live_texture(&mut r, &mut b, 4, 4, 21, 1);
        let g = r.texture(k).unwrap().graphics.unwrap();
        let mut s = FakeSurface {
            calls: Vec::new(),
            surfaces: BTreeMap::new(),
            level_pitch: 16,
            next: 1000,
            desc: Some(SurfaceDesc {
                width: 4,
                height: 4,
                size_bytes: 64,
            }),
            lock_fail: false,
            level_fail: false,
        };
        s.surfaces.insert(4242, (vec![0x77; 64], 16));
        (r, b, s, g)
    }
    #[test]
    fn surface_lock_rejects_null_and_orders_callbacks() {
        let (mut r, _b, mut s, g) = surface_world();
        assert_eq!(
            r.surface_lock(TextureGraphics(0), None, 0, &mut s).unwrap(),
            None
        );
        assert!(s.calls.is_empty());
        s.surfaces.insert(7, (vec![1, 2, 3], 9));
        // Drive through get_surface_level (id 1001), then lock that surface.
        // Pre-seed the level surface id by consuming one id first.
        let got = r.surface_lock(g, None, 2, &mut s).unwrap().unwrap();
        assert_eq!(s.calls[0], "level:2");
        assert_eq!(s.calls[1], "lock:32768");
        assert_eq!(got.1, 1001);
    }
    #[test]
    fn surface_dirty_forwards_code_verbatim() {
        let (mut r, _b, mut s, g) = surface_world();
        assert_eq!(r.surface_add_dirty(g, None, &mut s).unwrap(), -5);
        assert_eq!(s.calls, ["dirty:false"]);
        assert_eq!(
            r.surface_add_dirty(
                g,
                Some(Rect {
                    left: 1,
                    top: 2,
                    right: 3,
                    bottom: 4
                }),
                &mut s
            )
            .unwrap(),
            -5
        );
        assert_eq!(s.calls[1], "dirty:true");
    }
    #[test]
    fn preload_orders_begun_and_unbegun_paths() {
        let (mut r, _b, _s, g) = surface_world();
        let mut d = FakeDraw {
            calls: Vec::new(),
            begun: true,
            format: 4,
            quads: Vec::new(),
            strip_fail: false,
            intersect: true,
        };
        r.preload_texture(g, &mut d).unwrap();
        assert_eq!(d.calls, ["begun", "format", "bind", "point:1"]);
        let mut d = FakeDraw {
            calls: Vec::new(),
            begun: true,
            format: 0,
            quads: Vec::new(),
            strip_fail: false,
            intersect: true,
        };
        r.preload_texture(g, &mut d).unwrap();
        assert_eq!(
            d.calls,
            ["begun", "format", "setf:0x1c4", "bind", "point:1"]
        );
        let mut d = FakeDraw {
            calls: Vec::new(),
            begun: false,
            format: 4,
            quads: Vec::new(),
            strip_fail: false,
            intersect: true,
        };
        r.preload_texture(g, &mut d).unwrap();
        assert_eq!(
            d.calls,
            ["begun", "begin", "format", "bind", "point:1", "end"]
        );
    }
    #[test]
    fn fill_writes_rows_then_unlocks_and_dirties() {
        let (mut r, _b, mut s, g) = surface_world();
        assert!(r.fill_texture(g, 0, None, 0x11223344, 21, &mut s).unwrap());
        assert_eq!(s.calls[0], "desc");
        assert_eq!(s.calls[1], "level:0");
        assert!(s.calls.iter().any(|c| c == "write:64"));
        assert!(s.calls.iter().any(|c| c == "dirty:false"));
        let pos = s.calls.iter().position(|c| c == "write:64").unwrap();
        assert!(s.calls[pos + 1..].contains(&"unlock".to_string()));
        // Subrect fill sizes from the rect and strides rows by pitch:
        // 2x2 of 0x11223344 over pitch 16 leaves row padding untouched.
        s.calls.clear();
        let sub = Rect {
            left: 1,
            top: 1,
            right: 3,
            bottom: 3,
        };
        assert!(r
            .fill_texture(g, 0, Some(sub), 0x11223344, 21, &mut s)
            .unwrap());
        let sub_id = *s.surfaces.keys().filter(|k| **k < 4242).max().unwrap();
        let written = s.surfaces.get(&sub_id).unwrap().0.clone();
        assert_eq!(written.len(), 32);
        assert_eq!(
            &written[..8],
            &[0x44, 0x33, 0x22, 0x11, 0x44, 0x33, 0x22, 0x11]
        );
        assert_eq!(&written[8..16], &[0; 8]);
        assert_eq!(
            &written[16..24],
            &[0x44, 0x33, 0x22, 0x11, 0x44, 0x33, 0x22, 0x11]
        );
        assert_eq!(&written[24..32], &[0; 8]);
        // Unsupported format unlocks and returns false with no dirty.
        s.calls.clear();
        assert!(!r.fill_texture(g, 0, None, 0, 99, &mut s).unwrap());
        assert!(s.calls.contains(&"unlock".to_string()));
        assert!(!s.calls.iter().any(|c| c.starts_with("dirty")));
        // Missing desc returns false with desc as the only callback.
        s.calls.clear();
        s.desc = None;
        assert!(!r.fill_texture(g, 0, None, 0, 21, &mut s).unwrap());
        assert_eq!(s.calls, ["desc"]);
        // Short-pitch lock unlocks and reports false with no write and no
        // dirty (native would overrun; the host refuses instead of
        // truncating row pixels).
        s.desc = Some(SurfaceDesc {
            width: 4,
            height: 4,
            size_bytes: 64,
        });
        s.level_pitch = 8;
        s.calls.clear();
        assert!(!r.fill_texture(g, 0, None, 0, 21, &mut s).unwrap());
        assert!(s.calls.contains(&"unlock".to_string()));
        assert!(!s.calls.iter().any(|c| c.starts_with("write")));
        assert!(!s.calls.iter().any(|c| c.starts_with("dirty")));
    }
    #[test]
    fn d3dfmt_fourcc_words_are_exact() {
        // DXT fourccs as little-endian ASCII ("DXT1".."DXT5"); a wrong word
        // (e.g. DXT1+2) would select the wrong compressor and quantizer.
        assert_eq!(D3DFMT_DXT1, 0x31545844);
        assert_eq!(D3DFMT_DXT2, 0x32545844);
        assert_eq!(D3DFMT_DXT3, 0x33545844);
        assert_eq!(D3DFMT_DXT4, 0x34545844);
        assert_eq!(D3DFMT_DXT5, 0x35545844);
        assert_eq!(&D3DFMT_DXT3.to_le_bytes(), b"DXT3");
        assert_eq!(&D3DFMT_DXT1.to_le_bytes(), b"DXT1");
    }
    #[test]
    fn draw_orders_quad_clamp_strip_wrap() {
        let (mut r, _b, _s, _g) = surface_world();
        let k = r.create_texture().unwrap();
        r.close_texture(k, &mut Backend::default()).unwrap();
        let mut d = FakeDraw {
            calls: Vec::new(),
            begun: true,
            format: 4,
            quads: Vec::new(),
            strip_fail: false,
            intersect: true,
        };
        assert!(r.draw_texture_at(k, 10, 20, 4, 4, &mut d).unwrap());
        assert_eq!(
            d.quads[0][0],
            TlVertex {
                x: 10.0,
                y: 20.0,
                diffuse: 0xffff_ffff,
                u: 0.0,
                v: 0.0
            }
        );
        assert_eq!(
            d.quads[0][3],
            TlVertex {
                x: 14.0,
                y: 24.0,
                diffuse: 0xffff_ffff,
                u: 1.0,
                v: 1.0
            }
        );
        assert_eq!(
            d.calls,
            [
                "size",
                "intersect",
                "tex",
                "samp:13:3",
                "samp:14:3",
                "strip:5:2:4",
                "samp:13:1",
                "samp:14:1"
            ]
        );
        // Offscreen short-circuits true with no draw.
        let mut d = FakeDraw {
            calls: Vec::new(),
            begun: true,
            format: 4,
            quads: Vec::new(),
            strip_fail: false,
            intersect: false,
        };
        assert!(r.draw_texture_at(k, 10, 20, 4, 4, &mut d).unwrap());
        assert_eq!(d.calls, ["size", "intersect"]);
        assert!(d.quads.is_empty());
        // Negative strip skips the wrap restore and returns false.
        let mut d = FakeDraw {
            calls: Vec::new(),
            begun: true,
            format: 4,
            quads: Vec::new(),
            strip_fail: true,
            intersect: true,
        };
        assert!(!r.draw_texture_at(k, 10, 20, 4, 4, &mut d).unwrap());
        assert!(!d.calls.iter().any(|c| c == "samp:13:1"));
        // Tinted rect publishes diffuse colors in order.
        let mut d = FakeDraw {
            calls: Vec::new(),
            begun: true,
            format: 4,
            quads: Vec::new(),
            strip_fail: false,
            intersect: true,
        };
        assert!(r
            .draw_texture_rect(
                k,
                Rect {
                    left: 0,
                    top: 0,
                    right: 8,
                    bottom: 8
                },
                [1, 2, 3, 4],
                &mut d
            )
            .unwrap());
        assert_eq!(d.quads[0].map(|v| v.diffuse), [1, 2, 3, 4]);
    }
    struct FakeS3tc {
        calls: Vec<String>,
        locks_done: u32,
        fail_src_lock: bool,
        fail_dst_lock: bool,
    }
    impl TextureSurfaceSink for FakeS3tc {
        type Error = &'static str;
        fn get_surface_level(
            &mut self,
            _g: TextureGraphics,
            _l: u16,
        ) -> Result<Option<u64>, Self::Error> {
            self.calls.push("level".into());
            Ok(Some(11))
        }
        fn lock_surface(
            &mut self,
            _s: u64,
            _r: Option<Rect>,
            f: u32,
        ) -> Result<Option<LockedMapping>, Self::Error> {
            let n = self.locks_done;
            self.locks_done += 1;
            self.calls.push(format!("lock:{f:#x}"));
            if (n == 0 && self.fail_src_lock) || (n == 1 && self.fail_dst_lock) {
                return Ok(None);
            }
            Ok(Some(LockedMapping {
                mapping: 11,
                pitch: 16,
                writable_bytes: 64,
            }))
        }
        fn write_locked(&mut self, _m: u64, _b: &[u8]) -> Result<(), Self::Error> {
            Ok(())
        }
        fn unlock_surface(&mut self, _s: u64) -> Result<(), Self::Error> {
            self.calls.push("unlock".into());
            Ok(())
        }
        fn release_surface(&mut self, _s: u64) -> Result<(), Self::Error> {
            Ok(())
        }
        fn add_dirty_rect(
            &mut self,
            _g: TextureGraphics,
            _r: Option<Rect>,
        ) -> Result<i32, Self::Error> {
            Ok(0)
        }
        fn surface_desc(
            &mut self,
            _g: TextureGraphics,
            _l: u16,
        ) -> Result<Option<SurfaceDesc>, Self::Error> {
            Ok(Some(SurfaceDesc {
                width: 4,
                height: 4,
                size_bytes: 64,
            }))
        }
    }
    impl S3tcEncodeSink for FakeS3tc {
        type Error = &'static str;
        fn s3tc_encode(
            &mut self,
            _sb: u64,
            _sp: u32,
            _w: u32,
            _h: u32,
            masks: [u32; 4],
            _db: u64,
            encode: u32,
            weights: [f32; 3],
        ) -> Result<(), Self::Error> {
            self.calls
                .push(format!("encode:{encode}:{masks:?}:{weights:?}"));
            Ok(())
        }
        fn debug_name(&mut self, text: Option<&str>) -> Result<(), Self::Error> {
            self.calls.push(format!("debug:{text:?}"));
            Ok(())
        }
        fn log_enabled(&mut self) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }
    #[test]
    fn compress_selects_masks_and_orders_unlocks() {
        use texture_surface::compress_texture;
        let mut s = FakeS3tc {
            calls: Vec::new(),
            locks_done: 0,
            fail_src_lock: false,
            fail_dst_lock: false,
        };
        let g = TextureGraphics(9);
        let desc = SurfaceDesc {
            width: 4,
            height: 4,
            size_bytes: 64,
        };
        assert!(compress_texture(&mut s, g, Some(desc), 1, 16, 101, 202, Some("wall")).unwrap());
        // Full native order: 2x GetSurfaceLevel, lock src READONLY 0x10,
        // lock dst, debug-name, encode, debug-clear, unlock src, unlock dst.
        assert!(s.calls[0] == "level");
        assert!(s.calls[1] == "level");
        assert!(s.calls[2] == "lock:0x10");
        assert!(s.calls[3] == "lock:0x0");
        assert!(s.calls[4] == "debug:Some(\"wall\")");
        assert!(s.calls[5].starts_with("encode:0:[31744, 992, 31, 32768]"));
        assert!(s.calls[6] == "debug:None");
        assert_eq!(&s.calls[7..], ["unlock", "unlock"]);
        let mut s = FakeS3tc {
            calls: Vec::new(),
            locks_done: 0,
            fail_src_lock: false,
            fail_dst_lock: false,
        };
        assert!(compress_texture(&mut s, g, Some(desc), 2, 16, 101, 202, None).unwrap());
        assert!(s.calls[5].starts_with("encode:2:"));
        let mut s = FakeS3tc {
            calls: Vec::new(),
            locks_done: 0,
            fail_src_lock: false,
            fail_dst_lock: false,
        };
        assert!(compress_texture(&mut s, g, Some(desc), 3, 16, 101, 202, None).unwrap());
        assert!(s.calls[5].starts_with("encode:256:[3840, 240, 15, 61440]"));
        assert!(!compress_texture(&mut s, g, None, 1, 16, 101, 202, None).unwrap());
        // Lock-fail discipline: src miss unlocks nothing, dst miss unlocks
        // only the acquired src (native order: GetDesc, lock src READONLY,
        // lock dst, encode, unlock src, unlock dst).
        let mut s = FakeS3tc {
            calls: Vec::new(),
            locks_done: 0,
            fail_src_lock: true,
            fail_dst_lock: false,
        };
        assert!(!compress_texture(&mut s, g, Some(desc), 1, 16, 101, 202, None).unwrap());
        assert_eq!(s.calls, ["level", "level", "lock:0x10"]);
        let mut s = FakeS3tc {
            calls: Vec::new(),
            locks_done: 0,
            fail_src_lock: false,
            fail_dst_lock: true,
        };
        assert!(!compress_texture(&mut s, g, Some(desc), 1, 16, 101, 202, None).unwrap());
        assert_eq!(
            s.calls,
            ["level", "level", "lock:0x10", "lock:0x0", "unlock"]
        );
    }
    #[test]
    fn texture_faces_roundtrip_with_detach_words() {
        // RVAs 0x305b0/0x30690/0x30780/0x30860 through the texture facade:
        // set -> owned get -> unlock -> shared detach duplicates.
        let mut r = TextureResources::new().unwrap();
        let mut b = Backend::default();
        let material = r.create_material().unwrap();
        let group = r.create_face_group().unwrap();
        let other = r.create_face_group().unwrap();
        r.set_material(group, Some(material), &mut b).unwrap();
        r.set_material(other, Some(material), &mut b).unwrap();
        let faces: Vec<vr::I3dTriface> = vec![[7, 7, 7]];
        r.set_faces(group, Some(&faces), &mut b).unwrap();
        assert_eq!(r.get_faces(group, &mut b).unwrap().unwrap(), faces);
        r.unlock_faces(group, &mut b).unwrap();
        // Creator reference still held: shared -> detach duplicates.
        assert_eq!(r.detach_face_group_material(group, &mut b).unwrap(), 0);
        let fresh = r
            .objects()
            .managed()
            .resources()
            .face_group(group)
            .unwrap()
            .material
            .unwrap();
        assert_ne!(fresh, material);
        assert!(!r.is_poisoned());
    }
}
