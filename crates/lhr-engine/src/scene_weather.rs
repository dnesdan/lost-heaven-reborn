//! Scene weather and sector sound-property defaults from original LS3DF.
//!
//! PC SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Raw words preserve unknown fields without assigning guessed member names.
//! These operations do not construct a Scene, create Snow.tga or own resources.

/// Native scene allocation size, 0x33c bytes, expressed as little-endian words.
pub const SCENE_WORDS: usize = 0x33c / 4;
/// Native S_sector_sound_properties record size, 0x68 bytes.
pub const SECTOR_SOUND_PROPERTIES_WORDS: usize = 26;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WeatherResetUnavailable;

/// Original WeatherSystemReset, PC RVA0x4b3b0, under Game's PC24/RN
/// arithmetic policy. Only incoming bits0 and6 influence the result. Unknown
/// words remain untouched. Numeric helpers admit the fixed native angle and
/// direction; their host availability error is not a native return code.
pub fn weather_system_reset(scene: &mut [u32; SCENE_WORDS]) -> Result<(), WeatherResetUnavailable> {
    use crate::native_precision::{add, div, mul, sqrt, sub};
    use crate::native_transform::{try_rounded_sin_cos, SMatrix};
    let input = scene[0x298 / 4];
    scene[0x298 / 4] = (input & 0x41) | 6;
    let parameters = if input & 0x40 != 0 {
        [0x3b03126f, 0x3e99999a, 0x3e19999a]
    } else {
        [0x3ccccccd, 0x3fc00000, 0x3cf5c28f]
    };
    scene[0x2ac / 4..0x2b8 / 4].copy_from_slice(&parameters);
    scene[0x2b8 / 4..0x2c8 / 4].copy_from_slice(&[25f32.to_bits(), 12f32.to_bits(), 1000, 3000]);
    let (sine, cosine) =
        try_rounded_sin_cos(f32::from_bits(0x3e4ccccd)).map_err(|_| WeatherResetUnavailable)?;
    let mut rotation = SMatrix::identity();
    rotation.elements[0] = cosine;
    rotation.elements[1] = sine;
    rotation.elements[4] = -sine;
    rotation.elements[5] = cosine;
    let direction = rotation
        .try_transform_point([0., -1., 0.])
        .map_err(|_| WeatherResetUnavailable)?;
    for offset in [0x2c8 / 4, 0x2d4 / 4] {
        scene[offset..offset + 3].copy_from_slice(&direction.map(f32::to_bits));
    }
    scene[0x320 / 4] = 0x80606060;
    scene[0x324 / 4] = 0x606060;

    // SetDir RVA0x2d9d0 on precisely the direction generated above. Its
    // general small-vector/fallback paths are not exposed as a separate port.
    let [dx, dy, dz] = direction;
    let squared = add(add(mul(dz, dz), mul(dy, dy)), mul(dx, dx));
    let mut matrix = SMatrix::identity();
    let z = if f64::from(sub(squared, 1.).abs()) < 1e-8 {
        direction
    } else {
        let reciprocal = div(1., sqrt(squared));
        direction.map(|component| mul(reciprocal, component))
    };
    matrix.elements[8..11].copy_from_slice(&z);
    let x_squared = add(mul(-dx, -dx), mul(dz, dz));
    let reciprocal = div(1., sqrt(x_squared));
    let x = [
        mul(reciprocal, dz),
        mul(reciprocal, 0.),
        mul(-dx, reciprocal),
    ];
    matrix.elements[..3].copy_from_slice(&x);
    let y = [
        -sub(mul(x[1], dz), mul(x[2], dy)),
        -sub(mul(x[2], dx), mul(dz, x[0])),
        -sub(mul(dy, x[0]), mul(x[1], dx)),
    ];
    matrix.elements[4..7].copy_from_slice(&y);
    scene[0x2e0 / 4..0x320 / 4].copy_from_slice(&matrix.elements.map(f32::to_bits));
    scene[0x2a4 / 4] = 0;
    scene[0x328 / 4] = sqrt(add(mul(25., 25.), mul(12., 12.))).to_bits();
    Ok(())
}

/// Original SetupDefaults__25S_sector_sound_propertiesFv (PS2 0x453910),
/// corresponding to PC RVA0x4e9b0. Replaces all 26 words; integer and
/// float fields retain their original representation without inferred labels.
pub fn setup_sector_sound_properties_defaults(record: &mut [u32; SECTOR_SOUND_PROPERTIES_WORDS]) {
    *record = [
        0x3f800000, 0x3f7d70a4, 0x5d2, 0x3ec4b73e, 0x3f2c2bf0, 7, 0x3f59999a, 10, 0x3f800000, 500,
        0x3ef286bd, 0, 250, 0, 250, 0, 0, 0x3f733333, 0x3e579436, 0x3e705398, 0, 0, 0, 0, 0, 0,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_preserves_only_the_two_input_bits_and_unrelated_words() {
        for flags in [0, 1, 0x40, 0x41, 0xffff_ffbe, u32::MAX] {
            for incoming_precision in [false, true] {
                let mut state =
                    crate::native_precision::Pc24PrecisionState::new(incoming_precision);
                let mut scene = [0xdeadbeef; SCENE_WORDS];
                scene[0x298 / 4] = flags;
                state.run(|| weather_system_reset(&mut scene)).unwrap();
                assert_eq!(scene[0x298 / 4], (flags & 0x41) | 6);
                assert!(state.precision_raised());
                for (index, word) in scene.iter().enumerate() {
                    let offset = index * 4;
                    if offset != 0x298 && offset != 0x2a4 && !(0x2ac..=0x328).contains(&offset) {
                        assert_eq!(*word, 0xdeadbeef, "offset {offset:#x}");
                    }
                }
                assert_eq!(scene[0x2c8 / 4..0x2d4 / 4], scene[0x2d4 / 4..0x2e0 / 4]);
                assert_eq!(scene[0x2c0 / 4..0x2c8 / 4], [1000, 3000]);
                assert_eq!(scene[0x328 / 4], 769f32.sqrt().to_bits());
            }
        }
    }

    #[test]
    fn sound_defaults_overwrite_every_word_and_ignore_previous_contents() {
        let mut first = [0xdeadbeef; SECTOR_SOUND_PROPERTIES_WORDS];
        let mut second = [0; SECTOR_SOUND_PROPERTIES_WORDS];
        setup_sector_sound_properties_defaults(&mut first);
        setup_sector_sound_properties_defaults(&mut second);
        assert_eq!(first, second);
        assert_eq!(first[0], 1f32.to_bits());
        assert_eq!(first[2], 1490);
        assert_eq!(first[9], 500);
        assert_eq!(first[12..17], [250, 0, 250, 0, 0]);
        assert_eq!(first[20..], [0; 6]);
    }
}
