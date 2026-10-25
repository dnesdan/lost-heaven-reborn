//! Synthetic Installation inputs: no original game data or runtime evidence.
use lhr_assets::Installation;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
const A8: (u32, u32) = (0xd8dd8fac, 0x5324ace5);
const A2: (u32, u32) = (0x82a1c97b, 0x2d5085d4);
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-installation-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn pack(&self, name: &str, entries: &[Entry], key: (u32, u32)) {
        fs::write(self.0.join(name), pack(entries, key)).unwrap();
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Entry {
    name: Vec<u8>,
    size: u32,
    blocks: Vec<Vec<u8>>,
}
impl Entry {
    fn raw(name: &[u8], bytes: &[u8]) -> Self {
        let blocks = bytes
            .chunks(32768)
            .map(|b| {
                let mut block = vec![0];
                block.extend(b);
                block
            })
            .collect();
        Self {
            name: name.to_vec(),
            size: bytes.len() as u32,
            blocks,
        }
    }
}
fn crypt(bytes: &mut [u8], key: (u32, u32)) {
    let key = ((u64::from(key.0) << 32) | u64::from(key.1)).to_le_bytes();
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte ^= key[i % 8];
    }
}
fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn pack(entries: &[Entry], key: (u32, u32)) -> Vec<u8> {
    let mut out = vec![0; 20];
    out[..4].copy_from_slice(b"ISD0");
    let mut rows = Vec::new();
    for entry in entries {
        let start = out.len() as u32;
        let data = start + 32 + entry.name.len() as u32 + 1;
        let encoded_size: usize = entry.blocks.iter().map(|b| 4 + b.len()).sum();
        let mut header = [0; 32];
        for (at, value) in [
            (0, data - start + encoded_size as u32),
            (4, 32),
            (16, entry.size),
            (20, entry.blocks.len() as u32),
            (24, 0x8000 | (entry.name.len() + 1) as u32),
        ] {
            header[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        crypt(&mut header, key);
        out.extend(header);
        let mut name = entry.name.clone();
        name.push(0);
        crypt(&mut name, key);
        out.extend(name);
        let mut previous = 0;
        for block in &entry.blocks {
            out.extend(((previous << 16) | block.len() as u32).to_le_bytes());
            previous = block.len() as u32;
            let mut block = block.clone();
            crypt(&mut block, key);
            out.extend(block);
        }
        let upper = entry.name.to_ascii_uppercase();
        let hash = ((upper.len() as u32) << 16)
            | upper.iter().fold(0u16, |a, &b| a.wrapping_add(b as u16)) as u32;
        let mut row = [0; 28];
        for (at, value) in [(0, hash), (4, start), (8, data)] {
            row[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        let suffix = &upper[upper.len().saturating_sub(16)..];
        row[12..12 + suffix.len()].copy_from_slice(suffix);
        rows.push(row);
    }
    rows.sort_by_key(|row| word(row, 0));
    let table = out.len() as u32;
    let mut index: Vec<_> = rows.into_iter().flatten().collect();
    crypt(&mut index, key);
    out.extend(index);
    let mut header = Vec::new();
    for value in [entries.len() as u32, table, entries.len() as u32 * 28, 0] {
        header.extend(value.to_le_bytes());
    }
    crypt(&mut header, key);
    out[4..20].copy_from_slice(&header);
    out
}
fn mutate_row(bytes: &mut [u8], key: (u32, u32), mutate: impl FnOnce(&mut [u8])) {
    let mut header = bytes[4..20].to_vec();
    crypt(&mut header, key);
    let table = word(&header, 4) as usize;
    crypt(&mut bytes[table..table + 28], key);
    mutate(&mut bytes[table..table + 28]);
    crypt(&mut bytes[table..table + 28], key);
}
#[test]
fn installation_native_wav_and_asymmetric_keys_are_live() {
    for (filename, key) in [
        ("A8.DTA", A8),
        ("A2.DTA", A2),
        ("A6.DTA", (0x64cd8d0a, 0x4bc97b2d)),
        ("A1.DTA", (0x3d98766c, 0xde7009cd)),
        ("AC.DTA", (0x43876fea, 0x900cdba8)),
        ("AA.DTA", (0x5342760c, 0xedeac652)),
    ] {
        let dir = Directory::new();
        let mut wav = [0; 44];
        wav[..4].copy_from_slice(b"RIFF");
        wav[12..16].copy_from_slice(b"fmt ");
        wav[36..40].copy_from_slice(b"data");
        wav[22..24].copy_from_slice(&1u16.to_le_bytes());
        wav[32..34].copy_from_slice(&2u16.to_le_bytes());
        let mut block = vec![8];
        block.extend(wav);
        block.extend(100i16.to_le_bytes());
        block.push(1);
        dir.pack(
            filename,
            &[Entry {
                name: b"Sounds\\Synthetic.WAV".to_vec(),
                size: 48,
                blocks: vec![block],
            }],
            key,
        );
        let out = Installation::open(&dir.0)
            .unwrap()
            .read("SOUNDS/synthetic.wav")
            .unwrap();
        assert_eq!(&out[..44], &wav);
        assert_eq!(&out[44..46], &100i16.to_le_bytes());
        assert_eq!(out.len(), 48); // The legacy decoder rejects codec 8.
    }
}
#[test]
fn raw_spelling_hash_suffix_collision_and_source_row_identity() {
    let dir = Directory::new();
    // Equal hash + 16-byte suffix, but distinct full names; physical, native row
    // and canonical sort order intentionally differ.
    dir.pack(
        "A8.DTA",
        &[
            Entry::raw(b"BAXXXXXXXXXXXXXXXX", b"second"),
            Entry::raw(b"z\\Mixed/Spelling.BIN", b"mixed"),
            Entry::raw(b"ABXXXXXXXXXXXXXXXX", b"first"),
        ],
        A8,
    );
    let installation = Installation::open(&dir.0).unwrap();
    assert_eq!(installation.read("abxxxxxxxxxxxxxxxx").unwrap(), b"first");
    assert_eq!(installation.read("baxxxxxxxxxxxxxxxx").unwrap(), b"second");
    assert_eq!(installation.read("Z/MIXED/SPELLING.bin").unwrap(), b"mixed");
}
#[test]
fn priority_selected_errors_and_later_only_paths() {
    let dir = Directory::new();
    dir.pack("A8.DTA", &[Entry::raw(b"same", b"first")], A8);
    dir.pack(
        "A2.DTA",
        &[Entry::raw(b"SAME", b"later"), Entry::raw(b"later", b"only")],
        A2,
    );
    fs::write(dir.0.join("same"), b"loose").unwrap();
    let installation = Installation::open(&dir.0).unwrap();
    assert_eq!(installation.read("same").unwrap(), b"first");
    assert_eq!(installation.read("later").unwrap(), b"only");
    // Same-size mutation must also reject the selected source, even cached data.
    let mut bytes = fs::read(dir.0.join("A8.DTA")).unwrap();
    bytes[58] ^= 1;
    use std::io::{Seek, Write};
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(dir.0.join("A8.DTA"))
        .unwrap();
    file.seek(std::io::SeekFrom::Start(0)).unwrap();
    file.write_all(&bytes).unwrap();
    let modified = file.metadata().unwrap().modified().unwrap();
    file.set_modified(modified + std::time::Duration::from_secs(2))
        .unwrap();
    assert!(installation.read("same").is_err());
}
#[test]
fn installation_admits_all_original_archive_slots_in_native_order() {
    let dir = Directory::new();
    // Independent fixture keys are already decoded for the synthetic encoder.
    let archives = [
        ("A8.DTA", A8),
        ("b8.dta", (0x8297d55d, 0x40a54a4f)),
        ("A2.DTA", A2),
        ("A6.DTA", (0x64cd8d0a, 0x4bc97b2d)),
        ("A0.DTA", (0xd8d0a975, 0x467acde0)),
        ("A1.DTA", (0x3d98766c, 0xde7009cd)),
        ("A3.DTA", (0x43876fea, 0x900cdba8)),
        ("AC.DTA", (0x43876fea, 0x900cdba8)),
        ("A4.DTA", (0x43876fea, 0x900cdba8)),
        ("AA.DTA", (0x5342760c, 0xedeac652)),
        ("A5.DTA", (0xdeac5342, 0x760ce652)),
        ("A7.DTA", (0xd6fea900, 0xcdb76ce6)),
        ("A9.DTA", (0x6fee6324, 0xacda4783)),
        ("AB.DTA", (0xd8d0a975, 0x467acde0)),
    ];
    for (slot, (name, key)) in archives.iter().enumerate() {
        let mut entries = vec![Entry::raw(name.as_bytes(), name.as_bytes())];
        // Each adjacent pair shares a path; its earlier native slot must win.
        if slot > 0 {
            entries.push(Entry::raw(
                format!("pair{}", slot - 1).as_bytes(),
                name.as_bytes(),
            ));
        }
        if slot + 1 < archives.len() {
            entries.push(Entry::raw(
                format!("pair{slot}").as_bytes(),
                name.as_bytes(),
            ));
        }
        dir.pack(name, &entries, *key);
    }
    let installation = Installation::open(&dir.0).unwrap();
    for (slot, (name, _)) in archives.iter().enumerate() {
        assert_eq!(installation.read(name).unwrap(), name.as_bytes());
        if slot + 1 < archives.len() {
            assert_eq!(
                installation.read(&format!("pair{slot}")).unwrap(),
                name.as_bytes()
            );
        }
    }
}
#[test]
fn malformed_native_index_and_block_bounds_never_fall_back() {
    for fault in 0..5 {
        let dir = Directory::new();
        let mut entries = vec![
            Entry::raw(b"selected", b"abc"),
            Entry::raw(b"neighbor", b"xyz"),
        ];
        match fault {
            2 => entries[0].blocks[0].extend([9, 9]), // decoded output over logical size
            3 => entries[0].size = 4,                 // short decoded block
            4 => entries[0].blocks = vec![vec![99, 1]], // unsupported audio
            _ => (),
        }
        let mut bytes = pack(&entries, A8);
        // selected sorts after neighbor, so locate its native source row.
        if fault < 2 {
            let mut header = bytes[4..20].to_vec();
            crypt(&mut header, A8);
            let table = word(&header, 4) as usize;
            crypt(&mut bytes[table..], A8);
            let at = (table..bytes.len())
                .step_by(28)
                .find(|&at| word(&bytes, at + 4) == 20)
                .unwrap();
            if fault == 0 {
                bytes[at] ^= 1;
            } else {
                bytes[at + 12] ^= 1;
            }
            crypt(&mut bytes[table..], A8);
        }
        fs::write(dir.0.join("A8.DTA"), bytes).unwrap();
        dir.pack("A2.DTA", &[Entry::raw(b"selected", b"fallback")], A2);
        fs::write(dir.0.join("selected"), b"loose").unwrap();
        assert!(
            Installation::open(&dir.0)
                .unwrap()
                .read("selected")
                .is_err(),
            "fault {fault}"
        );
    }
}
#[test]
fn bounded_empty_and_unicode_domain_are_explicit() {
    let dir = Directory::new();
    dir.pack(
        "A8.DTA",
        &[
            Entry::raw(b"empty", b""),
            Entry::raw(b"\x8e.BIN", b"archive"),
        ],
        A8,
    );
    fs::write(dir.0.join("Ž.bin"), b"loose shadow").unwrap();
    fs::write(dir.0.join("č.txt"), b"loose unicode").unwrap();
    let installation = Installation::open(&dir.0).unwrap();
    assert!(installation.read("empty").unwrap().is_empty());
    let error = installation.read("ž.bin").unwrap_err();
    assert!(error.0.contains("UnsupportedName"));
    assert_eq!(installation.read("Č.txt").unwrap(), b"loose unicode");
    let malformed = Entry {
        name: b"empty".to_vec(),
        size: 0,
        blocks: vec![vec![0]],
    };
    dir.pack("A8.DTA", &[malformed], A8);
    assert!(Installation::open(&dir.0).unwrap().read("empty").is_err());
}
#[test]
fn large_native_chunks_and_admission_quota() {
    let dir = Directory::new();
    let payload: Vec<_> = (0..70000).map(|n| (n % 251) as u8).collect();
    dir.pack("A8.DTA", &[Entry::raw(b"large", &payload)], A8);
    assert_eq!(
        Installation::open(&dir.0).unwrap().read("large").unwrap(),
        payload
    );
    let oversized = Entry {
        name: b"huge".to_vec(),
        size: 64 * 1024 * 1024 + 1,
        blocks: vec![vec![0]],
    };
    dir.pack("A8.DTA", &[oversized], A8);
    assert!(Installation::open(&dir.0).is_err());
    let mut bytes = pack(&[Entry::raw(b"a", b"abc")], A8);
    mutate_row(&mut bytes, A8, |row| {
        row[8..12].copy_from_slice(&20u32.to_le_bytes())
    });
    fs::write(dir.0.join("A8.DTA"), bytes).unwrap();
    assert!(Installation::open(&dir.0).is_err());
    // CP1250 euro names expand to three-byte UTF-8 in both canonical map
    // keys. A budget sufficient for the former path+raw+512 charge must fail,
    // while a budget covering both retained keys admits the same source.
    let names = Directory::new();
    names.pack("A8.DTA", &[Entry::raw(&[0x80; 254], b"")], A8);
    let unlock = ((u64::from(A8.0) << 32) | u64::from(A8.1)) ^ 0x3498576239475694;
    let small = lhr_assets::rw_data::io::NativeFiles::new(&names.0, 0, 2000).unwrap();
    assert_eq!(
        small.register(b"A8.DTA", unlock),
        Err(lhr_assets::rw_data::io::StreamError::Bounds)
    );
    let adequate = lhr_assets::rw_data::io::NativeFiles::new(&names.0, 0, 4000).unwrap();
    assert!(adequate.register(b"A8.DTA", unlock).is_ok());
}

#[test]
fn cross_entry_encoded_extent_and_unsorted_index_are_not_repaired() {
    for unsorted in [false, true] {
        let dir = Directory::new();
        let mut bytes = pack(
            &[
                Entry::raw(b"a", b"abc"),
                Entry::raw(b"b", b"def"),
                Entry::raw(b"c", b"ghi"),
            ],
            A8,
        );
        if unsorted {
            let mut header = bytes[4..20].to_vec();
            crypt(&mut header, A8);
            let table = word(&header, 4) as usize;
            crypt(&mut bytes[table..], A8);
            for i in 0..28 {
                bytes.swap(table + i, table + 56 + i);
            }
            crypt(&mut bytes[table..], A8);
        } else {
            // The first block physically requests bytes belonging to its neighbor.
            // The source file is large enough; only the selected entry bound rejects it.
            let data = 20 + 32 + 2;
            bytes[data..data + 4].copy_from_slice(&100u32.to_le_bytes());
        }
        fs::write(dir.0.join("A8.DTA"), bytes).unwrap();
        dir.pack("A2.DTA", &[Entry::raw(b"a", b"fallback")], A2);
        fs::write(dir.0.join("a"), b"loose").unwrap();
        assert!(Installation::open(&dir.0).unwrap().read("a").is_err());
    }
}
