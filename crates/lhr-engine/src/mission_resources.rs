//! Canonical mission cache ownership. Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! UnInit RVA0x13f3b0 inlines reverse object Release, then item erase
//! (RVA0x148330); cache directories and allocated item capacity survive Clear.
//! Model and sound cache entries retain real frame references in ModelArena.
//! Sound frames bind a separate host PCM source/voice owner; this is not an
//! original DirectSound runtime. Animation cache ownership remains separate.
use crate::c_str::{CStrBackend, CStrKey, CStrings, StringError, StringOperationError};
use crate::frame_transform::FrameKey;
use crate::mission_components::{MissionMemoryDomain, MissionMemoryDomainId};
use crate::model_loading::{ModelArena, ModelError, ModelKey};
use crate::script_scheduler::FrameReferenceId;

#[path = "mission_model_cache.rs"]
mod model_cache;
#[path = "mission_sound_cache.rs"]
mod sound_cache;
pub use model_cache::{
    CacheMemoryError, CpuMissionCacheMemory, ModelCacheOpenError, ModelCacheOpenProgress,
    ModelCachePhase,
};
pub use sound_cache::{SoundCacheOpenError, SoundCacheOpenProgress, SoundCachePhase};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionCacheKind {
    Sound,
    Model,
    Animation,
}
impl MissionCacheKind {
    fn slot(self) -> usize {
        match self {
            Self::Sound => 0,
            Self::Model => 1,
            Self::Animation => 2,
        }
    }
}
/// Seed for actual 0x28-byte PC cache object; identities are observations.
#[derive(Clone, Copy)]
pub struct MissionCacheSeed {
    pub identity: u32,
    pub bytes: [u8; 0x28],
}
fn put(bytes: &mut [u8], at: usize, x: u32) {
    bytes[at..at + 4].copy_from_slice(&x.to_le_bytes());
}
fn constructor_header(kind: MissionCacheKind, seed: MissionCacheSeed) -> [u8; 0x28] {
    let mut bytes = seed.bytes;
    for offset in [4, 0x14] {
        bytes[offset] = (seed.identity >> 24) as u8;
        for delta in [4, 8, 12] {
            put(&mut bytes, offset + delta, 0);
        }
    }
    put(&mut bytes, 0x24, 0x100);
    put(
        &mut bytes,
        0,
        match kind {
            MissionCacheKind::Model => 0x625cf4,
            MissionCacheKind::Sound => 0x625d8c,
            MissionCacheKind::Animation => 0x625e24,
        },
    );
    bytes
}
#[derive(Debug)]
pub enum MissionCacheError<E> {
    Strings(StringOperationError<E>),
    Model(ModelError),
    Interrupted,
    ForeignDomain,
    Destructed,
    UnavailableResourceOwner(MissionCacheKind),
    AddDirUnavailable,
    HeaderProjectionUnavailable,
    HostAdoptionIntoNativeArray,
    UnsupportedDirectoryFormat,
    ArrayAllocationUnavailable,
    StringConstructorNullFault,
    InvalidArrayAllocation,
    Extent,
    Array(E),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CacheResource {
    Model(ModelKey),
    Sound(FrameKey),
}
impl CacheResource {
    fn frame(self) -> FrameKey {
        match self {
            Self::Model(model) => model.root(),
            Self::Sound(sound) => sound,
        }
    }
}
#[derive(Debug)]
struct CacheItem {
    resource: Option<CacheResource>,
    name: CStrKey,
}
impl CacheItem {
    fn model(&self) -> Option<ModelKey> {
        match self.resource {
            Some(CacheResource::Model(model)) => Some(model),
            _ => None,
        }
    }
    fn sound(&self) -> Option<FrameKey> {
        match self.resource {
            Some(CacheResource::Sound(sound)) => Some(sound),
            _ => None,
        }
    }
}
struct Cache {
    items: Vec<CacheItem>,
    dirs: Vec<CStrKey>,
    interrupted: bool,
    destructed: bool,
    pending_names: Vec<CStrKey>,
    pending_arrays: Vec<(u32, u32)>,
    header: [u8; 0x28],
    header_available: bool,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            dirs: Vec::new(),
            interrupted: false,
            destructed: false,
            pending_names: Vec::new(),
            pending_arrays: Vec::new(),
            header: [0; 0x28],
            header_available: false,
        }
    }
}
/// Original cache vector allocation primitive. All successful identities must
/// be unique across the same allocator's live component, string and array owners.
pub trait MissionCacheAllocator: CStrBackend + MissionMemoryDomain {
    fn allocate_cache_array(&mut self, bytes: u32) -> Result<Option<u32>, Self::Error>;
    fn free_cache_array(&mut self, identity: u32) -> Result<(), Self::Error>;
}
/// One owner shared by startup and C_mission. Rust Drop never invokes native
/// Release or allocator callbacks: explicit Clear remains required.
pub struct MissionCaches {
    caches: [Cache; 3],
    strings: CStrings,
    frame_domain: Option<FrameReferenceId>,
    memory_domain: Option<MissionMemoryDomainId>,
    model_open: Option<ModelCacheOpenProgress>,
    sound_open: Option<SoundCacheOpenProgress>,
}
impl MissionCaches {
    /// Construct over the original globals' zero-initialized storage seeds.
    /// Explicit seeds are available for independent constructor comparisons.
    pub fn empty() -> Result<Self, StringError> {
        Self::construct([
            MissionCacheSeed {
                identity: 0x647da8,
                bytes: [0; 0x28],
            },
            MissionCacheSeed {
                identity: 0x647dd0,
                bytes: [0; 0x28],
            },
            MissionCacheSeed {
                identity: 0x647d80,
                bytes: [0; 0x28],
            },
        ])
    }
    /// Execute the three exact constructors (RVA1bd080/1bd230/1bd3e0)
    /// over supplied seeds, in the array order Sound, Model, Animation.
    pub fn construct(seeds: [MissionCacheSeed; 3]) -> Result<Self, StringError> {
        let kinds = [
            MissionCacheKind::Sound,
            MissionCacheKind::Model,
            MissionCacheKind::Animation,
        ];
        Ok(Self {
            caches: std::array::from_fn(|n| Cache {
                header: constructor_header(kinds[n], seeds[n]),
                header_available: true,
                ..Cache::default()
            }),
            strings: CStrings::new()?,
            frame_domain: None,
            memory_domain: None,
            model_open: None,
            sound_open: None,
        })
    }
    /// Actual seeded member projection. Host-adopted model arrays have no
    /// native address projection until a real cache array allocator owns them.
    pub fn raw_header(&self, kind: MissionCacheKind) -> Option<&[u8; 0x28]> {
        let cache = &self.caches[kind.slot()];
        cache.header_available.then_some(&cache.header)
    }
    fn bind_memory<B: CStrBackend + MissionMemoryDomain>(
        &mut self,
        backend: &B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        let memory = backend.mission_memory_domain();
        if self.memory_domain.is_some_and(|bound| bound != memory) {
            return Err(MissionCacheError::ForeignDomain);
        }
        self.memory_domain = Some(memory);
        Ok(())
    }
    fn bind<B: CStrBackend + MissionMemoryDomain>(
        &mut self,
        arena: &ModelArena,
        backend: &B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        self.bind_memory(backend)?;
        let frame = arena.lookup_reference(FrameKey(0));
        if self.frame_domain.is_some_and(|bound| bound != frame) {
            return Err(MissionCacheError::ForeignDomain);
        }
        self.frame_domain = Some(frame);
        Ok(())
    }
    pub fn len(&self, kind: MissionCacheKind) -> usize {
        self.caches[kind.slot()].items.len()
    }
    pub fn directories(&self, kind: MissionCacheKind) -> &[CStrKey] {
        &self.caches[kind.slot()].dirs
    }
    pub fn strings(&self) -> &CStrings {
        &self.strings
    }
    pub fn pending_arrays(&self, kind: MissionCacheKind) -> &[(u32, u32)] {
        &self.caches[kind.slot()].pending_arrays
    }
    pub fn pending_names(&self, kind: MissionCacheKind) -> &[CStrKey] {
        &self.caches[kind.slot()].pending_names
    }
    pub fn is_interrupted(&self, kind: MissionCacheKind) -> bool {
        self.caches[kind.slot()].interrupted
    }
    /// Explicit host adoption, not a reconstruction of the original cache's
    /// loading/insertion function. Retains the same actual model and its name.
    /// Once a real native item array exists, use the reconstructed Open path;
    /// this helper cannot fabricate an original vector insertion into it.
    pub fn retain_model<B: CStrBackend + MissionMemoryDomain>(
        &mut self,
        model: ModelKey,
        name: &[u8],
        arena: &mut ModelArena,
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        self.bind(arena, backend)?;
        let cache = &mut self.caches[MissionCacheKind::Model.slot()];
        if cache.destructed {
            return Err(MissionCacheError::Destructed);
        }
        if cache.interrupted {
            return Err(MissionCacheError::Interrupted);
        }
        if u32::from_le_bytes(cache.header[8..12].try_into().unwrap()) != 0 {
            return Err(MissionCacheError::HostAdoptionIntoNativeArray);
        }
        arena.model(model).map_err(MissionCacheError::Model)?;
        let key = self
            .strings
            .create_null()
            .map_err(|e| MissionCacheError::Strings(e.into()))?;
        cache.pending_names.push(key);
        if let Err(error) = self.strings.assign(key, Some(name), backend) {
            cache.interrupted = true;
            return Err(MissionCacheError::Strings(error));
        }
        if let Err(error) = arena.retain(model.root()) {
            cache.interrupted = true;
            return Err(MissionCacheError::Model(error));
        }
        cache.pending_names.pop();
        cache.items.push(CacheItem {
            resource: Some(CacheResource::Model(model)),
            name: key,
        });
        cache.header_available = false;
        Ok(())
    }
    /// Release all objects from last to first before destroying ANY item name.
    /// An unavailable host callback stops the actual prefix; retry is rejected
    /// to avoid inventing native exception recovery or double releasing it.
    pub fn clear<B: CStrBackend + MissionMemoryDomain>(
        &mut self,
        kind: MissionCacheKind,
        arena: &mut ModelArena,
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        self.bind(arena, backend)?;
        let cache = &mut self.caches[kind.slot()];
        if cache.destructed {
            return Err(MissionCacheError::Destructed);
        }
        if cache.interrupted {
            return Err(MissionCacheError::Interrupted);
        }
        for item in cache.items.iter().rev() {
            let resource = item.resource.ok_or(MissionCacheError::Interrupted)?;
            if !matches!(
                (kind, resource),
                (MissionCacheKind::Model, CacheResource::Model(_))
                    | (MissionCacheKind::Sound, CacheResource::Sound(_))
            ) {
                cache.interrupted = true;
                return Err(MissionCacheError::Interrupted);
            }
            if let Err(error) = arena.release(resource.frame()) {
                cache.interrupted = true;
                return Err(MissionCacheError::Model(error));
            }
        }
        for item in &cache.items {
            if let Err(error) = self.strings.destruct_string(item.name, backend) {
                cache.interrupted = true;
                return Err(MissionCacheError::Strings(error));
            }
        }
        cache.items.clear();
        if cache.header_available {
            let begin = u32::from_le_bytes(cache.header[8..12].try_into().unwrap());
            put(&mut cache.header, 12, begin);
        }
        Ok(())
    }
    fn directory_input<E>(path: &[u8]) -> Result<&[u8], MissionCacheError<E>> {
        let path = &path[..path.iter().position(|&x| x == 0).unwrap_or(path.len())];
        // Actual Game C_str constructor408cc0 formats into a1024-byte local
        // buffer. Literal formats below the limit (including empty) need no
        // varargs. Percent directives/unterminated long output are separate
        // CRT/caller-stack dependencies, not silently literal filenames.
        if path.contains(&b'%') {
            return Err(MissionCacheError::UnsupportedDirectoryFormat);
        }
        if path.len() >= 1024 {
            return Err(MissionCacheError::Extent);
        }
        Ok(path)
    }
    fn normalized_directory<B: MissionCacheAllocator>(
        &mut self,
        slot: usize,
        path: &[u8],
        backend: &mut B,
    ) -> Result<CStrKey, MissionCacheError<B::Error>> {
        let key = self
            .strings
            .create_null()
            .map_err(|e| MissionCacheError::Strings(e.into()))?;
        self.caches[slot].pending_names.push(key);
        self.strings
            .assign(key, Some(path), backend)
            .map_err(MissionCacheError::Strings)?;
        if self
            .strings
            .string(key)
            .map_err(|e| MissionCacheError::Strings(e.into()))?
            .rep
            .is_none()
        {
            // Both native callers immediately dereference the fresh rep.
            return Err(MissionCacheError::StringConstructorNullFault);
        }
        if path.last() != Some(&b'\\') {
            let mut joined = path.to_vec();
            joined.push(b'\\');
            self.strings
                .assign(key, Some(&joined), backend)
                .map_err(MissionCacheError::Strings)?;
        }
        Ok(key)
    }
    fn remove_temporary<B: MissionCacheAllocator>(
        &mut self,
        slot: usize,
        key: CStrKey,
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        self.strings
            .destruct_string(key, backend)
            .map_err(MissionCacheError::Strings)?;
        if let Some(at) = self.caches[slot]
            .pending_names
            .iter()
            .position(|&value| value == key)
        {
            self.caches[slot].pending_names.remove(at);
        }
        Ok(())
    }
    fn del_dir_inner<B: MissionCacheAllocator>(
        &mut self,
        kind: MissionCacheKind,
        path: &[u8],
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        let slot = kind.slot();
        let temporary = self.normalized_directory(slot, path, backend)?;
        let target = self
            .strings
            .string(temporary)
            .map_err(|e| MissionCacheError::Strings(e.into()))?
            .rep;
        let target = target
            .map(|rep| self.strings.rep(rep).map(|r| r.bytes.clone()))
            .transpose()
            .map_err(|e| MissionCacheError::Strings(e.into()))?
            .unwrap_or_default();
        let mut found = None;
        for n in (0..self.caches[slot].dirs.len()).rev() {
            let key = self.caches[slot].dirs[n];
            let rep = self
                .strings
                .string(key)
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .rep;
            let bytes = rep
                .map(|rep| self.strings.rep(rep).map(|r| r.bytes.as_slice()))
                .transpose()
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .unwrap_or(&[]);
            if bytes == target {
                found = Some(n);
                break;
            }
        }
        if let Some(n) = found {
            for source in n + 1..self.caches[slot].dirs.len() {
                self.strings
                    .alias_into(
                        self.caches[slot].dirs[source - 1],
                        self.caches[slot].dirs[source],
                        backend,
                    )
                    .map_err(MissionCacheError::Strings)?;
            }
            let last = *self.caches[slot].dirs.last().unwrap();
            self.strings
                .destruct_string(last, backend)
                .map_err(MissionCacheError::Strings)?;
            self.caches[slot].dirs.pop();
            let end = self.header_word(slot, 0x1c).wrapping_sub(4);
            put(&mut self.caches[slot].header, 0x1c, end);
        }
        self.remove_temporary(slot, temporary, backend)
    }
    fn header_word(&self, slot: usize, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.caches[slot].header[offset..offset + 4]
                .try_into()
                .unwrap(),
        )
    }
    /// Original DelDir reverse-searches byte-exact normalized paths and removes
    /// only the last match. The array allocation and its capacity survive.
    pub fn del_dir<B: MissionCacheAllocator>(
        &mut self,
        kind: MissionCacheKind,
        path: &[u8],
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        self.bind_memory(backend)?;
        let path = Self::directory_input(path)?;
        let slot = kind.slot();
        if self.caches[slot].destructed {
            return Err(MissionCacheError::Destructed);
        }
        if self.caches[slot].interrupted {
            return Err(MissionCacheError::Interrupted);
        }
        let result = self.del_dir_inner(kind, path, backend);
        if result.is_err() {
            self.caches[slot].interrupted = true;
        }
        result
    }
    /// Original AddDir calls virtual DelDir first, then constructs another
    /// normalized C_str and appends one shared rep to the actual cache vector.
    /// Generic insertion's growth is count+max(count,1), preserving capacity
    /// when the freed last slot already fits (not a fixed reserve policy).
    pub fn add_dir<B: MissionCacheAllocator>(
        &mut self,
        kind: MissionCacheKind,
        path: &[u8],
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        self.bind_memory(backend)?;
        let path = Self::directory_input(path)?;
        let slot = kind.slot();
        if self.caches[slot].destructed {
            return Err(MissionCacheError::Destructed);
        }
        if self.caches[slot].interrupted {
            return Err(MissionCacheError::Interrupted);
        }
        let result = (|| {
            self.del_dir_inner(kind, path, backend)?;
            let temporary = self.normalized_directory(slot, path, backend)?;
            let old_begin = self.header_word(slot, 0x18);
            let old_end = self.header_word(slot, 0x1c);
            let cap = self.header_word(slot, 0x20);
            if cap.wrapping_sub(old_end) < 4 {
                let count = u32::try_from(self.caches[slot].dirs.len())
                    .map_err(|_| MissionCacheError::Extent)?;
                let new_count = count
                    .checked_add(count.max(1))
                    .ok_or(MissionCacheError::Extent)?;
                let bytes = new_count
                    .checked_mul(4)
                    .filter(|&n| n <= i32::MAX as u32)
                    .ok_or(MissionCacheError::Extent)?;
                let pointer = backend
                    .allocate_cache_array(bytes)
                    .map_err(MissionCacheError::Array)?
                    .ok_or(MissionCacheError::ArrayAllocationUnavailable)?;
                let aliases = self.caches.iter().any(|cache| {
                    let begin = u32::from_le_bytes(cache.header[0x18..0x1c].try_into().unwrap());
                    begin == pointer
                        || cache
                            .pending_arrays
                            .iter()
                            .any(|&(base, _)| base == pointer)
                });
                self.caches[slot].pending_arrays.push((pointer, bytes));
                if pointer == 0
                    || pointer.checked_add(bytes).is_none()
                    || aliases
                    || self.strings.owns_allocation(u64::from(pointer))
                {
                    return Err(MissionCacheError::InvalidArrayAllocation);
                }
                let mut copied = Vec::new();
                for &key in &self.caches[slot].dirs {
                    let copy = self
                        .strings
                        .copy_string(key)
                        .map_err(|e| MissionCacheError::Strings(e.into()))?;
                    self.caches[slot].pending_names.push(copy);
                    copied.push(copy);
                }
                let key = self
                    .strings
                    .copy_string(temporary)
                    .map_err(|e| MissionCacheError::Strings(e.into()))?;
                self.caches[slot].pending_names.push(key);
                copied.push(key);
                for &key in &self.caches[slot].dirs {
                    self.strings
                        .destruct_string(key, backend)
                        .map_err(MissionCacheError::Strings)?;
                }
                backend
                    .free_cache_array(old_begin)
                    .map_err(MissionCacheError::Array)?;
                put(&mut self.caches[slot].header, 0x20, pointer + bytes);
                put(&mut self.caches[slot].header, 0x18, pointer);
                put(
                    &mut self.caches[slot].header,
                    0x1c,
                    pointer + (count + 1) * 4,
                );
                self.caches[slot].dirs = copied;
                self.caches[slot]
                    .pending_arrays
                    .retain(|&(base, _)| base != pointer);
                let published = self.caches[slot].dirs.clone();
                self.caches[slot]
                    .pending_names
                    .retain(|key| !published.contains(key));
            } else {
                let key = self
                    .strings
                    .copy_string(temporary)
                    .map_err(|e| MissionCacheError::Strings(e.into()))?;
                self.caches[slot].dirs.push(key);
                put(&mut self.caches[slot].header, 0x1c, old_end + 4);
            }
            self.remove_temporary(slot, temporary, backend)
        })();
        if result.is_err() {
            self.caches[slot].interrupted = true;
        }
        result
    }
    /// Actual base cache destructor: Clear, directory C_str destruction and
    /// array free, empty item-array free, then the final base vtable store.
    /// The object allocation itself belongs to its external caller/global.
    pub fn destruct<B: MissionCacheAllocator>(
        &mut self,
        kind: MissionCacheKind,
        arena: &mut ModelArena,
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        self.bind(arena, backend)?;
        let slot = kind.slot();
        if self.caches[slot].destructed {
            return Err(MissionCacheError::Destructed);
        }
        if self.caches[slot].interrupted {
            return Err(MissionCacheError::Interrupted);
        }
        put(
            &mut self.caches[slot].header,
            0,
            match kind {
                MissionCacheKind::Model => 0x625cc0,
                MissionCacheKind::Sound => 0x625d58,
                MissionCacheKind::Animation => 0x625df0,
            },
        );
        let result = (|| {
            self.clear(kind, arena, backend)?;
            for &key in &self.caches[slot].dirs {
                self.strings
                    .destruct_string(key, backend)
                    .map_err(MissionCacheError::Strings)?;
            }
            let pointer = self.header_word(slot, 0x18);
            backend
                .free_cache_array(pointer)
                .map_err(MissionCacheError::Array)?;
            for offset in [0x18, 0x1c, 0x20] {
                put(&mut self.caches[slot].header, offset, 0);
            }
            self.caches[slot].dirs.clear();
            if !self.caches[slot].header_available {
                return Err(MissionCacheError::HeaderProjectionUnavailable);
            }
            let pointer = self.header_word(slot, 8);
            backend
                .free_cache_array(pointer)
                .map_err(MissionCacheError::Array)?;
            for offset in [8, 12, 16] {
                put(&mut self.caches[slot].header, offset, 0);
            }
            put(
                &mut self.caches[slot].header,
                0,
                match kind {
                    MissionCacheKind::Model => 0x625c90,
                    MissionCacheKind::Sound => 0x625d28,
                    MissionCacheKind::Animation => 0x625dc0,
                },
            );
            self.caches[slot].destructed = true;
            Ok(())
        })();
        if result.is_err() {
            self.caches[slot].interrupted = true;
        }
        result
    }
    /// Actual mission UnInit order differs from CloseSystem's cache order.
    pub fn clear_for_mission<B: CStrBackend + MissionMemoryDomain>(
        &mut self,
        arena: &mut ModelArena,
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        for kind in [
            MissionCacheKind::Sound,
            MissionCacheKind::Model,
            MissionCacheKind::Animation,
        ] {
            self.clear(kind, arena, backend)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::c_str::StringAllocation;
    struct Memory {
        domain: MissionMemoryDomainId,
        next: u64,
        frees: Vec<u64>,
        string_failure: bool,
        array_null: bool,
    }
    impl MissionMemoryDomain for Memory {
        fn mission_memory_domain(&self) -> MissionMemoryDomainId {
            self.domain
        }
    }
    impl CStrBackend for Memory {
        type Error = ();
        fn allocate_string(&mut self, _: u32) -> Result<Option<StringAllocation>, ()> {
            if self.string_failure {
                return Err(());
            }
            self.next += 1;
            Ok(Some(StringAllocation(self.next)))
        }
        fn free_string(&mut self, value: StringAllocation) -> Result<(), ()> {
            self.frees.push(value.0);
            Ok(())
        }
    }
    impl MissionCacheAllocator for Memory {
        fn allocate_cache_array(&mut self, _: u32) -> Result<Option<u32>, ()> {
            if self.array_null {
                return Ok(None);
            }
            self.next += 0x100;
            Ok(Some(self.next as u32))
        }
        fn free_cache_array(&mut self, value: u32) -> Result<(), ()> {
            self.frees.push(value as u64);
            Ok(())
        }
    }
    fn path_names(caches: &MissionCaches, kind: MissionCacheKind) -> Vec<Vec<u8>> {
        caches
            .directories(kind)
            .iter()
            .map(|&key| {
                caches
                    .strings()
                    .rep(caches.strings().string(key).unwrap().rep.unwrap())
                    .unwrap()
                    .bytes
                    .clone()
            })
            .collect()
    }
    #[test]
    fn directory_failures_keep_owned_temporary_and_stop_without_array_publication() {
        for string_failure in [false, true] {
            let mut caches = MissionCaches::empty().unwrap();
            let mut memory = Memory {
                domain: MissionMemoryDomainId::fresh(),
                next: 0x1000,
                frees: Vec::new(),
                string_failure,
                array_null: !string_failure,
            };
            let kind = MissionCacheKind::Model;
            assert!(caches.add_dir(kind, b"Models", &mut memory).is_err());
            assert!(caches.is_interrupted(kind));
            assert!(caches.directories(kind).is_empty());
            assert!(!caches.pending_names(kind).is_empty());
            let raw = caches.raw_header(kind).unwrap();
            assert_eq!(&raw[0x18..0x24], &[0; 12]);
            let pending = *caches.pending_names(kind).last().unwrap();
            assert_eq!(
                caches.strings().string(pending).unwrap().rep.is_none(),
                string_failure
            );
            let count = memory.next;
            assert!(matches!(
                caches.add_dir(kind, b"Other", &mut memory),
                Err(MissionCacheError::Interrupted)
            ));
            assert_eq!(memory.next, count);
        }
    }
    #[test]
    fn directory_normalization_reorders_exact_match_and_preserves_capacity_on_clear() {
        let mut caches = MissionCaches::empty().unwrap();
        let mut memory = Memory {
            domain: MissionMemoryDomainId::fresh(),
            next: 0x1000,
            frees: Vec::new(),
            string_failure: false,
            array_null: false,
        };
        let kind = MissionCacheKind::Model;
        assert!(matches!(
            caches.add_dir(kind, b"bad%s", &mut memory),
            Err(MissionCacheError::UnsupportedDirectoryFormat)
        ));
        assert!(caches.directories(kind).is_empty());
        caches.add_dir(kind, b"Models", &mut memory).unwrap();
        caches.add_dir(kind, b"Other", &mut memory).unwrap();
        let header = *caches.raw_header(kind).unwrap();
        caches.add_dir(kind, b"Models\\", &mut memory).unwrap();
        assert_eq!(
            path_names(&caches, kind),
            vec![b"Other\\".to_vec(), b"Models\\".to_vec()]
        );
        assert_eq!(
            caches.raw_header(kind).unwrap()[0x18..0x28],
            header[0x18..0x28]
        );
        caches.add_dir(kind, b"models", &mut memory).unwrap();
        assert_eq!(
            path_names(&caches, kind),
            vec![
                b"Other\\".to_vec(),
                b"Models\\".to_vec(),
                b"models\\".to_vec()
            ]
        );
        caches.del_dir(kind, b"Other", &mut memory).unwrap();
        assert_eq!(
            path_names(&caches, kind),
            vec![b"Models\\".to_vec(), b"models\\".to_vec()]
        );
        let header = *caches.raw_header(kind).unwrap();
        let free_count = memory.frees.len();
        caches
            .clear(kind, &mut ModelArena::default(), &mut memory)
            .unwrap();
        assert_eq!(caches.raw_header(kind).unwrap(), &header);
        assert_eq!(memory.frees.len(), free_count);
        assert!(caches.pending_names(kind).is_empty());
        assert!(caches.pending_arrays(kind).is_empty());
    }
    #[test]
    fn clear_releases_real_cached_models_and_rejects_foreign_arena_before_effects() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let mut foreign = ModelArena::default();
        let other = foreign.create_model().unwrap();
        assert_eq!(model.root(), other.root());
        let mut memory = Memory {
            domain: MissionMemoryDomainId::fresh(),
            next: 0,
            frees: Vec::new(),
            string_failure: false,
            array_null: false,
        };
        let mut caches = MissionCaches::empty().unwrap();
        caches
            .retain_model(model, b"example", &mut arena, &mut memory)
            .unwrap();
        arena.release(model.root()).unwrap();
        assert!(matches!(
            caches.clear(MissionCacheKind::Model, &mut foreign, &mut memory),
            Err(MissionCacheError::ForeignDomain)
        ));
        assert_eq!(foreign.frame(other.root()).unwrap().references, 1);
        assert!(memory.frees.is_empty());
        assert_eq!(caches.len(MissionCacheKind::Model), 1);
        caches
            .clear(MissionCacheKind::Model, &mut arena, &mut memory)
            .unwrap();
        assert!(arena.frame(model.root()).is_err());
        assert_eq!(memory.frees, vec![1]);
        assert_eq!(caches.len(MissionCacheKind::Model), 0);
    }
    #[test]
    fn seeded_cache_constructor_changes_only_evidenced_bytes() {
        let seed = [0xa5; 0x28];
        let caches = MissionCaches::construct(std::array::from_fn(|n| MissionCacheSeed {
            identity: 0x6d00_0000 + n as u32 * 0x100,
            bytes: seed,
        }))
        .unwrap();
        for kind in [
            MissionCacheKind::Sound,
            MissionCacheKind::Model,
            MissionCacheKind::Animation,
        ] {
            let bytes = caches.raw_header(kind).unwrap();
            for at in [4, 0x14] {
                assert_eq!(bytes[at], 0x6d);
                assert_eq!(&bytes[at + 1..at + 4], &[0xa5; 3]);
            }
            assert_eq!(
                u32::from_le_bytes(bytes[0x24..0x28].try_into().unwrap()),
                256
            );
        }
    }
}
