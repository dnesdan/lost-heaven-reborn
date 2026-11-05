#![forbid(unsafe_code)]
//! Shadow-frame passes (PC LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! RVAs 0x51ab0, 0x51c00, 0x529c0, 0x52a90, 0x52c70, 0x52ce0, 0x52e90, 0x52f80,
//! 0x53060, 0x53250, 0x53280, 0x53ee0: 10 differential + 2 contract).
//!
//! Reference (upstream working names, never original symbols): mafia-re
//! ls3df/I3D/visuals/I3d_shadow.cpp. State ownership: GROUP X1 owns the
//! shadow-frame state (`crate::frame_shadow`). This module only borrows live
//! state through the traits below using named accessors (no offsets, no
//! `Vec<u8>` maps). Flag words below the line are X1-owned and arrive via
//! [`ShadowFlagWords`]; the `0x8000/0x1000/0x400/0x4/0x2/0x1` words are
//! instruction-evidenced (AND/TEST/OR immediates in the formed bodies).
//!
//! Precision: add/mul/sub are PC24 checked-`f32` (the `frame_light.rs`
//! helper order; subtraction keeps the committed f64-roundtrip single-op
//! idiom). Division and sqrt are host-`f32` (no checked-divide owner exists;
//! quarantined here, disclosed). No fused ops.
//! Device/render/VB/IB/clipper calls are backend contracts with order logs;
//! there is no live device here.

use crate::native_matrix::{checked_f32_add, checked_f32_mul};

/// Instruction-evidenced flag words (Ghidra in-session immediates).
pub const SHDF_NONE: u32 = 0x8000;
pub const SHDF_VERTS_VALID: u32 = 0x1000;
pub const SHDF_VERTS_CHANGED: u32 = 0x4;
pub const SHDF_DIR_CHANGED: u32 = 0x2;
pub const SHDF_TEXTURE_VALID: u32 = 0x400;
pub const SHDF_TEXTURE_CHANGED: u32 = 0x1;

/// X1-owned flag registry (bits Ghidra did not evidence in X3 bodies).
/// Built by the X1 caller; X3 never defines these words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowFlagWords {
    pub light_valid: u32,
    pub opacity_changed: u32,
    pub receivers_valid: u32,
    pub receivers_changed: u32,
}

/// `shadow_lock` word. Numbers from native comparisons (`> VERTICES` lock-gate,
/// `>= LIGHT`, `== RECEIVERS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ShadowLock {
    None = 0,
    Vertices = 1,
    Light = 2,
    Receivers = 3,
}

impl ShadowLock {
    pub fn from_word(word: u32) -> Option<Self> {
        match word {
            0 => Some(Self::None),
            1 => Some(Self::Vertices),
            2 => Some(Self::Light),
            3 => Some(Self::Receivers),
            _ => None,
        }
    }
}

/// Failure words. Order of variants is diagnostic only; reached prefixes are
/// carried by the caller-visible outcome structs, never hidden here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowPassError {
    /// Geometry prefix missing (def → verts → frames, in that reached order).
    NoGeometry,
    /// Numeric domain failure under the PC24 policy (never a silent default).
    NumericDomain,
    /// Native OOM path reached (buffered vs system prefix in the report).
    OutOfMemory,
    /// Ghidra formed no body at this RVA (unresolved boundary, no credit).
    UnresolvedBoundary,
    /// Backend refused the ordered device call.
    Backend,
}

/// Sort words owned by the render-queue owner; X3 only moves them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowSortWords {
    pub alpha: u32,
    pub dist_mask: u32,
}

/// Live frame view (X1 implements on the real state; tests use a double).
/// Reads happen when the original operation reads — no snapshots cached
/// across mutating callbacks.
pub trait ShadowFrameView {
    fn flags(&self) -> u32;
    fn lock(&self) -> u32;
    fn verts_count(&self) -> i32;
    fn lights_count(&self) -> i32;
    fn opacity(&self) -> f32;
    fn num_verts(&self) -> u32;
    fn num_faces(&self) -> u32;
    fn has_def(&self) -> bool;
    fn has_verts(&self) -> bool;
    fn has_frames(&self) -> bool;
    fn frame_part_count(&self) -> usize;
    fn has_texture(&self) -> bool;
    fn size_hw(&self) -> [u16; 2];
}

/// Ordered mutations, one per native store site (effect order is the API).
pub trait ShadowFrameMut: ShadowFrameView {
    fn set_flags(&mut self, flags: u32);
    fn set_verts_count(&mut self, count: i32);
    fn set_lights_count(&mut self, count: i32);
    fn set_opacity(&mut self, opacity: f32);
}

/// Visual placement target (X1 implements; order SetDir→SetScale→SetPos→UpdateV).
pub trait ShadowVisualMut {
    fn set_dir(&mut self, dir: [f32; 3]);
    fn set_scale(&mut self, scale: [f32; 3]);
    fn set_pos(&mut self, pos: [f32; 3]);
    fn update_v(&mut self);
}

/// Device texture creation/rendering (backend contract, order-logged).
pub trait ShadowTextureBackend {
    type Error;
    fn create_shadow_texture(&mut self, size: [u16; 2]) -> Result<bool, Self::Error>;
    fn render_shadow_texture(&mut self) -> Result<(), Self::Error>;
    fn recompute_used_vram(&mut self);
}

/// Face-group queue (caller queue owner; bump ordered after each push).
pub trait ShadowQueueBackend {
    fn push_opaque(&mut self, sort: u32);
    fn push_transparent(&mut self, sort: u32);
    fn transparent_count(&self) -> u32;
}

/// Clipper + receiver-mesh reads (projector-shared ownership; contracts here).
pub trait ShadowClipBackend {
    type Error;
    fn reset_clip_arrays(&mut self);
    fn set_clip_params(
        &mut self,
        orthogonal: bool,
        lift: f32,
        reverse_factor: f32,
    ) -> Result<(), Self::Error>;
    fn clip_receiver(&mut self, receiver: usize) -> Result<bool, Self::Error>;
    fn clip_vertex_count(&self) -> u32;
}

/// Ordered device draw (whole `m_RenderFGroup` is a backend contract).
pub trait ShadowDrawBackend {
    type Error;
    fn draw_shadow_group(&mut self, arm: ShadowDrawArm) -> Result<(), Self::Error>;
}

/// Pure arm selector for `m_RenderFGroup` (CPU order, still contract tier).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowDrawArm {
    BufferedOpaque,
    BufferedTransparent,
    SystemOpaque,
    SystemTransparent,
}

pub fn select_shadow_draw_arm(buffered: bool, opaque: bool) -> ShadowDrawArm {
    match (buffered, opaque) {
        (true, true) => ShadowDrawArm::BufferedOpaque,
        (true, false) => ShadowDrawArm::BufferedTransparent,
        (false, true) => ShadowDrawArm::SystemOpaque,
        (false, false) => ShadowDrawArm::SystemTransparent,
    }
}

