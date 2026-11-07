//! Original Main.cpp racing-menu predicate (Game.exe RVA 0x1bebd0).
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000, VA 0x5bebd0. Reference sibling mafia-re @ 113b5b6
//! game/Main.cpp:262-266 (FUNCTION comment at :262, body `return false`)
//! plus declaration game/Main.h:28. The sole evidenced caller is
//! GM_MainMenu::OnBeforeCreate (mafia-re game/ui/g_m_func.cpp:271-280),
//! which selects "mainmenu" when this predicate is false and the PC-only
//! "mainmenurac" name when true; PS2 has no racing branch and always uses
//! "mainmenu". Prior probe reports the native body as 3 bytes
//! (XOR EAX,EAX; RET), not re-observed in this session. This owner executes
//! no callbacks and reads no state. It does not implement OnBeforeCreate,
//! the menu layout, or any mission selection.
//!
//! GM_MainMenu::OnBeforeCreate name-select (Game.exe VA 0x5e0670,
//! RVA 0x1e0670, 20-byte leaf VA 0x5e0670-0x5e0683 (CALL + TEST + two MOV
//! + JNZ + MOV + RET); Game.exe SHA-256 as above, base 0x400000).
//! Reference sibling mafia-re @ 113b5b6 game/ui/g_m_func.cpp:271-280
//! (GM_MainMenu::OnBeforeCreate) and game/ui/g_menu.cpp GM_Menu::Create
//! 0x5ea2e0 chain; cited only, never vendored. REA-observed read-only probe
//! this session: CALL 0x5bebd0 RacingInMainMenu, TEST AL,AL, then EAX takes
//! 0x6381d0 for the fall-through arm and 0x6381dc for the JNZ-taken arm, RET
//! at 0x5e0683; the "mainmenu" and "mainmenurac" strings are verified and
//! only one vtable/data reference (0x625eb8) anchors this leaf. REA
//! ev_1b813b8d carries the first 13 of the 20 body bytes (partial read),
//! ev_752737b8 carries the marker name (a LAB_ label, NOT a Ghidra function
//! entry, disclosed here), ev_1033bebe carries the cross references, and
//! ev_7056efa9 carries the strings. `analyze_function` was REJECTED for this
//! LAB_ leaf (no procedure), so this is marker plus binary-read evidence,
//! NOT a Ghidra body: tier contract holds and differential is unwarranted.
//! The typed select below owns exactly this branch and consumes the existing
//! racing_in_main_menu() predicate; it performs no menu construction.
//!
//! DEFERRED with explicit markers (no credit, untouched here):
//! the 0x5e0670-0x5e0683 tail (JNZ operand, second MOV, RET) belongs to the
//! OnBeforeCreate leaf; the distinct 12-byte code run at VA 0x5e0684-0x5e068f
//! is present-but-uncredited code needing an owner (not padding);
//! SetCameraTargetID frame positioning past the ID latch (0x5ebe80) and the
//! TickCamera approach (0x5ec8a0) need the mission scene owner; the GM_Menu
//! device half (sounds, parent callbacks, igraph/mission pump, Draw/TickCamera
//! device paths, zz_globalReturnCode global) and GameLoopSingle dispatch stay
//! deferred. The pure OnClick/OnChange/OnCreate/Tick-routing contracts below
//! are marker-only dispatch owners: no overlay function credit is claimed.
//!
//! OnCreate ownership (Game.exe SHA-256 as above, base 0x400000; reference
//! sibling mafia-re @ 113b5b6 game/ui/g_m_func.cpp:286-294
//! GM_MainMenu::OnCreate 0x5e0690, cited only, never vendored; marker-only,
//! no REA body observed in this session): the three small global owners the
//! body needs exist below as pure state with authored tests (no cargo
//! execution is claimed): (1) MenuCameraTarget, the lastKameraTargetID latch
//! (g_menu.h:323) written by SetCameraTargetID 0x5ebe80, (2)
//! MainMenuFreeride, the freeride_enabled word (g_loadsave.h, +0x1f0) plus
//! the FreerideStoreA 0x60e920 triple, reusing the checked loadsave/settings
//! owners (game_loadsave_controls.rs, game_loadsave_lifecycle.rs,
//! mission_load_session.rs), none of which owns freeride state, and (3)
//! MenuComponentLatches, the per-ID COMPF words with real ENABLED bit sense
//! plus the active-selection latch, consumed by the menu host's list owner
//! (engine menu.rs MenuStack: FindComponentByID 0x5eb2f0, ActivateByIDOrNext
//! 0x5eb210 fallback search). main_menu_oncreate_dispatch wires the ordered
//! transition with those real contracts (main_menu_oncreate_prefix is its
//! established alias); the scene/component-device half stays DEFERRED as
//! above, so RVA 0x1e0690 takes no overlay entry: owners-only helpers receive
//! no function credit.
//!
//! OnClick ownership (Game.exe VA 0x5e06e0, RVA 0x1e06e0; sibling
//! game/ui/g_m_func.cpp:309-354, cited only, never vendored; marker-only):
//! main_menu_onclick is the full 13-branch pure dispatch over the latch model
//! (MenuCloseAll with E_MAINMENU_RESULT vs MenuOpen of loadgame/freeride/
//! optionsb/exityesno); OnChange ownership (VA 0x5e4a50; g_m_func.cpp:299-304):
//! main_menu_onchange routes active->id into SetCameraTargetID. Both are pure
//! id routing; component lookup, stack allocation and device effects stay with
//! the menu host / DEFERRED owners.
//!
//! The tests in this module are hand-derived vectors authored without cargo
//! (no execution is claimed).

#![forbid(unsafe_code)]

/// Original RacingInMainMenu (FUNCTION GAME 0x5bebd0).
/// Always false in this binary, so the menu keeps the "mainmenu" mission.
pub fn racing_in_main_menu() -> bool {
    false
}

/// Typed original mission-name selection for GM_MainMenu::OnBeforeCreate
/// (Game.exe VA 0x5e0670, RVA 0x1e0670).
///
/// Native effect order is a single predicate read followed by a pointer
/// select: when the RacingInMainMenu arm is false the leaf returns the
/// 0x6381d0-side name ("mainmenu"), else the 0x6381dc-side name
/// ("mainmenurac"). SCOPE: this branch only; menu construction, OnCreate
/// (RVA 0x1e0690), OnChange/OnClick, the GM_Menu::Create 0x5ea2e0 stack, and
/// GameLoopSingle are DEFERRED (see module docs) and get no behavior here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuMission {
    Mainmenu,
    MainmenuRac,
}

impl MainMenuMission {
    /// Original mission-table name bytes selected by the native leaf.
    pub fn mission_name(self) -> &'static str {
        match self {
            Self::Mainmenu => "mainmenu",
            Self::MainmenuRac => "mainmenurac",
        }
    }

    /// Pure branch of the native TEST AL,AL select.
    pub fn from_racing(racing: bool) -> Self {
        if racing {
            Self::MainmenuRac
        } else {
            Self::Mainmenu
        }
    }
}

/// Pure branch of the native leaf for an explicit predicate value.
pub fn select_main_menu_mission(racing: bool) -> MainMenuMission {
    MainMenuMission::from_racing(racing)
}

