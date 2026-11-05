#![forbid(unsafe_code)]
//! Map-directory list, LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6 RVAs
//! 0x723e0/0x725f0/0x72610/0x72670. Names `m_AddNameToList`/`FindMapFile` are
//! upstream working names (NOT original); `IGraph::AddMapsDir` and friends
//! are evidenced original methods. Contract key is SHA+RVA, never the name.
//! Pinned mafia-re ab67b13 `ls3df/IGraph/IGraph.cpp:3229-3290`.

/// `I3D_OK`.
pub const MAPS_OK: i32 = 0;
/// `I3DERR_OUTOFMEM`.
pub const MAPS_OUTOFMEM: i32 = 0x80000002u32 as i32;
/// `I3DERR_INVALIDPARAMS` for the host empty-name guard (native reads
/// `name[-1]` out of bounds there; host rejects instead of faulting).
pub const MAPS_INVALIDPARAMS: i32 = 0x80000007u32 as i32;
/// Native `char path[256]` scratch.
pub const MAPS_PATH_CAP: usize = 256;

/// Owned map directories, each stored with a trailing backslash exactly as
/// the original stores its `len+2` copy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapsDirs {
    entries: Vec<Vec<u8>>,
}

impl MapsDirs {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn entries(&self) -> &[Vec<u8>] {
        &self.entries
    }
}

/// One directory enumeration row for the recursive walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// `cFileName` bytes only (no path).
    pub name: Vec<u8>,
    /// `dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0`.
    pub is_dir: bool,
}

/// `FindFirstFile`/`FindNextFile` enumeration behind the recursive walk.
/// `None` is `INVALID_HANDLE_VALUE` (clean `I3D_OK` after the push, with the
/// native log line withheld here). Otherwise the rows of that directory.
pub trait DirLister {
    fn list_subdirs(&mut self, dir: &[u8]) -> Option<Vec<DirEntry>>;
}

fn with_trailing_backslash(name: &[u8]) -> Option<Vec<u8>> {
    if name.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let extra = if name.last() == Some(&b'\\') { 0 } else { 1 };
    let need = name.len().checked_add(extra)?;
    if out.try_reserve_exact(need).is_err() {
        return None;
    }
    out.extend_from_slice(name);
    if extra == 1 {
        out.push(b'\\');
    }
    Some(out)
}

fn join_search_pattern(dir: &[u8]) -> Option<Vec<u8>> {
    let suffix = b"\\*.*";
    let need = dir.len().checked_add(suffix.len())?;
    if need > MAPS_PATH_CAP - 1 {
        return None;
    }
    let mut out = Vec::with_capacity(need);
    out.extend_from_slice(dir);
    out.extend_from_slice(suffix);
    Some(out)
}

fn join_subdir(dir: &[u8], leaf: &[u8]) -> Option<Vec<u8>> {
    let need = dir.len().checked_add(1)?.checked_add(leaf.len())?;
    if need > MAPS_PATH_CAP - 1 {
        return None;
    }
    let mut out = Vec::with_capacity(need);
    out.extend_from_slice(dir);
    out.push(b'\\');
    out.extend_from_slice(leaf);
    Some(out)
}

/// LS3DF RVA 0x723e0 `m_AddNameToList` (upstream working name, NOT original)
/// (mafia-re ls3df/IGraph/IGraph.cpp:3229-3267). `len+2` copy with a
/// backslash terminator is pushed first; with `recursive` each subdirectory
/// (skipping dot names) is added depth-first. Enumeration failure after the
/// push is still `MAPS_OK`.
pub fn add_name_to_list(
    dirs: &mut MapsDirs,
    name: &[u8],
    recursive: bool,
    lister: &mut impl DirLister,
) -> i32 {
    if name.is_empty() {
        return MAPS_INVALIDPARAMS;
    }
    let Some(stored) = with_trailing_backslash(name) else {
        return MAPS_OUTOFMEM;
    };
    if dirs.entries.try_reserve(1).is_err() {
        return MAPS_OUTOFMEM;
    }
    dirs.entries.push(stored);
    if !recursive {
        return MAPS_OK;
    }
    if join_search_pattern(name).is_none() {
        return MAPS_OK;
    }
    let Some(rows) = lister.list_subdirs(name) else {
        return MAPS_OK;
    };
    for row in &rows {
        if row.name.first() == Some(&b'.') || !row.is_dir {
            continue;
        }
        let Some(path) = join_subdir(name, &row.name) else {
            continue;
        };
        // Native ignores the recursive return and reports OK at the end;
        // host OOM surfaces only through the direct arms above.
        let _ = add_name_to_list(dirs, &path, true, lister);
    }
    MAPS_OK
}

