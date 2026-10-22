//! PC ITexture lifetime and concrete material integration.
//! Original PC LS3DF.dll SHA-256:
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Scoped RVAs: ctor0x75c10, Release0x75cd0, nondeleting dtor0x75c50,
//! Close0x78680, Unlock0x78820; managed removal0x7aee0 is defined in texture_manager. ITexture/C_str names have original PS2
//! provenance; PC layout/order are independently evidenced, not PS2 equivalence.
//! TextureResources is a descriptive host facade, not Create/upload/Lock or
//! renderer texture ownership. Constructor-unwritten fields remain unavailable.
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
        let bindings = self.textures.manager.install(input)?;
        for binding in bindings {
            let t = self.textures.texture_mut(binding.texture);
            t.flags = binding.flags;
            t.managed_record = binding.record;
            t.graphics = binding.graphics;
            t.managed_admitted = true;
        }
        Ok(())
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
        section_held: bool,
        fail_enter: bool,
        fail_filename: bool,
        fail_record: bool,
        fail_leave: bool,
        managed_files: BTreeMap<u64, bool>,
        managed_records: BTreeMap<u64, bool>,
        creation_enabled: bool,
        filename_none: bool,
        record_none: bool,
        array_none: bool,
        fail_copy_array: bool,
        filename_bytes: BTreeMap<u64, Vec<u8>>,
        arrays: BTreeMap<u64, Vec<Option<ManagedRecordKey>>>,
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
    impl ManagedTextureEffects for Backend {
        type Error = &'static str;
        fn creation_effects(
            &mut self,
        ) -> Option<&mut dyn ManagedTextureCreationEffects<Error = Self::Error>> {
            if self.creation_enabled {
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
}
