//! Original PC `C_Vehicle` construction over explicitly supplied storage.
//!
//! Construction preserves bytes the original constructor leaves untouched. It
//! does not initialize a model, BODY geometry, wheels or physics resources.
//! Native addresses here are inert 32-bit identity tokens, never host pointers.

/// A host projection cannot provide the next original memory access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleOwnerUnavailable {
    Storage(u32),
}

/// Detached storage for original vehicle lifetime operations. Rust allocation
/// and dropping these bytes have no original construction/destruction effects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VehicleOwnerState {
    bytes: Vec<u8>,
}

type Result<T> = std::result::Result<T, VehicleOwnerUnavailable>;

impl VehicleOwnerState {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn store_byte(&mut self, offset: u32, value: u8) -> Result<()> {
        *self
            .bytes
            .get_mut(offset as usize)
            .ok_or(VehicleOwnerUnavailable::Storage(offset))? = value;
        Ok(())
    }

    fn store(&mut self, offset: u32, value: u32) -> Result<()> {
        let start = offset as usize;
        let bytes = self
            .bytes
            .get_mut(start..start + 4)
            .ok_or(VehicleOwnerUnavailable::Storage(offset))?;
        bytes.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn zero_words(&mut self, start: u32, count: u32) -> Result<()> {
        for index in 0..count {
            self.store(start + index * 4, 0)?;
        }
        Ok(())
    }

    fn construct_member(&mut self, start: u32, marker: u8) -> Result<()> {
        self.store_byte(start, marker)?;
        self.zero_words(start + 4, 3)
    }

    /// Original constructor, returning its supplied native `this` identity.
    ///
    /// The original copies the high byte of `this` into several member marker
    /// bytes. The vtable token is the supplied PC build's original value. No
    /// native-address alignment, allocation, or pointer validity is asserted.
    /// Stores run in original order; host storage failure keeps their prefix.
    /// Each four-byte store requires all four bytes and is otherwise unapplied.
    pub fn construct(&mut self, native_this: u32) -> Result<u32> {
        self.construct_with_receiver_tag(native_this, (native_this >> 24) as u8)
    }
    /// Contained host identity adapters supply the original receiver-byte seed
    /// separately; host address translation must not invent allocator tags.
    pub(crate) fn construct_with_receiver_tag(
        &mut self,
        native_this: u32,
        marker: u8,
    ) -> Result<u32> {
        for offset in [0xd8, 0xe8, 0xfc, 0x10c, 0x204, 0x214] {
            self.construct_member(offset, marker)?;
        }
        self.zero_words(0x22c, 18)?;
        self.zero_words(0x274, 18)?;
        for offset in [
            0x32c, 0x33c, 0x450, 0x4ac, 0xca0, 0xcb0, 0xcc0, 0x1fb0, 0x1fc0, 0x1fd0, 0x1fe0, 0x1ff8,
        ] {
            self.construct_member(offset, marker)?;
        }
        for offset in [0x98, 0x2e0, 0x432] {
            self.store_byte(offset, 1)?;
        }
        self.store(0xc68, 0xdfff)?;
        self.store(0xc6c, 0xdfff)?;
        self.store(0x198, 0x3f00_0000)?;
        self.store(0x19c, 0x3f00_0000)?;
        self.store(0, 0x0062_4df0)?;
        self.store(0x2ec, 0)?;
        self.zero_words(0x2e4, 2)?;
        self.zero_words(0x2f0, 12)?;
        self.store(0x9c, 0xf02)?;
        self.store(0xa4, 2)?;
        self.store(0xa0, 0)?;
        self.store(0xa8, 0)?;
        self.store(0xb8, 0)?;
        self.store(0xb0, 0x3f80_0000)?;
        self.store(0xb4, 0)?;
        self.store_byte(0x13c, 0)?;
        self.store_byte(0x13d, 0)?;
        self.store(0x124, 0x3ecc_cccd)?;
        self.store(0x128, 0x3e99_999a)?;
        self.store(0x12c, 0)?;
        self.store(0x130, 0x3e4c_cccd)?;
        self.store(0x138, 0)?;
        self.store(0x134, 0x3e99_999a)?;
        for offset in [0x194, 0x200, 0x350] {
            self.store(offset, 0)?;
        }
        self.store_byte(0x490, 0)?;
        self.store_byte(0x491, 0)?;
        for offset in [0xc34, 0xc38, 0x4bc, 0xc74] {
            self.store(offset, 0)?;
        }
        self.store(0x2bc, 0x32)?;
        self.store(0xc24, 0x3ce5_6042)?;
        self.store(0xc70, 0)?;
        self.zero_words(0x64c, 29)?;
        for index in 0..4 {
            self.store(0x6cc - index * 4, 0)?;
            self.store(0x6dc - index * 4, 0)?;
        }
        self.zero_words(0x6e0, 3)?;
        self.zero_words(0x92c, 30)?;
        for index in 0..4 {
            self.store(0x9b0 - index * 4, 0)?;
            self.store(0x9c0 - index * 4, 0)?;
        }
        self.zero_words(0x9c4, 3)?;
        self.store(0x34c, 0)?;
        Ok(native_this)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(state: &VehicleOwnerState, offset: usize) -> u32 {
        u32::from_le_bytes(state.as_bytes()[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn constructor_retains_unwritten_storage_and_native_identity() {
        let initial: Vec<u8> = (0..0x2400).map(|index| (index * 37 + 9) as u8).collect();
        let mut state = VehicleOwnerState::from_bytes(initial.clone());
        assert_eq!(state.construct(0xa123_4567), Ok(0xa123_4567));
        for offset in [0xd8, 0xe8, 0x214, 0xcc0, 0x1fb0, 0x1ff8] {
            assert_eq!(state.as_bytes()[offset], 0xa1);
            assert_eq!(
                &state.as_bytes()[offset + 1..offset + 4],
                &initial[offset + 1..offset + 4]
            );
            assert_eq!(&state.as_bytes()[offset + 4..offset + 16], &[0; 12]);
        }
        for offset in [4, 0xac, 0xf8, 0x640, 0xe68, 0x2008, 0x23ff] {
            assert_eq!(state.as_bytes()[offset], initial[offset]);
        }
        assert_eq!(word(&state, 0), 0x624df0);
        assert_eq!(word(&state, 0x9c), 0xf02);
        assert_eq!(word(&state, 0xc24), 0x3ce56042);
        assert_eq!(word(&state, 0x34c), 0);
    }

    #[test]
    fn marker_is_exactly_the_native_address_high_byte() {
        for marker in 0..=255u32 {
            let mut state = VehicleOwnerState::from_bytes(vec![0x7d; 0x2008]);
            let address = marker << 24 | 0x0012_3456;
            assert_eq!(state.construct(address), Ok(address));
            assert_eq!(state.as_bytes()[0x1ff8], marker as u8);
            assert_eq!(state.as_bytes()[0x1ff9], 0x7d);
        }
    }

    #[test]
    fn unavailable_word_preserves_only_completed_original_stores() {
        let mut state = VehicleOwnerState::from_bytes(vec![0xab; 0xde]);
        assert_eq!(
            state.construct(0x1234_5678),
            Err(VehicleOwnerUnavailable::Storage(0xdc))
        );
        assert_eq!(state.as_bytes()[0xd8], 0x12);
        assert!(state.as_bytes()[..0xd8].iter().all(|byte| *byte == 0xab));
        assert!(state.as_bytes()[0xd9..].iter().all(|byte| *byte == 0xab));
    }

    #[test]
    fn highest_access_fails_before_later_low_offset_defaults() {
        let mut state = VehicleOwnerState::from_bytes(vec![0x5a; 0x2007]);
        assert_eq!(
            state.construct(0xee00_0010),
            Err(VehicleOwnerUnavailable::Storage(0x2004))
        );
        assert_eq!(state.as_bytes()[0x1ff8], 0xee);
        assert_eq!(word(&state, 0x2000), 0);
        assert_eq!(&state.as_bytes()[0x2004..], &[0x5a; 3]);
        assert_eq!(word(&state, 0), 0x5a5a5a5a);
        assert_eq!(state.as_bytes()[0x98], 0x5a);
    }
}
