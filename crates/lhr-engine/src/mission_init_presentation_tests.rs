//! Ordered initialization uses the retained scene at the reached render call.
use super::*;
use crate::game_init_order::{InitAttempt, InitRunError, InitStep, PresentedFrame};
use crate::load_scene_callback::LoadingScreenSource;
use crate::scene_render::SceneSnapshotSequence;

fn hud() -> crate::indicators::IndicatorsState {
    crate::indicators::IndicatorsState::constructed(crate::indicators::TimerClock {
        hour: 9,
        minute: 0,
        second: 0,
    })
}

/// Stage the C_game state the reached `Init` span overwrites, so every
/// clear and store is observable. The returned sound frame is the borrowed
/// identity the stopped-sound record names; the caller releases it. The
/// returned buffer id names the spare shadow-name buffer the list holds.
fn stage_dirty_game_state(
    session: &mut MissionLoadSession,
) -> (
    crate::frame_sound::SoundKey,
    crate::game_housekeeping::ShadowNameBufferId,
) {
    let sound = session.arena.create_owned_sound_frame().unwrap();
    let mut name = vec![0u8; 32];
    name[..6].copy_from_slice(b"shadow");
    let buffer = session
        .shadow_name_buffers
        .allocate(name.into_boxed_slice());
    let lists = &mut session.game_lists;
    lists
        .stop_sounds
        .push(crate::game_housekeeping::StoppedSound {
            sound,
            volume: 0.5,
            volume_out: 0.25,
            looped: true,
            time: 40,
        });
    lists.paused_i_shows.extend([3, -1]);
    lists.free_shadow_names.push(buffer);
    lists.unknown_140.extend([11, 12]);
    *session.game_inline.score_mut() = crate::game_housekeeping::ScoreState {
        score: 7,
        score_on: true,
    };
    session.game_inline.set_actor_lightness(true);
    session.game_inline.stage_camera_rot_repair(true);
    stage_dirty_ai_manager(session);
    stage_box_placements(session);
    (sound, buffer)
}

/// Stage the `AIManager` records the police shift begin and the
/// groups/follow/fight `Init` empty, with borrowed actor and frame ids.
fn stage_dirty_ai_manager(session: &mut MissionLoadSession) {
    use crate::ai_manager::*;
    let actor = crate::script_scheduler::ActorReferenceId::from_existing;
    let manager = session.ai_manager_mut();
    manager.police.stage(
        vec![AiAnimItem {
            frame: crate::frame_transform::FrameKey(5),
        }],
        vec![AiPoliceAction {
            members: vec![actor(21)],
        }],
        Some(actor(22)),
    );
    manager.groups.stage(
        vec![AiGroup {
            members: vec![AiGroupMember { entity: actor(23) }],
            ..AiGroup::default()
        }],
        100,
        4,
    );
    manager.groups.add_free_actor(actor(24));
    manager.follow.stage(
        vec![AiFollowGroup {
            followers: vec![actor(25)],
        }],
        300,
    );
    manager.fight.stage(vec![AiFightFollowship {
        members: vec![AiFightMember { entity: actor(26) }],
    }]);
}

/// The `PoliceBeginShift` stores over the staged manager (GAME 0x53ba50,
/// 0x531130, 0x532da0, 0x534ec0).
fn assert_ai_manager_shift_begun(session: &MissionLoadSession) {
    let manager = session.ai_manager();
    assert!(manager.police.items().is_empty());
    assert!(manager.police.actions().is_empty());
    assert_eq!(manager.police.next_id(), 0);
    assert_eq!(manager.police.unknown_24(), 0x0fff_ffff);
    assert_eq!(manager.police.on_duty(), 0);
    assert_eq!(manager.police.speed_factor().to_bits(), 0x3f80_0000);
    assert_eq!(manager.police.customer(), None);
    assert_eq!(manager.groups.unlock_time(), u32::MAX);
    assert_eq!(manager.groups.last_id(), 0);
    assert!(manager.groups.groups().is_empty());
    assert!(manager.groups.free_actors().is_empty());
    assert_eq!(manager.follow.check_time(), 0);
    assert!(manager.follow.groups().is_empty());
    assert!(manager.fight.parties().is_empty());
}

/// `EffectsStart`'s boundary after the box-placement clears,
/// `LoadStaticParticles`, `Mission_Start` and `DeRiciConns`: the force
/// feedback manager has no adopted process input.
const EFFECTS_STOP: &str = "EffectsStart: ff_manager.GameInit (GAME 0x4446b0, called at 0x5a0d63): process input has not been adopted from profile startup";

/// Stage the three box-placement vectors `EffectsStart` empties, with
/// borrowed identities whose owners (`BoxPlacementAdd`,
/// `BoxPlacementDestAdd`, `BoxPlacement_SaveInfo`) are unported.
fn stage_box_placements(session: &mut MissionLoadSession) {
    use crate::game_housekeeping::*;
    let frame = crate::script_scheduler::FrameReferenceId::from_existing;
    let lists = &mut session.game_lists;
    lists.boxes.push(BoxPlacementRef::staged(71));
    lists
        .box_dests
        .extend([BoxDestRef::staged(72), BoxDestRef::staged(73)]);
    lists.box_placement_temp.push(BoxPlacementTemp::staged(
        frame(74),
        None,
        [1.0, 2.0, 3.0],
        [0.0, 0.0, 1.0],
    ));
}

fn box_placement_counts(session: &MissionLoadSession) -> [usize; 3] {
    let lists = session.game_lists();
    [
        lists.boxes.len(),
        lists.box_dests.len(),
        lists.box_placement_temp.len(),
    ]
}

/// `IndicatorsGameInit` without the profile intake's options.
const SETTINGS_STOP: &str = "IndicatorsGameInit: mafiaIndicators.GameInit (GAME 0x5f73c0): mafiaLoadSave.settings.speedometer_mph (GAME 0x6d4b44) was never set by G_LoadSaveClass::Init's profile intake (AllDefault 0x6051e0 / ProfileLoad 0x6057f0)";

/// The options `G_LoadSaveClass::Init` leaves on a fresh installation (no
/// profile file: `AllDefault`, speedometer in miles).
pub(super) fn adopt_fresh_install_settings(session: &mut MissionLoadSession) {
    use crate::game_loadsave_lifecycle::*;
    let settings = profile_settings_after_load_save_init(ProfileFileIntake::NoProfileFile).unwrap();
    session.adopt_profile_settings(settings).unwrap();
}

/// The HUD textures `GameInit` selected (dial, icons).
fn hud_textures(session: &MissionLoadSession) -> (Option<&'static str>, Option<&'static str>) {
    let hud = session.indicators().unwrap();
    (hud.dial_asset(), hud.icons_asset())
}

/// A world with two mission-global programs: `GameInitStart` sets the
/// night-mission flag when its subroutine runs, `other` never runs.
fn programs_fixture() -> Fixture {
    let input = scene2(
        &[
            chunk(
                0xae50,
                &program_record(
                    b"GameInitStart",
                    b"commandblock 1\ngame_nightmission 1\ncommandblock 0",
                ),
            ),
            chunk(
                0xae50,
                &program_record(b"other", b"commandblock 1\nnoanimpreload\ncommandblock 0"),
            ),
        ]
        .concat(),
    );
    Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&input), None)
}

fn programs_session(fixture: &Fixture) -> MissionLoadSession {
    let mut session = programs_session_without_settings(fixture);
    adopt_fresh_install_settings(&mut session);
    session
}

fn programs_session_without_settings(fixture: &Fixture) -> MissionLoadSession {
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    load(&mut session, &[]).unwrap();
    session
}

fn program_runtimes(session: &MissionLoadSession) -> Vec<bool> {
    session
        .loader()
        .unwrap()
        .content
        .programs()
        .iter()
        .map(|program| program.runtime().is_some())
        .collect()
}

