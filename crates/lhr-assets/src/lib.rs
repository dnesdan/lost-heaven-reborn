//! Bounded read-only PC 4DS v29 and BMP intake. Source coordinates and order are preserved.
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};
pub mod animation;
mod cache;
pub mod klz;
pub mod texture_source;
pub use cache::{CacheObject, CachePart, CityCache, parse_city_cache};
mod dta;
mod menu;
pub mod rw_data;
mod scene;
pub mod scene_actor_stream;
pub mod script;
pub mod script_vm;
pub mod startup_archives;
mod tga;
pub use menu::{
    FontGlyph, FontMetrics, LanguageFontMap, MenuDefinition, MenuItem, TextDb, TextDbEntry,
    parse_font_metrics, parse_language_font_map, parse_menu_definition, parse_text_db,
};
pub use scene::{Scene, SceneActor, SceneObject, SceneScript, parse_scene2};
pub use tga::decode_tga;

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_MORPH_CHANNELS: usize = 100_000;
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
fn charge_morph_records<T>(total: &mut usize, count: usize) -> Result<()> {
    let next = count
        .checked_mul(std::mem::size_of::<T>())
        .and_then(|bytes| total.checked_add(bytes))
        .filter(|&bytes| bytes <= MAX_BYTES)
        .ok_or_else(|| Error("morph decoded storage quota exceeded".into()))?;
    *total = next;
    Ok(())
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
/// Original kind-7 target suffix. References retain their source values; this
/// does not resolve or update target links.
#[derive(Debug, Clone)]
pub struct TargetRecord {
    pub flags: u16,
    /// Original u16 reference words; the source u8 count is this vector's length.
    pub frame_indices: Vec<u16>,
}
/// Original PC kind-12 suffix consumed by I3D_occluder::LoadImage (RVA
/// 0x3c9b0): two u32 counts followed by packed vertices and triangle indices.
/// This is file data only; no native Build, planes or runtime identity is implied.
#[derive(Debug, Clone)]
pub struct OccluderRecord {
    /// Source vertex order and f32 bits, including nonfinite values. Consumers
    /// must validate their supported numerical domain before evaluating geometry.
    pub vertices: Vec<[f32; 3]>,
    /// Original three u16 indices per face, without conversion, reordering or
    /// range validation. The u32 vertex count is not narrowed to the index width.
    pub faces: Vec<[u16; 3]>,
}
/// Original PC singlemesh skin suffix. This preserves file records; it does not
/// establish a complete joint binding or admit the model to static rendering.
#[derive(Debug, Clone)]
pub struct SkinRecord {
    /// In the same order as the frame's geometry LODs.
    pub lods: Vec<SkinLodRecord>,
}
#[derive(Debug, Clone)]
pub struct SkinLodRecord {
    /// A zero bone-count byte has no following definition bytes in PC files.
    pub definition: Option<SkinLodDefinition>,
}
#[derive(Debug, Clone)]
pub struct SkinLodDefinition {
    /// Source count of the undeformed rigid vertex suffix.
    pub root_vertex_count: u32,
    /// Source min/max words, without numerical or bounds interpretation.
    pub bounds: [f32; 6],
    /// Source bone order; the stored bone count is this vector's length.
    pub bones: Vec<SkinBoneRecord>,
}
#[derive(Debug, Clone)]
pub struct SkinBoneRecord {
    /// Original matrix words in source order, without finite-value validation.
    pub matrix: [f32; 16],
    pub rigid_vertex_count: u32,
    pub weighted_vertex_count: u32,
    /// Original blend-joint reference word; no index conversion is applied.
    pub blend_joint_index: u32,
    pub bounds: [f32; 6],
    /// Original f32 words, including nonfinite/out-of-range values. Binding and
    /// deformation consumers must validate their own supported numerical domain.
    pub weights: Vec<f32>,
}
/// Raw original PC morph suffix. No morph evaluation or neutral state is implied.
#[derive(Debug, Clone)]
pub struct MorphRecord {
    /// A zero count has no following definition bytes.
    pub target_count: u8,
    pub definition: Option<MorphDefinition>,
}
#[derive(Debug, Clone)]
pub struct MorphDefinition {
    pub channel_count: u8,
    /// Source LOD order, corresponding to the geometry LODs.
    pub lods: Vec<MorphLodRecord>,
    /// Original final 40 bytes as little-endian words; semantics remain opaque.
    pub trailer_words: [u32; 10],
}
#[derive(Debug, Clone)]
pub struct MorphLodRecord {
    pub channels: Vec<MorphChannelRecord>,
}
#[derive(Debug, Clone)]
pub struct MorphChannelRecord {
    pub vertex_count: u16,
    /// Source vertex-major, target-minor order. Every float bit is retained.
    pub samples: Vec<MorphSampleRecord>,
    /// Absent for an empty channel. Zero leaves native index storage absent.
    /// Every nonzero source byte selects the explicit index array; no implicit
    /// vertex association is invented when it is zero.
    pub index_mode: Option<u8>,
    /// Present only for a nonzero index mode, with one index per channel vertex.
    pub vertex_indices: Vec<u16>,
}
#[derive(Debug, Clone)]
pub struct MorphSampleRecord {
    pub position: [f32; 3],
    pub normal: [f32; 3],
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
    /// Original kind-10 joint suffix, retained by scene and singlemesh intake.
    pub joint: Option<JointRecord>,
    /// Original kind-7 target metadata, without link resolution or evaluation.
    pub target: Option<TargetRecord>,
    /// Original kind-12 source geometry, without native Build or runtime creation.
    pub occluder: Option<OccluderRecord>,
    /// Inherited skin metadata, retained by scene and explicit character intake.
    pub skin: Option<SkinRecord>,
    /// Retained only by explicit singlemorph intake; requires runtime validation.
    pub morph: Option<MorphRecord>,
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
    fn raw_vec<const N: usize>(&mut self) -> Result<[f32; N]> {
        let bytes = self.take(N * 4)?;
        Ok(std::array::from_fn(|i| {
            f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
        }))
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
    parse_four_ds_impl(b, false, false, false, false).map(|(model, _)| model)
}
/// Broader scene intake: retains material effects as metadata and accepts sector,
/// light, target, occluder and joint frames, and visual-2 singlemesh skin records. Joint
/// and skin suffixes are raw metadata, without skinning or joint-runtime
/// behavior. Morph visuals and singlemesh instances remain unsupported.
/// Callers must implement or report material
/// effects; this does not promise identical original rendering or simulation.
pub fn parse_four_ds_scene(b: &[u8]) -> Result<Model> {
    parse_four_ds_impl(b, true, false, true, false).map(|(model, _)| model)
}
/// Bounded singlemesh metadata intake with the static material and animation
/// gates. Accepts only mesh, dummy and joint frames and requires a visual-2
/// frame. Raw skin records still require binding/deformation validation before
/// use; this function does not establish a renderable character.
pub fn parse_four_ds_skin(b: &[u8]) -> Result<Model> {
    let (model, _) = parse_four_ds_impl(b, false, false, true, false)?;
    if !model.frames.iter().any(|frame| frame.skin.is_some()) {
        return err("singlemesh metadata requires a visual-2 frame");
    }
    Ok(model)
}
/// Explicit original singlemorph metadata intake. Retains inherited skin and all
/// morph channels, source vertex associations and opaque words. Scene material
/// metadata and target frames are admitted, but LoadAnimCount must remain zero.
/// This establishes neither morph initialization nor renderable deformation.
/// Existing static, scene and visual-2-only intake keep their original gates.
pub fn parse_four_ds_singlemorph(b: &[u8]) -> Result<Model> {
    let (model, _) = parse_four_ds_impl(b, true, false, true, true)?;
    if !model.frames.iter().any(|frame| frame.morph.is_some()) {
        return err("singlemorph metadata requires a visual-3 frame");
    }
    Ok(model)
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
    let (model, load_animation_count) = parse_four_ds_impl(model_bytes, false, true, false, false)?;
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

fn parse_four_ds_impl(
    b: &[u8],
    scene: bool,
    allow_animation: bool,
    allow_skin: bool,
    allow_morph: bool,
) -> Result<(Model, u8)> {
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
    let mut total_skin_weights = 0usize;
    let mut total_morph_samples = 0usize;
    let mut total_morph_channels = 0usize;
    let mut total_morph_bytes = 0usize;
    let mut total_occluder_bytes = 0usize;
    let mut unsupported_billboard = false;
    for _ in 0..count {
        let kind = r.u8()?;
        if kind != 1
            && kind != 6
            && !(scene && [2, 5, 7, 10, 12].contains(&kind))
            && !(allow_skin && kind == 10)
        {
            return err("unsupported frame type; only static visual and dummy supported");
        };
        let visual = if kind == 1 { Some(r.u8()?) } else { None };
        let mut render_flags = 0;
        let mut logic_flags = 0;
        if let Some(visual) = visual {
            if allow_skin && visual == 3 && !allow_morph {
                return err("unsupported visual type 3; morph singlemesh unsupported");
            }
            if visual > 1
                && visual != 4
                && !(allow_skin && visual == 2)
                && !(allow_morph && visual == 3)
            {
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
        let mut target = None;
        let mut occluder = None;
        let mut skin = None;
        let mut morph = None;
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
            let geometry_bytes = vertices
                .checked_mul(12)
                .and_then(|bytes| faces.checked_mul(6).and_then(|n| bytes.checked_add(n)))
                .ok_or_else(|| Error("sector/occluder extent overflow".into()))?;
            if kind == 12 {
                // Retained storage is bounded in aggregate as well as by the
                // existing per-record host count policy; neither is a native limit.
                total_occluder_bytes = total_occluder_bytes
                    .checked_add(geometry_bytes)
                    .filter(|&bytes| bytes <= MAX_BYTES)
                    .ok_or_else(|| Error("occluder decoded storage quota exceeded".into()))?;
                // Prove the complete source extent before allocating either array.
                let bytes = r.take(geometry_bytes)?;
                let (vertex_bytes, face_bytes) = bytes.split_at(vertices * 12);
                occluder = Some(OccluderRecord {
                    vertices: vertex_bytes
                        .as_chunks::<12>()
                        .0
                        .iter()
                        .map(|vertex| {
                            std::array::from_fn(|i| {
                                f32::from_le_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap())
                            })
                        })
                        .collect(),
                    faces: face_bytes
                        .as_chunks::<6>()
                        .0
                        .iter()
                        .map(|face| {
                            std::array::from_fn(|i| {
                                u16::from_le_bytes(face[i * 2..i * 2 + 2].try_into().unwrap())
                            })
                        })
                        .collect(),
                });
            } else {
                r.take(geometry_bytes)?;
            }
            if kind == 5 {
                r.take(24)?;
                let portals = r.u8()?;
                for _ in 0..portals {
                    let points = r.u8()? as usize;
                    r.take(28 + points * 12)?;
                }
            }
        } else if kind == 7 {
            let flags = r.u16()?;
            let count = r.u8()? as usize;
            let mut frame_indices = Vec::with_capacity(count);
            for _ in 0..count {
                frame_indices.push(r.u16()?);
            }
            target = Some(TargetRecord {
                flags,
                frame_indices,
            });
        } else {
            let id = r.u16()? as usize;
            if id != 0 {
                if matches!(visual, Some(2 | 3)) {
                    return err("unsupported singlemesh instance");
                }
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
        if matches!(visual, Some(2 | 3)) {
            let mut skin_lods = Vec::with_capacity(lods.len());
            for _ in &lods {
                let bone_count = r.u8()? as usize;
                if bone_count == 0 {
                    skin_lods.push(SkinLodRecord { definition: None });
                    continue;
                }
                let root_vertex_count = r.u32()?;
                let bounds = r.raw_vec()?;
                let mut bones = Vec::with_capacity(bone_count);
                for _ in 0..bone_count {
                    let matrix = r.raw_vec()?;
                    let rigid_vertex_count = r.u32()?;
                    let weighted_vertex_count = r.u32()?;
                    let blend_joint_index = r.u32()?;
                    let bounds = r.raw_vec()?;
                    total_skin_weights = total_skin_weights
                        .checked_add(weighted_vertex_count as usize)
                        .ok_or_else(|| Error("skin weight quota exceeded".into()))?;
                    if total_skin_weights > 1_000_000 {
                        return err("skin weight quota exceeded");
                    }
                    let weight_bytes = r.take(weighted_vertex_count as usize * 4)?;
                    let weights = weight_bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| f32::from_le_bytes(*b))
                        .collect();
                    bones.push(SkinBoneRecord {
                        matrix,
                        rigid_vertex_count,
                        weighted_vertex_count,
                        blend_joint_index,
                        bounds,
                        weights,
                    });
                }
                skin_lods.push(SkinLodRecord {
                    definition: Some(SkinLodDefinition {
                        root_vertex_count,
                        bounds,
                        bones,
                    }),
                });
            }
            skin = Some(SkinRecord { lods: skin_lods });
        }
        if visual == Some(3) {
            let target_count = r.u8()?;
            let definition = if target_count == 0 {
                None
            } else {
                let channel_count = r.u8()?;
                let lod_count = r.u8()? as usize;
                if lod_count > lods.len() {
                    return err("morph LOD count exceeds geometry LODs");
                }
                // Empty channels still own vector metadata. Preflight the full
                // definition before allocating any LOD/channel arrays so a tiny
                // zero-vertex wire record cannot amplify without an aggregate cap.
                let channel_records = lod_count * channel_count as usize;
                total_morph_channels = total_morph_channels
                    .checked_add(channel_records)
                    .filter(|&count| count <= MAX_MORPH_CHANNELS)
                    .ok_or_else(|| Error("morph channel quota exceeded".into()))?;
                charge_morph_records::<MorphLodRecord>(&mut total_morph_bytes, lod_count)?;
                charge_morph_records::<MorphChannelRecord>(
                    &mut total_morph_bytes,
                    channel_records,
                )?;
                let mut morph_lods = Vec::with_capacity(lod_count);
                for lod in &lods[..lod_count] {
                    let mut channels = Vec::with_capacity(channel_count as usize);
                    for _ in 0..channel_count {
                        let vertex_count = r.u16()?;
                        let sample_count = vertex_count as usize * target_count as usize;
                        total_morph_samples = total_morph_samples
                            .checked_add(sample_count)
                            .ok_or_else(|| Error("morph sample quota exceeded".into()))?;
                        if total_morph_samples > 1_000_000 {
                            return err("morph sample quota exceeded");
                        }
                        charge_morph_records::<MorphSampleRecord>(
                            &mut total_morph_bytes,
                            sample_count,
                        )?;
                        // Check the complete sample extent before allocating records.
                        let sample_bytes = r.take(sample_count * 24)?;
                        let mut samples = Vec::with_capacity(sample_count);
                        for bytes in sample_bytes.as_chunks::<24>().0 {
                            let mut sample_reader = Reader { b: bytes, p: 0 };
                            samples.push(MorphSampleRecord {
                                position: sample_reader.raw_vec()?,
                                normal: sample_reader.raw_vec()?,
                            });
                        }
                        let index_mode = if vertex_count == 0 {
                            None
                        } else {
                            Some(r.u8()?)
                        };
                        let mut vertex_indices = Vec::new();
                        if index_mode.is_some_and(|mode| mode != 0) {
                            charge_morph_records::<u16>(
                                &mut total_morph_bytes,
                                vertex_count as usize,
                            )?;
                            vertex_indices.reserve(vertex_count as usize);
                            for _ in 0..vertex_count {
                                let index = r.u16()?;
                                if index as usize >= lod.vertices.len() {
                                    return err("morph index outside geometry vertex array");
                                }
                                vertex_indices.push(index);
                            }
                        }
                        channels.push(MorphChannelRecord {
                            vertex_count,
                            samples,
                            index_mode,
                            vertex_indices,
                        });
                    }
                    morph_lods.push(MorphLodRecord { channels });
                }
                let mut trailer_words = [0; 10];
                for word in &mut trailer_words {
                    *word = r.u32()?;
                }
                Some(MorphDefinition {
                    channel_count,
                    lods: morph_lods,
                    trailer_words,
                })
            };
            morph = Some(MorphRecord {
                target_count,
                definition,
            });
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
            target,
            occluder,
            skin,
            morph,
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
/// Archive-backed reads use the native ASCII filename domain; selected non-ASCII
/// archive names return an explicit unsupported error. Loose Unicode paths retain
/// bounded host IO. Empty archive entries use checked zero-block metadata because
/// native archive_open unconditionally fetches a first block.
/// The installation directory tree must be trusted and stable during use. Handle and
/// metadata checks detect ordinary replacement/mutation; they do not confine hostile
/// concurrent replacement of parent directories (which requires descriptor-relative IO).
pub struct Installation {
    root: PathBuf,
    archives: rw_data::io::NativeFiles,
    archive_index: std::collections::BTreeMap<String, (u64, dta::Entry)>,
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
            let ordinal = startup_archives::ARCHIVE_NAMES
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(name.as_bytes()));
            if let Some(ordinal) = ordinal
                && discovered.insert(ordinal, path).is_some()
            {
                return err("ambiguous duplicate archive filename");
            }
        }
        let archives = rw_data::io::NativeFiles::new(&root, MAX_BYTES as u32, 256 * 1024 * 1024)
            .map_err(|e| Error(format!("archive provider: {e:?}")))?;
        let mut archive_index = std::collections::BTreeMap::new();
        for (ordinal, path) in discovered {
            verify_contained(&root, &path)?;
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| Error("archive filename unavailable".into()))?;
            let owner = archives
                .register(
                    filename.as_bytes(),
                    startup_archives::ARCHIVE_UNLOCK_KEYS[ordinal],
                )
                .map_err(|e| Error(format!("archive registration: {e:?}")))?;
            verify_contained(&root, &path)?;
            for entry in archives
                .admitted_catalog(owner)
                .map_err(|e| Error(format!("archive catalog: {e:?}")))?
            {
                let key = dta::canonical(&entry.path).map_err(Error)?;
                // The first registered archive owns this canonical path, including
                // its original source row and exact native filename spelling.
                archive_index.entry(key).or_insert((owner, entry));
            }
        }
        Ok(Self {
            root,
            archives,
            archive_index,
        })
    }
    /// Read an admitted original Windows relative filename. Separator conversion
    /// happens only at this host IO boundary; the caller's name stays unchanged.
    /// Archive precedence, selected-source errors and loose-path checks are the
    /// same as `read`. Original non-ASCII filename encodings remain unavailable.
    pub fn read_original_path(&self, name: &str) -> Result<Vec<u8>> {
        if !name.is_ascii() {
            return err("non-ASCII original filename unsupported");
        }
        self.read(&name.replace('\\', "/"))
    }
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        safe_relative(name)?;
        let canonical = dta::canonical(name).map_err(Error)?;
        if let Some((owner, entry)) = self.archive_index.get(&canonical) {
            return self
                .archives
                .read_admitted(*owner, entry)
                .map_err(|e| Error(format!("selected archive read: {e:?}")));
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
    fn skin_scene() -> (Vec<u8>, [u32; 16], usize) {
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend([1, 2, 0x31, 0x42]);
        bytes.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0, 4, b'S', b'k', b'i', b'n', 0]);
        bytes.extend(0u16.to_le_bytes());
        bytes.push(2);
        for n in [2u16, 1] {
            bytes.extend((n as f32).to_le_bytes());
            bytes.extend(n.to_le_bytes());
            for _ in 0..n {
                bytes.extend([0; 32]);
            }
            bytes.push(0);
        }
        let words: [u32; 16] = [
            0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000, 0x3f800000, 0xbf800000,
            0x7f7fffff, 0xff7fffff, 0x7f800000, 0xff800000, 0x7fc01234, 0xffc05678, 0x7f801234,
            0xff801234,
        ];
        // First LOD deliberately retains raw counts/references outside any
        // renderer's supported range. Metadata intake does not bind joints.
        bytes.push(1);
        bytes.extend(u32::MAX.to_le_bytes());
        for word in &words[..6] {
            bytes.extend(word.to_le_bytes());
        }
        for word in words {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(u32::MAX.to_le_bytes());
        let weighted_count_offset = bytes.len();
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(u32::MAX.to_le_bytes());
        for word in &words[10..] {
            bytes.extend(word.to_le_bytes());
        }
        for word in words {
            bytes.extend(word.to_le_bytes());
        }
        // The original PC zero-count branch has no definition payload.
        bytes.push(0);
        bytes.push(6);
        bytes.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0, 4, b'T', b'a', b'i', b'l', 0]);
        bytes.extend([0; 24]);
        bytes.push(0);
        (bytes, words, weighted_count_offset)
    }
    #[test]
    fn skin_metadata_preserves_raw_words_lod_order_and_following_frame() {
        let (bytes, words, _) = skin_scene();
        for model in [
            parse_four_ds_scene(&bytes).unwrap(),
            parse_four_ds_skin(&bytes).unwrap(),
        ] {
            let frame = &model.frames[0];
            assert_eq!(frame.visual, Some(2));
            assert_eq!(frame.render_flags, 0x31);
            assert_eq!(frame.logic_flags, 0x42);
            let skin = frame.skin.as_ref().unwrap();
            assert_eq!(skin.lods.len(), 2);
            assert_eq!(frame.lods[0].vertices.len(), 2);
            assert_eq!(frame.lods[1].vertices.len(), 1);
            let definition = skin.lods[0].definition.as_ref().unwrap();
            assert_eq!(definition.root_vertex_count, u32::MAX);
            assert_eq!(definition.bounds.map(f32::to_bits), words[..6]);
            let bone = &definition.bones[0];
            assert_eq!(bone.matrix.map(f32::to_bits), words);
            assert_eq!(bone.rigid_vertex_count, u32::MAX);
            assert_eq!(bone.weighted_vertex_count, 16);
            assert_eq!(bone.blend_joint_index, u32::MAX);
            assert_eq!(bone.bounds.map(f32::to_bits), words[10..]);
            assert_eq!(
                bone.weights.iter().map(|w| w.to_bits()).collect::<Vec<_>>(),
                words
            );
            assert!(skin.lods[1].definition.is_none());
            assert_eq!(model.frames[1].name, "Tail");
            assert!(model.frames[1].skin.is_none());
        }
        assert!(parse_four_ds(&bytes).is_err());
        assert!(parse_four_ds_skin(&super::mesh_tests::mesh()).is_err());
        for length in 0..bytes.len() {
            assert!(parse_four_ds_scene(&bytes[..length]).is_err(), "{length}");
        }
    }
    #[test]
    fn skin_intake_keeps_weight_animation_and_variant_guards() {
        let (bytes, _, weighted_count_offset) = skin_scene();
        let mut invalid = bytes.clone();
        invalid[weighted_count_offset..weighted_count_offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(
            parse_four_ds_scene(&invalid)
                .unwrap_err()
                .0
                .contains("skin weight quota")
        );
        let mut invalid = bytes.clone();
        *invalid.last_mut().unwrap() = 1;
        assert!(
            parse_four_ds_skin(&invalid)
                .unwrap_err()
                .0
                .contains("animated model")
        );
        let mut invalid = bytes.clone();
        invalid.push(0);
        assert!(parse_four_ds_scene(&invalid).is_err());
        let mut morph = bytes;
        morph[19] = 3;
        assert!(
            parse_four_ds_scene(&morph)
                .unwrap_err()
                .0
                .contains("visual type 3;")
        );
        let mut instance = super::mesh_tests::mesh();
        instance[64] = 2;
        instance[112..114].copy_from_slice(&1u16.to_le_bytes());
        assert!(
            parse_four_ds_scene(&instance)
                .unwrap_err()
                .0
                .contains("singlemesh instance")
        );
    }
    #[test]
    fn skin_strict_route_keeps_static_material_gate() {
        let mut bytes = super::mesh_tests::mesh();
        bytes[16..20].copy_from_slice(&0x8000u32.to_le_bytes());
        bytes[64] = 2;
        bytes.insert(bytes.len() - 1, 0); // Zero-bone LOD, no definition payload.
        assert!(parse_four_ds_scene(&bytes).is_ok());
        assert!(
            parse_four_ds_skin(&bytes)
                .unwrap_err()
                .0
                .contains("unsupported alpha")
        );
    }
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
    fn occluder_scene(vertices: &[[u32; 3]], faces: &[[u16; 3]]) -> Vec<u8> {
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        for (kind, name) in [(12, b"Occluder".as_slice()), (6, b"Tail".as_slice())] {
            bytes.push(kind);
            bytes.extend(0u16.to_le_bytes());
            for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
                bytes.extend(value.to_le_bytes());
            }
            bytes.extend([0, name.len() as u8]);
            bytes.extend(name);
            bytes.push(0);
            if kind == 12 {
                bytes.extend((vertices.len() as u32).to_le_bytes());
                bytes.extend((faces.len() as u32).to_le_bytes());
                for vertex in vertices {
                    for word in vertex {
                        bytes.extend(word.to_le_bytes());
                    }
                }
                for face in faces {
                    for index in face {
                        bytes.extend(index.to_le_bytes());
                    }
                }
            } else {
                bytes.extend([0; 24]);
            }
        }
        bytes.push(0);
        bytes
    }
    #[test]
    fn occluder_geometry_preserves_raw_words_face_order_and_following_frame() {
        let vertices = [
            [0x80000000, 0x7f801234, 1],
            [0x7f800000, 0xff800000, 0xffc05678],
        ];
        // Out-of-range references remain raw source data, including u16::MAX.
        let faces = [[u16::MAX, 1, 0], [0, 0, 7]];
        let bytes = occluder_scene(&vertices, &faces);
        let model = parse_four_ds_scene(&bytes).unwrap();
        let frame = &model.frames[0];
        assert_eq!(frame.kind, 12);
        assert!(frame.lods.is_empty());
        let record = frame.occluder.as_ref().unwrap();
        assert_eq!(
            record
                .vertices
                .iter()
                .map(|v| v.map(f32::to_bits))
                .collect::<Vec<_>>(),
            vertices
        );
        assert_eq!(record.faces, faces);
        assert_eq!(model.frames[1].name, "Tail");
        assert!(model.frames[1].occluder.is_none());
        assert!(parse_four_ds(&bytes).is_err());
        for length in 0..bytes.len() {
            assert!(parse_four_ds_scene(&bytes[..length]).is_err(), "{length}");
        }
        let empty = parse_four_ds_scene(&occluder_scene(&[], &[])).unwrap();
        let record = empty.frames[0].occluder.as_ref().unwrap();
        assert!(record.vertices.is_empty() && record.faces.is_empty());
    }
    #[test]
    fn occluder_u32_counts_keep_existing_host_quota_and_exact_extents() {
        let mut bytes = occluder_scene(&[], &[]);
        // The suffix follows the common name/property bytes at this fixed fixture offset.
        let counts = 18 + 1 + 2 + 40 + 1 + 1 + b"Occluder".len() + 1;
        for count_offset in [counts, counts + 4] {
            bytes[count_offset..count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(
                parse_four_ds_scene(&bytes)
                    .unwrap_err()
                    .0
                    .contains("sector/occluder quota")
            );
            bytes[count_offset..count_offset + 4].fill(0);
        }
        // Counts wider than a face index are retained without narrowing.
        let vertices = vec![[0; 3]; 65_536];
        let model = parse_four_ds_scene(&occluder_scene(&vertices, &[[65_535; 3]])).unwrap();
        assert_eq!(
            model.frames[0].occluder.as_ref().unwrap().vertices.len(),
            65_536
        );
        assert_eq!(
            model.frames[0].occluder.as_ref().unwrap().faces,
            [[65_535; 3]]
        );
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
        for visual in [3, 5, 255] {
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

#[cfg(test)]
mod singlemorph_intake_tests {
    use super::*;

    fn common(bytes: &mut Vec<u8>, kind: u8, parent: u16, name: &[u8]) {
        bytes.push(kind);
        if kind == 1 {
            bytes.extend([3, 0x31, 0x42]);
        }
        bytes.extend(parent.to_le_bytes());
        for value in [0f32, 0., 0., 1., 1., 1., 1., 0., 0., 0.] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([0, name.len() as u8]);
        bytes.extend(name);
        bytes.push(0);
    }
    fn prefix() -> Vec<u8> {
        let mut bytes = b"4DS\0".to_vec();
        bytes.extend(29u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0u16.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        common(&mut bytes, 1, 0, b"Morph");
        bytes.extend(0u16.to_le_bytes()); // No instance.
        bytes.push(2);
        for vertices in [2u16, 1] {
            bytes.extend((vertices as f32).to_le_bytes());
            bytes.extend(vertices.to_le_bytes());
            bytes.extend(vec![0; vertices as usize * 32]);
            bytes.push(0);
        }
        bytes.extend([0, 0]); // Empty inherited skin in both geometry LODs.
        bytes
    }
    fn finish(bytes: &mut Vec<u8>) {
        common(bytes, 6, 1, b"Tail");
        bytes.extend([0; 24]);
        bytes.push(0);
    }
    fn words() -> [u32; 6] {
        [
            0x80000000, 0x7f801234, 0xffc05678, 1, 0x7f800000, 0xff800000,
        ]
    }
    fn fixture() -> (Vec<u8>, usize, usize) {
        let mut bytes = prefix();
        let morph_offset = bytes.len();
        bytes.extend([2, 3, 2]); // Targets, channels, LODs.
        let mut index_offset = 0;
        for lod in 0..2 {
            for channel in 0..3 {
                let vertices: u16 = if channel == 0 {
                    0
                } else if channel == 1 {
                    3 // Zero mode retains no invented association, even beyond geometry.
                } else if channel == 2 && lod == 0 {
                    2
                } else {
                    1
                };
                bytes.extend(vertices.to_le_bytes());
                if vertices == 0 {
                    continue; // No samples or index-mode byte.
                }
                for sample in 0..vertices * 2 {
                    for word in words().map(|word| word ^ sample as u32) {
                        bytes.extend(word.to_le_bytes());
                    }
                }
                let mode = if channel == 1 { 0 } else { 7 };
                bytes.push(mode);
                if mode != 0 {
                    index_offset = bytes.len();
                    for vertex in 0..vertices {
                        bytes.extend((if lod == 0 { 1 - vertex } else { 0 }).to_le_bytes());
                    }
                }
            }
        }
        for word in 0..10u32 {
            bytes.extend((0x7f800001 + word).to_le_bytes());
        }
        finish(&mut bytes);
        (bytes, morph_offset, index_offset)
    }
    #[test]
    fn raw_morph_lods_channels_and_numeric_words_are_retained() {
        let (bytes, _, _) = fixture();
        let model = parse_four_ds_singlemorph(&bytes).unwrap();
        let frame = &model.frames[0];
        assert_eq!(frame.visual, Some(3));
        assert_eq!(frame.skin.as_ref().unwrap().lods.len(), 2);
        assert!(
            frame
                .skin
                .as_ref()
                .unwrap()
                .lods
                .iter()
                .all(|l| l.definition.is_none())
        );
        let morph = frame.morph.as_ref().unwrap();
        assert_eq!(morph.target_count, 2);
        let definition = morph.definition.as_ref().unwrap();
        assert_eq!(definition.channel_count, 3);
        assert_eq!(definition.lods.len(), 2);
        for (lod_index, lod) in definition.lods.iter().enumerate() {
            assert_eq!(lod.channels.len(), 3);
            assert_eq!(lod.channels[0].vertex_count, 0);
            assert!(lod.channels[0].samples.is_empty());
            assert_eq!(lod.channels[0].index_mode, None);
            assert!(lod.channels[0].vertex_indices.is_empty());
            assert_eq!(lod.channels[1].index_mode, Some(0));
            assert!(lod.channels[1].vertex_indices.is_empty());
            assert_eq!(lod.channels[2].index_mode, Some(7));
            assert_eq!(
                lod.channels[2].vertex_indices,
                if lod_index == 0 { vec![1, 0] } else { vec![0] }
            );
            for channel in &lod.channels[1..] {
                assert_eq!(channel.samples.len(), channel.vertex_count as usize * 2);
                for (index, sample) in channel.samples.iter().enumerate() {
                    let expected = words().map(|word| word ^ index as u32);
                    assert_eq!(sample.position.map(f32::to_bits), expected[..3]);
                    assert_eq!(sample.normal.map(f32::to_bits), expected[3..]);
                }
            }
        }
        assert_eq!(
            definition.trailer_words,
            std::array::from_fn(|i| 0x7f800001 + i as u32)
        );
        assert_eq!(model.frames[1].name_bytes, b"Tail");
        assert!(model.frames[1].morph.is_none());
        assert!(parse_four_ds(&bytes).is_err());
        assert!(parse_four_ds_skin(&bytes).is_err());
        assert!(parse_four_ds_scene(&bytes).is_err());
        assert!(parse_four_ds_singlemorph(&super::mesh_tests::mesh()).is_err());
    }
    #[test]
    fn zero_targets_and_zero_channels_have_exact_consumption() {
        let mut bytes = prefix();
        bytes.push(0);
        finish(&mut bytes);
        let model = parse_four_ds_singlemorph(&bytes).unwrap();
        let morph = model.frames[0].morph.as_ref().unwrap();
        assert_eq!(morph.target_count, 0);
        assert!(morph.definition.is_none());
        assert_eq!(model.frames[1].name_bytes, b"Tail");
        let mut bytes = prefix();
        bytes.extend([3, 0, 0]);
        bytes.extend([0xff; 40]);
        finish(&mut bytes);
        let model = parse_four_ds_singlemorph(&bytes).unwrap();
        let definition = model.frames[0]
            .morph
            .as_ref()
            .unwrap()
            .definition
            .as_ref()
            .unwrap();
        assert!(definition.lods.is_empty());
        assert_eq!(definition.trailer_words, [u32::MAX; 10]);
        assert_eq!(model.frames[1].name_bytes, b"Tail");
    }
    #[test]
    fn target_suffix_preserves_raw_flags_references_and_following_frame() {
        for references in [vec![], vec![0u16, u16::MAX, 11]] {
            let mut bytes = prefix();
            bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
            bytes.push(0); // Zero-target morph suffix on the preceding frame.
            common(&mut bytes, 7, 1, b"Target");
            bytes.extend(0x81ffu16.to_le_bytes());
            bytes.push(references.len() as u8);
            for reference in &references {
                bytes.extend(reference.to_le_bytes());
            }
            finish(&mut bytes);
            let model = parse_four_ds_singlemorph(&bytes).unwrap();
            let target = model.frames[1].target.as_ref().unwrap();
            assert_eq!(target.flags, 0x81ff);
            assert_eq!(target.frame_indices, references);
            assert_eq!(model.frames[2].name_bytes, b"Tail");
            assert!(model.frames[2].target.is_none());
            for length in 0..bytes.len() {
                assert!(
                    parse_four_ds_singlemorph(&bytes[..length]).is_err(),
                    "{length}"
                );
            }
        }
    }
    #[test]
    fn zero_vertex_channels_have_an_aggregate_metadata_quota_across_frames_and_lods() {
        fn model(frame_count: u16) -> Vec<u8> {
            let mut bytes = prefix()[..18].to_vec();
            bytes[16..18].copy_from_slice(&frame_count.to_le_bytes());
            for _ in 0..frame_count {
                common(&mut bytes, 1, 0, b"Empty");
                bytes.extend(0u16.to_le_bytes());
                bytes.push(16);
                for _ in 0..16 {
                    bytes.extend(0f32.to_le_bytes());
                    bytes.extend(0u16.to_le_bytes());
                    bytes.push(0);
                }
                bytes.extend([0; 16]); // Zero-bone inherited skin in every LOD.
                bytes.extend([1, 255, 16]);
                bytes.extend([0; 16 * 255 * 2]); // Zero-vertex morph channels.
                bytes.extend([0; 40]);
            }
            bytes.push(0);
            bytes
        }
        // Each frame has 4,080 empty channels and no sample or index buffers.
        // The threshold must count all LODs and all frames rather than samples.
        assert_eq!(
            parse_four_ds_singlemorph(&model(24)).unwrap().frames.len(),
            24
        );
        let amplified = model(25);
        assert!(amplified.len() < 256 * 1024);
        assert!(
            parse_four_ds_singlemorph(&amplified)
                .unwrap_err()
                .0
                .contains("morph channel quota")
        );
    }
    #[test]
    fn malformed_morph_extents_indices_and_static_gates_fail() {
        let (bytes, morph_offset, index_offset) = fixture();
        for length in 0..bytes.len() {
            assert!(
                parse_four_ds_singlemorph(&bytes[..length]).is_err(),
                "{length}"
            );
        }
        let mut invalid = bytes.clone();
        invalid[index_offset..index_offset + 2].copy_from_slice(&1u16.to_le_bytes());
        assert!(
            parse_four_ds_singlemorph(&invalid)
                .unwrap_err()
                .0
                .contains("morph index")
        );
        let mut invalid = bytes.clone();
        invalid[morph_offset + 2] = 3;
        assert!(
            parse_four_ds_singlemorph(&invalid)
                .unwrap_err()
                .0
                .contains("morph LOD")
        );
        let mut invalid = prefix();
        invalid.extend([255, 1, 1]);
        invalid.extend(u16::MAX.to_le_bytes());
        assert!(
            parse_four_ds_singlemorph(&invalid)
                .unwrap_err()
                .0
                .contains("morph sample quota")
        );
        let mut invalid = bytes.clone();
        *invalid.last_mut().unwrap() = 1;
        assert!(
            parse_four_ds_singlemorph(&invalid)
                .unwrap_err()
                .0
                .contains("animated model")
        );
        let mut invalid = bytes;
        invalid.push(0);
        assert!(parse_four_ds_singlemorph(&invalid).is_err());
        let mut instance = super::mesh_tests::mesh();
        instance[64] = 3;
        instance[112..114].copy_from_slice(&1u16.to_le_bytes());
        assert!(
            parse_four_ds_singlemorph(&instance)
                .unwrap_err()
                .0
                .contains("singlemesh instance")
        );
    }
}

#[cfg(test)]
mod animation_archive_tests {
    use super::*;
    fn xor(bytes: &mut [u8], key: &[u8; 8]) {
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte ^= key[i % 8];
        }
    }
    fn archive(content: &[u8], k1: u32, k2: u32) -> Vec<u8> {
        let mut key = [0; 8];
        key[..4].copy_from_slice(&k2.to_le_bytes());
        key[4..].copy_from_slice(&k1.to_le_bytes());
        let name = b"Anims\\Synthetic.5DS\0";
        let data_offset = 20 + 32 + name.len();
        let payload_length = content.len() + 1;
        let mut bytes = vec![0; 20];
        bytes[..4].copy_from_slice(b"ISD0");
        let mut record = vec![0; 32];
        for (offset, word) in [
            (0, (32 + name.len() + 4 + payload_length) as u32),
            (4, 32),
            (16, content.len() as u32),
            (20, 1),
        ] {
            record[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        }
        record[24..26].copy_from_slice(&(name.len() as u16 | 0x8000).to_le_bytes());
        xor(&mut record, &key);
        bytes.extend(record);
        let mut encoded_name = name.to_vec();
        xor(&mut encoded_name, &key);
        bytes.extend(encoded_name);
        bytes.extend((payload_length as u32).to_le_bytes());
        let mut payload = vec![0];
        payload.extend(content);
        xor(&mut payload, &key);
        bytes.extend(payload);
        let table_offset = bytes.len();
        let mut row = vec![0; 28];
        let upper = name[..name.len() - 1].to_ascii_uppercase();
        let hash = ((upper.len() as u32) << 16) | upper.iter().map(|&v| v as u32).sum::<u32>();
        row[..4].copy_from_slice(&hash.to_le_bytes());
        let suffix = &upper[upper.len().saturating_sub(16)..];
        row[12..12 + suffix.len()].copy_from_slice(suffix);
        row[4..8].copy_from_slice(&20u32.to_le_bytes());
        row[8..12].copy_from_slice(&(data_offset as u32).to_le_bytes());
        xor(&mut row, &key);
        bytes.extend(row);
        let mut header = Vec::new();
        for word in [1, table_offset as u32, 28, 0] {
            header.extend(word.to_le_bytes());
        }
        xor(&mut header, &key);
        bytes[4..20].copy_from_slice(&header);
        bytes
    }
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn animation_archives_preserve_native_precedence_and_archive_first_lookup() {
        let path = std::env::temp_dir().join(format!("lhr-ac-registration-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let directory = Directory(path);
        fs::create_dir(directory.0.join("anims")).unwrap();
        fs::write(directory.0.join("anims/synthetic.5ds"), b"loose").unwrap();
        for (name, data, k1, k2) in [
            ("A1.dta", b"A1", 0x3d98766c, 0xde7009cd),
            ("aC.DtA", b"AC", 0x43876fea, 0x900cdba8),
            ("a4.DtA", b"A4", 0x43876fea, 0x900cdba8),
            ("AA.dta", b"AA", 0x5342760c, 0xedeac652),
        ] {
            fs::write(directory.0.join(name), archive(data, k1, k2)).unwrap();
        }
        let read = || {
            Installation::open(&directory.0)
                .unwrap()
                .read("ANIMS/SYNTHETIC.5DS")
                .unwrap()
        };
        assert_eq!(read(), b"A1");
        fs::remove_file(directory.0.join("A1.dta")).unwrap();
        assert_eq!(read(), b"AC");
        fs::copy(directory.0.join("aC.DtA"), directory.0.join("AC.dta")).unwrap();
        assert!(
            Installation::open(&directory.0)
                .err()
                .unwrap()
                .0
                .contains("duplicate archive")
        );
        fs::remove_file(directory.0.join("AC.dta")).unwrap();
        fs::remove_file(directory.0.join("aC.DtA")).unwrap();
        assert_eq!(read(), b"A4");
        fs::remove_file(directory.0.join("a4.DtA")).unwrap();
        assert_eq!(read(), b"AA");
        fs::remove_file(directory.0.join("AA.dta")).unwrap();
        assert_eq!(read(), b"loose");
    }

    #[test]
    fn original_windows_paths_share_archive_precedence_and_keep_host_path_checks() {
        let path = std::env::temp_dir().join(format!("lhr-original-path-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let directory = Directory(path);
        fs::create_dir(directory.0.join("anims")).unwrap();
        fs::write(directory.0.join("anims/synthetic.5ds"), b"loose").unwrap();
        fs::write(
            directory.0.join("a4.dta"),
            archive(b"original", 0x43876fea, 0x900cdba8),
        )
        .unwrap();
        let installation = Installation::open(&directory.0).unwrap();
        let original = r"ANIMS\SYNTHETIC.5DS";
        assert_eq!(
            installation.read_original_path(original).unwrap(),
            b"original"
        );
        assert!(installation.read(original).is_err());
        for invalid in [
            r"anims\..\outside",
            r"C:\outside",
            r"\outside",
            "anims/é.5ds",
        ] {
            assert!(
                installation.read_original_path(invalid).is_err(),
                "{invalid}"
            );
        }
        drop(installation);
        fs::remove_file(directory.0.join("a4.dta")).unwrap();
        let installation = Installation::open(&directory.0).unwrap();
        assert_eq!(installation.read_original_path(original).unwrap(), b"loose");
    }

    #[test]
    fn a0_audio_archive_key_and_precedence_preserve_selected_source_errors() {
        let path = std::env::temp_dir().join(format!("lhr-a0-registration-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let directory = Directory(path);
        for (name, data, k1, k2) in [
            ("A6.dta", b"A6", 0x64cd8d0a, 0x4bc97b2d),
            ("a0.DTA", b"A0", 0xd8d0a975, 0x467acde0),
            ("A1.dta", b"A1", 0x3d98766c, 0xde7009cd),
        ] {
            fs::write(directory.0.join(name), archive(data, k1, k2)).unwrap();
        }
        let read = || {
            Installation::open(&directory.0)
                .unwrap()
                .read("ANIMS/SYNTHETIC.5DS")
        };
        assert_eq!(read().unwrap(), b"A6");
        fs::remove_file(directory.0.join("A6.dta")).unwrap();
        assert_eq!(read().unwrap(), b"A0");
        let mut corrupted = archive(b"A0", 0xd8d0a975, 0x467acde0);
        let data_offset = 20 + 32 + b"Anims\\Synthetic.5DS\0".len();
        corrupted[data_offset + 4] = 0xff;
        fs::write(directory.0.join("a0.DTA"), corrupted).unwrap();
        assert!(read().unwrap_err().0.contains("selected archive read"));
        fs::remove_file(directory.0.join("a0.DTA")).unwrap();
        assert_eq!(read().unwrap(), b"A1");
    }
}
