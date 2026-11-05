#![forbid(unsafe_code)]
//! Keyboard CPU helpers, LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6 RVAs
//! 0x71910/0x71940/0x71960/0x71a10/0x71a30/0x71a50/0x71aa0/0x71af0/0x71b10/
//! 0x71b30/0x71b50/0x71c00. `IGraph::TranslateKey` and friends are evidenced
//! original methods; contract key is SHA+RVA, never the name. Pinned
//! mafia-re ab67b13 `ls3df/IGraph/IGraph.cpp:2800-2967`. Device acquisition
//! (`UpdateKeyboardData`, `GetShiftKeys(true)`) is withheld: this module owns
//! the CPU state and pure transitions only.

/// `KBINIT_WINMESSAGES` (IGraph.h:162).
pub const KBINIT_WINMESSAGES: u32 = 0x2;
/// `I3D_OK`.
pub const KB_OK: i32 = 0;
/// `I3DERR_INVALIDPARAMS`.
pub const KB_INVALIDPARAMS: i32 = 0x80000007u32 as i32;
/// `SKEY_SHIFT`.
pub const SKEY_SHIFT: u32 = 1;
/// `SKEY_CTRL`.
pub const SKEY_CTRL: u32 = 4;
/// `SKEY_ALT`: u32 = 8.
pub const SKEY_ALT: u32 = 8;
/// `SKEY_LSHIFT`.
pub const SKEY_LSHIFT: u32 = 0x20;
/// `SKEY_RSHIFT`.
pub const SKEY_RSHIFT: u32 = 0x40;
/// `SKEY_LCTRL`.
pub const SKEY_LCTRL: u32 = 0x80;
/// `SKEY_RCTRL`.
pub const SKEY_RCTRL: u32 = 0x100;
/// `SKEY_LALT`.
pub const SKEY_LALT: u32 = 0x200;
/// `SKEY_RALT`.
pub const SKEY_RALT: u32 = 0x400;
/// DirectInput scan codes used here.
pub const DIK_LCONTROL: usize = 0x1d;
pub const DIK_RCONTROL: usize = 0x9d;
pub const DIK_LSHIFT: usize = 0x2a;
pub const DIK_RSHIFT: usize = 0x36;
pub const DIK_LMENU: usize = 0x38;
pub const DIK_RMENU: usize = 0xb8;

