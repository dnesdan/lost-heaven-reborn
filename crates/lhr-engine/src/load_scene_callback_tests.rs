//! `cbLoadScene` (GAME 0x5bcc10) and the scene-open report sequence.
use super::*;
use crate::game_loadsave_lifecycle::{
    LOADPROGRESS_LOGO, LOADPROGRESS_MISSION_NAME, LOADPROGRESS_MISSION_PICTURE, LOADPROGRESS_SIDE,
    LOADPROGRESS_UNKNOWN_1000, LOADPROGRESS_UNKNOWN_2000, LOADPROGRESS_UNKNOWN_3000,
};
use crate::menu::LOADPROGRESS_BAR_ID;

fn item(id: u32, kind: [u8; 4]) -> lhr_assets::MenuItem {
    lhr_assets::MenuItem {
        id,
        kind,
        rect: [0.0, 0.0, 10.0, 10.0],
        text_id: 0,
        style: 0,
    }
}

/// A `loadprogress`-like definition: the window, the items `OnCreate`
/// addresses and the bar.
fn definition() -> lhr_assets::MenuDefinition {
    lhr_assets::MenuDefinition {
        version: 1,
        reserved: 0,
        items: vec![
            item(1, *b"dniw"),
            item(LOADPROGRESS_SIDE, *b"ctip"),
            item(LOADPROGRESS_MISSION_PICTURE, *b"ctip"),
            item(LOADPROGRESS_LOGO, *b"ctip"),
            item(LOADPROGRESS_MISSION_NAME, *b"txet"),
            item(LOADPROGRESS_BAR_ID, *b"rabp"),
            item(LOADPROGRESS_UNKNOWN_1000, *b"txet"),
            item(LOADPROGRESS_UNKNOWN_2000, *b"txet"),
            item(LOADPROGRESS_UNKNOWN_3000, *b"rabp"),
        ],
    }
}

fn load_def_entry(mission: &[u8], picture: &[u8], name_id: u32) -> Vec<u8> {
    let mut entry = vec![0u8; 0x44];
    entry[..mission.len()].copy_from_slice(mission);
    entry[0x20..0x20 + picture.len()].copy_from_slice(picture);
    entry[0x40..].copy_from_slice(&name_id.to_le_bytes());
    entry
}

#[derive(Default)]
struct Inputs {
    definition: Option<lhr_assets::MenuDefinition>,
    load_def: Option<Vec<u8>>,
    act_mission: Option<Vec<u8>>,
    fail_definition: bool,
    reads: Vec<&'static str>,
}

impl LoadProgressCreateInputs for Inputs {
    fn menu_definition(
        &mut self,
        name: &str,
    ) -> Result<Option<lhr_assets::MenuDefinition>, String> {
        assert_eq!(name, "loadprogress");
        self.reads.push("menu");
        if self.fail_definition {
            return Err("menu file refused".into());
        }
        Ok(self.definition.clone())
    }
    fn load_def(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.reads.push("load.def");
        Ok(self.load_def.clone())
    }
    fn act_mission_name(&mut self) -> Result<Vec<u8>, String> {
        self.reads.push("act_mission");
        self.act_mission
            .clone()
            .ok_or_else(|| "act_mission->mission_name is NULL".into())
    }
}

#[derive(Debug, PartialEq)]
enum Call {
    AlphaBlend,
    Present(LoadingScreenSource, u32),
}

#[derive(Default)]
struct Device {
    calls: Vec<Call>,
    refuse_present: bool,
}

impl LoadingScreenDevice for Device {
    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        self.calls.push(Call::AlphaBlend);
        Ok(())
    }
    fn present_loading_screen(&mut self, frame: LoadingScreenFrame<'_>) -> Result<(), String> {
        self.calls.push(Call::Present(
            frame.source,
            frame.screen.raw_progress().to_bits(),
        ));
        if self.refuse_present {
            return Err("host presentation refused".into());
        }
        Ok(())
    }
}

