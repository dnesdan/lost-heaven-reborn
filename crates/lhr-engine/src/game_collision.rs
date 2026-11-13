//! Session owner of the `g_collision` level state (`gifCollision`, GAME
//! 0x647f48; mafia-re `fe7e746` game/collision/g_colls.h:178-247 and
//! g_c_glob.cpp). It holds what `LoadCollision` installs and
//! `DeleteCollsLevel` frees: the level (`level`, +0x284), its byte count
//! (`level_size`, +0x288), the moving objects' grid (`dyn_grid`, +0x28c),
//! `step_actor` (+0x2c4) and the never-cleared `LoadedLevel` global
//! (0x648238).
//!
//! Entries (Game.exe SHA-256 08398317…16ab27):
//! - `g_collision::DeleteCollsLevel` GAME 0x5c28d0 (g_c_glob.cpp:23-41).
//! - `g_collision::LoadCollision` GAME 0x5c2b70 (g_c_glob.cpp:123-317),
//!   instruction walk 0x5c2b70..0x5c3561 (REA ev_f037ca63).
//!
//! The per-link virtual at 0x5c2d04 is vtable +0x50 with argument 1. In the
//! LS3DF vtable order (mafia-re ls3df/I3D/I3D.h:345-368, 2745) that slot is
//! the visual's `SetSpace(true)`, which bakes the visual's vertices into
//! world space. The LOD0 lock at 0x5c2f34/0x5c2f3c is the mesh's `GetLOD(0)`
//! then `LockVertices(1)`, read-only and never released. Both are
//! [`CollisionLevelScene`] calls on the caller's live owners; the lock hands
//! back the mesh's own buffer ([`LodVertexBuffer`]), which the bound faces
//! keep and read when the native code dereferences their vertex pointers.
//!
//! The loader reads the source the way the native code does: the header,
//! link table and names before the links, then each part of the level data
//! at the step that first reads it. Nothing is validated ahead of the step
//! that reaches it, so an earlier scene effect (a `SetSpace`, a lock) is never
//! suppressed by a defect further on. The structural audit reader
//! (`lhr_assets::klz`) is a separate host intake and is not consulted here.
//!
//! Dynamic objects are not inserted into this owner: `DynInsert`/`DynErase`
//! run over the vehicle-memory backend in `dynamic_collision_manager`. The
//! free and pending cell lists of this owner are therefore always empty and
//! their native clears are vacuous here. No query is connected yet.
#![forbid(unsafe_code)]

use std::{cell::RefCell, collections::BTreeMap, fmt, rc::Rc};

use crate::{
    collision_prepare::{
        classify_triangle, load_triangle, CollisionPreparationError, PreparedCollisionTriangle,
        TrianglePreparationSource,
    },
    frame_transform::FrameKey,
    script_scheduler::ActorReferenceId,
};
use lhr_assets::klz::{PrimitiveId, PrimitiveKind};

/// tree.klz header magic `'GifC'` (g_c_glob.h:150), compared at 0x5c2bc0.
/// The version word is not compared.
pub const KLZ_MAGIC: u32 = 0x4366_6947;
/// Link flag: the frame must be a visual and is put into world space.
pub const KLZF_VISUAL: u32 = 0x1;
/// Face frame index selecting the level's own vertex pool (0x5c2f70).
pub const KLZ_LEVEL_VERTICES: u16 = 0x7fff;
/// `I3D_frame::GetType()` of a visual (frame +0x110 compared at 0x5c2cdf).
pub const FRAME_VISUAL: u32 = 1;
/// The whole-word property store of a disabled face (0x5c322a).
pub const VOLF_DISABLED: u32 = 0x20;
/// `VOLF_NONFACE | VOLF_DISABLED`: GrantCoherence's skip test (g_c_glob.cpp:57).
const VOLF_SKIP_COHERENCE: u32 = 0xa0;
/// `S_klz_header` (g_c_glob.h:152-159).
const HEADER_BYTES: usize = 0x18;
/// `tCollisionLevel` (g_c_glob.h:122-147), the level data's own header.
const LEVEL_HEADER_BYTES: usize = 0x68;
/// `tCollTreeLevel` (g_c_glob.h:109-116).
const CELL_HEADER_BYTES: i64 = 0x10;
/// The non-face arrays in file order, after the faces (g_c_glob.cpp:170-180),
/// with their count words in `tCollisionLevel`.
const VOLUME_ARRAYS: [(PrimitiveKind, usize); 5] = [
    (PrimitiveKind::Aabb, 0x40),
    (PrimitiveKind::XtObb, 0x38),
    (PrimitiveKind::Cylinder, 0x58),
    (PrimitiveKind::Obb, 0x50),
    (PrimitiveKind::Sphere, 0x48),
];
const FACE_COUNT: usize = 0x30;
const VERTEX_COUNT: usize = 0x60;
/// Host bound on the cell table (the dyn grid's bound as well).
const MAX_CELLS: i64 = 1 << 24;

