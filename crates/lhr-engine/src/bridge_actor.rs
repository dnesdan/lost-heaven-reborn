//! C_bridge derived storage for the original PC actor lifecycle (kind 20).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x567d10, Init0x567e70, LoadData0x56a6b0, ShutDown0x56a7b0,
//! vtable VA0x625588, 0x194 bytes (PS2's 0x1a0). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! Member names are descriptive PC projections; no PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x194-byte bridge object.
/// Pointer words overwritten by construction are not promoted into owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeSeed {
    pub bytes_00_193: [u8; 0x194],
}

/// Owned bridge bytes after construction. Words the constructor stores are
/// observable state, not interpreted native pointers: their only admitted
/// readers are the documented Init/ShutDown/LoadData offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeState {
    bytes: [u8; 0x194],
}

impl BridgeState {
    pub(crate) fn allocation(seed: BridgeSeed) -> Self {
        Self {
            bytes: seed.bytes_00_193,
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
                .expect("bridge word"),
        )
    }

    fn byte(&self, offset: usize) -> u8 {
        self.bytes[offset]
    }

    /// Original constructor body in dependency order: base C_actor
    /// construction runs first (owned by the caller through construct_base),
    /// then the identity-derived control bytes, zeroed ranges, and the stored
    /// timer/flag words. Offsets below are byte offsets; the original indexes
    /// DWORD slots (each times four).
    pub(crate) fn complete_constructor(&mut self, identity: u32) {
        let control = (identity >> 24) as u8;
        self.put_byte(0x1c, control);
        self.put_word(0x74, 0);
        self.put_word(0x78, 0);
        self.put_word(0x7c, 0);
        self.put_byte(0x20, control);
        self.put_word(0x84, 0);
        self.put_word(0x88, 0);
        self.put_word(0x8c, 0);
        self.put_byte(0x5d, 0);
        self.put_word(0x90, 0);
        self.put_word(0x94, 0);
        self.put_byte(0x4f, 0);
        // The vtable store and kind marker live on the base actor owner, not
        // in these derived bytes; see construct_bridge.
        self.put_word(0x178, 0x3f5f66f3);
        self.put_word(0x160, 120000);
        self.put_word(0x164, 4000);
        self.put_word(0x168, 30000);
        self.put_word(0x16c, 10000);
        self.put_word(0x170, 20000);
        self.put_word(0x17c, 0x71);
        self.put_word(0x180, 0x72);
        self.put_word(0x184, 0x73);
        self.put_word(0x188, 0x74);
        self.put_word(0x18c, 0xb6);
        self.put_word(0x190, 0xb5);
        self.put_word(0x138, 7);
    }

    /// ShutDown target flag at +0x13c (byte). The collision-link word at +0xb4
    /// selects the game-owned bridge-collision refresh, which has no backend.
    /// Read-only inspector; the ShutDown effect owner is a later milestone.
    pub fn shutdown_byte(&self) -> u8 {
        self.byte(0x13c)
    }

    #[cfg(test)]
    pub(crate) fn set_shutdown_byte(&mut self, value: u8) {
        self.put_byte(0x13c, value);
    }

    /// Secondary frame handle resolved by Init at +0x90; zero means none.
    pub fn frame_handle(&self) -> u32 {
        self.word(0x90)
    }

    pub(crate) fn set_frame_handle(&mut self, value: u32) {
        self.put_word(0x90, value);
    }

    /// Model-name L-flag at +0x140 selecting the shared sector refresh.
    pub fn model_l_flag(&self) -> u32 {
        self.word(0x140)
    }

    pub(crate) fn set_model_l_flag(&mut self, value: u32) {
        self.put_word(0x140, value);
    }
}

/// Derive the sibling flap name the original Init looks up: an exact copy
/// with the last byte incremented (wrapping), matching the native last-char
/// `+ 1`. An empty name has no sibling; the original finds nothing for it.
pub fn derive_sibling_name(name: &[u8]) -> Option<Vec<u8>> {
    if name.is_empty() {
        return None;
    }
    let mut derived = name.to_vec();
    if let Some(last) = derived.last_mut() {
        *last = last.wrapping_add(1);
    }
    Some(derived)
}

