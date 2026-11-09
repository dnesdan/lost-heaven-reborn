//! C_traffic_car derived storage for the original PC actor lifecycle (kind 12).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x4aa7a0, Init0x4aa860, LoadData0x4aad20,
//! vtable VA0x624c98, 0x26c bytes (PS2's 0x290). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! Member names are descriptive PC projections; no PS2 member layout is assumed.
//!
//! PR3 parity (mafia-re `pr3`, commits 32b1b33..b0c02ef): constructor stores,
//! `DecrementCount` 0x4b9560, `SaveGameSave` 0x4bc160 / `SaveGameLoad`
//! 0x4bc190 (u16 tag 1), `SaveData` 0x4aabf0 (version 5 + three tuning
//! words + field 0x21c + counted 0x20-byte names). `RemoveAllCars` 0x4bad80
//! and `RemoveCar` 0x4bbe90 remain unimplemented stubs; the previous vulgar
//! working names are retired. `LoadData` 0x4aad20 remains a stub. Static
//! coefficients live in [`TrafficTuning`]; the static traffic count lives in
//! [`TrafficCarCount`].

/// Actual untouched allocation bytes of one 0x26c-byte traffic-car object.
/// Pointer words overwritten by construction are not promoted into owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrafficCarSeed {
    pub bytes_00_26b: [u8; 0x26c],
}

/// Owned traffic-car bytes after construction. Words the constructor stores
/// are observable state, not interpreted native pointers: their only admitted
/// readers are the documented Init/LoadData offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrafficCarState {
    bytes: [u8; 0x26c],
    elements: Vec<u8>,
}

impl TrafficCarState {
    pub(crate) fn allocation(seed: TrafficCarSeed) -> Self {
        Self {
            bytes: seed.bytes_00_26b,
            elements: Vec::new(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Retained per-element records (0x20 bytes each): a NUL-terminated name
    /// plus flag words. Meanings are uninterpreted; the bytes round-trip for
    /// a future effect owner.
    pub fn elements(&self) -> &[u8] {
        &self.elements
    }

    pub fn element_count(&self) -> usize {
        self.elements.len() / 0x20
    }

    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("traffic word"),
        )
    }

    /// Original constructor body in dependency order: base C_actor
    /// construction runs first (owned by the caller through construct_base),
    /// then the zeroed words and the three float words (180.0/150.0/170.0 at
    /// +0x224/+0x228/+0x22c). The three unit floats go to shared game-global
    /// storage (see [`TrafficTuning`]).
    pub(crate) fn complete_constructor(&mut self) {
        for offset in [0x220, 0x21c, 0x234, 0x238] {
            self.put_word(offset, 0);
        }
        self.put_word(0x224, 0x43340000);
        self.put_word(0x228, 0x43160000);
        self.put_word(0x22c, 0x432a0000);
    }

    pub(crate) fn put_load_word(&mut self, offset: usize, value: u32) {
        self.put_word(offset, value);
    }

    /// Speed word at +0x224 selected by every LoadData version.
    pub fn speed_word(&self) -> u32 {
        self.word(0x224)
    }

    /// Element-count word at +0x238 read by every LoadData version.
    pub fn count_word(&self) -> u32 {
        self.word(0x238)
    }

    pub(crate) fn replace_elements(&mut self, elements: Vec<u8>) {
        self.elements = elements;
    }
}

/// Shared traffic tuning words written by traffic-car construction (three unit
/// floats) and Init (one word copy). Later writes overwrite earlier ones,
/// matching the native static storage; traffic effects (unimplemented) read
/// these back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrafficTuning {
    pub unit_floats: [u32; 3],
    pub copy_source: u32,
    pub copy_target: u32,
}

impl TrafficTuning {
    pub(crate) fn construction_defaults() -> Self {
        Self {
            unit_floats: [0x3f800000, 0x3f800000, 0x3f800000],
            copy_source: 0,
            copy_target: 0,
        }
    }

