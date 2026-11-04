//! I3D_user word contracts, PC RVAs 0x5a140/0x5a190/0x5a1a0 (LS3DF SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//! The arena has no User frame kind (FrameKind check verified — never
//! invented here); these are standalone word contracts for the evidenced
//! ctor/dtor/duplicate words. Upstream working names (mafia-re
//! I3d_user.cpp), not evidenced original symbols.
#![forbid(unsafe_code)]

/// [frame+0x110] type word (differential 0x5a140: base ctor, vtable store,
/// u32 MOV stores).
pub const USER_TYPE_WORD: u32 = 8;
/// u32 add_data at +0x140 (differential 0x5a140, contract 0x5a1a0).
pub const USER_ADD_DATA_OFFSET: usize = 0x140;

/// Ctor words (differential 0x5a140, 38 B/9 insn/1 BB): base ctor, then
/// vtable, type word 8, add_data 0 — all u32 stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserCtorWords {
    pub type_word: u32,
    pub add_data: u32,
}
impl UserCtorWords {
    pub fn constructor() -> Self {
        Self {
            type_word: USER_TYPE_WORD,
            add_data: 0,
        }
    }
}

/// Duplicate add_data gate (contract 0x5a1a0: prologue + CMP type,8 +
/// add_data MOV pair + JMP tail, no Ghidra body; mafia-re I3d_user.cpp:20-29).
/// Order-exact: null limb -> gated add_data copy -> common tail (the caller
/// runs duplicate_frame). Some(word) carries the copy; None is the
/// cross-type residue.
pub fn user_duplicate_add_data(src_type: u32, src_add_data: u32) -> Option<u32> {
    (src_type == USER_TYPE_WORD).then_some(src_add_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_ctor_words_and_duplicate_gate() {
        assert_eq!(
            UserCtorWords::constructor(),
            UserCtorWords {
                type_word: 8,
                add_data: 0
            }
        );
        assert_eq!(user_duplicate_add_data(8, 0x1234), Some(0x1234));
        assert_eq!(user_duplicate_add_data(9, 0x1234), None);
        assert_eq!(user_duplicate_add_data(u32::MAX, 0), None);
    }
}
