//! Bounded 5DS v20 intake and isolated position-channel sampling.
//!
//! These are descriptive Rust names, not recovered original symbols. Layout is
//! evidenced by the PC loader; sampling corresponds to the position branch of
//! LS3DF RVA 0x1d320 under the game's single-precision x87 control word. Sampling
//! one channel does not apply a model pose, rotation, scale, notes or skinning.
use crate::{Error, Result};

const MAX_FILE: usize = 64 * 1024 * 1024;
const MAX_SAMPLE_MS: u32 = u16::MAX as u32 * 40 + 39;

#[derive(Clone, Debug)]
pub struct AnimationSet {
    /// Original eight timestamp bytes; no filesystem timestamp policy is applied.
    pub timestamp: [u8; 8],
    pub end_frame: u16,
    pub tracks: Vec<AnimationTrack>,
}
impl AnimationSet {
    pub fn duration_ms(&self) -> u32 {
        u32::from(self.end_frame) * 40
    }
}

#[derive(Clone, Debug)]
pub struct AnimationTrack {
    /// Original bytes, for exact matching against original frame names.
    pub name: Vec<u8>,
    pub flags: u16,
    pub position: Option<PositionTrack>,
}
impl AnimationTrack {
    /// Whole-track position-only playback is available only without other channels.
    pub fn position_only(&self) -> Result<&PositionTrack> {
        if self.flags != 2 {
            return Err(Error(
                "rotation, scale and note playback unavailable".into(),
            ));
        }
        self.position
            .as_ref()
            .ok_or_else(|| Error("position channel absent".into()))
    }
}

#[derive(Clone, Debug)]
pub struct PositionTrack {
    times: Vec<u16>,
    values: Vec<[f32; 3]>,
}
impl PositionTrack {
    pub fn key_times(&self) -> &[u16] {
        &self.times
    }
    pub fn key_values(&self) -> &[[f32; 3]] {
        &self.values
    }

    /// Host buffer allocation accounting only; excludes inline/owner storage.
    /// Uses actual vector capacities rather than key lengths.
    pub fn storage_bytes(&self) -> Result<usize> {
        self.times
            .capacity()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|times| {
                self.values
                    .capacity()
                    .checked_mul(std::mem::size_of::<[f32; 3]>())
                    .and_then(|values| times.checked_add(values))
            })
            .ok_or_else(|| Error("position storage accounting overflow".into()))
    }

    /// Samples only this channel before IPol boundary handling. No looping occurs
    /// here. The supported domain excludes signed times, frame-index wrapping,
    /// nonfinite values and extreme magnitudes outside the tested numeric policy.
    pub fn sample_position_channel(&self, current_ms: u32) -> Result<[f32; 3]> {
        if current_ms > MAX_SAMPLE_MS {
            return Err(Error(
                "position sample time outside supported domain".into(),
            ));
        }
        let frame = (current_ms / 40) as u16;
        if frame < self.times[0] {
            return Ok(self.values[0]);
        }
        let last = self.times.len() - 1;
        if frame >= self.times[last] {
            return Ok(self.values[last]);
        }
        let right = self.times.partition_point(|&key| key <= frame);
        let left = right - 1;
        // Keep separate f32 operations: original Game control word rounds each
        // arithmetic result to 24 significant bits. Do not replace with mul_add.
        let frame_time = current_ms as f32 * 0.025_f32;
        let numerator = frame_time - f32::from(self.times[left]);
        let fraction = numerator / f32::from(self.times[right] - self.times[left]);
        let mut out = [0.0; 3];
        for (axis, output) in out.iter_mut().enumerate() {
            let difference = self.values[right][axis] - self.values[left][axis];
            let displacement = difference * fraction;
            *output = self.values[left][axis] + displacement;
        }
        Ok(out)
    }
}

fn bytes(data: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    offset
        .checked_add(size)
        .and_then(|end| data.get(offset..end))
        .ok_or_else(|| Error("truncated 5DS data".into()))
}
fn word(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes(data, offset, 2)?.try_into().unwrap(),
    ))
}
fn dword(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes(data, offset, 4)?.try_into().unwrap(),
    ))
}
struct DecodeBudget(usize);
impl DecodeBudget {
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.0 = self
            .0
            .checked_sub(bytes)
            .ok_or_else(|| Error("5DS aggregate decoding budget exceeded".into()))?;
        Ok(())
    }
}

