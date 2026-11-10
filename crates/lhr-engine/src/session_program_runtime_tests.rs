//! Session-owned script execution uses actual game/actor/frame owners.
use super::*;
use crate::actor_world::{ActorSource, ActorWorld};
use crate::original_program::{RuntimeError, RuntimeView};
use crate::script_lookup::LookupError;
use crate::script_scheduler::ActorReferenceId;

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
        Err(LookupError::Backend("unexpected missing actor".into()))
    }
}

fn fixture(source: &[u8]) -> Fixture {
    let input = scene2(
        &[
            chunk(0xae20, &actor_record(b"alpha", 1, b"")),
            chunk(0xae50, &program_record(b"main", source)),
        ]
        .concat(),
    );
    Fixture::new(Some(&world(&[dummy(0, b"alpha")])), Some(&input), None)
}
fn run(session: &mut MissionLoadSession) -> Result<(), MissionGlobalProgramError> {
    session.process_owned_program_in_world(
        0,
        16,
        &mut DispatchBudget::new(64).unwrap(),
        &mut Diagnostic,
        ProgramWorldLocales {
            game_default_c_locale: true,
            ls3df_default_c_locale: true,
        },
    )
}
fn finish(session: &mut MissionLoadSession) {
    session.discard_global_program_runtimes_host().unwrap();
    discard(session);
}

struct GamePrograms {
    result: Result<bool, String>,
    calls: usize,
}
impl<'a> GameGlobalProgramWorldRunner<SessionProgramEffects<'a>, Diagnostic> for GamePrograms {
    fn run_game_global(
        &mut self,
        _: &mut crate::mission_content_loading::MissionContentOwner,
        _: &[u8],
        _: &mut DispatchBudget,
        _: &mut MissionProgramWorldContext<'_, SessionProgramEffects<'a>, Diagnostic>,
    ) -> Result<bool, String> {
        self.calls += 1;
        self.result.clone()
    }
}

#[test]
fn named_owned_run_keeps_required_game_first_and_mission_fallback_order() {
    let fixture = fixture(b"commandblock 1\ngame_nightmission 1\ncommandblock 0");
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    session.initialize_owned_global_programs().unwrap();
    let mut game = GamePrograms {
        result: Err("game owner unavailable".into()),
        calls: 0,
    };
    let named = |session: &mut MissionLoadSession, game: &mut GamePrograms| {
        session.run_owned_global_program_in_world(
            b"MAIN",
            game,
            &mut DispatchBudget::new(64).unwrap(),
            &mut Diagnostic,
            ProgramWorldLocales {
                game_default_c_locale: true,
                ls3df_default_c_locale: true,
            },
        )
    };
    assert!(matches!(
        named(&mut session, &mut game),
        Err(NamedGlobalProgramError::GameOwner(_))
    ));
    assert!(!session.game_state().night_mission());
    game.result = Ok(true);
    assert_eq!(
        named(&mut session, &mut game),
        Ok(NamedGlobalProgramOutcome::GameHandled)
    );
    assert!(!session.game_state().night_mission());
    game.result = Ok(false);
    assert_eq!(
        named(&mut session, &mut game),
        Ok(NamedGlobalProgramOutcome::MissionHandled { index: 0 })
    );
    assert!(session.game_state().night_mission());
    assert_eq!(game.calls, 3);
    finish(&mut session);
}