/// A mesh LOD0 vertex buffer as its owner shares it: `LockVertices(1)`
/// returns a pointer into the live vertex storage, not a copy. This is a view
/// of that storage: the shared bytes, the level's first byte, the vertex
/// stride and the level's vertex count. Clones are the same view; equality is
/// identity (same storage and first byte). Reads return the current contents,
/// so a later write by the owner (a `SetSpace` bake, a deform) is seen.
#[derive(Clone)]
pub struct LodVertexBuffer {
    bytes: Rc<RefCell<Vec<u8>>>,
    base: usize,
    stride: usize,
    count: usize,
}

impl LodVertexBuffer {
    /// A standalone buffer of packed positions (12-byte stride).
    pub fn new(positions: Vec<[f32; 3]>) -> Self {
        let count = positions.len();
        let bytes = positions
            .into_iter()
            .flatten()
            .flat_map(f32::to_le_bytes)
            .collect();
        Self {
            bytes: Rc::new(RefCell::new(bytes)),
            base: 0,
            stride: 12,
            count,
        }
    }
    /// A view of `count` vertices of `stride` bytes at `base` in the owner's
    /// live storage; the position is each vertex's first three LE `f32`
    /// (`I3D_vertex::pos`). `None` for a stride shorter than a position.
    pub(crate) fn live(
        bytes: Rc<RefCell<Vec<u8>>>,
        base: usize,
        stride: usize,
        count: usize,
    ) -> Option<Self> {
        (stride >= 12).then_some(Self {
            bytes,
            base,
            stride,
            count,
        })
    }
    fn at(&self, index: u16) -> Option<usize> {
        let index = usize::from(index);
        (index < self.count)
            .then(|| index.checked_mul(self.stride)?.checked_add(self.base))
            .flatten()
    }
    /// The vertex's current position (`I3D_vertex::pos`); `None` outside the
    /// level or when the storage no longer reaches it.
    pub fn position(&self, index: u16) -> Option<[f32; 3]> {
        let at = self.at(index)?;
        let bytes = self.bytes.borrow();
        let word = |i: usize| {
            bytes
                .get(at + 4 * i..at + 4 * i + 4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        };
        Some([word(0)?, word(1)?, word(2)?])
    }
    /// The owner's write of a vertex position; false outside the buffer.
    pub fn set_position(&self, index: u16, position: [f32; 3]) -> bool {
        let Some(at) = self.at(index) else {
            return false;
        };
        let mut bytes = self.bytes.borrow_mut();
        match bytes.get_mut(at..at + 12) {
            Some(slot) => {
                for (axis, value) in position.into_iter().enumerate() {
                    slot[4 * axis..4 * axis + 4].copy_from_slice(&value.to_le_bytes());
                }
                true
            }
            None => false,
        }
    }
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn same_buffer(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.bytes, &other.bytes) && self.base == other.base
    }
}

impl PartialEq for LodVertexBuffer {
    fn eq(&self, other: &Self) -> bool {
        self.same_buffer(other)
    }
}

impl fmt::Debug for LodVertexBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LodVertexBuffer({:p}+{:#x}, {} vertices)",
            Rc::as_ptr(&self.bytes),
            self.base,
            self.count
        )
    }
}

/// The caller's live owners reached by `LoadCollision`, in native call order.
pub trait CollisionLevelScene {
    type Error;
    /// `dtaOpen` plus the whole-file read; `None` when `dtaOpen` fails.
    fn read_source(&mut self, name: &str) -> Result<Option<Vec<u8>>, Self::Error>;
    /// `driver->FindFrameFast(act_mission->scene, name)` (vtable +0x84).
    fn find_frame_fast(&mut self, name: &[u8]) -> Result<Option<FrameKey>, Self::Error>;
    /// Frame type word (+0x110), read directly by the native code.
    fn frame_type(&mut self, frame: FrameKey) -> Result<u32, Self::Error>;
    /// Visual vtable +0x50 with `true`: `SetSpace(true)`.
    fn set_visual_space_world(&mut self, frame: FrameKey) -> Result<(), Self::Error>;
    /// For a type-1 frame: `None` for a mirror (`'MIRR'`, +0x1d4) or a visual
    /// without mesh (+0x1e0), otherwise the mesh's own buffer from
    /// `GetMesh()->GetLOD(0)->LockVertices(1)`, shared with its owner.
    fn lock_lod0_vertices(
        &mut self,
        frame: FrameKey,
    ) -> Result<Option<LodVertexBuffer>, Self::Error>;
}

/// Native early returns of `LoadCollision`; the owner is left cleared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadCollisionOutcome {
    /// `dtaOpen` returned a negative handle (0x5c2b97).
    SourceAbsent,
    /// The header magic was not `'GifC'` (0x5c2bc8).
    MagicMismatch,
    /// The level was installed.
    Installed,
}

/// Where a host-reported stop happened inside `LoadCollision`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadCollisionStage {
    ReadSource,
    FindFrameFast { link: usize },
    FrameType { link: usize },
    SetSpace { link: usize },
    LockLod0 { link: usize },
}

