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
//! DEFERRED with explicit markers (no credit, untouched here): the OnCreate
//! dispatch at VA 0x5e0690 (RVA 0x1e0690) past the pure prefix wired below;
//! the 0x5e0670-0x5e0683 tail (JNZ operand, second MOV, RET) belongs to the
//! OnBeforeCreate leaf; the distinct 12-byte code run at VA 0x5e0684-0x5e068f
//! is present-but-uncredited code needing an owner (not padding);
//! GM_MainMenu::OnChange and GM_MainMenu::OnClick have no owner here; the
//! GM_Menu::Create 0x5ea2e0 construction stack and GameLoopSingle dispatch
//! stay deferred. A select return never advances any of them automatically.
//!
//! OnCreate ownership (Game.exe SHA-256 as above, base 0x400000; reference
//! sibling mafia-re @ 113b5b6 game/ui/g_m_func.cpp:286-294
//! GM_MainMenu::OnCreate 0x5e0690, cited only, never vendored; marker-only,
//! no REA body observed in this session): the three small global owners the
//! body needs now exist below as pure state with authored tests (no cargo
//! execution is claimed): (1) MenuCameraTarget, the lastKameraTargetID latch
//! (g_menu.h:323) written by SetCameraTargetID 0x5ebe80, (2)
//! MainMenuFreeride, the freeride_enabled word (g_loadsave.h, +0x1f0) plus
//! the FreerideStoreA 0x60e920 triple, reusing the checked loadsave/settings
//! owners (game_loadsave_controls.rs, game_loadsave_lifecycle.rs,
//! mission_load_session.rs), none of which owns freeride state, and (3)
//! MenuComponentLatches, the per-ID disabled request plus the active-selection
//! latch, reusing the checked menu.rs host (hardcoded enabled() for
//! 990103|990113, selected index, 990101 Intro mapping), which owns no
//! per-ID disabled flag and no active-selection latch. main_menu_oncreate_prefix
//! wires the ordered latch transition; the component dispatch itself stays
//! DEFERRED (no menu-stack/component owners yet: FindComponentByID 0x5eb2f0
//! plus real COMPF flag bits, the ActivateByIDOrNext 0x5eb210 fallback
//! search, and SetCameraTargetID frame positioning past the ID latch all
//! need scene/component owners), so RVA 0x1e0690 takes no overlay entry:
//! owners-only helpers receive no function credit.
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

/// GM_Component flag bits for the deferred dispatch below (g_m_comps.h; names
/// ours per upstream): COMPF_ENABLED 0x20 is GM_Menu::SetDisabled's bit,
/// COMPF_ACTIVE 0x80000000 marks the menu's active component. Reference bits
/// only — no component owner applies them yet.
pub const COMPONENT_ENABLED: u32 = 0x20;
pub const COMPONENT_ACTIVE: u32 = 0x8000_0000;

/// Main-menu item ids from tables/menu/mainmenu.mnu, decimal
/// (g_m_func.cpp:28-34; symbolic names follow upstream, disclosed here).
/// Consistent with the menu.rs host, which maps 990101 to Intro and treats
/// 990106 as a disabled row.
pub const MAINMENU_ITEM_INTRO: u32 = 990101;
pub const MAINMENU_ITEM_FREE_RIDE_EXTREME: u32 = 990106;

/// Minimal per-ID disabled requests plus the active-selection latch. The
/// menu.rs host owns no per-ID disabled flag (its enabled() hardcodes
/// 990103|990113) and no active-selection latch (only a row index), so this
/// is a genuine gap. Absent ids read as enabled: the request latch's neutral
/// element (no SetDisabled call yet); definition-flag defaults stay with the
/// future component owner (DEFERRED). SCOPE: request latches only; component
/// lookup (FindComponentByID 0x5eb2f0, including its missing-component
/// no-op) and the ActivateByIDOrNext 0x5eb210 enabled/visible/activatable
/// checks plus the FindComponentByIDNext fallback need the component list
/// owner and stay DEFERRED.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuComponentLatches {
    enabled: std::collections::BTreeMap<u32, bool>,
    active: Option<u32>,
}

impl MenuComponentLatches {
    /// GM_Menu::SetDisabled (GAME 0x5eaed0) request record. The bool keeps
    /// the original sense (g_menu.cpp:904-913): true sets the COMPF_ENABLED
    /// bit, false clears it, so OnCreate's SetDisabled(990106, false)
    /// disables Free Ride Extreme.
    pub fn set_disabled(&mut self, id: u32, enabled: bool) {
        self.enabled.insert(id, enabled);
    }

    /// Live read of the latched ENABLED sense for one id.
    pub fn is_enabled(&self, id: u32) -> bool {
        self.enabled.get(&id).copied().unwrap_or(true)
    }

    /// GM_Menu::ActivateByIDOrNext (GAME 0x5eb210) request latch: records
    /// the requested id. Flag checks and the next-component fallback stay
    /// DEFERRED (no component owners yet).
    pub fn activate_by_id_or_next(&mut self, id: u32) {
        self.active = Some(id);
    }

    /// Live read of the latched active selection, if any.
    pub fn active_id(&self) -> Option<u32> {
        self.active
    }
}

/// GM_MainMenu::OnCreate pure prefix (Game.exe VA 0x5e0690, RVA 0x1e0690;
/// sibling g_m_func.cpp:286-294, cited only): the ordered latch transition
/// over the three owned globals. Native order: latch MENUCAM_START via
/// 0x5ebe80; when the freeride_enabled word (sibling
/// mafiaLoadSave.freeride_enabled; absolute address unverified in this
/// marker-only session) reads <= FREERIDE_COUNTRYSIDE, record
/// SetDisabled(FREE_RIDE_EXTREME, false); FreerideStoreA triple (1, 0, 1.0,
/// 1.0, 1.0) via 0x60e920; latch ActivateByIDOrNext(INTRO 990101) via
/// 0x5eb210; return 0. The full dispatch (real COMPF flag bits
/// on components, the next-component fallback, SetCameraTargetID frame
/// positioning, menu-stack effects) stays DEFERRED for the missing
/// scene/component owners, so this prefix takes no overlay function entry.
pub fn main_menu_oncreate_prefix(
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
}
