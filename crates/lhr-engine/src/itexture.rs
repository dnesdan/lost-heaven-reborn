//! Nonmanaged PC ITexture lifetime and concrete material integration.
//! Original PC LS3DF.dll SHA-256:
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Scoped RVAs: ctor0x75c10, Release0x75cd0, nondeleting dtor0x75c50,
//! Close0x78680, Unlock0x78820. ITexture/C_str names have original PS2
//! provenance; PC layout/order are independently evidenced, not PS2 equivalence.
//! TextureResources is a descriptive host facade, not Create/upload/Lock or
//! renderer texture ownership. Constructor-unwritten fields remain unavailable.
//! Managed/queued/pool ownership is rejected before effects (mask0xc0000040).
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
use crate::visual_resources::{
    self as vr, FaceGroupKey, MaterialKey, MeshLevelKey, MeshObjectKey, RawVertexBytesKey,
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
/// Descriptive identity for explicitly excluded managed-record ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ManagedRecordKey(pub u64);
pub trait TextureBackend:
    BufferBackend + CStrBackend<Error = <Self as BufferBackend>::Error>
{
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
struct Textures {
    identity: u64,
    slots: Vec<Option<ITexture>>,
    cached: Vec<TextureKey>,
    uncached: Vec<TextureKey>,
    poisoned: bool,
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
        if self.live(k)?.flags & 0xc0000040 != 0 || self.live(k)?.managed_record.is_some() {
            Err(TextureError::ManagedUnsupported)
        } else {
            Ok(())
        }
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
        self.domain(k)?;
        let result = (|| {
            self.unlock(k, b)?;
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
        self.domain(k)?;
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
            },
            external_strings: Vec::new(),
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
        if t.intake_installed || t.references != 1 || t.external_references != 1 {
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
    pub fn set_lod<B: TextureBackend + ?Sized>(
        &mut self,
        mesh: MeshObjectKey,
        level: Option<MeshLevelKey>,
        index: u32,
        backend: &mut B,
    ) -> Result<(), TextureOperationError<<B as BufferBackend>::Error>> {
        self.run_objects(backend, |o, b| o.set_lod(mesh, level, index, b))
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
    use super::*;
    use crate::c_str::{CRepKey, StringAllocation};
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
                file_name: Some(b"maps/wall.tga"),
                name_offset: Some(5),
                alpha_file_name: AlphaName::Bytes(b"maps/wall_a.tga"),
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
    impl TextureBackend for Backend {
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
}
