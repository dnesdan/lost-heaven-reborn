//! Bounded read-only projection of mafia-re `game/C_chunk.h` at pinned
//! 29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c. No native parity credit.
//! Invalid extents and unterminated stack-buffer strings are host admission
//! errors: the original's unchecked/undefined malformed-input paths aren't run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataChunkError {
    InvalidExtent,
    UnterminatedString,
    InvalidStringLength,
}

pub(crate) struct Chunk<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Chunk<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    /// RAscend reads the six-byte packed header, Descend skips to its end.
    /// Children must be inside the caller's admitted parent/file extent.
    pub(crate) fn ascend(&mut self) -> Result<Option<(u16, Chunk<'a>)>, DataChunkError> {
        if self.remaining() == 0 {
            return Ok(None);
        }
        let header = self
            .bytes
            .get(self.position..self.position + 6)
            .ok_or(DataChunkError::InvalidExtent)?;
        let kind = u16::from_le_bytes(header[..2].try_into().unwrap());
        let size = i32::from_le_bytes(header[2..].try_into().unwrap());
        if size < 6 {
            return Err(DataChunkError::InvalidExtent);
        }
        let end = self
            .position
            .checked_add(size as usize)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(DataChunkError::InvalidExtent)?;
        let child = Self::new(&self.bytes[self.position + 6..end]);
        self.position = end;
        Ok(Some((kind, child)))
    }
    /// C_chunk::Read fails without moving the cursor when a field doesn't fit.
    pub(crate) fn read(&mut self, count: usize) -> Option<&'a [u8]> {
        if count > self.remaining() {
            return None;
        }
        let start = self.position;
        self.position += count;
        Some(&self.bytes[start..self.position])
    }
    pub(crate) fn words(&mut self, destination: &mut [u32]) {
        if let Some(bytes) = self.read(destination.len() * 4) {
            for (out, bytes) in destination.iter_mut().zip(bytes.chunks_exact(4)) {
                *out = u32::from_le_bytes(bytes.try_into().unwrap());
            }
        }
    }
    pub(crate) fn string(&mut self) -> Result<Vec<u8>, DataChunkError> {
        let count = self.remaining().min(999);
        let bytes = self.read(count).unwrap();
        let end = bytes
            .iter()
            .position(|b| *b == 0)
            .ok_or(DataChunkError::UnterminatedString)?;
        Ok(bytes[..end].to_vec())
    }
}

#[cfg(test)]
pub(crate) fn chunk(kind: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = kind.to_le_bytes().to_vec();
    out.extend_from_slice(&((bytes.len() + 6) as i32).to_le_bytes());
    out.extend_from_slice(bytes);
    out
}