#[test]
fn owned_program_updates_live_game_flags_and_resolves_the_retained_actor() {
    let fixture = fixture(b"commandblock 1\ndim_act 1\nfindactor 0,\"alpha\"\ngame_nightmission 1\nnoanimpreload\ncommandblock 0");
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    session.game_inline.set_anim_preload(true);
    session.initialize_owned_global_programs().unwrap();
    assert!(!session.game_state().night_mission());
    assert!(session.game_state().anim_preload());
    let scene = session.scene_reference();
    run(&mut session).unwrap();
    assert!(session.game_state().night_mission());
    assert!(!session.game_state().anim_preload());
    assert_eq!(session.scene_reference(), scene);
    let content = &session.loader().unwrap().content;
    let actor = content.actors.world.source(ActorSource::Primary)[0].unwrap();
    assert_eq!(
        content.programs()[0]
            .runtime()
            .unwrap()
            .fields
            .actors
            .as_ref()
            .unwrap()[0]
            .reference(),
        Ok(Some(&actor))
    );
    assert_eq!(crate::native_precision::current_precision(), None);
    finish(&mut session);
}

#[test]
fn missing_game_owners_stop_at_the_reached_effect_and_keep_the_prefix() {
    for (unsupported, detail) in [
        ("use_lightcache", "session light-cache owner unavailable"),
        (
            "setcitytrafficvisible 1",
            "session city-traffic visibility owner unavailable",
        ),
    ] {
        let source = format!(
            "commandblock 1\ngame_nightmission 1\n{unsupported}\ngame_nightmission 0\ncommandblock 0"
        );
        let fixture = fixture(source.as_bytes());
        let mut session = fixture.session();
        load(&mut session, &[]).unwrap();
        session.initialize_owned_global_programs().unwrap();
        let error = run(&mut session).unwrap_err();
        assert_eq!(
            error,
            MissionGlobalProgramError::Runtime(RuntimeError::Scheduler(format!(
                "Backend(Effect({detail:?}))"
            ),))
        );
        assert!(session.game_state().night_mission());
        assert_eq!(
            session.loader().unwrap().content.programs()[0]
                .runtime()
                .unwrap()
                .fields
                .program_counter(),
            2
        );
        finish(&mut session);
    }
}

#[test]
fn completed_and_stopped_sessions_do_not_share_game_or_program_state() {
    let fixture = fixture(b"commandblock 1\ngame_nightmission 1\ncommandblock 0");
    let mut previous_scene = None;
    for _ in 0..3 {
        let mut session = fixture.session();
        load(&mut session, &[]).unwrap();
        assert!(!session.game_state().night_mission());
        assert_ne!(session.scene_reference(), previous_scene);
        previous_scene = session.scene_reference();
        assert!(matches!(
            run(&mut session),
            Err(MissionGlobalProgramError::Runtime(RuntimeError::Domain(
                "uninitialized program"
            )))
        ));
        session.initialize_owned_global_programs().unwrap();
        run(&mut session).unwrap();
        assert!(session.game_state().night_mission());
        finish(&mut session);
        assert!(session.initialize_owned_global_programs().is_err());
        assert!(run(&mut session).is_err());
    }
}

#[test]
fn session_programs_read_the_retained_clock_without_advancing_it() {
    let fixture = fixture(b"commandblock 1\ndim_flt 4\ngetgametime 0\ngetgametime 1\ncommandblock 0\ncommandblock 1\ngetgametime 2\ngetgametime 3\ncommandblock 0");
    let mut session = fixture.session();
    assert!(session.advance_owned_game_time(7).is_err());
    load(&mut session, &[]).unwrap();
    assert_eq!(session.advance_owned_game_time(7).unwrap(), 7);
    session.initialize_owned_global_programs().unwrap();
    run(&mut session).unwrap();
    assert_eq!(session.game_state().get_game_time(), 7);
    assert_eq!(session.advance_owned_game_time(5).unwrap(), 12);
    run(&mut session).unwrap();
    assert_eq!(session.game_state().get_game_time(), 12);
    assert_eq!(session.loader().unwrap().content.programs()[0].runtime().unwrap().fields.floats.as_deref().unwrap(), &[7.0, 7.0, 12.0, 12.0]);
    let clock = session.game_state().game_clock();
    finish(&mut session);
    assert!(session.advance_owned_game_time(1).is_err());
    drop(session);
    assert_eq!(clock.read(), None);
}
