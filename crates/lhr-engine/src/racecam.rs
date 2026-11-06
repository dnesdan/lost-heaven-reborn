//! C_racecam derived storage for the original PC actor lifecycle (kind 28).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor STUB VA0x596200, LoadData STUB 0x5962e0, vtable VA0x625950,
//! 0x88 bytes (PS2's 0xa0). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference sibling
//! mafia-re @ 113b5b6 game/camera/c_racecam.h:7 (vtable) + game/camera/
//! c_racecam.cpp:8-11 (constructor), :46-50 (LoadData); factory game/mission/
//! Mission.cpp:211 (CreateActor GAME 0x53f7d0, ACTOR_RACECAM 0x1c). Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! No PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x88-byte racecam object. The stub
/// constructor stores only the vtable and kind marker; every other word is
/// admitted untouched allocation, not interpreted native state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RacecamSeed {
    pub bytes_00_87: [u8; 0x88],
}

/// Owned racecam bytes after construction. The stub Init/DuplicateFrom/
/// GameInit/GameDone bodies delegate to the base, so no derived word has an
/// admitted writer yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RacecamState {
    bytes: [u8; 0x88],
}

impl RacecamState {
    pub(crate) fn allocation(seed: RacecamSeed) -> Self {
        Self {
            bytes: seed.bytes_00_87,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Original stub constructor body (mafia-re c_racecam.cpp:8-11): only the
    /// base construction plus the vtable store and kind marker, which live on
    /// the base owner (see construct_racecam).
    pub(crate) fn complete_constructor(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_stores_no_derived_words() {
        let mut state = RacecamState::allocation(RacecamSeed {
            bytes_00_87: [0xaa; 0x88],
        });
        state.complete_constructor();
        assert_eq!(state.bytes, [0xaa; 0x88]);
        assert_eq!(state.bytes(), &[0xaa; 0x88]);
    }
}
