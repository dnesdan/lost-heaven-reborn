//! Game text database owner (`G_TextDatabase`), marker-only contract scope.
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000; addresses below are GAME RVAs. Reference: mafia-re 113b5b6
//! `game/ui/g_textdb.h:9-33` (class, `S_text`, `data` 0x0 / `num_texts` 0x4 /
//! `texts` 0x8, method declarations) and `game/ui/g_textdb.cpp:10-104`
//! (global, ctor, dtor, `LoadDatabase`, `Unload`, `GetText`,
//! `GetTextOrNULL`).
//!
//! Owned here: the whole-file image plus the parsed `(id, file-offset)` table
//! and the sorted-id binary search (`GetText` GAME 0x60fb40,
//! `g_textdb.cpp:58-81`; `GetTextOrNULL` GAME 0x60fba0, `g_textdb.cpp:83-104`).
//! The search starts at step 0x1000, halves down while above the count, probes
//! the table end first and then scans with `<=`; `GetText` takes the end
//! branch on `texts[step].id < id` (`g_textdb.cpp:68`) while `GetTextOrNULL`
//! takes it on `<=` (`g_textdb.cpp:92`). Both return on exact id match;
//! `GetText` falls back to `" "` (GAME string 0x638390, `g_textdb.cpp:79-80`),
//! `GetTextOrNULL` to null. The two probe spellings agree on every present id;
//! only the search path differs.
//!
//! `new` zeroes all three words; the native ctor (GAME 0x60fa40,
//! `g_textdb.cpp:13-16`) only nulls `data` and leaves the rest for
//! `LoadDatabase`. `reset` mirrors the destructor body (GAME 0x60fa50,
//! `g_textdb.cpp:18-23`): drop the image, repeat calls are safe.
//!
//! DEFERRED with explicit markers (no credit): real file intake through the
//! `dtaOpen`/`dtaRead`/`dtaClose` path (`LoadDatabase` GAME 0x60fa60,
//! `g_textdb.cpp:26-47` takes a file name and turns the stored offsets into
//! pointers); the host supplies the already-read `.def` image to
//! [`TextDatabase::load_from_bytes`]. The `textDatabase` global singleton
//! (GAME 0x6d8714, `g_textdb.cpp:10-11`) and its `LanguageSelect` /
//! `CloseSystem` call sites stay with the host (mission lifecycle owner).
//! Transpose does not exist on PC (`g_textdb.cpp:1-3`).

#![forbid(unsafe_code)]

/// First binary-search probe step (GAME 0x60fb40/0x60fba0,
/// mafia-re `game/ui/g_textdb.cpp:63,87`).
pub const TEXT_SEARCH_START_STEP: u32 = 0x1000;
/// `GetText` miss fallback (GAME string 0x638390,
/// mafia-re `game/ui/g_textdb.cpp:79-80`).
pub const TEXT_MISSING: &str = " ";
/// `.def` header size: count dword plus the padding dword the table follows
/// (table lives at `data + 8`, mafia-re `game/ui/g_textdb.cpp:43`).
pub const TEXT_DEF_HEADER_LEN: usize = 8;
/// One table entry: id dword plus file-offset dword
/// (mafia-re `game/ui/g_textdb.h:12-17`).
pub const TEXT_DEF_ENTRY_LEN: usize = 8;

/// One text id with its absolute file offset into the `.def` image
/// (`S_text`, mafia-re `game/ui/g_textdb.h:12-17`; offsets become pointers in
/// `LoadDatabase`, `g_textdb.cpp:44-45`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextEntry {
    pub id: u32,
    pub offset: u32,
}

/// Typed owner of the `G_TextDatabase` state (mafia-re
/// `game/ui/g_textdb.h:9-22`): the whole `.def` file plus the entry table.
/// Offsets stay file-relative; no host pointers are fabricated.
#[derive(Clone, Debug, Default)]
pub struct TextDatabase {
    data: Option<Vec<u8>>,
    num_texts: u32,
    texts: Vec<TextEntry>,
}

impl TextDatabase {
    /// Nulled state (`data`/`num_texts`/`texts`); native ctor GAME 0x60fa40
    /// (mafia-re `game/ui/g_textdb.cpp:13-16`).
    pub fn new() -> Self {
        Self {
            data: None,
            num_texts: 0,
            texts: Vec::new(),
        }
    }

    /// Drop the image and empty the table; destructor body GAME 0x60fa50
    /// (mafia-re `game/ui/g_textdb.cpp:18-23`). Repeat calls are safe.
    pub fn reset(&mut self) {
        self.data = None;
        self.num_texts = 0;
        self.texts.clear();
    }

