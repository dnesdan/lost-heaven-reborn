#![forbid(unsafe_code)]
//! Clip-space projection and per-vertex lighting contracts.
//!
//! PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs (all CONTRACT tier, new module this wave): 0x1010, 0x10c0, 0x10f0,
//! 0x1370, 0x13e0, 0x2530, 0x3b40, 0x3dc0, 0x4110, 0x4370, 0x45f0, 0x619b0,
//! 0x65a60, 0x65a70, 0x66b50, 0x66be0, 0x66e90, 0x67d20, 0x685d0, 0x68760.
//!
//! Reference (upstream working names, never original symbols unless noted):
//! mafia-re ls3df/I3D/render/clipper.cpp (params/arrays), clipper2.cpp
//! (ortho/persp faces), clipping.cpp (mesh entry points), RadixSort.cpp
//! (radix sort), T_l.cpp (lighting/vertex helpers); headers clipper.h,
//! RadixSort.h, I3D.h, I3d_math.h, ls3df.h. Sibling revision ab67b13.
//!
//! State ownership: global clip state lives in the narrow [`ClipState`]
//! adapter; lighting globals live in [`LightState`]; radix scratch lives in
//! [`SortStorage`]. Semantic APIs borrow live state with named accessors and
//! never expose `Vec<u8>` CPU maps for clip objects. Byte slices appear only
//! at explicit binary boundaries (fog bytes, vertex-record swap bytes,
//! texture-examined texels are typed). `reserve_clip_arrays` grows only.
//! [`clip_facegroups`] owns vertex-index assignment, `center_t` accumulation
//! and overflow rollback.
//!
//! Effect order per probe: transform, flag, cull, near/far, x, y,
//! tessellate/triangulate, lift, spread, append, index, result-copy.
//!
//! Precision: exact `f32` with VC6 x87 evaluation order. Parenthesizations
//! are load-bearing and kept: ortho spread `((d.z*d.z + d.x*d.x) + d.y*d.y)`,
//! persp lift `(n.x*n.x + (n.y*n.y + n.z*n.z))`, center scaling through
//! component-wise `operator*=` with `f = 1.0 / n`. The spread degenerate
//! branch writes `n.z` where `d.z` is meant (native bug, kept and disclosed).
//! `t` numerator forms are kept per plane
//! (`(0.0 - a.z)`, `(1.0 - a.z)`, `(-1.0 - a.x)`, `(a.x + a.z)`, ...).
//! `FloatToInt` is round-to-nearest-even (`fld`/`fistp`, ls3df.h:83); light
//! packs clamp to `0xff` where the native clamps (`test ecx,0xffffff00`)
//! and keep the low byte where it stores `al` (fog). No fused operations,
//! no `f64` anywhere in this module.
//!
//! Withheld (not ported here): 0x1000 synthetic `$E5` ctor, `CheckClipFaces`
//! (linker-dropped; rides with 0x3dc0 as a private helper), clipping2.cpp
//! (marker only), inlined helpers (RadixSort helpers, T_l inlines, covered by
//! parents), 0x68360/0x68430 (ledger overlap: singlemesh_deformation symbols,
//! do not duplicate), 0x68840 (no mafia-re reconstruction).

// ---------------------------------------------------------------------------
// Words and flags (I3D.h / ls3df.h / clipper.h corroboration).
// ---------------------------------------------------------------------------

/// Marginal-zero threshold (ls3df.h:48).
pub const MRG_ZERO: f32 = 1e-8;
/// Marginal-zero bit mask (ls3df.h:49). Signed integer compare threshold.
pub const MRG_ZERO_BITMASK: i32 = 0x322b_cc77;
/// `1.0 / reverse` substitute when `reverse` is marginally zero or negative.
pub const INV_REVERSE_SUBSTITUTE: f32 = 1e8;
/// Reverse factor above which backfaces are kept (SetClipParams).
pub const BACKFACE_REVERSE_LIMIT: f32 = 0.999;

/// Clip plane bits (clipper.h:22-27).
pub const CLIP_LEFT: u32 = 0x1;
/// Clip plane bits (clipper.h:22-27).
pub const CLIP_RIGHT: u32 = 0x2;
/// Clip plane bits (clipper.h:22-27).
pub const CLIP_BOTTOM: u32 = 0x4;
/// Clip plane bits (clipper.h:22-27).
pub const CLIP_TOP: u32 = 0x8;
/// Clip plane bits (clipper.h:22-27).
pub const CLIP_NEAR: u32 = 0x10;
/// Clip plane bits (clipper.h:22-27).
pub const CLIP_FAR: u32 = 0x20;

/// Clipper array bounds (clipper.h:31-34).
pub const CLIP_MIN_VERTICES: usize = 512;
/// Clipper array bounds (clipper.h:31-34).
pub const CLIP_MAX_VERTICES: usize = 65000;
/// Clipper array bounds (clipper.h:31-34).
pub const CLIP_MIN_FACES: usize = 128;
/// Clipper array bounds (clipper.h:31-34).
pub const CLIP_MAX_FACES: usize = 32000;
/// Polygon scratch length (clipper.h polygon buffers).
pub const CLIP_POLY_SCRATCH: usize = 512;

/// Material words read by the clipper (I3D.h:3868-3870, 3866).
pub const MATF_TWO_SIDED: u32 = 0x1000_0000;
/// Material words read by the clipper (I3D.h:3868-3870, 3866).
pub const MATF_CKEY_ALPHA: u32 = 0x2000_0000;
/// Material words read by the clipper (I3D.h:3868-3870, 3866).
pub const MATF_TXT_ALPHA: u32 = 0x4000_0000;
/// Material words read by the clipper (I3D.h:3868-3870, 3866).
pub const MATF_ADD_MODE: u32 = 0x8000_0000;
/// Material words read by lighting (I3D.h:3855, 3864).
pub const MATF_TEXTURE: u32 = 0x0004_0000;
/// Material words read by lighting (I3D.h:3855, 3864).
pub const MATF_KEEP_DIFFUSE: u32 = 0x0800_0000;
/// Mesh-level words (I3D.h:4063-4064).
pub const MESHF_TEXTURE: u32 = 0x10;
/// Mesh-level words (I3D.h:4063-4064).
pub const MESHF_KEEP_DIFFUSE: u32 = 0x20;

/// Light type words (I3D.h:1877-1888).
pub const LIGHT_NULL: u32 = 0;
/// Light type words (I3D.h:1877-1888).
pub const LIGHT_POINT: u32 = 1;
/// Light type words (I3D.h:1877-1888).
pub const LIGHT_SPOT: u32 = 2;
/// Light type words (I3D.h:1877-1888).
pub const LIGHT_DIRECTIONAL: u32 = 3;
/// Light type words (I3D.h:1877-1888).
pub const LIGHT_AMBIENT: u32 = 4;
/// Light type words (I3D.h:1877-1888).
pub const LIGHT_FOG: u32 = 5;
/// Light type words (I3D.h:1877-1888).
pub const LIGHT_POINTAMBIENT: u32 = 6;
/// Light type words (I3D.h:1877-1888).
pub const LIGHT_POINTFOG: u32 = 7;

/// Light mode words (I3D.h:1891-1899).
pub const LIGHTMODE_VERTEX: u32 = 0x1;
/// Light mode words (I3D.h:1891-1899).
pub const LIGHTMODE_LIGHTMAP: u32 = 0x2;
/// Light mode words (I3D.h:1891-1899).
pub const LIGHTMODE_SHADOW: u32 = 0x20;
/// Frame flag word (I3D.h:259).
pub const FRMFLAGS_ON: u32 = 0x1;
/// Visual flag words (I3D.h:2683, 2689).
pub const VISF_TEXTURE_LOD: u32 = 0x10;
/// Visual flag words (I3D.h:2683, 2689).
pub const VISF_AVERAGE_LIGHT: u32 = 0x80;
/// Lightmap shadow-test flag (I3D.h:3170).
pub const LIGHT_GEO_COLLISIONS: u32 = 0x1;

/// Texel callback outcomes (I3D.h:1403-1405).
pub const TEXEL_OPAQUE: i32 = 0;
/// Texel callback outcomes (I3D.h:1403-1405).
pub const TEXEL_PARTIAL: i32 = 1;
/// Texel callback outcomes (I3D.h:1403-1405).
pub const TEXEL_TRANSPARENT: i32 = 2;

/// Environment modes (I3D.h:4068-4073).
pub const ENVMODE_REFLECT_Z: u32 = 0;
/// Environment modes (I3D.h:4068-4073).
pub const ENVMODE_REFLECT_Y: u32 = 1;
/// Environment modes (I3D.h:4068-4073).
pub const ENVMODE_PROJECT_Y: u32 = 2;
/// Environment modes (I3D.h:4068-4073).
pub const ENVMODE_PROJECT_X: u32 = 3;
/// Environment modes (I3D.h:4068-4073).
pub const ENVMODE_PROJECT_Z: u32 = 4;
/// Environment modes (I3D.h:4068-4073).
pub const ENVMODE_DUMMY: u32 = 5;

/// Fog modes (I3D.h:4552-4553).
pub const FOGPROC_FACTOR: u32 = 0;
/// Fog modes (I3D.h:4552-4553).
pub const FOGPROC_ALPHA: u32 = 1;

/// Kept-light cap (T_l.cpp:78).
pub const MAX_TEMP_LIGHTS: usize = 8;
/// Default vertex-color scale (T_l.cpp:62, 128 or 255 without MODULATE2X).
pub const DEFAULT_VERTEX_LIGHT_MULT: f32 = 128.0;
/// Default perspective tessellation depth (I3d_driver.cpp:223).
pub const DEFAULT_NON_ORTHO_TESS: f32 = 0.02;

// ---------------------------------------------------------------------------
// Shared math shapes (row-vector convention, D3D element order).
// ---------------------------------------------------------------------------

/// 4x4 row-vector matrix (`m[row * 4 + col]`, translation at 12/13/14).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipMatrix {
    /// Row-major elements in D3D order.
    pub m: [f32; 16],
}

impl ClipMatrix {
    /// Identity matrix.
    pub fn identity() -> Self {
        let mut m = [0.0f32; 16];
        m[0] = 1.0f32;
        m[5] = 1.0f32;
        m[10] = 1.0f32;
        m[15] = 1.0f32;
        Self { m }
    }

    /// Diagonal scale matrix (translation zero).
    pub fn from_scale(sx: f32, sy: f32, sz: f32) -> Self {
        let mut m = [0.0f32; 16];
        m[0] = sx;
        m[5] = sy;
        m[10] = sz;
        m[15] = 1.0f32;
        Self { m }
    }
}

/// One clip-space vertex: mesh vertex or clipping-made vertex (0x14 bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipVertex {
    /// Position in clip space.
    pub pos: [f32; 3],
    /// Result-vertex index, -1 until a face uses it.
    pub index: i32,
    /// `CLIP_*` planes it lies beyond.
    pub clip: u32,
}

impl Default for ClipVertex {
    fn default() -> Self {
        Self {
            pos: [0.0f32; 3],
            index: -1,
            clip: 0,
        }
    }
}

/// One triangle as three vertex indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triface {
    /// Vertex indices.
    pub vi: [u16; 3],
}

/// A face group's material terms read by the clipper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipMaterial {
    /// `MATF_*` words.
    pub flags: u32,
    /// Material alpha.
    pub alpha: f32,
}

/// One face group: faces plus optional material.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceGroup {
    /// Group faces in mesh-vertex indices.
    pub faces: Vec<Triface>,
    /// Group material (`None` means untextured default).
    pub material: Option<ClipMaterial>,
}

/// Narrow adapter owning every clipper global (clipper.cpp/clipper2.cpp).
///
/// `faces`/`faces_t` split the native single face block (`faces[num * 2]`,
/// transparent half at `faces + num`) into two typed owners so overflow is
/// checked instead of overflowing; the split is disclosed. `vert_end`
/// mirrors `clip_vert_end` as an index. `poly_buf1/2` are scratch.
#[derive(Clone, Debug)]
pub struct ClipState {
    /// Copy of the clip-space matrix (`clip_matrix`).
    pub matrix: ClipMatrix,
    /// Copy of the projector matrix (`clip_world`).
    pub world: ClipMatrix,
    /// Whether [`set_clip_params`] has run.
    pub has_params: bool,
    /// Unit-box volume (`clip_is_ortho`).
    pub is_ortho: bool,
    /// Normal offset of new vertices (`clip_lift`).
    pub lift: f32,
    /// Center spread of new vertices (`clip_spread`).
    pub spread: f32,
    /// Reverse factor (`clip_rev_factor`).
    pub rev_factor: f32,
    /// Its inverse (`clip_inv_reverse`).
    pub inv_reverse: f32,
    /// No face is culled (`clip_backfaces`).
    pub backfaces: bool,
    /// Projector scale ratios (`clip_ratio_x/y`).
    pub ratio_x: f32,
    /// Projector scale ratios (`clip_ratio_x/y`).
    pub ratio_y: f32,
    /// Clip vertices (`clip_verts`, length == `verts_size`).
    pub verts: Vec<ClipVertex>,
    /// Allocated vertex slots (`clip_vertices_size`).
    pub verts_size: usize,
    /// Vert-count cutoff (`clip_verts_max`).
    pub verts_max: usize,
    /// Used vertices: mesh plus clipping-made (`clip_vert_count`).
    pub vert_count: usize,
    /// Next free vertex index (`clip_vert_end`).
    pub vert_end: usize,
    /// Per-list face high-water mark (`clip_faces_alloc`).
    pub faces_alloc: usize,
    /// Opaque faces (`clip_faces`).
    pub faces: Vec<Triface>,
    /// Transparent faces (`clip_faces_t`).
    pub faces_t: Vec<Triface>,
    /// Used opaque faces (`num_clip_faces`).
    pub num_faces: usize,
    /// Used transparent faces (`num_clip_faces_t`).
    pub num_faces_t: usize,
    /// Next free opaque face (`clip_next_face` as length).
    pub next_face: usize,
    /// Next free transparent face (`clip_next_face_t` as length).
    pub next_face_t: usize,
    /// Opaque faces added by the current [`clip_facegroups`] call.
    pub added_faces: usize,
    /// Transparent faces added by the current [`clip_facegroups`] call.
    pub added_transp: usize,
    /// Current group is transparent (`clip_fg_blend`).
    pub fg_blend: bool,
    /// Current group skips culling (`clip_two_sided`).
    pub two_sided: u32,
    /// Transparent groups are clipped too (`clip_with_alpha`).
    pub with_alpha: u32,
    /// Result vertices (`clip_results`, length == `num_results`).
    pub results: Vec<[f32; 3]>,
    /// Result high-water mark (`clip_result_alloc`).
    pub result_alloc: usize,
    /// Used result vertices (`num_clip_results`).
    pub num_results: usize,
    /// Transparent result divisor, never counted (`num_clip_results_t`).
    pub num_results_t: usize,
    /// Transparent new-vertex sum (`clip_center_t`).
    pub center_t: [f32; 3],
    /// Polygon scratch, every other plane (`clip_poly_buf1`).
    pub poly_buf1: [u16; 512],
    /// Polygon scratch, every other plane (`clip_poly_buf2`).
    pub poly_buf2: [u16; 512],
    /// Live polygon length (`clip_poly_num`).
    pub poly_num: usize,
    /// Frustum edge-split depth (`RsNonOrthoTessellation`).
    pub non_ortho_tess: f32,
}

impl ClipState {
    /// Zero-initialized clipper plus driver default tessellation.
    pub fn new() -> Self {
        Self {
            matrix: ClipMatrix::identity(),
            world: ClipMatrix::identity(),
            has_params: false,
            is_ortho: false,
            lift: 0.0f32,
            spread: 0.0f32,
            rev_factor: 0.0f32,
            inv_reverse: 0.0f32,
            backfaces: false,
            ratio_x: 0.0f32,
            ratio_y: 0.0f32,
            verts: Vec::new(),
            verts_size: 0,
            verts_max: 0,
            vert_count: 0,
            vert_end: 0,
            faces_alloc: 0,
            faces: Vec::new(),
            faces_t: Vec::new(),
            num_faces: 0,
            num_faces_t: 0,
            next_face: 0,
            next_face_t: 0,
            added_faces: 0,
            added_transp: 0,
            fg_blend: false,
            two_sided: 0,
            with_alpha: 0,
            results: Vec::new(),
            result_alloc: 0,
            num_results: 0,
            num_results_t: 0,
            center_t: [0.0f32; 3],
            poly_buf1: [0u16; 512],
            poly_buf2: [0u16; 512],
            poly_num: 0,
            non_ortho_tess: DEFAULT_NON_ORTHO_TESS,
        }
    }
}

impl Default for ClipState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Private float helpers (exact f32, x87 order, no f64, no fused ops).
// ---------------------------------------------------------------------------

/// Native `IsMrgZeroLess`: signed integer compare against the bit mask.
#[inline]
fn is_mrg_zero_less(f: f32) -> bool {
    (f.to_bits() as i32) < MRG_ZERO_BITMASK
}

/// Native `IsAbsMrgZero`: `fabs` then the same signed compare.
#[inline]
fn is_abs_mrg_zero(f: f32) -> bool {
    (f.abs().to_bits() as i32) < MRG_ZERO_BITMASK
}

/// Native `FloatToInt` (ls3df.h:83): `fld`/`fistp` under the engine FPU
/// mode (round-to-nearest-even), explicitly NOT `__ftol` truncation.
/// Out-of-range magnitudes yield `0x80000000` natively; the host `as`
/// saturates instead (disclosed, reachable only at absurd magnitudes).
/// Private helper: no function credit.
#[inline]
fn float_to_int(f: f32) -> i32 {
    f.round_ties_even() as i32
}

/// `v * m` with the native assembly order per component:
/// `((v.x*m00 + m30) + v.y*m10) + v.z*m20`, and so on for y/z.
#[inline]
fn transform_point(p: [f32; 3], m: &ClipMatrix) -> [f32; 3] {
    let x = ((p[0] * m.m[0] + m.m[12]) + p[1] * m.m[4]) + p[2] * m.m[8];
    let y = ((p[0] * m.m[1] + m.m[13]) + p[1] * m.m[5]) + p[2] * m.m[9];
    let z = ((p[0] * m.m[2] + m.m[14]) + p[1] * m.m[6]) + p[2] * m.m[10];
    [x, y, z]
}

