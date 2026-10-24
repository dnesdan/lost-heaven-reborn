//! Indexed material sound IDs use their source table and the same sound owner.
use super::*;
impl<B> crate::indexed_sound_manager::IndexedSoundBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn indexed_sound_table(&mut self) -> Result<Vec<u8>, String> {
        self.frames.crash_indexed_sound_table().map_err(failure)
    }
    fn indexed_sound_open(&mut self, sound: u32, name_pointer: u32) -> Result<bool, String> {
        if !self.audio.owns(sound) {
            return Err("Indexed sound has no canonical owning sound frame".into());
        }
        if name_pointer == 0 {
            return Ok(false);
        }
        self.frames
            .vehicle_resource_effect(
                self.memory.reborrow(),
                VehicleResourceEffect::LoadSound {
                    vehicle: 0,
                    sound,
                    name: name_pointer,
                    flags: 0,
                },
            )
            .map(|code| code == 0)
            .map_err(failure)
    }
    fn indexed_sound_virtual(
        &mut self,
        sound: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, String> {
        if args.len() > 4 {
            return Err("Indexed sound virtual argument extent".into());
        }
        let mut arguments = [0; 4];
        arguments[..args.len()].copy_from_slice(args);
        self.frames
            .vehicle_resource_effect(
                self.memory.reborrow(),
                VehicleResourceEffect::SoundVirtual {
                    sound,
                    slot,
                    arguments,
                    count: args.len() as u8,
                },
            )
            .map_err(failure)
    }
}
