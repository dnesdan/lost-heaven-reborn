//! Plain per-object LOD ownership at the original LS3DF I3D boundary.
//!
//! PC LS3DF.dll SHA-256:
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Scoped original entries (RVAs): lod constructor 0x36c70 represented fields;
//! nondeleting cleanup 0x36ce0; scalar destructor 0x36cb0 cleanup followed here
//! by host retirement, excluding native allocation/delete and pointer return;
//! I3D_object destructor 0x36840 ten-LOD/mesh ownership prefix only, excluding
//! its visual/frame base cleanup. The I3D_object class name has original PS2
//! linker-symbol provenance; PC behavior/layout are independently evidenced.
//!
//! `Lod` is the PC diagnostic class name. Lighting/dirty field labels follow
//! descriptive upstream names; they are not claimed original linker symbols.
//! `I3dObjectResources` projects resource ownership plus ordinary cur_lod/fade
//! render state of the same I3D_object; it is not a complete native layout.
//! This host arena does not implement native Create, SetMesh, visual/frame base
//! destruction or native uploads. The bounded unconstructed lit subtype owns
//! its separate constructor state and inherited-buffer projections below.
//! Primitive retirement uses host tombstones, without native allocator/delete or pointer-return credit.
//!
//! One compatible backend domain and serialized nonreentrant callbacks are
//! required throughout. Explicit cleanup must occur before that backend goes
//! away; Drop cannot run these required effects. Read-only views remain raw
//! diagnostics after an interrupted cleanup; old slots can name retired LODs.
#[path = "lit_lod_storage.rs"]
mod lit_lod_storage;
#[path = "lit_object.rs"]
mod lit_object;
use crate::buffer_managers::{VertexOwnerKey, VertexRequest};
use crate::managed_visual_resources::{
    BindingState, ManagedError, ManagedOperationError, ManagedVisualBackend, ManagedVisualResources,
};
use crate::visual_resources::{
    CompleteVertexBuffer, FaceGroupKey, I3dTriface, MaterialKey, MeshLevelKey, MeshObjectKey,
    TextureKey,
};
pub use lit_lod_storage::{
    LitLodStorage, LitSourceBinding, LitStorageCounts, LitStoragePhase, RetiringLitArrays,
};
/// Re-exported lightmap word helpers so the rewrite-overlay symbol paths
/// (`i3d_object::convert_lightmap_texels`, …) resolve literally and not only
/// through the validator's `#[path]` fallback.
pub use lit_object::lightmap_atlas::{
    convert_lightmap_texels, duplicate_free_list, fill_line_rgb_swap, insert_sorted_free_rect,
    place_lightmap_rect, reinsert_shrunk_free_rect, resize_lightmap_rgb, AtlasError,
    AtlasStatistics, LightmapAtlasPool, LightmapPlacement, LightmapRectangle, LightmapTexels,
};
pub use lit_object::{
    lightmap_atlas, lit_lod_drop_plan, lit_rect_upload_plan, lit_vertex_color_doubled,
    plan_lit_lod_create, queue_lit_face_groups, saved_lightmaps, LitConstructError,
    LitConstructPlan, LitCreateOrder, LitCreatePlan, LitFaceGroupQueue, LitIndexLockBackend,
    LitLevelResources, LitLodDropPlan, LitObjectResources, LitPostprocessCounts,
    LitPostprocessOutcome, LitPostprocessRequest, LitQueueGroup, LitQueueMaterialClass,
    LitQueueRequest, LitQueuedGroup, LitRectUpload, LitRectUploadBackend, LitSaveContract,
    LitSortWords, ObjectVisualType, LIT_CREATE_VB_FAIL, LIT_GET_LVERTEX_MISS, LIT_MODE_LIGHTMAP,
    LIT_POSTPROCESS_CORRUPT, LIT_POSTPROCESS_IB_OOM, LIT_POSTPROCESS_LM_FAIL,
    LIT_POSTPROCESS_OOM_THIS, LIT_POSTPROCESS_UV_CHECK, LIT_QUEUE_OOM,
};

