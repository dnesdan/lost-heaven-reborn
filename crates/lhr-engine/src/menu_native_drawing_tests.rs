use super::*;
use crate::itexture::TextureResources;
use crate::menu::{
    MenuComponentCreateContext, MenuComponentDrawSink, MenuComponentPrimitive, MenuComponentText,
};
use crate::menu_draw_resources::MenuComponentResources;
use crate::native_precision::Pc24PrecisionState;
use lhr_assets::{MenuDefinition, MenuItem};
use std::collections::VecDeque;
use std::sync::Arc;

const PRECISION: MenuComponentPrecision = MenuComponentPrecision::Pc24Nearest;

struct Resources {
    fail_text: Option<u32>,
}

impl MenuComponentResources for Resources {
    fn text(&mut self, id: u32) -> Result<Option<Arc<[u8]>>, String> {
        if self.fail_text == Some(id) {
            Err("fixture text failure".into())
        } else {
            Ok(Some(Arc::from(b"fixture".as_slice())))
        }
    }

    fn create_alpha_texture(&mut self, _name: &[u8]) -> Result<Option<TextureKey>, String> {
        Err("unexpected fixture texture creation".into())
    }

    fn release_texture(&mut self, _key: TextureKey) -> Result<(), String> {
        Err("unexpected fixture texture release".into())
    }
}

fn item(id: u32, kind: [u8; 4], rect: [f32; 4], style: u32) -> MenuItem {
    MenuItem {
        id,
        kind,
        rect,
        text_id: 0,
        style: crate::main_menu::COMPONENT_VISIBLE | style,
    }
}

fn definition(items: Vec<MenuItem>) -> MenuDefinition {
    MenuDefinition {
        version: 1,
        reserved: 0,
        items,
    }
}

fn fixture_definition() -> MenuDefinition {
    definition(vec![
        item(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0),
        item(2, *b"tcer", [1.0, 2.0, 8.0, 6.0], 0x300),
    ])
}

fn create(
    stack: &mut MenuStack,
    definition: &MenuDefinition,
    precision: MenuComponentPrecision,
) -> Result<(), String> {
    stack.create_components_native(
        definition,
        MenuComponentCreateContext {
            screen_scale: [1.0, 1.0],
            left_margin_x: 0.0,
            precision,
        },
        &mut Resources { fail_text: None },
    )
}

fn stack() -> MenuStack {
    let mut stack = MenuStack::new();
    create(&mut stack, &fixture_definition(), PRECISION).unwrap();
    stack
}

fn inactive() -> MenuNativeDrawState {
    MenuNativeDrawState::new(0, None, [0, 0])
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Z(bool),
    Height,
    Width,
    Viewport(MenuNativeViewport),
    Components(MenuNativeMenuIdentity, u32, Option<TextureKey>),
    Cursor([u32; 2]),
}

#[derive(Debug, PartialEq)]
struct Primitive {
    component: u32,
    flags: u32,
    positions: Vec<[u32; 4]>,
    texture: Option<TextureKey>,
}

type PrimitiveCallback = dyn FnMut(&Primitive) -> Result<(), String>;

struct Sink<'a> {
    primitives: &'a mut Vec<Primitive>,
    fail_primitive: Option<usize>,
    on_primitive: &'a mut Option<Box<PrimitiveCallback>>,
}

impl MenuComponentDrawSink for Sink<'_> {
    fn primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String> {
        self.primitives.push(Primitive {
            component: primitive.component_id,
            flags: primitive.draw_flags,
            positions: primitive
                .vertices
                .iter()
                .map(|vertex| vertex.position.map(f32::to_bits))
                .collect(),
            texture: primitive.texture.copied(),
        });
        if let Some(on_primitive) = &mut self.on_primitive {
            on_primitive(self.primitives.last().unwrap())?;
        }
        if self.fail_primitive == Some(self.primitives.len()) {
            Err("fixture primitive failure".into())
        } else {
            Ok(())
        }
    }

    fn text(&mut self, _text: MenuComponentText<'_>) -> Result<(), String> {
        Err("unexpected fixture text draw".into())
    }
}

type EventCallback = dyn FnMut(&Event);

struct Backend {
    events: Vec<Event>,
    widths: VecDeque<u32>,
    heights: VecDeque<u32>,
    primitives: Vec<Primitive>,
    fail_call: Option<usize>,
    fail_primitive: Option<usize>,
    on_event: Option<Box<EventCallback>>,
    on_primitive: Option<Box<PrimitiveCallback>>,
}

impl Backend {
    fn new(viewports: usize) -> Self {
        Self {
            events: Vec::new(),
            widths: vec![800; viewports * 2].into(),
            heights: vec![600; viewports * 2].into(),
            primitives: Vec::new(),
            fail_call: None,
            fail_primitive: None,
            on_event: None,
            on_primitive: None,
        }
    }

    fn record(&mut self, event: Event) -> Result<(), String> {
        self.events.push(event.clone());
        if let Some(on_event) = &mut self.on_event {
            on_event(&event);
        }
        if self.fail_call == Some(self.events.len()) {
            Err("fixture backend failure".into())
        } else {
            Ok(())
        }
    }
}

impl MenuNativeDrawBackend for Backend {
    fn set_z_buffer(&mut self, enabled: bool) -> Result<(), String> {
        self.record(Event::Z(enabled))
    }

    fn screen_height(&mut self) -> Result<u32, String> {
        self.record(Event::Height)?;
        self.heights
            .pop_front()
            .ok_or_else(|| "unexpected fixture height read".into())
    }

    fn screen_width(&mut self) -> Result<u32, String> {
        self.record(Event::Width)?;
        self.widths
            .pop_front()
            .ok_or_else(|| "unexpected fixture width read".into())
    }

