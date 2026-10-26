use super::*;

fn patterned(width: usize, height: usize) -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[
                ((x + 3 * y) * 17 % 256) as u8,
                ((x * x + 7 * y) % 256) as u8,
                (255 - (x * 11 + y * 5) % 256) as u8,
            ]);
        }
    }
    pixels
}

// Hardcoded oracle produced by the pinned C++ ResizeImage body, compiled as a
// separate host program with contraction disabled and FloatToInt=lrintf/RN.
// This is a source-profile comparison, not original-runtime validation.
#[test]
fn same_size_filter_is_not_identity_and_preserves_source_rounding() {
    let original = patterned(4, 4);
    let output = resize_rgb_source([4, 4], &original, [4, 4]).unwrap();
    assert_eq!(
        output.as_ref(),
        &[
            0, 0, 255, 11, 1, 248, 23, 2, 240, 34, 4, 233, 34, 5, 252, 45, 5, 244, 57, 7, 237, 68,
            9, 230, 68, 9, 248, 79, 10, 241, 91, 11, 234, 102, 13, 226, 102, 14, 245, 113, 15, 238,
            125, 16, 230, 136, 18, 223,
        ]
    );
    assert_ne!(output.as_ref(), original);
}

#[test]
fn source_interior_double_offset_and_row_crossing_have_fixed_pixels() {
    assert_eq!(
        resize_rgb_source([9, 9], &patterned(9, 9), [2, 2])
            .unwrap()
            .as_ref(),
        &[112, 18, 225, 144, 37, 198, 140, 46, 205, 108, 65, 178]
    );
}

#[test]
fn float_to_int_rounds_half_values_to_even_and_zero_area_is_magenta() {
    let mut input = vec![0; 5 * 5 * 3];
    input[..3].copy_from_slice(&[2, 6, 10]);
    assert_eq!(
        resize_rgb_source([5, 5], &input, [2, 2]).unwrap().as_ref(),
        &[0, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        resize_rgb_source([1, 1], &[12, 34, 56], [1, 1])
            .unwrap()
            .as_ref(),
        &[255, 0, 255]
    );
}

#[test]
fn invalid_extents_or_storage_never_use_an_alternate_resize() {
    assert_eq!(
        resize_rgb_source([0, 1], &[], [1, 1]),
        Err(LightmapResizeError::UnsupportedDimensions)
    );
    assert_eq!(
        resize_rgb_source([4, 4], &[0; 48], [8, 2]),
        Err(LightmapResizeError::UnsupportedDimensions)
    );
    assert_eq!(
        resize_rgb_source([4, 4], &[0; 47], [2, 2]),
        Err(LightmapResizeError::InvalidRgbLength)
    );
}