/// A sector key no scene frame has: an entry an earlier fill left behind.
const STALE_SECTOR: crate::frame_transform::FrameKey =
    crate::frame_transform::FrameKey(u32::MAX - 1);

/// Stage everything the `VehicleSectors`, `PhysicsClear` and
/// `SoundAndLightness` spans overwrite, so each store and clear is
/// observable.
fn stage_dirty_init_clears(session: &mut MissionLoadSession) {
    use crate::game_housekeeping::*;
    session
        .vehicle_sectors
        .add_sector_for_projector_lights(STALE_SECTOR);
    session.vehicle_sectors.add_sector_for_sounds(STALE_SECTOR);
    session.public_physics_unknown_63d78c.extend([1, 2]);
    session
        .posscoll_frames
        .push(crate::actor_registry::PossCollEntry {
            frame: crate::frame_transform::FrameKey(9),
            radius: 2.0,
        });
    let mut timer = SoundPlayTimer::default();
    timer.tick(150);
    let game = &mut session.game_inline;
    game.stage_physics_clear_state(
        CheatKeysState {
            keys: [b'X'; 32],
            cheats: 0x2f0,
        },
        MissionEndFlags {
            game_done: true,
            mission_end: 2,
            growing_projectors: 3,
        },
        (4, 5, 6, timer),
    );
    game.advance_tick_counters(40);
    game.set_night_mission(true);
    game.set_timeout_event(11, 12);
    game.set_player_fire_event(13, 14);
    game.set_player_horn_event(15, 16);
    game.set_player_wfall_event(17, 18);
    game.set_race6_state(true);
    game.light_cache = Some(crate::mission_effects::SessionLightCache {
        header: crate::mission_effects::LightCacheHeader {
            begin_token: 0x100,
            end_token: 0x108,
            field_178: 5,
        },
        lights: vec![
            crate::mission_effects::LightFrameId(3),
            crate::mission_effects::LightFrameId(4),
        ],
    });
    game.light_cache_time = 5;
    game.ishows.insert(7);
    game.set_anim_preload(false);
    game.stage_lightness_next(3);
    let lists = &mut session.game_lists;
    lists.death_actors.extend([21, 22]);
    lists
        .i_show_fades
        .push(crate::ishow_renderer::CGameIshowFade {
            id: 7,
            from: 0.0,
            to: 1.0,
            time: 10,
            length: 100,
        });
    lists.enemy_talks.push(EnemyTalk {
        human: 31,
        chanel: 2,
    });
    lists.find_actors.push(FindActor {
        program: 41,
        reg: 1,
        id: 42,
    });
    lists.film_music_streams.extend([5, 6]);
    add_npc_kill_event(&mut lists.npc_kill_events, 51, 52, "fred");
    lists.lightness.push(LightnessEntry { marker: 1 });
    lists.invisible_actors.push(InvisibleActor {
        actor: 61,
        frame: 62,
    });
    session.sound_arena.stage(0xaa, 1, 2);
    session.indicators.as_mut().unwrap().stage_radar_car(5, 1);
}

/// GAME 0x5a09d6-0x5a0a12 (`VehicleSectors`): both statics erased, the
/// primary sector registered by the walk.
fn assert_vehicle_sectors_rebuilt(session: &MissionLoadSession) {
    let scene = session.scene().unwrap().unwrap();
    let primary = session.arena.scene_primary_sector(scene).unwrap().frame();
    let sectors = session.vehicle_sectors();
    assert_eq!(sectors.light_sectors().first(), Some(&primary));
    assert!(!sectors.light_sectors().contains(&STALE_SECTOR));
    assert!(sectors.sound_sectors().is_empty());
}

/// GAME 0x5a0a17-0x5a0c2c (`PhysicsClear`).
fn assert_physics_clear_applied(session: &MissionLoadSession) {
    use crate::game_housekeeping::*;
    let (physics, posscoll) = session.physics_clear_statics();
    assert!(physics.is_empty());
    assert!(posscoll.is_empty());
    let game = session.game_state();
    // strcpy(cheat_keys, "") stores the terminator only.
    let cheat = game.cheat_input();
    assert_eq!(cheat.keys[0], 0);
    assert_eq!(&cheat.keys[1..], &[b'X'; 31]);
    assert_eq!(cheat.cheats, 0);
    assert_eq!(game.mission_end_flags(), MissionEndFlags::default());
    assert_eq!(game.get_game_time(), 0);
    assert_eq!(game.game_frame_count(), 0);
    assert_eq!(game.get_init_frames(), 0);
    assert!(!game.night_mission());
    // The program pointers clear; their event words stay.
    assert_eq!(game.timeout_event(), (0, 12));
    assert_eq!(game.player_fire_event(), (0, 14));
    assert_eq!(game.player_horn_event(), (0, 16));
    assert_eq!(game.player_wfall_event(), (0, 18));
    assert_eq!(game.get_race6_state(), RACE6_STATE_NONE);
    let cache = game.light_cache.as_ref().unwrap();
    assert!(cache.lights.is_empty());
    assert_eq!(cache.header.end_token, cache.header.begin_token);
    assert_eq!(cache.header.field_178, u32::MAX);
    assert_eq!(game.light_cache_time, -1);
    assert!(game.ishows.is_empty());
    let (next_i_show, next_sound, fading, timer) = game.sound_counters();
    assert_eq!((next_i_show, next_sound, fading), (0, 0, 0));
    assert_eq!((timer.time_ms(), timer.period_ms()), (0, 813));
    assert!(game.anim_preload());
    let lists = session.game_lists();
    assert!(lists.death_actors.is_empty());
    assert!(lists.i_show_fades.is_empty());
    assert!(lists.enemy_talks.is_empty());
    assert!(lists.find_actors.is_empty());
    assert!(lists.film_music_streams.is_empty());
    assert!(lists.npc_kill_events.is_empty());
}

/// GAME 0x5a0c4a-0x5a0c93 (`SoundAndLightness`) before `RadarClear`.
fn assert_sound_arena_and_lightness_cleared(session: &MissionLoadSession) {
    assert!(session.sound_arena().is_cleared());
    assert_eq!(session.game_state().lightness_next(), 0);
    assert!(session.game_lists().lightness.is_empty());
    assert!(session.game_lists().invisible_actors.is_empty());
}

/// The list clear drops the id only: the buffer stays live and intact.
fn assert_shadow_buffer_kept(
    session: &MissionLoadSession,
    buffer: crate::game_housekeeping::ShadowNameBufferId,
) {
    let buffers = session.shadow_name_buffers();
    assert_eq!(buffers.live_count(), 1);
    let bytes = buffers.get(buffer).unwrap();
    assert_eq!(bytes.len(), 32);
    assert_eq!(&bytes[..6], b"shadow");
}

