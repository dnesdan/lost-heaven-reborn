//! Owned sound resources and a PCM mixer for the native Rust runtime.
//!
//! The call slots follow the PC I3D_sound vtable. The host supplies canonical
//! object addresses and resolves pointer arguments; no original DLL is loaded.
//! Mixing and device playback are host implementations, not a claim of exact
//! DirectSound, sector fade, cone, reverb, or voice prioritization equivalence.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use lhr_assets::Installation;

#[path = "runtime_audio_device.rs"]
mod device;
pub use device::{DeviceAudioOutput, DeviceAudioStatus};
#[path = "runtime_vehicle_audio.rs"]
pub mod vehicle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioError(pub String);

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AudioError {}

type Result<T> = std::result::Result<T, AudioError>;

/// Decoded source data. Original compressed archive audio is decoded by
/// Installation/RWData before the RIFF parser sees it.
#[derive(Debug)]
pub struct PcmSource {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    samples: Vec<f32>,
}

impl PcmSource {
    pub fn from_wav(name: &str, bytes: &[u8]) -> Result<Self> {
        let invalid = || AudioError("invalid or unsupported PCM WAV".into());
        if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
            return Err(invalid());
        }
        let word = |offset: usize| -> Result<u32> {
            Ok(u32::from_le_bytes(
                bytes
                    .get(offset..offset + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            ))
        };
        let end = (word(4)? as usize).checked_add(8).ok_or_else(invalid)?;
        if end > bytes.len() || end < 12 {
            return Err(invalid());
        }
        let mut format = None;
        let mut payload = None;
        let mut offset = 12usize;
        while offset < end {
            let header = bytes.get(offset..offset + 8).ok_or_else(invalid)?;
            let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
            let start = offset.checked_add(8).ok_or_else(invalid)?;
            let stop = start.checked_add(length).ok_or_else(invalid)?;
            let data = bytes
                .get(start..stop)
                .filter(|_| stop <= end)
                .ok_or_else(invalid)?;
            match &header[..4] {
                b"fmt " if format.is_none() && length >= 16 => {
                    let u16_at = |i| u16::from_le_bytes(data[i..i + 2].try_into().unwrap());
                    let u32_at = |i| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
                    format = Some((
                        u16_at(0),
                        u16_at(2),
                        u32_at(4),
                        u32_at(8),
                        u16_at(12),
                        u16_at(14),
                    ));
                }
                b"data" if payload.is_none() => payload = Some(data),
                _ => {}
            }
            offset = stop.checked_add(length & 1).ok_or_else(invalid)?;
        }
        if offset != end {
            return Err(invalid());
        }
        let (encoding, channels, sample_rate, byte_rate, align, bits) =
            format.ok_or_else(invalid)?;
        let data = payload.ok_or_else(invalid)?;
        if encoding != 1
            || !(channels == 1 || channels == 2)
            || !(bits == 8 || bits == 16)
            || !(1..=384_000).contains(&sample_rate)
            || align != channels * (bits / 8)
            || byte_rate != sample_rate * u32::from(align)
            || data.is_empty()
            || data.len() % usize::from(align) != 0
        {
            return Err(invalid());
        }
        let samples = if bits == 16 {
            data.as_chunks::<2>()
                .0
                .iter()
                .map(|s| f32::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0)
                .collect()
        } else {
            data.iter()
                .map(|s| (f32::from(*s) - 128.0) / 128.0)
                .collect()
        };
        Ok(Self {
            name: name.into(),
            sample_rate,
            channels,
            samples,
        })
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    pub fn duration_ms(&self) -> u32 {
        ((self.frames() as u64 * 1000) / u64::from(self.sample_rate)) as u32
    }
}

#[derive(Clone, Debug)]
pub struct SoundState {
    pub handle: u32,
    pub source: Option<Arc<PcmSource>>,
    pub parent: u32,
    pub on: bool,
    pub looping: bool,
    pub volume: f32,
    pub out_volume: f32,
    /// Multiplicative playback rate, matching I3D_sound::SetFrequency.
    pub frequency: f32,
    pub sound_type: u32,
    pub range: [f32; 4],
    pub cone: [f32; 2],
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub direction: [f32; 3],
    pub sectors: BTreeSet<u32>,
    /// Derived native fields; the common frame portion stays in ModelArena.
    pub native_tail: [u32; 43],
    frame_position: f64,
}

impl SoundState {
    fn new(handle: u32) -> Self {
        let mut tail = [0; 43];
        for (offset, value) in [
            (0x140, 0x208f),
            (0x144, 1),
            (0x168, 1f32.to_bits()),
            (0x16c, 1f32.to_bits()),
            (0x170, 10f32.to_bits()),
            (0x174, 10f32.to_bits()),
            (0x178, 100f32.to_bits()),
            (0x17c, 1f32.to_bits()),
            (0x18c, 1f32.to_bits()),
            (0x190, 1f32.to_bits()),
            (0x194, 1f32.to_bits()),
            (0x1a0, std::f32::consts::FRAC_PI_3.to_bits()),
            (0x1a4, (std::f32::consts::FRAC_PI_3 * 0.5).cos().to_bits()),
            (0x1a8, std::f32::consts::FRAC_PI_2.to_bits()),
            (0x1ac, (std::f32::consts::FRAC_PI_2 * 0.5).cos().to_bits()),
            (0x1b4, (-1f32).to_bits()),
            (0x1d8, 32),
            (0x1e8, 32),
        ] {
            tail[(offset - 0x140) / 4] = value;
        }
        Self {
            handle,
            source: None,
            parent: 0,
            on: false,
            looping: false,
            volume: 1.0,
            out_volume: 0.0,
            frequency: 1.0,
            sound_type: 0,
            range: [1.0, 10.0, 1.0, 0.0],
            cone: [std::f32::consts::FRAC_PI_3, std::f32::consts::FRAC_PI_2],
            position: [0.0; 3],
            velocity: [0.0; 3],
            direction: [0.0, 0.0, 1.0],
            sectors: BTreeSet::new(),
            native_tail: tail,
            frame_position: 0.0,
        }
    }

