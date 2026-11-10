//! Explicit terminal host disposal of embedded Rust program allocations.
//! This is not actor GameDone, native destruction or mission completion.
#![forbid(unsafe_code)]

use super::{ActorPhase, ActorReferenceId, ActorWorld};
use crate::original_program::{HostRuntimeDiscard, RuntimeError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorProgramKind {
    Entity,
    Detector,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ActorProgramDiscard {
    pub actor: ActorReferenceId,
    pub kind: ActorProgramKind,
    pub resources: HostRuntimeDiscard,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ActorProgramDiscardError {
    ReportAllocation,
    /// A caller took the program out of this owner. It must return it before
    /// disposal; absence is not proof that executable storage was released.
    Detached {
        actor: ActorReferenceId,
        kind: ActorProgramKind,
    },
    Runtime {
        actor: ActorReferenceId,
        kind: ActorProgramKind,
        error: RuntimeError,
    },
}

impl ActorWorld {
    /// Source destruction must not begin while an embedded executable runtime
    /// is retained or its program is detached. Includes interrupted actors;
    /// the check does not mutate actor phases or replay any cleanup callback.
    pub fn program_disposal_pending(&self) -> bool {
        self.actors
            .iter()
            .filter(|actor| actor.phase != ActorPhase::Dead)
            .any(|actor| {
                actor.entity.as_ref().is_some_and(|owner| {
                    owner
                        .program
                        .as_ref()
                        .is_none_or(|program| program.runtime().is_some())
                }) || actor.detector.as_ref().is_some_and(|owner| {
                    owner
                        .program
                        .as_ref()
                        .is_none_or(|program| program.runtime().is_some())
                })
            })
    }

    /// Retire only host-owned VM allocations, leaving source/name buffers and
    /// their actual actor owner for later source destruction. No native Done,
    /// free callback, actor transition or source-list mutation occurs here.
    ///
    /// Detached programs are checked before any release. Report capacity is
    /// also reserved first. Repeating a completed call reports no allocations;
    /// the retired CProgram's initialization admission remains consumed.
    pub fn discard_embedded_program_runtimes_host(
        &mut self,
    ) -> Result<Vec<ActorProgramDiscard>, ActorProgramDiscardError> {
        let mut count = 0usize;
        for actor in self
            .actors
            .iter()
            .filter(|actor| actor.phase != ActorPhase::Dead)
        {
            for (kind, slot) in [
                (
                    ActorProgramKind::Entity,
                    actor.entity.as_ref().map(|owner| &owner.program),
                ),
                (
                    ActorProgramKind::Detector,
                    actor.detector.as_ref().map(|owner| &owner.program),
                ),
            ] {
                if let Some(slot) = slot {
                    let program = slot.as_ref().ok_or(ActorProgramDiscardError::Detached {
                        actor: actor.lifetime.reference(),
                        kind,
                    })?;
                    count += usize::from(program.runtime().is_some());
                }
            }
        }
        let mut reports = Vec::new();
        reports
            .try_reserve_exact(count)
            .map_err(|_| ActorProgramDiscardError::ReportAllocation)?;
        // Host cleanup order only. The actual programs never leave their
        // owners, so failed runtime retirement cannot detach/drop a source.
        for actor in self
            .actors
            .iter_mut()
            .rev()
            .filter(|actor| actor.phase != ActorPhase::Dead)
        {
            let reference = actor.lifetime.reference();
            for (kind, program) in [
                (
                    ActorProgramKind::Entity,
                    actor
                        .entity
                        .as_mut()
                        .and_then(|owner| owner.program.as_mut()),
                ),
                (
                    ActorProgramKind::Detector,
                    actor
                        .detector
                        .as_mut()
                        .and_then(|owner| owner.program.as_mut()),
                ),
            ] {
                if let Some(program) = program.filter(|program| program.runtime().is_some()) {
                    let resources = program.discard_runtime_host().map_err(|error| {
                        ActorProgramDiscardError::Runtime {
                            actor: reference,
                            kind,
                            error,
                        }
                    })?;
                    reports.push(ActorProgramDiscard {
                        actor: reference,
                        kind,
                        resources,
                    });
                }
            }
        }
        Ok(reports)
    }
}
