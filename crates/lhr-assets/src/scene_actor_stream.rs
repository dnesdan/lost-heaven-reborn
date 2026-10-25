//! Lossless borrowed scene2 input for the original loader's ordered actor arm.
//! This is format intake, not actor construction. Unlike inspection metadata,
//! repeated fields, unknown chunks and opaque derived LoadData payloads survive.
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub kind: u16,
    /// Offset in the complete scene2 input, including this chunk's header.
    pub offset: usize,
    pub raw: &'a [u8],
    pub payload: &'a [u8],
}
impl<'a> Chunk<'a> {
    pub fn children(self) -> Chunks<'a> {
        Chunks {
            bytes: self.payload,
            base: self.offset + 6,
            position: 0,
            failed: false,
        }
    }
}
/// Decodes only the next header/extent. A malformed later field does not prevent
/// the mission owner from retaining effects of previously consumed fields.
#[derive(Clone, Debug)]
pub struct Chunks<'a> {
    bytes: &'a [u8],
    base: usize,
    position: usize,
    failed: bool,
}
impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.position == self.bytes.len() {
            return None;
        }
        let start = self.position;
        let remaining = &self.bytes[start..];
        let result = (|| {
            if remaining.len() < 6 {
                return Err(Error("truncated scene chunk header".into()));
            }
            let kind = u16::from_le_bytes(remaining[..2].try_into().unwrap());
            let length = u32::from_le_bytes(remaining[2..6].try_into().unwrap()) as usize;
            if length < 6 || length > remaining.len() {
                return Err(Error("invalid scene chunk extent".into()));
            }
            self.position += length;
            Ok(Chunk {
                kind,
                offset: self.base + start,
                raw: &remaining[..length],
                payload: &remaining[6..length],
            })
        })();
        if result.is_err() {
            self.failed = true;
        }
        Some(result)
    }
}
/// The exact single LS root; its direct child sequence follows original order.
/// No recursive inspection traversal promotes nested records into loader input.
pub fn scene2_root(bytes: &[u8]) -> Result<Chunk<'_>, Error> {
    if bytes.len() > crate::MAX_BYTES {
        return Err(Error("scene exceeds 64 MiB quota".into()));
    }
    let mut chunks = Chunks {
        bytes,
        base: 0,
        position: 0,
        failed: false,
    };
    let root = chunks
        .next()
        .ok_or_else(|| Error("missing LS scene root".into()))??;
    if root.kind != 0x4c53 || root.raw.len() != bytes.len() {
        return Err(Error("expected single LS scene root".into()));
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(kind: u16, bytes: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &((bytes.len() + 6) as u32).to_le_bytes(),
            bytes,
        ]
        .concat()
    }
    #[test]
    fn preserves_duplicate_order_unknown_fields_and_opaque_bytes() {
        let fields = [
            chunk(0xae24, &[0, 255, 1]),
            chunk(0xae23, b"x\0"),
            chunk(0xbeef, &[8]),
            chunk(0xae22, &5u32.to_le_bytes()),
            chunk(0xae23, b"y\0"),
        ]
        .concat();
        let bytes = chunk(0x4c53, &chunk(0xae20, &chunk(0xae21, &fields)));
        let collection = scene2_root(&bytes)
            .unwrap()
            .children()
            .next()
            .unwrap()
            .unwrap();
        let actor = collection.children().next().unwrap().unwrap();
        let decoded = actor.children().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            decoded.iter().map(|c| c.kind).collect::<Vec<_>>(),
            [0xae24, 0xae23, 0xbeef, 0xae22, 0xae23]
        );
        assert_eq!(decoded[0].payload, [0, 255, 1]);
        assert_eq!(
            decoded
                .iter()
                .flat_map(|c| c.raw)
                .copied()
                .collect::<Vec<_>>(),
            fields
        );
        assert_eq!(
            &bytes[decoded[3].offset..decoded[3].offset + decoded[3].raw.len()],
            decoded[3].raw
        );
    }
    #[test]
    fn reports_later_corruption_only_after_prior_field() {
        let fields = [chunk(0xae23, b"x\0"), vec![0x22, 0xae, 99, 0, 0, 0]].concat();
        let bytes = chunk(0x4c53, &chunk(0xae21, &fields));
        let actor = scene2_root(&bytes)
            .unwrap()
            .children()
            .next()
            .unwrap()
            .unwrap();
        let mut fields = actor.children();
        assert_eq!(fields.next().unwrap().unwrap().payload, b"x\0");
        assert!(fields.next().unwrap().is_err());
        assert!(fields.next().is_none());
    }
}
