//! C_game spatial effects use the existing frame graph, file cache and mixer.
use super::*;
use crate::game_spatial_sound::{GameSoundPending, GameSpatialSoundBackend, SpatialSoundRequest};

impl<B> GameSpatialSoundBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn game_sound_create(&mut self) -> Result<u32, String> {
        let driver = self.read_word(0x647ed8)?;
        self.frames
            .vehicle_resource_effect(
                self.memory.reborrow(),
                VehicleResourceEffect::CreateSound {
                    driver,
                    frame_type: 4,
                },
            )
            .map_err(failure)
    }
    fn game_sound_load(&mut self, sound: u32, name: u32) -> Result<u32, String> {
        self.frames
            .vehicle_resource_effect(
                self.memory.reborrow(),
                VehicleResourceEffect::LoadSound {
                    vehicle: 0,
                    sound,
                    name,
                    flags: 0,
                },
            )
            .map_err(failure)
    }
    fn game_sound_virtual(&mut self, sound: u32, slot: u32, args: &[u32]) -> Result<u32, String> {
        crate::indexed_sound_manager::IndexedSoundBackend::indexed_sound_virtual(
            self, sound, slot, args,
        )
    }
    fn game_sound_track(&mut self, resource: GameSoundPending) {
        self.state.game_sound_pending.push(resource);
    }
    fn game_sound_untrack(&mut self, resource: GameSoundPending) -> Result<(), String> {
        let index = self
            .state
            .game_sound_pending
            .iter()
            .rposition(|r| *r == resource)
            .ok_or_else(|| format!("Missing pending game sound ownership {resource:?}"))?;
        self.state.game_sound_pending.remove(index);
        Ok(())
    }
    fn game_sound_pending_last(&self) -> Option<GameSoundPending> {
        self.state.game_sound_pending.last().copied()
    }
}
impl<B> CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    pub(super) fn play_glass_break_sound(
        &mut self,
        name: u32,
        position: [f32; 3],
    ) -> Result<(), String> {
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        crate::game_spatial_sound::play_spatial_sound(
            self,
            game,
            0x870000,
            SpatialSoundRequest {
                parent: 0,
                name,
                sound_type: 1,
                position,
                range: [1., 25.],
                volume: 1.,
                fade: 0,
                fade_step: 0.001,
                looping: 0,
            },
        )
        .map(|_| ())
        .map_err(failure)
    }
    pub(crate) fn tick_game_sounds(&mut self, milliseconds: u32) -> Result<(), String> {
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        crate::game_spatial_sound::tick_sounds(self, game, milliseconds).map_err(failure)
    }
    pub(crate) fn cleanup_game_sounds(&mut self) -> Result<(), String> {
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        crate::game_spatial_sound::cleanup_sound_pending(self).map_err(failure)?;
        crate::game_spatial_sound::stop_all_sounds(self, game).map_err(failure)?;
        let begin = self.read_word(game + 0x2f8)?;
        self.free(begin)?;
        for offset in [0x2f8, 0x2fc, 0x300] {
            self.write_word(game + offset, 0)?;
        }
        Ok(())
    }
}
