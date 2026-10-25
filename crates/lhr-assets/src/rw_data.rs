//! rw_data.dll dtaSeek (PC RVA 0x3730), dtaClose (PC RVA 0x23e0),
//! dtaRead (PC RVA 0x25c0), DeCrypt (PC RVA 0x4690), and bounded native
//! archive registration/open lifecycle (see `open`).
//!
//! Snapshot admission, copying and release below are host adapters, not dtaOpen,
//! dtaRead or dtaClose reconstructions. Native lifecycle methods use OpenEffects
//! and share the read/seek/close state; snapshot adapters remain separate. The mutex
//! models serialized calls, excluding Win32 recursive critical-section reentry.
use std::{collections::BTreeSet, sync::Mutex};

/// Native failure result; handle zero is valid.
pub const FAILURE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadCursor {
    size: u32,
    position: u32,
    block_file_pointer: u32,
}
impl ReadCursor {
    pub fn new(size: u32) -> Self {
        Self {
            size,
            position: 0,
            block_file_pointer: 0,
        }
    }
    /// Host inspection; no corresponding native tell/size export is implied.
    pub fn position(&self) -> u32 {
        self.position
    }
    pub fn size(&self) -> u32 {
        self.size
    }
    pub fn block_file_pointer(&self) -> u32 {
        self.block_file_pointer
    }

