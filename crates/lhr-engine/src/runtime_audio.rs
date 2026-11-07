//! Owned sound resources and a PCM mixer for the native Rust runtime.
//!
//! The call slots follow the PC I3D_sound vtable. The host supplies canonical
//! object addresses and resolves pointer arguments; no original DLL is loaded.
//! Mixing and device playback are host implementations, not a claim of exact
//! DirectSound, sector fade, reverb, Doppler, or voice prioritization equivalence.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use crate::frame_transform::FrameKey;
use lhr_assets::Installation;

#[path = "runtime_audio_api.rs"]
mod api;
pub use api::{PlaybackQuery, SoundId, SoundKind};

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

/// Which live ModelArena frame owns a host PCM handle. Scene Sound factory
/// handles are direct keys; vehicle frame effects use inert projected handles.
/// Recording the owner at construction avoids guessing from numeric ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SoundFrameOwner {
    Scene(FrameKey),
    Vehicle(FrameKey),
}

/// Decoded source data. Original compressed archive audio is decoded by
/// Installation/RWData before the RIFF parser sees it.
#[derive(Debug)]
pub struct PcmSource {
    pub name: String,
    pub open_flags: u32,
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
            open_flags: 0,
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

/// Source-backed I3D state values. The upstream names are descriptive guesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Stop,
    Play,
    Pause,
    Resume,
    NoCamera,
    /// I3D_sound::Duplicate copies the low byte of the source's sound flags
    /// into m_SetState. Those bytes are not limited to the five named states.
    Native(u8),
}

impl PlaybackState {
    pub fn from_native_byte(raw: u8) -> Self {
        match raw {
            1 => Self::Stop,
            2 => Self::Play,
            3 => Self::Pause,
            4 => Self::Resume,
            5 => Self::NoCamera,
            other => Self::Native(other),
        }
    }

    pub fn native_byte(self) -> u8 {
        match self {
            Self::Stop => 1,
            Self::Play => 2,
            Self::Pause => 3,
            Self::Resume => 4,
            Self::NoCamera => 5,
            Self::Native(raw) => raw,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SoundState {
    pub handle: u32,
    generation: u64,
    frame_owner: Option<SoundFrameOwner>,
    pub source: Option<Arc<PcmSource>>,
    pub open_flags: u32,
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
    pub state: PlaybackState,
    pub world_scale: f32,
    /// Host PCM voice status, separate from the logical I3D lifetime.
    pub voice_playing: bool,
    frame_position: f64,
    clock_ms: u32,
    start_time: u32,
    pause_time: u32,
    pause_pos: u32,
}

impl SoundState {
    fn new(handle: u32, frame_owner: Option<SoundFrameOwner>, generation: u64) -> Self {
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
            generation,
            frame_owner,
            source: None,
            open_flags: 0,
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
            state: PlaybackState::Stop,
            world_scale: 1.0,
            voice_playing: false,
            clock_ms: 0,
            start_time: 0,
            pause_time: 0,
            pause_pos: 0,
        }
    }

    pub fn current_time_ms(&self) -> u32 {
        self.source.as_ref().map_or(0, |s| {
            (self.frame_position * 1000.0 / f64::from(s.sample_rate)) as u32
        })
    }

    pub fn is_playing(&self) -> bool {
        if self.source.is_none() || self.state == PlaybackState::Stop {
            return false;
        }
        let since = if matches!(self.state, PlaybackState::Pause | PlaybackState::Resume) {
            self.pause_time
        } else {
            self.start_time
        };
        self.looping
            || self
                .pause_pos
                .wrapping_add(self.clock_ms.wrapping_sub(since))
                < self.logical_duration_ms()
    }

    fn logical_duration_ms(&self) -> u32 {
        self.source.as_ref().map_or(0, |source| {
            (source.duration_ms() as f32 / self.frequency) as u32
        })
    }

    fn pause(&mut self) {
        if self.state != PlaybackState::Pause {
            self.pause_time = self.clock_ms;
            self.pause_pos = self.current_time_ms();
            self.state = PlaybackState::Pause;
            self.native_tail[0] &= !0x1000;
            self.voice_playing = false;
        }
    }

    fn seek(&mut self, time: u32) {
        let Some(source) = &self.source else { return };
        self.start_time = self.clock_ms.wrapping_sub(time);
        if matches!(self.state, PlaybackState::Pause | PlaybackState::Resume) {
            self.pause_time = self.clock_ms;
            self.pause_pos = time;
            if self.state == PlaybackState::Pause {
                return;
            }
        }
        // PCM is the host voice: retain its independent, frozen cursor while paused.
        self.frame_position = f64::from(time) * f64::from(source.sample_rate) / 1000.0;
        if self.looping {
            self.frame_position %= source.frames() as f64;
        }
    }

    fn set_on(&mut self, on: bool, camera: bool, sector: Option<u32>) {
        self.on = on;
        if self.source.is_none() {
            return;
        }
        if !on || self.sound_type == 0 {
            self.state = PlaybackState::Stop;
            self.voice_playing = false;
            if self.sound_type != 0 {
                self.native_tail[0] &= !0x1000;
            }
        } else if !camera {
            self.state = PlaybackState::NoCamera;
        } else if !self.sectors.is_empty()
            && !sector.is_some_and(|sector| self.sectors.contains(&sector))
        {
            self.pause();
        } else {
            if !self.sectors.is_empty() {
                self.native_tail[0] |= 0x200;
            }
            if matches!(self.state, PlaybackState::Pause | PlaybackState::Resume) {
                let elapsed = self.clock_ms.wrapping_sub(self.pause_time);
                let duration = self.logical_duration_ms();
                if duration == 0
                    || (!self.looping && self.pause_pos.wrapping_add(elapsed) >= duration)
                {
                    self.state = PlaybackState::Stop;
                    return;
                }
                let time = ((self.pause_pos.wrapping_add(elapsed) % duration) as f32
                    * self.frequency) as u32;
                self.seek(time);
                self.pause_pos = time;
            } else {
                self.pause_pos = 0;
            }
            self.state = PlaybackState::Play;
            self.native_tail[0] |= 0x1000;
            self.start_time = self.clock_ms;
        }
    }

    fn scaled_range(&self) -> (f32, f32, f32) {
        let field = |offset: usize| f32::from_bits(self.native_tail[(offset - 0x140) / 4]);
        (field(0x16c), field(0x174), field(0x180))
    }

    fn scaled_far_squared(&self) -> f32 {
        f32::from_bits(self.native_tail[(0x178 - 0x140) / 4])
    }

    fn refresh_scaled_range(&mut self) {
        if self.native_tail[0] & 0x84 == 0 {
            return;
        }
        let far = self.range[1] * self.world_scale;
        let mut near = self.range[0] * self.world_scale;
        if near >= far - 1e-8 {
            near = far - 1e-8;
        }
        let middle = near + self.range[2] * (far - near);
        for (offset, value) in [
            (0x16c, near),
            (0x174, far),
            (0x178, far * far),
            (0x180, middle),
        ] {
            self.native_tail[(offset - 0x140) / 4] = value.to_bits();
        }
        self.native_tail[0] &= !0x84;
    }
}

#[derive(Debug)]
struct Mixer {
    next_sound_generation: u64,
    sounds: BTreeMap<u32, SoundState>,
    sources: BTreeMap<String, Weak<PcmSource>>,
    listener_position: [f32; 3],
    listener_right: [f32; 3],
    listener_sector: Option<u32>,
    camera_present: bool,
    clock_ms: f64,
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

impl RuntimeAudio {
    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Default for RuntimeAudio {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Mixer {
            next_sound_generation: 0,
            sounds: BTreeMap::new(),
            sources: BTreeMap::new(),
            listener_position: [0.0; 3],
            listener_right: [1.0, 0.0, 0.0],
            listener_sector: None,
            camera_present: false,
            clock_ms: 0.0,
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
            0x144 => u32::from(sound.state.native_byte()),
            0x150 => sound.start_time,
            0x154 => sound.pause_time,
            0x158 => sound.pause_pos,
            0x1a4 => (sound.cone[0] * 0.5).cos().to_bits(),
            0x1ac => (sound.cone[1] * 0.5).cos().to_bits(),
            0x164 => u32::from(sound.looping),
            0x168 => sound.range[0].to_bits(),
            0x170 => sound.range[1].to_bits(),
            0x17c => sound.range[2].to_bits(),
            0x184 => sound.range[3].to_bits(),
            0x188 => sound.out_volume.to_bits(),
            0x18c => sound.volume.to_bits(),
            0x194 => sound.frequency.to_bits(),
            0x198 => sound.open_flags,
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
            0x198 => sound.open_flags = value,
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
        self.create_sound_at_with_owner(handle, None)
    }

    pub(crate) fn create_frame_sound_at(&self, handle: u32, owner: SoundFrameOwner) -> Result<()> {
        self.create_sound_at_with_owner(handle, Some(owner))
    }

    fn create_sound_at_with_owner(
        &self,
        handle: u32,
        owner: Option<SoundFrameOwner>,
    ) -> Result<()> {
        let mut mixer = self.lock()?;
        if handle == 0 || mixer.sounds.contains_key(&handle) {
            return Err(AudioError("invalid or already owned sound address".into()));
        }
        let generation = mixer
            .next_sound_generation
            .checked_add(1)
            .ok_or_else(|| AudioError("sound identity generation exhausted".into()))?;
        let mut sound = SoundState::new(handle, owner, generation);
        sound.clock_ms = mixer.clock_ms as u64 as u32;
        mixer.sounds.insert(handle, sound);
        mixer.next_sound_generation = generation;
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

    /// I3D_sound::m_SetState at PC RVA 0x59360 stores one byte without
    /// validating the five named playback states. Duplicate passes the low
    /// byte of the source's sound flags, so preserve every possible value.
    pub fn set_native_state_byte(&self, handle: u32, raw: u8) -> Result<()> {
        sound_mut(&mut *self.lock()?, handle)?.state = PlaybackState::from_native_byte(raw);
        Ok(())
    }

    pub fn live_sound_count(&self) -> Result<usize> {
        Ok(self.lock()?.sounds.len())
    }

    /// Attached host PCM voices, including paused or waiting sounds. This is
    /// distinct from the number of unique decoded PCM sources.
    pub fn live_voice_count(&self) -> Result<usize> {
        Ok(self
            .lock()?
            .sounds
            .values()
            .filter(|sound| sound.source.is_some())
            .count())
    }

    pub fn listener_present(&self) -> Result<bool> {
        Ok(self.lock()?.camera_present)
    }

    pub fn sound_handles(&self) -> Result<Vec<u32>> {
        Ok(self.lock()?.sounds.keys().copied().collect())
    }

    pub(crate) fn sound_frame_owner(&self, handle: u32) -> Result<SoundFrameOwner> {
        self.lock()?
            .sounds
            .get(&handle)
            .and_then(|sound| sound.frame_owner)
            .ok_or_else(|| AudioError("sound owner has no canonical frame".into()))
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
        self.load_sound_with_flags(handle, name, 0, installation)
    }

    pub fn load_sound_with_flags(
        &self,
        handle: u32,
        name: &str,
        flags: u32,
        installation: &Installation,
    ) -> Result<()> {
        self.load_sound_with_optional(handle, name, flags, || {
            installation
                .read_original_path(name)
                .map(Some)
                .map_err(|e| AudioError(format!("sound source {name}: {e:?}")))
        })?;
        Ok(())
    }

    /// Host-backed original path Open. Only a proven absent archive/loose
    /// entry becomes I3DERR_NOFILE. Decode, selected-source and owner errors
    /// remain explicit host interruptions instead of invented native HRESULTs.
    pub fn open_original_sound(
        &self,
        handle: u32,
        name: &str,
        flags: u32,
        installation: &Installation,
    ) -> Result<i32> {
        if !name.is_ascii() {
            return Err(AudioError(
                "non-ASCII sound path requires original filename encoding".into(),
            ));
        }
        let normalized = name.replace('\\', "/");
        let opened = self.load_sound_with_optional(handle, name, flags, || {
            installation
                .read_optional(&normalized)
                .map_err(|e| AudioError(format!("sound source {name}: {e:?}")))
        })?;
        Ok(if opened { 0 } else { 0x8000_0001u32 as i32 })
    }

    /// Scene/cache path Open bound to the factory-issued sound incarnation.
    pub(crate) fn open_sound_source(
        &self,
        id: &SoundId,
        name: &str,
        flags: u32,
        installation: &Installation,
    ) -> Result<i32> {
        if !name.is_ascii() {
            return Err(AudioError(
                "non-ASCII sound path requires original filename encoding".into(),
            ));
        }
        let normalized = name.replace('\\', "/");
        let opened = self.load_sound_source_with(id, name, flags, || {
            installation
                .read_optional(&normalized)
                .map_err(|e| AudioError(format!("sound source {name}: {e:?}")))
        })?;
        Ok(if opened { 0 } else { 0x8000_0001u32 as i32 })
    }

    /// C_I3D_sound_cache::Duplicate (Game 0x5c23f0) only reopens the
    /// source file. The cache ignores Open's HRESULT, not host identity errors.
    /// Read source fields once, then revalidate the retained destination on
    /// both sides of I/O. Never refresh either ID from an integer handle.
    pub(crate) fn reopen_cached_sound(
        &self,
        destination: &SoundId,
        source: &SoundId,
        installation: &Installation,
    ) -> Result<i32> {
        source.check_owner(self)?;
        let source = {
            let mut mixer = self.lock()?;
            let handle = source.checked_handle(&mixer)?;
            sound_mut(&mut mixer, handle)?.clone()
        };
        let filename = source
            .source
            .as_ref()
            .map_or("", |source| source.name.as_str());
        self.open_sound_source(destination, filename, source.open_flags, installation)
    }

    /// Native derived Duplicate RVA 0x59250. The common frame Duplicate is a
    /// separate ModelArena operation after these source, voice and field writes.
    pub fn duplicate_sound_derived(
        &self,
        destination: u32,
        source: u32,
        installation: &Installation,
    ) -> Result<i32> {
        let original = self.sound(source)?;
        self.sound_virtual(destination, 0x24, &[0])?;
        // Duplicate releases the destination's existing voice before Open.
        // Drop the host voice/source attachment here so an identical filename
        // cannot trigger Open's exact-name fast path and retain that old voice.
        {
            let mut mixer = self.lock()?;
            let target = sound_mut(&mut mixer, destination)?;
            target.source = None;
            target.voice_playing = false;
            target.frame_position = 0.0;
        }
        let filename = original
            .source
            .as_ref()
            .map_or("", |source| source.name.as_str());
        let result =
            self.open_original_sound(destination, filename, original.open_flags, installation)?;
        let copied_words = [
            0x164, 0x188, 0x18c, 0x194, 0x198, 0x168, 0x170, 0x17c, 0x184, 0x19c, 0x1a0, 0x1a8,
        ]
        .map(|offset| self.read_sound_word(source, offset))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
        let mut mixer = self.lock()?;
        let target = sound_mut(&mut mixer, destination)?;
        target.looping = original.looping;
        target.out_volume = original.out_volume;
        target.volume = original.volume;
        target.frequency = original.frequency;
        target.open_flags = original.open_flags;
        target.range = original.range;
        target.sound_type = original.sound_type;
        target.cone = original.cone;
        target.state = PlaybackState::from_native_byte(original.native_tail[0] as u8);
        for (offset, value) in [
            0x164, 0x188, 0x18c, 0x194, 0x198, 0x168, 0x170, 0x17c, 0x184, 0x19c, 0x1a0, 0x1a8,
        ]
        .into_iter()
        .zip(copied_words)
        {
            let index = ((offset - 0x140) / 4) as usize;
            target.native_tail[index] = value;
        }
        target.native_tail[1] = u32::from(target.state.native_byte());
        Ok(result)
    }

    #[cfg(test)]
    fn load_sound_with(
        &self,
        handle: u32,
        name: &str,
        read: impl FnOnce() -> Result<Vec<u8>>,
    ) -> Result<()> {
        self.load_sound_with_optional(handle, name, 0, || read().map(Some))?;
        Ok(())
    }

    fn load_sound_with_optional(
        &self,
        handle: u32,
        name: &str,
        flags: u32,
        read: impl FnOnce() -> Result<Option<Vec<u8>>>,
    ) -> Result<bool> {
        // Compatibility callers supply a handle at entry, not after I/O.
        let id = self.sound_id_from_handle(handle)?;
        self.load_sound_source_with(&id, name, flags, read)
    }

    fn load_sound_source_with(
        &self,
        id: &SoundId,
        name: &str,
        flags: u32,
        read: impl FnOnce() -> Result<Option<Vec<u8>>>,
    ) -> Result<bool> {
        id.check_owner(self)?;
        let canonical = canonical_name(name);
        let cached = {
            let mut mixer = self.lock()?;
            let handle = id.checked_handle(&mixer)?;
            let sound = sound_mut(&mut mixer, handle)?;
            // Open's strcmp fast path is exact, unlike its shared-source stricmp lookup.
            if sound
                .source
                .as_ref()
                .is_some_and(|source| source.name == name)
            {
                return Ok(true);
            }
            // Release the old voice and source BEFORE opening the replacement.
            sound.source = None;
            sound.voice_playing = false;
            sound.frame_position = 0.0;
            mixer.sources.get(&canonical).and_then(Weak::upgrade)
        };
        let source = match cached {
            Some(source) => source,
            None => {
                // Do not hold the mixer lock across archive/filesystem I/O.
                // A real read/decode error keeps its original precedence.
                let Some(bytes) = read()? else {
                    self.validate_sound_id(id)?;
                    return Ok(false);
                };
                let mut source = PcmSource::from_wav(name, &bytes)?;
                let flags = flags | 0x200 | if flags & 0x8 != 0 { 0x10 } else { 0 };
                source.open_flags = flags & if source.channels > 1 { !0x200 } else { !0 };
                Arc::new(source)
            }
        };
        let mut mixer = self.lock()?;
        // Check before publishing even a weak cache entry. Release/reuse during
        // I/O must not attach data to, or change the cache for, a replacement.
        let handle = id.checked_handle(&mixer)?;
        mixer.sources.insert(canonical, Arc::downgrade(&source));
        let camera = mixer.camera_present;
        let sector = mixer.listener_sector;
        let sound = sound_mut(&mut mixer, handle)?;
        if source.channels > 1 && sound.sound_type != 0 {
            sound.sound_type = 3;
        }
        if source.channels == 1 {
            sound.cone = [std::f32::consts::FRAC_PI_3, std::f32::consts::FRAC_PI_2];
        }
        sound.source = Some(source);
        sound.open_flags = sound.source.as_ref().unwrap().open_flags;
        if sound.on {
            sound.set_on(true, camera, sector);
        }
        sound.seek(0);
        sound.native_tail[0] = 0x21ff;
        Ok(true)
    }

    /// No active camera is distinct from a camera with no assigned sector.
    pub fn clear_listener(&self) -> Result<()> {
        self.lock()?.camera_present = false;
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
        m.camera_present = true;
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
        if sound.position != position {
            sound.position = position;
            sound.native_tail[0] |= 0x1;
        }
        if sound.direction != direction {
            sound.direction = direction;
            sound.native_tail[0] |= 0x2;
        }
        Ok(())
    }

    pub fn set_world_scale(&self, handle: u32, scale: f32) -> Result<()> {
        finite(&[scale])?;
        let mut mixer = self.lock()?;
        let sound = sound_mut(&mut mixer, handle)?;
        if sound.world_scale != scale {
            sound.world_scale = scale;
            sound.native_tail[0] |= 0x80;
        }
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
        self.legacy_sound_call(handle, slot, arguments)
    }

    pub fn set_velocity(&self, handle: u32, velocity: [f32; 3]) -> Result<()> {
        finite(&velocity)?;
        let mut mixer = self.lock()?;
        let sound = sound_mut(&mut mixer, handle)?;
        if !near_equal(&sound.velocity, &velocity) {
            sound.velocity = velocity;
            sound.native_tail[0] |= 0x2000;
        }
        Ok(())
    }

    /// Render interleaved stereo. The same path serves a native device callback
    /// and deterministic capture. The render duration drives the logical clock;
    /// paused voices retain their PCM cursor while their logical lifetime runs.
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
        let camera = mixer.camera_present;
        let next_clock =
            mixer.clock_ms + (output.len() / 2) as f64 * 1000.0 / f64::from(sample_rate);
        for sound in mixer.sounds.values_mut() {
            update_sound(sound, camera, sector, listener)?;
            if !sound.voice_playing {
                sound.clock_ms = next_clock as u64 as u32;
                continue;
            }
            let source = sound.source.as_ref().unwrap();
            let (left_gain, right_gain) = gains(sound, listener, right, sector)?;
            let increment =
                f64::from(source.sample_rate) / f64::from(sample_rate) * f64::from(sound.frequency);
            let frames = source.frames();
            for out in output.as_chunks_mut::<2>().0 {
                if sound.frame_position >= frames as f64 {
                    if !sound.looping {
                        sound.voice_playing = false;
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
            if sound.looping {
                sound.frame_position %= frames as f64;
            } else if sound.frame_position >= frames as f64 {
                sound.voice_playing = false;
            }
            sound.clock_ms = next_clock as u64 as u32;
        }
        mixer.clock_ms = next_clock;
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
    name.to_ascii_lowercase()
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> crate::vehicle_sound::VehicleGearSoundEffects<T>
    for RuntimeAudio
{
    // Compatibility boundary: each call re-resolves the raw token word with
    // `sound_id_from_handle`. No generation/liveness is carried in
    // `GearSoundHandle`, so the pre-existing repeated-lookup/ABA risk
    // remains; callback timing and behavior are unchanged. The full planned
    // `SoundId` migration is not complete.
    fn is_playing(
        &mut self,
        sound: crate::vehicle_sound::GearSoundHandle,
        query_flag: bool,
        _context: crate::vehicle_sound::VehicleGearSoundContext<'_, T>,
    ) -> std::result::Result<u8, crate::vehicle_sound::VehicleGearSoundUnavailable> {
        use crate::vehicle_sound::{
            VehicleGearSoundCall as Call, VehicleGearSoundUnavailable as Error,
        };
        self.sound_id_from_handle(sound.as_raw())
            .and_then(|id| {
                self.sound_is_playing(
                    &id,
                    if query_flag {
                        PlaybackQuery::Voice
                    } else {
                        PlaybackQuery::Logical
                    },
                )
            })
            .map(u8::from)
            .map_err(|_| Error::Effect(Call::IsPlaying { sound, query_flag }))
    }
    fn set_on(
        &mut self,
        sound: crate::vehicle_sound::GearSoundHandle,
        enabled: bool,
        _context: crate::vehicle_sound::VehicleGearSoundContext<'_, T>,
    ) -> std::result::Result<(), crate::vehicle_sound::VehicleGearSoundUnavailable> {
        use crate::vehicle_sound::{
            VehicleGearSoundCall as Call, VehicleGearSoundUnavailable as Error,
        };
        self.sound_id_from_handle(sound.as_raw())
            .and_then(|id| self.set_sound_enabled(&id, enabled))
            .map_err(|_| Error::Effect(Call::SetOn { sound, enabled }))
    }
    fn set_volume(
        &mut self,
        sound: crate::vehicle_sound::GearSoundHandle,
        volume_bits: u32,
        _context: crate::vehicle_sound::VehicleGearSoundContext<'_, T>,
    ) -> std::result::Result<(), crate::vehicle_sound::VehicleGearSoundUnavailable> {
        use crate::vehicle_sound::{
            VehicleGearSoundCall as Call, VehicleGearSoundUnavailable as Error,
        };
        self.sound_id_from_handle(sound.as_raw())
            .and_then(|id| self.set_sound_volume(&id, f32::from_bits(volume_bits)))
            .map_err(|_| Error::Effect(Call::SetVolume { sound, volume_bits }))
    }
}

fn near_equal<const N: usize>(left: &[f32; N], right: &[f32; N]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| (left - right).abs() < 1e-8)
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

/// Source S_vector::Normalize chooses X, then Z, then Y when squared
/// magnitude is below 1e-8, retaining the other components. Even zero becomes +Y.
/// Overflow/nonfinite arithmetic remains an explicit host PCM boundary.
fn normalize_cone_vector(mut vector: [f32; 3]) -> Result<[f32; 3]> {
    finite(&vector)?;
    let squared = vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2];
    finite(&[squared])?;
    if (squared - 1.0).abs() < 1e-8 {
        return Ok(vector);
    }
    if squared < 1e-8 {
        let axis = if vector[0] != 0.0 {
            0
        } else if vector[2] != 0.0 {
            2
        } else {
            1
        };
        vector[axis] = if vector[axis] < 0.0 { -1.0 } else { 1.0 };
    } else {
        let reciprocal = 1.0 / squared.sqrt();
        for component in &mut vector {
            *component *= reciprocal;
        }
    }
    finite(&vector)?;
    Ok(vector)
}

fn cone_cosine(sound: &SoundState, listener: [f32; 3]) -> Result<f32> {
    let to_camera =
        normalize_cone_vector(std::array::from_fn(|i| listener[i] - sound.position[i]))?;
    let direction = normalize_cone_vector(sound.direction)?;
    let cosine =
        direction[0] * to_camera[0] + direction[1] * to_camera[1] + direction[2] * to_camera[2];
    finite(&[cosine])?;
    Ok(cosine)
}

/// The source's update boundary decides when the independently owned host voice runs.
fn update_sound(
    sound: &mut SoundState,
    camera: bool,
    sector: Option<u32>,
    listener: [f32; 3],
) -> Result<()> {
    if sound.state == PlaybackState::Stop || !sound.on || !camera || sound.source.is_none() {
        return Ok(());
    }
    if sound.state == PlaybackState::NoCamera {
        sound.set_on(true, camera, sector);
    }
    sound.native_tail[0] &= !0x100;
    if sound.sound_type == 0 {
        sound.set_on(false, camera, sector);
        return Ok(());
    }
    if !sound.is_playing() {
        sound.state = PlaybackState::Stop;
        sound.native_tail[0] &= !0x1000;
        return Ok(());
    }
    sound.refresh_scaled_range();
    let distance_squared = sound
        .position
        .iter()
        .zip(listener)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f32>();
    let outside_sector =
        !sound.sectors.is_empty() && !sector.is_some_and(|sector| sound.sectors.contains(&sector));
    let outside_range = sound.sound_type != 3 && distance_squared >= sound.scaled_far_squared();
    if outside_sector || outside_range {
        sound.pause();
        return Ok(());
    }
    if sound.sound_type == 2
        && cone_cosine(sound, listener)? <= (sound.cone[1] * 0.5).cos()
        && sound.out_volume.abs() < 1e-8
    {
        sound.pause();
        return Ok(());
    }
    if sound.state == PlaybackState::Pause {
        sound.state = PlaybackState::Resume;
        sound.set_on(true, camera, sector);
    }
    sound.native_tail[0] &= !0x207f;
    if sound.native_tail[0] & 0x1000 != 0 {
        if sound.pause_pos == 0 {
            sound.frame_position = 0.0;
        }
        sound.voice_playing = true;
        sound.native_tail[0] &= !0x1000;
    }
    Ok(())
}

fn gains(
    sound: &SoundState,
    listener: [f32; 3],
    right: [f32; 3],
    sector: Option<u32>,
) -> Result<(f32, f32)> {
    if !sound.sectors.is_empty() && !sector.is_some_and(|s| sound.sectors.contains(&s)) {
        return Ok((0.0, 0.0));
    }
    if sound.sound_type == 3 {
        return Ok((sound.volume.max(0.0), sound.volume.max(0.0)));
    }
    let delta = std::array::from_fn::<_, 3, _>(|i| sound.position[i] - listener[i]);
    let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
    let (near, far, mid) = sound.scaled_range();
    let middle_volume = sound.range[3];
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
    if sound.sound_type == 2 {
        let cosine = cone_cosine(sound, listener)?;
        let inner = (sound.cone[0] * 0.5).cos();
        let outer = (sound.cone[1] * 0.5).cos();
        volume *= if cosine <= outer {
            sound.out_volume
        } else if cosine > inner {
            1.0
        } else {
            sound.out_volume + (1.0 - sound.out_volume) * (cosine - outer) / (inner - outer)
        };
    }
    let pan = if distance > f32::EPSILON {
        delta.iter().zip(right).map(|(a, b)| a * b).sum::<f32>() / distance
    } else {
        0.0
    }
    .clamp(-1.0, 1.0);
    // A centered point source preserves the original mono amplitude per channel.
    Ok((volume * (1.0 - pan.max(0.0)), volume * (1.0 + pan.min(0.0))))
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

    #[test]
    fn duplicate_state_byte_keeps_unnamed_native_value() {
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        audio.set_native_state_byte(0x1000, 0x8f).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x144).unwrap(), 0x8f);
        assert_eq!(
            audio.sound(0x1000).unwrap().state,
            PlaybackState::Native(0x8f)
        );
    }

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

    #[test]
    fn duplicate_replaces_existing_voice_and_copies_raw_flag_byte() {
        let path = std::env::temp_dir().join(format!(
            "lhr-sound-duplicate-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(path.join("Sounds")).unwrap();
        std::fs::write(path.join("Sounds/a.wav"), wav(&[1, 2, 3, 4], 1, 4)).unwrap();
        std::fs::write(path.join("Sounds/b.wav"), wav(&[4, 3, 2, 1], 1, 4)).unwrap();
        let installation = Installation::open(&path).unwrap();
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        audio.create_sound_at(0x1001).unwrap();
        assert_eq!(
            audio
                .open_original_sound(0x1000, "Sounds/a.wav", 0x10, &installation)
                .unwrap(),
            0
        );
        assert_eq!(
            audio
                .open_original_sound(0x1001, "Sounds/b.wav", 0x10, &installation)
                .unwrap(),
            0
        );
        assert_eq!(audio.live_source_count().unwrap(), 2);
        audio
            .sound_virtual(0x1000, 0x68, &[0.75f32.to_bits()])
            .unwrap();
        assert_eq!(
            audio
                .duplicate_sound_derived(0x1001, 0x1000, &installation)
                .unwrap(),
            0
        );
        let copied = audio.sound(0x1001).unwrap();
        assert_eq!(copied.source.as_ref().unwrap().name, "Sounds/a.wav");
        assert_eq!(copied.volume, 0.75);
        assert_eq!(copied.state, PlaybackState::Native(0xff));
        assert_eq!(audio.live_source_count().unwrap(), 1);
        audio.sound_virtual(0x1001, 0, &[]).unwrap();
        audio.sound_virtual(0x1000, 0, &[]).unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn loaded_sound_setters_dirty_flags_and_update_drive_duplicate_state() {
        let path = std::env::temp_dir().join(format!(
            "lhr-sound-flags-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("a.wav"), wav(&[4096; 16], 1, 4)).unwrap();
        let installation = Installation::open(&path).unwrap();
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        audio.create_sound_at(0x1001).unwrap();
        assert_eq!(
            audio
                .open_original_sound(0x1000, "a.wav", 8, &installation)
                .unwrap(),
            0
        );
        assert_eq!(audio.read_sound_word(0x1000, 0x198).unwrap(), 0x218);
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0x21ff);
        audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
        audio.sound_virtual(0x1000, 0x58, &[3]).unwrap();
        audio.sound_virtual(0x1000, 0x6c, &[1]).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x198).unwrap(), 0x318);
        // Loop is a per-sound play flag; its shared source retains its Open flags.
        assert_eq!(
            audio.sound(0x1000).unwrap().source.unwrap().open_flags,
            0x218
        );
        audio.sound_virtual(0x1000, 0x24, &[1]).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0x31ff);
        let mut output = [0.0; 2];
        audio.mix_stereo(4, &mut output).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0);
        assert!(output.iter().all(|sample| *sample > 0.0));
        audio
            .duplicate_sound_derived(0x1001, 0x1000, &installation)
            .unwrap();
        assert_eq!(audio.read_sound_word(0x1001, 0x144).unwrap(), 0);
        for (slot, arguments, bit) in [
            (0x58, vec![1], 0x100),
            (
                0x5c,
                vec![
                    2f32.to_bits(),
                    20f32.to_bits(),
                    0.5f32.to_bits(),
                    0.1f32.to_bits(),
                ],
                4,
            ),
            (0x60, vec![0.5f32.to_bits(), 1f32.to_bits()], 0x40),
            (0x64, vec![0.2f32.to_bits()], 0x10),
            (0x68, vec![0.75f32.to_bits()], 8),
            (0x74, vec![1.5f32.to_bits()], 0x20),
        ] {
            audio.sound_virtual(0x1000, slot, &arguments).unwrap();
            assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), bit);
            audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
            assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0);
            audio.sound_virtual(0x1000, slot, &arguments).unwrap();
            assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0);
        }
        audio.set_velocity(0x1000, [1.0, 0.0, 0.0]).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0x2000);
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0);
        audio.sound_virtual(0x1000, 0x6c, &[0]).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x198).unwrap(), 0x218);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn source_open_frequency_conversion_only_applies_to_first_shared_source() {
        let audio = RuntimeAudio::default();
        for handle in 1..=4 {
            audio.create_sound_at(handle).unwrap();
        }
        audio
            .load_sound_with_optional(1, "a.wav", 8, || Ok(Some(wav(&[1; 4], 1, 4))))
            .unwrap();
        audio
            .load_sound_with_optional(2, "A.WAV", 0, || {
                panic!("shared source must not read again")
            })
            .unwrap();
        assert_eq!(audio.read_sound_word(1, 0x198).unwrap(), 0x218);
        assert_eq!(audio.read_sound_word(2, 0x198).unwrap(), 0x218);
        audio
            .load_sound_with_optional(3, "b.wav", 0, || Ok(Some(wav(&[1; 4], 1, 4))))
            .unwrap();
        audio
            .load_sound_with_optional(4, "B.WAV", 8, || {
                panic!("later request cannot upgrade source")
            })
            .unwrap();
        assert_eq!(audio.read_sound_word(3, 0x198).unwrap(), 0x200);
        assert_eq!(audio.read_sound_word(4, 0x198).unwrap(), 0x200);
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
        audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
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
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
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
    fn positioned_sector_sound_resumes_at_elapsed_position() {
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

    fn playing(audio: &RuntimeAudio, real: bool) -> u32 {
        audio
            .sound_virtual(0x1000, 0x54, &[u32::from(real)])
            .unwrap()
    }

    #[test]
    fn replacement_failure_releases_old_voice_source_and_projection() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        audio
            .bind_source_projection(
                "synthetic.wav",
                SourceProjection {
                    source: 8,
                    name: 16,
                },
            )
            .unwrap();
        assert!(audio
            .load_sound_with(0x1000, "missing.wav", || {
                assert!(audio.sound(0x1000).unwrap().source.is_none());
                assert_eq!(playing(&audio, true), 0);
                Err(AudioError("synthetic missing file".into()))
            })
            .is_err());
        assert_eq!(playing(&audio, false), 0);
        assert_eq!(audio.live_source_count().unwrap(), 0);
        assert_eq!(
            audio.take_unused_source_projections().unwrap(),
            vec![SourceProjection {
                source: 8,
                name: 16
            }]
        );
        assert!(audio.sound(0x1000).unwrap().on);
        audio
            .load_sound_with(0x1000, "replacement.wav", || Ok(wav(&[8192; 4], 1, 4)))
            .unwrap();
        // Open resets snd_flags after its internal SetOn, including pending Play.
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0x21ff);
        audio.mix_stereo(4, &mut [0.0; 2]).unwrap();
        assert_eq!(playing(&audio, true), 0);
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        assert_eq!(playing(&audio, true), 1);
    }

    #[test]
    fn exact_name_preserves_voice_but_case_cache_match_reopens() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        audio.mix_stereo(4, &mut [0.0; 2]).unwrap();
        audio
            .load_sound_with(0x1000, "synthetic.wav", || {
                panic!("exact Open must not read")
            })
            .unwrap();
        assert_eq!(audio.sound(0x1000).unwrap().current_time_ms(), 250);
        // A second owner keeps the case-insensitive source cache alive across release.
        audio.create_sound_at(0x2000).unwrap();
        audio
            .load_sound_with(0x2000, "SYNTHETIC.WAV", || {
                panic!("shared source must not read")
            })
            .unwrap();
        let source = audio.sound(0x2000).unwrap().source.unwrap();
        audio
            .load_sound_with(0x1000, "SYNTHETIC.WAV", || {
                panic!("cache match must not read")
            })
            .unwrap();
        let reopened = audio.sound(0x1000).unwrap();
        assert!(Arc::ptr_eq(&source, reopened.source.as_ref().unwrap()));
        assert_eq!(reopened.current_time_ms(), 0);
        assert!(!reopened.voice_playing);
        assert!(reopened.is_playing());
    }

    #[test]
    fn deferred_play_real_flag_stop_restart_and_loop_restart() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x24, &[1]).unwrap();
        assert_eq!(playing(&audio, false), 1);
        assert_eq!(playing(&audio, true), 0);
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(playing(&audio, true), 1);
        audio.mix_stereo(4, &mut [0.0; 4]).unwrap();
        audio.sound_virtual(0x1000, 0x6c, &[1]).unwrap();
        let mut output = [0.0; 4];
        audio.mix_stereo(4, &mut output).unwrap();
        assert_eq!(output, [0.0, 0.0, 0.25, 0.25]);
        audio.sound_virtual(0x1000, 0x24, &[0]).unwrap();
        assert_eq!((playing(&audio, false), playing(&audio, true)), (0, 0));
        assert_eq!(audio.read_sound_word(0x1000, 0x144).unwrap(), 1);
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        assert_eq!(audio.sound(0x1000).unwrap().current_time_ms(), 0);
    }

    #[test]
    fn no_camera_waits_and_sector_none_is_not_an_unrestricted_camera() {
        let audio = loaded();
        audio.clear_listener().unwrap();
        audio.sound_virtual(0x1000, 0x24, &[1]).unwrap();
        assert_eq!(audio.sound(0x1000).unwrap().state, PlaybackState::NoCamera);
        audio.mix_stereo(4, &mut [0.0; 4]).unwrap();
        assert_eq!(playing(&audio, true), 0);
        audio.add_sector_sound(7, 0x1000).unwrap();
        audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(audio.sound(0x1000).unwrap().state, PlaybackState::Pause);
        assert_eq!(playing(&audio, true), 0);
        audio
            .set_listener([0.0; 3], [1.0, 0.0, 0.0], Some(7))
            .unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(playing(&audio, true), 1);
    }

    #[test]
    fn paused_seek_keeps_voice_cursor_and_expires_by_logical_clock() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        audio.mix_stereo(4, &mut [0.0; 2]).unwrap();
        audio.add_sector_sound(7, 0x1000).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!((playing(&audio, false), playing(&audio, true)), (1, 0));
        audio.sound_virtual(0x1000, 0x70, &[500]).unwrap();
        assert_eq!(audio.sound_virtual(0x1000, 0x80, &[]).unwrap(), 250);
        audio.mix_stereo(4, &mut [0.0; 4]).unwrap();
        assert_eq!(playing(&audio, false), 0);
        audio
            .set_listener([0.0; 3], [1.0, 0.0, 0.0], Some(7))
            .unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(audio.sound(0x1000).unwrap().state, PlaybackState::Stop);
    }

    #[test]
    fn range_pause_freezes_voice_then_loop_resume_uses_elapsed_time() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x58, &[1]).unwrap();
        audio.sound_virtual(0x1000, 0x6c, &[1]).unwrap();
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        audio.mix_stereo(4, &mut [0.0; 2]).unwrap();
        audio
            .set_position(0x1000, [20.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            .unwrap();
        audio.mix_stereo(4, &mut [0.0; 4]).unwrap();
        assert_eq!(audio.sound(0x1000).unwrap().current_time_ms(), 250);
        assert_eq!((playing(&audio, false), playing(&audio, true)), (1, 0));
        audio
            .set_position(0x1000, [0.0; 3], [0.0, 0.0, 1.0])
            .unwrap();
        let mut output = [0.0; 2];
        audio.mix_stereo(4, &mut output).unwrap();
        assert_eq!(output, [-0.25, -0.25]);
        assert_eq!(audio.sound(0x1000).unwrap().current_time_ms(), 0);
    }

    #[test]
    fn world_scale_controls_range_and_derived_cone_fields_follow_setters() {
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x58, &[1]).unwrap();
        audio
            .set_position(0x1000, [15.0, 0.0, 0.0], [0.0, 0.0, 1.0])
            .unwrap();
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        assert_eq!(playing(&audio, true), 0);
        audio.set_world_scale(0x1000, 2.0).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(playing(&audio, true), 1);
        assert_eq!(
            f32::from_bits(audio.read_sound_word(0x1000, 0x174).unwrap()),
            20.0
        );
        audio
            .sound_virtual(0x1000, 0x60, &[1.0f32.to_bits(), 7.0f32.to_bits()])
            .unwrap();
        assert_eq!(
            f32::from_bits(audio.read_sound_word(0x1000, 0x1a4).unwrap()),
            0.5f32.cos()
        );
        assert_eq!(
            f32::from_bits(audio.read_sound_word(0x1000, 0x1a8).unwrap()),
            std::f32::consts::PI
        );
        assert_eq!(
            f32::from_bits(audio.read_sound_word(0x1000, 0x1ac).unwrap()),
            std::f32::consts::FRAC_PI_2.cos()
        );
    }

    #[test]
    fn scaled_ranges_are_cached_until_an_eligible_update() {
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        let cached = || {
            [0x16c, 0x174, 0x178, 0x180]
                .map(|offset| f32::from_bits(audio.read_sound_word(0x1000, offset).unwrap()))
        };
        assert_eq!(cached(), [1.0, 10.0, 100.0, 0.0]);
        audio
            .sound_virtual(
                0x1000,
                0x5c,
                &[2f32.to_bits(), 20f32.to_bits(), 0.5f32.to_bits(), 0],
            )
            .unwrap();
        audio.set_world_scale(0x1000, 3.0).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(cached(), [1.0, 10.0, 100.0, 0.0]);
        audio
            .load_sound_with(0x1000, "synthetic.wav", || {
                Ok(wav(&[0, 8192, 16384, -8192], 1, 4))
            })
            .unwrap();
        audio.sound_virtual(0x1000, 0x58, &[3]).unwrap();
        audio.sound_virtual(0x1000, 0x24, &[1]).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(audio.sound(0x1000).unwrap().state, PlaybackState::NoCamera);
        assert_eq!(cached(), [1.0, 10.0, 100.0, 0.0]);
        audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(cached(), [6.0, 60.0, 3600.0, 33.0]);
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap() & 0x84, 0);
        audio
            .sound_virtual(
                0x1000,
                0x5c,
                &[3f32.to_bits(), 40f32.to_bits(), 0.25f32.to_bits(), 0],
            )
            .unwrap();
        audio.set_world_scale(0x1000, 2.0).unwrap();
        assert_eq!(cached(), [6.0, 60.0, 3600.0, 33.0]);
        audio.clear_listener().unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(cached(), [6.0, 60.0, 3600.0, 33.0]);
        audio.set_listener([0.0; 3], [1.0, 0.0, 0.0], None).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        assert_eq!(cached(), [6.0, 80.0, 6400.0, 24.5]);
        audio.mix_stereo(4, &mut [0.0; 8]).unwrap();
        audio.set_world_scale(0x1000, 4.0).unwrap();
        audio.sound_virtual(0x1000, 0x90, &[]).unwrap();
        // The IsPlaying(false) guard precedes cache refresh too.
        assert_eq!(audio.sound(0x1000).unwrap().state, PlaybackState::Stop);
        assert_eq!(cached(), [6.0, 80.0, 6400.0, 24.5]);
    }

    #[test]
    fn spot_cone_normalizes_zero_and_small_vectors_before_gating() {
        for (listener, direction, audible) in [
            ([0.0; 3], [0.0, 0.0, 1.0], false),
            ([0.0, 0.0, 1e-6], [0.0, 0.0, 1.0], true),
            ([0.0, 0.0, -1e-6], [0.0, 0.0, 1.0], false),
            ([1e-6, 0.0, 1e-6], [1.0, 0.0, 0.0], true),
            ([0.0; 3], [0.0; 3], true),
            ([0.0, 0.0, 1.0], [0.0; 3], false),
        ] {
            let audio = loaded();
            audio.set_position(0x1000, [0.0; 3], direction).unwrap();
            audio.set_listener(listener, [1.0, 0.0, 0.0], None).unwrap();
            audio.sound_virtual(0x1000, 0x58, &[2]).unwrap();
            audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
            assert_eq!(
                playing(&audio, true),
                u32::from(audible),
                "{listener:?} {direction:?}"
            );
        }
        let audio = loaded();
        audio.sound_virtual(0x1000, 0x58, &[2]).unwrap();
        audio
            .sound_virtual(0x1000, 0x64, &[0.25f32.to_bits()])
            .unwrap();
        audio.sound_virtual(0x1000, 0x88, &[1, 1]).unwrap();
        let mut output = [0.0; 4];
        audio.mix_stereo(4, &mut output).unwrap();
        assert_eq!(output, [0.0, 0.0, 0.0625, 0.0625]);
        // Finite input that overflows normalization does not silently become a cone decision.
        audio.set_position(0x1000, [0.0; 3], [f32::MAX; 3]).unwrap();
        assert!(audio.sound_virtual(0x1000, 0x90, &[]).is_err());
    }
    #[test]
    fn pin_ctor_default_words_match_native_immediates() {
        // RVA 0x57650 ctor defaults (mafia-re I3d_sound.cpp:34-70).
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        let s = audio.sound(0x1000).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0x208f);
        assert_eq!(audio.read_sound_word(0x1000, 0x144).unwrap(), 1);
        assert_eq!(audio.read_sound_word(0x1000, 0x164).unwrap(), 0);
        assert_eq!(
            audio.read_sound_word(0x1000, 0x168).unwrap(),
            1f32.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x170).unwrap(),
            10f32.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x178).unwrap(),
            100f32.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x17c).unwrap(),
            1f32.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x184).unwrap(),
            0f32.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x188).unwrap(),
            0f32.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x18c).unwrap(),
            1f32.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x194).unwrap(),
            1f32.to_bits()
        );
        assert_eq!(audio.read_sound_word(0x1000, 0x19c).unwrap(), 0);
        assert_eq!(
            audio.read_sound_word(0x1000, 0x1a0).unwrap(),
            std::f32::consts::FRAC_PI_3.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x1a8).unwrap(),
            std::f32::consts::FRAC_PI_2.to_bits()
        );
        assert_eq!(
            audio.read_sound_word(0x1000, 0x1b4).unwrap(),
            (-1f32).to_bits()
        );
        assert!(!s.on && !s.looping && s.source.is_none() && !s.voice_playing);
        assert_eq!(s.state, PlaybackState::Stop);
    }
    #[test]
    fn pin_dtor_stop_before_unlink_with_residue() {
        // RVA 0x577e0 order: stop effect precedes unlink/release; a stale
        // teardown cannot touch a replacement incarnation.
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        let id = audio.sound_id_from_handle(0x1000).unwrap();
        audio.sound_virtual(0x1000, 0x58, &[3]).unwrap();
        audio.sound_virtual(0x1000, 0x24, &[1]).unwrap();
        audio.sound_virtual(0x1000, 0, &[]).unwrap();
        audio.create_sound_at(0x1000).unwrap();
        assert!(audio.set_sound_volume(&id, 0.25).is_err());
        assert_eq!(audio.live_sound_count().unwrap(), 1);
        let cur = audio.sound_id_from_handle(0x1000).unwrap();
        audio.release_sound(&cur).unwrap();
        assert_eq!(audio.live_sound_count().unwrap(), 0);
        assert_eq!(audio.live_voice_count().unwrap(), 0);
        assert_eq!(audio.live_source_count().unwrap(), 0);
    }
    #[test]
    fn pin_open_tail_flags_and_loop_word() {
        // RVA 0x57940 Open tail + RVA 0x58010 loop-word mirror.
        let audio = RuntimeAudio::default();
        audio.create_sound_at(0x1000).unwrap();
        audio
            .load_sound_with(0x1000, "s.wav", || Ok(wav(&[0, 8192, 16384, -8192], 1, 4)))
            .unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x140).unwrap(), 0x21ff);
        assert_eq!(audio.read_sound_word(0x1000, 0x198).unwrap() & 0x200, 0x200);
        audio.sound_virtual(0x1000, 0x6c, &[1]).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x164).unwrap(), 1);
        assert_eq!(audio.read_sound_word(0x1000, 0x198).unwrap() & 0x100, 0x100);
        audio.sound_virtual(0x1000, 0x6c, &[0]).unwrap();
        assert_eq!(audio.read_sound_word(0x1000, 0x198).unwrap() & 0x100, 0);
    }
}

