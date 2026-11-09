//! Host lease regression tests over the existing real mission owner fixture.
use super::*;
use crate::actor_frames::ModelActorFrames;
use crate::actor_world::{ActorProgramDiscardError, ActorProgramKind, ActorWorldError};
use crate::human_actor::ActorDataReader;
use crate::mission_content_loading::{ContentFrames, ContentMemory};
use crate::original_program::{ProgramSeed, ProgramSliceReader};
use std::panic::{catch_unwind, panic_any, AssertUnwindSafe};
use std::sync::Arc;

fn add_program(f: &mut Fixture, kind: ActorProgramKind, source: &[u8]) -> ActorReferenceId {
    let content = &mut f.loader.content;
    let actor = content
        .actors
        .world
        .create_actor(
            match kind {
                ActorProgramKind::Entity => 27,
                ActorProgramKind::Detector => 5,
            },
            &mut content.memory,
        )
        .unwrap()
        .unwrap();
    let mut program = match kind {
        ActorProgramKind::Entity => content.actors.world.take_entity_program(actor),
        ActorProgramKind::Detector => content.actors.world.take_detector_program(actor),
    }
    .unwrap()
    .unwrap();
    program
        .set_source_code(Some(source), &mut content.memory)
        .unwrap();
    program.set_owner(Some(actor)).unwrap();
    match kind {
        ActorProgramKind::Entity => content
            .actors
            .world
            .return_entity_program(actor, Some(program)),
        ActorProgramKind::Detector => content
            .actors
            .world
            .return_detector_program(actor, Some(program)),
    }
    .unwrap();
    // Actual Model frame with the evidenced diagnostic name. This fixture
    // attachment does not claim to execute actor Init or GameInit.
    *content.actors.world.actor_frame_mut(actor).unwrap() =
        Some(f.arena.lookup_reference(f.roots[2]));
    actor
}

fn retained(world: &ActorWorld, actor: ActorReferenceId, kind: ActorProgramKind) -> &CProgram {
    match kind {
        ActorProgramKind::Entity => world.entity(actor).unwrap().program().unwrap(),
        ActorProgramKind::Detector => world.detector(actor).unwrap().program().unwrap(),
    }
}

fn init<B: RuntimeBackend, D: MissionProgramDiagnostic>(
    context: &mut MissionProgramWorldContext<'_, B, D>,
    world: &mut ActorWorld,
    kind: ActorProgramKind,
) -> Result<usize, RuntimeError> {
    match kind {
        ActorProgramKind::Entity => {
            context
                .init_entity_programs(world)
                .map_err(|error| match error {
                    EntityTickError::Runtime(error) => error,
                    other => panic!("unexpected actor failure: {other:?}"),
                })
        }
        ActorProgramKind::Detector => {
            context
                .init_detector_programs(world)
                .map_err(|error| match error {
                    DetectorTickError::Runtime(error) => error,
                    other => panic!("unexpected actor failure: {other:?}"),
                })
        }
    }
}

fn tick<B: RuntimeBackend, D: MissionProgramDiagnostic>(
    context: &mut MissionProgramWorldContext<'_, B, D>,
    world: &mut ActorWorld,
    kind: ActorProgramKind,
) -> Result<usize, RuntimeError> {
    let mut budget = DispatchBudget::new(32).unwrap();
    match kind {
        ActorProgramKind::Entity => {
            context
                .tick_entity_programs(world, 7, &mut budget)
                .map_err(|error| match error {
                    EntityTickError::Runtime(error) => error,
                    other => panic!("unexpected actor failure: {other:?}"),
                })
        }
        ActorProgramKind::Detector => context
            .tick_detector_programs(world, 7, &mut budget)
            .map_err(|error| match error {
                DetectorTickError::Runtime(error) => error,
                other => panic!("unexpected actor failure: {other:?}"),
            }),
    }
}

