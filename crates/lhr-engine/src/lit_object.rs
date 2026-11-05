//! Bounded unconstructed I3D_lit_object resource ownership. Source: pinned
//! mafia-re 29b65b0, I3d_lit_object.cpp constructor/destructor and litLod::Create.
//! Saved-lighting CPU state is separately owned from the shared mesh. Offline
//! Construct/Save and native GPU upload remain separate boundaries.
use super::*;
#[path = "lightmap_atlas.rs"]
pub mod lightmap_atlas;
#[path = "saved_lightmaps.rs"]
pub mod saved_lightmaps;
use lightmap_atlas::LightmapPlacement;
use saved_lightmaps::{SavedLightingLoad, SavedLitLod};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectVisualType {
    Plain,
    LitUnconstructed,
    Singlemesh,
    Singlemorph,
    Morph,
    Billboard,
}
impl ObjectVisualType {
    pub fn source_fourcc(self) -> u32 {
        match self {
            Self::Plain => 0x5f4a424f,            // OBJ_
            Self::LitUnconstructed => 0x50414d4c, // LMAP (not serialized ordinal 1)
            Self::Singlemesh => 0x4d474e53,       // SNGM
            Self::Morph => 0x4850_524d,           // MRPH
            Self::Singlemorph => 0x524d4d53,      // SMMR
            Self::Billboard => 0x4452_4242,       // BBRD
        }
    }
}
#[derive(Debug, PartialEq)]
pub struct LitLevelResources {
    pub constructed: bool,
    pub mode: u32,
    pub resolution: f32,
    pub angle: f32,
    pub lods: [Option<PlainLodKey>; 10],
    /// litLod's independently owned loaded arrays, atlas and index buffer.
    pub saved_lods: [Option<SavedLitLod>; 10],
}
#[derive(Debug, PartialEq)]
pub struct LitObjectResources {
    pub active_level: usize,
    pub levels: [LitLevelResources; 6],
}
impl LitObjectResources {
    fn new() -> Self {
        Self {
            active_level: 0,
            levels: std::array::from_fn(|_| LitLevelResources {
                constructed: false,
                mode: 2,
                resolution: 5.,
                angle: 45.,
                lods: [None; 10],
                saved_lods: std::array::from_fn(|_| None),
            }),
        }
    }
}
impl I3dObjectResources {
    pub fn lit_object(&self) -> Option<&LitObjectResources> {
        self.lit_object.as_deref()
    }
}
impl ObjectResources {
    /// SetActiveLevel frees only the old active lighting's render buffers;
    /// constructed lighting arrays and shared lightmap texture owners survive.
    /// This arena has no native m_PosVB allocation: GPU snapshots own theirs.
    pub fn set_active_lighting<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        level: usize,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        if level >= 6 {
            return Err(ObjectError::InvalidIndex(level as u32).into());
        }
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        let old = lit.active_level;
        let keys = lit.levels[old].lods;
        for key in keys.iter().flatten() {
            let lod = self.lod(*key)?;
            if lod.owner != Some(object) {
                return Err(ObjectError::LodOwned(*key).into());
            }
            if lod.state != LodState::Constructed {
                return Err(ObjectError::LodDestructed(*key).into());
            }
        }
        for key in keys.into_iter().flatten() {
            self.release_lit_render_vertices(key, backend)?;
        }
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .active_level = level;
        Ok(())
    }
    /// Lit SetMesh keeps its independent lightings when requested; unlike the
    /// plain class it does not run the plain LOD compatibility path. Visual
    /// bounds are projected by the frame adapter that owns the same object.
    pub(crate) fn lit_set_object_mesh<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        mesh: Option<MeshObjectKey>,
        keep_lighting: bool,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let record = self.object(object)?;
        if record.lit_object.is_none() {
            return Err(ObjectError::UnsupportedVisualSubtype.into());
        }
        let old = record.mesh;
        if old == mesh {
            return Ok(());
        }
        if let Some(mesh) = mesh {
            self.managed
                .resources()
                .mesh_object(mesh)
                .map_err(ManagedError::from)?;
        }
        let result = (|| {
            if !keep_lighting {
                self.destruct_active_lighting(object, backend)?;
            }
            if let Some(old) = old {
                managed_result(self.managed.release_mesh_object(old, backend))?;
            }
            self.objects[(object.slot - 1) as usize]
                .as_mut()
                .unwrap()
                .mesh = mesh;
            if let Some(mesh) = mesh {
                self.managed.retain_mesh_object(mesh)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Native Destruct(0) clears only the active lighting. Completed inactive
    /// levels remain owned. Effects stop at the first backend failure.
    pub fn destruct_active_lighting<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        let active = lit.active_level;
        let keys = lit.levels[active].lods;
        for key in keys.iter().flatten() {
            let lod = self.lod(*key)?;
            if lod.owner != Some(object) {
                return Err(ObjectError::LodOwned(*key).into());
            }
            if lod.state != LodState::Constructed {
                return Err(ObjectError::LodDestructed(*key).into());
            }
        }
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[active]
            .constructed = false;
        for (i, key) in keys.into_iter().enumerate() {
            if let Some(key) = key {
                self.cleanup_saved_lit_lod(object, active, i, backend)?;
                self.lods[(key.slot - 1) as usize] = None;
            }
            let level = &mut self.objects[(object.slot - 1) as usize]
                .as_mut()
                .unwrap()
                .lit_object
                .as_mut()
                .unwrap()
                .levels[active];
            level.lods[i] = None;
            level.saved_lods[i] = None;
        }
        Ok(())
    }
    /// Duplicate copies construction settings, never the constructed arrays or
    /// active lighting. Ordinary visual scalars go through the single owner
    /// duplicate_object_render_state (all destinations, lit or plain);
    /// lighting-level settings copy here. Divergence: the native Duplicate
    /// runs Destruct(0) on the destination first (mafia-re
    /// I3d_lit_object.cpp:1474); this path performs no dst teardown —
    /// teardown is caller-side (fresh-dst assumption, disclosed).
    pub(crate) fn copy_object_duplicate_state(
        &mut self,
        destination: ObjectResourceKey,
        source: ObjectResourceKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        let src = self.object(source)?;
        let settings = src.lit_object.as_ref().map(|lit| {
            std::array::from_fn::<_, 6, _>(|i| {
                let l = &lit.levels[i];
                (l.mode, l.resolution, l.angle)
            })
        });
        self.object(destination)?;
        // Scalar block has a single owner (duplicate_object_render_state);
        // lighting settings stay here.
        self.duplicate_object_render_state(destination, source)?;
        let dst = self.objects[(destination.slot - 1) as usize]
            .as_mut()
            .unwrap();
        if let Some(lit) = dst.lit_object.as_mut() {
            if let Some(settings) = settings {
                for (level, (mode, resolution, angle)) in lit.levels.iter_mut().zip(settings) {
                    level.mode = mode;
                    level.resolution = resolution;
                    level.angle = angle;
                }
            }
        }
        Ok(())
    }
    /// Transfers completed Load allocations into their actual object owner.
    /// The caller first performs native Destruct on the active lighting. A
    /// failing Load's partial result is installed through this same operation.
    pub fn install_saved_lighting<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        loaded: SavedLightingLoad,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        for (old, new) in lit.levels.iter().zip(&loaded.levels) {
            if let Some(new) = new {
                for (index, lod) in new.lods.iter().enumerate() {
                    if lod.is_some() {
                        if let Some(key) = old.lods[index] {
                            let common = self.lod(key)?;
                            if common.owner != Some(object) {
                                return Err(ObjectError::LodOwned(key).into());
                            }
                            if common.state != LodState::Constructed {
                                return Err(ObjectError::LodDestructed(key).into());
                            }
                        }
                    }
                }
            }
        }
        for (lighting, new) in loaded.levels.into_iter().enumerate() {
            if let Some(new) = new {
                let old = &mut self.objects[(object.slot - 1) as usize]
                    .as_mut()
                    .unwrap()
                    .lit_object
                    .as_mut()
                    .unwrap()
                    .levels[lighting];
                old.mode = new.mode;
                old.resolution = new.resolution;
                old.angle = new.angle;
                // On duplicate saved headers native completed LOD pointers are
                // overwritten. Rust drops replaced allocations exactly once.
                for (index, new) in new.lods.into_iter().enumerate() {
                    if let Some(new) = new {
                        if let Some(key) = self.object(object)?.lit_object.as_ref().unwrap().levels
                            [lighting]
                            .lods[index]
                        {
                            self.cleanup_saved_lit_lod(object, lighting, index, backend)?;
                            self.lods[(key.slot - 1) as usize] = None;
                            self.objects[(object.slot - 1) as usize]
                                .as_mut()
                                .unwrap()
                                .lit_object
                                .as_mut()
                                .unwrap()
                                .levels[lighting]
                                .lods[index] = None;
                        }
                        if self.object(object)?.lit_object.as_ref().unwrap().levels[lighting].lods
                            [index]
                            .is_none()
                        {
                            self.create_lit_lod_record(object, lighting, index)?;
                        }
                        self.objects[(object.slot - 1) as usize]
                            .as_mut()
                            .unwrap()
                            .lit_object
                            .as_mut()
                            .unwrap()
                            .levels[lighting]
                            .saved_lods[index] = Some(new);
                    }
                }
                if new.constructed {
                    self.objects[(object.slot - 1) as usize]
                        .as_mut()
                        .unwrap()
                        .lit_object
                        .as_mut()
                        .unwrap()
                        .levels[lighting]
                        .constructed = true;
                }
            }
        }
        Ok(())
    }
    pub fn saved_lit_lod(
        &self,
        object: ObjectResourceKey,
        lod: usize,
    ) -> Result<Option<&SavedLitLod>, ObjectError> {
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        if lod >= 10 {
            return Err(ObjectError::InvalidIndex(lod as u32));
        }
        Ok(lit.levels[lit.active_level].saved_lods[lod].as_ref())
    }
    pub fn create_unconstructed_lit_object_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, ObjectError> {
        let lit = Box::new(LitObjectResources::new());
        let key = self.create_object_resources(mesh)?;
        let record = self.objects[(key.slot - 1) as usize].as_mut().unwrap();
        record.visual_type = ObjectVisualType::LitUnconstructed;
        record.lit_object = Some(lit);
        Ok(key)
    }
    pub(crate) fn finish_unconstructed_lit_geometry(
        &mut self,
        lod: PlainLodKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        let index = self.index(lod.arena, lod.slot)?;
        self.lod(lod)?;
        self.lods[index].as_mut().unwrap().dirty = false;
        Ok(())
    }
    /// The host intake eagerly creates lighting-zero geometry. Native creation
    /// is lazy in m_AddFGroups. No constructed lit arrays/maps/index buffer are
    /// installed: all indices remain owned by the shared mesh face groups.
    pub(crate) fn create_unconstructed_lit_lod(
        &mut self,
        object: ObjectResourceKey,
        index: usize,
    ) -> Result<PlainLodKey, ObjectError> {
        self.create_lit_lod_record(object, 0, index)
    }
    fn create_lit_lod_record(
        &mut self,
        object: ObjectResourceKey,
        lighting: usize,
        index: usize,
    ) -> Result<PlainLodKey, ObjectError> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index as u32));
        }
        if lit.levels[lighting].lods[index].is_some() {
            return Err(ObjectError::SlotOccupied);
        }
        Self::reserve(&mut self.lit_storage)?;
        let key = self.create_plain_lod()?;
        self.lit_storage.push(LitLodStorage::new(key));
        let lod = self.lods[(key.slot - 1) as usize].as_mut().unwrap();
        // litLod's inline derived constructor overwrites these base words.
        lod.update_count = 0;
        lod.dirty = true;
        lod.owner = Some(object);
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[lighting]
            .lods[index] = Some(key);
        Ok(key)
    }
}