fn inputs() -> Inputs {
    Inputs {
        definition: Some(definition()),
        load_def: Some(load_def_entry(b"mise01", b"M01.tga", 0x5dc)),
        act_mission: Some(b"MISE01".to_vec()),
        ..Default::default()
    }
}

fn step(
    callback: &mut LoadSceneCallback,
    progress: &mut Option<LoadProgressScreen>,
    msg: u32,
    value: f32,
    phase: u32,
    device: &mut Device,
) -> Result<(), String> {
    callback.cb_load_scene(progress, msg, value.to_bits(), phase, &mut inputs(), device)
}

#[test]
fn other_messages_return_at_once() {
    let mut callback = LoadSceneCallback::default();
    let mut progress = Some(LoadProgressScreen::for_mission_open());
    let mut device = Device::default();
    // CBM_ERROR/CBM_LOG and anything else: no Create, no device, no store.
    for msg in [0, 1, 7, 9, 9_999, 10_001] {
        step(
            &mut callback,
            &mut progress,
            msg,
            0.0,
            LOAD_PROGRESS_BEGIN,
            &mut device,
        )
        .unwrap();
    }
    assert!(device.calls.is_empty());
    assert_eq!(progress, Some(LoadProgressScreen::for_mission_open()));
    assert_eq!(callback, LoadSceneCallback::default());
}

#[test]
fn begin_creates_and_presents_the_mafia_load_save_screen() {
    let mut callback = LoadSceneCallback {
        menu_progress: Some(LoadProgressScreen::for_load_callback()),
        last_bit: 0.4,
    };
    let mut progress = Some(LoadProgressScreen::for_mission_open());
    let mut device = Device::default();
    let mut inputs = inputs();
    callback
        .cb_load_scene(
            &mut progress,
            CBM_PROGRESS,
            0.0f32.to_bits(),
            LOAD_PROGRESS_BEGIN,
            &mut inputs,
            &mut device,
        )
        .unwrap();
    // menuProgress = NULL (the old screen is dropped), Create, Present.
    assert_eq!(callback.menu_progress(), None);
    assert_eq!(callback.last_bit(), 0.0);
    assert_eq!(inputs.reads, ["menu", "load.def", "act_mission"]);
    let screen = progress.as_ref().unwrap();
    assert_eq!(screen.menu.len(), definition().items.len());
    assert_eq!(
        device.calls,
        [Call::Present(LoadingScreenSource::MafiaLoadSave, 0)]
    );
    // OnCreate's stores (no title: the title items stay untouched).
    assert_eq!(
        screen.menu.staged_string(LOADPROGRESS_SIDE),
        Some(Some(&b"loadbok.tga"[..]))
    );
    assert_eq!(
        screen.menu.staged_string(LOADPROGRESS_LOGO),
        Some(Some(&b"mafia.tga"[..]))
    );
    assert_eq!(
        screen.menu.staged_string(LOADPROGRESS_MISSION_PICTURE),
        Some(Some(&b"M01.tga"[..]))
    );
    assert_eq!(screen.menu.get_text_id(LOADPROGRESS_MISSION_NAME), 0x5dc);
    for id in [
        LOADPROGRESS_UNKNOWN_1000,
        LOADPROGRESS_UNKNOWN_2000,
        LOADPROGRESS_UNKNOWN_3000,
    ] {
        assert!(!screen.menu.find_component_by_id(id).unwrap().is_visible());
    }
    assert!(screen
        .menu
        .find_component_by_id(LOADPROGRESS_SIDE)
        .unwrap()
        .is_visible());
}