#[test]
fn ordered_init_presents_after_mute_and_stops_before_later_owners() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let menu = fixture.0.join("tables/menu");
    std::fs::create_dir_all(&menu).unwrap();
    std::fs::write(
        menu.join("loadprogress.mnu"),
        super::loadprogress_menu_bytes(),
    )
    .unwrap();
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    adopt_fresh_install_settings(&mut session);
    // Main.cpp's mission starts open with progress: the loading screen.
    load_with_progress(&mut session, &[], &mut Screen::default(), true).unwrap();
    use crate::script_scheduler::FrameReferenceId;
    session
        .player_statics
        .set_player_navi_point(Some(FrameReferenceId::from_existing(3)));
    let statics = session.player_statics();
    let scene = session.scene().unwrap().unwrap();
    let camera = session.arena().scene_active_camera(scene).unwrap();
    let counts = session.owned_counts().unwrap();
    let audio = session.audio().clone();
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let mut presentations = 0;
    let mut loading_bars = Vec::new();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |frame| {
        match frame {
            PresentedFrame::Scene(snapshot) => {
                assert_eq!(audio.master_gain().unwrap(), 0.0);
                assert_eq!(snapshot.frame.viewport, [0, 0, 64, 64]);
                presentations += 1;
            }
            PresentedFrame::LoadingScreen(frame) => {
                assert_eq!(frame.source, LoadingScreenSource::MafiaLoadSave);
                loading_bars.push(frame.raw_progress()?);
            }
        }
        Ok(())
    });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the next unavailable owner must stop Init");
    };
    // The whole ResetMissionState span and the AIManager police shift begin
    // complete; `_show_progress_(0.0)` (GAME 0x5a3960 called at 0x5a09ba)
    // writes 0.7 to Open's created loading-screen bar and presents it once
    // through the host; SetPlayerNaviPoint(NULL) and can_car_damage follow.
    // Open's ResetIntern set intern_load, so VehicleSectors rebuilds the
    // light sectors; PhysicsClear, police and traffic init and the
    // sound-arena/lightness/radar clears follow, the (empty) global
    // programs are initialized and GameInitStart finds no program; the HUD
    // GameInit picks the day dial in miles; EffectsStart empties the
    // box-placement vectors and the unowned C_effects stops Init at
    // LoadStaticParticles.
    assert_eq!(failure.step, InitStep::EffectsStart);
    assert_eq!(failure.error.0, EFFECTS_STOP);
    assert_eq!(
        hud_textures(&session),
        (Some("2intmph.tga"), Some("2intauto.tga"))
    );
    assert_eq!(
        session.game_init_programs().game_init_start,
        Some(Ok(
            crate::mission_content_loading::NamedGlobalProgramOutcome::NoMatch
        ))
    );
    assert_vehicle_sectors_rebuilt(&session);
    assert_eq!(loading_bars, [0.7]);
    assert_eq!(
        session
            .load_save_progress()
            .progress
            .as_ref()
            .unwrap()
            .raw_progress()
            .unwrap(),
        0.7
    );
    assert_eq!(session.player_statics(), Default::default());
    assert_ne!(statics, Default::default());
    assert_eq!(session.ai_manager().groups.unlock_time(), u32::MAX);
    assert_eq!(presentations, 1);
    assert!(session.open_restore_volume());
    assert_eq!(session.scene().unwrap(), Some(scene));
    assert_eq!(session.arena().scene_active_camera(scene).unwrap(), camera);
    assert_eq!(session.owned_counts().unwrap(), counts);
    // TrafficFlags ran: the scream validity is set.
    assert!(session.traffic_flags().valid_scream());
    assert_eq!(sequence.next_number(), Ok(2));
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("Init must not replay presentation");
        }),
        Err(InitRunError::AlreadyEntered)
    );
    discard(&mut session);
}

#[test]
fn managed_loading_init_dispatch_stops_at_player_navi_without_replay() {
    use crate::menu_shared_resources::MenuSharedTexture;
    use crate::menu_text_drawing::MenuTextPrecision;
    use crate::script_scheduler::FrameReferenceId;

    #[derive(Debug, PartialEq)]
    enum Effect {
        Scene,
        Loading(LoadingScreenSource, f32),
    }

    let fixture = english_managed_loading_fixture();
    let mut session = fixture.session();
    adopt_fresh_install_settings(&mut session);
    session.select_loading_menu_startup_language().unwrap();
    session
        .configure_loading_menu_layout(fixture_managed_menu_layout())
        .unwrap();
    load_with_progress(&mut session, &[], &mut Screen::default(), true).unwrap();
    consume_open_restore(&mut session);
    session
        .player_statics
        .set_player_navi_point(Some(FrameReferenceId::from_existing(3)));
    session.player_statics.set_can_car_damage(true);
    let statics = session.player_statics();
    session.vehicle_sectors.add_sector_for_sounds(STALE_SECTOR);
    let sectors = session.vehicle_sectors().clone();
    let audio = session.audio().clone();
    let progress_key = session
        .menu_shared_resources
        .texture(MenuSharedTexture::Progress)
        .unwrap()
        .unwrap();
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let mut effects = Vec::new();
    let result =
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |frame| match frame {
            PresentedFrame::Scene(snapshot) => {
                assert_eq!(audio.master_gain().unwrap(), 0.0);
                assert_eq!(snapshot.frame.viewport, [0, 0, 64, 64]);
                effects.push(Effect::Scene);
                Ok(())
            }
            PresentedFrame::LoadingScreen(frame) => {
                effects.push(Effect::Loading(frame.source, frame.raw_progress().unwrap()));
                assert_eq!(frame.progress_texture_for_test(), Ok(Some(progress_key)));
                let draw = frame
                    .draw_components(u32::MAX, MenuTextPrecision::Unestablished)
                    .unwrap();
                draw.result.unwrap();
                assert_eq!(draw.scene.meshes.len(), 2);
                for mesh in &draw.scene.meshes {
                    assert_eq!(mesh.texture.as_ref().unwrap().rgba, [1, 2, 3, 255]);
                }
                Err("injected loading presentation failure".into())
            }
        });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("loading presentation failure must propagate");
    };
    assert_eq!(failure.step, InitStep::PlayerNavi);
    assert_eq!(
        failure.error.0,
        "PlayerNavi: Init _show_progress_(0.0): injected loading presentation failure"
    );
    assert_eq!(attempt.last(), Some(InitStep::PlayerNavi));
    assert_eq!(
        effects,
        [
            Effect::Scene,
            Effect::Loading(LoadingScreenSource::MafiaLoadSave, 0.7)
        ]
    );
    let screen = session.load_save_progress().progress.as_ref().unwrap();
    assert_eq!(screen.raw_progress(), Ok(0.7));
    assert_eq!(session.player_statics(), statics);
    assert_eq!(session.vehicle_sectors(), &sectors);
    assert!(!session.traffic_flags().valid_scream());
    assert!(session.open_restore_volume());
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| panic!(
            "replayed Init"
        )),
        Err(InitRunError::AlreadyEntered)
    );
    let restart =
        InitAttempt::default().run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("replayed session")
        });
    assert!(
        matches!(restart, Err(InitRunError::Callback(failure)) if failure.step == InitStep::ResetMissionState && failure.error.0.contains("mission state reset already entered"))
    );
    assert_eq!(sequence.next_number(), Ok(2));
    discard(&mut session);
}

#[test]
fn presented_init_render_unlocks_every_buffer_before_presentation() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    adopt_fresh_install_settings(&mut session);
    load(&mut session, &[]).unwrap();
    let visual = super::install_scene_visual(&mut session);
    // LoadCollision's LOD0 lock, kept like the native pointer.
    let view = session
        .arena
        .lock_visual_lod0_vertices(visual)
        .unwrap()
        .unwrap();
    let locked = |session: &MissionLoadSession| {
        let managers = session
            .arena()
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .managed()
            .managers();
        (
            managers.vertex_locked().to_vec(),
            managers.index_locked().to_vec(),
        )
    };
    let (held, _) = locked(&session);
    assert_eq!(held.len(), 1);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let mut presentations = 0;
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
        presentations += 1;
        Ok(())
    });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the next unavailable owner must stop Init");
    };
    // Opened without progress: no loading screen, so Init passes PlayerNavi
    // and runs on past the HUD GameInit.
    assert_eq!(failure.step, InitStep::EffectsStart);
    assert_eq!(presentations, 1);
    // I3D_driver::Render (LS3DF 0x10062030) unlocks the vertex manager
    // (0x100697e0) and then the index manager before it draws: no buffer
    // stays locked and the lock lists are empty.
    assert_eq!(locked(&session), (vec![], vec![]));
    // The collision's live view keeps reading the same storage.
    assert_eq!(view.position(1), Some([1., 0., 0.]));
    // The next access takes a fresh backend lock instead of reusing the
    // stale pointer and flags.
    let again = session
        .arena
        .lock_visual_lod0_vertices(visual)
        .unwrap()
        .unwrap();
    assert!(again.same_buffer(&view));
    assert_eq!(locked(&session).0, held);
    drop((view, again));
    discard(&mut session);
}

