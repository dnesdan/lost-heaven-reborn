//! Data-driven original menu layout and atlas text; no game scripts are executed here.
//!
//! Marker-only menu-stack contracts (Game.exe menu RVAs 0x1e06e0 OnClick,
//! 0x1e4a50 OnChange, 0x1e0690 OnCreate, 0x1ea2e0 Create stack; siblings
//! mafia-re game/ui/g_m_func.cpp:273-354 and game/ui/g_menu.cpp:610-691,
//! 735-853, 855-901, 903-914, 1046-1139, cited only, never vendored):
//! MenuComponent/MenuStack own the pure list contracts (intake with `upup`
//! skipping, FindComponentByID, SetDisabled COMPF bits, ActivateByID(All)
//! search, key/hover routing into the OnClick dispatch, close/exchange
//! latches). Device owners stay DEFERRED: menu sounds, parent callbacks, the
//! activeMenuWindow global and menu allocation (MenuOpen), the igraph pump
//! plus mission/scene owners (ExecuteMenu, TickCamera, Draw device paths),
//! and the component classes (Draw/Key/Mouse virtuals, text/value storage).
use lhr_assets::{FontMetrics, Installation, MenuDefinition, Texture as Image};
use crate::main_menu;
use lhr_render::{Mesh, Texture, Vertex};
use std::sync::Arc;

pub const DESIGN: [f32; 2] = [800.0, 600.0];
/// Host-side reading of the GM_MainMenu::OnClick dispatch (Game.exe VA
/// 0x5e06e0, RVA 0x1e06e0; sibling game/ui/g_m_func.cpp:309-354, cited only).
/// The full 13-branch dispatch lives in `main_menu_onclick`; this mirrors its
/// CloseAll outcomes that the host menu can observe. Variants beyond the host
/// admission subset (see `enabled`) are reachable only through
/// `action_for_id`/`click_effect`, never through pointer/keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Intro,
    Tutorial,
    NewGame,
    LoadGame,
    FreeRide,
    Extreme,
    Racing,
    Multiplayer,
    Carcyclopedia,
    Options,
    Credits,
    Profile,
    Quit,
    Unsupported(u32),
}

/// Pure id reading of the OnClick dispatch above: every native branch maps to
/// its outcome, unknown ids to `Unsupported` (the native no-op path returning
/// 0 with no menu effect).
pub fn action_for_id(id: u32) -> Action {
    match id {
        main_menu::MAINMENU_ITEM_INTRO => Action::Intro,
        main_menu::MAINMENU_ITEM_TUTORIAL => Action::Tutorial,
        main_menu::MAINMENU_ITEM_NEW_GAME => Action::NewGame,
        main_menu::MAINMENU_ITEM_LOAD_GAME => Action::LoadGame,
        main_menu::MAINMENU_ITEM_FREE_RIDE => Action::FreeRide,
        main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME => Action::Extreme,
        main_menu::MAINMENU_ITEM_RACING => Action::Racing,
        main_menu::MAINMENU_ITEM_MULTIPLAYER => Action::Multiplayer,
        main_menu::MAINMENU_ITEM_CARCYCLOPEDIA => Action::Carcyclopedia,
        main_menu::MAINMENU_ITEM_OPTIONS => Action::Options,
        main_menu::MAINMENU_ITEM_CREDITS => Action::Credits,
        main_menu::MAINMENU_ITEM_CHANGE_PROFILE => Action::Profile,
        main_menu::MAINMENU_ITEM_QUIT => Action::Quit,
        id => Action::Unsupported(id),
    }
}

/// GM_Menu::Tick distinguishes component-list traversal from traversal by ID.
/// This is the existing button-only host menu, not the complete component API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    Previous,
    Next,
    PreviousId,
    NextId,
}

/// Last actual window position. Tracking is independent of UI capture; hover
/// and click effects are applied only after the application admits an event.
#[derive(Default)]
pub struct Pointer {
    physical: Option<[f32; 2]>,
}
impl Pointer {
    pub fn observe(&mut self, physical: [f32; 2]) {
        self.physical = physical.into_iter().all(f32::is_finite).then_some(physical);
    }
    pub fn leave(&mut self) {
        self.physical = None;
    }
    fn position(&self, viewport: [u32; 2]) -> Option<[f32; 2]> {
        if viewport.contains(&0) {
            return None;
        }
        self.physical
            .map(|position| design_position(position, viewport))
    }
    pub fn hover(&self, menu: &mut MenuUi, viewport: [u32; 2]) -> bool {
        self.position(viewport)
            .is_some_and(|position| menu.point(position))
    }
    pub fn select_for_click(&self, menu: &mut MenuUi, viewport: [u32; 2]) -> bool {
        let Some(position) = self.position(viewport) else {
            return false;
        };
        // Captured movement deliberately did not change menu selection. Resolve
        // it again at the current position before allowing selected activation.
        menu.point(position);
        menu.hit(position)
    }
}

pub struct MenuUi {
    definition: MenuDefinition,
    labels: Vec<Vec<u8>>,
    font: FontMetrics,
    title_font: FontMetrics,
    atlas: Image,
    title_atlas: Image,
    language: [u16; 256],
    header: Image,
    paper: Image,
    selected: usize,
}
impl MenuUi {
    pub fn load(installation: &Installation) -> Result<Self, String> {
        let read = |name: &str| {
            installation
                .read(name)
                .map_err(|e| format!("Menu resource {name}: {e}"))
        };
        let definition = lhr_assets::parse_menu_definition(&read("tables/menu/mainmenu.mnu")?)
            .map_err(|e| e.to_string())?;
        validate_layout(&definition)?;
        let version = installation
            .read_optional("tables/version.txt")
            .map_err(|e| format!("Menu data version: {e}"))?;
        let code = crate::original_language::language_code_from_version(
            version.as_deref().unwrap_or_default(),
        );
        let paths = crate::original_language::language_resource_paths(code)?;
        let text = lhr_assets::parse_raw_text_db(&read(&paths.text_database)?)
            .map_err(|e| e.to_string())?;
        let labels = definition
            .items
            .iter()
            .map(|item| {
                text.get(item.text_id)
                    .map(<[u8]>::to_vec)
                    .ok_or_else(|| format!("Menu text ID {} is absent", item.text_id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if labels.iter().any(|label| label.len() > 128)
            || labels.iter().map(|label| label.len()).sum::<usize>() > 1024
        {
            return Err("Menu labels exceed supported text quota".into());
        }
        let lang = read(&paths.primary_map)?;
        if lang.len() != 568
            || &lang[..4] != b"Lang"
            || lang[4..6] != [(code >> 8) as u8, code as u8]
        {
            return Err("Unsupported menu language table".into());
        }
        let language =
            std::array::from_fn(|i| u16::from_le_bytes([lang[56 + i * 2], lang[57 + i * 2]]));
        let suffix_bytes = &lang[40..56];
        let suffix_end = suffix_bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or("Unterminated menu font suffix")?;
        let suffix_bytes = &suffix_bytes[..suffix_end];
        if !suffix_bytes
            .iter()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'/' | b'\\' | b':'))
        {
            return Err("Unsupported menu font suffix".into());
        }
        let suffix = std::str::from_utf8(suffix_bytes).unwrap();
        let decode = |name: &str| lhr_assets::decode_tga(&read(name)?).map_err(|e| e.to_string());
        let selected = definition
            .items
            .iter()
            .filter(|item| item.kind == *b"ttub")
            .position(|item| enabled(item.id))
            .unwrap_or(0);
        let ui = Self {
            definition,
            labels,
            language,
            selected,
            font: lhr_assets::parse_font_metrics(&read(&format!("tables/font1{suffix}.gmf"))?)
                .map_err(|e| e.to_string())?,
            title_font: lhr_assets::parse_font_metrics(&read(&format!(
                "tables/font2{suffix}.gmf"
            ))?)
            .map_err(|e| e.to_string())?,
            atlas: decode(&format!("maps/font1{suffix}.tga"))?,
            title_atlas: decode(&format!("maps/font2{suffix}.tga"))?,
            header: decode("maps/papir5a.tga")?,
            paper: decode("maps/papir3.tga")?,
        };
        for font in [&ui.font, &ui.title_font] {
            if !(0.1..=2.0).contains(&font.height_scale)
                || !(0.1..=2.0).contains(&font.width_scale)
                || !(0.001..=1.0).contains(&font.uv_height)
                || !(0.0..=1.0).contains(&font.tracking)
                || font.baseline.abs() > 2.0
                || font.glyphs.iter().any(|g| {
                    !(0.0..=2.0).contains(&g.width)
                        || !(0.0..=1.0).contains(&g.u_left)
                        || !(0.0..=1.0).contains(&g.u_right)
                        || !(0.0..=1.0).contains(&g.v_top)
                })
            {
                return Err("Menu font metrics exceed supported layout quota".into());
            }
        }
        for image in [&ui.atlas, &ui.title_atlas, &ui.header, &ui.paper] {
            if image.width > 2048 || image.height > 2048 {
                return Err("Menu artwork exceeds supported quota".into());
            }
        }
        Ok(ui)
    }
    fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.definition
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == *b"ttub")
            .map(|(i, _)| i)
    }
    fn selected_index(&self) -> usize {
        self.rows().nth(self.selected).unwrap_or(0)
    }
    pub fn focus_id(&self) -> u32 {
        self.definition.items[self.selected_index()].id
    }
    pub fn navigate(&mut self, navigation: Navigation) -> bool {
        let previous = self.selected;
        match navigation {
            Navigation::Previous => self.move_selection(-1),
            Navigation::Next => self.move_selection(1),
            Navigation::PreviousId | Navigation::NextId => {
                let current = self.focus_id();
                let rows: Vec<_> = self
                    .rows()
                    .enumerate()
                    .map(|(selection, index)| (selection, self.definition.items[index].id))
                    .filter(|(_, id)| enabled(*id))
                    .collect();
                let selected = if navigation == Navigation::NextId {
                    rows.iter()
                        .filter(|(_, id)| *id > current)
                        .min_by_key(|(_, id)| *id)
                        .or_else(|| rows.iter().min_by_key(|(_, id)| *id))
                } else {
                    rows.iter()
                        .filter(|(_, id)| *id < current)
                        .max_by_key(|(_, id)| *id)
                        .or_else(|| rows.iter().max_by_key(|(_, id)| *id))
                };
                if let Some((selection, _)) = selected {
                    self.selected = *selection;
                }
            }
        }
        previous != self.selected
    }
    pub fn move_selection(&mut self, direction: i32) {
        let rows: Vec<_> = self.rows().collect();
        if rows.is_empty() {
            return;
        }
        if direction == 0 {
            return;
        }
        for _ in 0..rows.len() {
            self.selected =
                (self.selected as i32 + direction).rem_euclid(rows.len() as i32) as usize;
            if enabled(self.definition.items[rows[self.selected]].id) {
                break;
            }
        }
    }
    pub fn point(&mut self, position: [f32; 2]) -> bool {
        let root = self.definition.items[0].rect;
        let rows: Vec<_> = self.rows().collect();
        for (selection, index) in rows.into_iter().enumerate() {
            let item = &self.definition.items[index];
            let [x, y, w, h] = item.rect;
            if enabled(item.id)
                && position[0] >= root[0] + x
                && position[0] < root[0] + x + w
                && position[1] >= root[1] + 40.0 + y
                && position[1] < root[1] + 40.0 + y + h
            {
                let changed = self.selected != selection;
                self.selected = selection;
                return changed;
            }
        }
        false
    }
    pub fn hit(&self, position: [f32; 2]) -> bool {
        let root = self.definition.items[0].rect;
        self.rows().any(|index| {
            let item = &self.definition.items[index];
            let [x, y, w, h] = item.rect;
            enabled(item.id)
                && position[0] >= root[0] + x
                && position[0] < root[0] + x + w
                && position[1] >= root[1] + 40.0 + y
                && position[1] < root[1] + 40.0 + y + h
        })
    }
    pub fn activate(&self) -> Action {
        action_for_id(self.focus_id())
    }