#[cfg(test)]
#[path = "runtime_audio_source_tests.rs"]
mod source_tests;

// ---------------------------------------------------------------------------
// ISound behavioral contracts (LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//
// CPU-only ports of the ISound family per sibling mafia-re @ ab67b13
// (ls3df/ISound/IS_3Dlistener.cpp, IS_3Dsound.cpp, IS_driver.cpp, IS_sound.cpp,
// IS_source.cpp, ISound.h). No Ghidra bodies were observable for these; every
// entry below is tier contract: reviewed upstream source plus pinning tests.
//
// Working names: all ISound method names are working guesses per
// ISound.h:9-11 unless PS2/Insanity3D-evidenced (ctor/dtor/AddRef/Release and
// the ISND_*/ISNDINIT_* flag names carry Insanity3D evidence; the rest are
// descriptive). Rust spelling is snake_case per repo rule; each function doc
// records its RVA/VA and mafia-re file:line.
//
// Scope: CPU logic only. DirectSound/COM/EAX submits stay behind the narrow
// backend traits defined here; file I/O (dtaOpen/dtaRead/dtaSeek/dtaClose)
// stays caller-side with raw bytes passed in. The WAV walk of source Open
// reuses PcmSource::from_wav plus a documented field adaptation (PcmSource
// drops bit depth/avg/align/data length, which the wave format needs).
// Nothing else from this module is reused: the existing set_sound_*,
// sound_position_ms and mixer helpers store host-PCM state with different
// order and defaults, so they are noted as gaps and left unwired.
//
// Ownership: native `delete this` (Release/dtor hitting zero) is documented
// per function; the caller drops/unlinks the owner exactly once. Shared
// original statics (ISndIsInit, MaxHWChannels, ChannelList,
// TerminatedChannelList, the EAX static cell) are modeled as caller-owned
// banks passed as &mut, which is the thread boundary. Typed state throughout;
// no byte maps. Precision: f32 exact everywhere except the sourced f64 log
// paths (all 866.0 volume/pan conversions use f64::ln like the C library
// log), with host saturation disclosed where x87 fistp would be UB.
//
// Withheld (no claims, no functions): driver InitEAX probe 0x7bc10
// (frozen-ledger retained entry plus a multi-stage DirectSound/EAX device
// probe), the full InitDSound device sequence 0x7be20 (frozen-ledger
// retained; only narrow CPU phases own that entry), the device
// half of sound IsPlaying 0x7d3e0 (only its CPU null-guard is ported, filed
// as a contract entry). Newly ported since that withhold note: listener
// Update 0x7b1b0 and CommitDeferredSettings 0x7b320 (submit wrappers behind
// the param backend), source Lock/Unlock 0x7d940/0x7d980 (CPU save words
// plus buffer submits). Construction-covered, no
// new functions, cited owners: empty listener dtor 0x7b0a0 (covered by
// isound_listener_new field defaults), synthetic scalar-deleting dtors
// 0x7b390/0x7c910 and $E5/$E10 list thunks 0x7c2d0/0x7c320 (covered by the
// owning destroy/bank-drop functions).

/// LS3D_RESULT words used by the ISound contracts (ls3df.h:20-35).
pub const ISOUND_OK: i32 = 0;
pub const ISOUND_NOFILE: i32 = 0x8000_0001u32 as i32;
pub const ISOUND_OUTOFMEM: i32 = 0x8000_0002u32 as i32;
pub const ISOUND_GENERIC: i32 = 0x8000_0005u32 as i32;
pub const ISOUND_INVALIDPARAMS: i32 = 0x8000_0007u32 as i32;
pub const ISOUND_NOTINITIALIZED: i32 = 0x8000_0009u32 as i32;
pub const ISOUND_UNSUPPORTED: i32 = 0x8000_000eu32 as i32;

/// Open/Play flags (ISound.h:61-69, Insanity3D-evidenced names).
pub const ISND_NOVOLUME: u32 = 0x2;
pub const ISND_NOPANNING: u32 = 0x4;
pub const ISND_FREQUENCY: u32 = 0x10;
pub const ISND_SYSMEM: u32 = 0x40;
pub const ISND_LOOP: u32 = 0x100;
pub const ISND_3DBUFFER: u32 = 0x200;
pub const ISND_NOREWIND: u32 = 0x400;

/// Driver create flags (ISound.h:46-57, Insanity3D-evidenced names).
pub const ISNDINIT_NOEAX: u32 = 0x10;
pub const ISNDINIT_NOHWCHANNELS: u32 = 0x20;
pub const ISNDINIT_HEADPHONES: u32 = 0x400;
pub const ISNDINIT_STEREO: u32 = 0x800;
pub const ISNDINIT_QUAD: u32 = 0x1000;
pub const ISNDINIT_SURROUND: u32 = 0x2000;
pub const ISNDINIT_EAX: u32 = 0x100000;
pub const ISNDINIT_EAX30: u32 = 0x200000;

/// Scene volume-dirty mark (I3D.h:721).
pub const SOUNDF_VOLUME: u32 = 0x8;

/// DirectSound volume/pan bounds (DirectX 8.1 SDK dsound.h values, disclosed:
// DSBVOLUME_MIN/MAX, DSBPAN_LEFT/RIGHT/CENTER are SDK constants, not reads).
pub const DSBVOLUME_MIN: i32 = -10000;
pub const DSBVOLUME_MAX: i32 = 0;
pub const DSBPAN_LEFT: i32 = -10000;
pub const DSBPAN_RIGHT: i32 = 10000;
pub const DSBPAN_CENTER: i32 = 0;

/// Engine marginal-zero bit pattern (ls3df.h:49).
pub const MRG_ZERO_BITS: u32 = 0x322b_cc77;
/// Cone radians-to-degrees factor: 180/3.1415926 in f32, NOT the engine PI
/// 3.1415927 (IS_3Dsound.cpp:65-67).
pub const CONE_RAD_TO_DEG: f32 = 57.295784;

/// Integer-exact spelling of IsAbsMrgZero (ls3df.h:57-69): |f| below
/// marginal zero compared as bits. No inline assembly on the host.
fn isound_is_abs_mrg_zero(value: f32) -> bool {
    value.abs().to_bits() < MRG_ZERO_BITS
}

/// DirectSound buffer-desc flag words used by source Open (DirectX 8.1 SDK
/// dsound.h values, disclosed).
pub const DSBCAPS_CTRL3D: u32 = 0x10;
pub const DSBCAPS_CTRLFREQUENCY: u32 = 0x20;
pub const DSBCAPS_CTRLVOLUME: u32 = 0x80;
pub const DSBCAPS_LOCSOFTWARE: u32 = 0x8;
pub const DSBCAPS_LOCDEFER: u32 = 0x40000;

/// EAX 3.0 listener property ids (eax.h order: NONE=0 .. FLAGS=25).
pub const EAX_LISTENER_NONE: u32 = 0;
pub const EAX_LISTENER_ALLPARAMETERS: u32 = 1;
pub const EAX_LISTENER_ENVIRONMENT: u32 = 2;
pub const EAX_LISTENER_ENVIRONMENTSIZE: u32 = 3;
pub const EAX_LISTENER_ENVIRONMENTDIFFUSION: u32 = 4;
pub const EAX_LISTENER_ROOM: u32 = 5;
pub const EAX_LISTENER_ROOMHF: u32 = 6;
pub const EAX_LISTENER_ROOMLF: u32 = 7;
pub const EAX_LISTENER_DECAYTIME: u32 = 8;
pub const EAX_LISTENER_DECAYHFRATIO: u32 = 9;
pub const EAX_LISTENER_DECAYLFRATIO: u32 = 10;
pub const EAX_LISTENER_REFLECTIONS: u32 = 11;
pub const EAX_LISTENER_REFLECTIONSDELAY: u32 = 12;
pub const EAX_LISTENER_REFLECTIONSPAN: u32 = 13;
pub const EAX_LISTENER_REVERB: u32 = 14;
pub const EAX_LISTENER_REVERBDELAY: u32 = 15;
pub const EAX_LISTENER_REVERBPAN: u32 = 16;
pub const EAX_LISTENER_ECHOTIME: u32 = 17;
pub const EAX_LISTENER_ECHODEPTH: u32 = 18;
pub const EAX_LISTENER_MODULATIONTIME: u32 = 19;
pub const EAX_LISTENER_MODULATIONDEPTH: u32 = 20;
pub const EAX_LISTENER_AIRABSORPTIONHF: u32 = 21;
pub const EAX_LISTENER_HFREFERENCE: u32 = 22;
pub const EAX_LISTENER_LFREFERENCE: u32 = 23;
pub const EAX_LISTENER_ROOMROLLOFFFACTOR: u32 = 24;
pub const EAX_LISTENER_FLAGS: u32 = 25;

/// Speaker-configuration codes (DirectX 8.1 SDK DSSPEAKER_* values).
pub const DSSPEAKER_HEADPHONE: u32 = 1;
pub const DSSPEAKER_QUAD: u32 = 3;
pub const DSSPEAKER_STEREO: u32 = 4;
pub const DSSPEAKER_5POINT1: u32 = 6;

/// CPU class of an EAX listener property for SetEAXProperty (IS_3Dlistener.cpp:123-156).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IsoundEaxClass {
    Integer,
    Float,
    Vector,
    None,
}

fn isound_eax_classify(property: u32) -> IsoundEaxClass {
    match property {
        EAX_LISTENER_ENVIRONMENT
        | EAX_LISTENER_ROOM
        | EAX_LISTENER_ROOMHF
        | EAX_LISTENER_ROOMLF
        | EAX_LISTENER_REFLECTIONS
        | EAX_LISTENER_REVERB
        | EAX_LISTENER_FLAGS => IsoundEaxClass::Integer,
        EAX_LISTENER_ENVIRONMENTSIZE
        | EAX_LISTENER_ENVIRONMENTDIFFUSION
        | EAX_LISTENER_DECAYTIME
        | EAX_LISTENER_DECAYHFRATIO
        | EAX_LISTENER_DECAYLFRATIO
        | EAX_LISTENER_REFLECTIONSDELAY
        | EAX_LISTENER_REVERBDELAY
        | EAX_LISTENER_ECHOTIME
        | EAX_LISTENER_ECHODEPTH
        | EAX_LISTENER_MODULATIONTIME
        | EAX_LISTENER_MODULATIONDEPTH
        | EAX_LISTENER_AIRABSORPTIONHF
        | EAX_LISTENER_HFREFERENCE
        | EAX_LISTENER_LFREFERENCE
        | EAX_LISTENER_ROOMROLLOFFFACTOR => IsoundEaxClass::Float,
        EAX_LISTENER_REFLECTIONSPAN | EAX_LISTENER_REVERBPAN => IsoundEaxClass::Vector,
        _ => IsoundEaxClass::None,
    }
}

/// Device side of the EAX listener properties. All words are caller-owned
/// addresses/values; IKsPropertySet submits stay behind this trait.
pub trait IsoundEaxListenerBackend {
    fn set_eax_int(&mut self, property: u32, value: u32);
    fn set_eax_float(&mut self, property: u32, value_bits: u32);
    fn set_eax_vector(&mut self, property: u32, address: u32);
    fn set_eax_raw(&mut self, property: u32, data: u32, size: u32);
    fn get_eax_raw(&mut self, property: u32, data: u32, size: u32, returned: u32);
}

