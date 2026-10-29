//! Synthetic tests against the real arena, light generations and load session.
use super::*;
use crate::frame_transform::I3dFrameState;
use crate::mission_load_session::{
    MissionDiscardPhase, MissionLoadCallError, MissionLoadPhase, MissionLoadRequest,
    MissionLoadSession, MissionOwnedCounts,
};
use crate::native_precision::Pc24PrecisionState;

fn dummy(arena: &mut ModelArena) -> FrameKey {
    arena
        .insert(I3dFrameState::dummy(), None, Vec::new(), Vec::new(), None)
        .unwrap()
}
fn bits(arena: &ModelArena, sector: SectorKey) -> u32 {
    arena.sector(sector).unwrap().geometry.group_bits
}
fn changes(arena: &ModelArena, sector: SectorKey) -> u32 {
    arena.sector_lights_changed(sector).unwrap()
}
fn seed_bits(arena: &mut ModelArena, sector: SectorKey, value: u32) {
    let index = arena.sector_index(sector).unwrap();
    arena.sectors[index].geometry.group_bits = value;
}

#[test]
fn setting_the_same_mask_still_invalidates_the_existing_lighting_generation() {
    let mut arena = ModelArena::default();
    let sector = arena.create_sector_frame().unwrap();
    let state = format!("{:?}", arena.state(sector.frame()).unwrap());
    let references = arena.frame(sector.frame()).unwrap().references;
    let version = arena.global_sector_version();
    arena.set_sector_group_bits(sector, 0x1234_5678).unwrap();
    assert_eq!(bits(&arena, sector), 0x1234_5678);
    assert_eq!(changes(&arena, sector), 1);
    arena.set_sector_group_bits(sector, 0x1234_5678).unwrap();
    assert_eq!(changes(&arena, sector), 2);
    assert_eq!(format!("{:?}", arena.state(sector.frame()).unwrap()), state);
    assert_eq!(arena.frame(sector.frame()).unwrap().references, references);
    assert_eq!(arena.global_sector_version(), version);
    assert_eq!(arena.sector_storage().live_allocations(), 0);
    arena.release(sector.frame()).unwrap();
    assert_eq!(arena.live_frame_count(), 0);
}

#[test]
fn lighting_generation_wraps_without_saturating_or_resetting_other_sectors() {
    let mut arena = ModelArena::default();
    let a = arena.create_sector_frame().unwrap();
    let b = arena.create_sector_frame().unwrap();
    arena
        .graph
        .sector_light_changes
        .insert(a.frame().0, u32::MAX);
    arena.graph.sector_light_changes.insert(b.frame().0, 17);
    arena.set_sector_group_bits(a, 0).unwrap();
    assert_eq!(changes(&arena, a), 0);
    assert_eq!(changes(&arena, b), 17);
    arena.release(a.frame()).unwrap();
    arena.release(b.frame()).unwrap();
}

#[test]
fn non_sector_and_retired_keys_cannot_create_lighting_entries() {
    let mut arena = ModelArena::default();
    let frame = dummy(&mut arena);
    assert!(arena.set_sector_group_bits(SectorKey(frame), 7).is_err());
    assert!(!arena.graph.sector_light_changes.contains_key(&frame.0));
    arena.release(frame).unwrap();
    assert!(arena.set_sector_group_bits(SectorKey(frame), 7).is_err());
    assert!(!arena.graph.sector_light_changes.contains_key(&frame.0));
}

#[test]
fn only_the_primary_hierarchy_is_filled_including_sectors_below_non_sectors() {
    Pc24PrecisionState::default().run(|| {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let branch = dummy(&mut arena);
        let nested = arena.create_sector_frame().unwrap();
        let detached = arena.create_sector_frame().unwrap();
        for sector in sectors.into_iter().chain([nested, detached]) {
            seed_bits(&mut arena, sector, 0x1357);
        }
        arena
            .link_frame(branch, Some(sectors[0].frame()), 0)
            .unwrap();
        arena.link_frame(nested.frame(), Some(branch), 0).unwrap();
        let count = arena.live_frame_count();
        let membership = arena.scene(scene).unwrap().frames().to_vec();
        let links = arena.links(branch).unwrap();
        fill_primary_sector_groups(&mut arena, scene).unwrap();
        for sector in [sectors[0], nested] {
            assert_eq!(bits(&arena, sector), u32::MAX);
            assert_eq!(changes(&arena, sector), 1);
        }
        for sector in [sectors[1], sectors[2], sectors[3], detached] {
            assert_eq!(bits(&arena, sector), 0x1357);
            assert_eq!(changes(&arena, sector), 0);
        }
        assert_eq!(arena.live_frame_count(), count);
        assert_eq!(arena.scene(scene).unwrap().frames(), membership.as_slice());
        assert_eq!(arena.links(branch).unwrap(), links);
        for frame in [nested.frame(), branch, detached.frame()] {
            arena.release(frame).unwrap();
        }
        for sector in sectors {
            arena.release(sector.frame()).unwrap();
        }
        arena.destroy_empty_scene_base(scene.root()).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    });
}

