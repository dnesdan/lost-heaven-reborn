//! Scoped host transport for the actual embedded program, not native scheduling.
#![forbid(unsafe_code)]

use super::{
    ActorPhase, ActorProgramKind, ActorReferenceId, ActorWorld, ActorWorldError, ActorWorldIdentity,
};
use crate::original_program::CProgram;

/// The actor remains in its original owner slot while the program is borrowed.
/// ActorWorld does not remove slots; public lifetime/source operations reject
/// this actor until the lease ends. Other actors and ordinary state remain live.
struct ProgramDispatchLease<'a> {
    world: &'a mut ActorWorld,
    world_identity: ActorWorldIdentity,
    index: usize,
    actor: ActorReferenceId,
    kind: ActorProgramKind,
    program: Option<CProgram>,
}

impl ProgramDispatchLease<'_> {
    fn restore(&mut self) -> Result<(), ActorWorldError> {
        if self.program.is_none() {
            return Ok(());
        }
        // A diagnostic receives &mut ActorWorld. Replacing that whole owner
        // violates its borrowed-owner contract, but must still never attach
        // this program to another world or cause a second panic during unwind.
        if self.world.identity() != self.world_identity {
            return Err(ActorWorldError::StaleActor(self.actor));
        }
        let actor = self
            .world
            .actors
            .get_mut(self.index)
            .filter(|actor| actor.lifetime.reference() == self.actor)
            .ok_or(ActorWorldError::StaleActor(self.actor))?;
        if !actor.program_dispatch_leased || actor.phase != ActorPhase::Ready {
            return Err(ActorWorldError::Busy(self.actor));
        }
        let slot = match self.kind {
            ActorProgramKind::Entity => {
                &mut actor
                    .entity
                    .as_mut()
                    .ok_or(ActorWorldError::NotEntity)?
                    .program
            }
            ActorProgramKind::Detector => {
                &mut actor
                    .detector
                    .as_mut()
                    .ok_or(ActorWorldError::NotDetector)?
                    .program
            }
        };
        if slot.is_some() {
            return Err(ActorWorldError::Busy(self.actor));
        }
        *slot = self.program.take();
        actor.program_dispatch_leased = false;
        Ok(())
    }
}

impl Drop for ProgramDispatchLease<'_> {
    fn drop(&mut self) {
        // Normal public actor mutations preserve the slot and lease. On an
        // invalid whole-world replacement there is no original owner left to
        // restore; preserve any original panic, without touching the new world.
        let _ = self.restore();
    }
}

impl ActorWorld {
    pub(super) fn guard_program_dispatch(
        &self,
        actor: ActorReferenceId,
    ) -> Result<(), ActorWorldError> {
        if self.actors[self.index(actor)?].program_dispatch_leased {
            Err(ActorWorldError::Busy(actor))
        } else {
            Ok(())
        }
    }

    /// Borrow the actual program and restore it before returning or unwinding,
    /// provided the callback preserves the borrowed ActorWorld itself.
    /// Its runtime stage, owner and reached changes are left untouched. The
    /// guard does not catch/translate a panic or retry any operation.
    ///
    /// A detached manual take is an error, not an empty program to skip. The
    /// callback may mutate live actors, but cannot replace the whole ActorWorld
    /// (the same owner contract as all callbacks taking a live world borrow).
    pub(crate) fn with_program_dispatch_lease<R>(
        &mut self,
        actor: ActorReferenceId,
        kind: ActorProgramKind,
        operation: impl FnOnce(&mut Self, &mut CProgram) -> R,
    ) -> Result<R, ActorWorldError> {
        let index = self.ready(actor)?;
        self.guard_program_dispatch(actor)?;
        let slot = match kind {
            ActorProgramKind::Entity => {
                &mut self.actors[index]
                    .entity
                    .as_mut()
                    .ok_or(ActorWorldError::NotEntity)?
                    .program
            }
            ActorProgramKind::Detector => {
                &mut self.actors[index]
                    .detector
                    .as_mut()
                    .ok_or(ActorWorldError::NotDetector)?
                    .program
            }
        };
        let program = slot.take().ok_or(ActorWorldError::Busy(actor))?;
        self.actors[index].program_dispatch_leased = true;
        let world_identity = self.identity();
        let mut lease = ProgramDispatchLease {
            world: self,
            world_identity,
            index,
            actor,
            kind,
            program: Some(program),
        };
        let result = operation(lease.world, lease.program.as_mut().unwrap());
        lease.restore()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor_world::{
        ActorCollectionBackend, ActorSource, ActorStorageId, BaseActorSeed, DetectorSeed,
        SourceBacking,
    };
    use crate::original_program::ProgramSeed;
    use std::panic::{catch_unwind, panic_any, AssertUnwindSafe};
    use std::sync::Arc;

    struct Preallocated;
    impl ActorCollectionBackend for Preallocated {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            panic!("source growth outside fixture")
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            panic!("source free outside fixture")
        }
    }
    fn world(seed: u32) -> (ActorWorld, ActorReferenceId) {
        let backing = SourceBacking {
            storage: Some(ActorStorageId(1)),
            capacity: 1,
        };
        let mut world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
        let actor = world
            .construct_detector(
                7,
                BaseActorSeed {
                    word_0e: 0,
                    actor_id: 0,
                    state_1c: 0,
                    bytes_21_5f: [0; 63],
                    bytes_64_65: [0; 2],
                    byte_67: 0,
                    release_frame: 0,
                    bytes_6d_6f: [0; 3],
                },
                DetectorSeed {
                    program: ProgramSeed {
                        address_high_byte: 0,
                        padding_0d: [0; 3],
                        words_1c_2c: [0; 5],
                        padding_31: [0; 3],
                        word_40: seed,
                        bytes_64: [0; 4],
                    },
                    bytes_d8_ef: [0; 24],
                    word_f4: 0,
                    bytes_105_107: [0; 3],
                    bytes_118_127: [0; 16],
                },
                &mut Preallocated,
            )
            .unwrap();
        (world, actor)
    }

    #[test]
    fn replaced_world_never_receives_borrowed_program_or_masks_original_panic() {
        for unwind in [false, true] {
            let (mut original, actor) = world(17);
            let (replacement, replacement_actor) = world(29);
            let replacement_identity = replacement.identity();
            let payload = Arc::new(0x8182);
            let result = catch_unwind(AssertUnwindSafe(|| {
                original.with_program_dispatch_lease(
                    actor,
                    ActorProgramKind::Detector,
                    |world, _| {
                        // Explicit violation of the borrowed-owner contract: the
                        // whole original actor owner is dropped. No source/native
                        // allocation is created by this constructor-only fixture.
                        *world = replacement;
                        if unwind {
                            panic_any(payload.clone());
                        }
                    },
                )
            }));
            if unwind {
                let caught = result.unwrap_err().downcast::<Arc<u32>>().unwrap();
                assert!(Arc::ptr_eq(&caught, &payload));
            } else {
                assert_eq!(result.unwrap(), Err(ActorWorldError::StaleActor(actor)));
            }
            assert_eq!(original.identity(), replacement_identity);
            assert!(original.detector(actor).is_err());
            assert_eq!(
                original
                    .detector(replacement_actor)
                    .unwrap()
                    .program()
                    .unwrap()
                    .seed()
                    .word_40,
                29
            );
            assert!(original
                .take_detector_program(replacement_actor)
                .unwrap()
                .is_some());
        }
    }
}
