#![forbid(unsafe_code)]
//! I3D_driver string helpers: FrameType2StringFunc and VisualType2String
//! (LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//! Upstream reference mafia-re ls3df/I3D/I3d_driver.cpp @ ab67b13
//! (FrameType2StringFunc 1531-1557, FrameType2String trampoline 1559-1563,
//! VisualType2String 1566-1593), frame enum I3D.h:62-83, visual fourccs
//! I3D.h:85-98. FrameType2String is an evidenced PS2 original trampoline that
//! only forwards to the free FrameType2StringFunc, so one port covers RVAs
//! 0x19930 and 0x19a10 with no separate credit; VisualType2String covers RVA
//! 0x19a20. Tier contract (new-module wave policy: contract regardless of
//! bodies; FrameType2StringFunc formed 20BB/45insn/142B per assignment,
//! VisualType2String LAB_ per assignment).
//!
//! Typed shared-buffer model: the original formats unknown types with sprintf
//! into a shared static str[64] (plus a fourcc static for visuals), so callers
//! must copy the result before the next call and must not share it across
//! threads. This port keeps one caller-owned scratch object with the same two
//! 64-byte banks; returned borrows live only until the next call on the same
//! bank, and the &mut requirement is the host thread-safety boundary. Known
//! arms return literals and never touch the scratch. The visual fourcc bytes
//! are little-endian type bytes truncated at the first zero; non-UTF8 bytes
//! are mapped lossily (native emits raw bytes), which the tests disclose on
//! the ASCII subset.

/// Frame type words (I3D.h:62-83).
pub const FRAME_VISUAL: u32 = 1;
pub const FRAME_LIGHT: u32 = 2;
pub const FRAME_CAMERA: u32 = 3;
pub const FRAME_SOUND: u32 = 4;
pub const FRAME_SECTOR: u32 = 5;
pub const FRAME_DUMMY: u32 = 6;
pub const FRAME_TARGET: u32 = 7;
pub const FRAME_USER: u32 = 8;
pub const FRAME_MODEL: u32 = 9;
pub const FRAME_JOINT: u32 = 10;
pub const FRAME_VOLUME: u32 = 11;
pub const FRAME_OCCLUDER: u32 = 12;
pub const FRAME_SCENE: u32 = 13;
pub const FRAME_AREA: u32 = 14;
pub const FRAME_SHADOW: u32 = 15;
pub const FRAME_LANDSCAPE: u32 = 16;
pub const FRAME_EMITOR: u32 = 17;

/// Visual fourccs (I3D.h:85-98).
pub const VISUAL_OBJECT: u32 = 0x5f4a_424f;
pub const VISUAL_LIT_OBJECT: u32 = 0x5041_4d4c;
pub const VISUAL_BILLBOARD: u32 = 0x4452_4242;
pub const VISUAL_SINGLEMESH: u32 = 0x4d47_4e53;
pub const VISUAL_LENSF: u32 = 0x534e_454c;
pub const VISUAL_MORPH: u32 = 0x4850_524d;
pub const VISUAL_SINGLEMORPH: u32 = 0x524d_4d53;
pub const VISUAL_PROJECTOR: u32 = 0x4a4f_5250;
pub const VISUAL_PART_BASE: u32 = 0x4254_5250;
pub const VISUAL_PART_ELEMENT: u32 = 0x4554_5250;
pub const VISUAL_MIRROR: u32 = 0x5252_494d;
pub const VISUAL_SHADOW: u32 = 0x0044_4853;
/// Landpatch fourcc (constructed by CreateFrame/CreateVisual, but with no
/// VisualType2String arm: formats as unknown, pinned by test).
pub const VISUAL_LANDPATCH: u32 = 0x5044_4e4c;

/// Caller-owned image of the two shared static str[64] banks. Banks are
/// independent: frame unknowns never disturb the visual bank and vice versa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverStringScratch {
    frame_buf: [u8; 64],
    frame_len: usize,
    visual_buf: [u8; 64],
    visual_len: usize,
}
impl Default for DriverStringScratch {
    fn default() -> Self {
        Self {
            frame_buf: [0; 64],
            frame_len: 0,
            visual_buf: [0; 64],
            visual_len: 0,
        }
    }
}