/// Live name-select consuming the existing racing_in_main_menu() predicate.
/// Reads the predicate when called; the return carries no menu state.
pub fn main_menu_mission() -> MainMenuMission {
    select_main_menu_mission(racing_in_main_menu())
}

/// Live mission-name bytes consuming the existing predicate.
pub fn main_menu_mission_name() -> &'static str {
    main_menu_mission().mission_name()
}

/// SetCameraTargetID ids besides the menu items' own ids (g_menu.h:37-40;
/// the MENUCAM_* spellings and the "camera_start" frame binding are upstream
/// working names, disclosed here, not original symbols).
/// MENUCAM_NONE (0) keeps the scene camera as it is; MENUCAM_START
/// (((uint)-1)) puts it at once at "camera_start" (or where the camera is),
/// looking 10 units ahead.
pub const MENU_CAMERA_NONE: u32 = 0;
pub const MENU_CAMERA_START: u32 = u32::MAX;

/// Pure latch for GM_Menu::lastKameraTargetID (g_menu.h:323), the id
/// GM_Menu::SetCameraTargetID (GAME 0x5ebe80) was given. No camera-target ID
/// owner exists in the tree (lab_state.rs carries only a diagnostic
/// [f32; 3] camera_target, frame_camera.rs and game_camera_reset.rs own
/// profile/reset state, never the menu latch), so this is a genuine gap.
/// SCOPE: the ID word only; frame positioning (FindFrame "camera_start",
/// GetWorldPos/GetWorldDir, tc_* stores, SetPos/SetDir/Update) needs the
/// mission scene owner and stays DEFERRED. Deferred two-step contract (native
/// SetCameraTargetID g_menu.cpp:1385-1425): the prefix retains START as the
/// unconsumed request; the future scene/tick owner must consume START,
/// position from "camera_start", then clear to MENU_CAMERA_NONE, reproducing
/// the native synchronous clear-to-0 (the MENUCAM_START arm ends with
/// lastKameraTargetID = 0 before break).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuCameraTarget {
    target_id: u32,
}

impl MenuCameraTarget {
    /// Initial ID is MENUCAM_NONE (0), matching the latch's cleared state
    /// once the camera stands (g_menu.h:323).
    pub fn new() -> Self {
        Self {
            target_id: MENU_CAMERA_NONE,
        }
    }

    /// Records the SetCameraTargetID argument; performs no scene access.
    pub fn set_camera_target_id(&mut self, id: u32) {
        self.target_id = id;
    }

    /// Live read of the latched ID, as the menu camera tick consumes it.
    pub fn camera_target_id(&self) -> u32 {
        self.target_id
    }
}

/// G_LoadSaveClass::freeride_enabled levels (g_loadsave.h; the FREERIDE_*
/// spellings are upstream working names, disclosed here): how much of the
/// free ride is open. Checked owners (game_loadsave_controls.rs,
/// game_loadsave_lifecycle.rs ProfileState, mission_load_session.rs) own no
/// freeride word, so this is a genuine gap.
pub const FREERIDE_SMALL_CITY: u32 = 0;
pub const FREERIDE_CITY: u32 = 1;
pub const FREERIDE_COUNTRYSIDE: u32 = 2;
pub const FREERIDE_EXTREME: u32 = 3;

/// Free-ride choices written by G_LoadSaveClass::FreerideStoreA (GAME
/// 0x60e920): Tommy's car descriptor, flags, and the free-ride menu
/// densities. The sibling S_freeride shape (g_loadsave.h) also carries
/// location and active, which StoreA never writes; they are not modeled
/// here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FreerideChoices {
    pub car: u32,
    pub flags: u32,
    pub traffic_density: f32,
    pub pedestrian_density: f32,
    pub police_density: f32,
}

/// Minimal freeride owner for the main-menu reset: the enabled word plus the
/// StoreA triple. This does not mirror the C++ class layout; other profile
/// bytes stay with the eventual profile file owner. Cross-reference: the
/// enabled word is the same +0x1f0 word as ProfileState.unknown_1f0
/// (game_loadsave_lifecycle.rs placeholder, never read as levels); the two
/// u32s must be unified under one authoritative owner by the profile-file
/// owner in the future.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MainMenuFreeride {
    pub enabled: u32,
    pub choices: FreerideChoices,
}

impl MainMenuFreeride {
    /// G_LoadSaveClass::FreerideStoreA (GAME 0x60e920) pure stores: car,
    /// flags, traffic, pedestrian and police densities land exactly; the
    /// enabled word is untouched.
    pub fn store_a(&mut self, car: u32, flags: u32, traffic: f32, pedestrians: f32, police: f32) {
        self.choices = FreerideChoices {
            car,
            flags,
            traffic_density: traffic,
            pedestrian_density: pedestrians,
            police_density: police,
        };
    }

    /// OnCreate gate (g_m_func.cpp:289): while the free ride is at most the
    /// countryside level, Free Ride Extreme is gated (SetDisabled path).
    pub fn extreme_gated(&self) -> bool {
        self.enabled <= FREERIDE_COUNTRYSIDE
    }
}

/// GM_Component flag bits for the deferred dispatch below (mafia-re
/// game/ui/g_m_comps.h:19-20,30-33; names ours per upstream): COMPF_VISIBLE
/// 0x10 is GM_Menu::SetHidden's bit, COMPF_ENABLED 0x20 is GM_Menu::SetDisabled's
/// bit, COMPF_MOUSE_CAPTURE 0x4000 keeps clicks on the active component,
/// COMPF_ACTIVATABLE 0x8000 gates activation, COMPF_ACTIVE 0x80000000 marks the
/// menu's active component. Reference bits only — the list-owning application
/// lives in the menu host; the latch here records requests with real bit sense.
pub const COMPONENT_VISIBLE: u32 = 0x10;
pub const COMPONENT_ENABLED: u32 = 0x20;
pub const COMPONENT_MOUSE_CAPTURE: u32 = 0x4000;
pub const COMPONENT_ACTIVATABLE: u32 = 0x8000;
pub const COMPONENT_ACTIVE: u32 = 0x8000_0000;

/// Default COMPF word for a freshly admitted component (g_m_comps.h:19-21,31):
/// visible, enabled and activatable. Definition style bits stay opaque.
pub const DEFAULT_COMPONENT_FLAGS: u32 =
    COMPONENT_VISIBLE | COMPONENT_ENABLED | COMPONENT_ACTIVATABLE;

/// Main-menu item ids from tables/menu/mainmenu.mnu, decimal
/// (mafia-re game/ui/g_m_func.cpp:28-41; symbolic names follow upstream,
/// disclosed here). Consistent with the menu host, which maps 990101 to Intro.
pub const MAINMENU_ITEM_INTRO: u32 = 990101;
pub const MAINMENU_ITEM_TUTORIAL: u32 = 990102;
pub const MAINMENU_ITEM_NEW_GAME: u32 = 990103;
pub const MAINMENU_ITEM_LOAD_GAME: u32 = 990104;
pub const MAINMENU_ITEM_FREE_RIDE: u32 = 990105;
pub const MAINMENU_ITEM_FREE_RIDE_EXTREME: u32 = 990106;
pub const MAINMENU_ITEM_RACING: u32 = 990107;
pub const MAINMENU_ITEM_MULTIPLAYER: u32 = 990108;
pub const MAINMENU_ITEM_CARCYCLOPEDIA: u32 = 990109;
pub const MAINMENU_ITEM_OPTIONS: u32 = 990110;
pub const MAINMENU_ITEM_CREDITS: u32 = 990111;
pub const MAINMENU_ITEM_CHANGE_PROFILE: u32 = 990112;
pub const MAINMENU_ITEM_QUIT: u32 = 990113;

