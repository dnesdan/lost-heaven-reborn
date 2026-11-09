//! I3D_volume word contracts, PC RVAs 0x5a7a0/0x5a840/0x5a850/0x5a860 (LS3DF
//! SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//! The arena has no Volume frame kind (FrameKind check verified — never
//! invented here); these are standalone word contracts for the evidenced
//! ctor/dtor/type/update words. Upstream working names (mafia-re
//! I3d_volume.cpp), not evidenced original symbols.
#![forbid(unsafe_code)]

/// [frame+0x110] type word FRAME_VOLUME=11 (differential 0x5a7a0).
pub const VOLUME_TYPE_WORD: u32 = 0xb;
/// u32 volume_type at +0x140 (contract 0x5a850: MOV EAX,[EAX+0x140], RET 4).
pub const VOLUME_TYPE_OFFSET: usize = 0x140;
/// Invalidate bit patterns, preserved verbatim with no float-domain claim
/// (differential 0x5a7a0: ECX immediates into +0x154..0x168).
pub const VOLUME_BBOX_MIN_WORD: u32 = 0x5a0e1bca;
pub const VOLUME_BBOX_MAX_WORD: u32 = 0xda0e1bca;

/// Ctor words (differential 0x5a7a0, 118 B/24 insn/1 BB): pre-vtable zeroes
/// (+0x140..0x150), vtable, type word 0xb, Invalidate box words, tail zeroes
/// (+0x16c/+0x170). All u32 stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeCtorWords {
    pub zeros_140_150: [u32; 5],
    pub type_word: u32,
    pub min_words: [u32; 3],
    pub max_words: [u32; 3],
    pub tail_zeros: [u32; 2],
}
impl VolumeCtorWords {
    pub fn constructor() -> Self {
        Self {
            zeros_140_150: [0; 5],
            type_word: VOLUME_TYPE_WORD,
            min_words: [VOLUME_BBOX_MIN_WORD; 3],
            max_words: [VOLUME_BBOX_MAX_WORD; 3],
            tail_zeros: [0; 2],
        }
    }
}

/// Volume-type read (contract 0x5a850): word0 of the six words at +0x140.
pub fn volume_type_of(words: &[u32; 6]) -> u32 {
    words[0]
}

/// Working name for the UpdateV type gate (mafia-re
/// ls3df/I3D/frames/I3d_volume.cpp:43-61; I3D.h FRAME_VOLUME=11).
/// The reference records no LS3DF FUNCTION address for a standalone gate
/// reader; the switch runs inside UpdateV (PC RVA 0x5a860). Reports whether
/// UpdateV applies a scale limb for the type (1, 5, 6, 4); pure helper, no
/// matrix moves.
pub fn volume_update_applies(volume_type: u32) -> bool {
    matches!(volume_type, 1 | 4 | 5 | 6)
}

/// UpdateV scale limb (contract 0x5a860: UPDATE-gated type switch per
/// mafia-re I3d_volume.cpp:39-65; UpdateMatrix/world-copy/WMAT_VALID stay
/// caller boundaries). Pure scale selection for tests and callers.
pub fn volume_update_scale_limb(volume_type: u32, sx: f32, _sy: f32, sz: f32) -> Option<[f32; 3]> {
    match volume_type {
        1 => Some([1.0, 1.0, 1.0]),
        5 | 6 => Some([sx, sx, sx]),
        4 => Some([sx, 1.0, sz]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_ctor_words_type_read_and_scale_limbs() {
        let ctor = VolumeCtorWords::constructor();
        assert_eq!(ctor.type_word, 0xb);
        assert_eq!(ctor.zeros_140_150, [0; 5]);
        assert_eq!(ctor.min_words, [VOLUME_BBOX_MIN_WORD; 3]);
        assert_eq!(ctor.max_words, [VOLUME_BBOX_MAX_WORD; 3]);
        assert_eq!(ctor.tail_zeros, [0; 2]);
        assert_eq!(VOLUME_TYPE_OFFSET, 0x140);
        assert_eq!(volume_type_of(&[5, 0, 0, 0, 0, 0]), 5);
        assert_eq!(volume_update_scale_limb(6, 2., 3., 4.), Some([2., 2., 2.]));
        assert_eq!(volume_update_scale_limb(4, 2., 3., 4.), Some([2., 1., 4.]));
        assert_eq!(volume_update_scale_limb(1, 2., 3., 4.), Some([1., 1., 1.]));
        assert_eq!(volume_update_scale_limb(7, 2., 3., 4.), None);
        assert!(volume_update_applies(1));
        assert!(volume_update_applies(4));
        assert!(volume_update_applies(5));
        assert!(volume_update_applies(6));
        assert!(!volume_update_applies(0));
        assert!(!volume_update_applies(2));
        assert!(!volume_update_applies(u32::MAX));
    }
}
