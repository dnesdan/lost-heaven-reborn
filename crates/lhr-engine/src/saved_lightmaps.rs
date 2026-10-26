//! Saved I3D_lit_object data (format 0x21), bounded at the native Load boundary.
//! Source contract: pinned mafia-re 29b65b0 I3d_lit_object.cpp Load and
//! litLod::postProcess. This does not implement offline Construct or Save.
use super::lightmap_atlas::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedLightMeshLod {
    pub vertex_count: u32,
    pub face_group_faces: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedLitVertex {
    pub source_vertex: u16,
    pub rectangle: u16,
    /// Coordinates after native postProcess, including half-texel centers.
    pub lightmap_uv: [f32; 2],
}
#[derive(Clone, Debug)]
pub struct SavedLightmapLod {
    pub flags: u16,
    pub vertices: Vec<SavedLitVertex>,
    /// Native independent index buffer, in linked face-group order.
    pub indices: Vec<u16>,
    pub face_rectangles: Vec<Vec<u16>>,
    pub placements: Vec<LightmapPlacement>,
    pub atlas: LightmapAtlasOwner,
    pub memory_bytes: u32,
    // Declared after atlas: the last clone releases its strong texture
    // reference before this lease returns rectangles or removes the weak page.
    pub(crate) _allocation: LightmapAllocationLease,
}
impl PartialEq for SavedLightmapLod {
    fn eq(&self, other: &Self) -> bool {
        self.flags == other.flags
            && self.vertices == other.vertices
            && self.indices == other.indices
            && self.face_rectangles == other.face_rectangles
            && self.placements == other.placements
            && self.memory_bytes == other.memory_bytes
            && std::sync::Arc::ptr_eq(&self.atlas, &other.atlas)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum SavedLitLod {
    Lightmap(SavedLightmapLod),
    VertexColors { flags: u16, colors: Vec<u32> },
}
#[derive(Clone, Debug)]
pub struct SavedLitLevel {
    pub mode: u32,
    pub resolution: f32,
    pub angle: f32,
    /// False for an interrupted header's successfully loaded LOD prefix.
    pub constructed: bool,
    pub lods: [Option<SavedLitLod>; 10],
}
#[derive(Clone, Debug)]
pub struct SavedLightingLoad {
    pub levels: [Option<SavedLitLevel>; 6],
    pub bytes_read: usize,
}
impl Default for SavedLightingLoad {
    fn default() -> Self {
        Self {
            levels: std::array::from_fn(|_| None),
            bytes_read: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedLightingError {
    Truncated,
    /// Native either ignores this read result or accepts a nonzero partial
    /// read. The safe host refuses it without inventing a native HRESULT.
    UncheckedReadTruncated,
    MissingMesh,
    Version(u8),
    InvalidLevel(u8),
    LodCount,
    FaceGroupCount,
    FaceCount,
    VertexCount,
    VertexIndex,
    RectangleIndex,
    VertexRectangleIndex,
    InvalidFloat,
    InvalidRectangle,
    RgbSingleSize,
    InvalidMode(u8),
    Quota,
    HostAllocation,
    Atlas(AtlasError),
}
#[derive(Debug)]
pub struct SavedLightingFailure {
    pub error: SavedLightingError,
    /// Native Load retains completed LODs and earlier levels on a later error.
    /// The failing temporary LOD is discarded.
    pub partial: SavedLightingLoad,
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    allocated: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SavedLightingError> {
        let end = self.at.checked_add(n).ok_or(SavedLightingError::Quota)?;
        let Some(out) = self.bytes.get(self.at..end) else {
            self.at = self.bytes.len();
            return Err(SavedLightingError::Truncated);
        };
        self.at = end;
        Ok(out)
    }
    fn relaxed(&mut self, n: usize) -> Result<&'a [u8], SavedLightingError> {
        let remaining = self.bytes.len() - self.at;
        self.take(n).map_err(|e| {
            if e == SavedLightingError::Truncated && remaining != 0 {
                SavedLightingError::UncheckedReadTruncated
            } else {
                e
            }
        })
    }
    fn u8(&mut self) -> Result<u8, SavedLightingError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, SavedLightingError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, SavedLightingError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn count(&mut self, max: usize) -> Result<usize, SavedLightingError> {
        let n = self.u32()? as usize;
        if n > max {
            Err(SavedLightingError::Quota)
        } else {
            Ok(n)
        }
    }
    fn float(&mut self) -> Result<f32, SavedLightingError> {
        let v = f32::from_bits(self.u32()?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(SavedLightingError::InvalidFloat)
        }
    }
    fn vec<T>(&mut self, n: usize) -> Result<Vec<T>, SavedLightingError> {
        self.allocated = self
            .allocated
            .checked_add(
                n.checked_mul(std::mem::size_of::<T>())
                    .ok_or(SavedLightingError::Quota)?,
            )
            .ok_or(SavedLightingError::Quota)?;
        if self.allocated > 256 * 1024 * 1024 {
            return Err(SavedLightingError::Quota);
        }
        let mut v = Vec::new();
        v.try_reserve_exact(n)
            .map_err(|_| SavedLightingError::HostAllocation)?;
        Ok(v)
    }
}

fn read_lod(
    r: &mut Reader<'_>,
    mode: u8,
    mesh: &SavedLightMeshLod,
    pool: &mut LightmapAtlasPool,
) -> Result<SavedLitLod, SavedLightingError> {
    let flags = r.u16()?;
    if mode & 2 == 0 {
        let n = if mode & 4 != 0 {
            r.u32().map(|n| n as usize).map_err(|e| {
                if e == SavedLightingError::Truncated {
                    SavedLightingError::UncheckedReadTruncated
                } else {
                    e
                }
            })?
        } else {
            mesh.vertex_count as usize
        };
        if n != mesh.vertex_count as usize {
            return Err(SavedLightingError::VertexCount);
        }
        let mut colors = r.vec(n)?;
        for _ in 0..n {
            colors.push(r.u32()?)
        }
        return Ok(SavedLitLod::VertexColors { flags, colors });
    }
    let nr = r.u16()? as usize;
    let nf = r.u16()? as usize;
    if nf != mesh.face_group_faces.len() {
        return Err(SavedLightingError::FaceGroupCount);
    }
    if nr == 0 {
        return Err(SavedLightingError::InvalidRectangle);
    }
    let mut rectangles = r.vec(nr)?;
    for _ in 0..nr {
        let single = r.u8()? != 0;
        let color = if single { Some(r.u32()?) } else { None };
        let maps = r.count(32)?;
        if !single && maps == 0 {
            return Err(SavedLightingError::InvalidRectangle);
        }
        let mut first = None;
        for m in 0..maps {
            let sx = r.count(32767)?;
            let sy = r.count(32767)?;
            if sx == 0 || sy == 0 {
                return Err(SavedLightingError::InvalidRectangle);
            }
            // Native single-color records still carry map dimensions but no
            // pixel bytes. Additional RGB maps are consumed and discarded.
            if !single {
                let n = sx
                    .checked_mul(sy)
                    .and_then(|n| n.checked_mul(3))
                    .filter(|&n| n <= 64 * 1024 * 1024)
                    .ok_or(SavedLightingError::Quota)?;
                let bytes = r.take(n)?;
                if m == 0 {
                    if sx == 3 && sy == 3 {
                        return Err(SavedLightingError::RgbSingleSize);
                    }
                    if sx < 3 || sy < 3 {
                        return Err(SavedLightingError::InvalidRectangle);
                    }
                    let mut owned = r.vec(n)?;
                    owned.extend_from_slice(bytes);
                    first = Some(LightmapRectangle {
                        width: sx as u16,
                        height: sy as u16,
                        texels: LightmapTexels::Rgb(owned),
                    });
                }
            }
        }
        rectangles.push(if let Some(c) = color {
            LightmapRectangle {
                width: 3,
                height: 3,
                texels: LightmapTexels::Single(c),
            }
        } else {
            first.ok_or(SavedLightingError::InvalidRectangle)?
        });
    }
    let n = u32::from_le_bytes(r.relaxed(4)?.try_into().unwrap()) as usize;
    if n > 65536 {
        return Err(SavedLightingError::Quota);
    }
    if n == 0 {
        return Err(SavedLightingError::VertexCount);
    }
    let mut uv = r.vec(n)?;
    for bytes in r.relaxed(n * 8)?.as_chunks::<8>().0 {
        let pair = [
            f32::from_le_bytes(bytes[..4].try_into().unwrap()),
            f32::from_le_bytes(bytes[4..].try_into().unwrap()),
        ];
        if !pair.into_iter().all(f32::is_finite) {
            return Err(SavedLightingError::InvalidFloat);
        }
        uv.push(pair);
    }
    let mut vertices = r.vec(n)?;
    for (lightmap_uv, bytes) in uv.into_iter().zip(r.relaxed(n * 4)?.as_chunks::<4>().0) {
        let source_vertex = u16::from_le_bytes(bytes[..2].try_into().unwrap());
        let rectangle = u16::from_le_bytes(bytes[2..].try_into().unwrap());
        if u32::from(source_vertex) >= mesh.vertex_count {
            return Err(SavedLightingError::VertexIndex);
        }
        vertices.push(SavedLitVertex {
            source_vertex,
            rectangle,
            lightmap_uv,
        });
    }
    let expected = mesh
        .face_group_faces
        .iter()
        .try_fold(0usize, |a, &b| a.checked_add(b as usize))
        .and_then(|n| n.checked_mul(3))
        .ok_or(SavedLightingError::Quota)?;
    let ni = u32::from_le_bytes(r.relaxed(4)?.try_into().unwrap()) as usize;
    if ni > 3 * 4 * 1024 * 1024 {
        return Err(SavedLightingError::Quota);
    }
    if ni != expected {
        return Err(SavedLightingError::FaceCount);
    }
    let mut indices = r.vec(ni)?;
    for bytes in r.relaxed(ni * 2)?.as_chunks::<2>().0 {
        let i = u16::from_le_bytes(*bytes);
        if i as usize >= n {
            return Err(SavedLightingError::VertexIndex);
        }
        indices.push(i);
    }
    let mut face_rectangles = r.vec(nf)?;
    for &expected in &mesh.face_group_faces {
        let count = r.count(4 * 1024 * 1024)?;
        if count != expected as usize {
            return Err(SavedLightingError::FaceCount);
        }
        let mut rects = r.vec(count)?;
        for _ in 0..count {
            let i = r.u16()?;
            if i as usize >= nr {
                return Err(SavedLightingError::RectangleIndex);
            }
            rects.push(i);
        }
        face_rectangles.push(rects);
    }
    let memory_bytes = rectangles
        .iter()
        .try_fold(0u32, |a, x| {
            a.checked_add(u32::from(x.width) * u32::from(x.height) * 4)
        })
        .ok_or(SavedLightingError::Quota)?;
    let (atlas, placements, allocation) =
        pool.pack(&rectangles).map_err(SavedLightingError::Atlas)?;
    // Truecolor memory accounting follows the final shrunk rectangle area.
    // Keep the original extent check above before atlas construction.
    let memory_bytes = if placements
        .iter()
        .zip(&rectangles)
        .any(|(p, r)| p.width != r.width || p.height != r.height)
    {
        placements
            .iter()
            .try_fold(0u32, |n, p| {
                n.checked_add(u32::from(p.width) * u32::from(p.height) * 4)
            })
            .ok_or(SavedLightingError::Quota)?
    } else {
        memory_bytes
    };
    let mut lod = SavedLightmapLod {
        flags,
        vertices,
        indices,
        face_rectangles,
        placements,
        atlas,
        memory_bytes,
        _allocation: allocation,
    };
    let (su, sv) = {
        let a = lod
            .atlas
            .read()
            .map_err(|_| SavedLightingError::Atlas(AtlasError::Poisoned))?;
        (1. / a.width as f32, 1. / a.height as f32)
    };
    for v in &mut lod.vertices {
        if v.rectangle as usize >= nr {
            return Err(SavedLightingError::VertexRectangleIndex);
        }
        let p = lod.placements[v.rectangle as usize];
        let [u, w] = v.lightmap_uv;
        // Keep the source arithmetic order (offsets are normalized before the
        // half texel is added). Rotation is the source's u >= 2 marker.
        v.lightmap_uv = if !p.rotated {
            [
                (f32::from(p.width) - 1.) * u * su + f32::from(p.x) * su + su * 0.5,
                (f32::from(p.height) - 1.) * w * sv + f32::from(p.y) * sv + sv * 0.5,
            ]
        } else {
            [
                (f32::from(p.height) - 1.) * w * su + f32::from(p.x) * su + su * 0.5,
                (f32::from(p.width) - 1.) * u * sv + f32::from(p.y) * sv + sv * 0.5,
            ]
        };
        if !v.lightmap_uv.into_iter().all(f32::is_finite) {
            return Err(SavedLightingError::InvalidFloat);
        }
    }
    Ok(SavedLitLod::Lightmap(lod))
}

/// Reads exactly one native Load payload, leaving enclosing chunk bytes to the
/// caller. Unlike native unchecked reads, every size and index is bounded.
/// On malformed level zero, returns an error instead of indexing before six
/// constructor records. Values above six retain native remapping to level one.
pub fn read_saved_lighting(
    bytes: &[u8],
    mesh: &[SavedLightMeshLod],
    pool: &mut LightmapAtlasPool,
) -> Result<SavedLightingLoad, Box<SavedLightingFailure>> {
    let mut out = SavedLightingLoad::default();
    let mut r = Reader {
        bytes,
        at: 0,
        allocated: 0,
    };
    let result = (|| {
        if mesh.len() > 10
            || mesh
                .iter()
                .any(|m| m.vertex_count > 65536 || m.face_group_faces.len() > 65535)
        {
            return Err(SavedLightingError::Quota);
        }
        let count = r
            .u8()
            .map_err(|e| {
                if e == SavedLightingError::Truncated {
                    SavedLightingError::UncheckedReadTruncated
                } else {
                    e
                }
            })?
            .count_ones();
        for _ in 0..count {
            let version = r.u8()?;
            let mode = r.u8()?;
            // The complete native header is read before count comparison.
            // No allocation/loop uses this unchecked file count.
            let lod_count = r.u32()? as usize;
            let resolution = r.float()?;
            let angle = r.float()?;
            let mut level = r.u8()?;
            if level > 6 {
                level = 1;
            }
            if level == 0 {
                return Err(SavedLightingError::InvalidLevel(level));
            }
            let at = (level - 1) as usize;
            // Metadata writes precede native version/count validation.
            let lev = out.levels[at].get_or_insert_with(|| SavedLitLevel {
                mode: mode.into(),
                resolution,
                angle,
                constructed: false,
                lods: std::array::from_fn(|_| None),
            });
            lev.mode = mode.into();
            lev.resolution = resolution;
            lev.angle = angle;
            if version != 0x21 {
                return Err(SavedLightingError::Version(version));
            }
            if lod_count != mesh.len() {
                return Err(SavedLightingError::LodCount);
            }
            if mode & 3 != 1 && mode & 3 != 2 {
                return Err(SavedLightingError::InvalidMode(mode));
            }
            for (i, m) in mesh.iter().enumerate() {
                lev.lods[i] = Some(read_lod(&mut r, mode, m, pool)?);
            }
            lev.constructed = true;
        }
        Ok(())
    })();
    out.bytes_read = r.at;
    match result {
        Ok(()) => Ok(out),
        Err(error) => Err(Box::new(SavedLightingFailure {
            error,
            partial: out,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mesh() -> Vec<SavedLightMeshLod> {
        vec![SavedLightMeshLod {
            vertex_count: 3,
            face_group_faces: vec![1],
        }]
    }
    fn header(bytes: &mut Vec<u8>, mode: u8, level: u8) {
        bytes.extend([0x21, mode]);
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(5f32.to_le_bytes());
        bytes.extend(45f32.to_le_bytes());
        bytes.push(level);
    }
    fn vertex_level(bytes: &mut Vec<u8>, level: u8) {
        header(bytes, 5, level);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(3u32.to_le_bytes());
        for color in [0xff102030u32, 0xff405060, 0xff708090] {
            bytes.extend(color.to_le_bytes());
        }
    }
    fn lightmap() -> Vec<u8> {
        let mut bytes = vec![1];
        header(&mut bytes, 2, 1);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.push(1);
        bytes.extend(0xff204080u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(3u32.to_le_bytes());
        for [u, v] in [[0f32, 0f32], [1., 0.], [0., 1.]] {
            bytes.extend(u.to_le_bytes());
            bytes.extend(v.to_le_bytes());
        }
        for vertex in 0u16..3 {
            bytes.extend(vertex.to_le_bytes());
            bytes.extend(0u16.to_le_bytes());
        }
        bytes.extend(3u32.to_le_bytes());
        for i in 0u16..3 {
            bytes.extend(i.to_le_bytes());
        }
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        bytes
    }
    #[test]
    fn load_owns_saved_indices_and_maps_real_atlas_texel_centers() {
        let bytes = lightmap();
        let mut pool = LightmapAtlasPool::default();
        let parsed = read_saved_lighting(&bytes, &mesh(), &mut pool).unwrap();
        assert_eq!(parsed.bytes_read, bytes.len());
        let SavedLitLod::Lightmap(lod) =
            parsed.levels[0].as_ref().unwrap().lods[0].as_ref().unwrap()
        else {
            panic!()
        };
        assert_eq!(lod.indices, [0, 1, 2]);
        assert_eq!(lod.face_rectangles, [vec![0]]);
        assert_eq!(lod.vertices[0].lightmap_uv, [0.5 / 128., 0.5 / 128.]);
        let atlas = lod.atlas.read().unwrap();
        assert_eq!(&atlas.rgba[0..4], &[0x20, 0x40, 0x80, 255]);
        assert_eq!(lod.vertices[1].lightmap_uv, [2.5 / 128., 0.5 / 128.]);
    }
    fn rgb_lightmap(width: u32, height: u32) -> Vec<u8> {
        let single = lightmap();
        let mut bytes = single[..22].to_vec();
        bytes.push(0);
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(width.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        bytes.extend([32, 64, 96].repeat((width * height) as usize));
        bytes.extend_from_slice(&single[39..]);
        bytes
    }
    #[test]
    fn resized_saved_lod_accounts_final_area_and_postprocess_uses_final_texel_centers() {
        let mut bytes = rgb_lightmap(1536, 8);
        let cursor = bytes.len();
        bytes.extend([9, 8, 7]);
        let parsed =
            read_saved_lighting(&bytes, &mesh(), &mut LightmapAtlasPool::default()).unwrap();
        assert_eq!(parsed.bytes_read, cursor);
        let SavedLitLod::Lightmap(lod) =
            parsed.levels[0].as_ref().unwrap().lods[0].as_ref().unwrap()
        else {
            panic!()
        };
        assert_eq!(lod.memory_bytes, 384 * 4 * 4);
        assert_eq!(
            [lod.placements[0].width, lod.placements[0].height],
            [384, 4]
        );
        assert_eq!(lod.vertices[0].lightmap_uv, [0.5 / 512., 0.5 / 128.]);
        assert_eq!(lod.vertices[1].lightmap_uv, [383.5 / 512., 0.5 / 128.]);
        assert_eq!(lod.vertices[2].lightmap_uv, [0.5 / 512., 3.5 / 128.]);
        assert_eq!(lod.indices, [0, 1, 2]);
    }
    #[test]
    fn rgb_three_by_n_saved_payload_remains_admitted() {
        let bytes = rgb_lightmap(3, 127);
        let parsed =
            read_saved_lighting(&bytes, &mesh(), &mut LightmapAtlasPool::default()).unwrap();
        assert_eq!(parsed.bytes_read, bytes.len());
        let SavedLitLod::Lightmap(lod) =
            parsed.levels[0].as_ref().unwrap().lods[0].as_ref().unwrap()
        else {
            panic!()
        };
        assert_eq!(lod.memory_bytes, 3 * 127 * 4);
        assert_eq!(
            [lod.placements[0].width, lod.placements[0].height],
            [3, 127]
        );
    }
    #[test]
    fn partial_load_preserves_previous_lighting_and_rejects_each_truncated_prefix() {
        let bytes = lightmap();
        for n in 0..bytes.len() {
            let failure =
                read_saved_lighting(&bytes[..n], &mesh(), &mut LightmapAtlasPool::default())
                    .unwrap_err();
            assert!(failure
                .partial
                .levels
                .iter()
                .flatten()
                .all(|l| !l.constructed));
        }
        let mut bytes = vec![3];
        vertex_level(&mut bytes, 1);
        header(&mut bytes, 2, 2);
        let failure =
            read_saved_lighting(&bytes, &mesh(), &mut LightmapAtlasPool::default()).unwrap_err();
        assert!(failure.partial.levels[0].as_ref().unwrap().constructed);
        assert!(!failure.partial.levels[1].as_ref().unwrap().constructed);
        assert_eq!(failure.error, SavedLightingError::Truncated);
    }
    #[test]
    fn malformed_level_and_mesh_counts_fail_without_generic_geometry_fallback() {
        let mut bytes = vec![1];
        vertex_level(&mut bytes, 0);
        assert_eq!(
            read_saved_lighting(&bytes, &mesh(), &mut LightmapAtlasPool::default())
                .unwrap_err()
                .error,
            SavedLightingError::InvalidLevel(0)
        );
        let mut bytes = lightmap();
        bytes[20] = 2;
        assert_eq!(
            read_saved_lighting(&bytes, &mesh(), &mut LightmapAtlasPool::default())
                .unwrap_err()
                .error,
            SavedLightingError::FaceGroupCount
        );
    }
}