    /// Pure GM_MainMenu::OnClick dispatch for the latched focus id (Game.exe
    /// VA 0x5e06e0; sibling game/ui/g_m_func.cpp:309-354): the MenuCloseAll
    /// result or the MenuOpen target. Component lookup, stack allocation and
    /// device effects stay with the stack/device owners (DEFERRED).
    pub fn click_effect(&self) -> Option<main_menu::MainMenuClickEffect> {
        main_menu::main_menu_onclick(self.focus_id())
    }
    pub fn pixels(&self) -> Texture {
        let mut canvas = Canvas::new();
        let [x, y, w, h] = self.definition.items[0].rect;
        canvas.image(&self.header, [x, y, w, 40.0], [0.0, 0.0, 1.0, 1.0], None);
        canvas.image(
            &self.paper,
            [x, y + 39.0, w, h - 40.0],
            [0.0, 0.0, 1.0, 1.0],
            None,
        );
        self.text(
            &mut canvas,
            &self.labels[0],
            [x, y + 4.0, w, 32.0],
            true,
            [255; 3],
        );
        for index in self.rows() {
            let item = &self.definition.items[index];
            let [rx, ry, rw, rh] = item.rect;
            let rect = [x + rx, y + 40.0 + ry, rw, rh];
            let width = self.text(
                &mut canvas,
                &self.labels[index],
                rect,
                false,
                if !enabled(item.id) { [127; 3] } else { [0; 3] },
            );
            if index == self.selected_index() {
                canvas.rectangle(
                    [rect[0] + (rw - width) * 0.5, rect[1] + rh - 3.0, width, 1.5],
                    [160, 0, 0, 255],
                );
            }
        }
        Texture {
            width: 800,
            height: 600,
            rgba: canvas.rgba,
            repeat: [false; 2],
        }
    }
    fn text(
        &self,
        canvas: &mut Canvas,
        text: &[u8],
        rect: [f32; 4],
        title: bool,
        color: [u8; 3],
    ) -> f32 {
        let (font, atlas) = if title {
            (&self.title_font, &self.title_atlas)
        } else {
            (&self.font, &self.atlas)
        };
        let header = |p: usize| f32::from_le_bytes(font.raw_header[p..p + 4].try_into().unwrap());
        let h = rect[3] * header(8);
        let tracking = h * font.tracking;
        let glyphs: Vec<_> = text
            .iter()
            .map(|byte| self.language[*byte as usize])
            .collect();
        let advance = |g: u16| {
            let width = font.glyphs[(g & 255) as usize].width;
            if width < 0.001 {
                h * font.width_scale * 0.3
            } else {
                h * font.width_scale * width - tracking
            }
        };
        let width = glyphs.iter().map(|g| advance(*g)).sum::<f32>() + tracking;
        let mut x = rect[0] + (rect[2] - width) * 0.5;
        for g in glyphs {
            let base = font.glyphs[(g & 255) as usize];
            let draw = |canvas: &mut Canvas, glyph: lhr_assets::FontGlyph, gx: f32| {
                if glyph.width >= 0.001 {
                    canvas.image(
                        atlas,
                        [
                            gx,
                            rect[1] + header(24) * h,
                            h * font.width_scale * glyph.width,
                            h,
                        ],
                        [
                            glyph.u_left,
                            glyph.v_top,
                            glyph.u_right,
                            glyph.v_top + header(16),
                        ],
                        Some(color),
                    );
                }
            };
            draw(canvas, base, x);
            let accent = ((g >> 8) & 0x7f) as usize;
            if accent != 0 {
                let mark = font.glyphs[accent];
                draw(
                    canvas,
                    mark,
                    x + h * font.width_scale * (base.width - mark.width) * 0.5,
                );
            }
            x += advance(g);
        }
        width
    }
}
/// Host admission subset for pointer/keyboard focus: the Lab-hosted original
/// menu admits New Game and Exit only. The full 13-branch OnClick dispatch
/// (`action_for_id`, `click_effect`, `main_menu::main_menu_onclick`) is
/// independent of this gate: it routes every native id, including ones the
/// host never focuses.
fn enabled(id: u32) -> bool {
    matches!(id, 990103 | 990113)
}

/// Host-decoded Tick navigation keys (Game.exe RVA 0x1ea8c0; sibling
/// game/ui/g_menu.cpp:777-802, cited only). DIK_* decoding plus IGraph shift
/// state are device input admitted by the caller; this only names the routed
/// direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickKey {
    Left,
    Right,
    Up,
    Down,
    TabForward,
    TabBackward,
}

/// Marker-only GM_Component id/flag word (Game.exe GM_Menu::components 0x10;
/// siblings game/ui/g_menu.h:283-284, game/ui/g_m_comps.h:19-33, cited only).
/// Class-specific behavior (Draw/Key/Mouse virtuals, text/value storage,
/// parent/child placement) needs the component-class owners and stays
/// DEFERRED; only the id plus the activation-gating COMPF word are owned here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuComponent {
    pub id: u32,
    pub flags: u32,
    /// Text-database id latch (native GM_Component::text_id, g_m_comps.h:140).
    pub text_id: u32,
}

impl MenuComponent {
    /// Fresh component word with the default COMPF sense (VISIBLE|ENABLED|
    /// ACTIVATABLE). The definition's own style bits stay opaque (DEFERRED).
    pub fn new(id: u32) -> Self {
        Self {
            id,
            flags: main_menu::DEFAULT_COMPONENT_FLAGS,
            text_id: 0,
        }
    }

    /// GM_Component::SetTextID id latch (Game.exe VA 0x5ea1c0, RVA 0x1ea1c0;
    /// sibling game/ui/g_menu.cpp:579-587, cited only): records the text id.
    /// The text-database pointer resolution (textDatabase.GetText) needs the
    /// text owner and stays DEFERRED.
    pub fn set_text_id(&mut self, text_id: u32) {
        self.text_id = text_id;
    }

    /// Live read of the latched text id (the inline GM_Component::GetTextID
    /// base, sibling game/ui/g_m_comps.h:107, cited only: no outline body, so
    /// no RVA is claimed here; the Menu-level lookup below owns 0x1eaff0).
    pub fn get_text_id(&self) -> u32 {
        self.text_id
    }

    /// GM_Menu::SetDisabled bit application (GAME 0x5eaed0; sibling
    /// game/ui/g_menu.cpp:903-914, cited only): true sets COMPF_ENABLED,
    /// false clears it.
    pub fn set_disabled(&mut self, enabled: bool) {
        if enabled {
            self.flags |= main_menu::COMPONENT_ENABLED;
        } else {
            self.flags &= !main_menu::COMPONENT_ENABLED;
        }
    }

    /// Live ENABLED-bit read.
    pub fn is_enabled(&self) -> bool {
        self.flags & main_menu::COMPONENT_ENABLED != 0
    }

    /// GM_Menu::SetHidden bit application (GAME 0x5eaf20; sibling
    /// game/ui/g_menu.cpp:915-924, cited only): true sets COMPF_VISIBLE,
    /// false clears it.
    pub fn set_hidden(&mut self, visible: bool) {
        if visible {
            self.flags |= main_menu::COMPONENT_VISIBLE;
        } else {
            self.flags &= !main_menu::COMPONENT_VISIBLE;
        }
    }

    /// Live VISIBLE-bit read.
    pub fn is_visible(&self) -> bool {
        self.flags & main_menu::COMPONENT_VISIBLE != 0
    }

    /// Activation gate (sibling game/ui/g_menu.cpp:1055,1070,1097-1098,
    /// cited only): ENABLED, VISIBLE and ACTIVATABLE together.
    pub fn can_activate(&self) -> bool {
        const GATE: u32 = main_menu::COMPONENT_ENABLED
            | main_menu::COMPONENT_VISIBLE
            | main_menu::COMPONENT_ACTIVATABLE;
        self.flags & GATE == GATE
    }

    /// Mark/unmark this component as the menu's active one (COMPF_ACTIVE bit,
    /// sibling game/ui/g_menu.cpp:815-826, cited only).
    pub fn set_active(&mut self) {
        self.flags |= main_menu::COMPONENT_ACTIVE;
    }
    pub fn clear_active(&mut self) {
        self.flags &= !main_menu::COMPONENT_ACTIVE;
    }
}

/// Explicit-unsupported menu call: the looked-up component class is missing,
/// so the call changes no state and is rejected. The payload names the
/// missing owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuStubError {
    Unsupported(&'static str),
}

impl std::fmt::Display for MenuStubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MenuStubError::Unsupported(what) => write!(f, "unsupported menu call: {what}"),
        }
    }
}

impl std::error::Error for MenuStubError {}

/// Marker-only GM_Menu component stack (Game.exe GM_Menu 0x30, Create RVA
/// 0x1ea2e0; siblings game/ui/g_menu.h:150-290, game/ui/g_menu.cpp:610-625,
/// cited only). Owns only the pure list contracts: intake, lookup, activation
/// search, and close/exchange latches. Device owners stay DEFERRED: menu
/// sounds, parent OnCreateChild/OnCloseChild callbacks, the activeMenuWindow
/// global and menu allocation (MenuOpen), the igraph pump plus mission/scene
/// owners (ExecuteMenu, TickCamera, Draw device paths), and the component
/// classes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuStack {
    components: Vec<MenuComponent>,
    active: Option<u32>,
    closing: bool,
    close_code: u32,
    open_id: u32,
    /// Staged per-component value latches for the GM_Menu value wrappers
    /// (GAME 0x5eb010-0x5eb190). Only ids with a looked-up component are
    /// staged; reads fall back to the native base-class defaults (NULL/0).
    /// Device-owned payloads (textures, text-database resolution) stay
    /// DEFERRED; these maps hold only the admitted host bytes/scalars.
    staged_texts: std::collections::BTreeMap<u32, Vec<u8>>,
    /// Staged float values as raw f32 bits (keeps the owner `Eq`; bit-exact).
    staged_floats: std::collections::BTreeMap<u32, u32>,
    staged_dwords: std::collections::BTreeMap<u32, u32>,
    staged_bools: std::collections::BTreeMap<u32, bool>,
}