macro_rules! key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name {
            arena: u64,
            slot: u32,
        }
        impl $name {
            pub fn arena(self) -> u64 {
                self.arena
            }
            pub fn slot(self) -> u32 {
                self.slot
            }
        }
    };
}
key!(PlainLodKey);
key!(ObjectResourceKey);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectError {
    Poisoned,
    Managed(ManagedError),
    ForeignArena,
    IdentityOverflow,
    HostAllocation,
    UnknownLod(PlainLodKey),
    UnknownObject(ObjectResourceKey),
    InvalidIndex(u32),
    UnsupportedIndex(u32),
    SlotOccupied,
    LodOwned(PlainLodKey),
    LodDestructed(PlainLodKey),
    StorageAlreadyInstalled,
    MeshReferenceOwned(MeshObjectKey),
    MeshCompatibilityUnavailable,
    UnsupportedVisualSubtype,
}
impl From<ManagedError> for ObjectError {
    fn from(e: ManagedError) -> Self {
        Self::Managed(e)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ObjectOperationError<E> {
    State(ObjectError),
    Backend(E),
}
impl<E> From<ObjectError> for ObjectOperationError<E> {
    fn from(e: ObjectError) -> Self {
        Self::State(e)
    }
}
impl<E> From<ManagedError> for ObjectOperationError<E> {
    fn from(e: ManagedError) -> Self {
        Self::State(e.into())
    }
}
fn managed_result<T, E>(
    r: Result<T, ManagedOperationError<E>>,
) -> Result<T, ObjectOperationError<E>> {
    r.map_err(|e| match e {
        ManagedOperationError::State(e) => e.into(),
        ManagedOperationError::Backend(e) => ObjectOperationError::Backend(e),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LodState {
    Constructed,
    Destructed,
}
/// VISF_DUPLICATE_MASK per mafia-re I3d_object.cpp:141-143 (PC RVA 0x36b10).
pub const DUPLICATE_VIS_MASK: u32 = 0x2_ffd0;
/// Object-field snapshot copied by Duplicate (PC RVA 0x36b10): bitwise
/// floats, masked visual flags, plain bound volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DuplicateObjectFields {
    pub lightness: f32,
    pub transparency: f32,
    pub fade: f32,
    pub visual_flags_masked: u32,
    pub bounds: crate::mesh_bounds::MeshBoundVolume,
    pub world_space: bool,
}
/// CPU tail parameters for lod::Create (PC RVA 0x37030): masked format,
/// vertex stride in floats, and whether the VB alloc arm is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodCreateParams {
    pub format: u32,
    pub stride_floats: u32,
    pub needs_alloc: bool,
}
/// Boundary marker for m_AddFGroups (PC RVA 0x38020, contract only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectFaceGroupContract;
/// Host projection of one packed-rectangle offset in a texture with room
/// left (mafia-re ab67b13 `ls3df/I3D/mesh/lightmap.h:34-38` OFFSET_STRUCT:
/// guessed PS2 member names `x`, `y`, `rotated`; 0xc bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TexblOffset {
    pub x: i32,
    pub y: i32,
    pub rotated: bool,
}
/// Host projection of the destructor-owned member of TEXBL_STRUCT, a
/// texture's free room (mafia-re ab67b13 `ls3df/I3D/mesh/lightmap.cpp:28-37`;
/// PS2-evidenced struct name, 0x28 bytes). Only the `offsets` list
/// (`C_list<OFFSET_STRUCT>`, grown from 10) is destructor-owned; the native
/// free-rectangle chain (`first`/`last`) is freed explicitly by
/// `m_EraseRectList`, and the texture pointer is released through the
/// `LightMap` destructor paths — neither is part of this teardown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TexblStruct {
    pub offsets: Vec<TexblOffset>,
    pub free_area: i32,
    pub lost_area: i32,
}
/// LS3DF RVA 0x5da70, VA 0x1005da70. `TEXBL_STRUCT::~TEXBL_STRUCT` working
/// name (mafia-re ab67b13 `ls3df/I3D/mesh/lightmap.h:85-87` SYNTHETIC; the
/// `TEXBL_STRUCT` name is PS2-evidenced but this destructor spelling is NOT
/// an original symbol). Contract tier (wave policy: native 1 BB / 28 B body
/// destroys exactly one member, no Ghidra body observed). Member-wise
/// teardown: the `offsets` `C_list` destructor (`ls3df.h:120-123`, which
/// calls `clear()`: `delete[]` the array, NULL it, zero both sizes) maps to
/// releasing the host vector storage. Scalar words keep their values (native
/// scalars are never zeroed; their storage vanishes with the struct), and
/// the free-rectangle chain and texture pointer are untouched. Idempotent.
pub fn texbl_struct_teardown(entry: &mut TexblStruct) {
    entry.offsets = Vec::new();
}
/// Opaque 0x20-byte `RENDER_FG_HELP_STRUCT` slot for the render queue
/// (mafia-re ls3df/I3D/I3D.h:4586-4601; PC-only working struct, 8 words).
/// Member names are not ported; size only pins the `C_list` element width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FaceGroupQueueEntry(pub [u32; 8]);
/// Host projection of `ADD_FG_HELP_STRUCT` queue state for LS3DF RVA 0x659a0
/// (mafia-re ls3df/I3D/I3D.h:4681-4700). Upstream working name
/// `ADD_FG_HELP_STRUCT`, not an evidenced original symbol. Probe verified a
/// formed Ghidra body (FUN_100659a0, `fgroups(0x400)` + `unknown_0c = 0`);
/// contract tier (partial coverage: the evidenced stores are pinned, the
/// host-zero rest is disclosed admission). Probe-pinned new ctor (not a
/// corroboration: this is the first host owner of these stores). Partial queue projection: only
/// the evidenced ctor stores are owned here: `unknown_0c = 0` and the `fgroups` `C_list` with grow
/// `0x400` (array NULL, sizes zero). The `sort` list starts empty
/// (`base NULL, res_num 0, count 0`); camera/sector/scene/`cam_pos` and the
/// `num_opaque`/`num_transparent` counts are left unwritten natively and
/// start at host zero/empty (disclosed, cleared by the separate `Clear`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FaceGroupQueue {
    pub unknown_0c: u32,
    pub fgroup_grow: u32,
    pub fgroups: Vec<FaceGroupQueueEntry>,
    pub sort: Vec<u32>,
    pub num_opaque: i32,
    pub num_transparent: i32,
}
impl FaceGroupQueue {
    /// LS3DF RVA 0x659a0 `ADD_FG_HELP_STRUCT::ctor` (mafia-re
    /// ls3df/I3D/I3D.h:4694-4698). Upstream working name, not original.
    pub fn new() -> Self {
        Self {
            unknown_0c: 0,
            fgroup_grow: 0x400,
            fgroups: Vec::new(),
            sort: Vec::new(),
            num_opaque: 0,
            num_transparent: 0,
        }
    }
    /// Empties the queue (mafia-re I3D.h:4701-4707, working name `Clear`).
    /// Helper phase, no function credit.
    pub fn clear_queue(&mut self) {
        self.num_opaque = 0;
        self.num_transparent = 0;
        self.fgroups.clear();
        self.sort.clear();
    }
}
/// lod::SetUV per-vertex offset add (PC RVA 0x36d00): host f32 adds with the
/// crate checked-add domain (x87 differences quarantined to the backend).
pub fn lod_uv_pair(uv: [f32; 2], ofs: [f32; 2]) -> Result<[f32; 2], ObjectLodError> {
    use crate::native_matrix::checked_f32_add as add;
    Ok([
        add(uv[0], ofs[0]).map_err(|_| ObjectLodError::NumericDomain)?,
        add(uv[1], ofs[1]).map_err(|_| ObjectLodError::NumericDomain)?,
    ])
}
/// Represented fields written by the plain PC lod constructor. Padding byte
/// 0x2f and the machine vtable pointer are deliberately not fabricated fields.
#[derive(Debug, PartialEq)]
pub struct Lod {
    pub lights_changed: u32,
    pub update_count: u32,
    pub light_key: u32,
    pub vertex_buffer: CompleteVertexBuffer,
    pub u_ofs: f32,
    pub v_ofs: f32,
    pub env_u_ofs: f32,
    pub env_v_ofs: f32,
    pub dirty: bool,
    pub alpha_dirty: bool,
    pub alpha: u8,
    /// Host ownership and cleanup state, separate from the native field layout.
    pub owner: Option<ObjectResourceKey>,
    pub state: LodState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectCleanupPhase {
    Alive,
    DestroyingLods,
    ReleasingMesh,
}
#[derive(Debug, PartialEq)]
pub struct I3dObjectResources {
    visual_type: ObjectVisualType,
    /// Raw native mesh pointer projection; it is not nulled during destruction.
    pub mesh: Option<MeshObjectKey>,
    /// Ordinary render state on this same owner; constructor values from source.
    pub cur_lod: u32,
    pub fade: f32,
    /// PC +0x210 (I3d_object.cpp:18, ctor 1.0). Bitwise copies, no arithmetic.
    pub lightness: f32,
    /// PC +0x214 (I3d_object.cpp:19, ctor 1.0). Bitwise copies, no arithmetic.
    pub transparency: f32,
    /// VISF_WORLD_SPACE 0x40 at +0x1d8 (I3D.h:2672). Host-owned bool; the
    /// native word shares vis_flags with other bits (see visual_flags).
    pub world_space: bool,
    /// Observed 0x100000 word at +0x1d8 in the SetSpace world arm
    /// (I3D.h:2688 names it VISF_INV_MAT_VALID; the reference spells
    /// WBOUND_VALID there — the port records the observed word).
    pub inv_matrix_valid: bool,
    /// Object-level visual flag word (base 0x2200 per I3D_visual ctor;
    /// VISF_OBJECT 0x800 etc. set by derived loaders). The frame-level
    /// visual_flags stay with the frame; this word travels with Duplicate.
    pub visual_flags: u32,
    /// Object bound volume (bbox + bsphere = native 10 dwords). Invalidated
    /// constructor box; bound-volume APIs recompute it.
    pub bounds: crate::mesh_bounds::MeshBoundVolume,
    /// All ten slots are cleared together only after all LOD deletes succeed.
    pub lods: [Option<PlainLodKey>; 10],
    pub phase: ObjectCleanupPhase,
    pub(crate) lit_object: Option<Box<LitObjectResources>>,
}
impl I3dObjectResources {
    pub fn visual_type(&self) -> ObjectVisualType {
        self.visual_type
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodBinding {
    pub lod: PlainLodKey,
    pub owner: VertexOwnerKey,
    pub state: BindingState,
}
/// Descriptive host arena, not an additional LS3DF DLL or full I3D_object class.
pub struct ObjectResources {
    managed: ManagedVisualResources,
    lods: Vec<Option<Lod>>,
    objects: Vec<Option<I3dObjectResources>>,
    bindings: Vec<LodBinding>,
    lit_storage: Vec<LitLodStorage>,
    poisoned: bool,
}
impl ObjectResources {
    pub fn new() -> Result<Self, ObjectError> {
        Ok(Self {
            managed: ManagedVisualResources::new()?,
            lods: Vec::new(),
            objects: Vec::new(),
            bindings: Vec::new(),
            lit_storage: Vec::new(),
            poisoned: false,
        })
    }
    pub fn managed(&self) -> &ManagedVisualResources {
        &self.managed
    }
    /// Counts saved-lighting LOD owners across all six levels, including inactive
    /// and interrupted load prefixes. Retired object slots are excluded.
    pub fn saved_lighting_lod_count(&self) -> usize {
        self.objects
            .iter()
            .flatten()
            .filter_map(I3dObjectResources::lit_object)
            .flat_map(|lit| lit.levels.iter())
            .map(|level| level.saved_lods.iter().flatten().count())
            .sum()
    }
    pub fn lod_bindings(&self) -> &[LodBinding] {
        &self.bindings
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned || self.managed.is_poisoned()
    }
    fn available(&self) -> Result<(), ObjectError> {
        if self.is_poisoned() {
            Err(ObjectError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn index(&self, arena: u64, slot: u32) -> Result<usize, ObjectError> {
        if arena != self.managed.resources().identity() {
            Err(ObjectError::ForeignArena)
        } else {
            slot.checked_sub(1)
                .map(|v| v as usize)
                .ok_or(ObjectError::IdentityOverflow)
        }
    }
    pub fn lod(&self, key: PlainLodKey) -> Result<&Lod, ObjectError> {
        self.lods
            .get(self.index(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(ObjectError::UnknownLod(key))
    }
    pub fn object(&self, key: ObjectResourceKey) -> Result<&I3dObjectResources, ObjectError> {
        self.objects
            .get(self.index(key.arena, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(ObjectError::UnknownObject(key))
    }
    fn reserve<T>(v: &mut Vec<T>) -> Result<u32, ObjectError> {
        let slot = u32::try_from(v.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(ObjectError::IdentityOverflow)?;
        v.try_reserve(1).map_err(|_| ObjectError::HostAllocation)?;
        Ok(slot)
    }
    pub fn create_plain_lod(&mut self) -> Result<PlainLodKey, ObjectError> {
        self.available()?;
        let slot = Self::reserve(&mut self.lods)?;
        self.lods.push(Some(Lod {
            lights_changed: u32::MAX,
            update_count: u32::MAX,
            light_key: u32::MAX,
            vertex_buffer: CompleteVertexBuffer::default(),
            u_ofs: 0.,
            v_ofs: 0.,
            env_u_ofs: 0.,
            env_v_ofs: 0.,
            dirty: false,
            alpha_dirty: false,
            alpha: 255,
            owner: None,
            state: LodState::Constructed,
        }));
        Ok(PlainLodKey {
            arena: self.managed.resources().identity(),
            slot,
        })
    }
    /// Fresh attachment retains exactly one shared mesh reference. This is a
    /// host intake adapter, not the original constructor or general SetMesh.
    pub fn create_object_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, ObjectError> {
        self.available()?;
        let slot = Self::reserve(&mut self.objects)?;
        if let Some(mesh) = mesh {
            self.managed.retain_mesh_object(mesh)?;
        }
        self.objects.push(Some(I3dObjectResources {
            visual_type: ObjectVisualType::Plain,
            mesh,
            cur_lod: 0,
            fade: 1.,
            lightness: 1.,
            transparency: 1.,
            world_space: false,
            inv_matrix_valid: false,
            visual_flags: 0x2200,
            bounds: crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR,
            lods: [None; 10],
            phase: ObjectCleanupPhase::Alive,
            lit_object: None,
        }));
        Ok(ObjectResourceKey {
            arena: self.managed.resources().identity(),
            slot,
        })
    }
    /// Working name for the direct `cur_lod` member read on `I3D_object`
    /// (mafia-re ls3df/I3D/I3D.h carries `cur_lod` as ordinary render state
    /// beside the mesh owner). The original reads the public member with no
    /// accessor, so no LS3DF FUNCTION address exists and none is claimed.
    pub fn object_cur_lod(&self, object: ObjectResourceKey) -> Result<u32, ObjectError> {
        Ok(self.object(object)?.cur_lod)
    }
    /// Working name for the LS3DF-inlined `I3D_visual::GetVisualType`
    /// (mafia-re ls3df/I3D/I3D.h:2748-2760, LS3DF_EXPORTS by-value branch).
    /// No LS3DF FUNCTION address exists for it; the only out-of-line copy is
    /// the game build by-reference one (Game.exe 0x47ad70), which is not
    /// claimed here. Reports the stored four-character visual type word.
    pub fn visual_type_word(&self, object: ObjectResourceKey) -> Result<u32, ObjectError> {
        Ok(self.object(object)?.visual_type().source_fourcc())
    }
    /// Working name for the LS3DF-inlined `I3D_object::GetMesh`
    /// (mafia-re ls3df/I3D/I3D.h:2896-2908, LS3DF_EXPORTS by-value branch).
    /// No LS3DF FUNCTION address exists for it; the only out-of-line copy is
    /// the game build by-reference one (Game.exe 0x47ad80), which is not
    /// claimed here. A missing mesh reads as None (native NULL).
    pub fn object_mesh_handle(
        &self,
        object: ObjectResourceKey,
    ) -> Result<Option<MeshObjectKey>, ObjectError> {
        Ok(self.object(object)?.mesh)
    }
    /// Working name for the LS3DF-inlined `I3D_object::GetMeshLOD`
    /// (mafia-re ls3df/I3D/I3D.h:2910-2916). No LS3DF FUNCTION address
    /// exists for it and the reference marks no Game.exe out-of-line copy
    /// either. Texture-LOD visuals (VISF_TEXTURE_LOD, 0x10) always render
    /// level 0; the others render the stored `cur_lod`.
    pub fn object_mesh_lod_index(&self, object: ObjectResourceKey) -> Result<u32, ObjectError> {
        let record = self.object(object)?;
        Ok(if record.visual_flags & 0x10 != 0 {
            0
        } else {
            record.cur_lod
        })
    }
    /// Working name for the direct `fade` member read on `I3D_object`
    /// (mafia-re ls3df/I3D/I3D.h carries `fade` as ordinary render state
    /// beside `cur_lod`). The original reads the public member with no
    /// accessor, so no LS3DF FUNCTION address exists and none is claimed.
    pub fn object_fade(&self, object: ObjectResourceKey) -> Result<f32, ObjectError> {
        Ok(self.object(object)?.fade)
    }
    /// Working name for the direct `lightness` member read on `I3D_object`
    /// (mafia-re ls3df/I3D/I3D.h:2922 `float lightness` at +0x210, the
    /// factor PS2 `SetLightness` scales the lights with). The original
    /// reads the public member with no accessor, so no LS3DF FUNCTION
    /// address exists and none is claimed. Reports the stored factor;
    /// same availability guard as `object_fade`.
    pub fn object_lightness_of(&self, object: ObjectResourceKey) -> Result<f32, ObjectError> {
        Ok(self.object(object)?.lightness)
    }
    /// Working name for the direct `transparency` member read on `I3D_object`
    /// (mafia-re ls3df/I3D/I3D.h:2923 `float transparency` at +0x214, 1.0
    /// opaque, written by `SetTransparency`). The original reads the public
    /// member with no accessor, so no LS3DF FUNCTION address exists and none
    /// is claimed. Reports the stored factor; same availability guard as
    /// `object_lightness_of`.
    pub fn object_transparency_of(&self, object: ObjectResourceKey) -> Result<f32, ObjectError> {
        Ok(self.object(object)?.transparency)
    }
    /// I3D_singlemesh and I3D_singlemorph inherit I3D_object mesh ownership.
    /// The graph separately owns joint binding, morph weights and CPU pose.
    pub fn create_singlemesh_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
        morph: bool,
    ) -> Result<ObjectResourceKey, ObjectError> {
        let key = self.create_object_resources(mesh)?;
        self.objects[(key.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .visual_type = if morph {
            ObjectVisualType::Singlemorph
        } else {
            ObjectVisualType::Singlemesh
        };
        Ok(key)
    }
    /// I3D_morph inherits ordinary mesh/LOD ownership. Its live morphBase
    /// weights and retained source definition belong to the owning frame.
    pub fn create_morph_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, ObjectError> {
        let key = self.create_object_resources(mesh)?;
        self.objects[(key.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .visual_type = ObjectVisualType::Morph;
        Ok(key)
    }
    /// Native I3D_billboard inherits I3D_object mesh and LOD teardown. Its
    /// orientation and camera matrix are held by the owning live Frame.
    pub fn create_billboard_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, ObjectError> {
        let key = self.create_object_resources(mesh)?;
        self.objects[(key.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .visual_type = ObjectVisualType::Billboard;
        Ok(key)
    }
    /// Unique transfer into one empty slot. Signed-negative native pointer
    /// indices are outside this safe host adapter, as are slots beyond nine.
    pub fn adopt_plain_lod(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
        lod: PlainLodKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        if index > i32::MAX as u32 {
            return Err(ObjectError::UnsupportedIndex(index));
        }
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index));
        }
        if !matches!(
            self.object(object)?.visual_type(),
            ObjectVisualType::Plain | ObjectVisualType::Morph
        ) {
            return Err(ObjectError::UnsupportedVisualSubtype);
        }
        if self.object(object)?.lods[index as usize].is_some() {
            return Err(ObjectError::SlotOccupied);
        }
        let current = self.lod(lod)?;
        if current.owner.is_some() {
            return Err(ObjectError::LodOwned(lod));
        }
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod));
        }
        self.lods[(lod.slot - 1) as usize].as_mut().unwrap().owner = Some(object);
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lods[index as usize] = Some(lod);
        Ok(())
    }
    /// Commit the native MRPH lod update-count field on its actual owned slot.
    /// CPU publication failures leave the prior count available for inspection.
    pub(crate) fn commit_morph_lod_update_count(
        &mut self,
        lod: PlainLodKey,
        count: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        let owner = self
            .lod(lod)?
            .owner
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        if self.object(owner)?.visual_type() != ObjectVisualType::Morph {
            return Err(ObjectError::UnsupportedVisualSubtype);
        }
        self.lods[(lod.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .update_count = count;
        Ok(())
    }
    /// Fresh managed storage only; no original lod::Create or upload claim.
    pub fn allocate_plain_lod_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        request: VertexRequest,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        let current = self.lod(lod)?;
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod).into());
        }
        if current.vertex_buffer != CompleteVertexBuffer::default() {
            return Err(ObjectError::StorageAlreadyInstalled.into());
        }
        if request.count == 0 || request.count > i32::MAX as u32 {
            return Err(ManagedError::InvalidExtent.into());
        }
        Self::reserve(&mut self.bindings)?;
        let position = if let Some(i) = self
            .bindings
            .iter()
            .position(|b| b.lod == lod && b.state != BindingState::Consumed)
        {
            if self.bindings[i].state != BindingState::Pending {
                return Err(ObjectError::StorageAlreadyInstalled.into());
            }
            i
        } else {
            let owner = self.managed.create_vertex_storage_owner()?;
            self.bindings.push(LodBinding {
                lod,
                owner,
                state: BindingState::Pending,
            });
            self.bindings.len() - 1
        };
        let owner = self.bindings[position].owner;
        let result = managed_result(
            self.managed
                .allocate_vertex_storage(owner, request, backend),
        );
        match result {
            Ok(Some(block)) => {
                self.lods[(lod.slot - 1) as usize]
                    .as_mut()
                    .unwrap()
                    .vertex_buffer = block;
                self.bindings[position].state = BindingState::Installed;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }
    fn cleanup_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        let current = self.lod(lod)?;
        if current.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(lod).into());
        }
        let descriptor = current.vertex_buffer;
        let position = self
            .bindings
            .iter()
            .position(|b| b.lod == lod && b.state != BindingState::Consumed);
        let owner = position.map(|i| self.bindings[i].owner);
        let result = managed_result(self.managed.free_vertex_storage(owner, descriptor, backend));
        if let Err(e) = result {
            self.poisoned = true;
            return Err(e);
        }
        if let Some(i) = position {
            self.bindings[i].state = BindingState::Consumed;
        }
        self.lods[(lod.slot - 1) as usize].as_mut().unwrap().state = LodState::Destructed;
        Ok(())
    }
    /// Plain nondeleting cleanup. Descriptor words remain stale, as natively;
    /// host state prevents a second cleanup or adoption of the destructed record.
    pub fn destruct_plain_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        if self.lod(lod)?.owner.is_some() {
            return Err(ObjectError::LodOwned(lod).into());
        }
        self.cleanup_lod(lod, backend)
    }
    /// Cleanup plus host record retirement. No native allocator/pointer return
    /// is represented. An already nondeleting-destructed record is rejected.
    pub fn destroy_plain_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        lod: PlainLodKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.destruct_plain_lod(lod, backend)?;
        self.lods[(lod.slot - 1) as usize] = None;
        Ok(())
    }
    /// All ten unique LOD deletes precede the single whole-array clear. Mesh
    /// Release follows with its raw pointer unchanged; visual/frame base cleanup
    /// remains outside this ownership projection. A failure stops every mutation.
    /// Provenance: PC RVA 0x36840 (ten-LOD/mesh ownership prefix) and the lit
    /// derived-dtor confirm-only body PC RVA 0x21600 (differential: Ghidra body
    /// 159 B, 46 insn/7 BB — vtable stores, 6x10 null-guarded deleting-dtor
    /// calls stride 0x38, tail CALL 0x10036840; mafia-re I3d_lit_object.cpp:
    /// 53-63) with derived-before-base order pinned by
    /// lit_dtor_teardown_retires_all_levels_and_mesh.
    pub fn destroy_object_resources<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let current = self.object(object)?;
        // The private arena maintains unique slots and live mesh ownership.
        // Preflight all keys before the first manager/backend effect.
        for lod in current.lods.iter().flatten().chain(
            current
                .lit_object
                .iter()
                .flat_map(|lit| lit.levels.iter())
                .flat_map(|level| level.lods.iter().flatten()),
        ) {
            let state = self.lod(*lod)?;
            if state.owner != Some(object) {
                return Err(ObjectError::LodOwned(*lod).into());
            }
            if state.state != LodState::Constructed {
                return Err(ObjectError::LodDestructed(*lod).into());
            }
        }
        if let Some(mesh) = current.mesh {
            self.managed
                .resources()
                .mesh_object(mesh)
                .map_err(ManagedError::from)?;
        }
        let has_lit = current.lit_object.is_some();
        let index = (object.slot - 1) as usize;
        self.objects[index].as_mut().unwrap().phase = ObjectCleanupPhase::DestroyingLods;
        let result = (|| {
            // Derived destructor runs before I3D_object base cleanup.
            if has_lit {
                for level in 0..6 {
                    for lod_index in 0..10 {
                        let lod = self.objects[index]
                            .as_ref()
                            .unwrap()
                            .lit_object
                            .as_ref()
                            .unwrap()
                            .levels[level]
                            .lods[lod_index];
                        if let Some(lod) = lod {
                            self.cleanup_saved_lit_lod(object, level, lod_index, backend)?;
                            self.lods[(lod.slot - 1) as usize] = None;
                            self.objects[index]
                                .as_mut()
                                .unwrap()
                                .lit_object
                                .as_mut()
                                .unwrap()
                                .levels[level]
                                .lods[lod_index] = None;
                        }
                        self.objects[index]
                            .as_mut()
                            .unwrap()
                            .lit_object
                            .as_mut()
                            .unwrap()
                            .levels[level]
                            .saved_lods[lod_index] = None;
                    }
                }
            }
            for i in 0..10 {
                if let Some(lod) = self.objects[index].as_ref().unwrap().lods[i] {
                    self.cleanup_lod(lod, backend)?;
                    self.lods[(lod.slot - 1) as usize] = None;
                }
            }
            let record = self.objects[index].as_mut().unwrap();
            record.lods = [None; 10];
            record.phase = ObjectCleanupPhase::ReleasingMesh;
            if let Some(mesh) = record.mesh {
                managed_result(self.managed.release_mesh_object(mesh, backend))?;
            }
            self.objects[index] = None;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn owned_mesh_references(&self, mesh: MeshObjectKey) -> usize {
        self.objects
            .iter()
            .flatten()
            .filter(|o| o.mesh == Some(mesh) && o.phase != ObjectCleanupPhase::ReleasingMesh)
            .count()
    }
    /// External Release cannot consume a reference retained by a live object.
    pub fn release_mesh_object<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        mesh: MeshObjectKey,
        backend: &mut B,
    ) -> Result<u32, ObjectOperationError<B::Error>> {
        self.available()?;
        let total = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(ManagedError::from)?
            .references;
        if u64::from(total) <= self.owned_mesh_references(mesh) as u64 {
            return Err(ObjectError::MeshReferenceOwned(mesh).into());
        }
        managed_result(self.managed.release_mesh_object(mesh, backend))
    }
}

impl ObjectResources {
    pub fn set_material_driver_time(&mut self, time: i32) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_driver_time(time)
            .map_err(Into::into)
    }
    pub fn get_material_texture(
        &mut self,
        key: MaterialKey,
        random: &mut crate::frame_light::LightCrtRandom,
    ) -> Result<Option<TextureKey>, ObjectError> {
        self.available()?;
        self.managed
            .get_material_texture(key, random)
            .map_err(Into::into)
    }
    pub fn get_material_env_texture(
        &mut self,
        key: MaterialKey,
        random: &mut crate::frame_light::LightCrtRandom,
    ) -> Result<Option<TextureKey>, ObjectError> {
        self.available()?;
        self.managed
            .get_material_env_texture(key, random)
            .map_err(Into::into)
    }
    pub fn set_material_current_frame(
        &mut self,
        key: MaterialKey,
        frame: i32,
        environment: bool,
    ) -> Result<bool, ObjectError> {
        self.available()?;
        self.managed
            .set_material_current_frame(key, frame, environment)
            .map_err(Into::into)
    }
    pub fn create_material(&mut self) -> Result<MaterialKey, ObjectError> {
        self.available()?;
        self.managed.create_material().map_err(Into::into)
    }
    pub fn create_animated_material(&mut self) -> Result<MaterialKey, ObjectError> {
        self.available()?;
        self.managed.create_animated_material().map_err(Into::into)
    }
    pub fn create_face_group(&mut self) -> Result<FaceGroupKey, ObjectError> {
        self.available()?;
        self.managed.create_face_group().map_err(Into::into)
    }
    pub fn create_mesh_level(&mut self) -> Result<MeshLevelKey, ObjectError> {
        self.available()?;
        self.managed.create_mesh_level().map_err(Into::into)
    }
    pub fn create_mesh_object(&mut self) -> Result<MeshObjectKey, ObjectError> {
        self.available()?;
        self.managed.create_mesh_object().map_err(Into::into)
    }
    pub fn retain_material(&mut self, key: MaterialKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.retain_material(key).map_err(Into::into)
    }
    pub fn retain_mesh_level(&mut self, key: MeshLevelKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.retain_mesh_level(key).map_err(Into::into)
    }
    pub fn retain_mesh_object(&mut self, key: MeshObjectKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.retain_mesh_object(key).map_err(Into::into)
    }
    pub fn link_next(
        &mut self,
        group: FaceGroupKey,
        next: FaceGroupKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.link_next(group, next).map_err(Into::into)
    }
    pub fn install_mesh_level_bbox(
        &mut self,
        key: MeshLevelKey,
        bbox: [[f32; 3]; 2],
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .install_mesh_level_bbox(key, bbox)
            .map_err(Into::into)
    }
    pub fn update_mesh_bound_volume(&mut self, key: MeshObjectKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .update_mesh_bound_volume(key)
            .map_err(Into::into)
    }
    pub fn allocate_group_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        num_faces: u32,
        usage: u32,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(
            self.managed
                .allocate_group_indices(group, num_faces, usage, backend),
        )
    }
    pub fn allocate_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        num_vertices: u32,
        request: VertexRequest,
        head: Option<FaceGroupKey>,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.allocate_level_vertices(
            level,
            num_vertices,
            request,
            head,
            backend,
        ))
    }
    pub fn release_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        backend: &mut B,
    ) -> Result<u32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.release_material(key, backend))
    }
    pub fn release_mesh_level<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshLevelKey,
        backend: &mut B,
    ) -> Result<u32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.release_mesh_level(key, backend))
    }
    pub fn destroy_face_group<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.destroy_face_group(group, backend))
    }
    pub fn set_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        material: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_material(group, material, backend))
    }
    pub fn create_faces<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        num: u32,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.create_faces(group, num, backend))
    }
    pub fn set_faces<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        faces: Option<&[I3dTriface]>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_faces(group, faces, backend))
    }
    pub fn get_faces<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<Option<&[I3dTriface]>, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.get_faces(group, backend))
    }
    pub fn unlock_faces<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.unlock_faces(group, backend))
    }
    pub fn set_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_texture(key, texture, backend))
    }
    pub fn set_env_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_env_texture(key, texture, backend))
    }
    pub fn set_env_texture_ex<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        blend: u32,
        mode: u32,
        alpha: f32,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(
            self.managed
                .set_env_texture_ex(key, texture, blend, mode, alpha, backend),
        )
    }
    pub fn set_material_ambient(
        &mut self,
        key: MaterialKey,
        color: [f32; 3],
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_ambient(key, color)
            .map_err(Into::into)
    }
    pub fn set_material_diffuse(
        &mut self,
        key: MaterialKey,
        color: [f32; 3],
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_diffuse(key, color)
            .map_err(Into::into)
    }
    pub fn set_material_emissive(
        &mut self,
        key: MaterialKey,
        color: [f32; 3],
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_emissive(key, color)
            .map_err(Into::into)
    }
    pub fn set_material_alpha(&mut self, key: MaterialKey, alpha: f32) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_alpha(key, alpha)
            .map_err(Into::into)
    }
    pub fn set_material_two_sided(&mut self, key: MaterialKey) -> Result<(), ObjectError> {
        self.available()?;
        self.managed.set_material_two_sided(key).map_err(Into::into)
    }
    pub fn set_material_env_type(
        &mut self,
        key: MaterialKey,
        blend: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_env_type(key, blend)
            .map_err(Into::into)
    }
    pub fn set_material_env_mode(
        &mut self,
        key: MaterialKey,
        mode: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_env_mode(key, mode)
            .map_err(Into::into)
    }
    pub fn set_material_env_scale(
        &mut self,
        key: MaterialKey,
        scale: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .set_material_env_scale(key, scale)
            .map_err(Into::into)
    }
    pub fn set_lod<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        level: Option<MeshLevelKey>,
        index: u32,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.set_lod(key, level, index, backend))
    }
    pub fn get_lod(
        &self,
        key: MeshObjectKey,
        index: u32,
    ) -> Result<Option<MeshLevelKey>, ObjectError> {
        self.available()?;
        self.managed.get_lod(key, index).map_err(Into::into)
    }
    pub fn is_mesh_compatible(
        &self,
        first: MeshObjectKey,
        second: MeshObjectKey,
    ) -> Result<bool, ObjectError> {
        self.available()?;
        self.managed
            .is_mesh_compatible(first, second)
            .map_err(Into::into)
    }
    pub fn mesh_object_stats(
        &self,
        key: MeshObjectKey,
    ) -> Result<crate::visual_resources::I3dMeshStats, ObjectError> {
        self.available()?;
        self.managed.mesh_object_stats(key).map_err(Into::into)
    }
    pub fn detach_mesh_materials<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MeshObjectKey,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.detach_mesh_materials(key, backend))
    }
    pub fn duplicate_mesh_object<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        destination: MeshObjectKey,
        source: Option<MeshObjectKey>,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(
            self.managed
                .duplicate_mesh_object(destination, source, backend),
        )
    }
    pub fn reclaim_unused_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.reclaim_unused_indices(backend))
    }
    pub fn reclaim_unused_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.reclaim_unused_vertices(backend))
    }
    pub fn unlock_all_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.unlock_all_indices(backend))
    }
    pub fn unlock_all_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.unlock_all_vertices(backend))
    }
}

