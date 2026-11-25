#![forbid(unsafe_code)]

use super::{
    DefinitionKind, DefinitionTableBackend, GameDefinitionTables, TableFile, TableLoadError,
};
use lhr_assets::Installation;
use std::io::{Cursor, Read, Seek, SeekFrom};

/// Owns the pending table input independently of the installation used to open it.
/// `Installation` supplies checked, bounded file contents. Source selection, decoding
/// and native handle cleanup happen at open. Subsequent seeks and reads use a cursor
/// over the owned contents, which stays here if an operation fails before close.
#[derive(Default)]
pub(crate) struct InstalledDefinitionSource {
    pending: Option<OpenFile>,
}

struct OpenFile {
    id: TableFile,
    contents: Cursor<Vec<u8>>,
}

impl InstalledDefinitionSource {
    /// Loads in table order. `Ok(false)` means the next table was absent.
    /// An interrupted table load or a retained file prevents further loading.
    pub fn load(
        &mut self,
        installation: &Installation,
        tables: &mut GameDefinitionTables,
    ) -> Result<bool, String> {
        if self.pending.is_some() {
            return Err("definition table source still owns a pending file".into());
        }
        tables
            .load(&mut InstallationBackend {
                installation,
                pending: &mut self.pending,
            })
            .map_err(|error| match error {
                TableLoadError::Backend(message) => message,
                TableLoadError::Interrupted => "definition table load was interrupted".into(),
            })
    }
}

struct InstallationBackend<'a> {
    installation: &'a Installation,
    pending: &'a mut Option<OpenFile>,
}

impl InstallationBackend<'_> {
    fn file(&mut self, id: TableFile) -> Result<&mut Cursor<Vec<u8>>, String> {
        self.pending
            .as_mut()
            .filter(|file| file.id == id)
            .map(|file| &mut file.contents)
            .ok_or_else(|| "definition table file is unavailable".into())
    }
}

