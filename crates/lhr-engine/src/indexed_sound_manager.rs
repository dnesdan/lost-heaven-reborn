//! Source-backed sounds.dat v2 intake and ConstructSound55f820. Numeric material
//! sound IDs select real table rows; they are never interpreted as pointers.
use crate::vehicle_init::VehicleInitBackend;
pub trait IndexedSoundBackend: VehicleInitBackend {
    fn indexed_sound_table(&mut self) -> Result<Vec<u8>, Self::Error>;
    fn indexed_sound_open(&mut self, sound: u32, name_pointer: u32) -> Result<bool, Self::Error>;
    fn indexed_sound_virtual(
        &mut self,
        sound: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, Self::Error>;
}
#[derive(Debug)]
pub enum IndexedSoundError<E> {
    Backend(E),
    Data(&'static str),
    AllocationFailed,
    UnsupportedSoundType(u8),
}
type Out<B, T> = Result<T, IndexedSoundError<<B as VehicleInitBackend>::Error>>;
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u32> {
    b.read_word(a).map_err(IndexedSoundError::Backend)
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, x: u32) -> Out<B, ()> {
    b.write_word(a, x).map_err(IndexedSoundError::Backend)
}
fn byte<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u8> {
    b.read_byte(a).map_err(IndexedSoundError::Backend)
}
fn putb<B: VehicleInitBackend>(b: &mut B, a: u32, x: u8) -> Out<B, ()> {
    b.write_byte(a, x).map_err(IndexedSoundError::Backend)
}
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> Result<&[u8], &'static str> {
        let end = self.at.checked_add(n).ok_or("sounds.dat extent")?;
        let out = self.data.get(self.at..end).ok_or("truncated sounds.dat")?;
        self.at = end;
        Ok(out)
    }
    fn word(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn byte(&mut self) -> Result<u8, &'static str> {
        Ok(self.bytes(1)?[0])
    }
}
struct Row {
    id: u32,
    unknown: u32,
    names: Vec<(Vec<u8>, u32)>,
    settings: Vec<[u8; 36]>,
}
struct Table {
    header: [u32; 2],
    rows: Vec<Row>,
}
fn parse(data: &[u8]) -> Result<Option<Table>, &'static str> {
    let mut r = Reader { data, at: 0 };
    if r.word()? != 2 {
        return Ok(None);
    }
    let header = [r.word()?, r.word()?];
    let ignored = r.word()?;
    let skip = (ignored as usize)
        .checked_mul(36)
        .ok_or("sounds.dat skipped extent")?;
    r.bytes(skip)?;
    let count = r.word()?;
    if count > 65536 {
        return Err("sounds.dat row limit");
    }
    let mut rows = Vec::new();
    for _ in 0..count {
        let description = r.word()?;
        r.bytes(description as usize)?;
        let id = r.word()?;
        let unknown = r.word()?;
        let files = r.byte()?;
        let variants = r.byte()?;
        if files > 6 || variants > 4 {
            return Err("sounds.dat fixed row extent");
        }
        let mut names = Vec::new();
        for _ in 0..files {
            let length = r.word()?;
            if length > 65535 {
                return Err("sounds.dat filename extent");
            }
            let name = r.bytes(length as usize)?.to_vec();
            if !name.is_ascii() || name.contains(&0) {
                return Err("sounds.dat filename domain");
            }
            let threshold = r.word()?;
            names.push((name, threshold));
        }
        let mut settings = Vec::new();
        for _ in 0..variants {
            settings.push(r.bytes(36)?.try_into().unwrap());
        }
        rows.push(Row {
            id,
            unknown,
            names,
            settings,
        });
    }
    Ok(Some(Table { header, rows }))
}
fn rows<B: VehicleInitBackend>(b: &mut B, m: u32) -> Out<B, (u32, u32)> {
    let p = word(b, m + 0x10)?;
    if p == 0 {
        return Ok((0, 0));
    }
    let end = word(b, m + 0x14)?;
    let bytes = end
        .checked_sub(p)
        .filter(|n| n.is_multiple_of(0xd0) && *n <= 65536 * 0xd0)
        .ok_or(IndexedSoundError::Data("sound manager row extent"))?;
    Ok((p, bytes / 0xd0))
}
/// Explicit original startup Load55f390 composition. Empty ConstructSound does
/// not manufacture a load; the standalone caller invokes this once at startup.
pub fn load_sound_manager<B: IndexedSoundBackend>(b: &mut B, manager: u32) -> Out<B, bool> {
    let data = b
        .indexed_sound_table()
        .map_err(IndexedSoundError::Backend)?;
    let Some(table) = parse(&data).map_err(IndexedSoundError::Data)? else {
        return Ok(false);
    };
    cleanup_sound_manager(b, manager)?;
    put(b, manager + 4, table.header[0])?;
    put(b, manager + 8, table.header[1])?;
    if table.rows.is_empty() {
        return Ok(true);
    }
    let allocation = b
        .allocate(table.rows.len() as u32 * 0xd0)
        .map_err(IndexedSoundError::Backend)?;
    if allocation == 0 {
        return Err(IndexedSoundError::AllocationFailed);
    }
    put(b, manager + 0x10, allocation)?;
    put(b, manager + 0x14, allocation)?;
    put(
        b,
        manager + 0x18,
        allocation + table.rows.len() as u32 * 0xd0,
    )?;
    for row in table.rows {
        let target = word(b, manager + 0x14)?;
        for i in 0..0x34 {
            put(b, target + i * 4, 0)?;
        }
        put(b, manager + 0x14, target + 0xd0)?;
        putb(b, target + 0x3e, 1)?;
        put(b, target + 4, row.id)?;
        put(b, target + 8, row.unknown)?;
        putb(b, target + 0x3c, row.names.len() as u8)?;
        putb(b, target + 0x3d, row.settings.len() as u8)?;
        for (i, (name, threshold)) in row.names.into_iter().enumerate() {
            if !name.is_empty() {
                let p = b
                    .allocate(name.len() as u32 + 1)
                    .map_err(IndexedSoundError::Backend)?;
                if p == 0 {
                    return Err(IndexedSoundError::AllocationFailed);
                }
                put(b, target + 0xc + i as u32 * 4, p)?;
                for (j, x) in name.into_iter().chain([0]).enumerate() {
                    putb(b, p + j as u32, x)?;
                }
            }
            put(b, target + 0x24 + i as u32 * 4, threshold)?;
        }
        for (i, setting) in row.settings.into_iter().enumerate() {
            for (j, x) in setting.into_iter().enumerate() {
                putb(b, target + 0x40 + i as u32 * 36 + j as u32, x)?;
            }
        }
    }
    Ok(true)
}
fn random<B: VehicleInitBackend>(b: &mut B, limit: u32) -> Out<B, u32> {
    if limit < 1 {
        return Ok(0);
    }
    let seed = word(b, 0x63a200)?;
    let next = seed.wrapping_mul(214013).wrapping_add(2531011);
    put(b, 0x63a200, next)?;
    Ok(((next >> 16) & 0x7fff) % limit)
}
fn virtual_call<B: IndexedSoundBackend>(
    b: &mut B,
    sound: u32,
    slot: u32,
    args: &[u32],
) -> Out<B, ()> {
    b.indexed_sound_virtual(sound, slot, args)
        .map_err(IndexedSoundError::Backend)?;
    Ok(())
}
/// Original55f820, preserving reverse ID lookup, two reached RNG draws,
/// inclusive weighted thresholds, load-failure row disable and setting order.
pub fn construct_sound<B: IndexedSoundBackend>(
    b: &mut B,
    manager: u32,
    sound: u32,
    id: u32,
) -> Out<B, bool> {
    let (base, count) = rows(b, manager)?;
    for i in (0..count).rev() {
        let row = base + i * 0xd0;
        if word(b, row + 4)? != id {
            continue;
        }
        let files = byte(b, row + 0x3c)?;
        let variants = byte(b, row + 0x3d)?;
        if byte(b, row + 0x3e)? == 0 || files == 0 || variants == 0 {
            return Ok(false);
        }
        if files > 6 || variants > 4 {
            return Err(IndexedSoundError::Data("sound row fixed extent"));
        }
        let draw = random(b, 100)? as i32;
        let mut choice = 0;
        while choice < u32::from(files) && draw > word(b, row + 0x24 + choice * 4)? as i32 {
            choice += 1;
        }
        if choice == u32::from(files) {
            choice -= 1
        }
        let variant = random(b, u32::from(variants))?;
        let p = word(b, row + 0xc + choice * 4)?;
        if !b
            .indexed_sound_open(sound, p)
            .map_err(IndexedSoundError::Backend)?
        {
            putb(b, row + 0x3e, 0)?;
            return Ok(false);
        }
        let config = row + 0x40 + variant * 36;
        let kind = byte(b, config)?;
        match kind {
            0 => virtual_call(b, sound, 0x58, &[3])?,
            1 => virtual_call(b, sound, 0x58, &[1])?,
            2 => virtual_call(b, sound, 0x58, &[2])?,
            3 => return Err(IndexedSoundError::UnsupportedSoundType(3)),
            _ => {}
        }
        let mut range = [0; 4];
        for (i, v) in range.iter_mut().enumerate() {
            *v = word(b, config + 4 + i as u32 * 4)?;
        }
        virtual_call(b, sound, 0x5c, &range)?;
        let cone = [word(b, config + 0x14)?, word(b, config + 0x18)?];
        virtual_call(b, sound, 0x60, &cone)?;
        let volume = word(b, config + 0x1c)?;
        virtual_call(b, sound, 0x68, &[volume])?;
        let out = word(b, config + 0x20)?;
        virtual_call(b, sound, 0x64, &[out])?;
        return Ok(true);
    }
    Ok(false)
}
/// Releases only owned table filename allocations and row storage. The voices
/// constructed from it retain their own existing RuntimeAudio source ownership.
pub fn cleanup_sound_manager<B: VehicleInitBackend>(b: &mut B, manager: u32) -> Out<B, ()> {
    let (base, count) = rows(b, manager)?;
    for i in 0..count {
        let row = base + i * 0xd0;
        let files = byte(b, row + 0x3c)?;
        if files > 6 {
            return Err(IndexedSoundError::Data("sound cleanup filename count"));
        }
        for j in 0..u32::from(files) {
            let p = word(b, row + 0xc + j * 4)?;
            if p != 0 {
                b.free(p).map_err(IndexedSoundError::Backend)?;
                put(b, row + 0xc + j * 4, 0)?;
            }
        }
    }
    if base != 0 {
        b.free(base).map_err(IndexedSoundError::Backend)?;
    }
    for o in [0x10, 0x14, 0x18] {
        put(b, manager + o, 0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    struct Owner {
        memory: VehicleFrameMemory,
        table: Vec<u8>,
        opens: Vec<Vec<u8>>,
        calls: Vec<(u32, Vec<u32>)>,
        accept: bool,
    }
    impl Owner {
        fn new(rows: &[(&str, u32)]) -> Self {
            let mut table = Vec::new();
            for x in [2u32, 7, 8, 0, rows.len() as u32] {
                table.extend(x.to_le_bytes());
            }
            for (name, id) in rows {
                table.extend(3u32.to_le_bytes());
                table.extend(b"row");
                table.extend(id.to_le_bytes());
                table.extend(99u32.to_le_bytes());
                table.extend([1, 1]);
                table.extend((name.len() as u32).to_le_bytes());
                table.extend(name.as_bytes());
                table.extend(99u32.to_le_bytes());
                let mut config = [0; 36];
                config[0] = 2;
                for (i, value) in [1f32, 10., 0.5, 0.25, 0.6, 1.2, 0.75, 0.2]
                    .into_iter()
                    .enumerate()
                {
                    config[4 + i * 4..8 + i * 4].copy_from_slice(&value.to_le_bytes());
                }
                table.extend(config);
            }
            let mut memory = VehicleFrameMemory::default();
            memory.map_bytes(0x646f50, &[0; 28]).unwrap();
            memory.map_bytes(0x63a200, &0u32.to_le_bytes()).unwrap();
            Self {
                memory,
                table,
                opens: Vec::new(),
                calls: Vec::new(),
                accept: true,
            }
        }
    }
    impl VehicleInitBackend for Owner {
        type Error = VehicleMemoryError;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.memory.read_byte(a)
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            self.memory.read_word(a)
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            self.memory.write_byte(a, v)
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.memory.write_word(a, v)
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.memory.allocate(n)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.memory.free(a)
        }
        fn collect_frame(
            &mut self,
            v: &mut VehicleInitFrameVector,
            f: u32,
        ) -> Result<(), Self::Error> {
            self.memory.collect_frame(v, f)
        }
        fn enumerate(
            &mut self,
            m: u32,
            q: VehicleInitQuery,
            t: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            self.memory.enumerate(m, q, t)
        }
    }
    impl IndexedSoundBackend for Owner {
        fn indexed_sound_table(&mut self) -> Result<Vec<u8>, Self::Error> {
            Ok(self.table.clone())
        }
        fn indexed_sound_open(&mut self, _: u32, p: u32) -> Result<bool, Self::Error> {
            let mut name = Vec::new();
            for i in 0..256 {
                let c = self.read_byte(p + i)?;
                if c == 0 {
                    break;
                }
                name.push(c);
            }
            self.opens.push(name);
            Ok(self.accept)
        }
        fn indexed_sound_virtual(&mut self, _: u32, s: u32, a: &[u32]) -> Result<u32, Self::Error> {
            self.calls.push((s, a.to_vec()));
            Ok(0)
        }
    }
    #[test]
    fn indexed_table_owns_names_configures_four_range_floats_and_cleans_up() {
        let mut b = Owner::new(&[("hit.wav", 42)]);
        assert!(!construct_sound(&mut b, 0x646f50, 0x123, 42).unwrap());
        assert!(b.opens.is_empty());
        assert!(load_sound_manager(&mut b, 0x646f50).unwrap());
        assert_eq!(b.read_word(0x646f54).unwrap(), 7);
        assert_eq!(b.read_word(0x646f58).unwrap(), 8);
        assert!(construct_sound(&mut b, 0x646f50, 0x123, 42).unwrap());
        assert_eq!(b.opens, vec![b"hit.wav".to_vec()]);
        assert_eq!(
            b.calls.iter().map(|x| x.0).collect::<Vec<_>>(),
            vec![0x58, 0x5c, 0x60, 0x68, 0x64]
        );
        assert_eq!(b.calls[0].1, vec![2]);
        assert_eq!(
            b.calls[1].1,
            vec![
                1f32.to_bits(),
                10f32.to_bits(),
                0.5f32.to_bits(),
                0.25f32.to_bits()
            ]
        );
        assert_eq!(b.calls[2].1, vec![0.6f32.to_bits(), 1.2f32.to_bits()]);
        assert_ne!(b.read_word(0x63a200).unwrap(), 0);
        cleanup_sound_manager(&mut b, 0x646f50).unwrap();
        assert_eq!(b.memory.allocations().count(), 0);
    }
    #[test]
    fn failed_reverse_id_match_disables_only_chosen_row_without_later_rng() {
        let mut b = Owner::new(&[("old.wav", 42), ("new.wav", 42)]);
        load_sound_manager(&mut b, 0x646f50).unwrap();
        b.accept = false;
        assert!(!construct_sound(&mut b, 0x646f50, 0x123, 42).unwrap());
        assert_eq!(b.opens, vec![b"new.wav".to_vec()]);
        let seed = b.read_word(0x63a200).unwrap();
        assert!(!construct_sound(&mut b, 0x646f50, 0x123, 42).unwrap());
        assert_eq!(b.read_word(0x63a200).unwrap(), seed);
        assert_eq!(b.opens.len(), 1);
        assert!(b.calls.is_empty());
        cleanup_sound_manager(&mut b, 0x646f50).unwrap();
        assert_eq!(b.memory.allocations().count(), 0);
    }
}