/// `RotateByMatrix` order (no translation):
/// `(v.x*m00 + v.y*m10) + v.z*m20`, and so on.
#[inline]
fn rotate_vector(v: [f32; 3], m: &ClipMatrix) -> [f32; 3] {
    let x = (v[0] * m.m[0] + v[1] * m.m[4]) + v[2] * m.m[8];
    let y = (v[0] * m.m[1] + v[1] * m.m[5]) + v[2] * m.m[9];
    let z = (v[0] * m.m[2] + v[1] * m.m[6]) + v[2] * m.m[10];
    [x, y, z]
}

/// Row-vector matrix product with per-element x87 order:
/// `((a0*b0 + a1*b1) + a2*b2) + a3*b3`.
fn mul_matrix(a: &ClipMatrix, b: &ClipMatrix) -> ClipMatrix {
    let mut m = [0.0f32; 16];
    for row in 0..4 {
        for col in 0..4 {
            let v = ((a.m[row * 4] * b.m[col] + a.m[row * 4 + 1] * b.m[4 + col])
                + a.m[row * 4 + 2] * b.m[8 + col])
                + a.m[row * 4 + 3] * b.m[12 + col];
            m[row * 4 + col] = v;
        }
    }
    ClipMatrix { m }
}

/// Row magnitude (`GetScale` per axis): `sqrt((x*x + y*y) + z*z)`.
fn row_magnitude(m: &ClipMatrix, row: usize) -> f32 {
    let b = row * 4;
    ((m.m[b] * m.m[b] + m.m[b + 1] * m.m[b + 1]) + m.m[b + 2] * m.m[b + 2]).sqrt()
}

/// Triangle normal `(w - v) x (u - v)`, not normalized.
fn tri_normal(out: &mut [f32; 3], u1: [f32; 3], v1: [f32; 3], w1: [f32; 3]) {
    let ux = w1[0] - v1[0];
    let uy = w1[1] - v1[1];
    let uz = w1[2] - v1[2];
    let vx = u1[0] - v1[0];
    let vy = u1[1] - v1[1];
    let vz = u1[2] - v1[2];
    out[0] = uy * vz - uz * vy;
    out[1] = uz * vx - ux * vz;
    out[2] = ux * vy - uy * vx;
}

/// Orthogonal clip flags: `x` vs `+-1`, `y` vs `+-1`, `z` vs `0/1`.
fn ortho_flag(p: [f32; 3]) -> u32 {
    let mut c = 0u32;
    if p[0] < -1.0f32 {
        c |= CLIP_LEFT;
    } else if p[0] > 1.0f32 {
        c |= CLIP_RIGHT;
    }
    if p[1] < -1.0f32 {
        c |= CLIP_BOTTOM;
    } else if p[1] > 1.0f32 {
        c |= CLIP_TOP;
    }
    if p[2] < 0.0f32 {
        c |= CLIP_NEAR;
    } else if p[2] > 1.0f32 {
        c |= CLIP_FAR;
    }
    c
}

/// Perspective clip flags: `|x| vs z`, `|y| vs z`, `z` vs `0/1`.
fn persp_flag(p: [f32; 3]) -> u32 {
    if p[2] < 0.0f32 {
        let mut c = CLIP_NEAR;
        let z = -p[2];
        if p[0].abs() > z {
            if p[0] < 0.0f32 {
                c |= CLIP_LEFT;
            } else {
                c |= CLIP_RIGHT;
            }
        }
        if p[1].abs() > z {
            if p[1] < 0.0f32 {
                c |= CLIP_BOTTOM;
            } else {
                c |= CLIP_TOP;
            }
        }
        c
    } else {
        let mut c = 0u32;
        if p[2] > 1.0f32 {
            c |= CLIP_FAR;
        }
        if p[0].abs() > p[2] {
            if p[0] < 0.0f32 {
                c |= CLIP_LEFT;
            } else {
                c |= CLIP_RIGHT;
            }
        }
        if p[1].abs() > p[2] {
            if p[1] < 0.0f32 {
                c |= CLIP_BOTTOM;
            } else {
                c |= CLIP_TOP;
            }
        }
        c
    }
}

/// Append a clipping-made vertex; returns its polygon index.
///
/// The array is pre-grown by [`reserve_clip_arrays`]; if the caller
/// overruns it the host pushes (native would overflow its block; the
/// vertex/face overflow checks below catch the modeled cases first).
fn alloc_vert(state: &mut ClipState, pos: [f32; 3], clip: u32) -> u16 {
    let at = state.vert_count;
    if at < state.verts.len() {
        state.verts[at].pos = pos;
        state.verts[at].clip = clip;
        state.verts[at].index = -1;
    } else {
        state.verts.push(ClipVertex {
            pos,
            index: -1,
            clip,
        });
        state.verts_size = state.verts.len();
    }
    state.vert_count = at + 1;
    state.vert_end = state.vert_count;
    at as u16
}

/// Backface/reverse cull shared by both face clippers.
///
/// Returns `true` when the face is culled. Plain `n.z > 0.0` gate, then
/// `IsMrgZeroLess(rev)` drop, then
/// `lhs = (inv * n.z) * n.z`, `rhs = (x*x + y*y) + n.z*n.z`.
fn cull_face(state: &ClipState, n: [f32; 3]) -> bool {
    if n[2] > 0.0f32 {
        if is_mrg_zero_less(state.rev_factor) {
            return true;
        }
        let x = state.ratio_x * n[0];
        let y = state.ratio_y * n[1];
        let lhs = (state.inv_reverse * n[2]) * n[2];
        let rhs = (x * x + y * y) + n[2] * n[2];
        if lhs > rhs {
            return true;
        }
    }
    false
}

/// Lift offset: degenerate axis snap, else `n * (lift / sqrt(sq))`.
///
/// Ortho callers pass `sq = (n.x*n.x + n.y*n.y) + n.z*n.z`; persp callers
/// pass `sq = n.x*n.x + (n.y*n.y + n.z*n.z)`. The parenthesization is kept.
fn lift_offset(n: [f32; 3], sq: f32, lift: f32) -> [f32; 3] {
    let mut nn = n;
    let off = if sq < MRG_ZERO {
        if nn[0] != 0.0f32 {
            nn[0] = if nn[0] < 0.0f32 { -1.0f32 } else { 1.0f32 };
        } else if nn[2] != 0.0f32 {
            nn[2] = if nn[2] < 0.0f32 { -1.0f32 } else { 1.0f32 };
        } else {
            nn[1] = if nn[1] < 0.0f32 { -1.0f32 } else { 1.0f32 };
        }
        [nn[0] * lift, nn[1] * lift, nn[2] * lift]
    } else {
        let f = lift / sq.sqrt();
        [nn[0] * f, nn[1] * f, nn[2] * f]
    };
    off
}

/// Spread step for one new vertex, including the native `n.z`-for-`d.z` bug.
///
/// `sq` parenthesization is the caller's (ortho
/// `((d.z*d.z + d.x*d.x) + d.y*d.y)`, persp `(d.x*d.x + d.y*d.y) + d.z*d.z`).
/// `n` is the face normal, mutated by the bug arm exactly as native.
fn spread_step(d: [f32; 3], sq: f32, spread: f32, n: &mut [f32; 3]) -> [f32; 3] {
    if sq < MRG_ZERO {
        let mut dd = d;
        if dd[0] != 0.0f32 {
            dd[0] = if dd[0] < 0.0f32 { -1.0f32 } else { 1.0f32 };
        } else if dd[2] != 0.0f32 {
            // Native bug, kept: sets n.z instead of d.z, leaving d.z as is.
            n[2] = if dd[2] < 0.0f32 { -1.0f32 } else { 1.0f32 };
        } else {
            dd[1] = if dd[1] < 0.0f32 { -1.0f32 } else { 1.0f32 };
        }
        // Component-wise operator*= with spread.
        dd[0] *= spread;
        dd[1] *= spread;
        dd[2] *= spread;
        dd
    } else {
        let f = spread / sq.sqrt();
        let mut dd = d;
        dd[0] *= f;
        dd[1] *= f;
        dd[2] *= f;
        dd
    }
}

// ---------------------------------------------------------------------------
// 0x1010 SetClipParams (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Set the next clipping's parameters (working guess, NOT original).
///
/// RVA 0x1010. Reference: mafia-re ls3df/I3D/render/clipper.cpp:151-167.
/// Native stores live matrix pointers (later caller mutation is visible);
/// the host snapshots both matrices (safe-Rust adaptation, disclosed).
/// Copies both matrices, stores the ortho flag/lift/spread/reverse, derives
/// `z/x` and `z/y` scale ratios, `1/reverse` (or `1e8` below `MRG_ZERO`) and
/// the backface flag (`reverse > 0.999`).
pub fn set_clip_params(
    state: &mut ClipState,
    m: &ClipMatrix,
    world: &ClipMatrix,
    ortho: bool,
    offset: f32,
    expand: f32,
    reverse: f32,
) {
    state.matrix = *m;
    state.world = *world;
    state.has_params = true;
    state.is_ortho = ortho;
    state.lift = offset;
    state.spread = expand;
    state.rev_factor = reverse;
    let sx = row_magnitude(world, 0);
    let sy = row_magnitude(world, 1);
    let sz = row_magnitude(world, 2);
    state.ratio_x = sz / sx;
    state.ratio_y = sz / sy;
    if reverse < MRG_ZERO {
        state.inv_reverse = INV_REVERSE_SUBSTITUTE;
    } else {
        state.inv_reverse = 1.0f32 / reverse;
    }
    state.backfaces = reverse > BACKFACE_REVERSE_LIMIT;
}

// ---------------------------------------------------------------------------
// 0x10c0 ResetClipArrays (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Empty the result before meshes are clipped (working guess, NOT original).
///
/// RVA 0x10c0. Reference: mafia-re ls3df/I3D/render/clipper.cpp:172-179.
/// Zeroes result/face counts and `center_t`; allocations are untouched.
pub fn reset_clip_arrays(state: &mut ClipState) {
    state.num_results = 0;
    state.num_faces = 0;
    state.num_faces_t = 0;
    state.center_t = [0.0f32; 3];
    state.num_results_t = 0;
    state.added_faces = 0;
    state.added_transp = 0;
    state.poly_num = 0;
}

// ---------------------------------------------------------------------------
// 0x10f0 ReserveClipArrays (upstream name, NOT original).
// ---------------------------------------------------------------------------

