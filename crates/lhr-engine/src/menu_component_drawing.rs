//! Native component ownership and the two component passes of GM_Menu::Draw.
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! constructor RVA 0x1e9be0, SetTextID 0x1ea1c0, CreateCompsByDef 0x1ea370,
//! BaseAfterMove 0x1e9da0, BaseDraw 0x1e9d60, picture SetString 0x1daa70
//! and destructor 0x1daaf0. Retained REA ev_db4a0df9e0a7833c486d1853927be420c359b5a0127ef829e8f70356126bdadd,
//! ev_ede8a90a795a20e5cc343f67e0754368c83e584413a163c350f7167f2648c5a6,
//! ev_74a2e158cdf22cc9ea0a9e6b763cb0738180e8fa2d8690e38b66897d5979e26a;
//! source correspondence mafia-re fe7e7464 game/ui/g_menu.cpp and g_m_comps.h.
//!
//! The explicitly selected Pc24Nearest policy uses separate binary32
//! round-to-nearest-even operations at each multiply/add, without fused
//! operations, clamping or half-pixel adjustment. That policy is conditional:
//! the loading caller's incoming
//! x87 control word and exception behavior are not established here. Nonfinite
//! geometry and overflow are explicit unsupported domains, not default values.
//! This is not the global driver/viewport/cursor/menu-parent Draw lifecycle.

#![forbid(unsafe_code)]

use super::{MenuComponent, MenuStack};
use crate::itexture::TextureKey;
use crate::main_menu;
use crate::menu_draw_resources::MenuComponentResources;
use lhr_assets::MenuDefinition;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