    /// Parse an already-read `textdb_<language>.def` image: count dword at
    /// +0, `(id, offset)` table at +8, NUL-terminated texts after it
    /// (`LoadDatabase` GAME 0x60fa60, mafia-re `game/ui/g_textdb.cpp:26-47`).
    /// The `!name` early-false path (`g_textdb.cpp:28-29`) has no filename
    /// here: a short/truncated image returns false instead. Previous data is
    /// dropped first (`g_textdb.cpp:33-36`), so failure leaves an empty DB.
    /// The host keeps ids ascending, as the file does and the search assumes.
    pub fn load_from_bytes(&mut self, bytes: &[u8]) -> bool {
        self.reset();
        if bytes.len() < TEXT_DEF_HEADER_LEN {
            return false;
        }
        let num = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let table_len = (num as usize).checked_mul(TEXT_DEF_ENTRY_LEN);
        let table_end = table_len.and_then(|len| TEXT_DEF_HEADER_LEN.checked_add(len));
        let Some(table_end) = table_end else {
            return false;
        };
        if table_end > bytes.len() {
            return false;
        }
        let mut texts = Vec::with_capacity(num as usize);
        for i in 0..num {
            let base = TEXT_DEF_HEADER_LEN + (i as usize) * TEXT_DEF_ENTRY_LEN;
            let id = u32::from_le_bytes([
                bytes[base],
                bytes[base + 1],
                bytes[base + 2],
                bytes[base + 3],
            ]);
            let offset = u32::from_le_bytes([
                bytes[base + 4],
                bytes[base + 5],
                bytes[base + 6],
                bytes[base + 7],
            ]);
            if (offset as usize) >= bytes.len() {
                self.reset();
                return false;
            }
            // The format stores NUL-terminated texts; a truncation that cuts
            // a text body (or its terminator) is rejected instead of reading
            // a silently unterminated tail.
            if !bytes[offset as usize..].contains(&0) {
                self.reset();
                return false;
            }
            texts.push(TextEntry { id, offset });
        }
        self.data = Some(bytes.to_vec());
        self.num_texts = num;
        self.texts = texts;
        true
    }

    /// Free the file (`Unload` GAME 0x60fb10, mafia-re
    /// `game/ui/g_textdb.cpp:49-56`): image dropped, `num_texts` zeroed.
    pub fn unload(&mut self) {
        self.reset();
    }

    /// Text of an id, `" "` for an id the DB lacks (`GetText` GAME 0x60fb40,
    /// mafia-re `game/ui/g_textdb.cpp:60-81`).
    pub fn get_text(&self, id: u32) -> &str {
        match self.find_index(id, false) {
            Some(index) => self.text_at(index),
            None => TEXT_MISSING,
        }
    }

    /// Text of an id, `None` for an id the DB lacks (`GetTextOrNULL` GAME
    /// 0x60fba0, mafia-re `game/ui/g_textdb.cpp:85-104`).
    pub fn get_text_or_null(&self, id: u32) -> Option<&str> {
        self.find_index(id, true).map(|index| self.text_at(index))
    }

    /// Entry count (`num_texts` at 0x4, mafia-re `game/ui/g_textdb.h:20`).
    pub fn num_texts(&self) -> u32 {
        self.num_texts
    }

    /// Whether a `.def` image is held.
    pub fn is_loaded(&self) -> bool {
        self.data.is_some()
    }

    /// Table entry by position (file order, ascending ids).
    pub fn entry(&self, index: usize) -> Option<&TextEntry> {
        self.texts.get(index)
    }

    /// Power-of-two binary search over the ascending ids (mafia-re
    /// `game/ui/g_textdb.cpp:58-68` / `:83-92`). `end_on_equal` selects the
    /// end-branch probe spelling: `<` for `GetText`, `<=` for
    /// `GetTextOrNULL`. Indexing is bounds-checked: when `num_texts` is an
    /// exact power of two the native `texts[step]` probe reads one past the
    /// table, so the hardened port takes the miss branch instead. The native
    /// first probe only routes `pos` and never returns the probed row, so a
    /// present id sitting exactly at `step` is missed natively; the hardened
    /// port returns it on equality first. Absent-id behavior is unchanged.
    fn find_index(&self, id: u32, end_on_equal: bool) -> Option<usize> {
        let num = self.num_texts;
        let mut step: u32 = TEXT_SEARCH_START_STEP;
        while step != 0 && step > num {
            step >>= 1;
        }
        if step == 0 {
            return None;
        }
        let mut pos: u32 = if step < num {
            let probe = self.texts.get(step as usize)?.id;
            if probe == id {
                return Some(step as usize);
            }
            let take_end = if end_on_equal {
                probe <= id
            } else {
                probe < id
            };
            if take_end { num - step - 1 } else { 0 }
        } else {
            0
        };
        step >>= 1;
        while step != 0 {
            let i = pos + step;
            step >>= 1;
            if let Some(entry) = self.texts.get(i as usize) {
                if entry.id <= id {
                    pos = i;
                }
            }
        }
        let hit = self.texts.get(pos as usize)?;
        if hit.id == id { Some(pos as usize) } else { None }
    }

