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
//! [`CollisionLevelScene`] calls on the caller's live owners.
//!
//! Dynamic objects are not inserted into this owner: `DynInsert`/`DynErase`
//! run over the vehicle-memory backend in `dynamic_collision_manager`. The
//! free and pending cell lists of this owner are therefore always empty and
//! their native clears are vacuous here. No query is connected yet.
#![forbid(unsafe_code)]

use crate::{
    collision_prepare::{
        classify_triangle, load_triangle, CollisionPreparationError, PreparedCollisionTriangle,
        TrianglePreparationSource,
    },
    frame_transform::FrameKey,
    script_scheduler::ActorReferenceId,
};
use lhr_assets::klz::{Klz, PrimitiveData, PrimitiveId, PrimitiveKind};

/// tree.klz header magic `'GifC'` (g_c_glob.h:150), compared at 0x5c2bc0.
pub const KLZ_MAGIC: u32 = 0x4366_6947;
/// Link flag: the frame must be a visual and is put into world space.
pub const KLZF_VISUAL: u32 = 0x1;
/// Face frame index selecting the level's own vertex pool (0x5c2f70).
pub const KLZ_LEVEL_VERTICES: u16 = 0x7fff;
/// `I3D_frame::GetType()` of a visual (frame +0x110 compared at 0x5c2cdf).
pub const FRAME_VISUAL: u32 = 1;
/// The whole-word property store of a disabled face (0x5c322a).
pub const VOLF_DISABLED: u32 = 0x20;
const HEADER_BYTES: usize = 0x18;

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
    /// without mesh (+0x1e0), otherwise the positions of
    /// `GetMesh()->GetLOD(0)->LockVertices(1)` in original vertex order.
    fn lock_lod0_vertices(&mut self, frame: FrameKey)
        -> Result<Option<Vec<[f32; 3]>>, Self::Error>;
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
/// owner failure or source data the native loader would read out of bounds.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadCollisionError<E> {
    Scene {
        stage: LoadCollisionStage,
        error: E,
    },
    /// Fewer than 0x18 header bytes, or a data offset beyond the file.
    TruncatedHeader,
    /// Structural rejection by the lhr-assets KLZ reader.
    Source(String),
    /// A face index outside its frame's locked LOD0 vertices.
    FrameVertexIndex {
        triangle: usize,
    },
    /// A face whose host arithmetic left the evidenced numeric domain.
    Numeric {
        triangle: usize,
        error: CollisionPreparationError,
    },
    /// A non-face volume without the 0x80/0x20 skip bits in a cell: native
    /// `GrantCoherence` would classify it as a face.
    NonFaceClassification {
        primitive: PrimitiveId,
    },
    /// The moving objects' grid dimensions are outside the host domain.
    DynamicGrid,
}

/// The grid header of the level (tCollisionLevel +0x0..+0x24).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionLevelGrid {
    pub min_x: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_z: f32,
    pub cell_size_x: f32,
    pub cell_size_z: f32,
    pub width: usize,
    pub height: usize,
    pub dyn_cell_size: f32,
}