fn channel(
    data: &[u8],
    offset: &mut usize,
    components: usize,
    budget: &mut DecodeBudget,
) -> Result<(Vec<u16>, usize)> {
    let count = usize::from(word(data, *offset)?);
    *offset += 2;
    if count == 0 {
        return Err(Error("empty 5DS channel unsupported".into()));
    }
    budget.charge(count * (2 + components * 4))?;
    let times = bytes(data, *offset, count * 2)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| u16::from_le_bytes(*v))
        .collect::<Vec<_>>();
    if times.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(Error("5DS keys must be strictly increasing".into()));
    }
    // The count word plus time array is padded to a four-byte boundary.
    *offset += count * 2 + if count.is_multiple_of(2) { 2 } else { 0 };
    let values = *offset;
    bytes(data, values, count * components * 4)?;
    *offset += count * components * 4;
    Ok((times, values))
}

/// Reads relative offsets without mutating or relocating input bytes. Rotation
/// and scale sections are bounded/skipped, not interpreted. Note channels and
/// unknown flags are rejected until their file layout and execution are covered.
pub fn parse_5ds(data: &[u8]) -> Result<AnimationSet> {
    parse_tracks(data, None)
}

/// One explicitly selected track. Other track bodies are not interpreted or
/// validated, so this is not evidence that the whole file supports playback.
#[derive(Clone, Debug)]
pub struct SelectedAnimationTrack {
    pub timestamp: [u8; 8],
    pub end_frame: u16,
    pub track: AnimationTrack,
}

/// Reads a selected record without silently dropping unavailable channels from
/// a complete animation set. A selected mixed rotation/position track still
/// fails `position_only`; its isolated position channel may be inspected.
pub fn read_5ds_track(data: &[u8], index: usize) -> Result<SelectedAnimationTrack> {
    let mut set = parse_tracks(data, Some(index))?;
    Ok(SelectedAnimationTrack {
        timestamp: set.timestamp,
        end_frame: set.end_frame,
        track: set.tracks.remove(0),
    })
}