    pub fn current_time_ms(&self) -> u32 {
        self.source.as_ref().map_or(0, |s| {
            (self.frame_position * 1000.0 / f64::from(s.sample_rate)) as u32
        })
    }

    pub fn is_playing(&self) -> bool {
        self.on
            && self.sound_type != 0
            && self
                .source
                .as_ref()
                .is_some_and(|s| self.looping || self.frame_position < s.frames() as f64)
    }
}

#[derive(Debug)]
struct Mixer {
    sounds: BTreeMap<u32, SoundState>,
    sources: BTreeMap<String, Weak<PcmSource>>,
    listener_position: [f32; 3],
    listener_right: [f32; 3],
    listener_sector: Option<u32>,
    mixed_frames: u64,
    source_projections: BTreeMap<String, SourceProjection>,
    load_failures: Vec<AudioLoadFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioLoadFailure {
    pub sound: u32,
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceProjection {
    pub source: u32,
    /// Native C_str allocation: reference/length words precede name bytes.
    pub name: u32,
}

/// Clones share the same resource owner, so a device callback and gameplay
/// effects see the same source lifetime, playback cursor and parameter writes.
#[derive(Clone, Debug)]
pub struct RuntimeAudio(Arc<Mutex<Mixer>>);

impl Default for RuntimeAudio {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Mixer {
            sounds: BTreeMap::new(),
            sources: BTreeMap::new(),
            listener_position: [0.0; 3],
            listener_right: [1.0, 0.0, 0.0],
            listener_sector: None,
            mixed_frames: 0,
            source_projections: BTreeMap::new(),
            load_failures: Vec::new(),
        })))
    }
}

