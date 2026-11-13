//! Installed synthetic source through the retained session's public VM API.
use super::*;
use crate::actor_world::{ActorSource, ActorWorld};
use crate::mission_content_loading::MissionContentOwner;
use crate::original_program::{CProgram, RuntimeBackend, RuntimeEffect, RuntimeStage, RuntimeView};
use crate::program_parameters::ScalarParameter;
use crate::script_lookup::LookupError;
use crate::script_scheduler::ActorReferenceId;

#[derive(Default)]
struct Effects {
    calls: usize,
    panic_allocate: bool,
}
impl RuntimeBackend for Effects {
    fn effect(
        &mut self,
        _: RuntimeView<'_>,
        effect: RuntimeEffect,
        _: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        assert_eq!(crate::native_precision::current_precision(), Some(true));
        self.calls += 1;
        crate::native_precision::record_precision(true);
        assert!(
            !(self.panic_allocate && effect == RuntimeEffect::AllocateFrames(1)),
            "session allocation callback unwind"
        );
        Ok(())
    }
}
struct Diagnostic;
impl MissionProgramDiagnostic for Diagnostic {
    fn message_beep(
        &mut self,
        _: &mut ActorWorld,
        _: &mut ModelArena,
        _: RuntimeView<'_>,
        _: &mut Option<ActorReferenceId>,
        _: &mut FrameReferenceId,
        _: i32,
    ) -> Result<(), LookupError> {
        panic!("these actual global programs have no diagnostic owner")
    }
}
fn locales() -> ProgramWorldLocales {
    ProgramWorldLocales {
        game_default_c_locale: true,
        ls3df_default_c_locale: true,
    }
}
fn source(program: &[u8]) -> Vec<u8> {
    scene2(
        &[
            chunk(0xae20, &actor_record(b"alpha", 1, b"")),
            chunk(0xae50, &program_record(b"main", program)),
        ]
        .concat(),
    )
}
fn loaded(fixture: &Fixture) -> MissionLoadSession {
    let mut session = MissionLoadSession::new(
        lhr_assets::Installation::open(&fixture.0).unwrap(),
        Pc24PrecisionState::new(true),
    )
    .unwrap();
    load(&mut session, &[]).unwrap();
    session
}
fn finish(session: &mut MissionLoadSession, effects: &mut Effects, done: bool) {
    if done {
        session.global_program_game_done(effects).unwrap();
    }
    session.discard_global_program_runtimes_host().unwrap();
    discard(session);
    assert!(matches!(
        session.scene(),
        Err(MissionSceneSelectionError::InvalidScene { .. })
    ));
}
/// An explicitly supplied empty Game-global collection, never a default
/// successful Game implementation. Mutation/panic instrument its call boundary.
struct EmptyGamePrograms {
    programs: Vec<CProgram>,
    calls: usize,
    redirect: Option<FrameReferenceId>,
    unload: Option<ActorReferenceId>,
    panic: bool,
}
impl EmptyGamePrograms {
    fn new(redirect: Option<FrameReferenceId>) -> Self {
        Self {
            programs: Vec::new(),
            calls: 0,
            redirect,
            unload: None,
            panic: false,
        }
    }
}
impl GameGlobalProgramWorldRunner<Effects, Diagnostic> for EmptyGamePrograms {
    fn run_game_global(
        &mut self,
        mission: &mut MissionContentOwner,
        _: &[u8],
        _: &mut DispatchBudget,
        context: &mut MissionProgramWorldContext<'_, Effects, Diagnostic>,
    ) -> Result<bool, String> {
        assert!(self.programs.is_empty());
        assert_eq!(crate::native_precision::current_precision(), Some(true));
        self.calls += 1;
        if let Some(reference) = self.redirect {
            *context.scene_mut() = reference;
        }
        if let Some(actor) = self.unload {
            assert!(mission.actors.world.unload_actor(Some(actor)));
        }
        crate::native_precision::record_precision(true);
        assert!(!self.panic, "session Game callback mutation then unwind");
        Ok(false)
    }
}
fn named(
    session: &mut MissionLoadSession,
    name: &[u8],
    game: &mut EmptyGamePrograms,
    effects: &mut Effects,
) -> Result<NamedGlobalProgramOutcome, NamedGlobalProgramError> {
    session.global_program_run_in_world(
        name,
        game,
        &mut DispatchBudget::new(32).unwrap(),
        effects,
        &mut Diagnostic,
        locales(),
    )
}
fn restore_scene(
    session: &mut MissionLoadSession,
    original: FrameReferenceId,
    effects: &mut Effects,
) {
    // An explicit new callback restores the same actual selection slot. Neither
    // the execution API nor terminal disposal silently substitutes an old key.
    assert_eq!(
        named(
            session,
            b"absent",
            &mut EmptyGamePrograms::new(Some(original)),
            effects,
        ),
        Ok(NamedGlobalProgramOutcome::NoMatch)
    );
    assert_eq!(session.scene_reference(), Some(original));
    assert!(session.scene().unwrap().is_some());
}