/// Host errors. Native has no error results: these name either a caller
/// owner failure or source data whose native read leaves the buffer it
/// belongs to, or reads a host pointer, at the step that reaches it.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadCollisionError<E> {
    Scene {
        stage: LoadCollisionStage,
        error: E,
    },
    /// Fewer than 0x18 header bytes, or a data offset beyond the file.
    TruncatedHeader,
    /// The link table runs past the file or past the data offset.
    LinkTable,
    /// A link's word or name outside the names block, or an unterminated name.
    LinkRecord {
        link: usize,
    },
    /// A link read that meets a word an earlier link rewrote with a host
    /// pointer (overlapping or shared link records).
    LinkAlias {
        link: usize,
    },
    /// The level data is shorter than its own 0x68-byte header.
    LevelHeader,
    /// A volume record outside the data, or its frame index past the links.
    VolumeRecord {
        primitive: PrimitiveId,
    },
    /// A face's vertex words outside the data.
    FaceRecord {
        triangle: usize,
    },
    /// A face's frame index past the links.
    FaceLink {
        triangle: usize,
    },
    /// A face index outside its frame's locked LOD0 vertices.
    FrameVertexIndex {
        triangle: usize,
    },
    /// A face whose host arithmetic left the evidenced numeric domain.
    Numeric {
        triangle: usize,
        error: CollisionPreparationError,
    },
    /// The cell table's dimensions are outside the host domain.
    CellTable,
    /// A cell's header or volume words outside the data.
    CellRecord {
        cell: usize,
    },
    /// A cell volume word that resolves to no record of its array; native
    /// `GrantCoherence` would dereference it.
    CellReference {
        cell: usize,
        reference: usize,
    },
    /// A level-pool face corner outside the pool, read by `GrantCoherence`.
    LevelVertexIndex {
        triangle: usize,
    },
    /// A non-face volume without the 0x80/0x20 skip bits in a cell: native
    /// `GrantCoherence` would classify it as a face.
    NonFaceClassification {
        primitive: PrimitiveId,
    },
    /// The moving objects' grid dimensions are outside the host domain.
    DynamicGrid,
}

/// The grid header of the level (tCollisionLevel +0x0..+0x24), read from the
/// level data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionLevelGrid {
    pub min_x: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_z: f32,
    pub cell_size_x: f32,
    pub cell_size_z: f32,
    /// Native `int` cell counts.
    pub width: i32,
    pub height: i32,
    pub dyn_cell_size: f32,
}

/// The native steps that have rewritten the published level, in order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum CollisionLevelPhase {
    /// `level = data` (g_c_glob.cpp:164, right after `LoadedLevel` at
    /// 0x5c2d0f): published, arrays not yet set.
    Stored,
    /// The header's array pointers set from the counts.
    ArraysSet,
    /// Every volume's frame index replaced by its link's frame.
    VolumesBound,
    /// Every face's vertex words bound (LOD0 locks taken before this).
    FacesBound,
    /// The cell table built and every cell's volume words resolved.
    CellsBound,
    /// `GrantCoherence` completed: the load is complete.
    Coherent,
}

/// Where a face's vertex words point after loading (0x5c2f6c..0x5c3231).
#[derive(Clone, Debug, PartialEq)]
pub enum TriangleVertices {
    /// The level's own pool.
    Level([u16; 3]),
    /// The frame link's locked LOD0 buffer, read live through `buffer`.
    Frame {
        link: u16,
        indices: [u16; 3],
        buffer: LodVertexBuffer,
    },
    /// Disabled face whose frame had no vertices; the words stay unbound.
    Unbound { link: u16, indices: [u16; 3] },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LevelTriangle {
    /// The face's live flags word (rewritten by binding and coherence).
    pub properties: u32,
    pub vertices: TriangleVertices,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelVolume {
    pub primitive: PrimitiveId,
    pub properties: u32,
    /// The link's frame after resolution; native null stays `None`.
    pub frame: Option<FrameKey>,
}

/// A cell's volume word after the load's tag switch (g_c_glob.cpp:266-288).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellVolume {
    /// A record of its array.
    Primitive(PrimitiveId),
    /// An unknown tag (left as stored), or an offset that does not land on a
    /// record of its array. Unread until something dereferences it.
    Unresolved(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LevelCell {
    pub max_y: f32,
    /// The cell's volume words in source order.
    pub references: Vec<CellVolume>,
}

/// Data offsets of the level's arrays, as the header pointers set them
/// (g_c_glob.cpp:165-181). Signed: native pointer arithmetic over `int`
/// widths may point before the data; such an offset fails when read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LevelLayout {
    faces: i64,
    volumes: [i64; 5],
    vertices: i64,
    cells: i64,
}

/// The level: tree.klz's data block read whole (native `level` is this
/// buffer) and the typed state the load has bound over it so far.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionLevel {
    data: Vec<u8>,
    layout: Option<LevelLayout>,
    phase: CollisionLevelPhase,
    /// Bound faces in array order; a stop during binding keeps the prefix.
    pub triangles: Vec<LevelTriangle>,
    /// Bound volumes: AABBs, XTOBBs, cylinders, OBBs, spheres.
    pub volumes: Vec<LevelVolume>,
    /// Built cells, row by row.
    pub cells: Vec<LevelCell>,
}

impl CollisionLevel {
    fn stored(data: Vec<u8>) -> Self {
        Self {
            data,
            layout: None,
            phase: CollisionLevelPhase::Stored,
            triangles: Vec::new(),
            volumes: Vec::new(),
            cells: Vec::new(),
        }
    }

    pub fn phase(&self) -> CollisionLevelPhase {
        self.phase
    }