#[test]
fn reverse_children_preserve_completed_subtrees_before_a_reached_owner_error() {
    Pc24PrecisionState::default().run(|| {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        for _ in 0..2 {
            let sector = arena.create_sector_frame().unwrap();
            arena
                .link_frame(sector.frame(), Some(primary.frame()), 0)
                .unwrap();
        }
        let first = SectorKey(child_at(&arena, primary.frame(), 0).unwrap());
        let last = SectorKey(child_at(&arena, primary.frame(), 1).unwrap());
        let leaf = arena.create_sector_frame().unwrap();
        arena
            .link_frame(leaf.frame(), Some(last.frame()), 0)
            .unwrap();
        let first_index = arena.sector_index(first).unwrap();
        arena.sectors[first_index].geometry.update_interrupted = true;
        assert_eq!(
            fill_primary_sector_groups(&mut arena, scene),
            Err(ModelError::Sector(SectorError::Interrupted))
        );
        for sector in [primary, last, leaf] {
            assert_eq!(bits(&arena, sector), u32::MAX);
            assert_eq!(changes(&arena, sector), 1);
        }
        assert_eq!(bits(&arena, first), 0);
        assert_eq!(changes(&arena, first), 0);
        // The interrupted sector is deliberately not made reusable for cleanup.
    });
}

#[test]
fn interrupted_scene_is_rejected_before_any_sector_write() {
    struct Stop;
    impl crate::scene_ownership::SceneCloseBackend for Stop {
        fn close_effect(
            &mut self,
            _: &ModelArena,
            _: SceneKey,
            effect: crate::scene_ownership::SceneCloseEffect,
        ) -> Result<(), ModelError> {
            Err(crate::scene_ownership::SceneOwnerError::BackendUnavailable(effect).into())
        }
    }
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let primary = arena.scene(scene).unwrap().sectors[0];
    assert!(arena.close_scene_with_backend(scene, &mut Stop).is_err());
    assert!(fill_primary_sector_groups(&mut arena, scene).is_err());
    assert_eq!(bits(&arena, primary), 0);
    assert_eq!(changes(&arena, primary), 0);
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(with_world: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "lhr-mission-sector-groups-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(root.join("missions/synthetic")).unwrap();
        std::fs::create_dir_all(root.join("Sounds")).unwrap();
        if with_world {
            std::fs::write(root.join("missions/synthetic/scene.4ds"), world()).unwrap();
        }
        Self(root)
    }
    fn session(&self) -> MissionLoadSession {
        MissionLoadSession::new(
            lhr_assets::Installation::open(&self.0).unwrap(),
            Pc24PrecisionState::new(false),
        )
        .unwrap()
    }
    fn scene2(&self) {
        let bytes = [
            0x4c53u16.to_le_bytes().as_slice(),
            &16u32.to_le_bytes(),
            &1u16.to_le_bytes(),
            &10u32.to_le_bytes(),
            &[5, 0, 1, 0],
        ]
        .concat();
        std::fs::write(self.0.join("missions/synthetic/scene2.bin"), bytes).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn world() -> Vec<u8> {
    let mut bytes = b"4DS\0".to_vec();
    bytes.extend(29u16.to_le_bytes());
    bytes.extend([0; 8]);
    bytes.extend(0u16.to_le_bytes()); // no materials
    bytes.extend(1u16.to_le_bytes()); // one sector
    bytes.push(5);
    bytes.extend(0u16.to_le_bytes()); // no parent
    for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(0); // culling
    bytes.push(6);
    bytes.extend(b"sector");
    bytes.push(0); // properties
    bytes.extend(0x801u32.to_le_bytes());
    bytes.extend(0x1234_5678u32.to_le_bytes()); // saved group mask, not all bits
    bytes.extend(0u32.to_le_bytes()); // vertices
    bytes.extend(0u32.to_le_bytes()); // faces
    for value in [0f32, 0., 0., 1., 1., 1.] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.push(0); // portals
    bytes.push(0); // LoadAnimCount
    bytes
}
struct Screen;
impl crate::scene_resources::SceneScreen for Screen {
    type Error = &'static str;
    fn screen_sx(&mut self) -> Result<i32, Self::Error> {
        Ok(800)
    }
    fn screen_sy(&mut self) -> Result<i32, Self::Error> {
        Ok(600)
    }
}
impl crate::driver_scene::DriverSceneScreen for Screen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error> {
        Ok([0, 0, 800, 600])
    }
}
fn load(session: &mut MissionLoadSession) -> Result<(), MissionLoadCallError> {
    session.load(
        MissionLoadRequest {
            world_input: "missions/synthetic/scene.i3d",
            scene2_path: "missions/synthetic/scene2.bin",
            model_directories: &[],
            projection: crate::game_camera_projection::GameCameraProjection {
                global_mode: 0,
                screen_flag_46f4: false,
            },
        },
        &mut Screen,
    )
}
fn discard(session: &mut MissionLoadSession) {
    assert_eq!(
        session.discard_terminal(),
        Ok(()),
        "{:?}",
        session.discard_failure()
    );
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::Complete
    );
    assert_eq!(
        session.owned_counts().unwrap(),
        MissionOwnedCounts::default()
    );
}

