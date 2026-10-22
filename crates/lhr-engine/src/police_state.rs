//! Bounded raw police-manager initialization and reset, with working names.
//!
//! Existing bytes and the constructor's opaque allocator marker are explicit
//! inputs. Native pointer words remain opaque words, never Rust pointers. This
//! module does not construct resources, own actors, or implement manager ON.

/// Raw words of one retained native allocation; these are not host addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawManagerPool {
    pub begin_word: u32,
    pub end_word: u32,
    pub allocation_end_word: u32,
}

/// The recovered raw state extent. Unknown bytes retain their supplied values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoliceManagerRawState {
    bytes: [u8; 0xac],
}

impl PoliceManagerRawState {
    /// Supply existing state without assuming zeroed construction or a zero mode byte.
    pub const fn from_bytes(bytes: [u8; 0xac]) -> Self {
        Self { bytes }
    }
    pub const fn as_bytes(&self) -> &[u8; 0xac] {
        &self.bytes
    }
    pub const fn raw_mode(&self) -> u8 {
        self.bytes[0]
    }
    /// The native guard tests truth without normalizing an existing mode byte.
    pub const fn mode_is_nonzero(&self) -> bool {
        self.raw_mode() != 0
    }
    /// Raw first script argument; no actor-slot interpretation is imposed.
    pub fn script_argument_0_bits(&self) -> u32 {
        self.word(0x34)
    }
    /// Stored native byte; script dispatch normalizes its argument separately.
    pub const fn script_argument_1_raw(&self) -> u8 {
        self.bytes[0x39]
    }
    /// Seven retained allocations in original order. No pointer is dereferenced.
    pub fn pool(&self, index: usize) -> Option<RawManagerPool> {
        if index >= 7 {
            return None;
        }
        let offset = 0x40 + index * 0x10;
        Some(RawManagerPool {
            begin_word: self.word(offset),
            end_word: self.word(offset + 4),
            allocation_end_word: self.word(offset + 8),
        })
    }

    /// Apply only the evidenced constructor stores. The original copies the
    /// high byte of its object address into seven opaque allocator bytes; this
    /// portable projection requires that marker explicitly. Mode and script
    /// arguments are preserved, so construction does not imply a zero mode byte.
    pub fn apply_constructor_fields(&mut self, allocator_marker: u8) {
        for offset in [0x04, 0x14, 0x1c, 0x24] {
            self.set_word(offset, 0);
        }
        for offset in [0x08, 0x18, 0x20, 0x28] {
            self.set_word(offset, 200);
        }
        for index in 0..7 {
            let marker = 0x3c + index * 0x10;
            self.bytes[marker] = allocator_marker;
            for offset in [marker + 4, marker + 8, marker + 12] {
                self.set_word(offset, 0);
            }
        }
    }

    /// Reset the observed scalar words and truncate four logical pools. Every
    /// allocation word and all pointee storage remain owned by the caller.
    /// Other pool ends, allocator markers, script arguments and unknown bytes
    /// are preserved. No element destruction or resource creation occurs.
    pub fn reset_retaining_pools(&mut self) {
        self.bytes[0] = 0;
        for (offset, value) in [
            (0x04, 0),
            (0x08, 0x14a),
            (0x0c, 0x4188_0000),
            (0x10, 0x41e0_0000),
            (0x14, 0),
            (0x18, 0x391),
            (0x1c, 0),
            (0x20, 0xd9),
            (0x24, 0),
            (0x28, 0xfad),
            (0x2c, 0),
            (0x30, 0),
        ] {
            self.set_word(offset, value);
        }
        for begin in [0x40, 0x50, 0x60, 0x80] {
            self.set_word(begin + 4, self.word(begin));
        }
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn set_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_preserves_mode_arguments_and_unwritten_bytes() {
        let before = std::array::from_fn(|i| (i as u8).wrapping_mul(13).wrapping_add(71));
        let mut state = PoliceManagerRawState::from_bytes(before);
        state.apply_constructor_fields(0x42);
        assert_eq!(state.raw_mode(), before[0]);
        assert!(state.mode_is_nonzero());
        assert_eq!(
            state.script_argument_0_bits(),
            u32::from_le_bytes(before[0x34..0x38].try_into().unwrap())
        );
        assert_eq!(state.script_argument_1_raw(), before[0x39]);
        for i in 0..7 {
            assert_eq!(
                state.pool(i),
                Some(RawManagerPool {
                    begin_word: 0,
                    end_word: 0,
                    allocation_end_word: 0
                })
            );
            assert_eq!(state.as_bytes()[0x3c + i * 16], 0x42);
        }
        assert_eq!(state.pool(7), None);
        for offset in [
            1, 2, 3, 0x0c, 0x10, 0x2c, 0x30, 0x38, 0x3a, 0x3b, 0x3d, 0x4d, 0x9f,
        ] {
            assert_eq!(state.as_bytes()[offset], before[offset]);
        }
        for offset in [0x08, 0x18, 0x20, 0x28] {
            assert_eq!(state.word(offset), 200);
        }
    }

    #[test]
    fn reset_retains_all_storage_and_truncates_only_four_pools() {
        let mut bytes = [0xa5; 0xac];
        for index in 0..7 {
            let offset = 0x40 + index * 16;
            for (at, word) in [
                (offset, 0x1000 + index as u32 * 0x100),
                (offset + 4, 0x1080 + index as u32 * 0x100),
                (offset + 8, 0x1100 + index as u32 * 0x100),
            ] {
                bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        let mut state = PoliceManagerRawState::from_bytes(bytes);
        let before = state.clone();
        state.reset_retaining_pools();
        assert!(!state.mode_is_nonzero());
        for i in 0..7 {
            let pool = state.pool(i).unwrap();
            let old = before.pool(i).unwrap();
            assert_eq!(pool.begin_word, old.begin_word);
            assert_eq!(pool.allocation_end_word, old.allocation_end_word);
            assert_eq!(
                pool.end_word,
                if [0, 1, 2, 4].contains(&i) {
                    old.begin_word
                } else {
                    old.end_word
                }
            );
            assert_eq!(state.as_bytes()[0x3c + i * 16], 0xa5);
        }
        assert_eq!(
            state.script_argument_0_bits(),
            before.script_argument_0_bits()
        );
        assert_eq!(
            state.script_argument_1_raw(),
            before.script_argument_1_raw()
        );
        assert_eq!(state.word(0x0c), 17f32.to_bits());
        assert_eq!(state.word(0x10), 28f32.to_bits());
        let once = state.clone();
        state.reset_retaining_pools();
        assert_eq!(state, once);
    }
}