    fn bytes(&self, at: i64, len: usize) -> Option<&[u8]> {
        let start = usize::try_from(at).ok()?;
        self.data.get(start..start.checked_add(len)?)
    }
    fn word(&self, at: i64) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(at, 4)?.try_into().ok()?))
    }
    fn float(&self, at: i64) -> Option<f32> {
        self.word(at).map(f32::from_bits)
    }
    fn count(&self, at: usize) -> Option<u32> {
        self.word(at as i64)
    }

    /// The grid header as the data holds it now; `None` while the data is
    /// shorter than its header.
    pub fn grid(&self) -> Option<CollisionLevelGrid> {
        if self.data.len() < LEVEL_HEADER_BYTES {
            return None;
        }
        let f = |at| self.float(at).expect("level header");
        Some(CollisionLevelGrid {
            min_x: f(0x0),
            min_z: f(0x4),
            max_x: f(0x8),
            max_z: f(0xc),
            cell_size_x: f(0x10),
            cell_size_z: f(0x14),
            width: self.word(0x18).expect("level header") as i32,
            height: self.word(0x1c).expect("level header") as i32,
            dyn_cell_size: f(0x20),
        })
    }

    /// The face's stored plane (`normal`, `d`), read from its record.
    pub fn triangle_plane(&self, index: usize) -> Option<[f32; 4]> {
        let raw = self.bytes(self.face_record(index)? + 16, 16)?;
        Some(std::array::from_fn(|i| {
            f32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap())
        }))
    }

    fn face_record(&self, index: usize) -> Option<i64> {
        let faces = self.layout?.faces;
        faces.checked_add(i64::try_from(index).ok()?.checked_mul(32)?)
    }

    /// A level-pool vertex, read from the pool when dereferenced.
    pub fn level_vertex(&self, index: u16) -> Option<[f32; 3]> {
        if u32::from(index) >= self.count(VERTEX_COUNT)? {
            return None;
        }
        let at = self.layout?.vertices + i64::from(index) * 12;
        Some([self.float(at)?, self.float(at + 4)?, self.float(at + 8)?])
    }

    /// One corner of a bound face, read live through its vertex word.
    pub fn triangle_vertex(&self, index: usize, corner: usize) -> Option<[f32; 3]> {
        match &self.triangles.get(index)?.vertices {
            TriangleVertices::Level(indices) => self.level_vertex(*indices.get(corner)?),
            TriangleVertices::Frame {
                indices, buffer, ..
            } => buffer.position(*indices.get(corner)?),
            TriangleVertices::Unbound { .. } => None,
        }
    }

    /// The face's current corner positions read through its vertex words.
    pub fn triangle_positions(&self, index: usize) -> Option<[[f32; 3]; 3]> {
        Some([
            self.triangle_vertex(index, 0)?,
            self.triangle_vertex(index, 1)?,
            self.triangle_vertex(index, 2)?,
        ])
    }

    fn volume_slot(&self, primitive: PrimitiveId) -> Option<usize> {
        let mut start = 0;
        for (kind, count_at) in VOLUME_ARRAYS {
            let count = self.count(count_at)? as usize;
            if kind == primitive.kind {
                return (primitive.index < count).then_some(start + primitive.index);
            }
            start += count;
        }
        None
    }

    /// A cell volume word resolved as g_c_glob.cpp:266-288 does.
    fn resolve_cell_volume(&self, word: u32) -> CellVolume {
        let tag = (word >> 24) as u8;
        let Some(kind) = PrimitiveKind::ALL.into_iter().find(|k| k.tag() == tag) else {
            return CellVolume::Unresolved(word);
        };
        let count_at = if kind == PrimitiveKind::Triangle {
            FACE_COUNT
        } else {
            VOLUME_ARRAYS.iter().find(|(k, _)| *k == kind).unwrap().1
        };
        let offset = (word & 0x00ff_ffff) as usize;
        let count = self.count(count_at).unwrap_or(0) as usize;
        if offset.is_multiple_of(kind.stride()) && offset / kind.stride() < count {
            CellVolume::Primitive(PrimitiveId {
                kind,
                index: offset / kind.stride(),
            })
        } else {
            CellVolume::Unresolved(word)
        }
    }
}

/// The moving objects' grid (S_dyn_grid, g_colls.h:131-140). Every cell is
/// null at load; this owner admits no dynamic objects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicCollisionGrid {
    pub min_x: f32,
    pub min_z: f32,
    pub cell_size_x: f32,
    pub cell_size_z: f32,
    pub width: u32,
    pub height: u32,
}

/// Live counts for owner disposal checks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GameCollisionCounts {
    pub levels: usize,
    pub dynamic_grids: usize,
}

/// Visible progress of the last `LoadCollision`, for diagnostics.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LoadCollisionProgress {
    pub links: usize,
    /// Links whose resolution loop iteration completed (visited backwards).
    pub links_visited: usize,
    pub links_resolved: usize,
    pub set_space_calls: usize,
}

/// A link's first word after the loader rewrote it (`S_klz_frame`'s union,
/// g_c_glob.h:168-177): the flags in the file, then the frame, then the LOD0
/// vertices.
#[derive(Clone, Debug)]
enum LinkWord {
    Frame(FrameKey),
    Vertices(LodVertexBuffer),
    /// Bytes of a pointer word that a later overlapping write cut.
    Fragment,
}

