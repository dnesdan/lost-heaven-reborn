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
                loading_bars.push(frame.screen.raw_progress());
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
    // sound-arena/lightness/radar clears follow, and the program runtime
    // stops Init.
    assert_eq!(failure.step, InitStep::GlobalProgramStart);
    assert!(
        failure.error.0.starts_with("GlobalProgramStart: C_mission::GlobalProgramGameInit (GAME 0x540520, called at 0x5a0c9e)"),
        "{}",
        failure.error
    );
    assert_vehicle_sectors_rebuilt(&session);
    assert_eq!(loading_bars, [0.7]);
    assert_eq!(
        session
            .load_save_progress()
            .progress
            .as_ref()
            .unwrap()
            .raw_progress(),
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
fn presented_init_render_unlocks_every_buffer_before_presentation() {
    let fixture = Fixture::new(Some(&world(&[dummy(0, b"part")])), Some(&scene2(&[])), None);
    let mut session = fixture.session_without_hud();
    session.install_indicators(hud()).unwrap();
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
    // and runs on to the program runtime.
    assert_eq!(failure.step, InitStep::GlobalProgramStart);
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
        panic!("the program runtime must stop Init");
    };
    // Opened without progress, `_show_progress_` is the native no-op and
    // the C_player statics are cleared; the owned spans run on to the
    // program runtime.
    assert_eq!(failure.step, InitStep::GlobalProgramStart);
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
    assert_eq!(
        attempt.run_with_presentation(&mut session, [64, 64], &mut sequence, |_| Ok(())),
        Err(InitRunError::AlreadyEntered)
    );
    assert_shadow_buffer_kept(&session, buffer);
    session.arena.release(sound.frame()).unwrap();
    discard(&mut session);
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
