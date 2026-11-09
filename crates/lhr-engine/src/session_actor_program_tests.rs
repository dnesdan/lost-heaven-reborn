//! Explicit session passes over actual scene2-loaded embedded program owners.
use super::*;
use crate::actor_world::{ActorProgramKind, ActorWorld};
use crate::entity_actor::EntityProcessState;
use crate::mission_content_loading::{DetectorTickError, EntityTickError};
use crate::original_program::{CProgram, RuntimeError, RuntimeStage, RuntimeView};
use crate::script_lookup::LookupError;
use crate::script_scheduler::ActorReferenceId;
use std::panic::{catch_unwind, panic_any, AssertUnwindSafe};
use std::sync::Arc;

fn locales() -> ProgramWorldLocales {
    ProgramWorldLocales {
        game_default_c_locale: true,
        ls3df_default_c_locale: true,
    }
}
fn fixture(entity: Option<&[u8]>, detector: Option<&[u8]>) -> Fixture {
    let mut body = Vec::new();
    if let Some(source) = entity {
        // A real empty Model is sufficient for the admitted Human Init prefix.
        body.extend(chunk(0x4000, &object(9, b"entity", &[])));
        let program = program_record(b"entity program", source);
        let data = [
            vec![6],
            1u32.to_le_bytes().to_vec(),
            0u32.to_le_bytes().to_vec(),
            vec![0; 64],
            vec![0],
            program[6..].to_vec(),
        ]
        .concat();
        body.extend(chunk(0xae20, &actor_record(b"entity", 27, &data)));
    }
    if let Some(source) = detector {
        let program = program_record(b"detector program", source);
        let data = [vec![0], 9u32.to_le_bytes().to_vec(), program[6..].to_vec()].concat();
        body.extend(chunk(0xae20, &actor_record(b"detector", 5, &data)));
    }
    Fixture::new(
        Some(&world(&[dummy(0, b"detector")])),
        Some(&scene2(&body)),
        None,
    )
}
fn actor(session: &MissionLoadSession, kind: ActorProgramKind) -> ActorReferenceId {
    let world = &session.loader().unwrap().content.actors.world;
    world
        .owned_actor_references()
        .find(|actor| match kind {
            ActorProgramKind::Entity => world.entity(*actor).is_ok(),
            ActorProgramKind::Detector => world.detector(*actor).is_ok(),
        })
        .unwrap()
}
fn program(session: &MissionLoadSession, kind: ActorProgramKind) -> &CProgram {
    let actor = actor(session, kind);
    let world = &session.loader().unwrap().content.actors.world;
    match kind {
        ActorProgramKind::Entity => world.entity(actor).unwrap().program().unwrap(),
        ActorProgramKind::Detector => world.detector(actor).unwrap().program().unwrap(),
    }
}
#[derive(Default)]
struct Diagnostic {
    calls: usize,
    panic_payload: Option<Arc<u32>>,
}
impl MissionProgramDiagnostic for Diagnostic {
    fn message_beep(
        &mut self,
        world: &mut ActorWorld,
        _: &mut ModelArena,
        fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        _: &mut FrameReferenceId,
        _: i32,
    ) -> Result<(), LookupError> {
        self.calls += 1;
        assert_eq!(crate::native_precision::current_precision(), Some(false));
        world.set_actor_dead(owner.unwrap(), true).unwrap();
        *fields.command_time = 321;
        if let Some(payload) = &self.panic_payload {
            panic_any(payload.clone());
        }
        Err(LookupError::Backend("unexpected session diagnostic".into()))
    }
}
fn initialize(
    session: &mut MissionLoadSession,
    kind: ActorProgramKind,
    diagnostic: &mut Diagnostic,
) -> Result<usize, SessionActorProgramError> {
    match kind {
        ActorProgramKind::Entity => session.initialize_owned_entity_programs(diagnostic, locales()),
        ActorProgramKind::Detector => {
            session.initialize_owned_detector_programs(diagnostic, locales())
        }
    }
}
fn tick(
    session: &mut MissionLoadSession,
    kind: ActorProgramKind,
    budget: &mut DispatchBudget,
    diagnostic: &mut Diagnostic,
) -> Result<usize, SessionActorProgramError> {
    match kind {
        ActorProgramKind::Entity => {
            session.tick_owned_entity_programs(16, budget, diagnostic, locales())
        }
        ActorProgramKind::Detector => {
            session.tick_owned_detector_programs(16, budget, diagnostic, locales())
        }
    }
}
fn finish(session: &mut MissionLoadSession) {
    session.discard_actor_program_runtimes_host().unwrap();
    discard(session);
}

