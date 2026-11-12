//! C_railway derived storage for the original PC actor lifecycle.
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor RVA0x865b0, Init0x868c0, LoadData0x86a20, destructor0x86760.
//! The original PS2 kind8 factory calls __ct__9C_railwayFv. Member names below
//! are descriptive PC projections; no PS2 member layout is assumed.
//!
//! PR3 parity (mafia-re `pr3` ba71d6e): `SaveData` 0x486920 writes the type
//! byte plus six words (+0x1a8/+0x1b0/+0x1ac/+0x1b4/+0x1b8/+0x1bc);
//! `LoadData` 0x486a20 reads them back. [`RailwayState::save_payload`] and
//! [`RailwayState::load_payload`] implement that round-trip over caller-owned
//! bytes.

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

    /// Original `C_railway::SaveData` 0x486920: type byte zero plus the six
    /// words in `load_payload` order. Returns the 25-byte payload; the caller
    /// owns the chunk framing.
    pub fn save_payload(&self) -> [u8; 25] {
        let mut out = [0u8; 25];
        out[0] = 0;
        for (i, offset) in [0x1a8, 0x1b0, 0x1ac, 0x1b4, 0x1b8, 0x1bc]
            .into_iter()
            .enumerate()
        {
            out[1 + i * 4..1 + (i + 1) * 4].copy_from_slice(&self.word(offset).to_le_bytes());
        }
        out
    }

    /// Material index read by `C_railway::GetMaterialID` (a materials.def
    /// index into the material list; descriptive name).
    pub fn material_word(&self) -> u32 {
        self.word(0x2ac)
    }

    pub(crate) fn set_material_word(&mut self, value: u32) {
        self.put_word(0x2ac, value);
    }

    /// Original `C_railway::DuplicateFrom`, GAME RVA 0x868f0 (VA 0x4868f0;
    /// mafia-re game/vehicles/C_railway.cpp:28-34 at pin 113b5b6): runs the
    /// base copy (caller-owned), then copies the two derived words
    /// (+0x1a8/+0x1b4). No other derived word is read.
    pub(crate) fn duplicate_from(&mut self, other: &Self) {
        for offset in [0x1a8, 0x1b4] {
            let word = other.word(offset);
            self.put_word(offset, word);
        }
    }
}

/// Extra save-game bytes written by `C_railway` past the base `C_actor`
/// payload (the save payload itself stays a `C_chunk` owner effect).
pub const RAILWAY_SAVE_GAME_EXTRA: i32 = 80;

/// Original `C_railway::SaveGameGetSize`, GAME RVA 0x96e40 (VA 0x496e40;
/// mafia-re game/vehicles/C_railway.cpp:101-105 at pin 113b5b6): the base
/// size plus 80. 32-bit wrapping matches the native `int` addition.
pub fn railway_save_game_size(base_size: i32) -> i32 {
    base_size.wrapping_add(RAILWAY_SAVE_GAME_EXTRA)
}

/// Original `C_railway::GetMaterialID`, GAME RVA 0x98a70 (VA 0x498a70;
/// mafia-re game/vehicles/C_railway.cpp:135-139 at pin 113b5b6): returns
/// the material word at +0x2ac, a materials.def index.
pub fn railway_material_id(state: &RailwayState) -> u32 {
    state.material_word()
}

/// Explicit boundary for the still-unimplemented small railway setters and
/// queries below. The entries name the original PC addresses (VA); the
/// frame, sound, collision and switch owners stay outside this contract, so
/// each reports the boundary instead of staging a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailwaySmallStubError {
    Unsupported { entry: u32 },
}