/// LS3DF RVAs 0x19930 and 0x19a10. Evidenced names FrameType2StringFunc (free)
/// and I3D_driver::FrameType2String (trampoline) from mafia-re
/// I3d_driver.cpp:1531-1563. Seventeen CASE_NAME arms return literals; any
/// other word formats `Unknown type:0x{:x}` (lowercase, no padding) into the
/// shared frame bank. The trampoline fold is exact: the method adds no
/// behavior of its own.
pub fn frame_type_string(scratch: &mut DriverStringScratch, type_word: u32) -> &str {
    match type_word {
        FRAME_VISUAL => "FRAME_VISUAL",
        FRAME_LIGHT => "FRAME_LIGHT",
        FRAME_CAMERA => "FRAME_CAMERA",
        FRAME_SOUND => "FRAME_SOUND",
        FRAME_SECTOR => "FRAME_SECTOR",
        FRAME_DUMMY => "FRAME_DUMMY",
        FRAME_TARGET => "FRAME_TARGET",
        FRAME_USER => "FRAME_USER",
        FRAME_MODEL => "FRAME_MODEL",
        FRAME_JOINT => "FRAME_JOINT",
        FRAME_VOLUME => "FRAME_VOLUME",
        FRAME_OCCLUDER => "FRAME_OCCLUDER",
        FRAME_SCENE => "FRAME_SCENE",
        FRAME_AREA => "FRAME_AREA",
        FRAME_SHADOW => "FRAME_SHADOW",
        FRAME_EMITOR => "FRAME_EMITOR",
        FRAME_LANDSCAPE => "FRAME_LANDSCAPE",
        _ => {
            let text = format!("Unknown type:0x{type_word:x}");
            let bytes = text.as_bytes();
            // Longest frame unknown ("Unknown type:0xffffffff") is 23 bytes,
            // far below the 64-byte bank; the assert pins the margin.
            debug_assert!(bytes.len() <= scratch.frame_buf.len());
            scratch.frame_buf[..bytes.len()].copy_from_slice(bytes);
            scratch.frame_len = bytes.len();
            std::str::from_utf8(&scratch.frame_buf[..scratch.frame_len]).unwrap_or("Unknown type:?")
        }
    }
}

