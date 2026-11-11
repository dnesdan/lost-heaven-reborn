//! Concrete Crash actor effects over the simulation's existing canonical owner.
use super::*;
use crate::crash_actor::{CrashActorBackend, CrashActorEffect, CrashActorError};

impl<B> CrashActorBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn crash_actor_effect(&mut self, effect: CrashActorEffect) -> Result<u32, String> {
        match effect {
            CrashActorEffect::FrameVirtual {
                frame,
                slot,
                arguments,
                count,
            } => {
                let args = arguments
                    .get(..usize::from(count))
                    .ok_or("Crash frame argument range")?;
                self.frames
                    .crash_frame_call(frame, slot, args)
                    .map_err(failure)
            }
            CrashActorEffect::FindFrameNamed { root, name, flags } => self
                .frames
                .crash_frame_find_named(root, name, flags)
                .map_err(failure),
            CrashActorEffect::UpdateWorld { frame } => self
                .frames
                .crash_frame_world(frame)
                .map(|_| 0)
                .map_err(failure),
            CrashActorEffect::UpdateHierarchyBounds { frame } => {
                let key = self.frames.frame_key(frame).map_err(failure)?;
                self.frames
                    .arena_mut()
                    .update_hr_bound_vol(key)
                    .map_err(failure)?;
                Ok(0)
            }
            CrashActorEffect::SetFrameSector { frame, position } => {
                if position != frame + 0x40 {
                    return Err(
                        "Crash sector position must use its canonical world position".into(),
                    );
                }
                self.frames
                    .crash_frame_place_sector(frame)
                    .map(|_| 0)
                    .map_err(failure)
            }
            CrashActorEffect::CreateSound => {
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
            CrashActorEffect::Dependency {
                actor,
                entry,
                arguments,
            } => match entry {
                0x405fa0 => {
                    // C_crash_obj's base actor path does not access the scratch
                    // argument used only by actor kind4's special sector case.
                    actor_game_init(self.frames, self.memory.reborrow(), actor, 0)
                        .map(|_| 0)
                        .map_err(failure)
                }
                0x4063e0 => actor_game_done(self.frames, self.memory.reborrow(), actor, 0)
                    .map(|_| 0)
                    .map_err(failure),
                0x407cd0 => crate::crash_actors::crash_debustize(self, actor)
                    .map(|_| 0)
                    .map_err(failure),
                0x431be0 => match crate::crash_actor::crash_actor_ai(self, actor, arguments[0]) {
                    Ok(()) => Ok(0),
                    // Unowned crash-effect routines from the AI flag-mask
                    // dispatch (model-gated visual/sound dispatches; REA
                    // dossier ev_456a2a52): stage the intent
                    // MissionHelpLog-style and keep the StaticResponse
                    // outcome instead of aborting the tick. Observed live
                    // as Dependency(5644656) killing the drive at
                    // [-1427.4,-219.7].
                    Err(CrashActorError::Dependency(entry))
                        if matches!(
                            entry,
                            0x5abe10 | 0x5aae10 | 0x562170 | 0x565720 | 0x565060
                        ) =>
                    {
                        self.state
                            .crash_ai_intents
                            .push(crate::crash_actor::CrashAiIntent { actor, entry });
                        Ok(0)
                    }
                    Err(other) => Err(failure(other)),
                },
                0x45a690 => crate::ph_thing_ai::ph_thing_ai(self, actor, arguments[0])
                    .map(|_| 0)
                    .map_err(failure),
                0x45a7a0 => crate::ph_thing_update::ph_thing_update(self, actor, arguments[0])
                    .map(|_| 0)
                    .map_err(failure),
                // Primary443860 is a JMP thunk; its separately verified
                // target4ab130 consists solely of RET4.
                0x443860 => Ok(0),
                0x5afe40 => {
                    let app = self.read_word(0x63788c)?;
                    let game = self.read_word(app + 0x24)?;
                    crate::crash_actor::crash_remove_actor_bindings(self, game, actor)
                        .map(|_| 0)
                        .map_err(failure)
                }
                0x5c36c0 => {
                    crate::dynamic_collision_manager::dyn_insert(self, 0x647f48, arguments[0])
                        .map(|_| 0)
                        .map_err(failure)
                }
                0x5c3ac0 => {
                    crate::dynamic_collision_manager::dyn_update(self, 0x647f48, arguments[0])
                        .map(|_| 0)
                        .map_err(failure)
                }
                0x5c38b0 => {
                    crate::dynamic_collision_manager::dyn_erase(self, 0x647f48, arguments[0])
                        .map(|_| 0)
                        .map_err(failure)
                }
                // Unowned 'H'-property crash effect (Game.exe 0x5b9b90;
                // REA dossier ev_c290764fcb5c0ff0f01ddabfebc91a687884741d97b3379fe6e53d54ef5c4870):
                // stage the intent like the flag-table effects and keep
                // the StaticResponse outcome. Reached via direct
                // effect calls (not through the 0x431be0 AI arm), so it
                // needs its own staging site; observed live aborting
                // the drive (entry 0x5b9b90, actor 0x1065aa4).
                0x5b9b90 => {
                    self.state
                        .crash_ai_intents
                        .push(crate::crash_actor::CrashAiIntent { actor, entry });
                    Ok(0)
                }
                _ => Err(format!(
                    "Crash actor dependency {entry:#x} actor{actor:#x} arguments{arguments:?}"
                )),
            },
        }
    }
}
