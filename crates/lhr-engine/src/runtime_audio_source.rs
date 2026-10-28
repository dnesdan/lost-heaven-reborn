//! Source loading retains the requested sound incarnation across unlocked I/O.
//! Legacy handles enter through the parent adapter; scene owners supply their
//! retained SoundId. Both routes use this implementation and the same mixer.
#![forbid(unsafe_code)]

use super::super::{canonical_name, sound_mut, Installation, PcmSource};
use super::{AudioError, Mixer, Result, RuntimeAudio, SoundId};
use std::sync::{Arc, Weak};

fn checked_handle(audio: &RuntimeAudio, id: &SoundId, mixer: &Mixer) -> Result<u32> {
    if !Weak::ptr_eq(&id.owner, &Arc::downgrade(&audio.0)) {
        return Err(AudioError(
            "sound belongs to a different audio owner".into(),
        ));
    }
    if !mixer
        .sounds
        .get(&id.handle)
        .is_some_and(|sound| sound.generation == id.generation)
    {
        return Err(AudioError("stale sound identity".into()));
    }
    Ok(id.handle)
}

impl RuntimeAudio {
    /// Original Open with a frame's retained identity. Preserve filename
    /// admission and native missing-file results; other errors interrupt loading.
    pub(crate) fn open_original_sound_id(
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
        let opened = self.load_sound_with_optional_id(id, name, flags, || {
            installation
                .read_optional(&normalized)
                .map_err(|e| AudioError(format!("sound source {name}: {e:?}")))
        })?;
        Ok(if opened { 0 } else { 0x8000_0001u32 as i32 })
    }

    /// C_I3D_sound_cache::Duplicate only reopens the source file. Hold its
    /// existing PCM reference across Open, including when source == destination;
    /// do not copy frame identity, volume or other destination parameters.
    pub(crate) fn reopen_cached_sound(
        &self,
        destination: &SoundId,
        source: &SoundId,
        installation: &Installation,
    ) -> Result<i32> {
        let (pcm, flags) = {
            let mixer = self.lock()?;
            let handle = checked_handle(self, source, &mixer)?;
            let sound = &mixer.sounds[&handle];
            (sound.source.clone(), sound.open_flags)
        };
        let filename = pcm.as_ref().map_or("", |source| source.name.as_str());
        self.open_original_sound_id(destination, filename, flags, installation)
    }

    /// One shared source-loading path. The reader runs without the mixer lock:
    /// it may fail, mutate live parameters or release/recreate the target.
    /// Revalidate before publishing any source or touching a replacement sound.
    /// This is an incarnation guard, not serialization of concurrent Opens on
    /// the same still-live sound; callers must order those operations.
    pub(in crate::runtime_audio) fn load_sound_with_optional_id(
        &self,
        id: &SoundId,
        name: &str,
        flags: u32,
        read: impl FnOnce() -> Result<Option<Vec<u8>>>,
    ) -> Result<bool> {
        let canonical = canonical_name(name);
        let cached = {
            let mut mixer = self.lock()?;
            let handle = checked_handle(self, id, &mixer)?;
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
                    // Absence is a native result only while the requested
                    // incarnation is still alive. Never turn a stale ID into it.
                    let mixer = self.lock()?;
                    checked_handle(self, id, &mixer)?;
                    return Ok(false);
                };
                let mut source = PcmSource::from_wav(name, &bytes)?;
                let flags = flags | 0x200 | if flags & 0x8 != 0 { 0x10 } else { 0 };
                source.open_flags = flags & if source.channels > 1 { !0x200 } else { !0 };
                Arc::new(source)
            }
        };
        let mut mixer = self.lock()?;
        let handle = checked_handle(self, id, &mixer)?;
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
}

#[cfg(test)]
#[path = "runtime_audio_source_tests.rs"]
mod tests;
