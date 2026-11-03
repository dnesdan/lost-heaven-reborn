//! C_pumper derived storage for the original PC actor lifecycle (kind 25).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x47c540, Init0x47c760, LoadData0x47c860,
//! vtable VA0x624990, 0x2ac bytes (PS2's 0x2e0). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! Member names are descriptive PC projections; no PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x2ac-byte pumper object.
/// Pointer words overwritten by construction are not promoted into owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PumperSeed {
    pub bytes_00_2ab: [u8; 0x2ac],
}

/// Owned pumper bytes after construction. Words the constructor stores are
/// observable state, not interpreted native pointers: their only admitted
/// readers are the documented Init/LoadData offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PumperState {
    bytes: [u8; 0x2ac],
}

impl PumperState {
    pub(crate) fn allocation(seed: PumperSeed) -> Self {
        Self {
            bytes: seed.bytes_00_2ab,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_byte(&mut self, offset: usize, value: u8) {
        self.bytes[offset] = value;
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("pumper word"),
        )
    }

    /// Original constructor body in dependency order: base C_actor
    /// construction runs first (owned by the caller through construct_base;
    /// the native intermediate base call has no separately observable state),
    /// then the zeroed words, the count/float words, the six indexed slots
    /// and the marker words. The two lookup words at +0xd4/+0xd8 come from an
    /// unreconstructed table search; they are retained as opaque zeros until
    /// their effect owner exists (no admitted reader exists yet).
    pub(crate) fn complete_constructor(&mut self) {
        self.put_word(0xac, 0);
        self.put_word(0xb0, 200);
        self.put_word(0x228, 0);
        self.put_word(0x22c, 200);
        self.put_word(0x25c, 0);
        self.put_word(0x260, 200);
        self.put_word(0x70, 0x40000000);
        self.put_word(0x74, 0x3f000000);
        self.put_word(0xcc, 0);
        for offset in [0x20c, 0x210, 0x214] {
            self.put_word(offset, 0);
        }
        self.put_word(0xd4, 0);
        self.put_word(0xd8, 0);
        self.put_byte(0xdc, 1);
        self.put_byte(0xdd, 0);
        self.put_word(0x78, 0x29a);
        self.put_word(0x7c, 0x29b);
        self.put_word(0x84, 0x497cc);
        self.put_word(0x80, 0);
        for offset in [0x218, 0x230, 0x208, 0x234, 0x24c, 0x2a8, 0x250, 0x254] {
            self.put_word(offset, 0);
        }
        self.put_byte(0x258, 0);
        for (index, value) in [0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d].iter().enumerate() {
            self.put_word(0xb4 + index * 4, 0);
            self.put_word(0x88 + index * 4, *value);
        }
        self.put_word(0x88, 0x1e);
        self.put_byte(0x264, 0);
        self.put_word(0x25c, 0);
        self.put_word(0x260, 0);
        for offset in [0xa0, 0xa4, 0xa8] {
            self.bytes[offset..offset + 4].copy_from_slice(&0xffffffffu32.to_le_bytes());
        }
        for offset in [0x238, 0x23c, 0x240, 0x244, 0x248, 0x220, 0x224] {
            self.put_word(offset, 0);
        }
    }

    pub(crate) fn put_load_word(&mut self, offset: usize, value: u32) {
        self.put_word(offset, value);
    }

    /// Indexed-slot block at +0x88 read by LoadData over the constructor
    /// values. Short payloads keep the constructor block, like the native
    /// bounded read.
    pub(crate) fn put_load_slots(&mut self, slots: &[u8; 0x18]) {
        self.bytes[0x88..0xa0].copy_from_slice(slots);
    }

    /// Counter word at +0x78 overwritten by every LoadData call.
    pub fn counter_a(&self) -> u32 {
        self.word(0x78)
    }

    /// Counter word at +0x7c overwritten by every LoadData call.
    pub fn counter_b(&self) -> u32 {
        self.word(0x7c)
    }

    /// Mode word at +0x80 overwritten by every LoadData call.
    pub fn mode_word(&self) -> u32 {
        self.word(0x80)
    }

    /// Price word at +0x84 overwritten by every LoadData call.
    pub fn price_word(&self) -> u32 {
        self.word(0x84)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> PumperState {
        let mut state = PumperState::allocation(PumperSeed {
            bytes_00_2ab: [0xaa; 0x2ac],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_exact_words_slots_and_markers() {
        let state = constructed();
        assert_eq!(state.word(0xac), 0);
        assert_eq!(state.word(0xb0), 200);
        assert_eq!(state.word(0x228), 0);
        assert_eq!(state.word(0x22c), 200);
        assert_eq!(state.word(0x70), 0x40000000);
        assert_eq!(state.word(0x74), 0x3f000000);
        assert_eq!(state.word(0xcc), 0);
        assert_eq!(state.word(0xd4), 0);
        assert_eq!(state.word(0xd8), 0);
        assert_eq!(state.bytes[0xdc], 1);
        assert_eq!(state.bytes[0xdd], 0);
        assert_eq!(state.word(0x78), 0x29a);
        assert_eq!(state.word(0x7c), 0x29b);
        assert_eq!(state.word(0x84), 0x497cc);
        assert_eq!(state.word(0x80), 0);
        assert_eq!(state.word(0x88), 0x1e);
        for (index, value) in [0x79u32, 0x7a, 0x7b, 0x7c, 0x7d].iter().enumerate() {
            assert_eq!(state.word(0x8c + index * 4), *value);
        }
        for offset in (0xb4..0xcc).step_by(4) {
            assert_eq!(state.word(offset), 0, "{offset:#x}");
        }
        assert_eq!(state.bytes[0x264], 0);
        assert_eq!(state.word(0x25c), 0);
        assert_eq!(state.word(0x260), 0);
        for offset in [0xa0, 0xa4, 0xa8] {
            assert_eq!(state.word(offset), 0xffffffff, "{offset:#x}");
        }
        for offset in [0x238, 0x23c, 0x240, 0x244, 0x248, 0x220, 0x224] {
            assert_eq!(state.word(offset), 0, "{offset:#x}");
        }
        assert_eq!(state.bytes[0x258], 0);
        // Untouched allocation survives outside the constructor writes.
        assert_eq!(state.bytes[0x00], 0xaa);
    }

    #[test]
    fn load_accessors_round_trip_stored_words() {
        let mut state = constructed();
        state.put_load_word(0x78, 670);
        state.put_load_word(0x7c, 669);
        state.put_load_word(0x80, 2);
        state.put_load_word(0x84, 300748);
        assert_eq!(state.counter_a(), 670);
        assert_eq!(state.counter_b(), 669);
        assert_eq!(state.mode_word(), 2);
        assert_eq!(state.price_word(), 300748);
    }
}
