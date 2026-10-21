//! Bounded read-only static 4DS v29 and BMP intake. Source coordinates and order are preserved.
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};
const MAX_BYTES: usize = 64 * 1024 * 1024;
#[derive(Debug, Clone)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn err<T>(s: &str) -> Result<T> {
    Err(Error(s.into()))
}
#[derive(Debug, Clone)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}
#[derive(Debug, Clone)]
pub struct FaceGroup {
    pub material: usize,
    pub indices: Vec<u32>,
}
#[derive(Debug, Clone)]
pub struct Lod {
    pub distance: f32,
    pub vertices: Vec<Vertex>,
    pub groups: Vec<FaceGroup>,
}
#[derive(Debug, Clone)]
pub struct Material {
    pub flags: u32,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub texture: String,
}
#[derive(Debug, Clone)]
pub struct Frame {
    pub name: String,
    pub parent: Option<usize>,
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub rotation_wxyz: [f32; 4],
    pub instance: Option<usize>,
    pub lods: Vec<Lod>,
}
#[derive(Debug, Clone)]
pub struct Model {
    pub materials: Vec<Material>,
    pub frames: Vec<Frame>,
}
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .p
            .checked_add(n)
            .ok_or_else(|| Error("offset overflow".into()))?;
        let b = self
            .b
            .get(self.p..end)
            .ok_or_else(|| Error(format!("truncated input at byte {}", self.p)))?;
        self.p = end;
        Ok(b)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f(&mut self) -> Result<f32> {
        let v = f32::from_bits(self.u32()?);
        if !v.is_finite() {
            return err("nonfinite numeric field");
        }
        Ok(v)
    }
    fn vec<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut v = [0.; N];
        for x in &mut v {
            *x = self.f()?;
        }
        Ok(v)
    }
    fn string(&mut self) -> Result<String> {
        let n = self.u8()? as usize;
        let b = self.take(n)?;
        if !b.is_ascii() || b.contains(&0) {
            return err("non-ASCII or NUL name unsupported");
        }
        Ok(String::from_utf8(b.to_vec()).unwrap())
    }
}
pub fn parse_four_ds(b: &[u8]) -> Result<Model> {
    if b.len() > MAX_BYTES {
        return err("4DS exceeds 64 MiB quota");
    }
    let mut r = Reader { b, p: 0 };
    if r.take(4)? != b"4DS\0" || r.u16()? != 29 {
        return err("expected 4DS version 29");
    };
    r.take(8)?;
    let count = r.u16()? as usize;
    let mut materials = Vec::new();
    for _ in 0..count {
        let flags = r.u32()?;
        let ambient = r.vec()?;
        let diffuse = r.vec()?;
        let emission: [f32; 3] = r.vec()?;
        let opacity = r.f()?;
        if flags
            & (0x8000 | 0x80000 | 0x01000000 | 0x04000000 | 0x20000000 | 0x40000000 | 0x80000000)
            != 0
            || opacity != 1.
            || emission != [0.; 3]
        {
            return err("unsupported alpha, reflection, emissive or animated material");
        }
        if ambient
            .iter()
            .chain(diffuse.iter())
            .any(|x| !(0. ..=1.).contains(x))
        {
            return err("material color outside 0..1");
        }
        let texture = r.string()?;
        if !texture.is_empty() {
            safe_relative(&texture)?;
            if !texture.to_ascii_lowercase().ends_with(".bmp") {
                return err("only BMP textures supported");
            }
        }
        materials.push(Material {
            flags,
            ambient,
            diffuse,
            texture,
        });
    }
    let count = r.u16()? as usize;
    if count > 16384 {
        return err("frame quota exceeded");
    };
    let mut frames = Vec::new();
    let mut total_vertices = 0usize;
    let mut total_indices = 0usize;
    for _ in 0..count {
        let kind = r.u8()?;
        if kind != 1 && kind != 6 {
            return err("unsupported frame type; only static visual and dummy supported");
        };
        if kind == 1 {
            let visual = r.u8()?;
            if visual > 1 {
                return err("unsupported visual type; only standard static mesh supported");
            };
            r.take(2)?;
        }
        let parent = r.u16()? as usize;
        let position = r.vec()?;
        let scale = r.vec()?;
        if scale.iter().any(|x| *x <= 0.) {
            return err("nonpositive scale unsupported");
        };
        let rotation_wxyz: [f32; 4] = r.vec()?;
        if rotation_wxyz
            .iter()
            .map(|x| (*x as f64).powi(2))
            .sum::<f64>()
            < 1e-20
        {
            return err("zero quaternion");
        };
        r.take(1)?;
        let name = r.string()?;
        r.string()?;
        let mut lods = Vec::new();
        let mut instance = None;
        if kind == 6 {
            r.take(24)?;
        } else {
            let id = r.u16()? as usize;
            if id != 0 {
                instance = Some(id - 1)
            } else {
                let n = r.u8()?;
                if n > 16 {
                    return err("LOD quota exceeded");
                };
                for _ in 0..n {
                    let distance = r.f()?;
                    let n = r.u16()? as usize;
                    total_vertices += n;
                    if total_vertices > 1_000_000 {
                        return err("vertex quota exceeded");
                    };
                    let mut vertices = Vec::new();
                    for _ in 0..n {
                        vertices.push(Vertex {
                            position: r.vec()?,
                            normal: r.vec()?,
                            uv: r.vec()?,
                        });
                    }
                    let groups = r.u8()?;
                    let mut faces = Vec::new();
                    for _ in 0..groups {
                        let n = r.u16()? as usize * 3;
                        total_indices += n;
                        if total_indices > 3_000_000 {
                            return err("index quota exceeded");
                        };
                        let mut indices = Vec::new();
                        for _ in 0..n {
                            let index = r.u16()? as usize;
                            if index >= vertices.len() {
                                return err("triangle index outside vertex array");
                            };
                            indices.push(index as u32)
                        }
                        let mat = r.u16()? as usize;
                        if mat == 0 || mat > materials.len() {
                            return err("material reference outside table");
                        };
                        faces.push(FaceGroup {
                            material: mat - 1,
                            indices,
                        });
                    }
                    lods.push(Lod {
                        distance,
                        vertices,
                        groups: faces,
                    });
                }
            }
        }
        frames.push(Frame {
            name,
            parent: parent.checked_sub(1),
            position,
            scale,
            rotation_wxyz,
            instance,
            lods,
        });
    }
    if r.u8()? != 0 {
        return err("animated model unsupported");
    };
    if r.p != b.len() {
        return err("trailing bytes after 4DS");
    }
    for (i, f) in frames.iter().enumerate() {
        let mut seen = vec![false; frames.len()];
        seen[i] = true;
        let mut p = f.parent;
        while let Some(n) = p {
            if n >= frames.len() || seen[n] {
                return err("invalid or cyclic frame hierarchy");
            };
            seen[n] = true;
            p = frames[n].parent;
        }
        if let Some(n) = f.instance
            && (n >= frames.len() || n == i || frames[n].lods.is_empty())
        {
            return err("invalid or chained mesh instance");
        }
    }
    Ok(Model { materials, frames })
}
fn safe_relative(s: &str) -> Result<()> {
    if s.is_empty()
        || s.contains(['\\', ':', '\0'])
        || Path::new(s)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return err("unsafe relative asset name");
    };
    Ok(())
}
/// Loose original installation files only. No external process or conversion is invoked.
pub struct Installation {
    root: PathBuf,
}
impl Installation {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = fs::canonicalize(root)
            .map_err(|_| Error("installation directory unavailable".into()))?;
        if !root.is_dir() {
            return err("installation root must be directory");
        };
        Ok(Self { root })
    }
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        safe_relative(name)?;
        let p = fs::canonicalize(self.root.join(name)).map_err(|_| {
            Error("original loose asset missing; DTA archive lookup is not yet supported".into())
        })?;
        if !p.starts_with(&self.root) {
            return err("asset resolves outside installation");
        };
        read_asset_file(p)
    }
}
/// Read a regular asset file with a 64 MiB allocation/read ceiling.
/// Errors intentionally contain no input paths or operating-system details.
pub fn read_asset_file(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path.as_ref()).map_err(|_| Error("asset unavailable".into()))?;
    if !metadata.is_file() {
        return err("asset must be a regular file");
    }
    if metadata.len() > MAX_BYTES as u64 {
        return err("asset exceeds 64 MiB quota");
    }
    let file = fs::File::open(path).map_err(|_| Error("asset unreadable".into()))?;
    let metadata = file
        .metadata()
        .map_err(|_| Error("asset metadata unavailable".into()))?;
    if !metadata.is_file() {
        return err("asset must be a regular file");
    }
    if metadata.len() > MAX_BYTES as u64 {
        return err("asset exceeds 64 MiB quota");
    }
    use std::io::Read;
    let mut b = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| Error("asset read failed".into()))?;
    if b.len() > MAX_BYTES {
        return err("asset exceeds 64 MiB quota");
    }
    Ok(b)
}
/// Uncompressed Windows BMP, 8-bit indexed or 24-bit BGR. Output is top-down RGBA.
pub fn decode_bmp(b: &[u8]) -> Result<Texture> {
    if b.len() > MAX_BYTES {
        return err("BMP exceeds 64 MiB quota");
    };
    let mut r = Reader { b, p: 0 };
    if r.take(2)? != b"BM" {
        return err("invalid BMP signature");
    };
    let size = r.u32()? as usize;
    r.take(4)?;
    let offset = r.u32()? as usize;
    let dib = r.u32()? as usize;
    if dib != 40 {
        return err("only BITMAPINFOHEADER BMP supported");
    };
    let width = r.u32()? as i32;
    let height = r.u32()? as i32;
    if width <= 0
        || height == 0
        || height == i32::MIN
        || width > 8192
        || height.unsigned_abs() > 8192
    {
        return err("BMP dimensions outside quota");
    };
    if r.u16()? != 1 {
        return err("invalid BMP planes");
    };
    let depth = r.u16()?;
    if depth != 8 && depth != 24 {
        return err("only indexed 8-bit and BGR24 BMP supported");
    };
    if r.u32()? != 0 {
        return err("compressed BMP unsupported");
    };
    r.take(12)?;
    let colors = r.u32()? as usize;
    r.take(4)?;
    let palette_count = if depth == 8 {
        if colors == 0 { 256 } else { colors }
    } else {
        0
    };
    if palette_count > 256 {
        return err("BMP palette exceeds 256 entries");
    };
    let mut palette = Vec::new();
    for _ in 0..palette_count {
        let c = r.take(4)?;
        palette.push([c[2], c[1], c[0], 255]);
    }
    if offset < r.p || size != b.len() {
        return err("BMP invalid data offset or file size");
    };
    let h = height.unsigned_abs() as usize;
    let w = width as usize;
    let row = (w * depth as usize).div_ceil(32) * 4;
    let end = offset
        .checked_add(row * h)
        .ok_or_else(|| Error("BMP extent overflow".into()))?;
    if end > b.len() || w * h > 16_777_216 {
        return err("BMP pixels truncated or exceed quota");
    };
    let mut rgba = vec![0; w * h * 4];
    for y in 0..h {
        let source_y = if height > 0 { h - 1 - y } else { y };
        for x in 0..w {
            let p = offset + source_y * row + x * (depth as usize / 8);
            let color = if depth == 8 {
                *palette
                    .get(b[p] as usize)
                    .ok_or_else(|| Error("BMP palette index out of range".into()))?
            } else {
                [b[p + 2], b[p + 1], b[p], 255]
            };
            rgba[(y * w + x) * 4..(y * w + x + 1) * 4].copy_from_slice(&color);
        }
    }
    Ok(Texture {
        width: width as u32,
        height: h as u32,
        rgba,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn empty() -> Vec<u8> {
        let mut b = b"4DS\0".to_vec();
        b.extend(29u16.to_le_bytes());
        b.extend([0; 8]);
        b.extend([0; 5]);
        b
    }
    #[test]
    fn empty_and_every_truncation() {
        let b = empty();
        assert!(parse_four_ds(&b).is_ok());
        for n in 0..b.len() {
            assert!(parse_four_ds(&b[..n]).is_err(), "{n}")
        }
        let mut b = b;
        b.push(0);
        assert!(parse_four_ds(&b).is_err())
    }
    #[test]
    fn rejects_version_and_names() {
        let mut b = empty();
        b[4] = 30;
        assert!(parse_four_ds(&b).is_err());
        for s in ["../x", "/x", "x\\y", "x:y", ""] {
            assert!(safe_relative(s).is_err())
        }
    }
    fn bmp() -> Vec<u8> {
        let mut b = vec![0; 58];
        b[..2].copy_from_slice(b"BM");
        b[2..6].copy_from_slice(&58u32.to_le_bytes());
        b[10..14].copy_from_slice(&54u32.to_le_bytes());
        b[14..18].copy_from_slice(&40u32.to_le_bytes());
        b[18..22].copy_from_slice(&1u32.to_le_bytes());
        b[22..26].copy_from_slice(&1u32.to_le_bytes());
        b[26] = 1;
        b[28] = 24;
        b[54..58].copy_from_slice(&[3, 2, 1, 0]);
        b
    }
    #[test]
    fn bmp_color_padding_truncation() {
        let b = bmp();
        let t = decode_bmp(&b).unwrap();
        assert_eq!(t.rgba, [1, 2, 3, 255]);
        for n in 0..b.len() {
            assert!(decode_bmp(&b[..n]).is_err())
        }
    }
    #[test]
    fn bmp_overflow_and_compression() {
        let mut b = bmp();
        b[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_bmp(&b).is_err());
        let mut b = bmp();
        b[30] = 1;
        assert!(decode_bmp(&b).is_err());
    }
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    fn mesh() -> Vec<u8> {
        let mut b = b"4DS\0".to_vec();
        b.extend(29u16.to_le_bytes());
        b.extend([0; 8]);
        b.extend(1u16.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        for x in [0f32, 0., 0., 1., 1., 1., 0., 0., 0., 1.] {
            b.extend(x.to_le_bytes());
        }
        b.push(0);
        b.extend(1u16.to_le_bytes());
        b.extend([1, 0, 0, 0]);
        b.extend(0u16.to_le_bytes());
        for x in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            b.extend(x.to_le_bytes());
        }
        b.extend([0, 0, 0]);
        b.extend(0u16.to_le_bytes());
        b.push(1);
        b.extend(0f32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        for x in [1f32, 2., 3., 0., 1., 0., 0.25, 0.75] {
            b.extend(x.to_le_bytes());
        }
        b.push(1);
        b.extend(1u16.to_le_bytes());
        b.extend([0; 6]);
        b.extend(1u16.to_le_bytes());
        b.push(0);
        b
    }
    #[test]
    fn source_order_and_invalid_references() {
        let b = mesh();
        let m = parse_four_ds(&b).unwrap();
        assert_eq!(m.frames[0].lods[0].vertices[0].position, [1., 2., 3.]);
        assert_eq!(m.frames[0].lods[0].groups[0].indices, [0, 0, 0]);
        for n in 0..b.len() {
            assert!(parse_four_ds(&b[..n]).is_err());
        }
        let mut invalid = b.clone();
        let n = invalid.len();
        invalid[n - 9] = 1;
        assert!(parse_four_ds(&invalid).is_err());
        let mut invalid = b;
        let n = invalid.len();
        invalid[n - 3] = 2;
        assert!(parse_four_ds(&invalid).is_err());
    }
    #[test]
    fn unsupported_materials_are_rejected_before_optional_payloads() {
        for flag in [
            0x8000u32, 0x80000, 0x01000000, 0x04000000, 0x20000000, 0x40000000, 0x80000000,
        ] {
            let mut b = mesh();
            b[16..20].copy_from_slice(&flag.to_le_bytes());
            let e = parse_four_ds(&b).unwrap_err();
            assert!(e.0.contains("unsupported alpha"), "flag {flag:08x}: {e}");
        }
        for (offset, value) in [(44, 0.5f32), (56, 0.5f32)] {
            let mut b = mesh();
            b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(
                parse_four_ds(&b)
                    .unwrap_err()
                    .0
                    .contains("unsupported alpha")
            );
        }
        let mut b = mesh();
        b[16..20].copy_from_slice(&0x30000u32.to_le_bytes());
        assert_eq!(parse_four_ds(&b).unwrap().materials[0].flags, 0x30000);
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;
    #[test]
    fn sparse_oversize_and_nonregular_inputs_are_rejected() {
        let path = std::env::temp_dir().join(format!("lhr-assets-quota-{}", std::process::id()));
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len(MAX_BYTES as u64 + 1).unwrap();
        let result = read_asset_file(&path);
        drop(file);
        fs::remove_file(&path).unwrap();
        assert_eq!(result.unwrap_err().0, "asset exceeds 64 MiB quota");
        assert_eq!(
            read_asset_file(std::env::temp_dir()).unwrap_err().0,
            "asset must be a regular file"
        );
        assert_eq!(read_asset_file(&path).unwrap_err().0, "asset unavailable");
    }
}
