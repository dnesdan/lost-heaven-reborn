use super::{Error, Result, decode_windows1250, err};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const MAX_STRING_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_TEXT_BYTES: usize = 16 * 1024 * 1024;
const MAX_MENU_ITEMS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDbEntry {
    pub id: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDb {
    /// Opaque second header word. Its meaning is not established.
    pub unknown: u32,
    pub entries: Vec<TextDbEntry>,
}
impl TextDb {
    pub fn get(&self, id: u32) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.text.as_str())
    }
}

/// Parse Czech/Polish Windows-1250 TextDB `.def` files. Other language encodings
/// are not inferred by this function.
pub fn parse_text_db(b: &[u8]) -> Result<TextDb> {
    if b.len() > MAX_BYTES {
        return err("TextDB exceeds 64 MiB quota");
    }
    if b.len() < 8 {
        return err("TextDB header truncated");
    }
    let count = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
    let table_end = 8usize
        .checked_add(
            count
                .checked_mul(8)
                .ok_or_else(|| Error("TextDB count overflow".into()))?,
        )
        .ok_or_else(|| Error("TextDB table overflow".into()))?;
    if count > MAX_ENTRIES || table_end > b.len() {
        return err("TextDB index outside file");
    }
    let unknown = u32::from_le_bytes(b[4..8].try_into().unwrap());
    let mut entries = Vec::with_capacity(count);
    let mut total_text_bytes = 0usize;
    for i in 0..count {
        let at = 8 + i * 8;
        let id = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let offset = u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap()) as usize;
        if offset < table_end || offset >= b.len() {
            return err("TextDB string offset outside heap");
        }
        let tail = &b[offset..];
        let end = tail
            .iter()
            .take(MAX_STRING_BYTES + 1)
            .position(|x| *x == 0)
            .ok_or_else(|| Error("TextDB string terminator missing".into()))?;
        if end > MAX_STRING_BYTES {
            return err("TextDB string exceeds 1 MiB quota");
        }
        total_text_bytes = total_text_bytes
            .checked_add(end + 1)
            .ok_or_else(|| Error("TextDB text size overflow".into()))?;
        if total_text_bytes > MAX_TOTAL_TEXT_BYTES {
            return err("TextDB decoded text exceeds 16 MiB quota");
        }
        entries.push(TextDbEntry {
            id,
            text: decode_windows1250(&tail[..end])?,
        });
    }
    Ok(TextDb { unknown, entries })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontGlyph {
    pub u_left: f32,
    pub v_top: f32,
    pub u_right: f32,
    /// Advance/width value stored by the original font table.
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontMetrics {
    /// The 28-byte header is retained because several fields have no established meaning.
    pub raw_header: [u8; 28],
    pub height_scale: f32,
    pub width_scale: f32,
    pub uv_height: f32,
    pub tracking: f32,
    pub baseline: f32,
    /// Glyph records are indexed by the engine's 8-bit glyph value.
    pub glyphs: Vec<FontGlyph>,
}

/// Parse the original GFnt metrics table (28-byte header + 256 fixed 16-byte glyph records).
pub fn parse_font_metrics(b: &[u8]) -> Result<FontMetrics> {
    if b.len() != 28 + 256 * 16 {
        return err("font metrics must contain exactly 256 glyph records");
    }
    if &b[..4] != b"GFnt" {
        return err("invalid GFnt signature");
    }
    let mut raw_header = [0; 28];
    raw_header.copy_from_slice(&b[..28]);
    let read_float = |p: usize| f32::from_bits(u32::from_le_bytes(b[p..p + 4].try_into().unwrap()));
    let height_scale = read_float(8);
    let width_scale = read_float(12);
    let uv_height = read_float(16);
    let tracking = read_float(20);
    let baseline = read_float(24);
    if [height_scale, width_scale, uv_height, tracking, baseline]
        .iter()
        .any(|v| !v.is_finite())
    {
        return err("font metrics header contains nonfinite value");
    }
    let mut glyphs = Vec::with_capacity(256);
    for i in 0..256 {
        let p = 28 + i * 16;
        let glyph = FontGlyph {
            u_left: read_float(p),
            v_top: read_float(p + 4),
            u_right: read_float(p + 8),
            width: read_float(p + 12),
        };
        if [glyph.u_left, glyph.v_top, glyph.u_right, glyph.width]
            .iter()
            .any(|v| !v.is_finite())
        {
            return err("font glyph contains nonfinite value");
        }
        glyphs.push(glyph);
    }
    Ok(FontMetrics {
        raw_header,
        height_scale,
        width_scale,
        uv_height,
        tracking,
        baseline,
        glyphs,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageFontMap {
    /// Unknown language metadata, including the two name fields and font suffix.
    pub raw_header: [u8; 56],
    pub code: String,
    pub display_name: String,
    pub font_suffix: String,
    /// 256 engine lookup values indexed by Windows-1250 input byte.
    pub glyph_map: [u16; 256],
}
impl LanguageFontMap {
    pub fn glyph(&self, input: u8) -> u16 {
        self.glyph_map[input as usize]
    }
}

/// Parse the observed 56-byte language metadata header and 256 little-endian map entries.
pub fn parse_language_font_map(b: &[u8]) -> Result<LanguageFontMap> {
    if b.len() != 56 + 256 * 2 {
        return err("language font map must contain exactly 256 entries");
    }
    let mut raw_header = [0; 56];
    raw_header.copy_from_slice(&b[..56]);
    let read_name = |start: usize, end: usize| -> Result<String> {
        let bytes = &b[start..end];
        let len = bytes.iter().position(|v| *v == 0).unwrap_or(bytes.len());
        decode_windows1250(&bytes[..len])
    };
    let marker = read_name(0, 8)?;
    let code = marker
        .strip_prefix("Lang")
        .ok_or_else(|| Error("invalid language marker".into()))?
        .to_owned();
    let display_name = read_name(8, 40)?;
    let font_suffix = read_name(40, 56)?;
    let mut glyph_map = [0u16; 256];
    for (i, glyph) in glyph_map.iter_mut().enumerate() {
        let p = 56 + i * 2;
        *glyph = u16::from_le_bytes(b[p..p + 2].try_into().unwrap());
    }
    Ok(LanguageFontMap {
        raw_header,
        code,
        display_name,
        font_suffix,
        glyph_map,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub id: u32,
    /// Four-byte game widget tag in file order (e.g. `b"ttub"`).
    pub kind: [u8; 4],
    /// Original canvas coordinates, unscaled.
    pub rect: [f32; 4],
    pub text_id: u32,
    /// Opaque style/flags word.
    pub style: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuDefinition {
    /// Opaque first header word following the signature.
    pub version: u32,
    /// Opaque fourth header word.
    pub reserved: u32,
    pub items: Vec<MenuItem>,
}
impl MenuDefinition {
    pub fn item(&self, id: u32) -> Option<&MenuItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

/// Parse the evidenced `Menu` header followed by fixed 32-byte UI records.
/// Widget tags and style bits remain opaque values.
pub fn parse_menu_definition(b: &[u8]) -> Result<MenuDefinition> {
    if b.len() > MAX_BYTES {
        return err("menu definition exceeds 64 MiB quota");
    }
    if b.len() < 16 || &b[..4] != b"Menu" {
        return err("invalid menu definition signature");
    }
    let version = u32::from_le_bytes(b[4..8].try_into().unwrap());
    let count = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
    let reserved = u32::from_le_bytes(b[12..16].try_into().unwrap());
    if count > MAX_MENU_ITEMS {
        return err("menu item quota exceeded");
    }
    let end = 16usize
        .checked_add(
            count
                .checked_mul(32)
                .ok_or_else(|| Error("menu item count overflow".into()))?,
        )
        .ok_or_else(|| Error("menu item extent overflow".into()))?;
    if end != b.len() {
        return err("menu definition record extent mismatch");
    }
    let mut items = Vec::with_capacity(count);
    for i in 0..count {
        let at = 16 + i * 32;
        let id = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let kind: [u8; 4] = b[at + 4..at + 8].try_into().unwrap();
        if !kind.iter().all(|v| (0x20..=0x7e).contains(v)) {
            return err("menu widget tag is not printable ASCII");
        }
        let mut rect = [0f32; 4];
        for (n, value) in rect.iter_mut().enumerate() {
            let p = at + 8 + n * 4;
            *value = f32::from_bits(u32::from_le_bytes(b[p..p + 4].try_into().unwrap()));
            if !value.is_finite() {
                return err("menu rectangle contains nonfinite value");
            }
        }
        let text_id = u32::from_le_bytes(b[at + 24..at + 28].try_into().unwrap());
        let style = u32::from_le_bytes(b[at + 28..at + 32].try_into().unwrap());
        items.push(MenuItem {
            id,
            kind,
            rect,
            text_id,
            style,
        });
    }
    Ok(MenuDefinition {
        version,
        reserved,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn textdb() -> Vec<u8> {
        let mut b = vec![0; 24];
        b[0..4].copy_from_slice(&2u32.to_le_bytes());
        b[4..8].copy_from_slice(&0x12345678u32.to_le_bytes());
        b[8..12].copy_from_slice(&101u32.to_le_bytes());
        b[12..16].copy_from_slice(&24u32.to_le_bytes());
        b[16..20].copy_from_slice(&102u32.to_le_bytes());
        b[20..24].copy_from_slice(&28u32.to_le_bytes());
        b.extend_from_slice(b"P\xe1\xed\0Ahoj\0");
        b
    }
    #[test]
    fn parses_textdb_cp1250_and_opaque_header() {
        let db = parse_text_db(&textdb()).unwrap();
        assert_eq!(db.unknown, 0x12345678);
        assert_eq!(db.get(101), Some("Páí"));
        assert_eq!(db.get(102), Some("Ahoj"));
        assert_eq!(db.get(999), None);
    }
    #[test]
    fn rejects_bad_textdb_offsets_and_missing_terminator() {
        let mut b = textdb();
        b[12..16].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_text_db(&b).is_err());
        let mut b = textdb();
        b.truncate(27);
        assert!(parse_text_db(&b).is_err());
    }
    #[test]
    fn rejects_repeated_offsets_that_exceed_aggregate_text_quota() {
        let count = 17usize;
        let table_end = 8 + count * 8;
        let mut b = vec![0; table_end];
        b[0..4].copy_from_slice(&(count as u32).to_le_bytes());
        for i in 0..count {
            let at = 8 + i * 8;
            b[at..at + 4].copy_from_slice(&(i as u32).to_le_bytes());
            b[at + 4..at + 8].copy_from_slice(&(table_end as u32).to_le_bytes());
        }
        b.extend(std::iter::repeat_n(b'a', MAX_STRING_BYTES));
        b.push(0);
        assert!(parse_text_db(&b).is_err());
    }
    fn menu_fixture() -> Vec<u8> {
        let mut b = b"Menu".to_vec();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&99u32.to_le_bytes());
        b.extend_from_slice(b"ttub");
        for f in [10.0f32, 20.0, 200.0, 30.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&102u32.to_le_bytes());
        b.extend_from_slice(&0xff00b1u32.to_le_bytes());
        b
    }
    #[test]
    fn parses_menu_layout_and_keeps_tag_and_style() {
        let m = parse_menu_definition(&menu_fixture()).unwrap();
        let i = m.item(99).unwrap();
        assert_eq!(i.kind, *b"ttub");
        assert_eq!(i.rect, [10.0, 20.0, 200.0, 30.0]);
        assert_eq!((i.text_id, i.style), (102, 0xff00b1));
    }
    #[test]
    fn parses_gfnt_metrics_and_opaque_header() {
        let mut b = vec![0; 28 + 256 * 16];
        b[..4].copy_from_slice(b"GFnt");
        b[8..12].copy_from_slice(&1.1f32.to_le_bytes());
        b[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        b[16..20].copy_from_slice(&0.2f32.to_le_bytes());
        b[20..24].copy_from_slice(&0.13f32.to_le_bytes());
        b[24..28].copy_from_slice(&0.3f32.to_le_bytes());
        let p = 28 + 33 * 16;
        for (i, v) in [0.1f32, 0.2, 0.3, 0.4].into_iter().enumerate() {
            b[p + i * 4..p + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        let f = parse_font_metrics(&b).unwrap();
        assert_eq!(f.height_scale, 1.1);
        assert_eq!(f.width_scale, 1.0);
        assert_eq!(f.uv_height, 0.2);
        assert_eq!(f.tracking, 0.13);
        assert_eq!(f.baseline, 0.3);
        assert_eq!(
            f.glyphs[33],
            FontGlyph {
                u_left: 0.1,
                v_top: 0.2,
                u_right: 0.3,
                width: 0.4
            }
        );
        assert_eq!(&f.raw_header[..4], b"GFnt");
    }
    #[test]
    fn reads_language_map_and_keeps_uninterpreted_header() {
        let mut b = vec![0; 568];
        b[0..6].copy_from_slice(b"Langcz");
        b[8..14].copy_from_slice(b"Czech\0");
        b[40..45].copy_from_slice(b"_lat\0");
        b[56 + 0x8a * 2..56 + 0x8a * 2 + 2].copy_from_slice(&0x008au16.to_le_bytes());
        let map = parse_language_font_map(&b).unwrap();
        assert_eq!(
            (
                map.code.as_str(),
                map.display_name.as_str(),
                map.font_suffix.as_str()
            ),
            ("cz", "Czech", "_lat")
        );
        assert_eq!(map.glyph(b'Z'), 0);
        assert_eq!(map.glyph(0x8a), 0x8a);
        assert_eq!(&map.raw_header[..6], b"Langcz");
    }
    #[test]
    fn rejects_malformed_menu_extent_and_nonfinite_rect() {
        let mut b = menu_fixture();
        b.pop();
        assert!(parse_menu_definition(&b).is_err());
        let mut b = menu_fixture();
        b[24..28].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse_menu_definition(&b).is_err());
    }
}
