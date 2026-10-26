//! Source-only LoadPatchFile/ReadPatchString/UnloadPatchFiles projection from
//! mafia-re game/mission/C_program.cpp (29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c).
//! Native REA credit is unchanged. Changes append in file/record order; the
//! existing CProgram consumer visits this slice in reverse priority order.
use crate::original_data_chunk::Chunk;
pub use crate::original_data_chunk::DataChunkError;
use crate::original_program::ProgramReplacement;

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