/// Room for a mesh of vertices/faces; grows only (upstream name).
///
/// RVA 0x10f0. Reference: mafia-re ls3df/I3D/render/clipper.cpp:184-238.
/// Vertex need is `verts + faces * 12` (scaled by `(1/tess + 1)` in
/// perspective when `tess < 1`), clamped to `65000` and at least `512`;
/// face need is `faces * 8` (same scale) plus used opaque faces, clamped to
/// `32000` and at least `128`. Existing faces are preserved; the vertex
/// cutoff keeps room for one last 15-vertex face. Host allocation failure
/// aborts rather than returning `false` (disclosed, including the native
/// OOM dbgPrintf which has no sink); oversized requests
/// clamp exactly as native, so `true` is returned here.
pub fn reserve_clip_arrays(state: &mut ClipState, vert_num: usize, face_num: usize) -> bool {
    if state.verts_size < CLIP_MAX_VERTICES {
        let mut num = vert_num + face_num * 12;
        if !state.is_ortho && state.non_ortho_tess < 1.0f32 {
            num = ((1.0f32 / state.non_ortho_tess + 1.0f32) * num as f32) as usize;
        }
        if num > CLIP_MAX_VERTICES {
            num = CLIP_MAX_VERTICES;
        }
        if state.verts_size < num {
            if num < CLIP_MIN_VERTICES {
                num = CLIP_MIN_VERTICES;
            }
            state.verts_size = num;
            state.verts.resize(num, ClipVertex::default());
            if state.vert_count > num {
                state.vert_count = num;
            }
            if !state.is_ortho && state.non_ortho_tess < 1.0f32 {
                let keep = ((1.0f32 / state.non_ortho_tess + 1.0f32) * 15.0f32) as usize;
                state.verts_max = state.verts_size.saturating_sub(keep);
            } else {
                state.verts_max = state.verts_size.saturating_sub(15);
            }
        }
    }
    if state.faces_alloc < CLIP_MAX_FACES {
        let mut num = face_num * 8;
        if !state.is_ortho && state.non_ortho_tess < 1.0f32 {
            num = ((1.0f32 / state.non_ortho_tess + 1.0f32) * num as f32) as usize;
        }
        num += state.num_faces;
        if num > CLIP_MAX_FACES {
            num = CLIP_MAX_FACES;
        }
        if state.faces_alloc < num {
            if num < CLIP_MIN_FACES {
                num = CLIP_MIN_FACES;
            }
            state.faces.reserve(num.saturating_sub(state.faces.len()));
            state
                .faces_t
                .reserve(num.saturating_sub(state.faces_t.len()));
            state.faces_alloc = num;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// 0x1370 FreeClipArrays (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Free the clip arrays, keeping the native face-block leak.
///
/// RVA 0x1370. Reference: mafia-re ls3df/I3D/render/clipper.cpp:243-255.
/// The vertex and result blocks are dropped; the face block is leaked with
/// `mem::forget` exactly as the original skips `delete[]` on it. Host also
/// clears counts that would otherwise dangle into the leaked block (native
/// leaves them stale until the next reset; disclosed).
pub fn free_clip_arrays(state: &mut ClipState) {
    state.verts = Vec::new();
    state.verts_size = 0;
    state.verts_max = 0;
    state.vert_count = 0;
    state.vert_end = 0;
    // Native leak, kept: the opaque/transparent face block is not freed.
    let leaked = core::mem::take(&mut state.faces);
    core::mem::forget(leaked);
    let leaked_t = core::mem::take(&mut state.faces_t);
    core::mem::forget(leaked_t);
    state.num_faces = 0;
    state.num_faces_t = 0;
    state.next_face = 0;
    state.next_face_t = 0;
    state.results = Vec::new();
    state.num_results = 0;
    state.result_alloc = 0;
    state.faces_alloc = 0;
}

// ---------------------------------------------------------------------------
// Polygon-pass scratch shared by both face clippers.
// ---------------------------------------------------------------------------

/// Local ping-pong polygon (mirrors the two 512-entry native buffers).
struct Polygon {
    /// Current polygon buffer.
    cur: [u16; 513],
    /// Next polygon buffer.
    nxt: [u16; 513],
    /// Live vertex count.
    len: usize,
}

impl Polygon {
    /// Triangle polygon closed with the first index.
    fn triangle(vi: [u16; 3]) -> Self {
        let mut cur = [0u16; 513];
        cur[0] = vi[0];
        cur[1] = vi[1];
        cur[2] = vi[2];
        cur[3] = vi[0];
        Self {
            cur,
            nxt: [0u16; 513],
            len: 3,
        }
    }

    /// Close `nxt` with the polygon start, swap buffers, publish length.
    fn close_swap(&mut self, out: usize) {
        self.nxt[out] = self.cur[0];
        core::mem::swap(&mut self.cur, &mut self.nxt);
        self.len = out;
    }

    /// Re-collect plane flags over the live polygon.
    fn recollect(&self, state: &ClipState) -> u32 {
        if self.len == 0 {
            return 0;
        }
        let mut clip = state.verts[self.cur[0] as usize].clip;
        for i in 1..self.len {
            clip |= state.verts[self.cur[i] as usize].clip;
        }
        clip
    }
}

// ---------------------------------------------------------------------------
// 0x13e0 ClipFacesOrtho (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Clip faces to the orthogonal unit box (working guess, NOT original).
///
/// RVA 0x13e0. Reference: mafia-re ls3df/I3D/render/clipper2.cpp:61-476.
/// Order per face: trivial-reject flag test, backface/reverse cull,
/// near/far pass, x pass, y pass, fan triangulation, normal lift, center
/// spread (with the `n.z`-for-`d.z` bug), append to the blended/opaque list.
/// New z-plane vertices recompute x/y flags with the `(0.0 - a.z)` /
/// `(1.0 - a.z)` `t` forms; x-plane vertices recompute y flags only;
/// y-plane vertices keep `clip = 0` (native leaves the slot stale; the host
/// clears it, disclosed). Stops adding faces once `vert_count >= verts_max`.
pub fn clip_faces_ortho(state: &mut ClipState, faces: &[Triface]) {
    let first_vert = state.vert_count;
    let start_o = state.faces.len();
    let start_t = state.faces_t.len();
    let mut pending: Vec<Triface> = Vec::new();

    for face in faces {
        let i0 = face.vi[0] as usize;
        let i1 = face.vi[1] as usize;
        let i2 = face.vi[2] as usize;
        if i0 >= state.verts.len() || i1 >= state.verts.len() || i2 >= state.verts.len() {
            continue;
        }
        let p0 = state.verts[i0].pos;
        let p1 = state.verts[i1].pos;
        let p2 = state.verts[i2].pos;
        let c0 = state.verts[i0].clip;
        let c1 = state.verts[i1].clip;
        let c2 = state.verts[i2].clip;
        if (c0 & c1 & c2) != 0 {
            continue;
        }
        if state.two_sided == 0 {
            let mut n = [0.0f32; 3];
            tri_normal(&mut n, p0, p1, p2);
            if cull_face(state, n) {
                continue;
            }
        }
        let mut clip = c0 | c1 | c2;
        if clip == 0 {
            pending.push(*face);
            continue;
        }
        let mut poly = Polygon::triangle(face.vi);
        // Near/far pass.
        if (clip & (CLIP_NEAR | CLIP_FAR)) != 0 {
            let mut out = 0usize;
            let mut n = poly.len;
            let mut pos = 0usize;
            while n > 0 {
                n -= 1;
                let ia = poly.cur[pos] as usize;
                let ib = poly.cur[pos + 1] as usize;
                let a = state.verts[ia];
                let b = state.verts[ib];
                if (a.clip & CLIP_NEAR) != 0 {
                    if (b.clip & CLIP_NEAR) == 0 {
                        if (b.clip & CLIP_FAR) != 0 {
                            let t = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let mut f = 0u32;
                            if x < -1.0f32 {
                                f |= CLIP_LEFT;
                            } else if x > 1.0f32 {
                                f |= CLIP_RIGHT;
                            }
                            if y < -1.0f32 {
                                f |= CLIP_BOTTOM;
                            } else if y > 1.0f32 {
                                f |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [x, y, 0.0f32], f);
                            out += 1;
                            let t2 = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                            let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                            let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                            let mut f2 = 0u32;
                            if x2 < -1.0f32 {
                                f2 |= CLIP_LEFT;
                            } else if x2 > 1.0f32 {
                                f2 |= CLIP_RIGHT;
                            }
                            if y2 < -1.0f32 {
                                f2 |= CLIP_BOTTOM;
                            } else if y2 > 1.0f32 {
                                f2 |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [x2, y2, 1.0f32], f2);
                            out += 1;
                        } else {
                            let t = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let mut f = 0u32;
                            if x < -1.0f32 {
                                f |= CLIP_LEFT;
                            } else if x > 1.0f32 {
                                f |= CLIP_RIGHT;
                            }
                            if y < -1.0f32 {
                                f |= CLIP_BOTTOM;
                            } else if y > 1.0f32 {
                                f |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [x, y, 0.0f32], f);
                            out += 1;
                        }
                    }
                } else if (a.clip & CLIP_FAR) != 0 {
                    if (b.clip & CLIP_NEAR) != 0 {
                        let t = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < -1.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 1.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 1.0f32], f);
                        out += 1;
                        let t2 = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                        let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                        let mut f2 = 0u32;
                        if x2 < -1.0f32 {
                            f2 |= CLIP_LEFT;
                        } else if x2 > 1.0f32 {
                            f2 |= CLIP_RIGHT;
                        }
                        if y2 < -1.0f32 {
                            f2 |= CLIP_BOTTOM;
                        } else if y2 > 1.0f32 {
                            f2 |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x2, y2, 0.0f32], f2);
                        out += 1;
                    } else if (b.clip & CLIP_FAR) == 0 {
                        let t = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < -1.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 1.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 1.0f32], f);
                        out += 1;
                    }
                } else {
                    poly.nxt[out] = poly.cur[pos];
                    out += 1;
                    if (b.clip & CLIP_NEAR) != 0 {
                        let t = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < -1.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 1.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 0.0f32], f);
                        out += 1;
                    } else if (b.clip & CLIP_FAR) != 0 {
                        let t = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < -1.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 1.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 1.0f32], f);
                        out += 1;
                    }
                }
                pos += 1;
            }
            poly.close_swap(out);
            clip = poly.recollect(state);
        }
        // X pass.
        if (clip & (CLIP_LEFT | CLIP_RIGHT)) != 0 {
            let mut out = 0usize;
            let mut n = poly.len;
            let mut pos = 0usize;
            while n > 0 {
                n -= 1;
                let ia = poly.cur[pos] as usize;
                let ib = poly.cur[pos + 1] as usize;
                let a = state.verts[ia];
                let b = state.verts[ib];
                if (a.clip & CLIP_LEFT) != 0 {
                    if (b.clip & CLIP_LEFT) == 0 {
                        if (b.clip & CLIP_RIGHT) != 0 {
                            let t = (-1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                            let mut f = 0u32;
                            if y < -1.0f32 {
                                f |= CLIP_BOTTOM;
                            } else if y > 1.0f32 {
                                f |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [-1.0f32, y, z], f);
                            out += 1;
                            let t2 = (1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                            let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                            let z2 = (b.pos[2] - a.pos[2]) * t2 + a.pos[2];
                            let mut f2 = 0u32;
                            if y2 < -1.0f32 {
                                f2 |= CLIP_BOTTOM;
                            } else if y2 > 1.0f32 {
                                f2 |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [1.0f32, y2, z2], f2);
                            out += 1;
                        } else {
                            let t = (-1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                            let mut f = 0u32;
                            if y < -1.0f32 {
                                f |= CLIP_BOTTOM;
                            } else if y > 1.0f32 {
                                f |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [-1.0f32, y, z], f);
                            out += 1;
                        }
                    }
                } else if (a.clip & CLIP_RIGHT) != 0 {
                    if (b.clip & CLIP_LEFT) != 0 {
                        let t = (1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        let mut f = 0u32;
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [1.0f32, y, z], f);
                        out += 1;
                        let t2 = (-1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                        let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                        let z2 = (b.pos[2] - a.pos[2]) * t2 + a.pos[2];
                        let mut f2 = 0u32;
                        if y2 < -1.0f32 {
                            f2 |= CLIP_BOTTOM;
                        } else if y2 > 1.0f32 {
                            f2 |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [-1.0f32, y2, z2], f2);
                        out += 1;
                    } else if (b.clip & CLIP_RIGHT) == 0 {
                        let t = (1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        let mut f = 0u32;
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [1.0f32, y, z], f);
                        out += 1;
                    }
                } else {
                    poly.nxt[out] = poly.cur[pos];
                    out += 1;
                    if (b.clip & CLIP_LEFT) != 0 {
                        let t = (-1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        let mut f = 0u32;
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [-1.0f32, y, z], f);
                        out += 1;
                    } else if (b.clip & CLIP_RIGHT) != 0 {
                        let t = (1.0f32 - a.pos[0]) / (b.pos[0] - a.pos[0]);
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        let mut f = 0u32;
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [1.0f32, y, z], f);
                        out += 1;
                    }
                }
                pos += 1;
            }
            poly.close_swap(out);
            clip = poly.recollect(state);
        }
        // Y pass.
        if (clip & (CLIP_BOTTOM | CLIP_TOP)) != 0 {
            let mut out = 0usize;
            let mut n = poly.len;
            let mut pos = 0usize;
            while n > 0 {
                n -= 1;
                let ia = poly.cur[pos] as usize;
                let ib = poly.cur[pos + 1] as usize;
                let a = state.verts[ia];
                let b = state.verts[ib];
                if (a.clip & CLIP_BOTTOM) != 0 {
                    if (b.clip & CLIP_BOTTOM) == 0 {
                        if (b.clip & CLIP_TOP) != 0 {
                            let t = (-1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                            poly.nxt[out] = alloc_vert(state, [x, -1.0f32, z], 0);
                            out += 1;
                            let t2 = (1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                            let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                            let z2 = (b.pos[2] - a.pos[2]) * t2 + a.pos[2];
                            poly.nxt[out] = alloc_vert(state, [x2, 1.0f32, z2], 0);
                            out += 1;
                        } else {
                            let t = (-1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                            poly.nxt[out] = alloc_vert(state, [x, -1.0f32, z], 0);
                            out += 1;
                        }
                    }
                } else if (a.clip & CLIP_TOP) != 0 {
                    if (b.clip & CLIP_BOTTOM) != 0 {
                        let t = (1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        poly.nxt[out] = alloc_vert(state, [x, 1.0f32, z], 0);
                        out += 1;
                        let t2 = (-1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                        let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                        let z2 = (b.pos[2] - a.pos[2]) * t2 + a.pos[2];
                        poly.nxt[out] = alloc_vert(state, [x2, -1.0f32, z2], 0);
                        out += 1;
                    } else if (b.clip & CLIP_TOP) == 0 {
                        let t = (1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        poly.nxt[out] = alloc_vert(state, [x, 1.0f32, z], 0);
                        out += 1;
                    }
                } else {
                    poly.nxt[out] = poly.cur[pos];
                    out += 1;
                    if (b.clip & CLIP_BOTTOM) != 0 {
                        let t = (-1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        poly.nxt[out] = alloc_vert(state, [x, -1.0f32, z], 0);
                        out += 1;
                    } else if (b.clip & CLIP_TOP) != 0 {
                        let t = (1.0f32 - a.pos[1]) / (b.pos[1] - a.pos[1]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let z = (b.pos[2] - a.pos[2]) * t + a.pos[2];
                        poly.nxt[out] = alloc_vert(state, [x, 1.0f32, z], 0);
                        out += 1;
                    }
                }
                pos += 1;
            }
            poly.close_swap(out);
        }
        state.poly_num = poly.len;
        for (i, v) in poly.cur.iter().take(poly.len.min(512)).enumerate() {
            state.poly_buf1[i] = *v;
        }
        if poly.len > 2 {
            for i in 2..poly.len {
                pending.push(Triface {
                    vi: [poly.cur[0], poly.cur[i - 1], poly.cur[i]],
                });
            }
            // Lift along the face normal, then spread from the center.
            let mut n = [0.0f32; 3];
            tri_normal(&mut n, p0, p1, p2);
            let sq = (n[0] * n[0] + n[1] * n[1]) + n[2] * n[2];
            let offs = lift_offset(n, sq, state.lift);
            let mut c = state.verts[poly.cur[0] as usize].pos;
            for i in 1..poly.len {
                let q = state.verts[poly.cur[i] as usize].pos;
                c[0] += q[0];
                c[1] += q[1];
                c[2] += q[2];
            }
            let f = 1.0f32 / poly.len as f32;
            c[0] *= f;
            c[1] *= f;
            c[2] *= f;
            let spread = state.spread;
            for i in 0..poly.len {
                let vi = poly.cur[i] as usize;
                if vi >= first_vert {
                    let mut pos = state.verts[vi].pos;
                    pos[0] += offs[0];
                    pos[1] += offs[1];
                    pos[2] += offs[2];
                    if spread > 0.0f32 {
                        let d = [pos[0] - c[0], pos[1] - c[1], pos[2] - c[2]];
                        // Outer parens document the x87 grouping; do not "simplify".
                        #[allow(unused_parens)]
                        let sq = ((d[2] * d[2] + d[0] * d[0]) + d[1] * d[1]);
                        let mut nn = n;
                        let step = spread_step(d, sq, spread, &mut nn);
                        pos[0] += step[0];
                        pos[1] += step[1];
                        pos[2] += step[2];
                    }
                    state.verts[vi].pos = pos;
                }
            }
        }
        if state.vert_count >= state.verts_max {
            break;
        }
    }
    if state.fg_blend {
        state.faces_t.extend(pending.iter().copied());
        state.added_transp += state.faces_t.len() - start_t;
        state.next_face_t = state.faces_t.len();
    } else {
        state.faces.extend(pending.iter().copied());
        state.added_faces += state.faces.len() - start_o;
        state.next_face = state.faces.len();
    }
}

// ---------------------------------------------------------------------------
// 0x2530 ClipFacesPersp (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Clip faces to the perspective frustum (working guess, NOT original).
///
/// RVA 0x2530. Reference: mafia-re ls3df/I3D/render/clipper2.cpp:482-974.
/// Same staging as ortho but with frustum `t` forms (`(a.x + a.z)` ...),
/// `z = -x / x` and `z = -y / y` reconstructions, depth tessellation past
/// `non_ortho_tess` (`step = ((b - a) * (1/dz)) * tess`), nearest-vertex
/// triangulation, persp lift `n.x*n.x + (n.y*n.y + n.z*n.z)` and plain
/// spread `d.x*d.x + d.y*d.y + d.z*d.z`. Always returns `true` (kept).
pub fn clip_faces_persp(state: &mut ClipState, faces: &[Triface]) -> bool {
    let first_vert = state.vert_count;
    let start_o = state.faces.len();
    let start_t = state.faces_t.len();
    let mut pending: Vec<Triface> = Vec::new();

    for face in faces {
        let i0 = face.vi[0] as usize;
        let i1 = face.vi[1] as usize;
        let i2 = face.vi[2] as usize;
        if i0 >= state.verts.len() || i1 >= state.verts.len() || i2 >= state.verts.len() {
            continue;
        }
        let p0 = state.verts[i0].pos;
        let p1 = state.verts[i1].pos;
        let p2 = state.verts[i2].pos;
        let c0 = state.verts[i0].clip;
        let c1 = state.verts[i1].clip;
        let c2 = state.verts[i2].clip;
        if (c0 & c1 & c2) != 0 {
            continue;
        }
        if state.two_sided == 0 {
            let mut n = [0.0f32; 3];
            tri_normal(&mut n, p0, p1, p2);
            if cull_face(state, n) {
                continue;
            }
        }
        let mut clip = c0 | c1 | c2;
        let mut poly = Polygon::triangle(face.vi);
        // Near/far pass (frustum flag forms for new vertices).
        if (clip & (CLIP_NEAR | CLIP_FAR)) != 0 {
            let mut out = 0usize;
            let mut n = poly.len;
            let mut pos = 0usize;
            while n > 0 {
                n -= 1;
                let ia = poly.cur[pos] as usize;
                let ib = poly.cur[pos + 1] as usize;
                let a = state.verts[ia];
                let b = state.verts[ib];
                if (a.clip & CLIP_NEAR) != 0 {
                    if (b.clip & CLIP_NEAR) == 0 {
                        if (b.clip & CLIP_FAR) != 0 {
                            let t = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let mut f = 0u32;
                            if x < 0.0f32 {
                                f |= CLIP_LEFT;
                            } else if x > 0.0f32 {
                                f |= CLIP_RIGHT;
                            }
                            if y < 0.0f32 {
                                f |= CLIP_BOTTOM;
                            } else if y > 0.0f32 {
                                f |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [x, y, 0.0f32], f);
                            out += 1;
                            let t2 = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                            let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                            let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                            let mut f2 = 0u32;
                            if x2 < -1.0f32 {
                                f2 |= CLIP_LEFT;
                            } else if x2 > 1.0f32 {
                                f2 |= CLIP_RIGHT;
                            }
                            if y2 < -1.0f32 {
                                f2 |= CLIP_BOTTOM;
                            } else if y2 > 1.0f32 {
                                f2 |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [x2, y2, 1.0f32], f2);
                            out += 1;
                        } else {
                            let t = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let mut f = 0u32;
                            if x < 0.0f32 {
                                f |= CLIP_LEFT;
                            } else if x > 0.0f32 {
                                f |= CLIP_RIGHT;
                            }
                            if y < 0.0f32 {
                                f |= CLIP_BOTTOM;
                            } else if y > 0.0f32 {
                                f |= CLIP_TOP;
                            }
                            poly.nxt[out] = alloc_vert(state, [x, y, 0.0f32], f);
                            out += 1;
                        }
                    }
                } else if (a.clip & CLIP_FAR) != 0 {
                    if (b.clip & CLIP_NEAR) != 0 {
                        let t = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < -1.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 1.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 1.0f32], f);
                        out += 1;
                        let t2 = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                        let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                        let mut f2 = 0u32;
                        if x2 < 0.0f32 {
                            f2 |= CLIP_LEFT;
                        } else if x2 > 0.0f32 {
                            f2 |= CLIP_RIGHT;
                        }
                        if y2 < 0.0f32 {
                            f2 |= CLIP_BOTTOM;
                        } else if y2 > 0.0f32 {
                            f2 |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x2, y2, 0.0f32], f2);
                        out += 1;
                    } else if (b.clip & CLIP_FAR) == 0 {
                        let t = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < -1.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 1.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 1.0f32], f);
                        out += 1;
                    }
                } else {
                    poly.nxt[out] = poly.cur[pos];
                    out += 1;
                    if (b.clip & CLIP_NEAR) != 0 {
                        let t = (0.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < 0.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 0.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < 0.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 0.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 0.0f32], f);
                        out += 1;
                    } else if (b.clip & CLIP_FAR) != 0 {
                        let t = (1.0f32 - a.pos[2]) / (b.pos[2] - a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let mut f = 0u32;
                        if x < -1.0f32 {
                            f |= CLIP_LEFT;
                        } else if x > 1.0f32 {
                            f |= CLIP_RIGHT;
                        }
                        if y < -1.0f32 {
                            f |= CLIP_BOTTOM;
                        } else if y > 1.0f32 {
                            f |= CLIP_TOP;
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, 1.0f32], f);
                        out += 1;
                    }
                }
                pos += 1;
            }
            poly.close_swap(out);
            clip = poly.recollect(state);
        }
        // X pass (frustum planes).
        if (clip & (CLIP_LEFT | CLIP_RIGHT)) != 0 {
            let mut out = 0usize;
            let mut n = poly.len;
            let mut pos = 0usize;
            while n > 0 {
                n -= 1;
                let ia = poly.cur[pos] as usize;
                let ib = poly.cur[pos + 1] as usize;
                let a = state.verts[ia];
                let b = state.verts[ib];
                if (a.clip & CLIP_LEFT) != 0 {
                    if (b.clip & CLIP_LEFT) == 0 {
                        if (b.clip & CLIP_RIGHT) != 0 {
                            let t =
                                (a.pos[0] + a.pos[2]) / (a.pos[0] + a.pos[2] - b.pos[0] - b.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let z = -x;
                            let mut f = 0u32;
                            if y.abs() > z {
                                if y < 0.0f32 {
                                    f |= CLIP_BOTTOM;
                                } else {
                                    f |= CLIP_TOP;
                                }
                            }
                            poly.nxt[out] = alloc_vert(state, [x, y, z], f);
                            out += 1;
                            let t2 =
                                (a.pos[2] - a.pos[0]) / (b.pos[0] - a.pos[0] - b.pos[2] + a.pos[2]);
                            let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                            let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                            let z2 = x2;
                            let mut f2 = 0u32;
                            if y2.abs() > z2 {
                                if y2 < 0.0f32 {
                                    f2 |= CLIP_BOTTOM;
                                } else {
                                    f2 |= CLIP_TOP;
                                }
                            }
                            poly.nxt[out] = alloc_vert(state, [x2, y2, z2], f2);
                            out += 1;
                        } else {
                            let t =
                                (a.pos[0] + a.pos[2]) / (a.pos[0] + a.pos[2] - b.pos[0] - b.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let z = -x;
                            let mut f = 0u32;
                            if y.abs() > z {
                                if y < 0.0f32 {
                                    f |= CLIP_BOTTOM;
                                } else {
                                    f |= CLIP_TOP;
                                }
                            }
                            poly.nxt[out] = alloc_vert(state, [x, y, z], f);
                            out += 1;
                        }
                    }
                } else if (a.clip & CLIP_RIGHT) != 0 {
                    if (b.clip & CLIP_LEFT) != 0 {
                        let t = (a.pos[2] - a.pos[0]) / (b.pos[0] - a.pos[0] - b.pos[2] + a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = x;
                        let mut f = 0u32;
                        if y.abs() > z {
                            if y < 0.0f32 {
                                f |= CLIP_BOTTOM;
                            } else {
                                f |= CLIP_TOP;
                            }
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, z], f);
                        out += 1;
                        let t2 =
                            (a.pos[0] + a.pos[2]) / (a.pos[0] + a.pos[2] - b.pos[0] - b.pos[2]);
                        let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                        let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                        let z2 = -x2;
                        let mut f2 = 0u32;
                        if y2.abs() > z2 {
                            if y2 < 0.0f32 {
                                f2 |= CLIP_BOTTOM;
                            } else {
                                f2 |= CLIP_TOP;
                            }
                        }
                        poly.nxt[out] = alloc_vert(state, [x2, y2, z2], f2);
                        out += 1;
                    } else if (b.clip & CLIP_RIGHT) == 0 {
                        let t = (a.pos[2] - a.pos[0]) / (b.pos[0] - a.pos[0] - b.pos[2] + a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = x;
                        let mut f = 0u32;
                        if y.abs() > z {
                            if y < 0.0f32 {
                                f |= CLIP_BOTTOM;
                            } else {
                                f |= CLIP_TOP;
                            }
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, z], f);
                        out += 1;
                    }
                } else {
                    poly.nxt[out] = poly.cur[pos];
                    out += 1;
                    if (b.clip & CLIP_LEFT) != 0 {
                        let t = (a.pos[0] + a.pos[2]) / (a.pos[0] + a.pos[2] - b.pos[0] - b.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = -x;
                        let mut f = 0u32;
                        if y.abs() > z {
                            if y < 0.0f32 {
                                f |= CLIP_BOTTOM;
                            } else {
                                f |= CLIP_TOP;
                            }
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, z], f);
                        out += 1;
                    } else if (b.clip & CLIP_RIGHT) != 0 {
                        let t = (a.pos[2] - a.pos[0]) / (b.pos[0] - a.pos[0] - b.pos[2] + a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = x;
                        let mut f = 0u32;
                        if y.abs() > z {
                            if y < 0.0f32 {
                                f |= CLIP_BOTTOM;
                            } else {
                                f |= CLIP_TOP;
                            }
                        }
                        poly.nxt[out] = alloc_vert(state, [x, y, z], f);
                        out += 1;
                    }
                }
                pos += 1;
            }
            poly.close_swap(out);
            clip = poly.recollect(state);
        }
        // Y pass (frustum planes, new vertices carry clip 0).
        if (clip & (CLIP_BOTTOM | CLIP_TOP)) != 0 {
            let mut out = 0usize;
            let mut n = poly.len;
            let mut pos = 0usize;
            while n > 0 {
                n -= 1;
                let ia = poly.cur[pos] as usize;
                let ib = poly.cur[pos + 1] as usize;
                let a = state.verts[ia];
                let b = state.verts[ib];
                if (a.clip & CLIP_BOTTOM) != 0 {
                    if (b.clip & CLIP_BOTTOM) == 0 {
                        if (b.clip & CLIP_TOP) != 0 {
                            let t =
                                (a.pos[1] + a.pos[2]) / (a.pos[1] + a.pos[2] - b.pos[1] - b.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let z = -y;
                            poly.nxt[out] = alloc_vert(state, [x, y, z], 0);
                            out += 1;
                            let t2 =
                                (a.pos[2] - a.pos[1]) / (b.pos[1] - a.pos[1] - b.pos[2] + a.pos[2]);
                            let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                            let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                            let z2 = y2;
                            poly.nxt[out] = alloc_vert(state, [x2, y2, z2], 0);
                            out += 1;
                        } else {
                            let t =
                                (a.pos[1] + a.pos[2]) / (a.pos[1] + a.pos[2] - b.pos[1] - b.pos[2]);
                            let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                            let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                            let z = -y;
                            poly.nxt[out] = alloc_vert(state, [x, y, z], 0);
                            out += 1;
                        }
                    }
                } else if (a.clip & CLIP_TOP) != 0 {
                    if (b.clip & CLIP_BOTTOM) != 0 {
                        let t = (a.pos[2] - a.pos[1]) / (b.pos[1] - a.pos[1] - b.pos[2] + a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = y;
                        poly.nxt[out] = alloc_vert(state, [x, y, z], 0);
                        out += 1;
                        let t2 =
                            (a.pos[1] + a.pos[2]) / (a.pos[1] + a.pos[2] - b.pos[1] - b.pos[2]);
                        let x2 = (b.pos[0] - a.pos[0]) * t2 + a.pos[0];
                        let y2 = (b.pos[1] - a.pos[1]) * t2 + a.pos[1];
                        let z2 = -y2;
                        poly.nxt[out] = alloc_vert(state, [x2, y2, z2], 0);
                        out += 1;
                    } else if (b.clip & CLIP_TOP) == 0 {
                        let t = (a.pos[2] - a.pos[1]) / (b.pos[1] - a.pos[1] - b.pos[2] + a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = y;
                        poly.nxt[out] = alloc_vert(state, [x, y, z], 0);
                        out += 1;
                    }
                } else {
                    poly.nxt[out] = poly.cur[pos];
                    out += 1;
                    if (b.clip & CLIP_BOTTOM) != 0 {
                        let t = (a.pos[1] + a.pos[2]) / (a.pos[1] + a.pos[2] - b.pos[1] - b.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = -y;
                        poly.nxt[out] = alloc_vert(state, [x, y, z], 0);
                        out += 1;
                    } else if (b.clip & CLIP_TOP) != 0 {
                        let t = (a.pos[2] - a.pos[1]) / (b.pos[1] - a.pos[1] - b.pos[2] + a.pos[2]);
                        let x = (b.pos[0] - a.pos[0]) * t + a.pos[0];
                        let y = (b.pos[1] - a.pos[1]) * t + a.pos[1];
                        let z = y;
                        poly.nxt[out] = alloc_vert(state, [x, y, z], 0);
                        out += 1;
                    }
                }
                pos += 1;
            }
            poly.close_swap(out);
        }
        state.poly_num = poly.len;
        for (i, v) in poly.cur.iter().take(poly.len.min(512)).enumerate() {
            state.poly_buf1[i] = *v;
        }
        if poly.len > 2 {
            // Depth tessellation: edges deeper than tess are split.
            let tess = state.non_ortho_tess;
            let mut tessellated = [0u16; 513];
            let mut tlen = 0usize;
            let mut min_z = 10.0f32;
            let mut min_i = 0usize;
            {
                let mut n = poly.len;
                let mut pos = 0usize;
                while n > 0 {
                    n -= 1;
                    let ia = poly.cur[pos] as usize;
                    let ib = poly.cur[pos + 1] as usize;
                    let a = state.verts[ia].pos;
                    let b = state.verts[ib].pos;
                    if a[2] < min_z {
                        min_z = a[2];
                        min_i = tlen;
                    }
                    tessellated[tlen] = poly.cur[pos];
                    tlen += 1;
                    let dz = (a[2] - b[2]).abs();
                    if dz > tess {
                        let k = (dz / tess) as usize;
                        // step = ((b - a) * (1/dz)) * tess.
                        let inv = 1.0f32 / dz;
                        let sx = (b[0] - a[0]) * inv * tess;
                        let sy = (b[1] - a[1]) * inv * tess;
                        let sz = (b[2] - a[2]) * inv * tess;
                        let mut px: [f32; 3] = if a[2] < b[2] {
                            a
                        } else {
                            [
                                b[0] - sx * k as f32,
                                b[1] - sy * k as f32,
                                b[2] - sz * k as f32,
                            ]
                        };
                        for _ in 0..k {
                            px[0] += sx;
                            px[1] += sy;
                            px[2] += sz;
                            tessellated[tlen] = alloc_vert(state, px, 0);
                            tlen += 1;
                        }
                    }
                    pos += 1;
                }
            }
            // Publish tessellated polygon into cur for lift/spread.
            for i in 0..tlen {
                poly.cur[i] = tessellated[i];
            }
            poly.cur[tlen] = tessellated[0];
            poly.len = tlen;
            state.poly_num = tlen;
            for (i, v) in poly.cur.iter().take(tlen.min(512)).enumerate() {
                state.poly_buf1[i] = *v;
            }
            if tlen == 3 {
                pending.push(Triface {
                    vi: [poly.cur[0], poly.cur[1], poly.cur[2]],
                });
            } else {
                // Rotate so the nearest vertex is in the middle buffer.
                let mut ordered = [0u16; 513];
                let first = min_i + 1;
                if first < tlen {
                    let tail = tlen - first;
                    for i in 0..tail {
                        ordered[i] = poly.cur[first + i];
                    }
                    for i in 0..min_i {
                        ordered[tail + i] = poly.cur[i];
                    }
                } else {
                    for i in 0..min_i {
                        ordered[i] = poly.cur[i];
                    }
                }
                let mut hi: usize = tlen - 2;
                let mut lo: usize = 0;
                let mut i = 1usize;
                let mut j = tlen - 3;
                pending.push(Triface {
                    vi: [ordered[hi], poly.cur[min_i], ordered[0]],
                });
                while j > i {
                    let zj = state.verts[ordered[j] as usize].pos[2];
                    let zi = state.verts[ordered[i] as usize].pos[2];
                    if zj < zi {
                        pending.push(Triface {
                            vi: [ordered[hi], ordered[lo], ordered[j]],
                        });
                        hi = j;
                        j -= 1;
                    } else {
                        pending.push(Triface {
                            vi: [ordered[hi], ordered[lo], ordered[i]],
                        });
                        lo = i;
                        i += 1;
                    }
                }
                pending.push(Triface {
                    vi: [ordered[hi], ordered[lo], ordered[j]],
                });
            }
            // Lift/spread over the tessellated polygon (persp sums).
            let mut n = [0.0f32; 3];
            tri_normal(&mut n, p0, p1, p2);
            let sq = n[0] * n[0] + (n[1] * n[1] + n[2] * n[2]);
            let offs = lift_offset(n, sq, state.lift);
            let mut c = state.verts[poly.cur[0] as usize].pos;
            for i in 1..tlen {
                let q = state.verts[poly.cur[i] as usize].pos;
                c[0] += q[0];
                c[1] += q[1];
                c[2] += q[2];
            }
            let f = 1.0f32 / tlen as f32;
            c[0] *= f;
            c[1] *= f;
            c[2] *= f;
            let spread = state.spread;
            for i in 0..tlen {
                let vi = poly.cur[i] as usize;
                if vi >= first_vert {
                    let mut pos = state.verts[vi].pos;
                    pos[0] += offs[0];
                    pos[1] += offs[1];
                    pos[2] += offs[2];
                    if spread > 0.0f32 {
                        let d = [pos[0] - c[0], pos[1] - c[1], pos[2] - c[2]];
                        let sq = (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2];
                        let mut nn = n;
                        let step = spread_step(d, sq, spread, &mut nn);
                        pos[0] += step[0];
                        pos[1] += step[1];
                        pos[2] += step[2];
                    }
                    state.verts[vi].pos = pos;
                }
            }
        }
        if state.vert_count >= state.verts_max {
            break;
        }
    }
    if state.fg_blend {
        state.faces_t.extend(pending.iter().copied());
        state.added_transp += state.faces_t.len() - start_t;
        state.next_face_t = state.faces_t.len();
    } else {
        state.faces.extend(pending.iter().copied());
        state.added_faces += state.faces.len() - start_o;
        state.next_face = state.faces.len();
    }
    true
}

// ---------------------------------------------------------------------------
// 0x3b40 ClipMesh (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Transform a mesh into clip space, then clip its face groups.
///
/// RVA 0x3b40. Reference: mafia-re ls3df/I3D/render/clipping.cpp:40-111.
/// Reserves arrays, latches `with_alpha`, transforms through `m * matrix`
/// (transform, then flag per ortho/persp arm), sets `vert_count` and calls
/// [`clip_facegroups`]. Returns `false` when reservation fails.
pub fn clip_mesh(
    state: &mut ClipState,
    verts: &[[f32; 3]],
    groups: &[FaceGroup],
    with_alpha: u32,
    m: &ClipMatrix,
) -> bool {
    let face_total: usize = groups.iter().map(|g| g.faces.len()).sum();
    if !reserve_clip_arrays(state, verts.len(), face_total) {
        return false;
    }
    state.with_alpha = with_alpha;
    let mm = mul_matrix(m, &state.matrix);
    if state.verts.len() < verts.len() {
        state.verts.resize(verts.len(), ClipVertex::default());
    }
    if state.is_ortho {
        for (i, v) in verts.iter().enumerate() {
            let pos = transform_point(*v, &mm);
            state.verts[i].pos = pos;
            state.verts[i].clip = ortho_flag(pos);
            state.verts[i].index = -1;
        }
    } else {
        for (i, v) in verts.iter().enumerate() {
            let pos = transform_point(*v, &mm);
            state.verts[i].pos = pos;
            state.verts[i].clip = persp_flag(pos);
            state.verts[i].index = -1;
        }
    }
    state.vert_count = verts.len();
    state.vert_end = state.vert_count;
    clip_facegroups(state, groups)
}

// ---------------------------------------------------------------------------
// 0x3dc0 ClipFacegroups (upstream name) + CheckClipFaces helper.
// ---------------------------------------------------------------------------

/// Opaque-face block still fits (`CheckClipFaces`, rides with 0x3dc0).
///
/// Not a credited function (linker-dropped copy): opaque used faces must not
/// exceed the per-list allocation. The native overflow dbgPrintf has no sink
/// (disclosed; a later differential wave adds one).
fn check_clip_faces(state: &ClipState) -> bool {
    state.num_faces <= state.faces_alloc
}

/// Clip face groups into the two face lists and resolve result vertices.
///
/// RVA 0x3dc0. Reference: mafia-re ls3df/I3D/render/clipping.cpp:116-210
/// (upstream name). Owns `vert_end`/next-face setup, per-group blend and
/// two-sided assignment (`backfaces` forces two-sided), ortho/persp dispatch,
/// vertex-index assignment with `center_t` accumulation for transparent
/// faces, result-vertex overflow rollback and the result-copy loop. Returns
/// `false` when nothing was added, on persp failure, on face overflow or on
/// result-vertex overflow (rolling `faces`/`faces_t` back in the last case).
pub fn clip_facegroups(state: &mut ClipState, groups: &[FaceGroup]) -> bool {
    state.vert_end = state.vert_count;
    state.next_face = state.faces.len();
    state.next_face_t = state.faces_t.len();
    state.num_faces = state.faces.len();
    state.num_faces_t = state.faces_t.len();
    state.added_faces = 0;
    state.added_transp = 0;
    let start_o = state.faces.len();
    let start_t = state.faces_t.len();

    for group in groups {
        match &group.material {
            Some(mat) => {
                if (mat.flags & (MATF_CKEY_ALPHA | MATF_TXT_ALPHA | MATF_ADD_MODE)) != 0
                    || mat.alpha < 1.0f32
                {
                    state.fg_blend = true;
                    if state.with_alpha == 0 {
                        continue;
                    }
                } else {
                    state.fg_blend = false;
                }
                state.two_sided = mat.flags & MATF_TWO_SIDED;
            }
            None => {
                state.two_sided = 0;
                state.fg_blend = false;
            }
        }
        if state.backfaces {
            state.two_sided = 1;
        }
        if state.is_ortho {
            clip_faces_ortho(state, &group.faces);
        } else if !clip_faces_persp(state, &group.faces) {
            return false;
        }
    }

    let added_o = state.faces.len() - start_o;
    let added_t = state.faces_t.len() - start_t;
    state.added_faces = added_o;
    state.added_transp = added_t;
    if added_o == 0 && added_t == 0 {
        return false;
    }
    // Vertex-index assignment. Native narrows `index` to short; the host
    // narrows with `as u16` identically (same low-16-bits wrap).
    let mut index: i32 = state.num_results as i32;
    if added_o > 0 {
        state.num_faces = state.faces.len();
        if !check_clip_faces(state) {
            return false;
        }
        for i in start_o..state.faces.len() {
            for k in 0..3 {
                let vi = state.faces[i].vi[k] as usize;
                let slot = state.verts[vi].index;
                if slot < 0 {
                    state.verts[vi].index = index;
                    state.faces[i].vi[k] = index as u16;
                    index += 1;
                } else {
                    state.faces[i].vi[k] = slot as u16;
                }
            }
        }
    }
    if added_t > 0 {
        state.num_faces_t = state.faces_t.len();
        for i in start_t..state.faces_t.len() {
            for k in 0..3 {
                let vi = state.faces_t[i].vi[k] as usize;
                let slot = state.verts[vi].index;
                if slot < 0 {
                    state.verts[vi].index = index;
                    state.faces_t[i].vi[k] = index as u16;
                    index += 1;
                    let p = state.verts[vi].pos;
                    state.center_t[0] += p[0];
                    state.center_t[1] += p[1];
                    state.center_t[2] += p[2];
                } else {
                    state.faces_t[i].vi[k] = slot as u16;
                }
            }
        }
    }
    let num = index as usize;
    if num > CLIP_MAX_VERTICES {
        state.faces.truncate(start_o);
        state.faces_t.truncate(start_t);
        state.num_faces = state.faces.len();
        state.num_faces_t = state.faces_t.len();
        state.added_faces = 0;
        state.added_transp = 0;
        return false;
    }
    if state.result_alloc < num {
        let mut need = num;
        if need < CLIP_MIN_VERTICES {
            need = CLIP_MIN_VERTICES;
        }
        state.results.resize(need, [0.0f32; 3]);
        state.result_alloc = need;
    }
    if state.results.len() < num {
        state.results.resize(num, [0.0f32; 3]);
    }
    let mut n = 0usize;
    let count = state.vert_count;
    for i in 0..count {
        if state.verts[i].index >= 0 {
            let at = state.verts[i].index as usize;
            state.results[at] = state.verts[i].pos;
            n += 1;
        }
    }
    state.num_results += n;
    state.next_face = state.faces.len();
    state.next_face_t = state.faces_t.len();
    true
}

// ---------------------------------------------------------------------------
// 0x4110 ClipMeshW (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Clip a mesh already in world space (working guess, NOT original).
///
/// RVA 0x4110. Reference: mafia-re ls3df/I3D/render/clipping.cpp:215-288.
/// Like [`clip_mesh`] but transforms through the clip matrix directly, with
/// the native `if (num_verts)` guard around each arm's loop.
pub fn clip_mesh_w(
    state: &mut ClipState,
    verts: &[[f32; 3]],
    groups: &[FaceGroup],
    with_alpha: u32,
) -> bool {
    let face_total: usize = groups.iter().map(|g| g.faces.len()).sum();
    if !reserve_clip_arrays(state, verts.len(), face_total) {
        return false;
    }
    state.with_alpha = with_alpha;
    let m = state.matrix;
    if state.verts.len() < verts.len() {
        state.verts.resize(verts.len(), ClipVertex::default());
    }
    if state.is_ortho {
        if !verts.is_empty() {
            for (i, v) in verts.iter().enumerate() {
                let pos = transform_point(*v, &m);
                state.verts[i].pos = pos;
                state.verts[i].clip = ortho_flag(pos);
                state.verts[i].index = -1;
            }
        }
    } else if !verts.is_empty() {
        for (i, v) in verts.iter().enumerate() {
            let pos = transform_point(*v, &m);
            state.verts[i].pos = pos;
            state.verts[i].clip = persp_flag(pos);
            state.verts[i].index = -1;
        }
    }
    state.vert_count = verts.len();
    state.vert_end = state.vert_count;
    clip_facegroups(state, groups)
}

// ---------------------------------------------------------------------------
// 0x4370 ClipSingleMesh (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Clip one single-mesh/morph vertex array (working guess, NOT original).
///
/// RVA 0x4370. Reference: mafia-re ls3df/I3D/render/clipping.cpp:293-364.
/// Same transform/flag/cull pipeline as [`clip_mesh`]; single-morph vertices
/// carry positions only, so the typed input is identical.
pub fn clip_single_mesh(
    state: &mut ClipState,
    verts: &[[f32; 3]],
    groups: &[FaceGroup],
    with_alpha: u32,
    m: &ClipMatrix,
) -> bool {
    clip_mesh(state, verts, groups, with_alpha, m)
}

// ---------------------------------------------------------------------------
// 0x45f0 ClipSingleMeshW (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Clip world-space single-mesh vertices (working guess, NOT original).
///
/// RVA 0x45f0. Reference: mafia-re ls3df/I3D/render/clipping.cpp:369-443.
/// Same as [`clip_mesh_w`] for single-mesh position arrays.
pub fn clip_single_mesh_w(
    state: &mut ClipState,
    verts: &[[f32; 3]],
    groups: &[FaceGroup],
    with_alpha: u32,
) -> bool {
    clip_mesh_w(state, verts, groups, with_alpha)
}

// ---------------------------------------------------------------------------
// 0x619b0 RadixSort32 (PS2-evidenced name).
// ---------------------------------------------------------------------------

/// One sorted entry: key plus payload (PS2 `sort_entry_32`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortEntry32 {
    /// Sort key, ascending unsigned.
    pub sort_value: u32,
    /// Associated data word.
    pub data: u32,
}

/// Second-buffer scratch kept across sorts (`GlobalStorage`).
#[derive(Clone, Debug, Default)]
pub struct SortStorage {
    /// Scratch buffer (`GlobalStorage`/`GlobalStorageSize`).
    pub buf: Vec<SortEntry32>,
}

/// Ascending unsigned 32-bit LSD radix sort (PS2-evidenced name).
///
/// RVA 0x619b0. Reference: mafia-re ls3df/I3D/render/RadixSort.cpp:197-254.
/// Four byte passes with per-byte uniform skip; the last byte uses the
/// signed-sort skip test (`0x80`/`0x7f` full) but still sorts unsigned.
/// Stable within equal keys. Empty input returns immediately. The scratch
/// buffer grows only.
pub fn radix_sort_32(data: &mut [SortEntry32], storage: &mut SortStorage) {
    if data.is_empty() {
        return;
    }
    let num = data.len();
    if storage.buf.len() < num {
        storage.buf.resize(
            num,
            SortEntry32 {
                sort_value: 0,
                data: 0,
            },
        );
    }
    let mut count = [[0u32; 256]; 4];
    for e in data.iter() {
        let v = e.sort_value;
        count[0][(v & 0xff) as usize] += 1;
        count[1][((v >> 8) & 0xff) as usize] += 1;
        count[2][((v >> 16) & 0xff) as usize] += 1;
        count[3][((v >> 24) & 0xff) as usize] += 1;
    }
    // Ping-pong between data and scratch; src/dst swap per executed pass.
    // `in_scratch` tracks where the live array currently sits.
    let mut in_scratch = false;
    for byte in 0..4 {
        let uniform = if byte < 3 {
            count[byte][0] as usize == num || count[byte][0xff] as usize == num
        } else {
            count[byte][0x80] as usize == num || count[byte][0x7f] as usize == num
        };
        if uniform {
            // Clear the uniform count exactly as native does.
            if byte < 3 {
                if count[byte][0] as usize == num {
                    count[byte][0] = 0;
                } else {
                    count[byte][0xff] = 0;
                }
            } else if count[byte][0x80] as usize == num {
                count[byte][0x80] = 0;
            } else {
                count[byte][0x7f] = 0;
            }
            continue;
        }
        let mut index = [0u32; 256];
        let mut next = 0u32;
        for i in 0..256 {
            index[i] = next;
            next += count[byte][i];
            count[byte][i] = 0;
        }
        let shift = (byte * 8) as u32;
        if !in_scratch {
            // Stable distribution from data into scratch.
            let scratch = &mut storage.buf[..num];
            for e in data.iter() {
                let b = ((e.sort_value >> shift) & 0xff) as usize;
                let at = index[b] as usize;
                scratch[at] = *e;
                index[b] += 1;
            }
            in_scratch = true;
        } else {
            // Stable distribution from scratch back into data. Counts are
            // order-independent, so the latched `index` bases are reused.
            let mut tmp: Vec<SortEntry32> = Vec::with_capacity(num);
            tmp.extend_from_slice(&storage.buf[..num]);
            for e in tmp.iter() {
                let b = ((e.sort_value >> shift) & 0xff) as usize;
                let at = index[b] as usize;
                data[at] = *e;
                index[b] += 1;
            }
            in_scratch = false;
        }
    }
    if in_scratch {
        data.copy_from_slice(&storage.buf[..num]);
    }
}

// ---------------------------------------------------------------------------
// Lighting state (T_l.cpp globals in a narrow adapter).
// ---------------------------------------------------------------------------

/// One sector light read by the lighting contracts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorLight {
    /// `I3DLIGHT_*` word.
    pub kind: u32,
    /// `I3DLIGHTMODE_*` words.
    pub mode: u32,
    /// `FRMFLAGS_ON` set.
    pub on: bool,
    /// Light power (skipped when marginally zero).
    pub power: f32,
    /// Light color.
    pub color: [f32; 3],
    /// World position (point/spot).
    pub w_pos: [f32; 3],
    /// World direction (spot/directional).
    pub w_dir: [f32; 3],
    /// Bound-sphere radius (spot range gate alongside the object radius).
    pub radius: f32,
    /// Near/far ranges.
    pub range_n: f32,
    /// Near/far ranges.
    pub range_f: f32,
    /// Far range squared (point/spot fast reject).
    pub range_f_2: f32,
    /// Spot cone cosines and scale.
    pub outer_cos: f32,
    /// Spot cone cosines and scale.
    pub inner_cos: f32,
    /// Spot cone cosines and scale.
    pub cone_scale: f32,
    /// Update counter feeding the light key.
    pub update_count: u32,
}

/// One picked light in object space (`S_temp_light` without pointers).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempLight {
    /// Position in object space.
    pub pos: [f32; 3],
    /// Direction towards the light, normalized.
    pub dir: [f32; 3],
    /// Color scaled by the object color.
    pub color: [f32; 3],
    /// Scaled ranges.
    pub range_n: f32,
    /// Scaled ranges.
    pub range_f: f32,
    /// Scaled ranges.
    pub range_f_2: f32,
    /// Inverse range width, when positive.
    pub range_delta: f32,
    /// Selection energy.
    pub energy: f32,
    /// Source update counter.
    pub update_count: u32,
    /// Source kind.
    pub kind: u32,
    /// Spot cone terms (copied so the vertex loop needs no light pointer).
    pub outer_cos: f32,
    /// Spot cone terms (copied so the vertex loop needs no light pointer).
    pub inner_cos: f32,
    /// Spot cone terms (copied so the vertex loop needs no light pointer).
    pub cone_scale: f32,
}

/// Lighting globals: ambient sum, kept lights, alpha, sector presence.
#[derive(Clone, Debug)]
pub struct LightState {
    /// Ambient sum (`TempAmbient`).
    pub ambient: [f32; 3],
    /// Kept lights, strongest first-come (`TempLightList`, max 8).
    pub picked: Vec<TempLight>,
    /// Vertex alpha already shifted (`TempAlpha`).
    pub alpha_word: u32,
    /// Color scale (`vertex_light_mult`, 128 or 255).
    pub mult: f32,
    /// A current sector is set (`pCurrentSector != NULL`).
    pub sector_present: bool,
}

impl LightState {
    /// Cleared rig with the default 128 scale.
    pub fn new() -> Self {
        Self {
            ambient: [0.0f32; 3],
            picked: Vec::new(),
            alpha_word: 0,
            mult: DEFAULT_VERTEX_LIGHT_MULT,
            sector_present: false,
        }
    }
}

impl Default for LightState {
    fn default() -> Self {
        Self::new()
    }
}

/// Object terms read by [`prepare_lights`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitObject {
    /// Object color scalar (`lightness`).
    pub lightness: f32,
    /// Fade and transparency scalars.
    pub fade: f32,
    /// Fade and transparency scalars.
    pub transparency: f32,
    /// World bound-sphere center.
    pub bound_pos: [f32; 3],
    /// World bound-sphere radius.
    pub bound_radius: f32,
    /// `VISF_*` words.
    pub vis_flags: u32,
}

/// Mesh-level terms read by [`prepare_lights`].
#[derive(Clone, Debug, PartialEq)]
pub struct LitMeshLevel {
    /// Vertex count.
    pub num_vertices: usize,
    /// `MESHF_*` words.
    pub mesh_flags: u32,
    /// Level material (`None` means untextured default).
    pub material: Option<LitMaterial>,
}

/// Material terms read by lighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitMaterial {
    /// Diffuse color.
    pub diffuse: [f32; 3],
    /// Ambient color.
    pub ambient: [f32; 3],
    /// Emissive color.
    pub emissive: [f32; 3],
    /// Material alpha.
    pub alpha: f32,
}

/// Alpha latch (`lod::alpha` + `alpha_dirty`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlphaRec {
    /// Last written alpha byte.
    pub alpha: u8,
    /// Needs rewrite.
    pub dirty: bool,
}

// ---------------------------------------------------------------------------
// 0x65a60 LightingSetSector (working guess; trivial setter).
// ---------------------------------------------------------------------------

/// Latch the current sector presence (working guess, NOT original).
///
/// RVA 0x65a60. Reference: mafia-re ls3df/I3D/render/T_l.cpp:133-138.
/// The original stores the `I3D_sector*`; the host keeps presence only,
/// with the light list passed per call by the sector owner (no pointers).
pub fn lighting_set_sector(state: &mut LightState, present: bool) {
    state.sector_present = present;
}

// ---------------------------------------------------------------------------
// 0x65a70 PrepareLights (PS2-evidenced name).
// ---------------------------------------------------------------------------

/// Pick sector lights and write vertex colors (PS2-evidenced name).
///
/// RVA 0x65a70. Reference: mafia-re ls3df/I3D/render/T_l.cpp:403-574.
/// Order: ambient reset, object-color latch, alpha word, bound/scale latch,
/// per-light select (ambient add; spot/point range-energy gate with
/// `d.x*d.x + d.y*d.y + d.z*d.z` vs `r*r` reject and strongest-8 keep;
/// directional keep; unused types skipped), key accumulate
/// (`sum * count`), cache check (`!force || (empty && key > 0)` gate, equal
/// key writes nothing), ambient/material fold, then per-vertex or
/// average-light color loop with the `alpha_dirty=false` latch. Returns
/// whether colors were written.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::cast_possible_truncation)]
pub fn prepare_lights(
    state: &mut LightState,
    dst: &mut [u32],
    src_pos: &[[f32; 3]],
    src_nrm: &[[f32; 3]],
    level: &LitMeshLevel,
    obj: &LitObject,
    inv: &ClipMatrix,
    sector_lights: &[SectorLight],
    mode: u32,
    force: bool,
    cached_key: &mut u32,
    rec: Option<&mut AlphaRec>,
) -> bool {
    state.ambient = [0.0f32; 3];
    state.picked.clear();
    let mut temp_color = [obj.lightness, obj.lightness, obj.lightness];
    let mat_alpha: f32;
    let mat_for_color: Option<LitMaterial>;
    match &level.material {
        Some(mat) => {
            mat_alpha = obj.fade * obj.transparency * mat.alpha * 255.0f32;
            if (level.mesh_flags & MESHF_TEXTURE) == 0
                || (level.mesh_flags & MESHF_KEEP_DIFFUSE) != 0
            {
                temp_color[0] *= mat.diffuse[0];
                temp_color[1] *= mat.diffuse[1];
                temp_color[2] *= mat.diffuse[2];
            }
            mat_for_color = Some(*mat);
        }
        None => {
            mat_alpha = obj.fade * obj.transparency * 255.0f32;
            mat_for_color = None;
        }
    }
    state.alpha_word = (float_to_int(mat_alpha) as u32) << 24;
    let scale =
        (row_magnitude(inv, 0) + row_magnitude(inv, 1) + row_magnitude(inv, 2)) * (1.0f32 / 3.0f32);
    let mut key: u32 = 0;
    for lt in sector_lights {
        if (lt.mode & mode) == 0 || !lt.on {
            continue;
        }
        if is_abs_mrg_zero(lt.power) {
            continue;
        }
        match lt.kind {
            LIGHT_AMBIENT => {
                state.ambient[0] += lt.color[0] * obj.lightness;
                state.ambient[1] += lt.color[1] * obj.lightness;
                state.ambient[2] += lt.color[2] * obj.lightness;
                // Wrapping x86 accumulation (debug builds must not trap).
                key = key.wrapping_add(lt.update_count);
            }
            LIGHT_SPOT => {
                let dx = obj.bound_pos[0] - lt.w_pos[0];
                let dy = obj.bound_pos[1] - lt.w_pos[1];
                let dz = obj.bound_pos[2] - lt.w_pos[2];
                let dist_2 = (dx * dx + dy * dy) + dz * dz;
                let r = obj.bound_radius + lt.radius;
                if !((r * r) <= dist_2) {
                    let energy = (lt.color[0] + lt.color[1] + lt.color[2]) / dist_2.sqrt();
                    if let Some(slot) = temp_light_slot(state, energy) {
                        let mut dir = rotate_vector(lt.w_dir, inv);
                        dir[0] = -dir[0];
                        dir[1] = -dir[1];
                        dir[2] = -dir[2];
                        dir = normalize_f32(dir);
                        let pos = transform_point(lt.w_pos, inv);
                        let range_n = scale * lt.range_n;
                        let range_f = scale * lt.range_f;
                        let mut delta = range_f - range_n;
                        if delta > 0.0f32 {
                            delta = 1.0f32 / delta;
                        }
                        let entry = TempLight {
                            pos,
                            dir,
                            color: [
                                temp_color[0] * lt.color[0],
                                temp_color[1] * lt.color[1],
                                temp_color[2] * lt.color[2],
                            ],
                            range_n,
                            range_f,
                            range_f_2: range_f * range_f,
                            range_delta: delta,
                            energy,
                            update_count: lt.update_count,
                            kind: lt.kind,
                            outer_cos: lt.outer_cos,
                            inner_cos: lt.inner_cos,
                            cone_scale: lt.cone_scale,
                        };
                        if slot < state.picked.len() {
                            state.picked[slot] = entry;
                        } else {
                            state.picked.push(entry);
                        }
                    }
                }
            }
            LIGHT_POINT => {
                let dx = obj.bound_pos[0] - lt.w_pos[0];
                let dy = obj.bound_pos[1] - lt.w_pos[1];
                let dz = obj.bound_pos[2] - lt.w_pos[2];
                let dist_2 = (dx * dx + dy * dy) + dz * dz;
                let r = obj.bound_radius + lt.range_f;
                if !((r * r) <= dist_2) {
                    let energy = (lt.color[0] + lt.color[1] + lt.color[2]) / dist_2.sqrt();
                    if let Some(slot) = temp_light_slot(state, energy) {
                        let pos = transform_point(lt.w_pos, inv);
                        let range_n = scale * lt.range_n;
                        let range_f = scale * lt.range_f;
                        let mut delta = range_f - range_n;
                        if delta > 0.0f32 {
                            delta = 1.0f32 / delta;
                        }
                        let entry = TempLight {
                            pos,
                            dir: [0.0f32; 3],
                            color: [
                                temp_color[0] * lt.color[0],
                                temp_color[1] * lt.color[1],
                                temp_color[2] * lt.color[2],
                            ],
                            range_n,
                            range_f,
                            range_f_2: range_f * range_f,
                            range_delta: delta,
                            energy,
                            update_count: lt.update_count,
                            kind: lt.kind,
                            outer_cos: lt.outer_cos,
                            inner_cos: lt.inner_cos,
                            cone_scale: lt.cone_scale,
                        };
                        if slot < state.picked.len() {
                            state.picked[slot] = entry;
                        } else {
                            state.picked.push(entry);
                        }
                    }
                }
            }
            LIGHT_NULL | LIGHT_FOG | LIGHT_POINTAMBIENT | LIGHT_POINTFOG => continue,
            LIGHT_DIRECTIONAL => {
                let energy = (lt.color[0] + lt.color[1]) + lt.color[2];
                if let Some(slot) = temp_light_slot(state, energy) {
                    let mut dir = rotate_vector(lt.w_dir, inv);
                    dir[0] = -dir[0];
                    dir[1] = -dir[1];
                    dir[2] = -dir[2];
                    dir = normalize_f32(dir);
                    let entry = TempLight {
                        pos: [0.0f32; 3],
                        dir,
                        color: [
                            temp_color[0] * lt.color[0],
                            temp_color[1] * lt.color[1],
                            temp_color[2] * lt.color[2],
                        ],
                        range_n: 0.0f32,
                        range_f: 0.0f32,
                        range_f_2: 0.0f32,
                        range_delta: 0.0f32,
                        energy,
                        update_count: lt.update_count,
                        kind: lt.kind,
                        outer_cos: lt.outer_cos,
                        inner_cos: lt.inner_cos,
                        cone_scale: lt.cone_scale,
                    };
                    if slot < state.picked.len() {
                        state.picked[slot] = entry;
                    } else {
                        state.picked.push(entry);
                    }
                }
            }
            _ => continue,
        }
    }
    if !state.picked.is_empty() {
        for l in state.picked.iter() {
            key = key.wrapping_add(l.update_count);
        }
        key = key.wrapping_mul(state.picked.len() as u32);
    }
    if !force || (state.picked.is_empty() && key > 0) {
        if key == *cached_key {
            return false;
        }
    }
    *cached_key = key;
    if let Some(mat) = mat_for_color {
        if (level.mesh_flags & MESHF_TEXTURE) == 0 || (level.mesh_flags & MESHF_KEEP_DIFFUSE) != 0 {
            state.ambient[0] *= mat.ambient[0];
            state.ambient[1] *= mat.ambient[1];
            state.ambient[2] *= mat.ambient[2];
        }
        state.ambient[0] += mat.emissive[0];
        state.ambient[1] += mat.emissive[1];
        state.ambient[2] += mat.emissive[2];
    }
    if (obj.vis_flags & VISF_AVERAGE_LIGHT) != 0 {
        let mut r = 0u32;
        let mut g = 0u32;
        let mut b = 0u32;
        for i in 0..level.num_vertices {
            let c = light_vertex(state, src_pos[i], src_nrm[i]);
            r += (c >> 16) & 0xff;
            g += (c >> 8) & 0xff;
            b += c & 0xff;
        }
        let num = level.num_vertices as u32;
        let rr = if r / num > 0xff { 0xff } else { r / num };
        let gg = if g / num > 0xff { 0xff } else { g / num };
        let bb = if b / num > 0xff { 0xff } else { b / num };
        let color = state.alpha_word | rr << 16 | gg << 8 | bb;
        for i in 0..level.num_vertices {
            dst[i] = color;
        }
    } else {
        for i in 0..level.num_vertices {
            dst[i] = light_vertex(state, src_pos[i], src_nrm[i]);
        }
    }
    if let Some(rec) = rec {
        rec.alpha = (state.alpha_word >> 24) as u8;
        rec.dirty = false;
    }
    true
}

/// Next free slot or first weaker light's index, else `None` (`-1`).
fn temp_light_slot(state: &LightState, energy: f32) -> Option<usize> {
    if state.picked.len() < MAX_TEMP_LIGHTS {
        return Some(state.picked.len());
    }
    for (i, l) in state.picked.iter().enumerate() {
        if l.energy < energy {
            return Some(i);
        }
    }
    None
}

/// Normalize with the `fabs(sq - 1) < MRG_ZERO` fast path; zero stays zero.
fn normalize_f32(v: [f32; 3]) -> [f32; 3] {
    let sq = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if (sq - 1.0f32).abs() < MRG_ZERO {
        return v;
    }
    if sq < MRG_ZERO {
        return [0.0f32; 3];
    }
    let f = 1.0f32 / sq.sqrt();
    [v[0] * f, v[1] * f, v[2] * f]
}

/// One vertex color from the ambient sum and kept lights (`LightVertex`).
///
/// Directional/point/spot arms follow the assembly order (dots as
/// `(dx*nx + dy*ny) + dz*nz`, distances as `(dx*dx + dy*dy) + dz*dz`);
/// contributions below `MRG_ZERO` (signed compare) are skipped; channels
/// pack blue/green/red via round-to-nearest-even with `0xff` clamp under
/// the alpha (native `test ecx,0xffffff00` arm; kept distinct from the fog
/// low-byte stores below).
fn light_vertex(state: &LightState, pos: [f32; 3], nrm: [f32; 3]) -> u32 {
    let mut acc = state.ambient;
    for l in state.picked.iter() {
        match l.kind {
            LIGHT_DIRECTIONAL => {
                let t = (l.dir[0] * nrm[0] + l.dir[1] * nrm[1]) + l.dir[2] * nrm[2];
                // Native signed-bit test plus marginal-zero skip.
                if is_mrg_zero_less(t) {
                    continue;
                }
                acc[0] += l.color[0] * t;
                acc[1] += l.color[1] * t;
                acc[2] += l.color[2] * t;
            }
            LIGHT_POINT => {
                // Range-before-dot order (native tests dot first): unobservable
                // reorder on the admitted finite domain (pure reads, no effects).
                let dx = l.pos[0] - pos[0];
                let dy = l.pos[1] - pos[1];
                let dz = l.pos[2] - pos[2];
                let t = (dx * nrm[0] + dy * nrm[1]) + dz * nrm[2];
                if t < 0.0f32 {
                    continue;
                }
                let dist_2 = (dx * dx + dy * dy) + dz * dz;
                if dist_2 > l.range_f_2 {
                    continue;
                }
                let dist = dist_2.sqrt();
                let mut factor = t / dist;
                if dist > l.range_n {
                    factor *= (l.range_f - dist) * l.range_delta;
                }
                if is_mrg_zero_less(factor) {
                    continue;
                }
                acc[0] += l.color[0] * factor;
                acc[1] += l.color[1] * factor;
                acc[2] += l.color[2] * factor;
            }
            LIGHT_SPOT => {
                let dx = l.pos[0] - pos[0];
                let dy = l.pos[1] - pos[1];
                let dz = l.pos[2] - pos[2];
                let t = (dx * nrm[0] + dy * nrm[1]) + dz * nrm[2];
                if t < 0.0f32 {
                    continue;
                }
                let dist_2 = (dx * dx + dy * dy) + dz * dz;
                if dist_2 > l.range_f_2 {
                    continue;
                }
                let dist = dist_2.sqrt();
                if is_mrg_zero_less(dist) {
                    continue;
                }
                let mut att = 1.0f32;
                if dist > l.range_n {
                    att = (l.range_f - dist) * l.range_delta;
                }
                let inv = 1.0f32 / dist;
                let cos = ((l.dir[0] * dx + l.dir[1] * dy) + l.dir[2] * dz) * inv;
                if cos < 0.0f32 {
                    continue;
                }
                if cos <= l.outer_cos {
                    continue;
                }
                if cos <= l.inner_cos {
                    att *= (cos - l.outer_cos) * l.cone_scale;
                }
                let factor = (t * inv) * att;
                if is_mrg_zero_less(factor) {
                    continue;
                }
                acc[0] += l.color[0] * factor;
                acc[1] += l.color[1] * factor;
                acc[2] += l.color[2] * factor;
            }
            _ => continue,
        }
    }
    let b = fistp_clamp(acc[2] * state.mult);
    let g = fistp_clamp(acc[1] * state.mult);
    let r = fistp_clamp(acc[0] * state.mult);
    state.alpha_word | r << 16 | g << 8 | b
}

/// `fistp` pack: round-to-nearest-even, out-of-byte magnitudes clamp to
/// `0xff` (native `test ecx, 0xffffff00` arm; saturating host cast lands in
/// the same arm as the native `0x80000000` wrap).
fn fistp_clamp(scaled: f32) -> u32 {
    if !scaled.is_finite() {
        return 0xff;
    }
    let v = scaled.round_ties_even() as i32;
    if (v as u32) & 0xffff_ff00 != 0 {
        // Covers negatives and magnitudes past 255, as the native
        // `test ecx, 0xffffff00` arm does.
        return 0xff;
    }
    (v as u32) & 0xff
}

// ---------------------------------------------------------------------------
// 0x66b50 SetVertexAlpha (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Rewrite the color alpha bytes when the object alpha moved.
///
/// RVA 0x66b50. Reference: mafia-re ls3df/I3D/render/T_l.cpp:578-599.
/// Alpha is `fade * transparency * (material alpha or 1) * 255` rounded to
/// nearest-even (native `fistp`);
/// the latch is always cleared and the stride-stepped bytes rewritten only
/// on change.
pub fn set_vertex_alpha(
    dst: &mut [u32],
    dst_stride: usize,
    num_vertices: usize,
    fade: f32,
    transparency: f32,
    mat_alpha: Option<f32>,
    rec: &mut AlphaRec,
) {
    let a = match mat_alpha {
        Some(m) => fade * transparency * m * 255.0f32,
        None => fade * transparency * 255.0f32,
    };
    let alpha = float_to_int(a) as u8;
    rec.dirty = false;
    if alpha != rec.alpha {
        rec.alpha = alpha;
        let stride = if dst_stride == 0 { 1 } else { dst_stride };
        let mut at = 0usize;
        for _ in 0..num_vertices {
            if at < dst.len() {
                let rgb = dst[at] & 0x00ff_ffff;
                dst[at] = rgb | (u32::from(alpha) << 24);
            }
            at += stride;
        }
    }
}

// ---------------------------------------------------------------------------
// 0x66be0 TexelCallback (working guess, NOT original).
// ---------------------------------------------------------------------------

/// One texel in `A,R,G,B` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexelRgba {
    /// Alpha byte.
    pub a: u8,
    /// Red byte.
    pub r: u8,
    /// Green byte.
    pub g: u8,
    /// Blue byte.
    pub b: u8,
}

/// Texture examined by [`texel_callback`].
#[derive(Clone, Copy, Debug)]
pub struct TexelTexture<'a> {
    /// Texture width in texels.
    pub width: u32,
    /// Texture height in texels.
    pub height: u32,
    /// Row-major texels.
    pub texels: &'a [TexelRgba],
    /// Interpolated coordinates.
    pub u: f32,
    /// Interpolated coordinates.
    pub v: f32,
}

/// Material terms read by [`texel_callback`].
#[derive(Clone, Copy, Debug)]
pub struct TexelQuery<'a> {
    /// Diffuse color (used when no pixels are present).
    pub diffuse: [f32; 3],
    /// Material alpha multiplier.
    pub alpha: f32,
    /// Luminance stands in for alpha (`MATF_ADD_MODE`).
    pub add_mode: bool,
    /// Sampled texture (`None` takes the diffuse branch).
    pub texture: Option<TexelTexture<'a>>,
}

/// Texel effect on a gathered ray color (working guess, NOT original).
///
/// RVA 0x66be0 (`not_in_procedure` in the sampled session: contract via
/// entry bytes). Reference: mafia-re ls3df/I3D/render/T_l.cpp:616-657.
/// Coordinates scale by size and clamp with `min((w*u) as uint, w - 1)`
/// (truncation); `ADD` materials take luminance
/// (`0.3/0.6/0.1`) for alpha, others the texel alpha, times material alpha.
/// Below `0.1` is transparent, above `0.95` opaque, else the color blends
/// toward `col * texel` by alpha: `col *= 1-a; col += (col*c) * a` where the
/// second `col` is the already-scaled one (kept).
pub fn texel_callback(col: &mut [f32; 3], query: &TexelQuery) -> i32 {
    if let Some(tex) = &query.texture {
        if tex.width == 0 || tex.height == 0 || tex.texels.is_empty() {
            return blend_diffuse(col, query.diffuse, query.alpha);
        }
        // Negative-UV mapping like collision_scene::default_texel_callback:
        // in-range negatives wrap exactly (__ftol semantics); huge
        // magnitudes saturate in `as i32` where x86 is indefinite, but the
        // clamp below still picks the far edge on both sides (equivalent).
        let mut x = ((tex.width as f32 * tex.u) as i32) as u32;
        let mut y = ((tex.height as f32 * tex.v) as i32) as u32;
        if x > tex.width - 1 {
            x = tex.width - 1;
        }
        if y > tex.height - 1 {
            y = tex.height - 1;
        }
        let at = (y * tex.width + x) as usize;
        if at >= tex.texels.len() {
            return blend_diffuse(col, query.diffuse, query.alpha);
        }
        let t = tex.texels[at];
        let c = [
            f32::from(t.r) * (1.0f32 / 255.0f32),
            f32::from(t.g) * (1.0f32 / 255.0f32),
            f32::from(t.b) * (1.0f32 / 255.0f32),
        ];
        let mut a = if query.add_mode {
            c[0] * 0.3f32 + c[1] * 0.6f32 + c[2] * 0.1f32
        } else {
            f32::from(t.a) * (1.0f32 / 255.0f32)
        };
        a *= query.alpha;
        if a < 0.1f32 {
            return TEXEL_TRANSPARENT;
        }
        if a > 0.95f32 {
            return TEXEL_OPAQUE;
        }
        let k = 1.0f32 - a;
        col[0] *= k;
        col[1] *= k;
        col[2] *= k;
        col[0] += (col[0] * c[0]) * a;
        col[1] += (col[1] * c[1]) * a;
        col[2] += (col[2] * c[2]) * a;
        TEXEL_PARTIAL
    } else {
        blend_diffuse(col, query.diffuse, query.alpha)
    }
}

/// Diffuse branch of [`texel_callback`].
fn blend_diffuse(col: &mut [f32; 3], diffuse: [f32; 3], alpha: f32) -> i32 {
    if alpha < 0.1f32 {
        return TEXEL_TRANSPARENT;
    }
    if alpha > 0.95f32 {
        return TEXEL_OPAQUE;
    }
    let k = 1.0f32 - alpha;
    col[0] *= k;
    col[1] *= k;
    col[2] *= k;
    col[0] += (col[0] * diffuse[0]) * alpha;
    col[1] += (col[1] * diffuse[1]) * alpha;
    col[2] += (col[2] * diffuse[2]) * alpha;
    TEXEL_PARTIAL
}

// ---------------------------------------------------------------------------
// 0x66e90 VertexLightingProcess (PS2-evidenced name).
// ---------------------------------------------------------------------------

/// Lightmap material terms (`emissive`/`ambient`/`diffuse` selection).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapMaterial {
    /// Emissive color.
    pub emissive: [f32; 3],
    /// Ambient color.
    pub ambient: [f32; 3],
    /// Diffuse color.
    pub diffuse: [f32; 3],
    /// Textured without `KEEP_DIFFUSE` forces white ambient/diffuse.
    pub textured_no_keep: bool,
}

