//! C_rail_generator derived storage for the original PC actor lifecycle (kind 38).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor STUB VA0x596620, Init0x596690, LoadData STUB 0x596710,
//! vtable VA0x6259e8, 0x84 bytes (PS2's 0xa0). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference sibling
//! mafia-re @ 113b5b6 game/vehicles/C_rail_generator.h:7 (vtable) + game/vehicles/
//! C_rail_generator.cpp:9-12 (constructor), :26-30 (Init), :40-44 (LoadData);
//! factory game/mission/Mission.cpp:211 (CreateActor GAME 0x53f7d0,
//! ACTOR_RAIL_GENERATOR 0x26). Offsets below are byte offsets into the derived
//! object; the base C_actor occupies 0x70. No PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x84-byte rail generator object.
/// The stub constructor stores only the vtable and kind marker; every other
/// word is admitted untouched allocation, not interpreted native state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailGeneratorSeed {
    pub bytes_00_83: [u8; 0x84],
}

/// Owned rail generator bytes after construction. The written Init body
/// (mafia-re C_rail_generator.cpp:26-30) stores only the base frame, which the
/// shared base init already owns; no derived word has an admitted writer yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailGeneratorState {
    bytes: [u8; 0x84],
}

impl RailGeneratorState {
    pub(crate) fn allocation(seed: RailGeneratorSeed) -> Self {
        Self {
            bytes: seed.bytes_00_83,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Original stub constructor body (mafia-re C_rail_generator.cpp:9-12):
    /// only the base construction plus the vtable store and kind marker, which
    /// live on the base owner (see construct_rail_generator).
    pub(crate) fn complete_constructor(&mut self) {}
}

/// Base-delegating `C_rail_generator` overrides. Every native body below is
/// an unwritten STUB that only logs and runs the base `C_actor` override
/// (mafia-re game/vehicles/C_rail_generator.cpp at pin 113b5b6); the
/// `C_actor` owners stay outside this contract, so each entry is an explicit
/// boundary. The empty destructor (GAME 0x596680, cpp:15-19) files no entry:
/// it stores the vtable again before the base's with no observable state
/// effect, like the honestly unfiled `C_don_ocas` destructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailGeneratorError {
    Unsupported { entry: u32 },
}

/// `C_rail_generator::GameInit` entry, GAME RVA 0x196860 (VA 0x596860;
/// mafia-re game/vehicles/C_rail_generator.cpp:47-52): logs and runs
/// `C_actor::GameInit`.
pub const RAIL_GENERATOR_GAME_INIT_ENTRY: u32 = 0x596860;
/// `C_rail_generator::GameDone` entry, GAME RVA 0x197a20 (VA 0x597a20;
/// mafia-re game/vehicles/C_rail_generator.cpp:54-59): logs and runs
/// `C_actor::GameDone`.
pub const RAIL_GENERATOR_GAME_DONE_ENTRY: u32 = 0x597a20;
/// `C_rail_generator::AI` entry, GAME RVA 0x197bf0 (VA 0x597bf0; mafia-re
/// game/vehicles/C_rail_generator.cpp:61-66): logs and runs `C_actor::AI`.
pub const RAIL_GENERATOR_AI_ENTRY: u32 = 0x597bf0;

/// Original `C_rail_generator::Init`, GAME RVA 0x196690 (VA 0x596690;
/// mafia-re game/vehicles/C_rail_generator.cpp:26-31): stores the frame on
/// the caller-owned `C_actor` base and returns true. The base store is owned
/// by the caller, so the port returns the frame value to publish alongside
/// the success flag; no derived word is written.
pub fn rail_generator_init(frame: u32) -> (u32, bool) {
    (frame, true)
}

/// Original `C_rail_generator::GameInit` stub: reports the base-owner
/// boundary.
pub fn rail_generator_game_init() -> Result<(), RailGeneratorError> {
    Err(RailGeneratorError::Unsupported {
        entry: RAIL_GENERATOR_GAME_INIT_ENTRY,
    })
}

/// Original `C_rail_generator::GameDone` stub: reports the base-owner
/// boundary.
pub fn rail_generator_game_done() -> Result<(), RailGeneratorError> {
    Err(RailGeneratorError::Unsupported {
        entry: RAIL_GENERATOR_GAME_DONE_ENTRY,
    })
}

/// Original `C_rail_generator::AI` stub: reports the base-owner boundary,
/// keeping the admitted tick time untouched by any host simulation.
pub fn rail_generator_ai(_time_ms: u32) -> Result<(), RailGeneratorError> {
    Err(RailGeneratorError::Unsupported {
        entry: RAIL_GENERATOR_AI_ENTRY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_stores_no_derived_words() {
        let mut state = RailGeneratorState::allocation(RailGeneratorSeed {
            bytes_00_83: [0xaa; 0x84],
        });
        state.complete_constructor();
        assert_eq!(state.bytes, [0xaa; 0x84]);
        assert_eq!(state.bytes(), &[0xaa; 0x84]);
    }

    #[test]
    fn rva_196690_init_publishes_frame_and_true() {
        // C_rail_generator::Init GAME RVA 0x196690 (VA 0x596690): frame = frm.
        assert_eq!(rail_generator_init(0), (0, true));
        assert_eq!(rail_generator_init(0x1234), (0x1234, true));
        assert_eq!(rail_generator_init(u32::MAX), (u32::MAX, true));
    }

    #[test]
    fn rva_196860_game_init_reports_base_boundary() {
        // C_rail_generator::GameInit GAME RVA 0x196860 (VA 0x596860).
        assert_eq!(RAIL_GENERATOR_GAME_INIT_ENTRY, 0x596860);
        assert_eq!(
            rail_generator_game_init(),
            Err(RailGeneratorError::Unsupported { entry: 0x596860 })
        );
    }

    #[test]
    fn rva_197a20_game_done_reports_base_boundary() {
        // C_rail_generator::GameDone GAME RVA 0x197a20 (VA 0x597a20).
        assert_eq!(RAIL_GENERATOR_GAME_DONE_ENTRY, 0x597a20);
        assert_eq!(
            rail_generator_game_done(),
            Err(RailGeneratorError::Unsupported { entry: 0x597a20 })
        );
    }

    #[test]
    fn rva_197bf0_ai_reports_base_boundary() {
        // C_rail_generator::AI GAME RVA 0x197bf0 (VA 0x597bf0).
        assert_eq!(RAIL_GENERATOR_AI_ENTRY, 0x597bf0);
        assert_eq!(
            rail_generator_ai(16),
            Err(RailGeneratorError::Unsupported { entry: 0x597bf0 })
        );
    }
}
