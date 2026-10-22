//! Bounded read-only static 4DS v29 and BMP intake. Source coordinates and order are preserved.
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};
pub mod animation;
mod cache;
pub use cache::{CacheObject, CachePart, CityCache, parse_city_cache};
mod dta;
mod menu;
mod scene;
pub mod script;
pub mod script_vm;
mod tga;
pub use menu::{
    FontGlyph, FontMetrics, LanguageFontMap, MenuDefinition, MenuItem, TextDb, TextDbEntry,
    parse_font_metrics, parse_language_font_map, parse_menu_definition, parse_text_db,
};
pub use scene::{Scene, SceneActor, SceneObject, SceneScript, parse_scene2};
pub use tga::decode_tga;

const MAX_BYTES: usize = 64 * 1024 * 1024;
#[derive(Debug, Clone)]
pub struct Error(pub String);
const UNSUPPORTED_BILLBOARD: &str =
    "unsupported visual type 4; only standard static mesh supported";
impl Error {
    /// True only after a complete model passed supported structural validation,
    /// including the billboard mesh and its five opaque axis/mode bytes. Other
    /// unsupported visuals, malformed payloads and truncated files return false.
    pub fn is_unsupported_billboard(&self) -> bool {
        self.0 == UNSUPPORTED_BILLBOARD
    }
}
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
    /// Zero-based material table index, or UNASSIGNED_MATERIAL for scene groups
    /// whose source reference is zero. The latter must not index the table.
    pub material: usize,
    pub indices: Vec<u32>,
}
pub const UNASSIGNED_MATERIAL: usize = usize::MAX;
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
    pub emission: [f32; 3],
    pub opacity: f32,
    pub alpha_texture: Option<String>,
    pub reflection: Option<f32>,
    pub environment_texture: Option<String>,
    pub animation: Option<TextureAnimation>,
}
#[derive(Debug, Clone)]
pub struct TextureAnimation {
    pub frames: u32,
    pub unknown: u16,
    pub delay: u32,
    pub unknown2: u32,
    pub unknown3: u32,
}
#[derive(Debug, Clone)]
pub struct JointRecord {
    /// Original 64-byte matrix in source order. Every float bit is retained,
    /// including nonfinite values; this metadata does not validate skinning.
    pub bone_matrix: [f32; 16],
    /// Original joint index, without a binding-table range assumption.
    pub index: u32,
}
#[derive(Debug, Clone)]
pub struct Frame {
    pub kind: u8,
    pub visual: Option<u8>,
    pub render_flags: u8,
    pub logic_flags: u8,
    pub culling: u8,
    pub name: String,
    /// Original length-prefixed bytes, separate from the display encoding.
    pub name_bytes: Vec<u8>,
    /// Opaque common-frame properties; preserve bytes without text conversion.
    pub properties: Vec<u8>,
    /// Original dummy bounding box. Retained as data, not a collision shape.
    pub dummy_bounds: Option<[[f32; 3]; 2]>,
    /// Original kind-10 joint suffix, retained only by broader scene intake.
    pub joint: Option<JointRecord>,
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
    parse_four_ds_impl(b, false, false).map(|(model, _)| model)
}
/// Broader scene intake: retains material effects as metadata and accepts sector,
/// light, target and joint frames. Joint suffixes are raw metadata, without
/// skinning or joint-runtime behavior. Callers must implement or report material
/// effects; this does not promise identical original rendering or simulation.
pub fn parse_four_ds_scene(b: &[u8]) -> Result<Model> {
    parse_four_ds_impl(b, true, false).map(|(model, _)| model)
}
/// Validated original single-root position-animation intake. The original
/// LoadAnimCount remains attached; this type must not enter a static-only path.
#[derive(Debug, Clone)]
pub struct PositionAnimatedModel {
    model: Model,
    load_animation_count: u8,
    animation: animation::AnimationSet,
}
impl PositionAnimatedModel {
    pub fn model(&self) -> &Model {
        &self.model
    }
    pub fn load_animation_count(&self) -> u8 {
        self.load_animation_count
    }
    pub fn animation(&self) -> &animation::AnimationSet {
        &self.animation
    }
    /// Moves geometry and animation together with the original count metadata.
    pub fn into_parts(self) -> (Model, u8, animation::AnimationSet) {
        (self.model, self.load_animation_count, self.animation)
    }
}