/// Shadow-hit classification for the occlusion test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowHit {
    /// Opaque visual hit (counts).
    Opaque,
    /// Billboard hit (ignored, as native).
    Billboard,
}

/// Backend shadow-ray probe (the `TestColGeo` boundary).
///
/// The lighting contract rereads live filter state through the probe and
/// never caches across it.
pub trait ShadowProbe {
    /// Cast from `from` along `dir`; `filter` is read and rewritten live.
    fn test_shadow(
        &mut self,
        from: [f32; 3],
        dir: [f32; 3],
        filter: &mut [f32; 3],
        col_normal: &mut [f32; 3],
    ) -> Option<ShadowHit>;
}

/// No-scene probe: every ray is unoccluded.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoShadow;

impl ShadowProbe for NoShadow {
    fn test_shadow(
        &mut self,
        _from: [f32; 3],
        _dir: [f32; 3],
        _filter: &mut [f32; 3],
        _col_normal: &mut [f32; 3],
    ) -> Option<ShadowHit> {
        None
    }
}

/// Lightmap texel color from sector lights (PS2-evidenced name).
///
/// RVA 0x66e90. Reference: mafia-re ls3df/I3D/render/T_l.cpp:685-829.
/// Order: material term selection, per-light ambient/point/spot/directional
/// sums (point `Square()` order `(x*x + y*y) + z*z` via `dot/dist` with
/// `(range_f - dist)` over the raw width (I3D.h:1946 documents range_delta
/// as raw; the TempLightList inversion is lightmap-path-only); spot adds the
/// `1/dist` cone test with the
/// `lt` reload order; directional `-dot`), shadow-ray filter per shadow
/// light (skipped for zero-sum colors; behind-face counting), inside test
/// (all rays from behind: magenta + `true`), final
/// `emissive + ambient*amb_sum + diffuse*diff_sum` scaled down to a max of 2
/// then clamped at 0. Returns the `inside` flag.
#[allow(clippy::too_many_arguments)]
pub fn vertex_lighting_process(
    out: &mut [f32; 3],
    pos: [f32; 3],
    normal: [f32; 3],
    mat: Option<LightmapMaterial>,
    lights: &[SectorLight],
    shadow: bool,
    inside_out: Option<&mut bool>,
    probe: &mut dyn ShadowProbe,
) -> bool {
    let (emissive, ambient, diffuse) = match mat {
        Some(m) => {
            if m.textured_no_keep {
                (m.emissive, [1.0f32; 3], [1.0f32; 3])
            } else {
                (m.emissive, m.ambient, m.diffuse)
            }
        }
        None => ([0.0f32; 3], [1.0f32; 3], [1.0f32; 3]),
    };
    let mut diff_sum = [0.0f32; 3];
    let mut amb_sum = [0.0f32; 3];
    let mut num_back = 0u32;
    let mut num_rays = 0u32;
    for lt in lights {
        if (lt.mode & LIGHTMODE_LIGHTMAP) == 0 || !lt.on {
            continue;
        }
        if is_abs_mrg_zero(lt.power) {
            continue;
        }
        let mut color = [0.0f32; 3];
        let from: [f32; 3];
        let dir: [f32; 3];
        let casts_ray: bool;
        match lt.kind {
            LIGHT_AMBIENT => {
                amb_sum[0] += lt.color[0];
                amb_sum[1] += lt.color[1];
                amb_sum[2] += lt.color[2];
                continue;
            }
            LIGHT_POINT => {
                let dx = lt.w_pos[0] - pos[0];
                let dy = lt.w_pos[1] - pos[1];
                let dz = lt.w_pos[2] - pos[2];
                let sq = (dx * dx + dy * dy) + dz * dz;
                if sq > lt.range_f_2 {
                    continue;
                }
                let dot = (dx * normal[0] + dy * normal[1]) + dz * normal[2];
                if dot <= 0.0f32 {
                    continue;
                }
                let dist = sq.sqrt();
                let dot_n = dot / dist;
                if dist <= lt.range_n {
                    color[0] = lt.color[0] * dot_n;
                    color[1] = lt.color[1] * dot_n;
                    color[2] = lt.color[2] * dot_n;
                } else {
                    // Raw width (I3D.h:1946 documents range_delta as range_f -
                    // range_n, maintained raw): the TempLightList inversion at
                    // T_l.cpp:462-465 applies to the lightmap staging path only.
                    let raw = lt.range_f - lt.range_n;
                    let k = dot_n * (lt.range_f - dist) / raw;
                    color[0] = lt.color[0] * k;
                    color[1] = lt.color[1] * k;
                    color[2] = lt.color[2] * k;
                }
                from = [
                    pos[0] + normal[0] * 0.0075f32,
                    pos[1] + normal[1] * 0.0075f32,
                    pos[2] + normal[2] * 0.0075f32,
                ];
                dir = [
                    lt.w_pos[0] - from[0],
                    lt.w_pos[1] - from[1],
                    lt.w_pos[2] - from[2],
                ];
                casts_ray = true;
            }
            LIGHT_SPOT => {
                let ldir = [-lt.w_dir[0], -lt.w_dir[1], -lt.w_dir[2]];
                let dx = lt.w_pos[0] - pos[0];
                let dy = lt.w_pos[1] - pos[1];
                let dz = lt.w_pos[2] - pos[2];
                let sq = (dx * dx + dy * dy) + dz * dz;
                if sq > lt.range_f_2 {
                    continue;
                }
                let dot = (dx * normal[0] + dy * normal[1]) + dz * normal[2];
                if dot <= 0.0f32 {
                    continue;
                }
                let dist = sq.sqrt();
                if is_mrg_zero_less(dist) {
                    continue;
                }
                let mut att = 1.0f32;
                if dist > lt.range_n {
                    let raw = lt.range_f - lt.range_n;
                    att = (lt.range_f - dist) / raw;
                }
                let inv = 1.0f32 / dist;
                let cos = ((ldir[0] * dx + ldir[1] * dy) + ldir[2] * dz) * inv;
                if cos <= lt.outer_cos {
                    continue;
                }
                if cos <= lt.inner_cos {
                    att *= (cos - lt.outer_cos) * lt.cone_scale;
                }
                let k = att * dot * inv;
                color[0] = lt.color[0] * k;
                color[1] = lt.color[1] * k;
                color[2] = lt.color[2] * k;
                from = [
                    pos[0] + normal[0] * 0.0075f32,
                    pos[1] + normal[1] * 0.0075f32,
                    pos[2] + normal[2] * 0.0075f32,
                ];
                dir = [
                    lt.w_pos[0] - from[0],
                    lt.w_pos[1] - from[1],
                    lt.w_pos[2] - from[2],
                ];
                casts_ray = true;
            }
            LIGHT_DIRECTIONAL => {
                let dot =
                    (lt.w_dir[0] * normal[0] + lt.w_dir[1] * normal[1]) + lt.w_dir[2] * normal[2];
                if dot >= 0.0f32 {
                    continue;
                }
                color[0] = lt.color[0] * -dot;
                color[1] = lt.color[1] * -dot;
                color[2] = lt.color[2] * -dot;
                from = [
                    pos[0] + normal[0] * 0.0075f32,
                    pos[1] + normal[1] * 0.0075f32,
                    pos[2] + normal[2] * 0.0075f32,
                ];
                dir = [-lt.w_dir[0], -lt.w_dir[1], -lt.w_dir[2]];
                casts_ray = true;
            }
            LIGHT_NULL | LIGHT_FOG | LIGHT_POINTAMBIENT | LIGHT_POINTFOG => continue,
            _ => continue,
        }
        if shadow && casts_ray && (lt.mode & LIGHTMODE_SHADOW) != 0 {
            if is_abs_mrg_zero((color[0] + color[1]) + color[2]) {
                continue;
            }
            num_rays += 1;
            let mut filter = [1.0f32; 3];
            let mut col_normal = [0.0f32; 3];
            if let Some(hit) = probe.test_shadow(from, dir, &mut filter, &mut col_normal) {
                if hit == ShadowHit::Opaque {
                    let mut nrm = dir;
                    let sq = (nrm[0] * nrm[0] + nrm[1] * nrm[1]) + nrm[2] * nrm[2];
                    // Degenerate zero-length ray keeps the zero vector (native
                    // Normalize yields inf/NaN and fails the backface test
                    // instead — disclosed; degenerate configurations only).
                    if sq > 0.0f32 {
                        let f = 1.0f32 / sq.sqrt();
                        nrm[0] *= f;
                        nrm[1] *= f;
                        nrm[2] *= f;
                    }
                    if (nrm[0] * col_normal[0] + nrm[1] * col_normal[1]) + nrm[2] * col_normal[2]
                        >= 0.0f32
                    {
                        num_back += 1;
                    }
                }
            }
            color[0] *= filter[0];
            color[1] *= filter[1];
            color[2] *= filter[2];
        }
        diff_sum[0] += color[0];
        diff_sum[1] += color[1];
        diff_sum[2] += color[2];
    }
    if num_back > 0 && num_back == num_rays {
        *out = [1.0f32, 0.0f32, 1.0f32];
        if let Some(inside) = inside_out {
            *inside = true;
        }
        return true;
    }
    out[0] = emissive[0] + (amb_sum[0] * ambient[0] + diffuse[0] * diff_sum[0]);
    out[1] = emissive[1] + (amb_sum[1] * ambient[1] + diffuse[1] * diff_sum[1]);
    out[2] = emissive[2] + (amb_sum[2] * ambient[2] + diffuse[2] * diff_sum[2]);
    let mut m = out[0];
    if out[1] > m {
        m = out[1];
    }
    if out[2] > m {
        m = out[2];
    }
    if m > 2.0f32 {
        let r = 1.0f32 / m;
        out[0] = out[0] * r * 2.0f32;
        out[1] = out[1] * r * 2.0f32;
        out[2] = out[2] * r * 2.0f32;
    }
    if out[0] < 0.0f32 {
        out[0] = 0.0f32;
    }
    if out[1] < 0.0f32 {
        out[1] = 0.0f32;
    }
    if out[2] < 0.0f32 {
        out[2] = 0.0f32;
    }
    if let Some(inside) = inside_out {
        *inside = false;
    }
    false
}