impl RuntimeAudio {
    pub fn record_load_failure(&self, sound: u32, name: &str, reason: &AudioError) -> Result<()> {
        self.lock()?.load_failures.push(AudioLoadFailure {
            sound,
            name: name.into(),
            reason: reason.to_string(),
        });
        Ok(())
    }
    pub fn load_failures(&self) -> Result<Vec<AudioLoadFailure>> {
        Ok(self.lock()?.load_failures.clone())
    }
    pub fn source_projection(&self, name: &str) -> Result<Option<SourceProjection>> {
        Ok(self
            .lock()?
            .source_projections
            .get(&canonical_name(name))
            .copied())
    }
    pub fn bind_source_projection(&self, name: &str, projection: SourceProjection) -> Result<()> {
        self.lock()?
            .source_projections
            .insert(canonical_name(name), projection);
        Ok(())
    }
    pub fn take_unused_source_projections(&self) -> Result<Vec<SourceProjection>> {
        let mut mixer = self.lock()?;
        let unused: Vec<_> = mixer
            .source_projections
            .keys()
            .filter(|key| {
                !mixer.sounds.values().any(|s| {
                    s.source
                        .as_ref()
                        .is_some_and(|source| canonical_name(&source.name) == **key)
                })
            })
            .cloned()
            .collect();
        Ok(unused
            .into_iter()
            .filter_map(|key| mixer.source_projections.remove(&key))
            .collect())
    }
    pub fn read_sound_word(&self, handle: u32, offset: u32) -> Result<u32> {
        let sound = self.sound(handle)?;
        if !(0x140..=0x1e8).contains(&offset) || !offset.is_multiple_of(4) {
            return Err(AudioError("invalid sound tail field".into()));
        }
        if offset == 0x160 {
            return sound.source.as_ref().map_or(Ok(0), |s| {
                Ok(self.source_projection(&s.name)?.map_or(0, |p| p.source))
            });
        }
        Ok(match offset {
            0x144 => {
                if sound.is_playing() {
                    2
                } else {
                    1
                }
            }
            0x164 => u32::from(sound.looping),
            0x168 => sound.range[0].to_bits(),
            0x170 => sound.range[1].to_bits(),
            0x17c => sound.range[2].to_bits(),
            0x184 => sound.range[3].to_bits(),
            0x188 => sound.out_volume.to_bits(),
            0x18c => sound.volume.to_bits(),
            0x194 => sound.frequency.to_bits(),
            0x19c => sound.sound_type,
            0x1a0 => sound.cone[0].to_bits(),
            0x1a8 => sound.cone[1].to_bits(),
            0x1c0..=0x1c8 => sound.velocity[((offset - 0x1c0) / 4) as usize].to_bits(),
            _ => sound.native_tail[((offset - 0x140) / 4) as usize],
        })
    }
    pub fn write_sound_word(&self, handle: u32, offset: u32, value: u32) -> Result<()> {
        if !(0x140..=0x1e8).contains(&offset) || !offset.is_multiple_of(4) {
            return Err(AudioError("invalid sound tail field".into()));
        }
        let mut mixer = self.lock()?;
        let sound = sound_mut(&mut mixer, handle)?;
        sound.native_tail[((offset - 0x140) / 4) as usize] = value;
        match offset {
            0x164 => sound.looping = value & 0xff != 0,
            0x168 => sound.range[0] = f32::from_bits(value),
            0x170 => sound.range[1] = f32::from_bits(value),
            0x17c => sound.range[2] = f32::from_bits(value),
            0x184 => sound.range[3] = f32::from_bits(value),
            0x188 => sound.out_volume = f32::from_bits(value),
            0x18c => sound.volume = f32::from_bits(value),
            0x194 => sound.frequency = f32::from_bits(value),
            0x19c => sound.sound_type = value,
            0x1a0 => sound.cone[0] = f32::from_bits(value),
            0x1a8 => sound.cone[1] = f32::from_bits(value),
            0x1c0..=0x1c8 => {
                sound.velocity[((offset - 0x1c0) / 4) as usize] = f32::from_bits(value)
            }
            _ => {}
        }
        Ok(())
    }
    fn lock(&self) -> Result<MutexGuard<'_, Mixer>> {
        self.0
            .lock()
            .map_err(|_| AudioError("audio owner mutex poisoned".into()))
    }

    /// The canonical address is allocated by the frame host, never fabricated
    /// from a null factory result or a private numeric sound bank.
    pub fn create_sound_at(&self, handle: u32) -> Result<()> {
        let mut mixer = self.lock()?;
        if handle == 0 || mixer.sounds.contains_key(&handle) {
            return Err(AudioError("invalid or already owned sound address".into()));
        }
        mixer.sounds.insert(handle, SoundState::new(handle));
        Ok(())
    }

    pub fn owns(&self, handle: u32) -> bool {
        self.0.lock().is_ok_and(|m| m.sounds.contains_key(&handle))
    }

    pub fn sound(&self, handle: u32) -> Result<SoundState> {
        self.lock()?
            .sounds
            .get(&handle)
            .cloned()
            .ok_or_else(|| AudioError(format!("unowned sound {handle:#x}")))
    }

    pub fn live_sound_count(&self) -> Result<usize> {
        Ok(self.lock()?.sounds.len())
    }

    pub fn sound_handles(&self) -> Result<Vec<u32>> {
        Ok(self.lock()?.sounds.keys().copied().collect())
    }

    pub fn live_source_count(&self) -> Result<usize> {
        Ok(self
            .lock()?
            .sources
            .values()
            .filter(|s| s.strong_count() != 0)
            .count())
    }

    pub fn load_sound(&self, handle: u32, name: &str, installation: &Installation) -> Result<()> {
        self.sound(handle)?;
        let canonical = canonical_name(name);
        let cached = self.lock()?.sources.get(&canonical).and_then(Weak::upgrade);
        let source = match cached {
            Some(source) => source,
            None => {
                let bytes = installation
                    .read_original_path(name)
                    .map_err(|e| AudioError(format!("sound source {name}: {e:?}")))?;
                Arc::new(PcmSource::from_wav(name, &bytes)?)
            }
        };
        let mut mixer = self.lock()?;
        mixer.sources.insert(canonical, Arc::downgrade(&source));
        let sound = mixer
            .sounds
            .get_mut(&handle)
            .ok_or_else(|| AudioError("sound released during load".into()))?;
        if sound
            .source
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &source))
        {
            return Ok(());
        }
        if source.channels > 1 && sound.sound_type != 0 {
            sound.sound_type = 3;
        }
        sound.source = Some(source);
        sound.frame_position = 0.0;
        Ok(())
    }

    pub fn set_listener(
        &self,
        position: [f32; 3],
        right: [f32; 3],
        sector: Option<u32>,
    ) -> Result<()> {
        finite(&position)?;
        finite(&right)?;
        let mut m = self.lock()?;
        m.listener_position = position;
        m.listener_right = right;
        m.listener_sector = sector;
        Ok(())
    }

    /// Called after canonical sound frame writes or a parent's world update.
    pub fn set_position(&self, handle: u32, position: [f32; 3], direction: [f32; 3]) -> Result<()> {
        finite(&position)?;
        finite(&direction)?;
        let mut mixer = self.lock()?;
        let sound = sound_mut(&mut mixer, handle)?;
        sound.position = position;
        sound.direction = direction;
        Ok(())
    }

    pub fn add_sector_sound(&self, sector: u32, handle: u32) -> Result<()> {
        sound_mut(&mut *self.lock()?, handle)?
            .sectors
            .insert(sector);
        Ok(())
    }

    pub fn delete_sector_sound(&self, sector: u32, handle: u32) -> Result<()> {
        sound_mut(&mut *self.lock()?, handle)?
            .sectors
            .remove(&sector);
        Ok(())
    }

    /// Primary PC vtable calls with scalar words. Pointer arguments (position,
    /// velocity, GetRange/GetCone outputs) are resolved by the canonical host.
    pub fn sound_virtual(&self, handle: u32, slot: u32, arguments: &[u32]) -> Result<u32> {
        let expected = match slot {
            0x00 | 0x18 | 0x80 | 0x84 | 0x90 => 0,
            0x24 | 0x54 | 0x58 | 0x64 | 0x68 | 0x6c | 0x70 | 0x74 => 1,
            0x2c | 0x60 | 0x88 => 2,
            0x5c => 4,
            _ => {
                return Err(AudioError(format!(
                    "unsupported sound vtable slot {slot:#x}"
                )))
            }
        };
        if arguments.len() != expected {
            return Err(AudioError(format!("sound slot {slot:#x} argument count")));
        }
        let mut mixer = self.lock()?;
        if slot == 0 {
            mixer
                .sounds
                .remove(&handle)
                .ok_or_else(|| AudioError("release of unowned sound".into()))?;
            mixer.sources.retain(|_, source| source.strong_count() != 0);
            return Ok(0);
        }
        let sound = sound_mut(&mut mixer, handle)?;
        let float = |i| -> Result<f32> {
            let value = f32::from_bits(arguments[i]);
            finite(&[value])?;
            Ok(value)
        };
        match slot {
            0x18 | 0x90 => {} // State is applied synchronously by the mixer.
            0x24 | 0x88 => {
                sound.on = arguments[0] != 0;
                if sound.on {
                    sound.frame_position = 0.0;
                }
            }
            0x2c => sound.parent = arguments[0],
            0x54 => return Ok(u32::from(sound.is_playing())),
            0x58 => {
                if arguments[0] > 3 {
                    return Err(AudioError("unsupported sound type".into()));
                }
                sound.sound_type =
                    if sound.source.as_ref().is_some_and(|s| s.channels > 1) && arguments[0] != 0 {
                        3
                    } else {
                        arguments[0]
                    };
            }
            0x5c => sound.range = [float(0)?, float(1)?, float(2)?, float(3)?],
            0x60 => {
                sound.cone = [
                    float(0)?.min(std::f32::consts::PI),
                    float(1)?.min(std::f32::consts::PI),
                ]
            }
            0x64 => sound.out_volume = float(0)?,
            0x68 => sound.volume = float(0)?,
            0x6c => sound.looping = arguments[0] != 0,
            0x70 => {
                if let Some(source) = &sound.source {
                    sound.frame_position =
                        f64::from(arguments[0]) * f64::from(source.sample_rate) / 1000.0;
                    if sound.looping {
                        sound.frame_position %= source.frames() as f64;
                    }
                }
            }
            0x74 => {
                let value = float(0)?;
                if value <= 0.0 {
                    return Err(AudioError("nonpositive sound frequency".into()));
                }
                sound.frequency = value;
            }
            0x80 => return Ok(sound.current_time_ms()),
            0x84 => return Ok(sound.source.as_ref().map_or(0, |s| s.duration_ms())),
            _ => unreachable!(),
        }
        Ok(0)
    }

    pub fn set_velocity(&self, handle: u32, velocity: [f32; 3]) -> Result<()> {
        finite(&velocity)?;
        sound_mut(&mut *self.lock()?, handle)?.velocity = velocity;
        Ok(())
    }

    /// Render interleaved stereo. The same path serves a native device callback
    /// and deterministic capture; rendering advances every enabled source even
    /// if it is outside the listener's range or current sector.
    pub fn mix_stereo(&self, sample_rate: u32, output: &mut [f32]) -> Result<()> {
        if sample_rate == 0 || !output.len().is_multiple_of(2) {
            return Err(AudioError(
                "invalid output sample rate or stereo buffer".into(),
            ));
        }
        output.fill(0.0);
        let mut mixer = self.lock()?;
        let listener = mixer.listener_position;
        let right = mixer.listener_right;
        let sector = mixer.listener_sector;
        for sound in mixer.sounds.values_mut() {
            if !sound.is_playing() {
                continue;
            }
            let source = sound.source.as_ref().unwrap();
            let (left_gain, right_gain) = gains(sound, listener, right, sector);
            let increment =
                f64::from(source.sample_rate) / f64::from(sample_rate) * f64::from(sound.frequency);
            let frames = source.frames();
            for out in output.as_chunks_mut::<2>().0 {
                if sound.frame_position >= frames as f64 {
                    if !sound.looping {
                        break;
                    }
                    sound.frame_position %= frames as f64;
                }
                let index = sound.frame_position as usize;
                let next = if index + 1 < frames {
                    index + 1
                } else if sound.looping {
                    0
                } else {
                    index
                };
                let fraction = (sound.frame_position - index as f64) as f32;
                let channels = usize::from(source.channels);
                for (channel, output_sample) in out.iter_mut().enumerate() {
                    let input_channel = channel.min(channels - 1);
                    let a = source.samples[index * channels + input_channel];
                    let b = source.samples[next * channels + input_channel];
                    *output_sample += (a + (b - a) * fraction)
                        * if channel == 0 { left_gain } else { right_gain };
                }
                sound.frame_position += increment;
            }
        }
        for sample in output.iter_mut() {
            *sample = sample.clamp(-1.0, 1.0);
        }
        mixer.mixed_frames = mixer.mixed_frames.saturating_add((output.len() / 2) as u64);
        Ok(())
    }

    pub fn mixed_frames(&self) -> Result<u64> {
        Ok(self.lock()?.mixed_frames)
    }
}