#[test]
fn actual_session_fills_world_sectors_before_scene2_and_preserves_normal_disposal() {
    let fixture = Fixture::new(true);
    fixture.scene2();
    let mut session = fixture.session();
    assert_eq!(load(&mut session), Ok(()), "{:?}", session.failure());
    assert_eq!(session.phase(), MissionLoadPhase::PrefixComplete);
    let scene = session.scene().unwrap();
    let sectors = session.arena().scene(scene).unwrap().sectors;
    let loaded = SectorKey(session.world_progress().unwrap().frames()[0]);
    for sector in [sectors[0], loaded] {
        assert_eq!(bits(session.arena(), sector), u32::MAX);
        assert_eq!(changes(session.arena(), sector), 1);
    }
    for sector in &sectors[1..] {
        assert_eq!(bits(session.arena(), *sector), 0);
        assert_eq!(changes(session.arena(), *sector), 0);
    }
    discard(&mut session);
}

#[test]
fn later_scene2_failure_keeps_group_changes_and_does_not_repeat_invalidation() {
    let fixture = Fixture::new(true);
    let mut session = fixture.session();
    assert_eq!(load(&mut session), Err(MissionLoadCallError::Stopped));
    assert_eq!(session.phase(), MissionLoadPhase::Scene2Input);
    let scene = session.scene().unwrap();
    let primary = session.arena().scene(scene).unwrap().sectors[0];
    let loaded = SectorKey(session.world_progress().unwrap().frames()[0]);
    assert_eq!(bits(session.arena(), loaded), u32::MAX);
    let before = [
        changes(session.arena(), primary),
        changes(session.arena(), loaded),
    ];
    assert_eq!(before, [1, 1]);
    assert_eq!(
        load(&mut session),
        Err(MissionLoadCallError::AlreadyEntered)
    );
    assert_eq!(
        [
            changes(session.arena(), primary),
            changes(session.arena(), loaded)
        ],
        before
    );
    discard(&mut session);
}

#[test]
fn failed_world_never_enters_group_initialization() {
    let fixture = Fixture::new(false);
    fixture.scene2();
    let mut session = fixture.session();
    assert_eq!(load(&mut session), Err(MissionLoadCallError::Stopped));
    assert_eq!(session.phase(), MissionLoadPhase::World);
    let primary = session
        .arena()
        .scene(session.scene().unwrap())
        .unwrap()
        .sectors[0];
    assert_eq!(bits(session.arena(), primary), 0);
    assert_eq!(changes(session.arena(), primary), 0);
    discard(&mut session);
}
