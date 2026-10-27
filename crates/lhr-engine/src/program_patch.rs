//! Source-only LoadPatchFile/ReadPatchString/UnloadPatchFiles projection from
//! mafia-re game/mission/C_program.cpp (29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c).
//! Native REA credit is unchanged. Changes append in file/record order; the
//! existing CProgram consumer visits this slice in reverse priority order.
use crate::original_data_chunk::Chunk;
pub use crate::original_data_chunk::DataChunkError;
use crate::original_program::ProgramReplacement;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct ProgramPatchTable {
    changes: Vec<ProgramReplacement>,
}
impl ProgramPatchTable {
    pub fn replacements(&self) -> &[ProgramReplacement] {
        &self.changes
    }
    /// The index/key starts at zero for each file, not at the global count.
    /// Admission failures retain already appended complete records.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), DataChunkError> {
        let mut root = Chunk::new(bytes);
        let Some((kind, mut chunk)) = root.ascend()? else {
            return Ok(());
        };
        if kind != 0x64 {
            return Ok(());
        }
        let mut index = 0;
        while chunk.remaining() != 0 {
            let key = (index % 13 + 0x85) as u8;
            let mission = read_string(&mut chunk, key)?;
            let name = read_string(&mut chunk, key)?;
            let frame = read_string(&mut chunk, key)?;
            let source = read_string(&mut chunk, key)?;
            self.changes.push(ProgramReplacement {
                mission,
                name,
                frame,
                source,
            });
            index += 1;
        }
        Ok(())
    }
    pub fn unload(&mut self) {
        while self.changes.pop().is_some() {}
    }
}

