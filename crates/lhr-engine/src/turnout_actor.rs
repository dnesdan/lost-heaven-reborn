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
        assert_eq!(
            turnout_save_game_size(i32::MAX),
            i32::MIN.wrapping_add(15)
        );
    }
}
