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
}