impl ObjectResources {
    pub(crate) fn copy_mesh_level_scalar_fields(
        &mut self,
        destination: MeshLevelKey,
        source: MeshLevelKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .copy_mesh_level_scalar_fields(destination, source)
            .map_err(Into::into)
    }
    pub(crate) fn reset_empty_mesh_bounds(
        &mut self,
        mesh: MeshObjectKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .reset_empty_mesh_bounds(mesh)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub(crate) fn clear_duplicate_mesh<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        mesh: MeshObjectKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.clear_duplicate_mesh(mesh, backend))
    }
    pub(crate) fn copy_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        destination: MaterialKey,
        source: Option<MaterialKey>,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.copy_material(destination, source, backend))
    }
    pub fn duplicate_face_group<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        destination: FaceGroupKey,
        source: FaceGroupKey,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(
            self.managed
                .duplicate_face_group(destination, source, backend),
        )
    }
    pub fn detach_face_group_material<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.detach_face_group_material(group, backend))
    }
    pub fn face_group_at(
        &self,
        level: MeshLevelKey,
        index: i32,
    ) -> Result<Option<FaceGroupKey>, ObjectError> {
        self.available()?;
        self.managed.face_group_at(level, index).map_err(Into::into)
    }
    pub fn count_mesh_level_faces(&self, level: MeshLevelKey) -> Result<u32, ObjectError> {
        self.available()?;
        self.managed
            .count_mesh_level_faces(level)
            .map_err(Into::into)
    }
    pub fn mesh_level_stats(
        &self,
        level: MeshLevelKey,
    ) -> Result<crate::visual_resources::I3dMeshStats, ObjectError> {
        self.available()?;
        self.managed.mesh_level_stats(level).map_err(Into::into)
    }
    pub fn add_mesh_level_group(
        &mut self,
        level: MeshLevelKey,
        group: FaceGroupKey,
        last: bool,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .add_mesh_level_group(level, group, last)
            .map_err(Into::into)
    }
    pub fn update_mesh_level_bbox_from_bytes(
        &mut self,
        level: MeshLevelKey,
        bytes: &[u8],
        stride: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .update_mesh_level_bbox_from_bytes(level, bytes, stride)
            .map_err(Into::into)
    }
    pub fn set_mesh_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        verts: Option<&[u8]>,
        num: u32,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(
            self.managed
                .set_mesh_level_vertices(level, verts, num, backend),
        )
    }
    pub fn create_mesh_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        num: u32,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.create_mesh_level_vertices(level, num, backend))
    }
    pub fn duplicate_mesh_level<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        destination: MeshLevelKey,
        source: Option<MeshLevelKey>,
        backend: &mut B,
    ) -> Result<i32, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(
            self.managed
                .duplicate_mesh_level(destination, source, backend),
        )
    }
}

impl ObjectResources {
    /// Resource-owning effects of object SetMesh368e0. The arena caller owns
    /// visual flags and the visual bbox copy/reset after this prefix.
    pub(crate) fn set_object_mesh<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        mesh: Option<MeshObjectKey>,
        preserve_lods: bool,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        if !matches!(
            self.object(object)?.visual_type(),
            ObjectVisualType::Plain | ObjectVisualType::Billboard
        ) {
            return Err(ObjectError::UnsupportedVisualSubtype.into());
        }
        let old = self.object(object)?.mesh;
        if old == mesh {
            return Ok(());
        }
        if let Some(mesh) = mesh {
            self.managed
                .resources()
                .mesh_object(mesh)
                .map_err(ManagedError::from)?;
        }
        if preserve_lods && mesh.is_some() {
            return Err(ObjectError::MeshCompatibilityUnavailable.into());
        }
        let index = (object.slot - 1) as usize;
        let result = (|| {
            {
                for slot in 0..10 {
                    if let Some(lod) = self.objects[index].as_ref().unwrap().lods[slot] {
                        self.cleanup_lod(lod, backend)?;
                        self.lods[(lod.slot - 1) as usize] = None;
                    }
                }
            }
            self.objects[index].as_mut().unwrap().lods = [None; 10];
            if let Some(old) = old {
                managed_result(self.managed.release_mesh_object(old, backend))?;
            }
            self.objects[index].as_mut().unwrap().mesh = mesh;
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
}

impl ObjectResources {
    pub(crate) fn append_duplicate_group(
        &mut self,
        level: MeshLevelKey,
        group: FaceGroupKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .append_duplicate_group(level, group)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub fn install_loaded_material_scalars(
        &mut self,
        key: MaterialKey,
        source: &lhr_assets::Material,
        flags: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .install_loaded_material_scalars(key, source, flags)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub(crate) fn install_loaded_lod_distance(
        &mut self,
        level: MeshLevelKey,
        distance: f32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .install_loaded_lod_distance(level, distance)
            .map_err(Into::into)
    }
}

impl ObjectResources {
    pub(crate) fn configure_loaded_animated_material(
        &mut self,
        key: MaterialKey,
        source: &lhr_assets::Material,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .configure_loaded_animated_material(key, source)
            .map_err(Into::into)
    }
    pub(crate) fn append_loaded_animated_frame<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        environment: bool,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.append_loaded_animated_frame(
            key,
            texture,
            environment,
            backend,
        ))
    }
    pub(crate) fn finish_loaded_animated_material(
        &mut self,
        key: MaterialKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .finish_loaded_animated_material(key)
            .map_err(Into::into)
    }
    pub(crate) fn adopt_loaded_material_texture<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        key: MaterialKey,
        texture: Option<TextureKey>,
        environment: bool,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.adopt_loaded_material_texture(
            key,
            texture,
            environment,
            backend,
        ))
    }
}

impl ObjectResources {
    pub(crate) fn lock_mesh_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        flags: u32,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.lock_level_vertices(level, flags, backend))
    }
    pub(crate) fn unlock_mesh_level_vertices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        level: MeshLevelKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.unlock_level_vertices(level, backend))
    }
    pub(crate) fn update_object_vertices(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
    ) -> Result<(), ObjectError> {
        self.available()?;
        if self.object(object)?.visual_type() != ObjectVisualType::Plain {
            return Err(ObjectError::UnsupportedVisualSubtype);
        }
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index));
        }
        if let Some(lod) = self.object(object)?.lods[index as usize] {
            let index = self.index(lod.arena, lod.slot)?;
            self.lods[index]
                .as_mut()
                .ok_or(ObjectError::UnknownLod(lod))?
                .dirty = true;
        }
        Ok(())
    }
    /// Original SetMesh keep_lods arm, PC RVA 0x368e0 (differential: Ghidra
    /// body 307 B, 101 insn/25 BB; mafia-re I3d_object.cpp:39-67). Closes the
    /// preserve_lods gap in set_object_mesh with the evidenced IsCompatible
    /// gate: same-pointer returns Ok(false) with no effects; keep holds only
    /// for (Some(old), Some(new)) compatible pairs, else the 10-lod delete
    /// runs. Returns whether the LOD array was cleared. Slot indexing uses
    /// the validating self.index() (slot 0 can never underflow here — the
    /// object was just read). Poison mirrors set_object_mesh on any closure
    /// error including backend failures (the binary has no sticky-fault word
    /// — host admission, same precedent). Plain/Billboard gate mirrors
    /// set_object_mesh.
    pub fn set_object_mesh_keep_compatible<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        mesh: Option<MeshObjectKey>,
        preserve_lods: bool,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        if !matches!(
            self.object(object)?.visual_type(),
            ObjectVisualType::Plain | ObjectVisualType::Billboard
        ) {
            return Err(ObjectError::UnsupportedVisualSubtype.into());
        }
        let old = self.object(object)?.mesh;
        if old == mesh {
            return Ok(false);
        }
        if let Some(mesh) = mesh {
            self.managed
                .resources()
                .mesh_object(mesh)
                .map_err(ManagedError::from)?;
        }
        let keep = if preserve_lods {
            match (old, mesh) {
                (Some(previous), Some(next)) => self.is_mesh_compatible(next, previous)?,
                _ => false,
            }
        } else {
            false
        };
        let index = self.index(object.arena, object.slot)?;
        let result = (|| {
            if !keep {
                for slot in 0..10 {
                    if let Some(lod) = self.objects[index].as_ref().unwrap().lods[slot] {
                        self.cleanup_lod(lod, backend)?;
                        self.lods[(lod.slot - 1) as usize] = None;
                    }
                }
                self.objects[index].as_mut().unwrap().lods = [None; 10];
            }
            if let Some(old) = old {
                managed_result(self.managed.release_mesh_object(old, backend))?;
            }
            self.objects[index].as_mut().unwrap().mesh = mesh;
            if let Some(mesh) = mesh {
                self.managed.retain_mesh_object(mesh)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result.map(|()| !keep)
    }
    /// Original UpdateVertices(), PC RVA 0x36a20 (differential: Ghidra body
    /// 35 B, 12 insn/5 BB; mafia-re I3d_object.cpp:71-77). Marks all 10
    /// present slots dirty (+0x2c). No mesh read, no failure word. Returns
    /// the marked count.
    pub fn mark_all_object_lods_dirty(
        &mut self,
        object: ObjectResourceKey,
    ) -> Result<u32, ObjectError> {
        self.available()?;
        let mut marked = 0u32;
        for slot in 0..10 {
            if let Some(lod) = self.object(object)?.lods[slot] {
                let index = self.index(lod.arena, lod.slot)?;
                self.lods[index]
                    .as_mut()
                    .ok_or(ObjectError::UnknownLod(lod))?
                    .dirty = true;
                marked += 1;
            }
        }
        Ok(marked)
    }
    /// Original DetachMesh(), PC RVA 0x36a70 (differential: Ghidra body 150 B,
    /// 57 insn/8 BB; mafia-re I3d_object.cpp:89-108). Effect order: shared
    /// mesh (refs>=2) else Ok(false); driver CreateMesh + Duplicate are
    /// backend contracts (no live D3D8 device); the fatal OOM word
    /// 0x80000002 surfaces HostAllocation, other negative duplicate codes
    /// surface MeshCompatibilityUnavailable (no closer host error exists —
    /// disclosed); else release old, store new.
    /// Native words: 0 ok, 0x80000002 fatal OOM, negative passthrough.
    pub fn detach_object_mesh_to_unique<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        let mesh = self.object(object)?.mesh;
        let Some(mesh) = mesh else {
            return Ok(false);
        };
        let references = self
            .managed
            .resources()
            .mesh_object(mesh)
            .map_err(ManagedError::from)?
            .references;
        if references < 2 {
            return Ok(false);
        }
        let fresh = self.managed.create_mesh_object()?;
        let code = managed_result(
            self.managed
                .duplicate_mesh_object(fresh, Some(mesh), backend),
        )?;
        if code == 0x8000_0002u32 as i32 {
            // Fatal native OOM word: distinct from a backend passthrough
            // failure below (duplicate_mesh_object surfaces it verbatim).
            managed_result(self.managed.release_mesh_object(fresh, backend))?;
            return Err(ObjectError::HostAllocation.into());
        }
        if code < 0 {
            managed_result(self.managed.release_mesh_object(fresh, backend))?;
            return Err(ObjectError::MeshCompatibilityUnavailable.into());
        }
        // A failed old-mesh release keeps the old mesh stored; the fresh mesh
        // is released so neither leaks (backend may still have effects —
        // disclosed, same class as the interruption prefix rule).
        managed_result(self.managed.release_mesh_object(mesh, backend)).inspect_err(|_| {
            let _ = self.managed.release_mesh_object(fresh, backend);
        })?;
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .mesh = Some(fresh);
        Ok(true)
    }
    /// Original Duplicate object-field prefix, PC RVA 0x36b10 (differential:
    /// Ghidra body 346 B, 100 insn/27 BB; mafia-re I3d_object.cpp:112-153).
    /// Copies lightness/transparency/fade bitwise, visual flags masked with
    /// VISF_DUPLICATE_MASK 0x2ffd0, the bound volume, world_space, and clears
    /// inv_matrix_valid. SOLE owner of the Duplicate scalar block: lit callers
    /// use this for scalars plus their lighting-settings copy (never a split
    /// word set). The frame-base Duplicate and SetMesh run before/around this
    /// (owned by callers); the emitor branch resolves through
    /// set_object_mesh(keep=true). Accepted visual types are the six fourccs
    /// mapped by source_fourcc (other native fourccs have no arena variant —
    /// the closed enum keeps this total and explicit). World-bound recompute
    /// travels with the bound-volume APIs; the snapshot carries the plain
    /// bound (disclosed).
    pub fn duplicate_object_render_state(
        &mut self,
        destination: ObjectResourceKey,
        source: ObjectResourceKey,
    ) -> Result<DuplicateObjectFields, ObjectError> {
        self.available()?;
        self.object(destination)?;
        let src = self.object(source)?;
        const ALLOW: [u32; 6] = [
            0x5f4a424f, 0x50414d4c, 0x4d474e53, 0x4850524d, 0x524d4d53, 0x44524242,
        ];
        if !ALLOW.contains(&src.visual_type().source_fourcc()) {
            return Err(ObjectError::UnsupportedVisualSubtype);
        }
        let snapshot = DuplicateObjectFields {
            lightness: src.lightness,
            transparency: src.transparency,
            fade: src.fade,
            visual_flags_masked: src.visual_flags & DUPLICATE_VIS_MASK,
            bounds: src.bounds,
            world_space: src.world_space,
        };
        let di = (destination.slot - 1) as usize;
        let dst = self.objects[di]
            .as_mut()
            .ok_or(ObjectError::UnknownObject(destination))?;
        dst.lightness = snapshot.lightness;
        dst.transparency = snapshot.transparency;
        dst.fade = snapshot.fade;
        dst.visual_flags = snapshot.visual_flags_masked;
        dst.bounds = snapshot.bounds;
        dst.world_space = snapshot.world_space;
        dst.inv_matrix_valid = false;
        Ok(snapshot)
    }
    /// Original lod::SetUV CPU policy, PC RVA 0x36d00 (differential: Ghidra
    /// body 806 B, 270 insn/54 BB; mafia-re I3d_object.cpp:181-241). Stores
    /// u_ofs/v_ofs FIRST (native order), masks format&=3, and returns the
    /// vertex stride in floats for the (format, hwtl) pair. The VB Lock/copy
    /// upload is a backend contract (no live device); offsets store
    /// unconditionally like the binary (no domain gate). hwtl is caller
    /// state (no arena hwtl flag exists — documented parameter).
    pub fn store_lod_uv_offsets(
        &mut self,
        lod: PlainLodKey,
        format: u32,
        uo: f32,
        vo: f32,
        hwtl: bool,
    ) -> Result<(u32, u32), ObjectError> {
        self.available()?;
        let i = self.index(lod.arena, lod.slot)?;
        let record = self.lods[i].as_mut().ok_or(ObjectError::UnknownLod(lod))?;
        record.u_ofs = uo;
        record.v_ofs = vo;
        Ok((
            format & 3,
            match (format & 3, hwtl) {
                (1, true) => 6,
                (2, true) => 8,
                (1, false) => 8,
                _ => 10,
            },
        ))
    }
    /// Original lod::Create CPU tail, PC RVA 0x37030 (differential: Ghidra
    /// body 2165 B, 709 insn/137 BB; mafia-re I3d_object.cpp:247-451). The
    /// VB alloc + vertex fill are backend contracts (alloc words and FVF
    /// selectors need a second pass against D3D8 constants — disclosed).
    /// This tail records the format mask + stride selection and the terminal
    /// dirty=false store on the live record. hwtl is caller state (see
    /// store_lod_uv_offsets).
    pub fn prepare_plain_lod_create(
        &mut self,
        lod: PlainLodKey,
        format: u32,
        hwtl: bool,
    ) -> Result<LodCreateParams, ObjectError> {
        self.available()?;
        let i = self.index(lod.arena, lod.slot)?;
        let needs_alloc = self.lods[i]
            .as_ref()
            .ok_or(ObjectError::UnknownLod(lod))?
            .vertex_buffer
            .buffer
            .is_none();
        let format = format & 7;
        let stride_floats = match (format, hwtl) {
            (1, true) => 6,
            (2, true) => 8,
            (1, false) => 8,
            _ => 10,
        };
        self.lods[i].as_mut().unwrap().dirty = false;
        Ok(LodCreateParams {
            format,
            stride_floats,
            needs_alloc,
        })
    }
    /// Original m_AddFGroups, PC RVA 0x38020 (contract: Ghidra formed no
    /// body — unformed gap 0x100378a5-0x10037dcf; mafia-re I3d_object.cpp:
    /// 594-852 reference only). Boundary marker: proxy-sector walk, W-bound
    /// distance, LOD-create gate, lighting/env/vertex processing and the
    /// sort-value queue are unevidenced here; every D3D8 call stays a backend
    /// contract. Always reports the boundary (no parity credit).
    pub fn queue_object_face_groups_contract(
        &self,
        _object: ObjectResourceKey,
    ) -> Result<ObjectFaceGroupContract, ObjectError> {
        self.available()?;
        Err(ObjectError::UnsupportedVisualSubtype)
    }
}