/// The header, link table and names block (`frames`/`names`, freed at the
/// end of the load). Null writes store zero bytes; non-null writes store a
/// typed host pointer that no later read may reinterpret as bytes.
struct LinkBlock {
    bytes: Vec<u8>,
    table_end: usize,
    pointers: BTreeMap<usize, LinkWord>,
}

enum LinkRead {
    Bytes(u32),
    Pointer(LinkWord),
}

impl LinkBlock {
    fn link_offset(&self, link: usize) -> usize {
        let at = HEADER_BYTES + link * 4;
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap()) as usize
    }
    /// Offsets relative to the header made pointers into the names
    /// (g_c_glob.cpp:146-150); anything outside the names block is a host
    /// error.
    fn link_word_at(&self, link: usize) -> Option<usize> {
        let offset = self.link_offset(link);
        (offset >= self.table_end && offset.checked_add(4)? <= self.bytes.len()).then_some(offset)
    }
    fn touches_pointer(&self, start: usize, end: usize) -> bool {
        self.pointers
            .range(start.saturating_sub(3)..end)
            .next()
            .is_some()
    }
    fn read_word(&self, at: usize) -> Option<LinkRead> {
        if let Some(word) = self.pointers.get(&at) {
            return match word {
                LinkWord::Fragment => None,
                word => Some(LinkRead::Pointer(word.clone())),
            };
        }
        if self.touches_pointer(at, at + 4) {
            return None;
        }
        Some(LinkRead::Bytes(u32::from_le_bytes(
            self.bytes[at..at + 4].try_into().unwrap(),
        )))
    }
    fn write_word(&mut self, at: usize, word: Option<LinkWord>) {
        let cut: Vec<usize> = self
            .pointers
            .range(at.saturating_sub(3)..at + 4)
            .map(|(&o, _)| o)
            .filter(|&o| o != at)
            .collect();
        for o in cut {
            self.pointers.insert(o, LinkWord::Fragment);
        }
        match word {
            Some(word) => {
                self.pointers.insert(at, word);
            }
            None => {
                self.pointers.remove(&at);
                self.bytes[at..at + 4].fill(0);
            }
        }
    }
    /// The link's name up to its NUL, inside the names block.
    fn name(&self, at: usize) -> Option<Vec<u8>> {
        let start = at + 4;
        let len = self.bytes.get(start..)?.iter().position(|&b| b == 0)?;
        if self.touches_pointer(start, start + len + 1) {
            return None;
        }
        Some(self.bytes[start..start + len].to_vec())
    }
    /// `frames[link]->frame`, after the links were resolved.
    fn frame(&self, link: usize) -> Option<Option<FrameKey>> {
        match self.read_word(self.link_word_at(link)?)? {
            LinkRead::Bytes(0) => Some(None),
            LinkRead::Pointer(LinkWord::Frame(frame)) => Some(Some(frame)),
            _ => None,
        }
    }
    /// `frames[link]->vertices`, after the LOD0 loop.
    fn vertices(&self, link: usize) -> Option<Option<LodVertexBuffer>> {
        match self.read_word(self.link_word_at(link)?)? {
            LinkRead::Bytes(0) => Some(None),
            LinkRead::Pointer(LinkWord::Vertices(buffer)) => Some(Some(buffer)),
            _ => None,
        }
    }
}

/// `gifCollision`: one per session, starting as the constructor leaves it
/// (GAME 0x5c4fa0: level, dyn_grid and step_actor null).
#[derive(Debug, Default)]
pub struct GameCollision {
    step_actor: Option<ActorReferenceId>,
    level: Option<CollisionLevel>,
    level_size: u32,
    dyn_grid: Option<DynamicCollisionGrid>,
    loaded_level: bool,
    progress: LoadCollisionProgress,
}

impl GameCollision {
    /// The published level, complete or as far as an interrupted load bound
    /// it (see [`CollisionLevel::phase`]).
    pub fn level(&self) -> Option<&CollisionLevel> {
        self.level.as_ref()
    }
    pub fn level_size(&self) -> u32 {
        self.level_size
    }
    pub fn dynamic_grid(&self) -> Option<&DynamicCollisionGrid> {
        self.dyn_grid.as_ref()
    }
    pub fn step_actor(&self) -> Option<ActorReferenceId> {
        self.step_actor
    }
    /// `C_human::MovementIntern` stores its human here around a step test.
    pub fn set_step_actor(&mut self, actor: Option<ActorReferenceId>) {
        self.step_actor = actor;
    }
    /// The `LoadedLevel` global: set by a load past link resolution, never
    /// cleared (not even by `DeleteCollsLevel`).
    pub fn loaded_level(&self) -> bool {
        self.loaded_level
    }
    pub fn load_progress(&self) -> LoadCollisionProgress {
        self.progress
    }
    pub fn counts(&self) -> GameCollisionCounts {
        GameCollisionCounts {
            levels: usize::from(self.level.is_some()),
            dynamic_grids: usize::from(self.dyn_grid.is_some()),
        }
    }

