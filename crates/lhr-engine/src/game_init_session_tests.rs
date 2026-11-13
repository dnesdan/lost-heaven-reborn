//! Production `Init` prefix over live session owners with fake backends.
//! Pins native order, unavailable-dependency prefixes and the preserved
//! scalar prefix; never runs actor `GameInit`, scripts or rendering.
use super::*;
use crate::mission_load_session::MissionLoadSession;
use crate::native_precision::Pc24PrecisionState;

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-game-init-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(path.join("missions/mise01")).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn session(&self) -> MissionLoadSession {
        let installation = lhr_assets::Installation::open(&self.0).unwrap();
        MissionLoadSession::new(installation, Pc24PrecisionState::new(false)).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
    [
        kind.to_le_bytes().as_slice(),
        &u32::try_from(payload.len() + 6).unwrap().to_le_bytes(),
        payload,
    ]
    .concat()
}

fn scene2(body: &[u8]) -> Vec<u8> {
    chunk(0x4c53, &[chunk(1, &[5, 0, 1, 0]), body.to_vec()].concat())
}

fn cache_bytes() -> Vec<u8> {
    fn text(value: &str) -> Vec<u8> {
        [
            (value.len() as u32).to_le_bytes().as_slice(),
            value.as_bytes(),
        ]
        .concat()
    }
    let mut body = text("car.i3d");
    for value in [1f32, 2., 3., 1., 0., 0., 0., 1., 2., 1.] {
        body.extend(value.to_le_bytes());
    }
    body.extend(7u32.to_le_bytes());
    for value in [2f32, 3., 4.] {
        body.extend(value.to_le_bytes());
    }
    let mut part = text("district");
    part.extend([0; 19 * 4]);
    part.extend(chunk(2000, &body));
    let mut root = 1u32.to_le_bytes().to_vec();
    root.extend(chunk(1000, &part));
    chunk(500, &root)
}

fn open_mise01(session: &mut MissionLoadSession) {
    use crate::mission_open_order::{OpenAttempt, OpenResult};
    session.set_open_mission_name("mise01").unwrap();
    session.set_open_configured_volume(0.75).unwrap();
    assert_eq!(OpenAttempt::default().run(session), Ok(OpenResult::Opened));
}

#[test]
fn init_stops_at_first_unowned_step_with_live_prefix_preserved() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    // The ordered Open probe is not yet a native Scene constructor. Init must
    // stop on the missing live owner after the first two scalar stores.
    assert_eq!(session.scene().unwrap(), None);
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    let mut attempt = InitAttempt::default();
    let error = attempt.run(&mut session).unwrap_err();
    match &error {
        InitRunError::Callback(failure) => {
            assert_eq!(failure.step, InitStep::ResetMissionState);
            assert!(
                failure.error.to_string().starts_with("ResetMissionState:"),
                "{}",
                failure.error
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(attempt.last(), Some(InitStep::ResetMissionState));
    // Native stores before the first unowned Scene read are retained. Weather
    // and later sound/AI/UI work must not be silently fabricated.
    assert_eq!(session.game_init_reset_observation(), (false, 0, None, 0),);
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    assert_eq!(
        session.reset_game_mission_state_for_init(),
        Err("mission state reset already entered".into())
    );
    assert_eq!(session.game_init_reset_observation().2, None);
    // Unowned steps never ran: police/traffic stay at cleared-statics
    // defaults (17.0/28.0/valid-scream arrive only with their owners).
    assert!(!session.police_init().active());
    assert_eq!(session.police_init().speed_limit(), 0.0);
    assert_eq!(session.police_init().speed_limit_arrest(), 0.0);
    assert!(!session.traffic_flags().taxi_driver_enabled());
    assert!(!session.traffic_flags().player_in_taxi());
    assert!(!session.traffic_flags().valid_scream());
    assert!(session.base_cache().is_open());
    assert_eq!(session.read_base_blocks_for_init(), Ok(true));
    let visited = attempt.last().unwrap();
    assert_eq!(visited, InitStep::ResetMissionState);
    assert_eq!(attempt.run(&mut session), Err(InitRunError::AlreadyEntered));
}

#[test]
fn guard_without_open_prefix_reports_its_step() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let mut attempt = InitAttempt::default();
    let error = attempt.run(&mut session).unwrap_err();
    match &error {
        InitRunError::Callback(failure) => {
            assert_eq!(failure.step, InitStep::GuardNotInitialized);
            assert!(
                failure
                    .error
                    .to_string()
                    .starts_with("GuardNotInitialized:"),
                "{}",
                failure.error
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(attempt.last(), Some(InitStep::GuardNotInitialized));
}

#[test]
fn restart_repeats_prefix_and_unowned_stop() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    for _ in 0..2 {
        let mut session = fixture.session();
        open_mise01(&mut session);
        let mut attempt = InitAttempt::default();
        let error = attempt.run(&mut session).unwrap_err();
        assert!(matches!(
            error,
            InitRunError::Callback(InitFailure {
                step: InitStep::ResetMissionState,
                ..
            })
        ));
        assert!(!session.police_init().active());
        assert!(!session.traffic_flags().valid_scream());
    }
}

#[test]
fn mark_initialized_arms_the_guard_against_rerun() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes());
    let mut session = fixture.session();
    open_mise01(&mut session);
    session.mark_game_initialized_for_init().unwrap();
    // The guard now reports the already-initialized boundary.
    let err = session.guard_game_init_for_init().unwrap_err();
    assert_eq!(err, "game already initialized");
    assert_eq!(
        session.reset_game_mission_state_for_init(),
        Err("game already initialized".into())
    );
}