impl DefinitionTableBackend for InstallationBackend<'_> {
    type Error = String;

    fn open(&mut self, kind: DefinitionKind) -> Result<Option<TableFile>, String> {
        let contents = self
            .installation
            .read_optional(kind.path())
            .map_err(|_| format!("could not open {kind:?} definition table"))?;
        let Some(contents) = contents else {
            return Ok(None);
        };
        let id = TableFile(kind.index() as u64 + 1);
        *self.pending = Some(OpenFile {
            id,
            contents: Cursor::new(contents),
        });
        Ok(Some(id))
    }

    fn seek_end(&mut self, file: TableFile) -> Result<usize, String> {
        let size = self
            .file(file)?
            .seek(SeekFrom::End(0))
            .map_err(|_| "could not seek to the end of the definition table")?;
        usize::try_from(size).map_err(|_| "definition table size is out of range".into())
    }

    fn seek_begin(&mut self, file: TableFile) -> Result<(), String> {
        self.file(file)?
            .rewind()
            .map_err(|_| "could not rewind the definition table".into())
    }

    fn allocate(&mut self, bytes: usize) -> Result<Vec<u8>, String> {
        let mut contents = Vec::new();
        contents
            .try_reserve_exact(bytes)
            .map_err(|_| "could not allocate definition table storage")?;
        contents.resize(bytes, 0);
        Ok(contents)
    }

    fn read_into(&mut self, file: TableFile, bytes: &mut [u8]) -> Result<(), String> {
        let count = self
            .file(file)?
            .read(bytes)
            .map_err(|_| "could not read definition table")?;
        if count != bytes.len() {
            return Err("short definition table read".into());
        }
        Ok(())
    }

    fn close(&mut self, file: TableFile) -> Result<(), String> {
        self.file(file)?;
        *self.pending = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lhr_assets::{rw_data::de_crypt, startup_archives::ARCHIVE_UNLOCK_KEYS};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "lhr-definition-tables-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn write_loose(&self, kind: DefinitionKind, contents: &[u8]) {
            let path = self.0.join(kind.path());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn items_archive(contents: &[u8], unlock_key: u64) -> Vec<u8> {
        let key = unlock_key ^ 0x3498576239475694;
        let mut name = *b"Tables\\Predmety.DEF\0";
        let data_offset = 20 + 32 + name.len() as u32;
        let payload_len = 1 + contents.len() as u32;
        let index_offset = data_offset + 4 + payload_len;

        let mut header = [0; 16];
        header[0..4].copy_from_slice(&1u32.to_le_bytes());
        header[4..8].copy_from_slice(&index_offset.to_le_bytes());
        header[8..12].copy_from_slice(&28u32.to_le_bytes());
        let mut record = [0; 32];
        record[0..4].copy_from_slice(&(index_offset - 20).to_le_bytes());
        record[4..8].copy_from_slice(&32u32.to_le_bytes());
        record[16..20].copy_from_slice(&(contents.len() as u32).to_le_bytes());
        record[20..24].copy_from_slice(&1u32.to_le_bytes());
        record[24..28].copy_from_slice(&(0x8000 | name.len() as u32).to_le_bytes());

        let upper = name[..name.len() - 1].to_ascii_uppercase();
        let sum: u32 = upper.iter().copied().map(u32::from).sum();
        let hash = ((upper.len() as u32) << 16) | sum;
        let mut row = [0; 28];
        row[0..4].copy_from_slice(&hash.to_le_bytes());
        row[4..8].copy_from_slice(&20u32.to_le_bytes());
        row[8..12].copy_from_slice(&data_offset.to_le_bytes());
        row[12..].copy_from_slice(&upper[upper.len() - 16..]);

        let mut payload = vec![0];
        payload.extend_from_slice(contents);
        let mut archive = b"ISD0".to_vec();
        for part in [&mut header[..], &mut record, &mut name] {
            de_crypt(part, key);
            archive.extend_from_slice(part);
        }
        archive.extend_from_slice(&payload_len.to_le_bytes());
        de_crypt(&mut payload, key);
        archive.extend_from_slice(&payload);
        de_crypt(&mut row, key);
        archive.extend_from_slice(&row);
        archive
    }

    #[test]
    fn later_absence_preserves_loaded_tables_and_allows_a_new_load() {
        let directory = Directory::new();
        let items = vec![7; DefinitionKind::Items.stride() + 1];
        directory.write_loose(DefinitionKind::Items, &items);
        directory.write_loose(DefinitionKind::Materials, &[]);
        let installation = Installation::open(&directory.0).unwrap();
        let mut tables = GameDefinitionTables::default();
        let mut source = InstalledDefinitionSource::default();

        assert_eq!(source.load(&installation, &mut tables), Ok(false));
        assert_eq!(tables.table(DefinitionKind::Items), Some(items.as_slice()));
        assert_eq!(tables.count(DefinitionKind::Items), 1);
        assert_eq!(tables.table(DefinitionKind::Materials), Some(&[][..]));
        assert_eq!(tables.table(DefinitionKind::Particles), None);
        assert_eq!(tables.pending_file(), None);
        assert!(!tables.interrupted());

        directory.write_loose(DefinitionKind::Particles, b"particles");
        directory.write_loose(DefinitionKind::Voices, b"voices");
        assert_eq!(source.load(&installation, &mut tables), Ok(true));
        assert_eq!(tables.table(DefinitionKind::Voices), Some(&b"voices"[..]));
        assert!(source.pending.is_none());
        assert_eq!(tables.unload(), Ok(()));
    }

    #[test]
    fn selected_dta_wins_and_selected_failure_never_uses_another_copy() {
        let directory = Directory::new();
        let selected_archive = directory.0.join("a8.dta");
        fs::write(
            &selected_archive,
            items_archive(b"selected", ARCHIVE_UNLOCK_KEYS[0]),
        )
        .unwrap();
        fs::write(
            directory.0.join("a2.dta"),
            items_archive(b"later archive", ARCHIVE_UNLOCK_KEYS[2]),
        )
        .unwrap();
        directory.write_loose(DefinitionKind::Items, b"loose");
        let installation = Installation::open(&directory.0).unwrap();
        let mut tables = GameDefinitionTables::default();
        let mut source = InstalledDefinitionSource::default();

        assert_eq!(source.load(&installation, &mut tables), Ok(false));
        assert_eq!(tables.table(DefinitionKind::Items), Some(&b"selected"[..]));

        fs::write(selected_archive, b"changed archive").unwrap();
        assert_eq!(
            source.load(&installation, &mut tables),
            Err("could not open Items definition table".into())
        );
        assert_eq!(tables.table(DefinitionKind::Items), Some(&b"selected"[..]));
        assert_eq!(tables.pending_file(), None);
        assert!(tables.interrupted());
        assert_eq!(
            source.load(&installation, &mut tables),
            Err("definition table load was interrupted".into())
        );
    }

    #[test]
    fn short_read_retains_the_owned_input_after_installation_drops() {
        let directory = Directory::new();
        directory.write_loose(DefinitionKind::Items, b"abc");
        let mut source = InstalledDefinitionSource::default();
        let id;
        {
            let installation = Installation::open(&directory.0).unwrap();
            let mut backend = InstallationBackend {
                installation: &installation,
                pending: &mut source.pending,
            };
            id = backend.open(DefinitionKind::Items).unwrap().unwrap();
            assert_eq!(backend.seek_end(id), Ok(3));
            assert_eq!(backend.file(id).unwrap().position(), 3);
            assert_eq!(backend.seek_begin(id), Ok(()));
            let mut output = [0xa5; 4];
            assert_eq!(
                backend.read_into(id, &mut output),
                Err("short definition table read".into())
            );
            assert_eq!(&output, b"abc\xa5");
        }

        let installation = Installation::open(&directory.0).unwrap();
        let mut fresh_tables = GameDefinitionTables::default();
        assert_eq!(
            source.load(&installation, &mut fresh_tables),
            Err("definition table source still owns a pending file".into())
        );
        let pending = source.pending.as_ref().unwrap();
        assert_eq!(pending.id, id);
        assert_eq!(pending.contents.position(), 3);
        assert_eq!(pending.contents.get_ref(), b"abc");
        assert_eq!(fresh_tables.table(DefinitionKind::Items), None);
        assert!(!fresh_tables.interrupted());
    }
}