/// Where a face's vertex words point after loading (0x5c2f6c..0x5c3231).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriangleVertices {
    /// The level's own pool.
    Level([u16; 3]),
    /// The frame link's locked LOD0 vertices.
    Frame { link: u16, indices: [u16; 3] },
    /// Disabled face whose frame had no vertices; the words stay unbound.
    Unbound { link: u16, indices: [u16; 3] },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelTriangle {
    pub properties: u32,
    pub stored_plane: [f32; 4],
    pub vertices: TriangleVertices,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelVolume {
    pub primitive: PrimitiveId,
    pub properties: u32,
    /// The link's frame after resolution; native null stays `None`.
    pub frame: Option<FrameKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LevelCell {
    pub max_y: f32,
    /// The cell's volume references in source order.
    pub references: Vec<PrimitiveId>,
}

/// The installed level. `frame_vertices` holds each link's read-only LOD0
/// lock, which the native code keeps until the level is freed.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionLevel {
    pub grid: CollisionLevelGrid,
    pub frames: Vec<Option<FrameKey>>,
    pub frame_vertices: Vec<Option<Vec<[f32; 3]>>>,
    pub level_vertices: Vec<[f32; 3]>,
    pub triangles: Vec<LevelTriangle>,
    pub volumes: Vec<LevelVolume>,
    pub cells: Vec<LevelCell>,
}

impl CollisionLevel {
    /// The face's current corner positions read through its vertex words.
    pub fn triangle_positions(&self, index: usize) -> Option<[[f32; 3]; 3]> {
        let triangle = self.triangles.get(index)?;
        let (pool, indices) = match triangle.vertices {
            TriangleVertices::Level(indices) => (self.level_vertices.as_slice(), indices),
            TriangleVertices::Frame { link, indices } => (
                self.frame_vertices.get(usize::from(link))?.as_deref()?,
                indices,
            ),
            TriangleVertices::Unbound { .. } => return None,
        };
        Some([
            *pool.get(usize::from(indices[0]))?,
            *pool.get(usize::from(indices[1]))?,
            *pool.get(usize::from(indices[2]))?,
        ])
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
    /// empty. `level_size` is stored before link resolution and the level
    /// is published only after every link was resolved; a stop in between
    /// keeps the reached scene effects and leaves `level` empty.
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
        let Some(bytes) = scene
            .read_source(name)
            .map_err(fail(LoadCollisionStage::ReadSource))?
        else {
            return Ok(LoadCollisionOutcome::SourceAbsent);
        };
        if bytes.len() < HEADER_BYTES {
            return Err(LoadCollisionError::TruncatedHeader);
        }
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        if word(0) != KLZ_MAGIC {
            return Ok(LoadCollisionOutcome::MagicMismatch);
        }
        let data_offset = word(8) as usize;
        let level_size = bytes
            .len()
            .checked_sub(data_offset)
            .and_then(|size| u32::try_from(size).ok())
            .ok_or(LoadCollisionError::TruncatedHeader)?;
        self.level_size = level_size;
        let klz =
            Klz::parse(&bytes).map_err(|error| LoadCollisionError::Source(error.to_string()))?;
        self.progress.links = klz.links.len();

        // 0x5c2c92..0x5c2d0b: backwards; the lookup overwrites the flags
        // word, a non-visual result of a visual link is dropped, a surviving
        // visual result is put into world space.
        let mut frames = vec![None; klz.links.len()];
        for index in (0..klz.links.len()).rev() {
            let link = &klz.links[index];
            let name = &link.raw[4..];
            let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
            let mut frame = scene
                .find_frame_fast(name)
                .map_err(fail(LoadCollisionStage::FindFrameFast { link: index }))?;
            if link.kind & KLZF_VISUAL != 0 {
                if let Some(found) = frame {
                    let kind = scene
                        .frame_type(found)
                        .map_err(fail(LoadCollisionStage::FrameType { link: index }))?;
                    if kind != FRAME_VISUAL {
                        frame = None;
                    }
                }
                if let Some(found) = frame {
                    self.progress.set_space_calls += 1;
                    scene
                        .set_visual_space_world(found)
                        .map_err(fail(LoadCollisionStage::SetSpace { link: index }))?;
                }
            }
            frames[index] = frame;
            self.progress.links_visited += 1;
            self.progress.links_resolved += usize::from(frame.is_some());
        }
        self.loaded_level = true;

        // 0x5c2e05..0x5c2efb: AABB, XTOBB, cylinder, OBB, sphere frames.
        let mut volumes = Vec::new();
        for kind in [
            PrimitiveKind::Aabb,
            PrimitiveKind::XtObb,
            PrimitiveKind::Cylinder,
            PrimitiveKind::Obb,
            PrimitiveKind::Sphere,
        ] {
            for record in klz.array(kind).iter() {
                let link = match record.data() {
                    PrimitiveData::Aabb { link, .. }
                    | PrimitiveData::XtObb { link, .. }
                    | PrimitiveData::Cylinder { link, .. }
                    | PrimitiveData::Obb { link, .. }
                    | PrimitiveData::Sphere { link, .. } => link as usize,
                    PrimitiveData::Triangle { .. } => unreachable!("volume array"),
                };
                volumes.push(LevelVolume {
                    primitive: PrimitiveId {
                        kind,
                        index: record.index,
                    },
                    properties: record.properties().word(),
                    // The reader bounds every volume link.
                    frame: frames[link],
                });
            }
        }

        // 0x5c2f05..0x5c2f4e: backwards; a type-1 frame's LOD0 lock, null
        // otherwise. The type is reread from the live frame.
        let mut frame_vertices = vec![None; frames.len()];
        for index in (0..frames.len()).rev() {
            let Some(frame) = frames[index] else {
                continue;
            };
            if scene
                .frame_type(frame)
                .map_err(fail(LoadCollisionStage::FrameType { link: index }))?
                != FRAME_VISUAL
            {
                continue;
            }
            frame_vertices[index] = scene
                .lock_lod0_vertices(frame)
                .map_err(fail(LoadCollisionStage::LockLod0 { link: index }))?;
        }

        // 0x5c2f6c..0x5c3248: faces bound to their vertices; a frame face
        // without vertices, or with near-coincident corners, is disabled.
        let level_vertices: Vec<[f32; 3]> = (0..klz.world_vertices.len())
            .map(|i| klz.world_vertices.get(i).expect("indexed vertex"))
            .collect();
        let mut triangles = Vec::with_capacity(klz.array(PrimitiveKind::Triangle).len());
        for record in klz.array(PrimitiveKind::Triangle).iter() {
            let PrimitiveData::Triangle {
                vertices,
                stored_plane,
            } = record.data()
            else {
                unreachable!("triangle array");
            };
            let indices = vertices.map(|vertex| vertex.index);
            let link = vertices[0].link;
            let mut properties = record.properties().word();
            let vertices = if link == KLZ_LEVEL_VERTICES {
                TriangleVertices::Level(indices)
            } else if let Some(pool) = frame_vertices[usize::from(link)].as_deref() {
                let corners = indices
                    .iter()
                    .map(|&i| pool.get(usize::from(i)).copied())
                    .collect::<Option<Vec<_>>>()
                    .ok_or(LoadCollisionError::FrameVertexIndex {
                        triangle: record.index,
                    })?;
                let loaded = load_triangle(
                    properties,
                    [corners[0], corners[1], corners[2]],
                    stored_plane,
                    TrianglePreparationSource::Frame,
                )
                .map_err(|error| LoadCollisionError::Numeric {
                    triangle: record.index,
                    error,
                })?;
                properties = loaded.properties;
                TriangleVertices::Frame { link, indices }
            } else {
                properties = VOLF_DISABLED;
                TriangleVertices::Unbound { link, indices }
            };
            triangles.push(LevelTriangle {
                properties,
                stored_plane,
                vertices,
            });
        }

        // 0x5c324e..0x5c33a9: the cell table, row by row.
        let cells: Vec<LevelCell> = klz
            .cells
            .iter()
            .map(|cell| LevelCell {
                max_y: cell.max_y(),
                references: cell.references().map(|r| r.primitive).collect(),
            })
            .collect();
        let grid = CollisionLevelGrid {
            min_x: klz.grid.min_x,
            min_z: klz.grid.min_z,
            max_x: klz.grid.max_x,
            max_z: klz.grid.max_z,
            cell_size_x: klz.grid.cell_width,
            cell_size_z: klz.grid.cell_length,
            width: klz.grid.width,
            height: klz.grid.length,
            dyn_cell_size: f32::from_le_bytes(klz.grid.raw[0x20..0x24].try_into().unwrap()),
        };
        let mut level = CollisionLevel {
            grid,
            frames,
            frame_vertices,
            level_vertices,
            triangles,
            volumes,
            cells,
        };

        // 0x5c33d3..0x5c3548, only with `dyn`.
        let dyn_grid = if dynamic {
            Some(dynamic_grid(&level.grid).ok_or(LoadCollisionError::DynamicGrid)?)
        } else {
            None
        };
        // 0x5c3554: GrantCoherence (GAME 0x5c2980), the classifier over each
        // cell's references in row-major order.
        grant_coherence(&mut level)?;
        self.level = Some(level);
        self.dyn_grid = dyn_grid;
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

fn grant_coherence<E>(level: &mut CollisionLevel) -> Result<(), LoadCollisionError<E>> {
    // `volumes` holds each kind's array contiguously in source index order.
    let volume_index = |primitive: PrimitiveId| {
        level
            .volumes
            .iter()
            .position(|volume| volume.primitive.kind == primitive.kind)
            .map(|start| start + primitive.index)
    };
    let starts: Vec<(PrimitiveKind, Option<usize>)> = PrimitiveKind::ALL
        .into_iter()
        .map(|kind| (kind, volume_index(PrimitiveId { kind, index: 0 })))
        .collect();
    for cell in 0..level.cells.len() {
        for reference in 0..level.cells[cell].references.len() {
            let primitive = level.cells[cell].references[reference];
            if primitive.kind != PrimitiveKind::Triangle {
                let start = starts
                    .iter()
                    .find(|(kind, _)| *kind == primitive.kind)
                    .and_then(|(_, start)| *start)
                    .expect("referenced volume array");
                let volume = &level.volumes[start + primitive.index];
                if volume.properties & 0xa0 == 0 {
                    return Err(LoadCollisionError::NonFaceClassification { primitive });
                }
                continue;
            }
            let Some(positions) = level.triangle_positions(primitive.index) else {
                // An unbound face is disabled; native skips it before reading.
                continue;
            };
            let triangle = &mut level.triangles[primitive.index];
            let mut prepared = PreparedCollisionTriangle {
                properties: triangle.properties,
                vertices: positions,
                stored_plane: triangle.stored_plane,
            };
            classify_triangle(&mut prepared).map_err(|error| LoadCollisionError::Numeric {
                triangle: primitive.index,
                error,
            })?;
            triangle.properties = prepared.properties;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "game_collision_tests.rs"]
pub(crate) mod tests;