impl MenuStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// GM_Menu::CreateCompsByDef intake (GAME 0x5ea370; sibling
    /// game/ui/g_menu.cpp:639-691, cited only): `upup` items never become
    /// components. Intake assignment: the first kept item is the root window,
    /// which keeps no ACTIVATABLE bit (windows/parents are not activatable,
    /// sibling game/ui/g_m_comps.h:199,260,269,302), while every later item is
    /// admitted with the default word. Factory NULL skips (g_menu.cpp:677-679),
    /// menuScreenScale/levyOkraj placement and CanBeParent chaining need the
    /// component-class owners and stay DEFERRED.
    pub fn create_from_definition(definition: &MenuDefinition) -> Self {
        let mut stack = Self::new();
        let mut root_seen = false;
        for item in definition.items.iter().filter(|item| !item.is_upup()) {
            let mut component = MenuComponent::new(item.id);
            // Native GM_Component ctor intake (sibling game/ui/g_menu.cpp:485-502,
            // cited only): the definition's text id lands on the component.
            component.set_text_id(item.text_id);
            if !root_seen {
                component.flags &= !main_menu::COMPONENT_ACTIVATABLE;
                root_seen = true;
            }
            stack.components.push(component);
        }
        stack
    }

    /// GM_Menu::Create ordered stack (GAME 0x5ea2e0; sibling
    /// game/ui/g_menu.cpp:610-625, cited only): clear, intake,
    /// ActivateByIDOrNext(1). The caller's OnCreate/OnChange run next; the
    /// open sound plus the parent OnCreateChild call are device owners
    /// (DEFERRED).
    pub fn create(definition: &MenuDefinition) -> Self {
        let mut stack = Self::create_from_definition(definition);
        stack.activate_by_id_or_next(1);
        stack
    }

    /// Live component count (intake-admitted only; `upup` excluded).
    pub fn len(&self) -> usize {
        self.components.len()
    }
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// GM_Menu::FindComponentByID (GAME 0x5eb2f0; sibling
    /// game/ui/g_menu.cpp:1081-1088, cited only): linear search, None for a
    /// missing component.
    pub fn find_component_by_id(&self, id: u32) -> Option<&MenuComponent> {
        self.components.iter().find(|component| component.id == id)
    }
    fn find_component_by_id_mut(&mut self, id: u32) -> Option<&mut MenuComponent> {
        self.components
            .iter_mut()
            .find(|component| component.id == id)
    }

    /// GM_Menu::SetDisabled (GAME 0x5eaed0; sibling game/ui/g_menu.cpp:903-914,
    /// cited only): no-op where the menu has no such component.
    pub fn set_disabled(&mut self, id: u32, enabled: bool) {
        if let Some(component) = self.find_component_by_id_mut(id) {
            component.set_disabled(enabled);
        }
    }

    /// GM_Menu::SetHidden (GAME 0x5eaf20; sibling game/ui/g_menu.cpp:915-924,
    /// cited only): no-op where the menu has no such component.
    pub fn set_hidden(&mut self, id: u32, visible: bool) {
        if let Some(component) = self.find_component_by_id_mut(id) {
            component.set_hidden(visible);
        }
    }

    /// GM_Menu::SetTextID (GAME 0x5eafd0, RVA 0x1eafd0; sibling
    /// game/ui/g_menu.cpp:929-935, cited only): records the text id on the
    /// looked-up component; no-op where the menu has no such component.
    pub fn set_text_id(&mut self, id: u32, text_id: u32) {
        if let Some(component) = self.find_component_by_id_mut(id) {
            component.set_text_id(text_id);
        }
    }

    /// GM_Menu::GetTextID (GAME 0x5eaff0, RVA 0x1eaff0; sibling
    /// game/ui/g_menu.cpp:937-944, cited only): the component's latched text
    /// id, 0 where the menu has no such component.
    pub fn get_text_id(&self, id: u32) -> u32 {
        self.find_component_by_id(id)
            .map_or(0, MenuComponent::get_text_id)
    }

    /// GM_Menu::SetText (GAME VA 0x5eb010, RVA 0x1eb010; sibling
    /// game/ui/g_menu.cpp:946-952, cited only): stages the text bytes on the
    /// looked-up component; no-op where the menu has no such component.
    /// The native stores the caller pointer (base `GM_Component::SetText`,
    /// sibling game/ui/g_m_comps.h:108); the host stages a copy, and the
    /// class-specific device effects stay DEFERRED.
    pub fn set_text(&mut self, id: u32, text: Vec<u8>) {
        if self.find_component_by_id(id).is_some() {
            self.staged_texts.insert(id, text);
        }
    }

    /// GM_Menu::GetText (GAME VA 0x5eb050, RVA 0x1eb050; sibling
    /// game/ui/g_menu.cpp:962-969, cited only): the staged text, None where
    /// the menu has no such component or nothing was staged (the native NULL
    /// in both cases; text-database resolution stays DEFERRED).
    pub fn get_text(&self, id: u32) -> Option<&[u8]> {
        if self.find_component_by_id(id).is_none() {
            return None;
        }
        self.staged_texts.get(&id).map(Vec::as_slice)
    }

    /// GM_Menu::SetString (GAME RVA 0x1eb030, VA 0x5eb030; reference mafia-re
    /// pin fe7e746 `game/ui/g_menu.cpp:954-960`, cited only):
    /// explicit-Unsupported STUB. The body looks the component up and calls
    /// its `SetString` virtual, but the base virtual is an empty no-op
    /// (sibling `game/ui/g_m_comps.h:109`) and every override needs the
    /// missing component-class owners, so nothing observable stages and the
    /// call is rejected. Unlike `set_text`, nothing lands in the staged
    /// texts, matching the native base-class effect (none).
    pub fn set_string(&self, _id: u32, _text: Option<&[u8]>) -> Result<(), MenuStubError> {
        Err(MenuStubError::Unsupported("component classes"))
    }

    /// GM_Menu::SetFloat (GAME VA 0x5eb070, RVA 0x1eb070; sibling
    /// game/ui/g_menu.cpp:971-977, cited only): stages the float on the
    /// looked-up component (progress bars, sliders); no-op where missing.
    pub fn set_float(&mut self, id: u32, value: f32) {
        if self.find_component_by_id(id).is_some() {
            self.staged_floats.insert(id, value.to_bits());
        }
    }

    /// GM_Menu::GetFloat (GAME VA 0x5eb090, RVA 0x1eb090; sibling
    /// game/ui/g_menu.cpp:979-986, cited only): the staged float, 0.0 where
    /// the menu has no such component or nothing was staged (the native
    /// base-class default).
    pub fn get_float(&self, id: u32) -> f32 {
        if self.find_component_by_id(id).is_none() {
            return 0.0;
        }
        self.staged_floats
            .get(&id)
            .map(|bits| f32::from_bits(*bits))
            .unwrap_or(0.0)
    }

    /// GM_Menu::SetDword (GAME VA 0x5eb0c0, RVA 0x1eb0c0; sibling
    /// game/ui/g_menu.cpp:988-994, cited only): stages the dword on the
    /// looked-up component; no-op where missing.
    pub fn set_dword(&mut self, id: u32, value: u32) {
        if self.find_component_by_id(id).is_some() {
            self.staged_dwords.insert(id, value);
        }
    }

    /// GM_Menu::GetDword (GAME VA 0x5eb0e0, RVA 0x1eb0e0; sibling
    /// game/ui/g_menu.cpp:996-1003, cited only): the staged dword, 0 where
    /// the menu has no such component or nothing was staged.
    pub fn get_dword(&self, id: u32) -> u32 {
        if self.find_component_by_id(id).is_none() {
            return 0;
        }
        self.staged_dwords.get(&id).copied().unwrap_or(0)
    }

    /// GM_Menu::SetBool (GAME VA 0x5eb100, RVA 0x1eb100; sibling
    /// game/ui/g_menu.cpp:1005-1011, cited only): stages the flag on the
    /// looked-up component; no-op where missing.
    pub fn set_bool(&mut self, id: u32, value: bool) {
        if self.find_component_by_id(id).is_some() {
            self.staged_bools.insert(id, value);
        }
    }

    /// GM_Menu::GetBool (GAME VA 0x5eb120, RVA 0x1eb120; sibling
    /// game/ui/g_menu.cpp:1013-1020, cited only): the staged flag, false
    /// where the menu has no such component or nothing was staged.
    pub fn get_bool(&self, id: u32) -> bool {
        if self.find_component_by_id(id).is_none() {
            return false;
        }
        self.staged_bools.get(&id).copied().unwrap_or(false)
    }

    /// GM_Menu::SetPointer (GAME RVA 0x1eb140, VA 0x5eb140; reference
    /// mafia-re pin fe7e746 `game/ui/g_menu.cpp:1022-1028`, cited only):
    /// explicit-Unsupported STUB. The body looks the component up and calls
    /// its `SetPointer` virtual, but the base virtual is an empty no-op
    /// (sibling `game/ui/g_m_comps.h:117`) and the only override stores a
    /// raw player pointer needing the missing component-class owner. The
    /// address stages as an opaque integer and is never dereferenced or
    /// converted into a host pointer; the call is rejected before any use.
    pub fn set_pointer(&self, _id: u32, _value_addr: usize) -> Result<(), MenuStubError> {
        Err(MenuStubError::Unsupported("component classes"))
    }

    /// GM_Menu::GetRangeFloat (GAME RVA 0x1eb160, VA 0x5eb160; reference
    /// mafia-re pin fe7e746 `game/ui/g_menu.cpp:1030-1036`, cited only):
    /// explicit-Unsupported STUB. The body looks the component up and reads
    /// its float range, leaving the out-words untouched where the menu has
    /// no such component; the ranges live in the missing component-class
    /// owners (the base virtual is an empty no-op, sibling
    /// `game/ui/g_m_comps.h:120`), so the out-words stay untouched here and
    /// the call is rejected.
    pub fn get_range_float(
        &self,
        _id: u32,
        _min: &mut f32,
        _max: &mut f32,
    ) -> Result<(), MenuStubError> {
        Err(MenuStubError::Unsupported("component classes"))
    }

    /// GM_Menu::SetRangeDword (GAME RVA 0x1eb190, VA 0x5eb190; reference
    /// mafia-re pin fe7e746 `game/ui/g_menu.cpp:1038-1044`, cited only):
    /// explicit-Unsupported STUB. The body looks the component up and writes
    /// its dword range (scrollbar classes, siblings `game/ui/g_m_comps.h:875,
    /// 1336, 1707`), whose component-class owners are missing; the base
    /// virtual is an empty no-op (sibling `game/ui/g_m_comps.h:121`), so
    /// nothing stages and the call is rejected.
    pub fn set_range_dword(&self, _id: u32, _min: u32, _max: u32) -> Result<(), MenuStubError> {
        Err(MenuStubError::Unsupported("component classes"))
    }

    fn set_active_id(&mut self, id: Option<u32>) {
        if let Some(old) = self.active {
            if let Some(component) = self.find_component_by_id_mut(old) {
                component.clear_active();
            }
        }
        self.active = id;
        if let Some(new) = id {
            if let Some(component) = self.find_component_by_id_mut(new) {
                component.set_active();
            }
        }
    }

    /// GM_Menu::ActivateByID (GAME 0x5eb1c0; sibling game/ui/g_menu.cpp:1046-1060,
    /// cited only): clears the old ACTIVE bit, takes the id's component, keeps
    /// it only where ENABLED|VISIBLE|ACTIVATABLE all hold, else none.
    pub fn activate_by_id(&mut self, id: u32) {
        if let Some(old) = self.active {
            if let Some(component) = self.find_component_by_id_mut(old) {
                component.clear_active();
            }
        }
        let take = self
            .find_component_by_id(id)
            .is_some_and(MenuComponent::can_activate);
        if take {
            self.active = Some(id);
            if let Some(component) = self.find_component_by_id_mut(id) {
                component.set_active();
            }
        } else {
            self.active = None;
        }
    }

    /// GM_Menu::ActivateByIDOrNext (GAME 0x5eb210; sibling
    /// game/ui/g_menu.cpp:1062-1079, cited only): ActivateByID, else the
    /// FindComponentByIDNext fallback, which always sets ACTIVE where it lands.
    pub fn activate_by_id_or_next(&mut self, id: u32) {
        self.activate_by_id(id);
        if self.active.is_none() {
            let fallback = self.find_by_id_next(id);
            self.set_active_id(fallback);
        }
    }

    /// GM_Menu::FindComponentByIDNext (GAME 0x5eb3c0; sibling
    /// game/ui/g_menu.cpp:1115-1139, cited only): lowest activatable id above
    /// `id`, else the lowest activatable id. None where nothing can activate.
    pub fn find_by_id_next(&self, id: u32) -> Option<u32> {
        let mut best: Option<u32> = None;
        for component in self.components.iter().filter(|c| c.can_activate()) {
            if component.id > id && best.is_none_or(|current| component.id < current) {
                best = Some(component.id);
            }
        }
        if best.is_some() {
            return best;
        }
        self.components
            .iter()
            .filter(|component| component.can_activate())
            .map(|component| component.id)
            .min()
    }

    /// GM_Menu::FindComponentByIDPrev (GAME 0x5eb320; sibling
    /// game/ui/g_menu.cpp:1090-1114, cited only): highest activatable id below
    /// `id`, else the highest activatable id. None where nothing can activate.
    pub fn find_by_id_prev(&self, id: u32) -> Option<u32> {
        let mut best: Option<u32> = None;
        for component in self.components.iter().filter(|c| c.can_activate()) {
            if component.id < id && best.is_none_or(|current| component.id > current) {
                best = Some(component.id);
            }
        }
        if best.is_some() {
            return best;
        }
        self.components
            .iter()
            .filter(|component| component.can_activate())
            .map(|component| component.id)
            .max()
    }

    /// GM_Menu::FindComponentNext (GAME 0x5eb4e0; sibling
    /// game/ui/g_menu.cpp:1161-1183, cited only): the activatable component
    /// after the one of `id` in list order, wrapping to the start (the start
    /// element itself excluded, as the original's `it != start` bound). None
    /// for an empty list or where nothing else can be activated.
    pub fn find_list_next(&self, id: u32) -> Option<u32> {
        let start = self.components.iter().position(|c| c.id == id)?;
        for offset in 1..self.components.len() {
            let candidate = &self.components[(start + offset) % self.components.len()];
            if candidate.can_activate() {
                return Some(candidate.id);
            }
        }
        None
    }

    /// GM_Menu::FindComponentPrev (GAME 0x5eb460; sibling
    /// game/ui/g_menu.cpp:1140-1160, cited only): the activatable component
    /// before the one of `id` in list order, wrapping from the end. None for
    /// an empty list or where nothing else can be activated.
    pub fn find_list_prev(&self, id: u32) -> Option<u32> {
        let count = self.components.len();
        let start = self.components.iter().position(|c| c.id == id)?;
        for offset in 1..count {
            let candidate = &self.components[(start + count - offset) % count];
            if candidate.can_activate() {
                return Some(candidate.id);
            }
        }
        None
    }

    /// GM_Menu::Tick pure activation routing (GAME 0x5ea8c0; sibling
    /// game/ui/g_menu.cpp:777-827, cited only): a host-decoded navigation key
    /// moves the active id through the list/id-order searches. Returns true
    /// where the selection changed; the caller then runs OnChange plus the
    /// change sound, which need component/device owners (DEFERRED).
    pub fn route_key(&mut self, key: TickKey) -> bool {
        let current = self.active.unwrap_or(0);
        let next = match key {
            TickKey::Left => self.find_list_prev(current),
            TickKey::Right => self.find_list_next(current),
            TickKey::Up => self.find_by_id_prev(current),
            TickKey::Down => self.find_by_id_next(current),
            TickKey::TabForward => self.find_list_next(current),
            TickKey::TabBackward => self.find_list_prev(current),
        };
        match next {
            Some(id) if Some(id) != self.active => {
                self.set_active_id(Some(id));
                true
            }
            _ => false,
        }
    }

    /// Mouse-in-use hover routing (sibling game/ui/g_menu.cpp:806-807,
    /// root->FindComponentAt, cited only): takes the already-resolved component
    /// id under the cursor; the pixel hit test (IsOverClientArea/IsOverActive
    /// plus viewport scale) is admitted by the caller through MenuUi::point.
    /// Returns true where the selection changed.
    pub fn hover(&mut self, id: u32) -> bool {
        if self
            .find_component_by_id(id)
            .is_some_and(MenuComponent::can_activate)
            && self.active != Some(id)
        {
            self.set_active_id(Some(id));
            return true;
        }
        false
    }

    /// Click routing for the latched active id into the main-menu OnClick
    /// (sibling game/ui/g_menu.cpp:828-829, cited only): pure dispatch, no
    /// device effects.
    pub fn click_active(&self) -> Option<main_menu::MainMenuClickEffect> {
        self.active.and_then(main_menu::main_menu_onclick)
    }

    /// Live read of the latched active selection, if any.
    pub fn active_id(&self) -> Option<u32> {
        self.active
    }

    /// Live reads of the close latch (GM_Menu::MenuClose 0x5eae10 code/flag).
    pub fn is_closing(&self) -> bool {
        self.closing
    }
    pub fn close_code(&self) -> u32 {
        self.close_code
    }
    /// MenuOpen's id, handed back to the parent in OnCreateChild/OnCloseChild
    /// (sibling game/ui/g_menu.h:288-289, cited only).
    pub fn open_id(&self) -> u32 {
        self.open_id
    }

    /// GM_Menu::MenuClose pure latch (GAME 0x5eae10; sibling
    /// game/ui/g_menu.cpp:883-891, cited only): records the code and raises
    /// the closing flag. The parent OnCloseChild call plus OnClose need the
    /// menu-class owner: DEFERRED.
    pub fn menu_close(&mut self, code: u32) {
        self.close_code = code;
        self.closing = true;
    }

    /// GM_Menu::MenuCloseAll pure latch (GAME 0x5eae40; sibling
    /// game/ui/g_menu.cpp:893-901, cited only): this menu's close plus the
    /// caller's result for the host loop. The parent chain and the
    /// zz_globalReturnCode global are device state: DEFERRED.
    pub fn menu_close_all(&mut self, code: u32, result: u32) -> u32 {
        self.menu_close(code);
        result
    }

    /// GM_Menu::MenuExchange pure rebuild (GAME 0x5ead80; sibling
    /// game/ui/g_menu.cpp:865-881, cited only): drops the components,
    /// recreates those of the exchanged definition, re-activates the old
    /// active id (or the next one). The OnBeforeExchange/OnExchange/OnChange
    /// callbacks need the menu-class owners: DEFERRED; the re-activated id is
    /// returned for them.
    pub fn menu_exchange(&mut self, definition: &MenuDefinition) -> Option<u32> {
        let id = self.active.unwrap_or(1);
        let (open_id, closing, close_code) = (self.open_id, self.closing, self.close_code);
        *self = Self::create_from_definition(definition);
        self.open_id = open_id;
        self.closing = closing;
        self.close_code = close_code;
        self.activate_by_id_or_next(id);
        self.active
    }

    /// GM_Menu::Destroy pure subset (GAME RVA 0x1ea690, VA 0x5ea690;
    /// reference mafia-re pin fe7e746 `game/ui/g_menu.cpp:693-700`, cited
    /// only): drops every admitted component with its staged values and
    /// resets the active/close latches. The native `delete this` frees the
    /// menu object itself, which has no host spelling: the caller drops this
    /// stack for that half.
    pub fn destroy(&mut self) {
        *self = Self::new();
    }

    /// GM_Menu::ForceUpdate (GAME RVA 0x1ea6d0, VA 0x5ea6d0; reference
    /// mafia-re pin fe7e746 `game/ui/g_menu.cpp:702-707`, cited only):
    /// explicit-Unsupported STUB. The body fans out to the components'
    /// `ForceUpdate` virtuals, whose component-class owners are missing, so
    /// nothing runs and the call is rejected.
    pub fn force_update(&self) -> Result<(), MenuStubError> {
        Err(MenuStubError::Unsupported("component classes"))
    }
}