// --- Batch W (lit_object + litLod) ports ----------------------------------
// PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
// Upstream names from mafia-re ls3df/I3D/visuals/I3d_lit_object.cpp are working
// names, never presented as original PC symbols; RVA mapping retained per item.

/// LITMODE_LIGHTMAP bit evidenced by litLod::Create entry TEST [ESP+0xc],0x2
/// (PC RVA 0x2a0e0); default level.mode = 2 agrees. The VERTEX bit value is
/// unevidenced, so callers resolve the mode into `mode_lightmap: bool`.
pub const LIT_MODE_LIGHTMAP: u32 = 2;
/// Both m_AddFGroups failure exits share one word, evidenced twice at
/// 0x10027a74/0x10027ad8 (lazy litLod alloc, Create gate). Gate failures
/// return before any queue push, so the queue stays empty.
pub const LIT_QUEUE_OOM: &str = "!! Out of memory    renderLitObject()";
/// litLod::Create alloc failure (PC RVA 0x2a0e0): returns false before the
/// position copy and lod::Create.
pub const LIT_CREATE_VB_FAIL: &str = "litLod::Create() - Error create VertexBuffer for LM !!";
/// litLod::postProcess failure words (PC RVA 0x2a790). Effect order: IB alloc
/// precedes the UV null check; the rectangle check runs after earlier
/// vertices were already rewritten.
pub const LIT_POSTPROCESS_OOM_THIS: &str = "!! out of memory  litLod::postLoadProcess()  this:0x%x";
pub const LIT_POSTPROCESS_LM_FAIL: &str =
    "!! can't create LM  litLod::postLoadProcess()  this:0x%x";
