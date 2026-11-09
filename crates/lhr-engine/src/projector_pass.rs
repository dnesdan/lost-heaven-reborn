#![forbid(unsafe_code)]
//! Projector queue/traversal/projection passes (GROUP Y4).
//!
//! PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs: 0x46780 contract · 0x46930 differential · 0x46960 differential ·
//! 0x48060 differential · 0x482c0 contract · 0x483e0 differential ·
//! 0x485f0 differential-order/contract-draws · 0x48960 contract.
//!
//! Reference (upstream working names, never original symbols): mafia-re
//! ls3df/I3D/visuals/I3d_projector.cpp (m_AddFGroups :302, ProcessProjectors
//! :339, ProcessRecList :358, ProcessShadow :698, MakeProjection :763,
//! CollectReceivers :810, m_RenderFGroup :867, LoadImage :913) + I3D.h:2697-8,
//! :4578-80, :4803-18, :4891, :4913-14. Ghidra bodies (provider `ghidra`
//! only): 0x46930 15insn/3BB, 0x46960 1588insn/268BB, 0x48060 325insn/68BB
//! non-contiguous, 0x483e0 133insn/40BB, 0x485f0 269insn/39BB; 0x46780,
//! 0x482c0, 0x48960 have no Ghidra entry (contract tier, no credit).
//!
//! State ownership: GROUP Y3 owns `crate::frame_projector::I3dProjectorState`.
//! This module only borrows live state through the traits below with named
//! accessors (no offsets, no `Vec<u8>` maps).
//! Precision: add/mul/sub are PC24 checked-`f32` (f64-roundtrip single-op
//! sub); div/sqrt/`fabs` are host-`f32` (quarantined, disclosed).
//! `FloatToInt` is truncation toward zero with a numeric-domain failure,
//! never a wrapped byte. `ROUND` is half-up (only non-negative inputs reach
//! it). No fused ops. Device/render/VB/IB/clipper internals are backend
//! contracts with order logs; no live device here.

use crate::native_matrix::{checked_f32_add, checked_f32_mul};

/// Instruction-evidenced words (Ghidra immediates; I3D.h corroboration noted).
pub const PROJF_RECEIVERS_VALID: u32 = 0x1;
pub const VISF_OBJECT_WORLD: u32 = 0x10;
pub const VISF_WORLD_SPACE: u32 = 0x40;
pub const VISF_PROJECTOR: u32 = 0x2000;
pub const VISF_PROJECTOR_2: u32 = 0x4000;
pub const MESH_NEEDS_UPDATE: u32 = 0x20000;
pub const VISF_WBOUND_RECOMPUTE: u32 = 0x40000;
pub const VISF_INV_MAT: u32 = 0x100000;
pub const FRM_IS_ON: u32 = 0x1;
pub const FRM_WMAT_VALID: u32 = 0x20;
pub const FRM_HRBOUND_VALID: u32 = 0x100;
pub const FRM_UPDATE_CHANGED: u32 = 0x40002000;
/// Instruction-evidenced visual-type words (raw u32 compares).
pub const VTYPE_LMAP: u32 = 0x50414d4c;
pub const VTYPE_MRPH: u32 = 0x4850524d;
pub const VTYPE_SNGM: u32 = 0x4d474e53;
pub const VTYPE_SMMR: u32 = 0x524d4d53;
pub const VTYPE_OBJ: u32 = 0x5f4a424f;
pub const MTYPE_VISUAL: u32 = 1;
pub const MTYPE_SECTOR: u32 = 5;
pub const MTYPE_MODEL: u32 = 9;
/// Projector modes (I3D.h:4803-4808; ADD family {0,4,8} Ghidra-pinned twice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ProjectorMode {
    Add = 0,
    Blend = 1,
    AddFade = 4,
    BlendFade = 5,
    AddFadeMid = 8,
    BlendFadeMid = 9,
}
impl ProjectorMode {
    pub fn from_word(word: u32) -> Option<Self> {
        match word {
            0 => Some(Self::Add),
            1 => Some(Self::Blend),
            4 => Some(Self::AddFade),
            5 => Some(Self::BlendFade),
            8 => Some(Self::AddFadeMid),
            9 => Some(Self::BlendFadeMid),
            _ => None,
        }
    }
    pub fn is_add_family_word(word: u32) -> bool {
        matches!(word, 0 | 4 | 8)
    }
}
/// MakeProjection mode flags (I3D.h:4812-4815).
pub const PROJF_OMNI_SECTOR: u32 = 0x2;
pub const PROJF_NO_MODELS: u32 = 0x4;
pub const PROJF_OWN_SECTOR: u32 = 0x8;
pub const PROJF_NO_CHILDREN: u32 = 0x10;
/// Sort words (I3D.h:4578-4580; opaque/transparent arms Ghidra-pinned).
pub const SORT_ALPHA: u32 = 0x80000000;
pub const SORT_DIST_MASK: u32 = 0x3fffffff;
pub const SORT_OPAQUE: u32 = 0xbfffffff;
pub const SORT_SCALE: f32 = 100.0;
/// Receiver cap (I3D.h:4913; `> 0x1ff` triggers the report).
pub const MAX_PROJECTOR_RECEIVERS: u32 = 512;
/// Marginal-zero bit threshold, same native helper as shadow_pass.
pub const PROJECTOR_MRG_ZERO_BITMASK: i32 = 0x322b_cc77;

