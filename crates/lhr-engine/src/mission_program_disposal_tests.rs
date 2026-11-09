//! Stopped embedded programs are retired before any scene/source destructor.
use super::*;
use crate::actor_world::{ActorPhase, ActorProgramKind};
use crate::original_program::{RuntimeBackend, RuntimeEffect, RuntimeView};
use crate::program_parameters::ScalarParameter;

struct Storage;
impl RuntimeBackend for Storage {
    fn effect(
        &mut self,
        _: RuntimeView<'_>,
        _: RuntimeEffect,
        _: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        Ok(()) // Test-only storage boundary; no gameplay command is executed.
    }
}
fn fixture() -> Fixture {
    let program = program_record(b"detector", b"dim_act 2");
    let data = [vec![0], 9u32.to_le_bytes().to_vec(), program[6..].to_vec()].concat();
    let input = scene2(
        &[
            chunk(0xae20, &actor_record(b"part", 5, &data)),
            chunk(0xae50, &program_record(b"global", b"dim_frm 1")),
        ]
        .concat(),
    );
    Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&input), None)
}
fn initialize(session: &mut MissionLoadSession) -> crate::script_scheduler::ActorReferenceId {
    let world = &mut session.loader.as_mut().unwrap().content.actors.world;
    let actor = world.owned_actor_references().next().unwrap();
    let mut program = world.take_detector_program(actor).unwrap().unwrap();
    let result = program.init_fresh_runtime(&mut Storage);
    world.return_detector_program(actor, Some(program)).unwrap();
    result.unwrap();
    actor
}

#[test]
fn embedded_runtime_refuses_disposal_before_any_destructor_then_retires_to_zero() {
    let fixture = fixture();
    let mut previous = None;
    for _ in 0..3 {
        let mut session = fixture.session();
        load(&mut session, &[]).unwrap();
        let actor = initialize(&mut session);
        assert_ne!(Some(actor), previous);
        previous = Some(actor);
        let before = session.owned_counts().unwrap();
        let source = session
            .loader()
            .unwrap()
            .content
            .actors
            .world
            .detector(actor)
            .unwrap()
            .program()
            .unwrap()
            .source()
            .unwrap()
            .bytes
            .clone();
        assert_eq!(
            session.discard_terminal(),
            Err(MissionDiscardCallError::ProgramRuntimeRetained)
        );
        assert_eq!(
            session.discard_progress().phase,
            MissionDiscardPhase::NotStarted
        );
        assert!(session.discard_failure().is_none());
        assert_eq!(session.owned_counts().unwrap(), before);
        assert_eq!(
            session.loader().unwrap().content.actors.world.phase(actor),
            Ok(ActorPhase::Ready)
        );
        let reports = session.discard_actor_program_runtimes_host().unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].actor, actor);
        assert_eq!(reports[0].kind, ActorProgramKind::Detector);
        assert_eq!(reports[0].resources.scalar_allocations, 1);
        let program = session
            .loader()
            .unwrap()
            .content
            .actors
            .world
            .detector(actor)
            .unwrap()
            .program()
            .unwrap();
        assert!(program.runtime().is_none());
        assert_eq!(program.source().unwrap().bytes, source);
        assert!(
            session
                .discard_actor_program_runtimes_host()
                .unwrap()
                .is_empty()
        );
        discard(&mut session);
        assert!(session.discard_actor_program_runtimes_host().is_err());
    }
}

#[test]
fn detached_program_must_return_before_host_disposal_or_source_destruction() {
    let fixture = fixture();
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let actor = initialize(&mut session);
    let program = session
        .loader
        .as_mut()
        .unwrap()
        .content
        .actors
        .world
        .take_detector_program(actor)
        .unwrap();
    let before = session.owned_counts().unwrap();
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::ProgramRuntimeRetained)
    );
    let error = session.discard_actor_program_runtimes_host().unwrap_err();
    assert!(error.contains("Detached"), "{error}");
    assert_eq!(session.owned_counts().unwrap(), before);
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::NotStarted
    );
    session
        .loader
        .as_mut()
        .unwrap()
        .content
        .actors
        .world
        .return_detector_program(actor, program)
        .unwrap();
    session.discard_actor_program_runtimes_host().unwrap();
    discard(&mut session);
}

#[test]
fn content_owner_also_refuses_before_destroying_unrelated_global_sources() {
    let fixture = fixture();
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let actor = initialize(&mut session);
    let before = session.owned_counts().unwrap();
    let result = session
        .loader
        .as_mut()
        .unwrap()
        .content
        .discard_terminal(&mut session.arena);
    assert!(result.unwrap_err().contains("program runtime"));
    assert_eq!(session.owned_counts().unwrap(), before);
    assert_eq!(
        session.loader().unwrap().content.actors.world.phase(actor),
        Ok(ActorPhase::Ready)
    );
    session.discard_actor_program_runtimes_host().unwrap();
    discard(&mut session);
}

#[test]
fn interrupted_embedded_compilation_keeps_source_for_explicit_host_discard() {
    struct FailAllocate;
    impl RuntimeBackend for FailAllocate {
        fn effect(
            &mut self,
            _: RuntimeView<'_>,
            effect: RuntimeEffect,
            _: Option<&ScalarParameter>,
        ) -> Result<(), String> {
            if effect == RuntimeEffect::AllocateScalar {
                Err("injected allocation boundary failure".into())
            } else {
                Ok(())
            }
        }
    }
    let fixture = fixture();
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    let world = &mut session.loader.as_mut().unwrap().content.actors.world;
    let actor = world.owned_actor_references().next().unwrap();
    let mut program = world.take_detector_program(actor).unwrap().unwrap();
    assert!(program.init_fresh_runtime(&mut FailAllocate).is_err());
    assert_eq!(
        program.runtime().unwrap().stage,
        crate::original_program::RuntimeStage::CompileInterrupted
    );
    world.return_detector_program(actor, Some(program)).unwrap();
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::ProgramRuntimeRetained)
    );
    let reports = session.discard_actor_program_runtimes_host().unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].resources.working_source_bytes > 0);
    discard(&mut session);
}