    pub(crate) fn init_copy(&mut self) {
        self.copy_target = self.copy_source;
    }

    /// Element coefficient at PC 0x63d7fc (`elems_coefff`, PS2 name).
    pub fn elems_coeff_bits(&self) -> u32 {
        self.unit_floats[0]
    }

    /// Mafia coefficient at PC 0x63d800 (`mafian_coefff`, PS2 name).
    pub fn mafian_coeff_bits(&self) -> u32 {
        self.unit_floats[1]
    }

    /// Police coefficient at PC 0x63d804 (`police_coefff`, PS2 name).
    pub fn police_coeff_bits(&self) -> u32 {
        self.unit_floats[2]
    }
}

/// Static traffic count at PC 0x63d810 (`count`, descriptive name).
/// `DecrementCount` 0x4b9560 decrements and clamps at zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficCarCount(pub i32);

impl TrafficCarCount {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i32 {
        self.0
    }

    /// Original `C_traffic_car::DecrementCount`: `--count; if (count < 0)
    /// count = 0;`.
    pub fn decrement(&mut self) {
        self.0 = self.0.wrapping_sub(1);
        if self.0 < 0 {
            self.0 = 0;
        }
    }
}

/// Save-game tag for `C_traffic_car::SaveGameSave` 0x4bc160 /
/// `SaveGameLoad` 0x4bc190: u16 value 1 after the base `C_actor` payload.
pub const TRAFFIC_SAVEGAME_TAG: u16 = 1;

/// Save-game tag mismatch, preserving the observed tag bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficSaveGameError {
    Truncated { expected: usize, observed: usize },
    UnexpectedTag { expected: u16, observed: u16 },
}

pub fn encode_traffic_savegame_tag() -> [u8; 2] {
    TRAFFIC_SAVEGAME_TAG.to_le_bytes()
}

pub fn decode_traffic_savegame_tag(bytes: &[u8]) -> Result<(), TrafficSaveGameError> {
    if bytes.len() < 2 {
        return Err(TrafficSaveGameError::Truncated {
            expected: 2,
            observed: bytes.len(),
        });
    }
    let observed = u16::from_le_bytes([bytes[0], bytes[1]]);
    if observed != TRAFFIC_SAVEGAME_TAG {
        return Err(TrafficSaveGameError::UnexpectedTag {
            expected: TRAFFIC_SAVEGAME_TAG,
            observed,
        });
    }
    Ok(())
}

/// Name record size for `SaveData` 0x4aabf0 (`TRAFFIC_NAME_SIZE`, mafia-re name).
pub const TRAFFIC_NAME_SIZE: usize = 0x20;
/// Version word written by `SaveData` 0x4aabf0.
pub const TRAFFIC_SAVEDATA_VERSION: u32 = 5;

/// Savegame payload of `C_traffic_car::SaveData`: version 5, the three
/// tuning words (+0x224/+0x228/+0x22c), field +0x21c, the name count and the
/// counted `TRAFFIC_NAME_SIZE` records. Offsets name PC derived fields;
/// values stay opaque words except for the counted layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrafficSaveData {
    pub tuning_224: u32,
    pub tuning_228: u32,
    pub tuning_22c: u32,
    pub field_21c: u32,
    pub names: Vec<[u8; TRAFFIC_NAME_SIZE]>,
}