// ---------------------------------------------------------------------------
// 0x67d20 EnvProcess (PS2-evidenced name).
// ---------------------------------------------------------------------------

/// Environment-map source vertex (position plus normal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvVertex {
    /// Object-space position.
    pub pos: [f32; 3],
    /// Object-space normal.
    pub normal: [f32; 3],
}

/// Dummy box for `ENVMODE_DUMMY` (inverse matrix only).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DummyBox {
    /// Inverse of the dummy's matrix.
    pub inv: ClipMatrix,
}

/// Write environment-map coordinates (PS2-evidenced name).
///
/// RVA 0x67d20. Reference: mafia-re ls3df/I3D/render/T_l.cpp:844-1213.
/// Modes 0/1 sphere-map the camera-reflected direction (`+u_ofs/+v_ofs`,
/// radius scaled by `scale`, degenerate center writes `0.5 + ofs`);
/// mode 1's center case falls through into mode 0's loop in native (kept
/// here as the shared center write, disclosed). Modes 2/3/4 project world
/// position pairs scaled by `envmap_scale * scale * 0.3535534`; mode 5 maps
/// into the dummy box with `z` clamped and `1 - y` flipped. Unknown modes
/// write nothing.
#[allow(clippy::too_many_arguments)]
pub fn env_process(
    dst: &mut [[f32; 2]],
    src: &[EnvVertex],
    cam_pos: [f32; 3],
    m: &ClipMatrix,
    scale: f32,
    envmap_scale: f32,
    u_ofs: f32,
    v_ofs: f32,
    mode: u32,
    dummy: Option<&DummyBox>,
) {
    let count = dst.len().min(src.len());
    match mode {
        ENVMODE_REFLECT_Z => {
            for i in 0..count {
                dst[i] = sphere_uv(&src[i], cam_pos, m, scale, u_ofs, v_ofs, false);
            }
        }
        ENVMODE_REFLECT_Y => {
            for i in 0..count {
                dst[i] = sphere_uv(&src[i], cam_pos, m, scale, u_ofs, v_ofs, true);
            }
        }
        ENVMODE_PROJECT_Y => {
            let k = envmap_scale * scale * 0.3535534f32;
            for i in 0..count {
                let p = transform_point(src[i].pos, m);
                dst[i] = [p[0] * k, p[2] * k];
            }
        }
        ENVMODE_PROJECT_X => {
            let k = envmap_scale * scale * 0.3535534f32;
            for i in 0..count {
                let p = transform_point(src[i].pos, m);
                dst[i] = [p[1] * k, p[2] * k];
            }
        }
        ENVMODE_PROJECT_Z => {
            let k = envmap_scale * scale * 0.3535534f32;
            for i in 0..count {
                let p = transform_point(src[i].pos, m);
                dst[i] = [p[0] * k, p[1] * k];
            }
        }
        ENVMODE_DUMMY => {
            let Some(d) = dummy else { return };
            let mat = mul_matrix(m, &d.inv);
            for i in 0..count {
                let p = transform_point(src[i].pos, &mat);
                let u = if p[2] < 0.0f32 {
                    0.0f32
                } else if p[2] > 1.0f32 {
                    1.0f32
                } else {
                    p[2]
                };
                let v = if p[1] < 0.0f32 {
                    1.0f32
                } else if p[1] > 1.0f32 {
                    0.0f32
                } else {
                    1.0f32 - p[1]
                };
                dst[i] = [u, v];
            }
        }
        _ => {}
    }
}

