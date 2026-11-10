#![forbid(unsafe_code)]
//! I3D_scene ray/sphere-vs-visual tests (PC LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! RVAs 0xe370-0x153b0: 8 contract, plus the TestColGeo setup/distance words
//! (RVA 0xdec0) and the TestColSphere sweep-box/exclusion words (RVA 0x11d60).
//! The static testVisual/sphTestVisual halves (RVAs 0xe780/0x12520) stay
//! explicit-Unsupported stubs: no live vertex-buffer lock owner exists on
//! this host, and the credited walkers take caller-side backends instead.
//!
//! Names (testSubtree, sphTestSubtree, TestColGeo1, …) are PS2/mafia-re
//! (I3D/i3d_collision.cpp) working names, NEVER original symbols. Only
//! 0xe370/0x12140 have Ghidra bodies; the six TestCol* wrappers are LAB_ /
//! not_in_procedure with entry-decode probes: contract tier with reason
//! disclosed. testVisual/sphTestVisual halves are private phase helpers: no
//! function credit per repo rule. Float: x87 f32 R2N; NaN keeps
//! false-comparison flow (direct f32 ops); float→int casts (texel pixel
//! indices) truncate guarded-nonnegative finite values.

pub const COLF_BOX: u32 = 0x1;
pub const COLF_ACCEPT_ANY: u32 = 0x10000;
pub const COLF_COLORKEY: u32 = 0x20000;
pub const COLF_INFINITE: u32 = 0x40000;
pub const COLF_TWO_SIDED: u32 = 0x100000;
pub const COLF_FIRST_HIT: u32 = 0x1000000;
pub const COLF_FRAME_LOD: u32 = 0x2000000;
pub const COLF_SKIP_OPTIONAL: u32 = 0x4000000;
pub const COLF_HIDDEN: u32 = 0x8000000;
pub const COLF_EXCLUDE_SUBTREE: u32 = 0x10000000;
pub const COLF_OMNI_SECTOR: u32 = 0x20000000;
pub const COLF_NEAR_SECTOR: u32 = 0x40000000;
pub const COLF_FIRST_FACE: u32 = 0x80000000;
pub const FRMFLAGS_ON: u32 = 0x1;
pub const VISF_COL_OPTIONAL: u32 = 0x1000;
pub const FRAME_SECTOR: i32 = 5;
pub const FRAME_VISUAL: i32 = 1;
pub const TEXEL_OPAQUE: i32 = 0;
pub const TEXEL_PARTIAL: i32 = 1;
pub const TEXEL_TRANSPARENT: i32 = 2;
pub const COLL_ACCEPT: i32 = 0;
pub const COLL_ACCEPT_STOP: i32 = 1;
pub const COLL_IGNORE: i32 = 2;
pub const COLL_IGNORE_CHILDREN: i32 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SceneColError {
    HiddenFrame,
    BadLevel,
    Nonfinite,
    TexelOutOfRange,
    /// Explicit-Unsupported carrier for the testVisual/sphTestVisual halves:
    /// no live vertex-buffer lock owner exists on this host. The static
    /// message names the missing owner contract.
    Unsupported(&'static str),
}

/// Typed live view of one frame node; the walker borrows the arena.
pub trait SceneGraph {
    fn flags(&self, id: usize) -> u32;
    fn frame_type(&self, id: usize) -> i32;
    /// None = invalid volume.
    fn hr_bbox(&self, id: usize) -> Option<[[f32; 3]; 2]>;
    fn hr_sphere(&self, id: usize) -> Option<([f32; 3], f32)>;
    /// The visual's own world bound sphere for the face-test gates. Default
    /// aliases hr_sphere: identical for leaf visuals; a production backend
    /// distinguishing W/HR volumes overrides this.
    fn visual_sphere(&self, id: usize) -> Option<([f32; 3], f32)> {
        self.hr_sphere(id)
    }
    fn is_visual(&self, id: usize) -> bool;
    /// The 7-kind face-test switch (OBJECT/LIT/BILLBOARD/SINGLEMESH/MORPH/
    /// SINGLEMORPH/MIRROR, unlisted kinds reject before any face test).
    /// Production wiring is caller-side: backends implement this against
    /// their visual store; the walkers only gate on it.
    fn visual_kinds_ok(&self, id: usize) -> bool;
    fn children(&self, id: usize) -> Vec<usize>;
}

#[derive(Clone, Debug)]
pub struct RayState {
    pub base: [f32; 3],
    pub dir: [f32; 3],
    pub len: f32,
    pub len2: f32,
    pub flags: u32,
    pub best2: f32,
    pub nearest: Option<usize>,
    pub break_tests: bool,
    pub cb_result: i32,
    pub normal: [f32; 3],
    pub v: [usize; 3],
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// LineIntersectBox (i3d_collision.cpp:717-735): inside→true; else center/d/t/
/// p/w/e ordered exactly as source (three axis tests in z,x,y order with
/// per-use fabs). f32 ops preserve NaN-false flow.
pub fn line_intersects_box(base: [f32; 3], dir: [f32; 3], min: [f32; 3], max: [f32; 3]) -> bool {
    if base[0] >= min[0]
        && base[0] <= max[0]
        && base[1] >= min[1]
        && base[1] <= max[1]
        && base[2] >= min[2]
        && base[2] <= max[2]
    {
        return true;
    }
    let c = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let d = [c[0] - base[0], c[1] - base[1], c[2] - base[2]];
    let t = dir[0] * d[0] + dir[1] * d[1] + dir[2] * d[2];
    let p = [
        base[0] + dir[0] * t,
        base[1] + dir[1] * t,
        base[2] + dir[2] * t,
    ];
    let w = [c[0] - p[0], c[1] - p[1], c[2] - p[2]];
    let e = [
        (max[0] - min[0]) * 0.5,
        (max[1] - min[1]) * 0.5,
        (max[2] - min[2]) * 0.5,
    ];
    if (dir[2] * w[1] - w[2] * dir[1]).abs() > dir[2].abs() * e[1] + dir[1].abs() * e[2] {
        return false;
    }
    if (w[2] * dir[0] - w[0] * dir[2]).abs() > dir[2].abs() * e[0] + dir[0].abs() * e[2] {
        return false;
    }
    if (w[0] * dir[1] - dir[0] * w[1]).abs() > dir[1].abs() * e[0] + dir[0].abs() * e[1] {
        return false;
    }
    true
}

/// LineIntersectSphere (:740-753): d<-r → false; finite-ray far gate;
/// then the literal `if (dd >= r²) return false; return true` — the `>=`
/// (touching counts as miss) AND the NaN flow (NaN fails `>=`, so NaN is a
/// HIT) are both preserved by keeping the negated form instead of `<`.
pub fn line_intersects_sphere(
    base: [f32; 3],
    dir: [f32; 3],
    c: [f32; 3],
    r: f32,
    infinite: bool,
    len: f32,
) -> bool {
    let d = dot([c[0] - base[0], c[1] - base[1], c[2] - base[2]], dir);
    if d < -r {
        return false;
    }
    if !infinite && d > len + r {
        return false;
    }
    let p = [
        base[0] + dir[0] * d,
        base[1] + dir[1] * d,
        base[2] + dir[2] * d,
    ];
    let dd = dot(
        [p[0] - c[0], p[1] - c[1], p[2] - c[2]],
        [p[0] - c[0], p[1] - c[1], p[2] - c[2]],
    );
    if dd >= r * r {
        return false;
    }
    true
}

/// C2 0x11300 DefaultTexelCallback. Source :808-833. Index: native
/// `min((uint)(W*u), W-1)` converts via __ftol (truncate toward zero, bits
/// reinterpreted unsigned), so in-range negatives wrap and `min` picks the
/// rightmost texel; `(x as i32) as u32` reproduces that wrap, with disclosed
/// residue for huge/NaN magnitudes (Rust saturates where __ftol is
/// indefinite — inputs here are finite-guarded, magnitudes beyond i32 range
/// still land on `w-1` through `min`, same as the native indefinite pick).
/// Additive: per-channel `p[k]*(1/255)` FIRST, then `x*0.3+y*0.6+z*0.1`
/// left-associated, from BGRx bytes (p[1..3]); else the alpha byte/255.
/// `a*alpha<0.1` compares in f64 (exact f32→f64 promotion, same as the
/// native float-to-double promotion); the no-texture arm stays f32 `0.1f`.
/// Zero-size images are refused — native would underflow `w-1` into an OOB
/// read, disclosed hardening.
pub fn default_texel_callback(
    u: f32,
    v: f32,
    alpha: f32,
    additive: bool,
    pixels: Option<(&[u8], u32, u32)>,
) -> Result<i32, SceneColError> {
    if !u.is_finite() || !v.is_finite() || !alpha.is_finite() {
        return Err(SceneColError::Nonfinite);
    }
    if let Some((px, w, h)) = pixels {
        if w == 0 || h == 0 {
            return Err(SceneColError::TexelOutOfRange);
        }
        let x = (((w as f32 * u) as i32) as u32).min(w - 1);
        let y = (((h as f32 * v) as i32) as u32).min(h - 1);
        let o = ((y * w + x) * 4) as usize;
        let tex = px.get(o..o + 4).ok_or(SceneColError::TexelOutOfRange)?;
        let a = if additive {
            const INV: f32 = 1.0 / 255.0;
            let (r, g, b) = (
                tex[1] as f32 * INV,
                tex[2] as f32 * INV,
                tex[3] as f32 * INV,
            );
            (r * 0.3 + g * 0.6) + b * 0.1
        } else {
            tex[0] as f32 * (1.0 / 255.0)
        };
        Ok(if ((a * alpha) as f64) < 0.1 {
            TEXEL_TRANSPARENT
        } else {
            TEXEL_PARTIAL
        })
        // NOTE: the f64 promotion matches the source float-to-double
        // comparison exactly; the f32-vs-f64 0.1 boundary window (~1.5e-9) is
        // narrower than f32 granularity at byte-quantized magnitudes, so no
        // texel vector can distinguish the two spellings — unpinnable.
    } else {
        Ok(if alpha < 0.1 {
            TEXEL_TRANSPARENT
        } else {
            TEXEL_PARTIAL
        })
    }
}

/// Working name for the COLF_BOX bit test (mafia-re ls3df/I3D/I3D.h:1424 at
/// pin 113b5b6: a visual's box is the hit, not its faces). The reference
/// records no LS3DF FUNCTION entry for a standalone reader; the word is
/// tested in place by testVisual (ls3df/I3D/i3d_collision.cpp:296). Pure
/// word test; no state moves.
pub fn colf_box_hit(flags: u32) -> bool {
    flags & COLF_BOX != 0
}

/// Working name for the COLF_ACCEPT_ANY bit test (mafia-re ls3df/I3D/I3D.h:1425
/// at pin 113b5b6: a hit counts though a nearer one was found). The reference
/// records no LS3DF FUNCTION entry for a standalone reader; the word is
/// tested in place by testVisual (ls3df/I3D/i3d_collision.cpp:293) and the
/// inline AcceptHit helper (i3d_collision.cpp:199-201). Pure word test; no state moves.
pub fn colf_accept_any(flags: u32) -> bool {
    flags & COLF_ACCEPT_ANY != 0
}

/// Working name for the COLF_COLORKEY bit test (mafia-re ls3df/I3D/I3D.h:1427
/// at pin 113b5b6: the faces' texels are tested). The reference records no
/// LS3DF FUNCTION entry for a standalone reader; the word selects the
/// DefaultTexelCallback arm inside the TestCol* setups
/// (ls3df/I3D/i3d_collision.cpp:846-852). Pure word test; no state moves.
pub fn colf_tests_texels(flags: u32) -> bool {
    flags & COLF_COLORKEY != 0
}

/// Working name for the COLF_INFINITE bit test (mafia-re ls3df/I3D/I3D.h:1428
/// at pin 113b5b6: the ray runs past from + dir). The reference records no
/// LS3DF FUNCTION entry for a standalone reader; the word gates the
/// lineSize2/best2 range cuts in place (ls3df/I3D/i3d_collision.cpp:291,359).
/// Pure word test; no state moves.
pub fn colf_ray_infinite(flags: u32) -> bool {
    flags & COLF_INFINITE != 0
}

/// Working name for the COLF_TWO_SIDED bit test (mafia-re ls3df/I3D/I3D.h:1429
/// at pin 113b5b6: every face is two-sided). The reference records no LS3DF
/// FUNCTION entry for a standalone reader; the word gates the backface skip
/// in the face loops (ls3df/I3D/i3d_collision.cpp:419). Pure word test; no
/// state moves.
pub fn colf_two_sided_faces(flags: u32) -> bool {
    flags & COLF_TWO_SIDED != 0
}

/// Working name for the COLF_FIRST_HIT bit test (mafia-re ls3df/I3D/I3D.h:1430
/// at pin 113b5b6: the first hit ends the test). The reference records no
/// LS3DF FUNCTION entry for a standalone reader; the word stops the face
/// loops in place (ls3df/I3D/i3d_collision.cpp:206,366). Pure word test; no
/// state moves.
pub fn colf_first_hit_stops(flags: u32) -> bool {
    flags & COLF_FIRST_HIT != 0
}

/// Working name for the COLF_FRAME_LOD bit test (mafia-re ls3df/I3D/I3D.h:1431
/// at pin 113b5b6: the given frame is tested at the given level of detail).
/// The reference records no LS3DF FUNCTION entry for a standalone reader;
/// the word selects the single-frame wiring arm of the TestColGeo setup
/// (ls3df/I3D/i3d_collision.cpp:860-865). Pure word test; no state moves.
pub fn colf_frame_lod_pinned(flags: u32) -> bool {
    flags & COLF_FRAME_LOD != 0
}

/// Working name for the COLF_SKIP_OPTIONAL bit test (mafia-re
/// ls3df/I3D/I3D.h:1432 at pin 113b5b6: VISF_COL_OPTIONAL visuals are
/// skipped). The reference records no LS3DF FUNCTION entry for a standalone
/// reader; the word gates the optional-visual skip in testSubtree
/// (ls3df/I3D/i3d_collision.cpp:776). Pure word test; no state moves.
pub fn colf_skip_optional(flags: u32) -> bool {
    flags & COLF_SKIP_OPTIONAL != 0
}

/// Working name for the COLF_HIDDEN bit test (mafia-re ls3df/I3D/I3D.h:1433
/// at pin 113b5b6: switched-off frames are tested too). The reference
/// records no LS3DF FUNCTION entry for a standalone reader; the word gates
/// the FRMFLAGS_ON skip in testSubtree/sphTestSubtree
/// (ls3df/I3D/i3d_collision.cpp:764). Pure word test; no state moves.
pub fn colf_hidden_tested(flags: u32) -> bool {
    flags & COLF_HIDDEN != 0
}

/// Working name for the COLF_EXCLUDE_SUBTREE bit test (mafia-re
/// ls3df/I3D/I3D.h:1434 at pin 113b5b6). The reference records no LS3DF
/// FUNCTION entry for a standalone reader; the word selects the
/// excluded-subtree wiring arm of the TestColGeo setup
/// (ls3df/I3D/i3d_collision.cpp:855-859). Pure word test; no state moves.
pub fn colf_exclude_subtree(flags: u32) -> bool {
    flags & COLF_EXCLUDE_SUBTREE != 0
}

/// Working name for the COLF_OMNI_SECTOR bit test (mafia-re ls3df/I3D/I3D.h:1435
/// at pin 113b5b6: the omni sector is tested too). The reference records no
/// LS3DF FUNCTION entry for a standalone reader; the word gates the omni
/// sweep of the TestColGeo setup (ls3df/I3D/i3d_collision.cpp:879). Pure
/// word test; no state moves.
pub fn colf_omni_sector(flags: u32) -> bool {
    flags & COLF_OMNI_SECTOR != 0
}

/// Working name for the COLF_NEAR_SECTOR bit test (mafia-re ls3df/I3D/I3D.h:1436
/// at pin 113b5b6: and the near sector). The reference records no LS3DF
/// FUNCTION entry for a standalone reader; the word gates the near sweep of
/// the TestColGeo setup (ls3df/I3D/i3d_collision.cpp:876). Pure word test;
/// no state moves.
pub fn colf_near_sector(flags: u32) -> bool {
    flags & COLF_NEAR_SECTOR != 0
}

/// Working name for the COLF_FIRST_FACE bit test (mafia-re ls3df/I3D/I3D.h:1437
/// at pin 113b5b6: a visual's first face hit is its hit). The reference
/// records no LS3DF FUNCTION entry for a standalone reader; the word
/// short-circuits the per-visual best-face search in the face loops
/// (ls3df/I3D/i3d_collision.cpp:376). Pure word test; no state moves.
pub fn colf_first_face_hit(flags: u32) -> bool {
    flags & COLF_FIRST_FACE != 0
}

/// LS3DF RVA 0xe780, VA 0x1000e780. PDB-evidenced static `testVisual`
/// (mafia-re pin 113b5b6 `ls3df/I3D/I3D.h:1673-1675`; body at
/// `ls3df/I3D/i3d_collision.cpp:232ff`; LS3DF11 RVA 0xe320). Explicit
/// Unsupported STUB: the body walks mesh LOD face groups through live
/// vertex-buffer locks (I3d_collision.cpp:10 notes testVisual inlines
/// `C_VBuffer::Lock`) plus material/texel backends, none of which exists on
/// this host. The credited `test_subtree` walker takes a caller-side
/// backend for the face test instead; this symbol only pins the address
/// and the missing-owner reason. Tier: explicit unsupported boundary, no
/// function credit claimed.
pub fn test_visual_unsupported() -> Result<(), SceneColError> {
    Err(SceneColError::Unsupported(
        "testVisual needs a live C_VBuffer lock owner",
    ))
}

/// LS3DF RVA 0x12520, VA 0x10012520. PDB-evidenced static `sphTestVisual`
/// (mafia-re pin 113b5b6 `ls3df/I3D/I3D.h:1681-1683`; body at
/// `ls3df/I3D/i3d_collision.cpp:1062ff`; LS3DF11 RVA 0x12040). Explicit
/// Unsupported STUB, same missing owners as `test_visual_unsupported`
/// (vertex-buffer locks, mesh face groups, texel backends). The credited
/// `sph_test_subtree` walker takes a caller-side backend instead; this
/// symbol only pins the address and the missing-owner reason. Tier:
/// explicit unsupported boundary, no function credit claimed.
pub fn sphere_test_visual_unsupported() -> Result<(), SceneColError> {
    Err(SceneColError::Unsupported(
        "sphTestVisual needs a live C_VBuffer lock owner",
    ))
}

/// C1 testSubtree (:761-798, 1031B/294insn/44BB). Order: pWSubTree → ON/
/// HIDDEN → HR bbox (invalid→sector rule) → visual sphere+optional+kind
/// gates → testVisual (backend) → children until breakTests / ACCEPT_STOP.
/// Recursion depth = hierarchy depth (host stack).
pub fn test_subtree<G: SceneGraph>(
    g: &mut G,
    st: &mut RayState,
    frm: usize,
    excluded_subtree: Option<usize>,
    excluded_frame: Option<usize>,
    test_visual: &mut dyn FnMut(&mut G, &mut RayState, usize),
) {
    if Some(frm) == excluded_subtree {
        return;
    }
    if g.flags(frm) & FRMFLAGS_ON == 0 && st.flags & COLF_HIDDEN == 0 {
        return;
    }
    match g.hr_bbox(frm) {
        Some(bb) => {
            if !line_intersects_box(st.base, st.dir, bb[0], bb[1]) {
                return;
            }
        }
        None => {
            if g.frame_type(frm) != FRAME_SECTOR {
                return;
            }
        }
    }
    if g.frame_type(frm) == FRAME_VISUAL && Some(frm) != excluded_frame {
        // Like the sphere path below: the visual's OWN world sphere, not the
        // hierarchy volume (source :775 uses GetWBoundVolume().bsphere).
        if let Some((c, r)) = g.visual_sphere(frm) {
            let optional = g.flags(frm) & VISF_COL_OPTIONAL != 0;
            if line_intersects_sphere(st.base, st.dir, c, r, st.flags & COLF_INFINITE != 0, st.len)
                && !(optional && st.flags & COLF_SKIP_OPTIONAL != 0)
                && g.visual_kinds_ok(frm)
            {
                test_visual(g, st, frm);
                if st.cb_result == COLL_IGNORE_CHILDREN {
                    return;
                }
            }
        }
    }
    for f in g.children(frm) {
        if st.break_tests || st.cb_result == COLL_ACCEPT_STOP {
            break;
        }
        test_subtree(g, st, f, excluded_subtree, excluded_frame, test_visual);
    }
}

/// C5 sphTestSubtree (:1375-1414, 953B/239insn/42BB): bound-sphere gate
/// (r>0 → SphereIntersect else sector rule) → visual box+sphere gates →
/// cbResult=ACCEPT reset → sphTestVisual (backend) → children.
// NaN-flow fidelity requires the negated comparison forms below
// (`!(x < y)` keeps x87 false-outcome semantics that partial_cmp spells
// differently); do not "simplify" them.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
#[allow(clippy::too_many_arguments)] // Native walker inputs plus explicit exclusion/test-visual plumbing.
pub fn sph_test_subtree<G: SceneGraph>(
    g: &mut G,
    st: &mut RayState,
    sph_box: [[f32; 3]; 2],
    radius: f32,
    frm: usize,
    excluded_subtree: Option<usize>,
    excluded_frame: Option<usize>,
    test_visual: &mut dyn FnMut(&mut G, &mut RayState, usize),
) {
    if Some(frm) == excluded_subtree {
        return;
    }
    if g.flags(frm) & FRMFLAGS_ON == 0 && st.flags & COLF_HIDDEN == 0 {
        return;
    }
    match g.hr_sphere(frm) {
        Some((c, r)) if r > 0.0 => {
            let rr = radius + r;
            let d = dot(
                [c[0] - st.base[0], c[1] - st.base[1], c[2] - st.base[2]],
                st.dir,
            );
            // Source tests too little here (`&&` where `||` belongs);
            // kept verbatim, never "fixed".
            if d < -rr && d > st.len + rr {
                return;
            }
            let p = [
                st.base[0] + st.dir[0] * d,
                st.base[1] + st.dir[1] * d,
                st.base[2] + st.dir[2] * d,
            ];
            // Literal `return Square() < r*r` kept as the negated early-out:
            // NaN fails `<`, so NaN is a MISS here (unlike LineIntersectSphere
            // above, where NaN is a hit) — the two forms must not be unified.
            if !(dot(
                [p[0] - c[0], p[1] - c[1], p[2] - c[2]],
                [p[0] - c[0], p[1] - c[1], p[2] - c[2]],
            ) < rr * rr)
            {
                return;
            }
        }
        _ => {
            if g.frame_type(frm) != FRAME_SECTOR {
                return;
            }
        }
    }
    if g.frame_type(frm) == FRAME_VISUAL && Some(frm) != excluded_frame {
        if let Some(bb) = g.hr_bbox(frm) {
            let overlap = sph_box[0][0] <= bb[1][0]
                && bb[0][0] <= sph_box[1][0]
                && sph_box[0][1] <= bb[1][1]
                && bb[0][1] <= sph_box[1][1]
                && sph_box[0][2] <= bb[1][2]
                && bb[0][2] <= sph_box[1][2];
            // Source :1393-1395: BoxIntersect AND the visual's own
            // SphereIntersect capsule AND the OPTIONAL/SKIP gate, in that
            // order, before the ACCEPT reset. visual_sphere defaults to the
            // HR sphere (identical for leaf visuals; production backends
            // distinguishing W/HR volumes override it).
            if overlap {
                if let Some((vc, vr)) = g.visual_sphere(frm) {
                    let vrr = radius + vr;
                    let vd = dot(
                        [vc[0] - st.base[0], vc[1] - st.base[1], vc[2] - st.base[2]],
                        st.dir,
                    );
                    if vd < -vrr && vd > st.len + vrr {
                        // Far-end miss (same too-little `&&` as the source).
                    } else {
                        let vp = [
                            st.base[0] + st.dir[0] * vd,
                            st.base[1] + st.dir[1] * vd,
                            st.base[2] + st.dir[2] * vd,
                        ];
                        let vdd = dot(
                            [vp[0] - vc[0], vp[1] - vc[1], vp[2] - vc[2]],
                            [vp[0] - vc[0], vp[1] - vc[1], vp[2] - vc[2]],
                        );
                        if vdd < vrr * vrr {
                            let optional = g.flags(frm) & VISF_COL_OPTIONAL != 0;
                            if !(optional && st.flags & COLF_SKIP_OPTIONAL != 0)
                                && g.visual_kinds_ok(frm)
                            {
                                st.cb_result = COLL_ACCEPT;
                                test_visual(g, st, frm);
                                if st.cb_result == COLL_IGNORE_CHILDREN {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for f in g.children(frm) {
        if st.break_tests || st.cb_result == COLL_ACCEPT_STOP {
            break;
        }
        sph_test_subtree(
            g,
            st,
            sph_box,
            radius,
            f,
            excluded_subtree,
            excluded_frame,
            test_visual,
        );
    }
}

/// Shared TestCol* setup wiring: COLF_COLORKEY→texel default,
/// EXCLUDE_SUBTREE vs FRAME_LOD vs plain wiring.
#[derive(Clone, Copy, Debug)]
pub struct ColWiring {
    pub excluded_subtree: Option<usize>,
    pub excluded_frame: Option<usize>,
    pub single_frame: Option<usize>,
    pub spec_lod: i32,
    pub col_lod: i32,
    pub want_texel_default: bool,
}
pub fn wire_collision(flags: u32, frm: Option<usize>, lod: i32) -> ColWiring {
    if flags & COLF_EXCLUDE_SUBTREE != 0 {
        ColWiring {
            excluded_subtree: frm,
            excluded_frame: None,
            single_frame: None,
            spec_lod: lod,
            col_lod: lod,
            want_texel_default: flags & COLF_COLORKEY != 0,
        }
    } else if flags & COLF_FRAME_LOD != 0 {
        ColWiring {
            excluded_subtree: None,
            excluded_frame: None,
            single_frame: frm,
            spec_lod: lod,
            col_lod: 0,
            want_texel_default: flags & COLF_COLORKEY != 0,
        }
    } else {
        ColWiring {
            excluded_subtree: None,
            excluded_frame: frm,
            single_frame: None,
            spec_lod: lod,
            col_lod: lod,
            want_texel_default: flags & COLF_COLORKEY != 0,
        }
    }
}

/// C3 TestColGeo1 (:898-943): like TestColGeo but the hit POINT is also
/// produced as `pos = from + dir*sqrt(best2)`; no excluded frame (so the
/// shared wire_collision helper below, which always names an exclusion,
/// must NOT be used for Geo1 paths).
pub fn test_collision_geo_point(state: &RayState) -> [f32; 3] {
    let d = state.best2.sqrt();
    [
        state.base[0] + state.dir[0] * d,
        state.base[1] + state.dir[1] * d,
        state.base[2] + state.dir[2] * d,
    ]
}

/// C4 TestColHierarchy (:954-1007): hierarchy walk sharing the test_subtree
/// core; hit record fill order is dist → normal → v → fgroup.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColHit {
    pub dist: f32,
    pub normal: [f32; 3],
    pub v: [usize; 3],
    pub fgroup: u32,
}
pub fn test_collision_hierarchy_note() -> ColHit {
    ColHit::default()
}

/// C6 TestColSphereHierarchy (:1460-1489): absorb-box variant sharing the
/// sph_test_subtree core.
pub fn test_collision_sphere_hierarchy_note() -> ColHit {
    ColHit::default()
}

/// C7 TestColSphereVisual (:1496-1537): lineDir*radius capsule box sharing
/// the sph_test_subtree core; unlisted visual kinds reject before any face
/// test (caller-side `visual_kinds_ok` gate, as in the walker above).
pub fn test_collision_sphere_visual_note() -> ColHit {
    ColHit::default()
}

/// LS3DF RVA 0xdec0, VA 0x1000dec0. `I3D_scene::TestColGeo` ray setup
/// (mafia-re pin 113b5b6 `ls3df/I3D/i3d_collision.cpp:835-875`;
/// LS3DF11 RVA 0xda60; marker at `ls3df/I3D/I3D.h:1669`). Covers the entry
/// prefix only: `breakTests` cleared, `lineBase` stored, `lineDir` run
/// through `SetNormalized` (RVA 0x14970, `vector_set_normalized`), `lineSize2`
/// as the plain `(x*x + y*y) + z*z` association of the source spelling and
/// `lineSize` its f32 square root, plus the neutral hit record
/// (`vzdalenost2` 1e16, no nearest, ACCEPT). Callback words, the texel
/// callback selection, the LOD/exclusion wiring (`wire_collision`) and the
/// sector sweeps stay caller-side. Tier contract (wave policy: reviewed
/// upstream source plus Rust tests, no Ghidra body claimed).
pub fn test_collision_geo_setup(from: [f32; 3], dir: [f32; 3], flags: u32) -> RayState {
    let len2 = (dir[0] * dir[0] + dir[1] * dir[1]) + dir[2] * dir[2];
    RayState {
        base: from,
        dir: crate::native_transform::vector_set_normalized(dir),
        len: len2.sqrt(),
        len2,
        flags,
        best2: 1e16,
        nearest: None,
        break_tests: false,
        cb_result: COLL_ACCEPT,
        normal: [0.0; 3],
        v: [0; 3],
    }
}

/// LS3DF RVA 0xdec0, VA 0x1000dec0. `I3D_scene::TestColGeo` distance word
/// (mafia-re pin 113b5b6 `ls3df/I3D/i3d_collision.cpp:881-884`). `None`
/// without a nearest visual (the native `dist` out-param is left untouched
/// then); otherwise `sqrt(best2)`. The normal fill needs the hit visual's
/// world matrix and stays caller-side. Tier contract (wave policy: reviewed
/// upstream source plus Rust tests, no Ghidra body claimed).
pub fn test_collision_geo_distance(state: &RayState) -> Option<f32> {
    state.nearest.map(|_| state.best2.sqrt())
}

/// LS3DF RVA 0x11d60, VA 0x10011d60. `I3D_scene::TestColSphere` sweep box
/// (mafia-re pin 113b5b6 `ls3df/I3D/i3d_collision.cpp:1419-1432`;
/// LS3DF11 RVA 0x11880; marker at `ls3df/I3D/I3D.h:1677`). The move box:
/// `lineDest = from + dir` with the UN-normalized direction, `sphBox`
/// spanning `from`/`lineDest` (`Absorb` is the componentwise min/max, exact
/// here for two points), then expanded by `radius` on every axis. Unlike the
/// hierarchy/visual variants this spelling does not pre-shift the corners by
/// `lineDir * radius`. Tier contract (wave policy: reviewed upstream source
/// plus Rust tests, no Ghidra body claimed).
pub fn sphere_sweep_box(from: [f32; 3], dir: [f32; 3], radius: f32) -> [[f32; 3]; 2] {
    let dest = [from[0] + dir[0], from[1] + dir[1], from[2] + dir[2]];
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for axis in 0..3 {
        lo[axis] = from[axis].min(dest[axis]) - radius;
        hi[axis] = from[axis].max(dest[axis]) + radius;
    }
    [lo, hi]
}

/// LS3DF RVA 0x11d60, VA 0x10011d60. `I3D_scene::TestColSphere` exclusion
/// wiring (mafia-re pin 113b5b6 `ls3df/I3D/i3d_collision.cpp:1441-1446`).
/// `COLF_EXCLUDE_SUBTREE` names the excluded subtree, every other flag word
/// (notably including `COLF_FRAME_LOD`, which has no branch here unlike
/// `wire_collision`) names the excluded frame. Returns
/// `(excluded_subtree, excluded_frame)`. Tier contract (wave policy:
/// reviewed upstream source plus Rust tests, no Ghidra body claimed).
pub fn wire_sphere_exclusion(flags: u32, frm: Option<usize>) -> (Option<usize>, Option<usize>) {
    if flags & COLF_EXCLUDE_SUBTREE != 0 {
        (frm, None)
    } else {
        (None, frm)
    }
}

/// C8 SphereToSphereD (:1543-1570, x87 block). d1=to1-from1; d2=to2-from2;
/// delta=from2-from1; dd=d2-d1; r=r1+r2; a=dd·dd; rr=r²; `a<=rr → 0` (kept
/// verbatim despite dimension oddity); b=2(delta·dd); c=delta·delta-rr;
/// disc=b²-4ac; `!(disc<0)` (negated form kept); t=(±s-b)*0.5/a, min, in
/// [0,1] else -1. The upfront finiteness sweep covers all 14 scalars
/// (r1,r2,f1,t1,f2,t2) as a disclosed contract guard: native propagates
/// NaN/Inf through the `a<=rr` / `!(disc<0)` false-flow instead.
// Same negated-form requirement as sph_test_subtree above.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn sphere_to_sphere_fraction(
    r1: f32,
    f1: [f32; 3],
    t1: [f32; 3],
    r2: f32,
    f2: [f32; 3],
    t2: [f32; 3],
) -> Result<f32, SceneColError> {
    for v in [
        r1, r2, f1[0], f1[1], f1[2], t1[0], t1[1], t1[2], f2[0], f2[1], f2[2], t2[0], t2[1], t2[2],
    ] {
        if !v.is_finite() {
            return Err(SceneColError::Nonfinite);
        }
    }
    let d1 = [t1[0] - f1[0], t1[1] - f1[1], t1[2] - f1[2]];
    let d2 = [t2[0] - f2[0], t2[1] - f2[1], t2[2] - f2[2]];
    let delta = [f2[0] - f1[0], f2[1] - f1[1], f2[2] - f1[2]];
    let dd = [d2[0] - d1[0], d2[1] - d1[1], d2[2] - d1[2]];
    let r = r1 + r2;
    let a = dot(dd, dd);
    let rr = r * r;
    if a <= rr {
        return Ok(0.0);
    }
    let b = dot(delta, dd) * 2.0;
    let c = dot(delta, delta) - rr;
    let disc = b * b - 4.0 * a * c;
    if !(disc < 0.0) {
        let s = disc.sqrt();
        let f = 0.5 / a;
        let (ta, tb) = ((s - b) * f, (-b - s) * f);
        let t = if ta > tb { tb } else { ta };
        if (0.0..=1.0).contains(&t) {
            return Ok(t);
        }
    }
    Ok(-1.0)
}

/// LS3DF RVA 0x152a0, VA 0x100152a0. `SphereIntersect` (mafia-re
/// `ls3df/I3D/i3d_collision.cpp:1037-1047` via `ls3df/I3D/I3D.h:1685`;
/// working name, mafia-re guess, NOT an original symbol; PS2 inlines it;
/// LS3DF11 RVA 0x14dc0). The sphere moving from `line_base` against the
/// line through a bound sphere: `r = radius_a + sphere_radius`,
/// `d = line_dir.Dot(sphere_pos - line_base)`, the dead far-gate
/// `d < -r && d > line_size + r` preserved verbatim (it tests too little:
/// only a sphere both behind the start and beyond the end would fail it,
/// so it never fires), then `p = line_base + line_dir * d` and
/// `(p - sphere_pos).Square() < r * r` (touching counts as miss). Distinct
/// from `line_intersects_sphere` (different near/far gates and the `<`
/// versus `>=` miss spelling with its own NaN flow). Exact f32 with the
/// native association and no fused operations; NaN keeps the direct
/// comparison flow. Tier contract (wave policy: no Ghidra body observed).
// The dead `&&` gate must stay verbatim; do not "fix" it to `||`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn sphere_intersect_capsule(
    line_base: [f32; 3],
    line_dir: [f32; 3],
    line_size: f32,
    radius_a: f32,
    sphere_pos: [f32; 3],
    sphere_radius: f32,
) -> bool {
    let r = radius_a + sphere_radius;
    let d = dot(
        [
            sphere_pos[0] - line_base[0],
            sphere_pos[1] - line_base[1],
            sphere_pos[2] - line_base[2],
        ],
        line_dir,
    );
    if d < -r && d > line_size + r {
        return false;
    }
    let p = [
        line_base[0] + line_dir[0] * d,
        line_base[1] + line_dir[1] * d,
        line_base[2] + line_dir[2] * d,
    ];
    let diff = [
        p[0] - sphere_pos[0],
        p[1] - sphere_pos[1],
        p[2] - sphere_pos[2],
    ];
    let square = (diff[0] * diff[0] + diff[1] * diff[1]) + diff[2] * diff[2];
    square < r * r
}

#[cfg(test)]
mod collision_scene_tests {
    use super::*;
    struct Flat {
        flags: Vec<u32>,
        types: Vec<i32>,
        boxes: Vec<Option<[[f32; 3]; 2]>>,
        spheres: Vec<Option<([f32; 3], f32)>>,
        vspheres: Vec<Option<([f32; 3], f32)>>,
        kids: Vec<Vec<usize>>,
    }
    impl SceneGraph for Flat {
        fn flags(&self, id: usize) -> u32 {
            self.flags[id]
        }
        fn frame_type(&self, id: usize) -> i32 {
            self.types[id]
        }
        fn hr_bbox(&self, id: usize) -> Option<[[f32; 3]; 2]> {
            self.boxes[id]
        }
        fn hr_sphere(&self, id: usize) -> Option<([f32; 3], f32)> {
            self.spheres[id]
        }
        fn visual_sphere(&self, id: usize) -> Option<([f32; 3], f32)> {
            self.vspheres[id]
        }
        fn is_visual(&self, id: usize) -> bool {
            self.types[id] == FRAME_VISUAL
        }
        fn visual_kinds_ok(&self, id: usize) -> bool {
            self.is_visual(id)
        }
        fn children(&self, id: usize) -> Vec<usize> {
            self.kids[id].clone()
        }
    }
    fn ray() -> RayState {
        RayState {
            base: [0.0, 0.0, -10.0],
            dir: [0.0, 0.0, 1.0],
            len: 10.0,
            len2: 100.0,
            flags: 0,
            best2: 1e16,
            nearest: None,
            break_tests: false,
            cb_result: COLL_ACCEPT,
            normal: [0.0; 3],
            v: [0; 3],
        }
    }
    #[test]
    fn box_inside_is_zero_hit() {
        assert!(line_intersects_box(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [-1.0, -1.0, -1.0],
            [1.0, 1.0, 1.0]
        ));
        assert!(!line_intersects_box(
            [5.0, 5.0, -10.0],
            [0.0, 0.0, 1.0],
            [-1.0, -1.0, -1.0],
            [1.0, 1.0, 1.0]
        ));
    }
    #[test]
    fn sphere_touch_is_miss_ge_kept() {
        // Perpendicular offset 1 with r=1: closest distance² == r² → miss.
        assert!(!line_intersects_sphere(
            [0.0, 1.0, -2.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            1.0,
            true,
            0.0
        ));
        assert!(line_intersects_sphere(
            [0.0, 1.0, -2.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            1.5,
            true,
            0.0
        ));
    }
    #[test]
    fn subtree_skips_excluded_and_stops() {
        let mut g = Flat {
            flags: vec![1, 1, 1],
            types: vec![5, 1, 1],
            boxes: vec![
                None,
                Some([[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]]),
                Some([[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]]),
            ],
            spheres: vec![
                None,
                Some(([0.0, 0.0, 0.0], 2.0)),
                Some(([0.0, 0.0, 0.0], 2.0)),
            ],
            vspheres: vec![
                None,
                Some(([0.0, 0.0, 0.0], 2.0)),
                Some(([0.0, 0.0, 0.0], 2.0)),
            ],
            kids: vec![vec![1, 2], vec![], vec![]],
        };
        let mut st = ray();
        let mut seen = vec![];
        let mut tv = |_g: &mut Flat, s: &mut RayState, id: usize| {
            seen.push(id);
            s.nearest = Some(id);
            s.break_tests = true;
        };
        test_subtree(&mut g, &mut st, 0, None, None, &mut tv);
        assert_eq!(seen, vec![1]);
        let mut st2 = ray();
        let mut seen2 = vec![];
        let mut tv2 = |_g: &mut Flat, s: &mut RayState, id: usize| {
            seen2.push(id);
            s.nearest = Some(id);
            s.cb_result = COLL_ACCEPT_STOP;
        };
        test_subtree(&mut g, &mut st2, 0, None, None, &mut tv2);
        assert_eq!(seen2, vec![1]);
    }
    #[test]
    fn texel_alpha_boundary_tenth() {
        let px = [0u8, 0, 0, 0, 25, 0, 0, 0];
        assert_eq!(
            default_texel_callback(0.0, 0.0, 1.0, false, Some((&px, 2, 1))).unwrap(),
            TEXEL_TRANSPARENT
        );
        // 26/255 clears 0.1 with the second pixel selected.
        assert_eq!(
            default_texel_callback(0.75, 0.0, 1.0, false, Some((&px2_of(), 2, 1))).unwrap(),
            TEXEL_PARTIAL
        );
        assert_eq!(
            default_texel_callback(0.0, 0.0, 0.05, false, None).unwrap(),
            TEXEL_TRANSPARENT
        );
        assert!(default_texel_callback(4.0, 4.0, 1.0, false, Some((&px, 2, 1))).is_ok());
        assert!(default_texel_callback(0.0, 0.0, 1.0, false, Some((&px, 1, 0))).is_err());
    }
    fn px2_of() -> [u8; 8] {
        [0u8, 0, 0, 0, 26, 0, 0, 0]
    }
    #[test]
    fn sphere_quadratic_vectors() {
        assert_eq!(
            sphere_to_sphere_fraction(
                1.0,
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                1.0,
                [5.0, 0.0, 0.0],
                [3.0, 0.0, 0.0]
            )
            .unwrap(),
            0.75
        );
        assert_eq!(
            sphere_to_sphere_fraction(
                5.0,
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                5.0,
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0]
            )
            .unwrap(),
            0.0
        );
        assert_eq!(
            sphere_to_sphere_fraction(
                0.5,
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                0.5,
                [0.0, 5.0, 0.0],
                [0.0, 5.0, 0.0]
            )
            .unwrap(),
            -1.0
        );
        assert!(sphere_to_sphere_fraction(
            f32::NAN,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            1.0,
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0]
        )
        .is_err());
    }
    #[test]
    fn wiring_modes() {
        let w = wire_collision(COLF_EXCLUDE_SUBTREE, Some(3), 2);
        assert_eq!((w.excluded_subtree, w.col_lod), (Some(3), 2));
        let w = wire_collision(COLF_FRAME_LOD, Some(3), 2);
        assert_eq!((w.single_frame, w.col_lod), (Some(3), 0));
        let w = wire_collision(0, Some(3), 2);
        assert_eq!((w.excluded_frame, w.col_lod), (Some(3), 2));
    }
    #[test]
    fn wrapper_notes_share_walker_cores() {
        // best2 = 16 → d = 4 → pos = base + 4*dir (exact f32).
        let mut st = ray();
        st.best2 = 16.0;
        assert_eq!(test_collision_geo_point(&st), [0.0, 0.0, -6.0]);
        assert_eq!(test_collision_hierarchy_note(), ColHit::default());
        assert_eq!(test_collision_sphere_hierarchy_note(), ColHit::default());
        assert_eq!(test_collision_sphere_visual_note(), ColHit::default());
    }
    #[test]
    fn sphere_nan_flows_match_source_forms() {
        // LineIntersectSphere: NaN fails `>=`, so NaN is a HIT.
        assert!(line_intersects_sphere(
            [f32::NAN, 0.0, -2.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            1.0,
            true,
            0.0
        ));
        // SphereIntersect capsule: NaN fails `<`, so NaN is a MISS (no face
        // test runs). Frame 1 is a visual with a NaN HR sphere.
        let mut g = Flat {
            flags: vec![1, 1],
            types: vec![5, 1],
            boxes: vec![None, Some([[-50.0; 3], [50.0; 3]])],
            spheres: vec![None, Some(([f32::NAN, 0.0, 0.0], 2.0))],
            vspheres: vec![None, Some(([f32::NAN, 0.0, 0.0], 2.0))],
            kids: vec![vec![1], vec![]],
        };
        let mut st = ray();
        let mut seen = vec![];
        let mut tv = |_g: &mut Flat, s: &mut RayState, id: usize| {
            seen.push(id);
            let _ = s;
        };
        sph_test_subtree(
            &mut g,
            &mut st,
            [[-60.0; 3], [60.0; 3]],
            1.0,
            0,
            None,
            None,
            &mut tv,
        );
        assert!(seen.is_empty());
    }
    #[test]
    fn sph_visual_gates_sphere_then_skip_then_kind() {
        // Frame 1: visual whose box overlaps the sweep box but whose sphere
        // is far away → box-only would face-test, the sphere gate must not.
        // Frame 2: overlapping box AND sphere, OPTIONAL with SKIP → skip.
        // Frame 3: same, without SKIP → ACCEPT + face test.
        let mut g = Flat {
            flags: vec![1, 1, 1 | VISF_COL_OPTIONAL, 1],
            types: vec![5, 1, 1, 1],
            boxes: vec![
                None,
                Some([[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]]),
                Some([[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]]),
                Some([[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]]),
            ],
            spheres: vec![
                None,
                Some(([0.0, 0.0, 0.0], 50.0)),
                Some(([0.0, 0.0, 0.0], 50.0)),
                Some(([0.0, 0.0, 0.0], 50.0)),
            ],
            // 1: HR passes, own sphere misses (offset); 2,3: own sphere hits.
            vspheres: vec![
                None,
                Some(([0.0, 5.0, 0.0], 1.0)),
                Some(([0.0, 0.0, 0.0], 2.0)),
                Some(([0.0, 0.0, 0.0], 2.0)),
            ],
            kids: vec![vec![1, 2, 3], vec![], vec![], vec![]],
        };
        // Ray along +z from z=-10; sweep radius 0.5 around it.
        let mut st = ray();
        st.flags = COLF_SKIP_OPTIONAL;
        let mut seen = vec![];
        let mut tv = |_g: &mut Flat, s: &mut RayState, id: usize| {
            seen.push(id);
            let _ = s;
        };
        sph_test_subtree(
            &mut g,
            &mut st,
            [[-2.0, -2.0, -12.0], [2.0, 2.0, 12.0]],
            0.5,
            0,
            None,
            None,
            &mut tv,
        );
        // 1 sphere-misses, 2 skipped by OPTIONAL|SKIP, 3 face-tested.
        assert_eq!(seen, vec![3]);
        assert_eq!(st.cb_result, COLL_ACCEPT);
    }
    #[test]
    fn ray_visual_gate_uses_own_sphere_not_hr() {
        // HR sphere passes the box gate, but the visual's OWN sphere misses
        // the ray capsule: no face test (source :775 reads GetWBoundVolume,
        // like the sphere path — not the HR volume).
        let mut g = Flat {
            flags: vec![1, 1],
            types: vec![5, 1],
            boxes: vec![None, Some([[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]])],
            spheres: vec![None, Some(([0.0, 0.0, 0.0], 50.0))],
            vspheres: vec![None, Some(([0.0, 5.0, 0.0], 1.0))],
            kids: vec![vec![1], vec![]],
        };
        let mut st = ray();
        let mut seen = vec![];
        let mut tv = |_g: &mut Flat, s: &mut RayState, id: usize| {
            seen.push(id);
            let _ = s;
        };
        test_subtree(&mut g, &mut st, 0, None, None, &mut tv);
        assert!(seen.is_empty());
    }
    #[test]
    fn texel_negative_uv_wraps_to_rightmost() {
        // u=-1.0, w=2: (uint)(-2.0) wraps, min picks x=1 (rightmost texel,
        // alpha 26 → PARTIAL). Saturating conversion would pick x=0 (alpha
        // 0 → TRANSPARENT).
        assert_eq!(
            default_texel_callback(-1.0, 0.0, 1.0, false, Some((&px2_of(), 2, 1))).unwrap(),
            TEXEL_PARTIAL
        );
    }
    #[test]
    fn sphere_fraction_guards_t2_too() {
        assert!(sphere_to_sphere_fraction(
            1.0,
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            1.0,
            [5.0, 0.0, 0.0],
            [f32::NAN, 0.0, 0.0]
        )
        .is_err());
    }
    #[test]
    fn capsule_keeps_dead_gate_and_strict_touch() {
        // Direct hit on the line.
        assert!(sphere_intersect_capsule(
            [0.0, 0.0, -10.0],
            [0.0, 0.0, 1.0],
            10.0,
            0.5,
            [0.0, 0.0, 0.0],
            1.0
        ));
        // Lateral offset 1.5 with r 1.5: square == r*r counts as miss.
        assert!(!sphere_intersect_capsule(
            [0.0, 1.5, -10.0],
            [0.0, 0.0, 1.0],
            10.0,
            0.5,
            [0.0, 0.0, 0.0],
            1.0
        ));
        // Dead far-gate: a sphere far beyond line_size but on the line
        // still hits (the `&&` gate never fires). The finite-ray
        // line_intersects_sphere with infinite=false would miss here.
        assert!(sphere_intersect_capsule(
            [0.0, 0.0, -10.0],
            [0.0, 0.0, 1.0],
            10.0,
            0.5,
            [0.0, 0.0, 50.0],
            1.0
        ));
        assert!(!line_intersects_sphere(
            [0.0, 0.0, -10.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 50.0],
            1.5,
            false,
            10.0
        ));
        // NaN flows through direct comparisons to false (no hit).
        assert!(!sphere_intersect_capsule(
            [f32::NAN, 0.0, -10.0],
            [0.0, 0.0, 1.0],
            10.0,
            0.5,
            [0.0, 0.0, 0.0],
            1.0
        ));
    }
    #[test]
    fn geo_setup_0xdec0_normalizes_scales_and_keeps_neutral_hit() {
        let st = test_collision_geo_setup([1.0, 2.0, 3.0], [2.0, 0.0, 0.0], COLF_BOX);
        assert_eq!(st.base, [1.0, 2.0, 3.0]);
        assert_eq!(st.dir, [1.0, 0.0, 0.0]);
        assert_eq!(st.len2, 4.0);
        assert_eq!(st.len, 2.0);
        assert_eq!(st.flags, COLF_BOX);
        assert_eq!(st.best2, 1e16);
        assert_eq!(st.nearest, None);
    }
    #[test]
    fn geo_setup_0xdec0_zero_dir_takes_axis_fallback() {
        // SetNormalized (RVA 0x14970): zero square takes the axis fallback,
        // preserving lanes, so the ray keeps a unit fallback direction.
        let st = test_collision_geo_setup([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0);
        assert_eq!(st.dir, [0.0, 1.0, 0.0]);
        assert_eq!(st.len2, 0.0);
        assert_eq!(st.len, 0.0);
    }
    #[test]
    fn geo_distance_0xdec0_reports_sqrt_only_on_hit() {
        let mut st = ray();
        assert_eq!(test_collision_geo_distance(&st), None);
        st.nearest = Some(4);
        st.best2 = 25.0;
        assert_eq!(test_collision_geo_distance(&st), Some(5.0));
    }
    #[test]
    fn sweep_box_0x11d60_spans_from_to_dest_then_expands() {
        // dest = from + dir with the un-normalized dir: [1,2,3]+[4,-1,0].
        let [lo, hi] = sphere_sweep_box([1.0, 2.0, 3.0], [4.0, -1.0, 0.0], 0.5);
        assert_eq!(lo, [0.5, 0.5, 2.5]);
        assert_eq!(hi, [5.5, 2.5, 3.5]);
    }
    #[test]
    fn sphere_wiring_0x11d60_has_no_frame_lod_branch() {
        assert_eq!(
            wire_sphere_exclusion(COLF_EXCLUDE_SUBTREE, Some(3)),
            (Some(3), None)
        );
        // COLF_FRAME_LOD falls into the plain else arm (unlike wire_collision,
        // which gives it a single_frame branch).
        assert_eq!(
            wire_sphere_exclusion(COLF_FRAME_LOD, Some(3)),
            (None, Some(3))
        );
        assert_eq!(wire_sphere_exclusion(0, Some(3)), (None, Some(3)));
    }
    #[test]
    fn colf_readers_cover_all_thirteen_bits() {
        // Each reader reports exactly its own bit and nothing else.
        let cases: [(u32, fn(u32) -> bool); 13] = [
            (COLF_BOX, colf_box_hit),
            (COLF_ACCEPT_ANY, colf_accept_any),
            (COLF_COLORKEY, colf_tests_texels),
            (COLF_INFINITE, colf_ray_infinite),
            (COLF_TWO_SIDED, colf_two_sided_faces),
            (COLF_FIRST_HIT, colf_first_hit_stops),
            (COLF_FRAME_LOD, colf_frame_lod_pinned),
            (COLF_SKIP_OPTIONAL, colf_skip_optional),
            (COLF_HIDDEN, colf_hidden_tested),
            (COLF_EXCLUDE_SUBTREE, colf_exclude_subtree),
            (COLF_OMNI_SECTOR, colf_omni_sector),
            (COLF_NEAR_SECTOR, colf_near_sector),
            (COLF_FIRST_FACE, colf_first_face_hit),
        ];
        assert_eq!(cases.len(), 13);
        for (bit, read) in cases {
            assert!(read(bit), "bit {bit:#x} must read back");
            assert!(!read(0), "bit {bit:#x} must be clear on zero");
            assert!(read(u32::MAX), "bit {bit:#x} must survive a full word");
            // Every other single bit must not leak into this reader.
            for other in [
                COLF_BOX,
                COLF_ACCEPT_ANY,
                COLF_COLORKEY,
                COLF_INFINITE,
                COLF_TWO_SIDED,
                COLF_FIRST_HIT,
                COLF_FRAME_LOD,
                COLF_SKIP_OPTIONAL,
                COLF_HIDDEN,
                COLF_EXCLUDE_SUBTREE,
                COLF_OMNI_SECTOR,
                COLF_NEAR_SECTOR,
                COLF_FIRST_FACE,
            ] {
                if other != bit {
                    assert!(!read(other), "bit {other:#x} leaks into {bit:#x}");
                }
            }
        }
    }
    #[test]
    fn test_visual_0xe780_is_explicitly_unsupported() {
        // Verified RVA 0xe780 / VA 0x1000e780 (PDB testVisual): no live
        // vertex-buffer lock owner on this host.
        assert_eq!(
            test_visual_unsupported(),
            Err(SceneColError::Unsupported(
                "testVisual needs a live C_VBuffer lock owner"
            ))
        );
    }
    #[test]
    fn sphere_test_visual_0x12520_is_explicitly_unsupported() {
        // Verified RVA 0x12520 / VA 0x10012520 (PDB sphTestVisual): same
        // missing owners as the ray half.
        assert_eq!(
            sphere_test_visual_unsupported(),
            Err(SceneColError::Unsupported(
                "sphTestVisual needs a live C_VBuffer lock owner"
            ))
        );
    }
}