/// Device side of listener creation (GetAllParameters copy).
pub trait IsoundListenerCreateBackend {
    fn copy_device_listener(&mut self, out: &mut IsoundListener) -> bool;
}

/// Device side of the primary buffer format submit.
pub trait IsoundPrimaryBufferBackend {
    fn set_primary_format(&mut self, format: &IsoundPcmFormat) -> bool;
}

/// Device side of driver init (DirectSoundCreate8/EAX COM, cooperative level,
/// primary buffer, 3D listener query). Returns false on any device failure.
pub trait IsoundDriverInitBackend {
    fn init_dsound(
        &mut self,
        hwnd: u32,
        mix_frequency: i32,
        num_channels: i32,
        bits_per_second: i32,
        flags: u32,
    ) -> bool;
}

/// Device side of driver Close (COM releases, module free, stream table).
pub trait IsoundDriverCloseBackend {
    fn note_unreleased_source(&mut self, name: &str);
    fn drop_stream_table(&mut self);
    fn free_stream_module(&mut self);
    fn release_device_listener(&mut self);
    fn release_eax_property_set(&mut self);
    fn release_eax_probe_buffer(&mut self);
    fn release_primary_buffer(&mut self);
    fn release_direct_sound(&mut self);
}

/// Diagnostic side of the driver dtor (dbgPrintf warning only).
pub trait IsoundDriverDtorBackend {
    fn note_destroy_while_initialized(&mut self);
}

/// Device side of the listener parameter submits (Update and
/// CommitDeferredSettings). The DS3D_IMMEDIATE submits stay behind this
/// trait; bool returns are FAILED submits (GENERIC).
pub trait IsoundListenerParamBackend {
    fn submit_listener_parameters(&mut self, listener: &IsoundListener) -> bool;
    fn commit_deferred_settings(&mut self) -> bool;
}

/// Device side of source Open (buffer creation, lock/fill/unlock, release).
pub trait IsoundSourceOpenBackend {
    fn create_source_buffer(
        &mut self,
        desc: &IsoundBufferDesc,
        format: &IsoundWaveFormat,
    ) -> Option<u32>;
    fn submit_source_bytes(&mut self, buffer: u32, size: u32) -> bool;
    fn release_source_buffer(&mut self, buffer: u32);
    fn lock_source_buffer(&mut self, buffer: u32) -> Option<(u32, u32)>;
    fn unlock_source_buffer(&mut self, buffer: u32, ptr: u32, size: u32) -> bool;
}

/// Live per-sound CPU queries (volume, play time, frequency factor, rate words).
pub trait IsoundSoundQuery {
    fn sound_volume(&mut self, sound: u64) -> f32;
    fn sound_play_time_ms(&mut self, sound: u64) -> u32;
    fn sound_freq_mult(&mut self, sound: u64) -> f32;
    fn sound_bytes_per_ms(&mut self, sound: u64) -> f32;
    fn sound_block_align(&mut self, sound: u64) -> u16;
}

/// Shared SetCurrTime cursor (IS_sound.cpp:377-380): the block before
/// ((int)(time*rate)/align - 1)*align, floored at zero. Helper phase of the
/// SetCurrTime port and the UpdateAllTerminatedBuffs resume path, no separate
/// credit.
fn isound_cursor_block_before(time_ms: u32, bytes_per_ms: f32, block_align: u16) -> u32 {
    // Corrupt zero align would divide by zero like the native WORD divide;
    // the host returns the floor (disclosed hardening).
    if block_align == 0 {
        return 0;
    }
    let stepped = (time_ms as f32 * bytes_per_ms) as i32 / i32::from(block_align) - 1;
    if stepped < 0 {
        0
    } else {
        (stepped * i32::from(block_align)) as u32
    }
}

/// Device side of sound voices: buffer submits, clock, and queries.
pub trait IsoundVoiceBackend: IsoundSoundQuery {
    fn clock_ms(&mut self) -> u32;
    fn buffer_playing(&mut self, buffer: u32) -> bool;
    fn buffer_position(&mut self, buffer: u32) -> u32;
    fn set_buffer_position(&mut self, buffer: u32, position: u32);
    fn stop_buffer(&mut self, buffer: u32);
    fn try_play_buffer(
        &mut self,
        buffer: u32,
        priority: u32,
        looping: bool,
        software: bool,
    ) -> bool;
    fn resume_buffer(&mut self, buffer: u32, looping: bool);
    fn set_buffer_volume(&mut self, buffer: u32, db: i32);
    fn set_buffer_pan(&mut self, buffer: u32, pan: i32);
    fn set_buffer_frequency(&mut self, buffer: u32, freq: u32);
    fn duplicate_buffer(&mut self, source_buffer: u32) -> std::result::Result<u32, IsoundDsError>;
    fn release_buffer(&mut self, buffer: u32);
    fn trace_ds_error(&mut self, message: &'static str);
}

/// Device side of 3D buffers. Bool returns are FAILED submits (GENERIC).
pub trait Isound3dBufferBackend {
    fn set_3d_position(&mut self, buffer: u32, pos: [f32; 3]) -> bool;
    fn set_3d_orientation(&mut self, buffer: u32, dir: [f32; 3]) -> bool;
    fn set_3d_velocity(&mut self, buffer: u32, velocity: [f32; 3]) -> bool;
    fn set_3d_min_distance(&mut self, buffer: u32, min: f32) -> bool;
    fn set_3d_max_distance(&mut self, buffer: u32, max: f32) -> bool;
    fn set_3d_cone_angles(&mut self, buffer: u32, inside_deg: u32, outside_deg: u32) -> bool;
    fn set_3d_outside_volume(&mut self, buffer: u32, millibels: i32) -> bool;
    fn query_3d_buffer(&mut self, buffer: u32) -> Option<u32>;
    fn release_3d_buffer(&mut self, buffer: u32);
}

/// Typed DirectSound error for TraceDSErrors (HRESULT numbers stay device-side).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoundDsError {
    AccessDenied,
    Allocated,
    AlreadyInitialized,
    BadFormat,
    BufferLost,
    ControlUnavail,
    Generic,
    InvalidCall,
    InvalidParam,
    NoAggregation,
    NoDriver,
    NoInterface,
    OtherAppHasPrio,
    OutOfMemory,
    PrioLevelNeeded,
    Uninitialized,
    Unsupported,
}

/// 3D listener CPU state (ISND_3Dlistener::ds_data projected to typed words).
#[derive(Clone, Debug, PartialEq)]
pub struct IsoundListener {
    pub ref_count: i32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub orient_front: [f32; 3],
    pub orient_top: [f32; 3],
    pub distance_factor: f32,
    pub doppler_factor: f32,
    pub rolloff_factor: f32,
    /// sizeof(DS3DLISTENER), 64 bytes (DirectX 8.1 SDK layout, disclosed).
    pub native_size: u32,
}

/// Sound CPU state (ISND_sound members ref/lpDSB8/source/flags/freq/volume).
#[derive(Clone, Debug, PartialEq)]
pub struct IsoundSound {
    pub ref_count: i32,
    pub buffer: Option<u32>,
    pub source: Option<u64>,
    pub flags: u32,
    pub freq_mult: f32,
    pub volume: f32,
}

/// 3D sound CPU state (ISND_3Dsound: base plus is3d_init/lpDSB3D8).
#[derive(Clone, Debug, PartialEq)]
pub struct Isound3dSound {
    pub base: IsoundSound,
    pub is_3d_init: bool,
    pub buffer_3d: Option<u32>,
}

/// Source CPU state (ISND_source members projected to typed words).
#[derive(Clone, Debug, PartialEq)]
pub struct IsoundSource {
    pub ref_count: i32,
    pub open_flags: u32,
    pub filename: Option<String>,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub data_bytes: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub bytes_per_ms: f32,
    pub buffer: Option<u32>,
    /// Lock's first area (ISound.h save_lock_ptr1/save_lick_size1, Insanity3D
    /// order and typo): the PC Lock submits for the first area only.
    pub lock_ptr: Option<u32>,
    pub lock_size: u32,
}

/// Driver CPU state (ISND_driver members; globals live in IsoundGlobals).
#[derive(Clone, Debug, PartialEq)]
pub struct IsoundDriver {
    pub ref_count: i32,
    pub hwnd: u32,
    pub has_direct_sound: bool,
    pub has_eax_property_set: bool,
    pub has_eax_probe_buffer: bool,
    pub has_stream_module: bool,
    pub has_stream_table: bool,
    pub has_primary_buffer: bool,
    pub has_device_listener: bool,
    pub volume: f32,
    pub create_flags: u32,
    pub max_freq: u32,
    pub min_freq: u32,
    /// Driver sources list projected to diagnostic names (pointers stay
    /// caller-owned; Close reports each name like the native dbgPrintf loop).
    pub open_source_names: Vec<String>,
}

/// Shared file globals: ISndIsInit and MaxHWChannels (IS_driver.cpp:12-18).
/// Passed as &mut, which is the thread boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsoundGlobals {
    pub is_init: bool,
    pub max_hw_channels: i32,
}

/// Active hardware channel (CHANNEL_STRUCT, ISound.h:248-254).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundChannelEntry {
    pub sound: u64,
    pub buffer: u32,
    pub priority: u32,
    pub is_loop: bool,
    pub start_time: u32,
}

/// Waiting channel (TERMINATED_CHANNEL_STRUCT, ISound.h:256-263).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundTerminatedEntry {
    pub sound: u64,
    pub buffer: u32,
    pub priority: u32,
    pub position: u32,
    pub stop_time: u32,
    pub is_loop: bool,
}

/// Caller-owned bank for ChannelList/TerminatedChannelList (IS_sound.cpp:10-16).
/// Removal is swap-remove, matching C_list::erase moving the last element
/// into the hole (ls3df.h). &mut is the thread boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IsoundChannelBank {
    pub active: Vec<IsoundChannelEntry>,
    pub terminated: Vec<IsoundTerminatedEntry>,
}

/// PCM format words (WAVEFORMATEX projection for InitBuffer/source Open).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundWaveFormat {
    pub format_tag: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
}

/// Primary-buffer format words (WAVEFORMATEX with cbSize 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundPcmFormat {
    pub bits_per_sample: u16,
    pub channels: u16,
    pub samples_per_sec: u32,
    pub format_tag: u16,
    pub block_align: u16,
    pub avg_bytes_per_sec: u32,
}

/// Source buffer description words (DSBUFFERDESC projection for source Open).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundBufferDesc {
    pub control_3d: bool,
    pub defer_location: bool,
    pub software_location: bool,
    pub buffer_bytes: u32,
    pub no_virtualization: bool,
}

/// S_wave_format projection (ISound.h:72-77).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundSourceFormat {
    pub num_channels: u32,
    pub bytes_per_sample: u32,
    pub samples_per_sec: u32,
    pub size: u32,
}

/// ISND_INIT record projection (ISound.h:37-43).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundInitRecord {
    pub hwnd: u32,
    pub mix_frequency: i32,
    pub num_channels: i32,
    pub bits_per_second: i32,
    pub flags: u32,
}

/// Scene volume-mark cell (I3D_sound entry projected to the Play/Stop query:
/// logical IsPlaying(false) plus the snd_flags word). The native iteration
/// runs over pI3DDriver->sounds; mapping scene sounds into this bank stays
/// caller-side and is NOT rewired to the existing mixer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoundVolumeMark {
    pub playing_logical: bool,
    pub snd_flags: u32,
}

/// Speaker selection (InitDSound speaker-config branch, IS_driver.cpp:365-378).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoundSpeakerConfig {
    Headphone,
    Quad,
    Surround51,
    Stereo,
}

impl IsoundSpeakerConfig {
    pub fn code(self) -> u32 {
        match self {
            Self::Headphone => DSSPEAKER_HEADPHONE,
            Self::Quad => DSSPEAKER_QUAD,
            Self::Surround51 => DSSPEAKER_5POINT1,
            Self::Stereo => DSSPEAKER_STEREO,
        }
    }
}

/// LS3DF RVA 0x7b060, VA 0x1007b060. ISND_3Dlistener::ISND_3Dlistener working
/// name (mafia-re ls3df/ISound/IS_3Dlistener.cpp:8): ref 1, zeroed data,
/// distance/doppler/rolloff factors 1.0, size word set.
pub fn isound_listener_new() -> IsoundListener {
    IsoundListener {
        ref_count: 1,
        position: [0.0; 3],
        velocity: [0.0; 3],
        orient_front: [0.0; 3],
        orient_top: [0.0; 3],
        distance_factor: 1.0,
        doppler_factor: 1.0,
        rolloff_factor: 1.0,
        native_size: 64,
    }
}

/// LS3DF RVA 0x7b0b0, VA 0x1007b0b0. ISND_3Dlistener::AddRef working name
/// (mafia-re ls3df/ISound/IS_3Dlistener.cpp:26): pre-increment, return new.
pub fn isound_listener_add_ref(listener: &mut IsoundListener) -> i32 {
    listener.ref_count = listener.ref_count.wrapping_add(1);
    listener.ref_count
}

/// LS3DF RVA 0x7b0c0, VA 0x1007b0c0. ISND_3Dlistener::Release working name
/// (mafia-re ls3df/ISound/IS_3Dlistener.cpp:33): pre-decrement; below zero
/// keeps the native warning path (returns 0, no drop); hitting zero returns 0
/// and the caller drops the owner exactly once (native `delete this`).
pub fn isound_listener_release(listener: &mut IsoundListener) -> i32 {
    listener.ref_count = listener.ref_count.wrapping_sub(1);
    if listener.ref_count < 0 {
        return 0;
    }
    if listener.ref_count == 0 {
        return 0;
    }
    listener.ref_count
}

/// LS3DF RVA 0x7b100, VA 0x1007b100. ISND_3Dlistener::SetPos working name
/// (mafia-re ls3df/ISound/IS_3Dlistener.cpp:48): store words, I3D_OK.
pub fn isound_listener_set_pos(listener: &mut IsoundListener, pos: [f32; 3]) -> i32 {
    listener.position = pos;
    ISOUND_OK
}

/// LS3DF RVA 0x7b120, VA 0x1007b120. ISND_3Dlistener::SetDir working name
/// (mafia-re ls3df/ISound/IS_3Dlistener.cpp:58): front/top words, I3D_OK.
pub fn isound_listener_set_dir(listener: &mut IsoundListener, dir: [f32; 3], up: [f32; 3]) -> i32 {
    listener.orient_front = dir;
    listener.orient_top = up;
    ISOUND_OK
}

/// LS3DF RVA 0x7b160, VA 0x1007b160. ISND_3Dlistener::SetVelocity working name
/// (mafia-re ls3df/ISound/IS_3Dlistener.cpp:71): store words, I3D_OK.
pub fn isound_listener_set_velocity(listener: &mut IsoundListener, velocity: [f32; 3]) -> i32 {
    listener.velocity = velocity;
    ISOUND_OK
}

/// LS3DF RVA 0x7b180, VA 0x1007b180. ISND_3Dlistener::SetDistanceFactor
/// working name; folded with I3D_material::SetEnvAlpha so the marker lives in
/// ISound.h:238-245 by the PDB-kept name: store word, I3D_OK.
pub fn isound_listener_set_distance_factor(listener: &mut IsoundListener, factor: f32) -> i32 {
    listener.distance_factor = factor;
    ISOUND_OK
}

/// LS3DF RVA 0x7b190, VA 0x1007b190. ISND_3Dlistener::SetDopplerFactor working
/// name (mafia-re ls3df/ISound/IS_3Dlistener.cpp:88): store word, I3D_OK.
pub fn isound_listener_set_doppler_factor(listener: &mut IsoundListener, factor: f32) -> i32 {
    listener.doppler_factor = factor;
    ISOUND_OK
}

/// LS3DF RVA 0x7b1a0, VA 0x1007b1a0. ISND_3Dlistener::SetRolloffFactor working
/// name (mafia-re ls3df/ISound/IS_3Dlistener.cpp:96): store word, I3D_OK.
pub fn isound_listener_set_rolloff_factor(listener: &mut IsoundListener, factor: f32) -> i32 {
    listener.rolloff_factor = factor;
    ISOUND_OK
}

/// LS3DF RVA 0x7b1e0, VA 0x1007b1e0. ISND_3Dlistener::SetEAXProperty working
/// name (mafia-re ls3df/ISound/IS_3Dlistener.cpp:115): without a property set
/// or EAX30 returns UNSUPPORTED; else the scalar is staged (the original
/// copies it into the shared static cell, here the value moves into the
/// backend call under &mut, which is the thread boundary) and integer, float
/// or vector properties submit with long/float/EAXVECTOR width; other ids
/// submit nothing. Always I3D_OK past the guard.
pub fn isound_listener_set_eax_property(
    eax_present: bool,
    create_flags: u32,
    property: u32,
    value: u32,
    backend: &mut impl IsoundEaxListenerBackend,
) -> i32 {
    if !eax_present || create_flags & ISNDINIT_EAX30 == 0 {
        return ISOUND_UNSUPPORTED;
    }
    match isound_eax_classify(property) {
        IsoundEaxClass::Integer => backend.set_eax_int(property, value),
        IsoundEaxClass::Float => backend.set_eax_float(property, value),
        IsoundEaxClass::Vector => backend.set_eax_vector(property, value),
        IsoundEaxClass::None => {}
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b280, VA 0x1007b280. ISND_3Dlistener::SetEAX3Property working
/// name (mafia-re ls3df/ISound/IS_3Dlistener.cpp:163): guard like
/// SetEAXProperty, then raw submit passthrough; the submit HRESULT is ignored,
/// I3D_OK.
pub fn isound_listener_set_eax3_property(
    eax_present: bool,
    create_flags: u32,
    property: u32,
    data: u32,
    size: u32,
    backend: &mut impl IsoundEaxListenerBackend,
) -> i32 {
    if !eax_present || create_flags & ISNDINIT_EAX30 == 0 {
        return ISOUND_UNSUPPORTED;
    }
    backend.set_eax_raw(property, data, size);
    ISOUND_OK
}

/// LS3DF RVA 0x7b2d0, VA 0x1007b2d0. ISND_3Dlistener::GetEAX3Property working
/// name (mafia-re ls3df/ISound/IS_3Dlistener.cpp:173): guard like
/// SetEAXProperty, then raw get passthrough; the get HRESULT is ignored,
/// I3D_OK.
pub fn isound_listener_get_eax3_property(
    eax_present: bool,
    create_flags: u32,
    property: u32,
    data: u32,
    size: u32,
    returned: u32,
    backend: &mut impl IsoundEaxListenerBackend,
) -> i32 {
    if !eax_present || create_flags & ISNDINIT_EAX30 == 0 {
        return ISOUND_UNSUPPORTED;
    }
    backend.get_eax_raw(property, data, size, returned);
    ISOUND_OK
}

/// LS3DF RVA 0x7c6d0, VA 0x1007c6d0. TraceDSErrors (mafia-re
/// ls3df/ISound/IS_sound.cpp:94): HRESULT switch mapped through the typed
/// error; unknown codes submit nothing (the native switch has no default).
/// The PRIOLEVELNEEDED message keeps the original PRORLEVELNEEDED misspelling
/// (native prints DSERR_PRIORLEVELNEEDED).
pub fn isound_trace_ds_error(error: IsoundDsError) -> &'static str {
    match error {
        IsoundDsError::AccessDenied => "TraceDSErrors() - DSERR_ACCESSDENIED",
        IsoundDsError::Allocated => "TraceDSErrors() - DSERR_ALLOCATED",
        IsoundDsError::AlreadyInitialized => "TraceDSErrors() - DSERR_ALREADYINITIALIZED",
        IsoundDsError::BadFormat => "TraceDSErrors() - DSERR_BADFORMAT",
        IsoundDsError::BufferLost => "TraceDSErrors() - DSERR_BUFFERLOST",
        IsoundDsError::ControlUnavail => "TraceDSErrors() - DSERR_CONTROLUNAVAIL",
        IsoundDsError::Generic => "TraceDSErrors() - DSERR_GENERIC",
        IsoundDsError::InvalidCall => "TraceDSErrors() - DSERR_INVALIDCALL",
        IsoundDsError::InvalidParam => "TraceDSErrors() - DSERR_INVALIDPARAM",
        IsoundDsError::NoAggregation => "TraceDSErrors() - DSERR_NOAGGREGATION",
        IsoundDsError::NoDriver => "TraceDSErrors() - DSERR_NODRIVER",
        IsoundDsError::NoInterface => "TraceDSErrors() - DSERR_NOINTERFACE",
        IsoundDsError::OtherAppHasPrio => "TraceDSErrors() - DSERR_OTHERAPPHASPRIO",
        IsoundDsError::OutOfMemory => "TraceDSErrors() - DSERR_OUTOFMEMORY",
        IsoundDsError::PrioLevelNeeded => "TraceDSErrors() - DSERR_PRIORLEVELNEEDED",
        IsoundDsError::Uninitialized => "TraceDSErrors() - DSERR_UNINITIALIZED",
        IsoundDsError::Unsupported => "TraceDSErrors() - DSERR_UNSUPPORTED",
    }
}

/// LS3DF RVA 0x7b370, VA 0x1007b370. ISND_3Dsound::ISND_3Dsound working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:7): base sound words plus cleared
/// 3D init and null 3D handle.
pub fn isound_3dsound_new() -> Isound3dSound {
    Isound3dSound {
        base: isound_sound_new(),
        is_3d_init: false,
        buffer_3d: None,
    }
}

/// LS3DF RVA 0x7b3b0, VA 0x1007b3b0. ISND_3Dsound::~ISND_3Dsound working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:13): Close effects, then the caller
/// drops the owner exactly once (native `delete this`; the scalar-deleting
/// destructor 0x7b390 is construction-covered by this owner).
pub fn isound_3dsound_destroy(
    sound: &mut Isound3dSound,
    self_id: u64,
    attached: Option<&mut IsoundSource>,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut (impl IsoundVoiceBackend + Isound3dBufferBackend),
) -> i32 {
    isound_3dsound_close(
        sound,
        self_id,
        attached,
        bank,
        max_hw_channels,
        create_flags,
        backend,
    )
}

/// LS3DF RVA 0x7b420, VA 0x1007b420. ISND_3Dsound::Is3DInit working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:20): copy of the init word.
pub fn isound_3dsound_is_3d_init(sound: &Isound3dSound) -> bool {
    sound.is_3d_init
}

/// LS3DF RVA 0x7b430, VA 0x1007b430. ISND_3Dsound::SetPos working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:27): null 3D handle is an OK no-op;
/// a failed position submit is GENERIC.
pub fn isound_3dsound_set_pos(
    sound: &Isound3dSound,
    pos: [f32; 3],
    backend: &mut impl Isound3dBufferBackend,
) -> i32 {
    let Some(handle) = sound.buffer_3d else {
        return ISOUND_OK;
    };
    if !backend.set_3d_position(handle, pos) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b470, VA 0x1007b470. ISND_3Dsound::SetDir working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:36): cone-orientation submit behind
/// the backend; null handle OK, failure GENERIC.
pub fn isound_3dsound_set_dir(
    sound: &Isound3dSound,
    dir: [f32; 3],
    backend: &mut impl Isound3dBufferBackend,
) -> i32 {
    let Some(handle) = sound.buffer_3d else {
        return ISOUND_OK;
    };
    if !backend.set_3d_orientation(handle, dir) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b4b0, VA 0x1007b4b0. ISND_3Dsound::SetVelocity working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:45): null handle OK, failure GENERIC.
pub fn isound_3dsound_set_velocity(
    sound: &Isound3dSound,
    velocity: [f32; 3],
    backend: &mut impl Isound3dBufferBackend,
) -> i32 {
    let Some(handle) = sound.buffer_3d else {
        return ISOUND_OK;
    };
    if !backend.set_3d_velocity(handle, velocity) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b4f0, VA 0x1007b4f0. ISND_3Dsound::SetDistances working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:54): min submit first; either failure
/// is GENERIC and the max submit is skipped after a min failure. Null handle
/// is an OK no-op.
pub fn isound_3dsound_set_distances(
    sound: &Isound3dSound,
    min: f32,
    max: f32,
    backend: &mut impl Isound3dBufferBackend,
) -> i32 {
    let Some(handle) = sound.buffer_3d else {
        return ISOUND_OK;
    };
    if !backend.set_3d_min_distance(handle, min) {
        return ISOUND_GENERIC;
    }
    if !backend.set_3d_max_distance(handle, max) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// Radians-to-DWORD degrees for SetCone (IS_3Dsound.cpp:68-77): f32 multiply
/// by 57.295784 (180/3.1415926, not engine PI), then truncate toward zero.
/// Non-positive inputs map to 0 and infinities saturate host-side; the native
/// (DWORD) cast there is x87 UB, disclosed (tests stay finite non-negative).
pub fn isound_cone_radians_to_dword(angle: f32) -> u32 {
    #[expect(
        clippy::neg_cmp_op_on_partial_ord,
        reason = "ordered negation keeps NaN on early-zero return; <= 0.0 would let NaN fall through to multiply plus float-to-int cast"
    )]
    if !(angle > 0.0) {
        return 0;
    }
    (angle * CONE_RAD_TO_DEG) as u32
}

/// LS3DF RVA 0x7b540, VA 0x1007b540. ISND_3Dsound::SetCone working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:70): both angles convert together
/// into one cone-angles submit; null handle OK, failure GENERIC.
pub fn isound_3dsound_set_cone(
    sound: &Isound3dSound,
    in_angle: f32,
    out_angle: f32,
    backend: &mut impl Isound3dBufferBackend,
) -> i32 {
    let Some(handle) = sound.buffer_3d else {
        return ISOUND_OK;
    };
    if !backend.set_3d_cone_angles(
        handle,
        isound_cone_radians_to_dword(in_angle),
        isound_cone_radians_to_dword(out_angle),
    ) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b590, VA 0x1007b590. ISND_3Dsound::SetOutVolume working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:81): (1-vol)*-10000 in f32, LONG
/// truncation toward zero (host-saturating on nonfinite, disclosed); null
/// handle OK, failure GENERIC.
pub fn isound_3dsound_set_out_volume(
    sound: &Isound3dSound,
    out_volume: f32,
    backend: &mut impl Isound3dBufferBackend,
) -> i32 {
    let Some(handle) = sound.buffer_3d else {
        return ISOUND_OK;
    };
    let millibels = ((1.0f32 - out_volume) * -10000.0f32) as i32;
    if !backend.set_3d_outside_volume(handle, millibels) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b5d0, VA 0x1007b5d0. ISND_3Dsound::Open working name (mafia-re
/// ls3df/ISound/IS_3Dsound.cpp:91): Close first (same-record reopen must be
/// caller-sequenced: close with the record attached, then open detached, so
/// the two &mut records never alias), then base Open; a non-OK base result
/// returns at once. With ISND_3DBUFFER the 3D interface is queried: failure
/// returns GENERIC with the base attachment retained and init still false.
/// Otherwise init is set true.
#[expect(
    clippy::too_many_arguments,
    reason = "ISND_3Dsound::Open port mirrors native open method with explicit Rust owners plus singular voice backend"
)]
pub fn isound_3dsound_open(
    sound: &mut Isound3dSound,
    self_id: u64,
    attached: Option<&mut IsoundSource>,
    fresh: &mut IsoundSource,
    fresh_id: u64,
    flags: u32,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut (impl IsoundVoiceBackend + Isound3dBufferBackend),
) -> i32 {
    isound_3dsound_close(
        sound,
        self_id,
        attached,
        bank,
        max_hw_channels,
        create_flags,
        &mut *backend,
    );
    let result = isound_sound_open(
        &mut sound.base,
        self_id,
        None,
        fresh,
        fresh_id,
        flags,
        bank,
        max_hw_channels,
        create_flags,
        &mut *backend,
    );
    if result != ISOUND_OK {
        return result;
    }
    if flags & ISND_3DBUFFER != 0 {
        let buffer = sound.base.buffer.unwrap_or(0);
        match backend.query_3d_buffer(buffer) {
            Some(handle) => sound.buffer_3d = Some(handle),
            None => return ISOUND_GENERIC,
        }
    }
    sound.is_3d_init = true;
    ISOUND_OK
}

/// LS3DF RVA 0x7b630, VA 0x1007b630. ISND_3Dsound::Close working name
/// (mafia-re ls3df/ISound/IS_3Dsound.cpp:105): init cleared first, 3D handle
/// released, then base Close. Always I3D_OK.
pub fn isound_3dsound_close(
    sound: &mut Isound3dSound,
    self_id: u64,
    attached: Option<&mut IsoundSource>,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut (impl IsoundVoiceBackend + Isound3dBufferBackend),
) -> i32 {
    sound.is_3d_init = false;
    if let Some(handle) = sound.buffer_3d.take() {
        backend.release_3d_buffer(handle);
    }
    isound_sound_close(
        &mut sound.base,
        self_id,
        attached,
        bank,
        max_hw_channels,
        create_flags,
        backend,
    )
}

/// LS3DF RVA 0x7b680, VA 0x1007b680. ISND_sound::GetSource working name kept
/// out of line after the 3D sounds (mafia-re ls3df/ISound/ISound.h:150-155):
/// copy of the attached source identity.
pub fn isound_sound_get_source(sound: &IsoundSound) -> Option<u64> {
    sound.source
}

/// LS3DF RVA 0x7b690, VA 0x1007b690. ISND_driver::ISND_driver working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:22): ref 1, null device words, volume
/// 0.0, no create flags, null window, zero unknowns.
pub fn isound_driver_new() -> IsoundDriver {
    IsoundDriver {
        ref_count: 1,
        hwnd: 0,
        has_direct_sound: false,
        has_eax_property_set: false,
        has_eax_probe_buffer: false,
        has_stream_module: false,
        has_stream_table: false,
        has_primary_buffer: false,
        has_device_listener: false,
        volume: 0.0,
        create_flags: 0,
        max_freq: 0,
        min_freq: 0,
        open_source_names: Vec::new(),
    }
}