/// E_MAINMENU_RESULT choices returned via MenuCloseAll by the main-menu OnClick
/// (mafia-re game/ui/g_m_func.h:5-23; names and values per upstream, disclosed
/// here). Only the codes the OnClick switch emits are listed.
pub const MAINMENU_TUTORIAL: u32 = 0x11;
pub const MAINMENU_NEW_GAME: u32 = 0x12;
pub const MAINMENU_MULTIPLAYER: u32 = 0x14;
pub const MAINMENU_RACING: u32 = 0x15;
pub const MAINMENU_INTRO: u32 = 0x16;
pub const MAINMENU_CREDITS: u32 = 0x17;
pub const MAINMENU_QUIT: u32 = 0x18;
pub const MAINMENU_CARCYCLOPEDIA: u32 = 0x19;
pub const MAINMENU_PROFILE_CHANGE: u32 = 0x1a;
pub const MAINMENU_EXTREME: u32 = 0x1b;

/// Cancel code the native switch passes as MenuClose(All)'s code: `(uint)-1`
/// (mafia-re game/ui/g_m_func.cpp:309-354).
pub const MENU_CLOSE_CANCEL: u32 = u32::MAX;

/// Menus the main-menu OnClick opens over itself (mafia-re
/// game/ui/g_m_func.cpp:142-245,324-342): Load Game opens "loadgame", Free
/// Ride "freeride", Options "optionsb", Quit the "exityesno" question. The
/// allocation plus parent linkage (GM_Menu::MenuOpen 0x5ead50) needs the device
/// menu allocator and the activeMenuWindow global, so only the definition name
/// contract is owned here; the open itself stays DEFERRED.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuOpenTarget {
    LoadGame,
    FreeRide,
    Options,
    ExitQuestion,
}

impl MainMenuOpenTarget {
    /// Original tables/menu/<name>.mnu stem selected by the native branch
    /// (mafia-re game/ui/g_m_func.cpp:371-389).
    pub fn definition_name(self) -> &'static str {
        match self {
            Self::LoadGame => "loadgame",
            Self::FreeRide => "freeride",
            Self::Options => "optionsb",
            Self::ExitQuestion => "exityesno",
        }
    }
}

/// Pure GM_MainMenu::OnClick effect: either MenuCloseAll with an
/// E_MAINMENU_RESULT, or MenuOpen of one of the four menus above. Native
/// returns 0 on every path (mafia-re game/ui/g_m_func.cpp:309-354); unknown
/// ids are that no-op path and yield no effect here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuClickEffect {
    CloseAll { result: u32 },
    Open { target: MainMenuOpenTarget },
}

impl MainMenuClickEffect {
    /// The E_MAINMENU_RESULT for CloseAll effects, if any.
    pub fn close_all_result(self) -> Option<u32> {
        match self {
            Self::CloseAll { result } => Some(result),
            Self::Open { .. } => None,
        }
    }

    /// The menu to open for Open effects, if any.
    pub fn open_target(self) -> Option<MainMenuOpenTarget> {
        match self {
            Self::Open { target } => Some(target),
            Self::CloseAll { .. } => None,
        }
    }
}

/// GM_MainMenu::OnClick full 13-branch dispatch (Game.exe VA 0x5e06e0, RVA
/// 0x1e06e0; sibling game/ui/g_m_func.cpp:309-354, cited only). Pure function
/// of the clicked id over the latch model: no component lookup, no menu stack,
/// no device access. Branch order follows the original switch.
pub fn main_menu_onclick(id: u32) -> Option<MainMenuClickEffect> {
    match id {
        MAINMENU_ITEM_INTRO => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_INTRO,
        }),
        MAINMENU_ITEM_TUTORIAL => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_TUTORIAL,
        }),
        MAINMENU_ITEM_NEW_GAME => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_NEW_GAME,
        }),
        MAINMENU_ITEM_LOAD_GAME => Some(MainMenuClickEffect::Open {
            target: MainMenuOpenTarget::LoadGame,
        }),
        MAINMENU_ITEM_FREE_RIDE => Some(MainMenuClickEffect::Open {
            target: MainMenuOpenTarget::FreeRide,
        }),
        MAINMENU_ITEM_FREE_RIDE_EXTREME => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_EXTREME,
        }),
        MAINMENU_ITEM_RACING => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_RACING,
        }),
        MAINMENU_ITEM_MULTIPLAYER => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_MULTIPLAYER,
        }),
        MAINMENU_ITEM_CARCYCLOPEDIA => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_CARCYCLOPEDIA,
        }),
        MAINMENU_ITEM_OPTIONS => Some(MainMenuClickEffect::Open {
            target: MainMenuOpenTarget::Options,
        }),
        MAINMENU_ITEM_CREDITS => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_CREDITS,
        }),
        MAINMENU_ITEM_CHANGE_PROFILE => Some(MainMenuClickEffect::CloseAll {
            result: MAINMENU_PROFILE_CHANGE,
        }),
        MAINMENU_ITEM_QUIT => Some(MainMenuClickEffect::Open {
            target: MainMenuOpenTarget::ExitQuestion,
        }),
        _ => None,
    }
}

/// GM_MainMenu::OnChange pure routing (Game.exe VA 0x5e4a50, RVA 0x1e4a50;
/// sibling game/ui/g_m_func.cpp:299-304, cited only):
/// `if (active) SetCameraTargetID(active->id)`. Returns the id the menu-camera
/// latch must take, or `None` when no component became active. Frame lookup and
/// TickCamera approach (GM_Menu::SetCameraTargetID 0x5ebe80 past the id latch,
/// TickCamera 0x5ec8a0) need the mission scene owner and stay DEFERRED.
pub fn main_menu_onchange(active_id: Option<u32>) -> Option<u32> {
    active_id
}

/// GM_Menu::Tick pure click routing into the main-menu OnClick (Game.exe RVA
/// 0x1ea8c0; sibling game/ui/g_menu.cpp:735-853, cited only): a resolved
/// non-zero clicked id runs the OnClick dispatch above and returns its effect.
/// Key/mouse decoding into the clicked id (DIK_* handling, FindComponentAt
/// hover, component Key/Mouse virtuals), the ACTIVE-bit swap plus OnChange
/// call, cursor motion, sounds, closing-destroy and TickCamera all need device
/// or component-class owners and stay DEFERRED.
pub fn main_menu_tick_click(clicked_id: u32) -> Option<MainMenuClickEffect> {
    if clicked_id == 0 {
        None
    } else {
        main_menu_onclick(clicked_id)
    }
}