    /// `g_collision::DeleteCollsLevel`, GAME 0x5c28d0: the level and its cell
    /// table freed, then the dynamic grid with its cells, and only in that
    /// arm the free and pending lists emptied. `level_size`, `step_actor` and
    /// `LoadedLevel` are left as they are.
    pub fn delete_colls_level(&mut self) {
        self.level = None;
        // The free/pending clears of the native arm are vacuous here.
        self.dyn_grid = None;
    }

    /// `C_game::Done`'s `gifCollision.GameDone()` (game.cpp:1379; GAME
    /// 0x5d5900 is `DynWipeOut` then `DeleteCollsLevel`). `DynWipeOut` has
    /// nothing to wipe on this owner, which never holds dynamic objects.
    pub fn game_done(&mut self) {
        self.delete_colls_level();
    }

    /// `g_collision::LoadCollision(name, dyn)`, GAME 0x5c2b70, in native
    /// order. `step_actor` is cleared and the previous level deleted before
    /// the source is opened, so an absent or foreign file leaves the owner
    /// empty. `level_size` is stored before link resolution; the level is
    /// published right after it (with `LoadedLevel`), before any of its
    /// arrays is read, and every later step rewrites the published level in
    /// place. A stop keeps the reached scene effects and every completed
    /// step: the level (at its [`CollisionLevel::phase`]) and, once built,
    /// the dynamic grid.
    pub fn load_collision<S: CollisionLevelScene>(
        &mut self,
        name: &str,
        dynamic: bool,
        scene: &mut S,
    ) -> Result<LoadCollisionOutcome, LoadCollisionError<S::Error>> {
        self.step_actor = None;
        self.delete_colls_level();
        self.progress = LoadCollisionProgress::default();
        let fail = |stage| move |error| LoadCollisionError::Scene { stage, error };
        let Some(mut bytes) = scene
            .read_source(name)
            .map_err(fail(LoadCollisionStage::ReadSource))?
        else {
            return Ok(LoadCollisionOutcome::SourceAbsent);
        };
        // dtaRead(0x18) keeps the bytes that exist, so the magic compare at
        // 0x5c2bc0 sees a short file's real first word; only a matching
        // magic goes on to need the whole header.
        if bytes.len() >= 4 && u32::from_le_bytes(bytes[..4].try_into().unwrap()) != KLZ_MAGIC {
            return Ok(LoadCollisionOutcome::MagicMismatch);
        }
        if bytes.len() < HEADER_BYTES {
            return Err(LoadCollisionError::TruncatedHeader);
        }
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let data_offset = word(8) as usize;
        let link_count = word(12) as usize;
        if data_offset > bytes.len() {
            return Err(LoadCollisionError::TruncatedHeader);
        }
        // `frames` (num_frames words) then `names` up to the data offset.
        let table_end = link_count
            .checked_mul(4)
            .and_then(|n| n.checked_add(HEADER_BYTES))
            .filter(|&end| end <= data_offset)
            .ok_or(LoadCollisionError::LinkTable)?;
        // 0x5c2c4e: the data size is stored before the links are resolved.
        self.level_size = u32::try_from(bytes.len() - data_offset)
            .map_err(|_| LoadCollisionError::TruncatedHeader)?;
        let data = bytes.split_off(data_offset);
        let mut links = LinkBlock {
            bytes,
            table_end,
            pointers: BTreeMap::new(),
        };
        self.progress.links = link_count;

        // 0x5c2c92..0x5c2d0b: backwards; the lookup overwrites the flags
        // word, a non-visual result of a visual link is dropped, a surviving
        // visual result is put into world space.
        for index in (0..link_count).rev() {
            let at = links
                .link_word_at(index)
                .ok_or(LoadCollisionError::LinkRecord { link: index })?;
            let flags = match links.read_word(at) {
                Some(LinkRead::Bytes(flags)) => flags,
                // The flags word holds an earlier link's pointer.
                _ => return Err(LoadCollisionError::LinkAlias { link: index }),
            };
            let link_name = match links.name(at) {
                Some(link_name) => link_name,
                None if links.bytes[at + 4..].contains(&0) => {
                    return Err(LoadCollisionError::LinkAlias { link: index })
                }
                None => return Err(LoadCollisionError::LinkRecord { link: index }),
            };
            let mut frame = scene
                .find_frame_fast(&link_name)
                .map_err(fail(LoadCollisionStage::FindFrameFast { link: index }))?;
            links.write_word(at, frame.map(LinkWord::Frame));
            if flags & KLZF_VISUAL != 0 {
                if let Some(found) = frame {
                    let kind = scene
                        .frame_type(found)
                        .map_err(fail(LoadCollisionStage::FrameType { link: index }))?;
                    if kind != FRAME_VISUAL {
                        frame = None;
                        links.write_word(at, None);
                    }
                }
                if let Some(found) = frame {
                    self.progress.set_space_calls += 1;
                    scene
                        .set_visual_space_world(found)
                        .map_err(fail(LoadCollisionStage::SetSpace { link: index }))?;
                }
            }
            self.progress.links_visited += 1;
            self.progress.links_resolved += usize::from(frame.is_some());
        }
        self.loaded_level = true;
        // g_c_glob.cpp:164: `level = data`, published before its arrays are set.
        let level = self.level.insert(CollisionLevel::stored(data));

        // g_c_glob.cpp:165-181: the array pointers from the header's counts.
        let Some(grid) = level.grid() else {
            return Err(LoadCollisionError::LevelHeader);
        };
        let header = |at| i64::from(level.count(at).expect("level header"));
        let faces = LEVEL_HEADER_BYTES as i64
            + 4 * (i64::from(grid.width) + 1)
            + 4 * (i64::from(grid.height) + 2);
        let mut cursor = faces + header(FACE_COUNT) * 32;
        let volumes = VOLUME_ARRAYS.map(|(kind, count_at)| {
            let start = cursor;
            cursor += header(count_at) * kind.stride() as i64;
            start
        });
        let vertices = cursor;
        let cells = vertices + header(VERTEX_COUNT) * 12 + 4;
        level.layout = Some(LevelLayout {
            faces,
            volumes,
            vertices,
            cells,
        });
        level.phase = CollisionLevelPhase::ArraysSet;

        // 0x5c2e05..0x5c2efb: each volume's frame index replaced by the
        // link's frame, AABBs, XTOBBs, cylinders, OBBs, spheres.
        for (array, (kind, count_at)) in VOLUME_ARRAYS.into_iter().enumerate() {
            for index in 0..level.count(count_at).expect("level header") as usize {
                let primitive = PrimitiveId { kind, index };
                let at = volumes[array] + (index * kind.stride()) as i64;
                let (Some(properties), Some(link)) = (level.word(at), level.word(at + 4)) else {
                    return Err(LoadCollisionError::VolumeRecord { primitive });
                };
                let frame = (link as usize)
                    .lt(&link_count)
                    .then(|| links.frame(link as usize))
                    .ok_or(LoadCollisionError::VolumeRecord { primitive })?
                    .ok_or(LoadCollisionError::LinkAlias {
                        link: link as usize,
                    })?;
                level.volumes.push(LevelVolume {
                    primitive,
                    properties,
                    frame,
                });
            }
        }
        level.phase = CollisionLevelPhase::VolumesBound;

        // 0x5c2f05..0x5c2f4e: backwards; a type-1 frame's LOD0 lock, null
        // otherwise. The frame and its type are reread live.
        for index in (0..link_count).rev() {
            let at = links
                .link_word_at(index)
                .ok_or(LoadCollisionError::LinkRecord { link: index })?;
            let frame = match links.read_word(at) {
                Some(LinkRead::Bytes(0)) => None,
                Some(LinkRead::Pointer(LinkWord::Frame(frame))) => Some(frame),
                _ => return Err(LoadCollisionError::LinkAlias { link: index }),
            };
            let mut vertices = None;
            if let Some(frame) = frame {
                if scene
                    .frame_type(frame)
                    .map_err(fail(LoadCollisionStage::FrameType { link: index }))?
                    == FRAME_VISUAL
                {
                    vertices = scene
                        .lock_lod0_vertices(frame)
                        .map_err(fail(LoadCollisionStage::LockLod0 { link: index }))?;
                }
            }
            links.write_word(at, vertices.map(LinkWord::Vertices));
        }

        // 0x5c2f6c..0x5c3248: faces bound to their vertices; a frame face
        // without vertices, or with near-coincident corners, is disabled.
        // Frame corners are read from the live buffer as they are bound.
        for index in 0..level.count(FACE_COUNT).expect("level header") as usize {
            let at = faces + index as i64 * 32;
            let record = level
                .bytes(at, 16)
                .ok_or(LoadCollisionError::FaceRecord { triangle: index })?;
            let w = |i: usize| u32::from_le_bytes(record[i * 4..i * 4 + 4].try_into().unwrap());
            let mut properties = w(0);
            let indices = [w(1), w(2), w(3)].map(|v| v as u16);
            let link = (w(1) >> 16) as u16;
            let vertices = if link == KLZ_LEVEL_VERTICES {
                TriangleVertices::Level(indices)
            } else {
                let buffer = (usize::from(link) < link_count)
                    .then(|| links.vertices(usize::from(link)))
                    .ok_or(LoadCollisionError::FaceLink { triangle: index })?
                    .ok_or(LoadCollisionError::LinkAlias {
                        link: usize::from(link),
                    })?;
                match buffer {
                    Some(buffer) => {
                        let corner = |i: u16| {
                            buffer
                                .position(i)
                                .ok_or(LoadCollisionError::FrameVertexIndex { triangle: index })
                        };
                        let corners = [
                            corner(indices[0])?,
                            corner(indices[1])?,
                            corner(indices[2])?,
                        ];
                        // The plane is carried, not read, by the binding test.
                        properties = load_triangle(
                            properties,
                            corners,
                            [0.0; 4],
                            TrianglePreparationSource::Frame,
                        )
                        .map_err(|error| LoadCollisionError::Numeric {
                            triangle: index,
                            error,
                        })?
                        .properties;
                        TriangleVertices::Frame {
                            link,
                            indices,
                            buffer,
                        }
                    }
                    None => {
                        properties = VOLF_DISABLED;
                        TriangleVertices::Unbound { link, indices }
                    }
                }
            };
            level.triangles.push(LevelTriangle {
                properties,
                vertices,
            });
        }
        level.phase = CollisionLevelPhase::FacesBound;

        // 0x5c324e..0x5c33a9: the cell table, row by row. Non-positive
        // dimensions run no iteration.
        let (width, height) = (i64::from(grid.width), i64::from(grid.height));
        if width > 0 && height > 0 && width.saturating_mul(height) > MAX_CELLS {
            return Err(LoadCollisionError::CellTable);
        }
        let mut cursor = cells;
        for cell in 0..(width.max(0) * height.max(0)) as usize {
            let count = level
                .word(cursor)
                .filter(|_| level.bytes(cursor, CELL_HEADER_BYTES as usize).is_some())
                .ok_or(LoadCollisionError::CellRecord { cell })?;
            let max_y = level.float(cursor + 12).expect("cell header");
            let words = cursor + CELL_HEADER_BYTES;
            let references = (0..i64::from(count))
                .map(|i| {
                    level
                        .word(words + 4 * i)
                        .map(|w| level.resolve_cell_volume(w))
                })
                .collect::<Option<Vec<_>>>()
                .ok_or(LoadCollisionError::CellRecord { cell })?;
            // The words, a byte per volume (not read here), aligned to 4.
            cursor = words + 5 * i64::from(count) + i64::from((4 - (count & 3)) & 3);
            level.cells.push(LevelCell { max_y, references });
        }
        level.phase = CollisionLevelPhase::CellsBound;

        // 0x5c33d3..0x5c3548, only with `dyn`.
        if dynamic {
            self.dyn_grid = Some(dynamic_grid(&grid).ok_or(LoadCollisionError::DynamicGrid)?);
        }
        // 0x5c3554: GrantCoherence (GAME 0x5c2980), the classifier over each
        // cell's references in row-major order.
        grant_coherence(level)?;
        level.phase = CollisionLevelPhase::Coherent;
        Ok(LoadCollisionOutcome::Installed)
    }
}

