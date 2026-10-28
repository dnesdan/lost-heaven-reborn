//! Source-backed I3D_sound frame owner for ordered Game scene2 loading.
//! LS3DF.dll SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Native factory RVA 0x193e0 case 4 allocates 0x1ec, constructs at 0x57650,
//! then appends the frame to the driver's borrowed sounds list. Derived
//! destructor 0x577e0 removes reciprocal sector/AREA links and that list,
//! then releases source and voice before base frame destruction. RuntimeAudio
//! owns host PCM source/voice storage; it is not an original DirectSound object.
#![forbid(unsafe_code)]

use crate::frame_area::AreaKey;
use crate::frame_transform::FrameKey;
use crate::model_loading::{ModelArena, ModelError};
use crate::runtime_audio::{AudioError, RuntimeAudio, SoundId};
use crate::scene_ownership::SectorKey;
use crate::script_scheduler::FrameReferenceId;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoundKey {
    frame: FrameKey,
    reference: FrameReferenceId,
}
impl SoundKey {
    pub fn frame(self) -> FrameKey {
        self.frame
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SoundError {
    ForeignArena,
    UnknownSound(FrameKey),
    BackendUnavailable,
    ForeignDriver,
    RegistryUnavailable,
    MissingRegistryMember(FrameKey),
    AllocationUnavailable,
    Audio(String),
}
impl From<AudioError> for SoundError {
    fn from(error: AudioError) -> Self {
        Self::Audio(error.to_string())
    }
}
impl From<SoundError> for ModelError {
    fn from(error: SoundError) -> Self {
        Self::Sound(error)
    }
}
#[derive(Debug)]
struct SoundDriverList {
    frames: Vec<SoundKey>,
    listeners: Vec<crate::scene_ownership::SceneKey>,
}
/// An explicit initialized host PCM backend. Clones borrow the same sound
/// registry and source/voice owner; no synthetic native pointer is exposed.
#[derive(Clone, Debug)]
pub struct HostSoundBackend {
    driver_identity: u64,
    pub(crate) audio: RuntimeAudio,
    registry: Arc<Mutex<SoundDriverList>>,
}
impl HostSoundBackend {
    pub(crate) fn uses_audio(&self, audio: &RuntimeAudio) -> bool {
        self.audio.same_owner(audio)
    }
    pub(crate) fn same_owner(&self, other: &Self) -> bool {
        self.driver_identity == other.driver_identity
            && Arc::ptr_eq(&self.registry, &other.registry)
            && self.audio.same_owner(&other.audio)
    }
    pub(crate) fn new(driver_identity: u64, audio: &RuntimeAudio) -> Self {
        Self {
            driver_identity,
            audio: audio.clone(),
            registry: Arc::new(Mutex::new(SoundDriverList {
                frames: Vec::new(),
                listeners: Vec::new(),
            })),
        }
    }
    pub fn driver_identity(&self) -> u64 {
        self.driver_identity
    }
    pub fn sounds(&self) -> Result<Vec<SoundKey>, SoundError> {
        Ok(self
            .registry
            .lock()
            .map_err(|_| SoundError::RegistryUnavailable)?
            .frames
            .clone())
    }
    pub fn listeners(&self) -> Result<Vec<crate::scene_ownership::SceneKey>, SoundError> {
        Ok(self
            .registry
            .lock()
            .map_err(|_| SoundError::RegistryUnavailable)?
            .listeners
            .clone())
    }
    pub(crate) fn create_listener(
        &self,
        scene: crate::scene_ownership::SceneKey,
    ) -> Result<(), SoundError> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| SoundError::RegistryUnavailable)?;
        registry
            .listeners
            .try_reserve(1)
            .map_err(|_| SoundError::AllocationUnavailable)?;
        registry.listeners.push(scene);
        Ok(())
    }
    pub(crate) fn release_listener(
        &self,
        scene: crate::scene_ownership::SceneKey,
    ) -> Result<(), SoundError> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| SoundError::RegistryUnavailable)?;
        let index = registry
            .listeners
            .iter()
            .position(|&candidate| candidate == scene)
            .ok_or(SoundError::RegistryUnavailable)?;
        registry.listeners.swap_remove(index);
        Ok(())
    }
    fn register(&self, key: SoundKey) -> Result<(), SoundError> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| SoundError::RegistryUnavailable)?;
        registry
            .frames
            .try_reserve(1)
            .map_err(|_| SoundError::AllocationUnavailable)?;
        registry.frames.push(key);
        Ok(())
    }
    fn check_member(&self, key: SoundKey) -> Result<(), SoundError> {
        if self
            .registry
            .lock()
            .map_err(|_| SoundError::RegistryUnavailable)?
            .frames
            .contains(&key)
        {
            Ok(())
        } else {
            Err(SoundError::MissingRegistryMember(key.frame))
        }
    }
    fn remove(&self, key: SoundKey) -> Result<(), SoundError> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| SoundError::RegistryUnavailable)?;
        let index = registry
            .frames
            .iter()
            .position(|&member| member == key)
            .ok_or(SoundError::MissingRegistryMember(key.frame))?;
        registry.frames.swap_remove(index);
        Ok(())
    }
}
#[derive(Debug)]
pub struct SoundFrame {
    pub key: SoundKey,
    audio_id: SoundId,
    registry_member: bool,
    pub(crate) sectors: Vec<SectorKey>,
    pub(crate) areas: Vec<AreaKey>,
}
impl SoundFrame {
    fn new(key: SoundKey, audio_id: SoundId) -> Self {
        Self {
            key,
            audio_id,
            registry_member: false,
            sectors: Vec::new(),
            areas: Vec::new(),
        }
    }
    /// The original mixer incarnation, never refreshed from the frame number.
    /// Borrowing/cloning this token does not retain a PCM source or voice.
    pub fn audio_id(&self) -> &SoundId {
        &self.audio_id
    }
    pub fn sectors(&self) -> &[SectorKey] {
        &self.sectors
    }
    pub fn areas(&self) -> &[AreaKey] {
        &self.areas
    }
}
/// A failed factory append retains the real constructed frame so the caller
/// can keep its creator reference as an inspectable interrupted prefix.
#[derive(Debug)]
pub struct SoundCreateFailure {
    pub frame: Option<SoundKey>,
    pub error: ModelError,
}
impl ModelArena {
    pub(crate) fn bind_host_sound_backend(
        &mut self,
        backend: HostSoundBackend,
    ) -> Result<(), ModelError> {
        if let Some(bound) = &self.sound_backend {
            return if bound.same_owner(&backend) {
                Ok(())
            } else {
                Err(SoundError::ForeignDriver.into())
            };
        }
        self.sound_backend = Some(backend);
        Ok(())
    }
    pub(crate) fn host_sound_backend(&self) -> Result<&HostSoundBackend, ModelError> {
        self.sound_backend
            .as_ref()
            .ok_or_else(|| SoundError::BackendUnavailable.into())
    }
    pub fn sound_key(&self, frame: FrameKey) -> Result<SoundKey, ModelError> {
        self.frame(frame)?;
        self.sound_frames
            .iter()
            .find(|sound| sound.key.frame == frame)
            .map(|sound| sound.key)
            .ok_or_else(|| SoundError::UnknownSound(frame).into())
    }
    pub fn sound(&self, key: SoundKey) -> Result<&SoundFrame, ModelError> {
        if self.lookup_reference(key.frame) != key.reference {
            return Err(SoundError::ForeignArena.into());
        }
        self.frame(key.frame)?;
        self.sound_frames
            .iter()
            .find(|sound| sound.key == key)
            .ok_or_else(|| SoundError::UnknownSound(key.frame).into())
    }
    /// Reached common I3D_frame::Duplicate 0x1c5b0 after the derived sound
    /// copy. Scene2 sound records have constructor-null properties/callbacks.
    /// Unsupported populated property/callback ownership stops before writes.
    pub fn duplicate_sound_frame_base(
        &mut self,
        destination: SoundKey,
        source: SoundKey,
    ) -> Result<i32, ModelError> {
        self.sound(destination)?;
        self.sound(source)?;
        let from = self.frame(source.frame)?.clone();
        let to = self.frame(destination.frame)?.clone();
        if !from.properties.is_empty() || !to.properties.is_empty() {
            return Err(SoundError::BackendUnavailable.into());
        }
        let source_state = *self.state(source.frame)?;
        let old_flags = self.state(destination.frame)?.flags;
        let source_name = (!from.name_is_null).then_some(from.name_bytes.clone());
        // Base Duplicate 0x1c5b0 always frees and allocates the destination
        // name. The host SetName fast path cannot reproduce that effect when
        // bytes compare equal, so retain the reached boundary explicitly.
        if source_name.as_deref() == (!to.name_is_null).then_some(to.name_bytes.as_slice()) {
            return Err(SoundError::BackendUnavailable.into());
        }
        let target_state = self.graph.projected_state_mut(destination.frame)?;
        target_state.local = source_state.local;
        target_state.scale = source_state.scale;
        target_state.rotation = source_state.rotation;
        target_state.flags =
            (source_state.flags & 0x0fa7_3e9f) | (old_flags & 0x40c000) | 0x4000_0000;
        target_state.world_scale = source_state.world_scale;
        target_state.update_count = target_state.update_count.wrapping_add(1);
        self.set_frame_name(destination.frame, source_name.as_deref())?;
        Ok(0)
    }
    pub fn create_owned_sound_frame(&mut self) -> Result<SoundKey, SoundCreateFailure> {
        let backend = self
            .host_sound_backend()
            .map_err(|error| SoundCreateFailure { frame: None, error })?
            .clone();
        let frame = self
            .create_sound_frame()
            .map_err(|error| SoundCreateFailure { frame: None, error })?;
        let key = SoundKey {
            frame,
            reference: self.lookup_reference(frame),
        };
        if self.sound_frames.try_reserve(1).is_err() {
            return Err(SoundCreateFailure {
                frame: Some(key),
                error: SoundError::AllocationUnavailable.into(),
            });
        }
        let audio_id =
            backend
                .audio
                .create_scene_sound(frame)
                .map_err(|error| SoundCreateFailure {
                    frame: Some(key),
                    error: SoundError::from(error).into(),
                })?;
        self.sound_frames.push(SoundFrame::new(key, audio_id));
        if let Err(error) = backend.register(key) {
            return Err(SoundCreateFailure {
                frame: Some(key),
                error: error.into(),
            });
        }
        self.sound_frames.last_mut().unwrap().registry_member = true;
        Ok(key)
    }
    pub(crate) fn check_sound_destruction(&self, key: SoundKey) -> Result<(), ModelError> {
        let sound = self.sound(key)?;
        let backend = self.host_sound_backend()?;
        if sound.registry_member {
            backend.check_member(key)?;
        }
        // Preserve the existing preflight error while rejecting a replacement
        // at the same legacy handle before any stop/unlink/release effects.
        backend
            .audio
            .validate_sound_id(&sound.audio_id)
            .map_err(|_| SoundError::BackendUnavailable)?;
        for &area in &sound.areas {
            self.area(area)?;
        }
        for &sector in &sound.sectors {
            self.sector(sector)?;
        }
        Ok(())
    }
    pub(crate) fn destroy_sound_derived(&mut self, key: SoundKey) -> Result<(), ModelError> {
        self.check_sound_destruction(key)?;
        let backend = self.host_sound_backend()?.clone();
        let audio_id = self.sound(key)?.audio_id.clone();
        backend
            .audio
            .set_sound_enabled(&audio_id, false)
            .map_err(SoundError::from)?;
        let position = self
            .sound_frames
            .iter()
            .position(|sound| sound.key == key)
            .unwrap();
        let sectors = self.sound_frames[position].sectors.clone();
        let areas = self.sound_frames[position].areas.clone();
        for sector in sectors {
            self.delete_sound_from_sector(sector, key)?;
        }
        for area in areas {
            self.delete_sound_from_area(area, key)?;
        }
        if self.sound_frames[position].registry_member {
            backend.remove(key)?;
        }
        backend
            .audio
            .release_sound(&audio_id)
            .map_err(SoundError::from)?;
        self.sound_frames.remove(position);
        Ok(())
    }
    pub fn add_sound_to_area(&mut self, area: AreaKey, sound: SoundKey) -> Result<(), ModelError> {
        self.area(area)?;
        self.sound(sound)?;
        self.area_mut(area)?.unknown_164 = -1;
        if self.area(area)?.sounds.contains(&sound.frame) {
            return Ok(());
        }
        self.area_mut(area)?
            .sounds
            .try_reserve(1)
            .map_err(|_| SoundError::AllocationUnavailable)?;
        self.area_mut(area)?.sounds.push(sound.frame);
        let owner = self
            .sound_frames
            .iter_mut()
            .find(|frame| frame.key == sound)
            .unwrap();
        if !owner.areas.contains(&area) {
            owner
                .areas
                .try_reserve(1)
                .map_err(|_| SoundError::AllocationUnavailable)?;
            owner.areas.push(area);
        }
        Ok(())
    }
    /// AREA DeleteSound updates both borrowed lists; its changed marker is
    /// written even when the requested sound was absent.
    pub fn delete_sound_from_area(
        &mut self,
        area: AreaKey,
        sound: SoundKey,
    ) -> Result<(), ModelError> {
        self.area(area)?;
        self.sound(sound)?;
        self.area_mut(area)?.unknown_164 = -1;
        let area_frame = self.area_mut(area)?;
        let index = area_frame
            .sounds
            .iter()
            .position(|&member| member == sound.frame)
            .ok_or(SoundError::UnknownSound(sound.frame))?;
        area_frame.sounds.swap_remove(index);
        let owner = self
            .sound_frames
            .iter_mut()
            .find(|frame| frame.key == sound)
            .unwrap();
        owner.areas.retain(|&member| member != area);
        Ok(())
    }
    pub fn add_sound_to_sector(
        &mut self,
        sector: SectorKey,
        sound: SoundKey,
    ) -> Result<(), ModelError> {
        self.sector(sector)?;
        self.sound(sound)?;
        let sector_index = self.sector_index(sector)?;
        if self.sectors[sector_index].sounds.contains(&sound.frame) {
            return Ok(());
        }
        self.sectors[sector_index]
            .sounds
            .try_reserve(1)
            .map_err(|_| SoundError::AllocationUnavailable)?;
        self.sectors[sector_index].sounds.push(sound.frame);
        let owner = self
            .sound_frames
            .iter_mut()
            .find(|frame| frame.key == sound)
            .unwrap();
        if !owner.sectors.contains(&sector) {
            owner
                .sectors
                .try_reserve(1)
                .map_err(|_| SoundError::AllocationUnavailable)?;
            owner.sectors.push(sector);
        }
        let audio_id = owner.audio_id.clone();
        self.host_sound_backend()?
            .audio
            .add_sound_sector(&audio_id, sector.frame())
            .map_err(SoundError::from)?;
        if self.state(sound.frame)?.flags & 1 != 0 {
            self.host_sound_backend()?
                .audio
                .set_sound_enabled(&audio_id, true)
                .map_err(SoundError::from)?;
        }
        Ok(())
    }
    pub fn delete_sound_from_sector(
        &mut self,
        sector: SectorKey,
        sound: SoundKey,
    ) -> Result<(), ModelError> {
        self.sector(sector)?;
        self.sound(sound)?;
        let index = self.sector_index(sector)?;
        let member = self.sectors[index]
            .sounds
            .iter()
            .position(|&candidate| candidate == sound.frame)
            .ok_or(SoundError::UnknownSound(sound.frame))?;
        let owner = self
            .sound_frames
            .iter_mut()
            .find(|frame| frame.key == sound)
            .unwrap();
        let reciprocal = owner
            .sectors
            .iter()
            .position(|&candidate| candidate == sector);
        if let Some(at) = reciprocal {
            owner.sectors.swap_remove(at);
        }
        let audio_id = owner.audio_id.clone();
        self.host_sound_backend()?
            .audio
            .delete_sound_sector(&audio_id, sector.frame())
            .map_err(SoundError::from)?;
        if reciprocal.is_some() && !self.sound(sound)?.sectors.is_empty() {
            if let Some(scene) = self.sector(sector)?.scene {
                if let Some(camera) = self.scene(scene)?.current_camera {
                    if self.camera(camera)?.sector == Some(sector) {
                        self.host_sound_backend()?
                            .audio
                            .set_sound_enabled(&audio_id, false)
                            .map_err(SoundError::from)?;
                    }
                }
            }
        }
        self.sectors[index].sounds.swap_remove(member);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_links_return_before_reciprocal_repair_or_sound_restart() {
        let mut arena = ModelArena::default();
        let audio = RuntimeAudio::default();
        arena
            .bind_host_sound_backend(HostSoundBackend::new(1, &audio))
            .unwrap();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let area = arena.create_area().unwrap();
        let sound = arena.create_owned_sound_frame().unwrap();
        arena.add_sound_to_sector(sector, sound).unwrap();
        arena.add_sound_to_area(area, sound).unwrap();
        let owner = arena
            .sound_frames
            .iter_mut()
            .find(|owner| owner.key == sound)
            .unwrap();
        owner.sectors.clear();
        owner.areas.clear();
        arena.area_mut(area).unwrap().unknown_164 = 0;

        arena.add_sound_to_sector(sector, sound).unwrap();
        arena.add_sound_to_area(area, sound).unwrap();
        assert!(arena.sound(sound).unwrap().sectors().is_empty());
        assert!(arena.sound(sound).unwrap().areas().is_empty());
        assert_eq!(arena.sector(sector).unwrap().sounds, [sound.frame()]);
        assert_eq!(arena.area(area).unwrap().sounds(), [sound.frame()]);
        assert_eq!(arena.area(area).unwrap().unknown_164, -1);

        arena.delete_sound_from_sector(sector, sound).unwrap();
        arena.delete_sound_from_area(area, sound).unwrap();
        arena.release(sound.frame()).unwrap();
        arena.release(area.frame()).unwrap();
        assert_eq!(audio.live_sound_count().unwrap(), 0);
    }

    #[test]
    fn sector_release_failure_retains_area_link_until_sound_destruction_resumes() {
        let mut arena = ModelArena::default();
        let audio = RuntimeAudio::default();
        arena
            .bind_host_sound_backend(HostSoundBackend::new(1, &audio))
            .unwrap();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let area = arena.create_area().unwrap();
        let sound = arena.create_owned_sound_frame().unwrap();
        arena.add_sound_to_sector(sector, sound).unwrap();
        arena.add_sound_to_area(area, sound).unwrap();

        // Model a reached reciprocal-list failure, after the native sound
        // SetOn(false) effect. The destructor visits Sector before AREA.
        arena
            .sectors
            .iter_mut()
            .find(|owner| owner.key == sector)
            .unwrap()
            .sounds
            .clear();
        assert!(matches!(
            arena.release(sound.frame()),
            Err(ModelError::Sound(SoundError::UnknownSound(frame))) if frame == sound.frame()
        ));
        assert_eq!(arena.sound(sound).unwrap().areas(), &[area]);
        assert_eq!(arena.area(area).unwrap().sounds(), &[sound.frame()]);
        assert_eq!(audio.live_sound_count().unwrap(), 1);

        arena
            .sectors
            .iter_mut()
            .find(|owner| owner.key == sector)
            .unwrap()
            .sounds
            .push(sound.frame());
        arena.release(sound.frame()).unwrap();
        assert!(arena.area(area).unwrap().sounds().is_empty());
        assert_eq!(audio.live_sound_count().unwrap(), 0);
        arena.release(area.frame()).unwrap();
    }
}

#[cfg(test)]
#[path = "frame_sound_identity_tests.rs"]
mod identity_tests;