/// Fresh shared globals (ISndIsInit false, no hardware channels).
pub fn isound_globals_new() -> IsoundGlobals {
    IsoundGlobals {
        is_init: false,
        max_hw_channels: 0,
    }
}

/// LS3DF RVA 0x7b6e0, VA 0x1007b6e0. ISND_driver::~ISND_driver working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:42): warns when still initialized;
/// the caller drops the owner exactly once afterwards.
pub fn isound_driver_destroy(globals: &IsoundGlobals, backend: &mut impl IsoundDriverDtorBackend) {
    if globals.is_init {
        backend.note_destroy_while_initialized();
    }
}

/// LS3DF RVA 0x7b730, VA 0x1007b730. ISND_driver::IsInit working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:50): copy of the shared init word.
pub fn isound_driver_is_init(globals: &IsoundGlobals) -> bool {
    globals.is_init
}

/// LS3DF RVA 0x7b740, VA 0x1007b740. ISND_driver::Close working name (mafia-re
/// ls3df/ISound/IS_driver.cpp:57): unreleased sources are reported by name in
/// list order, then device words release in native order (stream table,
/// module, listener, EAX property set, probe buffer, primary, DirectSound).
/// The shared init word clears unconditionally.
pub fn isound_driver_close(
    driver: &mut IsoundDriver,
    globals: &mut IsoundGlobals,
    backend: &mut impl IsoundDriverCloseBackend,
) {
    for name in std::mem::take(&mut driver.open_source_names) {
        backend.note_unreleased_source(&name);
    }
    if driver.has_direct_sound {
        if driver.has_stream_table {
            backend.drop_stream_table();
            driver.has_stream_table = false;
        }
        if driver.has_stream_module {
            backend.free_stream_module();
            driver.has_stream_module = false;
        }
        if driver.has_device_listener {
            backend.release_device_listener();
            driver.has_device_listener = false;
        }
        if driver.has_eax_property_set {
            backend.release_eax_property_set();
            driver.has_eax_property_set = false;
        }
        if driver.has_eax_probe_buffer {
            backend.release_eax_probe_buffer();
            driver.has_eax_probe_buffer = false;
        }
        if driver.has_primary_buffer {
            backend.release_primary_buffer();
            driver.has_primary_buffer = false;
        }
        backend.release_direct_sound();
        driver.has_direct_sound = false;
    }
    globals.is_init = false;
}

/// LS3DF RVA 0x7b810, VA 0x1007b810. ISND_driver::InitBuffer working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:97): PCM format words with
/// BlockAlign = bps/8*channels (integer division truncates before the
/// multiply) and AvgBytes = BlockAlign*rate; a failed primary-format submit
/// is INVALIDPARAMS, else I3D_OK.
pub fn isound_driver_init_buffer(
    mix_frequency: i32,
    num_channels: i32,
    bits_per_second: i32,
    backend: &mut impl IsoundPrimaryBufferBackend,
) -> i32 {
    let block_align = (bits_per_second as u32 / 8).wrapping_mul(num_channels as u32);
    let format = IsoundPcmFormat {
        bits_per_sample: bits_per_second as u16,
        channels: num_channels as u16,
        samples_per_sec: mix_frequency as u32,
        format_tag: 1,
        block_align: block_align as u16,
        avg_bytes_per_sec: block_align.wrapping_mul(mix_frequency as u32),
    };
    if !backend.set_primary_format(&format) {
        return ISOUND_INVALIDPARAMS;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b8a0, VA 0x1007b8a0. ISND_driver::SetVolume working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:113): when initialized and the new
/// volume differs past marginal zero, store max(min(vol,1),0) in that
/// max-then-min order (NaN folds to 1.0 through the windows.h macro order)
/// and mark every logically playing scene cell with SOUNDF_VOLUME. Always
/// I3D_OK.
#[expect(
    clippy::manual_clamp,
    reason = "min-then-max folds NaN to 1.0; clamp would return NaN"
)]
pub fn isound_driver_set_volume(
    driver: &mut IsoundDriver,
    globals: &IsoundGlobals,
    sounds: &mut [IsoundVolumeMark],
    volume: f32,
) -> i32 {
    if globals.is_init && !isound_is_abs_mrg_zero(volume - driver.volume) {
        driver.volume = volume.min(1.0).max(0.0);
        for sound in sounds.iter_mut() {
            if sound.playing_logical {
                sound.snd_flags |= SOUNDF_VOLUME;
            }
        }
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b950, VA 0x1007b950. ISND_driver::GetCaps working name; folded
/// so the marker lives in ISound.h:350-355 by the PDB-kept decorated name:
/// returns I3D_OK and ignores its output word (even null).
pub fn isound_driver_get_caps() -> i32 {
    ISOUND_OK
}

/// LS3DF RVA 0x7b960, VA 0x1007b960. ISND_driver::CreateSource working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:135): operator-new, caller-owned.
/// The native OUTOFMEM arm (null new) has no host fault path; allocation
/// failure surfaces as a host allocation error instead, disclosed.
pub fn isound_driver_create_source() -> IsoundSource {
    isound_source_new()
}

/// LS3DF RVA 0x7b9c0, VA 0x1007b9c0. ISND_driver::CreateSource out-variant
/// working name (mafia-re ls3df/ISound/IS_driver.cpp:142): null slot is
/// INVALIDPARAMS, else construct into the slot, I3D_OK.
pub fn isound_driver_create_source_out(slot: Option<&mut IsoundSource>) -> i32 {
    let Some(target) = slot else {
        return ISOUND_INVALIDPARAMS;
    };
    *target = isound_source_new();
    ISOUND_OK
}

/// LS3DF RVA 0x7ba00, VA 0x1007ba00. ISND_driver::CreateSound working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:152): operator-new, caller-owned.
pub fn isound_driver_create_sound() -> IsoundSound {
    isound_sound_new()
}

/// LS3DF RVA 0x7ba60, VA 0x1007ba60. ISND_driver::CreateSound out-variant
/// working name (mafia-re ls3df/ISound/IS_driver.cpp:159): null slot is
/// INVALIDPARAMS, else construct, I3D_OK.
pub fn isound_driver_create_sound_out(slot: Option<&mut IsoundSound>) -> i32 {
    let Some(target) = slot else {
        return ISOUND_INVALIDPARAMS;
    };
    *target = isound_sound_new();
    ISOUND_OK
}

/// LS3DF RVA 0x7baa0, VA 0x1007baa0. ISND_driver::Create3DSound working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:169): operator-new, caller-owned.
pub fn isound_driver_create_3dsound() -> Isound3dSound {
    isound_3dsound_new()
}

/// LS3DF RVA 0x7bb00, VA 0x1007bb00. ISND_driver::Create3DSound out-variant
/// working name (mafia-re ls3df/ISound/IS_driver.cpp:176): null slot is
/// INVALIDPARAMS, else construct, I3D_OK.
pub fn isound_driver_create_3dsound_out(slot: Option<&mut Isound3dSound>) -> i32 {
    let Some(target) = slot else {
        return ISOUND_INVALIDPARAMS;
    };
    *target = isound_3dsound_new();
    ISOUND_OK
}

/// LS3DF RVA 0x7bb40, VA 0x1007bb40. ISND_driver::CreateListener working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:186): operator-new, caller-owned; the
/// device listener refresh applies when the backend holds one (defaults kept
/// otherwise). The native null-new arm has no host fault path, disclosed.
pub fn isound_driver_create_listener(
    backend: &mut impl IsoundListenerCreateBackend,
) -> IsoundListener {
    let mut listener = isound_listener_new();
    backend.copy_device_listener(&mut listener);
    listener
}

/// LS3DF RVA 0x7bbd0, VA 0x1007bbd0. ISND_driver::CreateListener out-variant
/// working name (mafia-re ls3df/ISound/IS_driver.cpp:198): null slot is
/// INVALIDPARAMS, else create (with refresh) into the slot, I3D_OK.
pub fn isound_driver_create_listener_out(
    slot: Option<&mut IsoundListener>,
    backend: &mut impl IsoundListenerCreateBackend,
) -> i32 {
    let Some(target) = slot else {
        return ISOUND_INVALIDPARAMS;
    };
    *target = isound_driver_create_listener(backend);
    ISOUND_OK
}

/// Narrow CPU helper for InitDSound (mafia-re ls3df/ISound/IS_driver.cpp:296-
/// 306): frequency-bound repair. A card claiming equal, over-10kHz minimum,
/// zero minimum, or under-40kHz maximum bounds gets minimum 10000; then
/// minimum under 8000 becomes 8000 and maximum under 44000 becomes 44000.
/// The InitEAX probe and all device submits are withheld.
pub fn isound_driver_repair_freq_bounds(min_freq: u32, max_freq: u32) -> (u32, u32) {
    let mut min = min_freq;
    let mut max = max_freq;
    if min == max || min > 10000 || min == 0 || max < 40000 {
        min = 10000;
    }
    if min < 8000 {
        min = 8000;
    }
    if max < 44000 {
        max = 44000;
    }
    (min, max)
}

/// Narrow CPU helper for InitDSound (mafia-re ls3df/ISound/IS_driver.cpp:365-
/// 378): speaker selection priority HEADPHONES > QUAD > SURROUND > STEREO;
/// no flag keeps the current Windows configuration (None). The SetSpeaker-
/// Config submit itself is device-side and withheld.
pub fn isound_driver_speaker_config(flags: u32) -> Option<IsoundSpeakerConfig> {
    if flags & ISNDINIT_HEADPHONES != 0 {
        Some(IsoundSpeakerConfig::Headphone)
    } else if flags & ISNDINIT_QUAD != 0 {
        Some(IsoundSpeakerConfig::Quad)
    } else if flags & ISNDINIT_SURROUND != 0 {
        Some(IsoundSpeakerConfig::Surround51)
    } else if flags & ISNDINIT_STEREO != 0 {
        Some(IsoundSpeakerConfig::Stereo)
    } else {
        None
    }
}

/// LS3DF RVA 0x7c1f0, VA 0x1007c1f0. ISND_driver::Init mark working name
/// (mafia-re ls3df/ISound/IS_driver.cpp:384): a failed device init returns
/// GENERIC with no state change; else the window stores, the shared init word
/// sets, driver volume becomes 1.0, I3D_OK.
#[expect(
    clippy::too_many_arguments,
    reason = "ISND_driver::Init port mirrors native device-word phase with explicit Rust driver owners plus singular backend"
)]
pub fn isound_driver_init_mark(
    driver: &mut IsoundDriver,
    globals: &mut IsoundGlobals,
    hwnd: u32,
    mix_frequency: i32,
    num_channels: i32,
    bits_per_second: i32,
    flags: u32,
    backend: &mut impl IsoundDriverInitBackend,
) -> i32 {
    if !backend.init_dsound(hwnd, mix_frequency, num_channels, bits_per_second, flags) {
        return ISOUND_GENERIC;
    }
    driver.hwnd = hwnd;
    globals.is_init = true;
    driver.volume = 1.0;
    ISOUND_OK
}

/// LS3DF RVA 0x7c240, VA 0x1007c240. ISND_driver::Init from-record working
/// name (mafia-re ls3df/ISound/IS_driver.cpp:396): null record is
/// INVALIDPARAMS, else delegates field by field to the mark init.
pub fn isound_driver_init_from_record(
    driver: &mut IsoundDriver,
    globals: &mut IsoundGlobals,
    record: Option<&IsoundInitRecord>,
    backend: &mut impl IsoundDriverInitBackend,
) -> i32 {
    let Some(init) = record else {
        return ISOUND_INVALIDPARAMS;
    };
    isound_driver_init_mark(
        driver,
        globals,
        init.hwnd,
        init.mix_frequency,
        init.num_channels,
        init.bits_per_second,
        init.flags,
        backend,
    )
}

/// Inner best-scan phase of UpdateAllTerminatedBuffs (mafia-re
/// ls3df/ISound/IS_sound.cpp:60-67): highest priority wins, ties break to the
/// louder sound reread live per candidate; the scan runs from the end so the
/// last of equal pairs keeps the slot. Descriptive helper phase, no separate
/// original-function credit (belongs to RVA 0x7c340).
pub fn isound_pick_best_terminated(
    entries: &[IsoundTerminatedEntry],
    get_volume: &mut impl FnMut(u64) -> f32,
) -> Option<usize> {
    if entries.is_empty() {
        return None;
    }
    let mut best = entries.len() - 1;
    let mut index = entries.len() - 1;
    while index > 0 {
        index -= 1;
        if entries[index].priority > entries[best].priority
            || (entries[index].priority == entries[best].priority
                && get_volume(entries[index].sound) > get_volume(entries[best].sound))
        {
            best = index;
        }
    }
    Some(best)
}

/// LS3DF RVA 0x7c340, VA 0x1007c340. UpdateAllTerminatedBuffs (mafia-re
/// ls3df/ISound/IS_sound.cpp:23, PS2-evidenced name): no hardware channels or
/// no EAX returns at once. Ended non-looping actives erase (silent first,
/// then timed-out via stop submit); expired non-looping terminated entries
/// are forgotten except the last; when a channel is free the best terminated
/// entry resumes where it would be now (looping modulo the pitched length)
/// through SetCurrTime-time submit and play submit. All time math wraps like
/// DWORD arithmetic; play-time over frequency is a float division truncated
/// only for the integer positions. A zero pitched length resumes at zero on
/// the host (native modulo by the (uint) cast is x87/fistp UB), disclosed.
pub fn isound_update_all_terminated_buffs(
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    eax_present: bool,
    backend: &mut impl IsoundVoiceBackend,
) {
    if max_hw_channels == 0 || !eax_present {
        return;
    }
    let now = backend.clock_ms();
    let mut index = bank.active.len();
    while index > 0 {
        index -= 1;
        if bank.active[index].is_loop {
            continue;
        }
        if !backend.buffer_playing(bank.active[index].buffer) {
            bank.active.swap_remove(index);
        } else {
            // Active expiry compares raw DWORDs with no frequency division
            // (native `time - start >= GetPlayTime()`); division applies
            // only to the terminated-forget and resume paths below.
            let play_time = backend.sound_play_time_ms(bank.active[index].sound);
            if now.wrapping_sub(bank.active[index].start_time) >= play_time {
                backend.stop_buffer(bank.active[index].buffer);
                bank.active.swap_remove(index);
            }
        }
    }
    if bank.terminated.is_empty() {
        return;
    }
    let mut forget = bank.terminated.len() - 1;
    while forget > 0 {
        forget -= 1;
        if bank.terminated[forget].is_loop {
            continue;
        }
        let full = backend.sound_play_time_ms(bank.terminated[forget].sound) as f32
            / backend.sound_freq_mult(bank.terminated[forget].sound);
        let elapsed = bank.terminated[forget]
            .position
            .wrapping_sub(bank.terminated[forget].stop_time)
            .wrapping_add(now);
        if (elapsed as f32) >= full {
            bank.terminated.swap_remove(forget);
        }
    }
    if (max_hw_channels as usize) > bank.active.len() {
        let Some(best) =
            isound_pick_best_terminated(&bank.terminated, &mut |sound| backend.sound_volume(sound))
        else {
            return;
        };
        let entry = bank.terminated[best];
        let full =
            backend.sound_play_time_ms(entry.sound) as f32 / backend.sound_freq_mult(entry.sound);
        let rate = backend.sound_bytes_per_ms(entry.sound);
        let align = backend.sound_block_align(entry.sound);
        if entry.is_loop {
            let pitched = full as u32;
            let position = if pitched == 0 {
                0
            } else {
                entry
                    .position
                    .wrapping_sub(entry.stop_time)
                    .wrapping_add(now)
                    % pitched
            };
            backend.set_buffer_position(
                entry.buffer,
                isound_cursor_block_before(position, rate, align),
            );
            bank.active.push(IsoundChannelEntry {
                sound: entry.sound,
                buffer: entry.buffer,
                priority: entry.priority,
                is_loop: entry.is_loop,
                start_time: now.wrapping_sub(position),
            });
            backend.resume_buffer(entry.buffer, true);
        } else {
            let position = entry
                .position
                .wrapping_sub(entry.stop_time)
                .wrapping_add(now);
            backend.set_buffer_position(
                entry.buffer,
                isound_cursor_block_before(position, rate, align),
            );
            bank.active.push(IsoundChannelEntry {
                sound: entry.sound,
                buffer: entry.buffer,
                priority: entry.priority,
                is_loop: entry.is_loop,
                start_time: now.wrapping_sub(position),
            });
            backend.resume_buffer(entry.buffer, false);
        }
        bank.terminated.swap_remove(best);
    }
}

/// LS3DF RVA 0x7c8e0, VA 0x1007c8e0. ISND_sound::ISND_sound working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:153): ref 1, null buffer and source,
/// frequency factor and volume 1.0.
pub fn isound_sound_new() -> IsoundSound {
    IsoundSound {
        ref_count: 1,
        buffer: None,
        source: None,
        flags: 0,
        freq_mult: 1.0,
        volume: 1.0,
    }
}

/// LS3DF RVA 0x7c960, VA 0x1007c960. ISND_sound::~ISND_sound working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:162): Close effects, then the caller
/// drops the owner exactly once (the scalar-deleting destructor 0x7c910 is
/// construction-covered by this owner).
pub fn isound_sound_destroy(
    sound: &mut IsoundSound,
    self_id: u64,
    attached: Option<&mut IsoundSource>,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    isound_sound_close(
        sound,
        self_id,
        attached,
        bank,
        max_hw_channels,
        create_flags,
        backend,
    )
}

/// LS3DF RVA 0x7c9a0, VA 0x1007c9a0. ISND_sound::Release working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:169): pre-decrement; below zero keeps
/// the native warning path (0, no drop); zero returns 0 and the caller drops
/// exactly once (native `delete this`).
pub fn isound_sound_release(sound: &mut IsoundSound) -> i32 {
    sound.ref_count = sound.ref_count.wrapping_sub(1);
    if sound.ref_count < 0 {
        return 0;
    }
    if sound.ref_count == 0 {
        return 0;
    }
    sound.ref_count
}

/// LS3DF RVA 0x7c9d0, VA 0x1007c9d0. ISND_sound::Open working name (mafia-re
/// ls3df/ISound/IS_sound.cpp:184): Close first (same-record reopen must be
/// caller-sequenced as documented for 3D Open), then a sourceless record is
/// GENERIC; a failed duplicate traces its error and is GENERIC; else the
/// source attaches (AddRef effect on the fresh record), flags store, I3D_OK.
/// WAV scan leniency (disclosed): the host takes the first 16-byte `fmt ` and
/// first `data` chunk; native assumes `fmt ` first then scans for `data`
/// (a leading junk chunk would desync it, not the host).
#[expect(
    clippy::too_many_arguments,
    reason = "ISND_sound::Open port mirrors native open method with explicit Rust owners plus singular voice backend"
)]
pub fn isound_sound_open(
    sound: &mut IsoundSound,
    self_id: u64,
    attached: Option<&mut IsoundSource>,
    fresh: &mut IsoundSource,
    fresh_id: u64,
    flags: u32,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    isound_sound_close(
        sound,
        self_id,
        attached,
        bank,
        max_hw_channels,
        create_flags,
        &mut *backend,
    );
    let Some(source_buffer) = fresh.buffer else {
        return ISOUND_GENERIC;
    };
    match backend.duplicate_buffer(source_buffer) {
        Ok(handle) => sound.buffer = Some(handle),
        Err(error) => {
            backend.trace_ds_error(isound_trace_ds_error(error));
            return ISOUND_GENERIC;
        }
    }
    sound.source = Some(fresh_id);
    fresh.ref_count = fresh.ref_count.wrapping_add(1);
    sound.flags = flags;
    ISOUND_OK
}

/// LS3DF RVA 0x7ca30, VA 0x1007ca30. ISND_sound::Close working name (mafia-re
/// ls3df/ISound/IS_sound.cpp:202): stop submit first (result ignored), buffer
/// released and cleared, attached record released (its zero-drop stays
/// caller-owned). Always I3D_OK.
pub fn isound_sound_close(
    sound: &mut IsoundSound,
    self_id: u64,
    attached: Option<&mut IsoundSource>,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    // Native Close() calls Stop() first (submit before erase): bufferless
    // sounds skip both (Stop returns GENERIC early); sounding ones stop the
    // buffer, erase channel entries under the EAX gate, then release below.
    if let Some(handle) = sound.buffer {
        backend.stop_buffer(handle);
        isound_erase_sound_channels(
            bank,
            self_id,
            max_hw_channels,
            create_flags,
            sound.flags & ISND_SYSMEM == 0,
        );
    }
    if let Some(handle) = sound.buffer.take() {
        backend.release_buffer(handle);
    }
    if let Some(source) = attached {
        source.ref_count = source.ref_count.wrapping_sub(1);
    }
    sound.source = None;
    ISOUND_OK
}

/// Shared play submit (IS_sound.cpp:320-327): looping or plain play first,
/// software fallback on failure, GENERIC when both fail. Helper phase of the
/// two Play ports, no separate credit.
fn isound_submit_play(
    backend: &mut impl IsoundVoiceBackend,
    buffer: u32,
    priority: u32,
    looping: bool,
) -> i32 {
    if looping {
        if backend.try_play_buffer(buffer, priority, true, false) {
            return ISOUND_OK;
        }
        if backend.try_play_buffer(buffer, 0, true, true) {
            return ISOUND_OK;
        }
    } else {
        if backend.try_play_buffer(buffer, priority, false, false) {
            return ISOUND_OK;
        }
        if backend.try_play_buffer(buffer, 0, false, true) {
            return ISOUND_OK;
        }
    }
    ISOUND_GENERIC
}

/// LS3DF RVA 0x7ca70, VA 0x1007ca70. ISND_sound::Play with source working
/// name (mafia-re ls3df/ISound/IS_sound.cpp:218): Open, then Play(flags, 0)
/// on success; the Open result returns at once otherwise.
#[expect(
    clippy::too_many_arguments,
    reason = "ISND_sound::Play-with-source port chains native open/play methods with explicit Rust owners plus singular backend"
)]
pub fn isound_sound_play_with_source(
    sound: &mut IsoundSound,
    attached: Option<&mut IsoundSource>,
    fresh: &mut IsoundSource,
    fresh_id: u64,
    self_id: u64,
    flags: u32,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    let result = isound_sound_open(
        sound,
        self_id,
        attached,
        fresh,
        fresh_id,
        flags,
        bank,
        max_hw_channels,
        create_flags,
        &mut *backend,
    );
    if result != ISOUND_OK {
        return result;
    }
    isound_sound_play(
        sound,
        bank,
        self_id,
        flags,
        0,
        max_hw_channels,
        create_flags,
        backend,
    )
}

/// LS3DF RVA 0x7caa0, VA 0x1007caa0. ISND_sound::Play working name (mafia-re
/// ls3df/ISound/IS_sound.cpp:228): bufferless is GENERIC; rewind unless
/// NOREWIND. Hardware-managed voices (no NOHWCHANNELS, no SYSMEM) refresh an
/// already-listed channel and jump to submit (EAX voices play unmanaged with
/// zero priority, others with priority; the TERMINATEBY flag bits live in the
/// backend submit); otherwise the voice
/// takes a stopped channel, steals the lowest-priority one (ties to the
/// louder incumbent reread live), or waits on the terminated list with its
/// current position. Submit order (hardware attempt, software fallback) is
/// the shared helper.
#[expect(
    clippy::too_many_arguments,
    reason = "ISND_sound::Play port mirrors native channel/flag phase with explicit Rust owners plus singular voice backend"
)]
pub fn isound_sound_play(
    sound: &mut IsoundSound,
    bank: &mut IsoundChannelBank,
    self_id: u64,
    play_flags: u32,
    priority: u32,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    let Some(handle) = sound.buffer else {
        return ISOUND_GENERIC;
    };
    if play_flags & ISND_NOREWIND == 0 {
        backend.set_buffer_position(handle, 0);
    }
    let hw_managed = create_flags & ISNDINIT_NOHWCHANNELS == 0 && sound.flags & ISND_SYSMEM == 0;
    // DSBPLAY_TERMINATEBY_TIME/PRIORITY flag bits are abstracted into the
    // backend submit (EAX voices play unmanaged with zero priority, others
    // with priority); the bits themselves have no host carrier.
    let effective_priority = if hw_managed && create_flags & ISNDINIT_EAX == 0 {
        priority
    } else {
        0
    };
    if hw_managed && max_hw_channels > 0 {
        let mut listed = bank.active.len();
        while listed > 0 {
            listed -= 1;
            if bank.active[listed].sound == self_id {
                bank.active[listed].priority = priority;
                bank.active[listed].is_loop = play_flags & ISND_LOOP != 0;
                bank.active[listed].start_time = backend.clock_ms();
                return isound_submit_play(
                    backend,
                    handle,
                    effective_priority,
                    play_flags & ISND_LOOP != 0,
                );
            }
        }
        let mut stale = bank.terminated.len();
        while stale > 0 {
            stale -= 1;
            if bank.terminated[stale].sound == self_id {
                bank.terminated.swap_remove(stale);
                break;
            }
        }
        if (max_hw_channels as usize) <= bank.active.len() {
            let mut best: Option<usize> = None;
            let mut lowest = priority;
            let mut scan = bank.active.len();
            while scan > 0 {
                scan -= 1;
                if !backend.buffer_playing(bank.active[scan].buffer) {
                    best = Some(scan);
                    break;
                }
                let incumbent_lower = bank.active[scan].priority < lowest;
                let tie_louder = bank.active[scan].priority == lowest
                    && best.is_some_and(|slot| {
                        backend.sound_volume(bank.active[scan].sound)
                            > backend.sound_volume(bank.active[slot].sound)
                    });
                if incumbent_lower || tie_louder {
                    lowest = bank.active[scan].priority;
                    best = Some(scan);
                }
            }
            let Some(slot) = best else {
                bank.terminated.push(IsoundTerminatedEntry {
                    sound: self_id,
                    buffer: handle,
                    priority,
                    position: backend.buffer_position(handle),
                    stop_time: backend.clock_ms(),
                    is_loop: play_flags & ISND_LOOP != 0,
                });
                return ISOUND_OK;
            };
            let now = backend.clock_ms();
            // Explicit native quirk (IS_sound.cpp Play steal path): the NEW
            // sound's cursor, not the stopped one's, is filed as the
            // displaced entry's resume position.
            let position = backend.buffer_position(handle);
            let displaced = bank.active[slot];
            bank.terminated.push(IsoundTerminatedEntry {
                sound: displaced.sound,
                buffer: displaced.buffer,
                priority: displaced.priority,
                position,
                stop_time: now,
                is_loop: displaced.is_loop,
            });
            backend.stop_buffer(displaced.buffer);
            bank.active[slot] = IsoundChannelEntry {
                sound: self_id,
                buffer: handle,
                priority,
                is_loop: play_flags & ISND_LOOP != 0,
                start_time: now,
            };
        } else {
            bank.active.push(IsoundChannelEntry {
                sound: self_id,
                buffer: handle,
                priority,
                is_loop: play_flags & ISND_LOOP != 0,
                start_time: backend.clock_ms(),
            });
        }
    }
    isound_submit_play(
        backend,
        handle,
        effective_priority,
        play_flags & ISND_LOOP != 0,
    )
}

/// LS3DF RVA 0x7cf50, VA 0x1007cf50. ISND_sound::Stop working name (mafia-re
/// ls3df/ISound/IS_sound.cpp:333): bufferless is GENERIC; stop submit, then
/// under the EAX hardware gate (channels available, EAX, no NOHWCHANNELS, no
/// SYSMEM) the first matching channel and terminated entries erase.
pub fn isound_sound_stop(
    sound: &IsoundSound,
    self_id: u64,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    let Some(handle) = sound.buffer else {
        return ISOUND_GENERIC;
    };
    backend.stop_buffer(handle);
    isound_erase_sound_channels(
        bank,
        self_id,
        max_hw_channels,
        create_flags,
        sound.flags & ISND_SYSMEM == 0,
    );
    ISOUND_OK
}

/// Shared channel-erase half of Stop/Close (IS_sound.cpp Stop gate): first
/// matching active entry, then first matching terminated entry, each
/// swap-removed once. Close reaches it through Stop natively, so both call
/// here with the same gate.
fn isound_erase_sound_channels(
    bank: &mut IsoundChannelBank,
    self_id: u64,
    max_hw_channels: i32,
    create_flags: u32,
    not_sysmem: bool,
) {
    if max_hw_channels > 0
        && create_flags & ISNDINIT_EAX != 0
        && create_flags & ISNDINIT_NOHWCHANNELS == 0
        && not_sysmem
    {
        let mut listed = bank.active.len();
        while listed > 0 {
            listed -= 1;
            if bank.active[listed].sound == self_id {
                bank.active.swap_remove(listed);
                break;
            }
        }
        let mut waiting = bank.terminated.len();
        while waiting > 0 {
            waiting -= 1;
            if bank.terminated[waiting].sound == self_id {
                bank.terminated.swap_remove(waiting);
                break;
            }
        }
    }
}

/// LS3DF RVA 0x7d040, VA 0x1007d040. ISND_sound::GetCurrTime working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:361): bufferless is 0; else the device
/// byte position over bytes-per-ms, truncated. A non-positive rate returns 0
/// on the host (the native float division by zero is UB), disclosed.
pub fn isound_sound_get_curr_time(
    buffer: Option<u32>,
    bytes_per_ms: f32,
    backend: &mut impl IsoundVoiceBackend,
) -> u32 {
    let Some(handle) = buffer else {
        return 0;
    };
    #[expect(
        clippy::neg_cmp_op_on_partial_ord,
        reason = "ordered negation keeps NaN rate on early-zero return without backend query; <= 0.0 would query position then divide by NaN"
    )]
    if !(bytes_per_ms > 0.0) {
        return 0;
    }
    (backend.buffer_position(handle) as f32 / bytes_per_ms) as u32
}

