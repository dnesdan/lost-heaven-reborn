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

/// Original RacingInMainMenu (FUNCTION GAME 0x5bebd0).
/// Always false in this binary, so the menu keeps the "mainmenu" mission.
pub fn racing_in_main_menu() -> bool {
    false
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
}