#[test]
fn presented_init_runs_the_reset_span_in_native_order_with_the_hud() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    let mut indicators = hud();
    indicators.bonus_set_count(7);
    session.install_indicators(indicators).unwrap();
    adopt_fresh_install_settings(&mut session);
    load(&mut session, &[]).unwrap();
    let (sound, buffer) = stage_dirty_game_state(&mut session);
    stage_dirty_init_clears(&mut session);
    session.player_statics.set_player_navi_point(Some(
        crate::script_scheduler::FrameReferenceId::from_existing(3),
    ));
    session.player_statics.set_can_car_damage(true);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("ff_manager.GameInit must stop Init");
    };
    // Opened without progress, `_show_progress_` is the native no-op and
    // the C_player statics are cleared; the owned spans run on through the
    // HUD GameInit, which reads the night flag PhysicsClear cleared, and
    // EffectsStart's box-placement clears (checked with the other lists).
    assert_eq!(failure.step, InitStep::EffectsStart);
    assert_eq!(failure.error.0, EFFECTS_STOP);
    assert_eq!(
        hud_textures(&session),
        (Some("2intmph.tga"), Some("2intauto.tga"))
    );
    assert_eq!(session.player_statics(), Default::default());
    // GAME 0x5a08c3-0x5a098c: lists emptied, ScoreSetOn(false) hides the
    // HUD bonus (0x7fffffff), score zeroed, flags stored.
    assert_eq!(
        session.game_lists(),
        &crate::game_housekeeping::GameInitLists::default()
    );
    let game = session.game_state();
    assert_eq!(
        game.score(),
        crate::game_housekeeping::ScoreState::default()
    );
    assert_eq!(
        session.indicators().unwrap().bonus_count(),
        crate::indicators::BONUS_NONE
    );
    assert!(!game.camera_rot_repair());
    assert!(game.heal_cheat());
    assert!(game.stacks_clear());
    assert_eq!(game.death_fade_time(), 7000);
    assert!(!game.actor_lightness());
    assert_shadow_buffer_kept(&session, buffer);
    // GAME 0x5a0996-0x5a09b4: the police shift begins and the groups,
    // follow and fight managers are emptied.
    assert_ai_manager_shift_begun(&session);
    assert_vehicle_sectors_rebuilt(&session);
    assert_physics_clear_applied(&session);
    assert_sound_arena_and_lightness_cleared(&session);
    // GAME 0x5a0c93: mafiaIndicators.RadarClear().
    assert_eq!(session.indicators().unwrap().radar_car_count(), 0);
    // Clearing the record released nothing: the sound is still live.
    session.arena.release(sound.frame()).unwrap();
    discard(&mut session);
}

#[test]
fn presented_init_without_hud_stops_at_score_set_on_keeping_earlier_clears() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    // Open needs the HUD (the loading screen's GetLanguage); taking it away
    // afterwards isolates Init's own first HUD effect.
    session.indicators = None;
    let (sound, buffer) = stage_dirty_game_state(&mut session);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the absent HUD must stop Init");
    };
    assert_eq!(failure.step, InitStep::ResetMissionState);
    assert!(failure
        .error
        .0
        .ends_with("Init ScoreSetOn mafiaIndicators owner unavailable"));
    assert!(session.open_restore_volume());
    let lists = session.game_lists();
    assert!(lists.stop_sounds.is_empty());
    assert!(lists.paused_i_shows.is_empty());
    assert!(lists.free_shadow_names.is_empty());
    // The direct score_on store precedes the call; nothing after it runs.
    let game = session.game_state();
    assert_eq!(
        game.score(),
        crate::game_housekeeping::ScoreState {
            score: 7,
            score_on: false,
        }
    );
    assert!(game.camera_rot_repair());
    assert!(!game.heal_cheat());
    assert!(!game.stacks_clear());
    assert_eq!(game.death_fade_time(), 0);
    assert!(game.actor_lightness());
    assert_eq!(lists.unknown_140, [11, 12]);
    // The police shift begin after the reset span never ran.
    let manager = session.ai_manager();
    assert_eq!(manager.police.actions().len(), 1);
    assert_eq!(manager.police.items().len(), 1);
    assert_eq!(manager.groups.unlock_time(), 100);
    assert_eq!(manager.groups.free_actors().len(), 1);
    assert_eq!(manager.follow.check_time(), 300);
    assert_eq!(manager.fight.parties().len(), 1);
    // EffectsStart's box-placement clears are far past the stop.
    assert_eq!(box_placement_counts(&session), [1, 2, 1]);
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(())),
        Err(InitRunError::AlreadyEntered)
    );
    assert_shadow_buffer_kept(&session, buffer);
    session.arena.release(sound.frame()).unwrap();
    // The staged box survives the stop; Done's BoxPlacementDel is unported,
    // so disposal stops there (see disposal_clears_lightness_and_box_lists_in_done_order).
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert!(matches!(
        session.discard_failure().unwrap().cause,
        MissionDiscardCause::BoxPlacementDel { boxes: 1 }
    ));
}

#[test]
fn sound_and_lightness_without_hud_stops_at_radar_clear_keeping_earlier_clears() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    load(&mut session, &[]).unwrap();
    stage_dirty_init_clears(&mut session);
    // The owned prefix in Init order through TrafficFlags, HUD installed.
    use crate::game_init_order::InitOperations as _;
    let mut sequence = SceneSnapshotSequence::default();
    session.execute(InitStep::GuardNotInitialized).unwrap();
    session
        .reset_game_mission_state_with_presentation([64, 64], &mut sequence, &mut |_| Ok(()))
        .unwrap();
    for step in [
        InitStep::PoliceBeginShift,
        InitStep::PlayerNavi,
        InitStep::VehicleSectors,
        InitStep::PhysicsClear,
        InitStep::PoliceGameInit,
        InitStep::TrafficFlags,
    ] {
        session.execute(step).unwrap();
    }
    assert_physics_clear_applied(&session);
    // Without the HUD the step's last call stops it; the sound arena,
    // lightness and invisible-actor clears before it are kept.
    session.indicators = None;
    assert_eq!(
        session.execute(InitStep::SoundAndLightness),
        Err(crate::game_init_order::SessionInitError(
            "SoundAndLightness: Init RadarClear (GAME 0x5fa4a0) mafiaIndicators owner unavailable"
                .into()
        ))
    );
    assert_sound_arena_and_lightness_cleared(&session);
    discard(&mut session);
}

/// A `check.bin` with one node whose flags word is 0x7777, so
/// `DeRiciConns`' clear of 0x1000|0x4000 is observable as 0x2777.
fn write_one_node_check_bin(fixture: &Fixture) {
    let mut node = [0x77u8; 30];
    node[24..28].copy_from_slice(&0xf123_4567u32.to_le_bytes());
    node[28] = 200;
    node[29] = 0;
    let mut bytes = 0x01ab_cedfu32.to_le_bytes().to_vec();
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(node);
    std::fs::write(fixture.0.join("missions/synthetic/check.bin"), bytes).unwrap();
}