/// CPU keyboard state projecting the original globals (`m_KeyboardData`,
/// `m_OldKeyboardData`, `m_ReadKeysBuffer`, `m_KeyBuffer`, `m_KBLastKey`,
/// `m_KBLastChar`, `m_KBLastChar1`, `m_KBInitFlags`). Device handles stay
/// outside; callers feed `keyboard_data`/`old_keyboard_data` via the withheld
/// `UpdateKeyboardData` boundary before calling these pure helpers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardState {
    pub keyboard_data: [u8; 256],
    pub old_keyboard_data: [u8; 256],
    pub read_keys_buffer: [u8; 64],
    pub key_buffer: [u32; 32],
    pub last_key: u32,
    pub last_char: u32,
    pub last_char1: u32,
    pub init_flags: u32,
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self {
            keyboard_data: [0; 256],
            old_keyboard_data: [0; 256],
            read_keys_buffer: [0; 64],
            key_buffer: [0; 32],
            last_key: 0,
            last_char: 0,
            last_char1: 0,
            init_flags: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KeyChar {
    key: u8,
    c: u8,
    shift_c: u8,
}

/// `key_table` (IGraph.cpp:428-493) in source order, sentinel `{0,0,0}`
/// excluded (scan ends at the slice end instead).
const KEY_TABLE: &[KeyChar] = &[
    KeyChar {
        key: 0x39,
        c: b' ',
        shift_c: b' ',
    },
    KeyChar {
        key: 0x0b,
        c: b'0',
        shift_c: b')',
    },
    KeyChar {
        key: 0x02,
        c: b'1',
        shift_c: b'!',
    },
    KeyChar {
        key: 0x03,
        c: b'2',
        shift_c: b'@',
    },
    KeyChar {
        key: 0x04,
        c: b'3',
        shift_c: b'#',
    },
    KeyChar {
        key: 0x05,
        c: b'4',
        shift_c: b'$',
    },
    KeyChar {
        key: 0x06,
        c: b'5',
        shift_c: b'%',
    },
    KeyChar {
        key: 0x07,
        c: b'6',
        shift_c: b'^',
    },
    KeyChar {
        key: 0x08,
        c: b'7',
        shift_c: b'&',
    },
    KeyChar {
        key: 0x09,
        c: b'8',
        shift_c: b'*',
    },
    KeyChar {
        key: 0x0a,
        c: b'9',
        shift_c: b'(',
    },
    KeyChar {
        key: 0x1e,
        c: b'a',
        shift_c: b'A',
    },
    KeyChar {
        key: 0x30,
        c: b'b',
        shift_c: b'B',
    },
    KeyChar {
        key: 0x2e,
        c: b'c',
        shift_c: b'C',
    },
    KeyChar {
        key: 0x20,
        c: b'd',
        shift_c: b'D',
    },
    KeyChar {
        key: 0x12,
        c: b'e',
        shift_c: b'E',
    },
    KeyChar {
        key: 0x21,
        c: b'f',
        shift_c: b'F',
    },
    KeyChar {
        key: 0x22,
        c: b'g',
        shift_c: b'G',
    },
    KeyChar {
        key: 0x23,
        c: b'h',
        shift_c: b'H',
    },
    KeyChar {
        key: 0x17,
        c: b'i',
        shift_c: b'I',
    },
    KeyChar {
        key: 0x24,
        c: b'j',
        shift_c: b'J',
    },
    KeyChar {
        key: 0x25,
        c: b'k',
        shift_c: b'K',
    },
    KeyChar {
        key: 0x26,
        c: b'l',
        shift_c: b'L',
    },
    KeyChar {
        key: 0x32,
        c: b'm',
        shift_c: b'M',
    },
    KeyChar {
        key: 0x31,
        c: b'n',
        shift_c: b'N',
    },
    KeyChar {
        key: 0x18,
        c: b'o',
        shift_c: b'O',
    },
    KeyChar {
        key: 0x19,
        c: b'p',
        shift_c: b'P',
    },
    KeyChar {
        key: 0x10,
        c: b'q',
        shift_c: b'Q',
    },
    KeyChar {
        key: 0x13,
        c: b'r',
        shift_c: b'R',
    },
    KeyChar {
        key: 0x1f,
        c: b's',
        shift_c: b'S',
    },
    KeyChar {
        key: 0x14,
        c: b't',
        shift_c: b'T',
    },
    KeyChar {
        key: 0x16,
        c: b'u',
        shift_c: b'U',
    },
    KeyChar {
        key: 0x2f,
        c: b'v',
        shift_c: b'V',
    },
    KeyChar {
        key: 0x11,
        c: b'w',
        shift_c: b'W',
    },
    KeyChar {
        key: 0x2d,
        c: b'x',
        shift_c: b'X',
    },
    KeyChar {
        key: 0x15,
        c: b'y',
        shift_c: b'Y',
    },
    KeyChar {
        key: 0x2c,
        c: b'z',
        shift_c: b'Z',
    },
    KeyChar {
        key: 0x52,
        c: b'0',
        shift_c: b'0',
    },
    KeyChar {
        key: 0x4f,
        c: b'1',
        shift_c: b'1',
    },
    KeyChar {
        key: 0x50,
        c: b'2',
        shift_c: b'2',
    },
    KeyChar {
        key: 0x51,
        c: b'3',
        shift_c: b'3',
    },
    KeyChar {
        key: 0x4b,
        c: b'4',
        shift_c: b'4',
    },
    KeyChar {
        key: 0x4c,
        c: b'5',
        shift_c: b'5',
    },
    KeyChar {
        key: 0x4d,
        c: b'6',
        shift_c: b'6',
    },
    KeyChar {
        key: 0x47,
        c: b'7',
        shift_c: b'7',
    },
    KeyChar {
        key: 0x48,
        c: b'8',
        shift_c: b'8',
    },
    KeyChar {
        key: 0x49,
        c: b'9',
        shift_c: b'9',
    },
    KeyChar {
        key: 0x37,
        c: b'*',
        shift_c: b'*',
    },
    KeyChar {
        key: 0x4e,
        c: b'+',
        shift_c: b'+',
    },
    KeyChar {
        key: 0x4a,
        c: b'-',
        shift_c: b'-',
    },
    KeyChar {
        key: 0x53,
        c: b'.',
        shift_c: b'.',
    },
    KeyChar {
        key: 0xb5,
        c: b'/',
        shift_c: b'/',
    },
    KeyChar {
        key: 0x27,
        c: b';',
        shift_c: b':',
    },
    KeyChar {
        key: 0x0d,
        c: b'=',
        shift_c: b'+',
    },
    KeyChar {
        key: 0x33,
        c: b',',
        shift_c: b'<',
    },
    KeyChar {
        key: 0x0c,
        c: b'-',
        shift_c: b'_',
    },
    KeyChar {
        key: 0x34,
        c: b'.',
        shift_c: b'>',
    },
    KeyChar {
        key: 0x35,
        c: b'/',
        shift_c: b'?',
    },
    KeyChar {
        key: 0x29,
        c: b'`',
        shift_c: b'~',
    },
    KeyChar {
        key: 0x1a,
        c: b'[',
        shift_c: b'{',
    },
    KeyChar {
        key: 0x2b,
        c: b'\\',
        shift_c: b'|',
    },
    KeyChar {
        key: 0x1b,
        c: b']',
        shift_c: b'}',
    },
    KeyChar {
        key: 0x28,
        c: b'\'',
        shift_c: b'"',
    },
];

/// LS3DF RVA 0x71c00 `IGraph::TranslateKey`
/// (mafia-re ls3df/IGraph/IGraph.cpp:2942-2953). Scans `key_table` to the `0`
/// sentinel; shifted with `SKEY_SHIFT`. Unknown keys yield 0.
pub fn translate_key(key: u8, shift_keys: u32) -> u8 {
    for row in KEY_TABLE {
        if row.key == key {
            return if shift_keys & SKEY_SHIFT != 0 {
                row.shift_c
            } else {
                row.c
            };
        }
    }
    0
}

/// LS3DF RVA 0x71af0 `IGraph::KeyShift` (IGraph.cpp:2870-2881).
pub fn key_shift(state: &KeyboardState) -> bool {
    (state.keyboard_data[DIK_LSHIFT] & 0x80) != 0 || (state.keyboard_data[DIK_RSHIFT] & 0x80) != 0
}

/// LS3DF RVA 0x71b10 `IGraph::KeyCtrl` (IGraph.cpp:2891-2900).
pub fn key_ctrl(state: &KeyboardState) -> bool {
    (state.keyboard_data[DIK_LCONTROL] & 0x80) != 0
        || (state.keyboard_data[DIK_RCONTROL] & 0x80) != 0
}

/// LS3DF RVA 0x71b30 `IGraph::KeyAlt` (IGraph.cpp:2900-2909).
pub fn key_alt(state: &KeyboardState) -> bool {
    (state.keyboard_data[DIK_LMENU] & 0x80) != 0 || (state.keyboard_data[DIK_RMENU] & 0x80) != 0
}

/// LS3DF RVA 0x71b50 `IGraph::GetShiftKeys`
/// (mafia-re ls3df/IGraph/IGraph.cpp:2910-2941). `SKEY_*` plus the left/right
/// key own bits from the `0x80` DIK bits. The `update=true` device refresh
/// (`UpdateKeyboardData`) is withheld; callers feed fresh `keyboard_data` and
/// call this `update=false` projection.
pub fn get_shift_keys(state: &KeyboardState) -> u32 {
    let mut keys: u32 = 0;
    if key_shift(state) {
        keys = SKEY_SHIFT;
        if (state.keyboard_data[DIK_LSHIFT] & 0x80) != 0 {
            keys |= SKEY_LSHIFT;
        }
        if (state.keyboard_data[DIK_RSHIFT] & 0x80) != 0 {
            keys |= SKEY_RSHIFT;
        }
    }
    if key_ctrl(state) {
        keys |= SKEY_CTRL;
        if (state.keyboard_data[DIK_LCONTROL] & 0x80) != 0 {
            keys |= SKEY_LCTRL;
        }
        if (state.keyboard_data[DIK_RCONTROL] & 0x80) != 0 {
            keys |= SKEY_RCTRL;
        }
    }
    if key_alt(state) {
        keys |= SKEY_ALT;
        if (state.keyboard_data[DIK_LMENU] & 0x80) != 0 {
            keys |= SKEY_LALT;
        }
        if (state.keyboard_data[DIK_RMENU] & 0x80) != 0 {
            keys |= SKEY_RALT;
        }
    }
    keys
}

/// LS3DF RVA 0x71a30 `IGraph::TestKey` (IGraph.cpp:2850-2857). `u8` shift of
/// the key byte; nonzero down bit reads as true.
pub fn test_key(state: &KeyboardState, key: u8) -> bool {
    (state.keyboard_data[key as usize] >> 7) != 0
}

/// LS3DF RVA 0x71a50 `IGraph::DefineKeys` (IGraph.cpp:2858-2869). Copies at
/// most 64 watch keys; NULL (`None`) or `count > 64` is `INVALIDPARAMS`.
/// The native `count` argument is folded into `keys.len()` (overlong is
/// caller-side rejected).
pub fn define_keys(state: &mut KeyboardState, keys: Option<&[u8]>) -> i32 {
    let Some(keys) = keys else {
        return KB_INVALIDPARAMS;
    };
    if keys.len() > 64 {
        return KB_INVALIDPARAMS;
    }
    state.read_keys_buffer = [0; 64];
    state.read_keys_buffer[..keys.len()].copy_from_slice(keys);
    KB_OK
}

/// LS3DF RVA 0x71aa0 `IGraph::ReadKeys` (IGraph.cpp:2870-2882). One bit per
/// `DefineKeys` entry that is down. `ulong` is 32-bit on x86, so shifts wrap
/// modulo 32 (`wrapping_shl`), aliasing entries 32..63 onto 0..31.
pub fn read_keys_mask(state: &KeyboardState) -> u32 {
    let mut keys: u32 = 0;
    for i in 0..64 {
        if state.read_keys_buffer[i] > 0
            && (state.keyboard_data[state.read_keys_buffer[i] as usize] & 0x80) != 0
        {
            keys |= 1u32.wrapping_shl(i as u32);
        }
    }
    keys
}

/// LS3DF RVA 0x71960 `IGraph::ReadKey` (IGraph.cpp:2818-2840). `KBINIT`
/// branch serves the window-message key; otherwise the 256-scan returns the
/// first newly-down non-modifier and records its `TranslateKey` character.
pub fn read_key(state: &mut KeyboardState) -> u32 {
    if state.init_flags & KBINIT_WINMESSAGES != 0 {
        let key = state.last_key;
        state.last_key = 0;
        state.last_char = state.last_char1;
        state.last_char1 = 0;
        return key;
    }
    for i in 0..256 {
        if (state.keyboard_data[i] & 0x80) == 0 || state.old_keyboard_data[i] != 0 {
            continue;
        }
        if i == DIK_LCONTROL
            || i == DIK_RCONTROL
            || i == DIK_LSHIFT
            || i == DIK_RSHIFT
            || i == DIK_LMENU
            || i == DIK_RMENU
        {
            continue;
        }
        let shift = get_shift_keys(state) & (SKEY_SHIFT | SKEY_CTRL | SKEY_ALT);
        state.last_char = u32::from(translate_key(i as u8, shift));
        return i as u32;
    }
    0
}

/// LS3DF RVA 0x71a10 `IGraph::ReadChar` (IGraph.cpp:2841-2850).
/// Consumes and clears the pending character.
pub fn read_char(state: &mut KeyboardState) -> u32 {
    let c = state.last_char;
    state.last_char = 0;
    c
}

/// LS3DF RVA 0x71910 `IGraph::ReadCharBuffer` (IGraph.cpp:2800-2809).
/// Shifts the 32-entry buffer left by one (`memmove`), clears the tail, and
/// returns the new head.
pub fn read_char_buffer(state: &mut KeyboardState) -> u32 {
    state.key_buffer.copy_within(1.., 0);
    state.key_buffer[31] = 0;
    state.key_buffer[0]
}

/// LS3DF RVA 0x71940 `IGraph::ClearCharBuffer` (IGraph.cpp:2809-2818).
pub fn clear_char_buffer(state: &mut KeyboardState) {
    state.key_buffer = [0; 32];
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translate_scans_to_sentinel_with_shift_select() {
        assert_eq!(translate_key(0x1e, 0), b'a');
        assert_eq!(translate_key(0x1e, SKEY_SHIFT), b'A');
        assert_eq!(translate_key(0x02, SKEY_SHIFT), b'!');
        assert_eq!(translate_key(0xb5, 0), b'/');
        assert_eq!(translate_key(0x99, SKEY_SHIFT), 0);
    }
    #[test]
    fn shift_keys_combine_base_and_sided_bits() {
        let mut s = KeyboardState::default();
        assert_eq!(get_shift_keys(&s), 0);
        s.keyboard_data[DIK_LSHIFT] = 0x80;
        s.keyboard_data[DIK_RCONTROL] = 0x80;
        s.keyboard_data[DIK_RMENU] = 0x80;
        assert_eq!(
            get_shift_keys(&s),
            SKEY_SHIFT | SKEY_LSHIFT | SKEY_CTRL | SKEY_RCTRL | SKEY_ALT | SKEY_RALT
        );
        assert!(key_shift(&s));
        assert!(key_ctrl(&s));
        assert!(key_alt(&s));
        assert!(test_key(&s, DIK_LSHIFT as u8));
        assert!(!test_key(&s, 0x10));
    }
    #[test]
    fn define_and_read_keys_mask_with_32bit_wrap() {
        let mut s = KeyboardState::default();
        assert_eq!(define_keys(&mut s, None), KB_INVALIDPARAMS);
        assert_eq!(define_keys(&mut s, Some(&[0u8; 65])), KB_INVALIDPARAMS);
        assert_eq!(define_keys(&mut s, Some(&[0x1e, 0x30])), KB_OK);
        s.keyboard_data[0x1e] = 0x80;
        assert_eq!(read_keys_mask(&s), 1);
        let mut wide = [0u8; 64];
        wide[0] = 0x1e;
        wide[32] = 0x1e;
        assert_eq!(define_keys(&mut s, Some(&wide)), KB_OK);
        assert_eq!(read_keys_mask(&s), 1);
    }
    #[test]
    fn read_key_winmessages_branch_and_scan_skip_modifiers() {
        let mut s = KeyboardState {
            init_flags: KBINIT_WINMESSAGES,
            last_key: 0xcb,
            last_char1: b'x' as u32,
            ..Default::default()
        };
        assert_eq!(read_key(&mut s), 0xcb);
        assert_eq!((s.last_key, s.last_char, s.last_char1), (0, 0x78, 0));
        assert_eq!(read_char(&mut s), 0x78);
        assert_eq!(read_char(&mut s), 0);
        let mut t = KeyboardState::default();
        t.keyboard_data[DIK_LSHIFT] = 0x80;
        t.keyboard_data[0x1e] = 0x80;
        assert_eq!(read_key(&mut t), 0x1e);
        // Shift held: TranslateKey yields 'A', not 'a'.
        assert_eq!(t.last_char, u32::from(b'A'));
        t.keyboard_data[0x1e] = 0x80;
        t.old_keyboard_data[0x1e] = 0x80;
        assert_eq!(read_key(&mut t), 0);
    }
    #[test]
    fn char_buffer_shifts_and_clears() {
        let mut s = KeyboardState::default();
        s.key_buffer[0] = 11;
        s.key_buffer[1] = 22;
        s.key_buffer[31] = 33;
        assert_eq!(read_char_buffer(&mut s), 22);
        assert_eq!(s.key_buffer[30], 33);
        assert_eq!(s.key_buffer[31], 0);
        clear_char_buffer(&mut s);
        assert_eq!(s.key_buffer, [0; 32]);
    }
}
