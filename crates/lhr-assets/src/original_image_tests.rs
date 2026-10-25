//! Synthetic PC image records. All fixtures are created locally from public
//! source contracts; no original game assets are included.
use super::{decode_bmp, decode_bmp_original, texture_source};

fn bmp(depth: u16, width: i32, height: i32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; 54];
    bytes[..2].copy_from_slice(b"BM");
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&width.to_le_bytes());
    bytes[22..26].copy_from_slice(&height.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&depth.to_le_bytes());
    bytes.extend(pixels);
    let size = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&size.to_le_bytes());
    bytes
}
#[test]
fn original_bmp16_uses_1555_shifts_alpha_and_padded_bottom_up_rows() {
    // One pixel per row exercises 4-byte BMP padding. Bottom red has alpha0;
    // top cyan has alpha1; pad bytes must never become image data.
    let bytes = bmp(16, 1, 2, &[0, 0x7c, 0xad, 0xde, 0xff, 0x83, 0xbe, 0xef]);
    let image = texture_source::decode_texture_image(&bytes).unwrap();
    assert_eq!(image.bpp, 16);
    assert_eq!(image.source_pitch, 4);
    assert_eq!(image.palette_key, None);
    assert_eq!(image.texture.rgba, [0, 248, 248, 255, 248, 0, 0, 0]);
    assert!(decode_bmp(&bytes).is_err()); // Preserve the generic decoder's API policy.
    // Generic original decode still supports its existing top-down extension;
    // the native source adapter keeps rejecting that unsupported PC layout.
    let top_down = bmp(16, 1, -2, &[0, 0x7c, 0, 0, 0xff, 0x83, 0, 0]);
    assert_eq!(
        decode_bmp_original(&top_down).unwrap().rgba,
        [248, 0, 0, 0, 0, 248, 248, 255]
    );
    assert!(texture_source::decode_texture_image(&top_down).is_err());
}
#[test]
fn original_bmp32_preserves_each_source_alpha_byte_and_row_order() {
    let bytes = bmp(
        32,
        2,
        2,
        &[1, 2, 3, 0, 4, 5, 6, 17, 7, 8, 9, 128, 10, 11, 12, 255],
    );
    let image = texture_source::decode_texture_image(&bytes).unwrap();
    assert_eq!(image.bpp, 32);
    assert_eq!(image.source_pitch, 8);
    assert_eq!(image.palette_key, None);
    assert_eq!(
        image.texture.rgba,
        [9, 8, 7, 128, 12, 11, 10, 255, 3, 2, 1, 0, 6, 5, 4, 17]
    );
}
#[test]
fn original_bmp_depth_extensions_keep_compression_extent_and_dimension_guards() {
    for depth in [16, 32] {
        let bytes = bmp(depth, 1, 1, &[0xff; 4]);
        assert!(decode_bmp_original(&bytes).is_ok());
        for end in 0..bytes.len() {
            assert!(decode_bmp_original(&bytes[..end]).is_err(), "{depth}/{end}");
        }
        // Native LoadImage rejects every nonzero compression, including
        // BI_BITFIELDS. Do not reinterpret 565 or add externally guessed masks.
        for compression in [1u32, 2, 3, 6] {
            let mut invalid = bytes.clone();
            invalid[30..34].copy_from_slice(&compression.to_le_bytes());
            assert!(decode_bmp_original(&invalid).is_err());
            assert!(texture_source::decode_texture_image(&invalid).is_err());
        }
        for (start, value) in [(18, 0i32), (18, 8193), (22, 0), (22, i32::MIN)] {
            let mut invalid = bytes.clone();
            invalid[start..start + 4].copy_from_slice(&value.to_le_bytes());
            assert!(decode_bmp_original(&invalid).is_err());
        }
        for offset in [0u32, 53, 58, u32::MAX] {
            let mut invalid = bytes.clone();
            invalid[10..14].copy_from_slice(&offset.to_le_bytes());
            assert!(decode_bmp_original(&invalid).is_err());
        }
        let mut invalid = bytes;
        invalid[14..18].copy_from_slice(&108u32.to_le_bytes());
        assert!(decode_bmp_original(&invalid).is_err());
    }
}

fn indexed_tga(descriptor: u8, indices: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; 18];
    bytes[0] = 3;
    bytes[1] = 1;
    bytes[2] = 1;
    bytes[5..7].copy_from_slice(&256u16.to_le_bytes());
    bytes[7] = 24;
    bytes[12..14].copy_from_slice(&2u16.to_le_bytes());
    bytes[14..16].copy_from_slice(&2u16.to_le_bytes());
    bytes[16] = 8;
    bytes[17] = descriptor;
    bytes.extend(*b"id!");
    for index in 0..256u16 {
        bytes.extend([index as u8, 255 - index as u8, index as u8 ^ 0x55]);
    }
    bytes.extend(indices);
    bytes
}
#[test]
fn indexed_tga_retains_palette_key_pitch_and_top_down_pixels() {
    let bytes = indexed_tga(0, &[0, 255, 1, 128]);
    let image = texture_source::decode_texture_image(&bytes).unwrap();
    assert_eq!(image.bpp, 8);
    assert_eq!(image.source_pitch, 2);
    assert_eq!(image.palette_key, Some([0x55, 255, 0]));
    assert_eq!(
        image.texture.rgba,
        [
            0x54, 254, 1, 255, 0xd5, 127, 128, 255, 0x55, 255, 0, 255, 0xaa, 0, 255, 255
        ]
    );
    let top_down = indexed_tga(0x20, &[1, 128, 0, 255]);
    assert_eq!(
        texture_source::decode_texture_image(&top_down)
            .unwrap()
            .texture
            .rgba,
        image.texture.rgba
    );
    // Generic orientation support remains separate from native right-origin
    // semantics, which only diagnose that bit and do not reverse the row.
    let right_origin = indexed_tga(0x30, &[128, 1, 255, 0]);
    assert_eq!(
        super::decode_tga(&right_origin).unwrap().rgba,
        image.texture.rgba
    );
    assert!(texture_source::decode_texture_image(&right_origin).is_err());
}
#[test]
fn indexed_tga_extents_unsupported_palette_layouts_and_footer_are_bounded() {
    let bytes = indexed_tga(0x20, &[0, 1, 2, 255]);
    for end in 0..bytes.len() {
        assert!(super::decode_tga(&bytes[..end]).is_err(), "{end}");
    }
    for (offset, value) in [
        (1, 0),
        (2, 9),
        (3, 1),
        (5, 255),
        (6, 0),
        (7, 16),
        (7, 32),
        (16, 16),
        (17, 1),
        (17, 0x40),
        (17, 0x80),
    ] {
        let mut invalid = bytes.clone();
        invalid[offset] = value;
        assert!(super::decode_tga(&invalid).is_err(), "{offset}/{value}");
    }
    for (offset, value) in [(12, 0u16), (12, 8193), (14, 0), (14, 8193)] {
        let mut invalid = bytes.clone();
        invalid[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        assert!(super::decode_tga(&invalid).is_err());
    }
    let mut footer = bytes.clone();
    footer.extend([0; 8]);
    footer.extend(b"TRUEVISION-XFILE.\0");
    assert_eq!(
        super::decode_tga(&footer).unwrap().rgba,
        super::decode_tga(&bytes).unwrap().rgba
    );
    footer.push(0);
    assert!(super::decode_tga(&footer).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(super::decode_tga(&extra).is_err());
}
