//! I3D_mirror frame state (LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//! Upstream working names (mafia-re ls3df/I3D/visuals/I3d_mirror.cpp), not evidenced
//! original symbols; binary/RVA mapping retained per item. No FrameKind::Mirror exists
//! (FrameKind checked: Camera Occluder Scene Sector Model Mesh Lensflare Projector Light
//! Sound Dummy Area Joint Target Singlemesh); mirror frames use the neutral Model carrier
//! and are discriminated by LoadedFrame.mirror (same pattern as LoadedFrame.projector).
//! visual_type has no arena home yet, so the evidenced 'MIRR' dword is kept as an
//! uninterpreted tag word, like the ctor residue word +0x1f0 = 0x20 (no invented meaning).
//! Precision policy: SetColor uses bitwise copy + f32 comparisons mirroring the x87
//! FCOMP/FNSTSW/JP/JNZ limbs (NaN preserved); LoadImage and all range/color/view_box
//! copies are bitwise (LE decode / copy, no arithmetic). Reflection math below uses
//! host f32 in native source order via SMatrix::mul_affine/transform_point (no fused
//! ops, no clamping); non-finite inputs are refused, never a visibility/draw answer.
//! Device calls (dtaRead, operator new/delete, copy Release pair, base visual
//! ctor/dtor/LoadImage virtuals, every D3D8 render call) stay backend contracts.
//! Helper accessors get no function credit.
//! Numeric gate (disclosed hardening): non-finite AND subnormal f32 inputs are
//! refused throughout this module. x87 native has no subnormal gate; tiny
//! denormal colors/ranges/matrices report the boundary instead of computing.
use crate::frame_transform::{FrameKey, FrameKind, I3dFrameState};
use crate::mesh_bounds::MeshBoundVolume;
use crate::model_loading::{ModelArena, ModelError};
use crate::native_transform::SMatrix;
use crate::scene_ownership::{SceneKey, SectorKey};

/// LS3D_RESULT words evidenced at 0x31ab0 (MOV EAX imm / XOR EAX,EAX, RET 8).
pub const MIRROR_OK: u32 = 0;
pub const MIRROR_INVALIDPARAMS: u32 = 0x8000_0007;
/// Flag words evidenced by the AND/OR immediates at 0x32060/0x332e0 plus the
/// Draw cull/visibility arms at 0x31c70/0x321b0.
pub const MIRF_VIEW_BOX: u32 = 0x1;
pub const MIRF_PREPARED: u32 = 0x4;
pub const MIRF_VISIBLE: u32 = 0x100;
pub const MIRF_REFLECT: u32 = 0x200;
pub const VISF_BOUND_VALID: u32 = 0x0002_0000;
pub const VISF_WBOUND_VALID: u32 = 0x0004_0000;
pub const MIRROR_UPDATE_CLEAR_MASK: u32 = 0x001c_0000;
pub const MIRROR_DIRTY_TEST: u32 = 0x4000_2000;
/// Evidenced ctor stores: 'MIRR' at +0x1d4, 100.0f at +0x2d8, residue at +0x1f0.
pub const MIRROR_VISUAL_TAG: u32 = 0x5252_494d;
pub const MIRROR_RANGE_DEFAULT_BITS: u32 = 0x42c8_0000;
pub const MIRROR_RESIDUE_1F0: u32 = 0x20;

#[derive(Clone, Debug, PartialEq)]
pub struct I3dMirrorState {
    pub visual_tag: u32,
    pub residue_1f0: u32,
    pub range: f32,
    pub view_box: SMatrix,
    pub inv_view_box: SMatrix,
    pub mirror_matrix: SMatrix,
    pub bsphere_center: [f32; 3],
    pub bsphere_radius: f32,
    pub verts: Vec<[f32; 3]>,
    pub faces: Vec<[u16; 3]>,
    pub mirror_flags: u32,
    pub color: [f32; 3],
    pub render_number: u32,
    pub scene: Option<SceneKey>,
    pub copies: Vec<FrameKey>,
}

impl I3dMirrorState {
    pub(crate) fn constructor() -> Self {
        Self {
            visual_tag: MIRROR_VISUAL_TAG,
            residue_1f0: MIRROR_RESIDUE_1F0,
            range: f32::from_bits(MIRROR_RANGE_DEFAULT_BITS),
            view_box: SMatrix::identity(),
            inv_view_box: SMatrix::identity(),
            mirror_matrix: SMatrix::identity(),
            bsphere_center: [0.; 3],
            bsphere_radius: 0.,
            verts: Vec::new(),
            faces: Vec::new(),
            mirror_flags: 0,
            color: [0.; 3],
            render_number: u32::MAX,
            scene: None,
            copies: Vec::new(),
        }
    }
    pub fn has_view_box(&self) -> bool {
        self.mirror_flags & MIRF_VIEW_BOX != 0
    }
    pub fn is_prepared(&self) -> bool {
        self.mirror_flags & MIRF_PREPARED != 0
    }
}

/// Copy Release pair of the dtor/Duplicate loops (UserData-frame Release then visual
/// Release, in that order) plus the liveness probe for the Tick walk.
pub trait MirrorCopyBackend {
    fn release_mirror_copy(&mut self, copy: FrameKey) -> Result<(), ModelError>;
    /// Native Tick guard per copy: `frm->m_RefCount >= 2 && frm->m_pParent`.
    /// Returns (references, has_parent) with a live reread each step.
    fn copy_target(&self, copy: FrameKey) -> Result<(u32, bool), ModelError>;
}

/// dtaRead wiring + unported base LoadImage head (RVA 0x1d1c0). Straight-line native:
/// backend failure propagates with preceding stores retained (no native recovery branch).
pub trait MirrorImageBackend {
    fn load_base_frame_image(&mut self, key: FrameKey) -> Result<(), ModelError>;
    fn read_mirror_bytes(&mut self, len: usize) -> Result<Vec<u8>, ModelError>;
}

/// Visibility verdict for UpdateVisibility (PC RVA 0x321b0). The render-number
/// mismatch clears only VISIBLE (native `&= ~MIRF_VISIBLE; return`), while the
/// facing-away gate clears VISIBLE|REFLECT — the two are distinct arms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorVisibilityVerdict {
    Hidden,
    FacingAway,
    Visible { reflect: bool },
}

/// CPU draw plan for Draw (PC RVA 0x31c70, differential). Null-guard +
/// alpha/color pack + cull select in native order; every D3D8 call
/// (SetColorOp/AlphaOp, cull/clip/world/texture state, DrawIndexedPrimitive)
/// stays a backend contract behind MirrorDrawBackend. No D3D8 implementor
/// exists yet; the future draw emitter implements that trait. Refused inputs
/// (non-finite/subnormal, or finite out-of-[0,1] channels that fail the
/// FloatToInt domain) yield None — native would pack the wrapped word and
/// draw (disclosed hardening).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorCullMode {
    None,
    Cw,
    Ccw,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MirrorDrawPlan {
    pub color: u32,
    pub alpha_none: bool,
    pub cull: MirrorCullMode,
    pub verts: usize,
    pub faces: usize,
}
pub trait MirrorDrawBackend {
    type Error;
    fn emit_mirror_draw(&mut self, plan: MirrorDrawPlan) -> Result<(), Self::Error>;
}

/// One candidate visual for CollectVisuals (PC RVA 0x32500). Live views are
/// supplied by the caller in child→next pre-order; this plan only applies the
/// evidenced gate order (flag/sphere/visual/bound/clip) and reports pushes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MirrorCollectNode {
    pub id: u32,
    pub on_tick: bool,
    pub center: [f32; 3],
    pub radius: f32,
    pub is_visual: bool,
    pub object_no_proxy_mirror: bool,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
}

fn f32_finite(v: f32) -> bool {
    v.is_finite() && !v.is_subnormal()
}