    fn set_viewport(&mut self, viewport: MenuNativeViewport) -> Result<(), String> {
        self.record(Event::Viewport(viewport))
    }

    fn draw_components(
        &mut self,
        menu: &MenuNativeMenu,
        draw_flags: u32,
        progress_texture: Option<&TextureKey>,
    ) -> Result<(), String> {
        self.record(Event::Components(
            menu.identity(),
            draw_flags,
            progress_texture.copied(),
        ))?;
        menu.draw_components_native(
            draw_flags,
            progress_texture,
            &mut Sink {
                primitives: &mut self.primitives,
                fail_primitive: self.fail_primitive,
                on_primitive: &mut self.on_primitive,
            },
        )
    }

    fn draw_cursor(&mut self, x: f32, y: f32) -> Result<(), String> {
        self.record(Event::Cursor([x.to_bits(), y.to_bits()]))
    }
}

fn draw(
    menu: &MenuNativeMenu,
    globals: &MenuNativeDrawState,
    flags: u32,
    backend: &mut Backend,
) -> Result<(), MenuNativeDrawError> {
    draw_menu_native(menu, globals, flags, None, PRECISION, backend)
}

fn viewport(width: u32, height: u32) -> MenuNativeViewport {
    MenuNativeViewport {
        x: 0,
        y: 0,
        width,
        height,
        min_z: 0.0,
        max_z: 1.0,
    }
}

fn viewport_events(width: u32, height: u32) -> Vec<Event> {
    vec![
        Event::Height,
        Event::Width,
        Event::Width,
        Event::Height,
        Event::Viewport(viewport(width, height)),
        Event::Z(false),
    ]
}