/// LS3DF RVA 0x7d090, VA 0x1007d090. ISND_sound::SetCurrTime working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:372): bufferless is GENERIC; the byte
/// position is the block before ((int)(time*rate)/align - 1)*align, floored
/// at zero (nonfinite/overflowing products saturate on the host where native
/// fistp is UB — disclosed, corrupt-data edge); the device cursor submits,
/// then the listed channel start becomes now minus time under the EAX
/// hardware gate.
#[expect(
    clippy::too_many_arguments,
    reason = "ISND_sound::SetCurrTime port mirrors native cursor/channel phase with explicit Rust owners plus singular backend"
)]
pub fn isound_sound_set_curr_time(
    sound: &IsoundSound,
    self_id: u64,
    time_ms: u32,
    bytes_per_ms: f32,
    block_align: u16,
    bank: &mut IsoundChannelBank,
    max_hw_channels: i32,
    create_flags: u32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    let Some(handle) = sound.buffer else {
        return ISOUND_GENERIC;
    };
    let position = isound_cursor_block_before(time_ms, bytes_per_ms, block_align);
    backend.set_buffer_position(handle, position);
    if max_hw_channels > 0
        && create_flags & ISNDINIT_EAX != 0
        && create_flags & ISNDINIT_NOHWCHANNELS == 0
        && sound.flags & ISND_SYSMEM == 0
    {
        let mut listed = bank.active.len();
        while listed > 0 {
            listed -= 1;
            if bank.active[listed].sound == self_id {
                bank.active[listed].start_time = backend.clock_ms().wrapping_sub(time_ms);
                break;
            }
        }
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7d160, VA 0x1007d160. ISND_sound::SetVolume working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:399): NOVOLUME or bufferless is
/// GENERIC; the unscaled volume stores, then driver volume scales it: below
/// marginal zero (plain compare, so negatives too) submits MIN, at or above
/// 1.0 submits MAX, else (int)(ln*866.0) in f64 with a low clamp only.
/// Nonfinite scaled input returns GENERIC without submit on the host
/// (native fistp would be UB), disclosed.
pub fn isound_sound_set_volume(
    sound: &mut IsoundSound,
    driver_volume: f32,
    volume: f32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    if sound.flags & ISND_NOVOLUME != 0 || sound.buffer.is_none() {
        return ISOUND_GENERIC;
    }
    sound.volume = volume;
    let scaled = volume * driver_volume;
    let db = if scaled < 1e-8f32 {
        DSBVOLUME_MIN
    } else if scaled >= 1.0 {
        DSBVOLUME_MAX
    } else {
        if !scaled.is_finite() {
            return ISOUND_GENERIC;
        }
        let db = (f64::from(scaled).ln() * 866.0) as i32;
        if db < DSBVOLUME_MIN {
            DSBVOLUME_MIN
        } else {
            db
        }
    };
    backend.set_buffer_volume(sound.buffer.unwrap_or(0), db);
    ISOUND_OK
}

/// LS3DF RVA 0x7d1f0, VA 0x1007d1f0. ISND_sound::SetVolumeLOG float working
/// name (mafia-re ls3df/ISound/IS_sound.cpp:421): the volume stores BEFORE
/// the guard; NOVOLUME or bufferless is GENERIC; else (1-vol*driver)*-10000
/// in f32 submits with LONG truncation (host-saturating on nonfinite,
/// disclosed).
pub fn isound_sound_set_volume_log(
    sound: &mut IsoundSound,
    driver_volume: f32,
    volume: f32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    sound.volume = volume;
    if sound.flags & ISND_NOVOLUME != 0 || sound.buffer.is_none() {
        return ISOUND_GENERIC;
    }
    let atten = ((1.0f32 - volume * driver_volume) * -10000.0f32) as i32;
    backend.set_buffer_volume(sound.buffer.unwrap_or(0), atten);
    ISOUND_OK
}

/// LS3DF RVA 0x7d240, VA 0x1007d240. ISND_sound::SetVolumeLOG int working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:433): hundredths of a decibel less the
/// driver attenuation (int)(float)(ln(driver)*-866.0) with the float cast
/// kept (without it VC6 folds the signs); clamped low then high; NOVOLUME or
/// bufferless is GENERIC. Nonfinite driver log saturates on the host
/// (native fistp UB), disclosed.
pub fn isound_sound_set_volume_log_db(
    sound: &IsoundSound,
    driver_volume: f32,
    volume: i32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    if sound.flags & ISND_NOVOLUME != 0 || sound.buffer.is_none() {
        return ISOUND_GENERIC;
    }
    let attenuation = (f64::from(driver_volume).ln() * -866.0) as f32 as i32;
    let mut adjusted = volume.wrapping_sub(attenuation);
    adjusted = adjusted.clamp(DSBVOLUME_MIN, DSBVOLUME_MAX);
    backend.set_buffer_volume(sound.buffer.unwrap_or(0), adjusted);
    ISOUND_OK
}

/// LS3DF RVA 0x7d2b0, VA 0x1007d2b0. ISND_sound::SetFrequency working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:450): without the FREQUENCY flag or a
/// buffer is GENERIC; the rate truncates, clamps high then low against the
/// driver bounds, and the factor (freq/rate) stores before submit. A zero
/// rate returns GENERIC on the host (native float division by zero is UB),
/// disclosed.
pub fn isound_sound_set_frequency(
    sound: &mut IsoundSound,
    samples_per_sec: u32,
    min_freq: u32,
    max_freq: u32,
    frequency: f32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    if sound.flags & ISND_FREQUENCY == 0 || sound.buffer.is_none() {
        return ISOUND_GENERIC;
    }
    if samples_per_sec == 0 {
        return ISOUND_GENERIC;
    }
    let mut freq = (frequency * samples_per_sec as f32) as u32;
    if freq > max_freq {
        freq = max_freq;
    }
    if freq < min_freq {
        freq = min_freq;
    }
    sound.freq_mult = freq as f32 / samples_per_sec as f32;
    backend.set_buffer_frequency(sound.buffer.unwrap_or(0), freq);
    ISOUND_OK
}

/// LS3DF RVA 0x7d340, VA 0x1007d340. ISND_sound::SetPanning working name
/// (mafia-re ls3df/ISound/IS_sound.cpp:466): NOPANNING or bufferless is
/// GENERIC; marginal-zero centers; else (int)(ln*866.0) in f64 clamped high
/// then low. A post-guard nonfinite log (negative, NaN, infinite) returns
/// GENERIC without submit on the host (native submits fistp garbage),
/// disclosed.
pub fn isound_sound_set_panning(
    sound: &IsoundSound,
    panning: f32,
    backend: &mut impl IsoundVoiceBackend,
) -> i32 {
    if sound.flags & ISND_NOPANNING != 0 || sound.buffer.is_none() {
        return ISOUND_GENERIC;
    }
    let pan = if isound_is_abs_mrg_zero(panning) {
        DSBPAN_CENTER
    } else {
        if !panning.is_finite() || panning < 0.0 {
            return ISOUND_GENERIC;
        }
        let pan = (f64::from(panning).ln() * 866.0) as i32;
        pan.clamp(DSBPAN_LEFT, DSBPAN_RIGHT)
    };
    backend.set_buffer_pan(sound.buffer.unwrap_or(0), pan);
    ISOUND_OK
}

/// LS3DF RVA 0x7d3e0, VA 0x1007d3e0. ISND_sound::IsPlaying CPU prefix working
/// name (mafia-re ls3df/ISound/IS_sound.cpp:486): bufferless is false; the
/// playing-bit read is the withheld device half (GetStatus submit stays
/// behind the backend and claims nothing beyond the live bit).
pub fn isound_sound_is_playing_cpu(
    buffer: Option<u32>,
    backend: &mut impl IsoundVoiceBackend,
) -> bool {
    let Some(handle) = buffer else {
        return false;
    };
    backend.buffer_playing(handle)
}

/// LS3DF RVA 0x59370, VA 0x10059370. ISND_source::GetFileName working name
/// kept out of line in I3D code (mafia-re ls3df/ISound/ISound.h:103-110):
/// the filename or the engine empty string when unset.
pub fn isound_source_file_name(source: &IsoundSource) -> &str {
    source.filename.as_deref().unwrap_or("")
}

/// LS3DF RVA 0x7d430, VA 0x1007d430. ISND_source::ISND_source working name
/// (mafia-re ls3df/ISound/IS_source.cpp:26): ref 1, null buffer through the
/// Close arm (format words zeroed, rate zero). Filename and open flags start
/// unset on the host (native leaves them uninitialized until Open), and the
/// lock save words start clear (native leaves them uninitialized until Lock),
/// disclosed.
pub fn isound_source_new() -> IsoundSource {
    IsoundSource {
        ref_count: 1,
        open_flags: 0,
        filename: None,
        channels: 0,
        samples_per_sec: 0,
        data_bytes: 0,
        avg_bytes_per_sec: 0,
        block_align: 0,
        bits_per_sample: 0,
        bytes_per_ms: 0.0,
        buffer: None,
        lock_ptr: None,
        lock_size: 0,
    }
}

/// LS3DF RVA 0x7d480, VA 0x1007d480. ISND_source::~ISND_source working name
/// (mafia-re ls3df/ISound/IS_source.cpp:34): Close effects, then the caller
/// unlinks the record from its source bank exactly once (native
/// sources.remove(this)) and drops it.
pub fn isound_source_destroy(
    source: &mut IsoundSource,
    backend: &mut impl IsoundSourceOpenBackend,
) {
    isound_source_close(source, backend);
}

/// LS3DF RVA 0x7d550, VA 0x1007d550. ISND_source::Release working name
/// (mafia-re ls3df/ISound/IS_source.cpp:42): pre-decrement; below zero keeps
/// the native warning path (0, no drop); zero returns 0 and the caller drops
/// exactly once (native `delete this`).
pub fn isound_source_release(source: &mut IsoundSource) -> i32 {
    source.ref_count = source.ref_count.wrapping_sub(1);
    if source.ref_count < 0 {
        return 0;
    }
    if source.ref_count == 0 {
        return 0;
    }
    source.ref_count
}

/// WAV field adaptation for source Open: PcmSource::from_wav owns the
/// RIFF/WAVE/fmt/data walk and PCM validation, but drops bit depth,
/// byte rate, block align and data length, which the wave format needs. This
/// small scan rereads those header words the way the original dtaRead walk
/// does (first fmt chunk of 16+ bytes, first data chunk size). Helper phase
/// of the Open port, no separate credit.
fn isound_scan_wav_fields(bytes: &[u8]) -> Option<(u16, u16, u32, u32, u16, u16, u32)> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return None;
    }
    let end = (u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize).checked_add(8)?;
    if end > bytes.len() {
        return None;
    }
    let word = |data: &[u8], i: usize| -> Option<u16> {
        Some(u16::from_le_bytes(data.get(i..i + 2)?.try_into().ok()?))
    };
    let dword = |data: &[u8], i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(data.get(i..i + 4)?.try_into().ok()?))
    };
    let mut format: Option<(u16, u16, u32, u32, u16, u16)> = None;
    let mut data_len: Option<u32> = None;
    let mut offset = 12usize;
    while offset < end {
        let header = bytes.get(offset..offset + 8)?;
        let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let start = offset.checked_add(8)?;
        let stop = start.checked_add(length)?;
        let data = bytes.get(start..stop)?;
        if stop > end {
            return None;
        }
        if &header[..4] == b"fmt " && format.is_none() && length >= 16 {
            format = Some((
                word(data, 0)?,
                word(data, 2)?,
                dword(data, 4)?,
                dword(data, 8)?,
                word(data, 12)?,
                word(data, 14)?,
            ));
        } else if &header[..4] == b"data" && data_len.is_none() {
            data_len = Some(length as u32);
        }
        offset = stop.checked_add(length & 1)?;
    }
    let (tag, channels, rate, avg, align, bits) = format?;
    Some((tag, channels, rate, avg, align, bits, data_len?))
}

/// LS3DF RVA 0x7d590, VA 0x1007d590. ISND_source::Open working name (mafia-re
/// ls3df/ISound/IS_source.cpp:57): Close first; an uninitialized driver is
/// NOTINITIALIZED and a missing file (None bytes, the dtaOpen miss) is
/// NOFILE with only the open flags stored. The WAV walk reuses
/// PcmSource::from_wav (unreadable bytes are GENERIC; the native walk would
/// carry garbage formats into a later device failure with the same code,
/// disclosed); header words then fill the wave format, bytes-per-ms is
/// avg*0.001 in f32, stereo clears the 3D-buffer request, and the desc words
/// decode to frequency+volume control, 3D control when requested, deferred
/// placement on hardware voices (both EAX arms merge) else software
/// placement, with no-virtualization on 3D. Device creation, fill and unlock
/// failures are GENERIC; post-creation fill failures keep the buffer and
/// format words like the native fallthrough. Success stores the filename.
pub fn isound_source_open(
    source: &mut IsoundSource,
    name: &str,
    flags: u32,
    file_bytes: Option<&[u8]>,
    driver_is_init: bool,
    no_hw_channels: bool,
    backend: &mut impl IsoundSourceOpenBackend,
) -> i32 {
    isound_source_close(source, &mut *backend);
    if !driver_is_init {
        return ISOUND_NOTINITIALIZED;
    }
    source.open_flags = flags;
    let Some(bytes) = file_bytes else {
        return ISOUND_NOFILE;
    };
    if PcmSource::from_wav(name, bytes).is_err() {
        return ISOUND_GENERIC;
    }
    let Some((tag, channels, rate, avg, align, bits, data_len)) = isound_scan_wav_fields(bytes)
    else {
        return ISOUND_GENERIC;
    };
    source.channels = channels;
    source.samples_per_sec = rate;
    source.data_bytes = data_len;
    source.avg_bytes_per_sec = avg;
    source.block_align = align;
    source.bits_per_sample = bits;
    source.bytes_per_ms = avg as f32 * 0.001f32;
    if channels > 1 {
        source.open_flags &= !ISND_3DBUFFER;
    }
    let control_3d = source.open_flags & ISND_3DBUFFER != 0;
    let hardware_voice = !no_hw_channels && flags & ISND_SYSMEM == 0;
    let desc = IsoundBufferDesc {
        control_3d,
        defer_location: hardware_voice,
        software_location: !hardware_voice,
        buffer_bytes: data_len,
        no_virtualization: control_3d,
    };
    let format = IsoundWaveFormat {
        format_tag: tag,
        channels,
        samples_per_sec: rate,
        avg_bytes_per_sec: avg,
        block_align: align,
        bits_per_sample: bits,
    };
    let Some(handle) = backend.create_source_buffer(&desc, &format) else {
        return ISOUND_GENERIC;
    };
    source.buffer = Some(handle);
    if !backend.submit_source_bytes(handle, data_len) {
        return ISOUND_GENERIC;
    }
    source.filename = Some(name.to_string());
    ISOUND_OK
}

/// LS3DF RVA 0x7d890, VA 0x1007d890. ISND_source::Close working name (mafia-re
/// ls3df/ISound/IS_source.cpp:144): buffer released and cleared, format words
/// zeroed, rate zeroed. Filename, open flags and the lock save words are
/// retained (native keeps them; only a new Lock overwrites the save words).
pub fn isound_source_close(source: &mut IsoundSource, backend: &mut impl IsoundSourceOpenBackend) {
    if let Some(handle) = source.buffer.take() {
        backend.release_source_buffer(handle);
    }
    source.channels = 0;
    source.samples_per_sec = 0;
    source.data_bytes = 0;
    source.avg_bytes_per_sec = 0;
    source.block_align = 0;
    source.bits_per_sample = 0;
    source.bytes_per_ms = 0.0;
}

/// LS3DF RVA 0x7d8c0, VA 0x1007d8c0. ISND_source::IsOpen working name
/// (mafia-re ls3df/ISound/IS_source.cpp:156, Insanity3D-evidenced slot name):
/// data length nonzero.
pub fn isound_source_is_open(source: &IsoundSource) -> bool {
    source.data_bytes > 0
}

/// LS3DF RVA 0x7d8d0, VA 0x1007d8d0. ISND_source::GetPlayTime working name
/// (mafia-re ls3df/ISound/IS_source.cpp:164): milliseconds, empty is 0, else
/// data length over bytes-per-ms truncated. A non-positive rate returns 0 on
/// the host (native float division by zero is UB), disclosed.
pub fn isound_source_get_play_time(source: &IsoundSource) -> u32 {
    #[expect(
        clippy::neg_cmp_op_on_partial_ord,
        reason = "empty data returns zero via first disjunct; negation keeps nonzero data with NaN rate on early-zero return, while <= 0.0 would fall through to NaN division plus cast"
    )]
    if source.data_bytes == 0 || !(source.bytes_per_ms > 0.0) {
        return 0;
    }
    (source.data_bytes as f32 / source.bytes_per_ms) as u32
}

/// LS3DF RVA 0x7d910, VA 0x1007d910. ISND_source::GetFormat working name
/// (mafia-re ls3df/ISound/IS_source.cpp:173): copies the format words out,
/// I3D_OK.
pub fn isound_source_get_format(source: &IsoundSource, out: &mut IsoundSourceFormat) -> i32 {
    *out = IsoundSourceFormat {
        num_channels: u32::from(source.channels),
        bytes_per_sample: u32::from(source.bits_per_sample) / 8,
        samples_per_sec: source.samples_per_sec,
        size: source.data_bytes,
    };
    ISOUND_OK
}

