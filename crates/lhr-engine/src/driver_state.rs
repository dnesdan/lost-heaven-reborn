//! Bounded raw driver-state storage, using descriptive working names.
//!
//! The getter reads shared storage; the setter subset here has no renderer effects.
//! Storage must be supplied explicitly: driver construction is outside this contract.

/// A state write outside the recovered storage-only subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverStateError {
    /// The native storage bank has 63 entries. Negative native indices are unsafe
    /// memory accesses and are deliberately outside this safe adapter.
    OutsideBank,
    /// This state may invoke a renderer or update additional shared storage.
    RequiresBackend,
}

/// Shared raw state words. Keep one bank for serialized driver callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDriverState {
    words: [u32; 63],
}

impl RawDriverState {
    /// Supply existing words without assuming native constructor defaults.
    pub const fn from_words(words: [u32; 63]) -> Self {
        Self { words }
    }

    /// Read a raw state word. No boolean or actor interpretation is imposed.
    /// Unlike the native positive out-of-range diagnostic/zero return, this
    /// safe adapter reports `OutsideBank` for every out-of-bank index.
    pub fn get_raw(&self, state: i32) -> Result<u32, DriverStateError> {
        let index = usize::try_from(state).map_err(|_| DriverStateError::OutsideBank)?;
        self.words
            .get(index)
            .copied()
            .ok_or(DriverStateError::OutsideBank)
    }

    /// Store a word only for recovered arms with no additional effects.
    /// Rejection leaves the entire bank unchanged.
    pub fn set_storage_only(&mut self, state: i32, value: u32) -> Result<(), DriverStateError> {
        if !(0..63).contains(&state) {
            return Err(DriverStateError::OutsideBank);
        }
        if !matches!(state, 1 | 5 | 6 | 10..=17 | 20..=49 | 51 | 52 | 56..=61) {
            return Err(DriverStateError::RequiresBackend);
        }
        self.words[state as usize] = value;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_words_and_rejected_writes_preserve_other_states() {
        const SUPPORTED: [i32; 49] = [
            1, 5, 6, 10, 11, 12, 13, 14, 15, 16, 17, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30,
            31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 51, 52, 56,
            57, 58, 59, 60, 61,
        ];
        let mut bank = RawDriverState::from_words(std::array::from_fn(|i| i as u32));
        for value in [0, 1, 0x8000_0000, u32::MAX, 0x1234_5678] {
            for state in 0..63 {
                let before = bank.clone();
                if SUPPORTED.contains(&state) {
                    assert_eq!(bank.set_storage_only(state, value), Ok(()));
                    for other in 0..63 {
                        assert_eq!(
                            bank.get_raw(other),
                            if other == state {
                                Ok(value)
                            } else {
                                before.get_raw(other)
                            }
                        );
                    }
                } else {
                    assert_eq!(
                        bank.set_storage_only(state, value),
                        Err(DriverStateError::RequiresBackend)
                    );
                    assert_eq!(bank, before);
                }
            }
        }
        let before = bank.clone();
        for state in [-1, i32::MIN, 63, i32::MAX] {
            assert_eq!(bank.get_raw(state), Err(DriverStateError::OutsideBank));
            assert_eq!(
                bank.set_storage_only(state, 0),
                Err(DriverStateError::OutsideBank)
            );
        }
        assert_eq!(bank, before);
    }
}