/// `C_railway::Lock` entry, GAME RVA 0x97980 (VA 0x497980; mafia-re
/// game/vehicles/C_railway.cpp:206-210 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): the native body only
/// logs, so no RVA beyond this entry is claimed.
pub const RAILWAY_LOCK_ENTRY: u32 = 0x497980;
/// `C_railway::Unlock` entry, GAME RVA 0x97990 (VA 0x497990; mafia-re
/// game/vehicles/C_railway.cpp:180-184 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): the native body only
/// logs, so no RVA beyond this entry is claimed.
pub const RAILWAY_UNLOCK_ENTRY: u32 = 0x497990;
/// `C_railway::GetVagon` entry, GAME RVA 0x96e10 (VA 0x496e10; mafia-re
/// game/vehicles/C_railway.cpp:199-204 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): the native body only
/// logs and returns NULL, so no RVA beyond this entry is claimed.
pub const RAILWAY_GET_VAGON_ENTRY: u32 = 0x496e10;
/// `C_railway::GetEntry` entry, GAME RVA 0x95d30 (VA 0x495d30; mafia-re
/// game/vehicles/C_railway.cpp:192-197 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): the native body only
/// logs and returns false, so no RVA beyond this entry is claimed.
pub const RAILWAY_GET_ENTRY_ENTRY: u32 = 0x495d30;
/// `C_railway::SetRainDummy` entry, GAME RVA 0x989a0 (VA 0x4989a0; mafia-re
/// game/vehicles/C_railway.cpp:186-190 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): sets the rain dummy.
pub const RAILWAY_SET_RAIN_DUMMY_ENTRY: u32 = 0x4989a0;
/// `C_railway::AddOuterEntries` entry, GAME RVA 0x95100 (VA 0x495100; mafia-re
/// game/vehicles/C_railway.cpp:156-160 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): registers the outer
/// entries.
pub const RAILWAY_ADD_OUTER_ENTRIES_ENTRY: u32 = 0x495100;
/// `C_railway::AddInnerEntries` entry, GAME RVA 0x95160 (VA 0x495160; mafia-re
/// game/vehicles/C_railway.cpp:162-166 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): registers the inner
/// entries.
pub const RAILWAY_ADD_INNER_ENTRIES_ENTRY: u32 = 0x495160;
/// `C_railway::RemoveOuterEntries` entry, GAME RVA 0x951c0 (VA 0x4951c0;
/// mafia-re game/vehicles/C_railway.cpp:168-172 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): unregisters
/// the outer entries.
pub const RAILWAY_REMOVE_OUTER_ENTRIES_ENTRY: u32 = 0x4951c0;
/// `C_railway::RemoveInnerEntries` entry, GAME RVA 0x95220 (VA 0x495220;
/// mafia-re game/vehicles/C_railway.cpp:174-178 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): unregisters
/// the inner entries.
pub const RAILWAY_REMOVE_INNER_ENTRIES_ENTRY: u32 = 0x495220;
/// `C_railway::ChangeState` entry, GAME RVA 0x98060 (VA 0x498060; mafia-re
/// game/vehicles/C_railway.cpp:124-129 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): delegates to the base
/// `C_actor::ChangeState`.
pub const RAILWAY_CHANGE_STATE_ENTRY: u32 = 0x498060;

/// Original `C_railway::Lock`, GAME RVA 0x97980 (VA 0x497980; mafia-re
/// game/vehicles/C_railway.cpp:206-210 at pin fe7e746): the original only
/// logs (STUB: GAME in Game.exe). No lock owner exists here, so the
/// boundary is explicit.
pub fn railway_lock() -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_LOCK_ENTRY,
    })
}

/// Original `C_railway::Unlock`, GAME RVA 0x97990 (VA 0x497990; mafia-re
/// game/vehicles/C_railway.cpp:180-184 at pin fe7e746): the original only
/// logs (STUB: GAME in Game.exe). No lock owner exists here, so the
/// boundary is explicit.
pub fn railway_unlock() -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_UNLOCK_ENTRY,
    })
}

/// Original `C_railway::GetVagon` stub (mafia-re
/// game/vehicles/C_railway.cpp:199-204 at pin fe7e746; `STUB: GAME` VA
/// 0x496e10, RVA 0x96e10): the original only logs and returns NULL. The
/// wagon-frame owner stays outside this contract, so the boundary is
/// explicit; the native frame answer is an opaque identity that is never
/// dereferenced.
pub fn railway_get_vagon(_wagon: u32) -> Result<u32, RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_GET_VAGON_ENTRY,
    })
}

/// Original `C_railway::GetEntry` stub (mafia-re
/// game/vehicles/C_railway.cpp:192-197 at pin fe7e746; `STUB: GAME` VA
/// 0x495d30, RVA 0x95d30): the original only logs and returns false. The
/// actor/frame/placement owners stay outside this contract, so the boundary
/// is explicit; the native `bool` answer is preserved in the return type.
/// The actor identity is opaque and never dereferenced.
pub fn railway_get_entry(_actor: u32) -> Result<bool, RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_GET_ENTRY_ENTRY,
    })
}

/// Original `C_railway::SetRainDummy` stub (mafia-re
/// game/vehicles/C_railway.cpp:186-190 at pin fe7e746; `STUB: GAME` VA
/// 0x4989a0, RVA 0x989a0): the original only logs. The frame owner stays
/// outside this contract, so the boundary is explicit. The frame identity is
/// opaque and never dereferenced.
pub fn railway_set_rain_dummy(_inside: bool, _frame: u32) -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_SET_RAIN_DUMMY_ENTRY,
    })
}

/// Original `C_railway::AddOuterEntries` stub (mafia-re
/// game/vehicles/C_railway.cpp:156-160 at pin fe7e746; `STUB: GAME` VA
/// 0x495100, RVA 0x95100): the original only logs. The sound/entry owners
/// stay outside this contract, so the boundary is explicit.
pub fn railway_add_outer_entries() -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_ADD_OUTER_ENTRIES_ENTRY,
    })
}