/// Sphere-map UV for one vertex; `around_y` selects mode 1's projection.
///
/// Shared center write covers mode 1's fallthrough into mode 0's loop:
/// native resumes at the mode-0 label, which writes the same center value.
fn sphere_uv(
    v: &EnvVertex,
    cam_pos: [f32; 3],
    m: &ClipMatrix,
    scale: f32,
    u_ofs: f32,
    v_ofs: f32,
    around_y: bool,
) -> [f32; 2] {
    let p = transform_point(v.pos, m);
    let n = normalize_f32(rotate_vector(v.normal, m));
    let view = [cam_pos[0] - p[0], cam_pos[1] - p[1], cam_pos[2] - p[2]];
    let d = (n[0] * view[0] + n[1] * view[1]) + n[2] * view[2];
    let r = [
        (2.0f32 * d) * n[0] - view[0],
        (2.0f32 * d) * n[1] - view[1],
        (2.0f32 * d) * n[2] - view[2],
    ];
    let sq = (r[0] * r[0] + r[1] * r[1]) + r[2] * r[2];
    if is_mrg_zero_less(sq) {
        // Center case (mode-1 falls through to mode-0 here in native).
        return [0.5f32 + u_ofs, 0.5f32 + v_ofs];
    }
    let inv = 1.0f32 / sq.sqrt();
    let nx = r[0] * inv;
    let ny = r[1] * inv;
    let nz = r[2] * inv;
    if around_y {
        let denom = nz + 1.0f32;
        if is_mrg_zero_less(denom * denom) {
            return [0.5f32 + u_ofs, 0.5f32 + v_ofs];
        }
        let s = scale / denom.sqrt() / denom;
        [nx * s + 0.5f32 + u_ofs, ny * s + 0.5f32 + v_ofs]
    } else {
        let denom = nz + 1.0f32;
        if is_mrg_zero_less(denom * denom) {
            return [0.5f32 + u_ofs, 0.5f32 + v_ofs];
        }
        let s = scale / denom.sqrt() / denom;
        [-nx * s + 0.5f32 + u_ofs, ny * s + 0.5f32 + v_ofs]
    }
}

