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
    fn new(handle: u32, frame_owner: Option<SoundFrameOwner>) -> Self {
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
        let mut sound = SoundState::new(handle, owner);
        sound.clock_ms = mixer.clock_ms as u64 as u32;
        mixer.sounds.insert(handle, sound);
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

    /// C_I3D_sound_cache::Duplicate (Game 0x5c23f0) only reopens the
    /// source file. Unlike I3D_sound::Duplicate, it preserves destination
    /// parameters and its common frame, and the cache ignores Open's HRESULT.
    pub(crate) fn reopen_cached_sound(
        &self,
        destination: u32,
        source: u32,
        installation: &Installation,
    ) -> Result<i32> {
        let source = self.sound(source)?;
        let filename = source
            .source
            .as_ref()
            .map_or("", |source| source.name.as_str());
        self.open_original_sound(destination, filename, source.open_flags, installation)
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
        let canonical = canonical_name(name);
        let cached = {
            let mut mixer = self.lock()?;
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
                let Some(bytes) = read()? else {
                    return Ok(false);
                };
                let mut source = PcmSource::from_wav(name, &bytes)?;
                let flags = flags | 0x200 | if flags & 0x8 != 0 { 0x10 } else { 0 };
                source.open_flags = flags & if source.channels > 1 { !0x200 } else { !0 };
                Arc::new(source)
            }
        };
        let mut mixer = self.lock()?;
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
        let expected = match slot {
            0x00 | 0x18 | 0x80 | 0x84 | 0x90 => 0,
            0x24 | 0x54 | 0x58 | 0x64 | 0x68 | 0x6c | 0x70 | 0x74 => 1,
            0x2c | 0x60 | 0x88 => 2,
            0x5c => 4,
            _ => {
                return Err(AudioError(format!(
                    "unsupported sound vtable slot {slot:#x}"
                )));
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
        let camera = mixer.camera_present;
        let sector = mixer.listener_sector;
        let mixer_listener = mixer.listener_position;
        let sound = sound_mut(&mut mixer, handle)?;
        let float = |i| -> Result<f32> {
            let value = f32::from_bits(arguments[i]);
            finite(&[value])?;
            Ok(value)
        };
        match slot {
            0x18 | 0x90 => update_sound(sound, camera, sector, mixer_listener)?,
            0x24 | 0x88 => {
                sound.set_on(arguments[0] != 0, camera, sector);
                if slot == 0x88 && arguments[1] != 0 {
                    update_sound(sound, camera, sector, mixer_listener)?;
                }
            }
            0x2c => sound.parent = arguments[0],
            0x54 => {
                return Ok(u32::from(if arguments[0] != 0 {
                    sound.voice_playing
                } else {
                    sound.is_playing()
                }));
            }
            0x58 => {
                if arguments[0] > 3 {
                    return Err(AudioError("unsupported sound type".into()));
                }
                if sound.sound_type == arguments[0] {
                    return Ok(0);
                }
                sound.native_tail[0] |= 0x100;
                sound.sound_type =
                    if sound.source.as_ref().is_some_and(|s| s.channels > 1) && arguments[0] != 0 {
                        3
                    } else {
                        arguments[0]
                    };
            }
            0x5c => {
                let range = [float(0)?, float(1)?, float(2)?, float(3)?];
                if !near_equal(&sound.range, &range) {
                    sound.range = range;
                    sound.native_tail[0] |= 0x4;
                }
            }
            0x60 => {
                let cone = [float(0)?, float(1)?];
                if !near_equal(&sound.cone, &cone) {
                    sound.cone = cone.map(|angle| angle.min(std::f32::consts::PI));
                    sound.native_tail[0] |= 0x40;
                }
            }
            0x64 => {
                let value = float(0)?;
                if !near_equal(&[sound.out_volume], &[value]) {
                    sound.out_volume = value;
                    sound.native_tail[0] |= 0x10;
                }
            }
            0x68 => {
                let value = float(0)?;
                if !near_equal(&[sound.volume], &[value]) {
                    sound.volume = value;
                    sound.native_tail[0] |= 0x8;
                }
            }
            0x6c => {
                sound.looping = arguments[0] != 0;
                if sound.looping {
                    sound.open_flags |= 0x100;
                } else {
                    sound.open_flags &= !0x100;
                }
                if sound.on {
                    sound.set_on(true, camera, sector);
                }
            }
            0x70 => sound.seek(arguments[0]),
            0x74 => {
                let value = float(0)?;
                if value <= 0.0 {
                    return Err(AudioError("nonpositive sound frequency".into()));
                }
                if !near_equal(&[sound.frequency], &[value]) {
                    sound.frequency = value;
                    sound.native_tail[0] |= 0x20;
                }
            }
            0x80 => return Ok(sound.current_time_ms()),
            0x84 => return Ok(sound.source.as_ref().map_or(0, |s| s.duration_ms())),
            _ => unreachable!(),
        }
        Ok(0)
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
}