    /// NUL-terminated text at a table position; undecodable bytes read as
    /// empty, matching the ASCII fixture domain.
    fn text_at(&self, index: usize) -> &str {
        let offset = self
            .texts
            .get(index)
            .map(|entry| entry.offset as usize)
            .unwrap_or(usize::MAX);
        let bytes = self
            .data
            .as_ref()
            .and_then(|data| data.get(offset..))
            .unwrap_or(&[]);
        let len = bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(bytes.len());
        std::str::from_utf8(&bytes[..len]).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal `.def` image: count dword, padding dword, `(id, offset)` pairs,
    /// then NUL-terminated texts (mafia-re `game/ui/g_textdb.cpp:26-47`).
    fn make_def(entries: &[(u32, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let mut next = TEXT_DEF_HEADER_LEN + entries.len() * TEXT_DEF_ENTRY_LEN;
        for (id, text) in entries {
            bytes.extend_from_slice(&id.to_le_bytes());
            bytes.extend_from_slice(&(next as u32).to_le_bytes());
            next += text.len() + 1;
        }
        for (_, text) in entries {
            bytes.extend_from_slice(text.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    #[test]
    fn new_starts_empty() {
        let db = TextDatabase::new();
        assert!(!db.is_loaded());
        assert_eq!(db.num_texts(), 0);
        assert_eq!(db.entry(0), None);
        assert_eq!(db.get_text(1), " ");
        assert_eq!(db.get_text_or_null(1), None);
    }

    #[test]
    fn reset_empties_and_double_reset_is_safe() {
        let mut db = TextDatabase::new();
        assert!(db.load_from_bytes(&make_def(&[(10, "ten"), (20, "twenty")])));
        db.reset();
        assert!(!db.is_loaded());
        assert_eq!(db.num_texts(), 0);
        assert_eq!(db.get_text(10), " ");
        db.reset();
        assert!(!db.is_loaded());
        assert_eq!(db.num_texts(), 0);
    }

    #[test]
    fn load_parses_count_and_entry_offsets() {
        let mut db = TextDatabase::new();
        let image = make_def(&[(7, "seven"), (42, "forty-two"), (100, "hundred")]);
        assert!(db.load_from_bytes(&image));
        assert!(db.is_loaded());
        assert_eq!(db.num_texts(), 3);
        assert_eq!(
            db.entry(1),
            Some(&TextEntry {
                id: 42,
                offset: (TEXT_DEF_HEADER_LEN + 3 * TEXT_DEF_ENTRY_LEN + "seven".len() + 1)
                    as u32,
            })
        );
        assert_eq!(db.get_text(42), "forty-two");
        assert_eq!(db.get_text_or_null(100), Some("hundred"));

        let mut short = TextDatabase::new();
        assert!(!short.load_from_bytes(&image[..image.len() - 4]));
        assert!(!short.is_loaded());
        assert_eq!(short.num_texts(), 0);
        let mut tiny = TextDatabase::new();
        assert!(!tiny.load_from_bytes(&[1, 0, 0]));
        assert!(!tiny.is_loaded());
    }

    #[test]
    fn unload_releases_and_zeroes_count() {
        let mut db = TextDatabase::new();
        assert!(db.load_from_bytes(&make_def(&[(5, "five")])));
        db.unload();
        assert!(!db.is_loaded());
        assert_eq!(db.num_texts(), 0);
        assert_eq!(db.get_text(5), " ");
        assert_eq!(db.get_text_or_null(5), None);
        db.unload();
        assert!(!db.is_loaded());
    }

    #[test]
    fn get_text_hit_miss_and_boundaries() {
        let mut db = TextDatabase::new();
        assert!(db.load_from_bytes(&make_def(&[
            (10, "ten"),
            (20, "twenty"),
            (30, "thirty"),
            (40, "forty"),
            (50, "fifty"),
        ])));
        assert_eq!(db.get_text(10), "ten");
        assert_eq!(db.get_text(30), "thirty");
        assert_eq!(db.get_text(50), "fifty");
        assert_eq!(db.get_text(9), " ");
        assert_eq!(db.get_text(25), " ");
        assert_eq!(db.get_text(51), " ");
        assert_eq!(db.get_text(u32::MAX), " ");

        // Exact power-of-two count: the native `texts[step]` probe would read
        // past the table, the hardened port still resolves every id.
        let mut pow2 = TextDatabase::new();
        assert!(pow2.load_from_bytes(&make_def(&[
            (1, "one"),
            (2, "two"),
            (3, "three"),
            (4, "four"),
        ])));
        assert_eq!(pow2.get_text(1), "one");
        assert_eq!(pow2.get_text(2), "two");
        assert_eq!(pow2.get_text(3), "three");
        assert_eq!(pow2.get_text(4), "four");
        assert_eq!(pow2.get_text(5), " ");
    }

    #[test]
    fn get_text_or_null_none_vs_hit() {
        let mut db = TextDatabase::new();
        assert!(db.load_from_bytes(&make_def(&[(10, "ten"), (20, "twenty")])));
        assert_eq!(db.get_text_or_null(10), Some("ten"));
        assert_eq!(db.get_text_or_null(20), Some("twenty"));
        assert_eq!(db.get_text_or_null(9), None);
        assert_eq!(db.get_text_or_null(15), None);
        assert_eq!(db.get_text_or_null(21), None);
    }
}