// ---------------------------------------------------------------------------
// 0x685d0 FogProcess (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Fog depth input (`z`, with `w` for the sub-unit perspective divide).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogVertex {
    /// Depth.
    pub z: f32,
    /// Homogeneous divisor used when `0 <= z < 1`.
    pub w: f32,
}

/// Fog light ranges (fractions of the camera far plane).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogLight {
    /// Near fraction.
    pub range_n: f32,
    /// Far fraction.
    pub range_f: f32,
}

/// Write per-vertex fog bytes (working guess, NOT original).
///
/// RVA 0x685d0. Reference: mafia-re ls3df/I3D/render/T_l.cpp:1455-1580.
/// `range_f = far * light.far`, `range_n = range_f * light.near`,
/// `scale = 255 / (range_f - range_n)`. Depth is `z` when `z < 0` or
/// `z >= 1`, else `z / w`. `FACTOR` writes 255 near / 0 far with
/// `(range_f - depth) * scale` rounded to nearest-even, low byte kept
/// (native `mov al`, unlike the light-pack clamp above); `ALPHA` writes the
/// inverse (`0xff - byte`). Out-of-range depths take the ends directly.
pub fn fog_process(
    dst: &mut [u8],
    src: &[FogVertex],
    far_clip: f32,
    light: Option<&FogLight>,
    mode: u32,
) {
    let Some(lt) = light else { return };
    let range_f = far_clip * lt.range_f;
    let range_n = range_f * lt.range_n;
    let scale = 255.0f32 / (range_f - range_n);
    let count = dst.len().min(src.len());
    for i in 0..count {
        let z = src[i].z;
        let depth = if z < 0.0f32 || z - 1.0f32 >= 0.0f32 {
            z
        } else {
            z / src[i].w
        };
        match mode {
            FOGPROC_FACTOR => {
                if depth - range_f >= 0.0f32 {
                    dst[i] = 0;
                } else if depth - range_n < 0.0f32 {
                    dst[i] = 0xff;
                } else {
                    dst[i] = (float_to_int((range_f - depth) * scale) as u32 & 0xff) as u8;
                }
            }
            FOGPROC_ALPHA => {
                if depth - range_f >= 0.0f32 {
                    dst[i] = 0xff;
                } else if depth - range_n < 0.0f32 {
                    dst[i] = 0;
                } else {
                    let b = (float_to_int((range_f - depth) * scale) as u32 & 0xff) as u8;
                    dst[i] = 0xff - b;
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// 0x68760 SwapVertexBytes (working guess, NOT original).
// ---------------------------------------------------------------------------

/// Swap one byte per face vertex, once (working guess, NOT original).
///
/// RVA 0x68760. Reference: mafia-re ls3df/I3D/render/T_l.cpp:1584-1619.
/// Byte slices are the explicit vertex-record boundary (not a CPU map).
/// For each face corner `ofs = vi * stride`, when `b[ofs - 1]` holds the
/// `from` marker the bytes `a[ofs]`/`b[ofs]` are exchanged and the marker
/// becomes `to` (`0 -> 1`, or `1 -> 0` when restoring). Out-of-range
/// corners are skipped (native would overrun; disclosed).
pub fn swap_vertex_bytes(
    a: &mut [u8],
    b: &mut [u8],
    stride: usize,
    faces: &[Triface],
    restore: bool,
) {
    let (from, to) = if restore { (1u8, 0u8) } else { (0u8, 1u8) };
    for face in faces {
        for k in 0..3 {
            let ofs = face.vi[k] as usize * stride;
            if ofs == 0 || ofs >= a.len() || ofs >= b.len() {
                continue;
            }
            if b[ofs - 1] == from {
                let c = a[ofs];
                a[ofs] = b[ofs];
                b[ofs] = c;
                b[ofs - 1] = to;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests: one hand-derived vector per function (authored, not executed).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_state_ortho() -> ClipState {
        let mut s = ClipState::new();
        set_clip_params(
            &mut s,
            &ClipMatrix::identity(),
            &ClipMatrix::identity(),
            true,
            0.0,
            0.0,
            1.0,
        );
        s
    }

    #[test]
    fn set_clip_params_derives_ratios() {
        // World scale diag(2,3,4): ratio_x = 4/2, ratio_y = 4/3.
        let mut s = ClipState::new();
        set_clip_params(
            &mut s,
            &ClipMatrix::identity(),
            &ClipMatrix::from_scale(2.0, 3.0, 4.0),
            false,
            0.5,
            0.25,
            2.0,
        );
        assert_eq!(s.ratio_x, 4.0f32 / 2.0f32);
        assert_eq!(s.ratio_y, 4.0f32 / 3.0f32);
        assert_eq!(s.lift, 0.5);
        assert_eq!(s.spread, 0.25);
        assert_eq!(s.inv_reverse, 1.0f32 / 2.0f32);
        assert!(s.backfaces);
        assert!(!s.is_ortho);
    }

    #[test]
    fn reset_clip_arrays_clears_counts() {
        let mut s = ClipState::new();
        s.num_results = 7;
        s.num_faces = 3;
        s.num_faces_t = 2;
        s.center_t = [1.0, 2.0, 3.0];
        s.num_results_t = 5;
        reset_clip_arrays(&mut s);
        assert_eq!(s.num_results, 0);
        assert_eq!(s.num_faces, 0);
        assert_eq!(s.num_faces_t, 0);
        assert_eq!(s.center_t, [0.0, 0.0, 0.0]);
        assert_eq!(s.num_results_t, 0);
    }

    #[test]
    fn reserve_clip_arrays_grows_only() {
        let mut s = identity_state_ortho();
        assert!(reserve_clip_arrays(&mut s, 10, 5));
        // face need 5*8=40 -> MIN 128; vertex need 10+60=70 -> MIN 512.
        assert_eq!(s.faces_alloc, 128);
        assert_eq!(s.verts_size, 512);
        assert_eq!(s.verts_max, 512 - 15);
        assert!(reserve_clip_arrays(&mut s, 1, 1));
        assert_eq!(s.faces_alloc, 128);
        assert_eq!(s.verts_size, 512);
    }

    #[test]
    fn free_clip_arrays_keeps_face_leak_and_clears() {
        let mut s = identity_state_ortho();
        assert!(reserve_clip_arrays(&mut s, 4, 4));
        s.faces.push(Triface { vi: [0, 1, 2] });
        s.results.push([1.0, 2.0, 3.0]);
        s.num_results = 1;
        s.result_alloc = 512;
        free_clip_arrays(&mut s);
        assert!(s.verts.is_empty());
        assert_eq!(s.verts_size, 0);
        assert!(s.faces.is_empty());
        assert!(s.faces_t.is_empty());
        assert_eq!(s.faces_alloc, 0);
        assert!(s.results.is_empty());
        assert_eq!(s.num_results, 0);
        assert_eq!(s.result_alloc, 0);
    }

    fn inside_triangle_state() -> ClipState {
        // Ortho identity, triangle strictly inside the unit box.
        let mut s = identity_state_ortho();
        assert!(reserve_clip_arrays(&mut s, 3, 1));
        let pts = [[0.0f32, 0.0f32, 0.5f32]; 3];
        s.verts[0] = ClipVertex {
            pos: pts[0],
            index: -1,
            clip: 0,
        };
        s.verts[1] = ClipVertex {
            pos: [0.5, 0.0, 0.5],
            index: -1,
            clip: 0,
        };
        s.verts[2] = ClipVertex {
            pos: [0.0, 0.5, 0.5],
            index: -1,
            clip: 0,
        };
        s.vert_count = 3;
        s.vert_end = 3;
        s.two_sided = 1;
        s
    }

    #[test]
    fn clip_faces_ortho_appends_inside_face() {
        let mut s = inside_triangle_state();
        clip_faces_ortho(&mut s, &[Triface { vi: [0, 1, 2] }]);
        assert_eq!(s.faces.len(), 1);
        assert_eq!(s.faces[0].vi, [0, 1, 2]);
        assert_eq!(s.vert_count, 3);
    }

    #[test]
    fn clip_faces_persp_appends_inside_face() {
        let mut s = ClipState::new();
        set_clip_params(
            &mut s,
            &ClipMatrix::identity(),
            &ClipMatrix::identity(),
            false,
            0.0,
            0.0,
            0.0,
        );
        assert!(reserve_clip_arrays(&mut s, 3, 1));
        // |x|<=z, |y|<=z, 0<=z<=1 with z=0.5.
        for (i, p) in [[0.0f32, 0.0f32, 0.5f32], [0.25, 0.0, 0.5], [0.0, 0.25, 0.5]]
            .iter()
            .enumerate()
        {
            s.verts[i] = ClipVertex {
                pos: *p,
                index: -1,
                clip: 0,
            };
        }
        s.vert_count = 3;
        s.vert_end = 3;
        s.two_sided = 1;
        assert!(clip_faces_persp(&mut s, &[Triface { vi: [0, 1, 2] }]));
        assert_eq!(s.faces.len(), 1);
    }

    #[test]
    fn clip_mesh_end_to_end_ortho() {
        let mut s = identity_state_ortho();
        let groups = [FaceGroup {
            faces: vec![Triface { vi: [0, 1, 2] }],
            material: None,
        }];
        let verts = [[0.0f32, 0.0f32, 0.5f32], [0.5, 0.0, 0.5], [0.0, 0.5, 0.5]];
        assert!(clip_mesh(
            &mut s,
            &verts,
            &groups,
            0,
            &ClipMatrix::identity()
        ));
        // transform, flag, cull, no clipping, fan, index, result-copy.
        assert_eq!(s.num_results, 3);
        assert_eq!(s.results[0], [0.0, 0.0, 0.5]);
        assert_eq!(s.faces[0].vi, [0, 1, 2]);
    }

    #[test]
    fn clip_facegroups_skips_transparent_without_alpha() {
        let mut s = identity_state_ortho();
        assert!(reserve_clip_arrays(&mut s, 3, 1));
        for (i, p) in [[0.0f32, 0.0f32, 0.5f32], [0.5, 0.0, 0.5], [0.0, 0.5, 0.5]]
            .iter()
            .enumerate()
        {
            s.verts[i] = ClipVertex {
                pos: *p,
                index: -1,
                clip: 0,
            };
        }
        s.vert_count = 3;
        s.with_alpha = 0;
        let groups = [FaceGroup {
            faces: vec![Triface { vi: [0, 1, 2] }],
            material: Some(ClipMaterial {
                flags: MATF_TXT_ALPHA,
                alpha: 1.0,
            }),
        }];
        assert!(!clip_facegroups(&mut s, &groups));
        assert!(s.faces.is_empty() && s.faces_t.is_empty());
    }

    #[test]
    fn clip_mesh_w_uses_clip_matrix_directly() {
        let mut s = identity_state_ortho();
        let groups = [FaceGroup {
            faces: vec![Triface { vi: [0, 1, 2] }],
            material: None,
        }];
        let verts = [[0.0f32, 0.0f32, 0.5f32], [0.5, 0.0, 0.5], [0.0, 0.5, 0.5]];
        assert!(clip_mesh_w(&mut s, &verts, &groups, 0));
        assert_eq!(s.num_results, 3);
    }

    #[test]
    fn clip_single_mesh_matches_mesh() {
        let mut s = identity_state_ortho();
        let groups = [FaceGroup {
            faces: vec![Triface { vi: [0, 1, 2] }],
            material: None,
        }];
        let verts = [[0.0f32, 0.0f32, 0.5f32], [0.5, 0.0, 0.5], [0.0, 0.5, 0.5]];
        assert!(clip_single_mesh(
            &mut s,
            &verts,
            &groups,
            0,
            &ClipMatrix::identity()
        ));
        assert_eq!(s.num_results, 3);
    }

    #[test]
    fn clip_single_mesh_w_matches_mesh_w() {
        let mut s = identity_state_ortho();
        let groups = [FaceGroup {
            faces: vec![Triface { vi: [0, 1, 2] }],
            material: None,
        }];
        let verts = [[0.0f32, 0.0f32, 0.5f32], [0.5, 0.0, 0.5], [0.0, 0.5, 0.5]];
        assert!(clip_single_mesh_w(&mut s, &verts, &groups, 0));
        assert_eq!(s.num_results, 3);
    }

    #[test]
    fn radix_sort_32_orders_unsigned() {
        let mut data = [
            SortEntry32 {
                sort_value: 30,
                data: 3,
            },
            SortEntry32 {
                sort_value: 10,
                data: 1,
            },
            SortEntry32 {
                sort_value: 20,
                data: 2,
            },
        ];
        let mut storage = SortStorage::default();
        radix_sort_32(&mut data, &mut storage);
        assert_eq!(data[0].sort_value, 10);
        assert_eq!(data[1].sort_value, 20);
        assert_eq!(data[2].sort_value, 30);
        assert_eq!(data[0].data, 1);
        // Uniform high bytes are skipped without disturbing order.
        let mut single = [SortEntry32 {
            sort_value: 0x00ff_00ff,
            data: 9,
        }];
        radix_sort_32(&mut single, &mut storage);
        assert_eq!(single[0].data, 9);
    }

    #[test]
    fn lighting_set_sector_latches_presence() {
        let mut s = LightState::new();
        lighting_set_sector(&mut s, true);
        assert!(s.sector_present);
        lighting_set_sector(&mut s, false);
        assert!(!s.sector_present);
    }

    #[test]
    fn prepare_lights_ambient_only_packs() {
        // Ambient [0.1,0.2,0.3], lightness 1, fade 1: bytes 12/25/38 at 128.
        let mut st = LightState::new();
        let level = LitMeshLevel {
            num_vertices: 1,
            mesh_flags: 0,
            material: Some(LitMaterial {
                diffuse: [1.0, 1.0, 1.0],
                ambient: [1.0, 1.0, 1.0],
                emissive: [0.0, 0.0, 0.0],
                alpha: 1.0,
            }),
        };
        let obj = LitObject {
            lightness: 1.0,
            fade: 1.0,
            transparency: 1.0,
            bound_pos: [0.0, 0.0, 0.0],
            bound_radius: 1.0,
            vis_flags: 0,
        };
        let sector = [SectorLight {
            kind: LIGHT_AMBIENT,
            mode: LIGHTMODE_VERTEX,
            on: true,
            power: 1.0,
            color: [0.1, 0.2, 0.3],
            w_pos: [0.0, 0.0, 0.0],
            w_dir: [0.0, 0.0, 1.0],
            radius: 0.0,
            range_n: 0.0,
            range_f: 0.0,
            range_f_2: 0.0,
            outer_cos: 0.0,
            inner_cos: 0.0,
            cone_scale: 0.0,
            update_count: 7,
        }];
        let mut dst = [0u32; 1];
        let pos = [[0.0f32, 0.0f32, 0.0f32]];
        let nrm = [[0.0f32, 0.0f32, 1.0f32]];
        let mut key = 0u32;
        let mut rec = AlphaRec {
            alpha: 0,
            dirty: true,
        };
        assert!(prepare_lights(
            &mut st,
            &mut dst,
            &pos,
            &nrm,
            &level,
            &obj,
            &ClipMatrix::identity(),
            &sector,
            LIGHTMODE_VERTEX,
            true,
            &mut key,
            Some(&mut rec),
        ));
        // R2N fistp: 0.1*128=12.80000019->13, 0.2*128=25.6000004->26,
        // 0.3*128=38.4000015->38.
        let r = 13u32;
        let g = 26u32;
        let b = 38u32;
        assert_eq!(dst[0], 0xff00_0000 | r << 16 | g << 8 | b);
        assert_eq!(rec.alpha, 0xff);
        assert!(!rec.dirty);
        // Same key without force writes nothing.
        let mut dst2 = [0u32; 1];
        assert!(!prepare_lights(
            &mut st,
            &mut dst2,
            &pos,
            &nrm,
            &level,
            &obj,
            &ClipMatrix::identity(),
            &sector,
            LIGHTMODE_VERTEX,
            false,
            &mut key,
            None,
        ));
    }

    #[test]
    fn set_vertex_alpha_rewrites_on_change() {
        let mut dst = [0xff00_0000u32, 0xff00_0000u32];
        let mut rec = AlphaRec {
            alpha: 0,
            dirty: true,
        };
        set_vertex_alpha(&mut dst, 1, 2, 1.0, 1.0, Some(0.5), &mut rec);
        // 0.5*255 = 127.5 -> R2N fistp 128.
        let a = 128u8;
        assert_eq!(rec.alpha, a);
        assert!(!rec.dirty);
        assert_eq!(dst[0] >> 24, u32::from(a));
        // Second call with the same alpha leaves the words alone.
        let before = dst;
        set_vertex_alpha(&mut dst, 1, 2, 1.0, 1.0, Some(0.5), &mut rec);
        assert_eq!(dst, before);
    }

    #[test]
    fn texel_callback_classifies_and_blends() {
        // Opaque branch (alpha 1.0, no texture).
        let mut col = [1.0f32, 1.0f32, 1.0f32];
        let q = TexelQuery {
            diffuse: [1.0, 0.0, 0.0],
            alpha: 1.0,
            add_mode: false,
            texture: None,
        };
        assert_eq!(texel_callback(&mut col, &q), TEXEL_OPAQUE);
        // Transparent branch.
        let q2 = TexelQuery {
            diffuse: [1.0, 0.0, 0.0],
            alpha: 0.05,
            add_mode: false,
            texture: None,
        };
        assert_eq!(texel_callback(&mut col, &q2), TEXEL_TRANSPARENT);
        // Partial: col=1, c=1, a=0.5 -> 0.5 + 0.5*0.5 = 0.75 per channel.
        let mut col3 = [1.0f32, 1.0f32, 1.0f32];
        let q3 = TexelQuery {
            diffuse: [1.0, 1.0, 1.0],
            alpha: 0.5,
            add_mode: false,
            texture: None,
        };
        assert_eq!(texel_callback(&mut col3, &q3), TEXEL_PARTIAL);
        assert_eq!(col3, [0.75, 0.75, 0.75]);
        // Textured opaque white texel.
        let texels = [TexelRgba {
            a: 255,
            r: 255,
            g: 255,
            b: 255,
        }];
        let tex = TexelTexture {
            width: 1,
            height: 1,
            texels: &texels,
            u: 0.0,
            v: 0.0,
        };
        let mut col4 = [0.2f32, 0.3f32, 0.4f32];
        let q4 = TexelQuery {
            diffuse: [0.0, 0.0, 0.0],
            alpha: 1.0,
            add_mode: false,
            texture: Some(tex),
        };
        assert_eq!(texel_callback(&mut col4, &q4), TEXEL_OPAQUE);
    }

    #[test]
    fn vertex_lighting_no_lights_yields_emissive() {
        let mut out = [0.0f32; 3];
        let mut probe = NoShadow;
        let mat = LightmapMaterial {
            emissive: [0.1, 0.2, 0.3],
            ambient: [0.5, 0.5, 0.5],
            diffuse: [0.5, 0.5, 0.5],
            textured_no_keep: false,
        };
        let mut inside = true;
        let is_inside = vertex_lighting_process(
            &mut out,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            Some(mat),
            &[],
            false,
            Some(&mut inside),
            &mut probe,
        );
        assert!(!is_inside);
        assert!(!inside);
        assert_eq!(out, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn fistp_rounds_to_nearest_even() {
        // Pins the shared FloatToInt helper itself (ls3df.h:83 fld/fistp):
        // 2.5->2, 3.5->4, 127.5->128, -0.5->0 (never truncation).
        assert_eq!(float_to_int(2.5), 2);
        assert_eq!(float_to_int(3.5), 4);
        assert_eq!(float_to_int(127.5), 128);
        assert_eq!(float_to_int(-0.5), 0);
        assert_eq!(float_to_int(0.49999997), 0);
    }
    #[test]
    fn vertex_light_point_attenuation_uses_raw_width() {
        // T_l.cpp:685-829 point arm divides by the RAW width
        // (I3D.h:1946): range_n=2, range_f=6, dist=4, dot_n=1 gives
        // k = 1*(6-4)/(6-2) = 0.5. The inverted TempLight delta would give
        // 2.0 (then scaled to [2,0,0]); native gives [0.5,0,0].
        let light = SectorLight {
            kind: LIGHT_POINT,
            mode: LIGHTMODE_LIGHTMAP,
            on: true,
            power: 1.0,
            color: [1.0, 0.0, 0.0],
            w_pos: [0.0, 0.0, 4.0],
            w_dir: [0.0, 0.0, 1.0],
            radius: 0.0,
            range_n: 2.0,
            range_f: 6.0,
            range_f_2: 36.0,
            outer_cos: 0.0,
            inner_cos: 0.0,
            cone_scale: 0.0,
            update_count: 0,
        };
        let mut out = [9.0f32; 3];
        let mut probe = NoShadow;
        let hit = vertex_lighting_process(
            &mut out,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            None,
            &[light],
            false,
            None,
            &mut probe,
        );
        assert!(!hit);
        assert_eq!(out, [0.5, 0.0, 0.0]);
    }
    #[test]
    fn env_process_project_z_scales() {
        // k = 2*1*0.3535534; pos [1,2,0] -> [k, 2k].
        let src = [EnvVertex {
            pos: [1.0, 2.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        }];
        let mut dst = [[0.0f32; 2]];
        env_process(
            &mut dst,
            &src,
            [0.0, 0.0, 5.0],
            &ClipMatrix::identity(),
            1.0,
            2.0,
            0.0,
            0.0,
            ENVMODE_PROJECT_Z,
            None,
        );
        let k = 2.0f32 * 1.0f32 * 0.3535534f32;
        assert_eq!(dst[0], [1.0f32 * k, 2.0f32 * k]);
        // Dummy mode clamps z and flips y.
        let dummy = DummyBox {
            inv: ClipMatrix::identity(),
        };
        let src2 = [EnvVertex {
            pos: [0.0, 2.0, 5.0],
            normal: [0.0, 0.0, 1.0],
        }];
        let mut dst2 = [[9.0f32; 2]];
        env_process(
            &mut dst2,
            &src2,
            [0.0, 0.0, 5.0],
            &ClipMatrix::identity(),
            1.0,
            1.0,
            0.0,
            0.0,
            ENVMODE_DUMMY,
            Some(&dummy),
        );
        assert_eq!(dst2[0], [1.0, 0.0]);
    }

    #[test]
    fn fog_process_factor_midpoint() {
        // range_f = 10*1 = 10, range_n = 0, scale = 25.5.
        let light = FogLight {
            range_n: 0.0,
            range_f: 1.0,
        };
        let src = [
            FogVertex { z: 0.0, w: 1.0 },
            FogVertex { z: 5.0, w: 1.0 },
            FogVertex { z: 10.0, w: 1.0 },
        ];
        let mut dst = [9u8; 3];
        fog_process(&mut dst, &src, 10.0, Some(&light), FOGPROC_FACTOR);
        assert_eq!(dst[0], 0xff);
        // (10-5)*25.5 = 127.5 -> R2N fistp 128.
        assert_eq!(dst[1], 128);
        assert_eq!(dst[2], 0);
        // Alpha mode inverts the ends.
        let mut dst2 = [9u8; 3];
        fog_process(&mut dst2, &src, 10.0, Some(&light), FOGPROC_ALPHA);
        assert_eq!(dst2[0], 0);
        assert_eq!(dst2[2], 0xff);
    }

    #[test]
    fn swap_vertex_bytes_swaps_once_and_restores() {
        // stride 4: flag at ofs-1, value at ofs. vi=1 swaps, vi=2 carries a
        // neutral marker so both directions skip it, repeated vi=1 swaps once.
        let mut a = [60u8, 61, 62, 63, 20, 65, 66, 67, 30, 69, 70, 71];
        let mut b = [50u8, 51, 52, 0, 200, 201, 202, 2, 210, 211, 212, 213];
        let faces = [Triface { vi: [1, 2, 1] }];
        swap_vertex_bytes(&mut a, &mut b, 4, &faces, false);
        assert_eq!(a[4], 200);
        assert_eq!(b[4], 20);
        assert_eq!(b[3], 1);
        assert_eq!(a[8], 30);
        assert_eq!(b[8], 210);
        swap_vertex_bytes(&mut a, &mut b, 4, &faces, true);
        assert_eq!(a[4], 20);
        assert_eq!(b[4], 200);
        assert_eq!(b[3], 0);
        assert_eq!(a[8], 30);
    }
}