#[test]
fn retained_session_executes_actual_loaded_registers_and_scopes_its_precision_owner() {
    let bytes = source(b"commandblock 1\ndim_act 1\ndim_frm 2\nfindactor 0,\"alpha\"\nfindframe 0,\"beta\"\nfindframe 1,\"absent\"\ncommandblock 0");
    let fixture = Fixture::new(
        Some(&world(&[dummy(0, b"alpha"), dummy(0, b"beta")])),
        Some(&bytes),
        None,
    );
    let mut session = loaded(&fixture);
    let original = session.scene_reference().unwrap();
    let mut effects = Effects::default();
    // Loading alone never synthesizes GameInit or a compiled executable owner.
    assert!(session.loader().unwrap().content.programs()[0]
        .runtime()
        .is_none());
    assert!(matches!(
        session.process_program_in_world(
            0,
            7,
            &mut DispatchBudget::new(32).unwrap(),
            &mut effects,
            &mut Diagnostic,
            locales()
        ),
        Err(MissionGlobalProgramError::Runtime(
            crate::original_program::RuntimeError::Domain("uninitialized program")
        ))
    ));
    assert_eq!(effects.calls, 0);
    session.global_program_game_init(&mut effects).unwrap();
    let source = session.loader().unwrap().content.programs()[0]
        .source()
        .unwrap()
        .bytes
        .clone();
    let mut outer = Pc24PrecisionState::new(false);
    outer.run(|| {
        session
            .process_program_in_world(
                0,
                7,
                &mut DispatchBudget::new(32).unwrap(),
                &mut effects,
                &mut Diagnostic,
                locales(),
            )
            .unwrap();
        assert_eq!(crate::native_precision::current_precision(), Some(false));
    });
    assert!(!outer.precision_raised());
    assert!(session.precision_state().precision_raised());
    assert_eq!(crate::native_precision::current_precision(), None);
    let loader = session.loader().unwrap();
    let actor = loader.content.actors.world.source(ActorSource::Primary)[0].unwrap();
    let runtime = loader.content.programs()[0].runtime().unwrap();
    assert_eq!(
        runtime.fields.actors.as_ref().unwrap()[0].reference(),
        Ok(Some(&actor))
    );
    let frame = *runtime.fields.frames.as_ref().unwrap()[0]
        .reference()
        .unwrap()
        .unwrap();
    assert_eq!(
        session
            .arena()
            .frame(FrameKey(frame.identity()))
            .unwrap()
            .name_bytes,
        b"beta"
    );
    assert_eq!(
        runtime.fields.frames.as_ref().unwrap()[1].reference(),
        Ok(None)
    );
    assert_eq!(runtime.fields.program_counter(), 7);
    assert_eq!(loader.content.programs()[0].source().unwrap().bytes, source);
    assert_eq!(session.scene_reference(), Some(original));
    finish(&mut session, &mut effects, true);
}

