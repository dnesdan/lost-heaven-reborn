//! `cbLoadScene` (GAME 0x5bcc10) and the scene-open report sequence.
use super::*;
use crate::game_loadsave_lifecycle::{
    LOADPROGRESS_LOGO, LOADPROGRESS_MISSION_NAME, LOADPROGRESS_MISSION_PICTURE, LOADPROGRESS_SIDE,
    LOADPROGRESS_UNKNOWN_1000, LOADPROGRESS_UNKNOWN_2000, LOADPROGRESS_UNKNOWN_3000,
};
use crate::itexture::{CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources};
use crate::menu::{MenuComponentCreateContext, LOADPROGRESS_BAR_ID};
use crate::menu_draw_resources::{MenuComponentResources, MenuTextDatabase};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
enum PresentEvent {
    Reset,
    Clear(u32, f32),
    Begin,
    Blend,
    Z(bool),
    Height,
    Width,
    Viewport(MenuNativeViewport),
    Mesh,
    Cursor(f32, f32),
    End,
    Present,
}

#[derive(Default)]
struct PresentDevice {
    events: Vec<PresentEvent>,
    refuse_at: Option<usize>,
    meshes: Vec<Mesh>,
    after_first_mesh: Option<Box<dyn FnOnce()>>,
}

impl PresentDevice {
    fn record(&mut self, event: PresentEvent) -> Result<(), String> {
        self.events.push(event);
        if self.refuse_at == Some(self.events.len() - 1) {
            return Err("device refused operation".to_owned());
        }
        Ok(())
    }
}

impl LoadingScreenPresentBackend for PresentDevice {
    fn reset_render_props(&mut self) -> Result<(), String> {
        self.record(PresentEvent::Reset)
    }

    fn clear_color_and_depth(&mut self, argb: u32, depth: f32) -> Result<(), String> {
        self.record(PresentEvent::Clear(argb, depth))
    }

    fn begin_scene(&mut self) -> Result<(), String> {
        self.record(PresentEvent::Begin)
    }

    fn set_alpha_blend_state(&mut self) -> Result<(), String> {
        self.record(PresentEvent::Blend)
    }

    fn set_z_buffer(&mut self, enabled: bool) -> Result<(), String> {
        self.record(PresentEvent::Z(enabled))
    }

    fn screen_height(&mut self) -> Result<u32, String> {
        self.record(PresentEvent::Height)?;
        Ok(480)
    }

    fn screen_width(&mut self) -> Result<u32, String> {
        self.record(PresentEvent::Width)?;
        Ok(640)
    }

    fn set_viewport(&mut self, viewport: MenuNativeViewport) -> Result<(), String> {
        self.record(PresentEvent::Viewport(viewport))
    }

    fn draw_menu_mesh(&mut self, mesh: Mesh) -> Result<(), String> {
        self.record(PresentEvent::Mesh)?;
        self.meshes.push(mesh);
        if self.meshes.len() == 1 {
            if let Some(after_first_mesh) = self.after_first_mesh.take() {
                after_first_mesh();
            }
        }
        Ok(())
    }

    fn draw_cursor(&mut self, x: f32, y: f32) -> Result<(), String> {
        self.record(PresentEvent::Cursor(x, y))
    }

    fn end_scene(&mut self) -> Result<(), String> {
        self.record(PresentEvent::End)
    }

    fn present_scene(&mut self) -> Result<(), String> {
        self.record(PresentEvent::Present)
    }
}

