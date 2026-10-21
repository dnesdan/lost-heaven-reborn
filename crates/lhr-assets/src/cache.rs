//! Bounded city cache v1 placements, following pinned MIT workbench cache.js.
//! Edge vectors and the second stored scale are preserved without inventing semantics.
use super::{MAX_BYTES, Reader, Result, decode_windows1250, err};
#[derive(Debug, Clone)]
pub struct CacheObject {
    pub model: String,
    pub position: [f32; 3],
    pub rotation_wxyz: [f32; 4],
    pub scale: [f32; 3],
    pub flags: u32,
    pub secondary_scale: [f32; 3],
    pub trailing_bytes: usize,
}
#[derive(Debug, Clone)]
pub struct CachePart {
    pub name: String,
    pub sphere_position: [f32; 2],
    pub sphere_radius: f32,
    /// Source sequence: leftBottom,leftSide,rightBottom,rightSide,leftTop,
    /// bottomSide,rightTop,topSide, all XZ pairs, including raw edge vectors.
    pub bounds_and_edges: [[f32; 2]; 8],
    pub objects: Vec<CacheObject>,
}
#[derive(Debug, Clone, Default)]
pub struct CityCache {
    pub parts: Vec<CachePart>,
    pub unknown_chunks: usize,
}
fn string(r: &mut Reader<'_>) -> Result<String> {
    let n = r.u32()? as usize;
    if n == 0 || n > 4096 {
        return err("cache name quota exceeded");
    }
    let bytes = r.take(n)?;
    if bytes.contains(&0) {
        return err("NUL in cache name");
    }
    decode_windows1250(bytes)
}
fn chunk<'a>(r: &mut Reader<'a>, count: &mut usize) -> Result<(u16, Reader<'a>)> {
    *count += 1;
    if *count > 100_000 {
        return err("cache chunk quota exceeded");
    }
    let kind = r.u16()?;
    let size = r.u32()? as usize;
    if size < 6 {
        return err("cache chunk smaller than header");
    }
    Ok((
        kind,
        Reader {
            b: r.take(size - 6)?,
            p: 0,
        },
    ))
}
/// Source transforms are world coordinates, not relative to each cache part.
/// Unknown chunks remain counted; flags and secondary scale are not interpreted.
pub fn parse_city_cache(bytes: &[u8]) -> Result<CityCache> {
    if bytes.len() > MAX_BYTES {
        return err("cache exceeds 64 MiB quota");
    }
    let mut source = Reader { b: bytes, p: 0 };
    let mut count = 0;
    let (kind, mut root) = chunk(&mut source, &mut count)?;
    if kind != 500 || source.p != bytes.len() || root.u32()? != 1 {
        return err("expected city cache version 1");
    }
    let mut result = CityCache::default();
    let mut objects = 0;
    while root.p < root.b.len() {
        let (kind, mut part) = chunk(&mut root, &mut count)?;
        if kind != 1000 {
            result.unknown_chunks += 1;
            continue;
        }
        if result.parts.len() >= 4096 {
            return err("cache part quota exceeded");
        }
        let name = string(&mut part)?;
        let sphere_position = part.vec()?;
        let sphere_radius = part.f()?;
        let mut bounds_and_edges = [[0.; 2]; 8];
        for pair in &mut bounds_and_edges {
            *pair = part.vec()?;
        }
        let mut frames = Vec::new();
        while part.p < part.b.len() {
            let (kind, mut frame) = chunk(&mut part, &mut count)?;
            if kind != 2000 {
                result.unknown_chunks += 1;
                continue;
            }
            objects += 1;
            if objects > 65535 {
                return err("cache object quota exceeded");
            }
            let model = string(&mut frame)?;
            let position = frame.vec()?;
            let rotation_wxyz: [f32; 4] = frame.vec()?;
            if rotation_wxyz
                .iter()
                .map(|n| (*n as f64).powi(2))
                .sum::<f64>()
                < 1e-20
            {
                return err("zero cache quaternion");
            }
            let scale = frame.vec()?;
            let flags = frame.u32()?;
            let secondary_scale = frame.vec()?;
            let trailing_bytes = frame.b.len() - frame.p;
            frames.push(CacheObject {
                model,
                position,
                rotation_wxyz,
                scale,
                flags,
                secondary_scale,
                trailing_bytes,
            });
        }
        result.parts.push(CachePart {
            name,
            sphere_position,
            sphere_radius,
            bounds_and_edges,
            objects: frames,
        });
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(k: u16, b: &[u8]) -> Vec<u8> {
        [
            k.to_le_bytes().as_slice(),
            &((b.len() + 6) as u32).to_le_bytes(),
            b,
        ]
        .concat()
    }
    fn text(s: &str) -> Vec<u8> {
        [(s.len() as u32).to_le_bytes().as_slice(), s.as_bytes()].concat()
    }
    fn fixture() -> Vec<u8> {
        let mut object = text("synthetic.i3d");
        for n in [1f32, 2., 3., 1., 0., 0., 0., 1., 2., 1.] {
            object.extend(n.to_le_bytes());
        }
        object.extend(7u32.to_le_bytes());
        for n in [2f32, 3., 4.] {
            object.extend(n.to_le_bytes());
        }
        object.extend([9, 8]);
        let mut part = text("test district");
        part.extend([0; 19 * 4]);
        part.extend(chunk(2000, &object));
        part.extend(chunk(0xeeee, &[1, 2]));
        let mut root = 1u32.to_le_bytes().to_vec();
        root.extend(chunk(1000, &part));
        chunk(500, &root)
    }
    #[test]
    fn placement_flags_secondary_scale_and_truncations() {
        let b = fixture();
        let c = parse_city_cache(&b).unwrap();
        let o = &c.parts[0].objects[0];
        assert_eq!(o.position, [1., 2., 3.]);
        assert_eq!(o.scale, [1., 2., 1.]);
        assert_eq!(o.secondary_scale, [2., 3., 4.]);
        assert_eq!(o.flags, 7);
        assert_eq!(o.trailing_bytes, 2);
        assert_eq!(c.unknown_chunks, 1);
        for n in 0..b.len() {
            assert!(parse_city_cache(&b[..n]).is_err(), "{n}");
        }
    }
    #[test]
    fn invalid_root_and_extents_rejected() {
        let mut b = fixture();
        b[6] = 2;
        assert!(parse_city_cache(&b).is_err());
        let mut b = fixture();
        b[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_city_cache(&b).is_err());
        assert!(parse_city_cache(&[]).is_err());
    }
}
