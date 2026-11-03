//! C_door derived storage for the original PC actor lifecycle (kind 6).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x439290, Init0x4393a0, LoadData0x43b030,
//! vtable VA0x623b20, 0x1b8 bytes (PS2's 0x200). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! Member names are descriptive PC projections; no PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x1b8-byte door object.
/// Pointer words overwritten by construction are not promoted into owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorSeed {
    pub bytes_00_1b7: [u8; 0x1b8],
}

/// Owned door bytes after construction. Words the constructor stores are
/// observable state, not interpreted native pointers: their only admitted
/// readers are the documented Init/LoadData offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorState {
    bytes: [u8; 0x1b8],
}

impl DoorState {
    pub(crate) fn allocation(seed: DoorSeed) -> Self {
        Self {
            bytes: seed.bytes_00_1b7,
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
                .expect("door word"),
        )
    }

    /// Original constructor body in dependency order: base C_actor
    /// construction runs first (owned by the caller through construct_base),
    /// then the zeroed ranges, the stored timer words, the vtable-adjacent
    /// kind marker (on the base owner, see construct_door), the flag bytes
    /// and the float word. The three sound names are written to shared
    /// game-global buffers, retained on the world (see [`DoorSoundNames`]).
    pub(crate) fn complete_constructor(&mut self) {
        for offset in (0x78..0xc0).step_by(4) {
            self.put_word(offset, 0);
        }
        self.put_word(0x190, 0);
        for offset in (0x198..0x1b8).step_by(4) {
            self.put_word(offset, 0);
        }
        self.put_word(0x128, 0x3b83126f);
        self.put_word(0x12c, 0x3b83126f);
        self.put_byte(0x11d, 1);
        self.put_byte(0x11e, 0);
        self.put_word(0x120, 0x3fc90fdb);
        self.put_byte(0x125, 0);
        self.put_byte(0x124, 0);
        self.put_byte(0x11c, 0);
        self.put_byte(0x164, 0);
    }

    pub(crate) fn put_load_byte(&mut self, offset: usize, value: u8) {
        self.put_byte(offset, value);
    }

    pub(crate) fn put_load_word(&mut self, offset: usize, value: u32) {
        self.put_word(offset, value);
    }

    pub(crate) fn put_load_name(&mut self, offset: usize, name: &[u8]) {
        let slot = &mut self.bytes[offset..offset + 16];
        slot.fill(0);
        let length = name.len().min(16);
        slot[..length].copy_from_slice(&name[..length]);
    }

    /// Lock-flag byte at +0x11c selected by the version-7/8 LoadData path.
    pub fn lock_flag(&self) -> u8 {
        self.bytes[0x11c]
    }

    /// Open-sound name retained at +0x130 by every LoadData version.
    pub fn open_sound(&self) -> &[u8] {
        nul_terminated(&self.bytes[0x130..0x140])
    }

    /// Close-sound name retained at +0x140 by the LoadData tail.
    pub fn close_sound(&self) -> &[u8] {
        nul_terminated(&self.bytes[0x140..0x150])
    }

    /// Locked-sound name retained at +0x150 by the LoadData tail.
    pub fn locked_sound(&self) -> &[u8] {
        nul_terminated(&self.bytes[0x150..0x160])
    }

    /// Timer word at +0x128 shared by every LoadData version.
    pub fn timer_word(&self) -> u32 {
        self.word(0x128)
    }
}

fn nul_terminated(slot: &[u8]) -> &[u8] {
    let length = slot
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(slot.len());
    &slot[..length]
}

/// Shared default sound names written by every door construction into
/// game-global buffers. Later constructions overwrite earlier ones, matching
/// the native repeated strcpy into static storage; door effects (unimplemented)
/// read these back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DoorSoundNames {
    pub open: [u8; 16],
    pub close: [u8; 16],
    pub locked: [u8; 16],
}

