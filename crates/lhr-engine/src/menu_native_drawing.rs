//! Live host-side GM_Menu::Draw ordering, not a renderer or menu lifecycle.
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! Draw RVA 0x1ea7e0, SetViewport RVA 0x1e98a0. Private REA evidence
//! ev_db4a0df9e0a7833c486d1853927be420c359b5a0127ef829e8f70356126bdadd and
//! ev_0b1471de40adad0ae6844e06a3b352bdc5309036d0dcff07d7e34b39fa9440f4;
//! source correspondence mafia-re fe7e7464 game/ui/g_menu.cpp:711.
//!
//! Backend callbacks must perform the actual operations. Component callbacks
//! enter the same owned MenuStack and run its complete native component draw,
//! retaining its payload precision. The separate explicit top-level precision
//! policy covers only reached cursor conversions. Errors retain the reached
//! prefix: there is no automatic replay, rollback or Z-buffer restoration.
//! Allocation-cycle and released-parent errors are explicit unsupported domains.

#![forbid(unsafe_code)]

use crate::itexture::TextureKey;
use crate::menu::{MenuComponentDrawSink, MenuComponentPrecision, MenuStack};
use crate::native_precision::pc24_integer_ratio;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};

pub const MENUF_DRAW_PARENT: u32 = 0x0001_0000;
pub const MENUDRAW_CURSOR: u32 = 0x1000_0000;
pub const MENUDRAW_ACTIVE_MENU: u32 = 0x4000_0000;

#[derive(Debug)]
struct MenuNativeMenuOwner {
    components: RefCell<MenuStack>,
    parent: RefCell<Option<MenuNativeMenuIdentity>>,
    flags: Cell<u32>,
}

/// Cloning this handle aliases one allocation and its one actual MenuStack.
#[derive(Clone, Debug)]
pub struct MenuNativeMenu {
    owner: Rc<MenuNativeMenuOwner>,
}

#[derive(Clone)]
pub struct MenuNativeMenuIdentity {
    owner: Weak<MenuNativeMenuOwner>,
}

impl PartialEq for MenuNativeMenuIdentity {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.owner, &other.owner)
    }
}

impl Eq for MenuNativeMenuIdentity {}

impl fmt::Debug for MenuNativeMenuIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MenuNativeMenuIdentity")
            .field("alive", &(self.owner.strong_count() != 0))
            .finish()
    }
}