#[test]
fn effects_start_runs_the_effects_and_web_path_then_stops_at_ff_manager() {
    use crate::game_init_order::InitOperations as _;
    use crate::session_effects::StaticParticlesOutcome;
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    // MISE01's effects.bin: the empty CT_EFFECTS_BIN root chunk.
    std::fs::write(
        fixture.0.join("missions/synthetic/effects.bin"),
        chunk(0x64, &[]),
    )
    .unwrap();
    write_one_node_check_bin(&fixture);
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    adopt_fresh_install_settings(&mut session);
    stage_box_placements(&mut session);
    // `new C_effects`: no tables\effects.tbl in the fixture, so the
    // constructor's Load found nothing and marked last_guid -1.
    assert!(session.effects().effects().is_empty());
    assert_eq!(session.effects().last_guid(), -1);
    assert!(!session.effects().started());
    // Before the open prefix the step is refused before any clear.
    assert_eq!(
        session.execute(InitStep::EffectsStart),
        Err(crate::game_init_order::SessionInitError(
            "EffectsStart: game init requires completed open prefix".into()
        ))
    );
    assert_eq!(box_placement_counts(&session), [1, 2, 1]);
    assert_eq!(session.effects().static_particles(), None);
    load(&mut session, &[]).unwrap();
    assert_eq!(session.navigation().node_flags(0), Some(0x7777));
    let counts = session.owned_counts().unwrap();
    assert_eq!(counts.box_placements, 1);
    assert_eq!(counts.box_destinations, 2);
    assert_eq!(counts.box_placement_temporaries, 1);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("ff_manager.GameInit must stop Init");
    };
    // GAME 0x5a0cbd-0x5a0d34: the three erases; 0x5a0d3e: the root chunk
    // read to its end; 0x5a0d4b: Mission_Start; 0x5a0d59: DeRiciConns over
    // the loaded check.bin; 0x5a0d63 is the boundary.
    assert_eq!(failure.step, InitStep::EffectsStart);
    assert_eq!(failure.error.0, EFFECTS_STOP);
    assert_eq!(attempt.last(), Some(InitStep::EffectsStart));
    assert_eq!(box_placement_counts(&session), [0, 0, 0]);
    let effects = session.effects();
    assert_eq!(
        effects.static_particles(),
        Some(StaticParticlesOutcome::Read { effects: 0 })
    );
    assert!(effects.started());
    assert_eq!(effects.last_guid(), -1);
    assert_eq!(effects.static_emitter_count(), 0);
    assert_eq!(session.navigation().node_flags(0), Some(0x2777));
    assert_eq!(
        session.owned_counts().unwrap(),
        crate::mission_load_session::MissionOwnedCounts {
            box_placements: 0,
            box_destinations: 0,
            box_placement_temporaries: 0,
            ..counts
        }
    );
    // The stopped attempt is not replayed.
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(())),
        Err(InitRunError::AlreadyEntered)
    );
    // Disposal after the stop: Mission_Done ends the started mission and
    // every owner reaches zero.
    discard(&mut session);
    assert!(!session.effects().started());
}

#[test]
fn keyboard_mouse_startup_clears_diff_state_and_stops_at_missing_segments() {
    use crate::game_ai_segments::{CheckSegCluster, SegmentsLoadStage, SegmentsLoadState};
    use crate::game_loadsave_lifecycle::ProfileFileIntake;
    let source = scene2(
        &[
            chunk(0x4000, &object(9, b"part", &[])),
            chunk(0xae20, &actor_record(b"part", 1, b"")),
        ]
        .concat(),
    );
    let fixture = Fixture::new(Some(&world(&[])), Some(&source), None);
    assert!(!fixture.0.join("missions/synthetic/clusters.seg").exists());
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    adopt_fresh_install_settings(&mut session);
    let bindings = crate::game_loadsave_controls::profile_bindings_after_load_save_init(
        ProfileFileIntake::NoProfileFile,
    )
    .unwrap();
    session
        .adopt_keyboard_mouse_input(
            crate::session_input::KeyboardMouseInput::from_profile(bindings).unwrap(),
        )
        .unwrap();
    load(&mut session, &[]).unwrap();
    let world = &session.loader().unwrap().content.actors.world;
    let actor = world.owned_actor_references().next().unwrap();
    let frame = world.lookup_actor_frame(actor).unwrap().unwrap();
    session.game_lists.diff_frames.extend([frame, frame]);
    session.game_lists.diff_actors.push(actor);
    let capacities = (
        session.game_lists.diff_frames.capacity(),
        session.game_lists.diff_actors.capacity(),
    );
    session.game_inline.set_clear_diff_data();
    session.crash_object_count = u32::MAX;
    let counts = session.owned_counts().unwrap();
    assert_eq!((counts.diff_frames, counts.diff_actors), (2, 1));
    let mut attempt = InitAttempt::default();
    let mut sequence = SceneSnapshotSequence::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("missing clusters.seg must stop Init")
    };
    assert_eq!(failure.step, InitStep::SegmentsLoad);
    assert_eq!(attempt.last(), Some(InitStep::SegmentsLoad));
    assert_eq!(
        failure.error.0,
        concat!(
            "SegmentsLoad: AIManager.segments.Load: SegmentsLoadError { ",
            "stage: Open, failure: Io(Asset(Error(\"original asset unavailable\"))), ",
            "close_error: None }"
        )
    );
    let segments = &session.ai_manager().segments;
    assert_eq!(
        segments.state(),
        SegmentsLoadState::Interrupted(SegmentsLoadStage::Open)
    );
    assert_eq!(segments.clusters(), &[CheckSegCluster::default()]);
    assert_eq!(segments.num_segments(), 0);
    assert_eq!(segments.declared_segment_count(), None);
    assert_eq!(segments.pending_segment(), None);
    assert!(session.game_lists().diff_frames.is_empty());
    assert!(session.game_lists().diff_actors.is_empty());
    assert_eq!(
        (
            session.game_lists().diff_frames.capacity(),
            session.game_lists().diff_actors.capacity(),
        ),
        capacities
    );
    assert!(!session.game_state().clear_diff_data());
    assert_eq!(session.crash_object_count(), 0);
    assert!(crate::frame_lookup::ModelFrameLookup::arena_frame(&session.arena, frame).is_ok());
    assert_eq!(
        session
            .loader()
            .unwrap()
            .content
            .actors
            .world
            .lookup_actor_frame(actor)
            .unwrap(),
        Some(frame)
    );
    let after = session.owned_counts().unwrap();
    assert_eq!((after.diff_frames, after.diff_actors), (0, 0));
    assert_eq!(after.live_frames, counts.live_frames);
    assert_eq!(after.mission_actors, counts.mission_actors);
    assert_eq!(
        (
            after.ai_segment_clusters,
            after.ai_segments,
            after.ai_pending_segments,
        ),
        (1, 0, 0)
    );
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(())),
        Err(InitRunError::AlreadyEntered)
    );
    assert!(session.effects().started());
    assert!(session.force_feedback().game_init_complete());
    assert_eq!(session.force_feedback().effect_flag_bytes(), [0, 0]);
    assert!(!session.game_initialized);
    let replacement = crate::game_loadsave_controls::profile_bindings_after_load_save_init(
        ProfileFileIntake::NoProfileFile,
    )
    .unwrap();
    assert!(session
        .adopt_keyboard_mouse_input(
            crate::session_input::KeyboardMouseInput::from_profile(replacement).unwrap()
        )
        .is_err());
    discard(&mut session);
    assert!(session.input().is_some());
    assert!(!session.effects().started());
    assert_eq!(
        session.ai_manager().segments.state(),
        SegmentsLoadState::Cleared
    );
    assert_eq!(
        session.input().unwrap().controls().binding(1).unwrap().code,
        0x11
    );
}

#[test]
fn diff_clear_requires_open_and_refuses_terminal_disposal_state() {
    use crate::game_init_order::InitOperations as _;
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    let frame = crate::script_scheduler::FrameReferenceId::from_existing(37);
    session.game_lists.diff_frames.push(frame);
    session.game_inline.set_clear_diff_data();
    session.crash_object_count = 21;
    assert_eq!(
        session.execute(InitStep::DiffClear),
        Err(crate::game_init_order::SessionInitError(
            "DiffClear: game init requires completed open prefix".into()
        ))
    );
    assert_eq!(session.game_lists().diff_frames, [frame]);
    assert!(session.game_state().clear_diff_data());
    assert_eq!(session.crash_object_count(), 21);
    load(&mut session, &[]).unwrap();
    session.execute(InitStep::DiffClear).unwrap();
    assert!(session.game_lists().diff_frames.is_empty());
    assert!(!session.game_state().clear_diff_data());
    assert_eq!(session.crash_object_count(), 0);
    discard(&mut session);
    assert!(session.execute(InitStep::DiffClear).is_err());
}

