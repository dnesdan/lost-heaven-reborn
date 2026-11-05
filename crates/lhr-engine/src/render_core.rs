#![forbid(unsafe_code)]
//! Render-driver core: init/close plus the visual queue walk.
//!
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs 0x62010 (`I3D_driver::initRender`), 0x62020 (`closeRender`),
//! 0x65560 (`VisualProc`), 0x65770 (`NextFFrameProc`), 0x658f0
//! (`I3D_driver::BuildAllVBuffers`). Pinned mafia-re ab67b13
//! `ls3df/I3D/render/render.cpp:184-191,736-830`.
//!
//! Contract tier this wave: reviewed upstream source plus Rust tests, no
//! Ghidra body claimed. Float policy: plain f32 ops with single rounding,
//! no fused multiply-add, no f64 substitution. The per-level `sqrt(sqrt())`
//! is two separate f32 square roots in source order.

/// `I3D_OK` (ls3df.h).
pub const I3D_OK: i32 = 0;
/// Frame is visible (I3D.h:259).
pub const FRMFLAGS_ON: u32 = 0x1;
/// Visual must be clipped (I3D.h:280).
pub const FRMFLAGS_CLIP: u32 = 0x200000;
/// Every visual is clipped (I3D.h:4437).
pub const PROBLEM_CLIP_ALWAYS: u32 = 0x2;
/// Queued for the particle renderer (I3D.h:3540).
pub const PARTF_RENDERER: u32 = 0x600;
/// Alpha-blend mask selecting `sort_dist` (I3D.h:3536).
pub const PARTF_ALPHA_MASK: u32 = 0x3;
/// Lens flare met this frame (I3D.h:2979).
pub const LFF_MET: u32 = 0x1;
/// Object with mesh/LODs (I3D.h:2693).
pub const VISF_OBJECT: u32 = 0x800;
/// LOD follows loaded textures (I3D.h:2683).
pub const VISF_TEXTURE_LOD: u32 = 0x10;

/// Particle visual lifetime owned by the driver.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParticleState {
    /// `true` while `pParticleVisual` (render.cpp) is live.
    pub live: bool,
}

/// Device-adjacent particle effects behind a backend trait.
pub trait ParticleBackend {
    /// `InitParticle` (`new I3D_part_renderer`).
    fn init_particle(&mut self);
    /// `CloseParticle` tail (`Release` when live).
    fn close_particle(&mut self);
}

/// LS3DF RVA 0x62020 `I3D_driver::closeRender` (mafia-re
/// `ls3df/I3D/render/render.cpp:193-198`): the `CloseParticle` tail only.
/// A non-live particle issues no device call, mirroring the null check.
pub fn close_render(state: &mut ParticleState, backend: &mut impl ParticleBackend) {
    if state.live {
        backend.close_particle();
        state.live = false;
    }
}

/// LS3DF RVA 0x62010 `I3D_driver::initRender` (mafia-re
/// `ls3df/I3D/render/render.cpp:184-191`): `closeRender` first, then
/// `InitParticle`, returning `I3D_OK`. Order is part of the contract.
pub fn init_render(state: &mut ParticleState, backend: &mut impl ParticleBackend) -> i32 {
    close_render(state, backend);
    backend.init_particle();
    state.live = true;
    I3D_OK
}

/// Original visual-type discriminants (I3D.h:85-98).
pub const VISUAL_PART_BASE: u32 = 0x42545250;
/// Original visual-type discriminant (I3D.h:95).
pub const VISUAL_PART_ELEMENT: u32 = 0x45545250;
/// Original visual-type discriminant (I3D.h:93).
pub const VISUAL_PROJECTOR: u32 = 0x4a4f5250;
/// Original visual-type discriminant (I3D.h:90).
pub const VISUAL_LENS_FLARE: u32 = 0x534e454c;

/// Queue-walk visual kind for the `VisualProc` switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualProcKind {
    PartBase,
    PartElement,
    Projector,
    LensFlare,
    Other,
}

/// Typed live view of one visual for `VisualProc`.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualProcVisual {
    /// `m_pOwner != null`.
    pub owner_present: bool,
    /// `GetVisualType()` switch arm.
    pub kind: VisualProcKind,
    /// `m_FrmFlags` (mutated with `FRMFLAGS_CLIP` only).
    pub frm_flags: u32,
    /// `part_flags` (`PARTF_RENDERER` / `PARTF_ALPHA_MASK`).
    pub part_flags: u32,
    /// `sort_group` for queued particles.
    pub sort_group: u32,
    /// `sort_dist` for queued alpha particles (in-range finite only).
    pub sort_dist: f32,
}

/// Globals read by `VisualProc`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualProcState {
    /// `RenderNumber` stored into the owner.
    pub render_number: u32,
    /// `m_ProblemFlags & PROBLEM_CLIP_ALWAYS != 0`.
    pub problem_clip_always: bool,
    /// `renderState[RS_USEPROJECTORS] != 0`.
    pub use_projectors: bool,
}

/// Device-adjacent queue effects behind a backend trait.
pub trait VisualProcBackend {
    /// `vis->m_pOwner->m_RenderNumber = RenderNumber`.
    fn set_owner_render_number(&mut self, number: u32);
    /// Push to `ParticleElementsList.visuals` with the packed sort value.
    fn queue_particle(&mut self, sort_value: u32);
    /// `vis->m_AddFGroups()`.
    fn add_fgroups(&mut self);
    /// Scene lens-flare list already contains this flare.
    fn lensflare_known(&mut self) -> bool;
    /// Current `timeGetTime()` value (called only for a new flare).
    fn now(&mut self) -> u32;
    /// Register a new flare (`m_RefCount++`, push, timestamps).
    fn register_lensflare(&mut self, now: u32);
    /// `lf_flags |= LFF_MET`.
    fn mark_lensflare_met(&mut self);
}

