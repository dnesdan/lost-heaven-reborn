//! Source-backed Game sound cache Open for Scene2 filename field 0x10.
//! The inlined call path is in Game.exe SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! function 0x541030, branch 0x542db6..0x54307d. Native cached items own
//! real I3D_sound frames and derived source/voice state, not PCM data alone.
use super::*;
use crate::frame_sound::SoundKey;
use crate::runtime_audio::AudioError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoundCachePhase {
    Lookup,
    RemoveDestination,
    DuplicateHit,
    Directory,
    SoundOpen,
    InsertEntry,
    Complete,
}
#[derive(Debug)]
pub struct SoundCacheOpenProgress {
    pub target: SoundKey,
    pub name: Vec<u8>,
    pub flags: u32,
    pub phase: SoundCachePhase,
    pub directory: usize,
    pub path: Option<Vec<u8>>,
    pub cached_source: Option<SoundKey>,
    pub sound_open_result: Option<i32>,
    attempt: u64,
}
#[derive(Debug)]
pub enum SoundCacheOpenError<E> {
    Cache(MissionCacheError<E>),
    Model(ModelError),
    Audio(AudioError),
    Unsupported { entry: u32, detail: &'static str },
}
impl<E> From<MissionCacheError<E>> for SoundCacheOpenError<E> {
    fn from(error: MissionCacheError<E>) -> Self {
        Self::Cache(error)
    }
}
impl<E> From<ModelError> for SoundCacheOpenError<E> {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}
impl<E> From<AudioError> for SoundCacheOpenError<E> {
    fn from(error: AudioError) -> Self {
        Self::Audio(error)
    }
}
impl MissionCaches {
    pub fn sound_open_progress(&self) -> Option<&SoundCacheOpenProgress> {
        self.sound_open.as_ref()
    }
    fn erase_sound_item<B: CStrBackend>(
        &mut self,
        index: usize,
        backend: &mut B,
    ) -> Result<(), MissionCacheError<B::Error>> {
        let slot = MissionCacheKind::Sound.slot();
        let count = self.caches[slot].items.len();
        for at in index..count - 1 {
            let source = self.caches[slot].items[at + 1].name;
            let destination = self.caches[slot].items[at].name;
            self.caches[slot].items[at].resource = self.caches[slot].items[at + 1].resource;
            if let Err(error) = self.strings.alias_into(destination, source, backend) {
                self.caches[slot].interrupted = true;
                return Err(MissionCacheError::Strings(error));
            }
        }
        let tail = self.caches[slot].items.last().unwrap().name;
        if let Err(error) = self.strings.destruct_string(tail, backend) {
            self.caches[slot].interrupted = true;
            return Err(MissionCacheError::Strings(error));
        }
        self.caches[slot].items.pop();
        let begin = self.header_word(slot, 8);
        put(
            &mut self.caches[slot].header,
            12,
            begin + (count as u32 - 1) * 8,
        );
        Ok(())
    }
    fn begin_sound_open<B: CStrBackend + MissionMemoryDomain>(
        &mut self,
        arena: &ModelArena,
        target: SoundKey,
        name: &[u8],
        backend: &mut B,
    ) -> Result<Option<Vec<u8>>, SoundCacheOpenError<B::Error>> {
        self.bind(arena, backend)?;
        let slot = MissionCacheKind::Sound.slot();
        if self.caches[slot].destructed {
            return Err(MissionCacheError::Destructed.into());
        }
        if self.caches[slot].interrupted {
            return Err(MissionCacheError::Interrupted.into());
        }
        arena.sound(target)?;
        let name = &name[..name.iter().position(|&x| x == 0).unwrap_or(name.len())];
        if !name.is_ascii() {
            return Err(SoundCacheOpenError::Unsupported {
                entry: 0x542e9a,
                detail: "sound cache comparison requires original non-ASCII CRT locale",
            });
        }
        let attempt = self
            .sound_open
            .as_ref()
            .map_or(0, |progress| progress.attempt.wrapping_add(1));
        self.sound_open = Some(SoundCacheOpenProgress {
            target,
            name: name.to_vec(),
            flags: 0x10,
            phase: SoundCachePhase::Lookup,
            directory: 0,
            path: None,
            cached_source: None,
            sound_open_result: None,
            attempt,
        });
        if self.caches[slot]
            .items
            .iter()
            .any(|item| item.sound().is_none())
        {
            return Err(MissionCacheError::Interrupted.into());
        }
        if let Some(index) = self.caches[slot]
            .items
            .iter()
            .rposition(|item| item.sound() == Some(target.frame()))
        {
            self.sound_open.as_mut().unwrap().phase = SoundCachePhase::RemoveDestination;
            // The cache removes its own destination item without Release.
            self.erase_sound_item(index, backend)?;
        }
        for index in 0..self.caches[slot].items.len() {
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
            let cached_rep = self
                .strings
                .string(key)
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .rep;
            let cached = cached_rep
                .map(|rep| self.strings.rep(rep).map(|rep| rep.bytes.as_slice()))
                .transpose()
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .unwrap_or(&[]);
            if !cached.is_ascii() {
                return Err(SoundCacheOpenError::Unsupported {
                    entry: 0x542e9a,
                    detail: "cached sound name requires original non-ASCII CRT locale",
                });
            }
            let request_rep = self
                .strings
                .string(temporary)
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .rep;
            let request = request_rep
                .map(|rep| self.strings.rep(rep).map(|rep| rep.bytes.as_slice()))
                .transpose()
                .map_err(|e| MissionCacheError::Strings(e.into()))?
                .unwrap_or(&[]);
            let hit = model_cache::matches_name(cached, request);
            self.strings
                .destruct_string(temporary, backend)
                .map_err(MissionCacheError::Strings)?;
            self.caches[slot]
                .pending_names
                .retain(|&key| key != temporary);
            if hit {
                let frame = self.caches[slot].items[index]
                    .sound()
                    .ok_or(MissionCacheError::Interrupted)?;
                let source = arena.sound_key(frame)?;
                let progress = self.sound_open.as_mut().unwrap();
                progress.phase = SoundCachePhase::DuplicateHit;
                progress.cached_source = Some(source);
                return Ok(None);
            }
        }
        self.next_sound_directory()
    }
    fn next_sound_directory<E>(&mut self) -> Result<Option<Vec<u8>>, SoundCacheOpenError<E>> {
        let progress = self.sound_open.as_mut().expect("sound Open entered");
        progress.phase = SoundCachePhase::Directory;
        let dirs = &self.caches[MissionCacheKind::Sound.slot()].dirs;
        let Some(&key) = dirs.get(progress.directory) else {
            progress
                .sound_open_result
                .get_or_insert(0x8000_0001u32 as i32);
            progress.phase = SoundCachePhase::Complete;
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
        progress.phase = SoundCachePhase::SoundOpen;
        Ok(Some(path))
    }
    /// One entered native cache operation. A host Err interrupts the cache;
    /// original HRESULTs complete normally and preserve the executed prefix.
    pub fn open_static_sound<B: MissionCacheAllocator>(
        &mut self,
        arena: &mut ModelArena,
        installation: &lhr_assets::Installation,
        target: SoundKey,
        name: &[u8],
        backend: &mut B,
    ) -> Result<i32, SoundCacheOpenError<B::Error>> {
        let previous_attempt = self.sound_open.as_ref().map(|progress| progress.attempt);
        let result = self.open_static_sound_inner(arena, installation, target, name, backend);
        let entered = self.sound_open.as_ref().map(|progress| progress.attempt) != previous_attempt;
        if entered && result.is_err() {
            self.caches[MissionCacheKind::Sound.slot()].interrupted = true;
        }
        result
    }
    fn open_static_sound_inner<B: MissionCacheAllocator>(
        &mut self,
        arena: &mut ModelArena,
        installation: &lhr_assets::Installation,
        target: SoundKey,
        name: &[u8],
        backend: &mut B,
    ) -> Result<i32, SoundCacheOpenError<B::Error>> {
        let audio = arena.host_sound_backend()?.audio.clone();
        let mut path = self.begin_sound_open(arena, target, name, backend)?;
        if self.sound_open.as_ref().unwrap().phase == SoundCachePhase::DuplicateHit {
            let source = self.sound_open.as_ref().unwrap().cached_source.unwrap();
            let _open_result =
                audio.duplicate_sound_derived(target.frame().0, source.frame().0, installation)?;
            arena.duplicate_sound_frame_base(target, source)?;
            let progress = self.sound_open.as_mut().unwrap();
            progress.sound_open_result = Some(0);
            progress.phase = SoundCachePhase::Complete;
            return Ok(0);
        }
        let slot = MissionCacheKind::Sound.slot();
        while let Some(bytes) = path {
            let temporary = self
                .strings
                .create_null()
                .map_err(|e| MissionCacheError::Strings(e.into()))?;
            self.caches[slot].pending_names.push(temporary);
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
                return Err(SoundCacheOpenError::Unsupported {
                    entry: 0x542f2f,
                    detail: "null concatenated sound path requires native fault domain",
                });
            }
            let path_str =
                std::str::from_utf8(&bytes).map_err(|_| SoundCacheOpenError::Unsupported {
                    entry: 0x542f2f,
                    detail: "sound path outside original ASCII admission",
                })?;
            let result =
                audio.open_original_sound(target.frame().0, path_str, 0x10, installation)?;
            self.strings
                .destruct_string(temporary, backend)
                .map_err(MissionCacheError::Strings)?;
            self.caches[slot]
                .pending_names
                .retain(|&key| key != temporary);
            self.sound_open.as_mut().unwrap().sound_open_result = Some(result);
            if result < 0 {
                if [0x8000_0002u32 as i32, 0x8000_000au32 as i32].contains(&result) {
                    self.sound_open.as_mut().unwrap().phase = SoundCachePhase::Complete;
                    return Ok(result);
                }
                self.sound_open.as_mut().unwrap().directory += 1;
                path = self.next_sound_directory()?;
                continue;
            }
            self.sound_open.as_mut().unwrap().phase = SoundCachePhase::InsertEntry;
            let limit = u32::from_le_bytes(self.caches[slot].header[0x24..0x28].try_into().unwrap())
                as usize;
            if self.caches[slot].items.len() >= limit {
                let mut evict = None;
                for (index, item) in self.caches[slot].items.iter().enumerate() {
                    let frame = item.sound().ok_or(MissionCacheError::Interrupted)?;
                    if arena.frame(frame)?.references == 1 {
                        evict = Some(index);
                        break;
                    }
                }
                if let Some(index) = evict {
                    let frame = self.caches[slot].items[index].sound().unwrap();
                    arena.release(frame)?;
                    self.erase_sound_item(index, backend)?;
                }
            }
            self.append_empty_sound_item(backend)?;
            let item_name = self.caches[slot].items.last().unwrap().name;
            self.strings
                .assign(item_name, Some(name), backend)
                .map_err(MissionCacheError::Strings)?;
            // Native virtual CreateNew calls the same driver's type-4 factory.
            // A failed registration leaves a real pending creator reference.
            let cached = match arena.create_owned_sound_frame() {
                Ok(cached) => cached,
                Err(failure) => {
                    self.sound_open.as_mut().unwrap().cached_source = failure.frame;
                    return Err(SoundCacheOpenError::Model(failure.error));
                }
            };
            self.sound_open.as_mut().unwrap().cached_source = Some(cached);
            let _copy_open =
                audio.duplicate_sound_derived(cached.frame().0, target.frame().0, installation)?;
            arena.duplicate_sound_frame_base(cached, target)?;
            self.caches[slot].items.last_mut().unwrap().resource =
                Some(CacheResource::Sound(cached.frame()));
            self.sound_open.as_mut().unwrap().phase = SoundCachePhase::Complete;
            return Ok(result);
        }
        Ok(self
            .sound_open
            .as_ref()
            .unwrap()
            .sound_open_result
            .unwrap_or(0x8000_0001u32 as i32))
    }
    /// Native vector growth copies each live name owner, appends a null
    /// unfinished item, then publishes end/capacity words before assignment.
    fn append_empty_sound_item<B: MissionCacheAllocator>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), SoundCacheOpenError<B::Error>> {
        let slot = MissionCacheKind::Sound.slot();
        let count =
            u32::try_from(self.caches[slot].items.len()).map_err(|_| MissionCacheError::Extent)?;
        let begin = self.header_word(slot, 8);
        let end = self.header_word(slot, 12);
        let capacity = self.header_word(slot, 16);
        if !self.caches[slot].header_available || capacity.wrapping_sub(end) < 8 {
            let bytes = count
                .checked_add(count.max(1))
                .and_then(|n| n.checked_mul(8))
                .ok_or(MissionCacheError::Extent)?;
            let pointer = backend
                .allocate_cache_array(bytes)
                .map_err(MissionCacheError::Array)?
                .ok_or(MissionCacheError::ArrayAllocationUnavailable)?;
            self.caches[slot].pending_arrays.push((pointer, bytes));
            if pointer == 0
                || pointer.checked_add(bytes).is_none()
                || self.strings.owns_allocation(u64::from(pointer))
            {
                return Err(MissionCacheError::InvalidArrayAllocation.into());
            }
            let mut copies = Vec::new();
            for item in &self.caches[slot].items {
                let name = self
                    .strings
                    .copy_string(item.name)
                    .map_err(|e| MissionCacheError::Strings(e.into()))?;
                self.caches[slot].pending_names.push(name);
                copies.push(CacheItem {
                    resource: item.resource,
                    name,
                });
            }
            let key = self
                .strings
                .create_null()
                .map_err(|e| MissionCacheError::Strings(e.into()))?;
            self.caches[slot].pending_names.push(key);
            copies.push(CacheItem {
                resource: None,
                name: key,
            });
            for item in &self.caches[slot].items {
                self.strings
                    .destruct_string(item.name, backend)
                    .map_err(MissionCacheError::Strings)?;
            }
            backend
                .free_cache_array(begin)
                .map_err(MissionCacheError::Array)?;
            put(&mut self.caches[slot].header, 16, pointer + bytes);
            put(&mut self.caches[slot].header, 8, pointer);
            put(&mut self.caches[slot].header, 12, pointer + (count + 1) * 8);
            self.caches[slot].items = copies;
            self.caches[slot]
                .pending_arrays
                .retain(|&(base, _)| base != pointer);
            let names: Vec<_> = self.caches[slot]
                .items
                .iter()
                .map(|item| item.name)
                .collect();
            self.caches[slot]
                .pending_names
                .retain(|name| !names.contains(name));
            self.caches[slot].header_available = true;
        } else {
            let key = self
                .strings
                .create_null()
                .map_err(|e| MissionCacheError::Strings(e.into()))?;
            self.caches[slot].items.push(CacheItem {
                resource: None,
                name: key,
            });
            let begin = self.header_word(slot, 8);
            let new_count = self.caches[slot].items.len() as u32;
            put(&mut self.caches[slot].header, 12, begin + new_count * 8);
        }
        Ok(())
    }
}