impl TrafficSaveData {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(20 + self.names.len() * TRAFFIC_NAME_SIZE);
        out.extend_from_slice(&TRAFFIC_SAVEDATA_VERSION.to_le_bytes());
        out.extend_from_slice(&self.tuning_224.to_le_bytes());
        out.extend_from_slice(&self.tuning_228.to_le_bytes());
        out.extend_from_slice(&self.tuning_22c.to_le_bytes());
        out.extend_from_slice(&self.field_21c.to_le_bytes());
        let count = u32::try_from(self.names.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        for name in &self.names {
            out.extend_from_slice(name);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), TrafficSaveGameError> {
        if bytes.len() < 24 {
            return Err(TrafficSaveGameError::Truncated {
                expected: 24,
                observed: bytes.len(),
            });
        }
        let version = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if version != TRAFFIC_SAVEDATA_VERSION {
            return Err(TrafficSaveGameError::UnexpectedTag {
                expected: TRAFFIC_SAVEDATA_VERSION as u16,
                observed: version as u16,
            });
        }
        let tuning_224 = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let tuning_228 = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let tuning_22c = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let field_21c = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let count = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
        let needed = 24usize.saturating_add(count.saturating_mul(TRAFFIC_NAME_SIZE));
        if bytes.len() < needed {
            return Err(TrafficSaveGameError::Truncated {
                expected: needed,
                observed: bytes.len(),
            });
        }
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let start = 24 + i * TRAFFIC_NAME_SIZE;
            let mut record = [0u8; TRAFFIC_NAME_SIZE];
            record.copy_from_slice(&bytes[start..start + TRAFFIC_NAME_SIZE]);
            names.push(record);
        }
        Ok((
            Self {
                tuning_224,
                tuning_228,
                tuning_22c,
                field_21c,
                names,
            },
            needed,
        ))
    }
}

/// Still-unimplemented traffic-list removals. The entries name the original
/// PC addresses; the vulgar working names are retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficRemovalError {
    Unsupported { entry: u32 },
}

/// `C_traffic_car::RemoveAllCars` 0x4bad80: removes every active car.
/// Unimplemented; preserved as an explicit boundary.
pub const REMOVE_ALL_CARS_ENTRY: u32 = 0x4bad80;
/// `C_traffic_car::RemoveCar` 0x4bbe90: removes one actor's traffic car.
/// Unimplemented; preserved as an explicit boundary.
pub const REMOVE_CAR_ENTRY: u32 = 0x4bbe90;

pub fn remove_all_cars() -> Result<(), TrafficRemovalError> {
    Err(TrafficRemovalError::Unsupported {
        entry: REMOVE_ALL_CARS_ENTRY,
    })
}

pub fn remove_car(_actor: u32) -> Result<(), TrafficRemovalError> {
    Err(TrafficRemovalError::Unsupported {
        entry: REMOVE_CAR_ENTRY,
    })
}

/// One `C_traffic_car::S_ELEMENT` list slot (mafia-re game/vehicles/C_trf_c.h:56-61
/// at pin 113b5b6). Only the frame and car identities are admitted: both are
/// opaque native identities, never dereferenced on the host. The surrounding
/// unknown bytes carry no interpreted meaning and stay outside this contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficListSlot {
    pub frame: u32,
    pub car: u32,
}

/// Owner of the two traffic-car lists: the parked cars (`v_elems_parking`,
/// GAME 0x63d834) and the traffic's cars (`v_elems`, GAME 0x63d844). Native
/// statics start empty; growth/shrink order is the caller's, this only owns
/// the slots the accessors below publish.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrafficCarLists {
    parking: Vec<TrafficListSlot>,
    elems: Vec<TrafficListSlot>,
}

impl TrafficCarLists {
    pub fn parking_cars(&self) -> &[TrafficListSlot] {
        &self.parking
    }

    pub fn parking_cars_mut(&mut self) -> &mut Vec<TrafficListSlot> {
        &mut self.parking
    }

    pub fn elems(&self) -> &[TrafficListSlot] {
        &self.elems
    }

    pub fn elems_mut(&mut self) -> &mut Vec<TrafficListSlot> {
        &mut self.elems
    }
}

/// Original `C_traffic_car::GetParkingCars`, GAME RVA 0xb8540
/// (VA 0x4b8540; mafia-re game/vehicles/C_trf_c.cpp:29-33 at pin 113b5b6):
/// returns the parked-car list `v_elems_parking`, which
/// `C_traffic_generator::TryGoToAssHole` sends pedestrians to.
pub fn get_parking_cars(lists: &TrafficCarLists) -> &[TrafficListSlot] {
    lists.parking_cars()
}