/// The `dyn` arm's grid: `(int)` truncation of extent / `dyn_cell_size`,
/// an unsigned zero raised to one, then cell sizes divided by the signed
/// counts (FIDIV). Arithmetic is f32 single rounding (x87 PC24).
fn dynamic_grid(level: &CollisionLevelGrid) -> Option<DynamicCollisionGrid> {
    use crate::collision_swept::{div, sub};
    let truncate = |value: f32| -> Option<u32> {
        // (-1, 0) truncates to zero and is raised to one; other negatives
        // would wrap to huge unsigned counts.
        if !(value > -1.0 && value < 2_147_483_648.0) {
            return None;
        }
        let result = value.trunc() as i32;
        crate::native_precision::record_precision(result as f32 != value);
        Some((result as u32).max(1))
    };
    let extent_x = sub(level.max_x, level.min_x).ok()?;
    let extent_z = sub(level.max_z, level.min_z).ok()?;
    let width = truncate(div(extent_x, level.dyn_cell_size).ok()?)?;
    let height = truncate(div(extent_z, level.dyn_cell_size).ok()?)?;
    width.checked_mul(height).filter(|n| *n <= 1 << 24)?;
    Some(DynamicCollisionGrid {
        min_x: level.min_x,
        min_z: level.min_z,
        cell_size_x: div(extent_x, width as i32 as f32).ok()?,
        cell_size_z: div(extent_z, height as i32 as f32).ok()?,
        width,
        height,
    })
}