/// Separately admits original 4DS LoadAnimCount1 with a complete, matching 5DS
/// position-only set. Static intake keeps rejecting all nonzero counts. This
/// validates source compatibility, not model finalization or rendered playback.
pub fn parse_four_ds_position_animation(
    model_bytes: &[u8],
    animation_bytes: &[u8],
) -> Result<PositionAnimatedModel> {
    let (model, load_animation_count) = parse_four_ds_impl(model_bytes, false, true)?;
    if load_animation_count != 1 || model.frames.len() != 1 {
        return err("position animation requires original count1 and one root frame");
    }
    let frame = &model.frames[0];
    if frame.kind != 1
        || frame.visual != Some(0)
        || frame.parent.is_some()
        || frame.instance.is_some()
        || frame.lods.len() != 1
    {
        return err("position animation requires one root standard mesh and one LOD");
    }
    let animation = animation::parse_5ds(animation_bytes)?;
    if animation.tracks.len() != 1 || animation.duration_ms() == 0 {
        return err("position animation requires one nonzero-duration track");
    }
    let track = &animation.tracks[0];
    if !track.name.is_ascii() || track.name != frame.name.as_bytes() {
        return err("position animation frame name mismatch");
    }
    let positions = track.position_only()?;
    if positions
        .key_times()
        .last()
        .is_some_and(|&last| last > animation.end_frame)
    {
        return err("position animation key exceeds duration");
    }
    Ok(PositionAnimatedModel {
        model,
        load_animation_count,
        animation,
    })
}

