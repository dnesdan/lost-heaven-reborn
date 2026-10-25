//! Borrowed ordered scene2 cursor for the original mission loader.
//! Extents come from the shared lazy chunk decoder. Entering a container is
//! explicit: neither metadata traversal nor eager field parsing occurs here.
use lhr_assets::scene_actor_stream::{scene2_root, Chunk, Chunks};

struct Level<'a> {
    chunks: Chunks<'a>,
    end: usize,
}
pub struct MissionSceneInput<'a> {
    levels: Vec<Level<'a>>,
    current: Option<Chunk<'a>>,
    cursor: usize,
    decode_error: Option<String>,
}
impl<'a> MissionSceneInput<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        let root = scene2_root(bytes).map_err(|e| e.to_string())?;
        Ok(Self {
            levels: vec![Level {
                chunks: root.children(),
                end: root.raw.len(),
            }],
            current: None,
            cursor: 6,
            decode_error: None,
        })
    }
    /// Absolute original input position, including consumed chunk headers.
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn current(&self) -> Option<Chunk<'a>> {
        self.current
    }
    pub fn depth(&self) -> usize {
        self.levels.len()
    }
    /// Reads one header only. Previously consumed effects need not be rolled
    /// back when the next header/extent is malformed.
    pub fn next_chunk(&mut self) -> Result<Option<Chunk<'a>>, String> {
        if let Some(error) = &self.decode_error {
            return Err(error.clone());
        }
        if self.current.is_some() {
            return Err("current scene chunk remains open".into());
        }
        let level = self.levels.last_mut().ok_or("no open scene container")?;
        match level.chunks.next() {
            Some(Ok(chunk)) => {
                self.cursor = chunk.offset + 6;
                self.current = Some(chunk);
                Ok(Some(chunk))
            }
            Some(Err(error)) => {
                let error = error.to_string();
                self.decode_error = Some(error.clone());
                Err(error)
            }
            None => {
                self.cursor = level.end;
                Ok(None)
            }
        }
    }
    pub fn enter_current(&mut self) -> Result<(), String> {
        let chunk = self.current.take().ok_or("no current scene chunk")?;
        if self.cursor != chunk.offset + 6 {
            self.current = Some(chunk);
            return Err("partly consumed scene container".into());
        }
        self.levels.push(Level {
            chunks: chunk.children(),
            end: chunk.offset + chunk.raw.len(),
        });
        Ok(())
    }
    /// Leaves a fully consumed container. The enclosing decoder already owns
    /// its exact successor; there is no recursive promotion of nested records.
    pub fn leave_container(&mut self) -> Result<(), String> {
        if self.current.is_some() {
            return Err("current scene chunk remains open".into());
        }
        if self.levels.len() <= 1 {
            return Err("cannot leave LS root".into());
        }
        let level = self.levels.last().unwrap();
        if self.cursor != level.end {
            return Err("scene container still has unread input".into());
        }
        self.levels.pop();
        Ok(())
    }
    pub fn read(&mut self, length: usize) -> Result<&'a [u8], String> {
        let chunk = self.current.ok_or("no current scene chunk")?;
        let start = self.cursor - chunk.offset;
        let end = start
            .checked_add(length)
            .ok_or("scene read extent overflow")?;
        let result = chunk
            .raw
            .get(start..end)
            .ok_or("short scene field at original read boundary")?;
        self.cursor += length;
        Ok(result)
    }
    pub fn read_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.read(2)?.try_into().unwrap()))
    }
    pub fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.read(4)?.try_into().unwrap()))
    }
    /// Original CloseChunk seeks to the current extent's end.
    pub fn close_current(&mut self) -> Result<(), String> {
        let chunk = self.current.take().ok_or("no current scene chunk")?;
        self.cursor = chunk.offset + chunk.raw.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &((payload.len() + 6) as u32).to_le_bytes(),
            payload,
        ]
        .concat()
    }
    #[test]
    fn cursor_retains_exact_partial_field_and_nested_successor() {
        let bytes = chunk(
            0x4c53,
            &chunk(
                0x4000,
                &[
                    chunk(0x4010, &chunk(0x4011, &12u32.to_le_bytes())),
                    chunk(0x4010, b""),
                ]
                .concat(),
            ),
        );
        let mut input = MissionSceneInput::new(&bytes).unwrap();
        assert_eq!(input.next_chunk().unwrap().unwrap().kind, 0x4000);
        input.enter_current().unwrap();
        input.next_chunk().unwrap();
        input.enter_current().unwrap();
        let field = input.next_chunk().unwrap().unwrap();
        assert_eq!(input.read_u32().unwrap(), 12);
        assert_eq!(input.cursor(), field.offset + 10);
        input.close_current().unwrap();
        assert!(input.next_chunk().unwrap().is_none());
        input.leave_container().unwrap();
        assert_eq!(input.next_chunk().unwrap().unwrap().kind, 0x4010);
    }
    #[test]
    fn malformed_later_header_does_not_hide_consumed_prefix() {
        let bytes = chunk(
            0x4c53,
            &[chunk(1, &5u16.to_le_bytes()), vec![0x10, 0x40, 99, 0, 0, 0]].concat(),
        );
        let mut input = MissionSceneInput::new(&bytes).unwrap();
        input.next_chunk().unwrap();
        assert_eq!(input.read_u16().unwrap(), 5);
        input.close_current().unwrap();
        let cursor = input.cursor();
        assert!(input.next_chunk().is_err());
        assert_eq!(input.cursor(), cursor);
    }
}
