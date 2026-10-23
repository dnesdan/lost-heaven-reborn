//! Bounded read-only ISD0 archive format intake. Format interpretation informed by MIT
//! mafia-workbench 9f1edbfbc20ab2ac57e66d4f8a444127e92d602e/src/core/dta.js.
//! Copyright (c) 2026 Roman Šimr; see THIRD_PARTY_NOTICES.md for the MIT notice.
//! Header checksum-like fields are not verified. Audio codecs are unsupported.
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
type Result<T> = std::result::Result<T, String>;
const MAX_FILE: usize = 64 * 1024 * 1024;
const BLOCK: usize = 32768;
#[derive(Debug)]
pub(crate) struct Entry {
    pub path: String,
    pub size: usize,
    pub(crate) blocks: u32,
    start: u64,
    pub(crate) offset: u64,
    pub(crate) end: u64,
    pub(crate) encrypted: bool,
}
pub(crate) struct Archive {
    file: File,
    key: [u8; 8],
    entries: BTreeMap<String, Entry>,
    snapshot: std::fs::Metadata,
}

// Verify the opened handle against directory metadata; never reopen it for decoding.
// On Linux/macOS no-follow/nonblocking flags also avoid final-link and FIFO races.
pub(crate) fn checked_open(path: &Path) -> Result<File> {
    let before = std::fs::symlink_metadata(path).map_err(|_| "asset unavailable")?;
    if !before.is_file() {
        return Err("asset must be a regular file without symlink".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000 | 0x800);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100 | 0x4);
    }
    let file = options.open(path).map_err(|_| "asset unreadable")?;
    let handle = file.metadata().map_err(|_| "asset metadata unavailable")?;
    let after = std::fs::symlink_metadata(path).map_err(|_| "asset unavailable")?;
    if !handle.is_file() || !same_snapshot(&before, &handle) || !same_snapshot(&after, &handle) {
        return Err("asset changed while opening".into());
    }
    Ok(file)
}
pub(crate) fn same_snapshot(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if a.dev() != b.dev() || a.ino() != b.ino() {
            return false;
        }
    }
    a.is_file() == b.is_file() && a.len() == b.len() && a.modified().ok() == b.modified().ok()
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn xor(b: &mut [u8], key: &[u8; 8]) {
    for (i, v) in b.iter_mut().enumerate() {
        *v ^= key[i % 8]
    }
}
fn read(file: &mut File, length: u64, offset: u64, n: usize) -> Result<Vec<u8>> {
    if offset
        .checked_add(n as u64)
        .filter(|v| *v <= length)
        .is_none()
    {
        return Err("archive extent out of range".into());
    }
    let mut b = vec![0; n];
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(&mut b))
        .map_err(|_| "archive read failed")?;
    Ok(b)
}
pub(crate) fn canonical(s: &str) -> Result<String> {
    let s = s.replace('\\', "/");
    if s.len() > 1024
        || s.chars().any(|c| c.is_control() || c == ':')
        || s.split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with(['.', ' ']))
    {
        return Err("unsafe asset path".into());
    }
    Ok(s.to_lowercase())
}
// Windows-1250 mapping: undefined code points are rejected, not replaced.
fn name(b: &[u8]) -> Result<String> {
    const HIGH: [u32; 128] = [
        0x20ac, 0, 0x201a, 0, 0x201e, 0x2026, 0x2020, 0x2021, 0, 0x2030, 0x160, 0x2039, 0x15a,
        0x164, 0x17d, 0x179, 0, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0, 0x2122,
        0x161, 0x203a, 0x15b, 0x165, 0x17e, 0x17a, 0xa0, 0x2c7, 0x2d8, 0x141, 0xa4, 0x104, 0xa6,
        0xa7, 0xa8, 0xa9, 0x15e, 0xab, 0xac, 0xad, 0xae, 0x17b, 0xb0, 0xb1, 0x2db, 0x142, 0xb4,
        0xb5, 0xb6, 0xb7, 0xb8, 0x105, 0x15f, 0xbb, 0x13d, 0x2dd, 0x13e, 0x17c, 0x154, 0xc1, 0xc2,
        0x102, 0xc4, 0x139, 0x106, 0xc7, 0x10c, 0xc9, 0x118, 0xcb, 0x11a, 0xcd, 0xce, 0x10e, 0x110,
        0x143, 0x147, 0xd3, 0xd4, 0x150, 0xd6, 0xd7, 0x158, 0x16e, 0xda, 0x170, 0xdc, 0xdd, 0x162,
        0xdf, 0x155, 0xe1, 0xe2, 0x103, 0xe4, 0x13a, 0x107, 0xe7, 0x10d, 0xe9, 0x119, 0xeb, 0x11b,
        0xed, 0xee, 0x10f, 0x111, 0x144, 0x148, 0xf3, 0xf4, 0x151, 0xf6, 0xf7, 0x159, 0x16f, 0xfa,
        0x171, 0xfc, 0xfd, 0x163, 0x2d9,
    ];
    let b = b.strip_suffix(&[0]).unwrap_or(b);
    let mut s = String::new();
    for &v in b {
        let c = if v < 128 {
            v as u32
        } else {
            HIGH[v as usize - 128]
        };
        if c == 0 {
            return Err("undefined or NUL filename".into());
        }
        s.push(char::from_u32(c).unwrap())
    }
    Ok(s)
}
impl Archive {
    pub fn open(path: &Path, k1: u32, k2: u32) -> Result<Self> {
        Self::from_file(checked_open(path)?, k1, k2)
    }
    pub(crate) fn from_file(mut file: File, k1: u32, k2: u32) -> Result<Self> {
        let snapshot = file
            .metadata()
            .map_err(|_| "archive metadata unavailable")?;
        let length = snapshot.len();
        let mut key = [0; 8];
        key[..4].copy_from_slice(&k2.to_le_bytes());
        key[4..].copy_from_slice(&k1.to_le_bytes());
        let mut h = read(&mut file, length, 0, 20)?;
        if &h[..4] != b"ISD0" {
            return Err("ISD0 signature missing".into());
        }
        xor(&mut h[4..], &key);
        let count = u32le(&h, 4) as usize;
        let offset = u32le(&h, 8) as u64;
        let table_size = u32le(&h, 12) as usize;
        if count > 100000 || table_size != count * 28 || offset < 20 {
            return Err("invalid index bounds".into());
        }
        let mut t = read(&mut file, length, offset, table_size)?;
        xor(&mut t, &key);
        let mut entries = BTreeMap::new();
        for row in t.as_chunks::<28>().0 {
            let header_offset = u32le(row, 4) as u64;
            let data_offset = u32le(row, 8) as u64;
            let mut h = read(&mut file, length, header_offset, 32)?;
            xor(&mut h, &key);
            let stored = u32le(&h, 0) as u64;
            let end = header_offset
                .checked_add(stored)
                .ok_or("entry extent overflow")?;
            let size = u32le(&h, 16) as usize;
            let blocks = u32le(&h, 20);
            let flags = u16::from_le_bytes(h[24..26].try_into().unwrap());
            let n = (flags & 32767) as usize;
            if n == 0
                || n > 255
                || size > MAX_FILE
                || blocks as usize > size.div_ceil(BLOCK) + 1
                || (size > 0 && blocks == 0)
            {
                return Err("invalid entry quotas".into());
            }
            let mut raw = read(&mut file, length, header_offset + 32, n)?;
            xor(&mut raw, &key);
            let path = name(&raw)?;
            let canon = canonical(&path)?;
            if data_offset != header_offset + 32 + n as u64
                || end > length
                || end < data_offset
                || header_offset < 20
                || header_offset < offset + table_size as u64 && end > offset
            {
                return Err("entry extent or data offset invalid".into());
            }
            if entries
                .insert(
                    canon,
                    Entry {
                        path,
                        size,
                        blocks,
                        start: header_offset,
                        offset: data_offset,
                        end,
                        encrypted: flags & 32768 != 0,
                    },
                )
                .is_some()
            {
                return Err("duplicate canonical asset path".into());
            }
        }
        let mut spans: Vec<_> = entries.values().map(|e| (e.start, e.end)).collect();
        spans.sort_unstable();
        if spans.windows(2).any(|p| p[0].1 > p[1].0) {
            return Err("overlapping entry data".into());
        }
        let final_metadata = file
            .metadata()
            .map_err(|_| "archive metadata unavailable")?;
        if !same_snapshot(&snapshot, &final_metadata) {
            return Err("archive changed while indexing".into());
        }
        Ok(Self {
            file,
            key,
            entries,
            snapshot,
        })
    }
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }
    pub fn read(&mut self, path: &str, quota: usize) -> Result<Vec<u8>> {
        self.unchanged()?;
        let e = self.entries.get(&canonical(path)?).ok_or("asset absent")?;
        if e.size > quota.min(MAX_FILE) {
            return Err("asset exceeds caller quota".into());
        }
        let mut cursor = e.offset;
        let mut output = Vec::with_capacity(e.size);
        for _ in 0..e.blocks {
            let h = read(&mut self.file, e.end, cursor, 4)?;
            cursor += 4;
            let n = (u32le(&h, 0) & 65535) as usize;
            if n == 0 {
                return Err("empty archive block".into());
            }
            let mut b = read(&mut self.file, e.end, cursor, n)?;
            cursor += n as u64;
            if e.encrypted {
                xor(&mut b, &self.key)
            }
            let remaining = (e.size - output.len()).min(BLOCK);
            match b[0] {
                0 | 2..=7 => {
                    if b.len() - 1 > remaining {
                        return Err("raw block exceeds quota".into());
                    }
                    output.extend_from_slice(&b[1..])
                }
                1 => output.extend(lz(&b, remaining)?),
                _ => return Err("unsupported block codec (audio)".into()),
            }
        }
        if cursor != e.end {
            return Err("encoded size mismatch".into());
        }
        if output.len() != e.size {
            return Err("decoded size mismatch".into());
        }
        self.unchanged()?;
        Ok(output)
    }
    fn unchanged(&self) -> Result<()> {
        let now = self
            .file
            .metadata()
            .map_err(|_| "archive metadata unavailable")?;
        if !same_snapshot(&self.snapshot, &now) {
            return Err("archive changed after indexing".into());
        }
        Ok(())
    }
}
fn lz(b: &[u8], limit: usize) -> Result<Vec<u8>> {
    if b.len() < 3 || b[0] != 1 {
        return Err("invalid LZ header".into());
    }
    let mut out = Vec::new();
    let mut p = 3;
    let mut flags = u16::from_be_bytes([b[1], b[2]]);
    let mut bits = 16;
    while p < b.len() {
        if bits == 0 {
            let s = b.get(p..p + 2).ok_or("truncated LZ flags")?;
            flags = u16::from_be_bytes([s[0], s[1]]);
            p += 2;
            bits = 16;
            if p == b.len() {
                return Err("empty LZ flag group".into());
            }
        }
        if flags & 32768 == 0 {
            if out.len() == limit {
                return Err("LZ output quota".into());
            }
            out.push(b[p]);
            p += 1
        } else {
            let s = b.get(p..p + 2).ok_or("truncated LZ reference")?;
            let distance = ((s[0] as usize) << 4) | ((s[1] as usize) >> 4);
            if distance == 0 {
                let s = b.get(p..p + 4).ok_or("truncated LZ fill")?;
                let n = ((s[1] as usize) << 8) + s[2] as usize + 16;
                if out.len() + n > limit {
                    return Err("LZ fill quota".into());
                }
                out.resize(out.len() + n, s[3]);
                p += 4
            } else {
                let n = (s[1] & 15) as usize + 3;
                if distance > out.len() || out.len() + n > limit {
                    return Err("LZ reference bounds".into());
                }
                for _ in 0..n {
                    out.push(out[out.len() - distance])
                }
                p += 2
            }
        }
        flags <<= 1;
        bits -= 1;
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths() {
        for p in ["../x", "a/../b", "/a", "a//b", "a:foo", "a. ", "a\0b", ""] {
            assert!(canonical(p).is_err(), "{p:?}")
        }
        assert_eq!(canonical("MAPS\\TeSt.BMP").unwrap(), "maps/test.bmp");
        assert_eq!(name(&[0xc8, 0x8a, 0]).unwrap(), "ČŠ");
        assert!(name(&[0x81]).is_err());
    }
    #[test]
    fn lz_bounds() {
        assert_eq!(lz(&[1, 0, 0, b'a', b'b'], 2).unwrap(), b"ab");
        assert_eq!(lz(&[1, 0x40, 0, b'a', 0, 0x10], 4).unwrap(), b"aaaa");
        assert_eq!(
            lz(&[1, 0x80, 0, 0, 0, 0, b'x'], 16).unwrap(),
            vec![b'x'; 16]
        );
        for x in [&[1, 0x80, 0, 0, 0x10][..], &[1, 0x80, 0, 0][..]] {
            assert!(lz(x, 32768).is_err())
        }
        assert!(lz(&[1, 0, 0, b'a', b'b'], 1).is_err());
    }
}
#[cfg(test)]
mod container_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new(b: &[u8]) -> Self {
            let p = std::env::temp_dir().join(format!(
                "lhr-dta-prototype-{}-{}.dta",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
                .unwrap();
            file.write_all(b).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    fn fixture(names: &[&str], encrypted: bool) -> Vec<u8> {
        fixture_key(names, encrypted, [1; 8])
    }
    fn fixture_key(names: &[&str], encrypted: bool, key: [u8; 8]) -> Vec<u8> {
        fixture_payload(names, encrypted, key, &[0, b'a', b'b', b'c'], 3)
    }
    fn fixture_payload(
        names: &[&str],
        encrypted: bool,
        key: [u8; 8],
        payload: &[u8],
        size: u32,
    ) -> Vec<u8> {
        let mut b = vec![0; 20];
        b[..4].copy_from_slice(b"ISD0");
        let mut rows = Vec::new();
        for name in names {
            let start = b.len();
            let n = name.len() + 1;
            let data = start + 32 + n;
            let mut h = vec![0; 32];
            h[..4].copy_from_slice(&((32 + n + 4 + payload.len()) as u32).to_le_bytes());
            h[4..8].copy_from_slice(&32u32.to_le_bytes());
            h[16..20].copy_from_slice(&size.to_le_bytes());
            h[20..24].copy_from_slice(&1u32.to_le_bytes());
            h[24..26]
                .copy_from_slice(&((n as u16) | if encrypted { 32768 } else { 0 }).to_le_bytes());
            xor(&mut h, &key);
            b.extend(h);
            let mut name = name.as_bytes().to_vec();
            name.push(0);
            xor(&mut name, &key);
            b.extend(name);
            b.extend((payload.len() as u32).to_le_bytes());
            let mut payload = payload.to_vec();
            if encrypted {
                xor(&mut payload, &key)
            }
            b.extend(payload);
            let mut row = vec![0; 28];
            row[4..8].copy_from_slice(&(start as u32).to_le_bytes());
            row[8..12].copy_from_slice(&(data as u32).to_le_bytes());
            rows.extend(row);
        }
        let offset = b.len();
        xor(&mut rows, &key);
        b.extend(rows);
        let mut h = Vec::new();
        for v in [
            names.len() as u32,
            offset as u32,
            (names.len() * 28) as u32,
            0,
        ] {
            h.extend(v.to_le_bytes())
        }
        xor(&mut h, &key);
        b[4..20].copy_from_slice(&h);
        b
    }
    fn open(b: &[u8]) -> Result<Archive> {
        let f = Fixture::new(b);
        Archive::open(&f.0, 0x01010101, 0x01010101)
    }
    #[test]
    fn raw_encrypted_quota_and_every_truncation() {
        for encrypted in [false, true] {
            let b = fixture(&["MAPS\\A.BMP"], encrypted);
            let mut a = open(&b).unwrap();
            assert_eq!(a.read("maps/a.bmp", 3).unwrap(), b"abc");
            assert!(a.read("maps/a.bmp", 2).is_err());
            assert!(a.read("maps/missing.bmp", 9).is_err());
            for n in 0..b.len() {
                assert!(open(&b[..n]).is_err(), "{n}")
            }
        }
    }
    #[test]
    fn malicious_names_and_duplicate() {
        for names in [
            &["../x"][..],
            &["MAPS/a.bmp", "maps\\A.BMP"][..],
            &["a/./b"][..],
            &["a:"][..],
        ] {
            assert!(open(&fixture(names, false)).is_err())
        }
    }
    #[test]
    fn wrong_keys_and_bad_block_extent() {
        let b = fixture(&["x"], false);
        let f = Fixture::new(&b);
        assert!(Archive::open(&f.0, 0, 0).is_err());
        let mut b = b;
        let data = 20 + 32 + 2;
        b[data..data + 4].copy_from_slice(&65535u32.to_le_bytes());
        let mut a = open(&b).unwrap();
        assert!(a.read("x", 3).is_err());
    }
    #[test]
    fn rejects_overlapping_record_and_unknown_codec() {
        let mut b = fixture(&["x", "y"], false);
        let table = b.len() - 56;
        let row = b[table..table + 28].to_vec();
        b[table + 28..].copy_from_slice(&row);
        assert!(open(&b).is_err());
        let mut b = fixture(&["x"], false);
        b[20 + 32 + 2 + 4] = 8;
        let mut a = open(&b).unwrap();
        assert!(a.read("x", 3).is_err());
    }

    #[test]
    fn compressed_archive_and_all_raw_variants() {
        let b = fixture_payload(&["x"], true, [1; 8], &[1, 0, 0, b'a', b'b', b'c'], 3);
        assert_eq!(open(&b).unwrap().read("x", 3).unwrap(), b"abc");
        for codec in [0, 2, 3, 4, 5, 6, 7] {
            let b = fixture_payload(&["x"], false, [1; 8], &[codec, b'a', b'b', b'c'], 3);
            assert_eq!(open(&b).unwrap().read("x", 3).unwrap(), b"abc");
        }
        let b = fixture_payload(&["x"], false, [1; 8], &[1, 0, 0, b'a', b'b', b'c'], 2);
        assert!(open(&b).unwrap().read("x", 3).is_err());
    }
    #[test]
    fn opaque_header_fields_do_not_restrict_valid_encoded_extents() {
        let mut b = fixture(&["x"], false);
        // Real A6 records include both 32 and 33 here. This field is not
        // established as a header size and must remain uninterpreted.
        let mut h = b[20..52].to_vec();
        xor(&mut h, &[1; 8]);
        h[4..8].copy_from_slice(&33u32.to_le_bytes());
        h[26..32].fill(0xa5);
        xor(&mut h, &[1; 8]);
        b[20..52].copy_from_slice(&h);
        b[16..20].fill(0x7c);
        assert_eq!(open(&b).unwrap().read("x", 3).unwrap(), b"abc");
    }
    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "lhr-dta-install-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn archive(&self, name: &str, paths: &[&str]) {
            self.archive_payload(name, paths, b"abc");
        }
        fn archive_payload(&self, name: &str, paths: &[&str], content: &[u8]) {
            let (k1, k2): (u32, u32) = match name.to_ascii_uppercase().as_str() {
                "A1.DTA" => (0x3d98766c, 0xde7009cd),
                "A2.DTA" => (0x82a1c97b, 0x2d5085d4),
                "A6.DTA" => (0x64cd8d0a, 0x4bc97b2d),
                "A8.DTA" => (0xd8dd8fac, 0x5324ace5),
                "AA.DTA" => (0x5342760c, 0xedeac652),
                _ => panic!("test archive name"),
            };
            let mut key = [0; 8];
            key[..4].copy_from_slice(&k2.to_le_bytes());
            key[4..].copy_from_slice(&k1.to_le_bytes());
            let mut payload = vec![0];
            payload.extend_from_slice(content);
            std::fs::write(
                self.0.join(name),
                fixture_payload(paths, true, key, &payload, content.len() as u32),
            )
            .unwrap();
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn installation_case_discovery_lookup_archive_first_and_loose_fallback() {
        let d = Directory::new();
        d.archive("a2.DtA", &["models\\Test.4ds"]);
        let installation = crate::Installation::open(&d.0).unwrap();
        assert_eq!(installation.read("MODELS/TEST.4DS").unwrap(), b"abc");
        std::fs::create_dir(d.0.join("Models")).unwrap();
        std::fs::write(d.0.join("Models/TeSt.4DS"), b"loose").unwrap();
        assert_eq!(installation.read("models/test.4ds").unwrap(), b"abc");
        std::fs::write(d.0.join("other.txt"), b"loose").unwrap();
        assert_eq!(installation.read("OTHER.TXT").unwrap(), b"loose");
        for name in [
            "../x",
            "/x",
            "models/../x",
            "models/test.4ds/",
            "models/./test.4ds",
        ] {
            assert!(installation.read(name).is_err(), "{name}");
        }
    }
    #[test]
    fn installation_rejects_duplicate_archives_and_loose_case() {
        let d = Directory::new();
        d.archive("A2.dta", &["models/y"]);
        d.archive("a2.DTA", &["models/y"]);
        assert!(crate::Installation::open(&d.0).is_err());
        std::fs::remove_file(d.0.join("a2.DTA")).unwrap();
        std::fs::create_dir(d.0.join("models")).unwrap();
        std::fs::write(d.0.join("models/X"), b"a").unwrap();
        std::fs::write(d.0.join("models/x"), b"b").unwrap();
        assert!(
            crate::Installation::open(&d.0)
                .unwrap()
                .read("models/x")
                .is_err()
        );
    }
    #[test]
    fn first_registered_patch_archive_overlays_base_resource() {
        let d = Directory::new();
        d.archive_payload("A2.dta", &["models/x"], b"base");
        d.archive_payload("A8.dta", &["models/x"], b"patch");
        assert_eq!(
            crate::Installation::open(&d.0)
                .unwrap()
                .read("models/x")
                .unwrap(),
            b"patch"
        );
    }
    #[test]
    fn installation_malformed_archive_and_bad_loose_never_fall_back() {
        let d = Directory::new();
        std::fs::write(d.0.join("A2.dta"), b"broken").unwrap();
        assert!(crate::Installation::open(&d.0).is_err());
        std::fs::remove_file(d.0.join("A2.dta")).unwrap();
        d.archive("A2.dta", &["models/y"]);
        std::fs::create_dir_all(d.0.join("models/x")).unwrap();
        let installation = crate::Installation::open(&d.0).unwrap();
        assert!(installation.read("models/x").is_err());
        std::fs::remove_dir(d.0.join("models/x")).unwrap();
        let file = std::fs::File::create(d.0.join("models/x")).unwrap();
        file.set_len(MAX_FILE as u64 + 1).unwrap();
        assert!(installation.read("models/x").is_err());
    }
    #[test]
    fn retained_archive_handle_and_mutation_guard() {
        let d = Directory::new();
        d.archive("A2.dta", &["models/x"]);
        let installation = crate::Installation::open(&d.0).unwrap();
        std::fs::rename(d.0.join("A2.dta"), d.0.join("retained.dta")).unwrap();
        std::fs::write(d.0.join("A2.dta"), b"replacement").unwrap();
        assert_eq!(installation.read("models/x").unwrap(), b"abc");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(d.0.join("retained.dta"))
            .unwrap();
        file.set_len(1).unwrap();
        assert!(installation.read("models/x").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn installation_rejects_symlinks_inside_and_outside_root() {
        use std::os::unix::fs::symlink;
        let d = Directory::new();
        let outside = Directory::new();
        outside.archive("A2.dta", &["models/x"]);
        symlink(outside.0.join("A2.dta"), d.0.join("A2.dta")).unwrap();
        assert!(crate::Installation::open(&d.0).is_err());
        std::fs::remove_file(d.0.join("A2.dta")).unwrap();
        d.archive("A2.dta", &["models/x"]);
        symlink(&outside.0, d.0.join("loose")).unwrap();
        let installation = crate::Installation::open(&d.0).unwrap();
        assert_eq!(installation.read("models/x").unwrap(), b"abc");
        assert!(installation.read("loose/x").is_err());
        std::fs::remove_file(d.0.join("loose")).unwrap();
        std::fs::create_dir(d.0.join("loose")).unwrap();
        std::fs::write(d.0.join("real"), b"loose").unwrap();
        symlink(d.0.join("real"), d.0.join("loose/x")).unwrap();
        assert!(installation.read("loose/x").is_err());
    }
    #[test]
    fn corrupt_encoded_filename_and_decoded_quota() {
        let mut b = fixture(&["x"], false);
        b[20 + 32] = 0x81 ^ 1;
        assert!(open(&b).is_err());
        let mut b = fixture(&["x"], false);
        let mut h = b[20..52].to_vec();
        xor(&mut h, &[1; 8]);
        h[16..20].copy_from_slice(&((MAX_FILE as u32) + 1).to_le_bytes());
        xor(&mut h, &[1; 8]);
        b[20..52].copy_from_slice(&h);
        assert!(open(&b).is_err());
    }
}