/// LS3DF RVA 0x725f0 `IGraph::AddMapsDir`
/// (mafia-re ls3df/IGraph/IGraph.cpp:3277-3282). Thin wrapper over the list
/// helper with the global maps list as receiver.
pub fn add_maps_dir(
    dirs: &mut MapsDirs,
    dir: &[u8],
    recursive: bool,
    lister: &mut impl DirLister,
) -> i32 {
    add_name_to_list(dirs, dir, recursive, lister)
}

/// LS3DF RVA 0x72610 `IGraph::DeleteMapsDir`
/// (mafia-re ls3df/IGraph/IGraph.cpp:3291-3302). Reverse scan removing every
/// entry whose name contains `dir` (`strstr`, case-sensitive). Empty `dir`
/// matches everything, as `strstr` does.
pub fn delete_maps_dir(dirs: &mut MapsDirs, dir: &[u8]) -> i32 {
    let mut i = dirs.entries.len();
    while i > 0 {
        i -= 1;
        if contains_subslice(&dirs.entries[i], dir) {
            dirs.entries.swap_remove(i);
        }
    }
    MAPS_OK
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// LS3DF RVA 0x72670 `IGraph::ClearMapDirs`
/// (mafia-re ls3df/IGraph/IGraph.cpp:3304-3312). Frees each entry then clears
/// the list (`Vec` drop covers the native `delete[]` order).
pub fn clear_map_dirs(dirs: &mut MapsDirs) {
    dirs.entries.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Static {
        map: std::collections::BTreeMap<Vec<u8>, Vec<DirEntry>>,
    }
    impl DirLister for Static {
        fn list_subdirs(&mut self, dir: &[u8]) -> Option<Vec<DirEntry>> {
            self.map.get(dir).cloned()
        }
    }
    fn entry(name: &[u8], is_dir: bool) -> DirEntry {
        DirEntry {
            name: name.to_vec(),
            is_dir,
        }
    }
    #[test]
    fn add_appends_backslash_and_recurses_skipping_dot() {
        let mut dirs = MapsDirs::new();
        let mut lister = Static {
            map: [
                (
                    b"Maps".to_vec(),
                    vec![
                        entry(b".", true),
                        entry(b"..", true),
                        entry(b".hidden", true),
                        entry(b"sub", true),
                        entry(b"file.txt", false),
                    ],
                ),
                (b"Maps\\sub".to_vec(), vec![]),
            ]
            .into_iter()
            .collect(),
        };
        assert_eq!(add_maps_dir(&mut dirs, b"Maps", true, &mut lister), MAPS_OK);
        assert_eq!(
            dirs.entries().to_vec(),
            vec![b"Maps\\".to_vec(), b"Maps\\sub\\".to_vec()]
        );
    }
    #[test]
    fn add_without_backslash_keeps_existing_terminator() {
        let mut dirs = MapsDirs::new();
        let mut lister = Static {
            map: Default::default(),
        };
        assert_eq!(
            add_name_to_list(&mut dirs, b"A\\", false, &mut lister),
            MAPS_OK
        );
        assert_eq!(dirs.entries().to_vec(), vec![b"A\\".to_vec()]);
        assert_eq!(
            add_name_to_list(&mut dirs, b"", false, &mut lister),
            MAPS_INVALIDPARAMS
        );
    }
    #[test]
    fn delete_scans_reverse_with_substring_and_clear_empties() {
        let mut dirs = MapsDirs::new();
        let mut lister = Static {
            map: Default::default(),
        };
        for d in [b"A\\".as_slice(), b"AB\\".as_slice(), b"C\\".as_slice()] {
            assert_eq!(add_name_to_list(&mut dirs, d, false, &mut lister), MAPS_OK);
        }
        assert_eq!(delete_maps_dir(&mut dirs, b"A"), MAPS_OK);
        assert_eq!(dirs.entries().to_vec(), vec![b"C\\".to_vec()]);
        clear_map_dirs(&mut dirs);
        assert!(dirs.entries().is_empty());
        assert_eq!(
            add_name_to_list(&mut dirs, b"X\\", false, &mut lister),
            MAPS_OK
        );
        assert_eq!(delete_maps_dir(&mut dirs, b""), MAPS_OK);
        assert!(dirs.entries().is_empty());
    }
}