/// FloatToInt(x*255) channel pack with the native shift order (A R G B).
/// Native MakeColor rounds to nearest-even (ls3df.h:83 `fld`/`fistp`,
/// explicitly not `__ftol` truncation), so 127.5 packs as 128.
/// Alpha `(255-mask)/255` uses host division where native multiplies by
/// (1/255): 1-ulp differences on the quotient are possible (disclosed).
fn pack_channel(v: f32) -> Option<u32> {
    if !f32_finite(v) && !v.is_nan() {
        return None;
    }
    if v.is_nan() {
        return None;
    }
    let scaled = v * 255.0;
    if !scaled.is_finite() {
        return None;
    }
    let rounded = scaled.round_ties_even();
    if !(0.0..=255.0).contains(&rounded) {
        return None;
    }
    Some(rounded as u32 & 0xff)
}

fn make_mirror_color(alpha: f32, color: [f32; 3]) -> Option<u32> {
    Some(
        (pack_channel(alpha)? << 24)
            | (pack_channel(color[0])? << 16)
            | (pack_channel(color[1])? << 8)
            | pack_channel(color[2])?,
    )
}

/// Original Draw CPU order, PC RVA 0x31c70 (differential: Ghidra body 262
/// insn/33 BB; mafia-re I3d_mirror.cpp:118-151). Null verts/faces guard →
/// alpha `(255-(flags&0xff))/255` → MakeColor pack → cull select
/// (`0x200`→NONE else `0x400`→CW else CCW). Returns None on the null guard
/// (native draws nothing); device emission stays behind the backend.
pub fn mirror_draw_plan(
    state: &I3dMirrorState,
    color: [f32; 3],
    flags: u32,
) -> Option<MirrorDrawPlan> {
    if state.verts.is_empty() || state.faces.is_empty() {
        return None;
    }
    let alpha = (255 - (flags & 0xff)) as f32 / 255.0;
    if !f32_finite(alpha) {
        return None;
    }
    for lane in color {
        if !f32_finite(lane) {
            return None;
        }
    }
    let packed = make_mirror_color(alpha, color)?;
    // IsAbsMrgZero(a-1): exact opaque bit vs the 0x322bcc77 margin word.
    let opaque = (alpha - 1.0).abs().to_bits() as i32 <= 0x322b_cc77;
    Some(MirrorDrawPlan {
        color: packed,
        alpha_none: opaque,
        cull: if flags & 0x200 != 0 {
            MirrorCullMode::None
        } else if flags & 0x400 != 0 {
            MirrorCullMode::Cw
        } else {
            MirrorCullMode::Ccw
        },
        verts: state.verts.len(),
        faces: state.faces.len(),
    })
}

/// Original Tick walk, PC RVA 0x320f0 (contract: no Ghidra body;
/// mafia-re I3d_mirror.cpp:207-220). Ascending walk with no advance on
/// remove, exactly mirroring native `p = &copies[0]` + `p++`-on-keep: the
/// `for (i=size; i--;)` counter only counts down, traversal starts at the
/// front; `copies.remove(*p)` is order-preserving, releases run in ascending
/// index order. `time` is unused natively.
pub fn mirror_tick_copies(
    copies: &mut Vec<FrameKey>,
    backend: &mut impl MirrorCopyBackend,
) -> Result<(), ModelError> {
    let mut index = 0;
    while index < copies.len() {
        let (refs, has_parent) = backend.copy_target(copies[index])?;
        if refs >= 2 && has_parent {
            index += 1;
            continue;
        }
        let copy = copies[index];
        // Backend releases first (UserData-frame then visual inside the
        // implementor, in native order); removal follows with no advance so
        // the shifted element is re-examined, like native *p.
        backend.release_mirror_copy(copy)?;
        copies.remove(index);
    }
    Ok(())
}

/// Visibility input for UpdateVisibility (PC RVA 0x321b0). All live views are
/// caller-supplied; the pure predicate applies the evidenced gate order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MirrorVisibilityInput {
    pub render_number: u32,
    pub current_render: u32,
    pub world_dir_view: [f32; 3],
    pub fov_a: f32,
    pub world_pos: [f32; 3],
    pub cam_pos: [f32; 3],
    pub range: f32,
    pub fov_dist_correction: f32,
    pub hw_tl: bool,
    pub draw_mirrors: bool,
}
/// Original UpdateVisibility predicate, PC RVA 0x321b0 (differential: Ghidra
/// body 211 insn/35 BB; mafia-re I3d_mirror.cpp:230-248). Pure predicate
/// over caller-supplied live views (world dir/pos, camera, render number,
/// hw_tl + draw-mirrors device words).
pub fn mirror_update_visibility(input: MirrorVisibilityInput) -> Option<MirrorVisibilityVerdict> {
    let MirrorVisibilityInput {
        render_number,
        current_render,
        world_dir_view,
        fov_a,
        world_pos,
        cam_pos,
        range,
        fov_dist_correction,
        hw_tl,
        draw_mirrors,
    } = input;
    if render_number != current_render {
        return Some(MirrorVisibilityVerdict::Hidden);
    }
    for lane in world_dir_view
        .iter()
        .chain([fov_a, range, fov_dist_correction].iter())
        .chain(world_pos.iter())
        .chain(cam_pos.iter())
    {
        if !f32_finite(*lane) {
            return None;
        }
    }
    let mag_sq = world_dir_view[0] * world_dir_view[0]
        + world_dir_view[1] * world_dir_view[1]
        + world_dir_view[2] * world_dir_view[2];
    if mag_sq <= 0.0 || !mag_sq.is_finite() {
        return None;
    }
    let mag = mag_sq.sqrt();
    // Facing-away gate: dir.z > 0 && dir.z/mag > cos(fov_a*0.5) clears both.
    let half_cos = (fov_a * 0.5).cos();
    if world_dir_view[2] > 0.0 && world_dir_view[2] / mag > half_cos {
        return Some(MirrorVisibilityVerdict::FacingAway);
    }
    let dist = fov_dist_correction * range;
    if !dist.is_finite() {
        return None;
    }
    let d = [
        world_pos[0] - cam_pos[0],
        world_pos[1] - cam_pos[1],
        world_pos[2] - cam_pos[2],
    ];
    let sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if !sq.is_finite() {
        return None;
    }
    if sq > dist * dist || !hw_tl || !draw_mirrors {
        return Some(MirrorVisibilityVerdict::Visible { reflect: false });
    }
    Some(MirrorVisibilityVerdict::Visible { reflect: true })
}

/// View-box clip accumulator for CollectVisuals (PC RVA 0x32500,
/// differential: 162 insn/35 BB). Eight bbox corners through
/// `vis.GetMatrix * inv_view_box` with the native bit order
/// (x<1→1, x>-1→2, y<1→4, y>-1→8, z<1→0x10, z>-1→0x20); push iff 0x3f.
pub fn mirror_view_box_clip(
    bbox_min: [f32; 3],
    bbox_max: [f32; 3],
    matrix: &SMatrix,
) -> Option<u32> {
    for lane in bbox_min.iter().chain(bbox_max.iter()) {
        if !f32_finite(*lane) {
            return None;
        }
    }
    let mut clip = 0u32;
    for i in 0..8 {
        let corner = [
            if i & 1 == 0 { bbox_min[0] } else { bbox_max[0] },
            if i & 2 == 0 { bbox_min[1] } else { bbox_max[1] },
            if i & 4 == 0 { bbox_min[2] } else { bbox_max[2] },
        ];
        let p = matrix.transform_point(corner);
        if !p.iter().all(|v| f32_finite(*v)) {
            return None;
        }
        if p[0] < 1.0 {
            clip |= 1;
        }
        if p[0] > -1.0 {
            clip |= 2;
        }
        if p[1] < 1.0 {
            clip |= 4;
        }
        if p[1] > -1.0 {
            clip |= 8;
        }
        if p[2] < 1.0 {
            clip |= 0x10;
        }
        if p[2] > -1.0 {
            clip |= 0x20;
        }
    }
    Some(clip)
}