#[test]
fn session_actor_programs_share_game_and_actor_owners_and_restart_cleanly() {
    let fixture = fixture(
        Some(b"commandblock 1\ngame_nightmission 1\nenemy_vidim 1\ncommandblock 0"),
        Some(b"commandblock 1\nnoanimpreload\ndetector_setsignal -1,1\ncommandblock 0"),
    );
    let mut previous = None;
    for _ in 0..3 {
        let mut session = fixture.session();
        let mut diagnostic = Diagnostic::default();
        load(&mut session, &[]).unwrap();
        let entity = actor(&session, ActorProgramKind::Entity);
        let detector = actor(&session, ActorProgramKind::Detector);
        assert_ne!(Some(entity), previous);
        previous = Some(entity);
        assert_eq!(
            program(&session, ActorProgramKind::Entity).owner(),
            Some(entity)
        );
        assert_eq!(
            program(&session, ActorProgramKind::Detector).owner(),
            Some(detector)
        );
        let source_storage = program(&session, ActorProgramKind::Entity)
            .source()
            .unwrap()
            .bytes
            .as_ptr();
        session.game_inline.set_anim_preload(true);
        session
            .loader
            .as_mut()
            .unwrap()
            .content
            .actors
            .world
            .store_entity_process_state(
                entity,
                &EntityProcessState {
                    process_flags: 0,
                    visibility_bits: 3,
                    shut_up: 0,
                },
            )
            .unwrap();
        for kind in [ActorProgramKind::Entity, ActorProgramKind::Detector] {
            assert_eq!(initialize(&mut session, kind, &mut diagnostic), Ok(1));
            assert_eq!(initialize(&mut session, kind, &mut diagnostic), Ok(0));
        }
        let scene = session.scene_reference();
        let mut budget = DispatchBudget::new(32).unwrap();
        assert_eq!(
            tick(
                &mut session,
                ActorProgramKind::Entity,
                &mut budget,
                &mut diagnostic
            ),
            Ok(1)
        );
        let after_entity = budget.remaining();
        assert_eq!(
            tick(
                &mut session,
                ActorProgramKind::Detector,
                &mut budget,
                &mut diagnostic
            ),
            Ok(1)
        );
        assert!(budget.remaining() < after_entity);
        assert!(session.game_state().night_mission());
        assert!(!session.game_state().anim_preload());
        assert_eq!(session.scene_reference(), scene);
        let world = &session.loader().unwrap().content.actors.world;
        assert_eq!(
            world.entity_process_state(entity).unwrap().visibility_bits,
            1
        );
        assert!(world.detector_signal(detector).unwrap());
        assert_eq!(
            program(&session, ActorProgramKind::Entity)
                .source()
                .unwrap()
                .bytes
                .as_ptr(),
            source_storage
        );
        assert_eq!(diagnostic.calls, 0);
        assert_eq!(crate::native_precision::current_precision(), None);
        assert_eq!(
            session.discard_actor_program_runtimes_host().unwrap().len(),
            2
        );
        for kind in [ActorProgramKind::Entity, ActorProgramKind::Detector] {
            assert!(initialize(&mut session, kind, &mut diagnostic).is_err());
        }
        discard(&mut session);
    }
}

#[test]
fn session_actor_passes_keep_one_budget_across_kinds_and_refuse_replay() {
    let fixture = fixture(Some(b"game_nightmission 1"), Some(b"noanimpreload"));
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    session.game_inline.set_anim_preload(true);
    let mut diagnostic = Diagnostic::default();
    initialize(&mut session, ActorProgramKind::Entity, &mut diagnostic).unwrap();
    initialize(&mut session, ActorProgramKind::Detector, &mut diagnostic).unwrap();
    let mut budget = DispatchBudget::new(1).unwrap();
    assert_eq!(
        tick(
            &mut session,
            ActorProgramKind::Entity,
            &mut budget,
            &mut diagnostic
        ),
        Ok(1)
    );
    assert_eq!(budget.remaining(), 0);
    assert_eq!(
        tick(
            &mut session,
            ActorProgramKind::Detector,
            &mut budget,
            &mut diagnostic
        ),
        Err(SessionActorProgramError::Detector(
            DetectorTickError::Runtime(RuntimeError::Scheduler("BudgetExhausted".into()))
        ))
    );
    assert_eq!(
        program(&session, ActorProgramKind::Detector)
            .runtime()
            .unwrap()
            .fields
            .program_counter(),
        0
    );
    assert_eq!(
        program(&session, ActorProgramKind::Detector)
            .runtime()
            .unwrap()
            .stage,
        RuntimeStage::ProcessInterrupted
    );
    assert!(session.game_state().night_mission());
    assert!(session.game_state().anim_preload());
    assert_eq!(
        tick(
            &mut session,
            ActorProgramKind::Detector,
            &mut DispatchBudget::new(8).unwrap(),
            &mut diagnostic
        ),
        Err(SessionActorProgramError::Detector(
            DetectorTickError::Runtime(RuntimeError::Domain("runtime not ready"))
        ))
    );
    finish(&mut session);
}

