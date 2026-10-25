//! Bounded WavDecompression (PC RVA 0x4250): PCM16 mono/stereo and raw/LZ fallback.
//! Name and table identity are corroborated by PS2 linker symbols.
use super::wav_table::WAV_TABLE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WavError {
    Bounds,
    Unsupported,
}

/// Native predictor arithmetic wraps in 16 bits. Header matching compares only
/// three saved identifiers; it imposes no RIFF, PCM tag or sample-rate validation.
/// Truncated native scratch reads and zero-channel division are outside this domain.
pub(super) fn wav_decompression(
    input: &[u8],
    output: &mut [u8],
    header: &[u8; 44],
) -> Result<usize, WavError> {
    let limit = output.len().min(32768);
    let output = &mut output[..limit];
    let kind = *input.first().ok_or(WavError::Bounds)?;
    let row = match kind {
        12 => 1,
        16 => 2,
        20 => 3,
        24 => 4,
        28 => 5,
        32 => 6,
        _ => 0,
    };
    let mut has_header = true;
    for offset in [0, 12, 36] {
        let candidate = input.get(offset + 1..offset + 5).ok_or(WavError::Bounds)?;
        if candidate != &header[offset..offset + 4] {
            has_header = false;
            break;
        }
    }
    let mut source = 1;
    let mut written = 0;
    if has_header {
        let bytes = input.get(1..45).ok_or(WavError::Bounds)?;
        output
            .get_mut(..44)
            .ok_or(WavError::Bounds)?
            .copy_from_slice(bytes);
        source = 45;
        written = 44;
    }
    if source == input.len() {
        return Ok(written);
    }
    let channels = u16::from_le_bytes([header[22], header[23]]) as usize;
    if channels == 0 {
        return Err(WavError::Unsupported);
    }
    let align = u16::from_le_bytes([header[32], header[33]]) as usize;
    if align / channels != 2 {
        let count = super::decode_compression(&input[source..], &mut output[written..])
            .map_err(|_| WavError::Bounds)?;
        return Ok(written + count);
    }
    if !matches!(channels, 1 | 2) {
        return Err(WavError::Unsupported);
    }
    let mut samples = [0u16; 2];
    for sample in &mut samples[..channels] {
        let bytes = input.get(source..source + 2).ok_or(WavError::Bounds)?;
        output
            .get_mut(written..written + 2)
            .ok_or(WavError::Bounds)?
            .copy_from_slice(bytes);
        *sample = u16::from_le_bytes(bytes.try_into().unwrap());
        source += 2;
        written += 2;
    }
    while source < input.len() {
        for sample in &mut samples[..channels] {
            let code = *input.get(source).ok_or(WavError::Bounds)?;
            let delta = WAV_TABLE[row][(code & 127) as usize] as u16;
            *sample = if code & 128 == 0 {
                sample.wrapping_add(delta)
            } else {
                sample.wrapping_sub(delta)
            };
            output
                .get_mut(written..written + 2)
                .ok_or(WavError::Bounds)?
                .copy_from_slice(&sample.to_le_bytes());
            source += 1;
            written += 2;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn header(channels: u16) -> [u8; 44] {
        let mut h = [0; 44];
        h[..4].copy_from_slice(b"RIFF");
        h[12..16].copy_from_slice(b"fmt ");
        h[36..40].copy_from_slice(b"data");
        h[22..24].copy_from_slice(&channels.to_le_bytes());
        h[32..34].copy_from_slice(&(channels * 2).to_le_bytes());
        h
    }
    #[test]
    fn mono_header_wrap_and_identifier_only_match() {
        let h = header(1);
        let mut b = vec![8];
        let mut candidate = h;
        candidate[20] = 99; // format tag is copied metadata, not validated.
        b.extend_from_slice(&candidate);
        b.extend_from_slice(&32767i16.to_le_bytes());
        b.extend_from_slice(&[1, 129]);
        let mut out = [0; 50];
        assert_eq!(wav_decompression(&b, &mut out, &h), Ok(50));
        assert_eq!(&out[..44], &candidate);
        assert_eq!(&out[44..], &[255, 127, 0, 128, 255, 127]);
    }
    #[test]
    fn header_only_skips_sample_format_and_probe_short_circuits() {
        let mut h = header(0);
        h[32] = 0;
        let mut input = vec![8];
        input.extend_from_slice(&h);
        let mut output = [0; 44];
        assert_eq!(wav_decompression(&input, &mut output, &h), Ok(44));
        assert_eq!(output, h);
    }
    #[test]
    fn stereo_predictors_and_default_selector() {
        let h = header(2);
        let b = [99, 100, 0, 156, 255, 1, 129];
        let mut out = [0; 8];
        assert_eq!(wav_decompression(&b, &mut out, &h), Ok(8));
        assert_eq!(out, [100, 0, 156, 255, 101, 0, 155, 255]);
    }
    #[test]
    fn bounds_preserve_mutated_native_prefix_and_reject_unsupported_domain() {
        let h = header(2);
        let mut out = [0xa5; 8];
        assert_eq!(
            wav_decompression(&[8, 1, 0, 2, 0, 1], &mut out, &h),
            Err(WavError::Bounds)
        );
        assert_eq!(&out[..6], &[1, 0, 2, 0, 2, 0]);
        let h = header(0);
        assert_eq!(
            wav_decompression(&[8; 45], &mut out, &h),
            Err(WavError::Unsupported)
        );
        assert_eq!(
            wav_decompression(&[8, b'R', b'I', b'F', b'F'], &mut out, &header(1)),
            Err(WavError::Bounds)
        );
    }
    #[test]
    fn non_pcm16_uses_general_raw_and_nonzero_lz_after_optional_header() {
        for (channels, align) in [(1u16, 1u16), (2, 6), (3, 3)] {
            let mut h = header(channels);
            h[32..34].copy_from_slice(&align.to_le_bytes());
            for (encoded, expected) in [
                (vec![0, 7, 8, 9, 10], vec![7, 8, 9, 10]),
                (
                    vec![1, 0x10, 0, b'a', b'b', b'c', 0, 0x30],
                    b"abcabc".to_vec(),
                ),
                (vec![99, 0, 0, 7, 8, 9], vec![7, 8, 9]),
            ] {
                let mut input = vec![8];
                input.extend(h);
                input.extend(&encoded);
                let mut output = vec![0xa5; 44 + expected.len()];
                assert_eq!(wav_decompression(&input, &mut output, &h), Ok(output.len()));
                assert_eq!(&output[..44], &h);
                assert_eq!(&output[44..], &expected);
                // Subsequent archive blocks need no duplicate WAV header.
                let mut continuation = vec![32];
                continuation.extend(&encoded);
                let mut output = vec![0xa5; expected.len()];
                assert_eq!(
                    wav_decompression(&continuation, &mut output, &h),
                    Ok(expected.len())
                );
                assert_eq!(output, expected);
            }
        }
    }
    #[test]
    fn fallback_bounds_keep_header_and_completed_lz_output_prefix() {
        let mut h = header(1);
        h[32] = 1;
        let mut input = vec![8];
        input.extend(h);
        // First literal succeeds, then a backreference addresses before output.
        input.extend([1, 0x40, 0, b'x', 0x10, 0]);
        let mut output = [0xa5; 50];
        assert_eq!(
            wav_decompression(&input, &mut output, &h),
            Err(WavError::Bounds)
        );
        assert_eq!(&output[..44], &h);
        assert_eq!(output[44], b'x');
        assert_eq!(&output[45..], &[0xa5; 5]);
        let mut input = vec![8];
        input.extend(h);
        input.extend([0, 7, 8, 9, 10]);
        let mut short = [0xa5; 46];
        assert_eq!(
            wav_decompression(&input, &mut short, &h),
            Err(WavError::Bounds)
        );
        assert_eq!(&short[..44], &h);
        assert_eq!(&short[44..], &[0xa5; 2]);
        let mut huge = vec![8];
        huge.extend(h);
        huge.push(0);
        huge.extend(std::iter::repeat_n(1, 32725));
        assert_eq!(
            wav_decompression(&huge, &mut [0; 32769], &h),
            Err(WavError::Bounds)
        );
    }
}
