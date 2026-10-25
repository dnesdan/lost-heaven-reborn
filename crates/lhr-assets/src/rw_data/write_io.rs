//! Explicitly opted-in host writes, separate from Installation/read-only IO.
use super::*;
use std::{
    collections::BTreeMap,
    fs::{File, Metadata, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, PartialEq, Eq)]
pub enum HostWriteError {
    Policy,
    Identity,
}
struct WriteSource {
    file: File,
    path: PathBuf,
    name: Vec<u8>,
    snapshot: Metadata,
}
enum Allocation {
    Name(Vec<u8>),
    Block(Vec<u8>),
}
/// The root must be a dedicated writable output directory. This adapter never
/// acquires write access through Installation or a registered archive owner.
pub struct FilesystemWriteEffects {
    root: PathBuf,
    quota: u32,
    files: BTreeMap<u32, WriteSource>,
    allocations: BTreeMap<u64, Allocation>,
    allocated: u64,
    allocation_budget: u64,
    next_file: u32,
    next_allocation: u64,
}
impl FilesystemWriteEffects {
    pub fn new(root: &Path, quota: u32, allocation_budget: u64) -> Result<Self, HostWriteError> {
        let root = std::fs::canonicalize(root).map_err(|_| HostWriteError::Policy)?;
        if !root.is_dir() || quota > 64 * 1024 * 1024 {
            return Err(HostWriteError::Policy);
        }
        Ok(Self {
            root,
            quota,
            files: BTreeMap::new(),
            allocations: BTreeMap::new(),
            allocated: 0,
            allocation_budget,
            next_file: 0,
            next_allocation: 1,
        })
    }
    // Same canonical/case-collision/symlink/containment policy as read-only IO;
    // only the final path component may be absent when creating an output.
    fn resolve(&self, name: &[u8], create: bool) -> Option<PathBuf> {
        if name.len() >= 260 || !name.is_ascii() {
            return None;
        }
        let canonical = crate::dta::canonical(std::str::from_utf8(name).ok()?).ok()?;
        let parts = canonical.split('/').collect::<Vec<_>>();
        let mut path = self.root.clone();
        for (index, part) in parts.iter().enumerate() {
            let mut found = None;
            for child in crate::bounded_children(&path).ok()? {
                if child
                    .file_name()
                    .and_then(|v| v.to_str())
                    .is_some_and(|v| v.to_lowercase() == *part)
                    && found.replace(child).is_some()
                {
                    return None;
                }
            }
            let Some(child) = found else {
                return (create && index + 1 == parts.len()).then(|| path.join(part));
            };
            let metadata = std::fs::symlink_metadata(&child).ok()?;
            if metadata.file_type().is_symlink() {
                return None;
            }
            crate::verify_contained(&self.root, &child).ok()?;
            if index + 1 < parts.len() && !metadata.is_dir() {
                return None;
            }
            path = child;
        }
        Some(path)
    }
    fn busy(&self, path: &Path) -> bool {
        self.files.values().any(|source| source.path == path)
    }
    fn source(&mut self, id: u32) -> Option<&mut WriteSource> {
        let source = self.files.get(&id)?;
        if self.resolve(&source.name, false)? != source.path
            || !crate::dta::same_snapshot(&source.snapshot, &source.file.metadata().ok()?)
            || !crate::dta::same_snapshot(&source.snapshot, &std::fs::metadata(&source.path).ok()?)
        {
            return None;
        }
        self.files.get_mut(&id)
    }
    fn allocate(&mut self, allocation: Allocation) -> Result<u64, HostWriteError> {
        let size = match &allocation {
            Allocation::Name(bytes) | Allocation::Block(bytes) => bytes.len() as u64,
        };
        let Some(total) = self
            .allocated
            .checked_add(size)
            .filter(|&n| n <= self.allocation_budget)
        else {
            return Ok(0);
        };
        let token = self.next_allocation;
        self.next_allocation = token.checked_add(1).ok_or(HostWriteError::Identity)?;
        self.allocations.insert(token, allocation);
        self.allocated = total;
        Ok(token)
    }
    fn free(&mut self, token: u64, name: bool) -> Result<(), HostWriteError> {
        let allocation = self
            .allocations
            .get(&token)
            .ok_or(HostWriteError::Identity)?;
        if matches!(allocation, Allocation::Name(_)) != name {
            return Err(HostWriteError::Identity);
        }
        let size = match allocation {
            Allocation::Name(bytes) | Allocation::Block(bytes) => bytes.len(),
        };
        self.allocated -= size as u64;
        self.allocations.remove(&token);
        Ok(())
    }
}
impl Effects for FilesystemWriteEffects {
    type Error = HostWriteError;
    fn set_file_pointer(
        &mut self,
        file: u32,
        distance: i32,
        method: u32,
    ) -> Result<u32, Self::Error> {
        let quota = self.quota;
        let Some(source) = self.source(file) else {
            return Ok(FAILURE);
        };
        let base = match method {
            0 => 0,
            1 => match source.file.stream_position() {
                Ok(n) => n,
                Err(_) => return Ok(FAILURE),
            },
            2 => source.snapshot.len(),
            _ => return Ok(FAILURE),
        };
        let target = base as i64 + i64::from(distance);
        if target < 0 || target > i64::from(quota) {
            return Ok(FAILURE);
        }
        Ok(source
            .file
            .seek(SeekFrom::Start(target as u64))
            .ok()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(FAILURE))
    }
    fn close_handle(&mut self, file: u32) -> Result<bool, Self::Error> {
        Ok(self.files.remove(&file).is_some())
    }
    fn delete_file(&mut self, name: &[u8]) -> Result<bool, Self::Error> {
        let Some(path) = self.resolve(name, false) else {
            return Ok(false);
        };
        if self.busy(&path)
            || std::fs::metadata(&path)
                .ok()
                .is_none_or(|m| !m.is_file() || m.permissions().readonly())
        {
            return Ok(false);
        }
        Ok(std::fs::remove_file(path).is_ok())
    }
    fn file_attributes(&mut self, name: &[u8]) -> Result<u32, Self::Error> {
        Ok(self
            .resolve(name, false)
            .and_then(|p| std::fs::metadata(p).ok())
            .filter(|m| m.is_file())
            .map_or(FAILURE, |m| 0x80 | u32::from(m.permissions().readonly())))
    }
    fn set_file_attributes(&mut self, name: &[u8], attributes: u32) -> Result<bool, Self::Error> {
        let Some(path) = self.resolve(name, false) else {
            return Ok(false);
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            return Ok(false);
        };
        if !metadata.is_file() {
            return Ok(false);
        }
        let mut permissions = metadata.permissions();
        if permissions.readonly() == (attributes & 1 != 0) {
            return Ok(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = permissions.mode();
            permissions.set_mode(if attributes & 1 != 0 {
                mode & !0o222
            } else {
                mode | 0o200
            });
        }
        #[cfg(not(unix))]
        {
            permissions.set_readonly(attributes & 1 != 0);
        }
        Ok(std::fs::set_permissions(path, permissions).is_ok())
    }
    fn move_file(&mut self, from: &[u8], to: &[u8]) -> Result<bool, Self::Error> {
        let (Some(from), Some(to)) = (self.resolve(from, false), self.resolve(to, true)) else {
            return Ok(false);
        };
        // Win32 MoveFile does not replace an existing destination.
        if self.busy(&from)
            || to.exists()
            || std::fs::metadata(&from).ok().is_none_or(|m| !m.is_file())
        {
            return Ok(false);
        }
        Ok(std::fs::rename(from, to).is_ok())
    }
    fn free_name(&mut self, token: u64) -> Result<(), Self::Error> {
        self.free(token, true)
    }
    fn free_block(&mut self, token: u64) -> Result<(), Self::Error> {
        self.free(token, false)
    }
    fn free_wav(&mut self, _: u64) -> Result<(), Self::Error> {
        Err(HostWriteError::Identity)
    }
}
impl WriteEffects for FilesystemWriteEffects {
    fn write_file(&mut self, file: u32, bytes: &[u8]) -> Result<WriteFileResult, Self::Error> {
        let failed = WriteFileResult {
            success: false,
            bytes_written: 0,
        };
        let quota = u64::from(self.quota);
        let Some(source) = self.source(file) else {
            return Ok(failed);
        };
        let Ok(position) = source.file.stream_position() else {
            return Ok(failed);
        };
        if position
            .checked_add(bytes.len() as u64)
            .is_none_or(|end| end > quota)
        {
            return Ok(failed);
        }
        // One native write attempt: do not hide short writes with write_all.
        let result = source.file.write(bytes);
        if let Ok(snapshot) = source.file.metadata() {
            source.snapshot = snapshot;
        }
        Ok(match result {
            Ok(n) => WriteFileResult {
                success: true,
                bytes_written: n as u32,
            },
            Err(_) => failed,
        })
    }
}
impl OpenWriteEffects for FilesystemWriteEffects {
    fn copy_file(&mut self, from: &[u8], to: &[u8]) -> Result<bool, Self::Error> {
        let (Some(from), Some(to)) = (self.resolve(from, false), self.resolve(to, true)) else {
            return Ok(false);
        };
        if std::fs::metadata(&from)
            .ok()
            .is_some_and(|m| m.len() > u64::from(self.quota))
        {
            // A host admission exclusion must not become an ignored CopyFile
            // failure followed by promotion of an empty replacement.
            return Err(HostWriteError::Policy);
        }
        if self.busy(&from)
            || self.busy(&to)
            || to.exists()
                && std::fs::metadata(&to)
                    .ok()
                    .is_none_or(|m| !m.is_file() || m.permissions().readonly())
            || std::fs::metadata(&from)
                .ok()
                .is_none_or(|m| !m.is_file() || m.len() > u64::from(self.quota))
        {
            return Ok(false);
        }
        Ok(std::fs::copy(from, to).is_ok())
    }
    fn create_write_file(
        &mut self,
        name: &[u8],
        disposition: u32,
        attributes: u32,
    ) -> Result<u32, Self::Error> {
        if attributes != 0x80 || !matches!(disposition, 2 | 4) {
            return Err(HostWriteError::Policy);
        }
        let Some(path) = self.resolve(name, true) else {
            return Ok(FAILURE);
        };
        if self.busy(&path)
            || path.exists()
                && std::fs::metadata(&path).ok().is_none_or(|m| {
                    !m.is_file() || m.permissions().readonly() || m.len() > u64::from(self.quota)
                })
        {
            return Ok(FAILURE);
        }
        let Ok(file) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(disposition == 2)
            .open(&path)
        else {
            return Ok(FAILURE);
        };
        let snapshot = file.metadata().map_err(|_| HostWriteError::Identity)?;
        crate::verify_contained(&self.root, &path).map_err(|_| HostWriteError::Policy)?;
        let id = self.next_file;
        self.next_file = id
            .checked_add(1)
            .filter(|&n| n != FAILURE)
            .ok_or(HostWriteError::Identity)?;
        self.files.insert(
            id,
            WriteSource {
                file,
                path,
                name: name.to_vec(),
                snapshot,
            },
        );
        Ok(id)
    }
    fn allocate_temporary_name(&mut self, name: &[u8]) -> Result<u64, Self::Error> {
        let mut bytes = name.to_vec();
        bytes.push(0);
        self.allocate(Allocation::Name(bytes))
    }
    fn allocate_write_block(&mut self, size: u32) -> Result<u64, Self::Error> {
        if size != 32768 {
            return Err(HostWriteError::Policy);
        }
        self.allocate(Allocation::Block(vec![0; size as usize]))
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum WriteIoError {
    UnsupportedFlags,
    Native(OperationError<HostWriteError>),
    Failure,
    Closed,
}
impl From<OperationError<HostWriteError>> for WriteIoError {
    fn from(error: OperationError<HostWriteError>) -> Self {
        Self::Native(error)
    }
}
pub struct NativeWriteFiles {
    handles: Handles<FilesystemWriteEffects>,
}
impl NativeWriteFiles {
    pub fn new(root: &Path, quota: u32, allocation_budget: u64) -> Result<Self, HostWriteError> {
        Ok(Self {
            handles: Handles::new(FilesystemWriteEffects::new(root, quota, allocation_budget)?),
        })
    }
    /// Normal/create/buffer/change-readonly flags are supported. Hidden Win32
    /// attributes and unknown flags are explicit host exclusions before mutation.
    pub fn open(&self, name: &[u8], flags: u32) -> Result<NativeWriteStream<'_>, WriteIoError> {
        if flags & !(DTA_CREATE_FILE | DTA_FILE_BUFFER | DTA_CHANGE_ATTRIBUTES) != 0 {
            return Err(WriteIoError::UnsupportedFlags);
        }
        {
            let state = self
                .handles
                .state
                .lock()
                .map_err(|_| WriteIoError::Native(OperationError::LockPoisoned))?;
            let path = state
                .effects
                .resolve(name, true)
                .ok_or(WriteIoError::Failure)?;
            if state.effects.busy(&path)
                || path.exists()
                    && std::fs::metadata(&path)
                        .ok()
                        .is_none_or(|m| !m.is_file() || m.len() > u64::from(state.effects.quota))
            {
                return Err(WriteIoError::Failure);
            }
        }
        let handle = self.handles.dta_open_write(name, flags)?;
        if handle == FAILURE {
            return Err(WriteIoError::Failure);
        }
        Ok(NativeWriteStream {
            files: self,
            handle: Some(handle),
        })
    }
}
pub struct NativeWriteStream<'a> {
    files: &'a NativeWriteFiles,
    handle: Option<u32>,
}
impl NativeWriteStream<'_> {
    pub fn write(&mut self, bytes: &[u8]) -> Result<u32, WriteIoError> {
        let count = self
            .files
            .handles
            .dta_write(self.handle.ok_or(WriteIoError::Closed)?, Some(bytes))?;
        if count == FAILURE {
            Err(WriteIoError::Failure)
        } else {
            Ok(count)
        }
    }
    pub fn seek(&mut self, distance: i32, method: u32) -> Result<u32, WriteIoError> {
        let position = self.files.handles.dta_seek(
            self.handle.ok_or(WriteIoError::Closed)?,
            distance,
            method,
        )?;
        if position == FAILURE {
            Err(WriteIoError::Failure)
        } else {
            Ok(position)
        }
    }
    pub fn cursor(&self) -> Result<ReadCursor, WriteIoError> {
        Ok(self
            .files
            .handles
            .inspect(self.handle.ok_or(WriteIoError::Closed)?)?
            .ok_or(WriteIoError::Failure)?
            .cursor)
    }
    /// Follow native close: attempt promotion after successful buffered writes,
    /// or discard the temporary copy after a failed/short write. Delete/move
    /// boolean failures are ignored by native dtaClose; success is not a promise
    /// of atomic publication or successful promotion.
    pub fn close(&mut self) -> Result<(), WriteIoError> {
        let Some(handle) = self.handle else {
            return Ok(());
        };
        if !self.files.handles.dta_close(handle)? {
            return Err(WriteIoError::Failure);
        }
        self.handle = None;
        Ok(())
    }
}
impl Drop for NativeWriteStream<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "lhr-write-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn real_write_seek_cursor_truncate_and_last_close_share_native_handle_table() {
        let dir = Directory::new();
        std::fs::write(dir.0.join("Out"), b"abcdef").unwrap();
        let files = NativeWriteFiles::new(&dir.0, 100, 100000).unwrap();
        let mut stream = files.open(b"out", 0).unwrap();
        assert_eq!(stream.handle, Some(0));
        assert_eq!(stream.write(b"XY"), Ok(2));
        assert_eq!(stream.cursor().unwrap().position(), 0);
        assert_eq!(stream.cursor().unwrap().size(), 0);
        assert_eq!(stream.seek(0, 1), Ok(2));
        assert_eq!(stream.write(b"Z"), Ok(1));
        assert_eq!(stream.cursor().unwrap().position(), 2);
        // Failed BEGIN falls through to CURRENT, exactly like native dtaSeek.
        assert_eq!(stream.seek(-1, 0), Ok(2));
        assert_eq!(stream.write(b"!"), Ok(1));
        stream.close().unwrap();
        assert_eq!(stream.write(b"x"), Err(WriteIoError::Closed));
        assert_eq!(std::fs::read(dir.0.join("Out")).unwrap(), b"XY!def");
        let mut next = files.open(b"OUT", DTA_CREATE_FILE).unwrap();
        assert_eq!(next.handle, Some(0));
        assert_eq!(next.write(b"short"), Ok(5));
        next.close().unwrap();
        assert_eq!(std::fs::read(dir.0.join("Out")).unwrap(), b"short");
        let state = files.handles.state.lock().unwrap();
        assert!(state.effects.files.is_empty());
        assert!(state.effects.allocations.is_empty());
        assert_eq!(state.effects.allocated, 0);
    }
    #[test]
    fn buffered_copy_create_promotion_and_failure_discard_use_real_files() {
        let dir = Directory::new();
        std::fs::write(dir.0.join("out"), b"abcdef").unwrap();
        let files = NativeWriteFiles::new(&dir.0, 8, 100000).unwrap();
        {
            let mut stream = files.open(b"out", DTA_FILE_BUFFER).unwrap();
            assert_eq!(stream.write(b"XY"), Ok(2));
            assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"abcdef");
            assert_eq!(std::fs::read(dir.0.join("out.tmp")).unwrap(), b"XYcdef");
        } // Drop runs the same native close/promotion.
        assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"XYcdef");
        assert!(!dir.0.join("out.tmp").exists());
        let mut stream = files
            .open(b"out", DTA_FILE_BUFFER | DTA_CREATE_FILE)
            .unwrap();
        assert_eq!(stream.write(b"new"), Ok(3));
        assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"XYcdef");
        stream.close().unwrap();
        assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"new");
        let mut stream = files.open(b"out", DTA_FILE_BUFFER).unwrap();
        assert_eq!(stream.write(b"bad"), Ok(3));
        assert_eq!(stream.write(b"too large"), Err(WriteIoError::Failure));
        let record = files
            .handles
            .inspect(stream.handle.unwrap())
            .unwrap()
            .unwrap();
        assert_ne!(record.flags & 0x80, 0);
        stream.close().unwrap();
        assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"new");
        assert!(!dir.0.join("out.tmp").exists());
        let mut stream = files.open(b"absent", DTA_FILE_BUFFER).unwrap();
        assert_eq!(stream.write(b"ok"), Ok(2));
        stream.close().unwrap();
        assert_eq!(std::fs::read(dir.0.join("absent")).unwrap(), b"ok");
        let mut stream = files.open(b"out", DTA_FILE_BUFFER).unwrap();
        assert_eq!(
            files.handles.dta_write(stream.handle.unwrap(), None),
            Ok(FAILURE)
        );
        stream.close().unwrap();
        assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"new");
    }
    #[test]
    fn write_guards_reject_unsafe_paths_collision_external_mutation_and_hidden_flags() {
        let dir = Directory::new();
        let files = NativeWriteFiles::new(&dir.0, 100, 100000).unwrap();
        for name in [
            b"../escape".as_slice(),
            b"/escape",
            b"C:escape",
            b"x/../escape",
            b"x\0",
            b"",
            b"tail.",
        ] {
            assert!(files.open(name, DTA_CREATE_FILE).is_err(), "{name:?}");
        }
        assert!(matches!(
            files.open(b"hidden", DTA_TEMPORARY),
            Err(WriteIoError::UnsupportedFlags)
        ));
        assert!(!dir.0.join("hidden").exists());
        #[cfg(target_os = "linux")]
        {
            std::fs::write(dir.0.join("x"), b"lower").unwrap();
            std::fs::write(dir.0.join("X"), b"upper").unwrap();
            assert!(files.open(b"x", DTA_CREATE_FILE).is_err());
            assert_eq!(std::fs::read(dir.0.join("x")).unwrap(), b"lower");
            assert_eq!(std::fs::read(dir.0.join("X")).unwrap(), b"upper");
        }
        let mut stream = files.open(b"own", DTA_CREATE_FILE).unwrap();
        assert_eq!(stream.write(b"a"), Ok(1));
        assert!(files.open(b"own", DTA_CREATE_FILE).is_err());
        std::fs::write(dir.0.join("own"), b"external").unwrap();
        assert_eq!(stream.write(b"b"), Err(WriteIoError::Failure));
        stream.close().unwrap();
        assert_eq!(std::fs::read(dir.0.join("own")).unwrap(), b"external");
    }
    #[cfg(unix)]
    #[test]
    fn write_symlinks_and_readonly_retry_preserve_containment_and_permission_bits() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = Directory::new();
        let outside = Directory::new();
        std::fs::write(outside.0.join("keep"), b"outside").unwrap();
        symlink(outside.0.join("keep"), dir.0.join("link")).unwrap();
        symlink(&outside.0, dir.0.join("sub")).unwrap();
        symlink(outside.0.join("keep"), dir.0.join("out.tmp")).unwrap();
        let files = NativeWriteFiles::new(&dir.0, 100, 100000).unwrap();
        for (name, flags) in [
            (b"link".as_slice(), DTA_CREATE_FILE),
            (b"sub/keep", DTA_CREATE_FILE),
            (b"out", DTA_FILE_BUFFER | DTA_CREATE_FILE),
        ] {
            assert!(files.open(name, flags).is_err());
        }
        assert_eq!(std::fs::read(outside.0.join("keep")).unwrap(), b"outside");
        std::fs::write(dir.0.join("readonly"), b"old").unwrap();
        std::fs::set_permissions(
            dir.0.join("readonly"),
            std::fs::Permissions::from_mode(0o440),
        )
        .unwrap();
        assert!(files.open(b"readonly", DTA_CREATE_FILE).is_err());
        let mut stream = files
            .open(b"readonly", DTA_CREATE_FILE | DTA_CHANGE_ATTRIBUTES)
            .unwrap();
        assert_eq!(stream.write(b"new"), Ok(3));
        stream.close().unwrap();
        assert_eq!(std::fs::read(dir.0.join("readonly")).unwrap(), b"new");
        assert_eq!(
            std::fs::metadata(dir.0.join("readonly"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
    #[test]
    fn allocation_failure_preserves_created_file_prefix_without_fake_success_or_rollback() {
        let dir = Directory::new();
        std::fs::write(dir.0.join("out"), b"before").unwrap();
        let files = NativeWriteFiles::new(&dir.0, 100, 0).unwrap();
        assert!(matches!(
            files.open(b"out", DTA_CREATE_FILE),
            Err(WriteIoError::Failure)
        ));
        assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"");
        let state = files.handles.state.lock().unwrap();
        assert_eq!(state.effects.files.len(), 1); // Source retains this failed-open prefix.
        assert!(state.records.is_empty());
        assert_eq!(state.effects.allocated, 0);
        drop(state);
        drop(files); // Host disposal closes owned OS resources, without invented restoration.
        let files = NativeWriteFiles::new(&dir.0, 100, 0).unwrap();
        assert!(files.open(b"out", DTA_FILE_BUFFER).is_err());
        assert!(dir.0.join("out.tmp").exists());
        assert_eq!(std::fs::read(dir.0.join("out")).unwrap(), b"");
    }
    #[test]
    fn host_admission_failures_cannot_be_ignored_into_empty_buffered_replacements() {
        let dir = Directory::new();
        std::fs::write(dir.0.join("large"), b"too large").unwrap();
        std::fs::create_dir(dir.0.join("directory")).unwrap();
        let files = NativeWriteFiles::new(&dir.0, 2, 100000).unwrap();
        for name in [b"large".as_slice(), b"directory"] {
            assert!(files.open(name, DTA_FILE_BUFFER).is_err());
            assert!(
                !dir.0
                    .join(format!("{}.tmp", std::str::from_utf8(name).unwrap()))
                    .exists()
            );
        }
        assert_eq!(std::fs::read(dir.0.join("large")).unwrap(), b"too large");
        let mut stream = files.open(b"allowed", DTA_CREATE_FILE).unwrap();
        assert_eq!(stream.write(b"ok"), Ok(2));
        stream.close().unwrap();
    }
}
