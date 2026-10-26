//! Sound frames use the shared ModelArena common fields. Only derived sound
//! state lives in RuntimeAudio; canonical source/name allocations use the host.
use super::*;
use crate::runtime_audio::{AudioError, RuntimeAudio, SoundFrameOwner, SourceProjection};

/// Refresh the positions of every sound after its actual parent graph moves.
/// Scene sounds and vehicle effects retain their distinct live frame owners.
pub fn sync_runtime_audio_frames(
    audio: &RuntimeAudio,
    arena: &mut ModelArena,
) -> Result<(), AudioError> {
    for address in audio.sound_handles()? {
        let owner = audio.sound_frame_owner(address)?;
        let key = match owner {
            SoundFrameOwner::Scene(key) => {
                if address != key.0 || arena.sound_key(key).is_err() {
                    return Err(AudioError("sound owner has no canonical frame".into()));
                }
                key
            }
            SoundFrameOwner::Vehicle(key) => {
                let projected = key
                    .0
                    .checked_mul(SLOT)
                    .and_then(|offset| FRAME_BASE.checked_add(offset));
                if key.0 == 0 || projected != Some(address) || address >= FRAME_END {
                    return Err(AudioError("sound owner has no canonical frame".into()));
                }
                key
            }
        };
        if !arena
            .state(key)
            .is_ok_and(|state| state.kind == FrameKind::Sound)
        {
            return Err(AudioError("sound owner has no canonical frame".into()));
        }
        let on = audio.sound(address)?.on;
        arena
            .set_frame_on(key, u32::from(on))
            .map_err(|e| AudioError(format!("sound frame state: {e:?}")))?;
        let world = arena
            .graph
            .world_matrix(key)
            .map_err(|e| AudioError(format!("sound world transform: {e:?}")))?;
        audio.set_world_scale(
            address,
            arena
                .graph
                .state(key)
                .map_err(|e| AudioError(format!("sound world scale: {e:?}")))?
                .world_scale,
        )?;
        audio.set_position(
            address,
            world.elements[12..15].try_into().unwrap(),
            world.elements[8..11].try_into().unwrap(),
        )?;
    }
    Ok(())
}

fn failure<E>(error: AudioError) -> VehicleFrameError<E> {
    VehicleFrameError::Audio(error.to_string())
}

impl<B: VehicleInitBackend> VehicleFrameBackend<'_, B> {
    fn audio_owner(&self) -> Result<RuntimeAudio, VehicleFrameError<B::Error>> {
        self.game_host.as_ref().and_then(|host| host.audio()).ok_or(
            VehicleFrameError::UnavailableFrameMethod("audio owner not bound"),
        )
    }
    pub(super) fn game_audio_read_word(
        &self,
        key: FrameKey,
        offset: u32,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        self.audio_owner()?
            .read_sound_word(self.frame_address(key)?, offset)
            .map_err(failure)
    }
    pub(super) fn game_audio_write_word(
        &mut self,
        key: FrameKey,
        offset: u32,
        value: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        self.audio_owner()?
            .write_sound_word(self.frame_address(key)?, offset, value)
            .map_err(failure)
    }
    fn sync_audio_frame(
        &mut self,
        audio: &RuntimeAudio,
        object: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let key = self.frame_key(object)?;
        let world = self
            .arena
            .graph
            .world_matrix(key)
            .map_err(ModelError::from)?;
        audio
            .set_world_scale(
                object,
                self.arena
                    .graph
                    .state(key)
                    .map_err(ModelError::from)?
                    .world_scale,
            )
            .map_err(failure)?;
        audio
            .set_position(
                object,
                world.elements[12..15].try_into().unwrap(),
                world.elements[8..11].try_into().unwrap(),
            )
            .map_err(failure)
    }
    fn free_audio_sources(
        &mut self,
        audio: &RuntimeAudio,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        for projection in audio.take_unused_source_projections().map_err(failure)? {
            self.free(projection.name)?;
            self.free(projection.source)?;
        }
        Ok(())
    }
}