/// Original `C_traffic_car::GetElems`, GAME RVA 0xa9ca0 (VA 0x4a9ca0;
/// mafia-re game/vehicles/C_trf_c.h:82-87 at pin 113b5b6): returns the
/// traffic's car list `v_elems`.
pub fn get_elems(lists: &TrafficCarLists) -> &[TrafficListSlot] {
    lists.elems()
}

/// Original `C_traffic_car::vfunc_38`, GAME RVA 0xab120 (VA 0x4ab120;
/// mafia-re game/vehicles/C_trf_c.cpp:106-110 at pin 113b5b6): the last
/// vtable slot; the body only returns false and reads no argument.
pub fn traffic_car_vfunc_38(_args: [i32; 12]) -> bool {
    false
}

/// Extra save-game bytes written by `C_traffic_car::SaveGameSave` past the
/// base `C_actor` payload (the u16 tag handled by
/// [`encode_traffic_savegame_tag`]).
pub const TRAFFIC_SAVE_GAME_EXTRA: i32 = 2;

/// Original `C_traffic_car::SaveGameGetSize`, GAME RVA 0xbc150
/// (VA 0x4bc150; mafia-re game/vehicles/C_trf_c.cpp:160-164 at pin 113b5b6):
/// the base size plus 2. 32-bit wrapping matches the native `int` addition.
pub fn traffic_car_save_game_size(base_size: i32) -> i32 {
    base_size.wrapping_add(TRAFFIC_SAVE_GAME_EXTRA)
}

/// `C_traffic_car::Corovacka` entry, GAME RVA 0xb8480 (VA 0x4b8480;
/// mafia-re game/vehicles/C_trf_c.cpp:180-185 at pin 113b5b6): the model of
/// a traffic car's driver the player throws out of the car (NULL for a car
/// not of the traffic's). The native body is an unwritten STUB returning
/// NULL; the driver hand-off owner stays outside this contract, so the stub
/// is an explicit boundary reusing [`TrafficRemovalError`].
pub const COROVACKA_ENTRY: u32 = 0x4b8480;

/// Original `C_traffic_car::Corovacka` stub: always reports the boundary,
/// mirroring the native NULL (no driver model is handed out).
pub fn corovacka(_car: u32) -> Result<(), TrafficRemovalError> {
    Err(TrafficRemovalError::Unsupported {
        entry: COROVACKA_ENTRY,
    })
}

/// Still-unported base-delegating traffic-car overrides. The entries name
/// the original PC addresses; the `C_actor` base, scene/device, chunk and
/// collision owners stay outside this contract, so each stays an explicit
/// boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficOverrideError {
    Unsupported { entry: u32 },
}

/// `C_traffic_car::AI` 0x4afc40: AI tick, base-delegating. Unimplemented;
/// preserved as an explicit boundary.
pub const TRAFFIC_AI_ENTRY: u32 = 0x4afc40;
/// `C_traffic_car::Update` 0x4b5270: per-frame update, base-delegating.
/// Unimplemented; preserved as an explicit boundary.
pub const TRAFFIC_UPDATE_ENTRY: u32 = 0x4b5270;
/// `C_traffic_car::Hit` 0x4baad0: collision hit response, base-delegating.
/// Unimplemented; preserved as an explicit boundary.
pub const TRAFFIC_HIT_ENTRY: u32 = 0x4baad0;
/// `C_traffic_car::IsPossibleColl` 0x4ba9c0: collision query,
/// base-delegating. Unimplemented; preserved as an explicit boundary.
pub const TRAFFIC_IS_POSSIBLE_COLL_ENTRY: u32 = 0x4ba9c0;
/// `C_traffic_car::LoadData` 0x4aad20: load payload, base-delegating.
/// Unimplemented; preserved as an explicit boundary.
pub const TRAFFIC_LOAD_DATA_ENTRY: u32 = 0x4aad20;

