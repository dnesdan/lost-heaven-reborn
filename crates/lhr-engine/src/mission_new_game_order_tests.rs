//! Composition order of the Main.cpp new-game mission start.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    Step(NewMissionStep),
    /// Open entered with a fresh nested attempt.
    Open,
    /// Init entered with a fresh nested attempt.
    Init,
}

struct Recorder {
    calls: Vec<Call>,
    open: Result<OpenResult, &'static str>,
    fail: Option<NewMissionStep>,
}

impl Recorder {
    fn new(open: Result<OpenResult, &'static str>) -> Self {
        Self {
            calls: Vec::new(),
            open,
            fail: None,
        }
    }
}

impl NewMissionOperations for Recorder {
    type Error = &'static str;

    fn execute(&mut self, step: NewMissionStep) -> Result<(), Self::Error> {
        assert!(!matches!(
            step,
            NewMissionStep::Open | NewMissionStep::GameInit
        ));
        self.calls.push(Call::Step(step));
        if self.fail == Some(step) {
            return Err("unavailable");
        }
        Ok(())
    }

    fn open(&mut self, attempt: &mut OpenAttempt) -> Result<OpenResult, Self::Error> {
        assert!(!attempt.entered());
        self.calls.push(Call::Open);
        self.open
    }

    fn game_init(&mut self, attempt: &mut InitAttempt) -> Result<(), Self::Error> {
        assert!(!attempt.entered());
        self.calls.push(Call::Init);
        if self.fail == Some(NewMissionStep::GameInit) {
            return Err("unavailable");
        }
        Ok(())
    }
}

fn expected_calls(steps: &[NewMissionStep]) -> Vec<Call> {
    steps
        .iter()
        .map(|step| match step {
            NewMissionStep::Open => Call::Open,
            NewMissionStep::GameInit => Call::Init,
            other => Call::Step(*other),
        })
        .collect()
}

#[test]
fn opened_mission_runs_main_new_game_order_through_init() {
    let mut ops = Recorder::new(Ok(OpenResult::Opened));
    let mut attempt = NewMissionAttempt::default();
    assert_eq!(attempt.run(&mut ops), Ok(NewMissionResult::InitReturned));
    // Main.cpp:385-396: Close, Open, LoadCollision, ResetMissionNumbers,
    // SaveGameNewGame, then game->Init.
    assert_eq!(
        NewMissionStep::ORDER,
        &[
            NewMissionStep::ClosePrevious,
            NewMissionStep::Open,
            NewMissionStep::LoadCollision,
            NewMissionStep::ResetMissionNumbers,
            NewMissionStep::SaveGameNewGame,
            NewMissionStep::GameInit,
        ]
    );
    assert_eq!(attempt.reached(), NewMissionStep::ORDER);
    assert_eq!(ops.calls, expected_calls(NewMissionStep::ORDER));
    assert_eq!(attempt.open_result(), Some(OpenResult::Opened));
    assert_eq!(attempt.last(), Some(NewMissionStep::GameInit));
    // Even a returned Init stays scoped: the intro leg was not composed.
    assert_eq!(
        attempt.scope(),
        NewMissionScope::FreshSessionIntroNotComposed
    );
}

#[test]
fn nonzero_open_leaves_before_collision_and_init() {
    for result in [OpenResult::NoMedia, OpenResult::OpenBinFailed] {
        let mut ops = Recorder::new(Ok(result));
        let mut attempt = NewMissionAttempt::default();
        assert_eq!(
            attempt.run(&mut ops),
            Ok(NewMissionResult::OpenFailed(result))
        );
        assert_eq!(
            attempt.reached(),
            &[NewMissionStep::ClosePrevious, NewMissionStep::Open]
        );
        assert_eq!(attempt.open_result(), Some(result));
        assert!(!attempt.init().entered());
    }
}

#[test]
fn each_unavailable_step_stops_at_its_exact_prefix_without_replay() {
    for (index, step) in NewMissionStep::ORDER.iter().enumerate() {
        let mut ops = Recorder::new(if *step == NewMissionStep::Open {
            Err("unavailable")
        } else {
            Ok(OpenResult::Opened)
        });
        ops.fail = Some(*step);
        let mut attempt = NewMissionAttempt::default();
        assert_eq!(
            attempt.run(&mut ops),
            Err(NewMissionRunError::Callback(NewMissionFailure {
                step: *step,
                error: "unavailable",
            }))
        );
        assert_eq!(attempt.reached(), &NewMissionStep::ORDER[..=index]);
        assert_eq!(ops.calls, expected_calls(&NewMissionStep::ORDER[..=index]));
        let calls = ops.calls.len();
        assert_eq!(
            attempt.run(&mut ops),
            Err(NewMissionRunError::AlreadyEntered)
        );
        assert_eq!(ops.calls.len(), calls);
    }
}