/// GAME 0x5c2980. Each reference's flags are read first; a skipped face is
/// not read further. A classified face reads its stored normal and its first
/// two corners (the first edge), live, then has its low byte replaced. A
/// stop keeps the faces classified before it.
fn grant_coherence<E>(level: &mut CollisionLevel) -> Result<(), LoadCollisionError<E>> {
    for cell in 0..level.cells.len() {
        for reference in 0..level.cells[cell].references.len() {
            let primitive = match level.cells[cell].references[reference] {
                CellVolume::Primitive(primitive) => primitive,
                CellVolume::Unresolved(_) => {
                    return Err(LoadCollisionError::CellReference { cell, reference })
                }
            };
            if primitive.kind != PrimitiveKind::Triangle {
                let volume = level
                    .volume_slot(primitive)
                    .and_then(|slot| level.volumes.get(slot))
                    .expect("resolved volume");
                if volume.properties & VOLF_SKIP_COHERENCE == 0 {
                    return Err(LoadCollisionError::NonFaceClassification { primitive });
                }
                continue;
            }
            let triangle = primitive.index;
            if level.triangles[triangle].properties & VOLF_SKIP_COHERENCE != 0 {
                continue;
            }
            let stored_plane = level
                .triangle_plane(triangle)
                .ok_or(LoadCollisionError::FaceRecord { triangle })?;
            let corner = |i| {
                level
                    .triangle_vertex(triangle, i)
                    .ok_or(LoadCollisionError::LevelVertexIndex { triangle })
            };
            let (a, b) = (corner(0)?, corner(1)?);
            // The classifier reads only the first edge; the third corner is
            // not dereferenced.
            let mut prepared = PreparedCollisionTriangle {
                properties: level.triangles[triangle].properties,
                vertices: [a, b, b],
                stored_plane,
            };
            classify_triangle(&mut prepared)
                .map_err(|error| LoadCollisionError::Numeric { triangle, error })?;
            level.triangles[triangle].properties = prepared.properties;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "game_collision_tests.rs"]
pub(crate) mod tests;