pub const LIT_POSTPROCESS_IB_OOM: &str = "!! Out of memory  litLod::postLoadProcess";
pub const LIT_POSTPROCESS_UV_CHECK: &str =
    "!! neklaply kontroly m_pUV:0x%x  m_pLitUV:0x%x  litLod::postLoadProcess()";
pub const LIT_POSTPROCESS_CORRUPT: &str =
    "!! corrupted lightmap  litLod::postProcess()  m_numRectangles:%d < pVI->rect:%d";
pub const LIT_GET_LVERTEX_MISS: &str = "!! vertex not found GetLVertex() vertex:%d rectangle:%d";

/// Boundary marker for lit Save, PC RVA 0x27580 (contract: Ghidra formed no
/// body; mafia-re I3d_lit_object.cpp:1449-1455 reference only). Native prints
/// "NEMUZETE UKLADAT LM, KDYZ JE #define LS3D_FINAL !!!!!!!" and returns
/// I3DERR_GENERIC without touching state. Follows the ObjectFaceGroupContract
/// precedent: always reports the boundary, no parity credit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitSaveContract;

/// Caller-decoded material class for the lit queue arms. MATF_* bit values
/// are a material backend contract; only the class arms are evidenced here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LitQueueMaterialClass {
    Untextured,
    Opaque,
    KeyAlpha,
    Transparent,
}
/// Sort words the queue cannot derive: FloatToInt(sort_dist) and the
/// PRIM_SORT_* masks are unevidenced device/sort backend contracts. The CKEY
/// 0x8000 bit is evidenced (mafia-re I3d_lit_object.cpp:1784,1805); the
/// unconstructed CKEY bit value is not, so it rides the same contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitSortWords {
    pub dist_word: u32,
    pub dist_mask: u32,
    pub alpha_bit: u32,
    pub mat_mask: u32,
    pub ckey_bit: u32,
}
/// One mesh face group in link order (native walks `level->fgroups`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitQueueGroup {
    pub faces: u32,
    pub material: LitQueueMaterialClass,
    pub texture_uid: u32,
}
/// Queue intake: `constructed` selects the lightmap sort/first arms vs the
/// unconstructed arms; `lightmap` selects the env-texture arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LitQueueRequest {
    pub constructed: bool,
    pub lightmap: bool,
    pub lm_uid: u32,
    pub sort: LitSortWords,
    pub groups: Vec<LitQueueGroup>,
}
/// One queued face group. `first` is None on the unconstructed path (native
/// leaves `h.first` unset there); `env_texture_uid` is the lightmap texture
/// on the constructed lightmap path only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitQueuedGroup {
    pub group: usize,
    pub first: Option<u32>,
    pub texture_uid: Option<u32>,
    pub env_texture_uid: Option<u32>,
    pub sort_value: u32,
    pub transparent: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LitFaceGroupQueue {
    pub entries: Vec<LitQueuedGroup>,
    pub num_opaque: u32,
    pub num_transparent: u32,
}
/// Original m_AddFGroups queue order, PC RVA 0x27880 (differential: Ghidra
/// body 5278 B, 276 BB, 1525 insn, SEH entry; mafia-re I3d_lit_object.cpp:
/// 1560-1815). Pure CPU: preserves face-group link order, accumulates
/// `first += faces*3` with native 32-bit wrap, counts opaque/transparent,
/// and selects the lightmap vs own-texture sort arms. Proxy-sector walk,
/// W-bound distance, LOD select, update-count gate and every D3D8 call stay
/// backend contracts.
pub fn queue_lit_face_groups(request: &LitQueueRequest) -> LitFaceGroupQueue {
    let mut queue = LitFaceGroupQueue {
        entries: Vec::new(),
        num_opaque: 0,
        num_transparent: 0,
    };
    let mut first: u32 = 0;
    for (group, desc) in request.groups.iter().enumerate() {
        let (sort_value, opaque) = match desc.material {
            LitQueueMaterialClass::Untextured => (0, true),
            LitQueueMaterialClass::Transparent => (
                (request.sort.dist_word & request.sort.dist_mask) | request.sort.alpha_bit,
                false,
            ),
            LitQueueMaterialClass::KeyAlpha if request.constructed => (
                ((request.lm_uid | 0x8000) << 15) | (desc.texture_uid & request.sort.mat_mask),
                true,
            ),
            LitQueueMaterialClass::KeyAlpha => (
                (desc.texture_uid & request.sort.mat_mask) | request.sort.ckey_bit,
                true,
            ),
            LitQueueMaterialClass::Opaque if request.constructed => (
                ((request.lm_uid & request.sort.mat_mask) << 15)
                    | (desc.texture_uid & request.sort.mat_mask),
                true,
            ),
            LitQueueMaterialClass::Opaque => (desc.texture_uid & request.sort.mat_mask, true),
        };
        let at = if request.constructed {
            let at = first;
            first = first.wrapping_add(desc.faces.wrapping_mul(3));
            Some(at)
        } else {
            None
        };
        if opaque {
            queue.num_opaque = queue.num_opaque.wrapping_add(1);
        } else {
            queue.num_transparent = queue.num_transparent.wrapping_add(1);
        }
        queue.entries.push(LitQueuedGroup {
            group,
            first: at,
            texture_uid: (desc.material != LitQueueMaterialClass::Untextured)
                .then_some(desc.texture_uid),
            env_texture_uid: (request.constructed && request.lightmap).then_some(request.lm_uid),
            sort_value,
            transparent: !opaque,
        });
    }
    queue
}