#[test]
fn begin_without_a_screen_creates_the_callbacks_own() {
    let mut callback = LoadSceneCallback {
        menu_progress: None,
        last_bit: 0.3,
    };
    let mut progress = None;
    let mut device = Device::default();
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.0,
        LOAD_PROGRESS_BEGIN,
        &mut device,
    )
    .unwrap();
    let own = callback.menu_progress().unwrap();
    // GM_LoadProgress(0, 1.0f, NULL), created.
    assert_eq!(own.mission_part.to_bits(), 0x3f80_0000);
    assert_eq!(own.title, 0);
    assert_eq!(own.menu.len(), definition().items.len());
    assert_eq!(progress, None);
    assert_eq!(callback.last_bit(), 0.0);
    assert_eq!(
        device.calls,
        [Call::Present(LoadingScreenSource::LoadCallback, 0)]
    );
    // END destroys it; a CBM_PROGRESS END is a STEP instead.
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        1.0,
        LOAD_PROGRESS_END,
        &mut device,
    )
    .unwrap();
    assert!(callback.menu_progress().is_some());
    step(
        &mut callback,
        &mut progress,
        CBM_MISSION_PROGRESS,
        1.0,
        LOAD_PROGRESS_END,
        &mut device,
    )
    .unwrap();
    assert_eq!(callback.menu_progress(), None);
}

#[test]
fn steps_map_their_message_and_skip_small_changes() {
    let mut callback = LoadSceneCallback::default();
    let mut progress = Some(LoadProgressScreen::for_mission_open());
    let mut device = Device::default();
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.0,
        LOAD_PROGRESS_BEGIN,
        &mut device,
    )
    .unwrap();
    device.calls.clear();
    // CBM_PROGRESS halves: 0.03 -> 0.015, below 0.02: nothing at all.
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.03,
        LOAD_PROGRESS_STEP,
        &mut device,
    )
    .unwrap();
    assert!(device.calls.is_empty());
    assert_eq!(callback.last_bit(), 0.0);
    // 0.04 -> 0.02: not below, shown at 0.02 * 0.7 of the bar.
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.04,
        LOAD_PROGRESS_STEP,
        &mut device,
    )
    .unwrap();
    let bar = 0.02f32 * 0.7;
    assert_eq!(
        device.calls,
        [
            Call::AlphaBlend,
            Call::Present(LoadingScreenSource::MafiaLoadSave, bar.to_bits())
        ]
    );
    assert_eq!(callback.last_bit(), 0.02);
    // The scene's END is a STEP at half the bar.
    device.calls.clear();
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        1.0,
        LOAD_PROGRESS_END,
        &mut device,
    )
    .unwrap();
    assert_eq!(callback.last_bit(), 0.5);
    assert_eq!(progress.as_ref().unwrap().raw_progress(), 0.5 * 0.7);
    // The mission's message maps (p + 1) / 2.
    step(
        &mut callback,
        &mut progress,
        CBM_MISSION_PROGRESS,
        0.2,
        LOAD_PROGRESS_STEP,
        &mut device,
    )
    .unwrap();
    assert_eq!(callback.last_bit(), (0.2f32 + 1.0) * 0.5);
    // NaN compares unordered and is shown (jnp falls through).
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        f32::NAN,
        LOAD_PROGRESS_STEP,
        &mut device,
    )
    .unwrap();
    assert!(callback.last_bit().is_nan());
    // An unknown phase does nothing.
    device.calls.clear();
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.9,
        5,
        &mut device,
    )
    .unwrap();
    assert!(device.calls.is_empty());
}

#[test]
fn a_step_without_any_screen_still_sets_the_state_and_last_bit() {
    let mut callback = LoadSceneCallback::default();
    let mut progress = None;
    let mut device = Device::default();
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.5,
        LOAD_PROGRESS_STEP,
        &mut device,
    )
    .unwrap();
    assert_eq!(device.calls, [Call::AlphaBlend]);
    assert_eq!(callback.last_bit(), 0.25);
}

