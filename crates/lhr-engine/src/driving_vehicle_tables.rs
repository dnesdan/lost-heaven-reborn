//! Original-data ownership for driving's material and tyre tables.
//!
//! This bounded host intake composes the field projections in C_car's
//! LoadMaterials (PC RVA 0x1c7f0), LoadTyres (0x1d480), ConvertTyres (0x1d770)
//! and C_Vehicle table replacement. It does not import native heap images or
//! manufacture surface coefficients. Pointer/header allocation remains with
//! the caller's vehicle-memory owner.
use lhr_assets::Installation;

pub const MATERIAL_SOURCE_STRIDE: usize = 0x170;
pub const VEHICLE_MATERIAL_STRIDE: usize = 0x30;
pub const TYRE_SURFACE_STRIDE: usize = 0x5c;
const MAX_TABLE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct DrivingTyreDefinition {
    pub id: u32,
    pub name_bytes: [u8; 32],
    /// The original file's group extent word, retained without using it as a
    /// pointer or changing source surface order.
    pub source_extent: u32,
    /// Runtime S_SRFC values, including the original material ID at byte zero.
    pub surfaces: Vec<[u8; TYRE_SURFACE_STRIDE]>,
}
pub struct DrivingVehicleTables {
    material_source: Vec<u8>,
    tyre_source: Vec<u8>,
    material_bytes: Vec<u8>,
    material_index: [u8; 256],
    tyres: Vec<DrivingTyreDefinition>,
}
fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn float(bytes: &[u8], at: usize) -> f32 {
    f32::from_bits(word(bytes, at))
}
fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_float(bytes: &mut [u8], at: usize, value: f32) {
    put(bytes, at, value.to_bits());
}
fn clamp_ordered(value: f32, min: f32, max: f32) -> f32 {
    // C_car::LoadMaterials uses ordered `<` / `>` branches. Unordered NaNs
    // pass through with their payload; f32::clamp and reversed comparisons do
    // not reproduce that behavior.
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
fn materials(source: &[u8]) -> Result<(Vec<u8>, [u8; 256]), String> {
    if !source.len().is_multiple_of(MATERIAL_SOURCE_STRIDE)
        || source.len() / MATERIAL_SOURCE_STRIDE > 256
    {
        return Err("Original material definitions have an unsupported extent".into());
    }
    // InitSystem reaches C_car::LoadMaterials before the profile tables are
    // loaded. Its numOfMaterials < 1 branch publishes one default row.
    if source.is_empty() {
        let mut row = [0; VEHICLE_MATERIAL_STRIDE];
        for (offset, value) in [
            (0x04, 100.0),
            (0x08, 100.0),
            (0x0c, 100.0),
            (0x10, 100.0),
            (0x14, 0.0),
            (0x18, 0.35),
            (0x1c, 1.0),
        ] {
            put_float(&mut row, offset, value);
        }
        return Ok((row.to_vec(), [0; 256]));
    }
    let mut rows =
        Vec::with_capacity(source.len() / MATERIAL_SOURCE_STRIDE * VEHICLE_MATERIAL_STRIDE);
    let mut index = [0; 256];
    for (id, source) in source
        .as_chunks::<MATERIAL_SOURCE_STRIDE>()
        .0
        .iter()
        .enumerate()
    {
        let mut row = [0; VEHICLE_MATERIAL_STRIDE];
        put(&mut row, 0, id as u32);
        for (from, to) in [
            (0x114, 4),
            (0x110, 8),
            (0x11c, 0xc),
            (0x118, 0x10),
            (0x120, 0x14),
            (0x108, 0x18),
            (0x10c, 0x1c),
            (0x124, 0x20),
            (0x128, 0x24),
            (0x12c, 0x28),
        ] {
            put(&mut row, to, word(source, from));
        }
        let grip = clamp_ordered(float(&row, 0x18), 0., 1.);
        put_float(&mut row, 0x18, grip);
        let scale = clamp_ordered(float(&row, 0x1c), 1., 2.);
        put_float(&mut row, 0x1c, scale);
        if float(&row, 8) < float(&row, 4) {
            put(&mut row, 4, word(source, 0x110));
        }
        if float(&row, 0x10) < float(&row, 0xc) {
            put(&mut row, 0xc, word(source, 0x118));
        }
        let drag = clamp_ordered(float(&row, 0x14), 0., f32::from_bits(0x3e4ccccd));
        put_float(&mut row, 0x14, drag);
        // Original source-row offset comparisons, not a driving-model rule.
        let offset = id * MATERIAL_SOURCE_STRIDE;
        if (offset > 0 && offset < 0x730) || matches!(offset, 0xcf0 | 0xe60 | 0x36a0) {
            row[0x2c] = 1;
            row[0x2d] = 1;
        }
        index[id] = id as u8;
        rows.extend_from_slice(&row);
    }
    Ok((rows, index))
}
struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(n)
            .ok_or("Tyre source extent overflow")?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or("Truncated original tyre table")?;
        self.position = end;
        Ok(bytes)
    }
    fn word(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}
