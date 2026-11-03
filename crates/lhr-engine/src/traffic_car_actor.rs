//! C_traffic_car derived storage for the original PC actor lifecycle (kind 12).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x4aa7a0, Init0x4aa860, LoadData0x4aad20,
//! vtable VA0x624c98, 0x26c bytes (PS2's 0x290). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! Member names are descriptive PC projections; no PS2 member layout is assumed.

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
}