/// Original CollectVisuals traversal order, PC RVA 0x32500 (differential).
/// Pre-order over caller-supplied child→next links: self-test (flag gate →
/// sphere reject → visual gate → bound clip) then children child→next.
/// `bound_or_update` and the vis-matrix product are caller contracts (the
/// bound-valid recompute has no host owner here); clip uses
/// mirror_view_box_clip.
pub fn mirror_collect(
    state: &I3dMirrorState,
    nodes: &[MirrorCollectNode],
    children: &[Vec<usize>],
    root: usize,
    out: &mut Vec<u32>,
    combined: &[SMatrix],
) -> bool {
    fn visit(
        state: &I3dMirrorState,
        nodes: &[MirrorCollectNode],
        children: &[Vec<usize>],
        at: usize,
        out: &mut Vec<u32>,
        combined: &[SMatrix],
    ) {
        let node = &nodes[at];
        if !node.on_tick {
            return;
        }
        let d = [
            node.center[0] - state.bsphere_center[0],
            node.center[1] - state.bsphere_center[1],
            node.center[2] - state.bsphere_center[2],
        ];
        let rr = node.radius + state.bsphere_radius;
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > rr * rr {
            return;
        }
        if node.is_visual && node.object_no_proxy_mirror {
            let matrix = combined.get(at).copied().unwrap_or(SMatrix::identity());
            if mirror_view_box_clip(node.bbox_min, node.bbox_max, &matrix) == Some(0x3f) {
                out.push(node.id);
            }
        }
        for &child in children.get(at).map(Vec::as_slice).unwrap_or(&[]) {
            if child < nodes.len() {
                visit(state, nodes, children, child, out, combined);
            }
        }
    }
    if root >= nodes.len() {
        return false;
    }
    visit(state, nodes, children, root, out, combined);
    true
}

/// Rebuild failure for the PrepareReflection prefix (PC RVA 0x32730). No
/// device orchestration is attempted on these paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorRebuildError {
    NoViewBox,
    Numeric,
}
/// S_matrix::SetDir basis rows (I3d_math.cpp:632-653, PC RVA 0x2d9d0): the
/// rotation turning the z axis into `dir`. Exact port of the three limbs:
/// row2 = SetNormalized(dir); the sq = dir.z²+dir.x² fast arms; row0 =
/// SetNormalized(dir.z,0,-dir.x) when |dir.z|+|dir.x| > MRG_ZERO; row1 =
/// -(row0.Cross(dir)). MRG_ZERO = 1e-8 (ls3df.h:48); host f32 ops in source
/// order (no fused ops). None on refused (non-finite/subnormal) input.
/// Helper: no function credit.
fn set_dir_rows(dir: [f32; 3]) -> Option<[[f32; 3]; 3]> {
    const MRG_ZERO: f32 = 1e-8;
    for lane in dir {
        if !f32_finite(lane) {
            return None;
        }
    }
    fn normalized(v: [f32; 3]) -> Option<[f32; 3]> {
        const MRG_ZERO: f32 = 1e-8;
        let sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        if !sq.is_finite() {
            return None;
        }
        if (sq - 1.0).abs() < MRG_ZERO {
            return Some(v);
        }
        if sq < MRG_ZERO {
            // Dominant-axis fallback with exact == comparisons, as natively.
            if v[0] != 0.0 {
                return Some([if v[0] < 0.0 { -1.0 } else { 1.0 }, v[1], v[2]]);
            } else if v[2] != 0.0 {
                return Some([v[0], v[1], if v[2] < 0.0 { -1.0 } else { 1.0 }]);
            } else {
                return Some([v[0], if v[1] < 0.0 { -1.0 } else { 1.0 }, v[2]]);
            }
        }
        let inv = 1.0 / sq.sqrt();
        if !inv.is_finite() {
            return None;
        }
        Some([v[0] * inv, v[1] * inv, v[2] * inv])
    }
    let row2 = normalized(dir)?;
    // Row0 starts as the Identity row; the fast arms write two lanes first
    // (overwritten below whenever the second gate fires, exactly as natively).
    let mut row0 = [1.0, 0.0, 0.0];
    let sq = dir[2] * dir[2] + dir[0] * dir[0];
    if sq > MRG_ZERO {
        if (sq - 1.0).abs() < MRG_ZERO {
            row0[0] = dir[2];
            row0[2] = -dir[0];
        } else {
            let f = 1.0 / sq.sqrt();
            if !f.is_finite() {
                return None;
            }
            row0[0] = dir[2] * f;
            row0[2] = -(dir[0] * f);
        }
    }
    if dir[2].abs() + dir[0].abs() > MRG_ZERO {
        row0 = normalized([dir[2], 0.0, -dir[0]])?;
    }
    // row1 = -(row0.Cross(dir)), Cross per I3d_math.h (standard order).
    let cross = [
        row0[1] * dir[2] - row0[2] * dir[1],
        row0[2] * dir[0] - row0[0] * dir[2],
        row0[0] * dir[1] - row0[1] * dir[0],
    ];
    let row1 = [-cross[0], -cross[1], -cross[2]];
    Some([row0, row1, row2])
}
/// Affine inverse of a row-vector SMatrix (3x3 adjugate + `-t*R^-1`
/// translation). Returns None on non-finite input or a singular basis.
/// Host f32 arithmetic in source order (no fused ops); native x87
/// differences are quarantined to this helper (no function credit).
fn affine_inverse(m: &SMatrix) -> Option<SMatrix> {
    let e = &m.elements;
    for lane in e {
        if !f32_finite(*lane) {
            return None;
        }
    }
    // Basis rows r0..r2 of the 3x3.
    let (r0, r1, r2) = ([e[0], e[1], e[2]], [e[4], e[5], e[6]], [e[8], e[9], e[10]]);
    let det = r0[0] * (r1[1] * r2[2] - r1[2] * r2[1]) - r0[1] * (r1[0] * r2[2] - r1[2] * r2[0])
        + r0[2] * (r1[0] * r2[1] - r1[1] * r2[0]);
    if !det.is_finite() || det == 0.0 {
        return None;
    }
    let inv = 1.0 / det;
    if !inv.is_finite() {
        return None;
    }
    // Adjugate rows scaled by 1/det.
    let b0 = [
        (r1[1] * r2[2] - r1[2] * r2[1]) * inv,
        (r0[2] * r2[1] - r0[1] * r2[2]) * inv,
        (r0[1] * r1[2] - r0[2] * r1[1]) * inv,
    ];
    let b1 = [
        (r1[2] * r2[0] - r1[0] * r2[2]) * inv,
        (r0[0] * r2[2] - r0[2] * r2[0]) * inv,
        (r0[2] * r1[0] - r0[0] * r1[2]) * inv,
    ];
    let b2 = [
        (r1[0] * r2[1] - r1[1] * r2[0]) * inv,
        (r0[1] * r2[0] - r0[0] * r2[1]) * inv,
        (r0[0] * r1[1] - r0[1] * r1[0]) * inv,
    ];
    let t = [e[12], e[13], e[14]];
    let it = [
        -(t[0] * b0[0] + t[1] * b1[0] + t[2] * b2[0]),
        -(t[0] * b0[1] + t[1] * b1[1] + t[2] * b2[1]),
        -(t[0] * b0[2] + t[1] * b1[2] + t[2] * b2[2]),
    ];
    let mut out = SMatrix::identity();
    out.elements[0] = b0[0];
    out.elements[1] = b0[1];
    out.elements[2] = b0[2];
    out.elements[4] = b1[0];
    out.elements[5] = b1[1];
    out.elements[6] = b1[2];
    out.elements[8] = b2[0];
    out.elements[9] = b2[1];
    out.elements[10] = b2[2];
    out.elements[12] = it[0];
    out.elements[13] = it[1];
    out.elements[14] = it[2];
    if !out.elements.iter().all(|v| f32_finite(*v)) {
        return None;
    }
    Some(out)
}
/// Original PrepareReflection matrix rebuild prefix, PC RVA 0x32730
/// (differential: 727 insn/99 BB; mafia-re I3d_mirror.cpp:316-333). Pure
/// prefix: VIEW_BOX clear → not-prepared; else set PREPARED,
/// `m = view_box * world`, bsphere center from m(3) and radius from
/// `(1,1,1).RotateByMatrix(m)`, `inv_view_box = Inverse(m)`, then the mm arm
/// `m.SetDir(dir); m(3) = pos` (SetDir modeled exactly, including the
/// roll-discarding reconstruction), `mirror_matrix = Inverse(mm) * flip(mm)`
/// with `flip(2) *= -1` over the 3 vector lanes. `dir`/`pos` are the live
/// GetWorldDir/GetWorldPos views supplied by the caller. The Collect/Clear/
/// draw/copy orchestration stays behind the device backend; affine_inverse
/// and set_dir_rows are helpers with no function credit.
pub fn mirror_rebuild_matrices(
    state: &mut I3dMirrorState,
    world: &SMatrix,
    dir: [f32; 3],
    pos: [f32; 3],
) -> Result<bool, MirrorRebuildError> {
    if state.is_prepared() {
        return Ok(false);
    }
    if !state.has_view_box() {
        return Err(MirrorRebuildError::NoViewBox);
    }
    for lane in world.elements.iter().chain(dir.iter()).chain(pos.iter()) {
        if !f32_finite(*lane) {
            return Err(MirrorRebuildError::Numeric);
        }
    }
    state.mirror_flags |= MIRF_PREPARED;
    let m = state.view_box.mul_affine(world);
    state.bsphere_center = [m.elements[12], m.elements[13], m.elements[14]];
    // Native r = (1,1,1).RotateByMatrix(m), radius_2 = r.r: the rotation-only
    // image of (1,1,1), i.e. the column sum of the 3x3 basis. transform_point
    // adds translation, so the difference of the two images isolates it.
    let img = m.transform_point([1.0, 1.0, 1.0]);
    let org = m.transform_point([0.0, 0.0, 0.0]);
    let r = [img[0] - org[0], img[1] - org[1], img[2] - org[2]];
    let sq = r[0] * r[0] + r[1] * r[1] + r[2] * r[2];
    if !sq.is_finite() || sq < 0.0 {
        return Err(MirrorRebuildError::Numeric);
    }
    state.bsphere_radius = sq.sqrt();
    let inv = affine_inverse(&m).ok_or(MirrorRebuildError::Numeric)?;
    state.inv_view_box = inv;
    let rows = set_dir_rows(dir).ok_or(MirrorRebuildError::Numeric)?;
    let mut mm = *world;
    mm.elements[0] = rows[0][0];
    mm.elements[1] = rows[0][1];
    mm.elements[2] = rows[0][2];
    mm.elements[4] = rows[1][0];
    mm.elements[5] = rows[1][1];
    mm.elements[6] = rows[1][2];
    mm.elements[8] = rows[2][0];
    mm.elements[9] = rows[2][1];
    mm.elements[10] = rows[2][2];
    mm.elements[12] = pos[0];
    mm.elements[13] = pos[1];
    mm.elements[14] = pos[2];
    let mm_inv = affine_inverse(&mm).ok_or(MirrorRebuildError::Numeric)?;
    // flip(2) *= -1: S_vector 3 lanes only (slots 8,9,10; [11] untouched).
    let mut flip = mm;
    for col in 0..3 {
        flip.elements[8 + col] = -flip.elements[8 + col];
    }
    state.mirror_matrix = mm_inv.mul_affine(&flip);
    Ok(true)
}