/// Pointer-free original ConvertTyres field projection. The input extent may
/// include the file's trailing fields; the original converter reads 18 words.
pub fn convert_tyre_payload(
    material: u32,
    source: &[u8],
) -> Result<[u8; TYRE_SURFACE_STRIDE], String> {
    if source.len() < 72 {
        return Err("Original tyre surface has fewer than 18 source words".into());
    }
    let mut row = [0; TYRE_SURFACE_STRIDE];
    put(&mut row, 0, material);
    for (from, to) in [
        (8, 0xc),
        (0xc, 0x10),
        (0x10, 0x14),
        (0x14, 0x18),
        (0x18, 0x20),
        (0x1c, 0x24),
        (0x20, 0x2c),
        (0x2c, 0x38),
        (0x30, 0x3c),
        (0x34, 0x40),
        (0x38, 0x44),
        (0x3c, 0x4c),
        (0x40, 0x50),
        (0x44, 0x58),
    ] {
        put(&mut row, to, word(source, from));
    }
    for (from, to, scale) in [
        (0, 4, f32::from_bits(0x447a0000)),
        (4, 8, f32::from_bits(0x3c8efa2e)),
        (0x24, 0x30, f32::from_bits(0x447a0000)),
        (0x28, 0x34, f32::from_bits(0x3c8efa2e)),
    ] {
        let value = crate::collision_swept::mul(float(source, from), scale)
            .map_err(|e| format!("Original tyre conversion arithmetic: {e:?}"))?;
        put_float(&mut row, to, value);
    }
    for (hi, lo, to) in [
        (0x18, 0x14, 0x1c),
        (0x24, 0x20, 0x28),
        (0x44, 0x40, 0x48),
        (0x50, 0x4c, 0x54),
    ] {
        let value = crate::collision_swept::sub(float(&row, hi), float(&row, lo))
            .map_err(|e| format!("Original tyre conversion arithmetic: {e:?}"))?;
        put_float(&mut row, to, value);
    }
    Ok(row)
}
fn tyres(source: &[u8]) -> Result<Vec<DrivingTyreDefinition>, String> {
    let mut cursor = Cursor {
        bytes: source,
        position: 0,
    };
    if cursor.word()? != 1 {
        return Err("Unsupported original tyre table version".into());
    }
    let count = cursor.word()? as usize;
    if count > 256 {
        return Err("Original tyre definition quota exceeded".into());
    }
    let mut tyres = Vec::with_capacity(count);
    let mut surfaces = 0_usize;
    for _ in 0..count {
        let source_extent = cursor.word()?;
        let name_bytes = cursor.take(32)?.try_into().unwrap();
        let id = cursor.word()?;
        let count = cursor.word()? as usize;
        surfaces = surfaces
            .checked_add(count)
            .filter(|n| *n <= 16384)
            .ok_or("Original tyre surface quota exceeded")?;
        let mut rows = Vec::with_capacity(count);
        for _ in 0..count {
            let material = cursor.word()?;
            let size = cursor.word()? as usize;
            if !(72..=4096).contains(&size) {
                return Err("Unsupported original tyre surface payload extent".into());
            }
            rows.push(convert_tyre_payload(material, cursor.take(size)?)?);
        }
        tyres.push(DrivingTyreDefinition {
            id,
            name_bytes,
            source_extent,
            surfaces: rows,
        });
    }
    if cursor.position != source.len() {
        return Err("Original tyre table has unsupported trailing data".into());
    }
    Ok(tyres)
}
impl DrivingVehicleTables {
    pub fn load(installation: &Installation) -> Result<Self, String> {
        let material_source = installation
            .read("tables/materials.def")
            .map_err(|e| format!("Original material definitions: {e}"))?;
        let tyre_source = installation
            .read("tables/tyres.bin")
            .map_err(|e| format!("Original tyre definitions: {e}"))?;
        Self::from_source_bytes(material_source, tyre_source)
    }
    /// Owned source-byte intake also supports public synthetic format fixtures.
    pub fn from_source_bytes(
        material_source: Vec<u8>,
        tyre_source: Vec<u8>,
    ) -> Result<Self, String> {
        if material_source
            .len()
            .checked_add(tyre_source.len())
            .is_none_or(|n| n > MAX_TABLE_BYTES)
        {
            return Err("Original driving table source quota exceeded".into());
        }
        let (material_bytes, material_index) = materials(&material_source)?;
        let tyres = tyres(&tyre_source)?;
        Ok(Self {
            material_source,
            tyre_source,
            material_bytes,
            material_index,
            tyres,
        })
    }
    pub fn material_index_map(&self) -> &[u8; 256] {
        &self.material_index
    }
    pub fn material_bytes(&self) -> &[u8] {
        &self.material_bytes
    }
    pub fn tyres(&self) -> &[DrivingTyreDefinition] {
        &self.tyres
    }
    pub fn source_material_bytes(&self) -> &[u8] {
        &self.material_source
    }
    pub fn source_tyre_bytes(&self) -> &[u8] {
        &self.tyre_source
    }
    /// Native SetTyres prepends this empty named slot before source records.
    /// Pointer fields are null until the caller materializes owned surfaces.
    pub fn user_tyre_record() -> [u8; 0x28] {
        let mut bytes = [0; 0x28];
        bytes[4..9].copy_from_slice(b"user\0");
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn empty_tyres() -> Vec<u8> {
        [1_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat()
    }
    #[test]
    fn absent_material_table_yields_native_startup_default_and_unordered_nan_survives() {
        let empty = DrivingVehicleTables::from_source_bytes(Vec::new(), empty_tyres()).unwrap();
        let row = empty.material_bytes();
        assert_eq!(row.len(), VEHICLE_MATERIAL_STRIDE);
        assert_eq!(float(row, 0x04), 100.0);
        assert_eq!(float(row, 0x08), 100.0);
        assert_eq!(float(row, 0x0c), 100.0);
        assert_eq!(float(row, 0x10), 100.0);
        assert_eq!(float(row, 0x14), 0.0);
        assert_eq!(float(row, 0x18), 0.35);
        assert_eq!(float(row, 0x1c), 1.0);
        assert_eq!(&row[0x20..], [0; 16]);
        let mut source = vec![0; MATERIAL_SOURCE_STRIDE];
        put(&mut source, 0x108, 0x7fc1_2345);
        put(&mut source, 0x10c, 0xffc1_2345);
        put(&mut source, 0x120, 0x7fc5_4321);
        let table = DrivingVehicleTables::from_source_bytes(source, empty_tyres()).unwrap();
        for (at, bits) in [
            (0x18, 0x7fc1_2345),
            (0x1c, 0xffc1_2345),
            (0x14, 0x7fc5_4321),
        ] {
            assert_eq!(word(table.material_bytes(), at), bits);
        }
    }
    #[test]
    fn material_projection_keeps_integer_fields_and_original_clamps() {
        let mut source = vec![0; 2 * MATERIAL_SOURCE_STRIDE];
        for (at, v) in [
            (0x108, 2.),
            (0x10c, 0.5),
            (0x110, 2.),
            (0x114, 3.),
            (0x118, 4.),
            (0x11c, 5.),
            (0x120, 0.5),
        ] {
            put_float(&mut source, at, v);
        }
        put(&mut source, 0x124, u32::MAX);
        put(&mut source, 0x128, 0x7fc12345);
        let tables = DrivingVehicleTables::from_source_bytes(source, empty_tyres()).unwrap();
        let row = &tables.material_bytes()[..VEHICLE_MATERIAL_STRIDE];
        assert_eq!((float(row, 4), float(row, 8)), (2., 2.));
        assert_eq!((float(row, 0xc), float(row, 0x10)), (4., 4.));
        assert_eq!(float(row, 0x14).to_bits(), 0x3e4ccccd);
        assert_eq!(float(row, 0x18), 1.);
        assert_eq!(float(row, 0x1c), 1.);
        assert_eq!(word(row, 0x20), u32::MAX);
        assert_eq!(word(row, 0x24), 0x7fc12345);
        assert_eq!(&row[0x2c..], [0; 4]);
        assert_eq!(
            &tables.material_bytes()
                [VEHICLE_MATERIAL_STRIDE + 0x2c..VEHICLE_MATERIAL_STRIDE + 0x2e],
            [1, 1]
        );
        assert_eq!(&tables.material_index_map()[..3], [0, 1, 0]);
    }
    #[test]
    fn original_tyre_source_order_and_full_payload_extent_are_retained() {
        let mut source = Vec::new();
        source.extend(1_u32.to_le_bytes());
        source.extend(1_u32.to_le_bytes());
        source.extend(128_u32.to_le_bytes());
        let mut name = [0; 32];
        name[..9].copy_from_slice(b"synthetic");
        source.extend(name);
        source.extend(42_u32.to_le_bytes());
        source.extend(1_u32.to_le_bytes());
        source.extend(13_u32.to_le_bytes());
        source.extend(76_u32.to_le_bytes());
        for v in 1..=18 {
            source.extend((v as f32).to_le_bytes());
        }
        source.extend(0xdeadbeef_u32.to_le_bytes());
        let tables = DrivingVehicleTables::from_source_bytes(
            vec![0; MATERIAL_SOURCE_STRIDE],
            source.clone(),
        )
        .unwrap();
        let tyre = &tables.tyres()[0];
        assert_eq!(tyre.id, 42);
        assert_eq!(tyre.name_bytes, name);
        assert_eq!(tyre.source_extent, 128);
        assert_eq!(tyre.surfaces.len(), 1);
        let row = &tyre.surfaces[0];
        assert_eq!(word(row, 0), 13);
        assert_eq!(float(row, 4), 1000.);
        assert_eq!(float(row, 0x30), 10000.);
        for at in [0x1c, 0x28, 0x48, 0x54] {
            assert_eq!(float(row, at), 1.);
        }
        for end in 0..source.len() {
            assert!(
                DrivingVehicleTables::from_source_bytes(
                    vec![0; MATERIAL_SOURCE_STRIDE],
                    source[..end].to_vec()
                )
                .is_err(),
                "accepted truncated source at {end}"
            );
        }
    }
}