struct CompileStop {
    calls: usize,
    panic_payload: Option<Arc<u32>>,
}
impl RuntimeBackend for CompileStop {
    fn effect(
        &mut self,
        _: RuntimeView<'_>,
        effect: RuntimeEffect,
        _: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        self.calls += 1;
        if effect == RuntimeEffect::AllocateScalar {
            if let Some(payload) = &self.panic_payload {
                panic_any(payload.clone());
            }
            return Err("synthetic compile boundary".into());
        }
        Ok(()) // Only compiler storage notifications are reached in this test.
    }
}

#[test]
fn embedded_compile_error_and_unwind_keep_source_and_consumed_admission() {
    for kind in [ActorProgramKind::Entity, ActorProgramKind::Detector] {
        for unwind in [false, true] {
            let mut f = Fixture::new(&[]);
            let actor = add_program(&mut f, kind, b"dim_act 1");
            let source = retained(&f.loader.content.actors.world, actor, kind)
                .source()
                .unwrap();
            let allocation = source.allocation;
            let storage = source.bytes.as_ptr();
            let payload = Arc::new(0x5152);
            let mut effects = CompileStop {
                calls: 0,
                panic_payload: unwind.then(|| payload.clone()),
            };
            let mut diagnostic = Diagnostic::default();
            let mut context = MissionProgramWorldContext::new(
                &mut f.arena,
                &mut f.scene_reference,
                &mut effects,
                &mut diagnostic,
                locales(),
            );
            let result = catch_unwind(AssertUnwindSafe(|| {
                init(&mut context, &mut f.loader.content.actors.world, kind)
            }));
            if unwind {
                let caught = result.unwrap_err().downcast::<Arc<u32>>().unwrap();
                assert!(Arc::ptr_eq(&caught, &payload));
            } else {
                assert!(result.unwrap().is_err());
            }
            let program = retained(&f.loader.content.actors.world, actor, kind);
            assert_eq!(program.owner(), Some(actor));
            assert_eq!(program.source().unwrap().allocation, allocation);
            assert_eq!(program.source().unwrap().bytes.as_ptr(), storage);
            assert_eq!(
                program.source().unwrap().c_bytes(),
                Some(b"dim_act 1".as_slice())
            );
            assert_eq!(
                program.runtime().unwrap().stage,
                if unwind {
                    RuntimeStage::Initializing
                } else {
                    RuntimeStage::CompileInterrupted
                }
            );
            assert!(program.runtime().unwrap().working_source.is_some());
            let calls = context.effects_mut().calls;
            assert_eq!(
                init(&mut context, &mut f.loader.content.actors.world, kind),
                Err(RuntimeError::Domain("runtime not ready"))
            );
            assert_eq!(context.effects_mut().calls, calls);
            assert_eq!(
                f.loader
                    .content
                    .actors
                    .world
                    .discard_embedded_program_runtimes_host()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                init(&mut context, &mut f.loader.content.actors.world, kind),
                Err(RuntimeError::Domain("fresh source program required"))
            );
            assert_eq!(context.effects_mut().calls, calls);
            f.finish();
        }
    }
}