/// Source replacement data has startup lifetime, spanning mission replacement.
/// Loading is one ordered attempt; a failure retains every complete prior row.
/// This host composition does not execute or claim original InitSystem.
#[derive(Debug, Default)]
pub struct ProgramPatchStartup {
    table: Rc<ProgramPatchTable>,
    phase: ProgramPatchStartupPhase,
    failure: Option<ProgramPatchStartupError>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgramPatchStartupPhase {
    #[default]
    NotStarted,
    Loading(usize),
    Ready,
    Stopped(usize),
    Unloaded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramPatchStartupError {
    AlreadyEntered,
    NotReady,
    SourcesRetainTable,
    Input {
        path: &'static str,
        detail: String,
    },
    Data {
        path: &'static str,
        error: DataChunkError,
    },
}

/// A lease keeps the same immutable startup table alive with retained source
/// owners. Cloning this value copies only an Rc handle, never the table rows.
#[derive(Clone, Debug)]
pub struct ProgramPatchLease(Rc<ProgramPatchTable>);
impl ProgramPatchLease {
    pub fn table(&self) -> &ProgramPatchTable {
        &self.0
    }
    pub(crate) fn empty_for_inspection() -> Self {
        Self(Rc::new(ProgramPatchTable::default()))
    }
    #[cfg(test)]
    pub(crate) fn weak_table(&self) -> std::rc::Weak<ProgramPatchTable> {
        Rc::downgrade(&self.0)
    }
}

impl ProgramPatchStartup {
    pub fn table(&self) -> &ProgramPatchTable {
        &self.table
    }
    pub fn lease(&self) -> Result<ProgramPatchLease, ProgramPatchStartupError> {
        if self.phase != ProgramPatchStartupPhase::Ready {
            return Err(ProgramPatchStartupError::NotReady);
        }
        Ok(ProgramPatchLease(Rc::clone(&self.table)))
    }
    pub fn phase(&self) -> ProgramPatchStartupPhase {
        self.phase
    }
    pub fn failure(&self) -> Option<&ProgramPatchStartupError> {
        self.failure.as_ref()
    }
    /// Installation already owns the selected DTA-first archive/file backend.
    /// Missing optional files are skipped; selected-source errors stop loading.
    pub fn initialize(
        &mut self,
        installation: &lhr_assets::Installation,
    ) -> Result<(), ProgramPatchStartupError> {
        self.initialize_with(|path| installation.read_optional(path).map_err(|e| e.to_string()))
    }
    fn initialize_with(
        &mut self,
        mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<(), ProgramPatchStartupError> {
        if self.phase != ProgramPatchStartupPhase::NotStarted {
            return Err(ProgramPatchStartupError::AlreadyEntered);
        }
        let table =
            Rc::get_mut(&mut self.table).ok_or(ProgramPatchStartupError::SourcesRetainTable)?;
        for (index, path) in [
            "patch/a1.bin",
            "patch/a2.bin",
            "patch/a3.bin",
            "patch/a4.bin",
        ]
        .into_iter()
        .enumerate()
        {
            self.phase = ProgramPatchStartupPhase::Loading(index);
            let result = match read(path) {
                Ok(Some(bytes)) => table
                    .load(&bytes)
                    .map_err(|error| ProgramPatchStartupError::Data { path, error }),
                Ok(None) => Ok(()),
                Err(detail) => Err(ProgramPatchStartupError::Input { path, detail }),
            };
            if let Err(error) = result {
                self.phase = ProgramPatchStartupPhase::Stopped(index);
                self.failure = Some(error.clone());
                return Err(error);
            }
        }
        self.phase = ProgramPatchStartupPhase::Ready;
        Ok(())
    }
    /// Caller performs this only after disposing every source actor/program.
    /// Rust storage cleanup carries no native UnloadPatchFiles coverage credit.
    pub fn unload_after_sources(&mut self) -> Result<(), ProgramPatchStartupError> {
        // Busy admission precedes every row mutation and phase change.
        let table =
            Rc::get_mut(&mut self.table).ok_or(ProgramPatchStartupError::SourcesRetainTable)?;
        table.unload();
        self.phase = ProgramPatchStartupPhase::Unloaded;
        Ok(())
    }
}
fn read_string(chunk: &mut Chunk<'_>, key: u8) -> Result<Option<Vec<u8>>, DataChunkError> {
    let length = chunk.read(4).ok_or(DataChunkError::InvalidExtent)?;
    let length = i32::from_le_bytes(length.try_into().unwrap());
    if length == 0 {
        return Ok(None);
    }
    if length < 0 {
        return Err(DataChunkError::InvalidStringLength);
    }
    let bytes = chunk
        .read(length as usize)
        .ok_or(DataChunkError::InvalidExtent)?;
    Ok(Some(bytes.iter().map(|b| b ^ key).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::original_data_chunk::chunk;
    pub(crate) fn fixture() -> Vec<u8> {
        let mut bytes = Vec::new();
        for index in 0..15 {
            for string in [
                Some(b"mission\0".as_slice()),
                None,
                Some(b"frame\0".as_slice()),
                Some(&[index, 0][..]),
            ] {
                bytes.extend_from_slice(&(string.map_or(0, |s| s.len()) as i32).to_le_bytes());
                if let Some(string) = string {
                    bytes.extend(string.iter().map(|b| b ^ (index % 13 + 0x85)));
                }
            }
        }
        chunk(0x64, &bytes)
    }
    #[test]
    fn xor_wrap_nulls_append_order_per_file_reset_and_unload() {
        let mut table = ProgramPatchTable::default();
        table.load(&fixture()).unwrap();
        table.load(&fixture()).unwrap();
        assert_eq!(table.replacements().len(), 30);
        for (index, replacement) in table.replacements().iter().enumerate() {
            assert_eq!(
                replacement.mission.as_deref(),
                Some(b"mission\0".as_slice())
            );
            assert!(replacement.name.is_none());
            assert_eq!(replacement.frame.as_deref(), Some(b"frame\0".as_slice()));
            assert_eq!(
                replacement.source.as_deref(),
                Some(&[(index % 15) as u8, 0][..])
            );
        }
        table.unload();
        assert!(table.replacements().is_empty());
    }
    #[test]
    fn wrong_type_negative_lengths_and_short_records_never_append_fake_changes() {
        let mut table = ProgramPatchTable::default();
        table.load(&chunk(0x65, &[0xff; 20])).unwrap();
        assert!(table.replacements().is_empty());
        assert_eq!(
            table.load(&chunk(0x64, &(-1i32).to_le_bytes())),
            Err(DataChunkError::InvalidStringLength)
        );
        assert_eq!(
            table.load(&chunk(0x64, &[0; 15])),
            Err(DataChunkError::InvalidExtent)
        );
        assert!(table.replacements().is_empty());
    }
}
