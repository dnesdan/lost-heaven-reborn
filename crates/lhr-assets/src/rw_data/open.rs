//! Native registration/open paths sharing the read/seek/close table.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAllocation {
    Owner,
    Block,
    Wav,
}
/// Host effects carry native resource identities; allocation zero means failure.
/// Names are bounded ASCII in this implementation; locale-dependent strupr is excluded.
pub trait OpenEffects: ReadEffects {
    fn create_file(&mut self, name: &[u8], random_access: bool) -> Result<u32, Self::Error>;
    fn seek_file_end(&mut self, file: u32) -> Result<(u32, u32), Self::Error>;
    fn allocate_open(&mut self, kind: OpenAllocation, size: u32) -> Result<u64, Self::Error>;
    fn free_owner(&mut self, allocation: u64) -> Result<(), Self::Error>;
}
pub(super) struct Owner {
    token: u64,
    file: u32,
    refs: u32,
    registered: bool,
    key: u64,
    table: Vec<u8>,
}
macro_rules! effect {
    ($s:expr, $stage:ident, $call:expr) => {
        match $call {
            Ok(v) => v,
            Err(e) => {
                $s.interrupted = Some(EffectStage::$stage);
                return Err(OperationError::Backend(e));
            }
        }
    };
}
fn name_valid(name: &[u8]) -> bool {
    name.len() < 260 && name.is_ascii() && !name.contains(&0)
}
fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn ready<B: Effects>(s: &NativeState<B>) -> Result<(), OperationError<B::Error>> {
    if let Some(stage) = s.interrupted {
        Err(OperationError::Interrupted(stage))
    } else {
        Ok(())
    }
}
impl<B: OpenEffects> Handles<B> {
    /// dtaCreate and constructor, PC RVAs 0x1170/0x11e0. Invalid constructors
    /// undergo unsigned Release underflow and retain their native leaked resources.
    pub fn dta_create(&self, name: &[u8]) -> Result<Option<u64>, OperationError<B::Error>> {
        if !name_valid(name) {
            return Err(OperationError::UnsupportedOpen);
        }
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        ready(&s)?;
        let token = effect!(
            s,
            Allocate,
            s.effects.allocate_open(OpenAllocation::Owner, 12)
        );
        if token == 0 {
            return Ok(None);
        }
        if !s.admitted_allocations.insert(token) {
            return Err(OperationError::InvalidRecord);
        }
        let file = effect!(s, Open, s.effects.create_file(name, true));
        let mut owner = Owner {
            token,
            file,
            refs: 0,
            registered: false,
            key: 0,
            table: Vec::new(),
        };
        if file != FAILURE {
            let mut magic = [0; 4];
            let read = effect!(s, Read, s.effects.read_file(file, &mut magic));
            owner.registered = read.success && read.bytes_read == 4 && &magic == b"ISD0";
        }
        owner.refs = if owner.registered { 1 } else { u32::MAX };
        let valid = owner.registered;
        s.owners.push(owner);
        Ok(valid.then_some(token))
    }
    pub fn add_ref(&self, owner: u64) -> Result<Option<u32>, OperationError<B::Error>> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        ready(&s)?;
        Ok(s.owners.iter_mut().find(|o| o.token == owner).map(|o| {
            o.refs = o.refs.wrapping_add(1);
            o.refs
        }))
    }
    /// Last Release closes the archive first, then invalidates all borrowed files.
    /// CloseHandle's boolean is ignored, as in the native destructor.
    pub fn release(&self, owner: u64) -> Result<Option<u32>, OperationError<B::Error>> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        ready(&s)?;
        let Some(i) = s.owners.iter().position(|o| o.token == owner) else {
            return Ok(None);
        };
        s.owners[i].refs = s.owners[i].refs.wrapping_sub(1);
        let refs = s.owners[i].refs;
        if refs != 0 {
            return Ok(Some(refs));
        }
        let file = s.owners[i].file;
        if file != FAILURE {
            effect!(s, CloseHandle, s.effects.close_handle(file));
        }
        s.owners[i].registered = false;
        s.owners[i].table.clear();
        for j in 0..s.records.len() {
            if s.records[j].file != file {
                continue;
            }
            let r = s.records[j].clone();
            if r.temporary_name != 0 {
                effect!(s, FreeName, s.effects.free_name(r.temporary_name));
            }
            if r.block != 0 {
                effect!(s, FreeBlock, s.effects.free_block(r.block));
            }
            if r.wav != 0 {
                effect!(s, FreeWav, s.effects.free_wav(r.wav));
            }
            s.records[j].wav = 0;
            s.records[j].block = 0;
            s.records[j].cache_valid = false;
            s.records[j].file = FAILURE;
            s.read_caches[j] = None;
            s.archive_caches[j] = None;
        }
        effect!(s, FreeOwner, s.effects.free_owner(owner));
        s.owners.remove(i);
        Ok(Some(0))
    }
    /// UnlockPack, RVA 0x13b0. Table seek/read boolean status is ignored.
    /// Non-multiple sizes, sizes above 64 MiB and short index reads are host
    /// exclusions: native short reads can decrypt uninitialized newly grown entries.
    pub fn unlock_pack(&self, owner: u64, key: u64) -> Result<bool, OperationError<B::Error>> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        ready(&s)?;
        let Some(i) = s.owners.iter().position(|o| o.token == owner) else {
            return Ok(false);
        };
        let file = s.owners[i].file;
        let (p, e) = effect!(s, Seek, s.effects.seek_read_file(file, 4));
        if p == FAILURE && e != 0 {
            return Ok(false);
        }
        let mut header = [0; 16];
        let read = effect!(s, Read, s.effects.read_file(file, &mut header));
        if !read.success || read.bytes_read != 16 {
            return Ok(false);
        }
        let key = key ^ 0x3498576239475694;
        de_crypt(&mut header, key);
        let Some(i) = s
            .owners
            .iter()
            .rposition(|o| o.registered && o.file == file)
        else {
            return Ok(false);
        };
        s.owners[i].key = key;
        let size = word(&header, 8) as usize;
        if !size.is_multiple_of(28) || size > 64 * 1024 * 1024 {
            return Err(OperationError::UnsupportedOpen);
        }
        // Zero initialization is host storage only; a short read cannot admit it
        // as native table contents and disables subsequent mutation.
        s.owners[i].table.resize(size, 0);
        effect!(s, Seek, s.effects.seek_read_file(file, word(&header, 4)));
        let NativeState {
            owners,
            effects,
            interrupted,
            ..
        } = &mut *s;
        let read = match effects.read_file(file, &mut owners[i].table) {
            Ok(read) => read,
            Err(e) => {
                *interrupted = Some(EffectStage::Read);
                return Err(OperationError::Backend(e));
            }
        };
        if read.bytes_read != size as u32 {
            *interrupted = Some(EffectStage::Read);
            return Err(OperationError::UnsupportedOpen);
        }
        de_crypt(&mut owners[i].table, key);
        Ok(true)
    }
    pub fn dta_set_dta_first_force(&self) -> Result<(), OperationError<B::Error>> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        ready(&s)?;
        s.dta_first_force = true;
        Ok(())
    }
    pub fn dta_open(&self, name: &[u8], force: bool) -> Result<u32, OperationError<B::Error>> {
        if !name_valid(name) {
            return Err(OperationError::UnsupportedOpen);
        }
        let mut s = self
            .state
            .lock()
            .map_err(|_| OperationError::LockPoisoned)?;
        ready(&s)?;
        let first = force || s.dta_first_force;
        let h = if first {
            archive_open(&mut s, name)?
        } else {
            loose_open(&mut s, name)?
        };
        if h != FAILURE {
            return Ok(h);
        }
        if first {
            loose_open(&mut s, name)
        } else {
            archive_open(&mut s, name)
        }
    }
}
fn store<B: OpenEffects>(
    s: &mut NativeState<B>,
    record: HandleRecord,
    loose: Option<LooseReadCache>,
    archive: Option<ArchiveReadCache>,
) -> Result<u32, OperationError<B::Error>> {
    if !s.admitted_allocations.insert(record.block) {
        return Err(OperationError::InvalidRecord);
    }
    let i = s
        .records
        .iter()
        .position(|r| r.file == FAILURE)
        .unwrap_or(s.records.len());
    if i >= u32::MAX as usize {
        return Err(OperationError::HandleSpaceExhausted);
    }
    if i == s.records.len() {
        s.records.push(record);
        s.read_caches.push(loose);
        s.archive_caches.push(archive);
    } else {
        s.records[i] = record;
        s.read_caches[i] = loose;
        s.archive_caches[i] = archive;
    }
    Ok(i as u32)
}
fn loose_open<B: OpenEffects>(
    s: &mut NativeState<B>,
    name: &[u8],
) -> Result<u32, OperationError<B::Error>> {
    let file = effect!(s, Open, s.effects.create_file(name, false));
    if file == FAILURE {
        return Ok(FAILURE);
    }
    let (size, error) = effect!(s, Seek, s.effects.seek_file_end(file));
    if size == FAILURE && error != 0 {
        return Ok(FAILURE);
    }
    let block = effect!(
        s,
        Allocate,
        s.effects.allocate_open(OpenAllocation::Block, 32768)
    );
    if block == 0 {
        return Ok(FAILURE);
    }
    store(
        s,
        HandleRecord {
            file,
            flags: 1,
            cursor: ReadCursor::new(size),
            cache_valid: false,
            temporary_name: 0,
            temporary_name_bytes: Vec::new(),
            block,
            wav: 0,
        },
        Some(LooseReadCache {
            bytes: vec![0; 32768],
            block_start_file_pointer: 0,
            block_size: size.min(32768),
        }),
        None,
    )
}
fn first_index(table: &[u8], hash: u32) -> Option<usize> {
    let count = table.len() / 28;
    if count == 0 {
        return None;
    }
    let (mut lo, mut hi, mut mid) = (0i32, count as i32 - 1, count as i32 / 2);
    loop {
        let diff = hash.wrapping_sub(word(table, mid as usize * 28)) as i32;
        if diff > 0 {
            mid += 1;
            if mid > hi {
                return None;
            }
            lo = mid;
            mid += (hi - lo) / 2;
        } else if diff < 0 {
            mid -= 1;
            if mid < lo {
                return None;
            }
            hi = mid;
            mid = lo + (hi - lo) / 2;
        } else {
            break;
        }
    }
    while mid > 0 && word(table, (mid as usize - 1) * 28) == hash {
        mid -= 1;
    }
    Some(mid as usize)
}
fn archive_open<B: OpenEffects>(
    s: &mut NativeState<B>,
    name: &[u8],
) -> Result<u32, OperationError<B::Error>> {
    let upper = name.to_ascii_uppercase();
    let sum = upper
        .iter()
        .fold(0u16, |sum, &v| sum.wrapping_add(v as u16));
    let hash = ((name.len() as u32) << 16) | sum as u32;
    for i in 0..s.owners.len() {
        if !s.owners[i].registered {
            continue;
        }
        let Some(first) = first_index(&s.owners[i].table, hash) else {
            continue;
        };
        for j in first..s.owners[i].table.len() / 28 {
            let entry = &s.owners[i].table[j * 28..j * 28 + 28];
            if word(entry, 0) != hash {
                break;
            }
            let suffix = &upper[upper.len().saturating_sub(16)..];
            // strncmp stops at the first NUL, including names shorter than 16.
            let stored = &entry[12..28];
            if (0..16).any(|n| {
                stored[n] != suffix.get(n).copied().unwrap_or(0)
                    && (n == 0 || !stored[..n].contains(&0))
            }) {
                continue;
            }
            let (file, key, offset, data) = (
                s.owners[i].file,
                s.owners[i].key,
                word(entry, 4),
                word(entry, 8),
            );
            let (p, e) = effect!(s, Seek, s.effects.seek_read_file(file, offset));
            if p == FAILURE && e != 0 {
                return Ok(FAILURE);
            }
            let mut header = [0; 32];
            let r = effect!(s, Read, s.effects.read_file(file, &mut header));
            if !r.success || r.bytes_read != 32 {
                return Ok(FAILURE);
            }
            de_crypt(&mut header, key);
            let flags = word(&header, 24);
            let len = (flags & 0x7fff) as usize;
            if len >= 260 {
                return Err(OperationError::UnsupportedOpen);
            }
            let mut full = vec![0; len];
            let r = effect!(s, Read, s.effects.read_file(file, &mut full));
            if !r.success || r.bytes_read != len as u32 {
                return Ok(FAILURE);
            }
            de_crypt(&mut full, key);
            let end = full.iter().position(|&v| v == 0).unwrap_or(full.len());
            if !full[..end].is_ascii() {
                return Err(OperationError::UnsupportedOpen);
            }
            if full[..end].to_ascii_uppercase() != upper {
                continue;
            }
            let mut compressed = [0; 4];
            let r = effect!(s, Read, s.effects.read_file(file, &mut compressed));
            if !r.success || r.bytes_read != 4 {
                return Ok(FAILURE);
            }
            let mut kind = [0; 1];
            let r = effect!(s, Read, s.effects.read_file(file, &mut kind));
            if !r.success || r.bytes_read != 1 {
                return Ok(FAILURE);
            }
            let encrypted = flags & 0x8000 != 0;
            if encrypted {
                de_crypt(&mut kind, key);
            }
            let mut wav = 0;
            let mut wav_header = None;
            if kind[0] >= 8 {
                wav = effect!(
                    s,
                    Allocate,
                    s.effects.allocate_open(OpenAllocation::Wav, 44)
                );
                if wav == 0 {
                    return Ok(FAILURE);
                }
                if !s.admitted_allocations.insert(wav) {
                    return Err(OperationError::InvalidRecord);
                }
                let mut prefix = [0; 45];
                prefix[0] = kind[0];
                let r = effect!(s, Read, s.effects.read_file(file, &mut prefix[1..]));
                if !r.success || r.bytes_read != 44 {
                    return Ok(FAILURE);
                }
                if encrypted {
                    de_crypt(&mut prefix, key);
                } else {
                    de_crypt(&mut prefix[1..], key);
                }
                wav_header = Some(prefix[1..].try_into().unwrap());
            }
            let block = effect!(
                s,
                Allocate,
                s.effects.allocate_open(OpenAllocation::Block, 32768)
            );
            if block == 0 {
                if wav != 0 {
                    effect!(s, FreeWav, s.effects.free_wav(wav));
                }
                return Ok(FAILURE);
            }
            let size = word(&header, 16);
            return store(
                s,
                HandleRecord {
                    file,
                    flags: 0,
                    cursor: ReadCursor::new(size),
                    cache_valid: false,
                    temporary_name: 0,
                    temporary_name_bytes: Vec::new(),
                    block,
                    wav,
                },
                None,
                Some(ArchiveReadCache {
                    wav_header,
                    bytes: vec![0; 32768],
                    data_offset: data,
                    block_start_file_pointer: 0,
                    current_block: 0,
                    block_count: word(&header, 20),
                    compressed_block_size: word(&compressed, 0) & 0xffff,
                    previous_compressed_block_size: 0,
                    block_size: size.min(32768),
                    encrypted,
                    key,
                }),
            );
        }
    }
    Ok(FAILURE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };
    #[derive(Default)]
    struct Disk {
        names: BTreeMap<Vec<u8>, Vec<u8>>,
        files: BTreeMap<u32, (Vec<u8>, usize)>,
        next: u32,
        allocation: u64,
        calls: Vec<String>,
    }
    struct Backend(Arc<Mutex<Disk>>);
    impl Effects for Backend {
        type Error = ();
        fn set_file_pointer(&mut self, _: u32, _: i32, _: u32) -> Result<u32, ()> {
            unreachable!()
        }
        fn close_handle(&mut self, f: u32) -> Result<bool, ()> {
            let mut d = self.0.lock().unwrap();
            d.calls.push(format!("close {f}"));
            Ok(d.files.remove(&f).is_some())
        }
        fn delete_file(&mut self, _: &[u8]) -> Result<bool, ()> {
            unreachable!()
        }
        fn file_attributes(&mut self, _: &[u8]) -> Result<u32, ()> {
            unreachable!()
        }
        fn set_file_attributes(&mut self, _: &[u8], _: u32) -> Result<bool, ()> {
            unreachable!()
        }
        fn move_file(&mut self, _: &[u8], _: &[u8]) -> Result<bool, ()> {
            unreachable!()
        }
        fn free_name(&mut self, _: u64) -> Result<(), ()> {
            unreachable!()
        }
        fn free_block(&mut self, n: u64) -> Result<(), ()> {
            self.0.lock().unwrap().calls.push(format!("block {n}"));
            Ok(())
        }
        fn free_wav(&mut self, n: u64) -> Result<(), ()> {
            self.0.lock().unwrap().calls.push(format!("wav {n}"));
            Ok(())
        }
    }
    impl ReadEffects for Backend {
        fn seek_read_file(&mut self, f: u32, p: u32) -> Result<(u32, u32), ()> {
            self.0.lock().unwrap().files.get_mut(&f).unwrap().1 = p as usize;
            Ok((p, 0))
        }
        fn read_file(&mut self, f: u32, out: &mut [u8]) -> Result<ReadFileResult, ()> {
            let mut d = self.0.lock().unwrap();
            let (bytes, p) = d.files.get_mut(&f).unwrap();
            let n = out.len().min(bytes.len().saturating_sub(*p));
            out[..n].copy_from_slice(&bytes[*p..*p + n]);
            *p += n;
            Ok(ReadFileResult {
                success: true,
                bytes_read: n as u32,
            })
        }
    }
    impl OpenEffects for Backend {
        fn create_file(&mut self, n: &[u8], _: bool) -> Result<u32, ()> {
            let mut d = self.0.lock().unwrap();
            let Some(bytes) = d.names.get(n).cloned() else {
                return Ok(FAILURE);
            };
            let id = d.next;
            d.next += 1;
            d.files.insert(id, (bytes, 0));
            Ok(id)
        }
        fn seek_file_end(&mut self, f: u32) -> Result<(u32, u32), ()> {
            let mut d = self.0.lock().unwrap();
            let (bytes, p) = d.files.get_mut(&f).unwrap();
            *p = bytes.len();
            Ok((*p as u32, 0))
        }
        fn allocate_open(&mut self, _: OpenAllocation, _: u32) -> Result<u64, ()> {
            let mut d = self.0.lock().unwrap();
            d.allocation += 1;
            Ok(d.allocation)
        }
        fn free_owner(&mut self, n: u64) -> Result<(), ()> {
            self.0.lock().unwrap().calls.push(format!("owner {n}"));
            Ok(())
        }
    }
    fn archive_fixture() -> Vec<u8> {
        let key = 0x3498576239475694;
        let mut bytes = vec![0; 128];
        bytes[..4].copy_from_slice(b"ISD0");
        let mut header = [0; 16];
        header[4..8].copy_from_slice(&20u32.to_le_bytes());
        header[8..12].copy_from_slice(&28u32.to_le_bytes());
        de_crypt(&mut header, key);
        bytes[4..20].copy_from_slice(&header);
        let mut index = [0; 28];
        index[..4].copy_from_slice(&((1u32 << 16) | b'A' as u32).to_le_bytes());
        index[4..8].copy_from_slice(&48u32.to_le_bytes());
        index[8..12].copy_from_slice(&81u32.to_le_bytes());
        index[12] = b'A';
        de_crypt(&mut index, key);
        bytes[20..48].copy_from_slice(&index);
        let mut file = [0; 32];
        file[16..20].copy_from_slice(&3u32.to_le_bytes());
        file[20..24].copy_from_slice(&1u32.to_le_bytes());
        file[24..28].copy_from_slice(&1u32.to_le_bytes());
        de_crypt(&mut file, key);
        bytes[48..80].copy_from_slice(&file);
        let mut name = *b"a";
        de_crypt(&mut name, key);
        bytes[80] = name[0];
        bytes[81..85].copy_from_slice(&4u32.to_le_bytes());
        bytes[85..89].copy_from_slice(&[0, 7, 8, 9]);
        bytes
    }
    #[test]
    fn registration_open_read_seek_close_and_owner_release_share_storage() {
        let disk = Arc::new(Mutex::new(Disk::default()));
        disk.lock()
            .unwrap()
            .names
            .insert(b"pack".to_vec(), archive_fixture());
        disk.lock()
            .unwrap()
            .names
            .insert(b"a".to_vec(), vec![1, 2, 3]);
        let handles = Handles::new(Backend(disk.clone()));
        let owner = handles.dta_create(b"pack").unwrap().unwrap();
        assert_eq!(handles.add_ref(owner).unwrap(), Some(2));
        assert_eq!(handles.release(owner).unwrap(), Some(1));
        assert!(handles.unlock_pack(owner, 0).unwrap());
        let loose = handles.dta_open(b"a", false).unwrap();
        assert_eq!(loose, 0);
        let mut out = [0; 2];
        assert_eq!(handles.dta_read(loose, Some(&mut out)).unwrap(), 2);
        assert_eq!(out, [1, 2]);
        assert!(handles.dta_close(loose).unwrap());
        let archived = handles.dta_open(b"a", true).unwrap();
        assert_eq!(archived, 0);
        assert_eq!(handles.dta_read(archived, Some(&mut out)).unwrap(), 2);
        assert_eq!(out, [7, 8]);
        assert_eq!(handles.dta_seek(archived, 1, 0).unwrap(), 1);
        assert_eq!(handles.dta_read(archived, Some(&mut out)).unwrap(), 2);
        assert_eq!(out, [8, 9]);
        assert_eq!(handles.release(owner).unwrap(), Some(0));
        assert_eq!(handles.dta_read(archived, Some(&mut out)).unwrap(), FAILURE);
        assert!(handles.dta_close(archived).unwrap());
        assert_eq!(
            disk.lock().unwrap().calls,
            ["close 1", "block 2", "close 0", "block 3", "owner 1"]
        );
    }
    fn wav_archive_fixture(encrypted: bool) -> Vec<u8> {
        let key = 0x3498576239475694;
        let mut bytes = archive_fixture();
        let mut file: [u8; 32] = bytes[48..80].try_into().unwrap();
        de_crypt(&mut file, key);
        file[16..20].copy_from_slice(&32774u32.to_le_bytes());
        file[20..24].copy_from_slice(&2u32.to_le_bytes());
        file[24..28].copy_from_slice(&(1u32 | if encrypted { 0x8000 } else { 0 }).to_le_bytes());
        de_crypt(&mut file, key);
        bytes[48..80].copy_from_slice(&file);
        let mut header = [0; 44];
        header[..4].copy_from_slice(b"RIFF");
        header[12..16].copy_from_slice(b"fmt ");
        header[36..40].copy_from_slice(b"data");
        header[22..24].copy_from_slice(&1u16.to_le_bytes());
        header[32..34].copy_from_slice(&2u16.to_le_bytes());
        let mut first = vec![8];
        first.extend_from_slice(&header);
        first.extend_from_slice(&32767i16.to_le_bytes());
        first.extend(std::iter::repeat_n(1, 16361));
        let first_size = first.len() as u32;
        if encrypted {
            de_crypt(&mut first, key);
        } else {
            de_crypt(&mut first[1..45], key);
        }
        let mut second = vec![8, 100, 0, 1, 129];
        if encrypted {
            de_crypt(&mut second, key);
        }
        bytes.truncate(81);
        bytes.extend_from_slice(&first_size.to_le_bytes());
        bytes.extend_from_slice(&first);
        bytes.extend_from_slice(&((first_size << 16) | second.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&second);
        bytes
    }
    #[test]
    fn wav_open_read_reload_and_release_preserve_header_lifetime_and_cipher_calls() {
        for encrypted in [false, true] {
            let disk = Arc::new(Mutex::new(Disk::default()));
            disk.lock()
                .unwrap()
                .names
                .insert(b"pack".to_vec(), wav_archive_fixture(encrypted));
            let handles = Handles::new(Backend(disk.clone()));
            let owner = handles.dta_create(b"pack").unwrap().unwrap();
            assert!(handles.unlock_pack(owner, 0).unwrap());
            let h = handles.dta_open(b"a", true).unwrap();
            let cache = handles.inspect_archive_cache(h).unwrap().unwrap();
            assert_eq!(&cache.wav_header.unwrap()[..4], b"RIFF");
            let mut first = [0; 46];
            assert_eq!(handles.dta_read(h, Some(&mut first)).unwrap(), 46);
            assert_eq!(&first[..4], b"RIFF");
            assert_eq!(&first[44..], &[255, 127]);
            handles.dta_seek(h, 32768, 0).unwrap();
            let mut last = [0; 6];
            assert_eq!(handles.dta_read(h, Some(&mut last)).unwrap(), 6);
            assert_eq!(last, [100, 0, 101, 0, 100, 0]);
            handles.dta_seek(h, 0, 0).unwrap();
            assert_eq!(handles.dta_read(h, Some(&mut first)).unwrap(), 46);
            assert_eq!(&first[..4], b"RIFF");
            assert_eq!(handles.release(owner).unwrap(), Some(0));
            assert!(handles.inspect_archive_cache(h).unwrap().is_none());
            assert_eq!(
                disk.lock().unwrap().calls,
                ["close 0", "block 3", "wav 2", "owner 1"]
            );
        }
    }
    #[test]
    fn failed_constructor_retains_native_unsigned_underflow_leak() {
        let disk = Arc::new(Mutex::new(Disk::default()));
        disk.lock()
            .unwrap()
            .names
            .insert(b"bad".to_vec(), b"NOPE".to_vec());
        let handles = Handles::new(Backend(disk.clone()));
        assert_eq!(handles.dta_create(b"bad").unwrap(), None);
        let state = handles.state.lock().unwrap();
        assert_eq!(state.owners[0].refs, u32::MAX);
        assert!(!state.owners[0].registered);
        assert!(disk.lock().unwrap().calls.is_empty());
        assert_eq!(disk.lock().unwrap().files.len(), 1);
    }
}