#[test]
fn earlier_init_stop_preserves_diff_state_and_refuses_nonempty_diff_teardown() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    adopt_fresh_install_settings(&mut session);
    load(&mut session, &[]).unwrap();
    let scene_frame = session.scene().unwrap().unwrap().root();
    let frame = session.arena.lookup_reference(scene_frame);
    session.game_lists.diff_frames.push(frame);
    session.game_inline.set_clear_diff_data();
    session.crash_object_count = 23;
    let mut attempt = InitAttempt::default();
    let mut sequence = SceneSnapshotSequence::default();
    let Err(InitRunError::Callback(failure)) =
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()))
    else {
        panic!("missing process input must stop EffectsStart")
    };
    assert_eq!(failure.step, InitStep::EffectsStart);
    assert_eq!(session.game_lists().diff_frames, [frame]);
    assert!(session.game_state().clear_diff_data());
    assert_eq!(session.crash_object_count(), 23);
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::Stopped)
    );
    assert_eq!(
        session.discard_progress().phase,
        MissionDiscardPhase::GameDoneLists
    );
    assert!(matches!(
        session.discard_failure().unwrap().cause,
        MissionDiscardCause::ClearDiffData {
            frames: 1,
            actors: 0
        }
    ));
    assert!(!session.effects().started());
    assert_eq!(session.game_lists().diff_frames, [frame]);
    assert!(session.arena.frame(scene_frame).is_ok());
}

#[test]
fn static_particles_read_scene_only_for_named_frame_lookup() {
    use crate::session_effects::StaticParticlesOutcome;
    let mut entry = vec![0u8; 0x40];
    entry.extend(0i32.to_le_bytes());
    let unnamed = chunk(0x64, &chunk(0x3e8, &entry));
    entry.extend(chunk(0x10, b"part\0"));
    let named = chunk(0x64, &chunk(0x3e8, &entry));
    for (file, outcome) in [
        (None, Some(StaticParticlesOutcome::NotOpened)),
        (
            Some(chunk(0x64, &[])),
            Some(StaticParticlesOutcome::Read { effects: 0 }),
        ),
        (
            Some(unnamed),
            Some(StaticParticlesOutcome::Read { effects: 1 }),
        ),
        (Some(named), None),
    ] {
        let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
        if let Some(bytes) = file {
            std::fs::write(fixture.0.join("missions/synthetic/effects.bin"), bytes).unwrap();
        }
        write_one_node_check_bin(&fixture);
        let mut foreign = fixture.session();
        load(&mut foreign, &[]).unwrap();
        for selection in ["absent", "foreign", "non_scene", "stale"] {
            let mut session = fixture.session();
            load(&mut session, &[]).unwrap();
            let original = session.scene_reference().unwrap();
            let scene = session.scene().unwrap().unwrap();
            session.scene = match selection {
                "absent" => None,
                "foreign" => foreign.scene_reference(),
                "non_scene" => Some(
                    session
                        .arena
                        .lookup_reference(session.arena.scene(scene).unwrap().sectors[0].frame()),
                ),
                "stale" => {
                    let frame = session.arena.create_sound_frame().unwrap();
                    let reference = session.arena.lookup_reference(frame);
                    session.arena.release(frame).unwrap();
                    Some(reference)
                }
                _ => unreachable!(),
            };
            let redirected = session.scene_reference();
            if selection == "absent" {
                assert_eq!(session.scene().unwrap(), None);
            } else {
                assert!(session.scene().is_err(), "{selection}");
            }
            stage_box_placements(&mut session);
            let error = session.effects_start_for_init().unwrap_err();
            assert_eq!(box_placement_counts(&session), [0, 0, 0]);
            assert_eq!(session.scene_reference(), redirected);
            assert_eq!(session.effects().static_particles(), outcome, "{selection}");
            assert_eq!(
                session.effects().started(),
                outcome.is_some(),
                "{selection}"
            );
            if outcome.is_some() {
                assert_eq!(error, super::super::EFFECTS_START_STOP, "{selection}");
                assert_eq!(session.navigation().node_flags(0), Some(0x2777));
            } else {
                let expected = match selection {
                    "absent" => "FindFrame: act_mission has no scene",
                    "foreign" => "scene: ForeignReference",
                    "non_scene" | "stale" => "scene: InvalidScene",
                    _ => unreachable!(),
                };
                assert!(error.contains(expected), "{selection}: {error}");
                assert_eq!(session.navigation().node_flags(0), Some(0x7777));
            }
            session.scene = Some(original);
            discard(&mut session);
        }
        discard(&mut foreign);
    }
}

/// A table with one effect that has a model and a part, and an effects.bin
/// that starts it: `CreateSParticle` passes its checks and needs the
/// unported model and emitter owners.
#[test]
fn a_creatable_static_effect_stops_at_create_s_particle_before_mission_start() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    std::fs::create_dir_all(fixture.0.join("tables")).unwrap();
    let effect = [chunk(0x68, b"fx.i3d\0"), chunk(0x65, &[])].concat();
    std::fs::write(fixture.0.join("tables/effects.tbl"), chunk(0x64, &effect)).unwrap();
    let mut entry = vec![0u8; 0x40];
    entry.extend(0i32.to_le_bytes());
    std::fs::write(
        fixture.0.join("missions/synthetic/effects.bin"),
        chunk(0x64, &chunk(0x3e8, &entry)),
    )
    .unwrap();
    write_one_node_check_bin(&fixture);
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
    adopt_fresh_install_settings(&mut session);
    assert_eq!(session.effects().effects().len(), 1);
    load(&mut session, &[]).unwrap();
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("CreateSParticle must stop Init");
    };
    assert_eq!(failure.step, InitStep::EffectsStart);
    assert!(
        failure
            .error
            .0
            .starts_with("EffectsStart: C_effects::CreateSParticle (GAME 0x44a3a0) for effect 0"),
        "{}",
        failure.error.0
    );
    // Mission_Start and DeRiciConns after LoadStaticParticles never ran;
    // nothing was created.
    assert_eq!(session.navigation().node_flags(0), Some(0x7777));
    let effects = session.effects();
    assert!(!effects.started());
    assert_eq!(effects.static_particles(), None);
    assert_eq!(effects.static_emitter_count(), 0);
    // Mission_Done does nothing for the unstarted mission; owners reach zero
    // and the process-lifetime table stays.
    discard(&mut session);
    assert_eq!(session.effects().effects().len(), 1);
}

/// `C_game::Done` order on disposal: `LightnessClear` and the box
/// placement clears run before the scene is released; `invisible_actors`
/// is left as `Done` leaves it. A listed box needs the unported
/// `BoxPlacementDel` and stops the disposal after `LightnessClear`.
#[test]
fn disposal_clears_lightness_and_box_lists_in_done_order() {
    use crate::game_housekeeping::*;
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    for with_box in [false, true] {
        let mut session = fixture.session();
        load(&mut session, &[]).unwrap();
        stage_box_placements(&mut session);
        if !with_box {
            session.game_lists.boxes.clear();
        }
        let lists = &mut session.game_lists;
        lists.lightness.push(LightnessEntry { marker: 1 });
        lists.invisible_actors.push(InvisibleActor {
            actor: 61,
            frame: 62,
        });
        if !with_box {
            discard(&mut session);
            assert_eq!(box_placement_counts(&session), [0, 0, 0]);
        } else {
            assert_eq!(
                session.discard_terminal(),
                Err(MissionDiscardCallError::Stopped)
            );
            let failure = session.discard_failure().unwrap();
            assert_eq!(failure.phase, MissionDiscardPhase::GameDoneLists);
            assert!(matches!(
                failure.cause,
                MissionDiscardCause::BoxPlacementDel { boxes: 1 }
            ));
            // Nothing after the stop ran: the lists and the scene stay.
            assert_eq!(box_placement_counts(&session), [1, 2, 1]);
            assert!(session.scene().unwrap().is_some());
            let remaining = session.owned_counts().unwrap();
            assert_eq!(remaining.box_placements, 1);
            assert_eq!(remaining.box_destinations, 2);
            assert_eq!(remaining.box_placement_temporaries, 1);
        }
        assert!(session.game_lists().lightness.is_empty());
        assert_eq!(session.game_lists().invisible_actors.len(), 1);
    }
}