fn canonical_name(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> crate::vehicle_sound::VehicleGearSoundEffects<T>
    for RuntimeAudio
{
    fn is_playing(
        &mut self,
        sound: u32,
        query_flag: bool,
        _context: crate::vehicle_sound::VehicleGearSoundContext<'_, T>,
    ) -> std::result::Result<u8, crate::vehicle_sound::VehicleGearSoundUnavailable> {
        use crate::vehicle_sound::{
            VehicleGearSoundCall as Call, VehicleGearSoundUnavailable as Error,
        };
        self.sound_virtual(sound, 0x54, &[u32::from(query_flag)])
            .map(|v| v as u8)
            .map_err(|_| Error::Effect(Call::IsPlaying { sound, query_flag }))
    }
    fn set_on(
        &mut self,
        sound: u32,
        enabled: bool,
        _context: crate::vehicle_sound::VehicleGearSoundContext<'_, T>,
    ) -> std::result::Result<(), crate::vehicle_sound::VehicleGearSoundUnavailable> {
        use crate::vehicle_sound::{
            VehicleGearSoundCall as Call, VehicleGearSoundUnavailable as Error,
        };
        self.sound_virtual(sound, 0x24, &[u32::from(enabled)])
            .map(|_| ())
            .map_err(|_| Error::Effect(Call::SetOn { sound, enabled }))
    }
    fn set_volume(
        &mut self,
        sound: u32,
        volume_bits: u32,
        _context: crate::vehicle_sound::VehicleGearSoundContext<'_, T>,
    ) -> std::result::Result<(), crate::vehicle_sound::VehicleGearSoundUnavailable> {
        use crate::vehicle_sound::{
            VehicleGearSoundCall as Call, VehicleGearSoundUnavailable as Error,
        };
        self.sound_virtual(sound, 0x68, &[volume_bits])
            .map(|_| ())
            .map_err(|_| Error::Effect(Call::SetVolume { sound, volume_bits }))
    }
}

fn finite(values: &[f32]) -> Result<()> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(AudioError("nonfinite audio parameter".into()))
    }
}

