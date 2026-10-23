//! Bounded, read-only PC GifC v5 collision intake in source coordinates (Y up).
//!
//! This preserves the file before the original loader rewrites pointers and
//! disables unresolved/degenerate triangles. It neither binds scene frames nor
//! makes the stored records ready for collision queries. Stored planes, property
//! words, vertex indices, cell order and opaque words remain unchanged.
use super::{Error, MAX_BYTES, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveKind {
    Triangle,
    Aabb,
    XtObb,
    Cylinder,
    Obb,
    Sphere,
}
impl PrimitiveKind {
    pub const ALL: [Self; 6] = [
        Self::Triangle,
        Self::Aabb,
        Self::XtObb,
        Self::Cylinder,
        Self::Obb,
        Self::Sphere,
    ];
    pub const fn tag(self) -> u8 {
        match self {
            Self::Triangle => 0,
            Self::Aabb => 0x81,
            Self::XtObb => 0x80,
            Self::Cylinder => 0x84,
            Self::Obb => 0x83,
            Self::Sphere => 0x82,
        }
    }
    pub const fn stride(self) -> usize {
        match self {
            Self::Triangle | Self::Aabb => 32,
            Self::XtObb => 184,
            Self::Cylinder => 20,
            Self::Obb => 160,
            Self::Sphere => 24,
        }
    }
    const fn slot(self) -> usize {
        match self {
            Self::Triangle => 0,
            Self::Aabb => 1,
            Self::XtObb => 2,
            Self::Cylinder => 3,
            Self::Obb => 4,
            Self::Sphere => 5,
        }
    }
    const fn count_offset(self) -> usize {
        match self {
            Self::Triangle => 0x30,
            Self::Aabb => 0x40,
            Self::XtObb => 0x38,
            Self::Cylinder => 0x58,
            Self::Obb => 0x50,
            Self::Sphere => 0x48,
        }
    }
    fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.tag() == tag)
    }
}