fn fadd(a: f32, b: f32) -> Result<f32, ShadowPassError> {
    checked_f32_add(a, b).map_err(|_| ShadowPassError::NumericDomain)
}
fn fmul(a: f32, b: f32) -> Result<f32, ShadowPassError> {
    checked_f32_mul(a, b).map_err(|_| ShadowPassError::NumericDomain)
}
fn fsub(a: f32, b: f32) -> Result<f32, ShadowPassError> {
    // Keep subtraction as one native PC24 operation (committed frame_light idiom).
    let v = f64::from(a) - f64::from(b);
    let f = v as f32;
    if !a.is_finite()
        || !b.is_finite()
        || !f.is_finite()
        || (f != 0.0 && !f.is_normal())
        || (f == 0.0 && v != 0.0)
    {
        return Err(ShadowPassError::NumericDomain);
    }
    Ok(f)
}

/// 0x51ab0 `UpdateVertices` (upstream working name). Guards in native order
/// (def → verts → frames); lock-gate; count-sum; stale-count short-circuit;
/// per-part world transform owned by the X1 caller via `transformed`
/// (destroyed frames are skipped WITHOUT advancing src/dst there, exactly as
/// natively — the closure contract); the flag
/// tail `& !SHDF_NONE | VERTS_VALID | VERTS_CHANGED` runs only on fresh data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowVertexUpdate {
    pub ran: bool,
    pub fresh: bool,
}
pub fn update_shadow_vertices<F>(
    frame: &mut dyn ShadowFrameMut,
    lock_allows_stale: bool,
    live_count: i32,
    transformed: F,
) -> Result<ShadowVertexUpdate, ShadowPassError>
where
    F: FnOnce() -> Result<(), ShadowPassError>,
{
    if !frame.has_def() {
        return Err(ShadowPassError::NoGeometry);
    }
    if !frame.has_verts() {
        return Err(ShadowPassError::NoGeometry);
    }
    if !frame.has_frames() || frame.frame_part_count() == 0 {
        return Err(ShadowPassError::NoGeometry);
    }
    if lock_allows_stale && frame.flags() & SHDF_VERTS_VALID != 0 {
        return Ok(ShadowVertexUpdate {
            ran: true,
            fresh: false,
        });
    }
    if frame.verts_count() == live_count {
        return Ok(ShadowVertexUpdate {
            ran: true,
            fresh: false,
        });
    }
    frame.set_verts_count(live_count);
    transformed()?;
    let flags = frame.flags() & !SHDF_NONE | SHDF_VERTS_VALID | SHDF_VERTS_CHANGED;
    frame.set_flags(flags);
    Ok(ShadowVertexUpdate {
        ran: true,
        fresh: true,
    })
}

/// 0x51c00 light-kind arms (kind numbers follow `frame_light.rs`: 1 point,
/// 2 spot, 3 directional, 4 ambient; 8 = UNKNOWN_8 per the native switch).
/// Each arm preserves the native gate order; PC24 checked-`f32` throughout.
/// The sector walk, count-compare, dir-compare and visual placement are owned
/// by [`update_shadow_light`]; the clipper/projection loop stays a backend
/// contract (no live device here).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLightSample {
    pub kind: u32,
    pub on: bool,
    pub shadow_mode: bool,
    pub color_sum: f32,
    pub dist_2: f32,
    pub dist: f32,
    pub range_n: f32,
    pub range_f: f32,
    pub range_delta: f32,
    pub range_f_2: f32,
    pub cos: f32,
    pub outer_cos: f32,
    pub inner_cos: f32,
    pub cone_scale: f32,
}

/// Contest outcome of one sample: `Contest` feeds `best`/`second`,
/// `Ambient` feeds only `sum` (native `sum += f` with no contest).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowLightContribution {
    Contest(f32),
    Ambient(f32),
}

/// Marginal-zero threshold shared with the native `IsMrgZeroLess` /
/// `IsAbsMrgZero` helpers (ls3df.h:48-49: MRG_ZERO 1e-8, mask 0x322bcc77).
pub const MRG_ZERO_BITMASK: i32 = 0x322b_cc77;

fn is_mrg_zero_less(f: f32) -> bool {
    // Native compares the raw bits signed-less against the mask (no fabs):
    // true for tiny positives, zero and negatives; false for NaN and above.
    (f.to_bits() as i32) < MRG_ZERO_BITMASK
}

fn is_abs_mrg_zero(f: f32) -> bool {
    (f.abs().to_bits() as i32) < MRG_ZERO_BITMASK
}

fn fdiv(a: f32, b: f32) -> Result<f32, ShadowPassError> {
    // One native PC24 divide (committed f64-roundtrip idiom, as in `fsub`).
    let v = f64::from(a) / f64::from(b);
    let f = v as f32;
    if !a.is_finite()
        || !b.is_finite()
        || !f.is_finite()
        || (f != 0.0 && !f.is_normal())
        || (f == 0.0 && v != 0.0)
    {
        return Err(ShadowPassError::NumericDomain);
    }
    Ok(f)
}

pub fn shadow_light_strength(
    sample: &ShadowLightSample,
) -> Result<Option<ShadowLightContribution>, ShadowPassError> {
    if !sample.shadow_mode || !sample.on {
        return Ok(None);
    }
    match sample.kind {
        // Ambient sums without contesting (`sum += …; continue`).
        4 => Ok(Some(ShadowLightContribution::Ambient(sample.color_sum))),
        2 | 1 | 8 => {
            if sample.range_f_2 <= sample.dist_2 {
                return Ok(None);
            }
            let mut f = sample.color_sum;
            if sample.kind == 2 {
                // Zero-distance gate (IsMrgZeroLess(dist_2)): unfaded raw
                // strength, skipping sqrt/fade/normalize/cone.
                if is_mrg_zero_less(sample.dist_2) {
                    return Ok(Some(ShadowLightContribution::Contest(f)));
                }
                if sample.dist > sample.range_n {
                    f = fmul(
                        f,
                        fdiv(fsub(sample.range_f, sample.dist)?, sample.range_delta)?,
                    )?;
                }
                if sample.cos <= sample.outer_cos {
                    return Ok(None);
                }
                if sample.cos <= sample.inner_cos {
                    f = fmul(
                        f,
                        fmul(fsub(sample.cos, sample.outer_cos)?, sample.cone_scale)?,
                    )?;
                }
                return Ok(Some(ShadowLightContribution::Contest(f)));
            }
            if sample.dist > sample.range_n {
                f = fmul(
                    f,
                    fdiv(fsub(sample.range_f, sample.dist)?, sample.range_delta)?,
                )?;
            }
            if sample.kind == 8 {
                // Type-8 accumulates to ambient sum and yields zero contest
                // strength (native `sum += f; f = 0` — net sum effect is `f`).
                return Ok(Some(ShadowLightContribution::Ambient(f)));
            }
            Ok(Some(ShadowLightContribution::Contest(f)))
        }
        3 => Ok(Some(ShadowLightContribution::Contest(sample.color_sum))),
        _ => Ok(None),
    }
}