/// GM_Menu::MenuOpen definition-name contract (GAME 0x5ead50; sibling
/// game/ui/g_menu.cpp:855-863, cited only): names the definition the opened
/// menu builds. Allocation, parent linkage, Create and the activeMenuWindow
/// global need the device menu owner and stay DEFERRED.
pub fn menu_open_definition(target: main_menu::MainMenuOpenTarget) -> &'static str {
    target.definition_name()
}

/// Loading-screen progress-bar item id (`LOADPROGRESS_BAR 0x12c`, sibling
/// game/ui/g_m_func.h:162, cited only): the `pbar` the mission opening fills
/// to `mission_part` and the game init fills for the rest.
pub const LOADPROGRESS_BAR_ID: u32 = 0x12c;

/// GM_LoadProgress::SetProgressGameInit pure bar math (GAME VA 0x5e3cf0,
/// RVA 0x1e3cf0; sibling game/ui/g_m_func.cpp:1008-1013, cited only): the
/// game-init fraction mapped onto the bar past `mission_part`. The
/// `SetRawProgress` routing (`SetFloat(LOADPROGRESS_BAR, ...)`) stays with
/// the menu host (`MenuStack::set_float`); device drawing/presenting stays
/// DEFERRED.
pub fn load_progress_game_init_value(mission_part: f32, progress: f32) -> f32 {
    (1.0 - mission_part) * progress + mission_part
}

/// GM_LoadProgress::SetProgressMissionOpen pure bar math (GAME VA 0x5e3d20,
/// RVA 0x1e3d20; sibling game/ui/g_m_func.cpp:1015-1019, cited only): the
/// mission-open fraction mapped onto the bar up to `mission_part`. Same
/// routing split as above.
pub fn load_progress_mission_open_value(mission_part: f32, progress: f32) -> f32 {
    progress * mission_part
}

/// One mission entry of tables\load.def (working name; `S_load_def`,
/// sibling game/ui/g_m_func.cpp:263-269, cited only): the mission folder
/// (`mission`, e.g. "mise01"), the loading picture (`picture`, e.g.
/// "M01.tga") and the text-database name id (`name_id`, 0 for none).
/// The fixed 0x20-byte ASCII fields are held as Rust strings; over-long or
/// non-ASCII intake is rejected by the file owner (DEFERRED).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadDefEntry {
    pub mission: String,
    pub picture: String,
    pub name_id: u32,
}

/// Pure match fragment of `GM_LoadProgress::OnCreate` (part of the occupied
/// GAME 0x5e3b90 body; sibling game/ui/g_m_func.cpp:972-986, cited only):
/// the first entry whose mission folder matches `mission_name`
/// ASCII-case-insensitively (the native `_stricmp`), `None` where none
/// matches. The 8000-byte file intake, the picture/name-id routing
/// (`SetString`/`SetTextID`) and the fallback to the running mission stay
/// with the file/menu owners (DEFERRED). No function credit is taken.
pub fn find_load_def_entry(entries: &[LoadDefEntry], mission_name: &str) -> Option<usize> {
    entries
        .iter()
        .position(|entry| entry.mission.eq_ignore_ascii_case(mission_name))
}