fn parse_four_ds_impl(b: &[u8], scene: bool, allow_animation: bool) -> Result<(Model, u8)> {
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
        if !scene
            && (flags
                & (0x8000
                    | 0x80000
                    | 0x01000000
                    | 0x04000000
                    | 0x20000000
                    | 0x40000000
                    | 0x80000000)
                != 0
                || opacity != 1.
                || emission != [0.; 3])
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
        if !(0. ..=1.).contains(&opacity) {
            return err("material opacity outside 0..1");
        }
        let reflection = if flags & 0x80000 != 0 {
            Some(r.f()?)
        } else {
            None
        };
        let environment_texture = if reflection.is_some() {
            Some(texture_name(&mut r)?)
        } else {
            None
        };
        let texture = texture_name(&mut r)?;
        let alpha_texture =
            if !texture.is_empty() && flags & 0x40000000 != 0 && flags & 0xa1000000 == 0 {
                Some(texture_name(&mut r)?)
            } else {
                None
            };
        let animation = if flags & 0x04000000 != 0 {
            Some(TextureAnimation {
                frames: r.u32()?,
                unknown: r.u16()?,
                delay: r.u32()?,
                unknown2: r.u32()?,
                unknown3: r.u32()?,
            })
        } else {
            None
        };
        materials.push(Material {
            flags,
            ambient,
            diffuse,
            texture,
            emission,
            opacity,
            reflection,
            environment_texture,
            alpha_texture,
            animation,
        });
    }
    let count = r.u16()? as usize;
    if count > 16384 {
        return err("frame quota exceeded");
    };
    let mut frames = Vec::new();
    let mut total_vertices = 0usize;
    let mut total_indices = 0usize;
    let mut unsupported_billboard = false;
    for _ in 0..count {
        let kind = r.u8()?;
        if kind != 1 && kind != 6 && !(scene && [2, 5, 7, 10, 12].contains(&kind)) {
            return err("unsupported frame type; only static visual and dummy supported");
        };
        let visual = if kind == 1 { Some(r.u8()?) } else { None };
        let mut render_flags = 0;
        let mut logic_flags = 0;
        if let Some(visual) = visual {
            if visual > 1 && visual != 4 {
                return Err(Error(format!(
                    "unsupported visual type {visual}; only standard static mesh supported"
                )));
            };
            unsupported_billboard |= visual == 4;
            render_flags = r.u8()?;
            logic_flags = r.u8()?;
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
        let culling = r.u8()?;
        let name_start = r.p;
        let name = if scene {
            source_string(&mut r)?
        } else {
            r.string()?
        };
        let name_bytes = r.b[name_start + 1..r.p].to_vec();
        let property_length = r.u8()? as usize;
        let properties = r.take(property_length)?.to_vec();
        let mut dummy_bounds = None;
        let mut joint = None;
        let mut lods = Vec::new();
        let mut instance = None;
        if kind == 6 {
            let bytes = r.take(24)?;
            dummy_bounds = Some(std::array::from_fn(|row| {
                std::array::from_fn(|column| {
                    let offset = (row * 3 + column) * 4;
                    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
                })
            }));
        } else if kind == 10 {
            // The PC suffix is one fixed 68-byte record. Keep its words raw;
            // numeric and binding validation belong to consumers of the joint.
            let bytes = r.take(68)?;
            joint = Some(JointRecord {
                bone_matrix: std::array::from_fn(|i| {
                    f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
                }),
                index: u32::from_le_bytes(bytes[64..68].try_into().unwrap()),
            });
        } else if kind == 2 {
            r.take(84)?;
        } else if kind == 5 || kind == 12 {
            if kind == 5 {
                r.take(8)?;
            }
            let vertices = r.u32()? as usize;
            let faces = r.u32()? as usize;
            if vertices > 1_000_000 || faces > 1_000_000 {
                return err("sector/occluder quota exceeded");
            }
            r.take(vertices * 12 + faces * 6)?;
            if kind == 5 {
                r.take(24)?;
                let portals = r.u8()?;
                for _ in 0..portals {
                    let points = r.u8()? as usize;
                    r.take(28 + points * 12)?;
                }
            }
        } else if kind == 7 {
            r.take(2)?;
            let n = r.u8()? as usize;
            r.take(n * 2)?;
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
                        if (!scene && mat == 0) || mat > materials.len() {
                            return err("material reference outside table");
                        };
                        faces.push(FaceGroup {
                            material: mat.checked_sub(1).unwrap_or(UNASSIGNED_MATERIAL),
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
        // A billboard shares the standard mesh/instance layout, then always
        // stores five additional axis/mode bytes, including for mesh instances.
        // Consume and validate extents, but never return it as a renderable model.
        if visual == Some(4) {
            r.take(5)?;
        }
        frames.push(Frame {
            kind,
            visual,
            render_flags,
            logic_flags,
            culling,
            name,
            name_bytes,
            properties,
            dummy_bounds,
            joint,
            parent: parent.checked_sub(1),
            position,
            scale,
            rotation_wxyz,
            instance,
            lods,
        });
    }
    let load_animation_count = r.u8()?;
    if load_animation_count != 0 && !allow_animation {
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
    if unsupported_billboard {
        return err(UNSUPPORTED_BILLBOARD);
    }
    Ok((Model { materials, frames }, load_animation_count))
}
fn texture_name(r: &mut Reader<'_>) -> Result<String> {
    let name = r.string()?;
    if !name.is_empty() {
        safe_relative(&name)?;
        if !name.to_ascii_lowercase().ends_with(".bmp") {
            return err("only BMP textures supported");
        }
    }
    Ok(name)
}
fn source_string(r: &mut Reader<'_>) -> Result<String> {
    let n = r.u8()? as usize;
    let bytes = r.take(n)?;
    if bytes.contains(&0) {
        return err("NUL in source name");
    }
    decode_windows1250(bytes)
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
/// Original installation resources follow the evidenced DTA-first force-registration
/// order, then fall back to a loose file only when no archive contains the path.
/// A failed read of the selected DTA copy never falls back to loose or an older copy.
/// The installation directory tree must be trusted and stable during use. Handle and
/// metadata checks detect ordinary replacement/mutation; they do not confine hostile
/// concurrent replacement of parent directories (which requires descriptor-relative IO).
pub struct Installation {
    root: PathBuf,
    archives: Vec<std::sync::Mutex<dta::Archive>>,
    archive_index: std::collections::BTreeMap<String, usize>,
}
impl Installation {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = fs::canonicalize(root)
            .map_err(|_| Error("installation directory unavailable".into()))?;
        if !root.is_dir() {
            return err("installation root must be directory");
        }
        let mut discovered = std::collections::BTreeMap::new();
        for path in bounded_children(&root)? {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let key = match name.to_ascii_uppercase().as_str() {
                "A1.DTA" => Some((0x3d98766c, 0xde7009cd)),
                "A2.DTA" => Some((0x82a1c97b, 0x2d5085d4)),
                "A6.DTA" => Some((0x64cd8d0a, 0x4bc97b2d)),
                "A8.DTA" => Some((0xd8dd8fac, 0x5324ace5)),
                "AA.DTA" => Some((0x5342760c, 0xedeac652)),
                _ => None,
            };
            if let Some(keys) = key
                && discovered
                    .insert(name.to_ascii_uppercase(), (path, keys))
                    .is_some()
            {
                return err("ambiguous duplicate archive filename");
            }
        }
        let mut discovered: Vec<_> = discovered.into_iter().collect();
        const ORDER: [&str; 5] = ["A8.DTA", "A2.DTA", "A6.DTA", "A1.DTA", "AA.DTA"];
        discovered.sort_by_key(|(name, _)| {
            ORDER
                .iter()
                .position(|candidate| *candidate == name)
                .unwrap_or(ORDER.len())
        });
        let mut archives = Vec::new();
        let mut archive_index = std::collections::BTreeMap::new();
        for (_, (path, (k1, k2))) in discovered {
            verify_contained(&root, &path)?;
            let archive = dta::Archive::open(&path, k1, k2).map_err(Error)?;
            verify_contained(&root, &path)?;
            for entry in archive.entries() {
                let key = dta::canonical(&entry.path).map_err(Error)?;
                // dtaSetDtaFirstForce uses the first registered copy. The native
                // registration order puts A8 ahead of A2/A6/A1/AA for known archives.
                archive_index.entry(key).or_insert(archives.len());
            }
            archives.push(std::sync::Mutex::new(archive));
        }
        Ok(Self {
            root,
            archives,
            archive_index,
        })
    }
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        safe_relative(name)?;
        let canonical = dta::canonical(name).map_err(Error)?;
        if let Some(index) = self.archive_index.get(&canonical) {
            return self.archives[*index]
                .lock()
                .map_err(|_| Error("archive reader unavailable".into()))?
                .read(&canonical, MAX_BYTES)
                .map_err(Error);
        }
        let mut path = self.root.clone();
        let parts: Vec<_> = canonical.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            let mut matching = None;
            for child in bounded_children(&path)? {
                let Some(value) = child.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if value.to_lowercase() == *part && matching.replace(child).is_some() {
                    return err("ambiguous loose asset path");
                }
            }
            let Some(child) = matching else {
                return err("original asset unavailable");
            };
            let metadata = fs::symlink_metadata(&child)
                .map_err(|_| Error("loose asset metadata unavailable".into()))?;
            if metadata.file_type().is_symlink() {
                return err("loose asset symlink unsupported");
            }
            if i + 1 < parts.len() && !metadata.is_dir() {
                return err("loose asset parent is not directory");
            }
            verify_contained(&self.root, &child)?;
            path = child;
        }
        let mut file = dta::checked_open(&path).map_err(Error)?;
        verify_contained(&self.root, &path)?;
        read_asset_handle(&mut file)
    }
}
fn bounded_children(path: &Path) -> Result<Vec<PathBuf>> {
    let mut children = Vec::new();
    for entry in
        fs::read_dir(path).map_err(|_| Error("installation directory unreadable".into()))?
    {
        if children.len() == 100000 {
            return err("installation directory entry quota exceeded");
        }
        children.push(
            entry
                .map_err(|_| Error("installation directory entry unreadable".into()))?
                .path(),
        );
    }
    Ok(children)
}
fn verify_contained(root: &Path, path: &Path) -> Result<()> {
    let canonical = fs::canonicalize(path).map_err(|_| Error("asset unavailable".into()))?;
    if !canonical.starts_with(root) {
        return err("asset resolves outside installation");
    }
    Ok(())
}
fn read_asset_handle(file: &mut fs::File) -> Result<Vec<u8>> {
    use std::io::Read;
    let before = file
        .metadata()
        .map_err(|_| Error("asset metadata unavailable".into()))?;
    if !before.is_file() {
        return err("asset must be a regular file");
    }
    if before.len() > MAX_BYTES as u64 {
        return err("asset exceeds 64 MiB quota");
    }
    let mut b = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| Error("asset read failed".into()))?;
    let after = file
        .metadata()
        .map_err(|_| Error("asset metadata unavailable".into()))?;
    if b.len() > MAX_BYTES {
        return err("asset exceeds 64 MiB quota");
    }
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return err("asset changed while reading");
    }
    Ok(b)
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
    decode_bmp_impl(b, false)
}
/// Original-loader compatibility: initialize all 256 indexed palette slots to
/// black before reading the declared entries. Some shipped BMPs use an omitted
/// palette index. Strict decode_bmp keeps rejecting such files.
pub fn decode_bmp_original(b: &[u8]) -> Result<Texture> {
    decode_bmp_impl(b, true)
}
fn decode_bmp_impl(b: &[u8], original_palette: bool) -> Result<Texture> {
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
    if depth == 8 && original_palette {
        palette.resize(256, [0, 0, 0, 255]);
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
    #[test]
    fn common_frame_bytes_and_dummy_bounds_survive_intake() {
        let mut bytes = empty();
        bytes.truncate(16);
        bytes.extend(1u16.to_le_bytes());
        bytes.push(6);
        bytes.extend(0u16.to_le_bytes());
        for value in [0f32, -0., 3., 1., 2., 4., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0x89, 2, 0x8a, b'X']);
        bytes.extend([4, 0, 0xff, b'\t', b'A']);
        let bounds = [0x80000000u32, 0x7fc01234, 1, 0x3f800000, 0xbf800000, 0];
        for value in bounds {
            bytes.extend(value.to_le_bytes());
        }
        bytes.push(0);
        let model = parse_four_ds_scene(&bytes).unwrap();
        let frame = &model.frames[0];
        assert_eq!(frame.name, "ŠX");
        assert_eq!(frame.name_bytes, [0x8a, b'X']);
        assert_eq!(frame.properties, [0, 0xff, b'\t', b'A']);
        assert_eq!(frame.culling, 0x89);
        assert_eq!(frame.position[1].to_bits(), 0x80000000);
        assert_eq!(
            frame
                .dummy_bounds
                .unwrap()
                .into_iter()
                .flatten()
                .map(f32::to_bits)
                .collect::<Vec<_>>(),
            bounds
        );
        assert!(parse_four_ds(&bytes).is_err()); // ASCII model-name policy remains explicit.
        for length in 0..bytes.len() {
            assert!(parse_four_ds_scene(&bytes[..length]).is_err(), "{length}");
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
    #[test]
    fn original_palette_zero_initialization_does_not_consume_pixels() {
        let mut b = bmp();
        b.resize(62, 0);
        b[2..6].copy_from_slice(&62u32.to_le_bytes());
        b[10..14].copy_from_slice(&58u32.to_le_bytes());
        b[28] = 8;
        b[46..50].copy_from_slice(&1u32.to_le_bytes());
        b[54..58].copy_from_slice(&[3, 2, 1, 0]);
        b[58] = 255;
        assert!(decode_bmp(&b).is_err());
        assert_eq!(decode_bmp_original(&b).unwrap().rgba, [0, 0, 0, 255]);
        b[58] = 0;
        assert_eq!(decode_bmp_original(&b).unwrap().rgba, [1, 2, 3, 255]);
        for n in 0..b.len() {
            assert!(decode_bmp_original(&b[..n]).is_err());
        }
    }
}

#[cfg(test)]
mod mesh_tests {
    use super::*;
    pub(super) fn mesh() -> Vec<u8> {
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

/// Decode Windows-1250 text, rejecting undefined bytes instead of replacing names.
pub fn decode_windows1250(bytes: &[u8]) -> Result<String> {
    const UPPER: [u32; 128] = [
        0x20ac, 0x0000, 0x201a, 0x0000, 0x201e, 0x2026, 0x2020, 0x2021, 0x0000, 0x2030, 0x0160,
        0x2039, 0x015a, 0x0164, 0x017d, 0x0179, 0x0000, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022,
        0x2013, 0x2014, 0x0000, 0x2122, 0x0161, 0x203a, 0x015b, 0x0165, 0x017e, 0x017a, 0x00a0,
        0x02c7, 0x02d8, 0x0141, 0x00a4, 0x0104, 0x00a6, 0x00a7, 0x00a8, 0x00a9, 0x015e, 0x00ab,
        0x00ac, 0x00ad, 0x00ae, 0x017b, 0x00b0, 0x00b1, 0x02db, 0x0142, 0x00b4, 0x00b5, 0x00b6,
        0x00b7, 0x00b8, 0x0105, 0x015f, 0x00bb, 0x013d, 0x02dd, 0x013e, 0x017c, 0x0154, 0x00c1,
        0x00c2, 0x0102, 0x00c4, 0x0139, 0x0106, 0x00c7, 0x010c, 0x00c9, 0x0118, 0x00cb, 0x011a,
        0x00cd, 0x00ce, 0x010e, 0x0110, 0x0143, 0x0147, 0x00d3, 0x00d4, 0x0150, 0x00d6, 0x00d7,
        0x0158, 0x016e, 0x00da, 0x0170, 0x00dc, 0x00dd, 0x0162, 0x00df, 0x0155, 0x00e1, 0x00e2,
        0x0103, 0x00e4, 0x013a, 0x0107, 0x00e7, 0x010d, 0x00e9, 0x0119, 0x00eb, 0x011b, 0x00ed,
        0x00ee, 0x010f, 0x0111, 0x0144, 0x0148, 0x00f3, 0x00f4, 0x0151, 0x00f6, 0x00f7, 0x0159,
        0x016f, 0x00fa, 0x0171, 0x00fc, 0x00fd, 0x0163, 0x02d9,
    ];
    let mut output = String::new();
    for &byte in bytes {
        let code = if byte < 128 {
            byte as u32
        } else {
            UPPER[(byte - 128) as usize]
        };
        if byte >= 128 && code == 0 {
            return err("undefined Windows-1250 byte");
        }
        output.push(char::from_u32(code).unwrap());
    }
    Ok(output)
}

#[cfg(test)]
mod scene_model_tests {
    use super::*;
    fn joint_scene() -> (Vec<u8>, [u32; 16]) {
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        let words: [u32; 16] = [
            0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000, 0x3f800000, 0xbf800000,
            0x7f7fffff, 0xff7fffff, 0x7f800000, 0xff800000, 0x7fc01234, 0xffc05678, 0x7f801234,
            0xff801234,
        ];
        for (kind, parent, name) in [(10, 2u16, b"Bone".as_slice()), (6, 0, b"Root")] {
            bytes.push(kind);
            bytes.extend(parent.to_le_bytes());
            for value in [0f32, -0., 2., 1., 2., 3., 1., 0., 0., 0.] {
                bytes.extend(value.to_le_bytes());
            }
            bytes.extend([0x89, name.len() as u8]);
            bytes.extend(name);
            bytes.extend([3, 0, 0xff, 0x80]);
            if kind == 10 {
                for word in words {
                    bytes.extend(word.to_le_bytes());
                }
                bytes.extend(u32::MAX.to_le_bytes());
            } else {
                bytes.extend([0; 24]);
            }
        }
        bytes.push(0);
        (bytes, words)
    }
    #[test]
    fn joint_metadata_preserves_every_word_and_following_frame_alignment() {
        let (bytes, words) = joint_scene();
        let model = parse_four_ds_scene(&bytes).unwrap();
        let frame = &model.frames[0];
        assert_eq!(frame.kind, 10);
        assert_eq!(frame.visual, None);
        assert_eq!(frame.parent, Some(1));
        assert_eq!(frame.name_bytes, b"Bone");
        assert_eq!(frame.properties, [0, 0xff, 0x80]);
        assert_eq!(frame.culling, 0x89);
        assert!(frame.dummy_bounds.is_none());
        assert!(frame.instance.is_none());
        assert!(frame.lods.is_empty());
        let joint = frame.joint.as_ref().unwrap();
        assert_eq!(joint.bone_matrix.map(f32::to_bits), words);
        assert_eq!(joint.index, u32::MAX);
        let root = &model.frames[1];
        assert_eq!(root.name, "Root");
        assert_eq!(root.parent, None);
        assert!(root.dummy_bounds.is_some());
        assert!(root.joint.is_none());
        assert!(parse_four_ds(&bytes).is_err());
        for length in 0..bytes.len() {
            assert!(parse_four_ds_scene(&bytes[..length]).is_err(), "{length}");
        }
    }
    #[test]
    fn joint_intake_keeps_hierarchy_animation_and_extent_guards() {
        let (bytes, _) = joint_scene();
        for parent in [1u16, 3] {
            let mut invalid = bytes.clone();
            invalid[19..21].copy_from_slice(&parent.to_le_bytes());
            assert!(parse_four_ds_scene(&invalid).is_err());
        }
        let mut invalid = bytes.clone();
        *invalid.last_mut().unwrap() = 1;
        assert!(parse_four_ds_scene(&invalid).is_err());
        let mut invalid = bytes.clone();
        invalid.push(0);
        assert!(parse_four_ds_scene(&invalid).is_err());
        let mut invalid = bytes;
        invalid[16..18].copy_from_slice(&16385u16.to_le_bytes());
        assert!(parse_four_ds_scene(&invalid).is_err());
    }
    fn material_scene() -> Vec<u8> {
        let mut b = super::mesh_tests::mesh();
        // Reflection payload precedes diffuse name; independent alpha name follows it.
        b[16..20].copy_from_slice(&0x4008_8000u32.to_le_bytes());
        let mut names = 0.5f32.to_le_bytes().to_vec();
        for name in ["env.bmp", "color.bmp", "mask.bmp"] {
            names.push(name.len() as u8);
            names.extend(name.as_bytes());
        }
        b.splice(60..61, names);
        b
    }
    #[test]
    fn metadata_material_payloads_preserve_mesh_alignment() {
        let b = material_scene();
        assert!(parse_four_ds(&b).is_err());
        let m = parse_four_ds_scene(&b).unwrap();
        assert_eq!(m.materials[0].texture, "color.bmp");
        assert_eq!(m.materials[0].alpha_texture.as_deref(), Some("mask.bmp"));
        assert_eq!(
            m.materials[0].environment_texture.as_deref(),
            Some("env.bmp")
        );
        assert_eq!(m.materials[0].reflection, Some(0.5));
        assert_eq!(m.frames[0].lods[0].vertices[0].position, [1., 2., 3.]);
        for n in 0..b.len() {
            assert!(parse_four_ds_scene(&b[..n]).is_err(), "{n}");
        }
    }
    #[test]
    fn embedded_key_and_additive_flags_suppress_separate_alpha_payload() {
        for effect in [0x01000000u32, 0x20000000, 0x80000000] {
            let mut b = super::mesh_tests::mesh();
            b[16..20].copy_from_slice(&(0x40008000 | effect).to_le_bytes());
            let mut name = vec![9];
            name.extend(b"color.bmp");
            b.splice(60..61, name);
            let model = parse_four_ds_scene(&b).unwrap();
            assert!(model.materials[0].alpha_texture.is_none());
            assert_eq!(model.frames[0].lods[0].vertices[0].position, [1., 2., 3.]);
        }
    }
    #[test]
    fn zero_source_material_is_explicit_only_in_scene_mode() {
        let mut b = super::mesh_tests::mesh();
        let n = b.len();
        b[n - 3..n - 1].copy_from_slice(&0u16.to_le_bytes());
        assert!(parse_four_ds(&b).is_err());
        assert_eq!(
            parse_four_ds_scene(&b).unwrap().frames[0].lods[0].groups[0].material,
            UNASSIGNED_MATERIAL
        );
    }
    #[test]
    fn bounded_sector_metadata_and_portal_extents() {
        let mut b = b"4DS\0".to_vec();
        b.extend(29u16.to_le_bytes());
        b.extend([0; 8]);
        b.extend(0u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.push(5);
        b.extend(0u16.to_le_bytes());
        for f in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            b.extend(f.to_le_bytes());
        }
        b.extend([0, 0, 0]); // culling, name, properties
        b.extend([0; 8]);
        b.extend(0u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend([0; 24]);
        b.push(1);
        b.push(3);
        b.extend([0; 28 + 3 * 12]);
        b.push(0);
        let m = parse_four_ds_scene(&b).unwrap();
        assert_eq!(m.frames[0].kind, 5);
        assert!(m.frames[0].lods.is_empty());
        for n in 0..b.len() {
            assert!(parse_four_ds_scene(&b[..n]).is_err(), "{n}");
        }
        let mut huge = b.clone();
        huge[72..76].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_four_ds_scene(&huge).is_err());
    }
    #[test]
    fn cp1250_names_are_exact_and_undefined_bytes_rejected() {
        assert_eq!(
            decode_windows1250(&[0x8a, 0x9d, 0xec, 0x9a, 0x9e]).unwrap(),
            "Šťěšž"
        );
        assert!(decode_windows1250(&[0x81]).is_err());
    }
    fn billboard() -> Vec<u8> {
        let mut bytes = super::mesh_tests::mesh();
        assert_eq!(&bytes[63..65], &[1, 0]);
        bytes[64] = 4;
        let end = bytes.len() - 1;
        bytes.splice(end..end, [0; 5]);
        bytes
    }
    #[test]
    fn unsupported_billboard_is_classified_only_after_complete_validation() {
        let bytes = billboard();
        assert!(
            parse_four_ds_scene(&bytes)
                .unwrap_err()
                .is_unsupported_billboard()
        );
        for n in 0..bytes.len() {
            assert!(
                !parse_four_ds_scene(&bytes[..n])
                    .unwrap_err()
                    .is_unsupported_billboard(),
                "{n}"
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(
            !parse_four_ds_scene(&trailing)
                .unwrap_err()
                .is_unsupported_billboard()
        );
        let mut cyclic = bytes.clone();
        cyclic[67..69].copy_from_slice(&1u16.to_le_bytes());
        assert!(
            !parse_four_ds_scene(&cyclic)
                .unwrap_err()
                .is_unsupported_billboard()
        );
        let mut bad_index = bytes;
        let index_offset = bad_index.len() - 14;
        bad_index[index_offset..index_offset + 2].copy_from_slice(&1u16.to_le_bytes());
        assert!(
            !parse_four_ds_scene(&bad_index)
                .unwrap_err()
                .is_unsupported_billboard()
        );
    }
    #[test]
    fn other_unsupported_visuals_are_distinct_from_billboards() {
        for visual in [2, 3, 5, 255] {
            let mut bytes = super::mesh_tests::mesh();
            bytes[64] = visual;
            let error = parse_four_ds_scene(&bytes).unwrap_err();
            assert!(!error.is_unsupported_billboard());
            assert!(error.0.contains(&format!("visual type {visual};")));
        }
    }
    #[test]
    fn billboard_instance_requires_its_own_mode_payload() {
        let mut bytes = super::mesh_tests::mesh();
        let mut instance = bytes[63..112].to_vec();
        instance[1] = 4;
        instance.extend(1u16.to_le_bytes());
        instance.extend([0; 5]);
        let end = bytes.len() - 1;
        bytes.splice(end..end, instance);
        bytes[61..63].copy_from_slice(&2u16.to_le_bytes());
        assert!(
            parse_four_ds_scene(&bytes)
                .unwrap_err()
                .is_unsupported_billboard()
        );
        // Removing just the instance's axis/mode bytes must be a truncation,
        // rather than an unsupported billboard that callers may deliberately skip.
        let end = bytes.len() - 1;
        bytes.drain(end - 5..end);
        assert!(
            !parse_four_ds_scene(&bytes)
                .unwrap_err()
                .is_unsupported_billboard()
        );
    }
}

#[cfg(test)]
mod position_animation_intake_tests {
    use super::*;
    fn pair() -> (Vec<u8>, Vec<u8>) {
        let mut model = mesh_tests::mesh();
        model[110] = 4;
        model.splice(111..111, *b"Bone");
        assert_eq!(parse_four_ds(&model).unwrap().frames[0].name, "Bone");
        *model.last_mut().unwrap() = 1;
        let mut payload = Vec::new();
        payload.extend(1_u16.to_le_bytes());
        payload.extend(10_u16.to_le_bytes());
        payload.extend(12_u32.to_le_bytes());
        payload.extend(17_u32.to_le_bytes());
        payload.extend(b"Bone\0");
        payload.extend(2_u32.to_le_bytes());
        payload.extend(1_u16.to_le_bytes());
        payload.extend(0_u16.to_le_bytes());
        for value in [1_f32, 2., 3.] {
            payload.extend(value.to_le_bytes());
        }
        let mut animation = b"5DS\0".to_vec();
        animation.extend(20_u16.to_le_bytes());
        animation.extend([0; 8]);
        animation.extend((payload.len() as u32).to_le_bytes());
        animation.extend(payload);
        (model, animation)
    }
    #[test]
    fn preserves_original_animation_count_and_static_rejection() {
        let (model, animation) = pair();
        assert!(parse_four_ds(&model).is_err());
        assert!(parse_four_ds_scene(&model).is_err());
        let source = parse_four_ds_position_animation(&model, &animation).unwrap();
        assert_eq!(source.load_animation_count(), 1);
        assert_eq!(source.model().frames[0].name, "Bone");
        assert_eq!(source.animation().tracks[0].name, b"Bone");
        assert_eq!(*model.last().unwrap(), 1);
        let (geometry, count, set) = source.into_parts();
        assert_eq!(count, 1);
        assert_eq!(geometry.frames[0].name.as_bytes(), set.tracks[0].name);
    }
    #[test]
    fn incompatible_or_truncated_original_pairs_fail() {
        let (mut model, animation) = pair();
        for count in [0, 2] {
            *model.last_mut().unwrap() = count;
            assert!(parse_four_ds_position_animation(&model, &animation).is_err());
        }
        *model.last_mut().unwrap() = 1;
        let mut mismatch = animation.clone();
        mismatch[30] = b'b';
        assert!(parse_four_ds_position_animation(&model, &mismatch).is_err());
        for len in 0..model.len() {
            assert!(parse_four_ds_position_animation(&model[..len], &animation).is_err());
        }
        for len in 0..animation.len() {
            assert!(parse_four_ds_position_animation(&model, &animation[..len]).is_err());
        }
    }
}