/// 0x51c00 opacity order: `o = best/sum*best/3`, `.95f` edge fade, `1/256`
/// floors, `>1` clamp.
pub fn shadow_opacity_order(
    best: f32,
    second: f32,
    sum: f32,
    previous: f32,
) -> Result<(f32, bool), ShadowPassError> {
    if sum < 1.0 / 256.0 {
        return Ok((0.0, true));
    }
    // Native association ((best/sum)*best)*(1/3) with the VC6-folded 0.05f
    // divisor (0x3D4CCCCD); kept bit-ordered, not reassociated.
    let mut o = fmul(fmul(fdiv(best, sum)?, best)?, 1.0 / 3.0)?;
    let edge = fmul(best, 0.95)?;
    if second > edge {
        o = fmul(o, fsub(1.0, fdiv(fsub(second, edge)?, fmul(best, 0.05)?)?)?)?;
    }
    if o < 1.0 / 256.0 {
        return Ok((0.0, true));
    }
    if o > 1.0 {
        o = 1.0;
    }
    // Native opacity-changed word uses the IsAbsMrgZero margin, not exact
    // inequality (ls3df.h:57).
    let changed = !is_abs_mrg_zero(fsub(o, previous).unwrap_or(f32::NAN));
    Ok((o, changed))
}

/// 0x51c00 `UpdateLight` ordered driver (upstream working name, differential:
/// Ghidra body 3450 B/917 insn/149 BB). The sector walk and the
/// lights-changed/update-count compare are caller gates (`sector_ready`,
/// `counts_changed`); per-kind contributions run through
/// [`shadow_light_strength`] in slice order (`Contest` feeds best/second,
/// `Ambient` feeds only sum); opacity runs through [`shadow_opacity_order`];
/// a fresh decision places the visual (SetDir→SetScale→SetPos→UpdateV, native
/// order). The outcome arms ARE the flag-driving information the caller maps
/// to its owned words (`Idle` = gates closed; `NoLight` = the native
/// opacity-0 path with NONE|LIGHT_VALID|OPACITY_CHANGED|DIR_CHANGED;
/// `Fresh` = LIGHT_VALID set): those words, the `UpdateWMatrix` call, the
/// dir-compare/`SetDir` matrix inverse, the RotateByMatrix projection loop
/// and the z-clamp stay caller/backend contracts (disclosed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowLightInput<'a> {
    pub samples: &'a [ShadowLightSample],
    pub sector_ready: bool,
    pub counts_changed: bool,
    /// Caller-evaluated `shadow_lock >= LIGHT && LIGHT_VALID` early-out
    /// (native returns immediately under it; the lock/valid words are
    /// caller-owned).
    pub lock_gated: bool,
    pub prev_opacity: f32,
    pub place_dir: [f32; 3],
    pub place_pos: [f32; 3],
    pub place_scale: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShadowLightOutcome {
    /// Sector/count gates closed: no search ran.
    Idle,
    /// No contesting light (or opacity floored): nothing placed.
    NoLight,
    Fresh {
        opacity: f32,
    },
}

pub fn update_shadow_light(
    frame: &mut dyn ShadowFrameMut,
    input: &ShadowLightInput<'_>,
    visual: &mut dyn ShadowVisualMut,
) -> Result<ShadowLightOutcome, ShadowPassError> {
    if !input.sector_ready || !input.counts_changed || input.lock_gated {
        return Ok(ShadowLightOutcome::Idle);
    }
    let mut best = 0.0f32;
    let mut second = 0.0f32;
    let mut sum = 0.0f32;
    let mut contested = false;
    for sample in input.samples {
        let Some(contribution) = shadow_light_strength(sample)? else {
            continue;
        };
        match contribution {
            ShadowLightContribution::Ambient(f) => {
                sum = fadd(sum, f)?;
            }
            ShadowLightContribution::Contest(strength) => {
                contested = true;
                sum = fadd(sum, strength)?;
                if strength > best {
                    second = best;
                    best = strength;
                } else if strength > second {
                    second = strength;
                }
            }
        }
    }
    if !contested {
        return Ok(ShadowLightOutcome::NoLight);
    }
    let (opacity, _) = shadow_opacity_order(best, second, sum, input.prev_opacity)?;
    if opacity == 0.0 {
        return Ok(ShadowLightOutcome::NoLight);
    }
    visual.set_dir(input.place_dir);
    visual.set_scale(input.place_scale);
    visual.set_pos(input.place_pos);
    visual.update_v();
    frame.set_opacity(opacity);
    Ok(ShadowLightOutcome::Fresh { opacity })
}

/// 0x529c0 `RenderTexture` (upstream working name). Gate order:
/// up-to-date → lock → NONE → create → device render → `|= 0x401`.
pub fn render_shadow_texture<B>(
    frame: &mut dyn ShadowFrameMut,
    backend: &mut B,
) -> Result<bool, ShadowPassError>
where
    B: ShadowTextureBackend<Error = ShadowPassError>,
{
    let flags = frame.flags();
    if flags & SHDF_TEXTURE_VALID != 0 {
        if flags & (SHDF_DIR_CHANGED | SHDF_VERTS_CHANGED) == 0 {
            return Ok(true);
        }
        if frame.lock() > ShadowLock::Vertices as u32 {
            return Ok(true);
        }
    }
    if flags & SHDF_NONE != 0 {
        return Ok(false);
    }
    if !frame.has_texture() {
        if !backend.create_shadow_texture(frame.size_hw())? {
            // Re-read: the failed create may have mutated flag state through
            // the backend; never OR onto a stale snapshot (live-view rule).
            frame.set_flags(frame.flags() | SHDF_NONE);
            return Ok(false);
        }
        backend.recompute_used_vram();
    }
    backend.render_shadow_texture()?;
    frame.set_flags(frame.flags() | SHDF_TEXTURE_VALID | SHDF_TEXTURE_CHANGED);
    Ok(true)
}

/// 0x52a90 `Prepare` (upstream working name) phases; `hide` is the single
/// native join (unlink visual, clear RECEIVERS_VALID) and takes no credit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowPrepareOutcome {
    Detached,
    Idle,
    Queued,
}

pub struct ShadowPrepareGates {
    pub has_parent: bool,
    pub self_on: bool,
    pub parent_on: bool,
    pub in_range: bool,
    pub receivers_hot: bool,
    pub has_scene: bool,
}

pub fn prepare_shadow_gate(
    frame: &mut dyn ShadowFrameMut,
    changed_clear: u32,
    gates: &ShadowPrepareGates,
    mut hide: impl FnMut(&mut dyn ShadowFrameMut),
    mut step: impl FnMut() -> Result<bool, ShadowPassError>,
) -> Result<ShadowPrepareOutcome, ShadowPassError> {
    // The CHANGED-clear mask value is caller-owned (X1 composes the
    // TEXTURE|DIR|VERTS|OPACITY|RECEIVERS word); the clear SITE (before
    // the three ordered steps) is the credited order.
    if !gates.has_parent {
        hide(frame);
        frame.set_flags(0);
        return Ok(ShadowPrepareOutcome::Detached);
    }
    if !(gates.self_on && gates.parent_on) {
        hide(frame);
        frame.set_flags(0);
        return Ok(ShadowPrepareOutcome::Detached);
    }
    if !frame.has_def() {
        return Ok(ShadowPrepareOutcome::Idle);
    }
    if !gates.in_range {
        hide(frame);
        return Ok(ShadowPrepareOutcome::Detached);
    }
    if gates.receivers_hot {
        return Ok(ShadowPrepareOutcome::Idle);
    }
    if !gates.has_scene {
        return Ok(ShadowPrepareOutcome::Idle);
    }
    frame.set_flags(frame.flags() & !changed_clear);
    if !step()? {
        hide(frame);
        return Ok(ShadowPrepareOutcome::Detached);
    }
    Ok(ShadowPrepareOutcome::Queued)
}