/// CreateCompsByDef file probe (GAME 0x5ea370; sibling game/ui/g_menu.cpp:639-660,
/// cited only): the localized `<name>_<ll>.mnu` first, then `<name>.mnu`;
/// missing files fall through to the next candidate, unparseable content
/// errors. Name validation plus candidate order come from the asset owner
/// (`MenuDefinition::menu_file_candidates`).
pub fn load_definition_for_menu(
    installation: &Installation,
    name: &str,
    language_code: u32,
) -> Result<MenuDefinition, String> {
    let candidates = MenuDefinition::menu_file_candidates(name, language_code)
        .ok_or_else(|| format!("Menu name {name} is not admittable"))?;
    for path in &candidates {
        match installation.read(path) {
            Ok(bytes) => {
                return lhr_assets::parse_menu_definition(&bytes).map_err(|e| e.to_string());
            }
            Err(_) => continue,
        }
    }
    Err(format!("Menu definition {name} is unavailable"))
}

/// GM_MainMenu::OnCreate dispatch over this stack (GAME 0x5e0690; sibling
/// game/ui/g_m_func.cpp:286-294, cited only): SetDisabled with real COMPF bit
/// sense, then ActivateByIDOrNext(INTRO). The camera latch plus the
/// FreerideStoreA triple run in the lib latch owners
/// (`main_menu_oncreate_dispatch`); frame positioning stays DEFERRED.
pub fn oncreate_stack_activate(stack: &mut MenuStack, extreme_gated: bool) -> i32 {
    if extreme_gated {
        stack.set_disabled(main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME, false);
    }
    stack.activate_by_id_or_next(main_menu::MAINMENU_ITEM_INTRO);
    0
}

