//! C_wagon owned save-size and script event for the original PC actor lifecycle.
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor STUB VA0x4f1b50, vtable VA0x624e78, 0x1a8 bytes (PS2's 0x1e0).
//! Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference
//! sibling mafia-re @ 113b5b6 game/vehicles/C_wagon.h:13 (vtable),
//! :62 (path), :66 (`unknown_a8`), :69-70 (event owners) +
//! game/vehicles/C_wagon.cpp:33-40 (SaveGameGetSize). Only the path/event
//! owners read by the ported functions are modeled; the base `C_actor` size
//! and the mission web-path lookup behind `GetLastNode` (GAME 0x47b710,
//! unported: the `act_mission->web_path` owner stays outside this contract)
//! stay caller-owned.

/// Base save-game bytes written by `C_wagon` past the base `C_actor`
/// payload and the two counted vectors.
pub const WAGON_SAVE_GAME_BASE_EXTRA: i32 = 95;

/// `C_wagon::SaveGameGetSize` entry, GAME RVA 0xf1ca0 (VA 0x4f1ca0).
pub const WAGON_SAVE_GAME_GET_SIZE_ENTRY: u32 = 0x4f1ca0;
/// `C_wagon::SetEvent` entry, GAME RVA 0x7b6f0 (VA 0x47b6f0).
pub const WAGON_SET_EVENT_ENTRY: u32 = 0x47b6f0;

/// Owned wagon path/event state: the web-path node list at +0x7c, the second
/// counted vector at +0xa8, and the script/event pair from `WAGON_SETEVENT`
/// at +0x198/+0x19c. The event program is an opaque native identity, never
/// dereferenced on the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WagonState {
    path: Vec<u32>,
    unknown_a8: Vec<u32>,
    event_program: u32,
    event_id: i32,
}

impl WagonState {
    pub fn path(&self) -> &[u32] {
        &self.path
    }

    pub fn path_mut(&mut self) -> &mut Vec<u32> {
        &mut self.path
    }

    pub fn event_program(&self) -> u32 {
        self.event_program
    }

    pub fn event_id(&self) -> i32 {
        self.event_id
    }

    fn counted_len(len: usize) -> i32 {
        i32::try_from(len).unwrap_or(i32::MAX)
    }

    /// Original `C_wagon::SaveGameGetSize`, GAME RVA 0xf1ca0 (VA 0x4f1ca0;
    /// mafia-re game/vehicles/C_wagon.cpp:33-40 at pin 113b5b6): the base
    /// size plus 95 plus four bytes per entry of both counted vectors.
    /// 32-bit wrapping matches the native `int` arithmetic.
    pub fn save_game_size(&self, base_size: i32) -> i32 {
        base_size
            .wrapping_add(WAGON_SAVE_GAME_BASE_EXTRA)
            .wrapping_add(Self::counted_len(self.unknown_a8.len()).wrapping_mul(4))
            .wrapping_add(Self::counted_len(self.path.len()).wrapping_mul(4))
    }

    /// Original `C_wagon::SetEvent`, GAME RVA 0x7b6f0 (VA 0x47b6f0;
    /// mafia-re game/vehicles/C_wagon.h:44-49 at pin 113b5b6, reached from
    /// the scripts' `WAGON_SETEVENT`): stores the event program and id.
    pub fn set_event(&mut self, program: u32, id: i32) {
        self.event_program = program;
        self.event_id = id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rva_f1ca0_save_game_size_counts_both_vectors() {
        // C_wagon::SaveGameGetSize GAME RVA 0xf1ca0 (VA 0x4f1ca0).
        assert_eq!(WAGON_SAVE_GAME_GET_SIZE_ENTRY, 0x4f1ca0);
        assert_eq!(WAGON_SAVE_GAME_BASE_EXTRA, 95);
        let mut state = WagonState::default();
        assert_eq!(state.save_game_size(0), 95);
        state.path_mut().extend([1, 2, 3]);
        assert_eq!(state.save_game_size(10), 10 + 95 + 12);
        state.unknown_a8.push(7);
        state.unknown_a8.push(8);
        assert_eq!(state.save_game_size(10), 10 + 95 + 8 + 12);
        assert_eq!(state.save_game_size(i32::MAX), i32::MAX.wrapping_add(95 + 8 + 12));
    }

    #[test]
    fn rva_7b6f0_set_event_stores_program_and_id() {
        // C_wagon::SetEvent GAME RVA 0x7b6f0 (VA 0x47b6f0).
        assert_eq!(WAGON_SET_EVENT_ENTRY, 0x47b6f0);
        let mut state = WagonState::default();
        assert_eq!((state.event_program(), state.event_id()), (0, 0));
        state.set_event(0x1234, 9);
        assert_eq!((state.event_program(), state.event_id()), (0x1234, 9));
        state.set_event(0, -1);
        assert_eq!((state.event_program(), state.event_id()), (0, -1));
    }
}