/// Original SetViewBox store order, PC RVA 0x331b0 (contract: no Ghidra body;
/// mafia-re I3d_mirror.cpp:428-436). `view_box = m` scaled per-row by half
/// extents, translation `(max+min)*0.5 * view_box`; flags
/// `(flags&~PREPARED)|VIEW_BOX`.
pub fn mirror_set_view_box(
    state: &mut I3dMirrorState,
    bb_min: [f32; 3],
    bb_max: [f32; 3],
    view: SMatrix,
) -> bool {
    for lane in bb_min.iter().chain(bb_max.iter()) {
        if !f32_finite(*lane) {
            return false;
        }
    }
    let mut out = view;
    let half = [
        (bb_max[0] - bb_min[0]) * 0.5,
        (bb_max[1] - bb_min[1]) * 0.5,
        (bb_max[2] - bb_min[2]) * 0.5,
    ];
    if !half.iter().all(|v| f32_finite(*v)) {
        return false;
    }
    // Native scales view_box(0/1/2) (S_vector rows, 3 lanes each); the
    // affine-column slots [3,7,11] are left untouched.
    for (row, scale) in half.iter().enumerate() {
        for col in 0..3 {
            out.elements[row * 4 + col] *= *scale;
        }
    }
    let center = [
        (bb_max[0] + bb_min[0]) * 0.5,
        (bb_max[1] + bb_min[1]) * 0.5,
        (bb_max[2] + bb_min[2]) * 0.5,
    ];
    let translated = out.transform_point(center);
    out.elements[12] = translated[0];
    out.elements[13] = translated[1];
    out.elements[14] = translated[2];
    state.view_box = out;
    state.mirror_flags = (state.mirror_flags & !MIRF_PREPARED) | MIRF_VIEW_BOX;
    true
}

impl ModelArena {
    fn mirror_state_ref(&self, key: FrameKey) -> Result<&I3dMirrorState, ModelError> {
        self.frame(key)?
            .mirror
            .as_ref()
            .ok_or(ModelError::MirrorFrameRequired(key))
    }
    fn mirror_state_mut(&mut self, key: FrameKey) -> Result<&mut I3dMirrorState, ModelError> {
        let index = self.frame_index(key)?;
        self.frames[index]
            .mirror
            .as_mut()
            .ok_or(ModelError::MirrorFrameRequired(key))
    }
    fn mirror_sector_scene(
        &self,
        mut current: Option<FrameKey>,
    ) -> Result<Option<SceneKey>, ModelError> {
        while let Some(frame) = current {
            if self.state(frame)?.kind == FrameKind::Sector {
                return Ok(self.sector(SectorKey(frame))?.scene);
            }
            current = self.links(frame)?.parent;
        }
        Ok(None)
    }

    /// Original ctor, PC RVA 0x31820 (differential: 42 insn/1 BB + base-ctor /
    /// Identity callees). Order: base visual ctor effects (unported, RVA
    /// 0x5a200) precede; host stores copies-empty, residue word, Identity
    /// view_box, tag, scene None, range, zero color/counts/vectors/flags.
    pub fn create_mirror_frame(&mut self) -> Result<FrameKey, ModelError> {
        let mut state = I3dFrameState::model();
        debug_assert!(state.kind == FrameKind::Model);
        state.visual_flags = 0x2200;
        let key = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        let index = self.frame_index(key)?;
        self.frames[index].mirror = Some(I3dMirrorState::constructor());
        Ok(key)
    }

    /// Original dtor, PC RVA 0x31910 (differential: 60 insn/7 BB). Order-exact:
    /// scene detach via scene_delete_mirror; drop verts/faces vectors (host
    /// analog of the delete[] pair, infallible); per-copy backend releases in
    /// list order; clear copies + scene. Base visual dtor (0x5a370) stays with
    /// its producer.
    pub fn destroy_mirror_derived(
        &mut self,
        key: FrameKey,
        backend: &mut impl MirrorCopyBackend,
    ) -> Result<(), ModelError> {
        self.mirror_state_ref(key)?;
        let scene = self.mirror_state_ref(key)?.scene;
        if let Some(scene) = scene {
            self.scene_delete_mirror(scene, key)?;
        }
        let copies = std::mem::take(&mut self.mirror_state_mut(key)?.copies);
        for copy in &copies {
            backend.release_mirror_copy(*copy)?;
        }
        let state = self.mirror_state_mut(key)?;
        state.verts.clear();
        state.faces.clear();
        state.scene = None;
        Ok(())
    }

    /// Original SetColor, PC RVA 0x319f0 (contract: no Ghidra body; entry/mid/
    /// RET x87 clamp probes). Store then clamp each lane to [0,1]; NaN
    /// preserved per the JP/JNZ probe trace.
    #[allow(clippy::manual_clamp)]
    pub fn set_mirror_color(&mut self, key: FrameKey, color: [f32; 3]) -> Result<(), ModelError> {
        self.mirror_state_ref(key)?;
        let state = self.mirror_state_mut(key)?;
        state.color = color;
        for lane in state.color.iter_mut() {
            if *lane < 0.0 {
                *lane = 0.0;
            } else if *lane > 1.0 {
                *lane = 1.0;
            }
        }
        Ok(())
    }