/// GM_MainMenuExit::OnCreate (Game.exe VA 0x5e08f0, RVA 0x1e08f0; sibling
/// game/ui/g_m_func.cpp:363-368, cited only): shows "Quit Game?" in the
/// question window, then returns 0. Glyph rendering plus the component-class
/// text owner stay DEFERRED; only the text-id request is owned here.
pub fn main_menu_exit_oncreate(stack: &mut MenuStack) -> i32 {
    stack.set_text_id(main_menu::EXITYESNO_WINDOW, main_menu::TEXT_QUIT_GAME);
    0
}
fn validate_layout(definition: &MenuDefinition) -> Result<(), String> {
    if definition
        .items
        .first()
        .is_none_or(|item| item.kind != *b"dniw")
    {
        return Err("Main menu requires an original window root".into());
    }
    if definition.items.len() > 64
        || definition.items.iter().any(|item| {
            item.rect.iter().any(|v| !v.is_finite() || v.abs() > 1600.0)
                || item.rect[2] <= 0.0
                || item.rect[3] <= 0.0
                || item.rect[2] > 800.0
                || item.rect[3] > 600.0
        })
    {
        return Err("Menu layout exceeds supported canvas quota".into());
    }
    let mut ids = std::collections::HashSet::new();
    if definition.items.iter().any(|item| !ids.insert(item.id)) {
        return Err("Duplicate menu component ID".into());
    }
    if !definition
        .items
        .iter()
        .any(|item| item.kind == *b"ttub" && enabled(item.id))
    {
        return Err("Main menu has no supported selectable action".into());
    }
    Ok(())
}
pub fn quad(texture: Texture) -> Mesh {
    Mesh {
        material: Default::default(),
        vertices: vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [800.0, 0.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [800.0, 600.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.0, 600.0, 0.0],
                uv: [0.0, 1.0],
            },
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(Arc::new(texture)),
    }
}
pub fn design_position(position: [f32; 2], size: [u32; 2]) -> [f32; 2] {
    let scale = (size[0] as f32 / 800.0).min(size[1] as f32 / 600.0);
    [
        (position[0] - (size[0] as f32 - 800.0 * scale) * 0.5) / scale,
        (position[1] - (size[1] as f32 - 600.0 * scale) * 0.5) / scale,
    ]
}
struct Canvas {
    rgba: Vec<u8>,
}
impl Canvas {
    fn new() -> Self {
        Self {
            rgba: vec![0; 800 * 600 * 4],
        }
    }
    fn pixel(&mut self, x: usize, y: usize, p: [u8; 4]) {
        let dest = &mut self.rgba[(y * 800 + x) * 4..(y * 800 + x + 1) * 4];
        let alpha = u32::from(p[3]);
        let out = alpha + u32::from(dest[3]) * (255 - alpha) / 255;
        if out == 0 {
            return;
        }
        for c in 0..3 {
            dest[c] = ((u32::from(p[c]) * alpha
                + u32::from(dest[c]) * u32::from(dest[3]) * (255 - alpha) / 255)
                / out) as u8;
        }
        dest[3] = out as u8;
    }
    fn rectangle(&mut self, rect: [f32; 4], color: [u8; 4]) {
        for y in (rect[1].max(0.0) as usize)..((rect[1] + rect[3]).ceil().min(600.0) as usize) {
            for x in (rect[0].max(0.0) as usize)..((rect[0] + rect[2]).ceil().min(800.0) as usize) {
                self.pixel(x, y, color);
            }
        }
    }
    fn image(&mut self, image: &Image, rect: [f32; 4], uv: [f32; 4], tint: Option<[u8; 3]>) {
        for y in (rect[1].max(0.0) as usize)..((rect[1] + rect[3]).ceil().min(600.0) as usize) {
            let sy = ((uv[1] + (y as f32 - rect[1]) / rect[3] * (uv[3] - uv[1]))
                * image.height as f32)
                .clamp(0.0, image.height as f32 - 1.0) as usize;
            for x in (rect[0].max(0.0) as usize)..((rect[0] + rect[2]).ceil().min(800.0) as usize) {
                let sx = ((uv[0] + (x as f32 - rect[0]) / rect[2] * (uv[2] - uv[0]))
                    * image.width as f32)
                    .clamp(0.0, image.width as f32 - 1.0) as usize;
                let p = &image.rgba[(sy * image.width as usize + sx) * 4
                    ..(sy * image.width as usize + sx + 1) * 4];
                let rgb = tint.unwrap_or([p[0], p[1], p[2]]);
                self.pixel(x, y, [rgb[0], rgb[1], rgb[2], p[3]]);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> MenuUi {
        let mut bytes = b"GFnt".to_vec();
        bytes.extend([0; 24 + 256 * 16]);
        let font = lhr_assets::parse_font_metrics(&bytes).unwrap();
        let image = || Image {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        let mut items = vec![lhr_assets::MenuItem {
            id: 1,
            kind: *b"dniw",
            rect: [40.0, 40.0, 217.0, 353.0],
            text_id: 1,
            style: 0,
        }];
        for (i, id) in [990101, 990103, 990106, 990113].into_iter().enumerate() {
            items.push(lhr_assets::MenuItem {
                id,
                kind: *b"ttub",
                rect: [10.0, 12.0 + i as f32 * 25.0, 200.0, 30.0],
                text_id: 1,
                style: 0,
            });
        }
        MenuUi {
            definition: MenuDefinition {
                version: 1,
                reserved: 0,
                items,
            },
            labels: vec![b"Fixture".to_vec(); 5],
            font: font.clone(),
            title_font: font,
            atlas: image(),
            title_atlas: image(),
            language: [0; 256],
            header: image(),
            paper: image(),
            selected: 1,
        }
    }
    #[test]
    fn keyboard_and_pointer_skip_unimplemented_actions_and_dispatch_original_ids() {
        let mut ui = fixture();
        assert_eq!(ui.activate(), Action::NewGame);
        ui.move_selection(1);
        assert_eq!(ui.activate(), Action::Quit);
        ui.move_selection(0);
        assert_eq!(ui.activate(), Action::Quit);
        ui.move_selection(1);
        assert_eq!(ui.activate(), Action::NewGame);
        assert!(!ui.hit([60.0, 95.0]));
        assert!(!ui.point([60.0, 95.0]));
        assert!(!ui.hit([-10.0, -10.0]));
        assert!(ui.point([60.0, 175.0]));
        assert_eq!(ui.activate(), Action::Quit);
        ui.move_selection(-1);
        assert_eq!(ui.activate(), Action::NewGame);
    }

    #[test]
    fn navigation_wraps_enabled_rows_and_leaves_a_single_action_stable() {
        let mut ui = fixture();
        for navigation in [
            Navigation::Next,
            Navigation::Previous,
            Navigation::NextId,
            Navigation::PreviousId,
        ] {
            assert!(ui.navigate(navigation));
            assert_eq!(ui.activate(), Action::Quit);
            assert!(ui.navigate(navigation));
            assert_eq!(ui.activate(), Action::NewGame);
        }
        // Keep the disabled rows in their original positions. A navigation
        // press must neither focus one of them nor repaint unchanged focus.
        ui.definition.items[4].id = 990106;
        for navigation in [
            Navigation::Next,
            Navigation::Previous,
            Navigation::NextId,
            Navigation::PreviousId,
        ] {
            assert!(!ui.navigate(navigation));
            assert_eq!(ui.activate(), Action::NewGame);
        }
    }

    #[test]
    fn lab_captured_movement_cannot_activate_stale_menu_position_or_selection() {
        let mut ui = fixture();
        let mut pointer = Pointer::default();
        let viewport = [800, 600];
        assert!(!pointer.select_for_click(&mut ui, viewport));
        pointer.observe([60., 175.]);
        assert!(pointer.hover(&mut ui, viewport));
        assert_eq!(ui.activate(), Action::Quit);

        // A Lab drag updates physical position, but has no underlying hover
        // effect. After release, a stationary click in empty space does nothing.
        pointer.observe([700., 500.]);
        assert_eq!(ui.activate(), Action::Quit);
        assert!(!pointer.select_for_click(&mut ui, viewport));
        assert_eq!(ui.activate(), Action::Quit);

        // The same captured move into a different real row must select that
        // row on the later admitted click, not the previously selected Quit.
        pointer.observe([60., 125.]);
        assert_eq!(ui.activate(), Action::Quit);
        assert!(pointer.select_for_click(&mut ui, viewport));
        assert_eq!(ui.activate(), Action::NewGame);

        // Conversion is made with the click's current viewport, not the one
        // that was active when the last cursor event was captured.
        pointer.observe([120., 350.]);
        assert!(pointer.select_for_click(&mut ui, [1600, 1200]));
        assert_eq!(ui.activate(), Action::Quit);
        pointer.leave();
        assert!(!pointer.select_for_click(&mut ui, viewport));
    }
    #[test]
    fn unsupported_empty_duplicate_and_all_disabled_layouts_are_rejected() {
        let definition = MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![],
        };
        assert!(validate_layout(&definition).is_err());
        let root = lhr_assets::MenuItem {
            id: 1,
            kind: *b"dniw",
            rect: [40.0, 40.0, 217.0, 353.0],
            text_id: 101,
            style: 0,
        };
        let mut definition = MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![root.clone()],
        };
        assert!(validate_layout(&definition).is_err());
        definition.items.push(lhr_assets::MenuItem {
            id: 990106,
            kind: *b"ttub",
            ..root.clone()
        });
        assert!(validate_layout(&definition).is_err());
        definition.items[1].id = 990103;
        assert!(validate_layout(&definition).is_ok());
        definition.items.push(root);
        assert!(validate_layout(&definition).is_err());
    }
    #[test]
    fn pointer_coordinates_account_for_letterboxing() {
        assert_eq!(design_position([320.0, 240.0], [640, 480]), [400.0, 300.0]);
        assert_eq!(design_position([480.0, 320.0], [960, 640]), [400.0, 300.0]);
        assert!(design_position([0.0, 0.0], [960, 640])[0] < 0.0);
    }
    #[test]
    fn alpha_composition_preserves_transparent_canvas() {
        let mut canvas = Canvas::new();
        canvas.pixel(1, 1, [255, 0, 0, 128]);
        assert_eq!(&canvas.rgba[3204..3208], &[255, 0, 0, 128]);
        canvas.pixel(1, 1, [0, 0, 255, 255]);
        assert_eq!(&canvas.rgba[3204..3208], &[0, 0, 255, 255]);
    }

    struct Resources(std::path::PathBuf);
    impl Drop for Resources {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    impl Resources {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "lhr-menu-language-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir(&root).unwrap();
            let resources = Self(root);
            let mut menu = b"Menu".to_vec();
            for word in [1u32, 2, 0] {
                menu.extend(word.to_le_bytes());
            }
            for (id, kind, rect, text) in [
                (1u32, *b"dniw", [10f32, 10., 200., 100.], 100u32),
                (990103, *b"ttub", [10., 10., 100., 20.], 101),
            ] {
                menu.extend(id.to_le_bytes());
                menu.extend(kind);
                for value in rect {
                    menu.extend(value.to_le_bytes());
                }
                menu.extend(text.to_le_bytes());
                menu.extend(0u32.to_le_bytes());
            }
            resources.write("tables/menu/mainmenu.mnu", &menu);
            let mut font = vec![0; 28 + 256 * 16];
            font[..4].copy_from_slice(b"GFnt");
            for (offset, value) in [(8, 1f32), (12, 1.), (16, 1.)] {
                font[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            for (index, width) in [(1, 0.25f32), (5, 0.5), (6, 0.75)] {
                let offset = 28 + index * 16;
                font[offset + 8..offset + 12].copy_from_slice(&1f32.to_le_bytes());
                font[offset + 12..offset + 16].copy_from_slice(&width.to_le_bytes());
            }
            for name in ["tables/font1_lat.gmf", "tables/font2_lat.gmf"] {
                resources.write(name, &font);
            }
            let mut tga = vec![0; 18];
            tga[2] = 2;
            tga[12] = 1;
            tga[14] = 1;
            tga[16] = 32;
            tga[17] = 0x28;
            tga.extend([255; 4]);
            for name in [
                "maps/font1_lat.tga",
                "maps/font2_lat.tga",
                "maps/papir5a.tga",
                "maps/papir3.tga",
            ] {
                resources.write(name, &tga);
            }
            for (code, marker, glyph) in [("en", b'E', 5u16), ("cz", b'C', 6)] {
                let mut text = vec![0; 24];
                text[..4].copy_from_slice(&2u32.to_le_bytes());
                for (index, id, offset) in [(0, 100u32, 24u32), (1, 101, 26)] {
                    let start = 8 + index * 8;
                    text[start..start + 4].copy_from_slice(&id.to_le_bytes());
                    text[start + 4..start + 8].copy_from_slice(&offset.to_le_bytes());
                }
                text.extend([marker, 0, 0x81, marker, 0]);
                resources.write(&format!("tables/textdb_{code}.def"), &text);
                let mut language = vec![0; 568];
                language[..4].copy_from_slice(b"Lang");
                language[4..6].copy_from_slice(code.as_bytes());
                language[40..44].copy_from_slice(b"_lat");
                for index in 0..256 {
                    language[56 + index * 2..58 + index * 2].copy_from_slice(&1u16.to_le_bytes());
                }
                language[56 + 0x81 * 2..58 + 0x81 * 2].copy_from_slice(&glyph.to_le_bytes());
                resources.write(&format!("tables/lang_{code}.def"), &language);
            }
            resources
        }
        fn write(&self, name: &str, bytes: &[u8]) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }

    #[test]
    fn load_def_entry_matches_first_case_insensitive_hit() {
        // Pure fragment of GM_LoadProgress::OnCreate (part of GAME 0x5e3b90).
        let entries = vec![
            LoadDefEntry {
                mission: "mise01".into(),
                picture: "M01.tga".into(),
                name_id: 0x2cf,
            },
            LoadDefEntry {
                mission: "MISE01".into(),
                picture: "M01b.tga".into(),
                name_id: 0,
            },
            LoadDefEntry {
                mission: "freeride".into(),
                picture: "free.tga".into(),
                name_id: 0,
            },
        ];
        assert_eq!(find_load_def_entry(&entries, "mise01"), Some(0));
        assert_eq!(find_load_def_entry(&entries, "MISE01"), Some(0));
        assert_eq!(find_load_def_entry(&entries, "FreeRide"), Some(2));
        assert_eq!(find_load_def_entry(&entries, "mise02"), None);
        assert_eq!(find_load_def_entry(&[], "mise01"), None);
        // Zero name ids pass through untouched for the caller to gate on.
        assert_eq!(entries[1].name_id, 0);
    }

    #[test]
    fn data_version_selects_real_menu_labels_and_language_glyphs_without_a_codepage_guess() {
        let resources = Resources::new();
        let installation = Installation::open(&resources.0).unwrap();
        for (version, marker, width) in [(b"en-rest".as_slice(), b'E', 15f32), (b"cz", b'C', 20.)] {
            resources.write("tables/version.txt", version);
            let menu = MenuUi::load(&installation).unwrap();
            assert_eq!(menu.labels[0], [marker]);
            assert_eq!(menu.labels[1], [0x81, marker]);
            assert_eq!(menu.focus_id(), 990103);
            assert_eq!(
                menu.text(
                    &mut Canvas::new(),
                    &menu.labels[1],
                    [0., 0., 100., 20.],
                    false,
                    [0; 3]
                ),
                width
            );
            assert!(menu
                .pixels()
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] != 0));
        }
    }

    #[test]
    fn missing_and_short_data_version_default_to_english_but_selected_errors_do_not() {
        let resources = Resources::new();
        let installation = Installation::open(&resources.0).unwrap();
        assert_eq!(MenuUi::load(&installation).unwrap().labels[0], b"E");
        resources.write("tables/version.txt", b"c");
        assert_eq!(MenuUi::load(&installation).unwrap().labels[0], b"E");
        resources.write("tables/version.txt", b"en");
        resources.write("tables/textdb_en.def", b"invalid selected database");
        assert!(MenuUi::load(&installation).is_err());
        std::fs::remove_file(resources.0.join("tables/textdb_en.def")).unwrap();
        assert!(MenuUi::load(&installation)
            .err()
            .unwrap()
            .contains("textdb_en.def"));
        std::fs::remove_file(resources.0.join("tables/version.txt")).unwrap();
        std::fs::create_dir(resources.0.join("tables/version.txt")).unwrap();
        assert!(MenuUi::load(&installation)
            .err()
            .unwrap()
            .contains("Menu data version"));
        assert!(installation.read_optional("tables/version.txt").is_err());
        assert_eq!(
            installation.read_optional("tables/missing.def").unwrap(),
            None
        );
    }

    #[test]
    fn menu_uses_the_selected_font_family_and_preserves_source_uppercase_behavior() {
        let resources = Resources::new();
        let installation = Installation::open(&resources.0).unwrap();
        resources.write("tables/version.txt", b"EN");
        assert!(MenuUi::load(&installation)
            .err()
            .unwrap()
            .contains("textdb_%..def"));
        resources.write("tables/version.txt", b"en");
        let mut language = std::fs::read(resources.0.join("tables/lang_en.def")).unwrap();
        language[40..56].fill(0);
        language[40..45].copy_from_slice(b"_test");
        let mut font = std::fs::read(resources.0.join("tables/font1_lat.gmf")).unwrap();
        let width_offset = 28 + 5 * 16 + 12;
        font[width_offset..width_offset + 4].copy_from_slice(&1f32.to_le_bytes());
        for index in [1, 2] {
            resources.write(&format!("tables/font{index}_test.gmf"), &font);
            let atlas =
                std::fs::read(resources.0.join(format!("maps/font{index}_lat.tga"))).unwrap();
            resources.write(&format!("maps/font{index}_test.tga"), &atlas);
        }
        resources.write("tables/lang_en.def", &language);
        let menu = MenuUi::load(&installation).unwrap();
        assert_eq!(
            menu.text(
                &mut Canvas::new(),
                &menu.labels[1],
                [0., 0., 100., 20.],
                false,
                [0; 3]
            ),
            25.0
        );
        std::fs::remove_file(resources.0.join("tables/font1_test.gmf")).unwrap();
        assert!(MenuUi::load(&installation)
            .err()
            .unwrap()
            .contains("font1_test.gmf"));
        language[40..56].fill(0);
        language[40..46].copy_from_slice(b"../bad");
        resources.write("tables/lang_en.def", &language);
        assert_eq!(
            MenuUi::load(&installation).err().unwrap(),
            "Unsupported menu font suffix"
        );
        language[40..56].fill(b'x');
        resources.write("tables/lang_en.def", &language);
        assert_eq!(
            MenuUi::load(&installation).err().unwrap(),
            "Unterminated menu font suffix"
        );
        language[4..6].copy_from_slice(b"cz");
        resources.write("tables/lang_en.def", &language);
        assert_eq!(
            MenuUi::load(&installation).err().unwrap(),
            "Unsupported menu language table"
        );
    }

    fn stack_definition() -> MenuDefinition {
        let item = |id: u32, kind: [u8; 4]| lhr_assets::MenuItem {
            id,
            kind,
            rect: [10.0, 10.0, 100.0, 20.0],
            text_id: 0,
            style: 0,
        };
        MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![
                item(1, *b"dniw"),
                item(main_menu::MAINMENU_ITEM_INTRO, *b"ttub"),
                // The intake separator: never becomes a component.
                item(0, *b"upup"),
                item(main_menu::MAINMENU_ITEM_NEW_GAME, *b"ttub"),
                item(main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME, *b"ttub"),
                item(main_menu::MAINMENU_ITEM_QUIT, *b"ttub"),
            ],
        }
    }

    #[test]
    fn onclick_dispatch_covers_all_native_ids_through_the_latch_model() {
        let cases = [
            (main_menu::MAINMENU_ITEM_INTRO, Action::Intro),
            (main_menu::MAINMENU_ITEM_TUTORIAL, Action::Tutorial),
            (main_menu::MAINMENU_ITEM_NEW_GAME, Action::NewGame),
            (main_menu::MAINMENU_ITEM_LOAD_GAME, Action::LoadGame),
            (main_menu::MAINMENU_ITEM_FREE_RIDE, Action::FreeRide),
            (
                main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME,
                Action::Extreme,
            ),
            (main_menu::MAINMENU_ITEM_RACING, Action::Racing),
            (main_menu::MAINMENU_ITEM_MULTIPLAYER, Action::Multiplayer),
            (
                main_menu::MAINMENU_ITEM_CARCYCLOPEDIA,
                Action::Carcyclopedia,
            ),
            (main_menu::MAINMENU_ITEM_OPTIONS, Action::Options),
            (main_menu::MAINMENU_ITEM_CREDITS, Action::Credits),
            (main_menu::MAINMENU_ITEM_CHANGE_PROFILE, Action::Profile),
            (main_menu::MAINMENU_ITEM_QUIT, Action::Quit),
        ];
        for (id, action) in cases {
            assert_eq!(action_for_id(id), action);
        }
        assert_eq!(action_for_id(0), Action::Unsupported(0));
        assert_eq!(action_for_id(990100), Action::Unsupported(990100));
        // The fixture's focus (New Game) dispatches to its CloseAll outcome.
        let mut ui = fixture();
        ui.selected = 1;
        assert_eq!(ui.focus_id(), main_menu::MAINMENU_ITEM_NEW_GAME);
        assert_eq!(ui.activate(), Action::NewGame);
        assert_eq!(
            ui.click_effect(),
            Some(main_menu::MainMenuClickEffect::CloseAll {
                result: main_menu::MAINMENU_NEW_GAME,
            })
        );
    }

    #[test]
    fn create_intake_skips_upup_and_activates_intro_by_id_or_next() {
        let stack = MenuStack::create(&stack_definition());
        // Root plus four buttons; the `upup` item never becomes a component.
        assert_eq!(stack.len(), 5);
        assert!(!stack.is_empty());
        assert!(stack.find_component_by_id(0).is_none());
        // The root window is not activatable, so ActivateByIDOrNext(1) lands
        // on the lowest activatable id above it: Intro.
        assert_eq!(stack.active_id(), Some(main_menu::MAINMENU_ITEM_INTRO));
        assert!(
            stack
                .find_component_by_id(main_menu::MAINMENU_ITEM_INTRO)
                .unwrap()
                .can_activate()
        );
    }

    #[test]
    fn set_disabled_is_a_noop_for_missing_ids_and_gates_activation() {
        let mut stack = MenuStack::create(&stack_definition());
        stack.set_disabled(424242, false);
        assert_eq!(stack.active_id(), Some(main_menu::MAINMENU_ITEM_INTRO));
        // Disabling the active row keeps the latch; re-activation must fail
        // closed through the ENABLED gate.
        stack.set_disabled(main_menu::MAINMENU_ITEM_INTRO, false);
        stack.activate_by_id(main_menu::MAINMENU_ITEM_INTRO);
        assert_eq!(stack.active_id(), None);
        // The OrNext fallback skips the disabled row for New Game.
        stack.activate_by_id_or_next(main_menu::MAINMENU_ITEM_INTRO);
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_NEW_GAME)
        );
        stack.set_disabled(main_menu::MAINMENU_ITEM_INTRO, true);
        stack.activate_by_id(main_menu::MAINMENU_ITEM_INTRO);
        assert_eq!(stack.active_id(), Some(main_menu::MAINMENU_ITEM_INTRO));
    }

    #[test]
    fn set_hidden_toggles_visible_bit_and_keeps_activation() {
        // GAME 0x5eaf20 GM_Menu::SetHidden: missing ids are a no-op; hiding
        // clears only VISIBLE (ENABLED stays, so re-showing restores).
        let mut stack = MenuStack::create(&stack_definition());
        stack.set_hidden(424242, false);
        let intro = stack
            .find_component_by_id(main_menu::MAINMENU_ITEM_INTRO)
            .unwrap();
        assert!(intro.is_visible());
        stack.set_hidden(main_menu::MAINMENU_ITEM_INTRO, false);
        let intro = stack
            .find_component_by_id(main_menu::MAINMENU_ITEM_INTRO)
            .unwrap();
        assert!(!intro.is_visible());
        assert!(intro.is_enabled());
        stack.set_hidden(main_menu::MAINMENU_ITEM_INTRO, true);
        assert!(
            stack
                .find_component_by_id(main_menu::MAINMENU_ITEM_INTRO)
                .unwrap()
                .is_visible()
        );
    }

    #[test]
    fn id_and_list_searches_wrap_like_the_original_fallbacks() {
        let mut stack = MenuStack::create(&stack_definition());
        // Above the top: wrap to the lowest activatable id.
        assert_eq!(
            stack.find_by_id_next(u32::MAX),
            Some(main_menu::MAINMENU_ITEM_INTRO)
        );
        // Below the bottom: wrap to the highest activatable id.
        assert_eq!(
            stack.find_by_id_prev(0),
            Some(main_menu::MAINMENU_ITEM_QUIT)
        );
        assert_eq!(
            stack.find_by_id_next(main_menu::MAINMENU_ITEM_INTRO),
            Some(main_menu::MAINMENU_ITEM_NEW_GAME)
        );
        assert_eq!(
            stack.find_by_id_prev(main_menu::MAINMENU_ITEM_NEW_GAME),
            Some(main_menu::MAINMENU_ITEM_INTRO)
        );
        // List order wraps around the ends; the non-activatable root is
        // never returned.
        assert_eq!(
            stack.find_list_next(main_menu::MAINMENU_ITEM_QUIT),
            Some(main_menu::MAINMENU_ITEM_INTRO)
        );
        assert_eq!(
            stack.find_list_prev(main_menu::MAINMENU_ITEM_INTRO),
            Some(main_menu::MAINMENU_ITEM_QUIT)
        );
        assert_eq!(stack.find_list_next(424242), None);
        // All rows disabled: every search reports nothing to activate.
        for id in [
            main_menu::MAINMENU_ITEM_INTRO,
            main_menu::MAINMENU_ITEM_NEW_GAME,
            main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME,
            main_menu::MAINMENU_ITEM_QUIT,
        ] {
            stack.set_disabled(id, false);
        }
        assert_eq!(stack.find_by_id_next(1), None);
        assert_eq!(stack.find_by_id_prev(u32::MAX), None);
        assert_eq!(
            stack.find_list_next(main_menu::MAINMENU_ITEM_INTRO),
            None
        );
        assert_eq!(
            stack.find_list_prev(main_menu::MAINMENU_ITEM_QUIT),
            None
        );
    }

    #[test]
    fn tick_key_and_hover_routing_feed_the_onclick_dispatch() {
        let mut stack = MenuStack::create(&stack_definition());
        assert!(stack.route_key(TickKey::Down));
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_NEW_GAME)
        );
        assert!(stack.route_key(TickKey::Down));
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME)
        );
        assert!(stack.route_key(TickKey::Up));
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_NEW_GAME)
        );
        assert!(stack.route_key(TickKey::TabForward));
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME)
        );
        assert!(stack.route_key(TickKey::TabBackward));
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_NEW_GAME)
        );
        assert!(stack.route_key(TickKey::Left));
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_INTRO)
        );
        assert!(stack.route_key(TickKey::Right));
        assert_eq!(
            stack.active_id(),
            Some(main_menu::MAINMENU_ITEM_NEW_GAME)
        );
        // A key press that cannot move (single activatable row) changes nothing.
        let mut solo = MenuStack::create(&stack_definition());
        for id in [
            main_menu::MAINMENU_ITEM_NEW_GAME,
            main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME,
            main_menu::MAINMENU_ITEM_QUIT,
        ] {
            solo.set_disabled(id, false);
        }
        assert!(!solo.route_key(TickKey::Down));
        assert_eq!(
            solo.active_id(),
            Some(main_menu::MAINMENU_ITEM_INTRO)
        );
        assert!(stack.hover(main_menu::MAINMENU_ITEM_QUIT));
        assert_eq!(stack.active_id(), Some(main_menu::MAINMENU_ITEM_QUIT));
        assert!(!stack.hover(main_menu::MAINMENU_ITEM_QUIT));
        // Hovering the non-activatable root changes nothing.
        assert!(!stack.hover(1));
        assert_eq!(stack.active_id(), Some(main_menu::MAINMENU_ITEM_QUIT));
        assert_eq!(
            stack.click_active(),
            Some(main_menu::MainMenuClickEffect::Open {
                target: main_menu::MainMenuOpenTarget::ExitQuestion,
            })
        );
        assert_eq!(
            menu_open_definition(main_menu::MainMenuOpenTarget::FreeRide),
            "freeride"
        );
    }

    #[test]
    fn close_latches_and_exchange_rebuild_preserve_requests() {
        let mut stack = MenuStack::create(&stack_definition());
        assert!(!stack.is_closing());
        let result = stack.menu_close_all(
            main_menu::MENU_CLOSE_CANCEL,
            main_menu::MAINMENU_QUIT,
        );
        assert_eq!(result, main_menu::MAINMENU_QUIT);
        assert!(stack.is_closing());
        assert_eq!(stack.close_code(), main_menu::MENU_CLOSE_CANCEL);
        // Exchange keeps the close latch, rebuilds the list, and re-targets
        // the previously active id (Quit survives the rebuild here).
        stack.hover(main_menu::MAINMENU_ITEM_QUIT);
        let active = stack.menu_exchange(&stack_definition());
        assert_eq!(active, Some(main_menu::MAINMENU_ITEM_QUIT));
        assert_eq!(stack.active_id(), Some(main_menu::MAINMENU_ITEM_QUIT));
        assert!(stack.is_closing());
        // OnCreate stack activation gates Extreme, then selects Intro.
        let mut gated = MenuStack::create(&stack_definition());
        assert_eq!(oncreate_stack_activate(&mut gated, true), 0);
        assert!(
            !gated
                .find_component_by_id(main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME)
                .unwrap()
                .is_enabled()
        );
        gated.activate_by_id(main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME);
        assert_eq!(gated.active_id(), None);
        // The OrNext fallback skips the gated row for New Game.
        gated.activate_by_id_or_next(main_menu::MAINMENU_ITEM_FREE_RIDE_EXTREME);
        assert_eq!(
            gated.active_id(),
            Some(main_menu::MAINMENU_ITEM_QUIT)
        );
        assert_eq!(oncreate_stack_activate(&mut gated, false), 0);
        assert_eq!(
            gated.active_id(),
            Some(main_menu::MAINMENU_ITEM_INTRO)
        );
    }

    #[test]
    fn language_menu_probe_prefers_localized_then_falls_back() {
        let resources = Resources::new();
        let installation = Installation::open(&resources.0).unwrap();
        // Only the fallback exists: the probe falls through to it.
        let definition =
            load_definition_for_menu(&installation, "mainmenu", 0x656e).unwrap();
        assert_eq!(definition.items.len(), 2);
        assert!(load_definition_for_menu(&installation, "optionsb", 0x656e).is_err());
        assert!(load_definition_for_menu(&installation, "main menu", 0x656e).is_err());
        assert!(load_definition_for_menu(&installation, "", 0x656e).is_err());
    }

    #[test]
    fn set_text_id_0x1eafd0_and_get_text_id_0x1eaff0_gate_on_lookup() {
        let mut stack = MenuStack::create(&stack_definition());
        // Intake carries the definition's own text ids (all zero here).
        assert_eq!(stack.get_text_id(main_menu::MAINMENU_ITEM_INTRO), 0);
        assert_eq!(stack.get_text_id(424242), 0);
        // Missing ids: set is a no-op, get reads the neutral element.
        stack.set_text_id(424242, main_menu::TEXT_QUIT_GAME);
        assert_eq!(stack.get_text_id(424242), 0);
        stack.set_text_id(
            main_menu::MAINMENU_ITEM_INTRO,
            main_menu::TEXT_QUIT_GAME,
        );
        assert_eq!(
            stack.get_text_id(main_menu::MAINMENU_ITEM_INTRO),
            main_menu::TEXT_QUIT_GAME
        );
        // Other rows keep the neutral element.
        assert_eq!(stack.get_text_id(main_menu::MAINMENU_ITEM_NEW_GAME), 0);
        // Rewriting restores the previous latch, like the native store.
        stack.set_text_id(main_menu::MAINMENU_ITEM_INTRO, 0);
        assert_eq!(stack.get_text_id(main_menu::MAINMENU_ITEM_INTRO), 0);
    }

    #[test]
    fn main_menu_exit_oncreate_0x1e08f0_shows_quit_game_and_returns_zero() {
        let item = |id: u32| lhr_assets::MenuItem {
            id,
            kind: *b"ttub",
            rect: [10.0, 10.0, 100.0, 20.0],
            text_id: 0,
            style: 0,
        };
        let definition = MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![item(1), item(main_menu::EXITYESNO_WINDOW)],
        };
        let mut stack = MenuStack::create(&definition);
        assert_eq!(main_menu_exit_oncreate(&mut stack), 0);
        assert_eq!(
            stack.get_text_id(main_menu::EXITYESNO_WINDOW),
            main_menu::TEXT_QUIT_GAME
        );
        assert_eq!(main_menu::TEXT_QUIT_GAME, 0x77);
    }

    #[test]
    fn rva_1eb010_set_text_stages_only_for_known_component() {
        // GM_Menu::SetText GAME VA 0x5eb010 (RVA 0x1eb010).
        let mut stack = MenuStack::create(&stack_definition());
        stack.set_text(424242, b"missing".to_vec());
        assert_eq!(stack.get_text(424242), None);
        stack.set_text(main_menu::MAINMENU_ITEM_INTRO, b"intro".to_vec());
        assert_eq!(
            stack.get_text(main_menu::MAINMENU_ITEM_INTRO),
            Some(b"intro".as_slice())
        );
        // Other rows keep the NULL (None) latch.
        assert_eq!(stack.get_text(main_menu::MAINMENU_ITEM_NEW_GAME), None);
        stack.set_text(main_menu::MAINMENU_ITEM_INTRO, b"".to_vec());
        assert_eq!(
            stack.get_text(main_menu::MAINMENU_ITEM_INTRO),
            Some(b"".as_slice())
        );
    }

    #[test]
    fn rva_1eb050_get_text_returns_none_for_missing_component() {
        // GM_Menu::GetText GAME VA 0x5eb050 (RVA 0x1eb050).
        let stack = MenuStack::create(&stack_definition());
        assert_eq!(stack.get_text(424242), None);
        assert_eq!(stack.get_text(main_menu::MAINMENU_ITEM_QUIT), None);
    }

    #[test]
    fn rva_1eb070_set_float_stages_only_for_known_component() {
        // GM_Menu::SetFloat GAME VA 0x5eb070 (RVA 0x1eb070).
        let mut stack = MenuStack::create(&stack_definition());
        stack.set_float(424242, 0.75);
        assert_eq!(stack.get_float(424242), 0.0);
        stack.set_float(main_menu::MAINMENU_ITEM_NEW_GAME, 0.75);
        assert_eq!(
            stack.get_float(main_menu::MAINMENU_ITEM_NEW_GAME).to_bits(),
            0.75f32.to_bits()
        );
        stack.set_float(main_menu::MAINMENU_ITEM_NEW_GAME, -1.5);
        assert_eq!(
            stack.get_float(main_menu::MAINMENU_ITEM_NEW_GAME).to_bits(),
            (-1.5f32).to_bits()
        );
    }

    #[test]
    fn rva_1eb090_get_float_defaults_to_zero() {
        // GM_Menu::GetFloat GAME VA 0x5eb090 (RVA 0x1eb090): 0.0 where the
        // menu has no such component or nothing was staged.
        let stack = MenuStack::create(&stack_definition());
        assert_eq!(stack.get_float(424242), 0.0);
        assert_eq!(stack.get_float(main_menu::MAINMENU_ITEM_INTRO), 0.0);
    }

    #[test]
    fn rva_1eb0c0_set_dword_stages_only_for_known_component() {
        // GM_Menu::SetDword GAME VA 0x5eb0c0 (RVA 0x1eb0c0).
        let mut stack = MenuStack::create(&stack_definition());
        stack.set_dword(424242, 0xdead);
        assert_eq!(stack.get_dword(424242), 0);
        stack.set_dword(main_menu::MAINMENU_ITEM_QUIT, 0xdead_beef);
        assert_eq!(stack.get_dword(main_menu::MAINMENU_ITEM_QUIT), 0xdead_beef);
        stack.set_dword(main_menu::MAINMENU_ITEM_QUIT, u32::MAX);
        assert_eq!(stack.get_dword(main_menu::MAINMENU_ITEM_QUIT), u32::MAX);
    }

    #[test]
    fn rva_1eb0e0_get_dword_defaults_to_zero() {
        // GM_Menu::GetDword GAME VA 0x5eb0e0 (RVA 0x1eb0e0): 0 where the
        // menu has no such component or nothing was staged.
        let stack = MenuStack::create(&stack_definition());
        assert_eq!(stack.get_dword(424242), 0);
        assert_eq!(stack.get_dword(main_menu::MAINMENU_ITEM_QUIT), 0);
    }

    #[test]
    fn rva_1eb100_set_bool_stages_only_for_known_component() {
        // GM_Menu::SetBool GAME VA 0x5eb100 (RVA 0x1eb100).
        let mut stack = MenuStack::create(&stack_definition());
        stack.set_bool(424242, true);
        assert!(!stack.get_bool(424242));
        stack.set_bool(main_menu::MAINMENU_ITEM_INTRO, true);
        assert!(stack.get_bool(main_menu::MAINMENU_ITEM_INTRO));
        stack.set_bool(main_menu::MAINMENU_ITEM_INTRO, false);
        assert!(!stack.get_bool(main_menu::MAINMENU_ITEM_INTRO));
    }

    #[test]
    fn rva_1eb120_get_bool_defaults_to_false() {
        // GM_Menu::GetBool GAME VA 0x5eb120 (RVA 0x1eb120): false where the
        // menu has no such component or nothing was staged.
        let stack = MenuStack::create(&stack_definition());
        assert!(!stack.get_bool(424242));
        assert!(!stack.get_bool(main_menu::MAINMENU_ITEM_INTRO));
    }

    #[test]
    fn rva_1e3cf0_set_progress_game_init_maps_past_mission_part() {
        // GM_LoadProgress::SetProgressGameInit GAME VA 0x5e3cf0 (RVA 0x1e3cf0).
        assert_eq!(LOADPROGRESS_BAR_ID, 0x12c);
        assert_eq!(load_progress_game_init_value(0.7, 0.0), 0.7);
        assert_eq!(load_progress_game_init_value(0.7, 1.0), 1.0);
        let mid = load_progress_game_init_value(0.7, 0.5);
        assert!((mid - 0.85).abs() < 1e-6, "{mid}");
        assert_eq!(load_progress_game_init_value(0.0, 0.25), 0.25);
    }

    #[test]
    fn rva_1e3d20_set_progress_mission_open_maps_up_to_mission_part() {
        // GM_LoadProgress::SetProgressMissionOpen GAME VA 0x5e3d20
        // (RVA 0x1e3d20).
        assert_eq!(load_progress_mission_open_value(0.7, 0.0), 0.0);
        assert_eq!(load_progress_mission_open_value(0.7, 1.0), 0.7);
        let mid = load_progress_mission_open_value(0.7, 0.5);
        assert!((mid - 0.35).abs() < 1e-6, "{mid}");
        assert_eq!(load_progress_mission_open_value(0.0, 1.0), 0.0);
    }

    #[test]
    fn rva_1ea690_destroy_clears_components_values_and_latches() {
        // GM_Menu::Destroy GAME VA 0x5ea690 (RVA 0x1ea690): every admitted
        // component goes with its staged values; the native `delete this`
        // half is the caller dropping the stack.
        let mut stack = MenuStack::create(&stack_definition());
        stack.set_text(main_menu::MAINMENU_ITEM_INTRO, b"intro".to_vec());
        stack.set_float(main_menu::MAINMENU_ITEM_NEW_GAME, 0.5);
        stack.set_dword(main_menu::MAINMENU_ITEM_QUIT, 3);
        stack.set_bool(main_menu::MAINMENU_ITEM_QUIT, true);
        stack.menu_close(7);
        assert!(!stack.is_empty());
        stack.destroy();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
        assert_eq!(stack.active_id(), None);
        assert!(!stack.is_closing());
        assert_eq!(stack.close_code(), 0);
        assert_eq!(stack.open_id(), 0);
        assert_eq!(stack.get_text(main_menu::MAINMENU_ITEM_INTRO), None);
        assert_eq!(stack.get_float(main_menu::MAINMENU_ITEM_NEW_GAME), 0.0);
        assert_eq!(stack.get_dword(main_menu::MAINMENU_ITEM_QUIT), 0);
        assert!(!stack.get_bool(main_menu::MAINMENU_ITEM_QUIT));
        // Destroying an empty stack is a stable rewrite.
        stack.destroy();
        assert!(stack.is_empty());
    }

    #[test]
    fn rva_1ea6d0_force_update_is_unsupported() {
        // GM_Menu::ForceUpdate GAME VA 0x5ea6d0 (RVA 0x1ea6d0): the
        // component-class fan-out is missing, so nothing runs.
        let stack = MenuStack::create(&stack_definition());
        let error = stack.force_update().unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
    }

    #[test]
    fn rva_1eb030_set_string_is_unsupported_and_stages_nothing() {
        // GM_Menu::SetString GAME VA 0x5eb030 (RVA 0x1eb030): the base
        // component virtual is an empty no-op, so unlike SetText nothing
        // lands in the staged texts.
        let stack = MenuStack::create(&stack_definition());
        let error = stack
            .set_string(main_menu::MAINMENU_ITEM_INTRO, Some(b"intro"))
            .unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
        assert_eq!(stack.get_text(main_menu::MAINMENU_ITEM_INTRO), None);
        let error = stack.set_string(424242, None).unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
    }

    #[test]
    fn rva_1eb140_set_pointer_is_unsupported() {
        // GM_Menu::SetPointer GAME VA 0x5eb140 (RVA 0x1eb140): the raw
        // pointer has no typed owner, so the address is never stored.
        let stack = MenuStack::create(&stack_definition());
        let error = stack.set_pointer(main_menu::MAINMENU_ITEM_INTRO, 0x1234).unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
        let error = stack.set_pointer(424242, 0).unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
    }

    #[test]
    fn rva_1eb160_get_range_float_leaves_outputs_untouched() {
        // GM_Menu::GetRangeFloat GAME VA 0x5eb160 (RVA 0x1eb160): the ranges
        // live in the missing component classes, so the out-words stay
        // untouched for known and missing ids alike.
        let stack = MenuStack::create(&stack_definition());
        let (mut min, mut max) = (1.0, 2.0);
        let error = stack
            .get_range_float(main_menu::MAINMENU_ITEM_INTRO, &mut min, &mut max)
            .unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
        assert_eq!((min, max), (1.0, 2.0));
        let error = stack.get_range_float(424242, &mut min, &mut max).unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
        assert_eq!((min, max), (1.0, 2.0));
    }

    #[test]
    fn rva_1eb190_set_range_dword_is_unsupported() {
        // GM_Menu::SetRangeDword GAME VA 0x5eb190 (RVA 0x1eb190): the dword
        // ranges live in the missing component classes, so nothing stages.
        let stack = MenuStack::create(&stack_definition());
        let error = stack
            .set_range_dword(main_menu::MAINMENU_ITEM_QUIT, 0, 10)
            .unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
        let error = stack.set_range_dword(424242, 0, 10).unwrap_err();
        assert_eq!(error, MenuStubError::Unsupported("component classes"));
    }
}