impl ObjectResources {
    pub(crate) fn lock_face_group_indices<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        group: FaceGroupKey,
        backend: &mut B,
    ) -> Result<bool, ObjectOperationError<B::Error>> {
        self.available()?;
        managed_result(self.managed.lock_group_indices(group, backend))
    }
}

impl ObjectResources {
    pub(crate) fn clear_mesh_level_environment(
        &mut self,
        level: MeshLevelKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        self.managed
            .clear_mesh_level_environment(level)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_managers::{
        AllocationToken, BufferBackend, BufferKey, BufferKind, CreateResult, GraphicsBuffer,
        GraphicsRequest, LockedDataKey, OwnerLease,
    };
    use crate::visual_resources::{self as vr, RawVertexBytesKey};
    use std::collections::{BTreeMap, VecDeque};

    #[test]
    fn object_state_reads_match_constructor_members() {
        // Working names for the direct `cur_lod`/`fade`/`lightness` member
        // reads on I3D_object (I3D.h): the original reads the public members
        // with no accessor, so no LS3DF FUNCTION address exists and none is
        // claimed.
        let mut r = ObjectResources::new().unwrap();
        let bare = r.create_object_resources(None).unwrap();
        assert_eq!(r.object_cur_lod(bare).unwrap(), 0);
        assert_eq!(r.object_fade(bare).unwrap(), 1.);
        let mesh = r.create_mesh_object().unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        assert_eq!(r.object_cur_lod(object).unwrap(), 0);
        assert_eq!(r.object_fade(object).unwrap(), 1.);
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .cur_lod = 3;
        assert_eq!(r.object_cur_lod(object).unwrap(), 3);
        let foreign = ObjectResources::new()
            .unwrap()
            .create_object_resources(None)
            .unwrap();
        assert_eq!(r.object_cur_lod(foreign), Err(ObjectError::ForeignArena));
        assert_eq!(r.object_fade(foreign), Err(ObjectError::ForeignArena));
        assert_eq!(r.object_lightness_of(bare).unwrap(), 1.);
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lightness = 0.25;
        assert_eq!(r.object_lightness_of(object).unwrap(), 0.25);
        assert_eq!(
            r.object_lightness_of(foreign),
            Err(ObjectError::ForeignArena)
        );
    }
    #[test]
    fn object_transparency_reader_reports_constructor_default_and_store() {
        // Working name for the direct `transparency` member read on
        // I3D_object (I3D.h:2923): the original reads the public member
        // with no accessor, so no LS3DF FUNCTION address exists and none
        // is claimed.
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let bare = r.create_object_resources(None).unwrap();
        assert_eq!(r.object_transparency_of(bare).unwrap(), 1.);
        // Meshless objects are a setter no-op (native early return).
        r.set_object_transparency(bare, 0.5).unwrap();
        assert_eq!(r.object_transparency_of(bare).unwrap(), 1.);
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let first = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.set_object_transparency(object, 0.5).unwrap();
        assert_eq!(
            r.object_transparency_of(object).unwrap().to_bits(),
            0.5f32.to_bits()
        );
        let foreign = ObjectResources::new()
            .unwrap()
            .create_object_resources(None)
            .unwrap();
        assert_eq!(
            r.object_transparency_of(foreign),
            Err(ObjectError::ForeignArena)
        );
    }
    #[test]
    fn visual_getters_report_type_mesh_and_lod_gate() {
        // Working names for inlined I3D_visual::GetVisualType (I3D.h:2748-2760,
        // LS3DF_EXPORTS branch), I3D_object::GetMesh (I3D.h:2896-2908,
        // LS3DF_EXPORTS branch) and I3D_object::GetMeshLOD (I3D.h:2910-2916):
        // no LS3DF FUNCTION addresses exist (only Game.exe 0x47ad70/0x47ad80
        // for the first two, unclaimed here; none at all for the third).
        let mut r = ObjectResources::new().unwrap();
        let bare = r.create_object_resources(None).unwrap();
        assert_eq!(r.visual_type_word(bare).unwrap(), 0x5f4a424f);
        assert_eq!(r.object_mesh_handle(bare).unwrap(), None);
        assert_eq!(r.object_mesh_lod_index(bare).unwrap(), 0);
        let mesh = r.create_mesh_object().unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        assert_eq!(r.object_mesh_handle(object).unwrap(), Some(mesh));
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .cur_lod = 3;
        assert_eq!(r.object_mesh_lod_index(object).unwrap(), 3);
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .visual_flags |= 0x10;
        assert_eq!(r.object_mesh_lod_index(object).unwrap(), 0);
        let foreign = ObjectResources::new()
            .unwrap()
            .create_object_resources(None)
            .unwrap();
        assert_eq!(r.visual_type_word(foreign), Err(ObjectError::ForeignArena));
        assert_eq!(r.object_mesh_handle(foreign), Err(ObjectError::ForeignArena));
        assert_eq!(r.object_mesh_lod_index(foreign), Err(ObjectError::ForeignArena));
    }
    #[test]
    fn texbl_struct_teardown_releases_offsets_and_keeps_scalars() {
        // LS3DF RVA 0x5da70: hand-derived offsets, no game fixtures. The
        // 1 BB / 28 B body destroys only the offsets C_list (delete[] + NULL
        // + zero sizes); scalar words are never zeroed by a destructor.
        let mut entry = TexblStruct {
            offsets: vec![
                TexblOffset {
                    x: 4,
                    y: 8,
                    rotated: false,
                },
                TexblOffset {
                    x: 0,
                    y: 0,
                    rotated: true,
                },
            ],
            free_area: 1024,
            lost_area: 9,
        };
        texbl_struct_teardown(&mut entry);
        assert!(entry.offsets.is_empty());
        assert_eq!(entry.offsets.capacity(), 0);
        assert_eq!((entry.free_area, entry.lost_area), (1024, 9));
        // Idempotent: a second teardown is a clean no-op.
        texbl_struct_teardown(&mut entry);
        assert!(entry.offsets.is_empty());
        assert_eq!((entry.free_area, entry.lost_area), (1024, 9));
    }
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        BufferNew(BufferKind, AllocationToken),
        BufferFree(AllocationToken),
        BlockNew(BufferKind, u32, u32, AllocationToken),
        BlockFree(AllocationToken),
        Create(GraphicsRequest),
        Discard,
        Lock(GraphicsBuffer, u32),
        Unlock(GraphicsBuffer),
        Release(GraphicsBuffer),
        Vram,
        Oom,
        TextureRetain(TextureKey),
        TextureRelease(TextureKey),
    }
    /// Required callbacks backed by tracked CPU fixture storage; this is not GPU
    /// execution or a native texture implementation.
    struct Backend {
        next: u64,
        allocations: BTreeMap<u64, bool>,
        graphics: BTreeMap<u64, (bool, Vec<u8>)>,
        textures: BTreeMap<u64, u32>,
        events: Vec<Event>,
        rejects: VecDeque<i32>,
        fail_texture: Option<TextureKey>,
        fail_release: bool,
        fail_release_number: Option<usize>,
        fail_block: bool,
    }
    impl Default for Backend {
        fn default() -> Self {
            Self {
                next: 1,
                allocations: BTreeMap::new(),
                graphics: BTreeMap::new(),
                textures: BTreeMap::new(),
                events: Vec::new(),
                rejects: VecDeque::new(),
                fail_texture: None,
                fail_release: false,
                fail_release_number: None,
                fail_block: false,
            }
        }
    }
    impl Backend {
        fn next(&mut self) -> u64 {
            let n = self.next;
            self.next += 1;
            n
        }
        fn graphics_id(g: GraphicsBuffer) -> u64 {
            match g {
                GraphicsBuffer::Index(h) => h.0,
                GraphicsBuffer::Vertex(h) => h.0,
            }
        }
        fn releases(&self) -> Vec<GraphicsBuffer> {
            self.events
                .iter()
                .filter_map(|e| {
                    if let Event::Release(g) = e {
                        Some(*g)
                    } else {
                        None
                    }
                })
                .collect()
        }
    }
    impl BufferBackend for Backend {
        type Error = &'static str;
        fn allocate_buffer(&mut self, k: BufferKind) -> Result<AllocationToken, Self::Error> {
            let t = AllocationToken(self.next());
            self.allocations.insert(t.0, true);
            self.events.push(Event::BufferNew(k, t));
            Ok(t)
        }
        fn free_buffer(&mut self, t: AllocationToken) -> Result<(), Self::Error> {
            assert_eq!(self.allocations.remove(&t.0), Some(true));
            self.events.push(Event::BufferFree(t));
            Ok(())
        }
        fn allocate_block(
            &mut self,
            k: BufferKind,
            b: u32,
            n: u32,
        ) -> Result<AllocationToken, Self::Error> {
            if self.fail_block {
                return Err("block failure");
            }
            let t = AllocationToken(self.next());
            self.allocations.insert(t.0, false);
            self.events.push(Event::BlockNew(k, b, n, t));
            Ok(t)
        }
        fn free_block(&mut self, t: AllocationToken) -> Result<(), Self::Error> {
            assert_eq!(self.allocations.remove(&t.0), Some(false));
            self.events.push(Event::BlockFree(t));
            Ok(())
        }
        fn create_graphics(&mut self, r: GraphicsRequest) -> Result<CreateResult, Self::Error> {
            self.events.push(Event::Create(r));
            if let Some(hr) = self.rejects.pop_front() {
                return Ok(CreateResult::Rejected(hr));
            }
            let id = self.next();
            self.graphics.insert(id, (false, vec![0; r.bytes as usize]));
            Ok(CreateResult::Created(match r.kind {
                BufferKind::Index => GraphicsBuffer::Index(vr::IndexBufferHandle(id)),
                BufferKind::Vertex => GraphicsBuffer::Vertex(vr::VertexBufferHandle(id)),
            }))
        }
        fn discard_managed(&mut self) -> Result<(), Self::Error> {
            self.events.push(Event::Discard);
            Ok(())
        }
        fn lock(&mut self, g: GraphicsBuffer, f: u32) -> Result<LockedDataKey, Self::Error> {
            let storage = self.graphics.get_mut(&Self::graphics_id(g)).unwrap();
            assert!(!storage.0);
            storage.0 = true;
            self.events.push(Event::Lock(g, f));
            Ok(LockedDataKey(self.next()))
        }
        fn unlock(&mut self, g: GraphicsBuffer) -> Result<(), Self::Error> {
            let storage = self.graphics.get_mut(&Self::graphics_id(g)).unwrap();
            assert!(storage.0);
            storage.0 = false;
            self.events.push(Event::Unlock(g));
            Ok(())
        }
        fn release_graphics(&mut self, g: GraphicsBuffer) -> Result<(), Self::Error> {
            let storage = self.graphics.remove(&Self::graphics_id(g)).unwrap();
            assert!(!storage.0);
            self.events.push(Event::Release(g));
            if self.fail_release || self.fail_release_number == Some(self.releases().len()) {
                Err("graphics released then failed")
            } else {
                Ok(())
            }
        }
        fn driver_present(&self) -> bool {
            true
        }
        fn recompute_vram(&mut self) -> Result<(), Self::Error> {
            self.events.push(Event::Vram);
            Ok(())
        }
        fn warn_unreleased_buffer(&mut self, _: BufferKey) -> Result<(), Self::Error> {
            panic!("bridge must not destroy a used buffer")
        }
        fn out_of_memory(&mut self, _: BufferKind, _: u32, _: i32) -> Result<(), Self::Error> {
            self.events.push(Event::Oom);
            Ok(())
        }
    }
    impl ManagedVisualBackend for Backend {
        fn retain_texture(&mut self, t: TextureKey) -> Result<(), Self::Error> {
            *self.textures.get_mut(&t.0).ok_or("unknown texture")? += 1;
            self.events.push(Event::TextureRetain(t));
            Ok(())
        }
        fn release_texture(&mut self, t: TextureKey) -> Result<(), Self::Error> {
            let refs = self.textures.get_mut(&t.0).ok_or("unknown texture")?;
            assert!(*refs > 0);
            *refs -= 1;
            self.events.push(Event::TextureRelease(t));
            if self.fail_texture == Some(t) {
                Err("texture released then failed")
            } else {
                Ok(())
            }
        }
        fn texture_flags(&mut self, t: TextureKey) -> Result<u32, Self::Error> {
            self.textures.get(&t.0).ok_or("unknown texture")?;
            Ok(0)
        }
        fn free_raw_vertices(
            &mut self,
            _: MeshLevelKey,
            _: RawVertexBytesKey,
        ) -> Result<(), Self::Error> {
            Err("raw vertices unavailable")
        }
    }

