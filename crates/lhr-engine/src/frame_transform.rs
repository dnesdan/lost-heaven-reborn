//! Lazy LS3DF frame caches and the closed model/mesh/dummy/joint hierarchy subset.
//! UpdateWMatrixProc is an original DLL export, RVA0x1c7d0. UpdateMatrix,
//! LinkTo, RemoveChild and Update have retained PS2 symbols; PC entries are
//! 0x1a6e0,0x1b9d0,0x1bf20,0x1ca60. Setter names are descriptive views of
//! observed inline operations, not additional original symbols. ModelArena owns
//! references; this graph provides bounded model/dummy/joint/Target teardown.
//! ModelArena also composes attached plain-object resource teardown before the
//! visual/base destructor. Unbound/derived visuals and external registrations
//! remain unavailable.
//! Only LinkTo flags0/1,
//! model/mesh/dummy/joint/Target virtual updates and the owned deferred Target queue
//! are represented.
//! Joint cache and singlemesh discovery/binding use owned graph identities;
//! loaded-definition bounds and inherited visual updates are represented;
//! singlemesh rendering/resource backends remain separate.
use crate::{
    joint_state::{I3dJointState, SinglemeshInvalidationState},
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::{SMatrix, SQuat, vector_set_normalized},
    singlemesh_binding::{DBuildBindingOutcome, I3dSinglemeshBinding},
    singlemesh_bounds::{
        calculate_singlemesh_bounds, SinglemeshBoundVolume, SinglemeshBoundingBox,
        SinglemeshBoundsError,
    },
    target_state::{
        I3dTargetState, TargetFrameBackend, TargetRegistry, TargetUpdateError, TargetUpdateQueue,
    },
};
const DIRTY: u32 = 0x40002000;
#[path = "frame_lensflare_parameters.rs"]
mod lensflare_parameters;
pub(crate) use lensflare_parameters::LensflareParameter;
pub use lensflare_parameters::{lensflare_admit_glow, lensflare_fade_step, LensflareFadePlan};
pub use lensflare_parameters::{
    lensflare_plain_set_fade, lensflare_plain_set_intensity, lensflare_plain_set_range,
    lensflare_plain_set_size, lensflare_plain_set_test_interval,
};
pub use lensflare_parameters::{
    LensflareGlowError, LensflareParameterUnavailable, LensflareParameters, LensflareReleaseCursor,
    LENSFLARE_FIXED_EXTENT, LENSFLARE_FIXED_RADIUS,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameKey(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    Camera,
    Occluder,
    Scene,
    Sector,
    Model,
    Mesh,
    /// Native LENS visual; glow records and materials remain source-owned.
    Lensflare,
    /// Native visual type1, projector subtype PROJ.
    Projector,
    Light,
    Sound,
    Dummy,
    Area,
    Joint,
    Target,
    /// A Joint::UpdateV target with optional owned binding fields. Only an
    /// owned binding admits the inherited visual update and loaded bounds path.
    Singlemesh,
}
/// Fields observed by these methods, not a complete native object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct I3dFrameState {
    pub lensflare_parameters: Option<LensflareParameters>,
    pub kind: FrameKind,
    pub local: SMatrix,
    pub world: SMatrix,
    pub scale: [f32; 3],
    pub rotation: SQuat,
    pub flags: u32,
    pub world_scale: f32,
    pub update_count: u32,
    /// Original frame+0x13c: the driver's render number of the last render.
    /// Constructor value only; actual rendering writes require its producer.
    pub render_number: u32,
    pub visual_flags: u32,
    /// Native dummy-only field at +0x198; independent of mesh visual flags.
    pub dummy_flags: u32,
    /// Projector-only native cache word +0x258, cleared by UpdateV464a0.
    pub projector_cache_258: u32,
}
impl I3dFrameState {
    /// Actual model/base-frame constructor field projection.
    pub fn model() -> Self {
        Self {
            lensflare_parameters: None,
            kind: FrameKind::Model,
            local: SMatrix::identity(),
            world: SMatrix::identity(),
            scale: [1.; 3],
            rotation: SQuat {
                wxyz: [1., 0., 0., 0.],
            },
            flags: 0x4000003d,
            world_scale: 1.,
            update_count: 0,
            render_number: u32::MAX,
            visual_flags: 0,
            dummy_flags: 0,
            projector_cache_258: 0,
        }
    }
    /// PC ctor flag word at RVA 0x1a7f0 (LS3DF.dll e211c850…):
    /// UPDATE|WMAT_VALID|MAT_VALID|QUAT_VALID|ROT_VALID|ON = 0x4000003d.
    /// Descriptive view of the observed flag store, not an original symbol.
    /// Tier differential: Ghidra body FUN_1001a7f0 (397B/87 insn/1BB, Identity
    /// callees, flag/Invalidate/renderNumber stores) + the pre-existing
    /// model()/dummy()/joint()/target() projection re-affirmed by ctor-word
    /// tests (no new constructor code).
    pub fn ctor_flags() -> u32 {
        0x4000003d
    }
    /// Original standard Object/LitObject common constructor projection,
    /// RVAs0x367b0/0x21570. Mesh payload and visual subtype remain loader-owned.
    pub fn mesh() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Mesh;
        state.visual_flags = 0x2a00;
        state
    }
    /// I3D_lensflare constructor (PC RVA 0x09520) inherits I3D_visual
    /// (RVA 0x5a200). Its glows are owned by the source model.
    pub fn lensflare() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Lensflare;
        state.visual_flags = 0x2a00;
        state.lensflare_parameters = Some(LensflareParameters::constructed());
        state
    }
    /// Original dummy constructor projection, RVA0x1a1e0. Bounds and other
    /// derived payload fields are owned by the loader arena, outside this graph.
    pub fn dummy() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Dummy;
        state
    }
    /// Common I3D_frame constructor used by I3D_sound, PC RVA0x57650.
    /// Sound sources, playback and the derived tail are owned by RuntimeAudio.
    pub fn sound() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Sound;
        state
    }
    /// Common base-frame projection of the PC Joint constructor, RVA0x1ed30.
    /// The graph separately owns the derived joint fields.
    pub fn joint() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Joint;
        state
    }
    /// Original Target base-frame constructor projection, RVA 0x59490.
    /// Derived links and target flags are registered separately.
    pub fn target() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Target;
        state
    }
    /// Supported FrameLoadImage result. Caller separately proves mesh type/flags.
    pub fn source_mesh(position: [f32; 3], scale: [f32; 3], rotation: SQuat) -> Self {
        let mut s = Self::model();
        s.kind = FrameKind::Mesh;
        s.local.elements[12..15].copy_from_slice(&position);
        s.scale = scale;
        s.rotation = rotation;
        s.flags = 0x40000009;
        s.visual_flags = 0x2a00;
        s
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameLinks {
    pub parent: Option<FrameKey>,
    pub child: Option<FrameKey>,
    pub previous: Option<FrameKey>,
    pub next: Option<FrameKey>,
    pub child_count: u32,
}
#[derive(Clone, Debug)]
struct Node {
    alive: bool,
    state: I3dFrameState,
    links: FrameLinks,
    joint: Option<I3dJointState>,
    light: Option<crate::frame_light::I3dLightState>,
    singlemesh: Option<SinglemeshInvalidationState>,
    binding: Option<I3dSinglemeshBinding>,
    // Host availability markers, not original frame fields. An interrupted
    // guarded computation must not later masquerade as a valid native cache.
    local_error: Option<FrameError>,
    world_error: Option<FrameError>,
    joint_error: Option<FrameError>,
    singlemesh_bound_error: Option<FrameError>,
    /// Cached dummy inverse matrix for I3D_dummy::GetInvMatrix (PC RVA
    /// 0x68840, native +0x158). Set only by the lazy getter; the 0x10 flag
    /// is the validity bit.
    dummy_inv: Option<SMatrix>,
    /// Interrupted dummy inverse computation. Retains the native set-flag
    /// order while keeping a partial matrix unavailable to later readers.
    dummy_inv_error: Option<FrameError>,
}
#[derive(Clone, Copy, Debug)]
struct Visit {
    key: FrameKey,
    child: Option<FrameKey>,
    dirty: bool,
}
#[derive(Debug, Default)]
pub struct I3dFrameGraph {
    nodes: Vec<Node>,
    world_path: Vec<FrameKey>,
    visits: Vec<Visit>,
    targets: TargetRegistry,
    target_updates: TargetUpdateQueue,
    deferred_error: Option<FrameError>,
    /// Sector +0x140 invalidation counters shared with registered lights.
    pub(crate) sector_light_changes: std::collections::BTreeMap<u32, u32>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    LensflareParameterUnavailable(LensflareParameterUnavailable),
    UnknownFrame(FrameKey),
    IdentityOverflow,
    Cycle,
    NumericDomain(&'static str),
    QuaternionFromMatrixUnavailable,
    AllocationOverflow,
    UnsupportedLinkFlags(u32),
    UnsupportedLinkState,
    UnsupportedDestruction(FrameKey),
    UnsupportedFrameUpdate(FrameKey),
    UnsupportedDestructionRegistrations(FrameKey, u32),
    DirectionFromVectorsUnavailable,
    NotJoint(FrameKey),
    InvalidSinglemeshTarget(FrameKey),
    UnsupportedSinglemeshUpdate(FrameKey),
    JointMatrixUnavailable(FrameKey),
    SinglemeshBindingUnavailable(FrameKey),
    SinglemeshBoundsUnavailable(FrameKey),
    SinglemeshBoundsDefinitionTooShort(FrameKey),
    NotTarget(FrameKey),
    TargetUnavailable(FrameKey),
    UnsupportedTargetFlags(FrameKey, u32),
    TargetDirectionUnavailable(FrameKey),
}
/// Upstream working names for SHADOW_MODE_0/1/2 (mafia-re
/// I3d_visual.cpp:55-103), adapted to snake_case. Shared by the shadow and
/// projector mode words (PC RVAs 0x5a410/0x5a480).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowMode {
    Mode0 = 0,
    Mode1 = 1,
    Mode2 = 2,
}
impl ShadowMode {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Mode0),
            1 => Some(Self::Mode1),
            2 => Some(Self::Mode2),
            _ => None,
        }
    }
}
/// Device calls behind visual SetSpace (PC RVA 0x5a3c0) stay backend
/// contracts: VisualType2String + dbgPrintf. Getters run in native order
/// (type, then name) before the print.
pub trait VisualSpaceBackend {
    type Error;
    fn visual_type_label(&mut self, visual_type: u32) -> Result<String, Self::Error>;
    fn frame_name(&mut self) -> Result<Option<String>, Self::Error>;
    fn debug_print_unsupported_set_space(
        &mut self,
        type_label: &str,
        frame_name: Option<&str>,
    ) -> Result<(), Self::Error>;
}
/// Original I3D_void_interface setters, PC RVAs 0x5a760/0x5a770 (contract:
/// 2-insn walks, no Ghidra body; mafia-re I3d_visual.cpp:157-167). Params
/// unused; observed RET cleanups 0x10/0xc retained as evidence. SetWeight
/// returns I3DERR_GENERIC 0x80000005; GetWeight returns +0.0
/// (byte-verified const).
pub fn visual_void_set_weight(_region: i32, _level: i32, _weight: f32) -> i32 {
    0x8000_0005u32 as i32
}
pub fn visual_void_get_weight(_region: i32, _level: i32) -> f32 {
    0.0
}
/// Working name for `I3D_void_interface::NumLevels`
/// (mafia-re ls3df/I3D/visuals/I3d_visual.cpp:145-148). The reference
/// records no LS3DF FUNCTION address for it (plain .cpp definition without
/// a FUNCTION annotation), so no RVA is claimed. Void interfaces carry no
/// levels, hence the constant zero.
pub fn visual_void_num_levels() -> i32 {
    0
}
/// Working name for `I3D_void_interface::NumRegions`
/// (mafia-re ls3df/I3D/visuals/I3d_visual.cpp:150-153). The reference
/// records no LS3DF FUNCTION address for it (plain .cpp definition without
/// a FUNCTION annotation), so no RVA is claimed. Void interfaces carry no
/// regions, hence the constant zero.
pub fn visual_void_num_regions() -> i32 {
    0
}
/// Working name for the flag word written by the LS3DF-inlined
/// `I3D_frame::SetRot` (mafia-re ls3df/I3D/I3D.h:420-437, LS3DF_EXPORTS
/// branch). The reference records no LS3DF FUNCTION address for it (header
/// inline compiled into LS3DF; the only out-of-line copy is Game.exe
/// 0x43bce0, which is not claimed here). Clears HR_BOUND_W_VALID,
/// WMAT_VALID, MAT_VALID and ROT_VALID, then sets UPDATE and QUAT_VALID.
/// The quaternion store and the Game.exe normalization branch stay
/// caller-owned; only the flag word is modeled.
pub fn frame_flags_after_set_rot(flags: u32) -> u32 {
    (flags & !(0x100 | 0x20 | 0x10 | 0x04)) | 0x4000_0000 | 0x08
}
/// Working name for the flag word written by the LS3DF-inlined
/// `I3D_frame::SetPos` (mafia-re ls3df/I3D/I3D.h:447-455). The reference
/// records no LS3DF FUNCTION address for it (header inline; the only
/// out-of-line copy is Game.exe 0x460980, which is not claimed here).
/// A locked frame (FRMFLAGS_LOCKED, 0x80) stores nothing and keeps its
/// flags, reported here as `None`. Otherwise the translation store clears
/// HR_BOUND_W_VALID and WMAT_VALID and sets UPDATE; the matrix store stays
/// caller-owned.
pub fn frame_flags_after_set_pos(flags: u32) -> Option<u32> {
    if flags & 0x80 != 0 {
        return None;
    }
    Some((flags & !(0x100 | 0x20)) | 0x4000_0000)
}
/// Working name for the flag word written by the LS3DF-inlined
/// `I3D_frame::SetLocalMatrix` (mafia-re ls3df/I3D/I3D.h:495-503). The
/// reference records no LS3DF FUNCTION address for it and marks no Game.exe
/// out-of-line copy. Clears WMAT_VALID and QUAT_VALID, then sets UPDATE,
/// MAT_VALID and ROT_VALID. The matrix store and the scale remake from its
/// basis rows stay caller-owned.
pub fn frame_flags_after_set_local_matrix(flags: u32) -> u32 {
    (flags & !(0x20 | 0x08)) | 0x4000_0000 | 0x10 | 0x04
}
/// Working name for the flag word written by the LS3DF-inlined
/// `I3D_frame::SetDir1` (mafia-re ls3df/I3D/I3D.h:467-475). The reference
/// records no LS3DF FUNCTION address for it (header inline; the only
/// out-of-line copy is Game.exe 0x4609c0, which is not claimed here).
/// Clears HR_BOUND_W_VALID, WMAT_VALID, QUAT_VALID and ROT_VALID, then sets
/// UPDATE and MAT_VALID. The SetDir3 matrix build stays caller-owned.
pub fn frame_flags_after_set_dir1(flags: u32) -> u32 {
    (flags & !(0x100 | 0x20 | 0x08 | 0x04)) | 0x4000_0000 | 0x10
}
/// Working name for the flag word written by the LS3DF-inlined
/// `I3D_frame::SetScale` (mafia-re ls3df/I3D/I3D.h:483-493). The reference
/// records no LS3DF FUNCTION address for it (header inline; the only
/// out-of-line copy is Game.exe 0x47a750, which is not claimed here).
/// Clears HR_BOUND_W_VALID, WMAT_VALID, MAT_VALID and ROT_VALID, then sets
/// UPDATE and QUAT_VALID. The scale store and the conditional Make call
/// stay caller-owned (covered by `assign_scale`).
pub fn frame_flags_after_set_scale(flags: u32) -> u32 {
    (flags & !(0x100 | 0x20 | 0x10 | 0x04)) | 0x4000_0000 | 0x08
}
/// Working name for the flag effects of the LS3DF-inlined
/// `I3D_frame::UpdateMatrix` (mafia-re ls3df/I3D/I3D.h:382-405). The
/// reference records LS3DF 0x1001a6e0 for the out-of-line copy, but that
/// address is already occupied in docs/progress.json retained entries, so no
/// RVA is claimed here. Only the flag word is modeled: a matrix-stale frame gains
/// MAT_VALID and ROT_VALID, otherwise a rotation-stale frame gains
/// ROT_VALID and QUAT_VALID. The SetRot3 row scaling and the Make call stay
/// caller-owned.
pub fn frame_flags_after_update_matrix(flags: u32) -> u32 {
    if flags & 0x10 == 0 {
        flags | 0x10 | 0x04
    } else if flags & 0x04 == 0 {
        flags | 0x04 | 0x08
    } else {
        flags
    }
}
/// Working name for the FRMFLAGS_ON bit test (mafia-re ls3df/I3D/I3D.h:259).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is only read in place by the inlined `IsOn` (I3D.h:543-549; the
/// only out-of-line copy is Game.exe 0x47acf0, unclaimed here). The arena
/// `frame_is_on` method covers stateful reads; this is the pure word test
/// completing the free-reader family. Reports visibility; no cache moves.
pub fn frame_is_on_flag(flags: u32) -> bool {
    flags & 0x1 != 0
}
/// Working name for the FRMFLAGS_LOCKED bit test (mafia-re ls3df/I3D/I3D.h:265).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit only appears inside inlined stores such as `SetPos`. Reports
/// whether the frame position is locked; no cache moves.
pub fn frame_is_locked(flags: u32) -> bool {
    flags & 0x80 != 0
}
/// Working name for the FRMFLAGS_TICK bit test (mafia-re ls3df/I3D/I3D.h:281).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is maintained by the scene tick-list setters. Reports whether the
/// frame sits in its scene tick list; no cache moves.
pub fn frame_is_in_tick_list(flags: u32) -> bool {
    flags & 0x40_0000 != 0
}
/// Working name for the FRMFLAGS_UPDATE bit test (mafia-re ls3df/I3D/I3D.h:287).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is set by every transform setter. Reports whether the frame still
/// needs an update; no cache moves.
pub fn frame_update_is_pending(flags: u32) -> bool {
    flags & 0x4000_0000 != 0
}
/// Working name for the FRMFLAGS_NO_CULL bit test (mafia-re ls3df/I3D/I3D.h:282-284).
/// The reference records no LS3DF FUNCTION address for a standalone reader.
/// Reports whether portal culling skips the frame and its children; no cache
/// moves.
pub fn frame_is_no_cull(flags: u32) -> bool {
    flags & 0x0800_0000 != 0
}
/// Working name for the FRMFLAGS_DUPLICATE_MASK projection (mafia-re
/// ls3df/I3D/I3D.h:289-291). The reference records no LS3DF FUNCTION address
/// for the mask itself; it documents what `Duplicate` copies. Keeps every bit
/// except world-matrix/bound-volume validity, list membership (target, shadow,
/// tick, fast-find tree, weather), interpolator, sector/occluder hierarchy
/// bits and UPDATE.
pub fn frame_duplicate_flags(flags: u32) -> u32 {
    flags & 0x0fa7_3e9f
}
/// Working name for the null-source limb of `I3D_frame::Duplicate` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:799-804 at pin 113b5b6). The reference
/// records the body at PC RVA 0x1c5b0, which is already occupied in
/// `docs/rewrite-overlay.json`, so no RVA is claimed here. A null source
/// returns I3DERR_INVALIDPARAMS before any store; the composed port lives on
/// `ModelArena::duplicate_frame`. Reports whether the source is rejected.
pub fn frame_duplicate_rejects_null_source(is_null: bool) -> bool {
    is_null
}
/// Working name for the merged flag word of `I3D_frame::Duplicate` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:807-809 at pin 113b5b6). Same
/// occupied-body status as `frame_duplicate_flags`. Composes the masked
/// source word (`frame_duplicate_flags`), the kept destination list bits
/// (`frame_duplicate_preserved_bits`) and `FRMFLAGS_UPDATE`; the
/// matrix/scale/rotation copies stay caller-owned.
pub fn frame_duplicate_merged_flags(src_flags: u32, dst_flags: u32) -> u32 {
    frame_duplicate_flags(src_flags) | frame_duplicate_preserved_bits(dst_flags) | 0x4000_0000
}
/// Working name for the fast-find re-registration gate of
/// `I3D_frame::Duplicate` (mafia-re ls3df/I3D/frames/I3d_frame.cpp:824-827
/// at pin 113b5b6). Same occupied-body status as `frame_duplicate_flags`.
/// A duplicated frame rejoins the driver fast-find tree only when the copy
/// has a name and `renderState[RS_FAST_FIND]` is set; the tree edits stay
/// caller-owned.
pub fn frame_duplicate_rejoins_fast_find(has_name: bool, fast_find_on: bool) -> bool {
    has_name && fast_find_on
}
/// Working name for the null-name arm of `I3D_frame::Duplicate` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:813-816 at pin 113b5b6). Same
/// occupied-body status as `frame_duplicate_flags`. A sourceless name clears
/// both the copied name and the rebased original-name pointer; the
/// allocation, copy and fast-find skip for the named arm stay caller-owned.
pub fn frame_duplicate_clears_name_words(is_null_name: bool) -> bool {
    is_null_name
}
/// Working name for the original-name rebase of `I3D_frame::Duplicate`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:827 at pin 113b5b6:
/// `m_pOrigName = frame->m_pOrigName - frame->m_pName + m_pName`). Same
/// occupied-body status as `frame_duplicate_flags`. The native interior
/// pointer difference has no host equivalent, so it is modeled as wrapping
/// byte offsets from each name base; allocation and registration stay
/// caller-owned.
pub fn duplicate_orig_name_offset(
    src_name_base: usize,
    src_orig: usize,
    dst_name_base: usize,
) -> usize {
    src_orig.wrapping_sub(src_name_base).wrapping_add(dst_name_base)
}
/// Working name for the `I3D_frame::GetData` word read (mafia-re
/// ls3df/I3D/I3D.h:595-600; `m_UserData` at +0x08, I3D.h:660, Insanity3D's
/// name). The reference records no LS3DF FUNCTION address for a standalone
/// reader: the inlined body just returns the member, and the single meshes'
/// proxies reach their visual through it. The arena models no user-data word
/// on `I3dFrameState`, so this stays a pure word projection with the native
/// offset documented; no owner or device is consulted.
pub fn frame_data_word(user_data: u32) -> u32 {
    user_data
}
/// Working name for the inlined `I3D_frame::GetName` empty fallback
/// (mafia-re ls3df/I3D/I3D.h:583-587 at pin 113b5b6). The reference records
/// no LS3DF FUNCTION address for a standalone reader; only the Game.exe
/// out-of-line copy at 0x47ad00 exists, unclaimed here. Mirrors
/// `m_pName ? m_pName : ""` over raw bytes; no owner or device is consulted.
pub fn frame_name_bytes_or_empty(name: Option<&[u8]>) -> &[u8] {
    name.unwrap_or(b"")
}
/// Working name for the inlined `I3D_frame::GetOrigName` empty fallback
/// (mafia-re ls3df/I3D/I3D.h:589-593 at pin 113b5b6). The reference records
/// no LS3DF FUNCTION address and no Game.exe out-of-line copy for a
/// standalone reader. Mirrors `m_pOrigName ? m_pOrigName : ""`; pure.
/// The arena `frame_orig_name_or_empty` covers stored names; this pins the
/// byte-level empty rule with no owner.
pub fn frame_orig_name_bytes_or_empty(orig: Option<&[u8]>) -> &[u8] {
    orig.unwrap_or(b"")
}
/// Working name for the inlined `I3D_frame::GetProperty` empty fallback
/// (mafia-re ls3df/I3D/I3D.h:625-630 at pin 113b5b6, game-only `#else`
/// branch). The reference records no LS3DF FUNCTION address for a standalone
/// reader. Mirrors `m_pProperty ? m_pProperty : ""`; pure.
/// The arena `frame_property_or_empty` covers stored properties; this pins
/// the byte-level empty rule with no owner.
pub fn frame_property_bytes_or_empty(property: Option<&[u8]>) -> &[u8] {
    property.unwrap_or(b"")
}
/// Working name for the `FRAME_USER` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_USER` is 8, stored by the ctor in
/// ls3df/I3D/frames/I3d_user.cpp:5-13). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader; the word is
/// tested in place by `Duplicate`. Reports whether a stored type word
/// denotes a user frame; no cache moves.
pub fn frame_type_word_is_user(type_word: u32) -> bool {
    type_word == 8
}
/// Working name for the `FRAME_VOLUME` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-74 at pin 113b5b6;
/// `FRAME_VOLUME` is 11, stored by the ctor in
/// ls3df/I3D/frames/I3d_volume.cpp:5-13). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a volume frame; no cache moves.
pub fn frame_type_word_is_volume(type_word: u32) -> bool {
    type_word == 11
}
/// Working name for the `FRAME_DUMMY` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-69 at pin 113b5b6;
/// `FRAME_DUMMY` is 6). The reference records no LS3DF FUNCTION address
/// for a standalone discriminant reader. Reports whether a stored type
/// word denotes a dummy frame; no cache moves.
pub fn frame_type_word_is_dummy(type_word: u32) -> bool {
    type_word == 6
}
/// Working name for the `FRAME_JOINT` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-73 at pin 113b5b6;
/// `FRAME_JOINT` is 10). The reference records no LS3DF FUNCTION address
/// for a standalone discriminant reader. Reports whether a stored type
/// word denotes a joint frame; no cache moves.
pub fn frame_type_word_is_joint(type_word: u32) -> bool {
    type_word == 10
}
/// Working name for the `FRAME_TARGET` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-70 at pin 113b5b6;
/// `FRAME_TARGET` is 7). The reference records no LS3DF FUNCTION address
/// for a standalone discriminant reader. This tests the stored type word,
/// unlike `frame_is_target_frame` which tests the `FRMFLAGS_TARGET` list
/// bit; no cache moves.
pub fn frame_type_word_is_target(type_word: u32) -> bool {
    type_word == 7
}
/// Working name for the `FRAME_MODEL` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_MODEL` is 9). The reference records no LS3DF FUNCTION address
/// for a standalone discriminant reader. Reports whether a stored type
/// word denotes a model frame; no cache moves.
pub fn frame_type_word_is_model(type_word: u32) -> bool {
    type_word == 9
}
/// Working name for the `FRAME_NULL` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_NULL` is 0, stored by the base ctor in
/// ls3df/I3D/frames/I3d_frame.cpp:28-63). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader; the word is
/// tested in place by `Duplicate` and the enum switches. Reports whether
/// a stored type word denotes a bare frame; no cache moves.
pub fn frame_type_word_is_null(type_word: u32) -> bool {
    type_word == 0
}
/// Working name for the `FRAME_VISUAL` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_VISUAL` is 1, stored by the ctor in
/// ls3df/I3D/visuals/I3d_visual.cpp:8-19). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a visual frame; no cache moves.
pub fn frame_type_word_is_visual(type_word: u32) -> bool {
    type_word == 1
}
/// Working name for the `FRAME_LIGHT` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_LIGHT` is 2, stored by the ctor in
/// ls3df/I3D/frames/I3d_light.cpp:17-51). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a light frame; no cache moves.
pub fn frame_type_word_is_light(type_word: u32) -> bool {
    type_word == 2
}
/// Working name for the `FRAME_CAMERA` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_CAMERA` is 3, stored by the ctor in
/// ls3df/I3D/frames/I3d_camera.cpp:7-37). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a camera frame; no cache moves.
pub fn frame_type_word_is_camera(type_word: u32) -> bool {
    type_word == 3
}
/// Working name for the `FRAME_SOUND` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_SOUND` is 4, stored by the ctor in
/// ls3df/I3D/frames/I3d_sound.cpp:32-71). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a sound frame; no cache moves.
pub fn frame_type_word_is_sound(type_word: u32) -> bool {
    type_word == 4
}
/// Working name for the `FRAME_SECTOR` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-72 at pin 113b5b6;
/// `FRAME_SECTOR` is 5, stored by the ctor in
/// ls3df/I3D/frames/I3d_sector.cpp:99-126). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a sector frame; no cache moves.
pub fn frame_type_word_is_sector(type_word: u32) -> bool {
    type_word == 5
}
/// Working name for the `FRAME_OCCLUDER` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-76 at pin 113b5b6;
/// `FRAME_OCCLUDER` is 12, stored by the ctor in
/// ls3df/I3D/frames/I3d_occluder.cpp:53-81). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes an occluder frame; no cache moves.
pub fn frame_type_word_is_occluder(type_word: u32) -> bool {
    type_word == 12
}
/// Working name for the `FRAME_SCENE` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-77 at pin 113b5b6;
/// `FRAME_SCENE` is 13, stored by the ctor in
/// ls3df/I3D/I3d_scene.cpp:90-148). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a scene frame; no cache moves.
pub fn frame_type_word_is_scene(type_word: u32) -> bool {
    type_word == 13
}
/// Working name for the `FRAME_AREA` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-78 at pin 113b5b6;
/// `FRAME_AREA` is 14, stored by the ctor in
/// ls3df/I3D/frames/I3d_area.cpp:17-35). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes an area frame; no cache moves.
pub fn frame_type_word_is_area(type_word: u32) -> bool {
    type_word == 14
}
/// Working name for the `FRAME_SHADOW` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-78 at pin 113b5b6;
/// `FRAME_SHADOW` is 15, stored by the ctor in
/// ls3df/I3D/visuals/I3d_shadow.cpp:40). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a shadow frame; no cache moves.
pub fn frame_type_word_is_shadow(type_word: u32) -> bool {
    type_word == 15
}
/// Working name for the `FRAME_LANDSCAPE` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-79 at pin 113b5b6;
/// `FRAME_LANDSCAPE` is 16, PC only, stored by the ctor in
/// ls3df/I3D/visuals/I3d_landscape.cpp:348). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes a landscape frame; no cache moves.
pub fn frame_type_word_is_landscape(type_word: u32) -> bool {
    type_word == 16
}
/// Working name for the `FRAME_EMITOR` type-word discriminant behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-80 at pin 113b5b6;
/// `FRAME_EMITOR` is 17, stored by the ctor in
/// ls3df/I3D/visuals/I3d_particle.cpp:331). The reference records no LS3DF
/// FUNCTION address for a standalone discriminant reader. Reports whether
/// a stored type word denotes an emitor frame; no cache moves.
pub fn frame_type_word_is_emitor(type_word: u32) -> bool {
    type_word == 17
}
/// Working name for the `FRAME_LAST` bound sentinel behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-81 at pin 113b5b6;
/// `FRAME_LAST` is 18, the number of frame types, never a stored type: the
/// 4ds loader refuses a type from here on). The reference records no LS3DF
/// FUNCTION address for a standalone reader. Reports whether a word equals
/// the exclusive upper bound; no cache moves.
pub fn frame_type_word_is_last(type_word: u32) -> bool {
    type_word == 18
}
/// Working name for the loader-admitted type range behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-81 at pin 113b5b6:
/// `FRAME_LAST` is the count and the 4ds loader refuses a type from there
/// on). The reference records no LS3DF FUNCTION address for a standalone
/// range reader. Reports whether a stored type word names a loadable frame
/// kind (`type < FRAME_LAST`); no cache moves.
pub fn frame_type_in_valid_range(type_word: u32) -> bool {
    type_word < 18
}
/// Working name for the loader-refused type range behind
/// `I3D_frame::GetType` (mafia-re ls3df/I3D/I3D.h:61-81 at pin 113b5b6,
/// same `FRAME_LAST` bound as `frame_type_in_valid_range`). The reference
/// records no LS3DF FUNCTION address for a standalone range reader.
/// Reports whether the 4ds loader must refuse the type word
/// (`type >= FRAME_LAST`); no cache moves.
pub fn frame_type_loader_refuses(type_word: u32) -> bool {
    type_word >= 18
}
/// Working name for the FRMFLAGS_FFTREE bit test (mafia-re ls3df/I3D/I3D.h:286).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit gates `RemoveFromFFTree` in the destructor (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:71). Reports whether the frame sits in the
/// driver's fast-find tree; no cache moves.
pub fn frame_in_fast_find_tree(flags: u32) -> bool {
    flags & 0x2000_0000 != 0
}
/// Working name for the FRMFLAGS_TARGET bit test (mafia-re ls3df/I3D/I3D.h:269).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit gates `DeleteFromAnyTarget` in the destructor (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:73). Reports whether the frame is a
/// target's frame; no cache moves.
pub fn frame_is_target_frame(flags: u32) -> bool {
    flags & 0x4000 != 0
}
/// Working name for the FRMFLAGS_SHADOW bit test (mafia-re ls3df/I3D/I3D.h:270).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit gates `DeleteFromAnyShadow` in the destructor (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:75). Reports whether the frame is a
/// shadow's frame; no cache moves.
pub fn frame_is_shadow_frame(flags: u32) -> bool {
    flags & 0x8000 != 0
}
/// Working name for the FRMFLAGS_WEATHER bit test (mafia-re ls3df/I3D/I3D.h:288).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is set by `WeatherSystemAddDummy` and cleared by
/// `WeatherSystemDelDummy` (mafia-re ls3df/I3D/I3d_scene.cpp:1058-1078).
/// Reports whether the dummy is registered in a scene weather system; no
/// cache moves.
pub fn frame_is_weather_dummy(flags: u32) -> bool {
    flags & 0x8000_0000 != 0
}
/// Working name for the FRMFLAGS_CHANGED bit test (mafia-re ls3df/I3D/I3D.h:268).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit joins `FRMFLAGS_UPDATE` in the `Update` gate (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:907). Reports whether the frame moved since
/// the last update; no cache moves.
pub fn frame_has_changed(flags: u32) -> bool {
    flags & 0x2000 != 0
}
/// Working name for the FRMFLAGS_HR_AREAS bit test (mafia-re ls3df/I3D/I3D.h:275).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is maintained by area LinkTo/AddToRemoveFromSceneRing (mafia-re
/// ls3df/I3D/frames/I3d_area.cpp:90-112). Reports whether the hierarchy owns
/// areas; no cache moves.
pub fn frame_hr_areas_set(flags: u32) -> bool {
    flags & 0x40000 != 0
}
/// Working name for the FRMFLAGS_HR_SECTORS bit test (mafia-re ls3df/I3D/I3D.h:277).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is folded into LinkTo hierarchy propagation (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:503-528). Reports whether the hierarchy owns
/// sectors; no cache moves.
pub fn frame_hr_sectors_set(flags: u32) -> bool {
    flags & 0x80000 != 0
}
/// Working name for the FRMFLAGS_HR_OCCLUDERS bit test (mafia-re ls3df/I3D/I3D.h:279).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is folded into the same LinkTo propagation. Reports whether the
/// hierarchy owns occluders; no cache moves.
pub fn frame_hr_occluders_set(flags: u32) -> bool {
    flags & 0x100000 != 0
}
/// Working name for the FRMFLAGS_CLIP bit test (mafia-re ls3df/I3D/I3D.h:281).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit marks frames that have to be clipped. Reports the clip request;
/// no cache moves.
pub fn frame_is_clipped(flags: u32) -> bool {
    flags & 0x200000 != 0
}
/// Working name for the FRMFLAGS_IN_VIEW bit test (mafia-re ls3df/I3D/I3D.h:271-274).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the portal walk sets it for frames inside the view depth range. Reports
/// the in-view mark; no cache moves.
pub fn frame_in_view(flags: u32) -> bool {
    flags & 0x10000 != 0
}
/// Working name for the FRMFLAGS_ROT_VALID bit test (mafia-re ls3df/I3D/I3D.h:260).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is only tested inside the inlined `UpdateMatrix`/`SetRot` bodies.
/// Reports whether `m_Rot` currently holds the local matrix rotation; no
/// cache moves.
pub fn frm_rot_valid(flags: u32) -> bool {
    flags & 0x04 != 0
}
/// Working name for the FRMFLAGS_QUAT_VALID bit test (mafia-re ls3df/I3D/I3D.h:261).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is only tested inside the inlined `UpdateMatrix` body. Reports
/// whether `m_Rot` holds the rotation the local matrix was made from; no
/// cache moves.
pub fn frm_quat_valid(flags: u32) -> bool {
    flags & 0x08 != 0
}
/// Working name for the FRMFLAGS_MAT_VALID bit test (mafia-re ls3df/I3D/I3D.h:262).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the inlined `GetLocalMatrix` (mafia-re ls3df/I3D/I3D.h:407-413) refreshes
/// the matrix through it. Reports whether the local matrix is up to date; no
/// cache moves.
pub fn frm_mat_valid(flags: u32) -> bool {
    flags & 0x10 != 0
}
/// Working name for the FRMFLAGS_WMAT_VALID bit test (mafia-re ls3df/I3D/I3D.h:263).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the inlined `UpdateWMatrix` (mafia-re ls3df/I3D/I3D.h:505-510) gates on it.
/// Reports whether the world matrix is up to date; no cache moves.
pub fn frm_wmat_valid(flags: u32) -> bool {
    flags & 0x20 != 0
}
/// Working name for the FRMFLAGS_HR_BOUND_VALID bit test (mafia-re ls3df/I3D/I3D.h:264).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is cleared by `DiscardHRBoundVol` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:929-943) and set by `UpdateHRBoundVol`.
/// Reports whether the local hierarchy bound volume is up to date; no cache
/// moves.
pub fn frm_hr_bound_valid(flags: u32) -> bool {
    flags & 0x40 != 0
}
/// Working name for the FRMFLAGS_HR_BOUND_W_VALID bit test (mafia-re
/// ls3df/I3D/I3D.h:266). The reference records no LS3DF FUNCTION address for
/// a standalone reader; the inlined `GetHRBoundVolumeW` (mafia-re
/// ls3df/I3D/I3D.h:536-541) refreshes through it. Reports whether the world
/// hierarchy bound volume is up to date; no cache moves.
pub fn frm_hr_bound_w_valid(flags: u32) -> bool {
    flags & 0x100 != 0
}
/// Working name for the FRMFLAGS_INTERPOLATOR bit test (mafia-re
/// ls3df/I3D/I3D.h:285). The reference records no LS3DF FUNCTION address for
/// a standalone reader; the bit is tested by the scene walker (mafia-re
/// ls3df/I3D/I3d_scene.cpp:681-684) and excluded from the Duplicate mask.
/// Reports whether the frame owns an interpolator; no cache moves.
pub fn frm_has_interpolator(flags: u32) -> bool {
    flags & 0x1000_0000 != 0
}
/// Working name for the FRMFLAGS_CALLBACK_MASK projection (mafia-re
/// ls3df/I3D/I3D.h:267). The reference records no LS3DF FUNCTION address for
/// the mask itself; `SetCallback` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:225-234) intersects the input with it.
/// Keeps only the callback message bits; no cache moves.
pub fn frm_callback_bits(flags: u32) -> u32 {
    flags & 0x1e00
}
/// Working name for the `LS3D_CALLBACK_NOTE` gate (mafia-re ls3df/I3D/I3D.h:299
/// at pin 113b5b6: an animation passed a note key). The reference records no
/// LS3DF FUNCTION address for a standalone reader; the bit travels inside the
/// `FRMFLAGS_CALLBACK_MASK` word and is consumed inline by the interpolator
/// tick, so no RVA is claimed here. Exact bit test on the caller-supplied
/// flags word; no owner or cache is consulted.
pub fn frame_callback_wants_note(flags: u32) -> bool {
    flags & 0x200 != 0
}
/// Working name for the `LS3D_CALLBACK_LOD` gate (mafia-re ls3df/I3D/I3D.h:301
/// at pin 113b5b6: a visual's level of detail changed). Same inline-only
/// status as `frame_callback_wants_note`: no standalone LS3DF FUNCTION entry
/// exists, so no RVA is claimed here. Exact bit test; no state moves.
pub fn frame_callback_wants_lod(flags: u32) -> bool {
    flags & 0x400 != 0
}
/// Working name for the `LS3D_CALLBACK_ANIM_END` gate (mafia-re
/// ls3df/I3D/I3D.h:302 at pin 113b5b6: an animation reached an end). Same
/// inline-only status as `frame_callback_wants_note`: no standalone LS3DF
/// FUNCTION entry exists, so no RVA is claimed here. Exact bit test; no
/// state moves.
pub fn frame_callback_wants_anim_end(flags: u32) -> bool {
    flags & 0x800 != 0
}
/// Working name for the `LS3D_CALLBACK_NOTE_STRING` gate (mafia-re
/// ls3df/I3D/I3D.h:303 at pin 113b5b6: an animation passed a string note key).
/// Same inline-only status as `frame_callback_wants_note`: no standalone LS3DF
/// FUNCTION entry exists, so no RVA is claimed here. Exact bit test; no state
/// moves.
pub fn frame_callback_wants_note_string(flags: u32) -> bool {
    flags & 0x1000 != 0
}
/// Working name for the generic frame-callback message gate over the four
/// documented `LS3D_CALLBACK_*` bits (mafia-re ls3df/I3D/I3D.h:299-303 at pin
/// 113b5b6, inside `FRMFLAGS_CALLBACK_MASK` 0x1e00). The reference records no
/// LS3DF FUNCTION address for a standalone dispatcher, so no RVA is claimed
/// here. Reports whether the caller-supplied message bit is set in the flags
/// word; undocumented bits never report true. Pure helper, no state moves.
pub fn frame_callback_enables(flags: u32, message_bit: u32) -> bool {
    const KNOWN: u32 = 0x200 | 0x400 | 0x800 | 0x1000;
    message_bit & KNOWN != 0 && flags & message_bit != 0
}
/// Native `I3DLINK_UPDMATRIX` (mafia-re ls3df/I3D/I3D.h:307 at pin 113b5b6;
/// Insanity3D's name, same value and role). Keep the world matrix across
/// `LinkTo`: the local matrix is made again against the new parent. The
/// reference records no LS3DF FUNCTION address for the flag itself (it is a
/// `#define`, not a function); the flag word is consumed by `LinkTo` (PC RVA
/// 0x1b9d0, already occupied, not claimed here).
pub const I3DLINK_UPDMATRIX: u32 = 0x1;
/// Native `I3DLINK_DISCARD_BOUND` (mafia-re ls3df/I3D/I3D.h:308-309 at pin
/// 113b5b6; the name is a guess). Discard the old parent's and the frame's
/// hierarchy bound volumes on `LinkTo` instead of updating at once. Same
/// working-name status as `I3DLINK_UPDMATRIX`; the flag-2 path stays an
/// explicit `UnsupportedLinkFlags` rejection in `link_to_projection`, so this
/// word only classifies caller input and moves no state.
pub const I3DLINK_DISCARD_BOUND: u32 = 0x2;
/// Native `I3DLINK_UPDATE` (mafia-re ls3df/I3D/I3D.h:310-311 at pin 113b5b6;
/// the name is a guess). Update the frame at once on `LinkTo` instead of
/// discarding the bound volumes. Same working-name status as
/// `I3DLINK_UPDMATRIX`; the flag-4 path stays an explicit
/// `UnsupportedLinkFlags` rejection in `link_to_projection`.
pub const I3DLINK_UPDATE: u32 = 0x4;
/// Working name for the `flags & I3DLINK_UPDMATRIX` gate of `LinkTo`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:468-469 at pin 113b5b6). The
/// reference records no LS3DF FUNCTION address for the gate itself; the
/// branch lives inside the occupied `LinkTo` body (PC RVA 0x1b9d0).
/// Reports whether the link keeps the world matrix; no cache moves.
pub fn link_keeps_world_matrix(flags: u32) -> bool {
    flags & I3DLINK_UPDMATRIX != 0
}
/// Working name for the `flags & I3DLINK_DISCARD_BOUND` gate of `LinkTo`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:474-476 at pin 113b5b6). Same
/// working-name status as `link_keeps_world_matrix`. Reports whether the
/// link discards (rather than updates) the bound volumes; no cache moves.
pub fn link_discards_bounds(flags: u32) -> bool {
    flags & I3DLINK_DISCARD_BOUND != 0
}
/// Working name for the `flags & I3DLINK_UPDATE` gate of `LinkTo` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:498-501 at pin 113b5b6). Same
/// working-name status as `link_keeps_world_matrix`. Reports whether the
/// link updates the frame at once; no cache moves.
pub fn link_updates_at_once(flags: u32) -> bool {
    flags & I3DLINK_UPDATE != 0
}
/// Working name for the same-parent early-out of `I3D_frame::LinkTo`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:465-466 at pin 113b5b6). The
/// reference records the body at PC RVA 0x1b9d0, which is already occupied
/// in `docs/rewrite-overlay.json`, so no RVA is claimed here. Linking to
/// the current parent returns I3D_OK with no store; the unlink/relink walk
/// stays caller-owned.
pub fn link_to_same_parent_noop(same_parent: bool) -> bool {
    same_parent
}
/// Working name for the `UPDATE|CHANGED` gate of `UpdateChildren`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:887 at pin 113b5b6).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the word is only tested inside the occupied `UpdateChildren` body
/// (PC RVA 0x1c9a0, already ported, not claimed here).
/// Reports whether the frame still needs an update traversal; no cache moves.
pub fn frame_needs_update(flags: u32) -> bool {
    flags & (0x4000_0000 | 0x2000) != 0
}
/// Working name for the `changed` word of `UpdateChildren` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:887 at pin 113b5b6). The reference records
/// no LS3DF FUNCTION address for a standalone reader; the word is only
/// tested inside the occupied `UpdateChildren` body (PC RVA 0x1c9a0, already
/// ported, not claimed here). Keeps the `UPDATE|CHANGED` bits; the counter
/// bump and descent stay caller-owned.
pub fn frame_changed_word(flags: u32) -> u32 {
    flags & (0x4000_0000 | 0x2000)
}
/// Working name for the `ON && (UPDATE|CHANGED)` gate of
/// `I3D_target::UpdateV` (mafia-re ls3df/I3D/frames/I3d_target.cpp:115-119
/// at pin 113b5b6). The reference records no LS3DF FUNCTION address for the
/// gate itself; the queuing body (`UpdateTargetList.push_back`, PC RVA
/// 0x59920) is already ported in `target_state`, not claimed here.
/// Reports whether an on-frame with pending motion is queued for the
/// post-update target pass; no cache moves.
pub fn frame_queues_target_update(flags: u32) -> bool {
    flags & 0x1 != 0 && frame_needs_update(flags)
}
/// Working name for the target-list drain gate of `I3D_frame::Update`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:916-926 at pin 113b5b6). The
/// reference records the body at PC RVA 0x1ca60, which is already occupied
/// in `docs/rewrite-overlay.json`, so no RVA is claimed here. The guarded
/// `UpdateTargetList` flush runs only with pending targets while no flush
/// is already in progress; the per-target `UpdateFrames` calls and the
/// reentrancy flag stay caller-owned.
pub fn update_drains_target_list(pending_targets: usize, updating: bool) -> bool {
    pending_targets != 0 && !updating
}
/// Working name for the flag word written by `I3D_frame::SetCallback`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:225-234 at pin 113b5b6).
/// The reference records the body at PC RVA 0x1b3c0, which is already
/// occupied in `docs/rewrite-overlay.json`, so no RVA is claimed here.
/// Only the callback-mask bits (0x1e00) of the input matter: a present
/// callback sets them, an absent one clears them. The callback pointer
/// store stays caller-owned; only the flag word is modeled.
pub fn frame_flags_after_set_callback(flags: u32, has_callback: bool, input: u32) -> u32 {
    let masked = input & 0x1e00;
    if has_callback {
        flags | masked
    } else {
        flags & !masked
    }
}
/// Working name for the flag word written by `I3D_frame::SetOn`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:238-244 at pin 113b5b6).
/// The reference records the body at PC RVA 0x1b400, which is already
/// occupied in `docs/rewrite-overlay.json`, so no RVA is claimed here.
/// Setting `on` stores bit0 (FRMFLAGS_ON); clearing it restores the word
/// without touching any other bit. The kind dispatch and owner updates
/// stay caller-owned; only the flag word is modeled.
pub fn frame_flags_after_set_on(flags: u32, on: bool) -> u32 {
    if on {
        flags | 1
    } else {
        flags & !1
    }
}
/// Working name for the last-reference limb of `I3D_frame::Release` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:98-104 at pin 113b5b6). The reference
/// records the body at PC RVA 0x1aae0, which is already occupied in
/// `docs/rewrite-overlay.json`, so no RVA is claimed here. The pre-decrement
/// count above zero is returned and the frame survives; only a pre-decrement
/// count of one deletes the frame and returns zero. The count store and the
/// destruction stay caller-owned.
pub fn frame_release_deletes_last(refcount: u32) -> bool {
    refcount == 1
}
/// Working name for the null/empty limbs of `I3D_frame::SetProperty`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:834-845 at pin 113b5b6). The
/// reference records the body at PC RVA 0x1c700, which is already occupied
/// in `docs/rewrite-overlay.json`, so no RVA is claimed here. A null
/// property and an empty string both store NULL and return I3D_OK; only a
/// non-empty string allocates and copies. The buffer edits stay
/// caller-owned.
pub fn frame_set_property_stores_null(prop_len: Option<usize>) -> bool {
    matches!(prop_len, None | Some(0))
}
/// Working name for the in-class empty base `I3D_frame::Tick`
/// (mafia-re ls3df/I3D/I3D.h:350 at pin 113b5b6: `Tick(int time) {}`).
/// The reference records no LS3DF FUNCTION address for it: every kind with
/// tick work overrides it (scene, model, joint, target), and the base body
/// is empty. The time argument is ignored and no state moves.
pub fn frame_tick_noop(_time: i32) {}
/// Working name for the in-class empty base `I3D_frame::SaveImage`
/// (mafia-re ls3df/I3D/I3D.h:367 at pin 113b5b6: `SaveImage() {}`).
/// The reference records no LS3DF FUNCTION address for it: only kinds with
/// image payloads override it (area, sector, light, sound, portal and the
/// visuals), and the base body is empty. No state moves.
pub fn frame_save_image_noop() {}
/// Working name for the `TICK|SHADOW|TARGET` preservation of
/// `I3D_frame::Duplicate` (mafia-re ls3df/I3D/frames/I3d_frame.cpp:807-809
/// at pin 113b5b6). The reference records the body at PC RVA 0x1c5b0,
/// already occupied, so no RVA is claimed here. Returns the old word's
/// registration bits that survive the duplicate-mask copy; the mask
/// application and the unconditional `UPDATE` set stay with the caller.
pub fn frame_duplicate_preserved_bits(old_flags: u32) -> u32 {
    old_flags & (0x40_0000 | 0x8000 | 0x4000)
}
/// Working name for the hierarchy-bits extraction of `I3D_frame::LinkTo`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:503 at pin 113b5b6).
/// The reference records the body at PC RVA 0x1b9d0, already occupied,
/// so no RVA is claimed here. Keeps only `HR_SECTORS|HR_OCCLUDERS|HR_AREAS`;
/// the upward propagation walk stays caller-owned.
pub fn frame_hr_hierarchy_bits(flags: u32) -> u32 {
    flags & (0x8_0000 | 0x10_0000 | 0x4_0000)
}
/// Working name for the flag word written by `AddChild` and `RemoveChild`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:561 and :604 at pin 113b5b6).
/// Both bodies live at already-occupied PC RVAs (0x1bea0/0x1bf20, ported as
/// graph methods), so no RVA is claimed here. Both clear the local and
/// world hierarchy bound-volume validity; list edits stay caller-owned.
pub fn frame_flags_after_child_list_change(flags: u32) -> u32 {
    flags & !(0x100 | 0x40)
}
/// Working name for the flag word written by `I3D_frame::LinkTo` without
/// `I3DLINK_UPDMATRIX` (mafia-re ls3df/I3D/frames/I3d_frame.cpp:496-497
/// at pin 113b5b6). The reference records the body at PC RVA 0x1b9d0,
/// already occupied, so no RVA is claimed here. Clears `WMAT_VALID` and
/// sets `UPDATE`; the hierarchy relink stays caller-owned.
pub fn frame_flags_after_link_no_keep(flags: u32) -> u32 {
    (flags & !0x20) | 0x4000_0000
}
/// Working name for the flag word written by `I3D_frame::SetBoundVolume`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:993-1003 at pin 113b5b6).
/// The reference records the body at PC RVA 0x1ce80, already occupied,
/// so no RVA is claimed here. Clears `HR_BOUND_W_VALID` and sets
/// `HR_BOUND_VALID`; the box/sphere stores stay caller-owned.
pub fn frame_flags_after_set_bound_volume(flags: u32) -> u32 {
    (flags & !0x100) | 0x40
}
/// Working name for the flag word written by `I3D_frame::Update`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:907-915 at pin 113b5b6).
/// The reference records the body at PC RVA 0x1ca60, already occupied,
/// so no RVA is claimed here. Clears `UPDATE|CHANGED`; the visual-cache
/// and target-list effects stay caller-owned.
pub fn frame_flags_after_update(flags: u32) -> u32 {
    flags & !(0x4000_0000 | 0x2000)
}
/// Working name for the child flag word written by `UpdateChildren`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:893-896 at pin 113b5b6).
/// The reference records the body at PC RVA 0x1c9a0, already occupied,
/// so no RVA is claimed here. A changed parent clears the child's world
/// and world-bound validity and marks it changed; list walks stay
/// caller-owned.
pub fn frame_child_flags_after_parent_change(flags: u32) -> u32 {
    (flags & !(0x100 | 0x20)) | 0x2000
}
/// Working name for the flag word written by `SetWorldDir` and the
/// matrix half of `SetWorldPosDir` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:183-184 and :217-218 at pin 113b5b6).
/// The `SetWorldPosDir` body lives at already-occupied PC RVA 0x1b230 and
/// `SetWorldDir` (PC RVA 0x1b040) already has a graph projection, so no RVA
/// is claimed here. Clears `HR_BOUND_W_VALID|WMAT_VALID|QUAT_VALID|`
/// `ROT_VALID` and sets `UPDATE|MAT_VALID`; matrix builds stay caller-owned.
pub fn frame_flags_after_set_world_dir(flags: u32) -> u32 {
    (flags & !(0x100 | 0x20 | 0x08 | 0x04)) | (0x4000_0000 | 0x10)
}
/// Working name for the roll gate shared by `I3D_frame::SetWorldDir` and the
/// matrix half of `SetWorldPosDir` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:178 and :213 at pin 113b5b6). The
/// `SetWorldPosDir` body lives at already-occupied PC RVA 0x1b230 and
/// `SetWorldDir` (PC RVA 0x1b040) already has a graph projection, so no RVA
/// is claimed here. The roll matrix composes only when `fabs(roll)` exceeds
/// `MRG_ZERO` (1e-8 per ls3df/ls3df.h:48); an unordered (NaN) roll keeps the
/// unrolled branch, matching the native comparison. The matrix builds stay
/// caller-owned.
pub fn world_dir_applies_roll(roll: f32) -> bool {
    roll.abs() > 1e-8
}
/// Working name for the sector-ancestor early-out of
/// `I3D_scene::SetFrameSectorPos` (mafia-re ls3df/I3D/I3d_scene.cpp:517-533
/// at pin 113b5b6). The reference records the body at PC RVA 0x4a280, which
/// is already occupied in `docs/rewrite-overlay.json`, so no RVA is claimed
/// here. A frame already under a backdrop or omni sector keeps its link and
/// returns I3D_OK; the ancestor walk and the sector lookup stay
/// caller-owned.
pub fn set_frame_sector_pos_ancestor_keeps(
    ancestor_is_sector: bool,
    backdrop: bool,
    omni: bool,
) -> bool {
    ancestor_is_sector && (backdrop || omni)
}
/// Working name for the same-sector no-op of `I3D_scene::SetFrameSectorPos`
/// (mafia-re ls3df/I3D/I3d_scene.cpp:529-532 at pin 113b5b6). Same
/// occupied-body status as `set_frame_sector_pos_ancestor_keeps`. The frame
/// already in its world-position sector keeps its link and returns I3D_OK;
/// the lookup and the `LinkTo` call stay caller-owned.
pub fn set_frame_sector_pos_skips_same_sector(same_sector: bool) -> bool {
    same_sector
}
/// Working name for the `VISF_WORLD_SPACE` bit test (mafia-re
/// ls3df/I3D/I3D.h visual `vis_flags` 0x40; tested in
/// ls3df/I3D/visuals/I3d_visual.cpp:32 and :40 at pin 113b5b6).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit only gates `UpdateV`/`SetSpace` (PC RVAs 0x5a390/0x5a3c0,
/// already ported or occupied, not claimed here). Reports whether the
/// visual lives in world space; no cache moves.
pub fn frame_visual_in_world_space(visual_flags: u32) -> bool {
    visual_flags & 0x40 != 0
}
/// Working name for the parent flag word written by `UpdateChildren`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:898-901 at pin 113b5b6).
/// The reference records the body at PC RVA 0x1c9a0, already occupied,
/// so no RVA is claimed here. When a child moved, the parent drops both
/// hierarchy bound-volume validity bits and gains `CHANGED`; the descent
/// itself stays caller-owned.
pub fn frame_flags_after_update_children_parent(flags: u32) -> u32 {
    (flags & !(0x100 | 0x40)) | 0x2000
}
/// Working name for the flag word written by the keep-world half of
/// `I3D_frame::LinkTo` (mafia-re ls3df/I3D/frames/I3d_frame.cpp:491-492
/// at pin 113b5b6). The reference records the body at PC RVA 0x1b9d0,
/// already occupied, so no RVA is claimed here. Clears
/// `HR_BOUND_W_VALID|WMAT_VALID|MAT_VALID|ROT_VALID` and sets
/// `UPDATE|QUAT_VALID`; the matrix rebuild stays caller-owned. The second
/// store (SetDir3 then `UPDATE|MAT_VALID`) matches
/// `frame_flags_after_set_world_dir` exactly and is not repeated here.
pub fn frame_flags_after_link_keep_world_first(flags: u32) -> u32 {
    (flags & !(0x100 | 0x20 | 0x10 | 0x04)) | (0x4000_0000 | 0x08)
}
/// Working name for the old-chain hierarchy-bit clearing of
/// `I3D_frame::LinkTo` (mafia-re ls3df/I3D/frames/I3d_frame.cpp:505-528
/// at pin 113b5b6). Same occupied-body status as
/// `frame_hr_hierarchy_bits`: the upward walk stays caller-owned, only
/// the kept `HR_SECTORS|HR_OCCLUDERS|HR_AREAS` projection is modeled. A
/// sector ancestor keeps its sector bit (areas and occluders cleared), an
/// occluder keeps its occluder bit, an area keeps its area bit; any other
/// frame keeps none of the three.
pub fn link_old_parent_kept_hr_bits(is_sector: bool, is_occluder: bool, is_area: bool) -> u32 {
    if is_sector {
        0x8_0000
    } else if is_occluder {
        0x10_0000
    } else if is_area {
        0x4_0000
    } else {
        0
    }
}
/// Working name for the `Update` world-bound clear gate (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:907-915 at pin 113b5b6). Same
/// occupied-body status as `frame_flags_after_update`: the visual-cache
/// store stays caller-owned, only the gate is modeled. A visual frame
/// with pending motion (`UPDATE|CHANGED`) whose visual is not in world
/// space drops `VISF_WBOUND_VALID` (mafia-re ls3df/I3D/I3D.h:2704);
/// non-visuals never consult a `vis_flags` word.
pub fn frame_update_clears_visual_bound(
    is_visual: bool,
    vis_in_world_space: bool,
    flags: u32,
) -> bool {
    is_visual && frame_needs_update(flags) && !vis_in_world_space
}
/// Working name for the `UpdateChildren` world-bound clear gate (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:887-897 at pin 113b5b6). Same
/// occupied-body status as `frame_flags_after_update_children_parent`
/// (`UpdateChildren` is PC RVA 0x1c9a0): the `m_UpdateCount` bump and
/// the visual-cache store stay caller-owned. A moved parent clears the
/// same `VISF_WBOUND_VALID` bit on a local-space visual child.
pub fn frame_update_children_clears_visual_bound(
    is_visual: bool,
    vis_in_world_space: bool,
    parent_flags: u32,
) -> bool {
    frame_needs_update(parent_flags) && is_visual && !vis_in_world_space
}
/// Working name for the `I3D_frame::GetRot` remake gate (mafia-re
/// ls3df/I3D/I3D.h:5002-5009 at pin 113b5b6). The reference records no
/// LS3DF FUNCTION address for it: the body is header-inline, and the only
/// out-of-line copy is Game.exe 0x47a810, which is not claimed here.
/// Reports whether the rotation must be remade from the local matrix
/// (`QUAT_VALID` clear); the `Make` call stays caller-owned.
pub fn frame_rot_needs_make(flags: u32) -> bool {
    flags & 0x08 == 0
}
/// Working name for the `I3D_frame::GetDir` rebuild branch (mafia-re
/// ls3df/I3D/I3D.h:558-572 at pin 113b5b6). Same inline-only status as
/// `frame_rot_needs_make`: no LS3DF FUNCTION entry exists, and the only
/// out-of-line copy is Game.exe 0x47a890, which is not claimed here.
/// Reports whether the matrix must be rebuilt from the normalized
/// rotation first (`MAT_VALID` clear, returning the rebuilt row
/// verbatim); otherwise the caller normalizes a copy of the stored row.
/// Both normalizations stay caller-owned.
pub fn frame_dir_rebuilds_matrix(flags: u32) -> bool {
    flags & 0x10 == 0
}
/// Working name for the sector arm of `I3D_frame::UpdateHRBoundVol`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:954-957 at pin 113b5b6).
/// Same occupied-body status as `frame_flags_after_set_bound_volume`
/// (`UpdateHRBoundVol` is PC RVA 0x1cc50): the hierarchy walk stays
/// caller-owned. A sector only sets `HR_BOUND_VALID` and returns; the
/// bound-volume copy stays with the caller.
pub fn frame_flags_after_update_hr_bound_vol_sector(flags: u32) -> u32 {
    flags | 0x40
}
/// Working name for the `UpdateHRBoundVolW` local-refresh gate (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:1010-1011 at pin 113b5b6). Same
/// occupied-body status as `frm_hr_bound_w_valid`
/// (`UpdateHRBoundVolW` is PC RVA 0x1cf90): the `UpdateHRBoundVol` call
/// and the world-space copy stay caller-owned. Reports whether the local
/// hierarchy bound must be refreshed first (`HR_BOUND_VALID` clear).
pub fn frame_hr_bound_w_needs_local_refresh(flags: u32) -> bool {
    flags & 0x40 == 0
}
/// Working name for the sector early-out of
/// `I3D_frame::DiscardHRBoundVol` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:932-934 at pin 113b5b6). Same
/// occupied-body status as `discard_hr_bound_vol`: the ancestor walk
/// stays caller-owned. Sectors keep both hierarchy bound-volume
/// validity bits; every other frame clears them upward.
pub fn frame_discard_bound_skips_sector(is_sector: bool) -> bool {
    is_sector
}
/// Working name for the in-class empty base `I3D_frame::LoadImage`
/// (mafia-re ls3df/I3D/I3D.h:368 at pin 113b5b6: `LoadImage() {}`).
/// The reference records no LS3DF FUNCTION address for it: only kinds with
/// image payloads override it (area, sector, light, sound, portal and the
/// visuals), and the base body is empty like `Tick` and `SaveImage`.
/// No state moves.
pub fn frame_load_image_noop() {}
/// Working name for the in-class empty base `I3D_frame::UpdateV`
/// (mafia-re ls3df/I3D/I3D.h:366 at pin 113b5b6: `UpdateV() {}`).
/// The reference records no LS3DF FUNCTION address for it: the out-of-line
/// empty bodies are shared with the renderer base
/// (`CBaseVideoRenderer::OnWaitEnd`, 0x10088540), not LS3DF frame code,
/// so no RVA is claimed here. No state moves.
pub fn frame_update_v_noop() {}
/// Working name for the NUL-terminated name-length read shared by `GetName`,
/// `GetOrigName` and `GetProperty` (mafia-re ls3df/I3D/I3D.h:583-587,589-593,
/// 625-630 at pin 113b5b6: `m_pName ? m_pName : ""` and siblings). The
/// reference records no LS3DF FUNCTION address for a standalone reader (the
/// only out-of-line copy is Game.exe 0x47ad00 for `GetName`, which is not
/// claimed here; `GetOrigName` and `GetProperty` have none). Returns the byte
/// length before the first NUL; a slice without NUL reads whole. Pure helper,
/// no owner consulted.
pub fn frame_name_len_until_nul(name: &[u8]) -> usize {
    name.iter().position(|&b| b == 0).unwrap_or(name.len())
}
/// Working name for the allocation-size limb of `I3D_frame::SetProperty`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:834-845 at pin 113b5b6). The
/// reference records the body at PC RVA 0x1c700, which is already occupied in
/// `docs/rewrite-overlay.json`, so no RVA is claimed here. A non-empty
/// property allocates `len + 1` bytes; checked addition reports overflow as
/// `None` (the native calls the fatal ErrorHandler there, disclosed). Pure
/// helper, no buffer moves.
pub fn frame_property_alloc_len(prop_len: usize) -> Option<usize> {
    prop_len.checked_add(1)
}
/// Working name for the supported-flags mask of `I3D_frame::LinkTo`
/// (mafia-re ls3df/I3D/I3D.h:307-311 at pin 113b5b6: `I3DLINK_UPDMATRIX`,
/// `I3DLINK_DISCARD_BOUND`, `I3DLINK_UPDATE`). The reference records no LS3DF
/// FUNCTION address for the mask itself (defines, not functions); the flags
/// word is consumed by `LinkTo` (PC RVA 0x1b9d0, already occupied, not claimed
/// here). Reports the union of the three admitted bits. Pure helper.
pub fn link_supported_flags_mask() -> u32 {
    0x7
}
/// Working name for the content-equal early-out of `I3D_frame::SetName`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:350-377 at pin 113b5b6). The
/// reference records the body at PC RVA 0x1b6f0, which is already occupied in
/// `docs/rewrite-overlay.json`, so no RVA is claimed here. Compares C-string
/// content truncated at the first NUL; `None` equals only `None`. Pure helper,
/// no allocation or registration moves.
pub fn frame_set_name_is_content_equal(
    stored: Option<&[u8]>,
    incoming: Option<&[u8]>,
) -> bool {
    match (stored, incoming) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let a = &a[..frame_name_len_until_nul(a)];
            let b = &b[..frame_name_len_until_nul(b)];
            a == b
        }
        _ => false,
    }
}
/// Working name for the index gate of `I3D_frame::GetChild`
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:610-620 at pin 113b5b6:
/// `if (index < m_numChildren)`). The reference records the body at PC RVA
/// 0x1bfd0, which is already occupied in `docs/rewrite-overlay.json`, so no
/// RVA is claimed here. Reports whether a signed index selects an existing
/// child; negative indices never select. The native member is a signed int,
/// so the comparison stays signed (`u32::MAX` reads back as -1 and admits
/// nothing). Pure helper, no hierarchy walk.
pub fn frame_get_child_index_in_range(index: i32, child_count: u32) -> bool {
    index >= 0 && index < child_count as i32
}
impl I3dFrameGraph {
    pub fn insert(&mut self, state: I3dFrameState) -> Result<FrameKey, FrameError> {
        let id = u32::try_from(self.nodes.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(FrameError::IdentityOverflow)?;
        let required = self.nodes.len() + 1;
        self.nodes
            .try_reserve(1)
            .map_err(|_| FrameError::AllocationOverflow)?;
        self.world_path
            .try_reserve(required.saturating_sub(self.world_path.len()))
            .map_err(|_| FrameError::AllocationOverflow)?;
        self.visits
            .try_reserve(required.saturating_sub(self.visits.len()))
            .map_err(|_| FrameError::AllocationOverflow)?;
        self.nodes.push(Node {
            alive: true,
            state,
            links: FrameLinks::default(),
            joint: (state.kind == FrameKind::Joint).then(I3dJointState::default),
            light: None,
            singlemesh: None,
            binding: None,
            local_error: None,
            world_error: None,
            joint_error: None,
            singlemesh_bound_error: None,
            dummy_inv: None,
            dummy_inv_error: None,
        });
        // Retain bounded traversal storage so the scene charges all allocations
        // before ticking or lazily rendering; traversal cannot grow past nodes.
        Ok(FrameKey(id))
    }
    fn index(&self, key: FrameKey) -> Result<usize, FrameError> {
        let i = key.0.checked_sub(1).ok_or(FrameError::UnknownFrame(key))? as usize;
        if self.nodes.get(i).is_none_or(|node| !node.alive) {
            return Err(FrameError::UnknownFrame(key));
        }
        Ok(i)
    }
    fn node(&self, key: FrameKey) -> Result<&Node, FrameError> {
        Ok(&self.nodes[self.index(key)?])
    }
    fn node_mut(&mut self, key: FrameKey) -> Result<&mut Node, FrameError> {
        let i = self.index(key)?;
        Ok(&mut self.nodes[i])
    }
    /// Preflight the represented model/dummy/joint/Target destructor paths.
    /// Visual resources and external registrations
    /// require their concrete backends. Rejection leaves the graph unchanged.
    #[cfg(test)]
    pub(crate) fn check_destruction(&self, key: FrameKey) -> Result<(), FrameError> {
        self.check_destruction_excluding(key, 0, false, false)
    }
    pub(crate) fn check_destruction_with_owned_name(
        &self,
        key: FrameKey,
    ) -> Result<(), FrameError> {
        self.check_destruction_excluding(key, 0x2000_0000, false, false)
    }
    /// Plain ObjectResources must be owned and released by the arena before
    /// invoking visual5a370/base1a9a0. Unbound or derived visuals stay rejected.
    pub(crate) fn check_destruction_with_owned_visual(
        &self,
        key: FrameKey,
    ) -> Result<(), FrameError> {
        self.check_destruction_excluding(key, 0x2000_0000, true, false)
    }
    /// Caller has executed the represented empty Scene/Sector derived tail.
    pub(crate) fn check_empty_scene_base_destruction(
        &self,
        key: FrameKey,
    ) -> Result<(), FrameError> {
        if !matches!(self.state(key)?.kind, FrameKind::Scene | FrameKind::Sector) {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        self.check_destruction_excluding(key, 0x2000_0000, false, true)
    }
    fn check_destruction_excluding(
        &self,
        key: FrameKey,
        handled: u32,
        plain_visual: bool,
        empty_scene: bool,
    ) -> Result<(), FrameError> {
        let node = self.node(key)?;
        if !matches!(
            node.state.kind,
            FrameKind::Model
                | FrameKind::Dummy
                | FrameKind::Area
                | FrameKind::Joint
                | FrameKind::Target
                | FrameKind::Light
                | FrameKind::Sound
                | FrameKind::Camera
                | FrameKind::Occluder
        ) && !(empty_scene && matches!(node.state.kind, FrameKind::Scene | FrameKind::Sector))
            && !(plain_visual
                && matches!(
                    node.state.kind,
                    FrameKind::Mesh
                        | FrameKind::Projector
                        | FrameKind::Lensflare
                        | FrameKind::Singlemesh
                ))
        {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        if node.state.kind == FrameKind::Light && node.light.is_none() {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        let registrations = (node.state.flags & !handled)
            & (0x20000000
                | 0x8000
                | 0x400000
                | if node.state.kind == FrameKind::Dummy {
                    0x80000000
                } else {
                    0
                });
        if registrations != 0 {
            return Err(FrameError::UnsupportedDestructionRegistrations(
                key,
                registrations,
            ));
        }
        if node.state.kind == FrameKind::Target {
            if self.target_updates.is_flushing() || self.target_updates.pending().contains(&key) {
                return Err(FrameError::UnsupportedDestruction(key));
            }
            for &linked in self.target_state(key)?.frames() {
                self.index(linked)?;
            }
        }
        // Native removes only the first owner/occurrence. Never turn remaining
        // raw dangling pointers into apparently usable stable graph identities.
        let mut incoming = 0usize;
        for (index, target) in self.nodes.iter().enumerate() {
            if target.alive && target.state.kind == FrameKind::Target {
                let target_key = FrameKey((index + 1) as u32);
                if target_key != key {
                    if let Ok(state) = self.targets.state(target_key) {
                        incoming += state.frames().iter().filter(|&&frame| frame == key).count();
                    }
                }
            }
        }
        let own_target_clears_bit =
            node.state.kind == FrameKind::Target && self.target_state(key)?.frames().contains(&key);
        if incoming > 1
            || (incoming != 0 && (node.state.flags & 0x4000 == 0 || own_target_clears_bit))
        {
            return Err(FrameError::UnsupportedDestruction(key));
        }
        if node.state.kind == FrameKind::Joint {
            let joint = node.joint.as_ref().unwrap();
            for (index, target) in self.nodes.iter().enumerate() {
                if let Some(binding) = &target.binding {
                    for (slot, &linked) in binding.joints.iter().enumerate() {
                        if linked == Some(key)
                            && (joint.singlemesh != Some(FrameKey((index + 1) as u32))
                                || joint.index as usize != slot)
                        {
                            return Err(FrameError::UnsupportedDestruction(key));
                        }
                    }
                }
            }
        }
        if let Some(joint) = node.joint {
            if let Some(target) = joint.singlemesh {
                let binding = self.singlemesh_binding(target)?;
                let slot = binding
                    .joints
                    .get(joint.index as usize)
                    .ok_or(FrameError::SinglemeshBindingUnavailable(target))?;
                // None is an unavailable allocation slot in this projection;
                // it cannot be mistaken for an observed native null pointer.
                let linked = slot.ok_or(FrameError::SinglemeshBindingUnavailable(target))?;
                self.joint_state(linked)?;
            }
        }
        Ok(())
    }
    /// Derived joint unlink precedes common parent/child detachment. Hierarchy
    /// links carry no references; detaching children never releases them and
    /// RemoveChild deliberately preserves their matrix caches.
    pub(crate) fn destroy_derived(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind == FrameKind::Singlemesh && self.node(key)?.binding.is_some() {
            // I3D_singlemesh::~I3D_singlemesh clears each joint's backlink
            // before freeing its own joint array and per-LOD skin definitions.
            // A failed LoadImage can leave its constructor null joint array.
            let joints = self.singlemesh_binding(key)?.joints().to_vec();
            for joint in joints.into_iter().flatten() {
                if self.joint_state(joint)?.singlemesh == Some(key) {
                    self.node_mut(joint)?.joint.as_mut().unwrap().singlemesh = None;
                }
            }
            // The owned slot array is freed with the singlemesh. Clear the
            // host projection too, so surviving joints have no stale incoming
            // link after their backlink has been removed.
            self.node_mut(key)?
                .binding
                .as_mut()
                .unwrap()
                .joints
                .fill(None);
        }
        if let Some(joint) = self.node(key)?.joint {
            if let Some(target) = joint.singlemesh {
                let linked = self.singlemesh_binding(target)?.joints[joint.index as usize].unwrap();
                self.node_mut(linked)?.joint.as_mut().unwrap().singlemesh = None;
                self.node_mut(target)?.binding.as_mut().unwrap().joints[joint.index as usize] =
                    None;
            }
        }
        if self.state(key)?.kind == FrameKind::Target {
            let mut targets = std::mem::take(&mut self.targets);
            let result = targets.destroy_target(key, |linked| {
                self.node_mut(linked)
                    .expect("preflighted target link")
                    .state
                    .flags &= !0x4000;
            });
            self.targets = targets;
            result.map_err(|_| FrameError::TargetUnavailable(key))?;
        }
        Ok(())
    }
    /// The arena removes owner membership between derived and base effects.
    pub(crate) fn destroy_base(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.flags & 0x4000 != 0 {
            let mut flags = self.state(key)?.flags;
            self.targets.remove_from_any_target(key, &mut flags);
            self.node_mut(key)?.state.flags = flags;
        }
        if let Some(parent) = self.links(key)?.parent {
            self.remove_child(parent, key)?;
        }
        while let Some(child) = self.links(key)?.child {
            self.remove_child(key, child)?;
        }
        let node = self.node_mut(key)?;
        node.alive = false;
        node.joint = None;
        Ok(())
    }
    #[cfg(test)]
    fn destroy(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.check_destruction(key)?;
        self.destroy_derived(key)?;
        self.destroy_base(key)
    }
    /// Original I3D_frame::~I3D_frame, PC RVA 0x1a9a0 (already ledger-credited,
    /// frozen port `LS3DF bounded base, dummy and unbound Joint destruction`:
    /// NO overlay entry here; probe-verified formed Ghidra body, confirm-only;
    /// mafia-re ls3df/I3D/frames/I3d_frame.cpp:67). Represented base tail: preflight
    /// the model/dummy/joint/Target subset, run derived unlink, then detach
    /// parent/children and retire the node. Fast-tree removal, owner
    /// membership, Target/Shadow/Tick deregistration, property/name frees
    /// and lastDestroyedFrame remain arena-owned boundaries and stay
    /// rejected here via the registration preflight.
    pub fn destroy_frame(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.check_destruction_excluding(key, 0, false, false)?;
        self.destroy_derived(key)?;
        self.destroy_base(key)
    }
    pub(crate) fn camera_projection_changed(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind != FrameKind::Camera {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        let state = &mut self.node_mut(key)?.state;
        state.update_count = state.update_count.wrapping_add(1);
        Ok(())
    }
    pub(crate) fn change_flags(
        &mut self,
        key: FrameKey,
        clear: u32,
        set: u32,
    ) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.flags = (node.state.flags & !clear) | set;
        Ok(())
    }
    /// Dummy-flag word stores for the Normalize limbs (PC RVA 0x1a350).
    /// Mirrors change_flags on the separate dummy_flags word.
    pub fn change_dummy_flags(
        &mut self,
        key: FrameKey,
        clear: u32,
        set: u32,
    ) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.dummy_flags = (node.state.dummy_flags & !clear) | set;
        Ok(())
    }
    pub fn state(&self, key: FrameKey) -> Result<&I3dFrameState, FrameError> {
        Ok(&self.node(key)?.state)
    }
    /// Direct native field stores used by reconstructed callers. Unlike SetRot,
    /// SetScale or LinkTo, a store does not invoke additional invalidation. The
    /// caller must restrict writes to its evidenced offsets and preserve their
    /// order; hierarchy, derived bindings and lifetime remain graph-owned.
    pub(crate) fn begin_light_constructor(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind != FrameKind::Light || self.node(key)?.light.is_some() {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        self.node_mut(key)?.light = Some(crate::frame_light::I3dLightState::constructor_prefix());
        Ok(())
    }
    pub(crate) fn light_state(
        &self,
        key: FrameKey,
    ) -> Result<&crate::frame_light::I3dLightState, FrameError> {
        self.node(key)?
            .light
            .as_ref()
            .ok_or(FrameError::UnsupportedFrameUpdate(key))
    }
    pub(crate) fn light_state_mut(
        &mut self,
        key: FrameKey,
    ) -> Result<&mut crate::frame_light::I3dLightState, FrameError> {
        self.node_mut(key)?
            .light
            .as_mut()
            .ok_or(FrameError::UnsupportedFrameUpdate(key))
    }
    pub(crate) fn projected_state_mut(
        &mut self,
        key: FrameKey,
    ) -> Result<&mut I3dFrameState, FrameError> {
        Ok(&mut self.node_mut(key)?.state)
    }
    /// Common SetOn store, PC RVA 0x1b400 (mafia-re
    /// ls3df/I3D/frames/I3d_frame.cpp:238): only the low argument byte
    /// controls flag bit0. Matrix/volume cache flags and descendants are
    /// untouched.
    pub(crate) fn set_on(&mut self, key: FrameKey, on: u32) -> Result<(), FrameError> {
        let flags = &mut self.node_mut(key)?.state.flags;
        *flags = (*flags & !1) | u32::from(on as u8 != 0);
        Ok(())
    }
    /// Working name for the LS3DF-inlined `I3D_frame::IsOn` read
    /// (mafia-re ls3df/I3D/I3D.h:543-549). No LS3DF FUNCTION address exists
    /// for it: the only out-of-line copy is Game.exe 0x47acf0, which is not
    /// claimed here. Reports flag bit0 (FRMFLAGS_ON); cache flags and
    /// descendants are untouched.
    pub fn frame_is_on(&self, key: FrameKey) -> Result<bool, FrameError> {
        Ok(self.state(key)?.flags & 1 != 0)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetParent` read
    /// (mafia-re ls3df/I3D/I3D.h:5042-5046). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47ad10, which
    /// is not claimed here. Borrows the live hierarchy link; no cycle or
    /// ownership claim beyond that link.
    pub fn frame_parent_key(&self, key: FrameKey) -> Result<Option<FrameKey>, FrameError> {
        Ok(self.links(key)?.parent)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::NumChildren` read
    /// (mafia-re ls3df/I3D/I3D.h:602-611, LS3DF_EXPORTS by-value branch). No
    /// LS3DF FUNCTION address exists for it: the only out-of-line copy is
    /// Game.exe 0x47ad20, which is not claimed here. The native width is
    /// int; the host count stays within the admitted hierarchy domain.
    pub fn frame_child_count(&self, key: FrameKey) -> Result<u32, FrameError> {
        Ok(self.links(key)?.child_count)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetType` read
    /// (mafia-re ls3df/I3D/I3D.h:602-616, LS3DF_EXPORTS by-value branch).
    /// No LS3DF FUNCTION address exists for it: the only out-of-line copy
    /// is Game.exe 0x47a6c0, which is not claimed here. Reports the live
    /// frame kind; no caches move.
    pub fn frame_kind(&self, key: FrameKey) -> Result<FrameKind, FrameError> {
        Ok(self.state(key)?.kind)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetScale` read
    /// (mafia-re ls3df/I3D/I3D.h:477-481). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47aec0,
    /// which is not claimed here. Reports the live local scale; the
    /// matrix cache is untouched.
    pub fn frame_scale(&self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        Ok(self.state(key)?.scale)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetPos` read
    /// (mafia-re ls3df/I3D/I3D.h:460-465). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47a720,
    /// which is not claimed here. Reports the local matrix translation
    /// row without touching any cache.
    pub fn frame_local_position(&self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        let local = self.state(key)?.local;
        Ok(local.elements[12..15].try_into().unwrap())
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetWorldPos` read
    /// (mafia-re ls3df/I3D/I3D.h:551-556). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47a730,
    /// which is not claimed here. Refreshes the world cache first, then
    /// reports its translation row.
    pub fn frame_world_position(&mut self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        let world = self.world_matrix(key)?;
        Ok(world.elements[12..15].try_into().unwrap())
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetWorldScale` read
    /// (mafia-re ls3df/I3D/I3D.h:519-528). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47a7b0,
    /// which is not claimed here. Parentless frames report the local
    /// scale; otherwise the refreshed world matrix basis-row lengths.
    /// A non-finite world row is a host unavailability, never an invented
    /// scale; the failure poisons the world cache like other world reads.
    pub fn frame_world_scale(&mut self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        if self.links(key)?.parent.is_none() {
            return Ok(self.state(key)?.scale);
        }
        let world = self.world_matrix(key)?;
        world.try_get_scale().map_err(|_| {
            if let Ok(node) = self.node_mut(key) {
                node.world_error = Some(FrameError::NumericDomain("GetWorldScale GetScale"));
            }
            FrameError::NumericDomain("GetWorldScale GetScale")
        })
    }
    /// Working name for the full `m_FrmFlags` word read at +0xac
    /// (mafia-re ls3df/I3D/I3D.h:666). No LS3DF FUNCTION address exists
    /// for a whole-word getter: the reference only inlines single-bit
    /// tests (for example `IsOn`, Game.exe 0x47acf0, unclaimed here).
    /// Reports the live word; no cache moves.
    pub fn frame_flags_word(&self, key: FrameKey) -> Result<u32, FrameError> {
        Ok(self.state(key)?.flags)
    }
    /// Working name for the `m_UpdateCount` read at +0x118
    /// (mafia-re ls3df/I3D/I3D.h:675). No LS3DF FUNCTION address exists
    /// for it: the counter is only stored by the update traversal.
    /// Reports the live count; no cache moves.
    pub fn frame_update_count(&self, key: FrameKey) -> Result<u32, FrameError> {
        Ok(self.state(key)?.update_count)
    }
    /// Working name for the `m_RenderNumber` read at +0x13c
    /// (mafia-re ls3df/I3D/I3D.h:683, ctor value -1 as u32::MAX).
    /// No LS3DF FUNCTION address exists for it: the driver producer
    /// that writes later renders is unimplemented. Reports the live
    /// word; no cache moves.
    pub fn frame_render_number(&self, key: FrameKey) -> Result<u32, FrameError> {
        Ok(self.state(key)?.render_number)
    }
    /// Working name for the full visual-flags word read
    /// (mafia-re ls3df/I3D/I3D.h visual `vis_flags`). No LS3DF FUNCTION
    /// address exists for a whole-word getter: the reference only
    /// inlines masked mode readers (for example shadow/projector modes).
    /// Reports the live word; no cache moves.
    pub fn frame_visual_flags_word(&self, key: FrameKey) -> Result<u32, FrameError> {
        Ok(self.state(key)?.visual_flags)
    }
    /// Working name for the `m_pPrev` sibling read at +0x128
    /// (mafia-re ls3df/I3D/I3D.h:679). No LS3DF FUNCTION address exists
    /// for it: siblings are only walked by hierarchy traversals.
    /// Borrows the live link; no ownership claim.
    pub fn frame_prev_key(&self, key: FrameKey) -> Result<Option<FrameKey>, FrameError> {
        Ok(self.links(key)?.previous)
    }
    /// Working name for the `m_pNext` sibling read at +0x128
    /// (mafia-re ls3df/I3D/I3D.h:679). No LS3DF FUNCTION address exists
    /// for it: siblings are only walked by hierarchy traversals.
    /// Borrows the live link; no ownership claim.
    pub fn frame_next_key(&self, key: FrameKey) -> Result<Option<FrameKey>, FrameError> {
        Ok(self.links(key)?.next)
    }
    /// Working name for the `m_pChild` first-child read at +0x124 (mafia-re
    /// ls3df/I3D/I3D.h:678). No LS3DF FUNCTION address exists for a
    /// standalone reader: first children are only walked by hierarchy
    /// traversals. Borrows the live link; no ownership claim.
    pub fn frame_first_child_key(&self, key: FrameKey) -> Result<Option<FrameKey>, FrameError> {
        Ok(self.links(key)?.child)
    }
    /// Working name for the scalar `m_WorldScale` read at +0x114
    /// (mafia-re ls3df/I3D/I3D.h:674). No LS3DF FUNCTION address exists
    /// for it: the only out-of-line scale reader is the `GetWorldScale`
    /// vector (Game.exe 0x47a7b0, covered by `frame_world_scale`).
    /// Reports the stored averaged scale; the matrix cache is untouched.
    pub fn frame_world_scale_scalar(&self, key: FrameKey) -> Result<f32, FrameError> {
        Ok(self.state(key)?.world_scale)
    }
    /// LS3DF-exported `I3D_frame::SetRot` (mafia-re ls3df/I3D/I3D.h:420-427):
    /// the engine build stores the quaternion without the Normalize the
    /// game build performs (the game copy, Game.exe 0x43bce0, is covered by
    /// `assign_rotation`). Flag store clears HR_BOUND_W/WMAT/MAT/ROT and
    /// sets UPDATE/QUAT_VALID, like the game copy but without normalizing.
    pub fn store_frame_rotation(&mut self, key: FrameKey, rotation: SQuat) -> Result<(), FrameError> {
        let state = &mut self.node_mut(key)?.state;
        state.rotation = rotation;
        state.flags = (state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }
    /// Working name for the LS3DF-inlined `I3D_frame::SetLocalMatrix`
    /// (mafia-re ls3df/I3D/I3D.h:495-503). The reference records no LS3DF
    /// FUNCTION address for it (header inline compiled into LS3DF callers;
    /// the file notes inline sites but no out-of-line copy in either
    /// binary). Makes the input the local matrix, remakes the scale from
    /// its basis rows through the `GetScale` projection, then clears
    /// WMAT_VALID and QUAT_VALID and sets UPDATE, MAT_VALID and ROT_VALID.
    /// The scale is remade from the input before any store, so a
    /// host-unavailable scale leaves the prior matrix, scale and flags
    /// untouched (native has no failure; disclosed reorder).
    pub fn store_frame_local_matrix(
        &mut self,
        key: FrameKey,
        matrix: SMatrix,
    ) -> Result<(), FrameError> {
        let scale = matrix
            .try_get_scale()
            .map_err(|_| FrameError::NumericDomain("SetLocalMatrix GetScale"))?;
        let state = &mut self.node_mut(key)?.state;
        state.local = matrix;
        state.scale = scale;
        state.flags = (state.flags & 0xffffffd7) | 0x40000014;
        Ok(())
    }
    /// Working name for the LS3DF-inlined `I3D_frame::SetDir1`
    /// (mafia-re ls3df/I3D/I3D.h:467-475). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x4609c0,
    /// which is not claimed here. Turns the local matrix to `direction`
    /// with `up` as the up vector, then clears HR_BOUND_W/WMAT/QUAT/ROT
    /// and sets UPDATE/MAT_VALID. A degenerate direction/up pair leaves
    /// both caches unavailable instead of a partially written basis.
    pub fn set_direction_from_up(
        &mut self,
        key: FrameKey,
        direction: [f32; 3],
        up: [f32; 3],
    ) -> Result<(), FrameError> {
        let result = (|| {
            self.node_mut(key)?
                .state
                .local
                .try_set_dir3(direction, up)
                .map_err(|_| FrameError::DirectionFromVectorsUnavailable)?;
            let state = &mut self.node_mut(key)?.state;
            state.flags = (state.flags & 0xfffffed3) | 0x40000010;
            Ok(())
        })();
        let node = self.node_mut(key)?;
        node.local_error = result.as_ref().err().copied();
        node.world_error = result.as_ref().err().copied();
        result
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetDir` read
    /// (mafia-re ls3df/I3D/I3D.h:558-572). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47a890,
    /// which is not claimed here. With a stale local matrix the MAT_VALID
    /// bit is set first, the rotation is normalized in place and the
    /// basis rebuilt without rescaling rows (unlike UpdateMatrix); the
    /// returned row is raw. Otherwise a normalized copy of row 2 is
    /// returned and no state moves.
    pub fn frame_local_direction(&mut self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        let result = (|| {
            if self.state(key)?.flags & 0x10 == 0 {
                self.node_mut(key)?.state.flags |= 0x10;
                let mut rotation = self.state(key)?.rotation;
                rotation
                    .try_normalize()
                    .map_err(|_| FrameError::NumericDomain("GetDir Normalize"))?;
                self.node_mut(key)?.state.rotation = rotation;
                let rotation = self.state(key)?.rotation;
                self.node_mut(key)?
                    .state
                    .local
                    .try_set_rot3(rotation)
                    .map_err(|_| FrameError::NumericDomain("GetDir SetRot3"))?;
                let local = self.state(key)?.local;
                Ok([local.elements[8], local.elements[9], local.elements[10]])
            } else {
                let local = self.state(key)?.local;
                Ok(vector_set_normalized([
                    local.elements[8],
                    local.elements[9],
                    local.elements[10],
                ]))
            }
        })();
        let node = self.node_mut(key)?;
        node.local_error = result.as_ref().err().copied();
        node.world_error = result.as_ref().err().copied();
        result
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetWorldDir` read
    /// (mafia-re ls3df/I3D/I3D.h:573-581). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47aa40,
    /// which is not claimed here. Refreshes the world cache first, then
    /// returns its normalized row 2.
    pub fn frame_world_direction(&mut self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        let world = self.world_matrix(key)?;
        Ok(vector_set_normalized([
            world.elements[8],
            world.elements[9],
            world.elements[10],
        ]))
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetRot` read
    /// (mafia-re ls3df/I3D/I3D.h:5002-5009). The reference notes LS3DF
    /// keeps no out-of-line copy of it (only Game.exe 0x47a810, which is
    /// not claimed here); the body is ported verbatim from the header
    /// with host unavailability guards. With a stale rotation the
    /// QUAT_VALID bit is stored first, then the quaternion is remade from
    /// the local matrix; the stored value is returned either way. A
    /// matrix the host cannot convert poisons both caches instead of
    /// inventing a rotation.
    pub fn frame_rotation(&mut self, key: FrameKey) -> Result<SQuat, FrameError> {
        if self.state(key)?.flags & 8 == 0 {
            self.node_mut(key)?.state.flags |= 8;
            let result = SQuat::try_make_from_matrix(&self.state(key)?.local);
            match result {
                Ok(rotation) => self.node_mut(key)?.state.rotation = rotation,
                Err(_) => {
                    let error = FrameError::QuaternionFromMatrixUnavailable;
                    let node = self.node_mut(key)?;
                    node.local_error = Some(error);
                    node.world_error = Some(error);
                    return Err(error);
                }
            }
        }
        Ok(self.state(key)?.rotation)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetWorldRot` read
    /// (mafia-re ls3df/I3D/I3D.h:5034-5040). No LS3DF FUNCTION address
    /// exists for it: the only out-of-line copy is Game.exe 0x47a840,
    /// which is not claimed here. Makes the rotation from the refreshed
    /// world matrix into the return value; unlike `frame_rotation` no
    /// flag is stored. An unconvertible world cache is a host
    /// unavailability, never an invented rotation.
    pub fn frame_world_rotation(&mut self, key: FrameKey) -> Result<SQuat, FrameError> {
        let result = (|| {
            let world = self.world_matrix(key)?;
            SQuat::try_make_from_matrix(&world)
                .map_err(|_| FrameError::QuaternionFromMatrixUnavailable)
        })();
        if let Err(error) = result.as_ref() {
            if let Ok(node) = self.node_mut(key) {
                node.world_error = Some(*error);
            }
        }
        result
    }
    /// GetFrameInterpolator RVA0x1ea60 marks a newly bound destination.
    pub(crate) fn mark_animated(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.node_mut(key)?.state.flags |= 0x10000000;
        Ok(())
    }
    /// Register the parsed Target links after all model identities exist.
    /// LoadImage sets membership bit 0x4000 for every nonzero reference, while
    /// preserving duplicates and registration order in its owned registry.
    pub fn register_target(
        &mut self,
        key: FrameKey,
        state: I3dTargetState,
    ) -> Result<(), FrameError> {
        if self.state(key)?.kind != FrameKind::Target {
            return Err(FrameError::NotTarget(key));
        }
        for &linked in state.frames() {
            self.index(linked)?;
        }
        self.target_updates
            .reserve_pending(self.nodes.len())
            .map_err(|_| FrameError::TargetUnavailable(key))?;
        // All keys were preflighted; successful insertion is followed only by
        // infallible stores, including repeated stores for duplicate references.
        self.targets
            .insert(key, state)
            .map_err(|_| FrameError::TargetUnavailable(key))?;
        for index in 0..self.targets.state(key).unwrap().frames().len() {
            let linked = self.targets.state(key).unwrap().frames()[index];
            self.node_mut(linked)?.state.flags |= 0x4000;
        }
        Ok(())
    }

    /// Replace the empty constructor registration after parsed references have
    /// resolved. Unlike AddToTarget, LoadImage preserves duplicate ownership.
    pub(crate) fn apply_target_load_image_state(
        &mut self,
        key: FrameKey,
        state: I3dTargetState,
    ) -> Result<(), FrameError> {
        if !self.target_state(key)?.frames().is_empty() {
            return Err(FrameError::TargetUnavailable(key));
        }
        for &linked in state.frames() {
            self.index(linked)?;
        }
        *self
            .targets
            .state_mut(key)
            .map_err(|_| FrameError::TargetUnavailable(key))? = state;
        for index in 0..self.target_state(key)?.frames().len() {
            let linked = self.target_state(key)?.frames()[index];
            self.node_mut(linked)?.state.flags |= 0x4000;
        }
        Ok(())
    }

    pub fn target_state(&self, key: FrameKey) -> Result<&I3dTargetState, FrameError> {
        self.index(key)?;
        self.targets
            .state(key)
            .map_err(|_| FrameError::TargetUnavailable(key))
    }
    /// Native GetTarget scans live registrations in order, independent of the
    /// source file's parsed Target reference words.
    pub(crate) fn first_target_owner(
        &self,
        frame: FrameKey,
    ) -> Result<Option<FrameKey>, FrameError> {
        self.index(frame)?;
        Ok(self.targets.first_owner(frame))
    }
    /// Target::Duplicate stores the live derived flags after common copy.
    pub(crate) fn copy_target_duplicate_flags(
        &mut self,
        target: FrameKey,
        flags: u32,
    ) -> Result<(), FrameError> {
        self.target_state(target)?;
        self.targets
            .state_mut(target)
            .map_err(|_| FrameError::TargetUnavailable(target))?
            .copy_duplicate_flags(flags);
        Ok(())
    }
    /// Deferred DuplicateHierarchy AddToTarget, with its first-owner removal
    /// and one append per queued member. Preserve any completed prefix on an
    /// allocation failure, just as the live Target registry does.
    pub(crate) fn add_duplicate_target_member(
        &mut self,
        target: FrameKey,
        frame: FrameKey,
    ) -> Result<(), FrameError> {
        self.target_state(target)?;
        self.index(frame)?;
        let mut targets = std::mem::take(&mut self.targets);
        let result = targets.add_to_target(
            target,
            frame,
            &mut self.node_mut(frame).unwrap().state.flags,
        );
        self.targets = targets;
        result.map_err(|_| FrameError::TargetUnavailable(target))
    }

    pub fn joint_state(&self, key: FrameKey) -> Result<&I3dJointState, FrameError> {
        self.node(key)?
            .joint
            .as_ref()
            .ok_or(FrameError::NotJoint(key))
    }
    /// Mutable joint projection for the Duplicate derived half (PC RVA
    /// 0x1eeb0). Mirrors joint_state.
    pub fn joint_state_mut(&mut self, key: FrameKey) -> Result<&mut I3dJointState, FrameError> {
        self.node_mut(key)?
            .joint
            .as_mut()
            .ok_or(FrameError::NotJoint(key))
    }
    /// Import a known PC joint projection, including an available native cache.
    /// This host intake does not execute the original constructor or loader.
    pub fn insert_joint_state(
        &mut self,
        mut state: I3dFrameState,
        joint: I3dJointState,
    ) -> Result<FrameKey, FrameError> {
        if let Some(target) = joint.singlemesh {
            self.singlemesh_invalidation_state(target)?;
        }
        state.kind = FrameKind::Joint;
        let key = self.insert(state)?;
        self.node_mut(key)?.joint = Some(joint);
        Ok(key)
    }

    /// Original GetJointMatrix, PC RVA0x56ba0. The valid flag is set before
    /// UpdateMatrix, and each completed ancestor product replaces the cache.
    /// The first model/visual ancestor is excluded. A cached parent joint
    /// terminates the walk after one product. Unsupported arithmetic preserves
    /// preceding stores and marks the interrupted cache unavailable to callers.
    pub fn get_joint_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        if self.joint_state(key)?.flags & 0x10 != 0 {
            if let Some(error) = self.node(key)?.joint_error {
                return Err(error);
            }
            return self
                .joint_state(key)?
                .joint_matrix
                .ok_or(FrameError::JointMatrixUnavailable(key));
        }
        self.node_mut(key)?.joint.as_mut().unwrap().flags |= 0x10;
        let result = (|| {
            let local = self.local_matrix(key)?;
            self.node_mut(key)?.joint.as_mut().unwrap().joint_matrix = Some(local);
            let mut parent = self.links(key)?.parent;
            while let Some(at) = parent {
                let kind = self.state(at)?.kind;
                if matches!(
                    kind,
                    FrameKind::Model
                        | FrameKind::Mesh
                        | FrameKind::Lensflare
                        | FrameKind::Projector
                        | FrameKind::Singlemesh
                ) {
                    break;
                }
                let cached = kind == FrameKind::Joint && self.joint_state(at)?.flags & 0x10 != 0;
                let right = if cached {
                    self.get_joint_matrix(at)?
                } else {
                    self.local_matrix(at)?
                };
                let product = self
                    .joint_state(key)?
                    .joint_matrix
                    .unwrap()
                    .try_mul_affine(&right)
                    .map_err(|_| FrameError::NumericDomain("GetJointMatrix multiply"))?;
                self.node_mut(key)?.joint.as_mut().unwrap().joint_matrix = Some(product);
                if cached {
                    break;
                }
                parent = self.links(at)?.parent;
            }
            Ok(self.joint_state(key)?.joint_matrix.unwrap())
        })();
        self.node_mut(key)?.joint_error = result.as_ref().err().copied();
        result
    }
    /// Complete derived LoadImage payload; the caller applies common fields
    /// first. Matrix/index bits are retained even outside numeric getter domains.
    pub fn apply_joint_load_image_fields(
        &mut self,
        key: FrameKey,
        bone_matrix: SMatrix,
        index: u32,
    ) -> Result<(), FrameError> {
        self.node_mut(key)?
            .joint
            .as_mut()
            .ok_or(FrameError::NotJoint(key))?
            .apply_load_image_fields(bone_matrix, index);
        self.node_mut(key)?.joint_error = None;
        Ok(())
    }
    /// Import the mesh-presence and invalidation fields needed by the binding
    /// prefix. This is not a native singlemesh constructor or geometry loader.
    pub fn insert_singlemesh_binding(
        &mut self,
        state: I3dFrameState,
        invalidation: SinglemeshInvalidationState,
        mesh_present: bool,
    ) -> Result<FrameKey, FrameError> {
        let key = self.insert_singlemesh_invalidation_target(state, invalidation)?;
        self.node_mut(key)?.binding = Some(I3dSinglemeshBinding {
            mesh_present,
            ..Default::default()
        });
        Ok(key)
    }
    pub fn singlemesh_binding(&self, key: FrameKey) -> Result<&I3dSinglemeshBinding, FrameError> {
        self.node(key)?
            .binding
            .as_ref()
            .ok_or(FrameError::SinglemeshBindingUnavailable(key))
    }
    /// Last successfully calculated bound, available only while the native
    /// bound-valid flag is set and no interrupted calculation is pending.
    pub fn singlemesh_bound_volume(
        &self,
        key: FrameKey,
    ) -> Result<SinglemeshBoundVolume, FrameError> {
        let node = self.node(key)?;
        if let Some(error) = node.singlemesh_bound_error {
            return Err(error);
        }
        if node.state.visual_flags & 0x20000 == 0 {
            return Err(FrameError::SinglemeshBoundsUnavailable(key));
        }
        self.singlemesh_binding(key)?
            .bound_volume
            .ok_or(FrameError::SinglemeshBoundsUnavailable(key))
    }
    /// DBuild for an explicitly supplied, loaded first-LOD skin definition.
    /// After binding, the original bounds traversal eagerly evaluates joint
    /// matrices before the final DBuild visual-flag store. Missing mesh/empty
    /// discovery retains the native false result. The caller owns the validated
    /// LOD/definition relationship; this does not load resources or supply the
    /// missing-LOD and missing-definition fallback backends.
    pub fn d_build_loaded_singlemesh(
        &mut self,
        key: FrameKey,
        initial_bbox: SinglemeshBoundingBox,
        joint_boxes: &[SinglemeshBoundingBox],
    ) -> Result<bool, FrameError> {
        match self.d_build_binding(key)? {
            DBuildBindingOutcome::NoMesh | DBuildBindingOutcome::NoJoints => return Ok(false),
            DBuildBindingOutcome::BoundVolumeUpdateRequired => {}
        }
        self.update_singlemesh_loaded_bounds(key, initial_bbox, joint_boxes)?;
        let state = &mut self.node_mut(key)?.state;
        if state.visual_flags & 0x40 == 0 {
            state.visual_flags &= !0x40000;
        }
        Ok(true)
    }
    /// Original visual base state, PC RVA 0x5a200 (differential: Ghidra body
    /// 324 B, 63 insn/1 BB; mafia-re I3d_visual.cpp:8-19). Upstream working
    /// names, not original symbols. Base word only: frame ctor, invalidated
    /// bboxes, zeroed spheres, vis_flags 0x2200 (VISF_SHADOW|VISF_PROJECTOR).
    /// Derived object words (e.g. VISF_OBJECT 0x800 in mesh()) stay with
    /// their loader-owned constructors.
    pub fn visual_base_state() -> I3dFrameState {
        let mut state = I3dFrameState::model();
        state.visual_flags = 0x2200;
        state
    }
    /// Upstream working names for SHADOW_MODE_0/1/2 (mafia-re
    /// I3d_visual.cpp:55-103), adapted to snake_case. Contract tier per the
    /// unresolved-boundary rule: no Ghidra body (not_found; vtable-referenced
    /// bytes, procedure:null); behavior from entry/mid/RET probes.
    pub fn set_visual_shadow_mode(&mut self, key: FrameKey, mode: i32) -> Result<bool, FrameError> {
        let Some(mode) = ShadowMode::from_i32(mode) else {
            self.state(key)?;
            return Ok(false);
        };
        let flags = &mut self.node_mut(key)?.state.visual_flags;
        match mode {
            ShadowMode::Mode0 => *flags &= !0x600,
            ShadowMode::Mode1 => *flags = (*flags & !0x400) | 0x200,
            ShadowMode::Mode2 => *flags |= 0x600,
        }
        Ok(true)
    }
    /// PC RVA 0x5a460: TEST AH,4 (bit10 0x400) then (flags>>9)&1.
    pub fn visual_shadow_mode(&self, key: FrameKey) -> Result<ShadowMode, FrameError> {
        let flags = self.state(key)?.visual_flags;
        if flags & 0x400 != 0 {
            return Ok(ShadowMode::Mode2);
        }
        Ok(if flags & 0x200 != 0 {
            ShadowMode::Mode1
        } else {
            ShadowMode::Mode0
        })
    }
    /// PC RVA 0x5a480: same switch shape with 0x6000/0x2000/0x9fff masks.
    pub fn set_visual_projector_mode(
        &mut self,
        key: FrameKey,
        mode: i32,
    ) -> Result<bool, FrameError> {
        let Some(mode) = ShadowMode::from_i32(mode) else {
            self.state(key)?;
            return Ok(false);
        };
        let flags = &mut self.node_mut(key)?.state.visual_flags;
        match mode {
            ShadowMode::Mode0 => *flags &= !0x6000,
            ShadowMode::Mode1 => *flags = (*flags & !0x4000) | 0x2000,
            ShadowMode::Mode2 => *flags |= 0x6000,
        }
        Ok(true)
    }
    /// PC RVA 0x5a4d0: TEST AH,0x40 (bit14 0x4000) then (flags>>13)&1.
    pub fn visual_projector_mode(&self, key: FrameKey) -> Result<ShadowMode, FrameError> {
        let flags = self.state(key)?.visual_flags;
        if flags & 0x4000 != 0 {
            return Ok(ShadowMode::Mode2);
        }
        Ok(if flags & 0x2000 != 0 {
            ShadowMode::Mode1
        } else {
            ShadowMode::Mode0
        })
    }
    /// PC RVA 0x5a4f0: OR 0x20000, exact 3-insn body.
    pub fn mark_visual_bound_valid(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.node_mut(key)?.state.visual_flags |= 0x20000;
        Ok(())
    }
    /// Unconditional visual-flag word op shared by the mirror Duplicate /
    /// LoadImage / UpdateV contracts (native AND/OR immediates, no
    /// arithmetic). Helper: no function credit.
    pub(crate) fn set_visual_flag_words(
        &mut self,
        key: FrameKey,
        clear: u32,
        set: u32,
    ) -> Result<(), FrameError> {
        let state = &mut self.node_mut(key)?.state;
        state.visual_flags = (state.visual_flags & !clear) | set;
        Ok(())
    }
    /// m_UpdateCount = -1 stamp of the mirror UpdateV copies loop (u32::MAX).
    /// Helper: no function credit.
    pub(crate) fn mark_copy_stale(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.node_mut(key)?.state.update_count = u32::MAX;
        Ok(())
    }
    /// Original SetSpace, PC RVA 0x5a3c0 (contract: no Ghidra body;
    /// entry/mid/RET probes + format bytes; mafia-re I3d_visual.cpp:40-51).
    /// Ok(false) = early return, zero backend effects (both no-change arms
    /// evidenced). Else backend effects in native order (type, then name,
    /// then print); no flag is ever mutated (native SetSpace stores
    /// nothing). The visual_type word (+0x1d4) has no arena field yet, so the
    /// caller supplies it (narrow adapter — adding the field is a layout
    /// claim needing its own evidence). Device calls (VisualType2String +
    /// dbgPrintf) stay backend contracts.
    pub fn set_visual_space<B: VisualSpaceBackend>(
        &mut self,
        key: FrameKey,
        world: bool,
        visual_type: u32,
        backend: &mut B,
    ) -> Result<bool, FrameError>
    where
        FrameError: From<B::Error>,
    {
        self.state(key)?;
        let world_space = self.state(key)?.visual_flags & 0x40 != 0;
        if world_space == world {
            return Ok(false);
        }
        let label = backend
            .visual_type_label(visual_type)
            .map_err(FrameError::from)?;
        let name = backend.frame_name().map_err(FrameError::from)?;
        backend
            .debug_print_unsupported_set_space(&label, name.as_deref())
            .map_err(FrameError::from)?;
        Ok(true)
    }
    /// Populated-definition UpdateBVolumeProc path (PC RVA0x54810), after a
    /// successful binding allocation. The counter store precedes arithmetic
    /// and joint-cache work. On unsupported numeric input, completed joint
    /// cache writes remain, but no native partial volume is claimed or exposed.
    pub fn update_singlemesh_loaded_bounds(
        &mut self,
        key: FrameKey,
        initial_bbox: SinglemeshBoundingBox,
        joint_boxes: &[SinglemeshBoundingBox],
    ) -> Result<(), FrameError> {
        self.singlemesh_binding(key)?;
        let changed = self.singlemesh_invalidation_state(key)?.joints_changed;
        self.node_mut(key)?
            .binding
            .as_mut()
            .unwrap()
            .bvol_joints_changed = changed;
        let result = (|| {
            let binding = self.singlemesh_binding(key)?;
            if !binding.mesh_present || binding.joints.is_empty() {
                return Err(FrameError::SinglemeshBoundsUnavailable(key));
            }
            let count = binding.joints.len();
            if joint_boxes.len() < count {
                return Err(FrameError::SinglemeshBoundsDefinitionTooShort(key));
            }
            // Unwritten PC allocation slots are never interpreted as null.
            if binding.joints.iter().any(Option::is_none) {
                return Err(FrameError::SinglemeshBindingUnavailable(key));
            }
            let boxes: Vec<_> = joint_boxes[..count].iter().copied().map(Some).collect();
            let volume = calculate_singlemesh_bounds(initial_bbox, &boxes, |index| {
                let joint = self.singlemesh_binding(key)?.joints[index].unwrap();
                self.get_joint_matrix(joint)
            })
            .map_err(|error| match error {
                SinglemeshBoundsError::JointMatrix(error) => error,
                SinglemeshBoundsError::NumericDomain => {
                    FrameError::NumericDomain("singlemesh bound volume")
                }
            })?;
            let node = self.node_mut(key)?;
            node.binding.as_mut().unwrap().bound_volume = Some(volume);
            node.state.visual_flags = (node.state.visual_flags & !0x40000) | 0x20000;
            self.discard_hr_bound_vol(key)
        })();
        self.node_mut(key)?.singlemesh_bound_error = result.as_ref().err().copied();
        result
    }
    /// Singlemesh bounds virtual wrapper, PC RVA0x547e0 (singlemorph RVA0x57580
    /// tail-jumps here). A matching Joint-change counter only sets the valid
    /// bit. Otherwise the populated-definition calculation stores the counter
    /// before its graph/arithmetic work. An interrupted host calculation remains
    /// unavailable; its equal counter must not publish an incomplete volume.
    pub fn update_singlemesh_loaded_bounds_cached(
        &mut self,
        key: FrameKey,
        initial_bbox: SinglemeshBoundingBox,
        joint_boxes: &[SinglemeshBoundingBox],
    ) -> Result<(), FrameError> {
        let node = self.node(key)?;
        if let Some(error) = node.singlemesh_bound_error {
            return Err(error);
        }
        let stored = self.singlemesh_binding(key)?.bvol_joints_changed;
        let changed = self.singlemesh_invalidation_state(key)?.joints_changed;
        if stored != changed {
            self.update_singlemesh_loaded_bounds(key, initial_bbox, joint_boxes)
        } else {
            self.node_mut(key)?.state.visual_flags |= 0x20000;
            Ok(())
        }
    }
    /// Original PC findJoint, RVA0x54d80: descendants in sibling-head preorder,
    /// with at most 64 joints. Nested visual nodes do not stop the PC search.
    /// The receiver itself is excluded. Owned links are cycle-free.
    pub fn find_joint(&self, key: FrameKey) -> Result<Vec<FrameKey>, FrameError> {
        let mut found = Vec::new();
        let mut at = self.links(key)?.child;
        while let Some(current) = at {
            if self.state(current)?.kind == FrameKind::Joint {
                if found.len() == 64 {
                    // Native breaks this sibling level; unwind to its parent.
                    at = self.find_joint_next(key, current, false)?;
                    continue;
                }
                found.push(current);
            }
            at = match self.links(current)?.child {
                Some(child) => Some(child),
                None => self.find_joint_next(key, current, true)?,
            };
        }
        Ok(found)
    }
    fn find_joint_next(
        &self,
        root: FrameKey,
        mut current: FrameKey,
        include_sibling: bool,
    ) -> Result<Option<FrameKey>, FrameError> {
        if include_sibling {
            if let Some(next) = self.links(current)?.next {
                return Ok(Some(next));
            }
        }
        while let Some(parent) = self.links(current)?.parent {
            if parent == root {
                break;
            }
            if let Some(next) = self.links(parent)?.next {
                return Ok(Some(next));
            }
            current = parent;
        }
        Ok(None)
    }
    /// DBuild's binding prefix, PC RVA0x54c20. Old backreferences are cleared
    /// before checking mesh presence. Each dense slot takes the first matching
    /// index in findJoint order. Unwritten native allocation slots remain host
    /// unavailable markers. Bounds and the subsequent visual-flag store are
    /// deliberately pending; BoundVolumeUpdateRequired is not a true return.
    pub fn d_build_binding(&mut self, key: FrameKey) -> Result<DBuildBindingOutcome, FrameError> {
        self.singlemesh_binding(key)?;
        // An unwritten slot is not safe to dereference on the next native call.
        if self
            .singlemesh_binding(key)?
            .joints
            .iter()
            .any(Option::is_none)
        {
            return Err(FrameError::SinglemeshBindingUnavailable(key));
        }
        let old = std::mem::take(&mut self.node_mut(key)?.binding.as_mut().unwrap().joints);
        for joint in old.into_iter().flatten() {
            self.node_mut(joint)?.joint.as_mut().unwrap().singlemesh = None;
        }
        if !self.singlemesh_binding(key)?.mesh_present {
            return Ok(DBuildBindingOutcome::NoMesh);
        }
        let found = self.find_joint(key)?;
        if found.is_empty() {
            return Ok(DBuildBindingOutcome::NoJoints);
        }
        let mut slots = vec![None; found.len()];
        for (index, slot) in slots.iter_mut().enumerate() {
            for &joint in &found {
                if self.joint_state(joint)?.index == index as u32 {
                    *slot = Some(joint);
                    self.node_mut(joint)?.joint.as_mut().unwrap().singlemesh = Some(key);
                    break;
                }
            }
        }
        self.node_mut(key)?.binding.as_mut().unwrap().joints = slots;
        Ok(DBuildBindingOutcome::BoundVolumeUpdateRequired)
    }
    /// Import only a known singlemesh frame/counter projection for linked
    /// Joint::UpdateV. This does not construct or load a native singlemesh.
    pub fn insert_singlemesh_invalidation_target(
        &mut self,
        mut state: I3dFrameState,
        invalidation: SinglemeshInvalidationState,
    ) -> Result<FrameKey, FrameError> {
        state.kind = FrameKind::Singlemesh;
        let key = self.insert(state)?;
        self.node_mut(key)?.singlemesh = Some(invalidation);
        Ok(key)
    }
    pub fn singlemesh_invalidation_state(
        &self,
        key: FrameKey,
    ) -> Result<&SinglemeshInvalidationState, FrameError> {
        self.node(key)?
            .singlemesh
            .as_ref()
            .ok_or(FrameError::InvalidSinglemeshTarget(key))
    }
    /// Assign the observed pointer field using an owned target. This low-level
    /// projection does not populate the target's DBuild joint-index array.
    pub fn assign_joint_singlemesh_target(
        &mut self,
        key: FrameKey,
        target: Option<FrameKey>,
    ) -> Result<(), FrameError> {
        self.joint_state(key)?;
        if let Some(target) = target {
            self.singlemesh_invalidation_state(target)?;
        }
        self.node_mut(key)?.joint.as_mut().unwrap().singlemesh = target;
        Ok(())
    }
    /// Clear one singlemesh joint slot for the joint destructor. Host helper,
    /// no function credit. Out-of-range indices and missing bindings are
    /// explicit unavailability; a vacant slot clears idempotently.
    pub(crate) fn clear_singlemesh_joint_slot(
        &mut self,
        target: FrameKey,
        index: u32,
    ) -> Result<Option<FrameKey>, FrameError> {
        let slot = index as usize;
        let node = self.node_mut(target)?;
        let binding = node
            .binding
            .as_mut()
            .ok_or(FrameError::SinglemeshBindingUnavailable(target))?;
        if slot >= binding.joints.len() {
            return Err(FrameError::SinglemeshBindingUnavailable(target));
        }
        Ok(binding.joints[slot].take())
    }
    pub fn links(&self, key: FrameKey) -> Result<FrameLinks, FrameError> {
        Ok(self.node(key)?.links)
    }
    /// Working name for the inlined `I3D_frame::GetPos` local-row read
    /// (mafia-re ls3df/I3D/I3D.h:460-465 at pin 113b5b6: `return
    /// m_LocalMatrix(3)`). The reference records no LS3DF FUNCTION address
    /// for a standalone reader; the only out-of-line copy is Game.exe
    /// 0x47a720, which is not claimed here. Reports the stored local
    /// position row; no cache is refreshed.
    pub fn frame_local_pos_of(&self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        Ok(self.state(key)?.local.elements[12..15].try_into().unwrap())
    }
    /// Working name for the inlined `I3D_frame::GetWorldPos` read
    /// (mafia-re ls3df/I3D/I3D.h:552-556 at pin 113b5b6: `return
    /// GetMatrix()(3)`). Same inline-only status as `frame_local_pos_of`:
    /// no LS3DF FUNCTION entry exists, and the only out-of-line copy is
    /// Game.exe 0x47a730, which is not claimed here. Refreshes the world
    /// matrix through the native `WMAT_VALID` gate, then reports its
    /// position row.
    pub fn frame_world_pos_of(&mut self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        Ok(self.world_matrix(key)?.elements[12..15].try_into().unwrap())
    }
    /// Working name for the inlined `I3D_frame::GetScale` member read
    /// (mafia-re ls3df/I3D/I3D.h:477-481 at pin 113b5b6). Same
    /// inline-only status: no LS3DF FUNCTION entry exists, and the only
    /// out-of-line copy is Game.exe 0x47aec0, which is not claimed here.
    /// Reports the stored scale; no matrix is remade.
    pub fn frame_scale_of(&self, key: FrameKey) -> Result<[f32; 3], FrameError> {
        Ok(self.state(key)?.scale)
    }
    /// Working name for the inlined `I3D_frame::GetParent` member read
    /// (mafia-re ls3df/I3D/I3D.h:5043-5046 at pin 113b5b6). Same
    /// inline-only status: no LS3DF FUNCTION entry exists, and the only
    /// out-of-line copy is Game.exe 0x47ad10, which is not claimed here.
    /// Reports the stored parent link; unknown frames stay an explicit
    /// error, never a null parent.
    pub fn frame_parent_of(&self, key: FrameKey) -> Result<Option<FrameKey>, FrameError> {
        Ok(self.links(key)?.parent)
    }
    /// Working name for the inlined `I3D_frame::NumChildren` member read
    /// (mafia-re ls3df/I3D/I3D.h:619-623 at pin 113b5b6). Same
    /// inline-only status: no LS3DF FUNCTION entry exists, and the only
    /// out-of-line copy is Game.exe 0x47ad20, which is not claimed here.
    /// Reports the stored child count (native `int` width; the graph
    /// stores the same value in `u32`, so no truncation is observable
    /// through this reader).
    pub fn frame_num_children_of(&self, key: FrameKey) -> Result<u32, FrameError> {
        Ok(self.links(key)?.child_count)
    }
    pub fn storage_bytes(&self) -> Result<usize, FrameError> {
        let binding_bytes = self
            .nodes
            .iter()
            .try_fold(0usize, |total, node| {
                let bytes = match &node.binding {
                    Some(binding) => binding
                        .joints
                        .capacity()
                        .checked_mul(std::mem::size_of::<Option<FrameKey>>())?,
                    None => 0,
                };
                total.checked_add(bytes)
            })
            .ok_or(FrameError::AllocationOverflow)?;
        self.nodes
            .capacity()
            .checked_mul(std::mem::size_of::<Node>())
            .and_then(|n| {
                n.checked_add(
                    self.world_path
                        .capacity()
                        .checked_mul(std::mem::size_of::<FrameKey>())?,
                )
            })
            .and_then(|n| n.checked_add(binding_bytes))
            .and_then(|n| n.checked_add(self.targets.storage_bytes().ok()?))
            .and_then(|n| n.checked_add(self.target_updates.storage_bytes().ok()?))
            .and_then(|n| {
                n.checked_add(
                    self.visits
                        .capacity()
                        .checked_mul(std::mem::size_of::<Visit>())?,
                )
            })
            .ok_or(FrameError::AllocationOverflow)
    }
    /// Common field projection of FrameLoadImage, RVA0x1d1c0, after a
    /// complete record has been validated. The loader first constructs and
    /// links the frame, and separately owns raw names/properties. These stores
    /// ignore position lock and preserve quaternion bits without normalization.
    /// Short reads, name-cache effects and original allocation are outside scope.
    pub fn apply_common_load_image_fields(
        &mut self,
        key: FrameKey,
        position: [f32; 3],
        scale: [f32; 3],
        rotation: SQuat,
        raw_logic_flags: u8,
    ) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.local.elements[12..15].copy_from_slice(&position);
        node.state.scale = scale;
        node.state.rotation = rotation;
        node.state.flags = (node.state.flags & 0xffffff00) | u32::from(raw_logic_flags);
        node.state.flags = (node.state.flags & 0xfffffe8b) | 0x40000008;
        // The native mask invalidates both caches. Subsequent getters recheck
        // the complete new SRT rather than retaining a previous host interruption.
        node.local_error = None;
        node.world_error = None;
        Ok(())
    }
    /// Observed LoadHierarchy EndLoad root store, followed by original Update.
    /// Resource loading remains outside this field projection.
    pub fn mark_loaded_root(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.mark_loaded_root_fields(key)?;
        self.update(key)
    }
    pub(crate) fn mark_loaded_root_fields(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let state = &mut self.node_mut(key)?.state;
        state.flags = (state.flags & 0xfffffeff) | 0x40000000;
        Ok(())
    }
    /// Original bound-cache invalidation at RVA0x1cc00, used by model Close.
    /// Sector type5 stops both direct receiver and ancestor invalidation before
    /// any of its flags are cleared.
    pub fn discard_hr_bound_vol(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind == FrameKind::Sector {
            return Ok(());
        }
        self.node_mut(key)?.state.flags &= 0xfffffebf;
        let mut parent = self.links(key)?.parent;
        while let Some(key) = parent {
            let node = self.node_mut(key)?;
            if node.state.flags & 0x40 == 0 || node.state.kind == FrameKind::Sector {
                break;
            }
            node.state.flags &= 0xfffffebf;
            parent = node.links.parent;
        }
        Ok(())
    }
    /// Copies even equal values, preserving signed zero. Flag0x80 locks position.
    pub fn assign_position(&mut self, key: FrameKey, p: [f32; 3]) -> Result<(), FrameError> {
        let s = &mut self.node_mut(key)?.state;
        if s.flags & 0x80 == 0 {
            s.local.elements[12..15].copy_from_slice(&p);
            s.flags = (s.flags & 0xfffffedf) | 0x40000000;
        }
        Ok(())
    }
    /// Working name for the LS3DF-inlined `I3D_frame::SetLocalMatrix`
    /// (mafia-re ls3df/I3D/I3D.h:495-503). No LS3DF FUNCTION address exists
    /// for it and the reference marks no Game.exe out-of-line copy either;
    /// it is a header inline compiled into LS3DF. The local matrix is stored
    /// verbatim, the scale is remade from its basis-row lengths, and the flag
    /// store clears WMAT_VALID/QUAT_VALID while setting UPDATE/MAT_VALID/
    /// ROT_VALID. A non-finite basis row leaves both caches unavailable
    /// instead of storing a partial scale.
    pub fn set_frame_local_matrix(
        &mut self,
        key: FrameKey,
        matrix: SMatrix,
    ) -> Result<(), FrameError> {
        let scale = matrix
            .try_get_scale()
            .map_err(|_| FrameError::NumericDomain("SetLocalMatrix GetScale"))?;
        let node = self.node_mut(key)?;
        node.state.local = matrix;
        node.state.scale = scale;
        node.state.flags = (node.state.flags & 0xffffffd7) | 0x40000014;
        Ok(())
    }
    /// Working name for the LS3DF-inlined `I3D_frame::SetPos` three-float
    /// overload (mafia-re ls3df/I3D/I3D.h:5024-5032). No LS3DF FUNCTION
    /// address exists for it; the only out-of-line copy is Game.exe 0x47a6d0,
    /// which is not claimed here. Element stores preserve signed zero exactly
    /// like the vector `assign_position` copy. Flag 0x80 (FRMFLAGS_LOCKED)
    /// skips the stores; otherwise HR_BOUND_W/WMAT are cleared and UPDATE is
    /// set.
    pub fn set_frame_position_xyz(
        &mut self,
        key: FrameKey,
        x: f32,
        y: f32,
        z: f32,
    ) -> Result<(), FrameError> {
        let s = &mut self.node_mut(key)?.state;
        if s.flags & 0x80 == 0 {
            s.local.elements[12] = x;
            s.local.elements[13] = y;
            s.local.elements[14] = z;
            s.flags = (s.flags & 0xfffffedf) | 0x40000000;
        }
        Ok(())
    }
    /// Working name for the LS3DF-inlined `I3D_visual::GetInvMatrix`
    /// (mafia-re ls3df/I3D/I3D.h:2780-2790). No LS3DF FUNCTION address exists
    /// for it and the reference marks no Game.exe out-of-line copy either.
    /// World-space visuals (VISF_WORLD_SPACE, 0x40) report the identity; the
    /// others report the affine inverse of the refreshed world matrix. The
    /// native INV_MAT_VALID cache is not modeled: recomputing returns the same
    /// matrix and the flag has no other reader, so the omission is
    /// unobservable. A singular world matrix yields the identity (the native
    /// Inverse fallback); a non-finite one poisons the world cache.
    pub fn visual_inv_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        if self.state(key)?.visual_flags & 0x40 != 0 {
            return Ok(SMatrix::identity());
        }
        let result = (|| {
            let world = self.world_matrix(key)?;
            match world.inverse() {
                Ok((inverse, _)) => Ok(inverse),
                Err(_) => Err(FrameError::NumericDomain("GetInvMatrix inverse")),
            }
        })();
        if let Err(error) = result.as_ref() {
            if let Ok(node) = self.node_mut(key) {
                node.world_error = Some(*error);
            }
        }
        result
    }
    pub(crate) fn assign_lensflare_parameter(
        &mut self,
        key: FrameKey,
        parameter: LensflareParameter,
        value: f32,
    ) -> Result<(), FrameError> {
        let state = &mut self.node_mut(key)?.state;
        if state.kind != FrameKind::Lensflare {
            return Err(FrameError::NumericDomain("lensflare owner"));
        }
        state
            .lensflare_parameters
            .as_mut()
            .ok_or(FrameError::NumericDomain("lensflare owner"))?
            .assign(parameter, value)
            .map_err(FrameError::LensflareParameterUnavailable)
    }
    /// Scale stores precede the guarded matrix-to-quaternion dependency.
    pub fn assign_scale(&mut self, key: FrameKey, scale: [f32; 3]) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.scale = scale;
        if node.state.flags & 8 == 0 {
            match SQuat::try_make_from_matrix(&node.state.local) {
                Ok(rotation) => node.state.rotation = rotation,
                Err(_) => {
                    // Preserve native stores/flags preceding the unsupported
                    // dependency, while keeping interrupted caches unavailable.
                    let error = FrameError::QuaternionFromMatrixUnavailable;
                    node.local_error = Some(error);
                    node.world_error = Some(error);
                    return Err(error);
                }
            }
        }
        node.state.flags = (node.state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }
    /// Observed animation assignment: copy, original Normalize, then invalidate.
    /// Normalize checks each reached arithmetic intermediate; later matrix
    /// operations independently check their own numeric availability.
    pub fn assign_rotation(
        &mut self,
        key: FrameKey,
        mut rotation: SQuat,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        rotation
            .try_normalize()
            .map_err(|_| FrameError::NumericDomain("Normalize"))?;
        let state = &mut self.node_mut(key)?.state;
        state.rotation = rotation;
        state.flags = (state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }
    /// Assign a quaternion already normalized by the original track evaluator.
    /// Preserve its exact bits; running Normalize again would change samples.
    /// The input has the same numeric availability guard as assign_rotation.
    pub fn apply_evaluated_rotation(
        &mut self,
        key: FrameKey,
        rotation: SQuat,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        if !rotation.is_verified_input() {
            return Err(FrameError::NumericDomain("evaluated rotation"));
        }
        let state = &mut self.node_mut(key)?.state;
        state.rotation = rotation;
        state.flags = (state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }

    /// Original SetDirWorld (RVA 0x1b040), for a frame with a parent and a
    /// roll whose absolute value is at most the native double 1e-8 threshold.
    /// The parent cache is requested before inverse/direction arithmetic. A
    /// singular parent uses the original inverse identity fallback. SetDir3
    /// retains its native partial writes on an unavailable numeric dependency;
    /// host error markers prevent those caches from appearing complete later.
    /// Parentless SetDir and roll rotations above that threshold remain unavailable.
    pub fn set_direction_world(
        &mut self,
        key: FrameKey,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        let result = (|| {
            let parent = self
                .links(key)?
                .parent
                .ok_or(FrameError::TargetDirectionUnavailable(key))?;
            let parent_world = self.world_matrix(parent)?;
            let (inverse, _) = parent_world
                .inverse()
                .map_err(|_| FrameError::NumericDomain("SetDirWorld inverse"))?;
            let direction_matrix = crate::target_direction::try_target_direction_matrix(direction)
                .map_err(|_| FrameError::NumericDomain("SetDirWorld direction"))?;
            if !roll.is_finite() || f64::from(roll.abs()) > f64::from(1e-8_f32) {
                return Err(FrameError::TargetDirectionUnavailable(key));
            }
            let relative = direction_matrix
                .try_mul_affine(&inverse)
                .map_err(|_| FrameError::NumericDomain("SetDirWorld multiply"))?;
            self.node_mut(key)?
                .state
                .local
                .try_set_dir3(
                    relative.elements[8..11].try_into().unwrap(),
                    relative.elements[4..7].try_into().unwrap(),
                )
                .map_err(|_| FrameError::DirectionFromVectorsUnavailable)?;
            let state = &mut self.node_mut(key)?.state;
            state.flags = (state.flags & 0xfffffed3) | 0x40000010;
            Ok(())
        })();
        let node = self.node_mut(key)?;
        node.local_error = result.as_ref().err().copied();
        node.world_error = result.as_ref().err().copied();
        result
    }

    /// Original SetDir (LS3DF RVA 0x1ab50): pitch/yaw decomposition of the
    /// direction plus roll as three axis-angle quaternions, stored without a
    /// further normalization (the engine's SetRot; the game's copy normalizes,
    /// and SetDir inlines the former). The quaternion core (branch cutoffs,
    /// asin/atan association, cross/dot product order) is
    /// SQuat::try_make_frame_direction under checked PC24/RN arithmetic.
    /// Ghidra body FUN_1001ab50 (1249B/32BB/341 insn, Make callees) -> tier
    /// differential. Flag store is SetRot-style: MAT_VALID cleared,
    /// QUAT_VALID set. No stores precede the construction, so an unavailable
    /// dependency only poisons both caches.
    pub fn set_direction(
        &mut self,
        key: FrameKey,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        let result = (|| {
            let rotation = SQuat::try_make_frame_direction(direction, roll)
                .map_err(|_| FrameError::NumericDomain("frame SetDir"))?;
            let state = &mut self.node_mut(key)?.state;
            state.rotation = rotation;
            state.flags = (state.flags & 0xfffffecb) | 0x40000008;
            Ok(())
        })();
        let node = self.node_mut(key)?;
        node.local_error = result.as_ref().err().copied();
        node.world_error = result.as_ref().err().copied();
        result
    }
    /// Original SetWorldPos (LS3DF RVA 0x1b160). Parentless frames use SetPos
    /// directly. Otherwise the parent world cache is requested before the
    /// inverse/point arithmetic, a singular parent uses the original inverse
    /// identity fallback, and the transformed point goes through SetPos
    /// (position lock 0x80 honored, HR_BOUND_W/WMAT cleared, UPDATE set).
    /// No Ghidra body in session -> tier contract per the 0x2b830 standard.
    /// Failures retain preceding cache work and poison both child caches, so
    /// a rejected world pose can never masquerade as a valid native cache.
    pub fn set_world_position(
        &mut self,
        key: FrameKey,
        position: [f32; 3],
    ) -> Result<(), FrameError> {
        let Some(parent) = self.links(key)?.parent else {
            return self.assign_position(key, position);
        };
        let result = (|| {
            let parent_world = self.world_matrix(parent)?;
            let (inverse, _) = parent_world
                .inverse()
                .map_err(|_| FrameError::NumericDomain("SetWorldPos inverse"))?;
            let local = inverse
                .try_transform_point(position)
                .map_err(|_| FrameError::NumericDomain("SetWorldPos transform"))?;
            self.assign_position(key, local)?;
            Ok(())
        })();
        let node = self.node_mut(key)?;
        node.local_error = result.as_ref().err().copied();
        node.world_error = result.as_ref().err().copied();
        result
    }
    /// Original SetWorldPosDir (LS3DF RVA 0x1b230). Parentless frames run
    /// SetDir then SetPos in native order; a failed direction skips the
    /// position store (clean old state + poison) because the native
    /// intermediate rotation is unrepresentable. With a parent this reuses the
    /// SetWorldDir derivation (direction matrix, finite 1e-8 roll admission,
    /// right-multiply by the parent inverse, SetDir3 of rows 2/1 with the
    /// MAT_VALID flag store) and then applies SetPos to the transformed row
    /// 3: the caller's position argument is unused, preserving the original
    /// bug (mafia-re I3d_frame.cpp:219-220). No Ghidra body in session ->
    /// tier contract per the 0x2b830 standard.
    pub fn set_world_position_direction(
        &mut self,
        key: FrameKey,
        position: [f32; 3],
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), FrameError> {
        let Some(parent) = self.links(key)?.parent else {
            self.set_direction(key, direction, roll)?;
            return self.assign_position(key, position);
        };
        let result = (|| {
            let parent_world = self.world_matrix(parent)?;
            let (inverse, _) = parent_world
                .inverse()
                .map_err(|_| FrameError::NumericDomain("SetWorldPosDir inverse"))?;
            let direction_matrix = crate::target_direction::try_target_direction_matrix(direction)
                .map_err(|_| FrameError::NumericDomain("SetWorldPosDir direction"))?;
            if !roll.is_finite() || f64::from(roll.abs()) > f64::from(1e-8_f32) {
                return Err(FrameError::TargetDirectionUnavailable(key));
            }
            let relative = direction_matrix
                .try_mul_affine(&inverse)
                .map_err(|_| FrameError::NumericDomain("SetWorldPosDir multiply"))?;
            self.node_mut(key)?
                .state
                .local
                .try_set_dir3(
                    relative.elements[8..11].try_into().unwrap(),
                    relative.elements[4..7].try_into().unwrap(),
                )
                .map_err(|_| FrameError::DirectionFromVectorsUnavailable)?;
            let state = &mut self.node_mut(key)?.state;
            state.flags = (state.flags & 0xfffffed3) | 0x40000010;
            self.assign_position(key, relative.elements[12..15].try_into().unwrap())?;
            Ok(())
        })();
        let node = self.node_mut(key)?;
        node.local_error = result.as_ref().err().copied();
        node.world_error = result.as_ref().err().copied();
        result
    }

    /// RemoveChild preserves the detached child's world cache, even if now stale.
    /// Original I3D_frame::RemoveChild, PC RVA 0x1bf20 (VA 0x1001bf20; REA
    /// formed Ghidra body FUN_1001bf20 in LS3DF.dll
    /// e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// ev_57935d4fccfcf7b53b09675a5bb59b537b6c348c286cb10304b3a113ea9;
    /// mafia-re ls3df/I3D/frames/I3d_frame.cpp:579-606 at pin 113b5b6).
    /// Confirm-only on this pre-existing port. Order-exact: a
    /// non-child returns the frame untouched; otherwise numChildren--, the
    /// three unlink arms (middle splice, first-child advance, last-child
    /// clear with count reset to zero), then prev/next/parent nulling and
    /// the parent HR-bound-validity clear (0xfffffebf). The detached child's
    /// world cache is preserved even if now stale, matching the native
    /// no-refresh unlink.
    pub fn remove_child(&mut self, parent: FrameKey, child: FrameKey) -> Result<(), FrameError> {
        self.index(parent)?;
        let l = self.links(child)?;
        if l.parent != Some(parent) {
            return Ok(());
        }
        let p = self.node_mut(parent)?;
        p.links.child_count = p.links.child_count.wrapping_sub(1);
        if let Some(prev) = l.previous {
            self.node_mut(prev)?.links.next = l.next;
            if let Some(next) = l.next {
                self.node_mut(next)?.links.previous = Some(prev);
            }
        } else if let Some(next) = l.next {
            self.node_mut(parent)?.links.child = Some(next);
            self.node_mut(next)?.links.previous = None;
        } else {
            let p = self.node_mut(parent)?;
            p.links.child = None;
            p.links.child_count = 0;
        }
        let c = self.node_mut(child)?;
        c.links.parent = None;
        c.links.previous = None;
        c.links.next = None;
        self.node_mut(parent)?.state.flags &= 0xfffffebf;
        Ok(())
    }
    /// Raw 0x1bea0 prepend (LS3DF.dll e211c850…). Upstream working name
    /// I3D_frame::AddChild (mafia-re I3d_frame.cpp:559-577), not an
    /// evidenced original symbol. Ghidra body FUN_1001bea0 (118B/21
    /// insn/3BB, flags&=0xfffffebf first) -> tier differential. Clears
    /// 0x140 on the parent first, then links (empty: 5 stores + count 1;
    /// else prepend + count++). No cycle/HR-sector/keep-world work (see
    /// link_to); returns the child key.
    pub fn add_child(&mut self, parent: FrameKey, child: FrameKey) -> Result<FrameKey, FrameError> {
        self.index(parent)?;
        self.index(child)?;
        self.node_mut(parent)?.state.flags &= 0xffff_febf;
        let first = self.links(parent)?.child;
        if let Some(first) = first {
            self.node_mut(first)?.links.previous = Some(child);
        }
        let c = self.node_mut(child)?;
        c.links.parent = Some(parent);
        c.links.previous = None;
        c.links.next = first;
        let p = self.node_mut(parent)?;
        p.links.child = Some(child);
        // Native stores 1 on empty, wrapping_add(1) otherwise.
        p.links.child_count = if first.is_none() {
            1
        } else {
            p.links.child_count.wrapping_add(1)
        };
        Ok(child)
    }
    /// 0x1bfd0 projection (LS3DF.dll e211c850…). Upstream working name
    /// I3D_frame::GetChild (mafia-re I3d_frame.cpp:610-620), not an
    /// evidenced original symbol. No Ghidra body in session -> tier
    /// contract per the 0x2b830 standard. Signed index; 0 = head
    /// (most-recently-prepended). Negative and over-range yield None
    /// (native unsigned compare + dbgPrintf/NULL path). Short lists yield
    /// None (native fallthrough to NULL). Live-borrow view, no mutation.
    pub fn child_at(&self, parent: FrameKey, index: i32) -> Result<Option<FrameKey>, FrameError> {
        let count = self.links(parent)?.child_count;
        // Native unsigned compare: (unsigned)index < count.
        if (index as u32) >= count {
            return Ok(None);
        }
        let mut at = self.links(parent)?.child;
        let mut remaining = index;
        while let Some(key) = at {
            if remaining == 0 {
                return Ok(Some(key));
            }
            remaining -= 1;
            at = self.links(key)?.next;
        }
        // Short list (corrupt count): native falls through to NULL.
        Ok(None)
    }
    /// 0x1c2a0 projection (LS3DF.dll e211c850…). Upstream working name
    /// I3D_frame::EnumFrames (mafia-re I3d_frame.cpp:700-712), not an
    /// evidenced original symbol. No Ghidra body in session -> tier
    /// contract per the 0x2b830 standard. Child-subtree preorder over
    /// hierarchy links with next-snapshot discipline (callback may
    /// unlink/delete): the snapshot is taken before visiting, deleted
    /// keys are skipped by liveness check. Self is never visited
    /// (native returns OK on childless roots). First CANCELED
    /// (0x8000000d) short-circuits and propagates; SKIPCHILDREN (1)
    /// skips descendants but continues siblings. Returns 0 (I3D_OK) or
    /// 0x8000000d (I3DERR_CANCELED).
    pub fn enum_frames(
        &mut self,
        root: FrameKey,
        mut visit: impl FnMut(FrameKey) -> Result<u32, FrameError>,
    ) -> Result<u32, FrameError> {
        const CANCELED: u32 = 0x8000_000d;
        const SKIP: u32 = 1;
        // Snapshot first: the callback may unlink or delete visited frames.
        let mut children = Vec::new();
        let mut at = self.links(root)?.child;
        while let Some(key) = at {
            children.push(key);
            at = self.links(key)?.next;
        }
        for child in children {
            // Deleted mid-enumeration: skip by liveness (native destroyed-guard).
            if self.index(child).is_err() {
                continue;
            }
            if self.enum_subtree(child, &mut visit, CANCELED, SKIP)? == CANCELED {
                return Ok(CANCELED);
            }
        }
        Ok(0)
    }
    /// EnumSubtree helper phase of 0x1c2a0 (no independent function credit):
    /// visit self, then children preorder with the same snapshot discipline.
    fn enum_subtree(
        &mut self,
        key: FrameKey,
        visit: &mut impl FnMut(FrameKey) -> Result<u32, FrameError>,
        canceled: u32,
        skip: u32,
    ) -> Result<u32, FrameError> {
        let ret = visit(key)?;
        if ret == canceled {
            return Ok(canceled);
        }
        if ret == skip {
            return Ok(0);
        }
        let mut children = Vec::new();
        let mut at = self.links(key)?.child;
        while let Some(child) = at {
            children.push(child);
            at = self.links(child)?.next;
        }
        for child in children {
            if self.index(child).is_err() {
                continue;
            }
            if self.enum_subtree(child, visit, canceled, skip)? == canceled {
                return Ok(canceled);
            }
        }
        Ok(0)
    }
    /// 0x1c5b0 common-word prefix (LS3DF.dll e211c850…). Ghidra body
    /// FUN_1001c5b0 per re-probe; full entry lives on ModelArena::
    /// duplicate_frame (tree/name/property effects). Copies local/scale/
    /// rotation words, masked flags + UPDATE, update_count+1; clears cached
    /// matrix errors. u32 widths from instructions.
    pub(crate) fn duplicate_common_fields(
        &mut self,
        dst: FrameKey,
        src: FrameKey,
    ) -> Result<(), FrameError> {
        self.index(dst)?;
        self.index(src)?;
        const DUP_MASK: u32 = 0x0fa7_3e9f;
        const KEEP: u32 = 0x400000 | 0x8000 | 0x4000;
        const UPDATE: u32 = 0x4000_0000;
        let saved = self.state(dst)?.flags;
        let (local, scale, rotation, src_flags) = {
            let s = self.state(src)?;
            (s.local, s.scale, s.rotation, s.flags)
        };
        let d = self.node_mut(dst)?;
        d.state.local = local;
        d.state.scale = scale;
        d.state.rotation = rotation;
        d.state.flags = (src_flags & DUP_MASK) | (saved & KEEP) | UPDATE;
        d.state.update_count = d.state.update_count.wrapping_add(1);
        d.local_error = None;
        d.world_error = None;
        Ok(())
    }
    /// Native LinkTo flags0: prepend, preserve local pose, invalidate this world
    /// cache. Descendant caches stay unchanged until Update. Cycles are rejected.
    pub fn link_to(&mut self, child: FrameKey, parent: Option<FrameKey>) -> Result<(), FrameError> {
        self.link_to_with_flags(child, parent, 0).map(|_| ())
    }
    /// Original LinkTo, RVA0x1b9d0, for flags0 and world-preserving flags1.
    /// The native return value is zero, including the unchanged-parent path.
    /// Flags1 computes the old world cache before unlinking, then derives the
    /// new local position, scale and direction in native write order. Availability
    /// failures retain preceding link/cache/pose writes and poison interrupted
    /// caches. Unsupported flag2/4 ownership/callback paths are explicit errors.
    pub fn link_to_with_flags(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, FrameError> {
        self.link_to_projection(child, parent, flags, false, |graph, _, old, new| {
            if graph.closest_sector(old)? == graph.closest_sector(new)? {
                Ok(())
            } else {
                Err(FrameError::UnsupportedLinkState)
            }
        })
    }
    pub(crate) fn link_to_with_scene_context(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
        transition: impl FnMut(
            &mut Self,
            FrameKey,
            Option<FrameKey>,
            Option<FrameKey>,
        ) -> Result<(), FrameError>,
    ) -> Result<u32, FrameError> {
        self.link_to_projection(child, parent, flags, true, transition)
    }
    fn link_to_projection(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
        allow_hr: bool,
        mut transition: impl FnMut(
            &mut Self,
            FrameKey,
            Option<FrameKey>,
            Option<FrameKey>,
        ) -> Result<(), FrameError>,
    ) -> Result<u32, FrameError> {
        if flags > I3DLINK_UPDMATRIX {
            return Err(FrameError::UnsupportedLinkFlags(flags));
        }
        let result = self.link_to_inner(child, parent, flags, allow_hr, &mut transition);
        if let Err(
            error @ (FrameError::NumericDomain(_)
            | FrameError::QuaternionFromMatrixUnavailable
            | FrameError::DirectionFromVectorsUnavailable),
        ) = result
        {
            let node = self.node_mut(child)?;
            node.local_error = Some(error);
            node.world_error = Some(error);
        }
        result
    }
    pub(crate) fn closest_sector(
        &self,
        mut frame: Option<FrameKey>,
    ) -> Result<Option<FrameKey>, FrameError> {
        let mut remaining = self.nodes.len();
        while let Some(key) = frame {
            if remaining == 0 {
                return Err(FrameError::Cycle);
            }
            remaining -= 1;
            if self.state(key)?.kind == FrameKind::Sector {
                return Ok(Some(key));
            }
            frame = self.links(key)?.parent;
        }
        Ok(None)
    }
    fn link_to_inner(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
        allow_hr: bool,
        transition: &mut impl FnMut(
            &mut Self,
            FrameKey,
            Option<FrameKey>,
            Option<FrameKey>,
        ) -> Result<(), FrameError>,
    ) -> Result<u32, FrameError> {
        let old = self.links(child)?.parent;
        if old == parent {
            return Ok(0);
        }
        if !allow_hr && flags == I3DLINK_UPDMATRIX && self.state(child)?.flags & 0x1c0000 != 0 {
            return Err(FrameError::UnsupportedLinkState);
        }
        let mut at = parent;
        let mut count = 0;
        while let Some(k) = at {
            if k == child {
                return Err(FrameError::Cycle);
            }
            at = self.links(k)?.parent;
            count += 1;
            if count > self.nodes.len() {
                return Err(FrameError::Cycle);
            }
        }
        if flags & I3DLINK_UPDMATRIX != 0 {
            self.world_matrix(child)?;
        }
        if let Some(old) = old {
            self.remove_child(old, child)?;
        }
        if let Some(p) = parent {
            let first = self.links(p)?.child;
            if let Some(first) = first {
                self.node_mut(first)?.links.previous = Some(child);
            }
            let c = self.node_mut(child)?;
            c.links.parent = Some(p);
            c.links.previous = None;
            c.links.next = first;
            let p = self.node_mut(p)?;
            p.links.child = Some(child);
            p.links.child_count = p.links.child_count.wrapping_add(1);
            p.state.flags &= 0xfffffebf;
        }
        if flags & I3DLINK_UPDMATRIX == 0 {
            let s = &mut self.node_mut(child)?.state;
            s.flags = (s.flags & !0x20) | 0x40000000;
        } else {
            let relative = if let Some(p) = parent {
                let parent_world = self.world_matrix(p)?;
                // Native discards Inverse's false flag and uses its identity
                // fallback for a singular parent.
                let (inverse, _) = parent_world
                    .inverse()
                    .map_err(|_| FrameError::NumericDomain("LinkTo inverse"))?;
                let child_world = self.world_matrix(child)?;
                if !child_world.is_verified_input() || !inverse.is_verified_input() {
                    return Err(FrameError::NumericDomain("LinkTo multiply"));
                }
                let relative = child_world.mul_affine(&inverse);
                if !relative.is_verified_input() {
                    return Err(FrameError::NumericDomain("LinkTo multiply"));
                }
                relative
            } else {
                self.world_matrix(child)?
            };
            self.assign_position(child, relative.elements[12..15].try_into().unwrap())?;
            let scale = relative
                .try_get_scale()
                .map_err(|_| FrameError::NumericDomain("LinkTo GetScale"))?;
            // This setter intentionally derives any missing quaternion from the
            // previous local matrix, before SetDir3 replaces the basis.
            self.assign_scale(child, scale)?;
            self.node_mut(child)?
                .state
                .local
                .try_set_dir3(
                    relative.elements[8..11].try_into().unwrap(),
                    relative.elements[4..7].try_into().unwrap(),
                )
                .map_err(|_| FrameError::DirectionFromVectorsUnavailable)?;
            let node = self.node_mut(child)?;
            node.state.flags = (node.state.flags & 0xfffffed3) | 0x40000010;
            node.local_error = None;
            node.world_error = None;
        }
        let hr = self.state(child)?.flags & 0x1c0000;
        if hr != 0 {
            let mut at = parent;
            while let Some(frame) = at {
                self.node_mut(frame)?.state.flags |= hr;
                at = self.links(frame)?.parent;
            }
            let mut at = old;
            while let Some(frame) = at {
                let mask = match self.state(frame)?.kind {
                    FrameKind::Sector => 0xffebffff,
                    FrameKind::Occluder => 0xfff3ffff,
                    _ => 0xffe3ffff,
                };
                self.node_mut(frame)?.state.flags &= mask;
                // Actual intersection of three different HR masks is zero.
                // The native loop walks siblings but never loads child flags.
                let mut child = self.links(frame)?.child;
                let mut remaining = self.nodes.len();
                while let Some(key) = child {
                    if remaining == 0 {
                        return Err(FrameError::Cycle);
                    }
                    remaining -= 1;
                    child = self.links(key)?.next;
                }
                at = self.links(frame)?.parent;
            }
            transition(self, child, old, parent)?;
        }
        Ok(0)
    }
    /// Original PC0x1a6e0. Preserve flag writes before an unavailable dependency.
    pub fn update_matrix(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let result = self.update_matrix_inner(key);
        if let Ok(node) = self.node_mut(key) {
            node.local_error = result.as_ref().err().copied();
        }
        result
    }
    fn update_matrix_inner(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let flags = self.state(key)?.flags;
        if flags & 0x10 == 0 {
            self.node_mut(key)?.state.flags |= 0x14;
            let s = *self.state(key)?;
            self.node_mut(key)?
                .state
                .local
                .try_set_rot3(s.rotation)
                .map_err(|_| FrameError::NumericDomain("SetRot3"))?;
        } else if flags & 4 == 0 {
            if flags & 8 == 0 {
                let rotation = SQuat::try_make_from_matrix(&self.state(key)?.local)
                    .map_err(|_| FrameError::QuaternionFromMatrixUnavailable)?;
                self.node_mut(key)?.state.rotation = rotation;
            }
            self.node_mut(key)?.state.flags |= 0xc;
        } else {
            if let Some(error) = self.node(key)?.local_error {
                return Err(error);
            }
            return Ok(());
        }
        for row in 0..3 {
            for col in 0..3 {
                let i = row * 4 + col;
                let s = self.state(key)?;
                self.node_mut(key)?.state.local.elements[i] =
                    checked_f32_mul(s.local.elements[i], s.scale[row])
                        .map_err(|_| FrameError::NumericDomain("local row scaling"))?;
            }
        }
        Ok(())
    }
    pub fn local_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        self.update_matrix(key)?;
        Ok(self.state(key)?.local)
    }
    /// Observed inline guard. A valid cache skips the exported procedure entirely.
    pub fn world_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        if self.state(key)?.flags & 0x20 == 0 {
            self.update_w_matrix_proc(key)?;
        } else if let Some(error) = self.node(key)?.world_error {
            return Err(error);
        }
        Ok(self.state(key)?.world)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::UpdateWMatrix` (mafia-re
    /// ls3df/I3D/I3D.h:505-510 at pin 113b5b6: refresh the world matrix unless
    /// `FRMFLAGS_WMAT_VALID` is set). The reference records no LS3DF FUNCTION
    /// address for it (header inline compiled into LS3DF callers) and marks
    /// no Game.exe out-of-line copy either. A stale cache runs the occupied
    /// `UpdateWMatrixProc` path (PC RVA 0x1c7d0) through `world_matrix`; an
    /// already valid cache moves nothing. Unknown frames fail before any
    /// cache move.
    pub fn update_w_matrix(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if frm_wmat_valid(self.state(key)?.flags) {
            return Ok(());
        }
        self.world_matrix(key)?;
        Ok(())
    }
    /// Original I3D_visual::GetRenderMatrix, PC RVA 0x2ad00 (differential:
    /// REA formed Ghidra body FUN_1002ad00 5BB/40B/15insn; mafia-re
    /// ls3df/I3D/I3D.h:2796, working name not original per the header comment
    /// "the name is a guess"). Order-exact thin wrapper: vis_flags&0x40
    /// (VISF_WORLD_SPACE) returns the identity, else the 0x20-gated
    /// UpdateWMatrixProc path (RVA 0x1c7d0) via world_matrix, returning the
    /// world cache (native this+0x10). Reuses world_matrix and the existing
    /// 0x40 world-space handling, not the billboard ProductBillMatrix camera
    /// path. Callable on any frame key in this unified graph (world_matrix
    /// itself is unguarded); native method exists only on I3D_visual. Float
    /// policy: the identity arm is bit-exact with no arithmetic; the local
    /// arm inherits the guarded PC24/RN world_matrix policy.
    pub fn visual_render_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        if self.state(key)?.visual_flags & 0x40 != 0 {
            Ok(SMatrix::identity())
        } else {
            self.world_matrix(key)
        }
    }
    /// Original export0x1c7d0. An explicit stack preserves child-local-before-parent
    /// ordering without a host recursion limit or an invented native update budget.
    pub fn update_w_matrix_proc(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let mut path = std::mem::take(&mut self.world_path);
        path.clear();
        let result = (|| {
            let mut at = key;
            loop {
                self.update_matrix(at)?;
                path.push(at);
                match self.links(at)?.parent {
                    Some(p) if self.state(p)?.flags & 0x20 == 0 => at = p,
                    _ => break,
                }
            }
            for &k in path.iter().rev() {
                let result = self.assemble_world(k);
                self.node_mut(k)?.world_error = result.as_ref().err().copied();
                result?;
            }
            Ok(())
        })();
        self.world_path = path;
        if let Ok(node) = self.node_mut(key) {
            node.world_error = result.as_ref().err().copied();
        }
        result
    }
    fn assemble_world(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let s = *self.state(key)?;
        let parent = self.links(key)?.parent;
        let world = if let Some(p) = parent {
            if let Some(error) = self.node(p)?.world_error {
                return Err(error);
            }
            let right = self.state(p)?.world;
            s.local
                .try_mul_affine(&right)
                .map_err(|_| FrameError::NumericDomain("matrix multiply"))?
        } else {
            s.local
        };
        let unavailable = |_| FrameError::NumericDomain("world scale");
        let zy = checked_f32_add(s.scale[2], s.scale[1]).map_err(unavailable)?;
        // The parent branch adds X before the matrix copy; the root copies first.
        let total = if parent.is_some() {
            Some(checked_f32_add(zy, s.scale[0]).map_err(unavailable)?)
        } else {
            None
        };
        self.node_mut(key)?.state.world = world;
        let mut scale = match total {
            Some(v) => v,
            None => checked_f32_add(zy, s.scale[0]).map_err(unavailable)?,
        };
        if let Some(p) = parent {
            scale = checked_f32_mul(scale, self.state(p)?.world_scale).map_err(unavailable)?;
        }
        scale = checked_f32_mul(scale, f32::from_bits(0x3eaaaaab)).map_err(unavailable)?;
        let s = &mut self.node_mut(key)?.state;
        s.world_scale = scale;
        s.flags |= 0x20;
        Ok(())
    }
    fn enter_update(&mut self, key: FrameKey) -> Result<Visit, FrameError> {
        let n = self.node_mut(key)?;
        if n.state.kind == FrameKind::Scene {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        // Only the owned binding projection admits the evidenced inherited
        // visual update. The legacy target-only projection remains guarded.
        if n.state.kind == FrameKind::Singlemesh && n.binding.is_none() {
            return Err(FrameError::UnsupportedSinglemeshUpdate(key));
        }
        let dirty = n.state.flags & DIRTY != 0;
        if dirty {
            n.state.update_count = n.state.update_count.wrapping_add(1);
            if matches!(
                n.state.kind,
                FrameKind::Mesh
                    | FrameKind::Projector
                    | FrameKind::Singlemesh
                    | FrameKind::Lensflare
            ) && n.state.visual_flags & 0x40 == 0
            {
                n.state.visual_flags &= !0x40000;
            }
        }
        Ok(Visit {
            key,
            child: n.links.child,
            dirty,
        })
    }
    /// Observed UpdateV effects for the represented frame kinds. Dummy's
    /// original entry is RVA0x1a330; it clears its own bound flag only if dirty.
    pub fn update_v(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if matches!(
            self.state(key)?.kind,
            FrameKind::Scene | FrameKind::Sector | FrameKind::Camera | FrameKind::Occluder
        ) {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        if let Some(error) = self.deferred_error {
            return Err(error);
        }
        if self.state(key)?.kind == FrameKind::Light {
            let light = self.light_state(key)?;
            if light.pending_sector_add
                || ((5..=7).contains(&light.light_type) && !light.sectors.is_empty())
            {
                // Linked fog UpdateV also changes the owning sectors' selected
                // fog pointers; the graph alone cannot complete that effect.
                return Err(FrameError::UnsupportedFrameUpdate(key));
            }
            return self.update_light_v(key);
        }
        if self.state(key)?.kind == FrameKind::Target {
            let flags = self.state(key)?.flags;
            let result = self
                .target_updates
                .update_v(key, flags)
                .map_err(|_| FrameError::TargetUnavailable(key));
            if let Err(error) = result {
                self.deferred_error = Some(error);
            }
            return result;
        }
        if self.state(key)?.kind == FrameKind::Singlemesh && self.node(key)?.binding.is_none() {
            return Err(FrameError::UnsupportedSinglemeshUpdate(key));
        }
        if self.state(key)?.kind == FrameKind::Joint {
            let node = self.node_mut(key)?;
            if node.state.flags & DIRTY != 0 {
                node.joint_error = None;
            }
            let target = node
                .joint
                .as_mut()
                .unwrap()
                .invalidate_for_update(node.state.flags);
            if let Some(target) = target {
                let target_node = self.node_mut(target)?;
                let singlemesh = target_node
                    .singlemesh
                    .as_mut()
                    .ok_or(FrameError::InvalidSinglemeshTarget(target))?;
                singlemesh.joints_changed = singlemesh.joints_changed.wrapping_add(1);
                target_node.state.visual_flags &= !0x60000;
                self.discard_hr_bound_vol(target)?;
                let flags = &mut self.node_mut(target)?.state.flags;
                *flags = (*flags & 0xfffffebf) | 0x40000000;
            }
            return Ok(());
        }
        let s = &mut self.node_mut(key)?.state;
        if s.flags & DIRTY != 0 {
            if matches!(
                s.kind,
                FrameKind::Mesh
                    | FrameKind::Projector
                    | FrameKind::Singlemesh
                    | FrameKind::Lensflare
            ) && s.visual_flags & 0x40 == 0
            {
                s.visual_flags &= !0x1c0000;
                if s.kind == FrameKind::Projector {
                    s.projector_cache_258 = 0;
                }
            } else if s.kind == FrameKind::Dummy {
                s.dummy_flags &= !0x10;
            }
        }
        Ok(())
    }
    /// Original dummy UpdateV, PC RVA 0x1a330 (contract: TEST
    /// [ECX+0xac],0x40002000 walk, no Ghidra body; mafia-re
    /// I3d_dummy.cpp:38-44). Clears DUMMYF_INV_MAT_VALID (0x10, I3D.h:1815)
    /// iff UPDATE|CHANGED set. Complements the generic update flag
    /// bookkeeping (which clears the same bit in its DIRTY arm) with the
    /// exact native gate.
    pub fn dummy_update_v(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.flags & 0x4000_2000 != 0 {
            self.node_mut(key)?.state.dummy_flags &= !0x10;
        }
        Ok(())
    }
    /// Cached dummy inverse matrix for I3D_dummy::GetInvMatrix (PC RVA
    /// 0x68840). Helper: no function credit. Returns the stored matrix when
    /// the lazy getter has populated it.
    pub(crate) fn dummy_inv_cached(&self, key: FrameKey) -> Result<Option<SMatrix>, FrameError> {
        Ok(self.node(key)?.dummy_inv)
    }
    /// Interrupted dummy inverse computation for I3D_dummy::GetInvMatrix.
    /// Helper: no function credit.
    pub(crate) fn dummy_inv_failure(
        &self,
        key: FrameKey,
    ) -> Result<Option<FrameError>, FrameError> {
        Ok(self.node(key)?.dummy_inv_error)
    }
    /// Store the dummy inverse outcome for I3D_dummy::GetInvMatrix. Helper:
    /// no function credit. A failure clears the matrix so a partial native
    /// store can never masquerade as a valid cache. Exactly one of the two
    /// slots is Some (exclusive outcome); the single caller upholds it.
    pub(crate) fn store_dummy_inv(
        &mut self,
        key: FrameKey,
        matrix: Option<SMatrix>,
        error: Option<FrameError>,
    ) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.dummy_inv = matrix;
        node.dummy_inv_error = error;
        Ok(())
    }
    /// Original Update/recursive-child effects for model/standard-mesh/dummy/joint
    /// objects and owned singlemesh bindings, followed by deferred Target updates;
    /// target-only singlemesh projections cannot enter their own update.
    pub fn update(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if matches!(
            self.state(key)?.kind,
            FrameKind::Camera | FrameKind::Sector | FrameKind::Occluder | FrameKind::Area
        ) {
            return Err(FrameError::UnsupportedFrameUpdate(key));
        }
        self.update_with_camera(key, |_, key| Err(FrameError::UnsupportedFrameUpdate(key)))
    }
    pub(crate) fn update_with_camera(
        &mut self,
        key: FrameKey,
        mut camera_update: impl FnMut(&mut Self, FrameKey) -> Result<(), FrameError>,
    ) -> Result<(), FrameError> {
        if let Some(error) = self.deferred_error {
            return Err(error);
        }
        let mut visits = std::mem::take(&mut self.visits);
        visits.clear();
        let result = (|| {
            visits.push(self.enter_update(key)?);
            while let Some(v) = visits.last().copied() {
                if let Some(child) = v.child {
                    if v.dirty {
                        let s = &mut self.node_mut(child)?.state;
                        s.flags = (s.flags & 0xfffffedf) | 0x2000;
                    }
                    visits.push(self.enter_update(child)?);
                } else {
                    let done = visits.pop().unwrap();
                    if let Some(p) = visits.last_mut() {
                        let linked_fog = self.state(done.key)?.kind == FrameKind::Light
                            && (5..=7).contains(&self.light_state(done.key)?.light_type)
                            && !self.light_state(done.key)?.sectors.is_empty();
                        if linked_fog
                            || matches!(
                                self.state(done.key)?.kind,
                                FrameKind::Camera
                                    | FrameKind::Sector
                                    | FrameKind::Occluder
                                    | FrameKind::Area
                            )
                        {
                            camera_update(self, done.key)?;
                        } else {
                            self.update_v(done.key)?;
                        }
                        if self.state(done.key)?.flags & DIRTY != 0 {
                            self.node_mut(done.key)?.state.flags &= !DIRTY;
                            let s = &mut self.node_mut(p.key)?.state;
                            s.flags = (s.flags & 0xfffffebf) | 0x2000;
                        }
                        p.child = self.links(done.key)?.next;
                    }
                }
            }
            let linked_fog = self.state(key)?.kind == FrameKind::Light
                && (5..=7).contains(&self.light_state(key)?.light_type)
                && !self.light_state(key)?.sectors.is_empty();
            if linked_fog
                || matches!(
                    self.state(key)?.kind,
                    FrameKind::Camera | FrameKind::Sector | FrameKind::Occluder | FrameKind::Area
                )
            {
                camera_update(self, key)?;
            } else {
                self.update_v(key)?;
            }
            if self.state(key)?.kind != FrameKind::Sector {
                self.node_mut(key)?.state.flags &= 0xfffffebf;
                let mut parent = self.links(key)?.parent;
                while let Some(p) = parent {
                    if self.state(p)?.flags & 0x40 == 0 || self.state(p)?.kind == FrameKind::Sector
                    {
                        break;
                    }
                    self.node_mut(p)?.state.flags &= 0xfffffebf;
                    parent = self.links(p)?.parent;
                }
            }
            let s = &mut self.node_mut(key)?.state;
            if matches!(
                s.kind,
                FrameKind::Mesh
                    | FrameKind::Projector
                    | FrameKind::Singlemesh
                    | FrameKind::Lensflare
            ) && s.flags & DIRTY != 0
                && s.visual_flags & 0x40 == 0
            {
                s.visual_flags &= !0x40000;
            }
            s.flags &= !DIRTY;
            self.flush_target_updates()?;
            Ok(())
        })();
        self.visits = visits;
        result
    }
    fn flush_target_updates(&mut self) -> Result<(), FrameError> {
        let Some(count) = self.target_updates.begin_flush() else {
            return Ok(());
        };
        // Moving the registry avoids cloning linked-frame allocations. Nested
        // Update calls observe the queue's reentrancy guard and skip this flush.
        let targets = std::mem::take(&mut self.targets);
        let result = (|| {
            for index in 0..count {
                let key = self.target_updates.pending()[index];
                let target = targets
                    .state(key)
                    .map_err(|_| FrameError::TargetUnavailable(key))?;
                target
                    .update_frames_with_backend(key, self)
                    .map_err(|error| match error {
                        TargetUpdateError::World(error) => error,
                        TargetUpdateError::UnsupportedTargetFlags(flags) => {
                            FrameError::UnsupportedTargetFlags(key, flags)
                        }
                        TargetUpdateError::NumericDomain => {
                            FrameError::NumericDomain("Target UpdateFrames")
                        }
                    })?;
            }
            Ok(())
        })();
        self.targets = targets;
        match result {
            Ok(()) => {
                self.target_updates.finish_flush();
                Ok(())
            }
            Err(error) => {
                self.deferred_error = Some(error);
                Err(error)
            }
        }
    }
}

impl TargetFrameBackend for I3dFrameGraph {
    type Error = FrameError;
    fn world_matrix(&mut self, frame: FrameKey) -> Result<SMatrix, Self::Error> {
        I3dFrameGraph::world_matrix(self, frame)
    }
    fn set_direction_world(
        &mut self,
        frame: FrameKey,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), Self::Error> {
        I3dFrameGraph::set_direction_world(self, frame, direction, roll)
    }
    fn update_frame(&mut self, frame: FrameKey) -> Result<(), Self::Error> {
        self.update(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_on_reports_flag_bit_zero() {
        // Working name for inlined I3D_frame::IsOn (I3D.h:543-549): no
        // LS3DF FUNCTION address exists (only Game.exe 0x47acf0, unclaimed).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert!(graph.frame_is_on(key).unwrap());
        graph.set_on(key, 0x100).unwrap();
        assert!(!graph.frame_is_on(key).unwrap());
        graph.set_on(key, 1).unwrap();
        assert!(graph.frame_is_on(key).unwrap());
        assert_eq!(
            graph.frame_is_on(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_hierarchy_and_clip_flag_readers_report_only_their_bits() {
        // Working names for inlined FRMFLAGS reads (I3D.h:271-281): no LS3DF
        // FUNCTION addresses exist for standalone readers.
        assert!(frame_hr_areas_set(0x40000));
        assert!(frame_hr_sectors_set(0x80000));
        assert!(frame_hr_occluders_set(0x100000));
        assert!(frame_is_clipped(0x200000));
        assert!(frame_in_view(0x10000));
        assert!(!frame_hr_areas_set(0));
        assert!(!frame_hr_sectors_set(0x40000));
        assert!(!frame_hr_occluders_set(0x80000));
        assert!(!frame_is_clipped(0));
        assert!(!frame_in_view(0x200000));
        assert!(frame_in_view(u32::MAX));
        assert!(!frame_hr_areas_set(u32::MAX & !0x40000));
    }
    #[test]
    fn rot_valid_reports_only_bit_two() {
        assert!(frm_rot_valid(0x04));
        assert!(frm_rot_valid(u32::MAX));
        assert!(!frm_rot_valid(0));
        assert!(!frm_rot_valid(0x08));
        assert!(!frm_rot_valid(u32::MAX & !0x04));
    }
    #[test]
    fn quat_valid_reports_only_bit_three() {
        assert!(frm_quat_valid(0x08));
        assert!(frm_quat_valid(u32::MAX));
        assert!(!frm_quat_valid(0));
        assert!(!frm_quat_valid(0x04));
        assert!(!frm_quat_valid(u32::MAX & !0x08));
    }
    #[test]
    fn mat_valid_reports_only_bit_four() {
        assert!(frm_mat_valid(0x10));
        assert!(frm_mat_valid(u32::MAX));
        assert!(!frm_mat_valid(0));
        assert!(!frm_mat_valid(0x20));
        assert!(!frm_mat_valid(u32::MAX & !0x10));
    }
    #[test]
    fn wmat_valid_reports_only_bit_five() {
        assert!(frm_wmat_valid(0x20));
        assert!(frm_wmat_valid(u32::MAX));
        assert!(!frm_wmat_valid(0));
        assert!(!frm_wmat_valid(0x10));
        assert!(!frm_wmat_valid(u32::MAX & !0x20));
    }
    #[test]
    fn hr_bound_valid_reports_only_bit_six() {
        assert!(frm_hr_bound_valid(0x40));
        assert!(frm_hr_bound_valid(u32::MAX));
        assert!(!frm_hr_bound_valid(0));
        assert!(!frm_hr_bound_valid(0x100));
        assert!(!frm_hr_bound_valid(u32::MAX & !0x40));
    }
    #[test]
    fn hr_bound_w_valid_reports_only_bit_eight() {
        assert!(frm_hr_bound_w_valid(0x100));
        assert!(frm_hr_bound_w_valid(u32::MAX));
        assert!(!frm_hr_bound_w_valid(0));
        assert!(!frm_hr_bound_w_valid(0x40));
        assert!(!frm_hr_bound_w_valid(u32::MAX & !0x100));
    }
    #[test]
    fn interpolator_reader_reports_only_bit_twenty_eight() {
        assert!(frm_has_interpolator(0x1000_0000));
        assert!(frm_has_interpolator(u32::MAX));
        assert!(!frm_has_interpolator(0));
        assert!(!frm_has_interpolator(0x4000));
        assert!(!frm_has_interpolator(u32::MAX & !0x1000_0000));
    }
    #[test]
    fn callback_bits_keep_only_the_mask() {
        assert_eq!(frm_callback_bits(0x1e00), 0x1e00);
        assert_eq!(frm_callback_bits(u32::MAX), 0x1e00);
        assert_eq!(frm_callback_bits(0), 0);
        assert_eq!(frm_callback_bits(0x100), 0);
        assert_eq!(frm_callback_bits(0x1e01), 0x1e00);
        assert_eq!(frm_callback_bits(0x2000), 0);
    }
    #[test]
    fn set_frame_local_matrix_stores_matrix_and_scale() {
        // Working name for inlined I3D_frame::SetLocalMatrix (I3D.h:495-503):
        // no LS3DF FUNCTION address and no Game.exe copy exist.
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        let mut matrix = SMatrix::identity();
        matrix.elements[0] = 2.0;
        matrix.elements[5] = 3.0;
        matrix.elements[10] = 4.0;
        matrix.elements[12] = 5.0;
        matrix.elements[13] = 6.0;
        matrix.elements[14] = 7.0;
        graph.set_frame_local_matrix(key, matrix).unwrap();
        assert_eq!(graph.local_matrix(key).unwrap(), matrix);
        assert_eq!(graph.frame_scale(key).unwrap(), [2.0, 3.0, 4.0]);
        assert_eq!(graph.frame_local_position(key).unwrap(), [5.0, 6.0, 7.0]);
        let flags = graph.state(key).unwrap().flags;
        assert_eq!(flags & 0x40000014, 0x40000014);
        assert_eq!(flags & 0x28, 0);
        assert_eq!(
            graph.set_frame_local_matrix(FrameKey(u32::MAX), matrix),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn set_frame_position_xyz_matches_vector_store_and_lock() {
        // Working name for inlined I3D_frame::SetPos xyz overload
        // (I3D.h:5024-5032): no LS3DF FUNCTION address exists (only Game.exe
        // 0x47a6d0, unclaimed here).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        graph.set_frame_position_xyz(key, 1.0, -2.0, 3.5).unwrap();
        assert_eq!(graph.frame_local_position(key).unwrap(), [1.0, -2.0, 3.5]);
        graph.node_mut(key).unwrap().state.flags |= 0x80;
        graph.set_frame_position_xyz(key, 9.0, 9.0, 9.0).unwrap();
        assert_eq!(graph.frame_local_position(key).unwrap(), [1.0, -2.0, 3.5]);
        graph.node_mut(key).unwrap().state.flags &= !0x80;
        graph
            .set_frame_position_xyz(key, f32::from_bits(0x80000000), 0.0, 0.0)
            .unwrap();
        let pos = graph.frame_local_position(key).unwrap();
        assert_eq!(pos[0].to_bits(), 0x80000000);
        assert_eq!(pos[1].to_bits(), 0.0f32.to_bits());
        assert_eq!(pos[2].to_bits(), 0.0f32.to_bits());
    }
    #[test]
    fn visual_inv_matrix_returns_identity_in_world_space() {
        // Working name for inlined I3D_visual::GetInvMatrix (I3D.h:2780-2790):
        // no LS3DF FUNCTION address and no Game.exe copy exist.
        let mut graph = I3dFrameGraph::default();
        let world_space = graph.insert(I3dFrameState::model()).unwrap();
        graph.node_mut(world_space).unwrap().state.visual_flags |= 0x40;
        assert_eq!(
            graph.visual_inv_matrix(world_space).unwrap(),
            SMatrix::identity()
        );
        let local = graph.insert(I3dFrameState::model()).unwrap();
        graph.assign_position(local, [1.0, 2.0, 3.0]).unwrap();
        let inv = graph.visual_inv_matrix(local).unwrap();
        let world = graph.world_matrix(local).unwrap();
        let round_trip = inv.try_mul_affine(&world).unwrap();
        for (got, want) in round_trip.elements.iter().zip(SMatrix::identity().elements.iter()) {
            assert!((got - want).abs() < 1e-5, "{got:?} vs {want:?}");
        }
        assert_eq!(
            graph.visual_inv_matrix(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_parent_key_follows_live_hierarchy_links() {
        // Working name for inlined I3D_frame::GetParent (I3D.h:5042-5046):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47ad10).
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::dummy()).unwrap();
        assert_eq!(graph.frame_parent_key(child).unwrap(), None);
        graph.add_child(parent, child).unwrap();
        assert_eq!(graph.frame_parent_key(child).unwrap(), Some(parent));
        graph.remove_child(parent, child).unwrap();
        assert_eq!(graph.frame_parent_key(child).unwrap(), None);
    }
    #[test]
    fn frame_child_count_tracks_prepend_links() {
        // Working name for inlined I3D_frame::NumChildren (I3D.h:602-611):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47ad20).
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_child_count(parent).unwrap(), 0);
        let first = graph.insert(I3dFrameState::dummy()).unwrap();
        let second = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.add_child(parent, first).unwrap();
        assert_eq!(graph.frame_child_count(parent).unwrap(), 1);
        graph.add_child(parent, second).unwrap();
        assert_eq!(graph.frame_child_count(parent).unwrap(), 2);
    }
    #[test]
    fn remove_child_rva_0x1bf20_unlinks_all_arms_and_clears_parent_bits() {
        // PC RVA 0x1bf20 (VA 0x1001bf20; REA formed Ghidra body FUN_1001bf20,
        // ev_57935d4f...; mafia-re I3d_frame.cpp:579-606): non-children
        // return untouched; otherwise numChildren--, the three unlink arms,
        // child link nulling and the parent 0xfffffebf clear.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let first = graph.insert(I3dFrameState::dummy()).unwrap();
        let second = graph.insert(I3dFrameState::dummy()).unwrap();
        let third = graph.insert(I3dFrameState::dummy()).unwrap();
        // Prepend order: linking first, second, third leaves [third, second,
        // first].
        graph.add_child(parent, first).unwrap();
        graph.add_child(parent, second).unwrap();
        graph.add_child(parent, third).unwrap();
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), Some(third));
        // A non-child returns the frame untouched: no count or link moves.
        graph.remove_child(parent, parent).unwrap();
        assert_eq!(graph.frame_child_count(parent).unwrap(), 3);
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), Some(third));
        // Middle splice: second drops out, neighbors relink.
        graph.remove_child(parent, second).unwrap();
        assert_eq!(graph.frame_child_count(parent).unwrap(), 2);
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), Some(third));
        assert_eq!(graph.frame_next_key(third).unwrap(), Some(first));
        assert_eq!(graph.frame_prev_key(first).unwrap(), Some(third));
        assert_eq!(graph.frame_parent_key(second).unwrap(), None);
        assert_eq!(graph.frame_prev_key(second).unwrap(), None);
        assert_eq!(graph.frame_next_key(second).unwrap(), None);
        // First-child advance.
        graph.remove_child(parent, third).unwrap();
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), Some(first));
        assert_eq!(graph.frame_child_count(parent).unwrap(), 1);
        // Last-child clear resets the count to zero, not wrapping.
        graph.remove_child(parent, first).unwrap();
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), None);
        assert_eq!(graph.frame_child_count(parent).unwrap(), 0);
        // Parent validity bits clear on every removal path: preset the
        // 0x140 word, remove again via a fresh link, and check the clear.
        graph.add_child(parent, first).unwrap();
        graph.change_flags(parent, 0, 0x140).unwrap();
        assert_eq!(graph.state(parent).unwrap().flags & 0x140, 0x140);
        graph.remove_child(parent, first).unwrap();
        assert_eq!(graph.state(parent).unwrap().flags & 0x140, 0);
    }
    #[test]
    fn frame_first_child_key_reports_prepend_head() {
        // Working name for the m_pChild read at +0x124 (I3D.h:678): no
        // LS3DF FUNCTION address exists for a standalone reader.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), None);
        let first = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.add_child(parent, first).unwrap();
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), Some(first));
        let second = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.add_child(parent, second).unwrap();
        assert_eq!(graph.frame_first_child_key(parent).unwrap(), Some(second));
        assert_eq!(
            graph.frame_first_child_key(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_kind_reports_live_kind() {
        // Working name for inlined I3D_frame::GetType (I3D.h:602-616,
        // LS3DF_EXPORTS by-value branch): no LS3DF FUNCTION address exists
        // (only Game.exe 0x47a6c0, unclaimed here).
        let mut graph = I3dFrameGraph::default();
        let model = graph.insert(I3dFrameState::model()).unwrap();
        let dummy = graph.insert(I3dFrameState::dummy()).unwrap();
        assert_eq!(graph.frame_kind(model).unwrap(), FrameKind::Model);
        assert_eq!(graph.frame_kind(dummy).unwrap(), FrameKind::Dummy);
        assert_eq!(
            graph.frame_kind(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_scale_reads_live_scale() {
        // Working name for inlined I3D_frame::GetScale (I3D.h:477-481): no
        // LS3DF FUNCTION address exists (only Game.exe 0x47aec0).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_scale(key).unwrap(), [1., 1., 1.]);
        graph.assign_scale(key, [2., 3., 4.]).unwrap();
        assert_eq!(graph.frame_scale(key).unwrap(), [2., 3., 4.]);
        assert_eq!(
            graph.frame_scale(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_local_position_reads_local_row() {
        // Working name for inlined I3D_frame::GetPos (I3D.h:460-465): no
        // LS3DF FUNCTION address exists (only Game.exe 0x47a720).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_local_position(key).unwrap(), [0., 0., 0.]);
        graph.assign_position(key, [1., 2., 3.]).unwrap();
        assert_eq!(graph.frame_local_position(key).unwrap(), [1., 2., 3.]);
    }
    #[test]
    fn frame_world_position_uses_parent_cache() {
        // Working name for inlined I3D_frame::GetWorldPos (I3D.h:551-556):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47a730).
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::model()).unwrap();
        graph.assign_position(parent, [1., 2., 3.]).unwrap();
        graph.assign_position(child, [4., 5., 6.]).unwrap();
        graph.add_child(parent, child).unwrap();
        assert_eq!(graph.frame_world_position(child).unwrap(), [5., 7., 9.]);
        assert_eq!(graph.frame_world_position(parent).unwrap(), [1., 2., 3.]);
    }
    #[test]
    fn frame_world_scale_falls_back_to_local_without_parent() {
        // Working name for inlined I3D_frame::GetWorldScale (I3D.h:519-528):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47a7b0).
        let mut graph = I3dFrameGraph::default();
        let solo = graph.insert(I3dFrameState::model()).unwrap();
        graph.assign_scale(solo, [2., 3., 4.]).unwrap();
        assert_eq!(graph.frame_world_scale(solo).unwrap(), [2., 3., 4.]);
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::model()).unwrap();
        graph.assign_scale(parent, [2., 3., 4.]).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        assert_eq!(graph.frame_world_scale(child).unwrap(), [2., 3., 4.]);
    }
    #[test]
    fn store_frame_rotation_skips_normalize_and_sets_quat_flags() {
        // LS3DF SetRot (I3D.h:420-427): the engine build stores without the
        // Normalize the game copy performs (assign_rotation covers that).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        graph
            .store_frame_rotation(key, SQuat { wxyz: [2., 0., 0., 0.] })
            .unwrap();
        assert_eq!(
            graph.state(key).unwrap().rotation.wxyz,
            [2., 0., 0., 0.]
        );
        assert_eq!(
            graph.state(key).unwrap().flags & 0x40000008,
            0x40000008
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x134, 0);
        graph
            .assign_rotation(key, SQuat { wxyz: [2., 0., 0., 0.] })
            .unwrap();
        assert_eq!(
            graph.state(key).unwrap().rotation.wxyz,
            [1., 0., 0., 0.]
        );
    }
    #[test]
    fn set_direction_from_up_writes_dir3_rows_and_mat_flags() {
        // Working name for inlined I3D_frame::SetDir1 (I3D.h:467-475): no
        // LS3DF FUNCTION address exists (only Game.exe 0x4609c0).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        graph
            .set_direction_from_up(key, [0., 0., 1.], [0., 1., 0.])
            .unwrap();
        let local = graph.state(key).unwrap().local;
        assert_eq!(
            [local.elements[8], local.elements[9], local.elements[10]],
            [0., 0., 1.]
        );
        assert_eq!(
            graph.state(key).unwrap().flags & 0x40000010,
            0x40000010
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x12c, 0);
        assert_eq!(
            graph.set_direction_from_up(key, [0., 0., 1.], [0., 0., 2.]),
            Err(FrameError::DirectionFromVectorsUnavailable)
        );
    }
    #[test]
    fn frame_local_direction_rebuilds_row_when_matrix_stale() {
        // Working name for inlined I3D_frame::GetDir (I3D.h:558-572): no
        // LS3DF FUNCTION address exists (only Game.exe 0x47a890).
        let mut graph = I3dFrameGraph::default();
        let stale = graph
            .insert(I3dFrameState::source_mesh(
                [0., 0., 0.],
                [1., 1., 1.],
                SQuat { wxyz: [1., 0., 0., 0.] },
            ))
            .unwrap();
        assert_eq!(graph.state(stale).unwrap().flags & 0x10, 0);
        assert_eq!(
            graph.frame_local_direction(stale).unwrap(),
            [0., 0., 1.]
        );
        assert_eq!(graph.state(stale).unwrap().flags & 0x10, 0x10);
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_local_direction(key).unwrap(), [0., 0., 1.]);
    }
    #[test]
    fn frame_world_direction_normalizes_world_row() {
        // Working name for inlined I3D_frame::GetWorldDir (I3D.h:573-581):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47aa40).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_world_direction(key).unwrap(), [0., 0., 1.]);
        graph.set_direction(key, [1., 0., 0.], 0.).unwrap();
        let got = graph.frame_world_direction(key).unwrap();
        for (lane, want) in got.iter().zip([1., 0., 0.]) {
            assert!((lane - want).abs() < 1e-6, "{got:?}");
        }
    }
    #[test]
    fn frame_rotation_generates_quat_once_then_reuses_cache() {
        // Working name for inlined I3D_frame::GetRot (I3D.h:5002-5009): the
        // reference notes LS3DF keeps no out-of-line copy (only Game.exe
        // 0x47a810, unclaimed here).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        graph.node_mut(key).unwrap().state.flags &= !8;
        let first = graph.frame_rotation(key).unwrap();
        assert_eq!(graph.state(key).unwrap().flags & 8, 8);
        assert_eq!(graph.state(key).unwrap().rotation.wxyz, first.wxyz);
        let second = graph.frame_rotation(key).unwrap();
        assert_eq!(second.wxyz, first.wxyz);
    }
    #[test]
    fn frame_world_rotation_derives_quat_without_storing_flags() {
        // Working name for inlined I3D_frame::GetWorldRot (I3D.h:5034-5040):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47a840).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        let before = graph.state(key).unwrap().flags;
        let rotation = graph.frame_world_rotation(key).unwrap();
        assert_eq!(rotation.wxyz, [1., 0., 0., 0.]);
        assert_eq!(graph.state(key).unwrap().flags, before);
        assert_eq!(
            graph.state(key).unwrap().rotation.wxyz,
            [1., 0., 0., 0.]
        );
    }
    #[test]
    fn visual_void_counts_are_constant_zero() {
        // Working names for I3D_void_interface::NumLevels/NumRegions
        // (I3d_visual.cpp:145-153): plain definitions with no recorded
        // LS3DF FUNCTION address, so no RVA is claimed. Next to the
        // existing SetWeight/GetWeight pair (RVAs 0x5a760/0x5a770).
        assert_eq!(visual_void_num_levels(), 0);
        assert_eq!(visual_void_num_regions(), 0);
        assert_eq!(visual_void_num_levels(), visual_void_num_regions());
    }
    #[test]
    fn joint_destruction_clears_the_indexed_slot_joint_and_preserves_other_slots() {
        let mut graph = I3dFrameGraph::default();
        let mesh = graph
            .insert_singlemesh_binding(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 7 },
                true,
            )
            .unwrap();
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        let other = graph.insert(I3dFrameState::joint()).unwrap();
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().index = 1;
        graph
            .assign_joint_singlemesh_target(joint, Some(mesh))
            .unwrap();
        graph
            .assign_joint_singlemesh_target(other, Some(mesh))
            .unwrap();
        graph
            .node_mut(mesh)
            .unwrap()
            .binding
            .as_mut()
            .unwrap()
            .joints = vec![None, Some(other)];
        graph.destroy(joint).unwrap();
        assert_eq!(graph.state(joint), Err(FrameError::UnknownFrame(joint)));
        assert_eq!(graph.joint_state(other).unwrap().singlemesh, None);
        assert_eq!(
            graph.singlemesh_binding(mesh).unwrap().joints(),
            &[None, None]
        );
        assert_eq!(
            graph
                .singlemesh_invalidation_state(mesh)
                .unwrap()
                .joints_changed,
            7
        );
        assert_eq!(
            graph.destroy(mesh),
            Err(FrameError::UnsupportedDestruction(mesh))
        );
    }

    #[test]
    fn singlemesh_teardown_clears_both_sides_of_surviving_joint_links() {
        let mut graph = I3dFrameGraph::default();
        let mut state = I3dFrameState::mesh();
        state.kind = FrameKind::Singlemesh;
        let mesh = graph
            .insert_singlemesh_binding(
                state,
                SinglemeshInvalidationState { joints_changed: 0 },
                true,
            )
            .unwrap();
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(mesh))
            .unwrap();
        graph
            .node_mut(mesh)
            .unwrap()
            .binding
            .as_mut()
            .unwrap()
            .joints = vec![Some(joint)];
        graph.check_destruction_with_owned_visual(mesh).unwrap();
        graph.destroy_derived(mesh).unwrap();
        assert_eq!(graph.joint_state(joint).unwrap().singlemesh, None);
        assert_eq!(graph.singlemesh_binding(mesh).unwrap().joints(), &[None]);
        graph.destroy_base(mesh).unwrap();
        graph.destroy(joint).unwrap();
    }

    #[test]
    fn target_destruction_guards_pending_queue_and_dangling_duplicate_links() {
        let mut graph = I3dFrameGraph::default();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph
            .register_target(
                target,
                I3dTargetState::from_parsed_fields(0, &[1, 1], &[linked]).unwrap(),
            )
            .unwrap();
        assert_eq!(
            graph.destroy(linked),
            Err(FrameError::UnsupportedDestruction(linked))
        );
        graph.destroy(target).unwrap();
        assert_eq!(graph.state(linked).unwrap().flags & 0x4000, 0);
        graph.destroy(linked).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph
            .register_target(target, I3dTargetState::default())
            .unwrap();
        graph.update_v(target).unwrap();
        assert_eq!(
            graph.destroy(target),
            Err(FrameError::UnsupportedDestruction(target))
        );
        assert_eq!(graph.target_updates.pending(), &[target]);
        assert!(graph.target_state(target).is_ok());
    }

    #[test]
    fn unimplemented_registration_cleanup_rejects_before_any_graph_mutation() {
        for flag in [0x20000000, 0x8000, 0x400000, 0x80000000] {
            let mut graph = I3dFrameGraph::default();
            let parent = graph.insert(I3dFrameState::model()).unwrap();
            let child = graph.insert(I3dFrameState::dummy()).unwrap();
            graph.link_to(child, Some(parent)).unwrap();
            graph.node_mut(child).unwrap().state.flags |= flag;
            let links = graph.links(parent).unwrap();
            assert_eq!(
                graph.destroy(child),
                Err(FrameError::UnsupportedDestructionRegistrations(child, flag))
            );
            assert_eq!(graph.links(parent).unwrap(), links);
            assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        }
    }

    #[test]
    fn target_registration_retains_links_sets_membership_and_charges_storage() {
        let mut graph = I3dFrameGraph::default();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let before = graph.storage_bytes().unwrap();
        let state = I3dTargetState::from_parsed_fields(0, &[1, 1], &[linked]).unwrap();
        graph.register_target(target, state).unwrap();
        assert_eq!(
            graph.target_state(target).unwrap().frames(),
            [linked, linked]
        );
        assert_ne!(graph.state(linked).unwrap().flags & 0x4000, 0);
        assert!(graph.storage_bytes().unwrap() > before);
        assert_eq!(
            graph.register_target(linked, I3dTargetState::default()),
            Err(FrameError::NotTarget(linked))
        );
    }

    #[test]
    fn target_zero_flags_flushes_world_after_root_dirty_clear_and_retains_storage() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.link_to(target, Some(root)).unwrap();
        graph.assign_position(target, [2., 3., 4.]).unwrap();
        graph
            .register_target(
                target,
                I3dTargetState::from_parsed_fields(0, &[1], &[linked]).unwrap(),
            )
            .unwrap();
        let before = graph.storage_bytes().unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.state(root).unwrap().flags & DIRTY, 0);
        assert_eq!(graph.state(target).unwrap().flags & DIRTY, 0);
        assert_ne!(graph.state(target).unwrap().flags & 0x20, 0);
        assert_eq!(
            graph.state(target).unwrap().world.elements[12..15],
            [2., 3., 4.]
        );
        assert!(graph.target_updates.pending().is_empty());
        assert!(!graph.target_updates.is_flushing());
        assert_eq!(graph.storage_bytes().unwrap(), before);
    }

    #[test]
    fn deferred_target_failure_preserves_forward_prefix_and_poisoned_queue() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let mut targets = Vec::new();
        for flags in [0, 2, 0] {
            let target = graph.insert(I3dFrameState::target()).unwrap();
            graph.assign_position(target, [1., 2., 3.]).unwrap();
            graph
                .register_target(
                    target,
                    I3dTargetState::from_parsed_fields(flags, &[1], &[linked]).unwrap(),
                )
                .unwrap();
            graph.update_v(target).unwrap();
            targets.push(target);
        }
        let error = FrameError::UnsupportedTargetFlags(targets[1], 2);
        assert_eq!(graph.update(root), Err(error));
        assert_eq!(graph.state(root).unwrap().flags & DIRTY, 0);
        assert_ne!(graph.state(targets[0]).unwrap().flags & 0x20, 0);
        assert_ne!(graph.state(targets[1]).unwrap().flags & 0x20, 0);
        assert_eq!(graph.state(targets[2]).unwrap().flags & 0x20, 0);
        assert!(graph.target_updates.is_flushing());
        assert_eq!(graph.target_updates.pending(), targets);
        let root_state = *graph.state(root).unwrap();
        assert_eq!(graph.update(root), Err(error));
        assert_eq!(graph.update_v(targets[2]), Err(error));
        assert_eq!(*graph.state(root).unwrap(), root_state);
        assert_eq!(graph.target_updates.pending(), targets);
    }

    #[test]
    fn target_update_v_checks_active_and_dirty_only_and_retains_duplicates() {
        let mut graph = I3dFrameGraph::default();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph
            .register_target(target, I3dTargetState::default())
            .unwrap();
        graph.node_mut(target).unwrap().state.flags = 0x40000000;
        graph.update_v(target).unwrap();
        assert!(graph.target_updates.pending().is_empty());
        graph.node_mut(target).unwrap().state.flags = 1;
        graph.update_v(target).unwrap();
        assert!(graph.target_updates.pending().is_empty());
        graph.node_mut(target).unwrap().state.flags = 0x2001;
        graph.update_v(target).unwrap();
        graph.update_v(target).unwrap();
        assert_eq!(graph.target_updates.pending(), [target, target]);
    }

    #[test]
    fn target_look_at_reentrant_update_drops_appended_queue_after_snapshot() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let linked = graph.insert(I3dFrameState::dummy()).unwrap();
        let target = graph.insert(I3dFrameState::target()).unwrap();
        graph.link_to(linked, Some(root)).unwrap();
        graph.link_to(target, Some(linked)).unwrap();
        graph.assign_position(target, [1., 0., 0.]).unwrap();
        graph
            .register_target(
                target,
                I3dTargetState::from_parsed_fields(1, &[1], &[linked]).unwrap(),
            )
            .unwrap();
        graph.update(root).unwrap();
        assert_eq!(
            graph.state(linked).unwrap().local.elements[8..11],
            [1., 0., 0.]
        );
        // Ordinary traversal and the Target callback both update this frame.
        // Its Target child appends while the deferred queue is already busy.
        assert_eq!(graph.state(linked).unwrap().update_count, 2);
        assert!(graph.target_updates.pending().is_empty());
        assert!(!graph.target_updates.is_flushing());
        assert!(graph.deferred_error.is_none());
    }

    #[test]
    fn direction_world_uses_parent_inverse_and_reports_unavailable_backends() {
        let mut graph = I3dFrameGraph::default();
        let mut parent_state = I3dFrameState::model();
        parent_state.world.elements[0] = 2.;
        parent_state.world.elements[5] = 0.5;
        let parent = graph.insert(parent_state).unwrap();
        let child = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        graph.set_direction_world(child, [1., 0., 1.], 0.).unwrap();
        let state = graph.state(child).unwrap();
        assert_eq!(state.flags & 0x4000003c, 0x40000010);
        assert!(state.local.elements[8] < state.local.elements[10]);
        let isolated = graph.insert(I3dFrameState::dummy()).unwrap();
        let error = FrameError::TargetDirectionUnavailable(isolated);
        assert_eq!(
            graph.set_direction_world(isolated, [1., 0., 0.], 0.),
            Err(error)
        );
        assert_eq!(graph.world_matrix(isolated), Err(error));
        let roll_error = FrameError::TargetDirectionUnavailable(child);
        assert_eq!(
            graph.set_direction_world(child, [1., 0., 1.], 1.),
            Err(roll_error)
        );
        assert_eq!(graph.state(child).unwrap().flags & 0x4000003c, 0x40000010);
    }

    #[test]
    fn assigned_tiny_rotation_reaches_local_matrix_without_clamping() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::dummy()).unwrap();
        let rotation = SQuat {
            wxyz: [0.5, 1e-16, 0.5, -1e-16],
        };
        graph.assign_rotation(key, rotation).unwrap();
        let assigned = graph.state(key).unwrap().rotation;
        assert!(assigned.wxyz[1] > 0. && assigned.wxyz[1] < 1e-12);
        assert!(assigned.wxyz[3] < 0. && assigned.wxyz[3] > -1e-12);
        graph.local_matrix(key).unwrap();
        let before = *graph.state(key).unwrap();
        assert_eq!(
            graph.assign_rotation(
                key,
                SQuat {
                    wxyz: [1., 1e-20, 0., 0.]
                }
            ),
            Err(FrameError::NumericDomain("Normalize"))
        );
        assert_eq!(*graph.state(key).unwrap(), before);
    }

    #[test]
    fn evaluated_rotation_preserves_sample_bits_without_second_normalization() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::dummy()).unwrap();
        let rotation = SQuat {
            wxyz: [0.5, -0.25, 0.125, -0.],
        };
        graph.node_mut(key).unwrap().state.flags = 0x1234ffff;
        graph.apply_evaluated_rotation(key, rotation).unwrap();
        assert_eq!(
            graph.state(key).unwrap().rotation.wxyz.map(f32::to_bits),
            rotation.wxyz.map(f32::to_bits)
        );
        assert_eq!(
            graph.state(key).unwrap().flags,
            (0x1234ffff & 0xfffffecb) | 0x40000008
        );
        let before = *graph.state(key).unwrap();
        assert_eq!(
            graph.apply_evaluated_rotation(
                key,
                SQuat {
                    wxyz: [f32::NAN, 0., 0., 0.]
                }
            ),
            Err(FrameError::NumericDomain("evaluated rotation"))
        );
        assert_eq!(*graph.state(key).unwrap(), before);
    }

    #[test]
    fn joint_matrix_stops_at_visual_and_reuses_cached_parent() {
        let mut graph = I3dFrameGraph::default();
        let visual = graph.insert(mesh([100., 0., 0.])).unwrap();
        let mut dummy_state = I3dFrameState::dummy();
        dummy_state.local.elements[12] = 10.;
        let dummy = graph.insert(dummy_state).unwrap();
        let parent = graph.insert(I3dFrameState::joint()).unwrap();
        let child = graph.insert(I3dFrameState::joint()).unwrap();
        graph.assign_position(parent, [2., 0., 0.]).unwrap();
        graph.assign_position(child, [3., 0., 0.]).unwrap();
        graph.link_to(dummy, Some(visual)).unwrap();
        graph.link_to(parent, Some(dummy)).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        assert_eq!(graph.get_joint_matrix(parent).unwrap().elements[12], 12.);
        graph.assign_position(dummy, [20., 0., 0.]).unwrap();
        // Native cached joint bypasses the modified ancestor and own local work.
        assert_eq!(graph.get_joint_matrix(child).unwrap().elements[12], 15.);
        assert_eq!(graph.state(visual).unwrap().flags & 0x14, 0);
        graph.update(parent).unwrap();
        assert_eq!(graph.get_joint_matrix(child).unwrap().elements[12], 25.);
        assert_eq!(graph.joint_state(parent).unwrap().flags & 0x10, 0);
    }

    #[test]
    fn joint_matrix_early_valid_flag_cannot_hide_failed_computation() {
        let mut graph = I3dFrameGraph::default();
        let mut state = I3dFrameState::joint();
        state.flags = 0;
        state.rotation.wxyz[0] = f32::NAN;
        let joint = graph.insert(state).unwrap();
        let error = FrameError::NumericDomain("SetRot3");
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        assert_eq!(graph.joint_state(joint).unwrap().flags, 0x10);
        assert_eq!(graph.state(joint).unwrap().flags, 0x14);
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        graph
            .assign_rotation(
                joint,
                SQuat {
                    wxyz: [1., 0., 0., 0.],
                },
            )
            .unwrap();
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        graph.update_v(joint).unwrap();
        assert_eq!(graph.get_joint_matrix(joint), Ok(SMatrix::identity()));

        let mut state = I3dFrameState::dummy();
        state.flags = 0x14;
        state.local.elements[0] = f32::MAX;
        let dummy = graph.insert(state).unwrap();
        graph.link_to(joint, Some(dummy)).unwrap();
        graph.assign_scale(joint, [2., 1., 1.]).unwrap();
        graph.update_v(joint).unwrap();
        let error = FrameError::NumericDomain("GetJointMatrix multiply");
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
        assert_eq!(
            graph
                .joint_state(joint)
                .unwrap()
                .joint_matrix
                .unwrap()
                .elements[0],
            2.
        );
        assert_eq!(graph.get_joint_matrix(joint), Err(error));
    }

    #[test]
    fn joint_hot_cache_copies_raw_bits_and_missing_storage_is_unavailable() {
        let mut graph = I3dFrameGraph::default();
        let mut cache = SMatrix::identity();
        cache.elements[0] = f32::from_bits(0x7fc01234);
        cache.elements[12] = -0.;
        let state = I3dJointState {
            flags: 0x10,
            joint_matrix: Some(cache),
            ..Default::default()
        };
        let joint = graph
            .insert_joint_state(I3dFrameState::joint(), state)
            .unwrap();
        let before = *graph.state(joint).unwrap();
        assert_eq!(
            graph
                .get_joint_matrix(joint)
                .unwrap()
                .elements
                .map(f32::to_bits),
            cache.elements.map(f32::to_bits)
        );
        assert_eq!(*graph.state(joint).unwrap(), before);
        let missing = graph
            .insert_joint_state(
                I3dFrameState::joint(),
                I3dJointState {
                    flags: 0x10,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            graph.get_joint_matrix(missing),
            Err(FrameError::JointMatrixUnavailable(missing))
        );
    }

    #[test]
    fn binding_uses_preorder_first_match_and_keeps_bounds_pending() {
        let mut graph = I3dFrameGraph::default();
        let target = graph
            .insert_singlemesh_binding(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 7 },
                true,
            )
            .unwrap();
        let first = graph.insert(I3dFrameState::joint()).unwrap();
        let nested = graph.insert(mesh([0.; 3])).unwrap();
        let second = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .apply_joint_load_image_fields(second, SMatrix::identity(), 1)
            .unwrap();
        graph.link_to(nested, Some(target)).unwrap();
        graph.link_to(first, Some(target)).unwrap();
        graph.link_to(second, Some(nested)).unwrap();
        assert_eq!(graph.find_joint(target).unwrap(), vec![first, second]);
        let before = *graph.state(target).unwrap();
        assert_eq!(
            graph.d_build_binding(target),
            Ok(DBuildBindingOutcome::BoundVolumeUpdateRequired)
        );
        assert_eq!(
            graph.singlemesh_binding(target).unwrap().joints(),
            &[Some(first), Some(second)]
        );
        assert_eq!(*graph.state(target).unwrap(), before);
        assert_eq!(graph.joint_state(first).unwrap().singlemesh, Some(target));
        graph.remove_child(nested, second).unwrap();
        graph.d_build_binding(target).unwrap();
        assert_eq!(graph.joint_state(second).unwrap().singlemesh, None);

        let duplicate = graph.insert(I3dFrameState::joint()).unwrap();
        graph.link_to(duplicate, Some(target)).unwrap();
        graph.d_build_binding(target).unwrap();
        assert_eq!(
            graph.singlemesh_binding(target).unwrap().joints(),
            &[Some(duplicate), None]
        );
        assert_eq!(graph.joint_state(first).unwrap().singlemesh, None);
        assert_eq!(
            graph.d_build_binding(target),
            Err(FrameError::SinglemeshBindingUnavailable(target))
        );
        graph.update(target).unwrap();
    }

    #[test]
    fn binding_empty_paths_and_search_limit_are_explicit() {
        let mut graph = I3dFrameGraph::default();
        for (present, expected) in [
            (false, DBuildBindingOutcome::NoMesh),
            (true, DBuildBindingOutcome::NoJoints),
        ] {
            let key = graph
                .insert_singlemesh_binding(
                    I3dFrameState::mesh(),
                    SinglemeshInvalidationState { joints_changed: 0 },
                    present,
                )
                .unwrap();
            assert_eq!(graph.d_build_binding(key), Ok(expected));
        }
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut expected = Vec::new();
        for _ in 0..80 {
            let joint = graph.insert(I3dFrameState::joint()).unwrap();
            graph.link_to(joint, Some(root)).unwrap();
            expected.insert(0, joint);
        }
        expected.truncate(64);
        assert_eq!(graph.find_joint(root).unwrap(), expected);
        let bytes = graph.storage_bytes().unwrap();
        graph.find_joint(root).unwrap();
        assert_eq!(graph.storage_bytes().unwrap(), bytes);
    }

    #[test]
    fn loaded_d_build_eagerly_caches_bounds_before_palette_work() {
        let mut graph = I3dFrameGraph::default();
        let target = graph
            .insert_singlemesh_binding(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 23 },
                true,
            )
            .unwrap();
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            (-400_i32) as u32
        );
        let parent = graph.insert(I3dFrameState::joint()).unwrap();
        let child = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .apply_joint_load_image_fields(child, SMatrix::identity(), 1)
            .unwrap();
        graph.assign_position(parent, [2., 0., 0.]).unwrap();
        graph.assign_position(child, [3., 0., 0.]).unwrap();
        graph.link_to(parent, Some(target)).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        let bbox = SinglemeshBoundingBox {
            min: [-1.; 3],
            max: [1.; 3],
        };
        assert!(graph
            .d_build_loaded_singlemesh(target, SinglemeshBoundingBox::EMPTY, &[bbox; 2])
            .unwrap());
        assert_eq!(graph.joint_state(parent).unwrap().flags & 0x10, 0x10);
        assert_eq!(graph.joint_state(child).unwrap().flags & 0x10, 0x10);
        let volume = graph.singlemesh_bound_volume(target).unwrap();
        assert_eq!(volume.bbox.min, [1., -1., -1.]);
        assert_eq!(volume.bbox.max, [6., 1., 1.]);
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            23
        );
        assert_eq!(graph.state(target).unwrap().visual_flags & 0x60000, 0x20000);
        // The cached wrapper does not read a skin definition or rebuild links.
        graph
            .update_singlemesh_loaded_bounds_cached(target, bbox, &[])
            .unwrap();
        assert_eq!(graph.singlemesh_bound_volume(target).unwrap(), volume);
        graph.update_v(child).unwrap();
        assert_eq!(
            graph.singlemesh_bound_volume(target),
            Err(FrameError::SinglemeshBoundsUnavailable(target))
        );
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            23
        );
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            24
        );
        // Counter copy and prior cache effects persist if the next definition
        // cannot provide every record. A stale volume is no longer exposed.
        assert_eq!(
            graph.update_singlemesh_loaded_bounds_cached(target, bbox, &[]),
            Err(FrameError::SinglemeshBoundsDefinitionTooShort(target))
        );
        assert_eq!(
            graph
                .singlemesh_binding(target)
                .unwrap()
                .bvol_joints_changed(),
            24
        );
        assert_eq!(
            graph.singlemesh_bound_volume(target),
            Err(FrameError::SinglemeshBoundsDefinitionTooShort(target))
        );
        // The native counter write preceded failure. Host admission must not
        // turn that equal counter into a successfully published stale volume.
        assert_eq!(
            graph.update_singlemesh_loaded_bounds_cached(target, bbox, &[bbox; 2]),
            Err(FrameError::SinglemeshBoundsDefinitionTooShort(target))
        );
        graph
            .update_singlemesh_loaded_bounds(target, SinglemeshBoundingBox::EMPTY, &[bbox; 2])
            .unwrap();
        assert_eq!(graph.singlemesh_bound_volume(target).unwrap(), volume);
    }
    #[test]
    fn joint_update_invalidates_linked_singlemesh_and_only_cached_ancestors() {
        let mut graph = I3dFrameGraph::default();
        let far = graph.insert(I3dFrameState::model()).unwrap();
        let stop = graph.insert(I3dFrameState::dummy()).unwrap();
        let near = graph.insert(I3dFrameState::dummy()).unwrap();
        let target = graph
            .insert_singlemesh_invalidation_target(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState {
                    joints_changed: u32::MAX,
                },
            )
            .unwrap();
        graph.link_to(stop, Some(far)).unwrap();
        graph.link_to(near, Some(stop)).unwrap();
        graph.link_to(target, Some(near)).unwrap();
        for key in [far, near, target] {
            graph.node_mut(key).unwrap().state.flags = 0x175;
        }
        graph.node_mut(stop).unwrap().state.flags = 0x135;
        graph.node_mut(target).unwrap().state.visual_flags = 0xffffffff;
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(target))
            .unwrap();
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().flags = u32::MAX;
        let own_frame_flags = graph.state(joint).unwrap().flags;
        graph.update_v(joint).unwrap();
        assert_eq!(graph.joint_state(joint).unwrap().flags, 0xffffffe3);
        assert_eq!(graph.state(joint).unwrap().flags, own_frame_flags);
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            0
        );
        assert_eq!(graph.state(target).unwrap().visual_flags, 0xfff9ffff);
        assert_eq!(graph.state(target).unwrap().flags, 0x40000035);
        assert_eq!(graph.state(near).unwrap().flags, 0x35);
        assert_eq!(graph.state(stop).unwrap().flags, 0x135);
        assert_eq!(graph.state(far).unwrap().flags, 0x175);
        assert_eq!(
            graph.update_v(target),
            Err(FrameError::UnsupportedSinglemeshUpdate(target))
        );
        assert_eq!(
            graph.update(target),
            Err(FrameError::UnsupportedSinglemeshUpdate(target))
        );
    }

    #[test]
    fn inherited_joint_change_does_not_invalidate_singlemesh_vertices() {
        let mut graph = I3dFrameGraph::default();
        let target = graph
            .insert_singlemesh_invalidation_target(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 7 },
            )
            .unwrap();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph.link_to(joint, Some(parent)).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(target))
            .unwrap();
        graph.node_mut(joint).unwrap().state.flags = 0x3d;
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().flags = 0x1f;
        let target_before = *graph.state(target).unwrap();
        graph.update(parent).unwrap();
        assert_eq!(graph.joint_state(joint).unwrap().flags, 3);
        assert_eq!(graph.state(joint).unwrap().flags & DIRTY, 0);
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            7
        );
        assert_eq!(*graph.state(target).unwrap(), target_before);
        assert_eq!(
            graph.assign_joint_singlemesh_target(joint, Some(parent)),
            Err(FrameError::InvalidSinglemeshTarget(parent))
        );
        assert_eq!(graph.joint_state(joint).unwrap().singlemesh, Some(target));
    }

    fn mesh(position: [f32; 3]) -> I3dFrameState {
        I3dFrameState::source_mesh(
            position,
            [1.; 3],
            SQuat {
                wxyz: [1., 0., 0., 0.],
            },
        )
    }
    #[test]
    fn row_scaling_retains_earlier_stores_before_an_unavailable_product() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([0.; 3]);
        state.flags = 0x18;
        state.local.elements[0] = 2.;
        state.local.elements[1] = f32::MIN_POSITIVE;
        state.scale = [0.5, 1., 1.];
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("local row scaling"))
        );
        assert_eq!(graph.state(key).unwrap().local.elements[0], 1.);
        assert_eq!(
            graph.state(key).unwrap().local.elements[1],
            f32::MIN_POSITIVE
        );
        assert_eq!(graph.state(key).unwrap().flags, 0x1c);
        assert_eq!(
            graph.world_matrix(key),
            Err(FrameError::NumericDomain("local row scaling"))
        );
        assert_eq!(graph.state(key).unwrap().local.elements[0], 1.);
    }
    #[test]
    fn small_srt_and_quaternion_spills_are_available_but_overflow_is_guarded() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut state = mesh([1e-20, 0., 0.]);
        state.scale = [1e-20; 3];
        let child = graph.insert(state).unwrap();
        graph.link_to(child, Some(root)).unwrap();
        let world = graph.world_matrix(child).unwrap();
        assert_eq!(world.elements[0], 1e-20);
        assert_eq!(world.elements[12], 1e-20);
        let mut state = mesh([0.; 3]);
        state.rotation.wxyz = [0.5, 1e-23, 0., 0.];
        let tiny = graph.insert(state).unwrap();
        let matrix = graph.local_matrix(tiny).unwrap();
        assert_eq!(matrix.elements[6], -1e-23);
        assert_eq!(matrix.elements[9], 1e-23);
        state.rotation.wxyz = [0.5, f32::MAX, 0., 0.];
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        assert_eq!(graph.state(key).unwrap().local, state.local);
        assert_eq!(graph.state(key).unwrap().flags, state.flags | 0x14);
        assert_eq!(
            graph.world_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
    }
    #[test]
    fn common_image_fields_are_raw_stores_after_linking() {
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::mesh()).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        graph.node_mut(child).unwrap().state.flags |= 0x20000080;
        let position = [f32::from_bits(0x80000000), f32::from_bits(0x7fc01234), 7.];
        let rotation = SQuat {
            wxyz: [2., -3., 4., -0.],
        };
        graph
            .apply_common_load_image_fields(child, position, [-2., 0., 3.], rotation, 0x80)
            .unwrap();
        let state = graph.state(child).unwrap();
        assert_eq!(
            state.local.elements[12..15]
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            position.map(f32::to_bits)
        );
        assert_eq!(state.scale, [-2., 0., 3.]);
        assert_eq!(
            state.rotation.wxyz.map(f32::to_bits),
            rotation.wxyz.map(f32::to_bits)
        );
        assert_eq!(state.flags, 0x60000088);
        assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        assert_eq!(state.world, SMatrix::identity());
    }
    #[test]
    fn dummy_virtual_update_has_its_own_flags_and_clean_update_discards_bounds() {
        let mut graph = I3dFrameGraph::default();
        let mut state = I3dFrameState::dummy();
        state.flags = 0x140;
        state.dummy_flags = 0x31;
        state.visual_flags = u32::MAX;
        let key = graph.insert(state).unwrap();
        graph.update_v(key).unwrap();
        assert_eq!(graph.state(key).unwrap().dummy_flags, 0x31);
        graph.update(key).unwrap();
        assert_eq!(graph.state(key).unwrap().flags, 0);
        assert_eq!(graph.state(key).unwrap().update_count, 0);
        graph.mark_loaded_root(key).unwrap();
        assert_eq!(graph.state(key).unwrap().dummy_flags, 0x21);
        assert_eq!(graph.state(key).unwrap().visual_flags, u32::MAX);
        assert_eq!(graph.state(key).unwrap().update_count, 1);
        assert_eq!(graph.state(key).unwrap().flags, 0);
    }
    #[test]
    fn bound_invalidation_stops_at_first_clean_ancestor() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let middle = graph.insert(I3dFrameState::dummy()).unwrap();
        let leaf = graph.insert(I3dFrameState::mesh()).unwrap();
        graph.link_to(middle, Some(root)).unwrap();
        graph.link_to(leaf, Some(middle)).unwrap();
        graph.node_mut(root).unwrap().state.flags = 0x140;
        graph.node_mut(middle).unwrap().state.flags = 0x100;
        graph.node_mut(leaf).unwrap().state.flags = 0x140;
        graph.discard_hr_bound_vol(leaf).unwrap();
        assert_eq!(graph.state(leaf).unwrap().flags, 0);
        assert_eq!(graph.state(middle).unwrap().flags, 0x100);
        assert_eq!(graph.state(root).unwrap().flags, 0x140);
        graph.node_mut(middle).unwrap().state.flags |= 0x40;
        graph.discard_hr_bound_vol(leaf).unwrap();
        assert_eq!(graph.state(middle).unwrap().flags, 0);
        assert_eq!(graph.state(root).unwrap().flags, 0);
    }
    #[test]
    fn world_preserving_reparent_detach_and_locked_position_follow_native_rules() {
        for locked in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let a = graph.insert(mesh([10., 0., 0.])).unwrap();
            let b = graph.insert(mesh([20., 0., 0.])).unwrap();
            let mut state = mesh([3., 0., 0.]);
            if locked {
                state.flags |= 0x80;
            }
            let child = graph.insert(state).unwrap();
            graph.link_to(child, Some(a)).unwrap();
            assert_eq!(graph.world_matrix(child).unwrap().elements[12], 13.);
            assert_eq!(graph.link_to_with_flags(child, Some(b), 1), Ok(0));
            assert_eq!(graph.links(child).unwrap().parent, Some(b));
            assert_eq!(
                graph.state(child).unwrap().local.elements[12],
                if locked { 3. } else { -7. }
            );
            assert_eq!(
                graph.world_matrix(child).unwrap().elements[12],
                if locked { 23. } else { 13. }
            );
            graph.link_to_with_flags(child, None, 1).unwrap();
            assert_eq!(
                graph.world_matrix(child).unwrap().elements[12],
                if locked { 3. } else { 13. }
            );
        }
    }
    #[test]
    fn relink_errors_preserve_native_prefix_writes_and_mark_caches_unavailable() {
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(mesh([10., 0., 0.])).unwrap();
        let mut state = mesh([3., 0., 0.]);
        state.flags = 0x30;
        state.world.elements[12] = 7.;
        state.local.elements[1] = f32::NAN;
        let child = graph.insert(state).unwrap();
        assert_eq!(
            graph.link_to_with_flags(child, Some(parent), 2),
            Err(FrameError::UnsupportedLinkFlags(2))
        );
        assert_eq!(graph.links(child).unwrap().parent, None);
        assert_eq!(
            graph.link_to_with_flags(child, Some(parent), 1),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        assert_eq!(graph.state(child).unwrap().local.elements[12], -3.);
        assert_eq!(graph.state(child).unwrap().scale, [1.; 3]);
        assert_eq!(graph.state(child).unwrap().flags, 0x40000010);
        assert_eq!(
            graph.world_matrix(child),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
    }
    #[test]
    fn singular_parent_uses_native_inverse_identity_fallback() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([10., 0., 0.]);
        state.flags = 0x1c;
        for i in [0, 5, 10] {
            state.local.elements[i] = 0.;
        }
        let parent = graph.insert(state).unwrap();
        let child = graph.insert(mesh([3., 0., 0.])).unwrap();
        assert_eq!(graph.link_to_with_flags(child, Some(parent), 1), Ok(0));
        assert_eq!(graph.state(child).unwrap().local.elements[12], 3.);
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 10.);
    }
    #[test]
    fn parent_changes_stay_cached_until_update_then_repeated_reads_are_clean() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(mesh([1., 0., 0.])).unwrap();
        graph.link_to(child, Some(root)).unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 1.);
        graph.assign_position(root, [10., 0., 0.]).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 1.);
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 11.);
        assert_eq!(
            (
                graph.state(root).unwrap().update_count,
                graph.state(child).unwrap().update_count
            ),
            (2, 2)
        );
        let state = *graph.state(child).unwrap();
        graph.world_matrix(child).unwrap();
        assert_eq!(*graph.state(child).unwrap(), state);
    }
    #[test]
    fn native_head_order_and_remove_child_do_not_invent_matrix_invalidation() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let a = graph.insert(mesh([1., 0., 0.])).unwrap();
        let b = graph.insert(mesh([2., 0., 0.])).unwrap();
        graph.link_to(a, Some(root)).unwrap();
        graph.link_to(b, Some(root)).unwrap();
        assert_eq!(graph.links(root).unwrap().child, Some(b));
        assert_eq!(graph.links(b).unwrap().next, Some(a));
        assert_eq!(graph.links(a).unwrap().previous, Some(b));
        graph.assign_position(root, [10., 0., 0.]).unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        graph.remove_child(root, a).unwrap();
        assert_eq!(graph.links(a).unwrap().parent, None);
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        graph.update_w_matrix_proc(a).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 1.);
        graph.link_to(a, Some(root)).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        let root_links = graph.links(root).unwrap();
        assert_eq!(graph.link_to(root, Some(a)), Err(FrameError::Cycle));
        assert_eq!(graph.links(root).unwrap(), root_links);
    }
    #[test]
    fn equal_and_signed_zero_position_writes_invalidate_but_lock_suppresses() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(mesh([0.; 3])).unwrap();
        graph.world_matrix(key).unwrap();
        graph.assign_position(key, [-0., 0., -0.]).unwrap();
        assert_eq!(
            graph.state(key).unwrap().local.elements[12].to_bits(),
            0x80000000
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x20, 0);
        assert_ne!(graph.state(key).unwrap().flags & 0x40000000, 0);
        let mut state = mesh([1., 2., 3.]);
        state.flags |= 0x80;
        let locked = graph.insert(state).unwrap();
        graph.assign_position(locked, [9.; 3]).unwrap();
        assert_eq!(*graph.state(locked).unwrap(), state);
    }
    #[test]
    fn admitted_matrix_rotation_is_stored_before_cache_flags_and_scaling() {
        for setter in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let mut state = mesh([10., 20., 30.]);
            state.flags = 0x10;
            state.local.elements[0] = 2.;
            state.local.elements[5] = 0.5;
            state.local.elements[10] = -4.;
            state.rotation.wxyz = [0., 1., 0., 0.];
            state.scale = [3., 4., 5.];
            let key = graph.insert(state).unwrap();
            if setter {
                graph.assign_scale(key, [3., 4., 5.]).unwrap();
                assert_eq!(graph.state(key).unwrap().flags, 0x40000008);
                assert_eq!(graph.state(key).unwrap().local, state.local);
            }
            graph.update_matrix(key).unwrap();
            let got = *graph.state(key).unwrap();
            assert_eq!(
                got.rotation.wxyz.map(f32::to_bits),
                [1.0f32.to_bits(), 0, 0, 0]
            );
            let expected_diagonal = if setter { [3., 4., 5.] } else { [6., 2., -20.] };
            assert_eq!(
                [
                    got.local.elements[0],
                    got.local.elements[5],
                    got.local.elements[10]
                ],
                expected_diagonal
            );
            assert_eq!(&got.local.elements[12..15], &[10., 20., 30.]);
            assert_eq!(got.flags, if setter { 0x4000001c } else { 0x1c });
            graph.update_matrix(key).unwrap();
            assert_eq!(*graph.state(key).unwrap(), got);
        }
    }

    #[test]
    fn missing_axis_quaternion_reaches_frame_scale_and_world_cache_consumers() {
        for setter in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let mut state = mesh([10., 20., 30.]);
            state.flags = 0x10;
            state.local.elements[0] = 0.;
            state.rotation.wxyz = [0., 1., 0., 0.];
            state.scale = [2., 3., 4.];
            let key = graph.insert(state).unwrap();
            if setter {
                graph.assign_scale(key, [2., 3., 4.]).unwrap();
                assert_eq!(graph.state(key).unwrap().flags, 0x40000008);
            }
            let local = graph.local_matrix(key).unwrap();
            assert_eq!(
                graph.state(key).unwrap().rotation.wxyz.map(f32::to_bits),
                [1f32.to_bits(), 0, 0, 0]
            );
            assert_eq!(
                [local.elements[0], local.elements[5], local.elements[10]],
                if setter { [2., 3., 4.] } else { [0., 3., 4.] }
            );
            assert_eq!(&local.elements[12..15], &[10., 20., 30.]);
            assert_eq!(graph.world_matrix(key).unwrap(), local);
            let stable = *graph.state(key).unwrap();
            assert_eq!(graph.world_matrix(key).unwrap(), local);
            assert_eq!(*graph.state(key).unwrap(), stable);
        }
    }

    #[test]
    fn numeric_matrix_quaternion_failure_keeps_prior_scale_store_and_flag_order() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([0.; 3]);
        state.flags = 0x10;
        state.local.elements[0] = f32::from_bits(1);
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.assign_scale(key, [2., 3., 4.]),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
        assert_eq!(graph.state(key).unwrap().flags, 0x10);
        assert_eq!(
            graph.update_matrix(key),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.state(key).unwrap().flags, 0x10);
        for flags in [0x30, 0x34] {
            let mut cached = mesh([0.; 3]);
            cached.flags = flags;
            cached.local.elements[0] = f32::from_bits(1);
            let key = graph.insert(cached).unwrap();
            assert!(graph.world_matrix(key).is_ok());
            assert_eq!(
                graph.assign_scale(key, [2., 3., 4.]),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            assert_eq!(graph.state(key).unwrap().flags, flags);
            assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
            assert_eq!(
                graph.world_matrix(key),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            assert_eq!(
                graph.local_matrix(key),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            graph
                .assign_rotation(
                    key,
                    SQuat {
                        wxyz: [1., 0., 0., 0.],
                    },
                )
                .unwrap();
            assert!(graph.world_matrix(key).is_ok());
        }
        let mut bad = mesh([0.; 3]);
        bad.rotation.wxyz[0] = f32::NAN;
        let key = graph.insert(bad).unwrap();
        assert_eq!(
            graph.update_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x14, 0x14);
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        graph
            .assign_rotation(
                key,
                SQuat {
                    wxyz: [1., 0., 0., 0.],
                },
            )
            .unwrap();
        assert!(graph.local_matrix(key).is_ok());
        let mut state = I3dFrameState::model();
        state.scale = [f32::MAX; 3];
        let huge = graph.insert(state).unwrap();
        assert!(graph.update_w_matrix_proc(huge).is_err());
        assert!(graph.world_matrix(huge).is_err());
        graph.assign_scale(huge, [1.; 3]).unwrap();
        assert!(graph.world_matrix(huge).is_ok());
    }
    #[test]
    fn nonunit_source_quaternion_and_world_scale_follow_native_order() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([1., 2., 3.]);
        state.rotation.wxyz = [1.25, 2., 3., 4.];
        state.scale = [100_000_000., -100_000_000., 1.];
        state.local.elements[3] = 7.;
        let key = graph.insert(state).unwrap();
        graph.update_w_matrix_proc(key).unwrap();
        let got = graph.state(key).unwrap();
        assert_eq!(got.local.elements[0], 100_000_000.);
        assert_eq!(got.local.elements[5], -100_000_000.);
        assert_eq!(got.local.elements[10], 1.);
        assert_eq!(got.local.elements[3], 7.);
        assert_eq!(got.world.elements[12..15], [1., 2., 3.]);
        assert_eq!(got.world_scale.to_bits(), 0);
    }
    #[test]
    fn deep_valid_tree_uses_precharged_traversal_storage_without_recursion_budget() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut last = root;
        for _ in 0..512 {
            let key = graph.insert(mesh([1., 0., 0.])).unwrap();
            graph.link_to(key, Some(last)).unwrap();
            last = key;
        }
        let bytes = graph.storage_bytes().unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(last).unwrap().elements[12], 512.);
        assert_eq!(graph.storage_bytes().unwrap(), bytes);
    }
    #[test]
    fn ctor_word_matches_1a7f0() {
        // RVA 0x1a7f0: UPDATE|WMAT|MAT|QUAT|ROT|ON + identity pose + count 0.
        assert_eq!(I3dFrameState::ctor_flags(), 0x4000_003d);
        for s in [
            I3dFrameState::model(),
            I3dFrameState::dummy(),
            I3dFrameState::joint(),
            I3dFrameState::target(),
        ] {
            assert_eq!(s.flags, 0x4000_003d);
            assert_eq!(s.scale, [1.0, 1.0, 1.0]);
            assert_eq!(s.world_scale, 1.0);
            assert_eq!(s.update_count, 0);
            assert_eq!(s.render_number, u32::MAX);
            assert_eq!(s.local, SMatrix::identity());
            assert_eq!(s.world, SMatrix::identity());
        }
    }
    #[test]
    fn ctor_residue_is_poison_free() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.links(key).unwrap(), FrameLinks::default());
        assert!(graph.local_matrix(key).is_ok());
        assert!(graph.world_matrix(key).is_ok());
    }
    #[test]
    fn add_child_prepends_and_clears_parent_valid_bits() {
        // RVA 0x1bea0: 0x140 cleared first, empty stores 1, prepend + count++.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let first = graph.insert(I3dFrameState::dummy()).unwrap();
        let second = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.node_mut(parent).unwrap().state.flags |= 0x140;
        assert_eq!(graph.add_child(parent, first).unwrap(), first);
        assert_eq!(graph.links(parent).unwrap().child, Some(first));
        assert_eq!(graph.links(parent).unwrap().child_count, 1);
        assert_eq!(graph.add_child(parent, second).unwrap(), second);
        // Prepend: second is head, first follows.
        assert_eq!(graph.links(parent).unwrap().child, Some(second));
        assert_eq!(graph.links(second).unwrap().next, Some(first));
        assert_eq!(graph.links(first).unwrap().previous, Some(second));
        assert_eq!(graph.links(first).unwrap().next, None);
        assert_eq!(graph.links(parent).unwrap().child_count, 2);
        assert_eq!(graph.state(parent).unwrap().flags & 0x140, 0);
        // Child flags untouched; return identity.
        assert_eq!(graph.state(second).unwrap().flags, 0x4000_003d);
        assert!(graph.add_child(parent, FrameKey(999)).is_err());
        // Rejected intake leaves prior links intact.
        assert_eq!(graph.links(parent).unwrap().child_count, 2);
    }
    #[test]
    fn child_at_signed_bounds_and_head_order() {
        // RVA 0x1bfd0: signed index, 0 = head; negative/over-range -> None.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let a = graph.insert(I3dFrameState::dummy()).unwrap();
        let b = graph.insert(I3dFrameState::dummy()).unwrap();
        let c = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.add_child(parent, a).unwrap();
        graph.add_child(parent, b).unwrap();
        graph.add_child(parent, c).unwrap();
        assert_eq!(graph.child_at(parent, 0).unwrap(), Some(c));
        assert_eq!(graph.child_at(parent, 1).unwrap(), Some(b));
        assert_eq!(graph.child_at(parent, 2).unwrap(), Some(a));
        assert_eq!(graph.child_at(parent, 3).unwrap(), None);
        assert_eq!(graph.child_at(parent, -1).unwrap(), None);
        assert_eq!(graph.child_at(parent, i32::MAX).unwrap(), None);
    }
    #[test]
    fn enum_frames_preorder_cancel_skip_and_order() {
        // RVA 0x1c2a0: child-subtree preorder, self never visited, CANCEL wins.
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        // Empty root: OK with zero visits.
        let mut visits = Vec::new();
        assert_eq!(
            graph
                .enum_frames(root, |k| {
                    visits.push(k);
                    Ok(0)
                })
                .unwrap(),
            0
        );
        assert!(visits.is_empty());
        // Tree: root -> [c2(head), c1]; c1 -> [g0].
        let c0 = graph.insert(I3dFrameState::dummy()).unwrap();
        let c1 = graph.insert(I3dFrameState::dummy()).unwrap();
        let c2 = graph.insert(I3dFrameState::dummy()).unwrap();
        let g0 = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.add_child(root, c0).unwrap();
        graph.add_child(root, c1).unwrap();
        graph.add_child(root, c2).unwrap();
        graph.add_child(c1, g0).unwrap();
        let mut visits = Vec::new();
        assert_eq!(
            graph
                .enum_frames(root, |k| {
                    visits.push(k);
                    Ok(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(visits, vec![c2, c1, g0, c0]);
        // CANCEL at c1 stops the walk with the CANCELED word.
        let mut visits = Vec::new();
        assert_eq!(
            graph
                .enum_frames(root, |k| {
                    visits.push(k);
                    if k == c1 {
                        return Ok(0x8000_000d);
                    }
                    Ok(0)
                })
                .unwrap(),
            0x8000_000d
        );
        assert_eq!(visits, vec![c2, c1]);
        // SKIPCHILDREN skips descendants but continues siblings.
        let mut visits = Vec::new();
        assert_eq!(
            graph
                .enum_frames(root, |k| {
                    visits.push(k);
                    if k == c1 {
                        return Ok(1);
                    }
                    Ok(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(visits, vec![c2, c1, c0]);
        // Delete-during-visit is structural, not exercised: the children Vec
        // is collected before visiting and dead keys are skipped by liveness
        // check, but the FnMut(FrameKey) visitor cannot mutate the graph, so
        // the liveness skip is a defensive projection of the native
        // destroyed-guard, not an executed path here.
        let mut visits = Vec::new();
        assert_eq!(
            graph
                .enum_frames(root, |k| {
                    visits.push(k);
                    Ok(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(visits.len(), 4);
        // Traversal leaves caches usable.
        assert!(graph.local_matrix(root).is_ok());
    }
    #[test]
    fn set_direction_stores_bit_exact_product_with_set_rot_flags() {
        // RVA 0x1ab50: engine-SetRot store, no second normalization.
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.set_direction(key, [1., 0., 0.], 0.).unwrap();
        assert_eq!(
            graph.state(key).unwrap().rotation.wxyz.map(f32::to_bits),
            [0x3f3504f4, 0, 0xbf3504f4, 0]
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x4000003c, 0x40000008);
        // Unavailable construction retains old stores and poisons both caches.
        let before = *graph.state(key).unwrap();
        let error = FrameError::NumericDomain("frame SetDir");
        assert_eq!(graph.set_direction(key, [f32::NAN, 0., 0.], 0.), Err(error));
        assert_eq!(graph.state(key).unwrap().rotation, before.rotation);
        assert_eq!(graph.state(key).unwrap().flags, before.flags);
        // Errors persist on the node until a successful recompute heals them.
        assert_eq!(graph.node(key).unwrap().local_error, Some(error));
        assert_eq!(graph.node(key).unwrap().world_error, Some(error));
    }
    #[test]
    fn set_world_position_splits_parent_inverse_and_honors_lock() {
        // RVA 0x1b160: parentless direct, parented inverse-point, lock 0x80.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.assign_position(parent, [10., 0., 0.]).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        graph.set_world_position(child, [14., 1., 2.]).unwrap();
        assert_eq!(
            graph.state(child).unwrap().local.elements[12..15],
            [4., 1., 2.]
        );
        assert_eq!(graph.state(child).unwrap().flags & 0x40000120, 0x40000000);
        // Position lock suppresses the store but still succeeds.
        graph.node_mut(child).unwrap().state.flags |= 0x80;
        graph.set_world_position(child, [0., 0., 0.]).unwrap();
        assert_eq!(
            graph.state(child).unwrap().local.elements[12..15],
            [4., 1., 2.]
        );
        // Parentless frames copy through SetPos, preserving signed zero.
        let solo = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.set_world_position(solo, [-0., 1., 2.]).unwrap();
        let t = &graph.state(solo).unwrap().local.elements[12..15];
        assert_eq!(
            [t[0].to_bits(), t[1].to_bits(), t[2].to_bits()],
            [-0_f32, 1., 2.].map(f32::to_bits)
        );
        // A singular parent uses the identity fallback: local equals world.
        let singular = graph.insert(I3dFrameState::model()).unwrap();
        let leaf = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.assign_scale(singular, [0., 0., 0.]).unwrap();
        graph.link_to(leaf, Some(singular)).unwrap();
        graph.set_world_position(leaf, [5., 6., 7.]).unwrap();
        assert_eq!(
            graph.state(leaf).unwrap().local.elements[12..15],
            [5., 6., 7.]
        );
        // A rejected transform retains stores and poisons both caches.
        let error = FrameError::NumericDomain("SetWorldPos transform");
        assert_eq!(
            graph.set_world_position(leaf, [f32::NAN, 0., 0.]),
            Err(error)
        );
        assert_eq!(
            graph.state(leaf).unwrap().local.elements[12..15],
            [5., 6., 7.]
        );
        assert_eq!(graph.node(leaf).unwrap().local_error, Some(error));
        assert_eq!(graph.node(leaf).unwrap().world_error, Some(error));
    }
    #[test]
    fn set_world_position_direction_ignores_caller_position_under_parent() {
        // RVA 0x1b230: caller position dead under a parent (original bug).
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let first = graph.insert(I3dFrameState::dummy()).unwrap();
        let second = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.link_to(first, Some(parent)).unwrap();
        graph.link_to(second, Some(parent)).unwrap();
        graph.set_direction_world(first, [1., 0., 1.], 0.).unwrap();
        graph
            .set_world_position_direction(second, [9., 9., 9.], [1., 0., 1.], 0.)
            .unwrap();
        assert_eq!(
            graph.state(second).unwrap().local.elements,
            graph.state(first).unwrap().local.elements
        );
        assert_eq!(graph.state(second).unwrap().flags & 0x4000003c, 0x40000010);
        // Translated parent: caller position stays dead.
        graph.assign_position(parent, [10., 0., 0.]).unwrap();
        let moved = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.link_to(moved, Some(parent)).unwrap();
        graph
            .set_world_position_direction(moved, [9., 9., 9.], [0., 0., 1.], 0.)
            .unwrap();
        assert_eq!(
            graph.state(moved).unwrap().local.elements[12..15],
            [-10., 0., 0.]
        );
        assert_eq!(graph.state(moved).unwrap().local.elements[10], 1.);
        assert_eq!(graph.state(moved).unwrap().local.elements[5], 1.);
        // Above-threshold roll is unavailable and poisons both caches.
        let error = FrameError::TargetDirectionUnavailable(moved);
        assert_eq!(
            graph.set_world_position_direction(moved, [0., 0., 0.], [0., 0., 1.], 1.),
            Err(error)
        );
        assert_eq!(graph.node(moved).unwrap().local_error, Some(error));
        assert_eq!(graph.node(moved).unwrap().world_error, Some(error));
        // Parentless order is SetDir then SetPos, both effective.
        let solo = graph.insert(I3dFrameState::dummy()).unwrap();
        graph
            .set_world_position_direction(solo, [1., 2., 3.], [1., 0., 0.], 0.)
            .unwrap();
        assert_eq!(
            graph.state(solo).unwrap().rotation.wxyz.map(f32::to_bits),
            [0x3f3504f4, 0, 0xbf3504f4, 0]
        );
        assert_eq!(
            graph.state(solo).unwrap().local.elements[12..15],
            [1., 2., 3.]
        );
        assert_eq!(graph.state(solo).unwrap().flags & 0x4000003c, 0x40000008);
        // Parentless direction failure skips SetPos and poisons both caches.
        let bad = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.assign_position(bad, [7., 8., 9.]).unwrap();
        let dir_error = FrameError::NumericDomain("frame SetDir");
        assert_eq!(
            graph.set_world_position_direction(bad, [1., 2., 3.], [f32::NAN, 0., 0.], 0.),
            Err(dir_error)
        );
        assert_eq!(
            graph.state(bad).unwrap().local.elements[12..15],
            [7., 8., 9.]
        );
        assert_eq!(graph.node(bad).unwrap().local_error, Some(dir_error));
        assert_eq!(graph.node(bad).unwrap().world_error, Some(dir_error));
    }

    #[test]
    fn visual_base_word_is_2200_not_2a00() {
        assert_eq!(I3dFrameGraph::visual_base_state().visual_flags, 0x2200);
        assert_eq!(I3dFrameState::mesh().visual_flags, 0x2a00);
        assert_eq!(I3dFrameState::lensflare().visual_flags, 0x2a00);
    }
    #[test]
    fn shadow_projector_mode_roundtrips() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        for mode in [0, 1, 2] {
            assert!(graph.set_visual_shadow_mode(key, mode).unwrap());
            assert_eq!(
                graph.visual_shadow_mode(key).unwrap(),
                ShadowMode::from_i32(mode).unwrap()
            );
            assert!(graph.set_visual_projector_mode(key, mode).unwrap());
            assert_eq!(
                graph.visual_projector_mode(key).unwrap(),
                ShadowMode::from_i32(mode).unwrap()
            );
        }
        // Out-of-range modes are ignored with no flag effects (residue).
        let before = graph.state(key).unwrap().visual_flags;
        for mode in [3, -1, i32::MAX, i32::MIN] {
            assert!(!graph.set_visual_shadow_mode(key, mode).unwrap());
            assert!(!graph.set_visual_projector_mode(key, mode).unwrap());
        }
        assert_eq!(graph.state(key).unwrap().visual_flags, before);
        // Mask isolation: shadow writes never touch projector bits and back.
        graph.set_visual_shadow_mode(key, 2).unwrap();
        graph.set_visual_projector_mode(key, 0).unwrap();
        assert_eq!(graph.state(key).unwrap().visual_flags & 0x600, 0x600);
        assert_eq!(graph.state(key).unwrap().visual_flags & 0x6000, 0);
    }
    struct SpaceLog {
        calls: Vec<String>,
    }
    impl VisualSpaceBackend for SpaceLog {
        type Error = FrameError;
        fn visual_type_label(&mut self, visual_type: u32) -> Result<String, Self::Error> {
            self.calls.push(format!("type:{visual_type:#x}"));
            Ok(format!("vt{visual_type:#x}"))
        }
        fn frame_name(&mut self) -> Result<Option<String>, Self::Error> {
            self.calls.push("name".to_string());
            Ok(Some("frame".to_string()))
        }
        fn debug_print_unsupported_set_space(
            &mut self,
            type_label: &str,
            frame_name: Option<&str>,
        ) -> Result<(), Self::Error> {
            self.calls
                .push(format!("print:{type_label}:{}", frame_name.unwrap_or("?")));
            Ok(())
        }
    }
    #[test]
    fn set_visual_space_no_change_has_no_backend_effects() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        let mut log = SpaceLog { calls: Vec::new() };
        // Local space already: both no-change arms are backend-silent.
        assert!(!graph
            .set_visual_space(key, false, 0x5f4a424f, &mut log)
            .unwrap());
        assert!(log.calls.is_empty());
        graph.node_mut(key).unwrap().state.visual_flags |= 0x40;
        assert!(!graph
            .set_visual_space(key, true, 0x5f4a424f, &mut log)
            .unwrap());
        assert!(log.calls.is_empty());
        // Change arm calls type -> name -> print in order, flags untouched.
        let before = graph.state(key).unwrap().visual_flags;
        assert!(graph
            .set_visual_space(key, false, 0x5f4a424f, &mut log)
            .unwrap());
        assert_eq!(
            log.calls,
            ["type:0x5f4a424f", "name", "print:vt0x5f4a424f:frame"]
        );
        assert_eq!(graph.state(key).unwrap().visual_flags, before);
    }
    #[test]
    fn void_weight_words() {
        for (region, level) in [(0, 0), (1, 2), (-1, -1), (i32::MAX, i32::MIN)] {
            assert_eq!(
                visual_void_set_weight(region, level, 0.5),
                0x8000_0005u32 as i32
            );
            assert_eq!(
                visual_void_set_weight(region, level, f32::NAN),
                0x8000_0005u32 as i32
            );
            assert_eq!(visual_void_get_weight(region, level).to_bits(), 0x0000_0000);
        }
    }
    #[test]
    fn base_set_on_uses_only_low_byte_and_preserves_caches() {
        // PC RVA 0x1b400 (mafia-re I3d_frame.cpp:238): low byte gates bit0.
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::dummy()).unwrap();
        let before = *graph.state(key).unwrap();
        for on in [0u32, 0x100, 0x101, 0xffff_ff00, 0x80] {
            graph.set_on(key, on).unwrap();
            let mut expected = before;
            expected.flags = (expected.flags & !1) | u32::from(on as u8 != 0);
            assert_eq!(*graph.state(key).unwrap(), expected);
        }
        // Unknown frames fail without side effects on survivors.
        assert_eq!(
            graph.set_on(FrameKey(u32::MAX), 1),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
        assert_eq!(*graph.state(key).unwrap(), before);
    }
    #[test]
    fn base_destructor_retires_clean_frames_and_rejects_registrations() {
        // PC RVA 0x1a9a0 (mafia-re I3d_frame.cpp:67): represented tail.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        graph.destroy_frame(child).unwrap();
        assert_eq!(graph.state(child), Err(FrameError::UnknownFrame(child)));
        assert_eq!(graph.links(parent).unwrap().child, None);
        // Weather registration stays rejected until the derived step clears it.
        let weather = graph.insert(I3dFrameState::dummy()).unwrap();
        graph.change_flags(weather, 0, 0x8000_0000).unwrap();
        assert_eq!(
            graph.destroy_frame(weather),
            Err(FrameError::UnsupportedDestructionRegistrations(
                weather,
                0x8000_0000
            ))
        );
        graph.change_flags(weather, 0x8000_0000, 0).unwrap();
        graph.destroy_frame(weather).unwrap();
        assert_eq!(graph.state(weather), Err(FrameError::UnknownFrame(weather)));
        // Joint slot clears with the retiring joint.
        let mesh = graph
            .insert_singlemesh_binding(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 0 },
                true,
            )
            .unwrap();
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(mesh))
            .unwrap();
        graph
            .node_mut(mesh)
            .unwrap()
            .binding
            .as_mut()
            .unwrap()
            .joints = vec![Some(joint)];
        graph.destroy_frame(joint).unwrap();
        assert_eq!(graph.state(joint), Err(FrameError::UnknownFrame(joint)));
        assert_eq!(graph.singlemesh_binding(mesh).unwrap().joints(), &[None]);
        graph.destroy_frame(mesh).unwrap_err();
    }
    #[test]
    fn visual_render_matrix_selects_identity_or_gated_world() {
        // PC RVA 0x2ad00 (mafia-re ls3df/I3D/I3D.h:2796, working name per the
        // header comment): vis_flags&0x40 returns identity, else the 0x20-gated
        // world cache. Float policy: identity arm has no arithmetic; local arm
        // inherits the guarded PC24/RN world_matrix policy.
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::mesh()).unwrap();
        graph.assign_position(key, [3., 4., 5.]).unwrap();
        let world = graph.world_matrix(key).unwrap();
        assert_eq!(graph.visual_render_matrix(key).unwrap(), world);
        assert_eq!(
            graph.visual_render_matrix(key).unwrap().elements[12..15],
            [3., 4., 5.]
        );
        graph.assign_position(key, [7., 0., 0.]).unwrap();
        assert_eq!(graph.state(key).unwrap().flags & 0x20, 0);
        assert_eq!(graph.visual_render_matrix(key).unwrap().elements[12], 7.);
        graph.node_mut(key).unwrap().state.visual_flags |= 0x40;
        graph.assign_position(key, [9., 9., 9.]).unwrap();
        let before = *graph.state(key).unwrap();
        assert_eq!(
            graph.visual_render_matrix(key).unwrap(),
            SMatrix::identity()
        );
        assert_eq!(*graph.state(key).unwrap(), before);
        assert_eq!(
            graph.visual_render_matrix(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
        // Poisoned world errors propagate instead of returning a matrix.
        graph.node_mut(key).unwrap().state.visual_flags &= !0x40;
        graph.assign_scale(key, [f32::NAN, 1., 1.]).unwrap();
        assert!(matches!(
            graph.visual_render_matrix(key),
            Err(FrameError::NumericDomain(_))
        ));
    }
    #[test]
    fn frame_state_words_report_live_ctor_values() {
        // Working names for m_FrmFlags/m_UpdateCount/m_RenderNumber reads
        // (I3D.h:666,675,683): no LS3DF FUNCTION addresses exist.
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_flags_word(key).unwrap(), 0x4000003d);
        assert_eq!(graph.frame_update_count(key).unwrap(), 0);
        assert_eq!(graph.frame_render_number(key).unwrap(), u32::MAX);
        assert_eq!(
            graph.frame_flags_word(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
        assert_eq!(
            graph.frame_update_count(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
        assert_eq!(
            graph.frame_render_number(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_visual_flags_word_reports_live_word() {
        // Working name for the visual-flags whole-word read: no LS3DF
        // FUNCTION address exists (only masked mode readers).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::mesh()).unwrap();
        assert_eq!(graph.frame_visual_flags_word(key).unwrap(), 0x2a00);
        graph.set_visual_shadow_mode(key, 2).unwrap();
        assert_ne!(graph.frame_visual_flags_word(key).unwrap(), 0x2a00);
        assert_eq!(
            graph.frame_visual_flags_word(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_sibling_keys_follow_live_links() {
        // Working names for m_pPrev/m_pNext reads (I3D.h:679): no LS3DF
        // FUNCTION addresses exist.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let first = graph.insert(I3dFrameState::dummy()).unwrap();
        let second = graph.insert(I3dFrameState::dummy()).unwrap();
        assert_eq!(graph.frame_prev_key(first).unwrap(), None);
        assert_eq!(graph.frame_next_key(first).unwrap(), None);
        graph.add_child(parent, first).unwrap();
        graph.add_child(parent, second).unwrap();
        assert_eq!(graph.frame_next_key(second).unwrap(), Some(first));
        assert_eq!(graph.frame_prev_key(first).unwrap(), Some(second));
        assert_eq!(graph.frame_prev_key(second).unwrap(), None);
        assert_eq!(
            graph.frame_prev_key(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
        assert_eq!(
            graph.frame_next_key(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_world_scale_scalar_reports_stored_average() {
        // Working name for scalar m_WorldScale (I3D.h:674): no LS3DF
        // FUNCTION address exists (only the GetWorldScale vector).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        assert_eq!(graph.frame_world_scale_scalar(key).unwrap(), 1.0);
        graph.node_mut(key).unwrap().state.world_scale = 2.5;
        assert_eq!(graph.frame_world_scale_scalar(key).unwrap(), 2.5);
        assert_eq!(
            graph.frame_world_scale_scalar(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_flags_after_set_rot_clears_caches_and_sets_quat_update() {
        assert_eq!(frame_flags_after_set_rot(0), 0x4000_0000 | 0x08);
        assert_eq!(
            frame_flags_after_set_rot(0xffff_ffff),
            (0xffff_ffff & !(0x100 | 0x20 | 0x10 | 0x04)) | 0x4000_0000 | 0x08
        );
        assert_eq!(frame_flags_after_set_rot(0x100 | 0x20 | 0x10 | 0x04), 0x4000_0000 | 0x08);
    }
    #[test]
    fn frame_flags_after_set_pos_locks_and_clears_world_caches() {
        assert_eq!(frame_flags_after_set_pos(0x80), None);
        assert_eq!(frame_flags_after_set_pos(0x80 | 0x1234), None);
        assert_eq!(frame_flags_after_set_pos(0), Some(0x4000_0000));
        assert_eq!(
            frame_flags_after_set_pos(0x100 | 0x20 | 0x5555),
            Some((0x5555 & !(0x100 | 0x20)) | 0x4000_0000)
        );
    }
    #[test]
    fn frame_flags_after_set_local_matrix_sets_matched_caches() {
        assert_eq!(frame_flags_after_set_local_matrix(0), 0x4000_0014);
        assert_eq!(
            frame_flags_after_set_local_matrix(0x20 | 0x08 | 0x7777),
            (0x7777 & !(0x20 | 0x08)) | 0x4000_0000 | 0x10 | 0x04
        );
    }
    #[test]
    fn link_keep_world_first_and_old_chain_projection() {
        // Working names for the LinkTo keep-world first store
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:491-492) and the
        // old-chain hierarchy clearing (505-528): the body lives at the
        // occupied PC RVA 0x1b9d0, so no RVA is claimed here.
        assert_eq!(frame_flags_after_link_keep_world_first(0), 0x4000_0008);
        assert_eq!(frame_flags_after_link_keep_world_first(0x134 | 0x51), 0x4000_0049);
        assert_eq!(
            frame_flags_after_link_keep_world_first(0xffff_ffff),
            (0xffff_ffff & !0x134) | 0x4000_0000 | 0x08
        );
        assert_eq!(link_old_parent_kept_hr_bits(true, false, false), 0x8_0000);
        assert_eq!(link_old_parent_kept_hr_bits(false, true, false), 0x10_0000);
        assert_eq!(link_old_parent_kept_hr_bits(false, false, true), 0x4_0000);
        assert_eq!(link_old_parent_kept_hr_bits(false, false, false), 0);
        // Mixed inputs are outside the native domain (one frame has one
        // type); the chain reports sector first, host-defined.
        assert_eq!(link_old_parent_kept_hr_bits(true, true, true), 0x8_0000);
        assert_eq!(link_old_parent_kept_hr_bits(false, true, true), 0x10_0000);
    }
    #[test]
    fn update_gates_clear_only_local_space_visual_bound() {
        // Working names for the Update (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:907-915) and UpdateChildren
        // (893-897) VISF_WBOUND_VALID gates: occupied bodies 0x1ca60 and
        // 0x1c9a0, so no RVA is claimed here.
        assert!(frame_update_clears_visual_bound(true, false, 0x4000_0000));
        assert!(frame_update_clears_visual_bound(true, false, 0x2000));
        assert!(!frame_update_clears_visual_bound(true, true, 0x4000_0000));
        assert!(!frame_update_clears_visual_bound(false, false, 0x4000_0000));
        assert!(!frame_update_clears_visual_bound(true, false, 0));
        assert!(!frame_update_clears_visual_bound(true, false, 0x1));
        assert!(frame_update_children_clears_visual_bound(true, false, 0x4000_0000));
        assert!(frame_update_children_clears_visual_bound(true, false, 0x2000));
        assert!(!frame_update_children_clears_visual_bound(true, true, 0x4000_0000));
        assert!(!frame_update_children_clears_visual_bound(false, false, 0x4000_0000));
        assert!(!frame_update_children_clears_visual_bound(true, false, 0));
    }
    #[test]
    fn rot_and_dir_refresh_gates_follow_valid_bits() {
        // Working names for the inlined GetRot (mafia-re ls3df/I3D/I3D.h:5002-5009)
        // and GetDir (558-572) refresh branches: no LS3DF FUNCTION entries
        // exist (only Game.exe 0x47a810/0x47a890), so no RVA is claimed here.
        assert!(frame_rot_needs_make(0));
        assert!(!frame_rot_needs_make(0x08));
        assert!(frame_rot_needs_make(0xffff_ffff & !0x08));
        assert!(frame_dir_rebuilds_matrix(0));
        assert!(!frame_dir_rebuilds_matrix(0x10));
        assert!(frame_dir_rebuilds_matrix(0xffff_ffff & !0x10));
    }
    #[test]
    fn hr_bound_sector_and_refresh_gates() {
        // Working names for the UpdateHRBoundVol sector arm (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:954-957), the UpdateHRBoundVolW
        // gate (1010-1011) and the DiscardHRBoundVol sector early-out
        // (932-934): occupied bodies 0x1cc50/0x1cf90/0x1cc00, unclaimed.
        assert_eq!(frame_flags_after_update_hr_bound_vol_sector(0), 0x40);
        assert_eq!(frame_flags_after_update_hr_bound_vol_sector(0x1100), 0x1140);
        assert_eq!(frame_flags_after_update_hr_bound_vol_sector(0xffff_ffff), 0xffff_ffff);
        assert!(frame_hr_bound_w_needs_local_refresh(0));
        assert!(!frame_hr_bound_w_needs_local_refresh(0x40));
        assert!(frame_hr_bound_w_needs_local_refresh(0xffff_ffff & !0x40));
        assert!(frame_discard_bound_skips_sector(true));
        assert!(!frame_discard_bound_skips_sector(false));
    }
    #[test]
    fn frame_live_position_scale_parent_and_child_count_readers() {
        // Working names for the inlined GetPos (mafia-re ls3df/I3D/I3D.h:460-465),
        // GetWorldPos (552-556), GetScale (477-481), GetParent (5043-5046)
        // and NumChildren (619-623) member reads: no LS3DF FUNCTION entries
        // exist (only Game.exe 0x47a720/0x47a730/0x47aec0/0x47ad10/0x47ad20),
        // so no RVA is claimed here.
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::model()).unwrap();
        graph.node_mut(parent).unwrap().state.local.elements[12..15]
            .copy_from_slice(&[1., 2., 3.]);
        graph.node_mut(parent).unwrap().state.scale = [2., 3., 4.];
        assert_eq!(graph.frame_local_pos_of(parent).unwrap(), [1., 2., 3.]);
        assert_eq!(graph.frame_world_pos_of(parent).unwrap(), [0., 0., 0.]);
        assert_eq!(graph.frame_scale_of(parent).unwrap(), [2., 3., 4.]);
        assert_eq!(graph.frame_parent_of(child).unwrap(), None);
        assert_eq!(graph.frame_num_children_of(parent).unwrap(), 0);
        graph.add_child(parent, child).unwrap();
        assert_eq!(graph.frame_parent_of(child).unwrap(), Some(parent));
        assert_eq!(graph.frame_num_children_of(parent).unwrap(), 1);
        // The world reader follows the refreshed world cache, not the local row.
        graph.node_mut(child).unwrap().state.world.elements[12..15]
            .copy_from_slice(&[7., 8., 9.]);
        assert_eq!(graph.frame_world_pos_of(child).unwrap(), [7., 8., 9.]);
        assert_eq!(
            graph.frame_local_pos_of(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
        assert_eq!(
            graph.frame_num_children_of(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_flags_after_set_dir1_clears_rotation_caches() {
        assert_eq!(frame_flags_after_set_dir1(0), 0x4000_0000 | 0x10);
        assert_eq!(
            frame_flags_after_set_dir1(0x100 | 0x20 | 0x08 | 0x04 | 0x3333),
            (0x3333 & !(0x100 | 0x20 | 0x08 | 0x04)) | 0x4000_0000 | 0x10
        );
    }
    #[test]
    fn frame_flags_after_set_scale_clears_matrix_caches() {
        // Working name for inlined I3D_frame::SetScale (I3D.h:483-493):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47a750).
        assert_eq!(frame_flags_after_set_scale(0), 0x4000_0000 | 0x08);
        assert_eq!(
            frame_flags_after_set_scale(0x100 | 0x20 | 0x10 | 0x04 | 0x3333),
            (0x3333 & !(0x100 | 0x20 | 0x10 | 0x04)) | 0x4000_0000 | 0x08
        );
    }
    #[test]
    fn store_frame_local_matrix_remakes_scale_and_matched_caches() {
        // Working name for inlined I3D_frame::SetLocalMatrix (I3D.h:495-503):
        // no LS3DF FUNCTION address exists in either binary.
        use crate::native_transform::SMatrix;
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        let matrix = SMatrix {
            elements: [
                2., 0., 0., 0., //
                0., 3., 0., 0., //
                0., 0., 4., 0., //
                5., 6., 7., 1.,
            ],
        };
        graph.store_frame_local_matrix(key, matrix).unwrap();
        assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
        assert_eq!(
            graph.state(key).unwrap().local.elements[12..15],
            [5., 6., 7.]
        );
        assert_eq!(graph.state(key).unwrap().flags, 0x4000_0015);
        assert!(graph.store_frame_local_matrix(FrameKey(u32::MAX), matrix).is_err());
        let bad = SMatrix {
            elements: [
                f32::NAN, 0., 0., 0., //
                0., 1., 0., 0., //
                0., 0., 1., 0., //
                0., 0., 0., 1.,
            ],
        };
        assert!(graph.store_frame_local_matrix(key, bad).is_err());
        assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
        assert_eq!(graph.state(key).unwrap().flags, 0x4000_0015);
    }
    #[test]
    fn frame_flags_after_update_matrix_picks_the_stale_branch() {
        assert_eq!(frame_flags_after_update_matrix(0), 0x10 | 0x04);
        assert_eq!(frame_flags_after_update_matrix(0x10), 0x10 | 0x04 | 0x08);
        assert_eq!(frame_flags_after_update_matrix(0x10 | 0x04), 0x10 | 0x04);
        assert_eq!(
            frame_flags_after_update_matrix(0x10 | 0x04 | 0x08 | 0x4000_0000),
            0x10 | 0x04 | 0x08 | 0x4000_0000
        );
    }
    #[test]
    fn frame_locked_bit_matches_setpos_gate() {
        assert!(frame_is_locked(0x80));
        assert!(frame_is_locked(0xffff_ffff));
        assert!(!frame_is_locked(0));
        assert!(!frame_is_locked(0x7f));
        assert_eq!(frame_flags_after_set_pos(0x80), None);
    }
    #[test]
    fn frame_tick_bit_marks_scene_tick_list() {
        assert!(frame_is_in_tick_list(0x40_0000));
        assert!(frame_is_in_tick_list(0xffff_ffff));
        assert!(!frame_is_in_tick_list(0));
        assert!(!frame_is_in_tick_list(0x80));
    }
    #[test]
    fn frame_update_bit_marks_pending_update() {
        assert!(frame_update_is_pending(0x4000_0000));
        assert!(frame_update_is_pending(0xffff_ffff));
        assert!(!frame_update_is_pending(0));
        assert!(!frame_update_is_pending(0x40_0000));
    }
    #[test]
    fn frame_no_cull_bit_skips_portal_culling() {
        assert!(frame_is_no_cull(0x0800_0000));
        assert!(frame_is_no_cull(0xffff_ffff));
        assert!(!frame_is_no_cull(0));
        assert!(!frame_is_no_cull(0x80));
    }
    #[test]
    fn frame_duplicate_flags_keep_only_the_copied_bits() {
        assert_eq!(frame_duplicate_flags(0), 0);
        assert_eq!(frame_duplicate_flags(0xffff_ffff), 0x0fa7_3e9f);
        assert_eq!(frame_duplicate_flags(1), 1);
        assert_eq!(frame_duplicate_flags(0x4000_0000), 0);
        assert_eq!(frame_duplicate_flags(0x0fa7_3e9f), 0x0fa7_3e9f);
    }
    #[test]
    fn frame_list_membership_bits_report_dtor_gates() {
        assert!(frame_in_fast_find_tree(0x2000_0000));
        assert!(frame_in_fast_find_tree(0xffff_ffff));
        assert!(!frame_in_fast_find_tree(0));
        assert!(!frame_in_fast_find_tree(0x80));
        assert!(frame_is_target_frame(0x4000));
        assert!(frame_is_target_frame(0xffff_ffff));
        assert!(!frame_is_target_frame(0));
        assert!(!frame_is_target_frame(0x8000));
        assert!(frame_is_shadow_frame(0x8000));
        assert!(frame_is_shadow_frame(0xffff_ffff));
        assert!(!frame_is_shadow_frame(0));
        assert!(!frame_is_shadow_frame(0x4000));
    }
    #[test]
    fn frame_weather_and_changed_bits_report_state_flags() {
        assert!(frame_is_weather_dummy(0x8000_0000));
        assert!(frame_is_weather_dummy(0xffff_ffff));
        assert!(!frame_is_weather_dummy(0));
        assert!(!frame_is_weather_dummy(0x2000_0000));
        assert!(frame_has_changed(0x2000));
        assert!(frame_has_changed(0xffff_ffff));
        assert!(!frame_has_changed(0));
        assert!(!frame_has_changed(0x4000_0000));
    }
    #[test]
    fn link_flag_words_match_native_linkto_bits() {
        // Working names for the I3DLINK_* words (mafia-re ls3df/I3D/I3D.h:306-311):
        // the reference records no LS3DF FUNCTION addresses (defines, not functions).
        assert_eq!(I3DLINK_UPDMATRIX, 0x1);
        assert_eq!(I3DLINK_DISCARD_BOUND, 0x2);
        assert_eq!(I3DLINK_UPDATE, 0x4);
        assert!(link_keeps_world_matrix(I3DLINK_UPDMATRIX));
        assert!(link_keeps_world_matrix(I3DLINK_UPDMATRIX | I3DLINK_DISCARD_BOUND));
        assert!(!link_keeps_world_matrix(0));
        assert!(!link_keeps_world_matrix(I3DLINK_DISCARD_BOUND | I3DLINK_UPDATE));
        assert!(!link_keeps_world_matrix(u32::MAX & !I3DLINK_UPDMATRIX));
        assert!(link_discards_bounds(I3DLINK_DISCARD_BOUND));
        assert!(!link_discards_bounds(0));
        assert!(!link_discards_bounds(I3DLINK_UPDMATRIX));
        assert!(link_updates_at_once(I3DLINK_UPDATE));
        assert!(!link_updates_at_once(0));
        assert!(!link_updates_at_once(I3DLINK_UPDMATRIX));
    }
    #[test]
    fn update_w_matrix_refreshes_only_a_stale_world_cache() {
        // Working name for inlined I3D_frame::UpdateWMatrix (I3D.h:505-510): no
        // LS3DF FUNCTION address exists (header inline, no Game.exe copy either).
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(I3dFrameState::model()).unwrap();
        // Fresh constructor frames carry WMAT_VALID: the gate is a no-op.
        assert!(frm_wmat_valid(graph.state(key).unwrap().flags));
        graph.update_w_matrix(key).unwrap();
        assert!(frm_wmat_valid(graph.state(key).unwrap().flags));
        // Clearing the bit forces the UpdateWMatrixProc path and restores it;
        // a parentless world cache matches the local matrix.
        graph.change_flags(key, 0x20, 0).unwrap();
        assert!(!frm_wmat_valid(graph.state(key).unwrap().flags));
        graph.update_w_matrix(key).unwrap();
        assert!(frm_wmat_valid(graph.state(key).unwrap().flags));
        assert_eq!(
            graph.world_matrix(key).unwrap(),
            graph.local_matrix(key).unwrap()
        );
        // Unknown frames fail before any cache move.
        assert_eq!(
            graph.update_w_matrix(FrameKey(u32::MAX)),
            Err(FrameError::UnknownFrame(FrameKey(u32::MAX)))
        );
    }
    #[test]
    fn frame_callback_message_bits_report_only_their_bits() {
        // Working names for the inlined LS3D_CALLBACK_* gates (I3D.h:299-303):
        // no LS3DF FUNCTION addresses exist for standalone readers.
        assert!(frame_callback_wants_note(0x200));
        assert!(frame_callback_wants_lod(0x400));
        assert!(frame_callback_wants_anim_end(0x800));
        assert!(frame_callback_wants_note_string(0x1000));
        assert!(frame_callback_wants_note(0x1e00));
        assert!(frame_callback_wants_lod(0x1e00));
        assert!(frame_callback_wants_anim_end(0x1e00));
        assert!(frame_callback_wants_note_string(0x1e00));
        assert!(!frame_callback_wants_note(0));
        assert!(!frame_callback_wants_lod(0x200));
        assert!(!frame_callback_wants_anim_end(0x400));
        assert!(!frame_callback_wants_note_string(0x800));
        assert!(!frame_callback_wants_note(u32::MAX & !0x200));
        assert!(!frame_callback_wants_lod(u32::MAX & !0x400));
        assert!(!frame_callback_wants_anim_end(u32::MAX & !0x800));
        assert!(!frame_callback_wants_note_string(u32::MAX & !0x1000));
        assert!(frame_callback_wants_note(u32::MAX));
        assert!(frame_callback_wants_lod(u32::MAX));
        assert!(frame_callback_wants_anim_end(u32::MAX));
        assert!(frame_callback_wants_note_string(u32::MAX));
    }
    #[test]
    fn frame_callback_enables_only_documented_message_bits() {
        // Working name for the generic LS3D_CALLBACK_* dispatcher (I3D.h:299-303):
        // no LS3DF FUNCTION address exists for a standalone dispatcher.
        for bit in [0x200u32, 0x400, 0x800, 0x1000] {
            assert!(frame_callback_enables(bit, bit));
            assert!(frame_callback_enables(0x1e00, bit));
            assert!(frame_callback_enables(u32::MAX, bit));
            assert!(!frame_callback_enables(0, bit));
            assert!(!frame_callback_enables(u32::MAX & !bit, bit));
        }
        // Loader message words and undocumented bits never dispatch.
        for bit in [0x8u32, 0x10, 0x20, 0x100, 0x2000, 0] {
            assert!(!frame_callback_enables(0x1e00, bit));
            assert!(!frame_callback_enables(u32::MAX, bit));
        }
    }
    #[test]
    fn frame_update_gates_report_either_pending_bit() {
        // Working names for the UPDATE|CHANGED gates (I3d_frame.cpp:887,
        // I3d_target.cpp:117): no standalone LS3DF FUNCTION entries.
        assert!(!frame_needs_update(0));
        assert!(frame_needs_update(0x4000_0000));
        assert!(frame_needs_update(0x2000));
        assert!(frame_needs_update(0x4000_2000));
        assert!(frame_needs_update(u32::MAX));
        assert!(!frame_needs_update(u32::MAX & !(0x4000_0000 | 0x2000)));
        // Target queuing additionally requires ON.
        assert!(!frame_queues_target_update(0x4000_0000));
        assert!(!frame_queues_target_update(0x2000));
        assert!(frame_queues_target_update(0x1 | 0x4000_0000));
        assert!(frame_queues_target_update(0x1 | 0x2000));
        assert!(frame_queues_target_update(u32::MAX));
        assert!(!frame_queues_target_update(u32::MAX & !0x1));
    }
    #[test]
    fn frame_set_callback_masks_and_branches_on_presence() {
        // Working name for SetCallback (I3d_frame.cpp:225-234): PC RVA
        // 0x1b3c0 already occupied, not claimed here.
        assert_eq!(frame_flags_after_set_callback(0, true, 0x1e00), 0x1e00);
        assert_eq!(frame_flags_after_set_callback(0xffff, true, 0x200), 0xffff);
        assert_eq!(frame_flags_after_set_callback(0xffff, false, 0x200), 0xffff & !0x200);
        assert_eq!(frame_flags_after_set_callback(0x1e00, false, 0x1e00), 0);
        // Bits outside 0x1e00 never pass the mask.
        assert_eq!(frame_flags_after_set_callback(0, true, 0x8), 0);
        assert_eq!(frame_flags_after_set_callback(0xffff, false, 0x8), 0xffff);
        assert_eq!(frame_flags_after_set_callback(0, true, u32::MAX), 0x1e00);
    }
    #[test]
    fn frame_set_on_stores_bit_zero_and_empty_bases_move_nothing() {
        // Working names for SetOn (I3d_frame.cpp:238-244, RVA 0x1b400) and
        // the empty bases Tick (I3D.h:350) and SaveImage (I3D.h:367): the
        // RVA is occupied and not claimed; the bases have no LS3DF address.
        assert_eq!(frame_flags_after_set_on(0, true), 1);
        assert_eq!(frame_flags_after_set_on(0xfffffffe, true), u32::MAX);
        assert_eq!(frame_flags_after_set_on(1, true), 1);
        assert_eq!(frame_flags_after_set_on(1, false), 0);
        assert_eq!(frame_flags_after_set_on(u32::MAX, false), u32::MAX & !1);
        assert_eq!(frame_flags_after_set_on(0x4000_0000, false), 0x4000_0000);
        frame_tick_noop(0);
        frame_tick_noop(-1);
        frame_tick_noop(i32::MAX);
        frame_save_image_noop();
    }
    #[test]
    fn frame_duplicate_and_hierarchy_bits_match_native_words() {
        // Working names for Duplicate (I3d_frame.cpp:807-809, RVA 0x1c5b0)
        // and LinkTo hr_flags (I3d_frame.cpp:503, RVA 0x1b9d0): both
        // occupied, neither claimed here.
        assert_eq!(frame_duplicate_preserved_bits(0), 0);
        assert_eq!(
            frame_duplicate_preserved_bits(0x40_0000 | 0x8000 | 0x4000),
            0x40_0000 | 0x8000 | 0x4000
        );
        assert_eq!(frame_duplicate_preserved_bits(u32::MAX), 0x40_0000 | 0x8000 | 0x4000);
        assert_eq!(frame_hr_hierarchy_bits(0), 0);
        assert_eq!(
            frame_hr_hierarchy_bits(0x8_0000 | 0x10_0000 | 0x4_0000),
            0x8_0000 | 0x10_0000 | 0x4_0000
        );
        assert_eq!(
            frame_hr_hierarchy_bits(u32::MAX),
            0x8_0000 | 0x10_0000 | 0x4_0000
        );
        assert_eq!(frame_hr_hierarchy_bits(0x1e00), 0);
    }
    #[test]
    fn frame_child_list_and_link_words_clear_only_their_bits() {
        // Working names for AddChild/RemoveChild (RVAs 0x1bea0/0x1bf20) and
        // LinkTo no-keep path (RVA 0x1b9d0): all occupied, none claimed.
        assert_eq!(frame_flags_after_child_list_change(0x100 | 0x40), 0);
        assert_eq!(frame_flags_after_child_list_change(u32::MAX), u32::MAX & !(0x100 | 0x40));
        assert_eq!(frame_flags_after_child_list_change(0x4000_0000), 0x4000_0000);
        assert_eq!(frame_flags_after_link_no_keep(0x20), 0x4000_0000);
        assert_eq!(
            frame_flags_after_link_no_keep(0),
            0x4000_0000
        );
        assert_eq!(
            frame_flags_after_link_no_keep(u32::MAX),
            (u32::MAX & !0x20) | 0x4000_0000
        );
    }
    #[test]
    fn frame_bound_update_and_propagation_words_match_native_stores() {
        // Working names for SetBoundVolume (RVA 0x1ce80), Update (RVA
        // 0x1ca60) and UpdateChildren (RVA 0x1c9a0): all occupied.
        assert_eq!(frame_flags_after_set_bound_volume(0x100), 0x40);
        assert_eq!(frame_flags_after_set_bound_volume(0), 0x40);
        assert_eq!(
            frame_flags_after_set_bound_volume(u32::MAX),
            (u32::MAX & !0x100) | 0x40
        );
        assert_eq!(frame_flags_after_update(0x4000_0000 | 0x2000), 0);
        assert_eq!(frame_flags_after_update(u32::MAX), u32::MAX & !(0x4000_0000 | 0x2000));
        assert_eq!(
            frame_child_flags_after_parent_change(0x100 | 0x20),
            0x2000
        );
        assert_eq!(
            frame_child_flags_after_parent_change(0),
            0x2000
        );
        assert_eq!(
            frame_flags_after_update_children_parent(0x100 | 0x40),
            0x2000
        );
        assert_eq!(
            frame_flags_after_update_children_parent(0),
            0x2000
        );
    }
    #[test]
    fn frame_world_dir_and_world_space_words_match_native_stores() {
        // Working names for SetWorldDir/SetWorldPosDir
        // (I3d_frame.cpp:183-184/:217-218; RVAs 0x1b040/0x1b230 occupied or
        // projected) and VISF_WORLD_SPACE (I3d_visual.cpp:32/:40; RVAs
        // 0x5a390/0x5a3c0 occupied or ported): none claimed here.
        assert_eq!(
            frame_flags_after_set_world_dir(0x100 | 0x20 | 0x08 | 0x04),
            0x4000_0000 | 0x10
        );
        assert_eq!(frame_flags_after_set_world_dir(0), 0x4000_0000 | 0x10);
        assert_eq!(
            frame_flags_after_set_world_dir(u32::MAX),
            (u32::MAX & !(0x100 | 0x20 | 0x08 | 0x04)) | (0x4000_0000 | 0x10)
        );
        assert!(frame_visual_in_world_space(0x40));
        assert!(frame_visual_in_world_space(u32::MAX));
        assert!(!frame_visual_in_world_space(0));
        assert!(!frame_visual_in_world_space(0xffffffbf));
    }
    #[test]
    fn on_flag_reader_reports_only_bit_zero() {
        // Working name for the FRMFLAGS_ON bit (I3D.h:259): no LS3DF
        // FUNCTION address exists for a standalone reader (only Game.exe
        // 0x47acf0, unclaimed). The arena frame_is_on method covers stateful
        // reads; this pins the pure word test.
        assert!(frame_is_on_flag(0x1));
        assert!(frame_is_on_flag(u32::MAX));
        assert!(!frame_is_on_flag(0));
        assert!(!frame_is_on_flag(0x80));
        assert!(!frame_is_on_flag(u32::MAX & !0x1));
    }
    #[test]
    fn data_word_returns_stored_user_word() {
        // Working name for the inlined I3D_frame::GetData (I3D.h:595-600,
        // m_UserData at +0x08): no LS3DF FUNCTION address exists for a
        // standalone reader.
        assert_eq!(frame_data_word(0), 0);
        assert_eq!(frame_data_word(1), 1);
        assert_eq!(frame_data_word(0x1234_5678), 0x1234_5678);
        assert_eq!(frame_data_word(u32::MAX), u32::MAX);
    }

    #[test]
    fn duplicate_rejects_only_the_null_source() {
        // Working name for the I3D_frame::Duplicate null limb
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:799-804): the occupied
        // body at PC RVA 0x1c5b0 is not claimed here.
        assert!(frame_duplicate_rejects_null_source(true));
        assert!(!frame_duplicate_rejects_null_source(false));
    }

    #[test]
    fn duplicate_null_name_clears_both_name_words() {
        // Working name for the null-name arm of I3D_frame::Duplicate
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:813-816): a sourceless
        // name clears both the copied name and the rebased original-name
        // pointer; the occupied body at PC RVA 0x1c5b0 is not claimed here.
        assert!(frame_duplicate_clears_name_words(true));
        assert!(!frame_duplicate_clears_name_words(false));
    }

    #[test]
    fn duplicate_merges_masked_source_kept_lists_and_update() {
        // Working name for the merged flag word of I3D_frame::Duplicate
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:807-809): source masked
        // with FRMFLAGS_DUPLICATE_MASK, destination list bits kept, UPDATE
        // always set.
        assert_eq!(frame_duplicate_merged_flags(0, 0), 0x4000_0000);
        assert_eq!(
            frame_duplicate_merged_flags(u32::MAX, u32::MAX),
            0x0fa7_3e9f | 0x0040_c000 | 0x4000_0000
        );
        assert_eq!(
            frame_duplicate_merged_flags(0, 0x4000),
            0x4000_0000 | 0x4000
        );
        assert_eq!(
            frame_duplicate_merged_flags(0x10, 0x20 | 0x100),
            0x4000_0000 | 0x10
        );
    }

    #[test]
    fn duplicate_rejoins_fast_find_only_with_name_and_flag() {
        // Working name for the RS_FAST_FIND re-registration gate of
        // I3D_frame::Duplicate (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:824-827): the occupied body is not
        // claimed here.
        assert!(frame_duplicate_rejoins_fast_find(true, true));
        assert!(!frame_duplicate_rejoins_fast_find(true, false));
        assert!(!frame_duplicate_rejoins_fast_find(false, true));
        assert!(!frame_duplicate_rejoins_fast_find(false, false));
    }

    #[test]
    fn duplicate_orig_name_offset_rebases_pointer_difference() {
        // Working name for the original-name rebase of I3D_frame::Duplicate
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:827:
        // `m_pOrigName = frame->m_pOrigName - frame->m_pName + m_pName`).
        // Wrapping offsets stand in for the native interior pointers.
        assert_eq!(duplicate_orig_name_offset(100, 104, 200), 204);
        assert_eq!(duplicate_orig_name_offset(100, 100, 200), 200);
        assert_eq!(duplicate_orig_name_offset(0, 0, 0), 0);
        assert_eq!(
            duplicate_orig_name_offset(0, usize::MAX, 1),
            usize::MAX.wrapping_add(1)
        );
        assert_eq!(
            duplicate_orig_name_offset(8, 3, 100),
            3usize.wrapping_sub(8).wrapping_add(100)
        );
    }

    #[test]
    fn changed_word_keeps_only_update_and_changed() {
        // Working name for the `changed` word of UpdateChildren (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:887): UPDATE 0x40000000 and
        // CHANGED 0x2000; the occupied body is not claimed here.
        assert_eq!(frame_changed_word(0x4000_2000), 0x4000_2000);
        assert_eq!(frame_changed_word(u32::MAX), 0x4000_2000);
        assert_eq!(frame_changed_word(0), 0);
        assert_eq!(frame_changed_word(0x4000_0000), 0x4000_0000);
        assert_eq!(frame_changed_word(0x2000), 0x2000);
        assert_eq!(frame_changed_word(0x0fa7_3e9f), 0x2000);
    }

    #[test]
    fn target_list_drains_only_with_pending_targets_outside_flush() {
        // Working name for the drain gate of I3D_frame::Update (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:916-926): pending list and no
        // reentrant flush; the occupied body is not claimed here.
        assert!(update_drains_target_list(1, false));
        assert!(update_drains_target_list(usize::MAX, false));
        assert!(!update_drains_target_list(0, false));
        assert!(!update_drains_target_list(1, true));
        assert!(!update_drains_target_list(0, true));
    }

    #[test]
    fn sector_pos_ancestor_keep_needs_backdrop_or_omni_sector() {
        // Working name for the sector-ancestor early-out of
        // I3D_scene::SetFrameSectorPos (mafia-re
        // ls3df/I3D/I3d_scene.cpp:517-533): the occupied body at PC RVA
        // 0x4a280 is not claimed here.
        assert!(set_frame_sector_pos_ancestor_keeps(true, true, false));
        assert!(set_frame_sector_pos_ancestor_keeps(true, false, true));
        assert!(set_frame_sector_pos_ancestor_keeps(true, true, true));
        assert!(!set_frame_sector_pos_ancestor_keeps(true, false, false));
        assert!(!set_frame_sector_pos_ancestor_keeps(false, true, true));
        assert!(!set_frame_sector_pos_ancestor_keeps(false, false, false));
    }

    #[test]
    fn sector_pos_skips_only_the_same_sector() {
        // Working name for the same-sector no-op of
        // I3D_scene::SetFrameSectorPos (mafia-re
        // ls3df/I3D/I3d_scene.cpp:529-532): the occupied body is not
        // claimed here.
        assert!(set_frame_sector_pos_skips_same_sector(true));
        assert!(!set_frame_sector_pos_skips_same_sector(false));
    }

    #[test]
    fn world_dir_roll_gate_matches_mrg_zero() {
        // Working name for the roll gate of SetWorldDir/SetWorldPosDir
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:178 and :213):
        // `fabs(roll) > MRG_ZERO` with MRG_ZERO 1e-8 per ls3df/ls3df.h:48.
        // NaN stays unordered (false), matching the native comparison.
        assert!(world_dir_applies_roll(1e-7));
        assert!(world_dir_applies_roll(-1.0));
        assert!(!world_dir_applies_roll(0.0));
        assert!(!world_dir_applies_roll(-0.0));
        assert!(!world_dir_applies_roll(1e-8));
        assert!(!world_dir_applies_roll(5e-9));
        assert!(!world_dir_applies_roll(f32::NAN));
    }

    #[test]
    fn release_deletes_only_the_last_reference() {
        // Working name for the last-reference limb of I3D_frame::Release
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:98-104): the occupied
        // body at PC RVA 0x1aae0 is not claimed here. The argument is the
        // pre-decrement count; only one deletes.
        assert!(frame_release_deletes_last(1));
        assert!(!frame_release_deletes_last(0));
        assert!(!frame_release_deletes_last(2));
        assert!(!frame_release_deletes_last(u32::MAX));
    }

    #[test]
    fn set_property_stores_null_for_null_and_empty() {
        // Working name for the null/empty limbs of I3D_frame::SetProperty
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:834-845): both store
        // NULL and return I3D_OK; the occupied body at PC RVA 0x1c700 is
        // not claimed here.
        assert!(frame_set_property_stores_null(None));
        assert!(frame_set_property_stores_null(Some(0)));
        assert!(!frame_set_property_stores_null(Some(1)));
        assert!(!frame_set_property_stores_null(Some(usize::MAX)));
    }

    #[test]
    fn link_to_same_parent_is_a_noop() {
        // Working name for the same-parent early-out of I3D_frame::LinkTo
        // (mafia-re ls3df/I3D/frames/I3d_frame.cpp:465-466): returns
        // I3D_OK with no store; the occupied body at PC RVA 0x1b9d0 is not
        // claimed here.
        assert!(link_to_same_parent_noop(true));
        assert!(!link_to_same_parent_noop(false));
    }

    #[test]
    fn base_empty_load_image_does_not_move_state() {
        // Working name for the in-class empty base LoadImage
        // (mafia-re ls3df/I3D/I3D.h:368): no LS3DF FUNCTION address exists.
        frame_load_image_noop();
    }

    #[test]
    fn base_empty_update_v_does_not_move_state() {
        // Working name for the in-class empty base UpdateV
        // (mafia-re ls3df/I3D/I3D.h:366): the out-of-line empties are shared
        // with the renderer base, not LS3DF frame code, so no RVA is claimed.
        frame_update_v_noop();
    }

    #[test]
    fn name_length_stops_at_first_nul() {
        // Working name for the NUL-terminated length behind GetName/GetOrigName/
        // GetProperty (mafia-re ls3df/I3D/I3D.h:583-593,625-630): no LS3DF
        // FUNCTION address exists (only Game.exe 0x47ad00 for GetName).
        assert_eq!(frame_name_len_until_nul(b""), 0);
        assert_eq!(frame_name_len_until_nul(b"abc"), 3);
        assert_eq!(frame_name_len_until_nul(b"a\0bc"), 1);
        assert_eq!(frame_name_len_until_nul(b"\0abc"), 0);
        assert_eq!(frame_name_len_until_nul(b"ab\0"), 2);
    }

    #[test]
    fn property_allocation_adds_nul_or_reports_overflow() {
        // Working name for the SetProperty len+1 limb (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:834-845): the occupied body at PC
        // RVA 0x1c700 is not claimed here.
        assert_eq!(frame_property_alloc_len(0), Some(1));
        assert_eq!(frame_property_alloc_len(3), Some(4));
        assert_eq!(frame_property_alloc_len(usize::MAX), None);
    }

    #[test]
    fn link_mask_covers_the_three_admitted_bits() {
        // Working name for the I3DLINK_* mask (mafia-re ls3df/I3D/I3D.h:307-311):
        // defines, not functions, so no LS3DF FUNCTION address exists.
        assert_eq!(link_supported_flags_mask(), 0x7);
        assert_eq!(
            link_supported_flags_mask(),
            super::I3DLINK_UPDMATRIX | super::I3DLINK_DISCARD_BOUND | super::I3DLINK_UPDATE
        );
    }

    #[test]
    fn set_name_content_compare_uses_c_string_semantics() {
        // Working name for the SetName content-equal early-out (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:350-377): the occupied body at PC
        // RVA 0x1b6f0 is not claimed here.
        assert!(frame_set_name_is_content_equal(None, None));
        assert!(frame_set_name_is_content_equal(Some(b"a"), Some(b"a")));
        assert!(frame_set_name_is_content_equal(
            Some(b"a\0extra"),
            Some(b"a")
        ));
        assert!(!frame_set_name_is_content_equal(Some(b"a"), Some(b"b")));
        assert!(!frame_set_name_is_content_equal(None, Some(b"a")));
        assert!(!frame_set_name_is_content_equal(Some(b"a"), None));
        assert!(!frame_set_name_is_content_equal(Some(b""), Some(b"a")));
    }

    #[test]
    fn get_child_gate_accepts_only_in_range_indices() {
        // Working name for the GetChild index gate (mafia-re
        // ls3df/I3D/frames/I3d_frame.cpp:610-620): the occupied body at PC
        // RVA 0x1bfd0 is not claimed here.
        assert!(frame_get_child_index_in_range(0, 1));
        assert!(frame_get_child_index_in_range(2, 3));
        assert!(!frame_get_child_index_in_range(3, 3));
        assert!(!frame_get_child_index_in_range(-1, 3));
        assert!(!frame_get_child_index_in_range(0, 0));
        assert!(!frame_get_child_index_in_range(i32::MIN, 5));
        assert!(!frame_get_child_index_in_range(i32::MAX, u32::MAX));
    }

    #[test]
    fn frame_name_byte_empties_match_null_pointer_fallback() {
        // Working names for the GetName/GetOrigName/GetProperty `? : ""`
        // fallbacks (mafia-re ls3df/I3D/I3D.h:583-593,625-630): no LS3DF
        // FUNCTION addresses exist for standalone readers (only Game.exe
        // 0x47ad00 for GetName, unclaimed). Pure byte rules, no owner.
        assert_eq!(frame_name_bytes_or_empty(None), b"");
        assert_eq!(frame_name_bytes_or_empty(Some(b"a")), b"a");
        assert_eq!(frame_name_bytes_or_empty(Some(b"")), b"");
        assert_eq!(frame_orig_name_bytes_or_empty(None), b"");
        assert_eq!(frame_orig_name_bytes_or_empty(Some(b"m.f")), b"m.f");
        assert_eq!(frame_property_bytes_or_empty(None), b"");
        assert_eq!(frame_property_bytes_or_empty(Some(b"p")), b"p");
        assert_eq!(frame_property_bytes_or_empty(Some(b"")), b"");
    }

    #[test]
    fn frame_type_word_discriminants_match_frame_enum_values() {
        // Working names for the FRAME_* discriminants behind GetType
        // (mafia-re ls3df/I3D/I3D.h:61-74): no LS3DF FUNCTION addresses
        // exist for standalone readers.
        assert!(frame_type_word_is_user(8));
        assert!(frame_type_word_is_volume(11));
        assert!(frame_type_word_is_dummy(6));
        assert!(frame_type_word_is_joint(10));
        assert!(frame_type_word_is_target(7));
        assert!(frame_type_word_is_model(9));
        assert!(!frame_type_word_is_user(0));
        assert!(!frame_type_word_is_user(11));
        assert!(!frame_type_word_is_volume(8));
        assert!(!frame_type_word_is_dummy(7));
        assert!(!frame_type_word_is_joint(9));
        assert!(!frame_type_word_is_target(0x4000));
        assert!(!frame_type_word_is_model(u32::MAX));
        assert!(!frame_type_word_is_user(u32::MAX));
    }

    #[test]
    fn frame_type_word_remaining_discriminants_match_frame_enum_values() {
        // Working names for the remaining FRAME_* discriminants behind GetType
        // (mafia-re ls3df/I3D/I3D.h:61-80): no LS3DF FUNCTION addresses
        // exist for standalone readers; values are the enum order.
        assert!(frame_type_word_is_null(0));
        assert!(frame_type_word_is_visual(1));
        assert!(frame_type_word_is_light(2));
        assert!(frame_type_word_is_camera(3));
        assert!(frame_type_word_is_sound(4));
        assert!(frame_type_word_is_sector(5));
        assert!(frame_type_word_is_occluder(12));
        assert!(frame_type_word_is_scene(13));
        assert!(frame_type_word_is_area(14));
        assert!(frame_type_word_is_shadow(15));
        assert!(frame_type_word_is_landscape(16));
        assert!(frame_type_word_is_emitor(17));
        assert!(!frame_type_word_is_null(1));
        assert!(!frame_type_word_is_visual(0));
        assert!(!frame_type_word_is_light(1));
        assert!(!frame_type_word_is_camera(2));
        assert!(!frame_type_word_is_sound(5));
        assert!(!frame_type_word_is_sector(4));
        assert!(!frame_type_word_is_occluder(13));
        assert!(!frame_type_word_is_scene(14));
        assert!(!frame_type_word_is_area(13));
        assert!(!frame_type_word_is_shadow(14));
        assert!(!frame_type_word_is_landscape(17));
        assert!(!frame_type_word_is_emitor(16));
        assert!(!frame_type_word_is_visual(u32::MAX));
        assert!(!frame_type_word_is_scene(u32::MAX));
    }

    #[test]
    fn frame_type_bound_helpers_match_last_sentinel() {
        // Working names for the FRAME_LAST bound (mafia-re ls3df/I3D/I3D.h:81:
        // 18 is the count; the 4ds loader refuses a type from here on): no
        // LS3DF FUNCTION address exists for a standalone reader.
        assert!(frame_type_word_is_last(18));
        assert!(!frame_type_word_is_last(17));
        assert!(!frame_type_word_is_last(0));
        assert!(!frame_type_word_is_last(u32::MAX));
        assert!(frame_type_in_valid_range(0));
        assert!(frame_type_in_valid_range(17));
        assert!(!frame_type_in_valid_range(18));
        assert!(!frame_type_in_valid_range(u32::MAX));
        assert!(frame_type_loader_refuses(18));
        assert!(frame_type_loader_refuses(u32::MAX));
        assert!(!frame_type_loader_refuses(0));
        assert!(!frame_type_loader_refuses(17));
        assert_eq!(
            frame_type_in_valid_range(17),
            !frame_type_loader_refuses(17)
        );
        assert_eq!(
            frame_type_in_valid_range(18),
            !frame_type_loader_refuses(18)
        );
    }
}
