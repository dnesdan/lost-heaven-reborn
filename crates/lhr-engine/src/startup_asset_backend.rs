//! Read-only host bridge for the archive/file prefix of original startup.
//! Each successful operation owns a real rw_data resource. Platform graphics,
//! desktop messages and CD discovery remain explicit capability boundaries.
//! This adapter itself has no original-function coverage credit.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

use lhr_assets::rw_data::io::{NativeFiles, NativeStream, SelectedSource, StreamError};

use crate::original_system::{
    GraphInitDescriptor, SystemBackend, SystemDomainId, SystemEffect, SystemError, SystemHandle,
};

struct ArchiveOwner {
    handle: SystemHandle,
    native: u64,
    unlocked: bool,
}

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Borrowing the provider keeps streams tied to their genuine resource owner.
/// Dropping this bridge closes its streams through NativeStream; archive Release
/// remains an explicit caller operation, and provider Drop is host disposal.
pub struct StartupAssetBackend<'a> {
    provider: &'a NativeFiles,
    domain: SystemDomainId,
    archives: Vec<ArchiveOwner>,
    streams: BTreeMap<u64, NativeStream<'a>>,
    dta_first: bool,
    patches: crate::program_patch::ProgramPatchTable,
}

impl<'a> StartupAssetBackend<'a> {
    pub fn new(provider: &'a NativeFiles) -> Self {
        Self {
            provider,
            domain: SystemDomainId::default(),
            archives: Vec::new(),
            streams: BTreeMap::new(),
            dta_first: false,
            patches: Default::default(),
        }
    }

    /// Pass this live slice to mission_actor_stream/CProgram loading so the
    /// existing reverse-priority replacement loop uses the startup patch data.
    pub fn program_replacements(&self) -> &[crate::original_program::ProgramReplacement] {
        self.patches.replacements()
    }
    pub fn unload_patch_files(&mut self) {
        self.patches.unload();
    }

    pub fn archive_count(&self) -> usize {
        self.archives.len()
    }

    pub fn open_file_count(&self) -> usize {
        self.streams.len()
    }

    /// C_chunk::OpenFile read prefix: genuine seek-to-end, then rewind.
    pub fn read_chunk_file(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>, SystemError> {
        let Some(handle) = self.open_file(path)? else {
            return Ok(None);
        };
        self.consume_chunk_file(handle).map(Some)
    }
    fn consume_chunk_file(&mut self, handle: SystemHandle) -> Result<Vec<u8>, SystemError> {
        let result = (|| {
            let stream = self
                .streams
                .get_mut(&handle.0)
                .ok_or(SystemError::InvalidReply("foreign chunk file handle"))?;
            let size = stream.seek(0, 2).map_err(provider_error)?;
            stream.seek(0, 0).map_err(provider_error)?;
            if size > 64 * 1024 * 1024 {
                return Err(SystemError::Unsupported("chunk file quota"));
            }
            let bytes = self.read_file(handle, size as usize)?;
            if bytes.len() != size as usize {
                return Err(SystemError::Backend("short chunk file read"));
            }
            Ok(bytes)
        })();
        let close = self.effect(SystemEffect::CloseFile(handle));
        match result {
            Ok(bytes) => {
                close?;
                Ok(bytes)
            }
            Err(error) => {
                let _ = close;
                Err(error)
            }
        }
    }

    fn reserve_handle(&mut self) -> Result<SystemHandle, SystemError> {
        // Distinct bridges never alias opaque resource IDs, even when their
        // underlying native tables both begin with token one.
        NEXT_HANDLE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map(SystemHandle)
            .map_err(|_| SystemError::Backend("startup handle identity exhausted"))
    }
}

fn provider_error(_: StreamError) -> SystemError {
    // Policy, snapshot, admission and interrupted-operation errors must never
    // become a native success or a request to try an older/loose copy.
    SystemError::Backend("startup archive/file provider interrupted")
}

fn canonical_path(path: &[u8]) -> Result<String, SystemError> {
    if !path.is_ascii() || path.len() >= 260 {
        return Err(SystemError::Unsupported("startup ASCII filename domain"));
    }
    let path = std::str::from_utf8(path).unwrap().replace('\\', "/");
    if path.bytes().any(|b| b.is_ascii_control() || b == b':')
        || path.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' '])
        })
    {
        return Err(SystemError::Unsupported("startup relative filename domain"));
    }
    Ok(path.to_ascii_lowercase())
}

