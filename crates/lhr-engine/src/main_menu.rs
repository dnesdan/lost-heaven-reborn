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
//! DEFERRED with explicit markers (no credit, untouched here): OnCreate at VA
//! 0x5e0690 (RVA 0x1e0690) needs camera/freeride owners; the 0x5e0670-0x5e0683
//! tail (JNZ operand, second MOV, RET) belongs to this leaf; the distinct
//! 12-byte code run at VA 0x5e0684-0x5e068f is present-but-uncredited code
//! needing an owner (not padding); GM_MainMenu::OnChange and GM_MainMenu::OnClick have no owner here; the
//! GM_Menu::Create 0x5ea2e0 construction stack and GameLoopSingle dispatch
//! stay deferred. A select return never advances any of them automatically.
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
}