/// litLod::Create order selector, PC RVA 0x2a0e0 (differential). The
/// `InitialUnlit` arm ignores the lod::Create result, exactly as natively;
/// every other fallible arm propagates failure. Mode contract: native tests
/// LITMODE_LIGHTMAP then LITMODE_VERTEX with VERTEX-first priority (mode 3
/// takes the vertex-colors arm, mode 0 the lightmap arms); the VERTEX bit
/// value is unevidenced, so callers resolve the mode into `mode_lightmap`
/// and must only pass single-bit modes — other words take the
/// caller-resolved arm by contract (reachable via the SetMode u32
/// passthrough; disclosed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LitCreatePlan {
    InitialUnlit,
    VertexColors { double_saturate: bool },
    HwtlLightmap,
    PositionAlloc { reuse_positions: bool },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitCreateOrder {
    /// Pre-pass: LIGHTMAP && !dirty && positions held frees and zeroes the
    /// position buffer before the arm below runs.
    pub release_positions: bool,
    pub plan: LitCreatePlan,
}
pub fn plan_lit_lod_create(
    mode_lightmap: bool,
    dirty: bool,
    has_positions: bool,
    lit_vertices: u32,
    hw_tl: bool,
    no_modulate2x: bool,
) -> LitCreateOrder {
    let release_positions = mode_lightmap && !dirty && has_positions;
    let held = has_positions && !release_positions;
    let plan = if lit_vertices == 0 {
        LitCreatePlan::InitialUnlit
    } else if !mode_lightmap {
        LitCreatePlan::VertexColors {
            double_saturate: no_modulate2x,
        }
    } else if hw_tl {
        LitCreatePlan::HwtlLightmap
    } else {
        LitCreatePlan::PositionAlloc {
            reuse_positions: dirty && held,
        }
    };
    LitCreateOrder {
        release_positions,
        plan,
    }
}
/// PROBLEM_NO_MODULATE2X color double, PC RVA 0x2a0e0 (differential: mid
/// probes AND 0xff / SHL 1). Pure u32 bit math, widths from the
/// instructions; alpha passes through untouched.
pub fn lit_vertex_color_doubled(color: u32) -> u32 {
    let alpha = color & 0xff00_0000;
    let red = ((color >> 15) & 0x1fe).min(0xff);
    let green = ((color >> 7) & 0x1fe).min(0xff);
    let blue = ((color & 0xff) << 1).min(0xff);
    alpha | (red << 16) | (green << 8) | blue
}

/// litLod destructor heap plan, PC RVA 0x29f80 (differential: 339 B, 18 BB,
/// 113 insn). Single-color rectangles carry the inline color and free no
/// heap data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitLodDropPlan {
    pub free_positions: bool,
    pub free_colors: bool,
}
pub fn lit_lod_drop_plan(saved: Option<&SavedLitLod>) -> LitLodDropPlan {
    match saved {
        Some(SavedLitLod::Lightmap(lod)) => LitLodDropPlan {
            free_positions: lod.memory_bytes > 0,
            free_colors: lod.memory_bytes == 0,
        },
        Some(SavedLitLod::VertexColors { .. }) => LitLodDropPlan {
            free_positions: false,
            free_colors: true,
        },
        None => LitLodDropPlan {
            free_positions: false,
            free_colors: false,
        },
    }
}

/// litLod::postProcess completion, PC RVA 0x2a790 (differential).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitPostprocessCounts {
    /// Atlas bytes (m_MemSize analog, owned by the existing atlas).
    pub memory_bytes: u32,
    pub lit_vertices: u32,
    pub indices: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LitPostprocessOutcome {
    Ready(LitPostprocessCounts),
    DeferredIndexAlloc,
}
/// Post-process intake for litLod::postProcess (PC RVA 0x2a790). Groups the
/// face-group table, the loaded flag and the source binding so the ordered
/// tail stays callable without argument sprawl.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitPostprocessRequest<'a> {
    pub face_group_faces: &'a [u32],
    pub loaded: bool,
    pub source: LitSourceBinding,
}