impl SystemBackend for StartupAssetBackend<'_> {
    fn domain_id(&self) -> SystemDomainId {
        self.domain
    }

    fn effect(&mut self, effect: SystemEffect) -> Result<(), SystemError> {
        match effect {
            SystemEffect::UnloadPatchFiles => {
                self.patches.unload();
                Ok(())
            }
            SystemEffect::LoadPatchFile(path) => {
                if let Some(bytes) = self.read_chunk_file(path)? {
                    self.patches
                        .load(&bytes)
                        .map_err(|_| SystemError::Backend("invalid patch chunk data"))?;
                }
                Ok(())
            }
            SystemEffect::DtaFirstForce => {
                self.provider.dta_first_force().map_err(provider_error)?;
                self.dta_first = true;
                Ok(())
            }
            SystemEffect::UnlockArchive(handle, key) => {
                let archive = self
                    .archives
                    .iter_mut()
                    .find(|owner| owner.handle == handle)
                    .ok_or(SystemError::InvalidReply("foreign startup archive handle"))?;
                self.provider
                    .unlock_archive(archive.native, key)
                    .map_err(provider_error)?;
                archive.unlocked = true;
                Ok(())
            }
            SystemEffect::ReleaseArchive(handle) => {
                let index = self
                    .archives
                    .iter()
                    .position(|owner| owner.handle == handle)
                    .ok_or(SystemError::InvalidReply("foreign startup archive handle"))?;
                let refs = self
                    .provider
                    .release(self.archives[index].native)
                    .map_err(provider_error)?;
                if refs != Some(0) {
                    return Err(SystemError::InvalidReply("startup archive last Release"));
                }
                self.archives.remove(index);
                Ok(())
            }
            SystemEffect::CloseFile(handle) => {
                let stream = self
                    .streams
                    .get_mut(&handle.0)
                    .ok_or(SystemError::InvalidReply("foreign startup file handle"))?;
                stream.close().map_err(provider_error)?;
                self.streams.remove(&handle.0);
                Ok(())
            }
            _ => Err(SystemError::Unsupported("startup platform/system effect")),
        }
    }

    fn create_archive(&mut self, path: &[u8]) -> Result<Option<SystemHandle>, SystemError> {
        canonical_path(path)?;
        let handle = self.reserve_handle()?;
        let Some(native) = self.provider.create_archive(path).map_err(provider_error)? else {
            return Ok(None);
        };
        self.archives.push(ArchiveOwner {
            handle,
            native,
            unlocked: false,
        });
        Ok(Some(handle))
    }

    fn find_game_file(&mut self, _: &[u8]) -> Result<Option<Vec<u8>>, SystemError> {
        Err(SystemError::Unsupported("Windows CD drive discovery"))
    }

    fn open_file(&mut self, path: &[u8]) -> Result<Option<SystemHandle>, SystemError> {
        let canonical = canonical_path(path)?;
        if !self.dta_first {
            return Err(SystemError::Unsupported(
                "startup DTA-first prefix required",
            ));
        }
        let mut selected = SelectedSource::Loose;
        for archive in &self.archives {
            if !archive.unlocked {
                continue;
            }
            if self
                .provider
                .catalog(archive.native)
                .map_err(provider_error)?
                .iter()
                .any(|entry| entry.path.replace('\\', "/").to_ascii_lowercase() == canonical)
            {
                selected = SelectedSource::Archive(archive.native);
                break;
            }
        }
        let handle = self.reserve_handle()?;
        let stream = match self.provider.open(path, selected) {
            Ok(stream) => stream,
            Err(StreamError::Failure) => return Ok(None),
            Err(error) => return Err(provider_error(error)),
        };
        self.streams.insert(handle.0, stream);
        Ok(Some(handle))
    }

    fn read_file(&mut self, file: SystemHandle, count: usize) -> Result<Vec<u8>, SystemError> {
        let stream = self
            .streams
            .get_mut(&file.0)
            .ok_or(SystemError::InvalidReply("foreign startup file handle"))?;
        if count > 64 * 1024 * 1024 {
            return Err(SystemError::Unsupported("startup read quota"));
        }
        let mut bytes = vec![0; count];
        let read = stream.read(&mut bytes).map_err(provider_error)? as usize;
        if read > bytes.len() {
            return Err(SystemError::InvalidReply("startup read exceeds request"));
        }
        bytes.truncate(read);
        Ok(bytes)
    }

    fn get_graph(&mut self) -> Result<Option<SystemHandle>, SystemError> {
        Err(SystemError::Unsupported("original IGraph startup owner"))
    }
    fn graph_init(&mut self, _: SystemHandle, _: &GraphInitDescriptor) -> Result<i32, SystemError> {
        Err(SystemError::Unsupported("original IGraph Init"))
    }
    fn load_resource_string(
        &mut self,
        _: SystemHandle,
        _: u32,
        _: usize,
    ) -> Result<Vec<u8>, SystemError> {
        Err(SystemError::Unsupported("Windows resource strings"))
    }
    fn get_sound_driver(&mut self) -> Result<SystemHandle, SystemError> {
        Err(SystemError::Unsupported("original ISound startup owner"))
    }
    fn sound_init(&mut self, _: SystemHandle, _: SystemHandle, _: u32) -> Result<i32, SystemError> {
        Err(SystemError::Unsupported("original ISound Init"))
    }
    fn get_driver(&mut self) -> Result<SystemHandle, SystemError> {
        Err(SystemError::Unsupported(
            "original I3D_driver startup owner",
        ))
    }
    fn driver_init(&mut self, _: SystemHandle, _: u32) -> Result<i32, SystemError> {
        Err(SystemError::Unsupported("original I3D_driver Init"))
    }
    fn play_logo(&mut self, _: &[u8], _: [f32; 2]) -> Result<bool, SystemError> {
        Err(SystemError::Unsupported("original IShow startup owner"))
    }
    fn profile_init(&mut self) -> Result<bool, SystemError> {
        Err(SystemError::Unsupported("original profile startup owner"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "lhr-startup-files-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn genuine_loose_streams_have_independent_cursors_and_reject_closed_identity() {
        let dir = Directory::new();
        std::fs::write(dir.0.join("Version.TXT"), b"123").unwrap();
        let provider = NativeFiles::new(&dir.0, 64, 256 * 1024).unwrap();
        let mut backend = StartupAssetBackend::new(&provider);
        assert!(matches!(
            backend.open_file(b"version.txt"),
            Err(SystemError::Unsupported(_))
        ));
        backend.effect(SystemEffect::DtaFirstForce).unwrap();
        let first = backend.open_file(b"version.txt").unwrap().unwrap();
        let second = backend.open_file(b"VERSION.TXT").unwrap().unwrap();
        assert_ne!(first, second);
        let mut foreign = StartupAssetBackend::new(&provider);
        foreign.effect(SystemEffect::DtaFirstForce).unwrap();
        let foreign_file = foreign.open_file(b"version.txt").unwrap().unwrap();
        assert_ne!(first, foreign_file);
        assert!(matches!(
            foreign.read_file(first, 1),
            Err(SystemError::InvalidReply(_))
        ));
        foreign
            .effect(SystemEffect::CloseFile(foreign_file))
            .unwrap();
        assert_eq!(backend.read_file(first, 2), Ok(b"12".to_vec()));
        assert_eq!(backend.read_file(first, 2), Ok(b"3".to_vec()));
        assert_eq!(backend.read_file(second, 2), Ok(b"12".to_vec()));
        backend.effect(SystemEffect::CloseFile(first)).unwrap();
        assert_eq!(backend.open_file_count(), 1);
        assert!(matches!(
            backend.read_file(first, 1),
            Err(SystemError::InvalidReply(_))
        ));
        assert!(matches!(
            backend.effect(SystemEffect::ReleaseArchive(second)),
            Err(SystemError::InvalidReply(_))
        ));
        assert!(matches!(
            backend.get_graph(),
            Err(SystemError::Unsupported(_))
        ));
        backend.effect(SystemEffect::CloseFile(second)).unwrap();
        assert_eq!(backend.open_file_count(), 0);
        provider.shutdown().unwrap();
    }

    #[test]
    fn startup_effect_loads_real_patch_files_in_order_and_unloads() {
        let dir = Directory::new();
        std::fs::create_dir(dir.0.join("patch")).unwrap();
        // One nullable record; the decoded strings include the terminal NUL.
        fn fixture(source: &[u8]) -> Vec<u8> {
            let mut bytes = Vec::new();
            for string in [
                Some(b"mission\0".as_slice()),
                None,
                Some(b"frame\0".as_slice()),
                Some(source),
            ] {
                bytes.extend_from_slice(&(string.map_or(0, |s| s.len()) as i32).to_le_bytes());
                if let Some(string) = string {
                    bytes.extend(string.iter().map(|b| b ^ 0x85));
                }
            }
            crate::original_data_chunk::chunk(0x64, &bytes)
        }
        std::fs::write(dir.0.join("patch/a1.bin"), fixture(b"old\0")).unwrap();
        std::fs::write(dir.0.join("patch/a4.bin"), fixture(b"new\0")).unwrap();
        let provider = NativeFiles::new(&dir.0, 64, 256 * 1024).unwrap();
        let mut assets = StartupAssetBackend::new(&provider);
        assets.effect(SystemEffect::DtaFirstForce).unwrap();
        for path in [
            b"patch\\a1.bin",
            b"patch\\a2.bin",
            b"patch\\a3.bin",
            b"patch\\a4.bin",
        ] {
            assets.effect(SystemEffect::LoadPatchFile(path)).unwrap();
        }
        assert_eq!(assets.program_replacements().len(), 2);
        assert_eq!(
            assets
                .program_replacements()
                .iter()
                .rev()
                .map(|r| r.source.as_deref().unwrap())
                .collect::<Vec<_>>(),
            vec![b"new\0".as_slice(), b"old\0".as_slice()]
        );
        assert_eq!(assets.open_file_count(), 0);
        assets.effect(SystemEffect::UnloadPatchFiles).unwrap();
        assert!(assets.program_replacements().is_empty());
    }

    #[test]
    fn genuine_effects_stream_reaches_component_and_owned_cleanup() {
        use crate::mission_components::*;
        struct Heap(MissionMemoryDomainId, Vec<u32>);
        impl MissionMemoryDomain for Heap {
            fn mission_memory_domain(&self) -> MissionMemoryDomainId {
                self.0
            }
        }
        impl MissionAllocator for Heap {
            type Error = ();
            fn allocate(
                &mut self,
                _: MissionComponentKind,
            ) -> Result<Option<MissionAllocation>, ()> {
                Ok(None)
            }
            fn free(&mut self, identity: u32) -> Result<(), ()> {
                self.1.push(identity);
                Ok(())
            }
            fn open_effects_table(&mut self) -> Result<bool, ()> {
                panic!("adapter must use genuine file")
            }
        }
        let dir = Directory::new();
        std::fs::create_dir(dir.0.join("tables")).unwrap();
        let name = crate::original_data_chunk::chunk(0x67, b"synthetic effect\0");
        std::fs::write(
            dir.0.join("tables/effects.tbl"),
            crate::original_data_chunk::chunk(0x64, &name),
        )
        .unwrap();
        let provider = NativeFiles::new(&dir.0, 64, 256 * 1024).unwrap();
        let mut assets = StartupAssetBackend::new(&provider);
        assets.effect(SystemEffect::DtaFirstForce).unwrap();
        let domain = MissionMemoryDomainId::fresh();
        let mut heap = Heap(domain, Vec::new());
        let mut allocator = MissionAssetAllocator::new(&mut heap, &mut assets);
        let mut component = MissionComponent::construct_effects_prefix(
            MissionAllocation {
                domain,
                identity: 0x123400,
                bytes: vec![0xa5; 0x38],
            },
            0x39,
        )
        .unwrap();
        component
            .finish_effects_constructor(&mut allocator)
            .unwrap();
        assert_eq!(
            component.effects().unwrap().effects[0].name,
            b"synthetic effect"
        );
        assert_eq!(component.word(0x30), u32::MAX);
        assert_eq!(component.word(0x14), 0); // no forged x86 tree pointer
        assert_eq!(allocator.assets.open_file_count(), 0);
        component.clear_effects().unwrap();
        assert!(component.effects().unwrap().effects.is_empty());
        component.destroy(&mut allocator).unwrap();
        assert_eq!(allocator.memory.1, [0, 0, 0]);
        let mut populated = MissionComponent::construct_effects_prefix(
            MissionAllocation {
                domain,
                identity: 0x124400,
                bytes: vec![0xa5; 0x38],
            },
            0x39,
        )
        .unwrap();
        populated
            .finish_effects_constructor(&mut allocator)
            .unwrap();
        assert_eq!(populated.effects().unwrap().effects.len(), 1);
        populated.destroy(&mut allocator).unwrap();
        assert!(populated.effects().is_none());
        assert_eq!(allocator.memory.1, [0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn allocation_failure_is_real_native_null_and_bad_names_do_not_open() {
        let dir = Directory::new();
        let provider = NativeFiles::new(&dir.0, 64, 0).unwrap();
        let mut backend = StartupAssetBackend::new(&provider);
        assert_eq!(backend.create_archive(b"a8.dta"), Ok(None));
        assert_eq!(backend.archive_count(), 0);
        backend.effect(SystemEffect::DtaFirstForce).unwrap();
        assert!(matches!(
            backend.open_file(b"../version.txt"),
            Err(SystemError::Unsupported(_))
        ));
        assert!(matches!(
            backend.open_file(b"C:\\version.txt"),
            Err(SystemError::Unsupported(_))
        ));
        assert_eq!(backend.open_file_count(), 0);
        assert_eq!(backend.open_file(b"missing.txt"), Ok(None));
    }
}

/// Binds the mission's caller-owned allocator to this startup provider. The
/// opened effects stream remains here until the constructor consumes it.
/// No allocator or native pointer identity is fabricated by the adapter.
pub struct MissionAssetAllocator<'a, 'p, A> {
    pub memory: &'a mut A,
    pub assets: &'a mut StartupAssetBackend<'p>,
    effects_stream: Option<SystemHandle>,
}
#[derive(Debug)]
pub enum MissionAssetError<E> {
    Memory(E),
    Assets(SystemError),
}
impl<'a, 'p, A> MissionAssetAllocator<'a, 'p, A> {
    pub fn new(memory: &'a mut A, assets: &'a mut StartupAssetBackend<'p>) -> Self {
        Self {
            memory,
            assets,
            effects_stream: None,
        }
    }
}
impl<A: crate::mission_components::MissionMemoryDomain>
    crate::mission_components::MissionMemoryDomain for MissionAssetAllocator<'_, '_, A>
{
    fn mission_memory_domain(&self) -> crate::mission_components::MissionMemoryDomainId {
        self.memory.mission_memory_domain()
    }
}
impl<A: crate::mission_components::MissionAllocator> crate::mission_components::MissionAllocator
    for MissionAssetAllocator<'_, '_, A>
{
    type Error = MissionAssetError<A::Error>;
    fn allocate(
        &mut self,
        kind: crate::mission_components::MissionComponentKind,
    ) -> Result<Option<crate::mission_components::MissionAllocation>, Self::Error> {
        self.memory
            .allocate(kind)
            .map_err(MissionAssetError::Memory)
    }
    fn free(&mut self, identity: u32) -> Result<(), Self::Error> {
        self.memory
            .free(identity)
            .map_err(MissionAssetError::Memory)
    }
    fn open_effects_table(&mut self) -> Result<bool, Self::Error> {
        if self.effects_stream.is_some() {
            return Err(MissionAssetError::Assets(SystemError::InvalidReply(
                "effects stream already open",
            )));
        }
        self.effects_stream = self
            .assets
            .open_file(b"tables\\effects.tbl")
            .map_err(MissionAssetError::Assets)?;
        Ok(self.effects_stream.is_some())
    }
    fn read_effects_table(&mut self) -> Result<Option<Vec<u8>>, Self::Error> {
        let Some(handle) = self.effects_stream.take() else {
            return Ok(None);
        };
        self.assets
            .consume_chunk_file(handle)
            .map(Some)
            .map_err(MissionAssetError::Assets)
    }
}
impl<A> crate::c_str::CStrBackend for MissionAssetAllocator<'_, '_, A>
where
    A: crate::mission_components::MissionAllocator
        + crate::c_str::CStrBackend<Error = <A as crate::mission_components::MissionAllocator>::Error>,
{
    type Error = MissionAssetError<<A as crate::mission_components::MissionAllocator>::Error>;
    fn allocate_string(
        &mut self,
        size: u32,
    ) -> Result<Option<crate::c_str::StringAllocation>, Self::Error> {
        self.memory
            .allocate_string(size)
            .map_err(MissionAssetError::Memory)
    }
    fn free_string(
        &mut self,
        allocation: crate::c_str::StringAllocation,
    ) -> Result<(), Self::Error> {
        self.memory
            .free_string(allocation)
            .map_err(MissionAssetError::Memory)
    }
}
