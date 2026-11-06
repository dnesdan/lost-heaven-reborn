//! Constructor-only C_player ownership. Game08398317... ctor58f170,
//! destructor58f220; deleting wrapper58f200. No Player GameInit/control policy.
//! PLAYER_LOCKCONTROLS (mafia-re 113b5b6 game/mission/C_program.cpp:4172)
//! LockControls GAME 0x595f00 stores its bool here; Do_Breath stays DEFERRED
//! (needs player animation/breath owner, no new RVAs).
#![forbid(unsafe_code)]
use crate::human_actor::{EmptyHumanCollection, HumanSeed, HumanState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSeed {
    pub human: HumanSeed,
    pub allocator_tag: u8,
}
#[derive(Debug)]
pub struct PlayerState {
    pub(crate) constructor_words: [u32; 2],
    pub(crate) collection: EmptyHumanCollection,
    controls_locked: bool,
}
impl PlayerState {
    pub(crate) fn construct(allocator_tag: u8, human: &mut HumanState) -> Self {
        for component in 2..=5 {
            human.properties.set_component_bits(component, 0x4220_0000);
        }
        human.properties.set_component_bits(1, 0x4348_0000);
        human.properties.set_component_bits(10, 0x3f80_0000);
        human.voice_index = 3;
        Self {
            constructor_words: [0, 200],
            collection: EmptyHumanCollection::construct(allocator_tag),
            controls_locked: false,
        }
    }
    pub fn constructor_words(&self) -> &[u32; 2] {
        &self.constructor_words
    }
    pub fn allocator_tag(&self) -> u8 {
        self.collection.allocator_tag
    }
    /// Original C_player lock word for PLAYER_LOCKCONTROLS.
    pub fn controls_locked(&self) -> bool {
        self.controls_locked
    }
    /// Original LockControls(bool) store; Do_Breath is not run here.
    pub fn set_controls_locked(&mut self, locked: bool) {
        self.controls_locked = locked;
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    use crate::human_actor::{HumanAnimationMachineSeed, HumanAuxiliarySeed, HumanProperties};
    fn human_seed() -> HumanSeed {
        HumanSeed {
            allocator_tag: 0xd7,
            animation_machine: HumanAnimationMachineSeed {
                untouched_word: 0xa5a5_a5a5,
            },
            previous_animation_code: 0,
            requested_animation_code: 0,
            human_type_mask: 0,
            current_properties: HumanProperties::from_record([0xa5; 64]),
            target_e: None,
            target_n: None,
            base_frame: None,
            auxiliary: std::array::from_fn(|_| HumanAuxiliarySeed {
                untouched_word: 0,
                padding: [0; 3],
                untouched_tail: [0; 16],
            }),
        }
    }
    #[test]
    fn lock_flag_starts_unlocked_and_stores_bool() {
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert!(!player.controls_locked());
        player.set_controls_locked(true);
        assert!(player.controls_locked());
        player.set_controls_locked(false);
        assert!(!player.controls_locked());
    }
}