/// ExecuteMenu loop guard (Game.exe RVA 0x1eba40; sibling
/// game/ui/g_menu.cpp:1281-1379, cited only): the original runs the menu while
/// `zz_globalReturnCode` is 0. The pump itself (igraph wins/messages, mission
/// tick vs game camera tick, blink timing, key repeat, mouse buttons,
/// TickCamera, scene render/present, Destroy, HUD flag stack) needs device and
/// mission owners and stays DEFERRED.
pub fn execute_menu_continues(return_code: u32) -> bool {
    return_code == 0
}

/// Minimal per-ID COMPF words plus the active-selection latch. The menu host
/// owns no per-ID flags (its enabled() hardcodes 990103|990113) and no
/// active-selection latch (only a row index), so this is a genuine gap.
/// Absent ids read the neutral element: the default word
/// (VISIBLE|ENABLED|ACTIVATABLE), i.e. enabled; definition-flag defaults stay
/// with the future component-class owner (DEFERRED). SCOPE: request words plus
/// the active latch; the list-owning lookups (FindComponentByID 0x5eb2f0,
/// including its missing-component no-op) and the ActivateByIDOrNext 0x5eb210
/// enabled/visible/activatable checks plus the FindComponentByIDNext fallback
/// run in the menu host's list owner, which reads these words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuComponentLatches {
    flags: std::collections::BTreeMap<u32, u32>,
    active: Option<u32>,
}

impl MenuComponentLatches {
    /// GM_Menu::SetDisabled (GAME 0x5eaed0; sibling game/ui/g_menu.cpp:903-914,
    /// cited only) request record with real bit sense: true sets the
    /// COMPF_ENABLED bit, false clears it, so OnCreate's
    /// SetDisabled(990106, false) disables Free Ride Extreme. Missing ids are
    /// admitted with the default word first, matching the no-op-then-set order
    /// of the original's FindComponentByID gate for a present component.
    pub fn set_disabled(&mut self, id: u32, enabled: bool) {
        let flags = self.flags.entry(id).or_insert(DEFAULT_COMPONENT_FLAGS);
        if enabled {
            *flags |= COMPONENT_ENABLED;
        } else {
            *flags &= !COMPONENT_ENABLED;
        }
    }

    /// Live read of the latched ENABLED sense for one id.
    pub fn is_enabled(&self, id: u32) -> bool {
        self.flags
            .get(&id)
            .copied()
            .unwrap_or(DEFAULT_COMPONENT_FLAGS)
            & COMPONENT_ENABLED
            != 0
    }

    /// Live read of the full latched COMPF word for one id (default word for
    /// ids with no request yet). The ACTIVE bit is owned by the active latch
    /// below, not stored here; readers OR it in from `active_id`.
    pub fn component_flags(&self, id: u32) -> u32 {
        self.flags
            .get(&id)
            .copied()
            .unwrap_or(DEFAULT_COMPONENT_FLAGS)
    }

    /// GM_Menu::ActivateByIDOrNext (GAME 0x5eb210; sibling
    /// game/ui/g_menu.cpp:1062-1079, cited only) request latch: records the
    /// requested id. The ENABLED|VISIBLE|ACTIVATABLE gate and the
    /// FindComponentByIDNext fallback need the component list owner and stay
    /// DEFERRED here; the host list owner consumes `active_id` for them.
    pub fn activate_by_id_or_next(&mut self, id: u32) {
        self.active = Some(id);
    }

    /// Live read of the latched active selection, if any.
    pub fn active_id(&self) -> Option<u32> {
        self.active
    }
}

/// GM_MainMenu::OnCreate real dispatch (Game.exe VA 0x5e0690, RVA 0x1e0690;
/// sibling game/ui/g_m_func.cpp:286-294, cited only): the ordered transition
/// over the three owned globals, now driving the real latch contracts instead
/// of a pure prefix. Native order: latch MENUCAM_START via 0x5ebe80; when the
/// freeride_enabled word (sibling mafiaLoadSave.freeride_enabled; absolute
/// address unverified in this marker-only session) reads <=
/// FREERIDE_COUNTRYSIDE, record SetDisabled(FREE_RIDE_EXTREME, false) with real
/// COMPF_ENABLED bit sense via 0x5eaed0; FreerideStoreA triple (1, 0, 1.0,
/// 1.0, 1.0) via 0x60e920; latch ActivateByIDOrNext(INTRO 990101) via
/// 0x5eb210; return 0. The dispatch's list-owning half (real COMPF words on
/// components, the next-component fallback for the INTRO activation,
/// SetCameraTargetID frame positioning past the ID latch, menu-stack effects)
/// runs in the menu host's component owner, which consumes these latches;
/// scene positioning stays DEFERRED for the missing scene owner. Takes no
/// overlay function entry: owners-only helpers receive no function credit.
pub fn main_menu_oncreate_dispatch(
    camera: &mut MenuCameraTarget,
    freeride: &mut MainMenuFreeride,
    components: &mut MenuComponentLatches,
) -> i32 {
    camera.set_camera_target_id(MENU_CAMERA_START);
    if freeride.extreme_gated() {
        components.set_disabled(MAINMENU_ITEM_FREE_RIDE_EXTREME, false);
    }
    freeride.store_a(1, 0, 1.0, 1.0, 1.0);
    components.activate_by_id_or_next(MAINMENU_ITEM_INTRO);
    0
}

/// Ordered latch transition kept under its established name; a thin alias of
/// the real dispatch above (same order, same latch contracts).
pub fn main_menu_oncreate_prefix(
    camera: &mut MenuCameraTarget,
    freeride: &mut MainMenuFreeride,
    components: &mut MenuComponentLatches,
) -> i32 {
    main_menu_oncreate_dispatch(camera, freeride, components)
}

/// Quit-question item ids from tables/menu/exityesno.mnu (mafia-re
/// game/ui/g_m_func.cpp:133-136; upstream working names, disclosed here).
pub const EXITYESNO_WINDOW: u32 = 0x82;
pub const EXITYESNO_YES: u32 = 0x8a;
pub const EXITYESNO_NO: u32 = 0x8b;

/// Text-database id of "Quit Game?" (mafia-re game/ui/g_m_func.cpp:139;
/// upstream working name, disclosed here).
pub const TEXT_QUIT_GAME: u32 = 0x77;

/// Pure GM_MainMenuExit::OnClick effect (Game.exe VA 0x5e0640, RVA 0x1e0640;
/// sibling game/ui/g_m_func.cpp:248-261, cited only): No closes the question
/// with the cancel code, Yes closes every menu with MAINMENU_QUIT. Native
/// returns 0 on every path; unknown ids are that no-op path and yield no
/// effect here. The parent OnCloseChild call plus the zz_globalReturnCode
/// global stay DEFERRED, as with the other menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenuExitClickEffect {
    Close,
    CloseAll { result: u32 },
}

/// GM_MainMenuExit::OnClick two-branch dispatch (Game.exe VA 0x5e0640, RVA
/// 0x1e0640; sibling game/ui/g_m_func.cpp:248-261, cited only).
pub fn main_menu_exit_onclick(id: u32) -> Option<MainMenuExitClickEffect> {
    match id {
        EXITYESNO_NO => Some(MainMenuExitClickEffect::Close),
        EXITYESNO_YES => Some(MainMenuExitClickEffect::CloseAll {
            result: MAINMENU_QUIT,
        }),
        _ => None,
    }
}