/// FPU `FloatToInt` (ls3df.h:82-89) for the admitted in-range finite domain:
/// round to nearest even, then truncate. Out-of-range and NaN inputs are
/// outside this contract (native yields `0x80000000`; host saturation
/// differs and is not claimed).
// Same caller horizon as visual_proc above.
#[allow(dead_code)]
fn float_to_int(value: f32) -> i32 {
    value.round_ties_even() as i32
}

/// LS3DF RVA 0x65560 `VisualProc` (mafia-re
/// `ls3df/I3D/render/render.cpp:736-775`). The original is `static`; this
/// port stays crate-private (`pub(crate)`) for the same reason. Effect
/// order: owner render number, clip flag, then the type switch. `PROJECTOR`
/// without `RS_USEPROJECTORS` returns before `m_AddFGroups` (fallthrough to
/// the default arm otherwise).
// No production callers yet (later waves drive the frame lists); the
// pinning tests below exercise it. Remove once wired.
#[allow(dead_code)]
pub(crate) fn visual_proc(
    visual: &mut VisualProcVisual,
    state: &VisualProcState,
    backend: &mut impl VisualProcBackend,
) {
    if visual.owner_present {
        backend.set_owner_render_number(state.render_number);
    }
    if state.problem_clip_always {
        visual.frm_flags |= FRMFLAGS_CLIP;
    }
    match visual.kind {
        VisualProcKind::PartBase | VisualProcKind::PartElement => {
            if (visual.part_flags & PARTF_RENDERER) != 0 {
                let mut sort_value = visual.sort_group << 16;
                if (visual.part_flags & PARTF_ALPHA_MASK) != 0 {
                    sort_value |= float_to_int(visual.sort_dist) as u32;
                }
                backend.queue_particle(sort_value);
            } else {
                backend.add_fgroups();
            }
        }
        VisualProcKind::Projector => {
            if !state.use_projectors {
                return;
            }
            backend.add_fgroups();
        }
        VisualProcKind::Other => {
            backend.add_fgroups();
        }
        VisualProcKind::LensFlare => {
            if !backend.lensflare_known() {
                let now = backend.now();
                backend.register_lensflare(now);
            }
            backend.mark_lensflare_met();
        }
    }
}

/// Mutable `RSLodQuality` / `cam_pos` globals for the fast-frame walk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NextFrameState {
    /// `RSLodQuality`.
    pub rs_lod_quality: f32,
    /// `AddFGHlpStruct.cam_pos`.
    pub cam_pos: [f32; 3],
}

/// Typed frame node: sibling order is slice order (`m_pNext`), children are
/// `m_pChild` recursion. `mesh_dists` is `Some` exactly when the visual is
/// an object with a mesh (`VISF_OBJECT` plus non-null mesh); each entry is
/// that level's `lod_dist`.
#[derive(Clone, Debug, PartialEq)]
pub struct FframeNode {
    /// `m_FrmFlags & FRMFLAGS_ON != 0`.
    pub on: bool,
    /// `m_Type == FRAME_VISUAL`.
    pub is_visual: bool,
    /// `vis_flags` (`VISF_OBJECT` / `VISF_TEXTURE_LOD`).
    pub vis_flags: u32,
    /// `m_WorldScale`.
    pub world_scale: f32,
    /// `GetWBoundVolume().bsphere.pos`.
    pub bound_pos: [f32; 3],
    /// Per-level `lod_dist` values when applicable.
    pub mesh_dists: Option<Vec<f32>>,
    /// `m_pChild` subtree.
    pub children: Vec<FframeNode>,
}

/// Device-adjacent face-group effects behind a backend trait.
pub trait NextFrameBackend {
    /// `vis->m_AddFGroups()`.
    fn add_fgroups(&mut self);
    /// `AddFGHlpStruct.fgroups.reset()`.
    fn reset_fgroups(&mut self);
}

/// LS3DF RVA 0x65770 `NextFFrameProc` (mafia-re
/// `ls3df/I3D/render/render.cpp:777-811`). Sibling iteration with child
/// recursion, gated on `FRMFLAGS_ON`. Object meshes queue once per level as
/// if seen from that level's distance; other visuals queue once. Numeric
/// order per level: `cam_pos = bound`, `RSLodQuality = 1.0 / world_scale`
/// (f32 div), `d = sqrt(sqrt(lod_dist))` (two separate f32 roots, unfused),
/// `cam_pos.z += (d + prev) * 0.5` (f32 add then mul). Empty mesh storage
/// queues nothing (host admission; native would read out of bounds).
pub fn next_fframe_proc(
    siblings: &[FframeNode],
    state: &mut NextFrameState,
    backend: &mut impl NextFrameBackend,
) {
    for node in siblings {
        if !node.on {
            continue;
        }
        if node.is_visual {
            if (node.vis_flags & VISF_OBJECT) != 0 {
                if let Some(dists) = node.mesh_dists.as_ref() {
                    if !dists.is_empty() {
                        let mut count = dists.len();
                        if (node.vis_flags & VISF_TEXTURE_LOD) != 0 {
                            count = 1;
                        }
                        if count > dists.len() {
                            count = dists.len();
                        }
                        let mut prev = 0.0f32;
                        for index in 0..count {
                            state.cam_pos = node.bound_pos;
                            state.rs_lod_quality = 1.0f32 / node.world_scale;
                            let inner = dists[index].sqrt();
                            let d = inner.sqrt();
                            let half = (d + prev) * 0.5f32;
                            state.cam_pos[2] = state.cam_pos[2] + half;
                            backend.add_fgroups();
                            prev = d;
                        }
                    }
                } else {
                    state.cam_pos = node.bound_pos;
                    state.rs_lod_quality = 1.0f32 / node.world_scale;
                    backend.add_fgroups();
                }
            } else {
                state.cam_pos = node.bound_pos;
                state.rs_lod_quality = 1.0f32 / node.world_scale;
                backend.add_fgroups();
            }
            backend.reset_fgroups();
        }
        next_fframe_proc(&node.children, state, backend);
    }
}