impl DoorSoundNames {
    pub(crate) fn construction_defaults() -> Self {
        let mut names = Self {
            open: [0; 16],
            close: [0; 16],
            locked: [0; 16],
        };
        names.open[..12].copy_from_slice(b"d_woodop.wav");
        names.close[..12].copy_from_slice(b"d_woodcl.wav");
        names.locked[..12].copy_from_slice(b"d_woodlo.wav");
        names
    }
}

/// Payload versions admitted by C_door::LoadData. Versions 5-7 populate
/// ascending field prefixes; version 8 populates the full flag/field set.
/// Any other version consumes only its own version word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorLoadVersion {
    V5,
    V6,
    V7,
    V8,
    Unknown,
}

impl DoorLoadVersion {
    pub fn decode(word: u32) -> Self {
        match word {
            5 => Self::V5,
            6 => Self::V6,
            7 => Self::V7,
            8 => Self::V8,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> DoorState {
        let mut state = DoorState::allocation(DoorSeed {
            bytes_00_1b7: [0xaa; 0x1b8],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_exact_zero_ranges_words_and_flags() {
        let state = constructed();
        for offset in (0x78..0xc0).step_by(4) {
            assert_eq!(
                u32::from_le_bytes(state.bytes[offset..offset + 4].try_into().unwrap()),
                0,
                "{offset:#x}"
            );
        }
        assert_eq!(state.word(0x190), 0);
        for offset in (0x198..0x1b8).step_by(4) {
            assert_eq!(
                u32::from_le_bytes(state.bytes[offset..offset + 4].try_into().unwrap()),
                0,
                "{offset:#x}"
            );
        }
        assert_eq!(state.word(0x128), 0x3b83126f);
        assert_eq!(state.word(0x12c), 0x3b83126f);
        assert_eq!(state.bytes[0x11d], 1);
        assert_eq!(state.bytes[0x11e], 0);
        assert_eq!(state.word(0x120), 0x3fc90fdb);
        assert_eq!(state.bytes[0x125], 0);
        assert_eq!(state.bytes[0x124], 0);
        assert_eq!(state.bytes[0x11c], 0);
        assert_eq!(state.bytes[0x164], 0);
        // Untouched allocation survives outside the constructor writes.
        assert_eq!(state.bytes[0x00], 0xaa);
        assert_eq!(state.word(0x130), 0xaaaaaaaa);
    }

    #[test]
    fn construction_sound_defaults_match_original_strings() {
        let names = DoorSoundNames::construction_defaults();
        assert_eq!(&names.open[..12], b"d_woodop.wav");
        assert_eq!(&names.close[..12], b"d_woodcl.wav");
        assert_eq!(&names.locked[..12], b"d_woodlo.wav");
    }

    #[test]
    fn load_version_decodes_known_and_unknown_words() {
        assert_eq!(DoorLoadVersion::decode(5), DoorLoadVersion::V5);
        assert_eq!(DoorLoadVersion::decode(6), DoorLoadVersion::V6);
        assert_eq!(DoorLoadVersion::decode(7), DoorLoadVersion::V7);
        assert_eq!(DoorLoadVersion::decode(8), DoorLoadVersion::V8);
        assert_eq!(DoorLoadVersion::decode(0), DoorLoadVersion::Unknown);
        assert_eq!(DoorLoadVersion::decode(9), DoorLoadVersion::Unknown);
        assert_eq!(DoorLoadVersion::decode(u32::MAX), DoorLoadVersion::Unknown);
    }

    #[test]
    fn name_slots_terminate_and_truncate_like_native_buffers() {
        let mut state = constructed();
        state.put_load_name(0x130, b"d_woodop.wav");
        assert_eq!(state.open_sound(), b"d_woodop.wav");
        state.put_load_name(0x140, b"0123456789abcdefSURPLUS");
        assert_eq!(state.close_sound(), b"0123456789abcdef");
        // Untouched slots carry allocation bytes with no terminator, exactly
        // like the native buffers before any LoadData write.
        assert_eq!(state.locked_sound(), &[0xaa; 16]);
    }
}