#[test]
fn named_session_call_rereads_single_scene_slot_and_never_projects_a_foreign_alias() {
    let bytes = source(b"commandblock 1\ndim_frm 1\nfindframe 0,\"alpha\"\ncommandblock 0");
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"alpha")])), Some(&bytes), None);
    let mut foreign = loaded(&fixture);
    for use_foreign in [false, true] {
        let mut session = loaded(&fixture);
        let original = session.scene_reference().unwrap();
        let scene = session.scene().unwrap().unwrap();
        let replacement = if use_foreign {
            let reference = foreign.scene_reference().unwrap();
            assert_eq!(reference.identity(), original.identity());
            assert_ne!(reference, original);
            reference
        } else {
            session
                .arena()
                .lookup_reference(session.arena().scene(scene).unwrap().sectors[0].frame())
        };
        let mut effects = Effects::default();
        session.global_program_game_init(&mut effects).unwrap();
        assert_eq!(
            named(
                &mut session,
                b"main",
                &mut EmptyGamePrograms::new(None),
                &mut effects
            ),
            Ok(NamedGlobalProgramOutcome::MissionHandled { index: 0 })
        );
        assert!(named(
            &mut session,
            b"main",
            &mut EmptyGamePrograms::new(Some(replacement)),
            &mut effects
        )
        .is_err());
        assert_eq!(session.scene_reference(), Some(replacement));
        if use_foreign {
            assert!(
                matches!(session.scene(), Err(MissionSceneSelectionError::ForeignReference(reference)) if reference == replacement)
            );
            assert!(session.scene_close_report().is_none());
        } else {
            assert!(
                matches!(session.scene(), Err(MissionSceneSelectionError::InvalidScene { reference, .. }) if reference == replacement)
            );
        }
        assert_eq!(session.driver().scenes(), &[scene]);
        assert!(session.arena().scene(scene).is_ok());
        assert_eq!(
            session.loader().unwrap().content.programs()[0]
                .runtime()
                .unwrap()
                .fields
                .program_counter(),
            2
        );
        restore_scene(&mut session, original, &mut effects);
        finish(&mut session, &mut effects, true);
    }
    discard(&mut foreign);
}

#[test]
fn game_callback_unwind_retains_actual_session_selection_world_and_precision() {
    let bytes = source(b"dim_frm 1");
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"alpha")])), Some(&bytes), None);
    let mut session = loaded(&fixture);
    let original = session.scene_reference().unwrap();
    let scene = session.scene().unwrap().unwrap();
    let sector = session
        .arena()
        .lookup_reference(session.arena().scene(scene).unwrap().sectors[0].frame());
    let actor = session
        .loader()
        .unwrap()
        .content
        .actors
        .world
        .source(ActorSource::Primary)[0]
        .unwrap();
    let mut game = EmptyGamePrograms::new(Some(sector));
    game.unload = Some(actor);
    game.panic = true;
    let mut effects = Effects::default();
    session.global_program_game_init(&mut effects).unwrap();
    let mut outer = Pc24PrecisionState::new(false);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| outer.run(|| {
            let _ = named(&mut session, b"main", &mut game, &mut effects);
        })))
        .is_err()
    );
    assert_eq!(crate::native_precision::current_precision(), None);
    assert!(!outer.precision_raised());
    assert!(session.precision_state().precision_raised());
    assert_eq!(game.calls, 1);
    assert_eq!(session.scene_reference(), Some(sector));
    assert!(session.scene().is_err());
    assert!(!session
        .loader()
        .unwrap()
        .content
        .actors
        .world
        .source(ActorSource::Primary)
        .contains(&Some(actor)));
    assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
    assert!(session.failure().is_none());
    restore_scene(&mut session, original, &mut effects);
    finish(&mut session, &mut effects, true);
}

#[test]
fn process_effect_unwind_keeps_actual_runtime_interrupted_and_restores_outer_precision() {
    let bytes = source(b"dim_frm 1");
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"alpha")])), Some(&bytes), None);
    let mut session = loaded(&fixture);
    let mut effects = Effects::default();
    session.global_program_game_init(&mut effects).unwrap();
    effects.panic_allocate = true;
    let mut outer = Pc24PrecisionState::new(false);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| outer.run(|| {
            let _ = session.process_program_in_world(
                0,
                7,
                &mut DispatchBudget::new(8).unwrap(),
                &mut effects,
                &mut Diagnostic,
                locales(),
            );
        })))
        .is_err()
    );
    assert_eq!(crate::native_precision::current_precision(), None);
    assert!(!outer.precision_raised());
    assert!(session.precision_state().precision_raised());
    let runtime = session.loader().unwrap().content.programs()[0]
        .runtime()
        .unwrap();
    assert_eq!(runtime.stage, RuntimeStage::ProcessInterrupted);
    assert_eq!(runtime.fields.frame_count, 1);
    assert!(runtime.fields.frames.is_none());
    assert_eq!(runtime.fields.program_counter(), 0);
    let reached = effects.calls;
    assert!(session
        .process_program_in_world(
            0,
            7,
            &mut DispatchBudget::new(8).unwrap(),
            &mut effects,
            &mut Diagnostic,
            locales()
        )
        .is_err());
    assert_eq!(effects.calls, reached);
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::ProgramRuntimeRetained)
    );
    finish(&mut session, &mut effects, false);
}