    /// Read branch of the original dtaSeek. Method 2 intentionally admits EOF+1.
    /// All arithmetic and signed tests use the original 32-bit domain.
    pub fn dta_seek(&mut self, distance: i32, method: u32) -> u32 {
        let position = match method {
            0 if distance >= 0 && distance as u32 <= self.size => distance as u32,
            1 => {
                let next = self.position.wrapping_add(distance as u32);
                if (next as i32) < 0 || next > self.size {
                    return FAILURE;
                }
                next
            }
            2 => {
                let next = self.size.wrapping_add(distance as u32);
                if distance > 1 || (next as i32) < 0 {
                    return FAILURE;
                }
                next
            }
            _ => return FAILURE,
        };
        self.position = position;
        self.block_file_pointer = position % 32768;
        position
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorError {
    LockPoisoned,
    SnapshotTooLarge,
    HandleSpaceExhausted,
    InvalidHandle,
    /// Native method 2 can leave the cursor beyond EOF. Snapshot reads exclude
    /// that native unsafe domain rather than inventing a read result for it.
    BeyondEnd,
}
struct Snapshot {
    cursor: ReadCursor,
    bytes: Vec<u8>,
}

/// Serialized reusable zero-based handle storage for durable byte snapshots.
/// The mutex covers lookup, cursor mutation and copying, including rejected
/// seeks. Write handles are not admitted by this bounded read-mode API.
#[derive(Default)]
pub struct ReadHandles {
    slots: Mutex<Vec<Option<Snapshot>>>,
}
impl ReadHandles {
    /// Host admission of already decoded bytes; performs no native open work.
    pub fn insert_snapshot(&self, bytes: Vec<u8>) -> Result<u32, CursorError> {
        let size = u32::try_from(bytes.len()).map_err(|_| CursorError::SnapshotTooLarge)?;
        let mut slots = self.slots.lock().map_err(|_| CursorError::LockPoisoned)?;
        let snapshot = Snapshot {
            cursor: ReadCursor::new(size),
            bytes,
        };
        if let Some(index) = slots.iter().position(Option::is_none) {
            slots[index] = Some(snapshot);
            return Ok(index as u32);
        }
        let handle = u32::try_from(slots.len())
            .ok()
            .filter(|&h| h != FAILURE)
            .ok_or(CursorError::HandleSpaceExhausted)?;
        slots.push(Some(snapshot));
        Ok(handle)
    }
    /// Read-mode dtaSeek handle validation and cursor behavior. Out-of-range and
    /// released slots return the native failure word without changing storage.
    pub fn dta_seek(&self, handle: u32, distance: i32, method: u32) -> Result<u32, CursorError> {
        let mut slots = self.slots.lock().map_err(|_| CursorError::LockPoisoned)?;
        Ok(
            match slots.get_mut(handle as usize).and_then(Option::as_mut) {
                Some(snapshot) => snapshot.cursor.dta_seek(distance, method),
                None => FAILURE,
            },
        )
    }
    pub fn inspect(&self, handle: u32) -> Result<ReadCursor, CursorError> {
        let slots = self.slots.lock().map_err(|_| CursorError::LockPoisoned)?;
        slots
            .get(handle as usize)
            .and_then(Option::as_ref)
            .map(|s| s.cursor)
            .ok_or(CursorError::InvalidHandle)
    }
    /// Host snapshot copy, preserving the destination tail on short EOF reads.
    /// This does not model native physical I/O errors or block-cache mutations.
    pub fn read_snapshot(&self, handle: u32, destination: &mut [u8]) -> Result<usize, CursorError> {
        let mut slots = self.slots.lock().map_err(|_| CursorError::LockPoisoned)?;
        let snapshot = slots
            .get_mut(handle as usize)
            .and_then(Option::as_mut)
            .ok_or(CursorError::InvalidHandle)?;
        let start = snapshot.cursor.position as usize;
        let source = snapshot.bytes.get(start..).ok_or(CursorError::BeyondEnd)?;
        let count = source.len().min(destination.len());
        destination[..count].copy_from_slice(&source[..count]);
        snapshot.cursor.position += count as u32;
        snapshot.cursor.block_file_pointer = snapshot.cursor.position % 32768;
        Ok(count)
    }
    /// Host snapshot release. A valid empty slot succeeds; invalid indices fail.
    /// There is no OS handle here and this does not reconstruct dtaClose.
    pub fn release_snapshot(&self, handle: u32) -> Result<bool, CursorError> {
        let mut slots = self.slots.lock().map_err(|_| CursorError::LockPoisoned)?;
        match slots.get_mut(handle as usize) {
            Some(slot) => {
                *slot = None;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

/// Required native effects. Resource values are identities in this backend's
/// storage; zero is null for allocations and u32::MAX is an invalid file.
/// Callbacks run under the table lock and must not reenter it. Err is a host
/// interruption, possibly after an effect; no rollback or retry is implied.
pub trait Effects {
    type Error;
    fn set_file_pointer(
        &mut self,
        file: u32,
        distance: i32,
        method: u32,
    ) -> Result<u32, Self::Error>;
    fn close_handle(&mut self, file: u32) -> Result<bool, Self::Error>;
    fn delete_file(&mut self, name: &[u8]) -> Result<bool, Self::Error>;
    fn file_attributes(&mut self, name: &[u8]) -> Result<u32, Self::Error>;
    fn set_file_attributes(&mut self, name: &[u8], attributes: u32) -> Result<bool, Self::Error>;
    fn move_file(&mut self, from: &[u8], to: &[u8]) -> Result<bool, Self::Error>;
    fn free_name(&mut self, allocation: u64) -> Result<(), Self::Error>;
    fn free_block(&mut self, allocation: u64) -> Result<(), Self::Error>;
    fn free_wav(&mut self, allocation: u64) -> Result<(), Self::Error>;
}

/// Admitted projection of a native handle record. Allocation identities and
/// filename bytes must agree with the required backend's actual storage.
/// Allocation identities are unique lifetime tokens, never recycled addresses;
/// admission excludes aliases and re-admission of freed allocations. File IDs
/// may be shared only by borrowed archive records. Buffered filesystem names
/// must be nonnull, NUL-free and <=511 bytes; rename names also require >=4.
/// The host must supply live resources; admission does not probe backend memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleRecord {
    pub file: u32,
    pub flags: u32,
    pub cursor: ReadCursor,
    pub cache_valid: bool,
    pub temporary_name: u64,
    /// Bytes before the native terminating NUL; buffered rename requires >=4.
    pub temporary_name_bytes: Vec<u8>,
    pub block: u64,
    pub wav: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum OperationError<E> {
    LockPoisoned,
    InvalidRecord,
    Interrupted(EffectStage),
    HandleSpaceExhausted,
    Backend(E),
    UnsupportedRead,
    UnsafeReadState,
    UnsupportedOpen,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectStage {
    Seek,
    Read,
    Write,
    CopyFile,
    CloseHandle,
    DeleteFile,
    FileAttributes,
    SetFileAttributes,
    MoveFile,
    FreeName,
    FreeBlock,
    FreeWav,
    Open,
    Allocate,
    FreeOwner,
}
struct NativeState<B> {
    records: Vec<HandleRecord>,
    read_caches: Vec<Option<LooseReadCache>>,
    archive_caches: Vec<Option<ArchiveReadCache>>,
    effects: B,
    interrupted: Option<EffectStage>,
    admitted_allocations: BTreeSet<u64>,
    owners: Vec<open::Owner>,
    dta_first_force: bool,
}
/// Shared serialized storage for original dtaSeek and dtaClose. Admission is a
/// host operation, not dtaOpen. Closed slots, including slot zero, are reused.
pub struct Handles<B> {
    state: Mutex<NativeState<B>>,
}
impl<B: Effects> Handles<B> {
    pub fn new(effects: B) -> Self {
        Self {
            state: Mutex::new(NativeState {
                records: Vec::new(),
                read_caches: Vec::new(),
                archive_caches: Vec::new(),
                effects,
                interrupted: None,
                admitted_allocations: BTreeSet::new(),
                owners: Vec::new(),
                dta_first_force: false,
            }),
        }
    }
    pub fn admit(&self, record: HandleRecord) -> Result<u32, OperationError<B::Error>> {
        if record.file == FAILURE
            || (record.flags & 5 == 5
                && (record.temporary_name == 0 || record.temporary_name_bytes.len() > 511))
            || (record.flags & 5 == 5
                && record.flags & 0x80 == 0
                && (record.temporary_name_bytes.len() < 4
                    || record.temporary_name_bytes.len() > 511))
            || record.temporary_name_bytes.contains(&0)
        {
            return Err(OperationError::InvalidRecord);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = state.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let allocations = [record.temporary_name, record.block, record.wav];
        for (i, allocation) in allocations.iter().enumerate() {
            if *allocation != 0
                && (allocations[..i].contains(allocation)
                    || state.admitted_allocations.contains(allocation))
            {
                return Err(OperationError::InvalidRecord);
            }
        }
        if state.records.iter().any(|r| {
            r.file != FAILURE && r.file == record.file && (r.flags | record.flags) & 1 != 0
        }) {
            return Err(OperationError::InvalidRecord);
        }
        state
            .admitted_allocations
            .extend(allocations.into_iter().filter(|&v| v != 0));
        if let Some(index) = state.records.iter().position(|r| r.file == FAILURE) {
            state.records[index] = record;
            state.read_caches[index] = None;
            state.archive_caches[index] = None;
            return Ok(index as u32);
        }
        let index = u32::try_from(state.records.len())
            .ok()
            .filter(|&v| v != FAILURE)
            .ok_or(OperationError::HandleSpaceExhausted)?;
        state.records.push(record);
        state.read_caches.push(None);
        state.archive_caches.push(None);
        Ok(index)
    }
    pub fn inspect(&self, handle: u32) -> Result<Option<HandleRecord>, OperationError<B::Error>> {
        let state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        Ok(state.records.get(handle as usize).cloned())
    }
    /// Original dtaSeek, RVA 0x3730. Write-mode failures deliberately fall
    /// through BEGIN -> CURRENT -> END, storing every OS result before testing.
    pub fn dta_seek(
        &self,
        handle: u32,
        distance: i32,
        method: u32,
    ) -> Result<u32, OperationError<B::Error>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = state.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let mut failed_stage = None;
        let result = (|| {
            let NativeState {
                records, effects, ..
            } = &mut *state;
            let Some(record) = records
                .get_mut(handle as usize)
                .filter(|r| r.file != FAILURE)
            else {
                return Ok(FAILURE);
            };
            if record.flags & 2 == 0 {
                return Ok(record.cursor.dta_seek(distance, method));
            }
            if method > 2 {
                return Ok(FAILURE);
            }
            for origin in method..=2 {
                record.cursor.position = effects
                    .set_file_pointer(record.file, distance, origin)
                    .map_err(|error| {
                        failed_stage = Some(EffectStage::Seek);
                        OperationError::Backend(error)
                    })?;
                if record.cursor.position != FAILURE {
                    record.cursor.block_file_pointer = record.cursor.position & 0x7fff;
                    return Ok(record.cursor.position);
                }
            }
            Ok(FAILURE)
        })();
        if result.is_err() {
            state.interrupted = failed_stage;
        }
        result
    }
    /// Original dtaClose, RVA 0x23e0. Returns the defined AL boolean. Backend
    /// interruption preserves the completed prefix and disables further mutations
    /// to prevent retrying completed frees; it is not a native false.
    pub fn dta_close(&self, handle: u32) -> Result<bool, OperationError<B::Error>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = state.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let mut failed_stage = None;
        let result = (|| {
            let NativeState {
                records, effects, ..
            } = &mut *state;
            let Some(record) = records.get_mut(handle as usize) else {
                return Ok(false);
            };
            if record.file == FAILURE {
                return Ok(true);
            }
            if record.flags & 1 != 0 {
                if !effects.close_handle(record.file).map_err(|error| {
                    failed_stage = Some(EffectStage::CloseHandle);
                    OperationError::Backend(error)
                })? {
                    return Ok(false);
                }
                if record.flags & 4 != 0 {
                    let name = &record.temporary_name_bytes;
                    if record.flags & 0x80 != 0 {
                        effects.delete_file(name).map_err(|error| {
                            failed_stage = Some(EffectStage::DeleteFile);
                            OperationError::Backend(error)
                        })?;
                    } else {
                        let destination = &name[..name.len() - 4];
                        if !effects.delete_file(destination).map_err(|error| {
                            failed_stage = Some(EffectStage::DeleteFile);
                            OperationError::Backend(error)
                        })? {
                            let attributes =
                                effects.file_attributes(destination).map_err(|error| {
                                    failed_stage = Some(EffectStage::FileAttributes);
                                    OperationError::Backend(error)
                                })?;
                            if attributes != FAILURE {
                                effects
                                    .set_file_attributes(destination, attributes & !1)
                                    .map_err(|error| {
                                        failed_stage = Some(EffectStage::SetFileAttributes);
                                        OperationError::Backend(error)
                                    })?;
                                effects.delete_file(destination).map_err(|error| {
                                    failed_stage = Some(EffectStage::DeleteFile);
                                    OperationError::Backend(error)
                                })?;
                            }
                        }
                        effects.move_file(name, destination).map_err(|error| {
                            failed_stage = Some(EffectStage::MoveFile);
                            OperationError::Backend(error)
                        })?;
                    }
                }
            }
            if record.temporary_name != 0 {
                effects.free_name(record.temporary_name).map_err(|error| {
                    failed_stage = Some(EffectStage::FreeName);
                    OperationError::Backend(error)
                })?;
            }
            if record.block != 0 {
                effects.free_block(record.block).map_err(|error| {
                    failed_stage = Some(EffectStage::FreeBlock);
                    OperationError::Backend(error)
                })?;
            }
            if record.wav != 0 {
                effects.free_wav(record.wav).map_err(|error| {
                    failed_stage = Some(EffectStage::FreeWav);
                    OperationError::Backend(error)
                })?;
            }
            record.wav = 0;
            record.block = 0;
            record.cache_valid = false;
            record.file = FAILURE;
            // Native leaves temporary_name stale; file invalidation prevents reuse.
            Ok(true)
        })();
        if result.is_err() {
            state.interrupted = failed_stage;
        } else if matches!(result, Ok(true))
            && let Some(cache) = state.read_caches.get_mut(handle as usize)
        {
            *cache = None;
            state.archive_caches[handle as usize] = None;
        }
        result
    }
}

/// Native ReadFile return and out-count are independent, including on failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadFileResult {
    pub success: bool,
    pub bytes_read: u32,
}
/// Read-only native effects. A failed seek is only FAILURE with nonzero last_error.
pub trait ReadEffects: Effects {
    fn seek_read_file(&mut self, file: u32, position: u32) -> Result<(u32, u32), Self::Error>;
    fn read_file(&mut self, file: u32, output: &mut [u8]) -> Result<ReadFileResult, Self::Error>;
}
/// Host admission of a loose file's live 32 KiB allocation and numeric cache state.
/// Bytes include the entire allocation, even when only block_size bytes are valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LooseReadCache {
    pub bytes: Vec<u8>,
    pub block_start_file_pointer: u32,
    pub block_size: u32,
}
/// Host projection of a live archive block allocation and native read locals.
/// Admission is not dtaOpen. The backend owns the shared physical file position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveReadCache {
    /// Saved native 44-byte WAV allocation, retained until close/unregister.
    pub wav_header: Option<[u8; 44]>,
    pub bytes: Vec<u8>,
    pub data_offset: u32,
    pub block_start_file_pointer: u32,
    pub current_block: u32,
    pub block_count: u32,
    pub compressed_block_size: u32,
    pub previous_compressed_block_size: u32,
    pub block_size: u32,
    pub encrypted: bool,
    pub key: u64,
}
impl<B: ReadEffects> Handles<B> {
    pub fn attach_archive_cache(
        &self,
        handle: u32,
        cache: ArchiveReadCache,
    ) -> Result<(), OperationError<B::Error>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = state.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let record = state
            .records
            .get(handle as usize)
            .ok_or(OperationError::InvalidRecord)?;
        if record.file == FAILURE
            || record.flags & 3 != 0
            || record.block == 0
            || cache.wav_header.is_some() != (record.wav != 0)
            || cache.bytes.len() != 32768
            || cache.block_size > 32768
            || cache.compressed_block_size > 32769
            || cache.previous_compressed_block_size > 65535
            || cache.block_count == 0
            || cache.current_block >= cache.block_count
        {
            return Err(OperationError::InvalidRecord);
        }
        state.archive_caches[handle as usize] = Some(cache);
        Ok(())
    }
    pub fn inspect_archive_cache(
        &self,
        handle: u32,
    ) -> Result<Option<ArchiveReadCache>, OperationError<B::Error>> {
        let state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        Ok(state
            .archive_caches
            .get(handle as usize)
            .and_then(Clone::clone))
    }
    /// Attach native cache state to an admitted record. This is not dtaOpen.
    pub fn attach_loose_cache(
        &self,
        handle: u32,
        cache: LooseReadCache,
    ) -> Result<(), OperationError<B::Error>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = state.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let record = state
            .records
            .get(handle as usize)
            .ok_or(OperationError::InvalidRecord)?;
        if record.file == FAILURE
            || record.flags & 1 == 0
            || record.block == 0
            || cache.bytes.len() != 32768
            || cache.block_size > 32768
        {
            return Err(OperationError::InvalidRecord);
        }
        state.read_caches[handle as usize] = Some(cache);
        Ok(())
    }
    /// Read-mode dtaRead, PC RVA 0x25c0, including bounded PCM16 mono/stereo WAV.
    /// Other WAV channel layouts and the non-PCM16 fallback remain unsupported.
    /// None represents a null native buffer, including for a zero-length call.
    /// Host-unsafe cursor/cache ranges and malformed archive blocks return errors.
    /// Errors may preserve completed I/O and partial output/cache mutations;
    /// numeric cache and cursor locals commit only on success.
    /// Native I/O failure returns FAILURE with its completed output/cache prefix.
    pub fn dta_read(
        &self,
        handle: u32,
        output: Option<&mut [u8]>,
    ) -> Result<u32, OperationError<B::Error>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = state.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let Some(output) = output else {
            return Ok(FAILURE);
        };
        let Some(record) = state.records.get(handle as usize) else {
            return Ok(FAILURE);
        };
        if record.file == FAILURE || record.flags & 2 != 0 {
            return Ok(FAILURE);
        }
        if record.flags & 1 == 0 {
            return archive_read(&mut state, handle as usize, output);
        }
        if output.len() > u32::MAX as usize || record.cursor.position > record.cursor.size {
            return Err(OperationError::UnsafeReadState);
        }
        let Some(cache) = state.read_caches[handle as usize].as_ref() else {
            return Err(OperationError::UnsupportedRead);
        };
        let mut start = cache.block_start_file_pointer;
        let mut block_size = cache.block_size;
        let mut fp = record.cursor.position;
        let mut bp = record.cursor.block_file_pointer;
        let size = record.cursor.size;
        let file = record.file;
        let refill = !record.cache_valid || start > fp || start.wrapping_add(block_size) < fp;
        if !refill
            && (start > size
                || block_size > size - start
                || bp > block_size
                || start.checked_add(block_size).is_none()
                || block_size > 32768)
        {
            return Err(OperationError::UnsafeReadState);
        }
        let len = (output.len() as u32).min(size - fp) as usize;
        let mut failed_stage = None;
        let result = (|| {
            let NativeState {
                records,
                read_caches,
                effects,
                ..
            } = &mut *state;
            let record = &mut records[handle as usize];
            let cache = read_caches[handle as usize].as_mut().unwrap();
            macro_rules! seek {
                ($position:expr) => {{
                    let (position, error) =
                        effects.seek_read_file(file, $position).map_err(|e| {
                            failed_stage = Some(EffectStage::Seek);
                            OperationError::Backend(e)
                        })?;
                    if position == FAILURE && error != 0 {
                        return Ok(FAILURE);
                    }
                }};
            }
            macro_rules! read {
                ($buffer:expr) => {{
                    let buffer: &mut [u8] = $buffer;
                    let expected = buffer.len() as u32;
                    let status = effects.read_file(file, buffer).map_err(|e| {
                        failed_stage = Some(EffectStage::Read);
                        OperationError::Backend(e)
                    })?;
                    if !status.success || status.bytes_read != expected {
                        return Ok(FAILURE);
                    }
                }};
            }
            if refill {
                start = fp & !0x7fff;
                seek!(start);
                block_size = (size - start).min(32768);
                read!(&mut cache.bytes[..block_size as usize]);
                record.cache_valid = true;
                bp = fp - start;
            }
            let remaining = (block_size - bp) as usize;
            if remaining >= len {
                output[..len].copy_from_slice(&cache.bytes[bp as usize..bp as usize + len]);
                fp += len as u32;
                bp += len as u32;
                if bp == block_size && fp < size {
                    start += block_size;
                    record.cache_valid = false;
                }
            } else {
                output[..remaining].copy_from_slice(&cache.bytes[bp as usize..block_size as usize]);
                record.cache_valid = false;
                fp += remaining as u32;
                bp = 0;
                start += block_size;
                seek!(start);
                block_size = (size - start).min(32768);
                let rest = len - remaining;
                if rest >= block_size as usize {
                    read!(&mut output[remaining..len]);
                    fp += rest as u32;
                } else if rest > 0 {
                    read!(&mut cache.bytes[..block_size as usize]);
                    output[remaining..len].copy_from_slice(&cache.bytes[..rest]);
                    fp += rest as u32;
                    bp = rest as u32;
                    record.cache_valid = true;
                }
            }
            record.cursor.position = fp;
            record.cursor.block_file_pointer = bp;
            cache.block_start_file_pointer = start;
            cache.block_size = block_size;
            Ok(len as u32)
        })();
        if result.is_err() {
            state.interrupted = failed_stage;
        }
        result
    }
    /// Host inspection of cache bytes, including partial native I/O writes.
    pub fn inspect_loose_cache(
        &self,
        handle: u32,
    ) -> Result<Option<LooseReadCache>, OperationError<B::Error>> {
        let state = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        Ok(state
            .read_caches
            .get(handle as usize)
            .and_then(Clone::clone))
    }
}