#[test]
fn presented_init_initializes_the_global_programs_and_runs_game_init_start_once() {
    use crate::mission_content_loading::NamedGlobalProgramOutcome;
    let fixture = programs_fixture();
    let mut session = programs_session(&fixture);
    assert_eq!(program_runtimes(&session), [false, false]);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("ff_manager.GameInit must stop Init");
    };
    assert_eq!(failure.step, InitStep::EffectsStart);
    assert_eq!(failure.error.0, EFFECTS_STOP);
    // GAME 0x5a0c9e: GlobalProgramGameInit gave every program its runtime;
    // 0x5a0cae: the empty diff_programs left GameInitStart to the mission's
    // program, whose subroutine set the flag PhysicsClear had cleared.
    assert_eq!(program_runtimes(&session), [true, true]);
    let record = session.game_init_programs();
    assert!(record.initialized);
    assert_eq!(
        record.game_init_start,
        Some(Ok(NamedGlobalProgramOutcome::MissionHandled { index: 0 }))
    );
    assert!(session.game_state().night_mission());
    // 0x5a0cb8: the HUD GameInit read that live flag after GameInitStart
    // set it, and the fresh-install speedometer in miles.
    assert_eq!(
        hud_textures(&session),
        (Some("2intnocmph.tga"), Some("2intauton.tga"))
    );
    // `other` was initialized but never called.
    assert!(session.game_state().anim_preload());
    // The single owner refuses a second GameInit of the same programs.
    let mut budget = DispatchBudget::new(64).unwrap();
    assert_eq!(
        session.run_global_program_start(
            &mut budget,
            &mut crate::mission_load_session::GameInitProgramDiagnostic
        ),
        Err("GlobalProgramGameInit already ran for these programs".into())
    );
    assert_eq!(budget.remaining(), 64);
    // The retained runtimes refuse terminal disposal until the host
    // discards them; then every owner reaches zero.
    assert_eq!(
        session.discard_terminal(),
        Err(MissionDiscardCallError::ProgramRuntimeRetained)
    );
    assert_eq!(
        session
            .discard_global_program_runtimes_host()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(program_runtimes(&session), [false, false]);
    discard(&mut session);
}

#[test]
fn a_matching_diff_program_stops_game_init_start_after_the_global_game_init() {
    let fixture = programs_fixture();
    let mut session = programs_session(&fixture);
    session
        .game_inline
        .diff_programs_mut()
        .push(crate::game_housekeeping::DiffProgram {
            name: "gameinitstart".into(),
        });
    let mut sequence = SceneSnapshotSequence::default();
    let result =
        InitAttempt::default()
            .run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the matched diff program must stop Init");
    };
    // C_game::GlobalProgramRun (0x5afd50) matches without case; its
    // CallSubroutine has no owner, so the run stops before the mission's
    // program, after GlobalProgramGameInit initialized both.
    assert_eq!(failure.step, InitStep::GlobalProgramStart);
    assert!(
        failure.error.0.contains("matched diff program Some(0)"),
        "{}",
        failure.error
    );
    assert_eq!(program_runtimes(&session), [true, true]);
    assert!(!session.game_state().night_mission());
    assert!(session.game_init_programs().initialized);
    assert!(matches!(
        session.game_init_programs().game_init_start,
        Some(Err(_))
    ));
    session.discard_global_program_runtimes_host().unwrap();
    discard(&mut session);
}

#[test]
fn init_refuses_to_initialize_programs_a_host_diagnostic_already_initialized() {
    let fixture = programs_fixture();
    let mut session = programs_session(&fixture);
    // A host diagnostic ran the pair first through the same owner.
    let mut budget = DispatchBudget::new(64).unwrap();
    assert_eq!(
        session.run_global_program_start(
            &mut budget,
            &mut crate::mission_load_session::GameInitProgramDiagnostic
        ),
        Ok(crate::mission_content_loading::NamedGlobalProgramOutcome::MissionHandled { index: 0 })
    );
    let mut sequence = SceneSnapshotSequence::default();
    let result =
        InitAttempt::default()
            .run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("Init must not initialize the programs again");
    };
    assert_eq!(failure.step, InitStep::GlobalProgramStart);
    assert_eq!(
        failure.error.0,
        "GlobalProgramStart: GlobalProgramGameInit already ran for these programs"
    );
    assert_eq!(program_runtimes(&session), [true, true]);
    session.discard_global_program_runtimes_host().unwrap();
    discard(&mut session);
}

#[test]
fn hud_game_init_without_profile_settings_stops_before_any_store() {
    let fixture = programs_fixture();
    let mut session = programs_session_without_settings(&fixture);
    assert_eq!(session.load_save_progress().settings, None);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the unset options must stop Init");
    };
    // The reached prefix is kept: GameInitStart ran and set the night flag;
    // GameInit read it, then found no options and stored nothing.
    assert_eq!(failure.step, InitStep::IndicatorsGameInit);
    assert_eq!(failure.error.0, SETTINGS_STOP);
    assert!(session.game_init_programs().initialized);
    assert!(session.game_state().night_mission());
    assert_eq!(hud_textures(&session), (None, None));
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("Init must not replay")
        }),
        Err(InitRunError::AlreadyEntered)
    );
    assert_eq!(
        session
            .discard_global_program_runtimes_host()
            .unwrap()
            .len(),
        2
    );
    discard(&mut session);
}

#[test]
fn profile_intake_does_not_replace_or_partially_install_process_state() {
    use crate::game_loadsave_lifecycle::{FreerideProfile, ProfileFileIntake::*, ProfileSettings};
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    assert!(session.adopt_load_save_profile(ProfileFilePresent).is_err());
    assert_eq!(session.freeride_profile, None);
    assert_eq!(session.load_save_progress().settings, None);
    let settings = session.adopt_load_save_profile(NoProfileFile).unwrap();
    assert!(session.adopt_load_save_profile(NoProfileFile).is_err());
    assert_eq!(session.load_save_progress().settings, Some(settings));
    assert_eq!(
        session.freeride_profile,
        Some(FreerideProfile {
            freeride_enabled: 0,
            race_cars: 1
        })
    );

    let mut session = fixture.session();
    let existing = ProfileSettings {
        speedometer_mph: false,
    };
    session.adopt_profile_settings(existing).unwrap();
    assert!(session.adopt_load_save_profile(NoProfileFile).is_err());
    assert_eq!(session.freeride_profile, None);
    assert_eq!(session.load_save_progress().settings, Some(existing));
}

#[test]
fn profile_settings_are_adopted_once_and_survive_disposal() {
    use crate::game_loadsave_lifecycle::ProfileSettings;
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    adopt_fresh_install_settings(&mut session);
    assert_eq!(
        session.adopt_profile_settings(ProfileSettings {
            speedometer_mph: false
        }),
        Err("mafiaLoadSave.settings already set by the profile intake".into())
    );
    load(&mut session, &[]).unwrap();
    discard(&mut session);
    // A process global like the rest of mafiaLoadSave: not a mission owner.
    assert_eq!(
        session.load_save_progress().settings,
        Some(ProfileSettings {
            speedometer_mph: true
        })
    );
}