fn sound_mut(mixer: &mut Mixer, handle: u32) -> Result<&mut SoundState> {
    mixer
        .sounds
        .get_mut(&handle)
        .ok_or_else(|| AudioError(format!("unowned sound {handle:#x}")))
}

fn gains(
    sound: &SoundState,
    listener: [f32; 3],
    right: [f32; 3],
    sector: Option<u32>,
) -> (f32, f32) {
    if sector.is_some_and(|s| !sound.sectors.is_empty() && !sound.sectors.contains(&s)) {
        return (0.0, 0.0);
    }
    if sound.sound_type == 3 {
        return (sound.volume.max(0.0), sound.volume.max(0.0));
    }
    let delta = std::array::from_fn::<_, 3, _>(|i| sound.position[i] - listener[i]);
    let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
    let [near, far, middle, middle_volume] = sound.range;
    let mid = near + (far - near) * middle;
    let attenuation = if distance <= near {
        1.0
    } else if distance >= far {
        0.0
    } else if distance <= mid && mid > near {
        1.0 + (middle_volume - 1.0) * (distance - near) / (mid - near)
    } else if far > mid {
        middle_volume * (1.0 - (distance - mid) / (far - mid))
    } else {
        0.0
    };
    let mut volume = sound.volume.max(0.0) * attenuation.clamp(0.0, 1.0);
    if sound.sound_type == 2 && distance > f32::EPSILON {
        let length = sound.direction.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length > f32::EPSILON {
            let cosine = -delta
                .iter()
                .zip(sound.direction)
                .map(|(a, b)| a * b)
                .sum::<f32>()
                / (distance * length);
            let inner = (sound.cone[0] * 0.5).cos();
            let outer = (sound.cone[1] * 0.5).cos();
            volume *= if cosine >= inner {
                1.0
            } else if cosine <= outer || inner <= outer {
                sound.out_volume
            } else {
                sound.out_volume + (1.0 - sound.out_volume) * (cosine - outer) / (inner - outer)
            };
        }
    }
    let pan = if distance > f32::EPSILON {
        delta.iter().zip(right).map(|(a, b)| a * b).sum::<f32>() / distance
    } else {
        0.0
    }
    .clamp(-1.0, 1.0);
    // A centered point source preserves the original mono amplitude per channel.
    (volume * (1.0 - pan.max(0.0)), volume * (1.0 + pan.min(0.0)))
}