static NEXT_MENU_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuComponentPrecision {
    Unestablished,
    Pc24Nearest,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuComponentCreateContext {
    pub screen_scale: [f32; 2],
    pub left_margin_x: f32,
    pub precision: MenuComponentPrecision,
}

/// Stable only within its originating, non-reused native stack lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuComponentKey {
    owner: u64,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuComponentChild {
    Uncomposed(u32),
    Native(MenuComponentKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuComponentKind {
    WindowClean,
    Rectangle,
    RectangleTexture,
    StaticText,
    ProgressBar,
}

impl MenuComponentKind {
    fn from_file_tag(tag: [u8; 4]) -> Result<Self, String> {
        match &tag {
            b"cniw" => Ok(Self::WindowClean),
            b"tcer" => Ok(Self::Rectangle),
            b"ttcr" => Ok(Self::RectangleTexture),
            b"txts" => Ok(Self::StaticText),
            b"rabp" => Ok(Self::ProgressBar),
            _ => Err(format!(
                "unsupported native menu component file tag {tag:?}"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuComponentGeometry {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub relative_position: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuComponentPrimitiveTopology {
    LineStrip,
    TriangleStrip,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuComponentDrawVertex {
    pub position: [f32; 4],
    pub uv: [f32; 2],
    pub argb: u32,
}

pub struct MenuComponentPrimitive<'a> {
    pub component: MenuComponentKey,
    pub component_id: u32,
    pub draw_flags: u32,
    pub topology: MenuComponentPrimitiveTopology,
    pub vertices: &'a [MenuComponentDrawVertex],
    pub texture: Option<&'a TextureKey>,
}

pub struct MenuComponentText<'a> {
    pub component: MenuComponentKey,
    pub component_id: u32,
    pub draw_flags: u32,
    pub bytes: &'a [u8],
    pub rect: [f32; 4],
    pub argb: u32,
    pub text_flags: u32,
    pub font: u32,
}

/// Ordered, borrowed commands; a failed sink preserves its already reached prefix.
pub trait MenuComponentDrawSink {
    fn primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String>;
    fn progress_primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String> {
        self.primitive(primitive)
    }
    fn text(&mut self, text: MenuComponentText<'_>) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeDrawPhase {
    Active,
    Pending,
    Dispatching,
    Dispatched,
    Finished,
    Stopped,
}

#[derive(Clone, Copy, Debug)]
enum NativeDrawStage {
    Enter(u32),
    ProgressRed(u32),
    Children { draw_flags: u32, next_child: usize },
}

#[derive(Clone, Copy, Debug)]
struct NativeDrawFrame {
    key: MenuComponentKey,
    stage: NativeDrawStage,
}

#[derive(Debug)]
pub struct MenuComponentDrawCursor {
    owner: u64,
    owner_liveness: Weak<()>,
    traversal: Vec<NativeDrawFrame>,
    phase: Rc<Cell<NativeDrawPhase>>,
}

impl MenuComponentDrawCursor {
    pub fn complete_call(&mut self, result: Result<(), String>) -> Result<(), String> {
        match result {
            Err(error) => {
                self.phase.set(NativeDrawPhase::Stopped);
                Err(error)
            }
            Ok(()) if self.phase.get() == NativeDrawPhase::Dispatched => {
                self.phase.set(NativeDrawPhase::Active);
                Ok(())
            }
            Ok(()) => {
                self.phase.set(NativeDrawPhase::Stopped);
                Err("native component draw has no completed pending call".to_owned())
            }
        }
    }
}

impl Drop for MenuComponentDrawCursor {
    fn drop(&mut self) {
        self.phase.set(NativeDrawPhase::Stopped);
    }
}

#[derive(Debug)]
enum NativeDrawTexture {
    Fixed(Option<TextureKey>),
    Progress(Option<TextureKey>),
}

#[derive(Debug)]
enum NativeDrawArguments {
    Primitive {
        topology: MenuComponentPrimitiveTopology,
        vertices: Vec<MenuComponentDrawVertex>,
        texture: NativeDrawTexture,
    },
    Text {
        bytes: Arc<[u8]>,
        rect: [f32; 4],
        argb: u32,
        text_flags: u32,
        font: u32,
    },
}

#[derive(Debug)]
pub struct MenuComponentDrawCall {
    component: MenuComponentKey,
    component_id: u32,
    draw_flags: u32,
    arguments: NativeDrawArguments,
    owner_liveness: Weak<()>,
    phase: Rc<Cell<NativeDrawPhase>>,
}

impl MenuComponentDrawCall {
    pub fn dispatch(self, sink: &mut dyn MenuComponentDrawSink) -> Result<(), String> {
        self.dispatch_with_progress_texture(sink, Ok)
    }

    pub(crate) fn dispatch_with_progress_texture(
        self,
        sink: &mut dyn MenuComponentDrawSink,
        resolve_progress_texture: impl FnOnce(Option<TextureKey>) -> Result<Option<TextureKey>, String>,
    ) -> Result<(), String> {
        if self.phase.get() != NativeDrawPhase::Pending {
            self.phase.set(NativeDrawPhase::Stopped);
            return Err("native component draw call is no longer dispatchable".to_owned());
        }
        if self.owner_liveness.upgrade().is_none() {
            self.phase.set(NativeDrawPhase::Stopped);
            return Err("native component draw call has a stale menu owner".to_owned());
        }
        self.phase.set(NativeDrawPhase::Dispatching);
        let result = match self.arguments {
            NativeDrawArguments::Primitive {
                topology,
                vertices,
                texture,
            } => (|| {
                let is_progress = matches!(texture, NativeDrawTexture::Progress(_));
                let texture = match texture {
                    NativeDrawTexture::Fixed(texture) => texture,
                    NativeDrawTexture::Progress(texture) => {
                        let texture = resolve_progress_texture(texture)?;
                        if self.owner_liveness.upgrade().is_none() {
                            return Err(
                                "native component draw call has a stale menu owner".to_owned()
                            );
                        }
                        if self.phase.get() != NativeDrawPhase::Dispatching {
                            return Err(
                                "native component draw interrupted during callback".to_owned()
                            );
                        }
                        texture
                    }
                };
                let primitive = MenuComponentPrimitive {
                    component: self.component,
                    component_id: self.component_id,
                    draw_flags: self.draw_flags,
                    topology,
                    vertices: &vertices,
                    texture: texture.as_ref(),
                };
                if is_progress {
                    sink.progress_primitive(primitive)
                } else {
                    sink.primitive(primitive)
                }
            })(),
            NativeDrawArguments::Text {
                bytes,
                rect,
                argb,
                text_flags,
                font,
            } => sink.text(MenuComponentText {
                component: self.component,
                component_id: self.component_id,
                draw_flags: self.draw_flags,
                bytes: &bytes,
                rect,
                argb,
                text_flags,
                font,
            }),
        };
        match result {
            Err(error) => {
                self.phase.set(NativeDrawPhase::Stopped);
                Err(error)
            }
            Ok(()) if self.phase.get() == NativeDrawPhase::Dispatching => {
                self.phase.set(NativeDrawPhase::Dispatched);
                Ok(())
            }
            Ok(()) => {
                self.phase.set(NativeDrawPhase::Stopped);
                Err("native component draw interrupted during callback".to_owned())
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum NativeTextureSlot {
    Empty,
    Live(TextureKey),
    InterruptedRelease(TextureKey),
}

impl NativeTextureSlot {
    fn release(&mut self, resources: &mut dyn MenuComponentResources) -> Result<(), String> {
        match self {
            Self::Empty => Ok(()),
            Self::InterruptedRelease(_) => Err(
                "native texture Release is interrupted; backend reconciliation is required, not replay"
                    .to_owned(),
            ),
            Self::Live(key) => {
                let key = *key;
                *self = Self::InterruptedRelease(key);
                resources.release_texture(key)?;
                *self = Self::Empty;
                Ok(())
            }
        }
    }

    fn drawable(&self) -> Result<Option<&TextureKey>, String> {
        match self {
            Self::Empty => Ok(None),
            Self::Live(key) => Ok(Some(key)),
            Self::InterruptedRelease(_) => {
                Err("interrupted native texture ownership cannot be drawn".to_owned())
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum NativeComponentPayload {
    WindowClean,
    Rectangle,
    RectangleTexture(NativeTextureSlot),
    StaticText,
    ProgressBar { value: f32 },
}

impl NativeComponentPayload {
    fn new(kind: MenuComponentKind) -> Self {
        match kind {
            MenuComponentKind::WindowClean => Self::WindowClean,
            MenuComponentKind::Rectangle => Self::Rectangle,
            MenuComponentKind::RectangleTexture => Self::RectangleTexture(NativeTextureSlot::Empty),
            MenuComponentKind::StaticText => Self::StaticText,
            MenuComponentKind::ProgressBar => Self::ProgressBar { value: 0.0 },
        }
    }

    fn kind(&self) -> MenuComponentKind {
        match self {
            Self::WindowClean => MenuComponentKind::WindowClean,
            Self::Rectangle => MenuComponentKind::Rectangle,
            Self::RectangleTexture(_) => MenuComponentKind::RectangleTexture,
            Self::StaticText => MenuComponentKind::StaticText,
            Self::ProgressBar { .. } => MenuComponentKind::ProgressBar,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeComponentPhase {
    Constructing,
    Live,
    Destroyed,
}

#[derive(Debug, PartialEq)]
pub(super) struct NativeMenuComponent {
    key: MenuComponentKey,
    parent: Option<MenuComponentKey>,
    geometry: MenuComponentGeometry,
    precision: MenuComponentPrecision,
    pub(super) text: Option<Arc<[u8]>>,
    payload: NativeComponentPayload,
    phase: NativeComponentPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeMenuPhase {
    Creating,
    Ready,
    Destroying,
}

#[derive(Debug, PartialEq)]
pub(super) struct NativeMenuState {
    owner: u64,
    root: Option<MenuComponentKey>,
    phase: NativeMenuPhase,
    next_destroy: usize,
    draw_liveness: Option<Arc<()>>,
}

impl MenuComponent {
    pub fn native_key(&self) -> Option<MenuComponentKey> {
        self.native.as_ref().map(|native| native.key)
    }

    pub fn native_kind(&self) -> Option<MenuComponentKind> {
        self.native.as_ref().map(|native| native.payload.kind())
    }

    pub fn native_geometry(&self) -> Option<MenuComponentGeometry> {
        self.native.as_ref().map(|native| native.geometry)
    }

    pub fn native_parent(&self) -> Option<MenuComponentKey> {
        self.native.as_ref().and_then(|native| native.parent)
    }

    /// A borrowed live texture, never an interrupted Release record.
    pub fn texture_key(&self) -> Result<Option<&TextureKey>, String> {
        match self.native.as_ref().map(|native| &native.payload) {
            Some(NativeComponentPayload::RectangleTexture(texture)) => texture.drawable(),
            _ => Ok(None),
        }
    }

    /// Inspection only: this key is neither drawable nor retryable.
    pub fn interrupted_texture_key(&self) -> Option<&TextureKey> {
        match self.native.as_ref().map(|native| &native.payload) {
            Some(NativeComponentPayload::RectangleTexture(
                NativeTextureSlot::InterruptedRelease(key),
            )) => Some(key),
            _ => None,
        }
    }

    pub(super) fn is_native_destroyed(&self) -> bool {
        self.native
            .as_ref()
            .is_some_and(|native| native.phase == NativeComponentPhase::Destroyed)
    }

    pub(super) fn set_native_float(&mut self, value: f32) -> Result<bool, String> {
        let Some(native) = &mut self.native else {
            return Ok(false);
        };
        if native.phase != NativeComponentPhase::Live {
            return Err("native scalar setter requires a live component".to_owned());
        }
        if let NativeComponentPayload::ProgressBar { value: current } = &mut native.payload {
            *current = value;
        }
        Ok(true)
    }
}

impl MenuStack {
    pub(super) fn assert_uncomposed(&self, message: &str) {
        assert!(self.native.is_none(), "{message}");
    }

    pub fn has_native_lifecycle(&self) -> bool {
        self.native.is_some()
    }

    pub fn native_root_key(&self) -> Result<Option<MenuComponentKey>, String> {
        match &self.native {
            Some(state) => Ok(state.root),
            None if self.components.is_empty() => Ok(None),
            None => Err("uncomposed menu has no native component root".to_owned()),
        }
    }

    fn native_state(&self) -> Result<&NativeMenuState, String> {
        self.native
            .as_ref()
            .ok_or_else(|| "uncomposed menu has no native component lifecycle".to_owned())
    }

    pub(super) fn require_native_ready(&self) -> Result<(), String> {
        if self.native_state()?.phase != NativeMenuPhase::Ready {
            return Err("native menu component construction/destruction is incomplete".to_owned());
        }
        Ok(())
    }

    fn native_component(&self, key: MenuComponentKey) -> Result<&NativeMenuComponent, String> {
        let state = self.native_state()?;
        let component = self
            .components
            .get(key.index)
            .and_then(|component| component.native.as_ref())
            .ok_or_else(|| "native component link has no owning allocation".to_owned())?;
        if state.owner != key.owner
            || component.key != key
            || component.phase != NativeComponentPhase::Live
        {
            return Err("foreign, stale or incomplete native component link".to_owned());
        }
        Ok(component)
    }

    fn begin_native_creation(&mut self) -> Result<u64, String> {
        if self.native.is_some() || !self.components.is_empty() {
            return Err(
                "native creation requires an empty stack after explicit destruction".to_owned(),
            );
        }
        let owner = NEXT_MENU_OWNER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |owner| {
                owner.checked_add(1)
            })
            .map_err(|_| "native menu owner identity exhausted".to_owned())?;
        self.native = Some(NativeMenuState {
            owner,
            root: None,
            phase: NativeMenuPhase::Creating,
            next_destroy: 0,
            draw_liveness: Some(Arc::new(())),
        });
        Ok(owner)
    }

    pub fn create_empty_native(&mut self) -> Result<(), String> {
        self.begin_native_creation()?;
        self.native.as_mut().unwrap().phase = NativeMenuPhase::Ready;
        Ok(())
    }

    pub(crate) fn prepare_empty_loading_recreate(&mut self) -> Result<(), String> {
        if !self.components.is_empty() {
            return Err("loading-menu Create would detach live components".into());
        }
        if let Some(native) = &self.native {
            if native.phase != NativeMenuPhase::Ready {
                return Err("loading-menu Create cannot replace an interrupted lifecycle".into());
            }
            self.native = None;
        }
        Ok(())
    }

    /// Mutating native constructor phase; every reached allocation stays owned on Err.
    /// A failed construction must be destroyed, not implicitly replayed or replaced.
    pub fn create_components_native(
        &mut self,
        definition: &MenuDefinition,
        context: MenuComponentCreateContext,
        resources: &mut dyn MenuComponentResources,
    ) -> Result<(), String> {
        let owner = self.begin_native_creation()?;
        let mut parent = None;
        for (row, item) in definition.items.iter().enumerate() {
            if item.is_upup() {
                while let Some(key) = parent {
                    let Some(ancestor) = self.native_component(key)?.parent else {
                        break;
                    };
                    parent = Some(ancestor);
                }
                continue;
            }
            let kind = MenuComponentKind::from_file_tag(item.kind)
                .map_err(|error| format!("menu row {row}: {error}"))?;
            let position = [
                multiply(context.precision, item.rect[0], context.screen_scale[0])?,
                multiply(context.precision, item.rect[1], context.screen_scale[1])?,
            ];
            let size = [
                multiply(context.precision, item.rect[2], context.screen_scale[0])?,
                multiply(context.precision, item.rect[3], context.screen_scale[1])?,
            ];
            let key = MenuComponentKey {
                owner,
                index: self.components.len(),
            };
            self.components.push(MenuComponent {
                id: item.id,
                flags: item.style,
                text_id: item.text_id,
                children: Vec::new(),
                native: Some(NativeMenuComponent {
                    key,
                    parent: None,
                    geometry: MenuComponentGeometry {
                        position,
                        size,
                        relative_position: position,
                    },
                    precision: context.precision,
                    text: None,
                    payload: NativeComponentPayload::new(kind),
                    phase: NativeComponentPhase::Constructing,
                }),
            });
            let text = if item.text_id == 0 {
                None
            } else {
                resources.text(item.text_id)?
            };
            let native = self.components[key.index].native.as_mut().unwrap();
            native.text = text;
            native.parent = parent;
            native.phase = NativeComponentPhase::Live;
            if let Some(parent_key) = parent {
                self.native_component(parent_key)?;
                self.components[parent_key.index]
                    .children
                    .push(MenuComponentChild::Native(key));
            }
            if self.native.as_ref().unwrap().root.is_none() {
                self.native.as_mut().unwrap().root = Some(key);
                parent = Some(key);
                let native = self.components[key.index].native.as_mut().unwrap();
                native.geometry.relative_position[0] = add(
                    native.precision,
                    native.geometry.relative_position[0],
                    context.left_margin_x,
                )?;
            }
            if kind == MenuComponentKind::WindowClean {
                parent = Some(key);
            }
        }
        self.native.as_mut().unwrap().phase = NativeMenuPhase::Ready;
        Ok(())
    }

    /// SetTextID latches the id before the fallible database lookup; Err retains old text.
    pub fn set_text_id_native(
        &mut self,
        id: u32,
        text_id: u32,
        resources: &mut dyn MenuComponentResources,
    ) -> Result<(), String> {
        self.require_native_ready()?;
        let Some(component) = self.find_component_by_id_mut(id) else {
            return Ok(());
        };
        component.text_id = text_id;
        let text = if text_id == 0 {
            None
        } else {
            resources.text(text_id)?
        };
        component.native.as_mut().unwrap().text = text;
        Ok(())
    }

    /// Picture replacement releases the old texture before calling the filename factory.
    /// Native NULL creation is unsupported by the non-null resource factory contract.
    pub fn set_string_native(
        &mut self,
        id: u32,
        string: Option<&[u8]>,
        resources: &mut dyn MenuComponentResources,
    ) -> Result<(), String> {
        self.require_native_ready()?;
        let Some(component) = self.find_component_by_id_mut(id) else {
            return Ok(());
        };
        let NativeComponentPayload::RectangleTexture(texture) =
            &mut component.native.as_mut().unwrap().payload
        else {
            return Ok(());
        };
        texture.release(resources)?;
        let name = string.ok_or_else(|| {
            "unsupported native CreateITexture NULL primary name; not an empty filename".to_owned()
        })?;
        if let Some(key) = resources.create_alpha_texture(name)? {
            *texture = NativeTextureSlot::Live(key);
        }
        Ok(())
    }

    /// Destroys allocations in creation order. An interrupted Release keeps its key
    /// and completed deletion prefix; a later call cannot repeat that Release.
    pub fn destroy_native(
        &mut self,
        resources: &mut dyn MenuComponentResources,
    ) -> Result<(), String> {
        self.native_state()?;
        self.native.as_mut().unwrap().draw_liveness = None;
        self.native.as_mut().unwrap().phase = NativeMenuPhase::Destroying;
        while self.native.as_ref().unwrap().next_destroy < self.components.len() {
            let index = self.native.as_ref().unwrap().next_destroy;
            let component = &mut self.components[index];
            let native = component
                .native
                .as_mut()
                .ok_or_else(|| "uncomposed allocation in native menu".to_owned())?;
            if let NativeComponentPayload::RectangleTexture(texture) = &mut native.payload {
                texture.release(resources)?;
            }
            native.text = None;
            native.phase = NativeComponentPhase::Destroyed;
            component.children.clear();
            self.native.as_mut().unwrap().next_destroy += 1;
        }
        *self = Self::new();
        Ok(())
    }

    /// Host inspection of the actual ProgressBar value, not native GetFloat.
    pub fn progress_value(&self, id: u32) -> Option<f32> {
        match &self.find_component_by_id(id)?.native.as_ref()?.payload {
            NativeComponentPayload::ProgressBar { value } => Some(*value),
            _ => None,
        }
    }

    /// The complete BaseAfterMove pass precedes the visible-subtree BaseDraw
    /// pass. This component phase has no cursor, viewport, blink clock, menu
    /// parent or global driver state; those branches require their real owners.
    pub fn draw_components_native(
        &mut self,
        draw_flags: u32,
        progress_texture: Option<&TextureKey>,
        sink: &mut dyn MenuComponentDrawSink,
    ) -> Result<(), String> {
        let mut cursor = self.begin_components_draw_native(draw_flags)?;
        while let Some(call) = self.next_component_draw_native(&mut cursor, progress_texture)? {
            cursor.complete_call(call.dispatch(sink))?;
        }
        Ok(())
    }

    pub fn begin_components_draw_native(
        &mut self,
        draw_flags: u32,
    ) -> Result<MenuComponentDrawCursor, String> {
        self.require_native_ready()?;
        let state = self.native_state()?;
        let owner = state.owner;
        let owner_liveness = Arc::downgrade(state.draw_liveness.as_ref().unwrap());
        let root = state.root;
        if let Some(root) = root {
            self.after_move_component_native(root)?;
            let mut traversal = vec![(root, 0)];
            while let Some((parent, next_child)) = traversal.last_mut() {
                if let Some(child) = self.native_child(*parent, *next_child)? {
                    *next_child += 1;
                    self.after_move_component_native(child)?;
                    traversal.push((child, 0));
                } else {
                    traversal.pop();
                }
            }
        }
        Ok(MenuComponentDrawCursor {
            owner,
            owner_liveness,
            traversal: root
                .map(|key| NativeDrawFrame {
                    key,
                    stage: NativeDrawStage::Enter(draw_flags),
                })
                .into_iter()
                .collect(),
            phase: Rc::new(Cell::new(NativeDrawPhase::Active)),
        })
    }

    pub fn next_component_draw_native(
        &mut self,
        cursor: &mut MenuComponentDrawCursor,
        progress_texture: Option<&TextureKey>,
    ) -> Result<Option<MenuComponentDrawCall>, String> {
        match cursor.phase.get() {
            NativeDrawPhase::Finished => return Ok(None),
            NativeDrawPhase::Stopped => {
                return Err("native component draw traversal is stopped".to_owned());
            }
            NativeDrawPhase::Pending
            | NativeDrawPhase::Dispatching
            | NativeDrawPhase::Dispatched => {
                cursor.phase.set(NativeDrawPhase::Stopped);
                return Err("native component draw call is still pending".to_owned());
            }
            NativeDrawPhase::Active => {}
        }
        let result = self.prepare_component_draw_native(cursor, progress_texture);
        cursor.phase.set(match &result {
            Ok(Some(_)) => NativeDrawPhase::Pending,
            Ok(None) => NativeDrawPhase::Finished,
            Err(_) => NativeDrawPhase::Stopped,
        });
        result
    }

    fn prepare_component_draw_native(
        &self,
        cursor: &mut MenuComponentDrawCursor,
        progress_texture: Option<&TextureKey>,
    ) -> Result<Option<MenuComponentDrawCall>, String> {
        loop {
            let Some(frame) = cursor.traversal.last().copied() else {
                return Ok(None);
            };
            if self.native_state()?.owner != cursor.owner {
                return Err("native component draw traversal has a stale menu owner".to_owned());
            }
            self.require_native_ready()?;
            let (filtered, arguments) = match frame.stage {
                NativeDrawStage::Enter(draw_flags) => {
                    let native = self.native_component(frame.key)?;
                    let component = &self.components[frame.key.index];
                    if component.flags & main_menu::COMPONENT_VISIBLE == 0 {
                        cursor.traversal.pop();
                        continue;
                    }
                    let filtered = draw_flags & (component.flags | 0xff00_0000);
                    cursor.traversal.last_mut().unwrap().stage =
                        if matches!(&native.payload, NativeComponentPayload::ProgressBar { .. }) {
                            NativeDrawStage::ProgressRed(filtered)
                        } else {
                            NativeDrawStage::Children {
                                draw_flags: filtered,
                                next_child: 0,
                            }
                        };
                    (
                        filtered,
                        self.component_draw_arguments_native(frame.key, progress_texture)?,
                    )
                }
                NativeDrawStage::ProgressRed(filtered) => {
                    let native = self.native_component(frame.key)?;
                    let NativeComponentPayload::ProgressBar { value } = &native.payload else {
                        return Err(
                            "native progress draw lost its owning progress component".to_owned()
                        );
                    };
                    let vertices = filled_vertices(
                        native.precision,
                        native.geometry,
                        0xffff_0000,
                        Some(*value),
                    )?;
                    cursor.traversal.last_mut().unwrap().stage = NativeDrawStage::Children {
                        draw_flags: filtered,
                        next_child: 0,
                    };
                    (
                        filtered,
                        Some(NativeDrawArguments::Primitive {
                            topology: MenuComponentPrimitiveTopology::TriangleStrip,
                            vertices: vertices.to_vec(),
                            texture: NativeDrawTexture::Progress(progress_texture.copied()),
                        }),
                    )
                }
                NativeDrawStage::Children {
                    draw_flags,
                    next_child,
                } => {
                    if let Some(child) = self.native_child(frame.key, next_child)? {
                        cursor.traversal.last_mut().unwrap().stage = NativeDrawStage::Children {
                            draw_flags,
                            next_child: next_child + 1,
                        };
                        cursor.traversal.push(NativeDrawFrame {
                            key: child,
                            stage: NativeDrawStage::Enter(draw_flags),
                        });
                    } else {
                        cursor.traversal.pop();
                    }
                    continue;
                }
            };
            if let Some(arguments) = arguments {
                return Ok(Some(MenuComponentDrawCall {
                    component: frame.key,
                    component_id: self.components[frame.key.index].id,
                    draw_flags: filtered,
                    arguments,
                    owner_liveness: Weak::clone(&cursor.owner_liveness),
                    phase: Rc::clone(&cursor.phase),
                }));
            }
        }
    }

    fn native_child(
        &self,
        parent: MenuComponentKey,
        index: usize,
    ) -> Result<Option<MenuComponentKey>, String> {
        self.native_component(parent)?;
        match self.components[parent.index].children.get(index) {
            None => Ok(None),
            Some(MenuComponentChild::Uncomposed(_)) => {
                Err("uncomposed child link in native menu".to_owned())
            }
            Some(MenuComponentChild::Native(key)) => {
                if self.native_component(*key)?.parent != Some(parent) {
                    return Err("native child link does not match its actual parent".to_owned());
                }
                Ok(Some(*key))
            }
        }
    }

    fn after_move_component_native(&mut self, key: MenuComponentKey) -> Result<(), String> {
        let native = self.native_component(key)?;
        let relative = native.geometry.relative_position;
        let parent = native.parent;
        let precision = native.precision;
        if let Some(parent) = parent {
            let origin = self.native_component(parent)?.geometry.position;
            let geometry = &mut self.components[key.index].native.as_mut().unwrap().geometry;
            geometry.position = origin;
            geometry.position[0] = add(precision, geometry.position[0], relative[0])?;
            geometry.position[1] = add(precision, geometry.position[1], relative[1])?;
        } else {
            self.components[key.index]
                .native
                .as_mut()
                .unwrap()
                .geometry
                .position = relative;
        }
        Ok(())
    }

    fn component_draw_arguments_native(
        &self,
        key: MenuComponentKey,
        progress_texture: Option<&TextureKey>,
    ) -> Result<Option<NativeDrawArguments>, String> {
        let native = self.native_component(key)?;
        let component = &self.components[key.index];
        let alternate_color = component.flags & 0x100 != 0;
        let alternate_style = component.flags & 0x200 != 0;
        match &native.payload {
            NativeComponentPayload::WindowClean => Ok(None),
            NativeComponentPayload::Rectangle => {
                let color = match (alternate_style, alternate_color) {
                    (true, true) => 0xffff_ffff,
                    (true, false) => 0xff00_0000,
                    (false, true) => 0x7f00_0000,
                    (false, false) => 0xff00_0000,
                };
                if alternate_style || alternate_color {
                    let vertices = filled_vertices(native.precision, native.geometry, color, None)?;
                    Ok(Some(NativeDrawArguments::Primitive {
                        topology: MenuComponentPrimitiveTopology::TriangleStrip,
                        vertices: vertices.to_vec(),
                        texture: NativeDrawTexture::Fixed(None),
                    }))
                } else {
                    let vertices = outline_vertices(native.precision, native.geometry, color)?;
                    Ok(Some(NativeDrawArguments::Primitive {
                        topology: MenuComponentPrimitiveTopology::LineStrip,
                        vertices: vertices.to_vec(),
                        texture: NativeDrawTexture::Fixed(None),
                    }))
                }
            }
            NativeComponentPayload::RectangleTexture(texture) => {
                if let Some(texture) = texture.drawable()? {
                    let vertices =
                        filled_vertices(native.precision, native.geometry, 0xffff_ffff, None)?;
                    Ok(Some(NativeDrawArguments::Primitive {
                        topology: MenuComponentPrimitiveTopology::TriangleStrip,
                        vertices: vertices.to_vec(),
                        texture: NativeDrawTexture::Fixed(Some(*texture)),
                    }))
                } else {
                    Ok(None)
                }
            }
            NativeComponentPayload::StaticText => {
                if let Some(bytes) = &native.text {
                    let color = match (alternate_style, alternate_color) {
                        (true, true) => 0xff00_0000,
                        (true, false) => 0xffff_ff00,
                        (false, true) => 0xffff_ffff,
                        (false, false) => 0xff7f_7f7f,
                    };
                    let text_flags = if component.flags & 0x80 != 0 {
                        0x2
                    } else if component.flags & 0x40 != 0 {
                        0x4
                    } else {
                        0
                    };
                    Ok(Some(NativeDrawArguments::Text {
                        bytes: Arc::clone(bytes),
                        rect: [
                            native.geometry.position[0],
                            native.geometry.position[1],
                            native.geometry.size[0],
                            native.geometry.size[1],
                        ],
                        argb: color,
                        text_flags,
                        font: component.flags & 0xf,
                    }))
                } else {
                    Ok(None)
                }
            }
            NativeComponentPayload::ProgressBar { .. } => {
                let vertices =
                    filled_vertices(native.precision, native.geometry, 0x5f7f_7f7f, Some(1.0))?;
                Ok(Some(NativeDrawArguments::Primitive {
                    topology: MenuComponentPrimitiveTopology::TriangleStrip,
                    vertices: vertices.to_vec(),
                    texture: NativeDrawTexture::Progress(progress_texture.copied()),
                }))
            }
        }
    }
}

fn vertex(x: f32, y: f32, uv: [f32; 2], argb: u32) -> MenuComponentDrawVertex {
    MenuComponentDrawVertex {
        position: [x, y, 0.0, 1.0],
        uv,
        argb,
    }
}

fn outline_vertices(
    precision: MenuComponentPrecision,
    geometry: MenuComponentGeometry,
    color: u32,
) -> Result<[MenuComponentDrawVertex; 5], String> {
    let [left, top] = geometry.position;
    let right = add(precision, left, geometry.size[0])?;
    let bottom = add(precision, top, geometry.size[1])?;
    Ok([
        vertex(left, top, [0.0, 0.0], color),
        vertex(right, top, [0.0, 0.0], color),
        vertex(right, bottom, [0.0, 0.0], color),
        vertex(left, bottom, [0.0, 0.0], color),
        vertex(left, top, [0.0, 0.0], color),
    ])
}

fn filled_vertices(
    precision: MenuComponentPrecision,
    geometry: MenuComponentGeometry,
    color: u32,
    part: Option<f32>,
) -> Result<[MenuComponentDrawVertex; 4], String> {
    let [left, top] = geometry.position;
    let (width, right_u) = match part {
        Some(part) => (multiply(precision, geometry.size[0], part)?, part),
        None => (geometry.size[0], 1.0),
    };
    let right = add(precision, left, width)?;
    let bottom = add(precision, top, geometry.size[1])?;
    Ok([
        vertex(left, top, [0.0, 0.0], color),
        vertex(right, top, [right_u, 0.0], color),
        vertex(left, bottom, [0.0, 1.0], color),
        vertex(right, bottom, [right_u, 1.0], color),
    ])
}

fn multiply(precision: MenuComponentPrecision, left: f32, right: f32) -> Result<f32, String> {
    match precision {
        MenuComponentPrecision::Unestablished => {
            Err("unsupported unestablished native component precision at multiply".to_owned())
        }
        MenuComponentPrecision::Pc24Nearest => {
            let result = crate::native_precision::mul(left, right);
            finite_arithmetic(left, right, result)
        }
    }
}

fn add(precision: MenuComponentPrecision, left: f32, right: f32) -> Result<f32, String> {
    match precision {
        MenuComponentPrecision::Unestablished => {
            Err("unsupported unestablished native component precision at add".to_owned())
        }
        MenuComponentPrecision::Pc24Nearest => {
            let result = crate::native_precision::add(left, right);
            finite_arithmetic(left, right, result)
        }
    }
}

fn finite_arithmetic(left: f32, right: f32, result: f32) -> Result<f32, String> {
    if left.is_finite() && right.is_finite() && result.is_finite() {
        Ok(result)
    } else {
        Err("unsupported nonfinite/overflow native component geometry under the portable PC24 policy".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itexture::TextureResources;
    use lhr_assets::MenuItem;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    const VISIBLE: u32 = main_menu::COMPONENT_VISIBLE;
    const CONTEXT: MenuComponentCreateContext = MenuComponentCreateContext {
        screen_scale: [1.0, 1.0],
        left_margin_x: 0.0,
        precision: MenuComponentPrecision::Pc24Nearest,
    };

    #[derive(Debug, PartialEq, Eq)]
    enum ResourceCall {
        Text(u32),
        Create(Vec<u8>),
        Release(TextureKey),
    }

    struct Resources {
        textures: TextureResources,
        texts: BTreeMap<u32, Arc<[u8]>>,
        calls: Vec<ResourceCall>,
        fail_text: Option<u32>,
        fail_create: bool,
        create_none: bool,
        fail_release: Option<TextureKey>,
    }

    impl Resources {
        fn new() -> Self {
            Self {
                textures: TextureResources::new().unwrap(),
                texts: BTreeMap::new(),
                calls: Vec::new(),
                fail_text: None,
                fail_create: false,
                create_none: false,
                fail_release: None,
            }
        }
    }

    impl MenuComponentResources for Resources {
        fn text(&mut self, text_id: u32) -> Result<Option<Arc<[u8]>>, String> {
            self.calls.push(ResourceCall::Text(text_id));
            if self.fail_text == Some(text_id) {
                return Err("text failure".to_owned());
            }
            Ok(self.texts.get(&text_id).map(Arc::clone))
        }

        fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
            self.calls.push(ResourceCall::Create(name.to_vec()));
            if self.fail_create {
                return Err("factory failure".to_owned());
            }
            if self.create_none {
                return Ok(None);
            }
            Ok(Some(self.textures.create_texture().unwrap()))
        }

        fn release_texture(&mut self, key: TextureKey) -> Result<(), String> {
            self.calls.push(ResourceCall::Release(key));
            if self.fail_release == Some(key) {
                return Err("uncertain release failure".to_owned());
            }
            Ok(())
        }
    }

    #[derive(Debug, PartialEq)]
    enum Command {
        Primitive {
            key: MenuComponentKey,
            id: u32,
            draw_flags: u32,
            topology: MenuComponentPrimitiveTopology,
            vertices: Vec<MenuComponentDrawVertex>,
            texture: Option<TextureKey>,
        },
        Text {
            key: MenuComponentKey,
            id: u32,
            draw_flags: u32,
            bytes: Vec<u8>,
            rect: [f32; 4],
            argb: u32,
            text_flags: u32,
            font: u32,
        },
    }

    impl Command {
        fn id(&self) -> u32 {
            match self {
                Self::Primitive { id, .. } | Self::Text { id, .. } => *id,
            }
        }
    }

    #[derive(Default)]
    struct Sink {
        commands: Vec<Command>,
        attempts: usize,
        fail_at: Option<usize>,
    }

    impl Sink {
        fn record(&mut self, command: Command) -> Result<(), String> {
            let index = self.attempts;
            self.attempts += 1;
            if self.fail_at == Some(index) {
                return Err("sink failure".to_owned());
            }
            self.commands.push(command);
            Ok(())
        }
    }

    impl MenuComponentDrawSink for Sink {
        fn primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String> {
            self.record(Command::Primitive {
                key: primitive.component,
                id: primitive.component_id,
                draw_flags: primitive.draw_flags,
                topology: primitive.topology,
                vertices: primitive.vertices.to_vec(),
                texture: primitive.texture.copied(),
            })
        }

        fn text(&mut self, text: MenuComponentText<'_>) -> Result<(), String> {
            self.record(Command::Text {
                key: text.component,
                id: text.component_id,
                draw_flags: text.draw_flags,
                bytes: text.bytes.to_vec(),
                rect: text.rect,
                argb: text.argb,
                text_flags: text.text_flags,
                font: text.font,
            })
        }
    }

    struct CallbackSink<Callback> {
        callback: Callback,
        recorded: Sink,
    }

    impl<Callback> MenuComponentDrawSink for CallbackSink<Callback>
    where
        Callback: for<'packet> FnMut(&MenuComponentPrimitive<'packet>) -> Result<(), String>,
    {
        fn primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String> {
            (self.callback)(&primitive)?;
            self.recorded.primitive(primitive)
        }

        fn text(&mut self, text: MenuComponentText<'_>) -> Result<(), String> {
            self.recorded.text(text)
        }
    }

    fn draw_incremental(
        stack: &RefCell<MenuStack>,
        draw_flags: u32,
        progress_texture: Option<&TextureKey>,
        sink: &mut dyn MenuComponentDrawSink,
    ) -> Result<(), String> {
        let mut cursor = stack
            .borrow_mut()
            .begin_components_draw_native(draw_flags)?;
        loop {
            let call = stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, progress_texture)?;
            let Some(call) = call else {
                return Ok(());
            };
            cursor.complete_call(call.dispatch(sink))?;
        }
    }

    fn row(id: u32, kind: [u8; 4], rect: [f32; 4], text_id: u32, style: u32) -> MenuItem {
        MenuItem {
            id,
            kind,
            rect,
            text_id,
            style,
        }
    }

    fn root() -> MenuItem {
        row(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0, VISIBLE)
    }

    fn definition(items: Vec<MenuItem>) -> MenuDefinition {
        MenuDefinition {
            version: 1,
            reserved: 0,
            items,
        }
    }

    fn create(items: Vec<MenuItem>, resources: &mut Resources) -> MenuStack {
        let mut stack = MenuStack::new();
        stack
            .create_components_native(&definition(items), CONTEXT, resources)
            .unwrap();
        stack
    }

    fn geometry(stack: &MenuStack, id: u32) -> MenuComponentGeometry {
        stack
            .find_component_by_id(id)
            .unwrap()
            .native_geometry()
            .unwrap()
    }

    fn picture_key(stack: &MenuStack, id: u32) -> TextureKey {
        *stack
            .find_component_by_id(id)
            .unwrap()
            .texture_key()
            .unwrap()
            .unwrap()
    }

    #[test]
    fn empty_native_creation_establishes_an_authoritative_ready_owner() {
        let mut stack = MenuStack::new();
        assert!(!stack.has_native_lifecycle());
        assert_eq!(
            stack.require_native_ready(),
            Err("uncomposed menu has no native component lifecycle".to_owned())
        );

        assert_eq!(stack.create_empty_native(), Ok(()));
        assert!(stack.has_native_lifecycle());
        assert!(stack.is_empty());
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(stack.require_native_ready(), Ok(()));
        let owner = stack.native.as_ref().unwrap().owner;
        assert_ne!(owner, 0);
        assert_eq!(
            stack.native.as_ref(),
            Some(&NativeMenuState {
                owner,
                root: None,
                phase: NativeMenuPhase::Ready,
                next_destroy: 0,
                draw_liveness: Some(Arc::new(())),
            })
        );

        assert_eq!(
            stack.create_empty_native(),
            Err("native creation requires an empty stack after explicit destruction".to_owned())
        );
        assert_eq!(
            stack.native.as_ref(),
            Some(&NativeMenuState {
                owner,
                root: None,
                phase: NativeMenuPhase::Ready,
                next_destroy: 0,
                draw_liveness: Some(Arc::new(())),
            })
        );
        assert!(stack.has_native_lifecycle());
        assert!(stack.is_empty());
    }

    #[test]
    fn empty_native_setters_and_draw_do_not_reach_resources_or_commands() {
        let mut stack = MenuStack::new();
        stack.create_empty_native().unwrap();
        let owner = stack.native.as_ref().unwrap().owner;
        let mut resources = Resources::new();
        resources.fail_text = Some(7);
        resources.fail_create = true;

        assert_eq!(stack.set_float(300, f32::NAN), Ok(()));
        assert_eq!(stack.set_text_id_native(400, 7, &mut resources), Ok(()));
        assert_eq!(stack.set_text_id_native(400, 0, &mut resources), Ok(()));
        assert_eq!(
            stack.set_string_native(100, Some(b"unreached.bin"), &mut resources),
            Ok(())
        );
        assert_eq!(stack.set_string_native(102, None, &mut resources), Ok(()));
        let mut sink = Sink {
            fail_at: Some(0),
            ..Sink::default()
        };
        assert_eq!(stack.draw_components_native(0, None, &mut sink), Ok(()));
        assert_eq!(
            stack.draw_components_native(u32::MAX, None, &mut sink),
            Ok(())
        );

        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
        assert!(resources.calls.is_empty());
        assert!(stack.has_native_lifecycle());
        assert!(stack.is_empty());
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(
            stack.native.as_ref(),
            Some(&NativeMenuState {
                owner,
                root: None,
                phase: NativeMenuPhase::Ready,
                next_destroy: 0,
                draw_liveness: Some(Arc::new(())),
            })
        );
    }

    #[test]
    fn empty_native_destroy_and_recreate_share_fresh_owner_identities() {
        let mut stack = MenuStack::new();
        let mut resources = Resources::new();
        stack.create_empty_native().unwrap();
        let first_owner = stack.native.as_ref().unwrap().owner;
        assert_eq!(stack.destroy_native(&mut resources), Ok(()));
        assert!(!stack.has_native_lifecycle());
        assert_eq!(stack, MenuStack::new());
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(
            stack.destroy_native(&mut resources),
            Err("uncomposed menu has no native component lifecycle".to_owned())
        );
        assert_eq!(
            stack.set_text_id_native(400, 7, &mut resources),
            Err("uncomposed menu has no native component lifecycle".to_owned())
        );
        assert_eq!(
            stack.set_string_native(100, Some(b"unreached.bin"), &mut resources),
            Err("uncomposed menu has no native component lifecycle".to_owned())
        );
        assert_eq!(stack, MenuStack::new());

        stack.create_empty_native().unwrap();
        let second_owner = stack.native.as_ref().unwrap().owner;
        assert_ne!(first_owner, second_owner);
        let root_definition = definition(vec![root()]);
        assert_eq!(
            stack.create_components_native(&root_definition, CONTEXT, &mut resources),
            Err("native creation requires an empty stack after explicit destruction".to_owned())
        );
        assert_eq!(stack.native.as_ref().unwrap().owner, second_owner);
        assert_eq!(stack.require_native_ready(), Ok(()));
        assert_eq!(stack.native_root_key(), Ok(None));
        stack.destroy_native(&mut resources).unwrap();
        assert!(!stack.has_native_lifecycle());

        stack
            .create_components_native(&root_definition, CONTEXT, &mut resources)
            .unwrap();
        let third_owner = stack.native.as_ref().unwrap().owner;
        assert!(![first_owner, second_owner].contains(&third_owner));
        let root_key = stack.native_root_key().unwrap().unwrap();
        assert_eq!(root_key.owner, third_owner);
        assert_eq!(stack.len(), 1);
        assert!(stack.has_native_lifecycle());
        assert_eq!(
            stack.create_empty_native(),
            Err("native creation requires an empty stack after explicit destruction".to_owned())
        );
        assert_eq!(stack.native_root_key(), Ok(Some(root_key)));
        assert_eq!(stack.native.as_ref().unwrap().owner, third_owner);
        assert_eq!(stack.len(), 1);
        stack.destroy_native(&mut resources).unwrap();
        assert!(!stack.has_native_lifecycle());

        stack.create_empty_native().unwrap();
        let fourth_owner = stack.native.as_ref().unwrap().owner;
        assert!(![first_owner, second_owner, third_owner].contains(&fourth_owner));
        assert!(stack.has_native_lifecycle());
        assert!(stack.is_empty());
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(stack.require_native_ready(), Ok(()));
        stack.destroy_native(&mut resources).unwrap();
        assert!(!stack.has_native_lifecycle());
        assert_eq!(stack, MenuStack::new());
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn empty_native_creation_rejects_nonempty_legacy_components() {
        let legacy_definition = definition(vec![root()]);
        let mut stack = MenuStack::create_from_definition(&legacy_definition);
        assert!(!stack.has_native_lifecycle());
        assert_eq!(
            stack.create_empty_native(),
            Err("native creation requires an empty stack after explicit destruction".to_owned())
        );
        assert!(!stack.has_native_lifecycle());
        assert_eq!(stack.len(), 1);
        assert_eq!(stack, MenuStack::create_from_definition(&legacy_definition));
        assert_eq!(
            stack.native_root_key(),
            Err("uncomposed menu has no native component root".to_owned())
        );
        assert_eq!(
            stack.require_native_ready(),
            Err("uncomposed menu has no native component lifecycle".to_owned())
        );
    }

    #[test]
    fn empty_native_creation_cannot_complete_or_replay_zero_component_interruption() {
        let mut stack = MenuStack::new();
        let mut resources = Resources::new();
        let root_definition = definition(vec![root()]);
        let context = MenuComponentCreateContext {
            screen_scale: [1.0, 1.0],
            left_margin_x: 0.0,
            precision: MenuComponentPrecision::Unestablished,
        };
        assert!(!stack.has_native_lifecycle());
        assert_eq!(
            stack.create_components_native(&root_definition, context, &mut resources),
            Err("unsupported unestablished native component precision at multiply".to_owned())
        );
        let owner = stack.native.as_ref().unwrap().owner;
        assert!(stack.has_native_lifecycle());
        assert!(stack.is_empty());
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(
            stack.native.as_ref(),
            Some(&NativeMenuState {
                owner,
                root: None,
                phase: NativeMenuPhase::Creating,
                next_destroy: 0,
                draw_liveness: Some(Arc::new(())),
            })
        );
        assert_eq!(
            stack.create_empty_native(),
            Err("native creation requires an empty stack after explicit destruction".to_owned())
        );
        assert_eq!(
            stack.create_components_native(&root_definition, context, &mut resources),
            Err("native creation requires an empty stack after explicit destruction".to_owned())
        );
        assert_eq!(
            stack.set_float(300, 0.5),
            Err("native menu component construction/destruction is incomplete".to_owned())
        );
        assert_eq!(
            stack.set_text_id_native(400, 7, &mut resources),
            Err("native menu component construction/destruction is incomplete".to_owned())
        );
        assert_eq!(
            stack.set_string_native(100, Some(b"unreached.bin"), &mut resources),
            Err("native menu component construction/destruction is incomplete".to_owned())
        );
        let mut sink = Sink {
            fail_at: Some(0),
            ..Sink::default()
        };
        assert_eq!(
            stack.draw_components_native(u32::MAX, None, &mut sink),
            Err("native menu component construction/destruction is incomplete".to_owned())
        );
        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
        assert!(resources.calls.is_empty());
        assert!(stack.has_native_lifecycle());
        assert!(stack.is_empty());
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(
            stack.native.as_ref(),
            Some(&NativeMenuState {
                owner,
                root: None,
                phase: NativeMenuPhase::Creating,
                next_destroy: 0,
                draw_liveness: Some(Arc::new(())),
            })
        );

        assert_eq!(stack.destroy_native(&mut resources), Ok(()));
        assert!(!stack.has_native_lifecycle());
        assert_eq!(stack, MenuStack::new());
        assert_eq!(stack.create_empty_native(), Ok(()));
        assert!(stack.has_native_lifecycle());
        assert_ne!(stack.native.as_ref().unwrap().owner, owner);
        assert_eq!(stack.require_native_ready(), Ok(()));
        assert_eq!(stack.native_root_key(), Ok(None));
        assert!(stack.is_empty());
        assert!(resources.calls.is_empty());
        stack.destroy_native(&mut resources).unwrap();
        assert!(!stack.has_native_lifecycle());
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn native_root_key_is_none_for_empty_uncomposed_and_native_stacks() {
        let mut stack = MenuStack::new();
        assert!(stack.native.is_none());
        assert_eq!(stack.native_root_key(), Ok(None));
        let mut resources = Resources::new();
        stack
            .create_components_native(&definition(vec![]), CONTEXT, &mut resources)
            .unwrap();
        assert!(stack.is_empty());
        assert_eq!(stack.native_root_key(), Ok(None));
        stack.destroy_native(&mut resources).unwrap();
        assert_eq!(stack.native_root_key(), Ok(None));
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn native_root_key_rejects_nonempty_uncomposed_stack() {
        let stack = MenuStack::create_from_definition(&definition(vec![root()]));
        assert_eq!(stack.len(), 1);
        assert!(stack.native.is_none());
        assert_eq!(
            stack.native_root_key(),
            Err("uncomposed menu has no native component root".to_owned())
        );
    }

    #[test]
    fn native_root_key_is_none_for_interrupted_root_constructor() {
        let mut resources = Resources::new();
        resources.fail_text = Some(7);
        let mut stack = MenuStack::new();
        let result = stack.create_components_native(
            &definition(vec![row(1, *b"cniw", [1.0; 4], 7, VISIBLE)]),
            CONTEXT,
            &mut resources,
        );
        assert_eq!(result, Err("text failure".to_owned()));
        assert_eq!(stack.len(), 1);
        assert_eq!(
            stack.native.as_ref().unwrap().phase,
            NativeMenuPhase::Creating
        );
        assert_eq!(stack.native.as_ref().unwrap().root, None);
        assert_eq!(
            stack.components[0].native.as_ref().unwrap().phase,
            NativeComponentPhase::Constructing
        );
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(resources.calls, vec![ResourceCall::Text(7)]);
        stack.destroy_native(&mut resources).unwrap();
        assert_eq!(stack.native_root_key(), Ok(None));
    }

    #[test]
    fn native_root_key_preserves_assigned_root_when_later_construction_fails() {
        for fail_text in [None, Some(8)] {
            let mut resources = Resources::new();
            resources.fail_text = fail_text;
            let mut stack = MenuStack::new();
            let result = stack.create_components_native(
                &definition(vec![root(), row(2, *b"tcer", [1.0; 4], 8, VISIBLE)]),
                CONTEXT,
                &mut resources,
            );
            let expected_phase = if fail_text.is_some() {
                assert_eq!(result, Err("text failure".to_owned()));
                NativeMenuPhase::Creating
            } else {
                assert_eq!(result, Ok(()));
                NativeMenuPhase::Ready
            };
            assert_eq!(stack.len(), 2);
            let root_key = stack.components[0].native_key().unwrap();
            assert_eq!(stack.native.as_ref().unwrap().phase, expected_phase);
            assert_eq!(stack.native.as_ref().unwrap().root, Some(root_key));
            assert_eq!(stack.native_root_key(), Ok(Some(root_key)));
            assert_eq!(resources.calls, vec![ResourceCall::Text(8)]);
            stack.destroy_native(&mut resources).unwrap();
            assert_eq!(stack.native_root_key(), Ok(None));
        }
    }

    #[test]
    fn constructors_use_real_file_tags_flags_text_and_independent_scale() {
        let bytes: Arc<[u8]> = Arc::from([0x81, 0xff, b'X']);
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::clone(&bytes));
        let mut stack = MenuStack::new();
        let tags = [*b"cniw", *b"tcer", *b"ttcr", *b"txts", *b"rabp"];
        let kinds = [
            MenuComponentKind::WindowClean,
            MenuComponentKind::Rectangle,
            MenuComponentKind::RectangleTexture,
            MenuComponentKind::StaticText,
            MenuComponentKind::ProgressBar,
        ];
        let items = tags
            .into_iter()
            .enumerate()
            .map(|(index, kind)| row(index as u32 + 1, kind, [1.5, 2.5, 3.5, 4.5], 7, 0xa500_03d3))
            .collect();
        stack
            .create_components_native(
                &definition(items),
                MenuComponentCreateContext {
                    screen_scale: [2.0, 3.0],
                    left_margin_x: 11.0,
                    precision: MenuComponentPrecision::Pc24Nearest,
                },
                &mut resources,
            )
            .unwrap();
        assert_eq!(stack.len(), 5);
        assert_eq!(
            resources.calls,
            (0..5).map(|_| ResourceCall::Text(7)).collect::<Vec<_>>()
        );
        for (index, component) in stack.components.iter().enumerate() {
            assert_eq!(component.flags, 0xa500_03d3);
            assert_eq!(component.text_id, 7);
            assert_eq!(component.native_kind(), Some(kinds[index]));
            assert!(Arc::ptr_eq(
                component.native.as_ref().unwrap().text.as_ref().unwrap(),
                &bytes
            ));
            assert_eq!(
                component.native_geometry().unwrap(),
                MenuComponentGeometry {
                    position: [3.0, 7.5],
                    size: [7.0, 13.5],
                    relative_position: [if index == 0 { 14.0 } else { 3.0 }, 7.5],
                }
            );
        }
        assert_eq!(stack.progress_value(5), Some(0.0));
        assert_eq!(
            stack
                .find_component_by_id(3)
                .unwrap()
                .texture_key()
                .unwrap(),
            None
        );
        assert!(stack.staged_texts.is_empty());
        assert!(stack.staged_strings.is_empty());
        let mut sink = Sink::default();
        stack
            .draw_components_native(u32::MAX, None, &mut sink)
            .unwrap();
        assert_eq!(geometry(&stack, 1).position, [14.0, 7.5]);
        for id in 2..=5 {
            assert_eq!(geometry(&stack, id).position, [17.0, 15.0]);
        }
    }

    #[test]
    fn parent_links_are_allocation_scoped_and_upup_returns_all_the_way_to_root() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![
                root(),
                row(2, *b"cniw", [1.0, 2.0, 30.0, 40.0], 0, VISIBLE),
                row(2, *b"cniw", [3.0, 4.0, 20.0, 20.0], 0, VISIBLE),
                row(3, *b"tcer", [5.0, 6.0, 10.0, 10.0], 0, VISIBLE),
                row(99, *b"upup", [f32::NAN; 4], 99, u32::MAX),
                row(4, *b"txts", [7.0, 8.0, 10.0, 10.0], 0, VISIBLE),
            ],
            &mut resources,
        );
        assert_eq!(stack.len(), 5);
        let keys: Vec<_> = stack
            .components
            .iter()
            .map(|component| component.native_key().unwrap())
            .collect();
        assert_ne!(keys[1], keys[2]);
        assert_eq!(
            stack.components[0].children,
            vec![
                MenuComponentChild::Native(keys[1]),
                MenuComponentChild::Native(keys[4])
            ]
        );
        assert_eq!(
            stack.components[1].children,
            vec![MenuComponentChild::Native(keys[2])]
        );
        assert_eq!(
            stack.components[2].children,
            vec![MenuComponentChild::Native(keys[3])]
        );
        assert_eq!(stack.components[3].native_parent(), Some(keys[2]));
        assert_eq!(stack.components[4].native_parent(), Some(keys[0]));
        assert_eq!(
            stack.find_component_by_id(2).unwrap().native_key(),
            Some(keys[1])
        );
        stack
            .draw_components_native(u32::MAX, None, &mut Sink::default())
            .unwrap();
        assert_eq!(
            stack.components[2].native_geometry().unwrap().position,
            [14.0, 26.0]
        );
        assert_eq!(geometry(&stack, 3).position, [19.0, 32.0]);
        assert_eq!(geometry(&stack, 4).position, [17.0, 28.0]);
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn text_lookup_failure_keeps_the_constructing_allocation_and_completed_prefix() {
        let mut resources = Resources::new();
        resources.fail_text = Some(8);
        let mut stack = MenuStack::new();
        let result = stack.create_components_native(
            &definition(vec![
                root(),
                row(2, *b"txts", [1.0; 4], 8, VISIBLE),
                row(3, *b"tcer", [2.0; 4], 0, VISIBLE),
            ]),
            CONTEXT,
            &mut resources,
        );
        assert_eq!(result, Err("text failure".to_owned()));
        assert_eq!(stack.len(), 2);
        assert_eq!(
            stack.components[0].native.as_ref().unwrap().phase,
            NativeComponentPhase::Live
        );
        assert!(stack.components[0].children.is_empty());
        assert_eq!(stack.components[1].text_id, 8);
        assert_eq!(
            stack.components[1].native.as_ref().unwrap().phase,
            NativeComponentPhase::Constructing
        );
        assert_eq!(resources.calls, vec![ResourceCall::Text(8)]);
        assert!(stack
            .draw_components_native(0, None, &mut Sink::default())
            .is_err());
        assert!(stack
            .create_components_native(&definition(vec![]), CONTEXT, &mut resources)
            .is_err());
        stack.destroy_native(&mut resources).unwrap();
        assert!(stack.is_empty());
        assert!(stack.native.is_none());
    }

    #[test]
    fn unsupported_classes_are_errors_not_substitutes_or_successful_skips() {
        for unsupported in [*b"dniw", *b"ttub", *b"xxxx", *b"zero"] {
            let mut resources = Resources::new();
            let mut stack = MenuStack::new();
            let result = stack.create_components_native(
                &definition(vec![
                    root(),
                    row(2, unsupported, [1.0; 4], 9, VISIBLE),
                    row(3, *b"rabp", [1.0; 4], 0, VISIBLE),
                ]),
                CONTEXT,
                &mut resources,
            );
            assert!(result
                .unwrap_err()
                .contains("menu row 1: unsupported native menu component"));
            assert_eq!(stack.len(), 1);
            assert!(resources.calls.is_empty());
            stack.destroy_native(&mut resources).unwrap();
        }
    }

    #[test]
    fn text_id_latches_before_lookup_and_zero_never_queries_database() {
        let bytes: Arc<[u8]> = Arc::from([0x81, 0xff]);
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::clone(&bytes));
        let mut stack = create(
            vec![root(), row(2, *b"txts", [1.0; 4], 7, VISIBLE)],
            &mut resources,
        );
        resources.calls.clear();
        resources.fail_text = Some(8);
        assert_eq!(
            stack.set_text_id_native(2, 8, &mut resources),
            Err("text failure".to_owned())
        );
        assert_eq!(stack.get_text_id(2), 8);
        assert!(Arc::ptr_eq(
            stack.components[1]
                .native
                .as_ref()
                .unwrap()
                .text
                .as_ref()
                .unwrap(),
            &bytes
        ));
        stack.set_text_id_native(2, 0, &mut resources).unwrap();
        stack.set_text_id_native(999, 8, &mut resources).unwrap();
        assert_eq!(stack.get_text_id(2), 0);
        assert_eq!(stack.get_text(2), None);
        assert_eq!(resources.calls, vec![ResourceCall::Text(8)]);
        stack.set_text_id_native(2, 9, &mut resources).unwrap();
        assert_eq!(stack.get_text_id(2), 9);
        assert_eq!(stack.get_text(2), None);
        assert_eq!(
            resources.calls,
            vec![ResourceCall::Text(8), ResourceCall::Text(9)]
        );
    }

    #[test]
    fn pictures_release_before_create_and_native_none_is_not_a_host_error() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"ttcr", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        stack
            .set_string_native(2, Some(b"first.tga"), &mut resources)
            .unwrap();
        let first = picture_key(&stack, 2);
        stack
            .set_string_native(2, Some(b"second.tga"), &mut resources)
            .unwrap();
        let second = picture_key(&stack, 2);
        assert_ne!(first, second);
        resources.create_none = true;
        stack
            .set_string_native(2, Some(b"missing.tga"), &mut resources)
            .unwrap();
        assert_eq!(stack.components[1].texture_key().unwrap(), None);
        assert_eq!(
            resources.calls,
            vec![
                ResourceCall::Create(b"first.tga".to_vec()),
                ResourceCall::Release(first),
                ResourceCall::Create(b"second.tga".to_vec()),
                ResourceCall::Release(second),
                ResourceCall::Create(b"missing.tga".to_vec()),
            ]
        );
        assert!(stack.staged_strings.is_empty());
        assert_eq!(stack.staged_string(2), None);
        stack.destroy_native(&mut resources).unwrap();
        assert_eq!(resources.calls.len(), 5);
    }

    #[test]
    fn null_picture_name_is_unsupported_only_after_successful_old_release() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"ttcr", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        stack
            .set_string_native(2, Some(b"old.tga"), &mut resources)
            .unwrap();
        let old = picture_key(&stack, 2);
        resources.calls.clear();
        assert!(stack
            .set_string_native(2, None, &mut resources)
            .unwrap_err()
            .contains("NULL primary name"));
        assert_eq!(resources.calls, vec![ResourceCall::Release(old)]);
        assert_eq!(stack.components[1].texture_key().unwrap(), None);
        stack.set_string_native(1, None, &mut resources).unwrap();
        stack.set_string_native(999, None, &mut resources).unwrap();
        assert_eq!(resources.calls, vec![ResourceCall::Release(old)]);
    }

    #[test]
    fn host_factory_failure_keeps_the_completed_release_without_old_texture_replay() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"ttcr", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        stack
            .set_string_native(2, Some(b"old.tga"), &mut resources)
            .unwrap();
        let old = picture_key(&stack, 2);
        resources.calls.clear();
        resources.fail_create = true;
        assert_eq!(
            stack.set_string_native(2, Some(b"new.tga"), &mut resources),
            Err("factory failure".to_owned())
        );
        assert_eq!(stack.components[1].texture_key().unwrap(), None);
        assert_eq!(
            resources.calls,
            vec![
                ResourceCall::Release(old),
                ResourceCall::Create(b"new.tga".to_vec())
            ]
        );
        stack
            .draw_components_native(0, None, &mut Sink::default())
            .unwrap();
        stack.destroy_native(&mut resources).unwrap();
        assert_eq!(resources.calls.len(), 2);
    }

    #[test]
    fn uncertain_release_keeps_interrupted_key_but_never_draws_or_replays_it() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"ttcr", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        stack
            .set_string_native(2, Some(b"old.tga"), &mut resources)
            .unwrap();
        let old = picture_key(&stack, 2);
        resources.calls.clear();
        resources.fail_release = Some(old);
        assert_eq!(
            stack.set_string_native(2, Some(b"new.tga"), &mut resources),
            Err("uncertain release failure".to_owned())
        );
        assert_eq!(stack.components[1].interrupted_texture_key(), Some(&old));
        assert!(stack.components[1].texture_key().is_err());
        let mut sink = Sink::default();
        assert!(stack.draw_components_native(0, None, &mut sink).is_err());
        assert!(sink.commands.is_empty());
        assert!(stack
            .set_string_native(2, Some(b"retry.tga"), &mut resources)
            .is_err());
        assert!(stack.destroy_native(&mut resources).is_err());
        assert!(stack.destroy_native(&mut resources).is_err());
        assert_eq!(stack.components[1].interrupted_texture_key(), Some(&old));
        assert_eq!(resources.calls, vec![ResourceCall::Release(old)]);
    }

    #[test]
    fn destruction_keeps_completed_creation_order_prefix_and_later_live_owner() {
        let bytes: Arc<[u8]> = Arc::from([0x80]);
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::clone(&bytes));
        let mut root = root();
        root.text_id = 7;
        let mut stack = create(
            vec![
                root,
                row(2, *b"ttcr", [1.0; 4], 7, VISIBLE),
                row(3, *b"ttcr", [1.0; 4], 7, VISIBLE),
                row(4, *b"ttcr", [1.0; 4], 7, VISIBLE),
            ],
            &mut resources,
        );
        for id in 2..=4 {
            stack
                .set_string_native(id, Some(b"image.tga"), &mut resources)
                .unwrap();
        }
        let first = picture_key(&stack, 2);
        let second = picture_key(&stack, 3);
        let third = picture_key(&stack, 4);
        resources.calls.clear();
        resources.fail_release = Some(second);
        assert_eq!(
            stack.destroy_native(&mut resources),
            Err("uncertain release failure".to_owned())
        );
        assert_eq!(
            resources.calls,
            vec![ResourceCall::Release(first), ResourceCall::Release(second)]
        );
        assert_eq!(stack.native.as_ref().unwrap().next_destroy, 2);
        assert!(stack.find_component_by_id(1).is_none());
        assert!(stack.find_component_by_id(2).is_none());
        assert!(stack.components[0].native.as_ref().unwrap().text.is_none());
        assert!(stack.components[1].native.as_ref().unwrap().text.is_none());
        assert_eq!(stack.components[2].interrupted_texture_key(), Some(&second));
        assert_eq!(picture_key(&stack, 4), third);
        assert_eq!(Arc::strong_count(&bytes), 4);
        assert!(stack.destroy_native(&mut resources).is_err());
        assert_eq!(
            resources.calls,
            vec![ResourceCall::Release(first), ResourceCall::Release(second)]
        );
        assert!(stack
            .draw_components_native(0, None, &mut Sink::default())
            .is_err());
        assert!(stack.set_text_id_native(4, 0, &mut resources).is_err());
    }

    #[test]
    fn successful_destroy_releases_every_picture_and_clears_only_after_ok() {
        let bytes: Arc<[u8]> = Arc::from(*b"A");
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::clone(&bytes));
        let mut stack = create(
            vec![root(), row(2, *b"ttcr", [1.0; 4], 7, VISIBLE)],
            &mut resources,
        );
        let old_root = stack.components[0].native_key().unwrap();
        stack
            .set_string_native(2, Some(b"image.tga"), &mut resources)
            .unwrap();
        let texture = picture_key(&stack, 2);
        assert_eq!(Arc::strong_count(&bytes), 3);
        resources.calls.clear();
        stack.destroy_native(&mut resources).unwrap();
        assert_eq!(resources.calls, vec![ResourceCall::Release(texture)]);
        assert_eq!(stack, MenuStack::new());
        assert_eq!(Arc::strong_count(&bytes), 2);
        stack
            .create_components_native(&definition(vec![root()]), CONTEXT, &mut resources)
            .unwrap();
        assert_ne!(stack.components[0].native_key().unwrap(), old_root);
        stack.components[0]
            .children
            .push(MenuComponentChild::Native(old_root));
        assert!(stack
            .draw_components_native(0, None, &mut Sink::default())
            .unwrap_err()
            .contains("foreign, stale"));
    }

    #[test]
    fn every_after_move_finishes_before_first_sink_call_even_for_hidden_subtrees() {
        let mut resources = Resources::new();
        let mut stack = MenuStack::new();
        stack
            .create_components_native(
                &definition(vec![
                    root(),
                    row(2, *b"tcer", [1.0, 2.0, 10.0, 10.0], 0, VISIBLE | 0x200),
                    row(3, *b"cniw", [3.0, 4.0, 10.0, 10.0], 0, 0),
                    row(4, *b"txts", [5.0, 6.0, 10.0, 10.0], 0, VISIBLE),
                    row(99, *b"upup", [0.0; 4], 0, 0),
                    row(5, *b"tcer", [7.0, 8.0, 10.0, 10.0], 0, VISIBLE | 0x200),
                ]),
                MenuComponentCreateContext {
                    screen_scale: [2.0, 3.0],
                    left_margin_x: 5.0,
                    precision: MenuComponentPrecision::Pc24Nearest,
                },
                &mut resources,
            )
            .unwrap();
        let mut sink = Sink {
            fail_at: Some(0),
            ..Sink::default()
        };
        assert_eq!(
            stack.draw_components_native(u32::MAX, None, &mut sink),
            Err("sink failure".to_owned())
        );
        assert_eq!(sink.attempts, 1);
        assert!(sink.commands.is_empty());
        for (id, position) in [
            (1, [25.0, 60.0]),
            (2, [27.0, 66.0]),
            (3, [31.0, 72.0]),
            (4, [41.0, 90.0]),
            (5, [39.0, 84.0]),
        ] {
            assert_eq!(geometry(&stack, id).position, position);
        }
        let mut sink = Sink::default();
        stack
            .draw_components_native(u32::MAX, None, &mut sink)
            .unwrap();
        assert_eq!(
            sink.commands.iter().map(Command::id).collect::<Vec<_>>(),
            vec![2, 5]
        );
        assert_eq!(geometry(&stack, 4).position, [41.0, 90.0]);
    }

    #[test]
    fn invisible_root_still_moves_its_whole_tree_but_draws_nothing() {
        let mut resources = Resources::new();
        let mut root = root();
        root.style = 0;
        let mut stack = create(
            vec![root, row(2, *b"tcer", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        let mut sink = Sink::default();
        stack
            .draw_components_native(u32::MAX, None, &mut sink)
            .unwrap();
        assert_eq!(geometry(&stack, 2).position, [11.0, 21.0]);
        assert!(sink.commands.is_empty());
        stack.set_hidden(1, true);
        stack
            .draw_components_native(u32::MAX, None, &mut sink)
            .unwrap();
        assert_eq!(
            sink.commands.iter().map(Command::id).collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn rectangle_styles_preserve_native_colors_topology_vertices_and_uvs() {
        for (style, color) in [
            (0, 0xff00_0000),
            (0x100, 0x7f00_0000),
            (0x200, 0xff00_0000),
            (0x300, 0xffff_ffff),
        ] {
            let mut resources = Resources::new();
            let mut stack = create(
                vec![
                    root(),
                    row(2, *b"tcer", [1.0, 2.0, 8.0, 6.0], 0, VISIBLE | style),
                ],
                &mut resources,
            );
            let mut sink = Sink::default();
            stack
                .draw_components_native(u32::MAX, None, &mut sink)
                .unwrap();
            let (topology, vertices) = if style == 0 {
                (
                    MenuComponentPrimitiveTopology::LineStrip,
                    vec![
                        vertex(11.0, 22.0, [0.0, 0.0], color),
                        vertex(19.0, 22.0, [0.0, 0.0], color),
                        vertex(19.0, 28.0, [0.0, 0.0], color),
                        vertex(11.0, 28.0, [0.0, 0.0], color),
                        vertex(11.0, 22.0, [0.0, 0.0], color),
                    ],
                )
            } else {
                (
                    MenuComponentPrimitiveTopology::TriangleStrip,
                    vec![
                        vertex(11.0, 22.0, [0.0, 0.0], color),
                        vertex(19.0, 22.0, [1.0, 0.0], color),
                        vertex(11.0, 28.0, [0.0, 1.0], color),
                        vertex(19.0, 28.0, [1.0, 1.0], color),
                    ],
                )
            };
            assert_eq!(
                sink.commands,
                vec![Command::Primitive {
                    key: stack.find_component_by_id(2).unwrap().native_key().unwrap(),
                    id: 2,
                    draw_flags: 0xff00_0010,
                    topology,
                    vertices,
                    texture: None,
                }]
            );
        }
    }

    #[test]
    fn base_draw_filters_inherited_flags_without_replacing_own_text_style() {
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::from([0x81, 0xff, b'X']));
        let mut stack = create(
            vec![
                root(),
                row(2, *b"cniw", [1.0, 2.0, 10.0, 10.0], 0, VISIBLE | 0x100),
                row(
                    3,
                    *b"txts",
                    [3.0, 4.0, 8.0, 6.0],
                    7,
                    VISIBLE | 0x300 | 0xc0 | 0xb,
                ),
            ],
            &mut resources,
        );
        let mut sink = Sink::default();
        stack
            .draw_components_native(0xa5ff_ffff, None, &mut sink)
            .unwrap();
        assert_eq!(
            sink.commands,
            vec![Command::Text {
                key: stack.find_component_by_id(3).unwrap().native_key().unwrap(),
                id: 3,
                draw_flags: 0xa500_0010,
                bytes: vec![0x81, 0xff, b'X'],
                rect: [14.0, 26.0, 8.0, 6.0],
                argb: 0xff00_0000,
                text_flags: 2,
                font: 0xb,
            }]
        );
        assert_eq!(stack.find_component_by_id(3).unwrap().flags, 0x3db);
        sink.commands.clear();
        stack.draw_components_native(0, None, &mut sink).unwrap();
        let Command::Text {
            draw_flags,
            text_flags,
            font,
            argb,
            ..
        } = &sink.commands[0]
        else {
            panic!("expected text");
        };
        assert_eq!(
            (*draw_flags, *text_flags, *font, *argb),
            (0, 2, 0xb, 0xff00_0000)
        );
    }

    #[test]
    fn static_text_preserves_all_colors_unsigned_bytes_and_alignment_priority() {
        for (style, color) in [
            (0, 0xff7f_7f7f),
            (0x100, 0xffff_ffff),
            (0x200, 0xffff_ff00),
            (0x300, 0xff00_0000),
        ] {
            for (alignment, text_flags) in [(0, 0), (0x40, 4), (0x80, 2), (0xc0, 2)] {
                let mut resources = Resources::new();
                resources.texts.insert(7, Arc::from([0x81, 0xff, b'X']));
                let mut stack = create(
                    vec![
                        root(),
                        row(
                            2,
                            *b"txts",
                            [1.0, 2.0, 8.0, 6.0],
                            7,
                            VISIBLE | style | alignment | 9,
                        ),
                    ],
                    &mut resources,
                );
                let mut sink = Sink::default();
                stack
                    .draw_components_native(u32::MAX, None, &mut sink)
                    .unwrap();
                assert_eq!(
                    sink.commands,
                    vec![Command::Text {
                        key: stack.find_component_by_id(2).unwrap().native_key().unwrap(),
                        id: 2,
                        draw_flags: 0xff00_0010,
                        bytes: vec![0x81, 0xff, b'X'],
                        rect: [11.0, 22.0, 8.0, 6.0],
                        argb: color,
                        text_flags,
                        font: 9,
                    }]
                );
            }
        }
    }

    #[test]
    fn null_static_text_is_not_an_owned_empty_string() {
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::from([]));
        let mut stack = create(
            vec![root(), row(2, *b"txts", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        let mut sink = Sink::default();
        stack.draw_components_native(0, None, &mut sink).unwrap();
        assert!(sink.commands.is_empty());
        stack.set_text_id_native(2, 7, &mut resources).unwrap();
        stack.draw_components_native(0, None, &mut sink).unwrap();
        assert_eq!(sink.commands.len(), 1);
        let Command::Text { bytes, .. } = &sink.commands[0] else {
            panic!("expected empty text");
        };
        assert!(bytes.is_empty());
        stack.set_text_id_native(2, 0, &mut resources).unwrap();
        stack.draw_components_native(0, None, &mut sink).unwrap();
        assert_eq!(sink.commands.len(), 1);
    }

    #[test]
    fn scalar_setters_return_errors_without_unwind_or_changes_after_partial_create() {
        let progress_id = crate::menu::LOADPROGRESS_BAR_ID;
        let root_text: Arc<[u8]> = Arc::from([0x81, 0xff]);
        let mut resources = Resources::new();
        resources.texts.insert(8, Arc::clone(&root_text));
        resources.fail_text = Some(7);
        let mut root_row = root();
        root_row.text_id = 8;
        let mut stack = MenuStack::new();
        assert_eq!(
            stack.create_components_native(
                &definition(vec![
                    root_row,
                    row(progress_id, *b"rabp", [1.0, 2.0, 8.0, 6.0], 7, VISIBLE),
                ]),
                CONTEXT,
                &mut resources,
            ),
            Err("text failure".to_owned())
        );
        let root_key = stack.native_root_key().unwrap().unwrap();
        let progress_key = stack.components[1].native_key().unwrap();
        let root_geometry = geometry(&stack, 1);
        let progress_geometry = geometry(&stack, progress_id);
        assert_eq!(
            resources.calls,
            vec![ResourceCall::Text(8), ResourceCall::Text(7)]
        );

        let direct_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            stack.components[1].set_native_float(0.875)
        }));
        assert!(direct_result.is_ok(), "partial component setter unwound");
        assert_eq!(
            direct_result.unwrap(),
            Err("native scalar setter requires a live component".to_owned())
        );
        let results = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            [
                stack.set_float(progress_id, 0.75),
                stack.set_float(1, 0.5),
                stack.set_float(u32::MAX, 0.25),
            ]
        }));
        assert!(results.is_ok(), "partial menu scalar setter unwound");
        for result in results.unwrap() {
            assert_eq!(
                result,
                Err("native menu component construction/destruction is incomplete".to_owned())
            );
        }

        assert_eq!(stack.len(), 2);
        assert!(stack.has_native_lifecycle());
        assert_eq!(stack.native_root_key(), Ok(Some(root_key)));
        assert_eq!(stack.components[0].native_key(), Some(root_key));
        assert_eq!(stack.components[1].native_key(), Some(progress_key));
        assert_eq!(geometry(&stack, 1), root_geometry);
        assert_eq!(geometry(&stack, progress_id), progress_geometry);
        assert_eq!(stack.components[1].text_id, 7);
        assert_eq!(stack.progress_value(progress_id).unwrap().to_bits(), 0);
        assert_eq!(stack.get_float(progress_id).to_bits(), 0);
        assert!(stack.staged_floats.is_empty());
        assert!(stack.components[0].children.is_empty());
        assert!(stack.components[1].children.is_empty());
        let native_root = stack.components[0].native.as_ref().unwrap();
        assert_eq!(native_root.phase, NativeComponentPhase::Live);
        assert!(Arc::ptr_eq(native_root.text.as_ref().unwrap(), &root_text));
        let native_progress = stack.components[1].native.as_ref().unwrap();
        assert_eq!(native_progress.phase, NativeComponentPhase::Constructing);
        assert_eq!(native_progress.parent, None);
        assert!(native_progress.text.is_none());
        assert_eq!(
            stack.native.as_ref(),
            Some(&NativeMenuState {
                owner: root_key.owner,
                root: Some(root_key),
                phase: NativeMenuPhase::Creating,
                next_destroy: 0,
                draw_liveness: Some(Arc::new(())),
            })
        );
        assert_eq!(
            resources.calls,
            vec![ResourceCall::Text(8), ResourceCall::Text(7)]
        );
        stack.destroy_native(&mut resources).unwrap();
        assert!(!stack.has_native_lifecycle());
        assert_eq!(stack, MenuStack::new());
        assert_eq!(
            resources.calls,
            vec![ResourceCall::Text(8), ResourceCall::Text(7)]
        );
    }

    #[test]
    fn scalar_setter_preserves_interrupted_destroy_prefix_without_unwind_or_replay() {
        let progress_id = crate::menu::LOADPROGRESS_BAR_ID;
        let mut resources = Resources::new();
        let mut stack = create(
            vec![
                root(),
                row(2, *b"ttcr", [1.0; 4], 0, VISIBLE),
                row(progress_id, *b"rabp", [1.0; 4], 0, VISIBLE),
            ],
            &mut resources,
        );
        stack.set_float(progress_id, 0.25).unwrap();
        stack
            .set_string_native(2, Some(b"owned.bin"), &mut resources)
            .unwrap();
        let texture_key = picture_key(&stack, 2);
        let root_key = stack.native_root_key().unwrap().unwrap();
        resources.fail_release = Some(texture_key);
        assert_eq!(
            stack.destroy_native(&mut resources),
            Err("uncertain release failure".to_owned())
        );

        let results = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            [
                stack.set_float(progress_id, 0.75),
                stack.set_float(1, 0.5),
                stack.set_float(u32::MAX, 0.25),
            ]
        }));
        assert!(results.is_ok(), "interrupted destroy scalar setter unwound");
        for result in results.unwrap() {
            assert_eq!(
                result,
                Err("native menu component construction/destruction is incomplete".to_owned())
            );
        }

        assert!(stack.has_native_lifecycle());
        assert_eq!(stack.len(), 3);
        assert_eq!(stack.native_root_key(), Ok(Some(root_key)));
        assert_eq!(
            stack.progress_value(progress_id).unwrap().to_bits(),
            0.25f32.to_bits()
        );
        assert!(stack.staged_floats.is_empty());
        assert_eq!(
            stack.components[0].native.as_ref().unwrap().phase,
            NativeComponentPhase::Destroyed
        );
        assert_eq!(
            stack.components[2].native.as_ref().unwrap().phase,
            NativeComponentPhase::Live
        );
        assert_eq!(
            stack.components[1].interrupted_texture_key(),
            Some(&texture_key)
        );
        assert_eq!(
            stack.native.as_ref(),
            Some(&NativeMenuState {
                owner: root_key.owner,
                root: Some(root_key),
                phase: NativeMenuPhase::Destroying,
                next_destroy: 1,
                draw_liveness: None,
            })
        );
        assert_eq!(
            resources.calls,
            vec![
                ResourceCall::Create(b"owned.bin".to_vec()),
                ResourceCall::Release(texture_key),
            ]
        );
    }

    #[test]
    fn live_scalar_setter_preserves_bits_and_native_nonprogress_noops() {
        let progress_id = crate::menu::LOADPROGRESS_BAR_ID;
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(progress_id, *b"rabp", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        for bits in [
            0x8000_0000,
            0x0000_0001,
            0xbfc0_0000,
            0x3fc0_0000,
            0x7fc0_1234,
        ] {
            assert_eq!(stack.set_float(progress_id, f32::from_bits(bits)), Ok(()));
            assert_eq!(stack.progress_value(progress_id).unwrap().to_bits(), bits);
            assert_eq!(stack.get_float(progress_id).to_bits(), 0);
            assert_eq!(stack.set_float(1, 0.75), Ok(()));
            assert_eq!(stack.set_float(u32::MAX, 0.5), Ok(()));
            assert_eq!(stack.progress_value(progress_id).unwrap().to_bits(), bits);
            assert!(stack.staged_floats.is_empty());
        }
        assert!(resources.calls.is_empty());
        stack.destroy_native(&mut resources).unwrap();
        assert!(!stack.has_native_lifecycle());
    }

    #[test]
    fn progress_draws_grey_then_unclamped_red_with_the_same_global_texture_identity() {
        let mut resources = Resources::new();
        let progress_texture = resources.textures.create_texture().unwrap();
        let mut stack = create(
            vec![root(), row(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0, VISIBLE)],
            &mut resources,
        );
        for (part, right) in [(-0.5, 7.0), (0.0, 11.0), (1.5, 23.0)] {
            stack.set_float(2, part).unwrap();
            assert_eq!(stack.get_float(2).to_bits(), 0);
            assert_eq!(stack.progress_value(2), Some(part));
            let mut sink = Sink::default();
            stack
                .draw_components_native(0, Some(&progress_texture), &mut sink)
                .unwrap();
            let key = stack.find_component_by_id(2).unwrap().native_key().unwrap();
            assert_eq!(
                sink.commands,
                vec![
                    Command::Primitive {
                        key,
                        id: 2,
                        draw_flags: 0,
                        topology: MenuComponentPrimitiveTopology::TriangleStrip,
                        vertices: vec![
                            vertex(11.0, 22.0, [0.0, 0.0], 0x5f7f_7f7f),
                            vertex(19.0, 22.0, [1.0, 0.0], 0x5f7f_7f7f),
                            vertex(11.0, 28.0, [0.0, 1.0], 0x5f7f_7f7f),
                            vertex(19.0, 28.0, [1.0, 1.0], 0x5f7f_7f7f),
                        ],
                        texture: Some(progress_texture),
                    },
                    Command::Primitive {
                        key,
                        id: 2,
                        draw_flags: 0,
                        topology: MenuComponentPrimitiveTopology::TriangleStrip,
                        vertices: vec![
                            vertex(11.0, 22.0, [0.0, 0.0], 0xffff_0000),
                            vertex(right, 22.0, [part, 0.0], 0xffff_0000),
                            vertex(11.0, 28.0, [0.0, 1.0], 0xffff_0000),
                            vertex(right, 28.0, [part, 1.0], 0xffff_0000),
                        ],
                        texture: Some(progress_texture),
                    },
                ]
            );
        }
        struct IdentitySink<'a> {
            expected: &'a TextureKey,
            component: MenuComponentKey,
            count: usize,
        }
        impl MenuComponentDrawSink for IdentitySink<'_> {
            fn primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String> {
                assert_eq!(primitive.texture, Some(self.expected));
                assert_eq!(primitive.component, self.component);
                assert_eq!(primitive.component_id, 2);
                self.count += 1;
                Ok(())
            }
            fn text(&mut self, _: MenuComponentText<'_>) -> Result<(), String> {
                panic!("progress must not emit text");
            }
        }
        let mut sink = IdentitySink {
            expected: &progress_texture,
            component: stack.find_component_by_id(2).unwrap().native_key().unwrap(),
            count: 0,
        };
        stack
            .draw_components_native(0, Some(&progress_texture), &mut sink)
            .unwrap();
        assert_eq!(sink.count, 2);
        assert!(resources.calls.is_empty());
        assert!(stack.staged_floats.is_empty());
    }

    #[test]
    fn mixed_components_emit_depth_first_commands_without_detached_texture_owners() {
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::from([0xff, b'X']));
        let mut stack = create(
            vec![
                root(),
                row(2, *b"tcer", [1.0; 4], 0, VISIBLE | 0x200),
                row(3, *b"ttcr", [2.0; 4], 0, VISIBLE),
                row(4, *b"cniw", [3.0; 4], 0, VISIBLE),
                row(5, *b"txts", [4.0; 4], 7, VISIBLE),
                row(6, *b"rabp", [5.0; 4], 0, VISIBLE),
                row(99, *b"upup", [0.0; 4], 0, 0),
                row(7, *b"tcer", [6.0; 4], 0, VISIBLE | 0x300),
            ],
            &mut resources,
        );
        stack
            .set_string_native(3, Some(b"image.tga"), &mut resources)
            .unwrap();
        let picture = picture_key(&stack, 3);
        resources.calls.clear();
        let mut sink = Sink::default();
        stack
            .draw_components_native(u32::MAX, None, &mut sink)
            .unwrap();
        assert_eq!(
            sink.commands.iter().map(Command::id).collect::<Vec<_>>(),
            vec![2, 3, 5, 6, 6, 7]
        );
        let Command::Primitive {
            texture, vertices, ..
        } = &sink.commands[1]
        else {
            panic!("expected picture");
        };
        assert_eq!(*texture, Some(picture));
        assert_eq!(
            *vertices,
            vec![
                vertex(12.0, 22.0, [0.0, 0.0], 0xffff_ffff),
                vertex(14.0, 22.0, [1.0, 0.0], 0xffff_ffff),
                vertex(12.0, 24.0, [0.0, 1.0], 0xffff_ffff),
                vertex(14.0, 24.0, [1.0, 1.0], 0xffff_ffff),
            ]
        );
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn incremental_callbacks_borrow_actual_owner_and_read_late_progress_arguments() {
        let mut resources = Resources::new();
        let gray_texture = resources.textures.create_texture().unwrap();
        let red_texture = resources.textures.create_texture().unwrap();
        let mut stack = create(
            vec![
                root(),
                row(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0, VISIBLE),
                row(3, *b"cniw", [3.0, 4.0, 10.0, 10.0], 0, 0),
                row(4, *b"tcer", [5.0, 6.0, 10.0, 10.0], 0, VISIBLE | 0x200),
            ],
            &mut resources,
        );
        stack.set_float(2, 0.25).unwrap();
        let key = stack.find_component_by_id(2).unwrap().native_key().unwrap();
        let allocation = std::ptr::from_ref(stack.find_component_by_id(2).unwrap());
        let stack = Rc::new(RefCell::new(stack));
        let alias = Rc::clone(&stack);
        let texture = Cell::new(gray_texture);
        let calls = Cell::new(0);
        let mut sink = CallbackSink {
            callback: |primitive: &MenuComponentPrimitive<'_>| {
                {
                    let actual = alias
                        .try_borrow()
                        .expect("callback must permit owner reads");
                    assert_eq!(geometry(&actual, 3).position, [13.0, 24.0]);
                    assert_eq!(geometry(&actual, 4).position, [18.0, 30.0]);
                    assert!(std::ptr::eq(
                        std::ptr::from_ref(actual.find_component_by_id(2).unwrap()),
                        allocation,
                    ));
                }
                if calls.get() == 0 {
                    assert_eq!(primitive.component, key);
                    assert_eq!(primitive.component_id, 2);
                    assert_eq!(primitive.texture.copied(), Some(gray_texture));
                    let mut actual = alias.try_borrow_mut().expect("callback must permit writes");
                    actual.set_float(2, 0.75).unwrap();
                    let native = actual
                        .find_component_by_id_mut(2)
                        .unwrap()
                        .native
                        .as_mut()
                        .unwrap();
                    native.geometry.position = [31.0, 42.0];
                    native.geometry.size = [12.0, 10.0];
                    actual.find_component_by_id_mut(3).unwrap().flags |= VISIBLE;
                    texture.set(red_texture);
                } else if calls.get() == 1 {
                    assert_eq!(primitive.component, key);
                    assert_eq!(primitive.texture.copied(), Some(red_texture));
                    assert_eq!(alias.borrow().progress_value(2), Some(0.75));
                }
                calls.set(calls.get() + 1);
                Ok(())
            },
            recorded: Sink::default(),
        };
        let mut cursor = stack.borrow_mut().begin_components_draw_native(0).unwrap();
        loop {
            let current_texture = texture.get();
            let call = stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, Some(&current_texture))
                .unwrap();
            let Some(call) = call else { break };
            cursor.complete_call(call.dispatch(&mut sink)).unwrap();
        }
        assert_eq!(calls.get(), 3);
        assert_eq!(
            sink.recorded
                .commands
                .iter()
                .map(Command::id)
                .collect::<Vec<_>>(),
            vec![2, 2, 4]
        );
        assert_eq!(
            &sink.recorded.commands[..2],
            &[
                Command::Primitive {
                    key,
                    id: 2,
                    draw_flags: 0,
                    topology: MenuComponentPrimitiveTopology::TriangleStrip,
                    vertices: vec![
                        vertex(11.0, 22.0, [0.0, 0.0], 0x5f7f_7f7f),
                        vertex(19.0, 22.0, [1.0, 0.0], 0x5f7f_7f7f),
                        vertex(11.0, 28.0, [0.0, 1.0], 0x5f7f_7f7f),
                        vertex(19.0, 28.0, [1.0, 1.0], 0x5f7f_7f7f),
                    ],
                    texture: Some(gray_texture),
                },
                Command::Primitive {
                    key,
                    id: 2,
                    draw_flags: 0,
                    topology: MenuComponentPrimitiveTopology::TriangleStrip,
                    vertices: vec![
                        vertex(31.0, 42.0, [0.0, 0.0], 0xffff_0000),
                        vertex(40.0, 42.0, [0.75, 0.0], 0xffff_0000),
                        vertex(31.0, 52.0, [0.0, 1.0], 0xffff_0000),
                        vertex(40.0, 52.0, [0.75, 1.0], 0xffff_0000),
                    ],
                    texture: Some(red_texture),
                },
            ]
        );
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn incremental_draw_keeps_entered_filter_and_reads_visibility_and_links_live() {
        let mut resources = Resources::new();
        let stack = RefCell::new(create(
            vec![
                row(1, *b"cniw", [10.0, 20.0, 100.0, 80.0], 0, VISIBLE | 0x200),
                row(2, *b"tcer", [1.0; 4], 0, VISIBLE | 0x200),
                row(3, *b"tcer", [2.0; 4], 0, 0x200),
                row(4, *b"tcer", [3.0; 4], 0, VISIBLE | 0x200),
                row(5, *b"tcer", [4.0; 4], 0, VISIBLE | 0x100),
            ],
            &mut resources,
        ));
        let mut sink = CallbackSink {
            callback: |primitive: &MenuComponentPrimitive<'_>| {
                let mut actual = stack
                    .try_borrow_mut()
                    .expect("no traversal borrow during callbacks");
                if primitive.component_id == 2 {
                    actual.find_component_by_id_mut(1).unwrap().flags = 0;
                    actual.find_component_by_id_mut(3).unwrap().flags |= VISIBLE;
                    actual
                        .find_component_by_id_mut(1)
                        .unwrap()
                        .children
                        .swap(1, 2);
                } else if primitive.component_id == 4 {
                    actual
                        .find_component_by_id_mut(1)
                        .unwrap()
                        .children
                        .swap(2, 3);
                    actual
                        .find_component_by_id_mut(1)
                        .unwrap()
                        .native
                        .as_mut()
                        .unwrap()
                        .geometry
                        .position = [90.0, 80.0];
                }
                Ok(())
            },
            recorded: Sink::default(),
        };
        draw_incremental(&stack, u32::MAX, None, &mut sink).unwrap();
        assert_eq!(
            sink.recorded
                .commands
                .iter()
                .map(Command::id)
                .collect::<Vec<_>>(),
            vec![2, 4, 5, 3]
        );
        let flags = sink
            .recorded
            .commands
            .iter()
            .map(|command| {
                let Command::Primitive { draw_flags, .. } = command else {
                    panic!("expected rectangle")
                };
                *draw_flags
            })
            .collect::<Vec<_>>();
        assert_eq!(
            flags,
            vec![0xff00_0210, 0xff00_0210, 0xff00_0010, 0xff00_0210]
        );
        assert_eq!(geometry(&stack.borrow(), 3).position, [12.0, 22.0]);
        assert_eq!(geometry(&stack.borrow(), 5).position, [14.0, 24.0]);
        assert_eq!(stack.borrow().find_component_by_id(1).unwrap().flags, 0);
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn incremental_text_call_pins_the_same_raw_allocation_across_owner_mutation() {
        struct TextSink<'owner> {
            stack: &'owner RefCell<MenuStack>,
            resources: &'owner RefCell<Resources>,
            original: &'owner Arc<[u8]>,
            replacement: &'owner Arc<[u8]>,
            recorded: Sink,
        }
        impl MenuComponentDrawSink for TextSink<'_> {
            fn primitive(&mut self, _: MenuComponentPrimitive<'_>) -> Result<(), String> {
                panic!("static text must not emit primitives");
            }

            fn text(&mut self, text: MenuComponentText<'_>) -> Result<(), String> {
                {
                    let actual = self
                        .stack
                        .try_borrow()
                        .expect("text callback must permit reads");
                    let component = actual.find_component_by_id(2).unwrap();
                    assert_eq!(text.component, component.native_key().unwrap());
                    assert_eq!(text.component_id, component.id);
                    assert!(std::ptr::eq(
                        text.bytes,
                        component.native.as_ref().unwrap().text.as_deref().unwrap()
                    ));
                    assert!(std::ptr::eq(text.bytes, self.original.as_ref()));
                    assert_eq!(Arc::strong_count(self.original), 4);
                }
                self.stack
                    .try_borrow_mut()
                    .expect("text callback must permit writes")
                    .set_text_id_native(2, 8, &mut *self.resources.borrow_mut())
                    .unwrap();
                assert_eq!(text.bytes, self.original.as_ref());
                assert_eq!(Arc::strong_count(self.original), 3);
                {
                    let actual = self.stack.borrow();
                    let component = actual.find_component_by_id(2).unwrap();
                    assert_eq!(component.text_id, 8);
                    assert!(std::ptr::eq(
                        component.native.as_ref().unwrap().text.as_deref().unwrap(),
                        self.replacement.as_ref()
                    ));
                }
                self.recorded.text(text)
            }
        }
        let original: Arc<[u8]> = Arc::from([0xff, 0x81, 0, b'X']);
        let replacement: Arc<[u8]> = Arc::from([b'Y', 0xfe]);
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::clone(&original));
        resources.texts.insert(8, Arc::clone(&replacement));
        let stack = RefCell::new(create(
            vec![
                root(),
                row(2, *b"txts", [1.0, 2.0, 3.0, 4.0], 7, VISIBLE | 0x87),
            ],
            &mut resources,
        ));
        let key = stack
            .borrow()
            .find_component_by_id(2)
            .unwrap()
            .native_key()
            .unwrap();
        let resources = RefCell::new(resources);
        assert_eq!(Arc::strong_count(&original), 3);
        let mut sink = TextSink {
            stack: &stack,
            resources: &resources,
            original: &original,
            replacement: &replacement,
            recorded: Sink::default(),
        };
        draw_incremental(&stack, 0, None, &mut sink).unwrap();
        assert_eq!(Arc::strong_count(&original), 2);
        assert_eq!(
            sink.recorded.commands,
            vec![Command::Text {
                key,
                id: 2,
                draw_flags: 0,
                bytes: vec![0xff, 0x81, 0, b'X'],
                rect: [11.0, 22.0, 3.0, 4.0],
                argb: 0xff7f_7f7f,
                text_flags: 0x2,
                font: 7,
            }]
        );
        assert_eq!(
            resources.borrow().calls,
            vec![ResourceCall::Text(7), ResourceCall::Text(8)]
        );
    }

    #[test]
    fn incremental_texture_call_has_identity_but_no_release_authority() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"ttcr", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        stack
            .set_string_native(2, Some(b"old.tga"), &mut resources)
            .unwrap();
        let old = picture_key(&stack, 2);
        let stack = RefCell::new(stack);
        let resources = RefCell::new(resources);
        let mut sink = CallbackSink {
            callback: |primitive: &MenuComponentPrimitive<'_>| {
                {
                    let actual = stack
                        .try_borrow()
                        .expect("texture callback must permit reads");
                    let component = actual.find_component_by_id(2).unwrap();
                    assert_eq!(primitive.component, component.native_key().unwrap());
                    assert_eq!(primitive.texture, component.texture_key().unwrap());
                    assert_eq!(
                        resources.borrow().calls,
                        vec![ResourceCall::Create(b"old.tga".to_vec())]
                    );
                }
                stack
                    .try_borrow_mut()
                    .expect("texture callback must permit writes")
                    .set_string_native(2, Some(b"new.tga"), &mut *resources.borrow_mut())
                    .unwrap();
                assert_eq!(primitive.texture.copied(), Some(old));
                assert_ne!(picture_key(&stack.borrow(), 2), old);
                Ok(())
            },
            recorded: Sink::default(),
        };
        draw_incremental(&stack, 0, None, &mut sink).unwrap();
        let new = picture_key(&stack.borrow(), 2);
        assert_eq!(sink.recorded.attempts, 1);
        let Command::Primitive { texture, .. } = &sink.recorded.commands[0] else {
            panic!("expected texture command")
        };
        assert_eq!(*texture, Some(old));
        stack
            .borrow_mut()
            .destroy_native(&mut *resources.borrow_mut())
            .unwrap();
        assert_eq!(
            resources.borrow().calls,
            vec![
                ResourceCall::Create(b"old.tga".to_vec()),
                ResourceCall::Release(old),
                ResourceCall::Create(b"new.tga".to_vec()),
                ResourceCall::Release(new),
            ]
        );
        assert!(!stack.borrow().has_native_lifecycle());
    }

    #[test]
    fn incremental_late_invalid_link_stops_after_prefix_without_replay() {
        let mut resources = Resources::new();
        let stack = RefCell::new(create(
            vec![
                root(),
                row(2, *b"tcer", [1.0; 4], 0, VISIBLE | 0x200),
                row(3, *b"tcer", [2.0; 4], 0, VISIBLE | 0x200),
            ],
            &mut resources,
        ));
        let child = stack
            .borrow()
            .find_component_by_id(3)
            .unwrap()
            .native_key()
            .unwrap();
        let mut cursor = stack.borrow_mut().begin_components_draw_native(0).unwrap();
        let mut sink = CallbackSink {
            callback: |primitive: &MenuComponentPrimitive<'_>| {
                assert_eq!(primitive.component_id, 2);
                stack
                    .try_borrow_mut()
                    .expect("callback must permit link mutation")
                    .find_component_by_id_mut(1)
                    .unwrap()
                    .children[1] = MenuComponentChild::Uncomposed(3);
                Ok(())
            },
            recorded: Sink::default(),
        };
        let call = stack
            .borrow_mut()
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        cursor.complete_call(call.dispatch(&mut sink)).unwrap();
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "uncomposed child link in native menu"
        );
        stack
            .borrow_mut()
            .find_component_by_id_mut(1)
            .unwrap()
            .children[1] = MenuComponentChild::Native(child);
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        assert_eq!(sink.recorded.attempts, 1);
        assert_eq!(
            sink.recorded
                .commands
                .iter()
                .map(Command::id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn incremental_destroy_recreate_in_gray_callback_stops_the_old_owner() {
        let mut resources = Resources::new();
        let replacement = definition(vec![
            root(),
            row(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0, VISIBLE),
            row(3, *b"tcer", [3.0; 4], 0, VISIBLE | 0x200),
        ]);
        let mut stack = MenuStack::new();
        stack
            .create_components_native(&replacement, CONTEXT, &mut resources)
            .unwrap();
        stack.set_float(2, 0.25).unwrap();
        let old = stack.find_component_by_id(2).unwrap().native_key().unwrap();
        let stack = Rc::new(RefCell::new(stack));
        let alias = Rc::clone(&stack);
        let resources = RefCell::new(resources);
        let mut cursor = stack.borrow_mut().begin_components_draw_native(0).unwrap();
        let mut sink = CallbackSink {
            callback: |primitive: &MenuComponentPrimitive<'_>| {
                assert_eq!(primitive.component, old);
                assert_eq!(primitive.vertices[0].argb, 0x5f7f_7f7f);
                assert!(Rc::ptr_eq(&stack, &alias));
                let mut actual = alias
                    .try_borrow_mut()
                    .expect("gray callback must permit recreation");
                actual.destroy_native(&mut *resources.borrow_mut()).unwrap();
                actual
                    .create_components_native(&replacement, CONTEXT, &mut *resources.borrow_mut())
                    .unwrap();
                actual.set_float(2, 0.9).unwrap();
                Ok(())
            },
            recorded: Sink::default(),
        };
        let call = stack
            .borrow_mut()
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        cursor.complete_call(call.dispatch(&mut sink)).unwrap();
        assert_ne!(
            stack.borrow().find_component_by_id(2).unwrap().native_key(),
            Some(old)
        );
        assert_eq!(stack.borrow().progress_value(2), Some(0.9));
        assert_eq!(geometry(&stack.borrow(), 2).position, [1.0, 2.0]);
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal has a stale menu owner"
        );
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        assert_eq!(sink.recorded.attempts, 1);
        assert_eq!(
            sink.recorded
                .commands
                .iter()
                .map(Command::id)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(resources.borrow().calls.is_empty());
    }

    #[test]
    fn incremental_interrupted_destroy_keeps_ready_guard_and_uncertain_release() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![
                root(),
                row(2, *b"rabp", [1.0; 4], 0, VISIBLE),
                row(3, *b"ttcr", [2.0; 4], 0, VISIBLE),
            ],
            &mut resources,
        );
        stack
            .set_string_native(3, Some(b"image.tga"), &mut resources)
            .unwrap();
        let texture = picture_key(&stack, 3);
        resources.fail_release = Some(texture);
        let stack = RefCell::new(stack);
        let resources = RefCell::new(resources);
        let mut cursor = stack.borrow_mut().begin_components_draw_native(0).unwrap();
        let mut sink = CallbackSink {
            callback: |_: &MenuComponentPrimitive<'_>| {
                assert_eq!(
                    stack
                        .try_borrow_mut()
                        .expect("callback must permit destruction")
                        .destroy_native(&mut *resources.borrow_mut()),
                    Err("uncertain release failure".to_owned())
                );
                Ok(())
            },
            recorded: Sink::default(),
        };
        let call = stack
            .borrow_mut()
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        cursor.complete_call(call.dispatch(&mut sink)).unwrap();
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native menu component construction/destruction is incomplete"
        );
        assert_eq!(stack.borrow().native.as_ref().unwrap().next_destroy, 2);
        assert_eq!(
            stack
                .borrow()
                .find_component_by_id(3)
                .unwrap()
                .interrupted_texture_key(),
            Some(&texture)
        );
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        resources.borrow_mut().fail_release = None;
        assert_eq!(stack.borrow_mut().destroy_native(&mut *resources.borrow_mut()), Err("native texture Release is interrupted; backend reconciliation is required, not replay".to_owned()));
        assert_eq!(
            resources.borrow().calls,
            vec![
                ResourceCall::Create(b"image.tga".to_vec()),
                ResourceCall::Release(texture)
            ]
        );
        assert_eq!(sink.recorded.attempts, 1);
        assert_eq!(
            sink.recorded
                .commands
                .iter()
                .map(Command::id)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn incremental_pending_texture_call_rejects_successful_destroy_before_dispatch() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"ttcr", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        stack
            .set_string_native(2, Some(b"image.tga"), &mut resources)
            .unwrap();
        let texture = picture_key(&stack, 2);
        let key = stack.find_component_by_id(2).unwrap().native_key().unwrap();
        let mut cursor = stack.begin_components_draw_native(0).unwrap();
        let call = stack
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        assert_eq!(call.component, key);
        assert_eq!(cursor.owner, key.owner);
        assert!(Weak::ptr_eq(&call.owner_liveness, &cursor.owner_liveness));
        assert_eq!(call.owner_liveness.strong_count(), 1);
        assert!(matches!(
            &call.arguments,
            NativeDrawArguments::Primitive {
                texture: NativeDrawTexture::Fixed(Some(call_texture)), ..
            }
                if *call_texture == texture
        ));

        assert_eq!(stack.destroy_native(&mut resources), Ok(()));
        assert_eq!(stack, MenuStack::new());
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Pending);
        let mut sink = Sink {
            fail_at: Some(0),
            ..Sink::default()
        };
        let result = call.dispatch(&mut sink);
        assert_eq!(
            result,
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Stopped);
        assert_eq!(
            cursor.complete_call(result),
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(
            cursor.complete_call(Ok(())),
            Err("native component draw has no completed pending call".to_owned())
        );
        for _ in 0..2 {
            assert_eq!(
                stack
                    .next_component_draw_native(&mut cursor, None)
                    .unwrap_err(),
                "native component draw traversal is stopped"
            );
        }
        assert_eq!(
            resources.calls,
            vec![
                ResourceCall::Create(b"image.tga".to_vec()),
                ResourceCall::Release(texture),
            ]
        );
    }

    #[test]
    fn incremental_pending_texture_call_rejects_interrupted_destroy_before_dispatch() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![
                root(),
                row(2, *b"ttcr", [1.0; 4], 0, VISIBLE),
                row(3, *b"ttcr", [2.0; 4], 0, VISIBLE),
                row(4, *b"ttcr", [3.0; 4], 0, VISIBLE),
            ],
            &mut resources,
        );
        for id in 2..=4 {
            stack
                .set_string_native(id, Some(b"image.tga"), &mut resources)
                .unwrap();
        }
        let released = picture_key(&stack, 2);
        let interrupted = picture_key(&stack, 3);
        let later = picture_key(&stack, 4);
        let mut cursor = stack.begin_components_draw_native(0).unwrap();
        let call = stack
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        let key = call.component;
        assert_eq!(call.component_id, 2);
        assert!(matches!(
            &call.arguments,
            NativeDrawArguments::Primitive {
                texture: NativeDrawTexture::Fixed(Some(texture)), ..
            }
                if *texture == released
        ));
        resources.calls.clear();
        resources.fail_release = Some(interrupted);

        assert_eq!(
            stack.destroy_native(&mut resources),
            Err("uncertain release failure".to_owned())
        );
        assert_eq!(stack.native.as_ref().unwrap().owner, key.owner);
        assert_eq!(
            stack.native.as_ref().unwrap().phase,
            NativeMenuPhase::Destroying
        );
        assert_eq!(stack.native.as_ref().unwrap().next_destroy, 2);
        assert!(stack.find_component_by_id(2).is_none());
        assert_eq!(
            stack.components[2].interrupted_texture_key(),
            Some(&interrupted)
        );
        assert_eq!(picture_key(&stack, 4), later);
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Pending);
        let mut sink = Sink {
            fail_at: Some(0),
            ..Sink::default()
        };
        let result = call.dispatch(&mut sink);
        assert_eq!(
            result,
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Stopped);
        assert_eq!(
            cursor.complete_call(result),
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(
            stack
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        resources.fail_release = None;
        assert_eq!(
            stack.destroy_native(&mut resources),
            Err("native texture Release is interrupted; backend reconciliation is required, not replay".to_owned())
        );
        assert_eq!(
            resources.calls,
            vec![
                ResourceCall::Release(released),
                ResourceCall::Release(interrupted)
            ]
        );
        assert_eq!(picture_key(&stack, 4), later);
    }

    #[test]
    fn incremental_pending_call_rejects_recreated_owner_without_stopping_its_draw() {
        let mut resources = Resources::new();
        let replacement = definition(vec![root(), row(2, *b"ttcr", [1.0; 4], 0, VISIBLE)]);
        let mut stack = MenuStack::new();
        stack
            .create_components_native(&replacement, CONTEXT, &mut resources)
            .unwrap();
        stack
            .set_string_native(2, Some(b"old.tga"), &mut resources)
            .unwrap();
        let old_texture = picture_key(&stack, 2);
        let mut cursor = stack.begin_components_draw_native(0).unwrap();
        let call = stack
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        let old_key = call.component;
        stack.destroy_native(&mut resources).unwrap();
        stack
            .create_components_native(&replacement, CONTEXT, &mut resources)
            .unwrap();
        stack
            .set_string_native(2, Some(b"new.tga"), &mut resources)
            .unwrap();
        let new_texture = picture_key(&stack, 2);
        let mut replacement_cursor = stack.begin_components_draw_native(0).unwrap();
        let replacement_call = stack
            .next_component_draw_native(&mut replacement_cursor, None)
            .unwrap()
            .unwrap();
        let new_key = replacement_call.component;
        assert_eq!(new_key.index, old_key.index);
        assert_ne!(new_key.owner, old_key.owner);
        assert_ne!(new_texture, old_texture);
        assert!(!Weak::ptr_eq(
            &call.owner_liveness,
            &replacement_call.owner_liveness
        ));
        assert_eq!(replacement_call.owner_liveness.strong_count(), 1);
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Pending);
        let mut sink = Sink::default();
        let result = call.dispatch(&mut sink);
        assert_eq!(
            result,
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Stopped);
        assert_eq!(
            cursor.complete_call(result),
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(
            stack
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        assert_eq!(replacement_cursor.phase.get(), NativeDrawPhase::Pending);
        replacement_cursor
            .complete_call(replacement_call.dispatch(&mut sink))
            .unwrap();
        assert!(stack
            .next_component_draw_native(&mut replacement_cursor, None)
            .unwrap()
            .is_none());
        assert_eq!(sink.attempts, 1);
        assert!(matches!(
            sink.commands.as_slice(),
            [Command::Primitive { key, texture: Some(texture), .. }]
                if *key == new_key && *texture == new_texture
        ));
        stack.destroy_native(&mut resources).unwrap();
        assert_eq!(
            resources.calls,
            vec![
                ResourceCall::Create(b"old.tga".to_vec()),
                ResourceCall::Release(old_texture),
                ResourceCall::Create(b"new.tga".to_vec()),
                ResourceCall::Release(new_texture),
            ]
        );
    }

    #[test]
    fn incremental_pending_text_call_rejects_canonical_owner_drop_before_dispatch() {
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::from([0x80, b'A']));
        let stack = Rc::new(RefCell::new(create(
            vec![root(), row(2, *b"txts", [1.0; 4], 7, VISIBLE)],
            &mut resources,
        )));
        let alias = Rc::clone(&stack);
        let mut cursor = stack.borrow_mut().begin_components_draw_native(0).unwrap();
        let call = stack
            .borrow_mut()
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        assert!(Rc::ptr_eq(&stack, &alias));
        assert_eq!(call.component.owner, cursor.owner);
        assert_eq!(call.owner_liveness.strong_count(), 1);
        assert!(matches!(
            &call.arguments,
            NativeDrawArguments::Text { bytes, .. } if bytes.as_ref() == [0x80, b'A']
        ));
        drop(stack);
        assert_eq!(call.owner_liveness.strong_count(), 1);
        assert_eq!(alias.borrow().native.as_ref().unwrap().owner, cursor.owner);
        drop(alias);
        assert_eq!(call.owner_liveness.strong_count(), 0);
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Pending);
        let mut sink = Sink {
            fail_at: Some(0),
            ..Sink::default()
        };
        let result = call.dispatch(&mut sink);
        assert_eq!(
            result,
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
        assert_eq!(cursor.phase.get(), NativeDrawPhase::Stopped);
        assert_eq!(
            cursor.complete_call(result),
            Err("native component draw call has a stale menu owner".to_owned())
        );
        assert_eq!(
            cursor.complete_call(Ok(())),
            Err("native component draw has no completed pending call".to_owned())
        );
        let mut other = create(vec![root()], &mut resources);
        assert_ne!(other.native.as_ref().unwrap().owner, cursor.owner);
        assert_eq!(
            other
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        assert_eq!(resources.calls, vec![ResourceCall::Text(7)]);
    }

    #[test]
    fn incremental_pending_protocol_errors_invalidate_calls_without_dispatch_replay() {
        for misuse in [
            "next_before_dispatch",
            "complete_before_dispatch",
            "next_before_complete",
        ] {
            let mut resources = Resources::new();
            let mut stack = create(
                vec![root(), row(2, *b"rabp", [1.0; 4], 0, VISIBLE)],
                &mut resources,
            );
            let mut cursor = stack.begin_components_draw_native(0).unwrap();
            let call = stack
                .next_component_draw_native(&mut cursor, None)
                .unwrap()
                .unwrap();
            let mut sink = Sink::default();
            if misuse == "next_before_complete" {
                call.dispatch(&mut sink).unwrap();
                assert_eq!(
                    stack
                        .next_component_draw_native(&mut cursor, None)
                        .unwrap_err(),
                    "native component draw call is still pending"
                );
                assert_eq!(
                    cursor.complete_call(Ok(())),
                    Err("native component draw has no completed pending call".to_owned())
                );
                assert_eq!(sink.attempts, 1);
                assert_eq!(sink.commands.len(), 1);
            } else {
                if misuse == "next_before_dispatch" {
                    stack.destroy_native(&mut resources).unwrap();
                    assert_eq!(
                        stack
                            .next_component_draw_native(&mut cursor, None)
                            .unwrap_err(),
                        "native component draw call is still pending"
                    );
                } else {
                    assert_eq!(
                        cursor.complete_call(Ok(())),
                        Err("native component draw has no completed pending call".to_owned())
                    );
                }
                assert_eq!(
                    call.dispatch(&mut sink),
                    Err("native component draw call is no longer dispatchable".to_owned())
                );
                assert_eq!(sink.attempts, 0);
                assert!(sink.commands.is_empty());
            }
            assert_eq!(
                stack
                    .next_component_draw_native(&mut cursor, None)
                    .unwrap_err(),
                "native component draw traversal is stopped"
            );
            assert!(resources.calls.is_empty());
        }
    }

    #[test]
    fn incremental_dropped_cursor_cancels_its_pending_call_without_a_callback() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"rabp", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        let mut cursor = stack.begin_components_draw_native(0).unwrap();
        let call = stack
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        drop(cursor);
        let mut sink = Sink::default();
        assert_eq!(
            call.dispatch(&mut sink),
            Err("native component draw call is no longer dispatchable".to_owned())
        );
        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn incremental_callback_protocol_error_cannot_be_overwritten_by_sink_success() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"rabp", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        let mut cursor = stack.begin_components_draw_native(0).unwrap();
        let call = stack
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        let mut sink = CallbackSink {
            callback: |_: &MenuComponentPrimitive<'_>| {
                assert_eq!(
                    cursor.complete_call(Ok(())),
                    Err("native component draw has no completed pending call".to_owned())
                );
                Ok(())
            },
            recorded: Sink::default(),
        };
        let result = call.dispatch(&mut sink);
        assert_eq!(
            result,
            Err("native component draw interrupted during callback".to_owned())
        );
        assert_eq!(sink.recorded.attempts, 1);
        assert_eq!(sink.recorded.commands.len(), 1);
        drop(sink);
        assert_eq!(
            cursor.complete_call(result),
            Err("native component draw interrupted during callback".to_owned())
        );
        assert_eq!(
            stack
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
    }

    #[test]
    fn incremental_sink_error_keeps_mutations_and_gray_prefix_without_replay() {
        let mut resources = Resources::new();
        let stack = RefCell::new(create(
            vec![
                root(),
                row(2, *b"rabp", [1.0, 2.0, 8.0, 6.0], 0, VISIBLE),
                row(3, *b"tcer", [3.0; 4], 0, VISIBLE),
            ],
            &mut resources,
        ));
        stack.borrow_mut().set_float(2, 0.25).unwrap();
        let key = stack
            .borrow()
            .find_component_by_id(2)
            .unwrap()
            .native_key()
            .unwrap();
        let mut cursor = stack.borrow_mut().begin_components_draw_native(0).unwrap();
        let mut sink = CallbackSink {
            callback: |primitive: &MenuComponentPrimitive<'_>| {
                assert_eq!(primitive.component, key);
                let mut actual = stack
                    .try_borrow_mut()
                    .expect("failed callback must permit mutation");
                if primitive.vertices[0].argb == 0x5f7f_7f7f {
                    actual.set_float(2, 0.75).unwrap();
                } else {
                    assert_eq!(actual.progress_value(2), Some(0.75));
                    assert_eq!(primitive.vertices[1].position, [17.0, 22.0, 0.0, 1.0]);
                    actual.find_component_by_id_mut(3).unwrap().flags = 0;
                }
                Ok(())
            },
            recorded: Sink {
                fail_at: Some(1),
                ..Sink::default()
            },
        };
        let gray = stack
            .borrow_mut()
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        cursor.complete_call(gray.dispatch(&mut sink)).unwrap();
        let red = stack
            .borrow_mut()
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        let result = red.dispatch(&mut sink);
        assert_eq!(result, Err("sink failure".to_owned()));
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        assert_eq!(cursor.complete_call(result), Err("sink failure".to_owned()));
        assert_eq!(stack.borrow().progress_value(2), Some(0.75));
        assert_eq!(stack.borrow().find_component_by_id(3).unwrap().flags, 0);
        assert_eq!(sink.recorded.attempts, 2);
        assert_eq!(
            sink.recorded.commands,
            vec![Command::Primitive {
                key,
                id: 2,
                draw_flags: 0,
                topology: MenuComponentPrimitiveTopology::TriangleStrip,
                vertices: vec![
                    vertex(11.0, 22.0, [0.0, 0.0], 0x5f7f_7f7f),
                    vertex(19.0, 22.0, [1.0, 0.0], 0x5f7f_7f7f),
                    vertex(11.0, 28.0, [0.0, 1.0], 0x5f7f_7f7f),
                    vertex(19.0, 28.0, [1.0, 1.0], 0x5f7f_7f7f),
                ],
                texture: None,
            }]
        );
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn incremental_numeric_error_keeps_gray_mutation_and_stops_even_after_repair() {
        let mut resources = Resources::new();
        let stack = RefCell::new(create(
            vec![root(), row(2, *b"rabp", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        ));
        let mut cursor = stack.borrow_mut().begin_components_draw_native(0).unwrap();
        let mut sink = CallbackSink {
            callback: |_: &MenuComponentPrimitive<'_>| {
                stack
                    .try_borrow_mut()
                    .expect("gray callback must permit numeric mutation")
                    .set_float(2, f32::NAN)
                    .unwrap();
                Ok(())
            },
            recorded: Sink::default(),
        };
        let gray = stack
            .borrow_mut()
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .unwrap();
        cursor.complete_call(gray.dispatch(&mut sink)).unwrap();
        assert!(stack.borrow().progress_value(2).unwrap().is_nan());
        assert_eq!(stack.borrow_mut().next_component_draw_native(&mut cursor, None).unwrap_err(), "unsupported nonfinite/overflow native component geometry under the portable PC24 policy");
        stack.borrow_mut().set_float(2, 0.5).unwrap();
        assert_eq!(
            stack
                .borrow_mut()
                .next_component_draw_native(&mut cursor, None)
                .unwrap_err(),
            "native component draw traversal is stopped"
        );
        assert_eq!(sink.recorded.attempts, 1);
        assert_eq!(sink.recorded.commands.len(), 1);
        let Command::Primitive { vertices, .. } = &sink.recorded.commands[0] else {
            panic!("expected gray progress")
        };
        assert!(vertices.iter().all(|vertex| vertex.argb == 0x5f7f_7f7f));
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn incremental_empty_and_finished_cursors_do_not_preflight_unreached_native_state() {
        let mut resources = Resources::new();
        let mut stack = MenuStack::new();
        stack.create_empty_native().unwrap();
        let mut cursor = stack.begin_components_draw_native(u32::MAX).unwrap();
        stack.destroy_native(&mut resources).unwrap();
        assert!(stack
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .is_none());
        assert!(stack
            .create_components_native(
                &definition(vec![row(1, *b"nope", [0.0; 4], 0, 0)]),
                CONTEXT,
                &mut resources
            )
            .is_err());
        assert!(stack.has_native_lifecycle());
        assert!(stack.is_empty());
        assert_eq!(stack.native_root_key(), Ok(None));
        assert_eq!(
            stack.begin_components_draw_native(0).unwrap_err(),
            "native menu component construction/destruction is incomplete"
        );
        assert!(stack
            .next_component_draw_native(&mut cursor, None)
            .unwrap()
            .is_none());
        assert!(resources.calls.is_empty());
    }

    #[test]
    fn sink_failure_retains_grey_progress_prefix_and_never_reaches_later_components() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![
                root(),
                row(2, *b"rabp", [1.0; 4], 0, VISIBLE),
                row(3, *b"tcer", [2.0; 4], 0, VISIBLE),
            ],
            &mut resources,
        );
        let mut sink = Sink {
            fail_at: Some(1),
            ..Sink::default()
        };
        assert_eq!(
            stack.draw_components_native(0, None, &mut sink),
            Err("sink failure".to_owned())
        );
        assert_eq!(sink.attempts, 2);
        assert_eq!(
            sink.commands,
            vec![Command::Primitive {
                key: stack.find_component_by_id(2).unwrap().native_key().unwrap(),
                id: 2,
                draw_flags: 0,
                topology: MenuComponentPrimitiveTopology::TriangleStrip,
                vertices: vec![
                    vertex(11.0, 21.0, [0.0, 0.0], 0x5f7f_7f7f),
                    vertex(12.0, 21.0, [1.0, 0.0], 0x5f7f_7f7f),
                    vertex(11.0, 22.0, [0.0, 1.0], 0x5f7f_7f7f),
                    vertex(12.0, 22.0, [1.0, 1.0], 0x5f7f_7f7f),
                ],
                texture: None,
            }]
        );
        assert_eq!(geometry(&stack, 3).position, [12.0, 22.0]);
    }

    #[test]
    fn nonfinite_progress_retains_only_the_completed_grey_command() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![root(), row(2, *b"rabp", [1.0; 4], 0, VISIBLE)],
            &mut resources,
        );
        stack.set_float(2, f32::NAN).unwrap();
        assert!(stack.progress_value(2).unwrap().is_nan());
        assert_eq!(stack.get_float(2).to_bits(), 0);
        let mut sink = Sink::default();
        assert!(stack
            .draw_components_native(0, None, &mut sink)
            .unwrap_err()
            .contains("nonfinite/overflow"));
        assert_eq!(sink.attempts, 1);
        assert_eq!(sink.commands.len(), 1);
        let Command::Primitive { vertices, .. } = &sink.commands[0] else {
            panic!("expected grey progress");
        };
        assert!(vertices.iter().all(|vertex| vertex.argb == 0x5f7f_7f7f));
        assert!(stack.staged_floats.is_empty());
    }

    #[test]
    fn root_margin_failure_preserves_earlier_constructor_text_and_allocation() {
        let bytes: Arc<[u8]> = Arc::from([0x81]);
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::clone(&bytes));
        let mut root = root();
        root.text_id = 7;
        let mut stack = MenuStack::new();
        let error = stack
            .create_components_native(
                &definition(vec![root]),
                MenuComponentCreateContext {
                    left_margin_x: f32::NAN,
                    ..CONTEXT
                },
                &mut resources,
            )
            .unwrap_err();
        assert!(error.contains("nonfinite/overflow"));
        assert_eq!(resources.calls, vec![ResourceCall::Text(7)]);
        assert_eq!(stack.len(), 1);
        assert_eq!(geometry(&stack, 1).position, [10.0, 20.0]);
        assert_eq!(geometry(&stack, 1).relative_position, [10.0, 20.0]);
        assert_eq!(
            stack.native.as_ref().unwrap().root,
            stack.components[0].native_key()
        );
        assert!(Arc::ptr_eq(
            stack.components[0]
                .native
                .as_ref()
                .unwrap()
                .text
                .as_ref()
                .unwrap(),
            &bytes
        ));
        assert!(stack
            .draw_components_native(0, None, &mut Sink::default())
            .is_err());
        stack.destroy_native(&mut resources).unwrap();
        assert_eq!(Arc::strong_count(&bytes), 2);
        assert_eq!(stack, MenuStack::new());
    }

    #[test]
    fn after_move_numeric_failure_keeps_reached_assignment_prefix_and_no_commands() {
        let mut resources = Resources::new();
        let mut root = root();
        root.rect[0] = f32::MAX;
        let mut stack = create(
            vec![
                root,
                row(2, *b"txts", [0.0, 1.0, 1.0, 1.0], 0, VISIBLE),
                row(3, *b"cniw", [f32::MAX, 2.0, 1.0, 1.0], 0, VISIBLE),
                row(4, *b"tcer", [3.0; 4], 0, VISIBLE),
            ],
            &mut resources,
        );
        let mut sink = Sink::default();
        assert!(stack
            .draw_components_native(0, None, &mut sink)
            .unwrap_err()
            .contains("nonfinite/overflow"));
        assert_eq!(geometry(&stack, 2).position, [f32::MAX, 21.0]);
        assert_eq!(geometry(&stack, 3).position, [f32::MAX, 20.0]);
        assert_eq!(geometry(&stack, 4).position, [3.0, 3.0]);
        assert_eq!(sink.attempts, 0);
        assert!(sink.commands.is_empty());
    }

    #[test]
    fn progress_arithmetic_rounds_multiply_before_add_without_fma() {
        let geometry = MenuComponentGeometry {
            position: [-f32::from_bits(0x3f80_0002), 0.0],
            size: [f32::from_bits(0x3f80_0001), 1.0],
            relative_position: [0.0; 2],
        };
        let vertices = filled_vertices(
            MenuComponentPrecision::Pc24Nearest,
            geometry,
            0xffff_0000,
            Some(f32::from_bits(0x3f80_0001)),
        )
        .unwrap();
        assert_eq!(vertices[1].position[0].to_bits(), 0);
        assert_eq!(vertices[3].position[0].to_bits(), 0);
        assert_eq!(vertices[1].uv[0].to_bits(), 0x3f80_0001);
        assert_eq!(vertices[3].uv[0].to_bits(), 0x3f80_0001);
        assert_eq!(
            vertices[0].position[0].to_bits(),
            (-f32::from_bits(0x3f80_0002)).to_bits()
        );
    }

    #[test]
    fn unestablished_precision_fails_only_when_arithmetic_is_actually_reached() {
        let mut resources = Resources::new();
        let context = MenuComponentCreateContext {
            precision: MenuComponentPrecision::Unestablished,
            ..CONTEXT
        };
        for items in [vec![], vec![row(99, *b"upup", [f32::NAN; 4], 7, u32::MAX)]] {
            let mut stack = MenuStack::new();
            stack
                .create_components_native(&definition(items), context, &mut resources)
                .unwrap();
            let mut sink = Sink::default();
            stack.draw_components_native(0, None, &mut sink).unwrap();
            assert!(sink.commands.is_empty());
            stack.destroy_native(&mut resources).unwrap();
            assert_eq!(stack, MenuStack::new());
        }
        let mut stack = MenuStack::new();
        let error = stack
            .create_components_native(
                &definition(vec![row(2, *b"bad!", [1.0; 4], 7, VISIBLE)]),
                context,
                &mut resources,
            )
            .unwrap_err();
        assert!(error.contains("unsupported native menu component"));
        assert!(!error.contains("precision"));
        stack.destroy_native(&mut resources).unwrap();
        let error = stack
            .create_components_native(&definition(vec![root()]), context, &mut resources)
            .unwrap_err();
        assert!(error.contains("unestablished native component precision"));
        assert!(resources.calls.is_empty());
        assert!(stack.is_empty());
        assert_eq!(
            stack.native.as_ref().unwrap().phase,
            NativeMenuPhase::Creating
        );
        stack.destroy_native(&mut resources).unwrap();
    }

    #[test]
    fn native_payload_keeps_its_precision_for_later_numeric_draw_operations() {
        let mut resources = Resources::new();
        let mut stack = create(
            vec![row(1, *b"tcer", [1.0; 4], 0, VISIBLE | 0x200)],
            &mut resources,
        );
        assert_eq!(
            stack.components[0].native.as_ref().unwrap().precision,
            MenuComponentPrecision::Pc24Nearest
        );
        stack.components[0].native.as_mut().unwrap().precision =
            MenuComponentPrecision::Unestablished;
        let mut sink = Sink::default();
        let error = stack
            .draw_components_native(0, None, &mut sink)
            .unwrap_err();
        assert!(error.contains("unestablished native component precision"));
        assert!(sink.commands.is_empty());
        assert_eq!(sink.attempts, 0);
        stack.destroy_native(&mut resources).unwrap();
    }

    #[test]
    fn unestablished_precision_preserves_non_numeric_resource_effects_and_empty_draws() {
        let bytes: Arc<[u8]> = Arc::from([0x81]);
        let mut resources = Resources::new();
        resources.texts.insert(7, Arc::clone(&bytes));
        let mut window = create(vec![root()], &mut resources);
        window.components[0].native.as_mut().unwrap().precision =
            MenuComponentPrecision::Unestablished;
        let mut sink = Sink::default();
        window.draw_components_native(0, None, &mut sink).unwrap();
        assert!(sink.commands.is_empty());
        assert_eq!(sink.attempts, 0);
        window.destroy_native(&mut resources).unwrap();
        assert!(resources.calls.is_empty());

        let mut picture = create(vec![row(1, *b"ttcr", [1.0; 4], 0, VISIBLE)], &mut resources);
        picture.components[0].native.as_mut().unwrap().precision =
            MenuComponentPrecision::Unestablished;
        picture.set_text_id_native(1, 7, &mut resources).unwrap();
        picture
            .set_string_native(1, Some(b"old.tga"), &mut resources)
            .unwrap();
        let old = picture_key(&picture, 1);
        picture
            .set_string_native(1, Some(b"new.tga"), &mut resources)
            .unwrap();
        let new = picture_key(&picture, 1);
        assert_ne!(old, new);
        assert_eq!(
            picture.draw_components_native(0, None, &mut sink),
            Err("unsupported unestablished native component precision at add".to_owned())
        );
        assert_eq!(picture_key(&picture, 1), new);
        assert!(sink.commands.is_empty());
        assert_eq!(sink.attempts, 0);
        resources.fail_create = true;
        assert_eq!(
            picture.set_string_native(1, Some(b"missing.tga"), &mut resources),
            Err("factory failure".to_owned())
        );
        assert_eq!(picture.components[0].texture_key().unwrap(), None);
        picture.draw_components_native(0, None, &mut sink).unwrap();
        assert!(sink.commands.is_empty());
        assert_eq!(sink.attempts, 0);
        picture.destroy_native(&mut resources).unwrap();
        assert_eq!(picture, MenuStack::new());
        assert_eq!(Arc::strong_count(&bytes), 2);
        assert_eq!(
            resources.calls,
            vec![
                ResourceCall::Text(7),
                ResourceCall::Create(b"old.tga".to_vec()),
                ResourceCall::Release(old),
                ResourceCall::Create(b"new.tga".to_vec()),
                ResourceCall::Release(new),
                ResourceCall::Create(b"missing.tga".to_vec()),
            ]
        );
    }

    #[test]
    fn uncomposed_compatibility_stack_cannot_impersonate_native_lifecycle() {
        let mut resources = Resources::new();
        let mut stack = MenuStack::create_from_definition(&definition(vec![root()]));
        assert!(stack.set_text_id_native(1, 0, &mut resources).is_err());
        assert!(stack
            .set_string_native(1, Some(b"image.tga"), &mut resources)
            .is_err());
        assert!(stack
            .draw_components_native(0, None, &mut Sink::default())
            .is_err());
        assert!(stack.destroy_native(&mut resources).is_err());
        assert!(stack
            .create_components_native(&definition(vec![]), CONTEXT, &mut resources)
            .is_err());
        assert!(resources.calls.is_empty());
        assert_eq!(stack.len(), 1);
    }

    #[test]
    #[should_panic(expected = "native component duplication requires resource owners")]
    fn native_owned_components_cannot_use_legacy_duplication() {
        let mut resources = Resources::new();
        let stack = create(vec![root()], &mut resources);
        stack.components[0].duplicated();
    }

    #[test]
    #[should_panic(expected = "use destroy_native with the texture resource owner")]
    fn native_owned_stack_cannot_use_legacy_destroy() {
        let mut resources = Resources::new();
        let mut stack = create(vec![root()], &mut resources);
        stack.destroy();
    }
}
