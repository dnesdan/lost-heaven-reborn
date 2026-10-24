//! Temporary physical objects share the car caller's live frame/heap/world.
use super::*;
use crate::ph_thing_update::{PhThingUpdateBackend, PhThingUpdateEffect};
#[path = "car_runtime_simulation_crash_actor.rs"]
mod actor;
#[path = "car_runtime_simulation_crash_ai.rs"]
mod ai;
#[path = "car_runtime_simulation_crash_models.rs"]
mod models;
#[path = "car_runtime_simulation_crash_response.rs"]
mod response;
#[path = "car_runtime_simulation_crash_sound.rs"]
mod sound;

impl<B> PhThingUpdateBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn ph_thing_update_effect(&mut self, e: PhThingUpdateEffect) -> Result<u32, String> {
        match e {
            PhThingUpdateEffect::FrameVirtual {
                frame,
                slot,
                arguments,
                count,
            } => self
                .frames
                .crash_frame_call(frame, slot, &arguments[..usize::from(count)])
                .map_err(failure),
            PhThingUpdateEffect::UpdateWorld { frame } => {
                self.frames.crash_frame_world(frame).map_err(failure)?;
                Ok(0)
            }
            PhThingUpdateEffect::SetFrameSector {
                scene,
                frame,
                position,
            } => {
                let app = self.read_word(0x63788c)?;
                if scene != self.read_word(app + 0x10)? || position != frame + 0x40 {
                    return Err(
                        "Physical-object scene owner/position projection unavailable".into(),
                    );
                }
                self.frames
                    .crash_frame_place_sector(frame)
                    .map_err(failure)?;
                Ok(0)
            }
            PhThingUpdateEffect::SetInactive { actor, reason } => {
                crate::crash_actor::crash_actor_set_state(self, actor, reason).map_err(failure)?;
                Ok(0)
            }
            PhThingUpdateEffect::LoadSound {
                manager,
                sound,
                source,
            } => crate::indexed_sound_manager::construct_sound(self, manager, sound, source)
                .map(u32::from)
                .map_err(failure),
        }
    }
}

impl<B> CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    pub(crate) fn prepare_crash_services(&mut self) -> Result<(), String> {
        if !crate::indexed_sound_manager::load_sound_manager(self, 0x646f50).map_err(failure)? {
            return Err("Original indexed sound table failed to load".into());
        }
        Ok(())
    }
    /// Original retirement gate before the active actor phase.
    pub(crate) fn prepare_temporary_actors(&mut self) -> Result<(), String> {
        if self.read_word(0x63d6d4)? > 20 {
            crate::crash_actor::crash_retire_furthest_actor(self).map_err(failure)?;
        }
        Ok(())
    }
    /// Remaining live-forward active-actor phase after the original car pair.
    pub(crate) fn tick_temporary_actors(&mut self, milliseconds: u32) -> Result<(), String> {
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        crate::crash_actor::crash_temporary_actor_phase(self, game, milliseconds).map_err(failure)
    }
    /// Original temporary-actor lifecycle precedes releasing the caller's
    /// vector capacities and model cache. No actor/frame is dropped implicitly.
    pub(crate) fn cleanup_temporary_actors(&mut self) -> Result<(), String> {
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        crate::crash_actors::cleanup_crash_pending(self).map_err(failure)?;
        crate::crash_actor::crash_clear_temporary_actors(self, game).map_err(failure)?;
        self.cleanup_glass_services()?;
        self.cleanup_game_sounds()?;
        crate::crash_actors::cleanup_crash_model_cache(self).map_err(failure)?;
        crate::indexed_sound_manager::cleanup_sound_manager(self, 0x646f50).map_err(failure)?;
        for offset in [0xec, 0xfc, 0x120, 0x130] {
            let begin = self.read_word(game + offset + 4)?;
            let end = self.read_word(game + offset + 8)?;
            // ClearTemporaryActors empties its owning list. Other Game
            // buckets can retain borrowed, now stale scalar identities until
            // their caller destroys the pointer vectors; do not dereference
            // or Release those identities a second time.
            if offset == 0x120 && begin != end {
                return Err("Temporary actor ownership retained live entries".into());
            }
            self.free(begin)?;
            for field in [4, 8, 12] {
                self.write_word(game + offset + field, 0)?;
            }
        }
        Ok(())
    }
}
