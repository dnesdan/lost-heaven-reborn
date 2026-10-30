//! FINDFRAME / FINDACTOR on the actual CProgram registers and live world.
//! Source: mafia-re 6c9f71c9bb52da7188eced90f9f732e7290657e9,
//! C_program::Process (PC RVA 0x6d3b0), and the existing script_lookup contract.
//! This is not derived Entity/Detector dispatch, recording or GameInit.
#![forbid(unsafe_code)]

use super::{
    ActorReferenceId, FrameReferenceId, NameParameter, Process, ResetState, RuntimeBackend,
    RuntimeEffect, RuntimeError, RuntimeReferenceSlot, RuntimeView, ScalarParameter,
};
use crate::frame_lookup::{FrameLookupBackend, SceneRoots};
use crate::script_lookup::{self, LookupError, LookupRegisters, LookupWorldBackend};

/// The diagnostic must borrow the CProgram's actual owner, not a separately
/// stored owner in a lookup view. Runtime callbacks may also mutate registers.
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

/// Borrowed scene/diagnostic inputs for a step of an existing mission program.
/// The frame backend resolves owner-scoped identities; roots and locale are
/// explicit inputs, not an invented empty scene or inferred game-global state.
pub struct ProgramLookupScene<'a> {
    pub frames: &'a dyn FrameLookupBackend,
    pub roots: Option<SceneRoots>,
    pub diagnostic: &'a mut dyn crate::actor_world::ActorWorldDiagnosticBackend,
    pub diagnostic_default_c_locale: bool,
}

/// Compose required allocation effects and an existing live world. This wrapper
/// only borrows them; it constructs neither register storage nor game objects.
/// ActorWorldLookup supplies the production arena/actor/diagnostic adapter.
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

/// The program owner is authoritative, including after a diagnostic callback.
/// All other reads use the existing world. There are no callbacks in the search
/// suffix; the diagnostic itself goes through RuntimeBackend with mutable owner.
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
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> {
        self.world.frames()
    }
    fn scene(&self) -> Result<SceneRoots, LookupError> {
        self.world.scene()
    }
    fn message_beep(&mut self, _: i32) -> Result<(), LookupError> {
        Err(LookupError::Backend("diagnostic requires the live runtime boundary".into()))
    }
}

struct Registers<'a> {
    actor_count: &'a u32,
    frame_count: &'a u32,
    actors: &'a mut Option<Box<[RuntimeReferenceSlot<ActorReferenceId>]>>,
    frames: &'a mut Option<Box<[RuntimeReferenceSlot<FrameReferenceId>]>>,
}
impl LookupRegisters for Registers<'_> {
    fn actor_register_count(&self) -> usize {
        // The native upper-bound comparison uses a signed count. Negative
        // counts admit no nonnegative index; do not infer backing from the Box.
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
        let slot = self.actors.as_mut().and_then(|a| a.get_mut(index as usize))
            .ok_or_else(|| LookupError::Backend("actor register backing unavailable".into()))?;
        *slot = value.map_or(RuntimeReferenceSlot::Empty, RuntimeReferenceSlot::Reference);
        Ok(())
    }
    fn write_frame_register(
        &mut self,
        index: u32,
        value: Option<FrameReferenceId>,
    ) -> Result<(), LookupError> {
        let slot = self.frames.as_mut().and_then(|a| a.get_mut(index as usize))
            .ok_or_else(|| LookupError::Backend("frame register backing unavailable".into()))?;
        *slot = value.map_or(RuntimeReferenceSlot::Empty, RuntimeReferenceSlot::Reference);
        Ok(())
    }
}

impl<B: RuntimeBackend> Process<'_, B> {
    pub(super) fn dispatch_lookup(
        &mut self,
        state: &mut ResetState,
        actors: bool,
    ) -> Result<(), RuntimeError> {
        // FINDACTOR's diagnostic precedes parameter/slot reads. In particular,
        // do not retain the PC, payload, owner or register counts across it.
        if actors && self.owner.is_some() {
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
            .and_then(|instruction| instruction.parameter.as_ref())
            .and_then(|parameter| parameter.name_parameter())
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
        let count = if actors { registers.actor_register_count() } else { registers.frame_register_count() };
        if index as usize >= count {
            // Native upper-bound no-op: no world, name or storage is read.
            return Ok(());
        }
        let name = parameter.name()
            .ok_or(RuntimeError::Domain("current named string unavailable"))?;
        if name.is_empty() && (actors || self.owner.is_none()) {
            // Copying the owner/null needs no scene or replacement world. The
            // reached diagnostic has already validated any owner-frame read.
            return if actors {
                registers.write_actor_register(index as u32, *self.owner)
            } else {
                registers.write_frame_register(index as u32, None)
            }.map_err(RuntimeError::Lookup);
        }
        let world = ProgramOwnerWorld {
            world: self.backend.lookup_world().map_err(RuntimeError::Lookup)?,
            owner: self.owner,
        };
        if actors {
            script_lookup::find_actor_after_diagnostic(&mut registers, &world, index, name)
        } else {
            script_lookup::find_frame_into(&mut registers, &world, index, name)
        }.map_err(RuntimeError::Lookup)
    }
}

#[cfg(test)]
#[path = "program_runtime_lookup_tests.rs"]
mod tests;