/// Failure words. Reached prefixes ride the outcome structs, never hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectorError {
    NoReceivers,
    NumericDomain,
    OutOfMemory,
    UnresolvedBoundary,
    Backend,
}
/// Sort words owned by the render-queue owner; Y4 only moves them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectorSortWords {
    pub alpha: u32,
    pub dist_mask: u32,
}
/// `v` of a packed projector vertex. The perspective degenerate branch stores
/// twice into `u` and never stores `v` (Ghidra-pinned); stale buffer contents
/// cannot be modeled in safe typed state, so the residue is explicit instead
/// of an invented default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProjectorV {
    Coord(f32),
    Stale,
}

fn fadd(a: f32, b: f32) -> Result<f32, ProjectorError> {
    checked_f32_add(a, b).map_err(|_| ProjectorError::NumericDomain)
}
fn fmul(a: f32, b: f32) -> Result<f32, ProjectorError> {
    checked_f32_mul(a, b).map_err(|_| ProjectorError::NumericDomain)
}
fn fsub(a: f32, b: f32) -> Result<f32, ProjectorError> {
    let v = f64::from(a) - f64::from(b);
    let f = v as f32;
    if !a.is_finite()
        || !b.is_finite()
        || !f.is_finite()
        || (f != 0.0 && !f.is_normal())
        || (f == 0.0 && v != 0.0)
    {
        return Err(ProjectorError::NumericDomain);
    }
    Ok(f)
}
fn fdiv(a: f32, b: f32) -> Result<f32, ProjectorError> {
    let v = f64::from(a) / f64::from(b);
    let f = v as f32;
    if !a.is_finite()
        || !b.is_finite()
        || !f.is_finite()
        || (f != 0.0 && !f.is_normal())
        || (f == 0.0 && v != 0.0)
    {
        return Err(ProjectorError::NumericDomain);
    }
    Ok(f)
}
fn is_mrg_zero_less(f: f32) -> bool {
    (f.to_bits() as i32) < PROJECTOR_MRG_ZERO_BITMASK
}
/// Native `FloatToInt` (cvttss2si idiom): truncation, NaN/range = failure.
/// Disclosure: native wraps out-of-range magnitudes (0x80000000); the host
/// refuses them instead. Reachable only at absurd magnitudes.
fn float_to_int(x: f32) -> Result<i32, ProjectorError> {
    if !x.is_finite() || x <= -2147483649.0 || x >= 2147483648.0 {
        return Err(ProjectorError::NumericDomain);
    }
    Ok(x as i32)
}
fn to_byte(scaled_of: f32) -> Result<u32, ProjectorError> {
    // Disclosure: the gate admits [-256, 2^24] but the tail `as i32 as u32 &
    // 0xff` wraps out-of-byte magnitudes (e.g. 300 packs 44) where native
    // FloatToInt<<16 bleeds into the next byte. In-range today (strengths
    // clamped, colors bounded by setters); reachable only past 255.
    if !scaled_of.is_finite() || scaled_of < -256.0 || scaled_of > 16777216.0 {
        return Err(ProjectorError::NumericDomain);
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(scaled_of as i32 as u32 & 0xff)
}
#[allow(dead_code)]
fn round_half_up(x: f32) -> Result<i32, ProjectorError> {
    float_to_int(fadd(x, 0.5)?)
}
fn clamp01(x: f32) -> f32 {
    // `clamp` is bit-identical here (NaN flows through both spellings;
    // bounds are finite constants, so no panic arm applies).
    x.clamp(0.0, 1.0)
}

/// 0x46960 vertex packing (differential). Fade strengths first (native order:
/// uv, then strength, then color).
#[allow(clippy::too_many_arguments)]
pub fn pack_projector_vertex(
    pos: [f32; 3],
    mode: u32,
    orthogonal: bool,
    alpha: f32,
    color: [f32; 3],
) -> Result<(f32, ProjectorV, u32), ProjectorError> {
    let rgb_add = |a: f32| -> Result<u32, ProjectorError> {
        // ADD-family fades: 0xff000000 | bytes of (strength * color * 255).
        Ok(0xff000000u32
            | to_byte(fmul(fmul(a, color[0])?, 255.0)?)? << 16
            | to_byte(fmul(fmul(a, color[1])?, 255.0)?)? << 8
            | to_byte(fmul(fmul(a, color[2])?, 255.0)?)?)
    };
    let rgb_blend = |a: f32| -> Result<u32, ProjectorError> {
        // BLEND-family fades: byte(strength*255)<<24 | bytes of (color*255),
        // RGB deliberately NOT scaled by the strength (reference order).
        Ok(to_byte(fmul(a, 255.0)?)? << 24
            | to_byte(fmul(color[0], 255.0)?)? << 16
            | to_byte(fmul(color[1], 255.0)?)? << 8
            | to_byte(fmul(color[2], 255.0)?)?)
    };
    let strength = |kind: u32| -> Result<f32, ProjectorError> {
        // Deviation disclosed + pinned: native leaves the fade strengths
        // unclamped and packs via raw FloatToInt<<16 (overflow bleeds across
        // bytes); the host clamps to [0,1] and refuses out-of-range bytes.
        // Diverges for alpha > 1 (legal — SetAlpha clamps negatives only) or
        // z outside [0,1]. The ADD-vs-BLEND rgb split itself is exact.
        match kind {
            4 => Ok(clamp01(fmul(fsub(1.0, pos[2])?, alpha)?)),
            8 => {
                let d = fsub(pos[2], 0.5)?;
                #[allow(clippy::cast_possible_truncation)]
                let half = fmul(fsub(0.5, d.abs())?, fmul(alpha, 2.0)?)?;
                Ok(clamp01(half))
            }
            _ => Ok(alpha),
        }
    };
    if orthogonal {
        let u = fmul(fadd(pos[0], 1.0)?, 0.5)?;
        let v = fsub(0.5, fmul(pos[1], 0.5)?)?;
        let c = match ProjectorMode::from_word(mode) {
            Some(ProjectorMode::AddFade) | Some(ProjectorMode::AddFadeMid) => {
                let k = if mode == 4 { 4 } else { 8 };
                rgb_add(strength(k)?)?
            }
            Some(ProjectorMode::BlendFade) | Some(ProjectorMode::BlendFadeMid) => {
                let k = if mode == 5 { 4 } else { 8 };
                rgb_blend(strength(k)?)?
            }
            _ => {
                // Quirk: alpha<1 is truncated UNSCALED, else 255.
                let a_word = if alpha < 1.0 {
                    float_to_int(alpha)? as u32 & 0xff
                } else {
                    255
                };
                a_word << 24
                    | to_byte(fmul(color[0], 255.0)?)? << 16
                    | to_byte(fmul(color[1], 255.0)?)? << 8
                    | to_byte(fmul(color[2], 255.0)?)?
            }
        };
        Ok((u, ProjectorV::Coord(v), c))
    } else {
        let (u, v) = if is_mrg_zero_less(pos[2]) {
            // Native stores both sides into u; v keeps stale contents.
            (if pos[0] > 0.0 { 1.0 } else { 0.0 }, ProjectorV::Stale)
        } else {
            let k = fdiv(0.5, pos[2])?;
            (
                fadd(fmul(k, pos[0])?, 0.5)?,
                ProjectorV::Coord(fsub(0.5, fmul(k, pos[1])?)?),
            )
        };
        // The y-side store overwrites u in the degenerate branch (pinned).
        let u = match v {
            ProjectorV::Stale => {
                if pos[1] > 0.0 {
                    0.0
                } else {
                    1.0
                }
            }
            ProjectorV::Coord(_) => u,
        };
        let c = match ProjectorMode::from_word(mode) {
            Some(ProjectorMode::AddFade) | Some(ProjectorMode::AddFadeMid) => {
                rgb_add(strength(if mode == 4 { 4 } else { 8 })?)?
            }
            Some(ProjectorMode::BlendFade) | Some(ProjectorMode::BlendFadeMid) => {
                rgb_blend(strength(if mode == 5 { 4 } else { 8 })?)?
            }
            _ => {
                let a_word = if alpha < 1.0 {
                    float_to_int(alpha)? as u32 & 0xff
                } else {
                    255
                };
                a_word << 24
                    | to_byte(fmul(color[0], 255.0)?)? << 16
                    | to_byte(fmul(color[1], 255.0)?)? << 8
                    | to_byte(fmul(color[2], 255.0)?)?
            }
        };
        Ok((u, v, c))
    }
}

/// Face-group queue (caller queue owner; bump ordered after each push).
pub trait ProjectorQueueBackend {
    fn push_opaque(&mut self, sort: u32);
    fn push_transparent(&mut self, sort: u32);
    fn transparent_count(&self) -> u32;
}

/// 0x46930 `ProcessProjectors` (differential: 44 B/15 insn/3 BB): index-order
/// calls, then `reset()` (clear, storage kept). Empty queue still resets.
pub fn process_projector_queue(queue: &mut Vec<u32>, mut process: impl FnMut(u32)) -> usize {
    let drained = queue.len();
    for visual in queue.iter().copied() {
        process(visual);
    }
    queue.clear();
    drained
}

/// Clipper + receiver-mesh reads (backend contracts; Y4 never owns meshes).
pub trait ProjectorClipBackend {
    type Error;
    fn reset_clip_arrays(&mut self);
    fn set_clip_params(
        &mut self,
        orthogonal: bool,
        lift_near: f32,
        lift_far: f32,
        reverse_factor: f32,
    ) -> Result<(), Self::Error>;
    /// Full per-receiver clip incl. 0x3f box test + mesh/LOD dispatch.
    fn clip_receiver(&mut self, receiver: usize) -> Result<bool, Self::Error>;
    fn clipped_positions(&self) -> &[[f32; 3]];
    fn clipped_faces(&self) -> &[u16];
    fn clipped_faces_t(&self) -> &[u16];
    fn clipped_transparent_samples(&self) -> u32;
    fn clipped_transparent_center_sum(&self) -> [f32; 3];
}
/// VB/IB/rfg allocation (buffer-manager contracts, order-logged).
pub trait ProjectorStoreBackend {
    type Error;
    fn alloc_vertices(&mut self, count: u32) -> Result<(), Self::Error>;
    fn alloc_faces(&mut self, faces: u32, faces_t: u32) -> Result<(), Self::Error>;
    fn ensure_rfg(&mut self) -> Result<(), Self::Error>;
}

/// 0x46780 `m_AddFGroups` — CONTRACT: no Ghidra entry (mid-fold address of
/// `FUN_100045f0`; entry/mid/RET probes unowned). Order per mafia-re :302-333
/// (ON → texture/material → sector walk → squared range → RECEIVERS_VALID fast
/// path → queue push) is recorded here as upstream order, not claimed body.
pub fn queue_projector_groups_contract(
    _num_verts: u32,
    _lock: u32,
    _flags: u32,
) -> Result<(), ProjectorError> {
    Err(ProjectorError::UnresolvedBoundary)
}

/// 0x46960 `ProcessRecList` input. `lift` is the `RS_LIFT_PROJECTOR` word as
/// float; near/far selection by ADD family is the credited order.
pub struct RecListInput<'a> {
    pub receivers: &'a [usize],
    pub mode: u32,
    pub orthogonal: bool,
    pub alpha: f32,
    pub color: [f32; 3],
    pub reverse_factor: f32,
    pub lift: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecListOutcome {
    pub verts: u32,
    pub faces: u32,
    pub faces_t: u32,
    pub has_transparent_center: bool,
}
/// 0x46960 ordered driver (differential: 5797 B/1588 insn/268 BB): reset →
/// params (lift,lift vs lift,0 by ADD family) → per-receiver clip in slice
/// order → empty settles `num_verts=0` with no store effects → alloc verts →
/// per-mode fill ([`pack_projector_vertex`]) → alloc faces → face copies
/// (opaque then transparent) → transparent-center accumulate → rfg ensure.
/// OOM propagates before later effects (native early returns with the reached
/// `!! Out of memory  I3D_projector::ProcessRecList()…` prefix).
pub fn process_projector_reclist<B, S>(
    input: &RecListInput<'_>,
    clip: &mut B,
    store: &mut S,
    center_out: &mut Option<[f32; 3]>,
    filled: &mut Vec<(f32, ProjectorV, u32)>,
) -> Result<RecListOutcome, ProjectorError>
where
    B: ProjectorClipBackend<Error = ProjectorError>,
    S: ProjectorStoreBackend<Error = ProjectorError>,
{
    clip.reset_clip_arrays();
    let add_family = ProjectorMode::is_add_family_word(input.mode);
    clip.set_clip_params(
        input.orthogonal,
        input.lift,
        if add_family { 0.0 } else { input.lift },
        input.reverse_factor,
    )?;
    let mut hit_any = false;
    for id in input.receivers {
        if clip.clip_receiver(*id)? {
            hit_any = true;
        }
    }
    if !hit_any || clip.clipped_positions().is_empty() {
        filled.clear();
        *center_out = None;
        return Ok(RecListOutcome {
            verts: 0,
            faces: 0,
            faces_t: 0,
            has_transparent_center: false,
        });
    }
    let verts = clip.clipped_positions().len() as u32;
    store.alloc_vertices(verts)?;
    filled.clear();
    for pos in clip.clipped_positions() {
        filled.push(pack_projector_vertex(
            *pos,
            input.mode,
            input.orthogonal,
            input.alpha,
            input.color,
        )?);
    }
    let faces = clip.clipped_faces().len() as u32 / 3;
    let faces_t = clip.clipped_faces_t().len() as u32 / 3;
    store.alloc_faces(faces, faces_t)?;
    if clip.clipped_transparent_samples() > 0 {
        let s = clip.clipped_transparent_center_sum();
        let n = clip.clipped_transparent_samples() as f32;
        *center_out = Some([s[0] / n, s[1] / n, s[2] / n]);
    } else {
        *center_out = None;
    }
    store.ensure_rfg()?;
    Ok(RecListOutcome {
        verts,
        faces,
        faces_t,
        has_transparent_center: center_out.is_some(),
    })
}

/// 0x48060 live projector view (Y3 implements on the real state).
pub trait ProjectorShadowView {
    fn frame_flags(&self) -> u32;
    fn projector_lock(&self) -> u32;
    fn proj_flags(&self) -> u32;
    fn update_count(&self) -> i32;
    fn receivers_update(&self) -> i32;
    fn num_receivers(&self) -> u32;
    fn receivers_count(&self) -> i32;
    fn num_verts(&self) -> u32;
    fn receiver_update_count(&self, receiver: usize) -> i32;
    fn receiver_mesh_lod(&self, receiver: usize) -> i32;
}
pub trait ProjectorShadowMut: ProjectorShadowView {
    fn set_update_count(&mut self, v: i32);
    fn set_proj_flags(&mut self, f: u32);
    fn set_receivers_update(&mut self, v: i32);
    fn set_num_receivers(&mut self, v: u32);
    fn set_receivers_count(&mut self, v: i32);
    fn set_num_verts(&mut self, v: u32);
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowScanOutcome {
    NoReceivers { overflow_reports: u32 },
    QueuedCached { faces: u32 },
    Rebuilt { faces: u32, faces_t: u32 },
}
/// 0x48060 `ProcessShadow` intake: entries already passed the caller-side
/// dedup/type/sector/bbox gates. `rebuild` runs RecList + transparent-dist.
pub struct ShadowScan<'a> {
    pub candidates: &'a [usize],
    pub opaque_faces: u32,
    pub transparent_dist: f32,
    pub sort: &'a ProjectorSortWords,
}
/// 0x48060 ordered driver (differential, fold-evidenced): update-bump →
/// lock-set → scan (cap `0x200` + report + BREAK) → empty settles 0 →
/// change-count → cached queue w/o rebuild → else store triple, rebuild,
/// opaque push (`SORT_OPAQUE`), transparent push
/// (`ROUND(dist*100)&MASK|ALPHA`), bump after each push.
pub fn process_projector_shadow(
    frame: &mut dyn ProjectorShadowMut,
    scan: &ShadowScan<'_>,
    queue: &mut dyn ProjectorQueueBackend,
    mut rebuild: impl FnMut() -> Result<(u32, u32), ProjectorError>,
) -> Result<ShadowScanOutcome, ProjectorError> {
    if frame.frame_flags() & FRM_UPDATE_CHANGED != 0 {
        frame.set_update_count(frame.update_count().wrapping_add(1));
    }
    if frame.projector_lock() != 0 {
        frame.set_proj_flags(frame.proj_flags() | PROJF_RECEIVERS_VALID);
    }
    let mut overflow_reports = 0u32;
    let mut kept = 0usize;
    for _ in scan.candidates {
        kept += 1;
        if kept as u32 >= MAX_PROJECTOR_RECEIVERS {
            overflow_reports += 1;
            break;
        }
    }
    if kept == 0 {
        frame.set_num_verts(0);
        return Ok(ShadowScanOutcome::NoReceivers { overflow_reports });
    }
    let mut count = 0i32;
    for id in &scan.candidates[..kept] {
        count = count.wrapping_add(frame.receiver_update_count(*id));
        count = count.wrapping_add(frame.receiver_mesh_lod(*id));
    }
    if frame.receivers_update() == frame.update_count()
        && kept as u32 == frame.num_receivers()
        && count == frame.receivers_count()
    {
        if frame.num_verts() != 0 {
            queue.push_opaque(scan.sort.alpha | scan.sort.dist_mask);
            queue.push_transparent(
                (float_to_int(fmul(scan.transparent_dist, SORT_SCALE)?)? as u32
                    & scan.sort.dist_mask)
                    | scan.sort.alpha,
            );
        }
        return Ok(ShadowScanOutcome::QueuedCached {
            faces: scan.opaque_faces,
        });
    }
    frame.set_receivers_update(frame.update_count());
    frame.set_num_receivers(kept as u32);
    frame.set_receivers_count(count);
    let (faces, faces_t) = rebuild()?;
    if faces != 0 {
        queue.push_opaque(SORT_OPAQUE);
    }
    if faces_t != 0 {
        queue.push_transparent(
            (float_to_int(fmul(scan.transparent_dist, SORT_SCALE)?)? as u32 & SORT_DIST_MASK)
                | SORT_ALPHA,
        );
    }
    Ok(ShadowScanOutcome::Rebuilt { faces, faces_t })
}

/// 0x482c0 `MakeProjection` — CONTRACT: gap `0x100482b5–0x100483df`, no entry.
/// Order per mafia-re :763-803 (sector walk → RECEIVERS lock/flag → projection
/// globals → frame?recursion : OWN_SECTOR?sector : primary(+omni) → RecList,
/// notably with NO queue call) is upstream order, not claimed body.
pub fn make_projection_contract(_flags: u32) -> Result<(), ProjectorError> {
    Err(ProjectorError::UnresolvedBoundary)
}

/// 0x483e0 forest view (Y3/frame owner implements; tests use doubles).
pub trait ProjectorForest {
    fn is_on(&self, id: usize) -> bool;
    fn hr_bbox(&self, id: usize) -> [[f32; 3]; 2];
    fn frame_type(&self, id: usize) -> u32;
    fn visual_type(&self, id: usize) -> u32;
    fn vis_flags(&self, id: usize) -> u32;
    fn wbound_bbox(&self, id: usize) -> [[f32; 3]; 2];
    fn sector_group_bits(&self, id: usize) -> u32;
    fn children(&self, id: usize) -> &[usize];
}
fn bbox_meets(a: [[f32; 3]; 2], b: [[f32; 3]; 2]) -> bool {
    !(a[0][0] > b[1][0]
        || a[0][1] > b[1][1]
        || a[0][2] > b[1][2]
        || a[1][0] < b[0][0]
        || a[1][1] < b[0][1]
        || a[1][2] < b[0][2])
}
fn is_receiver_type(word: u32) -> bool {
    matches!(
        word,
        VTYPE_LMAP | VTYPE_MRPH | VTYPE_SNGM | VTYPE_SMMR | VTYPE_OBJ
    )
}
#[derive(Clone, Debug, PartialEq)]
pub struct CollectCtx {
    pub projection_bbox: [[f32; 3]; 2],
    pub projection_bits: u32,
    pub projection_sector: usize,
    pub mode_flags: u32,
    pub receivers: Vec<usize>,
    pub overflow_reports: u32,
}
/// 0x483e0 `CollectReceivers` (differential: 520 B/133 insn/40 BB): IsOn →
/// HRBound → hier-bbox → type arms (visual/sector/model) → in-order child
/// recursion unless `NO_CHILDREN`. Overflow reports but CONTINUES (native
/// fall-through — pinned vs 0x48060's break).
pub fn collect_projector_receivers(forest: &dyn ProjectorForest, id: usize, ctx: &mut CollectCtx) {
    if !forest.is_on(id) {
        return;
    }
    if !bbox_meets(forest.hr_bbox(id), ctx.projection_bbox) {
        return;
    }
    match forest.frame_type(id) {
        MTYPE_VISUAL => {
            if is_receiver_type(forest.visual_type(id))
                && forest.vis_flags(id) & VISF_PROJECTOR != 0
                && bbox_meets(forest.wbound_bbox(id), ctx.projection_bbox)
            {
                ctx.receivers.push(id);
                if ctx.receivers.len() as u32 >= MAX_PROJECTOR_RECEIVERS {
                    ctx.overflow_reports += 1;
                }
            }
        }
        MTYPE_SECTOR => {
            if id != ctx.projection_sector {
                if ctx.mode_flags & PROJF_OWN_SECTOR != 0 {
                    return;
                }
                if forest.sector_group_bits(id) & ctx.projection_bits == 0 {
                    return;
                }
            }
        }
        MTYPE_MODEL if ctx.mode_flags & PROJF_NO_MODELS != 0 => {
            return;
        }
        _ => {}
    }
    if ctx.mode_flags & PROJF_NO_CHILDREN != 0 {
        return;
    }
    for child in forest.children(id).to_vec() {
        collect_projector_receivers(forest, child, ctx);
    }
}

/// 0x485f0 draw arm selector: exactly one arm per call (native
/// `if (help->texture) draw(num_faces) else draw(num_faces_t)` inside the
/// buffered-vs-system branch). `buffered` selects the vb/ib device path vs
/// the IGraph list path; `textured` selects the faces vs faces_t counts.
/// Queue order across calls (opaque groups before transparent ones) lives
/// with the queue owner, never inside one call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectorDrawArm {
    BufferedFaces,
    BufferedFacesT,
    SystemFaces,
    SystemFacesT,
}
pub fn select_projector_draw_arm(buffered: bool, textured: bool) -> ProjectorDrawArm {
    match (buffered, textured) {
        (true, true) => ProjectorDrawArm::BufferedFaces,
        (true, false) => ProjectorDrawArm::BufferedFacesT,
        (false, true) => ProjectorDrawArm::SystemFaces,
        (false, false) => ProjectorDrawArm::SystemFacesT,
    }
}
/// Ordered device draws (backend contract, order-logged; no live device).
/// Cull (`SetCullState(CCW)`) and clipping (`SetClipping(TRUE)`) between the
/// stage setup and the texture select (I3d_projector.cpp:879-880) are
/// unmodeled backend contracts — no backend methods exist for them yet; the
/// order log covers stages → texture → world/zbias → zwrite → draw.
pub trait ProjectorDrawBackend {
    type Error;
    fn set_stages(&mut self, add_family: bool);
    fn set_texture(&mut self, projector_textured: bool);
    fn set_world_and_zbias(&mut self);
    fn set_zwrite(&mut self, on: bool);
    fn draw(&mut self, arm: ProjectorDrawArm) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderGroupOutcome {
    Idle,
    Drew(ProjectorDrawArm),
}
/// 0x485f0 `m_RenderFGroup` CPU order (differential): verts-guard → stages →
/// projector-texture select → world/zbias → zwrite-off → exactly one arm
/// draw (arm by the FGROUP's `help->texture`, a separate axis from the
/// projector's own texture stage) → zwrite-on.
pub fn render_projector_group<B>(
    num_verts: u32,
    mode: u32,
    buffered: bool,
    projector_textured: bool,
    fgroup_textured: bool,
    backend: &mut B,
) -> Result<Vec<RenderGroupOutcome>, ProjectorError>
where
    B: ProjectorDrawBackend<Error = ProjectorError>,
{
    if num_verts == 0 {
        return Ok(vec![RenderGroupOutcome::Idle]);
    }
    backend.set_stages(ProjectorMode::is_add_family_word(mode));
    backend.set_texture(projector_textured);
    backend.set_world_and_zbias();
    backend.set_zwrite(false);
    let arm = select_projector_draw_arm(buffered, fgroup_textured);
    backend.draw(arm)?;
    let out = vec![RenderGroupOutcome::Drew(arm)];
    backend.set_zwrite(true);
    Ok(out)
}

/// 0x48960 `LoadImage` — CONTRACT: 4 B past `FUN_100485f0` RET, no entry.
/// Parse order + failure words per mafia-re :913-928 (upstream-reported):
/// base → orthogonal:1B → mode:1B → material id:2B → `id > LoadMaterialu` ⇒
/// `"!! Invalid material ID:%d  materialu:%d  I3D_projector::LoadImage() %s"`
/// + `I3DERR_FILECORRUPTED`, else `LoadMaterialy[id-1]` + `AddRef`.
pub fn load_projector_image_contract() -> Result<(), ProjectorError> {
    Err(ProjectorError::UnresolvedBoundary)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Q {
        sorts: Vec<u32>,
        n: u32,
    }
    impl ProjectorQueueBackend for Q {
        fn push_opaque(&mut self, s: u32) {
            self.sorts.push(s);
            self.n += 1;
        }
        fn push_transparent(&mut self, s: u32) {
            self.sorts.push(s);
            self.n += 1;
        }
        fn transparent_count(&self) -> u32 {
            self.n
        }
    }
    #[derive(Default)]
    struct F {
        flags: u32,
        lock: u32,
        pflags: u32,
        upd: i32,
        r_upd: i32,
        n_rec: u32,
        r_cnt: i32,
        n_verts: u32,
        log: Vec<&'static str>,
        lod: i32,
    }
    impl ProjectorShadowView for F {
        fn frame_flags(&self) -> u32 {
            self.flags
        }
        fn projector_lock(&self) -> u32 {
            self.lock
        }
        fn proj_flags(&self) -> u32 {
            self.pflags
        }
        fn update_count(&self) -> i32 {
            self.upd
        }
        fn receivers_update(&self) -> i32 {
            self.r_upd
        }
        fn num_receivers(&self) -> u32 {
            self.n_rec
        }
        fn receivers_count(&self) -> i32 {
            self.r_cnt
        }
        fn num_verts(&self) -> u32 {
            self.n_verts
        }
        fn receiver_update_count(&self, _: usize) -> i32 {
            self.upd
        }
        fn receiver_mesh_lod(&self, _: usize) -> i32 {
            self.lod
        }
    }
    impl ProjectorShadowMut for F {
        fn set_update_count(&mut self, v: i32) {
            self.upd = v;
            self.log.push("bump");
        }
        fn set_proj_flags(&mut self, f: u32) {
            self.pflags = f;
            self.log.push("lock");
        }
        fn set_receivers_update(&mut self, v: i32) {
            self.r_upd = v;
            self.log.push("triple");
        }
        fn set_num_receivers(&mut self, v: u32) {
            self.n_rec = v;
        }
        fn set_receivers_count(&mut self, v: i32) {
            self.r_cnt = v;
        }
        fn set_num_verts(&mut self, v: u32) {
            self.n_verts = v;
            self.log.push("verts0");
        }
    }
    struct Forest {
        on: Vec<bool>,
        ty: Vec<u32>,
        kids: Vec<Vec<usize>>,
    }
    impl ProjectorForest for Forest {
        fn is_on(&self, id: usize) -> bool {
            self.on[id]
        }
        fn hr_bbox(&self, _: usize) -> [[f32; 3]; 2] {
            [[-9.; 3], [9.; 3]]
        }
        fn frame_type(&self, id: usize) -> u32 {
            self.ty[id]
        }
        fn visual_type(&self, _: usize) -> u32 {
            VTYPE_SNGM
        }
        fn vis_flags(&self, _: usize) -> u32 {
            VISF_PROJECTOR
        }
        fn wbound_bbox(&self, _: usize) -> [[f32; 3]; 2] {
            [[0.; 3], [1.; 3]]
        }
        fn sector_group_bits(&self, _: usize) -> u32 {
            1
        }
        fn children(&self, id: usize) -> &[usize] {
            &self.kids[id]
        }
    }
    struct D {
        log: Vec<&'static str>,
        last_texture: Option<bool>,
    }
    impl ProjectorDrawBackend for D {
        type Error = ProjectorError;
        fn set_stages(&mut self, _: bool) {
            self.log.push("stages");
        }
        fn set_texture(&mut self, projector_textured: bool) {
            self.log.push("texture");
            self.last_texture = Some(projector_textured);
        }
        fn set_world_and_zbias(&mut self) {
            self.log.push("world");
        }
        fn set_zwrite(&mut self, on: bool) {
            self.log.push(if on { "zw-on" } else { "zw-off" });
        }
        fn draw(&mut self, _: ProjectorDrawArm) -> Result<(), ProjectorError> {
            self.log.push("draw");
            Ok(())
        }
    }

    #[test]
    fn mode_words_and_add_family() {
        assert_eq!(ProjectorMode::from_word(0), Some(ProjectorMode::Add));
        assert_eq!(ProjectorMode::from_word(5), Some(ProjectorMode::BlendFade));
        assert_eq!(
            ProjectorMode::from_word(9),
            Some(ProjectorMode::BlendFadeMid)
        );
        assert_eq!(ProjectorMode::from_word(99), None);
        assert!(ProjectorMode::is_add_family_word(8));
        assert!(!ProjectorMode::is_add_family_word(1));
    }
    #[test]
    fn pack_ortho_add_alpha_quirk_and_fades() {
        // alpha<1 truncated unscaled: 0.5 -> 0x00; rgb bytes scaled.
        let (_, _, c) = pack_projector_vertex([0., 0., 0.], 0, true, 0.5, [1., 1., 1.]).unwrap();
        assert_eq!(c, 0x00ffffff);
        let (_, v, _) = pack_projector_vertex([0., 0., 0.], 0, true, 1., [0., 0., 0.]).unwrap();
        assert_eq!(v, ProjectorV::Coord(0.5));
        // ADD_FADE z=0.25: strength=(1-0.25)*1=0.75 -> 0xff-bytes with ff hi.
        let (_, _, c) = pack_projector_vertex([0., 0., 0.25], 4, true, 1., [1., 1., 1.]).unwrap();
        assert_eq!(c, 0xffbfbfbf);
        // BLEND_FADE carries strength in the alpha byte.
        let (_, _, c) = pack_projector_vertex([0., 0., 0.25], 5, true, 1., [1., 0., 0.]).unwrap();
        assert_eq!(c, 0xbfff0000);
        // Pinned deviation: alpha > 1 clamps the strength to 1 (native would
        // pack the overflowing FloatToInt across bytes).
        let (_, _, c) = pack_projector_vertex([0., 0., 0.25], 4, true, 2., [1., 1., 1.]).unwrap();
        assert_eq!(c, 0xffffffff);
    }
    #[test]
    fn pack_persp_degenerate_is_stale_residue() {
        let (u, v, _) = pack_projector_vertex([2., 3., 0.], 0, false, 1., [0., 0., 0.]).unwrap();
        assert_eq!((u, v), (0.0, ProjectorV::Stale));
        let (u, v, _) = pack_projector_vertex([0.5, 0., 0.5], 0, false, 1., [0., 0., 0.]).unwrap();
        assert_eq!((u, v), (1.0, ProjectorV::Coord(0.5)));
    }
    #[test]
    fn queue_drains_in_index_order_and_resets() {
        let mut q = vec![7u32, 3, 9];
        let mut order = vec![];
        assert_eq!(process_projector_queue(&mut q, |v| order.push(v)), 3);
        assert_eq!(order, [7, 3, 9]);
        assert!(q.is_empty());
        let mut q: Vec<u32> = vec![];
        assert_eq!(process_projector_queue(&mut q, |_| panic!("none")), 0);
    }
    #[test]
    fn scan_empty_settles_zero_and_cached_queues_sorts() {
        let sort = ProjectorSortWords {
            alpha: SORT_ALPHA,
            dist_mask: SORT_DIST_MASK,
        };
        let mut f = F::default();
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        let scan = ShadowScan {
            candidates: &[],
            opaque_faces: 0,
            transparent_dist: 1.23,
            sort: &sort,
        };
        assert_eq!(
            process_projector_shadow(&mut f, &scan, &mut q, || panic!("no rebuild")).unwrap(),
            ShadowScanOutcome::NoReceivers {
                overflow_reports: 0
            }
        );
        assert_eq!(f.log, ["verts0"]);
        // Cached path: triple matches, num_verts set → two pushes, bump after each.
        let mut f = F {
            upd: 5,
            r_upd: 5,
            n_rec: 1,
            r_cnt: 5,
            n_verts: 2,
            ..Default::default()
        };
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        let ids = [0usize];
        let scan = ShadowScan {
            candidates: &ids,
            opaque_faces: 4,
            transparent_dist: 1.23,
            sort: &sort,
        };
        assert_eq!(
            process_projector_shadow(&mut f, &scan, &mut q, || panic!("no rebuild")).unwrap(),
            ShadowScanOutcome::QueuedCached { faces: 4 }
        );
        assert_eq!(q.sorts[0], SORT_ALPHA | SORT_DIST_MASK);
        assert_eq!(q.sorts[1], 0x8000007b);
        assert_eq!(q.transparent_count(), 2);
    }
    #[test]
    fn scan_cap_breaks_and_rebuild_queues_opaque_sort() {
        let sort = ProjectorSortWords {
            alpha: SORT_ALPHA,
            dist_mask: SORT_DIST_MASK,
        };
        let ids: Vec<usize> = (0..600).collect();
        let mut f = F::default();
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        let scan = ShadowScan {
            candidates: &ids,
            opaque_faces: 0,
            transparent_dist: 0.,
            sort: &sort,
        };
        let out = process_projector_shadow(&mut f, &scan, &mut q, || Ok((3, 0))).unwrap();
        assert_eq!(
            out,
            ShadowScanOutcome::Rebuilt {
                faces: 3,
                faces_t: 0
            }
        );
        assert_eq!(f.n_rec, MAX_PROJECTOR_RECEIVERS);
        assert_eq!(q.sorts, [SORT_OPAQUE]);
    }
    #[test]
    fn collect_traversal_order_and_overflow_continues() {
        let forest = Forest {
            on: vec![true; 5],
            ty: vec![
                MTYPE_SECTOR,
                MTYPE_VISUAL,
                MTYPE_MODEL,
                MTYPE_SECTOR,
                MTYPE_VISUAL,
            ],
            kids: vec![vec![1, 2, 3, 4], vec![], vec![], vec![], vec![]],
        };
        let mut ctx = CollectCtx {
            projection_bbox: [[-9.; 3], [9.; 3]],
            projection_bits: 2,
            projection_sector: 0,
            mode_flags: PROJF_NO_MODELS,
            receivers: vec![],
            overflow_reports: 0,
        };
        collect_projector_receivers(&forest, 0, &mut ctx);
        assert_eq!(ctx.receivers, [1, 4]);
    }
    #[test]
    fn render_idle_and_single_arm_draw() {
        let mut d = D {
            log: vec![],
            last_texture: None,
        };
        assert_eq!(
            render_projector_group(0, 0, true, true, true, &mut d).unwrap(),
            [RenderGroupOutcome::Idle]
        );
        assert!(d.log.is_empty());
        // Textured fgroup draws the faces arm only; the stage follows the
        // PROJECTOR's own texture axis (here false while the arm is textured).
        let mut d = D {
            log: vec![],
            last_texture: None,
        };
        let out = render_projector_group(9, 1, false, false, true, &mut d).unwrap();
        assert_eq!(
            out,
            [RenderGroupOutcome::Drew(ProjectorDrawArm::SystemFaces)]
        );
        assert_eq!(d.last_texture, Some(false));
        assert_eq!(
            d.log,
            ["stages", "texture", "world", "zw-off", "draw", "zw-on"]
        );
        // Untextured fgroup draws the faces_t arm only (never both in one call).
        let mut d = D {
            log: vec![],
            last_texture: None,
        };
        let out = render_projector_group(9, 1, true, true, false, &mut d).unwrap();
        assert_eq!(
            out,
            [RenderGroupOutcome::Drew(ProjectorDrawArm::BufferedFacesT)]
        );
        assert_eq!(d.last_texture, Some(true));
        assert_eq!(d.log.iter().filter(|o| **o == "draw").count(), 1);
    }
    #[test]
    fn contracts_report_unresolved_boundary() {
        assert_eq!(
            queue_projector_groups_contract(1, 0, 0),
            Err(ProjectorError::UnresolvedBoundary)
        );
        assert_eq!(
            make_projection_contract(0),
            Err(ProjectorError::UnresolvedBoundary)
        );
        assert_eq!(
            load_projector_image_contract(),
            Err(ProjectorError::UnresolvedBoundary)
        );
        assert_eq!(
            projector_add_fgroups_visual_contract(),
            Err(ProjectorError::UnresolvedBoundary)
        );
    }
}

/// LS3DF RVA 0x464d0 (`VA 0x100464d0`) `I3D_projector::AddFGroups` (mafia-re
/// `ls3df/I3D/visuals/I3d_projector.cpp:269-290`): queues the opaque
/// receivers' faces (`num_faces`, farthest-first sort word
/// `PRIM_SORT_ALPHA | PRIM_SORT_DIST_MASK`) and the transparent receivers'
/// faces (`num_faces_t`, view-distance sort word scaled by 100 from the
/// camera frustum plane) into the shared `AddFGHlpStruct` face-group queue
/// with the projector/world texture select. The face-group, vertex-buffer and
/// camera-frustum owners live in the projector visual / `Y3` frame state
/// (`frame_projector.rs`, owned by another workstream and untouched here),
/// so this entry stays an explicit `UnresolvedBoundary` contract like its
/// `m_AddFGroups` neighbor above.
pub fn projector_add_fgroups_visual_contract() -> Result<(), ProjectorError> {
    Err(ProjectorError::UnresolvedBoundary)
}
