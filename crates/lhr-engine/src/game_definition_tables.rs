//! Owned PC game definitions read by G_LoadSaveClass::Init.
//!
//! mafia-re f42a25d, game/g_material.cpp, Game.exe LoadMaterialsFromBin
//! 0x60f5c0 and UnloadMaterials 0x60f780. The file/table names are based on
//! PC paths; `g_material` and the C++ table type names originate from PS2 or
//! upstream research. No native pointer/allocator or complete function credit
//! is represented by these host-owned source buffers.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefinitionKind {
    Items,
    Materials,
    Particles,
    Voices,
}
impl DefinitionKind {
    pub const ORDER: [Self; 4] = [Self::Items, Self::Materials, Self::Particles, Self::Voices];
    fn index(self) -> usize {
        match self {
            Self::Items => 0,
            Self::Materials => 1,
            Self::Particles => 2,
            Self::Voices => 3,
        }
    }
    pub fn path(self) -> &'static str {
        match self {
            Self::Items => "tables/predmety.def",
            Self::Materials => "tables/materials.def",
            Self::Particles => "tables/particles.def",
            Self::Voices => "tables/voices.def",
        }
    }
    pub fn stride(self) -> usize {
        match self {
            Self::Items => 0xbc,
            Self::Materials => 0x170,
            Self::Particles => 0xa4,
            Self::Voices => 0xa4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableFile(pub u64);

/// Backend must operate on the real archive/file and allocation owner. A
/// synthetic backend can inject failure and mutations to check reached prefixes.
pub trait DefinitionTableBackend {
    type Error;
    fn open(&mut self, kind: DefinitionKind) -> Result<Option<TableFile>, Self::Error>;
    fn seek_end(&mut self, file: TableFile) -> Result<usize, Self::Error>;
    fn seek_begin(&mut self, file: TableFile) -> Result<(), Self::Error>;
    /// Provide the actual owner storage with its allocation-state bytes.
    fn allocate(&mut self, bytes: usize) -> Result<Vec<u8>, Self::Error>;
    /// Read into that storage. Partial mutations remain visible on error.
    fn read_into(&mut self, file: TableFile, bytes: &mut [u8]) -> Result<(), Self::Error>;
    fn close(&mut self, file: TableFile) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum TableLoadError<E> {
    Backend(E),
    Interrupted,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TableUnloadError {
    PendingFile(TableFile),
    InterruptedLoad,
}

/// The actual owned source buffers consumed by later vehicle/material work.
/// A failed later open retains earlier tables; an interrupted read retains the
/// just-published allocation and any bytes the backend wrote before failure.
pub struct GameDefinitionTables {
    tables: [Option<Vec<u8>>; 4],
    counts: [usize; 4],
    pending_file: Option<TableFile>,
    interrupted: bool,
}
impl Default for GameDefinitionTables {
    fn default() -> Self {
        Self {
            tables: std::array::from_fn(|_| None),
            counts: [0; 4],
            pending_file: None,
            interrupted: false,
        }
    }
}
impl GameDefinitionTables {
    pub fn table(&self, kind: DefinitionKind) -> Option<&[u8]> {
        self.tables[kind.index()].as_deref()
    }
    pub fn count(&self, kind: DefinitionKind) -> usize {
        self.counts[kind.index()]
    }
    pub fn pending_file(&self) -> Option<TableFile> {
        self.pending_file
    }
    pub fn interrupted(&self) -> bool {
        self.interrupted
    }
    fn effect<T, E>(&mut self, result: Result<T, E>) -> Result<T, TableLoadError<E>> {
        result.map_err(|error| {
            self.interrupted = true;
            TableLoadError::Backend(error)
        })
    }
    /// Open, replace, seek, allocate, read and close each file in source order.
    /// Missing file is the source's false return, with previously loaded tables
    /// left owned. There is no automatic rollback after any failed boundary.
    pub fn load<B: DefinitionTableBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<bool, TableLoadError<B::Error>> {
        if self.interrupted {
            return Err(TableLoadError::Interrupted);
        }
        for kind in DefinitionKind::ORDER {
            let Some(file) = self.effect(backend.open(kind))? else {
                return Ok(false);
            };
            self.pending_file = Some(file);
            // Native deletes the prior array after successful open, before
            // seeking. Its old count remains until the new file is closed.
            self.tables[kind.index()] = None;
            let size = self.effect(backend.seek_end(file))?;
            self.effect(backend.seek_begin(file))?;
            let bytes = self.effect(backend.allocate(size))?;
            self.tables[kind.index()] = Some(bytes);
            // Take the live allocation temporarily so the backend can mutate
            // it while `self` records an error; restore it even after failure.
            let mut bytes = self.tables[kind.index()].take().unwrap();
            let read = backend.read_into(file, &mut bytes);
            self.tables[kind.index()] = Some(bytes);
            self.effect(read)?;
            self.effect(backend.close(file))?;
            self.pending_file = None;
            self.counts[kind.index()] = size / kind.stride();
        }
        Ok(true)
    }
    /// UnloadMaterials deletes the four table arrays and zeroes each count in
    /// items, materials, particles, voices order. It does not close a file left
    /// at an unavailable backend boundary; that external owner remains visible.
    /// A host backend interruption cannot be completed as native Unload:
    /// the file identity and partial allocation must remain available for
    /// diagnostics or an explicit, separately evidenced cleanup owner.
    pub fn ensure_unload_ready(&self) -> Result<(), TableUnloadError> {
        if let Some(file) = self.pending_file {
            return Err(TableUnloadError::PendingFile(file));
        }
        if self.interrupted {
            return Err(TableUnloadError::InterruptedLoad);
        }
        Ok(())
    }
    pub fn unload(&mut self) -> Result<(), TableUnloadError> {
        self.ensure_unload_ready()?;
        for kind in DefinitionKind::ORDER {
            self.tables[kind.index()] = None;
            self.counts[kind.index()] = 0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Event {
        Open(DefinitionKind),
        SeekEnd(TableFile),
        SeekBegin(TableFile),
        Allocate(usize),
        Read(TableFile, usize),
        Close(TableFile),
    }
    #[derive(Default)]
    struct Source {
        events: Vec<Event>,
        missing: Option<DefinitionKind>,
        fail_read: Option<DefinitionKind>,
    }
    impl DefinitionTableBackend for Source {
        type Error = &'static str;
        fn open(&mut self, kind: DefinitionKind) -> Result<Option<TableFile>, Self::Error> {
            self.events.push(Event::Open(kind));
            Ok((self.missing != Some(kind)).then_some(TableFile(kind.index() as u64 + 1)))
        }
        fn seek_end(&mut self, file: TableFile) -> Result<usize, Self::Error> {
            self.events.push(Event::SeekEnd(file));
            Ok(DefinitionKind::ORDER[file.0 as usize - 1].stride() + 1)
        }
        fn seek_begin(&mut self, file: TableFile) -> Result<(), Self::Error> {
            self.events.push(Event::SeekBegin(file));
            Ok(())
        }
        fn allocate(&mut self, bytes: usize) -> Result<Vec<u8>, Self::Error> {
            self.events.push(Event::Allocate(bytes));
            Ok(vec![0xa5; bytes])
        }
        fn read_into(&mut self, file: TableFile, bytes: &mut [u8]) -> Result<(), Self::Error> {
            self.events.push(Event::Read(file, bytes.len()));
            let midpoint = bytes.len() / 2;
            bytes[..midpoint].fill(file.0 as u8);
            if self.fail_read == Some(DefinitionKind::ORDER[file.0 as usize - 1]) {
                Err("read")
            } else {
                bytes[midpoint..].fill(file.0 as u8);
                Ok(())
            }
        }
        fn close(&mut self, file: TableFile) -> Result<(), Self::Error> {
            self.events.push(Event::Close(file));
            Ok(())
        }
    }
    #[test]
    fn full_load_publishes_all_four_actual_buffers_and_floor_counts() {
        let mut tables = GameDefinitionTables::default();
        let mut source = Source::default();
        assert_eq!(tables.load(&mut source), Ok(true));
        assert_eq!(source.events.len(), 24);
        for kind in DefinitionKind::ORDER {
            assert_eq!(tables.count(kind), 1);
            assert_eq!(tables.table(kind).unwrap().len(), kind.stride() + 1);
        }
        tables.unload().unwrap();
        assert!(DefinitionKind::ORDER
            .iter()
            .all(|&kind| tables.table(kind).is_none() && tables.count(kind) == 0));
    }
    #[test]
    fn missing_later_open_retains_completed_earlier_tables() {
        let mut tables = GameDefinitionTables::default();
        let mut source = Source {
            missing: Some(DefinitionKind::Particles),
            ..Default::default()
        };
        assert_eq!(tables.load(&mut source), Ok(false));
        assert_eq!(tables.count(DefinitionKind::Items), 1);
        assert_eq!(tables.count(DefinitionKind::Materials), 1);
        assert_eq!(tables.table(DefinitionKind::Particles), None);
        assert_eq!(tables.pending_file(), None);
        assert_eq!(
            source.events.last(),
            Some(&Event::Open(DefinitionKind::Particles))
        );
    }
    #[test]
    fn read_error_keeps_seed_and_partial_bytes_and_open_file_owner() {
        let mut tables = GameDefinitionTables::default();
        let mut source = Source {
            fail_read: Some(DefinitionKind::Materials),
            ..Default::default()
        };
        assert_eq!(
            tables.load(&mut source),
            Err(TableLoadError::Backend("read"))
        );
        let bytes = tables.table(DefinitionKind::Materials).unwrap();
        assert!(bytes[..bytes.len() / 2].iter().all(|&b| b == 2));
        assert!(bytes[bytes.len() / 2..].iter().all(|&b| b == 0xa5));
        assert_eq!(tables.count(DefinitionKind::Materials), 0);
        let length = bytes.len();
        assert_eq!(tables.pending_file(), Some(TableFile(2)));
        assert_eq!(
            tables.unload(),
            Err(TableUnloadError::PendingFile(TableFile(2)))
        );
        assert_eq!(
            tables.table(DefinitionKind::Materials).unwrap().len(),
            length
        );
        assert_eq!(tables.load(&mut source), Err(TableLoadError::Interrupted));
        assert_eq!(
            source.events.last(),
            Some(&Event::Read(TableFile(2), length))
        );
    }
}
