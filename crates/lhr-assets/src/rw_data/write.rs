//! Source-supported dtaOpenWrite/dtaWrite using the existing shared handle,
//! seek and close machinery. Host filesystem ownership is a separate adapter.
use super::*;

pub const DTA_TEMPORARY: u32 = 1;
pub const DTA_FILE_BUFFER: u32 = 0x100;
pub const DTA_CREATE_FILE: u32 = 0x10000;
pub const DTA_CHANGE_ATTRIBUTES: u32 = 0x100000;

/// Native CreateFileA words selected by dtaOpenWrite (rw_data.dll RVA 0x39f0,
/// REA ev_27c70d9c). Values are the exact binary immediates; names follow
/// Win32, not original symbols (original names unknown).
pub const NATIVE_CREATE_ALWAYS: u32 = 2;
pub const NATIVE_OPEN_ALWAYS: u32 = 4;
pub const NATIVE_FILE_ATTRIBUTE_HIDDEN: u32 = 2;
pub const NATIVE_FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
/// Native handle flag bits observed at RVA 0x3f50 (REA ev_9aa51e71,
/// TEST CL,3 gate; OR CL,0x80 discard mark).
pub const NATIVE_FH_FROM_FILESYSTEM: u32 = 1;
pub const NATIVE_FH_WRITE: u32 = 2;
pub const NATIVE_FH_BUFFER: u32 = 4;
pub const NATIVE_FH_FAILED_WRITING_TO_BUFFER: u32 = 0x80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteFileResult {
    pub success: bool,
    pub bytes_written: u32,
}
pub trait WriteEffects: Effects {
    fn write_file(&mut self, file: u32, bytes: &[u8]) -> Result<WriteFileResult, Self::Error>;
}
pub trait OpenWriteEffects: WriteEffects {
    fn copy_file(&mut self, from: &[u8], to: &[u8]) -> Result<bool, Self::Error>;
    /// Native disposition: CREATE_ALWAYS=2 or OPEN_ALWAYS=4. Attributes are
    /// HIDDEN=2 or NORMAL=0x80. A failed native open returns FAILURE.
    fn create_write_file(
        &mut self,
        name: &[u8],
        disposition: u32,
        attributes: u32,
    ) -> Result<u32, Self::Error>;
    fn allocate_temporary_name(&mut self, name: &[u8]) -> Result<u64, Self::Error>;
    fn allocate_write_block(&mut self, size: u32) -> Result<u64, Self::Error>;
}
macro_rules! effect {
    ($s:expr, $stage:ident, $call:expr) => {
        match $call {
            Ok(value) => value,
            Err(error) => {
                $s.interrupted = Some(EffectStage::$stage);
                return Err(OperationError::Backend(error));
            }
        }
    };
}
impl<B: OpenWriteEffects> Handles<B> {
    /// Original dtaOpenWrite (rw_data.dll RVA 0x39f0). Native failures preserve
    /// completed filesystem and allocation effects; they are not rolled back
    /// into an invented transaction. Native effect order: copy/delete tmp →
    /// CreateFile → [GetFileAttributes → SetFileAttributes → CreateFile retry]
    /// → tmp-name alloc → 32768B block alloc → first-free slot reuse else push.
    /// Allocation failure retains the OS handle without a record (native leak).
    pub fn dta_open_write(&self, name: &[u8], flags: u32) -> Result<u32, OperationError<B::Error>> {
        if name.len() >= 260 || !name.is_ascii() || name.contains(&0) {
            return Err(OperationError::UnsupportedOpen);
        }
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = s.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let attributes = if flags & DTA_TEMPORARY != 0 {
            NATIVE_FILE_ATTRIBUTE_HIDDEN
        } else {
            NATIVE_FILE_ATTRIBUTE_NORMAL
        };
        let disposition = if flags & DTA_CREATE_FILE != 0 {
            NATIVE_CREATE_ALWAYS
        } else {
            NATIVE_OPEN_ALWAYS
        };
        let mut temporary = Vec::new();
        let path = if flags & DTA_FILE_BUFFER != 0 {
            temporary.extend(name);
            temporary.extend(b".tmp");
            // Equality is equivalent to the native `disposition & OPEN_ALWAYS`
            // test for the two produced values (CREATE_ALWAYS=2 has bit 2 clear).
            if disposition == NATIVE_OPEN_ALWAYS {
                effect!(s, CopyFile, s.effects.copy_file(name, &temporary));
            } else {
                effect!(s, DeleteFile, s.effects.delete_file(&temporary));
            }
            temporary.as_slice()
        } else {
            name
        };
        let mut file = effect!(
            s,
            Open,
            s.effects.create_write_file(path, disposition, attributes)
        );
        if file == FAILURE && flags & DTA_CHANGE_ATTRIBUTES != 0 {
            let current = effect!(s, FileAttributes, s.effects.file_attributes(path));
            if current == FAILURE {
                return Ok(FAILURE);
            }
            effect!(
                s,
                SetFileAttributes,
                s.effects.set_file_attributes(path, current & !1)
            );
            file = effect!(
                s,
                Open,
                s.effects.create_write_file(path, disposition, attributes)
            );
        }
        if file == FAILURE {
            return Ok(FAILURE);
        }
        let temporary_name = if flags & DTA_FILE_BUFFER != 0 {
            let token = effect!(s, Allocate, s.effects.allocate_temporary_name(&temporary));
            if token == 0 {
                return Ok(FAILURE);
            }
            if !s.admitted_allocations.insert(token) {
                return Err(OperationError::InvalidRecord);
            }
            token
        } else {
            0
        };
        let block = effect!(s, Allocate, s.effects.allocate_write_block(32768));
        if block == 0 {
            return Ok(FAILURE);
        }
        open::store(
            &mut s,
            HandleRecord {
                file,
                flags: if temporary_name != 0 {
                    NATIVE_FH_FROM_FILESYSTEM | NATIVE_FH_WRITE | NATIVE_FH_BUFFER
                } else {
                    NATIVE_FH_FROM_FILESYSTEM | NATIVE_FH_WRITE
                },
                cursor: ReadCursor::new(0),
                cache_valid: false,
                temporary_name,
                temporary_name_bytes: temporary,
                block,
                wav: 0,
                timestamp: 0,
            },
            None,
            None,
        )
    }
}
impl<B: WriteEffects> Handles<B> {
    /// Original dtaWrite (rw_data.dll RVA 0x3f50). A successful write advances
    /// the OS file position only; it does not update the record's logical
    /// file_pointer or size. A failed or short buffered write sets the discard
    /// flag consumed by dtaClose.
    pub fn dta_write(
        &self,
        handle: u32,
        bytes: Option<&[u8]>,
    ) -> Result<u32, OperationError<B::Error>> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        if let Some(stage) = s.interrupted {
            return Err(OperationError::Interrupted(stage));
        }
        let Some(record) = s.records.get_mut(handle as usize) else {
            return Ok(FAILURE);
        };
        // The source tests either filesystem/write bit, not that both are set.
        if record.file == FAILURE
            || record.flags & (NATIVE_FH_FROM_FILESYSTEM | NATIVE_FH_WRITE) == 0
        {
            return Ok(FAILURE);
        }
        let Some(bytes) = bytes else {
            if record.flags & NATIVE_FH_BUFFER != 0 {
                record.flags |= NATIVE_FH_FAILED_WRITING_TO_BUFFER;
            }
            return Ok(FAILURE);
        };
        let len = u32::try_from(bytes.len()).map_err(|_| OperationError::InvalidRecord)?;
        let file = record.file;
        let written = effect!(s, Write, s.effects.write_file(file, bytes));
        if !written.success || written.bytes_written != len {
            let record = &mut s.records[handle as usize];
            if record.flags & NATIVE_FH_BUFFER != 0 {
                record.flags |= NATIVE_FH_FAILED_WRITING_TO_BUFFER;
            }
            return Ok(FAILURE);
        }
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Trace {
        calls: Vec<String>,
        allocation: u64,
        written: Vec<u8>,
        mode: u8,
        fail_open: bool,
        ignored_close_failures: bool,
    }
    struct Backend(Arc<Mutex<Trace>>);
    impl Backend {
        fn call(&self, call: &str) {
            self.0.lock().unwrap().calls.push(call.into());
        }
    }
    impl Effects for Backend {
        type Error = ();
        fn set_file_pointer(&mut self, _: u32, _: i32, _: u32) -> Result<u32, ()> {
            self.call("seek");
            Ok(7)
        }
        fn close_handle(&mut self, _: u32) -> Result<bool, ()> {
            self.call("close");
            Ok(true)
        }
        fn delete_file(&mut self, name: &[u8]) -> Result<bool, ()> {
            self.call(&format!("delete {}", String::from_utf8_lossy(name)));
            Ok(!self.0.lock().unwrap().ignored_close_failures)
        }
        fn file_attributes(&mut self, _: &[u8]) -> Result<u32, ()> {
            self.call("attributes");
            Ok(0x81)
        }
        fn set_file_attributes(&mut self, _: &[u8], attributes: u32) -> Result<bool, ()> {
            self.call(&format!("set-attributes {attributes}"));
            Ok(false)
        }
        fn move_file(&mut self, _: &[u8], _: &[u8]) -> Result<bool, ()> {
            self.call("move");
            Ok(false)
        }
        fn free_name(&mut self, _: u64) -> Result<(), ()> {
            self.call("free-name");
            Ok(())
        }
        fn free_block(&mut self, _: u64) -> Result<(), ()> {
            self.call("free-block");
            Ok(())
        }
        fn free_wav(&mut self, _: u64) -> Result<(), ()> {
            unreachable!()
        }
    }
    impl OpenWriteEffects for Backend {
        fn copy_file(&mut self, _: &[u8], _: &[u8]) -> Result<bool, ()> {
            self.call("copy");
            Ok(false)
        }
        fn create_write_file(
            &mut self,
            _: &[u8],
            disposition: u32,
            attributes: u32,
        ) -> Result<u32, ()> {
            self.call(&format!("open {disposition} {attributes}"));
            let mut trace = self.0.lock().unwrap();
            let fail = trace.fail_open;
            trace.fail_open = false;
            Ok(if fail { FAILURE } else { 100 })
        }
        fn allocate_temporary_name(&mut self, _: &[u8]) -> Result<u64, ()> {
            self.call("name");
            let mut trace = self.0.lock().unwrap();
            trace.allocation += 1;
            Ok(trace.allocation)
        }
        fn allocate_write_block(&mut self, _: u32) -> Result<u64, ()> {
            self.call("block");
            let mut trace = self.0.lock().unwrap();
            trace.allocation += 1;
            Ok(trace.allocation)
        }
    }
    impl WriteEffects for Backend {
        fn write_file(&mut self, _: u32, bytes: &[u8]) -> Result<WriteFileResult, ()> {
            let mut trace = self.0.lock().unwrap();
            trace.calls.push("write".into());
            let (count, success) = match trace.mode {
                0 => (bytes.len(), true),
                1 => (2.min(bytes.len()), true),
                2 => (1.min(bytes.len()), false),
                _ => (1.min(bytes.len()), false),
            };
            trace.written.extend(&bytes[..count]);
            if trace.mode == 3 {
                return Err(());
            }
            Ok(WriteFileResult {
                success,
                bytes_written: count as u32,
            })
        }
    }
    #[test]
    fn native_write_short_failure_and_null_set_discard_preserve_output_and_numeric_cursor() {
        for mode in [1, 2] {
            let trace = Arc::new(Mutex::new(Trace {
                mode,
                ..Trace::default()
            }));
            let handles = Handles::new(Backend(trace.clone()));
            let handle = handles.dta_open_write(b"out", DTA_FILE_BUFFER).unwrap();
            assert_eq!(handles.dta_seek(handle, 0, 1), Ok(7));
            assert_eq!(handles.dta_write(handle, Some(b"abc")), Ok(FAILURE));
            let record = handles.inspect(handle).unwrap().unwrap();
            assert_eq!(record.cursor.position(), 7);
            assert_eq!(record.cursor.size(), 0);
            assert_ne!(record.flags & 0x80, 0);
            assert_eq!(
                trace.lock().unwrap().written,
                if mode == 1 {
                    b"ab".to_vec()
                } else {
                    b"a".to_vec()
                }
            );
            assert_eq!(handles.dta_close(handle), Ok(true));
            let calls = &trace.lock().unwrap().calls;
            assert!(calls.iter().any(|call| call == "delete out.tmp"));
            assert!(!calls.iter().any(|call| call == "move"));
        }
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = Handles::new(Backend(trace.clone()));
        let handle = handles.dta_open_write(b"out", DTA_FILE_BUFFER).unwrap();
        assert_eq!(handles.dta_write(handle, None), Ok(FAILURE));
        assert_eq!(handles.dta_write(handle, Some(&[])), Ok(0));
        assert_eq!(handles.dta_write(99, Some(b"x")), Ok(FAILURE));
        assert_ne!(handles.inspect(handle).unwrap().unwrap().flags & 0x80, 0);
    }
    #[test]
    fn native_open_retry_and_close_keep_ignored_results_and_effect_order() {
        let trace = Arc::new(Mutex::new(Trace {
            fail_open: true,
            ignored_close_failures: true,
            ..Trace::default()
        }));
        let handles = Handles::new(Backend(trace.clone()));
        let handle = handles
            .dta_open_write(b"out", DTA_FILE_BUFFER | DTA_CHANGE_ATTRIBUTES)
            .unwrap();
        assert_eq!(handle, 0);
        assert_eq!(handles.dta_write(handle, Some(b"abc")), Ok(3));
        assert_eq!(handles.dta_close(handle), Ok(true));
        assert_eq!(
            trace.lock().unwrap().calls,
            [
                "copy",
                "open 4 128",
                "attributes",
                "set-attributes 128",
                "open 4 128",
                "name",
                "block",
                "write",
                "close",
                "delete out",
                "attributes",
                "set-attributes 128",
                "delete out",
                "move",
                "free-name",
                "free-block",
            ]
        );
        let record = handles.inspect(handle).unwrap().unwrap();
        assert_eq!(record.file, FAILURE);
        assert_eq!(record.block, 0);
        // Native leaves the freed temporary-name pointer stale behind invalid file.
        assert_ne!(record.temporary_name, 0);
    }
    #[test]
    fn open_write_create_always_deletes_tmp_and_marks_buffered() {
        // Disposition CREATE_ALWAYS (REA ev_27c70d9c: TEST BL,4 false branch):
        // DeleteFile on the tmp name, result ignored, then open with 2/HIDDEN-or-NORMAL.
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = Handles::new(Backend(trace.clone()));
        let handle = handles
            .dta_open_write(b"out", DTA_FILE_BUFFER | DTA_CREATE_FILE)
            .unwrap();
        assert_ne!(handle, FAILURE);
        assert_eq!(
            &trace.lock().unwrap().calls[..3],
            ["delete out.tmp", "open 2 128", "name"]
        );
        let record = handles.inspect(handle).unwrap().unwrap();
        assert_eq!(record.temporary_name_bytes, b"out.tmp");
        assert_eq!(
            record.flags & (NATIVE_FH_FROM_FILESYSTEM | NATIVE_FH_WRITE | NATIVE_FH_BUFFER),
            NATIVE_FH_FROM_FILESYSTEM | NATIVE_FH_WRITE | NATIVE_FH_BUFFER
        );
        assert_eq!(handles.dta_close(handle), Ok(true));
    }
    #[test]
    fn temporary_flag_flows_hidden_attribute_to_backend() {
        // Native passes HIDDEN=2 to CreateFileA for DTA_TEMPORARY (REA ev_27c70d9c
        // attr formula); this host's write adapter rejects non-NORMAL attributes
        // as Policy (write_io.rs), so the effect must still carry 2 here.
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = Handles::new(Backend(trace.clone()));
        let handle = handles.dta_open_write(b"t", DTA_TEMPORARY).unwrap();
        assert_ne!(handle, FAILURE);
        assert!(trace.lock().unwrap().calls.iter().any(|c| c == "open 4 2"));
        assert_eq!(handles.dta_close(handle), Ok(true));
    }
    #[test]
    fn null_write_marks_discard_only_on_buffered_handles() {
        let trace = Arc::new(Mutex::new(Trace::default()));
        let handles = Handles::new(Backend(trace.clone()));
        // Buffered handle: null sets the discard mark (OR 0x80).
        let buffered = handles.dta_open_write(b"b", DTA_FILE_BUFFER).unwrap();
        assert_eq!(handles.dta_write(buffered, None), Ok(FAILURE));
        assert_ne!(
            handles.inspect(buffered).unwrap().unwrap().flags & NATIVE_FH_FAILED_WRITING_TO_BUFFER,
            0
        );
        // Direct handle: null fails without the discard mark.
        let direct = handles.dta_open_write(b"d", DTA_CREATE_FILE).unwrap();
        assert_eq!(handles.dta_write(direct, None), Ok(FAILURE));
        assert_eq!(
            handles.inspect(direct).unwrap().unwrap().flags & NATIVE_FH_FAILED_WRITING_TO_BUFFER,
            0
        );
    }
    #[test]
    fn interrupted_write_keeps_actual_output_prefix_and_forbids_replay() {
        let trace = Arc::new(Mutex::new(Trace {
            mode: 3,
            ..Trace::default()
        }));
        let handles = Handles::new(Backend(trace.clone()));
        let handle = handles
            .dta_open_write(b"out", DTA_FILE_BUFFER | DTA_CREATE_FILE)
            .unwrap();
        assert_eq!(
            handles.dta_write(handle, Some(b"abc")),
            Err(OperationError::Backend(()))
        );
        assert_eq!(trace.lock().unwrap().written, b"a");
        assert_eq!(
            handles.inspect(handle).unwrap().unwrap().cursor.position(),
            0
        );
        assert_eq!(
            handles.dta_close(handle),
            Err(OperationError::Interrupted(EffectStage::Write))
        );
        assert_eq!(
            handles.dta_write(handle, Some(b"abc")),
            Err(OperationError::Interrupted(EffectStage::Write))
        );
        assert_eq!(
            trace
                .lock()
                .unwrap()
                .calls
                .iter()
                .filter(|call| *call == "write")
                .count(),
            1
        );
    }
}