/// The model-name L-flag selecting the shared sector refresh: names starting
/// with L skip it, all others run it.
pub fn model_name_starts_with_l(name: &[u8]) -> bool {
    name.first() == Some(&b'L')
}
/// Shared ferry definition retained from tables\parnik.def by LoadData. The
/// original reads the file per LoadData call into game-global storage; the
/// bytes stay opaque here (an extractor or future effect owner interprets
/// them). A single retained load stands in for repeated identical reloads of
/// the immutable archive file.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BridgeTable {
    bytes: Vec<u8>,
}

impl BridgeTable {
    pub fn from_file(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Shared sector refresh written by Init when the bridge model name does not
/// start with L. Mirrors the game-global DAT_00646fa8 table (two nine-word
/// rows refreshed to 0xffff) and the six cleared words around DAT_00646f80.
/// Meanings are not interpreted; offsets and values are retained exactly.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BridgeSectors {
    pub rows: [[u32; 9]; 2],
    /// Cleared words at DAT_00646f80, 84, 8c, 90, 94, 98, 9c and a0, in order.
    pub cleared: [u32; 8],
}

impl BridgeSectors {
    pub(crate) fn refresh(&mut self) {
        for row in &mut self.rows {
            for word in row.iter_mut() {
                *word = 0xffff;
            }
        }
        self.cleared = [0; 8];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed(identity: u32) -> BridgeState {
        let mut state = BridgeState::allocation(BridgeSeed {
            bytes_00_193: [0xaa; 0x194],
        });
        state.complete_constructor(identity);
        state
    }

    #[test]
    fn constructor_stores_exact_control_words_and_flags() {
        let state = constructed(0x81000000);
        assert_eq!(state.byte(0x1c), 0x81);
        assert_eq!(state.byte(0x20), 0x81);
        for offset in [0x74, 0x78, 0x7c, 0x84, 0x88, 0x8c, 0x90, 0x94] {
            assert_eq!(state.word(offset), 0, "{offset:#x}");
        }
        assert_eq!(state.byte(0x5d), 0);
        assert_eq!(state.byte(0x4f), 0);
        assert_eq!(state.word(0x178), 0x3f5f66f3);
        assert_eq!(
            [
                state.word(0x160),
                state.word(0x164),
                state.word(0x168),
                state.word(0x16c),
                state.word(0x170)
            ],
            [120000, 4000, 30000, 10000, 20000]
        );
        assert_eq!(
            [
                state.word(0x17c),
                state.word(0x180),
                state.word(0x184),
                state.word(0x188)
            ],
            [0x71, 0x72, 0x73, 0x74]
        );
        assert_eq!([state.word(0x18c), state.word(0x190)], [0xb6, 0xb5]);
        assert_eq!(state.word(0x138), 7);
    }

    #[test]
    fn constructor_control_byte_follows_identity_high_byte() {
        let state = constructed(0x42000000);
        assert_eq!(state.byte(0x1c), 0x42);
        assert_eq!(state.byte(0x20), 0x42);
    }

    #[test]
    fn sibling_name_copies_and_increments_last_byte() {
        assert_eq!(
            derive_sibling_name(b"LLsklap01"),
            Some(b"LLsklap02".to_vec())
        );
        assert_eq!(derive_sibling_name(b"s"), Some(b"t".to_vec()));
        assert_eq!(derive_sibling_name(b"\xff"), Some(b"\x00".to_vec()));
        assert_eq!(derive_sibling_name(b""), None);
        assert!(model_name_starts_with_l(b"LLsklap01"));
        assert!(!model_name_starts_with_l(b"sklapx01"));
        assert!(!model_name_starts_with_l(b""));
    }

    #[test]
    fn sector_refresh_sets_table_and_clears_words() {
        let mut sectors = BridgeSectors::default();
        sectors.refresh();
        assert_eq!(sectors.rows, [[0xffff; 9]; 2]);
        assert_eq!(sectors.cleared, [0; 8]);
    }

    #[test]
    fn shutdown_handle_and_l_flag_round_trip() {
        let mut state = constructed(0x81000000);
        assert_eq!(state.shutdown_byte(), 0xaa);
        state.set_shutdown_byte(1);
        assert_eq!(state.shutdown_byte(), 1);
        state.set_frame_handle(0x10203040);
        assert_eq!(state.frame_handle(), 0x10203040);
        state.set_model_l_flag(1);
        assert_eq!(state.model_l_flag(), 1);
    }

    #[test]
    fn ferry_table_retains_opaque_bytes() {
        let table = BridgeTable::from_file(vec![4, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(table.bytes(), &[4, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(BridgeTable::default().bytes(), &[]);
    }
}
