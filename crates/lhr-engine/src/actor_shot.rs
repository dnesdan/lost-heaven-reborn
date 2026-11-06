//! C_actor_shot derived storage for the original PC actor lifecycle (kind 33).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor STUB VA0x566f40, Init STUB 0x566f90, Update STUB 0x566fd0,
//! GameInit STUB 0x567330, GameDone STUB 0x5673f0, vtable VA0x625458, 0x8c bytes
//! (PS2's 0xa0). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference sibling
//! mafia-re @ 113b5b6 game/actors/C_actor_shot.h:25 (vtable) + game/actors/
//! C_actor_shot.cpp:9-12 (constructor), :53-57 (Init), :60-64 (Update), :67-71
//! (GameInit); factory game/mission/Mission.cpp:211 (CreateActor GAME 0x53f7d0,
//! ACTOR_ACTOR_SHOT 0x21). Offsets below are byte offsets into the derived
//! object; the base C_actor occupies 0x70. Member names follow mafia-re; the
//! firing/shoot guesses are mafia-re's. No PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x8c-byte shot object. The stub
/// constructor stores only the vtable and kind marker; every other word is
/// admitted untouched allocation, not interpreted native state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorShotSeed {
    pub bytes_00_8b: [u8; 0x8c],
}

/// Owned shot bytes after construction. Words the stub bodies store are
/// observable state, not interpreted native pointers: their only admitted
/// readers are the documented firing-state offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorShotState {
    bytes: [u8; 0x8c],
}

impl ActorShotState {
    pub(crate) fn allocation(seed: ActorShotSeed) -> Self {
        Self {
            bytes: seed.bytes_00_8b,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    // Test-only storage helpers until evidenced writers land (native stub
    // Init/Update store no derived words).
    #[allow(dead_code)]
    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("shot word"),
        )
    }

    /// Original stub constructor body (mafia-re C_actor_shot.cpp:9-12): only
    /// the base construction plus the vtable store and kind marker, which live
    /// on the base owner (see construct_actor_shot). No derived word is stored;
    /// the -1 particle default has no evidenced store in the stub bodies and
    /// stays allocation bytes until its effect owner exists.
    pub(crate) fn complete_constructor(&mut self) {}

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_shot_time(&mut self, value: i32) {
        self.put_word(0x78, value as u32);
    }

    /// Shot start word at +0x78; -1 lets Update start a shot (mafia-re guess).
    pub fn shot_time(&self) -> i32 {
        self.word(0x78) as i32
    }

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_firing(&mut self, value: bool) {
        self.bytes[0x7c] = u8::from(value);
    }

    /// Firing flag at +0x7c; Update does nothing without it (mafia-re guess).
    pub fn firing(&self) -> bool {
        self.bytes[0x7c] != 0
    }

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_shoots(&mut self, value: bool) {
        self.bytes[0x84] = u8::from(value);
    }

    /// Shoot flag at +0x84; Update fires C_game::NewShoot with it (mafia-re guess).
    pub fn shoots(&self) -> bool {
        self.bytes[0x84] != 0
    }

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_particle(&mut self, value: i32) {
        self.put_word(0x88, value as u32);
    }

    /// Record particle word at +0x88; -1 means none (mafia-re name).
    pub fn particle(&self) -> i32 {
        self.word(0x88) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> ActorShotState {
        let mut state = ActorShotState::allocation(ActorShotSeed {
            bytes_00_8b: [0xaa; 0x8c],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_no_derived_words() {
        let state = constructed();
        assert_eq!(state.bytes, [0xaa; 0x8c]);
    }

    #[test]
    fn firing_state_round_trips_stored_words() {
        let mut state = constructed();
        assert_eq!(state.shot_time(), 0xaaaaaaaau32 as i32);
        assert!(state.firing());
        assert!(state.shoots());
        assert_eq!(state.particle(), 0xaaaaaaaau32 as i32);
        state.set_shot_time(-1);
        state.set_firing(false);
        state.set_shoots(true);
        state.set_particle(-1);
        assert_eq!(state.shot_time(), -1);
        assert!(!state.firing());
        assert!(state.shoots());
        assert_eq!(state.particle(), -1);
        state.set_shot_time(120);
        state.set_firing(true);
        state.set_shoots(false);
        state.set_particle(7);
        assert_eq!(state.shot_time(), 120);
        assert!(state.firing());
        assert!(!state.shoots());
        assert_eq!(state.particle(), 7);
    }
}