/// Native UpdateLMFromRectInfos rect plan, PC RVA 0x21830 (contract: Ghidra
/// formed no procedure; 220-insn linear walk 0x21830-0x21ac5). Precision
/// policy: f32 arithmetic in native order (w*u / w*(u-2)), truncation toward
/// zero for x/y (native ftol); the u>=2.0 rotated marker is preserved
/// bit-exactly (FCOMP vs 2.0f). Device upload stays a backend contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LitRectUpload {
    pub dest_x: u32,
    pub dest_y: u32,
    pub rotated: bool,
}
/// Resolve one rect's texture destination. `u_base`/`v_base` are the saved
/// rectangle words (u carries the +2.0 rotated marker, as Load stores it).
/// Out-of-atlas results are refused; native would write OOB (disclosed
/// hardening). `placement` is the atlas packer output for the same rect: it
/// only cross-checks the destination against the packed cell, never supplies
/// dest_y (native resolves both axes from the RECTANGLE_INFO floats).
pub fn lit_rect_upload_plan(
    placement: &LightmapPlacement,
    atlas_w: u32,
    atlas_h: u32,
    u_base: f32,
    v_base: f32,
) -> Option<LitRectUpload> {
    if atlas_w == 0 || atlas_h == 0 {
        return None;
    }
    let (u, rotated) = if u_base < 2.0 {
        (u_base, false)
    } else {
        (u_base - 2.0, true)
    };
    if !u.is_finite() || u < 0.0 || !v_base.is_finite() || v_base < 0.0 {
        return None;
    }
    let x = (atlas_w as f32 * u).trunc();
    let y = (atlas_h as f32 * v_base).trunc();
    if !(x >= 0.0 && y >= 0.0 && x < atlas_w as f32 && y < atlas_h as f32) {
        return None;
    }
    let (dx, dy) = (x as u32, y as u32);
    // Cross-check against the packed cell (u16 texels, inclusive bounds).
    let in_cell = dx >= u32::from(placement.x)
        && dy >= u32::from(placement.y)
        && dx < u32::from(placement.x) + u32::from(placement.width)
        && dy < u32::from(placement.y) + u32::from(placement.height);
    if !in_cell {
        return None;
    }
    Some(LitRectUpload {
        dest_x: dx,
        dest_y: dy,
        rotated,
    })
}
/// Device upload stays a backend contract: Lock/FillLineRGB/Unlock +
/// AddDirtyRect have no host owner (no live D3D8 device). The single-color
/// BGRX splat and the resized-then-maybe-rotated fill are the backend's
/// ordered effects; the plan supplies dest_x/dest_y/rotated. No D3D8
/// implementor exists yet: the future emitter implements this trait against
/// the locked surface (see render_lit_face_group_contract for the sibling
/// device boundary).
pub trait LitRectUploadBackend {
    type Error;
    fn upload_lit_rect(
        &mut self,
        plan: LitRectUpload,
        texels_bgr: &[u8],
        pitch: u32,
    ) -> Result<(), Self::Error>;
}
/// Device index-buffer lock stays a backend contract (native virtual calls
/// plus the global registry have no host owner). Native flag words: -1 no
/// lod, 0 locked.
pub trait LitIndexLockBackend {
    type Error;
    fn lock_lit_index_buffer(&mut self, lod: PlainLodKey) -> Result<(), Self::Error>;
}

/// Offline-construct entry plan, PC RVA 0x21bb0 (contract: 70-insn entry
/// sample; full body not recovered). Owns only the guard prefix:
/// active-lighting teardown (native Destruct(0) first effect, reused), mesh
/// check (native 0x80000007 word), resolution store. Sector walk, timer,
/// face grouping, AA/fill/blur/single-color branches are OPEN. A backend
/// failure inside the teardown keeps its carrier via `Backend(E)`; the arena
/// poison flag is already set by the failing teardown itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitConstructPlan {
    pub lighting: usize,
    pub resolution: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LitConstructError<E> {
    Object(ObjectError),
    /// Native I3DERR_INVALIDPARAMS word 0x80000007 (no mesh).
    MissingMesh,
    /// Backend failure inside the Destruct teardown (the arena poison flag is
    /// already set by the failing teardown itself).
    Backend(E),
}
impl<E> From<ObjectError> for LitConstructError<E> {
    fn from(e: ObjectError) -> Self {
        Self::Object(e)
    }
}

