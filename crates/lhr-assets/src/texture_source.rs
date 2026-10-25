//! Source pixels and metadata required by original texture creation. RGBA alone
//! cannot distinguish 24-bit opaque sources from 32-bit sources carrying alpha.
use crate::{Error, Texture};
#[derive(Debug)]
pub struct DecodedTextureImage {
    pub texture: Texture,
    pub bpp: u16,
    pub palette_key: Option<[u8; 3]>,
    /// Native decoded source row stride, before conversion to packed RGBA.
    pub source_pitch: u32,
}
#[derive(Debug)]
pub enum TextureImageDecodeError {
    /// Well-identified input outside this bounded original-creation decoder.
    Unsupported(&'static str),
    Corrupt(Error),
}
impl From<Error> for TextureImageDecodeError {
    fn from(error: Error) -> Self {
        Self::Corrupt(error)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureImageInfo {
    pub width: u32,
    pub height: u32,
    pub bpp: u16,
}
/// Header-only image information, before pixel loading. Keeping this separate
/// preserves Create's visible state when a valid header has truncated pixels.
pub fn inspect_texture_image(bytes: &[u8]) -> Result<TextureImageInfo, TextureImageDecodeError> {
    let corrupt =
        || TextureImageDecodeError::Corrupt(Error("texture image header truncated".into()));
    if bytes.starts_with(b"BM") {
        if bytes.len() < 54 {
            return Err(corrupt());
        }
        let width = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
        let height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
        let bpp = u16::from_le_bytes([bytes[28], bytes[29]]);
        if width <= 0 || height <= 0 {
            return Err(corrupt());
        }
        Ok(TextureImageInfo {
            width: width as u32,
            height: height as u32,
            bpp,
        })
    } else {
        if bytes.len() < 18 {
            return Err(corrupt());
        }
        Ok(TextureImageInfo {
            width: u16::from_le_bytes([bytes[12], bytes[13]]) as u32,
            height: u16::from_le_bytes([bytes[14], bytes[15]]) as u32,
            bpp: bytes[16] as u16,
        })
    }
}
pub fn decode_texture_image(bytes: &[u8]) -> Result<DecodedTextureImage, TextureImageDecodeError> {
    let info = inspect_texture_image(bytes)?;
    if bytes.len() > 64 * 1024 * 1024
        || info.width > 8192
        || info.height > 8192
        || u64::from(info.width) * u64::from(info.height) > 16_777_216
    {
        return Err(TextureImageDecodeError::Unsupported("image quota"));
    }
    if bytes.starts_with(b"BM") {
        if u32::from_le_bytes(bytes[14..18].try_into().unwrap()) != 40
            || u32::from_le_bytes(bytes[30..34].try_into().unwrap()) != 0
        {
            return Err(TextureImageDecodeError::Unsupported("BMP source layout"));
        }
        if !matches!(info.bpp, 8 | 16 | 24 | 32) {
            return Err(TextureImageDecodeError::Unsupported("BMP source depth"));
        }
        let texture = crate::decode_bmp_original(bytes)?;
        // The validated BMP decoder requires a 40-byte BITMAPINFOHEADER and
        // admits BI_RGB indexed8/X1R5G5B5/BGR24/BGRA32; indexed palette entry0
        // is its color-key source.
        let bpp = info.bpp;
        let palette_key = if bpp == 8 {
            Some([bytes[56], bytes[55], bytes[54]])
        } else {
            None
        };
        Ok(DecodedTextureImage {
            texture,
            bpp,
            palette_key,
            source_pitch: (info.width * u32::from(bpp)).div_ceil(32) * 4,
        })
    } else {
        if bytes.len() >= 18
            && (!crate::tga::supported_pixel_layout(bytes)
                || bytes[17] & 0xd0 != 0
                || (matches!(info.bpp, 8 | 24) && bytes[17] & 0x0f != 0))
        {
            // The reusable decoder handles right-origin images, but original
            // LoadImage only diagnoses that bit. Do not claim equivalent pixels.
            return Err(TextureImageDecodeError::Unsupported("TGA source layout"));
        }
        let texture = crate::decode_tga(bytes)?;
        let palette_key = if info.bpp == 8 {
            let start = 18 + bytes[0] as usize;
            Some([bytes[start + 2], bytes[start + 1], bytes[start]])
        } else {
            None
        };
        Ok(DecodedTextureImage {
            texture,
            bpp: info.bpp,
            palette_key,
            source_pitch: info.width * u32::from(info.bpp) / 8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_information_survives_truncated_pixels_and_capability_errors_stay_distinct() {
        let mut bytes = vec![0; 18];
        bytes[2] = 2;
        bytes[12] = 2;
        bytes[14] = 2;
        bytes[16] = 24;
        bytes[17] = 0x20;
        assert_eq!(
            inspect_texture_image(&bytes).unwrap(),
            TextureImageInfo {
                width: 2,
                height: 2,
                bpp: 24
            }
        );
        assert!(matches!(
            decode_texture_image(&bytes),
            Err(TextureImageDecodeError::Corrupt(_))
        ));
        for depth in [8, 15, 64] {
            bytes[16] = depth;
            assert!(matches!(
                decode_texture_image(&bytes),
                Err(TextureImageDecodeError::Unsupported(_))
            ));
        }
        bytes[16] = 24;
        for descriptor in [0x10, 0x40, 0x80, 1] {
            bytes[17] = descriptor;
            assert!(matches!(
                decode_texture_image(&bytes),
                Err(TextureImageDecodeError::Unsupported(_))
            ));
        }
    }
    #[test]
    fn sixteen_bit_source_preserves_native_shift_expansion_and_alpha_bit() {
        let mut bytes = vec![0; 18];
        bytes[2] = 2;
        bytes[12] = 2;
        bytes[14] = 1;
        bytes[16] = 16;
        bytes[17] = 0x20;
        bytes.extend_from_slice(&[0xff, 0x7f, 0x21, 0x84]);
        let image = decode_texture_image(&bytes).unwrap();
        assert_eq!(image.bpp, 16);
        assert_eq!(image.texture.rgba, [248, 248, 248, 0, 8, 8, 8, 255]);
    }
}