/// Original `C_traffic_car::AI` (GAME VA 0x4afc40, RVA 0xafc40; mafia-re
/// game/vehicles/C_trf_c.cpp:117-122 at pin 113b5b6): logs and runs the
/// `C_actor` base tick. The base/scene owners stay outside this contract,
/// so the boundary is explicit.
pub fn traffic_ai(_time_ms: u32) -> Result<(), TrafficOverrideError> {
    Err(TrafficOverrideError::Unsupported {
        entry: TRAFFIC_AI_ENTRY,
    })
}

/// Original `C_traffic_car::Update` (GAME VA 0x4b5270, RVA 0xb5270;
/// mafia-re game/vehicles/C_trf_c.cpp:124-129 at pin 113b5b6): logs and
/// runs the `C_actor` base update. The base/scene owners stay outside this
/// contract, so the boundary is explicit.
pub fn traffic_update(_time_ms: u32) -> Result<(), TrafficOverrideError> {
    Err(TrafficOverrideError::Unsupported {
        entry: TRAFFIC_UPDATE_ENTRY,
    })
}

/// Original `C_traffic_car::Hit` (GAME VA 0x4baad0, RVA 0xbaad0; mafia-re
/// game/vehicles/C_trf_c.cpp:152-158 at pin 113b5b6): logs and runs the
/// `C_actor` base hit response, returning its answer. The actor/vector,
/// frame and collision owners stay outside this contract, so the boundary
/// is explicit; the native `bool` answer is preserved in the return type.
pub fn traffic_hit() -> Result<bool, TrafficOverrideError> {
    Err(TrafficOverrideError::Unsupported {
        entry: TRAFFIC_HIT_ENTRY,
    })
}

/// Original `C_traffic_car::IsPossibleColl` (GAME VA 0x4ba9c0,
/// RVA 0xba9c0; mafia-re game/vehicles/C_trf_c.cpp:145-150 at pin
/// 113b5b6): logs and runs the `C_actor` base collision query, returning
/// its answer. The actor/vector and collision owners stay outside this
/// contract, so the boundary is explicit; the native `bool` answer is
/// preserved in the return type.
pub fn traffic_is_possible_coll() -> Result<bool, TrafficOverrideError> {
    Err(TrafficOverrideError::Unsupported {
        entry: TRAFFIC_IS_POSSIBLE_COLL_ENTRY,
    })
}

/// Original `C_traffic_car::LoadData` (GAME VA 0x4aad20, RVA 0xaad20;
/// mafia-re game/vehicles/C_trf_c.cpp:99-104 at pin 113b5b6): logs and
/// runs the `C_actor` base load, returning its answer. The chunk owner
/// stays outside this contract, so the boundary is explicit; the native
/// `bool` answer is preserved in the return type.
pub fn traffic_load_data() -> Result<bool, TrafficOverrideError> {
    Err(TrafficOverrideError::Unsupported {
        entry: TRAFFIC_LOAD_DATA_ENTRY,
    })
}

impl TrafficCarState {
    /// Original `C_traffic_car::DuplicateFrom`, GAME RVA 0xaab20
    /// (VA 0x4aab20; mafia-re game/vehicles/C_trf_c.cpp:73-90 at pin
    /// 113b5b6): runs the base copy (caller-owned), then copies the four
    /// derived words (+0x21c/+0x224/+0x228/+0x22c) and the counted
    /// `TRAFFIC_NAME_SIZE` name records. The host clones the admitted
    /// element bytes verbatim; native `strcpy` tail bytes past each NUL are
    /// uninitialized heap garbage and stay untracked.
    pub(crate) fn duplicate_from(&mut self, other: &Self) {
        for offset in [0x21c, 0x224, 0x228, 0x22c] {
            let word = other.word(offset);
            self.put_word(offset, word);
        }
        self.replace_elements(other.elements.clone());
    }
}