#[test]
fn handles_alias_one_stack_and_opaque_allocation_identity() {
    let menu = MenuNativeMenu::new(stack(), None, 17);
    let alias = menu.clone();
    let other = MenuNativeMenu::new(stack(), None, 17);
    assert_eq!(menu.identity(), alias.identity());
    assert_ne!(menu.identity(), other.identity());
    assert_eq!(
        menu.identity().upgrade().unwrap().identity(),
        menu.identity()
    );
    let key = menu.with_components(MenuStack::native_root_key).unwrap();
    assert_eq!(
        alias.with_components(MenuStack::native_root_key).unwrap(),
        key
    );
    alias
        .with_components_mut(|stack| {
            stack.set_hidden(2, false);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        menu.with_components(|stack| Ok(
            stack.find_component_by_id(2).unwrap().flags & crate::main_menu::COMPONENT_VISIBLE
        ))
        .unwrap(),
        0
    );
    alias.set_flags(91);
    alias.set_parent(Some(other.identity()));
    assert_eq!(menu.flags(), 91);
    assert_eq!(menu.parent(), Some(other.identity()));
    menu.set_parent(None);
    assert_eq!(alias.parent(), None);
}

#[test]
fn weak_identity_does_not_keep_the_menu_allocation_alive() {
    let identity = {
        let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
        menu.identity()
    };
    assert!(identity.upgrade().is_none());
    assert_eq!(identity, identity.clone());
}

#[test]
fn scopes_report_conflicts_and_release_after_fallible_operations() {
    let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
    menu.with_components_mut(|_| {
        assert_eq!(
            menu.with_components(|_| Ok(())),
            Err(MenuNativeMenuAccessError::ComponentsBorrowed)
        );
        assert_eq!(
            menu.with_components_mut(|_| Ok(())),
            Err(MenuNativeMenuAccessError::ComponentsBorrowed)
        );
        Ok(())
    })
    .unwrap();
    let failure = menu.with_components_mut::<()>(|_| Err("scoped failure".into()));
    assert_eq!(
        failure,
        Err(MenuNativeMenuAccessError::ComponentOperation(
            "scoped failure".into()
        ))
    );
    menu.with_components_mut(|_| Ok(())).unwrap();
    menu.with_components(|_| {
        menu.with_components(|_| Ok(())).unwrap();
        assert_eq!(
            menu.with_components_mut(|_| Ok(())),
            Err(MenuNativeMenuAccessError::ComponentsBorrowed)
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn global_handles_share_live_flags_identity_and_signed_cursor() {
    let globals = inactive();
    let alias = globals.clone();
    let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
    alias.set_blink_time(u32::MAX);
    alias.set_active_menu_window(Some(menu.identity()));
    alias.set_cursor([i32::MIN, i32::MAX]);
    assert_eq!(globals.blink_time(), u32::MAX);
    assert_eq!(globals.active_menu_window(), Some(menu.identity()));
    assert_eq!(
        [globals.cursor_x(), globals.cursor_y()],
        [i32::MIN, i32::MAX]
    );
    globals.set_cursor_x(-3);
    globals.set_cursor_y(7);
    globals.set_active_menu_window(None);
    assert_eq!([alias.cursor_x(), alias.cursor_y()], [-3, 7]);
    assert_eq!(alias.active_menu_window(), None);
}

#[test]
fn null_root_draw_reaches_z_on_without_numeric_policy() {
    let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
    let mut backend = Backend::new(0);
    draw_menu_native(
        &menu,
        &inactive(),
        0,
        None,
        MenuComponentPrecision::Unestablished,
        &mut backend,
    )
    .unwrap();
    assert_eq!(backend.events, [Event::Z(false), Event::Z(true)]);
}

#[test]
fn nonempty_legacy_root_is_not_guessed_from_component_count() {
    let menu = MenuNativeMenu::new(
        MenuStack::create_from_definition(&fixture_definition()),
        None,
        0,
    );
    let mut backend = Backend::new(0);
    let error = draw(&menu, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(error.stage, MenuNativeDrawStage::Root);
    assert_eq!(error.menu, menu.identity());
    assert_eq!(
        error.failure,
        MenuNativeDrawFailure::MenuAccess(MenuNativeMenuAccessError::ComponentOperation(
            "uncomposed menu has no native component root".into()
        ))
    );
    assert_eq!(backend.events, [Event::Z(false)]);
}

fn partial_stack(root_text: u32, child_text: u32) -> MenuStack {
    let mut definition = fixture_definition();
    definition.items[0].text_id = root_text;
    definition.items[1].text_id = child_text;
    let mut stack = MenuStack::new();
    assert!(stack
        .create_components_native(
            &definition,
            MenuComponentCreateContext {
                screen_scale: [1.0, 1.0],
                left_margin_x: 0.0,
                precision: PRECISION,
            },
            &mut Resources { fail_text: Some(7) }
        )
        .is_err());
    stack
}

#[test]
fn interrupted_root_constructor_keeps_null_root_branch() {
    let stack = partial_stack(7, 0);
    assert_eq!(stack.len(), 1);
    let menu = MenuNativeMenu::new(stack, None, 0);
    let mut backend = Backend::new(0);
    draw(&menu, &inactive(), 0, &mut backend).unwrap();
    assert_eq!(backend.events, [Event::Z(false), Event::Z(true)]);
}

#[test]
fn assigned_partial_root_reaches_viewport_before_component_stop() {
    let stack = partial_stack(0, 7);
    assert!(stack.native_root_key().unwrap().is_some());
    let menu = MenuNativeMenu::new(stack, None, 0);
    let mut backend = Backend::new(1);
    let error = draw(&menu, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(error.stage, MenuNativeDrawStage::Components);
    let mut expected = vec![Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.push(Event::Components(menu.identity(), 0, None));
    assert_eq!(backend.events, expected);
}

#[test]
fn native_components_follow_full_viewport_and_second_z_off() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let mut backend = Backend::new(1);
    draw(&menu, &inactive(), MENUDRAW_ACTIVE_MENU, &mut backend).unwrap();
    let mut expected = vec![Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.extend([
        Event::Components(menu.identity(), MENUDRAW_ACTIVE_MENU, None),
        Event::Z(true),
    ]);
    assert_eq!(backend.events, expected);
    assert_eq!(backend.primitives.len(), 1);
    assert_eq!(backend.primitives[0].component, 2);
    assert_eq!(
        backend.primitives[0].positions[0][..2],
        [11.0_f32.to_bits(), 22.0_f32.to_bits()]
    );
}

#[test]
fn parent_draw_only_clears_active_bit_then_child_reestablishes_viewport_z_off() {
    let parent = MenuNativeMenu::new(stack(), None, 0);
    let child = MenuNativeMenu::new(stack(), Some(parent.identity()), MENUF_DRAW_PARENT);
    let flags = MENUDRAW_ACTIVE_MENU | MENUDRAW_CURSOR | 0x8000_1234;
    let mut backend = Backend::new(2);
    draw(&child, &inactive(), flags, &mut backend).unwrap();
    let mut expected = vec![Event::Z(false), Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.extend([
        Event::Components(parent.identity(), flags & !MENUDRAW_ACTIVE_MENU, None),
        Event::Z(true),
    ]);
    expected.extend(viewport_events(800, 600));
    expected.extend([
        Event::Components(child.identity(), flags, None),
        Event::Z(true),
    ]);
    assert_eq!(backend.events, expected);
    assert_eq!(child.flags(), MENUF_DRAW_PARENT);
    assert_eq!(parent.flags(), 0);
}

#[test]
fn unused_parent_branch_does_not_upgrade_released_identity() {
    let parent = MenuNativeMenu::new(MenuStack::new(), None, 0);
    let child = MenuNativeMenu::new(MenuStack::new(), Some(parent.identity()), 0);
    drop(parent);
    let mut backend = Backend::new(0);
    draw(&child, &inactive(), 0, &mut backend).unwrap();
    assert_eq!(backend.events, [Event::Z(false), Event::Z(true)]);
    child.set_parent(None);
    child.set_flags(MENUF_DRAW_PARENT);
    backend.events.clear();
    draw(&child, &inactive(), 0, &mut backend).unwrap();
    assert_eq!(backend.events, [Event::Z(false), Event::Z(true)]);
}

#[test]
fn reached_released_parent_stops_after_initial_z_off() {
    let parent = MenuNativeMenu::new(MenuStack::new(), None, 0);
    let child = MenuNativeMenu::new(MenuStack::new(), Some(parent.identity()), MENUF_DRAW_PARENT);
    drop(parent);
    let mut backend = Backend::new(0);
    let error = draw(&child, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(error.failure, MenuNativeDrawFailure::ReleasedParent);
    assert_eq!(error.stage, MenuNativeDrawStage::Parent);
    assert_eq!(error.menu, child.identity());
    assert_eq!(backend.events, [Event::Z(false)]);
}

#[test]
fn reached_self_and_two_allocation_cycles_stop_without_restore() {
    let first = MenuNativeMenu::new(MenuStack::new(), None, MENUF_DRAW_PARENT);
    first.set_parent(Some(first.identity()));
    let mut backend = Backend::new(0);
    let error = draw(&first, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(error.failure, MenuNativeDrawFailure::ParentCycle);
    assert_eq!(backend.events, [Event::Z(false)]);
    let second = MenuNativeMenu::new(MenuStack::new(), Some(first.identity()), MENUF_DRAW_PARENT);
    first.set_parent(Some(second.identity()));
    backend.events.clear();
    let error = draw(&first, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(error.menu, first.identity());
    assert_eq!(error.failure, MenuNativeDrawFailure::ParentCycle);
    assert_eq!(backend.events, [Event::Z(false), Event::Z(false)]);
}

#[test]
fn blink_comparison_is_unsigned_strictly_above_700() {
    for blink in [0, 700, 701, u32::MAX] {
        let menu = MenuNativeMenu::new(stack(), None, 0);
        let mut backend = Backend::new(1);
        let flags = MENUDRAW_CURSOR | MENUDRAW_ACTIVE_MENU | 0xa000_3210;
        draw(
            &menu,
            &MenuNativeDrawState::new(blink, None, [0, 0]),
            flags,
            &mut backend,
        )
        .unwrap();
        let expected = if blink > 700 {
            flags & !MENUDRAW_CURSOR
        } else {
            flags
        };
        assert!(backend
            .events
            .contains(&Event::Components(menu.identity(), expected, None)));
    }
}

#[test]
fn blink_parent_and_flags_are_read_after_initial_z_callback() {
    let parent = MenuNativeMenu::new(stack(), None, 0);
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let globals = inactive();
    let mut backend = Backend::new(2);
    let live_menu = menu.clone();
    let live_globals = globals.clone();
    let parent_identity = parent.identity();
    let mut first = true;
    backend.on_event = Some(Box::new(move |event| {
        if first && *event == Event::Z(false) {
            first = false;
            live_menu.set_parent(Some(parent_identity.clone()));
            live_menu.set_flags(MENUF_DRAW_PARENT);
            live_globals.set_blink_time(701);
        }
    }));
    draw(
        &menu,
        &globals,
        MENUDRAW_CURSOR | MENUDRAW_ACTIVE_MENU,
        &mut backend,
    )
    .unwrap();
    let components: Vec<_> = backend
        .events
        .iter()
        .filter(|event| matches!(event, Event::Components(..)))
        .cloned()
        .collect();
    assert_eq!(
        components,
        [
            Event::Components(parent.identity(), 0, None),
            Event::Components(menu.identity(), MENUDRAW_ACTIVE_MENU, None)
        ]
    );
}

#[test]
fn viewport_clamps_with_fresh_reads_not_cached_minimums() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let mut backend = Backend::new(0);
    backend.widths = [900, 800, 1100].into();
    backend.heights = [700, 600, 1000].into();
    draw(&menu, &inactive(), 0, &mut backend).unwrap();
    assert_eq!(
        backend.events,
        [
            Event::Z(false),
            Event::Height,
            Event::Width,
            Event::Width,
            Event::Width,
            Event::Height,
            Event::Height,
            Event::Viewport(viewport(1100, 1000)),
            Event::Z(false),
            Event::Components(menu.identity(), 0, None),
            Event::Z(true),
        ]
    );
}

#[test]
fn viewport_dimensions_are_unsigned_including_high_bit_and_zero() {
    for (widths, heights, expected) in [
        (
            [0x8000_0000, 1, u32::MAX],
            [0x8000_0001, 2, u32::MAX],
            viewport(u32::MAX, u32::MAX),
        ),
        (
            [0x7fff_ffff, 0x8000_0000, 9],
            [0x7fff_ffff, 0x8000_0000, 9],
            viewport(0x7fff_ffff, 0x7fff_ffff),
        ),
        ([0, 0, 9], [0, 0, 9], viewport(0, 0)),
    ] {
        let menu = MenuNativeMenu::new(stack(), None, 0);
        let mut backend = Backend::new(0);
        backend.widths = widths.into();
        backend.heights = heights.into();
        draw(&menu, &inactive(), 0, &mut backend).unwrap();
        assert!(backend.events.contains(&Event::Viewport(expected)));
    }
}

#[test]
fn active_allocation_draws_cursor_without_any_draw_flag_gate() {
    for flags in [0, MENUDRAW_ACTIVE_MENU, MENUDRAW_CURSOR, u32::MAX] {
        let menu = MenuNativeMenu::new(stack(), None, 0);
        let globals = MenuNativeDrawState::new(701, Some(menu.identity()), [-3, 7]);
        let mut backend = Backend::new(2);
        draw(&menu, &globals, flags, &mut backend).unwrap();
        let mut expected = vec![Event::Z(false)];
        expected.extend(viewport_events(800, 600));
        expected.push(Event::Components(
            menu.identity(),
            flags & !MENUDRAW_CURSOR,
            None,
        ));
        expected.extend(viewport_events(800, 600));
        expected.extend([
            Event::Cursor([(-3.0_f32).to_bits(), 7.0_f32.to_bits()]),
            Event::Z(true),
        ]);
        assert_eq!(backend.events, expected);
    }
}

#[test]
fn distinct_or_released_active_allocation_does_not_draw_cursor() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let other = MenuNativeMenu::new(stack(), None, 0);
    let globals = MenuNativeDrawState::new(0, Some(other.identity()), [1, 2]);
    for release in [false, true] {
        let mut backend = Backend::new(1);
        if release {
            let temporary = MenuNativeMenu::new(stack(), None, 0);
            globals.set_active_menu_window(Some(temporary.identity()));
        }
        draw(&menu, &globals, u32::MAX, &mut backend).unwrap();
        assert!(!backend
            .events
            .iter()
            .any(|event| matches!(event, Event::Cursor(..))));
        assert_eq!(backend.events.last(), Some(&Event::Z(true)));
    }
}

#[test]
fn active_identity_is_read_after_component_callback() {
    for initially_active in [false, true] {
        let menu = MenuNativeMenu::new(stack(), None, 0);
        let globals =
            MenuNativeDrawState::new(0, initially_active.then(|| menu.identity()), [3, 4]);
        let live_globals = globals.clone();
        let identity = menu.identity();
        let mut backend = Backend::new(2);
        backend.on_event = Some(Box::new(move |event| {
            if matches!(event, Event::Components(..)) {
                live_globals.set_active_menu_window((!initially_active).then(|| identity.clone()));
            }
        }));
        draw(&menu, &globals, 0, &mut backend).unwrap();
        assert_eq!(
            backend
                .events
                .iter()
                .any(|event| matches!(event, Event::Cursor(..))),
            !initially_active
        );
    }
}

#[test]
fn cursor_branch_stays_reached_but_coordinates_reload_after_viewport() {
    let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
    let globals = MenuNativeDrawState::new(0, Some(menu.identity()), [1, 2]);
    let live_globals = globals.clone();
    let mut backend = Backend::new(1);
    backend.on_event = Some(Box::new(move |event| {
        if matches!(event, Event::Viewport(..)) {
            live_globals.set_active_menu_window(None);
            live_globals.set_cursor([-7, 13]);
        }
    }));
    draw(&menu, &globals, 0, &mut backend).unwrap();
    let mut expected = vec![Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.extend([
        Event::Cursor([(-7.0_f32).to_bits(), 13.0_f32.to_bits()]),
        Event::Z(true),
    ]);
    assert_eq!(backend.events, expected);
    assert_eq!(globals.active_menu_window(), None);
}

#[test]
fn active_parent_cursor_is_independent_of_active_draw_bit() {
    let parent = MenuNativeMenu::new(stack(), None, 0);
    let child = MenuNativeMenu::new(stack(), Some(parent.identity()), MENUF_DRAW_PARENT);
    let globals = MenuNativeDrawState::new(0, Some(parent.identity()), [5, 6]);
    let live_globals = globals.clone();
    let child_identity = child.identity();
    let mut backend = Backend::new(4);
    backend.on_event = Some(Box::new(move |event| {
        if matches!(event, Event::Cursor(..)) {
            live_globals.set_active_menu_window(Some(child_identity.clone()));
        }
    }));
    draw(&child, &globals, MENUDRAW_ACTIVE_MENU, &mut backend).unwrap();
    let components: Vec<_> = backend
        .events
        .iter()
        .filter(|event| matches!(event, Event::Components(..)))
        .cloned()
        .collect();
    assert_eq!(
        components,
        [
            Event::Components(parent.identity(), 0, None),
            Event::Components(child.identity(), MENUDRAW_ACTIVE_MENU, None)
        ]
    );
    assert_eq!(
        backend
            .events
            .iter()
            .filter(|event| matches!(event, Event::Cursor(..)))
            .count(),
        2
    );
    assert_eq!(backend.events.last(), Some(&Event::Z(true)));
}

#[test]
fn cursor_signed_integer_conversions_round_to_nearest_even_binary32() {
    for (cursor, expected) in [
        ([0, -1], [0, 0xbf80_0000]),
        ([i32::MIN, i32::MAX], [0xcf00_0000, 0x4f00_0000]),
        ([16_777_217, 16_777_219], [0x4b80_0000, 0x4b80_0002]),
        ([-16_777_217, -16_777_219], [0xcb80_0000, 0xcb80_0002]),
    ] {
        let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
        let globals = MenuNativeDrawState::new(0, Some(menu.identity()), cursor);
        let mut backend = Backend::new(1);
        draw(&menu, &globals, 0, &mut backend).unwrap();
        assert_eq!(backend.events[7], Event::Cursor(expected));
    }
}

#[test]
fn cursor_conversion_updates_only_the_reached_precision_prefix() {
    for (cursor, initial, expected) in [
        ([1, -1], false, false),
        ([1, -1], true, true),
        ([16_777_217, 0], false, true),
        ([0, 16_777_217], false, true),
    ] {
        let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
        let globals = MenuNativeDrawState::new(0, Some(menu.identity()), cursor);
        let mut backend = Backend::new(1);
        let mut state = Pc24PrecisionState::new(initial);
        state
            .run(|| draw(&menu, &globals, 0, &mut backend))
            .unwrap();
        assert_eq!(state.precision_raised(), expected);
    }
    let menu = MenuNativeMenu::new(MenuStack::new(), None, 0);
    let globals = MenuNativeDrawState::new(0, Some(menu.identity()), [16_777_217, 16_777_219]);
    let mut backend = Backend::new(1);
    backend.fail_call = Some(7);
    let mut state = Pc24PrecisionState::new(false);
    let error = state
        .run(|| draw(&menu, &globals, 0, &mut backend))
        .unwrap_err();
    assert_eq!(error.stage, MenuNativeDrawStage::ViewportZOff);
    assert!(!state.precision_raised());
    assert_eq!(backend.events.last(), Some(&Event::Z(false)));
}

#[test]
fn unknown_top_level_precision_stops_at_reached_cursor_after_components() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let globals = MenuNativeDrawState::new(0, Some(menu.identity()), [0, 0]);
    let mut backend = Backend::new(2);
    let error = draw_menu_native(
        &menu,
        &globals,
        0,
        None,
        MenuComponentPrecision::Unestablished,
        &mut backend,
    )
    .unwrap_err();
    assert_eq!(error.menu, menu.identity());
    assert_eq!(error.stage, MenuNativeDrawStage::CursorY);
    assert_eq!(error.failure, MenuNativeDrawFailure::PrecisionUnestablished);
    let mut expected = vec![Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.push(Event::Components(menu.identity(), 0, None));
    expected.extend(viewport_events(800, 600));
    assert_eq!(backend.events, expected);
    assert_eq!(backend.primitives.len(), 1);
    assert_eq!(backend.events.last(), Some(&Event::Z(false)));
}

#[test]
fn top_level_precision_does_not_replace_component_payload_policy() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let mut backend = Backend::new(1);
    draw_menu_native(
        &menu,
        &inactive(),
        0,
        None,
        MenuComponentPrecision::Unestablished,
        &mut backend,
    )
    .unwrap();
    assert_eq!(backend.primitives.len(), 1);
    assert_eq!(
        backend.primitives[0].positions[0][..2],
        [11.0_f32.to_bits(), 22.0_f32.to_bits()]
    );
    assert_eq!(backend.events.last(), Some(&Event::Z(true)));
}

#[test]
fn child_root_is_read_after_parent_callback_populates_the_live_stack() {
    let parent = MenuNativeMenu::new(stack(), None, 0);
    let child = MenuNativeMenu::new(MenuStack::new(), Some(parent.identity()), MENUF_DRAW_PARENT);
    let live_child = child.clone();
    let parent_identity = parent.identity();
    let mut backend = Backend::new(2);
    backend.on_event = Some(Box::new(move |event| {
        if matches!(event, Event::Components(identity, ..) if *identity == parent_identity) {
            live_child
                .with_components_mut(|stack| create(stack, &fixture_definition(), PRECISION))
                .unwrap();
        }
    }));
    draw(&child, &inactive(), 0, &mut backend).unwrap();
    assert!(child
        .with_components(MenuStack::native_root_key)
        .unwrap()
        .is_some());
    let components: Vec<_> = backend
        .events
        .iter()
        .filter(|event| matches!(event, Event::Components(..)))
        .cloned()
        .collect();
    assert_eq!(
        components,
        [
            Event::Components(parent.identity(), 0, None),
            Event::Components(child.identity(), 0, None)
        ]
    );
    assert_eq!(backend.primitives.len(), 2);
}

#[test]
fn viewport_callback_can_replace_the_actual_root_before_component_draw() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let live_menu = menu.clone();
    let mut backend = Backend::new(1);
    backend.on_event = Some(Box::new(move |event| {
        if matches!(event, Event::Viewport(..)) {
            live_menu
                .with_components_mut(|stack| {
                    stack.destroy_native(&mut Resources { fail_text: None })?;
                    create(
                        stack,
                        &definition(vec![
                            item(11, *b"cniw", [30.0, 40.0, 100.0, 80.0], 0),
                            item(12, *b"tcer", [3.0, 4.0, 8.0, 6.0], 0x300),
                        ]),
                        PRECISION,
                    )
                })
                .unwrap();
        }
    }));
    draw(&menu, &inactive(), 0, &mut backend).unwrap();
    assert_eq!(backend.primitives.len(), 1);
    assert_eq!(backend.primitives[0].component, 12);
    assert_eq!(
        backend.primitives[0].positions[0][..2],
        [33.0_f32.to_bits(), 44.0_f32.to_bits()]
    );
    assert!(menu
        .with_components(|stack| Ok(stack.find_component_by_id(2).is_none()))
        .unwrap());
}

#[test]
fn every_host_callback_can_enter_both_live_menu_stacks() {
    let parent = MenuNativeMenu::new(stack(), None, 0);
    let child = MenuNativeMenu::new(stack(), Some(parent.identity()), MENUF_DRAW_PARENT);
    let globals = MenuNativeDrawState::new(0, Some(child.identity()), [1, 2]);
    let live_parent = parent.clone();
    let live_child = child.clone();
    let mut backend = Backend::new(3);
    backend.on_event = Some(Box::new(move |_| {
        live_parent
            .with_components_mut(|stack| {
                assert!(stack.native_root_key()?.is_some());
                Ok(())
            })
            .unwrap();
        live_child
            .with_components_mut(|stack| {
                assert!(stack.native_root_key()?.is_some());
                Ok(())
            })
            .unwrap();
    }));
    let live_parent = parent.clone();
    let live_child = child.clone();
    let primitive_callbacks = std::rc::Rc::new(std::cell::Cell::new(0));
    let reached = primitive_callbacks.clone();
    backend.on_primitive = Some(Box::new(move |_| {
        for menu in [&live_parent, &live_child] {
            menu.with_components(|stack| {
                assert!(stack.native_root_key()?.is_some());
                Ok(())
            })
            .map_err(|error| error.to_string())?;
            menu.with_components_mut(|stack| {
                assert!(stack.native_root_key()?.is_some());
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        }
        reached.set(reached.get() + 1);
        Ok(())
    }));
    draw(&child, &globals, MENUDRAW_ACTIVE_MENU, &mut backend).unwrap();
    assert_eq!(backend.primitives.len(), 2);
    assert_eq!(primitive_callbacks.get(), 2);
    assert_eq!(backend.events.last(), Some(&Event::Z(true)));
}

#[test]
fn gray_callback_reads_after_move_and_mutates_live_progress_and_children() {
    let mut components = MenuStack::new();
    create(
        &mut components,
        &definition(vec![
            item(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0),
            item(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0),
            item(3, *b"tcer", [3.0, 4.0, 8.0, 6.0], 0x300),
        ]),
        PRECISION,
    )
    .unwrap();
    components.set_float(2, 0.25).unwrap();
    components.set_hidden(3, false);
    let menu = MenuNativeMenu::new(components, None, 0);
    let live_menu = menu.clone();
    let callbacks = std::rc::Rc::new(std::cell::Cell::new(0));
    let reached = callbacks.clone();
    let mut backend = Backend::new(1);
    backend.on_primitive = Some(Box::new(move |primitive| {
        live_menu
            .with_components(|stack| {
                let hidden = stack.find_component_by_id(3).unwrap();
                assert_eq!(hidden.native_geometry().unwrap().position, [13.0, 24.0]);
                Ok(())
            })
            .map_err(|error| error.to_string())?;
        if reached.get() == 0 {
            assert_eq!(primitive.component, 2);
            live_menu
                .with_components_mut(|stack| {
                    assert_eq!(stack.progress_value(2), Some(0.25));
                    stack.set_float(2, 0.75)?;
                    stack.set_hidden(3, true);
                    Ok(())
                })
                .map_err(|error| error.to_string())?;
        }
        reached.set(reached.get() + 1);
        Ok(())
    }));
    draw(&menu, &inactive(), 0, &mut backend).unwrap();
    assert_eq!(callbacks.get(), 3);
    assert_eq!(
        backend
            .primitives
            .iter()
            .map(|primitive| primitive.component)
            .collect::<Vec<_>>(),
        [2, 2, 3]
    );
    for (primitive, right) in backend.primitives[..2].iter().zip([19.0_f32, 17.0_f32]) {
        assert_eq!(
            primitive.positions.iter().map(|position| position[0]).max(),
            Some(right.to_bits())
        );
    }
    menu.with_components(|stack| {
        assert_eq!(stack.progress_value(2), Some(0.75));
        assert_ne!(
            stack.find_component_by_id(3).unwrap().flags & crate::main_menu::COMPONENT_VISIBLE,
            0
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(backend.events.last(), Some(&Event::Z(true)));
}

#[test]
fn gray_callback_error_retains_mutation_and_prefix_without_red_or_z_restore() {
    let mut components = MenuStack::new();
    create(
        &mut components,
        &definition(vec![
            item(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0),
            item(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0),
            item(3, *b"tcer", [3.0, 4.0, 8.0, 6.0], 0x300),
        ]),
        PRECISION,
    )
    .unwrap();
    components.set_float(2, 0.25).unwrap();
    let menu = MenuNativeMenu::new(components, None, 0);
    let live_menu = menu.clone();
    let mut backend = Backend::new(1);
    backend.on_primitive = Some(Box::new(move |_| {
        live_menu
            .with_components_mut(|stack| stack.set_float(2, 0.75))
            .map_err(|error| error.to_string())?;
        Err("fixture gray callback failure".into())
    }));
    let error = draw(&menu, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(error.menu, menu.identity());
    assert_eq!(error.stage, MenuNativeDrawStage::Components);
    assert_eq!(
        error.failure,
        MenuNativeDrawFailure::Backend("fixture gray callback failure".into())
    );
    assert_eq!(backend.primitives.len(), 1);
    assert_eq!(backend.primitives[0].component, 2);
    let mut expected = vec![Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.push(Event::Components(menu.identity(), 0, None));
    assert_eq!(backend.events, expected);
    menu.with_components(|stack| {
        assert_eq!(stack.progress_value(2), Some(0.75));
        Ok(())
    })
    .unwrap();
}

#[test]
fn gray_callback_destroy_recreate_rejects_old_cursor_without_restoring_z() {
    let mut components = MenuStack::new();
    create(
        &mut components,
        &definition(vec![
            item(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0),
            item(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0),
            item(3, *b"tcer", [3.0, 4.0, 8.0, 6.0], 0x300),
        ]),
        PRECISION,
    )
    .unwrap();
    components.set_float(2, 0.25).unwrap();
    let menu = MenuNativeMenu::new(components, None, 0);
    let identity = menu.identity();
    let original_root = menu.with_components(MenuStack::native_root_key).unwrap();
    let live_menu = menu.clone();
    let mut backend = Backend::new(1);
    backend.on_primitive = Some(Box::new(move |primitive| {
        assert_eq!(primitive.component, 2);
        live_menu
            .with_components_mut(|stack| {
                stack.destroy_native(&mut Resources { fail_text: None })?;
                create(stack, &fixture_definition(), PRECISION)?;
                assert_ne!(stack.native_root_key()?, original_root);
                Ok(())
            })
            .map_err(|error| error.to_string())
    }));
    let error = draw(&menu, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(menu.identity(), identity);
    assert_eq!(error.menu, identity);
    assert_eq!(error.stage, MenuNativeDrawStage::Components);
    assert_eq!(
        error.failure,
        MenuNativeDrawFailure::Backend(
            "native component draw traversal has a stale menu owner".into()
        )
    );
    assert_eq!(backend.primitives.len(), 1);
    assert_eq!(backend.primitives[0].component, 2);
    let mut expected = vec![Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.push(Event::Components(menu.identity(), 0, None));
    assert_eq!(backend.events, expected);
    menu.with_components(|stack| {
        assert_ne!(stack.native_root_key()?, original_root);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.progress_value(2), None);
        Ok(())
    })
    .unwrap();
    let mut fresh_backend = Backend::new(1);
    draw(&menu, &inactive(), 0, &mut fresh_backend).unwrap();
    assert_eq!(fresh_backend.primitives.len(), 1);
    assert_eq!(fresh_backend.primitives[0].component, 2);
    assert_eq!(fresh_backend.events.last(), Some(&Event::Z(true)));
}

#[test]
fn externally_borrowed_components_stop_at_the_actual_root_read() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let mut backend = Backend::new(0);
    menu.with_components_mut(|_| {
        let error = draw(&menu, &inactive(), 0, &mut backend).unwrap_err();
        assert_eq!(error.menu, menu.identity());
        assert_eq!(error.stage, MenuNativeDrawStage::Root);
        assert_eq!(
            error.failure,
            MenuNativeDrawFailure::MenuAccess(MenuNativeMenuAccessError::ComponentsBorrowed)
        );
        Ok(())
    })
    .unwrap();
    assert_eq!(backend.events, [Event::Z(false)]);
    menu.with_components_mut(|_| Ok(())).unwrap();
}

#[test]
fn every_backend_failure_retains_exact_reached_prefix_without_z_restore() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    let globals = MenuNativeDrawState::new(0, Some(menu.identity()), [1, 2]);
    let mut baseline = Backend::new(2);
    draw(&menu, &globals, 0, &mut baseline).unwrap();
    let stages = [
        MenuNativeDrawStage::InitialZOff,
        MenuNativeDrawStage::ViewportHeight,
        MenuNativeDrawStage::ViewportWidth,
        MenuNativeDrawStage::ViewportWidthLimit,
        MenuNativeDrawStage::ViewportHeightLimit,
        MenuNativeDrawStage::SetViewport,
        MenuNativeDrawStage::ViewportZOff,
        MenuNativeDrawStage::Components,
        MenuNativeDrawStage::ViewportHeight,
        MenuNativeDrawStage::ViewportWidth,
        MenuNativeDrawStage::ViewportWidthLimit,
        MenuNativeDrawStage::ViewportHeightLimit,
        MenuNativeDrawStage::SetViewport,
        MenuNativeDrawStage::ViewportZOff,
        MenuNativeDrawStage::Cursor,
        MenuNativeDrawStage::FinalZOn,
    ];
    assert_eq!(baseline.events.len(), stages.len());
    for (index, stage) in stages.into_iter().enumerate() {
        let mut backend = Backend::new(2);
        backend.fail_call = Some(index + 1);
        let error = draw(&menu, &globals, 0, &mut backend).unwrap_err();
        assert_eq!(error.menu, menu.identity());
        assert_eq!(error.stage, stage);
        assert_eq!(
            error.failure,
            MenuNativeDrawFailure::Backend("fixture backend failure".into())
        );
        assert_eq!(
            backend.events,
            baseline.events[..=index],
            "failed boundary {}",
            index + 1
        );
        assert_eq!(backend.primitives.len(), usize::from(index >= 8));
    }
}

#[test]
fn conditional_viewport_rereads_have_distinct_error_stages() {
    let menu = MenuNativeMenu::new(stack(), None, 0);
    for (fail_call, stage) in [
        (5, MenuNativeDrawStage::ViewportWidthClamp),
        (7, MenuNativeDrawStage::ViewportHeightClamp),
    ] {
        let mut backend = Backend::new(0);
        backend.widths = [900, 800, 700].into();
        backend.heights = [700, 600, 500].into();
        backend.fail_call = Some(fail_call);
        let error = draw(&menu, &inactive(), 0, &mut backend).unwrap_err();
        assert_eq!(error.stage, stage);
        assert_eq!(backend.events.len(), fail_call);
        assert!(!backend.events.contains(&Event::Z(true)));
        assert!(backend.primitives.is_empty());
    }
}

#[test]
fn parent_failure_keeps_parent_identity_and_does_not_resume_the_child() {
    let parent = MenuNativeMenu::new(stack(), None, 0);
    let child = MenuNativeMenu::new(stack(), Some(parent.identity()), MENUF_DRAW_PARENT);
    let mut backend = Backend::new(1);
    backend.fail_call = Some(9);
    let error = draw(&child, &inactive(), MENUDRAW_ACTIVE_MENU, &mut backend).unwrap_err();
    assert_eq!(error.menu, parent.identity());
    assert_eq!(error.stage, MenuNativeDrawStage::Components);
    let mut expected = vec![Event::Z(false), Event::Z(false)];
    expected.extend(viewport_events(800, 600));
    expected.push(Event::Components(parent.identity(), 0, None));
    assert_eq!(backend.events, expected);
    assert!(backend.primitives.is_empty());
}

#[test]
fn full_after_move_precedes_base_draw_and_sink_failure_preserves_primitive_prefix() {
    let mut components = MenuStack::new();
    create(
        &mut components,
        &definition(vec![
            item(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0),
            item(2, *b"tcer", [1.0, 2.0, 8.0, 6.0], 0x300),
            item(3, *b"tcer", [3.0, 4.0, 8.0, 6.0], 0x300),
            item(4, *b"tcer", [5.0, 6.0, 8.0, 6.0], 0x300),
        ]),
        PRECISION,
    )
    .unwrap();
    components.set_hidden(4, false);
    let menu = MenuNativeMenu::new(components, None, 0);
    let mut backend = Backend::new(1);
    backend.fail_primitive = Some(2);
    let error = draw(&menu, &inactive(), 0, &mut backend).unwrap_err();
    assert_eq!(error.stage, MenuNativeDrawStage::Components);
    assert_eq!(
        error.failure,
        MenuNativeDrawFailure::Backend("fixture primitive failure".into())
    );
    assert_eq!(
        backend
            .primitives
            .iter()
            .map(|primitive| primitive.component)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(
        backend.events.last(),
        Some(&Event::Components(menu.identity(), 0, None))
    );
    menu.with_components(|stack| {
        let hidden = stack.find_component_by_id(4).unwrap();
        assert_eq!(hidden.native_geometry().unwrap().position, [15.0, 26.0]);
        Ok(())
    })
    .unwrap();
}

#[test]
fn progress_texture_borrows_the_same_resource_for_parent_and_child() {
    let mut textures = TextureResources::new().unwrap();
    let texture = textures.create_texture().unwrap();
    let progress = definition(vec![
        item(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0),
        item(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0),
    ]);
    let mut parent_stack = MenuStack::new();
    create(&mut parent_stack, &progress, PRECISION).unwrap();
    let mut child_stack = MenuStack::new();
    create(&mut child_stack, &progress, PRECISION).unwrap();
    let parent = MenuNativeMenu::new(parent_stack, None, 0);
    let child = MenuNativeMenu::new(child_stack, Some(parent.identity()), MENUF_DRAW_PARENT);
    let mut backend = Backend::new(2);
    draw_menu_native(
        &child,
        &inactive(),
        MENUDRAW_ACTIVE_MENU,
        Some(&texture),
        PRECISION,
        &mut backend,
    )
    .unwrap();
    let components: Vec<_> = backend
        .events
        .iter()
        .filter(|event| matches!(event, Event::Components(..)))
        .cloned()
        .collect();
    assert_eq!(
        components,
        [
            Event::Components(parent.identity(), 0, Some(texture)),
            Event::Components(child.identity(), MENUDRAW_ACTIVE_MENU, Some(texture))
        ]
    );
    assert_eq!(backend.primitives.len(), 4);
    assert!(backend
        .primitives
        .iter()
        .all(|primitive| primitive.texture == Some(texture)));
    assert_eq!(textures.texture(texture).unwrap().references, 1);
}