#[test]
fn end_keeps_the_mafia_load_save_screen() {
    let mut callback = LoadSceneCallback::default();
    let mut progress = Some(LoadProgressScreen::for_mission_open());
    let mut device = Device::default();
    step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.0,
        LOAD_PROGRESS_BEGIN,
        &mut device,
    )
    .unwrap();
    let created = progress.clone();
    step(
        &mut callback,
        &mut progress,
        CBM_MISSION_PROGRESS,
        1.0,
        LOAD_PROGRESS_END,
        &mut device,
    )
    .unwrap();
    assert_eq!(progress, created);
    assert_eq!(device.calls.len(), 1);
}

#[test]
fn a_refused_present_stops_before_the_last_bit_store() {
    let mut callback = LoadSceneCallback {
        menu_progress: None,
        last_bit: 0.3,
    };
    let mut progress = Some(LoadProgressScreen::for_mission_open());
    let mut device = Device {
        refuse_present: true,
        ..Default::default()
    };
    let error = step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        0.0,
        LOAD_PROGRESS_BEGIN,
        &mut device,
    )
    .unwrap_err();
    assert_eq!(error, "host presentation refused");
    // Create ran; BEGIN's `last_bit = 0` did not.
    assert!(!progress.as_ref().unwrap().menu.is_empty());
    assert_eq!(callback.last_bit(), 0.3);
    let error = step(
        &mut callback,
        &mut progress,
        CBM_PROGRESS,
        1.6,
        LOAD_PROGRESS_STEP,
        &mut device,
    )
    .unwrap_err();
    assert_eq!(error, "host presentation refused");
    assert_eq!(progress.as_ref().unwrap().raw_progress(), 0.8 * 0.7);
    assert_eq!(callback.last_bit(), 0.3);
}

#[test]
fn a_failed_create_stops_before_present() {
    let mut callback = LoadSceneCallback::default();
    let mut progress = Some(LoadProgressScreen::for_mission_open());
    let mut device = Device::default();
    let mut inputs = Inputs {
        fail_definition: true,
        ..inputs()
    };
    let error = callback
        .cb_load_scene(
            &mut progress,
            CBM_PROGRESS,
            0,
            LOAD_PROGRESS_BEGIN,
            &mut inputs,
            &mut device,
        )
        .unwrap_err();
    assert_eq!(error, "menu file refused");
    assert!(device.calls.is_empty());
    assert_eq!(inputs.reads, ["menu"]);
}

#[test]
fn on_create_follows_the_native_order_and_first_match() {
    // A missing menu file is the native early return: no components, so
    // every write is a no-op, but load.def is still read.
    let mut screen = LoadProgressScreen::for_mission_open();
    let mut missing = Inputs {
        load_def: None,
        ..Default::default()
    };
    screen.create(&mut missing).unwrap();
    assert!(screen.menu.is_empty());
    assert_eq!(missing.reads, ["menu", "load.def"]);
    // Title, the screen's own mission name, case-insensitive first match,
    // a zero name id leaves the name item alone, a truncated tail entry is
    // not counted.
    let mut screen = LoadProgressScreen::new(0x2cf, 0.5, Some(b"Mise02".to_vec()));
    let mut bytes = load_def_entry(b"mise01", b"M01.tga", 7);
    bytes.extend(load_def_entry(b"MISE02", b"M02.tga", 0));
    bytes.extend(load_def_entry(b"mise02", b"other.tga", 9));
    bytes.extend([0u8; 0x43]);
    let mut own = Inputs {
        definition: Some(definition()),
        load_def: Some(bytes),
        ..Default::default()
    };
    screen.create(&mut own).unwrap();
    assert_eq!(own.reads, ["menu", "load.def"]);
    assert_eq!(
        screen.menu.staged_string(LOADPROGRESS_MISSION_PICTURE),
        Some(Some(&b"M02.tga"[..]))
    );
    assert_eq!(screen.menu.get_text_id(LOADPROGRESS_MISSION_NAME), 0);
    // A screen without the title items stages no title text; with the
    // items, SetTextID(0x190, title) and (0x191, 0x2ce).
    let mut titled = definition();
    titled
        .items
        .extend([item(0x190, *b"txet"), item(0x191, *b"txet")]);
    let mut screen = LoadProgressScreen::new(0x2cf, 0.5, None);
    let mut act = Inputs {
        definition: Some(titled),
        load_def: Some(load_def_entry(b"mise01", b"M01.tga", 7)),
        act_mission: Some(b"mise01".to_vec()),
        ..Default::default()
    };
    screen.create(&mut act).unwrap();
    assert_eq!(screen.menu.get_text_id(0x190), 0x2cf);
    assert_eq!(screen.menu.get_text_id(0x191), 0x2ce);
    assert_eq!(screen.menu.get_text_id(LOADPROGRESS_MISSION_NAME), 7);
}