#[test]
fn rootless_loading_present_reaches_device_in_order_without_resources() {
    let mut screen = LoadProgressScreen::new(0, 0.7, None);
    let mut inputs = Inputs {
        layout: Some(present_layout()),
        ..Default::default()
    };
    inputs
        .progress_reply
        .set(Some(Err("progress lookup must not run")));
    screen.create(&mut inputs).unwrap();
    let globals = MenuNativeDrawState::new(0, None, [0, 0]);
    let expected = [
        PresentEvent::Reset,
        PresentEvent::Clear(0, 1.0),
        PresentEvent::Begin,
        PresentEvent::Blend,
        PresentEvent::Z(false),
        PresentEvent::Z(true),
        PresentEvent::End,
        PresentEvent::Present,
    ];
    for source in [
        LoadingScreenSource::MafiaLoadSave,
        LoadingScreenSource::LoadCallback,
    ] {
        let frame = LoadingScreenFrame::from_inputs(source, &screen, &inputs);
        for refuse_at in 0..=expected.len() {
            let mut device = PresentDevice {
                refuse_at: (refuse_at < expected.len()).then_some(refuse_at),
                ..Default::default()
            };
            let result = frame.present(
                &globals,
                MenuComponentPrecision::Unestablished,
                MenuTextPrecision::Unestablished,
                &mut device,
            );
            assert_eq!(
                device.events,
                expected[..(refuse_at + 1).min(expected.len())]
            );
            if refuse_at < expected.len() {
                assert!(result.unwrap_err().contains("device refused operation"));
            } else {
                assert_eq!(result, Ok(()));
            }
            assert_eq!(inputs.progress_lookups.get(), 0);
        }
    }
}

#[test]
fn loading_present_stops_at_cursor_precision_or_draws_active_cursor() {
    let screen = LoadProgressScreen::new(0, 0.7, None);
    let inputs = Inputs::default();
    let frame =
        LoadingScreenFrame::from_inputs(LoadingScreenSource::LoadCallback, &screen, &inputs);
    let globals = MenuNativeDrawState::new(701, Some(frame.menu_identity()), [11, -12]);
    let viewport = MenuNativeViewport {
        x: 0,
        y: 0,
        width: 640,
        height: 480,
        min_z: 0.0,
        max_z: 1.0,
    };
    let expected = [
        PresentEvent::Reset,
        PresentEvent::Clear(0, 1.0),
        PresentEvent::Begin,
        PresentEvent::Blend,
        PresentEvent::Z(false),
        PresentEvent::Height,
        PresentEvent::Width,
        PresentEvent::Width,
        PresentEvent::Height,
        PresentEvent::Viewport(viewport),
        PresentEvent::Z(false),
        PresentEvent::Cursor(11.0, -12.0),
        PresentEvent::Z(true),
        PresentEvent::End,
        PresentEvent::Present,
    ];
    let mut device = PresentDevice::default();
    let error = frame
        .present(
            &globals,
            MenuComponentPrecision::Unestablished,
            MenuTextPrecision::Unestablished,
            &mut device,
        )
        .unwrap_err();
    assert!(error.contains("CursorY: PrecisionUnestablished"), "{error}");
    assert_eq!(device.events, expected[..11]);

    for refuse_at in 0..=expected.len() {
        let mut device = PresentDevice {
            refuse_at: (refuse_at < expected.len()).then_some(refuse_at),
            ..Default::default()
        };
        let result = frame.present(
            &globals,
            MenuComponentPrecision::Pc24Nearest,
            MenuTextPrecision::Unestablished,
            &mut device,
        );
        assert_eq!(
            device.events,
            expected[..(refuse_at + 1).min(expected.len())]
        );
        assert_eq!(result.is_ok(), refuse_at == expected.len());
    }
}

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
    layout: Option<MenuComponentCreateContext>,
    resources: Option<MenuDrawResources>,
    progress_reply: PresentProgressReply,
    progress_lookups: Cell<usize>,
}

type PresentProgressReply = Rc<Cell<Option<Result<TextureKey, &'static str>>>>;

impl LoadProgressCreateInputs for Inputs {
    fn native_layout(&self) -> Option<MenuComponentCreateContext> {
        self.layout
    }

    fn component_resources(&mut self) -> Option<&mut dyn MenuComponentResources> {
        self.resources
            .as_mut()
            .map(|resources| resources as &mut dyn MenuComponentResources)
    }

    fn draw_resources(&self) -> Option<&MenuDrawResources> {
        self.resources.as_ref()
    }

