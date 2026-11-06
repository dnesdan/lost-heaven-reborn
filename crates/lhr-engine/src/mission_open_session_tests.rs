//! Production `Open` over live session owners with fake installation backends.
//! These vectors pin native order, error prefixes and zero-disposal; they do
//! not claim gameplay, GameInit, scheduling or completion.
use super::*;
use crate::mission_load_session::{MissionLoadSession, MissionOwnedCounts};
use crate::mission_session_navigation::SessionNavigationOutcome;
use crate::mission_session_roads::SessionRoadsOutcome;
use crate::native_precision::Pc24PrecisionState;

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-mission-open-{}-{}",
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

    fn installation(&self) -> lhr_assets::Installation {
        lhr_assets::Installation::open(&self.0).unwrap()
    }

    fn session(&self) -> MissionLoadSession {
        MissionLoadSession::new(self.installation(), Pc24PrecisionState::new(false)).unwrap()
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

fn cache_bytes(models: &[&str]) -> Vec<u8> {
    fn text(value: &str) -> Vec<u8> {
        [
            (value.len() as u32).to_le_bytes().as_slice(),
            value.as_bytes(),
        ]
        .concat()
    }
    let mut frames = Vec::new();
    for model in models {
        let mut body = text(model);
        for value in [1f32, 2., 3., 1., 0., 0., 0., 1., 2., 1.] {
            body.extend(value.to_le_bytes());
        }
        body.extend(7u32.to_le_bytes());
        for value in [2f32, 3., 4.] {
            body.extend(value.to_le_bytes());
        }
        frames.extend(chunk(2000, &body));
    }
    let mut part = text("district");
    part.extend([0; 19 * 4]);
    part.extend(frames);
    let mut root = 1u32.to_le_bytes().to_vec();
    root.extend(chunk(1000, &part));
    chunk(500, &root)
}

fn open_mise01(
    session: &mut MissionLoadSession,
) -> Result<OpenResult, OpenRunError<SessionOpenError>> {
    session.set_open_mission_name("mise01").unwrap();
    session.set_open_configured_volume(0.75).unwrap();
    OpenAttempt::default().run(session)
}

#[test]
fn mise01_open_returns_opened_with_completed_effects() {
    // Driver scope only: ordered OpenBin probe + staged cache intake with
    // conditional absent branches (nav/roads/audio skipped like the native
    // conditional ROpen arms). scene.4ds intake, weather reset and sector
    // fill belong to the probe phase (mission_load_probe), not Open order;
    // the empty scene.4ds here is not read by any Open step.
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write(
        "missions/mise01/cache.bin",
        &cache_bytes(&["car.i3d", "0rek23.i3d"]),
    );
    let mut session = fixture.session();
    let mut attempt = OpenAttempt::default();
    session.set_open_mission_name("mise01").unwrap();
    session.set_open_configured_volume(0.75).unwrap();
    assert_eq!(attempt.run(&mut session), Ok(OpenResult::Opened));
    assert!(attempt.entered());
    assert_eq!(attempt.last(), Some(OpenStep::FinishProgress));
    assert!(session.open_mission_opened());
    assert_eq!(session.open_cache_progress(), 2);
    assert!(session.open_restore_volume());
    assert_eq!(session.open_mission_name(), Some(b"mise01".as_slice()));
    assert_eq!(session.open_configured_volume(), 0.75);
    assert!(session.base_cache().is_open());
    assert_eq!(session.base_cache().blocks().len(), 1);
    assert_eq!(session.base_cache().model_sets().len(), 2);
    assert_eq!(
        session.base_cache().model_sets()[0].name.as_slice(),
        b"car.i3d".as_slice()
    );
    assert_eq!(
        session.base_cache().model_sets()[1].name.as_slice(),
        b"0rek17.i3d".as_slice()
    );
    assert!(matches!(
        session.navigation().outcome(),
        SessionNavigationOutcome::Absent { .. }
    ));
    assert!(matches!(
        session.roads().outcome(),
        SessionRoadsOutcome::Absent { .. }
    ));
    assert!(!session.audio_present_for_open());
    assert!(!session.open_muted());
    assert_eq!(session.open_restored(), None);
    assert_eq!(attempt.run(&mut session), Err(OpenRunError::AlreadyEntered));
}

#[test]
fn missing_scene2_returns_open_bin_failed_with_effects_preserved() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/cache.bin", &cache_bytes(&["car.i3d"]));
    let mut session = fixture.session();
    session.set_open_mission_name("mise01").unwrap();
    session.set_open_configured_volume(0.5).unwrap();
    let mut attempt = OpenAttempt::default();
    assert_eq!(attempt.run(&mut session), Ok(OpenResult::OpenBinFailed));
    assert_eq!(attempt.last(), Some(OpenStep::AdvanceCacheProgress));
    assert_eq!(session.open_cache_progress(), 2);
    assert!(session.open_restore_volume());
    assert!(!session.open_mission_opened());
    assert!(!session.base_cache().is_open());
    assert!(session.base_cache().blocks().is_empty());
}