#[test]
fn on_create_rereads_the_act_mission_name_per_entry_and_bounds_its_reads() {
    let mut bytes = load_def_entry(b"mise01", b"M01.tga", 1);
    bytes.extend(load_def_entry(b"mise02", b"M02.tga", 2));
    let mut screen = LoadProgressScreen::for_mission_open();
    let mut inputs = Inputs {
        definition: Some(definition()),
        load_def: Some(bytes),
        act_mission: Some(b"mise02".to_vec()),
        ..Default::default()
    };
    screen.create(&mut inputs).unwrap();
    assert_eq!(
        inputs.reads,
        ["menu", "load.def", "act_mission", "act_mission"]
    );
    assert_eq!(screen.menu.get_text_id(LOADPROGRESS_MISSION_NAME), 2);
    // NULL act_mission name: the compare stops the screen's creation.
    let mut screen = LoadProgressScreen::for_mission_open();
    let mut inputs = Inputs {
        definition: Some(definition()),
        load_def: Some(load_def_entry(b"mise01", b"M01.tga", 1)),
        ..Default::default()
    };
    assert_eq!(
        screen.create(&mut inputs).unwrap_err(),
        "act_mission->mission_name is NULL"
    );
    // A matching entry whose picture has no NUL in the read bytes.
    let mut entry = load_def_entry(b"mise01", b"", 1);
    entry[0x20..0x44].fill(b'x');
    let mut inputs = Inputs {
        definition: Some(definition()),
        load_def: Some(entry),
        act_mission: Some(b"mise01".to_vec()),
        ..Default::default()
    };
    assert!(screen
        .create(&mut inputs)
        .unwrap_err()
        .contains("runs past the read bytes"));
}

#[test]
fn scene_open_reports_follow_load_hierarchy_and_load_image() {
    let bits = |value: f32| value.to_bits();
    assert_eq!(
        scene_open_progress_reports(0, 0),
        [
            (bits(0.0), LOAD_PROGRESS_BEGIN),
            (bits(1.0), LOAD_PROGRESS_END)
        ]
    );
    // (i / 2) / 4 for materials, (j / 2) / 4 + 0.5 for frames, each through
    // the 0.02 gate starting from 0.
    assert_eq!(
        scene_open_progress_reports(4, 4),
        [
            (bits(0.0), LOAD_PROGRESS_BEGIN),
            (bits(0.25), LOAD_PROGRESS_STEP),
            (bits(0.5), LOAD_PROGRESS_STEP),
            (bits(0.75), LOAD_PROGRESS_STEP),
            (bits(1.0), LOAD_PROGRESS_END),
        ]
    );
    // Dense counts are thinned by the gate, never reported twice.
    let reports = scene_open_progress_reports(1000, 1000);
    let steps: Vec<f32> = reports[1..reports.len() - 1]
        .iter()
        .map(|(value, _)| f32::from_bits(*value))
        .collect();
    assert!(steps.windows(2).all(|pair| pair[1] >= pair[0] + 0.02));
    assert_eq!(steps.len(), 49);
}