/// Payload versions admitted by C_traffic_car::LoadData. Versions 4 and 5
/// share the four-word header plus the counted 0x20-byte element array; any
/// other version consumes only its own version word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficLoadVersion {
    V4,
    V5,
    Unknown,
}

impl TrafficLoadVersion {
    pub fn decode(word: u32) -> Self {
        match word {
            4 => Self::V4,
            5 => Self::V5,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> TrafficCarState {
        let mut state = TrafficCarState::allocation(TrafficCarSeed {
            bytes_00_26b: [0xaa; 0x26c],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_exact_zero_and_float_words() {
        let state = constructed();
        for offset in [0x220, 0x21c, 0x234, 0x238] {
            assert_eq!(state.word(offset), 0, "{offset:#x}");
        }
        assert_eq!(state.word(0x224), 0x43340000);
        assert_eq!(state.word(0x228), 0x43160000);
        assert_eq!(state.word(0x22c), 0x432a0000);
        assert_eq!(
            f32::from_le_bytes(state.bytes[0x224..0x228].try_into().unwrap()),
            180.0
        );
        assert_eq!(
            f32::from_le_bytes(state.bytes[0x228..0x22c].try_into().unwrap()),
            150.0
        );
        assert_eq!(
            f32::from_le_bytes(state.bytes[0x22c..0x230].try_into().unwrap()),
            170.0
        );
        assert_eq!(state.elements(), &[]);
        // Untouched allocation survives outside the constructor writes.
        assert_eq!(state.bytes[0x00], 0xaa);
    }

    #[test]
    fn tuning_defaults_match_original_constant_writes() {
        let tuning = TrafficTuning::construction_defaults();
        assert_eq!(tuning.unit_floats, [0x3f800000; 3]);
        let mut tuning = tuning;
        tuning.copy_source = 0x12345678;
        tuning.init_copy();
        assert_eq!(tuning.copy_target, 0x12345678);
    }

    #[test]
    fn load_version_decodes_known_and_unknown_words() {
        assert_eq!(TrafficLoadVersion::decode(4), TrafficLoadVersion::V4);
        assert_eq!(TrafficLoadVersion::decode(5), TrafficLoadVersion::V5);
        assert_eq!(TrafficLoadVersion::decode(0), TrafficLoadVersion::Unknown);
        assert_eq!(TrafficLoadVersion::decode(6), TrafficLoadVersion::Unknown);
        assert_eq!(
            TrafficLoadVersion::decode(u32::MAX),
            TrafficLoadVersion::Unknown
        );
    }

    #[test]
    fn element_accounting_counts_whole_records() {
        let mut state = constructed();
        assert_eq!(state.element_count(), 0);
        state.replace_elements(vec![0u8; 0x40]);
        assert_eq!(state.element_count(), 2);
        assert_eq!(state.elements().len(), 0x40);
    }

    #[test]
    fn tuning_coefficients_map_to_pr3_static_order() {
        let tuning = TrafficTuning::construction_defaults();
        assert_eq!(tuning.elems_coeff_bits(), 0x3f800000);
        assert_eq!(tuning.mafian_coeff_bits(), 0x3f800000);
        assert_eq!(tuning.police_coeff_bits(), 0x3f800000);
    }

    #[test]
    fn decrement_count_clamps_at_zero() {
        let mut count = TrafficCarCount::new(2);
        count.decrement();
        assert_eq!(count.value(), 1);
        count.decrement();
        assert_eq!(count.value(), 0);
        count.decrement();
        assert_eq!(count.value(), 0);
        let mut negative = TrafficCarCount::new(-5);
        negative.decrement();
        assert_eq!(negative.value(), 0);
    }

    #[test]
    fn savegame_tag_round_trips_and_rejects_mismatch() {
        assert_eq!(encode_traffic_savegame_tag(), [1, 0]);
        assert_eq!(decode_traffic_savegame_tag(&[1, 0]), Ok(()));
        assert_eq!(
            decode_traffic_savegame_tag(&[2, 0]),
            Err(TrafficSaveGameError::UnexpectedTag {
                expected: 1,
                observed: 2
            })
        );
        assert_eq!(
            decode_traffic_savegame_tag(&[1]),
            Err(TrafficSaveGameError::Truncated {
                expected: 2,
                observed: 1
            })
        );
    }

    #[test]
    fn savedata_round_trips_names_and_rejects_truncation() {
        let mut first = [0u8; TRAFFIC_NAME_SIZE];
        first[..4].copy_from_slice(b"taxi");
        let payload = TrafficSaveData {
            tuning_224: 0x43340000,
            tuning_228: 0x43160000,
            tuning_22c: 0x432a0000,
            field_21c: 7,
            names: vec![first, [0xabu8; TRAFFIC_NAME_SIZE]],
        };
        let bytes = payload.encode();
        assert_eq!(bytes.len(), 24 + 2 * TRAFFIC_NAME_SIZE);
        let (decoded, consumed) = TrafficSaveData::decode(&bytes).unwrap();
        assert_eq!(decoded, payload);
        assert_eq!(consumed, bytes.len());
        assert_eq!(
            TrafficSaveData::decode(&bytes[..23]),
            Err(TrafficSaveGameError::Truncated {
                expected: 24,
                observed: 23
            })
        );
        let mut bad_version = bytes.clone();
        bad_version[0] = 6;
        assert!(TrafficSaveData::decode(&bad_version).is_err());
    }

    #[test]
    fn removals_stay_explicit_unsupported_boundaries() {
        assert_eq!(
            remove_all_cars(),
            Err(TrafficRemovalError::Unsupported {
                entry: REMOVE_ALL_CARS_ENTRY
            })
        );
        assert_eq!(
            remove_all_cars(),
            Err(TrafficRemovalError::Unsupported { entry: 0x4bad80 })
        );
        assert_eq!(
            remove_car(0x1234),
            Err(TrafficRemovalError::Unsupported { entry: 0x4bbe90 })
        );
    }

    #[test]
    fn rva_b8540_get_parking_cars_returns_parking_list() {
        // C_traffic_car::GetParkingCars GAME RVA 0xb8540 (VA 0x4b8540).
        let mut lists = TrafficCarLists::default();
        assert!(get_parking_cars(&lists).is_empty());
        lists.parking_cars_mut().push(TrafficListSlot { frame: 1, car: 2 });
        lists.parking_cars_mut().push(TrafficListSlot { frame: 3, car: 4 });
        lists.elems_mut().push(TrafficListSlot { frame: 9, car: 9 });
        assert_eq!(
            get_parking_cars(&lists),
            &[TrafficListSlot { frame: 1, car: 2 }, TrafficListSlot { frame: 3, car: 4 }]
        );
    }

    #[test]
    fn rva_a9ca0_get_elems_returns_traffic_list() {
        // C_traffic_car::GetElems GAME RVA 0xa9ca0 (VA 0x4a9ca0).
        let mut lists = TrafficCarLists::default();
        assert!(get_elems(&lists).is_empty());
        lists.elems_mut().push(TrafficListSlot { frame: 7, car: 8 });
        lists.parking_cars_mut().push(TrafficListSlot { frame: 1, car: 1 });
        assert_eq!(get_elems(&lists), &[TrafficListSlot { frame: 7, car: 8 }]);
    }

    #[test]
    fn rva_ab120_vfunc_38_always_false() {
        // C_traffic_car::vfunc_38 GAME RVA 0xab120 (VA 0x4ab120).
        assert!(!traffic_car_vfunc_38([0; 12]));
        assert!(!traffic_car_vfunc_38([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]));
        assert!(!traffic_car_vfunc_38([i32::MIN; 12]));
    }

    #[test]
    fn rva_bc150_save_game_size_adds_two() {
        // C_traffic_car::SaveGameGetSize GAME RVA 0xbc150 (VA 0x4bc150).
        assert_eq!(TRAFFIC_SAVE_GAME_EXTRA, 2);
        assert_eq!(traffic_car_save_game_size(0), 2);
        assert_eq!(traffic_car_save_game_size(41), 43);
        assert_eq!(traffic_car_save_game_size(i32::MAX), i32::MIN.wrapping_add(1));
    }

    #[test]
    fn rva_aab20_duplicate_from_copies_words_and_names() {
        // C_traffic_car::DuplicateFrom GAME RVA 0xaab20 (VA 0x4aab20).
        let mut source = constructed();
        source.put_load_word(0x21c, 0x11111111);
        source.put_load_word(0x224, 0x22222222);
        source.put_load_word(0x228, 0x33333333);
        source.put_load_word(0x22c, 0x44444444);
        source.replace_elements(vec![0x5au8; 0x40]);
        let mut target = constructed();
        target.put_load_word(0x224, 0xdeadbeef);
        target.duplicate_from(&source);
        for (offset, word) in [
            (0x21c, 0x11111111u32),
            (0x224, 0x22222222),
            (0x228, 0x33333333),
            (0x22c, 0x44444444),
        ] {
            assert_eq!(target.word(offset), word, "{offset:#x}");
        }
        assert_eq!(target.elements(), &[0x5au8; 0x40]);
        // Untouched words outside the four copied offsets survive.
        assert_eq!(target.word(0x220), 0);
    }

    #[test]
    fn rva_b8480_corovacka_reports_unsupported_null() {
        // C_traffic_car::Corovacka GAME RVA 0xb8480 (VA 0x4b8480): STUB NULL.
        assert_eq!(COROVACKA_ENTRY, 0x4b8480);
        assert_eq!(
            corovacka(0x1234),
            Err(TrafficRemovalError::Unsupported { entry: 0x4b8480 })
        );
    }

    #[test]
    fn rva_afc40_ai_reports_unsupported() {
        // C_traffic_car::AI GAME RVA 0xafc40 (VA 0x4afc40): base-delegating STUB.
        assert_eq!(TRAFFIC_AI_ENTRY, 0x4afc40);
        assert_eq!(
            traffic_ai(16),
            Err(TrafficOverrideError::Unsupported { entry: 0x4afc40 })
        );
    }

    #[test]
    fn rva_b5270_update_reports_unsupported() {
        // C_traffic_car::Update GAME RVA 0xb5270 (VA 0x4b5270): base-delegating STUB.
        assert_eq!(TRAFFIC_UPDATE_ENTRY, 0x4b5270);
        assert_eq!(
            traffic_update(16),
            Err(TrafficOverrideError::Unsupported { entry: 0x4b5270 })
        );
    }

    #[test]
    fn rva_baad0_hit_reports_unsupported() {
        // C_traffic_car::Hit GAME RVA 0xbaad0 (VA 0x4baad0): base-delegating STUB.
        assert_eq!(TRAFFIC_HIT_ENTRY, 0x4baad0);
        assert_eq!(
            traffic_hit(),
            Err(TrafficOverrideError::Unsupported { entry: 0x4baad0 })
        );
    }

    #[test]
    fn rva_ba9c0_is_possible_coll_reports_unsupported() {
        // C_traffic_car::IsPossibleColl GAME RVA 0xba9c0 (VA 0x4ba9c0): base-delegating STUB.
        assert_eq!(TRAFFIC_IS_POSSIBLE_COLL_ENTRY, 0x4ba9c0);
        assert_eq!(
            traffic_is_possible_coll(),
            Err(TrafficOverrideError::Unsupported { entry: 0x4ba9c0 })
        );
    }

    #[test]
    fn rva_aad20_load_data_reports_unsupported() {
        // C_traffic_car::LoadData GAME RVA 0xaad20 (VA 0x4aad20): base-delegating STUB.
        assert_eq!(TRAFFIC_LOAD_DATA_ENTRY, 0x4aad20);
        assert_eq!(
            traffic_load_data(),
            Err(TrafficOverrideError::Unsupported { entry: 0x4aad20 })
        );
    }
}