/// The four bytes of the source property word, in their stored order.
/// A cell's reference tag selects an array; this classification is separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Properties {
    pub classification: u8,
    pub sort: u8,
    pub flags: u8,
    pub material: u8,
}
impl Properties {
    pub const fn word(self) -> u32 {
        u32::from_le_bytes([self.classification, self.sort, self.flags, self.material])
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexReference {
    pub index: u16,
    pub link: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimitiveData {
    /// The first link chooses the buffer for all three original indices.
    /// Other link words are retained, even if they differ from the first.
    Triangle {
        vertices: [VertexReference; 3],
        stored_plane: [f32; 4],
    },
    Aabb {
        link: u32,
        min: [f32; 3],
        max: [f32; 3],
    },
    XtObb {
        link: u32,
        min: [f32; 3],
        max: [f32; 3],
        extent_min: [f32; 3],
        extent_max: [f32; 3],
        matrix: [f32; 16],
        inverse: [f32; 16],
    },
    Cylinder {
        link: u32,
        center: [f32; 2],
        radius: f32,
    },
    Obb {
        link: u32,
        extent_min: [f32; 3],
        extent_max: [f32; 3],
        matrix: [f32; 16],
        inverse: [f32; 16],
    },
    Sphere {
        link: u32,
        center: [f32; 3],
        radius: f32,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct PrimitiveRecord<'a> {
    kind: PrimitiveKind,
    pub index: usize,
    pub offset: usize,
    raw: &'a [u8],
}
impl<'a> PrimitiveRecord<'a> {
    pub fn kind(self) -> PrimitiveKind {
        self.kind
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn properties(self) -> Properties {
        Properties {
            classification: self.raw[0],
            sort: self.raw[1],
            flags: self.raw[2],
            material: self.raw[3],
        }
    }
    /// Float bits are retained, including nonfinite values. Numerical admission
    /// belongs to the eventual consumer; intake does not repair geometry.
    pub fn data(self) -> PrimitiveData {
        let b = self.raw;
        match self.kind {
            PrimitiveKind::Triangle => PrimitiveData::Triangle {
                vertices: std::array::from_fn(|i| VertexReference {
                    index: u16_at(b, 4 + i * 4),
                    link: u16_at(b, 6 + i * 4),
                }),
                stored_plane: floats(b, 16),
            },
            PrimitiveKind::Aabb => PrimitiveData::Aabb {
                link: word(b, 4),
                min: floats(b, 8),
                max: floats(b, 20),
            },
            PrimitiveKind::XtObb => PrimitiveData::XtObb {
                link: word(b, 4),
                min: floats(b, 8),
                max: floats(b, 20),
                extent_min: floats(b, 32),
                extent_max: floats(b, 44),
                matrix: floats(b, 56),
                inverse: floats(b, 120),
            },
            PrimitiveKind::Cylinder => PrimitiveData::Cylinder {
                link: word(b, 4),
                center: floats(b, 8),
                radius: float(b, 16),
            },
            PrimitiveKind::Obb => PrimitiveData::Obb {
                link: word(b, 4),
                extent_min: floats(b, 8),
                extent_max: floats(b, 20),
                matrix: floats(b, 32),
                inverse: floats(b, 96),
            },
            PrimitiveKind::Sphere => PrimitiveData::Sphere {
                link: word(b, 4),
                center: floats(b, 8),
                radius: float(b, 20),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PrimitiveArray<'a> {
    kind: PrimitiveKind,
    pub offset: usize,
    raw: &'a [u8],
}
impl<'a> PrimitiveArray<'a> {
    pub fn kind(self) -> PrimitiveKind {
        self.kind
    }
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn len(self) -> usize {
        self.raw.len() / self.kind.stride()
    }
    pub fn is_empty(self) -> bool {
        self.raw.is_empty()
    }
    pub fn get(self, index: usize) -> Option<PrimitiveRecord<'a>> {
        if index >= self.len() {
            return None;
        }
        let relative = index * self.kind.stride();
        Some(PrimitiveRecord {
            kind: self.kind,
            index,
            offset: self.offset.checked_add(relative)?,
            raw: &self.raw[relative..relative + self.kind.stride()],
        })
    }
    pub fn iter(self) -> impl ExactSizeIterator<Item = PrimitiveRecord<'a>> {
        (0..self.len()).map(move |i| self.get(i).unwrap())
    }
}

#[derive(Debug, Clone)]
pub struct Link<'a> {
    pub index: usize,
    pub offset: usize,
    /// Source link-kind word, not a runtime frame type inferred by this parser.
    pub kind: u32,
    pub name: String,
    pub raw: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct FloatArray<'a> {
    raw: &'a [u8],
}
impl<'a> FloatArray<'a> {
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn len(self) -> usize {
        self.raw.len() / 4
    }
    pub fn is_empty(self) -> bool {
        self.raw.is_empty()
    }
    pub fn get(self, index: usize) -> Option<f32> {
        (index < self.len()).then(|| float(self.raw, index * 4))
    }
    pub fn iter(self) -> impl ExactSizeIterator<Item = f32> {
        (0..self.len()).map(move |i| float(self.raw, i * 4))
    }
}
#[derive(Debug, Clone, Copy)]
pub struct VertexArray<'a> {
    raw: &'a [u8],
}
impl<'a> VertexArray<'a> {
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn len(self) -> usize {
        self.raw.len() / 12
    }
    pub fn is_empty(self) -> bool {
        self.raw.is_empty()
    }
    pub fn get(self, index: usize) -> Option<[f32; 3]> {
        (index < self.len()).then(|| floats(self.raw, index * 12))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Grid<'a> {
    pub raw: &'a [u8],
    pub min_x: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_z: f32,
    pub cell_width: f32,
    pub cell_length: f32,
    pub width: usize,
    pub length: usize,
    pub x: FloatArray<'a>,
    pub z: FloatArray<'a>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimitiveId {
    pub kind: PrimitiveKind,
    pub index: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellReference {
    pub packed: u32,
    pub flag: u8,
    pub primitive: PrimitiveId,
}
impl CellReference {
    pub fn relative_byte_offset(self) -> usize {
        (self.packed & 0x00ff_ffff) as usize
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Cell<'a> {
    pub index: usize,
    pub offset: usize,
    raw: &'a [u8],
}
impl<'a> Cell<'a> {
    pub fn raw(self) -> &'a [u8] {
        self.raw
    }
    pub fn header(self) -> &'a [u8] {
        &self.raw[..16]
    }
    pub fn len(self) -> usize {
        word(self.raw, 0) as usize
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    pub fn max_y(self) -> f32 {
        float(self.raw, 12)
    }
    /// Source pointer words are opaque, not native addresses on this host.
    pub fn pointer_words(self) -> [u32; 2] {
        [word(self.raw, 4), word(self.raw, 8)]
    }
    pub fn references(self) -> impl ExactSizeIterator<Item = CellReference> + 'a {
        (0..self.len()).map(move |i| {
            let packed = word(self.raw, 16 + i * 4);
            let kind = PrimitiveKind::from_tag((packed >> 24) as u8).unwrap();
            CellReference {
                packed,
                flag: self.raw[16 + self.len() * 4 + i],
                primitive: PrimitiveId {
                    kind,
                    index: (packed & 0x00ff_ffff) as usize / kind.stride(),
                },
            }
        })
    }
    pub fn padding(self) -> &'a [u8] {
        &self.raw[16 + self.len() * 5..]
    }
}

#[derive(Debug)]
pub struct Klz<'a> {
    pub bytes: &'a [u8],
    pub header: &'a [u8],
    pub grid_offset: usize,
    pub links: Vec<Link<'a>>,
    pub grid: Grid<'a>,
    pub separator: &'a [u8],
    pub primitives: [PrimitiveArray<'a>; 6],
    pub world_vertices: VertexArray<'a>,
    pub cell_separator: &'a [u8],
    /// Row-major original order: index = z * width + x.
    pub cells: Vec<Cell<'a>>,
    pub tail: &'a [u8],
}
impl<'a> Klz<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        parse_klz(bytes)
    }
    pub fn array(&self, kind: PrimitiveKind) -> PrimitiveArray<'a> {
        self.primitives[kind.slot()]
    }
    pub fn record(&self, id: PrimitiveId) -> Option<PrimitiveRecord<'a>> {
        self.array(id.kind).get(id.index)
    }
    pub fn cell(&self, x: usize, z: usize) -> Option<Cell<'a>> {
        if x >= self.grid.width || z >= self.grid.length {
            return None;
        }
        self.cells
            .get(z.checked_mul(self.grid.width)?.checked_add(x)?)
            .copied()
    }
}

