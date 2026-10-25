//! C_railway derived storage for the original PC actor lifecycle.
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor RVA0x865b0, Init0x868c0, LoadData0x86a20, destructor0x86760.
//! The original PS2 kind8 factory calls __ct__9C_railwayFv. Member names below
//! are descriptive PC projections; no PS2 member layout is assumed.

/// Actual untouched allocation bytes after the inherited0x70-byte C_actor.
/// Pointer words overwritten by construction are not promoted into owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailwaySeed {
    pub bytes_70_36f: [u8; 0x300],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailwayVectorSlot {
    At7c,
    AtA8,
    AtEc,
    At100,
    At250,
    At260,
    At278,
}
impl RailwayVectorSlot {
    pub fn control_offset(self) -> usize {
        match self {
            Self::At7c => 0x7c,
            Self::AtA8 => 0xa8,
            Self::AtEc => 0xec,
            Self::At100 => 0x100,
            Self::At250 => 0x250,
            Self::At260 => 0x260,
            Self::At278 => 0x278,
        }
    }
}
/// Non-owning identity of backing supplied by the actual heap owner. Native
/// addresses in static fixtures are identities, never host pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailwayAllocation(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailwayVectorHeader {
    pub control: u8,
    pub begin: Option<RailwayAllocation>,
    pub end: u32,
    pub capacity_end: u32,
}
#[derive(Debug)]
pub struct RailwayState {
    bytes_70_36f: [u8; 0x300],
}
impl RailwayState {
    pub(crate) fn allocation(seed: RailwaySeed) -> Self {
        Self {
            bytes_70_36f: seed.bytes_70_36f,
        }
    }
    pub(crate) fn complete_constructor(&mut self, identity: u32) {
        let high = (identity >> 24) as u8;
        for slot in [
            RailwayVectorSlot::At7c,
            RailwayVectorSlot::AtA8,
            RailwayVectorSlot::AtEc,
            RailwayVectorSlot::At100,
            RailwayVectorSlot::At250,
            RailwayVectorSlot::At260,
            RailwayVectorSlot::At278,
        ] {
            let offset = slot.control_offset();
            self.byte_mut(offset, high);
            for field in [4, 8, 12] {
                self.put_word(offset + field, 0);
            }
        }
        for (offset, value) in [
            (0x158, 0),
            (0x15c, 200),
            // S_rail_human constructor4832e0 at railway+2b0; actual PS2
            // __ct__12S_rail_humanFv corroborates its role, not the PC layout. Its intermediate
            // +40=200 store is followed by+40=0 before returning.
            (0x2ec, 0),
            (0x2f0, 0),
            (0x2f4, 0x12),
            (0x340, 0),
            (0xdc, 0),
            (0x1a8, 1),
            (0x1b0, 0x41880000),
            (0x1ac, 0x41000000),
            (0x1b4, 0x41480000),
            (0x1b8, 0x40400000),
            (0x1bc, 0x461c4000),
            (0x1c0, 0x38d1b717),
            (0xd8, 10000),
            (0x2b8, 0),
            (0x2b4, 0),
            (0x28c, 0),
            (0x290, 0),
            (0x294, 0),
            (0x1dc, 0),
            (0x160, 0),
            (0x164, 0),
        ] {
            self.put_word(offset, value);
        }
        self.byte_mut(0x1d9, 0);
    }
    pub fn bytes(&self) -> &[u8; 0x300] {
        &self.bytes_70_36f
    }
    pub(crate) fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes_70_36f[offset - 0x70..offset - 0x70 + 4]
                .try_into()
                .unwrap(),
        )
    }
    pub fn vector(&self, slot: RailwayVectorSlot) -> RailwayVectorHeader {
        let offset = slot.control_offset();
        let begin = self.word(offset + 4);
        RailwayVectorHeader {
            control: self.bytes_70_36f[offset - 0x70],
            begin: (begin != 0).then_some(RailwayAllocation(begin)),
            end: self.word(offset + 8),
            capacity_end: self.word(offset + 12),
        }
    }
    pub(crate) fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes_70_36f[offset - 0x70..offset - 0x70 + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn byte_mut(&mut self, offset: usize, value: u8) {
        self.bytes_70_36f[offset - 0x70] = value;
    }
    pub(crate) fn clear_vector(&mut self, slot: RailwayVectorSlot) {
        for offset in [4, 8, 12] {
            self.put_word(slot.control_offset() + offset, 0);
        }
    }
    /// Original read-mode, opened nested-chunk branch. Insufficient remaining
    /// bytes skip that field without advancing, as the native guards do. The
    /// caller already owns the decoded file bytes; this does not emulate file
    /// write mode, unbounded chunk stacks or failed dtaRead side effects.
    pub(crate) fn load_payload(&mut self, bytes: &[u8]) -> usize {
        let mut position = usize::from(!bytes.is_empty()); // version byte discarded
        for offset in [0x1a8, 0x1b0, 0x1ac, 0x1b4, 0x1b8, 0x1bc] {
            if let Some(word) = bytes.get(position..position + 4) {
                self.put_word(offset, u32::from_le_bytes(word.try_into().unwrap()));
                position += 4;
            }
        }
        position
    }
}