/// GM_MainMenuExit::OnBeforeCreate leaf (Game.exe VA 0x5e08e0, RVA 0x1e08e0;
/// sibling game/ui/g_m_func.cpp:356-361, cited only).
pub fn main_menu_exit_definition() -> &'static str {
    "exityesno"
}

/// GM_Options::OnBeforeCreate leaf (Game.exe VA 0x5e0900, RVA 0x1e0900;
/// sibling game/ui/g_m_func.cpp:370-375, cited only).
pub fn options_definition() -> &'static str {
    "optionsb"
}

/// GM_FreeRide::OnBeforeCreate leaf (Game.exe VA 0x5e0910, RVA 0x1e0910;
/// sibling game/ui/g_m_func.cpp:377-382, cited only): an inline literal in
/// the reference, kept here as the definition-name contract.
pub fn freeride_definition() -> &'static str {
    "freeride"
}

/// GM_LoadGame::OnBeforeCreate leaf (Game.exe VA 0x5e0920, RVA 0x1e0920;
/// sibling game/ui/g_m_func.cpp:384-389, cited only).
pub fn load_game_definition() -> &'static str {
    "loadgame"
}

/// Options item ids from tables/menu/optionsb.mnu (mafia-re
/// game/ui/g_m_func.cpp:62-65; upstream working names, disclosed here).
pub const OPTIONSB_CONTROLS: u32 = 0x64;
pub const OPTIONSB_AUDIO_VIDEO: u32 = 0x65;
pub const OPTIONSB_BACK: u32 = 0x190;

/// Menus the options menu opens over itself (mafia-re
/// game/ui/g_m_func.cpp:394-410, cited only): Controls opens its page menu,
/// Audio & Video opens "optionsa". Allocation plus parent linkage stay
/// DEFERRED, as with the main-menu open targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsOpenTarget {
    Controls,
    AudioVideo,
}

/// Pure GM_Options::OnClick effect (Game.exe VA 0x5e0ad0, RVA 0x1e0ad0;
/// sibling game/ui/g_m_func.cpp:394-410, cited only): Controls and Audio &
/// Video open their menus, Back closes the options with the cancel code.
/// Native returns 0 on every path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsClickEffect {
    Open { target: OptionsOpenTarget },
    Close,
}

/// GM_Options::OnClick three-branch dispatch (Game.exe VA 0x5e0ad0, RVA
/// 0x1e0ad0; sibling game/ui/g_m_func.cpp:394-410, cited only). Unknown ids
/// are the native no-op path and yield no effect here.
pub fn options_onclick(id: u32) -> Option<OptionsClickEffect> {
    match id {
        OPTIONSB_CONTROLS => Some(OptionsClickEffect::Open {
            target: OptionsOpenTarget::Controls,
        }),
        OPTIONSB_AUDIO_VIDEO => Some(OptionsClickEffect::Open {
            target: OptionsOpenTarget::AudioVideo,
        }),
        OPTIONSB_BACK => Some(OptionsClickEffect::Close),
        _ => None,
    }
}

/// GM_OptionsAudioVideo::OnBeforeCreate leaf (Game.exe VA 0x5e0bb0, RVA
/// 0x1e0bb0; sibling game/ui/g_m_func.cpp:412-417, cited only).
pub fn options_audio_video_definition() -> &'static str {
    "optionsa"
}

/// Options-controls pages, one menu each (mafia-re game/ui/g_m_func.cpp:94-96;
/// upstream working names, disclosed here).
pub const OPTIONSC_PAGE_PLAYER: u32 = 1;
pub const OPTIONSC_PAGE_CAR: u32 = 2;
pub const OPTIONSC_PAGE_OTHER: u32 = 3;

/// GM_OptionsControls::OnBeforeCreate page dispatch (Game.exe VA 0x5e1480,
/// RVA 0x1e1480; sibling game/ui/g_m_func.cpp:578-592, cited only): the
/// player page opens "optionsc1", the car page "optionsc2", every other page
/// value "optionsc3" (the native default arm).
pub fn options_controls_definition(page: u32) -> &'static str {
    match page {
        OPTIONSC_PAGE_PLAYER => "optionsc1",
        OPTIONSC_PAGE_CAR => "optionsc2",
        _ => "optionsc3",
    }
}

/// Game-failed item ids from tables/menu/gamefailed.mnu (mafia-re
/// game/ui/g_m_func.cpp:43-47; upstream working names, disclosed here).
pub const GAMEFAILED_RETRY_ITEM: u32 = 0x3e8;
pub const GAMEFAILED_EXIT_ITEM: u32 = 0x3ea;

/// ExecuteMenu results the failure menu closes with (mafia-re
/// game/ui/g_m_func.h:96-101; upstream working names, disclosed here).
pub const GAMEFAILED_RESULT_EXIT: u32 = 0x4fb;
pub const GAMEFAILED_RESULT_RETRY: u32 = 0x4fd;

/// GM_GameFailed::OnClick dispatch (Game.exe VA 0x5e2950, RVA 0x1e2950;
/// sibling game/ui/g_m_func.cpp:893-906, cited only): the pure result the
/// native MenuCloseAll with the cancel code carries. Unknown ids are the
/// native no-op path and yield None. The parent chain plus the
/// zz_globalReturnCode global stay DEFERRED.
pub fn game_failed_onclick(id: u32) -> Option<u32> {
    match id {
        GAMEFAILED_EXIT_ITEM => Some(GAMEFAILED_RESULT_EXIT),
        GAMEFAILED_RETRY_ITEM => Some(GAMEFAILED_RESULT_RETRY),
        _ => None,
    }
}

/// Question-box item ids from tables/menu/questionbox.mnu (mafia-re
/// game/ui/g_m_func.h:62-66; upstream working names, disclosed here).
pub const QUESTIONBOX_WINDOW: u32 = 0x82;
pub const QUESTIONBOX_TEXT: u32 = 0x83;
pub const QUESTIONBOX_YES: u32 = 0x8a;
pub const QUESTIONBOX_NO: u32 = 0x8b;

/// Codes the question box closes with, and the ExecuteMenu results for a box
/// run without a parent menu (mafia-re game/ui/g_m_func.h:68-75; upstream
/// working names, disclosed here). Escape closes with No's code/result.
pub const QUESTIONBOX_CODE_NO: u32 = 0;
pub const QUESTIONBOX_CODE_YES: u32 = 1;
pub const QUESTIONBOX_RESULT_YES: u32 = 0xb;
pub const QUESTIONBOX_RESULT_NO: u32 = 0xc;

/// Escape key value the question-box OnKey compares (DIK_ESCAPE is 1;
/// mafia-re game/ui/g_menu.h:165, cited only).
pub const QUESTION_BOX_ESCAPE_KEY: u8 = 1;

/// GM_QuestionBox::OnBeforeCreate leaf (Game.exe VA 0x5e2340, RVA 0x1e2340;
/// sibling game/ui/g_m_func.cpp:790-795, cited only).
pub fn question_box_definition() -> &'static str {
    "questionbox"
}