/// Original DeCrypt, PC RVA 0x4690. The native NOT/XOR/NOT operations
/// reduce to byte XOR; reverse traversal is preserved for both regions.
pub fn de_crypt(buffer: &mut [u8], key: u64) {
    let key = key.to_le_bytes();
    let chunks = buffer.len() / 8;
    for chunk in (0..chunks).rev() {
        for i in 0..8 {
            buffer[chunk * 8 + i] ^= key[i];
        }
    }
    for i in (chunks * 8..buffer.len()).rev() {
        buffer[i] ^= key[i - chunks * 8];
    }
}

// Bounded translation of the three PC-inlined block decoders. Malformed native
// memory accesses are host errors, never invented native failure words.
fn decode_archive_block(input: &[u8], output: &mut [u8]) -> Result<usize, ()> {
    let kind = *input.first().ok_or(())?;
    if kind >= 8 {
        return Err(());
    }
    if kind != 1 {
        let source = &input[1..];
        output
            .get_mut(..source.len())
            .ok_or(())?
            .copy_from_slice(source);
        return Ok(source.len());
    }
    let mut position = 3;
    let mut flags = u16::from_be_bytes(input.get(1..3).ok_or(())?.try_into().unwrap());
    let mut bits = 16;
    let mut out = 0usize;
    while position < input.len() {
        if bits == 0 {
            flags = u16::from_be_bytes(
                input
                    .get(position..position + 2)
                    .ok_or(())?
                    .try_into()
                    .unwrap(),
            );
            position += 2;
            bits = 16;
        }
        if flags & 0x8000 == 0 {
            *output.get_mut(out).ok_or(())? = *input.get(position).ok_or(())?;
            out += 1;
            position += 1;
        } else {
            let a = *input.get(position).ok_or(())?;
            let b = *input.get(position + 1).ok_or(())?;
            let offset = ((a as usize) << 4) + (b as usize >> 4);
            if offset == 0 {
                let c = *input.get(position + 2).ok_or(())?;
                let value = *input.get(position + 3).ok_or(())?;
                let count =
                    (((b as u16) << 8).wrapping_add(c as u16).wrapping_add(15)) as usize + 1;
                output
                    .get_mut(out..out.checked_add(count).ok_or(())?)
                    .ok_or(())?
                    .fill(value);
                out += count;
                position += 4;
            } else {
                let count = (b & 15) as usize + 3;
                for _ in 0..count {
                    let value = *output.get(out.checked_sub(offset).ok_or(())?).ok_or(())?;
                    *output.get_mut(out).ok_or(())? = value;
                    out += 1;
                }
                position += 2;
            }
        }
        flags <<= 1;
        bits -= 1;
    }
    Ok(out)
}

