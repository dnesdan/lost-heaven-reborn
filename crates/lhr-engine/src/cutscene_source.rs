//! Serialized input for the bounded PC cutscene LoadFull prefix.
//! Buffers here are file data, never a replacement for a native game object.
#![forbid(unsafe_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutsceneSourceReadError {
    pub requested: usize,
    pub copied: usize,
}

/// Owns an already acquired source and its current read position. Installation
/// acquisition is a buffered host adapter, not native dtaOpen/read/close parity.
#[derive(Debug)]
pub struct CutsceneSource {
    bytes: Vec<u8>,
    cursor: usize,
}
impl CutsceneSource {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes, cursor: 0 }
    }
    pub fn from_installation(
        installation: &lhr_assets::Installation,
        path: &str,
    ) -> Result<Self, lhr_assets::Error> {
        installation.read_original_path(path).map(Self::from_bytes)
    }
    pub fn position(&self) -> usize {
        self.cursor
    }
    pub fn source_len(&self) -> usize {
        self.bytes.len()
    }
    /// A truncated read retains the copied source prefix and reached cursor.
    /// This is a safe host failure, not native truncated-success behavior.
    pub(crate) fn read_exact(&mut self, target: &mut [u8]) -> Result<(), CutsceneSourceReadError> {
        let copied = target.len().min(self.bytes.len() - self.cursor);
        target[..copied].copy_from_slice(&self.bytes[self.cursor..self.cursor + copied]);
        self.cursor += copied;
        if copied == target.len() {
            Ok(())
        } else {
            Err(CutsceneSourceReadError {
                requested: target.len(),
                copied,
            })
        }
    }
}

/// The complete serialized 96-byte header. Unknown fields remain file bytes.
/// Accessors describe only the LoadFull fields needed by this prefix.
#[derive(Debug)]
pub struct CutsceneSourceHeader([u8; 96]);
impl CutsceneSourceHeader {
    pub(crate) fn from_bytes(bytes: [u8; 96]) -> Self {
        Self(bytes)
    }
    pub fn magic(&self) -> u32 {
        self.word(0)
    }
    pub fn actor_descriptor_count(&self) -> u32 {
        self.word(12)
    }
    pub fn serialized_bytes(&self) -> &[u8; 96] {
        &self.0
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.0[offset..offset + 4].try_into().unwrap())
    }
}