/// LS3DF RVA 0x7b1b0, VA 0x1007b1b0. ISND_3Dlistener::Update working name
/// (mafia-re ls3df/ISound/IS_3Dlistener.cpp:102): hands the stored listener
/// words to the device (DS3D_IMMEDIATE submit behind the backend); a failed
/// submit is GENERIC, else I3D_OK. No null-device guard is invented: the
/// caller owns a live device listener, like the native driver path.
pub fn isound_listener_update(
    listener: &IsoundListener,
    backend: &mut impl IsoundListenerParamBackend,
) -> i32 {
    if !backend.submit_listener_parameters(listener) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7b320, VA 0x1007b320. ISND_3Dlistener::CommitDeferredSettings
/// working name (mafia-re ls3df/ISound/IS_3Dlistener.cpp:182): deferred
/// submit behind the backend; a failed submit is GENERIC, else I3D_OK.
pub fn isound_listener_commit_deferred_settings(
    backend: &mut impl IsoundListenerParamBackend,
) -> i32 {
    if !backend.commit_deferred_settings() {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x7d940, VA 0x1007d940. ISND_source::Lock working name (mafia-re
/// ls3df/ISound/IS_source.cpp:179): the whole-buffer lock submits behind the
/// backend; the first area stages into the save words (native
/// save_lock_ptr1/save_lick_size1) and out carries the pointer, I3D_OK. A
/// bufferless record or a failed submit is GENERIC with the out word and the
/// save words untouched (the native null-buffer path would fault; the host
/// returns GENERIC, disclosed hardening).
pub fn isound_source_lock(
    source: &mut IsoundSource,
    data: &mut u32,
    backend: &mut impl IsoundSourceOpenBackend,
) -> i32 {
    let Some(handle) = source.buffer else {
        return ISOUND_GENERIC;
    };
    let Some((ptr, size)) = backend.lock_source_buffer(handle) else {
        return ISOUND_GENERIC;
    };
    source.lock_ptr = Some(ptr);
    source.lock_size = size;
    *data = ptr;
    ISOUND_OK
}

/// LS3DF RVA 0x7d980, VA 0x1007d980. ISND_source::Unlock working name
/// (mafia-re ls3df/ISound/IS_source.cpp:189): submits the staged first area
/// (native save_lock_ptr1/save_lick_size1) behind the backend; success is
/// I3D_OK, failure GENERIC. A bufferless record or a missing staged area
/// (no prior Lock) is GENERIC with no submit (the native path would carry
/// stale words; the host requires the staged area, disclosed hardening).
pub fn isound_source_unlock(
    source: &IsoundSource,
    backend: &mut impl IsoundSourceOpenBackend,
) -> i32 {
    let Some(handle) = source.buffer else {
        return ISOUND_GENERIC;
    };
    let Some(ptr) = source.lock_ptr else {
        return ISOUND_GENERIC;
    };
    if !backend.unlock_source_buffer(handle, ptr, source.lock_size) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// ISND_source::AddRef working name. Header-inline (mafia-re
/// ls3df/ISound/ISound.h:96-100): no out-of-line FUNCTION address exists,
/// so no RVA is claimed. Pre-increments the count and returns the new
/// value; unlike Release there is no warning arm.
pub fn isound_source_add_ref(source: &mut IsoundSource) -> i32 {
    source.ref_count = source.ref_count.wrapping_add(1);
    source.ref_count
}

/// ISND_sound::GetVolume working name. Header-inline (mafia-re
/// ls3df/ISound/ISound.h:163-166): no out-of-line FUNCTION address exists,
/// so no RVA is claimed. Copies the stored volume word.
pub fn isound_sound_get_volume(sound: &IsoundSound) -> f32 {
    sound.volume
}

/// ISND_sound::GetFrequency working name. Header-inline (mafia-re
/// ls3df/ISound/ISound.h:168-171): no out-of-line FUNCTION address exists,
/// so no RVA is claimed. Copies the stored frequency factor (the
/// SetFrequency multiplier), not a Hertz rate.
pub fn isound_sound_get_frequency(sound: &IsoundSound) -> f32 {
    sound.freq_mult
}

/// Hardware 3D channel count rule working name. Narrow CPU phase of the
/// retained-ledger InitDSound entry (LS3DF RVA 0x7be20 is occupied, so no
/// separate credit is claimed): mafia-re ls3df/ISound/IS_driver.cpp:308-313.
/// Under the NOHWCHANNELS gate the count comes from the device caps word and
/// clamps above 35 down to 30; behind the gate the stored count is untouched,
/// so the host returns None (keep current) instead of inventing a zero.
pub fn isound_init_hw_channels(caps_free_hw_buffers: u32, create_flags: u32) -> Option<i32> {
    if create_flags & ISNDINIT_NOHWCHANNELS != 0 {
        return None;
    }
    let claimed = caps_free_hw_buffers as i32;
    Some(if claimed > 35 { 30 } else { claimed })
}

/// Source-bank unlink working name. CPU phase of the credited source
/// destructor entry (LS3DF RVA 0x7d480): mafia-re
/// ls3df/ISound/IS_source.cpp:37 `pISndDriver->sources.remove(this)`.
/// The caller-owned bank drops the first matching identity by swap-remove,
/// matching the C_list::remove shape used by the in-file banks (ls3df.h);
/// no separate credit is claimed.
pub fn isound_unlink_source(bank: &mut Vec<u64>, self_id: u64) {
    if let Some(slot) = bank.iter().position(|id| *id == self_id) {
        bank.swap_remove(slot);
    }
}

/// Source buffer-description words working name. CPU phase of the credited
/// source Open entry (LS3DF RVA 0x7d590): mafia-re
/// ls3df/ISound/IS_source.cpp:95-111. Frequency and volume control always;
/// 3D control when requested; deferred placement on hardware voices (both
/// native EAX arms merge into LOCDEFER) else software placement; the
/// no-virtualization algorithm on 3D buffers. No separate credit is claimed.
pub fn isound_source_buffer_desc_words(
    want_3d: bool,
    hardware_voice: bool,
    buffer_bytes: u32,
) -> IsoundBufferDesc {
    IsoundBufferDesc {
        control_3d: want_3d,
        defer_location: hardware_voice,
        software_location: !hardware_voice,
        buffer_bytes,
        no_virtualization: want_3d,
    }
}

/// EAX listener value width working name. Width arms of the credited
/// SetEAXProperty entry (LS3DF RVA 0x7b1e0): mafia-re
/// ls3df/ISound/IS_3Dlistener.cpp:123-157. Integer and float properties
/// submit 4 bytes (sizeof(long)/sizeof(float)); vector properties submit 12
/// bytes (sizeof(EAXVECTOR): three floats, eax.h:137-139); other ids submit
/// nothing. No separate credit is claimed.
pub const EAX_VECTOR_BYTES: u32 = 12;

/// EAX listener value width (see EAX_VECTOR_BYTES): byte width of the staged
/// submit for a property id, or None when the id submits nothing. Helper
/// phase of the credited 0x7b1e0 entry; no separate credit is claimed.
pub fn isound_eax_property_width(property: u32) -> Option<u32> {
    match isound_eax_classify(property) {
        IsoundEaxClass::Integer | IsoundEaxClass::Float => Some(4),
        IsoundEaxClass::Vector => Some(EAX_VECTOR_BYTES),
        IsoundEaxClass::None => None,
    }
}

/// Source stereo/3D flag rule working name. CPU phase of the credited source
/// Open entry (LS3DF RVA 0x7d590): mafia-re ls3df/ISound/IS_source.cpp:91-93.
/// DirectSound positions only mono sounds in 3D, so a stereo source clears
/// the 3D-buffer request. No separate credit is claimed.
pub fn isound_source_open_flags_for_channels(open_flags: u32, channels: u16) -> u32 {
    if channels > 1 {
        open_flags & !ISND_3DBUFFER
    } else {
        open_flags
    }
}

/// Hardware voice-management gate working name. Shared CPU gate of Play
/// (mafia-re ls3df/ISound/IS_sound.cpp:235), the Stop/Close channel erase
/// (:338, via the shared erase helper) and SetCurrTime (:383), plus source
/// Open's placement select with the open flags (IS_source.cpp:101): neither
/// NOHWCHANNELS nor SYSMEM means the voice is hardware-managed. Helper
/// phase of those credited entries; no separate credit is claimed.
pub fn isound_hw_voice_managed(create_flags: u32, flags: u32) -> bool {
    create_flags & ISNDINIT_NOHWCHANNELS == 0 && flags & ISND_SYSMEM == 0
}

#[cfg(test)]
mod isound_contract_tests {
    use super::*;
    use std::collections::HashMap;

    struct Rig {
        eax: Vec<String>,
        refresh_to: Option<IsoundListener>,
        refreshed: bool,
        primary: Option<IsoundPcmFormat>,
        primary_ok: bool,
        dsound_ok: bool,
        init_seen: Vec<String>,
        close_log: Vec<String>,
        unreleased: Vec<String>,
        dtor: bool,
        create: Option<u32>,
        created_desc: Option<IsoundBufferDesc>,
        created_format: Option<IsoundWaveFormat>,
        submit_ok: bool,
        released: Vec<u32>,
        clock: u32,
        playing: HashMap<u32, bool>,
        positions: HashMap<u32, u32>,
        set_positions: Vec<(u32, u32)>,
        stopped: Vec<u32>,
        play_calls: Vec<String>,
        play_ok: (bool, bool),
        resumed: Vec<(u32, bool)>,
        volumes_set: Vec<(u32, i32)>,
        pans_set: Vec<(u32, i32)>,
        freqs_set: Vec<(u32, u32)>,
        volumes: HashMap<u64, f32>,
        play_times: HashMap<u64, u32>,
        freq_mults: HashMap<u64, f32>,
        bpms: HashMap<u64, f32>,
        aligns: HashMap<u64, u16>,
        duplicate: std::result::Result<u32, IsoundDsError>,
        traces: Vec<&'static str>,
        ok3d: bool,
        calls3d: Vec<String>,
        query3d: Option<u32>,
        released3d: Vec<u32>,
        params_ok: bool,
        param_calls: Vec<String>,
        lock_area: Option<(u32, u32)>,
        unlock_ok: bool,
        unlocked: Vec<(u32, u32, u32)>,
    }

    impl Default for Rig {
        fn default() -> Self {
            Self {
                eax: Vec::new(),
                refresh_to: None,
                refreshed: false,
                primary: None,
                primary_ok: true,
                dsound_ok: true,
                init_seen: Vec::new(),
                close_log: Vec::new(),
                unreleased: Vec::new(),
                dtor: false,
                create: Some(7),
                created_desc: None,
                created_format: None,
                submit_ok: true,
                released: Vec::new(),
                clock: 0,
                playing: HashMap::new(),
                positions: HashMap::new(),
                set_positions: Vec::new(),
                stopped: Vec::new(),
                play_calls: Vec::new(),
                play_ok: (true, true),
                resumed: Vec::new(),
                volumes_set: Vec::new(),
                pans_set: Vec::new(),
                freqs_set: Vec::new(),
                volumes: HashMap::new(),
                play_times: HashMap::new(),
                freq_mults: HashMap::new(),
                bpms: HashMap::new(),
                aligns: HashMap::new(),
                duplicate: Err(IsoundDsError::Generic),
                traces: Vec::new(),
                ok3d: true,
                calls3d: Vec::new(),
                query3d: Some(9),
                released3d: Vec::new(),
                params_ok: true,
                param_calls: Vec::new(),
                lock_area: Some((0x5000, 128)),
                unlock_ok: true,
                unlocked: Vec::new(),
            }
        }
    }

    impl IsoundEaxListenerBackend for Rig {
        fn set_eax_int(&mut self, property: u32, value: u32) {
            self.eax.push(format!("int:{property}:{value}"));
        }
        fn set_eax_float(&mut self, property: u32, value_bits: u32) {
            self.eax.push(format!("float:{property}:{value_bits}"));
        }
        fn set_eax_vector(&mut self, property: u32, address: u32) {
            self.eax.push(format!("vec:{property}:{address}"));
        }
        fn set_eax_raw(&mut self, property: u32, data: u32, size: u32) {
            self.eax.push(format!("raw:{property}:{data}:{size}"));
        }
        fn get_eax_raw(&mut self, property: u32, data: u32, size: u32, returned: u32) {
            self.eax
                .push(format!("get:{property}:{data}:{size}:{returned}"));
        }
    }

    impl IsoundListenerCreateBackend for Rig {
        fn copy_device_listener(&mut self, out: &mut IsoundListener) -> bool {
            self.refreshed = true;
            if let Some(params) = self.refresh_to.clone() {
                *out = params;
                true
            } else {
                false
            }
        }
    }

    impl IsoundPrimaryBufferBackend for Rig {
        fn set_primary_format(&mut self, format: &IsoundPcmFormat) -> bool {
            self.primary = Some(*format);
            self.primary_ok
        }
    }

    impl IsoundDriverInitBackend for Rig {
        fn init_dsound(
            &mut self,
            hwnd: u32,
            mix_frequency: i32,
            num_channels: i32,
            bits_per_second: i32,
            flags: u32,
        ) -> bool {
            self.init_seen.push(format!(
                "{hwnd}:{mix_frequency}:{num_channels}:{bits_per_second}:{flags}"
            ));
            self.dsound_ok
        }
    }

    impl IsoundDriverCloseBackend for Rig {
        fn note_unreleased_source(&mut self, name: &str) {
            self.unreleased.push(name.to_string());
        }
        fn drop_stream_table(&mut self) {
            self.close_log.push("streams".to_string());
        }
        fn free_stream_module(&mut self) {
            self.close_log.push("module".to_string());
        }
        fn release_device_listener(&mut self) {
            self.close_log.push("listener".to_string());
        }
        fn release_eax_property_set(&mut self) {
            self.close_log.push("eax".to_string());
        }
        fn release_eax_probe_buffer(&mut self) {
            self.close_log.push("probe".to_string());
        }
        fn release_primary_buffer(&mut self) {
            self.close_log.push("primary".to_string());
        }
        fn release_direct_sound(&mut self) {
            self.close_log.push("dsound".to_string());
        }
    }

    impl IsoundDriverDtorBackend for Rig {
        fn note_destroy_while_initialized(&mut self) {
            self.dtor = true;
        }
    }

    impl IsoundSourceOpenBackend for Rig {
        fn create_source_buffer(
            &mut self,
            desc: &IsoundBufferDesc,
            format: &IsoundWaveFormat,
        ) -> Option<u32> {
            self.created_desc = Some(*desc);
            self.created_format = Some(*format);
            self.create
        }
        fn submit_source_bytes(&mut self, _buffer: u32, _size: u32) -> bool {
            self.submit_ok
        }
        fn release_source_buffer(&mut self, buffer: u32) {
            self.released.push(buffer);
        }
        fn lock_source_buffer(&mut self, _buffer: u32) -> Option<(u32, u32)> {
            self.lock_area
        }
        fn unlock_source_buffer(&mut self, buffer: u32, ptr: u32, size: u32) -> bool {
            self.unlocked.push((buffer, ptr, size));
            self.unlock_ok
        }
    }

    impl IsoundListenerParamBackend for Rig {
        fn submit_listener_parameters(&mut self, _listener: &IsoundListener) -> bool {
            self.param_calls.push("params".to_string());
            self.params_ok
        }
        fn commit_deferred_settings(&mut self) -> bool {
            self.param_calls.push("commit".to_string());
            self.params_ok
        }
    }

    impl IsoundSoundQuery for Rig {
        fn sound_volume(&mut self, sound: u64) -> f32 {
            self.volumes.get(&sound).copied().unwrap_or(1.0)
        }
        fn sound_play_time_ms(&mut self, sound: u64) -> u32 {
            self.play_times.get(&sound).copied().unwrap_or(0)
        }
        fn sound_freq_mult(&mut self, sound: u64) -> f32 {
            self.freq_mults.get(&sound).copied().unwrap_or(1.0)
        }
        fn sound_bytes_per_ms(&mut self, sound: u64) -> f32 {
            self.bpms.get(&sound).copied().unwrap_or(1.0)
        }
        fn sound_block_align(&mut self, sound: u64) -> u16 {
            self.aligns.get(&sound).copied().unwrap_or(4)
        }
    }

    impl IsoundVoiceBackend for Rig {
        fn clock_ms(&mut self) -> u32 {
            self.clock
        }
        fn buffer_playing(&mut self, buffer: u32) -> bool {
            self.playing.get(&buffer).copied().unwrap_or(false)
        }
        fn buffer_position(&mut self, buffer: u32) -> u32 {
            self.positions.get(&buffer).copied().unwrap_or(0)
        }
        fn set_buffer_position(&mut self, buffer: u32, position: u32) {
            self.set_positions.push((buffer, position));
        }
        fn stop_buffer(&mut self, buffer: u32) {
            self.stopped.push(buffer);
        }
        fn try_play_buffer(
            &mut self,
            buffer: u32,
            priority: u32,
            looping: bool,
            software: bool,
        ) -> bool {
            let attempt = self.play_calls.len();
            self.play_calls
                .push(format!("{buffer}:{priority}:{looping}:{software}"));
            if attempt == 0 {
                self.play_ok.0
            } else {
                self.play_ok.1
            }
        }
        fn resume_buffer(&mut self, buffer: u32, looping: bool) {
            self.resumed.push((buffer, looping));
        }
        fn set_buffer_volume(&mut self, buffer: u32, db: i32) {
            self.volumes_set.push((buffer, db));
        }
        fn set_buffer_pan(&mut self, buffer: u32, pan: i32) {
            self.pans_set.push((buffer, pan));
        }
        fn set_buffer_frequency(&mut self, buffer: u32, freq: u32) {
            self.freqs_set.push((buffer, freq));
        }
        fn duplicate_buffer(
            &mut self,
            _source_buffer: u32,
        ) -> std::result::Result<u32, IsoundDsError> {
            self.duplicate
        }
        fn release_buffer(&mut self, buffer: u32) {
            self.released.push(buffer);
        }
        fn trace_ds_error(&mut self, message: &'static str) {
            self.traces.push(message);
        }
    }

    impl Isound3dBufferBackend for Rig {
        fn set_3d_position(&mut self, buffer: u32, pos: [f32; 3]) -> bool {
            self.calls3d.push(format!("pos:{buffer}:{pos:?}"));
            self.ok3d
        }
        fn set_3d_orientation(&mut self, buffer: u32, dir: [f32; 3]) -> bool {
            self.calls3d.push(format!("dir:{buffer}:{dir:?}"));
            self.ok3d
        }
        fn set_3d_velocity(&mut self, buffer: u32, velocity: [f32; 3]) -> bool {
            self.calls3d.push(format!("vel:{buffer}:{velocity:?}"));
            self.ok3d
        }
        fn set_3d_min_distance(&mut self, buffer: u32, min: f32) -> bool {
            self.calls3d.push(format!("min:{buffer}:{min}"));
            self.ok3d
        }
        fn set_3d_max_distance(&mut self, buffer: u32, max: f32) -> bool {
            self.calls3d.push(format!("max:{buffer}:{max}"));
            self.ok3d
        }
        fn set_3d_cone_angles(&mut self, buffer: u32, inside: u32, outside: u32) -> bool {
            self.calls3d
                .push(format!("cone:{buffer}:{inside}:{outside}"));
            self.ok3d
        }
        fn set_3d_outside_volume(&mut self, buffer: u32, millibels: i32) -> bool {
            self.calls3d.push(format!("out:{buffer}:{millibels}"));
            self.ok3d
        }
        fn query_3d_buffer(&mut self, _buffer: u32) -> Option<u32> {
            self.query3d
        }
        fn release_3d_buffer(&mut self, buffer: u32) {
            self.released3d.push(buffer);
        }
    }

    fn test_wav(channels: u16, rate: u32, bits: u16, payload: &[u8]) -> Vec<u8> {
        let align = u32::from(channels) * u32::from(bits) / 8;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend((36 + payload.len() as u32).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(rate.to_le_bytes());
        bytes.extend((rate * align).to_le_bytes());
        bytes.extend((align as u16).to_le_bytes());
        bytes.extend(bits.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        bytes
    }

    #[test]
    fn isound_listener_new_defaults() {
        let listener = isound_listener_new();
        assert_eq!(listener.ref_count, 1);
        assert_eq!(listener.position, [0.0; 3]);
        assert_eq!(listener.velocity, [0.0; 3]);
        assert_eq!(listener.orient_front, [0.0; 3]);
        assert_eq!(listener.orient_top, [0.0; 3]);
        assert_eq!(listener.distance_factor, 1.0);
        assert_eq!(listener.doppler_factor, 1.0);
        assert_eq!(listener.rolloff_factor, 1.0);
        assert_eq!(listener.native_size, 64);
    }

    #[test]
    fn isound_listener_add_ref_increments() {
        let mut listener = isound_listener_new();
        assert_eq!(isound_listener_add_ref(&mut listener), 2);
        assert_eq!(listener.ref_count, 2);
    }

    #[test]
    fn isound_listener_release_drop_and_warn_paths() {
        let mut listener = isound_listener_new();
        isound_listener_add_ref(&mut listener);
        assert_eq!(isound_listener_release(&mut listener), 1);
        assert_eq!(isound_listener_release(&mut listener), 0);
        assert_eq!(listener.ref_count, 0);
        let mut fresh = isound_listener_new();
        fresh.ref_count = 0;
        assert_eq!(isound_listener_release(&mut fresh), 0);
        assert_eq!(fresh.ref_count, -1);
    }

    #[test]
    fn isound_listener_set_pos_stores() {
        let mut listener = isound_listener_new();
        assert_eq!(isound_listener_set_pos(&mut listener, [1.0, 2.0, 3.0]), 0);
        assert_eq!(listener.position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn isound_listener_set_dir_stores() {
        let mut listener = isound_listener_new();
        assert_eq!(
            isound_listener_set_dir(&mut listener, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            0
        );
        assert_eq!(listener.orient_front, [0.0, 0.0, 1.0]);
        assert_eq!(listener.orient_top, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn isound_listener_set_velocity_stores() {
        let mut listener = isound_listener_new();
        assert_eq!(
            isound_listener_set_velocity(&mut listener, [4.0, 5.0, 6.0]),
            0
        );
        assert_eq!(listener.velocity, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn isound_listener_set_distance_factor_stores() {
        let mut listener = isound_listener_new();
        assert_eq!(isound_listener_set_distance_factor(&mut listener, 2.5), 0);
        assert_eq!(listener.distance_factor, 2.5);
    }

    #[test]
    fn isound_listener_set_doppler_factor_stores() {
        let mut listener = isound_listener_new();
        assert_eq!(isound_listener_set_doppler_factor(&mut listener, 0.5), 0);
        assert_eq!(listener.doppler_factor, 0.5);
    }

    #[test]
    fn isound_listener_set_rolloff_factor_stores() {
        let mut listener = isound_listener_new();
        assert_eq!(isound_listener_set_rolloff_factor(&mut listener, 3.0), 0);
        assert_eq!(listener.rolloff_factor, 3.0);
    }

    #[test]
    fn isound_listener_set_eax_property_classes() {
        let mut rig = Rig::default();
        assert_eq!(
            isound_listener_set_eax_property(false, ISNDINIT_EAX30, EAX_LISTENER_ROOM, 1, &mut rig),
            ISOUND_UNSUPPORTED
        );
        assert!(rig.eax.is_empty());
        assert_eq!(
            isound_listener_set_eax_property(true, 0, EAX_LISTENER_ROOM, 1, &mut rig),
            ISOUND_UNSUPPORTED
        );
        assert!(rig.eax.is_empty());
        let flags = ISNDINIT_EAX30;
        assert_eq!(
            isound_listener_set_eax_property(true, flags, EAX_LISTENER_ROOM, 7, &mut rig),
            0
        );
        assert_eq!(
            isound_listener_set_eax_property(
                true,
                flags,
                EAX_LISTENER_DECAYTIME,
                0x3f80_0000,
                &mut rig
            ),
            0
        );
        assert_eq!(
            isound_listener_set_eax_property(true, flags, EAX_LISTENER_REVERBPAN, 0x2000, &mut rig),
            0
        );
        assert_eq!(
            isound_listener_set_eax_property(
                true,
                flags,
                EAX_LISTENER_ALLPARAMETERS,
                0x3000,
                &mut rig
            ),
            0
        );
        assert_eq!(
            rig.eax,
            vec![
                "int:5:7".to_string(),
                "float:8:1065353216".to_string(),
                "vec:16:8192".to_string(),
            ]
        );
    }

    #[test]
    fn isound_listener_set_eax3_property_passthrough() {
        let mut rig = Rig::default();
        assert_eq!(
            isound_listener_set_eax3_property(true, 0, 4, 0x100, 8, &mut rig),
            ISOUND_UNSUPPORTED
        );
        assert_eq!(
            isound_listener_set_eax3_property(true, ISNDINIT_EAX30, 4, 0x100, 8, &mut rig),
            0
        );
        assert_eq!(rig.eax, vec!["raw:4:256:8".to_string()]);
    }

    #[test]
    fn isound_listener_get_eax3_property_passthrough() {
        let mut rig = Rig::default();
        assert_eq!(
            isound_listener_get_eax3_property(false, ISNDINIT_EAX30, 4, 0x100, 8, 0x200, &mut rig),
            ISOUND_UNSUPPORTED
        );
        assert_eq!(
            isound_listener_get_eax3_property(true, ISNDINIT_EAX30, 4, 0x100, 8, 0x200, &mut rig),
            0
        );
        assert_eq!(rig.eax, vec!["get:4:256:8:512".to_string()]);
    }

    #[test]
    fn isound_3dsound_new_words() {
        let sound = isound_3dsound_new();
        assert!(!sound.is_3d_init);
        assert_eq!(sound.buffer_3d, None);
        assert_eq!(sound.base.ref_count, 1);
        assert_eq!(sound.base.freq_mult, 1.0);
        assert_eq!(sound.base.volume, 1.0);
    }

    #[test]
    fn isound_3dsound_destroy_runs_close() {
        let mut rig = Rig::default();
        let mut sound = isound_3dsound_new();
        sound.base.buffer = Some(11);
        sound.base.source = Some(5);
        sound.buffer_3d = Some(12);
        sound.is_3d_init = true;
        let mut attached = isound_source_new();
        attached.ref_count = 4;
        let mut bank = IsoundChannelBank::default();
        assert_eq!(
            isound_3dsound_destroy(
                &mut sound,
                7,
                Some(&mut attached),
                &mut bank,
                0,
                0,
                &mut rig
            ),
            0
        );
        assert!(!sound.is_3d_init);
        assert_eq!(sound.base.buffer, None);
        assert_eq!(sound.base.source, None);
        assert_eq!(attached.ref_count, 3);
        assert_eq!(rig.released3d, vec![12]);
        assert_eq!(rig.stopped, vec![11]);
        assert_eq!(rig.released, vec![11]);
    }

    #[test]
    fn isound_3dsound_is_3d_init_copies() {
        let mut sound = isound_3dsound_new();
        assert!(!isound_3dsound_is_3d_init(&sound));
        sound.is_3d_init = true;
        assert!(isound_3dsound_is_3d_init(&sound));
    }

    #[test]
    fn isound_3dsound_set_pos_null_and_fail() {
        let mut rig = Rig::default();
        let sound = isound_3dsound_new();
        assert_eq!(isound_3dsound_set_pos(&sound, [1.0, 0.0, 0.0], &mut rig), 0);
        assert!(rig.calls3d.is_empty());
        let mut live = isound_3dsound_new();
        live.buffer_3d = Some(9);
        assert_eq!(isound_3dsound_set_pos(&live, [1.0, 0.0, 0.0], &mut rig), 0);
        assert_eq!(rig.calls3d.len(), 1);
        rig.ok3d = false;
        assert_eq!(
            isound_3dsound_set_pos(&live, [1.0, 0.0, 0.0], &mut rig),
            ISOUND_GENERIC
        );
    }

    #[test]
    fn isound_3dsound_set_dir_submit() {
        let mut rig = Rig::default();
        let mut live = isound_3dsound_new();
        live.buffer_3d = Some(9);
        assert_eq!(isound_3dsound_set_dir(&live, [0.0, 0.0, 1.0], &mut rig), 0);
        assert_eq!(rig.calls3d.len(), 1);
    }

    #[test]
    fn isound_3dsound_set_velocity_submit() {
        let mut rig = Rig::default();
        let mut live = isound_3dsound_new();
        live.buffer_3d = Some(9);
        assert_eq!(
            isound_3dsound_set_velocity(&live, [1.0, 2.0, 3.0], &mut rig),
            0
        );
        assert_eq!(rig.calls3d.len(), 1);
    }

    #[test]
    fn isound_3dsound_set_distances_min_first() {
        let mut rig = Rig::default();
        let mut live = isound_3dsound_new();
        live.buffer_3d = Some(9);
        assert_eq!(isound_3dsound_set_distances(&live, 2.0, 20.0, &mut rig), 0);
        assert_eq!(
            rig.calls3d,
            vec!["min:9:2".to_string(), "max:9:20".to_string()]
        );
        rig.calls3d.clear();
        rig.ok3d = false;
        assert_eq!(
            isound_3dsound_set_distances(&live, 2.0, 20.0, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(rig.calls3d, vec!["min:9:2".to_string()]);
    }

    #[test]
    fn isound_3dsound_set_cone_degrees() {
        assert_eq!(isound_cone_radians_to_dword(0.0), 0);
        assert_eq!(isound_cone_radians_to_dword(1.0), 57);
        assert_eq!(isound_cone_radians_to_dword(2.0), 114);
        assert_eq!(isound_cone_radians_to_dword(-1.0), 0);
        let mut rig = Rig::default();
        let sound = isound_3dsound_new();
        assert_eq!(isound_3dsound_set_cone(&sound, 1.0, 2.0, &mut rig), 0);
        assert!(rig.calls3d.is_empty());
        let mut live = isound_3dsound_new();
        live.buffer_3d = Some(9);
        assert_eq!(isound_3dsound_set_cone(&live, 1.0, 2.0, &mut rig), 0);
        assert_eq!(rig.calls3d, vec!["cone:9:57:114".to_string()]);
    }

    #[test]
    fn isound_3dsound_set_out_volume_words() {
        let mut rig = Rig::default();
        let sound = isound_3dsound_new();
        assert_eq!(isound_3dsound_set_out_volume(&sound, 0.25, &mut rig), 0);
        assert!(rig.calls3d.is_empty());
        let mut live = isound_3dsound_new();
        live.buffer_3d = Some(9);
        assert_eq!(isound_3dsound_set_out_volume(&live, 0.25, &mut rig), 0);
        assert_eq!(isound_3dsound_set_out_volume(&live, 0.0, &mut rig), 0);
        assert_eq!(isound_3dsound_set_out_volume(&live, 1.0, &mut rig), 0);
        assert_eq!(
            rig.calls3d,
            vec![
                "out:9:-7500".to_string(),
                "out:9:-10000".to_string(),
                "out:9:0".to_string(),
            ]
        );
    }

    #[test]
    fn isound_3dsound_open_branches() {
        let mut rig = Rig {
            duplicate: Ok(21),
            ..Default::default()
        };
        let mut bank = IsoundChannelBank::default();
        let mut sound = isound_3dsound_new();
        let mut fresh = isound_source_new();
        assert_eq!(
            isound_3dsound_open(
                &mut sound,
                7,
                None,
                &mut fresh,
                3,
                ISND_3DBUFFER,
                &mut bank,
                0,
                0,
                &mut rig,
            ),
            ISOUND_GENERIC
        );
        fresh.buffer = Some(20);
        assert_eq!(
            isound_3dsound_open(
                &mut sound,
                7,
                None,
                &mut fresh,
                3,
                ISND_3DBUFFER,
                &mut bank,
                0,
                0,
                &mut rig,
            ),
            0
        );
        assert!(sound.is_3d_init);
        assert_eq!(sound.buffer_3d, Some(9));
        assert_eq!(sound.base.buffer, Some(21));
        rig.query3d = None;
        let mut second = isound_3dsound_new();
        let mut other = isound_source_new();
        other.buffer = Some(20);
        assert_eq!(
            isound_3dsound_open(
                &mut second,
                8,
                None,
                &mut other,
                4,
                ISND_3DBUFFER,
                &mut bank,
                0,
                0,
                &mut rig,
            ),
            ISOUND_GENERIC
        );
        assert!(!second.is_3d_init);
        assert_eq!(second.base.buffer, Some(21));
        let mut plain = isound_3dsound_new();
        let mut third = isound_source_new();
        third.buffer = Some(20);
        assert_eq!(
            isound_3dsound_open(&mut plain, 9, None, &mut third, 5, 0, &mut bank, 0, 0, &mut rig,),
            0
        );
        assert!(plain.is_3d_init);
        assert_eq!(plain.buffer_3d, None);
    }

    #[test]
    fn isound_3dsound_close_clears() {
        let mut rig = Rig::default();
        let mut sound = isound_3dsound_new();
        sound.is_3d_init = true;
        sound.buffer_3d = Some(12);
        sound.base.buffer = Some(11);
        let mut bank = IsoundChannelBank::default();
        assert_eq!(
            isound_3dsound_close(&mut sound, 7, None, &mut bank, 0, 0, &mut rig),
            0
        );
        assert!(!sound.is_3d_init);
        assert_eq!(sound.buffer_3d, None);
        assert_eq!(sound.base.buffer, None);
        assert_eq!(rig.released3d, vec![12]);
        assert_eq!(rig.released, vec![11]);
    }

    #[test]
    fn isound_sound_get_source_copies() {
        let mut sound = isound_sound_new();
        assert_eq!(isound_sound_get_source(&sound), None);
        sound.source = Some(42);
        assert_eq!(isound_sound_get_source(&sound), Some(42));
    }

    #[test]
    fn isound_driver_new_words() {
        let driver = isound_driver_new();
        assert_eq!(driver.ref_count, 1);
        assert_eq!(driver.hwnd, 0);
        assert!(!driver.has_direct_sound);
        assert_eq!(driver.volume, 0.0);
        assert_eq!(driver.create_flags, 0);
        assert!(driver.open_source_names.is_empty());
    }

    #[test]
    fn isound_globals_new_cleared() {
        let globals = isound_globals_new();
        assert!(!globals.is_init);
        assert_eq!(globals.max_hw_channels, 0);
    }

    #[test]
    fn isound_driver_destroy_warns_only_when_init() {
        let mut rig = Rig::default();
        let idle = isound_globals_new();
        isound_driver_destroy(&idle, &mut rig);
        assert!(!rig.dtor);
        let live = IsoundGlobals {
            is_init: true,
            max_hw_channels: 4,
        };
        isound_driver_destroy(&live, &mut rig);
        assert!(rig.dtor);
    }

    #[test]
    fn isound_driver_is_init_copies() {
        assert!(!isound_driver_is_init(&isound_globals_new()));
        assert!(isound_driver_is_init(&IsoundGlobals {
            is_init: true,
            max_hw_channels: 0
        }));
    }

    #[test]
    fn isound_driver_close_order() {
        let mut rig = Rig::default();
        let mut driver = isound_driver_new();
        driver.open_source_names = vec!["a.wav".to_string(), "b.wav".to_string()];
        driver.has_direct_sound = true;
        driver.has_stream_table = true;
        driver.has_stream_module = true;
        driver.has_device_listener = true;
        driver.has_eax_property_set = true;
        driver.has_eax_probe_buffer = true;
        driver.has_primary_buffer = true;
        let mut globals = IsoundGlobals {
            is_init: true,
            max_hw_channels: 8,
        };
        isound_driver_close(&mut driver, &mut globals, &mut rig);
        assert_eq!(
            rig.unreleased,
            vec!["a.wav".to_string(), "b.wav".to_string()]
        );
        assert_eq!(
            rig.close_log,
            vec![
                "streams".to_string(),
                "module".to_string(),
                "listener".to_string(),
                "eax".to_string(),
                "probe".to_string(),
                "primary".to_string(),
                "dsound".to_string(),
            ]
        );
        assert!(!globals.is_init);
        assert!(!driver.has_direct_sound);
        let mut bare = isound_driver_new();
        let mut idle = isound_globals_new();
        idle.is_init = true;
        isound_driver_close(&mut bare, &mut idle, &mut rig);
        assert!(!idle.is_init);
    }

    #[test]
    fn isound_driver_init_buffer_words() {
        let mut rig = Rig::default();
        assert_eq!(isound_driver_init_buffer(22050, 2, 16, &mut rig), 0);
        assert_eq!(
            rig.primary,
            Some(IsoundPcmFormat {
                bits_per_sample: 16,
                channels: 2,
                samples_per_sec: 22050,
                format_tag: 1,
                block_align: 4,
                avg_bytes_per_sec: 88200,
            })
        );
        assert_eq!(isound_driver_init_buffer(22050, 1, 12, &mut rig), 0);
        assert_eq!(rig.primary.unwrap().block_align, 1);
        rig.primary_ok = false;
        assert_eq!(
            isound_driver_init_buffer(22050, 1, 8, &mut rig),
            ISOUND_INVALIDPARAMS
        );
    }

    #[test]
    fn isound_driver_set_volume_guard_and_mark() {
        let mut driver = isound_driver_new();
        driver.volume = 1.0;
        let idle = isound_globals_new();
        let mut sounds = vec![
            IsoundVolumeMark {
                playing_logical: true,
                snd_flags: 0,
            },
            IsoundVolumeMark {
                playing_logical: false,
                snd_flags: 0,
            },
        ];
        assert_eq!(
            isound_driver_set_volume(&mut driver, &idle, &mut sounds, 0.5),
            0
        );
        assert_eq!(driver.volume, 1.0);
        assert_eq!(sounds[0].snd_flags, 0);
        let live = IsoundGlobals {
            is_init: true,
            max_hw_channels: 0,
        };
        assert_eq!(
            isound_driver_set_volume(&mut driver, &live, &mut sounds, 0.5),
            0
        );
        assert_eq!(driver.volume, 0.5);
        // Playing sounds are marked SOUNDF_VOLUME (8); idle sounds are not.
        assert_eq!(sounds[0].snd_flags, 8);
        assert_eq!(sounds[1].snd_flags, 0);
        assert_eq!(
            isound_driver_set_volume(&mut driver, &live, &mut sounds, 0.5),
            0
        );
        assert_eq!(sounds[0].snd_flags, 8);
        assert_eq!(
            isound_driver_set_volume(&mut driver, &live, &mut sounds, 0.25),
            0
        );
        assert_eq!(driver.volume, 0.25);
        assert_eq!(sounds[0].snd_flags, SOUNDF_VOLUME);
        assert_eq!(sounds[1].snd_flags, 0);
        assert_eq!(
            isound_driver_set_volume(&mut driver, &live, &mut sounds, 5.0),
            0
        );
        assert_eq!(driver.volume, 1.0);
        assert_eq!(
            isound_driver_set_volume(&mut driver, &live, &mut sounds, f32::NAN),
            0
        );
        assert_eq!(driver.volume, 1.0);
    }

    #[test]
    fn isound_driver_get_caps_ok() {
        assert_eq!(isound_driver_get_caps(), 0);
    }

    #[test]
    fn isound_driver_create_source_words() {
        let source = isound_driver_create_source();
        assert_eq!(source.ref_count, 1);
        assert_eq!(source.buffer, None);
    }

    #[test]
    fn isound_driver_create_source_out_null_and_ok() {
        assert_eq!(isound_driver_create_source_out(None), ISOUND_INVALIDPARAMS);
        let mut slot = isound_source_new();
        slot.ref_count = 9;
        assert_eq!(isound_driver_create_source_out(Some(&mut slot)), 0);
        assert_eq!(slot.ref_count, 1);
    }

    #[test]
    fn isound_driver_create_sound_words() {
        let sound = isound_driver_create_sound();
        assert_eq!(sound.ref_count, 1);
        assert_eq!(sound.freq_mult, 1.0);
    }

    #[test]
    fn isound_driver_create_sound_out_null_and_ok() {
        assert_eq!(isound_driver_create_sound_out(None), ISOUND_INVALIDPARAMS);
        let mut slot = isound_sound_new();
        slot.volume = 9.0;
        assert_eq!(isound_driver_create_sound_out(Some(&mut slot)), 0);
        assert_eq!(slot.volume, 1.0);
    }

    #[test]
    fn isound_driver_create_3dsound_words() {
        let sound = isound_driver_create_3dsound();
        assert!(!sound.is_3d_init);
        assert_eq!(sound.buffer_3d, None);
    }

    #[test]
    fn isound_driver_create_3dsound_out_null_and_ok() {
        assert_eq!(isound_driver_create_3dsound_out(None), ISOUND_INVALIDPARAMS);
        let mut slot = isound_3dsound_new();
        slot.is_3d_init = true;
        assert_eq!(isound_driver_create_3dsound_out(Some(&mut slot)), 0);
        assert!(!slot.is_3d_init);
    }

    #[test]
    fn isound_driver_create_listener_refresh_and_default() {
        let mut rig = Rig::default();
        let listener = isound_driver_create_listener(&mut rig);
        assert!(rig.refreshed);
        assert_eq!(listener.distance_factor, 1.0);
        let mut with_device = Rig::default();
        let mut params = isound_listener_new();
        params.position = [7.0, 8.0, 9.0];
        with_device.refresh_to = Some(params);
        let refreshed = isound_driver_create_listener(&mut with_device);
        assert_eq!(refreshed.position, [7.0, 8.0, 9.0]);
    }

    #[test]
    fn isound_driver_create_listener_out_null_and_ok() {
        let mut rig = Rig::default();
        assert_eq!(
            isound_driver_create_listener_out(None, &mut rig),
            ISOUND_INVALIDPARAMS
        );
        let mut slot = isound_listener_new();
        slot.distance_factor = 9.0;
        assert_eq!(
            isound_driver_create_listener_out(Some(&mut slot), &mut rig),
            0
        );
        assert_eq!(slot.distance_factor, 1.0);
    }

    #[test]
    fn isound_driver_repair_freq_bounds_vectors() {
        // Narrow-helper credit exception (overlay 0x7be20): this phase owns
        // its entry (sibling phases like speaker_config take none).
        assert_eq!(isound_driver_repair_freq_bounds(8000, 48000), (8000, 48000));
        assert_eq!(
            isound_driver_repair_freq_bounds(44100, 44100),
            (10000, 44100)
        );
        assert_eq!(isound_driver_repair_freq_bounds(0, 0), (10000, 44000));
        assert_eq!(
            isound_driver_repair_freq_bounds(11025, 48000),
            (10000, 48000)
        );
        assert_eq!(isound_driver_repair_freq_bounds(4000, 96000), (8000, 96000));
        // max<40000 fires the warn arm too: min resets to 10000.
        assert_eq!(
            isound_driver_repair_freq_bounds(8000, 32000),
            (10000, 44000)
        );
    }

    #[test]
    fn isound_driver_speaker_config_priority() {
        assert_eq!(
            isound_driver_speaker_config(ISNDINIT_HEADPHONES | ISNDINIT_STEREO),
            Some(IsoundSpeakerConfig::Headphone)
        );
        assert_eq!(
            isound_driver_speaker_config(ISNDINIT_QUAD | ISNDINIT_SURROUND),
            Some(IsoundSpeakerConfig::Quad)
        );
        assert_eq!(
            isound_driver_speaker_config(ISNDINIT_SURROUND),
            Some(IsoundSpeakerConfig::Surround51)
        );
        assert_eq!(
            isound_driver_speaker_config(ISNDINIT_STEREO),
            Some(IsoundSpeakerConfig::Stereo)
        );
        assert_eq!(isound_driver_speaker_config(0), None);
        assert_eq!(IsoundSpeakerConfig::Surround51.code(), DSSPEAKER_5POINT1);
        assert_eq!(IsoundSpeakerConfig::Headphone.code(), DSSPEAKER_HEADPHONE);
    }

    #[test]
    fn isound_driver_init_mark_fail_keeps_state() {
        let mut rig = Rig {
            dsound_ok: false,
            ..Default::default()
        };
        let mut driver = isound_driver_new();
        let mut globals = isound_globals_new();
        assert_eq!(
            isound_driver_init_mark(&mut driver, &mut globals, 3, 22050, 2, 16, 0, &mut rig),
            ISOUND_GENERIC
        );
        assert!(!globals.is_init);
        assert_eq!(driver.hwnd, 0);
        assert_eq!(rig.init_seen.len(), 1);
        rig.dsound_ok = true;
        assert_eq!(
            isound_driver_init_mark(&mut driver, &mut globals, 3, 22050, 2, 16, 0, &mut rig),
            0
        );
        assert!(globals.is_init);
        assert_eq!(driver.hwnd, 3);
        assert_eq!(driver.volume, 1.0);
    }

    #[test]
    fn isound_driver_init_from_record_null_and_delegate() {
        let mut rig = Rig::default();
        let mut driver = isound_driver_new();
        let mut globals = isound_globals_new();
        assert_eq!(
            isound_driver_init_from_record(&mut driver, &mut globals, None, &mut rig),
            ISOUND_INVALIDPARAMS
        );
        assert!(rig.init_seen.is_empty());
        let record = IsoundInitRecord {
            hwnd: 5,
            mix_frequency: 44100,
            num_channels: 1,
            bits_per_second: 8,
            flags: ISNDINIT_STEREO,
        };
        assert_eq!(
            isound_driver_init_from_record(&mut driver, &mut globals, Some(&record), &mut rig),
            0
        );
        assert_eq!(rig.init_seen, vec!["5:44100:1:8:2048".to_string()]);
        assert!(globals.is_init);
    }

    #[test]
    fn isound_pick_best_priority_then_volume() {
        let mut get = |sound: u64| match sound {
            1 => 0.9,
            2 => 0.2,
            3 => 0.9,
            _ => 0.0,
        };
        assert_eq!(isound_pick_best_terminated(&[], &mut get), None);
        let entry = |sound: u64, priority: u32| IsoundTerminatedEntry {
            sound,
            buffer: 0,
            priority,
            position: 0,
            stop_time: 0,
            is_loop: false,
        };
        assert_eq!(
            isound_pick_best_terminated(&[entry(1, 3)], &mut get),
            Some(0)
        );
        assert_eq!(
            isound_pick_best_terminated(&[entry(1, 3), entry(2, 5)], &mut get),
            Some(1)
        );
        assert_eq!(
            isound_pick_best_terminated(&[entry(2, 5), entry(3, 5)], &mut get),
            Some(1)
        );
        assert_eq!(
            isound_pick_best_terminated(&[entry(1, 5), entry(2, 5)], &mut get),
            Some(0)
        );
    }

    #[test]
    fn isound_update_all_terminated_buffs_scenario() {
        let mut rig = Rig {
            clock: 1000,
            ..Default::default()
        };
        rig.playing.insert(10, false);
        rig.play_times.insert(2, 500);
        rig.play_times.insert(3, 2000);
        rig.freq_mults.insert(2, 1.0);
        rig.freq_mults.insert(3, 1.0);
        rig.volumes.insert(2, 0.2);
        rig.volumes.insert(3, 0.9);
        rig.bpms.insert(3, 1.0);
        rig.aligns.insert(3, 4);
        let mut bank = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 1,
                buffer: 10,
                priority: 1,
                is_loop: false,
                start_time: 0,
            }],
            terminated: vec![
                IsoundTerminatedEntry {
                    sound: 2,
                    buffer: 20,
                    priority: 5,
                    position: 100,
                    stop_time: 50,
                    is_loop: false,
                },
                IsoundTerminatedEntry {
                    sound: 3,
                    buffer: 30,
                    priority: 5,
                    position: 0,
                    stop_time: 0,
                    is_loop: false,
                },
            ],
        };
        isound_update_all_terminated_buffs(&mut bank, 4, true, &mut rig);
        assert_eq!(
            bank.active,
            vec![IsoundChannelEntry {
                sound: 3,
                buffer: 30,
                priority: 5,
                is_loop: false,
                start_time: 0,
            }]
        );
        assert!(bank.terminated.is_empty());
        assert_eq!(rig.set_positions, vec![(30, 996)]);
        assert_eq!(rig.resumed, vec![(30, false)]);
        let mut gated = bank.clone();
        isound_update_all_terminated_buffs(&mut gated, 0, true, &mut rig);
        assert_eq!(gated, bank);
        isound_update_all_terminated_buffs(&mut gated, 4, false, &mut rig);
        assert_eq!(gated, bank);
    }
    #[test]
    fn isound_update_active_expiry_ignores_frequency() {
        // Native active branch compares raw DWORDs (`time - start >=
        // GetPlayTime`) with no frequency division (division applies only
        // to the terminated-forget/resume paths). play_time 1000, freq 2:
        // elapsed 600 keeps the voice (divided comparison would expire it).
        let mut rig = Rig {
            clock: 600,
            ..Default::default()
        };
        rig.playing.insert(10, true);
        rig.play_times.insert(1, 1000);
        rig.freq_mults.insert(1, 2.0);
        let mut bank = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 1,
                buffer: 10,
                priority: 1,
                is_loop: false,
                start_time: 0,
            }],
            terminated: vec![],
        };
        isound_update_all_terminated_buffs(&mut bank, 4, true, &mut rig);
        assert_eq!(bank.active.len(), 1);
        assert!(rig.stopped.is_empty());
    }

    #[test]
    fn isound_trace_ds_error_messages() {
        assert_eq!(
            isound_trace_ds_error(IsoundDsError::PrioLevelNeeded),
            "TraceDSErrors() - DSERR_PRIORLEVELNEEDED"
        );
        assert_eq!(
            isound_trace_ds_error(IsoundDsError::BufferLost),
            "TraceDSErrors() - DSERR_BUFFERLOST"
        );
        assert_eq!(
            isound_trace_ds_error(IsoundDsError::Unsupported),
            "TraceDSErrors() - DSERR_UNSUPPORTED"
        );
    }

    #[test]
    fn isound_sound_new_words() {
        let sound = isound_sound_new();
        assert_eq!(sound.ref_count, 1);
        assert_eq!(sound.buffer, None);
        assert_eq!(sound.source, None);
        assert_eq!(sound.flags, 0);
        assert_eq!(sound.freq_mult, 1.0);
        assert_eq!(sound.volume, 1.0);
    }

    #[test]
    fn isound_sound_destroy_runs_close() {
        let mut rig = Rig::default();
        let mut sound = isound_sound_new();
        sound.buffer = Some(11);
        sound.source = Some(5);
        let mut attached = isound_source_new();
        attached.ref_count = 2;
        let mut bank = IsoundChannelBank::default();
        assert_eq!(
            isound_sound_destroy(
                &mut sound,
                7,
                Some(&mut attached),
                &mut bank,
                0,
                0,
                &mut rig
            ),
            0
        );
        assert_eq!(sound.buffer, None);
        assert_eq!(attached.ref_count, 1);
        assert_eq!(rig.stopped, vec![11]);
        assert_eq!(rig.released, vec![11]);
    }

    #[test]
    fn isound_sound_release_paths() {
        let mut sound = isound_sound_new();
        assert_eq!(isound_sound_release(&mut sound), 0);
        assert_eq!(sound.ref_count, 0);
        let mut live = isound_sound_new();
        live.ref_count = 3;
        assert_eq!(isound_sound_release(&mut live), 2);
        live.ref_count = 0;
        assert_eq!(isound_sound_release(&mut live), 0);
        assert_eq!(live.ref_count, -1);
    }

    #[test]
    fn isound_sound_open_branches() {
        let mut rig = Rig::default();
        let mut bank = IsoundChannelBank::default();
        let mut sound = isound_sound_new();
        let mut fresh = isound_source_new();
        assert_eq!(
            isound_sound_open(&mut sound, 7, None, &mut fresh, 3, 0, &mut bank, 0, 0, &mut rig,),
            ISOUND_GENERIC
        );
        assert_eq!(sound.buffer, None);
        rig.duplicate = Err(IsoundDsError::BufferLost);
        fresh.buffer = Some(20);
        assert_eq!(
            isound_sound_open(&mut sound, 7, None, &mut fresh, 3, 0, &mut bank, 0, 0, &mut rig,),
            ISOUND_GENERIC
        );
        assert_eq!(rig.traces, vec!["TraceDSErrors() - DSERR_BUFFERLOST"]);
        assert_eq!(fresh.ref_count, 1);
        rig.duplicate = Ok(21);
        assert_eq!(
            isound_sound_open(
                &mut sound, 7, None, &mut fresh, 3, ISND_LOOP, &mut bank, 0, 0, &mut rig,
            ),
            0
        );
        assert_eq!(sound.buffer, Some(21));
        assert_eq!(sound.source, Some(3));
        assert_eq!(sound.flags, ISND_LOOP);
        assert_eq!(fresh.ref_count, 2);
    }

    #[test]
    fn isound_sound_close_effects() {
        let mut rig = Rig::default();
        let mut bank = IsoundChannelBank::default();
        let mut sound = isound_sound_new();
        sound.buffer = Some(11);
        sound.source = Some(5);
        sound.flags = ISND_LOOP;
        let mut attached = isound_source_new();
        attached.ref_count = 2;
        assert_eq!(
            isound_sound_close(
                &mut sound,
                7,
                Some(&mut attached),
                &mut bank,
                0,
                0,
                &mut rig,
            ),
            0
        );
        assert_eq!(sound.buffer, None);
        assert_eq!(sound.source, None);
        assert_eq!(attached.ref_count, 1);
        assert_eq!(rig.stopped, vec![11]);
        assert_eq!(rig.released, vec![11]);
        let mut bare = isound_sound_new();
        assert_eq!(
            isound_sound_close(&mut bare, 8, None, &mut bank, 0, 0, &mut rig),
            0
        );
        assert!(rig.stopped.len() == 1 && rig.released.len() == 1);
    }
    #[test]
    fn isound_sound_close_erases_channels_like_stop() {
        // Native Close() calls Stop() first: under the EAX gate the closing
        // voice's active + terminated entries vanish before the release.
        let mut rig = Rig::default();
        let mut bank = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 7,
                buffer: 11,
                priority: 1,
                is_loop: false,
                start_time: 0,
            }],
            terminated: vec![IsoundTerminatedEntry {
                sound: 7,
                buffer: 12,
                priority: 1,
                position: 0,
                stop_time: 0,
                is_loop: false,
            }],
        };
        let mut sound = isound_sound_new();
        sound.buffer = Some(11);
        sound.flags = 0;
        let mut attached = isound_source_new();
        assert_eq!(
            isound_sound_close(
                &mut sound,
                7,
                Some(&mut attached),
                &mut bank,
                4,
                ISNDINIT_EAX,
                &mut rig,
            ),
            0
        );
        assert!(bank.active.is_empty());
        assert!(bank.terminated.is_empty());
        assert_eq!(rig.stopped, vec![11]);
    }

    #[test]
    fn isound_sound_play_with_source_chains() {
        let mut rig = Rig::default();
        let mut sound = isound_sound_new();
        let mut fresh = isound_source_new();
        let mut bank = IsoundChannelBank::default();
        assert_eq!(
            isound_sound_play_with_source(
                &mut sound,
                None,
                &mut fresh,
                3,
                7,
                0,
                &mut bank,
                0,
                ISNDINIT_EAX,
                &mut rig
            ),
            ISOUND_GENERIC
        );
        fresh.buffer = Some(20);
        rig.duplicate = Ok(21);
        assert_eq!(
            isound_sound_play_with_source(
                &mut sound,
                None,
                &mut fresh,
                3,
                7,
                0,
                &mut bank,
                0,
                ISNDINIT_EAX,
                &mut rig
            ),
            0
        );
        assert_eq!(rig.play_calls, vec!["21:0:false:false".to_string()]);
    }

    #[test]
    fn isound_sound_play_channel_paths() {
        let mut rig = Rig {
            clock: 700,
            ..Default::default()
        };
        rig.positions.insert(11, 40);
        rig.playing.insert(90, true);
        rig.volumes.insert(9, 0.1);
        let mut sound = isound_sound_new();
        assert_eq!(
            isound_sound_play(
                &mut sound,
                &mut IsoundChannelBank::default(),
                1,
                0,
                0,
                4,
                0,
                &mut rig
            ),
            ISOUND_GENERIC
        );
        sound.buffer = Some(11);
        let mut bank = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 9,
                buffer: 90,
                priority: 1,
                is_loop: false,
                start_time: 0,
            }],
            terminated: Vec::new(),
        };
        assert_eq!(
            isound_sound_play(&mut sound, &mut bank, 1, 0, 5, 1, 0, &mut rig),
            0
        );
        assert_eq!(
            bank.active,
            vec![IsoundChannelEntry {
                sound: 1,
                buffer: 11,
                priority: 5,
                is_loop: false,
                start_time: 700,
            }]
        );
        assert_eq!(bank.terminated.len(), 1);
        assert_eq!(bank.terminated[0].sound, 9);
        // Native quirk pinned: the incoming cursor (buffer 11 -> 40),
        // not the displaced voice's, is filed for the terminated entry.
        assert_eq!(bank.terminated[0].position, 40);
        assert_eq!(rig.stopped, vec![90]);
        assert_eq!(rig.set_positions, vec![(11, 0)]);
        assert_eq!(rig.play_calls.last().unwrap(), "11:5:false:false");
        let mut waiting = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 9,
                buffer: 90,
                priority: 9,
                is_loop: false,
                start_time: 0,
            }],
            terminated: Vec::new(),
        };
        rig.play_calls.clear();
        assert_eq!(
            isound_sound_play(&mut sound, &mut waiting, 1, 0, 1, 1, 0, &mut rig),
            0
        );
        assert!(rig.play_calls.is_empty());
        assert_eq!(waiting.terminated.len(), 1);
        assert_eq!(waiting.terminated[0].sound, 1);
        let mut relisted = isound_sound_new();
        relisted.buffer = Some(11);
        let mut listed = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 1,
                buffer: 11,
                priority: 2,
                is_loop: false,
                start_time: 10,
            }],
            terminated: Vec::new(),
        };
        rig.clock = 800;
        assert_eq!(
            isound_sound_play(&mut relisted, &mut listed, 1, ISND_LOOP, 7, 4, 0, &mut rig),
            0
        );
        assert_eq!(listed.active[0].priority, 7);
        assert!(listed.active[0].is_loop);
        assert_eq!(listed.active[0].start_time, 800);
        assert_eq!(rig.play_calls.last().unwrap(), "11:7:true:false");
    }

    #[test]
    fn isound_sound_stop_gate() {
        let mut rig = Rig::default();
        let bare = isound_sound_new();
        assert_eq!(
            isound_sound_stop(
                &bare,
                1,
                &mut IsoundChannelBank::default(),
                4,
                ISNDINIT_EAX,
                &mut rig
            ),
            ISOUND_GENERIC
        );
        let mut sound = isound_sound_new();
        sound.buffer = Some(11);
        let mut bank = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 1,
                buffer: 11,
                priority: 3,
                is_loop: false,
                start_time: 0,
            }],
            terminated: vec![IsoundTerminatedEntry {
                sound: 1,
                buffer: 11,
                priority: 3,
                position: 5,
                stop_time: 6,
                is_loop: false,
            }],
        };
        assert_eq!(
            isound_sound_stop(&sound, 1, &mut bank, 4, ISNDINIT_EAX, &mut rig),
            0
        );
        assert_eq!(rig.stopped, vec![11]);
        assert!(bank.active.is_empty() && bank.terminated.is_empty());
        bank.active.push(IsoundChannelEntry {
            sound: 1,
            buffer: 11,
            priority: 3,
            is_loop: false,
            start_time: 0,
        });
        assert_eq!(isound_sound_stop(&sound, 1, &mut bank, 4, 0, &mut rig), 0);
        assert_eq!(bank.active.len(), 1);
    }

    #[test]
    fn isound_sound_get_curr_time_vectors() {
        let mut rig = Rig::default();
        assert_eq!(isound_sound_get_curr_time(None, 1.0, &mut rig), 0);
        assert_eq!(isound_sound_get_curr_time(Some(9), 0.0, &mut rig), 0);
        rig.positions.insert(9, 250);
        assert_eq!(isound_sound_get_curr_time(Some(9), 1.0, &mut rig), 250);
    }

    #[test]
    fn isound_sound_set_curr_time_vectors() {
        let mut rig = Rig {
            clock: 900,
            ..Default::default()
        };
        let sound = isound_sound_new();
        assert_eq!(
            isound_sound_set_curr_time(
                &sound,
                1,
                100,
                1.0,
                4,
                &mut IsoundChannelBank::default(),
                4,
                ISNDINIT_EAX,
                &mut rig
            ),
            ISOUND_GENERIC
        );
        let mut live = isound_sound_new();
        live.buffer = Some(11);
        let mut bank = IsoundChannelBank {
            active: vec![IsoundChannelEntry {
                sound: 1,
                buffer: 11,
                priority: 3,
                is_loop: false,
                start_time: 0,
            }],
            terminated: Vec::new(),
        };
        assert_eq!(
            isound_sound_set_curr_time(&live, 1, 100, 1.0, 4, &mut bank, 4, ISNDINIT_EAX, &mut rig),
            0
        );
        assert_eq!(rig.set_positions, vec![(11, 96)]);
        assert_eq!(bank.active[0].start_time, 800);
        assert_eq!(
            isound_sound_set_curr_time(&live, 1, 0, 1.0, 4, &mut bank, 4, ISNDINIT_EAX, &mut rig),
            0
        );
        assert_eq!(rig.set_positions[1], (11, 0));
    }

    #[test]
    fn isound_sound_set_volume_db_pins() {
        let mut rig = Rig::default();
        let mut guarded = isound_sound_new();
        guarded.flags = ISND_NOVOLUME;
        guarded.buffer = Some(11);
        assert_eq!(
            isound_sound_set_volume(&mut guarded, 1.0, 0.5, &mut rig),
            ISOUND_GENERIC
        );
        // Guard-before-store like the source: NOVOLUME leaves volume alone.
        assert_eq!(guarded.volume, 1.0);
        let mut live = isound_sound_new();
        live.buffer = Some(11);
        assert_eq!(isound_sound_set_volume(&mut live, 1.0, 1.0, &mut rig), 0);
        assert_eq!(isound_sound_set_volume(&mut live, 1.0, 0.0, &mut rig), 0);
        assert_eq!(isound_sound_set_volume(&mut live, 1.0, -0.5, &mut rig), 0);
        assert_eq!(isound_sound_set_volume(&mut live, 1.0, 0.5, &mut rig), 0);
        assert_eq!(
            rig.volumes_set,
            vec![(11, 0), (11, -10000), (11, -10000), (11, -600)]
        );
        assert_eq!(
            isound_sound_set_volume(&mut live, 1.0, f32::NAN, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(rig.volumes_set.len(), 4);
    }

    #[test]
    fn isound_sound_set_volume_log_stores_first() {
        let mut rig = Rig::default();
        let mut guarded = isound_sound_new();
        guarded.flags = ISND_NOVOLUME;
        assert_eq!(
            isound_sound_set_volume_log(&mut guarded, 1.0, 0.25, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(guarded.volume, 0.25);
        assert!(rig.volumes_set.is_empty());
        let mut live = isound_sound_new();
        live.buffer = Some(11);
        assert_eq!(
            isound_sound_set_volume_log(&mut live, 1.0, 0.25, &mut rig),
            0
        );
        assert_eq!(rig.volumes_set, vec![(11, -7500)]);
    }

    #[test]
    fn isound_sound_set_volume_log_db_pins() {
        let mut rig = Rig::default();
        let mut live = isound_sound_new();
        live.buffer = Some(11);
        assert_eq!(
            isound_sound_set_volume_log_db(&live, 1.0, -5000, &mut rig),
            0
        );
        assert_eq!(isound_sound_set_volume_log_db(&live, 0.5, 0, &mut rig), 0);
        assert_eq!(isound_sound_set_volume_log_db(&live, 1.0, 100, &mut rig), 0);
        assert_eq!(
            isound_sound_set_volume_log_db(&live, 1.0, -20000, &mut rig),
            0
        );
        assert_eq!(
            rig.volumes_set,
            vec![(11, -5000), (11, -600), (11, 0), (11, -10000)]
        );
        let guarded = isound_sound_new();
        assert_eq!(
            isound_sound_set_volume_log_db(&guarded, 1.0, 0, &mut rig),
            ISOUND_GENERIC
        );
    }

    #[test]
    fn isound_sound_set_frequency_pins() {
        let mut rig = Rig::default();
        let mut plain = isound_sound_new();
        plain.buffer = Some(11);
        assert_eq!(
            isound_sound_set_frequency(&mut plain, 22050, 8000, 44000, 2.0, &mut rig),
            ISOUND_GENERIC
        );
        let mut live = isound_sound_new();
        live.buffer = Some(11);
        live.flags = ISND_FREQUENCY;
        assert_eq!(
            isound_sound_set_frequency(&mut live, 22050, 8000, 44000, 2.0, &mut rig),
            0
        );
        assert_eq!(rig.freqs_set, vec![(11, 44000)]);
        assert_eq!(live.freq_mult, 44000.0f32 / 22050.0f32);
        assert_eq!(
            isound_sound_set_frequency(&mut live, 22050, 8000, 44000, 0.1, &mut rig),
            0
        );
        assert_eq!(rig.freqs_set[1], (11, 8000));
        assert_eq!(live.freq_mult, 8000.0f32 / 22050.0f32);
        assert_eq!(
            isound_sound_set_frequency(&mut live, 0, 8000, 44000, 1.0, &mut rig),
            ISOUND_GENERIC
        );
    }

    #[test]
    fn isound_sound_set_panning_pins() {
        let mut rig = Rig::default();
        let bare = isound_sound_new();
        assert_eq!(
            isound_sound_set_panning(&bare, 0.5, &mut rig),
            ISOUND_GENERIC
        );
        let mut live = isound_sound_new();
        live.buffer = Some(11);
        assert_eq!(isound_sound_set_panning(&live, 0.0, &mut rig), 0);
        assert_eq!(isound_sound_set_panning(&live, 0.5, &mut rig), 0);
        assert_eq!(isound_sound_set_panning(&live, 2.0, &mut rig), 0);
        assert_eq!(isound_sound_set_panning(&live, 1e10, &mut rig), 0);
        assert_eq!(
            rig.pans_set,
            vec![(11, 0), (11, -600), (11, 600), (11, 10000)]
        );
        assert_eq!(
            isound_sound_set_panning(&live, -1.0, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(
            isound_sound_set_panning(&live, f32::NAN, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(rig.pans_set.len(), 4);
        let mut nopan = isound_sound_new();
        nopan.buffer = Some(11);
        nopan.flags = ISND_NOPANNING;
        assert_eq!(
            isound_sound_set_panning(&nopan, 0.5, &mut rig),
            ISOUND_GENERIC
        );
    }

    #[test]
    fn isound_sound_is_playing_cpu_null_guard() {
        let mut rig = Rig::default();
        assert!(!isound_sound_is_playing_cpu(None, &mut rig));
        rig.playing.insert(11, true);
        assert!(isound_sound_is_playing_cpu(Some(11), &mut rig));
        rig.playing.insert(11, false);
        assert!(!isound_sound_is_playing_cpu(Some(11), &mut rig));
    }

    #[test]
    fn isound_source_file_name_fallback() {
        let mut source = isound_source_new();
        assert_eq!(isound_source_file_name(&source), "");
        source.filename = Some("Sounds/a.wav".to_string());
        assert_eq!(isound_source_file_name(&source), "Sounds/a.wav");
    }

    #[test]
    fn isound_source_new_zeroed() {
        let source = isound_source_new();
        assert_eq!(source.ref_count, 1);
        assert_eq!(source.filename, None);
        assert_eq!(source.data_bytes, 0);
        assert_eq!(source.bytes_per_ms, 0.0);
        assert_eq!(source.buffer, None);
    }

    #[test]
    fn isound_source_destroy_closes() {
        let mut rig = Rig::default();
        let mut source = isound_source_new();
        source.buffer = Some(7);
        source.data_bytes = 4;
        source.bytes_per_ms = 1.0;
        isound_source_destroy(&mut source, &mut rig);
        assert_eq!(source.buffer, None);
        assert_eq!(source.data_bytes, 0);
        assert_eq!(source.bytes_per_ms, 0.0);
        assert_eq!(rig.released, vec![7]);
    }

    #[test]
    fn isound_source_release_paths() {
        let mut source = isound_source_new();
        assert_eq!(isound_source_release(&mut source), 0);
        let mut live = isound_source_new();
        live.ref_count = 4;
        assert_eq!(isound_source_release(&mut live), 3);
        live.ref_count = 0;
        assert_eq!(isound_source_release(&mut live), 0);
        assert_eq!(live.ref_count, -1);
    }

    #[test]
    fn isound_source_open_vectors() {
        let mut rig = Rig::default();
        let mut source = isound_source_new();
        assert_eq!(
            isound_source_open(
                &mut source,
                "a.wav",
                0,
                Some(&test_wav(1, 1000, 8, &[200; 4])),
                false,
                false,
                &mut rig
            ),
            ISOUND_NOTINITIALIZED
        );
        assert_eq!(
            isound_source_open(&mut source, "a.wav", ISND_LOOP, None, true, false, &mut rig),
            ISOUND_NOFILE
        );
        assert_eq!(source.open_flags, ISND_LOOP);
        assert_eq!(
            isound_source_open(
                &mut source,
                "a.wav",
                0,
                Some(&b"BAD!"[..]),
                true,
                false,
                &mut rig
            ),
            ISOUND_GENERIC
        );
        assert_eq!(source.channels, 0);
        let wav = test_wav(1, 1000, 8, &[200, 201, 202, 203]);
        assert_eq!(
            isound_source_open(
                &mut source,
                "a.wav",
                ISND_3DBUFFER,
                Some(&wav),
                true,
                false,
                &mut rig
            ),
            0
        );
        assert_eq!(source.channels, 1);
        assert_eq!(source.samples_per_sec, 1000);
        assert_eq!(source.data_bytes, 4);
        assert_eq!(source.bytes_per_ms, 1.0);
        assert_eq!(source.open_flags & ISND_3DBUFFER, ISND_3DBUFFER);
        assert_eq!(source.buffer, Some(7));
        assert_eq!(source.filename.as_deref(), Some("a.wav"));
        assert!(isound_source_is_open(&source));
        assert_eq!(
            rig.created_desc,
            Some(IsoundBufferDesc {
                control_3d: true,
                defer_location: true,
                software_location: false,
                buffer_bytes: 4,
                no_virtualization: true,
            })
        );
        assert_eq!(
            rig.created_format,
            Some(IsoundWaveFormat {
                format_tag: 1,
                channels: 1,
                samples_per_sec: 1000,
                avg_bytes_per_sec: 1000,
                block_align: 1,
                bits_per_sample: 8,
            })
        );
        let stereo = test_wav(2, 1000, 8, &[128; 4]);
        let mut second = isound_source_new();
        assert_eq!(
            isound_source_open(
                &mut second,
                "b.wav",
                ISND_3DBUFFER,
                Some(&stereo),
                true,
                false,
                &mut rig
            ),
            0
        );
        assert_eq!(second.open_flags & ISND_3DBUFFER, 0);
        assert!(!rig.created_desc.unwrap().control_3d);
        let mut sysmem = isound_source_new();
        assert_eq!(
            isound_source_open(
                &mut sysmem,
                "a.wav",
                0,
                Some(&wav),
                true,
                false,
                &mut Rig::default()
            ),
            0
        );
        rig.create = None;
        let mut failed = isound_source_new();
        assert_eq!(
            isound_source_open(&mut failed, "a.wav", 0, Some(&wav), true, false, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(failed.buffer, None);
        assert_eq!(failed.channels, 1);
        rig.create = Some(8);
        rig.submit_ok = false;
        let mut short = isound_source_new();
        assert_eq!(
            isound_source_open(&mut short, "a.wav", 0, Some(&wav), true, false, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(short.buffer, Some(8));
    }

    #[test]
    fn isound_source_close_retains_name_and_flags() {
        let mut rig = Rig::default();
        let mut source = isound_source_new();
        source.buffer = Some(7);
        source.filename = Some("a.wav".to_string());
        source.open_flags = ISND_3DBUFFER;
        source.data_bytes = 4;
        isound_source_close(&mut source, &mut rig);
        assert_eq!(source.buffer, None);
        assert_eq!(source.data_bytes, 0);
        assert_eq!(source.filename.as_deref(), Some("a.wav"));
        assert_eq!(source.open_flags, ISND_3DBUFFER);
        assert_eq!(rig.released, vec![7]);
    }

    #[test]
    fn isound_source_is_open_words() {
        let mut source = isound_source_new();
        assert!(!isound_source_is_open(&source));
        source.data_bytes = 4;
        assert!(isound_source_is_open(&source));
    }

    #[test]
    fn isound_source_get_play_time_vectors() {
        let mut source = isound_source_new();
        assert_eq!(isound_source_get_play_time(&source), 0);
        source.data_bytes = 4;
        source.bytes_per_ms = 1.0;
        assert_eq!(isound_source_get_play_time(&source), 4);
        source.bytes_per_ms = 0.0;
        assert_eq!(isound_source_get_play_time(&source), 0);
    }

    #[test]
    fn isound_source_get_format_copies() {
        let mut source = isound_source_new();
        source.channels = 1;
        source.bits_per_sample = 16;
        source.samples_per_sec = 22050;
        source.data_bytes = 44100;
        let mut out = IsoundSourceFormat {
            num_channels: 0,
            bytes_per_sample: 0,
            samples_per_sec: 0,
            size: 0,
        };
        assert_eq!(isound_source_get_format(&source, &mut out), 0);
        assert_eq!(
            out,
            IsoundSourceFormat {
                num_channels: 1,
                bytes_per_sample: 2,
                samples_per_sec: 22050,
                size: 44100,
            }
        );
    }

    #[test]
    fn isound_listener_update_0x7b1b0_ok_and_generic() {
        let mut rig = Rig::default();
        let listener = isound_listener_new();
        assert_eq!(isound_listener_update(&listener, &mut rig), ISOUND_OK);
        assert_eq!(rig.param_calls, vec!["params".to_string()]);
        rig.params_ok = false;
        assert_eq!(
            isound_listener_update(&listener, &mut rig),
            ISOUND_GENERIC
        );
    }

    #[test]
    fn isound_listener_commit_deferred_0x7b320_ok_and_generic() {
        let mut rig = Rig::default();
        assert_eq!(isound_listener_commit_deferred_settings(&mut rig), ISOUND_OK);
        assert_eq!(rig.param_calls, vec!["commit".to_string()]);
        rig.params_ok = false;
        assert_eq!(
            isound_listener_commit_deferred_settings(&mut rig),
            ISOUND_GENERIC
        );
    }

    #[test]
    fn isound_source_lock_0x7d940_stores_first_area() {
        let mut rig = Rig::default();
        let mut source = isound_source_new();
        // Bufferless: GENERIC, out and save words untouched.
        let mut data = 0xdead;
        assert_eq!(
            isound_source_lock(&mut source, &mut data, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(data, 0xdead);
        assert_eq!((source.lock_ptr, source.lock_size), (None, 0));
        source.buffer = Some(7);
        assert_eq!(
            isound_source_lock(&mut source, &mut data, &mut rig),
            ISOUND_OK
        );
        assert_eq!(data, 0x5000);
        assert_eq!((source.lock_ptr, source.lock_size), (Some(0x5000), 128));
        // Failed submit: GENERIC, prior staging retained.
        rig.lock_area = None;
        let mut retry = 0xdead;
        assert_eq!(
            isound_source_lock(&mut source, &mut retry, &mut rig),
            ISOUND_GENERIC
        );
        assert_eq!(retry, 0xdead);
        assert_eq!((source.lock_ptr, source.lock_size), (Some(0x5000), 128));
    }

    #[test]
    fn isound_source_unlock_0x7d980_submits_staged_area() {
        let mut rig = Rig::default();
        let plain = isound_source_new();
        // Bufferless and unstaged: GENERIC with no submit.
        assert_eq!(isound_source_unlock(&plain, &mut rig), ISOUND_GENERIC);
        let mut staged = isound_source_new();
        staged.buffer = Some(7);
        assert_eq!(isound_source_unlock(&staged, &mut rig), ISOUND_GENERIC);
        assert!(rig.unlocked.is_empty());
        let mut data = 0;
        assert_eq!(
            isound_source_lock(&mut staged, &mut data, &mut rig),
            ISOUND_OK
        );
        assert_eq!(isound_source_unlock(&staged, &mut rig), ISOUND_OK);
        assert_eq!(rig.unlocked, vec![(7, 0x5000, 128)]);
        rig.unlock_ok = false;
        assert_eq!(
            isound_source_unlock(&staged, &mut rig),
            ISOUND_GENERIC
        );
    }

    #[test]
    fn isound_source_add_ref_pre_increments() {
        let mut source = isound_source_new();
        assert_eq!(isound_source_add_ref(&mut source), 2);
        assert_eq!(source.ref_count, 2);
    }

    #[test]
    fn isound_sound_inline_getters_copy_words() {
        let mut sound = isound_sound_new();
        sound.volume = 0.25;
        sound.freq_mult = 2.0;
        assert_eq!(isound_sound_get_volume(&sound), 0.25);
        assert_eq!(isound_sound_get_frequency(&sound), 2.0);
    }

    #[test]
    fn isound_init_hw_channels_gate_and_clamp() {
        assert_eq!(isound_init_hw_channels(20, 0), Some(20));
        assert_eq!(isound_init_hw_channels(35, 0), Some(35));
        assert_eq!(isound_init_hw_channels(36, 0), Some(30));
        assert_eq!(isound_init_hw_channels(90, 0), Some(30));
        assert_eq!(isound_init_hw_channels(20, ISNDINIT_NOHWCHANNELS), None);
    }

    #[test]
    fn isound_unlink_source_removes_first_match() {
        let mut bank = vec![3, 5, 5, 7];
        isound_unlink_source(&mut bank, 5);
        assert_eq!(bank.len(), 3);
        assert!(bank.contains(&5));
        isound_unlink_source(&mut bank, 9);
        assert_eq!(bank.len(), 3);
        isound_unlink_source(&mut bank, 5);
        assert!(!bank.contains(&5));
    }

    #[test]
    fn isound_source_buffer_desc_words_arms() {
        let hw_3d = isound_source_buffer_desc_words(true, true, 1024);
        assert!(hw_3d.control_3d && hw_3d.defer_location && !hw_3d.software_location);
        assert!(hw_3d.no_virtualization);
        assert_eq!(hw_3d.buffer_bytes, 1024);
        let sw = isound_source_buffer_desc_words(false, false, 512);
        assert!(!sw.control_3d && !sw.defer_location && sw.software_location);
        assert!(!sw.no_virtualization);
        assert_eq!(sw.buffer_bytes, 512);
    }

    #[test]
    fn isound_eax_property_width_arms() {
        assert_eq!(isound_eax_property_width(EAX_LISTENER_ROOM), Some(4));
        assert_eq!(isound_eax_property_width(EAX_LISTENER_DECAYTIME), Some(4));
        assert_eq!(
            isound_eax_property_width(EAX_LISTENER_REVERBPAN),
            Some(EAX_VECTOR_BYTES)
        );
        assert_eq!(isound_eax_property_width(EAX_LISTENER_ALLPARAMETERS), None);
        assert_eq!(isound_eax_property_width(0xffff), None);
        assert_eq!(EAX_VECTOR_BYTES, 12);
    }

    #[test]
    fn isound_source_open_flags_for_channels_stereo_rule() {
        assert_eq!(
            isound_source_open_flags_for_channels(ISND_3DBUFFER | ISND_FREQUENCY, 1),
            ISND_3DBUFFER | ISND_FREQUENCY
        );
        assert_eq!(
            isound_source_open_flags_for_channels(ISND_3DBUFFER | ISND_FREQUENCY, 2),
            ISND_FREQUENCY
        );
        assert_eq!(isound_source_open_flags_for_channels(0, 6), 0);
    }

    #[test]
    fn isound_hw_voice_managed_gate() {
        assert!(isound_hw_voice_managed(0, 0));
        assert!(!isound_hw_voice_managed(ISNDINIT_NOHWCHANNELS, 0));
        assert!(!isound_hw_voice_managed(0, ISND_SYSMEM));
        assert!(!isound_hw_voice_managed(
            ISNDINIT_NOHWCHANNELS,
            ISND_SYSMEM
        ));
        assert!(isound_hw_voice_managed(ISNDINIT_EAX, ISND_LOOP));
    }
}

// ---------------------------------------------------------------------------
// IShow music-stream behavioral contracts (LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//
// CPU-plausible prefixes per sibling mafia-re @ 113b5b6
// (ls3df/IGraph/IShowStream.cpp:1-118, ls3df/ISound/ISound.h:278-295,
// ls3df/IGraph/IGraph.h:474-496). Every entry below is tier contract:
// reviewed upstream source plus pinning tests. No Ghidra bodies were
// re-observed here; shapes follow the assigned REA notes, filed after the
// 0x7d3e0 IsPlaying-CPU precedent (CPU guard/prefix ported, device submits
// stay behind the backend).
//
// Working names: IShow::OpenStream/CloseStream/PlayStream/StopStream/
// PauseStream/SetStreamVolume/SetStreamPan are PC-only guessed IShow method
// names per source (IGraph.h class comment); IShow::GetVolume and
// GetDecodedFrameNum are PS2-evidenced original names
// per IGraph.h:454,469 (the first two live in original_logo), while
// IsStreamPlaying is a PC-only guessed name per the stream-block note;
// Rust spelling is snake_case per repo rule; each function doc records its
// RVA/VA and mafia-re file:line.
// The stream/file members below (stream handle, init/paused/done flags and
// the stream_shows bank) are working-guess projections per source
// (IGraph.h:505-534 notes PC members guessed).
//
// Scope: CPU list/flag logic only. ISND stream Init/Open/Close/Play/Stop/
// Pause/Resume/Volume/Pan submits stay behind IshowStreamBackend; Bink,
// DirectShow, Ogg and DirectSound buffering are withheld with no functions.
// StreamCallback end-marking (ISND_STREAM_END scan) is caller-side and stays
// unwired; only the done flag it would set is stored here.
//
// Ownership: typed state throughout, no byte maps. Show identities are
// caller-owned u64 values; the bank removes by swap-remove matching
// C_list::remove moving the last element into the hole (ls3df.h:305-335).
// Native `delete this` does not apply; the caller drops owners.
//
/// IShow stream CPU state (IShowStream.cpp:30-55 members m_Stream,
/// m_StreamInit, m_StreamPaused, m_StreamDone, working guess per source).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IshowStream {
    pub stream: i32,
    pub stream_init: bool,
    pub stream_paused: bool,
    pub stream_done: bool,
}

/// Caller-owned bank for the stream_shows list (IShowStream.cpp:10,
/// GLOBAL LS3DF 0x101c5db8, working guess per source). Removal is
/// swap-remove like C_list::remove; &mut is the thread boundary. Show ids
/// must be caller-stable: a reused id resurrects a stale entry, so the
/// caller owns liveness and must drop ids on teardown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IshowStreamBank {
    pub shows: Vec<u64>,
}

/// Device side of the ISND stream table (ISound.h:278-295
/// ISND_STREAM_FUNCS). Bool returns are SUCCEEDED submits; name and unknown
/// words are caller-owned. Result words reuse the ISound domain constants
/// (numerically equal to the I3D LS3D_RESULT words, ls3df.h:20-35).
/// All COM/DSound submits stay behind this trait.
/// The native StreamCallback (ISND_STREAM_END -> m_StreamDone) has no host
/// carrier: backends report end-of-stream via ishow_stream_mark_done below.
/// StreamCallback (0x758e0) stays withheld; IsStreamPlaying (0x75840) is now
/// ported as ishow_is_stream_playing below (owner lifts the prior withhold).
pub trait IshowStreamBackend {
    fn stream_init(&mut self, unknown: i32) -> bool;
    fn stream_open(&mut self, unknown: i32, name: &str, unknown1: i32) -> Option<i32>;
    fn stream_close(&mut self, unknown: i32) -> bool;
    fn stream_close_stream(&mut self, stream: i32) -> bool;
    fn stream_play(&mut self, stream: i32, looping: bool) -> bool;
    fn stream_stop(&mut self, stream: i32) -> bool;
    fn stream_pause(&mut self, stream: i32) -> bool;
    fn stream_resume(&mut self, stream: i32) -> bool;
    fn stream_set_volume(&mut self, stream: i32, unknown: i32, volume: i32) -> bool;
    fn stream_set_pan(&mut self, stream: i32, unknown: i32, pan: i32) -> bool;
}

/// LS3DF RVA 0x75920, VA 0x10075920. IShow::OpenStream (mafia-re
/// ls3df/IGraph/IShowStream.cpp:30): CloseStream prefix, then ISND Init submit
/// (FAILED is UNSUPPORTED, success sets init flag), then ISND Open submit
/// (FAILED is NOFILE). The CPU list/flag portion files the inline
/// stream_shows growth shape verified against IShowStream.cpp:40-53: found
/// flag, base pointer walk over size, size-tested loop, push_back only when
/// absent. Unused int arg is carried (ignored like native). Stream names are
/// UTF-8 only; non-UTF8 or interior-NUL game data is unrepresentable
/// (narrowed domain, disclosed). Native takes the list base
/// before the size test (OOB on empty); the host guards size != 0 first
/// (disclosed hardening).
pub fn ishow_open_stream(
    state: &mut IshowStream,
    show_id: u64,
    bank: &mut IshowStreamBank,
    name: &str,
    _unused: i32,
    backend: &mut impl IshowStreamBackend,
) -> i32 {
    ishow_close_stream(state, show_id, bank, backend);
    if !backend.stream_init(0) {
        return ISOUND_UNSUPPORTED;
    }
    state.stream_init = true;
    let Some(handle) = backend.stream_open(0, name, 0) else {
        return ISOUND_NOFILE;
    };
    state.stream = handle;
    let mut found = false;
    let size = bank.shows.len();
    if size != 0 {
        let mut index = 0usize;
        let mut remaining = size;
        while remaining != 0 {
            remaining -= 1;
            if bank.shows[index] == show_id {
                found = true;
                break;
            }
            index += 1;
        }
    }
    if !found {
        bank.shows.push(show_id);
    }
    ISOUND_OK
}

/// Host-side end-of-stream marker (native StreamCallback body, no separate
/// credit): the backend calls this when the device reports ISND_STREAM_END.
/// Keeps stream_done reachable now that OpenStream files no callback.
pub fn ishow_stream_mark_done(state: &mut IshowStream) {
    state.stream_done = true;
}

/// LS3DF RVA 0x75a20, VA 0x10075a20. IShow::CloseStream (mafia-re
/// ls3df/IGraph/IShowStream.cpp:59): clears paused/done flags, swap-removes
/// the show from stream_shows, then guarded submits. A live stream handle
/// first forwards to Stop (result ignored like native), then CloseStream
/// (FAILED returns early keeping the handle); then an init flag forwards to
/// Close and clears only on SUCCEEDED. Returns void in native.
pub fn ishow_close_stream(
    state: &mut IshowStream,
    show_id: u64,
    bank: &mut IshowStreamBank,
    backend: &mut impl IshowStreamBackend,
) {
    state.stream_paused = false;
    state.stream_done = false;
    if let Some(position) = bank.shows.iter().position(|id| *id == show_id) {
        bank.shows.swap_remove(position);
    }
    if state.stream != -1 {
        let _ = ishow_stop_stream(state, backend);
        if !backend.stream_close_stream(state.stream) {
            return;
        }
        state.stream = -1;
    }
    if state.stream_init && backend.stream_close(0) {
        state.stream_init = false;
    }
}

/// LS3DF RVA 0x75ad0, VA 0x10075ad0. IShow::PlayStream (mafia-re
/// ls3df/IGraph/IShowStream.cpp:78): paused-flag branch. When paused, clears
/// the flag and resumes (FAILED is GENERIC); else plays with loop word
/// (FAILED is GENERIC). I3D_OK on success.
pub fn ishow_play_stream(
    state: &mut IshowStream,
    looping: bool,
    backend: &mut impl IshowStreamBackend,
) -> i32 {
    if state.stream_paused {
        state.stream_paused = false;
        if !backend.stream_resume(state.stream) {
            return ISOUND_GENERIC;
        }
    } else if !backend.stream_play(state.stream, looping) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x75b30, VA 0x10075b30. IShow::StopStream (mafia-re
/// ls3df/IGraph/IShowStream.cpp:91): forwards to the stream Stop submit;
/// FAILED is GENERIC else I3D_OK. No flag writes.
pub fn ishow_stop_stream(state: &IshowStream, backend: &mut impl IshowStreamBackend) -> i32 {
    if !backend.stream_stop(state.stream) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x75b60, VA 0x10075b60. IShow::PauseStream (mafia-re
/// ls3df/IGraph/IShowStream.cpp:98): Pause submit first (FAILED is GENERIC
/// without flag write); success sets the paused flag, I3D_OK.
pub fn ishow_pause_stream(state: &mut IshowStream, backend: &mut impl IshowStreamBackend) -> i32 {
    if !backend.stream_pause(state.stream) {
        return ISOUND_GENERIC;
    }
    state.stream_paused = true;
    ISOUND_OK
}

/// LS3DF RVA 0x75b90, VA 0x10075b90. IShow::SetStreamVolume (mafia-re
/// ls3df/IGraph/IShowStream.cpp:108): forwards stream plus zero unknown word
/// and volume to SetVolume; FAILED is GENERIC else I3D_OK.
pub fn ishow_set_stream_volume(
    state: &IshowStream,
    volume: i32,
    backend: &mut impl IshowStreamBackend,
) -> i32 {
    if !backend.stream_set_volume(state.stream, 0, volume) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x75bc0, VA 0x10075bc0. IShow::SetStreamPan (mafia-re
/// ls3df/IGraph/IShowStream.cpp:115): forwards stream plus zero unknown word
/// and pan to SetPan; FAILED is GENERIC else I3D_OK.
pub fn ishow_set_stream_pan(
    state: &IshowStream,
    pan: i32,
    backend: &mut impl IshowStreamBackend,
) -> i32 {
    if !backend.stream_set_pan(state.stream, 0, pan) {
        return ISOUND_GENERIC;
    }
    ISOUND_OK
}

/// LS3DF RVA 0x74a30 dB-map sub-portion, VA 0x10074a30. IShow::SetVolume
/// evidenced original name (mafia-re ls3df/IGraph/IShow.cpp:909-931): CPU
/// decibel map only. Below marginal zero (plain `vol < MRG_ZERO` compare, so
/// negatives too) yields DSBVOLUME_MIN, else `(int)(log(vol)*866.0)` with a
/// low clamp only (no high clamp, unlike ISND_sound::SetVolume). The x87
/// FYL2X lowering follows the probe's prior REA note (FLDLN2/FLD/FYL2X +
/// FMUL 866.0 + __ftol + clamp at -10000, not re-observed here); no Ghidra
/// proc was re-observed here. The host uses
/// f64::ln*866.0 like the neighboring isound_sound_set_volume/
/// isound_sound_set_panning paths. Host `as i32` saturates where native
/// __ftol/fistp is UB on nonfinite/overflow, disclosed. The full SetVolume
/// stays device-WITHHELD (BinkSetVolume and the DirectSound/BasicAudio
/// submits have no function here).
pub fn ishow_volume_db(vol: f32) -> i32 {
    if vol < 1e-8f32 {
        DSBVOLUME_MIN
    } else {
        ((f64::from(vol).ln() * 866.0) as i32).max(DSBVOLUME_MIN)
    }
}

/// LS3DF RVA 0x75840, VA 0x10075840. IShow::IsStreamPlaying PC-only guessed
/// name (mafia-re ls3df/IGraph/IGraph.h:481-483 `return
/// !m_StreamDone`, stream block marked PC-only): pure-CPU one-liner, no backend. Native int 0/1 is
/// carried as host bool (exact value map, width adaptation disclosed).
/// No Ghidra proc was
/// observed here; byte proof is the negated byte at [ECX+0x5E] (m_StreamDone
/// at +0x5E per IGraph.h:520). Owner lifts the prior withhold.
pub fn ishow_is_stream_playing(state: &IshowStream) -> bool {
    !state.stream_done
}

#[cfg(test)]
mod ishow_stream_contract_tests {
    use super::*;

    struct Rig {
        init_ok: bool,
        open: Option<i32>,
        close_ok: bool,
        close_stream_ok: bool,
        play_ok: bool,
        stop_ok: bool,
        pause_ok: bool,
        resume_ok: bool,
        volume_ok: bool,
        pan_ok: bool,
        calls: Vec<String>,
        opened_name: Option<String>,
    }

    impl Default for Rig {
        fn default() -> Self {
            Self {
                init_ok: true,
                open: Some(7),
                close_ok: true,
                close_stream_ok: true,
                play_ok: true,
                stop_ok: true,
                pause_ok: true,
                resume_ok: true,
                volume_ok: true,
                pan_ok: true,
                calls: Vec::new(),
                opened_name: None,
            }
        }
    }

    impl IshowStreamBackend for Rig {
        fn stream_init(&mut self, unknown: i32) -> bool {
            self.calls.push(format!("init:{unknown}"));
            self.init_ok
        }
        fn stream_open(&mut self, unknown: i32, name: &str, unknown1: i32) -> Option<i32> {
            self.calls.push(format!("open:{unknown}:{name}:{unknown1}"));
            self.opened_name = Some(name.to_string());
            self.open
        }
        fn stream_close(&mut self, unknown: i32) -> bool {
            self.calls.push(format!("close:{unknown}"));
            self.close_ok
        }
        fn stream_close_stream(&mut self, stream: i32) -> bool {
            self.calls.push(format!("close_stream:{stream}"));
            self.close_stream_ok
        }
        fn stream_play(&mut self, stream: i32, looping: bool) -> bool {
            self.calls.push(format!("play:{stream}:{looping}"));
            self.play_ok
        }
        fn stream_stop(&mut self, stream: i32) -> bool {
            self.calls.push(format!("stop:{stream}"));
            self.stop_ok
        }
        fn stream_pause(&mut self, stream: i32) -> bool {
            self.calls.push(format!("pause:{stream}"));
            self.pause_ok
        }
        fn stream_resume(&mut self, stream: i32) -> bool {
            self.calls.push(format!("resume:{stream}"));
            self.resume_ok
        }
        fn stream_set_volume(&mut self, stream: i32, unknown: i32, volume: i32) -> bool {
            self.calls
                .push(format!("volume:{stream}:{unknown}:{volume}"));
            self.volume_ok
        }
        fn stream_set_pan(&mut self, stream: i32, unknown: i32, pan: i32) -> bool {
            self.calls.push(format!("pan:{stream}:{unknown}:{pan}"));
            self.pan_ok
        }
    }

    fn live_stream() -> IshowStream {
        IshowStream {
            stream: 7,
            stream_init: true,
            stream_paused: false,
            stream_done: false,
        }
    }

    #[test]
    fn ishow_open_stream_inits_opens_and_grows_once() {
        let mut rig = Rig::default();
        let mut state = IshowStream {
            stream: -1,
            stream_init: false,
            stream_paused: true,
            stream_done: true,
        };
        let mut bank = IshowStreamBank::default();
        assert_eq!(
            ishow_open_stream(&mut state, 11, &mut bank, "music", 0, &mut rig),
            0
        );
        assert!(state.stream_init);
        assert_eq!(state.stream, 7);
        assert_eq!(bank.shows, vec![11]);
        assert_eq!(rig.opened_name.as_deref(), Some("music"));
        assert_eq!(
            ishow_open_stream(&mut state, 11, &mut bank, "music", 0, &mut rig),
            0
        );
        assert_eq!(bank.shows, vec![11]);
    }

    #[test]
    fn ishow_open_clears_prefix_flags_and_done_is_backend_marked() {
        // Open starts with the CloseStream prefix (paused/done clear) and
        // end-of-stream stays backend-marked (no callback carrier in Open).
        let mut rig = Rig::default();
        let mut state = IshowStream {
            stream: -1,
            stream_init: false,
            stream_paused: true,
            stream_done: true,
        };
        let mut bank = IshowStreamBank::default();
        assert_eq!(
            ishow_open_stream(&mut state, 11, &mut bank, "music", 0, &mut rig),
            0
        );
        assert!(!state.stream_paused);
        assert!(!state.stream_done);
        ishow_stream_mark_done(&mut state);
        assert!(state.stream_done);
    }

    #[test]
    fn ishow_open_stream_maps_init_and_open_errors() {
        let mut bank = IshowStreamBank::default();
        let mut state = IshowStream {
            stream: -1,
            stream_init: false,
            stream_paused: false,
            stream_done: false,
        };
        let mut failing = Rig {
            init_ok: false,
            ..Rig::default()
        };
        assert_eq!(
            ishow_open_stream(&mut state, 11, &mut bank, "music", 0, &mut failing),
            ISOUND_UNSUPPORTED
        );
        assert!(!state.stream_init);
        let mut missing = Rig {
            open: None,
            ..Rig::default()
        };
        assert_eq!(
            ishow_open_stream(&mut state, 11, &mut bank, "music", 0, &mut missing),
            ISOUND_NOFILE
        );
        assert!(state.stream_init);
    }

    #[test]
    fn ishow_close_stream_flags_swap_remove_and_guards() {
        let mut rig = Rig::default();
        let mut state = live_stream();
        state.stream_paused = true;
        state.stream_done = true;
        let mut bank = IshowStreamBank {
            shows: vec![11, 22, 33],
        };
        ishow_close_stream(&mut state, 22, &mut bank, &mut rig);
        assert!(!state.stream_paused);
        assert!(!state.stream_done);
        // swap-remove moves the last element into the hole (C_list::erase
        // semantics), not an order-preserving erase: [11, 22, 33] -> [11, 33].
        assert_eq!(bank.shows, vec![11, 33]);
        assert_eq!(state.stream, -1);
        assert!(!state.stream_init);
        assert!(rig.calls.iter().any(|c| c == "stop:7"));
    }

    #[test]
    fn ishow_close_stream_keeps_handle_on_close_failure() {
        let mut rig = Rig {
            close_stream_ok: false,
            ..Rig::default()
        };
        let mut state = live_stream();
        let mut bank = IshowStreamBank { shows: vec![11] };
        ishow_close_stream(&mut state, 11, &mut bank, &mut rig);
        assert!(bank.shows.is_empty());
        assert_eq!(state.stream, 7);
        assert!(state.stream_init);
    }

    #[test]
    fn ishow_play_stream_paused_branch() {
        let mut rig = Rig::default();
        let mut paused = live_stream();
        paused.stream_paused = true;
        assert_eq!(ishow_play_stream(&mut paused, true, &mut rig), 0);
        assert!(!paused.stream_paused);
        assert!(rig.calls.iter().any(|c| c == "resume:7"));
        let mut fresh = live_stream();
        assert_eq!(ishow_play_stream(&mut fresh, true, &mut rig), 0);
        assert!(rig.calls.iter().any(|c| c == "play:7:true"));
        rig.resume_ok = false;
        let mut failing = live_stream();
        failing.stream_paused = true;
        assert_eq!(
            ishow_play_stream(&mut failing, false, &mut rig),
            ISOUND_GENERIC
        );
        assert!(!failing.stream_paused);
    }

    #[test]
    fn ishow_stop_pause_volume_pan_forwards() {
        let mut rig = Rig::default();
        let state = live_stream();
        assert_eq!(ishow_stop_stream(&state, &mut rig), 0);
        let mut pausable = live_stream();
        assert_eq!(ishow_pause_stream(&mut pausable, &mut rig), 0);
        assert!(pausable.stream_paused);
        assert_eq!(ishow_set_stream_volume(&state, -1000, &mut rig), 0);
        assert_eq!(ishow_set_stream_pan(&state, 1000, &mut rig), 0);
        rig.stop_ok = false;
        assert_eq!(ishow_stop_stream(&state, &mut rig), ISOUND_GENERIC);
        rig.pause_ok = false;
        let mut failing = live_stream();
        assert_eq!(ishow_pause_stream(&mut failing, &mut rig), ISOUND_GENERIC);
        assert!(!failing.stream_paused);
    }

    #[test]
    fn ishow_volume_db_hand_vectors() {
        // 1.0 -> 0 hundredths of a dB; 0.0 and sub-MRG_ZERO -> DSBVOLUME_MIN;
        // 0.5 -> (int)(ln(0.5)*866.0) = (int)(-600.265...) = -600.
        assert_eq!(ishow_volume_db(1.0), 0);
        assert_eq!(ishow_volume_db(0.0), DSBVOLUME_MIN);
        assert_eq!(ishow_volume_db(5e-9f32), DSBVOLUME_MIN);
        assert_eq!(ishow_volume_db(-1.0), DSBVOLUME_MIN);
        assert_eq!(ishow_volume_db(0.5), -600);
        // No high clamp (unlike ISND SetVolume): positive dB passes through.
        assert_eq!(ishow_volume_db(2.0), 600);
        // Strict less-than edge: exactly MRG_ZERO takes the ln arm, whose
        // cast still lands on the low clamp.
        assert_eq!(ishow_volume_db(1e-8f32), DSBVOLUME_MIN);
        assert_eq!(ishow_volume_db(1e-7f32), DSBVOLUME_MIN);
        // NaN takes the ln arm (comparison false); saturating cast gives 0
        // where native __ftol is UB (disclosed).
        assert_eq!(ishow_volume_db(f32::NAN), 0);
    }

    #[test]
    fn ishow_is_stream_playing_negates_done() {
        let mut playing = live_stream();
        playing.stream_done = false;
        assert!(ishow_is_stream_playing(&playing));
        playing.stream_done = true;
        assert!(!ishow_is_stream_playing(&playing));
    }
}