/// Original `C_railway::AddInnerEntries` stub (mafia-re
/// game/vehicles/C_railway.cpp:162-166 at pin fe7e746; `STUB: GAME` VA
/// 0x495160, RVA 0x95160): the original only logs. The sound/entry owners
/// stay outside this contract, so the boundary is explicit.
pub fn railway_add_inner_entries() -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_ADD_INNER_ENTRIES_ENTRY,
    })
}

/// Original `C_railway::RemoveOuterEntries` stub (mafia-re
/// game/vehicles/C_railway.cpp:168-172 at pin fe7e746; `STUB: GAME` VA
/// 0x4951c0, RVA 0x951c0): the original only logs. The sound/entry owners
/// stay outside this contract, so the boundary is explicit.
pub fn railway_remove_outer_entries() -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_REMOVE_OUTER_ENTRIES_ENTRY,
    })
}

/// Original `C_railway::RemoveInnerEntries` stub (mafia-re
/// game/vehicles/C_railway.cpp:174-178 at pin fe7e746; `STUB: GAME` VA
/// 0x495220, RVA 0x95220): the original only logs. The sound/entry owners
/// stay outside this contract, so the boundary is explicit.
pub fn railway_remove_inner_entries() -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_REMOVE_INNER_ENTRIES_ENTRY,
    })
}