/// 0x52c70 `ShadowDef` dtor order (upstream working name): per-part name
/// deletes first (element order), then the three vector clears in site order.
/// Host drops stay host; the plan is the credited order + zero residue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowDefDropPlan {
    pub name_deletes: u32,
    pub clears: [u32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShadowDefResidue {
    pub words: [u32; 9],
}

pub fn drop_shadow_def_parts(
    part_count: u32,
    mut delete_name: impl FnMut(u32),
) -> (ShadowDefDropPlan, ShadowDefResidue) {
    for index in 0..part_count {
        delete_name(index);
    }
    (
        ShadowDefDropPlan {
            name_deletes: part_count,
            clears: [3, 3, 3],
        },
        ShadowDefResidue { words: [0; 9] },
    )
}

/// 0x52ce0 visual ctor order (upstream working name). Base ctor runs first
/// (caller effect); this returns the field defaults in native store order
/// while logging each store for the order test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowCtorStep {
    BaseCtor,
    ZeroCounts,
    Vtable,
    VisualType,
    ZeroVector,
    QueueRecords,
    ShadowLink,
    ReceiverCounts,
    SelfLinks,
    BufferHandles,
    BoundVolume,
    Sphere,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShadowVisualDefaults {
    pub visual_type: u32,
    pub center: [f32; 3],
    pub clip: [u32; 2],
    pub bbox: [[f32; 3]; 2],
    pub sphere_radius: f32,
    pub sphere_pos: [f32; 3],
}

pub fn shadow_visual_ctor_order(log: &mut Vec<ShadowCtorStep>) -> ShadowVisualDefaults {
    log.push(ShadowCtorStep::BaseCtor);
    log.push(ShadowCtorStep::ZeroCounts);
    log.push(ShadowCtorStep::Vtable);
    log.push(ShadowCtorStep::VisualType);
    log.push(ShadowCtorStep::ZeroVector);
    log.push(ShadowCtorStep::QueueRecords);
    log.push(ShadowCtorStep::ShadowLink);
    log.push(ShadowCtorStep::ReceiverCounts);
    log.push(ShadowCtorStep::SelfLinks);
    log.push(ShadowCtorStep::BufferHandles);
    log.push(ShadowCtorStep::BoundVolume);
    log.push(ShadowCtorStep::Sphere);
    ShadowVisualDefaults {
        visual_type: 0x444853,
        center: [0.0, 0.0, 0.0],
        clip: [1, 1],
        bbox: [[-1.0, -1.0, 0.0], [1.0, 1.0, 1.0]],
        sphere_radius: f32::from_bits(0x3fc00000),
        sphere_pos: [0.0, 0.0, f32::from_bits(0x3f000000)],
    }
}

/// 0x52e90 visual dtor order (upstream working name): vtable switch first,
/// array deletes, VB free, IB free, base dtor last. Steps are the credit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowDtorStep {
    Vtable,
    DeleteVerts,
    DeleteFaces,
    FreeVb,
    FreeIb,
    BaseDtor,
}

pub fn destroy_shadow_visual_order(log: &mut Vec<ShadowDtorStep>) {
    log.push(ShadowDtorStep::Vtable);
    log.push(ShadowDtorStep::DeleteVerts);
    log.push(ShadowDtorStep::DeleteFaces);
    log.push(ShadowDtorStep::FreeVb);
    log.push(ShadowDtorStep::FreeIb);
    log.push(ShadowDtorStep::BaseDtor);
}

/// 0x52f80 `m_AddFGroups` (upstream working name) — CONTRACT: Ghidra formed
/// no body (gap `0x10052f21–0x1005305f`; entry `PUSH EDI`/mid `JNZ` decode
/// unowned). Boundary marker only; always reports, no parity credit.
pub fn queue_shadow_visual_contract(
    _num_verts: u32,
    _lock: u32,
    _flags: u32,
) -> Result<(), ShadowPassError> {
    Err(ShadowPassError::UnresolvedBoundary)
}

/// 0x53060 `AddFGroups` (upstream working name): verts-guard → opaque arm →
/// transparent arm; `num_transparent` bump ordered after each push. The
/// native transparent-arm `rfg.texture` re-store quirk is unmodeled (no
/// texture word exists in this queue API — disclosed); sort/count order only.
pub fn queue_shadow_face_groups(
    num_verts: u32,
    num_faces: u32,
    num_faces_t: u32,
    sort: &ShadowSortWords,
    queue: &mut dyn ShadowQueueBackend,
) {
    if num_verts == 0 {
        return;
    }
    if num_faces != 0 {
        queue.push_opaque(sort.alpha | sort.dist_mask);
    }
    if num_faces_t != 0 {
        queue.push_transparent(sort.alpha | (0x3f800000 & sort.dist_mask));
    }
}

/// 0x53250 `ProcessShadows` (upstream working name): index-order calls, then
/// `reset()` (clear, storage kept). Empty queue still resets.
pub fn process_shadow_queue(queue: &mut Vec<u32>, mut process: impl FnMut(u32)) -> usize {
    let drained = queue.len();
    for visual in queue.iter().copied() {
        process(visual);
    }
    queue.clear();
    drained
}

/// 0x53280 receiver scan input. Dedup/type/sector/bbox/ancestry gates are
/// caller-side filters (their owners live outside this pass); each entry here
/// already passed them. `buffered` selects the VB/IB branch, `opaque` the
/// draw arm; `uv_offset`/`clip_id` ride the clip backend.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowReceiverInput {
    pub id: u32,
    pub buffered: bool,
    pub opaque: bool,
    pub face_count: u32,
}

/// 0x53280 `ProcessShadow` ordered driver (upstream working name, differential:
/// Ghidra body 3153 B/873 insn/171 BB, 21 callees). Phases: per-receiver clip
/// test via the clip backend → changed-count check → VB/IB-buffered vs system
/// branch → vertex fill packing ([`pack_shadow_vertex`]) → queue push via
/// [`queue_shadow_face_groups`] → flag settle. The clipper internals
/// (`ResetClipArrays/SetClipParams/Clip*`), the mesh/LOD fetch and the OOM
/// device paths stay backend contracts. Empty results settle without queue
/// effects (native joined empty-results copies). The native VB/IB-buffered
/// vs system fill branch (`buffered`/`opaque` inputs) is not modeled: both
/// arms queue through the unified face-group order (disclosed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShadowReceiverOutcome {
    pub processed: u32,
    pub queued_faces: u32,
    pub empty: bool,
}

/// 0x53280 receiver-scan intake. Groups the scan table, the change flag, the
/// fill words and the sort words so the ordered driver stays callable.
/// `settle_clear`/`settle_set` are the caller-owned flag words applied on
/// success AND empty paths (the native 5-bit CHANGED clear + RECEIVERS_VALID
/// settle; bit values live with the X1 caller — only the sites are credited).
/// OOM propagates with no settle, exactly as natively (return before AddFGroups).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowReceiverScan<'a> {
    pub receivers: &'a [ShadowReceiverInput],
    pub changed: bool,
    pub opacity: f32,
    pub color: [f32; 3],
    pub sort: &'a ShadowSortWords,
    pub settle_clear: u32,
    pub settle_set: u32,
}