/// Streaming capture of the actual PCM output. A placeholder header is repaired
/// on finish; samples are bounded by RIFF's 32-bit data length, not held in RAM.
pub struct StereoWavCapture {
    file: std::fs::File,
    sample_rate: u32,
    data_bytes: u32,
}

impl StereoWavCapture {
    pub fn create(path: impl AsRef<std::path::Path>, sample_rate: u32) -> Result<Self> {
        use std::io::Write;
        if sample_rate == 0 || sample_rate > 384_000 {
            return Err(AudioError("invalid capture sample rate".into()));
        }
        let mut file = std::fs::File::create(path)
            .map_err(|e| AudioError(format!("create audio capture: {e}")))?;
        file.write_all(&[0; 44])
            .map_err(|e| AudioError(format!("write audio capture: {e}")))?;
        Ok(Self {
            file,
            sample_rate,
            data_bytes: 0,
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<()> {
        use std::io::Write;
        if !samples.len().is_multiple_of(2) || !samples.iter().all(|s| s.is_finite()) {
            return Err(AudioError("invalid stereo capture samples".into()));
        }
        let added = u32::try_from(samples.len())
            .ok()
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| AudioError("audio capture exceeds WAV limit".into()))?;
        let next = self
            .data_bytes
            .checked_add(added)
            .filter(|n| *n <= u32::MAX - 36)
            .ok_or_else(|| AudioError("audio capture exceeds WAV limit".into()))?;
        let mut buffer = [0u8; 4096];
        for chunk in samples.chunks(buffer.len() / 2) {
            for (sample, destination) in chunk.iter().zip(buffer.as_chunks_mut::<2>().0.iter_mut())
            {
                let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
                destination.copy_from_slice(&value.to_le_bytes());
            }
            self.file
                .write_all(&buffer[..chunk.len() * 2])
                .map_err(|e| AudioError(format!("write audio capture samples: {e}")))?;
        }
        self.data_bytes = next;
        Ok(())
    }

    pub fn frames(&self) -> u64 {
        u64::from(self.data_bytes / 4)
    }

    pub fn finish(mut self) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        let mut header = Vec::with_capacity(44);
        header.extend(b"RIFF");
        header.extend((self.data_bytes + 36).to_le_bytes());
        header.extend(b"WAVEfmt ");
        header.extend(16u32.to_le_bytes());
        header.extend(1u16.to_le_bytes());
        header.extend(2u16.to_le_bytes());
        header.extend(self.sample_rate.to_le_bytes());
        header.extend((self.sample_rate * 4).to_le_bytes());
        header.extend(4u16.to_le_bytes());
        header.extend(16u16.to_le_bytes());
        header.extend(b"data");
        header.extend(self.data_bytes.to_le_bytes());
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.write_all(&header))
            .and_then(|_| self.file.flush())
            .map_err(|e| AudioError(format!("finish audio capture: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + samples.len() as u32 * 2).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(rate.to_le_bytes());
        bytes.extend((rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend((channels * 2).to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend((samples.len() as u32 * 2).to_le_bytes());
        for sample in samples {
            bytes.extend(sample.to_le_bytes());
        }
        bytes
    }

    fn loaded() -> RuntimeAudio {
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        let source = Arc::new(
            PcmSource::from_wav("synthetic.wav", &wav(&[0, 8192, 16384, -8192], 1, 4)).unwrap(),
        );
        let mut m = audio.lock().unwrap();
        m.sources
            .insert("synthetic.wav".into(), Arc::downgrade(&source));
        m.sounds.get_mut(&0x1000).unwrap().source = Some(source);
        drop(m);
        audio.sound_virtual(0x1000, 0x58, &[3]).unwrap();
        audio
    }

    #[test]
    fn pcm_validation_rejects_truncation_bad_align_and_empty() {
        let bytes = wav(&[1, 2], 1, 22050);
        assert_eq!(PcmSource::from_wav("a", &bytes).unwrap().frames(), 2);
        for n in 0..bytes.len() {
            assert!(PcmSource::from_wav("a", &bytes[..n]).is_err());
        }
        let mut bad = bytes.clone();
        bad[32] = 8;
        assert!(PcmSource::from_wav("a", &bad).is_err());
        assert!(PcmSource::from_wav("a", &wav(&[], 1, 22050)).is_err());
    }

    #[test]
    fn actual_slots_play_volume_frequency_loop_seek_release() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x24, &[1]).unwrap();
        audio
            .sound_virtual(0x1000, 0x68, &[0.5f32.to_bits()])
            .unwrap();
        let mut out = [0.0; 4];
        audio.mix_stereo(4, &mut out).unwrap();
        assert_eq!(out, [0.0, 0.0, 0.125, 0.125]);
        audio
            .sound_virtual(0x1000, 0x74, &[2.0f32.to_bits()])
            .unwrap();
        audio.mix_stereo(4, &mut out).unwrap();
        assert_eq!(out, [0.25, 0.25, 0.0, 0.0]);
        assert_eq!(audio.sound_virtual(0x1000, 0x54, &[0]).unwrap(), 0);
        audio.sound_virtual(0x1000, 0x6c, &[1]).unwrap();
        audio.sound_virtual(0x1000, 0x70, &[250]).unwrap();
        audio
            .sound_virtual(0x1000, 0x74, &[1.0f32.to_bits()])
            .unwrap();
        audio.mix_stereo(4, &mut out).unwrap();
        assert_eq!(out, [0.125, 0.125, 0.25, 0.25]);
        audio.sound_virtual(0x1000, 0, &[]).unwrap();
        assert_eq!(audio.live_sound_count().unwrap(), 0);
        assert_eq!(audio.live_source_count().unwrap(), 0);
        assert!(!audio.owns(0x1000));
        audio.create_sound_at(0x1000).unwrap();
        assert!(!audio.sound(0x1000).unwrap().is_playing());
    }

    #[test]
    fn positioned_sector_sound_advances_while_inaudible() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x58, &[1]).unwrap();
        audio
            .sound_virtual(
                0x1000,
                0x5c,
                &[
                    1.0f32.to_bits(),
                    10.0f32.to_bits(),
                    0.5f32.to_bits(),
                    0.5f32.to_bits(),
                ],
            )
            .unwrap();
        audio
            .set_position(0x1000, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            .unwrap();
        audio.add_sector_sound(1, 0x1000).unwrap();
        audio
            .set_listener([0.0; 3], [1.0, 0.0, 0.0], Some(2))
            .unwrap();
        audio.sound_virtual(0x1000, 0x24, &[1]).unwrap();
        let mut out = [0.0; 2];
        audio.mix_stereo(4, &mut out).unwrap();
        assert_eq!(out, [0.0, 0.0]);
        audio
            .set_listener([0.0; 3], [1.0, 0.0, 0.0], Some(1))
            .unwrap();
        audio.mix_stereo(4, &mut out).unwrap();
        assert_eq!(out, [0.0, 0.25]);
        audio.delete_sector_sound(1, 0x1000).unwrap();
        assert!(audio.sound(0x1000).unwrap().sectors.is_empty());
    }
}