fn archive_read<B: ReadEffects>(
    state: &mut NativeState<B>,
    index: usize,
    output: &mut [u8],
) -> Result<u32, OperationError<B::Error>> {
    let Some(cache) = state.archive_caches[index].as_ref() else {
        return Err(OperationError::UnsupportedRead);
    };
    let record = &state.records[index];
    if output.len() > u32::MAX as usize
        || record.cursor.position > record.cursor.size
        || record.cursor.block_file_pointer > 32768
    {
        return Err(OperationError::UnsafeReadState);
    }

    // Native copies numeric locals; cache bytes remain in their live allocation.
    let mut local = ArchiveReadCache {
        wav_header: cache.wav_header,
        bytes: Vec::new(),
        data_offset: cache.data_offset,
        block_start_file_pointer: cache.block_start_file_pointer,
        current_block: cache.current_block,
        block_count: cache.block_count,
        compressed_block_size: cache.compressed_block_size,
        previous_compressed_block_size: cache.previous_compressed_block_size,
        block_size: cache.block_size,
        encrypted: cache.encrypted,
        key: cache.key,
    };
    let mut fp = record.cursor.position;
    let mut bp = record.cursor.block_file_pointer;
    let len = (output.len() as u32).min(record.cursor.size - fp) as usize;
    let file = record.file;
    let mut failed_stage = None;
    let result = (|| {
        let record = &mut state.records[index];
        let cache = state.archive_caches[index].as_mut().unwrap();
        let effects = &mut state.effects;
        macro_rules! seek {
            ($p:expr) => {{
                let (position, error) = effects.seek_read_file(file, $p).map_err(|e| {
                    failed_stage = Some(EffectStage::Seek);
                    OperationError::Backend(e)
                })?;
                if position == FAILURE && error != 0 {
                    return Ok(FAILURE);
                }
            }};
        }
        macro_rules! read {
            ($out:expr,$clear:expr) => {{
                let out: &mut [u8] = $out;
                let expected = out.len() as u32;
                let status = effects.read_file(file, out).map_err(|e| {
                    failed_stage = Some(EffectStage::Read);
                    OperationError::Backend(e)
                })?;
                if !status.success || status.bytes_read != expected {
                    if $clear {
                        record.cache_valid = false;
                    }
                    return Ok(FAILURE);
                }
            }};
        }
        macro_rules! header {
            ($clear:expr,$previous:expr) => {{
                let mut header = [0; 4];
                read!(&mut header, $clear);
                let value = u32::from_le_bytes(header);
                local.compressed_block_size = value & 65535;
                if $previous {
                    local.previous_compressed_block_size = value >> 16;
                }
            }};
        }
        macro_rules! decode {
            ($out:expr,$clear:expr) => {{
                let compressed = local.compressed_block_size as usize;
                if compressed == 0 || compressed > 32769 {
                    return Err(OperationError::UnsafeReadState);
                }
                let mut payload = vec![0; compressed];
                read!(&mut payload, $clear);
                if local.encrypted {
                    de_crypt(&mut payload, local.key);
                }
                if payload[0] >= 8 && local.wav_header.is_none() {
                    return Err(OperationError::UnsupportedRead);
                }
                if !local.encrypted && local.current_block == 0 && payload[0] >= 8 {
                    de_crypt(
                        payload
                            .get_mut(1..45)
                            .ok_or(OperationError::UnsafeReadState)?,
                        local.key,
                    );
                }
                if payload[0] >= 8 {
                    let header = local
                        .wav_header
                        .as_ref()
                        .ok_or(OperationError::UnsupportedRead)?;
                    wav::wav_decompression(&payload, $out, header).map_err(|e| match e {
                        wav::WavError::Unsupported => OperationError::UnsupportedRead,
                        wav::WavError::Bounds => OperationError::UnsafeReadState,
                    })? as u32
                } else {
                    decode_archive_block(&payload, $out)
                        .map_err(|_| OperationError::UnsafeReadState)? as u32
                }
            }};
        }
        let target = fp >> 15;
        if target != local.current_block {
            if target < local.current_block && target < local.current_block >> 1 {
                seek!(local.data_offset);
                local.current_block = 0;
                local.block_start_file_pointer = 0;
                header!(false, true);
            }
            while target != local.current_block {
                if target > local.current_block {
                    local.block_start_file_pointer = local
                        .block_start_file_pointer
                        .wrapping_add(local.compressed_block_size + 4);
                    local.current_block += 1;
                } else {
                    local.block_start_file_pointer = local
                        .block_start_file_pointer
                        .wrapping_sub(local.previous_compressed_block_size + 4);
                    local.current_block -= 1;
                }
                seek!(
                    local
                        .data_offset
                        .wrapping_add(local.block_start_file_pointer)
                );
                header!(false, true);
            }
            bp = fp.wrapping_sub(local.current_block << 15);
            record.cache_valid = false;
        }
        if !record.cache_valid {
            seek!(
                local
                    .data_offset
                    .wrapping_add(local.block_start_file_pointer)
                    .wrapping_add(4)
            );
            local.block_size = decode!(&mut cache.bytes, false);
            record.cache_valid = true;
        }
        if bp > local.block_size || local.block_size > 32768 {
            return Err(OperationError::UnsafeReadState);
        }
        let remaining = (local.block_size - bp) as usize;
        let result;
        if remaining >= len {
            output[..len].copy_from_slice(&cache.bytes[bp as usize..bp as usize + len]);
            fp += len as u32;
            bp += len as u32;
            result = len;
            if bp == local.block_size && local.current_block.wrapping_add(1) < local.block_count {
                local.block_start_file_pointer = local
                    .block_start_file_pointer
                    .wrapping_add(local.compressed_block_size + 4);
                local.current_block += 1;
                seek!(
                    local
                        .data_offset
                        .wrapping_add(local.block_start_file_pointer)
                );
                header!(false, true);
                bp = 0;
                record.cache_valid = false;
            }
        } else {
            output[..remaining]
                .copy_from_slice(&cache.bytes[bp as usize..local.block_size as usize]);
            fp += remaining as u32;
            bp = 0;
            let mut written = remaining;
            let rest = len - remaining;
            local.block_start_file_pointer = local
                .block_start_file_pointer
                .wrapping_add(local.compressed_block_size + 4);
            seek!(
                local
                    .data_offset
                    .wrapping_add(local.block_start_file_pointer)
            );
            for _ in 0..rest / 32768 {
                header!(true, false);
                local.current_block = local.current_block.wrapping_add(1);
                local.block_size = decode!(
                    output
                        .get_mut(written..)
                        .ok_or(OperationError::UnsafeReadState)?,
                    true
                );
                written = written
                    .checked_add(local.block_size as usize)
                    .ok_or(OperationError::UnsafeReadState)?;
                fp = fp.wrapping_add(local.block_size);
                local.block_start_file_pointer = local
                    .block_start_file_pointer
                    .wrapping_add(local.compressed_block_size + 4);
            }
            header!(true, true);
            local.current_block = local.current_block.wrapping_add(1);
            let tail = rest % 32768;
            if tail > 0 {
                local.block_size = decode!(&mut cache.bytes, true);
                if tail > cache.bytes.len()
                    || written
                        .checked_add(tail)
                        .filter(|&end| end <= output.len())
                        .is_none()
                {
                    return Err(OperationError::UnsafeReadState);
                }
                output[written..written + tail].copy_from_slice(&cache.bytes[..tail]);
                fp = fp.wrapping_add(tail as u32);
                bp = tail as u32;
                written += tail;
            } else {
                record.cache_valid = false;
            }
            result = written;
        }
        record.cursor.position = fp;
        record.cursor.block_file_pointer = bp;
        // Allocation bytes have already been mutated; numeric locals commit together.
        local.bytes = std::mem::take(&mut cache.bytes);
        *cache = local;
        Ok(result as u32)
    })();
    if let Some(stage) = failed_stage {
        state.interrupted = Some(stage);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Arc};

    #[derive(Default)]
    struct Trace {
        calls: Vec<String>,
        seeks: VecDeque<u32>,
        close_failure: bool,
        free_error: bool,
        physical: usize,
        source: Option<Vec<u8>>,
        fail_read_at: Option<usize>,
        reads: usize,
        read_failure: bool,
    }
    struct Backend(Arc<Mutex<Trace>>);
    impl Effects for Backend {
        type Error = &'static str;
        fn set_file_pointer(
            &mut self,
            _: u32,
            distance: i32,
            method: u32,
        ) -> Result<u32, Self::Error> {
            let mut t = self.0.lock().unwrap();
            t.calls.push(format!("seek {distance} {method}"));
            Ok(t.seeks.pop_front().expect("unexpected seek"))
        }
        fn close_handle(&mut self, file: u32) -> Result<bool, Self::Error> {
            let mut t = self.0.lock().unwrap();
            t.calls.push(format!("close {file}"));
            Ok(!t.close_failure)
        }
        fn delete_file(&mut self, name: &[u8]) -> Result<bool, Self::Error> {
            self.0
                .lock()
                .unwrap()
                .calls
                .push(format!("delete {}", String::from_utf8_lossy(name)));
            Ok(false)
        }
        fn file_attributes(&mut self, _: &[u8]) -> Result<u32, Self::Error> {
            self.0.lock().unwrap().calls.push("attributes".into());
            Ok(7)
        }
        fn set_file_attributes(&mut self, _: &[u8], attributes: u32) -> Result<bool, Self::Error> {
            self.0
                .lock()
                .unwrap()
                .calls
                .push(format!("set attributes {attributes}"));
            Ok(false)
        }
        fn move_file(&mut self, from: &[u8], to: &[u8]) -> Result<bool, Self::Error> {
            self.0.lock().unwrap().calls.push(format!(
                "move {} {}",
                String::from_utf8_lossy(from),
                String::from_utf8_lossy(to)
            ));
            Ok(false)
        }
        fn free_name(&mut self, allocation: u64) -> Result<(), Self::Error> {
            self.0
                .lock()
                .unwrap()
                .calls
                .push(format!("name {allocation}"));
            Ok(())
        }
        fn free_block(&mut self, allocation: u64) -> Result<(), Self::Error> {
            let mut t = self.0.lock().unwrap();
            t.calls.push(format!("block {allocation}"));
            if t.free_error {
                Err("freed then interrupted")
            } else {
                Ok(())
            }
        }
        fn free_wav(&mut self, allocation: u64) -> Result<(), Self::Error> {
            self.0
                .lock()
                .unwrap()
                .calls
                .push(format!("wav {allocation}"));
            Ok(())
        }
    }
    fn record(flags: u32) -> HandleRecord {
        HandleRecord {
            file: 42,
            flags,
            cursor: ReadCursor::new(32768),
            cache_valid: true,
            temporary_name: 1,
            temporary_name_bytes: b"asset.tmp".to_vec(),
            block: 2,
            wav: 3,
        }
    }
    #[test]
    fn read_boundaries_wrap_and_failed_state_preservation() {
        let mut cursor = ReadCursor::new(32768);
        assert_eq!(cursor.dta_seek(32768, 0), 32768);
        assert_eq!(cursor.block_file_pointer(), 0);
        let prior = cursor;
        for (distance, method) in [(-1, 0), (32769, 0), (1, 1), (2, 2), (0, 3)] {
            assert_eq!(cursor.dta_seek(distance, method), FAILURE);
            assert_eq!(cursor, prior);
        }
        assert_eq!(cursor.dta_seek(1, 2), 32769);
        assert_eq!(cursor.dta_seek(-1, 1), 32768);
        let mut high = ReadCursor::new(u32::MAX);
        assert_eq!(high.dta_seek(1, 2), 0);
        assert_eq!(high.dta_seek(i32::MIN, 2), i32::MAX as u32);
        assert_eq!(high.dta_seek(1, 1), FAILURE);
    }
    #[test]
    fn write_fallthrough_high_bit_and_failed_position_commit() {
        let trace = Arc::new(Mutex::new(Trace {
            seeks: [FAILURE, FAILURE, 0x8000_8001, FAILURE, FAILURE].into(),
            ..Trace::default()
        }));
        let handles = Handles::new(Backend(trace.clone()));
        assert_eq!(handles.admit(record(2)).unwrap(), 0);
        assert_eq!(handles.dta_seek(0, -7, 0).unwrap(), 0x8000_8001);
        assert_eq!(
            handles
                .inspect(0)
                .unwrap()
                .unwrap()
                .cursor
                .block_file_pointer(),
            1
        );
        assert_eq!(handles.dta_seek(0, 9, 1).unwrap(), FAILURE);
        let cursor = handles.inspect(0).unwrap().unwrap().cursor;
        assert_eq!(cursor.position(), FAILURE);
        assert_eq!(cursor.block_file_pointer(), 1);
        assert_eq!(handles.dta_seek(0, 0, u32::MAX).unwrap(), FAILURE);
        assert_eq!(
            trace.lock().unwrap().calls,
            [
                "seek -7 0",
                "seek -7 1",
                "seek -7 2",
                "seek 9 1",
                "seek 9 2"
            ]
        );
    }
    #[test]
    fn close_prefix_ignored_results_and_reusable_zero_slot() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = Handles::new(Backend(trace.clone()));
        let original = record(5);
        handles.admit(original.clone()).unwrap();
        trace.lock().unwrap().close_failure = true;
        assert!(!handles.dta_close(0).unwrap());
        assert_eq!(handles.inspect(0).unwrap(), Some(original));
        trace.lock().unwrap().close_failure = false;
        trace.lock().unwrap().calls.clear();
        assert!(handles.dta_close(0).unwrap());
        assert_eq!(
            trace.lock().unwrap().calls,
            [
                "close 42",
                "delete asset",
                "attributes",
                "set attributes 6",
                "delete asset",
                "move asset.tmp asset",
                "name 1",
                "block 2",
                "wav 3"
            ]
        );
        let closed = handles.inspect(0).unwrap().unwrap();
        assert_eq!(
            (
                closed.file,
                closed.temporary_name,
                closed.block,
                closed.wav,
                closed.cache_valid
            ),
            (FAILURE, 1, 0, 0, false)
        );
        let count = trace.lock().unwrap().calls.len();
        assert!(handles.dta_close(0).unwrap());
        assert!(!handles.dta_close(FAILURE).unwrap());
        assert_eq!(handles.dta_seek(0, 0, 0).unwrap(), FAILURE);
        assert_eq!(trace.lock().unwrap().calls.len(), count);
        assert_eq!(handles.admit(record(0)), Err(OperationError::InvalidRecord));
        let mut replacement = record(0);
        replacement.temporary_name = 4;
        replacement.block = 5;
        replacement.wav = 6;
        assert_eq!(handles.admit(replacement).unwrap(), 0);
        trace.lock().unwrap().calls.clear();
        assert!(handles.dta_close(0).unwrap());
        assert_eq!(trace.lock().unwrap().calls, ["name 4", "block 5", "wav 6"]);
    }
    #[test]
    fn interrupted_free_cannot_be_retried_and_state_remains_inspectable() {
        let trace = Arc::new(Mutex::new(Trace {
            free_error: true,
            ..Trace::default()
        }));
        let handles = Handles::new(Backend(trace.clone()));
        handles.admit(record(1)).unwrap();
        assert_eq!(
            handles.dta_close(0),
            Err(OperationError::Backend("freed then interrupted"))
        );
        let count = trace.lock().unwrap().calls.len();
        assert_eq!(
            handles.dta_close(0),
            Err(OperationError::Interrupted(EffectStage::FreeBlock))
        );
        assert_eq!(
            handles.dta_seek(0, 0, 0),
            Err(OperationError::Interrupted(EffectStage::FreeBlock))
        );
        assert_eq!(
            handles.admit(record(1)),
            Err(OperationError::Interrupted(EffectStage::FreeBlock))
        );
        assert_eq!(trace.lock().unwrap().calls.len(), count);
        assert_eq!(handles.inspect(0).unwrap().unwrap().file, 42);
    }
    #[test]
    fn snapshots_preserve_short_tail_and_reject_beyond_end() {
        let handles = ReadHandles::default();
        assert_eq!(handles.insert_snapshot(vec![7, 8, 9]).unwrap(), 0);
        assert_eq!(handles.dta_seek(0, 1, 1).unwrap(), 1);
        let mut destination = [0xa5; 4];
        assert_eq!(handles.read_snapshot(0, &mut destination).unwrap(), 2);
        assert_eq!(destination, [8, 9, 0xa5, 0xa5]);
        assert_eq!(handles.dta_seek(0, 1, 2).unwrap(), 4);
        assert_eq!(
            handles.read_snapshot(0, &mut destination),
            Err(CursorError::BeyondEnd)
        );
        assert_eq!(destination, [8, 9, 0xa5, 0xa5]);
        assert!(handles.release_snapshot(0).unwrap());
        assert!(handles.release_snapshot(0).unwrap());
        assert_eq!(handles.insert_snapshot(vec![]).unwrap(), 0);
    }
    impl ReadEffects for Backend {
        fn seek_read_file(&mut self, _: u32, position: u32) -> Result<(u32, u32), Self::Error> {
            let mut t = self.0.lock().unwrap();
            t.calls.push(format!("read seek {position}"));
            t.physical = position as usize;
            Ok((position, 0))
        }
        fn read_file(&mut self, _: u32, output: &mut [u8]) -> Result<ReadFileResult, Self::Error> {
            let mut t = self.0.lock().unwrap();
            t.calls.push(format!("read {}", output.len()));
            t.reads += 1;
            let failure = t.read_failure || t.fail_read_at == Some(t.reads);
            let count = if failure {
                output.len().min(2)
            } else {
                output.len()
            };
            for (i, byte) in output[..count].iter_mut().enumerate() {
                *byte = t.source.as_ref().map_or((t.physical + i) as u8, |s| {
                    s.get(t.physical + i).copied().unwrap_or(0)
                });
                if failure {
                    *byte = !*byte;
                }
            }
            t.physical += count;
            Ok(ReadFileResult {
                success: !failure,
                bytes_read: count as u32,
            })
        }
    }
    fn loose(trace: Arc<Mutex<Trace>>, size: u32) -> Handles<Backend> {
        let handles = Handles::new(Backend(trace));
        let mut r = record(1);
        r.cursor = ReadCursor::new(size);
        r.cache_valid = false;
        handles.admit(r).unwrap();
        handles
            .attach_loose_cache(
                0,
                LooseReadCache {
                    bytes: vec![0xa5; 32768],
                    block_start_file_pointer: 0,
                    block_size: 0,
                },
            )
            .unwrap();
        handles
    }
    #[test]
    fn loose_read_cache_seek_and_direct_crossing() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = loose(trace.clone(), 100000);
        let mut first = [0; 3];
        assert_eq!(handles.dta_read(0, Some(&mut first)).unwrap(), 3);
        assert_eq!(first, [0, 1, 2]);
        assert_eq!(handles.dta_seek(0, 32766, 0).unwrap(), 32766);
        let mut crossing = vec![0xa5; 40000];
        assert_eq!(handles.dta_read(0, Some(&mut crossing)).unwrap(), 40000);
        for (i, byte) in crossing.iter().enumerate() {
            assert_eq!(*byte, (32766 + i) as u8);
        }
        assert!(!handles.inspect(0).unwrap().unwrap().cache_valid);
        assert_eq!(
            handles.inspect(0).unwrap().unwrap().cursor.position(),
            72766
        );
        assert_eq!(
            trace.lock().unwrap().calls,
            ["read seek 0", "read 32768", "read seek 32768", "read 39998"]
        );
    }
    #[test]
    fn loose_failed_crossing_preserves_numeric_state_and_output_prefix() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = loose(trace.clone(), 40000);
        assert_eq!(handles.dta_read(0, Some(&mut [0; 1])).unwrap(), 1);
        handles.dta_seek(0, 32766, 0).unwrap();
        let prior = handles.inspect(0).unwrap().unwrap().cursor;
        trace.lock().unwrap().read_failure = true;
        let mut output = [0xa5; 8];
        assert_eq!(handles.dta_read(0, Some(&mut output)).unwrap(), FAILURE);
        assert_eq!(output, [254, 255, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]);
        assert_eq!(handles.inspect(0).unwrap().unwrap().cursor, prior);
        assert!(!handles.inspect(0).unwrap().unwrap().cache_valid);
        assert_eq!(
            handles.inspect_loose_cache(0).unwrap().unwrap().bytes[..2],
            [255, 254]
        );
    }
    #[test]
    fn loose_zero_length_eof_and_guard_results() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = loose(trace.clone(), 0);
        assert_eq!(handles.dta_read(0, None).unwrap(), FAILURE);
        assert_eq!(handles.dta_read(99, Some(&mut [])).unwrap(), FAILURE);
        assert_eq!(handles.dta_read(0, Some(&mut [])).unwrap(), 0);
        assert_eq!(trace.lock().unwrap().calls, ["read seek 0", "read 0"]);
        handles.dta_seek(0, 1, 2).unwrap();
        assert_eq!(
            handles.dta_read(0, Some(&mut [])),
            Err(OperationError::UnsafeReadState)
        );
    }
    #[test]
    fn loose_cached_end_is_strict_and_unsafe_projection_is_rejected() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = loose(trace.clone(), 65536);
        handles.dta_read(0, Some(&mut [0; 1])).unwrap();
        handles.dta_seek(0, 32768, 0).unwrap();
        let mut output = [0xa5; 1];
        // Native strict cache-end comparison plus dtaSeek's modulo block cursor
        // reuses the first cached byte at this exact boundary.
        assert_eq!(handles.dta_read(0, Some(&mut output)).unwrap(), 1);
        assert_eq!(output, [0]);
        assert_eq!(trace.lock().unwrap().calls.len(), 2);
        handles
            .attach_loose_cache(
                0,
                LooseReadCache {
                    bytes: vec![0; 32768],
                    block_start_file_pointer: 65000,
                    block_size: 32768,
                },
            )
            .unwrap();
        handles.dta_seek(0, 65001, 0).unwrap();
        assert_eq!(
            handles.dta_read(0, Some(&mut output)),
            Err(OperationError::UnsafeReadState)
        );
        assert_eq!(trace.lock().unwrap().calls.len(), 2);
        assert!(handles.dta_close(0).unwrap());
        assert_eq!(handles.inspect_loose_cache(0).unwrap(), None);
    }
    fn archive(trace: Arc<Mutex<Trace>>, blocks: &[Vec<u8>]) -> Handles<Backend> {
        let mut physical = Vec::new();
        let mut previous = 0;
        for block in blocks {
            physical.extend_from_slice(&((previous << 16) | block.len() as u32).to_le_bytes());
            physical.extend_from_slice(block);
            previous = block.len() as u32;
        }
        trace.lock().unwrap().source = Some(physical);
        let handles = Handles::new(Backend(trace));
        let mut r = record(0);
        r.wav = 0;
        r.cache_valid = false;
        r.cursor = ReadCursor::new((blocks.len() * 32768) as u32);
        handles.admit(r).unwrap();
        handles
            .attach_archive_cache(
                0,
                ArchiveReadCache {
                    wav_header: None,
                    bytes: vec![0xa5; 32768],
                    data_offset: 0,
                    block_start_file_pointer: 0,
                    current_block: 0,
                    block_count: blocks.len() as u32,
                    compressed_block_size: blocks[0].len() as u32,
                    previous_compressed_block_size: 0,
                    block_size: 0,
                    encrypted: false,
                    key: 0,
                },
            )
            .unwrap();
        handles
    }
    #[test]
    fn archive_admission_requires_wav_allocation_and_header_to_agree() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = archive(trace, &[vec![0, 1]]);
        let original = handles.inspect_archive_cache(0).unwrap().unwrap();
        // The coherent absent/zero state is already installed by archive().
        handles.attach_archive_cache(0, original.clone()).unwrap();
        let mut with_header = original.clone();
        with_header.wav_header = Some([0; 44]);
        assert_eq!(
            handles.attach_archive_cache(0, with_header.clone()),
            Err(OperationError::InvalidRecord)
        );
        assert_eq!(
            handles.inspect_archive_cache(0).unwrap(),
            Some(original.clone())
        );
        // Model a nonnull native WAV allocation without changing cache contents.
        handles.state.lock().unwrap().records[0].wav = 3;
        assert_eq!(
            handles.attach_archive_cache(0, original),
            Err(OperationError::InvalidRecord)
        );
        assert_eq!(
            handles
                .inspect_archive_cache(0)
                .unwrap()
                .unwrap()
                .wav_header,
            None
        );
        handles
            .attach_archive_cache(0, with_header.clone())
            .unwrap();
        assert_eq!(
            handles.inspect_archive_cache(0).unwrap(),
            Some(with_header.clone())
        );
        let mut missing_header = with_header.clone();
        missing_header.wav_header = None;
        assert_eq!(
            handles.attach_archive_cache(0, missing_header),
            Err(OperationError::InvalidRecord)
        );
        assert_eq!(handles.inspect_archive_cache(0).unwrap(), Some(with_header));
    }
    #[test]
    fn archive_raw_lz_navigation_and_exact_boundary_prefetch() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let raw = |v| {
            let mut b = vec![v; 32769];
            b[0] = 0;
            b
        };
        // One LZ run of 32768 bytes, followed by raw blocks.
        let mut lz = vec![1, 0xff, 0];
        for _ in 0..8 {
            lz.extend_from_slice(&[0, 0x0f, 0xf0, 7]);
        }
        let handles = archive(trace.clone(), &[lz, raw(8), raw(9), raw(10)]);
        let mut out = vec![0; 32768];
        assert_eq!(handles.dta_read(0, Some(&mut out)).unwrap(), 32768);
        assert!(out.iter().all(|&v| v == 7));
        let cache = handles.inspect_archive_cache(0).unwrap().unwrap();
        assert_eq!(
            (cache.current_block, cache.previous_compressed_block_size),
            (1, 35)
        );
        assert!(!handles.inspect(0).unwrap().unwrap().cache_valid);
        handles.dta_seek(0, 3 * 32768 + 2, 0).unwrap();
        assert_eq!(handles.dta_read(0, Some(&mut [0; 1])).unwrap(), 1);
        handles.dta_seek(0, 1, 0).unwrap();
        let mut byte = [0];
        handles.dta_read(0, Some(&mut byte)).unwrap();
        assert_eq!(byte, [7]);
        assert!(
            trace
                .lock()
                .unwrap()
                .calls
                .iter()
                .filter(|s| *s == "read seek 0")
                .count()
                >= 1
        );
    }
    #[test]
    fn archive_crossing_direct_and_failed_prefetch_keep_prefix() {
        let raw = |v| {
            let mut b = vec![v; 32769];
            b[0] = 0;
            b
        };
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = archive(trace.clone(), &[raw(3), raw(4), raw(5), raw(6)]);
        handles.dta_read(0, Some(&mut [0; 1])).unwrap();
        handles.dta_seek(0, 32766, 0).unwrap();
        let mut out = vec![0; 32772];
        assert_eq!(handles.dta_read(0, Some(&mut out)).unwrap(), 32772);
        assert_eq!(&out[..2], &[3, 3]);
        assert!(out[2..32770].iter().all(|&v| v == 4));
        assert_eq!(&out[32770..], &[5, 5]);
        handles.dta_seek(0, 3 * 32768 - 1, 0).unwrap();
        let prior = handles.inspect(0).unwrap().unwrap();
        let next = trace.lock().unwrap().reads + 1;
        trace.lock().unwrap().fail_read_at = Some(next);
        let mut out = [0xa5];
        assert_eq!(handles.dta_read(0, Some(&mut out)).unwrap(), FAILURE);
        assert_eq!(out, [5]);
        assert_eq!(handles.inspect(0).unwrap().unwrap().cursor, prior.cursor);
        assert!(handles.inspect(0).unwrap().unwrap().cache_valid);
    }
    #[test]
    fn archive_decoder_overlapping_backreference_and_host_bounds() {
        let mut out = [0; 16];
        assert_eq!(
            decode_archive_block(&[1, 0x10, 0, b'a', b'b', b'c', 0, 0x30], &mut out),
            Ok(6)
        );
        assert_eq!(&out[..6], b"abcabc");
        assert!(decode_archive_block(&[1, 0x80, 0, 0x10, 0], &mut out).is_err());
        assert!(decode_archive_block(&[8], &mut out).is_err());
    }
    #[test]
    fn archive_encryption_and_unsupported_wav_are_distinct() {
        let key = 0x0807060504030201;
        let mut payload = vec![0; 32769];
        payload[1..].fill(0x6a);
        de_crypt(&mut payload, key);
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = archive(trace.clone(), &[payload]);
        let mut cache = handles.inspect_archive_cache(0).unwrap().unwrap();
        cache.encrypted = true;
        cache.key = key;
        handles.attach_archive_cache(0, cache).unwrap();
        let mut out = [0; 9];
        assert_eq!(handles.dta_read(0, Some(&mut out)).unwrap(), 9);
        assert_eq!(out, [0x6a; 9]);
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = archive(trace, &[vec![8; 32]]);
        assert_eq!(
            handles.dta_read(0, Some(&mut [0; 1])),
            Err(OperationError::UnsupportedRead)
        );
        assert_eq!(handles.inspect(0).unwrap().unwrap().cursor.position(), 0);
        let mut bytes = (0..31).collect::<Vec<u8>>();
        let original = bytes.clone();
        de_crypt(&mut bytes, key);
        for (i, &byte) in bytes.iter().enumerate() {
            assert_eq!(byte, original[i] ^ ((i % 8 + 1) as u8));
        }
        de_crypt(&mut bytes, key);
        assert_eq!(bytes, original);
    }
}

mod open;
mod wav;
mod wav_table;
pub use open::{OpenAllocation, OpenEffects};

/// Read-only filesystem host adapter; adds no native function credit.
pub mod io;

mod write;
pub use write::{
    DTA_CHANGE_ATTRIBUTES, DTA_CREATE_FILE, DTA_FILE_BUFFER, DTA_TEMPORARY, OpenWriteEffects,
    WriteEffects, WriteFileResult,
};