struct MutatingDiagnostic<'a> {
    memory: &'a mut ContentMemory,
    actor: ActorReferenceId,
    other: ActorReferenceId,
    kind: ActorProgramKind,
    calls: usize,
    panic_payload: Option<Arc<u32>>,
}
impl MissionProgramDiagnostic for MutatingDiagnostic<'_> {
    fn message_beep(
        &mut self,
        world: &mut ActorWorld,
        arena: &mut ModelArena,
        fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        _: &mut FrameReferenceId,
        argument: i32,
    ) -> Result<(), LookupError> {
        assert_eq!(argument, -1);
        assert_eq!(*owner, Some(self.actor));
        self.calls += 1;
        let actor = self.actor;
        let references = world.state(actor).unwrap().references;
        let allocations = (self.memory.actors.len(), self.memory.programs.len());
        let busy = Err(ActorWorldError::Busy(actor));
        let mut frames = ModelActorFrames::borrow(arena, true);
        assert_eq!(world.init(actor, None, &mut frames), busy);
        assert_eq!(world.program_set_event(actor, 0), busy);
        let replacement = CProgram::construct(ProgramSeed {
            address_high_byte: 0,
            padding_0d: [0; 3],
            words_1c_2c: [0; 5],
            padding_31: [0; 3],
            word_40: 0,
            bytes_64: [0; 4],
        });
        match self.kind {
            ActorProgramKind::Entity => {
                assert!(
                    matches!(world.take_entity_program(actor), Err(ActorWorldError::Busy(found)) if found == actor)
                );
                assert_eq!(
                    world.return_entity_program(actor, Some(replacement)),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(
                    world.release_entity(actor, &mut frames, self.memory),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(
                    world.delete_entity(actor, 1, &mut frames, self.memory),
                    Err(ActorWorldError::Busy(actor))
                );
            }
            ActorProgramKind::Detector => {
                assert!(
                    matches!(world.take_detector_program(actor), Err(ActorWorldError::Busy(found)) if found == actor)
                );
                assert_eq!(
                    world.return_detector_program(actor, Some(replacement)),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(
                    world.release_detector(actor, &mut frames, self.memory),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(
                    world.delete_detector(actor, 1, &mut frames, self.memory),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(
                    world.init_detector(actor),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(
                    world.game_init_detector(actor),
                    Err(ActorWorldError::Busy(actor))
                );
            }
        }
        let frames = ModelFrameLookup::borrow(arena, true);
        let mut reader = ActorDataReader::new(&[0; 128]);
        match self.kind {
            ActorProgramKind::Entity => {
                assert_eq!(
                    world.load_human_data(actor, &mut reader),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(
                    world.load_entity_data(actor, &mut reader, &[], None, &frames, self.memory),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(reader.position(), 0);
            }
            ActorProgramKind::Detector => {
                let mut reader = ProgramSliceReader::new(&[0; 128]);
                assert_eq!(
                    world.load_detector_data(actor, &mut reader, &[], None, &frames, self.memory),
                    Err(ActorWorldError::Busy(actor))
                );
                assert_eq!(reader.position(), 0);
            }
        }
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        assert_eq!(world.state(actor).unwrap().references, references);
        assert_eq!(
            (self.memory.actors.len(), self.memory.programs.len()),
            allocations
        );
        assert!(
            matches!(world.discard_embedded_program_runtimes_host(), Err(ActorProgramDiscardError::Detached { actor: found, .. }) if found == actor)
        );
        // The lease protects only its actual program owner, not the world.
        world.set_actor_dead(actor, true).unwrap();
        *fields.command_time = 123;
        world
            .delete_base(
                self.other,
                1,
                &mut ContentFrames {
                    frames: ModelActorFrames::borrow(arena, true),
                    memory: self.memory,
                },
            )
            .unwrap();
        if let Some(payload) = &self.panic_payload {
            panic_any(payload.clone());
        }
        Err(LookupError::Backend("synthetic diagnostic boundary".into()))
    }
}

#[test]
fn embedded_dispatch_keeps_actual_program_after_mutation_delete_other_error_and_unwind() {
    for kind in [ActorProgramKind::Entity, ActorProgramKind::Detector] {
        for unwind in [false, true] {
            let mut f = Fixture::new(&[]);
            let actor = add_program(
                &mut f,
                kind,
                b"commandblock 1\ndim_act 1\nfindactor 0,\"alpha\"\ncommandblock 0",
            );
            let mut diagnostic = Diagnostic::default();
            init(
                &mut MissionProgramWorldContext::new(
                    &mut f.arena,
                    &mut f.scene_reference,
                    &mut f.effects,
                    &mut diagnostic,
                    locales(),
                ),
                &mut f.loader.content.actors.world,
                kind,
            )
            .unwrap();
            let program = retained(&f.loader.content.actors.world, actor, kind);
            let source_allocation = program.source().unwrap().allocation;
            let source_storage = program.source().unwrap().bytes.as_ptr();
            let instructions = program.runtime().unwrap().instructions.active().as_ptr();
            let payload = Arc::new(0x6162);
            let content = &mut f.loader.content;
            let mut diagnostic = MutatingDiagnostic {
                memory: &mut content.memory,
                actor,
                other: f.actors[0],
                kind,
                calls: 0,
                panic_payload: unwind.then(|| payload.clone()),
            };
            let mut context = MissionProgramWorldContext::new(
                &mut f.arena,
                &mut f.scene_reference,
                &mut f.effects,
                &mut diagnostic,
                locales(),
            );
            let result = catch_unwind(AssertUnwindSafe(|| {
                tick(&mut context, &mut content.actors.world, kind)
            }));
            if unwind {
                let caught = result.unwrap_err().downcast::<Arc<u32>>().unwrap();
                assert!(Arc::ptr_eq(&caught, &payload));
            } else {
                assert_eq!(
                    result.unwrap(),
                    Err(RuntimeError::Lookup(LookupError::Backend(
                        "synthetic diagnostic boundary".into()
                    )))
                );
            }
            let program = retained(&content.actors.world, actor, kind);
            assert_eq!(program.owner(), Some(actor));
            assert_eq!(program.source().unwrap().allocation, source_allocation);
            assert_eq!(program.source().unwrap().bytes.as_ptr(), source_storage);
            let runtime = program.runtime().unwrap();
            assert_eq!(runtime.instructions.active().as_ptr(), instructions);
            assert_eq!(runtime.stage, RuntimeStage::ProcessInterrupted);
            assert_eq!(runtime.fields.command_time, 123);
            assert_eq!(content.actors.world.phase(actor), Ok(ActorPhase::Ready));
            assert!(content.actors.world.actor_dead(actor).unwrap());
            assert_eq!(
                content.actors.world.inspect(f.actors[0]).unwrap().0,
                ActorPhase::Dead
            );
            assert_eq!(
                tick(&mut context, &mut content.actors.world, kind),
                Err(RuntimeError::Domain("runtime not ready"))
            );
            assert_eq!(
                init(&mut context, &mut content.actors.world, kind),
                Err(RuntimeError::Domain("runtime not ready"))
            );
            assert_eq!(diagnostic.calls, 1);
            assert_eq!(
                content
                    .actors
                    .world
                    .discard_embedded_program_runtimes_host()
                    .unwrap()
                    .len(),
                1
            );
            f.finish();
        }
    }
}

#[test]
fn embedded_passes_refuse_detached_manual_programs_until_returned() {
    let mut f = Fixture::new(&[]);
    let entity = add_program(&mut f, ActorProgramKind::Entity, b"dim_act 1");
    let detector = add_program(&mut f, ActorProgramKind::Detector, b"dim_frm 1");
    let world = &mut f.loader.content.actors.world;
    let entity_program = world.take_entity_program(entity).unwrap();
    let detector_program = world.take_detector_program(detector).unwrap();
    let mut diagnostic = Diagnostic::default();
    let mut context = MissionProgramWorldContext::new(
        &mut f.arena,
        &mut f.scene_reference,
        &mut f.effects,
        &mut diagnostic,
        locales(),
    );
    assert_eq!(
        context.init_entity_programs(world),
        Err(EntityTickError::Actor(entity))
    );
    assert_eq!(
        context.init_detector_programs(world),
        Err(DetectorTickError::Actor(detector))
    );
    let mut budget = DispatchBudget::new(8).unwrap();
    assert_eq!(
        context.tick_entity_programs(world, 1, &mut budget),
        Err(EntityTickError::Actor(entity))
    );
    assert_eq!(
        context.tick_detector_programs(world, 1, &mut budget),
        Err(DetectorTickError::Actor(detector))
    );
    assert_eq!(budget.remaining(), 8);
    assert!(context.effects_mut().events.is_empty());
    world.return_entity_program(entity, entity_program).unwrap();
    world
        .return_detector_program(detector, detector_program)
        .unwrap();
    assert_eq!(context.init_entity_programs(world), Ok(1));
    assert_eq!(context.init_detector_programs(world), Ok(1));
    world.discard_embedded_program_runtimes_host().unwrap();
    f.finish();
}