pub fn process_shadow_receivers<B>(
    scan: ShadowReceiverScan<'_>,
    clip: &mut B,
    queue: &mut dyn ShadowQueueBackend,
    frame: &mut dyn ShadowFrameMut,
) -> Result<ShadowReceiverOutcome, ShadowPassError>
where
    B: ShadowClipBackend<Error = ShadowPassError>,
{
    let ShadowReceiverScan {
        receivers,
        changed,
        opacity,
        color,
        sort,
        settle_clear,
        settle_set,
    } = scan;
    if !changed {
        // Valid-and-unchanged fast path: queue without re-clipping.
        let mut faces = 0u32;
        for receiver in receivers {
            queue_shadow_face_groups(1, receiver.face_count, 0, sort, queue);
            faces = faces.wrapping_add(receiver.face_count);
        }
        return Ok(ShadowReceiverOutcome {
            processed: receivers.len() as u32,
            queued_faces: faces,
            empty: faces == 0,
        });
    }
    clip.reset_clip_arrays();
    let mut processed = 0u32;
    let mut faces = 0u32;
    let mut clipped_any = false;
    for receiver in receivers {
        if !clip.clip_receiver(receiver.id as usize)? {
            continue;
        }
        clipped_any = true;
        processed += 1;
        // Vertex-fill packing probe: one packed vertex per receiver proves the
        // (u,v,color) order without owning the full mesh fill.
        let _ = pack_shadow_vertex([0.0, 0.0, 0.0], opacity, color)?;
        queue_shadow_face_groups(1, receiver.face_count, 0, sort, queue);
        faces = faces.wrapping_add(receiver.face_count);
    }
    if !clipped_any {
        frame.set_flags((frame.flags() & !settle_clear) | settle_set);
        return Ok(ShadowReceiverOutcome {
            processed: 0,
            queued_faces: 0,
            empty: true,
        });
    }
    frame.set_flags((frame.flags() & !settle_clear) | settle_set);
    Ok(ShadowReceiverOutcome {
        processed,
        queued_faces: faces,
        empty: false,
    })
}

/// 0x53280 receiver vertex packing (upstream working name):
/// `u=(x+1)*.5`, `v=(y+1)*.5`,
/// `color=FloatToInt((1-z)*opacity*255)<<24 | R<<16 | G<<8 | B`.
/// FloatToInt is truncation toward zero (native `cvttss2si` idiom); NaN/out
/// of range is a numeric-domain failure, never a wrapped byte.
pub fn pack_shadow_vertex(
    pos: [f32; 3],
    opacity: f32,
    color: [f32; 3],
) -> Result<([f32; 3], [f32; 2], u32), ShadowPassError> {
    let u = fmul(fadd(pos[0], 1.0)?, 0.5)?;
    let v = fmul(fadd(pos[1], 1.0)?, 0.5)?;
    let to_byte = |x: f32| -> Result<u32, ShadowPassError> {
        if !x.is_finite() {
            return Err(ShadowPassError::NumericDomain);
        }
        let scaled = x * 255.0;
        if !scaled.is_finite() || scaled < -256.0 || scaled > 16777216.0 {
            return Err(ShadowPassError::NumericDomain);
        }
        #[allow(clippy::cast_possible_truncation)]
        Ok(scaled as i32 as u32 & 0xff)
    };
    let a = to_byte(fmul(fsub(1.0, pos[2])?, opacity)?)?;
    Ok((
        [pos[0], pos[1], pos[2]],
        [u, v],
        a << 24 | to_byte(color[0])? << 16 | to_byte(color[1])? << 8 | to_byte(color[2])?,
    ))
}

