use super::{Error, Result, Texture, err};

const MAX_BYTES: usize = 64 * 1024 * 1024;

// The original LoadImage reads 256 BGR triples for an indexed8 source,
// irrespective of the TGA color-map origin/length fields. Admit only the
// layout whose declared map agrees with those source reads.
pub(super) fn supported_pixel_layout(header: &[u8]) -> bool {
    header.len() >= 18
        && ((header[1] == 0 && header[2] == 2 && matches!(header[16], 16 | 24 | 32))
            || (header[1] == 1
                && header[2] == 1
                && header[16] == 8
                && u16::from_le_bytes([header[3], header[4]]) == 0
                && u16::from_le_bytes([header[5], header[6]]) == 256
                && header[7] == 24))
}

/// Decode uncompressed true-color TGA (X1R5G5B5/BGR24/BGRA32), or indexed8
/// with a complete 256-entry BGR24 palette, to top-down RGBA.
/// The optional 26-byte Truevision footer is accepted; other trailing data is rejected.
pub fn decode_tga(b: &[u8]) -> Result<Texture> {
    if b.len() > MAX_BYTES {
        return err("TGA exceeds 64 MiB quota");
    }
    if b.len() < 18 {
        return err("TGA header truncated");
    }
    let id_len = b[0] as usize;
    let indexed = b[1] == 1;
    let width = u16::from_le_bytes([b[12], b[13]]) as usize;
    let height = u16::from_le_bytes([b[14], b[15]]) as usize;
    let depth = b[16];
    let descriptor = b[17];
    if !supported_pixel_layout(b) {
        return err("unsupported TGA pixel or palette layout");
    }
    if width == 0 || height == 0 || width > 8192 || height > 8192 || width * height > 16_777_216 {
        return err("TGA dimensions outside quota");
    }
    if descriptor & 0xc0 != 0 || (matches!(depth, 8 | 24) && descriptor & 0x0f != 0) {
        return err("unsupported TGA interleave or alpha layout");
    }
    let bytes_per_pixel = depth as usize / 8;
    let palette_start = 18usize
        .checked_add(id_len)
        .ok_or_else(|| Error("TGA extent overflow".into()))?;
    let palette_bytes = if indexed { 256 * 3 } else { 0 };
    let data_start = palette_start
        .checked_add(palette_bytes)
        .ok_or_else(|| Error("TGA extent overflow".into()))?;
    // Prove the full ID and palette extent before reading any palette entry.
    if data_start > b.len() {
        return err("TGA image ID or palette truncated");
    }
    let pixel_bytes = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(bytes_per_pixel))
        .ok_or_else(|| Error("TGA extent overflow".into()))?;
    let data_end = data_start
        .checked_add(pixel_bytes)
        .ok_or_else(|| Error("TGA extent overflow".into()))?;
    if data_end > b.len() {
        return err("TGA pixels truncated");
    }
    if data_end != b.len()
        && !(data_end + 26 == b.len() && &b[data_end + 8..] == b"TRUEVISION-XFILE.\0")
    {
        return err("unexpected trailing TGA data");
    }
    let top_origin = descriptor & 0x20 != 0;
    let right_origin = descriptor & 0x10 != 0;
    let mut rgba = vec![0; width * height * 4];
    for sy in 0..height {
        let y = if top_origin { sy } else { height - 1 - sy };
        for sx in 0..width {
            let x = if right_origin { width - 1 - sx } else { sx };
            let src = data_start + (sy * width + sx) * bytes_per_pixel;
            let dst = (y * width + x) * 4;
            if indexed {
                let entry = palette_start + b[src] as usize * 3;
                rgba[dst..dst + 4].copy_from_slice(&[b[entry + 2], b[entry + 1], b[entry], 255]);
            } else if depth == 16 {
                let c = u16::from_le_bytes([b[src], b[src + 1]]);
                rgba[dst..dst + 4].copy_from_slice(&super::decode_x1r5g5b5(c));
            } else {
                rgba[dst..dst + 4].copy_from_slice(&[
                    b[src + 2],
                    b[src + 1],
                    b[src],
                    if depth == 32 { b[src + 3] } else { 255 },
                ]);
            }
        }
    }
    Ok(Texture {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(depth: u8, descriptor: u8, pixels: &[u8], footer: bool) -> Vec<u8> {
        let mut b = vec![0; 18];
        b[2] = 2;
        b[12..14].copy_from_slice(&2u16.to_le_bytes());
        b[14..16].copy_from_slice(&1u16.to_le_bytes());
        b[16] = depth;
        b[17] = descriptor;
        b.extend_from_slice(pixels);
        if footer {
            b.extend_from_slice(&[0; 8]);
            b.extend_from_slice(b"TRUEVISION-XFILE.\0");
        }
        b
    }
    #[test]
    fn decodes_bottom_origin_bgr_and_truevision_footer() {
        let t = decode_tga(&fixture(24, 0, &[1, 2, 3, 4, 5, 6], true)).unwrap();
        assert_eq!((t.width, t.height), (2, 1));
        assert_eq!(t.rgba, [3, 2, 1, 255, 6, 5, 4, 255]);
    }
    #[test]
    fn decodes_rgba_and_origin_bits() {
        let t = decode_tga(&fixture(32, 0x30, &[1, 2, 3, 4, 5, 6, 7, 8], false)).unwrap();
        assert_eq!(t.rgba, [7, 6, 5, 8, 3, 2, 1, 4]);
    }
    #[test]
    fn rejects_compressed_truncated_and_overflowing_tga() {
        assert!(decode_tga(&fixture(24, 0, &[0; 6], false)[..17]).is_err());
        let mut compressed = fixture(24, 0, &[0; 6], false);
        compressed[2] = 10;
        assert!(decode_tga(&compressed).is_err());
        assert!(decode_tga(&fixture(24, 0, &[0; 3], false)).is_err());
        let mut huge = fixture(32, 0, &[0; 8], false);
        huge[12..14].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(decode_tga(&huge).is_err());
    }
}
