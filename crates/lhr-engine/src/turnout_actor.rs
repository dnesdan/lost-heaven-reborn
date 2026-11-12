//! C_turnout owned save-size for the original PC actor lifecycle (kind 40).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor STUB VA0x4bc360, vtable VA0x624d58, 0x1a0 bytes (PS2's 0x1d0).
//! Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference
//! sibling mafia-re @ 113b5b6 game/vehicles/C_turnout.h:8 (vtable) +
//! game/vehicles/C_turnout.cpp:8-12 (constructor), :77-81 (SaveGameGetSize),
//! :97-101 (`Start` STUB GAME 0x4bd210, unported: the turnout-switch owner
//! stays outside this contract). Only the save-game size is reconstructed;
//! the base `C_actor` size stays caller-owned.

/// Extra save-game bytes written by `C_turnout` past the base `C_actor`
/// payload.
pub const TURNOUT_SAVE_GAME_EXTRA: i32 = 16;

/// `C_turnout::SaveGameGetSize` entry, GAME RVA 0xbd420 (VA 0x4bd420).
pub const TURNOUT_SAVE_GAME_GET_SIZE_ENTRY: u32 = 0x4bd420;

/// Original `C_turnout::SaveGameGetSize`, GAME RVA 0xbd420 (VA 0x4bd420;
/// mafia-re game/vehicles/C_turnout.cpp:77-81 at pin 113b5b6): the base
/// size plus 16. 32-bit wrapping matches the native `int` addition.
pub fn turnout_save_game_size(base_size: i32) -> i32 {
    base_size.wrapping_add(TURNOUT_SAVE_GAME_EXTRA)
}

/// Explicit boundary for the still-unimplemented small turnout switch below.
/// The entry names the original PC address (VA); the turnout-switch owner
/// stays outside this contract, so it reports the boundary instead of
/// staging a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnoutStubError {
    Unsupported { entry: u32 },
}

/// `C_turnout::Start` entry, GAME RVA 0xbd210 (VA 0x4bd210; mafia-re
/// game/vehicles/C_turnout.cpp:97-101 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): switches the turnout.
pub const TURNOUT_START_ENTRY: u32 = 0x4bd210;

/// `C_turnout::~C_turnout` entry, GAME RVA 0xbc490 (VA 0x4bc490; mafia-re
/// game/vehicles/C_turnout.cpp:15-18 at pin fe7e746; `FUNCTION: GAME` code in
/// Game.exe, not a data offset and not another binary): declared empty in the
/// reference (it re-stores the vtable before the base destructor runs).
pub const TURNOUT_DESTROY_ENTRY: u32 = 0x4bc490;

/// `C_turnout::SaveGameSave` entry, GAME RVA 0xbd430 (VA 0x4bd430; mafia-re
/// game/vehicles/C_turnout.cpp:83-88 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): forwards to the base
/// `C_actor::SaveGameSave`.
pub const TURNOUT_SAVE_GAME_SAVE_ENTRY: u32 = 0x4bd430;

/// Original `C_turnout::~C_turnout` (mafia-re
/// game/vehicles/C_turnout.cpp:15-18 at pin fe7e746; `FUNCTION: GAME` VA
/// 0x4bc490, RVA 0xbc490): the reference body is empty. No turnout state lives
/// in this contract, so destruction is a no-op.
pub fn turnout_destroy() {}

/// Original `C_turnout::Start` stub (mafia-re
/// game/vehicles/C_turnout.cpp:97-101 at pin fe7e746; `STUB: GAME` VA
/// 0x4bd210, RVA 0xbd210): the original only logs. The turnout-switch owner
/// stays outside this contract, so the boundary is explicit.
pub fn turnout_start() -> Result<(), TurnoutStubError> {
    Err(TurnoutStubError::Unsupported {
        entry: TURNOUT_START_ENTRY,
    })
}

/// Original `C_turnout::SaveGameSave` stub (mafia-re
/// game/vehicles/C_turnout.cpp:83-88 at pin fe7e746; `STUB: GAME` VA
/// 0x4bd430, RVA 0xbd430): the original only logs and returns the base
/// `C_actor::SaveGameSave` answer. The save-buffer owner stays outside this
/// contract, so the boundary is explicit; the native `bool` answer is
/// preserved in the return type.
pub fn turnout_save_game_save() -> Result<bool, TurnoutStubError> {
    Err(TurnoutStubError::Unsupported {
        entry: TURNOUT_SAVE_GAME_SAVE_ENTRY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rva_bd420_save_game_size_adds_sixteen() {
        // C_turnout::SaveGameGetSize GAME RVA 0xbd420 (VA 0x4bd420).
        assert_eq!(TURNOUT_SAVE_GAME_GET_SIZE_ENTRY, 0x4bd420);
        assert_eq!(TURNOUT_SAVE_GAME_EXTRA, 16);
        assert_eq!(turnout_save_game_size(0), 16);
        assert_eq!(turnout_save_game_size(64), 80);
        assert_eq!(turnout_save_game_size(i32::MAX), i32::MIN.wrapping_add(15));
    }

    #[test]
    fn rva_bd210_start_reports_unsupported() {
        // C_turnout::Start GAME RVA 0xbd210 (VA 0x4bd210).
        assert_eq!(TURNOUT_START_ENTRY, 0x4bd210);
        assert_eq!(
            turnout_start(),
            Err(TurnoutStubError::Unsupported { entry: 0x4bd210 })
        );
    }

    #[test]
    fn rva_bc490_destroy_is_empty() {
        // C_turnout::~C_turnout GAME RVA 0xbc490 (VA 0x4bc490).
        assert_eq!(TURNOUT_DESTROY_ENTRY, 0x4bc490);
        turnout_destroy();
    }

    #[test]
    fn rva_bd430_save_game_save_reports_unsupported() {
        // C_turnout::SaveGameSave GAME RVA 0xbd430 (VA 0x4bd430).
        assert_eq!(TURNOUT_SAVE_GAME_SAVE_ENTRY, 0x4bd430);
        assert_eq!(
            turnout_save_game_save(),
            Err(TurnoutStubError::Unsupported { entry: 0x4bd430 })
        );
    }
}