    fn request(count: u32) -> VertexRequest {
        VertexRequest {
            fvf: 0x112,
            usage: 0,
            stride: 32,
            count,
            format: 0,
            exclusive: false,
        }
    }
    fn shared_mesh(
        r: &mut ObjectResources,
        b: &mut Backend,
    ) -> (MeshObjectKey, MeshLevelKey, MaterialKey) {
        let material = r.create_material().unwrap();
        let head = r.create_face_group().unwrap();
        let tail = r.create_face_group().unwrap();
        r.set_material(head, Some(material), b).unwrap();
        r.set_material(tail, Some(material), b).unwrap();
        r.allocate_group_indices(head, 1, 0, b).unwrap();
        r.allocate_group_indices(tail, 2, 0, b).unwrap();
        r.link_next(head, tail).unwrap();
        let level = r.create_mesh_level().unwrap();
        r.allocate_level_vertices(level, 4, request(4), Some(head), b)
            .unwrap();
        r.install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
            .unwrap();
        let mesh = r.create_mesh_object().unwrap();
        r.set_lod(mesh, Some(level), 0, b).unwrap();
        r.release_mesh_level(level, b).unwrap();
        r.release_material(material, b).unwrap();
        (mesh, level, material)
    }
    #[test]
    fn render_lod_early_returns_preserve_cur_state_and_scale_is_lazy() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, level, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .cur_lod = 7;
        assert_eq!(r.get_object_lod(object, f32::NAN, f32::NAN, 0.), Ok(0));
        assert_eq!(r.object(object).unwrap().cur_lod, 7); // lod_dist<=0 early return
        r.install_loaded_lod_distance(level, 100.).unwrap();
        assert_eq!(r.get_object_lod(object, 100., 1., 1.), Ok(0));
        assert_eq!(r.get_object_lod(object, 100.01, 1., 1.), Ok(1));
        assert_eq!(r.object(object).unwrap().cur_lod, 1); // cull sentinel is stored, not a level
        assert_eq!(
            r.get_object_lod(object, 50., 1., 0.),
            Err(ObjectLodError::NumericDomain)
        );
        assert_eq!(r.object(object).unwrap().cur_lod, 1);
    }
    #[test]
    fn render_fade_skips_null_mesh_and_hidden_private_lod_slots_after_truncation() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let null = r.create_object_resources(None).unwrap();
        r.set_render_fade(null, f32::NAN).unwrap();
        assert_eq!(r.object(null).unwrap().fade, 1.);
        let (mesh, level, _) = shared_mesh(&mut r, &mut b);
        r.set_lod(mesh, Some(level), 1, &mut b).unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let first = r.create_plain_lod().unwrap();
        let hidden = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.adopt_plain_lod(object, 1, hidden).unwrap();
        r.set_lod(mesh, None, 1, &mut b).unwrap(); // actual num_lods truncation, object slot1 retained
        r.set_render_fade(object, 0.5).unwrap();
        assert!(r.lod(first).unwrap().alpha_dirty);
        assert!(!r.lod(hidden).unwrap().alpha_dirty);
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .alpha_dirty = false;
        r.set_render_fade(object, 0.5001).unwrap();
        assert_eq!(r.object(object).unwrap().fade, 0.5001);
        assert!(!r.lod(first).unwrap().alpha_dirty); // same packed byte, still stores float
    }
    #[test]
    fn constructor_nondelete_and_null_object_preserve_represented_fields_without_fake_callbacks() {
        let mut r = crate::ls3df::i3d_object::ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let key = r.create_plain_lod().unwrap();
        let lod = r.lod(key).unwrap();
        assert_eq!(
            (lod.lights_changed, lod.update_count, lod.light_key),
            (u32::MAX, u32::MAX, u32::MAX)
        );
        assert_eq!(lod.vertex_buffer, CompleteVertexBuffer::default());
        assert_eq!(
            [
                lod.u_ofs.to_bits(),
                lod.v_ofs.to_bits(),
                lod.env_u_ofs.to_bits(),
                lod.env_v_ofs.to_bits()
            ],
            [0; 4]
        );
        assert!(!lod.dirty && !lod.alpha_dirty);
        assert_eq!(lod.alpha, 255);
        r.destruct_plain_lod(key, &mut b).unwrap();
        assert_eq!(r.lod(key).unwrap().state, LodState::Destructed);
        assert!(b.events.is_empty());
        let object = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.adopt_plain_lod(object, 0, key),
            Err(ObjectError::LodDestructed(key))
        );
        assert!(r.destroy_plain_lod(key, &mut b).is_err());
        r.destroy_object_resources(object, &mut b).unwrap();
        assert!(r.object(object).is_err());
        assert!(b.events.is_empty());
        assert!(!r.is_poisoned());
    }
    #[test]
    fn two_objects_own_distinct_lod_blocks_but_keep_shared_mesh_and_graphics_until_last_owner() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, level, material) = shared_mesh(&mut r, &mut b);
        let a = r.create_object_resources(Some(mesh)).unwrap();
        let z = r.create_object_resources(Some(mesh)).unwrap();
        let la = r.create_plain_lod().unwrap();
        let lz = r.create_plain_lod().unwrap();
        r.allocate_plain_lod_vertices(la, request(3), &mut b)
            .unwrap();
        r.allocate_plain_lod_vertices(lz, request(5), &mut b)
            .unwrap();
        r.adopt_plain_lod(a, 0, la).unwrap();
        r.adopt_plain_lod(z, 9, lz).unwrap();
        let avb = r.lod(la).unwrap().vertex_buffer;
        let zvb = r.lod(lz).unwrap().vertex_buffer;
        assert_eq!(avb.buffer, zvb.buffer);
        assert_eq!(avb.graphics, zvb.graphics);
        assert_ne!(avb.base, zvb.base);
        assert_eq!(
            avb.graphics,
            r.managed()
                .resources()
                .mesh_level(level)
                .unwrap()
                .vertex_buffer
                .graphics
        );
        assert_eq!(r.release_mesh_object(mesh, &mut b).unwrap(), 2);
        let before = b.events.len();
        assert_eq!(
            r.release_mesh_object(mesh, &mut b),
            Err(ObjectOperationError::State(
                ObjectError::MeshReferenceOwned(mesh)
            ))
        );
        assert_eq!(b.events.len(), before);
        r.destroy_object_resources(a, &mut b).unwrap();
        assert!(r.object(a).is_err());
        assert!(r.lod(la).is_err());
        assert!(b.releases().is_empty());
        assert_eq!(
            r.managed()
                .resources()
                .mesh_object(mesh)
                .unwrap()
                .references,
            1
        );
        assert!(r.managed().resources().material(material).is_ok());
        assert_eq!(r.lod_bindings()[0].state, BindingState::Consumed);
        assert_eq!(r.lod_bindings()[1].state, BindingState::Installed);
        r.destroy_object_resources(z, &mut b).unwrap();
        assert!(r.lod(lz).is_err());
        assert!(r.managed().resources().mesh_object(mesh).is_err());
        assert!(r.managed().resources().material(material).is_err());
        assert_eq!(b.releases().len(), 1);
        assert!(matches!(b.releases()[0], GraphicsBuffer::Vertex(_)));
        assert_eq!(r.managed().managers().index_buffers().len(), 1);
        r.reclaim_unused_indices(&mut b).unwrap();
        assert!(b.allocations.is_empty());
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn all_ten_slots_destroy_in_index_order_even_when_allocated_in_reverse() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let object = r.create_object_resources(None).unwrap();
        let mut expected = vec![None; 10];
        for i in (0..10).rev() {
            let lod = r.create_plain_lod().unwrap();
            let mut q = request(1);
            q.exclusive = true;
            r.allocate_plain_lod_vertices(lod, q, &mut b).unwrap();
            let raw = r.lod(lod).unwrap().vertex_buffer;
            expected[i] = Some(GraphicsBuffer::Vertex(raw.graphics.unwrap()));
            r.adopt_plain_lod(object, i as u32, lod).unwrap();
        }
        r.destroy_object_resources(object, &mut b).unwrap();
        assert_eq!(
            b.releases(),
            expected.into_iter().flatten().collect::<Vec<_>>()
        );
        assert!(r
            .lod_bindings()
            .iter()
            .all(|x| x.state == BindingState::Consumed));
        assert!(b.allocations.is_empty());
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn interrupted_second_lod_keeps_every_raw_slot_until_complete_loop_and_never_replays_prefix() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let mesh = r.create_mesh_object().unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        r.release_mesh_object(mesh, &mut b).unwrap();
        let first = r.create_plain_lod().unwrap();
        let last = r.create_plain_lod().unwrap();
        let mut q = request(1);
        q.exclusive = true;
        r.allocate_plain_lod_vertices(first, q, &mut b).unwrap();
        r.allocate_plain_lod_vertices(last, q, &mut b).unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.adopt_plain_lod(object, 9, last).unwrap();
        let before_slots = r.object(object).unwrap().lods;
        b.fail_release_number = Some(2);
        assert_eq!(
            r.destroy_object_resources(object, &mut b),
            Err(ObjectOperationError::Backend(
                "graphics released then failed"
            ))
        );
        assert!(r.is_poisoned());
        let raw = r.object(object).unwrap();
        assert_eq!(raw.lods, before_slots);
        assert_eq!(raw.mesh, Some(mesh));
        assert_eq!(raw.phase, ObjectCleanupPhase::DestroyingLods);
        assert!(r.lod(first).is_err());
        assert_eq!(r.lod(last).unwrap().state, LodState::Constructed);
        assert_eq!(r.lod_bindings()[0].state, BindingState::Consumed);
        assert_eq!(r.lod_bindings()[1].state, BindingState::Installed);
        assert_eq!(
            r.managed()
                .managers()
                .vertex_owner(r.lod_bindings()[1].owner)
                .unwrap()
                .lease,
            OwnerLease::Freed
        );
        assert_eq!(
            r.managed()
                .resources()
                .mesh_object(mesh)
                .unwrap()
                .references,
            1
        );
        let before = b.events.len();
        assert_eq!(r.create_plain_lod(), Err(ObjectError::Poisoned));
        assert_eq!(
            r.release_mesh_object(mesh, &mut b),
            Err(ObjectOperationError::State(ObjectError::Poisoned))
        );
        assert!(r.destroy_object_resources(object, &mut b).is_err());
        assert!(r.reclaim_unused_vertices(&mut b).is_err());
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn mesh_release_failure_sees_all_lod_slots_cleared_but_keeps_raw_mesh_pointer() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, material) = shared_mesh(&mut r, &mut b);
        let texture = TextureKey(91);
        b.textures.insert(91, 1);
        r.set_texture(material, Some(texture), &mut b).unwrap();
        let object = r.create_object_resources(Some(mesh)).unwrap();
        r.release_mesh_object(mesh, &mut b).unwrap();
        let lod = r.create_plain_lod().unwrap();
        r.allocate_plain_lod_vertices(lod, request(1), &mut b)
            .unwrap();
        r.adopt_plain_lod(object, 4, lod).unwrap();
        b.fail_texture = Some(texture);
        assert_eq!(
            r.destroy_object_resources(object, &mut b),
            Err(ObjectOperationError::Backend(
                "texture released then failed"
            ))
        );
        let raw = r.object(object).unwrap();
        assert_eq!(raw.lods, [None; 10]);
        assert_eq!(raw.mesh, Some(mesh));
        assert_eq!(raw.phase, ObjectCleanupPhase::ReleasingMesh);
        assert!(r.lod(lod).is_err());
        assert_eq!(
            r.managed()
                .resources()
                .mesh_object(mesh)
                .unwrap()
                .references,
            0
        );
        assert!(r.is_poisoned());
        assert_eq!(b.textures[&91], 1);
    }
    #[test]
    fn standalone_nondelete_preserves_stale_descriptor_and_cannot_be_reused_or_adopted() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let lod = r.create_plain_lod().unwrap();
        r.allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap();
        let block = r.lod(lod).unwrap().vertex_buffer;
        r.destruct_plain_lod(lod, &mut b).unwrap();
        assert_eq!(r.lod(lod).unwrap().vertex_buffer, block);
        assert_eq!(r.lod(lod).unwrap().state, LodState::Destructed);
        assert_eq!(r.lod_bindings()[0].state, BindingState::Consumed);
        assert_eq!(
            r.managed()
                .managers()
                .vertex_owner(r.lod_bindings()[0].owner)
                .unwrap()
                .lease,
            OwnerLease::Freed
        );
        assert!(b.allocations.is_empty());
        let object = r.create_object_resources(None).unwrap();
        let before = b.events.len();
        assert_eq!(
            r.adopt_plain_lod(object, 0, lod),
            Err(ObjectError::LodDestructed(lod))
        );
        assert!(r
            .allocate_plain_lod_vertices(lod, request(2), &mut b)
            .is_err());
        assert!(r.destruct_plain_lod(lod, &mut b).is_err());
        assert_eq!(b.events.len(), before);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn unique_adoption_foreign_keys_indices_and_double_storage_reject_before_effects() {
        let mut r = ObjectResources::new().unwrap();
        let mut other = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let foreign = other.create_plain_lod().unwrap();
        let lod = r.create_plain_lod().unwrap();
        let a = r.create_object_resources(None).unwrap();
        let z = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.adopt_plain_lod(a, 0, foreign),
            Err(ObjectError::ForeignArena)
        );
        assert_eq!(
            r.adopt_plain_lod(a, 10, lod),
            Err(ObjectError::InvalidIndex(10))
        );
        assert_eq!(
            r.adopt_plain_lod(a, u32::MAX, lod),
            Err(ObjectError::UnsupportedIndex(u32::MAX))
        );
        r.adopt_plain_lod(a, 0, lod).unwrap();
        assert_eq!(
            r.adopt_plain_lod(z, 0, lod),
            Err(ObjectError::LodOwned(lod))
        );
        assert!(r.destroy_plain_lod(lod, &mut b).is_err());
        assert!(b.events.is_empty());
        r.allocate_plain_lod_vertices(lod, request(1), &mut b)
            .unwrap();
        let before = b.events.len();
        assert!(r
            .allocate_plain_lod_vertices(lod, request(1), &mut b)
            .is_err());
        assert_eq!(b.events.len(), before);
        r.destroy_object_resources(a, &mut b).unwrap();
        assert!(!r.is_poisoned());
        assert!(b.allocations.is_empty());
    }
    #[test]
    fn graphics_false_retains_pending_plain_owner_and_retry_keeps_identity() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let lod = r.create_plain_lod().unwrap();
        b.rejects.extend([-1, -2, -3]);
        assert!(!r
            .allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap());
        assert!(!r.is_poisoned());
        let owner = r.lod_bindings()[0].owner;
        assert_eq!(
            r.managed().managers().vertex_owner(owner).unwrap().lease,
            OwnerLease::Vacant
        );
        assert_eq!(
            r.lod(lod).unwrap().vertex_buffer,
            CompleteVertexBuffer::default()
        );
        assert!(b.allocations.is_empty());
        assert!(r
            .allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap());
        assert_eq!(r.lod_bindings()[0].owner, owner);
        r.destroy_plain_lod(lod, &mut b).unwrap();
        assert!(r.lod(lod).is_err());
        assert!(b.allocations.is_empty());
    }
    #[test]
    fn object_faces_roundtrip_shares_material_on_duplicate() {
        // RVAs 0x305b0/0x30690/0x30780/0x30800 through the object layer:
        // set -> get (borrow) -> unlock -> duplicate shares bytes+material.
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let material = r.create_material().unwrap();
        let source = r.create_face_group().unwrap();
        let destination = r.create_face_group().unwrap();
        r.set_material(source, Some(material), &mut b).unwrap();
        let faces: Vec<vr::I3dTriface> = vec![[1, 2, 3], [4, 5, 6]];
        r.set_faces(source, Some(&faces), &mut b).unwrap();
        assert_eq!(r.get_faces(source, &mut b).unwrap().unwrap(), &faces);
        r.unlock_faces(source, &mut b).unwrap();
        assert_eq!(
            r.duplicate_face_group(destination, source, &mut b).unwrap(),
            0
        );
        assert_eq!(r.get_faces(destination, &mut b).unwrap().unwrap(), &faces);
        r.unlock_faces(destination, &mut b).unwrap();
        // Detach path through the object layer duplicates shared materials.
        let other = r.create_face_group().unwrap();
        r.set_material(other, Some(material), &mut b).unwrap();
        r.release_material(material, &mut b).unwrap();
        assert_eq!(r.detach_face_group_material(source, &mut b).unwrap(), 0);
        assert_ne!(
            r.managed.resources().face_group(source).unwrap().material,
            Some(material)
        );
        assert!(!r.is_poisoned());
    }
    #[test]
    fn setmesh_same_ptr_is_noop_then_null_clears() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let object = r.create_object_resources(None).unwrap();
        assert!(!r
            .set_object_mesh_keep_compatible(object, None, true, &mut b)
            .unwrap());
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        assert!(r
            .set_object_mesh_keep_compatible(object, Some(mesh), false, &mut b)
            .unwrap());
        assert_eq!(r.object(object).unwrap().mesh, Some(mesh));
        assert!(!r
            .set_object_mesh_keep_compatible(object, Some(mesh), true, &mut b)
            .unwrap());
        assert!(r
            .set_object_mesh_keep_compatible(object, None, false, &mut b)
            .unwrap());
        assert_eq!(r.object(object).unwrap().mesh, None);
        assert!(r
            .object(object)
            .unwrap()
            .lods
            .iter()
            .all(|slot| slot.is_none()));
    }
    #[test]
    fn setmesh_keep_compatible_skips_lod_delete() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh_a, _, _) = shared_mesh(&mut r, &mut b);
        let (mesh_b, _, _) = shared_mesh(&mut r, &mut b);
        assert!(r.is_mesh_compatible(mesh_b, mesh_a).unwrap());
        let bare = r.create_mesh_object().unwrap();
        assert!(!r.is_mesh_compatible(mesh_a, bare).unwrap());
        let object = r.create_object_resources(Some(mesh_a)).unwrap();
        let lod = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, lod).unwrap();
        // Compatible keep: LOD survives, mesh swaps, returns not-cleared.
        assert!(!r
            .set_object_mesh_keep_compatible(object, Some(mesh_b), true, &mut b)
            .unwrap());
        assert_eq!(r.object(object).unwrap().mesh, Some(mesh_b));
        assert_eq!(r.object(object).unwrap().lods[0], Some(lod));
        // Incompatible keep: full clear, returns cleared.
        assert!(r
            .set_object_mesh_keep_compatible(object, Some(bare), true, &mut b)
            .unwrap());
        assert_eq!(r.object(object).unwrap().mesh, Some(bare));
        assert!(r
            .object(object)
            .unwrap()
            .lods
            .iter()
            .all(|slot| slot.is_none()));
        assert!(!r.is_poisoned());
    }
    #[test]
    fn update_all_marks_only_present_slots() {
        let mut r = ObjectResources::new().unwrap();
        let object = r.create_object_resources(None).unwrap();
        let mut keys = Vec::new();
        for slot in [0u32, 3, 9] {
            let lod = r.create_plain_lod().unwrap();
            r.adopt_plain_lod(object, slot, lod).unwrap();
            keys.push(lod);
        }
        assert_eq!(r.mark_all_object_lods_dirty(object).unwrap(), 3);
        for lod in keys {
            assert!(r.lod(lod).unwrap().dirty);
        }
    }
    #[test]
    fn update_indexed_bounds_rejects_oob() {
        let mut r = ObjectResources::new().unwrap();
        let object = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.update_object_vertices(object, 10),
            Err(ObjectError::InvalidIndex(10))
        );
        // Empty slot is a silent no-op.
        r.update_object_vertices(object, 2).unwrap();
        let lod = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 2, lod).unwrap();
        r.update_object_vertices(object, 2).unwrap();
        assert!(r.lod(lod).unwrap().dirty);
    }
    #[test]
    fn detach_single_ref_is_noop_shared_duplicates() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        // No mesh at all: no-op.
        let bare = r.create_object_resources(None).unwrap();
        assert!(!r.detach_object_mesh_to_unique(bare, &mut b).unwrap());
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        // Owned meshes carry test + object references: shared, duplicates.
        assert!(r.detach_object_mesh_to_unique(object, &mut b).unwrap());
        let fresh = r.object(object).unwrap().mesh.unwrap();
        assert_ne!(fresh, mesh);
        // The fresh mesh is uniquely owned now: second detach is a no-op.
        assert!(!r.detach_object_mesh_to_unique(object, &mut b).unwrap());
        assert_eq!(r.object(object).unwrap().mesh, Some(fresh));
        assert!(!r.is_poisoned());
    }
    #[test]
    fn duplicate_field_copy_masks_and_copies_bounds() {
        use super::object_lod::{
            render_stage_reset_flags, render_total_alpha, ObjectSpaceTransition,
        };
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let src = r.create_object_resources(Some(mesh)).unwrap();
        let dst = r.create_object_resources(Some(mesh)).unwrap();
        {
            let record = r.objects[(src.slot - 1) as usize].as_mut().unwrap();
            record.lightness = 0.5;
            record.transparency = 0.25;
            record.fade = 0.75;
            record.world_space = true;
            record.inv_matrix_valid = true;
            record.visual_flags = 0xffff_ffff;
            record.bounds = crate::mesh_bounds::MeshBoundVolume {
                bbox: [[-1., -2., -3.], [4., 5., 6.]],
                center: [1.5, 1.5, 1.5],
                radius: 5.,
            };
        }
        let snapshot = r.duplicate_object_render_state(dst, src).unwrap();
        assert_eq!(
            (snapshot.lightness, snapshot.transparency, snapshot.fade),
            (0.5, 0.25, 0.75)
        );
        assert_eq!(snapshot.visual_flags_masked, 0x2_ffd0);
        let record = r.object(dst).unwrap();
        assert_eq!(record.lightness, 0.5);
        assert_eq!(record.visual_flags, 0x2_ffd0);
        assert_eq!(record.bounds.bbox, [[-1., -2., -3.], [4., 5., 6.]]);
        // Billboard is in the fourcc allowlist; transitions stay usable.
        let board = r.create_billboard_resources(Some(mesh)).unwrap();
        r.duplicate_object_render_state(board, src).unwrap();
        // Lit destination: the single scalar owner copies scalars too (the
        // lighting-settings copy in copy_object_duplicate_state adds lit
        // levels on top, never a split word set).
        let lit = r
            .create_unconstructed_lit_object_resources(Some(mesh))
            .unwrap();
        r.duplicate_object_render_state(lit, src).unwrap();
        let lit_record = r.object(lit).unwrap();
        assert_eq!(
            (
                lit_record.lightness,
                lit_record.transparency,
                lit_record.fade,
                lit_record.world_space,
                lit_record.inv_matrix_valid
            ),
            (0.5, 0.25, 0.75, true, false)
        );
        let _ = (
            ObjectSpaceTransition::AlreadyWorld,
            render_stage_reset_flags(false, false, false),
            render_total_alpha(1., 1., 1.).unwrap(),
        );
    }
    #[test]
    fn lod_uv_store_masks_and_adds_offsets() {
        let mut r = ObjectResources::new().unwrap();
        let lod = r.create_plain_lod().unwrap();
        assert_eq!(
            r.store_lod_uv_offsets(lod, 7, 0.5, -0.25, false).unwrap(),
            (3, 10)
        );
        let record = r.lod(lod).unwrap();
        assert_eq!((record.u_ofs, record.v_ofs), (0.5, -0.25));
        assert_eq!(
            super::lod_uv_pair([1., 2.], [0.5, -0.25]).unwrap(),
            [1.5, 1.75]
        );
    }
    #[test]
    fn prepare_create_records_format_and_clears_dirty() {
        let mut r = ObjectResources::new().unwrap();
        let lod = r.create_plain_lod().unwrap();
        r.lods[(lod.slot - 1) as usize].as_mut().unwrap().dirty = true;
        let params = r.prepare_plain_lod_create(lod, 9, true).unwrap();
        assert_eq!(
            (params.format, params.stride_floats, params.needs_alloc),
            (1, 6, true)
        );
        assert!(!r.lod(lod).unwrap().dirty);
    }
    #[test]
    fn queue_face_groups_reports_contract_boundary() {
        let mut r = ObjectResources::new().unwrap();
        let object = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.queue_object_face_groups_contract(object),
            Err(ObjectError::UnsupportedVisualSubtype)
        );
    }
    #[test]
    fn object_space_flag_machine_and_null_mesh_arms() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let bare = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.set_object_space(bare, true).unwrap(),
            super::object_lod::ObjectSpaceTransition::BakeToWorld
        );
        assert!(r.object(bare).unwrap().world_space);
        assert_eq!(
            r.set_object_space(bare, true).unwrap(),
            super::object_lod::ObjectSpaceTransition::AlreadyWorld
        );
        // Null-mesh world arm still clears the flag (binary clear-then-check).
        assert_eq!(
            r.set_object_space(bare, false).unwrap(),
            super::object_lod::ObjectSpaceTransition::BakeToLocal
        );
        let cleared = r.object(bare).unwrap();
        assert!(!cleared.world_space && !cleared.inv_matrix_valid);
        assert_eq!(
            r.set_object_space(bare, false).unwrap(),
            super::object_lod::ObjectSpaceTransition::AlreadyLocal
        );
        // Mesh-backed world arm sets the 0x100000 word.
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let base = b.events.len();
        r.set_object_space(object, true).unwrap();
        r.set_object_space(object, false).unwrap();
        let record = r.object(object).unwrap();
        assert!(!record.world_space && record.inv_matrix_valid);
        // Backend traffic from the shared_mesh fixture predates the state
        // machine: only count new effects (the machine performs none).
        assert_eq!(b.events.len(), base);
    }
    #[test]
    fn object_lightness_epsilon_skips_store_and_marks_on_change() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let null = r.create_object_resources(None).unwrap();
        r.set_object_lightness(null, 0.).unwrap(); // mesh-null skip
        assert_eq!(r.object(null).unwrap().lightness, 1.);
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let first = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lights_changed = 0;
        r.set_object_lightness(object, 1.).unwrap(); // equal: skip
        assert_eq!(
            r.lods[(first.slot - 1) as usize]
                .as_ref()
                .unwrap()
                .lights_changed,
            0
        );
        r.set_object_lightness(object, 0.).unwrap(); // |0-1| = 1: store
        assert_eq!(
            r.object(object).unwrap().lightness.to_bits(),
            0f32.to_bits()
        );
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lights_changed = 0;
        r.set_object_lightness(object, 5e-9).unwrap(); // sub-epsilon: skip
        assert_eq!(
            r.object(object).unwrap().lightness.to_bits(),
            0f32.to_bits()
        );
        assert_eq!(
            r.lods[(first.slot - 1) as usize]
                .as_ref()
                .unwrap()
                .lights_changed,
            0
        );
        r.set_object_lightness(object, 5e-8).unwrap(); // above: store + marks
        assert_eq!(
            r.object(object).unwrap().lightness.to_bits(),
            5e-8f32.to_bits()
        );
        let lod = r.lod(first).unwrap();
        assert_eq!((lod.lights_changed, lod.light_key), (u32::MAX, u32::MAX));
        // Canonical NaN bits are positive-huge: SIGNED compare stores it.
        r.set_object_lightness(object, f32::NAN).unwrap();
        assert!(r.object(object).unwrap().lightness.is_nan());
        // No clamp: out-of-range values store exactly.
        r.set_object_lightness(object, 2.).unwrap();
        assert_eq!(
            r.object(object).unwrap().lightness.to_bits(),
            2f32.to_bits()
        );
    }
    #[test]
    fn object_transparency_quantized_compare_stores_without_clamp() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let null = r.create_object_resources(None).unwrap();
        r.set_object_transparency(null, 0.).unwrap();
        assert_eq!(r.object(null).unwrap().transparency, 1.);
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let first = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.set_object_transparency(object, 0.).unwrap();
        assert!(r.lod(first).unwrap().alpha_dirty);
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .alpha_dirty = false;
        // 0.5 -> byte 128; 0.50001 stays in byte 128: stored, no dirty.
        r.set_object_transparency(object, 0.5).unwrap();
        assert_eq!(
            r.object(object).unwrap().transparency.to_bits(),
            0.5f32.to_bits()
        );
        assert!(r.lod(first).unwrap().alpha_dirty);
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .alpha_dirty = false;
        r.set_object_transparency(object, 0.50001).unwrap();
        assert!(!r.lod(first).unwrap().alpha_dirty);
        // Indefinite-word equality: NaN vs NaN has equal bytes (no dirty).
        r.set_object_transparency(object, f32::NAN).unwrap();
        assert!(r.object(object).unwrap().transparency.is_nan());
        assert!(r.lod(first).unwrap().alpha_dirty);
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .alpha_dirty = false;
        r.set_object_transparency(object, f32::NAN).unwrap();
        assert!(!r.lod(first).unwrap().alpha_dirty);
        // No clamp: 1.5 stores exactly (byte 382).
        r.set_object_transparency(object, 1.5).unwrap();
        assert_eq!(
            r.object(object).unwrap().transparency.to_bits(),
            1.5f32.to_bits()
        );
        assert!(r.lod(first).unwrap().alpha_dirty);
    }
    #[test]
    fn object_reset_light_info_marks_adopted_slots_only() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let null = r.create_object_resources(None).unwrap();
        r.reset_object_light_info(null).unwrap();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let first = r.create_plain_lod().unwrap();
        let hidden = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.adopt_plain_lod(object, 1, hidden).unwrap(); // beyond num_lods=1
        for key in [first, hidden] {
            let lod = r.lods[(key.slot - 1) as usize].as_mut().unwrap();
            lod.lights_changed = 3;
            lod.light_key = 7;
            lod.update_count = 0;
        }
        r.reset_object_light_info(object).unwrap();
        let kept = r.lod(first).unwrap();
        assert_eq!(
            (kept.lights_changed, kept.light_key, kept.update_count),
            (u32::MAX, u32::MAX, 0)
        );
        let skipped = r.lod(hidden).unwrap();
        assert_eq!((skipped.lights_changed, skipped.light_key), (3, 7));
    }
    #[test]
    fn object_vertex_lock_guards_and_residue() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        assert_eq!(
            r.resolve_object_vertex_lock(object, 10),
            Err(ObjectError::InvalidIndex(10))
        );
        assert_eq!(r.resolve_object_vertex_lock(object, 0).unwrap(), None);
        let first = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        assert_eq!(r.resolve_object_vertex_lock(object, 0).unwrap(), None);
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .vertex_buffer = crate::visual_resources::CompleteVertexBuffer {
            buffer: Some(crate::visual_resources::VertexBufferPointer::RawBytes(
                crate::visual_resources::RawVertexBytesKey(7),
            )),
            base: 3,
            graphics: None,
        };
        let target = r.resolve_object_vertex_lock(object, 0).unwrap().unwrap();
        assert_eq!(target.base, 3);
        // Guard path performs no lock calls: only fixture-prior locks exist.
        let locked = r.managed.managers().vertex_locked().len();
        let events = b.events.len();
        assert_eq!(
            r.resolve_object_vertex_lock(object, 0)
                .unwrap()
                .unwrap()
                .base,
            3
        );
        assert_eq!(r.resolve_object_vertex_lock(object, 9).unwrap(), None);
        assert_eq!(r.managed.managers().vertex_locked().len(), locked);
        assert_eq!(b.events.len(), events);
    }
    #[test]
    fn render_lod_select_total_alpha_and_reset_order() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        // MATF_LOD0 forces slot 0 regardless of cur_lod.
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .cur_lod = 7;
        assert_eq!(r.select_render_lod_index(object, true).unwrap(), 0);
        // cur_lod beyond count is an error; the cull sentinel
        // (cur_lod == count == num_lods == 1 here) resolves to Ok(count)
        // without indexing any slot.
        assert!(r.select_render_lod_index(object, false).is_err());
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .cur_lod = 1;
        assert_eq!(r.select_render_lod_index(object, false).unwrap(), 1);
        // Sentinel (cur_lod == count == num_lods) resolves without indexing.
        r.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .cur_lod = 0;
        // Total-alpha boundary: exactly 1.0 is opaque.
        assert_eq!(
            super::object_lod::render_total_alpha(1., 1., 1.)
                .unwrap()
                .to_bits(),
            1f32.to_bits()
        );
        assert!(super::object_lod::render_total_alpha(0.9999999, 1., 1.).unwrap() < 1.);
        assert_eq!(
            super::object_lod::render_total_alpha(0.5, 0.5, 0.5)
                .unwrap()
                .to_bits(),
            0.125f32.to_bits()
        );
        assert_eq!(
            super::object_lod::render_total_alpha(f32::NAN, 1., 1.),
            Err(super::object_lod::ObjectLodError::NumericDomain)
        );
        // Reset-bit protocol words.
        assert_eq!(
            super::object_lod::render_stage_reset_flags(false, false, false),
            0
        );
        assert_eq!(
            super::object_lod::render_stage_reset_flags(true, false, false),
            2
        );
        assert_eq!(
            super::object_lod::render_stage_reset_flags(false, true, false),
            4
        );
        assert_eq!(
            super::object_lod::render_stage_reset_flags(false, false, true),
            1
        );
        assert_eq!(
            super::object_lod::render_stage_reset_flags(true, true, true),
            7
        );
        // Render-command order the backend submitter must follow.
        let selected = r.select_render_lod_index(object, false).unwrap();
        assert_eq!(selected, 0);
        let total = super::object_lod::render_total_alpha(1., 1., 1.).unwrap();
        assert_eq!(total.to_bits(), 1f32.to_bits());
        let reset = super::object_lod::render_stage_reset_flags(true, false, true);
        assert_eq!(reset, 3);
    }
    #[test]
    fn object_load_image_checked_prefix_and_residue() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let intake: Result<(), ObjectOperationError<&'static str>> =
            Err(ObjectOperationError::State(ObjectError::InvalidIndex(77)));
        assert_eq!(
            r.apply_object_load_image_checked(object, None, intake, &mut b),
            Err(ObjectOperationError::State(ObjectError::InvalidIndex(77)))
        );
        assert_eq!(r.object(object).unwrap().mesh, Some(mesh)); // no pollution
                                                                // Null attach clears the mesh (native Release-skip: no extra release).
        r.apply_object_load_image_checked(object, None, Ok(()), &mut b)
            .unwrap();
        assert_eq!(r.object(object).unwrap().mesh, None);
        assert!(r
            .object(object)
            .unwrap()
            .lods
            .iter()
            .all(|slot| slot.is_none()));
        // Null-to-null attach is a silent no-op.
        let bare = r.create_object_resources(None).unwrap();
        let events = b.events.len();
        r.apply_object_load_image_checked(bare, None, Ok(()), &mut b)
            .unwrap();
        assert_eq!(b.events.len(), events);
    }
    #[test]
    fn set_fade_binary_words_through_existing_port() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let object = r.create_object_resources(Some(mesh)).unwrap();
        let first = r.create_plain_lod().unwrap();
        r.adopt_plain_lod(object, 0, first).unwrap();
        r.set_render_fade(object, 0.5).unwrap();
        assert!(r.lod(first).unwrap().alpha_dirty);
        r.lods[(first.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .alpha_dirty = false;
        r.set_render_fade(object, 0.5001).unwrap();
        assert_eq!(r.object(object).unwrap().fade, 0.5001);
        assert!(!r.lod(first).unwrap().alpha_dirty);
        // Admitted divergence: the binary stores unvalidated floats, while the
        // committed port gates the presentation domain (NumericDomain). Pinned
        // as policy, not as binary parity.
        assert_eq!(
            r.set_render_fade(object, f32::NAN),
            Err(super::object_lod::ObjectLodError::NumericDomain)
        );
        assert_eq!(r.object(object).unwrap().fade, 0.5001); // untouched residue
    }
    #[test]
    fn lit_set_mode_stores_after_destruct_per_level_roundtrip() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        r.set_lit_lighting_mode(lit, 1, &mut b).unwrap();
        let view = r.object(lit).unwrap();
        assert_eq!(view.lit_object().unwrap().levels[0].mode, 1);
        assert_eq!(view.lit_object().unwrap().levels[0].resolution, 5.);
        assert_eq!(view.lit_object().unwrap().levels[0].angle, 45.);
        r.set_lit_lighting_mode(lit, 0, &mut b).unwrap();
        r.set_lit_lighting_mode(lit, u32::MAX, &mut b).unwrap();
        assert_eq!(
            r.object(lit).unwrap().lit_object().unwrap().levels[0].mode,
            u32::MAX
        );
        r.set_active_lighting(lit, 5, &mut b).unwrap();
        r.set_lit_lighting_mode(lit, 7, &mut b).unwrap();
        r.set_active_lighting(lit, 0, &mut b).unwrap();
        r.set_lit_lighting_mode(lit, 9, &mut b).unwrap();
        let view = r.object(lit).unwrap().lit_object().unwrap();
        assert_eq!(view.levels[0].mode, 9);
        assert_eq!(view.levels[5].mode, 7);
        for (i, level) in view.levels.iter().enumerate() {
            if i != 0 && i != 5 {
                assert_eq!(level.mode, 2);
            }
        }
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_set_mode_failure_keeps_mode_and_residue() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let plain = r.create_object_resources(None).unwrap();
        assert_eq!(
            r.set_lit_lighting_mode(plain, 1, &mut b),
            Err(ObjectError::UnsupportedVisualSubtype.into())
        );
        assert!(!r.is_poisoned());
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        let lod = r.create_unconstructed_lit_lod(lit, 0).unwrap();
        r.allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap();
        b.fail_release = true;
        assert!(matches!(
            r.set_lit_lighting_mode(lit, 1, &mut b),
            Err(ObjectOperationError::Backend(_))
        ));
        assert_eq!(
            r.object(lit).unwrap().lit_object().unwrap().levels[0].mode,
            2
        );
        assert!(r.is_poisoned());
    }
    #[test]
    fn lit_active_level_switch_bounds_and_residue() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(
            r.set_active_lighting(lit, 6, &mut b),
            Err(ObjectError::InvalidIndex(6).into())
        );
        assert_eq!(r.object(lit).unwrap().lit_object().unwrap().active_level, 0);
        assert!(!r.is_poisoned());
        let lod = r.create_unconstructed_lit_lod(lit, 0).unwrap();
        r.set_active_lighting(lit, 3, &mut b).unwrap();
        assert_eq!(r.object(lit).unwrap().lit_object().unwrap().active_level, 3);
        assert!(r.lod(lod).unwrap().dirty);
        assert_eq!(r.lod(lod).unwrap().lights_changed, u32::MAX);
        assert_eq!(r.lod(lod).unwrap().light_key, u32::MAX);
        r.allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap();
        r.set_active_lighting(lit, 0, &mut b).unwrap();
        b.fail_release = true;
        assert!(r.set_active_lighting(lit, 1, &mut b).is_err());
        assert_eq!(r.object(lit).unwrap().lit_object().unwrap().active_level, 0);
        assert!(r.is_poisoned());
    }
    #[test]
    fn lit_destruct_marks_unconstructed_first_despite_backend_failure() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        r.destruct_active_lighting(lit, &mut b).unwrap();
        assert!(!r.object(lit).unwrap().lit_object().unwrap().levels[0].constructed);
        assert!(!r.is_poisoned());
        let lod = r.create_unconstructed_lit_lod(lit, 2).unwrap();
        r.allocate_plain_lod_vertices(lod, request(2), &mut b)
            .unwrap();
        b.fail_release = true;
        assert!(matches!(
            r.destruct_active_lighting(lit, &mut b),
            Err(ObjectOperationError::Backend(_))
        ));
        assert!(!r.object(lit).unwrap().lit_object().unwrap().levels[0].constructed);
        assert!(r.is_poisoned());
    }
    #[test]
    fn lit_dtor_teardown_retires_all_levels_and_mesh() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let lit = r
            .create_unconstructed_lit_object_resources(Some(mesh))
            .unwrap();
        let lod = r.create_unconstructed_lit_lod(lit, 4).unwrap();
        r.destroy_object_resources(lit, &mut b).unwrap();
        assert_eq!(r.object(lit), Err(ObjectError::UnknownObject(lit)));
        assert_eq!(r.lod(lod), Err(ObjectError::UnknownLod(lod)));
        assert_eq!(r.saved_lighting_lod_count(), 0);
        assert!(r
            .lod_bindings()
            .iter()
            .all(|x| x.state == BindingState::Consumed || x.lod != lod));
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_duplicate_copies_settings_only_with_masked_scalars() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let src = r
            .create_unconstructed_lit_object_resources(Some(mesh))
            .unwrap();
        let dst = r
            .create_unconstructed_lit_object_resources(Some(mesh))
            .unwrap();
        r.set_active_lighting(src, 4, &mut b).unwrap();
        r.set_lit_lighting_mode(src, 1, &mut b).unwrap();
        r.set_active_lighting(src, 0, &mut b).unwrap();
        {
            let record = r.objects[(src.slot - 1) as usize].as_mut().unwrap();
            record.lit_object.as_mut().unwrap().levels[2].resolution = 9.5;
            record.lit_object.as_mut().unwrap().levels[2].angle = 30.;
            record.lightness = 0.5;
            record.visual_flags = 0xffff_ffff;
        }
        r.copy_object_duplicate_state(dst, src).unwrap();
        let (s, d) = (r.object(src).unwrap(), r.object(dst).unwrap());
        let (sl, dl) = (s.lit_object().unwrap(), d.lit_object().unwrap());
        for i in 0..6 {
            assert_eq!(
                (
                    dl.levels[i].mode,
                    dl.levels[i].resolution,
                    dl.levels[i].angle
                ),
                (
                    sl.levels[i].mode,
                    sl.levels[i].resolution,
                    sl.levels[i].angle
                )
            );
            assert_eq!(dl.levels[i].lods, [None; 10]);
        }
        assert_eq!(
            (
                dl.levels[4].mode,
                dl.levels[2].resolution,
                dl.levels[2].angle
            ),
            (1, 9.5, 30.)
        );
        assert_eq!(dl.active_level, 0);
        assert_eq!(d.lightness, 0.5);
        assert_eq!(d.visual_flags, DUPLICATE_VIS_MASK);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_set_mesh_keep_swap_and_same_pointer_noop() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let (first, _, _) = shared_mesh(&mut r, &mut b);
        let (second, _, _) = shared_mesh(&mut r, &mut b);
        let lit = r
            .create_unconstructed_lit_object_resources(Some(first))
            .unwrap();
        r.set_lit_lighting_mode(lit, 1, &mut b).unwrap();
        r.lit_set_object_mesh(lit, Some(second), true, &mut b)
            .unwrap();
        assert_eq!(r.object(lit).unwrap().mesh, Some(second));
        assert_eq!(
            r.object(lit).unwrap().lit_object().unwrap().levels[0].mode,
            1
        );
        r.lit_set_object_mesh(lit, Some(second), true, &mut b)
            .unwrap();
        assert_eq!(r.object(lit).unwrap().mesh, Some(second));
        r.lit_set_object_mesh(lit, Some(first), false, &mut b)
            .unwrap();
        assert_eq!(r.object(lit).unwrap().mesh, Some(first));
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_save_contract_always_refuses_without_state_touch() {
        use super::lit_object::LitSaveContract;
        let mut r = ObjectResources::new().unwrap();
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(
            r.save_lit_lighting_contract(lit),
            Err(ObjectError::UnsupportedVisualSubtype)
        );
        assert_eq!(
            r.object(lit).unwrap().lit_object().unwrap().levels[0].mode,
            2
        );
        assert!(!r.is_poisoned());
        let _ = LitSaveContract;
    }
    #[test]
    fn lit_update_all_marks_level_major_and_skips_nulls() {
        let mut r = ObjectResources::new().unwrap();
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(r.update_all_lit_vertices(lit).unwrap(), 0);
        let a = r.create_plain_lod().unwrap();
        r.lods[(a.slot - 1) as usize].as_mut().unwrap().owner = Some(lit);
        r.lods[(a.slot - 1) as usize].as_mut().unwrap().dirty = false;
        r.objects[(lit.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[0]
            .lods[0] = Some(a);
        let c = r.create_plain_lod().unwrap();
        r.lods[(c.slot - 1) as usize].as_mut().unwrap().owner = Some(lit);
        r.lods[(c.slot - 1) as usize].as_mut().unwrap().dirty = false;
        r.objects[(lit.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[5]
            .lods[9] = Some(c);
        assert_eq!(r.update_all_lit_vertices(lit).unwrap(), 2);
        assert!(r.lod(a).unwrap().dirty);
        assert!(r.lod(c).unwrap().dirty);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_update_lod_column_marks_six_and_enforces_bound() {
        let mut r = ObjectResources::new().unwrap();
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(
            r.update_lit_lod_vertices(lit, 10),
            Err(ObjectError::InvalidIndex(10))
        );
        assert_eq!(r.update_lit_lod_vertices(lit, 9).unwrap(), 0);
        for level in 0..6 {
            let key = r.create_plain_lod().unwrap();
            r.lods[(key.slot - 1) as usize].as_mut().unwrap().owner = Some(lit);
            r.lods[(key.slot - 1) as usize].as_mut().unwrap().dirty = false;
            r.objects[(lit.slot - 1) as usize]
                .as_mut()
                .unwrap()
                .lit_object
                .as_mut()
                .unwrap()
                .levels[level]
                .lods[9] = Some(key);
            r.lit_storage.push(super::LitLodStorage::new(key));
        }
        assert_eq!(r.update_lit_lod_vertices(lit, 9).unwrap(), 6);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_rect_count_and_placements_words() {
        use super::saved_lightmaps::{SavedLightmapLod, SavedLitLod};
        let mut r = ObjectResources::new().unwrap();
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(r.lit_rect_count(lit, 0, 0).unwrap(), 0);
        assert_eq!(r.lit_rect_placements(lit, 0, 0).unwrap(), None);
        assert_eq!(
            r.lit_rect_count(lit, 6, 0),
            Err(ObjectError::InvalidIndex(6))
        );
        assert_eq!(
            r.lit_rect_count(lit, 0, 10),
            Err(ObjectError::InvalidIndex(10))
        );
        let _ = &lit;
        let _ = SavedLitLod::VertexColors {
            flags: 0,
            colors: vec![1],
        };
        let _ = std::mem::size_of::<SavedLightmapLod>();
    }
    #[test]
    fn lit_upload_plan_matches_native_truncation_and_marker() {
        use super::lightmap_atlas::LightmapPlacement;
        let placement = LightmapPlacement {
            x: 60,
            y: 8,
            rotated: false,
            width: 8,
            height: 8,
        };
        let plan = super::lit_rect_upload_plan(&placement, 128, 64, 0.5, 0.125).unwrap();
        assert_eq!((plan.dest_x, plan.dest_y, plan.rotated), (64, 8, false));
        let rotated = super::lit_rect_upload_plan(&placement, 128, 64, 2.5, 0.125).unwrap();
        assert_eq!((rotated.dest_x, rotated.rotated), (64, true));
        // Native y truncates, not rounds: v=0.19 -> y=12, inside the cell.
        let vtrunc = super::lit_rect_upload_plan(&placement, 128, 64, 0.5, 0.19).unwrap();
        assert_eq!(vtrunc.dest_y, 12);
        // Outside the packed cell and out-of-atlas results are refused.
        assert!(super::lit_rect_upload_plan(&placement, 128, 64, 0.5, 0.5).is_none());
        assert!(super::lit_rect_upload_plan(&placement, 128, 64, -0.5, 0.125).is_none());
        assert!(super::lit_rect_upload_plan(&placement, 128, 64, 1.5, 0.125).is_none());
        let origin = LightmapPlacement {
            x: 0,
            y: 0,
            rotated: false,
            width: 4,
            height: 4,
        };
        let edge = super::lit_rect_upload_plan(&origin, 128, 64, 2.0, 0.0).unwrap();
        assert_eq!((edge.dest_x, edge.dest_y, edge.rotated), (0, 0, true));
    }
    #[test]
    fn lit_lock_flag_words_roundtrip_none_and_zero() {
        struct Stub;
        impl super::LitIndexLockBackend for Stub {
            type Error = &'static str;
            fn lock_lit_index_buffer(&mut self, _lod: PlainLodKey) -> Result<(), Self::Error> {
                Ok(())
            }
        }
        let mut r = ObjectResources::new().unwrap();
        let mut stub = Stub;
        let lit = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(
            r.lit_index_lock_flags(lit, 0, 0, 0x1234, &mut stub)
                .unwrap(),
            None
        );
        let lod = r.create_unconstructed_lit_lod(lit, 0).unwrap();
        assert_eq!(
            r.lit_index_lock_flags(lit, 0, 0, u32::MAX, &mut stub)
                .unwrap(),
            Some(0)
        );
        let _ = lod;
        assert!(!r.is_poisoned());
        // Backend lock failures keep their backend carrier (no state-word erasure).
        struct FailStub;
        impl super::LitIndexLockBackend for FailStub {
            type Error = &'static str;
            fn lock_lit_index_buffer(&mut self, _lod: PlainLodKey) -> Result<(), Self::Error> {
                Err("index lock unavailable")
            }
        }
        let mut fail = FailStub;
        assert_eq!(
            r.lit_index_lock_flags(lit, 0, 0, 0, &mut fail),
            Err(ObjectOperationError::Backend("index lock unavailable"))
        );
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_construct_plan_guards_mesh_and_tears_down_active() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let bare = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(
            r.plan_lit_construct(bare, 3.0, &mut b),
            Err(super::LitConstructError::MissingMesh)
        );
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let lit = r
            .create_unconstructed_lit_object_resources(Some(mesh))
            .unwrap();
        let lod = r.create_unconstructed_lit_lod(lit, 1).unwrap();
        let plan = r.plan_lit_construct(lit, 3.0, &mut b).unwrap();
        assert_eq!(plan.lighting, 0);
        assert_eq!(plan.resolution, 3.0);
        assert_eq!(
            r.object(lit).unwrap().lit_object().unwrap().levels[0].resolution,
            3.0
        );
        assert!(r.lod(lod).is_err());
        // Backend failure inside the teardown keeps its backend carrier (fresh
        // arena so the failing release empties its buffer, like the SetMode
        // residue test above).
        let mut r2 = ObjectResources::new().unwrap();
        let mut b2 = Backend::default();
        let mesh2 = r2.create_mesh_object().unwrap();
        let lit2 = r2
            .create_unconstructed_lit_object_resources(Some(mesh2))
            .unwrap();
        let lod2 = r2.create_unconstructed_lit_lod(lit2, 0).unwrap();
        r2.allocate_plain_lod_vertices(lod2, request(2), &mut b2)
            .unwrap();
        b2.fail_release = true;
        assert!(matches!(
            r2.plan_lit_construct(lit2, 3.0, &mut b2),
            Err(super::LitConstructError::Backend(_))
        ));
        assert!(r2.is_poisoned());
    }
    #[test]
    fn lit_memory_sum_order_count_words_and_residue() {
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let bare = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(r.total_lit_lightmap_memory(bare).unwrap(), 0);
        let (mesh, _, _) = shared_mesh(&mut r, &mut b);
        let lit = r
            .create_unconstructed_lit_object_resources(Some(mesh))
            .unwrap();
        assert_eq!(r.total_lit_lightmap_memory(lit).unwrap(), 0);
        assert!(!r.is_poisoned());
    }
    #[test]
    fn lit_queue_preserves_link_order_first_offsets_counts_and_lm_sort() {
        use super::{
            LitQueueGroup, LitQueueMaterialClass, LitQueueRequest, LitQueuedGroup, LitSortWords,
        };
        fn sort_words() -> LitSortWords {
            LitSortWords {
                dist_word: 0x100,
                dist_mask: 0x1ff,
                alpha_bit: 0x4000_0000,
                mat_mask: 0x7fff,
                ckey_bit: 0x2000,
            }
        }
        let req = LitQueueRequest {
            constructed: true,
            lightmap: true,
            lm_uid: 0x9,
            sort: sort_words(),
            groups: vec![
                LitQueueGroup {
                    faces: 2,
                    material: LitQueueMaterialClass::Untextured,
                    texture_uid: 0,
                },
                LitQueueGroup {
                    faces: 1,
                    material: LitQueueMaterialClass::Opaque,
                    texture_uid: 0x1_2345,
                },
                LitQueueGroup {
                    faces: 4,
                    material: LitQueueMaterialClass::Transparent,
                    texture_uid: 0x999,
                },
            ],
        };
        let queue = super::queue_lit_face_groups(&req);
        assert_eq!(queue.entries.len(), 3);
        assert_eq!((queue.num_opaque, queue.num_transparent), (2, 1));
        assert_eq!(
            queue.entries[0],
            LitQueuedGroup {
                group: 0,
                first: Some(0),
                texture_uid: None,
                env_texture_uid: Some(0x9),
                sort_value: 0,
                transparent: false,
            }
        );
        assert_eq!(queue.entries[1].first, Some(6));
        assert_eq!(queue.entries[1].sort_value, 0x4_a345);
        assert_eq!(queue.entries[2].first, Some(9));
        assert_eq!(queue.entries[2].sort_value, 0x100 | 0x4000_0000);
        assert!(queue.entries[2].transparent);
    }
    #[test]
    fn lit_queue_ckey_packs_evidence_bit_and_unconstructed_uses_own_sort() {
        use super::{LitQueueGroup, LitQueueMaterialClass, LitQueueRequest, LitSortWords};
        let sort = LitSortWords {
            dist_word: 0x100,
            dist_mask: 0x1ff,
            alpha_bit: 0x4000_0000,
            mat_mask: 0x7fff,
            ckey_bit: 0x2000,
        };
        let mut req = LitQueueRequest {
            constructed: true,
            lightmap: true,
            lm_uid: 0x9,
            sort,
            groups: vec![LitQueueGroup {
                faces: 1,
                material: LitQueueMaterialClass::KeyAlpha,
                texture_uid: 0x7,
            }],
        };
        let queue = super::queue_lit_face_groups(&req);
        assert_eq!(queue.entries[0].sort_value, 0x4004_8007);
        assert_eq!(queue.entries[0].first, Some(0));
        req.constructed = false;
        let flat = super::queue_lit_face_groups(&req);
        assert!(flat.entries.iter().all(|e| e.first.is_none()));
        assert!(flat.entries.iter().all(|e| e.env_texture_uid.is_none()));
        assert_eq!(flat.entries[0].sort_value, 0x7 | 0x2000);
    }
    #[test]
    fn lit_create_plan_arms_and_position_release_prefix() {
        use super::{LitCreateOrder, LitCreatePlan};
        assert_eq!(
            super::LIT_CREATE_VB_FAIL,
            "litLod::Create() - Error create VertexBuffer for LM !!"
        );
        assert_eq!(
            super::plan_lit_lod_create(true, false, true, 0, false, false),
            LitCreateOrder {
                release_positions: true,
                plan: LitCreatePlan::InitialUnlit
            }
        );
        assert_eq!(
            super::plan_lit_lod_create(false, true, false, 3, false, true).plan,
            LitCreatePlan::VertexColors {
                double_saturate: true
            }
        );
        assert_eq!(
            super::plan_lit_lod_create(true, true, false, 5, true, false).plan,
            LitCreatePlan::HwtlLightmap
        );
        assert_eq!(
            super::plan_lit_lod_create(true, true, true, 5, false, false),
            LitCreateOrder {
                release_positions: false,
                plan: LitCreatePlan::PositionAlloc {
                    reuse_positions: true
                },
            }
        );
    }
    #[test]
    fn lit_vertex_color_doubled_saturates_per_instruction_widths() {
        assert_eq!(super::lit_vertex_color_doubled(0x0000_0000), 0x0000_0000);
        assert_eq!(super::lit_vertex_color_doubled(0xffff_ffff), 0xffff_ffff);
        assert_eq!(super::lit_vertex_color_doubled(0xff10_2030), 0xff20_4060);
        assert_eq!(super::lit_vertex_color_doubled(0x8080_8080), 0x80ff_ffff);
    }
    #[test]
    fn lit_drop_plan_branches_follow_dtor_arms() {
        use super::saved_lightmaps::SavedLitLod;
        assert_eq!(
            super::lit_lod_drop_plan(None),
            super::LitLodDropPlan {
                free_positions: false,
                free_colors: false
            }
        );
        assert_eq!(
            super::lit_lod_drop_plan(Some(&SavedLitLod::VertexColors {
                flags: 0,
                colors: vec![1]
            })),
            super::LitLodDropPlan {
                free_positions: false,
                free_colors: true
            }
        );
    }
    #[test]
    fn lit_postprocess_failure_prefixes_match_original_words() {
        assert_eq!(
            super::LIT_QUEUE_OOM,
            "!! Out of memory    renderLitObject()"
        );
        assert_eq!(
            super::LIT_POSTPROCESS_OOM_THIS,
            "!! out of memory  litLod::postLoadProcess()  this:0x%x"
        );
        assert_eq!(
            super::LIT_POSTPROCESS_LM_FAIL,
            "!! can't create LM  litLod::postLoadProcess()  this:0x%x"
        );
        assert_eq!(
            super::LIT_POSTPROCESS_IB_OOM,
            "!! Out of memory  litLod::postLoadProcess"
        );
        assert_eq!(
            super::LIT_POSTPROCESS_UV_CHECK,
            "!! neklaply kontroly m_pUV:0x%x  m_pLitUV:0x%x  litLod::postLoadProcess()"
        );
        assert_eq!(
            super::LIT_POSTPROCESS_CORRUPT,
            "!! corrupted lightmap  litLod::postProcess()  m_numRectangles:%d < pVI->rect:%d"
        );
        assert_eq!(
            super::LIT_GET_LVERTEX_MISS,
            "!! vertex not found GetLVertex() vertex:%d rectangle:%d"
        );
    }
    #[test]
    fn lit_postprocess_corrupt_rectangle_keeps_payload_and_phase() {
        use super::lightmap_atlas::{LightmapAtlasPool, LightmapRectangle, LightmapTexels};
        use super::saved_lightmaps::{
            SavedLightingLoad, SavedLightmapLod, SavedLitLevel, SavedLitLod, SavedLitVertex,
        };
        let mut pool = LightmapAtlasPool::default();
        let (atlas, placements, allocation) = pool
            .pack(&[LightmapRectangle {
                width: 3,
                height: 3,
                texels: LightmapTexels::Single(0xff20_4080),
            }])
            .unwrap();
        // The allocation lease keeps the page alive across retention.
        pool.retain_live_pages();
        assert_eq!(pool.statistics().live_pages, 1);
        let payload = SavedLitLod::Lightmap(SavedLightmapLod {
            flags: 0,
            vertices: vec![SavedLitVertex {
                source_vertex: 0,
                rectangle: 5,
                lightmap_uv: [0., 0.],
            }],
            indices: vec![0, 1, 2],
            face_rectangles: vec![vec![0]],
            placements,
            atlas,
            memory_bytes: 36,
            _allocation: allocation,
        });
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let object = r.create_unconstructed_lit_object_resources(None).unwrap();
        let mut levels: [Option<SavedLitLevel>; 6] = std::array::from_fn(|_| None);
        levels[0] = Some(SavedLitLevel {
            mode: 2,
            resolution: 5.,
            angle: 45.,
            constructed: true,
            lods: std::array::from_fn(|i| if i == 0 { Some(payload.clone()) } else { None }),
        });
        r.install_saved_lighting(
            object,
            SavedLightingLoad {
                levels,
                bytes_read: 0,
            },
            &mut b,
        )
        .unwrap();
        let key = r.object(object).unwrap().lit_object().unwrap().levels[0].lods[0].unwrap();
        r.begin_lit_preparation(key).unwrap();
        let source = super::LitSourceBinding {
            level: r.create_mesh_level().unwrap(),
            vertices: CompleteVertexBuffer::default(),
            vertex_count: 0,
            revision: 0,
        };
        assert!(matches!(
            r.apply_lit_postprocess_order(
                object,
                0,
                0,
                super::LitPostprocessRequest {
                    face_group_faces: &[1],
                    loaded: false,
                    source,
                },
                &mut b
            ),
            Err(ObjectOperationError::State(ObjectError::InvalidIndex(_)))
        ));
        assert!(r.object(object).unwrap().lit_object().unwrap().levels[0].saved_lods[0].is_some());
        assert_eq!(
            r.lit_lod_storage(key).unwrap().phase,
            super::LitStoragePhase::Preparing
        );
    }
    #[test]
    fn lit_postprocess_vertex_colors_finishes_without_index_work() {
        use super::saved_lightmaps::{SavedLightingLoad, SavedLitLevel, SavedLitLod};
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let object = r.create_unconstructed_lit_object_resources(None).unwrap();
        let payload = SavedLitLod::VertexColors {
            flags: 0,
            colors: vec![1, 2, 3],
        };
        let mut levels: [Option<SavedLitLevel>; 6] = std::array::from_fn(|_| None);
        levels[0] = Some(SavedLitLevel {
            mode: 0,
            resolution: 5.,
            angle: 45.,
            constructed: true,
            lods: std::array::from_fn(|i| if i == 0 { Some(payload.clone()) } else { None }),
        });
        r.install_saved_lighting(
            object,
            SavedLightingLoad {
                levels,
                bytes_read: 0,
            },
            &mut b,
        )
        .unwrap();
        let key = r.object(object).unwrap().lit_object().unwrap().levels[0].lods[0].unwrap();
        r.begin_lit_preparation(key).unwrap();
        let source = super::LitSourceBinding {
            level: r.create_mesh_level().unwrap(),
            vertices: CompleteVertexBuffer::default(),
            vertex_count: 0,
            revision: 0,
        };
        assert_eq!(
            r.apply_lit_postprocess_order(
                object,
                0,
                0,
                super::LitPostprocessRequest {
                    face_group_faces: &[1],
                    loaded: true,
                    source,
                },
                &mut b
            )
            .unwrap(),
            super::LitPostprocessOutcome::Ready(super::LitPostprocessCounts {
                memory_bytes: 0,
                lit_vertices: 3,
                indices: 0,
            })
        );
        assert!(!r.lod(key).unwrap().dirty);
        assert_eq!(
            r.lit_lod_storage(key).unwrap().phase,
            super::LitStoragePhase::Ready
        );
    }
    #[test]
    fn lit_destroy_clears_slots_retires_record_and_reports_plan() {
        use super::saved_lightmaps::{SavedLightingLoad, SavedLitLevel, SavedLitLod};
        let mut r = ObjectResources::new().unwrap();
        let mut b = Backend::default();
        let object = r.create_unconstructed_lit_object_resources(None).unwrap();
        let payload = SavedLitLod::VertexColors {
            flags: 0,
            colors: vec![1],
        };
        let mut levels: [Option<SavedLitLevel>; 6] = std::array::from_fn(|_| None);
        levels[0] = Some(SavedLitLevel {
            mode: 0,
            resolution: 5.,
            angle: 45.,
            constructed: true,
            lods: std::array::from_fn(|i| if i == 0 { Some(payload.clone()) } else { None }),
        });
        r.install_saved_lighting(
            object,
            SavedLightingLoad {
                levels,
                bytes_read: 0,
            },
            &mut b,
        )
        .unwrap();
        let key = r.object(object).unwrap().lit_object().unwrap().levels[0].lods[0].unwrap();
        assert_eq!(
            r.destroy_lit_lod_allocation(object, 0, 0, &mut b).unwrap(),
            super::LitLodDropPlan {
                free_positions: false,
                free_colors: true
            }
        );
        let lit = r.object(object).unwrap().lit_object().unwrap();
        assert!(lit.levels[0].lods[0].is_none());
        assert!(lit.levels[0].saved_lods[0].is_none());
        assert!(matches!(r.lod(key), Err(ObjectError::UnknownLod(_))));
        assert_eq!(
            r.lit_lod_storage_records()
                .iter()
                .find(|s| s.lod == key)
                .unwrap()
                .phase,
            super::LitStoragePhase::Retired
        );
        assert_eq!(
            r.destroy_lit_lod_allocation(object, 0, 3, &mut b).unwrap(),
            super::LitLodDropPlan {
                free_positions: false,
                free_colors: false
            }
        );
    }
    #[test]
    fn lit_update_gate_reports_moved_and_recreated_resets_light_keys() {
        let mut r = ObjectResources::new().unwrap();
        let object = r.create_unconstructed_lit_object_resources(None).unwrap();
        let key = r.create_unconstructed_lit_lod(object, 0).unwrap();
        assert!(r.sync_lit_update_count(key, 7).unwrap());
        assert_eq!(r.lod(key).unwrap().update_count, 7);
        assert!(!r.sync_lit_update_count(key, 7).unwrap());
        r.note_lit_lod_recreated(key).unwrap();
        assert_eq!(r.lod(key).unwrap().lights_changed, u32::MAX);
        assert_eq!(r.lod(key).unwrap().light_key, u32::MAX);
    }
    #[test]
    fn lit_render_entry_reports_the_device_boundary() {
        let mut r = ObjectResources::new().unwrap();
        let object = r.create_unconstructed_lit_object_resources(None).unwrap();
        assert_eq!(
            r.render_lit_face_group_contract(object),
            Err(ObjectError::UnsupportedVisualSubtype)
        );
    }
    #[test]
    fn lit_load_failure_words_and_level_remap() {
        use super::lightmap_atlas::LightmapAtlasPool;
        use super::saved_lightmaps::{read_saved_lighting, SavedLightingError};
        let mut pool = LightmapAtlasPool::default();
        fn level_bytes(version: u8, mode: u8, lods: u32, level: u8) -> Vec<u8> {
            let mut bytes = vec![1];
            bytes.push(version);
            bytes.push(mode);
            bytes.extend(lods.to_le_bytes());
            bytes.extend(5f32.to_le_bytes());
            bytes.extend(45f32.to_le_bytes());
            bytes.push(level);
            bytes
        }
        fn mesh() -> Vec<super::saved_lightmaps::SavedLightMeshLod> {
            vec![super::saved_lightmaps::SavedLightMeshLod {
                vertex_count: 3,
                face_group_faces: vec![1],
            }]
        }
        let bytes = level_bytes(0x20, 2, 1, 1);
        let failure = read_saved_lighting(&bytes, &mesh(), &mut pool).unwrap_err();
        assert_eq!(failure.error, SavedLightingError::Version(0x20));
        let level = failure.partial.levels[0].as_ref().unwrap();
        assert_eq!((level.mode, level.resolution, level.angle), (2, 5., 45.));
        assert!(!level.constructed);
        let bytes = level_bytes(0x21, 2, 2, 1);
        let failure = read_saved_lighting(&bytes, &mesh(), &mut pool).unwrap_err();
        assert_eq!(failure.error, SavedLightingError::LodCount);
        let failure = read_saved_lighting(&[1, 0x21], &mesh(), &mut pool).unwrap_err();
        assert_eq!(failure.error, SavedLightingError::Truncated);
    }
}

#[path = "object_lod.rs"]
pub mod object_lod;
pub use object_lod::ObjectLodError;
pub(crate) use object_lod::{render_derived_visual_fade, render_fade};

#[cfg(test)]
mod face_group_queue_tests {
    use super::*;

    #[test]
    fn add_fg_help_ctor_pins_grow_and_zero_word() {
        // LS3DF RVA 0x659a0 `ADD_FG_HELP_STRUCT::ctor`
        // (mafia-re ls3df/I3D/I3D.h:4694-4698, working name).
        let queue = FaceGroupQueue::new();
        assert_eq!(queue.unknown_0c, 0);
        assert_eq!(queue.fgroup_grow, 0x400);
        assert!(queue.fgroups.is_empty());
        assert!(queue.sort.is_empty());
        assert_eq!((queue.num_opaque, queue.num_transparent), (0, 0));
        assert_eq!(
            std::mem::size_of::<FaceGroupQueueEntry>(),
            0x20,
            "RENDER_FG_HELP_STRUCT width"
        );
        let mut queued = FaceGroupQueue::new();
        queued.fgroups.push(FaceGroupQueueEntry([1; 8]));
        queued.sort.push(7);
        queued.num_opaque = 1;
        queued.clear_queue();
        assert!(queued.fgroups.is_empty() && queued.sort.is_empty());
        assert_eq!((queued.num_opaque, queued.num_transparent), (0, 0));
        assert_eq!(queued.fgroup_grow, 0x400);
    }
}
