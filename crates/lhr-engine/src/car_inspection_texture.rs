//! Bounded indexed BMP color-key admission for the explicit CPU RGBA8
//! inspection destination (native D3DFMT_A8R8G8B8, format21). This does not
//! select an original graphics-driver format or reconstruct Texture::Create.

pub(super) fn decode_rgba8_color_key(
    bytes: &[u8],
) -> Result<(lhr_assets::Texture, bool), lhr_assets::Error> {
    let mut texture = lhr_assets::decode_bmp_original(bytes)?;
    // The validated decoder admits BITMAPINFOHEADER with depth8 or24 only.
    // BGR24 color-key behavior stays outside this bounded inspection helper.
    if bytes.get(28..30) != Some(&8u16.to_le_bytes()) {
        return Ok((texture, false));
    }
    let palette = bytes
        .get(54..58)
        .ok_or_else(|| lhr_assets::Error("indexed color-key palette unavailable".into()))?;
    let key = [palette[2], palette[1], palette[0]];
    for pixel in texture.rgba.as_chunks_mut::<4>().0.iter_mut() {
        pixel[3] = if pixel[..3] == key { 0 } else { 255 };
    }
    Ok((texture, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rgba8_key_compares_palette_rgb_including_duplicate_entries_ignoring_reserved_alpha() {
        let mut bytes = vec![0u8; 74];
        bytes[..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&74u32.to_le_bytes());
        bytes[10..14].copy_from_slice(&66u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&3u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&2u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&8u16.to_le_bytes());
        bytes[46..50].copy_from_slice(&3u32.to_le_bytes());
        bytes[54..66].copy_from_slice(&[10, 20, 30, 0, 10, 20, 30, 255, 90, 80, 70, 0]);
        bytes[66..74].copy_from_slice(&[2, 0, 1, 0, 1, 2, 0, 0]);
        let original = lhr_assets::decode_bmp_original(&bytes).unwrap();
        let (keyed, proven) = decode_rgba8_color_key(&bytes).unwrap();
        assert!(proven);
        assert!(original
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255));
        assert_eq!(
            keyed
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| pixel[3])
                .collect::<Vec<_>>(),
            [0, 255, 0, 255, 0, 0]
        );
        for (opaque, keyed) in original
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(keyed.rgba.as_chunks::<4>().0.iter())
        {
            assert_eq!(&opaque[..3], &keyed[..3]);
        }
        bytes[54 + 3] = 137;
        assert_eq!(decode_rgba8_color_key(&bytes).unwrap().0.rgba, keyed.rgba);
    }
}