/// Pure question-box dismissal: MenuClose with a code where a parent menu
/// opened the box, else MenuCloseAll with a code plus an ExecuteMenu result.
/// The parent OnCloseChild call plus the zz_globalReturnCode global stay
/// DEFERRED, as with the other menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionBoxClose {
    Dismiss { code: u32 },
    DismissAll { code: u32, result: u32 },
}

/// GM_QuestionBox::OnKey dispatch (Game.exe VA 0x5e2380, RVA 0x1e2380;
/// sibling game/ui/g_m_func.cpp:807-818, cited only): non-Escape keys are
/// the native no-op path (None, native still returns 0); Escape dismisses
/// the box with No's code, or every menu with No's result where no parent
/// menu opened it.
pub fn question_box_onkey(key: u8, open_id: u32) -> Option<QuestionBoxClose> {
    if key != QUESTION_BOX_ESCAPE_KEY {
        return None;
    }
    if open_id != 0 {
        Some(QuestionBoxClose::Dismiss {
            code: QUESTIONBOX_CODE_NO,
        })
    } else {
        Some(QuestionBoxClose::DismissAll {
            code: QUESTIONBOX_CODE_NO,
            result: QUESTIONBOX_RESULT_NO,
        })
    }
}

/// GM_QuestionBox::OnClick dispatch (Game.exe VA 0x5e23b0, RVA 0x1e23b0;
/// sibling game/ui/g_m_func.cpp:820-839, cited only): Yes and No each dismiss
/// the box where a parent menu opened it, else dismiss every menu with the
/// matching result. Unknown ids are the native no-op path (None).
pub fn question_box_onclick(id: u32, open_id: u32) -> Option<QuestionBoxClose> {
    let (code, result) = match id {
        QUESTIONBOX_YES => (QUESTIONBOX_CODE_YES, QUESTIONBOX_RESULT_YES),
        QUESTIONBOX_NO => (QUESTIONBOX_CODE_NO, QUESTIONBOX_RESULT_NO),
        _ => return None,
    };
    if open_id != 0 {
        Some(QuestionBoxClose::Dismiss { code })
    } else {
        Some(QuestionBoxClose::DismissAll { code, result })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn racing_in_main_menu_reports_no_racing_variant_across_calls() {
        let observed = [
            racing_in_main_menu(),
            racing_in_main_menu(),
            racing_in_main_menu(),
        ];
        assert_eq!(observed, [false, false, false]);
        assert!(!racing_in_main_menu());
    }
    #[test]
    fn false_predicate_selects_mainmenu_name() {
        assert_eq!(
            select_main_menu_mission(false),
            MainMenuMission::Mainmenu
        );
        assert_eq!(select_main_menu_mission(false).mission_name(), "mainmenu");
        assert_eq!(MainMenuMission::from_racing(false).mission_name(), "mainmenu");
    }
    #[test]
    fn true_predicate_selects_mainmenurac_name() {
        assert_eq!(
            select_main_menu_mission(true),
            MainMenuMission::MainmenuRac
        );
        assert_eq!(
            select_main_menu_mission(true).mission_name(),
            "mainmenurac"
        );
        assert_eq!(
            MainMenuMission::from_racing(true).mission_name(),
            "mainmenurac"
        );
    }
    #[test]
    fn selection_is_deterministic_and_consumes_the_live_predicate() {
        let first = main_menu_mission();
        let second = main_menu_mission();
        assert_eq!(first, second);
        assert_eq!(first, select_main_menu_mission(racing_in_main_menu()));
        assert_eq!(main_menu_mission_name(), first.mission_name());
        assert_eq!(main_menu_mission(), MainMenuMission::Mainmenu);
        assert_eq!(main_menu_mission_name(), "mainmenu");
    }
    #[test]
    fn camera_target_latch_starts_cleared_and_records_start() {
        let mut camera = MenuCameraTarget::new();
        assert_eq!(camera.camera_target_id(), MENU_CAMERA_NONE);
        assert_eq!(MENU_CAMERA_START, u32::MAX);
        camera.set_camera_target_id(MENU_CAMERA_START);
        assert_eq!(camera.camera_target_id(), MENU_CAMERA_START);
        camera.set_camera_target_id(MAINMENU_ITEM_INTRO);
        assert_eq!(camera.camera_target_id(), MAINMENU_ITEM_INTRO);
    }
    #[test]
    fn freeride_store_a_lands_exact_words_and_leaves_enabled_untouched() {
        let mut freeride = MainMenuFreeride::default();
        assert_eq!(freeride.enabled, FREERIDE_SMALL_CITY);
        freeride.enabled = FREERIDE_CITY;
        freeride.store_a(1, 0, 1.0, 1.0, 1.0);
        assert_eq!(freeride.enabled, FREERIDE_CITY);
        assert_eq!(freeride.choices.car, 1);
        assert_eq!(freeride.choices.flags, 0);
        assert_eq!(freeride.choices.traffic_density.to_bits(), 0x3f80_0000);
        assert_eq!(freeride.choices.pedestrian_density.to_bits(), 0x3f80_0000);
        assert_eq!(freeride.choices.police_density.to_bits(), 0x3f80_0000);
    }
    #[test]
    fn extreme_gate_arms_at_or_below_countryside_only() {
        let mut freeride = MainMenuFreeride::default();
        for enabled in [FREERIDE_SMALL_CITY, FREERIDE_CITY, FREERIDE_COUNTRYSIDE] {
            freeride.enabled = enabled;
            assert!(freeride.extreme_gated());
        }
        for enabled in [FREERIDE_EXTREME, FREERIDE_EXTREME + 1, u32::MAX] {
            freeride.enabled = enabled;
            assert!(!freeride.extreme_gated());
        }
        // The gate read mutates nothing.
        assert_eq!(
            freeride,
            MainMenuFreeride {
                enabled: u32::MAX,
                choices: FreerideChoices::default(),
            }
        );
    }
    #[test]
    fn component_latches_default_enabled_and_record_requests() {
        let mut components = MenuComponentLatches::default();
        assert_eq!(components.active_id(), None);
        assert!(components.is_enabled(MAINMENU_ITEM_FREE_RIDE_EXTREME));
        assert!(components.is_enabled(MAINMENU_ITEM_INTRO));
        components.set_disabled(MAINMENU_ITEM_FREE_RIDE_EXTREME, false);
        assert!(!components.is_enabled(MAINMENU_ITEM_FREE_RIDE_EXTREME));
        assert!(components.is_enabled(MAINMENU_ITEM_INTRO));
        components.set_disabled(MAINMENU_ITEM_FREE_RIDE_EXTREME, true);
        assert!(components.is_enabled(MAINMENU_ITEM_FREE_RIDE_EXTREME));
        components.activate_by_id_or_next(MAINMENU_ITEM_INTRO);
        assert_eq!(components.active_id(), Some(MAINMENU_ITEM_INTRO));
    }
    #[test]
    fn oncreate_prefix_orders_latches_and_returns_zero_from_defaults() {
        let mut camera = MenuCameraTarget::new();
        let mut freeride = MainMenuFreeride::default();
        let mut components = MenuComponentLatches::default();
        assert_eq!(
            main_menu_oncreate_prefix(&mut camera, &mut freeride, &mut components),
            0
        );
        assert_eq!(camera.camera_target_id(), MENU_CAMERA_START);
        assert!(!components.is_enabled(MAINMENU_ITEM_FREE_RIDE_EXTREME));
        assert!(components.is_enabled(MAINMENU_ITEM_INTRO));
        assert_eq!(freeride.enabled, FREERIDE_SMALL_CITY);
        assert_eq!(freeride.choices.car, 1);
        assert_eq!(freeride.choices.flags, 0);
        assert_eq!(freeride.choices.traffic_density.to_bits(), 0x3f80_0000);
        assert_eq!(freeride.choices.pedestrian_density.to_bits(), 0x3f80_0000);
        assert_eq!(freeride.choices.police_density.to_bits(), 0x3f80_0000);
        assert_eq!(components.active_id(), Some(MAINMENU_ITEM_INTRO));
    }
    #[test]
    fn oncreate_prefix_skips_the_disable_once_extreme_is_open() {
        let mut camera = MenuCameraTarget::new();
        let mut freeride = MainMenuFreeride {
            enabled: FREERIDE_EXTREME,
            choices: FreerideChoices::default(),
        };
        let mut components = MenuComponentLatches::default();
        assert_eq!(
            main_menu_oncreate_prefix(&mut camera, &mut freeride, &mut components),
            0
        );
        assert_eq!(camera.camera_target_id(), MENU_CAMERA_START);
        assert!(components.is_enabled(MAINMENU_ITEM_FREE_RIDE_EXTREME));
        assert_eq!(components.active_id(), Some(MAINMENU_ITEM_INTRO));
        // A second pass is a stable rewrite, not an accumulation.
        assert_eq!(
            main_menu_oncreate_prefix(&mut camera, &mut freeride, &mut components),
            0
        );
        assert_eq!(camera.camera_target_id(), MENU_CAMERA_START);
        assert!(components.is_enabled(MAINMENU_ITEM_FREE_RIDE_EXTREME));
        assert_eq!(freeride.choices.car, 1);
        assert_eq!(components.active_id(), Some(MAINMENU_ITEM_INTRO));
    }
    #[test]
    fn onclick_dispatch_covers_all_thirteen_native_branches() {
        // Item order follows the original switch (g_m_func.cpp:309-354).
        let close_all = [
            (MAINMENU_ITEM_INTRO, MAINMENU_INTRO),
            (MAINMENU_ITEM_TUTORIAL, MAINMENU_TUTORIAL),
            (MAINMENU_ITEM_NEW_GAME, MAINMENU_NEW_GAME),
            (MAINMENU_ITEM_FREE_RIDE_EXTREME, MAINMENU_EXTREME),
            (MAINMENU_ITEM_RACING, MAINMENU_RACING),
            (MAINMENU_ITEM_MULTIPLAYER, MAINMENU_MULTIPLAYER),
            (MAINMENU_ITEM_CARCYCLOPEDIA, MAINMENU_CARCYCLOPEDIA),
            (MAINMENU_ITEM_CREDITS, MAINMENU_CREDITS),
            (MAINMENU_ITEM_CHANGE_PROFILE, MAINMENU_PROFILE_CHANGE),
        ];
        for (id, result) in close_all {
            assert_eq!(
                main_menu_onclick(id),
                Some(MainMenuClickEffect::CloseAll { result })
            );
        }
        let opens = [
            (MAINMENU_ITEM_LOAD_GAME, MainMenuOpenTarget::LoadGame),
            (MAINMENU_ITEM_FREE_RIDE, MainMenuOpenTarget::FreeRide),
            (MAINMENU_ITEM_OPTIONS, MainMenuOpenTarget::Options),
            (MAINMENU_ITEM_QUIT, MainMenuOpenTarget::ExitQuestion),
        ];
        for (id, target) in opens {
            assert_eq!(
                main_menu_onclick(id),
                Some(MainMenuClickEffect::Open { target })
            );
        }
        // The native default path returns 0 with no menu effect.
        for id in [0, 1, 990100, 990114, 990411, u32::MAX] {
            assert_eq!(main_menu_onclick(id), None);
        }
    }
    #[test]
    fn onclick_effects_carry_result_or_definition_name() {
        let effect = main_menu_onclick(MAINMENU_ITEM_NEW_GAME).unwrap();
        assert_eq!(effect.close_all_result(), Some(MAINMENU_NEW_GAME));
        assert_eq!(effect.open_target(), None);
        let effect = main_menu_onclick(MAINMENU_ITEM_LOAD_GAME).unwrap();
        assert_eq!(effect.close_all_result(), None);
        assert_eq!(effect.open_target(), Some(MainMenuOpenTarget::LoadGame));
        assert_eq!(
            MainMenuOpenTarget::LoadGame.definition_name(),
            "loadgame"
        );
        assert_eq!(MainMenuOpenTarget::FreeRide.definition_name(), "freeride");
        assert_eq!(MainMenuOpenTarget::Options.definition_name(), "optionsb");
        assert_eq!(
            MainMenuOpenTarget::ExitQuestion.definition_name(),
            "exityesno"
        );
        assert_eq!(MENU_CLOSE_CANCEL, u32::MAX);
    }
    #[test]
    fn onchange_routes_the_active_id_to_the_camera_latch() {
        assert_eq!(main_menu_onchange(Some(MAINMENU_ITEM_OPTIONS)), Some(MAINMENU_ITEM_OPTIONS));
        assert_eq!(main_menu_onchange(None), None);
        // The routed id lands in the latch unchanged.
        let mut camera = MenuCameraTarget::new();
        camera.set_camera_target_id(main_menu_onchange(Some(MAINMENU_ITEM_INTRO)).unwrap());
        assert_eq!(camera.camera_target_id(), MAINMENU_ITEM_INTRO);
    }
    #[test]
    fn tick_click_resolves_to_the_onclick_dispatch() {
        assert_eq!(main_menu_tick_click(0), None);
        assert_eq!(
            main_menu_tick_click(MAINMENU_ITEM_QUIT),
            Some(MainMenuClickEffect::Open {
                target: MainMenuOpenTarget::ExitQuestion,
            })
        );
        assert_eq!(
            main_menu_tick_click(MAINMENU_ITEM_RACING),
            Some(MainMenuClickEffect::CloseAll {
                result: MAINMENU_RACING,
            })
        );
        assert_eq!(main_menu_tick_click(12345), None);
    }
    #[test]
    fn execute_guard_runs_while_no_result_is_latched() {
        assert!(execute_menu_continues(0));
        for code in [MAINMENU_QUIT, 0x4fd, u32::MAX] {
            assert!(!execute_menu_continues(code));
        }
    }
    #[test]
    fn component_flags_keep_real_compf_bit_sense() {
        let mut components = MenuComponentLatches::default();
        assert_eq!(
            components.component_flags(MAINMENU_ITEM_INTRO),
            DEFAULT_COMPONENT_FLAGS
        );
        assert_eq!(
            DEFAULT_COMPONENT_FLAGS
                & (COMPONENT_VISIBLE | COMPONENT_ENABLED | COMPONENT_ACTIVATABLE),
            COMPONENT_VISIBLE | COMPONENT_ENABLED | COMPONENT_ACTIVATABLE
        );
        components.set_disabled(MAINMENU_ITEM_INTRO, false);
        assert_eq!(
            components.component_flags(MAINMENU_ITEM_INTRO),
            DEFAULT_COMPONENT_FLAGS & !COMPONENT_ENABLED
        );
        assert!(!components.is_enabled(MAINMENU_ITEM_INTRO));
        components.set_disabled(MAINMENU_ITEM_INTRO, true);
        assert_eq!(
            components.component_flags(MAINMENU_ITEM_INTRO),
            DEFAULT_COMPONENT_FLAGS
        );
    }
    #[test]
    fn oncreate_dispatch_matches_prefix_order_with_real_bits() {
        let mut camera = MenuCameraTarget::new();
        let mut freeride = MainMenuFreeride::default();
        let mut components = MenuComponentLatches::default();
        assert_eq!(
            main_menu_oncreate_dispatch(&mut camera, &mut freeride, &mut components),
            0
        );
        assert_eq!(camera.camera_target_id(), MENU_CAMERA_START);
        assert_eq!(
            components.component_flags(MAINMENU_ITEM_FREE_RIDE_EXTREME),
            DEFAULT_COMPONENT_FLAGS & !COMPONENT_ENABLED
        );
        assert_eq!(components.active_id(), Some(MAINMENU_ITEM_INTRO));
    }
    #[test]
    fn exit_onclick_0x1e0640_closes_or_closes_every_menu() {
        assert_eq!(
            main_menu_exit_onclick(EXITYESNO_NO),
            Some(MainMenuExitClickEffect::Close)
        );
        assert_eq!(
            main_menu_exit_onclick(EXITYESNO_YES),
            Some(MainMenuExitClickEffect::CloseAll {
                result: MAINMENU_QUIT,
            })
        );
        for id in [0, EXITYESNO_WINDOW, 0x89, 0x8c, u32::MAX] {
            assert_eq!(main_menu_exit_onclick(id), None);
        }
    }
    #[test]
    fn definition_leaves_name_their_menus() {
        // Game.exe VA/RVA: 0x5e08e0/0x1e08e0, 0x5e0900/0x1e0900,
        // 0x5e0910/0x1e0910, 0x5e0920/0x1e0920.
        assert_eq!(main_menu_exit_definition(), "exityesno");
        assert_eq!(options_definition(), "optionsb");
        assert_eq!(freeride_definition(), "freeride");
        assert_eq!(load_game_definition(), "loadgame");
        assert_eq!(options_audio_video_definition(), "optionsa");
        assert_eq!(question_box_definition(), "questionbox");
    }
    #[test]
    fn options_onclick_0x1e0ad0_routes_all_three_branches() {
        assert_eq!(
            options_onclick(OPTIONSB_CONTROLS),
            Some(OptionsClickEffect::Open {
                target: OptionsOpenTarget::Controls,
            })
        );
        assert_eq!(
            options_onclick(OPTIONSB_AUDIO_VIDEO),
            Some(OptionsClickEffect::Open {
                target: OptionsOpenTarget::AudioVideo,
            })
        );
        assert_eq!(options_onclick(OPTIONSB_BACK), Some(OptionsClickEffect::Close));
        for id in [0, OPTIONSB_CONTROLS - 1, OPTIONSB_BACK + 1, u32::MAX] {
            assert_eq!(options_onclick(id), None);
        }
    }
    #[test]
    fn options_controls_definition_0x1e1480_selects_the_page_menu() {
        assert_eq!(
            options_controls_definition(OPTIONSC_PAGE_PLAYER),
            "optionsc1"
        );
        assert_eq!(options_controls_definition(OPTIONSC_PAGE_CAR), "optionsc2");
        // The native default arm: the Other page and every other value.
        for page in [OPTIONSC_PAGE_OTHER, 0, 4, u32::MAX] {
            assert_eq!(options_controls_definition(page), "optionsc3");
        }
    }
    #[test]
    fn game_failed_onclick_0x1e2950_maps_both_items() {
        assert_eq!(
            game_failed_onclick(GAMEFAILED_EXIT_ITEM),
            Some(GAMEFAILED_RESULT_EXIT)
        );
        assert_eq!(
            game_failed_onclick(GAMEFAILED_RETRY_ITEM),
            Some(GAMEFAILED_RESULT_RETRY)
        );
        assert_eq!(GAMEFAILED_RESULT_EXIT, 0x4fb);
        assert_eq!(GAMEFAILED_RESULT_RETRY, 0x4fd);
        for id in [0, GAMEFAILED_RETRY_ITEM - 1, GAMEFAILED_EXIT_ITEM + 1, u32::MAX] {
            assert_eq!(game_failed_onclick(id), None);
        }
    }
    #[test]
    fn question_box_onkey_0x1e2380_escapes_by_parent() {
        // Non-Escape keys are the native no-op path.
        for key in [0, QUESTION_BOX_ESCAPE_KEY + 1, u8::MAX] {
            assert_eq!(question_box_onkey(key, 0), None);
            assert_eq!(question_box_onkey(key, 7), None);
        }
        // With a parent menu the box closes with No's code.
        assert_eq!(
            question_box_onkey(QUESTION_BOX_ESCAPE_KEY, 7),
            Some(QuestionBoxClose::Dismiss {
                code: QUESTIONBOX_CODE_NO,
            })
        );
        // Without one every menu closes with No's result.
        assert_eq!(
            question_box_onkey(QUESTION_BOX_ESCAPE_KEY, 0),
            Some(QuestionBoxClose::DismissAll {
                code: QUESTIONBOX_CODE_NO,
                result: QUESTIONBOX_RESULT_NO,
            })
        );
        assert_eq!(QUESTIONBOX_CODE_NO, 0);
        assert_eq!(QUESTIONBOX_RESULT_NO, 0xc);
    }
    #[test]
    fn question_box_onclick_0x1e23b0_maps_yes_no_by_parent() {
        assert_eq!(
            question_box_onclick(QUESTIONBOX_YES, 7),
            Some(QuestionBoxClose::Dismiss {
                code: QUESTIONBOX_CODE_YES,
            })
        );
        assert_eq!(
            question_box_onclick(QUESTIONBOX_NO, 7),
            Some(QuestionBoxClose::Dismiss {
                code: QUESTIONBOX_CODE_NO,
            })
        );
        assert_eq!(
            question_box_onclick(QUESTIONBOX_YES, 0),
            Some(QuestionBoxClose::DismissAll {
                code: QUESTIONBOX_CODE_YES,
                result: QUESTIONBOX_RESULT_YES,
            })
        );
        assert_eq!(
            question_box_onclick(QUESTIONBOX_NO, 0),
            Some(QuestionBoxClose::DismissAll {
                code: QUESTIONBOX_CODE_NO,
                result: QUESTIONBOX_RESULT_NO,
            })
        );
        for id in [0, QUESTIONBOX_WINDOW, 0x89, 0x8c, u32::MAX] {
            assert_eq!(question_box_onclick(id, 0), None);
            assert_eq!(question_box_onclick(id, 7), None);
        }
    }
}
