//! Original C_I3D_cache<I3D_model>::Open caller state (Game RVA0x87e0).
//! SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Cache entries/directories are the same MissionCaches owner used by startup
//! and UnInit. Static plain Mesh/Dummy copies share real CPU resource owners;
//! derived hierarchies and arbitrary callbacks remain explicit dependencies.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelCachePhase {
    Lookup,
    RemoveDestination,
    DuplicateHit,
    Directory,
    ModelOpen,
    InsertEntry,
    Complete,
}
#[derive(Clone, Debug)]
pub struct ModelCacheOpenProgress {
    pub target: ModelKey,
    pub name: Vec<u8>,
    pub flags: u32,
    pub phase: ModelCachePhase,
    pub directory: usize,
    pub path: Option<Vec<u8>>,
    pub cached_source: Option<ModelKey>,
    pub model_open_result: Option<i32>,
    /// Host invocation identity separates a rejected admission from an entered
    /// operation. It does not reconstruct an original object field.
    attempt: u64,
}
#[derive(Debug)]
pub enum ModelCacheOpenError<E> {
    Cache(MissionCacheError<E>),
    Model(ModelError),
    Unsupported { entry: u32, detail: &'static str },
}
impl<E> From<MissionCacheError<E>> for ModelCacheOpenError<E> {
    fn from(error: MissionCacheError<E>) -> Self {
        Self::Cache(error)
    }
}
/// Literal ASCII counterpart of original wildcard comparison: '*' accepts the
/// remaining suffix immediately; '?' consumes one non-NUL byte. Case folding
/// outside the default ASCII CRT domain requires the original locale owner.
fn matches_name(value: &[u8], pattern: &[u8]) -> bool {
    let mut at = 0;
    for &byte in pattern {
        if byte == b'*' {
            return true;
        }
        let Some(&actual) = value.get(at) else {
            return false;
        };
        if byte != b'?' && !byte.eq_ignore_ascii_case(&actual) {
            return false;
        }
        at += 1;
    }
    at == value.len()
}
impl MissionCaches {
    pub fn model_open_progress(&self) -> Option<&ModelCacheOpenProgress> {
        self.model_open.as_ref()
    }
    /// Forward original S_itm assignment5c2540, then tail destruction409230.
    /// Assignment publishes the object word before retaining/releasing names.
    fn erase_model_item<B: CStrBackend>(
        &mut self,
        index: usize,
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        let count = self.caches[1].items.len();
        for at in index..count - 1 {
            let source = self.caches[1].items[at + 1].name;
            let destination = self.caches[1].items[at].name;
            self.caches[1].items[at].model = self.caches[1].items[at + 1].model;
            if let Err(error) = self.strings.alias_into(destination, source, backend) {
                self.caches[1].interrupted = true;
                return Err(MissionCacheError::Strings(error));
            }
        }
        let tail = self.caches[1].items.last().unwrap().name;
        if let Err(error) = self.strings.destruct_string(tail, backend) {
            self.caches[1].interrupted = true;
            return Err(MissionCacheError::Strings(error));
        }
        self.caches[1].items.pop();
        let begin = self.header_word(1, 8);
        put(
            &mut self.caches[1].header,
            12,
            begin + (count as u32 - 1) * 8,
        );
        Ok(())
    }
    /// Execute actual cache selection and expose its first real directory path.
    /// Caller flags0/no external callback forwards0x10; hits select the existing
    /// genuine model owner for the cache wrapper's following Duplicate call.
    fn begin_model_open<B: CStrBackend + MissionMemoryDomain>(
        &mut self,
        arena: &ModelArena,
        target: ModelKey,
        name: &[u8],
        backend: &mut B,
    ) -> Result<Option<Vec<u8>>, ModelCacheOpenError<B::Error>> {
        self.bind(arena, backend)?;
        let slot = MissionCacheKind::Model.slot();
        if self.caches[slot].destructed {
            return Err(MissionCacheError::Destructed.into());
        }
        if self.caches[slot].interrupted {
            return Err(MissionCacheError::Interrupted.into());
        }
        arena.model(target).map_err(ModelCacheOpenError::Model)?;
        let name = &name[..name.iter().position(|&x| x == 0).unwrap_or(name.len())];
        if !name.is_ascii() {
            return Err(ModelCacheOpenError::Unsupported {
                entry: 0x4088c9,
                detail: "cache comparison requires original non-ASCII CRT locale",
            });
        }
        let attempt = self
            .model_open
            .as_ref()
            .map_or(0, |p| p.attempt.wrapping_add(1));
        self.model_open = Some(ModelCacheOpenProgress {
            target,
            name: name.to_vec(),
            flags: 0x10,
            phase: ModelCachePhase::Lookup,
            directory: 0,
            path: None,
            cached_source: None,
            model_open_result: None,
            attempt,
        });
        if self.caches[slot]
            .items
            .iter()
            .any(|item| item.model.is_none())
        {
            return Err(MissionCacheError::Interrupted.into());
        }
        if let Some(index) = self.caches[slot]
            .items
            .iter()
            .rposition(|item| item.model == Some(target))
        {
            self.model_open.as_mut().unwrap().phase = ModelCachePhase::RemoveDestination;
            // Original removes this slot WITHOUT releasing its model reference.
            self.erase_model_item(index, backend)?;
        }
        for index in 0..self.caches[slot].items.len() {
            // The original constructs/releases a separate request C_str for
            // every comparison, before invoking Duplicate on a matching item.
            let temporary = self
                .strings
                .create_null()
                .map_err(|e| MissionCacheError::Strings(e.into()))?;
            self.caches[slot].pending_names.push(temporary);
            if let Err(error) = self.strings.assign(temporary, Some(name), backend) {
                self.caches[slot].interrupted = true;
                return Err(MissionCacheError::Strings(error).into());
            }
            let key = self.caches[slot].items[index].name;
            let rep = self
                .strings
                .string(key)
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .rep;
            let bytes = rep
                .map(|rep| self.strings.rep(rep).map(|rep| rep.bytes.as_slice()))
                .transpose()
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .unwrap_or(&[]);
            if !bytes.is_ascii() {
                return Err(ModelCacheOpenError::Unsupported {
                    entry: 0x40892a,
                    detail: "cached name requires original non-ASCII CRT locale",
                });
            }
            let pattern_rep = self
                .strings
                .string(temporary)
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .rep;
            let pattern = pattern_rep
                .map(|rep| self.strings.rep(rep).map(|r| r.bytes.as_slice()))
                .transpose()
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .unwrap_or(&[]);
            let hit = matches_name(bytes, pattern);
            self.strings
                .destruct_string(temporary, backend)
                .map_err(MissionCacheError::Strings)?;
            self.caches[slot]
                .pending_names
                .retain(|&key| key != temporary);
            if hit {
                let source = self.caches[slot].items[index]
                    .model
                    .ok_or(MissionCacheError::Interrupted)?;
                let progress = self.model_open.as_mut().unwrap();
                progress.phase = ModelCachePhase::DuplicateHit;
                progress.cached_source = Some(source);
                return Ok(None);
            }
        }
        self.next_model_directory()
    }
    fn next_model_directory<E>(&mut self) -> Result<Option<Vec<u8>>, ModelCacheOpenError<E>> {
        let progress = self.model_open.as_mut().expect("model Open entered");
        progress.phase = ModelCachePhase::Directory;
        let dirs = &self.caches[MissionCacheKind::Model.slot()].dirs;
        let Some(&key) = dirs.get(progress.directory) else {
            progress
                .model_open_result
                .get_or_insert(0x80000001u32 as i32);
            progress.phase = ModelCachePhase::Complete;
            return Ok(None);
        };
        let rep = self
            .strings
            .string(key)
            .map_err(|e| MissionCacheError::Strings(e.into()))?
            .rep;
        let mut path = rep
            .map(|rep| self.strings.rep(rep).map(|rep| rep.bytes.clone()))
            .transpose()
            .map_err(|e| MissionCacheError::Strings(e.into()))?
            .unwrap_or_default();
        path.extend_from_slice(&progress.name);
        progress.path = Some(path.clone());
        progress.phase = ModelCachePhase::ModelOpen;
        Ok(Some(path))
    }
}

/// Concrete CPU heap for canonical cache strings/arrays. Identity arithmetic
/// uses reserved, disjoint byte ranges; buffers remain owned until explicit free.
pub struct CpuMissionCacheMemory {
    domain: MissionMemoryDomainId,
    next: u32,
    blocks: std::collections::BTreeMap<u32, Vec<u8>>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum CacheMemoryError {
    Extent,
    Allocation,
    UnknownAllocation,
}
impl Default for CpuMissionCacheMemory {
    fn default() -> Self {
        Self {
            domain: MissionMemoryDomainId::fresh(),
            next: 0x10000,
            blocks: Default::default(),
        }
    }
}
impl CpuMissionCacheMemory {
    pub fn live_allocations(&self) -> usize {
        self.blocks.len()
    }
    pub fn live_bytes(&self) -> usize {
        self.blocks.values().map(Vec::len).sum()
    }
    fn allocate(&mut self, bytes: u32) -> Result<Option<u32>, CacheMemoryError> {
        let next = self
            .next
            .checked_add(bytes.max(1))
            .and_then(|n| n.checked_add(15))
            .map(|n| n & !15)
            .ok_or(CacheMemoryError::Extent)?;
        let mut block = Vec::new();
        block
            .try_reserve_exact(bytes as usize)
            .map_err(|_| CacheMemoryError::Allocation)?;
        block.resize(bytes as usize, 0);
        let identity = self.next;
        self.blocks.insert(identity, block);
        self.next = next;
        Ok(Some(identity))
    }
    fn free(&mut self, id: u32) -> Result<(), CacheMemoryError> {
        if id == 0 {
            return Ok(());
        }
        self.blocks
            .remove(&id)
            .map(|_| ())
            .ok_or(CacheMemoryError::UnknownAllocation)
    }
}
impl MissionMemoryDomain for CpuMissionCacheMemory {
    fn mission_memory_domain(&self) -> MissionMemoryDomainId {
        self.domain
    }
}
impl CStrBackend for CpuMissionCacheMemory {
    type Error = CacheMemoryError;
    fn allocate_string(
        &mut self,
        bytes: u32,
    ) -> Result<Option<crate::c_str::StringAllocation>, Self::Error> {
        self.allocate(bytes)
            .map(|id| id.map(|id| crate::c_str::StringAllocation(u64::from(id))))
    }
    fn free_string(&mut self, id: crate::c_str::StringAllocation) -> Result<(), Self::Error> {
        self.free(u32::try_from(id.0).map_err(|_| CacheMemoryError::Extent)?)
    }
}
impl MissionCacheAllocator for CpuMissionCacheMemory {
    fn allocate_cache_array(&mut self, bytes: u32) -> Result<Option<u32>, Self::Error> {
        self.allocate(bytes)
    }
    fn free_cache_array(&mut self, id: u32) -> Result<(), Self::Error> {
        self.free(id)
    }
}

impl MissionCaches {
    /// Original flags0/no-outer-callback cache caller. Resource loading and
    /// pristine hierarchy copying use actual shared CPU resource owners.
    /// Entered host errors permanently interrupt this cache; they do not model
    /// an original HRESULT or exception recovery. Rejected admission leaves the
    /// previously valid owner usable, and native HRESULTs complete normally.
    pub fn open_static_model<B: MissionCacheAllocator>(
        &mut self,
        arena: &mut ModelArena,
        installation: &lhr_assets::Installation,
        target: ModelKey,
        name: &[u8],
        backend: &mut B,
    ) -> Result<i32, ModelCacheOpenError<B::Error>> {
        let previous_attempt = self.model_open.as_ref().map(|progress| progress.attempt);
        let result = self.open_static_model_inner(arena, installation, target, name, backend);
        // Native HRESULT returns complete normally. A host Err after entry
        // retains an incomplete original call, including NULL allocation and
        // partial ModelOpen/Duplicate effects; replay invents native recovery.
        let entered = self.model_open.as_ref().map(|progress| progress.attempt) != previous_attempt;
        if entered && result.is_err() {
            self.caches[1].interrupted = true;
        }
        result
    }
    fn open_static_model_inner<B: MissionCacheAllocator>(
        &mut self,
        arena: &mut ModelArena,
        installation: &lhr_assets::Installation,
        target: ModelKey,
        name: &[u8],
        backend: &mut B,
    ) -> Result<i32, ModelCacheOpenError<B::Error>> {
        let mut path = self.begin_model_open(arena, target, name, backend)?;
        if self.model_open.as_ref().unwrap().phase == ModelCachePhase::DuplicateHit {
            let source = self.model_open.as_ref().unwrap().cached_source.unwrap();
            arena
                .duplicate_pristine_cache_model(target, source)
                .map_err(ModelCacheOpenError::Model)?;
            let progress = self.model_open.as_mut().unwrap();
            progress.model_open_result = Some(0);
            progress.phase = ModelCachePhase::Complete;
            return Ok(0);
        }
        while let Some(bytes) = path {
            let temporary = self
                .strings
                .create_null()
                .map_err(|e| MissionCacheError::Strings(e.into()))?;
            self.caches[1].pending_names.push(temporary);
            self.strings
                .assign(temporary, Some(&bytes), backend)
                .map_err(MissionCacheError::Strings)?;
            if self
                .strings
                .string(temporary)
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .rep
                .is_none()
            {
                return Err(ModelCacheOpenError::Unsupported {
                    entry: 0x408a40,
                    detail: "null concatenated model path requires native fault domain",
                });
            }
            let path_str =
                std::str::from_utf8(&bytes).map_err(|_| ModelCacheOpenError::Unsupported {
                    entry: 0x408a40,
                    detail: "model path outside original ASCII admission",
                })?;
            let result =
                match arena.open_original_static_cache_model(target, installation, path_str) {
                    Ok(_) => 0,
                    Err(failure) => match failure.error {
                        ModelError::NativeModelOpenResult(result) => result as i32,
                        // No invented NoFile/corrupt/OOM mapping for host adapter
                        // errors; destination and temporary path retain the prefix.
                        error => return Err(ModelCacheOpenError::Model(error)),
                    },
                };
            self.strings
                .destruct_string(temporary, backend)
                .map_err(MissionCacheError::Strings)?;
            self.caches[1].pending_names.retain(|&key| key != temporary);
            self.model_open.as_mut().unwrap().model_open_result = Some(result);
            if result < 0 {
                if [0x80000002u32 as i32, 0x8000000au32 as i32].contains(&result) {
                    return Ok(result);
                }
                self.model_open.as_mut().unwrap().directory += 1;
                path = self.next_model_directory()?;
                continue;
            }
            self.model_open.as_mut().unwrap().phase = ModelCachePhase::InsertEntry;
            let limit =
                u32::from_le_bytes(self.caches[1].header[0x24..0x28].try_into().unwrap()) as usize;
            if self.caches[1].items.len() >= limit {
                let mut evict = None;
                for (i, item) in self.caches[1].items.iter().enumerate() {
                    let model = item.model.ok_or(MissionCacheError::Interrupted)?;
                    if arena
                        .frame(model.root())
                        .map_err(ModelCacheOpenError::Model)?
                        .references
                        == 1
                    {
                        evict = Some(i);
                        break;
                    }
                }
                if let Some(index) = evict {
                    let model = self.caches[1].items[index].model.unwrap();
                    if let Err(error) = arena.release(model.root()) {
                        self.caches[1].interrupted = true;
                        return Err(ModelCacheOpenError::Model(error));
                    }
                    self.erase_model_item(index, backend)?;
                }
            }

            let count =
                u32::try_from(self.caches[1].items.len()).map_err(|_| MissionCacheError::Extent)?;
            let begin = self.header_word(1, 8);
            let end = self.header_word(1, 12);
            let capacity = self.header_word(1, 16);
            // Host-adopted items have no original vector layout. Materialize
            // every entry into a real owned array before publishing projection.
            if !self.caches[1].header_available || capacity.wrapping_sub(end) < 8 {
                let bytes = count
                    .checked_add(count.max(1))
                    .and_then(|n| n.checked_mul(8))
                    .ok_or(MissionCacheError::Extent)?;
                let pointer = backend
                    .allocate_cache_array(bytes)
                    .map_err(MissionCacheError::Array)?
                    .ok_or(MissionCacheError::ArrayAllocationUnavailable)?;
                self.caches[1].pending_arrays.push((pointer, bytes));
                if pointer == 0
                    || pointer.checked_add(bytes).is_none()
                    || self.strings.owns_allocation(u64::from(pointer))
                {
                    return Err(MissionCacheError::InvalidArrayAllocation.into());
                }
                let mut copies = Vec::new();
                for item in &self.caches[1].items {
                    let name = self
                        .strings
                        .copy_string(item.name)
                        .map_err(|e| MissionCacheError::Strings(e.into()))?;
                    self.caches[1].pending_names.push(name);
                    copies.push(CacheItem {
                        model: item.model,
                        name,
                    });
                }
                let key = self
                    .strings
                    .create_null()
                    .map_err(|e| MissionCacheError::Strings(e.into()))?;
                self.caches[1].pending_names.push(key);
                copies.push(CacheItem {
                    model: None,
                    name: key,
                });
                for item in &self.caches[1].items {
                    self.strings
                        .destruct_string(item.name, backend)
                        .map_err(MissionCacheError::Strings)?;
                }
                backend
                    .free_cache_array(begin)
                    .map_err(MissionCacheError::Array)?;
                put(&mut self.caches[1].header, 16, pointer + bytes);
                put(&mut self.caches[1].header, 8, pointer);
                put(&mut self.caches[1].header, 12, pointer + (count + 1) * 8);
                self.caches[1].items = copies;
                self.caches[1]
                    .pending_arrays
                    .retain(|&(base, _)| base != pointer);
                let names: Vec<_> = self.caches[1].items.iter().map(|i| i.name).collect();
                self.caches[1].pending_names.retain(|n| !names.contains(n));
                self.caches[1].header_available = true;
            } else {
                let key = self
                    .strings
                    .create_null()
                    .map_err(|e| MissionCacheError::Strings(e.into()))?;
                self.caches[1].items.push(CacheItem {
                    model: None,
                    name: key,
                });
            }
            // The unpublished object word remains distinct from native NULL.
            let key = self.caches[1].items.last().unwrap().name;
            let begin = self.header_word(1, 8);
            let count = self.caches[1].items.len() as u32;
            put(&mut self.caches[1].header, 12, begin + count * 8);
            self.strings
                .assign(key, Some(name), backend)
                .map_err(MissionCacheError::Strings)?;
            let cached = arena.create_model().map_err(ModelCacheOpenError::Model)?;
            self.model_open.as_mut().unwrap().cached_source = Some(cached);
            arena
                .duplicate_pristine_cache_model(cached, target)
                .map_err(ModelCacheOpenError::Model)?;
            self.caches[1].items.last_mut().unwrap().model = Some(cached);
            self.model_open.as_mut().unwrap().phase = ModelCachePhase::Complete;
            return Ok(0);
        }
        Ok(self
            .model_open
            .as_ref()
            .unwrap()
            .model_open_result
            .unwrap_or(0x80000001u32 as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_wildcard_star_ignores_remaining_pattern() {
        assert!(matches_name(b"Models\\A.i3d", b"mOdEls\\?.I3D"));
        assert!(matches_name(b"anything", b"*ignored"));
        assert!(!matches_name(b"", b"?"));
        assert!(!matches_name(b"ab", b"a"));
    }
    #[test]
    fn empty_cache_does_not_invent_default_directory() {
        let mut arena = ModelArena::default();
        let target = arena.create_model().unwrap();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        assert!(cache
            .begin_model_open(&arena, target, b"a.i3d", &mut memory)
            .unwrap()
            .is_none());
        assert_eq!(
            cache.model_open_progress().unwrap().model_open_result,
            Some(0x80000001u32 as i32)
        );
        assert_eq!(memory.live_allocations(), 0);
        arena.release(target.root()).unwrap();
    }
    #[test]
    fn directory_owner_and_foreign_heap_rejection_preserve_cleanup() {
        let mut arena = ModelArena::default();
        let target = arena.create_model().unwrap();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        cache
            .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
            .unwrap();
        assert_eq!(
            cache
                .begin_model_open(&arena, target, b"A.i3d", &mut memory)
                .unwrap(),
            Some(b"Models\\A.i3d".to_vec())
        );
        let live = memory.live_allocations();
        let mut foreign = CpuMissionCacheMemory::default();
        assert!(matches!(
            cache.begin_model_open(&arena, target, b"A.i3d", &mut foreign),
            Err(ModelCacheOpenError::Cache(MissionCacheError::ForeignDomain))
        ));
        assert_eq!(memory.live_allocations(), live);
        assert_eq!(foreign.live_allocations(), 0);
        cache
            .destruct(MissionCacheKind::Model, &mut arena, &mut memory)
            .unwrap();
        assert_eq!(memory.live_allocations(), 0);
        arena.release(target.root()).unwrap();
    }
    fn installation_fixture() -> (std::path::PathBuf, lhr_assets::Installation) {
        let path = std::env::temp_dir().join(format!(
            "lhr-cache-open-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(path.join("models")).unwrap();
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.push(6);
        bytes.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0x11, 4]);
        bytes.extend(b"part");
        bytes.push(0);
        bytes.extend([0; 24]);
        bytes.push(0);
        for name in ["a1", "a2", "a3"] {
            std::fs::write(path.join(format!("models/{name}.4ds")), &bytes).unwrap();
        }
        let installation = lhr_assets::Installation::open(&path).unwrap();
        (path, installation)
    }
    #[test]
    fn real_cache_miss_growth_hit_and_last_release_use_same_owners() {
        let (path, installation) = installation_fixture();
        let mut arena = ModelArena::default();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        cache
            .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
            .unwrap();
        let mut targets = Vec::new();
        for (i, name) in [b"a1.i3d", b"a2.i3d", b"a3.i3d"].iter().enumerate() {
            let target = arena.create_model().unwrap();
            arena
                .set_frame_name(target.root(), Some(b"target"))
                .unwrap();
            assert_eq!(
                cache
                    .open_static_model(&mut arena, &installation, target, *name, &mut memory)
                    .unwrap(),
                0
            );
            assert_eq!(cache.len(MissionCacheKind::Model), i + 1);
            assert_eq!(
                cache.model_open_progress().unwrap().phase,
                ModelCachePhase::Complete
            );
            assert_eq!(arena.model(target).unwrap().frames.len(), 1);
            let capacity = cache.header_word(1, 16) - cache.header_word(1, 8);
            assert_eq!(capacity, [8, 16, 32][i]);
            targets.push(target);
        }
        let hit = arena.create_model().unwrap();
        assert_eq!(
            cache
                .open_static_model(&mut arena, &installation, hit, b"A2.I3D", &mut memory)
                .unwrap(),
            0
        );
        assert_eq!(cache.len(MissionCacheKind::Model), 3);
        targets.push(hit);
        let transferred = cache.caches[1].items[0].model.unwrap();
        cache.erase_model_item(0, &mut memory).unwrap();
        assert_eq!(cache.len(MissionCacheKind::Model), 2);
        assert_eq!(arena.frame(transferred.root()).unwrap().references, 1);
        assert_eq!(
            cache
                .strings
                .rep(
                    cache
                        .strings
                        .string(cache.caches[1].items[0].name)
                        .unwrap()
                        .rep
                        .unwrap()
                )
                .unwrap()
                .bytes,
            b"a2.i3d"
        );
        targets.push(transferred);
        cache
            .destruct(MissionCacheKind::Model, &mut arena, &mut memory)
            .unwrap();
        assert_eq!(memory.live_allocations(), 0);
        for model in targets {
            arena.release(model.root()).unwrap();
        }
        assert_eq!(arena.live_frame_count(), 0);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn zero_limit_evicts_unpinned_but_all_pinned_grows_beyond_limit() {
        for pinned in [false, true] {
            let (path, installation) = installation_fixture();
            let mut arena = ModelArena::default();
            let mut cache = MissionCaches::empty().unwrap();
            let mut memory = CpuMissionCacheMemory::default();
            cache
                .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
                .unwrap();
            put(&mut cache.caches[1].header, 0x24, 0);
            let first = arena.create_model().unwrap();
            let second = arena.create_model().unwrap();
            cache
                .open_static_model(&mut arena, &installation, first, b"a1.i3d", &mut memory)
                .unwrap();
            let cached = cache.model_open_progress().unwrap().cached_source.unwrap();
            if pinned {
                arena.retain(cached.root()).unwrap();
            }
            cache
                .open_static_model(&mut arena, &installation, second, b"a2.i3d", &mut memory)
                .unwrap();
            assert_eq!(
                cache.len(MissionCacheKind::Model),
                if pinned { 2 } else { 1 }
            );
            assert_eq!(arena.frame(cached.root()).is_ok(), pinned);
            cache
                .destruct(MissionCacheKind::Model, &mut arena, &mut memory)
                .unwrap();
            assert_eq!(memory.live_allocations(), 0);
            if pinned {
                assert_eq!(arena.frame(cached.root()).unwrap().references, 1);
                arena.release(cached.root()).unwrap();
            }
            arena.release(first.root()).unwrap();
            arena.release(second.root()).unwrap();
            assert_eq!(arena.live_frame_count(), 0);
            std::fs::remove_dir_all(path).unwrap();
        }
    }
    struct FailingArrays {
        memory: CpuMissionCacheMemory,
        reject: bool,
        arrays: usize,
        strings: usize,
    }
    impl MissionMemoryDomain for FailingArrays {
        fn mission_memory_domain(&self) -> MissionMemoryDomainId {
            self.memory.mission_memory_domain()
        }
    }
    impl CStrBackend for FailingArrays {
        type Error = CacheMemoryError;
        fn allocate_string(
            &mut self,
            bytes: u32,
        ) -> Result<Option<crate::c_str::StringAllocation>, Self::Error> {
            self.strings += 1;
            self.memory.allocate_string(bytes)
        }
        fn free_string(&mut self, id: crate::c_str::StringAllocation) -> Result<(), Self::Error> {
            self.memory.free_string(id)
        }
    }
    impl MissionCacheAllocator for FailingArrays {
        fn allocate_cache_array(&mut self, bytes: u32) -> Result<Option<u32>, Self::Error> {
            self.arrays += 1;
            if self.reject {
                Ok(None)
            } else {
                self.memory.allocate_cache_array(bytes)
            }
        }
        fn free_cache_array(&mut self, id: u32) -> Result<(), Self::Error> {
            self.memory.free_cache_array(id)
        }
    }
    #[test]
    fn null_growth_after_target_load_retains_prefix_and_rejects_replay() {
        let (path, installation) = installation_fixture();
        let mut arena = ModelArena::default();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = FailingArrays {
            memory: CpuMissionCacheMemory::default(),
            reject: false,
            arrays: 0,
            strings: 0,
        };
        cache
            .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
            .unwrap();
        let first = arena.create_model().unwrap();
        let target = arena.create_model().unwrap();
        cache
            .open_static_model(&mut arena, &installation, first, b"a1.i3d", &mut memory)
            .unwrap();
        let old_cached = cache.model_open_progress().unwrap().cached_source.unwrap();
        let old_header = cache.caches[1].header;
        memory.reject = true;
        assert!(matches!(
            cache.open_static_model(&mut arena, &installation, target, b"a2.i3d", &mut memory),
            Err(ModelCacheOpenError::Cache(
                MissionCacheError::ArrayAllocationUnavailable
            ))
        ));
        assert!(cache.is_interrupted(MissionCacheKind::Model));
        assert_eq!(cache.len(MissionCacheKind::Model), 1);
        assert_eq!(cache.caches[1].header, old_header);
        assert_eq!(cache.caches[1].items[0].model, Some(old_cached));
        assert_eq!(arena.frame(old_cached.root()).unwrap().references, 1);
        let frames = arena.model(target).unwrap().frames.clone();
        assert_eq!(frames.len(), 1);
        let filename = arena.model_filename(target).unwrap();
        let epoch = arena.lifecycle_epoch(target).unwrap();
        let calls = (memory.arrays, memory.strings);
        let attempt = cache.model_open_progress().unwrap().attempt;
        memory.reject = false;
        assert!(matches!(
            cache.open_static_model(&mut arena, &installation, target, b"a2.i3d", &mut memory),
            Err(ModelCacheOpenError::Cache(MissionCacheError::Interrupted))
        ));
        assert_eq!((memory.arrays, memory.strings), calls);
        assert_eq!(arena.model(target).unwrap().frames, frames);
        assert_eq!(arena.model_filename(target).unwrap(), filename);
        assert_eq!(arena.lifecycle_epoch(target).unwrap(), epoch);
        assert_eq!(cache.model_open_progress().unwrap().attempt, attempt);
        // The interrupted original call deliberately retains its owners. No
        // native UnInit/recovery or successful last-release is invented here.
        assert!(memory.memory.live_allocations() > 0);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn rejected_admission_keeps_old_progress_usable_and_native_hresult_can_repeat() {
        let (path, installation) = installation_fixture();
        let mut arena = ModelArena::default();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        let target = arena.create_model().unwrap();
        assert_eq!(
            cache
                .open_static_model(&mut arena, &installation, target, b"a1.i3d", &mut memory)
                .unwrap(),
            0x80000001u32 as i32
        );
        let attempt = cache.model_open_progress().unwrap().attempt;
        let mut foreign = CpuMissionCacheMemory::default();
        assert!(matches!(
            cache.open_static_model(&mut arena, &installation, target, b"a1.i3d", &mut foreign),
            Err(ModelCacheOpenError::Cache(MissionCacheError::ForeignDomain))
        ));
        assert!(!cache.is_interrupted(MissionCacheKind::Model));
        assert_eq!(cache.model_open_progress().unwrap().attempt, attempt);
        assert!(matches!(
            cache.open_static_model(&mut arena, &installation, target, b"\xff", &mut memory),
            Err(ModelCacheOpenError::Unsupported {
                entry: 0x4088c9,
                ..
            })
        ));
        assert!(!cache.is_interrupted(MissionCacheKind::Model));
        assert_eq!(cache.model_open_progress().unwrap().attempt, attempt);
        assert_eq!(
            cache
                .open_static_model(&mut arena, &installation, target, b"a1.i3d", &mut memory)
                .unwrap(),
            0x80000001u32 as i32
        );
        assert!(!cache.is_interrupted(MissionCacheKind::Model));
        cache
            .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
            .unwrap();
        // A known original invalid-parameter HRESULT closes the destination,
        // returns normally, and remains eligible for the next genuine Open.
        assert_eq!(
            cache
                .open_static_model(&mut arena, &installation, target, b"a1.4ds", &mut memory)
                .unwrap(),
            0x80000007u32 as i32
        );
        assert!(!cache.is_interrupted(MissionCacheKind::Model));
        assert_eq!(
            cache
                .open_static_model(&mut arena, &installation, target, b"a1.i3d", &mut memory)
                .unwrap(),
            0
        );
        cache
            .destruct(MissionCacheKind::Model, &mut arena, &mut memory)
            .unwrap();
        arena.release(target.root()).unwrap();
        assert_eq!(memory.live_allocations(), 0);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn model_open_host_error_keeps_closed_target_prefix_and_cannot_replay() {
        let (path, installation) = installation_fixture();
        let mut arena = ModelArena::default();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = FailingArrays {
            memory: CpuMissionCacheMemory::default(),
            reject: false,
            arrays: 0,
            strings: 0,
        };
        cache
            .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
            .unwrap();
        let target = arena.create_model().unwrap();
        cache
            .open_static_model(&mut arena, &installation, target, b"a1.i3d", &mut memory)
            .unwrap();
        let old_child = arena.model(target).unwrap().frames[0];
        assert!(matches!(
            cache.open_static_model(
                &mut arena,
                &installation,
                target,
                b"missing.i3d",
                &mut memory
            ),
            Err(ModelCacheOpenError::Model(ModelError::Asset(_)))
        ));
        assert!(cache.is_interrupted(MissionCacheKind::Model));
        assert!(arena.model(target).unwrap().frames.is_empty());
        assert!(arena.frame(old_child).is_err());
        assert_eq!(cache.len(MissionCacheKind::Model), 1);
        assert_eq!(
            cache.model_open_progress().unwrap().phase,
            ModelCachePhase::ModelOpen
        );
        let epoch = arena.lifecycle_epoch(target).unwrap();
        let calls = (memory.arrays, memory.strings);
        let attempt = cache.model_open_progress().unwrap().attempt;
        assert!(matches!(
            cache.open_static_model(&mut arena, &installation, target, b"a1.i3d", &mut memory),
            Err(ModelCacheOpenError::Cache(MissionCacheError::Interrupted))
        ));
        assert_eq!(arena.lifecycle_epoch(target).unwrap(), epoch);
        assert_eq!((memory.arrays, memory.strings), calls);
        assert_eq!(cache.model_open_progress().unwrap().attempt, attempt);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn host_adoption_hit_then_miss_publishes_owned_array_and_destructs_all_storage() {
        let (path, installation) = installation_fixture();
        let mut arena = ModelArena::default();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        cache
            .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
            .unwrap();
        let source = arena.create_model().unwrap();
        arena
            .open_original_static_cache_model(source, &installation, "Models\\a1.i3d")
            .unwrap();
        cache
            .retain_model(source, b"existing.i3d", &mut arena, &mut memory)
            .unwrap();
        arena.release(source.root()).unwrap();
        assert!(cache.raw_header(MissionCacheKind::Model).is_none());
        let hit = arena.create_model().unwrap();
        cache
            .open_static_model(&mut arena, &installation, hit, b"existing.i3d", &mut memory)
            .unwrap();
        assert!(cache.raw_header(MissionCacheKind::Model).is_none());
        let target = arena.create_model().unwrap();
        cache
            .open_static_model(&mut arena, &installation, target, b"a2.i3d", &mut memory)
            .unwrap();
        assert_eq!(cache.len(MissionCacheKind::Model), 2);
        assert!(cache.raw_header(MissionCacheKind::Model).is_some());
        assert_eq!(cache.header_word(1, 12) - cache.header_word(1, 8), 16);
        assert_eq!(cache.header_word(1, 16) - cache.header_word(1, 8), 16);
        assert_eq!(arena.frame(source.root()).unwrap().references, 1);
        cache
            .destruct(MissionCacheKind::Model, &mut arena, &mut memory)
            .unwrap();
        assert_eq!(memory.live_allocations(), 0);
        assert_eq!(memory.live_bytes(), 0);
        arena.release(hit.root()).unwrap();
        arena.release(target.root()).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn native_owned_array_rejects_host_adoption_before_any_owner_effect() {
        let (path, installation) = installation_fixture();
        let mut arena = ModelArena::default();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        cache
            .add_dir(MissionCacheKind::Model, b"Models", &mut memory)
            .unwrap();
        let target = arena.create_model().unwrap();
        cache
            .open_static_model(&mut arena, &installation, target, b"a1.i3d", &mut memory)
            .unwrap();
        let donor = arena.create_model().unwrap();
        let header = cache.caches[1].header;
        let heap = (memory.live_allocations(), memory.live_bytes());
        let references = arena.frame(donor.root()).unwrap().references;
        let frames = arena.live_frame_count();
        assert!(matches!(
            cache.retain_model(donor, b"adopted.i3d", &mut arena, &mut memory),
            Err(MissionCacheError::HostAdoptionIntoNativeArray)
        ));
        assert_eq!(cache.caches[1].header, header);
        assert_eq!(cache.len(MissionCacheKind::Model), 1);
        assert!(cache.raw_header(MissionCacheKind::Model).is_some());
        assert!(cache.pending_names(MissionCacheKind::Model).is_empty());
        assert!(!cache.is_interrupted(MissionCacheKind::Model));
        assert_eq!((memory.live_allocations(), memory.live_bytes()), heap);
        assert_eq!(arena.frame(donor.root()).unwrap().references, references);
        assert_eq!(arena.live_frame_count(), frames);
        let next = arena.create_model().unwrap();
        cache
            .open_static_model(&mut arena, &installation, next, b"a2.i3d", &mut memory)
            .unwrap();
        cache
            .destruct(MissionCacheKind::Model, &mut arena, &mut memory)
            .unwrap();
        assert_eq!(memory.live_allocations(), 0);
        assert_eq!(memory.live_bytes(), 0);
        for model in [target, donor, next] {
            arena.release(model.root()).unwrap();
        }
        assert_eq!(arena.live_frame_count(), 0);
        std::fs::remove_dir_all(path).unwrap();
    }
}