fn invalid(message: &str) -> Error {
    Error(format!("KLZ: {message}"))
}
fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
fn float(b: &[u8], at: usize) -> f32 {
    f32::from_bits(word(b, at))
}
fn floats<const N: usize>(b: &[u8], at: usize) -> [f32; N] {
    std::array::from_fn(|i| float(b, at + i * 4))
}
fn range(b: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    let end = at
        .checked_add(len)
        .ok_or_else(|| invalid("offset overflow"))?;
    b.get(at..end).ok_or_else(|| invalid("truncated input"))
}
fn take<'a>(b: &'a [u8], at: &mut usize, count: usize, stride: usize) -> Result<&'a [u8]> {
    let len = count
        .checked_mul(stride)
        .ok_or_else(|| invalid("array length overflow"))?;
    let raw = range(b, *at, len)?;
    *at += len;
    Ok(raw)
}
fn charge(total: &mut usize, count: usize, stride: usize) -> Result<()> {
    *total = count
        .checked_mul(stride)
        .and_then(|n| total.checked_add(n))
        .filter(|n| *n <= MAX_BYTES)
        .ok_or_else(|| invalid("decoded storage quota exceeded"))?;
    Ok(())
}
fn monotonic(axis: FloatArray<'_>) -> bool {
    let mut previous = f32::NEG_INFINITY;
    axis.iter().all(|v| {
        let valid = v.is_finite() && v > previous;
        previous = v;
        valid
    })
}

