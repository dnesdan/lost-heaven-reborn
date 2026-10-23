//! Bounded read-only host IO. These adapters add no reconstructed function credit.
use super::*;
use std::{
    collections::BTreeMap,
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Debug, PartialEq, Eq)]
pub enum HostError {
    Policy,
    Identity,
    Exhausted,
    ReadOnly,
}
#[derive(Clone, Copy)]
struct CheckedEntry {
    size: u32,
    blocks: u32,
    data: u64,
    end: u64,
    encrypted: bool,
}
struct Source {
    file: File,
    snapshot: Metadata,
    entries: BTreeMap<String, CheckedEntry>,
    catalog: BTreeMap<String, crate::dta::Entry>,
    key: u64,
    read_bounds: Option<(u64, u64)>,
}
impl Source {
    /// Host preflight uses the native pure block decoders, never another provider.
    /// Exact decoded chunks are required by the native 32768-byte seek domain.
    fn validate_entry(
        &mut self,
        entry: CheckedEntry,
        wav_header: Option<&[u8; 44]>,
    ) -> Result<(), StreamError> {
        if !crate::dta::same_snapshot(
            &self.snapshot,
            &self.file.metadata().map_err(|_| StreamError::Failure)?,
        ) {
            return Err(StreamError::Failure);
        }
        let mut cursor = entry.data;
        let mut remaining = entry.size as usize;
        let mut previous_compressed = 0;
        let mut encoded = vec![0; 32769];
        let mut decoded = vec![0; 32768];
        for block in 0..entry.blocks {
            if cursor.checked_add(4).is_none_or(|end| end > entry.end) {
                return Err(StreamError::Bounds);
            }
            self.file
                .seek(SeekFrom::Start(cursor))
                .map_err(|_| StreamError::Failure)?;
            let mut header = [0; 4];
            self.file
                .read_exact(&mut header)
                .map_err(|_| StreamError::Failure)?;
            let link = u32::from_le_bytes(header);
            let size = (link & 0xffff) as usize;
            if block > 0 && (link >> 16) as usize != previous_compressed {
                return Err(StreamError::Bounds);
            }
            previous_compressed = size;
            cursor += 4;
            if size == 0
                || size > encoded.len()
                || cursor
                    .checked_add(size as u64)
                    .is_none_or(|end| end > entry.end)
            {
                return Err(StreamError::Bounds);
            }
            let input = &mut encoded[..size];
            self.file
                .read_exact(input)
                .map_err(|_| StreamError::Failure)?;
            cursor += size as u64;
            if entry.encrypted {
                de_crypt(input, self.key);
            }
            if !entry.encrypted && block == 0 && input[0] >= 8 {
                de_crypt(input.get_mut(1..45).ok_or(StreamError::Bounds)?, self.key);
            }
            let count = if input[0] >= 8 {
                wav::wav_decompression(input, &mut decoded, wav_header.ok_or(StreamError::Bounds)?)
                    .map_err(|_| StreamError::Bounds)?
            } else {
                decode_archive_block(input, &mut decoded).map_err(|_| StreamError::Bounds)?
            };
            let expected = remaining.min(32768);
            if count != expected || expected == 0 {
                return Err(StreamError::Bounds);
            }
            remaining -= count;
        }
        if remaining != 0 || cursor != entry.end {
            return Err(StreamError::Bounds);
        }
        if !crate::dta::same_snapshot(
            &self.snapshot,
            &self.file.metadata().map_err(|_| StreamError::Failure)?,
        ) {
            return Err(StreamError::Failure);
        }
        Ok(())
    }
}
/// Owns host resources. Ordinary IO failures return native failure words; Err is
/// reserved for invariant violations and unsupported mutation effects.
pub struct FilesystemEffects {
    root: PathBuf,
    files: BTreeMap<u32, Source>,
    allocations: BTreeMap<u64, (OpenAllocation, u32)>,
    next_file: u32,
    next_allocation: u64,
    allocated: u64,
    budget: u64,
}
impl FilesystemEffects {
    pub fn new(root: &Path, allocation_budget: u64) -> Result<Self, HostError> {
        let root = std::fs::canonicalize(root).map_err(|_| HostError::Policy)?;
        if !root.is_dir() {
            return Err(HostError::Policy);
        }
        Ok(Self {
            root,
            files: BTreeMap::new(),
            allocations: BTreeMap::new(),
            next_file: 0,
            next_allocation: 1,
            allocated: 0,
            budget: allocation_budget,
        })
    }
    fn resolve(&self, name: &[u8]) -> Option<PathBuf> {
        if name.len() >= 260 || !name.is_ascii() {
            return None;
        }
        let name = crate::dta::canonical(std::str::from_utf8(name).ok()?).ok()?;
        let mut path = self.root.clone();
        for part in name.split('/') {
            let mut found = None;
            for child in crate::bounded_children(&path).ok()? {
                if child
                    .file_name()
                    .and_then(|v| v.to_str())
                    .is_some_and(|v| v.to_lowercase() == part)
                    && found.replace(child).is_some()
                {
                    return None;
                }
            }
            let child = found?;
            if std::fs::symlink_metadata(&child)
                .ok()?
                .file_type()
                .is_symlink()
            {
                return None;
            }
            crate::verify_contained(&self.root, &child).ok()?;
            path = child;
        }
        Some(path)
    }
    fn source(&mut self, id: u32) -> Option<&mut Source> {
        let source = self.files.get_mut(&id)?;
        if !crate::dta::same_snapshot(&source.snapshot, &source.file.metadata().ok()?) {
            return None;
        }
        Some(source)
    }
    fn free(&mut self, token: u64, kind: OpenAllocation) -> Result<(), HostError> {
        let Some(&(actual, size)) = self.allocations.get(&token) else {
            return Err(HostError::Identity);
        };
        if actual != kind {
            return Err(HostError::Identity);
        }
        self.allocations.remove(&token);
        self.allocated -= size as u64;
        Ok(())
    }
    /// Safe host disposal, deliberately distinct from native Close/Release.
    pub fn dispose(&mut self) {
        self.files.clear();
        self.allocations.clear();
        self.allocated = 0;
    }
}
impl Effects for FilesystemEffects {
    type Error = HostError;
    fn set_file_pointer(
        &mut self,
        file: u32,
        distance: i32,
        method: u32,
    ) -> Result<u32, HostError> {
        let Some(s) = self.source(file) else {
            return Ok(FAILURE);
        };
        let base = match method {
            0 => 0,
            1 => match s.file.stream_position() {
                Ok(v) => v,
                Err(_) => return Ok(FAILURE),
            },
            2 => s.snapshot.len(),
            _ => return Ok(FAILURE),
        };
        let target = base as i64 + distance as i64;
        if target < 0 || target > u32::MAX as i64 {
            return Ok(FAILURE);
        }
        Ok(s.file
            .seek(SeekFrom::Start(target as u64))
            .ok()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(FAILURE))
    }
    fn close_handle(&mut self, file: u32) -> Result<bool, HostError> {
        Ok(self.files.remove(&file).is_some())
    }
    fn delete_file(&mut self, _: &[u8]) -> Result<bool, HostError> {
        Err(HostError::ReadOnly)
    }
    fn file_attributes(&mut self, _: &[u8]) -> Result<u32, HostError> {
        Err(HostError::ReadOnly)
    }
    fn set_file_attributes(&mut self, _: &[u8], _: u32) -> Result<bool, HostError> {
        Err(HostError::ReadOnly)
    }
    fn move_file(&mut self, _: &[u8], _: &[u8]) -> Result<bool, HostError> {
        Err(HostError::ReadOnly)
    }
    fn free_name(&mut self, _: u64) -> Result<(), HostError> {
        Err(HostError::Identity)
    }
    fn free_block(&mut self, n: u64) -> Result<(), HostError> {
        self.free(n, OpenAllocation::Block)
    }
    fn free_wav(&mut self, n: u64) -> Result<(), HostError> {
        self.free(n, OpenAllocation::Wav)
    }
}
impl ReadEffects for FilesystemEffects {
    fn seek_read_file(&mut self, file: u32, position: u32) -> Result<(u32, u32), HostError> {
        let Some(s) = self.source(file) else {
            return Ok((FAILURE, 1));
        };
        if position as u64 > s.snapshot.len()
            || s.read_bounds
                .is_some_and(|(start, end)| (position as u64) < start || position as u64 > end)
        {
            return Ok((FAILURE, 1));
        }
        Ok(match s.file.seek(SeekFrom::Start(position as u64)) {
            Ok(_) => (position, 0),
            Err(_) => (FAILURE, 1),
        })
    }
    fn read_file(&mut self, file: u32, out: &mut [u8]) -> Result<ReadFileResult, HostError> {
        let failure = ReadFileResult {
            success: false,
            bytes_read: 0,
        };
        if out.len() > 64 * 1024 * 1024 {
            return Ok(failure);
        }
        let Some(s) = self.source(file) else {
            return Ok(failure);
        };
        if let Some((start, end)) = s.read_bounds {
            let Ok(position) = s.file.stream_position() else {
                return Ok(failure);
            };
            if position < start
                || position
                    .checked_add(out.len() as u64)
                    .is_none_or(|n| n > end)
            {
                return Ok(failure);
            }
        }
        Ok(match s.file.read(out) {
            Ok(n) => ReadFileResult {
                success: true,
                bytes_read: n as u32,
            },
            Err(_) => failure,
        })
    }
}
impl OpenEffects for FilesystemEffects {
    fn create_file(&mut self, name: &[u8], _: bool) -> Result<u32, HostError> {
        if self.files.len() >= 4096 {
            return Ok(FAILURE);
        }
        let Some(path) = self.resolve(name) else {
            return Ok(FAILURE);
        };
        let Ok(file) = crate::dta::checked_open(&path) else {
            return Ok(FAILURE);
        };
        if crate::verify_contained(&self.root, &path).is_err() {
            return Ok(FAILURE);
        }
        let Ok(snapshot) = file.metadata() else {
            return Ok(FAILURE);
        };
        if snapshot.len() >= u32::MAX as u64 {
            return Ok(FAILURE);
        }
        let id = self.next_file;
        if id == FAILURE {
            return Err(HostError::Exhausted);
        }
        self.next_file += 1;
        self.files.insert(
            id,
            Source {
                file,
                snapshot,
                entries: BTreeMap::new(),
                catalog: BTreeMap::new(),
                key: 0,
                read_bounds: None,
            },
        );
        Ok(id)
    }
    fn seek_file_end(&mut self, file: u32) -> Result<(u32, u32), HostError> {
        let Some(s) = self.source(file) else {
            return Ok((FAILURE, 1));
        };
        Ok(match s.file.seek(SeekFrom::End(0)) {
            Ok(n) => (n as u32, 0),
            Err(_) => (FAILURE, 1),
        })
    }
    fn allocate_open(&mut self, kind: OpenAllocation, size: u32) -> Result<u64, HostError> {
        if self
            .allocated
            .checked_add(size as u64)
            .is_none_or(|n| n > self.budget)
        {
            return Ok(0);
        }
        let token = self.next_allocation;
        self.next_allocation = token.checked_add(1).ok_or(HostError::Exhausted)?;
        self.allocations.insert(token, (kind, size));
        self.allocated += size as u64;
        Ok(token)
    }
    fn free_owner(&mut self, n: u64) -> Result<(), HostError> {
        self.free(n, OpenAllocation::Owner)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StreamError {
    UnsupportedName,
    Native(OperationError<HostError>),
    Failure,
    Bounds,
    Closed,
}
impl From<OperationError<HostError>> for StreamError {
    fn from(e: OperationError<HostError>) -> Self {
        Self::Native(e)
    }
}
/// Explicit source selection prevents fallback after any selected-copy failure.
#[derive(Clone, Copy, Debug)]
pub enum SelectedSource {
    Loose,
    Archive(u64),
}
pub struct NativeFiles {
    handles: Handles<FilesystemEffects>,
    quota: u32,
    gate: Mutex<()>,
    closed: std::sync::atomic::AtomicBool,
}
impl NativeFiles {
    /// The budget covers logical effect allocation charges and retained Source
    /// string lengths plus a fixed per-entry policy allowance. It excludes
    /// transient parsing, returned catalog/Installation clones and allocator
    /// capacity/overhead; it is not a total process memory or RSS limit.
    pub fn new(root: &Path, quota: u32, allocation_budget: u64) -> Result<Self, HostError> {
        if quota > 64 * 1024 * 1024 {
            return Err(HostError::Policy);
        }
        Ok(Self {
            handles: Handles::new(FilesystemEffects::new(root, allocation_budget)?),
            quota,
            gate: Mutex::new(()),
            closed: std::sync::atomic::AtomicBool::new(false),
        })
    }
    /// Key is the native UnlockPack argument, before its fixed XOR.
    pub fn register(&self, name: &[u8], unlock_key: u64) -> Result<u64, StreamError> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
        if self.closed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StreamError::Closed);
        }
        let owner = self.handles.dta_create(name)?.ok_or(StreamError::Failure)?;
        {
            let mut s = self
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
            let file = s
                .owners
                .iter()
                .find(|o| o.token == owner)
                .ok_or(StreamError::Failure)?
                .file;
            let source = s.effects.source(file).ok_or(StreamError::Failure)?;
            let clone = source.file.try_clone().map_err(|_| StreamError::Failure)?;
            let key = unlock_key ^ 0x3498576239475694;
            // The duplicate shares the cursor, but this entire intake runs under
            // both locks, before UnlockPack's explicit seek. No decoder is called.
            let index = crate::dta::Archive::from_file(clone, (key >> 32) as u32, key as u32)
                .map_err(|_| StreamError::Bounds)?;
            let storage = index.entries().try_fold(0u64, |total, entry| {
                let canonical =
                    crate::dta::canonical(&entry.path).map_err(|_| StreamError::Bounds)?;
                // Source retains two canonical map keys, one decoded path and
                // one raw native name. The fixed allowance is policy, not RSS.
                (canonical.len() as u64)
                    .checked_mul(2)
                    .and_then(|n| n.checked_add(entry.path.len() as u64))
                    .and_then(|n| n.checked_add(entry.native_name.len() as u64))
                    .and_then(|n| n.checked_add(512))
                    .and_then(|n| total.checked_add(n))
                    .ok_or(StreamError::Bounds)
            })?;
            if s.effects
                .allocated
                .checked_add(storage)
                .is_none_or(|n| n > s.effects.budget)
            {
                return Err(StreamError::Bounds);
            }
            let allocation = s
                .effects
                .allocations
                .get_mut(&owner)
                .ok_or(StreamError::Bounds)?;
            allocation.1 = allocation
                .1
                .checked_add(u32::try_from(storage).map_err(|_| StreamError::Bounds)?)
                .ok_or(StreamError::Bounds)?;
            s.effects.allocated += storage;
            let source = s.effects.source(file).ok_or(StreamError::Failure)?;
            source.key = key;
            source.catalog = index
                .entries()
                .map(|e| (crate::dta::canonical(&e.path).unwrap(), e.clone()))
                .collect();
            source.entries = index
                .entries()
                .map(|e| {
                    (
                        crate::dta::canonical(&e.path).unwrap(),
                        CheckedEntry {
                            size: e.size as u32,
                            blocks: e.blocks,
                            data: e.offset,
                            end: e.end,
                            encrypted: e.encrypted,
                        },
                    )
                })
                .collect();
        }
        if !self.handles.unlock_pack(owner, unlock_key)? {
            return Err(StreamError::Failure);
        }
        Ok(owner)
    }
    /// Immutable identities admitted from the same retained file used by native IO.
    pub(crate) fn catalog(&self, owner: u64) -> Result<Vec<crate::dta::Entry>, StreamError> {
        let _gate = self.gate.lock().map_err(|_| StreamError::Failure)?;
        let mut state = self
            .handles
            .state
            .lock()
            .map_err(|_| StreamError::Failure)?;
        let file = state
            .owners
            .iter()
            .find(|o| o.token == owner)
            .ok_or(StreamError::Failure)?
            .file;
        let source = state.effects.source(file).ok_or(StreamError::Failure)?;
        Ok(source.catalog.values().cloned().collect())
    }

    /// Checked Installation selection; never searches another owner or the loose tree.
    pub(crate) fn read_admitted(
        &self,
        owner: u64,
        entry: &crate::dta::Entry,
    ) -> Result<Vec<u8>, StreamError> {
        if !entry.native_name.is_ascii() {
            return Err(StreamError::UnsupportedName);
        }
        {
            let _gate = self.gate.lock().map_err(|_| StreamError::Failure)?;
            if self.closed.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(StreamError::Closed);
            }
            let mut state = self
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Failure)?;
            let registered = state
                .owners
                .iter()
                .find(|o| o.token == owner)
                .ok_or(StreamError::Failure)?;
            let row = entry.row as usize * 28;
            if registered.table.get(row..row + 28) != Some(entry.table_row.as_slice()) {
                return Err(StreamError::Bounds);
            }
            if entry.size == 0 {
                // The host empty accommodation must still select the admitted
                // native row, including native hash/suffix and table order.
                let upper = entry.native_name.to_ascii_uppercase();
                let sum = upper.iter().fold(0u16, |n, &b| n.wrapping_add(b as u16));
                let hash = ((upper.len() as u32) << 16) | u32::from(sum);
                let first =
                    super::open::first_index(&registered.table, hash).ok_or(StreamError::Bounds)?;
                if (entry.row as usize) < first
                    || (first..=entry.row as usize).any(|n| {
                        u32::from_le_bytes(registered.table[n * 28..n * 28 + 4].try_into().unwrap())
                            != hash
                    })
                {
                    return Err(StreamError::Bounds);
                }
                let suffix = &upper[upper.len().saturating_sub(16)..];
                let mut expected = [0; 16];
                expected[..suffix.len()].copy_from_slice(suffix);
                let stored = &entry.table_row[12..28];
                // Native strncmp ignores bytes after a matching NUL.
                let compared = suffix.len().saturating_add(1).min(16);
                if stored[..compared] != expected[..compared] {
                    return Err(StreamError::Bounds);
                }
            }
            let file = registered.file;
            let source = state.effects.source(file).ok_or(StreamError::Failure)?;
            let canonical = crate::dta::canonical(&entry.path).map_err(|_| StreamError::Bounds)?;
            if source.catalog.get(&canonical) != Some(entry) || entry.size > self.quota as usize {
                return Err(StreamError::Bounds);
            }
            // Native archive_open always fetches a first block. A checked empty
            // entry instead has exactly zero blocks and no encoded bytes.
            if entry.size == 0 {
                let checked = *source.entries.get(&canonical).ok_or(StreamError::Bounds)?;
                source.validate_entry(checked, None)?;
                return Ok(Vec::new());
            }
        }
        let mut stream = self.open(&entry.native_name, SelectedSource::Archive(owner))?;
        let selected = (|| {
            let handle = stream.live_handle()? as usize;
            let state = self
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Failure)?;
            let cache = state.archive_caches[handle]
                .as_ref()
                .ok_or(StreamError::Bounds)?;
            let registered = state
                .owners
                .iter()
                .find(|o| o.token == owner)
                .ok_or(StreamError::Failure)?;
            if state.records[handle].file != registered.file
                || cache.data_offset as u64 != entry.offset
                || cache.block_count != entry.blocks
                || cache.encrypted != entry.encrypted
                || state.records[handle].cursor.size() as usize != entry.size
            {
                return Err(StreamError::Bounds);
            }
            Ok(())
        })();
        selected?;
        let mut output = vec![0; entry.size];
        let mut position = 0;
        while position < output.len() {
            let end = (position + 32768).min(output.len());
            let count = stream.read(&mut output[position..end])? as usize;
            if count == 0 || count > end - position {
                return Err(StreamError::Bounds);
            }
            position += count;
        }
        stream.close()?;
        Ok(output)
    }
    pub fn release(&self, owner: u64) -> Result<Option<u32>, StreamError> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
        Ok(self.handles.release(owner)?)
    }
    pub fn open(
        &self,
        name: &[u8],
        source: SelectedSource,
    ) -> Result<NativeStream<'_>, StreamError> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
        if self.closed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(StreamError::Closed);
        }
        let _canonical = std::str::from_utf8(name)
            .ok()
            .and_then(|n| crate::dta::canonical(n).ok())
            .ok_or(StreamError::Bounds)?;
        if !name.is_ascii() || name.len() >= 260 {
            return Err(StreamError::Bounds);
        }
        let owner = match source {
            SelectedSource::Loose => None,
            SelectedSource::Archive(n) => Some(n),
        };
        let handle = self.handles.open_selected(name, owner)?;
        if handle == FAILURE {
            return Err(StreamError::Failure);
        }
        let size = self
            .handles
            .inspect(handle)?
            .ok_or(StreamError::Failure)?
            .cursor
            .size();
        if size > self.quota {
            self.handles.dta_close(handle)?;
            return Err(StreamError::Bounds);
        }
        let validation = (|| {
            let mut state = self
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
            if let Some(cache) = state.archive_caches[handle as usize].as_ref() {
                let data = cache.data_offset as u64;
                let wav_header = cache.wav_header;
                let file = state.records[handle as usize].file;
                let source = state.effects.source(file).ok_or(StreamError::Failure)?;
                let entry = *source
                    .entries
                    .values()
                    .find(|entry| entry.data == data)
                    .ok_or(StreamError::Bounds)?;
                if entry.size != size {
                    return Err(StreamError::Bounds);
                }
                source.validate_entry(entry, wav_header.as_ref())?;
            }
            Ok(())
        })();
        if let Err(error) = validation {
            self.handles.dta_close(handle)?;
            return Err(error);
        }
        let block = self
            .handles
            .inspect(handle)?
            .ok_or(StreamError::Failure)?
            .block;
        Ok(NativeStream {
            files: self,
            handle: Some(handle),
            block,
        })
    }
    /// Native teardown stops at the first interruption; Drop still disposes host resources.
    pub fn shutdown(&self) -> Result<(), StreamError> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
        if self.closed.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return Err(StreamError::Closed);
        }
        let (handles, owners) = {
            let s = self
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
            (
                (0..s.records.len()).map(|n| n as u32).collect::<Vec<_>>(),
                s.owners.iter().map(|o| o.token).collect::<Vec<_>>(),
            )
        };
        for handle in handles {
            self.handles.dta_close(handle)?;
        }
        for owner in owners {
            self.handles.release(owner)?;
        }
        Ok(())
    }
}
impl Drop for NativeFiles {
    fn drop(&mut self) {
        let s = self
            .handles
            .state
            .get_mut()
            .unwrap_or_else(|e| e.into_inner());
        s.effects.dispose();
    }
}
pub struct NativeStream<'a> {
    files: &'a NativeFiles,
    handle: Option<u32>,
    block: u64,
}
impl NativeStream<'_> {
    fn live_handle(&self) -> Result<u32, StreamError> {
        let h = self.handle.ok_or(StreamError::Closed)?;
        let r = self.files.handles.inspect(h)?.ok_or(StreamError::Closed)?;
        if r.file == FAILURE || r.block != self.block {
            return Err(StreamError::Closed);
        }
        Ok(h)
    }

    pub fn read(&mut self, out: &mut [u8]) -> Result<u32, StreamError> {
        let _gate = self
            .files
            .gate
            .lock()
            .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
        if out.len() > self.files.quota as usize {
            return Err(StreamError::Bounds);
        }
        let h = self.live_handle()?;
        {
            let mut s = self
                .files
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
            let file = s.records.get(h as usize).ok_or(StreamError::Failure)?.file;
            if s.effects.source(file).is_none() {
                return Err(StreamError::Failure);
            }
        }
        let cursor = self
            .files
            .handles
            .inspect(h)?
            .ok_or(StreamError::Failure)?
            .cursor;
        if cursor.position() > cursor.size() {
            return Err(StreamError::Bounds);
        }
        {
            let mut s = self
                .files
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
            let file = s.records[h as usize].file;
            let data = s.archive_caches[h as usize]
                .as_ref()
                .map(|c| c.data_offset as u64);
            let source = s.effects.source(file).ok_or(StreamError::Failure)?;
            source.read_bounds = match data {
                Some(data) => Some(
                    source
                        .entries
                        .values()
                        .find(|entry| entry.data == data)
                        .map(|entry| (entry.data, entry.end))
                        .ok_or(StreamError::Bounds)?,
                ),
                None => None,
            };
        }
        let result = self.files.handles.dta_read(h, Some(out));
        {
            let mut s = self
                .files
                .handles
                .state
                .lock()
                .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
            let file = s.records[h as usize].file;
            let source = s.effects.files.get_mut(&file).ok_or(StreamError::Failure)?;
            source.read_bounds = None;
            if !crate::dta::same_snapshot(
                &source.snapshot,
                &source.file.metadata().map_err(|_| StreamError::Failure)?,
            ) {
                return Err(StreamError::Failure);
            }
        }
        let n = result?;
        if n == FAILURE {
            Err(StreamError::Failure)
        } else {
            Ok(n)
        }
    }
    pub fn seek(&mut self, distance: i32, method: u32) -> Result<u32, StreamError> {
        let _gate = self
            .files
            .gate
            .lock()
            .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
        let n = self
            .files
            .handles
            .dta_seek(self.live_handle()?, distance, method)?;
        if n == FAILURE {
            Err(StreamError::Failure)
        } else {
            Ok(n)
        }
    }
    pub fn close(&mut self) -> Result<(), StreamError> {
        let _gate = self
            .files
            .gate
            .lock()
            .map_err(|_| StreamError::Native(OperationError::LockPoisoned))?;
        if self.handle.is_some() {
            let h = match self.live_handle() {
                Ok(h) => h,
                Err(StreamError::Closed) => {
                    self.handle = None;
                    return Ok(());
                }
                Err(e) => return Err(e),
            };
            if !self.files.handles.dta_close(h)? {
                return Err(StreamError::Failure);
            }
            self.handle = None;
        }
        Ok(())
    }
}
impl Drop for NativeStream<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "lhr-native-io-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn pack(encrypted: bool) -> Vec<u8> {
        let key = 0x3498576239475694;
        let mut out = vec![0; 89];
        out[..4].copy_from_slice(b"ISD0");
        let mut header = [0; 16];
        header[..4].copy_from_slice(&1u32.to_le_bytes());
        header[4..8].copy_from_slice(&20u32.to_le_bytes());
        header[8..12].copy_from_slice(&28u32.to_le_bytes());
        de_crypt(&mut header, key);
        out[4..20].copy_from_slice(&header);
        let mut row = [0; 28];
        row[..4].copy_from_slice(&((1u32 << 16) | 65).to_le_bytes());
        row[4..8].copy_from_slice(&48u32.to_le_bytes());
        row[8..12].copy_from_slice(&81u32.to_le_bytes());
        row[12] = b'A';
        de_crypt(&mut row, key);
        out[20..48].copy_from_slice(&row);
        let mut file = [0; 32];
        file[..4].copy_from_slice(&41u32.to_le_bytes());
        file[16..20].copy_from_slice(&3u32.to_le_bytes());
        file[20..24].copy_from_slice(&1u32.to_le_bytes());
        file[24..28].copy_from_slice(&(1u32 | if encrypted { 0x8000 } else { 0 }).to_le_bytes());
        de_crypt(&mut file, key);
        out[48..80].copy_from_slice(&file);
        let mut name = *b"a";
        de_crypt(&mut name, key);
        out[80] = name[0];
        out[81..85].copy_from_slice(&4u32.to_le_bytes());
        let mut block = [0, 7, 8, 9];
        if encrypted {
            de_crypt(&mut block, key);
        }
        out[85..89].copy_from_slice(&block);
        out
    }
    fn encoded_pack(mut block: Vec<u8>, size: u32, encrypted: bool) -> Vec<u8> {
        let key = 0x3498576239475694;
        let mut out = pack(false);
        let mut header: [u8; 32] = out[48..80].try_into().unwrap();
        de_crypt(&mut header, key);
        header[..4].copy_from_slice(&(37 + block.len() as u32).to_le_bytes());
        header[16..20].copy_from_slice(&size.to_le_bytes());
        header[24..28].copy_from_slice(&(1u32 | if encrypted { 0x8000 } else { 0 }).to_le_bytes());
        de_crypt(&mut header, key);
        out[48..80].copy_from_slice(&header);
        if encrypted {
            de_crypt(&mut block, key);
        }
        out.truncate(81);
        out.extend_from_slice(&(block.len() as u32).to_le_bytes());
        out.extend(block);
        out
    }
    #[test]
    fn real_files_lz_and_pcm16_wav_plain_and_cipher_mono_and_stereo() {
        for encrypted in [false, true] {
            let dir = Directory::new();
            std::fs::write(
                dir.0.join("lz"),
                encoded_pack(vec![1, 0, 0, 7, 8, 9], 3, encrypted),
            )
            .unwrap();
            for channels in [1u16, 2] {
                let mut header = [0; 44];
                header[..4].copy_from_slice(b"RIFF");
                header[12..16].copy_from_slice(b"fmt ");
                header[36..40].copy_from_slice(b"data");
                header[22..24].copy_from_slice(&channels.to_le_bytes());
                header[32..34].copy_from_slice(&(channels * 2).to_le_bytes());
                let mut block = vec![8];
                block.extend_from_slice(&header);
                for _ in 0..channels {
                    block.extend_from_slice(&100i16.to_le_bytes());
                }
                block.extend(std::iter::repeat_n(1, channels as usize));
                if !encrypted {
                    de_crypt(&mut block[1..45], 0x3498576239475694);
                }
                std::fs::write(
                    dir.0.join(format!("wav{channels}")),
                    encoded_pack(block, 44 + channels as u32 * 4, encrypted),
                )
                .unwrap();
            }
            let files = NativeFiles::new(&dir.0, 100, 1000000).unwrap();
            let owner = files.register(b"lz", 0).unwrap();
            let mut lz = files.open(b"a", SelectedSource::Archive(owner)).unwrap();
            let mut out = [0; 3];
            assert_eq!(lz.read(&mut out).unwrap(), 3);
            assert_eq!(out, [7, 8, 9]);
            for channels in [1u16, 2] {
                let owner = files
                    .register(format!("wav{channels}").as_bytes(), 0)
                    .unwrap();
                let mut wav = files.open(b"a", SelectedSource::Archive(owner)).unwrap();
                let mut out = vec![0; 44 + channels as usize * 4];
                assert_eq!(wav.read(&mut out).unwrap(), out.len() as u32);
                assert_eq!(&out[..4], b"RIFF");
                assert_eq!(&out[44..46], &100i16.to_le_bytes());
                wav.seek(0, 0).unwrap();
                assert_eq!(wav.read(&mut out).unwrap(), out.len() as u32);
            }
            files.shutdown().unwrap();
        }
    }
    #[test]
    fn preflight_rejects_decoded_mismatch_trailing_bytes_and_short_chains() {
        let dir = Directory::new();
        let mut trailing = pack(false);
        trailing.extend_from_slice(&[0, 0]);
        let mut header: [u8; 32] = trailing[48..80].try_into().unwrap();
        de_crypt(&mut header, 0x3498576239475694);
        header[..4].copy_from_slice(&43u32.to_le_bytes());
        de_crypt(&mut header, 0x3498576239475694);
        trailing[48..80].copy_from_slice(&header);
        let mut short_chain = pack(false);
        let mut header: [u8; 32] = short_chain[48..80].try_into().unwrap();
        de_crypt(&mut header, 0x3498576239475694);
        header[20..24].copy_from_slice(&2u32.to_le_bytes());
        de_crypt(&mut header, 0x3498576239475694);
        short_chain[48..80].copy_from_slice(&header);
        let mut wav_header = [0; 44];
        wav_header[..4].copy_from_slice(b"RIFF");
        wav_header[12..16].copy_from_slice(b"fmt ");
        wav_header[36..40].copy_from_slice(b"data");
        wav_header[22..24].copy_from_slice(&1u16.to_le_bytes());
        wav_header[32..34].copy_from_slice(&2u16.to_le_bytes());
        let mut wav = vec![8];
        wav.extend_from_slice(&wav_header);
        wav.extend_from_slice(&100i16.to_le_bytes());
        wav.push(1);
        let cases = [
            encoded_pack(vec![0, 7, 8, 9], 2, false),
            trailing,
            short_chain,
            encoded_pack(vec![0, 7, 8, 9], 4, true),
            encoded_pack(vec![1, 0, 0, 7, 8, 9], 2, false),
            encoded_pack(wav, 47, true),
        ];
        std::fs::write(dir.0.join("a"), [1, 2, 3]).unwrap();
        let files = NativeFiles::new(&dir.0, 100, 1000000).unwrap();
        for (i, bytes) in cases.iter().enumerate() {
            let name = format!("bad{i}");
            std::fs::write(dir.0.join(&name), bytes).unwrap();
            let owner = files.register(name.as_bytes(), 0).unwrap();
            assert!(
                matches!(
                    files.open(b"a", SelectedSource::Archive(owner)),
                    Err(StreamError::Bounds)
                ),
                "malformed case {i}"
            );
            assert_eq!(
                files
                    .handles
                    .state
                    .lock()
                    .unwrap()
                    .records
                    .iter()
                    .filter(|r| r.file != FAILURE)
                    .count(),
                0
            );
            files.release(owner).unwrap();
        }
        let mut good = files.open(b"a", SelectedSource::Loose).unwrap();
        let mut out = [0; 3];
        assert_eq!(good.read(&mut out).unwrap(), 3);
        assert_eq!(out, [1, 2, 3]);
    }
    #[test]
    fn real_files_native_lifetime_selection_and_stale_stream_drop() {
        for encrypted in [false, true] {
            let dir = Directory::new();
            std::fs::write(dir.0.join("PACK"), pack(encrypted)).unwrap();
            std::fs::write(dir.0.join("A"), [1, 2, 3]).unwrap();
            let files = NativeFiles::new(&dir.0, 100, 100000).unwrap();
            let owner = files.register(b"pack", 0).unwrap();
            let mut a = files.open(b"a", SelectedSource::Archive(owner)).unwrap();
            let mut b = files.open(b"a", SelectedSource::Archive(owner)).unwrap();
            let mut out = [0; 2];
            assert_eq!(a.read(&mut out).unwrap(), 2);
            assert_eq!(out, [7, 8]);
            assert_eq!(b.read(&mut out).unwrap(), 2);
            assert_eq!(out, [7, 8]);
            a.seek(1, 0).unwrap();
            a.read(&mut out).unwrap();
            assert_eq!(out, [8, 9]);
            a.seek(1, 2).unwrap();
            assert_eq!(a.read(&mut out), Err(StreamError::Bounds));
            files.release(owner).unwrap();
            assert_eq!(a.read(&mut out), Err(StreamError::Closed));
            let mut loose = files.open(b"a", SelectedSource::Loose).unwrap();
            drop(a);
            drop(b);
            loose.read(&mut out).unwrap();
            assert_eq!(out, [1, 2]);
            loose.close().unwrap();
            assert!(files.open(b"a", SelectedSource::Archive(owner)).is_err());
            files.shutdown().unwrap();
            assert!(matches!(
                files.open(b"a", SelectedSource::Loose),
                Err(StreamError::Closed)
            ));
        }
    }
    #[test]
    fn allocation_identity_partial_reads_and_native_failures() {
        let dir = Directory::new();
        std::fs::write(dir.0.join("a"), [1, 2, 3]).unwrap();
        let mut effects = FilesystemEffects::new(&dir.0, 32768).unwrap();
        let f = effects.create_file(b"a", false).unwrap();
        assert_eq!(f, 0);
        let mut out = [9; 5];
        let r = effects.read_file(f, &mut out).unwrap();
        assert!(r.success);
        assert_eq!(r.bytes_read, 3);
        assert_eq!(out, [1, 2, 3, 9, 9]);
        assert_eq!(effects.create_file(b"../a", false).unwrap(), FAILURE);
        assert_eq!(effects.seek_read_file(f, 4).unwrap(), (FAILURE, 1));
        let token = effects.allocate_open(OpenAllocation::Block, 32768).unwrap();
        assert_ne!(token, 0);
        assert_eq!(effects.allocate_open(OpenAllocation::Owner, 12).unwrap(), 0);
        assert_eq!(effects.free_wav(token), Err(HostError::Identity));
        effects.free_block(token).unwrap();
        assert_eq!(effects.free_block(token), Err(HostError::Identity));
        assert!(effects.allocate_open(OpenAllocation::Block, 1).unwrap() > token);
        effects.close_handle(f).unwrap();
        assert!(!effects.read_file(f, &mut out).unwrap().success);
        assert!(effects.delete_file(b"a").is_err());
    }
    #[test]
    fn cache_hits_check_snapshots_and_path_quota_failures_stay_selected() {
        let dir = Directory::new();
        std::fs::write(dir.0.join("pack"), pack(false)).unwrap();
        std::fs::write(dir.0.join("a"), [1, 2, 3]).unwrap();
        let files = NativeFiles::new(&dir.0, 3, 100000).unwrap();
        let owner = files.register(b"pack", 0).unwrap();
        let mut stream = files.open(b"a", SelectedSource::Archive(owner)).unwrap();
        let mut out = [0];
        stream.read(&mut out).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(dir.0.join("pack"))
            .unwrap()
            .set_len(1)
            .unwrap();
        assert_eq!(stream.read(&mut out), Err(StreamError::Failure));
        assert_eq!(out, [7]);
        assert!(files.open(b"a", SelectedSource::Archive(owner)).is_err());
        assert!(files.open(b"../a", SelectedSource::Loose).is_err());
        assert!(files.open("é".as_bytes(), SelectedSource::Loose).is_err());
        let too_small = NativeFiles::new(&dir.0, 2, 100000).unwrap();
        assert!(matches!(
            too_small.open(b"a", SelectedSource::Loose),
            Err(StreamError::Bounds)
        ));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.0.join("a"), dir.0.join("link")).unwrap();
            assert!(files.open(b"link", SelectedSource::Loose).is_err());
        }
    }
}
