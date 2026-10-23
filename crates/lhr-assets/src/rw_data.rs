//! rw_data.dll dtaSeek (PC RVA 0x3730) and dtaClose (PC RVA 0x23e0).
//!
//! Snapshot admission, copying and release below are host adapters, not dtaOpen,
//! dtaRead or dtaClose reconstructions. They do not reproduce archive streaming,
//! cache errors, native ownership or installation priority selection. The mutex
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectStage {
    Seek,
    CloseHandle,
    DeleteFile,
    FileAttributes,
    SetFileAttributes,
    MoveFile,
    FreeName,
    FreeBlock,
    FreeWav,
}
struct NativeState<B> {
    records: Vec<HandleRecord>,
    effects: B,
    interrupted: Option<EffectStage>,
    admitted_allocations: BTreeSet<u64>,
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
                effects,
                interrupted: None,
                admitted_allocations: BTreeSet::new(),
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
            return Ok(index as u32);
        }
        let index = u32::try_from(state.records.len())
            .ok()
            .filter(|&v| v != FAILURE)
            .ok_or(OperationError::HandleSpaceExhausted)?;
        state.records.push(record);
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
        }
        result
    }
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
}
