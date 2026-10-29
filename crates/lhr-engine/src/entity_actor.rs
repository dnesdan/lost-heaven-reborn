//! C_entity secondary C_program and constructor-empty resource ownership.
//! Game08398317... ctor5071a0, destructor507310, Init507470, LoadData507e20.
//! PS2 names corroborate character-log/substate-lock and unsigned collections;
//! member names here are descriptive, not original layout/member assertions.
#![forbid(unsafe_code)]
use crate::human_actor::{EmptyHumanCollection, HumanSeed};
use crate::original_program::{CProgram, ProgramSeed};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntitySeed {
    pub human: HumanSeed,
    pub program: ProgramSeed,
    pub allocator_tag_a: u8,
    pub allocator_tag_b: u8,
    pub untouched_word: u32,
    pub untouched_padding: [u8; 3],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityResourceSlot {
    Last,
    Auxiliary,
    ThirdUnsigned,
    SecondUnsigned,
    FirstUnsigned,
    SubstateLocks,
    CharacterLogs,
}
#[derive(Debug)]
pub struct EntityState {
    pub(crate) program: Option<CProgram>,
    character_logs: EmptyHumanCollection,
    substate_locks: EmptyHumanCollection,
    unsigned: [EmptyHumanCollection; 3],
    auxiliary: EmptyHumanCollection,
    last: EmptyHumanCollection,
    constructor_words: [u32; 8],
    untouched_word: u32,
    constructor_byte: u8,
    untouched_padding: [u8; 3],
}
impl EntityState {
    pub(crate) fn construct(seed: EntitySeed) -> Self {
        let mut program = CProgram::construct(seed.program);
        program.set_entity_vtable();
        Self {
            program: Some(program),
            character_logs: EmptyHumanCollection::construct(seed.allocator_tag_a),
            substate_locks: EmptyHumanCollection::construct(seed.allocator_tag_a),
            unsigned: [
                EmptyHumanCollection::construct(seed.allocator_tag_a),
                EmptyHumanCollection::construct(seed.allocator_tag_a),
                EmptyHumanCollection::construct(seed.allocator_tag_b),
            ],
            auxiliary: EmptyHumanCollection::construct(seed.allocator_tag_b),
            last: EmptyHumanCollection::construct(seed.allocator_tag_b),
            constructor_words: [0; 8],
            untouched_word: seed.untouched_word,
            constructor_byte: 0,
            untouched_padding: seed.untouched_padding,
        }
    }
    pub fn program(&self) -> Option<&CProgram> {
        self.program.as_ref()
    }
    pub fn constructor_words(&self) -> &[u32; 8] {
        &self.constructor_words
    }
    pub fn untouched_word(&self) -> u32 {
        self.untouched_word
    }
    pub fn constructor_byte(&self) -> u8 {
        self.constructor_byte
    }
    pub fn untouched_padding(&self) -> &[u8; 3] {
        &self.untouched_padding
    }
    fn resource(&self, slot: EntityResourceSlot) -> &EmptyHumanCollection {
        match slot {
            EntityResourceSlot::Last => &self.last,
            EntityResourceSlot::Auxiliary => &self.auxiliary,
            EntityResourceSlot::ThirdUnsigned => &self.unsigned[2],
            EntityResourceSlot::SecondUnsigned => &self.unsigned[1],
            EntityResourceSlot::FirstUnsigned => &self.unsigned[0],
            EntityResourceSlot::SubstateLocks => &self.substate_locks,
            EntityResourceSlot::CharacterLogs => &self.character_logs,
        }
    }
    pub fn allocator_tag(&self, slot: EntityResourceSlot) -> u8 {
        self.resource(slot).allocator_tag
    }
    pub fn collection_is_empty(&self, slot: EntityResourceSlot) -> bool {
        self.resource(slot).is_empty()
    }
    pub(crate) fn clear_resource(&mut self, slot: EntityResourceSlot) {
        match slot {
            EntityResourceSlot::Last => self.last.clear(),
            EntityResourceSlot::Auxiliary => self.auxiliary.clear(),
            EntityResourceSlot::ThirdUnsigned => self.unsigned[2].clear(),
            EntityResourceSlot::SecondUnsigned => self.unsigned[1].clear(),
            EntityResourceSlot::FirstUnsigned => self.unsigned[0].clear(),
            EntityResourceSlot::SubstateLocks => self.substate_locks.clear(),
            EntityResourceSlot::CharacterLogs => self.character_logs.clear(),
        }
    }
}