/// Four sector child lists queued by `BuildAllVBuffers`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuildScene {
    /// `primary_sector->m_pChild`.
    pub primary: Vec<FframeNode>,
    /// `omni_sector->m_pChild`.
    pub omni: Vec<FframeNode>,
    /// `backdrop_sector->m_pChild`.
    pub backdrop: Vec<FframeNode>,
    /// `near_sector->m_pChild`.
    pub near: Vec<FframeNode>,
}

/// Full build effects: frame create/release plus the queue walk.
pub trait BuildBackend: NextFrameBackend {
    /// `CreateFrame(FRAME_SECTOR)`.
    fn create_sector_frame(&mut self);
    /// `frm->Release()`.
    fn release_sector_frame(&mut self);
    /// `AddFGHlpStruct.Clear()`.
    fn clear_fg(&mut self);
    /// `AddFGHlpStruct.scene = scene`.
    fn set_scene(&mut self);
    /// `AddFGHlpStruct.sector = (I3D_sector *)frm`.
    fn set_sector_temp(&mut self);
    /// `AddFGHlpStruct.camera = scene->current_camera`.
    fn set_camera(&mut self);
}

/// LS3DF RVA 0x658f0 `I3D_driver::BuildAllVBuffers` (mafia-re
/// `ls3df/I3D/render/render.cpp:813-830`). Order-exact: create the temporary
/// sector frame, clear, set scene/sector/camera, queue primary, omni,
/// backdrop and near children in that order, release, clear again.
pub fn build_all_vbuffers(
    scene: &BuildScene,
    state: &mut NextFrameState,
    backend: &mut impl BuildBackend,
) {
    backend.create_sector_frame();
    backend.clear_fg();
    backend.set_scene();
    backend.set_sector_temp();
    backend.set_camera();
    next_fframe_proc(&scene.primary, state, backend);
    next_fframe_proc(&scene.omni, state, backend);
    next_fframe_proc(&scene.backdrop, state, backend);
    next_fframe_proc(&scene.near, state, backend);
    backend.release_sector_frame();
    backend.clear_fg();
}

/// `I3DERR_GENERIC` (ls3df.h:26).
pub const I3DERR_GENERIC: i32 = 0x8000_0005u32 as i32;
/// `I3DERR_NOTINITIALIZED` (ls3df.h:30).
pub const I3DERR_NOTINITIALIZED: i32 = 0x8000_0009u32 as i32;

/// FPU status-word error-summary bit tested on `Render` entry
/// (render.cpp:228-232, `fpu & 0x80`).
pub const FPU_EXCEPTION_SUMMARY: u16 = 0x80;
/// FPU control-word mask applied on `Render` entry (render.cpp:233-235,
/// `fpu &= 0xf0ff`): single precision, round to nearest.
pub const FPU_ENTRY_CONTROL_MASK: u16 = 0xf0ff;

/// Mirrors `fpu & 0x80 != 0` on the `fnstsw` word.
pub fn fpu_has_exception(status: u16) -> bool {
    status & FPU_EXCEPTION_SUMMARY != 0
}

/// Mirrors `fpu &= 0xf0ff` on the `fnstcw` word.
pub fn fpu_entry_control(control: u16) -> u16 {
    control & FPU_ENTRY_CONTROL_MASK
}

/// Scene viewport in absolute bounds (`I3D_scene::viewport`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneViewport {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Cached device viewport in desired form (`m_D3DViewPort8` after `CopyRect`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Typed live view of the scene words read and written by `Render`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderSceneInput {
    /// Absolute viewport bounds.
    pub viewport: SceneViewport,
    /// `scene->current_camera != null`.
    pub has_camera: bool,
    /// `scene->m_RenderNumber`, incremented with wrapping on entry.
    pub render_number: u32,
}

/// Driver words read and written by `Render` (`I3DIsInit`, `rendering`,
/// `renderCB`, `m_D3DViewPort8`, `RenderNumber`/`RenderNumberMinus1`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderDriverState {
    /// `I3DIsInit`.
    pub is_init: bool,
    /// `rendering`.
    pub rendering: bool,
    /// `renderCB`.
    pub render_cb: bool,
    /// `m_D3DViewPort8` cache in desired form.
    pub viewport: RenderRect,
    /// `RenderNumber`.
    pub render_number: u32,
    /// `RenderNumberMinus1`.
    pub render_number_minus1: u32,
}

/// Builds the `RECT rc` of render.cpp:270-272: left and top pass through,
/// right and bottom carry width and height with wrapping subtraction.
pub fn scene_desired_rect(viewport: &SceneViewport) -> RenderRect {
    RenderRect {
        left: viewport.left,
        top: viewport.top,
        right: viewport.right.wrapping_sub(viewport.left),
        bottom: viewport.bottom.wrapping_sub(viewport.top),
    }
}

/// Mirrors `EqualRect(&rc, &m_D3DViewPort8)`.
pub fn render_rect_equal(a: &RenderRect, b: &RenderRect) -> bool {
    a == b
}