impl MenuNativeMenuIdentity {
    pub fn upgrade(&self) -> Option<MenuNativeMenu> {
        self.owner.upgrade().map(|owner| MenuNativeMenu { owner })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuNativeMenuAccessError {
    ComponentsBorrowed,
    ComponentOperation(String),
}

impl fmt::Display for MenuNativeMenuAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentsBorrowed => formatter.write_str("native menu components are borrowed"),
            Self::ComponentOperation(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MenuNativeMenuAccessError {}

impl MenuNativeMenu {
    pub fn new(components: MenuStack, parent: Option<MenuNativeMenuIdentity>, flags: u32) -> Self {
        Self {
            owner: Rc::new(MenuNativeMenuOwner {
                components: RefCell::new(components),
                parent: RefCell::new(parent),
                flags: Cell::new(flags),
            }),
        }
    }

    pub fn identity(&self) -> MenuNativeMenuIdentity {
        MenuNativeMenuIdentity {
            owner: Rc::downgrade(&self.owner),
        }
    }

    pub fn parent(&self) -> Option<MenuNativeMenuIdentity> {
        self.owner.parent.borrow().clone()
    }

    pub fn set_parent(&self, parent: Option<MenuNativeMenuIdentity>) {
        *self.owner.parent.borrow_mut() = parent;
    }

    pub fn flags(&self) -> u32 {
        self.owner.flags.get()
    }

    pub fn set_flags(&self, flags: u32) {
        self.owner.flags.set(flags);
    }

    pub fn with_components<R>(
        &self,
        operation: impl FnOnce(&MenuStack) -> Result<R, String>,
    ) -> Result<R, MenuNativeMenuAccessError> {
        let components = self
            .owner
            .components
            .try_borrow()
            .map_err(|_| MenuNativeMenuAccessError::ComponentsBorrowed)?;
        operation(&components).map_err(MenuNativeMenuAccessError::ComponentOperation)
    }

    pub fn with_components_mut<R>(
        &self,
        operation: impl FnOnce(&mut MenuStack) -> Result<R, String>,
    ) -> Result<R, MenuNativeMenuAccessError> {
        let mut components = self
            .owner
            .components
            .try_borrow_mut()
            .map_err(|_| MenuNativeMenuAccessError::ComponentsBorrowed)?;
        operation(&mut components).map_err(MenuNativeMenuAccessError::ComponentOperation)
    }

    pub fn draw_components_native(
        &self,
        draw_flags: u32,
        progress_texture: Option<&TextureKey>,
        sink: &mut dyn MenuComponentDrawSink,
    ) -> Result<(), String> {
        self.draw_components_native_with_progress_texture(
            draw_flags,
            &|| Ok(progress_texture.copied()),
            sink,
        )
    }

    pub(crate) fn draw_components_native_with_progress_texture(
        &self,
        draw_flags: u32,
        resolve_progress_texture: &dyn Fn() -> Result<Option<TextureKey>, String>,
        sink: &mut dyn MenuComponentDrawSink,
    ) -> Result<(), String> {
        let mut cursor = self
            .with_components_mut(|components| components.begin_components_draw_native(draw_flags))
            .map_err(|error| error.to_string())?;
        loop {
            let call = self
                .with_components_mut(|components| {
                    components.next_component_draw_native(&mut cursor, None)
                })
                .map_err(|error| error.to_string())?;
            let Some(call) = call else {
                return Ok(());
            };
            cursor.complete_call(
                call.dispatch_with_progress_texture(sink, |_| resolve_progress_texture()),
            )?;
        }
    }
}

#[derive(Debug)]
struct MenuNativeGlobals {
    blink_time: Cell<u32>,
    active_menu_window: RefCell<Option<MenuNativeMenuIdentity>>,
    cursor_x: Cell<i32>,
    cursor_y: Cell<i32>,
}

#[derive(Clone, Debug)]
pub struct MenuNativeDrawState {
    globals: Rc<MenuNativeGlobals>,
}

impl MenuNativeDrawState {
    pub fn new(
        blink_time: u32,
        active_menu_window: Option<MenuNativeMenuIdentity>,
        cursor: [i32; 2],
    ) -> Self {
        Self {
            globals: Rc::new(MenuNativeGlobals {
                blink_time: Cell::new(blink_time),
                active_menu_window: RefCell::new(active_menu_window),
                cursor_x: Cell::new(cursor[0]),
                cursor_y: Cell::new(cursor[1]),
            }),
        }
    }

    pub fn blink_time(&self) -> u32 {
        self.globals.blink_time.get()
    }

    pub fn set_blink_time(&self, blink_time: u32) {
        self.globals.blink_time.set(blink_time);
    }

    pub fn active_menu_window(&self) -> Option<MenuNativeMenuIdentity> {
        self.globals.active_menu_window.borrow().clone()
    }

    pub fn set_active_menu_window(&self, active_menu_window: Option<MenuNativeMenuIdentity>) {
        *self.globals.active_menu_window.borrow_mut() = active_menu_window;
    }

    pub fn cursor_x(&self) -> i32 {
        self.globals.cursor_x.get()
    }

    pub fn cursor_y(&self) -> i32 {
        self.globals.cursor_y.get()
    }

    pub fn set_cursor(&self, cursor: [i32; 2]) {
        self.set_cursor_x(cursor[0]);
        self.set_cursor_y(cursor[1]);
    }

    pub fn set_cursor_x(&self, cursor_x: i32) {
        self.globals.cursor_x.set(cursor_x);
    }

    pub fn set_cursor_y(&self, cursor_y: i32) {
        self.globals.cursor_y.set(cursor_y);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuNativeViewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub min_z: f32,
    pub max_z: f32,
}

pub trait MenuNativeDrawBackend {
    fn set_z_buffer(&mut self, enabled: bool) -> Result<(), String>;
    fn screen_height(&mut self) -> Result<u32, String>;
    fn screen_width(&mut self) -> Result<u32, String>;
    fn set_viewport(&mut self, viewport: MenuNativeViewport) -> Result<(), String>;
    fn draw_components(
        &mut self,
        menu: &MenuNativeMenu,
        draw_flags: u32,
        progress_texture: Option<&TextureKey>,
    ) -> Result<(), String>;
    fn draw_cursor(&mut self, x: f32, y: f32) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuNativeDrawStage {
    Parent,
    InitialZOff,
    Root,
    ViewportHeight,
    ViewportWidth,
    ViewportWidthLimit,
    ViewportWidthClamp,
    ViewportHeightLimit,
    ViewportHeightClamp,
    SetViewport,
    ViewportZOff,
    Components,
    CursorY,
    CursorX,
    Cursor,
    FinalZOn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuNativeDrawFailure {
    Backend(String),
    MenuAccess(MenuNativeMenuAccessError),
    ReleasedParent,
    ParentCycle,
    PrecisionUnestablished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuNativeDrawError {
    pub menu: MenuNativeMenuIdentity,
    pub stage: MenuNativeDrawStage,
    pub failure: MenuNativeDrawFailure,
}

impl fmt::Display for MenuNativeDrawError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "native menu Draw {:?}: {:?}",
            self.stage, self.failure
        )
    }
}

impl std::error::Error for MenuNativeDrawError {}

pub fn draw_menu_native(
    menu: &MenuNativeMenu,
    globals: &MenuNativeDrawState,
    draw_flags: u32,
    progress_texture: Option<&TextureKey>,
    precision: MenuComponentPrecision,
    backend: &mut dyn MenuNativeDrawBackend,
) -> Result<(), MenuNativeDrawError> {
    draw_menu_inner(
        menu,
        globals,
        draw_flags,
        progress_texture,
        precision,
        backend,
        &mut Vec::new(),
    )
}

fn draw_menu_inner(
    menu: &MenuNativeMenu,
    globals: &MenuNativeDrawState,
    mut draw_flags: u32,
    progress_texture: Option<&TextureKey>,
    precision: MenuComponentPrecision,
    backend: &mut dyn MenuNativeDrawBackend,
    ancestors: &mut Vec<MenuNativeMenuIdentity>,
) -> Result<(), MenuNativeDrawError> {
    let identity = menu.identity();
    if ancestors.contains(&identity) {
        return Err(MenuNativeDrawError {
            menu: identity,
            stage: MenuNativeDrawStage::Parent,
            failure: MenuNativeDrawFailure::ParentCycle,
        });
    }
    ancestors.push(identity.clone());
    backend_result(
        &identity,
        MenuNativeDrawStage::InitialZOff,
        backend.set_z_buffer(false),
    )?;
    if globals.blink_time() > 700 {
        draw_flags &= !MENUDRAW_CURSOR;
    }
    if let Some(parent) = menu.parent() {
        if menu.flags() & MENUF_DRAW_PARENT != 0 {
            let parent = parent.upgrade().ok_or_else(|| MenuNativeDrawError {
                menu: identity.clone(),
                stage: MenuNativeDrawStage::Parent,
                failure: MenuNativeDrawFailure::ReleasedParent,
            })?;
            draw_menu_inner(
                &parent,
                globals,
                draw_flags & !MENUDRAW_ACTIVE_MENU,
                progress_texture,
                precision,
                backend,
                ancestors,
            )?;
        }
    }
    let root = menu
        .with_components(MenuStack::native_root_key)
        .map_err(|error| MenuNativeDrawError {
            menu: identity.clone(),
            stage: MenuNativeDrawStage::Root,
            failure: MenuNativeDrawFailure::MenuAccess(error),
        })?;
    if root.is_some() {
        set_full_viewport(&identity, backend)?;
        backend_result(
            &identity,
            MenuNativeDrawStage::Components,
            backend.draw_components(menu, draw_flags, progress_texture),
        )?;
    }
    if globals.active_menu_window().as_ref() == Some(&identity) {
        set_full_viewport(&identity, backend)?;
        let cursor_y = cursor_coordinate(
            &identity,
            MenuNativeDrawStage::CursorY,
            globals.cursor_y(),
            precision,
        )?;
        let cursor_x = cursor_coordinate(
            &identity,
            MenuNativeDrawStage::CursorX,
            globals.cursor_x(),
            precision,
        )?;
        backend_result(
            &identity,
            MenuNativeDrawStage::Cursor,
            backend.draw_cursor(cursor_x, cursor_y),
        )?;
    }
    backend_result(
        &identity,
        MenuNativeDrawStage::FinalZOn,
        backend.set_z_buffer(true),
    )?;
    ancestors.pop();
    Ok(())
}

fn backend_result<R>(
    identity: &MenuNativeMenuIdentity,
    stage: MenuNativeDrawStage,
    result: Result<R, String>,
) -> Result<R, MenuNativeDrawError> {
    result.map_err(|message| MenuNativeDrawError {
        menu: identity.clone(),
        stage,
        failure: MenuNativeDrawFailure::Backend(message),
    })
}

fn set_full_viewport(
    identity: &MenuNativeMenuIdentity,
    backend: &mut dyn MenuNativeDrawBackend,
) -> Result<(), MenuNativeDrawError> {
    let mut height = backend_result(
        identity,
        MenuNativeDrawStage::ViewportHeight,
        backend.screen_height(),
    )?;
    let mut width = backend_result(
        identity,
        MenuNativeDrawStage::ViewportWidth,
        backend.screen_width(),
    )?;
    let width_limit = backend_result(
        identity,
        MenuNativeDrawStage::ViewportWidthLimit,
        backend.screen_width(),
    )?;
    if width > width_limit {
        width = backend_result(
            identity,
            MenuNativeDrawStage::ViewportWidthClamp,
            backend.screen_width(),
        )?;
    }
    let height_limit = backend_result(
        identity,
        MenuNativeDrawStage::ViewportHeightLimit,
        backend.screen_height(),
    )?;
    if height > height_limit {
        height = backend_result(
            identity,
            MenuNativeDrawStage::ViewportHeightClamp,
            backend.screen_height(),
        )?;
    }
    backend_result(
        identity,
        MenuNativeDrawStage::SetViewport,
        backend.set_viewport(MenuNativeViewport {
            x: 0,
            y: 0,
            width,
            height,
            min_z: 0.0,
            max_z: 1.0,
        }),
    )?;
    backend_result(
        identity,
        MenuNativeDrawStage::ViewportZOff,
        backend.set_z_buffer(false),
    )
}

fn cursor_coordinate(
    identity: &MenuNativeMenuIdentity,
    stage: MenuNativeDrawStage,
    coordinate: i32,
    precision: MenuComponentPrecision,
) -> Result<f32, MenuNativeDrawError> {
    match precision {
        MenuComponentPrecision::Pc24Nearest => Ok(pc24_integer_ratio(i64::from(coordinate), 1)),
        MenuComponentPrecision::Unestablished => Err(MenuNativeDrawError {
            menu: identity.clone(),
            stage,
            failure: MenuNativeDrawFailure::PrecisionUnestablished,
        }),
    }
}

#[cfg(test)]
#[path = "menu_native_drawing_tests.rs"]
mod tests;
