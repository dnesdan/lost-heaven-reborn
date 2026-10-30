//! Original FINDFRAME / FINDACTOR runtime over the actual CProgram registers.
//! Reference: mafia-re 6c9f71c C_program::Process (Game RVA 0x6d3b0).
//! Recording, derived VMs and GameInit remain unsupported.
#![forbid(unsafe_code)]

use super::{
    ActorReferenceId, FrameReferenceId, NameParameter, Process, ResetState, RuntimeBackend,
    RuntimeEffect, RuntimeError, RuntimeReferenceSlot, RuntimeView, ScalarParameter,
};
use crate::script_lookup::{self, LookupError, LookupRegisters, LookupWorldBackend};

/// A bound world must perform diagnostics against the live CProgram owner.
pub trait RuntimeLookupWorld: LookupWorldBackend {
    fn runtime_message_beep(
        &mut self,
        fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        argument: i32,
    ) -> Result<(), LookupError>;
}

impl RuntimeLookupWorld for crate::actor_world::ActorWorldLookup<'_> {
    fn runtime_message_beep(
        &mut self,
        _fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        argument: i32,
    ) -> Result<(), LookupError> {
        self.diagnostic.message_beep(self.world, owner, argument)
    }
}

/// This wrapper borrows real owners. It does not make another program or world.
pub struct LookupRuntimeBackend<'a, B, W> {
    effects: &'a mut B,
    world: &'a mut W,
}
impl<'a, B, W> LookupRuntimeBackend<'a, B, W> {
    pub fn new(effects: &'a mut B, world: &'a mut W) -> Self {
        Self { effects, world }
    }
}
impl<B: RuntimeBackend, W: RuntimeLookupWorld> RuntimeBackend for LookupRuntimeBackend<'_, B, W> {
    fn effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        published: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        self.effects.effect(fields, effect, published)
    }
    fn name_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        published: Option<&NameParameter>,
    ) -> Result<(), String> {
        self.effects.name_parameter_effect(fields, effect, published)
    }
    fn lookup_world(&mut self) -> Result<&mut dyn LookupWorldBackend, LookupError> {
        Ok(self.world)
    }
    fn lookup_message_beep(
        &mut self,
        fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        argument: i32,
    ) -> Result<(), LookupError> {
        self.world.runtime_message_beep(fields, owner, argument)
    }
}

/// Inject the actual CProgram actor, not the optional owner field of a reused
/// lookup adapter. No callbacks occur in the search suffix.
struct ProgramOwnerWorld<'a> {
    world: &'a dyn LookupWorldBackend,
    owner: &'a Option<ActorReferenceId>,
}
impl LookupWorldBackend for ProgramOwnerWorld<'_> {
    fn diagnostic_default_c_locale(&self) -> bool {
        self.world.diagnostic_default_c_locale()
    }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        Ok(*self.owner)
    }
    fn actor_frame(&self, actor: ActorReferenceId) -> Result<Option<FrameReferenceId>, LookupError> {
        self.world.actor_frame(actor)
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        self.world.primary()
    }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> {
        self.world.players()
    }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        self.world.secondary()
    }
    fn frames(&self) -> Result<&dyn crate::frame_lookup::FrameLookupBackend, LookupError> {
        self.world.frames()
    }
    fn scene(&self) -> Result<crate::frame_lookup::SceneRoots, LookupError> {
        self.world.scene()
    }
    fn message_beep(&mut self, _: i32) -> Result<(), LookupError> {
        Err(LookupError::Backend("diagnostic requires live program state".into()))
    }
}

