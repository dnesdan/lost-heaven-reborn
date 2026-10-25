//! Owned PC effects table tree. Names are upstream descriptive names, from
//! mafia-re `game/mission/C_effects.{cpp,h}` pinned at
//! 29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c. Source-only implementation;
//! no new native function credit. Byte strings retain original spelling.
use crate::original_data_chunk::Chunk;
pub use crate::original_data_chunk::DataChunkError;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectsTable {
    pub effects: Vec<Effect>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effect {
    pub name: Vec<u8>,
    pub model: Vec<u8>,
    pub parts: Vec<EffectPart>,
    /// Native offsets 0x1c and 0x28, retained as raw float bits.
    pub vectors: [[u32; 3]; 2],
    pub flags: [u8; 2],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectPart {
    pub name: Vec<u8>,
    pub frame: Vec<u8>,
    pub subparts: Vec<EffectSubpart>,
    /// Contiguous native scalar words at offsets 0x20..=0x98.
    /// Offset 0x7c is an integer; all other words are raw float bits.
    pub parameter_words: [u32; 31],
}
impl Default for EffectPart {
    fn default() -> Self {
        let mut words = [0; 31];
        for offset in [0x50, 0x58, 0x5c, 0x60, 0x70] {
            words[(offset - 0x20) / 4] = 1f32.to_bits();
        }
        Self {
            name: Vec::new(),
            frame: Vec::new(),
            subparts: Vec::new(),
            parameter_words: words,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectSubpart {
    pub name: Vec<u8>,
    /// Contiguous native scalar words at offsets 0x0c..=0x64.
    /// 0x30, 0x34 and 0x60 are integers; others are raw float bits.
    pub parameter_words: [u32; 23],
}
impl Default for EffectSubpart {
    fn default() -> Self {
        let mut words = [0; 23];
        for offset in [0x10, 0x38, 0x3c, 0x40, 0x50] {
            words[(offset - 0xc) / 4] = 1f32.to_bits();
        }
        words[(0x30 - 0xc) / 4] = 1000;
        words[(0x60 - 0xc) / 4] = 1;
        words[(0x64 - 0xc) / 4] = 9.8f32.to_bits();
        Self {
            name: Vec::new(),
            parameter_words: words,
        }
    }
}
impl EffectsTable {
    /// Clear runs before opening on Load. Parsing preserves the completed
    /// prefix on admission failure, rather than claiming a successful load.
    pub fn load(&mut self, bytes: &[u8]) -> Result<(), DataChunkError> {
        self.clear();
        let mut root = Chunk::new(bytes);
        while let Some((kind, mut chunk)) = root.ascend()? {
            if kind != 0x64 {
                break;
            }
            let mut effect = Effect::default();
            while let Some((kind, mut field)) = chunk.ascend()? {
                match kind {
                    0x65 => effect.parts.push(load_part(field)?),
                    0x67 => effect.name = field.string()?,
                    0x68 => effect.model = field.string()?,
                    0x6e => {
                        for flag in &mut effect.flags {
                            if let Some(value) = field.read(1) {
                                *flag = value[0];
                            }
                        }
                    }
                    _ => {
                        field.words(&mut effect.vectors[0]);
                        field.words(&mut effect.vectors[1]);
                    }
                }
            }
            self.effects.push(effect);
        }
        Ok(())
    }
    /// Release children while retaining the effects vector's capacity.
    /// C_effect/C_effect_part destructors release children back-to-front.
    pub fn clear(&mut self) {
        for effect in &mut self.effects {
            while let Some(mut part) = effect.parts.pop() {
                while part.subparts.pop().is_some() {}
            }
        }
        self.effects.clear();
    }
}
fn load_part(mut chunk: Chunk<'_>) -> Result<EffectPart, DataChunkError> {
    let mut part = EffectPart::default();
    while let Some((kind, mut field)) = chunk.ascend()? {
        match kind {
            0x66 => part.subparts.push(load_subpart(field)?),
            0x67 => part.name = field.string()?,
            0x69 => part.frame = field.string()?,
            0x6b => {
                field.words(&mut part.parameter_words[6..9]);
                field.words(&mut part.parameter_words[9..12]);
            }
            0x6c => {
                field.words(&mut part.parameter_words[14..17]);
                field.words(&mut part.parameter_words[17..20]);
            }
            0x6d => field.words(&mut part.parameter_words[23..24]),
            _ => {
                field.words(&mut part.parameter_words[0..3]);
                field.words(&mut part.parameter_words[3..6]);
                for i in [12, 13, 14, 17, 20, 21, 22, 24, 25, 26, 27, 28, 29, 30] {
                    field.words(&mut part.parameter_words[i..i + 1]);
                }
            }
        }
    }
    Ok(part)
}
fn load_subpart(mut chunk: Chunk<'_>) -> Result<EffectSubpart, DataChunkError> {
    let mut part = EffectSubpart::default();
    while let Some((kind, mut field)) = chunk.ascend()? {
        match kind {
            0x67 => part.name = field.string()?,
            0x6b => field.words(&mut part.parameter_words[22..23]),
            0x6c => {
                field.words(&mut part.parameter_words[11..14]);
                field.words(&mut part.parameter_words[14..17]);
            }
            _ => {
                for range in [
                    0..3,
                    3..6,
                    6..9,
                    9..10,
                    10..11,
                    11..12,
                    14..15,
                    17..18,
                    18..19,
                    19..20,
                    20..21,
                    21..22,
                ] {
                    field.words(&mut part.parameter_words[range]);
                }
            }
        }
    }
    Ok(part)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::original_data_chunk::chunk;
    #[test]
    fn nested_defaults_overrides_unknown_parameters_and_clear_reload() {
        let mut sub = chunk(0x67, b"spark\0tail");
        sub.extend(chunk(0x6c, &[0x33; 24]));
        let mut params = vec![0; 44];
        params.extend(2f32.to_le_bytes());
        params.extend(3f32.to_le_bytes());
        params.extend([0; 20]);
        sub.extend(chunk(0xff, &params));
        let mut part = chunk(0x69, b"frame\0");
        part.extend(chunk(0x66, &sub));
        let mut effect = chunk(0x68, b"model.i3d\0");
        effect.extend(chunk(0x65, &part));
        effect.extend(chunk(0x6e, &[2, 3]));
        let mut bytes = chunk(0x64, &effect);
        bytes.extend(chunk(0x64, &[]));
        bytes.extend(chunk(0x70, &[]));
        bytes.extend(chunk(0x64, &effect)); // root stops at first other type
        let mut table = EffectsTable::default();
        table.load(&bytes).unwrap();
        assert_eq!(table.effects.len(), 2);
        assert_eq!(table.effects[0].model, b"model.i3d");
        assert_eq!(table.effects[0].flags, [2, 3]);
        let part = &table.effects[0].parts[0];
        assert_eq!(part.frame, b"frame");
        assert_eq!(part.parameter_words[12], 1f32.to_bits());
        let sub = &part.subparts[0];
        assert_eq!(sub.name, b"spark");
        assert_eq!(sub.parameter_words[11], 2f32.to_bits());
        assert_eq!(sub.parameter_words[12], 0x33333333); // params update x only
        assert_eq!(sub.parameter_words[14], 3f32.to_bits());
        assert_eq!(sub.parameter_words[22], 9.8f32.to_bits());
        let capacity = table.effects.capacity();
        table.clear();
        assert_eq!(table.effects.capacity(), capacity);
        table.load(&chunk(0x64, &[])).unwrap();
        assert_eq!(table.effects.len(), 1);
    }
    #[test]
    fn short_reads_preserve_defaults_and_bad_chunks_are_explicit() {
        let mut table = EffectsTable::default();
        table.load(&chunk(0x64, &chunk(0x6a, &[0; 4]))).unwrap();
        assert_eq!(table.effects[0].vectors, [[0; 3]; 2]);
        assert_eq!(
            table.load(&[0x64, 0, 5, 0, 0, 0]),
            Err(DataChunkError::InvalidExtent)
        );
        assert_eq!(
            table.load(&chunk(0x64, &chunk(0x67, b"no nul"))),
            Err(DataChunkError::UnterminatedString)
        );
    }
}