    /// Original Duplicate, PC RVA 0x31ab0 (contract: no body; probes evidence
    /// the guard word paths, release loop, delete pair, base Duplicate call,
    /// FRAME_VISUAL check). Order: null/self → 0x80000007, no effects;
    /// release-but-retain copies (native "released, but stay in the list",
    /// mafia-re I3d_mirror.cpp:73 — the stale keys are pruned by the next
    /// Tick walk); clear geometry; duplicate_frame tail; non-mirror src →
    /// OK; mirror src → view_box/flags/geometry/bound/range/color copy with
    /// unconditional (vis&~WBOUND)|BOUND_VALID. The mirror tail past
    /// 0x10031b45 is upstream-corroborated with the byte tail disclosed as
    /// unprobed.
    pub fn duplicate_mirror_frame(
        &mut self,
        dst: FrameKey,
        src: FrameKey,
        backend: &mut impl MirrorCopyBackend,
    ) -> Result<u32, ModelError> {
        self.frame(src).map_err(|_| ModelError::UnknownFrame(src))?;
        self.frame(dst).map_err(|_| ModelError::UnknownFrame(dst))?;
        if src == dst {
            return Ok(MIRROR_INVALIDPARAMS);
        }
        let copies = self.mirror_state_ref(dst)?.copies.clone();
        for copy in &copies {
            backend.release_mirror_copy(*copy)?;
        }
        {
            let state = self.mirror_state_mut(dst)?;
            state.verts.clear();
            state.faces.clear();
        }
        self.duplicate_frame(dst, src)?;
        if self.frame(src)?.mirror.is_none() || self.frame(dst)?.mirror.is_none() {
            return Ok(MIRROR_OK);
        }
        let source = self.mirror_state_ref(src)?.clone();
        let dst_state = self.mirror_state_mut(dst)?;
        // Native copies view_box, flags (cleared PREPARED, set VIEW_BOX),
        // verts/faces, range and color only (mafia-re I3d_mirror.cpp:96-108):
        // the prepared-cache words (inv_view_box, mirror_matrix, bsphere,
        // render_number) keep their dst residue, exactly as natively.
        dst_state.view_box = source.view_box;
        dst_state.mirror_flags = (dst_state.mirror_flags & !MIRF_PREPARED) | MIRF_VIEW_BOX;
        dst_state.verts = source.verts;
        dst_state.faces = source.faces;
        dst_state.range = source.range;
        dst_state.color = source.color;
        let di = self.frame_index(dst)?;
        if let Some(origin) = self.frame(src)?.visual_bounds {
            self.frames[di].visual_bounds = Some(origin);
        }
        self.graph
            .set_visual_flag_words(dst, VISF_WBOUND_VALID, VISF_BOUND_VALID)
            .map_err(ModelError::from)?;
        Ok(MIRROR_OK)
    }