/// LS3DF RVA 0x19a20. Evidenced name I3D_driver::VisualType2String from
/// mafia-re I3d_driver.cpp:1566-1593. Twelve fourcc arms return literals
/// (LANDPATCH is constructed by CreateFrame/CreateVisual but has no string
/// arm, so it formats as unknown); any other word
/// formats `Unknown type:0x{:x}  {fourcc}` into the shared visual bank, where
/// the fourcc is the little-endian type bytes cut at the first zero. High
/// (non-UTF8) bytes map via lossy conversion (disclosed: native emits the
/// raw bytes; outside the ASCII contract domain).
pub fn visual_type_string(scratch: &mut DriverStringScratch, type_word: u32) -> &str {
    match type_word {
        VISUAL_OBJECT => "I3D_VISUAL_OBJECT",
        VISUAL_LIT_OBJECT => "I3D_VISUAL_LIT_OBJECT",
        VISUAL_BILLBOARD => "I3D_VISUAL_BILLBOARD",
        VISUAL_SINGLEMESH => "I3D_VISUAL_SINGLEMESH",
        VISUAL_LENSF => "I3D_VISUAL_LENSF",
        VISUAL_MORPH => "I3D_VISUAL_MORPH",
        VISUAL_SINGLEMORPH => "I3D_VISUAL_SINGLEMORPH",
        VISUAL_PROJECTOR => "I3D_VISUAL_PROJECTOR",
        VISUAL_PART_BASE => "I3D_VISUAL_PART_BASE",
        VISUAL_PART_ELEMENT => "I3D_VISUAL_PART_ELEMENT",
        VISUAL_SHADOW => "I3D_VISUAL_SHADOW",
        VISUAL_MIRROR => "I3D_VISUAL_MIRROR",
        _ => {
            let raw = type_word.to_le_bytes();
            let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
            let fourcc = String::from_utf8_lossy(&raw[..end]).into_owned();
            let text = format!("Unknown type:0x{type_word:x}  {fourcc}");
            let bytes = text.as_bytes();
            // Longest unknown ("Unknown type:0xffffffff  xxxx") is ~28 bytes,
            // far below the 64-byte bank; the assert pins the margin.
            debug_assert!(bytes.len() <= scratch.visual_buf.len());
            scratch.visual_buf[..bytes.len()].copy_from_slice(bytes);
            scratch.visual_len = bytes.len();
            std::str::from_utf8(&scratch.visual_buf[..scratch.visual_len])
                .unwrap_or("Unknown type:?")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_known_arms_return_literals_and_banks_stay_empty() {
        let mut scratch = DriverStringScratch::default();
        let arms = [
            (FRAME_VISUAL, "FRAME_VISUAL"),
            (FRAME_LIGHT, "FRAME_LIGHT"),
            (FRAME_CAMERA, "FRAME_CAMERA"),
            (FRAME_SOUND, "FRAME_SOUND"),
            (FRAME_SECTOR, "FRAME_SECTOR"),
            (FRAME_DUMMY, "FRAME_DUMMY"),
            (FRAME_TARGET, "FRAME_TARGET"),
            (FRAME_USER, "FRAME_USER"),
            (FRAME_MODEL, "FRAME_MODEL"),
            (FRAME_JOINT, "FRAME_JOINT"),
            (FRAME_VOLUME, "FRAME_VOLUME"),
            (FRAME_OCCLUDER, "FRAME_OCCLUDER"),
            (FRAME_SCENE, "FRAME_SCENE"),
            (FRAME_AREA, "FRAME_AREA"),
            (FRAME_SHADOW, "FRAME_SHADOW"),
            (FRAME_EMITOR, "FRAME_EMITOR"),
            (FRAME_LANDSCAPE, "FRAME_LANDSCAPE"),
        ];
        for (word, name) in arms {
            assert_eq!(frame_type_string(&mut scratch, word), name);
        }
        assert_eq!(scratch, DriverStringScratch::default());
    }

    #[test]
    fn frame_unknown_formats_hex_into_shared_bank() {
        let mut scratch = DriverStringScratch::default();
        assert_eq!(frame_type_string(&mut scratch, 0), "Unknown type:0x0");
        assert_eq!(frame_type_string(&mut scratch, 18), "Unknown type:0x12");
        assert_eq!(frame_type_string(&mut scratch, 0xabc), "Unknown type:0xabc");
        assert_eq!(
            frame_type_string(&mut scratch, u32::MAX),
            "Unknown type:0xffffffff"
        );
        let owned = frame_type_string(&mut scratch, 0x99).to_owned();
        assert_eq!(owned, "Unknown type:0x99");
        assert_eq!(
            frame_type_string(&mut scratch, 0xaa),
            "Unknown type:0xaa",
            "next call overwrites the shared bank; the owned copy survives"
        );
        assert_eq!(owned, "Unknown type:0x99");
    }

    #[test]
    fn visual_known_arms_return_literals() {
        let mut scratch = DriverStringScratch::default();
        let arms = [
            (VISUAL_OBJECT, "I3D_VISUAL_OBJECT"),
            (VISUAL_LIT_OBJECT, "I3D_VISUAL_LIT_OBJECT"),
            (VISUAL_BILLBOARD, "I3D_VISUAL_BILLBOARD"),
            (VISUAL_SINGLEMESH, "I3D_VISUAL_SINGLEMESH"),
            (VISUAL_LENSF, "I3D_VISUAL_LENSF"),
            (VISUAL_MORPH, "I3D_VISUAL_MORPH"),
            (VISUAL_SINGLEMORPH, "I3D_VISUAL_SINGLEMORPH"),
            (VISUAL_PROJECTOR, "I3D_VISUAL_PROJECTOR"),
            (VISUAL_PART_BASE, "I3D_VISUAL_PART_BASE"),
            (VISUAL_PART_ELEMENT, "I3D_VISUAL_PART_ELEMENT"),
            (VISUAL_SHADOW, "I3D_VISUAL_SHADOW"),
            (VISUAL_MIRROR, "I3D_VISUAL_MIRROR"),
        ];
        for (word, name) in arms {
            assert_eq!(visual_type_string(&mut scratch, word), name);
        }
        assert_eq!(scratch, DriverStringScratch::default());
    }

    #[test]
    fn visual_unknown_appends_fourcc_bytes_cut_at_first_zero() {
        let mut scratch = DriverStringScratch::default();
        assert_eq!(
            visual_type_string(&mut scratch, 0x4142_4344),
            "Unknown type:0x41424344  DCBA"
        );
        assert_eq!(
            visual_type_string(&mut scratch, 0x0041_4243),
            "Unknown type:0x414243  CBA"
        );
        assert_eq!(visual_type_string(&mut scratch, 0), "Unknown type:0x0  ");
        assert_eq!(
            visual_type_string(&mut scratch, VISUAL_LANDPATCH),
            "Unknown type:0x50444e4c  LNDP",
            "LANDPATCH has no literal arm and formats as unknown"
        );
        let owned = visual_type_string(&mut scratch, 0x4142_4344).to_owned();
        assert_eq!(visual_type_string(&mut scratch, 0), "Unknown type:0x0  ");
        assert_eq!(owned, "Unknown type:0x41424344  DCBA");
    }

    #[test]
    fn frame_and_visual_banks_are_independent() {
        let mut scratch = DriverStringScratch::default();
        let frame = frame_type_string(&mut scratch, 0x99).to_owned();
        let visual = visual_type_string(&mut scratch, 0x4142_4344).to_owned();
        assert_eq!(frame, "Unknown type:0x99");
        assert_eq!(visual, "Unknown type:0x41424344  DCBA");
        assert_eq!(frame_type_string(&mut scratch, 0xaa), "Unknown type:0xaa");
        assert_eq!(visual, "Unknown type:0x41424344  DCBA");
    }
}