    fn progress_texture(&self) -> Result<Option<TextureKey>, String> {
        self.progress_lookups.set(self.progress_lookups.get() + 1);
        self.progress_reply.get().transpose().map_err(str::to_owned)
    }

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

fn present_layout() -> MenuComponentCreateContext {
    MenuComponentCreateContext {
        screen_scale: [1.0; 2],
        left_margin_x: 0.0,
        precision: MenuComponentPrecision::Pc24Nearest,
    }
}

const FIRST_PROGRESS_PIXEL: [u8; 4] = [1, 2, 3, 255];
const SECOND_PROGRESS_PIXEL: [u8; 4] = [4, 5, 6, 255];

struct PresentImages;

impl TextureImageSource for PresentImages {
    fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, TextureImageError> {
        let [red, green, blue, alpha] = if path.ends_with(b"first.tga") {
            FIRST_PROGRESS_PIXEL
        } else if path.ends_with(b"second.tga") {
            SECOND_PROGRESS_PIXEL
        } else {
            return Err(TextureImageError::Missing);
        };
        let mut image = vec![0; 18];
        image[2] = 2;
        image[12] = 1;
        image[14] = 1;
        image[16] = 32;
        image[17] = 0x28;
        image.extend([blue, green, red, alpha]);
        Ok(image)
    }
}

fn present_progress_fixture() -> (LoadProgressScreen, Inputs, TextureKey) {
    let mut resources = MenuDrawResources::from_cpu_factory(
        MenuTextDatabase::new(),
        TextureResources::new().unwrap(),
        CpuTextureStorage::default(),
        PresentImages,
    )
    .unwrap();
    let first = resources
        .create_alpha_texture(b"first.tga")
        .unwrap()
        .unwrap();
    let second = resources
        .create_alpha_texture(b"second.tga")
        .unwrap()
        .unwrap();
    assert_ne!(first, second);
    let mut items = vec![item(1, *b"cniw"), item(LOADPROGRESS_BAR_ID, *b"rabp")];
    for item in &mut items {
        item.style = crate::main_menu::COMPONENT_VISIBLE;
    }
    let mut inputs = Inputs {
        definition: Some(lhr_assets::MenuDefinition {
            version: 1,
            reserved: 0,
            items,
        }),
        layout: Some(present_layout()),
        resources: Some(resources),
        ..Default::default()
    };
    inputs.progress_reply.set(Some(Ok(first)));
    let mut screen = LoadProgressScreen::for_mission_open();
    screen.create(&mut inputs).unwrap();
    screen.set_raw_progress(0.5).unwrap();
    (screen, inputs, second)
}

const PRESENT_THROUGH_GRAY: [PresentEvent; 12] = [
    PresentEvent::Reset,
    PresentEvent::Clear(0, 1.0),
    PresentEvent::Begin,
    PresentEvent::Blend,
    PresentEvent::Z(false),
    PresentEvent::Height,
    PresentEvent::Width,
    PresentEvent::Width,
    PresentEvent::Height,
    PresentEvent::Viewport(MenuNativeViewport {
        x: 0,
        y: 0,
        width: 640,
        height: 480,
        min_z: 0.0,
        max_z: 1.0,
    }),
    PresentEvent::Z(false),
    PresentEvent::Mesh,
];

#[test]
fn loading_present_resolves_progress_again_after_gray_submission() {
    let (screen, inputs, second) = present_progress_fixture();
    let frame =
        LoadingScreenFrame::from_inputs(LoadingScreenSource::LoadCallback, &screen, &inputs);
    let reply = Rc::clone(&inputs.progress_reply);
    let mut device = PresentDevice {
        after_first_mesh: Some(Box::new(move || reply.set(Some(Ok(second))))),
        ..Default::default()
    };
    assert_eq!(inputs.progress_lookups.get(), 0);
    frame
        .present(
            &MenuNativeDrawState::new(0, None, [0, 0]),
            MenuComponentPrecision::Unestablished,
            MenuTextPrecision::Unestablished,
            &mut device,
        )
        .unwrap();
    assert_eq!(inputs.progress_lookups.get(), 2);
    assert_eq!(
        device.events[..PRESENT_THROUGH_GRAY.len()],
        PRESENT_THROUGH_GRAY
    );
    assert_eq!(
        device.events[PRESENT_THROUGH_GRAY.len()..],
        [
            PresentEvent::Mesh,
            PresentEvent::Z(true),
            PresentEvent::End,
            PresentEvent::Present
        ]
    );
    assert_eq!(device.meshes.len(), 2);
    assert_eq!(
        device.meshes[0].texture.as_ref().unwrap().rgba,
        FIRST_PROGRESS_PIXEL
    );
    assert_eq!(
        device.meshes[1].texture.as_ref().unwrap().rgba,
        SECOND_PROGRESS_PIXEL
    );
    assert_eq!(device.meshes[1].vertices[3].uv, [0.5, 1.0]);
}

#[test]
fn loading_present_rejects_a_missing_named_progress_texture_at_each_layer() {
    for missing_on_red in [false, true] {
        let (screen, mut inputs, _) = present_progress_fixture();
        let missing = inputs
            .resources
            .as_mut()
            .unwrap()
            .create_alpha_texture(MenuSharedTexture::Progress.filename())
            .unwrap();
        assert_eq!(missing, None);
        let reply = Rc::clone(&inputs.progress_reply);
        let mut device = PresentDevice::default();
        if missing_on_red {
            device.after_first_mesh = Some(Box::new(move || reply.set(missing.map(Ok))));
        } else {
            reply.set(missing.map(Ok));
        }
        let frame =
            LoadingScreenFrame::from_inputs(LoadingScreenSource::MafiaLoadSave, &screen, &inputs);
        let error = frame
            .present(
                &MenuNativeDrawState::new(0, None, [0, 0]),
                MenuComponentPrecision::Unestablished,
                MenuTextPrecision::Unestablished,
                &mut device,
            )
            .unwrap_err();
        assert!(
            error.contains("missing loading progress texture: kulicky.tga"),
            "{error}"
        );
        if missing_on_red {
            assert_eq!(inputs.progress_lookups.get(), 2);
            assert_eq!(device.events, PRESENT_THROUGH_GRAY);
            assert_eq!(device.meshes.len(), 1);
            assert_eq!(
                device.meshes[0].texture.as_ref().unwrap().rgba,
                FIRST_PROGRESS_PIXEL
            );
        } else {
            assert_eq!(inputs.progress_lookups.get(), 1);
            assert_eq!(
                device.events,
                &PRESENT_THROUGH_GRAY[..PRESENT_THROUGH_GRAY.len() - 1]
            );
            assert!(device.meshes.is_empty());
        }
    }
}

#[test]
fn loading_present_preserves_gray_prefix_when_red_texture_lookup_fails() {
    let (screen, inputs, _) = present_progress_fixture();
    let frame =
        LoadingScreenFrame::from_inputs(LoadingScreenSource::MafiaLoadSave, &screen, &inputs);
    let reply = Rc::clone(&inputs.progress_reply);
    let mut device = PresentDevice {
        after_first_mesh: Some(Box::new(move || {
            reply.set(Some(Err("red texture refused")))
        })),
        ..Default::default()
    };
    let error = frame
        .present(
            &MenuNativeDrawState::new(0, None, [0, 0]),
            MenuComponentPrecision::Unestablished,
            MenuTextPrecision::Unestablished,
            &mut device,
        )
        .unwrap_err();
    assert!(error.contains("red texture refused"), "{error}");
    assert_eq!(inputs.progress_lookups.get(), 2);
    assert_eq!(device.events, PRESENT_THROUGH_GRAY);
    assert_eq!(device.meshes.len(), 1);
    assert_eq!(
        device.meshes[0].texture.as_ref().unwrap().rgba,
        FIRST_PROGRESS_PIXEL
    );
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
        self.calls
            .push(Call::Present(frame.source, frame.raw_progress()?.to_bits()));
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

fn assert_load_progress_screen_contents(
    actual: &LoadProgressScreen,
    expected: &LoadProgressScreen,
) {
    assert_eq!(actual.title, expected.title);
    assert_eq!(actual.mission_part, expected.mission_part);
    assert_eq!(actual.mission_name, expected.mission_name);
    assert_eq!(
        actual.uses_native_components().unwrap(),
        expected.uses_native_components().unwrap()
    );
    assert_eq!(actual.menu_base_for_test(), expected.menu_base_for_test());
    actual
        .with_components_for_test(|menu| {
            expected
                .with_components_for_test(|expected_menu| {
                    assert_eq!(menu, expected_menu);
                    Ok(())
                })
                .unwrap();
            Ok(())
        })
        .unwrap();
}

#[test]
fn other_messages_return_at_once() {
    let mut callback = LoadSceneCallback::default();
    let mut progress = Some(LoadProgressScreen::for_mission_open());
    let original_menu = progress.as_ref().unwrap().menu_identity_for_test();
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
    let screen = progress.as_ref().unwrap();
    assert_eq!(screen.menu_identity_for_test(), original_menu);
    assert_load_progress_screen_contents(screen, &LoadProgressScreen::for_mission_open());
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
    assert_eq!(screen.component_count(), Ok(definition().items.len()));
    assert_eq!(
        screen.staged_mission_picture(),
        Ok(Some(Some(b"M01.tga".to_vec())))
    );
    screen
        .with_components_for_test(|menu| {
            assert_eq!(menu.len(), definition().items.len());
            assert_eq!(
                device.calls,
                [Call::Present(LoadingScreenSource::MafiaLoadSave, 0)]
            );
            // OnCreate's stores (no title: the title items stay untouched).
            assert_eq!(
                menu.staged_string(LOADPROGRESS_SIDE),
                Some(Some(&b"loadbok.tga"[..]))
            );
            assert_eq!(
                menu.staged_string(LOADPROGRESS_LOGO),
                Some(Some(&b"mafia.tga"[..]))
            );
            assert_eq!(
                menu.staged_string(LOADPROGRESS_MISSION_PICTURE),
                Some(Some(&b"M01.tga"[..]))
            );
            assert_eq!(menu.get_text_id(LOADPROGRESS_MISSION_NAME), 0x5dc);
            for id in [
                LOADPROGRESS_UNKNOWN_1000,
                LOADPROGRESS_UNKNOWN_2000,
                LOADPROGRESS_UNKNOWN_3000,
            ] {
                assert!(!menu.find_component_by_id(id).unwrap().is_visible());
            }
            assert!(menu
                .find_component_by_id(LOADPROGRESS_SIDE)
                .unwrap()
                .is_visible());
            Ok(())
        })
        .unwrap();
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
    own.with_components_for_test(|menu| {
        assert_eq!(menu.len(), definition().items.len());
        Ok(())
    })
    .unwrap();
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
    assert_eq!(
        progress.as_ref().unwrap().raw_progress().unwrap(),
        0.5 * 0.7
    );
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
    let original_menu = progress.as_ref().unwrap().menu_identity_for_test();
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
    let mut created = LoadProgressScreen::for_mission_open();
    created.create(&mut inputs()).unwrap();
    step(
        &mut callback,
        &mut progress,
        CBM_MISSION_PROGRESS,
        1.0,
        LOAD_PROGRESS_END,
        &mut device,
    )
    .unwrap();
    let screen = progress.as_ref().unwrap();
    assert_eq!(screen.menu_identity_for_test(), original_menu);
    assert_load_progress_screen_contents(screen, &created);
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
    progress
        .as_ref()
        .unwrap()
        .with_components_for_test(|menu| {
            assert!(!menu.is_empty());
            Ok(())
        })
        .unwrap();
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
    assert_eq!(
        progress.as_ref().unwrap().raw_progress().unwrap(),
        0.8 * 0.7
    );
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
    screen
        .with_components_for_test(|menu| {
            assert!(menu.is_empty());
            Ok(())
        })
        .unwrap();
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
    screen
        .with_components_for_test(|menu| {
            assert_eq!(
                menu.staged_string(LOADPROGRESS_MISSION_PICTURE),
                Some(Some(&b"M02.tga"[..]))
            );
            assert_eq!(menu.get_text_id(LOADPROGRESS_MISSION_NAME), 0);
            Ok(())
        })
        .unwrap();
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
    screen
        .with_components_for_test(|menu| {
            assert_eq!(menu.get_text_id(0x190), 0x2cf);
            assert_eq!(menu.get_text_id(0x191), 0x2ce);
            assert_eq!(menu.get_text_id(LOADPROGRESS_MISSION_NAME), 7);
            Ok(())
        })
        .unwrap();
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
    screen
        .with_components_for_test(|menu| {
            assert_eq!(menu.get_text_id(LOADPROGRESS_MISSION_NAME), 2);
            Ok(())
        })
        .unwrap();
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