#[test]
fn session_vm_admission_preserves_not_started_stopped_and_terminal_owners() {
    let bytes = source(b"dim_frm 1");
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"alpha")])), Some(&bytes), None);
    let missing = Fixture::new(None, Some(&bytes), None);
    let mut before_load = fixture.session();
    let mut stopped = missing.session();
    assert_eq!(
        load(&mut stopped, &[]),
        Err(LoadStop::Step(
            crate::mission_open_order::OpenStep::OpenWorld
        ))
    );
    let first_failure = format!("{:?}", stopped.failure());
    let mut terminal = loaded(&fixture);
    discard(&mut terminal);
    for session in [&mut before_load, &mut stopped, &mut terminal] {
        let before = session.owned_counts().unwrap();
        let precision = session.precision_state();
        let mut effects = Effects::default();
        let mut game = EmptyGamePrograms::new(None);
        assert!(matches!(
            session.process_program_in_world(
                0,
                7,
                &mut DispatchBudget::new(8).unwrap(),
                &mut effects,
                &mut Diagnostic,
                locales()
            ),
            Err(MissionGlobalProgramError::SourceUnavailable(_))
        ));
        assert!(matches!(
            named(session, b"main", &mut game, &mut effects),
            Err(NamedGlobalProgramError::SourceUnavailable(_))
        ));
        assert_eq!(effects.calls, 0);
        assert_eq!(game.calls, 0);
        assert_eq!(session.owned_counts().unwrap(), before);
        assert_eq!(session.precision_state(), precision);
    }
    assert_eq!(format!("{:?}", stopped.failure()), first_failure);
    assert!(before_load.scene().unwrap().is_none());
    before_load.discard_terminal().unwrap();
    assert_eq!(
        before_load.owned_counts().unwrap(),
        MissionOwnedCounts::default()
    );
    discard(&mut stopped);
}

#[test]
fn terminal_disposal_refuses_foreign_or_stale_selection_without_releasing_local_scene() {
    let bytes = source(b"dim_frm 1");
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"alpha")])), Some(&bytes), None);
    let mut foreign = loaded(&fixture);
    for use_foreign in [false, true] {
        let mut session = loaded(&fixture);
        let scene = session.scene().unwrap().unwrap();
        let selected = if use_foreign {
            let reference = foreign.scene_reference().unwrap();
            assert_eq!(reference.identity(), scene.root().0);
            reference
        } else {
            let model = session.arena.create_model().unwrap();
            let reference = session.arena.lookup_reference(model.root());
            session.arena.close(model).unwrap();
            session.arena.release(model.root()).unwrap();
            reference
        };
        let foreign_counts = foreign.owned_counts().unwrap();
        assert_eq!(
            named(
                &mut session,
                b"absent",
                &mut EmptyGamePrograms::new(Some(selected)),
                &mut Effects::default()
            ),
            Ok(NamedGlobalProgramOutcome::NoMatch)
        );
        assert!(session.scene().is_err());
        assert_eq!(
            session.discard_terminal(),
            Err(MissionDiscardCallError::Stopped)
        );
        assert_eq!(session.discard_progress().phase, MissionDiscardPhase::Scene);
        assert!(!session.discard_progress().scene_release_executed);
        assert!(matches!(
            session.discard_failure().unwrap().cause,
            MissionDiscardCause::SceneOwner(_)
        ));
        assert_eq!(session.scene_reference(), Some(selected));
        assert_eq!(session.driver().scenes(), &[scene]);
        assert!(session.arena().scene(scene).is_ok());
        // Earlier source disposal is retained; the refused Scene release is
        // never replaced with a cached original key or automatically retried.
        let retained = session.owned_counts().unwrap();
        assert_eq!(retained.mission_actors, 0);
        assert_eq!(retained.mission_program_sources, 0);
        assert_eq!(retained.driver_scenes, 1);
        assert_eq!(
            session.discard_terminal(),
            Err(MissionDiscardCallError::AlreadyEntered)
        );
        assert_eq!(session.owned_counts().unwrap(), retained);
        assert_eq!(foreign.owned_counts().unwrap(), foreign_counts);
    }
    discard(&mut foreign);
}