impl<B: VehicleComponentsBackend> VehicleFrameBackend<'_, B> {
    pub(super) fn game_audio_resource(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        effect: VehicleResourceEffect,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let audio = self.audio_owner()?;
        match effect {
            VehicleResourceEffect::CreateSound { frame_type, .. } => {
                if frame_type != 4 {
                    return Err(VehicleFrameError::UnavailableFrameMethod(
                        "audio factory frame type",
                    ));
                }
                let key = self.arena.create_sound_frame()?;
                let address = self.frame_address(key)?;
                if let Err(error) =
                    audio.create_frame_sound_at(address, SoundFrameOwner::Vehicle(key))
                {
                    self.arena.release(key)?;
                    return Err(failure(error));
                }
                Ok(address)
            }
            VehicleResourceEffect::LoadSound { sound, name, .. } => {
                let mut name_bytes = Vec::new();
                // Cache::Open408d90 constructs an empty S_string for NULL.
                // Its registered directory prefix and real file Open still run.
                for offset in 0..if name == 0 { 0 } else { 4096u32 } {
                    let byte = memory.read_byte_with_backend(
                        self,
                        name.checked_add(offset)
                            .ok_or(VehicleFrameError::Address(name))?,
                    )?;
                    if byte == 0 {
                        break;
                    }
                    name_bytes.push(byte);
                }
                if name_bytes.len() == 4096 {
                    return Err(VehicleFrameError::Audio(
                        "sound filename exceeds bound".into(),
                    ));
                }
                let name = std::str::from_utf8(&name_bytes).map_err(|_| {
                    VehicleFrameError::Audio("sound filename encoding unsupported".into())
                })?;
                let installation =
                    self.installation
                        .ok_or(VehicleFrameError::UnavailableFrameMethod(
                            "original sound installation",
                        ))?;
                // Game startup registers "Sounds\\" with this sound cache.
                // Cache::Open prefixes each requested original table filename.
                let original_path = format!("Sounds\\{name}");
                if let Err(error) = audio.load_sound(sound, &original_path, installation) {
                    audio
                        .record_load_failure(sound, &original_path, &error)
                        .map_err(failure)?;
                    self.free_audio_sources(&audio)?;
                    // Original cache/Open failures are result codes. SoundInit
                    // then releases this real frame and clears its owner slot.
                    return Ok(if error.0.contains("original asset unavailable") {
                        0x8000_0001
                    } else {
                        0x8000_0005
                    });
                }
                let state = audio.sound(sound).map_err(failure)?;
                let source =
                    state
                        .source
                        .as_ref()
                        .ok_or(VehicleFrameError::UnavailableFrameMethod(
                            "sound source not loaded",
                        ))?;
                let name_bytes = source.name.as_bytes();
                if audio
                    .source_projection(&source.name)
                    .map_err(failure)?
                    .is_none()
                {
                    let source_address = self.allocate_projected(&mut memory, 0x40)?;
                    if source_address == 0 {
                        return Err(VehicleFrameError::Audio(
                            "source allocation returned null".into(),
                        ));
                    }
                    let name_address =
                        match self.allocate_projected(&mut memory, (name_bytes.len() + 9) as u32) {
                            Ok(value) if value != 0 => value,
                            Ok(_) => {
                                self.free_projected(&mut memory, source_address)?;
                                return Err(VehicleFrameError::Audio(
                                    "source filename allocation returned null".into(),
                                ));
                            }
                            Err(error) => {
                                self.free_projected(&mut memory, source_address)?;
                                return Err(error);
                            }
                        };
                    for offset in 0..0x40 {
                        self.write_byte(source_address + offset, 0)?;
                    }
                    self.write_word(source_address + 4, 1)?;
                    self.write_word(source_address + 0xc, name_address)?;
                    self.write_word(name_address, 1)?;
                    self.write_word(name_address + 4, name_bytes.len() as u32)?;
                    for (offset, byte) in name_bytes.iter().copied().chain([0]).enumerate() {
                        self.write_byte(name_address + 8 + offset as u32, byte)?;
                    }
                    audio
                        .bind_source_projection(
                            &source.name,
                            SourceProjection {
                                source: source_address,
                                name: name_address,
                            },
                        )
                        .map_err(failure)?;
                }
                self.free_audio_sources(&audio)?;
                Ok(0)
            }
            VehicleResourceEffect::SoundVirtual {
                sound,
                slot,
                arguments,
                count,
            } => {
                if count > 4 {
                    return Err(VehicleFrameError::Audio(
                        "sound virtual argument count".into(),
                    ));
                }
                self.game_audio_virtual(sound, slot, &arguments[..count as usize])?
                    .ok_or(VehicleFrameError::Address(sound))
            }
            VehicleResourceEffect::AddSectorSound { sector, sound } => {
                self.frame_key(sector)?;
                audio.add_sector_sound(sector, sound).map_err(failure)?;
                Ok(0)
            }
            VehicleResourceEffect::DeleteSectorSound { sector, sound } => {
                audio.delete_sector_sound(sector, sound).map_err(failure)?;
                Ok(0)
            }
            _ => Err(VehicleFrameError::UnavailableFrameMethod(
                "non-audio resource in audio adapter",
            )),
        }
    }

    pub(super) fn game_audio_virtual(
        &mut self,
        object: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<Option<u32>, VehicleFrameError<B::Error>> {
        let Some(audio) = self.game_host.as_ref().and_then(|host| host.audio()) else {
            return Ok(None);
        };
        if !audio.owns(object) {
            return Ok(None);
        }
        let key = self.frame_key(object)?;
        let result = match (slot, args) {
            (0, []) => {
                let references = self.arena.frame(key)?.references;
                if references == 1 {
                    audio.sound_virtual(object, 0, &[]).map_err(failure)?;
                }
                self.arena.release(key)?;
                self.free_audio_sources(&audio)?;
                references.wrapping_sub(1)
            }
            (0x18, []) => {
                self.arena.graph.update(key).map_err(ModelError::from)?;
                self.sync_audio_frame(&audio, object)?;
                audio.sound_virtual(object, slot, args).map_err(failure)?
            }
            (0x90, []) => {
                self.sync_audio_frame(&audio, object)?;
                audio.sound_virtual(object, slot, args).map_err(failure)?
            }
            (0x24, [_]) | (0x88, [_, _]) => {
                self.arena.set_frame_on(key, u32::from(args[0] != 0))?;
                self.sync_audio_frame(&audio, object)?;
                audio.sound_virtual(object, slot, args).map_err(failure)?
            }
            (0x2c, [parent, flags]) => {
                let parent = if *parent == 0 {
                    None
                } else {
                    Some(self.frame_key(*parent)?)
                };
                let result = self
                    .arena
                    .graph
                    .link_to_with_flags(key, parent, *flags)
                    .map_err(ModelError::from)?;
                audio.sound_virtual(object, slot, args).map_err(failure)?;
                self.sync_audio_frame(&audio, object)?;
                result
            }
            (0x78, [near, far, middle, volume]) => {
                let state = audio.sound(object).map_err(failure)?;
                for (address, value) in [near, far, middle, volume].into_iter().zip(state.range) {
                    self.write_word(*address, value.to_bits())?;
                }
                0
            }
            (0x7c, [inside, outside]) => {
                let state = audio.sound(object).map_err(failure)?;
                self.write_word(*inside, state.cone[0].to_bits())?;
                self.write_word(*outside, state.cone[1].to_bits())?;
                0
            }
            (0x8c, [address]) => {
                let velocity = [
                    f32::from_bits(self.read_word(*address)?),
                    f32::from_bits(self.read_word(*address + 4)?),
                    f32::from_bits(self.read_word(*address + 8)?),
                ];
                audio.set_velocity(object, velocity).map_err(failure)?;
                0
            }
            _ => audio.sound_virtual(object, slot, args).map_err(failure)?,
        };
        if audio.owns(object) {
            self.arena
                .set_frame_on(key, u32::from(audio.sound(object).map_err(failure)?.on))?;
        }
        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_frame_sync_reads_canonical_inherited_scale_after_parent_changes() {
        let mut arena = ModelArena::default();
        let parent = arena.create_sound_frame().unwrap();
        let child = arena.create_sound_frame().unwrap();
        arena
            .graph
            .link_to_with_flags(child, Some(parent), 0)
            .unwrap();
        arena.graph.assign_scale(parent, [2.0; 3]).unwrap();
        arena.graph.assign_scale(child, [3.0; 3]).unwrap();
        let mut memory = VehicleFrameMemory::default();
        let address = VehicleFrameBackend::new(&mut arena, &mut memory)
            .frame_address(child)
            .unwrap();
        let audio = RuntimeAudio::default();
        audio
            .create_frame_sound_at(address, SoundFrameOwner::Vehicle(child))
            .unwrap();
        sync_runtime_audio_frames(&audio, &mut arena).unwrap();
        assert_eq!(audio.sound(address).unwrap().world_scale, 6.0);
        arena.graph.assign_scale(parent, [4.0; 3]).unwrap();
        arena.graph.update(parent).unwrap();
        sync_runtime_audio_frames(&audio, &mut arena).unwrap();
        assert_eq!(audio.sound(address).unwrap().world_scale, 12.0);
    }

    #[test]
    fn shared_pcm_sync_resolves_scene_and_vehicle_sound_frame_owners() {
        let mut arena = ModelArena::default();
        let audio = RuntimeAudio::default();
        arena
            .bind_host_sound_backend(crate::frame_sound::HostSoundBackend::new(1, &audio))
            .unwrap();
        let scene_sound = arena.create_owned_sound_frame().unwrap();
        let vehicle_sound = arena.create_sound_frame().unwrap();
        let mut memory = VehicleFrameMemory::default();
        let vehicle_address = VehicleFrameBackend::new(&mut arena, &mut memory)
            .frame_address(vehicle_sound)
            .unwrap();
        audio
            .create_frame_sound_at(vehicle_address, SoundFrameOwner::Vehicle(vehicle_sound))
            .unwrap();
        assert_eq!(audio.live_sound_count().unwrap(), 2);
        assert!(audio.owns(scene_sound.frame().0));
        assert!(audio.owns(vehicle_address));

        arena
            .graph
            .assign_position(scene_sound.frame(), [11.0, 2.0, 3.0])
            .unwrap();
        arena
            .graph
            .assign_position(vehicle_sound, [-7.0, 5.0, 9.0])
            .unwrap();
        sync_runtime_audio_frames(&audio, &mut arena).unwrap();
        assert_eq!(
            audio.sound(scene_sound.frame().0).unwrap().position,
            [11.0, 2.0, 3.0]
        );
        assert_eq!(
            audio.sound(vehicle_address).unwrap().position,
            [-7.0, 5.0, 9.0]
        );

        arena
            .graph
            .assign_position(scene_sound.frame(), [13.0, 2.0, 3.0])
            .unwrap();
        arena
            .graph
            .assign_position(vehicle_sound, [-9.0, 5.0, 9.0])
            .unwrap();
        sync_runtime_audio_frames(&audio, &mut arena).unwrap();
        assert_eq!(
            audio.sound(scene_sound.frame().0).unwrap().position,
            [13.0, 2.0, 3.0]
        );
        assert_eq!(
            audio.sound(vehicle_address).unwrap().position,
            [-9.0, 5.0, 9.0]
        );

        arena.release(scene_sound.frame()).unwrap();
        assert!(!audio.owns(scene_sound.frame().0));
        audio.sound_virtual(vehicle_address, 0, &[]).unwrap();
        arena.release(vehicle_sound).unwrap();
        assert_eq!(audio.live_sound_count().unwrap(), 0);
        assert_eq!(arena.live_frame_count(), 0);
    }

    #[test]
    fn shared_pcm_sync_refuses_an_unowned_direct_frame() {
        let mut arena = ModelArena::default();
        let audio = RuntimeAudio::default();
        let unrelated = arena.create_sound_frame().unwrap();
        audio.create_sound_at(unrelated.0).unwrap();
        assert_eq!(
            sync_runtime_audio_frames(&audio, &mut arena).unwrap_err(),
            AudioError("sound owner has no canonical frame".into())
        );
    }

    #[test]
    fn shared_pcm_sync_retains_reached_prefix_before_later_orphan_error() {
        let mut arena = ModelArena::default();
        let audio = RuntimeAudio::default();
        let first = arena.create_sound_frame().unwrap();
        let later = arena.create_sound_frame().unwrap();
        let mut memory = VehicleFrameMemory::default();
        let backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let first_address = backend.frame_address(first).unwrap();
        let later_address = backend.frame_address(later).unwrap();
        audio
            .create_frame_sound_at(first_address, SoundFrameOwner::Vehicle(first))
            .unwrap();
        audio.create_sound_at(first_address + 1).unwrap();
        audio
            .create_frame_sound_at(later_address, SoundFrameOwner::Vehicle(later))
            .unwrap();
        arena.graph.assign_position(first, [4.0, 5.0, 6.0]).unwrap();
        arena.graph.assign_position(later, [7.0, 8.0, 9.0]).unwrap();

        assert_eq!(
            sync_runtime_audio_frames(&audio, &mut arena).unwrap_err(),
            AudioError("sound owner has no canonical frame".into())
        );
        assert_eq!(
            audio.sound(first_address).unwrap().position,
            [4.0, 5.0, 6.0]
        );
        assert_eq!(audio.sound(later_address).unwrap().position, [0.0; 3]);
    }
}