/// Original `C_railway::ChangeState` stub (mafia-re
/// game/vehicles/C_railway.cpp:124-129 at pin fe7e746; `STUB: GAME` VA
/// 0x498060, RVA 0x98060): the original only logs and runs the base
/// `C_actor::ChangeState`. The base state owner stays outside this contract,
/// so the boundary is explicit.
pub fn railway_change_state() -> Result<(), RailwaySmallStubError> {
    Err(RailwaySmallStubError::Unsupported {
        entry: RAILWAY_CHANGE_STATE_ENTRY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> RailwayState {
        RailwayState::allocation(RailwaySeed {
            bytes_70_36f: [0xaa; 0x300],
        })
    }

    #[test]
    fn save_payload_round_trips_six_words() {
        let mut value = state();
        let words = [
            (0x1a8, 0x11111111u32),
            (0x1b0, 0x22222222),
            (0x1ac, 0x33333333),
            (0x1b4, 0x44444444),
            (0x1b8, 0x55555555),
            (0x1bc, 0x66666666),
        ];
        for (offset, word) in words {
            value.put_word(offset, word);
        }
        let saved = value.save_payload();
        assert_eq!(saved[0], 0);
        let mut restored = state();
        let consumed = restored.load_payload(&saved);
        assert_eq!(consumed, 25);
        for (offset, word) in words {
            assert_eq!(restored.word(offset), word, "{offset:#x}");
        }
    }

    #[test]
    fn load_payload_skips_missing_tail_without_advancing() {
        let mut value = state();
        let before = value.word(0x1bc);
        let consumed = value.load_payload(&[0u8; 1 + 5 * 4]);
        assert_eq!(consumed, 1 + 5 * 4);
        assert_eq!(value.word(0x1bc), before);
    }

    #[test]
    fn rva_868f0_duplicate_from_copies_two_words_only() {
        // C_railway::DuplicateFrom GAME RVA 0x868f0 (VA 0x4868f0).
        let mut source = state();
        source.put_word(0x1a8, 0x11111111);
        source.put_word(0x1b4, 0x22222222);
        source.put_word(0x1ac, 0x33333333);
        let mut target = state();
        target.duplicate_from(&source);
        assert_eq!(target.word(0x1a8), 0x11111111);
        assert_eq!(target.word(0x1b4), 0x22222222);
        // +0x1ac is saved/loaded but not duplicated: the untouched seed stays.
        assert_eq!(target.word(0x1ac), 0xaaaaaaaa);
    }

    #[test]
    fn rva_96e40_save_game_size_adds_eighty() {
        // C_railway::SaveGameGetSize GAME RVA 0x96e40 (VA 0x496e40).
        assert_eq!(RAILWAY_SAVE_GAME_EXTRA, 80);
        assert_eq!(railway_save_game_size(0), 80);
        assert_eq!(railway_save_game_size(100), 180);
        assert_eq!(railway_save_game_size(i32::MAX), i32::MIN.wrapping_add(79));
    }

    #[test]
    fn rva_98a70_get_material_id_reads_material_word() {
        // C_railway::GetMaterialID GAME RVA 0x98a70 (VA 0x498a70).
        let mut value = state();
        value.set_material_word(7);
        assert_eq!(railway_material_id(&value), 7);
        assert_eq!(value.material_word(), 7);
        value.set_material_word(0);
        assert_eq!(railway_material_id(&value), 0);
    }

    #[test]
    fn rva_97980_lock_reports_unsupported() {
        // C_railway::Lock GAME RVA 0x97980 (VA 0x497980).
        assert_eq!(RAILWAY_LOCK_ENTRY, 0x497980);
        assert_eq!(
            railway_lock(),
            Err(RailwaySmallStubError::Unsupported { entry: 0x497980 })
        );
    }

    #[test]
    fn rva_97990_unlock_reports_unsupported() {
        // C_railway::Unlock GAME RVA 0x97990 (VA 0x497990).
        assert_eq!(RAILWAY_UNLOCK_ENTRY, 0x497990);
        assert_eq!(
            railway_unlock(),
            Err(RailwaySmallStubError::Unsupported { entry: 0x497990 })
        );
    }

    #[test]
    fn rva_96e10_get_vagon_reports_unsupported() {
        // C_railway::GetVagon GAME RVA 0x96e10 (VA 0x496e10).
        assert_eq!(RAILWAY_GET_VAGON_ENTRY, 0x496e10);
        assert_eq!(
            railway_get_vagon(0),
            Err(RailwaySmallStubError::Unsupported { entry: 0x496e10 })
        );
        assert_eq!(
            railway_get_vagon(1),
            Err(RailwaySmallStubError::Unsupported { entry: 0x496e10 })
        );
    }

    #[test]
    fn rva_95d30_get_entry_reports_unsupported() {
        // C_railway::GetEntry GAME RVA 0x95d30 (VA 0x495d30).
        assert_eq!(RAILWAY_GET_ENTRY_ENTRY, 0x495d30);
        assert_eq!(
            railway_get_entry(0x1234),
            Err(RailwaySmallStubError::Unsupported { entry: 0x495d30 })
        );
    }

    #[test]
    fn rva_989a0_set_rain_dummy_reports_unsupported() {
        // C_railway::SetRainDummy GAME RVA 0x989a0 (VA 0x4989a0).
        assert_eq!(RAILWAY_SET_RAIN_DUMMY_ENTRY, 0x4989a0);
        assert_eq!(
            railway_set_rain_dummy(true, 0x1234),
            Err(RailwaySmallStubError::Unsupported { entry: 0x4989a0 })
        );
        assert_eq!(
            railway_set_rain_dummy(false, 0),
            Err(RailwaySmallStubError::Unsupported { entry: 0x4989a0 })
        );
    }

    #[test]
    fn rva_95100_add_outer_entries_reports_unsupported() {
        // C_railway::AddOuterEntries GAME RVA 0x95100 (VA 0x495100).
        assert_eq!(RAILWAY_ADD_OUTER_ENTRIES_ENTRY, 0x495100);
        assert_eq!(
            railway_add_outer_entries(),
            Err(RailwaySmallStubError::Unsupported { entry: 0x495100 })
        );
    }

    #[test]
    fn rva_95160_add_inner_entries_reports_unsupported() {
        // C_railway::AddInnerEntries GAME RVA 0x95160 (VA 0x495160).
        assert_eq!(RAILWAY_ADD_INNER_ENTRIES_ENTRY, 0x495160);
        assert_eq!(
            railway_add_inner_entries(),
            Err(RailwaySmallStubError::Unsupported { entry: 0x495160 })
        );
    }

    #[test]
    fn rva_951c0_remove_outer_entries_reports_unsupported() {
        // C_railway::RemoveOuterEntries GAME RVA 0x951c0 (VA 0x4951c0).
        assert_eq!(RAILWAY_REMOVE_OUTER_ENTRIES_ENTRY, 0x4951c0);
        assert_eq!(
            railway_remove_outer_entries(),
            Err(RailwaySmallStubError::Unsupported { entry: 0x4951c0 })
        );
    }

    #[test]
    fn rva_95220_remove_inner_entries_reports_unsupported() {
        // C_railway::RemoveInnerEntries GAME RVA 0x95220 (VA 0x495220).
        assert_eq!(RAILWAY_REMOVE_INNER_ENTRIES_ENTRY, 0x495220);
        assert_eq!(
            railway_remove_inner_entries(),
            Err(RailwaySmallStubError::Unsupported { entry: 0x495220 })
        );
    }

    #[test]
    fn rva_98060_change_state_reports_unsupported() {
        // C_railway::ChangeState GAME RVA 0x98060 (VA 0x498060).
        assert_eq!(RAILWAY_CHANGE_STATE_ENTRY, 0x498060);
        assert_eq!(
            railway_change_state(),
            Err(RailwaySmallStubError::Unsupported { entry: 0x498060 })
        );
    }
}