    /// Original LinkTo, PC RVA 0x32000 (differential: 33 insn/8 BB). Order:
    /// DeleteMirror if attached; sector walk + AddMirror; base
    /// link_frame_base; UpdateV; base word. Native UpdateV needs no device,
    /// so no backend threads through here.
    pub fn link_mirror_frame(
        &mut self,
        key: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, ModelError> {
        self.mirror_state_ref(key)?;
        if let Some(scene) = self.mirror_state_ref(key)?.scene {
            self.scene_delete_mirror(scene, key)?;
            self.mirror_state_mut(key)?.scene = None;
        }
        if let Some(scene) = self.mirror_sector_scene(parent)? {
            self.scene_add_mirror(scene, key)?;
            self.mirror_state_mut(key)?.scene = Some(scene);
        }
        let result = self.link_frame_base(key, parent, flags)?;
        self.update_mirror_frame(key)?;
        Ok(result)
    }

    /// Original UpdateV, PC RVA 0x32060 (contract: no body; TEST/AND/loop/CALL
    /// probes). Dirty (flags & 0x40002000): clear 0x1c0000 + PREPARED, stamp
    /// copies stale (unknown copy keys are skipped, not propagated —
    /// hardening, disclosed); sceneless: sector walk + AddMirror. Copies
    /// stamped via live graph state.
    pub fn update_mirror_frame(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.mirror_state_ref(key)?;
        if self.state(key)?.flags & MIRROR_DIRTY_TEST != 0 {
            self.graph
                .set_visual_flag_words(key, MIRROR_UPDATE_CLEAR_MASK, 0)
                .map_err(ModelError::from)?;
            self.mirror_state_mut(key)?.mirror_flags &= !MIRF_PREPARED;
            let copies = self.mirror_state_ref(key)?.copies.clone();
            for copy in copies {
                let _ = self.graph.mark_copy_stale(copy);
            }
        }
        if self.mirror_state_ref(key)?.scene.is_none() {
            let parent = self.links(key)?.parent;
            if let Some(scene) = self.mirror_sector_scene(parent)? {
                self.scene_add_mirror(scene, key)?;
                self.mirror_state_mut(key)?.scene = Some(scene);
            }
        }
        Ok(())
    }

    /// Original LoadImage, PC RVA 0x332e0 (differential: 55 insn/1 BB). Order:
    /// base image via backend; 0x28 B vis_bound + (vis&~WBOUND)|BOUND_VALID;
    /// 0x58 B view_box/color/range/counts + (mirf&~PREPARED)|VIEW_BOX; new
    /// verts (n x12) + read; new faces (m x6, 3xu16) + read. Widths from the
    /// LEA/SHL immediates. dtaRead/operator_new = backend/host-Vec
    /// (checked_mul → AllocationOverflow, preceding stores retained; a short
    /// backend read maps to AllocationOverflow too — a truncation/contract
    /// violation with no closer host word, disclosed).
    pub fn load_mirror_image(
        &mut self,
        key: FrameKey,
        source: &mut impl MirrorImageBackend,
    ) -> Result<(), ModelError> {
        self.mirror_state_ref(key)?;
        source.load_base_frame_image(key)?;
        let bound = source.read_mirror_bytes(0x28)?;
        if bound.len() != 0x28 {
            return Err(ModelError::AllocationOverflow);
        }
        let f = |off: usize| f32::from_le_bytes(bound[off..off + 4].try_into().unwrap());
        let volume = MeshBoundVolume {
            bbox: [[f(0), f(4), f(8)], [f(12), f(16), f(20)]],
            center: [f(24), f(28), f(32)],
            radius: f(36),
        };
        let di = self.frame_index(key)?;
        self.frames[di].visual_bounds = Some(volume);
        self.graph
            .set_visual_flag_words(key, VISF_WBOUND_VALID, VISF_BOUND_VALID)
            .map_err(ModelError::from)?;
        let block = source.read_mirror_bytes(0x58)?;
        if block.len() != 0x58 {
            return Err(ModelError::AllocationOverflow);
        }
        let g = |off: usize| f32::from_le_bytes(block[off..off + 4].try_into().unwrap());
        let mut view = [0f32; 16];
        for (i, v) in view.iter_mut().enumerate() {
            *v = g(i * 4);
        }
        let color = [g(0x40), g(0x44), g(0x48)];
        let range = g(0x4c);
        // I3D.h:3784-3786: num_verts at +0x50, num_faces at +0x54.
        let num_verts = u32::from_le_bytes(block[0x50..0x54].try_into().unwrap());
        let num_faces = u32::from_le_bytes(block[0x54..0x58].try_into().unwrap());
        {
            let state = self.mirror_state_mut(key)?;
            state.view_box = SMatrix { elements: view };
            state.color = color;
            state.range = range;
            state.mirror_flags = (state.mirror_flags & !MIRF_PREPARED) | MIRF_VIEW_BOX;
        }
        let vert_bytes = (num_verts as usize)
            .checked_mul(12)
            .ok_or(ModelError::AllocationOverflow)?;
        let raw_verts = source.read_mirror_bytes(vert_bytes)?;
        if raw_verts.len() != vert_bytes {
            return Err(ModelError::AllocationOverflow);
        }
        let mut verts = Vec::new();
        verts
            .try_reserve(num_verts as usize)
            .map_err(|_| ModelError::AllocationOverflow)?;
        let (chunks, remainder) = raw_verts.as_chunks::<12>();
        debug_assert!(remainder.is_empty());
        for chunk in chunks {
            let mut v = [0f32; 3];
            for (i, lane) in v.iter_mut().enumerate() {
                *lane = f32::from_le_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
            }
            verts.push(v);
        }
        let face_bytes = (num_faces as usize)
            .checked_mul(6)
            .ok_or(ModelError::AllocationOverflow)?;
        let raw_faces = source.read_mirror_bytes(face_bytes)?;
        if raw_faces.len() != face_bytes {
            return Err(ModelError::AllocationOverflow);
        }
        let mut faces = Vec::new();
        faces
            .try_reserve(num_faces as usize)
            .map_err(|_| ModelError::AllocationOverflow)?;
        let (chunks, remainder) = raw_faces.as_chunks::<6>();
        debug_assert!(remainder.is_empty());
        for chunk in chunks {
            let mut t = [0u16; 3];
            for (i, lane) in t.iter_mut().enumerate() {
                *lane = u16::from_le_bytes(chunk[i * 2..i * 2 + 2].try_into().unwrap());
            }
            faces.push(t);
        }
        let state = self.mirror_state_mut(key)?;
        state.verts = verts;
        state.faces = faces;
        Ok(())
    }

    /// Original Tick, PC RVA 0x320f0 (contract: no Ghidra body; mafia-re
    /// I3d_mirror.cpp:207-220). Delegates to the index walk with no advance
    /// on remove; `time` is unused natively.
    pub fn mirror_tick(
        &mut self,
        key: FrameKey,
        backend: &mut impl MirrorCopyBackend,
    ) -> Result<(), ModelError> {
        self.mirror_state_ref(key)?;
        let index = self.frame_index(key)?;
        mirror_tick_copies(
            &mut self.frames[index].mirror.as_mut().unwrap().copies,
            backend,
        )
    }

    /// Mirror visibility write-back for UpdateVisibility (PC RVA 0x321b0,
    /// differential). The predicate core is the pure mirror_update_visibility;
    /// this only stores the flag words in native order (mismatch clears
    /// VISIBLE alone, facing-away clears VISIBLE|REFLECT).
    pub fn mirror_store_visibility(
        &mut self,
        key: FrameKey,
        verdict: MirrorVisibilityVerdict,
    ) -> Result<(), ModelError> {
        self.mirror_state_ref(key)?;
        let state = self.mirror_state_mut(key)?;
        match verdict {
            MirrorVisibilityVerdict::Hidden => {
                state.mirror_flags &= !MIRF_VISIBLE;
            }
            MirrorVisibilityVerdict::FacingAway => {
                state.mirror_flags &= !(MIRF_VISIBLE | MIRF_REFLECT);
            }
            MirrorVisibilityVerdict::Visible { reflect } => {
                state.mirror_flags |= MIRF_VISIBLE;
                if reflect {
                    state.mirror_flags |= MIRF_REFLECT;
                } else {
                    state.mirror_flags &= !MIRF_REFLECT;
                }
            }
        }
        Ok(())
    }

    /// Mirror reflection rebuild write-back for PrepareReflection (PC RVA
    /// 0x32730, differential). Pure prefix only; orchestration stays behind
    /// the device backend. Mirror-domain failures keep their own error
    /// (NoViewBox/Numeric), never an allocation word.
    pub fn mirror_rebuild(
        &mut self,
        key: FrameKey,
        world: &SMatrix,
        dir: [f32; 3],
        pos: [f32; 3],
    ) -> Result<bool, MirrorRebuildError> {
        self.mirror_state_ref(key)
            .map_err(|_| MirrorRebuildError::Numeric)?;
        let index = self
            .frame_index(key)
            .map_err(|_| MirrorRebuildError::Numeric)?;
        mirror_rebuild_matrices(self.frames[index].mirror.as_mut().unwrap(), world, dir, pos)
    }

    /// Mirror view-box write-back for SetViewBox (PC RVA 0x331b0, contract).
    pub fn mirror_store_view_box(
        &mut self,
        key: FrameKey,
        bb_min: [f32; 3],
        bb_max: [f32; 3],
        view: SMatrix,
    ) -> Result<bool, ModelError> {
        self.mirror_state_ref(key)?;
        let index = self.frame_index(key)?;
        Ok(mirror_set_view_box(
            self.frames[index].mirror.as_mut().unwrap(),
            bb_min,
            bb_max,
            view,
        ))
    }

    pub fn mirror_state(&self, key: FrameKey) -> Result<&I3dMirrorState, ModelError> {
        self.mirror_state_ref(key)
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the clamped color triple the occupied `SetColor` setter (PC RVA
    /// 0x319f0; mafia-re ls3df/I3D/visuals/I3d_mirror.cpp at pin 113b5b6)
    /// stores. The original exposes no `I3D_mirror::GetColor` (that file
    /// defines only `SetColor`/`SetViewBox`); same guard as `mirror_state`.
    pub fn mirror_color_of(&self, key: FrameKey) -> Result<[f32; 3], ModelError> {
        Ok(self.mirror_state(key)?.color)
    }
    /// Upstream working name, not an evidenced original symbol: gated read
    /// of the `view_box` the occupied `SetViewBox` setter (PC RVA 0x331b0)
    /// stores, reported only while the `MIRF_VIEW_BOX` flag it sets is
    /// present. The original exposes no `I3D_mirror::GetViewBox`; same guard
    /// as `mirror_state`.
    pub fn mirror_view_box_of(&self, key: FrameKey) -> Result<Option<SMatrix>, ModelError> {
        let state = self.mirror_state(key)?;
        if state.has_view_box() {
            Ok(Some(state.view_box))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct LogBackend {
        released: Vec<FrameKey>,
        live: std::collections::BTreeMap<u32, (u32, bool)>,
        base_images: Vec<FrameKey>,
        reads: Vec<Vec<u8>>,
    }
    impl MirrorCopyBackend for LogBackend {
        fn release_mirror_copy(&mut self, copy: FrameKey) -> Result<(), ModelError> {
            self.released.push(copy);
            Ok(())
        }
        fn copy_target(&self, copy: FrameKey) -> Result<(u32, bool), ModelError> {
            Ok(self.live.get(&copy.0).copied().unwrap_or((0, false)))
        }
    }
    impl MirrorImageBackend for LogBackend {
        fn load_base_frame_image(&mut self, key: FrameKey) -> Result<(), ModelError> {
            self.base_images.push(key);
            Ok(())
        }
        fn read_mirror_bytes(&mut self, len: usize) -> Result<Vec<u8>, ModelError> {
            assert!(!self.reads.is_empty());
            let out = self.reads.remove(0);
            assert_eq!(out.len(), len);
            Ok(out)
        }
    }
    fn log() -> LogBackend {
        LogBackend {
            released: Vec::new(),
            live: std::collections::BTreeMap::new(),
            base_images: Vec::new(),
            reads: Vec::new(),
        }
    }

    #[test]
    fn ctor_stores_native_words_in_order() {
        let mut arena = ModelArena::default();
        let key = arena.create_mirror_frame().unwrap();
        let s = arena.mirror_state_ref(key).unwrap();
        assert_eq!(s.visual_tag, 0x5252_494d);
        assert_eq!(s.residue_1f0, 0x20);
        assert_eq!(s.range.to_bits(), 0x42c8_0000);
        assert_eq!(s.view_box, SMatrix::identity());
        assert!(s.verts.is_empty() && s.faces.is_empty());
        assert_eq!(s.mirror_flags, 0);
        assert_eq!(s.color, [0.; 3]);
        assert!(s.scene.is_none() && s.copies.is_empty());
        assert!(arena.frame(key).unwrap().visual_bounds.is_none());
    }

    #[test]
    fn mirror_color_reader_reports_ctor_default_then_setter_store() {
        let mut arena = ModelArena::default();
        let key = arena.create_mirror_frame().unwrap();
        assert_eq!(arena.mirror_color_of(key).unwrap(), [0.; 3]);
        arena.set_mirror_color(key, [0.25, 0.5, 0.75]).unwrap();
        assert_eq!(arena.mirror_color_of(key).unwrap(), [0.25, 0.5, 0.75]);
        arena.set_mirror_color(key, [-1.0, 2.0, f32::NAN]).unwrap();
        let color = arena.mirror_color_of(key).unwrap();
        assert_eq!(color[0].to_bits(), 0.0f32.to_bits());
        assert_eq!(color[1].to_bits(), 1.0f32.to_bits());
        assert!(color[2].is_nan());
    }

    #[test]
    fn mirror_view_box_reader_is_none_until_set_view_box_stores() {
        let mut arena = ModelArena::default();
        let key = arena.create_mirror_frame().unwrap();
        assert_eq!(arena.mirror_view_box_of(key).unwrap(), None);
        assert!(arena
            .mirror_store_view_box(key, [-1., -1., -1.], [1., 1., 1.], SMatrix::identity())
            .unwrap());
        assert_eq!(
            arena.mirror_view_box_of(key).unwrap(),
            Some(SMatrix::identity())
        );
    }

    #[test]
    fn set_color_roundtrips_and_clamps_with_nan_preserved() {
        let mut arena = ModelArena::default();
        let key = arena.create_mirror_frame().unwrap();
        arena.set_mirror_color(key, [0.25, 0.5, 0.75]).unwrap();
        assert_eq!(
            arena.mirror_state_ref(key).unwrap().color,
            [0.25, 0.5, 0.75]
        );
        arena.set_mirror_color(key, [-1.0, 2.0, f32::NAN]).unwrap();
        let c = arena.mirror_state_ref(key).unwrap().color;
        assert_eq!(c[0].to_bits(), 0.0f32.to_bits());
        assert_eq!(c[1].to_bits(), 1.0f32.to_bits());
        assert!(c[2].is_nan());
    }

    #[test]
    fn duplicate_guard_word_leaves_dst_untouched() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let dst = arena.create_mirror_frame().unwrap();
        arena.set_mirror_color(dst, [0.1, 0.2, 0.3]).unwrap();
        let before = arena.mirror_state_ref(dst).unwrap().clone();
        assert_eq!(
            arena
                .duplicate_mirror_frame(dst, dst, &mut backend)
                .unwrap(),
            0x8000_0007
        );
        assert!(backend.released.is_empty());
        assert_eq!(arena.mirror_state_ref(dst).unwrap(), &before);
    }

    #[test]
    fn duplicate_deep_copies_and_stays_independent() {
        let mut arena = ModelArena::default();
        let mut backend = log();
        let src = arena.create_mirror_frame().unwrap();
        let dst = arena.create_mirror_frame().unwrap();
        // A stale dst copy is released but stays in the list (native residue).
        arena
            .mirror_state_mut(dst)
            .unwrap()
            .copies
            .push(FrameKey(77));
        {
            let s = arena.mirror_state_mut(src).unwrap();
            s.verts = vec![[1., 2., 3.], [4., 5., 6.]];
            s.faces = vec![[0, 1, 0]];
            s.range = 50.0;
            s.color = [0.2, 0.4, 0.6];
            s.mirror_flags = 0x10;
        }
        assert_eq!(
            arena
                .duplicate_mirror_frame(dst, src, &mut backend)
                .unwrap(),
            0
        );
        assert_eq!(backend.released, vec![FrameKey(77)]);
        assert_eq!(
            arena.mirror_state_ref(dst).unwrap().copies,
            vec![FrameKey(77)]
        );
        let (s, d) = (
            arena.mirror_state_ref(src).unwrap().clone(),
            arena.mirror_state_ref(dst).unwrap().clone(),
        );
        assert_eq!(d.verts, s.verts);
        assert_eq!(d.faces, s.faces);
        assert_eq!(d.view_box, s.view_box);
        assert_eq!(d.mirror_flags & 0x1, 0x1);
        assert_eq!(d.mirror_flags & 0x4, 0);
        assert_eq!(d.range.to_bits(), s.range.to_bits());
        arena
            .mirror_state_mut(src)
            .unwrap()
            .verts
            .push([9., 9., 9.]);
        assert_eq!(arena.mirror_state_ref(dst).unwrap().verts.len(), 2);
    }

    #[test]
    fn draw_null_and_flag_order() {
        let mut arena = ModelArena::default();
        let key = arena.create_mirror_frame().unwrap();
        let empty = arena.mirror_state_ref(key).unwrap().clone();
        assert!(mirror_draw_plan(&empty, [0.5, 0.5, 0.5], 0).is_none());
        let mut tri = empty;
        tri.verts = vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        tri.faces = vec![[0, 1, 2]];
        let plan = mirror_draw_plan(&tri, [1.0, 0.0, 0.0], 0).unwrap();
        assert_eq!(plan.color, 0xffff_0000);
        assert!(plan.alpha_none);
        assert_eq!(plan.cull, MirrorCullMode::Ccw);
        let nocull = mirror_draw_plan(&tri, [1.0, 0.0, 0.0], 0x200).unwrap();
        assert_eq!(nocull.cull, MirrorCullMode::None);
        let cw = mirror_draw_plan(&tri, [1.0, 0.0, 0.0], 0x400).unwrap();
        assert_eq!(cw.cull, MirrorCullMode::Cw);
        // FloatToInt round-to-nearest-even (native MakeColor): 127.5 packs
        // as 128 (2.5->2, 3.5->4 likewise).
        assert_eq!(pack_channel(0.5), Some(128));
        assert_eq!(pack_channel(2.5 / 255.0), Some(2));
        assert_eq!(pack_channel(3.5 / 255.0), Some(4));
        assert_eq!(pack_channel(1.0), Some(255));
        assert_eq!(pack_channel(0.0), Some(0));
    }

    #[test]
    fn visibility_true_false_and_render_mismatch() {
        // Facing camera (dir -z): visible + reflect when in range with HWTL.
        let input = MirrorVisibilityInput {
            render_number: 7,
            current_render: 7,
            world_dir_view: [0.0, 0.0, -1.0],
            fov_a: 1.0,
            world_pos: [0.0, 0.0, 0.0],
            cam_pos: [0.0, 0.0, 5.0],
            range: 100.0,
            fov_dist_correction: 1.0,
            hw_tl: true,
            draw_mirrors: true,
        };
        let verdict = mirror_update_visibility(input).unwrap();
        assert_eq!(verdict, MirrorVisibilityVerdict::Visible { reflect: true });
        // Facing away clears both VISIBLE and REFLECT.
        assert_eq!(
            mirror_update_visibility(MirrorVisibilityInput {
                world_dir_view: [0.0, 0.0, 1.0],
                fov_a: 0.5,
                cam_pos: [0.0, 0.0, -5.0],
                ..input
            })
            .unwrap(),
            MirrorVisibilityVerdict::FacingAway
        );
        // Render-number mismatch clears VISIBLE alone.
        assert_eq!(
            mirror_update_visibility(MirrorVisibilityInput {
                current_render: 8,
                ..input
            })
            .unwrap(),
            MirrorVisibilityVerdict::Hidden
        );
        // Write-back keeps the arms distinct: Hidden preserves REFLECT.
        let mut arena = ModelArena::default();
        let key = arena.create_mirror_frame().unwrap();
        arena.mirror_state_mut(key).unwrap().mirror_flags = MIRF_VISIBLE | MIRF_REFLECT;
        arena
            .mirror_store_visibility(key, MirrorVisibilityVerdict::Hidden)
            .unwrap();
        assert_eq!(
            arena.mirror_state_ref(key).unwrap().mirror_flags,
            MIRF_REFLECT
        );
        arena
            .mirror_store_visibility(key, MirrorVisibilityVerdict::FacingAway)
            .unwrap();
        assert_eq!(arena.mirror_state_ref(key).unwrap().mirror_flags, 0);
    }

    #[test]
    fn collect_order_and_clip() {
        let mut state = I3dMirrorState::constructor();
        state.bsphere_center = [0., 0., 0.];
        state.bsphere_radius = 10.0;
        let nodes = vec![
            MirrorCollectNode {
                id: 1,
                on_tick: true,
                center: [0., 0., 0.],
                radius: 1.0,
                is_visual: true,
                object_no_proxy_mirror: true,
                bbox_min: [-0.5, -0.5, -0.5],
                bbox_max: [0.5, 0.5, 0.5],
            },
            MirrorCollectNode {
                id: 2,
                on_tick: true,
                center: [100., 0., 0.],
                radius: 1.0,
                is_visual: true,
                object_no_proxy_mirror: true,
                bbox_min: [-0.5, -0.5, -0.5],
                bbox_max: [0.5, 0.5, 0.5],
            },
        ];
        let children = vec![vec![1], vec![]];
        let combined = vec![SMatrix::identity(), SMatrix::identity()];
        let mut out = Vec::new();
        assert!(mirror_collect(
            &state, &nodes, &children, 0, &mut out, &combined
        ));
        assert_eq!(out, vec![1]);
        assert_eq!(
            mirror_view_box_clip([-2., -2., -2.], [2., 2., 2.], &SMatrix::identity()).unwrap(),
            0x3f
        );
    }

    #[test]
    fn reflection_matrix_spots_and_inverse_roundtrip() {
        // Identity view_box + identity world: mirror = diag(1,1,-1,1),
        // inv_view_box = identity, radius = sqrt(3).
        let mut state = I3dMirrorState::constructor();
        assert!(mirror_set_view_box(
            &mut state,
            [-1., -1., -1.],
            [1., 1., 1.],
            SMatrix::identity()
        ));
        assert!(mirror_rebuild_matrices(
            &mut state,
            &SMatrix::identity(),
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0]
        )
        .unwrap());
        assert_eq!(
            [
                state.mirror_matrix.elements[0],
                state.mirror_matrix.elements[5],
                state.mirror_matrix.elements[10],
                state.mirror_matrix.elements[15]
            ],
            [1.0, 1.0, -1.0, 1.0]
        );
        assert_eq!(state.inv_view_box, SMatrix::identity());
        assert!((state.bsphere_radius - 3f32.sqrt()).abs() < 1e-5);
        // Affine inverse roundtrip on a translated/scaled sample.
        let mut sample = SMatrix::identity();
        sample.elements[0] = 2.0;
        sample.elements[5] = 0.5;
        sample.elements[12] = 4.0;
        sample.elements[13] = -6.0;
        let inv = affine_inverse(&sample).unwrap();
        let back = sample.mul_affine(&inv);
        for (got, want) in back
            .elements
            .iter()
            .zip(SMatrix::identity().elements.iter())
        {
            assert!((got - want).abs() < 1e-5, "{got} != {want}");
        }
        // Singular basis is refused, never a matrix answer.
        let mut singular = SMatrix::identity();
        singular.elements[5] = 0.0;
        singular.elements[6] = 0.0;
        singular.elements[4] = 0.0;
        assert!(affine_inverse(&singular).is_none());
        // Non-uniform scale distinguishes the (1,1,1) column-sum norm from
        // partial-row formulations: r=(2,3,4), radius=sqrt(29).
        let mut scaled = I3dMirrorState::constructor();
        scaled.mirror_flags = super::MIRF_VIEW_BOX;
        scaled.view_box = SMatrix::identity();
        scaled.view_box.elements[0] = 2.0;
        scaled.view_box.elements[5] = 3.0;
        scaled.view_box.elements[10] = 4.0;
        assert!(mirror_rebuild_matrices(
            &mut scaled,
            &SMatrix::identity(),
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0]
        )
        .unwrap());
        assert!((scaled.bsphere_radius - 29f32.sqrt()).abs() < 1e-4);
        // SetDir reconstructs the basis from dir alone (roll discarded):
        // z→x maps rows to (0,0,-1),(0,1,0),(1,0,0); z→z is identity.
        assert_eq!(
            set_dir_rows([0.0, 0.0, 1.0]).unwrap(),
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert_eq!(
            set_dir_rows([1.0, 0.0, 0.0]).unwrap(),
            [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]
        );
    }

    #[test]
    fn tick_residue_and_view_box_math() {
        let mut copies = vec![FrameKey(1), FrameKey(2), FrameKey(3)];
        let mut backend = log();
        backend.live.insert(1, (2, true));
        backend.live.insert(2, (1, false));
        backend.live.insert(3, (0, false));
        mirror_tick_copies(&mut copies, &mut backend).unwrap();
        assert_eq!(copies, vec![FrameKey(1)]);
        // Ascending release order (native p-from-front walk).
        assert_eq!(backend.released, vec![FrameKey(2), FrameKey(3)]);
        let mut state = I3dMirrorState::constructor();
        assert!(mirror_rebuild_matrices(
            &mut state,
            &SMatrix::identity(),
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0]
        )
        .is_err());
        assert!(mirror_set_view_box(
            &mut state,
            [-1., -2., -3.],
            [3., 2., 1.],
            SMatrix::identity()
        ));
        assert!(state.has_view_box() && !state.is_prepared());
        assert_eq!(
            [
                state.view_box.elements[12],
                state.view_box.elements[13],
                state.view_box.elements[14]
            ],
            [2.0, 0.0, -2.0]
        );
        // Affine-column slots survive the row scale untouched.
        let mut skewed = SMatrix::identity();
        skewed.elements[3] = 5.0;
        skewed.elements[7] = 6.0;
        skewed.elements[11] = 7.0;
        assert!(mirror_set_view_box(
            &mut state,
            [-1., -1., -1.],
            [1., 1., 1.],
            skewed
        ));
        assert_eq!(
            [
                state.view_box.elements[3],
                state.view_box.elements[7],
                state.view_box.elements[11]
            ],
            [5.0, 6.0, 7.0]
        );
        assert!(mirror_rebuild_matrices(
            &mut state,
            &SMatrix::identity(),
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0]
        )
        .unwrap());
        assert!(!mirror_rebuild_matrices(
            &mut state,
            &SMatrix::identity(),
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0]
        )
        .unwrap());
    }