/// This is a borrowed view of the one authoritative runtime register bank.
struct Registers<'a> {
    actor_count: &'a u32,
    frame_count: &'a u32,
    actors: &'a mut Option<Box<[RuntimeReferenceSlot<ActorReferenceId>]>>,
    frames: &'a mut Option<Box<[RuntimeReferenceSlot<FrameReferenceId>]>>,
}
impl LookupRegisters for Registers<'_> {
    fn actor_register_count(&self) -> usize {
        (*self.actor_count as i32).max(0) as usize
    }
    fn frame_register_count(&self) -> usize {
        (*self.frame_count as i32).max(0) as usize
    }
    fn write_actor_register(
        &mut self,
        index: u32,
        value: Option<ActorReferenceId>,
    ) -> Result<(), LookupError> {
        let slot = self.actors.as_mut().and_then(|v| v.get_mut(index as usize))
            .ok_or_else(|| LookupError::Backend("actor register backing unavailable".into()))?;
        *slot = value.map_or(RuntimeReferenceSlot::Empty, RuntimeReferenceSlot::Reference);
        Ok(())
    }
    fn write_frame_register(
        &mut self,
        index: u32,
        value: Option<FrameReferenceId>,
    ) -> Result<(), LookupError> {
        let slot = self.frames.as_mut().and_then(|v| v.get_mut(index as usize))
            .ok_or_else(|| LookupError::Backend("frame register backing unavailable".into()))?;
        *slot = value.map_or(RuntimeReferenceSlot::Empty, RuntimeReferenceSlot::Reference);
        Ok(())
    }
}
impl<B: RuntimeBackend> Process<'_, B> {
    pub(super) fn dispatch_lookup(
        &mut self,
        state: &mut ResetState,
        actor_lookup: bool,
    ) -> Result<(), RuntimeError> {
        // The original FINDACTOR diagnostic runs before the instruction is read.
        // It may mutate the PC, actor owner, counts and register backing.
        if actor_lookup && self.owner.is_some() {
            let needs_beep = {
                let world = ProgramOwnerWorld {
                    world: self.backend.lookup_world().map_err(RuntimeError::Lookup)?,
                    owner: self.owner,
                };
                script_lookup::actor_lookup_needs_beep(&world).map_err(RuntimeError::Lookup)?
            };
            if needs_beep {
                self.backend.lookup_message_beep(
                    RuntimeView {
                        flags: self.flags,
                        scheduler: state,
                        last_pc: self.last_pc,
                        elapsed: self.elapsed,
                        command_budget_bits: self.command_budget_bits,
                        actor_count: self.actor_count,
                        frame_count: self.frame_count,
                        actors: self.actors,
                        frames: self.frames,
                    },
                    self.owner,
                    -1,
                ).map_err(RuntimeError::Lookup)?;
            }
        }
        let parameter = self.instructions.active().get(state.program_counter as usize)
            .and_then(|i| i.parameter.as_ref())
            .and_then(|p| p.name_parameter())
            .ok_or(RuntimeError::Domain("current named parameter unavailable"))?;
        let index = parameter.register()
            .ok_or(RuntimeError::Domain("current named register unavailable"))?;
        if index < 0 {
            return Err(RuntimeError::Lookup(LookupError::NegativeIndex(index)));
        }
        let mut registers = Registers {
            actor_count: self.actor_count,
            frame_count: self.frame_count,
            actors: self.actors,
            frames: self.frames,
        };
        let count = if actor_lookup {
            registers.actor_register_count()
        } else {
            registers.frame_register_count()
        };
        if index as usize >= count {
            return Ok(());
        }
        let name = parameter.name()
            .ok_or(RuntimeError::Domain("current named string unavailable"))?;
        if name.is_empty() && (actor_lookup || self.owner.is_none()) {
            return if actor_lookup {
                registers.write_actor_register(index as u32, *self.owner)
            } else {
                registers.write_frame_register(index as u32, None)
            }.map_err(RuntimeError::Lookup);
        }
        let world = ProgramOwnerWorld {
            world: self.backend.lookup_world().map_err(RuntimeError::Lookup)?,
            owner: self.owner,
        };
        if actor_lookup {
            script_lookup::find_actor_after_diagnostic(&mut registers, &world, index, name)
        } else {
            script_lookup::find_frame_into(&mut registers, &world, index, name)
        }.map_err(RuntimeError::Lookup)
    }
}

#[cfg(test)]
#[path = "program_runtime_lookup_tests.rs"]
mod tests;
