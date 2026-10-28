//! Named operations over the existing mixer, not a second sound world.
//! Legacy numeric slots are decoded only at the compatibility entry below.
#![forbid(unsafe_code)]

use super::{finite, near_equal, sound_mut, update_sound, AudioError, Mixer, Result, RuntimeAudio};
use super::{SoundFrameOwner, SoundState};
use crate::frame_transform::FrameKey;
use std::sync::{Arc, Mutex, Weak};

#[path = "runtime_audio_source.rs"]
mod source;

/// Borrowed identity of one sound incarnation in one mixer. Cloning an ID does
/// not retain a voice, PCM source or mixer. A released/reused handle stays stale.
#[derive(Clone, Debug)]
pub struct SoundId {
    owner: Weak<Mutex<Mixer>>,
    handle: u32,
    generation: u64,
}

impl PartialEq for SoundId {
    fn eq(&self, other: &Self) -> bool {
        self.handle == other.handle
            && self.generation == other.generation
            && Weak::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for SoundId {}

/// Descriptive names for the existing runtime's four source sound kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundKind {
    Null = 0,
    Point = 1,
    Spot = 2,
    Ambient = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackQuery {
    /// Logical playback lifetime, including a paused voice's elapsed time.
    Logical,
    /// Whether the host PCM voice is currently running.
    Voice,
}

#[derive(Clone, Copy)]
enum Command {
    Release,
    Validate,
    AddSector(FrameKey),
    DeleteSector(FrameKey),
    Update,
    Enable { enabled: bool, immediate: bool },
    LinkParent(u32),
    IsPlaying(PlaybackQuery),
    // Raw kind is confined to binary/legacy adapters; validate after identity lookup.
    Kind(u32),
    Range([f32; 4]),
    Cone([f32; 2]),
    OutsideVolume(f32),
    Volume(f32),
    Loop(bool),
    Seek(u32),
    Frequency(f32),
    Position,
    Duration,
}

impl RuntimeAudio {
    /// Scene factory boundary: construct the sound and return its incarnation
    /// under one lock, before the frame can publish it to the driver registry.
    /// Legacy projected factories share this mixer's counter and SoundState
    /// constructor; they must not allocate identities in a separate namespace.
    pub(crate) fn create_scene_sound(&self, frame: FrameKey) -> Result<SoundId> {
        let mut mixer = self.lock()?;
        let handle = frame.0;
        if handle == 0 || mixer.sounds.contains_key(&handle) {
            return Err(AudioError("invalid or already owned sound address".into()));
        }
        let generation = mixer
            .next_sound_generation
            .checked_add(1)
            .ok_or_else(|| AudioError("sound identity generation exhausted".into()))?;
        let mut sound = SoundState::new(handle, Some(SoundFrameOwner::Scene(frame)), generation);
        sound.clock_ms = mixer.clock_ms as u64 as u32;
        let id = SoundId {
            owner: Arc::downgrade(&self.0),
            handle,
            generation,
        };
        mixer.sounds.insert(handle, sound);
        mixer.next_sound_generation = generation;
        Ok(id)
    }

    /// Reversible missing-owner fault for lifecycle tests. Preserve the exact
    /// state and generation; the callback runs without holding the mixer lock.
    #[cfg(test)]
    pub(crate) fn with_sound_temporarily_missing<R>(
        &self,
        id: &SoundId,
        callback: impl FnOnce() -> R,
    ) -> R {
        assert!(Weak::ptr_eq(&id.owner, &Arc::downgrade(&self.0)));
        let sound = {
            let mut mixer = self.lock().unwrap();
            assert_eq!(
                mixer.sounds.get(&id.handle).unwrap().generation,
                id.generation
            );
            mixer.sounds.remove(&id.handle).unwrap()
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback));
        {
            let mut mixer = self.lock().unwrap();
            assert!(!mixer.sounds.contains_key(&id.handle));
            mixer.sounds.insert(id.handle, sound);
        }
        match outcome {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Read-only preflight, not a lease. Each subsequent operation revalidates
    /// the retained ID under its own lock before touching the current sound.
    pub(crate) fn validate_sound_id(&self, id: &SoundId) -> Result<()> {
        self.sound_command(id, Command::Validate).map(|_| ())
    }

    /// The scene owner validates sector ownership and maintains reciprocal
    /// lists. These methods only update this incarnation's PCM membership.
    pub(crate) fn add_sound_sector(&self, id: &SoundId, sector: FrameKey) -> Result<()> {
        self.sound_command(id, Command::AddSector(sector))
            .map(|_| ())
    }

    pub(crate) fn delete_sound_sector(&self, id: &SoundId, sector: FrameKey) -> Result<()> {
        self.sound_command(id, Command::DeleteSector(sector))
            .map(|_| ())
    }

    /// Compatibility bridge for a handle issued by the existing frame factory.
    /// Capture its incarnation once; semantic consumers should retain this ID,
    /// not repeatedly turn an old integer into an ID after a possible release.
    pub fn sound_id_from_handle(&self, handle: u32) -> Result<SoundId> {
        let mixer = self.lock()?;
        let sound = mixer
            .sounds
            .get(&handle)
            .ok_or_else(|| AudioError(format!("unowned sound {handle:#x}")))?;
        Ok(SoundId {
            owner: Arc::downgrade(&self.0),
            handle,
            generation: sound.generation,
        })
    }

    fn sound_command(&self, id: &SoundId, command: Command) -> Result<u32> {
        if !Weak::ptr_eq(&id.owner, &Arc::downgrade(&self.0)) {
            return Err(AudioError(
                "sound belongs to a different audio owner".into(),
            ));
        }
        let mut mixer = self.lock()?;
        if !mixer
            .sounds
            .get(&id.handle)
            .is_some_and(|sound| sound.generation == id.generation)
        {
            return Err(AudioError("stale sound identity".into()));
        }
        // Liveness validation and the operation share this lock. No raw-handle
        // lookup, release or reuse can intervene between them.
        apply(&mut mixer, id.handle, command)
    }

    /// Release only when the caller owns the existing frame's release operation.
    /// SoundId itself is non-owning and never releases anything on Drop.
    pub fn release_sound(&self, id: &SoundId) -> Result<()> {
        self.sound_command(id, Command::Release).map(|_| ())
    }
    pub fn update_sound(&self, id: &SoundId) -> Result<()> {
        self.sound_command(id, Command::Update).map(|_| ())
    }
    pub fn set_sound_enabled(&self, id: &SoundId, enabled: bool) -> Result<()> {
        self.set_sound_playback(id, enabled, false)
    }
    /// Immediate update matches Play's second argument; it is not implicit in
    /// SetOn. An update failure keeps the preceding SetOn effect, as before.
    pub fn set_sound_playback(&self, id: &SoundId, enabled: bool, immediate: bool) -> Result<()> {
        self.sound_command(id, Command::Enable { enabled, immediate })
            .map(|_| ())
    }
    pub fn sound_is_playing(&self, id: &SoundId, query: PlaybackQuery) -> Result<bool> {
        self.sound_command(id, Command::IsPlaying(query))
            .map(|value| value != 0)
    }
    pub fn set_sound_kind(&self, id: &SoundId, kind: SoundKind) -> Result<()> {
        self.sound_command(id, Command::Kind(kind as u32))
            .map(|_| ())
    }
    /// Binary scene-record boundary. Delay raw-kind validation until the same
    /// locked identity check used by the other setters; a bad record must not
    /// hide a stale incarnation. Semantic callers use set_sound_kind instead.
    pub(crate) fn set_sound_kind_from_record(&self, id: &SoundId, kind: u32) -> Result<()> {
        self.sound_command(id, Command::Kind(kind)).map(|_| ())
    }
    /// Original parameter order: near, far, middle fraction, middle volume.
    /// Preserve existing finite checks; do not invent clamping or sorting.
    pub fn set_sound_range(&self, id: &SoundId, range: [f32; 4]) -> Result<()> {
        self.sound_command(id, Command::Range(range)).map(|_| ())
    }
    /// Inner and outer angles in radians, retaining the existing upper PI cap.
    pub fn set_sound_cone(&self, id: &SoundId, cone: [f32; 2]) -> Result<()> {
        self.sound_command(id, Command::Cone(cone)).map(|_| ())
    }
    pub fn set_sound_outside_volume(&self, id: &SoundId, volume: f32) -> Result<()> {
        self.sound_command(id, Command::OutsideVolume(volume))
            .map(|_| ())
    }
    pub fn set_sound_volume(&self, id: &SoundId, volume: f32) -> Result<()> {
        self.sound_command(id, Command::Volume(volume)).map(|_| ())
    }
    pub fn set_sound_looping(&self, id: &SoundId, looping: bool) -> Result<()> {
        self.sound_command(id, Command::Loop(looping)).map(|_| ())
    }
    pub fn seek_sound_ms(&self, id: &SoundId, milliseconds: u32) -> Result<()> {
        self.sound_command(id, Command::Seek(milliseconds))
            .map(|_| ())
    }
    pub fn set_sound_frequency(&self, id: &SoundId, multiplier: f32) -> Result<()> {
        self.sound_command(id, Command::Frequency(multiplier))
            .map(|_| ())
    }
    pub fn sound_position_ms(&self, id: &SoundId) -> Result<u32> {
        self.sound_command(id, Command::Position)
    }
    pub fn sound_duration_ms(&self, id: &SoundId) -> Result<u32> {
        self.sound_command(id, Command::Duration)
    }

    pub(super) fn legacy_sound_call(&self, handle: u32, slot: u32, words: &[u32]) -> Result<u32> {
        let count = match slot {
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
        if words.len() != count {
            return Err(AudioError(format!("sound slot {slot:#x} argument count")));
        }
        let float = |index| f32::from_bits(words[index]);
        let command = match slot {
            0x00 => Command::Release,
            0x18 | 0x90 => Command::Update,
            0x24 => Command::Enable {
                enabled: words[0] != 0,
                immediate: false,
            },
            0x88 => Command::Enable {
                enabled: words[0] != 0,
                immediate: words[1] != 0,
            },
            0x2c => Command::LinkParent(words[0]),
            0x54 => Command::IsPlaying(if words[0] != 0 {
                PlaybackQuery::Voice
            } else {
                PlaybackQuery::Logical
            }),
            0x58 => Command::Kind(words[0]),
            0x5c => Command::Range([float(0), float(1), float(2), float(3)]),
            0x60 => Command::Cone([float(0), float(1)]),
            0x64 => Command::OutsideVolume(float(0)),
            0x68 => Command::Volume(float(0)),
            0x6c => Command::Loop(words[0] != 0),
            0x70 => Command::Seek(words[0]),
            0x74 => Command::Frequency(float(0)),
            0x80 => Command::Position,
            0x84 => Command::Duration,
            _ => unreachable!(),
        };
        // Decoding does not validate floats/types or mutate state. Preserve the
        // original precedence: slot/count, mutex, handle, then parameter checks.
        let mut mixer = self.lock()?;
        apply(&mut mixer, handle, command)
    }
}

fn apply(mixer: &mut Mixer, handle: u32, command: Command) -> Result<u32> {
    if matches!(command, Command::Release) {
        mixer
            .sounds
            .remove(&handle)
            .ok_or_else(|| AudioError("release of unowned sound".into()))?;
        mixer.sources.retain(|_, source| source.strong_count() != 0);
        return Ok(0);
    }
    let camera = mixer.camera_present;
    let sector = mixer.listener_sector;
    let listener = mixer.listener_position;
    let sound = sound_mut(mixer, handle)?;
    match command {
        Command::Release => unreachable!(),
        Command::Validate => {}
        Command::AddSector(sector) => {
            sound.sectors.insert(sector.0);
        }
        Command::DeleteSector(sector) => {
            sound.sectors.remove(&sector.0);
        }
        Command::Update => update_sound(sound, camera, sector, listener)?,
        Command::Enable { enabled, immediate } => {
            sound.set_on(enabled, camera, sector);
            if immediate {
                update_sound(sound, camera, sector, listener)?;
            }
        }
        Command::LinkParent(parent) => sound.parent = parent,
        Command::IsPlaying(query) => {
            return Ok(u32::from(match query {
                PlaybackQuery::Voice => sound.voice_playing,
                PlaybackQuery::Logical => sound.is_playing(),
            }));
        }
        Command::Kind(kind) => {
            if kind > 3 {
                return Err(AudioError("unsupported sound type".into()));
            }
            if sound.sound_type == kind {
                return Ok(0);
            }
            sound.native_tail[0] |= 0x100;
            sound.sound_type = if sound
                .source
                .as_ref()
                .is_some_and(|source| source.channels > 1)
                && kind != 0
            {
                3
            } else {
                kind
            };
        }
        Command::Range(range) => {
            finite(&range)?;
            if !near_equal(&sound.range, &range) {
                sound.range = range;
                sound.native_tail[0] |= 0x4;
            }
        }
        Command::Cone(cone) => {
            finite(&cone)?;
            if !near_equal(&sound.cone, &cone) {
                sound.cone = cone.map(|angle| angle.min(std::f32::consts::PI));
                sound.native_tail[0] |= 0x40;
            }
        }
        Command::OutsideVolume(volume) => {
            finite(&[volume])?;
            if !near_equal(&[sound.out_volume], &[volume]) {
                sound.out_volume = volume;
                sound.native_tail[0] |= 0x10;
            }
        }
        Command::Volume(volume) => {
            finite(&[volume])?;
            if !near_equal(&[sound.volume], &[volume]) {
                sound.volume = volume;
                sound.native_tail[0] |= 0x8;
            }
        }
        Command::Loop(looping) => {
            sound.looping = looping;
            if looping {
                sound.open_flags |= 0x100;
            } else {
                sound.open_flags &= !0x100;
            }
            if sound.on {
                sound.set_on(true, camera, sector);
            }
        }
        Command::Seek(milliseconds) => sound.seek(milliseconds),
        Command::Frequency(frequency) => {
            finite(&[frequency])?;
            if frequency <= 0.0 {
                return Err(AudioError("nonpositive sound frequency".into()));
            }
            if !near_equal(&[sound.frequency], &[frequency]) {
                sound.frequency = frequency;
                sound.native_tail[0] |= 0x20;
            }
        }
        Command::Position => return Ok(sound.current_time_ms()),
        Command::Duration => {
            return Ok(sound
                .source
                .as_ref()
                .map_or(0, |source| source.duration_ms()));
        }
    }
    Ok(0)
}

#[cfg(test)]
#[path = "runtime_audio_api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_audio_scene_tests.rs"]
mod scene_tests;