/// Structural validation is host policy, not a reproduction of native error
/// handling. Duplicate link names/offsets and opaque padding/tail are retained.
/// Unknown reference tags, misaligned byte offsets and out-of-range references
/// are rejected before any safe record accessor is made available.
pub fn parse_klz(bytes: &[u8]) -> Result<Klz<'_>> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid("source byte quota exceeded"));
    }
    let header = range(bytes, 0, 24)?;
    if &header[..4] != b"GifC" || word(header, 4) != 5 {
        return Err(invalid("expected GifC version 5"));
    }
    let grid_offset = word(header, 8) as usize;
    let link_count = word(header, 12) as usize;
    let mut cursor = 24;
    let link_offsets = take(bytes, &mut cursor, link_count, 4)?;
    let link_start = cursor;
    if grid_offset < link_start {
        return Err(invalid("grid overlaps link table"));
    }
    let grid_raw = range(bytes, grid_offset, 104)?;
    let mut storage = 0;
    charge(&mut storage, link_count, std::mem::size_of::<Link<'_>>())?;
    let mut links = Vec::with_capacity(link_count);
    for index in 0..link_count {
        let offset = word(link_offsets, index * 4) as usize;
        if offset < link_start || offset >= grid_offset {
            return Err(invalid("frame link offset outside link block"));
        }
        let available = range(bytes, offset, grid_offset - offset)?;
        if available.len() < 5 {
            return Err(invalid("truncated frame link"));
        }
        let name_len = available[4..]
            .iter()
            .take(4097)
            .position(|b| *b == 0)
            .ok_or_else(|| {
                invalid(if available.len() - 4 > 4096 {
                    "frame name quota exceeded"
                } else {
                    "unterminated frame name"
                })
            })?;
        let string_len = name_len + 1;
        let padded = string_len
            .checked_add(3)
            .ok_or_else(|| invalid("frame name overflow"))?
            & !3;
        let raw = range(available, 0, 4 + padded)?;
        // Windows-1250 can expand to three UTF-8 bytes per source byte.
        charge(&mut storage, name_len, 3)?;
        links.push(Link {
            index,
            offset,
            kind: word(raw, 0),
            name: super::decode_windows1250(&raw[4..4 + name_len])?,
            raw,
        });
    }
    let dimensions: [f32; 6] = floats(grid_raw, 0);
    let [min_x, min_z, max_x, max_z, cell_width, cell_length] = dimensions;
    let width = word(grid_raw, 24) as usize;
    let length = word(grid_raw, 28) as usize;
    if dimensions.iter().any(|v| !v.is_finite())
        || max_x <= min_x
        || max_z <= min_z
        || cell_width <= 0.
        || cell_length <= 0.
        || width == 0
        || length == 0
    {
        return Err(invalid("invalid grid dimensions"));
    }
    let cell_count = width
        .checked_mul(length)
        .ok_or_else(|| invalid("cell count overflow"))?;
    charge(&mut storage, cell_count, std::mem::size_of::<Cell<'_>>())?;
    cursor = grid_offset + 104;
    let x = FloatArray {
        raw: take(
            bytes,
            &mut cursor,
            width
                .checked_add(1)
                .ok_or_else(|| invalid("axis count overflow"))?,
            4,
        )?,
    };
    let z = FloatArray {
        raw: take(
            bytes,
            &mut cursor,
            length
                .checked_add(1)
                .ok_or_else(|| invalid("axis count overflow"))?,
            4,
        )?,
    };
    if !monotonic(x) || !monotonic(z) {
        return Err(invalid("nonfinite or unordered grid boundaries"));
    }
    let grid = Grid {
        raw: grid_raw,
        min_x,
        min_z,
        max_x,
        max_z,
        cell_width,
        cell_length,
        width,
        length,
        x,
        z,
    };
    let separator = take(bytes, &mut cursor, 1, 4)?;
    let mut primitives = PrimitiveKind::ALL.map(|kind| PrimitiveArray {
        kind,
        offset: 0,
        raw: &[],
    });
    for array in &mut primitives {
        let count = word(grid_raw, array.kind.count_offset()) as usize;
        array.offset = cursor;
        array.raw = take(bytes, &mut cursor, count, array.kind.stride())?;
        for record in array.iter() {
            let classification = record.properties().classification;
            if if array.kind == PrimitiveKind::Triangle {
                classification > 7
            } else {
                classification != array.kind.tag()
            } {
                return Err(invalid("record classification does not match array"));
            }
            if array.kind != PrimitiveKind::Triangle && word(record.raw, 4) as usize >= links.len()
            {
                return Err(invalid("volume frame link out of range"));
            }
        }
    }
    let world_vertices = VertexArray {
        raw: take(bytes, &mut cursor, word(grid_raw, 0x60) as usize, 12)?,
    };
    for record in primitives[0].iter() {
        let first_link = u16_at(record.raw, 6);
        if first_link == 0x7fff {
            if (0..3).any(|i| u16_at(record.raw, 4 + i * 4) as usize >= world_vertices.len()) {
                return Err(invalid("world vertex index out of range"));
            }
        } else if first_link as usize >= links.len() {
            return Err(invalid("triangle frame link out of range"));
        }
    }
    let cell_separator = take(bytes, &mut cursor, 1, 4)?;
    // Reject impossible cell counts before reserving their decoded metadata.
    range(
        bytes,
        cursor,
        cell_count
            .checked_mul(16)
            .ok_or_else(|| invalid("cell headers overflow"))?,
    )?;
    let mut cells = Vec::with_capacity(cell_count);
    for index in 0..cell_count {
        let offset = cursor;
        let cell_header = take(bytes, &mut cursor, 1, 16)?;
        let count = word(cell_header, 0) as usize;
        let reference_bytes = take(bytes, &mut cursor, count, 4)?;
        take(bytes, &mut cursor, count, 1)?;
        take(bytes, &mut cursor, (4 - (count & 3)) & 3, 1)?;
        for i in 0..count {
            let packed = word(reference_bytes, i * 4);
            let kind = PrimitiveKind::from_tag((packed >> 24) as u8)
                .ok_or_else(|| invalid("unknown cell reference tag"))?;
            let relative = (packed & 0x00ff_ffff) as usize;
            let array = primitives[kind.slot()];
            if !relative.is_multiple_of(kind.stride()) || relative / kind.stride() >= array.len() {
                return Err(invalid(
                    "misaligned or out-of-range relative byte reference",
                ));
            }
        }
        cells.push(Cell {
            index,
            offset,
            raw: &bytes[offset..cursor],
        });
    }
    Ok(Klz {
        bytes,
        header,
        grid_offset,
        links,
        grid,
        separator,
        primitives,
        world_vertices,
        cell_separator,
        cells,
        tail: &bytes[cursor..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(b: &mut [u8], at: usize, value: u32) {
        b[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut b = vec![0; 40 + 104];
        b[..4].copy_from_slice(b"GifC");
        put(&mut b, 4, 5);
        put(&mut b, 8, 40);
        put(&mut b, 12, 2);
        // Duplicate source names and offsets must stay distinct link slots.
        put(&mut b, 24, 32);
        put(&mut b, 28, 32);
        put(&mut b, 32, 1);
        b[36] = b'f';
        b[38] = 0xa5;
        b[39] = 0x5a;
        for (i, n) in [0_f32, 0., 10., 10., 10., 10.].into_iter().enumerate() {
            put(&mut b, 40 + i * 4, n.to_bits());
        }
        put(&mut b, 40 + 24, 1);
        put(&mut b, 40 + 28, 1);
        for kind in PrimitiveKind::ALL {
            put(
                &mut b,
                40 + kind.count_offset(),
                if kind == PrimitiveKind::Triangle {
                    2
                } else {
                    1
                },
            );
        }
        put(&mut b, 40 + 0x60, 3);
        for n in [0_f32, 10., 0., 10.] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        b.extend_from_slice(&[1, 2, 3, 4]);
        for kind in PrimitiveKind::ALL {
            let count = if kind == PrimitiveKind::Triangle {
                2
            } else {
                1
            };
            for index in 0..count {
                let start = b.len();
                b.resize(start + kind.stride(), 0);
                b[start] = if kind == PrimitiveKind::Triangle {
                    3
                } else {
                    kind.tag()
                };
                b[start + 1..start + 4].copy_from_slice(&[0x34, 0x56, 0x78]);
                for at in (8..kind.stride()).step_by(4) {
                    put(&mut b, start + at, (100. + at as f32).to_bits());
                }
                if kind == PrimitiveKind::Triangle {
                    for i in 0..3 {
                        let link = if i == 0 {
                            if index == 0 { 0x7fff } else { 0 }
                        } else {
                            0xffff
                        };
                        put(&mut b, start + 4 + i * 4, (link << 16) | i as u32);
                    }
                    // Plane metadata retains payload/sign bits without repair.
                    put(&mut b, start + 16, 0x7fc1_2345);
                    put(&mut b, start + 20, 0x8000_0000);
                }
            }
        }
        for n in 0..9 {
            b.extend_from_slice(&(n as f32).to_le_bytes());
        }
        b.extend_from_slice(&[5, 6, 7, 8]);
        let start = b.len();
        b.resize(start + 16, 0);
        put(&mut b, start, 7);
        put(&mut b, start + 4, 0xdead_beef);
        put(&mut b, start + 8, 0xbaad_f00d);
        put(&mut b, start + 12, 42_f32.to_bits());
        // Relative32 selects triangle1, not triangle32. Keep original order.
        for packed in [
            32,
            0,
            0x8100_0000,
            0x8000_0000,
            0x8400_0000,
            0x8300_0000,
            0x8200_0000,
        ] {
            b.extend_from_slice(&u32::to_le_bytes(packed));
        }
        b.extend_from_slice(&[7, 6, 5, 4, 3, 2, 1, 0xa5]);
        b.extend_from_slice(&[0xde, 0xad]);
        b
    }

    #[test]
    fn preserves_source_identity_properties_planes_and_all_array_layouts() {
        let b = fixture();
        let parsed = parse_klz(&b).unwrap();
        assert_eq!(parsed.bytes, b);
        assert_eq!(parsed.links.len(), 2);
        assert_eq!(parsed.links[0].offset, parsed.links[1].offset);
        assert_eq!(parsed.links[0].name, "f");
        assert_eq!(parsed.links[0].raw, &[1, 0, 0, 0, b'f', 0, 0xa5, 0x5a]);
        assert_eq!(parsed.separator, &[1, 2, 3, 4]);
        assert_eq!(parsed.cell_separator, &[5, 6, 7, 8]);
        assert_eq!(parsed.tail, &[0xde, 0xad]);
        assert_eq!(parsed.world_vertices.get(2), Some([6., 7., 8.]));
        assert_eq!(parsed.world_vertices.get(3), None);
        let t = parsed.array(PrimitiveKind::Triangle).get(1).unwrap();
        assert_eq!(t.index, 1);
        assert_eq!(t.properties().word(), 0x7856_3403);
        let PrimitiveData::Triangle {
            vertices,
            stored_plane,
        } = t.data()
        else {
            panic!()
        };
        assert_eq!(vertices[0], VertexReference { index: 0, link: 0 });
        assert_eq!(
            vertices[2],
            VertexReference {
                index: 2,
                link: 0xffff
            }
        );
        assert_eq!(stored_plane[0].to_bits(), 0x7fc1_2345);
        assert_eq!(stored_plane[1].to_bits(), 0x8000_0000);
        for kind in PrimitiveKind::ALL {
            let a = parsed.array(kind);
            assert_eq!(a.raw().len(), a.len() * kind.stride());
            assert!(a.get(a.len()).is_none());
            let r = a.get(0).unwrap();
            assert_eq!(r.raw(), &b[r.offset..r.offset + kind.stride()]);
            match r.data() {
                PrimitiveData::Triangle { .. } => {}
                PrimitiveData::Aabb { link, min, max } => {
                    assert_eq!(link, 0);
                    assert_eq!(min, [108., 112., 116.]);
                    assert_eq!(max, [120., 124., 128.]);
                }
                PrimitiveData::XtObb {
                    link,
                    min,
                    max,
                    extent_min,
                    extent_max,
                    matrix,
                    inverse,
                } => {
                    assert_eq!(link, 0);
                    assert_eq!(min[0], 108.);
                    assert_eq!(max[2], 128.);
                    assert_eq!(extent_min[0], 132.);
                    assert_eq!(extent_max[2], 152.);
                    assert_eq!(matrix[0], 156.);
                    assert_eq!(matrix[15], 216.);
                    assert_eq!(inverse[0], 220.);
                    assert_eq!(inverse[15], 280.);
                }
                PrimitiveData::Cylinder {
                    link,
                    center,
                    radius,
                } => {
                    assert_eq!(link, 0);
                    assert_eq!(center, [108., 112.]);
                    assert_eq!(radius, 116.);
                }
                PrimitiveData::Obb {
                    link,
                    extent_min,
                    extent_max,
                    matrix,
                    inverse,
                } => {
                    assert_eq!(link, 0);
                    assert_eq!(extent_min[0], 108.);
                    assert_eq!(extent_max[2], 128.);
                    assert_eq!(matrix[0], 132.);
                    assert_eq!(matrix[15], 192.);
                    assert_eq!(inverse[0], 196.);
                    assert_eq!(inverse[15], 256.);
                }
                PrimitiveData::Sphere {
                    link,
                    center,
                    radius,
                } => {
                    assert_eq!(link, 0);
                    assert_eq!(center, [108., 112., 116.]);
                    assert_eq!(radius, 120.);
                }
            }
        }
        let cell = parsed.cell(0, 0).unwrap();
        assert_eq!(cell.pointer_words(), [0xdead_beef, 0xbaad_f00d]);
        assert_eq!(cell.max_y(), 42.);
        assert_eq!(cell.padding(), &[0xa5]);
        let refs: Vec<_> = cell.references().collect();
        assert_eq!(
            refs[0].primitive,
            PrimitiveId {
                kind: PrimitiveKind::Triangle,
                index: 1
            }
        );
        assert_eq!(refs[0].relative_byte_offset(), 32);
        assert_eq!(refs[0].flag, 7);
        assert_eq!(refs[2].primitive.kind, PrimitiveKind::Aabb);
        assert_eq!(refs[3].primitive.kind, PrimitiveKind::XtObb);
        assert!(parsed.cell(1, 0).is_none());
    }

    #[test]
    fn rejects_every_truncation_before_the_required_end() {
        let b = fixture();
        for end in 0..b.len() - 2 {
            assert!(parse_klz(&b[..end]).is_err(), "truncated length {end}");
        }
        assert!(parse_klz(&b[..b.len() - 2]).is_ok());
    }

    #[test]
    fn bounds_frame_name_scan_and_accepts_exact_quota() {
        for name_len in [4096, 4097] {
            let mut b = fixture();
            let mut link = vec![b'a'; 4 + ((name_len + 4) & !3)];
            put(&mut link, 0, 1);
            link[4 + name_len..].fill(0);
            let grid_offset = 40 + link.len() - 8;
            b.splice(32..40, link);
            put(&mut b, 8, grid_offset as u32);
            if name_len == 4096 {
                let p = parse_klz(&b).unwrap();
                assert_eq!(p.links[0].name.len(), 4096);
                assert_eq!(p.links[1].name.len(), 4096);
            } else {
                assert_eq!(
                    parse_klz(&b).unwrap_err().0,
                    "KLZ: frame name quota exceeded"
                );
            }
        }
    }

    #[test]
    fn rejects_bad_byte_references_and_structural_fields() {
        let original = fixture();
        let p = parse_klz(&original).unwrap();
        let ref_offset = p.cells[0].offset + 16;
        let triangle_offset = p.array(PrimitiveKind::Triangle).offset;
        let volume_offset = p.array(PrimitiveKind::Aabb).offset;
        let mutations = [
            (0, 0),
            (4, 4),
            (8, 25),
            (12, u32::MAX),
            (24, 31),
            (24, 39),
            (40 + 24, 0),
            (40 + 24, u32::MAX),
            (40 + 28, u32::MAX),
            (40 + 16, 0_f32.to_bits()),
            (40 + 16, f32::NAN.to_bits()),
            (40 + 104 + 4, 0),
            (40 + 0x30, u32::MAX),
            (40 + 0x60, u32::MAX),
            (triangle_offset, 8),
            (triangle_offset + 4, 0x7fff_0003),
            (triangle_offset + 4, 0x7ffe_0000),
            (volume_offset, 0x80),
            (volume_offset + 4, 2),
            (p.cells[0].offset, u32::MAX),
            (ref_offset, 1),
            (ref_offset, 64),
            (ref_offset, 0x0100_0000),
            (ref_offset, 0x8000_0001),
            (ref_offset, 0x8000_00b8),
        ];
        for (offset, value) in mutations {
            let mut b = original.clone();
            put(&mut b, offset, value);
            assert!(
                parse_klz(&b).is_err(),
                "accepted mutation {offset}:{value:x}"
            );
        }
        let mut b = original.clone();
        b[37] = b'x';
        b[38] = b'x';
        b[39] = b'x';
        assert!(parse_klz(&b).is_err());
    }
}
