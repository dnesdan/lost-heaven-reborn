//! C_box_new derived storage for the original PC actor lifecycle (kind 35).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x5674f0, Init0x567540, LoadData0x567730,
//! vtable VA0x6254f0, 0x3a8 bytes (PS2's 0x660). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! Member names are descriptive PC projections; no PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x3a8-byte box object. The
/// constructor stores only the vtable and kind marker; every other word is
/// admitted untouched allocation, not interpreted native state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxNewSeed {
    pub bytes_00_3a7: [u8; 0x3a8],
}

/// Owned box bytes after construction and Init/LoadData effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxNewState {
    bytes: [u8; 0x3a8],
}

impl BoxNewState {
    pub(crate) fn allocation(seed: BoxNewSeed) -> Self {
        Self {
            bytes: seed.bytes_00_3a7,
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

    fn float(&self, offset: usize) -> f32 {
        f32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("box float"),
        )
    }

    /// Original constructor body: base C_box_new construction runs first
    /// (owned by the caller through construct_base), then the vtable store
    /// and the kind marker on the base owner (see construct_box_new).
    pub(crate) fn complete_constructor(&mut self) {}

    /// Original C_box_new::Init tail after the base init: six words from the
    /// frame hierarchy volume. The UpdateHRBoundVol/UpdateWMatrix/GetScale
    /// refreshes are unobservable (they feed no stored word), so the caller
    /// supplies the already-refreshed hierarchy volume and only the stored
    /// selection is mirrored here: volumes strictly inside (0.01, 1000.0)
    /// scale the +0x398 word by 50, everything else (including NaN) keeps the
    /// 10.0 default. NaN comparisons stay false on both sides, exactly like
    /// the native x87 comparisons.
    pub(crate) fn complete_init(&mut self, volume: f32) {
        self.put_byte(0x38c, 1);
        self.put_word(0x390, 0x3f800000);
        self.put_word(0x394, 0x3f800000);
        self.put_word(0x398, 0x41200000);
        self.put_word(0x39c, 0x3f000000);
        self.put_word(0x3a0, 0x3c23d70a);
        if volume > 0.01 && volume < 1000.0 {
            self.put_word(0x398, (volume * 50.0).to_bits());
        }
    }

    pub(crate) fn put_load_byte(&mut self, offset: usize, value: u8) {
        self.put_byte(offset, value);
    }

    pub(crate) fn put_load_word(&mut self, offset: usize, value: u32) {
        self.put_word(offset, value);
    }

    /// Density word at +0x398 shared by Init, every LoadData variant and the
    /// trailing clamp.
    pub fn density_word(&self) -> f32 {
        self.float(0x398)
    }

    /// Trailing LoadData clamp: above 100000.0 pins to 100000.0, below 0.01
    /// pins to 0.01. NaN passes both comparisons through untouched, like the
    /// native float compares.
    pub(crate) fn clamp_density(&mut self) {
        let density = self.float(0x398);
        if density > 100000.0 {
            self.put_word(0x398, 0x47c35000);
        }
        if density < 0.01 {
            self.put_word(0x398, 0x3c23d70a);
        }
    }
}

/// Payload flag selecting the C_box_new::LoadData field set: variant 0 reads
/// the six words and the +0x3a0 word, variant 1 skips those and reads +0x348,
/// variant 2 additionally reads +0x34c. Any other flag consumes only itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxLoadFlag {
    V0,
    V1,
    V2,
    Other,
}

impl BoxLoadFlag {
    pub fn decode(flag: u8) -> Self {
        match flag {
            0 => Self::V0,
            1 => Self::V1,
            2 => Self::V2,
            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> BoxNewState {
        let mut state = BoxNewState::allocation(BoxNewSeed {
            bytes_00_3a7: [0xaa; 0x3a8],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_no_derived_words() {
        let state = constructed();
        assert_eq!(state.bytes, [0xaa; 0x3a8]);
    }

    #[test]
    fn init_tail_selects_density_by_volume() {
        let mut state = constructed();
        state.complete_init(0.005);
        assert_eq!(state.density_word(), 10.0);
        assert_eq!(state.bytes[0x38c], 1);
        assert_eq!(state.float(0x390), 1.0);
        assert_eq!(state.float(0x394), 1.0);
        assert_eq!(state.float(0x39c), 0.5);
        assert_eq!(state.float(0x3a0), 0.01);
        let mut state = constructed();
        state.complete_init(0.01);
        assert_eq!(state.density_word(), 10.0);
        let mut state = constructed();
        state.complete_init(40.0);
        assert_eq!(state.density_word(), 2000.0);
        let mut state = constructed();
        state.complete_init(999.0);
        assert_eq!(state.density_word(), 49950.0);
        let mut state = constructed();
        state.complete_init(1000.0);
        assert_eq!(state.density_word(), 10.0);
        let mut state = constructed();
        state.complete_init(f32::NAN);
        assert_eq!(state.density_word(), 10.0);
    }

    #[test]
    fn density_clamp_pins_both_sides_and_passes_nan() {
        let mut state = constructed();
        state.complete_init(40.0);
        state.put_load_word(0x398, 0x47c35000 | 1);
        state.clamp_density();
        assert_eq!(state.density_word(), 100000.0);
        state.put_load_word(0x398, 0x3c23d709);
        state.clamp_density();
        assert_eq!(state.density_word(), 0.01);
        state.put_load_word(0x398, 40.0f32.to_bits());
        state.clamp_density();
        assert_eq!(state.density_word(), 40.0);
        state.put_load_word(0x398, f32::NAN.to_bits());
        state.clamp_density();
        assert!(state.density_word().is_nan());
    }

    #[test]
    fn load_flag_decodes_known_and_other_values() {
        assert_eq!(BoxLoadFlag::decode(0), BoxLoadFlag::V0);
        assert_eq!(BoxLoadFlag::decode(1), BoxLoadFlag::V1);
        assert_eq!(BoxLoadFlag::decode(2), BoxLoadFlag::V2);
        assert_eq!(BoxLoadFlag::decode(3), BoxLoadFlag::Other);
        assert_eq!(BoxLoadFlag::decode(255), BoxLoadFlag::Other);
    }
}