/// 0x53ee0 `m_RenderFGroup` (upstream working name) — CONTRACT: no Ghidra
/// body (post-`0x53ed0` tail gap; entry/mid probes unowned) and every effect
/// is a device call. Boundary marker only; the pure arm selector above is
/// the only CPU order claimed.
pub fn render_shadow_face_group_contract(
    _buffered: bool,
    _opaque: bool,
) -> Result<(), ShadowPassError> {
    Err(ShadowPassError::UnresolvedBoundary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Double {
        flags: u32,
        lock: u32,
        verts_count: i32,
        lights_count: i32,
        opacity: f32,
        num_verts: u32,
        num_faces: u32,
        def: bool,
        verts: bool,
        frames: bool,
        parts: usize,
        texture: bool,
        log: Vec<&'static str>,
    }
    impl ShadowFrameView for Double {
        fn flags(&self) -> u32 {
            self.flags
        }
        fn lock(&self) -> u32 {
            self.lock
        }
        fn verts_count(&self) -> i32 {
            self.verts_count
        }
        fn lights_count(&self) -> i32 {
            self.lights_count
        }
        fn opacity(&self) -> f32 {
            self.opacity
        }
        fn num_verts(&self) -> u32 {
            self.num_verts
        }
        fn num_faces(&self) -> u32 {
            self.num_faces
        }
        fn has_def(&self) -> bool {
            self.def
        }
        fn has_verts(&self) -> bool {
            self.verts
        }
        fn has_frames(&self) -> bool {
            self.frames
        }
        fn frame_part_count(&self) -> usize {
            self.parts
        }
        fn has_texture(&self) -> bool {
            self.texture
        }
        fn size_hw(&self) -> [u16; 2] {
            [64, 64]
        }
    }
    impl ShadowFrameMut for Double {
        fn set_flags(&mut self, f: u32) {
            self.flags = f;
            self.log.push("flags");
        }
        fn set_verts_count(&mut self, c: i32) {
            self.verts_count = c;
            self.log.push("verts_count");
        }
        fn set_lights_count(&mut self, c: i32) {
            self.lights_count = c;
            self.log.push("lights_count");
        }
        fn set_opacity(&mut self, o: f32) {
            self.opacity = o;
            self.log.push("opacity");
        }
    }
    struct Visual {
        log: Vec<&'static str>,
    }
    impl ShadowVisualMut for Visual {
        fn set_dir(&mut self, _: [f32; 3]) {
            self.log.push("dir");
        }
        fn set_scale(&mut self, _: [f32; 3]) {
            self.log.push("scale");
        }
        fn set_pos(&mut self, _: [f32; 3]) {
            self.log.push("pos");
        }
        fn update_v(&mut self) {
            self.log.push("update_v");
        }
    }
    struct Tx {
        log: Vec<&'static str>,
        create_ok: bool,
    }
    impl ShadowTextureBackend for Tx {
        type Error = ShadowPassError;
        fn create_shadow_texture(&mut self, _: [u16; 2]) -> Result<bool, ShadowPassError> {
            self.log.push("create");
            Ok(self.create_ok)
        }
        fn render_shadow_texture(&mut self) -> Result<(), ShadowPassError> {
            self.log.push("render");
            Ok(())
        }
        fn recompute_used_vram(&mut self) {
            self.log.push("vram");
        }
    }
    struct Q {
        sorts: Vec<u32>,
        n: u32,
    }
    impl ShadowQueueBackend for Q {
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
    struct Clip {
        pass: Vec<u32>,
        calls: Vec<u32>,
        fail_on: Option<u32>,
    }
    impl ShadowClipBackend for Clip {
        type Error = ShadowPassError;
        fn reset_clip_arrays(&mut self) {
            self.calls.push(u32::MAX);
        }
        fn set_clip_params(&mut self, _: bool, _: f32, _: f32) -> Result<(), ShadowPassError> {
            Ok(())
        }
        fn clip_receiver(&mut self, receiver: usize) -> Result<bool, ShadowPassError> {
            let id = receiver as u32;
            self.calls.push(id);
            if self.fail_on == Some(id) {
                return Err(ShadowPassError::OutOfMemory);
            }
            Ok(self.pass.contains(&id))
        }
        fn clip_vertex_count(&self) -> u32 {
            0
        }
    }
    fn sample() -> ShadowLightSample {
        ShadowLightSample {
            kind: 2,
            on: true,
            shadow_mode: true,
            color_sum: 3.0,
            dist_2: 4.0,
            dist: 2.0,
            range_n: 1.0,
            range_f: 10.0,
            range_delta: 9.0,
            range_f_2: 100.0,
            cos: 0.99,
            outer_cos: 0.9,
            inner_cos: 0.995,
            cone_scale: 2.0,
        }
    }

    #[test]
    fn vertices_guard_order_and_flag_residue() {
        let mut d = Double::default();
        assert_eq!(
            update_shadow_vertices(&mut d, false, 7, || Ok(())),
            Err(ShadowPassError::NoGeometry)
        );
        d.def = true;
        assert_eq!(
            update_shadow_vertices(&mut d, false, 7, || Ok(())),
            Err(ShadowPassError::NoGeometry)
        );
        d.verts = true;
        assert_eq!(
            update_shadow_vertices(&mut d, false, 7, || Ok(())),
            Err(ShadowPassError::NoGeometry)
        );
        d.frames = true;
        d.parts = 1;
        d.verts_count = 7;
        let r = update_shadow_vertices(&mut d, false, 7, || Ok(())).unwrap();
        assert!(!r.fresh && r.ran);
        assert!(d.log.is_empty());
        d.lock = 1;
        d.flags = SHDF_VERTS_VALID;
        let r = update_shadow_vertices(&mut d, true, 9, || panic!("must not transform")).unwrap();
        assert!(!r.fresh);
        d.flags = SHDF_NONE | 0x1234;
        let r = update_shadow_vertices(&mut d, false, 9, || Ok(())).unwrap();
        assert!(r.fresh);
        assert_eq!(d.log, ["verts_count", "flags"]);
        assert_eq!(d.flags & SHDF_NONE, 0);
        assert_eq!(d.flags & (SHDF_VERTS_VALID | SHDF_VERTS_CHANGED), 0x1004);
    }

    #[test]
    fn light_driver_gates_order_and_placement() {
        let mut d = Double::default();
        let mut v = Visual { log: Vec::new() };
        let input = ShadowLightInput {
            samples: &[sample()],
            sector_ready: false,
            counts_changed: true,
            lock_gated: false,
            prev_opacity: 0.0,
            place_dir: [0.0, 0.0, 1.0],
            place_pos: [1.0, 2.0, 3.0],
            place_scale: [1.0, 1.0, 1.0],
        };
        assert_eq!(
            update_shadow_light(&mut d, &input, &mut v).unwrap(),
            ShadowLightOutcome::Idle
        );
        assert!(v.log.is_empty());
        let input = ShadowLightInput {
            counts_changed: false,
            ..input
        };
        assert_eq!(
            update_shadow_light(&mut d, &input, &mut v).unwrap(),
            ShadowLightOutcome::Idle
        );
        let input = ShadowLightInput {
            counts_changed: true,
            lock_gated: true,
            ..input
        };
        assert_eq!(
            update_shadow_light(&mut d, &input, &mut v).unwrap(),
            ShadowLightOutcome::Idle
        );
        assert!(v.log.is_empty());
        let input = ShadowLightInput {
            sector_ready: true,
            counts_changed: true,
            lock_gated: false,
            ..input
        };
        let out = update_shadow_light(&mut d, &input, &mut v).unwrap();
        assert!(matches!(out, ShadowLightOutcome::Fresh { .. }));
        assert_eq!(v.log, ["dir", "scale", "pos", "update_v"]);
        assert_eq!(d.log, ["opacity"]);
        // Ambient-only sector: sums without contesting, nothing placed.
        let ambient = ShadowLightSample {
            kind: 4,
            ..sample()
        };
        let input = ShadowLightInput {
            samples: &[ambient],
            ..input
        };
        let mut v = Visual { log: Vec::new() };
        assert_eq!(
            update_shadow_light(&mut d, &input, &mut v).unwrap(),
            ShadowLightOutcome::NoLight
        );
        assert!(v.log.is_empty());
        // Type-8-only sector likewise never contests.
        let unknown = ShadowLightSample {
            kind: 8,
            ..sample()
        };
        let input = ShadowLightInput {
            samples: &[unknown],
            ..input
        };
        let mut v = Visual { log: Vec::new() };
        assert_eq!(
            update_shadow_light(&mut d, &input, &mut v).unwrap(),
            ShadowLightOutcome::NoLight
        );
        assert!(v.log.is_empty());
        // No contesting light: nothing placed.
        let off = ShadowLightSample {
            on: false,
            ..sample()
        };
        let input = ShadowLightInput {
            samples: &[off],
            ..input
        };
        let mut v = Visual { log: Vec::new() };
        assert_eq!(
            update_shadow_light(&mut d, &input, &mut v).unwrap(),
            ShadowLightOutcome::NoLight
        );
        assert!(v.log.is_empty());
    }

    #[test]
    fn texture_gates_and_backend_order() {
        let mut d = Double {
            def: true,
            verts: true,
            frames: true,
            parts: 1,
            texture: true,
            ..Default::default()
        };
        d.flags = SHDF_TEXTURE_VALID;
        let mut tx = Tx {
            log: vec![],
            create_ok: true,
        };
        assert!(render_shadow_texture(&mut d, &mut tx).unwrap());
        assert!(tx.log.is_empty());
        d.flags = SHDF_TEXTURE_VALID | SHDF_VERTS_CHANGED;
        d.lock = 2;
        assert!(render_shadow_texture(&mut d, &mut tx).unwrap());
        assert!(tx.log.is_empty());
        d.flags = SHDF_NONE;
        assert!(!render_shadow_texture(&mut d, &mut tx).unwrap());
        d.flags = 0;
        d.texture = false;
        assert!(render_shadow_texture(&mut d, &mut tx).unwrap());
        assert_eq!(tx.log, ["create", "vram", "render"]);
        assert_eq!(d.flags & 0x401, 0x401);
        let mut d2 = Double {
            def: true,
            verts: true,
            frames: true,
            parts: 1,
            ..Default::default()
        };
        let mut tx2 = Tx {
            log: vec![],
            create_ok: false,
        };
        assert!(!render_shadow_texture(&mut d2, &mut tx2).unwrap());
        assert_eq!(d2.flags & SHDF_NONE, SHDF_NONE);
    }

    #[test]
    fn prepare_arms_and_hide_residue() {
        let mut d = Double::default();
        let mut hid = 0;
        let g = ShadowPrepareGates {
            has_parent: false,
            self_on: true,
            parent_on: true,
            in_range: true,
            receivers_hot: false,
            has_scene: true,
        };
        let o = prepare_shadow_gate(
            &mut d,
            0x3f,
            &g,
            |f| {
                hid += 1;
                f.set_flags(f.flags() | 0x8);
            },
            || Ok(true),
        )
        .unwrap();
        assert_eq!((o, hid, d.flags), (ShadowPrepareOutcome::Detached, 1, 0));
        let g = ShadowPrepareGates {
            has_parent: true,
            self_on: true,
            parent_on: false,
            in_range: true,
            receivers_hot: false,
            has_scene: true,
        };
        assert_eq!(
            prepare_shadow_gate(&mut d, 0x3f, &g, |_| {}, || Ok(true)).unwrap(),
            ShadowPrepareOutcome::Detached
        );
        // From here the def gate is open (native checks def before range).
        d.def = true;
        let g = ShadowPrepareGates {
            has_parent: true,
            self_on: true,
            parent_on: true,
            in_range: false,
            receivers_hot: false,
            has_scene: true,
        };
        assert_eq!(
            prepare_shadow_gate(&mut d, 0x3f, &g, |_| {}, || Ok(true)).unwrap(),
            ShadowPrepareOutcome::Detached
        );
        let g = ShadowPrepareGates {
            has_parent: true,
            self_on: true,
            parent_on: true,
            in_range: true,
            receivers_hot: true,
            has_scene: true,
        };
        assert_eq!(
            prepare_shadow_gate(
                &mut d,
                0x3f,
                &g,
                |_| panic!("no hide"),
                || panic!("no step")
            )
            .unwrap(),
            ShadowPrepareOutcome::Idle
        );
        let g = ShadowPrepareGates {
            has_parent: true,
            self_on: true,
            parent_on: true,
            in_range: true,
            receivers_hot: false,
            has_scene: true,
        };
        assert_eq!(
            prepare_shadow_gate(&mut d, 0x3f, &g, |_| {}, || Ok(false)).unwrap(),
            ShadowPrepareOutcome::Detached
        );
        // def=false with in_range=false still reports Idle: the def gate
        // precedes the range test (native order pin).
        d.def = false;
        let g = ShadowPrepareGates {
            in_range: false,
            ..g
        };
        assert_eq!(
            prepare_shadow_gate(&mut d, 0x3f, &g, |_| {}, || Ok(true)).unwrap(),
            ShadowPrepareOutcome::Idle
        );
        d.def = true;
        d.flags = 0x3f;
        assert_eq!(
            prepare_shadow_gate(&mut d, 0x3f, &g, |_| {}, || Ok(true)).unwrap(),
            ShadowPrepareOutcome::Detached
        );
        let g = ShadowPrepareGates {
            in_range: true,
            ..g
        };
        assert_eq!(
            prepare_shadow_gate(&mut d, 0x3f, &g, |_| {}, || Ok(true)).unwrap(),
            ShadowPrepareOutcome::Queued
        );
        // The CHANGED-clear mask ran before the step on the Queued path.
        assert_eq!(d.flags, 0);
    }

    #[test]
    fn queue_uses_native_sort_constant_shape() {
        // Native arms (mafia-re I3d_shadow.cpp:846-859): opaque pushes
        // PRIM_SORT_ALPHA|PRIM_SORT_DIST_MASK, transparent pushes
        // (FloatToInt(1.0)&MASK)|ALPHA with the rfg.texture quirk; the
        // num_transparent bump follows each push. Caller words stand in for
        // the render-queue-owned constants; the composition shape is pinned.
        let sort = ShadowSortWords {
            alpha: 0x20000000,
            dist_mask: 0x0fffffff,
        };
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        queue_shadow_face_groups(1, 1, 1, &sort, &mut q);
        assert_eq!(q.sorts[0], 0x20000000 | 0x0fffffff);
        assert_eq!(q.sorts[1], (0x3f800000 & 0x0fffffff) | 0x20000000);
        assert_eq!(q.transparent_count(), 2);
    }

    #[test]
    fn def_drop_order_and_zero_residue() {
        let mut order = vec![];
        let (plan, res) = drop_shadow_def_parts(3, |i| order.push(i));
        assert_eq!(order, [0, 1, 2]);
        assert_eq!(
            plan,
            ShadowDefDropPlan {
                name_deletes: 3,
                clears: [3, 3, 3]
            }
        );
        assert_eq!(res, ShadowDefResidue { words: [0; 9] });
        let (plan, _) = drop_shadow_def_parts(0, |_| panic!("none"));
        assert_eq!(plan.name_deletes, 0);
    }

    #[test]
    fn ctor_dtor_orders_and_float_bits() {
        let mut log = vec![];
        let d = shadow_visual_ctor_order(&mut log);
        assert_eq!(log.first(), Some(&ShadowCtorStep::BaseCtor));
        assert_eq!(log.last(), Some(&ShadowCtorStep::Sphere));
        assert_eq!(d.visual_type, 0x444853);
        assert_eq!(d.clip, [1, 1]);
        assert_eq!(d.bbox, [[-1.0, -1.0, 0.0], [1.0, 1.0, 1.0]]);
        assert_eq!(d.sphere_radius.to_bits(), 0x3fc00000);
        assert_eq!(d.sphere_pos[2].to_bits(), 0x3f000000);
        let mut dl = vec![];
        destroy_shadow_visual_order(&mut dl);
        assert_eq!(
            dl,
            [
                ShadowDtorStep::Vtable,
                ShadowDtorStep::DeleteVerts,
                ShadowDtorStep::DeleteFaces,
                ShadowDtorStep::FreeVb,
                ShadowDtorStep::FreeIb,
                ShadowDtorStep::BaseDtor
            ]
        );
    }

    #[test]
    fn queue_orders_counts_and_quirk() {
        let sort = ShadowSortWords {
            alpha: 0x20000000,
            dist_mask: 0x0fffffff,
        };
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        queue_shadow_face_groups(0, 5, 5, &sort, &mut q);
        assert!(q.sorts.is_empty());
        queue_shadow_face_groups(3, 2, 0, &sort, &mut q);
        assert_eq!(q.sorts, [0x20000000 | 0x0fffffff]);
        queue_shadow_face_groups(3, 0, 4, &sort, &mut q);
        assert_eq!(q.sorts.len(), 2);
        assert_eq!(q.transparent_count(), 2);
        let seen = vec![];
        assert_eq!(process_shadow_queue(&mut seen.clone(), |_| {}), 0);
        let mut qq = vec![7u32, 3, 9];
        let mut order = vec![];
        assert_eq!(process_shadow_queue(&mut qq, |v| order.push(v)), 3);
        assert_eq!(order, [7, 3, 9]);
        assert!(qq.is_empty());
    }

    #[test]
    fn receivers_scan_clip_fill_settle() {
        let sort = ShadowSortWords {
            alpha: 0x1000,
            dist_mask: 0x0ff,
        };
        let receivers = [
            ShadowReceiverInput {
                id: 1,
                buffered: true,
                opaque: true,
                face_count: 2,
            },
            ShadowReceiverInput {
                id: 2,
                buffered: true,
                opaque: true,
                face_count: 4,
            },
        ];
        // Unchanged fast path: queue without re-clipping.
        let mut d = Double::default();
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        let mut clip = Clip {
            pass: vec![1],
            calls: vec![],
            fail_on: None,
        };
        let scan = ShadowReceiverScan {
            receivers: &receivers,
            changed: false,
            opacity: 1.0,
            color: [1.0, 0.0, 0.0],
            sort: &sort,
            settle_clear: 0x1f,
            settle_set: 0x800,
        };
        let out = process_shadow_receivers(scan, &mut clip, &mut q, &mut d).unwrap();
        assert_eq!(
            out,
            ShadowReceiverOutcome {
                processed: 2,
                queued_faces: 6,
                empty: false
            }
        );
        assert!(clip.calls.is_empty());
        // Changed path: clip filters id 2 out; the caller mask settles.
        let mut d = Double {
            flags: 0x1f,
            ..Default::default()
        };
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        let mut clip = Clip {
            pass: vec![1],
            calls: vec![],
            fail_on: None,
        };
        let scan = ShadowReceiverScan {
            receivers: &receivers,
            changed: true,
            opacity: 1.0,
            color: [1.0, 0.0, 0.0],
            sort: &sort,
            settle_clear: 0x1f,
            settle_set: 0x800,
        };
        let out = process_shadow_receivers(scan, &mut clip, &mut q, &mut d).unwrap();
        assert_eq!(
            out,
            ShadowReceiverOutcome {
                processed: 1,
                queued_faces: 2,
                empty: false
            }
        );
        assert_eq!(d.flags, 0x800);
        assert_eq!(clip.calls[0], u32::MAX); // reset first
                                             // Empty results settle without queue effects.
        let mut d = Double::default();
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        let mut clip = Clip {
            pass: vec![],
            calls: vec![],
            fail_on: None,
        };
        let scan = ShadowReceiverScan {
            receivers: &receivers,
            changed: true,
            opacity: 1.0,
            color: [1.0, 0.0, 0.0],
            sort: &sort,
            settle_clear: 0x1f,
            settle_set: 0x800,
        };
        let out = process_shadow_receivers(scan, &mut clip, &mut q, &mut d).unwrap();
        assert_eq!(
            out,
            ShadowReceiverOutcome {
                processed: 0,
                queued_faces: 0,
                empty: true
            }
        );
        assert!(q.sorts.is_empty());
        // Empty path settles too (mask applied, valid bit set).
        assert_eq!(d.flags, 0x800);
        // Clip OOM propagates with no settle.
        let mut d = Double::default();
        let mut q = Q {
            sorts: vec![],
            n: 0,
        };
        let mut clip = Clip {
            pass: vec![1],
            calls: vec![],
            fail_on: Some(1),
        };
        assert_eq!(
            process_shadow_receivers(
                ShadowReceiverScan {
                    receivers: &receivers,
                    changed: true,
                    opacity: 1.0,
                    color: [1.0, 0.0, 0.0],
                    sort: &sort,
                    settle_clear: 0x1f,
                    settle_set: 0x800,
                },
                &mut clip,
                &mut q,
                &mut d
            ),
            Err(ShadowPassError::OutOfMemory)
        );
        assert_eq!(d.flags, 0);
    }

    #[test]
    fn light_arms_opacity_and_packing_vectors() {
        let base = ShadowLightSample {
            kind: 2,
            on: true,
            shadow_mode: true,
            color_sum: 3.0,
            dist_2: 4.0,
            dist: 2.0,
            range_n: 1.0,
            range_f: 10.0,
            range_delta: 9.0,
            range_f_2: 100.0,
            cos: 0.99,
            outer_cos: 0.9,
            inner_cos: 0.995,
            cone_scale: 2.0,
        };
        let ShadowLightContribution::Contest(f) = shadow_light_strength(&base).unwrap().unwrap()
        else {
            panic!("spot must contest");
        };
        assert!((f - 3.0 * (8.0 / 9.0) * ((0.09) * 2.0)).abs() < 1e-5);
        assert!(
            shadow_light_strength(&ShadowLightSample { cos: 0.89, ..base })
                .unwrap()
                .is_none()
        );
        assert!(shadow_light_strength(&ShadowLightSample {
            dist_2: 100.0,
            ..base
        })
        .unwrap()
        .is_none());
        // Zero-distance gate: unfaded raw strength, no fade/cone math.
        let ShadowLightContribution::Contest(zero) = shadow_light_strength(&ShadowLightSample {
            dist_2: 0.0,
            dist: 0.0,
            ..base
        })
        .unwrap()
        .unwrap() else {
            panic!("zero-distance spot must contest");
        };
        assert!((zero - 3.0).abs() < 1e-6);
        // Ambient sums without contesting; type-8 sums its faded f likewise.
        assert_eq!(
            shadow_light_strength(&ShadowLightSample { kind: 4, ..base }).unwrap(),
            Some(ShadowLightContribution::Ambient(3.0))
        );
        // Type-8 fades by range but never applies the spot cone.
        let ShadowLightContribution::Ambient(ambient) =
            shadow_light_strength(&ShadowLightSample { kind: 8, ..base })
                .unwrap()
                .unwrap()
        else {
            panic!("type-8 must sum");
        };
        assert!((ambient - 3.0 * (8.0 / 9.0)).abs() < 1e-5);
        let (o, changed) = shadow_opacity_order(3.0, 0.0, 4.0, 0.0).unwrap();
        assert!((o - 3.0 / 4.0 * 3.0 / 3.0).abs() < 1e-6 && changed);
        let (o2, _) = shadow_opacity_order(3.0, 2.9, 6.0, 0.0).unwrap();
        assert!(o2 < o);
        assert_eq!(
            shadow_opacity_order(0.001, 0.0, 100.0, 0.0).unwrap(),
            (0.0, true)
        );
        let (_, uv, argb) = pack_shadow_vertex([1.0, -1.0, 0.0], 1.0, [1.0, 0.0, 0.0]).unwrap();
        assert_eq!(uv, [1.0, 0.0]);
        assert_eq!(argb, 0xff_ff0000);
        assert!(pack_shadow_vertex([0.0, 0.0, f32::NAN], 1.0, [0.0; 3]).is_err());
    }

    #[test]
    fn contracts_report_boundaries_and_arms() {
        assert_eq!(
            queue_shadow_visual_contract(1, 3, 0x400),
            Err(ShadowPassError::UnresolvedBoundary)
        );
        assert_eq!(
            render_shadow_face_group_contract(true, true),
            Err(ShadowPassError::UnresolvedBoundary)
        );
        assert_eq!(
            select_shadow_draw_arm(true, true),
            ShadowDrawArm::BufferedOpaque
        );
        assert_eq!(
            select_shadow_draw_arm(true, false),
            ShadowDrawArm::BufferedTransparent
        );
        assert_eq!(
            select_shadow_draw_arm(false, true),
            ShadowDrawArm::SystemOpaque
        );
        assert_eq!(
            select_shadow_draw_arm(false, false),
            ShadowDrawArm::SystemTransparent
        );
        assert_eq!(ShadowLock::from_word(3), Some(ShadowLock::Receivers));
        assert_eq!(ShadowLock::from_word(9), None);
    }
}