#[test]
fn missing_mission_name_reports_copy_prefix_and_preserves_prefix() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    let mut session = fixture.session();
    let mut attempt = OpenAttempt::default();
    let error = attempt.run(&mut session).unwrap_err();
    match &error {
        OpenRunError::Callback(failure) => {
            assert_eq!(failure.step, OpenStep::CopyMissionName);
            assert!(
                failure.error.to_string().starts_with("CopyMissionName:"),
                "{}",
                failure.error
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(attempt.last(), Some(OpenStep::CopyMissionName));
    assert_eq!(session.open_cache_progress(), 0);
    assert!(!session.open_mission_opened());
    assert!(!session.open_restore_volume());
}

#[test]
fn invalid_mission_name_reports_copy_prefix_before_any_file_effect() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let error = session.set_open_mission_name("").unwrap_err();
    assert!(error.contains("mission name"));
    let mut attempt = OpenAttempt::default();
    let error = attempt.run(&mut session).unwrap_err();
    match &error {
        OpenRunError::Callback(failure) => {
            assert_eq!(failure.step, OpenStep::CopyMissionName);
            assert!(
                failure.error.to_string().starts_with("CopyMissionName:"),
                "{}",
                failure.error
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn spaced_mission_name_passes_validation_like_native_strcpy() {
    // Native strcpy accepts anything; spaces and dots pass validation.
    let fixture = Fixture::new();
    let mut session = fixture.session();
    session.set_open_mission_name("my mission.4ds").unwrap();
    assert_eq!(
        session.open_mission_name(),
        Some(b"my mission.4ds".as_slice())
    );
}

#[test]
fn close_disposes_to_zero_and_restart_repeats() {
    let fixture = Fixture::new();
    fixture.write("missions/mise01/scene.4ds", &[]);
    fixture.write("missions/mise01/scene2.bin", &scene2(&[]));
    fixture.write("missions/mise01/cache.bin", &cache_bytes(&["car.i3d"]));
    let mut session = fixture.session();
    assert_eq!(open_mise01(&mut session), Ok(OpenResult::Opened));
    assert!(session.base_cache().is_open());
    session.close_previous_for_open().unwrap();
    assert!(!session.base_cache().is_open());
    assert!(session.base_cache().blocks().is_empty());
    assert!(session.base_cache().model_sets().is_empty());
    assert_eq!(session.navigation().live_allocations(), 0);
    assert_eq!(session.roads().node_count(), 0);
    session.discard_terminal().unwrap();
    assert_eq!(
        session.owned_counts().unwrap(),
        MissionOwnedCounts::default()
    );
    // Post-discard mutators stay rejected with no mutation: both flags were
    // set during the open above and stay unchanged by the refused marks.
    assert!(session.mark_open_restore_volume().is_err());
    assert!(session.advance_open_cache_progress().is_err());
    assert!(session.mark_open_mission_opened().is_err());
    assert!(session.open_restore_volume());
    assert!(session.open_mission_opened());
    let mut fresh = fixture.session();
    assert_eq!(open_mise01(&mut fresh), Ok(OpenResult::Opened));
    assert!(fresh.base_cache().is_open());
    fresh.close_previous_for_open().unwrap();
    fresh.discard_terminal().unwrap();
    assert_eq!(fresh.owned_counts().unwrap(), MissionOwnedCounts::default());
}