/// A world whose `GameInitStart` sets the night flag and then reaches a
/// command whose owner the session lacks (the city-traffic visibility
/// latch), so the run fails part-way through.
fn failing_programs_fixture() -> Fixture {
    let input = scene2(&chunk(
        0xae50,
        &program_record(
            b"GameInitStart",
            b"commandblock 1\ngame_nightmission 1\nsetcitytrafficvisible 1\ncommandblock 0",
        ),
    ));
    Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&input), None)
}

#[test]
fn a_failed_game_init_start_run_stops_init_at_global_program_start() {
    let fixture = failing_programs_fixture();
    let mut session = programs_session(&fixture);
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the failed GameInitStart run must stop Init");
    };
    assert_eq!(failure.step, InitStep::GlobalProgramStart);
    assert!(
        failure.error.0.starts_with(
            "GlobalProgramStart: GlobalProgramRun(\"GameInitStart\") (GAME 0x540580):"
        ),
        "{}",
        failure.error
    );
    assert!(
        failure
            .error
            .0
            .contains("session city-traffic visibility owner unavailable"),
        "{}",
        failure.error
    );
    // The reached prefix: the programs are initialized, the night flag the
    // run stored before the failing command stays, and the HUD GameInit
    // after the step never ran.
    let record = session.game_init_programs();
    assert!(record.entered && record.initialized);
    assert!(matches!(record.game_init_start, Some(Err(_))));
    assert!(session.game_state().night_mission());
    assert_eq!(hud_textures(&session), (None, None));
    assert_eq!(program_runtimes(&session), [true]);
    // Neither the owner nor Init repeats the run.
    let mut budget = DispatchBudget::new(64).unwrap();
    assert_eq!(
        session.run_global_program_start(
            &mut budget,
            &mut crate::mission_load_session::GameInitProgramDiagnostic
        ),
        Err("GlobalProgramGameInit already ran for these programs".into())
    );
    assert_eq!(budget.remaining(), 64);
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("Init must not replay")
        }),
        Err(InitRunError::AlreadyEntered)
    );
    assert_eq!(
        session
            .discard_global_program_runtimes_host()
            .unwrap()
            .len(),
        1
    );
    discard(&mut session);
}

/// A storage-only program backend for staging a runtime.
struct StagingStorage;
impl crate::original_program::RuntimeBackend for StagingStorage {
    fn effect(
        &mut self,
        _: crate::original_program::RuntimeView<'_>,
        _: crate::original_program::RuntimeEffect,
        _: Option<&crate::program_parameters::ScalarParameter>,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn an_interrupted_global_program_game_init_is_not_repeated() {
    let fixture = programs_fixture();
    let mut session = programs_session(&fixture);
    // Test staging: the first program already holds a runtime, so the
    // reverse GameInit initializes `other` (index 1) and then fails at
    // `GameInitStart` (index 0).
    session
        .loader
        .as_mut()
        .unwrap()
        .content
        .program_mut_for_tests(0)
        .unwrap()
        .init_fresh_runtime(&mut StagingStorage)
        .unwrap();
    let mut sequence = SceneSnapshotSequence::default();
    let result =
        InitAttempt::default()
            .run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(()));
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("the failed GameInit must stop Init");
    };
    assert_eq!(failure.step, InitStep::GlobalProgramStart);
    assert!(
        failure
            .error
            .0
            .starts_with("GlobalProgramStart: GlobalProgramGameInit (GAME 0x540520): Program"),
        "{}",
        failure.error
    );
    let record = session.game_init_programs();
    assert!(record.entered);
    assert!(!record.initialized);
    assert_eq!(record.game_init_start, None);
    assert_eq!(program_runtimes(&session), [true, true]);
    // A second pass would initialize `other` again: refused before any
    // program is touched.
    let mut budget = DispatchBudget::new(64).unwrap();
    assert_eq!(
        session.run_global_program_start(
            &mut budget,
            &mut crate::mission_load_session::GameInitProgramDiagnostic
        ),
        Err(
            "GlobalProgramGameInit was interrupted for these programs; its partial initialization is not repeated"
                .into()
        )
    );
    assert_eq!(budget.remaining(), 64);
    assert_eq!(program_runtimes(&session), [true, true]);
    assert!(!session.game_state().night_mission());
    assert_eq!(
        session
            .discard_global_program_runtimes_host()
            .unwrap()
            .len(),
        2
    );
    discard(&mut session);
}

#[test]
fn indicators_install_once_and_keep_the_live_owner() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    assert!(session.indicators().is_none());
    let mut first = hud();
    first.bonus_set_count(3);
    session.install_indicators(first).unwrap();
    assert_eq!(
        session.install_indicators(hud()),
        Err("mafiaIndicators already installed".into())
    );
    assert_eq!(session.indicators().unwrap().bonus_count(), 3);
}

#[test]
fn failed_presentation_retains_muted_prefix_without_restore_or_retry() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let mut attempt = InitAttempt::default();
    let mut sequence = SceneSnapshotSequence::default();
    let result = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
        Err("injected GPU submission failure".into())
    });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("presentation failure must propagate");
    };
    assert_eq!(failure.step, InitStep::ResetMissionState);
    assert!(failure.error.0.contains("injected GPU submission failure"));
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(session.game_init_reset_observation().3, -1);
    assert!(!session.open_restore_volume());
    // Even a new outer attempt cannot repeat an interrupted owner's effects.
    assert!(InitAttempt::default()
        .run_with_presentation(&mut session, [64, 64], &mut sequence, |_| panic!("retry"))
        .is_err());
    assert_eq!(sequence.next_number(), Ok(2));
    discard(&mut session);
}

#[test]
fn unavailable_snapshot_never_calls_presenter_or_skips_the_reached_prefix() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let mut sequence = SceneSnapshotSequence::default();
    let result =
        InitAttempt::default().run_with_presentation(&mut session, [0, 64], &mut sequence, |_| {
            panic!("invalid snapshot must not be presented")
        });
    let Err(InitRunError::Callback(failure)) = result else {
        panic!("snapshot failure must propagate");
    };
    assert_eq!(failure.step, InitStep::ResetMissionState);
    assert!(failure.error.0.contains("snapshot viewport"));
    assert_eq!(sequence.next_number(), Ok(1));
    assert_eq!(session.audio().master_gain().unwrap(), 0.0);
    assert_eq!(session.game_init_reset_observation().3, -1);
    assert!(!session.open_restore_volume());
    discard(&mut session);
}

#[test]
fn admission_failure_does_not_create_a_snapshot_or_call_presentation() {
    let fixture = Fixture::new(None, None, None);
    let mut session = fixture.session();
    let mut sequence = SceneSnapshotSequence::default();
    let result =
        InitAttempt::default().run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("unloaded scene")
        });
    assert!(
        matches!(result, Err(InitRunError::Callback(failure)) if failure.step == InitStep::GuardNotInitialized)
    );
    assert_eq!(sequence.next_number(), Ok(1));
    assert_eq!(session.audio().master_gain().unwrap(), 1.0);
    assert!(!session.open_restore_volume());
}

#[test]
fn presentation_unwind_keeps_one_shot_guards_and_actual_owners() {
    let fixture = Fixture::new(Some(&world(&[])), Some(&scene2(&[])), None);
    let mut session = fixture.session();
    load(&mut session, &[]).unwrap();
    consume_open_restore(&mut session);
    let before = session.owned_counts().unwrap();
    let mut sequence = SceneSnapshotSequence::default();
    let mut attempt = InitAttempt::default();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| {
            panic!("injected presentation unwind");
        });
    }))
    .is_err());
    assert_eq!(attempt.last(), Some(InitStep::ResetMissionState));
    assert_eq!(attempt.run(&mut session), Err(InitRunError::AlreadyEntered));
    assert!(!session.open_restore_volume());
    assert_eq!(session.owned_counts().unwrap(), before);
    discard(&mut session);
}