    #[test]
    fn load_image_parses_counts_vectors_and_flag_words() {
        fn f32le(v: f32, out: &mut Vec<u8>) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let mut arena = ModelArena::default();
        let key = arena.create_mirror_frame().unwrap();
        let mut bound = vec![];
        for v in [-1., -2., -3., 4., 5., 6., 0., 0., 0.] {
            f32le(v, &mut bound);
        }
        f32le(7.5, &mut bound);
        let mut block = vec![];
        for i in 0..16 {
            f32le(i as f32, &mut block);
        }
        for v in [0.2f32, 0.4, 0.6, 42.0] {
            f32le(v, &mut block);
        }
        // num_verts then num_faces words at 0x50/0x54 per I3D.h:3784-3786.
        block.extend_from_slice(&2u32.to_le_bytes());
        block.extend_from_slice(&1u32.to_le_bytes());
        let mut vb = vec![];
        for v in [1., 2., 3., 4., 5., 6.] {
            f32le(v, &mut vb);
        }
        let fb = vec![0u8, 0, 1, 0, 0, 0];
        let mut backend = log();
        backend.reads = vec![bound, block, vb, fb];
        arena.load_mirror_image(key, &mut backend).unwrap();
        assert_eq!(backend.base_images, [key]);
        let vol = arena.frame(key).unwrap().visual_bounds.unwrap();
        assert_eq!(vol.bbox, [[-1., -2., -3.], [4., 5., 6.]]);
        assert_eq!(vol.radius.to_bits(), 7.5f32.to_bits());
        let s = arena.mirror_state_ref(key).unwrap();
        assert_eq!(s.mirror_flags & 0x5, 0x1);
        assert_eq!(s.range.to_bits(), 42.0f32.to_bits());
        assert_eq!(s.color, [0.2, 0.4, 0.6]);
        assert_eq!(s.verts, [[1., 2., 3.], [4., 5., 6.]]);
        assert_eq!(s.faces, [[0, 1, 0]]);
    }
}