/// Device-adjacent full-scene effects behind a backend trait.
pub trait RenderBackend {
    /// `fnstsw` word on entry.
    fn fpu_status(&mut self) -> u16;
    /// `fnstcw` word on entry.
    fn fpu_control(&mut self) -> u16;
    /// `dbgPrintf` when the exception summary bit is set.
    fn report_fpu_exception(&mut self, status: u16);
    /// `fldcw` of the masked control word.
    fn apply_fpu_control(&mut self, masked: u16);
    /// `SetViewport(&m_D3DViewPort8)`; `false` is `FAILED`.
    fn set_viewport(&mut self, rect: &RenderRect) -> bool;
    /// `ResetRenderProps` plus `Clear` on the no-camera path.
    fn clear_no_camera(&mut self);
    /// `timeGetTime()` for the entry timestamp (render.cpp:322, read only
    /// when `!renderCB`); the value is kept out of pure state.
    fn now(&mut self) -> u32;
    /// Per-flare `timeGetTime()` collapsed to one hook (render.cpp:770 reads
    /// it per newly registered flare inside the visual walk; the host
    /// backend fans it out as needed). Runs regardless of `renderCB` like
    /// the native flare loop.
    fn lensflare_now(&mut self) -> u32;
    /// `I3D_frame::UpdateWMatrixProc`.
    fn update_wmatrix_proc(&mut self);
    /// `I3D_visual::UpdateWBoundProc`.
    fn update_wbound_proc(&mut self);
    /// `RadixSort32` queue sort.
    fn radix_sort(&mut self);
    /// `VisualProc` queue walk.
    fn visual_proc(&mut self);
    /// `NextFFrameProc` queue walk.
    fn next_fframe_proc(&mut self);
    /// `I3D_driver::BuildAllVBuffers` queue walk.
    fn build_all_vbuffers(&mut self);
}

