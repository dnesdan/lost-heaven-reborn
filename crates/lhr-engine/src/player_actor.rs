//! Constructor-only C_player ownership. Game08398317... ctor58f170,
//! destructor58f220; deleting wrapper58f200. No Player GameInit/control policy.
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
        }
    }
    pub fn constructor_words(&self) -> &[u32; 2] {
        &self.constructor_words
    }
    pub fn allocator_tag(&self) -> u8 {
        self.collection.allocator_tag
    }
}