#[test]
fn session_actor_program_admission_refuses_incomplete_and_terminal_sessions() {
    let fixture = fixture(Some(b"game_nightmission 1"), Some(b"noanimpreload"));
    let mut session = fixture.session();
    let mut diagnostic = Diagnostic::default();
    let mut budget = DispatchBudget::new(8).unwrap();
    for kind in [ActorProgramKind::Entity, ActorProgramKind::Detector] {
        assert!(matches!(
            initialize(&mut session, kind, &mut diagnostic),
            Err(SessionActorProgramError::SourceUnavailable(_))
        ));
        assert!(matches!(
            tick(&mut session, kind, &mut budget, &mut diagnostic),
            Err(SessionActorProgramError::SourceUnavailable(_))
        ));
    }
    load(&mut session, &[]).unwrap();
    discard(&mut session);
    for kind in [ActorProgramKind::Entity, ActorProgramKind::Detector] {
        assert!(matches!(
            initialize(&mut session, kind, &mut diagnostic),
            Err(SessionActorProgramError::SourceUnavailable(_))
        ));
        assert!(matches!(
            tick(&mut session, kind, &mut budget, &mut diagnostic),
            Err(SessionActorProgramError::SourceUnavailable(_))
        ));
    }
    assert_eq!(budget.remaining(), 8);
    assert_eq!(diagnostic.calls, 0);
    assert!(!session.game_state().night_mission());
}

#[test]
fn session_actor_unwind_retains_game_prefix_program_and_precision_owner() {
    for kind in [ActorProgramKind::Entity, ActorProgramKind::Detector] {
        let source = b"commandblock 1\ngame_nightmission 1\ndim_act 1\nfindactor 0,\"unused\"\nnoanimpreload\ncommandblock 0";
        let fixture = match kind {
            ActorProgramKind::Entity => fixture(Some(source), None),
            ActorProgramKind::Detector => fixture(None, Some(source)),
        };
        let mut session = fixture.session();
        load(&mut session, &[]).unwrap();
        session.game_inline.set_anim_preload(true);
        let actor = actor(&session, kind);
        let frame = session
            .loader()
            .unwrap()
            .content
            .actors
            .world
            .state(actor)
            .unwrap()
            .frame
            .unwrap();
        session
            .arena
            .set_frame_name(FrameKey(frame.identity()), Some(b"chcipnuti chlapka"))
            .unwrap();
        let source_storage = program(&session, kind).source().unwrap().bytes.as_ptr();
        let payload = Arc::new(0x7172);
        let mut diagnostic = Diagnostic {
            calls: 0,
            panic_payload: Some(payload.clone()),
        };
        initialize(&mut session, kind, &mut diagnostic).unwrap();
        let instructions = program(&session, kind)
            .runtime()
            .unwrap()
            .instructions
            .active()
            .as_ptr();
        let caught = catch_unwind(AssertUnwindSafe(|| {
            tick(
                &mut session,
                kind,
                &mut DispatchBudget::new(32).unwrap(),
                &mut diagnostic,
            )
        }))
        .unwrap_err()
        .downcast::<Arc<u32>>()
        .unwrap();
        assert!(Arc::ptr_eq(&caught, &payload));
        assert_eq!(crate::native_precision::current_precision(), None);
        assert!(!session.precision_state().precision_raised);
        assert!(session.game_state().night_mission());
        assert!(session.game_state().anim_preload());
        let program = program(&session, kind);
        assert_eq!(program.owner(), Some(actor));
        assert_eq!(program.source().unwrap().bytes.as_ptr(), source_storage);
        assert_eq!(
            program.runtime().unwrap().instructions.active().as_ptr(),
            instructions
        );
        assert_eq!(
            program.runtime().unwrap().stage,
            RuntimeStage::ProcessInterrupted
        );
        assert_eq!(program.runtime().unwrap().fields.command_time, 321);
        assert!(session
            .loader()
            .unwrap()
            .content
            .actors
            .world
            .actor_dead(actor)
            .unwrap());
        let expected = RuntimeError::Domain("runtime not ready");
        let error = match kind {
            ActorProgramKind::Entity => {
                SessionActorProgramError::Entity(EntityTickError::Runtime(expected))
            }
            ActorProgramKind::Detector => {
                SessionActorProgramError::Detector(DetectorTickError::Runtime(expected))
            }
        };
        assert_eq!(
            tick(
                &mut session,
                kind,
                &mut DispatchBudget::new(8).unwrap(),
                &mut diagnostic
            ),
            Err(error)
        );
        assert!(initialize(&mut session, kind, &mut diagnostic).is_err());
        assert_eq!(diagnostic.calls, 1);
        finish(&mut session);
    }
}