fn parse_tracks(data: &[u8], selected: Option<usize>) -> Result<AnimationSet> {
    if data.len() > MAX_FILE || bytes(data, 0, 4)? != b"5DS\0" {
        return Err(Error("invalid 5DS header".into()));
    }
    if word(data, 4)? != 20 {
        return Err(Error("unsupported 5DS version".into()));
    }
    let timestamp = bytes(data, 6, 8)?.try_into().unwrap();
    let size = dword(data, 14)? as usize;
    if size != data.len().saturating_sub(18) {
        return Err(Error("5DS payload size mismatch".into()));
    }
    let payload = bytes(data, 18, size)?;
    let count = usize::from(word(payload, 0)?);
    let end_frame = word(payload, 2)?;
    let table_end = 4 + count * 8;
    bytes(payload, 0, table_end)?;
    if selected.is_some_and(|index| index >= count) {
        return Err(Error("5DS track index out of range".into()));
    }
    // Bound total allocations AND repeated scanning for aliased offsets. A small
    // file cannot multiply a shared name/channel into unbounded decoded output.
    let mut budget = DecodeBudget(data.len().saturating_mul(4).min(MAX_FILE));
    let decoded_count = if selected.is_some() { 1 } else { count };
    budget.charge(decoded_count * std::mem::size_of::<AnimationTrack>())?;
    let mut tracks = Vec::with_capacity(decoded_count);
    for index in 0..count {
        if selected.is_some_and(|wanted| wanted != index) {
            continue;
        }
        let name_offset = dword(payload, 4 + index * 8)? as usize;
        let track_offset = dword(payload, 8 + index * 8)? as usize;
        if name_offset < table_end || track_offset < table_end {
            return Err(Error("5DS offset overlaps record table".into()));
        }
        let remaining = payload
            .get(name_offset..)
            .ok_or_else(|| Error("invalid 5DS name offset".into()))?;
        let scan = &remaining[..remaining.len().min(budget.0)];
        let name_len = scan
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| Error("unterminated 5DS name".into()))?;
        budget.charge(name_len + 1)?;
        let name = remaining[..name_len].to_vec();
        let flags = word(payload, track_offset)?;
        if flags & !0x0e != 0 {
            return Err(Error("5DS notes or unknown channels unavailable".into()));
        }
        bytes(payload, track_offset, 4)?; // flags followed by retained reserved word
        let mut cursor = track_offset + 4;
        if flags & 4 != 0 {
            channel(payload, &mut cursor, 4, &mut budget)?;
        }
        let position = if flags & 2 != 0 {
            let (times, values_offset) = channel(payload, &mut cursor, 3, &mut budget)?;
            let mut values = Vec::with_capacity(times.len());
            for key in 0..times.len() {
                let mut value = [0.0_f32; 3];
                for (axis, component) in value.iter_mut().enumerate() {
                    *component =
                        f32::from_bits(dword(payload, values_offset + key * 12 + axis * 4)?);
                    let magnitude = component.abs();
                    if !component.is_finite()
                        || magnitude > 1_000_000.0
                        || (magnitude != 0.0 && magnitude < f32::from_bits(897988541))
                    {
                        return Err(Error(
                            "5DS position outside supported numeric domain".into(),
                        ));
                    }
                }
                values.push(value);
            }
            Some(PositionTrack { times, values })
        } else {
            None
        };
        if flags & 8 != 0 {
            channel(payload, &mut cursor, 3, &mut budget)?;
        }
        tracks.push(AnimationTrack {
            name,
            flags,
            position,
        });
    }
    Ok(AnimationSet {
        timestamp,
        end_frame,
        tracks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(flags: u16, times: &[u16], values: &[[f32; 3]]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend(1_u16.to_le_bytes());
        payload.extend(10_u16.to_le_bytes());
        payload.extend(12_u32.to_le_bytes());
        payload.extend(17_u32.to_le_bytes());
        payload.extend(b"Bone\0");
        payload.extend(flags.to_le_bytes());
        payload.extend(0_u16.to_le_bytes());
        if flags & 4 != 0 {
            payload.extend(1_u16.to_le_bytes());
            payload.extend(0_u16.to_le_bytes());
            for v in [0_f32, 0., 0., 1.] {
                payload.extend(v.to_le_bytes());
            }
        }
        if flags & 2 != 0 {
            payload.extend((times.len() as u16).to_le_bytes());
            for time in times {
                payload.extend(time.to_le_bytes());
            }
            if times.len().is_multiple_of(2) {
                payload.extend(0_u16.to_le_bytes());
            }
            for value in values {
                for axis in value {
                    payload.extend(axis.to_le_bytes());
                }
            }
        }
        let mut data = b"5DS\0".to_vec();
        data.extend(20_u16.to_le_bytes());
        data.extend([0; 8]);
        data.extend((payload.len() as u32).to_le_bytes());
        data.extend(payload);
        data
    }

    #[test]
    fn two_key_position_and_endpoint_policy() {
        let data = fixture(2, &[0, 10], &[[0.; 3], [10., 20., 30.]]);
        let set = parse_5ds(&data).unwrap();
        assert_eq!(set.duration_ms(), 400);
        assert_eq!(set.tracks[0].name, b"Bone");
        let track = set.tracks[0].position_only().unwrap();
        assert_eq!(track.sample_position_channel(0).unwrap(), [0.; 3]);
        assert_eq!(track.sample_position_channel(200).unwrap(), [5., 10., 15.]);
        assert_eq!(track.sample_position_channel(400).unwrap(), [10., 20., 30.]);
        assert_eq!(track.sample_position_channel(999).unwrap(), [10., 20., 30.]);
    }

    #[test]
    fn odd_count_and_nonzero_first_key() {
        let data = fixture(2, &[5, 10, 20], &[[1.; 3], [3.; 3], [7.; 3]]);
        let track = parse_5ds(&data).unwrap().tracks.remove(0).position.unwrap();
        assert_eq!(track.key_times(), &[5, 10, 20]);
        assert_eq!(track.sample_position_channel(0).unwrap(), [1.; 3]);
        assert_eq!(track.sample_position_channel(600).unwrap(), [5.; 3]);
    }

    #[test]
    fn isolated_channel_does_not_enable_rotation_playback() {
        let set = parse_5ds(&fixture(6, &[0], &[[1., 2., 3.]])).unwrap();
        assert!(set.tracks[0].position_only().is_err());
        assert_eq!(
            set.tracks[0]
                .position
                .as_ref()
                .unwrap()
                .sample_position_channel(0)
                .unwrap(),
            [1., 2., 3.]
        );
        assert!(parse_5ds(&fixture(0x10, &[], &[])).is_err());
        assert!(parse_5ds(&fixture(1, &[], &[])).is_err());
    }

    #[test]
    fn aliased_names_and_channels_have_an_aggregate_budget() {
        let mut data = fixture(2, &[0], &[[1., 2., 3.]]);
        let original = data[18..].to_vec();
        let count = 128_usize;
        let name_offset = 4 + count * 8;
        let name_len = 65_536_usize;
        let track_offset = name_offset + name_len + 1;
        let mut payload = Vec::new();
        payload.extend((count as u16).to_le_bytes());
        payload.extend(10_u16.to_le_bytes());
        for _ in 0..count {
            payload.extend((name_offset as u32).to_le_bytes());
            payload.extend((track_offset as u32).to_le_bytes());
        }
        payload.resize(track_offset - 1, b'A');
        payload.push(0);
        payload.extend(&original[17..]);
        data.truncate(14);
        data.extend((payload.len() as u32).to_le_bytes());
        data.extend(payload);
        assert!(parse_5ds(&data).is_err());
        // One explicitly selected record does not amplify the aliased names.
        assert_eq!(read_5ds_track(&data, 0).unwrap().track.name.len(), name_len);

        // Also bound work and allocations when names are tiny but channel arrays
        // are repeatedly decoded through the same offset.
        let mut channel_aliases = data.clone();
        channel_aliases[18 + name_offset] = 0;
        channel_aliases.truncate(18 + track_offset);
        channel_aliases.extend(2_u32.to_le_bytes());
        channel_aliases.extend(u16::MAX.to_le_bytes());
        for time in 0..u16::MAX {
            channel_aliases.extend(time.to_le_bytes());
        }
        for _ in 0..u16::MAX {
            for value in [1_f32; 3] {
                channel_aliases.extend(value.to_le_bytes());
            }
        }
        let size = channel_aliases.len() - 18;
        channel_aliases[14..18].copy_from_slice(&(size as u32).to_le_bytes());
        assert!(parse_5ds(&channel_aliases).is_err());
        assert_eq!(
            read_5ds_track(&channel_aliases, 0)
                .unwrap()
                .track
                .position
                .unwrap()
                .key_times()
                .len(),
            usize::from(u16::MAX)
        );
    }

    #[test]
    fn storage_accounting_uses_actual_capacity() {
        let mut times = Vec::with_capacity(11);
        times.push(0);
        let mut values = Vec::with_capacity(17);
        values.push([0.; 3]);
        let expected = times.capacity() * std::mem::size_of::<u16>()
            + values.capacity() * std::mem::size_of::<[f32; 3]>();
        let track = PositionTrack { times, values };
        assert_eq!(track.storage_bytes().unwrap(), expected);
        assert!(track.storage_bytes().unwrap() > 14);
    }

    #[test]
    fn selected_track_is_explicitly_bounded() {
        let data = fixture(6, &[0], &[[1., 2., 3.]]);
        let selected = read_5ds_track(&data, 0).unwrap();
        assert_eq!(selected.end_frame, 10);
        assert_eq!(selected.track.name, b"Bone");
        assert!(selected.track.position_only().is_err());
        assert!(read_5ds_track(&data, 1).is_err());
    }

    #[test]
    fn malformed_inputs_and_out_of_domain_samples_fail() {
        let good = fixture(2, &[0, 10], &[[0.; 3], [10.; 3]]);
        for len in 0..good.len() {
            assert!(parse_5ds(&good[..len]).is_err());
        }
        let mut bad = good.clone();
        bad[22..26].copy_from_slice(&0_u32.to_le_bytes());
        assert!(parse_5ds(&bad).is_err());
        assert!(parse_5ds(&fixture(2, &[0, 0], &[[0.; 3]; 2])).is_err());
        assert!(parse_5ds(&fixture(2, &[], &[])).is_err());
        assert!(parse_5ds(&fixture(2, &[0], &[[f32::NAN; 3]])).is_err());
        let track = parse_5ds(&good).unwrap().tracks.remove(0).position.unwrap();
        assert!(track.sample_position_channel(MAX_SAMPLE_MS + 1).is_err());
    }

    #[test]
    fn original_helper_synthetic_float32_vectors() {
        let data = fixture(
            2,
            &[390, 414],
            &[
                [
                    f32::from_bits(3283738510),
                    f32::from_bits(1133452070),
                    f32::from_bits(3255904646),
                ],
                [
                    f32::from_bits(3275531816),
                    f32::from_bits(1145845040),
                    f32::from_bits(3295735468),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(15600)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [3283738510, 1133452070, 3255904646]
        );
        let data = fixture(
            2,
            &[390, 414],
            &[
                [
                    f32::from_bits(3283738510),
                    f32::from_bits(1133452070),
                    f32::from_bits(3255904646),
                ],
                [
                    f32::from_bits(3275531816),
                    f32::from_bits(1145845040),
                    f32::from_bits(3295735468),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(15601)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [3283732262, 1133470177, 3256157837]
        );
        let data = fixture(
            2,
            &[390, 414],
            &[
                [
                    f32::from_bits(3283738510),
                    f32::from_bits(1133452070),
                    f32::from_bits(3255904646),
                ],
                [
                    f32::from_bits(3275531816),
                    f32::from_bits(1145845040),
                    f32::from_bits(3295735468),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(16559)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [3275544312, 1145835986, 3295719644]
        );
        let data = fixture(
            2,
            &[390, 414],
            &[
                [
                    f32::from_bits(3283738510),
                    f32::from_bits(1133452070),
                    f32::from_bits(3255904646),
                ],
                [
                    f32::from_bits(3275531816),
                    f32::from_bits(1145845040),
                    f32::from_bits(3295735468),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(16560)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [3275531816, 1145845040, 3295735468]
        );
        let data = fixture(
            2,
            &[600, 639, 697],
            &[
                [
                    f32::from_bits(1132543099),
                    f32::from_bits(3292391793),
                    f32::from_bits(3275153644),
                ],
                [
                    f32::from_bits(3292754965),
                    f32::from_bits(1140648161),
                    f32::from_bits(1135441329),
                ],
                [
                    f32::from_bits(1142194499),
                    f32::from_bits(3291717127),
                    f32::from_bits(3292448000),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(23999)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [1132543099, 3292391793, 3275153644]
        );
        let data = fixture(
            2,
            &[600, 639, 697],
            &[
                [
                    f32::from_bits(1132543099),
                    f32::from_bits(3292391793),
                    f32::from_bits(3275153644),
                ],
                [
                    f32::from_bits(3292754965),
                    f32::from_bits(1140648161),
                    f32::from_bits(1135441329),
                ],
                [
                    f32::from_bits(1142194499),
                    f32::from_bits(3291717127),
                    f32::from_bits(3292448000),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(24000)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [1132543099, 3292391793, 3275153644]
        );
        let data = fixture(
            2,
            &[600, 639, 697],
            &[
                [
                    f32::from_bits(1132543099),
                    f32::from_bits(3292391793),
                    f32::from_bits(3275153644),
                ],
                [
                    f32::from_bits(3292754965),
                    f32::from_bits(1140648161),
                    f32::from_bits(1135441329),
                ],
                [
                    f32::from_bits(1142194499),
                    f32::from_bits(3291717127),
                    f32::from_bits(3292448000),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(24001)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [1132521226, 3292378489, 3275131365]
        );
        let data = fixture(
            2,
            &[600, 639, 697],
            &[
                [
                    f32::from_bits(1132543099),
                    f32::from_bits(3292391793),
                    f32::from_bits(3275153644),
                ],
                [
                    f32::from_bits(3292754965),
                    f32::from_bits(1140648161),
                    f32::from_bits(1135441329),
                ],
                [
                    f32::from_bits(1142194499),
                    f32::from_bits(3291717127),
                    f32::from_bits(3292448000),
                ],
            ],
        );
        let set = parse_5ds(&data).unwrap();
        let actual = set.tracks[0]
            .position_only()
            .unwrap()
            .sample_position_channel(27879)
            .unwrap();
        assert_eq!(
            actual.map(f32::to_bits),
            [1142184797, 3291708494, 3292440172]
        );
    }
}