/// LS3DF RVA 0x62030 `I3D_driver::Render` (mafia-re
/// `ls3df/I3D/render/render.cpp:220`).
/// Contract tier, partial coverage (0x2b830 standard): device-adjacent
/// scene pass, no original-data round-trip. The evidenced-head claim covers
/// only the entry prefix — null scene, uninit, FPU status/control,
/// reentrancy guard, render-number increment, no-camera clear, and the
/// `EqualRect`/`CopyRect` viewport path with `SetViewport` failure. The six
/// backend stages are host-grouped collapse points in a host-chosen order,
/// NOT the native call sequence (native interleaves its own sector passes;
/// `NextFFrameProc` belongs to `BuildAllVBuffers`, which `Render` never
/// calls directly). The entry timestamp reads only when `!renderCB`
/// (render.cpp:322); the flare timestamp hook covers the per-flare reads.
/// Omitted tail (all backend-owned, disclosed): `Clear` color (`bg_color`
/// words have no host carrier), `BeginScene`/`EndScene`, PRE/POST scene and
/// render callbacks, FOV/listener work, camera `UpdateSector`, area/sector/
/// sound passes, shadow/portal/occluder passes, backdrop/mirror/landscape/
/// weather/particle/flare device passes, `pCurrentScene` nulling, and the
/// reentrancy `dbgPrintf` (no diagnostic hook). `rendering` clears only when
/// not inside the scene callback, matching `if (!renderCB) rendering = false`.
pub fn render_scene(
    scene: Option<&mut RenderSceneInput>,
    state: &mut RenderDriverState,
    backend: &mut impl RenderBackend,
) -> i32 {
    let scene = match scene {
        Some(scene) => scene,
        None => return I3DERR_GENERIC,
    };
    if !state.is_init {
        return I3DERR_NOTINITIALIZED;
    }
    let status = backend.fpu_status();
    if fpu_has_exception(status) {
        backend.report_fpu_exception(status);
    }
    let control = backend.fpu_control();
    backend.apply_fpu_control(fpu_entry_control(control));
    if state.rendering && !state.render_cb {
        return I3DERR_GENERIC;
    }
    state.rendering = true;
    let prev = scene.render_number;
    scene.render_number = scene.render_number.wrapping_add(1);
    state.render_number_minus1 = prev;
    state.render_number = scene.render_number;
    if !scene.has_camera {
        backend.clear_no_camera();
        if !state.render_cb {
            state.rendering = false;
        }
        return I3DERR_GENERIC;
    }
    let desired = scene_desired_rect(&scene.viewport);
    if !render_rect_equal(&desired, &state.viewport) {
        state.viewport = desired;
        if !backend.set_viewport(&state.viewport) {
            if !state.render_cb {
                state.rendering = false;
            }
            return I3DERR_NOTINITIALIZED;
        }
    }
    // Entry timestamp only outside the scene callback (render.cpp:322).
    // Both timestamps are intentionally discarded: they feed device-side
    // area/flare passes with no host carrier (disclosed, see the trait).
    let _frame_time = if state.render_cb { 0 } else { backend.now() };
    backend.update_wmatrix_proc();
    backend.update_wbound_proc();
    backend.radix_sort();
    let _flare_time = backend.lensflare_now();
    backend.visual_proc();
    backend.next_fframe_proc();
    backend.build_all_vbuffers();
    if !state.render_cb {
        state.rendering = false;
    }
    I3D_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ParticleLog {
        events: Vec<&'static str>,
    }
    impl ParticleBackend for ParticleLog {
        fn init_particle(&mut self) {
            self.events.push("init");
        }
        fn close_particle(&mut self) {
            self.events.push("close");
        }
    }

    #[test]
    fn init_closes_first_then_inits_and_returns_ok() {
        let mut state = ParticleState { live: true };
        let mut backend = ParticleLog { events: Vec::new() };
        assert_eq!(init_render(&mut state, &mut backend), I3D_OK);
        assert_eq!(backend.events, vec!["close", "init"]);
        assert!(state.live);
    }

    #[test]
    fn close_is_quiet_when_nothing_is_live() {
        let mut state = ParticleState { live: false };
        let mut backend = ParticleLog { events: Vec::new() };
        close_render(&mut state, &mut backend);
        assert!(backend.events.is_empty());
        assert!(!state.live);
        close_render(&mut state, &mut backend);
        assert!(backend.events.is_empty());
    }

    struct VisualLog {
        owner: Vec<u32>,
        particles: Vec<u32>,
        adds: usize,
        known: bool,
        now_calls: usize,
        registers: Vec<u32>,
        mets: usize,
    }
    impl VisualProcBackend for VisualLog {
        fn set_owner_render_number(&mut self, number: u32) {
            self.owner.push(number);
        }
        fn queue_particle(&mut self, sort_value: u32) {
            self.particles.push(sort_value);
        }
        fn add_fgroups(&mut self) {
            self.adds += 1;
        }
        fn lensflare_known(&mut self) -> bool {
            self.known
        }
        fn now(&mut self) -> u32 {
            self.now_calls += 1;
            1234
        }
        fn register_lensflare(&mut self, now: u32) {
            self.registers.push(now);
        }
        fn mark_lensflare_met(&mut self) {
            self.mets += 1;
        }
    }

    fn log() -> VisualLog {
        VisualLog {
            owner: Vec::new(),
            particles: Vec::new(),
            adds: 0,
            known: false,
            now_calls: 0,
            registers: Vec::new(),
            mets: 0,
        }
    }

    fn state() -> VisualProcState {
        VisualProcState {
            render_number: 7,
            problem_clip_always: false,
            use_projectors: true,
        }
    }

    #[test]
    fn visual_proc_sets_owner_and_clip_before_switch() {
        let mut visual = VisualProcVisual {
            owner_present: true,
            kind: VisualProcKind::Other,
            frm_flags: 0,
            part_flags: 0,
            sort_group: 0,
            sort_dist: 0.0,
        };
        let mut backend = log();
        let proc_state = VisualProcState {
            render_number: 9,
            problem_clip_always: true,
            use_projectors: true,
        };
        visual_proc(&mut visual, &proc_state, &mut backend);
        assert_eq!(backend.owner, vec![9]);
        assert_eq!(visual.frm_flags & FRMFLAGS_CLIP, FRMFLAGS_CLIP);
        assert_eq!(backend.adds, 1);
    }

    #[test]
    fn visual_proc_queues_renderer_particles_with_sort_value() {
        let mut visual = VisualProcVisual {
            owner_present: false,
            kind: VisualProcKind::PartBase,
            frm_flags: 0,
            part_flags: PARTF_RENDERER | 0x2,
            sort_group: 3,
            sort_dist: 2.5,
        };
        let mut backend = log();
        visual_proc(&mut visual, &state(), &mut backend);
        // sort_group 3 << 16 = 196608; FloatToInt(2.5) = 2 (even).
        assert_eq!(backend.particles, vec![196608 | 2]);
        assert_eq!(backend.adds, 0);
        // Without the renderer bits the visual adds directly.
        visual.part_flags = 0;
        visual_proc(&mut visual, &state(), &mut backend);
        assert_eq!(backend.adds, 1);
    }

    #[test]
    fn visual_proc_projector_guard_falls_through_when_enabled() {
        let mut visual = VisualProcVisual {
            owner_present: false,
            kind: VisualProcKind::Projector,
            frm_flags: 0,
            part_flags: 0,
            sort_group: 0,
            sort_dist: 0.0,
        };
        let mut backend = log();
        let off = VisualProcState {
            render_number: 7,
            problem_clip_always: false,
            use_projectors: false,
        };
        visual_proc(&mut visual, &off, &mut backend);
        assert_eq!(backend.adds, 0);
        visual_proc(&mut visual, &state(), &mut backend);
        assert_eq!(backend.adds, 1);
    }

    #[test]
    fn visual_proc_registers_new_lensflare_once_then_marks() {
        let mut visual = VisualProcVisual {
            owner_present: false,
            kind: VisualProcKind::LensFlare,
            frm_flags: 0,
            part_flags: 0,
            sort_group: 0,
            sort_dist: 0.0,
        };
        let mut backend = log();
        backend.known = false;
        visual_proc(&mut visual, &state(), &mut backend);
        assert_eq!(backend.registers, vec![1234]);
        assert_eq!(backend.now_calls, 1);
        assert_eq!(backend.mets, 1);
        assert_eq!(backend.adds, 0);
        backend.known = true;
        visual_proc(&mut visual, &state(), &mut backend);
        assert_eq!(backend.registers.len(), 1);
        assert_eq!(backend.now_calls, 1);
        assert_eq!(backend.mets, 2);
    }

    struct NextLog {
        adds: usize,
        resets: usize,
    }
    impl NextFrameBackend for NextLog {
        fn add_fgroups(&mut self) {
            self.adds += 1;
        }
        fn reset_fgroups(&mut self) {
            self.resets += 1;
        }
    }

    fn plain_visual(bound_z: f32) -> FframeNode {
        FframeNode {
            on: true,
            is_visual: true,
            vis_flags: 0,
            world_scale: 2.0,
            bound_pos: [1.0, 2.0, bound_z],
            mesh_dists: None,
            children: Vec::new(),
        }
    }

    #[test]
    fn next_fframe_proc_plain_visual_sets_single_cam_and_quality() {
        let mut frame_state = NextFrameState {
            rs_lod_quality: 0.0,
            cam_pos: [0.0, 0.0, 0.0],
        };
        let mut backend = NextLog { adds: 0, resets: 0 };
        next_fframe_proc(&[plain_visual(10.0)], &mut frame_state, &mut backend);
        // RSLodQuality = 1.0 / 2.0 = 0.5; cam_pos = bound.
        assert_eq!(frame_state.rs_lod_quality, 0.5);
        assert_eq!(frame_state.cam_pos, [1.0, 2.0, 10.0]);
        assert_eq!((backend.adds, backend.resets), (1, 1));
    }

    #[test]
    fn next_fframe_proc_object_levels_use_double_sqrt_unfused() {
        let node = FframeNode {
            on: true,
            is_visual: true,
            vis_flags: VISF_OBJECT,
            world_scale: 1.0,
            bound_pos: [0.0, 0.0, 0.0],
            mesh_dists: Some(vec![16.0, 81.0]),
            children: Vec::new(),
        };
        let mut frame_state = NextFrameState {
            rs_lod_quality: 0.0,
            cam_pos: [0.0, 0.0, 0.0],
        };
        let mut backend = NextLog { adds: 0, resets: 0 };
        // Level 0: sqrt(16)=4, sqrt(4)=2, (2+0)*0.5=1 -> z=1.
        // Level 1: sqrt(81)=9, sqrt(9)=3, (3+2)*0.5=2.5 -> z=2.5.
        next_fframe_proc(&[node], &mut frame_state, &mut backend);
        assert_eq!(frame_state.cam_pos[2], 2.5);
        assert_eq!(frame_state.rs_lod_quality, 1.0);
        assert_eq!((backend.adds, backend.resets), (2, 1));
    }

    #[test]
    fn next_fframe_proc_skips_off_frames_and_texture_lod_single_level() {
        let off = FframeNode {
            on: false,
            is_visual: true,
            vis_flags: 0,
            world_scale: 1.0,
            bound_pos: [9.0, 9.0, 9.0],
            mesh_dists: None,
            children: vec![plain_visual(3.0)],
        };
        let textured = FframeNode {
            on: true,
            is_visual: true,
            vis_flags: VISF_OBJECT | VISF_TEXTURE_LOD,
            world_scale: 1.0,
            bound_pos: [0.0, 0.0, 5.0],
            mesh_dists: Some(vec![16.0, 81.0]),
            children: Vec::new(),
        };
        let mut frame_state = NextFrameState {
            rs_lod_quality: 0.0,
            cam_pos: [0.0, 0.0, 0.0],
        };
        let mut backend = NextLog { adds: 0, resets: 0 };
        next_fframe_proc(&[off, textured], &mut frame_state, &mut backend);
        // Off frame plus its child are skipped; textured object queues one
        // level (dists[0]=16 -> d=2, z=5+1=6).
        assert_eq!(frame_state.cam_pos, [0.0, 0.0, 6.0]);
        assert_eq!((backend.adds, backend.resets), (1, 1));
    }

    struct BuildLog {
        order: Vec<&'static str>,
        adds: usize,
        resets: usize,
    }
    impl NextFrameBackend for BuildLog {
        fn add_fgroups(&mut self) {
            self.adds += 1;
        }
        fn reset_fgroups(&mut self) {
            self.resets += 1;
        }
    }
    impl BuildBackend for BuildLog {
        fn create_sector_frame(&mut self) {
            self.order.push("create");
        }
        fn release_sector_frame(&mut self) {
            self.order.push("release");
        }
        fn clear_fg(&mut self) {
            self.order.push("clear");
        }
        fn set_scene(&mut self) {
            self.order.push("scene");
        }
        fn set_sector_temp(&mut self) {
            self.order.push("sector");
        }
        fn set_camera(&mut self) {
            self.order.push("camera");
        }
    }

    #[test]
    fn build_all_vbuffers_orders_create_queue_release_clear() {
        let scene = BuildScene {
            primary: vec![plain_visual(1.0)],
            omni: Vec::new(),
            backdrop: vec![plain_visual(2.0)],
            near: Vec::new(),
        };
        let mut frame_state = NextFrameState {
            rs_lod_quality: 0.0,
            cam_pos: [0.0, 0.0, 0.0],
        };
        let mut backend = BuildLog {
            order: Vec::new(),
            adds: 0,
            resets: 0,
        };
        build_all_vbuffers(&scene, &mut frame_state, &mut backend);
        assert_eq!(
            backend.order,
            vec!["create", "clear", "scene", "sector", "camera", "release", "clear"]
        );
        assert_eq!((backend.adds, backend.resets), (2, 2));
        // Last queued child is the backdrop one (z = 2.0).
        assert_eq!(frame_state.cam_pos[2], 2.0);
    }
}

/// PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
/// LS3DF RVA 0x6fdc0 `SortModesCallback`
/// (mafia-re ls3df/IGraph/IGraph.cpp:2104-2118, pinned at ab67b13).
/// `qsort` comparison for `D3DDISPLAYMODE`: Width ascending, then Height
/// ascending, then Format descending compared as signed `int`. Returns -1/0/1
/// exactly as the original (the equal-Format tail returns
/// `(int)Format < (int)Format` as 0/1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub format: u32,
}

pub fn compare_display_modes(a: &DisplayMode, b: &DisplayMode) -> i32 {
    if a.width < b.width {
        return -1;
    }
    if a.width > b.width {
        return 1;
    }
    if a.height < b.height {
        return -1;
    }
    if a.height > b.height {
        return 1;
    }
    if (a.format as i32) > (b.format as i32) {
        return -1;
    }
    i32::from((a.format as i32) < (b.format as i32))
}

/// Sorts display modes with the original comparison (Width/Height ascending,
/// Format descending as signed int). Order among fully equal modes is stable
/// here; the original `qsort` leaves it unspecified (documented divergence,
/// no observable contract).
pub fn sort_display_modes(modes: &mut [DisplayMode]) {
    modes.sort_by(|a, b| {
        let c = compare_display_modes(a, b);
        if c < 0 {
            std::cmp::Ordering::Less
        } else if c > 0 {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
}

#[cfg(test)]
mod display_mode_tests {
    use super::*;
    #[test]
    fn comparator_orders_width_height_then_format_desc_signed() {
        let lo = DisplayMode {
            width: 640,
            height: 480,
            format: 22,
        };
        let hi = DisplayMode {
            width: 800,
            height: 600,
            format: 22,
        };
        assert_eq!(compare_display_modes(&lo, &hi), -1);
        assert_eq!(compare_display_modes(&hi, &lo), 1);
        let a = DisplayMode {
            width: 800,
            height: 480,
            format: 22,
        };
        let b = DisplayMode {
            width: 800,
            height: 600,
            format: 22,
        };
        assert_eq!(compare_display_modes(&a, &b), -1);
        let fa = DisplayMode {
            width: 800,
            height: 600,
            format: 25,
        };
        let fb = DisplayMode {
            width: 800,
            height: 600,
            format: 23,
        };
        assert_eq!(compare_display_modes(&fa, &fb), -1);
        assert_eq!(compare_display_modes(&fb, &fa), 1);
        assert_eq!(compare_display_modes(&fa, &fa), 0);
    }
    #[test]
    fn sort_modes_callback_orders_mixed_list() {
        let mut modes = vec![
            DisplayMode {
                width: 800,
                height: 600,
                format: 23,
            },
            DisplayMode {
                width: 640,
                height: 480,
                format: 22,
            },
            DisplayMode {
                width: 800,
                height: 600,
                format: 25,
            },
            DisplayMode {
                width: 800,
                height: 480,
                format: 22,
            },
        ];
        sort_display_modes(&mut modes);
        assert_eq!(
            modes,
            vec![
                DisplayMode {
                    width: 640,
                    height: 480,
                    format: 22,
                },
                DisplayMode {
                    width: 800,
                    height: 480,
                    format: 22,
                },
                DisplayMode {
                    width: 800,
                    height: 600,
                    format: 25,
                },
                DisplayMode {
                    width: 800,
                    height: 600,
                    format: 23,
                },
            ]
        );
    }
}

#[cfg(test)]
mod render_scene_tests {
    use super::*;

    struct RenderLog {
        events: Vec<&'static str>,
        status: u16,
        control: u16,
        reported: Vec<u16>,
        applied: Vec<u16>,
        viewport_result: bool,
        viewports: Vec<RenderRect>,
        clears: usize,
        nows: usize,
        flare_nows: usize,
        now_value: u32,
    }

    impl RenderBackend for RenderLog {
        fn fpu_status(&mut self) -> u16 {
            self.events.push("fpu_status");
            self.status
        }
        fn fpu_control(&mut self) -> u16 {
            self.events.push("fpu_control");
            self.control
        }
        fn report_fpu_exception(&mut self, status: u16) {
            self.events.push("report_fpu");
            self.reported.push(status);
        }
        fn apply_fpu_control(&mut self, masked: u16) {
            self.events.push("apply_fpu");
            self.applied.push(masked);
        }
        fn set_viewport(&mut self, rect: &RenderRect) -> bool {
            self.events.push("set_viewport");
            self.viewports.push(*rect);
            self.viewport_result
        }
        fn clear_no_camera(&mut self) {
            self.events.push("clear_no_camera");
            self.clears += 1;
        }
        fn now(&mut self) -> u32 {
            self.events.push("now");
            self.nows += 1;
            self.now_value
        }
        fn lensflare_now(&mut self) -> u32 {
            self.events.push("lensflare_now");
            self.flare_nows += 1;
            self.now_value
        }
        fn update_wmatrix_proc(&mut self) {
            self.events.push("update_wmatrix_proc");
        }
        fn update_wbound_proc(&mut self) {
            self.events.push("update_wbound_proc");
        }
        fn radix_sort(&mut self) {
            self.events.push("radix_sort");
        }
        fn visual_proc(&mut self) {
            self.events.push("visual_proc");
        }
        fn next_fframe_proc(&mut self) {
            self.events.push("next_fframe_proc");
        }
        fn build_all_vbuffers(&mut self) {
            self.events.push("build_all_vbuffers");
        }
    }

    fn log() -> RenderLog {
        RenderLog {
            events: Vec::new(),
            status: 0,
            control: 0x1234,
            reported: Vec::new(),
            applied: Vec::new(),
            viewport_result: true,
            viewports: Vec::new(),
            clears: 0,
            nows: 0,
            flare_nows: 0,
            now_value: 777,
        }
    }

    fn ready_state() -> RenderDriverState {
        RenderDriverState {
            is_init: true,
            rendering: false,
            render_cb: false,
            viewport: RenderRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            render_number: 41,
            render_number_minus1: 40,
        }
    }

    fn scene_hit() -> RenderSceneInput {
        RenderSceneInput {
            viewport: SceneViewport {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            has_camera: true,
            render_number: 41,
        }
    }

    #[test]
    fn success_runs_stages_in_order_with_single_time_read() {
        let mut scene = scene_hit();
        let mut state = ready_state();
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3D_OK);
        assert_eq!(
            backend.events,
            vec![
                "fpu_status",
                "fpu_control",
                "apply_fpu",
                "now",
                "update_wmatrix_proc",
                "update_wbound_proc",
                "radix_sort",
                "lensflare_now",
                "visual_proc",
                "next_fframe_proc",
                "build_all_vbuffers",
            ]
        );
        assert_eq!(backend.nows, 1);
        assert_eq!(backend.flare_nows, 1);
        assert_eq!(backend.now_value, 777);
        assert_eq!(scene.render_number, 42);
        assert_eq!(state.render_number, 42);
        assert_eq!(state.render_number_minus1, 41);
        assert!(!state.rendering);
        // 0x1234 & 0xf0ff = 0x1034.
        assert_eq!(backend.applied, vec![0x1034]);
    }

    #[test]
    fn null_scene_returns_generic_without_backend_calls() {
        let mut state = ready_state();
        let mut backend = log();
        let result = render_scene(None, &mut state, &mut backend);
        assert_eq!(result, I3DERR_GENERIC);
        assert!(backend.events.is_empty());
        assert!(!state.rendering);
    }

    #[test]
    fn uninit_returns_not_initialized_without_backend_calls() {
        let mut scene = scene_hit();
        let mut state = RenderDriverState {
            is_init: false,
            ..ready_state()
        };
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3DERR_NOTINITIALIZED);
        assert!(backend.events.is_empty());
        assert_eq!(scene.render_number, 41);
    }

    #[test]
    fn reentrant_returns_generic_after_fpu_only() {
        let mut scene = scene_hit();
        let mut state = RenderDriverState {
            rendering: true,
            ..ready_state()
        };
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3DERR_GENERIC);
        assert_eq!(
            backend.events,
            vec!["fpu_status", "fpu_control", "apply_fpu"]
        );
        assert_eq!(backend.nows, 0);
        assert_eq!(scene.render_number, 41);
        assert!(state.rendering);
    }

    #[test]
    fn callback_reentry_runs_stages_and_keeps_rendering() {
        let mut scene = scene_hit();
        let mut state = RenderDriverState {
            rendering: true,
            render_cb: true,
            ..ready_state()
        };
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3D_OK);
        assert!(backend.events.contains(&"build_all_vbuffers"));
        // Entry timestamp is callback-gated (render.cpp:322); the flare loop
        // runs regardless, so its hook still fires once.
        assert_eq!(backend.nows, 0);
        assert_eq!(backend.flare_nows, 1);
        assert!(state.rendering);
    }

    #[test]
    fn missing_camera_clears_and_returns_generic() {
        let mut scene = RenderSceneInput {
            has_camera: false,
            ..scene_hit()
        };
        let mut state = ready_state();
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3DERR_GENERIC);
        assert_eq!(
            backend.events,
            vec!["fpu_status", "fpu_control", "apply_fpu", "clear_no_camera"]
        );
        assert_eq!(backend.clears, 1);
        assert_eq!(backend.nows, 0);
        assert_eq!(scene.render_number, 42);
        assert!(!state.rendering);
    }

    #[test]
    fn rect_miss_copies_then_sets_viewport() {
        let mut scene = RenderSceneInput {
            viewport: SceneViewport {
                left: 10,
                top: 20,
                right: 110,
                bottom: 70,
            },
            has_camera: true,
            render_number: 7,
        };
        let mut state = ready_state();
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3D_OK);
        // right = 110 - 10 = 100, bottom = 70 - 20 = 50.
        let desired = RenderRect {
            left: 10,
            top: 20,
            right: 100,
            bottom: 50,
        };
        assert_eq!(scene_desired_rect(&scene.viewport), desired);
        assert_eq!(state.viewport, desired);
        assert_eq!(backend.viewports, vec![desired]);
        assert_eq!(
            backend.events,
            vec![
                "fpu_status",
                "fpu_control",
                "apply_fpu",
                "set_viewport",
                "now",
                "update_wmatrix_proc",
                "update_wbound_proc",
                "radix_sort",
                "lensflare_now",
                "visual_proc",
                "next_fframe_proc",
                "build_all_vbuffers",
            ]
        );
    }

    #[test]
    fn rect_hit_skips_device_set() {
        let mut scene = scene_hit();
        let mut state = ready_state();
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3D_OK);
        assert!(!backend.events.contains(&"set_viewport"));
        assert!(backend.viewports.is_empty());
    }

    #[test]
    fn rect_miss_failure_returns_not_initialized_with_cache_copied() {
        let mut scene = RenderSceneInput {
            viewport: SceneViewport {
                left: 10,
                top: 20,
                right: 110,
                bottom: 70,
            },
            has_camera: true,
            render_number: 7,
        };
        let mut state = ready_state();
        let mut backend = log();
        backend.viewport_result = false;
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3DERR_NOTINITIALIZED);
        // CopyRect runs before the failing SetViewport.
        assert_eq!(
            state.viewport,
            RenderRect {
                left: 10,
                top: 20,
                right: 100,
                bottom: 50,
            }
        );
        assert_eq!(
            backend.events,
            vec!["fpu_status", "fpu_control", "apply_fpu", "set_viewport"]
        );
        assert_eq!(backend.nows, 0);
        assert!(!state.rendering);
    }

    #[test]
    fn fpu_exception_reported_and_control_masked() {
        let mut scene = scene_hit();
        let mut state = ready_state();
        let mut backend = log();
        backend.status = 0x81;
        backend.control = 0x1234;
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3D_OK);
        assert_eq!(backend.reported, vec![0x81]);
        assert_eq!(backend.applied, vec![0x1034]);
        assert_eq!(
            backend.events[0..4].to_vec(),
            vec!["fpu_status", "report_fpu", "fpu_control", "apply_fpu"]
        );
        assert!(fpu_has_exception(0x80));
        assert!(!fpu_has_exception(0x7f));
        assert_eq!(fpu_entry_control(0x1234), 0x1034);
    }

    #[test]
    fn render_number_wraps() {
        let mut scene = RenderSceneInput {
            viewport: SceneViewport {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            has_camera: true,
            render_number: u32::MAX,
        };
        let mut state = ready_state();
        let mut backend = log();
        let result = render_scene(Some(&mut scene), &mut state, &mut backend);
        assert_eq!(result, I3D_OK);
        assert_eq!(scene.render_number, 0);
        assert_eq!(state.render_number, 0);
        assert_eq!(state.render_number_minus1, u32::MAX);
    }
}