impl ObjectResources {
    /// Original SetMode, PC RVA 0x275a0 (differential: REA ghidra body 42 B,
    /// 13 insn/1 BB; mafia-re I3d_lit_object.cpp:1457-1464). Effect order is
    /// the Destruct(0) virtual call ([EAX+0x94]) FIRST, then the
    /// unconditional u32 mode store at levels[active_level] (IMUL stride
    /// 0x38 from this+0x224; the single BB has no validation branch, so any
    /// u32 passes through bitwise). Native reads the active level
    /// unchecked; the host rejects active_level >= 6 with InvalidIndex
    /// instead of indexing out of bounds (disclosed hardening; unreachable
    /// via set_active_lighting, which gates < 6). Divergence (hardening):
    /// native discards the Destruct(0) result and always stores; the host
    /// propagates the error and skips the store, following the
    /// lit_set_object_mesh host pattern. Poison is set only when the
    /// failure came from the mutating phase (the callee poisoned the
    /// arena): pre-effect validation failures (LodOwned/LodDestructed,
    /// nothing mutated) pass through unpoisoned.
    pub fn set_lit_lighting_mode<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        mode: u32,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let active = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?
            .active_level;
        if active >= 6 {
            return Err(ObjectError::InvalidIndex(active as u32).into());
        }
        let result = (|| {
            self.destruct_active_lighting(object, backend)?;
            self.objects[(object.slot - 1) as usize]
                .as_mut()
                .unwrap()
                .lit_object
                .as_mut()
                .unwrap()
                .levels[active]
                .mode = mode;
            Ok(())
        })();
        // Poison discipline is callee-owned: mutating-phase failures poison
        // inside the teardown, pre-effect validation failures (nothing
        // mutated) pass through unpoisoned. No flag is set here.
        result
    }
    /// Original Save refusal, PC RVA 0x27580 (contract: Ghidra formed no
    /// body; mafia-re I3d_lit_object.cpp:1449-1455 reference only). Offline
    /// Save stays outside this arena like offline Construct; file/device
    /// calls stay backend contracts. Always reports the boundary (no parity
    /// credit).
    pub fn save_lit_lighting_contract(
        &self,
        _object: ObjectResourceKey,
    ) -> Result<LitSaveContract, ObjectError> {
        self.available()?;
        Err(ObjectError::UnsupportedVisualSubtype)
    }
    /// Native UpdateVertices, PC RVA 0x21740 (contract: Ghidra formed no
    /// procedure at VA 0x10021740; 31-insn linear walk 0x21740-0x21783).
    /// Effect order: lighting 0..6 major, lod 0..10 minor, null lods skipped
    /// in place. Native writes litLod bytes +0x2c=1, +0x4=-1, +0xC=-1; the
    /// two light-key words have no host owner and are subsumed by `dirty`
    /// (no light cache exists on SavedLitLod). Returns marked lod records,
    /// mirroring mark_all_object_lods_dirty. Two-pass (validate ownership,
    /// then mark): native single-pass never fails on raw pointers; the host
    /// hardens with LodOwned/LodDestructed before any mutation (disclosed).
    pub fn update_all_lit_vertices(
        &mut self,
        object: ObjectResourceKey,
    ) -> Result<u32, ObjectError> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        for level in &lit.levels {
            for key in level.lods.iter().flatten() {
                let lod = self.lod(*key)?;
                if lod.owner != Some(object) {
                    return Err(ObjectError::LodOwned(*key));
                }
                if lod.state != LodState::Constructed {
                    return Err(ObjectError::LodDestructed(*key));
                }
            }
        }
        let mut marked = 0u32;
        for lighting in 0..6 {
            for index in 0..10 {
                if let Some(key) =
                    self.object(object)?.lit_object.as_ref().unwrap().levels[lighting].lods[index]
                {
                    let at = self.index(key.arena, key.slot)?;
                    self.lods[at]
                        .as_mut()
                        .ok_or(ObjectError::UnknownLod(key))?
                        .dirty = true;
                    marked += 1;
                }
            }
        }
        Ok(marked)
    }
    /// Native UpdateVertices(index), PC RVA 0x21790 (contract: no procedure
    /// at 0x10021790; 19-insn walk 0x21790-0x217c3). Native applies NO index
    /// bound (raw lods[index] column); the host enforces 0..10 (disclosed
    /// hardening). Two-pass like update_all_lit_vertices (disclosed above).
    pub fn update_lit_lod_vertices(
        &mut self,
        object: ObjectResourceKey,
        lod: usize,
    ) -> Result<u32, ObjectError> {
        if lod >= 10 {
            return Err(ObjectError::InvalidIndex(lod as u32));
        }
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        for level in &lit.levels {
            if let Some(key) = level.lods[lod] {
                let rec = self.lod(key)?;
                if rec.owner != Some(object) {
                    return Err(ObjectError::LodOwned(key));
                }
                if rec.state != LodState::Constructed {
                    return Err(ObjectError::LodDestructed(key));
                }
            }
        }
        let mut marked = 0u32;
        for lighting in 0..6 {
            if let Some(key) =
                self.object(object)?.lit_object.as_ref().unwrap().levels[lighting].lods[lod]
            {
                let at = self.index(key.arena, key.slot)?;
                self.lods[at]
                    .as_mut()
                    .ok_or(ObjectError::UnknownLod(key))?
                    .dirty = true;
                marked += 1;
            }
        }
        Ok(marked)
    }
    /// Native GetNumRectInfos, PC RVA 0x217d0 (contract: 11-insn walk). Null
    /// lod record reads word 0. Native checks no index bounds (host
    /// hardens). Vertex-color lods own no rectangles: native m_numRectangles
    /// is a lightmap-lod word (disclosed mapping).
    pub fn lit_rect_count(
        &self,
        object: ObjectResourceKey,
        lighting: usize,
        lod: usize,
    ) -> Result<usize, ObjectError> {
        if lighting >= 6 {
            return Err(ObjectError::InvalidIndex(lighting as u32));
        }
        if lod >= 10 {
            return Err(ObjectError::InvalidIndex(lod as u32));
        }
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        Ok(match &lit.levels[lighting].saved_lods[lod] {
            None => 0,
            Some(SavedLitLod::VertexColors { .. }) => 0,
            Some(SavedLitLod::Lightmap(lod)) => lod.placements.len(),
        })
    }
    /// Native GetRectInfos, PC RVA 0x21800 (contract: 11-insn walk). Returns
    /// the live placement slice; None covers both native-NULL cases (no
    /// record / vertex-color lod). No copy: views borrow state.
    pub fn lit_rect_placements(
        &self,
        object: ObjectResourceKey,
        lighting: usize,
        lod: usize,
    ) -> Result<Option<&[LightmapPlacement]>, ObjectError> {
        if lighting >= 6 {
            return Err(ObjectError::InvalidIndex(lighting as u32));
        }
        if lod >= 10 {
            return Err(ObjectError::InvalidIndex(lod as u32));
        }
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        Ok(match &lit.levels[lighting].saved_lods[lod] {
            Some(SavedLitLod::Lightmap(lod)) => Some(&lod.placements),
            _ => None,
        })
    }
    /// Native GetTotalLMMemory, PC RVA 0x27830 (contract: 33-insn full linear
    /// body 0x27830-0x27879). Mesh-absent reads word 0. Wrapping add matches
    /// native ADD. Divergences (disclosed): vertex-color lods contribute 0
    /// (their native m_MemSize word is unrecovered, OPEN); the sum runs over
    /// saved payloads while native sums live lods' m_MemSize, so a
    /// constructed-but-unsaved lod reads 0 here.
    pub fn total_lit_lightmap_memory(&self, object: ObjectResourceKey) -> Result<u32, ObjectError> {
        let rec = self.object(object)?;
        rec.lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        if rec.mesh.is_none() {
            return Ok(0);
        }
        let lit = rec.lit_object.as_ref().unwrap();
        let mut total = 0u32;
        for level in &lit.levels {
            for saved in level.saved_lods.iter().flatten() {
                if let SavedLitLod::Lightmap(lod) = saved {
                    total = total.wrapping_add(lod.memory_bytes);
                }
            }
        }
        Ok(total)
    }
    /// Offline-construct entry guard, PC RVA 0x21bb0 (contract: 70-insn entry
    /// sample owns the Construct identity via SEH+Destruct(0)+INVALIDPARAMS+
    /// sector-walk prefix; body phases OPEN). Native guard order is teardown
    /// first (Destruct(0), then the mesh INVALIDPARAMS check), matched here:
    /// a mesh-less object with live LODs is torn down before MissingMesh
    /// reports. Offline lighting stays out of scope.
    pub fn plan_lit_construct<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        resolution: f32,
        backend: &mut B,
    ) -> Result<LitConstructPlan, LitConstructError<B::Error>> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        let lighting = lit.active_level;
        self.destruct_active_lighting(object, backend)
            .map_err(|e| match e {
                ObjectOperationError::State(e) => LitConstructError::Object(e),
                ObjectOperationError::Backend(e) => LitConstructError::Backend(e),
            })?;
        if self.object(object)?.mesh.is_none() {
            return Err(LitConstructError::MissingMesh);
        }
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[lighting]
            .resolution = resolution;
        Ok(LitConstructPlan {
            lighting,
            resolution,
        })
    }
    /// Native GetRectInfoFlags, PC RVA 0x21ad0 (contract: full span
    /// 0x21ad0-0x21bab walked). None (native -1) when no constructed lod
    /// record; Some(0) after the backend lock. The `flags` operand is
    /// accepted and ignored exactly like native [ESP+0x10]. The resident
    /// `[ESI+4] & 0x10` word gates a second virtual call natively and is
    /// unrecovered here: the port always locks (hardening, disclosed).
    /// Backend lock failures keep their backend carrier (no erasure into a
    /// state word).
    pub fn lit_index_lock_flags<B: LitIndexLockBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        lighting: usize,
        lod: usize,
        _flags: u32,
        backend: &mut B,
    ) -> Result<Option<u32>, ObjectOperationError<B::Error>> {
        if lighting >= 6 {
            return Err(ObjectError::InvalidIndex(lighting as u32).into());
        }
        if lod >= 10 {
            return Err(ObjectError::InvalidIndex(lod as u32).into());
        }
        self.available()?;
        let key = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?
            .levels[lighting]
            .lods[lod];
        let Some(key) = key else { return Ok(None) };
        let rec = self.lod(key)?;
        if rec.owner != Some(object) {
            return Err(ObjectError::LodOwned(key).into());
        }
        if rec.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(key).into());
        }
        backend
            .lock_lit_index_buffer(key)
            .map_err(ObjectOperationError::Backend)?;
        Ok(Some(0))
    }
    /// m_AddFGroups update-count arm (mafia-re I3d_lit_object.cpp:1614-1618).
    /// Returns the native `moved` word; the frame count is a caller backend
    /// contract. Helper: no function credit.
    pub fn sync_lit_update_count(
        &mut self,
        lod: PlainLodKey,
        update_count: u32,
    ) -> Result<bool, ObjectError> {
        self.available()?;
        let current = self.lod(lod)?;
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod));
        }
        let moved = current.update_count != update_count;
        if moved {
            self.lods[(lod.slot - 1) as usize]
                .as_mut()
                .unwrap()
                .update_count = update_count;
        }
        Ok(moved)
    }
    /// m_AddFGroups post-Create arm (mafia-re I3d_lit_object.cpp:1610-1611).
    /// Runs after the backend vertex allocation, never before it. Helper: no
    /// credit.
    pub fn note_lit_lod_recreated(&mut self, lod: PlainLodKey) -> Result<(), ObjectError> {
        self.available()?;
        let current = self.lod(lod)?;
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod));
        }
        let record = self.lods[(lod.slot - 1) as usize].as_mut().unwrap();
        record.lights_changed = u32::MAX;
        record.light_key = u32::MAX;
        Ok(())
    }
    /// Original m_RenderFGroup, PC RVA 0x28d40 (contract: Ghidra 12.1.4
    /// formed no body — unformed gap 0x10028d1e-0x10029f7f; mafia-re
    /// I3d_lit_object.cpp:1824-2083 reference only). Every stage-state/
    /// texture/draw call stays a backend contract; the LOD select, alpha
    /// product and reset-bit words are the existing select_render_lod_index
    /// / render_total_alpha / render_stage_reset_flags ports. Always reports
    /// the boundary (no parity credit).
    pub fn render_lit_face_group_contract(
        &self,
        _object: ObjectResourceKey,
    ) -> Result<ObjectFaceGroupContract, ObjectError> {
        self.available()?;
        Err(ObjectError::UnsupportedVisualSubtype)
    }
    /// Original litLod destructor order, PC RVA 0x29f80 (differential: 339 B,
    /// 18 BB, 113 insn; mafia-re I3d_lit_object.cpp:2087-2117). Runs the
    /// existing derived-payload → IB → VB retirement, then the owner-dtor
    /// NULL order (record retires, both slots clear). An empty slot is a
    /// native NULL-skip. The m_MemSize=0 / PIndex-NULL residue is modeled by
    /// phase Retired + consumed index + cleared slots. The LodOwned check is
    /// host hardening; native deletes unchecked.
    pub fn destroy_lit_lod_allocation<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        lighting: usize,
        index: usize,
        backend: &mut B,
    ) -> Result<LitLodDropPlan, ObjectOperationError<B::Error>> {
        self.available()?;
        if lighting >= 6 {
            return Err(ObjectError::InvalidIndex(lighting as u32).into());
        }
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index as u32).into());
        }
        let key = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?
            .levels[lighting]
            .lods[index];
        let Some(key) = key else {
            return Ok(lit_lod_drop_plan(None));
        };
        if self.lod(key)?.owner != Some(object) {
            return Err(ObjectError::LodOwned(key).into());
        }
        let plan = lit_lod_drop_plan(
            self.object(object)?.lit_object.as_ref().unwrap().levels[lighting].saved_lods[index]
                .as_ref(),
        );
        self.cleanup_saved_lit_lod(object, lighting, index, backend)?;
        self.lods[(key.slot - 1) as usize] = None;
        let level = &mut self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[lighting];
        level.lods[index] = None;
        level.saved_lods[index] = None;
        Ok(plan)
    }
    /// Original litLod::postProcess order, PC RVA 0x2a790 (differential: 1320
    /// B, 68 BB, 415 insn; mafia-re I3d_lit_object.cpp:2200-2276). The caller
    /// owns the phase gate: storage must be Preparing (the atlas create and
    /// m_pUV allocation it models precede this call, as does Load's own UV
    /// remap, which already applied the half-texel rotation arms in source
    /// order — host f32, no fusion). Divergences (hardening, both disclosed):
    /// the `!loaded` index-buffer allocation is deferred until AFTER the
    /// rectangle and face-count gates — natively the IB alloc precedes the
    /// UV null-check and the rect recheck runs mid-rewrite with partial
    /// m_pLitUV mutation, while the port performs no backend effect and
    /// retains payload + Preparing phase on those paths; the face-count gate
    /// (expected==0 or saved indices != faces*3 → InvalidIndex) has no
    /// native counterpart (native allocates NumFaces*3 unconditionally).
    /// Face-group accumulation uses native 32-bit wrapping arithmetic.
    /// Vertex-color payloads do no index work (their render path uses the
    /// face groups' own IB). No new float math here; sort_dist conversion
    /// stays a backend contract.
    pub fn apply_lit_postprocess_order<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        lighting: usize,
        index: usize,
        request: LitPostprocessRequest<'_>,
        backend: &mut B,
    ) -> Result<LitPostprocessOutcome, ObjectOperationError<B::Error>> {
        let LitPostprocessRequest {
            face_group_faces,
            loaded,
            source,
        } = request;
        self.available()?;
        if lighting >= 6 {
            return Err(ObjectError::InvalidIndex(lighting as u32).into());
        }
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index as u32).into());
        }
        let key = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?
            .levels[lighting]
            .lods[index];
        let Some(key) = key else {
            return Err(ObjectError::InvalidIndex(index as u32).into());
        };
        if self.lod(key)?.owner != Some(object) {
            return Err(ObjectError::LodOwned(key).into());
        }
        if self.lit_lod_storage(key)?.phase != LitStoragePhase::Preparing {
            return Err(ObjectError::StorageAlreadyInstalled.into());
        }
        enum Payload {
            VertexColors {
                verts: usize,
            },
            Lightmap {
                rects: usize,
                indices: usize,
                memory: u32,
            },
        }
        let payload = match self.object(object)?.lit_object.as_ref().unwrap().levels[lighting]
            .saved_lods[index]
            .as_ref()
        {
            None => return Err(ObjectError::InvalidIndex(index as u32).into()),
            Some(SavedLitLod::VertexColors { colors, .. }) => Payload::VertexColors {
                verts: colors.len(),
            },
            Some(SavedLitLod::Lightmap(lod)) => Payload::Lightmap {
                rects: lod.placements.len(),
                indices: lod.indices.len(),
                memory: lod.memory_bytes,
            },
        };
        if let Payload::Lightmap { rects, .. } = payload {
            let bad = self.object(object)?.lit_object.as_ref().unwrap().levels[lighting].saved_lods
                [index]
                .as_ref()
                .unwrap();
            let SavedLitLod::Lightmap(lod) = bad else {
                // Payload kind is re-read, never assumed: without &mut access
                // between the borrows it cannot change, but a graceful state
                // error beats a panic if that ever stops holding.
                return Err(ObjectError::InvalidIndex(index as u32).into());
            };
            if lod
                .vertices
                .iter()
                .any(|v| usize::from(v.rectangle) >= rects)
            {
                return Err(ObjectError::InvalidIndex(index as u32).into());
            }
        }
        match payload {
            Payload::VertexColors { verts } => {
                self.finish_lit_preparation(key, source)?;
                Ok(LitPostprocessOutcome::Ready(LitPostprocessCounts {
                    memory_bytes: 0,
                    lit_vertices: u32::try_from(verts).map_err(|_| ObjectError::HostAllocation)?,
                    indices: 0,
                }))
            }
            Payload::Lightmap {
                indices, memory, ..
            } => {
                let faces = face_group_faces
                    .iter()
                    .fold(0u32, |a, &b| a.wrapping_add(b));
                let expected = faces.wrapping_mul(3);
                if expected == 0 || indices != expected as usize {
                    return Err(ObjectError::InvalidIndex(index as u32).into());
                }
                if !loaded && !self.allocate_saved_lit_indices(key, expected, backend)? {
                    return Ok(LitPostprocessOutcome::DeferredIndexAlloc);
                }
                self.finish_lit_preparation(key, source)?;
                let verts = match self.object(object)?.lit_object.as_ref().unwrap().levels[lighting]
                    .saved_lods[index]
                    .as_ref()
                    .unwrap()
                {
                    SavedLitLod::Lightmap(lod) => lod.vertices.len(),
                    SavedLitLod::VertexColors { .. } => {
                        return Err(ObjectError::InvalidIndex(index as u32).into());
                    }
                };
                Ok(LitPostprocessOutcome::Ready(LitPostprocessCounts {
                    memory_bytes: memory,
                    lit_vertices: u32::try_from(verts).map_err(|_| ObjectError::HostAllocation)?,
                    indices: expected,
                }))
            }
        }
    }
}
